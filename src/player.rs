use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};

use anyhow::{Context, Result};

use crate::config::Config;

#[derive(Clone)]
pub struct Player {
    mpv_binary: String,
    fullscreen: bool,
    log_level: String,
    cookies_from_browser: String,
    playback_client: String,
}

impl Player {
    pub fn new(mpv_binary: &str, fullscreen: bool) -> Self {
        Self::with_config(mpv_binary, fullscreen, &Config::default())
    }

    pub fn from_config(config: &Config) -> Self {
        Self::with_config(
            &config.mpv_binary,
            config.play_in_fullscreen,
            config,
        )
    }

    fn with_config(mpv_binary: &str, fullscreen: bool, config: &Config) -> Self {
        Self {
            mpv_binary: mpv_binary.to_string(),
            fullscreen,
            log_level: config.mpv_log_level.clone(),
            cookies_from_browser: config.cookies_from_browser.clone(),
            playback_client: config.playback_client.clone(),
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "mpv (binary: {}, fullscreen: {}, client: {})",
            self.mpv_binary,
            self.fullscreen,
            if self.playback_client.trim().is_empty() {
                "auto"
            } else {
                self.playback_client.trim()
            }
        )
    }

    pub fn available(&self) -> bool {
        let argv = vec!["--version".to_string()];
        log::info!("[mpv] availability check :: {} {}", self.mpv_binary, crate::yt::quote(&argv));
        let output = Command::new(&self.mpv_binary)
            .arg("--version")
            .stderr(Stdio::null())
            .output();
        let available = output.map(|o| o.status.success()).unwrap_or(false);
        log::info!("[mpv] availability check :: found={available}");
        available
    }

    /// Plays a YouTube page URL, letting mpv's ytdl hook do the extracting.
    ///
    /// The signed `googlevideo` URLs that `yt-dlp --get-url` hands back are
    /// frequently rejected with HTTP 403 once ffmpeg fetches them, so the page
    /// URL plus a format selector is the only path that reliably works.
    pub fn play(&self, url: &str, profile: &str) -> Result<()> {
        let mut args = self.base_args();
        args.extend(self.ytdl_args(profile));
        if self.fullscreen {
            args.push("--fullscreen".into());
        }
        args.push(url.to_string());
        self.spawn(args, "play")
    }

    /// The `--ytdl-*` options that carry the quality profile, the cookies and
    /// the pinned player client.
    ///
    /// Pinning matters: yt-dlp's `web_embedded` client hands out signed URLs
    /// that ffmpeg cannot fetch (`HTTP error 403 Forbidden`). With browser
    /// cookies loaded, yt-dlp often picks that client, so playback is pinned to
    /// `default` - verified to offer 720p/1080p and to return fetchable URLs.
    pub fn ytdl_args(&self, profile: &str) -> Vec<String> {
        let mut args = vec![format!("--ytdl-format={profile}")];
        if let Some(browser) = self.cookies_from_browser() {
            args.push(format!("--ytdl-raw-options=cookies-from-browser={browser}"));
        }
        if let Some(client) = self.single_client() {
            args.push(format!(
                "--ytdl-raw-options=extractor-args=youtube:player_client={client}"
            ));
        }
        args
    }

    /// mpv parses `--ytdl-raw-options` as a comma-separated list, so a value
    /// like `default,web,tv` makes mpv abort with "option parameter could not be
    /// parsed". Only the first client is used, which is also the one that keeps
    /// 1080p and returns URLs ffmpeg can fetch.
    fn single_client(&self) -> Option<String> {
        let configured = self.playback_client.trim();
        if configured.is_empty() {
            return None;
        }
        match configured.split_once(',') {
            Some((first, rest)) => {
                let first = first.trim().to_string();
                log::warn!(
                    "[mpv] play :: playback_client={configured:?} contains a comma; mpv cannot \
                     pass a client list, using {first:?} and dropping {rest:?}"
                );
                (!first.is_empty()).then_some(first)
            }
            None => Some(configured.to_string()),
        }
    }

    pub fn play_audio(&self, url: &str) -> Result<()> {
        if url.is_empty() {
            anyhow::bail!("no audio stream to play")
        }
        let mut args = self.base_args();
        args.push("--no-video".into());
        args.push("--force-window=no".into());
        args.extend(self.ytdl_args("ba/b"));
        args.push(url.to_string());
        self.spawn(args, "play-audio")
    }

