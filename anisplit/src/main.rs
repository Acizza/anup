mod err;

use anime::local::{EpisodeMatcher, Episodes};
use anime::remote::anilist::AniList;
use anime::remote::{RemoteService, SeriesInfo};
use err::{Error, Result};
use gumdrop::Options;
use snafu::{ensure, OptionExt, ResultExt};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::io;
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

    let all_episodes = Episodes::parse_all(&path, &matcher)?;

    match all_episodes.len() {
        len if len > 1 => {
            println!("found multiple titles in directory.. these will be moved instead\nrerun the tool afterwards to split up merged seasons\n");

            let data = SeriesData {
                name_format,
                link_method: LinkMethod::Move,
                path,
                out_dir,
            };

            split_multiple_titles(&args, data, all_episodes, remote)
        }
        1 => {
            let (_, episodes) = all_episodes.into_iter().next().unwrap();

            let series = {
                let title = parse_path_title(&path)?;
                find_series_info(&args, title, &remote)?
            };

            println!("processing merged seasons of {}\n", series.title.preferred);

            let data = SeriesData {
                name_format,
                link_method: LinkMethod::from_args(&args),
                path,
                out_dir,
            };

            format_sequels(data, series, episodes, remote)
        }
        _ => Ok(()),
    }
}

fn split_multiple_titles(
    args: &CmdOptions,
    data: SeriesData,
    all_episodes: HashMap<String, Episodes>,
    remote: AniList,
) -> Result<()> {
    let original_title = parse_path_title(&data.path)?;

    for (title, episodes) in all_episodes {
        if title == original_title {
            continue;
        }

        println!("moving {}", title);

        let info = find_series_info(args, title, &remote)?;
        let actions = PendingActions::generate(&data, &info, &episodes, 0)?;

        if !actions.confirm_proceed()? {
            continue;
        }

        actions.execute()?;
    }

    Ok(())
}

fn format_sequels(
    data: SeriesData,
    mut info: SeriesInfo,
    episodes: Episodes,
    remote: AniList,
) -> Result<()> {
    let mut episode_offset = 0;
    let mut total_actions = 0;

    while let Some(sequel) = info.sequel {
        info = remote.search_info_by_id(sequel)?;
        episode_offset += info.episodes;

        println!("looking for {}", info.title.preferred);

        let actions = match PendingActions::generate(&data, &info, &episodes, episode_offset) {
            Ok(actions) => actions,
            Err(err @ Error::NoEpisodes) => {
                println!("| {}", err);
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        if !actions.confirm_proceed()? {
            continue;
        }

        total_actions += actions.execute()?;

        // We don't need to sleep if there isn't another sequel
        if info.sequel.is_none() {
            break;
        }

        thread::sleep(Duration::from_millis(500));
    }

    println!("\n{} actions performed", total_actions);
    Ok(())
}

struct SeriesData {
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

#[derive(Copy, Clone)]
enum LinkMethod {
    Symlink,
    Hardlink,
    Move,
}

impl LinkMethod {
    fn from_args(args: &CmdOptions) -> Self {
        if args.symlink {
            Self::Symlink
        } else if args.hardlink {
            Self::Hardlink
        } else if args.move_files {
            Self::Move
        } else {
            Self::default()
        }
    }

    fn execute<P>(self, from: P, to: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let from = from.as_ref();
        let to = to.as_ref();

        let result = match self {
            Self::Symlink => symlink(from, to),
            Self::Hardlink => fs::hard_link(from, to),
            Self::Move => fs::rename(from, to),
        };

        result.context(err::LinkIO { from, to })
    }

    fn plural_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlinks",
            Self::Hardlink => "hardlinks",
            Self::Move => "moves",
        }
    }
}

impl Default for LinkMethod {
    fn default() -> LinkMethod {
        LinkMethod::Symlink
    }
}

struct FormatAction {
    from: PathBuf,
    to: PathBuf,
}

impl FormatAction {
    #[inline(always)]
    fn new<S, O>(from: S, to: O) -> Self
    where
        S: Into<PathBuf>,
        O: Into<PathBuf>,
    {
        Self {
            from: from.into(),
            to: to.into(),
        }
    }
}

struct PendingActions {
    actions: Vec<FormatAction>,
    out_dir: PathBuf,
    method: LinkMethod,
}

impl PendingActions {
    fn generate(
        data: &SeriesData,
        info: &SeriesInfo,
        episodes: &Episodes,
        episode_offset: u32,
    ) -> Result<Self> {
        let out_dir = data.out_dir.join(&info.title.preferred);
        let mut actions = Vec::new();
        let mut has_any_episodes = false;

        for real_ep_num in (1 + episode_offset)..=(episode_offset + info.episodes) {
            let original_filename = match episodes.get(&real_ep_num) {
                Some(filename) => {
                    has_any_episodes = true;
                    filename
                }
                None => continue,
            };

            let episode_path = data.path.join(original_filename);

            let new_filename = data
                .name_format
                .process(&info.title.preferred, real_ep_num - episode_offset);

            let new_path = out_dir.join(new_filename);

            actions.push(FormatAction::new(episode_path, new_path));
        }

        if !has_any_episodes {
            return Err(Error::NoEpisodes);
        }

        Ok(Self {
            actions,
            out_dir,
            method: data.link_method,
        })
    }

    fn confirm_proceed(&self) -> Result<bool> {
        if self.actions.is_empty() {
            println!("| no actions to be performed");
            return Ok(true);
        }

        println!(
            "| the following file {} will be made:",
            self.method.plural_str()
        );

        for action in &self.actions {
            println!(
                "{} -> {}",
                action.from.to_string_lossy(),
                action.to.to_string_lossy()
            );
        }

        println!("| is this okay? (Y/n)");

        let answer = {
            let mut buffer = String::new();
            io::stdin().read_line(&mut buffer).context(err::IO)?;
            buffer.trim_end().to_string()
        };

        match answer.as_ref() {
            "n" | "N" => Ok(false),
            _ => Ok(true),
        }
    }

    fn execute(self) -> Result<u32> {
        if self.actions.is_empty() {
            return Ok(0);
        }

        if !self.out_dir.exists() {
            fs::create_dir_all(&self.out_dir).context(err::FileIO {
                path: &self.out_dir,
            })?;
        }

        let mut actions_performed = 0;

        for action in self.actions {
            match self.method.execute(action.from, action.to) {
                Ok(_) => actions_performed += 1,
                Err(Error::LinkIO { source, .. })
                    if source.kind() == io::ErrorKind::AlreadyExists =>
                {
                    actions_performed += 1;
                }
                Err(err) => eprintln!("{}", err),
            }
        }

        Ok(actions_performed)
    }
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
