//! System icons, resolved through the freedesktop icon theme spec.
//!
//! Icon names are looked up the way a desktop toolkit does: the user's theme
//! first (from `gtk-4.0`/`gtk-3.0` settings, or `XDG_ICON_THEME`), then the
//! themes it inherits from, then `hicolor` as the floor. Both SVG and PNG are
//! supported - on a modern desktop almost every theme is SVG, so PNG alone
//! would silently fall back to the legacy folder pictures.
//!
//! Nothing here is allowed to fail loudly: an icon that cannot be found simply
//! draws nothing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// Where an icon lives, and how big it will be drawn.
pub struct IconTheme {
    dirs: Vec<PathBuf>,
    cache: HashMap<(String, u32), Option<TextureHandle>>,
}

impl IconTheme {
    pub fn new(ctx: &Context) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let theme = preferred_theme(&home);
        let dirs = search_dirs(&home, &theme);
        log::info!(
            "[icons] theme {theme} resolved to {} directories",
            dirs.len()
        );
        let _ = ctx;
        Self {
            dirs,
            cache: HashMap::new(),
        }
    }

    /// The icon for `name` at `size`, cached after the first hit.
    pub fn get(&mut self, ctx: &Context, name: &str, size: u32) -> Option<TextureHandle> {
        let key = (name.to_string(), size);
        if let Some(hit) = self.cache.get(&key) {
            return hit.clone();
        }
        let loaded = self
            .locate(name, size)
            .and_then(|path| load_texture(ctx, &path, size));
        if loaded.is_none() {
            log::debug!("[icons] no icon named {name} at {size}px");
        }
        self.cache.insert(key, loaded.clone());
        loaded
    }

    /// The first of `names` that the theme actually provides.
    ///
    /// Icon names are a suggestion, not a guarantee: `go-up` is GTK, `go-previous`
    /// is what other toolkits call it, and a minimal theme ships neither. Asking
    /// for a short chain is the difference between an icon and a blank.
    pub fn first_of(&mut self, ctx: &Context, names: &[&str], size: u32) -> Option<TextureHandle> {
        names
            .iter()
            .find_map(|name| self.get(ctx, name, size))
    }

    /// The first existing file for `name`, or `None`.
    pub fn locate(&self, name: &str, size: u32) -> Option<PathBuf> {
        for dir in &self.dirs {
            for relative in icon_candidates(name, size) {
                let candidate = dir.join(&relative);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }
}

/// The icon theme to look in first.
pub fn preferred_theme(home: &Path) -> String {
    if let Ok(name) = std::env::var("XDG_ICON_THEME")
        && !name.trim().is_empty()
    {
        return name;
    }
    for settings in [
        home.join(".config/gtk-4.0/settings.ini"),
        home.join(".config/gtk-3.0/settings.ini"),
    ] {
        if let Some(name) = read_setting(&settings, "gtk-icon-theme-name") {
            return name;
        }
    }
    "Adwaita".into()
}

/// One value out of an ini file, comments and all.
pub fn read_setting(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key && !value.trim().is_empty()).then(|| value.trim().to_string())
        })
}

/// Themes the given one inherits from, in order.
pub fn inherited_themes(index_theme: &str) -> Vec<String> {
    index_theme
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect()
}

/// Directories to search, in priority order.
///
/// `index.theme` decides the inheritance chain, and the walk stops as soon as
/// a theme turns up a file - otherwise a theme with a partial index would
/// shadow the ones it inherits from.
pub fn search_dirs(home: &Path, theme: &str) -> Vec<PathBuf> {
    let roots = [
        home.join(".icons"),
        home.join(".local/share/icons"),
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/local/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
    ];
    let mut queue = vec![theme.to_string()];
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<PathBuf> = Vec::new();

    while let Some(current) = if queue.is_empty() {
        None
    } else {
        Some(queue.remove(0))
    } {
        if seen.contains(&current) {
            continue;
        }
        seen.push(current.clone());

        let mut theme_dirs: Vec<PathBuf> = Vec::new();
        for root in &roots {
            let dir = root.join(&current);
            if dir.is_dir() {
                theme_dirs.push(dir);
            }
            if root.ends_with("pixmaps") && current == "pixmaps" {
                theme_dirs.push(root.to_path_buf());
            }
        }
        if theme_dirs.is_empty() {
            continue;
        }

        out.extend(theme_dirs.iter().cloned());
        let index = theme_dirs[0].join("index.theme");
        if let Ok(text) = std::fs::read_to_string(&index) {
            queue.extend(inherited_themes(&setting(&text, "Inherits").unwrap_or_default()));
        }
    }

    // `hicolor` is the specification's floor, and a Qt theme like Qogir
    // inherits nothing from Adwaita - so GTK-only names (`go-up`, `folder-new`)
    // would otherwise be unreachable. Walk the toolkits' own defaults last,
    // unless the theme already got there through inheritance.
    for fallback in ["hicolor", "Adwaita", "AdwaitaLegacy"] {
        if seen.iter().any(|name| name == fallback) {
            continue;
        }
        seen.push(fallback.to_string());
        for root in roots.iter().take(3) {
            let dir = root.join(fallback);
            if dir.is_dir() {
                out.push(dir);
            }
        }
    }
    out
}

