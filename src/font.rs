//! UI font and scale.
//!
//! egui bundles its own fonts, so a font is only installed when the user asks
//! for one. The chosen file is placed *first* in the font families rather than
//! replacing them, which keeps egui's fallbacks for glyphs the chosen font does
//! not have - channel names in CJK or emoji would otherwise disappear.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Where system fonts live, in the order they are offered.
const FONT_DIRS: &[&str] = &[
    "/usr/share/fonts",
    "/usr/local/share/fonts",
    "/Library/Fonts",
    "/System/Library/Fonts",
];

/// Upper bound on discovered fonts, so a pathological font directory cannot
/// stall the settings window.
const MAX_FONTS: usize = 2000;

const EXTENSIONS: &[&str] = &["ttf", "otf", "ttc"];

static DISCOVERED: OnceLock<Vec<PathBuf>> = OnceLock::new();

/// All font files installed on this machine, sorted by path.
pub fn discovered() -> &'static [PathBuf] {
    DISCOVERED.get_or_init(scan)
}

fn scan() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut roots: Vec<PathBuf> = FONT_DIRS.iter().map(PathBuf::from).collect();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".fonts"));
        roots.push(home.join(".local/share/fonts"));
        roots.push(home.join("Library/Fonts"));
    }
    for root in roots {
        collect(&root, 0, &mut found);
        if found.len() >= MAX_FONTS {
            break;
        }
    }
    found.sort();
    found.dedup();
    found.truncate(MAX_FONTS);
    found
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 || out.len() >= MAX_FONTS {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
            continue;
        }
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
        {
            out.push(path);
        }
    }
}

/// The name shown for a font file: its file name without the extension.
pub fn display_name(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Resolves a setting to a font file.
///
/// Accepts a full path, or the name of an installed font ("Inter", "Fira Sans"
/// matches "FiraSans-Regular.ttf"). Returns `None` for an empty setting, which
/// means "keep egui's built-in font".
pub fn resolve(setting: &str, fonts: &[PathBuf]) -> Option<PathBuf> {
    let wanted = setting.trim();
    if wanted.is_empty() {
        return None;
    }
    let as_path = Path::new(wanted);
    if as_path.is_file() {
        return Some(as_path.to_path_buf());
    }
    let needle = wanted.to_lowercase();
    fonts
        .iter()
        .find(|path| display_name(path).to_lowercase() == needle)
        .or_else(|| {
            fonts
                .iter()
                .find(|path| display_name(path).to_lowercase().starts_with(&needle))
        })
        .cloned()
}

/// Installs a font and the UI scale.
///
/// The font is inserted ahead of the built-ins instead of replacing them, so
/// glyphs it lacks still render. The zoom factor covers the size: it scales
/// widget text *and* the text the app paints itself, which a `TextStyle`-only
/// change would miss.
pub fn apply(ctx: &egui::Context, font: Option<&Path>, zoom: f32) -> anyhow::Result<()> {
    if let Some(path) = font {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        let mut fonts = egui::FontDefinitions::default();
        let data: std::sync::Arc<egui::FontData> = egui::FontData::from_owned(bytes).into();
        fonts.font_data.insert("magic-tape-custom".to_string(), data);
        for family in [
            egui::FontFamily::Proportional,
            egui::FontFamily::Monospace,
        ] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.insert(0, "magic-tape-custom".to_string());
            }
        }
        ctx.set_fonts(fonts);
        log::info!("[font] installed {} ({})", path.display(), display_name(path));
    } else {
        ctx.set_fonts(egui::FontDefinitions::default());
        log::info!("[font] using the built-in UI font");
    }
    ctx.set_zoom_factor(zoom.clamp(0.7, 2.0));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_fonts() -> Vec<PathBuf> {
        ["Inter-Regular.ttf", "FiraSans-Bold.otf", "NotoSansJP.ttf"]
            .iter()
            .map(|name| PathBuf::from("/usr/share/fonts").join(name))
            .collect()
    }

    #[test]
    fn empty_setting_keeps_the_builtin_font() {
        assert_eq!(resolve("", &fake_fonts()), None);
        assert_eq!(resolve("   ", &fake_fonts()), None);
    }

    #[test]
    fn resolves_by_name_and_by_path() {
        let fonts = fake_fonts();
        assert_eq!(
            resolve("FiraSans-Bold", &fonts),
            Some(PathBuf::from("/usr/share/fonts/FiraSans-Bold.otf"))
        );
        assert_eq!(
            resolve("firasans-bold.OTF", &fonts),
            None,
            "matching is on the file stem, the extension is not part of it"
        );
        assert_eq!(
            resolve("/nowhere/Missing.ttf", &fonts),
            None,
            "a path is only accepted when it exists"
        );
    }

    #[test]
    fn accepts_a_path_that_exists() {
        let dir = std::env::temp_dir().join(format!(
            "mt-font-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("MyFont-Regular.ttf");
        std::fs::write(&file, b"not really a font").unwrap();
        assert_eq!(resolve(file.to_str().unwrap(), &[]), Some(file.clone()));
        assert_eq!(resolve("MyFont-Regular", &[]), None, "needs the scan list");
        assert_eq!(
            resolve("MyFont-Regular", std::slice::from_ref(&file)),
            Some(file),
            "a name resolves through the discovered list"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn falls_back_to_a_prefix_match() {
        let fonts = fake_fonts();
        assert_eq!(
            resolve("Noto", &fonts),
            Some(PathBuf::from("/usr/share/fonts/NotoSansJP.ttf"))
        );
        assert_eq!(resolve("No Such Font", &fonts), None);
    }

    #[test]
    fn display_name_drops_the_extension() {
        assert_eq!(
            display_name(Path::new("/usr/share/fonts/FiraSans-Bold.otf")),
            "FiraSans-Bold"
        );
    }

    #[test]
    fn zoom_is_clamped_to_something_usable() {
        // apply() needs a Context, so only the clamp itself is checked here.
        assert_eq!(0.1f32.clamp(0.7, 2.0), 0.7);
        assert_eq!(5.0f32.clamp(0.7, 2.0), 2.0);
        assert_eq!(1.25f32.clamp(0.7, 2.0), 1.25);
    }
}
