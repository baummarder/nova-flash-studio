//! Classic left toolbox (M22 UI correction — 2009 desktop style).
//!
//! A narrow vertical strip of small icon buttons for the drawing/edit
//! tools, replacing the previous horizontal text strip inside the
//! stage. Rendered as its own left panel so it stays docked while the
//! stage scrolls/zooms — like the toolbox in classic Flash-style
//! authoring tools.

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui, vec2};

use crate::state::{AppState, Tool, UiAction};
use crate::theme::{self, ToolIcon};

const TOOLS: &[(Tool, ToolIcon)] = &[
    (Tool::Select, ToolIcon::Select),
    (Tool::Line, ToolIcon::Line),
    (Tool::Pencil, ToolIcon::Pencil),
    (Tool::Brush, ToolIcon::Brush),
    (Tool::Eraser, ToolIcon::Eraser),
    (Tool::Fill, ToolIcon::Fill),
    (Tool::PathEdit, ToolIcon::PathEdit),
    (Tool::Pen, ToolIcon::Pen),
    (Tool::SkinPaint, ToolIcon::SkinPaint),
];

pub fn show(ui: &mut Ui, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();
    ui.vertical(|ui| {
        ui.add_space(4.0);
        for (tool, icon) in TOOLS {
            let selected = state.tool == *tool;
            let (rect, response) = ui.allocate_exact_size(vec2(40.0, 30.0), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    format!("{} — {}", tool.label(), tool.hint()),
                )
            });
            let clicked = response.clicked();
            let hovered = response.hovered();
            let pressed = response.is_pointer_button_down_on();
            let _ = response.on_hover_text(format!("{}\n{}", tool.label(), tool.hint()));
            if clicked {
                actions.push(UiAction::SetTool(*tool));
            }

            // Classic raised/sunken bevel button.
            let painter = ui.painter();
            let raised = selected || hovered || pressed;
            painter.rect_filled(
                rect,
                egui::CornerRadius::same(2),
                if selected {
                    theme::BG_HOVER
                } else {
                    theme::BG_PANEL_ALT
                },
            );
            theme::bevel_frame(painter, rect, raised, selected);
            let color = if selected {
                theme::ACCENT
            } else {
                theme::TEXT_DIM
            };
            theme::tool_icon(painter, rect.center(), *icon, color);
        }
        ui.add_space(4.0);
        theme::hairline(ui);
        ui.add_space(4.0);
        // Active tool hint at the bottom, classic status style.
        ui.with_layout(Layout::top_down(Align::Min).with_main_wrap(true), |ui| {
            ui.set_max_width(40.0);
            ui.label(
                RichText::new(state.tool.label())
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        });
    });
    actions
}