fn setting(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key && !value.trim().is_empty()).then(|| value.trim().to_string())
        })
}

/// Files to try for one icon name, best first.
///
/// Nearest size wins in each direction, then `scalable`, then the contexts a
/// file manager would use.
pub fn icon_candidates(name: &str, size: u32) -> Vec<String> {
    // `devices` holds the drives, `legacy` and `symbolic` are where older and
    // symbolic themes keep them - leaving these out is why a drive icon can be
    // missing on a desktop that plainly has one.
    const CONTEXTS: [&str; 7] = [
        "places",
        "apps",
        "devices",
        "status",
        "actions",
        "symbolic/actions",
        "legacy",
    ];
    let mut sizes = sizes_from(size);
    sizes.push(0); // the scalable directory, last
    let sizes = &sizes;

    let mut out = Vec::new();
    for context in CONTEXTS {
        for scale in sizes.iter().copied() {
            for extension in ["svg", "png"] {
                if scale == 0 {
                    // Symbolic and legacy themes keep their files directly in
                    // the context directory, with no size folder at all.
                    out.push(format!("{context}/{name}.svg"));
                    out.push(format!("scalable/{context}/{name}.svg"));
                } else {
                    out.push(format!("{scale}x{scale}/{context}/{name}.{extension}"));
                    if scale != size {
                        // High-DPI variants sit in `NNxNN@2`.
                        out.push(format!("{scale}x{scale}@2/{context}/{name}.{extension}"));
                    }
                }
            }
        }
    }
    out
}

/// Sizes to try: the one asked for first, then the rest of the ladder nearest
/// to it. Exact first matters - a 24px request should not quietly get the
/// 16px picture and have it scaled up.
fn sizes_from(size: u32) -> Vec<u32> {
    const LADDER: [u32; 7] = [16, 22, 24, 32, 48, 64, 128];
    let mut rest: Vec<u32> = LADDER
        .iter()
        .copied()
        .filter(|candidate| *candidate != size)
        .collect();
    rest.sort_by_key(|candidate| candidate.abs_diff(size));
    let mut out = vec![size];
    out.extend(rest);
    out
}

fn load_texture(ctx: &Context, path: &Path, size: u32) -> Option<TextureHandle> {
    let bytes = std::fs::read(path).ok()?;
    let image = match path.extension().and_then(|e| e.to_str()) {
        Some("svg") => rasterise_svg(&bytes, size)?,
        _ => decode(&bytes)?,
    };
    Some(ctx.load_texture(path.display().to_string(), image, TextureOptions::LINEAR))
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (width, height) = decoded.dimensions();
    Some(ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        decoded.as_raw(),
    ))
}