    fn cookies_from_browser(&self) -> Option<&str> {
        let browser = self.cookies_from_browser.trim();
        (!browser.is_empty()).then_some(browser)
    }

    fn base_args(&self) -> Vec<String> {
        vec![
            "--force-window=yes".into(),
            "--keep-open=no".into(),
            "--hwdec=auto-safe".into(),
            format!("--msg-level={}", self.log_level),
        ]
    }

    fn spawn(&self, args: Vec<String>, action: &str) -> Result<()> {
        log::info!(
            "[mpv] {action} :: {} {}",
            self.mpv_binary,
            crate::yt::quote(&args)
        );
        let child = Command::new(&self.mpv_binary)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match child {
            Ok(child) => {
                let pid = child.id();
                log::info!("[mpv] {action} :: started, pid={pid}");
                watch(action, child);
                Ok(())
            }
            Err(error) => {
                log::error!("[mpv] {action} :: FAILED to launch: {error}");
                Err(error).with_context(|| format!("could not launch {}", self.mpv_binary))
            }
        }
    }
}

/// Forwards mpv's own diagnostics into the app log and reports how it ended.
///
/// mpv writes its status output (tracks, AO/VO, A/V sync) to stdout and only
/// problems to stderr, so both pipes have to be captured.
fn watch(action: &str, mut child: Child) {
    let action = action.to_string();
    let (tx, rx): (Sender<String>, Receiver<String>) = channel();
    let stdout = child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>);
    let stderr = child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>);
    for stream in [stdout, stderr].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                let level = if line.contains("error")
                    || line.contains("Error")
                    || line.contains("failed")
                    || line.contains("Failed")
                {
                    log::Level::Error
                } else if line.contains("warn") || line.contains("Warn") {
                    log::Level::Warn
                } else {
                    log::Level::Info
                };
                if tx.send(format!("{level}|{line}")).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let waiter = action.clone();
    std::thread::spawn(move || {
        for message in rx {
            let Some((level, line)) = message.split_once('|') else {
                continue;
            };
            match level {
                "ERROR" => log::error!("[mpv] {waiter} :: {line}"),
                "WARN" => log::warn!("[mpv] {waiter} :: {line}"),
                _ => log::info!("[mpv] {waiter} :: {line}"),
            }
        }
        match child.wait() {
            Ok(status) => {
                if status.success() {
                    log::info!("[mpv] {waiter} :: exited normally ({status})");
                } else {
                    log::error!("[mpv] {waiter} :: FAILED, mpv exited with {status}");
                }
            }
            Err(error) => log::error!("[mpv] {waiter} :: could not wait for mpv: {error}"),
        }
    });
}

pub fn open_browser(url: &str) -> Result<()> {
    log::info!("[open] browser :: {url}");
    opener::open(url).with_context(|| format!("opening {url}"))
}

pub fn open_directory(path: &Path) -> Result<()> {
    log::info!("[open] directory :: {}", path.display());
    opener::open(path).with_context(|| format!("opening {}", path.display()))
}

pub fn notify(config: &Config, body: &str) {
    if !config.notification_enabled {
        return;
    }
    log::info!("[notify] {body}");
    let duration = std::time::Duration::from_millis(config.notification_duration_ms);
    let _ = notify_rust::Notification::new()
        .summary("magic-tape")
        .body(body)
        .timeout(duration)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(playback_client: &str) -> Player {
        Player::from_config(&Config {
            playback_client: playback_client.to_string(),
            cookies_from_browser: "firefox".to_string(),
            ..Default::default()
        })
    }

    #[test]
    fn passes_format_cookies_and_client_to_mpv() {
        let args = player("default").ytdl_args("bv*[height<=1080]+ba");
        assert!(args.contains(&"--ytdl-format=bv*[height<=1080]+ba".to_string()));
        assert!(args
            .contains(&"--ytdl-raw-options=cookies-from-browser=firefox".to_string()));
        assert!(args.contains(
            &"--ytdl-raw-options=extractor-args=youtube:player_client=default".to_string()
        ));
        assert!(
            args.iter().all(|a| !a.contains(',')),
            "commas make mpv abort with an unparsable option: {args:?}"
        );
    }

    #[test]
    fn drops_client_lists_because_mpv_cannot_pass_them() {
        assert_eq!(player("default,web,tv").single_client().as_deref(), Some("default"));
        assert_eq!(player("  web  ").single_client().as_deref(), Some("web"));
        assert_eq!(player("").single_client(), None);
        assert_eq!(player("   ").single_client(), None);
    }
}
