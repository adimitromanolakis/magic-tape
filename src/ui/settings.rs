use eframe::egui;

use crate::app::{App, ToastKind};
use crate::config::{CommentsSort, ListView, PinnedPlacement, Theme};
use crate::ui::{accent, muted};

pub fn theme_label(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
        Theme::System => "System",
    }
}

pub fn settings_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_settings;
    egui::Window::new("Settings")
        .open(&mut open)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (index, label) in ["General", "Playback", "Comments", "Maintenance"]
                    .iter()
                    .enumerate()
                {
                    if ui
                        .selectable_label(
                            app.settings_tab == index,
                            egui::RichText::new(*label).size(13.0),
                        )
                        .clicked()
                    {
                        app.settings_tab = index;
                    }
                }
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                match app.settings_tab {
                    0 => general(app, ui),
                    1 => playback(app, ui),
                    2 => comments(app, ui),
                    _ => maintenance(app, ui),
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        match app.save_config() {
                            Ok(()) => app.toast("Settings saved".into(), ToastKind::Success),
                            Err(error) => app.toast(error.to_string(), ToastKind::Error),
                        }
                    }
                    if ui.button("Reset to defaults").clicked() {
                        app.config = crate::config::Config::default();
                        let _ = app.save_config();
                        app.toast("Defaults restored".into(), ToastKind::Success);
                    }
                    ui.label(
                        egui::RichText::new(app.paths.config_file.to_string_lossy())
                            .size(10.5)
                            .color(muted()),
                    );
                });
            });
        });
    app.show_settings = open;
}

fn general(app: &mut App, ui: &mut egui::Ui) {
    ui.label("Browsing");
    egui::Grid::new("general_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Browser for cookies");
        ui.add(
            egui::TextEdit::singleline(&mut app.config.cookies_from_browser)
                .desired_width(220.0),
        );
        ui.end_row();
        ui.label("Videos per page");
        ui.add(egui::Slider::new(&mut app.config.list_length, 1..=99).text("videos"));
        ui.end_row();
        ui.label("Default view");
        egui::ComboBox::from_id_salt("list_view")
            .selected_text(match app.config.list_view {
                ListView::Grid => "Grid",
                ListView::List => "List",
            })
            .show_ui(ui, |ui| {
                let _ = ui.selectable_label(app.config.list_view == ListView::Grid, "Grid");
                let _ = ui.selectable_label(app.config.list_view == ListView::List, "List");
            });
        ui.end_row();
        ui.label("Theme");
        egui::ComboBox::from_id_salt("theme")
            .selected_text(theme_label(app.config.theme))
            .show_ui(ui, |ui| {
                for (option, label) in [
                    (Theme::Dark, "Dark"),
                    (Theme::Light, "Light"),
                    (Theme::System, "System"),
                ] {
                    if ui
                        .selectable_label(app.config.theme == option, label)
                        .clicked()
                    {
                        app.config.theme = option;
                    }
                }
            });
        ui.end_row();
    });
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "Cookies from your browser let magic-tape read your private feeds, history and subscriptions.",
            )
            .size(11.5)
            .color(muted()),
        )
        .wrap(),
    );
    ui.add_space(8.0);

    ui.label("Appearance");
    font_controls(app, ui);
    ui.add_space(8.0);

    ui.label("Notifications");
    ui.checkbox(&mut app.config.notification_enabled, "Show desktop notifications");
    if app.config.notification_enabled {
        ui.add(
            egui::Slider::new(&mut app.config.notification_duration_ms, 500..=15000)
                .suffix(" ms")
                .text("Duration"),
        );
    }
}

/// UI font and scale.
///
/// The font is picked from the fonts installed on this machine; the chosen one
/// is inserted ahead of egui's built-ins, so glyphs it lacks still render.
fn font_controls(app: &mut App, ui: &mut egui::Ui) {
    let fonts = crate::font::discovered();
    let resolved = crate::font::resolve(&app.config.ui_font, fonts);

    ui.horizontal(|ui| {
        ui.label("Font");
        let selected = if app.config.ui_font.trim().is_empty() {
            "System default".to_string()
        } else {
            resolved
                .as_deref()
                .map(crate::font::display_name)
                .unwrap_or_else(|| app.config.ui_font.clone())
        };
        egui::ComboBox::from_id_salt("ui_font")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(app.config.ui_font.trim().is_empty(), "System default")
                    .clicked()
                {
                    app.config.ui_font.clear();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut app.font_filter)
                        .hint_text("filter fonts...")
                        .desired_width(220.0),
                );
                ui.separator();
                let needle = app.font_filter.to_lowercase();
                for path in fonts.iter().filter(|path| {
                    needle.is_empty()
                        || crate::font::display_name(path).to_lowercase().contains(&needle)
                }) {
                    let name = crate::font::display_name(path);
                    if ui
                        .selectable_label(app.config.ui_font == path.to_string_lossy(), name)
                        .on_hover_text(path.to_string_lossy())
                        .clicked()
                    {
                        app.config.ui_font = path.to_string_lossy().into_owned();
                    }
                }
            });
        ui.label(
            egui::RichText::new(format!("{} installed", crate::font::discovered().len()))
                .size(11.0)
                .color(muted()),
        );
    });
    if !app.config.ui_font.trim().is_empty() && resolved.is_none() {
        ui.colored_label(
            egui::Color32::from_rgb(0xE0, 0x50, 0x50),
            format!("No installed font matches \"{}\"", app.config.ui_font),
        );
    }
    ui.horizontal(|ui| {
        ui.label("Size");
        ui.add(
            egui::Slider::new(&mut app.config.ui_zoom, 0.7..=2.0)
                .text("scale")
                .clamping(egui::SliderClamping::Always),
        );
        if ui.button("Reset").clicked() {
            app.config.ui_zoom = 1.0;
        }
    });
}

