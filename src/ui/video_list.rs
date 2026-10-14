use eframe::egui;

use crate::app::{App, Section, ToastKind};
use crate::config::ListView;
use crate::model::Video;
use crate::ui::{accent, ellipsize, muted, warn_color, SCROLLBAR_GAP};

/// Row geometry for the compact video list.
///
/// `show_rows` derives the scrollable content height from the row height it is
/// given, so it has to equal the height a row really takes: the thumbnail plus
/// the frame's margin. When these disagree the scrollbar is calibrated for a
/// list that does not exist and the last rows cannot be scrolled to.
const LIST_ROW_CONTENT_H: f32 = 68.0;
const LIST_ROW_MARGIN: f32 = 4.0;
const LIST_ROW_H: f32 = LIST_ROW_CONTENT_H + 2.0 * LIST_ROW_MARGIN;

/// Row geometry for the channel list, same rule as above.
/// Width of one channel card before the gaps are taken out.
const CHANNEL_TILE_W: f32 = 190.0;
/// Room under the avatar for the name and the subscriber count.
const CHANNEL_CARD_TEXT_H: f32 = 40.0;

pub fn content(app: &mut App, ui: &mut egui::Ui) {
    match app.section {
        Section::Channels => channel_list(app, ui),
        _ => video_list(app, ui),
    }
}

fn rows(app: &App) -> usize {
    match app.section {
        Section::Feeds => app.feed_state.videos.len(),
        Section::Playlists => app.playlists.len(),
        Section::Liked => app.liked.len(),
        Section::History => app.history.len(),
        Section::Channels => app.channel_results.len(),
    }
}

fn select(app: &mut App, index: usize) {
    app.select(Some(index));
}

fn video_list(app: &mut App, ui: &mut egui::Ui) {
    let count = rows(app);
    if count == 0 {
        empty_state(app, ui);
        return;
    }
    match app.list_view {
        ListView::Grid => grid(app, ui, count),
        ListView::List => list(app, ui, count),
    }
}

fn grid(app: &mut App, ui: &mut egui::Ui, count: usize) {
    // The scrollbar is drawn over the content, so the cards are laid out in the
    // width that is left once its strip is removed.
    let available = (ui.available_width() - SCROLLBAR_GAP).max(200.0);
    let columns = ((available / 260.0).floor() as usize).clamp(1, 6);
    app.columns = columns;
    let tile = (available - 8.0 * (columns.saturating_sub(1)) as f32) / columns as f32;
    let thumb_h = (tile - 12.0) * 9.0 / 16.0;
    let card_h = thumb_h + 46.0;
    let rows = count.div_ceil(columns);
    let row_h = card_h + 8.0;
    let viewport = ui.available_height();
    let scroll = egui::ScrollArea::vertical()
        .id_salt("grid_scroll")
        .auto_shrink([false, false]);
    let scroll = match app.take_scroll_offset(columns, row_h, viewport) {
        Some(offset) => scroll.vertical_scroll_offset(offset),
        None => scroll,
    };
    app.visible_rows = (viewport / row_h).floor().max(1.0) as usize;

    scroll.show_rows(ui, row_h, rows, |ui, range| {
            for row in range {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    for col in 0..columns {
                        let index = row * columns + col;
                        if index >= count {
                            break;
                        }
                        let Some(video) = item_at(app, index) else {
                            continue;
                        };
                        app.ensure_thumbnail(&video);
                        grid_card(app, ui, index, &video, tile, thumb_h);
                    }
                });
            }
        });
    footer(app, ui);
}

