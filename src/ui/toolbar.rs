use eframe::egui;

use crate::app::{App, Section, ToastKind};
use crate::config::ListView;
use crate::model::{DurationFilter, Feed};
use crate::ui::{accent, muted};

pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(app.section.title())
                .size(18.0)
                .strong()
                .color(accent()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Settings").clicked() {
                app.show_settings = true;
            }
            if ui.button("Help").on_hover_text("Keyboard shortcuts").clicked() {
                app.show_shortcuts = true;
            }
            if ui
                .button(if app.list_view == ListView::Grid {
                    "Grid"
                } else {
                    "List"
                })
                .on_hover_text("Switch between the thumbnail grid and the compact list")
                .clicked()
            {
                app.list_view = match app.list_view {
                    ListView::Grid => ListView::List,
                    ListView::List => ListView::Grid,
                };
                app.config.list_view = app.list_view;
                let _ = app.save_config();
            }
            ui.label(
                egui::RichText::new(format!("{}/page", app.config.list_length))
                    .size(11.0)
                    .color(muted()),
            );
        });
    });

    ui.horizontal_wrapped(|ui| {
        match app.section {
            Section::Feeds => feed_controls(app, ui),
            Section::Playlists => playlist_controls(app, ui),
            Section::Channels => channel_controls(app, ui),
            Section::Liked | Section::History => {
                if ui
                    .button("Refresh")
                    .on_hover_text("Reload this list")
                    .clicked()
                {
                    match app.section {
                        Section::Liked => app.liked = app.store.liked(),
                        _ => {
                            app.history = app.store.watch_history();
                            app.history.reverse();
                        }
                    }
                    app.select(None);
                }
            }
        }
        paging_controls(app, ui);
    });
}

fn feed_controls(app: &mut App, ui: &mut egui::Ui) {
    match app.feed {
        Feed::Channel => {
            ui.label(
                egui::RichText::new(format!("channel: {}", app.channel_name))
                    .size(12.0)
                    .color(muted()),
            );
        }
        Feed::Search => {
            ui.label(
                egui::RichText::new(format!("results for \"{}\"", app.query))
                    .size(12.0)
                    .color(muted()),
            );
        }
        _ => {
            ui.label(
                egui::RichText::new(app.feed.label())
                    .size(12.0)
                    .color(muted()),
            );
        }
    }
    ui.separator();
    if !app.query.is_empty() {
        ui.label(
            egui::RichText::new(format!("\"{}\"", app.query))
                .size(12.0)
                .color(accent()),
        );
        for filter in [
            DurationFilter::Any,
            DurationFilter::Short,
            DurationFilter::Medium,
            DurationFilter::Long,
        ] {
            if ui
                .selectable_label(app.filter == filter, egui::RichText::new(filter.label()).size(11.5))
                .clicked()
            {
                app.filter = filter;
                app.load_feed(app.feed, 0);
            }
            ui.add_space(4.0);
        }
    } else {
        ui.add(
            egui::TextEdit::singleline(&mut app.search_input)
                .hint_text("Search YouTube...")
                .desired_width(220.0),
        );
        if ui.button("Search").clicked() {
            start_search(app);
        }
        if ui.button("History").on_hover_text("Search history").clicked()
            && let Some(term) = app.store.search_history().first().cloned() {
                app.search_input = term;
            }
    }
    ui.separator();
    if ui
        .button("Refresh")
        .on_hover_text("Reload this feed from the start")
        .clicked()
    {
        app.load_feed(app.feed, 0);
    }
}

/// Page controls that stay reachable without scrolling to the bottom of the
/// list. `,` and `.` do the same from the keyboard.
pub fn paging_controls(app: &mut App, ui: &mut egui::Ui) {
    let total = app.item_count();
    let step = app.config.list_length.max(1);
    let start = app.feed_state.start;

    let prev_label = if start > 0 {
        format!("\u{25c0} Prev ({})", start.saturating_sub(step) + 1)
    } else {
        "\u{25c0} Prev".to_string()
    };
    if ui
        .add_enabled(start > 0, egui::Button::new(prev_label))
        .on_hover_text("Load the previous page (,)")
        .clicked()
    {
        app.load_prev_page();
    }

    let next_label = if app.has_more() {
        format!("Next ({}) \u{25b6}", total + step)
    } else {
        "Next \u{25b6}".to_string()
    };
    if ui
        .add_enabled(app.has_more(), egui::Button::new(next_label))
        .on_hover_text("Load more videos (.) - also happens automatically at the end of the list")
        .clicked()
    {
        app.load_next_page();
    }
    if ui
        .add_enabled(total > 0, egui::Button::new("Top"))
        .on_hover_text("Jump to the first video (Home)")
        .clicked()
    {
        app.select(Some(0));
    }
}

fn playlist_controls(app: &mut App, ui: &mut egui::Ui) {
    ui.add(
        egui::TextEdit::singleline(&mut app.playlist_query)
            .hint_text("Search playlists...")
            .desired_width(240.0),
    );
    if ui.button("Search").clicked() {
        app.load_playlists(&app.playlist_query.clone(), 0);
    }
}

fn channel_controls(app: &mut App, ui: &mut egui::Ui) {
    ui.add(
        egui::TextEdit::singleline(&mut app.channel_query)
            .hint_text("Find channels on YouTube...")
            .desired_width(240.0),
    );
    if ui.button("Search").clicked() {
        app.search_channels(&app.channel_query.clone(), 0);
    }
    if ui.button("My subscriptions").on_hover_text("List your imported subscriptions").clicked() {
        let subs = app.subscriptions.clone();
        app.channel_results = subs
            .into_iter()
            .map(|(id, name)| crate::model::Channel {
                thumbnail: Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")),
                id,
                name,
                subscribers: None,
            })
            .collect();
        app.status = format!("{} subscriptions", app.channel_results.len());
    }
    if ui.button("Import").on_hover_text("Import subscriptions from YouTube").clicked() {
        app.import_subscriptions();
    }
}

pub fn start_search(app: &mut App) {
    let term = app.search_input.trim().to_string();
    if term.is_empty() {
        app.toast("Enter something to search for".into(), ToastKind::Info);
        return;
    }
    app.start_search(&term);
}