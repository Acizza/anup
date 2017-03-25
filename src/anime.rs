extern crate regex;

use ::std::collections::HashMap;
use ::std::fs;
use ::std::path::Path;
use self::regex::Regex;

#[derive(Debug)]
pub struct Anime {
    pub name: String,
    pub episode_paths: HashMap<u32, String>,
}

impl Anime {
    pub fn new(path: &Path) -> Anime {
        // TODO: Replace with custom solution (?)
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(?:\[.+?\](?:\s+|_+)?)?(?P<name>.+?)(?:\s+|_+)-(?:\s+|_+)(?P<episode>\d+)").unwrap();
        }

        let mut anime_name = String::new();
        let mut episodes = HashMap::new();

        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();

            let caps = RE.captures(name.to_str().unwrap()).unwrap();
            anime_name = caps["name"].to_string();

            episodes.insert(caps["episode"].parse().unwrap(), entry.path().to_str().unwrap().to_string());
        }

        Anime {
            name: anime_name,
            episode_paths: episodes,
        }
    }
}