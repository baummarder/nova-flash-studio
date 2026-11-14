//! Converts a [`nova_core::Project`] into a real SWF file.
//!
//! Scripts are compiled to AVM2 ABC and embedded with `DoABC2`; display classes
//! are linked through `SymbolClass`. The timeline is baked frame by frame: for
//! every frame all visible objects are resolved (keyframes + motion tweens), the
//! previous placements are removed and the current placements are added. This is
//! larger than an optimized keyframe diff, but it is simple, deterministic and
//! exactly matches what the stage shows.
//!
//! Scripted objects are the exception: they are placed once and then owned by
//! ActionScript, so a per-frame placement cannot reset a scripted transform.
//!
//! Not yet exported: sounds, bitmaps and text. The builder returns explicit
//! warnings instead of silently dropping them. A compiler error aborts the build
//! with source diagnostics; no partial ABC is embedded.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use nova_as::{CompileOptions, CompiledUnit, compile_with_options};
use nova_core::{
    AssetKind, CodeProvenance, LayerKind, ObjectKind, PathCommand, Project, Rgba, Scene, SoundCue,
    SwfCompression, SymbolId, TargetRuntime, resolve_scene,
};
use nova_swf::shape::{Command, Point, Shape, ShapePath};
use nova_swf::types::{Color, Matrix, PlaceObject, Rect as SwfRect};
use nova_swf::{Compression, Movie, Tag};

mod error;

pub use error::BuildError;

