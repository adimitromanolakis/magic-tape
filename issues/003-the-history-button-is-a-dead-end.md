# The History button fills one term and stops

Status: open

## What happens

The History button in the toolbar takes the *first* entry from the stored
search history, drops it into the search box, and stops. There is no list, so
the other terms are unreachable from the UI.

## Why it matters

Search history is only useful if you can go back to a term you used before.
The shell version had a whole `j` menu for this. Right now it is a single slot
with extra steps.

## What fixed looks like

A popup listing the stored terms, newest first, each one clickable to search.
Terms should be selectable across, and there should be a way to remove one and a
way to clear the lot - the store already supports clearing it from
Settings -> Maintenance.

## Where

- `src/ui/toolbar.rs` - the button
- `src/store.rs` - `search_history()`
- `src/app.rs` - `start_search`, for the click-through
