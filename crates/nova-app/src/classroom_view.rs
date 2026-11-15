//! Classroom mode UI (M20.7): Edu-only workspace for teachers and students.
//!
//! All persistence goes through `nova-edu`; nothing here touches the
//! `Project` document or `.nova` format. Roles are workflow convenience —
//! not authentication — and the UI says so.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};
use nova_edu::assignment::{self, Assignment, StarterRef};
use nova_edu::classroom;
use nova_edu::feedback::{self, AuthorRef, CommentCategory};
use nova_edu::package::{self, PackageAuthor, PackageFile, PackageKind, PackageManifest};
use nova_edu::profile::EduRole;
use nova_edu::progress::{self};
use nova_edu::submission::{self, Submission, SubmissionState};
use nova_edu::template;
use nova_edu::workspace::Workspace;
use uuid::Uuid;

use crate::edu_state::{EduTab, EduUi};
use crate::state::UiAction;
use crate::theme;

/// Renders the full classroom workspace. Returns UI actions for the app
/// (currently: opening starter `.nova` projects).
pub fn show(
    ui: &mut Ui,
    edu: &mut EduUi,
    project_json: Option<&[u8]>,
    project_name: &str,
) -> Vec<UiAction> {
    let mut actions = Vec::new();

    // ---------------------------------------------------------- header
    ui.horizontal(|ui| {
        ui.label(RichText::new("Klassenzimmer").strong().size(18.0));
        ui.separator();
        ui.label(
            RichText::new(
                "Rollen sind lokale Arbeitsweisen, keine Anmeldung — jede Person mit Dateizugriff kann Daten lesen und ändern.",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    });
    ui.add_space(6.0);

    if edu.error.is_some() {
        ui.colored_label(
            theme::AMBER,
            format!("⚠ {}", edu.error.clone().unwrap_or_default()),
        );
        if ui.small_button("Fehler schließen").clicked() {
            edu.clear_error();
        }
        ui.separator();
    }
    if !edu.status.is_empty() {
        ui.label(RichText::new(&edu.status).small().color(theme::GREEN));
        ui.separator();
    }

    // ---------------------------------------------------------- workspace gate
    if edu.workspace.is_none() {
        workspace_gate(ui, edu);
        return actions;
    }

    // ---------------------------------------------------------- role + tabs
    ui.horizontal(|ui| {
        ui.label("Rolle:");
        for (role, label) in [
            (EduRole::Teacher, "Lehrkraft"),
            (EduRole::Student, "Lernende/r"),
            (EduRole::Assistant, "Assistenz"),
        ] {
            if ui.selectable_label(edu.role() == role, label).clicked() {
                edu.profile.role = role;
                edu.save_local(&branding_config_dir());
            }
        }
        ui.separator();
        if let Some(ws) = &edu.workspace {
            ui.label(
                RichText::new(ws.root.display().to_string())
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
        if ui.small_button("Workspace wechseln").clicked() {
            edu.workspace = None;
        }
        if ui.small_button("Beispieldaten").clicked() {
            match edu.create_sample() {
                Ok(()) => edu.status = "Beispiel-Daten erstellt.".into(),
                Err(e) => edu.set_error(e),
            }
        }
    });
    ui.add_space(4.0);

    ui.horizontal_wrapped(|ui| {
        for tab in EduTab::ALL {
            // Students don't see class-admin-heavy tabs the same way; the
            // tab itself renders role-appropriate content.
            if ui.selectable_label(edu.tab == *tab, tab.label()).clicked() {
                edu.tab = *tab;
            }
        }
    });
    ui.separator();

    let Some(ws) = edu.workspace.clone() else {
        return actions;
    };

    match edu.tab {
        EduTab::Overview => overview_tab(ui, edu, &ws),
        EduTab::Classes => classes_tab(ui, edu, &ws),
        EduTab::Assignments => {
            assignments_tab(ui, edu, &ws, &mut actions, project_json, project_name)
        }
        EduTab::Submissions => submissions_tab(ui, edu, &ws),
        EduTab::Feedback => feedback_tab(ui, edu, &ws),
        EduTab::Progress => progress_tab(ui, edu, &ws),
        EduTab::Templates => templates_tab(ui, edu, &ws),
    }

    actions
}

fn branding_config_dir() -> PathBuf {
    nova_core::Edition::current()
        .config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
}

// ---------------------------------------------------------------- workspace

fn workspace_gate(ui: &mut Ui, edu: &mut EduUi) {
    ui.add_space(20.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("Willkommen im Klassenzimmer").strong().size(22.0));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Wähle einen Edu-Workspace (Ordner mit school.json) oder erstelle einen neuen.\n\
                 Workspaces können auf USB-Sticks, Netzlaufwerken oder lokal liegen — alles läuft offline.",
            )
            .color(theme::TEXT_DIM),
        );
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if ui.button("Workspace öffnen …").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    && let Err(e) = edu.open_workspace(dir) {
                        edu.set_error(e);
                    }
            if ui.button("Neuer Workspace …").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
                    && let Err(e) = edu.create_workspace(dir, "") {
                        edu.set_error(e);
                    }
        });
        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Tipp: „Neuer Workspace“ legt school.json und die Datenordner an; danach kannst du Beispieldaten erzeugen.",
            )
            .small()
            .color(theme::TEXT_FAINT),
        );
    });
}

// ----------------------------------------------------------------- overview

fn overview_tab(ui: &mut Ui, edu: &EduUi, ws: &Workspace) {
    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    let subs = if ws.assignments_dir().exists() {
        {
            asgs.iter()
                .map(|a| {
                    submission::list_submissions(ws, a.id)
                        .map(|v| v.len())
                        .unwrap_or(0)
                })
                .sum::<usize>()
        }
    } else {
        0
    };

    ui.columns(4, |cols| {
        stat(
            &mut cols[0],
            "Klassen",
            classes.iter().filter(|c| !c.archived).count(),
        );
        stat(
            &mut cols[1],
            "Mitglieder",
            classes.iter().map(|c| c.members.len()).sum(),
        );
        stat(
            &mut cols[2],
            "Aufgaben",
            asgs.iter().filter(|a| !a.archived).count(),
        );
        stat(&mut cols[3], "Abgaben", subs);
    });
    ui.add_space(8.0);

    if edu.is_teacher() {
        ui.label(RichText::new("Nächste Schritte (Lehrkraft)").strong());
        ui.label("• Klasse anlegen und Mitglieder eintragen — Tab „Klassen“");
        ui.label("• Aufgabe mit Starterprojekt erstellen und verteilen — Tab „Aufgaben“");
        ui.label("• Abgaben sichten und Feedback geben — Tabs „Abgaben“/„Feedback“");
    } else {
        ui.label(RichText::new("Nächste Schritte (Lernende/r)").strong());
        ui.label("• Deine Klasse prüfen — Tab „Klassen“");
        ui.label("• Zugewiesene Aufgaben öffnen und Starter laden — Tab „Aufgaben“");
        ui.label("• Projekt als Abgabe exportieren — Tab „Aufgaben“ → „Abgabe exportieren“");
        ui.label("• Feedback der Lehrkraft lesen — Tab „Feedback“");
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new("Schulkonfiguration (school.json):")
            .small()
            .color(theme::TEXT_DIM),
    );
    ui.label(
        RichText::new(format!(
            "Schule: {} · Cloud-KI: {} · SWF-Export: {}",
            if edu.school.school.is_empty() {
                "—"
            } else {
                &edu.school.school
            },
            if edu.school.policy.allow_cloud_ai {
                "an"
            } else {
                "aus"
            },
            if edu.school.policy.allow_swf_export {
                "an"
            } else {
                "aus"
            },
        ))
        .small(),
    );
}

