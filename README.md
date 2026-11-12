# magic-tape

A native desktop YouTube client for Linux, written in Rust with [egui](https://github.com/emilk/egui).
It is a rewrite of the original `magic-tape.sh` Bash/fzf terminal script: the same features, but as a
real window with a thumbnail grid, a detail pane and mouse support.

## Requirements

| Tool | Needed for |
| --- | --- |
| `yt-dlp` | everything (feeds, search, subscriptions, comments) |
| `mpv` | playback |
| browser cookies | subscriptions, For you feed, history, private videos |

Optional: `libnotify`/`notify-send` for desktop notifications, `xdg-utils` to open folders.

## Build and run

```sh
cargo build --release
./target/release/magic-tape
```

Or straight into something:

```sh
magic-tape                              # opens on the For you feed
magic-tape --search "rust programming"  # open straight into a search
magic-tape --feed foryou                # foryou | subscriptions
```

## What you get

- **Feeds** in the sidebar: For you, Subscriptions, Channel, Previous action.
- **Search** with duration filters (any / short / medium / long / playlists).
- **Playlists, channels, liked videos and watch history** as separate sections.
- **Detail pane** for the selected video: description, comments (sortable, pinned-first), and
  buttons to play, download, like, subscribe, open in browser or copy the link.
- **Playback through mpv** — 1080p (`Enter`), best quality (`F`), audio only (`A`).
- **Downloads** to a configurable folder, video or audio only.
- **Local subscriptions and likes** — tracked like the original script; subscribing does not
  change your YouTube account.
- A **grid or list view**, dark / light / system theme, a searchable UI font list, and UI scale.

### Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Up` / `Down` / `J` / `K` | move the selection |
| `Page Up` / `Page Down` | move one screenful |
| `Home` / `End` | first / last video |
| `Enter` / `F` / `A` | play 1080p / best quality / audio only |
| `D` / `Shift+D` | download video / audio |
| `L` / `S` | toggle like / subscribe |
| `B` / `C` | open in browser / copy link |
| `R` | refresh the current feed |
| `,` / `.` | previous / next page |
| `Ctrl+F` / `Ctrl+,` / `Ctrl+Q` | search box / settings / quit |

Scrolling past the end of a feed auto-loads the next page.

### Upstream caveat: no Trending feed

YouTube retired `/feed/trending`, so the app does not offer it. Search, the For you feed and
channel feeds all work.

## Configuration

Settings live in `~/.config/magic-tape/config.toml`, created on first run; the Settings window
(`Ctrl+,`) writes them. If the old `magic-tape.conf` from the shell script exists, its values are
imported once as defaults. The important ones:

| Key | Meaning |
| --- | --- |
| `cookies_from_browser` | browser to read cookies from: `chrome`, `firefox`, ... (empty disables) |
| `download_directory` | where downloads go (default `~/Downloads`) |
| `list_length` | videos per page (1-99) |
| `list_view` | `grid` or `list` |
| `theme` | `dark`, `light`, `system` |
| `show_comments`, `comments_max` | comment loading |
| `comments_sort` | `new`, `top`, `old` |
| `ui_font`, `ui_zoom` | interface font and scale (0.7 - 2.0) |
| `mpv_binary`, `ytdlp_binary` | override the executables |

State on disk:

- `~/.local/share/magic-tape/subscriptions.txt` — your subscription list (backed up on import)
- `~/.cache/magic-tape/history/` — watch history, liked videos, search history
- `~/.cache/magic-tape/jpg/` — thumbnail cache (clearable in Settings)
- `~/.cache/magic-tape/comments/` — cached comments

All of these are managed from Settings → Maintenance, which also imports subscriptions from your
YouTube account, clears caches and history, and can update yt-dlp.

## Troubleshooting

Every `yt-dlp` and `mpv` invocation is logged, so failures are diagnosable:

```sh
magic-tape 2> magic-tape.log   # the app logs to stderr
RUST_LOG=debug magic-tape      # more detail
```

Common issues:

- **The subscriptions feed is empty.** The cookies could not be read — set
  `cookies_from_browser` to a browser you are signed into YouTube with, then refresh.
  Settings → Maintenance can re-check it.
- **Playback errors with `HTTP error 403`.** This is a yt-dlp client quirk; the app pins the
  `default` client (Settings → Playback) which avoids it.
- **Choppy playback on AMD GPUs.** mpv fell back to software decoding. Adding your user to the
  `video` group and loading the `amdgpu` kernel module enables hardware decoding.

## Development

```sh
cargo test                                  # unit tests, including real yt-dlp payload fixtures
cargo test -- --ignored --nocapture live_   # opt-in: plays a real video through mpv
cargo clippy --all-targets
```

Layout: `src/main.rs` (window, CLI, keyboard), `src/app.rs` (state and actions),
`src/model.rs` (types and yt-dlp JSON parsing), `src/yt.rs` (the yt-dlp command line),
`src/store.rs` (history, likes, subscriptions), `src/tasks.rs` (background workers),
`src/thumbs.rs` (thumbnail queue), `src/player.rs` (mpv and browser launching),
`src/ui/` (sidebar, toolbar, grid/list, detail pane, settings).
`tests/fixtures/*.json` are trimmed captures of real `yt-dlp -J` output, so a change in
YouTube's response format is caught by `cargo test` rather than at runtime.

## Licence

GPL-3.0-or-later, like the original script.
