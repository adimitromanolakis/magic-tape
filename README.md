# magic-tape

A native desktop client for YouTube, written in Rust with [egui](https://github.com/emilk/egui).
It is a rewrite of the original `magic-tape.sh` Bash/fzf terminal script: same features, but as a
real window with a thumbnail grid, a detail pane and mouse support.

All YouTube data is fetched by shelling out to `yt-dlp` (and playback is handed to `mpv`), exactly
like the original script. The Rust side only replaces the fzf/jq plumbing.

## Requirements

| Tool | Needed for |
| --- | --- |
| `yt-dlp` | everything (feeds, search, subscriptions, comments) |
| `mpv` | playback |
| `curl` | not used any more - thumbnails are fetched in-process |
| browser cookies | private feeds, history, subscriptions |

Optional: `libnotify`/`notify-send` for desktop notifications, `xdg-utils` to open folders.

Build and run:

```sh
cargo build --release
./target/release/magic-tape
```

## Speed

Measured on the development machine, per request:

| Request | Before | After |
|---------|--------|-------|
| Search, 5 results | 3.2 s | **1.0 s** |
| Comments, one video | 4.5 s | ~3.4 s |
| Subscriptions feed | 4.5 s | **1.8 s** |
| yt-dlp process start | 0.22 s | 0.22 s |

The large win is **not carrying cookies when they are not needed**. yt-dlp was
sent `--cookies-from-browser chrome` on every single call, and merely *having*
cookies puts the request on YouTube's authenticated path - a cookie-free search
takes 1.0 s and returns the same results. Public work (search, channel pages,
playlists, ordinary comments) now runs cookie-free and falls back only when
YouTube answers with a login error. Subscriptions, trending and downloads
always carry them.

Cookies are read from the browser on each request rather than exported to a
file. Exporting was tried: it saved about 0.2 s a call, and yt-dlp's browser
reader reaches YouTube's `SID`/`HSID`/`SSID` cookies where a static export
silently drops them - which turns the subscriptions feed into an empty list
with exit code 0. An empty feed from a signed-in request is now reported in the
log and Settings can re-check it on demand.

Thumbnails are 720p (`hq720`) and mipmapped. Two things were wrong before:
`maxresdefault` is the same 1280x720 but was tried first on every video, so it
cost a 404 whenever it was absent; and linear filtering with no mip chain
aliased badly when a 1280px picture was drawn into a 260px card, which reads as
nearest-neighbour.

Fetching is bounded and configurable in Settings -> Maintenance:
`parallel_ytdlp_runs` (default 6) and `parallel_thumbnails` (default 8). Comments
are always limited to one, since that is a correctness property, not a tuning
knob.

Caches are pruned oldest-first: 1200 videos of comments, 4000 thumbnails, and a
byte budget on each.

## Icons

Icons in the folder chooser come from your own desktop icon theme, resolved
the way a toolkit does it: `XDG_ICON_THEME`, then `gtk-icon-theme-name` from
your GTK settings, then Adwaita, then `hicolor`. Both SVG and PNG are read, and
names are tried in a short chain, because `go-up` is a GTK name that other
toolkits call `go-previous`. A theme with no `index.theme`, or one that does
not inherit from Adwaita, still gets GTK-only icons from the fallbacks. When a
name really is nowhere to be found, the row draws its text and nothing else.

## Fonts and scale

Settings → General → Appearance. The font list is built from the fonts installed on the machine
(378 on the development box), searchable inside the dropdown. The chosen file is inserted *ahead*
of egui's built-ins rather than replacing them, so a font without CJK or emoji glyphs still shows
those characters through the fallbacks. The size slider scales the whole interface, including the
text the app paints itself.

Every text size comes from a single type scale, so the video grid, the compact list, the channel
cards, the sidebar and the detail pane all step through the same sizes. There is no synthetic bold:
a title is made prominent with a larger size and the theme's strongest text colour, so "bold" here
means brighter and bigger rather than a heavier face.

## Keyboard

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | move the selection |
| `PageDown` / `PageUp` | a screenful of rows, staying in the same column |
| `Home` / `End` | first / last item |
| `,` / `.` | previous / next page of results |
| `Enter` | play · `F` best · `A` audio only |
| `d` / `Shift+D` | download video / audio |
| `l` · `s` · `b` · `c` | like, subscribe, open in browser, copy link |
| `r` · `Ctrl+F` · `Ctrl+,` · `Ctrl+Q` | refresh, search, settings, quit |

## Comments

Comments load when you settle on a video: one yt-dlp run at a time, ever. A
fast pan through the grid costs the run already in flight plus, at most, the
video you stopped on - requests for videos you have already passed are dropped,
and already-cached comments are never fetched twice.

## Downloads

The download folder accepts absolute paths, which is what you want for a separate disk or a
scratch volume:

```
/mnt/data/youtube    absolute, used as it is
/tmp                 absolute
~/Videos             your home directory
Videos               your home directory (the default is ~/Downloads)
```

A folder that does not exist yet is created on the first download; the Settings window shows the
resolved path and warns when it is missing.

## Usage

```sh
magic-tape                                  # opens on your subscriptions feed
magic-tape --search "rust programming"      # open straight into a search
magic-tape --feed trending                  # subscriptions | trending | recommended
magic-tape --help
```

### The detail pane

The pane is a rail anchored to the right edge, full height, and it is drawn *over* the grid rather
than taking a column from it - so selecting a video leaves the grid's column count, card size and
scroll position exactly where they were. Its whole content scrolls as one page; the video's picture
sits at the top of the pane, fitted to the pane's width without being stretched, and `×` floats over
its top-right corner, where it stays while the details scroll and hides the pane without losing the
selection - which is how the column it covers comes back. The pane's left edge can be dragged to
resize it. See `detail_background` below for its colour.

### Switching feeds

Picking another feed, refreshing with `R`, or searching the feeds clears the grid as the request goes
out, and the empty grid shows a spinner with what is being fetched. The feed that is being replaced
is never left on screen looking like the answer to the click. Loading a *page* is the other way
round: the next page is appended to what is already there, and the list, the selection and the
scroll position stay where they were.

### Keyboard shortcuts

| Key | Action |
| --- | --- |
| `Up` / `Down` / `J` / `K` | move the selection |
| `Page Down` / `Page Up` (or `Shift+Down` / `Shift+Up`) | move one screenful |
| `Home` / `End` | first / last video |
| `Down` at the last loaded item | loads the next page automatically |
| `Enter` | play 1080p |
| `F` | play best quality |
| `A` | play audio only |
| `D` / `Shift+D` | download video / audio |
| `L` | toggle like |
| `S` | subscribe to the channel |
| `B` | open in browser |
| `C` | copy link |
| `R` | refresh the current feed |
| `,` / `.` (or `P` / `N`) | previous / next page |
| `Ctrl+F` | focus the search box |
| `Ctrl+,` | settings |
| `Ctrl+Q` | quit |

## Feature parity with `magic-tape.sh`

| Original shell feature | Here |
| --- | --- |
| `f` subscriptions feed | Feeds → Subscriptions |
| `t` trending feed | Feeds → Trending (see caveat below) |
| `y` algorithm feed | Feeds → For you |
| `c` channel feed | Feeds → Channel, or click a channel in the sidebar |
| `s` search with duration filters | search box + the four duration chips |
| `r` repeat previous action | Feeds → Previous action |
| `l` liked videos | Liked videos (and `L` to toggle) |
| `h` watch history | Watch history |
| `j` search history | History button next to the search box |
| `n` subscribe to a new channel | Channels → search, then Subscribe |
| `u` unsubscribe | right-click a channel in the Channels list |
| `I` import subscriptions | Settings → Maintenance, and the sidebar button |
| `P` edit preferences | Settings window (no `$EDITOR` round-trip) |
| `H` / `S` / `T` clear history, thumbnails | Settings → Maintenance |
| `Y` update yt-dlp | Settings → Maintenance |
| fzf pagination (`Previous Page` / `Next Page`) | Prev / Next buttons in the toolbar, `,` / `.`, and auto-loading at the end of the list |
| play 720p/1080p/best/audio | detail pane buttons + `Enter`/`F`/`A` |
| download video/audio | detail pane buttons + `D`/`Shift+D` |
| image previews (kitty/ueberzug/chafa) | native thumbnails in the grid and detail pane |
| comments in the mpv OSD | comments and description in the detail pane |
| notifications | `notify-rust` (same toggles) |
| `fzf` pagination | Load more / Previous page, or `,` and `.` |

New conveniences: a real video grid or list toggle, right-click context menus, a comments sort
(never changed at the source level before), PageUp/PageDown navigation that follows the window
size, auto-loading the next page when you walk off the end of the list, and CLI arguments to open
directly into a search.

### Upstream caveat: the Trending feed is gone

YouTube retired `/feed/trending`, so that entry now fails with a clear explanation instead of a raw
yt-dlp error. Search, the For you feed and channel feeds all still work.

## Configuration

Settings live in `~/.config/magic-tape/config.toml` and are created on first run; the Settings
window (`Ctrl+,`) writes them. If the old `magic-tape.conf` from the shell script is present, its
values are imported once as defaults.

| Key | Meaning |
| --- | --- |
| `cookies_from_browser` | browser passed to `yt-dlp --cookies-from-browser` (`""` disables) |
| `list_length` | videos per page (1-99) |
| `download_directory` | a leading `/` means absolute (`/tmp`, `/mnt/data`); anything else, `~/Videos` included, is relative to `$HOME` |
| `download_video_quality` | yt-dlp format selector used by the **Best** button |
| `show_comments`, `comments_max`, `comment_replies_max` | comment loading |
| `comments_sort` | `new`, `top`, `old` |
| `pinned_comments` | `first` or `last` |
| `list_view` | `grid` or `list` |
| `theme` | `dark`, `light`, `system` |
| `detail_background` | background of the detail rail as `#RRGGBB`; empty follows the theme's panel colour |
| `ui_font` | path or name of an installed font; empty keeps egui's built-in font |
| `ui_zoom` | UI scale, 0.7 - 2.0 |
| `mpv_binary`, `ytdlp_binary` | override the executables |

### State on disk

- `~/.local/share/magic-tape/subscriptions.txt` - local subscription list (backed up on import)
- `~/.cache/magic-tape/history/` - watch history, liked videos, search history, last action
- `~/.cache/magic-tape/jpg/` - thumbnail cache (cleared from Settings)
- `~/.cache/magic-tape/comments/` - fetched comment metadata

Subscribing and liking are tracked locally, exactly as in the shell script - they do not change
your YouTube account. The app reminds you to confirm in the browser when you subscribe.

## Logging

Every `yt-dlp` and `mpv` invocation is logged, so a failure is always diagnosable:

```sh
magic-tape 2> magic-tape.log          # the app logs to stderr
RUST_LOG=debug magic-tape             # adds working directories and more detail
```

Log lines are tagged with their source:

```
[yt-dlp] list-feed :: yt-dlp --flat-playlist --playlist-start 1 ... https://www.youtube.com/feed/subscriptions
[yt-dlp] list-feed :: exit=exit status: 0 in 1.48s, stdout=4821B, stderr=0B
[mpv] play :: mpv --force-window=yes --keep-open=no --hwdec=auto-safe "--ytdl-format=22/bv*[height<=720]+ba/b[height<=720] / wv*+ba/w" https://www.youtube.com/watch?v=...
[mpv] play :: mpv --force-window=yes --keep-open=no --hwdec=auto-safe "--ytdl-format=bv*[height<=1080]+ba/b" https://www.youtube.com/watch?v=...
[mpv] play :: started, pid=385865
[mpv] play :: ● Video  --vid=1  (av1 1280x720 29.97 fps) [default]
[mpv] play :: ● Audio  --aid=1  --alang=eng  (opus 2ch 48000 Hz) [default]
[mpv] play :: Using hardware decoding (vaapi-copy).
[mpv] play :: AO: [pipewire] 48000Hz stereo 2ch floatp
[mpv] play :: VO: [gpu-next/libplacebo] 1920x1080 yuv420p
[mpv] play :: exited normally (exit status: 0)
```

On a machine without hardware acceleration the log will show things like
`amdgpu_query_info(ACCEL_WORKING) failed` or `Failed to acquire DRM master: Permission denied`.
Those mean mpv fell back to software decoding - playback still works, just without GPU decoding.
Adding your user to the `video` group and loading the `amdgpu` kernel module fixes that.

`yt-dlp` stderr is echoed line by line, and both of mpv's pipes are forwarded - its track and
`AO:`/`VO:` status goes to **stdout**, while only problems go to stderr, so both are captured.
Set **Settings → Playback → mpv log level** to change the detail (`all=warn` by default,
`all=info` for the full playback chain). Any mpv failure - including errors from its internal yt-dlp call, prefixed
`[ytdl_hook]` - is in the log.

Every mpv invocation - playback, audio-only and the availability probe alike - also prints the
complete command line to **stdout** as it happens, one argument per line, since the
`--ytdl-format` selectors carry commas and slashes that are unreadable packed together:

```
[mpv] play :: mpv (resolved: /usr/bin/mpv) with 9 argument(s)
[mpv] play ::   argv[0] = mpv
[mpv] play ::   argv[1] = --force-window=yes
[mpv] play ::   argv[5] = --ytdl-format=bv*[height<=1080]+ba/b[height<=1080] / wv*+ba/w
[mpv] play ::   argv[9] = https://www.youtube.com/watch?v=...
[mpv] play :: mpv pid=385865
[mpv] play :: pid=385865 exited normally (exit status: 0)
```

## How playback works

Clicking a quality button hands mpv the **YouTube page URL** plus a format selector and lets mpv's
own ytdl hook do the extracting:

```
mpv --force-window=yes --keep-open=no --hwdec=auto-safe \
    "--ytdl-format=22/bv*[height<=720]+ba/b[height<=720] / wv*+ba/w" \
    --ytdl-raw-options=cookies-from-browser=firefox \
    https://www.youtube.com/watch?v=...
```

The cookies setting is forwarded so private and members-only videos keep working, and the player
client is pinned (Settings → Playback → *yt-dlp client*, default `default`).

Pinning is the important part. When yt-dlp has browser cookies it often switches to its
`web_embedded` client, and the signed URLs that client produces are rejected by ffmpeg with
`HTTP error 403 Forbidden` - which is why playback failed even though yt-dlp itself had resolved
the streams fine. Verified on this machine:

| client | 1080p available | URL fetchable by ffmpeg |
| --- | --- | --- |
| `default` | yes | yes (HTTP 206) |
| `web_embedded` | yes | **no (HTTP 403)** |
| `tv` / `web` / `ios` / `web_safari` | no formats offered here | - |

Only one client name may be given: mpv parses `--ytdl-raw-options` as a comma-separated list, so
`default,web,tv` makes mpv abort with `Error parsing option ytdl-raw-options`. The app splits the
value itself, uses the first entry and logs what it dropped.

An earlier version resolved direct media URLs first with `yt-dlp --get-url` and passed them to
mpv. That does not work in practice: the signed `googlevideo` URLs YouTube hands out are frequently
rejected with `HTTP error 403 Forbidden` when ffmpeg fetches them, especially once
`--cookies-from-browser` pushes yt-dlp towards its embedded/web client. Handing over the page URL
instead is also simpler, merges video and audio inside mpv, keeps seeking, and gets hardware
decoding (`vaapi-copy`, `nvdec`, ...) where it is available.

## Development

```sh
cargo test                                  # unit tests, including real yt-dlp payload fixtures
cargo test -- --ignored --nocapture live_   # opt-in: plays a real video through mpv
cargo clippy --all-targets
```

Layout:

| File | Responsibility |
| --- | --- |
| `src/main.rs` | window setup, panel composition, keyboard handling, CLI args |
| `src/app.rs` | application state, actions, job results |
| `src/model.rs` | `Video`/`Channel`/`Comment` types and all yt-dlp JSON parsing |
| `src/yt.rs` | the yt-dlp command line |
| `src/store.rs` | history, likes, subscriptions, thumbnail paths |
| `src/tasks.rs` | background worker threads and their result channel |
| `src/thumbs.rs` | thumbnail download queue and egui textures |
| `src/player.rs` | mpv and browser launching, notifications |
| `src/ui/` | sidebar, toolbar, grid/list, detail pane, settings, toasts |

`tests/fixtures/*.json` are trimmed captures of real `yt-dlp -J` output, so a change in YouTube's
response format is caught by `cargo test` rather than at runtime.

## Licence

GPL-3.0-or-later, like the original script.
