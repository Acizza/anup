mod err;

use anime::local::detect;
use anime::local::{CategorizedEpisodes, EpisodeParser, SortedEpisodes};
use anime::remote::anilist::AniList;
use anime::remote::{RemoteService, SeriesInfo};
use anime::SeriesKind;
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

const PARSER_TITLE_REP: &str = "{title}";
const PARSER_EPISODE_REP: &str = "{episode}";

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
    let path = args.path.canonicalize().context(err::IO)?;

    let name_format = match &args.name_format {
        Some(format) => NameFormat::new(format)?,
        None => NameFormat::new(format!("{} - {}.mkv", PARSER_TITLE_REP, PARSER_EPISODE_REP))?,
    };

    let matcher = match &args.matcher {
        Some(pattern) => {
            EpisodeParser::custom_with_replacements(pattern, PARSER_TITLE_REP, PARSER_EPISODE_REP)?
        }
        None => EpisodeParser::default(),
    };

    let out_dir = match &args.out_dir {
        Some(out_dir) => PathBuf::from(out_dir),
        None => path.parent().context(err::NoDirParent)?.into(),
    };

    let all_episodes = CategorizedEpisodes::parse_all(&path, &matcher)?;

    match all_episodes.len() {
        len if len > 1 => {
            println!("found multiple titles in directory.. these will be moved instead\nrerun the tool afterwards to split up merged seasons / episode categories\n");

            let data = SeriesData {
                name_format,
                link_method: LinkMethod::Move,
                path,
                out_dir,
            };

            split_multiple_titles(data, all_episodes)
        }
        1 => {
            let remote = AniList::Unauthenticated;
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

            format_all_series(data, series, episodes, remote)
        }
        _ => Ok(()),
    }
}

fn split_multiple_titles(
    data: SeriesData,
    all_episodes: HashMap<String, CategorizedEpisodes>,
) -> Result<()> {
    let original_title = parse_path_title(&data.path)?;

    for (title, episodes) in all_episodes {
        if title == original_title {
            continue;
        }

        let out_dir = data.out_dir.join(&title);

        println!("moving {}", title);

        let actions = episodes
            .take()
            .into_iter()
            .flat_map(|(_, eps)| eps.take().into_iter())
            .map(|ep| {
                let ep_path = data.path.join(&ep.filename);
                let out_path = out_dir.join(ep.filename);
                FormatAction::new(ep_path, out_path)
            })
            .collect();

        let actions = PendingActions {
            actions,
            out_dir,
            method: data.link_method,
        };

        if !actions.confirm_proceed()? {
            continue;
        }

        actions.execute()?;
    }

    Ok(())
}

fn format_all_series(
    data: SeriesData,
    info: SeriesInfo,
    mut episodes: CategorizedEpisodes,
    remote: AniList,
) -> Result<()> {
    let mut total_actions = 0;

    // Split up merged seasons first
    if let Some(season_eps) = episodes.remove(&SeriesKind::Season) {
        total_actions += format_series_sequels(&data, &info, &season_eps, &remote)?;
    }

    // Now we should split episode categories
    for (cat, cat_eps) in episodes.iter() {
        let cat_str = match cat {
            SeriesKind::Season => unreachable!(),
            SeriesKind::Special => "specials",
            SeriesKind::OVA => "OVAs",
            SeriesKind::ONA => "ONAs",
            SeriesKind::Movie => "movies",
            SeriesKind::Music => "music",
        };

        println!("spltting series {}..", cat_str);

        let cat_info = match info.sequel_by_kind(*cat) {
            Some(sequel) => remote.search_info_by_id(sequel.id)?,
            None => continue,
        };

        let actions = match PendingActions::generate(&data, &cat_info, &cat_eps, 0) {
            Ok(actions) => actions,
            Err(err @ Error::NoEpisodes) => {
                println!("| {}", err);
                return Ok(());
            }
            Err(err) => return Err(err),
        };

        if actions.confirm_proceed()? {
            total_actions += actions.execute()?;
        }

        thread::sleep(Duration::from_millis(500));
    }

    println!("\n{} actions performed", total_actions);
    Ok(())
}

