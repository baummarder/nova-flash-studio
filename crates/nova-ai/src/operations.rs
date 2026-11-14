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

use nova_core::{Easing, ObjectInstance, Project, Rgba, Script, Symbol, Tween};
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
        #[serde(default)]
        frames: Option<usize>,
    },
    AddObject {
        name: String,
        #[serde(default)]
        scene: Option<String>,
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
    },
    AddTween {
        layer: String,
        /// 1-based frame number of the first keyframe.
        frame: usize,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        easing: Easing,
    },
    AddSymbol {
        name: String,
        width: f32,
        height: f32,
        #[serde(default)]
        fill: Option<String>,
        #[serde(default)]
        corner_radius: Option<f32>,
    },
    PlaceSymbol {
        symbol: String,
        name: String,
        x: f32,
        y: f32,
        #[serde(default)]
        scene: Option<String>,
        #[serde(default)]
        layer: Option<String>,
        #[serde(default)]
        fill: Option<String>,
    },
    AddScript {
        path: String,
        source: String,
    },
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum OperationError {
    #[error("Ungültige Projektoperation: {0}")]
    Invalid(String),
    #[error("Nicht unterstützte Projektoperation: {0}")]
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
        match apply_one(&mut preview, operation) {
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
    let plan = preview_operations(project, operations);
    if !plan.errors.is_empty() {
        return Err(OperationError::Invalid(plan.errors.join(" ")));
    }
    let mut next = project.clone();
    let mut summaries = Vec::new();
    for operation in operations {
        summaries.push(apply_one(&mut next, operation)?);
    }
    next.validate()
        .map_err(|error| OperationError::Invalid(error.to_string()))?;
    *project = next;
    Ok(summaries)
}

fn apply_one(
    project: &mut Project,
    operation: &ProjectOperation,
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
            check_size(*width, *height, "Bühne")?;
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
            Ok(format!("Bühne '{name}' auf {width}×{height} setzen"))
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
                    "Ein Projekt kann höchstens 256 Szenen enthalten.",
                ));
            }
            project.add_scene(name);
            Ok(format!("Szene '{name}' hinzufügen"))
        }
        ProjectOperation::AddLayer {
            name,
            scene,
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
            let scene = scene_mut(project, scene.as_deref())?;
            if scene.layers.iter().any(|layer| layer.name == name) {
                return Err(OperationError::invalid(format!(
                    "Eine Ebene namens '{name}' existiert in dieser Szene bereits."
                )));
            }
            if scene.layers.len() >= 1_000 {
                return Err(OperationError::invalid(
                    "Eine Szene kann höchstens 1000 Ebenen enthalten.",
                ));
            }
            let scene_name = scene.name.clone();
            scene.layers.push(nova_core::Layer::new(name, frames));
            project.touch();
            Ok(format!("Ebene '{name}' in '{scene_name}' hinzufügen"))
        }
        ProjectOperation::AddObject {
            name,
            scene,
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
                            "Ein Textobjekt benötigt nichtleeren Text.",
                        ));
                    }
                    let size = font_size.unwrap_or(24.0);
                    if !size.is_finite() || !(1.0..=512.0).contains(&size) {
                        return Err(OperationError::invalid(
                            "Die Textgröße muss zwischen 1 und 512 liegen.",
                        ));
                    }
                    ObjectInstance::text(name, *x, *y, text, size, fill)
                }
            };
            let layer_name = insert_object(project, scene.as_deref(), layer.as_deref(), object)?;
            Ok(format!(
                "{} '{name}' auf Ebene '{layer_name}' hinzufügen",
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
                    "UpdateObject benötigt mindestens eine zu ändernde Eigenschaft.",
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
                        "Die Textgröße muss zwischen 1 und 512 liegen.",
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
                        "Objektname '{name}' ist {count}-mal vorhanden. Gib eine Ebene an, um das Ziel eindeutig zu wählen."
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
            label,
            duplicate_previous,
        } => {
            let index = checked_frame(*frame)?;
            if let Some(label) = label {
                validate_label(label)?;
            }
            let scene = scene_mut(project, scene.as_deref())?;
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
                "Schlüsselbild {} in Ebene '{layer_name}' (Szene '{scene_name}') anlegen",
                index + 1
            ))
        }
        ProjectOperation::SetFrameLabel {
            layer,
            frame,
            label,
            scene,
        } => {
            let index = checked_frame(*frame)?;
            validate_label(label)?;
            let scene = scene_mut(project, scene.as_deref())?;
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
            easing,
        } => {
            let index = checked_frame(*frame)?;
            let scene = scene_mut(project, scene.as_deref())?;
            let scene_name = scene.name.clone();
            let layer = find_layer_mut(scene, layer)?;
            let layer_name = layer.name.clone();
            if index >= layer.frames.len() || !layer.frames[index].keyframe {
                return Err(OperationError::invalid(format!(
                    "Frame {} in Ebene '{layer_name}' ist kein vorhandenes Schlüsselbild.",
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
                    "Schlüsselbild {} in Ebene '{layer_name}' enthält keine Objekte für einen Tween.",
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
                    "Für den Tween ab Frame {} fehlt ein späteres Schlüsselbild mit denselben Objekt-IDs.",
                    index + 1
                )));
            }
            layer.frames[index].tween = Some(Tween { easing: *easing });
            project.touch();
            Ok(format!(
                "Motion-Tween ab Frame {} in Ebene '{layer_name}' (Szene '{scene_name}') setzen",
                index + 1
            ))
        }
        ProjectOperation::AddSymbol {
            name,
            width,
            height,
            fill,
            corner_radius,
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
                    "Die Bibliothek kann höchstens 1000 Symbole enthalten.",
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
            let mut symbol = Symbol::new(name, *width, *height, fill);
            symbol.corner_radius = corner_radius.min(width.min(*height) / 2.0);
            project.symbols.push(symbol);
            project.touch();
            Ok(format!("Symbol '{name}' zur Bibliothek hinzufügen"))
        }
        ProjectOperation::PlaceSymbol {
            symbol,
            name,
            x,
            y,
            scene,
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
            let layer_name = insert_object(project, scene.as_deref(), layer.as_deref(), object)?;
            Ok(format!(
                "Symbolinstanz '{name}' auf Ebene '{layer_name}' platzieren"
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
                    "Ein Projekt kann höchstens 1000 Skripte enthalten.",
                ));
            }
            project
                .scripts
                .push(Script::new(path.clone(), source.clone()));
            project.touch();
            Ok(format!("Skript '{path}' hinzufügen"))
        }
    }
}

