# Known issues

Short, actionable write-ups of things that are wrong or missing, in the order
they were spotted. Each file is self-contained: what happens, why it matters,
what "fixed" should look like, and which files to touch.

Status values: `open`, `fixed`, `wontfix`.

| # | Issue | Status |
|---|-------|--------|
| [001](001-ctrl-f-does-not-focus-the-search-box.md) | `Ctrl+F` clears the search box but never focuses it | fixed |
| [002](002-escape-does-nothing.md) | Escape is not handled anywhere in the app | open |
| [003](003-the-history-button-is-a-dead-end.md) | The History button fills one term and stops | open |
| [004](004-cookies-from-browser-default-breaks-everything.md) | `cookies_from_browser` defaults to `firefox`, and a missing browser kills every request | open |
| [005](005-no-unsubscribe-in-quick-access.md) | No unsubscribe on the sidebar's quick-access rows | open |
| [006](006-the-detail-panel-is-thin.md) | No channel header, nothing to jump to from a video | open |
| [007](007-download-quality-is-a-config-string.md) | Download quality means editing TOML | open |

Also fixed, not worth its own file: the download folder now has a **Browse...**
button that opens a folder chooser (`src/ui/folder_picker.rs`) instead of
making you type the path. It is hand-rolled rather than a native dialog, since
the usual crates want a GTK toolchain at build time or a portal service at run
time.

Parity notes: the shell version this app replaced (`magic-tape.sh`) has a
miscellaneous menu with unlike, unsubscribe, clear watch history, clear search
history, clear thumbnail cache and update yt-dlp. All of those exist here; the
issues above are about the ones that are present but weaker than they should
be.
