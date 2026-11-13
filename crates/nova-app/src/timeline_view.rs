//! Timeline dock: layer lanes, keyframes, tweens, labels and the playhead.
//!
//! The timeline is painted directly instead of being built from hundreds of
//! widgets. Per-cell responses are still registered so clicking a frame, opening
//! its context menu and hovering labels work as expected.

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use nova_core::Project;
use uuid::Uuid;

use crate::state::{AppState, KeyframeRef, Selection, UiAction};
use crate::theme;

const NAME_WIDTH: f32 = 154.0;
const ROW_HEIGHT: f32 = 22.0;
const HEADER_HEIGHT: f32 = 20.0;

struct FrameCell {
    keyframe: bool,
    tween: bool,
    label: Option<String>,
    objects: usize,
}

struct LayerRow {
    #[allow(dead_code)]
    index: usize,
    id: Uuid,
    name: String,
    visible: bool,
    locked: bool,
    kind: String,
    cells: Vec<FrameCell>,
}

pub fn show(ui: &mut Ui, project: &mut Project, state: &mut AppState) -> Vec<UiAction> {
    let mut actions = Vec::new();

    let (frame_count, fps) = project
        .active_scene()
        .map(|scene| (scene.frame_count().max(1), scene.fps))
        .unwrap_or((1, 24.0));

    toolbar(ui, state, frame_count, fps, &mut actions);
    theme::hairline(ui);

    let rows: Vec<LayerRow> = project
        .active_scene()
        .map(|scene| {
            scene
                .layers
                .iter()
                .enumerate()
                .rev()
                .map(|(index, layer)| LayerRow {
                    index,
                    id: layer.id,
                    name: layer.name.clone(),
                    visible: layer.visible,
                    locked: layer.locked,
                    kind: layer.kind.label().to_string(),
                    cells: layer
                        .frames
                        .iter()
                        .map(|frame| FrameCell {
                            keyframe: frame.keyframe,
                            tween: frame.tween.is_some(),
                            label: frame.label.clone(),
                            objects: frame.objects.len(),
                        })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();

    let content_height = HEADER_HEIGHT + rows.len() as f32 * ROW_HEIGHT;
    let frame_width = state.frame_width.clamp(8.0, 40.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        // Fixed layer-name column ---------------------------------------
        ui.allocate_ui_with_layout(
            Vec2::new(NAME_WIDTH, content_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let (header, _) =
                    ui.allocate_exact_size(Vec2::new(NAME_WIDTH, HEADER_HEIGHT), Sense::hover());
                ui.painter().rect_filled(header, 0.0, theme::BG_PANEL_ALT);
                ui.painter().text(
                    header.left_center() + Vec2::new(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    "EBENE",
                    FontId::proportional(10.0),
                    theme::TEXT_FAINT,
                );

                for (row_index, row) in rows.iter().enumerate() {
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(NAME_WIDTH, ROW_HEIGHT), Sense::click());
                    let selected = state.selection == Selection::Layer(row.id);
                    let fill = if selected {
                        theme::BG_HOVER
                    } else if row_index % 2 == 0 {
                        theme::BG_PANEL
                    } else {
                        theme::BG_PANEL_ALT
                    };
                    ui.painter().rect_filled(rect, 0.0, fill);

                    let stripe = Rect::from_min_size(rect.left_top(), Vec2::new(3.0, ROW_HEIGHT));
                    ui.painter()
                        .rect_filled(stripe, 0.0, layer_color(row.index));

                    let text_color = if row.visible {
                        theme::TEXT
                    } else {
                        theme::TEXT_FAINT
                    };
                    ui.painter().text(
                        rect.left_center() + Vec2::new(10.0, 0.0),
                        Align2::LEFT_CENTER,
                        &row.name,
                        FontId::proportional(11.5),
                        text_color,
                    );

                    let mut badges = Vec::new();
                    if row.locked {
                        badges.push("gesperrt");
                    }
                    if !row.visible {
                        badges.push("verborgen");
                    }
                    if row.kind != "Normal" {
                        badges.push(row.kind.as_str());
                    }
                    if !badges.is_empty() {
                        ui.painter().text(
                            rect.right_center() - Vec2::new(7.0, 0.0),
                            Align2::RIGHT_CENTER,
                            badges.join(" · "),
                            FontId::proportional(9.0),
                            theme::TEXT_FAINT,
                        );
                    }

                    if response.clicked() {
                        state.selection = Selection::Layer(row.id);
                        state.set_status(format!("Ebene '{}' ausgewählt", row.name));
                    }
                    response.on_hover_text(format!(
                        "{} · {} Frames{}",
                        row.name,
                        row.cells.len(),
                        if row.locked { " · gesperrt" } else { "" }
                    ));
                }
            },
        );

        // Scrolling frame area -----------------------------------------
        egui::ScrollArea::horizontal()
            .id_salt("timeline_scroll")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let total_width = (frame_count as f32 * frame_width).max(ui.available_width());
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(total_width, content_height), Sense::hover());
                paint_grid(
                    ui,
                    rect,
                    &rows,
                    state,
                    frame_count,
                    frame_width,
                    &mut actions,
                );
            });
    });

    actions
}

