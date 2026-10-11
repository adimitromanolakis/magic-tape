use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

pub struct ThumbCache {
    textures: HashMap<String, TextureHandle>,
    pending: HashMap<String, String>,
    /// Ids with no public thumbnail. Asking again would only 404 again.
    failed: HashSet<String>,
    placeholder: TextureHandle,
}

impl ThumbCache {
    pub fn new(ctx: &Context) -> Self {
        Self {
            textures: HashMap::new(),
            pending: HashMap::new(),
            failed: HashSet::new(),
            placeholder: load_placeholder(ctx),
        }
    }

    pub fn get(&self, id: &str) -> TextureHandle {
        self.textures.get(id).cloned().unwrap_or_else(|| self.placeholder.clone())
    }

    /// Whether a real picture has arrived, so the UI can draw an initial
    /// instead of the placeholder while it is still missing.
    pub fn has(&self, id: &str) -> bool {
        self.textures.contains_key(id)
    }

    pub fn insert(&mut self, ctx: &Context, id: &str, path: PathBuf) {
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(source) = image::load_from_memory(&bytes).map(|i| i.into_rgba8()) {
                let (width, height) = source.dimensions();
                let size = [width as usize, height as usize];
                let image = ColorImage::from_rgba_unmultiplied(size, source.as_raw());
                let texture = ctx.load_texture(
                    format!("thumb-{id}"),
                    image,
                    TextureOptions::LINEAR,
                );
                self.textures.insert(id.to_string(), texture);
            }
    }

    pub fn request_pending(&mut self, id: &str) -> bool {
        !self.textures.contains_key(id)
            && !self.pending.contains_key(id)
            && !self.failed.contains(id)
    }

    pub fn mark_pending(&mut self, id: &str, url: String) {
        self.pending.insert(id.to_string(), url);
    }

    pub fn clear_pending(&mut self, id: &str) {
        self.pending.remove(id);
    }

    /// Remembers that an id has no reachable thumbnail, so the placeholder
    /// stays put instead of the request being retried on every frame.
    pub fn mark_failed(&mut self, id: &str) {
        self.failed.insert(id.to_string());
    }

    pub fn forget_all(&mut self) {
        self.textures.clear();
        self.pending.clear();
        self.failed.clear();
    }

    pub fn cached_count(&self) -> usize {
        self.textures.len()
    }
}

/// The video id in an `i.ytimg.com` thumbnail URL, if that is what it is.
pub fn video_id_of(url: &str) -> Option<&str> {
    let path = url.split('?').next().unwrap_or(url);
    let rest = path.split_once("/vi/")?.1;
    let id = rest.split('/').next().unwrap_or_default();
    (!id.is_empty()).then_some(id)
}

/// Whether a URL is one of YouTube's owner-only "custom" thumbnails.
///
/// Those are served to the uploader and their subscribers, not to anonymous
/// requests: asking for one is a guaranteed 404. A video whose owner uploaded
/// a custom thumbnail has no public variant at all, which is a different thing
/// and only a runtime fallback can discover.
pub fn is_custom_thumbnail(url: &str) -> bool {
    let path = url.split('?').next().unwrap_or(url);
    path.rsplit('/').next().is_some_and(|f| f.contains("_custom_"))
}

/// Candidate URLs to try, best first, for a thumbnail that may be unreachable.
///
/// The named variants are tried from sharpest to smallest; `mqdefault` is last
/// because it is 16:9 - the shape the cards use - and the one YouTube keeps
/// published most reliably.
pub fn public_variants(url: &str) -> Vec<String> {
    let Some(id) = video_id_of(url) else {
        return vec![url.to_string()];
    };
    ["maxresdefault.jpg", "hq720.jpg", "hqdefault.jpg", "mqdefault.jpg"]
        .iter()
        .map(|name| format!("https://i.ytimg.com/vi/{id}/{name}"))
        .collect()
}

/// A channel avatar URL we can actually fetch, or `None` if there is not one.
///
/// Two traps live here. YouTube hands out avatar URLs protocol-relative
/// (`//yt3.googleusercontent.com/...`), which a plain GET cannot resolve, and
/// a *video* thumbnail on `i.ytimg.com` is not a channel picture at all - the
/// subscriptions file stores only an id, and `i.ytimg.com/vi/<UC...>` 404s for
/// every channel. Neither is worth requesting, so both become "no picture".
pub fn channel_avatar_url(raw: Option<String>) -> Option<String> {
    let url = raw?;
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("//") {
        return Some(format!("https:{trimmed}"));
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return None;
    }
    let path = trimmed.split('?').next().unwrap_or(trimmed);
    (!path.contains("i.ytimg.com/vi/")).then(|| trimmed.to_string())
}