#[derive(Debug, Clone, Default)]
pub struct BuildStats {
    pub frames: usize,
    pub placed_objects: usize,
    pub defined_shapes: usize,
    pub defined_sprites: usize,
    pub defined_bitmaps: usize,
    pub defined_sounds: usize,
    pub embedded_scripts: usize,
    pub abc_bytes: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
pub struct BuildOutput {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
    pub stats: BuildStats,
}

struct CompiledScript {
    id: uuid::Uuid,
    path: String,
    index: usize,
    linked_object: Option<uuid::Uuid>,
    unit: CompiledUnit,
    /// ABC bytes were preserved from an import; no class metadata is available.
    preserved: bool,
}

/// Builds the active scene (or, when configured, all scenes) of `project` as an
/// SWF byte stream.
pub fn build_swf(project: &Project) -> Result<BuildOutput, BuildError> {
    project.validate()?;
    let scene = project.active_scene().ok_or(BuildError::NoScene)?;
    let export = &project.export;

    let mut warnings = Vec::new();
    let export_scenes: Vec<&Scene> = if export.export_all_scenes {
        project.scenes.iter().collect()
    } else {
        vec![scene]
    };
    if project.scenes.len() > 1 && !export.export_all_scenes {
        warnings.push(format!(
            "Es wird nur die aktive Szene '{}' exportiert ({} weitere Szenen ignoriert). Aktiviere 'Alle Szenen aneinanderhängen', um alle zu exportieren.",
            scene.name,
            project.scenes.len() - 1
        ));
    }
    if export.export_all_scenes {
        for other in export_scenes.iter().skip(1) {
            if (other.width - scene.width).abs() > 0.01
                || (other.height - scene.height).abs() > 0.01
            {
                warnings.push(format!(
                    "Szene '{}' hat andere Bühnenmaße als '{}'; der SWF-Export verwendet {}×{} px.",
                    other.name, scene.name, scene.width, scene.height
                ));
            }
            if (other.fps - scene.fps).abs() > 0.01 {
                warnings.push(format!(
                    "Szene '{}' hat {} fps statt {} fps; die SWF-Timeline verwendet die FPS der ersten Szene.",
                    other.name, other.fps, scene.fps
                ));
            }
            if other.background != scene.background {
                warnings.push(format!(
                    "Szene '{}' hat einen anderen Hintergrund; SWF kennt nur eine Bühnenfarbe pro Film.",
                    other.name
                ));
            }
        }
    }

    let scripts_enabled = export.include_scripts && export.target != TargetRuntime::Avm1;
    let mut compiled_scripts = Vec::new();
    if !project.scripts.is_empty() && !scripts_enabled {
        warnings.push(format!(
            "ActionScript-Export ist deaktiviert: {} Skript(e) wurden nicht in die SWF eingebettet.",
            project.scripts.len()
        ));
    }
    if export.target == TargetRuntime::Avm1 {
        warnings.push(
            "AVM1-Ziel gewählt: Die Datei enthält keine Skripte und keine AVM1-Bytecode-Tags."
                .into(),
        );
    }
    if scripts_enabled {
        for (index, script) in project.scripts.iter().enumerate() {
            if let Some(abc) = &script.abc {
                if abc.is_empty() {
                    return Err(BuildError::ActionScript(format!(
                        "{}: Preserved ABC data is empty.",
                        script.path
                    )));
                }
                warnings.push(format!(
                    "{}: {} wird unverändert eingebettet. Der Quelltext ist keine Originalquelle und wird nicht neu kompiliert.",
                    script.path,
                    script.provenance.label()
                ));
                compiled_scripts.push(CompiledScript {
                    id: script.id,
                    path: script.path.clone(),
                    index,
                    linked_object: script.linked_object,
                    unit: CompiledUnit {
                        abc: abc.clone(),
                        classes: Vec::new(),
                        diagnostics: Vec::new(),
                    },
                    preserved: true,
                });
                continue;
            }
            if script.provenance != CodeProvenance::OriginalSource {
                warnings.push(format!(
                    "{}: Quelltext ist als '{}' markiert und wird als Quelltext kompiliert.",
                    script.path,
                    script.provenance.label()
                ));
            }
            let compiled = compile_with_options(&script.source, CompileOptions::default());
            for diagnostic in &compiled.diagnostics {
                let rendered = diagnostic.render(&script.source, &script.path);
                if diagnostic.severity == nova_as::Severity::Error {
                    return Err(BuildError::ActionScript(rendered));
                }
                warnings.push(rendered);
            }
            if compiled.abc.is_empty() {
                return Err(BuildError::ActionScript(format!(
                    "{}: Der Compiler hat keine ABC-Daten erzeugt.",
                    script.path
                )));
            }
            compiled_scripts.push(CompiledScript {
                id: script.id,
                path: script.path.clone(),
                index,
                linked_object: script.linked_object,
                unit: compiled,
                preserved: false,
            });
        }
    }

    let has_scripts = !compiled_scripts.is_empty();
    let requested_version = export.swf_version;
    let swf_version = if has_scripts {
        requested_version.max(10)
    } else {
        requested_version
    };
    if has_scripts && swf_version != requested_version {
        warnings.push(format!(
            "SWF-Version von {requested_version} auf {swf_version} angehoben (DoABC2 benötigt Version 10)."
        ));
    }

    let mut movie = match export.target {
        TargetRuntime::Avm2 => Movie::new_avm2(scene.width, scene.height, scene.fps, swf_version),
        TargetRuntime::Avm1 => {
            Movie::new_avm1(scene.width, scene.height, scene.fps, swf_version.min(8))
        }
    };
    movie.set_background(to_color(scene.background));

    // Concatenate scenes when requested. Scene names become frame labels at
    // their first frame unless that frame already has an explicit label.
    let mut resolved: Vec<Vec<nova_core::ResolvedObject>> = Vec::new();
    let mut frame_labels: Vec<Option<String>> = Vec::new();
    let mut frame_sounds: Vec<Vec<(Uuid, SoundCue)>> = Vec::new();
    for (scene_index, current) in export_scenes.iter().enumerate() {
        for frame in 0..current.frame_count() {
            resolved.push(resolve_scene(current, frame));
            let explicit = current
                .layers
                .iter()
                .filter_map(|layer| layer.frames.get(frame))
                .find_map(|frame| frame.label.clone());
            let label = if frame == 0 && scene_index > 0 {
                Some(explicit.unwrap_or_else(|| current.name.clone()))
            } else {
                explicit
            };
            frame_labels.push(label);
            let mut cues = Vec::new();
            for layer in &current.layers {
                if let Some(cue) = layer.frames.get(frame).and_then(|frame| frame.sound) {
                    cues.push((layer.id, cue));
                }
            }
            frame_sounds.push(cues);
        }
    }

    let mut next_id: u16 = 1;
    let mut stats = BuildStats {
        frames: resolved.len(),
        ..Default::default()
    };

    let mut characters = CharacterTables::default();
    // Library symbols are real sprite characters with their own timelines. The
    // recursive definition order also defines nested dependencies first.
    for symbol in &project.symbols {
        characters.define_symbol(project, symbol.id, &mut movie, &mut warnings, &mut stats)?;
    }
    characters.next_id = next_id.max(characters.next_id);

    // Remember which character every root instance gets so the per-frame
    // placement only has to look up the map.
    let mut instance_characters: HashMap<uuid::Uuid, u16> = HashMap::new();
    for objects in &resolved {
        for object in objects {
            if let Some(character) = characters.character_for(
                project,
                &object.instance,
                &mut movie,
                &mut warnings,
                &mut stats,
            )? {
                instance_characters.insert(object.instance.id, character);
            }
        }
    }
    next_id = characters.next_id;

    // Link compiled classes to SWF characters. Objects with an assigned script
    // are wrapped in a one-frame sprite so the class can extend MovieClip and
    // control its own transform/timeline. A script that is not assigned to an
    // object may become the document class of the main timeline (character 0).
    let mut symbol_links: Vec<(u16, String)> = Vec::new();
    // Objects that received a usable script/class link. Their transform is
    // owned by ActionScript after the first frame, so the baked timeline must
    // not overwrite it on every frame.
    let mut scripted_objects: HashSet<uuid::Uuid> = HashSet::new();
    if has_scripts {
        // Preserved SWF imports carry the original SymbolClass table. The
        // character ids in that table refer to the imported SWF, so map them to
        // reconstructed objects through `ObjectInstance::source_character` and
        // wrap the first matching object in a fresh sprite character.
        let preserved_symbol_classes = project
            .source
            .as_ref()
            .map(|source| source.symbol_classes.clone())
            .unwrap_or_default();
        let has_preserved_abc = compiled_scripts.iter().any(|script| script.preserved);
        if has_preserved_abc {
            for link in &preserved_symbol_classes {
                if link.character_id == 0 {
                    symbol_links.push((0, link.class_name.clone()));
                    continue;
                }
                let Some(object_id) = resolved
                    .iter()
                    .flatten()
                    .find(|object| object.instance.source_character == Some(link.character_id))
                    .map(|object| object.instance.id)
                else {
                    warnings.push(format!(
                        "SymbolClass '{}': Charakter {} ist in der rekonstruierten Szene nicht sichtbar; die Klasse wird nicht verknüpft.",
                        link.class_name, link.character_id
                    ));
                    continue;
                };
                let Some(&character) = instance_characters.get(&object_id) else {
                    warnings.push(format!(
                        "SymbolClass '{}': Das rekonstruierte Objekt ist nicht exportierbar; die Klasse wird nicht verknüpft.",
                        link.class_name
                    ));
                    continue;
                };
                let sprite_id = next_id;
                next_id = next_character_id(next_id)?;
                movie.tags.push(Tag::DefineSprite {
                    id: sprite_id,
                    frame_count: 1,
                    tags: vec![
                        Tag::PlaceObject2(
                            PlaceObject::new(1)
                                .character(character)
                                .matrix(Matrix::default()),
                        ),
                        Tag::ShowFrame,
                    ],
                });
                instance_characters.insert(object_id, sprite_id);
                scripted_objects.insert(object_id);
                stats.defined_sprites += 1;
                symbol_links.push((sprite_id, link.class_name.clone()));
            }
        }

        let compiled_ids: HashSet<uuid::Uuid> =
            compiled_scripts.iter().map(|script| script.id).collect();
        let mut object_script_ids: HashMap<uuid::Uuid, uuid::Uuid> = HashMap::new();
        for objects in &resolved {
            for object in objects {
                if let Some(script) = object.instance.script {
                    if !compiled_ids.contains(&script) {
                        warnings.push(format!(
                            "Objekt '{}' verweist auf ein nicht vorhandenes Skript ({}).",
                            object.instance.name, script
                        ));
                        continue;
                    }
                    object_script_ids
                        .entry(object.instance.id)
                        .or_insert(script);
                }
            }
        }

        let mut script_targets: HashMap<uuid::Uuid, Vec<uuid::Uuid>> = HashMap::new();
        for (object_id, script_id) in &object_script_ids {
            script_targets
                .entry(*script_id)
                .or_default()
                .push(*object_id);
        }
        for script in &compiled_scripts {
            let Some(object_id) = script.linked_object else {
                continue;
            };
            if let Some(existing) = object_script_ids.get(&object_id) {
                if *existing != script.id {
                    warnings.push(format!(
                        "{}: Das verknüpfte Objekt ist bereits einem anderen Skript zugewiesen; die direkte Objektzuweisung hat Vorrang.",
                        script.path
                    ));
                    continue;
                }
            }
            let targets = script_targets.entry(script.id).or_default();
            if !targets.contains(&object_id) {
                targets.push(object_id);
            }
        }

        let mut linked_scripts: HashSet<uuid::Uuid> = HashSet::new();
        for script in &compiled_scripts {
            let Some(targets) = script_targets.get(&script.id).cloned() else {
                continue;
            };
            if targets.is_empty() {
                continue;
            }
            let Some(class_name) = choose_link_class(script, &mut warnings) else {
                continue;
            };
            let mut targets = targets;
            targets.sort();
            targets.dedup();
            for object_id in targets {
                if object_script_ids
                    .get(&object_id)
                    .is_some_and(|assigned| *assigned != script.id)
                {
                    continue;
                }
                if scripted_objects.contains(&object_id) {
                    warnings.push(format!(
                        "{}: Mehrere Skripte sind demselben Objekt zugewiesen; nur die erste Zuordnung wird exportiert.",
                        script.path
                    ));
                    continue;
                }
                let Some(&character) = instance_characters.get(&object_id) else {
                    warnings.push(format!(
                        "{}: Das zugewiesene Objekt ist in den exportierten Szenen nicht sichtbar und wurde nicht verknüpft.",
                        script.path
                    ));
                    continue;
                };
                let sprite_id = next_id;
                next_id = next_character_id(next_id)?;
                movie.tags.push(Tag::DefineSprite {
                    id: sprite_id,
                    frame_count: 1,
                    tags: vec![
                        Tag::PlaceObject2(
                            PlaceObject::new(1)
                                .character(character)
                                .matrix(Matrix::default()),
                        ),
                        Tag::ShowFrame,
                    ],
                });
                instance_characters.insert(object_id, sprite_id);
                scripted_objects.insert(object_id);
                stats.defined_sprites += 1;
                symbol_links.push((sprite_id, class_name.clone()));
                linked_scripts.insert(script.id);
            }
        }

        let mut roots: Vec<(String, String)> = Vec::new();
        for script in &compiled_scripts {
            if linked_scripts.contains(&script.id) {
                continue;
            }
            if script.preserved && !preserved_symbol_classes.is_empty() {
                // The imported SymbolClass table already describes the class
                // relationships. Do not invent an additional document class.
                continue;
            }
            let Some(class_name) = choose_root_class(script) else {
                continue;
            };
            roots.push((class_name, script.path.clone()));
        }
        roots.sort_by(|left, right| left.0.cmp(&right.0));
        roots.dedup_by(|left, right| left.0 == right.0);
        match roots.as_slice() {
            [(class_name, _)] => symbol_links.push((0, class_name.clone())),
            [] => {
                if symbol_links.is_empty() {
                    warnings.push(
                        "ABC wurde eingebettet, aber keine Klasse konnte per SymbolClass verknüpft werden; die Skripte werden nicht instanziiert."
                            .into(),
                    );
                }
            }
            _ => {
                let names = roots
                    .iter()
                    .map(|(class_name, path)| format!("{class_name} ({path})"))
                    .collect::<Vec<_>>()
                    .join(", ");
                warnings.push(format!(
                    "Mehrere Dokumentklassen-Kandidaten gefunden ({names}); es wird keine Hauptklasse verknüpft."
                ));
            }
        }

        let mut abc_names: HashSet<String> = HashSet::new();
        for script in &compiled_scripts {
            let base = if script.path.is_empty() {
                format!("script{}", script.index)
            } else {
                script.path.clone()
            };
            let mut name = base.clone();
            let mut suffix = 1;
            while !abc_names.insert(name.clone()) {
                name = format!("{base}#{suffix}");
                suffix += 1;
            }
            movie.tags.push(Tag::DoAbc2 {
                // Lazy initialization; the SymbolClass lookup of a linked class
                // triggers the script initializer. No runtime has confirmed this
                // ordering yet, so the limitation is documented.
                flags: 1,
                name,
                data: script.unit.abc.clone(),
            });
            stats.embedded_scripts += 1;
            stats.abc_bytes += script.unit.abc.len();
        }

        if !symbol_links.is_empty() {
            symbol_links.sort_by_key(|(character, _)| *character);
            movie.tags.push(Tag::SymbolClass {
                links: symbol_links,
            });
        }
    }

    // Scripted objects are placed once and then owned by ActionScript. They get
    // fixed depths above the baked timeline so a per-frame PlaceObject cannot
    // overwrite the transform/alpha the script has set.
    let scripted_depth_base = resolved.iter().map(Vec::len).max().unwrap_or(0) as u32 + 1;
    let mut next_scripted_depth = scripted_depth_base;
    let mut scripted_depths: HashMap<uuid::Uuid, u16> = HashMap::new();
    let mut active_scripted: HashSet<uuid::Uuid> = HashSet::new();

    let mut previous_depths = 0usize;
    let mut warned_timeline_sound = false;
    let mut warned_mask_target = false;
    for (frame_index, objects) in resolved.iter().enumerate() {
        for depth in 1..=previous_depths {
            movie.tags.push(Tag::RemoveObject2 {
                depth: depth as u16,
            });
        }

        if let Some(cues) = frame_sounds.get(frame_index) {
            for (_layer_id, cue) in cues {
                if let Some(sound_id) = characters.define_sound(
                    project,
                    cue.asset,
                    &mut movie,
                    &mut warnings,
                    &mut stats,
                )? {
                    let rate = characters
                        .sound_sample_rates
                        .get(&cue.asset)
                        .copied()
                        .unwrap_or(44_100);
                    let in_point = (cue.offset > 0.0)
                        .then(|| (cue.offset * rate as f32).round().max(0.0) as u32);
                    let out_point = (cue.duration > 0.0).then(|| {
                        ((cue.offset.max(0.0) + cue.duration) * rate as f32)
                            .round()
                            .max(0.0) as u32
                    });
                    if !cue.event && !warned_timeline_sound {
                        warnings.push(
                            "Timeline-Sounds werden in dieser Version als StartSound-Ereignis exportiert; eine echte SoundStream-Kompression ist nicht implementiert."
                                .into(),
                        );
                        warned_timeline_sound = true;
                    }
                    movie.tags.push(Tag::StartSound {
                        sound_id,
                        loop_count: cue.loop_count.max(1),
                        in_point,
                        out_point,
                    });
                }
            }
        }

        let current_scripted: HashSet<uuid::Uuid> = objects
            .iter()
            .map(|object| object.instance.id)
            .filter(|id| scripted_objects.contains(id))
            .collect();
        let disappeared: Vec<uuid::Uuid> = active_scripted
            .iter()
            .copied()
            .filter(|id| !current_scripted.contains(id))
            .collect();
        for id in disappeared {
            if let Some(depth) = scripted_depths.get(&id) {
                movie.tags.push(Tag::RemoveObject2 { depth: *depth });
            }
            active_scripted.remove(&id);
        }

        // Frame labels defined on any layer's keyframe at this position, plus
        // scene boundaries for concatenated exports.
        if let Some(label) = frame_labels.get(frame_index).cloned().flatten() {
            movie.tags.push(Tag::FrameLabel {
                label,
                named_anchor: true,
            });
        }

        let mut used_names: HashMap<String, u32> = HashMap::new();
        let mut next_depth: u16 = 1;
        let mut layer_depths: HashMap<usize, u16> = HashMap::new();
        for object in objects {
            let id = object.instance.id;
            let Some(&character) = instance_characters.get(&id) else {
                continue;
            };
            if scripted_objects.contains(&id) {
                if active_scripted.contains(&id) {
                    continue;
                }
                let depth = if let Some(depth) = scripted_depths.get(&id) {
                    *depth
                } else {
                    let depth = u16::try_from(next_scripted_depth).map_err(|_| {
                        BuildError::Limit("SWF-Tiefe für Skriptobjekte überschritten".into())
                    })?;
                    next_scripted_depth += 1;
                    scripted_depths.insert(id, depth);
                    depth
                };
                let name = unique_name(&object.instance.name, depth, &mut used_names);
                let transform = object.instance.transform;
                let matrix = Matrix::from_transform(
                    transform.scale_x,
                    transform.scale_y,
                    transform.rotation,
                    transform.x,
                    transform.y,
                );
                movie.tags.push(Tag::PlaceObject2(
                    PlaceObject::new(depth)
                        .character(character)
                        .matrix(matrix)
                        .named(name),
                ));
                active_scripted.insert(id);
                stats.placed_objects += 1;
                continue;
            }

            let depth = next_depth;
            next_depth = next_depth.checked_add(1).ok_or_else(|| {
                BuildError::Limit("SWF-Tiefe der exportierten Szene überschritten".into())
            })?;
            let name = unique_name(&object.instance.name, depth, &mut used_names);
            let transform = object.instance.transform;
            let matrix = Matrix::from_transform(
                transform.scale_x,
                transform.scale_y,
                transform.rotation,
                transform.x,
                transform.y,
            );
            let mut place = PlaceObject::new(depth)
                .character(character)
                .matrix(matrix)
                .named(name);
            if object.layer_kind == LayerKind::Mask {
                let target_depth = object
                    .layer_index
                    .checked_sub(1)
                    .and_then(|target| layer_depths.get(&target).copied());
                if let Some(target_depth) = target_depth {
                    place.clip_depth = Some(target_depth);
                } else if !warned_mask_target {
                    warnings.push(
                        "Eine Maskenebene hat keine sichtbare Ebene direkt darunter; die Maske wird ohne ClipDepth exportiert."
                            .into(),
                    );
                    warned_mask_target = true;
                }
            }
            movie.tags.push(Tag::PlaceObject2(place));
            stats.placed_objects += 1;
            layer_depths.insert(object.layer_index, depth);
        }
        previous_depths = next_depth.saturating_sub(1) as usize;
        movie.tags.push(Tag::ShowFrame);
    }

    let compression = match export.compression {
        SwfCompression::None => Compression::None,
        SwfCompression::Zlib => Compression::Zlib,
    };
    let bytes = movie.to_bytes(compression)?;
    stats.bytes = bytes.len();

    Ok(BuildOutput {
        bytes,
        warnings,
        stats,
    })
}

/// Builds a scene that is not necessarily the active one; useful for previews.
pub fn build_scene(project: &Project, scene: &Scene) -> Result<BuildOutput, BuildError> {
    let mut copy = project.clone();
    copy.active_scene = Some(scene.id);
    build_swf(&copy)
}

#[derive(Default)]
struct CharacterTables {
    next_id: u16,
    symbols: HashMap<SymbolId, u16>,
    symbols_in_progress: HashSet<SymbolId>,
    shapes: HashMap<ShapeKey, u16>,
    bitmaps: HashMap<(uuid::Uuid, i32, i32, i32, i32), u16>,
    sounds: HashMap<uuid::Uuid, u16>,
    failed_sounds: HashSet<uuid::Uuid>,
    sound_sample_rates: HashMap<uuid::Uuid, u32>,
    warned_buttons: HashSet<SymbolId>,
}

impl CharacterTables {
    fn alloc(&mut self) -> Result<u16, BuildError> {
        if self.next_id == 0 {
            self.next_id = 1;
        }
        let id = self.next_id;
        self.next_id = next_character_id(self.next_id)?;
        Ok(id)
    }

