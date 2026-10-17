# NOTES.md — magic-tape (Rust rewrite)

Engineering notes for agents and humans working on this codebase. `README.md` is the user
documentation; this file is the *why* and the *gotchas*.

## 1. What this is

A native desktop YouTube client written in Rust with **egui/eframe 0.35**, replacing the original
`magic-tape.sh` (Bash + fzf + jq) that still sits in the parent directory. Same feature set, a
real window instead of a terminal UI.

Deliberate non-goals: this does **not** reimplement YouTube extraction. `yt-dlp` remains the
extractor and `mpv` the player; Rust replaces only the fzf/jq plumbing and adds a GUI. Do not
"improve" it by adding a YouTube API client or a Rust yt-dlp port — see §6.1 for why the shelling
out is load-bearing.

State at the time of writing: `v0.2.0`, 17 files, 45 unit tests + 3 opt-in network tests, zero
clippy warnings. Tracked in git; tag `v0.2.0` marks the first working
release.

## 2. Quick start

```sh
cargo build --release                    # binary: target/release/magic-tape
cargo test                               # unit tests, no network
cargo test -- --ignored --nocapture live_  # opt-in: hits YouTube + mpv
cargo clippy --all-targets               # must stay at zero warnings
```

Runtime requirements: `yt-dlp` (everything) and `mpv` (playback), both on `PATH`. Optional:
`notify-rust`/D-Bus for notifications, `xdg-open` for "open folder", `ffmpeg` (mpv's dependency).

Run it with a pre-filled search or feed:

```sh
magic-tape --search "rust programming"
magic-tape --feed trending        # subscriptions | trending | recommended
```

## 3. Architecture

Single window, single-threaded UI, all slow work on worker threads.

```
   main.rs ── eframe event loop
      │  logic()  → drain worker results into App state, repaint while busy
      │  ui()     → Panel::left(sidebar) / bottom(status) / CentralPanel
      │             + the detail pane, an `Area` floating over the list (§6.3.16)
      ▼
   app.rs (App)  ── the only mutable state: config, selection, feed, section, detail
      │  spawns jobs, applies Outcome
      ▼
   tasks.rs (Worker)  ── std::thread per job, results over mpsc
      │
      ├─► yt.rs      (YtDlp)   runs yt-dlp, returns parsed models
      ├─► store.rs   (Store)   history / likes / subscriptions as plain text files
      └─► thumbs.rs  (ThumbCache) download queue + egui textures
```

Rules that keep this sane:

- **`App` owns everything mutable; UI modules take `&mut App`.** No interior mutability, no Rc/RefCell.
- **Never call `Store` file readers from the UI.** `Store::subscriptions()`, `watch_history()` and
  friends hit the disk on every call; the sidebar, the detail pane and the Settings window all need
  them every frame, so `App` caches them (`App::subscriptions`, `App::is_subscribed`) and reloads
  after any write (`reload_subscriptions()` after import/subscribe/unsubscribe). Calling
  `store.is_subscribed()` in a per-frame path is a regression — the detail pane's Subscribe button
  used to re-read and re-parse the whole file on every repaint.
- **`Worker::spawn(label, closure)`** returns a closure producing one `Outcome`. The `label` is
  only for debug logs; the closure does the work. Never spawn from the UI without a label.
- **The UI never blocks.** `logic()` drains `worker.drain()` every frame and calls
  `request_repaint_after(120ms)` while `worker.busy()`, so results appear without a user event.
- `thumbs.rs` decodes images with the `image` crate into `ColorImage`s; `egui_extras` is *not* used.

## 4. Module map

| File | Lines | Responsibility |
| --- | --- | --- |
| `src/main.rs` | 437 | Window/panel composition, CLI args, key handling, selection movement |
| `src/app.rs` | 1996 | Application state, actions (play/like/subscribe/download), `Outcome` handling, navigation helpers |
| `src/model.rs` | 645 | `Video`/`Channel`/`Comment` types + **all** yt-dlp JSON parsing, time/number formatting |
| `src/yt.rs` | 990 | The yt-dlp command line; one `run()` that logs everything; per-video comment files; URL encode/decode |
| `src/player.rs` | 443 | mpv invocation, log forwarding, exit-status watcher, notifications, `opener` |
| `src/store.rs` | 456 | Watch history, liked, search history, last action, subscriptions, thumb paths |
| `src/tasks.rs` | 403 | `Outcome` enum, `Worker` (thread + mpsc), thumbnail download |
| `src/font.rs` | 225 | Font discovery and installation, UI scale |
| `src/thumbs.rs` | 291 | Thumbnail texture cache, placeholder, in-flight de-duplication |
| `src/config.rs` | 466 | `Config` (TOML, `#[serde(default)]`), `Paths`, legacy `magic-tape.conf` import |
| `src/ui/sidebar.rs` | 292 | Nav, feed list, subscription quick access, replay-previous-action |
| `src/ui/toolbar.rs` | 304 | Two-row toolbar + `paging_controls()` (shared with the list footer) |
| `src/ui/video_list.rs` | 562 | Virtualised grid, compact list, channel list, context menus, empty state |
| `src/ui/detail.rs` | 510 | Detail rail: picture, action buttons, description, comments, floating close button |
| `src/ui/settings.rs` | 654 | Settings window (4 tabs) + shortcuts window |
| `src/ui/toasts.rs` | 47 | Toast overlay (anchored below the toolbar) + status bar |
| `src/ui/mod.rs` | 122 | Colors, `card()`, text measurement and `ellipsize()` |
| `src/ui/theme.rs` | 192 | Design tokens: the type scale, the card metrics, colour parsing |

### Data on disk

| Path | Contents |
| --- | --- |
| `~/.config/magic-tape/config.toml` | settings; created on first run; legacy `magic-tape.conf` values imported as defaults |
| `~/.local/share/magic-tape/subscriptions.txt` | `<channel_id> <name>` per line; backed up on import |
| `~/.cache/magic-tape/history/` | `watch_history.txt`, `liked.txt`, `search_history.txt`, `last_action.txt` |
| `~/.cache/magic-tape/jpg/` | `img-<id>.jpg` thumbnail cache |
| `~/.cache/magic-tape/comments/` | fetched comment metadata |

All paths come from `dirs::*` via `config::Paths`; nothing is hardcoded.

## 5. Conventions

**Logging** — `env_logger`, default filter `info`, `RUST_LOG=debug` for more. Every line is tagged
so a failure can be traced end to end:

| Tag | Emitted by | Contains |
| --- | --- | --- |
| `[magic-tape]` | `app.rs` boot | resolved binaries, config/cache paths, cookies client |
| `[yt-dlp]` | `yt.rs` | full argv, cwd, exit status, wall time, stdout/stderr sizes, **every stderr line** |
| `[mpv]` | `player.rs` | full argv, pid, both pipes forwarded line by line, exit status |
| `[action]` | `app.rs` | play / download / like / subscribe with the target URL |
| `[nav]` | `main.rs` | auto-load-more decisions |
| `[open]`, `[notify]` | `player.rs` | browser/folder opens, desktop notifications |

Each yt-dlp call site passes a label to `run_output`/`run_output_in`: `list-feed`,
`list-playlists`, `search-channels`, `import-subscriptions`, `video-metadata`, `download`,
`version`, `update` — plus an `availability check` logged by `YtDlp::available()`. Add a label when
adding a call.

**Fonts** — `font.rs` discovers installed fonts once (`OnceLock`) and installs the chosen one
*ahead of* egui's built-ins, never replacing them, so glyphs the chosen font lacks (CJK, emoji) fall
back. UI size is a `Context::set_zoom_factor`, not a `TextStyle` edit, because the app paints some
of its own text with explicit `FontId`s that a style change would miss. Both are applied from
`MagicTape::apply_font_change` in `logic()`, which only reinstalls when the setting actually changed,
the same trick as the theme.

**Typography** — every text size comes from `ui::theme` (`T_APP`, `T_PAGE`, `T_TITLE`, `T_BODY`,
`T_META`, `T_CAPTION`), and every card radius, padding and gap from the same module's `CARD_*`
constants. Do not reintroduce a literal `.size(..)` on text: the scale is what keeps the grid, the
compact list, the channel cards, the sidebar, the detail pane and the settings window consistent.
The only literal sizes left in `src/ui` are spinner diameters. Note that egui has no bold face -
`RichText::strong()` only swaps in `strong_text_color()` (white on dark, black on light) - so a
"bolder" title means a larger size plus that colour, which is what `theme::title_color` does for
the grid card.

**The query belongs to the search feed** — `App::query` used to be a free-floating
string that every request read. `yt::list_feed` matched `(Some(q), _)`, so once a term was
stored *every* feed - channels, trending, subscriptions - asked yt-dlp for
`/results?search_query=...`: one search permanently hijacked navigation. Two functions now own
that decision: `app::search_query_for`, which only answers for `Feed::Search`, and
`yt::feed_path`, which only searches when the feed *is* the search feed. Neither is clever enough
to be obvious, hence both are tested. `App::feed_before_search` and `leave_search` give the user
an explicit way back, and the search box stays on screen instead of being swapped for the term.

**Download paths** — a leading `/` means absolute, anything else (including `~/x`) is relative to
`$HOME`. Do not normalise away the `~`: it is the only thing distinguishing the two. The setting used
to be `$HOME + value`, so `/mnt` silently became `$HOME/mnt`; `config::resolve_download_dir` is the
single place this is decided, and it is covered by tests including the migration of the old
`/Downloads` default to `~/Downloads`.

**Thumbnails** — the URL yt-dlp hands over is often unreachable: a `*_custom_*.jpg` is the
owner's own thumbnail and is served to nobody else, and the sharp variants are simply absent on
older uploads. A *removed* video has none at all - `Zper_lsRofk` 404s on `mqdefault` too - so no
amount of retrying helps. `model::pick_thumbnail` skips the owner-only names up front, and
`tasks::download_thumbnail` walks `thumbs::public_variants` (maxres → hq720 → hqdefault →
mqdefault, 16:9 last as the safety net) and keeps the first that answers. A video with nothing
public ends up on the placeholder via `ThumbCache::failed`, and logs a warning: it used to raise a
red toast per card, which buried the UI.

**Scrollbars** — every `ScrollArea` needs `auto_shrink([false, false])`. Without it the box hugs
its content width and the bar is drawn inset from the panel edge instead of at it; the sidebar's
two were the only ones missing it.

**Icons** — `ui::icons` resolves names through the freedesktop layout: the user's
theme, then `Inherits=` from `index.theme`, then `hicolor`, Adwaita and
AdwaitaLegacy as toolkits do. Two details cost a bug each: the context list has
to include `devices`, `legacy` and `symbolic/actions` (drives and symbolic icons
live there, and Adwaita's symbolic files have no size directory at all), and a
Qt theme like Qogir inherits nothing from Adwaita, so GTK-only names such as
`go-up` are only reachable through the explicit fallbacks. Names are asked for
as short chains (`go-up`, then `go-previous`), because a theme that ships
neither must not produce a blank row.

**Paging** — `App::page_target` is row-based, not item-based, and keeps the column.
This is the second time a size here was measured in the wrong unit: `visible_rows`
is a count of *rows*, and using it directly as an item delta moved 19 items across a
4-column grid, which is under five rows - so PageDown looked broken. In the list view
`columns` is 1, which must be set explicitly on every path that pages, since paging
multiplies by it and the grid leaves its value behind.

**Comment fetching** — bounded by a single slot, not by a timer. The 450ms settle
was standing in for the concurrency limit; with one-in-flight and newest-request-wins
in place it only has to collapse a burst of keystrokes, so it is 50ms. Two lessons
worth keeping: keying the in-flight guard on URL *text* let the same video run three
times (`watch?v=id`, `youtu.be/id`, tracking parameters), and a debounce that looks
like the fix for "too many processes" will hide a missing limit - check which one is
actually doing the work.

**Cookies** — two rounds of guessing wrong here, both worth remembering. The first guess was
that `--cookies-from-browser` was slow because it re-reads the browser's encrypted store; it
is not, and passing an exported file saves only ~0.2s. The real cost is *carrying* cookies at
all, which puts the request on YouTube's authenticated path. So public endpoints
(`Auth::Public`: search, channels, playlists, ordinary comments) go out anonymous and fall back
only on a login error, while subscriptions/trending/downloads are `Auth::SignedIn`.

The second guess was that exporting cookies to a file was worth it. It was not: yt-dlp's browser
reader reaches YouTube's `SID`/`HSID`/`SSID`, and a static export silently drops them - the jar
contained only `.google.com` cookies, passed a "does this look like a cookie jar" check that was
too weak, and the subscriptions feed then answered with 860 bytes and exit code 0. No videos, no
error. The file is gone; cookies go to the browser every time.

The lesson both times: an empty result that arrives with a success code is the worst possible
failure, and neither a size check nor a 0600 permission catches it. `live_feed_has_videos` exists
because of that - it boots the app and counts what the feed actually holds.

**Fetching limits** — `tasks::Permits` is a counting semaphore per `JobKind`. The permit is
taken *inside* the worker thread; taking it before spawning would block the UI thread,
which is the one thing the pool must never do. Comments stay at exactly one and are not
configurable: that single-flight is correctness, not tuning.

**The comment debounce** — the 450ms settle was standing in for a concurrency limit that now
exists. With one fetch in flight and the newest request winning, a fast pan costs the run already
going plus at most the video you stopped on, so the timer only has to collapse a burst of
keystrokes: 50ms. A test fails if it ever creeps back above that.

**Per-frame work** — two hot-path rules, both learned the hard way: do not call `set_visuals` every
frame (theme changes are tracked in `MagicTape::applied`), and do not build a `reqwest` client per
request (`tasks.rs` keeps one in a `OnceLock` for all workers).

**Errors** — `anyhow::Result` in the plumbing layer, `bail!` for expected user-facing failures.
Every failure becomes a toast *and* a log line; never swallow an error silently.

**Tests** — the default suite must not touch the network. Anything that does is `#[ignore]`d and
lives in the `mod live` block at the bottom of `yt.rs`. JSON parsing is pinned by
`tests/fixtures/*.json`, which are trimmed captures of real `yt-dlp -J` output (see §6.1).

## 6. Hard-won knowledge

Everything in this section cost a debugging cycle. It is the most important part of this file.

### 6.1 yt-dlp / YouTube (verified 2026-10, yt-dlp 2026.08.19)

1. **`-J` returns a playlist object, not a list.** `{ "entries": [...], "channel": ..., ... }`.
   The 2023 shell script parsed newline-delimited objects; that shape is gone. `model::parse_entries`
   accepts all three (playlist object / bare array / NDJSON) — keep all three.
2. **Channel feeds carry no channel on the entry.** The entries have `title` but *no* `channel`, and
   the channel lives on the playlist object. `PlaylistContext` in `model.rs` inherits it; without
   this, every video in a channel feed shows an empty channel name. Covered by two tests.
3. **`--print url` is wrong for merged formats; use `--get-url`.** A merged `bv*+ba` selection has
   no single `url` field, so `--print url` prints the literal string `NA`. (`--quiet` also swallows
   the output.) This caused the original "no playable stream returned by yt-dlp" error.
4. **`--playlist-start` is 1-based.** The shell script passed 0 on first load, which yt-dlp rejects
   with `playlist start "0" must be positive`. `yt.rs` converts 0-based offsets to `start + 1`.
5. **`/feed/trending` no longer exists** (YouTube retired it). The menu entry stays; `yt::explain()`
   rewrites that specific failure into a readable sentence instead of a raw 404.
6. **Signed `googlevideo` URLs from the `web_embedded` client 403 for ffmpeg.** Measured here:
   `default` → HTTP 206, `web_embedded` → HTTP 403, for the same video. Adding a browser UA and a
   Referer does *not* help ffmpeg, even though curl accepts them. This is why playback hands mpv
   the **page URL** and lets mpv's ytdl hook resolve, and why the client is pinned (below).
7. **Only `player_client=default` is usable here.** `tv`, `web`, `ios`, `web_safari` returned no
   formats at all on this box; `mweb` returned 403 URLs. Re-check the matrix before changing
   `config::Config::playback_client` (default `"default"`).
8. **Thumbnail URLs carry long `?sqp=…&rs=…` query strings.** `pick_thumbnail` strips them, and
   falls back to the canonical `https://i.ytimg.com/vi/<id>/mqdefault.jpg` so downloads stay small.
9. Entries can be `null` (filtered/private), and `duration_string`/`timestamp` are usually null in
   favour of numeric `duration` and `release_timestamp`. Both handled.

### 6.2 mpv

1. **Never resolve streams yourself.** `Player::play(url, selector)` passes the page URL plus
   `--ytdl-format=…`. mpv merges video+audio itself, seeking works, and hardware decoding engages
   (`Using hardware decoding (vaapi)`), which the hand-resolved path never reached.
2. **`--ytdl-raw-options` is a comma-separated list.** Passing `player_client=default,web,tv` makes
   mpv abort with `Error parsing option ytdl-raw-options (option parameter could not be parsed)`.
   `Player::single_client()` takes the first entry and logs what it dropped; there is a test.
3. **mpv writes its status to stdout and only problems to stderr.** Piping stdout to `/dev/null`
   hides `● Video`, `AO:`, `VO:` and the A/V status — it looks like a silent failure. Both pipes are
   captured and forwarded to the log.
4. Cookies are forwarded as `--ytdl-raw-options=cookies-from-browser=<browser>` so private videos
   work. Verified by deliberately passing a bogus browser name and confirming the error arrives
   from yt-dlp through the hook.
5. The `playback_client` pin exists purely to stop (6.1.6). Leave `web_embedded` out.

### 6.3 egui / eframe 0.35

This is a new API surface; several things differ from older tutorials.

0. **Panels and closures fight over borrows.** A quick-access list cannot hold `&App.subscriptions`
   while the filter box mutates `App` and row clicks need `&mut App`. Either scope the immutable
   borrow (clone the rows, or compute values first) or split the draw into phases.
1. **`eframe::App` has no `update()`.** It is `fn logic(&mut self, ctx, frame)` for non-UI work and
   `fn ui(&mut self, ui: &mut egui::Ui, frame)` for drawing. Drain worker results in `logic()`.
2. **Panels are `Panel::left(id).show(ui, …)`, not `SidePanel::…show(ctx, …)`.** `Panel::top/bottom/
   left/right` plus `CentralPanel::default().show(ui, …)`. `Window`/`Area` still take `&Context`.
3. **`ScrollArea::vertical_scroll_offset` takes `Option<f32>` (`None` = keep current).** Passing
   `f32::NAN` as a sentinel sets `Some(NaN)`, which produces NaN geometry and renders a **blank
   grid** with a correct-looking status bar. `App::take_scroll_offset` returns `Option<f32>` for
   this reason. This cost a long debugging session — check it first if the list renders empty.
4. **`UiBuilder::scope_builder` with an explicit `max_rect` does not advance the parent cursor.**
   A child that only paints (never allocates) leaves `min_rect` empty, so siblings overlap. The
   grid therefore calls `ui.allocate_exact_size(...)` first and paints relative to the returned
   rect, inside a `horizontal_top` row.
5. **`measure` the viewport height *before* drawing the scroll area.** `ui.available_height()` after
   it is ~0. This silently broke `visible_rows` and therefore PageUp/PageDown.
6. `egui::ImageOptions` has **no `fit_to_fraction`**; use the `Image` widget with
   `.fit_to_exact_size()` and `.corner_radius()`.
7. `Frame::new()` exists; `Frame::NONE` is the empty one. `StrokeKind::Inside` must be passed to
   `painter().rect(..., stroke, StrokeKind::Inside)`.
8. `FontId` is `Clone`, **not** `Copy` — helpers that take it should take `&FontId` and `.clone()`.
   Measuring text needs `ui.fonts_mut(|f| f.layout_no_wrap(text, font, WHITE))`.
9. `Layout::top_down(egui::Align::Min)` is the default; there is no `UiLayoutDirection`.
10. **Don't put a right-aligned cluster and left controls in one `ui.horizontal`.** They overlap
    when the row overflows (this bit the toolbar twice). The toolbar is now two rows and the
    second uses `ui.horizontal_wrapped`.
11. Truncating long card titles is done with `ui::ellipsize()` + `painter.text()`, because a
    `Label` inside a fixed-rect card forces layout side effects.
12. **egui draws a vertical scrollbar *over* the content**; it does not reserve space for it.
    `ScrollArea::max_width` narrows the box, but the bar is then drawn at that box's edge, so the
    text still runs underneath it. To keep a gap you must narrow the *rows*: the video grid
    computes its tile width from `available_width() - SCROLLBAR_GAP`.
13. **`Button::selectable` centres its text and 0.35 offers no way to change that**, so
    `add_sized(.., Button::selectable(..))` is not a drop-in for `ui.selectable_label` - the
    quick-access rows are drawn by hand instead (`allocate_exact_size` + `painter.text` with
    `Align2::LEFT_CENTER` + `ui::ellipsize`). The same hand-rolled row keeps the height exactly
    equal to the `row_height` given to `show_rows`, which is what the scrollbar is calibrated from.
14. Keep the hand-rolled row's *hover* and *selected* fills. `allocate_exact_size(.., Sense::click())`
    gives no visual feedback on its own.
14. **`ScrollArea::show_rows` derives the scrollable content height from the row height you pass
    it, plus `item_spacing.y`.** If that row height is not what a row really takes, the scrollbar
    is calibrated for a list that does not exist: the thumb sits in the wrong place and the last
    rows cannot be scrolled to. Two lists had this bug and both are now derived from named
    constants so the arithmetic cannot drift:
    - compact video list: `LIST_ROW_CONTENT_H (68) + 2 * LIST_ROW_MARGIN (4) = 76` (was 74)
    - channel list: `CHANNEL_ROW_CONTENT_H (40) + 2 * CHANNEL_ROW_MARGIN (6) = 52` (was 62 -
      off by 10px per row, so with 480 rows the last 90 were unreachable)
    Changing a row's content height means changing the matching constant, not the literal passed
    to `show_rows`.

15. **The bundled proportional font has no `←`.** egui's fallback chain for proportional text is
    Ubuntu-Light -> NotoEmoji -> emoji-icon-font, and U+2190 (`←`) is in none of them: it exists
    only in Hack, the monospace face. So `"← Back"` drew a missing-glyph box. Use a glyph the
    chain actually covers - `⬅` (U+2B05), `➡` (U+27A1), `◀`/`▶` (U+25C0/U+25B6) and `↩` (U+21A9)
    all render. The toolbar standardises on the `⬅`/`➡` pair for paging, so those arrows match the
    Back button instead of the flat triangles.

16. **The detail pane is an `Area`, not a `Panel::right`, and that is load-bearing.** As a panel it
    took its width out of the central panel, so selecting a video changed the grid's column count
    (`grid()` derives it from `available_width()`), resized every card and moved every row - and
    clearing the selection put them all back, which made clicking through a feed flash the layout.
    An area of its own leaves the grid's width, and therefore its geometry and scroll offset,
    untouched; it covers the right-most column instead, which is why it carries a close button.
    It is drawn as a rail: `App::content_rect` supplies the top edge (the rect the list was given,
    so the toolbar is never covered) and `Context::content_rect()` the flush right edge and the
    bottom, over the right end of the status bar, as the panel was. Three consequences: the pane's
    size has to be forced with `ui.set_min_size` + `ui.set_max_size`, because an area otherwise
    sizes itself to its content and would grow over the status bar; the layering works out like the
    panel's did, since every panel shares the window's base layer, so `Order::Middle` puts the area
    above the grid and still below windows, popups and tooltips; and `Context` has no `style()` in
    0.35 - take the style from a `Ui`.

    The pane has **one** scroller: a scroller per section (comments had their own) left the rest of
    the pane unreachable. `ui.set_max_width(available - SCROLLBAR_GAP)` inside it keeps text clear
    of the bar egui draws over its content (§12). Its fill comes from `config.detail_background`
    through `ui::pane_fill()`, which falls back to the theme's panel fill when the setting is empty
    or unparsable.

    The close button is **painted, not laid out** - `Frame`, then `body()`, then `close_button()` -
    so the picture can start at the top of the pane instead of under a caption row. Painting it
    after the scroller also puts it on top of the picture and keeps it there while the details
    scroll beneath it, and a later `ui.interact` on the same rect wins the hit test against the
    scroller underneath, which is what makes the button clickable at all. It draws its own dark
    disc because it sits over a photograph, where `Visuals::text_color` is not readable, and the
    cross is two `line_segment`s rather than a glyph: egui has no bold face, so `RichText::strong()`
    only brightens text and a heavier X has to be a heavier stroke.

17. **A load from the top clears the list as it *starts*, not when it lands.** Clicking another feed
    kept the outgoing feed on screen until the new one arrived: a stale grid that reads as if the
    click did nothing, with nothing on screen saying otherwise. `App::begin_load` - split out of
    `load_feed` the way `begin_search` is, so the transition is testable without yt-dlp - now drops
    `feed_state` (and the selection with it, which empties the detail pane) before the job is
    spawned. The empty list is the point: `video_list::empty_state` is what draws the spinner, and
    the `status` line is what says which feed is being fetched.

    Two scoping rules are load-bearing. The clear happens only for `start == 0`, because a later
    page is *appended* to what is already loaded and paging must not blank the list. And
    `scroll_to_index` is set to the top, because the old offset means nothing in a list that has not
    arrived yet. One consequence worth knowing: a feed load that *fails* now leaves an empty view
    with the error toast as the only trace, where it used to leave the previous feed.

    Emptying the list exposed the second half of the same bug, which was there all along: two loads
    can be in flight at once (nothing cancels the first), so the answer to the feed the user just
    left arrives *after* the new list was emptied and paints the old feed into it - the same thing
    the user complained about, from the other end. `App::feed_request` records `(feed, query,
    channel, start)` as the request goes out, and `handle_outcome` drops an answer that does not
    match it (with a log line). All four parts are needed: two searches differ only in the query,
    and one channel's page from another's differ only in the channel, both at `start == 0`. The
    guard also has to come before the branch clears `feed_state.loading`, or a stale answer would
    stop the spinner while its successor was still running.

    The spinner and its caption are placed by hand inside `centered_and_justified`, which hands its
    child the whole area: stacking them from the top puts the block up under the toolbar (§6.4 has
    the measurement), so the block's height is subtracted from the available height and half the
    remainder added above it.

### 6.4 Verifying changes on this machine

- **The real X display is `:0`** (Xorg + KWin, AMD Radeon RX 6750 XT, VA-API). `DISPLAY` is
  already `:0` in the shell. Run there to exercise real GPU output.
- **Do not test under Xvfb** if you care about video output: Xvfb has no DRI3, so mpv falls back to
  `vo=sdl` with software decode. That is an artefact of the harness, not a bug in the app.
- **Match windows by exact name.** `xdotool search --name "magic-tape"` also matches the user's
  original `magic-tape.sh` window in kitty, and `xdotool key --window` then delivers keystrokes to
  the terminal. Reliable incantation:
  ```sh
  WID=$(for w in $(xdotool search --onlyvisible --name "." 2>/dev/null); do
          [ "$(xdotool getwindowname $w 2>/dev/null)" = "magic-tape" ] && echo $w; done | head -1)
  ```
- **To see exactly what mpv asks yt-dlp for**, put a shim earlier on `PATH` that logs argv and the
  `url` fields of the JSON, then `cat`s it through to the real binary. This is how §6.1.6 and §6.2.2
  were proven; it is faster and more reliable than reading logs after the fact.
- **Never `pkill -f "release/magic-tape"`.** The pattern matches the invoking shell's own command
  line and kills it. Record `$!` and `kill` the PID.
- **Screenshot *into* a transient state, don't try to catch it.** A loading state is gone in a
  second or two, so drive it and grab frames. `xdotool key r` (refresh) goes through the same
  `load_feed(feed, 0)` that the sidebar's feed rows do, which makes the clear in §6.3.17 checkable
  without guessing at coordinates: sample the content area for pixels that differ from the panel
  fill, before, during and after - one process sampling in a loop, because a shell round trip per
  frame is slower than the window being measured. The last run: 68.2% of the sampled pixels drawn,
  **0.1%** for the second the fetch took - nothing but the spinner and its caption, at y 474 in a
  panel spanning 115..834, whose centre is 474.5, and x 793.5 in a panel spanning 236..1360, centre
  798 (the 4.5px is the faint ellipsis at the end of the caption, not the layout) - then 66.2% once
  it landed. The block's bbox wobbled between samples (y 339, 330, 345), which is the spinner
  turning. Counting ink is also what caught the caption's placement: the first version stacked it
  at the top of the panel, y 120.
- Screenshot with `import -window <id> out.png` (ImageMagick is installed).

## 7. Change playbooks

**Add a feed type** — add a `Feed` variant in `model.rs` (it owns `path()`, `label()` and
`request_needs_extractor_args()`), a `feed_item` row in `ui/sidebar.rs`, a `Feed` match arm in
`ui/toolbar.rs::feed_controls`, and a `feed_label()` arm in `app.rs` if it should be replayable.

**Add a setting** — field in `Config` (it is `#[serde(default)]`, so old configs still load), a
widget in the matching `ui/settings.rs` tab, and if it affects launching, a field on `Player` fed
from `Config` in `Player::from_config`. `App::save_config` runs on window close.

**Change a keybinding** — the mapping is a single `match` in `main.rs::handle_keys`, then the
`Command` enum at the bottom of the file, then add the row to the shortcuts table in
`ui/settings.rs`. Note the "user is typing in a text field" guard: navigation commands are
allowed through it, action commands are not.

**Change the grid layout** — `ui/video_list.rs::grid` owns column count (currently
`(available / 260).clamp(1, 6)`), tile width, thumbnail aspect and row height. Cards are
`grid_card`, which allocates its own rect; keep the allocation or §6.3.4 returns. The count is
read from the central panel's width and nothing else: the detail pane is an overlay (§6.3.16), so
no part of this may depend on whether it is open. `grid()` logs a `[grid]` line whenever the count
changes, which is the quickest way to catch a layout that moves on its own.

## 8. Known limitations and open threads

- **Trending feed is dead upstream** (§6.1.5). The entry remains, with an explanatory error.
- **No retry on playback failure.** If mpv exits immediately the log shows why, but the app does
  not automatically retry with a different client. Worth adding if users hit it again.
- **The pinned client is a single value.** A real fallback chain (try `default`, then `tv`) would
  need a retry loop, since mpv accepts only one client per invocation.
- **Comment loading is slow** (3–5 s) and is triggered on selection; it runs on a worker but the
  detail pane shows a spinner meanwhile. Each video gets its own `~/.cache/magic-tape/comments/
  <video id>.info.json` (a single shared file let two quick selections clobber each other), and the
  directory is pruned to the newest 20 files after each load.
- **Notifications are best-effort** via `notify-rust`; no D-Bus fallback chain like the shell
  script's notify-send → kitty → osascript.
- **The quick-access panel lists every subscription**, virtualised with `show_rows` so hundreds
  of channels cost nothing per frame. The filter box appears past `FILTER_THRESHOLD` (15) as a
  convenience, not as the only way to reach a channel.
- **Thumbnails are unbounded in memory** (texture cache grows for the session); the on-disk cache
  is clearable from Settings → Maintenance.
- The two `#[ignore]`d live tests are the only automated playback coverage; there is no automated
  GUI test (no egui_kittest harness).

## 9. Regression coverage map

| Area | Tests |
| --- | --- |
| yt-dlp JSON shapes (real fixtures) | `parses_real_search_payload`, `inherits_channel_for_real_channel_feed_payload`, `parses_real_channel_search_payload` |
| Parsing edge cases | `parses_playlist_object_with_entries`, `parses_bare_array_and_ndjson`, `skips_null_entries`, `ignores_query_strings_on_thumbnails`, `parses_live_status_and_release_timestamp`, `empty_input_is_not_an_error` |
| mpv argument construction | `passes_format_cookies_and_client_to_mpv`, `drops_client_lists_because_mpv_cannot_pass_them` |
| Navigation maths | `page_step_is_one_screenful_minus_one_row`, `selection_clamps_at_both_ends`, `scroll_offset_centres_the_selected_row`, `select_requests_a_scroll_only_when_the_selection_changes` |
| Detail pane state | `a_selection_shows_the_detail_pane_and_clearing_it_hides_the_pane` |
| Feed loading | `a_load_from_the_top_empties_the_list_while_it_runs`, `loading_a_later_page_keeps_the_list_on_screen`, `an_answer_overtaken_by_a_newer_load_is_dropped` |
| Icons | `reads_a_setting_and_ignores_comments`, `inheritance_chains_are_split_and_trimmed`, `candidates_try_the_asked_size_first_and_scalable_last`, `sizeless_symbolic_directories_are_tried`, `an_unusual_size_still_gets_a_candidate`, `a_missing_theme_still_falls_back_to_the_toolkit_defaults`, `search_order_follows_inheritance`, `the_users_gtk_theme_wins_over_the_default` |
| Design tokens | `the_type_scale_is_ordered_from_caption_to_app`, `the_card_text_zone_holds_a_title_over_a_meta_line`, `a_selected_fill_stays_a_tint_of_the_accent`, `a_colour_setting_takes_the_forms_a_person_would_type`, `an_unusable_colour_setting_leaves_the_pane_on_the_theme` |
| Thumbnails | `reads_the_video_id_out_of_a_thumbnail_url`, `spotts_the_owner_only_variants`, `variants_are_the_same_video_sharpest_first`, `a_url_from_elsewhere_is_left_alone`, `a_failed_id_is_never_asked_for_again` |
| The feed has videos in it | `live_feed_has_videos`, `live_search_has_videos` (both opt-in: they need the network and a signed-in browser) |
| Fetch permits | `a_permit_is_returned_when_the_job_finishes`, `the_pool_holds_the_line_at_its_limit`, `raising_the_limit_lets_waiting_jobs_through`, `a_limit_of_zero_is_treated_as_one` |
| Cache retention | `thumbnails_are_pruned_oldest_first_and_only_when_needed`, `the_byte_budget_can_prune_before_the_count`, `comment_files_are_pruned_to_the_cap` |
| Cookie arguments | `a_signed_in_request_always_carries_browser_cookies`, `a_public_request_carries_no_cookies`, `an_auth_failure_is_told_apart_from_a_broken_video` |
| Comment fetching | `the_settle_delay_is_short_enough_to_feel_immediate`, `a_finished_fetch_queues_only_what_is_still_on_screen`, `paging_fast_never_queues_more_than_one_fetch`, `only_one_comment_fetch_is_ever_in_flight`, `a_video_is_identified_by_its_id_not_its_url` |
| Paging moves whole rows | `a_page_is_a_screenful_of_rows`, `paging_in_a_grid_moves_rows_not_items`, `paging_in_the_list_moves_one_row_per_item`, `paging_clamps_at_both_ends`, `paging_from_a_partial_last_row_stays_on_it` |
| Search does not hijack other feeds | `only_the_search_feed_carries_the_query`, `a_leftover_query_cannot_poison_another_feed`, `leaving_a_search_returns_to_the_previous_feed`, `a_search_inside_a_search_keeps_the_original_return_feed`, `an_empty_term_starts_nothing`, `leaving_a_search_from_another_feed_only_clears_the_term`, `a_term_only_searches_the_search_feed`, `a_stored_term_never_reaches_ytdlp_for_another_feed` |
| Download path resolution | `absolute_paths_are_used_as_they_are`, `relative_and_home_paths_resolve_against_home`, `trailing_slashes_and_blanks_are_tidied`, `the_old_style_default_is_migrated` |
| Font resolution | `resolves_by_name_and_by_path`, `accepts_a_path_that_exists`, `falls_back_to_a_prefix_match`, `display_name_drops_the_extension`, `zoom_is_clamped_to_something_usable` |
| Subscription cache & quick-access filter | `subscription_lookup_uses_the_cache`, `subscription_filter_matches_case_insensitively_and_sorts` |
| Comment metadata files | `video_stamp_is_per_video_and_path_safe`, `comment_files_are_pruned_to_the_cap` |
| Store round-trips | `likes_are_idempotent`, `subscriptions_roundtrip`, `history_entries_roundtrip`, `search_history_is_deduped_and_reversed` |
| Network / playback (opt-in) | `live_plays_page_url_through_mpv`, `live_lists_formats`, `live_downloads_into_an_absolute_directory` |

When a YouTube response format changes, `cargo test` should fail via the fixtures *before* a user
sees a broken list. If you regenerate them, keep the trimming and note the capture date in §6.1.