fn playback(app: &mut App, ui: &mut egui::Ui) {
    ui.label("mpv");
    ui.horizontal(|ui| {
        ui.label("Binary");
        ui.add(
            egui::TextEdit::singleline(&mut app.config.mpv_binary).desired_width(160.0),
        );
        if ui.button("Test").clicked() {
            let player = crate::player::Player::new(&app.config.mpv_binary, false);
            if player.available() {
                app.toast("mpv works".into(), ToastKind::Success);
            } else {
                app.toast("mpv not found".into(), ToastKind::Error);
            }
        }
    });
    ui.checkbox(&mut app.config.play_in_fullscreen, "Open videos fullscreen");
    ui.horizontal(|ui| {
        ui.label("mpv log level");
        ui.add(
            egui::TextEdit::singleline(&mut app.config.mpv_log_level)
                .desired_width(160.0)
                .font(egui::TextStyle::Monospace),
        );
    });
    ui.horizontal(|ui| {
        ui.label("yt-dlp client");
        ui.add(
            egui::TextEdit::singleline(&mut app.config.playback_client)
                .desired_width(160.0)
                .font(egui::TextStyle::Monospace),
        );
    });
    ui.add(
        egui::Label::new(
            egui::RichText::new(
                "Which YouTube client mpv uses for playback. `web_embedded` returns URLs that ffmpeg cannot fetch (HTTP 403), so it is left out. Leave empty to let yt-dlp decide.",
            )
            .size(11.5)
            .color(muted()),
        )
        .wrap(),
    );
    ui.add_space(8.0);

    ui.label("Downloads");
    ui.horizontal(|ui| {
        ui.label("Folder");
        ui.add(
            egui::TextEdit::singleline(&mut app.config.download_directory)
                .desired_width(220.0)
                .hint_text("/Downloads, /tmp/youtube, ~/Videos"),
        );
        if ui
            .button("Browse...")
            .on_hover_text("Pick a folder from the filesystem")
            .clicked()
        {
            let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
            app.folder_picker = Some(crate::ui::folder_picker::FolderPicker::start(
                &app.config.download_directory,
                &home,
            ));
        }
    });
    let resolved = app.download_dir();
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("Downloads go to {}", resolved.display()))
                .size(11.0)
                .color(muted()),
        )
        .wrap(),
    );
    if !resolved.exists() {
        ui.colored_label(
            egui::Color32::from_rgb(0xE0, 0xB0, 0x40),
            "That folder does not exist yet - it will be created on the first download.",
        );
    }
    ui.checkbox(&mut app.config.show_mpv_keybindings, "Show mpv keybindings in the help panel");
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Format selector used by the Best button (yt-dlp syntax)").color(muted()),
    );
    ui.add(
        egui::TextEdit::singleline(&mut app.config.download_video_quality)
            .desired_width(400.0)
            .font(egui::TextStyle::Monospace),
    );
    ui.label(
        egui::RichText::new("Examples: bv*[height<=1080]+ba  -  bestaudio/best  -  22").color(muted()),
    );
}

fn comments(app: &mut App, ui: &mut egui::Ui) {
    ui.checkbox(&mut app.config.show_comments, "Load description and comments");
    ui.checkbox(
        &mut app.config.colored_messages,
        "Colored messages in the status bar",
    );
    ui.add(
        egui::Slider::new(&mut app.config.comments_max, 1..=200)
            .text("Comments to load"),
    );
    ui.add(
        egui::Slider::new(&mut app.config.comment_replies_max, 0..=50)
            .text("Max replies"),
    );
    ui.horizontal(|ui| {
        ui.label("Sort by");
        egui::ComboBox::from_id_salt("sort")
            .selected_text(match app.config.comments_sort {
                CommentsSort::New => "newest",
                CommentsSort::Top => "top",
                CommentsSort::Old => "oldest",
            })
            .show_ui(ui, |ui| {
                let _ = ui.selectable_label(app.config.comments_sort == CommentsSort::Old, "oldest");
                let _ = ui.selectable_label(app.config.comments_sort == CommentsSort::New, "newest");
                let _ = ui.selectable_label(app.config.comments_sort == CommentsSort::Top, "top");
            });
        ui.label("Pinned comment");
        egui::ComboBox::from_id_salt("pinned")
            .selected_text(match app.config.pinned_comments {
                PinnedPlacement::First => "top",
                PinnedPlacement::Last => "bottom",
            })
            .show_ui(ui, |ui| {
                let _ = ui.selectable_label(
                    app.config.pinned_comments == PinnedPlacement::First,
                    "top",
                );
                let _ = ui.selectable_label(
                    app.config.pinned_comments == PinnedPlacement::Last,
                    "bottom",
                );
            });
    });
    if let Some(video) = app.current_video()
        && ui.button("Reload comments now").clicked() {
            app.load_comments_for(video);
        }
}

