use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
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
        log::info!("[mpv] availability check :: {}", self.mpv_binary);
        let available = MpvInvocation::new("probe", &self.mpv_binary)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        log::info!("[mpv] availability check :: found={available}");
        available
    }

    /// Plays a YouTube page URL, letting mpv's ytdl hook do the extracting.
    ///
    /// The signed `googlevideo` URLs that `yt-dlp --get-url` hands back are
    /// frequently rejected with HTTP 403 once ffmpeg fetches them, so the page
    /// URL plus a format selector is the only path that reliably works.
    pub fn play(&self, url: &str, profile: &str) -> Result<()> {
        let mut call = MpvInvocation::new("play", &self.mpv_binary)
            .args(self.base_args())
            .args(self.ytdl_args(profile));
        if self.fullscreen {
            call = call.arg("--fullscreen");
        }
        call.arg(url).run()
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
        MpvInvocation::new("play-audio", &self.mpv_binary)
            .args(self.base_args())
            .arg("--no-video")
            .arg("--force-window=no")
            .args(self.ytdl_args("ba/b"))
            .arg(url)
            .run()
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
}

/// One mpv invocation.
///
/// Every start of mpv in this app goes through here - playback, audio-only
/// playback and the `--version` probe behind the availability check - so
/// there is exactly one place to read what mpv was asked to do, however many
/// calls happen.
pub struct MpvInvocation<'a> {
    /// What this run is for; also the log tag: `play`, `play-audio`, `probe`.
    action: &'a str,
    /// The mpv executable exactly as configured, so the printout shows what
    /// the user asked for rather than a resolved guess.
    binary: &'a str,
    args: Vec<String>,
}

impl<'a> MpvInvocation<'a> {
    pub fn new(action: &'a str, binary: &'a str) -> Self {
        Self {
            action,
            binary,
            args: Vec::new(),
        }
    }

    /// Adds one argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds several, keeping their order.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// The argv: the executable first, exactly as it will be spawned.
    fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.binary.to_string());
        argv.extend(self.args.iter().cloned());
        argv
    }

    /// What gets printed: the call, then each argument on its own line.
    ///
    /// One line per argument because the `--ytdl-format` selectors carry
    /// commas, slashes and spaces: packed into one string they cannot be
    /// checked by eye, which is the whole reason for printing them.
    fn printout(&self) -> Vec<String> {
        let argv = self.argv();
        let resolved = crate::yt::resolve_binary(self.binary)
            .map(|path| format!(" (resolved: {})", path.display()))
            .unwrap_or_default();
        let mut lines = vec![format!(
            "[mpv] {} :: {}{} with {} argument(s)",
            self.action,
            self.binary,
            resolved,
            argv.len() - 1
        )];
        for (index, arg) in argv.iter().enumerate() {
            lines.push(format!("[mpv] {} ::   argv[{index}] = {arg}", self.action));
        }
        lines
    }

    /// Starts mpv, printing the invocation first, and hands the child to the
    /// log forwarder.
    pub fn run(&self) -> Result<()> {
        print_lines(self.printout());
        log::info!(
            "[mpv] {} :: {} {}",
            self.action,
            self.binary,
            crate::yt::quote(&self.args)
        );
        let child = Command::new(self.binary)
            .args(&self.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        match child {
            Ok(child) => {
                let pid = child.id();
                log::info!("[mpv] {} :: started, pid={pid}", self.action);
                print_lines([format!("[mpv] {} :: mpv pid={pid}", self.action)]);
                watch(self.action, child);
                Ok(())
            }
            Err(error) => {
                log::error!("[mpv] {} :: FAILED to launch: {error}", self.action);
                Err(error).with_context(|| format!("could not launch {}", self.binary))
            }
        }
    }

    /// Runs mpv to completion and captures its output - for the probe, which
    /// must not leak mpv's pipes into the app's.
    pub fn output(&self) -> Result<Output> {
        print_lines(self.printout());
        log::info!(
            "[mpv] {} :: {} {}",
            self.action,
            self.binary,
            crate::yt::quote(&self.args)
        );
        Command::new(self.binary)
            .args(&self.args)
            .stderr(Stdio::null())
            .output()
            .with_context(|| format!("could not run {}", self.binary))
    }
}

/// Lines on stdout.
///
/// Errors are dropped rather than propagated: a closed pipe (`magic-tape |
/// head`) must not take the player down, and the stderr log still has
/// everything that was printed.
fn print_lines<I: IntoIterator<Item = String>>(lines: I) {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    for line in lines {
        let _ = writeln!(stdout, "{line}");
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
    // Named in the exit lines: two mpv runs can overlap (play a video, then
    // play another without closing the first), and without the pid there is
    // nothing to tie an "exited normally" back to the invocation it belongs to.
    let pid = child.id();
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
                    print_lines([format!("[mpv] {waiter} :: pid={pid} exited normally ({status})")]);
                    log::info!("[mpv] {waiter} :: pid={pid} exited normally ({status})");
                } else {
                    print_lines([format!(
                        "[mpv] {waiter} :: pid={pid} FAILED, mpv exited with {status}"
                    )]);
                    log::error!("[mpv] {waiter} :: pid={pid} FAILED, mpv exited with {status}");
                }
            }
            Err(error) => {
                print_lines([format!(
                    "[mpv] {waiter} :: pid={pid} could not wait for mpv: {error}"
                )]);
                log::error!("[mpv] {waiter} :: pid={pid} could not wait for mpv: {error}");
            }
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

    #[test]
    fn the_printout_lists_the_binary_and_every_argument_in_order() {
        // The point of the wrapper: any invocation can be read back off the
        // screen - the binary first, then each argument as it is passed.
        let call = MpvInvocation::new("play", "mpv")
            .args(["--force-window=yes", "--keep-open=no"])
            .arg("https://www.youtube.com/watch?v=abc");
        let lines = call.printout();
        assert!(
            lines[0].starts_with("[mpv] play :: mpv"),
            "the header names the executable: {:?}",
            lines[0]
        );
        assert!(
            lines[0].ends_with("with 3 argument(s)"),
            "and counts them: {:?}",
            lines[0]
        );
        assert_eq!(lines[1], "[mpv] play ::   argv[0] = mpv");
        assert_eq!(lines[2], "[mpv] play ::   argv[1] = --force-window=yes");
        assert_eq!(lines[3], "[mpv] play ::   argv[2] = --keep-open=no");
        assert_eq!(
            lines[4],
            "[mpv] play ::   argv[3] = https://www.youtube.com/watch?v=abc",
            "the url is the last argument, as mpv expects"
        );
        assert_eq!(lines.len(), 5, "one header line plus one line per argument");
    }

    #[test]
    fn the_probe_prints_too() {
        // Every mpv call goes through the wrapper, the availability probe
        // included - that one used to be assembled on its own.
        let call = MpvInvocation::new("probe", "mpv").arg("--version");
        assert_eq!(call.argv(), vec!["mpv".to_string(), "--version".to_string()]);
    }
}
