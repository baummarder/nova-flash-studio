//! SWF import and reconstruction.
//!
//! The importer deliberately does not try to be a byte-for-byte decompiler.
//! It reconstructs the parts the editor can represent (stage settings, frames,
//! layers by depth, shapes, frame labels and preserved ABC bytes) and reports
//! everything else. The original SWF bytes stay in [`crate::ImportedProject`]
//! so unsupported content is never lost silently.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use nova_core::{
    Layer, ObjectInstance, PathCommand, PathShape, Project, Rgba, Scene, Script, SourceFormat,
    SourceInfo, SymbolClassLink, Transform,
};
use swf::{FillStyle, ShapeRecord, Tag};

use crate::capabilities::{CompatibilityReport, FormatKind, IssueCategory};
use crate::conversion::ImportedData;
use crate::error::{FormatError, FormatResult};

#[derive(Debug, Clone)]
struct ShapeDef {
    width: f32,
    height: f32,
    center_x: f32,
    center_y: f32,
    paths: Vec<PathShape>,
    fill: Rgba,
}

#[derive(Debug, Clone)]
struct SpriteDef {
    frames: Vec<Vec<DisplayEntry>>,
    num_frames: u16,
}

#[derive(Debug, Clone)]
struct DisplayEntry {
    depth: u16,
    character_id: u16,
    matrix: Affine,
    name: Option<String>,
    alpha: f32,
}

#[derive(Debug, Clone, Copy)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl Affine {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn from_swf(matrix: &swf::Matrix) -> Self {
        Self {
            a: matrix.a.to_f32(),
            b: matrix.b.to_f32(),
            c: matrix.c.to_f32(),
            d: matrix.d.to_f32(),
            tx: matrix.tx.to_pixels() as f32,
            ty: matrix.ty.to_pixels() as f32,
        }
    }

    fn compose(self, child: Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }

    fn transform_point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    fn to_transform(self) -> Transform {
        let scale_x = (self.a * self.a + self.b * self.b).sqrt();
        let mut scale_y = (self.c * self.c + self.d * self.d).sqrt();
        let determinant = self.a * self.d - self.b * self.c;
        if determinant < 0.0 {
            scale_y = -scale_y;
        }
        Transform {
            x: self.tx,
            y: self.ty,
            scale_x: if scale_x.abs() < 1e-6 { 1.0 } else { scale_x },
            scale_y: if scale_y.abs() < 1e-6 { 1.0 } else { scale_y },
            rotation: self.b.atan2(self.a).to_degrees(),
        }
    }
}

#[derive(Debug, Default)]
struct TimelineDef {
    frames: Vec<Vec<DisplayEntry>>,
    labels: BTreeMap<usize, String>,
}

