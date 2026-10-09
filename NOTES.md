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

State at the time of writing: `v0.2.0`, 16 files (10 modules + 6 UI modules), 32 unit tests + 2
opt-in network tests, zero clippy warnings. Tracked in git; tag `v0.2.0` marks the first working
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
      │  ui()     → Panel::left(sidebar) / Panel::right(detail) / bottom(status) / CentralPanel
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
| `src/main.rs` | 352 | Window/panel composition, CLI args, key handling, selection movement |
| `src/app.rs` | 1082 | Application state, actions (play/like/subscribe/download), `Outcome` handling, navigation helpers |
| `src/model.rs` | 643 | `Video`/`Channel`/`Comment` types + **all** yt-dlp JSON parsing, time/number formatting |
| `src/yt.rs` | 411 | The yt-dlp command line; one `run()` that logs everything; per-video comment files; URL encode/decode |
| `src/player.rs` | 293 | mpv invocation, log forwarding, exit-status watcher, notifications, `opener` |
| `src/store.rs` | 366 | Watch history, liked, search history, last action, subscriptions, thumb paths |
| `src/tasks.rs` | 114 | `Outcome` enum, `Worker` (thread + mpsc), thumbnail download |
| `src/thumbs.rs` | 74 | Thumbnail texture cache, placeholder, in-flight de-duplication |
| `src/config.rs` | 323 | `Config` (TOML, `#[serde(default)]`), `Paths`, legacy `magic-tape.conf` import |
| `src/ui/sidebar.rs` | 248 | Nav, feed list, subscription quick access, replay-previous-action |
| `src/ui/toolbar.rs` | 228 | Two-row toolbar + `paging_controls()` (shared with the list footer) |
| `src/ui/video_list.rs` | 427 | Virtualised grid, compact list, channel list, context menus, empty state |
| `src/ui/detail.rs` | 338 | Right panel: thumbnail, action buttons, description, comments |
| `src/ui/settings.rs` | 396 | Settings window (4 tabs) + shortcuts window |
| `src/ui/toasts.rs` | 54 | Toast overlay (anchored below the toolbar) + status bar |
| `src/ui/mod.rs` | 60 | Colors, `card()`, text measurement and `ellipsize()` |

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
`grid_card`, which allocates its own rect; keep the allocation or §6.3.4 returns.

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
- **The quick-access panel draws at most 200 rows** and relies on its filter box beyond that
  (`FILTER_THRESHOLD` / `MAX_QUICK_ACCESS_ROWS` in `ui/sidebar.rs`). The Channels section lists
  everything; virtualising the sidebar list would be the next step.
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
| Subscription cache & quick-access filter | `subscription_lookup_uses_the_cache`, `subscription_filter_matches_case_insensitively_and_sorts` |
| Comment metadata files | `video_stamp_is_per_video_and_path_safe`, `comment_files_are_pruned_to_the_cap` |
| Store round-trips | `likes_are_idempotent`, `subscriptions_roundtrip`, `history_entries_roundtrip`, `search_history_is_deduped_and_reversed` |
| Network / playback (opt-in) | `live_plays_page_url_through_mpv`, `live_lists_formats` |

When a YouTube response format changes, `cargo test` should fail via the fixtures *before* a user
sees a broken list. If you regenerate them, keep the trimming and note the capture date in §6.1.
