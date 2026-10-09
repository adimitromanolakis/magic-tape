use eframe::egui;

use crate::app::{App, ToastKind};
use crate::ui::{muted, ok_color};

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
                    ui.label(egui::RichText::new(&toast.text).size(12.0).color(color));
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
            ui.label(egui::RichText::new("idle").size(11.0).color(muted()));
        }
        ui.label(
            egui::RichText::new(app.status.clone())
                .size(11.5)
                .color(muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("uptime {:.0}s", app.started.elapsed().as_secs()))
                    .size(11.0)
                    .color(muted()),
            );
        });
    });
}