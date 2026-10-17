use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CommentsSort {
    New,
    Top,
    #[default]
    Old,
}

impl CommentsSort {
    pub fn as_ytdlp(self) -> &'static str {
        match self {
            CommentsSort::New => "new",
            CommentsSort::Top => "top",
            CommentsSort::Old => "old",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PinnedPlacement {
    #[default]
    First,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ListView {
    #[default]
    Grid,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub cookies_from_browser: String,
    pub list_length: usize,
    pub download_directory: String,
    pub download_video_quality: String,
    pub show_comments: bool,
    pub comments_max: usize,
    pub comment_replies_max: usize,
    pub comments_sort: CommentsSort,
    pub pinned_comments: PinnedPlacement,
    pub show_mpv_keybindings: bool,
    pub notification_enabled: bool,
    pub notification_duration_ms: u64,
    pub colored_messages: bool,
    pub list_view: ListView,
    pub theme: Theme,
    /// Background of the detail pane, as `#RRGGBB`. Empty means the theme's
    /// panel fill, which is what the rest of the chrome uses.
    pub detail_background: String,
    pub ui_font: String,
    pub ui_zoom: f32,
    pub play_in_fullscreen: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub mpv_binary: String,
    pub mpv_log_level: String,
    pub playback_client: String,
    pub ytdlp_binary: String,
    /// How many yt-dlp processes may run at once. Each is heavy Python, so
    /// the useful ceiling is low; six keeps feeds, comments and a download
    /// moving without thrashing the machine.
    pub parallel_ytdlp_runs: usize,
    /// How many thumbnail fetches may run at once. These are cheap HTTP
    /// requests, so this can be higher.
    pub parallel_thumbnails: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cookies_from_browser: "firefox".into(),
            list_length: 20,
            download_directory: "~/Downloads".into(),
            download_video_quality: "bv*+ba/b".into(),
            show_comments: true,
            comments_max: 20,
            comment_replies_max: 10,
            comments_sort: CommentsSort::Old,
            pinned_comments: PinnedPlacement::First,
            show_mpv_keybindings: false,
            notification_enabled: true,
            notification_duration_ms: 4000,
            colored_messages: true,
            list_view: ListView::Grid,
            theme: Theme::Dark,
            detail_background: String::new(),
            ui_font: String::new(),
            ui_zoom: 1.0,
            play_in_fullscreen: true,
            window_width: 1360.0,
            window_height: 860.0,
            mpv_binary: "mpv".into(),
            mpv_log_level: "all=warn".into(),
            playback_client: "default".into(),
            ytdlp_binary: "yt-dlp".into(),
            parallel_ytdlp_runs: 6,
            parallel_thumbnails: 8,
        }
    }
}

impl Config {
    pub fn load(path: &PathBuf) -> Self {
        let mut config = Self::default();
        if let Ok(text) = fs::read_to_string(path) {
            match toml::from_str::<Self>(&text) {
                Ok(loaded) => config = loaded,
                Err(err) => {
                    eprintln!("magic-tape: invalid config ({err}), falling back to defaults");
                    Self::import_legacy_into(&mut config);
                }
            }
        } else {
            Self::import_legacy_into(&mut config);
        }
        config.clamp();
        config
    }

    pub fn save(&self, path: &PathBuf) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let text = toml::to_string_pretty(self)?;
        fs::write(path, text).with_context(|| format!("writing config to {}", path.display()))
    }

