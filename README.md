# anup

[![dependency status](https://deps.rs/repo/github/acizza/anup/status.svg)](https://deps.rs/repo/github/acizza/anup)

This is a TUI / CLI application to play and manage anime with [AniList](https://anilist.co) for Linux systems.

Note that only local files are supported.

Current features include:
* Automatic series detection
* Easy playing of unwatched episodes
* TUI interface to view, play, and modify all series added to the program
* Offline mode
* Automatic series status handling (watching, rewatching, completed, etc)
* Automatic series start / end date handling

Also included is a tool called `anisplit`. Its purpose is to split up a series that contains multiple seasons merged into one.
For more information about this tool, please go [here](anisplit/).

# Building

The only dependencies this project has is a relatively recent stable version of Rust, pkg-config, and OpenSSL (the latter two most likely already being present).
If your distribution does not provide a recent version of Rust, you can obtain the latest version [here](https://rustup.rs/).

1. Navigate to the project directory
2. Run `cargo build --release`
3. Wait for compilation to complete

You will find the `anup` and `anisplit` binaries in the `target/release/` folder. None of the other files there need to be kept.

# Usage

By default, the program will look for anime in `~/anime/` and play episodes with `mpv`. To change these, run the program at least once to generate the config file and change the `series_dir` and `player` fields in `~/.config/anup/config.toml`, respectively.

## Logging in to AniList

Before you can add a series, you will need to authenticate the program to make changes to your anime list. To do this, visit the URL printed in the TUI (or [here](https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token)) and follow the instructions to obtain an access token. Once you have a token, you will need to paste it in either with the `token` command in the TUI, or with the `-t` CLI flag.

Note that the token will be saved to `~/.config/anup/token.toml` and base64 encoded. You can disable this token at any time from the `Apps` section of your AniList account settings.

## Adding a series

To add a series to the program, simply launch the program with a keyword similar to the name of the folder the series is in.
For example, if you have a series in a folder named `[Group] Example Name [1080p]`, a keyword of `example` or `name` will match it. The AniList series that best matches the detected series title of the episode files inside the detected folder will then be added to the program's series list.

If you would prefer to use a keyword that isn't similar to a folder name, or the automatic detection selects the wrong one, you can use the `-p` flag to manually specify the (absolute) path to the series.

## TODO