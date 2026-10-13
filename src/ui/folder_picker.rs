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
    /// Name for a folder to create; not a path.
    pub new_folder: String,
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
            new_folder: String::new(),
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

/// Mount points that are worth offering: the volumes, not the system.
///
/// This is what a traditional chooser calls "Other Locations". Without it
/// there is no way to reach `/mnt` or `/run/media/<user>/<volume>` without
/// typing the path, which is the whole reason for a folder picker.
///
/// Prefixes a desktop file manager would also hide are dropped: btrfs
/// subvolumes under `/home` and `/var`, `/usr`, and friends are not somewhere
/// anyone downloads videos to.
const INTERNAL_PREFIXES: [&str; 12] = [
    "/home", "/usr", "/var", "/boot", "/etc", "/opt", "/srv", "/root", "/snap", "/proc", "/sys",
    "/run/user",
];

/// The interesting mounts out of a `/proc/mounts` dump.
///
/// Kept pure and separate from the disk read so it can be tested against a
/// fixture, which matters because this is the part that decides whether a USB
/// stick shows up at all.
pub fn parse_mounts(text: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(device), Some(target)) = (fields.next(), fields.next()) else {
            continue;
        };
        // A real filesystem, not a pseudo one.
        if !device.starts_with("/dev/") {
            continue;
        }
        if target == "/" || !target.starts_with('/') {
            continue;
        }
        let hidden = INTERNAL_PREFIXES
            .iter()
            .any(|prefix| target == *prefix || target.starts_with(&format!("{prefix}/")));
        if hidden {
            continue;
        }
        let path = PathBuf::from(target);
        if !out.contains(&path) {
            out.push(path);
        }
    }
    outermost(out)
}

/// Keeps only the top of each stack of nested mounts.
///
/// A Steam library on a stick mounts its own volumes *inside* that stick's
/// mount point, and a btrfs layout nests several more. Listing those alongside
/// the drive they sit on is how you end up with thirteen entries and no idea
/// which one is the disk.
fn outermost(mounts: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut by_depth = mounts.clone();
    by_depth.sort_by_key(|path| path.components().count());
    let mut kept: Vec<PathBuf> = Vec::new();
    for mount in by_depth {
        if !kept.iter().any(|parent| mount.starts_with(parent)) {
            kept.push(mount);
        }
    }
    // Back to the order the kernel listed them in, which is roughly the order
    // they were plugged in - steadier for the eye than a sorted list.
    mounts.into_iter().filter(|m| kept.contains(m)).collect()
}

/// Volumes currently mounted, if `/proc/mounts` can be read.
pub fn volumes() -> Vec<PathBuf> {
    std::fs::read_to_string("/proc/mounts")
        .map(|text| parse_mounts(&text))
        .unwrap_or_default()
}

/// The standard shortcuts down the side of a traditional chooser, skipping
/// any that do not exist.
pub fn places(home: &Path) -> Vec<(&'static str, &'static [&'static str], PathBuf)> {
    // The names are the freedesktop ones, so the user's own theme supplies
    // the pictures rather than anything shipped with the app.
    let candidates: [(&str, &[&str], PathBuf); 9] = [
        ("Home", &["user-home"][..], home.to_path_buf()),
        ("Desktop", &["user-desktop"][..], home.join("Desktop")),
        ("Downloads", &["folder-downloads", "folder-download"][..], home.join("Downloads")),
        ("Documents", &["folder-documents"][..], home.join("Documents")),
        ("Videos", &["folder-videos"][..], home.join("Videos")),
        ("Music", &["folder-music"][..], home.join("Music")),
        ("Pictures", &["folder-pictures"][..], home.join("Pictures")),
        ("Home folder", &["filesystem", "drive-harddisk"][..], PathBuf::from("/")),
        ("tmp", &["folder"][..], PathBuf::from("/tmp")),
    ];
    candidates
        .into_iter()
        .filter(|(_, _, path)| path.is_dir())
        .collect()
}

/// Edge length of the system icons, in points.
const ICON_PX: u32 = 16;

/// A button carrying a system icon, degrading to plain text when the theme has
/// nothing by that name.
fn icon_button_label<'a>(
    app: &mut App,
    ctx: &egui::Context,
    icons: &[&str],
    text: &'a str,
) -> egui::Button<'a> {
    let image = app.icons.first_of(ctx, icons, ICON_PX).map(|texture| {
        let size = egui::vec2(ICON_PX as f32, ICON_PX as f32);
        egui::Image::new(egui::load::SizedTexture::from_handle(&texture))
            .max_size(size)
    });
    egui::Button::opt_image_and_text(image, Some(text.into()))
}

/// A selectable row with a system icon, falling back to plain text when the
/// theme has nothing by that name.
fn icon_button(
    ui: &mut egui::Ui,
    app: &mut App,
    icons: &[&str],
    text: egui::RichText,
    selected: bool,
) -> egui::Response {
    let Some(texture) = app.icons.first_of(ui.ctx(), icons, ICON_PX) else {
        return ui.selectable_label(selected, text);
    };
    ui.horizontal(|ui| {
        let size = egui::vec2(ICON_PX as f32, ICON_PX as f32);
        ui.add(egui::Image::new(egui::load::SizedTexture::from_handle(&texture)).max_size(size));
        ui.selectable_label(selected, text)
    })
    .inner
}