    fn character_for(
        &mut self,
        project: &Project,
        object: &nova_core::ObjectInstance,
        movie: &mut Movie,
        warnings: &mut Vec<String>,
        stats: &mut BuildStats,
    ) -> Result<Option<u16>, BuildError> {
        match &object.kind {
            ObjectKind::Rect { .. } | ObjectKind::Ellipse { .. } => {
                let key = shape_key(object).expect("primitive shape");
                if let Some(id) = self.shapes.get(&key) {
                    return Ok(Some(*id));
                }
                let id = self.alloc()?;
                movie.tags.push(Tag::DefineShape {
                    id,
                    shape: key.to_shape(),
                });
                self.shapes.insert(key, id);
                stats.defined_shapes += 1;
                Ok(Some(id))
            }
            ObjectKind::Path {
                width,
                height,
                paths,
            } => {
                let id = self.alloc()?;
                movie.tags.push(Tag::DefineShape {
                    id,
                    shape: path_shape(*width, *height, paths),
                });
                stats.defined_shapes += 1;
                Ok(Some(id))
            }
            ObjectKind::Text(text) => {
                let id = self.alloc()?;
                let (width, height) = text.metrics();
                movie.tags.push(Tag::DefineShape {
                    id,
                    shape: path_shape(width, height, &text.paths(object.fill)),
                });
                stats.defined_shapes += 1;
                Ok(Some(id))
            }
            ObjectKind::Bitmap { asset, crop, .. } => {
                self.define_bitmap(project, *asset, *crop, movie, warnings, stats)
            }
            ObjectKind::Symbol { symbol } => {
                self.define_symbol(project, *symbol, movie, warnings, stats)?;
                Ok(self.symbols.get(symbol).copied())
            }
        }
    }

