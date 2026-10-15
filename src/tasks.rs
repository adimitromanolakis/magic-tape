use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

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
    /// A comment fetch that failed, tagged with its url so the in-flight guard
    /// can be released even when nothing arrives.
    CommentsFailed {
        url: String,
        error: String,
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

/// What a job spends a permit on.
///
/// yt-dlp runs are heavy Python processes; thumbnail fetches are cheap HTTP
/// requests. Keeping them in separate pools means a page of thumbnails cannot
/// crowd out a feed load, and neither can starve the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Ytdlp,
    Thumbnail,
    /// For jobs that cost nothing worth capping (cache reads, small file work).
    #[allow(dead_code)]
    Unmetered,
}

/// A counting semaphore.
///
/// Waiting happens *inside* the worker thread, never before spawning: a
/// blocked UI thread would freeze the window the user is trying to type into.
#[derive(Debug)]
pub struct Permits {
    state: Mutex<(usize, usize)>,
    space: Condvar,
}

impl Permits {
    /// `limit` slots, `in_use` running.
    fn new(limit: usize) -> Self {
        Self {
            state: Mutex::new((limit.max(1), 0)),
            space: Condvar::new(),
        }
    }

    /// Changes the limit while jobs are waiting.
    pub fn set_limit(&self, limit: usize) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0 = limit.max(1);
        drop(state);
        self.space.notify_all();
    }

    #[allow(dead_code)]
    pub fn limit(&self) -> usize {
        self.state.lock().map(|s| s.0).unwrap_or(1)
    }

    fn acquire(self: &Arc<Self>) -> Permit {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.1 >= state.0 {
            state = self
                .space
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
        state.1 += 1;
        Permit {
            pool: Arc::clone(self),
        }
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.1 = state.1.saturating_sub(1);
        drop(state);
        self.space.notify_one();
    }
}

struct Permit {
    pool: Arc<Permits>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.pool.release();
    }
}

pub struct Worker {
    tx: Sender<Outcome>,
    rx: std::sync::mpsc::Receiver<Outcome>,
    inflight: usize,
    ytdlp: Arc<Permits>,
    thumbnails: Arc<Permits>,
}

impl Worker {
    pub fn new(ytdlp_limit: usize, thumbnail_limit: usize) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            tx,
            rx,
            inflight: 0,
            ytdlp: Arc::new(Permits::new(ytdlp_limit)),
            thumbnails: Arc::new(Permits::new(thumbnail_limit)),
        }
    }

    /// Applies new limits from the settings without a restart.
    pub fn set_limits(&mut self, ytdlp: usize, thumbnails: usize) {
        self.ytdlp.set_limit(ytdlp);
        self.thumbnails.set_limit(thumbnails);
    }

    #[allow(dead_code)]
    pub fn limits(&self) -> (usize, usize) {
        (self.ytdlp.limit(), self.thumbnails.limit())
    }

    fn pool(&self, kind: JobKind) -> Option<Arc<Permits>> {
        match kind {
            JobKind::Ytdlp => Some(Arc::clone(&self.ytdlp)),
            JobKind::Thumbnail => Some(Arc::clone(&self.thumbnails)),
            JobKind::Unmetered => None,
        }
    }

    pub fn spawn<F>(&mut self, label: &'static str, kind: JobKind, work: F)
    where
        F: FnOnce() -> Outcome + Send + 'static,
    {
        let tx = self.tx.clone();
        let pool = self.pool(kind);
        self.inflight += 1;
        std::thread::spawn(move || {
            // Taken here, on the worker, so waiting never touches the UI thread.
            let _permit = pool.as_ref().map(|pool| pool.acquire());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn pool(limit: usize) -> Arc<Permits> {
        Arc::new(Permits::new(limit))
    }

    #[test]
    fn a_permit_is_returned_when_the_job_finishes() {
        let pool = pool(2);
        {
            let _first = pool.acquire();
            let _second = pool.acquire();
        }
        let _third = pool.acquire();
    }

    #[test]
    fn the_pool_holds_the_line_at_its_limit() {
        let pool = pool(6);
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(std::sync::Barrier::new(6));
        let mut handles = Vec::new();
        for _ in 0..24 {
            let pool = Arc::clone(&pool);
            let running = Arc::clone(&running);
            let peak = Arc::clone(&peak);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let _permit = pool.acquire();
                let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                barrier.wait();
                running.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(
            peak.load(Ordering::SeqCst),
            6,
            "never more than the configured number of jobs at once"
        );
        assert_eq!(running.load(Ordering::SeqCst), 0, "every permit came back");
    }

    #[test]
    fn raising_the_limit_lets_waiting_jobs_through() {
        let pool = pool(1);
        let held = pool.acquire();
        let done = Arc::new(AtomicUsize::new(0));
        let waiter = {
            let pool = Arc::clone(&pool);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                let _permit = pool.acquire();
                done.store(1, Ordering::SeqCst);
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(done.load(Ordering::SeqCst), 0, "blocked while the limit holds");
        pool.set_limit(2);
        waiter.join().unwrap();
        assert_eq!(done.load(Ordering::SeqCst), 1);
        drop(held);
    }

    #[test]
    fn a_limit_of_zero_is_treated_as_one() {
        let pool = pool(0);
        assert_eq!(pool.limit(), 1, "a zero would deadlock every job");
    }
}