    pub fn clamp(&mut self) {
        self.list_length = self.list_length.clamp(1, 99);
        self.comments_max = self.comments_max.clamp(1, 500);
        self.comment_replies_max = self.comment_replies_max.clamp(0, 100);
        self.parallel_ytdlp_runs = self.parallel_ytdlp_runs.clamp(1, 16);
        self.parallel_thumbnails = self.parallel_thumbnails.clamp(1, 32);
        self.download_directory = normalise_directory(&self.download_directory);
        if self.download_directory == "/Downloads" {
            // The shell script stored this as "$HOME/Downloads"; the leading
            // slash used to mean "under my home", now it means absolute.
            self.download_directory = "~/Downloads".into();
        }
        self.ui_zoom = self.ui_zoom.clamp(0.7, 2.0);
        if self.window_width < 640.0 {
            self.window_width = 640.0;
        }
        if self.window_height < 480.0 {
            self.window_height = 480.0;
        }
    }

    fn import_legacy_into(&mut self) {
        let Some(legacy) = Paths::new().legacy_config_file().exists().then(|| {
            fs::read_to_string(Paths::new().legacy_config_file()).unwrap_or_default()
        }) else {
            return;
        };
        for line in legacy.lines() {
            let line = line.trim_start_matches('#').trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "LIST_LENGTH" => {
                    if let Ok(v) = value.parse() {
                        self.list_length = v;
                    }
                }
                "PREF_BROWSER" => self.cookies_from_browser = value.to_string(),
                // The old config always meant $HOME + value.
                "DOWNLOAD_DIRECTORY" => {
                    self.download_directory = format!("~{}", value)
                }
                "COMMENTS_MAX" => {
                    if let Ok(v) = value.parse() {
                        self.comments_max = v;
                    }
                }
                "COMMENT_REPLIES_MAX" => {
                    if let Ok(v) = value.parse() {
                        self.comment_replies_max = v;
                    }
                }
                "COMMENTS_SORT" => {
                    self.comments_sort = match value {
                        "new" => CommentsSort::New,
                        "top" => CommentsSort::Top,
                        _ => CommentsSort::Old,
                    };
                }
                "PINNED_COMMENTS" => {
                    self.pinned_comments = match value {
                        "last" => PinnedPlacement::Last,
                        _ => PinnedPlacement::First,
                    };
                }
                "COMMENTS_TOGGLE" => self.show_comments = value == "yes",
                "SHOW_MPV_KEYBINDINGS" => self.show_mpv_keybindings = value == "yes",
                "NOTIFICATION_ENABLED" => self.notification_enabled = value != "no",
                "NOTIFICATION_DURATION" => {
                    if let Ok(v) = value.parse() {
                        self.notification_duration_ms = v;
                    }
                }
                "COLORED_MESSAGES" => self.colored_messages = value == "yes",
                "UI_FONT" => self.ui_font = value.to_string(),
                "IMAGE_SUPPORT" => {
                    if value == "none" {
                        self.list_view = ListView::List;
                    }
                }
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub thumb_dir: PathBuf,
    pub json_dir: PathBuf,
    pub comments_dir: PathBuf,
    pub history_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let config_file = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("magic-tape")
            .join("config.toml");
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(".local/share"))
            .join("magic-tape");
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"))
            .join("magic-tape");
        Self {
            config_file,
            thumb_dir: cache_dir.join("jpg"),
            json_dir: cache_dir.join("json"),
            comments_dir: cache_dir.join("comments"),
            history_dir: cache_dir.join("history"),
            data_dir,
            cache_dir,
        }
    }

    pub fn legacy_config_file(&self) -> PathBuf {
        self.config_file.with_file_name("magic-tape.conf")
    }

    pub fn watch_history(&self) -> PathBuf {
        self.history_dir.join("watch_history.txt")
    }

    pub fn liked_history(&self) -> PathBuf {
        self.history_dir.join("liked.txt")
    }

    pub fn search_history(&self) -> PathBuf {
        self.history_dir.join("search_history.txt")
    }

    pub fn last_action(&self) -> PathBuf {
        self.history_dir.join("last_action.txt")
    }

    pub fn subscriptions(&self) -> PathBuf {
        self.data_dir.join("subscriptions.txt")
    }

    pub fn subscriptions_backup(&self) -> PathBuf {
        self.data_dir.join(format!("subscriptions-{}.bak.txt", chrono_today()))
    }