    fn define_bitmap(
        &mut self,
        project: &Project,
        asset_id: uuid::Uuid,
        crop: Option<nova_core::BitmapCrop>,
        movie: &mut Movie,
        warnings: &mut Vec<String>,
        stats: &mut BuildStats,
    ) -> Result<Option<u16>, BuildError> {
        let crop = crop.unwrap_or(nova_core::BitmapCrop::FULL).normalized();
        let key = (
            asset_id,
            (crop.x * 10_000.0).round() as i32,
            (crop.y * 10_000.0).round() as i32,
            (crop.width * 10_000.0).round() as i32,
            (crop.height * 10_000.0).round() as i32,
        );
        if let Some(id) = self.bitmaps.get(&key) {
            return Ok(Some(*id));
        }
        let Some(asset) = project.asset(asset_id) else {
            warnings.push(
                "Bitmap-Objekt verweist auf ein unbekanntes Asset und wurde übersprungen.".into(),
            );
            return Ok(None);
        };
        if asset.kind != AssetKind::Bitmap {
            warnings.push(format!(
                "Asset '{}' ist kein Bitmap und wurde für die Platzierung übersprungen.",
                asset.name
            ));
            return Ok(None);
        }
        if asset.data.is_empty() {
            warnings.push(format!(
                "Bitmap-Asset '{}' enthält keine eingebetteten Daten und wurde übersprungen.",
                asset.name
            ));
            return Ok(None);
        }
        let full_crop = crop.is_full();
        let id = if full_crop && asset.mime.eq_ignore_ascii_case("image/jpeg") {
            let id = self.alloc()?;
            movie.tags.push(Tag::DefineBitsJPEG2 {
                id,
                data: asset.data.clone(),
            });
            id
        } else {
            let image = image::load_from_memory(&asset.data)
                .map_err(|error| BuildError::Asset(format!("Bitmap '{}': {error}", asset.name)))?;
            let rgba = image.to_rgba8();
            let (source_width, source_height) = rgba.dimensions();
            let x = (crop.x * source_width as f32).floor() as u32;
            let y = (crop.y * source_height as f32).floor() as u32;
            let width = (crop.width * source_width as f32).round().max(1.0) as u32;
            let height = (crop.height * source_height as f32).round().max(1.0) as u32;
            let width = width.min(source_width.saturating_sub(x)).max(1);
            let height = height.min(source_height.saturating_sub(y)).max(1);
            let cropped = image::imageops::crop_imm(
                &rgba,
                x.min(source_width - 1),
                y.min(source_height - 1),
                width,
                height,
            )
            .to_image();
            let width = u16::try_from(cropped.width())
                .map_err(|_| BuildError::Limit("Bitmap ist breiter als 65535 Pixel".into()))?;
            let height = u16::try_from(cropped.height())
                .map_err(|_| BuildError::Limit("Bitmap ist höher als 65535 Pixel".into()))?;
            let id = self.alloc()?;
            movie.tags.push(Tag::DefineBitsLossless2 {
                id,
                width,
                height,
                rgba: cropped.into_raw(),
            });
            id
        };
        self.bitmaps.insert(key, id);
        stats.defined_bitmaps += 1;
        Ok(Some(id))
    }

