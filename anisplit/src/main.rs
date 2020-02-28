mod err;

use anime::local::{EpisodeMatcher, Episodes};
use anime::remote::anilist::AniList;
use anime::remote::{RemoteService, SeriesInfo};
use err::Result;
use gumdrop::Options;
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Options)]
struct CmdOptions {
    #[options(help = "print help message")]
    help: bool,
    #[options(free, required, help = "the path pointing to the series to split")]
    path: PathBuf,
    #[options(
        help = "the path to create the split seasons in. By default, the parent directory of the series path will be used"
    )]
    out_dir: Option<PathBuf>,
    #[options(
        help = "the anime series ID. Use if the program doesn't detect the right series automatically"
    )]
    series_id: Option<u32>,
    #[options(
        help = "the format to rename the files as. Must contain \"{title}\" and \"{episode}\""
    )]
    name_format: Option<String>,
    #[options(help = "the custom regex pattern to match episode files with")]
    matcher: Option<String>,
    #[options(no_short, help = "link episode files via symlinks")]
    symlink: bool,
    #[options(no_short, help = "link episode files via hardlinks")]
    hardlink: bool,
    #[options(no_short, help = "link episode files via file moves")]
    move_files: bool,
    #[options(no_short, help = "show the changes to files that would be made")]
    preview: bool,
}

