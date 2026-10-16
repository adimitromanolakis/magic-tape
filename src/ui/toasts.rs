use eframe::egui;

use crate::app::{App, ToastKind};
use crate::ui::{muted, ok_color, T_BODY, T_CAPTION, T_META};

pub fn render(app: &mut App, ctx: &egui::Context) {
    let area = egui::Area::new(egui::Id::new("toast_area"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 62.0))
        .order(egui::Order::Tooltip);
    area.show(ctx, |ui| {
        let mut index = 0;
        for toast in app.toasts.iter().rev() {
            let color = match toast.kind {
                ToastKind::Info => muted(),
                ToastKind::Success => ok_color(),
                ToastKind::Error => egui::Color32::from_rgb(0xE0, 0x50, 0x50),
            };
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(235))
                .stroke(egui::Stroke::new(1.0, color))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_max_width(360.0);
                    ui.label(egui::RichText::new(&toast.text).size(T_BODY).color(color));
                });
            index += 1;
            if index >= 4 {
                break;
            }
        }
    });
}

pub fn status_bar(app: &App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        if app.worker.busy() {
            ui.add(egui::Spinner::new().size(13.0));
        } else {
            ui.label(egui::RichText::new("idle").size(T_CAPTION).color(muted()));
        }
        ui.label(
            egui::RichText::new(app.status.clone())
                .size(T_META)
                .color(muted()),
        );
    });
}