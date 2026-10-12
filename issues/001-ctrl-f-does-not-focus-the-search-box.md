# Ctrl+F clears the search box but never focuses it

Status: fixed

## What happens

Pressing `Ctrl+F` clears whatever is in the search box and that is all it does.
There is no `request_focus` anywhere in the codebase, so the caret does not move
into the field. The user has to click the box before typing, and anything they
had typed is gone for nothing.

## Why it matters

The shortcut exists to save a mouse trip. As written it saves nothing and
loses input. It is also the only advertised way to reach the search box from
anywhere in the app.

## What fixed looks like

`Ctrl+F` focuses the main search box and selects its contents, so typing
replaces what is there. If the user is in another section, it switches to
Feeds first, because that is where the main search box lives.

Escape should then return focus to the list - see
[002](002-escape-does-nothing.md).

## Fix

`Ctrl+F` now clears the box, switches to Feeds if another section is open, and
focuses the field. The toolbar owns the focus request through
`App::focus_search`, because `request_focus` is a request for the *next* frame
and asking once from the key handler is not enough.

## Where

- `src/main.rs` - `Command::FocusSearch` handling in `handle_keys`
- `src/ui/toolbar.rs` - the `TextEdit` needs a stable `id_source` so it can be
  focused by id
- `src/ui/mod.rs` - a good place for the shared id and a `focus_search_input`
  helper, so the shortcut and the widget cannot drift apart