fn stat(ui: &mut Ui, label: &str, value: usize) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(90.0);
        ui.label(RichText::new(value.to_string()).strong().size(24.0));
        ui.label(RichText::new(label).small().color(theme::TEXT_DIM));
    });
}

// ------------------------------------------------------------------ classes

fn classes_tab(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    if edu.is_teacher() {
        teacher_classes(ui, edu, ws);
    } else {
        student_classes(ui, edu, ws);
    }
}

fn teacher_classes(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    ui.horizontal(|ui| {
        ui.label("Neue Klasse:");
        ui.text_edit_singleline(&mut edu.input_class_name);
        if ui.button("Anlegen").clicked() && !edu.input_class_name.trim().is_empty() {
            match classroom::create_classroom(ws, &edu.input_class_name) {
                Ok(c) => {
                    edu.selected_class = Some(c.id);
                    edu.input_class_name.clear();
                    edu.status = format!("Klasse '{}' angelegt.", c.name);
                }
                Err(e) => edu.set_error(e.to_string()),
            }
        }
        if ui.button("Archivierte anzeigen").clicked() {
            edu.status = "Archivierte Klassen bleiben in der Liste markiert.".into();
        }
    });
    ui.add_space(6.0);

    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    if classes.is_empty() {
        ui.label(
            RichText::new("Noch keine Klassen. Lege oben eine an oder erzeuge Beispieldaten.")
                .color(theme::TEXT_DIM),
        );
        return;
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        for class in &classes {
            let selected = edu.selected_class == Some(class.id);
            egui::CollapsingHeader::new(format!(
                "{}  ({} Mitglieder, {} Gruppen){}",
                class.name,
                class.members.len(),
                class.groups.len(),
                if class.archived { "  [archiviert]" } else { "" }
            ))
            .default_open(selected)
            .show(ui, |ui| {
                if selected || edu.selected_class.is_none() {
                    edu.selected_class = Some(class.id);
                }
                class_detail(ui, edu, ws, class.id);
            });
        }
    });
}

fn class_detail(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, class_id: Uuid) {
    let Ok(class) = classroom::get_classroom(ws, class_id) else {
        return;
    };

    ui.horizontal(|ui| {
        ui.label(RichText::new("Aktionen:").strong());
        if ui.small_button("Umbenennen").clicked() {
            match classroom::rename_classroom(
                ws,
                class_id,
                &class.name,
                &class.subject,
                &class.term,
            ) {
                Ok(_) => edu.status = "Gespeichert.".into(),
                Err(e) => edu.set_error(e.to_string()),
            }
        }
        if !class.archived && ui.small_button("Archivieren").clicked() {
            let _ = classroom::archive_classroom(ws, class_id);
            edu.status = "Klasse archiviert.".into();
        }
        if ui.small_button("Klasse löschen").clicked() {
            if let Err(e) = classroom::delete_classroom(ws, class_id) {
                edu.set_error(e.to_string());
            } else {
                edu.selected_class = None;
                edu.status =
                    "Klasse gelöscht (Abgaben/Feedback bleiben als Sidecars erhalten).".into();
            }
        }
        if ui.small_button(".novaclassroom exportieren").clicked() {
            export_classroom_package(ui, edu, ws, &class);
        }
        if ui.small_button(".novaclassroom importieren").clicked() {
            import_package(ui, edu, ws, PackageKind::Classroom);
        }
        if ui.small_button("Teilnehmerliste exportieren").clicked() {
            export_roster(ui, edu, ws, &class);
        }
        if ui.small_button("Teilnehmerliste importieren").clicked() {
            import_roster(ui, edu, ws, class_id);
        }
    });
    ui.add_space(4.0);

    // ---- groups -----------------------------------------------------
    ui.label(RichText::new("Gruppen").strong());
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut edu.input_group_name);
        if ui.button("Gruppe anlegen").clicked() && !edu.input_group_name.trim().is_empty() {
            match classroom::add_group(ws, class_id, &edu.input_group_name) {
                Ok(_) => {
                    edu.input_group_name.clear();
                    edu.status = "Gruppe angelegt.".into();
                }
                Err(e) => edu.set_error(e.to_string()),
            }
        }
    });
    for group in &class.groups {
        ui.horizontal(|ui| {
            ui.label(format!("• {}", group.name));
            if ui.small_button("Löschen").clicked() {
                let _ = classroom::remove_group(ws, class_id, group.id);
                edu.status = "Gruppe entfernt; Mitglieder wurden keiner Gruppe zugeordnet.".into();
            }
        });
    }
    if class.groups.is_empty() {
        ui.label(
            RichText::new("Keine Gruppen.")
                .small()
                .color(theme::TEXT_FAINT),
        );
    }

    // ---- members ----------------------------------------------------
    ui.add_space(4.0);
    ui.label(RichText::new("Mitglieder").strong());
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut edu.input_member_name);
        ui.text_edit_singleline(&mut edu.input_pseudonym);
        ui.label("(Pseudonym, optional)");
        if ui.button("Mitglied anlegen").clicked()
            && (!edu.input_member_name.trim().is_empty() || !edu.input_pseudonym.trim().is_empty())
        {
            match classroom::add_member(
                ws,
                class_id,
                &edu.input_member_name,
                edu.input_pseudonym
                    .trim()
                    .is_empty()
                    .then_some(edu.input_pseudonym.as_str()),
            ) {
                Ok(_) => {
                    edu.input_member_name.clear();
                    edu.input_pseudonym.clear();
                    edu.status = "Mitglied hinzugefügt.".into();
                }
                Err(e) => edu.set_error(e.to_string()),
            }
        }
    });

    let mut remove_member = None;
    for member in class.members.iter().filter(|m| !m.archived) {
        let group_name = member
            .group_id
            .and_then(|gid| class.groups.iter().find(|g| g.id == gid))
            .map(|g| g.name.clone());
        ui.horizontal(|ui| {
            ui.label(format!("• {}", member.author_name()));
            if let Some(g) = group_name {
                ui.label(
                    RichText::new(format!("[{g}]"))
                        .small()
                        .color(theme::TEXT_DIM),
                );
            }
            egui::ComboBox::from_id_salt(format!("grp-{}", member.id))
                .selected_text(
                    member
                        .group_id
                        .and_then(|gid| class.groups.iter().find(|g| g.id == gid))
                        .map(|g| g.name.clone())
                        .unwrap_or_else(|| "—".into()),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(member.group_id.is_none(), "—")
                        .clicked()
                    {
                        let _ = classroom::move_member_to_group(ws, class_id, member.id, None);
                    }
                    for g in &class.groups {
                        if ui
                            .selectable_label(member.group_id == Some(g.id), &g.name)
                            .clicked()
                        {
                            let _ = classroom::move_member_to_group(
                                ws,
                                class_id,
                                member.id,
                                Some(g.id),
                            );
                        }
                    }
                });
            if ui.small_button("Entfernen").clicked() {
                remove_member = Some(member.id);
            }
        });
    }
    if let Some(mid) = remove_member {
        let _ = classroom::remove_member(ws, class_id, mid);
        edu.status = "Mitglied entfernt.".into();
    }
    if class.members.is_empty() {
        ui.label(
            RichText::new("Keine Mitglieder.")
                .small()
                .color(theme::TEXT_FAINT),
        );
    }
}

