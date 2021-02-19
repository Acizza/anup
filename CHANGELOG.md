# Changelog

## To Be Released

### Feature Changes

* The time remaining text in the info panel now only shows the minutes remaining. This was done to reduce the amount of renders that occur while watching an anime episode.

### Internal Changes

* The TUI is now rendered semi-reactively instead of once every second. Key presses will still always trigger a rerender, but ticks will only cause a rerender if state changes have occured. This reduces the CPU usage massively while the program is idle or while watching an anime episode.

* The TUI now operates asynchronously. Components now spawn their own async tasks to respond to events or perform ticks. State changes aren't tied to rerendering yet, though, so there is still a delay before changes are displayed.

* The async runtime has been changed from async-std to Tokio.

* The error / status log is now rendered by widgets provided by the [tui_utils](https://github.com/Acizza/tui-utils) library. This provides a small performance improvement.

## 0.1.1 - February 8th, 2021

### Internal Changes

* Migrated CI to GitHub Actions