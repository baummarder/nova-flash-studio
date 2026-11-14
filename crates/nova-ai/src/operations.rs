//! Safe, validated project operations proposed by the AI agent.
//!
//! File diffs are deliberately separate from project edits. A diff changes a
//! script file on disk; an operation changes the in-memory `.nova` document
//! (scenes, layers, objects, symbols, scripts). Operations only add or update
//! content, never delete it, and are applied transactionally: either the whole
//! batch succeeds or the project is left untouched.
//!
//! The UI previews the batch before applying it. This module is UI-independent
//! and has no network access.

use nova_core::{
    Asset, AssetKind, Easing, LayerKind, ObjectInstance, Project, Rgba, Script, SoundCue, Symbol,
    SymbolKind, Tween,
};
use serde::{Deserialize, Serialize};

/// Upper bound for one agent-proposed batch. Keeps previews and apply work
/// bounded and prevents accidental project floods.
pub const MAX_OPERATIONS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObjectShape {
    #[default]
    Rect,
    Ellipse,
    Text,
}

impl ObjectShape {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rect => "Rechteck",
            Self::Ellipse => "Ellipse",
            Self::Text => "Text",
        }
    }
}

/// One structured edit to the project model.
///
/// The JSON representation is tagged with `op`, for example:
/// `{"op":"add_layer","name":"Hintergrund"}`. Scene and layer selectors are
/// names; omitted scene selectors use the active scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProjectOperation {
    RenameProject {
        name: String,
    },
    SetStage {
        width: f32,
        height: f32,
        #[serde(default)]
        fps: Option<f32>,
        #[serde(default)]
        background: Option<String>,
        #[serde(default)]
        scene: Option<String>,
    },
    AddScene {
        name: String,
    },
    AddLayer {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        /// When set, the layer is added to this library symbol's timeline
        /// instead of a scene.
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        frames: Option<usize>,
    },
    AddObject {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        shape: ObjectShape,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        text: String,
        #[serde(default)]
        font_size: Option<f32>,
    },
    UpdateObject {
        name: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default)]
        rotation: Option<f32>,
        #[serde(default)]
        scale_x: Option<f32>,
        #[serde(default)]
        scale_y: Option<f32>,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        font_size: Option<f32>,
    },
    AddKeyframe {
        layer: String,
        /// 1-based frame number, like the timeline UI.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        label: Option<String>,
        /// Copy the objects of the preceding keyframe into the new one. This
        /// is what makes a following `add_tween` useful.
        #[serde(default)]
        duplicate_previous: bool,
    },
    SetFrameLabel {
        layer: String,
        /// 1-based frame number, like the timeline UI.
        frame: usize,
        label: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
    },
    AddTween {
        layer: String,
        /// 1-based frame number of the first keyframe.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        easing: Easing,
        #[serde(default)]
        kind: Option<nova_core::TweenKind>,
    },
    AddSymbol {
        name: String,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        corner_radius: Option<f32>,
        #[serde(default)]
        kind: Option<SymbolKind>,
    },
    SetSymbolKind {
        symbol: String,
        kind: SymbolKind,
    },
    PlaceSymbol {
        symbol: String,
        name: String,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        /// Destination timeline: a library symbol name. When set, the
        /// instance is placed inside that symbol's timeline.
        #[serde(default)]
        symbol_target: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        fill: Option<String>,
    },
    /// Registers a bitmap asset. `path` is resolved by the caller-provided
    /// workspace-confined reader; the operation never touches the filesystem
    /// itself.
    ImportBitmapAsset {
        name: String,
        path: String,
    },
    /// Registers an audio asset (WAV or MP3) from a workspace-relative path.
    ImportAudioAsset {
        name: String,
        path: String,
    },
    /// Places an embedded bitmap asset as an object on a timeline.
    PlaceBitmap {
        asset: String,
        name: String,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
    },
    /// Attaches a sound cue to a frame of a sound layer (created if needed).
    PlaceSoundCue {
        asset: String,
        /// 1-based frame number.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        event: bool,
    },
    SetLayerKind {
        layer: String,
        kind: LayerKind,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        symbol: Option<String>,
    },
    AddScript {
        path: String,
        source: String,
    },
    /// Sets or clears the game components (collision, physics) of a placed
    /// object. `clear` removes all game metadata; otherwise provided specs
    /// replace the matching component and `no_collision`/`no_physics` remove
    /// a single component.
    SetGameObject {
        object: String,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        collision: Option<nova_core::CollisionSpec>,
        #[serde(default)]
        no_collision: bool,
        #[serde(default)]
        physics: Option<nova_core::PhysicsSpec>,
        #[serde(default)]
        no_physics: bool,
        #[serde(default)]
        clear: bool,
    },
    /// Configures scene-level game settings (camera, input bindings).
    /// `clear` removes the whole game configuration; otherwise provided
    /// sub-configs replace the existing ones.
    SetSceneGame {
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        camera: Option<nova_core::CameraConfig>,
        #[serde(default)]
        input: Option<nova_core::InputBindings>,
        #[serde(default)]
        clear: bool,
    },
    /// Installs the reusable AS3 game-kit scripts (`game.Input`,
    /// `game.Collision`, `game.Camera`, `game.Random`, `game.Physics`).
    /// Existing scripts are never overwritten.
    InstallGameKit,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum OperationError {
    #[error("UngÃ¼ltige Projektoperation: {0}")]
    Invalid(String),
    #[error("Nicht unterstÃ¼tzte Projektoperation: {0}")]
    Unsupported(String),
}

impl OperationError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationPlan {
    /// Human-readable summaries in execution order.
    pub summaries: Vec<String>,
    /// Errors that would block the batch. Empty means the batch is applicable.
    pub errors: Vec<String>,
}

impl OperationPlan {
    pub fn is_applicable(&self) -> bool {
        self.errors.is_empty() && !self.summaries.is_empty()
    }
}

/// Validates a batch against a clone of the project without touching the
/// caller's project. This is what the preview pane renders.
pub fn preview_operations(project: &Project, operations: &[ProjectOperation]) -> OperationPlan {
    preview_operations_with(project, operations, &|_| {
        Err("Dateizugriff ist ohne Workspace nicht konfiguriert.".to_string())
    })
}