fn grid_card(
    app: &mut App,
    ui: &mut egui::Ui,
    index: usize,
    video: &Video,
    tile: f32,
    thumb_h: f32,
) {
    let card_h = thumb_h + 46.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(tile, card_h), egui::Sense::click());
    let selected = app.selected == Some(index);
    let bg = if selected {
        ui.visuals().widgets.active.bg_fill
    } else {
        ui.visuals().panel_fill
    };
    let border = if selected {
        accent()
    } else {
        ui.visuals().widgets.noninteractive.bg_stroke.color
    };
    ui.painter().rect(
        rect,
        8.0,
        bg,
        egui::Stroke::new(if selected { 2.0 } else { 1.0 }, border),
        egui::StrokeKind::Inside,
    );

    let inner = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(6.0, 6.0),
        rect.right_bottom() - egui::vec2(6.0, 6.0),
    );
    let thumb_rect =
        egui::Rect::from_min_size(inner.left_top(), egui::vec2(inner.width(), thumb_h));
    let texture = app.thumbs.get(&video.id);
    egui::Image::new(&texture)
        .fit_to_exact_size(thumb_rect.size())
        .corner_radius(6.0)
        .paint_at(ui, thumb_rect);

    let painter = ui.painter();
    if let Some(duration) = &video.duration {
        painter.text(
            egui::pos2(thumb_rect.right() - 6.0, thumb_rect.bottom() - 6.0),
            egui::Align2::RIGHT_BOTTOM,
            duration,
            egui::FontId::proportional(11.0),
            egui::Color32::WHITE,
        );
    }
    if video.is_live {
        painter.text(
            egui::pos2(thumb_rect.left() + 8.0, thumb_rect.top() + 6.0),
            egui::Align2::LEFT_TOP,
            "LIVE",
            egui::FontId::proportional(11.0),
            egui::Color32::RED,
        );
    }

    // The card can only fit an ellipsised title, so hovering the picture
    // reports what was cut off. `Sense::hover` keeps the card's click target.
    ui.interact(
        thumb_rect,
        ui.make_persistent_id(("thumb_title", index)),
        egui::Sense::hover(),
    )
    .on_hover_text(video.title.clone());

    let title_font = egui::FontId::proportional(12.5);
    let sub_font = egui::FontId::proportional(11.5);
    let detail = match (&video.published, video.views_label()) {
        (Some(published), Some(views)) => format!("{views} - {published}"),
        (None, Some(views)) => views,
        (Some(published), None) => published.clone(),
        (None, None) => String::new(),
    };
    let sub = if detail.is_empty() {
        video.channel.clone()
    } else {
        format!("{} - {detail}", video.channel)
    };
    let title = ellipsize(ui, &video.title, &title_font, inner.width());
    let sub = ellipsize(ui, &sub, &sub_font, inner.width());
    painter.text(
        egui::pos2(inner.left(), thumb_rect.bottom() + 15.0),
        egui::Align2::LEFT_BOTTOM,
        title,
        title_font,
        ui.visuals().text_color(),
    );
    painter.text(
        egui::pos2(inner.left(), inner.bottom()),
        egui::Align2::LEFT_BOTTOM,
        sub,
        sub_font,
        muted(),
    );

    handle_click(app, &response, index, video);
}

fn list(app: &mut App, ui: &mut egui::Ui, count: usize) {
    // One item per row. Paging reads this, and a stale value left over from
    // the grid would multiply every page step by the old column count.
    app.columns = 1;
    let row_h = LIST_ROW_H;
    let viewport = ui.available_height();
    let scroll = egui::ScrollArea::vertical()
        .id_salt("list_scroll")
        .auto_shrink([false, false])
        .max_width((ui.available_width() - SCROLLBAR_GAP).max(200.0));
    let scroll = match app.take_scroll_offset(1, row_h, viewport) {
        Some(offset) => scroll.vertical_scroll_offset(offset),
        None => scroll,
    };
    app.visible_rows = (viewport / row_h).floor().max(1.0) as usize;
    scroll.show_rows(ui, row_h, count, |ui, range| {
            for index in range {
                let Some(video) = item_at(app, index) else {
                    continue;
                };
                app.ensure_thumbnail(&video);
                let selected = app.selected == Some(index);
                let frame = egui::Frame::new()
                    .fill(if selected {
                        ui.visuals().widgets.active.bg_fill
                    } else {
                        ui.visuals().panel_fill
                    })
                    .corner_radius(6.0)
                    .inner_margin(LIST_ROW_MARGIN);
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let texture = app.thumbs.get(&video.id);
                        let response = ui
                            .add(
                                egui::Image::new(&texture)
                                    .fit_to_exact_size(egui::vec2(120.0, 68.0))
                                    .corner_radius(4.0)
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_text(video.title.clone());
                        handle_click(app, &response, index, &video);
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(video.title.clone())
                                        .size(13.0)
                                        .strong(),
                                )
                                .truncate(),
                            );
                            let mut meta = format!(
                                "{} - {}",
                                video.channel,
                                video.published.clone().unwrap_or_else(|| "unknown".into())
                            );
                            if let Some(views) = video.views_label() {
                                meta.push_str(&format!(" - {views}"));
                            }
                            if let Some(duration) = &video.duration {
                                meta.push_str(&format!(" - {duration}"));
                            }
                            ui.add(
                                egui::Label::new(egui::RichText::new(meta).size(11.5).color(muted()))
                                    .truncate(),
                            );
                        });
                    });
                });
            }
        });
    footer(app, ui);
}