pub fn import(bytes: &[u8]) -> FormatResult<ImportedData> {
    let swf_buf = swf::decompress_swf(bytes)
        .map_err(|error| FormatError::Swf(format!("Header/Dekomprimierung: {error:?}")))?;
    let movie = swf::parse_swf(&swf_buf)
        .map_err(|error| FormatError::Swf(format!("Tag-Parsing: {error:?}")))?;
    let header = movie.header.swf_header();
    let width = (header.stage_size.x_max - header.stage_size.x_min).to_pixels() as f32;
    let height = (header.stage_size.y_max - header.stage_size.y_min).to_pixels() as f32;
    let fps = header.frame_rate.to_f32();
    if width <= 0.0 || height <= 0.0 {
        return Err(FormatError::Swf(
            "Bühnenmaße sind leer oder ungültig".into(),
        ));
    }

    let mut report = CompatibilityReport::new(FormatKind::Swf);
    report.source_version = Some(format!("SWF {}", header.version));
    report.set_stat("Version", header.version.to_string());
    report.set_stat("Kompression", format!("{:?}", header.compression));
    report.set_stat("Bühne", format!("{width:.2} x {height:.2} px"));
    report.set_stat("FrameRate", format!("{fps:.3}"));
    report.set_stat("Header-Frames", header.num_frames.to_string());

    let mut shapes: HashMap<u16, ShapeDef> = HashMap::new();
    let mut sprites: HashMap<u16, &swf::Sprite<'_>> = HashMap::new();
    collect_definitions(&movie.tags, &mut shapes, &mut sprites);

    let mut sprite_defs: HashMap<u16, SpriteDef> = HashMap::new();
    for (id, sprite) in &sprites {
        let timeline = parse_timeline(&sprite.tags, &mut report, &format!("Sprite {id}"));
        sprite_defs.insert(
            *id,
            SpriteDef {
                frames: timeline.frames,
                num_frames: sprite.num_frames,
            },
        );
    }

    let mut unsupported_seen = HashSet::new();
    collect_unsupported(
        &movie.tags,
        "Haupt-Timeline",
        &mut report,
        &mut unsupported_seen,
    );

    let main_timeline = parse_timeline(&movie.tags, &mut report, "Haupt-Timeline");
    let frame_count = header
        .num_frames
        .max(main_timeline.frames.len() as u16)
        .max(1) as usize;
    let mut frames = main_timeline.frames;
    if frames.is_empty() {
        frames.push(Vec::new());
    }
    while frames.len() < frame_count {
        frames.push(frames.last().cloned().unwrap_or_default());
    }

    let depths: BTreeSet<u16> = frames
        .iter()
        .flat_map(|entries| entries.iter().map(|entry| entry.depth))
        .collect();

    let mut scene = Scene::new("SWF-Import", width, height);
    scene.fps = if (1.0..=120.0).contains(&fps) {
        fps
    } else {
        report.warning(
            IssueCategory::Timeline,
            format!("Ungültige FrameRate {fps}; verwende 24."),
        );
        24.0
    };
    if let Some(background) = movie.header.background_color() {
        scene.background = Rgba::rgba(background.r, background.g, background.b, background.a);
    }
    scene.layers.clear();

    for depth in depths {
        let mut layer = Layer::new(format!("Tiefe {depth}"), frame_count);
        for frame_index in 0..frame_count {
            let mut objects = Vec::new();
            for entry in frames
                .get(frame_index)
                .into_iter()
                .flatten()
                .filter(|entry| entry.depth == depth)
            {
                expand_entry(
                    entry,
                    entry.character_id,
                    frame_index,
                    &shapes,
                    &sprite_defs,
                    &mut objects,
                    &mut report,
                    0,
                );
            }
            layer.frames[frame_index].keyframe = true;
            layer.frames[frame_index].objects = objects;
        }
        scene.layers.push(layer);
    }
    if scene.layers.is_empty() {
        scene.layers.push(Layer::new("Ebene 1", frame_count));
        scene.layers[0].frames[0].keyframe = true;
    }
    for (frame_index, label) in &main_timeline.labels {
        if let Some(frame) = scene
            .layers
            .first_mut()
            .and_then(|layer| layer.frames.get_mut(*frame_index))
        {
            frame.label = Some(label.clone());
        }
    }

    let mut project = Project::new_empty("SWF-Import");
    project.metadata.name = "SWF-Import".into();
    project.metadata.description =
        "Aus SWF rekonstruiert. Nicht unterstützte Tags bleiben nur in der Originaldatei erhalten."
            .into();
    project.scenes = vec![scene];
    project.active_scene = project.scenes.first().map(|scene| scene.id);
    project.symbols.clear();
    project.scripts.clear();
    project.source = Some(SourceInfo {
        format: SourceFormat::Swf,
        original_path: None,
        original_version: Some(format!("SWF {}", header.version)),
        note: "Rekonstruktion aus SWF; Shapes und Sprite-Timelines sind gebacken.".into(),
        symbol_classes: Vec::new(),
    });

    let mut abc_count = 0;
    let mut symbol_classes: Vec<SymbolClassLink> = Vec::new();
    for tag in &movie.tags {
        match tag {
            Tag::DoAbc(data) => {
                abc_count += 1;
                project.scripts.push(imported_abc(
                    format!("ImportedBytecode{abc_count}.as"),
                    *data,
                    &mut report,
                ));
            }
            Tag::DoAbc2(data) => {
                abc_count += 1;
                let name = swf_str(data.name);
                let file = if name.is_empty() {
                    format!("ImportedBytecode{abc_count}.as")
                } else {
                    format!("{}.as", sanitize_file_name(&name))
                };
                project
                    .scripts
                    .push(imported_abc(file, data.data, &mut report));
            }
            Tag::SymbolClass(links) => {
                for link in links {
                    let class_name = swf_str(link.class_name).to_string();
                    symbol_classes.push(SymbolClassLink {
                        character_id: link.id,
                        class_name: class_name.clone(),
                    });
                    report.info(
                        IssueCategory::Code,
                        format!(
                            "SymbolClass: Charakter {} ist mit Klasse '{}' verknüpft.",
                            link.id, class_name
                        ),
                    );
                }
            }
            Tag::ExportAssets(assets) => {
                for asset in assets {
                    report.info(
                        IssueCategory::Assets,
                        format!(
                            "ExportAsset: '{}' (Charakter {}).",
                            swf_str(asset.name),
                            asset.id
                        ),
                    );
                }
            }
            Tag::Metadata(_) => {
                report.info(IssueCategory::Structure, "SWF enthält XMP-Metadaten.");
            }
            _ => {}
        }
    }

    if let Some(source) = &mut project.source {
        source.symbol_classes = symbol_classes.clone();
    }
    if !symbol_classes.is_empty() {
        report.info(
            IssueCategory::Code,
            format!(
                "{} SymbolClass-Verknüpfung(en) werden beim SWF-Export rekonstruiert.",
                symbol_classes.len()
            ),
        );
    }

    report.set_stat("Formen", shapes.len().to_string());
    report.set_stat("Sprites", sprites.len().to_string());
    report.set_stat("ABC-Skripte", abc_count.to_string());
    report.set_stat(
        "Rekonstruierte Ebenen",
        project.scenes[0].layers.len().to_string(),
    );
    report.set_stat("Rekonstruierte Frames", frame_count.to_string());
    if abc_count > 0 {
        report.warning(
            IssueCategory::Code,
            "ABC-Bytecode wurde erhalten, aber nicht dekompiliert. Der angezeigte Text ist keine Originalquelle.",
        );
    }

    Ok(ImportedData {
        project,
        report,
        source_version: Some(format!("SWF {}", header.version)),
    })
}

