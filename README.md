# anitrack
This is a command line application to automatically play downloaded anime and update the watched episode count on [MyAnimeList](https://myanimelist.net/).

# Usage
First, please ensure your episode names resemble that of a \*cough\* torrent \*cough\*. The program can detect multiple layouts that are commonly used, such as:

* `[Group] Series Title - 01.mkv`
* `[Group]_Series_Title_-_01.mkv`
* `[Group] Series Title - 01 [1080p].mkv`
* `Series Title - 01.mkv`

Once your legally obtained episodes resemble that format, you can either launch the program directly in the folder that contains the series' episodes, or provide the path to the folder as an argument to the program.

The first time you launch the program, it will ask you for your MyAnimeList username and password and will save it by default. If you do not want your credentials to be saved, you can launch the program with the `--nosave` flag. If you do want your credentials saved, please keep in mind that they are **NOT** securely encrypted.

The first time you try to play a new series, the program will search MyAnimeList for the name detected in the episode files, and prompt you to select which series you are actually watching. It will then create a file in the same directory called `.anitrack` that will save your selection so you do not have to enter it again.

After you select the series you want to watch (or you launch the program again later), the program will open the next unwatched episode of the series in your default video player. Once you exit the video player (or on Windows, the moment you play the episode) , the program will automatically increment the watched episode count on your MyAnimeList profile and prompt you to either rate the series, drop it, put it on hold, play the next episode, or exit the program.

For series that have multiple seasons and groups that don't split them up, you can use the `-s` flag with the season number to watch a specific season. For example, if a series has one season that is 24 episodes and a second season that is 12, and the episode number on your files goes up to 36, you can launch the program with `-s 2` to start playing at the 25th episode file. Note that you will have to select the correct series up to the specified season number in order for the offset to be calculated correctly.