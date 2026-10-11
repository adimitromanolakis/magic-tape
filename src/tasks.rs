use std::path::PathBuf;
use std::sync::mpsc::Sender;

use anyhow::{Context, Result};

/// Handles work that must not block the UI thread.
///
/// Workers own everything they need (`YtDlp`, `Store`, `Config` clones) and
/// report back over a channel; the app drains the receiver once per frame.
pub enum Outcome {
    FeedLoaded {
        feed: crate::model::Feed,
        channel: String,
        query: Option<String>,
        start: usize,
        videos: Vec<crate::model::Video>,
    },
    PlaylistsLoaded {
        query: String,
        start: usize,
        videos: Vec<crate::model::Video>,
    },
    ChannelsLoaded {
        query: String,
        start: usize,
        channels: Vec<crate::model::Channel>,
    },
    SubscriptionsImported {
        count: usize,
        backup: Option<String>,
    },
    CommentsLoaded {
        url: String,
        description: String,
        comments: Vec<crate::model::Comment>,
        count: u64,
        pinned_first: bool,
    },
    DownloadFinished {
        title: String,
        audio_only: bool,
    },
    YtDlpUpdated {
        version: String,
    },
    ThumbnailReady {
        id: String,
        path: PathBuf,
    },
    /// A thumbnail that cannot be fetched at all - a custom thumbnail with no
    /// public variant. Not worth interrupting the user over.
    ThumbnailMissing {
        id: String,
        error: String,
    },
    Failed {
        label: String,
        error: String,
    },
}

pub struct Worker {
    tx: Sender<Outcome>,
    rx: std::sync::mpsc::Receiver<Outcome>,
    inflight: usize,
}

impl Worker {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            tx,
            rx,
            inflight: 0,
        }
    }

    pub fn spawn<F>(&mut self, label: &'static str, work: F)
    where
        F: FnOnce() -> Outcome + Send + 'static,
    {
        let tx = self.tx.clone();
        self.inflight += 1;
        std::thread::spawn(move || {
            let outcome = work();
            log::debug!("job finished: {label}");
            let _ = tx.send(outcome);
        });
    }

    pub fn drain(&mut self) -> Vec<Outcome> {
        let mut out = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            self.inflight = self.inflight.saturating_sub(1);
            out.push(result);
        }
        out
    }

    pub fn busy(&self) -> bool {
        self.inflight > 0
    }
}

/// One HTTP client for the whole process.
///
/// Building a `reqwest` client per download meant a fresh TLS handshake and
/// connection pool for every thumbnail; a page of 20 videos created 20 of
/// them. `OnceLock` is set on first use and shared by all worker threads.
fn thumbnail_client() -> Result<&'static reqwest::blocking::Client> {
    static CLIENT: std::sync::OnceLock<Result<reqwest::blocking::Client>> =
        std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .user_agent("Mozilla/5.0 (X11; Linux x86_64) magic-tape")
                .build()
                .context("building the thumbnail HTTP client")
        })
        .as_ref()
        .map_err(|_| anyhow::anyhow!("the thumbnail HTTP client could not be built"))
}

pub fn download_thumbnail(url: &str, dest: PathBuf) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    // The URL yt-dlp hands over is frequently unreachable: a custom thumbnail is
    // owner-only, and the high-resolution variants are simply absent on older
    // uploads. Walk the public variants and keep the first that answers, so a
    // card shows a picture instead of a stack of 404s.
    let mut last = None;
    for candidate in crate::thumbs::public_variants(url) {
        match fetch_thumbnail(&candidate) {
            Ok(bytes) => return write_thumbnail(dest, bytes),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no thumbnail candidate for {url}")))
}

/// Reads a channel's avatar out of its page.
///
/// Channel pictures are not on `i.ytimg`, and the subscriptions file keeps
/// only an id, so when a channel has no avatar URL of its own the page is the
/// one place that still publishes it. The result is cached as an image file, so
/// this runs once per channel rather than once per visit.
pub fn download_channel_avatar(id: &str, dest: PathBuf) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    let page = fetch_text(&format!("https://www.youtube.com/channel/{id}"))?;
    let url = og_image(&page).ok_or_else(|| anyhow::anyhow!("no avatar published for {id}"))?;
    let bytes = fetch_thumbnail(&url)?;
    write_thumbnail(dest, bytes)
}

/// The `og:image` of a channel page.
pub fn og_image(html: &str) -> Option<String> {
    let marker = r#"<meta property="og:image" content=""#;
    let start = html.find(marker)? + marker.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn fetch_text(url: &str) -> Result<String> {
    Ok(thumbnail_client()?
        .get(url)
        .send()?
        .error_for_status()?
        .text()?)
}

fn fetch_thumbnail(url: &str) -> Result<Vec<u8>> {
    Ok(thumbnail_client()?
        .get(url)
        .send()?
        .error_for_status()?
        .bytes()?
        .to_vec())
}

fn write_thumbnail(dest: PathBuf, bytes: Vec<u8>) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(())
}
#[cfg(test)]
mod avatar_tests {
    use super::og_image;

    #[test]
    fn reads_the_avatar_out_of_a_channel_page() {
        let html = r#"<head><meta property="og:title" content="3Blue1Brown">
            <meta property="og:image" content="https://yt3.googleusercontent.com/ytc/AIdro_nFz=s900-c-k-c0x00ffffff-no-rj">
            <meta property="og:url" content="https://www.youtube.com/channel/UC1"></head>"#;
        assert_eq!(
            og_image(html).as_deref(),
            Some("https://yt3.googleusercontent.com/ytc/AIdro_nFz=s900-c-k-c0x00ffffff-no-rj")
        );
    }

    #[test]
    fn a_page_without_one_is_none() {
        assert_eq!(og_image("<html><body>nothing here</body></html>"), None);
        assert_eq!(og_image(""), None);
    }
}