fn imported_abc(path: String, data: &[u8], report: &mut CompatibilityReport) -> Script {
    let note = format!(
        "// Preserved AVM2 ABC bytecode from the imported SWF.\n\
         // Original ActionScript source was not recovered.\n\
         // This text is a marker, not decompiled or original code.\n\
         // ABC size: {} bytes.\n",
        data.len()
    );
    let mut script = Script::imported_bytecode(format!("scripts/{path}"), note, data.to_vec());
    script.name = path;
    report.info(
        IssueCategory::Code,
        format!(
            "ABC-Bytes ({} Bytes) wurden unverändert als Skript gespeichert.",
            data.len()
        ),
    );
    script
}

fn sanitize_file_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.is_empty() {
        "ImportedBytecode".into()
    } else {
        clean
    }
}

fn collect_definitions<'a>(
    tags: &'a [Tag<'a>],
    shapes: &mut HashMap<u16, ShapeDef>,
    sprites: &mut HashMap<u16, &'a swf::Sprite<'a>>,
) {
    for tag in tags {
        match tag {
            Tag::DefineShape(shape) => {
                let mut ignored = CompatibilityReport::new(FormatKind::Swf);
                if let Some(def) = shape_to_def(shape, &mut ignored) {
                    shapes.insert(shape.id, def);
                }
            }
            Tag::DefineSprite(sprite) => {
                sprites.insert(sprite.id, sprite);
                collect_definitions(&sprite.tags, shapes, sprites);
            }
            _ => {}
        }
    }
}