/// Like [`preview_operations`], but asset import operations resolve their
/// `path` through `read_asset`. The reader is expected to enforce workspace
/// confinement; this module never touches the filesystem itself.
pub fn preview_operations_with(
    project: &Project,
    operations: &[ProjectOperation],
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> OperationPlan {
    let mut preview = project.clone();
    let mut summaries = Vec::new();
    if operations.len() > MAX_OPERATIONS {
        return OperationPlan {
            summaries,
            errors: vec![format!(
                "Zu viele Projektoperationen: {} (Maximum {MAX_OPERATIONS}).",
                operations.len()
            )],
        };
    }
    for (index, operation) in operations.iter().enumerate() {
        match apply_one(&mut preview, operation, read_asset) {
            Ok(summary) => summaries.push(format!("{}. {summary}", index + 1)),
            Err(error) => {
                return OperationPlan {
                    summaries,
                    errors: vec![format!("Operation {}: {error}", index + 1)],
                };
            }
        }
    }
    if let Err(error) = preview.validate() {
        return OperationPlan {
            summaries,
            errors: vec![format!("Projektvalidierung: {error}")],
        };
    }
    OperationPlan {
        summaries,
        errors: Vec::new(),
    }
}

/// Applies a whole batch transactionally. On error the project is unchanged.
pub fn apply_operations(
    project: &mut Project,
    operations: &[ProjectOperation],
) -> Result<Vec<String>, OperationError> {
    apply_operations_with(project, operations, &|_| {
        Err("Dateizugriff ist ohne Workspace nicht konfiguriert.".to_string())
    })
}

/// Like [`apply_operations`], but asset import operations resolve `path`
/// through the workspace-confined `read_asset` callback.
pub fn apply_operations_with(
    project: &mut Project,
    operations: &[ProjectOperation],
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<Vec<String>, OperationError> {
    let plan = preview_operations_with(project, operations, read_asset);
    if !plan.errors.is_empty() {
        return Err(OperationError::Invalid(plan.errors.join(" ")));
    }
    let mut next = project.clone();
    let mut summaries = Vec::new();
    for operation in operations {
        summaries.push(apply_one(&mut next, operation, read_asset)?);
    }
    next.validate()
        .map_err(|error| OperationError::Invalid(error.to_string()))?;
    *project = next;
    Ok(summaries)
}

fn apply_one(
    project: &mut Project,
    operation: &ProjectOperation,
    read_asset: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<String, OperationError> {
    match operation {
        ProjectOperation::RenameProject { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Projektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            project.metadata.name = name.to_string();
            project.touch();
            Ok(format!("Projekt umbenennen in '{name}'"))
        }
        ProjectOperation::SetStage {
            width,
            height,
            fps,
            background,
            scene,
        } => {
            check_size(*width, *height, "BÃ¼hne")?;
            if let Some(fps) = fps {
                if !fps.is_finite() || !(1.0..=120.0).contains(fps) {
                    return Err(OperationError::invalid(
                        "Die Bildrate muss zwischen 1 und 120 liegen.",
                    ));
                }
            }
            let background = background.as_deref().map(parse_color).transpose()?;
            let scene = scene_mut(project, scene.as_deref())?;
            scene.width = *width;
            scene.height = *height;
            if let Some(fps) = fps {
                scene.fps = *fps;
            }
            if let Some(background) = background {
                scene.background = background;
            }
            let name = scene.name.clone();
            project.touch();
            Ok(format!("BÃ¼hne '{name}' auf {width}Ã—{height} setzen"))
        }
        ProjectOperation::AddScene { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Szenenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if project.scenes.iter().any(|scene| scene.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Szene namens '{name}' existiert bereits."
                )));
            }
            if project.scenes.len() >= 256 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann hÃ¶chstens 256 Szenen enthalten.",
                ));
            }
            project.add_scene(name);
            Ok(format!("Szene '{name}' hinzufÃ¼gen"))
        }
        ProjectOperation::AddLayer {
            name,
            scene,
            symbol,
            frames,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Ebenenname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            let frames = frames.unwrap_or(1);
            if !(1..=100_000).contains(&frames) {
                return Err(OperationError::invalid(
                    "Eine neue Ebene muss 1 bis 100000 Frames haben.",
                ));
            }
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            if scene.layers.iter().any(|layer| layer.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Ebene namens '{name}' existiert in dieser Timeline bereits."
                )));
            }
            if scene.layers.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Eine Timeline kann hÃ¶chstens 1000 Ebenen enthalten.",
                ));
            }
            let scene_name = scene.name.clone();
            scene.layers.push(nova_core::Layer::new(name, frames));
            project.touch();
            Ok(format!("Ebene '{name}' in '{scene_name}' hinzufÃ¼gen"))
        }
        ProjectOperation::AddObject {
            name,
            scene,
            symbol,
            layer,
            shape,
            x,
            y,
            width,
            height,
            fill,
            text,
            font_size,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Objektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            check_size(*width, *height, "Objekt")?;
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Objektposition muss eine endliche Zahl sein.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or_else(|| match shape {
                    ObjectShape::Text => Rgba::WHITE,
                    _ => Rgba::rgb(78, 161, 255),
                });
            let object = match shape {
                ObjectShape::Rect => ObjectInstance::rect(name, *x, *y, *width, *height, fill),
                ObjectShape::Ellipse => {
                    ObjectInstance::ellipse(name, *x, *y, *width, *height, fill)
                }
                ObjectShape::Text => {
                    if text.trim().is_empty() {
                        return Err(OperationError::invalid(
                            "Ein Textobjekt benÃ¶tigt nichtleeren Text.",
                        ));
                    }
                    let size = font_size.unwrap_or(24.0);
                    if !size.is_finite() || !(1.0..=512.0).contains(&size) {
                        return Err(OperationError::invalid(
                            "Die TextgrÃ¶ÃŸe muss zwischen 1 und 512 liegen.",
                        ));
                    }
                    ObjectInstance::text(name, *x, *y, text, size, fill)
                }
            };
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "{} '{name}' auf Ebene '{layer_name}' hinzufÃ¼gen",
                shape.label()
            ))
        }
        ProjectOperation::UpdateObject {
            name,
            scene,
            layer,
            x,
            y,
            rotation,
            scale_x,
            scale_y,
            fill,
            text,
            font_size,
        } => {
            if x.is_none()
                && y.is_none()
                && rotation.is_none()
                && scale_x.is_none()
                && scale_y.is_none()
                && fill.is_none()
                && text.is_none()
                && font_size.is_none()
            {
                return Err(OperationError::invalid(
                    "UpdateObject benÃ¶tigt mindestens eine zu Ã¤ndernde Eigenschaft.",
                ));
            }
            for (value, label) in [(x, "x"), (y, "y"), (rotation, "rotation")] {
                if let Some(value) = value {
                    if !value.is_finite() {
                        return Err(OperationError::invalid(format!(
                            "Objekt-Eigenschaft '{label}' muss eine endliche Zahl sein."
                        )));
                    }
                }
            }
            for (value, label) in [(scale_x, "scale_x"), (scale_y, "scale_y")] {
                if let Some(value) = value {
                    if !value.is_finite() || *value <= 0.0 || *value > 1_000.0 {
                        return Err(OperationError::invalid(format!(
                            "Objekt-Eigenschaft '{label}' muss zwischen 0 und 1000 liegen."
                        )));
                    }
                }
            }
            if let Some(size) = font_size {
                if !size.is_finite() || !(1.0..=512.0).contains(size) {
                    return Err(OperationError::invalid(
                        "Die TextgrÃ¶ÃŸe muss zwischen 1 und 512 liegen.",
                    ));
                }
            }
            let fill = fill.as_deref().map(parse_color).transpose()?;
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let mut matches = Vec::new();
            for (layer_index, candidate) in scene.layers.iter().enumerate() {
                if let Some(selector) = layer.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                    if candidate.name != selector {
                        continue;
                    }
                }
                for (frame_index, frame) in candidate.frames.iter().enumerate() {
                    for (object_index, object) in frame.objects.iter().enumerate() {
                        if object.name == name.trim() {
                            matches.push((layer_index, frame_index, object_index));
                        }
                    }
                }
            }
            match matches.len() {
                0 => {
                    return Err(OperationError::invalid(format!(
                        "Objekt '{name}' existiert in Szene '{scene_name}' nicht."
                    )));
                }
                1 => {}
                count => {
                    return Err(OperationError::invalid(format!(
                        "Objektname '{name}' ist {count}-mal vorhanden. Gib eine Ebene an, um das Ziel eindeutig zu wÃ¤hlen."
                    )));
                }
            }
            let (layer_index, frame_index, object_index) = matches[0];
            let object = &mut scene.layers[layer_index].frames[frame_index].objects[object_index];
            if (text.is_some() || font_size.is_some())
                && !matches!(object.kind, nova_core::ObjectKind::Text(_))
            {
                return Err(OperationError::invalid(format!(
                    "Objekt '{name}' ist kein Textobjekt."
                )));
            }
            if let Some(x) = x {
                object.transform.x = *x;
            }
            if let Some(y) = y {
                object.transform.y = *y;
            }
            if let Some(rotation) = rotation {
                object.transform.rotation = *rotation;
            }
            if let Some(scale_x) = scale_x {
                object.transform.scale_x = *scale_x;
            }
            if let Some(scale_y) = scale_y {
                object.transform.scale_y = *scale_y;
            }
            if let Some(fill) = fill {
                object.fill = fill;
            }
            if let Some(text) = text {
                if let nova_core::ObjectKind::Text(text_object) = &mut object.kind {
                    text_object.text = text.clone();
                }
            }
            if let Some(size) = font_size {
                if let nova_core::ObjectKind::Text(text_object) = &mut object.kind {
                    text_object.size = *size;
                }
            }
            project.touch();
            Ok(format!(
                "Objekt '{name}' in Szene '{scene_name}' aktualisieren"
            ))
        }
        ProjectOperation::AddKeyframe {
            layer,
            frame,
            scene,
            symbol,
            label,
            duplicate_previous,
        } => {
            let index = checked_frame(*frame)?;
            if let Some(label) = label {
                validate_label(label)?;
            }
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            let duplicated = if *duplicate_previous {
                index
                    .checked_sub(1)
                    .and_then(|previous| layer.last_keyframe_at_or_before(previous))
                    .and_then(|previous| layer.frames.get(previous))
                    .map(|frame| frame.objects.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            layer.ensure_frames(index + 1);
            let frame = &mut layer.frames[index];
            frame.keyframe = true;
            if *duplicate_previous {
                frame.objects = duplicated;
            }
            if let Some(label) = label {
                frame.label = Some(label.trim().to_string());
            }
            project.touch();
            Ok(format!(
                "SchlÃ¼sselbild {} in Ebene '{layer_name}' (Szene '{scene_name}') anlegen",
                index + 1
            ))
        }
        ProjectOperation::SetFrameLabel {
            layer,
            frame,
            label,
            scene,
            symbol,
        } => {
            let index = checked_frame(*frame)?;
            validate_label(label)?;
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            if index >= layer.frames.len() {
                return Err(OperationError::invalid(format!(
                    "Frame {} existiert in Ebene '{layer_name}' nicht.",
                    index + 1
                )));
            }
            layer.frames[index].label = Some(label.trim().to_string());
            project.touch();
            Ok(format!(
                "Frame {} in Ebene '{layer_name}' (Szene '{scene_name}') als '{}' beschriften",
                index + 1,
                label.trim()
            ))
        }
        ProjectOperation::AddTween {
            layer,
            frame,
            scene,
            symbol,
            easing,
            kind,
        } => {
            let index = checked_frame(*frame)?;
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            if index >= layer.frames.len() || !layer.frames[index].keyframe {
                return Err(OperationError::invalid(format!(
                    "Frame {} in Ebene '{layer_name}' ist kein vorhandenes SchlÃ¼sselbild.",
                    index + 1
                )));
            }
            let ids: Vec<_> = layer.frames[index]
                .objects
                .iter()
                .map(|object| object.id)
                .collect();
            if ids.is_empty() {
                return Err(OperationError::invalid(format!(
                    "SchlÃ¼sselbild {} in Ebene '{layer_name}' enthÃ¤lt keine Objekte fÃ¼r einen Tween.",
                    index + 1
                )));
            }
            let next = layer
                .frames
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, frame)| {
                    frame.keyframe && frame.objects.iter().any(|object| ids.contains(&object.id))
                });
            if next.is_none() {
                return Err(OperationError::invalid(format!(
                    "FÃ¼r den Tween ab Frame {} fehlt ein spÃ¤teres SchlÃ¼sselbild mit denselben Objekt-IDs.",
                    index + 1
                )));
            }
            layer.frames[index].tween = Some(Tween {
                easing: *easing,
                kind: kind.unwrap_or_default(),
            });
            project.touch();
            Ok(format!(
                "{}-Tween ab Frame {} in Ebene '{layer_name}' (Szene '{scene_name}') setzen",
                kind.unwrap_or_default().label(),
                index + 1
            ))
        }
        ProjectOperation::AddSymbol {
            name,
            width,
            height,
            fill,
            corner_radius,
            kind,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Symbolname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            check_size(*width, *height, "Symbol")?;
            if project.symbols.iter().any(|symbol| symbol.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Symbol namens '{name}' existiert bereits."
                )));
            }
            if project.symbols.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Die Bibliothek kann hÃ¶chstens 1000 Symbole enthalten.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or_else(|| Rgba::rgb(78, 161, 255));
            let corner_radius = corner_radius.unwrap_or(0.0);
            if !corner_radius.is_finite() || corner_radius < 0.0 {
                return Err(OperationError::invalid(
                    "Der Eckenradius muss eine nichtnegative Zahl sein.",
                ));
            }
            let mut symbol =
                Symbol::with_kind(name, kind.unwrap_or_default(), *width, *height, fill);
            symbol.corner_radius = corner_radius.min(width.min(*height) / 2.0);
            project.symbols.push(symbol);
            project.touch();
            Ok(format!(
                "{}-Symbol '{name}' zur Bibliothek hinzufÃ¼gen",
                kind.unwrap_or_default().label()
            ))
        }
        ProjectOperation::SetSymbolKind { symbol, kind } => {
            let name = symbol.trim();
            let target = project
                .symbols
                .iter_mut()
                .find(|candidate| candidate.name == name)
                .ok_or_else(|| {
                    OperationError::invalid(format!("Symbol '{name}' existiert nicht."))
                })?;
            if target.kind == *kind {
                return Err(OperationError::invalid(format!(
                    "Symbol '{name}' ist bereits vom Typ '{}'.",
                    kind.label()
                )));
            }
            target.kind = *kind;
            project.touch();
            Ok(format!("Symbol '{name}' als '{}' setzen", kind.label()))
        }
        ProjectOperation::PlaceSymbol {
            symbol,
            name,
            x,
            y,
            scene,
            symbol_target,
            layer,
            fill,
        } => {
            let symbol = project
                .symbols
                .iter()
                .find(|candidate| candidate.name == symbol.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Symbol '{}' existiert nicht.", symbol.trim()))
                })?;
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Instanzname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Symbolposition muss eine endliche Zahl sein.",
                ));
            }
            let fill = fill
                .as_deref()
                .map(parse_color)
                .transpose()?
                .unwrap_or(symbol.fill);
            let object = ObjectInstance::symbol(name, *x, *y, symbol.id, fill);
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol_target.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "Symbolinstanz '{name}' auf Ebene '{layer_name}' platzieren"
            ))
        }
        ProjectOperation::ImportBitmapAsset { name, path } => {
            let name = validate_asset_name(name)?;
            let path = validate_asset_path(path, &["png", "jpg", "jpeg", "gif"])?;
            if project.assets.iter().any(|asset| asset.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Asset namens '{name}' existiert bereits."
                )));
            }
            if project.assets.len() >= 2_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann hÃ¶chstens 2000 Assets enthalten.",
                ));
            }
            let data = read_asset(&path)
                .map_err(|error| OperationError::invalid(format!("Asset '{path}': {error}")))?;
            if data.is_empty() || data.len() > 64 * 1024 * 1024 {
                return Err(OperationError::invalid(format!(
                    "Asset '{path}' ist leer oder Ã¼berschreitet 64 MB."
                )));
            }
            let image = image::load_from_memory(&data).map_err(|error| {
                OperationError::invalid(format!("Asset '{path}' ist kein gÃ¼ltiges Bild: {error}"))
            })?;
            let (width, height) = (image.width(), image.height());
            let mime = match path.rsplit('.').next().unwrap_or_default() {
                "png" => "image/png",
                "gif" => "image/gif",
                _ => "image/jpeg",
            };
            project.assets.push(Asset::bitmap(
                name.clone(),
                path.clone(),
                mime,
                width,
                height,
                data,
            ));
            project.touch();
            Ok(format!(
                "Bitmap-Asset '{name}' importieren ({width}Ã—{height})"
            ))
        }
        ProjectOperation::ImportAudioAsset { name, path } => {
            let name = validate_asset_name(name)?;
            let path = validate_asset_path(path, &["wav", "mp3"])?;
            if project.assets.iter().any(|asset| asset.name == name) {
                return Err(OperationError::invalid(format!(
                    "Ein Asset namens '{name}' existiert bereits."
                )));
            }
            if project.assets.len() >= 2_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann hÃ¶chstens 2000 Assets enthalten.",
                ));
            }
            let data = read_asset(&path)
                .map_err(|error| OperationError::invalid(format!("Asset '{path}': {error}")))?;
            if data.is_empty() || data.len() > 256 * 1024 * 1024 {
                return Err(OperationError::invalid(format!(
                    "Asset '{path}' ist leer oder Ã¼berschreitet 256 MB."
                )));
            }
            let is_wav = path.ends_with(".wav");
            let (mime, sample_rate, channels, duration) = if is_wav {
                match wav_info(&data) {
                    Some(info) => ("audio/wav", info.0, info.1, info.2),
                    None => {
                        return Err(OperationError::invalid(format!(
                            "Asset '{path}' ist keine gÃ¼ltige PCM-WAV-Datei."
                        )));
                    }
                }
            } else {
                ("audio/mpeg", 0, 0, 0.0)
            };
            project.assets.push(Asset::audio(
                name.clone(),
                path.clone(),
                mime,
                sample_rate,
                channels,
                duration,
                data,
            ));
            project.touch();
            Ok(format!("Audio-Asset '{name}' importieren"))
        }
        ProjectOperation::PlaceBitmap {
            asset,
            name,
            x,
            y,
            scene,
            symbol,
            layer,
        } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(OperationError::invalid(
                    "Objektname muss 1 bis 120 Zeichen enthalten.",
                ));
            }
            if !x.is_finite() || !y.is_finite() {
                return Err(OperationError::invalid(
                    "Bitmapposition muss eine endliche Zahl sein.",
                ));
            }
            let asset = project
                .assets
                .iter()
                .find(|candidate| candidate.name == asset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Asset '{}' existiert nicht.", asset.trim()))
                })?;
            if asset.kind != AssetKind::Bitmap || !asset.is_embedded() {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist kein eingebettetes Bitmap.",
                    asset.name
                )));
            }
            if asset.width == 0 || asset.height == 0 {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' hat keine gÃ¼ltigen Abmessungen.",
                    asset.name
                )));
            }
            let (asset_id, asset_w, asset_h) = (asset.id, asset.width, asset.height);
            let object =
                ObjectInstance::bitmap(name, *x, *y, asset_id, asset_w as f32, asset_h as f32);
            let layer_name = insert_object(
                project,
                scene.as_deref(),
                symbol.as_deref(),
                layer.as_deref(),
                object,
            )?;
            Ok(format!(
                "Bitmap '{name}' auf Ebene '{layer_name}' platzieren"
            ))
        }
        ProjectOperation::PlaceSoundCue {
            asset,
            frame,
            scene,
            symbol,
            layer,
            event,
        } => {
            let index = checked_frame(*frame)?;
            let asset = project
                .assets
                .iter()
                .find(|candidate| candidate.name == asset.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!("Asset '{}' existiert nicht.", asset.trim()))
                })?;
            if asset.kind != AssetKind::Audio || !asset.is_embedded() {
                return Err(OperationError::invalid(format!(
                    "Asset '{}' ist kein eingebettetes Audio.",
                    asset.name
                )));
            }
            let asset_id = asset.id;
            let asset_name = asset.name.clone();
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            scene.ensure_frame_count(index + 1);
            // Reuse an existing sound layer, honouring an explicit selector,
            // or create a dedicated one.
            let layer_id = match layer.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                Some(name) => scene
                    .layers
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .map(|candidate| candidate.id)
                    .ok_or_else(|| {
                        OperationError::invalid(format!(
                            "Ebene '{name}' existiert in '{scene_name}' nicht."
                        ))
                    })?,
                None => scene
                    .layers
                    .iter()
                    .find(|candidate| candidate.kind == LayerKind::Sound)
                    .map(|candidate| candidate.id)
                    .unwrap_or_else(|| {
                        let mut sound_layer = nova_core::Layer::new("Sound", scene.frame_count());
                        sound_layer.kind = LayerKind::Sound;
                        scene.layers.push(sound_layer);
                        scene.layers.last().map(|layer| layer.id).unwrap()
                    }),
            };
            let target = scene
                .layers
                .iter_mut()
                .find(|candidate| candidate.id == layer_id)
                .ok_or_else(|| OperationError::invalid("Sound-Ebene nicht gefunden."))?;
            if target.kind != LayerKind::Sound {
                return Err(OperationError::invalid(format!(
                    "Ebene '{}' ist keine Sound-Ebene.",
                    target.name
                )));
            }
            target.ensure_frames(index + 1);
            target.frames[index].keyframe = true;
            target.frames[index].sound = Some(if *event {
                SoundCue::event(asset_id)
            } else {
                SoundCue::timeline(asset_id)
            });
            project.touch();
            Ok(format!(
                "Sound '{asset_name}' auf Frame {} in '{scene_name}' setzen",
                index + 1
            ))
        }
        ProjectOperation::SetLayerKind {
            layer,
            kind,
            scene,
            symbol,
        } => {
            let scene = timeline_mut(project, scene.as_deref(), symbol.as_deref())?;
            let scene_name = scene.name.clone();
            let target = scene
                .layers
                .iter_mut()
                .find(|candidate| candidate.name == layer.trim())
                .ok_or_else(|| {
                    OperationError::invalid(format!(
                        "Ebene '{}' existiert in '{scene_name}' nicht.",
                        layer.trim()
                    ))
                })?;
            if target.kind == *kind {
                return Err(OperationError::invalid(format!(
                    "Ebene '{}' ist bereits vom Typ '{}'.",
                    target.name,
                    kind.label()
                )));
            }
            let layer_name = target.name.clone();
            target.kind = *kind;
            project.touch();
            Ok(format!(
                "Ebene '{layer_name}' in '{scene_name}' als '{}' setzen",
                kind.label()
            ))
        }
        ProjectOperation::AddScript { path, source } => {
            let path = normalize_script_path(path)?;
            if project.scripts.iter().any(|script| script.path == path) {
                return Err(OperationError::invalid(format!(
                    "Ein Skript unter '{path}' existiert bereits."
                )));
            }
            if source.chars().count() > 1_000_000 {
                return Err(OperationError::invalid(
                    "Skriptquelle ist auf 1.000.000 Zeichen begrenzt.",
                ));
            }
            if project.scripts.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Ein Projekt kann hÃ¶chstens 1000 Skripte enthalten.",
                ));
            }
            project
                .scripts
                .push(Script::new(path.clone(), source.clone()));
            project.touch();
            Ok(format!("Skript '{path}' hinzufÃ¼gen"))
        }
        ProjectOperation::SetGameObject {
            object,
            scene,
            collision,
            no_collision,
            physics,
            no_physics,
            clear,
        } => {
            if collision.is_none()
                && physics.is_none()
                && !no_collision
                && !no_physics
                && !clear
            {
                return Err(OperationError::invalid(
                    "SetGameObject benÃ¶tigt mindestens eine Ã„nderung (collision, physics, no_*, clear).",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let needle = object.trim();
            let mut matches = Vec::new();
            for (layer_index, layer) in scene.layers.iter().enumerate() {
                for (frame_index, frame) in layer.frames.iter().enumerate() {
                    for (object_index, candidate) in frame.objects.iter().enumerate() {
                        if candidate.name == needle {
                            matches.push((layer_index, frame_index, object_index));
                        }
                    }
                }
            }
            match matches.len() {
                0 => {
                    return Err(OperationError::invalid(format!(
                        "Objekt '{needle}' existiert in Szene '{scene_name}' nicht."
                    )));
                }
                1 => {}
                count => {
                    return Err(OperationError::invalid(format!(
                        "Objektname '{needle}' ist {count}-mal vorhanden."
                    )));
                }
            }
            let (layer_index, frame_index, object_index) = matches[0];
            let target =
                &mut scene.layers[layer_index].frames[frame_index].objects[object_index];
            if *clear {
                target.game = None;
            } else {
                let mut config = target.game.clone().unwrap_or_default();
                if *no_collision {
                    config.collision = None;
                } else if let Some(spec) = collision {
                    if !spec.width.is_finite()
                        || !spec.height.is_finite()
                        || spec.width <= 0.0
                        || spec.height <= 0.0
                        || spec.width > 100_000.0
                        || spec.height > 100_000.0
                    {
                        return Err(OperationError::invalid(
                            "KollisionsausmaÃŸe mÃ¼ssen positive endliche Zahlen sein.",
                        ));
                    }
                    config.collision = Some(*spec);
                }
                if *no_physics {
                    config.physics = None;
                } else if let Some(spec) = physics {
                    if !spec.gravity_scale.is_finite()
                        || !spec.friction.is_finite()
                        || !spec.bounce.is_finite()
                        || spec.friction < 0.0
                        || spec.friction > 1_000.0
                        || !(0.0..=1.0).contains(&spec.bounce)
                    {
                        return Err(OperationError::invalid(
                            "Physikwerte sind ungÃ¼ltig (bounce 0..=1, reibung >= 0).",
                        ));
                    }
                    config.physics = Some(*spec);
                }
                target.game = Some(config);
            }
            project.touch();
            Ok(format!(
                "Game-Metadaten fÃ¼r Objekt '{needle}' in Szene '{scene_name}' gesetzt"
            ))
        }
        ProjectOperation::SetSceneGame {
            scene,
            camera,
            input,
            clear,
        } => {
            if camera.is_none() && input.is_none() && !clear {
                return Err(OperationError::invalid(
                    "SetSceneGame benÃ¶tigt mindestens camera, input oder clear.",
                ));
            }
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            if *clear {
                scene.game = None;
            } else {
                let mut config = scene.game.clone().unwrap_or_default();
                if let Some(camera) = camera {
                    if !camera.zoom.is_finite() || camera.zoom <= 0.0 || camera.zoom > 64.0 {
                        return Err(OperationError::invalid(
                            "Kamera-Zoom muss zwischen 0 und 64 liegen.",
                        ));
                    }
                    if camera.follow.chars().count() > 120 {
                        return Err(OperationError::invalid(
                            "Kamera-Folgeobjekt-Name ist zu lang.",
                        ));
                    }
                    config.camera = Some(camera.clone());
                }
                if let Some(input) = input {
                    for (value, label) in [
                        (&input.left, "left"),
                        (&input.right, "right"),
                        (&input.up, "up"),
                        (&input.down, "down"),
                        (&input.jump, "jump"),
                        (&input.action, "action"),
                    ] {
                        if value.trim().is_empty() || value.chars().count() > 40 {
                            return Err(OperationError::invalid(format!(
                                "Eingabe-Bindung '{label}' ist leer oder zu lang."
                            )));
                        }
                    }
                    config.input = Some(input.clone());
                }
                scene.game = Some(config);
            }
            project.touch();
            Ok(format!("Spielkonfiguration fÃ¼r Szene '{scene_name}' gesetzt"))
        }
        ProjectOperation::InstallGameKit => {
            let before = project.scripts.len();
            nova_core::gamekit::install(project);
            let added = project.scripts.len() - before;
            project.touch();
            Ok(format!(
                "Spiel-Kit installiert ({added} Skript(e) hinzugefÃ¼gt; vorhandene bleiben unverÃ¤ndert)"
            ))
        }
    }
}

fn insert_object(
    project: &mut Project,
    scene: Option<&str>,
    symbol: Option<&str>,
    layer: Option<&str>,
    object: ObjectInstance,
) -> Result<String, OperationError> {
    let scene = timeline_mut(project, scene, symbol)?;
    let layer_name = layer.map(str::trim).filter(|name| !name.is_empty());
    let target = match layer_name {
        Some(name) => scene
            .layers
            .iter_mut()
            .find(|layer| layer.name == name)
            .ok_or_else(|| {
                OperationError::invalid(format!(
                    "Ebene '{name}' existiert in Szene '{}' nicht.",
                    scene.name
                ))
            })?,
        None => scene.layers.first_mut().ok_or_else(|| {
            OperationError::invalid("Die Szene enthÃ¤lt keine Ebene fÃ¼r das Objekt.")
        })?,
    };
    let layer_name = target.name.clone();
    let Some(frame) = target.frames.first_mut() else {
        return Err(OperationError::invalid(format!(
            "Ebene '{layer_name}' enthÃ¤lt keinen Frame."
        )));
    };
    frame.keyframe = true;
    frame.objects.push(object);
    project.touch();
    Ok(layer_name)
}

fn find_layer_mut<'a>(
    scene: &'a mut nova_core::Scene,
    name: &str,
) -> Result<&'a mut nova_core::Layer, OperationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OperationError::invalid("Ebenenname ist leer."));
    }
    let scene_name = scene.name.clone();
    scene
        .layers
        .iter_mut()
        .find(|layer| layer.name == name)
        .ok_or_else(|| {
            OperationError::invalid(format!(
                "Ebene '{name}' existiert in Szene '{scene_name}' nicht."
            ))
        })
}

