# anup

[![Build Status](https://travis-ci.org/Acizza/anup.svg?branch=rewrite)](https://travis-ci.org/Acizza/anup)
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

This project requires the following dependencies:

* A recent stable version of Rust
* SQLite
* pkg-config

Note that pkg-config is most likely already installed. If your distribution does not provide a recent version of Rust, you can obtain the latest version [here](https://rustup.rs/).

Once the dependencies are installed, you can build the project simply by running `cargo build --release` in the project's directory. Once compilation is complete, you will find the `anup` and `anisplit` binaries in the `target/release/` folder. None of the other files in that directory need to be kept.

# Usage

By default, the program will look for anime in `~/anime/` and play episodes with `mpv`. To change these, run the program once to generate the config file and change the `series_dir` and `player` fields in `~/.config/anup/config.toml`, respectively.

## Logging in to AniList

Before you can add and play a series, you will need to authenticate the program to make changes to your anime list. To do this, visit the URL printed in the TUI (or [here](https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token)) and follow the instructions to obtain an access token. Once you have a token, you will need to paste it in either with the `anilist` command in the TUI (enter commands by pressing ":"), or with the `-t` CLI flag.

Note that the token will be saved to `~/.config/anup/token.toml` and base64 encoded. You can disable this token at any time from the `Apps` section of your AniList account settings.

## Adding a Series

When in the TUI, you can add a new series to the program by pressing ":", entering `add <series name>`, and then pressing enter. The program will try to find the folder within the `series_dir` set in your config file that best matches the series name entered, and then try to select the series that best matches a cleaned version of the folder's name from AniList. If the program can not confidently detect the correct series from AniList, it will prompt you to select which one you want.

### Custom Episode Matcher

Sometimes, you may encounter cases when trying to add a series where the program cannot detect its episodes. When that happens, you can specify a custom episode matcher via regex by adding a series like before, but with an additional argument:

`add <series name> matcher="<episode matcher regex>"`

The episode matcher regex pattern **must** contain the magic value `{episode}`, which should be placed where the episode number is within the episode's filename.

#### Example 1:
* Filename: `EP01 - Series Name.mkv`
* Matcher: `EP{episode}`
* Parsed episode: `01`

#### Example 2:
* Filename: `[Group] 02 Series Name.mkv`
* Matcher: `\[.+?\] {episode}`
* Parsed episode: `02`

#### Example 3:
* Filename: `Series Name With Number At End 1 03.mkv`
* Matcher: `.+?{episode}\.mkv`
* Parsed episode: `03`

### Custom Path

If the series you want to add can not be detected from the name you want to use for it, or the series resides outside of the `series_dir` set in your config file, you can manually specify the path to it by using the `add` command with an additional `path` argument:

`add <series name> path="<absolute path to series>"`

### Custom ID

Overriding the ID of a new series is useful for cases where the program will not select your intended series, but is confident enough to pick it for you. This can be done by using the `add` command with an additional `id` argument:

`add <series name> id=<id from AniList>`

You can obtain the ID of a series by going to [AniList](https://anilist.co), going to the page of the series you want, and using the numbers from the resulting URL that are located where `<series id>` appears here:

`https://anilist.co/anime/<series id>/<series name>/`

### Combining Options

You can combine multiple options from the sections above in any order when using the `add` command. For example:

`add toradora id=1 matcher="EP{episode}" path="/media/anime/Cowboy Bebop"`

The above command will add `Cowboy Bebop` to the series list, look for episodes matching `EP{episode}` at the path `/media/anime/Cowboy Bebop`, and list the series in the program as `toradora`.

## TODO