    fn define_sound(
        &mut self,
        project: &Project,
        asset_id: uuid::Uuid,
        movie: &mut Movie,
        warnings: &mut Vec<String>,
        stats: &mut BuildStats,
    ) -> Result<Option<u16>, BuildError> {
        if let Some(id) = self.sounds.get(&asset_id) {
            return Ok(Some(*id));
        }
        if self.failed_sounds.contains(&asset_id) {
            return Ok(None);
        }
        let Some(asset) = project.asset(asset_id) else {
            warnings.push("Sound-Cue verweist auf ein unbekanntes Asset.".into());
            self.failed_sounds.insert(asset_id);
            return Ok(None);
        };
        if asset.kind != AssetKind::Audio || asset.data.is_empty() {
            warnings.push(format!(
                "Audio-Asset '{}' enthält keine exportierbaren Daten und wurde übersprungen.",
                asset.name
            ));
            self.failed_sounds.insert(asset_id);
            return Ok(None);
        }
        let Some(payload) = sound_payload(asset, warnings) else {
            self.failed_sounds.insert(asset_id);
            return Ok(None);
        };
        let id = self.alloc()?;
        movie.tags.push(Tag::DefineSound {
            id,
            format: payload.format,
            rate: payload.rate,
            size_16bit: true,
            stereo: payload.stereo,
            sample_count: payload.sample_count,
            data: payload.data,
        });
        self.sounds.insert(asset_id, id);
        self.sound_sample_rates
            .insert(asset_id, payload.sample_rate);
        stats.defined_sounds += 1;
        Ok(Some(id))
    }

