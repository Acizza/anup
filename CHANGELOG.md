# Changelog

## 0.4.0 - March 21st, 2021

### Breaking Changes

* The config file now uses the [RON](https://github.com/ron-rs/ron) file format instead of TOML. This reduces both the compile time and size of release binaries, and is a cleaner format overall compared to TOML. Old config files will have to be manually migrated to the new one at `~/.config/anup/config.ron`. The RON specification can be found [here](https://github.com/ron-rs/ron/wiki/Specification).

* Database migrations from June 15h, 2020 have been removed. If you have a series database from before that date and have not ran the program since then, you can either delete it or run the last stable version first to upgrade it.
  The series database is located at `~/.local/share/anup/data.db`.

### Fixes

* Errors during initialization of the TUI will no longer screw up the terminal.

### Internal Changes

* The entire TUI now only uses widgets and layouts from the `tui-utils` library. This reduces the size of release binaries and provides a small performance improvement during rendering.

## 0.3.0 - March 5th, 2021

### Features

* Logins are now performed asynchronously. The info & user panel will show a message describing who is logging in while it's in progress. This allows the TUI to be interactive immediately after launching the program.

### Fixes

* Fixed various overflow issues in the add series panel.

### Internal Changes

* The following components are now rendered partially or fully with widgets from the tui-utils library to provide a small performance improvement:
    * The add series panel (fully)
    * The main info panel (fully)
    * The delete series panel (fully)
    * The user management panel (partially)
    * The split series panel (partially)
    * Input widget (fully)

## 0.2.0 - February 22nd, 2021

### Feature Changes

* The time remaining text in the info panel now only shows the minutes remaining. This was done to reduce the amount of renders that occur while watching an anime episode.

### Internal Changes

* The TUI is now rendered reactively instead of once every second. Only key presses, state changes, and window size changes will trigger a rerender. This reduces the CPU usage massively while the program is idle or while watching an anime episode.

* The TUI now operates asynchronously. Components now spawn their own async tasks to respond to events or perform ticks.

* The async runtime has been changed from async-std to Tokio.

* The status log and command prompt are now rendered by widgets provided by the [tui_utils](https://github.com/Acizza/tui-utils) library. This provides a small performance improvement.

## 0.1.1 - February 8th, 2021

### Internal Changes

* Migrated CI to GitHub Actions