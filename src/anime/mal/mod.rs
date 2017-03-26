extern crate rquery;
extern crate hyper;

mod request;

use self::request::RequestType;
use self::rquery::Document;

#[derive(Debug)]
pub struct AnimeInfo {
    pub id:       u32,
    pub name:     String,
    pub episodes: u32,
}

impl AnimeInfo {
    pub fn request(name: &str, username: String, password: String) -> Vec<AnimeInfo> {
        let req = request::execute(RequestType::Search(name.into()), username, password);
        let doc = Document::new_from_xml_stream(req).unwrap();

        let mut entries = Vec::new();

        for entry in doc.select_all("entry").unwrap() {
            entries.push(AnimeInfo {
                id:       entry.select("id").unwrap().text().parse().unwrap(),
                name:     entry.select("title").unwrap().text().clone(),
                episodes: entry.select("episodes").unwrap().text().parse().unwrap(),
            });
        }

        entries
    }
}