fn export_classroom_package(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    class: &nova_edu::classroom::Classroom,
) {
    let manifest = PackageManifest {
        created_by: PackageAuthor {
            display_name: edu.profile.author_name().to_string(),
            role: "teacher".into(),
        },
        payload: serde_json::to_value(class).unwrap_or(serde_json::Value::Null),
        ..Default::default()
    };
    let files = vec![PackageFile {
        path: "classroom.json".into(),
        data: serde_json::to_vec_pretty(class).unwrap_or_default(),
    }];
    let fname = package::package_filename(PackageKind::Classroom, &class.name, class.id);
    let Some(dir) = rfd::FileDialog::new()
        .set_directory(ws.root.clone())
        .pick_folder()
    else {
        return;
    };
    let path = dir.join(fname);
    match package::write_package(&path, PackageKind::Classroom, &manifest, &files) {
        Ok(_) => edu.status = format!("Klassen-Paket: {}", path.display()),
        Err(e) => edu.set_error(e.to_string()),
    }
    let _ = ui;
}

fn export_roster(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    class: &nova_edu::classroom::Classroom,
) {
    let path = ws.reports_dir().join(format!("liste-{}.json", class.name));
    let data = serde_json::json!({
        "schema": 1,
        "class": class.name,
        "members": class.members.iter().map(|m| serde_json::json!({
            "display_name": m.display_name,
            "pseudonym": m.pseudonym,
        })).collect::<Vec<_>>(),
    });
    match std::fs::create_dir_all(ws.reports_dir())
        .and_then(|_| std::fs::write(&path, serde_json::to_string_pretty(&data).unwrap()))
    {
        Ok(_) => edu.status = format!("Teilnehmerliste exportiert: {}", path.display()),
        Err(e) => edu.set_error(format!("Export: {e}")),
    }
    let _ = ui;
}

fn import_roster(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, class_id: Uuid) {
    let Some(file) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file()
    else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&file) else {
        edu.set_error("Datei nicht lesbar.");
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        edu.set_error("Keine gültige JSON-Datei.");
        return;
    };
    let Some(members) = v.get("members").and_then(|m| m.as_array()) else {
        edu.set_error("'members'-Array fehlt.");
        return;
    };
    let mut added = 0;
    for m in members {
        let name = m.get("display_name").and_then(|s| s.as_str()).unwrap_or("");
        let pseudo = m.get("pseudonym").and_then(|s| s.as_str());
        if classroom::add_member(ws, class_id, name, pseudo).is_ok() {
            added += 1;
        }
    }
    edu.status = format!("{added} Mitglieder importiert.");
    let _ = ui;
}

