//! Application state that is independent of the immediate-mode UI.

use std::path::PathBuf;
use std::time::Instant;

use egui::Vec2;
use nova_as::Diagnostic;
use nova_core::{Easing, Frame, Project, Rgba, ScriptId, Transform};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Code,
    Animation,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Code => "Code",
            Self::Animation => "Animation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    Timeline,
    Console,
    Problems,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    None,
    Scene,
    Layer(Uuid),
    Objects(Vec<Uuid>),
    Symbol(Uuid),
}

impl Selection {
    pub fn object(id: Uuid) -> Self {
        Self::Objects(vec![id])
    }

    pub fn object_ids(&self) -> &[Uuid] {
        match self {
            Self::Objects(ids) => ids,
            _ => &[],
        }
    }

    pub fn contains_object(&self, id: Uuid) -> bool {
        self.object_ids().contains(&id)
    }
}

/// A keyframe selected in the timeline. The frame is a layer-local index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyframeRef {
    pub layer: Uuid,
    pub frame: usize,
}

/// One keyframe copied into the internal clipboard, with its offset from the
/// first copied frame.
#[derive(Debug, Clone)]
pub struct ClipboardKeyframe {
    pub offset: usize,
    pub frame: Frame,
}

#[derive(Debug, Clone, Default)]
pub struct KeyframeClipboard {
    pub frames: Vec<ClipboardKeyframe>,
    pub source_layer: Option<Uuid>,
}

/// Snapshot of the transforms at the start of a stage drag. Keeping the start
/// values avoids accumulating floating point drift over many frames.
#[derive(Debug, Clone)]
pub struct ObjectDragStart {
    pub frame: usize,
    pub objects: Vec<(Uuid, Uuid, Transform)>,
}

/// A point-to-point marquee selection in stage coordinates.
#[derive(Debug, Clone)]
pub struct Marquee {
    pub start: egui::Pos2,
    pub current: egui::Pos2,
    pub base: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Echo,
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub level: LogLevel,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    New,
    /// Open the file picker before loading anything.
    OpenPrompt,
    Quit,
}

#[derive(Debug, Clone)]
pub enum Dialog {
    None,
    NewProject {
        name: String,
    },
    Unsaved {
        next: PendingAction,
    },
    About,
    Help,
    Error {
        title: String,
        message: String,
    },
    /// Import/export compatibility report. `is_error` only changes the
    /// emphasis; the report text is always shown verbatim.
    CompatibilityReport {
        title: String,
        report: String,
        is_error: bool,
    },
    ExportDone {
        path: PathBuf,
        bytes: usize,
        frames: usize,
        warnings: Vec<String>,
    },
    RuntimeSetup {
        adobe_path: String,
        test_result: Option<crate::flash::ValidationRecord>,
        error: Option<String>,
    },
    FlashSettings {
        default_runtime: crate::flash::RuntimeKind,
        adobe_path: String,
        ruffle_path: String,
        first_launch_setup_done: bool,
        last_validation: Option<crate::flash::ValidationRecord>,
        test_result: Option<crate::flash::ValidationRecord>,
        error: Option<String>,
    },
    AiProviders,
    AutosaveRecovery {
        path: PathBuf,
        recovery: crate::autosave::Recovery,
    },
}

#[derive(Debug, Clone)]
pub enum UiAction {
    NewProjectPrompt,
    OpenPrompt,
    Save,
    SaveAs,
    Export,
    OpenInFlashPlayer,
    OpenFlashSettings,
    OpenAiEditor,
    OpenAiProviders,
    Quit,
    About,
    Help,
    SetMode(Mode),
    SetBottomTab(BottomTab),
    TogglePlay,
    Stop,
    Step(i32),
    FirstFrame,
    LastFrame,
    AddLayer,
    AddScript,
    AddSymbol,
    AddRect,
    AddEllipse,
    AddSymbolInstance(Uuid),
    InsertKeyframe,
    InsertBlankKeyframe,
    RemoveKeyframe,
    DeleteSelection,
    CheckScript(ScriptId),
    OpenScript(ScriptId),
    FitStage,
    ZoomBy(f32),
    ToggleOnion,
    ToggleGrid,
    SelectScene(Uuid),
    JumpToFrame(usize),
    Deselect,
    Undo,
    Redo,
    CreateTween,
    RemoveTween,
    SetTweenEasing(Easing),
    CopyKeyframes,
    PasteKeyframes,
    DuplicateKeyframes,
    InsertFrame,
    RemoveFrame,
    MoveKeyframesToLayer(i32),
    ToggleAutosave,
    SetAutosaveInterval(u64),
}

