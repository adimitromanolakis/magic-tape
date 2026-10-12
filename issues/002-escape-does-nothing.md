# Escape is not handled anywhere

Status: open

## What happens

There is no `Key::Escape` handler in the entire app. Escape does nothing:
settings stay open, the shortcuts window stays open, a search keeps its results
on screen.

## Why it matters

Escape is the universal "get me out of this" key. Its absence is the single
most-noticed gap when moving from a TUI to a GUI, and it is what a user reaches
for first after the search results are showing.

## What fixed looks like

One handler, in priority order:

1. a settings window or shortcuts window is open -> close it
2. search results are showing -> `leave_search()`, the same thing the Back
   button does
3. the sidebar filter is filled -> clear it
4. otherwise -> drop the current selection

## Where

- `src/main.rs` - `handle_keys`, alongside the other commands
