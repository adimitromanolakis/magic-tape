use eframe::egui;

use crate::app::{App, ToastKind};
use crate::config::{CommentsSort, PinnedPlacement};
use crate::ui::{
    accent, muted, ok_color, pane_fill, warn_color, SCROLLBAR_GAP, T_BODY, T_CAPTION, T_META,
    T_TITLE,
};

/// Narrowest and widest the rail can be dragged to.
const MIN_WIDTH: f32 = 260.0;
const MAX_WIDTH: f32 = 620.0;
/// Pointer width of the grab strip on the pane's left edge.
const HANDLE_WIDTH: f32 = 5.0;
/// Height of the band the thumbnail is centred in at the top of the pane.
const THUMB_HEIGHT: f32 = 170.0;

/// The detail pane: a rail anchored to the window's right edge, full height.
///
/// It is an `Area` rather than a `Panel::right`, and that is what keeps the
/// grid still: `grid()` derives its column count from the central panel's
/// width, so a panel would resize every card and move every row each time the
/// selection changed. This rail is drawn over the list's right-most column
/// instead. It starts below the toolbar, so no control is ever covered, and
/// runs to the window's bottom edge over the right end of the status bar,
/// which is what the panel it replaced did.
pub fn detail_panel(app: &mut App, ctx: &egui::Context) {
    if !app.detail_open {
        return;
    }
    let Some(selected) = app.current_video() else {
        return;
    };
    if app.detail.video.is_none() {
        app.detail.video = Some(selected.clone());
    }
    let video = app.detail.video.clone().unwrap_or(selected);
    // Where the list drew itself this frame; without it there is nothing to
    // anchor to yet.
    let Some(area) = app.content_rect else {
        return;
    };

    let window = ctx.content_rect();
    let width = app
        .detail_width
        .clamp(MIN_WIDTH, MAX_WIDTH.min((window.width() / 2.0).max(MIN_WIDTH)));
    let rect = egui::Rect::from_min_max(
        egui::pos2(window.right() - width, area.top()),
        window.right_bottom(),
    );

    egui::Area::new(egui::Id::new("detail_pane"))
        // Above the panels, which all share the base layer, and below windows,
        // popups and tooltips - the order the pane had as a panel.
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .fade_in(false)
        .show(ctx, |ui| pane(app, ui, &video, rect));
}

