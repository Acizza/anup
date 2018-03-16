#[cfg(unix)]
mod unix {
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    const LAUNCH_SCRIPT: &str = "anitrack.sh";

    pub fn run() {
        move_to_output(LAUNCH_SCRIPT);
    }

    fn move_to_output(name: &str) {
        let profile = env::var("PROFILE").unwrap();

        match profile.as_ref() {
            "debug" | "release" => {
                let mut out_path = PathBuf::from("target");
                out_path.push(profile);
                out_path.push(name);

                fs::copy(name, out_path).unwrap();
            }
            _ => (),
        }
    }
}

fn main() {
    #[cfg(unix)]
    unix::run();
}