pub struct AppState {
    pub mode: Mode,
    pub selection: Selection,
    pub active_script: Option<ScriptId>,
    pub diagnostics: Vec<Diagnostic>,
    pub diagnostics_for: Option<ScriptId>,
    pub frame: usize,
    pub playing: bool,
    pub loop_playback: bool,
    pub onion_skin: bool,
    pub show_grid: bool,
    pub zoom: f32,
    pub camera_center: egui::Pos2,
    pub zoom_initialized: bool,
    pub last_stage_size: Vec2,
    pub frame_width: f32,
    pub bottom_tab: BottomTab,
    pub console: Vec<LogLine>,
    pub console_input: String,
    pub dirty: bool,
    pub path: Option<PathBuf>,
    pub dialog: Dialog,
    pub pending_jump: Option<(usize, usize)>,
    pub new_object_color: Rgba,
    pub selected_symbol: Option<Uuid>,
    pub status: String,
    pub dragging: Option<ObjectDragStart>,
    pub marquee: Option<Marquee>,
    pub play_accumulator: f32,
    pub last_export: Option<(PathBuf, usize, usize, Vec<String>)>,
    pub auto_check: bool,
    pub selected_keyframes: Vec<KeyframeRef>,
    pub frame_range: Option<(usize, usize)>,
    pub keyframe_clipboard: Option<KeyframeClipboard>,
    pub autosave_enabled: bool,
    pub autosave_seconds: u64,
    pub last_autosave: Instant,
    pub last_autosave_path: Option<PathBuf>,
}

impl AppState {
    pub fn new(project: &Project) -> Self {
        let active_script = project.scripts.first().map(|script| script.id);
        let scene = project.active_scene();
        let camera_center = scene
            .map(|scene| egui::pos2(scene.width / 2.0, scene.height / 2.0))
            .unwrap_or(egui::pos2(275.0, 200.0));
        let mut state = Self {
            mode: Mode::Animation,
            selection: Selection::None,
            active_script,
            diagnostics: Vec::new(),
            diagnostics_for: None,
            frame: 0,
            playing: false,
            loop_playback: true,
            onion_skin: false,
            show_grid: true,
            zoom: 1.0,
            camera_center,
            zoom_initialized: false,
            last_stage_size: Vec2::ZERO,
            frame_width: 18.0,
            bottom_tab: BottomTab::Timeline,
            console: Vec::new(),
            console_input: String::new(),
            dirty: false,
            path: None,
            dialog: Dialog::None,
            pending_jump: None,
            new_object_color: Rgba::rgb(245, 166, 35),
            selected_symbol: project.symbols.first().map(|symbol| symbol.id),
            status: "Bereit".to_string(),
            dragging: None,
            marquee: None,
            play_accumulator: 0.0,
            last_export: None,
            auto_check: true,
            selected_keyframes: Vec::new(),
            frame_range: None,
            keyframe_clipboard: None,
            autosave_enabled: true,
            autosave_seconds: 60,
            last_autosave: Instant::now(),
            last_autosave_path: None,
        };
        state.log(
            LogLevel::Info,
            "NØVA Flash Studio gestartet. Demo-Projekt geladen.",
        );
        state
    }

    pub fn log(&mut self, level: LogLevel, text: impl Into<String>) {
        self.console.push(LogLine {
            level,
            text: text.into(),
        });
        if self.console.len() > 500 {
            self.console.drain(0..self.console.len() - 500);
        }
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    /// Clamps the playhead to the active scene.
    pub fn clamp_frame(&mut self, project: &Project) {
        let max = project
            .active_scene()
            .map(|scene| scene.frame_count().saturating_sub(1))
            .unwrap_or(0);
        self.frame = self.frame.min(max);
    }

    pub fn reset_view(&mut self, project: &Project) {
        self.zoom_initialized = false;
        self.zoom = 1.0;
        if let Some(scene) = project.active_scene() {
            self.camera_center = egui::pos2(scene.width / 2.0, scene.height / 2.0);
        }
        self.frame = 0;
        self.playing = false;
        self.play_accumulator = 0.0;
        self.selection = Selection::None;
        self.dragging = None;
        self.marquee = None;
        self.selected_keyframes.clear();
        self.frame_range = None;
    }

    /// Resolves the object with `id` visible at the current frame.
    pub fn resolved_object<'a>(
        &self,
        project: &'a Project,
        id: Uuid,
    ) -> Option<nova_core::ResolvedObject> {
        let scene = project.active_scene()?;
        nova_core::resolve_scene(scene, self.frame)
            .into_iter()
            .find(|object| object.instance.id == id)
    }

    /// The layer that currently owns the selected object.
    pub fn layer_of_object(&self, project: &Project, id: Uuid) -> Option<Uuid> {
        self.resolved_object(project, id)
            .map(|object| object.layer_id)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == nova_as::Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == nova_as::Severity::Warning)
            .count()
    }
}
