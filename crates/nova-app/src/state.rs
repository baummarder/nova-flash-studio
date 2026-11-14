//! Application state that is independent of the immediate-mode UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use egui::Vec2;
use nova_as::Diagnostic;
use nova_core::{
    Easing, Frame, LayerKind, Project, ProjectTemplate, Rgba, ScriptId, Transform, TweenKind,
};
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
    Asset(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Line,
    Pencil,
    Brush,
    Eraser,
    Fill,
    PathEdit,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Auswahl",
            Self::Line => "Linie",
            Self::Pencil => "Bleistift",
            Self::Brush => "Pinsel",
            Self::Eraser => "Radierer",
            Self::Fill => "Füllen",
            Self::PathEdit => "Pfad",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Select => "Objekte auswählen, verschieben, skalieren und rotieren.",
            Self::Line => "Gerade Linie ziehen; Shift erzwingt 45°-Schritte.",
            Self::Pencil => "Dünnen Freihandstrich zeichnen.",
            Self::Brush => "Breiten Freihandstrich zeichnen.",
            Self::Eraser => "Objekt unter dem Zeiger entfernen.",
            Self::Fill => "Farbe des getroffenen Objekts ändern.",
            Self::PathEdit => "Pfadknoten des ausgewählten Objekts bearbeiten.",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DraftStroke {
    pub tool: Tool,
    pub points: Vec<egui::Pos2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransformMode {
    ScaleTopLeft,
    ScaleTopRight,
    ScaleBottomLeft,
    ScaleBottomRight,
    Rotate,
}

#[derive(Debug, Clone)]
pub struct TransformDrag {
    pub mode: TransformMode,
    pub start_pointer: egui::Pos2,
    pub centroid: egui::Pos2,
    pub start_angle: f32,
    pub start_transforms: Vec<(Uuid, Uuid, Transform)>,
    pub start_scale: (f32, f32),
}

/// Path-node drag for the Path edit tool: which object, which sub-path,
/// which command and which point slot (0 = move-to/line-to point,
/// 1..=2 = curve control/end for `CurveTo`).
#[derive(Debug, Clone, Copy)]
pub struct NodeDrag {
    pub object: Uuid,
    pub layer: Uuid,
    pub path: usize,
    pub command: usize,
    pub slot: usize,
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
        template: ProjectTemplate,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignMode {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
    DistributeHorizontal,
    DistributeVertical,
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
    RestartPlayback,
    Step(i32),
    FirstFrame,
    LastFrame,
    AddLayer,
    AddFolder,
    AddScript,
    AddSymbol,
    AddMovieClipSymbol,
    AddRect,
    AddEllipse,
    AddText,
    AddSymbolInstance(Uuid),
    AddBitmapAsset(Uuid),
    AddSoundCue(Uuid),
    PreviewAsset(Uuid),
    ImportBitmap,
    ImportAudio,
    EnterSymbol(Uuid),
    ExitSymbol,
    DuplicateSymbol(Uuid),
    RenameSymbol(Uuid, String),
    SetTool(Tool),
    SetLayerKind(LayerKind),
    SetTweenKind(TweenKind),
    AddScene,
    DeleteScene,
    InsertKeyframe,
    InsertBlankKeyframe,
    RemoveKeyframe,
    DeleteSelection,
    CopyObjects,
    PasteObjects,
    DuplicateObjects,
    AlignSelection(AlignMode),
    CheckScript(ScriptId),
    OpenScript(ScriptId),
    FitStage,
    ZoomBy(f32),
    ToggleOnion,
    ToggleGrid,
    ToggleSnap,
    ToggleFind,
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
    pub snap_to_grid: bool,
    pub grid_size: f32,
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
    pub object_clipboard: Vec<nova_core::ObjectInstance>,
    pub find_open: bool,
    pub find_project: bool,
    pub find_query: String,
    pub find_replace: String,
    pub find_case_sensitive: bool,
    pub find_offset: usize,
    pub autosave_enabled: bool,
    pub autosave_seconds: u64,
    pub last_autosave: Instant,
    pub last_autosave_path: Option<PathBuf>,
    /// `Some(id)` while a library symbol timeline is the editing target.
    pub edit_symbol: Option<Uuid>,
    pub tool: Tool,
    pub draft: Option<DraftStroke>,
    pub transform_drag: Option<TransformDrag>,
    pub node_drag: Option<NodeDrag>,
    pub asset_textures: HashMap<Uuid, egui::TextureHandle>,
    pub selected_asset: Option<Uuid>,
    pub selected_sound_cue: Option<(Uuid, usize)>,
    /// Live game-preview runtime while the timeline plays.
    pub game_preview: Option<nova_core::game::runtime::GameRuntime>,
    /// Master volume for preview audio, 0.0..=1.0.
    pub preview_volume: f32,
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
            snap_to_grid: false,
            grid_size: 20.0,
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
            object_clipboard: Vec::new(),
            find_open: false,
            find_project: false,
            find_query: String::new(),
            find_replace: String::new(),
            find_case_sensitive: false,
            find_offset: 0,
            autosave_enabled: true,
            autosave_seconds: 60,
            last_autosave: Instant::now(),
            last_autosave_path: None,
            edit_symbol: None,
            tool: Tool::Select,
            draft: None,
            transform_drag: None,
            node_drag: None,
            asset_textures: HashMap::new(),
            selected_asset: project.assets.first().map(|asset| asset.id),
            selected_sound_cue: None,
            game_preview: None,
            preview_volume: 1.0,
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

    /// The timeline currently targeted by the stage and timeline panels.
    pub fn timeline<'a>(&self, project: &'a Project) -> Option<&'a nova_core::Scene> {
        project.edit_scene(self.edit_symbol)
    }

    pub fn enter_symbol(&mut self, project: &Project, id: Uuid) {
        if project.symbol(id).is_some() {
            self.edit_symbol = Some(id);
            self.selection = Selection::None;
            self.frame = 0;
            self.selected_keyframes.clear();
            self.drag_cleanup();
            self.clamp_frame(project);
        }
    }

    pub fn exit_symbol(&mut self, project: &Project) {
        self.edit_symbol = None;
        self.selection = Selection::None;
        self.frame = 0;
        self.selected_keyframes.clear();
        self.drag_cleanup();
        self.clamp_frame(project);
    }

    pub fn drag_cleanup(&mut self) {
        self.dragging = None;
        self.marquee = None;
        self.draft = None;
        self.transform_drag = None;
        self.node_drag = None;
    }

    /// Clamps the playhead to the active scene or symbol timeline.
    pub fn clamp_frame(&mut self, project: &Project) {
        let max = project
            .edit_scene(self.edit_symbol)
            .map(|scene| scene.frame_count().saturating_sub(1))
            .unwrap_or(0);
        self.frame = self.frame.min(max);
    }

    pub fn reset_view(&mut self, project: &Project) {
        self.zoom_initialized = false;
        self.zoom = 1.0;
        if let Some(scene) = project.edit_scene(self.edit_symbol) {
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
        let scene = project.edit_scene(self.edit_symbol)?;
        nova_core::resolve_scene(scene, self.frame)
            .into_iter()
            .find(|object| object.instance.id == id)
    }

    pub fn asset_texture(
        &mut self,
        ctx: &egui::Context,
        asset: &nova_core::Asset,
    ) -> Option<&egui::TextureHandle> {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            self.asset_textures.entry(asset.id)
        {
            let image = image::load_from_memory(&asset.data).ok()?.to_rgba8();
            let size = [image.width() as usize, image.height() as usize];
            let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
            entry.insert(ctx.load_texture(
                format!("asset-{}", asset.id),
                color,
                egui::TextureOptions::LINEAR,
            ));
        }
        self.asset_textures.get(&asset.id)
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
