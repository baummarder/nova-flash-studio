//! Enterprise mode UI (M21.9): administration workspace for the Enterprise
//! edition. All persistence goes through `nova-ent`; nothing here touches
//! the `Project` document or `.nova` format. Roles are workflow policy —
//! the UI states this explicitly; OS file permissions are the real boundary.

use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};
use nova_ent::admin;
use nova_ent::assets::{self, LibraryAsset};
use nova_ent::builds::{self, BuildProfile, BuildTarget};
use nova_ent::integrations::{self, Integration, IntegrationKind};
use nova_ent::profile::EnterpriseRole;
use nova_ent::review::{self, Review, ReviewStatus};
use nova_ent::team::{self, ProjectRegistration, TeamMember};
use nova_ent::versions;
use nova_ent::workspace::Workspace;

use crate::ent_state::{EntTab, EntUi};
use crate::state::UiAction;
use crate::theme;

/// Renders the full enterprise workspace. `current_project` is the path of
/// the currently open `.nova` (for "register current project").
pub fn show(
    ui: &mut Ui,
    ent: &mut EntUi,
    current_project: Option<&Path>,
    current_project_name: &str,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // ---------------------------------------------------------- header
    ui.horizontal(|ui| {
        ui.label(RichText::new("Enterprise").strong().size(18.0));
        ui.separator();
        ui.label(
            RichText::new(
                "Rollen und Berechtigungen sind lokale Workflow-Regeln, keine Anmeldung — \
                 Dateizugriff regelt das Betriebssystem.",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    });
    ui.add_space(6.0);

    if let Some(err) = &ent.error {
        ui.colored_label(theme::AMBER, format!("⚠ {}", err));
        if ui.small_button("Schließen").clicked() {
            ent.clear_error();
        }
        ui.separator();
    }
    if !ent.status.is_empty() {
        ui.label(RichText::new(&ent.status).small().color(theme::GREEN));
        ui.separator();
    }

    // ---------------------------------------------------------- workspace gate
    if ent.workspace.is_none() {
        workspace_gate(ui, ent);
        return actions;
    }

    // ---------------------------------------------------------- role + tabs
    ui.horizontal(|ui| {
        ui.label("Meine Rolle:");
        for role in EnterpriseRole::ALL {
            if ui
                .selectable_label(ent.profile.role == *role, role.label())
                .clicked()
            {
                ent.profile.role = *role;
                ent.save_local(&config_dir());
            }
        }
        ui.separator();
        if let Some(ws) = &ent.workspace {
            ui.label(
                RichText::new(ws.root.display().to_string())
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
        if ui.small_button("Workspace wechseln").clicked() {
            ent.workspace = None;
        }
    });
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        for tab in EntTab::ALL {
            if ui.selectable_label(ent.tab == *tab, tab.label()).clicked() {
                ent.tab = *tab;
            }
        }
    });
    ui.separator();

    let ws = match ent.ws() {
        Ok(w) => w.clone(),
        Err(e) => {
            ent.set_error(e);
            return actions;
        }
    };

    match ent.tab {
        EntTab::Overview => overview_tab(ui, ent, &ws),
        EntTab::Projects => projects_tab(
            ui,
            ent,
            &ws,
            current_project,
            current_project_name,
            &mut actions,
        ),
        EntTab::Team => team_tab(ui, ent, &ws),
        EntTab::Assets => assets_tab(ui, ent, &ws),
        EntTab::Versions => versions_tab(ui, ent, &ws),
        EntTab::Reviews => reviews_tab(ui, ent, &ws),
        EntTab::Builds => builds_tab(ui, ent, &ws, current_project),
        EntTab::Admin => admin_tab(ui, ent, &ws),
        EntTab::Integrations => integrations_tab(ui, ent, &ws),
    }

    actions
}

fn config_dir() -> PathBuf {
    nova_core::Edition::current()
        .config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
}

// ---------------------------------------------------------------- workspace

fn workspace_gate(ui: &mut Ui, ent: &mut EntUi) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Enterprise-Workspace").strong().size(22.0));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Wähle einen Enterprise-Workspace (Ordner mit org.json) oder erstelle einen neuen.\n\
                 Alles läuft offline — Projekte werden nur registriert, nie verschoben oder verändert.",
            )
            .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Workspace öffnen …").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                && let Err(e) = ent.open_workspace(dir)
            {
                ent.set_error(e);
            }
            if ui.button("Neuer Workspace …").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Organisation".into());
                if let Err(e) = ent.create_workspace(dir, &name) {
                    ent.set_error(e);
                }
            }
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Der Workspace enthält Organisation, Team, Projekt-Registry, Asset-Bibliothek, \
                 Versionen, Reviews, Build-Profile und Audit-Log — alles als lokale JSON-Dateien.",
            )
            .small()
            .color(theme::TEXT_FAINT),
        );
    });
}

