//! First-run language wizard (M22.4) and its settings re-entry (M22.5).
//!
//! Shown full-screen before the regular UI when no language has been
//! stored yet (`i18n::language_is_chosen()`), and on demand from the
//! settings dialog. The user picks one of the five supported languages,
//! sees a live preview in that language, and confirms — the choice is
//! persisted per edition via `app-settings.json`.

use eframe::egui::{self, Align, Layout, RichText, Ui, Vec2};

use crate::i18n::{self, Lang};
use crate::theme;

/// Returns `true` once the user confirmed a language.
pub fn show(ui: &mut Ui, selected: &mut Lang) -> bool {
    let mut confirmed = false;
    egui::CentralPanel::default()
        .frame(
            egui::Frame::NONE
                .fill(theme::BG_DEEPEST)
                .inner_margin(egui::Margin::same(48)),
        )
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(
                    RichText::new("NØVA")
                        .size(44.0)
                        .strong()
                        .color(theme::ACCENT),
                );
                ui.label(
                    RichText::new("FLASH STUDIO")
                        .size(15.0)
                        .color(theme::TEXT_FAINT),
                );
                ui.add_space(28.0);
                theme::hairline(ui);
                ui.add_space(20.0);

                ui.label(
                    RichText::new(i18n::t("wizard.title"))
                        .size(20.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.add_space(6.0);
                ui.label(RichText::new(i18n::t("wizard.subtitle")).color(theme::TEXT_DIM));
                ui.add_space(24.0);

                // Language selection — native names per spec.
                ui.horizontal_wrapped(|ui| {
                    for lang in Lang::ALL {
                        let is_sel = *selected == *lang;
                        let label = RichText::new(lang.native_name())
                            .size(15.0)
                            .color(if is_sel { theme::ACCENT } else { theme::TEXT });
                        if ui.selectable_label(is_sel, label).clicked() {
                            *selected = *lang;
                        }
                    }
                });
                ui.add_space(20.0);

                // Preview card in the selected language.
                egui::Frame::new()
                    .fill(theme::BG_PANEL)
                    .stroke(egui::Stroke::new(1.0, theme::BORDER))
                    .corner_radius(egui::CornerRadius::same(6))
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_max_width(520.0);
                        ui.label(
                            RichText::new(i18n::t_for(*selected, "menu.file"))
                                .strong()
                                .color(theme::ACCENT),
                        );
                        ui.add_space(4.0);
                        for key in [
                            "menu.file.new",
                            "menu.edit",
                            "menu.view",
                            "toolbar.export",
                            "dock.tab.timeline",
                            "settings.language",
                        ] {
                            ui.label(
                                RichText::new(format!("• {}", i18n::t_for(*selected, key)))
                                    .color(theme::TEXT_DIM),
                            );
                        }
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(i18n::t_for(*selected, "wizard.subtitle"))
                                .small()
                                .color(theme::TEXT_FAINT),
                        );
                    });
                ui.add_space(28.0);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new(i18n::t("wizard.confirm")).strong())
                                .min_size(Vec2::new(140.0, 32.0)),
                        )
                        .clicked()
                    {
                        confirmed = true;
                    }
                });
            });
        });
    confirmed
}