fn student_classes(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    let active: Vec<_> = classes.iter().filter(|c| !c.archived).collect();
    if active.is_empty() {
        ui.label(RichText::new("Derzeit keine Klasse zugeordnet.").color(theme::TEXT_DIM));
        return;
    }
    // Member self-linking: the student picks their roster entry once; the id
    // is stored in the local profile (still no authentication).
    ui.horizontal(|ui| {
        ui.label("Ich bin:");
        let mut picked = None;
        'outer: for class in &active {
            for member in class.members.iter().filter(|m| !m.archived) {
                if ui
                    .selectable_label(
                        edu.profile.member_id == Some(member.id),
                        format!("{} ({})", member.author_name(), class.name),
                    )
                    .clicked()
                {
                    picked = Some(member.id);
                    break 'outer;
                }
            }
        }
        if let Some(id) = picked {
            edu.profile.member_id = Some(id);
            edu.save_local(&branding_config_dir());
            edu.status = "Mitglied verknüpft.".into();
        }
        if edu.profile.member_id.is_some() {
            ui.label(RichText::new("[verknüpft]").small().color(theme::GREEN));
        }
    });
    ui.add_space(4.0);
    // Students see all classes; their own member record links via profile.member_id.
    for class in &active {
        egui::CollapsingHeader::new(&class.name).show(ui, |ui| {
            ui.label(format!(
                "Fach: {} · Jahrgang: {}",
                if class.subject.is_empty() {
                    "—"
                } else {
                    &class.subject
                },
                if class.term.is_empty() {
                    "—"
                } else {
                    &class.term
                }
            ));
            if class.groups.is_empty() {
                ui.label("Keine Gruppen.");
            } else {
                ui.label(
                    RichText::new(format!(
                        "Gruppen: {}",
                        class
                            .groups
                            .iter()
                            .map(|g| g.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .small(),
                );
            }
            let count = class.members.iter().filter(|m| !m.archived).count();
            ui.label(
                RichText::new(format!("{count} Mitglieder"))
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        });
    }
}

// --------------------------------------------------------------- assignments

fn assignments_tab(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    actions: &mut Vec<UiAction>,
    project_json: Option<&[u8]>,
    project_name: &str,
) {
    if edu.is_teacher() {
        teacher_assignments(ui, edu, ws);
    } else {
        student_assignments(ui, edu, ws, actions, project_json, project_name);
    }
}

fn teacher_assignments(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    // ---- create ------------------------------------------------------
    egui::CollapsingHeader::new("Neue Aufgabe")
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Titel:");
                ui.text_edit_singleline(&mut edu.input_assignment_title);
            });
            ui.label("Anweisungen (Markdown):");
            ui.add(
                egui::TextEdit::multiline(&mut edu.input_assignment_instructions)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            );
            ui.horizontal(|ui| {
                ui.label("Starter (.nova, optional):");
                ui.text_edit_singleline(&mut edu.draft_starter);
                if ui.button("…").clicked()
                    && let Some(f) = rfd::FileDialog::new()
                        .add_filter("NØVA-Projekt", &["nova"])
                        .pick_file()
                {
                    if let Ok(rel) = f.strip_prefix(&ws.root) {
                        edu.draft_starter = rel.to_string_lossy().replace('\\', "/");
                    } else {
                        edu.set_error("Starter muss innerhalb des Workspace liegen.");
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Aufgabe anlegen").clicked()
                    && !edu.input_assignment_title.trim().is_empty()
                {
                    let classes = classroom::list_classrooms(ws).unwrap_or_default();
                    let class_ids: Vec<_> = classes
                        .iter()
                        .filter(|c| !c.archived)
                        .map(|c| c.id)
                        .collect();
                    let starter = if edu.draft_starter.trim().is_empty() {
                        StarterRef::None
                    } else {
                        StarterRef::Workspace {
                            path: edu.draft_starter.clone(),
                        }
                    };
                    match assignment::save_new_assignment(
                        ws,
                        Assignment {
                            title: edu.input_assignment_title.clone(),
                            instructions_md: edu.input_assignment_instructions.clone(),
                            starter,
                            targets: nova_edu::assignment::AssignmentTargets {
                                class_ids,
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    ) {
                        Ok(a) => {
                            edu.selected_assignment = Some(a.id);
                            edu.input_assignment_title.clear();
                            edu.input_assignment_instructions.clear();
                            edu.draft_starter.clear();
                            edu.status = "Aufgabe angelegt.".into();
                        }
                        Err(e) => edu.set_error(e.to_string()),
                    }
                }
            });
        });
    ui.add_space(6.0);

    // ---- list ---------------------------------------------------------
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    if asgs.is_empty() {
        ui.label(RichText::new("Noch keine Aufgaben.").color(theme::TEXT_DIM));
        return;
    }
    for a in &asgs {
        let selected = edu.selected_assignment == Some(a.id);
        egui::CollapsingHeader::new(format!(
            "{}{}",
            a.title,
            if a.archived { "  [archiviert]" } else { "" }
        ))
        .default_open(selected)
        .show(ui, |ui| {
            if edu.selected_assignment.is_none() {
                edu.selected_assignment = Some(a.id);
            }
            assignment_detail_teacher(ui, edu, ws, a);
        });
    }
}

fn assignment_detail_teacher(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, a: &Assignment) {
    ui.label(
        RichText::new(format!(
            "Anweisungen:\n{}",
            if a.instructions_md.is_empty() {
                "—"
            } else {
                &a.instructions_md
            }
        ))
        .small(),
    );
    ui.label(
        RichText::new(format!(
            "Fällig: {} · Spätabgaben: {}",
            a.due_unix
                .map(|t| {
                    // simple UTC date via report helper is private; show unix->date
                    unix_date(t)
                })
                .unwrap_or_else(|| "—".into()),
            if a.allow_late {
                "erlaubt"
            } else {
                "nicht erlaubt"
            }
        ))
        .small()
        .color(theme::TEXT_DIM),
    );
    ui.horizontal(|ui| {
        if !a.archived && ui.button("Als .novaassignment exportieren").clicked() {
            export_assignment_package(ui, edu, ws, a);
        }
        if !a.archived && ui.button("Archivieren").clicked() {
            let _ = assignment::archive_assignment(ws, a.id);
            edu.status = "Aufgabe archiviert.".into();
        }
        if ui.button("Löschen").clicked() {
            let _ = assignment::delete_assignment(ws, a.id);
            edu.selected_assignment = None;
            edu.status = "Aufgabe gelöscht.".into();
        }
        // Import a .novaassignment back (e.g. received from a colleague).
        if ui.button(".novaassignment importieren").clicked() {
            import_package(ui, edu, ws, PackageKind::Assignment);
        }
    });
    // Submission count
    let subs = submission::list_submissions(ws, a.id).unwrap_or_default();
    let members = subs
        .iter()
        .map(|s| s.member_id)
        .collect::<std::collections::HashSet<_>>()
        .len();
    ui.label(
        RichText::new(format!(
            "Abgaben: {} Einreichungen von {} Personen",
            subs.len(),
            members
        ))
        .small()
        .color(theme::TEXT_FAINT),
    );
}

fn unix_date(unix: u64) -> String {
    // Reuse report's civil-date algorithm via a tiny local copy (UTC).
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn export_assignment_package(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, a: &Assignment) {
    let starter = match assignment::starter_source(ws, a) {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            edu.set_error("Aufgabe hat keinen Starter.");
            return;
        }
        Err(e) => {
            edu.set_error(e.to_string());
            return;
        }
    };
    let (starter_abs, starter_rel) = starter;
    let starter_bytes = match std::fs::read(&starter_abs) {
        Ok(b) => b,
        Err(e) => {
            edu.set_error(format!("Starter lesen: {e}"));
            return;
        }
    };
    let files = vec![
        PackageFile {
            path: "assignment.json".into(),
            data: serde_json::to_vec_pretty(a).unwrap_or_default(),
        },
        PackageFile {
            path: starter_rel,
            data: starter_bytes,
        },
    ];
    let manifest = PackageManifest {
        created_by: PackageAuthor {
            display_name: edu.profile.author_name().to_string(),
            role: "teacher".into(),
        },
        content: [("assignment".to_string(), "assignment.json".to_string())]
            .into_iter()
            .collect(),
        payload: serde_json::to_value(a).unwrap_or(serde_json::Value::Null),
        ..Default::default()
    };
    let filename = package::package_filename(PackageKind::Assignment, &a.title, a.id);
    let Some(dir) = rfd::FileDialog::new()
        .set_directory(ws.root.clone())
        .pick_folder()
    else {
        return;
    };
    let path = dir.join(filename);
    match package::write_package(&path, PackageKind::Assignment, &manifest, &files) {
        Ok(_) => edu.status = format!("Paket exportiert: {}", path.display()),
        Err(e) => edu.set_error(e.to_string()),
    }
    let _ = ui;
}

fn import_package(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, kind: PackageKind) {
    let Some(file) = rfd::FileDialog::new()
        .add_filter(kind.extension(), &[kind.extension()])
        .pick_file()
    else {
        return;
    };
    match package::read_package(&file) {
        Ok(pkg) => {
            // Transactional: everything is validated before any state write.
            let result = match kind {
                PackageKind::Assignment => import_assignment_pkg(edu, ws, &pkg),
                PackageKind::Submission => import_submission_pkg(edu, ws, &pkg),
                PackageKind::Feedback => import_feedback_pkg(edu, ws, &pkg),
                PackageKind::Classroom => import_classroom_pkg(edu, ws, &pkg),
            };
            match result {
                Ok(msg) => edu.status = msg,
                Err(e) => edu.set_error(e.to_string()),
            }
        }
        Err(e) => edu.set_error(format!("Paket ungültig: {e}")),
    }
    let _ = ui;
}

fn import_assignment_pkg(
    _edu: &mut EduUi,
    ws: &Workspace,
    pkg: &package::ExtractedPackage,
) -> Result<String, String> {
    let Some(data) = pkg.files.get("assignment.json") else {
        return Err("assignment.json fehlt im Paket".into());
    };
    let mut a: Assignment =
        serde_json::from_slice(data).map_err(|e| format!("assignment.json: {e}"))?;
    a.id = Uuid::new_v4(); // imported copy gets fresh id to avoid collisions
    assignment::save_new_assignment(ws, a).map_err(|e| e.to_string())?;
    // Copy starter file if present.
    for (name, bytes) in &pkg.files {
        if name.starts_with("starter/") {
            let dest = ws
                .root
                .join("templates")
                .join(name.trim_start_matches("starter/"));
            if let Some(p) = dest.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            let _ = std::fs::write(&dest, bytes);
        }
    }
    Ok(format!(
        "Aufgabe importiert (von {}).",
        pkg.manifest.created_by.display_name
    ))
}

fn import_submission_pkg(
    _edu: &mut EduUi,
    ws: &Workspace,
    pkg: &package::ExtractedPackage,
) -> Result<String, String> {
    let Some(data) = pkg.files.get("submission.json") else {
        return Err("submission.json fehlt im Paket".into());
    };
    let sub: Submission =
        serde_json::from_slice(data).map_err(|e| format!("submission.json: {e}"))?;
    // Write payload files under inbox tree via record path.
    let mut files = Vec::new();
    for (name, bytes) in &pkg.files {
        if name == "submission.json" || name == "manifest.json" {
            continue;
        }
        files.push((name.clone(), bytes.clone()));
    }
    submission::record_submission(ws, sub, &files).map_err(|e| e.to_string())?;
    Ok(format!(
        "Abgabe importiert (von {}).",
        pkg.manifest.created_by.display_name
    ))
}

fn import_feedback_pkg(
    _edu: &mut EduUi,
    ws: &Workspace,
    pkg: &package::ExtractedPackage,
) -> Result<String, String> {
    let Some(data) = pkg.files.get("feedback.json") else {
        return Err("feedback.json fehlt im Paket".into());
    };
    let bundle: feedback::FeedbackBundle =
        serde_json::from_slice(data).map_err(|e| format!("feedback.json: {e}"))?;
    // Persist via the public bundle writer.
    let mut bundle = bundle;
    bundle.updated_unix = nova_edu::workspace::now_unix();
    feedback::save_feedback_bundle(ws, &bundle).map_err(|e| e.to_string())?;
    Ok(format!(
        "Feedback importiert (von {}).",
        pkg.manifest.created_by.display_name
    ))
}

fn import_classroom_pkg(
    _edu: &mut EduUi,
    ws: &Workspace,
    pkg: &package::ExtractedPackage,
) -> Result<String, String> {
    // .novaclassroom carries classroom.json + optional members/groups.
    let Some(data) = pkg.files.get("classroom.json") else {
        return Err("classroom.json fehlt im Paket".into());
    };
    let mut class: nova_edu::classroom::Classroom =
        serde_json::from_slice(data).map_err(|e| format!("classroom.json: {e}"))?;
    class.id = Uuid::new_v4();
    for m in class.members.iter_mut() {
        m.id = Uuid::new_v4();
        m.group_id = None;
    }
    for g in class.groups.iter_mut() {
        g.id = Uuid::new_v4();
    }
    // Persist by writing the file directly (validated on next load).
    let path = ws.classroom_path(class.id);
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&class).unwrap())
        .map_err(|e| e.to_string())?;
    Ok(format!("Klasse '{}' importiert.", class.name))
}

// -------------------------------------------------------------- student view

fn student_assignments(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    actions: &mut Vec<UiAction>,
    project_json: Option<&[u8]>,
    project_name: &str,
) {
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    let active: Vec<_> = asgs.iter().filter(|a| !a.archived).collect();
    if active.is_empty() {
        ui.label(RichText::new("Derzeit keine Aufgaben.").color(theme::TEXT_DIM));
        return;
    }
    let member_id = edu.profile.member_id;
    for a in &active {
        let subs = submission::latest_per_member(ws, a.id).unwrap_or_default();
        let mine = member_id.and_then(|mid| subs.iter().find(|s| s.member_id == mid));
        egui::CollapsingHeader::new(format!(
            "{}  {}",
            a.title,
            match mine.map(|s| s.state) {
                Some(SubmissionState::Submitted) => "[abgegeben]",
                Some(SubmissionState::Reviewed) => "[gesehen]",
                Some(SubmissionState::Graded) => "[bewertet]",
                Some(SubmissionState::Returned) => "[zurückgegeben]",
                _ => "[offen]",
            }
        ))
        .show(ui, |ui| {
            if !a.instructions_md.is_empty() {
                ui.label(RichText::new(&a.instructions_md).small());
            }
            ui.label(
                RichText::new(format!(
                    "Fällig: {}",
                    a.due_unix.map(unix_date).unwrap_or_else(|| "—".into())
                ))
                .small()
                .color(theme::TEXT_DIM),
            );
            // Starter öffnen
            if let Ok(Some((_abs, _rel))) = assignment::starter_source(ws, a)
                && ui.button("Starterprojekt öffnen").clicked()
            {
                // starter_source returns abs path in second tuple elem? no —
                // returns (abs, rel). Recompute abs:
                if let StarterRef::Workspace { path } = &a.starter {
                    actions.push(UiAction::OpenProjectPath(ws.root.join(path)));
                }
            }
            ui.horizontal(|ui| {
                // Submit current project as .novasubmission
                if ui
                    .button("Aktuelles Projekt als Abgabe exportieren")
                    .clicked()
                {
                    submit_current_project(ui, edu, ws, a, project_json, project_name);
                }
                if ui.button(".novasubmission exportieren…").clicked() {
                    // Package an existing recorded submission? For students the
                    // primary flow is submit-current; this exports the package
                    // file of the *current* project too.
                    submit_current_project(ui, edu, ws, a, project_json, project_name);
                }
            });
            if let Some(sub) = mine {
                ui.label(
                    RichText::new(format!(
                        "Letzte Abgabe: Revision {} · {}",
                        sub.revision,
                        unix_date(sub.submitted_unix)
                    ))
                    .small()
                    .color(theme::TEXT_FAINT),
                );
            }
        });
    }
}

fn submit_current_project(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    a: &Assignment,
    project_json: Option<&[u8]>,
    project_name: &str,
) {
    let Some(member_id) = edu.profile.member_id else {
        edu.set_error("Kein Mitglied zugeordnet — Profil zuerst vervollständigen.");
        return;
    };
    let Some(class_id) = find_student_class(ws, edu) else {
        edu.set_error("Keine Klasse für dieses Profil gefunden.");
        return;
    };
    let Some(json) = project_json else {
        edu.set_error("Kein Projekt geladen.");
        return;
    };

    // Record submission in workspace inbox AND build .novasubmission package.
    let sub = Submission {
        assignment_id: a.id,
        class_id,
        member_id,
        note: Some(format!("Export aus '{project_name}'")),
        ..Default::default()
    };
    let files = vec![("project/arbeit.nova".to_string(), json.to_vec())];
    match submission::record_submission(ws, sub, &files) {
        Ok(recorded) => {
            // Also write the exchange package next to it.
            let manifest = PackageManifest {
                created_by: PackageAuthor {
                    display_name: edu.profile.author_name().to_string(),
                    role: "student".into(),
                },
                payload: serde_json::to_value(&recorded).unwrap_or(serde_json::Value::Null),
                ..Default::default()
            };
            let pkg_files = vec![
                PackageFile {
                    path: "submission.json".into(),
                    data: serde_json::to_vec_pretty(&recorded).unwrap_or_default(),
                },
                PackageFile {
                    path: "project/arbeit.nova".into(),
                    data: json.to_vec(),
                },
            ];
            let fname = package::package_filename(PackageKind::Submission, &a.title, recorded.id);
            let Some(dir) = rfd::FileDialog::new()
                .set_directory(ws.root.clone())
                .pick_folder()
            else {
                return;
            };
            let path = dir.join(fname);
            match package::write_package(&path, PackageKind::Submission, &manifest, &pkg_files) {
                Ok(_) => edu.status = format!("Abgabe gespeichert + Paket: {}", path.display()),
                Err(e) => edu.set_error(e.to_string()),
            }
        }
        Err(e) => edu.set_error(e.to_string()),
    }
    let _ = ui;
}

fn find_student_class(ws: &Workspace, edu: &EduUi) -> Option<Uuid> {
    let mid = edu.profile.member_id?;
    for class in classroom::list_classrooms(ws).ok()? {
        if class.members.iter().any(|m| m.id == mid) {
            return Some(class.id);
        }
    }
    None
}

// --------------------------------------------------------------- submissions

fn submissions_tab(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    if edu.is_teacher() {
        teacher_submissions(ui, edu, ws);
    } else {
        student_submissions(ui, edu, ws);
    }
}

fn teacher_submissions(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    if asgs.is_empty() {
        ui.label(RichText::new("Keine Aufgaben — zuerst eine anlegen.").color(theme::TEXT_DIM));
        return;
    }
    let selected = edu
        .selected_assignment
        .or_else(|| asgs.first().map(|a| a.id));
    ui.horizontal(|ui| {
        ui.label("Aufgabe:");
        for a in asgs.iter().filter(|a| !a.archived) {
            if ui
                .selectable_label(selected == Some(a.id), &a.title)
                .clicked()
            {
                edu.selected_assignment = Some(a.id);
            }
        }
    });
    let Some(aid) = selected else { return };
    let subs = submission::latest_per_member(ws, aid).unwrap_or_default();
    if subs.is_empty() {
        ui.label(RichText::new("Noch keine Abgaben.").color(theme::TEXT_DIM));
        return;
    }
    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    for sub in &subs {
        let class = classes.iter().find(|c| c.id == sub.class_id);
        let member_name = class
            .and_then(|c| classroom::find_member(c, sub.member_id))
            .map(|m| m.author_name().to_string())
            .unwrap_or_else(|| sub.member_id.to_string()[..8].to_string());
        ui.horizontal(|ui| {
            ui.label(format!(
                "• {} — Rev {} — {} — {}",
                member_name,
                sub.revision,
                sub.state.label(),
                unix_date(sub.submitted_unix)
            ));
            if sub.late {
                ui.label(RichText::new("[spät]").small().color(theme::AMBER));
            }
            if ui.small_button("Ansehen").clicked()
                && let Ok(abs) = sub.project_abs(ws)
            {
                edu.review_project = Some(abs);
                edu.status = format!("Abgabe von {member_name} wird im Projekt-Tab geöffnet.");
            }
            if ui.small_button("Gesehen").clicked() {
                let _ = submission::set_submission_state(ws, sub.id, SubmissionState::Reviewed);
                edu.status = "Status: Gesehen.".into();
            }
            if ui.small_button("Bewertet").clicked() {
                let _ = submission::set_submission_state(ws, sub.id, SubmissionState::Graded);
                edu.status = "Status: Bewertet.".into();
            }
            if ui.small_button("Feedback").clicked() {
                edu.tab = EduTab::Feedback;
                edu.selected_assignment = Some(aid);
                edu.selected_member = Some(sub.member_id);
            }
        });
    }
    ui.add_space(6.0);
    if ui.button(".novasubmission importieren").clicked() {
        import_package(ui, edu, ws, PackageKind::Submission);
    }
}

fn student_submissions(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let Some(mid) = edu.profile.member_id else {
        ui.label(RichText::new("Kein Mitglied zugeordnet.").color(theme::TEXT_DIM));
        return;
    };
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    let mut any = false;
    for a in asgs.iter().filter(|a| !a.archived) {
        let subs = submission::list_submissions(ws, a.id).unwrap_or_default();
        let mine: Vec<_> = subs.iter().filter(|s| s.member_id == mid).collect();
        if mine.is_empty() {
            continue;
        }
        any = true;
        ui.label(RichText::new(&a.title).strong());
        for sub in mine {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "  Rev {} · {} · {}",
                    sub.revision,
                    sub.state.label(),
                    unix_date(sub.submitted_unix)
                ));
                if ui.small_button("Paket exportieren").clicked() {
                    export_submission_package(ui, edu, ws, sub);
                }
            });
        }
    }
    if !any {
        ui.label(RichText::new("Noch keine eigenen Abgaben.").color(theme::TEXT_DIM));
    }
    ui.add_space(4.0);
    if ui.button(".novasubmission importieren").clicked() {
        import_package(ui, edu, ws, PackageKind::Submission);
    }
}

