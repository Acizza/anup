# Changelog

## To Be Released

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