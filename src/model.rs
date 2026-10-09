use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feed {
    Subscriptions,
    Trending,
    Recommended,
    Search,
    Channel,
    Repeat,
}

impl Feed {
    pub fn path(&self, channel: &str) -> String {
        match self {
            Feed::Subscriptions => "/feed/subscriptions".into(),
            Feed::Trending => "/feed/trending".into(),
            Feed::Recommended => String::new(),
            Feed::Search => String::new(),
            Feed::Channel => {
                if channel.starts_with('@') {
                    format!("/{channel}/videos")
                } else {
                    format!("/channel/{channel}/videos")
                }
            }
            Feed::Repeat => String::new(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Feed::Subscriptions => "Subscriptions",
            Feed::Trending => "Trending",
            Feed::Recommended => "For you",
            Feed::Search => "Search results",
            Feed::Channel => "Channel",
            Feed::Repeat => "Previous action",
        }
    }

    pub fn request_needs_extractor_args(&self) -> bool {
        matches!(self, Feed::Subscriptions | Feed::Trending | Feed::Recommended)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationFilter {
    Any,
    Short,
    Medium,
    Long,
    Playlists,
}

impl DurationFilter {
    pub fn label(&self) -> &'static str {
        match self {
            DurationFilter::Any => "Any duration",
            DurationFilter::Short => "Up to 4 minutes",
            DurationFilter::Medium => "4 to 20 minutes",
            DurationFilter::Long => "Over 20 minutes",
            DurationFilter::Playlists => "Playlists only",
        }
    }

    pub fn query_value(&self) -> &'static str {
        match self {
            DurationFilter::Any => "&sp=EgQQARgE",
            DurationFilter::Short => "&sp=EgQQARgB",
            DurationFilter::Medium => "&sp=EgQQARgD",
            DurationFilter::Long => "&sp=EgQQARgC",
            DurationFilter::Playlists => "&sp=EgQQAxgE",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub url: String,
    pub channel_id: String,
    pub channel: String,
    pub duration: Option<String>,
    pub views: Option<u64>,
    pub thumbnail: Option<String>,
    pub published: Option<String>,
    pub is_live: bool,
    pub is_upcoming: bool,
}

impl Video {
    pub fn views_label(&self) -> Option<String> {
        self.views.map(|v| {
            if v >= 1_000_000 {
                format!("{:.1}M views", v as f64 / 1_000_000.0)
            } else if v >= 1_000 {
                format!("{:.1}K views", v as f64 / 1_000.0)
            } else {
                format!("{v} views")
            }
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub subscribers: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub author: String,
    pub text: String,
    pub time_text: String,
    pub like_count: u64,
    pub is_pinned: bool,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    webpage_url: Option<String>,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    duration_string: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    view_count: Option<u64>,
    #[serde(default)]
    timestamp: Option<i64>,
    #[serde(default)]
    approximate_date: Option<String>,
    #[serde(default)]
    release_timestamp: Option<i64>,
    #[serde(default)]
    live_status: Option<String>,
    #[serde(default)]
    playlist_uploader: Option<String>,
    #[serde(default)]
    playlist_uploader_id: Option<String>,
    #[serde(default)]
    thumbnails: Option<Vec<RawThumbnail>>,
    #[serde(default)]
    channel_follower_count: Option<u64>,
    #[serde(default)]
    uploader: Option<String>,
    #[serde(default)]
    uploader_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawThumbnail {
    #[serde(default)]
    url: Option<String>,
}

impl RawEntry {
    fn into_video(self, parent: &PlaylistContext) -> Option<Video> {
        let id = self.id.clone()?;
        if id == "null" || id.is_empty() {
            return None;
        }
        let live_status = self.live_status.unwrap_or_default();
        let is_live = matches!(live_status.as_str(), "is_live" | "is_upcoming");
        let is_upcoming = live_status == "is_upcoming";
        let duration = self
            .duration_string
            .filter(|d| !d.is_empty())
            .or_else(|| {
                self.duration
                    .filter(|d| d.is_finite() && *d > 0.0)
                    .map(format_duration)
            });
        let published = self
            .approximate_date
            .filter(|d| !d.is_empty())
            .or_else(|| {
                self.timestamp
                    .filter(|t| *t > 0)
                    .or_else(|| self.release_timestamp.filter(|t| *t > 0))
                    .map(human_time)
            });
        let thumbnail = self
            .thumbnails
            .and_then(pick_thumbnail)
            .or_else(|| self.id.as_deref().and_then(thumbnail_for_id))
            .or_else(|| parent.thumbnail.clone());
        let channel_id = first_present([
            self.channel_id.as_deref(),
            self.playlist_uploader_id.as_deref(),
            self.uploader_id.as_deref(),
            parent.channel_id.as_deref(),
        ])
        .unwrap_or_default()
        .to_string();
        let channel = first_present([
            self.channel.as_deref(),
            self.playlist_uploader.as_deref(),
            self.uploader.as_deref(),
            parent.channel.as_deref(),
        ])
        .unwrap_or_default()
        .to_string();
        let url = self
            .url
            .filter(|u| u.starts_with("http"))
            .or_else(|| {
                self.webpage_url
                    .filter(|u| u.starts_with("http"))
                    .map(|u| u.to_string())
            })
            .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}"));
        Some(Video {
            id,
            title: self.title.unwrap_or_default(),
            url,
            channel_id,
            channel,
            duration,
            views: self.view_count,
            thumbnail,
            published,
            is_live,
            is_upcoming,
        })
    }
}

/// yt-dlp reports channel metadata on the playlist object rather than on every
/// entry, which is what channel feeds rely on.
#[derive(Debug, Default)]
struct PlaylistContext {
    channel: Option<String>,
    channel_id: Option<String>,
    thumbnail: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct RawPlaylist {
    #[serde(default)]
    entries: Option<Vec<Option<RawEntry>>>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    channel_id: Option<String>,
    #[serde(default)]
    thumbnails: Option<Vec<RawThumbnail>>,
}

fn first_present(candidates: [Option<&str>; 4]) -> Option<&str> {
    candidates
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty() && *value != "null")
}

fn thumbnail_for_id(id: &str) -> Option<String> {
    let id = id.split(['?', '&']).next().unwrap_or(id);
    if id.len() < 4
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"))
}

fn pick_thumbnail(thumbs: Vec<RawThumbnail>) -> Option<String> {
    thumbs
        .iter()
        .filter_map(|t| {
            let url = t.url.as_deref()?;
            let url = url.split('?').next().unwrap_or(url);
            (!url.is_empty()).then(|| url.to_string())
        })
        .find(|url| url.contains("/vi/"))
        .or_else(|| {
            thumbs.iter().find_map(|t| {
                let url = t.url.as_deref()?.split('?').next()?;
                (!url.is_empty()).then(|| url.to_string())
            })
        })
}

pub fn parse_videos(json: &str) -> Vec<Video> {
    let (entries, context) = parse_entries(json);
    entries
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.into_video(&context))
        .collect()
}

pub fn parse_channels(json: &str) -> Vec<Channel> {
    let (entries, context) = parse_entries(json);
    let from_playlist = if context.channel_id.is_some() {
        Some(Channel {
            id: context.channel_id.clone().unwrap_or_default(),
            name: context.channel.clone().unwrap_or_default(),
            subscribers: None,
            thumbnail: context.thumbnail.clone(),
        })
    } else {
        None
    };
    let mut channels: Vec<Channel> = entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = first_present([
                entry.channel_id.as_deref(),
                entry.playlist_uploader_id.as_deref(),
                entry.uploader_id.as_deref(),
                entry.id.as_deref(),
            ])
            .filter(|id| id.starts_with("UC"))?
            .to_string();
            let name = first_present([
                entry.channel.as_deref(),
                entry.playlist_uploader.as_deref(),
                entry.uploader.as_deref(),
                entry.title.as_deref(),
            ])
            .unwrap_or_default()
            .to_string();
            if name.is_empty() {
                return None;
            }
            let thumbnail = entry
                .thumbnails
                .and_then(pick_thumbnail)
                .or_else(|| thumbnail_for_id(&id));
            Some(Channel {
                id,
                name,
                subscribers: entry.channel_follower_count.map(compact_count),
                thumbnail,
            })
        })
        .collect();
    if let Some(playlist) = from_playlist {
        channels.retain(|c| c.id != playlist.id);
        channels.insert(0, playlist);
    }
    channels.sort_by(|a, b| match (a.name.is_empty(), b.name.is_empty()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        (false, false) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    channels
}

/// Accepts the three shapes yt-dlp emits: a bare array, a playlist object with
/// `entries`, or newline-delimited JSON from older builds.
fn parse_entries(json: &str) -> (Vec<Option<RawEntry>>, PlaylistContext) {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return (Vec::new(), PlaylistContext::default());
    }
    if let Ok(playlist) = serde_json::from_str::<RawPlaylist>(trimmed) {
        let context = PlaylistContext {
            channel: playlist.channel.filter(|c| !c.is_empty()),
            channel_id: playlist.channel_id.filter(|c| !c.is_empty()),
            thumbnail: playlist.thumbnails.and_then(pick_thumbnail),
        };
        return (playlist.entries.unwrap_or_default(), context);
    }
    if let Ok(list) = serde_json::from_str::<Vec<RawEntry>>(trimmed) {
        return (list.into_iter().map(Some).collect(), PlaylistContext::default());
    }
    let mut out = Vec::new();
    for line in trimmed.lines().filter(|l| !l.trim().is_empty()) {
        if let Ok(entry) = serde_json::from_str::<RawEntry>(line) {
            out.push(Some(entry));
        }
    }
    (out, PlaylistContext::default())
}

#[derive(Debug, Deserialize)]
struct RawInfoJson {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    comment_count: Option<u64>,
    #[serde(default)]
    comments: Option<Vec<RawComment>>,
}

#[derive(Debug, Deserialize)]
struct RawComment {
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default, rename = "_time_text")]
    time_text: Option<String>,
    #[serde(default)]
    like_count: Option<u64>,
    #[serde(default)]
    is_pinned: Option<bool>,
}

pub fn parse_info_json(json: &str) -> (String, Vec<Comment>) {
    let Ok(info) = serde_json::from_str::<RawInfoJson>(json) else {
        return (String::new(), Vec::new());
    };
    let comments = info
        .comments
        .unwrap_or_default()
        .into_iter()
        .map(|c| Comment {
            author: c.author.unwrap_or_else(|| "unknown".into()),
            text: c.text.unwrap_or_default(),
            time_text: c.time_text.unwrap_or_default(),
            like_count: c.like_count.unwrap_or(0),
            is_pinned: c.is_pinned.unwrap_or(false),
        })
        .collect();
    (
        info.description.unwrap_or_default(),
        comments,
    )
}

pub fn comment_count(json: &str) -> u64 {
    serde_json::from_str::<RawInfoJson>(json)
        .ok()
        .and_then(|i| i.comment_count)
        .unwrap_or(0)
}

pub fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as i64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

pub fn compact_count(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn human_time(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(timestamp);
    let diff = (now - timestamp).max(0);
    let minutes = diff / 60;
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        plural(minutes, "minute")
    } else if diff < 86_400 {
        plural(diff / 3600, "hour")
    } else if diff < 7 * 86_400 {
        plural(diff / 86_400, "day")
    } else if diff < 30 * 86_400 {
        plural(diff / (7 * 86_400), "week")
    } else if diff < 365 * 86_400 {
        plural(diff / (30 * 86_400), "month")
    } else {
        plural(diff / (365 * 86_400), "year")
    }
}

fn plural(n: i64, unit: &str) -> String {
    if n == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{n} {unit}s ago")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_playlist_object_with_entries() {
        let json = r#"{"id":"rust","title":"rust","_type":"playlist","entries":[
            {"id":"abc","title":"Hello","channel":"Chan","channel_id":"UC1","view_count":1234,"duration":201,"url":"https://www.youtube.com/watch?v=abc"},
            {"id":"abcdefghijk","title":"World","channel":"Chan","channel_id":"UC1","view_count":null,"duration":null}]}"#;
        let videos = parse_videos(json);
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].title, "Hello");
        assert_eq!(videos[0].duration.as_deref(), Some("3:21"));
        assert_eq!(videos[0].views_label().unwrap(), "1.2K views");
        assert_eq!(videos[1].duration, None);
        assert_eq!(
            videos[1].thumbnail.as_deref(),
            Some("https://i.ytimg.com/vi/abcdefghijk/mqdefault.jpg")
        );
    }

    #[test]
    fn inherits_channel_from_playlist_object() {
        let json = r#"{"id":"UC9","channel":"Chan","channel_id":"UC9","title":"Chan - Videos","entries":[
            {"id":"v1","title":"T","url":"https://www.youtube.com/watch?v=v1","duration":65}]}"#;
        let videos = parse_videos(json);
        assert_eq!(videos[0].channel, "Chan");
        assert_eq!(videos[0].channel_id, "UC9");
    }

    #[test]
    fn parses_bare_array_and_ndjson() {
        let array = r#"[{"id":"a","title":"A"},{"id":"b","title":"B"}]"#;
        assert_eq!(parse_videos(array).len(), 2);
        let ndjson = "{\"id\":\"a\",\"title\":\"A\"}\n{\"id\":\"b\",\"title\":\"B\"}";
        assert_eq!(parse_videos(ndjson).len(), 2);
    }

    #[test]
    fn skips_null_entries() {
        let json = r#"{"entries":[{"id":null,"title":"A"},null,{"id":"ok","title":"B"}]}"#;
        let videos = parse_videos(json);
        assert_eq!(videos.len(), 1);
        assert_eq!(videos[0].id, "ok");
    }

    #[test]
    fn parses_channel_search_results() {
        let json = r#"{"entries":[
            {"id":"UCsBjURrPoezykLs9EqgamOA","title":"Fireship","channel":"Fireship","channel_id":"UCsBjURrPoezykLs9EqgamOA","channel_follower_count":4300000}]}"#;
        let channels = parse_channels(json);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "Fireship");
        assert_eq!(channels[0].subscribers.as_deref(), Some("4.3M"));
    }

    #[test]
    fn ignores_query_strings_on_thumbnails() {
        let json = r#"{"entries":[{"id":"x","title":"T","thumbnails":[
            {"url":"https://i.ytimg.com/vi/x/hq720.jpg?sqp=-oaymw&rs=AOn4","width":480,"height":360}]}]}"#;
        let videos = parse_videos(json);
        assert_eq!(
            videos[0].thumbnail.as_deref(),
            Some("https://i.ytimg.com/vi/x/hq720.jpg")
        );
    }

    #[test]
    fn parses_live_status_and_release_timestamp() {
        let json = r#"{"entries":[{"id":"x","title":"T","live_status":"is_upcoming","release_timestamp":1700000000}]}"#;
        let videos = parse_videos(json);
        assert!(videos[0].is_live);
        assert!(videos[0].is_upcoming);
        assert_eq!(videos[0].published.as_deref(), Some("2 years ago"));
    }

    const SEARCH_PAYLOAD: &str = include_str!("../tests/fixtures/search.json");
    const CHANNEL_FEED_PAYLOAD: &str = include_str!("../tests/fixtures/channel_feed.json");
    const CHANNEL_SEARCH_PAYLOAD: &str = include_str!("../tests/fixtures/channel_search.json");

    #[test]
    fn parses_real_search_payload() {
        let videos = parse_videos(SEARCH_PAYLOAD);
        assert_eq!(videos.len(), 3);
        assert_eq!(videos[0].id, "5C_HPTJg5ek");
        assert_eq!(videos[0].channel, "Fireship");
        assert_eq!(videos[0].duration.as_deref(), Some("2:29"));
        assert_eq!(videos[0].url, "https://www.youtube.com/watch?v=5C_HPTJg5ek");
        assert!(videos[0].thumbnail.as_deref().unwrap().starts_with("https://"));
        assert_eq!(videos[0].views_label().as_deref(), Some("2.5M views"));
    }

    #[test]
    fn inherits_channel_for_real_channel_feed_payload() {
        let videos = parse_videos(CHANNEL_FEED_PAYLOAD);
        assert_eq!(videos.len(), 2);
        assert!(videos.iter().all(|v| v.channel == "Fireship"));
        assert!(videos.iter().all(|v| v.channel_id == "UCsBjURrPoezykLs9EqgamOA"));
        assert!(videos.iter().all(|v| v.duration.is_some()));
    }

    #[test]
    fn parses_real_channel_search_payload() {
        let channels = parse_channels(CHANNEL_SEARCH_PAYLOAD);
        assert!(channels.len() >= 2);
        let fireship = channels
            .iter()
            .find(|c| c.id == "UCsBjURrPoezykLs9EqgamOA")
            .expect("fireship");
        assert_eq!(fireship.name, "Fireship");
        assert_eq!(fireship.subscribers.as_deref(), Some("4.3M"));
        assert!(fireship.thumbnail.is_some());
    }

    #[test]
    fn empty_input_is_not_an_error() {
        assert!(parse_videos("").is_empty());
        assert!(parse_videos("   ").is_empty());
        assert!(parse_videos("null").is_empty());
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(65.0), "1:05");
        assert_eq!(format_duration(3725.0), "1:02:05");
    }

    #[test]
    fn parses_comments_and_description() {
        let json = r#"{"description":"desc","comment_count":2,"comments":[{"author":"A","text":"hi","_time_text":"2 days ago","like_count":3,"is_pinned":true},{"author":"B","text":"yo","_time_text":"1 day ago","like_count":0,"is_pinned":false}]}"#;
        let (desc, comments) = parse_info_json(json);
        assert_eq!(desc, "desc");
        assert_eq!(comments.len(), 2);
        assert!(comments[0].is_pinned);
        assert_eq!(comment_count(json), 2);
    }
}
