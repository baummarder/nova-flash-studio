//! Classic desktop theme for NØVA Flash Studio — "2009 studio" look.
//!
//! Charcoal/graphite surfaces, subtle bevels, thin separators and
//! compact controls — inspired by professional desktop creative
//! software of the late 2000s. All existing constant names are kept
//! so every view inherits the new palette without call-site changes.

use egui::{Color32, Context, FontFamily, FontId, Pos2, Rect, Stroke, TextStyle};

/// Pasteboard / deepest background (outside the stage).
pub const BG_DEEPEST: Color32 = Color32::from_rgb(24, 25, 27);
/// Standard panel surface.
pub const BG_PANEL: Color32 = Color32::from_rgb(52, 54, 57);
/// Slightly lighter panel / header strip.
pub const BG_PANEL_ALT: Color32 = Color32::from_rgb(61, 63, 67);
/// Inputs, wells, recessed areas.
pub const BG_INPUT: Color32 = Color32::from_rgb(38, 40, 43);
/// Hover state.
pub const BG_HOVER: Color32 = Color32::from_rgb(76, 79, 84);
/// Strong border.
pub const BORDER: Color32 = Color32::from_rgb(88, 91, 97);
/// Soft separator / inner border.
pub const BORDER_SOFT: Color32 = Color32::from_rgb(70, 73, 78);
/// Bevel highlight (top/left of raised controls).
pub const EDGE_HI: Color32 = Color32::from_rgb(104, 108, 115);
/// Bevel shadow (bottom/right of raised controls, top of sunken).
pub const EDGE_LO: Color32 = Color32::from_rgb(30, 32, 34);

pub const TEXT: Color32 = Color32::from_rgb(216, 216, 216);
pub const TEXT_DIM: Color32 = Color32::from_rgb(168, 171, 176);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(126, 130, 136);
/// Classic selection/highlight blue (era-appropriate, not neon).
pub const ACCENT: Color32 = Color32::from_rgb(76, 155, 224);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(43, 92, 143);
pub const AMBER: Color32 = Color32::from_rgb(222, 160, 60);
pub const RED: Color32 = Color32::from_rgb(224, 96, 96);
pub const GREEN: Color32 = Color32::from_rgb(112, 186, 124);
pub const BLUE: Color32 = Color32::from_rgb(104, 152, 214);
pub const PURPLE: Color32 = Color32::from_rgb(178, 142, 216);

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
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
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
        widget.corner_radius = egui::CornerRadius::same(2);
    }

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);
    style.spacing.slider_width = 130.0;
    style.spacing.interact_size.y = 22.0;
    style.spacing.scroll.bar_width = 9.0;
    style.spacing.window_margin = egui::Margin::same(6);

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(16.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(12.5, FontFamily::Proportional)),
        (
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        ),
        (
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
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

/// Draws a hairline separator that matches the theme.
pub fn hairline(ui: &mut egui::Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 1.0), egui::Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(1.0, BORDER_SOFT),
    );
}

/// Draw a classic two-tone bevel around `rect`.
/// `raised = true` → light top/left, dark bottom/right (buttons,
/// panels); `raised = false` → inverted (sunken wells, active press).
pub fn bevel(painter: &egui::Painter, rect: Rect, raised: bool) {
    let (hi, lo) = if raised {
        (EDGE_HI, EDGE_LO)
    } else {
        (EDGE_LO, EDGE_HI)
    };
    let r = rect;
    painter.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, hi));
    painter.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, hi));
    painter.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, lo));
    painter.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, lo));
}