/// The rail itself: a fixed-size frame, clipped to `rect`, holding one
/// scrolling column with the close button floating over its top.
fn pane(app: &mut App, ui: &mut egui::Ui, video: &crate::model::Video, rect: egui::Rect) {
    ui.set_min_size(rect.size());
    ui.set_max_size(rect.size());
    let fill = pane_fill(&app.config.detail_background, ui.visuals());
    // Almost no top margin: the picture is the first thing in the pane, and the
    // close button floats over it instead of claiming a row of its own.
    let frame = egui::Frame::new().fill(fill).inner_margin(egui::Margin {
        left: 12,
        right: 12,
        top: 6,
        bottom: 8,
    });
    let inner = rect.size() - frame.total_margin().sum();
    frame.show(ui, |ui| {
        ui.scope(|ui| {
            ui.set_min_size(inner);
            // A panel clipped what overflowed it; an area does not.
            ui.set_clip_rect(rect.intersect(ui.clip_rect()));
            body(app, ui, video);
        });
        resize_strip(app, ui, inner);
    });
    // After the scroller, so the button is painted over the picture and stays
    // where it is while the details scroll beneath it.
    close_button(app, ui, rect);
    // The separator a panel draws between itself and the list beside it.
    ui.painter().vline(
        rect.left(),
        rect.y_range(),
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
}

/// The close affordance, floating in the pane's top-right corner.
///
/// It is painted, not laid out, so the picture can start at the top of the
/// pane. The cross is two line segments rather than a glyph: egui has no bold
/// face (`RichText::strong()` only swaps in a brighter colour), so a heavier X
/// means a heavier stroke. It carries its own dark disc because it sits over a
/// photograph, where the theme's text colour is not readable.
fn close_button(app: &mut App, ui: &mut egui::Ui, pane: egui::Rect) {
    /// Hit area - generous, since the cross itself is only drawn lines.
    const SIZE: f32 = 20.0;
    /// Half the width of the cross.
    const ARM: f32 = 4.0;
    const WEIGHT: f32 = 3.2;
    /// Gap between the button and the pane's corner.
    const INSET_X: f32 = 10.0;
    const INSET_Y: f32 = 3.0;

    let rect = egui::Rect::from_min_size(
        egui::pos2(pane.right() - INSET_X - SIZE, pane.top() + INSET_Y),
        egui::vec2(SIZE, SIZE),
    );
    let response = ui.interact(rect, egui::Id::new("detail_close"), egui::Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    // Dark enough at rest to hold a white cross over a white page - the light
    // theme's pane fill is close to the glyph's own colour - and darker still
    // under the pointer.
    let disc = if hovered {
        egui::Color32::from_black_alpha(190)
    } else {
        egui::Color32::from_black_alpha(20)
    };
    let color = if hovered {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_white_alpha(225)
    };
    ui.painter().circle_filled(rect.center(), SIZE / 2.0, disc);
    let center = rect.center();
    let stroke = egui::Stroke::new(WEIGHT, color);
    for (x, y) in [(-1.0, -1.0), (-1.0, 1.0)] {
        let corner = egui::vec2(ARM * x, ARM * y);
        ui.painter()
            .line_segment([center + corner, center - corner], stroke);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        app.detail_open = false;
    }
    let _ = response.on_hover_text("Hide the details pane");
}

/// The pane's content: one column in one scroller. The comments used to have a
/// scroller of their own inside a pane that could not scroll at all, which left
/// everything below the fold unreachable.
fn body(app: &mut App, ui: &mut egui::Ui, video: &crate::model::Video) {
    let scroll = egui::ScrollArea::vertical()
        .id_salt("detail_scroll")
        .auto_shrink([false, false]);
    scroll.show(ui, |ui| {
        ui.scope(|ui| {
            // egui draws its scrollbar over the content, so keep the column
            // clear of the bar rather than letting text run under it.
            ui.set_max_width((ui.available_width() - SCROLLBAR_GAP).max(80.0));
            ui.vertical(|ui| {
                thumbnail(app, ui, &video.id);
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(&video.title).size(T_TITLE).strong())
                        .wrap(),
                );
                ui.label(
                    egui::RichText::new(&video.channel)
                        .size(T_BODY)
                        .color(muted()),
                );
                let mut meta = String::new();
                if let Some(views) = video.views_label() {
                    meta.push_str(&views);
                }
                if let Some(published) = &video.published {
                    if !meta.is_empty() {
                        meta.push_str(" - ");
                    }
                    meta.push_str(published);
                }
                if let Some(duration) = &video.duration {
                    if !meta.is_empty() {
                        meta.push_str(" - ");
                    }
                    meta.push_str(duration);
                }
                if !meta.is_empty() {
                    ui.label(egui::RichText::new(meta).size(T_META).color(muted()));
                }
                ui.add_space(4.0);
                actions(app, ui, &video.url);
                ui.add_space(4.0);
                meta_row(app, ui, video);
                ui.separator();
                description(app, ui);
                ui.separator();
                comments(app, ui);
            });
        });
    });
}

