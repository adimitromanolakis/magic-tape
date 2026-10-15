use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::Paths;
use crate::model::Video;

#[derive(Debug, Clone, Default)]
pub struct HistoryEntry {
    pub channel_id: String,
    pub channel: String,
    pub url: String,
    pub title: String,
}

impl HistoryEntry {
    pub fn from_video(video: &Video) -> Self {
        Self {
            channel_id: video.channel_id.clone(),
            channel: video.channel.clone(),
            url: video.url.clone(),
            title: video.title.clone(),
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let mut parts = line.splitn(4, ' ');
        let channel_id = parts.next()?.to_string();
        let channel = parts.next().unwrap_or_default().to_string();
        let url = parts.next().unwrap_or_default().to_string();
        let title = parts.next().unwrap_or_default().to_string();
        if title.is_empty() && url.is_empty() {
            return None;
        }
        Some(Self {
            channel_id,
            channel,
            url,
            title,
        })
    }

    fn to_line(&self) -> String {
        format!(
            "{} {} {} {}",
            self.channel_id, self.channel, self.url, self.title
        )
    }

    pub fn video_id(&self) -> &str {
        self.url
            .split("v=")
            .nth(1)
            .map(|id| id.split('&').next().unwrap_or(id))
            .unwrap_or("")
    }
}

#[derive(Clone)]
pub struct Store {
    pub paths: Paths,
}

impl Store {
    pub fn new(paths: Paths) -> Self {
        paths.ensure();
        Self { paths }
    }

    fn read(path: &Path) -> Vec<HistoryEntry> {
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(HistoryEntry::parse)
            .collect()
    }

    fn append(path: &Path, entry: &HistoryEntry) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
        use std::io::Write;
        writeln!(file, "{}", entry.to_line())?;
        Ok(())
    }

    pub fn watch_history(&self) -> Vec<HistoryEntry> {
        Self::read(&self.paths.watch_history())
    }

    pub fn push_watch_history(&self, video: &Video) -> Result<()> {
        let entry = HistoryEntry::from_video(video);
        if !self
            .watch_history()
            .iter()
            .any(|e| e.url == entry.url)
        {
            Self::append(&self.paths.watch_history(), &entry)?;
        }
        Ok(())
    }

    pub fn clear_watch_history(&self) -> Result<()> {
        fs::write(self.paths.watch_history(), "")?;
        Ok(())
    }

    pub fn liked(&self) -> Vec<HistoryEntry> {
        let mut entries = Self::read(&self.paths.liked_history());
        entries.reverse();
        entries
    }

    pub fn is_liked(&self, url: &str) -> bool {
        Self::read(&self.paths.liked_history())
            .iter()
            .any(|e| e.url == url)
    }

    pub fn toggle_like(&self, video: &Video) -> Result<bool> {
        let path = self.paths.liked_history();
        let existing = Self::read(&path);
        if existing.iter().any(|e| e.url == video.url) {
            let kept: Vec<String> = existing
                .iter()
                .filter(|e| e.url != video.url)
                .map(|e| e.to_line())
                .collect();
            fs::write(&path, kept.join("\n"))?;
            Ok(false)
        } else {
            Self::append(&path, &HistoryEntry::from_video(video))?;
            Ok(true)
        }
    }

    pub fn search_history(&self) -> Vec<String> {
        let mut terms: Vec<String> = fs::read_to_string(self.paths.search_history())
            .unwrap_or_default()
            .lines()
            .map(|l| crate::yt::urldecode(l.trim()))
            .filter(|l| !l.is_empty())
            .collect();
        terms.reverse();
        terms.dedup();
        terms
    }

