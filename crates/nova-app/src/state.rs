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
    Tiles,
    Console,
    Problems,
}

/// Painting tool inside the tilemap editor (bottom "Kacheln" tab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TilemapTool {
    #[default]
    Pencil,
    Eraser,
    Rect,
    Flood,
    Eyedrop,
    Replace,
    /// Region select / move (M11.1).
    Select,
}

impl TilemapTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pencil => "Stift",
            Self::Eraser => "Radierer",
            Self::Rect => "Rechteck",
            Self::Flood => "Füllen",
            Self::Eyedrop => "Pipette",
            Self::Replace => "Ersetzen",
            Self::Select => "Auswahl",
        }
    }
}

/// State of the tilemap editor panel.
#[derive(Debug, Clone)]
pub struct TilemapEditor {
    /// (layer id, object id) of the tilemap being edited.
    pub object: Option<(Uuid, Uuid)>,
    /// Selected tile as a cell value (0 = empty, else tileset index + 1).
    pub tile: u16,
    /// Brush stamp edge length in cells (1 = single tile).
    pub brush: u32,
    pub tool: TilemapTool,
    /// Active tile layer index.
    pub layer: usize,
    /// Drag start for the rectangle tool.
    pub stroke_start: Option<(i32, i32)>,
    /// Tile captured by the replace/eyedrop tools.
    pub replace_from: u16,
    // ------------------------------------------------ M11.1 canvas state
    /// Canvas zoom in px per tile unit; 0 = auto-fit (recomputed per frame
    /// until the user zooms or pans explicitly).
    pub zoom: f32,
    /// Screen-space pan offset added to the fitted origin (pixels).
    pub pan: egui::Vec2,
    /// Whether the user has touched zoom/pan (disables auto-fit).
    pub view_custom: bool,
    /// Grid lines visibility.
    pub show_grid: bool,
    /// Grid line opacity 0.0..=1.0.
    pub grid_opacity: f32,
    /// Solid-tile collision overlay.
    pub show_collision: bool,
    /// Normalized cell selection (x0,y0,x1,y1 inclusive) on the active layer.
    pub selection: Option<(i32, i32, i32, i32)>,
    /// Drag start while the Select tool creates a selection.
    pub select_start: Option<(i32, i32)>,
    /// Move drag: (grab cell, current offset in cells).
    pub move_drag: Option<((i32, i32), (i32, i32))>,
    /// Copied tile region (single layer).
    pub clipboard: Option<nova_core::tilemap::TileRegion>,
    /// Mirror the brush stamp / pasted region horizontally.
    pub flip_h: bool,
    /// Mirror the brush stamp / pasted region vertically.
    pub flip_v: bool,
    /// Next canvas click places the clipboard (paste armed).
    pub paste_armed: bool,
    /// Last hovered cell (for paste placement and brush preview).
    pub cursor_cell: Option<(i32, i32)>,
    /// Tileset slicing dialog open flag.
    pub tileset_dialog: bool,
}

/// State of the tileset slicing dialog (bitmap -> Tileset).
#[derive(Debug, Clone)]
pub struct TilesetDialog {
    /// Selected bitmap asset id.
    pub asset: Option<Uuid>,
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    pub margin_x: u32,
    pub margin_y: u32,
    pub spacing_x: u32,
    pub spacing_y: u32,
    /// Per-tile solid flags for the preview/create step.
    pub solid: Vec<bool>,
    /// Optional name prefix; tiles are named "{prefix} {n}" when set.
    pub name_prefix: String,
}

impl Default for TilesetDialog {
    fn default() -> Self {
        Self {
            asset: None,
            name: String::new(),
            tile_width: 16,
            tile_height: 16,
            margin_x: 0,
            margin_y: 0,
            spacing_x: 0,
            spacing_y: 0,
            solid: Vec::new(),
            name_prefix: String::new(),
        }
    }
}

impl Default for TilemapEditor {
    fn default() -> Self {
        Self {
            object: None,
            tile: 1,
            brush: 1,
            tool: TilemapTool::Pencil,
            layer: 0,
            stroke_start: None,
            replace_from: 0,
            zoom: 0.0,
            pan: egui::Vec2::ZERO,
            view_custom: false,
            show_grid: true,
            grid_opacity: 0.35,
            show_collision: false,
            selection: None,
            select_start: None,
            move_drag: None,
            clipboard: None,
            flip_h: false,
            flip_v: false,
            paste_armed: false,
            cursor_cell: None,
            tileset_dialog: false,
        }
    }
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
    Pen,
    /// Paints vertex bone weights on skinned path objects (M17).
    SkinPaint,
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
            Self::Pen => "Stift",
            Self::SkinPaint => "Gewichte",
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
            Self::Pen => {
                "Anker setzen: Klick = Ecke, Ziehen = Kurve. Enter/Doppelklick = fertig, Klick auf Startpunkt = schließen, Esc = abbrechen."
            }
            Self::SkinPaint => {
                "Knochengewicht auf Pfadpunkte malen (Linke Maustaste = hinzufügen, Shift = entfernen). Knochen und Radius in den Eigenschaften wählen."
            }
        }
    }
}

/// One anchor of an in-progress pen path (stage coordinates).
#[derive(Debug, Clone, Copy)]
pub struct PenAnchor {
    pub pos: egui::Pos2,
    /// Quadratic control point for the segment ending at this anchor.
    pub handle: Option<egui::Pos2>,
}

/// In-progress pen path: anchors in stage space, committed as a Path object.
#[derive(Debug, Clone, Default)]
pub struct PenDraft {
    pub anchors: Vec<PenAnchor>,
    /// Stage position where the current press started (for curve drags).
    pub press: Option<egui::Pos2>,
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
    /// Pen tool draft: anchors in stage coordinates. `handle` is the
    /// quadratic control point for the segment ending at this anchor.
    pub pen: Option<PenDraft>,
    /// Bone-joint drag while a rigged symbol timeline is being edited.
    pub bone_drag: Option<Uuid>,
    /// Bone selected for weight painting (SkinPaint tool).
    pub skin_bone: Option<Uuid>,
    /// Weight brush radius in screen pixels.
    pub skin_radius: f32,
    pub asset_textures: HashMap<Uuid, egui::TextureHandle>,
    pub selected_asset: Option<Uuid>,
    pub selected_sound_cue: Option<(Uuid, usize)>,
    /// Live game-preview runtime while the timeline plays.
    pub game_preview: Option<nova_core::game::runtime::GameRuntime>,
    /// Master volume for preview audio, 0.0..=1.0.
    pub preview_volume: f32,
    /// Tilemap editor state (bottom "Kacheln" tab).
    pub tilemap: TilemapEditor,
    /// Tileset slicing dialog state.
    pub tileset_dialog: TilesetDialog,
    /// Edit-mode particle preview systems: object id -> (spec, system, origin).
    pub edit_particles: std::collections::HashMap<
        Uuid,
        (
            nova_core::model::ParticleEmitter,
            nova_core::particles::ParticleSystem,
            (f32, f32),
        ),
    >,
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
            pen: None,
            bone_drag: None,
            skin_bone: None,
            skin_radius: 24.0,
            asset_textures: HashMap::new(),
            selected_asset: project.assets.first().map(|asset| asset.id),
            selected_sound_cue: None,
            game_preview: None,
            preview_volume: 1.0,
            tilemap: TilemapEditor::default(),
            tileset_dialog: TilesetDialog::default(),
            edit_particles: std::collections::HashMap::new(),
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
        self.pen = None;
        self.bone_drag = None;
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