fn format_series_sequels(
    data: &SeriesData,
    initial_info: &SeriesInfo,
    episodes: &SortedEpisodes,
    remote: &AniList,
) -> Result<u32> {
    let mut episode_offset = 0;
    let mut total_actions = 0;

    let mut info = Cow::Borrowed(initial_info);

    while let Some(sequel) = info.direct_sequel() {
        info = remote.search_info_by_id(sequel.id)?.into();
        episode_offset += info.episodes;

        println!("looking for {}", info.title.preferred);

        let actions = match PendingActions::generate(data, &info, episodes, episode_offset) {
            Ok(actions) => actions,
            Err(err @ Error::NoEpisodes) => {
                println!("| {}", err);
                return Ok(total_actions);
            }
            Err(err) => return Err(err),
        };

        if !actions.confirm_proceed()? {
            continue;
        }

        total_actions += actions.execute()?;

        // We don't need to sleep if there isn't another sequel
        if info.sequels.is_empty() {
            break;
        }

        thread::sleep(Duration::from_millis(500));
    }

    Ok(total_actions)
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
            format.contains(PARSER_TITLE_REP),
            err::MissingFormatGroup { group: "title" }
        );

        ensure!(
            format.contains(PARSER_EPISODE_REP),
            err::MissingFormatGroup { group: "episode" }
        );

        Ok(NameFormat(format))
    }

    fn process<S>(&self, name: S, episode: u32) -> String
    where
        S: AsRef<str>,
    {
        self.0
            .replace(PARSER_TITLE_REP, name.as_ref())
            .replace(PARSER_EPISODE_REP, &format!("{:02}", episode))
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
        episodes: &SortedEpisodes,
        episode_offset: u32,
    ) -> Result<Self> {
        let out_dir = data.out_dir.join(&info.title.preferred);
        let mut actions = Vec::new();
        let mut has_any_episodes = false;

        for real_ep_num in (1 + episode_offset)..=(episode_offset + info.episodes) {
            let episode = match episodes.find(real_ep_num) {
                Some(episode) => {
                    has_any_episodes = true;
                    episode
                }
                None => continue,
            };

            let episode_path = data.path.join(&episode.filename);

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

    let title = detect::dir::parse_title(path).context(err::FolderTitleParse)?;
    Ok(title)
}

fn find_series_info<S>(args: &CmdOptions, title: S, remote: &AniList) -> Result<SeriesInfo>
where
    S: AsRef<str>,
{
    const MIN_CONFIDENCE: f32 = 0.85;

    match args.series_id {
        Some(id) => {
            let info = remote.search_info_by_id(id)?;
            Ok(info)
        }
        None => {
            let title = title.as_ref();
            let results = remote
                .search_info_by_name(title)?
                .into_iter()
                .map(Cow::Owned);

            SeriesInfo::closest_match(title, MIN_CONFIDENCE, results)
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
        let result = NameFormat::new(format!("missing_title - {}.mkv", PARSER_EPISODE_REP));

        match result {
            Err(err::Error::MissingFormatGroup { group }) if group == "title" => (),
            Ok(_) => panic!("expected missing title group error"),
            Err(err) => panic!("expected missing title group error, got: {:?}", err),
        }
    }

    #[test]
    fn name_format_detect_no_episode_group() {
        let result = NameFormat::new(format!("{} - missing_episode.mkv", PARSER_TITLE_REP));

        match result {
            Err(err::Error::MissingFormatGroup { group }) if group == "episode" => (),
            Ok(_) => panic!("expected missing episode group error"),
            Err(err) => panic!("expected missing episode group error, got: {:?}", err),
        }
    }
}
