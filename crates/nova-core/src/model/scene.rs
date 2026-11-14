use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Frame, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Normal,
    Folder,
    Guide,
    Mask,
    Sound,
}

impl LayerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Folder => "Ordner",
            Self::Guide => "Hilfsebene",
            Self::Mask => "Maske",
            Self::Sound => "Sound",
        }
    }

    pub fn is_visual(self) -> bool {
        !matches!(self, Self::Folder | Self::Guide | Self::Sound)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: Uuid,
    pub name: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub kind: LayerKind,
    /// Folder layer that contains this layer. The referenced layer must itself
    /// have `kind == Folder`; validation rejects dangling references and cycles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Uuid>,
    /// Only meaningful for folder layers. Collapsed folders hide their children
    /// in the timeline but do not hide them on the stage.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub expanded: bool,
    /// Contiguous frames, starting at frame 0. Index == frame number.
    pub frames: Vec<Frame>,
}

fn default_true() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Layer {
    pub fn new(name: impl Into<String>, frame_count: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            visible: true,
            locked: false,
            kind: LayerKind::Normal,
            parent: None,
            expanded: true,
            frames: (0..frame_count)
                .map(|i| Frame {
                    keyframe: i == 0,
                    ..Default::default()
                })
                .collect(),
        }
    }

    /// Ensures at least `count` frames exist.
    pub fn ensure_frames(&mut self, count: usize) {
        while self.frames.len() < count {
            self.frames.push(Frame::default());
        }
    }

    /// Number of frames this layer contributes to the scene duration.
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn last_keyframe_at_or_before(&self, frame: usize) -> Option<usize> {
        (0..=frame.min(self.frames.len().saturating_sub(1)))
            .rev()
            .find(|&i| self.frames[i].keyframe)
    }

    pub fn next_keyframe_after(&self, frame: usize) -> Option<usize> {
        ((frame + 1)..self.frames.len()).find(|&i| self.frames[i].keyframe)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: Uuid,
    pub name: String,
    pub width: f32,
    pub height: f32,
    pub background: Rgba,
    /// Frames per second.
    pub fps: f32,
    pub layers: Vec<Layer>,
    /// Optional game configuration (camera, input bindings). Format v3
    /// addition; ignored by the timeline and the SWF exporter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<crate::model::GameSceneConfig>,
}

