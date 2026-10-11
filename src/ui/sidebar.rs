use eframe::egui;

use crate::app::{App, Section};
use crate::model::{DurationFilter, Feed};
use crate::ui::{accent, card, ellipsize, muted, SCROLLBAR_GAP};
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
                        // No avatar URL to hand over; the Channels page fetches
                        // one from the channel page instead of guessing.
                        thumbnail: None,
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
        "search" => app.feed = Feed::Search,
        "repeat" | "subscriptions" => app.feed = Feed::Subscriptions,
        _ => app.feed = Feed::Subscriptions,
    }
    // Only the search feed carries a term, so only it has to drop the filter.
    if !action.query.is_empty() && app.feed == Feed::Search {
        app.filter = DurationFilter::Any;
    } else if app.feed != Feed::Search {
        app.query.clear();
    }
    app.load_feed(app.feed, action.start);
}

/// Quick access to the local subscription list.
///
/// Every subscription is shown, virtualised so that costs nothing to have
/// hundreds of them. The filter box is a convenience for long lists, not the
/// only way to reach a channel.
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

    // Indices, not cloned rows: the list is virtualised, so only the handful of
    // rows on screen need their text, and the click handler needs `&mut App`.
    let matches = app.filtered_subscription_indices();
    let matching = matches.len();
    let filtering = !app.subs_filter.trim().is_empty();

    if total == 0 {
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .auto_shrink([false, false])
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

    // Every subscription is listed - accounts run to well over 300 - so the rows
    // are virtualised. The row height is the height a selectable row really
    // takes, because `show_rows` derives the content height from it.
    let row_height = ui.spacing().interact_size.y;
    if matching == 0 {
        ui.label(
            egui::RichText::new("No channel matches that filter")
                .size(12.0)
                .color(muted()),
        );
    } else {
        egui::ScrollArea::vertical()
            .id_salt("subs_scroll")
            .max_height(240.0)
            // Without this the box shrinks to the width of the rows and the bar
            // is drawn inset from the panel edge instead of at its right side.
            .auto_shrink([false, false])
            .show_rows(ui, row_height, matching, |ui, range| {
                // egui draws a vertical scrollbar *over* the content instead of
                // reserving room for it, so each row is allocated to stop short
                // of the bar. Sizing the row (rather than the scroll area) is
                // what works: `max_width` narrows the box, and the bar is then
                // drawn at that box's edge, so the text still ran underneath it.
                //
                // The row is drawn by hand rather than with `selectable_label`
                // because that centres its text, and because an explicit
                // allocation is what keeps the height equal to the `row_height`
                // `show_rows` was told about - which is what the scrollbar is
                // calibrated from.
                let width = (ui.available_width() - SCROLLBAR_GAP).max(60.0);
                let font = egui::FontId::proportional(12.5);
                let paint_font = font.clone();
                let text = ui.visuals().text_color();
                for index in range {
                    let Some(slot) = matches.get(index).copied() else {
                        continue;
                    };
                    let Some((id, name)) = app.subscriptions.get(slot) else {
                        continue;
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::click());
                    let selected = app.section == Section::Feeds
                        && app.feed == Feed::Channel
                        && app.channel_id == *id;
                    if selected || response.hovered() {
                        let fill = if selected {
                            ui.visuals().widgets.active.bg_fill
                        } else {
                            ui.visuals().widgets.hovered.bg_fill
                        };
                        ui.painter().rect_filled(rect, 4.0, fill);
                    }
                    let label = ellipsize(ui, name, &font, width - 12.0);
                    ui.painter().text(
                        egui::pos2(rect.left() + 6.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        label,
                        paint_font.clone(),
                        text,
                    );
                    if response.clicked() {
                        let (id, name) = app.subscriptions[slot].clone();
                        open_channel(app, &id, &name);
                    }
                }
            });
    }

    ui.add_space(6.0);
    card(ui, |ui| {
        ui.label(
            egui::RichText::new(if filtering {
                format!("{matching} matching of {total}")
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

fn open_channel(app: &mut App, id: &str, name: &str) {
    app.select(None);
    app.section = Section::Feeds;
    app.feed = Feed::Channel;
    app.channel_id = id.to_string();
    app.channel_name = name.to_string();
    app.subs_filter.clear();
    app.load_feed(Feed::Channel, 0);
}
