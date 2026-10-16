use eframe::egui;

use crate::app::{App, Section, ToastKind};
use crate::config::ListView;
use crate::model::{DurationFilter, Feed};
use crate::ui::{
    accent, focus_search_input, input, muted, submitted, text_width, title_color, CARD_RADIUS,
    SEARCH_INPUT_ID, T_BODY, T_CAPTION, T_META, T_PAGE,
};

pub fn toolbar(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        page_title(ui, app.section.title());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Keep the buttons clear of the panel's right edge.
            ui.add_space(12.0);
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
                    .size(T_CAPTION)
                    .color(muted()),
            );
        });
    });

    // The controls sit on a subtle rounded band, so the second row reads as a
    // toolbar rather than a loose row of widgets, and it is inset from both
    // edges the same way the title is.
    let width = ui.available_width();
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .corner_radius(CARD_RADIUS)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_min_width((width - 20.0).max(0.0));
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
        });
}

/// The page title in the top bar.
///
/// A small rounded accent bar, then the title in the theme's strongest colour
/// with a hairline accent underline. It is inset from the panel edge so it does
/// not sit flush against the left of the window.
fn page_title(ui: &mut egui::Ui, title: &str) {
    let font = egui::FontId::proportional(T_PAGE);
    let indent = 12.0;
    let bar_w = 4.0;
    let gap = 9.0;
    let text_w = text_width(ui, title, &font);
    let block = egui::vec2(indent + bar_w + gap + text_w + 6.0, T_PAGE + 12.0);
    let (rect, _response) = ui.allocate_exact_size(block, egui::Sense::hover());
    let painter = ui.painter();

    let bar = egui::Rect::from_center_size(
        egui::pos2(rect.left() + indent + bar_w / 2.0, rect.center().y - 2.0),
        egui::vec2(bar_w, T_PAGE * 1.1),
    );
    painter.rect_filled(bar, bar_w / 2.0, accent());
    painter.text(
        egui::pos2(bar.right() + gap, bar.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        font,
        title_color(ui.visuals()),
    );

    let underline = egui::Rect::from_min_size(
        egui::pos2(bar.right() + gap, bar.bottom() + 5.0),
        egui::vec2(text_w, 2.0),
    );
    painter.rect_filled(underline, 1.0, accent().gamma_multiply(0.55));
}

fn feed_controls(app: &mut App, ui: &mut egui::Ui) {
    match app.feed {
        Feed::Channel => {
            ui.label(
                egui::RichText::new(format!("channel: {}", app.channel_name))
                    .size(T_BODY)
                    .color(muted()),
            );
        }
        Feed::Search => {
            ui.label(
                egui::RichText::new(format!("results for \"{}\"", app.query))
                    .size(T_BODY)
                    .color(muted()),
            );
        }
        _ => {
            ui.label(
                egui::RichText::new(app.feed.label())
                    .size(T_BODY)
                    .color(muted()),
            );
        }
    }
    ui.separator();
    // The search box stays available everywhere. It used to be swapped for the
    // term and the duration filters as soon as a query existed, which left no
    // way to start another search.
    let search_field = input(ui, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.search_input)
                .id_source(SEARCH_INPUT_ID)
                .hint_text("Search YouTube...")
                .desired_width(220.0),
        )
    });
    if app.focus_search {
        // Only this section carries the main search box, so the flag is
        // cleared here rather than when the key is pressed.
        app.focus_search = false;
        focus_search_input(ui);
    }
    // Enter searches too, so the field does not need a mouse trip to the
    // button after typing.
    let search_clicked = ui.button("Search").clicked();
    if submitted(ui, &search_field, search_clicked) {
        start_search(app);
    }
    if ui.button("History").on_hover_text("Search history").clicked()
        && let Some(term) = app.store.search_history().first().cloned()
    {
        app.search_input = term;
    }
    if app.feed == Feed::Search {
        ui.label(
            egui::RichText::new(format!("\"{}\"", app.query))
                .size(T_BODY)
                .color(accent()),
        );
        for filter in [
            DurationFilter::Any,
            DurationFilter::Short,
            DurationFilter::Medium,
            DurationFilter::Long,
        ] {
            if ui
                .selectable_label(app.filter == filter, egui::RichText::new(filter.label()).size(T_META))
                .clicked()
            {
                app.filter = filter;
                app.load_feed(app.feed, 0);
            }
            ui.add_space(4.0);
        }
        if ui
            .button("⬅ Back")
            .on_hover_text("Leave the search results and return to the previous feed")
            .clicked()
        {
            app.leave_search();
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
        format!("\u{2b05} Prev ({})", start.saturating_sub(step) + 1)
    } else {
        "\u{2b05} Prev".to_string()
    };
    if ui
        .add_enabled(start > 0, egui::Button::new(prev_label))
        .on_hover_text("Load the previous page (,)")
        .clicked()
    {
        app.load_prev_page();
    }

    let next_label = if app.has_more() {
        format!("Next ({}) \u{27a1}", total + step)
    } else {
        "Next \u{27a1}".to_string()
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
    let field = input(ui, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.playlist_query)
                .hint_text("Search playlists...")
                .desired_width(240.0),
        )
    });
    let search_clicked = ui.button("Search").clicked();
    if submitted(ui, &field, search_clicked) {
        app.load_playlists(&app.playlist_query.clone(), 0);
    }
}

fn channel_controls(app: &mut App, ui: &mut egui::Ui) {
    let field = input(ui, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.channel_query)
                .hint_text("Find channels on YouTube...")
                .desired_width(240.0),
        )
    });
    let search_clicked = ui.button("Search").clicked();
    if submitted(ui, &field, search_clicked) {
        app.search_channels(&app.channel_query.clone(), 0);
    }
    if ui.button("My subscriptions").on_hover_text("List your imported subscriptions").clicked() {
        let subs = app.subscriptions.clone();
        app.channel_results = subs
            .into_iter()
            .map(|(id, name)| crate::model::Channel {
                // The subscriptions file has no avatar URL; one is fetched from
                // the channel page and cached, rather than guessed at.
                thumbnail: None,
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