    pub fn ensure(&self) {
        for dir in [
            &self.cache_dir,
            &self.thumb_dir,
            &self.json_dir,
            &self.comments_dir,
            &self.history_dir,
            &self.data_dir,
        ] {
            fs::create_dir_all(dir).ok();
        }
        for file in [
            self.watch_history(),
            self.liked_history(),
            self.search_history(),
            self.subscriptions(),
            self.last_action(),
        ] {
            if !file.exists() {
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::write(&file, "").ok();
            }
        }
    }
}

/// Tidies a download directory setting: surrounding spaces and trailing
/// slashes. `~` is deliberately *kept*, because it is what distinguishes
/// "relative to my home" from "absolute" - stripping it would turn `~/Videos`
/// into the absolute path `/Videos`.
fn normalise_directory(setting: &str) -> String {
    let trimmed = setting.trim();
    let mut out = trimmed.to_string();
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    if out.is_empty() {
        "~/Downloads".to_string()
    } else {
        out
    }
}

/// Resolves the download directory setting to a real path.
///
/// A leading `/` means an absolute path (`/tmp`, `/mnt/data`), anything else -
/// including a bare `Downloads` and `~/Videos` - is resolved against the home
/// directory.
pub fn resolve_download_dir(setting: &str, home: &Path) -> PathBuf {
    let cleaned = normalise_directory(setting);
    if cleaned == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = cleaned.strip_prefix("~/") {
        return home.join(rest);
    }
    let path = Path::new(&cleaned);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        home.join(path)
    }
}

fn chrono_today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = now / 86_400;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> &'static Path {
        Path::new("/home/tester")
    }

    #[test]
    fn absolute_paths_are_used_as_they_are() {
        for path in ["/tmp", "/mnt", "/mnt/data/youtube", "/tmp/a b/c"] {
            assert_eq!(
                resolve_download_dir(path, home()),
                PathBuf::from(path),
                "an absolute path must not be put under $HOME"
            );
        }
    }

    #[test]
    fn relative_and_home_paths_resolve_against_home() {
        assert_eq!(
            resolve_download_dir("Downloads", home()),
            PathBuf::from("/home/tester/Downloads")
        );
        assert_eq!(
            resolve_download_dir("~/Videos", home()),
            PathBuf::from("/home/tester/Videos")
        );
        assert_eq!(
            resolve_download_dir("~/", home()),
            PathBuf::from("/home/tester"),
            "a bare ~ is the home directory itself"
        );
    }

    #[test]
    fn the_old_style_default_is_migrated() {
        // A config written by the shell script, or by the first Rust build.
        let mut config = Config {
            download_directory: "/Downloads".into(),
            ..Default::default()
        };
        config.clamp();
        assert_eq!(config.download_directory, "~/Downloads");
        assert_eq!(
            resolve_download_dir(&config.download_directory, home()),
            PathBuf::from("/home/tester/Downloads")
        );
    }

    #[test]
    fn trailing_slashes_and_blanks_are_tidied() {
        assert_eq!(resolve_download_dir("/tmp/", home()), PathBuf::from("/tmp"));
        assert_eq!(
            resolve_download_dir("  /mnt/data/  ", home()),
            PathBuf::from("/mnt/data")
        );
        assert_eq!(
            resolve_download_dir("", home()),
            PathBuf::from("/home/tester/Downloads"),
            "an empty setting falls back to the default"
        );
        assert_eq!(
            resolve_download_dir("   ", home()),
            PathBuf::from("/home/tester/Downloads")
        );
    }

    #[test]
    fn the_default_is_the_home_downloads_folder() {
        let config = Config::default();
        assert_eq!(config.download_directory, "~/Downloads");
        assert_eq!(
            resolve_download_dir(&config.download_directory, home()),
            PathBuf::from("/home/tester/Downloads")
        );
    }
}
