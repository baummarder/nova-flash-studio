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
    AssetKind, BodyType, CodeProvenance, CollisionShape, LayerKind, ObjectKind, PathCommand,
    Project, Rgba, Scene, SoundCue, SwfCompression, SymbolId, TargetRuntime, resolve_scene,
};
use nova_swf::shape::{Command, FillStyle, GradientRecord, Point, Shape, ShapePath};
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
    // Scene transitions only work when every scene is present in the SWF
    // (the NovaGame bridge jumps between concatenated scene ranges). When a
    // project configures transitions, all scenes are exported automatically.
    let needs_all_scenes = project
        .scenes
        .iter()
        .any(|s| s.game.as_ref().is_some_and(|g| !g.transitions.is_empty()));
    let export_scenes: Vec<&Scene> = if export.export_all_scenes || needs_all_scenes {
        project.scenes.iter().collect()
    } else {
        vec![scene]
    };
    if needs_all_scenes && !export.export_all_scenes {
        warnings.push(
            "Szeneübergänge konfiguriert: Alle Szenen werden automatisch in die SWF exportiert."
                .into(),
        );
    }
    if project.scenes.len() > 1 && !export.export_all_scenes && !needs_all_scenes {
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

    warn_unsupported_features(project, &export_scenes, &mut warnings);

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

        // M12: generated runtime data for the game.NovaGame AS3 bridge.
        // Emitted as a regular compiled script so it travels inside the same
        // DoABC2 set; it is a plain class and never a document-class candidate.
        if let Some((path, source)) = nova_game_data_source(project, &export_scenes, &mut warnings)
        {
            let compiled = compile_with_options(&source, CompileOptions::default());
            for diagnostic in &compiled.diagnostics {
                let rendered = diagnostic.render(&source, &path);
                if diagnostic.severity == nova_as::Severity::Error {
                    return Err(BuildError::ActionScript(rendered));
                }
                warnings.push(rendered);
            }
            if !compiled.abc.is_empty() {
                compiled_scripts.push(CompiledScript {
                    id: uuid::Uuid::nil(),
                    path,
                    index: compiled_scripts.len(),
                    linked_object: None,
                    unit: compiled,
                    preserved: false,
                });
            }
        }
    }

    let has_scripts = !compiled_scripts.is_empty();
    let requested_version = export.swf_version;
    // M14: playerglobal APIs used by modern AS3 (JSON, ByteArray helpers,
    // flash.utils.*, flash.text.*, BitmapData, …) are gated at SWF v13 in
    // Ruffle's playerglobals. Script projects therefore export at v13+;
    // v10 remains the floor for trivial scripts but is raised automatically.
    let min_script_version = if has_scripts { 13 } else { requested_version };
    let swf_version = requested_version.max(min_script_version);
    if has_scripts && swf_version != requested_version {
        warnings.push(format!(
            "SWF-Version von {requested_version} auf {swf_version} angehoben (DoABC2/playerglobal-APIs benötigen mindestens Version 13)."
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
    let avm2_links = if has_scripts {
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
            if let Some(existing) = object_script_ids.get(&object_id)
                && *existing != script.id
            {
                warnings.push(format!(
                        "{}: Das verknüpfte Objekt ist bereits einem anderen Skript zugewiesen; die direkte Objektzuweisung hat Vorrang.",
                        script.path
                    ));
                continue;
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

        // ABC is only embedded when at least one class link exists; unlinked
        // bytecode can never be instantiated and would only mark the file as
        // AVM2, which breaks bitmap playback in some runtimes.
        if !symbol_links.is_empty() {
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
        }

        let avm2_links_present = !symbol_links.is_empty();
        if avm2_links_present {
            symbol_links.sort_by_key(|(character, _)| *character);
            movie.tags.push(Tag::SymbolClass {
                links: symbol_links,
            });
        }
        avm2_links_present
    } else {
        false
    };

    // An AVM2-flagged file without any linked class cannot execute its ABC.
    // Some runtimes (verified: Ruffle 0.7.1) then refuse to render bitmap
    // characters on the timeline, so scriptless/unlinked projects are emitted
    // with the AS3 flag cleared. Timeline content is identical either way.
    let avm2_inert = export.target == TargetRuntime::Avm2 && !avm2_links;
    if avm2_inert {
        warnings.push(
            "Keine Skript-Verknüpfung gefunden; die SWF wird ohne AS3-Flag exportiert (Bitmaps bleiben in AVM1-kompatiblen Playern sichtbar)."
                .into(),
        );
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

    if avm2_inert
        && let Some(Tag::FileAttributes {
            action_script_3, ..
        }) = movie.tags.first_mut()
    {
        *action_script_3 = false;
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

/// Warns once per object about authoring features the SWV format cannot
/// reproduce. The `.nova` data is preserved; nothing is silently dropped.
fn warn_unsupported_features(project: &Project, scenes: &[&Scene], warnings: &mut Vec<String>) {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut scan = |scene: &Scene, warnings: &mut Vec<String>| {
        for layer in &scene.layers {
            for frame in &layer.frames {
                for object in &frame.objects {
                    if object.particles.is_some() && seen.insert(object.id) {
                        warnings.push(format!(
                            "Partikelemitter '{}' läuft in der SWF nur, wenn das Spiel die game.NovaGame-Laufzeit verwendet (NovaGame.init); sonst bleibt die Einstellung ohne Darstellung.",
                            object.name
                        ));
                    }
                }
            }
        }
    };
    for scene in scenes {
        scan(scene, warnings);
    }
    // Symbol timelines can host objects too; scan them for completeness.
    for symbol in &project.symbols {
        scan(&symbol.timeline, warnings);
    }
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
            ObjectKind::Tilemap(map) => self.define_tilemap(project, map, movie, warnings, stats),
        }
    }

    /// Bakes a tilemap into a single lossless bitmap: every visible layer is
    /// alpha-composited cell-by-cell from the shared tileset image. Layer
    /// opacity is honoured; per-tile solidity is editor metadata and does not
    /// affect pixels.
    fn define_tilemap(
        &mut self,
        project: &Project,
        map: &nova_core::TilemapData,
        movie: &mut Movie,
        warnings: &mut Vec<String>,
        stats: &mut BuildStats,
    ) -> Result<Option<u16>, BuildError> {
        let Some((out_w, out_h, out)) = bake_tilemap(project, map, warnings)? else {
            return Ok(None);
        };
        let width = u16::try_from(out_w)
            .map_err(|_| BuildError::Limit("Kachelkarte breiter als 65535 Pixel".into()))?;
        let height = u16::try_from(out_h)
            .map_err(|_| BuildError::Limit("Kachelkarte höher als 65535 Pixel".into()))?;
        let id = self.alloc()?;
        movie.tags.push(Tag::DefineBitsLossless2 {
            id,
            width,
            height,
            rgba: out,
        });
        stats.defined_bitmaps += 1;
        Ok(Some(id))
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
        // M15: a rigged symbol's sprite length covers the active bone clip
        // even when the authored timeline is shorter — held frames resolve
        // the last keyframe and the skeleton supplies the motion.
        let clip_frames = symbol
            .active_clip
            .as_deref()
            .and_then(|name| symbol.bone_clips.iter().find(|c| c.name == name))
            .map(|clip| clip.frames as usize)
            .unwrap_or(0);
        let frame_count = symbol.frame_count().max(1).max(clip_frames);
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
                if let Some(cue) = layer.frames.get(frame_index).and_then(|frame| frame.sound)
                    && let Some(sound_id) =
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
            let objects = nova_core::resolve_symbol_frame(symbol, frame_index);
            let mut next_depth: u16 = 1;
            let mut layer_depths: HashMap<usize, u16> = HashMap::new();
            let mut used_names: HashMap<String, u32> = HashMap::new();
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
                // M15: named instances inside symbols (stable across frames)
                // so scripts can reach rigged parts via getChildByName.
                let name = unique_name(&object.instance.name, depth, &mut used_names);
                let mut place = PlaceObject::new(depth)
                    .character(character)
                    .matrix(matrix)
                    .named(name);
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

/// Bakes a tilemap into an RGBA pixel buffer `(width, height, rgba)`.
///
/// Every visible layer is alpha-composited cell-by-cell from the shared
/// tileset image; layer opacity is honoured and `tile_rect` accounts for
/// margins/spacing. Returns `Ok(None)` with a human-readable warning pushed
/// to `warnings` when the map cannot be baked (missing tileset/asset,
/// invalid metrics, oversized output). Shared by the SWF exporter and the
/// XFL bitmap export.
pub fn bake_tilemap(
    project: &Project,
    map: &nova_core::TilemapData,
    warnings: &mut Vec<String>,
) -> Result<Option<(u32, u32, Vec<u8>)>, BuildError> {
    if map.columns == 0 || map.rows == 0 {
        return Ok(None);
    }
    let Some(tileset) = project.tilesets.iter().find(|t| t.id == map.tileset) else {
        warnings.push(
            "Kachelkarte verweist auf einen unbekannten Tileset und wurde übersprungen.".into(),
        );
        return Ok(None);
    };
    let Some(asset) = project.asset(tileset.asset) else {
        warnings.push(format!(
            "Tileset '{}' verweist auf ein unbekanntes Asset und wurde übersprungen.",
            tileset.name
        ));
        return Ok(None);
    };
    if asset.data.is_empty() {
        warnings.push(format!(
            "Tileset-Asset '{}' enthält keine eingebetteten Daten.",
            asset.name
        ));
        return Ok(None);
    }
    let source = image::load_from_memory(&asset.data)
        .map_err(|error| BuildError::Asset(format!("Tileset '{}': {error}", asset.name)))?
        .to_rgba8();
    let (src_w, src_h) = source.dimensions();
    if tileset.tile_width == 0 || tileset.tile_height == 0 || tileset.columns == 0 {
        warnings.push(format!(
            "Tileset '{}' hat ungültige Kachelmaße und wurde übersprungen.",
            tileset.name
        ));
        return Ok(None);
    }
    let out_w = map.columns as u32 * tileset.tile_width;
    let out_h = map.rows as u32 * tileset.tile_height;
    if out_w == 0 || out_h == 0 || out_w > 8192 || out_h > 8192 {
        warnings.push(format!(
            "Kachelkarte ist zu groß für den Export ({out_w}×{out_h}px)."
        ));
        return Ok(None);
    }
    let src_px = source.as_raw();
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    for layer in &map.layers {
        if !layer.visible {
            continue;
        }
        let opacity = layer.opacity.clamp(0.0, 1.0);
        for cy in 0..map.rows {
            for cx in 0..map.columns {
                let cell = layer.cells.get(cy * map.columns + cx).copied().unwrap_or(0);
                if cell == 0 {
                    continue;
                }
                let index = cell as usize - 1;
                // tile_rect honours margins/spacing and rejects tiles
                // that fall outside the source image.
                let Some((sx, sy, tw, th)) = tileset.tile_rect(index, src_w, src_h) else {
                    continue;
                };
                let copy_w = tw.min(src_w - sx);
                let copy_h = th.min(src_h - sy);
                let dx = cx as u32 * tileset.tile_width;
                let dy = cy as u32 * tileset.tile_height;
                for py in 0..copy_h {
                    for px in 0..copy_w {
                        let s = ((sy + py) * src_w + sx + px) as usize * 4;
                        let sa = src_px[s + 3] as f32 / 255.0 * opacity;
                        if sa <= 0.0 {
                            continue;
                        }
                        let d = ((dy + py) * out_w + dx + px) as usize * 4;
                        let inv = 1.0 - sa;
                        for c in 0..3 {
                            out[d + c] = (src_px[s + c] as f32 * sa + out[d + c] as f32 * inv)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                        }
                        out[d + 3] = ((sa * 255.0).max(out[d + 3] as f32 * inv))
                            .round()
                            .clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    Ok(Some((out_w, out_h, out)))
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
        | ObjectKind::Tilemap(_)
        | ObjectKind::Text(_) => None,
    }
}

fn path_shape(width: f32, height: f32, paths: &[nova_core::PathShape]) -> Shape {
    let mut fills = Vec::new();
    let mut shape_paths = Vec::new();
    for path in paths {
        let fill_index = fills.len() + 1;
        fills.push(match &path.gradient {
            Some(gradient) => gradient_fill_style(gradient, width, height),
            None => FillStyle::Solid(to_color(path.fill)),
        });
        let mut commands = Vec::with_capacity(path.commands.len());
        // Track the current pen position for cubic subdivision.
        let mut pen = (0.0f32, 0.0f32);
        for command in &path.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    pen = (x, y);
                    commands.push(Command::MoveTo(Point::new(to_twips(x), to_twips(y))));
                }
                PathCommand::LineTo { x, y } => {
                    pen = (x, y);
                    commands.push(Command::LineTo(Point::new(to_twips(x), to_twips(y))));
                }
                PathCommand::CurveTo {
                    control_x,
                    control_y,
                    x,
                    y,
                } => {
                    pen = (x, y);
                    commands.push(Command::CurveTo {
                        control: Point::new(to_twips(control_x), to_twips(control_y)),
                        to: Point::new(to_twips(x), to_twips(y)),
                    });
                }
                PathCommand::CubicTo {
                    c1_x,
                    c1_y,
                    c2_x,
                    c2_y,
                    x,
                    y,
                } => {
                    // SWF shape records only store quadratic curves; cubic
                    // segments are approximated by adaptive quadratic
                    // chains (de Casteljau subdivision).
                    for (cx, cy, tx, ty) in
                        cubic_to_quadratics(pen, (c1_x, c1_y), (c2_x, c2_y), (x, y))
                    {
                        commands.push(Command::CurveTo {
                            control: Point::new(to_twips(cx), to_twips(cy)),
                            to: Point::new(to_twips(tx), to_twips(ty)),
                        });
                    }
                    pen = (x, y);
                }
            }
        }
        shape_paths.push(ShapePath {
            fill: Some(fill_index as u16),
            line: None,
            commands,
        });
    }
    if fills.is_empty() {
        fills.push(FillStyle::Solid(Color::WHITE));
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

/// Maps a model [`nova_core::Gradient`] to a SWF gradient fill style.
/// The gradient space is the object's local, centre-based bounds:
/// linear ramps span the projection of the bounds along `angle`, radial
/// gradients are centred on (`center_x`,`center_y`) with `radius`.
fn gradient_fill_style(gradient: &nova_core::Gradient, width: f32, height: f32) -> FillStyle {
    let records: Vec<GradientRecord> = gradient
        .stops
        .iter()
        .map(|stop| GradientRecord {
            ratio: (stop.position.clamp(0.0, 1.0) * 255.0).round() as u8,
            color: to_color(stop.color),
        })
        .collect();
    let matrix = match gradient.kind {
        nova_core::GradientKind::Linear => {
            let theta = gradient.angle.to_radians();
            let (ux, uy) = (theta.cos(), theta.sin());
            // Half-extents of the bounds projected onto ramp/perp axes.
            let ex = (width.abs() * ux.abs() + height.abs() * uy.abs()) / 2.0;
            let ey = (width.abs() * uy.abs() + height.abs() * ux.abs()) / 2.0;
            // u = ramp dir, v = perpendicular; gradient x in [0,1] maps
            // to -ex..+ex along u, y in [0,1] to -ey..+ey along v.
            Matrix {
                a: ux * 2.0 * ex,
                b: uy * 2.0 * ex,
                c: -uy * 2.0 * ey,
                d: ux * 2.0 * ey,
                tx: 0.0,
                ty: 0.0,
            }
        }
        nova_core::GradientKind::Radial => Matrix {
            a: gradient.radius.max(1.0),
            b: 0.0,
            c: 0.0,
            d: gradient.radius.max(1.0),
            tx: gradient.center_x,
            ty: gradient.center_y,
        },
    };
    match gradient.kind {
        nova_core::GradientKind::Linear => FillStyle::LinearGradient { matrix, records },
        nova_core::GradientKind::Radial => FillStyle::RadialGradient { matrix, records },
    }
}

/// Approximates a cubic Bézier `p0..p3` by quadratic segments
/// `(control, to)` pairs. Uses recursive de Casteljau subdivision with a
/// distance-based flatness test; deterministic (fixed depth cap).
fn cubic_to_quadratics(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p3: (f32, f32),
) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::new();
    subdivide_cubic(p0, c1, c2, p3, 0, &mut out);
    out
}

fn subdivide_cubic(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p3: (f32, f32),
    depth: u32,
    out: &mut Vec<(f32, f32, f32, f32)>,
) {
    // Flatness: max distance of control points from the chord.
    let chord = (p3.0 - p0.0, p3.1 - p0.1);
    let chord_len = (chord.0 * chord.0 + chord.1 * chord.1).sqrt();
    let dist = |p: (f32, f32)| -> f32 {
        if chord_len < 1e-6 {
            return ((p.0 - p0.0).powi(2) + (p.1 - p0.1).powi(2)).sqrt();
        }
        ((p.0 - p0.0) * chord.1 - (p.1 - p0.1) * chord.0).abs() / chord_len
    };
    let flat = dist(c1).max(dist(c2)) < 0.6 && depth >= 2 || depth >= 5;
    if flat {
        // Quadratic control = intersection of endpoint tangents.
        let t1 = (p0.0 + (c1.0 - p0.0), p0.1 + (c1.1 - p0.1));
        let t2 = (p3.0 + (c2.0 - p3.0), p3.1 + (c2.1 - p3.1));
        let d1 = (t1.0 - p0.0, t1.1 - p0.1);
        let d2 = (t2.0 - p3.0, t2.1 - p3.1);
        let denom = d1.0 * d2.1 - d1.1 * d2.0;
        let ctrl = if denom.abs() < 1e-6 {
            ((c1.0 + c2.0) / 2.0, (c1.1 + c2.1) / 2.0)
        } else {
            // Intersect line(p0, d1) with line(p3, d2).
            let t = ((p3.0 - p0.0) * d2.1 - (p3.1 - p0.1) * d2.0) / denom;
            (p0.0 + d1.0 * t, p0.1 + d1.1 * t)
        };
        out.push((ctrl.0, ctrl.1, p3.0, p3.1));
        return;
    }
    // de Casteljau split at t=0.5.
    let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let (a, b, c) = (mid(p0, c1), mid(c1, c2), mid(c2, p3));
    let (d, e) = (mid(a, b), mid(b, c));
    let m = mid(d, e);
    subdivide_cubic(p0, a, d, m, depth + 1, out);
    subdivide_cubic(m, e, c, p3, depth + 1, out);
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

// ---------------------------------------------------------------------------
// M12: generated game.NovaGameData for the AS3 runtime bridge
// ---------------------------------------------------------------------------

/// Returns `Some((path, source))` when the export contains game runtime
/// metadata worth bridging into the SWF: multiple scenes, scene-level game
/// config, or per-object collision/physics. The generated class is plain AS3
/// (no display inheritance) so it can never become a document-class candidate.
fn nova_game_data_source(
    project: &Project,
    export_scenes: &[&Scene],
    warnings: &mut Vec<String>,
) -> Option<(String, String)> {
    let has_scene_game = export_scenes.iter().any(|s| s.game.is_some());
    let has_object_game = export_scenes.iter().any(|scene| {
        (0..scene.frame_count()).any(|frame| {
            resolve_scene(scene, frame)
                .iter()
                .any(|o| o.instance.game.is_some() || o.instance.particles.is_some())
        })
    });
    let has_tile_colliders = export_scenes.iter().any(|scene| {
        (0..scene.frame_count()).any(|frame| {
            resolve_scene(scene, frame)
                .iter()
                .any(|o| tilemap_grid(project, &o.instance).is_some())
        })
    });
    if export_scenes.len() <= 1 && !has_scene_game && !has_object_game && !has_tile_colliders {
        return None;
    }

    let mut out = String::with_capacity(4096);
    out.push_str("package game {\n");
    out.push_str("    /// Generated by NØVA Flash Studio for the game.NovaGame bridge.\n");
    out.push_str("    /// Do not edit; regenerated on every SWF export.\n");
    out.push_str("    public class NovaGameData {\n");
    out.push_str("        public static var SCENE_COUNT:int = 0;\n");
    out.push_str("        public static var SCENE_NAMES:Object = {};\n");
    out.push_str("        public static var SCENE_START:Object = {};\n");
    out.push_str("        public static var SCENE_END:Object = {};\n");
    out.push_str("        public static var COLLIDERS:Object = {};\n");
    out.push_str("        public static var CAMERAS:Object = {};\n");
    out.push_str("        public static var WORLDS:Object = {};\n");
    out.push_str("        public static var INPUTS:Object = {};\n");
    out.push_str("        public static var TRANSITIONS:Object = {};\n");
    out.push_str("        public static var EMITTERS:Object = {};\n");
    out.push_str("        public static var TRANS_COUNT:int = 0;\n");
    let fps = export_scenes.first().map(|s| s.fps).unwrap_or(30.0);
    out.push_str(&format!(
        "        public static var FPS:Number = {};\n",
        num(fps)
    ));
    out.push_str("        private static var ready:Boolean = false;\n");
    out.push_str("        public static function init():void {\n");
    out.push_str("            if (ready) { return; }\n");
    out.push_str("            ready = true;\n");

    // Scene frame ranges (1-based currentFrame space).
    let mut frame = 1usize;
    for (idx, scene) in export_scenes.iter().enumerate() {
        let count = scene.frame_count();
        let end = frame + count.saturating_sub(1);
        let name = as3_string(&scene.name);
        out.push_str(&format!(
            "            SCENE_NAMES[{idx}] = \"{name}\"; SCENE_START[{idx}] = {frame}; SCENE_END[{idx}] = {end};\n"
        ));
        frame = end + 1;
    }
    out.push_str(&format!(
        "            SCENE_COUNT = {};\n",
        export_scenes.len()
    ));

    // Per-scene collider table: name -> spec string.
    // spec: name|shape|w|h|layer|mask|btype|gx|fr|bo|ctrl|points
    // shape: 0 rect, 1 circle, 2 polygon, 3 physics-only (no collision)
    // btype: 0 static, 1 dynamic, 2 kinematic
    let mut table_n = 0usize;
    for scene in export_scenes {
        let mut seen: HashMap<Uuid, usize> = HashMap::new();
        let mut entries: Vec<(String, String)> = Vec::new();
        for frame_idx in 0..scene.frame_count() {
            for object in resolve_scene(scene, frame_idx) {
                if seen.contains_key(&object.instance.id) {
                    continue;
                }
                let spec = match &object.instance.game {
                    Some(game) => collider_spec(&object.instance.name, game, warnings),
                    None => tilemap_grid_spec(project, &object.instance, warnings),
                };
                if let Some(spec) = spec {
                    let slot = entries.len();
                    seen.insert(object.instance.id, slot);
                    entries.push(spec);
                }
            }
        }
        if entries.is_empty() {
            continue;
        }
        let scene_key = as3_string(&scene.name);
        let table_var = format!("c{table_n}");
        table_n += 1;
        out.push_str(&format!(
            "            var {table_var}:Object = {{}}; {table_var}.count = {};\n",
            entries.len()
        ));
        for (i, (name, spec)) in entries.iter().enumerate() {
            out.push_str(&format!(
                "            {table_var}[{i}] = \"{}|{}\";\n",
                as3_string(name),
                spec
            ));
        }
        out.push_str(&format!(
            "            COLLIDERS[\"{scene_key}\"] = {table_var};\n"
        ));
    }

    // Per-scene camera / world / input / transitions.
    let mut trans_idx = 0usize;
    for scene in export_scenes {
        let scene_key = as3_string(&scene.name);
        if let Some(game) = &scene.game {
            if let Some(cam) = &game.camera {
                let follow = sanitize_as3_name(&cam.follow);
                out.push_str(&format!(
                    "            CAMERAS[\"{scene_key}\"] = \"{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}\";\n",
                    cam.enabled as u8,
                    num(cam.left),
                    num(cam.top),
                    num(cam.right),
                    num(cam.bottom),
                    num(cam.zoom),
                    num(cam.follow_smoothing),
                    num(cam.dead_zone.0),
                    num(cam.dead_zone.1),
                    num(cam.shake),
                    follow
                ));
            }
            if let Some(world) = &game.world
                && (world.width > 0.0 || world.height > 0.0)
            {
                out.push_str(&format!(
                    "            WORLDS[\"{scene_key}\"] = \"{}|{}|{}|{}\";\n",
                    num(world.width),
                    num(world.height),
                    num(world.spawn.0),
                    num(world.spawn.1)
                ));
            }
            if let Some(input) = &game.input {
                out.push_str(&format!(
                    "            INPUTS[\"{scene_key}\"] = \"{}|{}|{}|{}|{}|{}\";\n",
                    key_code(&input.left, warnings),
                    key_code(&input.right, warnings),
                    key_code(&input.up, warnings),
                    key_code(&input.down, warnings),
                    key_code(&input.jump, warnings),
                    key_code(&input.action, warnings)
                ));
            }
            for transition in &game.transitions {
                let Some(code) = key_code_opt(&transition.key) else {
                    warnings.push(format!(
                        "Szene '{}': Übergangstaste '{}' wird in der SWF nicht unterstützt und ignoriert.",
                        scene.name, transition.key
                    ));
                    continue;
                };
                let target = as3_string(&transition.target);
                out.push_str(&format!(
                    "            TRANSITIONS[{trans_idx}] = \"{code}|{target}\";\n"
                ));
                trans_idx += 1;
            }
        }
    }
    out.push_str(&format!("            TRANS_COUNT = {trans_idx};\n"));

    // Per-scene particle emitters (NovaGame bridge only).
    // spec: name|rate|burst|lt0|lt1|ss0|ss1|se0|se1|os0|os1|oe0|oe1|
    //       csr|csg|csb|cer|ceg|ceb|sp0|sp1|dir|spread|grav|drag|rs0|rs1|max|space
    for scene in export_scenes {
        let mut seen_emit: HashMap<Uuid, ()> = HashMap::new();
        let mut emit_entries: Vec<String> = Vec::new();
        for frame_idx in 0..scene.frame_count() {
            for object in resolve_scene(scene, frame_idx) {
                let Some(emitter) = &object.instance.particles else {
                    continue;
                };
                if !seen_emit.insert(object.instance.id, ()).is_none() {
                    continue;
                }
                emit_entries.push(emitter_spec(
                    &sanitize_as3_name(&object.instance.name),
                    emitter,
                ));
            }
        }
        if emit_entries.is_empty() {
            continue;
        }
        let scene_key = as3_string(&scene.name);
        let table_var = format!("e{}", table_n);
        table_n += 1;
        out.push_str(&format!(
            "            var {table_var}:Object = {{}}; {table_var}.count = {};\n",
            emit_entries.len()
        ));
        for (i, spec) in emit_entries.iter().enumerate() {
            out.push_str(&format!("            {table_var}[{i}] = \"{spec}\";\n"));
        }
        out.push_str(&format!(
            "            EMITTERS[\"{scene_key}\"] = {table_var};\n"
        ));
    }

    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("}\n");

    Some(("scripts/game/NovaGameData.as".to_string(), out))
}

/// Returns the solid-cell bitmap for a tilemap object instance, or `None`
/// when the object is not a tilemap, has no solid tiles, or exceeds the
/// bridge's cell cap. Mirrors `nova_core::game::attach_tile_colliders`:
/// a cell is solid when any layer holds a tile whose tileset flag is solid.
fn tilemap_grid(
    project: &Project,
    instance: &nova_core::model::ObjectInstance,
) -> Option<(usize, usize, f32, f32, String)> {
    let nova_core::model::ObjectKind::Tilemap(map) = &instance.kind else {
        return None;
    };
    if map.columns == 0 || map.rows == 0 || map.columns * map.rows > 8192 {
        return None;
    }
    let tileset = project.tilesets.iter().find(|t| t.id == map.tileset)?;
    if tileset.solid.iter().all(|s| !s) {
        return None;
    }
    let mut solid = vec![false; map.columns * map.rows];
    for layer in &map.layers {
        for (index, cell) in layer.cells.iter().enumerate() {
            if *cell > 0 && tileset.is_solid(*cell as usize - 1) {
                solid[index] = true;
            }
        }
    }
    if !solid.iter().any(|s| *s) {
        return None;
    }
    let bits: String = solid.iter().map(|s| if *s { '1' } else { '0' }).collect();
    Some((map.columns, map.rows, map.tile_width, map.tile_height, bits))
}

/// Builds a shape-4 "tile grid" collider spec for the AS3 bridge. The bridge
/// expands contiguous solid runs into static rect bodies sharing the map's
/// object name (one contact pair per map object, like the preview).
/// spec tail: `cols|rows|layer|mask|0|0|0|0|0|tw;th;bits`
fn tilemap_grid_spec(
    project: &Project,
    instance: &nova_core::model::ObjectInstance,
    warnings: &mut Vec<String>,
) -> Option<(String, String)> {
    let (cols, rows, tw, th, bits) = tilemap_grid(project, instance)?;
    let (layer, mask) = instance
        .game
        .as_ref()
        .and_then(|g| g.collision.as_ref())
        .map(|c| (c.layer, c.mask))
        .unwrap_or((1, u32::MAX));
    let name = sanitize_as3_name(&instance.name);
    if name.is_empty() {
        warnings.push(
            "Ein Tilemap-Objekt ohne Namen kann keinen Tile-Kollider exportieren.".to_string(),
        );
        return None;
    }
    Some((
        name,
        format!(
            "4|{}|{}|{}|{}|0|0|0|0|0|{};{};{}",
            cols,
            rows,
            layer,
            mask,
            num(tw),
            num(th),
            bits
        ),
    ))
}

/// Builds the collider spec payload for one object, or `None` when the object
/// has no collision/physics data. Tilemap objects are handled separately by
/// [`tilemap_grid_spec`].
fn collider_spec(
    name: &str,
    game: &nova_core::GameObjectConfig,
    warnings: &mut Vec<String>,
) -> Option<(String, String)> {
    let (collision, physics) = match (&game.collision, &game.physics) {
        (None, None) => return None,
        (c, p) => (c.clone(), *p),
    };
    // No physics spec => the body is not integrated by the preview either;
    // it acts as static geometry (body type 0).
    let btype = match physics.as_ref().map(|p| &p.body) {
        Some(BodyType::Dynamic) => 1,
        Some(BodyType::Kinematic) => 2,
        _ => 0,
    };
    let (shape_code, w, h, points) = match collision.as_ref().map(|c| &c.shape) {
        Some(CollisionShape::Circle) => {
            let c = collision.as_ref().unwrap();
            (1u8, c.width, c.height, String::new())
        }
        Some(CollisionShape::Polygon { points }) => {
            let c = collision.as_ref().unwrap();
            // M13: the AS3 bridge runs SAT which requires convex polygons.
            // The preview approximates concave input by its convex hull —
            // apply the same hull here so SWF behaviour matches preview
            // instead of producing undefined SAT results.
            let hull = if nova_core::collision::is_convex(points) {
                points.clone()
            } else {
                nova_core::collision::convex_hull(points)
            };
            if hull.len() != points.len() {
                warnings.push(format!(
                    "Kollider '{}' ist ein Konkavpolygon; es wird wie in der Vorschau durch die konvexe Hülle approximiert.",
                    name
                ));
            }
            let pts = hull
                .iter()
                .map(|(x, y)| format!("{},{}", num(*x), num(*y)))
                .collect::<Vec<_>>()
                .join(";");
            (2u8, c.width, c.height, pts)
        }
        _ => {
            // Rectangle, or physics-only (shape 3 = moves but never collides,
            // approximating the preview's visual-bound fallback).
            if let Some(c) = &collision {
                (0u8, c.width, c.height, String::new())
            } else {
                (3u8, 0.0, 0.0, String::new())
            }
        }
    };
    let (layer, mask) = collision
        .as_ref()
        .map(|c| (c.layer, c.mask))
        .unwrap_or((1, u32::MAX));
    let (gx, fr, bo, ctrl) = physics
        .as_ref()
        .map(|p| {
            (
                p.gravity_scale,
                p.friction,
                p.bounce,
                if p.controlled { 1 } else { 0 },
            )
        })
        .unwrap_or((1.0, 0.0, 0.0, 0));
    let spec = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        shape_code,
        num(w),
        num(h),
        layer,
        mask,
        btype,
        num(gx),
        num(fr),
        num(bo),
        ctrl
    );
    if shape_code == 2 && points.split(';').count() < 3 {
        warnings.push(format!(
            "Kollider '{}' hat ein Polygon mit weniger als 3 Punkten; es wird als Rechteck exportiert.",
            name
        ));
    }
    Some((sanitize_as3_name(name), format!("{spec}|{points}")))
}

/// Serialises a [`nova_core::ParticleEmitter`] into the pipe-delimited spec
/// consumed by `game.NovaGame` (29 fields, indices documented in
/// `nova_game_data_source`).
fn emitter_spec(name: &str, e: &nova_core::ParticleEmitter) -> String {
    let space = match e.space {
        nova_core::ParticleSpace::Local => 0,
        nova_core::ParticleSpace::World => 1,
    };
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        name,
        num(e.rate),
        e.burst,
        num(e.lifetime.0),
        num(e.lifetime.1),
        num(e.size_start.0),
        num(e.size_start.1),
        num(e.size_end.0),
        num(e.size_end.1),
        num(e.opacity_start.0),
        num(e.opacity_start.1),
        num(e.opacity_end.0),
        num(e.opacity_end.1),
        e.color_start.r,
        e.color_start.g,
        e.color_start.b,
        e.color_end.r,
        e.color_end.g,
        e.color_end.b,
        num(e.speed.0),
        num(e.speed.1),
        num(e.direction),
        num(e.spread),
        num(e.gravity),
        num(e.drag),
        num(e.rotation_speed.0),
        num(e.rotation_speed.1),
        e.max_particles,
        space
    )
}

/// egui-style key names -> AVM2 KeyboardEvent.keyCode values.
fn key_code_opt(name: &str) -> Option<u32> {
    let code = match name {
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Space" => 32,
        "Enter" => 13,
        "Escape" => 27,
        "Tab" => 9,
        "Backspace" => 8,
        "ShiftLeft" | "Shift" => 16,
        "ControlLeft" | "Control" => 17,
        "AltLeft" | "Alt" => 18,
        "CapsLock" => 20,
        "Comma" => 188,
        "Period" => 190,
        "Slash" => 191,
        "Semicolon" => 186,
        "Quote" => 222,
        "BracketLeft" => 219,
        "BracketRight" => 221,
        "Backquote" => 192,
        "Minus" => 189,
        "Equal" => 187,
        "Backslash" => 220,
        "Numpad0" => 96,
        "Numpad1" => 97,
        "Numpad2" => 98,
        "Numpad3" => 99,
        "Numpad4" => 100,
        "Numpad5" => 101,
        "Numpad6" => 102,
        "Numpad7" => 103,
        "Numpad8" => 104,
        "Numpad9" => 105,
        "F1" => 112,
        "F2" => 113,
        "F3" => 114,
        "F4" => 115,
        "F5" => 116,
        "F6" => 117,
        "F7" => 118,
        "F8" => 119,
        "F9" => 120,
        "F10" => 121,
        "F11" => 122,
        "F12" => 123,
        _ => {
            if let Some(rest) = name.strip_prefix("Key")
                && rest.len() == 1
            {
                return Some(rest.chars().next().unwrap().to_ascii_uppercase() as u32);
            }
            if let Some(rest) = name.strip_prefix("Digit")
                && rest.len() == 1
                && rest.chars().next().unwrap().is_ascii_digit()
            {
                return Some(rest.chars().next().unwrap() as u32);
            }
            return None;
        }
    };
    Some(code)
}

fn key_code(name: &str, warnings: &mut Vec<String>) -> u32 {
    key_code_opt(name).unwrap_or_else(|| {
        warnings.push(format!(
            "Eingabe-Name '{name}' wird in der SWF nicht unterstützt (KeyCode 0 = keine Taste)."
        ));
        0
    })
}

/// Mirrors `unique_name` in this file: ASCII alnum + '_', '_' prefix for a
/// leading digit, "Objekt" when empty.
fn sanitize_as3_name(name: &str) -> String {
    let mut clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if clean.is_empty() {
        clean = "Objekt".into();
    }
    if clean.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        clean.insert(0, '_');
    }
    clean
}

/// Escapes a string for embedding in generated AS3 double-quoted literals.
fn as3_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\n', '\r'], " ")
}

/// Compact float formatting for generated data (no exponent noise).
fn num(v: f32) -> String {
    let mut s = format!("{v:.4}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".into();
    }
    s
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
    fn gradient_path_exports_native_gradient_fill() {
        let mut project = Project::demo();
        project.export.compression = nova_core::SwfCompression::None;
        // Give the first path object a linear gradient.
        let scene = project.active_scene_mut().unwrap();
        // Ensure a Path object exists (demo may only have primitives).
        if !scene.layers[0].frames[0]
            .objects
            .iter()
            .any(|o| matches!(o.kind, nova_core::ObjectKind::Path { .. }))
        {
            let (hw, hh) = (30.0f32, 30.0f32);
            scene.layers[0].frames[0]
                .objects
                .push(nova_core::ObjectInstance::path(
                    "GradientShape",
                    200.0,
                    200.0,
                    60.0,
                    60.0,
                    vec![nova_core::PathShape::new(
                        nova_core::Rgba::WHITE,
                        vec![
                            nova_core::PathCommand::MoveTo { x: -hw, y: -hh },
                            nova_core::PathCommand::LineTo { x: hw, y: -hh },
                            nova_core::PathCommand::LineTo { x: hw, y: hh },
                            nova_core::PathCommand::LineTo { x: -hw, y: hh },
                            nova_core::PathCommand::LineTo { x: -hw, y: -hh },
                        ],
                    )],
                    nova_core::Rgba::WHITE,
                ));
        }
        let object = scene.layers[0].frames[0]
            .objects
            .iter_mut()
            .find(|o| matches!(o.kind, nova_core::ObjectKind::Path { .. }))
            .expect("path object present");
        let nova_core::ObjectKind::Path { paths, .. } = &mut object.kind else {
            unreachable!()
        };
        for path in paths.iter_mut() {
            path.gradient = Some(nova_core::Gradient {
                kind: nova_core::GradientKind::Linear,
                stops: vec![
                    nova_core::GradientStop {
                        position: 0.0,
                        color: nova_core::Rgba::rgb(255, 0, 0),
                    },
                    nova_core::GradientStop {
                        position: 1.0,
                        color: nova_core::Rgba::rgb(0, 0, 255),
                    },
                ],
                angle: 45.0,
                center_x: 0.0,
                center_y: 0.0,
                radius: 50.0,
            });
        }
        let output = build_swf(&project).unwrap();
        let info = nova_swf::inspect(&output.bytes).unwrap();
        assert!(info.warnings.is_empty(), "{:?}", info.warnings);
        // Parse with the reference SWF reader: a real gradient fill must
        // be present in a DefineShape tag.
        let buf = swf::decompress_swf(&mut std::io::Cursor::new(&output.bytes)).unwrap();
        let parsed = swf::parse_swf(&buf).unwrap();
        let has_gradient = parsed.tags.iter().any(|tag| {
            matches!(tag, swf::Tag::DefineShape(shape)
            if shape.styles.fill_styles.iter().any(|f| matches!(
                f,
                swf::FillStyle::LinearGradient(_) | swf::FillStyle::RadialGradient(_)
            )))
        });
        assert!(has_gradient, "SWF must contain a native gradient fill");
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
