# anup

[![Build Status](https://travis-ci.org/Acizza/anup.svg?branch=rewrite)](https://travis-ci.org/Acizza/anup)
[![total lines](https://tokei.rs/b1/github/acizza/anup)](https://github.com/acizza/anup)

This is a [TUI](https://en.wikipedia.org/wiki/Text-based_user_interface) application to play and manage anime with [AniList](https://anilist.co) for Linux systems.

Current features include:
* Automatic series detection (including specials, OVA's, and ONA's)
* Easy playing of unwatched episodes
* TUI interface to view, play, and modify all series added to the program
* Offline mode
* Multi-user support
* Built-in series splitter for [merged seasons](#merged-seasons) and integrated specials, OVA's, ONA's, and movies
* Automatic series status handling (watching, rewatching, completed, etc)
* Automatic series start / end date handling

Please note that only local files are supported.

# Screenshots

Main panel:

<img src="res/main_panel.png?raw=true" alt="Screenshot showing main panel" width="512"/>

Adding a series:

<img src="res/add_series.png?raw=true" alt="Screenshot showing a series being added" width="512"/>

User management:

<img src="res/user_panel.png?raw=true" alt="Screenshot showing user management panel" width="512"/>

# Building

This project requires the following dependencies:

* A recent stable version of Rust
* SQLite
* pkg-config
* xdg-open / xdg-utils

Note that pkg-config and xdg-open / xdg-utils are most likely already installed. If your distribution does not provide a recent version of Rust, you can obtain the latest version [here](https://rustup.rs/).

Once the dependencies are installed, you can build the project simply by running `cargo build --release` in the project's directory. Once compilation is complete, you will find the `anup` binary in the `target/release/` folder. None of the other files in that directory need to be kept.

# Usage

By default, the program will look for anime in `~/anime/` and play episodes with `mpv`. To change these, run the program once to generate the config file and change the `series_dir` and `player` fields in `~/.config/anup/config.toml`, respectively.

## Adding an Account

Before you can add and play a series, you will need to add an AniList account to the program. To do this, open [this URL](https://anilist.co/api/v2/oauth/authorize?client_id=427&response_type=token) and follow the instructions to obtain an account access token. Once you have a token, you will need to paste it into the program. To do this, first press `u` to open user management, and then `Tab` to switch to the add user panel. Now press either `Ctrl + Shift + V` **or** `Ctrl + V` (depending on your terminal) to paste the token. Once your token has been pasted in, you can press enter to add your account.

You can repeat this process as needed to add more accounts. Once you are done, you can press `Escape` to return to the main panel.

All accounts are saved to `~/.local/share/anup/users.bin` and are **not encrypted**. You can disable an account's token at any time by going to your AniList account settings, and navigating to the `Apps` section.

## Adding a Series

You can add a new series to the program by pressing the `a` key. A new panel will be displayed showing inputs for the series name, ID, path, and episode pattern that can cycled through with the tab key.

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

### Episode Pattern

This input is used to specify a pattern to use for detecting episodes. While the default episode detection works with many formats, there may be times where overriding it is necessary.

The pattern matches filenames 1-to-1 except where `*` and `#` are specified. The `*` character matches everything in the filename until the next character in the pattern is encountered, and `#` marks where episode numbers appear in the filename.

Here are a few examples of custom patterns:

#### Example 1:
* Filename: `EP01 - Series Title.mkv`
* Pattern: `ep#`
* Parsed episode: `01`

#### Example 2:
* Filename: `[Some Tags] 02 Series Title.mkv`
* Pattern 1: `[*] #`
* Pattern 2: `*#`
* Parsed episode: `02`

#### Example 3:
* Filename: `Series Title With Number At End 1 03.mkv`
* Pattern: `Series Title With Number At End 1 #`
* Parsed episode: `03`

### Example 4:
* Filename: `[Tag 1][Tag 2] Series Title - Episode Description - EP04.mkv`
* Pattern: `[*][*] series title -*- ep#`
* Parsed episode: `04`

Note that each example above can be detected by the default detector.

## Watching a Series

Once at least one series has been added, you can play the next episode of one by selecting the series with the up and down arrow keys and pressing enter. This will play the episode with the player set in your config file.

Once you start playing an episode, you should see a timer counting down in the `Info` panel. This represents the time needed until the episode will be considered watched. You can change how much of an episode you need to watch by modifying the `percent_watched_to_progress` field in the `[episode]` section of your config file. This field can be set to `0.0` if you do not wish to use this feature.

If you do not see a timer when you start playing an episode and have not touched the `percent_watched_to_progress` field in your config, make sure that the video player / script used to launch your video player does **not** exit immediately after starting to play something. You will need to set `percent_watched_to_progress` to `0.0` if there is no way to bypass that behavior, as episodes will never be counted as watched otherwise.

Once the timer disappears, the watched episodes of the series will be increased and synced to AniList (unless offline) when you exit your video player.

If the program cannot sync the newly watched episode to AniList (either because you're running in offline mode, or the request fails), you should notice a `[*]` symbol next to the series name on the main panel. This indicates that the series has changes locally that are not synced to AniList. The changes will automatically be synced to AniList the next time you run the program in online mode and do something with the series (watch an episode, rate it, etc). You can also use the `synctoremote` command to perform a sync immediately.

## Modifying an Existing Series

You can modify a series that has already been added to the program by selecting it and pressing the `e` key. The opened panel is similar to when adding a new series, except existing series information will be prefilled and the name input will be disabled.

Explanations for each input can be [found above in the section about adding a series, or by clicking here](#id).

## Deleting a Series

You can remove a series from the program (and optionally delete its files on disk) by selecting the series with the arrow keys and then by pressing `Shift + D`. The opened panel will show you the path the series is located in and display whether or not the episodes will be deleted on disk as well. You can toggle whether or not the episodes will be deleted by pressing the `d` key. By default, the episodes will **not** be deleted.

Once you have verified that everything is correct, you can press `Enter` to remove the series.

## Splitting a Series

In order to watch merged seasons, specials, OVA's, ONA's, and (numbered) movies that are in the same folder as the main series, you will need to split them up into their own folders first.

If you are unfamiliar with the concept of a merged season, please read about them [below](#merged-seasons).

You can split a series by selecting it in the program and pressing the `s` key to open the split series panel. Once loaded, the panel will show you all of the detected series within the folder that were found from AniList. You can then press the `s` key again to split each series into its own folder within the series path set in your config. Splitting a series does **not** move or copy any files; it only creates symbolic links.

After each series has been split, they can be selected with the arrow keys and added to the program by pressing `Enter`. You then only need to specify a nickname for the series and press `Enter` again to add the series. This process can be repeated as many times as necessary.

Once you are done, you can press `Escape` to go back to the main series panel.

### Merged Seasons

A merged season is simply a season of an anime that has its episodes joined together with another season sequentially. The following table shows what 3 merged seasons may look like, where each season has 3 episodes:

<table>
  <tr>
    <td><b>Filename</b></td>
    <td><b>Season</b></td>
    <td><b>Actual Episode</b></td>
  </tr>
  <tr>
    <td>Series Title - 01.mkv</td>
    <td rowspan="3">1</td>
    <td>1</td>
  </tr>
  <tr>
    <td>Series Title - 02.mkv</td>
    <td>2</td>
  </tr>
  <tr>
    <td>Series Title - 03.mkv</td>
    <td>3</td>
  </tr>
  <tr>
    <td>Series Title - 04.mkv</td>
    <td rowspan="3">2</td>
    <td>1</td>
  </tr>
  <tr>
    <td>Series Title - 05.mkv</td>
    <td>2</td>
  </tr>
  <tr>
    <td>Series Title - 06.mkv</td>
    <td>3</td>
  </tr>
  <tr>
    <td>Series Title - 07.mkv</td>
    <td rowspan="3">3</td>
    <td>1</td>
  </tr>
  <tr>
    <td>Series Title - 08.mkv</td>
    <td>2</td>
  </tr>
  <tr>
    <td>Series Title - 09.mkv</td>
    <td>3</td>
  </tr>
</table>

# Keybindings

| Key       | Action                                                 |
| --------- | ------------------------------------------------------ |
| Q         | Exit the program                                       |
| Enter     | Play the next unwatched episode of the selected series |
| A         | Add a new series to the program                        |
| E         | Edit the selected series                               |
| Shift + D | Delete the selected series                             |
| S         | Split the selected series                              |
| U         | Open the user management panel                         |
| :         | Enter a command                                        |

## Panels

| Key    | Action                                     |
| ------ | ------------------------------------------ |
| Tab    | Cycle the active input / focused sub-panel |
| Escape | Go back one panel                          |

## Inputs

| Key             | Action                                  |
| --------------- | --------------------------------------- |
| Left arrow key  | Move the cursor to the left             |
| Right arrow key | Move the cursor to the right            |
| Home            | Move the cursor to the beginning        |
| End             | Move the cursor to the end              |
| Backspace       | Delete the character next to the cursor |

## Command Entry

| Key    | Action                   |
| ------ | ------------------------ |
| Tab    | Autocomplete the command |
| Enter  | Run the command          |
| Escape | Return to the main panel |

# Commands

| Name | Arguments | Description |
| ---- | ----- | ----------- |
| args | `<player args>` | The arguments to pass to the video player when playing an episode of the selected series |
| progress | `<f, forward \| b, backwards>` | Manually increment / decrement the watched episodes of the selected series |
| syncfromremote | | Retrieve the list entry of the selected series from AniList |
| synctoremote | | Update the list entry of the selected series on AniList |
| rate | `<0-100>` | Rate the selected series
| status | `<w, watching \| c, completed \| h, hold \| d, drop \| p, plan \| r, rewatch>` | Set the current watch status for the selected series

# Automatic Status & Date Management

The status of each series and its start/end date are also automatically managed by the program. The table below shows the various status transitions that occur, where the `From` column is the status before watching an episode, and the `To` column is the status after watching one:

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
