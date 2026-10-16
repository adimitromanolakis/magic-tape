pub mod detail;
pub mod folder_picker;
pub mod icons;
pub mod settings;
pub mod sidebar;
pub mod theme;
pub mod toasts;
pub mod toolbar;
pub mod video_list;

// The design tokens are re-exported flat so call sites read `crate::ui::T_TITLE`
// rather than `crate::ui::theme::T_TITLE`.
pub use theme::*;

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

/// Id of the main search box, so the `Ctrl+F` shortcut can focus the very
/// widget the toolbar draws. Both sides must agree on this.
pub const SEARCH_INPUT_ID: &str = "main_search";

/// Puts the caret in the main search box.
///
/// Called once per frame while `App::focus_search` is set: `request_focus` is
/// a request for the *next* frame, so asking once and hoping is not enough.
pub fn focus_search_input(ui: &egui::Ui) {
    ui.ctx()
        .memory_mut(|memory| memory.request_focus(egui::Id::new(SEARCH_INPUT_ID)));
}

/// Draws a text field with [`theme::INPUT_RADIUS`] instead of the wider radius
/// the buttons use.
///
/// egui paints a `TextEdit`'s fill, border and focus ring with the corner
/// radius of `visuals.widgets.*`, and a `Style` has no per-widget-type radius,
/// so the radius is overridden for the extent of `add`. Handing `TextEdit` a
/// `Frame` would be simpler, but a custom frame replaces the frame egui paints
/// - the fill colour and the focus ring go with it.
pub fn input<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope(|ui| {
        let radius = egui::CornerRadius::same(INPUT_RADIUS as u8);
        let widgets = &mut ui.style_mut().visuals.widgets;
        for visuals in [
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            visuals.corner_radius = radius;
        }
        add(ui)
    })
    .inner
}

/// True when a text field's contents should be taken as submitted: the button
/// beside it was clicked, or Enter was pressed with the field focused.
/// Enter is checked against the field's own response rather than globally,
/// because the app-wide key handler maps Enter to "play the selection"; that
/// shortcut is dropped while a field has focus, so nothing else would pick the
/// key up.
pub fn submitted(ui: &egui::Ui, field: &egui::Response, clicked: bool) -> bool {
    clicked
        || (ui.input(|i| i.key_pressed(egui::Key::Enter))
            && (field.has_focus() || field.lost_focus()))
}

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