    fn define_symbol(
        &mut self,
        project: &Project,
        symbol_id: SymbolId,
        movie: &mut Movie,
        warnings: &mut Vec<String>,
        stats: &mut BuildStats,
    ) -> Result<u16, BuildError> {
        if let Some(id) = self.symbols.get(&symbol_id) {
            return Ok(*id);
        }
        if !self.symbols_in_progress.insert(symbol_id) {
            return Err(BuildError::Limit(
                "Rekursiver Symbol-Zyklus beim SWF-Export erkannt".into(),
            ));
        }
        let Some(symbol) = project.symbol(symbol_id) else {
            self.symbols_in_progress.remove(&symbol_id);
            return Err(BuildError::Limit(format!(
                "Symbol {symbol_id} existiert nicht"
            )));
        };
        if symbol.kind == nova_core::SymbolKind::Button && self.warned_buttons.insert(symbol_id) {
            warnings.push(format!(
                "Button-Symbol '{}' wird als MovieClip-Sprite exportiert; Button-Zustände und Hit-Tests sind nicht implementiert.",
                symbol.name
            ));
        }
        let frame_count = symbol.frame_count().max(1);
        let mut tags = Vec::new();
        let mut previous_depths = 0usize;
        let mut warned_timeline_sound = false;
        let mut warned_mask_target = false;
        for frame_index in 0..frame_count {
            for depth in 1..=previous_depths {
                tags.push(Tag::RemoveObject2 {
                    depth: depth as u16,
                });
            }
            if let Some(label) = symbol
                .timeline
                .layers
                .iter()
                .filter_map(|layer| layer.frames.get(frame_index))
                .find_map(|frame| frame.label.clone())
            {
                tags.push(Tag::FrameLabel {
                    label,
                    named_anchor: false,
                });
            }
            for layer in &symbol.timeline.layers {
                if let Some(cue) = layer.frames.get(frame_index).and_then(|frame| frame.sound) {
                    if let Some(sound_id) =
                        self.define_sound(project, cue.asset, movie, warnings, stats)?
                    {
                        let rate = self
                            .sound_sample_rates
                            .get(&cue.asset)
                            .copied()
                            .unwrap_or(44_100);
                        let in_point = (cue.offset > 0.0)
                            .then(|| (cue.offset * rate as f32).round().max(0.0) as u32);
                        let out_point = (cue.duration > 0.0).then(|| {
                            ((cue.offset.max(0.0) + cue.duration) * rate as f32)
                                .round()
                                .max(0.0) as u32
                        });
                        if !cue.event && !warned_timeline_sound {
                            warnings.push(format!(
                                "Timeline-Sound im Symbol '{}' wird als StartSound-Ereignis exportiert; echtes SoundStreaming ist nicht implementiert.",
                                symbol.name
                            ));
                            warned_timeline_sound = true;
                        }
                        tags.push(Tag::StartSound {
                            sound_id,
                            loop_count: cue.loop_count.max(1),
                            in_point,
                            out_point,
                        });
                    }
                }
            }
            let objects = resolve_scene(&symbol.timeline, frame_index);
            let mut next_depth: u16 = 1;
            let mut layer_depths: HashMap<usize, u16> = HashMap::new();
            for object in &objects {
                let Some(character) =
                    self.character_for(project, &object.instance, movie, warnings, stats)?
                else {
                    continue;
                };
                let depth = next_depth;
                next_depth = next_depth.checked_add(1).ok_or_else(|| {
                    BuildError::Limit("SWF-Tiefe eines Symbols überschritten".into())
                })?;
                let transform = object.instance.transform;
                let matrix = Matrix::from_transform(
                    transform.scale_x,
                    transform.scale_y,
                    transform.rotation,
                    transform.x,
                    transform.y,
                );
                let mut place = PlaceObject::new(depth).character(character).matrix(matrix);
                if object.layer_kind == LayerKind::Mask {
                    let target = object
                        .layer_index
                        .checked_sub(1)
                        .and_then(|target| layer_depths.get(&target).copied());
                    if let Some(target) = target {
                        place.clip_depth = Some(target);
                    } else if !warned_mask_target {
                        warnings.push(format!(
                            "Maske im Symbol '{}' hat keine sichtbare Ebene direkt darunter; kein ClipDepth.",
                            symbol.name
                        ));
                        warned_mask_target = true;
                    }
                }
                tags.push(Tag::PlaceObject2(place));
                stats.placed_objects += 1;
                layer_depths.insert(object.layer_index, depth);
            }
            previous_depths = next_depth.saturating_sub(1) as usize;
            tags.push(Tag::ShowFrame);
        }
        let sprite_id = self.alloc()?;
        movie.tags.push(Tag::DefineSprite {
            id: sprite_id,
            frame_count: frame_count as u16,
            tags,
        });
        self.symbols.insert(symbol_id, sprite_id);
        self.symbols_in_progress.remove(&symbol_id);
        stats.defined_sprites += 1;
        Ok(sprite_id)
    }
}

struct SoundPayload {
    format: u8,
    rate: u8,
    stereo: bool,
    sample_rate: u32,
    sample_count: u32,
    data: Vec<u8>,
}

fn sound_rate_index(sample_rate: u32) -> u8 {
    match sample_rate {
        0..=7_999 => 0,
        8_000..=15_999 => 1,
        16_000..=31_999 => 2,
        _ => 3,
    }
}

fn sound_payload(asset: &nova_core::Asset, warnings: &mut Vec<String>) -> Option<SoundPayload> {
    let data = &asset.data;
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let mut offset = 12usize;
        let mut channels = 0u16;
        let mut sample_rate = 0u32;
        let mut bits = 0u16;
        let mut audio_format = 0u16;
        let mut pcm: Option<&[u8]> = None;
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
                audio_format = u16::from_le_bytes([data[body], data[body + 1]]);
                channels = u16::from_le_bytes([data[body + 2], data[body + 3]]);
                sample_rate = u32::from_le_bytes([
                    data[body + 4],
                    data[body + 5],
                    data[body + 6],
                    data[body + 7],
                ]);
                bits = u16::from_le_bytes([data[body + 14], data[body + 15]]);
            } else if id == b"data" {
                pcm = Some(&data[body..body + size]);
            }
            offset = body + size + (size & 1);
        }
        if audio_format != 1 || bits != 16 || channels == 0 || sample_rate == 0 {
            warnings.push(format!(
                "WAV '{}' wird nicht exportiert: nur PCM mit 16 Bit wird unterstützt (Format {}, {} Bit).",
                asset.name, audio_format, bits
            ));
            return None;
        }
        let pcm = pcm?;
        let stereo = channels > 1;
        let bytes_per_sample = 2usize * channels as usize;
        Some(SoundPayload {
            format: 0,
            rate: sound_rate_index(sample_rate),
            stereo,
            sample_rate,
            sample_count: (pcm.len() / bytes_per_sample) as u32,
            data: pcm.to_vec(),
        })
    } else if data.len() > 4 {
        let start = data
            .windows(2)
            .position(|window| window[0] == 0xFF && (window[1] & 0xE0) == 0xE0)
            .unwrap_or(0);
        let mp3 = &data[start..];
        if mp3.len() < 4 {
            warnings.push(format!("MP3 '{}' ist zu kurz für einen Frame.", asset.name));
            return None;
        }
        let header = u32::from_be_bytes([mp3[0], mp3[1], mp3[2], mp3[3]]);
        let sample_rate = match (header >> 10) & 0x03 {
            0 => 44_100,
            1 => 48_000,
            2 => 32_000,
            _ => 22_050,
        };
        let stereo = ((header >> 6) & 0x03) != 3;
        let duration = if asset.duration.is_finite() && asset.duration > 0.0 {
            asset.duration
        } else {
            let bitrate_index = ((header >> 12) & 0x0F) as usize;
            const MP3_BITRATES: [u32; 16] = [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
            ];
            let bitrate = MP3_BITRATES.get(bitrate_index).copied().unwrap_or(128) * 1000;
            if bitrate == 0 {
                0.0
            } else {
                (mp3.len() as f64 * 8.0 / bitrate as f64) as f32
            }
        };
        if duration <= 0.0 {
            warnings.push(format!(
                "MP3 '{}': Dauer konnte nicht bestimmt werden; der Sound wird nicht exportiert.",
                asset.name
            ));
            return None;
        }
        Some(SoundPayload {
            format: 2,
            rate: sound_rate_index(sample_rate),
            stereo,
            sample_rate,
            sample_count: (duration * sample_rate as f32).round() as u32,
            data: mp3.to_vec(),
        })
    } else {
        warnings.push(format!(
            "Audio-Asset '{}' hat kein erkanntes WAV/MP3-Format und wird nicht exportiert.",
            asset.name
        ));
        None
    }
}

