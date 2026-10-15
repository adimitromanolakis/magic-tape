use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::Config;
use crate::model::{self, Channel, Comment, DurationFilter, Feed, Video};

#[derive(Clone)]
pub struct YtDlp {
    binary: String,
    browser: String,
}

pub struct Listing {
    pub videos: Vec<Video>,
}

impl YtDlp {
    pub fn new(config: &Config) -> Self {
        Self {
            binary: config.ytdlp_binary.clone(),
            browser: config.cookies_from_browser.clone(),
        }
    }

    /// The cookie arguments a signed-in request carries.
    ///
    /// Straight from the browser, every time. Exporting them to a file was
    /// tried and reverted: it saved about 0.2s per call, and yt-dlp's browser
    /// reader reaches cookies a static export drops - notably YouTube's own
    /// SID/HSID/SSID, without which the subscriptions feed answers with an
    /// empty playlist and exit code 0.
    fn cookie_args(&self) -> Vec<String> {
        if self.browser.trim().is_empty() {
            Vec::new()
        } else {
            vec![
                "--cookies-from-browser".into(),
                self.browser.trim().to_string(),
            ]
        }
    }

    pub fn describe(&self) -> String {
        let version = self.version();
        let browser = if self.browser.trim().is_empty() {
            "none".to_string()
        } else {
            self.browser.clone()
        };
        let resolved = resolve_binary(&self.binary);
        format!(
            "yt-dlp {version} (binary: {}{}, cookies-from-browser: {browser})",
            self.binary,
            match &resolved {
                Some(path) => format!(" -> {}", path.display()),
                None => String::new(),
            }
        )
    }

    pub fn available(&self) -> bool {
        let argv = ["--version".to_string()];
        log::info!("[yt-dlp] availability check :: {} {}", self.binary, quote(&argv));
        let available = Command::new(&self.binary)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        log::info!("[yt-dlp] availability check :: found={available}");
        available
    }