/// Renders the picker, and writes the answer back into the config.
///
/// Shaped like a traditional file chooser: a path bar across the top, the
/// standard places and mounted volumes down the left, the current folder in
/// the middle, and the actions along the bottom.
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
    let mut error: Option<String> = None;
    let volumes = volumes();

    egui::Window::new("Choose a folder")
        .open(&mut open)
        .resizable(true)
        .default_width(700.0)
        .default_height(480.0)
        .show(ctx, |ui| {
            // Path bar: type anywhere and go.
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut picker.typed)
                        .desired_width(ui.available_width() - 60.0),
                );
                if ui.button("Go").clicked() || response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
                {
                    let wanted = PathBuf::from(picker.typed.trim());
                    if wanted.is_dir() {
                        picker.enter(wanted);
                    } else if wanted.is_absolute() {
                        error = Some(format!("{} is not a folder", wanted.display()));
                    }
                }
            });

            let body = ui.available_height();
            ui.horizontal_top(|ui| {
                // Places and volumes.
                ui.allocate_ui_with_layout(
                    egui::vec2(180.0, body),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            egui::RichText::new("PLACES").size(10.5).color(muted()),
                        );
                        egui::ScrollArea::vertical()
                            .id_salt("picker_places")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                let here = picker.current.clone();
                                for (label, icons, path) in places(&home) {
                                    let current = path == here;
                                    let text = egui::RichText::new(label).strong().color(
                                        if current { accent() } else { ui.visuals().text_color() },
                                    );
                                    if icon_button(ui, app, icons, text, current).clicked() {
                                        picker.enter(path);
                                    }
                                }
                                if !volumes.is_empty() {
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new("VOLUMES").size(10.5).color(muted()),
                                    );
                                    for volume in &volumes {
                                        let name = volume
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned();
                                        let current = *volume == here;
                                        let label = egui::RichText::new(name).color(
                                            if current { accent() } else { ui.visuals().text_color() },
                                        );
                                        let response = icon_button(
                                            ui,
                                            app,
                                            &["drive-harddisk", "drive-removable-media", "folder"],
                                            label,
                                            current,
                                        )
                                        .on_hover_text(volume.display().to_string());
                                        if response.clicked() {
                                            picker.enter(volume.clone());
                                        }
                                    }
                                }
                            });
                    },
                );
                ui.add_space(6.0);

                // The folder being looked at.
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), body),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(picker.current.display().to_string())
                                    .size(12.0)
                                    .strong(),
                            );
                        });
                        ui.add_space(4.0);
                        let listing = ui.available_height() - 40.0;
                        egui::ScrollArea::vertical()
                            .id_salt("picker_listing")
                            .auto_shrink([false, false])
                            .max_height(listing.max(120.0))
                            .show(ui, |ui| {
                                if icon_button(
                                    ui,
                                    app,
                                    &["go-up", "go-previous"],
                                    egui::RichText::new(".. (up one level)"),
                                    false,
                                )
                                .clicked()
                                {
                                    picker.go_up(&home);
                                    return;
                                }
                                let dirs = subdirectories(&picker.current);
                                if dirs.is_empty() {
                                    ui.label(
                                        egui::RichText::new("No subfolders here")
                                            .size(12.0)
                                            .color(muted()),
                                    );
                                }
                                for dir in dirs {
                                    let name = dir
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned();
                                    if icon_button(
                                        ui,
                                        app,
                                        &["folder"],
                                        egui::RichText::new(format!("{name}/")),
                                        false,
                                    )
                                    .clicked()
                                    {
                                        picker.enter(dir);
                                    }
                                }
                            });
                    },
                );
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut picker.new_folder)
                        .hint_text("new folder name")
                        .desired_width(160.0),
                );
                if ui
                    .add(icon_button_label(
                        app,
                        ui.ctx(),
                        &["folder-new", "folder", "list-add"],
                        "Create",
                    ))
                    .clicked()
                {
                    let name = picker.new_folder.trim().to_string();
                    if name.is_empty() {
                        error = Some("Give the new folder a name".into());
                    } else {
                        let candidate = picker.current.join(&name);
                        match std::fs::create_dir(&candidate) {
                            Ok(()) => {
                                picker.new_folder.clear();
                                picker.enter(candidate);
                            }
                            Err(problem) => {
                                error = Some(format!("{problem}"));
                            }
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(icon_button_label(
                            app,
                            ui.ctx(),
                            &["window-close-symbolic", "dialog-cancel"],
                            "Cancel",
                        ))
                        .clicked()
                    {
                        cancelled = true;
                    }
                    let use_it = icon_button_label(
                        app,
                        ui.ctx(),
                        &["object-select-symbolic", "dialog-ok", "emblem-ok-symbolic"],
                        "Use this folder",
                    );
                    if ui.add(use_it).clicked() {
                        chosen = true;
                    }
                });
            });
            if let Some(problem) = &error {
                ui.colored_label(egui::Color32::from_rgb(0xE0, 0x70, 0x70), problem);
            }
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
    fn volumes_are_the_mounts_you_would_want_to_download_to() {
        // A real /proc/mounts from a KDE box with two USB sticks.
        let mounts = "\
sysfs /sys sysfs rw 0 0
proc /proc proc rw 0 0
/dev/sda2 / btrfs rw 0 0
/dev/sda2 /home btrfs rw 0 0
/dev/sda2 /var/cache btrfs rw 0 0
/dev/sda2 /home/me/.local/share/Steam btrfs rw 0 0
/dev/sda2 /home/me/sync ext4 rw 0 0
/dev/sdb1 /run/media/me/MAINLINUX ext4 rw 0 0
/dev/sdc1 /run/media/me/WD1 ntfs3 rw 0 0
/dev/sdd1 /mnt/backup ext4 rw 0 0
/dev/sdb2 /run/media/me/WD1/SteamLibrary ext4 rw 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sde1 /media/photo ext4 rw 0 0
";
        assert_eq!(
            parse_mounts(mounts),
            vec![
                PathBuf::from("/run/media/me/MAINLINUX"),
                PathBuf::from("/run/media/me/WD1"),
                PathBuf::from("/mnt/backup"),
                PathBuf::from("/media/photo"),
            ],
            "removable volumes and /mnt, but no btrfs subvolumes, no pseudo \
             filesystems, and nothing nested inside a volume we already list"
        );
    }

    #[test]
    fn nested_mounts_do_not_clutter_the_list() {
        let mounts = "\
/dev/sdb1 /run/media/me/WD1 ext4 rw 0 0
/dev/sdb2 /run/media/me/WD1/SteamLibrary/steamapps ext4 rw 0 0
/dev/sdb3 /run/media/me/WD1/SteamLibrary/steamapps/compatdata ext4 rw 0 0
/dev/nvme0p2 / btrfs rw 0 0
";
        assert_eq!(
            parse_mounts(mounts),
            vec![PathBuf::from("/run/media/me/WD1")],
            "the stick is listed once, not four times"
        );
    }

    #[test]
    fn a_duplicated_mount_is_listed_once() {
        let mounts = "\
/dev/sda1 /media/one ext4 rw 0 0
/dev/sda2 /media/one ext4 rw 0 0
";
        assert_eq!(parse_mounts(mounts), vec![PathBuf::from("/media/one")]);
    }

    #[test]
    fn rubbish_mount_lines_are_skipped() {
        assert!(parse_mounts("").is_empty());
        assert!(parse_mounts("nonsense\n/dev/sda1\n").is_empty());
        assert!(parse_mounts("/dev/sda1 relative-target ext4 rw 0 0").is_empty());
    }

    #[test]
    fn places_include_home_and_the_filesystem_root() {
        let root = scratch("places");
        let home = root.join("me");
        std::fs::create_dir_all(home.join("Videos")).unwrap();
        let labels: Vec<&str> = places(&home).iter().map(|(label, _, _)| *label).collect();
        assert!(labels.contains(&"Home"));
        assert!(labels.contains(&"Videos"), "an existing shortcut shows");
        assert!(!labels.contains(&"Music"), "one that does not exist is left out");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Opt-in check that the volumes on *this* machine are the ones offered.
    /// Run with: `cargo test -- --ignored --nocapture live_volumes`
    #[test]
    #[ignore = "depends on what is actually mounted"]
    fn live_volumes_are_offered() {
        let found = volumes();
        println!("volumes offered: {found:?}");
        for known in ["/mnt", "/run/media", "/media"] {
            let root = Path::new(known);
            if !root.is_dir() {
                continue;
            }
            // Either the mount point itself, or something mounted inside it.
            let covered = found.iter().any(|volume| {
                volume == root || root.ancestors().any(|a| a == volume.as_path())
            }) || subdirectories(root).iter().any(|sub| {
                sub.starts_with("/run/media") || sub.starts_with("/mnt") || sub.starts_with("/media")
            });
            assert!(covered, "{known} exists but nothing leads there: {found:?}");
        }
    }

    #[test]
    fn the_typed_path_wins_when_it_is_a_real_folder() {
        let root = scratch("selection");
        let other = root.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let picker = FolderPicker {
            current: root.clone(),
            typed: other.display().to_string(),
            new_folder: String::new(),
        };
        assert_eq!(picker.selection(), other);

        // Nonsense typed: fall back to what is being looked at.
        let picker = FolderPicker {
            current: root.clone(),
            typed: "/nope/nowhere".into(),
            new_folder: String::new(),
        };
        assert_eq!(picker.selection(), root);
        let _ = std::fs::remove_dir_all(&root);
    }
}