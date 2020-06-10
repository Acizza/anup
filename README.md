# anup

[![Build Status](https://travis-ci.org/Acizza/anup.svg?branch=rewrite)](https://travis-ci.org/Acizza/anup)
[![total lines](https://tokei.rs/b1/github/acizza/anup)](https://github.com/acizza/anup)

This is a TUI / CLI application to play and manage anime with [AniList](https://anilist.co) for Linux systems.

Note that only local files are supported.

Current features include:
* Automatic series detection
* Easy playing of unwatched episodes
* TUI interface to view, play, and modify all series added to the program
* Offline mode
* Multi-user support
* Automatic series status handling (watching, rewatching, completed, etc)
* Automatic series start / end date handling

Also included is a tool called `anisplit`. Its purpose is to split up a series that contains multiple seasons merged into one.
For more information about this tool, please go [here](anisplit/).

# Building

This project requires the following dependencies:

* A recent stable version of Rust
* SQLite
* pkg-config
* xdg-open / xdg-utils

Note that pkg-config and xdg-open / xdg-utils are most likely already installed. If your distribution does not provide a recent version of Rust, you can obtain the latest version [here](https://rustup.rs/).

Once the dependencies are installed, you can build the project simply by running `cargo build --release` in the project's directory. Once compilation is complete, you will find the `anup` and `anisplit` binaries in the `target/release/` folder. None of the other files in that directory need to be kept.

# Usage

By default, the program will look for anime in `~/anime/` and play episodes with `mpv`. To change these, run the program once to generate the config file and change the `series_dir` and `player` fields in `~/.config/anup/config.toml`, respectively.

## Adding an Account

Before you can add and play a series, you will need to add an AniList account to the program. To do this, open [this URL](https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token) and follow the instructions to obtain an account access token. Once you have a token, you will need to paste it into the program. To do this, first press `u` to open user management, and then `Tab` to switch to the add user panel. Now press either `Ctrl + Shift + V` **or** `Ctrl + V` (depending on your terminal) to paste the token. Once your token has been pasted in, you can press enter to add your account.

You can repeat this process as needed to add more accounts. Once you are done, you can press `Escape` to return to the main panel.

All accounts are saved to `~/.local/share/anup/users.mpack` and are **not encrypted**. You can disable an account's token at any time by going to your AniList account settings, and navigating to the `Apps` section.

## Adding a Series

You can add a new series to the program by pressing the `a` key. A new panel will be displayed showing inputs for the series name, ID, path, and episode regex that can cycled through with the tab key.

First, you will need to enter a name for the series that is similar to the name of the directory the series is in. For example, the name `kaguya` will match a directory named `[Tags] Kaguya-sama wa Kokurasetai [Tags]`. This is the only input that is required to have a value.

The program will show you the detected path of the series relative to the set `series_dir` in your config, and the number of episodes found at the bottom of the panel in real time.

Once you have finished entering the series name and any other fields, you can press enter to search for and add the series from AniList. The program will try to automatically select the best matching series from AniList for you, but in some cases it can not do so confidently. When that happens, you will be shown a list of found series to choose from. You can scroll through the list with the up and down arrow keys and select the desired series with enter.

The following sections go into detail about each of the optional inputs:

### ID

This input represents the ID of the series from AniList. This is used to override automatic detection should it fail to select the series you wanted.

You can obtain the ID of a series by going to [AniList](https://anilist.co), going to the page of the series you want, and using the numbers from the resulting URL that are located where `<series id>` appears here:

`https://anilist.co/anime/<series id>/<series name>/`

### Path

This input represents the path to the series on disk. This can either be relative to the `series_dir` set in your config, or an absolute path.

### Episode Regex

This input is used to specify a regex pattern to use for detecting episodes. While the default episode detection works with many formats, there may be times where overriding it is necessary.

The regex pattern must contain a magic value named `{episode}`. This is used as a marker for the program to know where the episode numbers appear in the filename and expands internally to the pattern `\d+`.

Here are a few examples of custom patterns:

#### Example 1:
* Filename: `EP01 - Series Name.mkv`
* Regex: `EP{episode}`
* Parsed episode: `01`

#### Example 2:
* Filename: `[Group] 02 Series Name.mkv`
* Regex: `\[.+?\] {episode}`
* Parsed episode: `02`

#### Example 3:
* Filename: `Series Name With Number At End 1 03.mkv`
* Regex: `.+?{episode}\.mkv`
* Parsed episode: `03`

Note that each example above can be detected by the default detector.

## Watching a Series

Once at least one series has been added, you can play the next episode of one by selecting the series with the up and down arrow keys and pressing enter. This will play the episode with the player set in your config file.

Once you start playing an episode, you should see a timer counting down in the `Info` panel. This represents the time needed until the episode will be considered watched. You can change how much of an episode you need to watch by modifying the `percent_watched_to_progress` field in the `[episode]` section of your config file. This field can be set to `0.0` if you do not wish to use this feature.

If you do not see a timer when you start playing an episode, please make sure that the video player / script used to launch your video player does **not** exit immediately after starting to play something. If this can't be fixed, you can use the `progress f` command to manually increment the watched episode count.

Once the timer disappears, the watched episodes of the series will be increased and synced to AniList (unless offline) when you exit your video player.

### Automatic Status & Date Management

The status of the series and its start/end date are also automatically managed by the program. The table below shows the various status transitions that occur, where the `From` column is the status before watching an episode, and the `To` column is the status after watching one:

| From          | To         | Notes         |
| ------------- | ---------- | ------------- |
| Plan To Watch | Watching   | **[1]**       |
| Completed     | Rewatching | **[2]**       |
| Rewatching    | Completed  | **[3][4][6]** |
| Dropped       | Watching   | **[5]**       |
| On Hold       | Watching   |               |
| Watching      | Completed  | **[4][6]**    |

* **[1]** The start date will also be set for the series.
* **[2]** If `reset_dates_on_rewatch` is set to `true` in your config file, the start & end dates will be reset.
* **[3]** The series rewatch count will also be increased.
* **[4]** The end date will also be set for the series if it is not already present.
* **[5]** The number of watched episodes will be reset to 0.
* **[6]** This transition will only happen when all episodes have been watched.

## Modifying an Existing Series

You can modify a series that has already been added to the program by using the `set` command. Each argument is described in the following sections:

### ID

You can modify the series ID by adding `id=<series id>` to the command. To get the series ID, follow the steps shown [here](###ID).

### Path

The relative / absolute path to the series can be changed by adding `path="<path>"` to the command.

### Episode Regex

The episode regex pattern can be changed by adding `matcher="<regex pattern>"` to the command. More information on the required pattern can be found [here](###Episode-Regex).

### Combining Options

You can combine multiple options from the sections above in any order when using the `set` command. For example:

`set id=1 matcher="EP{episode}" path="/media/anime/Cowboy Bebop"`

The above command will set the currently selected series to `Cowboy Bebop`, and look for episodes matching `EP{episode}` at the path `/media/anime/Cowboy Bebop`.