fn checked_frame(frame: usize) -> Result<usize, OperationError> {
    if frame == 0 {
        return Err(OperationError::invalid(
            "Frame-Nummern sind 1-basiert; 0 ist ungÃ¼ltig.",
        ));
    }
    if frame > 100_000 {
        return Err(OperationError::invalid(
            "Frame-Nummer ist auf 100000 begrenzt.",
        ));
    }
    Ok(frame - 1)
}

fn validate_label(label: &str) -> Result<(), OperationError> {
    let label = label.trim();
    if label.is_empty() || label.chars().count() > 64 {
        return Err(OperationError::invalid(
            "Frame-Label muss 1 bis 64 Zeichen enthalten.",
        ));
    }
    if label.chars().any(char::is_control) {
        return Err(OperationError::invalid(
            "Frame-Label darf keine Steuerzeichen enthalten.",
        ));
    }
    Ok(())
}

/// Resolves the edit target: a symbol timeline when `symbol` names a library
/// symbol, otherwise a scene (or the active scene when `scene` is empty).
fn timeline_mut<'a>(
    project: &'a mut Project,
    scene: Option<&str>,
    symbol: Option<&str>,
) -> Result<&'a mut nova_core::Scene, OperationError> {
    let symbol = symbol.map(str::trim).filter(|name| !name.is_empty());
    if let Some(name) = symbol {
        if scene.is_some() {
            return Err(OperationError::invalid(
                "Gib entweder eine Szene oder ein Symbol an, nicht beides.",
            ));
        }
        return project
            .symbols
            .iter_mut()
            .find(|candidate| candidate.name == name)
            .map(|symbol| &mut symbol.timeline)
            .ok_or_else(|| OperationError::invalid(format!("Symbol '{name}' existiert nicht.")));
    }
    scene_mut(project, scene)
}

