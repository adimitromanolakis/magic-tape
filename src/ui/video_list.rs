use eframe::egui;

use crate::app::{App, Section, ToastKind};
use crate::config::ListView;
use crate::model::Video;
use crate::ui::{accent, ellipsize, muted, warn_color};

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
    let available = ui.available_width();
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
    let row_h = 74.0;
    let viewport = ui.available_height();
    let scroll = egui::ScrollArea::vertical()
        .id_salt("list_scroll")
        .auto_shrink([false, false]);
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
                    .inner_margin(4.0);
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let texture = app.thumbs.get(&video.id);
                        let response = ui.add(
                            egui::Image::new(&texture)
                                .fit_to_exact_size(egui::vec2(120.0, 68.0))
                                .corner_radius(4.0)
                                .sense(egui::Sense::click()),
                        );
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
    let row_h = 62.0;
    let viewport = ui.available_height();
    let scroll = egui::ScrollArea::vertical()
        .id_salt("channels_scroll")
        .auto_shrink([false, false]);
    let scroll = match app.take_scroll_offset(1, row_h, viewport) {
        Some(offset) => scroll.vertical_scroll_offset(offset),
        None => scroll,
    };
    app.visible_rows = (viewport / row_h).floor().max(1.0) as usize;
    scroll.show_rows(ui, row_h, count, |ui, range| {
            for index in range {
                let Some(channel) = app.channel_results.get(index).cloned() else {
                    continue;
                };
                app.ensure_channel_thumbnail(&channel);
                let selected = app.selected == Some(index);
                egui::Frame::new()
                    .fill(if selected {
                        ui.visuals().widgets.active.bg_fill
                    } else {
                        ui.visuals().panel_fill
                    })
                    .corner_radius(6.0)
                    .inner_margin(6.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let texture = app.thumbs.get(&channel.id);
                            let response = ui.add(
                                egui::Image::new(&texture)
                                    .fit_to_exact_size(egui::vec2(72.0, 40.0))
                                    .corner_radius(4.0)
                                    .sense(egui::Sense::click()),
                            );
                            response.context_menu(|ui| {
                                let channel = app.channel_results[index].clone();
                                if ui.button("Unsubscribe").clicked() {
                                    app.unsubscribe(&channel.name);
                                }
                                if ui.button("Copy channel name").clicked() {
                                    ui.ctx().copy_text(channel.name.clone());
                                }
                            });
                            if response.clicked() {
                                app.select(Some(index));
                                if let Some(channel) = app.current_channel() {
                                    app.section = Section::Feeds;
                                    app.feed = crate::model::Feed::Channel;
                                    app.channel_id = channel.id.clone();
                                    app.channel_name = channel.name.clone();
                                    app.load_feed(crate::model::Feed::Channel, 0);
                                }
                            }
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(channel.name.clone()).size(13.0).strong(),
                                    )
                                    .truncate(),
                                );
                                if let Some(subs) = &channel.subscribers {
                                    ui.label(
                                        egui::RichText::new(format!("{subs} subscribers"))
                                            .size(11.5)
                                            .color(warn_color()),
                                    );
                                }
                            });
                        });
                    });
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

