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
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "unavailable".into())
    }

    pub fn update(&self) -> Result<String> {
        self.run_output("update", &["-U"]).context("updating yt-dlp")
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
        let path = match (query, feed) {
            (Some(q), _) => format!(
                "/results?search_query={}{}",
                urlencode(q),
                filter.query_value()
            ),
            (None, feed) => feed.path(channel),
        };
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

        let out = self.run_output("list-feed", &args).map_err(|error| explain(feed, error))?;
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
        let out = self.run_output("list-playlists", &args)?;
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
        let out = self.run_output("search-channels", &args)?;
        let channels = model::parse_channels(&out);
        if channels.is_empty() {
            bail!("no channels found for \"{query}\"")
        } else {
            Ok(channels)
        }
    }

    pub fn subscriptions(&self) -> Result<Vec<Channel>> {
        let out = self.run_output(
            "import-subscriptions",
            &["--flat-playlist", "-J", "https://www.youtube.com/feed/channels"],
        )?;
        Ok(model::parse_channels(&out))
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
        self.run_output("video-metadata", &[
            "-q",
            "--write-comments",
            "--no-download",
            "--extractor-args",
            &extractor,
            "--write-info-json",
            url,
            "-o",
            &template.to_string_lossy(),
        ])?;
        let path = template.with_extension("info.json");
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let (description, comments) = model::parse_info_json(&json);
        let count = model::comment_count(&json);
        prune_comment_files(comments_dir, 20);
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
        let mut cmd = Command::new(&self.binary);
        if !self.browser.trim().is_empty() {
            cmd.args(["--cookies-from-browser", self.browser.trim()]);
        }
        cmd.args(args);
        run(cmd, &self.binary, label)
    }

    fn run_output_in<S: AsRef<std::ffi::OsStr>>(
        &self,
        label: &str,
        args: &[S],
        dir: &Path,
    ) -> Result<String> {
        let mut cmd = Command::new(&self.binary);
        if !self.browser.trim().is_empty() {
            cmd.args(["--cookies-from-browser", self.browser.trim()]);
        }
        cmd.args(args).current_dir(dir);
        run(cmd, &self.binary, label)
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

/// Keeps the most recent comment files and deletes the rest: one file per video
/// would otherwise grow without bound in the cache directory.
fn prune_comment_files(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();
    if files.len() <= keep {
        return;
    }
    files.sort_by_key(|(modified, _)| *modified);
    for (_, path) in &files[..files.len() - keep] {
        std::fs::remove_file(path).ok();
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
#[cfg(test)]
mod live {
    use super::*;

    fn home() -> std::path::PathBuf {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    }

    fn init_logging() {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or("info"),
        )
        .is_test(false)
        .try_init();
    }

    fn test_config() -> Config {
        Config {
            cookies_from_browser: String::new(),
            notification_enabled: false,
            mpv_log_level: std::env::var("MT_MPV_LOG").unwrap_or_else(|_| "all=warn".into()),
            ..Default::default()
        }
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
            "22/bv*[height<=720]+ba/b[height<=720] / wv*+ba/w",
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