fn parse_timeline(
    tags: &[Tag<'_>],
    report: &mut CompatibilityReport,
    context: &str,
) -> TimelineDef {
    let mut current: BTreeMap<u16, DisplayEntry> = BTreeMap::new();
    let mut timeline = TimelineDef::default();
    let mut pending_label: Option<String> = None;

    for tag in tags {
        match tag {
            Tag::PlaceObject(place) => {
                let depth = place.depth;
                let existing = current.get(&depth).cloned();
                let character_id = match place.action {
                    swf::PlaceObjectAction::Place(id) | swf::PlaceObjectAction::Replace(id) => id,
                    swf::PlaceObjectAction::Modify | swf::PlaceObjectAction::None => {
                        existing.as_ref().map_or(0, |entry| entry.character_id)
                    }
                };
                if character_id == 0 {
                    report.warning(
                        IssueCategory::Timeline,
                        format!("{context}: PlaceObject auf Tiefe {depth} hat keinen Charakter."),
                    );
                    continue;
                }
                let matrix = place
                    .matrix
                    .as_ref()
                    .map(Affine::from_swf)
                    .or_else(|| existing.as_ref().map(|entry| entry.matrix))
                    .unwrap_or(Affine::IDENTITY);
                let name = place
                    .name
                    .map(swf_str)
                    .or_else(|| existing.as_ref().and_then(|entry| entry.name.clone()));
                let alpha = place
                    .color_transform
                    .map(|transform| {
                        let alpha_mult: f32 = transform.a_multiply.into();
                        (existing.as_ref().map_or(1.0, |entry| entry.alpha) * alpha_mult)
                            .clamp(0.0, 1.0)
                    })
                    .unwrap_or_else(|| existing.as_ref().map_or(1.0, |entry| entry.alpha));
                if place.is_visible == Some(false) {
                    current.remove(&depth);
                    continue;
                }
                if place.filters.is_some() {
                    report.warning(
                        IssueCategory::Filter,
                        format!("{context}: Filter auf Tiefe {depth} werden nicht übernommen."),
                    );
                }
                current.insert(
                    depth,
                    DisplayEntry {
                        depth,
                        character_id,
                        matrix,
                        name,
                        alpha,
                    },
                );
            }
            Tag::RemoveObject(remove) => {
                current.remove(&remove.depth);
            }
            Tag::ShowFrame => {
                let frame_index = timeline.frames.len();
                if let Some(label) = pending_label.take() {
                    timeline.labels.insert(frame_index, label);
                }
                timeline
                    .frames
                    .push(current.values().cloned().collect::<Vec<_>>());
            }
            Tag::FrameLabel(label) => {
                pending_label = Some(swf_str(label.label));
            }
            Tag::DefineSceneAndFrameLabelData(data) => {
                for label in &data.frame_labels {
                    timeline
                        .labels
                        .insert(label.frame_num as usize, swf_str(label.label));
                }
            }
            Tag::End => break,
            _ => {}
        }
    }
    if timeline.frames.is_empty() {
        timeline.frames.push(current.values().cloned().collect());
    }
    timeline
}

fn expand_entry(
    entry: &DisplayEntry,
    root_character: u16,
    frame_index: usize,
    shapes: &HashMap<u16, ShapeDef>,
    sprites: &HashMap<u16, SpriteDef>,
    out: &mut Vec<ObjectInstance>,
    report: &mut CompatibilityReport,
    depth_limit: usize,
) {
    if depth_limit > 24 {
        report.warning(
            IssueCategory::Timeline,
            "Sprite-Verschachtelung überschreitet 24 Ebenen; weitere Inhalte werden übersprungen.",
        );
        return;
    }
    if let Some(shape) = shapes.get(&entry.character_id) {
        let (center_x, center_y) = entry.matrix.transform_point(shape.center_x, shape.center_y);
        let affine = Affine {
            tx: center_x,
            ty: center_y,
            ..entry.matrix
        };
        let mut paths = shape.paths.clone();
        if entry.alpha < 1.0 {
            for path in &mut paths {
                path.fill.a = ((path.fill.a as f32) * entry.alpha).round() as u8;
            }
        }
        let fill = paths.first().map(|path| path.fill).unwrap_or(shape.fill);
        let name = entry
            .name
            .clone()
            .unwrap_or_else(|| format!("Shape {}", entry.character_id));
        let mut object =
            ObjectInstance::path(name, 0.0, 0.0, shape.width, shape.height, paths, fill);
        object.transform = affine.to_transform();
        object.source_character = Some(root_character);
        out.push(object);
        return;
    }

    if let Some(sprite) = sprites.get(&entry.character_id) {
        if sprite.frames.is_empty() {
            report.warning(
                IssueCategory::Timeline,
                format!("Sprite {} enthält keine Frames.", entry.character_id),
            );
            return;
        }
        let local_frame = if sprite.num_frames == 0 {
            0
        } else {
            frame_index % sprite.frames.len()
        };
        let prefix = entry
            .name
            .clone()
            .unwrap_or_else(|| format!("Sprite {}", entry.character_id));
        for child in &sprite.frames[local_frame] {
            let mut child = child.clone();
            child.matrix = entry.matrix.compose(child.matrix);
            child.alpha *= entry.alpha;
            child.name = Some(format!(
                "{prefix}/{}",
                child
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Tiefe {}", child.depth))
            ));
            expand_entry(
                &child,
                root_character,
                frame_index,
                shapes,
                sprites,
                out,
                report,
                depth_limit + 1,
            );
        }
        return;
    }

    report.warning(
        IssueCategory::Structure,
        format!(
            "Charakter {} auf Tiefe {} ist weder Form noch Sprite und wird übersprungen.",
            entry.character_id, entry.depth
        ),
    );
}

fn shape_to_def(shape: &swf::Shape, report: &mut CompatibilityReport) -> Option<ShapeDef> {
    let bounds = &shape.shape_bounds;
    let width = (bounds.x_max - bounds.x_min).to_pixels() as f32;
    let height = (bounds.y_max - bounds.y_min).to_pixels() as f32;
    if width <= 0.0 || height <= 0.0 {
        report.warning(
            IssueCategory::Geometry,
            format!(
                "Shape {} hat leere Grenzen und wird übersprungen.",
                shape.id
            ),
        );
        return None;
    }
    let center_x = (bounds.x_min.to_pixels() + bounds.x_max.to_pixels()) as f32 / 2.0;
    let center_y = (bounds.y_min.to_pixels() + bounds.y_max.to_pixels()) as f32 / 2.0;
    let mut fills = shape.styles.fill_styles.clone();
    let mut paths: Vec<PathShape> = Vec::new();
    let mut current_fill: Option<u32> = None;
    let mut current_path: Option<PathShape> = None;
    let mut current = (0.0_f32, 0.0_f32);

    for record in &shape.shape {
        match record {
            ShapeRecord::StyleChange(change) => {
                if let Some(new_styles) = &change.new_styles {
                    fills.extend(new_styles.fill_styles.clone());
                }
                if change.fill_style_1.is_some() {
                    report.warning(
                        IssueCategory::Geometry,
                        format!("Shape {}: Zweite Füllung wird nicht übernommen.", shape.id),
                    );
                }
                if let Some(fill) = change.fill_style_0 {
                    current_fill = (fill != 0).then_some(fill);
                }
                if change.line_style.is_some() {
                    report.warning(
                        IssueCategory::Geometry,
                        format!(
                            "Shape {}: Linienstile werden als Füllung nicht übernommen.",
                            shape.id
                        ),
                    );
                }
                if let Some(move_to) = change.move_to {
                    if let Some(path) = current_path.take() {
                        if !path.commands.is_empty() {
                            paths.push(path);
                        }
                    }
                    current = (
                        move_to.x.to_pixels() as f32 - center_x,
                        move_to.y.to_pixels() as f32 - center_y,
                    );
                    if let Some(fill_index) = current_fill {
                        let fill = fill_color(fills.get(fill_index as usize - 1), report);
                        current_path = Some(PathShape::new(
                            fill,
                            vec![PathCommand::MoveTo {
                                x: current.0,
                                y: current.1,
                            }],
                        ));
                    }
                }
            }
            ShapeRecord::StraightEdge { delta } => {
                current.0 += delta.dx.to_pixels() as f32;
                current.1 += delta.dy.to_pixels() as f32;
                if let Some(path) = &mut current_path {
                    path.commands.push(PathCommand::LineTo {
                        x: current.0,
                        y: current.1,
                    });
                }
            }
            ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                let control_x = current.0 + control_delta.dx.to_pixels() as f32;
                let control_y = current.1 + control_delta.dy.to_pixels() as f32;
                current.0 = control_x + anchor_delta.dx.to_pixels() as f32;
                current.1 = control_y + anchor_delta.dy.to_pixels() as f32;
                if let Some(path) = &mut current_path {
                    path.commands.push(PathCommand::CurveTo {
                        control_x,
                        control_y,
                        x: current.0,
                        y: current.1,
                    });
                }
            }
        }
    }
    if let Some(path) = current_path.take() {
        if !path.commands.is_empty() {
            paths.push(path);
        }
    }
    if paths.is_empty() {
        let fill = fill_color(fills.first(), report);
        paths.push(PathShape::new(
            fill,
            vec![
                PathCommand::MoveTo {
                    x: -width / 2.0,
                    y: -height / 2.0,
                },
                PathCommand::LineTo {
                    x: width / 2.0,
                    y: -height / 2.0,
                },
                PathCommand::LineTo {
                    x: width / 2.0,
                    y: height / 2.0,
                },
                PathCommand::LineTo {
                    x: -width / 2.0,
                    y: height / 2.0,
                },
                PathCommand::LineTo {
                    x: -width / 2.0,
                    y: -height / 2.0,
                },
            ],
        ));
        report.warning(
            IssueCategory::Geometry,
            format!(
                "Shape {}: Keine Füllpfade erkannt; Bounding-Box wird als Rechteck verwendet.",
                shape.id
            ),
        );
    }
    Some(ShapeDef {
        width,
        height,
        center_x,
        center_y,
        paths,
        fill: Rgba::WHITE,
    })
}

