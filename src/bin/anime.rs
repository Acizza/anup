extern crate regex;

use ::std::collections::HashMap;
use ::std::fs;
use ::std::path::Path;
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
            static ref RE: Regex = Regex::new(r"(?:\[.+?\](?:\s+|_+)?)?(?P<name>.+?)(?:\s+|_+)-(?:\s+|_+)(?P<episode>\d+)").unwrap();
        }

        let mut anime_name = String::new();
        let mut episodes = HashMap::new();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let name = entry.file_name();

            let caps = RE.captures(name.to_str().unwrap()).ok_or(ErrorKind::NoneFound)?;
            anime_name = caps["name"].to_string();

            episodes.insert(caps["episode"].parse()?, entry.path().to_str().unwrap().to_string());
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
}