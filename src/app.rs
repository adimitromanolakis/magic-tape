use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui;

use crate::config::{Config, ListView, Paths};
use crate::model::{Channel, Comment, DurationFilter, Feed, Video};
use crate::player::{self, Player};
use crate::store::{LastAction, Store};
use crate::tasks::{Outcome, Worker};
use crate::thumbs::ThumbCache;
use crate::yt::YtDlp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Feeds,
    Playlists,
    Channels,
    Liked,
    History,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Feeds => "Feeds",
            Section::Playlists => "Playlists",
            Section::Channels => "Channels",
            Section::Liked => "Liked videos",
            Section::History => "Watch history",
        }
    }
}

#[derive(Default)]
pub struct Detail {
    pub video: Option<Video>,
    pub description: String,
    pub comments: Vec<Comment>,
    pub comment_count: u64,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct FeedState {
    pub videos: Vec<Video>,
    pub start: usize,
    pub has_more: bool,
    pub loading: bool,
}

impl Default for FeedState {
    fn default() -> Self {
        Self {
            videos: Vec::new(),
            start: 0,
            has_more: true,
            loading: false,
        }
    }
}

pub struct App {
    pub ctx: egui::Context,
    pub config: Config,
    pub paths: Paths,
    pub store: Store,
    pub yt: YtDlp,
    pub player: Player,
    pub worker: Worker,
    pub thumbs: ThumbCache,
    pub icons: crate::ui::icons::IconTheme,

    pub section: Section,
    pub feed: Feed,
    pub query: String,
    /// The feed to come back to when the search results are dismissed.
    pub feed_before_search: Feed,
    pub filter: DurationFilter,
    pub channel_id: String,
    pub channel_name: String,
    pub channel_query: String,
    pub channel_results: Vec<Channel>,
    pub playlist_query: String,
    pub playlists: Vec<Video>,
    pub liked: Vec<crate::store::HistoryEntry>,
    pub history: Vec<crate::store::HistoryEntry>,

    pub feed_state: FeedState,
    pub selected: Option<usize>,
    pub detail: Detail,
    pub search_input: String,
    /// Set by `Ctrl+F`; the toolbar consumes it by focusing the search box.
    pub focus_search: bool,
    pub status: String,
    pub toasts: VecDeque<Toast>,
    pub show_settings: bool,
    /// Open folder chooser for the download directory, if one is showing.
    pub folder_picker: Option<crate::ui::folder_picker::FolderPicker>,
    pub show_shortcuts: bool,
    pub settings_tab: usize,
    pub columns: usize,
    pub list_view: ListView,
    pub ytdlp_version: String,
    pub started: Instant,
    pub subscriptions: Vec<(String, String)>,
    pub subscription_ids: std::collections::HashSet<String>,
    pub subs_filter: String,
    pub font_filter: String,
    pub visible_rows: usize,
    pub scroll_to_index: Option<usize>,
    pub pending_select: Option<usize>,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: Instant,
}

pub enum ToastKind {
    Info,
    Success,
    Error,
}

impl Toast {
    pub fn age(&self) -> Duration {
        self.born.elapsed()
    }
}

#[derive(Debug, Default)]
pub struct Startup {
    pub query: Option<String>,
    pub feed: Option<Feed>,
}

impl App {
    pub fn new(ctx: &egui::Context, startup: Startup) -> Self {
        let paths = Paths::new();
        let config = Config::load(&paths.config_file);
        let store = Store::new(paths.clone());
        let yt = YtDlp::new(&config);
        let player = Player::from_config(&config);
        Self {
            ctx: ctx.clone(),
            config,
            paths,
            store,
            yt,
            player,
            worker: Worker::new(),
            thumbs: ThumbCache::new(ctx),
            icons: crate::ui::icons::IconTheme::new(ctx),
            section: Section::Feeds,
            feed: startup.feed.unwrap_or(Feed::Subscriptions),
            feed_before_search: Feed::Subscriptions,
            query: startup.query.clone().unwrap_or_default(),
            search_input: startup.query.unwrap_or_default(),
            focus_search: false,
            filter: DurationFilter::Any,
            channel_id: String::new(),
            channel_name: String::new(),
            channel_query: String::new(),
            channel_results: Vec::new(),
            playlist_query: String::new(),
            playlists: Vec::new(),
            liked: Vec::new(),
            history: Vec::new(),
            feed_state: FeedState::default(),
            selected: None,
            detail: Default::default(),
            status: "Ready".into(),
            toasts: VecDeque::new(),
            show_settings: false,
            folder_picker: None,
            show_shortcuts: false,
            settings_tab: 0,
            columns: 3,
            list_view: ListView::Grid,
            ytdlp_version: String::new(),
            started: Instant::now(),
            subscriptions: Vec::new(),
            subscription_ids: std::collections::HashSet::new(),
            subs_filter: String::new(),
            font_filter: String::new(),
            visible_rows: 0,
            scroll_to_index: None,
            pending_select: None,
        }
    }