fn load_placeholder(ctx: &Context) -> TextureHandle {
    let (w, h) = (320u32, 180u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let stripe = ((x / 16 + y / 16) % 2) == 0;
            let base = if stripe { 34 } else { 40 };
            pixels.extend_from_slice(&[base, base + 4, base + 10, 255]);
        }
    }
    let image = ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &pixels);
    ctx.load_texture("placeholder", image, TextureOptions::NEAREST)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_video_id_out_of_a_thumbnail_url() {
        assert_eq!(
            video_id_of("https://i.ytimg.com/vi/Zper_lsRofk/hq720_custom_2.jpg"),
            Some("Zper_lsRofk")
        );
        assert_eq!(
            video_id_of("https://i.ytimg.com/vi/abc123/hq720.jpg?sqp=xyz&rs=1"),
            Some("abc123"),
            "the signed query string is not part of the id"
        );
        assert_eq!(video_id_of("https://example.com/thumb.png"), None);
    }

    #[test]
    fn spotts_the_owner_only_variants() {
        assert!(is_custom_thumbnail(
            "https://i.ytimg.com/vi/Zper_lsRofk/hq720_custom_2.jpg"
        ));
        assert!(is_custom_thumbnail(
            "https://i.ytimg.com/vi/Zper_lsRofk/hq720_custom_2.jpg?sqp=x"
        ));
        assert!(!is_custom_thumbnail(
            "https://i.ytimg.com/vi/Zper_lsRofk/hq720.jpg"
        ));
        assert!(!is_custom_thumbnail(
            "https://i.ytimg.com/vi/Zper_lsRofk/mqdefault.jpg"
        ));
    }

    #[test]
    fn variants_are_the_same_video_sharpest_first() {
        let variants = public_variants("https://i.ytimg.com/vi/Zper_lsRofk/hq720_custom_2.jpg");
        assert_eq!(
            variants,
            vec![
                "https://i.ytimg.com/vi/Zper_lsRofk/maxresdefault.jpg",
                "https://i.ytimg.com/vi/Zper_lsRofk/hq720.jpg",
                "https://i.ytimg.com/vi/Zper_lsRofk/hqdefault.jpg",
                "https://i.ytimg.com/vi/Zper_lsRofk/mqdefault.jpg",
            ]
        );
        assert!(
            variants.last().unwrap().ends_with("mqdefault.jpg"),
            "the 16:9 fallback must be tried last so it is the safety net"
        );
    }

    #[test]
    fn a_url_from_elsewhere_is_left_alone() {
        let url = "https://example.com/thumb.png";
        assert_eq!(public_variants(url), vec![url.to_string()]);
    }

    #[test]
    fn a_failed_id_is_never_asked_for_again() {
        let ctx = Context::default();
        let mut cache = ThumbCache::new(&ctx);
        assert!(cache.request_pending("abc"));
        cache.mark_pending("abc", "https://example.com/x.jpg".into());
        assert!(!cache.request_pending("abc"), "already in flight");
        cache.clear_pending("abc");
        assert!(cache.request_pending("abc"), "free to try again");
        cache.mark_failed("abc");
        assert!(!cache.request_pending("abc"), "known dead, stop asking");
        cache.forget_all();
        assert!(cache.request_pending("abc"), "a cache wipe forgets the dead too");
    }
}

#[cfg(test)]
mod avatar_tests {
    use super::*;

    #[test]
    fn protocol_relative_avatars_get_a_scheme() {
        // What yt-dlp hands over for a searched channel; a bare GET on this
        // fails with "relative URL without a base".
        assert_eq!(
            channel_avatar_url(Some(
                "//yt3.googleusercontent.com/ytc/AIdro_nFz=s176-c-k-c0x00ffffff-no-rj".into()
            )),
            Some("https://yt3.googleusercontent.com/ytc/AIdro_nFz=s176-c-k-c0x00ffffff-no-rj".into())
        );
    }

    #[test]
    fn a_video_thumbnail_is_not_a_channel_avatar() {
        // i.ytimg.com/vi/<UC...> 404s for every channel id.
        assert_eq!(
            channel_avatar_url(Some("https://i.ytimg.com/vi/UCYO_jab/mqdefault.jpg".into())),
            None
        );
        assert_eq!(
            channel_avatar_url(Some("https://i.ytimg.com/vi/abc/mqdefault.jpg?sqp=x".into())),
            None
        );
    }

    #[test]
    fn nothing_to_fetch_stays_nothing_to_fetch() {
        assert_eq!(channel_avatar_url(None), None);
        assert_eq!(channel_avatar_url(Some("  ".into())), None);
        assert_eq!(channel_avatar_url(Some("garbage".into())), None);
    }

    #[test]
    fn a_real_avatar_passes_through_untouched() {
        let url = "https://yt3.googleusercontent.com/ytc/AIdro_nFz=s900-c-k-c0x00ffffff-no-rj";
        assert_eq!(channel_avatar_url(Some(url.into())), Some(url.into()));
    }
}
