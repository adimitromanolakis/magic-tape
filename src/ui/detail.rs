use eframe::egui;

use crate::app::{App, ToastKind};
use crate::config::{CommentsSort, PinnedPlacement};
use crate::ui::{accent, muted, ok_color, warn_color};

pub fn detail_panel(app: &mut App, _ctx: &egui::Context, root: &mut egui::Ui) {
    let Some(video) = app.current_video() else {
        return;
    };
    if app.detail.video.is_none() {
        app.detail.video = Some(video.clone());
    }
    let video = app.detail.video.clone().unwrap_or(video);

    egui::Panel::right("detail")
        .resizable(true)
        .default_size(340.0)
        .min_size(260.0)
        .show(root, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let texture = app.thumbs.get(&video.id);
                    ui.add(
                        egui::Image::new(&texture)
                            .fit_to_exact_size(egui::vec2(ui.available_width() - 8.0, 170.0))
                            .corner_radius(8.0),
                    );
                });
                ui.add_space(6.0);
                ui.add(
                    egui::Label::new(egui::RichText::new(&video.title).size(14.0).strong())
                        .wrap(),
                );
                ui.label(
                    egui::RichText::new(&video.channel)
                        .size(12.0)
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
                    ui.label(egui::RichText::new(meta).size(11.5).color(muted()));
                }
                ui.add_space(4.0);
                actions(app, ui, &video.url);
                ui.add_space(4.0);
                meta_row(app, ui, &video);
                ui.separator();
                description(app, ui);
                ui.separator();
                comments(app, ui);
            });
        });
}

fn actions(app: &mut App, ui: &mut egui::Ui, url: &str) {
    ui.horizontal(|ui| {
        for (label, profile) in [("1080p", "1080p"), ("720p", "720p"), ("Best", "best")] {
            let response = ui.add_sized(
                [86.0, 26.0],
                egui::Button::new(egui::RichText::new(label).size(12.0).strong()),
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
            .map(|v| app.store.is_subscribed(&v.channel_id))
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
                .size(11.5)
                .color(warn_color()),
        );
    }
}

fn description(app: &App, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("DESCRIPTION").size(11.0).color(muted()));
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
            font_id: egui::FontId::proportional(12.0),
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
                .size(11.0)
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
                .size(11.5)
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
        ui.label(egui::RichText::new(error).size(11.5).color(egui::Color32::RED));
        return;
    }
    if app.detail.loading {
        ui.add(egui::Spinner::new().size(16.0));
        return;
    }
    if app.detail.comments.is_empty() {
        ui.label(
            egui::RichText::new("No comments loaded")
                .size(11.5)
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

    egui::ScrollArea::vertical()
        .id_salt("comments_scroll")
        .max_height(320.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for comment in ordered {
                comment_card(ui, &comment);
            }
        });
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
                        .size(12.0)
                        .strong()
                        .color(accent()),
                );
                if comment.is_pinned {
                    ui.label(
                        egui::RichText::new("pinned")
                            .size(10.0)
                            .color(warn_color()),
                    );
                }
                ui.label(
                    egui::RichText::new(&comment.time_text)
                        .size(10.5)
                        .color(muted()),
                );
            });
            ui.add(egui::Label::new(egui::RichText::new(&comment.text).size(11.8)).wrap());
            ui.label(
                egui::RichText::new(format!("{} likes", comment.like_count))
                    .size(10.5)
                    .color(ok_color()),
            );
        });
    ui.add_space(4.0);
}