impl Scene {
    pub fn new(name: impl Into<String>, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            width,
            height,
            background: Rgba::rgb(18, 20, 26),
            fps: 24.0,
            layers: vec![Layer::new("Ebene 1", 1)],
            game: None,
        }
    }

    /// Longest layer determines the timeline length.
    pub fn frame_count(&self) -> usize {
        self.layers
            .iter()
            .map(Layer::frame_count)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn ensure_frame_count(&mut self, count: usize) {
        for layer in &mut self.layers {
            layer.ensure_frames(count);
        }
    }

    pub fn layer(&self, index: usize) -> Option<&Layer> {
        self.layers.get(index)
    }

    pub fn layer_mut(&mut self, index: usize) -> Option<&mut Layer> {
        self.layers.get_mut(index)
    }

    pub fn layer_by_id(&self, id: Uuid) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    /// A mask layer masks the next lower visual layer. This convention is used
    /// by the editor preview and the SWF clip-depth export.
    pub fn mask_target_index(&self, mask_index: usize) -> Option<usize> {
        if self.layers.get(mask_index)?.kind != LayerKind::Mask {
            return None;
        }
        let target = mask_index.checked_sub(1)?;
        self.layers
            .get(target)
            .filter(|layer| layer.kind.is_visual())
            .map(|_| target)
    }

    /// Depth of a layer below top-level folders. Top-level layers have depth 0.
    pub fn layer_depth(&self, id: Uuid) -> usize {
        let mut depth = 0usize;
        let mut current = self.layer_by_id(id).and_then(|layer| layer.parent);
        while let Some(parent) = current {
            depth += 1;
            if depth > self.layers.len() {
                break;
            }
            current = self.layer_by_id(parent).and_then(|layer| layer.parent);
        }
        depth
    }

    /// A layer is visible only when it and every containing folder is visible.
    pub fn layer_effectively_visible(&self, layer: &Layer) -> bool {
        if !layer.visible {
            return false;
        }
        let mut parent = layer.parent;
        let mut guard = 0usize;
        while let Some(id) = parent {
            let Some(folder) = self.layer_by_id(id) else {
                break;
            };
            if !folder.visible {
                return false;
            }
            parent = folder.parent;
            guard += 1;
            if guard > self.layers.len() {
                break;
            }
        }
        true
    }

    /// Locked folders lock all of their descendants in the editor.
    pub fn layer_effectively_locked(&self, layer: &Layer) -> bool {
        if layer.locked {
            return true;
        }
        let mut parent = layer.parent;
        let mut guard = 0usize;
        while let Some(id) = parent {
            let Some(folder) = self.layer_by_id(id) else {
                break;
            };
            if folder.locked {
                return true;
            }
            parent = folder.parent;
            guard += 1;
            if guard > self.layers.len() {
                break;
            }
        }
        false
    }

    /// Removes dangling/cyclic parent links after a layer changed type or was
    /// removed. Children of a removed folder are re-parented by
    /// [`Scene::remove_layer`] before this runs.
    pub fn repair_folder_links(&mut self) {
        let folders: std::collections::HashSet<Uuid> = self
            .layers
            .iter()
            .filter(|layer| layer.kind == LayerKind::Folder)
            .map(|layer| layer.id)
            .collect();
        for layer in &mut self.layers {
            if layer
                .parent
                .is_some_and(|parent| parent == layer.id || !folders.contains(&parent))
            {
                layer.parent = None;
            }
        }
        // Break cycles: walk each chain and detach the first link that repeats.
        for index in 0..self.layers.len() {
            let start = self.layers[index].id;
            let mut seen = std::collections::HashSet::new();
            let mut current = Some(start);
            while let Some(id) = current {
                if !seen.insert(id) {
                    if let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) {
                        layer.parent = None;
                    }
                    break;
                }
                current = self.layer_by_id(id).and_then(|layer| layer.parent);
            }
        }
    }

    /// Removes a layer and re-parents its direct children to its former folder.
    pub fn remove_layer(&mut self, id: Uuid) -> bool {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return false;
        };
        if self.layers.len() <= 1 {
            return false;
        }
        let former_parent = self.layers[index].parent;
        self.layers.remove(index);
        for layer in &mut self.layers {
            if layer.parent == Some(id) {
                layer.parent = former_parent;
            }
        }
        self.repair_folder_links();
        true
    }

    /// True when `layer` is the folder `ancestor` or one of its descendants.
    pub fn layer_is_descendant_of(&self, layer: Uuid, ancestor: Uuid) -> bool {
        if layer == ancestor {
            return true;
        }
        let mut parent = self.layer_by_id(layer).and_then(|layer| layer.parent);
        let mut guard = 0usize;
        while let Some(id) = parent {
            if id == ancestor {
                return true;
            }
            parent = self.layer_by_id(id).and_then(|layer| layer.parent);
            guard += 1;
            if guard > self.layers.len() {
                break;
            }
        }
        false
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1.0..=8192.0).contains(&self.width) || !(1.0..=8192.0).contains(&self.height) {
            return Err(format!(
                "Szene '{}': Bühnenmaße müssen zwischen 1 und 8192 px liegen",
                self.name
            ));
        }
        if !(1.0..=120.0).contains(&self.fps) {
            return Err(format!(
                "Szene '{}': FPS muss zwischen 1 und 120 liegen",
                self.name
            ));
        }
        if self.layers.is_empty() {
            return Err(format!(
                "Szene '{}': mindestens eine Ebene nötig",
                self.name
            ));
        }
        let ids: Vec<Uuid> = self.layers.iter().map(|layer| layer.id).collect();
        for layer in &self.layers {
            if let Some(parent) = layer.parent {
                if parent == layer.id {
                    return Err(format!(
                        "Ebene '{}' kann nicht ihr eigener Ordner sein",
                        layer.name
                    ));
                }
                let Some(folder) = self.layer_by_id(parent) else {
                    return Err(format!(
                        "Ebene '{}' verweist auf einen unbekannten Ordner",
                        layer.name
                    ));
                };
                if folder.kind != LayerKind::Folder {
                    return Err(format!(
                        "Ebene '{}' ist der Nicht-Ordner-Ebene '{}' untergeordnet",
                        layer.name, folder.name
                    ));
                }
                let mut current = Some(parent);
                let mut guard = 0usize;
                while let Some(id) = current {
                    if id == layer.id {
                        return Err(format!(
                            "Ordnerhierarchie um '{}' enthält einen Zyklus",
                            layer.name
                        ));
                    }
                    current = self.layer_by_id(id).and_then(|candidate| candidate.parent);
                    guard += 1;
                    if guard > ids.len() {
                        return Err(format!(
                            "Ordnerhierarchie um '{}' enthält einen Zyklus",
                            layer.name
                        ));
                    }
                }
            }
        }
        for layer in &self.layers {
            if layer.frames.is_empty() {
                return Err(format!("Ebene '{}' hat keine Frames", layer.name));
            }
            for (frame_index, frame) in layer.frames.iter().enumerate() {
                for object in &frame.objects {
                    if !object.transform.x.is_finite()
                        || !object.transform.y.is_finite()
                        || !object.transform.scale_x.is_finite()
                        || !object.transform.scale_y.is_finite()
                        || !object.transform.rotation.is_finite()
                    {
                        return Err(format!(
                            "Objekt '{}' hat ungültige Transformwerte",
                            object.name
                        ));
                    }
                    if let super::ObjectKind::Bitmap {
                        width,
                        height,
                        crop,
                        ..
                    } = &object.kind
                    {
                        if !width.is_finite()
                            || !height.is_finite()
                            || *width <= 0.0
                            || *height <= 0.0
                        {
                            return Err(format!(
                                "Bitmap-Objekt '{}' hat ungültige Maße",
                                object.name
                            ));
                        }
                        if let Some(crop) = crop
                            && (!crop.x.is_finite()
                                || !crop.y.is_finite()
                                || !crop.width.is_finite()
                                || !crop.height.is_finite()
                                || crop.width <= 0.0
                                || crop.height <= 0.0)
                        {
                            return Err(format!(
                                "Bitmap-Objekt '{}' hat einen ungültigen Ausschnitt",
                                object.name
                            ));
                        }
                    }
                }
                if let Some(cue) = &frame.sound {
                    if !cue.offset.is_finite() || !cue.duration.is_finite() || cue.duration < 0.0 {
                        return Err(format!(
                            "Sound-Cue in Ebene '{}' hat ungültige Zeitwerte",
                            layer.name
                        ));
                    }
                    if layer.kind != LayerKind::Sound {
                        return Err(format!(
                            "Sound-Cue in Ebene '{}' benötigt eine Sound-Ebene",
                            layer.name
                        ));
                    }
                }
                if let Some(tween) = &frame.tween
                    && tween.kind == super::TweenKind::Shape
                {
                    let Some(next_index) = layer.next_keyframe_after(frame_index) else {
                        return Err(format!(
                            "Form-Tween in Ebene '{}' hat kein Ziel-Schlüsselbild",
                            layer.name
                        ));
                    };
                    let next = &layer.frames[next_index];
                    for object in &frame.objects {
                        if let Some(target) = next
                            .objects
                            .iter()
                            .find(|candidate| candidate.id == object.id)
                            && !super::shapes_compatible(&object.kind, &target.kind)
                        {
                            return Err(format!(
                                "Form-Tween für '{}' verbindet inkompatible Formen",
                                object.name
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_hierarchy_validates_and_controls_visibility() {
        let mut scene = Scene::new("Ordner", 100.0, 100.0);
        let mut folder = Layer::new("Grafik", 1);
        folder.kind = LayerKind::Folder;
        let folder_id = folder.id;
        let mut child = Layer::new("Kind", 1);
        child.parent = Some(folder_id);
        child.visible = true;
        let child_id = child.id;
        scene.layers = vec![folder, child];
        scene.validate().unwrap();
        assert_eq!(scene.layer_depth(child_id), 1);
        assert!(scene.layer_effectively_visible(scene.layer_by_id(child_id).unwrap()));

        scene.layers[0].visible = false;
        assert!(!scene.layer_effectively_visible(scene.layer_by_id(child_id).unwrap()));
    }

    #[test]
    fn cycles_and_non_folder_parents_are_rejected() {
        let mut scene = Scene::new("Ordner", 100.0, 100.0);
        let mut first = Layer::new("A", 1);
        first.kind = LayerKind::Folder;
        let mut second = Layer::new("B", 1);
        second.kind = LayerKind::Folder;
        first.parent = Some(second.id);
        second.parent = Some(first.id);
        scene.layers = vec![first, second];
        assert!(scene.validate().is_err());

        let mut plain = Layer::new("C", 1);
        plain.parent = Some(plain.id);
        scene.layers[0].parent = None;
        scene.layers[1].parent = None;
        scene.layers.push(plain);
        assert!(scene.validate().is_err());
    }
}