fn toolbar(
    ui: &mut Ui,
    state: &mut AppState,
    frame_count: usize,
    fps: f32,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        if ui
            .button("Erster")
            .on_hover_text("Zum ersten Frame (Home)")
            .clicked()
        {
            actions.push(UiAction::FirstFrame);
        }
        if ui
            .button("<")
            .on_hover_text("Ein Frame zurück (Pfeil links)")
            .clicked()
        {
            actions.push(UiAction::Step(-1));
        }
        let play_label = if state.playing { "Pause" } else { "Abspielen" };
        if ui
            .button(play_label)
            .on_hover_text("Wiedergabe starten oder anhalten (Leertaste)")
            .clicked()
        {
            actions.push(UiAction::TogglePlay);
        }
        if ui
            .button("Stop")
            .on_hover_text("Wiedergabe stoppen und zum Anfang springen")
            .clicked()
        {
            actions.push(UiAction::Stop);
        }
        if ui
            .button(">")
            .on_hover_text("Ein Frame vor (Pfeil rechts)")
            .clicked()
        {
            actions.push(UiAction::Step(1));
        }
        if ui
            .button("Letzter")
            .on_hover_text("Zum letzten Frame (End)")
            .clicked()
        {
            actions.push(UiAction::LastFrame);
        }

        ui.separator();

        ui.label(egui::RichText::new("Frame").small().color(theme::TEXT_DIM));
        let mut frame = (state.frame + 1) as u32;
        let response = ui.add(
            egui::DragValue::new(&mut frame)
                .range(1..=frame_count.max(1) as u32)
                .speed(0.25),
        );
        if response.changed() {
            state.frame = (frame.saturating_sub(1) as usize).min(frame_count - 1);
            state.playing = false;
        }
        ui.label(
            egui::RichText::new(format!("/ {frame_count}"))
                .small()
                .color(theme::TEXT_FAINT),
        );
        ui.label(
            egui::RichText::new(format!("{fps:.0} fps"))
                .small()
                .color(theme::TEXT_FAINT),
        );

        ui.separator();
        ui.checkbox(&mut state.onion_skin, "Onion");
        ui.checkbox(&mut state.loop_playback, "Loop");

        let mut zoom = state.frame_width;
        if ui
            .add(
                egui::DragValue::new(&mut zoom)
                    .range(8.0..=40.0)
                    .speed(0.4)
                    .suffix(" px"),
            )
            .on_hover_text("Breite eines Frames in der Timeline")
            .changed()
        {
            state.frame_width = zoom;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let target = matches!(state.selection, Selection::Layer(_) | Selection::Objects(_));
            if ui
                .add_enabled(target, egui::Button::new("KB löschen"))
                .on_hover_text("Schlüsselbild am aktuellen Frame entfernen")
                .clicked()
            {
                actions.push(UiAction::RemoveKeyframe);
            }
            if ui
                .add_enabled(target, egui::Button::new("Leeres KB"))
                .on_hover_text("Leeres Schlüsselbild einfügen (Shift+F6)")
                .clicked()
            {
                actions.push(UiAction::InsertBlankKeyframe);
            }
            if ui
                .add_enabled(target, egui::Button::new("KB einfügen"))
                .on_hover_text("Schlüsselbild mit dem Inhalt des vorherigen einfügen (F6)")
                .clicked()
            {
                actions.push(UiAction::InsertKeyframe);
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn paint_grid(
    ui: &mut Ui,
    rect: Rect,
    rows: &[LayerRow],
    state: &mut AppState,
    frame_count: usize,
    frame_width: f32,
    actions: &mut Vec<UiAction>,
) {
    let painter = ui.painter_at(rect);
    let width = (frame_count as f32 * frame_width).max(1.0);

    // Ruler ------------------------------------------------------------
    let ruler = Rect::from_min_size(rect.min, Vec2::new(width.max(rect.width()), HEADER_HEIGHT));
    painter.rect_filled(ruler, 0.0, theme::BG_PANEL_ALT);
    painter.line_segment(
        [ruler.left_bottom(), ruler.right_bottom()],
        Stroke::new(1.0, theme::BORDER_SOFT),
    );

    // Clicking or dragging the ruler scrubs the playhead.
    let ruler_id = ui.make_persistent_id("timeline_ruler");
    let ruler_response = ui.interact(ruler, ruler_id, Sense::click_and_drag());
    let shift = ui.input(|input| input.modifiers.shift);
    if ruler_response.drag_started() && shift {
        if let Some(pointer) = ruler_response.interact_pointer_pos() {
            if let Some(frame) = frame_at_x(pointer.x, rect.left(), frame_width, frame_count) {
                state.frame_range = Some((frame, frame));
            }
        }
    }
    if ruler_response.clicked() || ruler_response.dragged() {
        if let Some(pointer) = ruler_response.interact_pointer_pos() {
            if let Some(frame) = frame_at_x(pointer.x, rect.left(), frame_width, frame_count) {
                state.frame = frame;
                state.playing = false;
                if shift || (ruler_response.dragged() && state.frame_range.is_some()) {
                    let start = state.frame_range.map(|(start, _)| start).unwrap_or(frame);
                    state.frame_range = Some((start.min(frame), start.max(frame)));
                }
                state.set_status(format!("Frame {}", frame + 1));
            }
        }
    }

    let label_step = if frame_width >= 16.0 { 5 } else { 10 };
    for frame in (0..frame_count).step_by(label_step) {
        let x = rect.left() + frame as f32 * frame_width;
        painter.text(
            Pos2::new(x + 2.0, ruler.center().y),
            Align2::LEFT_CENTER,
            (frame + 1).to_string(),
            FontId::proportional(9.0),
            theme::TEXT_FAINT,
        );
    }

    // Rows -------------------------------------------------------------
    for (row_index, row) in rows.iter().enumerate() {
        let top = rect.top() + HEADER_HEIGHT + row_index as f32 * ROW_HEIGHT;
        let row_rect =
            Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, ROW_HEIGHT));

        let row_fill = if row_index % 2 == 0 {
            Color32::from_rgb(23, 27, 35)
        } else {
            Color32::from_rgb(20, 24, 31)
        };
        painter.rect_filled(row_rect, 0.0, row_fill);

        if let Some((a, b)) = state.frame_range {
            let start = a.min(b);
            let end = a.max(b);
            let range = Rect::from_min_max(
                Pos2::new(rect.left() + start as f32 * frame_width, top),
                Pos2::new(
                    rect.left() + (end + 1) as f32 * frame_width,
                    top + ROW_HEIGHT,
                ),
            );
            painter.rect_filled(
                range,
                0.0,
                Color32::from_rgba_unmultiplied(245, 166, 35, 28),
            );
        }

        let selected = state.selection == Selection::Layer(row.id);
        let mut last_keyframe: Option<usize> = None;

        for frame in 0..frame_count {
            let cell_rect = Rect::from_min_size(
                Pos2::new(rect.left() + frame as f32 * frame_width, top),
                Vec2::new(frame_width, ROW_HEIGHT),
            );
            let data = row.cells.get(frame);
            if data.is_some_and(|cell| cell.keyframe) {
                last_keyframe = Some(frame);
            }

            let fill = match (data, last_keyframe) {
                (Some(cell), _) if cell.keyframe && cell.objects > 0 => {
                    Color32::from_rgb(43, 55, 73)
                }
                (Some(cell), _) if cell.keyframe => Color32::from_rgb(35, 40, 52),
                (_, Some(_)) => Color32::from_rgb(27, 33, 44),
                _ => row_fill,
            };
            painter.rect_filled(cell_rect, 0.0, fill);

            // Tween span: draw a line from this keyframe to the next one.
            if let Some(cell) = data {
                if cell.keyframe && cell.tween {
                    if let Some(next) = row
                        .cells
                        .iter()
                        .enumerate()
                        .skip(frame + 1)
                        .find(|(_, candidate)| candidate.keyframe)
                        .map(|(index, _)| index)
                    {
                        let start = cell_rect.center();
                        let end = Pos2::new(
                            rect.left() + next as f32 * frame_width + frame_width * 0.5,
                            start.y,
                        );
                        painter.line_segment([start, end], Stroke::new(2.0, theme::ACCENT_DIM));
                        painter.circle_filled(end, 2.5, theme::ACCENT);
                    }
                }
            }

            // Keyframe marker.
            if let Some(cell) = data {
                if cell.keyframe {
                    let center = cell_rect.center();
                    if cell.objects > 0 {
                        painter.circle_filled(center, 3.2, theme::ACCENT);
                    } else {
                        painter.circle_stroke(center, 3.0, Stroke::new(1.4, theme::TEXT_FAINT));
                    }
                } else if last_keyframe.is_some() {
                    painter.line_segment(
                        [
                            Pos2::new(cell_rect.left() + 2.0, cell_rect.center().y),
                            Pos2::new(cell_rect.right() - 2.0, cell_rect.center().y),
                        ],
                        Stroke::new(1.0, Color32::from_rgb(58, 69, 90)),
                    );
                }

                if cell.label.is_some() {
                    let flag = vec![
                        cell_rect.left_top() + Vec2::new(1.0, 1.0),
                        cell_rect.left_top() + Vec2::new(1.0, 7.0),
                        cell_rect.left_top() + Vec2::new(7.0, 1.0),
                    ];
                    painter.add(Shape::convex_polygon(flag, theme::AMBER, Stroke::NONE));
                }
            }

            if state.selected_keyframes.contains(&KeyframeRef {
                layer: row.id,
                frame,
            }) {
                painter.rect_stroke(
                    cell_rect.shrink(1.0),
                    1.0,
                    Stroke::new(1.6, theme::ACCENT),
                    StrokeKind::Inside,
                );
            }

            // Interaction.
            let id = ui.make_persistent_id(("timeline_cell", row.id, frame));
            let mut response = ui.interact(cell_rect, id, Sense::click());
            if let Some(label) = data.and_then(|cell| cell.label.as_ref()) {
                response = response.on_hover_text(format!("Frame {} · Label '{label}'", frame + 1));
            } else if data.is_some_and(|cell| cell.keyframe) {
                response = response.on_hover_text(format!(
                    "Frame {} · Schlüsselbild · {} Objekt(e)",
                    frame + 1,
                    data.map(|cell| cell.objects).unwrap_or(0)
                ));
            }

            if response.clicked() {
                let shift = ui.input(|input| input.modifiers.shift);
                let reference = KeyframeRef {
                    layer: row.id,
                    frame,
                };
                if shift && data.is_some_and(|cell| cell.keyframe) {
                    if let Some(index) = state
                        .selected_keyframes
                        .iter()
                        .position(|candidate| *candidate == reference)
                    {
                        state.selected_keyframes.remove(index);
                    } else {
                        state.selected_keyframes.push(reference);
                    }
                } else {
                    state.selected_keyframes.clear();
                    if data.is_some_and(|cell| cell.keyframe) {
                        state.selected_keyframes.push(reference);
                    }
                    state.frame = frame;
                    state.selection = Selection::Layer(row.id);
                }
                state.playing = false;
                state.set_status(format!("Frame {} · {}", frame + 1, row.name));
            }
            if response.double_clicked() {
                state.frame = frame;
                state.playing = false;
                state.selection = Selection::Layer(row.id);
                actions.push(UiAction::InsertKeyframe);
            }
            response.context_menu(|ui| {
                let previous_frame = state.frame;
                state.frame = frame;
                state.playing = false;
                state.selection = Selection::Layer(row.id);
                state.selected_keyframes.clear();
                if data.is_some_and(|cell| cell.keyframe) {
                    state.selected_keyframes.push(KeyframeRef {
                        layer: row.id,
                        frame,
                    });
                }
                ui.label(
                    egui::RichText::new(format!("Frame {} · {}", frame + 1, row.name))
                        .small()
                        .strong(),
                );
                ui.separator();
                if ui.button("Schlüsselbild einfügen (F6)").clicked() {
                    actions.push(UiAction::InsertKeyframe);
                    ui.close();
                }
                if ui.button("Leeres Schlüsselbild (Shift+F6)").clicked() {
                    actions.push(UiAction::InsertBlankKeyframe);
                    ui.close();
                }
                if ui.button("Schlüsselbild entfernen").clicked() {
                    actions.push(UiAction::RemoveKeyframe);
                    ui.close();
                }
                ui.separator();
                if ui.button("Motion-Tween erstellen").clicked() {
                    actions.push(UiAction::CreateTween);
                    ui.close();
                }
                if ui.button("Motion-Tween entfernen").clicked() {
                    actions.push(UiAction::RemoveTween);
                    ui.close();
                }
                ui.separator();
                if ui.button("Schlüsselbilder kopieren").clicked() {
                    actions.push(UiAction::CopyKeyframes);
                    ui.close();
                }
                if ui.button("Schlüsselbilder einfügen").clicked() {
                    actions.push(UiAction::PasteKeyframes);
                    ui.close();
                }
                if ui.button("Schlüsselbilder duplizieren").clicked() {
                    actions.push(UiAction::DuplicateKeyframes);
                    ui.close();
                }
                ui.separator();
                if ui.button("Bereich bis hier markieren").clicked() {
                    state.frame_range =
                        Some((previous_frame.min(frame), previous_frame.max(frame)));
                    state.set_status("Frame-Bereich markiert");
                    ui.close();
                }
                if ui.button("Bereich aufheben").clicked() {
                    state.frame_range = None;
                    ui.close();
                }
                ui.separator();
                if ui.button("Frame einfügen (F5)").clicked() {
                    actions.push(UiAction::InsertFrame);
                    ui.close();
                }
                if ui.button("Frame entfernen (Shift+F5)").clicked() {
                    actions.push(UiAction::RemoveFrame);
                    ui.close();
                }
                ui.separator();
                if ui.button("Schlüsselbilder in Ebene darüber").clicked() {
                    actions.push(UiAction::MoveKeyframesToLayer(-1));
                    ui.close();
                }
                if ui.button("Schlüsselbilder in Ebene darunter").clicked() {
                    actions.push(UiAction::MoveKeyframesToLayer(1));
                    ui.close();
                }
                if ui.button("Zu diesem Frame springen").clicked() {
                    actions.push(UiAction::JumpToFrame(frame));
                    ui.close();
                }
            });
        }

        // Cell separators every five frames.
        let mut frame = 0;
        while frame <= frame_count {
            let x = rect.left() + frame as f32 * frame_width;
            let heavy = frame % 5 == 0;
            painter.line_segment(
                [Pos2::new(x, top), Pos2::new(x, top + ROW_HEIGHT)],
                Stroke::new(
                    1.0,
                    if heavy {
                        Color32::from_rgb(44, 52, 68)
                    } else {
                        Color32::from_rgb(30, 36, 46)
                    },
                ),
            );
            frame += 1;
        }

        if selected {
            painter.rect_stroke(
                Rect::from_min_size(Pos2::new(rect.left(), top), Vec2::new(width, ROW_HEIGHT)),
                0.0,
                Stroke::new(1.0, theme::ACCENT_DIM),
                StrokeKind::Inside,
            );
        }
    }

    // Playhead ---------------------------------------------------------
    let playhead_x = rect.left() + state.frame as f32 * frame_width + frame_width * 0.5;
    painter.line_segment(
        [
            Pos2::new(playhead_x, rect.top()),
            Pos2::new(playhead_x, rect.bottom()),
        ],
        Stroke::new(1.5, theme::PLAYHEAD),
    );
    let triangle = vec![
        Pos2::new(playhead_x - 4.5, rect.top()),
        Pos2::new(playhead_x + 4.5, rect.top()),
        Pos2::new(playhead_x, rect.top() + 7.0),
    ];
    painter.add(Shape::convex_polygon(
        triangle,
        theme::PLAYHEAD,
        Stroke::NONE,
    ));

    // Empty-space hint.
    if rows.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Keine Ebenen",
            FontId::proportional(12.0),
            theme::TEXT_FAINT,
        );
    }
}

