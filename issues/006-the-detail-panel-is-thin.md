# The detail panel is thin

Status: open

## What happens

The detail panel shows a selected video's description and comments. There is no
channel header - no avatar, no name, no subscriber count - and nothing that
takes you from a video to its channel or to the channel's other videos.

## Why it matters

A video is mostly a way to reach its channel. Without a jump, going from a
video to more of the same means going back to the sidebar, finding the channel,
and clicking it - the search box reset bug of the navigation.

## What fixed looks like

- channel header at the top of the panel: avatar, name, subscriber count
- click it to open the channel feed, which the search fix already makes safe
- keep the existing description and comments below it

## Where

- `src/ui/detail.rs`
- `src/app.rs` - `open channel`, shared with the Channels grid and sidebar
