pub mod detail;
pub mod settings;
pub mod sidebar;
pub mod toasts;
pub mod toolbar;
pub mod video_list;

use eframe::egui;

pub fn accent() -> egui::Color32 {
    egui::Color32::from_rgb(0xE6, 0x2E, 0x5C)
}

pub fn ok_color() -> egui::Color32 {
    egui::Color32::from_rgb(0x3D, 0xD6, 0x8A)
}

pub fn warn_color() -> egui::Color32 {
    egui::Color32::from_rgb(0xF5, 0xC2, 0x42)
}

pub fn muted() -> egui::Color32 {
    egui::Color32::from_gray(140)
}

pub fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) -> egui::InnerResponse<()> {
    egui::Frame::new()
        .fill(ui.visuals().panel_fill)
        .corner_radius(10.0)
        .inner_margin(12.0)
        .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
        .show(ui, add)
}

/// Horizontal space kept free at the right of a scrollable list so the
/// scrollbar never covers the last characters of a row. egui draws the bar
/// over the content instead of reserving room for it.
pub const SCROLLBAR_GAP: f32 = 14.0;

pub fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    let font = font.clone();
    ui.fonts_mut(|f| f.layout_no_wrap(text.to_string(), font, egui::Color32::WHITE))
        .rect
        .width()
}

pub fn ellipsize(ui: &egui::Ui, text: &str, font: &egui::FontId, max_width: f32) -> String {
    if text_width(ui, text, font) <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0usize, chars.len());
    while low < high {
        let mid = (low + high).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect();
        if text_width(ui, &candidate, font) + text_width(ui, "…", font) <= max_width {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let mut out: String = chars[..low].iter().collect();
    out.push('…');
    out
}