fn scene_mut<'a>(
    project: &'a mut Project,
    selector: Option<&str>,
) -> Result<&'a mut nova_core::Scene, OperationError> {
    let selector = selector.map(str::trim).filter(|name| !name.is_empty());
    match selector {
        Some(name) => project
            .scenes
            .iter_mut()
            .find(|scene| scene.name == name)
            .ok_or_else(|| OperationError::invalid(format!("Szene '{name}' existiert nicht."))),
        None => project
            .active_scene_mut()
            .ok_or_else(|| OperationError::invalid("Das Projekt enthÃ¤lt keine aktive Szene.")),
    }
}

fn check_size(width: f32, height: f32, label: &str) -> Result<(), OperationError> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(OperationError::invalid(format!(
            "{label}breite und -hÃ¶he mÃ¼ssen positive endliche Zahlen sein."
        )));
    }
    if width > 100_000.0 || height > 100_000.0 {
        return Err(OperationError::invalid(format!(
            "{label}abmessungen sind auf 100000 begrenzt."
        )));
    }
    Ok(())
}

fn validate_asset_name(name: &str) -> Result<String, OperationError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(OperationError::invalid(
            "Assetname muss 1 bis 120 Zeichen enthalten.",
        ));
    }
    if name
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(OperationError::invalid(
            "Assetname darf keine Pfad- oder Steuerzeichen enthalten.",
        ));
    }
    Ok(name.to_string())
}

