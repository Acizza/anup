# anup
This is a command line application to play downloaded anime and to update its watch progress to your [AniList](https://anilist.co) account.

It tries to make watching anime as easy as possible by letting you watch the next unwatched episode of a series simply by launching the program with a keyword from a terminal or run prompt.

Current features include:
* Automatic detection of a series based off of episode filenames
* Playing the next unwatched episode(s) of an anime in your default video player
* Offline mode
* Options to rate, put on hold, and drop an anime after playing each episode
* Support for rewatching an already completed anime
* Support for continuing an anime that has already been dropped or put on hold
* Automatic setting of the date you started watching and finished watching an anime
* Tracking multiple series in a single folder
* Support for anime that have multiple seasons merged together

The program is only intended to be used on Linux, but untested support is included for Windows and macOS.

# Building
anup is written in [Rust](https://www.rust-lang.org), so you will have to compile the application before you can use it.

Linux
-----
1. Install the latest nightly version of Rust from your distribution's package manager, or from [here](https://rustup.rs).
2. Ensure OpenSSL, GCC, pkgconfig, and xdg_utils are installed (note: these packages usually are already).
3. In the directory you cloned the project in, run `cargo build --release`.

Windows
-------
1. Install the latest nightly version of Rust from [here](https://rustup.rs).
2. Open CMD / PowerShell in the directory you cloned the project in by pressing Shift + Right click in the directory and selecting "Open command window here" or "Open PowerShell window here".
3. Run `cargo build --release` and wait for it to finish.

Once the application has finished compiling, you will find the resulting executable in the `target/release/` directory.

If you're using Linux, you will also find a shell script in the same directory named `anup.sh`.
This script will run the program in a new terminal for you if you try to launch it from something like a run prompt, but you do not have to use it.

# Playing An Anime
Whenever you want to watch a series through the program for the first time, you will need to specify the path to it with the `-p` flag.
To avoid having to specify the path to the series every time you want to play it, you can give the series a custom name.
For example, if you wanted to watch Toradora and save it as "tora", you can launch the program like so:
* Linux: `anup.sh tora -p <path to episodes>`
* Windows: `anup.exe tora -p <path to episodes>`
* macOS: `anup tora -p <path to episodes>`

The next time you want to watch the same series, you can simply launch the program with the name you gave it. For example, to watch Toradora again:
* Linux: `anup.sh tora`
* Windows: `anup.exe tora`
* macOS: `anup tora`

Tracking Multiple Series In A Single Folder
-------------------------------------------
Often, places you obtain anime from will be bundled together with specials or movies in the same folder as the main series. To track them separately, you can launch the program with a `subseries` parameter. For example, if you wanted to watch Toradora's specials and name the subseries "sp", you can launch the program like so:

* Linux: `anup.sh tora sp`
* Windows: `anup.exe tora sp`
* macOS: `anup tora sp`

The program will then prompt you to select which series in the folder you want to track, and save it for the next time you want to play the same subseries.

Offline Mode
------------
To play an anime in offline mode, you would launch the program the same ways you would above, but with the `-o` flag. For example, to play the saved series "tora" in offline mode:
* Linux: `anup.sh tora -o`
* Windows: `anup.exe tora -o`
* macOS: `anup tora -o`

To sync the updates you made while offline to AniList, you can either watch the series again without the `-o` flag, or simply launch the program with the `--sync` flag to synchronize all updates made to your saved series.

# Episode File Detection
Whenever you play a series, the program will try to automatically detect the real name of the series and which episodes of it you currently have.
The program can automatically detect episodes in multiple common layouts, such as:

* `[Group] Series Title - 01.mkv`
* `[Group]_Series_Title_-_01.mkv`
* `[Group] Series Title - 01 [1080p].mkv`
* `Series Title - 01.mkv`

In cases where the automatic detection fails, the program will prompt you to enter a custom [regex](https://www.regular-expressions.info/) pattern to use. When entering a custom pattern, you will need to input the `{name}` and `{episode}` magic values in the appropriate places. If you're watching a one-off like a movie, then the `{episode}` marker can be omitted.

For example, to parse episode files that are formatted like this:

`[Group] Ep01 - Series Title.mkv`

You could use this pattern to parse it:

`\[Group\] Ep{episode} - {name}.mkv`

# Playing An Anime With Merged Seasons
Occasionally, you may come across a series that has multiple seasons of it combined into the same name. For these cases, you can use the `-s` flag with the season number you want to watch.

Note that the program will prompt you to select the name of every season before the one that you want to watch, unless you have already watched them through the program. This is necessary so the program can calculate the correct offset in episodes.

# Configuration

The program's configuration file (which contains your account access token) is located in one of the following locations, depending on your platform:
* Linux: `~/.config/anup/`
* Windows: `C:\Users\{USERNAME}\AppData\Roaming\anup\config\`
* macOS: `~/Library/Preferences/anup/`