    pub fn push_search_history(&self, term: &str) -> Result<()> {
        let encoded = crate::yt::urlencode(term);
        let mut lines: Vec<String> = fs::read_to_string(self.paths.search_history())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty() && *l != encoded)
            .map(|l| l.to_string())
            .collect();
        lines.push(encoded);
        fs::write(self.paths.search_history(), lines.join("\n"))?;
        Ok(())
    }

    pub fn clear_search_history(&self) -> Result<()> {
        fs::write(self.paths.search_history(), "")?;
        Ok(())
    }

    pub fn last_action(&self) -> Option<LastAction> {
        let text = fs::read_to_string(self.paths.last_action()).ok()?;
        let mut lines = text.lines();
        Some(LastAction {
            query: lines.next()?.trim().to_string(),
            start: lines.next()?.trim().parse().unwrap_or(0),
            count: lines.next()?.trim().parse().unwrap_or(20),
            feed: lines.next()?.trim().to_string(),
            channel: lines.next().unwrap_or_default().trim().to_string(),
        })
    }

    pub fn save_last_action(&self, action: &LastAction) -> Result<()> {
        fs::write(
            self.paths.last_action(),
            format!(
                "{}\n{}\n{}\n{}\n{}\n",
                action.query, action.start, action.count, action.feed, action.channel
            ),
        )?;
        Ok(())
    }

    pub fn subscriptions(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.paths.subscriptions())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(2, ' ');
                let id = parts.next()?.trim();
                let name = parts.next().unwrap_or("").trim();
                if id.is_empty() {
                    None
                } else {
                    Some((id.to_string(), name.to_string()))
                }
            })
            .collect()
    }

    pub fn is_subscribed(&self, channel_id: &str) -> bool {
        !channel_id.is_empty()
            && self
                .subscriptions()
                .iter()
                .any(|(id, _)| id == channel_id)
    }

    pub fn subscribe(&self, channel_id: &str, name: &str) -> Result<()> {
        if channel_id.is_empty() || self.is_subscribed(channel_id) {
            return Ok(());
        }
        Self::append(
            &self.paths.subscriptions(),
            &HistoryEntry {
                channel_id: channel_id.to_string(),
                channel: name.to_string(),
                url: String::new(),
                title: String::new(),
            },
        )
    }

    pub fn unsubscribe(&self, name: &str) -> Result<()> {
        let path = self.paths.subscriptions();
        let kept: Vec<String> = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter(|line| {
                let display = line.split_once(' ').map(|x| x.1).unwrap_or("").trim();
                display != name && !line.trim().eq_ignore_ascii_case(name)
            })
            .map(|l| l.to_string())
            .collect();
        fs::write(&path, kept.join("\n"))?;
        Ok(())
    }

    pub fn replace_subscriptions(&self, entries: &[(String, String)]) -> Result<()> {
        let path = self.paths.subscriptions();
        if path.exists() {
            let existing = fs::read_to_string(&path).unwrap_or_default();
            if !existing.trim().is_empty() {
                fs::write(self.paths.subscriptions_backup(), existing)?;
            }
        }
        let body: Vec<String> = entries
            .iter()
            .filter(|(id, _)| !id.is_empty())
            .map(|(id, name)| format!("{id} {name}"))
            .collect();
        fs::write(&path, body.join("\n"))?;
        Ok(())
    }

    pub fn thumb_path(&self, id: &str) -> PathBuf {
        self.paths.thumb_dir.join(format!("img-{id}.jpg"))
    }

    /// Deletes the least recently used thumbnails until the cache fits.
    ///
    /// Without this the directory only ever grew: the one control was "clear
    /// everything", so a user who browses for a week ends up with a cache of
    /// every video they ever passed over.
    pub fn prune_thumbnails(&self, keep: usize, budget_bytes: u64) {
        let Ok(entries) = fs::read_dir(&self.paths.thumb_dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let meta = entry.metadata().ok()?;
                let modified = meta.modified().ok()?;
                Some((modified, meta.len(), entry.path()))
            })
            .collect();
        // Either bound can be the binding one, so both are checked before
        // deciding there is nothing to do.
        let mut bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
        if files.len() <= keep && bytes <= budget_bytes {
            return;
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let droppable = files.len().saturating_sub(keep);
        let mut dropped = 0usize;
        for (_, size, path) in &files {
            if dropped >= droppable && bytes <= budget_bytes {
                break;
            }
            bytes = bytes.saturating_sub(*size);
            fs::remove_file(path).ok();
            dropped += 1;
        }
        if dropped > 0 {
            log::info!("[store] thumbnail cache :: pruned {dropped} old files");
        }
    }

    pub fn clear_thumbnails(&self) -> Result<()> {
        if let Ok(entries) = fs::read_dir(&self.paths.thumb_dir) {
            for entry in entries.flatten() {
                fs::remove_file(entry.path()).ok();
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LastAction {
    pub query: String,
    pub start: usize,
    pub count: usize,
    pub feed: String,
    pub channel: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "mt-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        let paths = Paths {
            config_file: dir.join("config.toml"),
            data_dir: dir.join("data"),
            cache_dir: dir.join("cache"),
            thumb_dir: dir.join("cache/jpg"),
            json_dir: dir.join("cache/json"),
            comments_dir: dir.join("cache/comments"),
            history_dir: dir.join("cache/history"),
        };
        (Store::new(paths), dir)
    }

    #[test]
    fn thumbnails_are_pruned_oldest_first_and_only_when_needed() {
        let (store, dir) = temp_store("thumbs");
        let thumb_dir = store.paths.thumb_dir.clone();
        for i in 0..6 {
            std::fs::write(store.thumb_path(&format!("v{i}")), "x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(std::fs::read_dir(&thumb_dir).unwrap().count(), 6);

        store.prune_thumbnails(4, u64::MAX);
        let left: Vec<String> = std::fs::read_dir(&thumb_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left.len(), 4, "the count cap is respected");
        assert!(
            left.contains(&"img-v5.jpg".to_string()),
            "the newest survive: {left:?}"
        );
        assert!(
            !left.contains(&"img-v0.jpg".to_string()),
            "the oldest go first: {left:?}"
        );

        store.prune_thumbnails(10, u64::MAX);
        assert_eq!(
            std::fs::read_dir(&thumb_dir).unwrap().count(),
            4,
            "a limit above the count must not delete anything"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_byte_budget_can_prune_before_the_count() {
        let (store, dir) = temp_store("thumbs");
        let thumb_dir = store.paths.thumb_dir.clone();
        for i in 0..5 {
            std::fs::write(store.thumb_path(&format!("b{i}")), "0123456789").unwrap();
        }
        store.prune_thumbnails(100, 15);
        let left = std::fs::read_dir(&thumb_dir)
            .unwrap()
            .count();
        assert!(left < 5, "a tight byte budget trims even under the count");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn video(title: &str, id: &str) -> Video {
        Video {
            id: id.into(),
            title: title.into(),
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
    fn likes_are_idempotent() {
        let (store, dir) = temp_store("likes");
        assert!(store.toggle_like(&video("A", "a")).unwrap());
        assert!(store.is_liked("https://www.youtube.com/watch?v=a"));
        assert!(!store.toggle_like(&video("A", "a")).unwrap());
        assert!(!store.is_liked("https://www.youtube.com/watch?v=a"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn subscriptions_roundtrip() {
        let (store, dir) = temp_store("subs");
        store.subscribe("UC1", "Chan").unwrap();
        store.subscribe("UC1", "Chan").unwrap();
        store.subscribe("UC2", "Other").unwrap();
        assert_eq!(store.subscriptions().len(), 2);
        store.unsubscribe("Other").unwrap();
        assert_eq!(store.subscriptions(), vec![("UC1".into(), "Chan".into())]);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn history_entries_roundtrip() {
        let entry = HistoryEntry::from_video(&video("Hello", "xyz"));
        let parsed = HistoryEntry::parse(&entry.to_line()).unwrap();
        assert_eq!(parsed.title, "Hello");
        assert_eq!(parsed.video_id(), "xyz");
    }

    #[test]
    fn search_history_is_deduped_and_reversed() {
        let (store, dir) = temp_store("search");
        store.push_search_history("cats").unwrap();
        store.push_search_history("dogs").unwrap();
        store.push_search_history("cats").unwrap();
        assert_eq!(store.search_history(), vec!["cats".to_string(), "dogs".to_string()]);
        fs::remove_dir_all(dir).ok();
    }
}