fn maintenance(app: &mut App, ui: &mut egui::Ui) {
    ui.label("Subscriptions");
    ui.label(
        egui::RichText::new(format!(
            "{} subscriptions stored locally",
            app.subscriptions.len()
        ))
        .size(11.5)
        .color(muted()),
    );
    if ui.button("Import subscriptions from YouTube").clicked() {
        app.import_subscriptions();
    }
    if !app.subscriptions.is_empty()
        && ui.button("Reload subscription list").clicked()
    {
        app.reload_subscriptions();
        app.toast(
            format!("{} subscriptions reloaded", app.subscriptions.len()),
            ToastKind::Info,
        );
    }
    ui.add_space(6.0);

    ui.label("History");
    ui.horizontal(|ui| {
        if ui.button("Clear watch history").clicked() {
            match app.store.clear_watch_history() {
                Ok(()) => {
                    app.history.clear();
                    app.toast("Watch history cleared".into(), ToastKind::Success);
                }
                Err(error) => app.toast(error.to_string(), ToastKind::Error),
            }
        }
        if ui.button("Clear search history").clicked() {
            match app.store.clear_search_history() {
                Ok(()) => app.toast("Search history cleared".into(), ToastKind::Success),
                Err(error) => app.toast(error.to_string(), ToastKind::Error),
            }
        }
    });
    ui.add_space(6.0);

    ui.label("Thumbnail cache");
    ui.label(
        egui::RichText::new(format!(
            "{} thumbnails in memory - folder {}",
            app.thumbs.cached_count(),
            app.paths.thumb_dir.display()
        ))
        .size(11.5)
        .color(muted()),
    );
    if ui.button("Clear thumbnail cache").clicked() {
        match app.store.clear_thumbnails() {
            Ok(()) => {
                app.thumbs.forget_all();
                app.toast("Thumbnail cache cleared".into(), ToastKind::Success);
            }
            Err(error) => app.toast(error.to_string(), ToastKind::Error),
        }
    }
    ui.add_space(6.0);

    ui.label("Tools");
    ui.horizontal(|ui| {
        if ui.button("Update yt-dlp").clicked() {
            let yt = app.yt.clone();
            app.worker.spawn("update yt-dlp", move || {
                match yt.update() {
                    Ok(version) => crate::tasks::Outcome::YtDlpUpdated {
                        version: version.trim().to_string(),
                    },
                    Err(error) => crate::tasks::Outcome::Failed {
                        label: "updating yt-dlp".into(),
                        error: error.to_string(),
                    },
                }
            });
        }
        if ui.button("Open cache folder").clicked() {
            let _ = crate::player::open_directory(&app.paths.cache_dir);
        }
        if ui.button("Open config folder").clicked()
            && let Some(parent) = app.paths.config_file.parent() {
                let _ = crate::player::open_directory(parent);
            }
    });
}

pub fn shortcuts_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.show_shortcuts;
    egui::Window::new("Keyboard shortcuts")
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            egui::Grid::new("shortcuts").num_columns(2).striped(true).show(ui, |ui| {
                let rows = [
                    ("Up / Down or J / K", "Move selection"),
                    ("Page Down / Page Up", "Move one screenful (Shift+Down / Shift+Up)"),
                    ("Home / End", "First / last video"),
                    ("Down at the end", "Loads the next page automatically"),
                    ("Enter", "Play selected video"),
                    ("Space", "Play / pause handled by mpv"),
                    ("F", "Play best quality"),
                    ("A", "Play audio only"),
                    ("D", "Download video"),
                    ("Shift + D", "Download audio"),
                    ("L", "Toggle like"),
                    ("S", "Subscribe to channel"),
                    ("B", "Open in browser"),
                    ("C", "Copy link"),
                    ("R", "Refresh current feed"),
                    ("Ctrl + F", "Search"),
                    (", / .", "Previous / next page"),
                    ("Ctrl + ,", "Settings"),
                    ("Esc", "Close panels"),
                    ("Ctrl + Q", "Quit"),
                ];
                for (key, action) in rows {
                    ui.label(egui::RichText::new(key).strong().color(accent()));
                    ui.label(action);
                    ui.end_row();
                }
            });
            if app.config.show_mpv_keybindings {
                ui.separator();
                ui.label(egui::RichText::new("mpv keys").strong());
                ui.label("space pause - f fullscreen - [ ] speed - , . frame - q quit");
            }
            ui.add_space(6.0);
            if ui.button("Close").clicked() {
                app.show_shortcuts = false;
            }
        });
    app.show_shortcuts = open;
}