fn export_submission_package(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, sub: &Submission) {
    // Rebuild package from stored payload files.
    let mut files = Vec::new();
    for key in sub.checksums.keys() {
        if let Ok(abs) = nova_edu::error::safe_join(&ws.root, key)
            && let Ok(bytes) = std::fs::read(&abs)
        {
            // strip "inbox/.../files/" prefix → package path
            let rel = key
                .split("/files/")
                .nth(1)
                .unwrap_or(key.as_str())
                .to_string();
            files.push(PackageFile {
                path: rel,
                data: bytes,
            });
        }
    }
    let manifest = PackageManifest {
        created_by: PackageAuthor {
            display_name: edu.profile.author_name().to_string(),
            role: "student".into(),
        },
        payload: serde_json::to_value(sub).unwrap_or(serde_json::Value::Null),
        ..Default::default()
    };
    // Ensure submission.json present
    let mut all = vec![PackageFile {
        path: "submission.json".into(),
        data: serde_json::to_vec_pretty(sub).unwrap_or_default(),
    }];
    all.extend(files);
    let fname = package::package_filename(PackageKind::Submission, "abgabe", sub.id);
    let Some(dir) = rfd::FileDialog::new()
        .set_directory(ws.root.clone())
        .pick_folder()
    else {
        return;
    };
    let path = dir.join(fname);
    match package::write_package(&path, PackageKind::Submission, &manifest, &all) {
        Ok(_) => edu.status = format!("Paket exportiert: {}", path.display()),
        Err(e) => edu.set_error(e.to_string()),
    }
    let _ = ui;
}