fn handle_click(app: &mut App, response: &egui::Response, index: usize, video: &Video) {
    if response.clicked() {
        select(app, index);
    }
    if response.double_clicked() {
        select(app, index);
        app.play_selected("1080p");
    }
    response.context_menu(|ui| {
        if ui.button("Play 1080p").clicked() {
            select(app, index);
            app.play_selected("1080p");
        }
        if ui.button("Play best").clicked() {
            select(app, index);
            app.play_selected("best");
        }
        if ui.button("Play audio only").clicked() {
            select(app, index);
            app.play_selected("audio");
        }
        ui.separator();
        if ui.button("Download video").clicked() {
            select(app, index);
            app.download_selected(false);
        }
        if ui.button("Download audio").clicked() {
            select(app, index);
            app.download_selected(true);
        }
        if ui.button("Toggle like").clicked() {
            select(app, index);
            app.toggle_like();
        }
        if ui.button("Subscribe to channel").clicked() {
            select(app, index);
            app.subscribe_to_current();
        }
        if ui.button("Open in browser").clicked()
            && let Err(error) = crate::player::open_browser(&video.url) {
                app.toast(error.to_string(), ToastKind::Error);
            }
    });
}

fn item_at(app: &App, index: usize) -> Option<Video> {
    match app.section {
        Section::Feeds => app.feed_state.videos.get(index).cloned(),
        Section::Playlists => app.playlists.get(index).cloned(),
        Section::Liked => app.liked.get(index).map(crate::app::entry_to_video),
        Section::History => app.history.get(index).map(crate::app::entry_to_video),
        Section::Channels => None,
    }
}