/// Rasterise an SVG at the requested pixel size.
fn rasterise_svg(bytes: &[u8], size: u32) -> Option<ColorImage> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let scale = size as f32 / tree.size().width().max(1.0);
    let width = (tree.size().width() * scale).round().max(1.0) as u32;
    let height = (tree.size().height() * scale).round().max(1.0) as u32;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).or_else(|| tiny_skia::Pixmap::new(32, 32))?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Some(ColorImage::from_rgba_unmultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_setting_and_ignores_comments() {
        let text = "# a comment\ngtk-icon-theme-name=Qogir\nother=1\n";
        assert_eq!(setting(text, "gtk-icon-theme-name").as_deref(), Some("Qogir"));
        assert_eq!(setting(text, "missing"), None);
        assert_eq!(setting("", "anything"), None);
    }

    #[test]
    fn inheritance_chains_are_split_and_trimmed() {
        assert_eq!(
            inherited_themes("Adwaita-dark,Adwaita,hicolor"),
            vec!["Adwaita-dark", "Adwaita", "hicolor"]
        );
        assert!(inherited_themes("").is_empty());
        assert_eq!(inherited_themes("hicolor"), vec!["hicolor"]);
    }

    #[test]
    fn candidates_try_the_asked_size_first_and_scalable_last() {
        let candidates = icon_candidates("folder", 24);
        assert_eq!(candidates[0], "24x24/places/folder.svg");
        assert!(
            candidates
                .last()
                .is_some_and(|last| last.starts_with("scalable/")),
            "scalable is the last resort: {candidates:?}"
        );
        let last_common = candidates
            .iter()
            .rposition(|c| c.contains("/places/") || c.contains("/apps/"))
            .unwrap_or_default();
        let first_exotic = candidates
            .iter()
            .position(|c| c.contains("/legacy/") || c.contains("/symbolic/"))
            .unwrap_or(usize::MAX);
        assert!(
            first_exotic > last_common,
            "places and apps are exhausted before legacy or symbolic: {candidates:?}"
        );
        assert!(
            candidates.iter().any(|c| c.contains("/devices/")),
            "drives live in the devices context"
        );
    }

    #[test]
    fn sizeless_symbolic_directories_are_tried() {
        let candidates = icon_candidates("object-select-symbolic", 16);
        assert!(
            candidates.contains(&"symbolic/actions/object-select-symbolic.svg".to_string()),
            "Adwaita keeps symbolic icons with no size folder: {candidates:?}"
        );
    }

    #[test]
    fn an_unusual_size_still_gets_a_candidate() {
        let candidates = icon_candidates("folder", 40);
        assert!(
            candidates.contains(&"40x40/places/folder.svg".to_string()),
            "the exact size is tried even when it is not on the ladder"
        );
    }

    #[test]
    fn a_missing_theme_still_falls_back_to_the_toolkit_defaults() {
        // A theme that is not installed must not leave the picker iconless.
        let home = PathBuf::from("/definitely/not/a/home");
        let dirs = search_dirs(&home, "no-such-theme-xyz");
        let names: Vec<String> = dirs
            .iter()
            .filter_map(|dir| dir.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec!["hicolor", "Adwaita", "AdwaitaLegacy"],
            "the fallbacks a desktop toolkit would use"
        );
    }

    #[test]
    fn search_order_follows_inheritance() {
        // A tiny fake icon tree: `Fake` inherits `Base`, which inherits
        // `hicolor`.
        let root = std::env::temp_dir().join("magic-tape-icons-test");
        let _ = std::fs::remove_dir_all(&root);
        for (theme, inherits) in [("Fake", "Base"), ("Base", "hicolor"), ("hicolor", "")] {
            // Under `~/.icons`, which is where a user theme would live.
            let dir = root.join(".icons").join(theme);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("index.theme"),
                format!("[Icon Theme]\nInherits={inherits}\n"),
            )
            .unwrap();
        }
        let dirs = search_dirs(&root, "Fake");
        let names: Vec<String> = dirs
            .iter()
            .filter_map(|d| d.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            &names[..3],
            ["Fake", "Base", "hicolor"],
            "in inheritance order; hicolor appears twice because it is searched \
             in both the user and the system root, user first"
        );
        assert!(
            search_dirs(&root, "Fake").contains(&PathBuf::from("/usr/share/icons/hicolor")),
            "the system root is searched after the user's own"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_users_gtk_theme_wins_over_the_default() {
        let home = std::env::temp_dir().join("magic-tape-icon-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".config/gtk-4.0")).unwrap();
        std::fs::write(
            home.join(".config/gtk-4.0/settings.ini"),
            "[Settings]\ngtk-icon-theme-name=Breeze\n",
        )
        .unwrap();
        assert_eq!(preferred_theme(&home), "Breeze");
        let _ = std::fs::remove_dir_all(&home);
    }
}
#[cfg(test)]
mod live_tests {
    use super::*;

    /// Opt-in: reports which of the names the folder picker asks for the
    /// installed theme actually provides.
    /// Run with: `cargo test -- --ignored --nocapture live_icon_names`
    #[test]
    #[ignore = "depends on the installed icon themes"]
    fn live_icon_names() {
        let ctx = egui::Context::default();
        let theme = IconTheme::new(&ctx);
        println!("searching {} directories for theme", theme.dirs.len());
        for name in [
            "user-home",
            "user-desktop",
            "folder-downloads",
            "folder-documents",
            "folder-videos",
            "folder-music",
            "folder-pictures",
            "filesystem",
            "folder",
            "folder-new",
            "go-up",
            "drive-harddisk",
            "object-select-symbolic",
            "dialog-ok",
        ] {
            match theme.locate(name, 16) {
                Some(path) => println!("{name:24} -> {}", path.display()),
                None => println!("{name:24} -> MISSING"),
            }
        }
    }
}