    pub fn boot(&mut self) {
        log::info!(
            "[magic-tape] starting: {} | {} | cache: {}",
            self.yt.describe(),
            self.player.describe(),
            self.paths.cache_dir.display()
        );
        log::info!(
            "[magic-tape] config: {}",
            self.paths.config_file.display()
        );
        self.reload_subscriptions();
        if !self.query.trim().is_empty() {
            // `magic-tape --search rust` starts in the search results: remember
            // the feed the user asked for so "Back" can return to it.
            if self.feed != Feed::Search {
                self.feed_before_search = self.feed;
            }
            self.feed = Feed::Search;
            let query = self.query.clone();
            self.start_search(&query);
        } else if !self.yt.available() {
            self.toast(
                "yt-dlp was not found on PATH - install it to browse YouTube".into(),
                ToastKind::Error,
            );
            self.status = "yt-dlp missing".into();
        } else {
            self.status = "Loading your subscriptions feed...".into();
            self.load_feed(Feed::Subscriptions, 0);
        }
        if !self.player.available() {
            self.toast(
                "mpv was not found on PATH - playback is disabled".into(),
                ToastKind::Error,
            );
        }
        let ytdlp = self.yt.clone();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            self.config.window_width,
            self.config.window_height,
        )));
        self.worker.spawn("check yt-dlp", move || {
            Outcome::YtDlpUpdated {
                version: ytdlp.version(),
            }
        });
    }

    /// Where downloads go, honouring absolute paths and `~`.
    pub fn download_dir(&self) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        crate::config::resolve_download_dir(&self.config.download_directory, &home)
    }

    pub fn list_length(&self) -> usize {
        self.config.list_length.max(1)
    }

    pub fn toast(&mut self, text: String, kind: ToastKind) {
        let kind_tag = match kind {
            ToastKind::Info => "info",
            ToastKind::Success => "ok",
            ToastKind::Error => "err",
        };
        log::info!("[{kind_tag}] {text}");
        self.toasts.push_back(Toast {
            text,
            kind,
            born: Instant::now(),
        });
        while self.toasts.len() > 4 {
            self.toasts.pop_front();
        }
    }

    pub fn load_feed(&mut self, feed: Feed, start: usize) {
        let yt = self.yt.clone();
        let filter = self.filter;
        let query: Option<String> = search_query_for(feed, &self.query)
            .map(str::to_string);
        let channel = self.channel_id.clone();
        let count = self.list_length();
        self.feed_state.loading = true;
        self.status = if start == 0 {
            format!("Loading {}...", feed.label())
        } else {
            format!("Loading more {}...", feed.label())
        };
        self.worker.spawn("load feed",
            move || match yt.list_feed(feed, &channel, query.as_deref(), filter, start, count) {
            Ok(listing) => Outcome::FeedLoaded {
                feed,
                channel,
                query,
                start,
                videos: listing.videos,
            },
            Err(error) => Outcome::Failed {
                label: format!("loading {}", feed.label()),
                error: error.to_string(),
            },
        });
    }

    pub fn load_playlists(&mut self, query: &str, start: usize) {
        let yt = self.yt.clone();
        let query = query.trim().to_string();
        if query.is_empty() {
            self.toast("Enter a search term for playlists first".into(), ToastKind::Info);
            return;
        }
        let count = self.list_length();
        self.status = "Searching playlists...".into();
        self.worker.spawn(
            "load playlists",
            move || match yt.list_playlists(&query, start, count) {
                Ok(videos) => Outcome::PlaylistsLoaded { query, start, videos },
                Err(error) => Outcome::Failed {
                    label: "searching playlists".into(),
                    error: error.to_string(),
                },
            },
        );
    }

    pub fn search_channels(&mut self, query: &str, start: usize) {
        let yt = self.yt.clone();
        let query = query.trim().to_string();
        if query.is_empty() {
            self.toast("Enter a channel name first".into(), ToastKind::Info);
            return;
        }
        let count = self.list_length();
        self.status = "Searching channels...".into();
        self.worker.spawn(
            "search channels",
            move || match yt.search_channels(&query, start, count) {
                Ok(channels) => Outcome::ChannelsLoaded { query, start, channels },
                Err(error) => Outcome::Failed {
                    label: "searching channels".into(),
                    error: error.to_string(),
                },
            },
        );
    }

    pub fn import_subscriptions(&mut self) {
        let yt = self.yt.clone();
        let store = self.store.clone();
        self.status = "Importing your YouTube subscriptions...".into();
        self.worker.spawn("import subscriptions",
            move || {
            match yt.subscriptions() {
                Ok(channels) => {
                    let entries: Vec<(String, String)> = channels
                        .into_iter()
                        .map(|c| (c.id, c.name))
                        .collect();
                    let count = entries.len();
                    match store.replace_subscriptions(&entries) {
                        Ok(()) => Outcome::SubscriptionsImported {
                            count,
                            backup: Some(
                                store
                                    .paths
                                    .subscriptions_backup()
                                    .to_string_lossy()
                                    .into_owned(),
                            ),
                        },
                        Err(error) => Outcome::Failed {
                            label: "saving subscriptions".into(),
                            error: error.to_string(),
                        },
                    }
                }
                Err(error) => Outcome::Failed {
                    label: "importing subscriptions".into(),
                    error: error.to_string(),
                },
            }
        });
    }

    pub fn load_comments_for(&mut self, video: Video) {
        let config = self.config.clone();
        let yt = self.yt.clone();
        let comments_dir = self.paths.comments_dir.clone();
        let pinned_first = config.pinned_comments == crate::config::PinnedPlacement::First;
        let sort = config.comments_sort.as_ytdlp().to_string();
        let url = video.url.clone();
        self.detail.loading = true;
        self.detail.error = None;
        self.worker.spawn(
            "load comments",
            move || {
                match yt.video_metadata(
                    &url,
                    &comments_dir,
                    config.comments_max,
                    config.comment_replies_max,
                    &sort,
                ) {
                    Ok((description, comments, count)) => Outcome::CommentsLoaded {
                        url,
                        description,
                        comments,
                        count,
                        pinned_first,
                    },
                    Err(error) => Outcome::Failed {
                        label: "loading comments".into(),
                        error: error.to_string(),
                    },
                }
            },
        );
    }

    pub fn play_selected(&mut self, profile: &'static str) {
        let Some(video) = self.current_video() else {
            return;
        };
        let selector = self.selector_for(profile);
        log::info!(
            "[action] play :: profile={profile} selector={selector:?} title={:?} url={}",
            video.title,
            video.url
        );
        self.status = format!("Playing \"{}\"...", video.title);
        let result = if profile == "audio" {
            self.player.play_audio(&video.url)
        } else {
            self.player.play(&video.url, &selector)
        };
        match result {
            Ok(()) => {
                self.notify_played(&video, "Playing");
            }
            Err(error) => {
                self.status = "Playback failed".into();
                self.toast(format!("mpv: {error}"), ToastKind::Error);
            }
        }
    }

    /// Maps a quality button to a yt-dlp format selector.
    pub fn selector_for(&self, profile: &str) -> String {
        match profile {
            "720p" => "22/bv*[height<=720]+ba/b[height<=720] / wv*+ba/w".to_string(),
            "1080p" => "bv*[height<=1080]+ba/b[height<=1080] / wv*+ba/w".to_string(),
            "audio" => "ba/b".to_string(),
            _ => self.config.download_video_quality.clone(),
        }
    }

    fn notify_played(&mut self, video: &Video, prefix: &str) {
        let _ = self.store.push_watch_history(video);
        player::notify(&self.config, &format!("{prefix}: {}", video.title));
    }

    pub fn download_selected(&mut self, audio_only: bool) {
        let Some(video) = self.current_video() else {
            return;
        };
        log::info!(
            "[action] download :: audio_only={audio_only} title=\"{}\" url={}",
            video.title,
            video.url
        );
        let yt = self.yt.clone();
        let directory = self.download_dir().to_string_lossy().into_owned();
        let title = video.title.clone();
        self.status = format!("Downloading \"{title}\"...");
        self.worker.spawn(
            "download",
            move || match yt.download(&video.url, std::path::Path::new(&directory), audio_only) {
                Ok(()) => Outcome::DownloadFinished { title, audio_only },
                Err(error) => Outcome::Failed {
                    label: "downloading".into(),
                    error: error.to_string(),
                },
            },
        );
    }

    pub fn unsubscribe(&mut self, name: &str) {
        match self.store.unsubscribe(name) {
            Ok(()) => {
                self.reload_subscriptions();
                self.channel_results
                    .retain(|c| !c.name.eq_ignore_ascii_case(name));
                self.selected = None;
                self.toast(
                    format!("Unsubscribed from {name}. Unsubscribe on YouTube too for it to take effect there."),
                    ToastKind::Success,
                );
            }
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    pub fn toggle_like(&mut self) {
        let Some(video) = self.current_video() else {
            return;
        };
        log::info!("[action] like :: {}", video.url);
        match self.store.toggle_like(&video) {
            Ok(liked) => {
                self.liked = self.store.liked();
                let msg = if liked {
                    "Added to liked videos"
                } else {
                    "Removed from liked videos"
                };
                self.toast(msg.into(), ToastKind::Success);
            }
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    pub fn subscribe_to_current(&mut self) {
        let Some(video) = self.current_video() else {
            return;
        };
        log::info!("[action] subscribe :: {} {}", video.channel_id, video.channel);
        if video.channel_id.is_empty() {
            self.toast("This channel cannot be subscribed to".into(), ToastKind::Error);
            return;
        }
        match self.store.subscribe(&video.channel_id, &video.channel) {
            Ok(()) => {
                self.reload_subscriptions();
                self.toast(
                    format!("Subscribed to {}", video.channel),
                    ToastKind::Success,
                )
            }
            Err(error) => self.toast(error.to_string(), ToastKind::Error),
        }
    }

    pub fn current_video(&self) -> Option<Video> {
        if self.section == Section::Feeds {
            self.feed_state
                .videos
                .get(self.selected?)
                .cloned()
        } else if self.section == Section::Playlists {
            self.playlists.get(self.selected?).cloned()
        } else if self.section == Section::Liked {
            self.liked
                .get(self.selected?)
                .map(entry_to_video)
        } else {
            self.history.get(self.selected?).map(entry_to_video)
        }
    }

    pub fn select(&mut self, index: Option<usize>) {
        if index != self.selected {
            self.scroll_to_index = index;
        }
        self.selected = index;
        self.detail = Default::default();
        if let Some(video) = self.current_video() {
            self.ensure_thumbnail(&video);
            if self.config.show_comments {
                self.load_comments_for(video);
            }
        }
    }

    pub fn start_search(&mut self, term: &str) {
        let term = term.trim().to_string();
        if !self.begin_search(&term) {
            return;
        }
        let _ = self.store.push_search_history(&term);
        self.load_feed(Feed::Search, 0);
    }

    /// Starts a search without doing any I/O, so the transition can be tested
    /// without yt-dlp. Returns false when there is nothing to search for.
    fn begin_search(&mut self, term: &str) -> bool {
        let term = term.trim();
        if term.is_empty() {
            return false;
        }
        self.section = Section::Feeds;
        if self.feed != Feed::Search {
            self.feed_before_search = self.feed;
        }
        self.feed = Feed::Search;
        self.query = term.to_string();
        self.filter = DurationFilter::Any;
        true
    }

    /// Leaves the search results and returns to the feed that was open before.
    ///
    /// The query is cleared as well: leaving it behind used to poison every
    /// later request, because a non-empty query made yt-dlp build a search URL
    /// for *every* feed.
    pub fn leave_search(&mut self) {
        if let Some(feed) = self.end_search() {
            self.load_feed(feed, 0);
        }
    }

    /// The `leave_search` state change, without the reload. Returns the feed
    /// that now needs loading, if any.
    fn end_search(&mut self) -> Option<Feed> {
        if self.feed != Feed::Search {
            self.query.clear();
            return None;
        }
        self.query.clear();
        self.filter = DurationFilter::Any;
        self.feed = self.feed_before_search;
        Some(self.feed)
    }

    /// How far PageUp/PageDown move: one screenful, minus a row of overlap.
    pub fn page_step(&self) -> usize {
        Self::step_for(self.visible_rows)
    }

    pub fn step_for(visible_rows: usize) -> usize {
        if visible_rows == 0 { 5 } else { visible_rows.saturating_sub(1).max(1) }
    }

    /// Clamps a requested selection index into the current list.
    pub fn clamp_index(current: Option<usize>, count: usize, delta: isize) -> Option<usize> {
        if count == 0 {
            return None;
        }
        let current = current.unwrap_or(0) as isize;
        Some((current + delta).clamp(0, count as isize - 1) as usize)
    }

    /// Converts a pending `scroll_to_index` into a vertical scroll offset that
    /// centres the selected row, so keyboard navigation stays visible in the
    /// virtualised grid and list.
    pub fn take_scroll_offset(
        &mut self,
        columns: usize,
        row_height: f32,
        viewport_height: f32,
    ) -> Option<f32> {
        let index = self.scroll_to_index.take()?;
        let row = index / columns.max(1);
        Some((row as f32 * row_height - viewport_height / 2.0 + row_height).max(0.0))
    }

    /// The subscription list is cached because the sidebar and the detail pane
    /// need it on every frame; re-reading the file per frame was pointless I/O.
    /// Loads the subscription list and keeps it sorted by name, which is the
    /// order quick access shows it in.
    pub fn reload_subscriptions(&mut self) {
        self.subscriptions = self.store.subscriptions();
        self.subscriptions
            .sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        self.subscription_ids = self
            .subscriptions
            .iter()
            .map(|(id, _)| id.clone())
            .collect();
    }

    pub fn is_subscribed(&self, channel_id: &str) -> bool {
        !channel_id.is_empty() && self.subscription_ids.contains(channel_id)
    }

    /// Indices into `subscriptions` of the channels matching the quick-access
    /// filter, in display order (the cache is already sorted by name).
    ///
    /// Indices rather than clones: the quick-access list is virtualised, so it
    /// only needs the names of the rows actually on screen, and the row click
    /// handler needs `&mut App`.
    pub fn filtered_subscription_indices(&self) -> Vec<usize> {
        let needle = self.subs_filter.trim().to_lowercase();
        self.subscriptions
            .iter()
            .enumerate()
            .filter(|(_, (_, name))| needle.is_empty() || name.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn item_count(&self) -> usize {
        match self.section {
            Section::Feeds => self.feed_state.videos.len(),
            Section::Playlists => self.playlists.len(),
            Section::Channels => self.channel_results.len(),
            Section::Liked => self.liked.len(),
            Section::History => self.history.len(),
        }
    }

    pub fn has_more(&self) -> bool {
        self.section == Section::Feeds && self.feed_state.has_more
    }

    pub fn load_next_page(&mut self) {
        if !self.has_more() || self.feed_state.loading {
            return;
        }
        let next = self.feed_state.start + self.list_length();
        let feed = self.feed;
        self.load_feed(feed, next);
    }

    pub fn load_prev_page(&mut self) {
        if self.section != Section::Feeds
            || self.feed_state.start == 0
            || self.feed_state.loading
        {
            return;
        }
        let previous = self.feed_state.start.saturating_sub(self.list_length());
        let feed = self.feed;
        self.load_feed(feed, previous);
    }

    /// Queues thumbnails for a slice of videos.
    ///
    /// Split out of `handle_outcome` so the feed's own buffer can be borrowed
    /// mutably while the items in it are read - previously this needed a full
    /// `Vec<Video>` clone on every page load.
    fn ensure_all_thumbnails(app: &mut App, videos: &mut [Video]) {
        let wanted: Vec<(String, String)> = videos
            .iter()
            .filter_map(|v| {
                v.thumbnail
                    .clone()
                    .filter(|u| !u.is_empty())
                    .map(|url| (v.id.clone(), url))
            })
            .collect();
        for (id, url) in wanted {
            app.ensure_image(&id, url);
        }
    }

    /// Merges a freshly loaded page into the list, skipping ids already held.
    ///
    /// Returns how many videos were actually inserted, which is what the
    /// caller needs to shift the selection when prepending.
    fn merge_videos(existing: &mut Vec<Video>, incoming: Vec<Video>, prepend: bool) -> usize {
        if incoming.is_empty() {
            return 0;
        }
        let mut seen: std::collections::HashSet<String> =
            existing.iter().map(|v| v.id.clone()).collect();
        let fresh: Vec<Video> = incoming
            .into_iter()
            .filter(|v| seen.insert(v.id.clone()))
            .collect();
        let inserted = fresh.len();
        if prepend {
            existing.splice(0..0, fresh);
        } else {
            existing.extend(fresh);
        }
        inserted
    }

    pub fn ensure_thumbnail(&mut self, video: &Video) {
        if let Some(url) = video.thumbnail.clone().filter(|u| !u.is_empty()) {
            self.ensure_image(&video.id, url);
        }
    }

    pub fn ensure_channel_thumbnail(&mut self, channel: &Channel) {
        match crate::thumbs::channel_avatar_url(channel.thumbnail.clone()) {
            Some(url) => self.ensure_image(&channel.id, url),
            // No picture of its own: ask the channel page for one.
            None => self.ensure_avatar(&channel.id),
        }
    }

    fn ensure_avatar(&mut self, id: &str) {
        let id = id.to_string();
        if !self.thumbs.request_pending(&id) {
            return;
        }
        if let Some(existing) = self.cached_thumb_path(&id) {
            self.thumbs.insert(&self.ctx, &id, existing);
            return;
        }
        self.thumbs.mark_pending(&id, format!("channel:{id}"));
        let dest = self.store.thumb_path(&id);
        self.worker.spawn(
            "fetch channel avatar",
            move || match crate::tasks::download_channel_avatar(&id, dest.clone()) {
                Ok(()) => Outcome::ThumbnailReady { id, path: dest },
                Err(error) => Outcome::ThumbnailMissing {
                    id,
                    error: error.to_string(),
                },
            },
        );
    }

    fn ensure_image(&mut self, id: &str, url: String) {
        let id = id.to_string();
        if !self.thumbs.request_pending(&id) {
            return;
        }
        if let Some(existing) = self.cached_thumb_path(&id) {
            self.thumbs.insert(&self.ctx, &id, existing);
            return;
        }
        self.thumbs.mark_pending(&id, url.clone());
        let dest = self.store.thumb_path(&id);
        self.worker.spawn(
            "download thumbnail",
            move || match crate::tasks::download_thumbnail(&url, dest.clone()) {
                Ok(()) => Outcome::ThumbnailReady { id, path: dest },
                Err(error) => Outcome::ThumbnailMissing {
                    id,
                    error: error.to_string(),
                },
            },
        );
    }

    fn cached_thumb_path(&self, id: &str) -> Option<PathBuf> {
        let path = self.store.thumb_path(id);
        path.exists().then_some(path)
    }

    pub fn handle_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::FeedLoaded {
                feed,
                channel,
                query,
                start,
                videos,
            } => {
                let loaded = videos.len();
                self.feed_state.has_more = loaded >= self.list_length();
                if start == 0 {
                    self.feed_state.videos = videos;
                    self.selected = None;
                    self.detail = Default::default();
                } else {
                    // A page below the current start is older content: it belongs
                    // in front, or the list would read newest-first again after
                    // going back a page.
                    let prepend = start < self.feed_state.start;
                    let inserted = Self::merge_videos(&mut self.feed_state.videos, videos, prepend);
                    // Prepending shifts every existing index down.
                    if prepend
                        && inserted > 0
                        && let Some(current) = self.selected
                    {
                        self.selected = Some((current + inserted).min(
                            self.feed_state.videos.len().saturating_sub(1),
                        ));
                    }
                }
                if let Some(target) = self.pending_select.take()
                    && target < self.feed_state.videos.len()
                {
                    self.select(Some(target));
                }
                self.feed_state.start = start;
                self.feed_state.loading = false;
                // Move the buffer out so it can be borrowed both as `&[Video]`
                // (to read) and as `&mut App` (to queue the downloads).
                let mut videos = std::mem::take(&mut self.feed_state.videos);
                Self::ensure_all_thumbnails(self, &mut videos);
                self.feed_state.videos = videos;
                self.status = format!(
                    "{} videos loaded - {}",
                    self.feed_state.videos.len(),
                    feed.label()
                );
                if self.section == Section::Feeds && feed == self.feed
                    && let Err(error) = self.store.save_last_action(&LastAction {
                        query: query.unwrap_or_default(),
                        start,
                        count: self.list_length(),
                        feed: feed_label(feed).into(),
                        channel,
                    }) {
                        log::warn!("could not persist last action: {error}");
                    }
            }
            Outcome::PlaylistsLoaded { query, start, videos } => {
                self.playlist_query = query;
                if start == 0 {
                    self.playlists = videos;
                    self.selected = None;
                } else {
                    self.playlists.extend(videos);
                }
                self.status = format!("{} playlists found", self.playlists.len());
            }
            Outcome::ChannelsLoaded { query, start, channels } => {
                self.channel_query = query;
                if start == 0 {
                    self.channel_results = channels;
                    self.selected = None;
                } else {
                    self.channel_results.extend(channels);
                }
                self.status = format!("{} channels found", self.channel_results.len());
            }
            Outcome::SubscriptionsImported { count, backup } => {
                self.reload_subscriptions();
                self.status = format!("Imported {count} subscriptions");
                self.toast(
                    match backup.clone() {
                        Some(path) => format!(
                            "Imported {count} subscriptions. Previous list backed up to {path}"
                        ),
                        None => format!("Imported {count} subscriptions"),
                    },
                    ToastKind::Success,
                );
                if let Some(backup) = backup {
                    self.open_file_in_explorer(&backup);
                }
            }
            Outcome::CommentsLoaded {
                url,
                description,
                comments,
                count,
                pinned_first,
            } => {
                let matches = self
                    .detail
                    .video
                    .as_ref()
                    .map(|v| v.url == url)
                    .unwrap_or(false);
                if matches {
                    self.detail.description = description;
                    self.detail.comments = comments;
                    self.detail.comment_count = count;
                    self.detail.loading = false;
                    self.detail.error = None;
                }
                let _ = pinned_first;
            }
            Outcome::DownloadFinished { title, audio_only } => {
                self.status = "Download complete".into();
                let kind = if audio_only { "audio" } else { "video" };
                self.toast(format!("Downloaded {kind} for \"{title}\""), ToastKind::Success);
                let directory = self.download_dir();
                player::notify(&self.config, &format!("Download complete: {title}"));
                let _ = player::open_directory(&directory);
            }
            Outcome::YtDlpUpdated { version } => {
                self.ytdlp_version = version;
            }
            Outcome::ThumbnailMissing { id, error } => {
                // Logged, not toasted: a channel of uploads with private
                // thumbnails used to bury the UI under red boxes.
                self.thumbs.clear_pending(&id);
                self.thumbs.mark_failed(&id);
                log::warn!("thumbnail unavailable for {id}: {error}");
            }
            Outcome::ThumbnailReady { id, path } => {
                self.thumbs.clear_pending(&id);
                self.thumbs.insert(&self.ctx, &id, path);
            }
            Outcome::Failed { label, error } => {
                self.feed_state.loading = false;
                self.detail.loading = false;
                self.status = format!("Failed while {label}");
                self.toast(format!("Error {label}: {error}"), ToastKind::Error);
            }
        }
    }

    fn open_file_in_explorer(&mut self, path: &str) {
        if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok() {
            let _ = std::process::Command::new("xdg-open").arg(path).spawn();
        }
    }

    pub fn save_config(&mut self) -> Result<()> {
        self.config.clamp();
        self.config.save(&self.paths.config_file)?;
        self.yt = YtDlp::new(&self.config);
        self.player = Player::from_config(&self.config);
        self.list_view = self.config.list_view;
        Ok(())
    }

    pub fn prune_toasts(&mut self) {
        while let Some(front) = self.toasts.front() {
            if front.age() > Duration::from_secs(6) {
                self.toasts.pop_front();
            } else {
                break;
            }
        }
    }
}

fn feed_label(feed: Feed) -> &'static str {
    match feed {
        Feed::Subscriptions => "subscriptions",
        Feed::Trending => "trending",
        Feed::Recommended => "recommended",
        Feed::Channel => "channel",
        Feed::Repeat => "repeat",
        Feed::Search => "search",
    }
}

/// The search term to send with a feed request, if any.
///
/// Only the search feed carries a term. This is the single place that decides
/// it, and it is deliberately not "any feed while a query is stored": a stored
/// query turned every later request into a search, which is why clicking a
/// channel after searching used to keep showing the same results.
pub fn search_query_for(feed: Feed, query: &str) -> Option<&str> {
    if feed != Feed::Search {
        return None;
    }
    match query.trim() {
        "" => None,
        trimmed => Some(trimmed),
    }
}

pub fn entry_to_video(entry: &crate::store::HistoryEntry) -> Video {
    let id = entry.video_id().to_string();
    Video {
        id: id.clone(),
        title: entry.title.clone(),
        url: entry.url.clone(),
        channel_id: entry.channel_id.clone(),
        channel: entry.channel.clone(),
        duration: None,
        views: None,
        thumbnail: Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")),
        published: None,
        is_live: false,
        is_upcoming: false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_step_is_one_screenful_minus_one_row() {
        assert_eq!(App::step_for(0), 5, "unknown viewport falls back to 5");
        assert_eq!(App::step_for(1), 1);
        assert_eq!(App::step_for(6), 5);
        assert_eq!(App::step_for(20), 19);
    }

    #[test]
    fn selection_clamps_at_both_ends() {
        assert_eq!(App::clamp_index(None, 10, 1), Some(1));
        assert_eq!(App::clamp_index(Some(0), 10, -1), Some(0));
        assert_eq!(App::clamp_index(Some(9), 10, 1), Some(9));
        assert_eq!(App::clamp_index(Some(3), 10, 100), Some(9));
        assert_eq!(App::clamp_index(Some(3), 10, -100), Some(0));
        assert_eq!(App::clamp_index(Some(0), 0, 1), None, "empty list has no selection");
    }

    #[test]
    fn scroll_offset_centres_the_selected_row() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        // nothing pending -> keep whatever offset the scroll area already has
        assert_eq!(app.take_scroll_offset(4, 100.0, 400.0), None);

        app.scroll_to_index = Some(0);
        assert_eq!(app.take_scroll_offset(4, 100.0, 400.0), Some(0.0));

        // row 5 of 4 columns starts at 500px; centred in a 400px viewport
        app.scroll_to_index = Some(20);
        assert_eq!(app.take_scroll_offset(4, 100.0, 400.0), Some(400.0));
    }

    fn video(id: &str) -> Video {
        Video {
            id: id.into(),
            title: format!("title {id}"),
            url: format!("https://www.youtube.com/watch?v={id}"),
            channel_id: "UC1".into(),
            channel: "Chan".into(),
            duration: None,
            views: None,
            thumbnail: None,
            published: None,
            is_live: false,
            is_upcoming: false,
        }
    }

    #[test]
    fn merging_a_page_appends_at_the_end_by_default() {
        let mut videos = vec![video("a"), video("b")];
        let inserted = App::merge_videos(&mut videos, vec![video("c")], false);
        assert_eq!(inserted, 1);
        assert_eq!(
            videos.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn merging_an_older_page_prepends_so_the_list_stays_in_feed_order() {
        let mut videos = vec![video("d"), video("e")];
        let inserted = App::merge_videos(&mut videos, vec![video("b"), video("c")], true);
        assert_eq!(inserted, 2);
        assert_eq!(
            videos.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
            ["b", "c", "d", "e"]
        );
    }

    #[test]
    fn merging_skips_ids_already_loaded() {
        let mut videos = vec![video("a")];
        let inserted = App::merge_videos(&mut videos, vec![video("a"), video("b")], false);
        assert_eq!(inserted, 1, "only the unseen video counts as inserted");
        assert_eq!(videos.len(), 2);
        assert_eq!(videos[0].id, "a");
    }

    fn app_with_subscriptions(count: usize) -> App {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.subscriptions = (0..count)
            .map(|i| {
                (
                    format!("UC{i:024}"),
                    format!("Channel {i:03}"),
                )
            })
            .collect();
        app.subscription_ids = app.subscriptions.iter().map(|(id, _)| id.clone()).collect();
        app
    }

    #[test]
    fn subscription_lookup_uses_the_cache() {
        let app = app_with_subscriptions(3);
        assert!(app.is_subscribed("UC000000000000000000000000"));
        assert!(!app.is_subscribed("UC999999999999999999999999"));
        assert!(!app.is_subscribed(""), "an empty id is never subscribed");
    }

    #[test]
    fn subscription_filter_matches_case_insensitively() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.subscriptions = vec![
            ("UCa".to_string(), "Zebra channel".to_string()),
            ("UCb".to_string(), "alpha CHANNEL".to_string()),
            ("UCc".to_string(), "Beta Talk".to_string()),
        ];

        assert_eq!(
            app.filtered_subscription_indices().len(),
            3,
            "no filter matches every channel"
        );

        app.subs_filter = "channel".into();
        let matched: Vec<&str> = app
            .filtered_subscription_indices()
            .into_iter()
            .map(|i| app.subscriptions[i].1.as_str())
            .collect();
        assert_eq!(
            matched,
            vec!["Zebra channel", "alpha CHANNEL"],
            "matches case-insensitively"
        );

        app.subs_filter = "talK".into();
        assert_eq!(
            app.filtered_subscription_indices().len(),
            1,
            "matches mid-word"
        );
        app.subs_filter = "zzz".into();
        assert!(app.filtered_subscription_indices().is_empty());
    }

    #[test]
    fn quick_access_lists_every_channel_not_a_capped_page() {
        let app = app_with_subscriptions(640);
        assert_eq!(
            app.filtered_subscription_indices().len(),
            app.subscriptions.len(),
            "every subscription is reachable; there is no row cap"
        );
    }

    #[test]
    fn subscriptions_are_kept_in_display_order() {
        let dir = std::env::temp_dir().join(format!(
            "mt-order-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("subscriptions.txt");
        std::fs::write(&file, "UCb zebra\nUCa Alpha\nUCc beta\n").unwrap();
        // `Paths::subscriptions()` derives the file from data_dir.
        let paths = crate::config::Paths {
            data_dir: dir.clone(),
            ..crate::config::Paths::new()
        };
        let store = crate::store::Store::new(paths);
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.store = store;
        app.reload_subscriptions();
        let names: Vec<&str> = app
            .subscriptions
            .iter()
            .map(|(_, name)| name.as_str())
            .collect();
        assert_eq!(names, vec!["Alpha", "beta", "zebra"], "sorted by name");
        assert!(app.is_subscribed("UCa"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_the_search_feed_carries_the_query() {
        // The bug: a stored query turned every later request into a search, so
        // clicking a channel or Trending kept showing the same results.
        assert_eq!(search_query_for(Feed::Search, "rust"), Some("rust"));
        assert_eq!(search_query_for(Feed::Search, "  rust  "), Some("rust"));
        assert_eq!(search_query_for(Feed::Search, "   "), None);
        for feed in [
            Feed::Subscriptions,
            Feed::Trending,
            Feed::Recommended,
            Feed::Channel,
            Feed::Repeat,
        ] {
            assert_eq!(
                search_query_for(feed, "rust"),
                None,
                "{feed:?} must not be searched"
            );
        }
    }

    #[test]
    fn a_leftover_query_cannot_poison_another_feed() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        assert!(app.begin_search("rust"));
        assert_eq!(app.feed, Feed::Search);
        assert_eq!(app.feed_before_search, Feed::Subscriptions);
        assert_eq!(search_query_for(app.feed, &app.query), Some("rust"));

        // Opening a channel after a search asks for the channel, not the term,
        // however the search was left behind.
        app.feed = Feed::Channel;
        assert_eq!(search_query_for(app.feed, &app.query), None);
    }

    #[test]
    fn leaving_a_search_returns_to_the_previous_feed() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.feed = Feed::Trending;
        assert!(app.begin_search("rust"));
        assert_eq!(app.feed_before_search, Feed::Trending);
        assert_eq!(app.end_search(), Some(Feed::Trending));
        assert_eq!(app.feed, Feed::Trending);
        assert_eq!(app.query, "", "the term must not outlive the search");
    }

    #[test]
    fn a_search_inside_a_search_keeps_the_original_return_feed() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.begin_search("rust");
        app.begin_search("zig");
        assert_eq!(app.feed_before_search, Feed::Subscriptions);
        assert_eq!(app.query, "zig", "the newer term wins");
    }

    #[test]
    fn an_empty_term_starts_nothing() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        assert!(!app.begin_search("   "));
        assert_eq!(app.feed, Feed::Subscriptions);
        assert_eq!(app.query, "");
    }

    #[test]
    fn leaving_a_search_from_another_feed_only_clears_the_term() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.feed = Feed::Trending;
        app.query = "rust".into();
        assert_eq!(app.end_search(), None, "no search to leave, no feed to load");
        assert_eq!(app.feed, Feed::Trending, "the feed stays put");
        assert_eq!(app.query, "");
    }

    #[test]
    fn select_requests_a_scroll_only_when_the_selection_changes() {
        let mut app = App::new(&egui::Context::default(), Startup::default());
        app.select(Some(2));
        assert_eq!(app.scroll_to_index, Some(2));
        app.take_scroll_offset(1, 70.0, 300.0);
        app.select(Some(2));
        assert_eq!(app.scroll_to_index, None, "same index needs no scrolling");
    }
}