/// Validates a workspace-relative asset path: relative, no traversal, no
/// drive/UNC components, whitelisted extension, bounded length.
fn validate_asset_path(path: &str, extensions: &[&str]) -> Result<String, OperationError> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || path.chars().count() > 512 {
        return Err(OperationError::invalid("Assetpfad ist leer oder zu lang."));
    }
    if path.starts_with('/')
        || path.starts_with("//")
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return Err(OperationError::invalid(format!(
            "Unsicherer Assetpfad '{path}'."
        )));
    }
    let extension = path.rsplit('.').next().unwrap_or_default();
    if !extensions.contains(&extension.to_ascii_lowercase().as_str()) {
        return Err(OperationError::invalid(format!(
            "Assettyp wird nicht unterstÃ¼tzt (erlaubt: {}).",
            extensions.join(", ")
        )));
    }
    Ok(path)
}

/// Extracts `(sample_rate, channels, duration_seconds)` from PCM WAV data.
fn wav_info(data: &[u8]) -> Option<(u32, u16, f32)> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut data_size = 0u32;
    while offset + 8 <= data.len() {
        let id = &data[offset..offset + 4];
        let size = u32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]) as usize;
        let body = offset + 8;
        if body + size > data.len() {
            break;
        }
        if id == b"fmt " && size >= 16 {
            let audio_format = u16::from_le_bytes([data[body], data[body + 1]]);
            channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
            sample_rate = u32::from_le_bytes([
                data[body + 4],
                data[body + 5],
                data[body + 6],
                data[body + 7],
            ]);
            let bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
            if audio_format != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
                return None;
            }
        } else if id == b"data" {
            data_size = size as u32;
        }
        offset = body + size + (size & 1);
    }
    if sample_rate == 0 || data_size == 0 {
        return None;
    }
    let bytes_per_second = sample_rate as f32 * channels as f32 * 2.0;
    Some((sample_rate, channels, data_size as f32 / bytes_per_second))
}

