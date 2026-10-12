//! A folder chooser for the download directory.
//!
//! Hand-rolled rather than a native dialog: egui has no file dialog, and the
//! usual crates either need a GTK toolchain at build time or a portal service
//! that is not always there. This works wherever the app does, and the part
//! that can actually be wrong - the path handling - is testable on its own.

use std::path::{Path, PathBuf};

use crate::app::{App, ToastKind};
use crate::ui::{accent, muted};

pub struct FolderPicker {
    /// The folder currently being listed.
    pub current: PathBuf,
    /// Typed by the user, and where the folder is really taken from when it
    /// names a directory that exists.
    pub typed: String,
}

impl FolderPicker {
    /// Opens on the folder the setting points at.
    pub fn start(setting: &str, home: &Path) -> Self {
        let home = home.to_path_buf();
        let wanted = crate::config::resolve_download_dir(setting, &home);
        let current = nearest_existing(&wanted, &home);
        Self {
            typed: current.display().to_string(),
            current,
        }
    }

    pub fn enter(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.typed = path.display().to_string();
            self.current = path;
        }
    }

    pub fn go_up(&mut self, home: &Path) {
        self.enter(parent_dir(&self.current, home));
    }

    /// The folder to hand back: what was typed, if it is a real directory,
    /// otherwise the one being looked at.
    pub fn selection(&self) -> PathBuf {
        let typed = Path::new(&self.typed);
        if typed.is_dir() {
            typed.to_path_buf()
        } else {
            self.current.clone()
        }
    }
}

/// The first of `path` and its ancestors that exists, falling back to `home`.
pub fn nearest_existing(path: &Path, home: &Path) -> PathBuf {
    if path.is_dir() {
        return path.to_path_buf();
    }
    for ancestor in path.ancestors() {
        if ancestor.is_dir() {
            return ancestor.to_path_buf();
        }
    }
    home.to_path_buf()
}

/// The directory above `dir`.
///
/// The filesystem root has no parent at all and must stay put - falling back
/// to `home` there would teleport the picker out of `/` and back down into the
/// user's own folders.
pub fn parent_dir(dir: &Path, home: &Path) -> PathBuf {
    match dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        None => dir.to_path_buf(),
        // A bare relative name such as "Videos": there is nothing above it to
        // go to, so start from home.
        Some(_) => home.to_path_buf(),
    }
}

/// Immediate subdirectories, dot-directories hidden, case-insensitive order.
///
/// Files are left out on purpose: this picks a folder, not a file.
pub fn subdirectories(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let hidden = name.to_string_lossy().starts_with('.');
            let is_dir = entry.path().is_dir();
            (!hidden && is_dir).then(|| entry.path())
        })
        .collect();
    out.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    out
}

/// Renders the picker, and writes the answer back into the config.
pub fn picker_window(app: &mut App, ctx: &egui::Context) {
    let Some(mut picker) = app.folder_picker.take() else {
        return;
    };
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut open = true;
    let mut chosen = false;
    // Set inside the closure, applied after: `open` is already borrowed by
    // `.open(&mut open)`.
    let mut cancelled = false;

    egui::Window::new("Choose a folder")
        .open(&mut open)
        .resizable(true)
        .default_width(460.0)
        .default_height(400.0)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new("Folder").color(muted()));
            ui.add(
                egui::TextEdit::singleline(&mut picker.typed)
                    .desired_width(ui.available_width()),
            );

            ui.add_space(4.0);
            let listing = ui.available_height() - 60.0;
            egui::ScrollArea::vertical()
                .id_salt("folder_picker_list")
                .auto_shrink([false, false])
                .max_height(listing.max(120.0))
                .show(ui, |ui| {
                    if ui.selectable_label(false, ".. (up)").clicked() {
                        picker.go_up(&home);
                        return;
                    }
                    let dirs = subdirectories(&picker.current);
                    if dirs.is_empty() {
                        ui.label(
                            egui::RichText::new("No subfolders here").size(12.0).color(muted()),
                        );
                    }
                    for dir in dirs {
                        let name = dir
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        if ui.selectable_label(false, name).clicked() {
                            picker.enter(dir);
                        }
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Use this folder").clicked() {
                    chosen = true;
                }
                if ui.button("Cancel").clicked() {
                    cancelled = true;
                }
                ui.label(
                    egui::RichText::new(picker.current.display().to_string())
                        .size(11.0)
                        .color(accent()),
                );
            });
        });

    if cancelled {
        open = false;
    }
    if chosen {
        let folder = picker.selection();
        let folder = crate::config::resolve_download_dir(&folder.display().to_string(), &home);
        app.config.download_directory = folder.display().to_string();
        app.config.clamp();
        app.toast(
            format!("Downloads will go to {}", folder.display()),
            ToastKind::Success,
        );
        return;
    }
    if open {
        app.folder_picker = Some(picker);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("magic-tape-picker-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_folder_opens_on_the_nearest_one_that_exists() {
        let root = scratch("nearest");
        let nested = root.join("deep/missing/leaf");
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();

        assert_eq!(
            nearest_existing(&nested, &home),
            root,
            "the closest existing ancestor, not the home fallback"
        );
        assert_eq!(nearest_existing(&home, &home), home);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn nothing_existing_falls_back_to_home() {
        let home = PathBuf::from("/definitely/not/here");
        assert_eq!(nearest_existing(&home, Path::new("/")), PathBuf::from("/"));
    }

    #[test]
    fn going_up_stops_at_the_root() {
        assert_eq!(
            parent_dir(Path::new("/mnt/data"), Path::new("/home/me")),
            PathBuf::from("/mnt")
        );
        assert_eq!(
            parent_dir(Path::new("/"), Path::new("/home/me")),
            PathBuf::from("/"),
            "the root has nowhere left to go"
        );
        assert_eq!(
            parent_dir(Path::new("/mnt"), Path::new("/home/me")),
            PathBuf::from("/"),
            "a first-level directory's parent is the root"
        );
    }

    #[test]
    fn lists_folders_only_without_the_hidden_ones() {
        let root = scratch("listing");
        std::fs::create_dir_all(root.join("Zebra")).unwrap();
        std::fs::create_dir_all(root.join("apple")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("notes.txt"), "x").unwrap();

        let listed: Vec<String> = subdirectories(&root)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(listed, vec!["apple", "Zebra"], "case-insensitive order");
        assert!(!listed.iter().any(|n| n == ".hidden"));
        assert!(!listed.iter().any(|n| n == "notes.txt"), "not a folder");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unreadable_folder_is_empty_rather_than_a_panic() {
        assert!(subdirectories(Path::new("/definitely/not/a/folder")).is_empty());
    }

    #[test]
    fn the_typed_path_wins_when_it_is_a_real_folder() {
        let root = scratch("selection");
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let picker = FolderPicker {
            current: root.clone(),
            typed: other.display().to_string(),
        };
        assert_eq!(picker.selection(), other);

        // Nonsense typed: fall back to what is being looked at.
        let picker = FolderPicker {
            current: root.clone(),
            typed: "/nope/nowhere".into(),
        };
        assert_eq!(picker.selection(), root);
        let _ = std::fs::remove_dir_all(&root);
    }
}