fn main() {
    let args = CmdOptions::parse_args_default_or_exit();

    if let Err(err) = run(args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: CmdOptions) -> Result<()> {
    let remote = AniList::unauthenticated();

    let path = args.path.canonicalize().context(err::IO)?;

    let name_format = match &args.name_format {
        Some(format) => NameFormat::new(format)?,
        None => NameFormat::new("{title} - {episode}.mkv")?,
    };

    let matcher = match &args.matcher {
        Some(pattern) => {
            let pattern = pattern
                .replace("{title}", "(?P<title>.+)")
                .replace("{episode}", r"(?P<episode>\d+)");
            EpisodeMatcher::from_pattern(pattern)?
        }
        None => EpisodeMatcher::new(),
    };

    let out_dir = match &args.out_dir {
        Some(out_dir) => PathBuf::from(out_dir),
        None => path.parent().context(err::NoDirParent)?.into(),
    };

    let data = SeriesData {
        episodes: Episodes::parse(&path, &matcher)?,
        name_format,
        link_method: LinkMethod::from_args(&args),
        path,
        out_dir,
    };

    let series = {
        let title = parse_path_title(&data.path)?;
        find_series_info(&args, title, &remote)?
    };

    format_sequels(&data, series, &remote)
}

struct SeriesData {
    episodes: Episodes,
    name_format: NameFormat,
    link_method: LinkMethod,
    path: PathBuf,
    out_dir: PathBuf,
}

struct NameFormat(String);

impl NameFormat {
    fn new<S>(format: S) -> Result<NameFormat>
    where
        S: Into<String>,
    {
        let format = format.into();

        ensure!(
            format.contains("{title}"),
            err::MissingFormatGroup { group: "title" }
        );

        ensure!(
            format.contains("{episode}"),
            err::MissingFormatGroup { group: "episode" }
        );

        Ok(NameFormat(format))
    }

    fn process<S>(&self, name: S, episode: u32) -> String
    where
        S: AsRef<str>,
    {
        self.0
            .replace("{title}", name.as_ref())
            .replace("{episode}", &format!("{:02}", episode))
    }
}

enum LinkMethod {
    Symlink,
    Hardlink,
    Move,
    Preview,
}

impl LinkMethod {
    fn from_args(args: &CmdOptions) -> LinkMethod {
        if args.symlink {
            LinkMethod::Symlink
        } else if args.hardlink {
            LinkMethod::Hardlink
        } else if args.move_files {
            LinkMethod::Move
        } else if args.preview {
            LinkMethod::Preview
        } else {
            LinkMethod::default()
        }
    }

    fn execute<P>(&self, from: P, to: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let from = from.as_ref();
        let to = to.as_ref();

        let result = match self {
            LinkMethod::Symlink => symlink(from, to),
            LinkMethod::Hardlink => fs::hard_link(from, to),
            LinkMethod::Move => fs::rename(from, to),
            LinkMethod::Preview => {
                println!(
                    "preview: {} -> {}\n",
                    from.to_string_lossy(),
                    to.to_string_lossy()
                );
                Ok(())
            }
        };

        result.context(err::LinkIO { from, to })
    }
}

impl Default for LinkMethod {
    fn default() -> LinkMethod {
        LinkMethod::Symlink
    }
}

fn format_sequels(data: &SeriesData, mut info: SeriesInfo, remote: &AniList) -> Result<()> {
    let mut episode_offset = 0;

    while let Some(sequel) = info.sequel {
        info = remote.search_info_by_id(sequel)?;
        episode_offset += info.episodes;

        format_series(&data, &info, episode_offset)?;

        // We don't need to sleep if there isn't another sequel
        if info.sequel.is_none() {
            break;
        }

        thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

fn format_series(data: &SeriesData, info: &SeriesInfo, episode_offset: u32) -> Result<()> {
    let out_dir = data.out_dir.join(&info.title.preferred);
    let mut out_dir_exists = out_dir.exists();

    let mut num_links_created = 0;

    for real_ep_num in (1 + episode_offset)..=(episode_offset + info.episodes) {
        let original_filename = match data.episodes.get(&real_ep_num) {
            Some(filename) => filename,
            None => continue,
        };

        // We only want to create the directory for the season if we have any episodes
        // from it
        if !out_dir_exists {
            fs::create_dir_all(&out_dir).context(err::FileIO { path: &out_dir })?;
            out_dir_exists = true;
        }

        let episode_path = data.path.join(original_filename);

        let new_filename = data
            .name_format
            .process(&info.title.preferred, real_ep_num - episode_offset);

        let link_path = out_dir.join(new_filename);

        match data.link_method.execute(&episode_path, &link_path) {
            Ok(()) => num_links_created += 1,
            Err(err) => eprintln!("{}", err),
        }
    }

    println!(
        "created {} links for {}",
        num_links_created, info.title.preferred
    );

    Ok(())
}

fn parse_path_title<P>(path: P) -> Result<String>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();

    ensure!(path.is_dir(), err::NotADirectory);

    let fname = path.file_name().context(err::NoDirName)?.to_string_lossy();
    let title = detect::dir::parse_title(fname).context(err::FolderTitleParse)?;

    Ok(title)
}

fn find_series_info<S>(args: &CmdOptions, title: S, remote: &AniList) -> Result<SeriesInfo>
where
    S: AsRef<str>,
{
    match args.series_id {
        Some(id) => {
            let info = remote.search_info_by_id(id)?;
            Ok(info)
        }
        None => {
            let title = title.as_ref();
            let results = remote.search_info_by_name(title)?.map(Cow::Owned);

            detect::series_info::closest_match(results, title)
                .map(|(_, info)| info.into_owned())
                .context(err::UnableToDetectSeries { title })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn name_format_detect_no_group() {
        NameFormat::new("useless").unwrap();
    }

    #[test]
    fn name_format_detect_no_title_group() {
        let result = NameFormat::new("missing_title - {episode}.mkv");

        match result {
            Err(err::Error::MissingFormatGroup { group }) if group == "title" => (),
            Ok(_) => panic!("expected missing title group error"),
            Err(err) => panic!("expected missing title group error, got: {:?}", err),
        }
    }

    #[test]
    fn name_format_detect_no_episode_group() {
        let result = NameFormat::new("{title} - missing_episode.mkv");

        match result {
            Err(err::Error::MissingFormatGroup { group }) if group == "episode" => (),
            Ok(_) => panic!("expected missing episode group error"),
            Err(err) => panic!("expected missing episode group error, got: {:?}", err),
        }
    }
}