fn parse_color(value: &str) -> Result<Rgba, OperationError> {
    Rgba::from_hex(value.trim()).ok_or_else(|| {
        OperationError::invalid(format!(
            "UngÃ¼ltige Farbe '{value}' (erwartet #rrggbb oder #rrggbbaa)."
        ))
    })
}

fn normalize_script_path(path: &str) -> Result<String, OperationError> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return Err(OperationError::invalid(format!(
            "Unsicherer Skriptpfad '{path}'."
        )));
    }
    if !path.starts_with("scripts/") {
        return Err(OperationError::invalid(
            "Neue Skripte mÃ¼ssen unter 'scripts/' liegen.",
        ));
    }
    if !path.ends_with(".as") {
        return Err(OperationError::invalid(
            "Skriptpfade mÃ¼ssen mit '.as' enden.",
        ));
    }
    if path.chars().count() > 240 {
        return Err(OperationError::invalid(
            "Skriptpfad ist auf 240 Zeichen begrenzt.",
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_batch_is_transactional_and_previews_summaries() {
        let mut project = Project::new_empty("Start");
        let operations = vec![
            ProjectOperation::RenameProject { name: "Neu".into() },
            ProjectOperation::AddLayer {
                name: "Hintergrund".into(),
                scene: None,
                symbol: None,
                frames: Some(4),
            },
            ProjectOperation::AddObject {
                name: "Held".into(),
                scene: None,
                symbol: None,
                layer: Some("Hintergrund".into()),
                shape: ObjectShape::Rect,
                x: 10.0,
                y: 20.0,
                width: 32.0,
                height: 48.0,
                fill: Some("#ff8800".into()),
                text: String::new(),
                font_size: None,
            },
        ];
        let plan = preview_operations(&project, &operations);
        assert!(plan.errors.is_empty(), "{:?}", plan.errors);
        assert_eq!(plan.summaries.len(), 3);
        assert_eq!(project.metadata.name, "Start", "preview mutated project");

        let summaries = apply_operations(&mut project, &operations).unwrap();
        assert_eq!(summaries.len(), 3);
        assert_eq!(project.metadata.name, "Neu");
        let scene = project.active_scene().unwrap();
        let layer = scene
            .layers
            .iter()
            .find(|layer| layer.name == "Hintergrund")
            .unwrap();
        assert_eq!(layer.frames.len(), 4);
        assert_eq!(layer.frames[0].objects[0].name, "Held");
    }

    #[test]
    fn invalid_late_operation_leaves_project_untouched() {
        let mut project = Project::demo();
        let before = project.clone();
        let operations = vec![
            ProjectOperation::AddLayer {
                name: "Neu".into(),
                scene: None,
                symbol: None,
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Fehlt".into(),
                scene: None,
                symbol: None,
                layer: Some("Nicht vorhanden".into()),
                shape: ObjectShape::Ellipse,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
        ];
        let error = apply_operations(&mut project, &operations).unwrap_err();
        assert!(error.to_string().contains("Nicht vorhanden"));
        assert_eq!(project, before);
    }

    #[test]
    fn symbols_scripts_and_text_objects_are_real_model_data() {
        let mut project = Project::new_empty("M8");
        let operations = vec![
            ProjectOperation::AddSymbol {
                name: "Kreis".into(),
                width: 40.0,
                height: 40.0,
                fill: Some("#00ff00".into()),
                corner_radius: Some(20.0),
                kind: None,
            },
            ProjectOperation::PlaceSymbol {
                symbol: "Kreis".into(),
                name: "KreisInstanz".into(),
                x: 100.0,
                y: 80.0,
                scene: None,
                symbol_target: None,
                layer: None,
                fill: None,
            },
            ProjectOperation::AddObject {
                name: "Titel".into(),
                scene: None,
                symbol: None,
                layer: None,
                shape: ObjectShape::Text,
                x: 20.0,
                y: 30.0,
                width: 200.0,
                height: 40.0,
                fill: None,
                text: "Hallo NÃ˜VA".into(),
                font_size: Some(32.0),
            },
            ProjectOperation::AddScript {
                path: "scripts/Added.as".into(),
                source: "class Added {}\n".into(),
            },
            ProjectOperation::SetStage {
                width: 800.0,
                height: 600.0,
                fps: Some(30.0),
                background: Some("#101010".into()),
                scene: None,
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        assert_eq!(project.symbols.len(), 1);
        assert!(
            project
                .scripts
                .iter()
                .any(|script| script.path == "scripts/Added.as")
        );
        let scene = project.active_scene().unwrap();
        assert_eq!((scene.width, scene.height, scene.fps), (800.0, 600.0, 30.0));
        let objects = &scene.layers[0].frames[0].objects;
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Symbol { .. }))
        );
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Text(_)))
        );
    }

    #[test]
    fn animation_operations_create_labeled_keyframes_and_tweens() {
        let mut project = Project::new_empty("Animation");
        let operations = vec![
            ProjectOperation::AddLayer {
                name: "Anim".into(),
                scene: None,
                symbol: None,
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Box".into(),
                scene: None,
                symbol: None,
                layer: Some("Anim".into()),
                shape: ObjectShape::Rect,
                x: 10.0,
                y: 10.0,
                width: 40.0,
                height: 40.0,
                fill: Some("#ff0000".into()),
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::UpdateObject {
                name: "Box".into(),
                scene: None,
                layer: Some("Anim".into()),
                x: Some(42.0),
                y: None,
                rotation: Some(15.0),
                scale_x: None,
                scale_y: None,
                fill: None,
                text: None,
                font_size: None,
            },
            ProjectOperation::AddKeyframe {
                layer: "Anim".into(),
                frame: 10,
                scene: None,
                symbol: None,
                label: Some("end".into()),
                duplicate_previous: true,
            },
            ProjectOperation::SetFrameLabel {
                layer: "Anim".into(),
                frame: 1,
                label: "start".into(),
                scene: None,
                symbol: None,
            },
            ProjectOperation::AddTween {
                layer: "Anim".into(),
                frame: 1,
                scene: None,
                symbol: None,
                easing: Easing::Linear,
                kind: None,
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        let layer = project
            .active_scene()
            .unwrap()
            .layers
            .iter()
            .find(|layer| layer.name == "Anim")
            .unwrap();
        assert_eq!(layer.frames.len(), 10);
        assert_eq!(layer.frames[0].label.as_deref(), Some("start"));
        assert_eq!(layer.frames[9].label.as_deref(), Some("end"));
        assert_eq!(layer.frames[0].objects[0].transform.x, 42.0);
        assert_eq!(layer.frames[0].objects[0].transform.rotation, 15.0);
        assert_eq!(
            layer.frames[0].objects[0].id, layer.frames[9].objects[0].id,
            "duplicated keyframe must preserve object identity for tweening"
        );
        assert_eq!(
            layer.frames[0].tween.as_ref().map(|tween| tween.easing),
            Some(Easing::Linear)
        );
    }

    #[test]
    fn tween_without_a_later_matching_keyframe_is_rejected() {
        let mut project = Project::new_empty("Kein Tween");
        let operations = vec![
            ProjectOperation::AddObject {
                name: "Box".into(),
                scene: None,
                symbol: None,
                layer: None,
                shape: ObjectShape::Rect,
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::AddTween {
                layer: "Ebene 1".into(),
                frame: 1,
                scene: None,
                symbol: None,
                easing: Easing::EaseIn,
                kind: None,
            },
        ];
        let error = apply_operations(&mut project, &operations).unwrap_err();
        assert!(error.to_string().contains("spÃ¤teres SchlÃ¼sselbild"));
    }

    #[test]
    fn unsafe_script_paths_are_rejected() {
        let mut project = Project::new_empty("Safe");
        for path in [
            "../escape.as",
            "C:/absolute.as",
            "scripts/../escape.as",
            "assets/not-a-script.as",
            "scripts/nope.txt",
        ] {
            let error = apply_operations(
                &mut project,
                &[ProjectOperation::AddScript {
                    path: path.into(),
                    source: String::new(),
                }],
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("Skript") || error.to_string().contains("Unsicher"),
                "{path}: {error}"
            );
        }
        assert!(
            project
                .scripts
                .iter()
                .all(|script| script.path != "../escape.as")
        );
    }

    #[test]
    fn operation_count_is_bounded() {
        let project = Project::new_empty("Limit");
        let operations = vec![ProjectOperation::AddScene { name: "S".into() }; MAX_OPERATIONS + 1];
        let plan = preview_operations(&project, &operations);
        assert!(!plan.errors.is_empty());
        assert!(plan.errors[0].contains("Zu viele"));
    }

    // ------------------------------------------------------------ M9 ops

    /// A 1x1 transparent PNG, enough for the importer to decode dimensions.
    fn tiny_png() -> Vec<u8> {
        // 1x1 RGBA PNG generated once; decode path is what matters here.
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    /// Minimal PCM WAV: 8kHz mono 16-bit, 800 samples (0.1 s).
    fn tiny_wav() -> Vec<u8> {
        let data_size = 1600u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&2u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        wav.extend(std::iter::repeat(0u8).take(data_size as usize));
        wav
    }

    #[test]
    fn movieclip_symbol_kind_and_symbol_timeline_targeting() {
        let mut project = Project::new_empty("Sym");
        let operations = vec![
            ProjectOperation::AddSymbol {
                name: "Clip".into(),
                width: 64.0,
                height: 64.0,
                fill: None,
                corner_radius: None,
                kind: Some(SymbolKind::MovieClip),
            },
            ProjectOperation::SetSymbolKind {
                symbol: "Clip".into(),
                kind: SymbolKind::Graphic,
            },
            ProjectOperation::AddLayer {
                name: "Innen".into(),
                scene: None,
                symbol: Some("Clip".into()),
                frames: Some(5),
            },
            ProjectOperation::AddObject {
                name: "Punkt".into(),
                scene: None,
                symbol: Some("Clip".into()),
                layer: Some("Innen".into()),
                shape: ObjectShape::Rect,
                x: 5.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
                fill: None,
                text: String::new(),
                font_size: None,
            },
            ProjectOperation::AddKeyframe {
                layer: "Innen".into(),
                frame: 5,
                scene: None,
                symbol: Some("Clip".into()),
                label: None,
                duplicate_previous: true,
            },
            ProjectOperation::AddTween {
                layer: "Innen".into(),
                frame: 1,
                scene: None,
                symbol: Some("Clip".into()),
                easing: Easing::Linear,
                kind: Some(nova_core::TweenKind::Shape),
            },
        ];
        apply_operations(&mut project, &operations).unwrap();
        let symbol = project
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Clip")
            .unwrap();
        assert_eq!(symbol.kind, SymbolKind::Graphic);
        let timeline = &symbol.timeline;
        assert!(timeline.layers.iter().any(|layer| layer.name == "Innen"));
        let layer = timeline
            .layers
            .iter()
            .find(|layer| layer.name == "Innen")
            .unwrap();
        assert_eq!(layer.frames.len(), 5);
        assert!(layer.frames[4].keyframe);
        assert_eq!(
            layer.frames[0].tween.as_ref().map(|t| t.kind),
            Some(nova_core::TweenKind::Shape)
        );
        assert!(
            layer.frames[0]
                .objects
                .iter()
                .any(|object| object.name == "Punkt")
        );
    }

    #[test]
    fn asset_import_requires_workspace_reader_and_validates_paths() {
        let mut project = Project::new_empty("Assets");
        // Without a reader the import must fail, never silently succeed.
        let denied = apply_operations(
            &mut project,
            &[ProjectOperation::ImportBitmapAsset {
                name: "BG".into(),
                path: "assets/bg.png".into(),
            }],
        )
        .unwrap_err();
        assert!(denied.to_string().contains("Workspace"));

        // Unsafe paths are rejected before the reader is even consulted.
        for path in ["../x.png", "/abs/x.png", "C:/x.png", "a/../b.png", "x.exe"] {
            let error = apply_operations_with(
                &mut project,
                &[ProjectOperation::ImportBitmapAsset {
                    name: "Bad".into(),
                    path: path.into(),
                }],
                &|_| Ok(tiny_png()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("Asset"), "{path}: {error}");
        }

        let reader = |path: &str| -> Result<Vec<u8>, String> {
            match path {
                "assets/bg.png" => Ok(tiny_png()),
                "assets/tune.wav" => Ok(tiny_wav()),
                _ => Err("not found".into()),
            }
        };
        let operations = vec![
            ProjectOperation::ImportBitmapAsset {
                name: "BG".into(),
                path: "assets/bg.png".into(),
            },
            ProjectOperation::ImportAudioAsset {
                name: "Tune".into(),
                path: "assets/tune.wav".into(),
            },
            ProjectOperation::PlaceBitmap {
                asset: "BG".into(),
                name: "Hintergrund".into(),
                x: 100.0,
                y: 100.0,
                scene: None,
                symbol: None,
                layer: None,
            },
            ProjectOperation::PlaceSoundCue {
                asset: "Tune".into(),
                frame: 1,
                scene: None,
                symbol: None,
                layer: None,
                event: true,
            },
            ProjectOperation::SetLayerKind {
                layer: "Ebene 1".into(),
                kind: LayerKind::Guide,
                scene: None,
                symbol: None,
            },
        ];
        apply_operations_with(&mut project, &operations, &reader).unwrap();
        assert_eq!(project.assets.len(), 2);
        let bitmap = project
            .assets
            .iter()
            .find(|asset| asset.name == "BG")
            .unwrap();
        assert_eq!(bitmap.kind, AssetKind::Bitmap);
        assert_eq!((bitmap.width, bitmap.height), (1, 1));
        let audio = project
            .assets
            .iter()
            .find(|asset| asset.name == "Tune")
            .unwrap();
        assert_eq!(audio.kind, AssetKind::Audio);
        assert_eq!(audio.sample_rate, 8000);
        let scene = project.active_scene().unwrap();
        assert!(scene.layers.iter().any(|layer| {
            layer.kind == LayerKind::Sound
                && layer.frames[0]
                    .sound
                    .is_some_and(|cue| cue.asset == audio.id)
        }));
        assert!(
            scene.layers[0].frames[0]
                .objects
                .iter()
                .any(|object| matches!(object.kind, nova_core::ObjectKind::Bitmap { .. }))
        );
    }

    #[test]
    fn game_operations_set_metadata_and_install_kit() {
        let mut project = Project::new_empty("Game");
        apply_operations(
            &mut project,
            &[
                ProjectOperation::AddObject {
                    name: "Held".into(),
                    scene: None,
                    symbol: None,
                    layer: None,
                    shape: ObjectShape::Rect,
                    x: 10.0,
                    y: 20.0,
                    width: 32.0,
                    height: 48.0,
                    fill: None,
                    text: String::new(),
                    font_size: None,
                },
                ProjectOperation::SetGameObject {
                    object: "Held".into(),
                    scene: None,
                    collision: Some(nova_core::CollisionSpec {
                        shape: nova_core::CollisionShape::Rectangle,
                        width: 32.0,
                        height: 48.0,
                    }),
                    no_collision: false,
                    physics: Some(nova_core::PhysicsSpec {
                        body: nova_core::BodyType::Dynamic,
                        gravity_scale: 1.0,
                        friction: 0.0,
                        bounce: 0.0,
                        controlled: true,
                    }),
                    no_physics: false,
                    clear: false,
                },
                ProjectOperation::SetSceneGame {
                    scene: None,
                    camera: Some(nova_core::CameraConfig {
                        enabled: true,
                        follow: "Held".into(),
                        left: 0.0,
                        top: 0.0,
                        right: 800.0,
                        bottom: 600.0,
                        zoom: 1.5,
                    }),
                    input: Some(nova_core::InputBindings {
                        left: "A".into(),
                        right: "D".into(),
                        up: "W".into(),
                        down: "S".into(),
                        jump: "Space".into(),
                        action: "E".into(),
                    }),
                    clear: false,
                },
                ProjectOperation::InstallGameKit,
            ],
        )
        .unwrap();

        let scene = project.active_scene().unwrap();
        let object = scene.layers[0].frames[0]
            .objects
            .iter()
            .find(|object| object.name == "Held")
            .unwrap();
        let game = object.game.as_ref().unwrap();
        assert_eq!(game.collision.unwrap().width, 32.0);
        assert!(game.physics.unwrap().controlled);
        let scene_game = scene.game.as_ref().unwrap();
        assert!(scene_game.camera.as_ref().unwrap().enabled);
        assert_eq!(scene_game.input.as_ref().unwrap().left, "A");
        for path in [
            "scripts/game/Input.as",
            "scripts/game/Collision.as",
            "scripts/game/Camera.as",
            "scripts/game/Random.as",
        ] {
            assert!(
                project.scripts.iter().any(|script| script.path == path),
                "missing {path}"
            );
        }

        // Idempotent: a second install adds nothing.
        let script_count = project.scripts.len();
        apply_operations(&mut project, &[ProjectOperation::InstallGameKit]).unwrap();
        assert_eq!(project.scripts.len(), script_count);

        // `clear` removes object game metadata.
        apply_operations(
            &mut project,
            &[ProjectOperation::SetGameObject {
                object: "Held".into(),
                scene: None,
                collision: None,
                no_collision: false,
                physics: None,
                no_physics: false,
                clear: true,
            }],
        )
        .unwrap();
        let scene = project.active_scene().unwrap();
        let object = scene.layers[0].frames[0]
            .objects
            .iter()
            .find(|object| object.name == "Held")
            .unwrap();
        assert!(object.game.is_none());
    }
}
