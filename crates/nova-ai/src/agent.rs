//! Project-aware agent context and provider request orchestration.

use std::fmt::Write as _;

use nova_as::Diagnostic;
use nova_core::{ObjectKind, Project, ScriptId, SourceFormat};
use nova_formats::{CompatibilityReport, FormatKind, analyze_project};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AiError, AiResult};
use crate::files::ProposedChange;
use crate::operations::{MAX_OPERATIONS, ProjectOperation};
use crate::provider::{ChatMessage, ChatResponse, ProviderManager};

pub const MAX_CONTEXT_CHARS: usize = 48_000;
pub const MAX_SOURCE_CHARS: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSelection {
    pub include_project_structure: bool,
    pub include_timeline: bool,
    pub include_script_metadata: bool,
    pub include_script_sources: bool,
    pub include_assets: bool,
    pub include_diagnostics: bool,
    pub selected_scripts: Vec<ScriptId>,
    pub selected_objects: Vec<Uuid>,
    pub selected_code: String,
    pub notes: String,
}

impl Default for ContextSelection {
    fn default() -> Self {
        Self {
            include_project_structure: true,
            include_timeline: true,
            include_script_metadata: true,
            include_script_sources: true,
            include_assets: true,
            include_diagnostics: true,
            selected_scripts: Vec::new(),
            selected_objects: Vec::new(),
            selected_code: String::new(),
            notes: String::new(),
        }
    }
}

impl ContextSelection {
    pub fn selected_source_chars(&self, project: &Project) -> usize {
        project
            .scripts
            .iter()
            .filter(|script| self.selected_scripts.contains(&script.id))
            .map(|script| script.source.len())
            .sum::<usize>()
            .min(MAX_CONTEXT_CHARS)
    }

    pub fn selected_count(&self) -> usize {
        self.selected_scripts.len()
    }
}