/// The picture at the top of the pane, centred in its band.
///
/// The frame keeps its own shape and is *not* stretched to fill the pane: a
/// 16:9 thumbnail forced into a tall box is squashed on a narrow pane and
/// stretched on a wide one, and neither reads as deliberate. The band keeps its
/// height whatever the picture's shape, so everything below it stays put while
/// thumbnails arrive.
fn thumbnail(app: &App, ui: &mut egui::Ui, id: &str) {
    let texture = app.thumbs.get(id);
    let available = ui.available_width().max(80.0);
    let band = egui::vec2(available, THUMB_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(band, egui::Sense::hover());
    let size = texture.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let scale = (band.x / size.x).min(band.y / size.y);
    let target = egui::Rect::from_center_size(rect.center(), size * scale);
    egui::Image::new(&texture)
        .fit_to_exact_size(target.size())
        .corner_radius(8.0)
        .paint_at(ui, target);
}

/// The grab strip on the pane's left edge: dragging left widens the pane, as
/// dragging the old panel's edge did.
fn resize_strip(app: &mut App, ui: &mut egui::Ui, size: egui::Vec2) {
    let rect = egui::Rect::from_min_size(ui.min_rect().min, size);
    let strip = egui::Rect::from_min_max(
        rect.left_top(),
        egui::pos2(rect.left() + HANDLE_WIDTH, rect.bottom()),
    );
    let response = ui.interact(strip, egui::Id::new("detail_resize"), egui::Sense::drag());
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if response.dragged() {
        app.detail_width = (app.detail_width - response.drag_delta().x).clamp(MIN_WIDTH, MAX_WIDTH);
    }
}

fn actions(app: &mut App, ui: &mut egui::Ui, url: &str) {
    ui.horizontal(|ui| {
        for (label, profile) in [("1080p", "1080p"), ("720p", "720p"), ("Best", "best")] {
            let response = ui.add_sized(
                [86.0, 26.0],
                egui::Button::new(egui::RichText::new(label).size(T_BODY).strong()),
            );
            if response.clicked() {
                app.play_selected(profile);
            }
        }
    });
    ui.horizontal(|ui| {
        if ui
            .add_sized([86.0, 26.0], egui::Button::new("Audio"))
            .on_hover_text("Play audio only")
            .clicked()
        {
            app.play_selected("audio");
        }
        if ui
            .add_sized([86.0, 26.0], egui::Button::new("Download"))
            .clicked()
        {
            app.download_selected(false);
        }
        if ui
            .add_sized([86.0, 26.0], egui::Button::new("MP3"))
            .on_hover_text("Download audio only")
            .clicked()
        {
            app.download_selected(true);
        }
    });
    ui.horizontal(|ui| {
        let liked = app.store.is_liked(url);
        let label = if liked { "Liked" } else { "Like" };
        if ui
            .add_sized([86.0, 26.0], egui::Button::new(label))
            .clicked()
        {
            app.toggle_like();
        }
        let subscribed = app
            .current_video()
            .map(|v| app.is_subscribed(&v.channel_id))
            .unwrap_or(false);
        let sub_label = if subscribed { "Subscribed" } else { "Subscribe" };
        if ui
            .add_sized([86.0, 26.0], egui::Button::new(sub_label))
            .on_hover_text("Subscribe to this channel locally")
            .clicked()
        {
            app.subscribe_to_current();
        }
        if ui
            .add_sized([86.0, 26.0], egui::Button::new("Browser"))
            .clicked()
            && let Err(error) = crate::player::open_browser(url) {
                app.toast(error.to_string(), ToastKind::Error);
            }
    });
}

fn meta_row(app: &mut App, ui: &mut egui::Ui, video: &crate::model::Video) {
    ui.horizontal(|ui| {
        if ui
            .link("Copy link")
            .on_hover_text("Copy the video URL to the clipboard")
            .clicked()
        {
            ui.ctx().copy_text(video.url.clone());
            app.toast("Link copied to clipboard".into(), ToastKind::Success);
        }
        if ui
            .link("Channel feed")
            .on_hover_text("Browse this channel's uploads")
            .clicked()
        {
            app.section = crate::app::Section::Feeds;
            app.feed = crate::model::Feed::Channel;
            app.channel_id = video.channel_id.clone();
            app.channel_name = video.channel.clone();
            app.select(None);
            app.load_feed(crate::model::Feed::Channel, 0);
        }
        if !video.url.is_empty() && ui.link("yt-dlp JSON").clicked() {
            ui.ctx().copy_text(video.url.clone());
        }
    });
    if video.is_upcoming {
        ui.label(
            egui::RichText::new("This is an upcoming premiere")
                .size(T_META)
                .color(warn_color()),
        );
    }
}

fn description(app: &App, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("DESCRIPTION").size(T_CAPTION).color(muted()));
    let text = if app.detail.description.is_empty() {
        if app.detail.loading {
            "Loading...".to_string()
        } else {
            "No description available".to_string()
        }
    } else {
        app.detail.description.clone()
    };
    let color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: ui.available_width(),
            max_rows: 10,
            overflow_character: None,
            break_anywhere: true,
        },
        ..Default::default()
    };
    job.append(
        &text,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::proportional(T_BODY),
            color,
            ..Default::default()
        },
    );
    ui.add(egui::Label::new(job));
}