fn insert_object(
    project: &mut Project,
    scene: Option<&str>,
    layer: Option<&str>,
    object: ObjectInstance,
) -> Result<String, OperationError> {
    let scene = scene_mut(project, scene)?;
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
            OperationError::invalid("Die Szene enthält keine Ebene für das Objekt.")
        })?,
    };
    let layer_name = target.name.clone();
    let Some(frame) = target.frames.first_mut() else {
        return Err(OperationError::invalid(format!(
            "Ebene '{layer_name}' enthält keinen Frame."
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
            "Frame-Nummern sind 1-basiert; 0 ist ungültig.",
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
            .ok_or_else(|| OperationError::invalid("Das Projekt enthält keine aktive Szene.")),
    }
}

fn check_size(width: f32, height: f32, label: &str) -> Result<(), OperationError> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(OperationError::invalid(format!(
            "{label}breite und -höhe müssen positive endliche Zahlen sein."
        )));
    }
    if width > 100_000.0 || height > 100_000.0 {
        return Err(OperationError::invalid(format!(
            "{label}abmessungen sind auf 100000 begrenzt."
        )));
    }
    Ok(())
}

fn parse_color(value: &str) -> Result<Rgba, OperationError> {
    Rgba::from_hex(value.trim()).ok_or_else(|| {
        OperationError::invalid(format!(
            "Ungültige Farbe '{value}' (erwartet #rrggbb oder #rrggbbaa)."
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
            "Neue Skripte müssen unter 'scripts/' liegen.",
        ));
    }
    if !path.ends_with(".as") {
        return Err(OperationError::invalid(
            "Skriptpfade müssen mit '.as' enden.",
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
                frames: Some(4),
            },
            ProjectOperation::AddObject {
                name: "Held".into(),
                scene: None,
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
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Fehlt".into(),
                scene: None,
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
            },
            ProjectOperation::PlaceSymbol {
                symbol: "Kreis".into(),
                name: "KreisInstanz".into(),
                x: 100.0,
                y: 80.0,
                scene: None,
                layer: None,
                fill: None,
            },
            ProjectOperation::AddObject {
                name: "Titel".into(),
                scene: None,
                layer: None,
                shape: ObjectShape::Text,
                x: 20.0,
                y: 30.0,
                width: 200.0,
                height: 40.0,
                fill: None,
                text: "Hallo NØVA".into(),
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
                frames: Some(1),
            },
            ProjectOperation::AddObject {
                name: "Box".into(),
                scene: None,
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
                label: Some("end".into()),
                duplicate_previous: true,
            },
            ProjectOperation::SetFrameLabel {
                layer: "Anim".into(),
                frame: 1,
                label: "start".into(),
                scene: None,
            },
            ProjectOperation::AddTween {
                layer: "Anim".into(),
                frame: 1,
                scene: None,
                easing: Easing::Linear,
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
                easing: Easing::EaseIn,
            },
        ];
        let error = apply_operations(&mut project, &operations).unwrap_err();
        assert!(error.to_string().contains("späteres Schlüsselbild"));
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
}