// ----------------------------------------------------------------- overview

fn overview_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    let ov = match admin::overview(ws) {
        Ok(o) => o,
        Err(e) => {
            ent.set_error(e.to_string());
            return;
        }
    };
    ui.columns(4, |cols| {
        stat(&mut cols[0], "Organisation", &ov.org_name);
        stat(
            &mut cols[1],
            "Team",
            &format!("{} aktiv / {} gesamt", ov.active_members, ov.members),
        );
        stat(
            &mut cols[2],
            "Projekte",
            &format!("{} (+{} archiviert)", ov.projects, ov.archived_projects),
        );
        stat(
            &mut cols[3],
            "Assets",
            &format!("{} in {} Ordnern", ov.assets, ov.asset_folders),
        );
    });
    ui.add_space(6.0);
    ui.columns(4, |cols| {
        stat(
            &mut cols[0],
            "Offene Reviews",
            &format!("{} ({} freigegeben)", ov.reviews_open, ov.reviews_approved),
        );
        stat(
            &mut cols[1],
            "Builds",
            &format!("{} Profile, {} Läufe", ov.build_profiles, ov.builds_total),
        );
        stat(
            &mut cols[2],
            "Versionen",
            &format!(
                "{} Snapshots, {:.1} MB",
                ov.versions_total,
                ov.versions_bytes as f64 / 1_048_576.0
            ),
        );
        stat(
            &mut cols[3],
            "Integrationen",
            &format!("{} aktiv", ov.integrations_enabled),
        );
    });
    ui.add_space(8.0);
    if !ov.integrity_problems.is_empty() {
        ui.colored_label(theme::AMBER, "Integritätsprobleme in der Asset-Bibliothek:");
        for p in ov.integrity_problems.iter().take(10) {
            ui.label(RichText::new(format!("• {p}")).small().color(theme::AMBER));
        }
    } else {
        ui.label(
            RichText::new("Asset-Integrität: OK")
                .small()
                .color(theme::GREEN),
        );
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label("Organisationsname:");
        ui.text_edit_singleline(&mut ent.input_org_name);
        if ui.button("Speichern").clicked() {
            match ws.org() {
                Ok(mut org) => {
                    org.name = ent.input_org_name.clone();
                    match ws.save_org(&org) {
                        Ok(()) => {
                            let _ = admin::record(ws, &ent.actor(), "org", "org.rename", &org.name);
                            ent.status = "Organisation gespeichert.".into();
                        }
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
}

fn stat(ui: &mut Ui, label: &str, value: &str) {
    ui.group(|ui| {
        ui.label(RichText::new(label).small().color(theme::TEXT_FAINT));
        ui.label(RichText::new(value).strong());
    });
}

// ----------------------------------------------------------------- projects

fn projects_tab(
    ui: &mut Ui,
    ent: &mut EntUi,
    ws: &Workspace,
    current_project: Option<&Path>,
    current_project_name: &str,
    actions: &mut Vec<UiAction>,
) {
    ui.horizontal(|ui| {
        if let Some(path) = current_project
            && ui
                .button("Aktuelles Projekt registrieren")
                .on_hover_text(format!("{current_project_name} ({})", path.display()))
                .clicked()
        {
            match team::register_project(ws, path, current_project_name, None) {
                Ok(reg) => {
                    let _ =
                        admin::record(ws, &ent.actor(), "project", "project.register", &reg.path);
                    ent.selected_project = Some(reg.id);
                    ent.status = format!("Projekt registriert: {}", reg.name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
        if ui.button("… Datei wählen").clicked()
            && let Some(f) = rfd::FileDialog::new()
                .add_filter("NØVA-Projekt", &["nova"])
                .pick_file()
        {
            let name = f
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match team::register_project(ws, &f, &name, None) {
                Ok(reg) => {
                    let _ =
                        admin::record(ws, &ent.actor(), "project", "project.register", &reg.path);
                    ent.selected_project = Some(reg.id);
                    ent.status = format!("Projekt registriert: {}", reg.name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.add_space(6.0);

    let projects = team::list_projects(ws).unwrap_or_default();
    if projects.is_empty() {
        ui.label(
            RichText::new("Keine Projekte registriert. Öffne oder erstelle ein .nova-Projekt und registriere es hier.")
                .color(theme::TEXT_DIM),
        );
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for reg in &projects {
            project_row(ui, ent, ws, reg, actions);
        }
    });
}

fn project_row(
    ui: &mut Ui,
    ent: &mut EntUi,
    ws: &Workspace,
    reg: &ProjectRegistration,
    actions: &mut Vec<UiAction>,
) {
    let selected = ent.selected_project == Some(reg.id);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(selected, RichText::new(&reg.name).strong())
            .clicked()
        {
            ent.selected_project = Some(reg.id);
        }
        if reg.archived {
            ui.label(
                RichText::new("[archiviert]")
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
        // Integrity: registered vs current sha.
        match (reg.registered_sha256.as_ref(), team::current_sha256(reg)) {
            (Some(old), Some(cur)) if old != &cur => {
                ui.label(
                    RichText::new("geändert seit Registrierung")
                        .small()
                        .color(theme::AMBER),
                );
            }
            (Some(_), Some(_)) => {
                ui.label(
                    RichText::new("unverändert")
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            }
            _ => {
                ui.label(RichText::new("Datei fehlt?").small().color(theme::AMBER));
            }
        }
        if ui.small_button("Öffnen").clicked() {
            actions.push(UiAction::OpenProjectPath(reg.abs_path()));
        }
        if !reg.archived && ui.small_button("Archivieren").clicked() {
            match team::archive_project(ws, reg.id) {
                Ok(_) => ent.status = "Projekt archiviert.".into(),
                Err(e) => ent.set_error(e.to_string()),
            }
        }
        if ui.small_button("Snapshot").clicked() {
            match versions::create_snapshot(ws, reg, &ent.actor(), "Manueller Snapshot", false) {
                Ok(v) => {
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "version",
                        "version.snapshot",
                        format!("{}: {}", reg.name, v.message),
                    );
                    ent.status = format!("Snapshot erstellt: {}", v.id);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.label(
        RichText::new(reg.path.clone())
            .small()
            .color(theme::TEXT_FAINT),
    );
}

// -------------------------------------------------------------------- team

fn team_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    ui.horizontal(|ui| {
        ui.label("Neues Mitglied:");
        ui.text_edit_singleline(&mut ent.input_member_name);
        egui::ComboBox::from_id_salt("role")
            .selected_text(ent.input_member_role.label())
            .show_ui(ui, |ui| {
                for role in EnterpriseRole::ALL {
                    ui.selectable_value(&mut ent.input_member_role, *role, role.label());
                }
            });
        if ui.button("Hinzufügen").clicked() {
            match team::add_member(ws, &ent.input_member_name, ent.input_member_role) {
                Ok(m) => {
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "team",
                        "team.add",
                        format!("{} ({})", m.display_name, m.role.label()),
                    );
                    ent.input_member_name.clear();
                    ent.status = format!("Mitglied hinzugefügt: {}", m.display_name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.add_space(6.0);

    let members = team::list_members(ws).unwrap_or_default();
    if members.is_empty() {
        ui.label(RichText::new("Noch keine Teammitglieder.").color(theme::TEXT_DIM));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for m in &members {
            member_row(ui, ent, ws, m);
        }
    });

    // Permission matrix overview.
    ui.add_space(8.0);
    ui.collapsing("Berechtigungsübersicht (Rollen → Aktionen)", |ui| {
        ui.label(
            RichText::new(
                "Policy-Matrix für die lokale Workflow-Steuerung — keine Authentifizierung.",
            )
            .small()
            .color(theme::TEXT_FAINT),
        );
        for role in EnterpriseRole::ALL {
            let allowed: Vec<&str> = nova_ent::profile::default_permissions(*role)
                .iter()
                .map(|a| a.label())
                .collect();
            ui.horizontal(|ui| {
                ui.label(RichText::new(role.label()).strong());
                ui.label(allowed.join(", "));
            });
        }
    });
}

fn member_row(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace, m: &TeamMember) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(ent.selected_member == Some(m.id), &m.display_name)
            .clicked()
        {
            ent.selected_member = Some(m.id);
        }
        ui.label(RichText::new(m.role.label()).small().color(theme::TEXT_DIM));
        if !m.active {
            ui.label(RichText::new("[inaktiv]").small().color(theme::TEXT_FAINT));
        }
        egui::ComboBox::from_id_salt(format!("mrole-{}", m.id))
            .selected_text(m.role.label())
            .show_ui(ui, |ui| {
                for role in EnterpriseRole::ALL {
                    if ui.selectable_label(m.role == *role, role.label()).clicked() {
                        match team::update_member(ws, m.id, |mm| mm.role = *role) {
                            Ok(_) => {
                                let _ = admin::record(
                                    ws,
                                    &ent.actor(),
                                    "team",
                                    "team.role",
                                    format!("{} → {}", m.display_name, role.label()),
                                );
                                ent.status = format!("Rolle von {} geändert.", m.display_name);
                            }
                            Err(e) => ent.set_error(e.to_string()),
                        }
                    }
                }
            });
        if ui.small_button("Entfernen").clicked() {
            match team::remove_member(ws, m.id) {
                Ok(()) => {
                    let _ = admin::record(ws, &ent.actor(), "team", "team.remove", &m.display_name);
                    ent.status = format!("{} entfernt.", m.display_name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    if !m.project_roles.is_empty() {
        let overrides: Vec<String> = m
            .project_roles
            .iter()
            .map(|(pid, r)| format!("{pid}: {}", r.label()))
            .collect();
        ui.label(
            RichText::new(format!("Projek roles: {}", overrides.join(", ")))
                .small()
                .color(theme::TEXT_FAINT),
        );
    }
}

// ------------------------------------------------------------------ assets

fn assets_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    ui.horizontal(|ui| {
        ui.label("Suche:");
        ui.text_edit_singleline(&mut ent.input_asset_name);
        if ui.button("Importieren …").clicked()
            && let Some(f) = rfd::FileDialog::new().pick_file()
        {
            let folder = ent.input_asset_folder.trim().to_string();
            let tags: Vec<String> = ent
                .input_asset_tags
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            match assets::import_asset(
                ws,
                &f,
                &ent.input_asset_name,
                &folder,
                &tags,
                ent.selected_asset,
                None,
            ) {
                Ok(a) => {
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "asset",
                        "asset.import",
                        format!("{} ({} Versionen)", a.name, a.versions.len()),
                    );
                    ent.selected_asset = Some(a.id);
                    ent.status = format!("Asset importiert: {}", a.name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
        if ui.button("Integrität prüfen").clicked() {
            match assets::verify_library(ws) {
                Ok(problems) if problems.is_empty() => {
                    ent.status = "Asset-Integrität: alle Blobs OK.".into();
                }
                Ok(problems) => ent.set_error(format!(
                    "{} Integritätsprobleme: {}",
                    problems.len(),
                    problems.join("; ")
                )),
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Ordner:");
        ui.text_edit_singleline(&mut ent.input_asset_folder);
        ui.label("Tags (Komma):");
        ui.text_edit_singleline(&mut ent.input_asset_tags);
    });
    ui.add_space(6.0);

    let query = ent.input_asset_name.trim().to_string();
    let assets = if query.is_empty() {
        assets::list_assets(ws).unwrap_or_default()
    } else {
        assets::search_assets(ws, &query).unwrap_or_default()
    };
    if assets.is_empty() {
        ui.label(RichText::new("Keine Assets in der Bibliothek.").color(theme::TEXT_DIM));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for a in &assets {
            asset_row(ui, ent, ws, a);
        }
    });
}

fn asset_row(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace, a: &LibraryAsset) {
    ui.horizontal(|ui| {
        if ui
            .selectable_label(ent.selected_asset == Some(a.id), &a.name)
            .clicked()
        {
            ent.selected_asset = Some(a.id);
        }
        ui.label(RichText::new(a.kind.label()).small().color(theme::TEXT_DIM));
        if !a.folder.is_empty() {
            ui.label(RichText::new(&a.folder).small().color(theme::TEXT_FAINT));
        }
        ui.label(
            RichText::new(format!("v{}", a.versions.len()))
                .small()
                .color(theme::TEXT_FAINT),
        );
        if ui.small_button("Export …").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            match assets::export_asset(ws, a, &dir) {
                Ok(p) => ent.status = format!("Exportiert: {}", p.display()),
                Err(e) => ent.set_error(e.to_string()),
            }
        }
        if ui.small_button("Löschen").clicked() {
            match assets::delete_asset(ws, a.id) {
                Ok(()) => {
                    let _ = admin::record(ws, &ent.actor(), "asset", "asset.delete", &a.name);
                    ent.status = format!("Asset gelöscht: {}", a.name);
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    if !a.tags.is_empty() {
        ui.label(
            RichText::new(format!("Tags: {}", a.tags.join(", ")))
                .small()
                .color(theme::TEXT_FAINT),
        );
    }
}

// ---------------------------------------------------------------- versions

fn versions_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    let Some(pid) = ent.selected_project else {
        ui.label(
            RichText::new("Wähle zuerst ein Projekt im Tab „Projekte“.").color(theme::TEXT_DIM),
        );
        return;
    };
    let reg = match team::get_project(ws, pid) {
        Ok(r) => r,
        Err(e) => {
            ent.set_error(e.to_string());
            return;
        }
    };
    ui.label(RichText::new(format!("Versionen für: {}", reg.name)).strong());
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut ent.input_version_message);
        if ui.button("Snapshot erstellen").clicked() {
            match versions::create_snapshot(
                ws,
                &reg,
                &ent.actor(),
                &ent.input_version_message,
                false,
            ) {
                Ok(v) => {
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "version",
                        "version.snapshot",
                        format!("{}: {}", reg.name, v.message),
                    );
                    ent.input_version_message.clear();
                    ent.status = format!(
                        "Snapshot erstellt ({} Änderungen)",
                        v.changes.as_ref().map(|c| c.headline()).unwrap_or_default()
                    );
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.add_space(6.0);

    let versions = versions::list_versions(ws, pid).unwrap_or_default();
    if versions.is_empty() {
        ui.label(RichText::new("Noch keine Snapshots.").color(theme::TEXT_DIM));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for v in &versions {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(ent.selected_version == Some(v.id), &v.id.to_string()[..8])
                    .clicked()
                {
                    ent.selected_version = Some(v.id);
                }
                ui.label(
                    RichText::new(if v.safety { "[Sicherheit] " } else { "" })
                        .small()
                        .color(theme::TEXT_FAINT),
                );
                ui.label(RichText::new(&v.author).small().color(theme::TEXT_DIM));
                ui.label(RichText::new(&v.message).small());
                if let Some(c) = &v.changes {
                    ui.label(RichText::new(c.headline()).small().color(theme::TEXT_FAINT));
                }
                ui.label(
                    RichText::new(format!("{} KB", v.size_bytes / 1024))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
                if !v.safety && ui.small_button("Wiederherstellen").clicked() {
                    match versions::restore_version(ws, &reg, v.id, &ent.actor()) {
                        Ok((restored, safety)) => {
                            let _ = admin::record(
                                ws,
                                &ent.actor(),
                                "version",
                                "version.restore",
                                format!("{} → {}", reg.name, restored.id),
                            );
                            ent.status = format!(
                                "Version {} wiederhergestellt{}.",
                                &restored.id.to_string()[..8],
                                if safety.is_some() {
                                    " (mit Sicherheits-Snapshot)"
                                } else {
                                    ""
                                }
                            );
                        }
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
                if ui.small_button("Vergleich").clicked() {
                    match versions::compare_with_current(ws, &reg, v.id) {
                        Ok(diff) => {
                            ent.status = format!("Aktuell vs. Version: {}", diff.headline());
                            if !diff.scenes_added.is_empty() {
                                ent.status.push_str(&format!(
                                    " | +Szenen: {}",
                                    diff.scenes_added.join(", ")
                                ));
                            }
                            if !diff.assets_added.is_empty() {
                                ent.status.push_str(&format!(
                                    " | +Assets: {}",
                                    diff.assets_added.join(", ")
                                ));
                            }
                        }
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
                if ui.small_button("Prüfen").clicked() {
                    match versions::get_version(ws, pid, v.id) {
                        Ok(entry) => match versions::verify_version(ws, &entry) {
                            Ok(true) => ent.status = "Snapshot-Integrität: OK.".into(),
                            Ok(false) => ent.set_error("Snapshot-Integrität: sha256-Mismatch!"),
                            Err(e) => ent.set_error(e.to_string()),
                        },
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
            });
        }
    });
}

// ----------------------------------------------------------------- reviews

fn reviews_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    // Create form.
    ui.horizontal(|ui| {
        ui.label("Neues Review:");
        ui.text_edit_singleline(&mut ent.input_review_title);
        if let Some(pid) = ent.selected_project {
            if ui.button("Anlegen").clicked() {
                match review::create_review(
                    ws,
                    pid,
                    &ent.input_review_title,
                    &ent.input_review_desc,
                    None,
                    &ent.actor(),
                ) {
                    Ok(r) => {
                        let _ =
                            admin::record(ws, &ent.actor(), "review", "review.create", &r.title);
                        ent.selected_review = Some(r.id);
                        ent.input_review_title.clear();
                        ent.input_review_desc.clear();
                        ent.status = "Review angelegt.".into();
                    }
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
        } else {
            ui.label(
                RichText::new("(Projekt im Tab „Projekte“ wählen)")
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
        ui.text_edit_singleline(&mut ent.input_review_desc);
    });
    ui.add_space(6.0);

    let reviews = review::list_reviews(ws).unwrap_or_default();
    if reviews.is_empty() {
        ui.label(RichText::new("Keine Reviews.").color(theme::TEXT_DIM));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for r in &reviews {
            review_row(ui, ent, ws, r);
        }
    });
}

fn review_row(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace, r: &Review) {
    let open = ent.selected_review == Some(r.id);
    ui.horizontal(|ui| {
        if ui
            .selectable_label(open, RichText::new(&r.title).strong())
            .clicked()
        {
            ent.selected_review = Some(r.id);
        }
        ui.label(
            RichText::new(r.status.label())
                .small()
                .color(match r.status {
                    ReviewStatus::Approved => theme::GREEN,
                    ReviewStatus::Rejected | ReviewStatus::ChangesRequested => theme::AMBER,
                    _ => theme::TEXT_DIM,
                }),
        );
        ui.label(
            RichText::new(format!("{} Kommentare", r.comments.len()))
                .small()
                .color(theme::TEXT_FAINT),
        );
    });
    if !open {
        return;
    }
    ui.indent("review-detail", |ui| {
        // Status transitions.
        ui.horizontal_wrapped(|ui| {
            for (to, label) in [
                (ReviewStatus::PendingReview, "Einreichen"),
                (ReviewStatus::InReview, "In Prüfung"),
                (ReviewStatus::ChangesRequested, "Änderungen"),
                (ReviewStatus::Approved, "Freigeben"),
                (ReviewStatus::Rejected, "Ablehnen"),
            ] {
                if r.status.can_transition_to(to) && ui.small_button(label).clicked() {
                    let note =
                        if matches!(to, ReviewStatus::Rejected | ReviewStatus::ChangesRequested) {
                            ent.input_review_comment.clone()
                        } else {
                            String::new()
                        };
                    match review::transition(ws, r.id, to, &ent.actor(), &note) {
                        Ok(updated) => {
                            let _ = admin::record(
                                ws,
                                &ent.actor(),
                                "review",
                                "review.transition",
                                format!("{} → {}", r.status.label(), to.label()),
                            );
                            ent.status = format!("Review: {}", updated.status.label());
                        }
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
            }
        });
        // Comments.
        for c in &r.comments {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&c.author).small().color(theme::TEXT_DIM));
                ui.label(RichText::new(c.category.label()).small());
                if c.orphaned {
                    ui.label(RichText::new("[verwaist]").small().color(theme::AMBER));
                }
                if c.resolved {
                    ui.label(RichText::new("[erledigt]").small().color(theme::TEXT_FAINT));
                }
                ui.label(RichText::new(&c.body).small());
            });
        }
        // New comment.
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(format!("cat-{}", r.id))
                .selected_text(ent.input_review_category.label())
                .show_ui(ui, |ui| {
                    for cat in [
                        review::CommentCategory::Praise,
                        review::CommentCategory::Problem,
                        review::CommentCategory::Question,
                        review::CommentCategory::Requirement,
                    ] {
                        ui.selectable_value(&mut ent.input_review_category, cat, cat.label());
                    }
                });
            ui.text_edit_singleline(&mut ent.input_review_comment);
            if ui.button("Kommentieren").clicked() {
                match review::add_comment(
                    ws,
                    r.id,
                    &ent.actor(),
                    ent.input_review_category,
                    &ent.input_review_comment,
                    None,
                    false,
                ) {
                    Ok(_) => {
                        ent.input_review_comment.clear();
                        ent.status = "Kommentar hinzugefügt.".into();
                    }
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
        });
    });
}

// ------------------------------------------------------------------ builds

fn builds_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace, current_project: Option<&Path>) {
    // Profiles.
    ui.horizontal(|ui| {
        ui.label("Neues Profil:");
        ui.text_edit_singleline(&mut ent.input_profile_name);
        if ui.button("Anlegen").clicked() {
            let p = BuildProfile {
                name: ent.input_profile_name.clone(),
                targets: vec![BuildTarget::Swf],
                ..Default::default()
            };
            match builds::save_profile(ws, &p) {
                Ok(saved) => {
                    let _ = admin::record(ws, &ent.actor(), "build", "build.profile", &saved.name);
                    ent.selected_profile = Some(saved.id);
                    ent.input_profile_name.clear();
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
        if let Some(path) = current_project
            && let Some(pid) = ent.selected_project
            && ui.button("Build ausführen").clicked()
        {
            let reg = team::get_project(ws, pid).ok();
            match (reg, nova_core::Project::load(path)) {
                (Some(reg), Ok(project)) => {
                    let profile = ent
                        .selected_profile
                        .and_then(|id| builds::get_profile(ws, id).ok())
                        .unwrap_or_else(|| BuildProfile {
                            name: "Ad-hoc".into(),
                            targets: vec![BuildTarget::Swf],
                            ..Default::default()
                        });
                    match builds::run_build(ws, &profile, &project, reg.id, &reg.path, &ent.actor())
                    {
                        Ok(records) => {
                            for rec in &records {
                                let _ = admin::record(
                                    ws,
                                    &ent.actor(),
                                    "build",
                                    "build.run",
                                    format!(
                                        "{} → {} ({} Warnungen)",
                                        reg.name, rec.output, rec.warnings
                                    ),
                                );
                            }
                            ent.status = format!("{} Build(s) ausgeführt.", records.len());
                        }
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
                (_, Err(e)) => ent.set_error(format!("Projekt laden: {e}")),
                (None, _) => ent.set_error("Kein Projekt registriert."),
            }
        }
    });
    ui.add_space(6.0);

    let profiles = builds::list_profiles(ws).unwrap_or_default();
    if profiles.is_empty() {
        ui.label(RichText::new("Keine Build-Profile.").color(theme::TEXT_DIM));
    } else {
        for p in &profiles {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(ent.selected_profile == Some(p.id), &p.name)
                    .clicked()
                {
                    ent.selected_profile = Some(p.id);
                }
                ui.label(
                    RichText::new(
                        p.targets
                            .iter()
                            .map(|t| t.label())
                            .collect::<Vec<_>>()
                            .join("+"),
                    )
                    .small()
                    .color(theme::TEXT_DIM),
                );
                if ui.small_button("Löschen").clicked() {
                    match builds::delete_profile(ws, p.id) {
                        Ok(()) => ent.status = "Profil gelöscht.".into(),
                        Err(e) => ent.set_error(e.to_string()),
                    }
                }
            });
        }
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Build-Historie").strong());
    let history = builds::build_history(ws).unwrap_or_default();
    if history.is_empty() {
        ui.label(
            RichText::new("Noch keine Builds.")
                .small()
                .color(theme::TEXT_DIM),
        );
    } else {
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for rec in history.iter().take(50) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&rec.profile_name).small().strong());
                        ui.label(
                            RichText::new(rec.target.label())
                                .small()
                                .color(theme::TEXT_DIM),
                        );
                        ui.label(
                            RichText::new(format!("{} KB", rec.size_bytes / 1024))
                                .small()
                                .color(theme::TEXT_FAINT),
                        );
                        ui.label(
                            RichText::new(format!("{} Warnungen", rec.warnings))
                                .small()
                                .color(theme::TEXT_FAINT),
                        );
                        ui.label(RichText::new(&rec.actor).small().color(theme::TEXT_FAINT));
                    });
                    ui.label(RichText::new(&rec.output).small().color(theme::TEXT_FAINT));
                }
            });
    }
}

// ------------------------------------------------------------------- admin

fn admin_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    ui.horizontal(|ui| {
        ui.label("Administration");
        ui.separator();
        if ui.button("Audit-Log exportieren").clicked() {
            let log = admin::audit_log(ws).unwrap_or_default();
            let mut csv = String::from("zeit;akteur;kategorie;aktion;detail;entitaet\n");
            for e in &log {
                csv.push_str(&format!(
                    "{};{};{};{};{};{}\n",
                    e.at_unix,
                    csv_escape(&e.actor),
                    csv_escape(&e.category),
                    csv_escape(&e.action),
                    csv_escape(&e.detail),
                    e.entity.map(|u| u.to_string()).unwrap_or_default()
                ));
            }
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                let path = dir.join("nova-audit.csv");
                match std::fs::write(&path, csv) {
                    Ok(()) => ent.status = format!("Audit-Log exportiert: {}", path.display()),
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
        }
        // Workspace package export/import.
        if ui.button("Workspace exportieren (.novaent)").clicked() {
            let files = nova_ent::package::collect_workspace_files(ws).unwrap_or_default();
            let manifest = nova_ent::package::PackageManifest {
                created_by: nova_ent::package::PackageAuthor {
                    display_name: ent.actor(),
                    role: ent.profile.role.label().to_string(),
                },
                ..Default::default()
            };
            let fname = nova_ent::package::package_filename(&ent.actor());
            if let Some(dir) = rfd::FileDialog::new()
                .set_directory(&ent.input_package_dir)
                .pick_folder()
            {
                let path = dir.join(fname);
                match nova_ent::package::write_package(&path, &manifest, &files) {
                    Ok(()) => {
                        let _ = admin::record(
                            ws,
                            &ent.actor(),
                            "package",
                            "package.export",
                            path.display().to_string(),
                        );
                        ent.status = format!("Workspace exportiert: {}", path.display());
                    }
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
        }
        if ui.button("Workspace importieren (.novaent)").clicked()
            && let Some(f) = rfd::FileDialog::new()
                .add_filter("NØVA Enterprise Workspace", &["novaent"])
                .pick_file()
        {
            match nova_ent::package::read_package(&f) {
                Ok(pkg) => {
                    let report = nova_ent::package::import_package(ws, &pkg);
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "package",
                        "package.import",
                        report.summary(),
                    );
                    if report.errors.is_empty() {
                        ent.status = format!("Import: {}", report.summary());
                    } else {
                        ent.set_error(format!(
                            "Import: {} — {}",
                            report.summary(),
                            report.errors.join("; ")
                        ));
                    }
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.add_space(8.0);

    ui.label(RichText::new("Audit-Log (neueste zuerst)").strong());
    let log = admin::audit_log(ws).unwrap_or_default();
    if log.is_empty() {
        ui.label(
            RichText::new("Noch keine Einträge.")
                .small()
                .color(theme::TEXT_DIM),
        );
    } else {
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for e in log.iter().take(100) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&e.actor).small().color(theme::TEXT_DIM));
                        ui.label(
                            RichText::new(format!("{}:{}", e.category, e.action))
                                .small()
                                .strong(),
                        );
                        ui.label(RichText::new(&e.detail).small());
                    });
                }
            });
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(';') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

// ------------------------------------------------------------ integrations

fn integrations_tab(ui: &mut Ui, ent: &mut EntUi, ws: &Workspace) {
    ui.label(
        RichText::new(
            "Optionale lokale Integrationen — standardmäßig deaktiviert. \
             Endpunkte bleiben lokal; Projektdateien verlassen den Rechner nur auf explizite Aktion. \
             API-Keys werden als Umgebungsvariablen-Referenzen gespeichert, nie im Klartext.",
        )
        .small()
        .color(theme::TEXT_DIM),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        ui.label("Name:");
        ui.text_edit_singleline(&mut ent.input_integration_name);
        egui::ComboBox::from_id_salt("ikind")
            .selected_text(ent.input_integration_kind.label())
            .show_ui(ui, |ui| {
                for k in [
                    IntegrationKind::Ollama,
                    IntegrationKind::OpenAiCompatible,
                    IntegrationKind::TeamEndpoint,
                ] {
                    ui.selectable_value(&mut ent.input_integration_kind, k, k.label());
                }
            });
        if ui.button("Hinzufügen").clicked() {
            let i = Integration {
                name: ent.input_integration_name.clone(),
                kind: ent.input_integration_kind,
                endpoint: ent.input_integration_endpoint.clone(),
                model: ent.input_integration_model.clone(),
                api_key_env: if ent.input_integration_key_env.trim().is_empty() {
                    None
                } else {
                    Some(ent.input_integration_key_env.trim().to_string())
                },
                ..Default::default()
            };
            match integrations::save_integration(ws, &i) {
                Ok(saved) => {
                    let _ = admin::record(
                        ws,
                        &ent.actor(),
                        "integration",
                        "integration.add",
                        &saved.name,
                    );
                    ent.selected_integration = Some(saved.id);
                    ent.input_integration_name.clear();
                }
                Err(e) => ent.set_error(e.to_string()),
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("Endpoint:");
        ui.text_edit_singleline(&mut ent.input_integration_endpoint);
        ui.label("Modell:");
        ui.text_edit_singleline(&mut ent.input_integration_model);
        ui.label("Key-Env:");
        ui.text_edit_singleline(&mut ent.input_integration_key_env);
    });
    ui.add_space(6.0);

    let integrations = integrations::list_integrations(ws).unwrap_or_default();
    if integrations.is_empty() {
        ui.label(RichText::new("Keine Integrationen konfiguriert.").color(theme::TEXT_DIM));
        return;
    }
    for i in &integrations {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(ent.selected_integration == Some(i.id), &i.name)
                .clicked()
            {
                ent.selected_integration = Some(i.id);
            }
            ui.label(RichText::new(i.kind.label()).small().color(theme::TEXT_DIM));
            ui.label(
                RichText::new(if i.enabled { "aktiv" } else { "deaktiviert" })
                    .small()
                    .color(if i.enabled {
                        theme::GREEN
                    } else {
                        theme::TEXT_FAINT
                    }),
            );
            if ui
                .small_button(if i.enabled {
                    "Deaktivieren"
                } else {
                    "Aktivieren"
                })
                .clicked()
            {
                let mut updated = i.clone();
                updated.enabled = !i.enabled;
                match integrations::save_integration(ws, &updated) {
                    Ok(_) => {
                        let _ = admin::record(
                            ws,
                            &ent.actor(),
                            "integration",
                            if updated.enabled {
                                "integration.enable"
                            } else {
                                "integration.disable"
                            },
                            &i.name,
                        );
                    }
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
            if ui.small_button("Löschen").clicked() {
                match integrations::delete_integration(ws, i.id) {
                    Ok(()) => ent.status = format!("{} gelöscht.", i.name),
                    Err(e) => ent.set_error(e.to_string()),
                }
            }
        });
        ui.label(
            RichText::new(format!(
                "{} · {} · Key: {}",
                i.endpoint,
                i.model,
                i.api_key_env.clone().unwrap_or_else(|| "—".into())
            ))
            .small()
            .color(theme::TEXT_FAINT),
        );
    }
}