// ----------------------------------------------------------------- feedback

fn feedback_tab(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    if edu.is_teacher() {
        teacher_feedback(ui, edu, ws);
    } else {
        student_feedback(ui, edu, ws);
    }
}

fn teacher_feedback(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    let aid = edu
        .selected_assignment
        .or_else(|| asgs.first().map(|a| a.id));
    let Some(aid) = aid else {
        ui.label(RichText::new("Keine Aufgaben.").color(theme::TEXT_DIM));
        return;
    };
    let subs = submission::latest_per_member(ws, aid).unwrap_or_default();
    if subs.is_empty() {
        ui.label(RichText::new("Keine Abgaben für Feedback.").color(theme::TEXT_DIM));
        return;
    }
    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    let mid = edu
        .selected_member
        .or_else(|| subs.first().map(|s| s.member_id));
    ui.horizontal(|ui| {
        ui.label("Abgabe:");
        for sub in &subs {
            let name = classes
                .iter()
                .find(|c| c.id == sub.class_id)
                .and_then(|c| classroom::find_member(c, sub.member_id))
                .map(|m| m.author_name().to_string())
                .unwrap_or_else(|| "?".into());
            if ui
                .selectable_label(
                    mid == Some(sub.member_id),
                    format!("{name} (Klasse {})", sub.class_id),
                )
                .clicked()
            {
                edu.selected_member = Some(sub.member_id);
            }
        }
    });
    let Some(sub) = subs.iter().find(|s| Some(s.member_id) == mid) else {
        return;
    };
    let bundle = feedback::get_feedback(ws, sub.class_id, aid, sub.member_id).unwrap_or_default();

    ui.add_space(4.0);
    ui.label(RichText::new("Kommentare").strong());
    for c in &bundle.comments {
        ui.horizontal(|ui| {
            ui.label(format!(
                "[{}] {} — {}",
                c.category.label(),
                c.author.display_name,
                unix_date(c.created_unix)
            ));
            if c.orphaned {
                ui.label(
                    RichText::new("[verwaister Anker]")
                        .small()
                        .color(theme::AMBER),
                );
            }
            if c.resolved {
                ui.label(RichText::new("[erledigt]").small().color(theme::GREEN));
            }
            if !c.resolved && ui.small_button("Erledigen").clicked() {
                let _ = feedback::resolve_comment(ws, sub.class_id, aid, sub.member_id, c.id);
            }
        });
        ui.label(RichText::new(format!("   {}", c.text)).small());
        if let Some(a) = &c.anchor {
            ui.label(
                RichText::new(format!("   Anker: {}", a.describe()))
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        }
    }
    if bundle.comments.is_empty() {
        ui.label(
            RichText::new("Noch kein Feedback.")
                .small()
                .color(theme::TEXT_FAINT),
        );
    }

    // Add comment
    ui.add_space(6.0);
    ui.label(RichText::new("Neuer Kommentar").strong());
    // Minimal inline inputs via edu fields reused is messy; use egui temp vars.
    comment_inputs(ui, edu, ws, sub, aid);

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Feedback exportieren (.novafeedback)").clicked() {
            export_feedback_package(ui, edu, ws, sub, aid);
        }
        if ui.button(".novafeedback importieren").clicked() {
            import_package(ui, edu, ws, PackageKind::Feedback);
        }
    });
}