fn comments(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("COMMENTS ({})", app.detail.comment_count))
                .size(T_CAPTION)
                .color(muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            egui::ComboBox::from_id_salt("comment_sort")
                .selected_text(match app.config.comments_sort {
                    CommentsSort::New => "newest",
                    CommentsSort::Top => "top",
                    CommentsSort::Old => "oldest",
                })
                .width(80.0)
                .show_ui(ui, |ui| {
                    for sort in [CommentsSort::Old, CommentsSort::New, CommentsSort::Top] {
                        let label = match sort {
                            CommentsSort::New => "newest",
                            CommentsSort::Top => "top",
                            CommentsSort::Old => "oldest",
                        };
                        if ui
                            .selectable_label(app.config.comments_sort == sort, label)
                            .clicked()
                        {
                            app.config.comments_sort = sort;
                            if let Some(video) = app.current_video() {
                                app.load_comments_for(video);
                            }
                        }
                    }
                });
        });
    });

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Pinned comment")
                .size(T_META)
                .color(muted()),
        );
        ui.add(
            egui::RadioButton::new(
                app.config.pinned_comments == PinnedPlacement::First,
                "top",
            ),
        );
        ui.add(
            egui::RadioButton::new(
                app.config.pinned_comments == PinnedPlacement::Last,
                "bottom",
            ),
        );
        if ui.small_button("Reload").clicked()
            && let Some(video) = app.current_video() {
                app.load_comments_for(video);
            }
    });

    if let Some(error) = &app.detail.error {
        ui.label(egui::RichText::new(error).size(T_META).color(egui::Color32::RED));
        return;
    }
    if app.detail.loading {
        ui.add(egui::Spinner::new().size(16.0));
        return;
    }
    if app.detail.comments.is_empty() {
        ui.label(
            egui::RichText::new("No comments loaded")
                .size(T_META)
                .color(muted()),
        );
        return;
    }

    let pinned_first = app.config.pinned_comments == PinnedPlacement::First;
    let comments = app.detail.comments.clone();
    let mut ordered = Vec::with_capacity(comments.len());
    let (pinned, rest): (Vec<_>, Vec<_>) = comments
        .into_iter()
        .partition(|c| c.is_pinned);
    if pinned_first {
        ordered.extend(pinned);
        ordered.extend(rest);
    } else {
        ordered.extend(rest);
        ordered.extend(pinned);
    }

    // No scroller of its own: the whole pane scrolls as one page.
    for comment in ordered {
        comment_card(ui, &comment);
    }
}

fn comment_card(ui: &mut egui::Ui, comment: &crate::model::Comment) {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .corner_radius(6.0)
        .inner_margin(8.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&comment.author)
                        .size(T_BODY)
                        .strong()
                        .color(accent()),
                );
                if comment.is_pinned {
                    ui.label(
                        egui::RichText::new("pinned")
                            .size(T_CAPTION)
                            .color(warn_color()),
                    );
                }
                ui.label(
                    egui::RichText::new(&comment.time_text)
                        .size(T_CAPTION)
                        .color(muted()),
                );
            });
            ui.add(egui::Label::new(egui::RichText::new(&comment.text).size(T_BODY)).wrap());
            ui.label(
                egui::RichText::new(format!("{} likes", comment.like_count))
                    .size(T_CAPTION)
                    .color(ok_color()),
            );
        });
    ui.add_space(4.0);
}
