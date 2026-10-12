# Download quality is a config string

Status: open

## What happens

Download quality is `download_video_quality = "bv*+ba/b"` in the config file -
yt-dlp format syntax, edited by hand. The hint text offers examples, but the
user is expected to know the grammar.

## Why it matters

It is the one setting most people would want to change, and it is the one that
requires the most knowledge. Playback already has a friendly picker; downloads
do not.

## What fixed looks like

A menu of presets next to the field, which write the syntax for you:

- Best available
- 1080p
- 720p
- 480p
- Audio only

The raw field stays, for anyone who wants the full syntax. The preset must show
which one is currently in force, including when the field holds something the
preset list does not cover.

## Where

- `src/ui/settings.rs` - Playback tab
- `src/app.rs` - `download_selected`