/// Maps a horizontal screen coordinate inside the frame grid to a frame
/// index. Coordinates before the grid clamp to the first frame, coordinates
/// past the end clamp to the last frame.
pub(crate) fn frame_at_x(
    x: f32,
    grid_left: f32,
    frame_width: f32,
    frame_count: usize,
) -> Option<usize> {
    if frame_count == 0 || frame_width <= 0.0 || !frame_width.is_finite() {
        return None;
    }
    let index = ((x - grid_left) / frame_width).floor();
    if !index.is_finite() {
        return None;
    }
    Some((index.max(0.0) as usize).min(frame_count - 1))
}

fn layer_color(index: usize) -> Color32 {
    const COLORS: [Color32; 5] = [
        theme::ACCENT,
        theme::AMBER,
        theme::BLUE,
        theme::PURPLE,
        theme::GREEN,
    ];
    COLORS[index % COLORS.len()]
}

#[cfg(test)]
mod tests {
    use super::frame_at_x;

    #[test]
    fn maps_pixel_positions_to_frames() {
        assert_eq!(frame_at_x(100.0, 100.0, 10.0, 48), Some(0));
        assert_eq!(frame_at_x(109.9, 100.0, 10.0, 48), Some(0));
        assert_eq!(frame_at_x(110.0, 100.0, 10.0, 48), Some(1));
        assert_eq!(frame_at_x(145.0, 100.0, 10.0, 48), Some(4));
        assert_eq!(frame_at_x(579.0, 100.0, 10.0, 48), Some(47));
    }

    #[test]
    fn clamps_outside_the_grid() {
        assert_eq!(frame_at_x(0.0, 100.0, 10.0, 48), Some(0));
        assert_eq!(frame_at_x(10_000.0, 100.0, 10.0, 48), Some(47));
    }

    #[test]
    fn rejects_degenerate_input() {
        assert_eq!(frame_at_x(100.0, 0.0, 10.0, 0), None);
        assert_eq!(frame_at_x(100.0, 0.0, 0.0, 48), None);
        assert_eq!(frame_at_x(f32::NAN, 0.0, 10.0, 48), None);
    }
}
