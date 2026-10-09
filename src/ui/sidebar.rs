use eframe::egui;

use crate::app::{App, Section};
use crate::model::{DurationFilter, Feed};
use crate::ui::{accent, card, muted};
use crate::yt;

pub fn sidebar(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("magic-tape")
                .size(20.0)
                .strong()
                .color(accent()),
        );
    });
    ui.label(
        egui::RichText::new(format!("yt-dlp {}", app.ytdlp_version))
            .size(11.0)
            .color(muted()),
    );
    ui.add_space(8.0);
    ui.separator();

    nav(app, ui, "Feeds", Section::Feeds);
    if app.section == Section::Feeds {
        ui.indent("feed_items", |ui| {
            feed_item(app, ui, Feed::Subscriptions, "Subscriptions");
            feed_item(app, ui, Feed::Trending, "Trending");
            feed_item(app, ui, Feed::Recommended, "For you");
            feed_item(app, ui, Feed::Channel, "Channel");
            if app.store.last_action().is_some() {
                feed_item(app, ui, Feed::Repeat, "Previous action");
            }
        });
    }

    ui.add_space(6.0);
    nav(app, ui, "Playlists", Section::Playlists);
    nav(app, ui, "Channels", Section::Channels);
    nav(app, ui, "Liked videos", Section::Liked);
    nav(app, ui, "Watch history", Section::History);

    ui.separator();
    ui.label(
        egui::RichText::new("QUICK ACCESS")
            .size(10.0)
            .color(muted()),
    );
    subscriptions_panel(app, ui);
}

fn nav(app: &mut App, ui: &mut egui::Ui, label: &str, section: Section) {
    let selected = app.section == section;
    let response = ui
        .selectable_label(
            selected,
            egui::RichText::new(label).strong().size(14.0),
        )
        .on_hover_text("Open section");
    if response.clicked() {
        app.select(None);
        app.section = section;
        match section {
            Section::Liked => {
                app.liked = app.store.liked();
            }
            Section::History => {
                app.history = app.store.watch_history();
                app.history.reverse();
            }
            Section::Channels => {
                let channels = app.subscriptions.clone();
                app.channel_results = channels
                    .into_iter()
                    .map(|(id, name)| crate::model::Channel {
                        thumbnail: Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")),
                        id,
                        name,
                        subscribers: None,
                    })
                    .collect();
            }
            Section::Playlists => {
                if app.playlists.is_empty() && !app.playlist_query.is_empty() {
                    app.load_playlists(&app.playlist_query.clone(), 0);
                }
            }
            Section::Feeds => {}
        }
    }
}

fn feed_item(app: &mut App, ui: &mut egui::Ui, feed: Feed, label: &str) {
    let selected = app.section == Section::Feeds && app.feed == feed;
    let response = ui
        .selectable_label(selected, egui::RichText::new(label).size(13.0))
        .on_hover_text(format!("Open the {label} feed"));
    if response.clicked() {
        app.select(None);
        app.section = Section::Feeds;
        app.feed = feed;
        if feed == Feed::Channel {
            let (id, name) = (app.channel_id.clone(), app.channel_name.clone());
            if id.is_empty() {
                app.toast("Pick a channel first".into(), crate::app::ToastKind::Info);
            } else {
                app.load_feed(Feed::Channel, 0);
                let _ = name;
            }
        } else if feed == Feed::Repeat {
            replay(app);
        } else {
            app.load_feed(feed, 0);
        }
    }
}

fn replay(app: &mut App) {
    let Some(action) = app.store.last_action() else {
        app.toast("No previous action recorded".into(), crate::app::ToastKind::Info);
        return;
    };
    app.query = yt::urldecode(&action.query);
    match action.feed.as_str() {
        "trending" => app.feed = Feed::Trending,
        "recommended" => app.feed = Feed::Recommended,
        "channel" => {
            app.feed = Feed::Channel;
            app.channel_id = action.channel.clone();
        }
        "repeat" | "subscriptions" => app.feed = Feed::Subscriptions,
        _ => app.feed = Feed::Subscriptions,
    }
    if !action.query.is_empty() && app.feed == Feed::Subscriptions {
        app.filter = DurationFilter::Any;
    }
    app.load_feed(app.feed, action.start);
}

/// Quick access to the local subscription list.
///
/// The list can run to thousands of channels, so it is filtered rather than
/// silently truncated: a filter box appears once the list is long enough to
/// need one, and the row count always says how much is hidden.
fn subscriptions_panel(app: &mut App, ui: &mut egui::Ui) {
    let total = app.subscriptions.len();

    if total > FILTER_THRESHOLD {
        ui.add(
            egui::TextEdit::singleline(&mut app.subs_filter)
                .hint_text("Filter channels...")
                .desired_width(ui.available_width())
                .font(egui::TextStyle::Small),
        );
        ui.add_space(2.0);
    }

    // Owned copies: the filter box above and the row clicks below both need
    // `&mut App`, so the list cannot hold a borrow of the cache.
    let matches = app.filtered_subscriptions();
    let filtering = !app.subs_filter.trim().is_empty();
    let shown = matches.len().min(MAX_QUICK_ACCESS_ROWS);

    if total == 0 {
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .id_salt("subs_scroll")
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("No subscriptions imported yet")
                        .size(12.0)
                        .color(muted()),
                );
                if ui.button("Import from YouTube").clicked() {
                    app.import_subscriptions();
                }
            });
        return;
    }

    egui::ScrollArea::vertical()
        .max_height(240.0)
        .id_salt("subs_scroll")
        .show(ui, |ui| {
            for (id, name) in matches.iter().take(MAX_QUICK_ACCESS_ROWS) {
                let selected = app.section == Section::Feeds
                    && app.feed == Feed::Channel
                    && app.channel_id == *id;
                let response = ui
                    .selectable_label(selected, egui::RichText::new(name).size(12.5))
                    .on_hover_text(format!("Open the feed of {name}"));
                if response.clicked() {
                    open_channel(app, id, name);
                }
            }
            if matches.is_empty() {
                ui.label(
                    egui::RichText::new("No channel matches that filter")
                        .size(12.0)
                        .color(muted()),
                );
            } else if shown < matches.len() {
                ui.label(
                    egui::RichText::new(format!(
                        "showing {shown} of {} - narrow it with the filter",
                        matches.len()
                    ))
                    .size(11.0)
                    .color(muted()),
                );
            }
        });

    ui.add_space(6.0);
    card(ui, |ui| {
        ui.label(
            egui::RichText::new(if filtering {
                format!("{} matching of {total}", matches.len())
            } else {
                format!("{total} subscriptions")
            })
            .size(11.5)
            .color(muted()),
        );
        if ui.button("Import subscriptions").clicked() {
            app.import_subscriptions();
        }
    });
}

/// Number of channels above which the quick-access list gets a filter box.
const FILTER_THRESHOLD: usize = 15;

/// Rows drawn before the panel asks the user to filter. Rendering every channel
/// of a 1000-subscription account every frame is not worth it.
const MAX_QUICK_ACCESS_ROWS: usize = 200;

fn open_channel(app: &mut App, id: &str, name: &str) {
    app.select(None);
    app.section = Section::Feeds;
    app.feed = Feed::Channel;
    app.channel_id = id.to_string();
    app.channel_name = name.to_string();
    app.subs_filter.clear();
    app.load_feed(Feed::Channel, 0);
}