/// Bevel + border for a control rect (used by toolbox buttons etc.).
pub fn bevel_frame(painter: &egui::Painter, rect: Rect, raised: bool, accent: bool) {
    bevel(painter, rect, raised);
    if accent {
        painter.rect_stroke(
            rect.shrink(1.0),
            egui::CornerRadius::same(2),
            Stroke::new(1.0, ACCENT),
            egui::StrokeKind::Inside,
        );
    }
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

/// Paint a simple 16x16 tool icon centered in `rect`.
/// Icons are drawn with primitives — no external assets.
pub fn tool_icon(painter: &egui::Painter, center: Pos2, kind: ToolIcon, color: Color32) {
    let s = 8.0_f32; // half-size
    let stroke = Stroke::new(1.4, color);
    match kind {
        ToolIcon::Select => {
            // Arrow cursor.
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-s * 0.55, -s),
                    center + egui::vec2(s * 0.15, -s * 0.15),
                    center + egui::vec2(-s * 0.05, s * 0.05),
                    center + egui::vec2(s * 0.55, s),
                    center + egui::vec2(s * 0.3, s * 0.85),
                    center + egui::vec2(s * 0.05, s * 0.6),
                    center + egui::vec2(-s * 0.2, s * 0.85),
                ],
                color,
                Stroke::NONE,
            ));
        }
        ToolIcon::Line => {
            painter.line_segment(
                [center + egui::vec2(-s, s), center + egui::vec2(s, -s)],
                stroke,
            );
        }
        ToolIcon::Pencil => {
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.6, s * 0.8),
                    center + egui::vec2(s * 0.5, -s * 0.6),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(s * 0.15, -s * 0.95),
                    center + egui::vec2(s * 0.55, -s * 0.55),
                ],
                stroke,
            );
        }
        ToolIcon::Brush => {
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.7, s * 0.7),
                    center + egui::vec2(s * 0.55, -s * 0.55),
                ],
                Stroke::new(3.0, color),
            );
            painter.line_segment(
                [
                    center + egui::vec2(s * 0.2, -s * 0.95),
                    center + egui::vec2(s * 0.55, -s * 0.6),
                ],
                stroke,
            );
        }
        ToolIcon::Eraser => {
            painter.rect_filled(
                Rect::from_center_size(center, egui::vec2(s * 1.4, s * 0.9)),
                egui::CornerRadius::same(2),
                color,
            );
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.7, -s * 0.45),
                    center + egui::vec2(s * 0.7, -s * 0.45),
                ],
                Stroke::new(1.0, BG_INPUT),
            );
        }
        ToolIcon::Fill => {
            // Paint bucket silhouette.
            painter.add(egui::Shape::convex_polygon(
                vec![
                    center + egui::vec2(-s * 0.6, -s * 0.2),
                    center + egui::vec2(s * 0.1, -s * 0.2),
                    center + egui::vec2(s * 0.6, s * 0.8),
                    center + egui::vec2(-s * 0.6, s * 0.8),
                ],
                color,
                Stroke::NONE,
            ));
            painter.line_segment(
                [
                    center + egui::vec2(s * 0.1, -s * 0.2),
                    center + egui::vec2(s * 0.6, s * 0.1),
                ],
                Stroke::new(1.0, BG_INPUT),
            );
        }
        ToolIcon::PathEdit => {
            painter.line_segment(
                [
                    center + egui::vec2(-s, s * 0.4),
                    center + egui::vec2(0.0, -s),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(0.0, -s),
                    center + egui::vec2(s, s * 0.4),
                ],
                stroke,
            );
            for p in [
                center + egui::vec2(-s, s * 0.4),
                center + egui::vec2(0.0, -s),
                center + egui::vec2(s, s * 0.4),
            ] {
                painter.circle_filled(p, 1.8, color);
            }
        }
        ToolIcon::Pen => {
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.7, -s * 0.7),
                    center + egui::vec2(s * 0.7, s * 0.7),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.7, s * 0.7),
                    center + egui::vec2(s * 0.7, -s * 0.7),
                ],
                stroke,
            );
        }
        ToolIcon::SkinPaint => {
            painter.line_segment(
                [
                    center + egui::vec2(-s * 0.7, 0.0),
                    center + egui::vec2(s * 0.7, 0.0),
                ],
                stroke,
            );
            painter.circle_stroke(center + egui::vec2(-s * 0.7, 0.0), s * 0.4, stroke);
            painter.circle_stroke(center + egui::vec2(s * 0.7, 0.0), s * 0.4, stroke);
        }
    }
}

/// Icon kinds for the toolbox (kept UI-agnostic).
#[derive(Debug, Clone, Copy)]
pub enum ToolIcon {
    Select,
    Line,
    Pencil,
    Brush,
    Eraser,
    Fill,
    PathEdit,
    Pen,
    SkinPaint,
}
