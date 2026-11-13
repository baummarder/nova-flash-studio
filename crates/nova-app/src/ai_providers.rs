//! Provider management dialog (Einstellungen → KI-Anbieter).
//!
//! API keys are written through the secure credential store and are never
//! rendered back into the dialog or included in status/error text.

use eframe::egui::{self, Context, RichText, TextEdit};
use nova_ai::{ProviderConfig, ProviderKind, ProviderManager};

use crate::state::{AppState, Dialog};
use crate::theme;

#[derive(Default)]
pub struct ProviderDialog {
    pub selected: Option<String>,
    pub editing: Option<ProviderConfig>,
    pub new_provider: bool,
    pub key_input: String,
    pub models: Vec<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub remove_confirm: Option<String>,
}

impl ProviderDialog {
    pub fn show(&mut self, ctx: &Context, manager: &mut ProviderManager, state: &mut AppState) {
        let mut open = matches!(state.dialog, Dialog::AiProviders);
        if !open {
            return;
        }
        if self.selected.is_none() {
            self.selected = manager
                .settings
                .providers
                .first()
                .map(|provider| provider.id.clone());
        }
        if self.editing.is_none() && !self.new_provider {
            if let Some(id) = self.selected.as_deref() {
                self.editing = manager.provider(id).cloned();
            }
        }

        let mut close_requested = false;
        egui::Window::new("KI-Anbieter")
            .id(egui::Id::unique("ai_providers_dialog"))
            .collapsible(false)
            .resizable(true)
            .default_width(820.0)
            .default_height(620.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Sicherer Anmeldedatenspeicher: {}",
                        manager.credential_backend()
                    ))
                    .small()
                    .color(theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for kind in [
                        ProviderKind::OllamaLocal,
                        ProviderKind::OllamaCloud,
                        ProviderKind::Groq,
                        ProviderKind::OpenAiCompatible,
                    ] {
                        if ui.button(format!("+ {}", kind.label())).clicked() {
                            self.new_provider = true;
                            self.selected = None;
                            self.editing = Some(ProviderConfig::new(kind));
                            self.key_input.clear();
                            self.models.clear();
                            self.status = None;
                            self.error = None;
                        }
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui.button("Schließen").clicked() {
                                close_requested = true;
                            }
                        },
                    );
                });
                ui.separator();

                let providers: Vec<(String, String, bool, bool, bool)> = manager
                    .settings
                    .providers
                    .iter()
                    .map(|provider| {
                        (
                            provider.id.clone(),
                            provider.name.clone(),
                            provider.enabled,
                            provider.is_cloud(),
                            manager.settings.default_provider.as_deref()
                                == Some(provider.id.as_str()),
                        )
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("provider_list")
                    .max_height(112.0)
                    .show(ui, |ui| {
                        if providers.is_empty() {
                            ui.label(
                                RichText::new(
                                    "Noch kein Anbieter konfiguriert. Lege oben einen Anbieter an.",
                                )
                                .color(theme::TEXT_DIM),
                            );
                        }
                        for (id, name, enabled, cloud, is_default) in &providers {
                            ui.horizontal(|ui| {
                                if ui
                                    .selectable_label(
                                        self.selected.as_deref() == Some(id.as_str()),
                                        RichText::new(name).strong(),
                                    )
                                    .clicked()
                                {
                                    self.selected = Some(id.clone());
                                    self.editing = manager.provider(id).cloned();
                                    self.new_provider = false;
                                    self.key_input.clear();
                                    self.models.clear();
                                    self.status = None;
                                    self.error = None;
                                }
                                if !enabled {
                                    ui.label(
                                        RichText::new("deaktiviert").small().color(theme::AMBER),
                                    );
                                }
                                if *cloud {
                                    ui.label(
                                        RichText::new("CLOUD").small().color(theme::AMBER),
                                    );
                                } else {
                                    ui.label(
                                        RichText::new("LOKAL").small().color(theme::GREEN),
                                    );
                                }
                                if *is_default {
                                    ui.label(
                                        RichText::new("Standard").small().color(theme::ACCENT),
                                    );
                                }
                            });
                        }
                    });
                ui.separator();

                let Some(mut editing) = self.editing.clone() else {
                    ui.label(
                        RichText::new("Wähle einen Anbieter oder lege einen neuen an.")
                            .color(theme::TEXT_DIM),
                    );
                    return;
                };
                let mut persist_editing = true;

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Name").small().color(theme::TEXT_DIM));
                    ui.add(TextEdit::singleline(&mut editing.name).desired_width(220.0));
                    ui.label(RichText::new("Typ").small().color(theme::TEXT_DIM));
                    let old_kind = editing.kind;
                    egui::ComboBox::from_id_salt("provider_kind")
                        .selected_text(editing.kind.label())
                        .show_ui(ui, |ui| {
                            for kind in [
                                ProviderKind::OllamaLocal,
                                ProviderKind::OllamaCloud,
                                ProviderKind::Groq,
                                ProviderKind::OpenAiCompatible,
                            ] {
                                ui.selectable_value(&mut editing.kind, kind, kind.label());
                            }
                        });
                    if editing.kind != old_kind && self.new_provider {
                        editing.base_url = editing.kind.default_base_url().to_string();
                    }
                    let mut enabled = editing.enabled;
                    if ui.checkbox(&mut enabled, "Aktiv").changed() {
                        editing.enabled = enabled;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Basis-URL").small().color(theme::TEXT_DIM));
                    ui.add(
                        TextEdit::singleline(&mut editing.base_url)
                            .desired_width(420.0)
                            .hint_text("https://…"),
                    );
                });
                if editing.is_cloud() {
                    ui.label(
                        RichText::new(
                            "CLOUD: Projektkontext wird nur nach ausdrücklicher Zustimmung im AI Agent Editor gesendet.",
                        )
                        .small()
                        .color(theme::AMBER),
                    );
                } else {
                    ui.label(
                        RichText::new("LOKAL: Die Inferenz bleibt auf diesem Rechner.")
                            .small()
                            .color(theme::GREEN),
                    );
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Modell").small().color(theme::TEXT_DIM));
                    ui.add(
                        TextEdit::singleline(&mut editing.model)
                            .desired_width(300.0)
                            .hint_text("Modell-ID"),
                    );
                    let can_load = !self.new_provider
                        && self.selected.as_deref().is_some_and(|id| id == editing.id);
                    if ui
                        .add_enabled(can_load, egui::Button::new("Modelle laden"))
                        .clicked()
                    {
                        match manager.discover_models(&editing.id) {
                            Ok(models) => {
                                self.models = models;
                                self.status = Some(format!(
                                    "{} Modell(e) gefunden.",
                                    self.models.len()
                                ));
                                self.error = None;
                                if editing.model.trim().is_empty() {
                                    if let Some(model) = self.models.first() {
                                        editing.model = model.clone();
                                    }
                                }
                            }
                            Err(error) => {
                                self.error = Some(error.to_string());
                                self.status = None;
                            }
                        }
                    }
                    if !self.models.is_empty() {
                        egui::ComboBox::from_id_salt("provider_models")
                            .selected_text(if editing.model.is_empty() {
                                "Modell wählen"
                            } else {
                                editing.model.as_str()
                            })
                            .show_ui(ui, |ui| {
                                for model in &self.models {
                                    ui.selectable_value(
                                        &mut editing.model,
                                        model.clone(),
                                        model,
                                    );
                                }
                            });
                    }
                });

                ui.horizontal(|ui| {
                    ui.label(RichText::new("API-Schlüssel").small().color(theme::TEXT_DIM));
                    ui.add(
                        TextEdit::singleline(&mut self.key_input)
                            .password(true)
                            .desired_width(300.0)
                            .hint_text(if self.new_provider {
                                "wird sicher gespeichert"
                            } else {
                                "leer lassen = unverändert"
                            }),
                    );
                    let saved = self
                        .selected
                        .as_deref()
                        .and_then(|id| manager.has_credential(id).ok())
                        .unwrap_or(false);
                    if saved {
                        ui.label(RichText::new("gespeichert").small().color(theme::GREEN));
                        if ui.button("Schlüssel löschen").clicked() {
                            if let Some(id) = self.selected.as_deref() {
                                match manager.delete_credential(id) {
                                    Ok(()) => {
                                        self.status =
                                            Some("API-Schlüssel gelöscht.".into());
                                        self.error = None;
                                    }
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
                        }
                    }
                });
                ui.label(
                    RichText::new(
                        "Der Schlüssel wird nie im Anbieter-JSON, in Logs oder in Fehlermeldungen gespeichert.",
                    )
                    .small()
                    .color(theme::TEXT_FAINT),
                );

                let mut temperature = editing.temperature.unwrap_or(0.2);
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut editing.temperature.is_some(),
                        "Temperatur überschreiben",
                    );
                    if editing.temperature.is_some() {
                        if ui
                            .add(egui::DragValue::new(&mut temperature).range(0.0..=2.0))
                            .changed()
                        {
                            editing.temperature = Some(temperature);
                        }
                    }
                });

                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Speichern").clicked() {
                        let config = editing.clone();
                        let was_new = self.new_provider;
                        let result = if self.new_provider {
                            manager.add(config).map(|id| {
                                self.selected = Some(id);
                            })
                        } else {
                            manager.update(config)
                        };
                        match result {
                            Ok(()) => {
                                let mut key_error = None;
                                if let Some(id) = self.selected.clone() {
                                    if !self.key_input.trim().is_empty() {
                                        if let Err(error) =
                                            manager.set_credential(&id, &self.key_input)
                                        {
                                            key_error = Some(error.to_string());
                                        } else {
                                            self.key_input.clear();
                                        }
                                    }
                                    self.editing = manager.provider(&id).cloned();
                                    persist_editing = false;
                                    self.new_provider = false;
                                    if was_new {
                                        let _ = manager.set_default(&id);
                                    }
                                }
                                if let Err(error) = manager.save() {
                                    self.error = Some(error.to_string());
                                } else if let Some(error) = key_error {
                                    self.error = Some(error);
                                    self.status = Some("Anbieter gespeichert, Schlüssel nicht gespeichert.".into());
                                } else {
                                    self.status = Some("Anbieter gespeichert.".into());
                                    self.error = None;
                                }
                            }
                            Err(error) => self.error = Some(error.to_string()),
                        }
                    }
                    let can_test = self
                        .selected
                        .as_deref()
                        .is_some_and(|id| manager.provider(id).is_some());
                    if ui
                        .add_enabled(can_test, egui::Button::new("Verbindung testen"))
                        .clicked()
                    {
                        if let Some(id) = self.selected.clone() {
                            match manager.test_connection(&id) {
                                Ok(test) => {
                                    self.models = test.models;
                                    self.status = Some(test.message);
                                    self.error = None;
                                }
                                Err(error) => {
                                    self.error = Some(error.to_string());
                                    self.status = None;
                                }
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Als Standard"),
                        )
                        .clicked()
                    {
                        if let Some(id) = self.selected.clone() {
                            match manager.set_default(&id).and_then(|_| manager.save()) {
                                Ok(()) => {
                                    self.status = Some("Standardanbieter gesetzt.".into());
                                    self.error = None;
                                }
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                    }
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Entfernen"),
                        )
                        .clicked()
                    {
                        self.remove_confirm = self.selected.clone();
                    }
                });

                if let Some(id) = self.remove_confirm.clone() {
                    ui.separator();
                    ui.label(
                        RichText::new("Diesen Anbieter und seinen gespeicherten Schlüssel entfernen?")
                            .color(theme::AMBER),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Wirklich entfernen").clicked() {
                            match manager.remove(&id).and_then(|_| manager.save()) {
                                Ok(()) => {
                                    self.selected = None;
                                    self.editing = None;
                                    persist_editing = false;
                                    self.remove_confirm = None;
                                    self.status = Some("Anbieter entfernt.".into());
                                    self.error = None;
                                }
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                        if ui.button("Abbrechen").clicked() {
                            self.remove_confirm = None;
                        }
                    });
                }

                if persist_editing {
                    self.editing = Some(editing);
                }

                if let Some(error) = &self.error {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).color(theme::RED));
                } else if let Some(status) = &self.status {
                    ui.add_space(4.0);
                    ui.label(RichText::new(status).color(theme::GREEN));
                }
            });

        if close_requested {
            open = false;
        }
        if !open {
            state.dialog = Dialog::None;
        }
    }
}
