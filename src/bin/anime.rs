extern crate regex;

use ::std::collections::HashMap;
use ::std::fs;
use ::std::path::Path;
use ::std::process::{Command, ExitStatus};
use self::regex::Regex;

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        ConvertInt(::std::num::ParseIntError);
    }

    errors {
        NoneFound {
            description("anime not found")
            display("anime not found")
        }

        EpisodeNotFound(ep: u32) {
            description("episode not found")
            display("unable to find episode {}", ep)
        }
    }
}

#[derive(Debug)]
pub struct LocalAnime {
    pub name: String,
    pub episode_paths: HashMap<u32, String>,
}

impl LocalAnime {
    pub fn find(path: &Path) -> Result<LocalAnime> {
        // TODO: Replace with custom solution (?)
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?:\[.+?\]\s*)?(?P<name>.+?)\s*-?\s*(?P<episode>\d+)\s*(?:\(|\[|\.)").unwrap();
        }

        let mut anime_name = String::new();
        let mut episodes = HashMap::new();

        for entry in fs::read_dir(path)? {
            let entry = entry?;

            let name = entry
                .file_name()
                .into_string()
                .unwrap()
                .replace('_', " ");

            let caps = match RE.captures(&name) {
                Some(v) => v,
                None => continue,
            };

            anime_name = caps["name"].to_string();
            episodes.insert(caps["episode"].parse()?, entry.path().to_str().unwrap().to_string());
        }

        if episodes.len() == 0 {
            bail!(ErrorKind::NoneFound)
        }

        Ok(LocalAnime {
            name: anime_name,
            episode_paths: episodes,
        })
    }

    pub fn get_episode(&self, ep: u32) -> Result<String> {
        match self.episode_paths.get(&ep) {
            Some(path) => Ok(path.clone()),
            None       => bail!(ErrorKind::EpisodeNotFound(ep)),
        }
    }

    pub fn play_episode(&self, ep: u32) -> Result<ExitStatus> {
        let output = Command::new("/usr/bin/xdg-open")
            .arg(self.get_episode(ep)?)
            .output()?;

        Ok(output.status)
    }
}