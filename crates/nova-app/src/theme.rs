//! Dark "studio" theme for NØVA Flash Studio.

use egui::{Color32, Context, FontFamily, FontId, Stroke, TextStyle};

pub const BG_DEEPEST: Color32 = Color32::from_rgb(9, 11, 15);
pub const BG_PANEL: Color32 = Color32::from_rgb(20, 23, 30);
pub const BG_PANEL_ALT: Color32 = Color32::from_rgb(25, 29, 38);
pub const BG_INPUT: Color32 = Color32::from_rgb(14, 17, 23);
pub const BG_HOVER: Color32 = Color32::from_rgb(35, 41, 53);
pub const BORDER: Color32 = Color32::from_rgb(45, 52, 66);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(32, 37, 48);
pub const TEXT: Color32 = Color32::from_rgb(214, 220, 232);
pub const TEXT_DIM: Color32 = Color32::from_rgb(139, 147, 165);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(95, 103, 120);
pub const ACCENT: Color32 = Color32::from_rgb(61, 214, 196);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(31, 105, 99);
pub const AMBER: Color32 = Color32::from_rgb(245, 166, 35);
pub const RED: Color32 = Color32::from_rgb(240, 104, 104);
pub const GREEN: Color32 = Color32::from_rgb(120, 200, 140);
pub const BLUE: Color32 = Color32::from_rgb(112, 168, 255);
pub const PURPLE: Color32 = Color32::from_rgb(197, 146, 234);

/// Accent used for the playhead and selection handles.
pub const PLAYHEAD: Color32 = Color32::from_rgb(255, 92, 92);

pub fn apply(ctx: &Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_PANEL_ALT;
    visuals.extreme_bg_color = BG_INPUT;
    visuals.faint_bg_color = BG_PANEL;
    visuals.code_bg_color = BG_INPUT;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = AMBER;
    visuals.error_fg_color = RED;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = BG_PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.bg_fill = BG_PANEL_ALT;
    visuals.widgets.inactive.weak_bg_fill = BG_PANEL_ALT;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.bg_fill = BG_HOVER;
    visuals.widgets.hovered.weak_bg_fill = BG_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.active.bg_fill = ACCENT_DIM;
    visuals.widgets.active.weak_bg_fill = ACCENT_DIM;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.open.bg_fill = BG_PANEL_ALT;
    visuals.widgets.open.weak_bg_fill = BG_PANEL_ALT;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(4);
    }

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(7.0, 6.0);
    style.spacing.button_padding = egui::vec2(9.0, 4.0);
    style.spacing.slider_width = 140.0;
    style.spacing.interact_size.y = 24.0;
    style.spacing.scroll.bar_width = 9.0;

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(18.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
    ]
    .into();

    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_global_style(style);
}

/// Draws a hairline separator that matches the theme (more subtle than
/// egui's default separator).
pub fn hairline(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, BORDER_SOFT),
    );
}

/// A small section title, used by the side panels.
pub fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .small()
            .color(TEXT_FAINT)
            .strong(),
    );
    ui.add_space(2.0);
}