fn fill_color(fill: Option<&FillStyle>, report: &mut CompatibilityReport) -> Rgba {
    let Some(fill) = fill else {
        return Rgba::rgb(180, 180, 180);
    };
    match fill {
        FillStyle::Color(color) => Rgba::rgba(color.r, color.g, color.b, color.a),
        FillStyle::LinearGradient(gradient)
        | FillStyle::RadialGradient(gradient)
        | FillStyle::FocalGradient { gradient, .. } => {
            report.warning(
                IssueCategory::Geometry,
                "Gradient-Füllung wird durch die erste Farbe angenähert.",
            );
            gradient
                .records
                .first()
                .map(|record| {
                    Rgba::rgba(
                        record.color.r,
                        record.color.g,
                        record.color.b,
                        record.color.a,
                    )
                })
                .unwrap_or_else(|| Rgba::rgb(180, 180, 180))
        }
        FillStyle::Bitmap { .. } => {
            report.warning(
                IssueCategory::Bitmap,
                "Bitmap-Füllung wird durch Grau ersetzt; das Original bleibt erhalten.",
            );
            Rgba::rgb(128, 128, 128)
        }
    }
}

fn collect_unsupported(
    tags: &[Tag<'_>],
    context: &str,
    report: &mut CompatibilityReport,
    seen: &mut HashSet<String>,
) {
    for tag in tags {
        match tag {
            Tag::DefineSprite(sprite) => {
                collect_unsupported(
                    &sprite.tags,
                    &format!("{context}/Sprite {}", sprite.id),
                    report,
                    seen,
                );
            }
            Tag::DefineShape(_)
            | Tag::ShowFrame
            | Tag::PlaceObject(_)
            | Tag::RemoveObject(_)
            | Tag::FrameLabel(_)
            | Tag::DoAbc(_)
            | Tag::DoAbc2(_)
            | Tag::SetBackgroundColor(_)
            | Tag::FileAttributes(_)
            | Tag::Metadata(_)
            | Tag::SymbolClass(_)
            | Tag::ExportAssets(_)
            | Tag::DefineSceneAndFrameLabelData(_)
            | Tag::ScriptLimits { .. }
            | Tag::ProductInfo(_)
            | Tag::DebugId(_)
            | Tag::Protect(_)
            | Tag::End => {}
            other => {
                let label = tag_label(other);
                let key = format!("{context}:{label}");
                if seen.insert(key) {
                    report.warning(
                        IssueCategory::UnsupportedTag,
                        format!(
                            "{context}: Tag {label} wird nicht in das editierbare Modell übernommen; die Originaldatei bleibt erhalten."
                        ),
                    );
                }
            }
        }
    }
}

fn tag_label(tag: &Tag<'_>) -> String {
    match tag {
        Tag::Unknown { tag_code, .. } => format!("Unknown({tag_code})"),
        Tag::DefineBits { .. } => "DefineBits".into(),
        Tag::DefineBitsJpeg2 { .. } => "DefineBitsJpeg2".into(),
        Tag::DefineBitsJpeg3(_) => "DefineBitsJpeg3".into(),
        Tag::DefineBitsLossless(_) => "DefineBitsLossless".into(),
        Tag::DefineButton(_) => "DefineButton".into(),
        Tag::DefineButton2(_) => "DefineButton2".into(),
        Tag::DefineEditText(_) => "DefineEditText".into(),
        Tag::DefineFont(_) => "DefineFont".into(),
        Tag::DefineFont2(_) => "DefineFont2".into(),
        Tag::DefineFont4(_) => "DefineFont4".into(),
        Tag::DefineMorphShape(_) => "DefineMorphShape".into(),
        Tag::DefineSound(_) => "DefineSound".into(),
        Tag::DefineText(_) => "DefineText".into(),
        Tag::DefineText2(_) => "DefineText2".into(),
        Tag::DefineVideoStream(_) => "DefineVideoStream".into(),
        Tag::DoAction(_) => "DoAction (AVM1)".into(),
        Tag::DoInitAction { .. } => "DoInitAction (AVM1)".into(),
        Tag::SoundStreamBlock(_) => "SoundStreamBlock".into(),
        Tag::StartSound { .. } => "StartSound".into(),
        Tag::StartSound2 { .. } => "StartSound2".into(),
        Tag::VideoFrame(_) => "VideoFrame".into(),
        Tag::ImportAssets { .. } => "ImportAssets".into(),
        Tag::JpegTables(_) => "JpegTables".into(),
        Tag::DefineBinaryData(_) => "DefineBinaryData".into(),
        Tag::CsmTextSettings(_) => "CsmTextSettings".into(),
        _ => "weiteres Tag".into(),
    }
}

fn swf_str(value: &swf::SwfStr) -> String {
    String::from_utf8_lossy(value.as_bytes()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conversion::export_swf;
    use nova_build::build_swf;
    use nova_core::{CodeProvenance, SwfCompression};

    #[test]
    fn imports_nova_exported_swf() {
        let project = Project::demo();
        let output = build_swf(&project).unwrap();
        let imported = import(&output.bytes).unwrap();
        assert_eq!(imported.project.scenes.len(), 1);
        assert!(imported.project.scenes[0].frame_count() >= 1);
        assert!(imported.report.stats.contains_key("Formen"));
        assert!(
            imported
                .project
                .scripts
                .iter()
                .any(|script| script.provenance == CodeProvenance::CompiledBytecode)
        );
        assert!(
            imported
                .project
                .scripts
                .iter()
                .all(|script| script.abc.is_some())
        );
    }

    #[test]
    fn rejects_corrupt_swf() {
        let error = import(b"not a swf").unwrap_err();
        assert!(matches!(error, FormatError::Swf(_)));
    }

    #[test]
    fn imports_fws_cws_and_reports_the_version() {
        for compression in [SwfCompression::None, SwfCompression::Zlib] {
            let mut project = Project::demo();
            project.export.compression = compression;
            project.export.swf_version = 10;
            let output = build_swf(&project).unwrap();
            let signature = &output.bytes[0..3];
            match compression {
                SwfCompression::None => assert_eq!(signature, b"FWS"),
                SwfCompression::Zlib => assert_eq!(signature, b"CWS"),
            }
            let imported = import(&output.bytes).unwrap();
            assert_eq!(imported.report.source_version.as_deref(), Some("SWF 10"));
            assert_eq!(imported.project.scenes[0].width, 550.0);
            assert_eq!(imported.project.scenes[0].height, 400.0);
            assert!((imported.project.scenes[0].fps - 24.0).abs() < 0.01);
        }
    }

    #[test]
    fn reconstructs_sprites_layers_labels_and_preserves_abc() {
        let project = Project::demo();
        let output = build_swf(&project).unwrap();
        let imported = import(&output.bytes).unwrap();
        assert!(
            imported
                .report
                .stats
                .get("Sprites")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0)
                >= 1
        );
        assert!(imported.project.scenes[0].layers.len() >= 4);
        assert!(imported.project.scenes[0].layers.iter().any(|layer| {
            layer
                .frames
                .iter()
                .any(|frame| frame.label.as_deref() == Some("intro"))
        }));
        let script = imported.project.scripts.first().unwrap();
        assert_eq!(script.provenance, CodeProvenance::CompiledBytecode);
        let abc = script.abc.clone().unwrap();
        assert!(!abc.is_empty());
        let source = imported.project.source.as_ref().unwrap();
        assert!(
            !source.symbol_classes.is_empty(),
            "SymbolClass links must be preserved"
        );
        assert!(
            imported.project.scenes[0]
                .layers
                .iter()
                .flat_map(|layer| layer.frames.iter())
                .flat_map(|frame| frame.objects.iter())
                .any(|object| object.source_character.is_some()),
            "reconstructed objects must retain their source character id"
        );

        let reexported = export_swf(&imported.project).unwrap();
        let swf_buf = swf::decompress_swf(&reexported.bytes[..]).unwrap();
        let movie = swf::parse_swf(&swf_buf).unwrap();
        let preserved: Vec<&[u8]> = movie
            .tags
            .iter()
            .filter_map(|tag| match tag {
                Tag::DoAbc(data) => Some(*data),
                Tag::DoAbc2(data) => Some(data.data),
                _ => None,
            })
            .collect();
        assert_eq!(preserved.len(), 1);
        assert_eq!(preserved[0], abc.as_slice());
        let symbol_links: Vec<(u16, String)> = movie
            .tags
            .iter()
            .filter_map(|tag| match tag {
                Tag::SymbolClass(links) => Some(
                    links
                        .iter()
                        .map(|link| (link.id, swf_str(link.class_name)))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(
            !symbol_links.is_empty(),
            "re-export must recreate SymbolClass links"
        );
        assert!(
            symbol_links
                .iter()
                .any(|(_, class_name)| class_name.contains("Main"))
        );
    }

    #[test]
    fn reports_unknown_tags_without_dropping_the_original() {
        let mut header = swf::Header::default_with_swf_version(10);
        header.stage_size = swf::Rectangle {
            x_min: swf::Twips::from_pixels(0.0),
            x_max: swf::Twips::from_pixels(550.0),
            y_min: swf::Twips::from_pixels(0.0),
            y_max: swf::Twips::from_pixels(400.0),
        };
        header.frame_rate = swf::Fixed8::from_f32(24.0);
        header.num_frames = 1;
        let tags = vec![
            Tag::SetBackgroundColor(swf::Color::from_rgb(0x101218, 255)),
            Tag::ShowFrame,
            Tag::Unknown {
                tag_code: 999,
                data: &[],
            },
            Tag::End,
        ];
        let mut patched = Vec::new();
        swf::write_swf(&header, &tags, &mut patched).unwrap();
        let imported = import(&patched).unwrap();
        assert!(
            imported
                .report
                .issues
                .iter()
                .any(|issue| issue.message.contains("Unknown(999)"))
        );
        assert_eq!(
            imported
                .report
                .count(crate::capabilities::IssueSeverity::Warning),
            1
        );
    }

    #[test]
    fn imports_zws_lzma() {
        let mut header = swf::Header::default_with_swf_version(13);
        header.compression = swf::Compression::Lzma;
        header.stage_size = swf::Rectangle {
            x_min: swf::Twips::from_pixels(0.0),
            x_max: swf::Twips::from_pixels(320.0),
            y_min: swf::Twips::from_pixels(0.0),
            y_max: swf::Twips::from_pixels(240.0),
        };
        header.frame_rate = swf::Fixed8::from_f32(12.0);
        header.num_frames = 1;
        let tags = vec![Tag::ShowFrame, Tag::End];
        let mut bytes = Vec::new();
        swf::write_swf(&header, &tags, &mut bytes).unwrap();
        assert_eq!(&bytes[0..3], b"ZWS");
        let imported = import(&bytes).unwrap();
        assert_eq!(imported.report.source_version.as_deref(), Some("SWF 13"));
        assert_eq!(imported.project.scenes[0].width, 320.0);
        assert_eq!(imported.project.scenes[0].height, 240.0);
    }

    #[test]
    fn imports_swf_version_nine() {
        let mut project = Project::demo();
        project.export.swf_version = 9;
        project.scripts.clear();
        let output = build_swf(&project).unwrap();
        let imported = import(&output.bytes).unwrap();
        assert_eq!(imported.report.source_version.as_deref(), Some("SWF 9"));
    }
}
