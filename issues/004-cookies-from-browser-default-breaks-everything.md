# `cookies_from_browser` defaults to firefox, and a missing browser kills every request

Status: open

## What happens

The default config asks for Firefox cookies. If Firefox is not installed, every
yt-dlp call fails with

```
ERROR: [youtube] could not find firefox cookies database in ...
```

and the app looks completely broken: no feed, no search, no channels, only red
toasts. Nothing degrades gracefully, because the cookie flag is passed
unconditionally.

## Why it matters

This is the default, so it hits anyone who has Chrome or Firefox-flatpak but no
Mozilla Firefox. It is also the single most confusing failure the app can have,
because the error names a browser the user may not know is configured.

## What fixed looks like

1. Default to no cookies, which works for public content.
2. If the configured browser has no usable profile, retry once without the
   cookie flag and carry on, rather than failing the request.
3. Say so once - a warning is enough, a toast per request is not.

## Where

- `src/config.rs` - the default
- `src/yt.rs` - where `--cookies-from-browser` is added to the argument list
