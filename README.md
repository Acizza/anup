# anup
This is a command line application to play downloaded anime and automatically syncronize it on [AniList](https://anilist.co).

The program allows you to very easily play and keep track of any series you have locally by letting you give it a custom name to refer to it again later.

Current features include:
* Automatic detection of a series based off its filename
* Playing the next unwatched episode(s) of a series
* Rating a series after playing an episode
* Dropping / putting a series on hold after playing an episode
* Rewatching support
* Automatically sets the start and end date of a series
* Support for episode files that have multiple seasons merged together

# Usage
First, please ensure the names of your legally obtained episodes resemble that of a \*cough\* torrent \*cough\*. The program can detect multiple layouts that are commonly used, such as:

* `[Group] Series Title - 01.mkv`
* `[Group]_Series_Title_-_01.mkv`
* `[Group] Series Title - 01 [1080p].mkv`
* `Series Title - 01.mkv`

To watch a series, you will need to specify the path to it with the `-p` flag. This only needs to be provided once if you decide to specify a name for the series. For example, if you want to watch Toradora and want to save it as "tora", you can launch the program like so:
* Linux: `anup.sh tora -p <path to episodes>`
* Windows: `anup.exe tora -p <path to episodes>`
* macOS: `anup tora -p <path to episodes>`

The next time you want to watch the same series, you can simply launch the program with the name you gave it. For example, to watch Toradora again:
* Linux: `anup.sh tora`
* Windows: `anup.exe tora`
* macOS: `anup tora`

When you start the program for the first time, a URL will be opened in your default browser to let you authorize the program to access your AniList account. If you do not want your access token to be saved, you can launch the program with the `--dontsavetoken` flag. Please keep in mind that your access token is only encoded in the Base64 format if you decide to save it, which means that anyone can decode and use it.

When you play a series for the first time, the program will search for the anime detected in the episode files, and prompt you to select which series you are actually watching. It will then create a file in the same directory called `.anup` that will save your selection so you do not have to enter it again.

After you select the series you want to watch (or play the series again later), the program will open the next unwatched episode of the series in your default video player. Once you exit the video player, the program will automatically increment the watched episode count on your anime list and prompt you to either rate the series, drop it, put it on hold, play the next episode, or exit the program.

If you're watching a series that has multiple seasons and your episode files aren't split up appropriately, you can use the `-s` flag with the season number to watch a specific season. For example, if a series has one season that is 24 episodes and a second season that is 12, and the episode number on your files goes up to 36, you can launch the program with `-s 2` to start playing at the 25th episode file. Note that you will have to select the correct series up to the specified season number in order for the offset to be calculated correctly.

# Configuration

The program's configuration file (which contains your account access token) is located in one of the following locations, depending on your platform:
* Linux: `~/.config/anup/`
* Windows: `C:\Users\{USERNAME}\AppData\Roaming\anup`
* macOS: `~/Library/Preferences/anup/`

If you need to update your access token, or no longer wish to keep it stored, you can remove the `access_token` field under the `[user]` category in the configuration file, and you will be prompted to enter a new token the next time you launch the program. Remember to launch the program with `--dontsavetoken` if you don't wish to store it anymore.