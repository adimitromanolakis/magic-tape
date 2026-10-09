use eframe::egui;

use crate::app::{App, Section};
use crate::model::{DurationFilter, Feed};
use crate::ui::{accent, card, muted};

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
                let channels = app.store.subscriptions();
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

fn subscriptions_panel(app: &mut App, ui: &mut egui::Ui) {
    let subs = app.store.subscriptions();
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .id_salt("subs_scroll")
        .show(ui, |ui| {
            if subs.is_empty() {
                ui.label(
                    egui::RichText::new("No subscriptions imported yet")
                        .size(12.0)
                        .color(muted()),
                );
                if ui.button("Import from YouTube").clicked() {
                    app.import_subscriptions();
                }
                return;
            }
            for (id, name) in subs.iter().take(200) {
                let selected = app.section == Section::Feeds
                    && app.feed == Feed::Channel
                    && app.channel_id == *id;
                let response = ui
                    .selectable_label(selected, egui::RichText::new(name).size(12.5))
                    .on_hover_text("Open this channel feed");
                if response.clicked() {
                    app.select(None);
                    app.section = Section::Feeds;
                    app.feed = Feed::Channel;
                    app.channel_id = id.clone();
                    app.channel_name = name.clone();
                    app.load_feed(Feed::Channel, 0);
                }
            }
            if subs.len() > 200 {
                ui.label(
                    egui::RichText::new(format!("+{} more in Channels", subs.len() - 200))
                        .size(11.0)
                        .color(muted()),
                );
            }
        });

    ui.add_space(6.0);
    card(ui, |ui| {
        ui.label(
            egui::RichText::new(format!("{} subscriptions", subs.len()))
                .size(11.5)
                .color(muted()),
        );
        if ui.button("Import subscriptions").clicked() {
            app.import_subscriptions();
        }
    });
}

use crate::yt;