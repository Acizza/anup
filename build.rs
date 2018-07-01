#[cfg(linux)]
mod linux {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    const LAUNCH_SCRIPT: &str = "anup.sh";

    pub fn run() {
        let profile = env::var("PROFILE").unwrap();

        let mut out_path = PathBuf::from("target");
        out_path.push(profile);
        out_path.push(LAUNCH_SCRIPT);

        fs::copy(LAUNCH_SCRIPT, out_path).unwrap();
    }
}

fn main() {
    #[cfg(linux)]
    linux::run();
}
