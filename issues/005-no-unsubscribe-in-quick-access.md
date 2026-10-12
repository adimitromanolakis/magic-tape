# No unsubscribe on the sidebar's quick-access rows

Status: open

## What happens

The Channels grid has Unsubscribe on right-click. The sidebar's quick-access
rows - the list of every subscription - have no such option, so leaving a
channel means finding it in the Channels page first.

## Why it matters

The quick-access list is where you go to *find* channels. Making you remember
that unsubscribing lives somewhere else is exactly the friction the
subscriptions list exists to remove.

## What fixed looks like

Right-click a row: Unsubscribe, Copy channel name, Open channel. Same actions
as the Channels card, so the two places behave alike.

## Where

- `src/ui/sidebar.rs` - `subscriptions_panel`
- `src/app.rs` - `unsubscribe`