fn next_character_id(current: u16) -> Result<u16, BuildError> {
    current
        .checked_add(1)
        .ok_or_else(|| BuildError::Limit("SWF-Charakter-ID 65535 überschritten".into()))
}

fn script_file_stem(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let file = file.strip_suffix(".as").unwrap_or(file);
    file.strip_suffix(".AS").unwrap_or(file).to_string()
}

fn class_is_display(class: &nova_as::CompiledClass, classes: &[nova_as::CompiledClass]) -> bool {
    fn base_is_display(base: &str) -> bool {
        let simple = base.rsplit('.').next().unwrap_or(base);
        simple == "MovieClip" || simple == "Sprite"
    }
    fn walk(
        class: &nova_as::CompiledClass,
        classes: &[nova_as::CompiledClass],
        depth: usize,
    ) -> bool {
        if class.extends_movieclip {
            return true;
        }
        let Some(base) = class.base.as_deref() else {
            return false;
        };
        if base_is_display(base) {
            return true;
        }
        if depth > 32 {
            return false;
        }
        classes
            .iter()
            .find(|candidate| {
                candidate.qualified_name == base
                    || candidate.name == base
                    || candidate.name == base.rsplit('.').next().unwrap_or(base)
            })
            .is_some_and(|base_class| walk(base_class, classes, depth + 1))
    }
    walk(class, classes, 0)
}

fn choose_link_class(script: &CompiledScript, warnings: &mut Vec<String>) -> Option<String> {
    if script.preserved {
        warnings.push(format!(
            "{}: Preserved ABC has no class metadata; automatic object linking is skipped.",
            script.path
        ));
        return None;
    }
    let classes = &script.unit.classes;
    if classes.is_empty() {
        warnings.push(format!(
            "{}: Skript enthält keine Klasse und kann keinem Objekt zugeordnet werden.",
            script.path
        ));
        return None;
    }
    let stem = script_file_stem(&script.path);
    let chosen = classes
        .iter()
        .find(|class| class.name.eq_ignore_ascii_case(&stem))
        .or_else(|| {
            classes
                .iter()
                .find(|class| class.name.eq_ignore_ascii_case("Main"))
        })
        .or_else(|| {
            if classes.len() == 1 {
                classes.first()
            } else {
                warnings.push(format!(
                    "{}: Mehrere Klassen, aber keine passt zum Dateinamen '{}'; verwende '{}'.",
                    script.path, stem, classes[0].name
                ));
                classes.first()
            }
        })?;
    if !class_is_display(chosen, classes) {
        warnings.push(format!(
            "{}: Klasse '{}' erbt nicht von MovieClip/Sprite und kann keinem SWF-Objekt zugeordnet werden.",
            script.path, chosen.name
        ));
        return None;
    }
    Some(chosen.qualified_name.clone())
}

fn choose_root_class(script: &CompiledScript) -> Option<String> {
    let stem = script_file_stem(&script.path);
    let classes = &script.unit.classes;
    let chosen = classes
        .iter()
        .find(|class| class.name.eq_ignore_ascii_case(&stem))
        .or_else(|| {
            classes
                .iter()
                .find(|class| class.name.eq_ignore_ascii_case("Main"))
        })
        .or_else(|| (classes.len() == 1).then(|| &classes[0]))?;
    class_is_display(chosen, classes).then(|| chosen.qualified_name.clone())
}