fn comment_inputs(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace, sub: &Submission, aid: Uuid) {
    // Store draft comment in edu.input_member_name/pseudonym temporarily? Better:
    // use egui memory via id — simplest: two static-ish fields on EduUi.
    // We reuse input_pseudonym as comment text and add a category selector.
    ui.horizontal(|ui| {
        ui.label("Kategorie:");
        // Category cycling via a small combo stored in prefs.extra? Keep simple:
        // use selectables writing to a transient field.
        for cat in [
            CommentCategory::Praise,
            CommentCategory::Issue,
            CommentCategory::Question,
            CommentCategory::Requirement,
        ] {
            if ui.selectable_label(false, cat.label()).clicked() {
                // handled below via edu.pending_package? no — store in prefs.extra
                edu.prefs.extra.insert(
                    "comment_category".into(),
                    serde_json::Value::String(format!("{:?}", cat)),
                );
            }
        }
    });
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut edu.input_pseudonym);
        if ui.button("Kommentar hinzufügen").clicked() && !edu.input_pseudonym.trim().is_empty() {
            let cat = match edu
                .prefs
                .extra
                .get("comment_category")
                .and_then(|v| v.as_str())
            {
                Some("Praise") => CommentCategory::Praise,
                Some("Question") => CommentCategory::Question,
                Some("Requirement") => CommentCategory::Requirement,
                _ => CommentCategory::Issue,
            };
            let author = AuthorRef {
                display_name: edu.profile.author_name().to_string(),
                role: edu.profile.role.label().to_string(),
            };
            match feedback::add_comment(
                ws,
                sub.class_id,
                aid,
                sub.member_id,
                author,
                None,
                cat,
                &edu.input_pseudonym.clone(),
            ) {
                Ok(_) => {
                    edu.input_pseudonym.clear();
                    edu.status = "Kommentar gespeichert.".into();
                }
                Err(e) => edu.set_error(e.to_string()),
            }
        }
    });
    ui.label(
        RichText::new(
            "(Objekt-Anker werden in der Projektansicht gesetzt; hier allgemeine Kommentare.)",
        )
        .small()
        .color(theme::TEXT_FAINT),
    );
}

fn export_feedback_package(
    ui: &mut Ui,
    edu: &mut EduUi,
    ws: &Workspace,
    sub: &Submission,
    aid: Uuid,
) {
    let bundle = match feedback::get_feedback(ws, sub.class_id, aid, sub.member_id) {
        Ok(b) => b,
        Err(e) => {
            edu.set_error(e.to_string());
            return;
        }
    };
    let manifest = PackageManifest {
        created_by: PackageAuthor {
            display_name: edu.profile.author_name().to_string(),
            role: "teacher".into(),
        },
        payload: serde_json::to_value(&bundle).unwrap_or(serde_json::Value::Null),
        ..Default::default()
    };
    let files = vec![PackageFile {
        path: "feedback.json".into(),
        data: serde_json::to_vec_pretty(&bundle).unwrap_or_default(),
    }];
    let fname = package::package_filename(PackageKind::Feedback, "feedback", bundle.class_id);
    let Some(dir) = rfd::FileDialog::new()
        .set_directory(ws.root.clone())
        .pick_folder()
    else {
        return;
    };
    let path = dir.join(fname);
    match package::write_package(&path, PackageKind::Feedback, &manifest, &files) {
        Ok(_) => edu.status = format!("Feedback-Paket: {}", path.display()),
        Err(e) => edu.set_error(e.to_string()),
    }
    let _ = ui;
}

