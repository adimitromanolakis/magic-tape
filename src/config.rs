use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    pub play_in_fullscreen: bool,
    pub window_width: f32,
    pub window_height: f32,
    pub mpv_binary: String,
    pub mpv_log_level: String,
    pub playback_client: String,
    pub ytdlp_binary: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cookies_from_browser: "firefox".into(),
            list_length: 20,
            download_directory: "/Downloads".into(),
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
            play_in_fullscreen: true,
            window_width: 1360.0,
            window_height: 860.0,
            mpv_binary: "mpv".into(),
            mpv_log_level: "all=warn".into(),
            playback_client: "default".into(),
            ytdlp_binary: "yt-dlp".into(),
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
        if !self.download_directory.starts_with('/') {
            self.download_directory.insert(0, '/');
        }
        while self.download_directory.ends_with('/') && self.download_directory.len() > 1 {
            self.download_directory.pop();
        }
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
                "DOWNLOAD_DIRECTORY" => self.download_directory = value.to_string(),
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