pub fn build_context(
    project: &Project,
    selection: &ContextSelection,
    active_frame: usize,
    mode: &str,
    diagnostics: &[Diagnostic],
) -> String {
    let format = project
        .source
        .as_ref()
        .map(|source| match source.format {
            SourceFormat::Nova => FormatKind::Nova,
            SourceFormat::Fla => FormatKind::Fla,
            SourceFormat::Swf => FormatKind::Swf,
        })
        .unwrap_or(FormatKind::Nova);
    let report = CompatibilityReport::new(format);
    let analysis = analyze_project(
        project,
        format,
        project
            .source
            .as_ref()
            .and_then(|s| s.original_version.clone()),
        &report,
    );

    let mut out = String::new();
    let _ = writeln!(out, "NOVA Flash Studio – Projektkontext");
    let _ = writeln!(out, "Format: {}", format.label());
    let _ = writeln!(out, "Projekt: {}", project.metadata.name);
    if !project.metadata.description.is_empty() {
        let _ = writeln!(out, "Beschreibung: {}", project.metadata.description);
    }
    let _ = writeln!(out, "Modus: {mode}, aktiver Frame: {}", active_frame + 1);
    let _ = writeln!(
        out,
        "Zusammenfassung: {} Szenen, {} Ebenen, {} Frames, {} Objekte, {} Symbole, {} Skripte, {} Assets",
        analysis.scenes,
        analysis.layers,
        analysis.frames,
        analysis.objects,
        analysis.symbols,
        analysis.scripts,
        analysis.assets
    );
    let _ = writeln!(
        out,
        "Bühne: {:.0}x{:.0} px, {:.0} fps",
        analysis.stage_width, analysis.stage_height, analysis.frame_rate
    );
    let _ = writeln!(
        out,
        "Export: Ziel {:?}, SWF-Version {}, Kompression {:?}, Skripte {}",
        project.export.target,
        project.export.swf_version,
        project.export.compression,
        if project.export.include_scripts {
            "aktiv"
        } else {
            "deaktiviert"
        }
    );
    if let Some(source) = &project.source {
        let _ = writeln!(
            out,
            "Importquelle: {:?}, Version {:?}, Hinweis: {}",
            source.format, source.original_version, source.note
        );
        if !source.symbol_classes.is_empty() {
            let _ = writeln!(
                out,
                "SymbolClass-Verknüpfungen (aus SWF erhalten): {}",
                source
                    .symbol_classes
                    .iter()
                    .map(|link| format!("{} -> {}", link.character_id, link.class_name))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }

    if selection.include_project_structure {
        let _ = writeln!(out, "\nSzenen:");
        for scene in &project.scenes {
            let active = if Some(scene.id) == project.active_scene {
                " [aktiv]"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "- {}{} ({:.0}x{:.0} px, {:.0} fps, {} Ebenen, {} Frames)",
                scene.name,
                active,
                scene.width,
                scene.height,
                scene.fps,
                scene.layers.len(),
                scene.frame_count()
            );
        }
        let _ = writeln!(out, "Symbole:");
        for symbol in &project.symbols {
            let _ = writeln!(
                out,
                "- {} ({:.1}x{:.1})",
                symbol.name, symbol.width, symbol.height
            );
        }
    }

    if selection.include_timeline {
        if let Some(scene) = project.active_scene() {
            let _ = writeln!(out, "\nAktive Szene '{}':", scene.name);
            for layer in &scene.layers {
                let keyframes: Vec<String> = layer
                    .frames
                    .iter()
                    .enumerate()
                    .filter(|(_, frame)| frame.keyframe)
                    .map(|(index, _)| (index + 1).to_string())
                    .collect();
                let tween_count = layer
                    .frames
                    .iter()
                    .filter(|frame| frame.tween.is_some())
                    .count();
                let _ = writeln!(
                    out,
                    "- Ebene '{}': {} Frames, Keyframes [{}], Tweens {}, sichtbar {}, gesperrt {}, Typ {:?}",
                    layer.name,
                    layer.frames.len(),
                    keyframes.join(", "),
                    tween_count,
                    layer.visible,
                    layer.locked,
                    layer.kind
                );
                let mut object_count = 0;
                for (frame_index, frame) in layer.frames.iter().enumerate() {
                    for object in &frame.objects {
                        if object_count >= 120 {
                            break;
                        }
                        object_count += 1;
                        let selected = if selection.selected_objects.contains(&object.id) {
                            " [ausgewählt]"
                        } else {
                            ""
                        };
                        let kind = object.kind.type_name();
                        let symbol = match &object.kind {
                            ObjectKind::Symbol { symbol } => format!(", Symbol {}", symbol),
                            _ => String::new(),
                        };
                        let script = object
                            .script
                            .map(|id| format!(", Skript {}", id))
                            .unwrap_or_default();
                        let character = object
                            .source_character
                            .map(|id| format!(", SWF-Charakter {}", id))
                            .unwrap_or_default();
                        let _ = writeln!(
                            out,
                            "  Frame {}: {}{} ({}{}{}{}) bei ({:.1}, {:.1})",
                            frame_index + 1,
                            object.name,
                            selected,
                            kind,
                            symbol,
                            script,
                            character,
                            object.transform.x,
                            object.transform.y
                        );
                    }
                }
            }
        }
    }

    if selection.include_assets && !project.assets.is_empty() {
        let _ = writeln!(out, "\nAssets:");
        for asset in &project.assets {
            let _ = writeln!(out, "- {} ({:?}, {})", asset.name, asset.kind, asset.file);
        }
    }

    if selection.include_script_metadata {
        let _ = writeln!(out, "\nSkripte:");
        for script in &project.scripts {
            let _ = writeln!(
                out,
                "- {} [{}]{}{}",
                script.path,
                script.provenance.label(),
                if script.abc.is_some() {
                    format!(
                        ", ABC erhalten: {} Bytes (kein Quelltext)",
                        script.abc.as_ref().map_or(0, Vec::len)
                    )
                } else {
                    String::new()
                },
                script
                    .linked_object
                    .map(|id| format!(", verknüpft mit Objekt {}", id))
                    .unwrap_or_default()
            );
        }
    }

    if selection.include_script_sources {
        let selected: Vec<_> = project
            .scripts
            .iter()
            .filter(|script| selection.selected_scripts.contains(&script.id))
            .collect();
        if selected.is_empty() {
            let _ = writeln!(
                out,
                "\nHinweis: Es sind keine Skripte als Kontext ausgewählt. Quelltext wird nicht übertragen."
            );
        } else {
            let _ = writeln!(out, "\nAusgewählte Skripte:");
            for script in selected {
                let _ = writeln!(
                    out,
                    "\n--- {} ({}, {} Bytes) ---",
                    script.path,
                    script.provenance.label(),
                    script.source.len()
                );
                if script.abc.is_some() {
                    let _ = writeln!(
                        out,
                        "[Nur ABC-Bytecode vorhanden. Der folgende Text ist KEINE Originalquelle und darf nicht als solche bezeichnet werden.]"
                    );
                }
                let source: String = script.source.chars().take(MAX_SOURCE_CHARS).collect();
                let _ = writeln!(out, "{source}");
                if script.source.chars().count() > MAX_SOURCE_CHARS {
                    let _ = writeln!(out, "[Quelltext für den Kontext gekürzt]");
                }
            }
        }
    }

    if !selection.selected_code.trim().is_empty() {
        let _ = writeln!(
            out,
            "\nVom Benutzer ausgewählter Code:\n--- Auswahl ---\n{}\n--- Ende Auswahl ---",
            selection
                .selected_code
                .chars()
                .take(MAX_SOURCE_CHARS)
                .collect::<String>()
        );
    }

    if !selection.notes.trim().is_empty() {
        let _ = writeln!(
            out,
            "\nZusätzliche Hinweise des Benutzers:\n{}",
            selection.notes
        );
    }

    if selection.include_diagnostics && !diagnostics.is_empty() {
        let _ = writeln!(out, "\nCompiler-Diagnostik:");
        for diagnostic in diagnostics.iter().take(80) {
            let _ = writeln!(
                out,
                "- {}:{}:{} {:?}: {}",
                diagnostic.span.line,
                diagnostic.span.column,
                diagnostic.span.len(),
                diagnostic.severity,
                diagnostic.message
            );
        }
    }

    if out.chars().count() > MAX_CONTEXT_CHARS {
        let mut truncated: String = out.chars().take(MAX_CONTEXT_CHARS).collect();
        truncated.push_str("\n[Kontext wurde auf das Sicherheitslimit gekürzt.]");
        return truncated;
    }
    out
}

pub const SYSTEM_PROMPT: &str = r##"Du bist der integrierte Entwicklungsagent von NØVA Flash Studio.
Du hilfst bei ActionScript 3, dem NØVA-Projektmodell, Timeline/Animation und dem bestehenden Compiler/Exporter.

Regeln:
- Erfinde keine Funktionen, die NØVA nicht hat. Shell-Befehle, beliebige Prozesse und Dateizugriffe außerhalb des Projektordners sind nicht erlaubt.
- Preserved ABC ist Bytecode, kein Originalquelltext. Bezeichne rekonstruierten oder dekompilierten Code niemals als Original.
- Bevorzuge Änderungen an vorhandenen ActionScript-Dateien. Für jede Dateiänderung muss der Benutzer einen Diff sehen und ausdrücklich zustimmen.
- Projektoperationen ändern Szenen, Ebenen, Objekte, Symbole und Skripte im Dokument. Der Benutzer sieht vorher eine Vorschau und muss zustimmen. Nichts wird automatisch angewendet.
- Lösche keine Dateien, Szenen, Ebenen, Objekte oder Symbole ohne ausdrückliche Bestätigung. Die unterstützten Operationen legen Inhalte an, aktualisieren Objekt-/Timeline-Eigenschaften und ändern Bühnen-/Projektmaße; sie löschen nichts.
- Wenn du einen Build benötigst, setze "actions":["build"].
- Antworte auf Deutsch, knapp und konkret.

Antworte ausschließlich mit einem JSON-Objekt:
{
  "message": "Erklärung für den Benutzer",
  "changes": [
    {"path": "scripts/Main.as", "content": "vollständiger neuer Dateiinhalt", "reason": "kurze Begründung", "delete": false}
  ],
  "operations": [],
  "actions": []
}
Wenn keine Dateiänderung nötig ist, lass "changes" leer. "content" ist immer der vollständige Dateiinhalt, nicht nur ein Patch.
Unterstützte Projektoperationen (nur diese, jeweils mit "op"):
- {"op":"rename_project","name":"Neuer Name"}
- {"op":"set_stage","width":800,"height":600,"fps":30,"background":"#101018","scene":"optionaler Szenenname"}
- {"op":"add_scene","name":"Szene 2"}
- {"op":"add_layer","name":"Hintergrund","scene":"optional","frames":10}
- {"op":"add_object","name":"Boden","scene":"optional","layer":"optional","shape":"rect|ellipse|text","x":0,"y":320,"width":550,"height":80,"fill":"#334455","text":"nur bei text","font_size":24}
- {"op":"update_object","name":"Boden","scene":"optional","layer":"optional","x":10,"y":320,"rotation":0,"scale_x":1,"scale_y":1,"fill":"#445566","text":"optional nur bei Text","font_size":24}
- {"op":"add_keyframe","layer":"Hintergrund","frame":10,"scene":"optional","label":"optional","duplicate_previous":true}
- {"op":"set_frame_label","layer":"Hintergrund","frame":1,"label":"start","scene":"optional"}
- {"op":"add_tween","layer":"Hintergrund","frame":1,"scene":"optional","symbol":"optional","easing":"linear|ease_in|ease_out|ease_in_out","kind":"motion|shape"}
- {"op":"add_symbol","name":"Kreis","width":40,"height":40,"fill":"#00c8ff","corner_radius":20,"kind":"graphic|movie_clip|button"}
- {"op":"set_symbol_kind","symbol":"Kreis","kind":"movie_clip"}
- {"op":"place_symbol","symbol":"Kreis","name":"Kreis 1","x":100,"y":80,"scene":"optional","symbol_target":"optional","layer":"optional","fill":"optional"}
- {"op":"set_layer_kind","layer":"Tonspur","kind":"sound|mask|guide|normal","scene":"optional","symbol":"optional"}
- {"op":"import_bitmap_asset","name":"Hintergrund","path":"assets/hintergrund.png"}
- {"op":"import_audio_asset","name":"Musik","path":"assets/musik.wav"}
- {"op":"place_bitmap","asset":"Hintergrund","name":"BG","x":275,"y":200,"scene":"optional","symbol":"optional","layer":"optional"}
- {"op":"place_sound_cue","asset":"Musik","frame":1,"scene":"optional","symbol":"optional","layer":"optional","event":true}
- {"op":"add_script","path":"scripts/Neu.as","source":"vollständiger ActionScript-3-Quelltext"}
- {"op":"add_bone","symbol":"Held","name":"Oberarm","x":0,"y":0,"length":40,"parent":"optional Knochenname","rotation":0}
- {"op":"set_bone_parent","symbol":"Held","bone":"Unterarm","parent":"Oberarm"}
- {"op":"add_bone_clip","symbol":"Held","name":"Schwung","frames":24,"looping":true}
- {"op":"add_bone_key","symbol":"Held","clip":"Schwung","bone":"Oberarm","frame":0,"rotation":0}
- {"op":"bind_object_to_bone","symbol":"Held","object":"ArmSegment","bone":"Oberarm"}
- {"op":"set_symbol_clip","symbol":"Held","clip":"Schwung"}
Szene und Symbol-Timeline werden über Namen ausgewählt; "symbol" zielt auf eine Bibliotheks-Timeline, "scene" auf eine Szene, ohne Auswahl gilt die aktive Szene. Asset-Pfade sind relativ zum Projektordner (png/jpg/jpeg/gif, wav/mp3). Neue Skripte müssen unter scripts/ liegen und auf .as enden. Neue Skripte müssen unter scripts/ liegen und auf .as enden. Keine Lösch-, Umbenennungs- oder Überschreiboperationen für bestehende Inhalte erfinden. Die Operationen werden erst nach ausdrücklicher Bestätigung transaktional angewendet."##;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentReply {
    pub message: String,
    pub changes: Vec<ProposedChange>,
    pub operations: Vec<ProjectOperation>,
    /// Entries that looked like operations but could not be parsed. They are
    /// surfaced in the UI instead of being silently dropped.
    pub operation_errors: Vec<String>,
    pub actions: Vec<String>,
    pub raw: String,
}

impl AgentReply {
    pub fn wants_build(&self) -> bool {
        self.actions.iter().any(|action| action == "build")
    }
}

#[derive(Debug, Clone)]
pub struct AgentOutcome {
    pub provider_id: String,
    pub provider_name: String,
    pub model: String,
    pub cloud: bool,
    pub context_chars: usize,
    pub context: String,
    pub reply: AgentReply,
}

/// A request prepared on the UI thread and executable on a worker thread. The
/// key stays in this value only until the request finishes.
pub struct PreparedRequest {
    pub provider: crate::provider::ProviderConfig,
    pub key: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub context: String,
    pub context_chars: usize,
    pub cloud: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AgentSession {
    pub provider_override: Option<String>,
    pub cloud_consent: bool,
    pub messages: Vec<ChatMessage>,
    pub context: ContextSelection,
    pub pending_changes: Vec<ProposedChange>,
    pub pending_operations: Vec<ProjectOperation>,
    pub last_context: String,
}

impl AgentSession {
    pub fn clear(&mut self) {
        self.messages.clear();
        self.pending_changes.clear();
        self.pending_operations.clear();
        self.last_context.clear();
    }

    pub fn prepare(
        &self,
        manager: &ProviderManager,
        project: &Project,
        active_frame: usize,
        mode: &str,
        diagnostics: &[Diagnostic],
        user_message: &str,
    ) -> AiResult<PreparedRequest> {
        let provider = manager
            .selected(self.provider_override.as_deref())
            .ok_or(AiError::NoProvider)?
            .clone();
        if !provider.enabled {
            return Err(AiError::ProviderUnavailable(provider.name.clone()));
        }
        if provider.is_cloud() && !self.cloud_consent {
            return Err(AiError::CloudConsentRequired);
        }
        let key = manager.credential(&provider.id)?;
        if provider.needs_credential() && key.as_deref().is_none_or(str::is_empty) {
            return Err(AiError::MissingCredential(provider.name.clone()));
        }
        let context = build_context(project, &self.context, active_frame, mode, diagnostics);
        let mut messages = vec![ChatMessage::system(SYSTEM_PROMPT)];
        for message in self.messages.iter().rev().take(12).rev() {
            messages.push(message.clone());
        }
        messages.push(ChatMessage::user(format!(
            "Projektkontext:\n{context}\n\nAufgabe des Benutzers:\n{user_message}"
        )));
        Ok(PreparedRequest {
            cloud: provider.is_cloud(),
            provider,
            key,
            messages,
            context_chars: context.chars().count(),
            context,
        })
    }

    pub fn request(
        &self,
        manager: &ProviderManager,
        project: &Project,
        active_frame: usize,
        mode: &str,
        diagnostics: &[Diagnostic],
        user_message: &str,
    ) -> AiResult<AgentOutcome> {
        let prepared = self.prepare(
            manager,
            project,
            active_frame,
            mode,
            diagnostics,
            user_message,
        )?;
        execute_prepared(prepared, manager.transport())
    }
}

pub fn execute_prepared(
    prepared: PreparedRequest,
    transport: &dyn crate::http::HttpTransport,
) -> AiResult<AgentOutcome> {
    let ChatResponse { content, model } = crate::provider::chat(
        &prepared.provider,
        prepared.key.as_deref(),
        &prepared.messages,
        transport,
    )?;
    let reply = parse_agent_reply(&content);
    Ok(AgentOutcome {
        provider_id: prepared.provider.id,
        provider_name: prepared.provider.name,
        model,
        cloud: prepared.cloud,
        context_chars: prepared.context_chars,
        context: prepared.context,
        reply,
    })
}

pub fn parse_agent_reply(text: &str) -> AgentReply {
    let raw = text.to_string();
    let Some(json) = extract_json(text) else {
        return AgentReply {
            message: text.trim().to_string(),
            changes: Vec::new(),
            operations: Vec::new(),
            operation_errors: Vec::new(),
            actions: Vec::new(),
            raw,
        };
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&json) else {
        return AgentReply {
            message: text.trim().to_string(),
            changes: Vec::new(),
            operations: Vec::new(),
            operation_errors: Vec::new(),
            actions: Vec::new(),
            raw,
        };
    };
    let message = value
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or(text)
        .trim()
        .to_string();
    let actions = value
        .get("actions")
        .and_then(|actions| actions.as_array())
        .map(|actions| {
            actions
                .iter()
                .filter_map(|action| action.as_str())
                .filter(|action| action == &"build")
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut changes = Vec::new();
    for key in ["changes", "files", "edits"] {
        let Some(entries) = value.get(key).and_then(|entries| entries.as_array()) else {
            continue;
        };
        for entry in entries {
            let path = entry
                .get("path")
                .or_else(|| entry.get("file"))
                .and_then(|path| path.as_str())
                .unwrap_or("");
            if path.trim().is_empty() {
                continue;
            }
            let content = entry
                .get("content")
                .or_else(|| entry.get("new_content"))
                .or_else(|| entry.get("newContent"))
                .and_then(|content| content.as_str())
                .unwrap_or("");
            let reason = entry
                .get("reason")
                .and_then(|reason| reason.as_str())
                .unwrap_or("")
                .to_string();
            let delete = entry
                .get("delete")
                .and_then(|delete| delete.as_bool())
                .unwrap_or(false)
                || entry
                    .get("kind")
                    .and_then(|kind| kind.as_str())
                    .is_some_and(|kind| kind == "delete");
            changes.push(ProposedChange {
                path: path.to_string(),
                content: content.to_string(),
                reason,
                delete,
                kind: None,
            });
        }
    }
    let mut operations = Vec::new();
    let mut operation_errors = Vec::new();
    for key in ["operations", "project_operations", "projectOperations"] {
        let Some(entries) = value.get(key).and_then(|entries| entries.as_array()) else {
            continue;
        };
        for (index, entry) in entries.iter().enumerate() {
            if operations.len() >= MAX_OPERATIONS {
                operation_errors.push(format!(
                    "Projektoperation {} ignoriert: Maximum {MAX_OPERATIONS} pro Antwort.",
                    index + 1
                ));
                continue;
            }
            match serde_json::from_value::<ProjectOperation>(entry.clone()) {
                Ok(operation) => operations.push(operation),
                Err(error) => operation_errors.push(format!(
                    "Projektoperation {} konnte nicht gelesen werden: {error}",
                    index + 1
                )),
            }
        }
    }
    AgentReply {
        message,
        changes,
        operations,
        operation_errors,
        actions,
        raw,
    }
}

fn extract_json(text: &str) -> Option<String> {
    if let Some(start) = text.find("```") {
        let rest = &text[start + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest).trim_start();
        if let Some(end) = rest.find("```") {
            let candidate = rest[..end].trim();
            if candidate.starts_with('{') {
                return Some(candidate.to_string());
            }
        }
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| text[start..=end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{CredentialStore, MemoryCredentialStore};
    use crate::http::{HttpResponse, MockTransport};
    use crate::provider::{ProviderConfig, ProviderKind, ProviderSettings};
    use std::sync::Arc;

    fn session_for(
        provider: ProviderConfig,
        consent: bool,
    ) -> (AgentSession, ProviderManager, Arc<MockTransport>) {
        let mut provider = provider;
        provider.model = "test-model".into();
        let transport = Arc::new(MockTransport::new(vec![HttpResponse::json(
            200,
            br#"{"message":{"content":"{\"message\":\"Fertig\",\"changes\":[],\"actions\":[\"build\"]}"}}"#
                .to_vec(),
        )]));
        let credentials = Box::new(MemoryCredentialStore::new());
        if provider.needs_credential() {
            credentials.set(&provider.id, "secret").unwrap();
        }
        let mut settings = ProviderSettings::default();
        settings.default_provider = Some(provider.id.clone());
        settings.providers.push(provider);
        let manager = ProviderManager::with_parts(
            settings,
            credentials,
            transport.clone(),
            std::env::temp_dir().join("nova-ai-agent-test"),
        );
        let mut session = AgentSession::default();
        session.cloud_consent = consent;
        (session, manager, transport)
    }

    #[test]
    fn context_contains_structure_and_only_selected_source() {
        let project = Project::demo();
        let mut selection = ContextSelection::default();
        selection.selected_scripts = vec![project.scripts[0].id];
        let context = build_context(&project, &selection, 0, "Code", &[]);
        assert!(context.contains("Aktive Szene"));
        assert!(context.contains("scripts/Main.as"));
        assert!(context.contains("public class Main"));
        assert!(context.contains("Keyframes"));
    }

    #[test]
    fn unselected_source_is_not_included() {
        let project = Project::demo();
        let selection = ContextSelection::default();
        let context = build_context(&project, &selection, 0, "Code", &[]);
        assert!(!context.contains("public class Main"));
        assert!(context.contains("keine Skripte als Kontext"));
    }

    #[test]
    fn preserved_abc_is_described_as_bytecode_not_source() {
        let mut project = Project::demo();
        project.scripts[0].abc = Some(vec![1, 2, 3, 4]);
        project.scripts[0].provenance = nova_core::CodeProvenance::CompiledBytecode;
        project.scripts[0].source = "NOT ORIGINAL SOURCE".into();
        let mut selection = ContextSelection::default();
        selection.selected_scripts = vec![project.scripts[0].id];
        let context = build_context(&project, &selection, 0, "Code", &[]);
        assert!(context.contains("ABC erhalten"));
        assert!(context.contains("KEINE Originalquelle"));
        assert!(!context.contains("1, 2, 3, 4"));
    }

    #[test]
    fn parses_json_changes_and_build_action() {
        let reply = parse_agent_reply(
            r#"```json
            {"message":"Datei anpassen","changes":[{"path":"scripts/Main.as","content":"class Main {}\n","reason":"fix"}],"actions":["build","shell"]}
            ```"#,
        );
        assert_eq!(reply.message, "Datei anpassen");
        assert_eq!(reply.changes.len(), 1);
        assert_eq!(reply.changes[0].path, "scripts/Main.as");
        assert!(reply.wants_build());
        assert!(!reply.actions.iter().any(|action| action == "shell"));
    }

    #[test]
    fn parses_structured_operations_and_reports_bad_entries() {
        let reply = parse_agent_reply(
            r##"```json
            {"message":"Bauen","changes":[],"operations":[
              {"op":"add_layer","name":"Hintergrund","frames":8},
              {"op":"add_object","name":"Boden","layer":"Hintergrund","shape":"rect","x":0,"y":300,"width":550,"height":80,"fill":"#334455"},
              {"op":"does_not_exist","name":"x"}
            ],"actions":["build"]}
            ```"##,
        );
        assert_eq!(reply.operations.len(), 2);
        assert!(matches!(
            reply.operations[0],
            ProjectOperation::AddLayer { .. }
        ));
        assert_eq!(reply.operation_errors.len(), 1);
        assert!(reply.operation_errors[0].contains("Projektoperation 3"));
    }

    #[test]
    fn plain_text_reply_is_not_treated_as_a_file_change() {
        let reply = parse_agent_reply("Nur eine Erklärung ohne JSON.");
        assert!(reply.changes.is_empty());
        assert_eq!(reply.message, "Nur eine Erklärung ohne JSON.");
    }

    #[test]
    fn cloud_requests_require_explicit_consent() {
        let provider = ProviderConfig::new(ProviderKind::OllamaCloud);
        let (session, manager, _) = session_for(provider, false);
        let error = session
            .request(&manager, &Project::demo(), 0, "Code", &[], "Hallo")
            .unwrap_err();
        assert!(matches!(error, AiError::CloudConsentRequired));
    }

    #[test]
    fn local_requests_do_not_require_cloud_consent() {
        let provider = ProviderConfig::new(ProviderKind::OllamaLocal);
        let (session, manager, transport) = session_for(provider, false);
        let outcome = session
            .request(&manager, &Project::demo(), 0, "Code", &[], "Hallo")
            .unwrap();
        assert!(!outcome.cloud);
        assert!(outcome.reply.wants_build());
        assert_eq!(transport.request_count(), 1);
    }

    #[test]
    fn per_session_provider_override_wins_over_default() {
        let mut local = ProviderConfig::new(ProviderKind::OllamaLocal);
        local.model = "local-model".into();
        let mut cloud = ProviderConfig::new(ProviderKind::OllamaCloud);
        cloud.model = "cloud-model".into();
        let local_id = local.id.clone();
        let cloud_id = cloud.id.clone();
        let transport = Arc::new(MockTransport::new(vec![HttpResponse::json(
            200,
            br#"{"message":{"content":"ok"}}"#.to_vec(),
        )]));
        let credentials = Box::new(MemoryCredentialStore::new());
        credentials.set(&cloud_id, "secret").unwrap();
        let mut settings = ProviderSettings::default();
        settings.default_provider = Some(local_id);
        settings.providers = vec![local, cloud];
        let manager = ProviderManager::with_parts(
            settings,
            credentials,
            transport,
            std::env::temp_dir().join("nova-ai-agent-override"),
        );
        let mut session = AgentSession::default();
        session.provider_override = Some(cloud_id.clone());
        session.cloud_consent = true;
        let outcome = session
            .request(&manager, &Project::demo(), 0, "Code", &[], "Hallo")
            .unwrap();
        assert_eq!(outcome.provider_id, cloud_id);
        assert!(outcome.cloud);
    }
}