fn channel_list(app: &mut App, ui: &mut egui::Ui) {
    let count = app.channel_results.len();
    if count == 0 {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("No channels yet - search above or import your subscriptions")
                    .color(muted()),
            );
        });
        return;
    }
    // A grid of equal cards rather than a ragged list: the names line up in
    // columns, so the eye scans down them instead of chasing a ragged edge.
    let available = (ui.available_width() - SCROLLBAR_GAP).max(200.0);
    let columns = ((available / CHANNEL_TILE_W).floor() as usize).clamp(1, 8);
    app.columns = columns;
    let gap = 8.0;
    let tile = (available - gap * (columns.saturating_sub(1)) as f32) / columns as f32;
    let avatar = (tile - 24.0).clamp(40.0, 104.0);
    let card_h = avatar + CHANNEL_CARD_TEXT_H;
    let row_h = card_h + gap;
    let rows = count.div_ceil(columns);
    let viewport = ui.available_height();
    let scroll = egui::ScrollArea::vertical()
        .id_salt("channels_scroll")
        .auto_shrink([false, false]);
    let scroll = match app.take_scroll_offset(columns, row_h, viewport) {
        Some(offset) => scroll.vertical_scroll_offset(offset),
        None => scroll,
    };
    app.visible_rows = (viewport / row_h).floor().max(1.0) as usize;

    scroll.show_rows(ui, row_h, rows, |ui, range| {
        for row in range {
            ui.spacing_mut().item_spacing.x = gap;
            for column in 0..columns {
                let index = row * columns + column;
                if index >= count {
                    ui.add_space(tile);
                    continue;
                }
                let channel = app.channel_results[index].clone();
                app.ensure_channel_thumbnail(&channel);
                let selected = app.selected == Some(index);

                // Painted by hand, like the quick-access rows: one allocation
                // gives both the hover state and the click target.
                let size = egui::vec2(tile, card_h);
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                let fill = if selected {
                    ui.visuals().widgets.active.bg_fill
                } else if response.hovered() {
                    ui.visuals().widgets.hovered.bg_fill
                } else {
                    ui.visuals().panel_fill
                };
                ui.painter().rect_filled(rect, 8.0, fill);

                let inner = rect.shrink(8.0);
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Center)),
                );
                if app.thumbs.has(&channel.id) {
                    let texture = app.thumbs.get(&channel.id);
                    child.add(
                        egui::Image::new(&texture)
                            .fit_to_exact_size(egui::vec2(avatar, avatar))
                            .corner_radius(avatar / 2.0),
                    );
                } else {
                    // Until the picture arrives - and for any channel that
                    // publishes none - show the initial, not a checkerboard.
                    let avatar_rect =
                        egui::Rect::from_center_size(inner.center(), egui::vec2(avatar, avatar));
                    child.painter().circle_filled(
                        avatar_rect.center(),
                        avatar / 2.0,
                        child.visuals().widgets.inactive.bg_fill,
                    );
                    let initial: String = channel.name.chars().take(1).collect();
                    child.painter().text(
                        avatar_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        initial.to_uppercase(),
                        egui::FontId::proportional(avatar * 0.4),
                        child.visuals().weak_text_color(),
                    );
                }
                child.add_space(4.0);
                let font = egui::FontId::proportional(12.5);
                let label = ellipsize(&child, &channel.name, &font, inner.width());
                child.add(
                    egui::Label::new(egui::RichText::new(label).size(12.5).strong())
                        .truncate(),
                );
                if let Some(subs) = &channel.subscribers {
                    child.label(
                        egui::RichText::new(format!("{subs} subscribers"))
                            .size(11.0)
                            .color(warn_color()),
                    );
                }

                response.context_menu(|ui| {
                    if ui.button("Unsubscribe").clicked() {
                        app.unsubscribe(&channel.name);
                    }
                    if ui.button("Copy channel name").clicked() {
                        ui.ctx().copy_text(channel.name.clone());
                    }
                });
                if response.clicked() {
                    app.select(Some(index));
                    app.section = Section::Feeds;
                    app.feed = crate::model::Feed::Channel;
                    app.channel_id = channel.id.clone();
                    app.channel_name = channel.name.clone();
                    app.load_feed(crate::model::Feed::Channel, 0);
                }
            }
        }
    });
}

fn empty_state(app: &mut App, ui: &mut egui::Ui) {
    if app.feed_state.loading || app.worker.busy() {
        ui.centered_and_justified(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(egui::RichText::new(app.status.clone()).color(muted()));
        });
        return;
    }
    ui.centered_and_justified(|ui| {
        ui.add_space(40.0);
        match app.section {
            Section::Liked => {
                ui.label("You have not liked any videos yet");
            }
            Section::History => {
                ui.label("Nothing in your watch history yet");
            }
            _ => {
                ui.label("No videos here yet - pick another feed or search");
            }
        }
    });
}

fn footer(app: &mut App, ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        let start = app.feed_state.start;
        crate::ui::toolbar::paging_controls(app, ui);
        ui.label(
            egui::RichText::new(format!(
                "{} items loaded{}",
                rows(app),
                if start > 0 { format!(" (from {start})") } else { String::new() }
            ))
            .size(11.5)
            .color(muted()),
        );
        if app.feed_state.loading {
            ui.add(egui::Spinner::new().size(14.0));
        }
    });
}