fn to_color(color: Rgba) -> Color {
    Color::rgba(color.r, color.g, color.b, color.a)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ShapeKey {
    Rect {
        width: i64,
        height: i64,
        radius: i64,
        fill: u32,
    },
    Ellipse {
        width: i64,
        height: i64,
        fill: u32,
    },
}

impl ShapeKey {
    fn to_shape(self) -> Shape {
        match self {
            Self::Rect {
                width,
                height,
                radius,
                fill,
            } => {
                let color = unpack_color(fill);
                if radius > 0 {
                    Shape::rounded_rect(
                        width as f32 / 100.0,
                        height as f32 / 100.0,
                        radius as f32 / 100.0,
                        color,
                    )
                } else {
                    Shape::rect(width as f32 / 100.0, height as f32 / 100.0, color)
                }
            }
            Self::Ellipse {
                width,
                height,
                fill,
            } => Shape::ellipse(
                width as f32 / 100.0,
                height as f32 / 100.0,
                unpack_color(fill),
            ),
        }
    }
}

fn shape_key(object: &nova_core::ObjectInstance) -> Option<ShapeKey> {
    let fill = pack_color(object.fill);
    match &object.kind {
        ObjectKind::Rect {
            width,
            height,
            corner_radius,
        } => Some(ShapeKey::Rect {
            width: quantize(*width),
            height: quantize(*height),
            radius: quantize(*corner_radius),
            fill,
        }),
        ObjectKind::Ellipse { width, height } => Some(ShapeKey::Ellipse {
            width: quantize(*width),
            height: quantize(*height),
            fill,
        }),
        ObjectKind::Path { .. }
        | ObjectKind::Symbol { .. }
        | ObjectKind::Bitmap { .. }
        | ObjectKind::Text(_) => None,
    }
}

fn path_shape(width: f32, height: f32, paths: &[nova_core::PathShape]) -> Shape {
    let mut fills = Vec::new();
    let mut shape_paths = Vec::new();
    for path in paths {
        let fill_index = fills.len() + 1;
        fills.push(to_color(path.fill));
        let commands = path
            .commands
            .iter()
            .map(|command| match *command {
                PathCommand::MoveTo { x, y } => {
                    Command::MoveTo(Point::new(to_twips(x), to_twips(y)))
                }
                PathCommand::LineTo { x, y } => {
                    Command::LineTo(Point::new(to_twips(x), to_twips(y)))
                }
                PathCommand::CurveTo {
                    control_x,
                    control_y,
                    x,
                    y,
                } => Command::CurveTo {
                    control: Point::new(to_twips(control_x), to_twips(control_y)),
                    to: Point::new(to_twips(x), to_twips(y)),
                },
            })
            .collect();
        shape_paths.push(ShapePath {
            fill: Some(fill_index as u16),
            line: None,
            commands,
        });
    }
    if fills.is_empty() {
        fills.push(Color::WHITE);
        shape_paths.push(ShapePath {
            fill: Some(1),
            line: None,
            commands: vec![
                Command::MoveTo(Point::new(to_twips(-width / 2.0), to_twips(-height / 2.0))),
                Command::LineTo(Point::new(to_twips(width / 2.0), to_twips(-height / 2.0))),
                Command::LineTo(Point::new(to_twips(width / 2.0), to_twips(height / 2.0))),
                Command::LineTo(Point::new(to_twips(-width / 2.0), to_twips(height / 2.0))),
                Command::LineTo(Point::new(to_twips(-width / 2.0), to_twips(-height / 2.0))),
            ],
        });
    }
    Shape {
        bounds: SwfRect::centered(width, height),
        fills,
        lines: Vec::new(),
        paths: shape_paths,
    }
}

fn to_twips(value: f32) -> i32 {
    (value * 20.0).round() as i32
}

fn quantize(value: f32) -> i64 {
    (value * 100.0).round() as i64
}

fn pack_color(color: Rgba) -> u32 {
    u32::from_be_bytes([color.r, color.g, color.b, color.a])
}

fn unpack_color(value: u32) -> Color {
    let [r, g, b, a] = value.to_be_bytes();
    Color::rgba(r, g, b, a)
}

fn unique_name(base: &str, depth: u16, used: &mut HashMap<String, u32>) -> String {
    let mut clean: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if clean.is_empty() {
        clean = "Objekt".into();
    }
    if clean.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        clean.insert(0, '_');
    }
    let count = used.entry(clean.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        clean
    } else {
        format!("{clean}_{depth}")
    }
}

/// Shared warning helpers used by the CLI and app.
pub fn describe_warnings(output: &BuildOutput) -> Vec<String> {
    output.warnings.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_builds_to_swf_bytes() {
        let project = Project::demo();
        let output = build_swf(&project).unwrap();
        assert!(output.bytes.len() > 200);
        assert_eq!(&output.bytes[0..3], b"CWS");
        assert_eq!(
            output.stats.frames,
            project.active_scene().unwrap().frame_count()
        );
        assert!(output.stats.placed_objects > 0);
        let info = nova_swf::inspect(&output.bytes).unwrap();
        assert_eq!(info.frame_rate, 24.0);
        assert_eq!(info.frame_count as usize, output.stats.frames);
        assert!(info.has_end);
        assert!(info.warnings.is_empty(), "{:?}", info.warnings);
        assert!(info.has_tag(nova_swf::tags::TAG_DEFINE_SHAPE_3));
        assert!(info.has_tag(nova_swf::tags::TAG_DEFINE_SPRITE));
        assert!(
            info.version >= 10,
            "AVM2 script content requires SWF 10+, got {}",
            info.version
        );
        assert!(info.has_tag(nova_swf::tags::TAG_DO_ABC2));
        assert!(info.has_tag(nova_swf::tags::TAG_SYMBOL_CLASS));
        assert_eq!(output.stats.embedded_scripts, 1);
        assert!(output.stats.abc_bytes > 0);
    }

    #[test]
    fn uncompressed_export_is_possible() {
        let mut project = Project::demo();
        project.export.compression = SwfCompression::None;
        let output = build_swf(&project).unwrap();
        assert_eq!(&output.bytes[0..3], b"FWS");
    }

    #[test]
    fn invalid_project_is_rejected() {
        let mut project = Project::demo();
        project.metadata.name.clear();
        assert!(build_swf(&project).is_err());
    }

    #[test]
    fn names_are_sanitized_and_unique() {
        let mut used = HashMap::new();
        assert_eq!(unique_name("Titelbalken", 1, &mut used), "Titelbalken");
        assert_eq!(unique_name("Titelbalken", 2, &mut used), "Titelbalken_2");
        assert_eq!(unique_name("1 Objekt", 3, &mut used), "_1_Objekt");
    }
}