fn student_feedback(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let Some(mid) = edu.profile.member_id else {
        ui.label(RichText::new("Kein Mitglied zugeordnet.").color(theme::TEXT_DIM));
        return;
    };
    // Find bundles for this member across assignments.
    let mut found = Vec::new();
    let fb_dir = ws.root.join("feedback");
    if fb_dir.exists() {
        for class_entry in std::fs::read_dir(&fb_dir).into_iter().flatten().flatten() {
            let class_dir = class_entry.path();
            if !class_dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&class_dir)
                .into_iter()
                .flatten()
                .flatten()
            {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                // Load file directly:
                if let Ok(text) = std::fs::read_to_string(&path)
                    && let Ok(b) = serde_json::from_str::<feedback::FeedbackBundle>(&text)
                    && b.member_id == mid
                    && !b.comments.is_empty()
                {
                    found.push(b);
                }
            }
        }
    }
    if found.is_empty() {
        ui.label(RichText::new("Noch kein Feedback.").color(theme::TEXT_DIM));
        return;
    }
    for bundle in &found {
        let asg_title = assignment::get_assignment(ws, bundle.assignment_id)
            .map(|a| a.title)
            .unwrap_or_else(|_| bundle.assignment_id.to_string());
        ui.label(RichText::new(&asg_title).strong());
        for c in &bundle.comments {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "• [{}] {}",
                    c.category.label(),
                    c.author.display_name
                ));
                if c.orphaned {
                    ui.label(
                        RichText::new("[Anker nicht mehr auflösbar]")
                            .small()
                            .color(theme::AMBER),
                    );
                }
                if !c.acknowledged && ui.small_button("Gelesen").clicked() {
                    let _ = feedback::acknowledge_comment(
                        ws,
                        bundle.class_id,
                        bundle.assignment_id,
                        bundle.member_id,
                        c.id,
                    );
                }
            });
            ui.label(RichText::new(format!("   {}", c.text)).small());
            if let Some(a) = &c.anchor {
                ui.label(
                    RichText::new(format!("   Anker: {}", a.describe()))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            }
        }
        ui.separator();
    }
    ui.add_space(4.0);
    if ui.button(".novafeedback importieren").clicked() {
        import_package(ui, edu, ws, PackageKind::Feedback);
    }
}

// ----------------------------------------------------------------- progress

fn progress_tab(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    let classes = classroom::list_classrooms(ws).unwrap_or_default();
    let asgs = assignment::list_assignments(ws).unwrap_or_default();
    if classes.is_empty() || asgs.is_empty() {
        ui.label(RichText::new("Klassen und Aufgaben nötig.").color(theme::TEXT_DIM));
        return;
    }
    let class = classes
        .iter()
        .find(|c| Some(c.id) == edu.selected_class)
        .or_else(|| classes.first());
    let Some(class) = class else { return };
    let asg = asgs
        .iter()
        .find(|a| Some(a.id) == edu.selected_assignment)
        .or_else(|| asgs.first());
    let Some(asg) = asg else { return };

    ui.horizontal(|ui| {
        ui.label("Klasse:");
        for c in &classes {
            if ui
                .selectable_label(Some(c.id) == edu.selected_class, &c.name)
                .clicked()
            {
                edu.selected_class = Some(c.id);
            }
        }
        ui.label("Aufgabe:");
        for a in asgs.iter().filter(|a| !a.archived) {
            if ui
                .selectable_label(Some(a.id) == edu.selected_assignment, &a.title)
                .clicked()
            {
                edu.selected_assignment = Some(a.id);
            }
        }
    });

    let Ok(class_full) = classroom::get_classroom(ws, class.id) else {
        return;
    };
    let rows = progress::assignment_progress(ws, &class_full, asg).unwrap_or_default();
    if rows.is_empty() {
        ui.label(RichText::new("Keine Mitglieder.").color(theme::TEXT_DIM));
        return;
    }
    egui::Grid::new("progress-grid")
        .striped(true)
        .show(ui, |ui| {
            ui.label(RichText::new("Mitglied").strong());
            ui.label(RichText::new("Gruppe").strong());
            ui.label(RichText::new("Status").strong());
            ui.label(RichText::new("Rev.").strong());
            ui.label(RichText::new("Abgabe").strong());
            ui.label(RichText::new("Spät").strong());
            ui.label(RichText::new("FB-Kommentare").strong());
            ui.end_row();
            for row in &rows {
                ui.label(&row.member_name);
                ui.label(row.group_name.as_deref().unwrap_or("—"));
                ui.label(row.status.label());
                ui.label(row.revision.to_string());
                ui.label(
                    row.submitted_unix
                        .map(unix_date)
                        .unwrap_or_else(|| "—".into()),
                );
                ui.label(if row.late { "ja" } else { "" });
                ui.label(format!(
                    "{} ({})",
                    row.feedback_comments, row.unacknowledged
                ));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button("Fortschritt als CSV exportieren").clicked() {
            let fname = format!(
                "fortschritt-{}-{}.csv",
                sanitize(&class_full.name),
                &asg.id.to_string()[..8]
            );
            match nova_edu::report::export_progress_csv(ws, &rows, &asg.title, &fname) {
                Ok(p) => edu.status = format!("Bericht: {}", p.display()),
                Err(e) => edu.set_error(e.to_string()),
            }
        }
        if ui.button("Klassenübersicht (Markdown)").clicked() {
            match progress::class_overview(ws, &class_full) {
                Ok(ov) => {
                    let dir = ws.reports_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let p = dir.join(format!("uebersicht-{}.md", sanitize(&class_full.name)));
                    let _ = std::fs::write(&p, nova_edu::report::class_overview_markdown(&ov));
                    edu.status = format!("Übersicht: {}", p.display());
                }
                Err(e) => edu.set_error(e.to_string()),
            }
        }
    });
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .take(30)
        .collect()
}

// ---------------------------------------------------------------- templates

fn templates_tab(ui: &mut Ui, edu: &mut EduUi, ws: &Workspace) {
    ui.label(RichText::new("Lehrvorlagen").strong());
    let catalog = template::TemplateCatalog::load(ws).unwrap_or_default();
    if catalog.templates.is_empty() {
        ui.label(
            RichText::new("Kein Katalog —.example: templates/catalog.json im Workspace.")
                .small()
                .color(theme::TEXT_FAINT),
        );
    } else {
        for t in &catalog.templates {
            ui.horizontal(|ui| {
                ui.label(format!("• {} ({})", t.title, t.level));
                ui.label(
                    RichText::new(format!("~{} Min", t.minutes))
                        .small()
                        .color(theme::TEXT_FAINT),
                );
            });
            ui.label(RichText::new(&t.description).small().color(theme::TEXT_DIM));
        }
    }
    ui.add_space(8.0);
    ui.label(RichText::new("Tutorials").strong());
    for (id, title, body) in template::builtin_tutorials() {
        egui::CollapsingHeader::new(title).show(ui, |ui| {
            ui.label(RichText::new(body).small());
            let _ = id;
        });
    }
    ui.add_space(6.0);
    if edu.school.retention_days.is_some() {
        ui.label(
            RichText::new(format!(
                "Hinweis: school.json enthält retentionDays={:?} — Aufbewahrung ist eine Empfehlung, nichts wird automatisch gelöscht.",
                edu.school.retention_days
            ))
            .small()
            .color(theme::AMBER),
        );
    }
}