    pub fn version(&self) -> String {
        self.run_output("version", &["--version"])
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|_| "unavailable".into())
    }

    pub fn update(&self) -> Result<String> {
        self.run_output("update", &["-U"]).context("updating yt-dlp")
    }

    pub fn subscriptions(&self) -> Result<Vec<Channel>> {
        let out = self.run_output(
            "import-subscriptions",
            &["--flat-playlist", "-J", "https://www.youtube.com/feed/channels"],
        )?;
        Ok(model::parse_channels(&out))
    }

    pub fn list_feed(
        &self,
        feed: Feed,
        channel: &str,
        query: Option<&str>,
        filter: DurationFilter,
        start: usize,
        count: usize,
    ) -> Result<Listing> {
        let first = start + 1;
        let last = start + count;
        let path = feed_path(feed, channel, query, filter);
        let mut args: Vec<String> = vec!["--flat-playlist".into()];
        if feed.request_needs_extractor_args() && query.is_none() {
            args.push("--extractor-args".into());
            args.push("youtubetab:approximate_date".into());
        }
        args.push("--playlist-start".into());
        args.push(first.to_string());
        args.push("--playlist-end".into());
        args.push(last.to_string());
        args.push("-J".into());
        args.push(format!("https://www.youtube.com{path}"));

        // Subscriptions and trending need the account; everything else is
        // public and costs about a second less without cookies.
        let auth = match feed {
            Feed::Subscriptions | Feed::Trending | Feed::Recommended => Auth::SignedIn,
            Feed::Search | Feed::Channel | Feed::Repeat => Auth::Public,
        };
        let out = self
            .run_with_auth("list-feed", &args, None, auth)
            .map_err(|error| explain(feed, error))?;
        if auth == Auth::SignedIn && !out.contains("\"entries\"") {
            // Anonymous requests to the subscriptions feed come back as an
            // empty playlist rather than an error, which is how a broken
            // cookie setup looks like "no new videos".
            log::warn!(
                "[yt-dlp] list-feed :: {} returned no entries while signed in - check your {} cookies",
                feed.label(),
                self.browser
            );
        }
        Ok(Listing {
            videos: model::parse_videos(&out),
        })
    }

    pub fn list_playlists(&self, query: &str, start: usize, count: usize) -> Result<Vec<Video>> {
        let args = vec![
            "--flat-playlist".to_string(),
            "--playlist-start".to_string(),
            (start + 1).to_string(),
            "--playlist-end".to_string(),
            (start + count).to_string(),
            "-J".to_string(),
            format!(
                "https://www.youtube.com/results?search_query={}{}",
                urlencode(query),
                DurationFilter::Playlists.query_value()
            ),
        ];
        let out = self.run_public("list-playlists", &args, None)?;
        Ok(model::parse_videos(&out))
    }

    pub fn search_channels(&self, query: &str, start: usize, count: usize) -> Result<Vec<Channel>> {
        let args = vec![
            "--flat-playlist".to_string(),
            "--playlist-start".to_string(),
            (start + 1).to_string(),
            "--playlist-end".to_string(),
            (start + count).to_string(),
            "-J".to_string(),
            format!(
                "https://www.youtube.com/results?search_query={}&sp=EgIQAg%253D%253D",
                urlencode(query)
            ),
        ];
        let out = self.run_public("search-channels", &args, None)?;
        let channels = model::parse_channels(&out);
        if channels.is_empty() {
            bail!("no channels found for \"{query}\"")
        } else {
            Ok(channels)
        }
    }

    pub fn comments_from_cache(
        url: &str,
        comments_dir: &Path,
    ) -> Option<(String, Vec<Comment>, u64)> {
        let path = comments_dir.join(video_stamp(url)).with_extension("info.json");
        let json = std::fs::read_to_string(path).ok()?;
        let (description, comments) = model::parse_info_json(&json);
        if description.is_empty() && comments.is_empty() {
            return None;
        }
        let count = model::comment_count(&json);
        Some((description, comments, count))
    }

    pub fn video_metadata(&self, url: &str, comments_dir: &Path, max: usize, replies: usize, sort: &str) -> Result<(String, Vec<Comment>, u64)> {
        let extractor = format!(
            "youtube:max_comments={max},all,{replies};comment_sort={sort}"
        );
        // One file per video: a single shared template let two comment loads
        // (fast enough arrowing through the grid) delete and rewrite each
        // other's output.
        let stamp = video_stamp(url);
        let template = comments_dir.join(&stamp);
        let _ = std::fs::remove_file(template.with_extension("info.json"));
        // Ordinary videos are public; an age-restricted one falls back to the
        // signed-in path by itself.
        self.run_public("video-metadata", &[
            "-q",
            "--write-comments",
            "--no-download",
            "--extractor-args",
            &extractor,
            "--write-info-json",
            url,
            "-o",
            &template.to_string_lossy(),
        ], None)?;
        let path = template.with_extension("info.json");
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (description, comments) = model::parse_info_json(&json);
        let count = model::comment_count(&json);
        prune_comment_files(comments_dir, COMMENT_CACHE_KEEP);
        Ok((description, comments, count))
    }

    pub fn download(&self, url: &str, dir: &Path, audio_only: bool) -> Result<()> {
        std::fs::create_dir_all(dir).ok();
        let mut args: Vec<String> = Vec::new();
        if audio_only {
            args.push("--extract-audio".into());
            args.push("--audio-quality".into());
            args.push("0".into());
            args.push("--embed-thumbnail".into());
        }
        args.push(url.into());
        self.run_output_in("download", &args, dir).map(|_| ())
    }

    fn run_output<S: AsRef<std::ffi::OsStr>>(&self, label: &str, args: &[S]) -> Result<String> {
        self.run_request(label, args, None, Auth::SignedIn)
    }

    fn run_with_auth<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: Option<&Path>,
        auth: Auth,
    ) -> Result<String> {
        self.run_request(label, args, dir, auth)
    }

    fn run_output_in<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: &Path,
    ) -> Result<String> {
        self.run_request(label, args, Some(dir), Auth::SignedIn)
    }

    /// A request that anyone may make, run without cookies first.
    ///
    /// Measured on this machine: a cookie-free search is 0.9s against 3.2s with
    /// cookies, and returns the same results. The cost is not reading the
    /// browser's store - it is *carrying* cookies, which puts the request on
    /// YouTube's authenticated path. So public work goes cookie-free and only
    /// falls back when YouTube says it needs a login.
    fn run_public<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: Option<&Path>,
    ) -> Result<String> {
        self.run_request(label, args, dir, Auth::Public)
    }

    /// Runs yt-dlp with cookies, and on an authentication failure refreshes the
    /// cookie file and tries exactly once more.
    ///
    /// One retry, not a loop: if a refreshed cookie file is still rejected,
    /// the user is told, because the cause is on their side (the browser
    /// logged out, or the wrong browser is configured) and retrying would only
    /// hammer YouTube.
    fn run_request<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: Option<&Path>,
        auth: Auth,
    ) -> Result<String> {
        if auth == Auth::Public {
            match self.invoke_with(label, args, dir, false) {
                Ok(stdout) => return Ok(stdout),
                Err(message) if !looks_like_auth_failure(&message) => bail!("{message}"),
                Err(message) => {
                    log::info!(
                        "[yt-dlp] {label} :: needs a login ({message}), retrying with cookies"
                    );
                }
            }
        }
        self.run_signed_in(label, args, dir)
    }

    /// The signed-in path: the exported file if we have one, the browser
    /// otherwise, then exactly one refresh-and-retry on an auth failure.
    fn run_signed_in<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: Option<&Path>,
    ) -> Result<String> {
        // Always signed in on the first attempt. Gating this on "do we have an
        // exported file" sent the request out *anonymously* on a cold start,
        // and the subscriptions feed answers an anonymous request with zero
        // entries and no error - so the app just looked empty.
        match self.invoke_with(label, args, dir, true) {
            Ok(stdout) => return Ok(stdout),
            Err(message) if !looks_like_auth_failure(&message) => bail!("{message}"),
            Err(_) => {}
        }

        // Retry once: Chrome being locked or mid-write can refuse a read that
        // works a moment later.
        std::thread::sleep(std::time::Duration::from_millis(400));
        match self.invoke_with(label, args, dir, true) {
            Ok(stdout) => {
                log::info!("[yt-dlp] {label} :: succeeded on the retry");
                Ok(stdout)
            }
            Err(retry) if looks_like_auth_failure(&retry) => bail!(
                "YouTube refused the {} cookies for this. Check that you are signed in to YouTube in that browser.",
                self.browser
            ),
            Err(retry) => Err(anyhow::anyhow!("{retry}")),
        }
    }

    /// One yt-dlp invocation, returning its stdout or its last stderr line.
    fn invoke_with<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: Option<&Path>,
        use_cookies: bool,
    ) -> Result<String, String> {
        let mut cmd = Command::new(&self.binary);
        if use_cookies {
            cmd.args(self.cookie_args());
        }
        cmd.args(args);
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        match run(cmd, &self.binary, label) {
            Ok(stdout) => Ok(stdout),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// Resolves a bare binary name against PATH so the log shows which yt-dlp is
/// really running - useful when several copies are installed.
pub fn resolve_binary(binary: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(binary);
    if direct.components().count() > 1 {
        return direct.is_file().then_some(direct);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(binary))
        .find(|candidate| candidate.is_file())
}

pub fn quote(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.is_empty() || arg.contains([' ', '\t', '\n', '"', '\'']) {
                format!("{arg:?}")
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn run(mut cmd: Command, binary: &str, label: &str) -> Result<String> {
    let argv: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    let cwd = cmd
        .get_current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_else(|| "<inherited>".into());
    log::info!("[yt-dlp] {label} :: {binary} {}", quote(&argv));
    log::debug!("[yt-dlp] {label} :: cwd={cwd}");

    let started = std::time::Instant::now();
    let output = cmd.output().with_context(|| {
        format!("could not run `{binary}` - install yt-dlp and make sure it is on PATH")
    })?;
    let elapsed = started.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    log::info!(
        "[yt-dlp] {label} :: exit={} in {:.2}s, stdout={}B, stderr={}B",
        output.status,
        elapsed.as_secs_f64(),
        stdout.len(),
        stderr.len()
    );
    for line in stderr.lines().filter(|l| !l.trim().is_empty()) {
        log::info!("[yt-dlp] {label} :: stderr: {line}");
    }
    if !output.status.success() {
        let message = stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("yt-dlp failed")
            .trim();
        log::error!("[yt-dlp] {label} :: FAILED: {message}");
        bail!("{message}")
    }
    Ok(stdout.into_owned())
}

/// Whether a request may be made by anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// Public data - search, channel pages, playlists, comments on ordinary
    /// videos. Try without cookies, because carrying them is the expensive
    /// part, and fall back only if YouTube insists.
    Public,
    /// Needs the account: subscriptions, trending, downloads.
    SignedIn,
}

/// Whether an error means "your cookies are no good" rather than "this video
/// is broken".
///
/// Worth being generous: re-exporting costs one slow call, and a wrong guess
/// only makes that call run twice.
pub fn looks_like_auth_failure(message: &str) -> bool {
    const MARKERS: [&str; 8] = [
        "sign in to confirm",
        "login required",
        "private video",
        "confirm you're not a bot",
        "members-only",
        "this video is available to this channel's members",
        "cookies",
        "use --cookies-from-browser",
    ];
    let lower = message.to_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

fn explain(feed: Feed, error: anyhow::Error) -> anyhow::Error {
    let message = error.to_string();
    if matches!(feed, Feed::Trending)
        && (message.contains("trending") || message.contains("does not exist"))
    {
        return anyhow::anyhow!(
            "YouTube retired the Trending feed, so this list can no longer be loaded. Try a search or the For you feed."
        );
    }
    error
}

/// How many videos' comments to keep.
///
/// This used to be 20, which meant the 21st video evicted the first: come back
/// to anything you had already watched and it cost another yt-dlp run, about
/// three seconds, to read comments that had been on disk an hour ago.
pub const COMMENT_CACHE_KEEP: usize = 1200;

/// A ceiling on the comment cache, so "keep 1200 videos" cannot mean "use up
/// the disk" if a video ever had an enormous thread.
pub const COMMENT_CACHE_BYTES: u64 = 512 * 1024 * 1024;

/// Keeps the most recent comment files and deletes the rest: one file per video
/// would otherwise grow without bound in the cache directory.
/// Deletes the least recently used comment files until both the count and the
/// byte budget are satisfied.
///
/// Sorted oldest first, so what survives is what was read most recently.
fn prune_comment_files(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            Some((modified, meta.len(), e.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
    // Oldest first, but never past the count we are willing to keep.
    let droppable = files.len() - keep;
    let mut dropped = 0usize;
    for (_, size, path) in &files {
        let over_count = dropped < droppable;
        let over_bytes = bytes > COMMENT_CACHE_BYTES;
        if !over_count && !over_bytes {
            break;
        }
        bytes = bytes.saturating_sub(*size);
        std::fs::remove_file(path).ok();
        dropped += 1;
    }
    if dropped > 0 {
        log::info!("[yt-dlp] comment cache :: pruned {dropped} old files");
    }
}

/// A filesystem-safe, collision-free stem for a video's metadata file.
///
/// Uses the `v=` parameter where present so repeated loads of the same video
/// reuse one file, and falls back to a hash of the URL otherwise.
fn video_stamp(url: &str) -> String {
    let id = url
        .split(['?', '&'])
        .find_map(|part| part.strip_prefix("v="))
        .filter(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        });
    match id {
        Some(id) => id.to_string(),
        None => {
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in url.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            format!("url-{hash:016x}")
        }
    }
}

/// The YouTube path a feed request asks for.
///
/// A term turns the request into a search - but only for the search feed. The
/// obvious, wrong version of this match is `(Some(q), _) =>`, which sent *any*
/// feed to `/results` whenever a query happened to be stored.
pub fn feed_path(
    feed: Feed,
    channel: &str,
    query: Option<&str>,
    filter: DurationFilter,
) -> String {
    match (query, feed) {
        (Some(q), Feed::Search) => format!(
            "/results?search_query={}{}",
            urlencode(q),
            filter.query_value()
        ),
        (_, feed) => feed.path(channel),
    }
}

pub fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_search_queries() {
        assert_eq!(urlencode("hello world"), "hello%20world");
        assert_eq!(urlencode("a+b&c"), "a%2Bb%26c");
        assert_eq!(urlencode("plain"), "plain");
    }

    #[test]
    fn comment_files_are_pruned_to_the_cap() {
        let dir = std::env::temp_dir().join(format!(
            "mt-comments-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..8 {
            std::fs::write(dir.join(format!("video{i}.info.json")), "{}").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        prune_comment_files(&dir, 3);
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 3, "the cap is respected");
        assert!(
            left.contains(&"video7.info.json".to_string()),
            "the newest files are the ones kept: {left:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn video_stamp_is_per_video_and_path_safe() {
        let a = video_stamp("https://www.youtube.com/watch?v=5C_HPTJg5ek");
        assert_eq!(a, "5C_HPTJg5ek");
        assert_ne!(
            a,
            video_stamp("https://www.youtube.com/watch?v=abcdefghijk"),
            "two videos must not share a metadata file"
        );
        assert_eq!(
            video_stamp("https://www.youtube.com/watch?app=desktop&v=5C_HPTJg5ek"),
            a,
            "the v= parameter is found wherever it sits in the query"
        );
        let fallback = video_stamp("https://www.youtube.com/shorts/xyz");
        assert!(fallback.starts_with("url-"));
        assert!(fallback.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        assert_ne!(fallback, video_stamp("https://www.youtube.com/shorts/abc"));
    }

    #[test]
    fn decodes_roundtrip() {
        assert_eq!(urldecode(&urlencode("cats & dogs?v")), "cats & dogs?v");
        assert_eq!(urldecode("a+b"), "a b");
    }
}
    #[test]
    fn a_term_only_searches_the_search_feed() {
        // The regression this guards: `(Some(q), _)` in the match meant that
        // clicking a channel after searching kept returning search results.
        for (feed, channel, expected) in [
            (Feed::Subscriptions, "", "/feed/subscriptions"),
            (Feed::Trending, "", "/feed/trending"),
            (Feed::Channel, "UC123", "/channel/UC123/videos"),
            (Feed::Channel, "@handle", "/@handle/videos"),
        ] {
            assert_eq!(
                feed_path(feed, channel, Some("rust"), DurationFilter::Any),
                expected,
                "{feed:?} must not be hijacked by a leftover term"
            );
        }
        assert_eq!(
            feed_path(Feed::Search, "", Some("rust lang"), DurationFilter::Any),
            format!(
            "/results?search_query=rust%20lang{}",
            DurationFilter::Any.query_value()
        )
        );
        assert_eq!(
            feed_path(Feed::Search, "", Some("rust"), DurationFilter::Long),
            format!("/results?search_query=rust{}", DurationFilter::Long.query_value())
        );
    }

    /// A stand-in for yt-dlp that records the arguments it was given.
    #[cfg(test)]
    fn recording_binary(dir: &Path, record: &Path) -> String {
        let script = dir.join("record-yt-dlp");
        let body = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\necho '{{}}'\n",
            record.display()
        );
        std::fs::write(&script, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        script.display().to_string()
    }

    #[test]
    fn a_signed_in_request_always_carries_browser_cookies() {
        // No exported jar: yt-dlp's browser reader is the only thing that
        // reaches YouTube's SID/HSID/SSID, and a static export silently drops
        // them - which makes the subscriptions feed answer with an empty
        // playlist and exit code 0.
        let dir = std::env::temp_dir().join(format!("mt-cookie-args-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("args.txt");
        let script = recording_binary(&dir, &record);
        let mut config = test_config();
        config.ytdlp_binary = script;
        config.cookies_from_browser = "chrome".into();
        let yt = YtDlp::new(&config);

        let _ = yt.list_feed(Feed::Subscriptions, "", None, DurationFilter::Any, 0, 5);
        let args = std::fs::read_to_string(&record).unwrap();
        assert!(
            args.contains("--cookies-from-browser") && args.contains("chrome"),
            "a signed-in request must always carry browser cookies: {args}"
        );
        assert!(
            !args.contains("--cookies "),
            "and never a file: {args}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_public_request_carries_no_cookies() {
        // The speed win depends on this: search must go out anonymously.
        let dir = std::env::temp_dir().join(format!("mt-public-args-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("args.txt");
        let script = recording_binary(&dir, &record);
        let mut config = test_config();
        config.ytdlp_binary = script;
        config.cookies_from_browser = "chrome".into();
        let yt = YtDlp::new(&config);

        let _ = yt.list_feed(Feed::Search, "", Some("rust"), DurationFilter::Any, 0, 5);
        let args = std::fs::read_to_string(&record).unwrap();
        assert!(
            !args.contains("--cookies"),
            "a search must not carry cookies: {args}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_auth_failure_is_told_apart_from_a_broken_video() {
        for message in [
            "ERROR: [youtube] x: Sign in to confirm you're not a bot",
            "ERROR: [youtube] x: Private video. Sign in if you've been granted access",
            "ERROR: [youtube] x: This video is available to this channel's members",
            "ERROR: [youtube] x: login required",
            "ERROR: use --cookies-from-browser or --cookies for this",
        ] {
            assert!(
                looks_like_auth_failure(message),
                "should be treated as an auth problem: {message}"
            );
        }
        for message in [
            "ERROR: [youtube] x: This video is unavailable",
            "ERROR: [youtube] x: Video unavailable. This video is no longer available",
            "ERROR: [youtube] x: Private video. Sign in if you've been granted access", // handled above
        ] {
            if message.contains("Sign in") {
                continue;
            }
            assert!(
                !looks_like_auth_failure(message),
                "should not cost a cookie refresh: {message}"
            );
        }
        assert!(!looks_like_auth_failure(""));
    }

    /// Opt-in: times a public request against a signed-in one through the real
    /// code path, which is the whole point of the split.
    /// Run with: `cargo test -- --ignored --nocapture live_public_vs_signed_in`
    #[test]
    #[ignore = "needs the network"]
    fn live_public_vs_signed_in() {
        crate::yt::live::init_logging();
        let mut config = test_config();
        config.cookies_from_browser =
            std::env::var("MT_BROWSER").unwrap_or_else(|_| "chrome".into());
        let yt = YtDlp::new(&config);

        let secs = |d: std::time::Duration| d.as_secs_f64();
        let time_it = |auth: Auth| {
            let started = std::time::Instant::now();
            let result = yt.run_with_auth(
                "timing",
                &[
                    "--flat-playlist",
                    "--playlist-end",
                    "5",
                    "-J",
                    "https://www.youtube.com/results?search_query=rust&sp=EgQQARgE",
                ],
                None,
                auth,
            );
            (started.elapsed(), result.is_ok())
        };
        let (public_time, public_ok) = time_it(Auth::Public);
        let (signed_in_time, signed_ok) = time_it(Auth::SignedIn);
        println!(
            "search, public (no cookies): {:.2}s ok={public_ok}\nsearch, signed in:          {:.2}s ok={signed_ok}",
            secs(public_time),
            secs(signed_in_time)
        );
    }

    #[test]
    fn a_stored_term_never_reaches_ytdlp_for_another_feed() {
        // Drives the real subprocess plumbing with a stand-in for yt-dlp that
        // records the URL it was asked for, so the regression is caught even if
        // somebody reorders the arguments above.
        let dir = std::env::temp_dir().join("magic-tape-fake-ytdlp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let record = dir.join("requested.txt");
        let script = dir.join("fake-yt-dlp");
        // A stand-in that appends the URL it was handed to a file.
        let body = "#!/bin/sh\nfor a in \"$@\"; do\n  case \"$a\" in\n    https://*) echo \"$a\" >> __RECORD__ ;;\n  esac\ndone\necho '{}'\n";
        std::fs::write(&script, body.replace("__RECORD__", &record.display().to_string())).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let mut config = test_config();
        config.ytdlp_binary = script.display().to_string();
        let yt = YtDlp::new(&config);

        yt.list_feed(Feed::Search, "", Some("rust"), DurationFilter::Any, 0, 5)
            .unwrap();
        yt.list_feed(Feed::Channel, "UC123", Some("rust"), DurationFilter::Any, 0, 5)
            .unwrap();
        yt.list_feed(Feed::Trending, "", Some("rust"), DurationFilter::Any, 0, 5)
            .unwrap();
        yt.list_feed(Feed::Channel, "UC123", None, DurationFilter::Any, 0, 5)
            .unwrap();

        let asked = std::fs::read_to_string(&record).unwrap();
        let urls: Vec<&str> = asked.lines().collect();
        assert_eq!(urls.len(), 4, "one request per call: {asked}");
        assert_eq!(
            urls[0],
            format!(
                "https://www.youtube.com/results?search_query=rust{}",
                DurationFilter::Any.query_value()
            )
        );
        assert_eq!(urls[1], "https://www.youtube.com/channel/UC123/videos");
        assert_eq!(urls[2], "https://www.youtube.com/feed/trending");
        assert_eq!(urls[3], "https://www.youtube.com/channel/UC123/videos");
        std::fs::remove_dir_all(&dir).ok();
    }

#[cfg(test)]
fn test_config() -> Config {
    Config {
        cookies_from_browser: String::new(),
        notification_enabled: false,
        mpv_log_level: std::env::var("MT_MPV_LOG").unwrap_or_else(|_| "all=warn".into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod live {
    use super::*;

    fn home() -> std::path::PathBuf {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    }

    pub(super) fn init_logging() {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .is_test(false)
        .try_init();
    }

    /// Opt-in end-to-end playback check: hands the page URL to the same
    /// `Player` the UI uses, so mpv's diagnostics land in the log.
    /// Run with: `cargo test -- --ignored --nocapture live_`
    #[test]
    #[ignore = "requires network access and mpv"]
    fn live_plays_page_url_through_mpv() {
        init_logging();
        let config = test_config();
        let player = crate::player::Player::from_config(&config);
        let url = "https://www.youtube.com/watch?v=5C_HPTJg5ek";
        for profile in [
            "bv*[height<=720]+ba/b[height<=720] / wv*+ba/w",
            "bv*[height<=1080]+ba/b[height<=1080] / wv*+ba/w",
        ] {
            println!("--- playing with {profile} ---");
            player.play(url, profile).expect("mpv should launch");
            std::thread::sleep(std::time::Duration::from_secs(8));
        }
        println!("done - check the [mpv] log lines for VO/AO and errors");
    }

    /// Opt-in end-to-end check that a download honours an absolute directory
    /// such as `/tmp` or `/mnt` instead of nesting it under `$HOME`.
    #[test]
    #[ignore = "requires network access and yt-dlp"]
    fn live_downloads_into_an_absolute_directory() {
        init_logging();
        let config = test_config();
        let yt = YtDlp::new(&config);
        let target = std::path::Path::new("/tmp/magic-tape-download-test");
        std::fs::remove_dir_all(target).ok();
        let resolved = crate::config::resolve_download_dir("/tmp/magic-tape-download-test", &home());
        assert_eq!(resolved, target, "an absolute path must be kept as it is");

        yt.download(
            "https://www.youtube.com/watch?v=5C_HPTJg5ek",
            &resolved,
            false,
        )
        .expect("the download should succeed");

        let files: Vec<_> = std::fs::read_dir(target)
            .expect("the download directory should have been created")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!files.is_empty(), "yt-dlp should have written a file");
        println!("downloaded into {}: {files:?}", target.display());
        std::fs::remove_dir_all(target).ok();
    }

    /// Opt-in check that mpv can list the formats for a real video.
    #[test]
    #[ignore = "requires network access"]
    fn live_lists_formats() {
        init_logging();
        let yt = YtDlp::new(&test_config());
        let out = yt
            .run_output(
                "list-formats",
                &["-F", "https://www.youtube.com/watch?v=5C_HPTJg5ek"],
            )
            .expect("yt-dlp -F should work");
        assert!(out.contains("1920x1080"), "expected a 1080p format");
    }
}
