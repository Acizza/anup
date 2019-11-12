mod err;

use anime::local::{EpisodeMap, EpisodeMatcher};
use anime::remote::anilist::AniList;
use anime::remote::{RemoteService, SeriesID, SeriesInfo};
use clap::{clap_app, ArgMatches};
use err::Result;
use snafu::{ensure, OptionExt, ResultExt};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn main() {
    let args = clap_app!(anisplit =>
        (version: env!("CARGO_PKG_VERSION"))
        (author: env!("CARGO_PKG_AUTHORS"))
        (about: "This is a tool to split up an anime series that has multiple \
                 seasons merged together.")
        (@arg path: +takes_value +required "The path pointing to the series to split")
        (@arg out_dir: -o --out +takes_value "The path to create the split seasons in. If this is not specified, the parent directory of the series path will be used")
        (@arg series_id: -i --id +takes_value "The anime series ID. Use if the program doesn't detect the right series automatically")
        (@arg name_format: -f --format +takes_value "The format to rename the files as. Must contain \"{title}\" and \"{episode}\"")
        (@arg matcher: -m --matcher +takes_value "The custom pattern to match episode files with")
    )
    .get_matches();

    if let Err(err) = run(&args) {
        err::display_error(err);
        std::process::exit(1);
    }
}

fn run(args: &ArgMatches) -> Result<()> {
    let remote = AniList::unauthenticated();

    let path = PathBuf::from(args.value_of("path").unwrap())
        .canonicalize()
        .context(err::IO)?;

    let name_format = match args.value_of("name_format") {
        Some(format) => NameFormat::new(format)?,
        None => NameFormat::new("{title} - {episode}.mkv")?,
    };

    let matcher = match args.value_of("matcher") {
        Some(pattern) => {
            let pattern = pattern
                .replace("{title}", "(?P<title>.+)")
                .replace("{episode}", r"(?P<episode>\d+)");
            EpisodeMatcher::from_pattern(pattern)?
        }
        None => EpisodeMatcher::new(),
    };

    let out_dir = match args.value_of("out_dir") {
        Some(out_dir) => PathBuf::from(out_dir),
        None => path.parent().context(err::NoDirParent)?.into(),
    };

    let data = SeriesData {
        episodes: EpisodeMap::parse(&path, &matcher)?,
        name_format,
        path,
        out_dir,
    };

    let series = {
        let title = parse_path_title(&data.path)?;
        find_series_info(args, title, &remote)?
    };

    format_sequels(&data, series, &remote)
}

struct SeriesData {
    episodes: EpisodeMap,
    name_format: NameFormat,
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

        if let Err(err) = symlink(&episode_path, &link_path) {
            eprintln!("failed to create symlink {:?}: {}", link_path, err);
        } else {
            num_links_created += 1;
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

    let title = path
        .file_name()
        .context(err::NoDirName)?
        .to_string_lossy()
        .into_owned();

    Ok(title)
}

fn find_series_info<S>(args: &ArgMatches, title: S, remote: &AniList) -> Result<SeriesInfo>
where
    S: AsRef<str>,
{
    match args.value_of("series_id") {
        Some(id) => {
            let id = id.parse::<SeriesID>().context(err::InvalidSeriesID)?;
            let series = remote.search_info_by_id(id)?;
            Ok(series)
        }
        None => {
            let title = title.as_ref();
            let mut results = remote.search_info_by_name(title)?;
            let best_result = detect::best_matching_info(title, &results)
                .context(err::UnableToDetectSeries { title })?;

            let series = results.swap_remove(best_result);
            Ok(series)
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
