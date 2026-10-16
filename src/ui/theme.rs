//! Design tokens.
//!
//! One type scale and one set of card metrics for the whole app, so a size or a
//! gap changes in a single place and every surface follows. Before this module
//! every size was an ad-hoc literal (10, 10.5, 11, 11.5, 12, 12.5, 13, 14, 18,
//! 20) scattered across the UI, and the card geometry was bare numbers.
//!
//! Note on "bold": egui has no bold face, and `RichText::strong()` only swaps in
//! a colour (`Visuals::strong_text_color`, which is white on dark and black on
//! light). Heavier text here is therefore a *larger* size plus that strongest
//! colour - see [`title_color`] - not a font weight.

use super::accent;
use eframe::egui;

// --- Type scale ------------------------------------------------------------

/// The sidebar wordmark.
pub const T_APP: f32 = 20.0;
/// A page, section or feed title in the toolbar.
pub const T_PAGE: f32 = 17.0;
/// Card titles and other emphasised headings.
pub const T_TITLE: f32 = 14.0;
/// Default body text, nav rows, buttons and labels.
pub const T_BODY: f32 = 12.5;
/// Secondary lines: channel names, counts, metadata.
pub const T_META: f32 = 11.5;
/// Small captions and tags.
pub const T_CAPTION: f32 = 10.5;

// --- Card metrics ----------------------------------------------------------

/// Corner radius of a card.
pub const CARD_RADIUS: f32 = 10.0;
/// Corner radius of a thumbnail inside a card.
pub const THUMB_RADIUS: f32 = 8.0;
/// Padding between a card's edge and its contents.
pub const CARD_PAD: f32 = 8.0;
/// Gap between cards, horizontally and vertically.
pub const CARD_GAP: f32 = 10.0;
/// Room under a grid thumbnail for the title and the meta line.
pub const CARD_TEXT_H: f32 = 52.0;
/// Baseline of a card title, measured down from the bottom of the thumbnail.
pub const CARD_TITLE_GAP: f32 = 17.0;
/// Corner radius of a badge (duration, LIVE).
pub const BADGE_RADIUS: f32 = 4.0;

// --- Control metrics -------------------------------------------------------

/// Corner radius of a text field.
///
/// Deliberately smaller than the widget radius the buttons use: a single-line
/// field is only about one line high, so the same radius that looks like a
/// rounded button on a taller control turns the field into a pill.
pub const INPUT_RADIUS: f32 = 4.0;

// --- Semantic colour -------------------------------------------------------

/// The "LIVE" badge.
pub fn live_color() -> egui::Color32 {
    egui::Color32::from_rgb(0xE5, 0x3A, 0x3A)
}

/// Emphasis colour for a card title: the strongest text colour the theme has.
pub fn title_color(visuals: &egui::Visuals) -> egui::Color32 {
    visuals.strong_text_color()
}

/// Fill of a selected card: the accent, tinted so text stays readable.
pub fn selected_fill(visuals: &egui::Visuals) -> egui::Color32 {
    accent().gamma_multiply(if visuals.dark_mode { 0.30 } else { 0.18 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_scale_is_ordered_from_caption_to_app() {
        // A regression guard: the scale must stay monotonic, so "smaller" and
        // "larger" always mean the same thing wherever a size is used.
        let scale = [T_CAPTION, T_META, T_BODY, T_TITLE, T_PAGE, T_APP];
        assert!(
            scale.windows(2).all(|pair| pair[0] < pair[1]),
            "the type scale is out of order: {scale:?}"
        );
    }

    #[test]
    fn a_selected_fill_stays_a_tint_of_the_accent() {
        // The point of the tint is a wash, not a solid block: a gamma below 1
        // keeps every channel dimmer than the accent itself.
        for dark in [true, false] {
            let visuals = if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            let fill = selected_fill(&visuals);
            let base = accent();
            assert!(fill.r() <= base.r() && fill.g() <= base.g() && fill.b() <= base.b());
            assert_ne!(fill, egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn the_card_text_zone_holds_a_title_over_a_meta_line() {
        // The grid card puts the title baseline `CARD_TITLE_GAP` below the
        // thumbnail and the meta baseline at the card's bottom padding; both a
        // 14pt title and an 11.5pt meta line have to fit inside `CARD_TEXT_H`
        // without colliding. The 0.78/0.22 ascent/descent split approximates
        // egui's default font.
        let ascent = |size: f32| size * 0.78;
        let descent = |size: f32| size * 0.22;
        let title_baseline = CARD_TITLE_GAP;
        let meta_baseline = CARD_TEXT_H - 2.0 * CARD_PAD;
        assert!(
            title_baseline - ascent(T_TITLE) >= 0.0,
            "the title would clip the thumbnail"
        );
        assert!(
            meta_baseline - ascent(T_META) > title_baseline + descent(T_TITLE),
            "the meta line would overlap the title"
        );
        assert!(
            meta_baseline + descent(T_META) <= CARD_TEXT_H - CARD_PAD,
            "the meta line would clip the card"
        );
    }
}
