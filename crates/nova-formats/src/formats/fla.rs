//! FLA/XFL import and an explicitly experimental XFL compatibility export.
//!
//! Modern `.fla` files are ZIP containers with XFL XML. This module implements
//! a documented subset. It never pretends to support the entire XFL schema and
//! it never writes a `.fla` file: the exporter produces a clearly labelled
//! `.xfl.zip` package whose validity has not been tested in Adobe Animate.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};

use nova_core::{
    Frame, Layer, LayerKind, ObjectInstance, ObjectKind, PathCommand, PathShape, Project, Rgba,
    Scene, Script, SourceFormat, SourceInfo, Symbol, Transform,
};
use roxmltree::{Document, Node};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::capabilities::{CompatibilityReport, FormatKind, IssueCategory};
use crate::conversion::{ExportData, ImportedData};
use crate::error::{FormatError, FormatResult};

pub fn import(bytes: &[u8]) -> FormatResult<ImportedData> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| FormatError::Zip(error.to_string()))?;
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| FormatError::Zip(error.to_string()))?;
        if !file.is_file() {
            continue;
        }
        let name = file
            .name()
            .map_err(|error| FormatError::Zip(error.to_string()))?
            .replace('\\', "/");
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        files.insert(name, data);
    }

    let document_name = files
        .keys()
        .find(|name| {
            name.eq_ignore_ascii_case("DOMDocument.xml")
                || name.to_ascii_lowercase().ends_with("/domdocument.xml")
        })
        .cloned()
        .ok_or_else(|| {
            FormatError::Fla("ZIP enthält keine DOMDocument.xml; kein XFL-Container.".into())
        })?;
    let document_text = String::from_utf8_lossy(&files[&document_name]);
    let document =
        Document::parse(&document_text).map_err(|error| FormatError::Xml(error.to_string()))?;
    let root = document.root_element();

    let mut report = CompatibilityReport::new(FormatKind::Fla);
    report.source_version = Some("XFL/ZIP".into());
    report.set_stat("Container-Einträge", files.len().to_string());

    let width = attr_f32(root, "width")
        .or_else(|| child_text_f32(root, "width"))
        .unwrap_or(550.0);
    let height = attr_f32(root, "height")
        .or_else(|| child_text_f32(root, "height"))
        .unwrap_or(400.0);
    let fps = attr_f32(root, "frameRate")
        .or_else(|| child_text_f32(root, "frameRate"))
        .unwrap_or(24.0);
    if !(1.0..=8192.0).contains(&width) || !(1.0..=8192.0).contains(&height) {
        return Err(FormatError::Fla(format!(
            "Ungültige Bühnenmaße in DOMDocument.xml: {width} x {height}"
        )));
    }

    let mut scene = Scene::new("FLA-Import", width, height);
    scene.fps = if (1.0..=120.0).contains(&fps) {
        fps
    } else {
        report.warning(
            IssueCategory::Timeline,
            format!("Ungültige FrameRate {fps}; verwende 24."),
        );
        24.0
    };
    if let Some(color) = attr_string(root, "backgroundColor")
        .or_else(|| child_text(root, "backgroundColor"))
        .and_then(|value| parse_color(&value))
    {
        scene.background = color;
    }

    let timeline_node = root
        .descendants()
        .find(|node| node.has_tag_name("DOMTimeline"));
    let layers_node = timeline_node.and_then(|timeline| {
        timeline
            .descendants()
            .find(|node| node.has_tag_name("layers"))
    });

    let mut symbols: Vec<Symbol> = Vec::new();
    let mut symbol_ids: HashMap<String, Uuid> = HashMap::new();
    let mut layers: Vec<Layer> = Vec::new();
    let mut shape_count = 0usize;
    let mut unsupported_count = 0usize;

    if let Some(layers_node) = layers_node {
        for layer_node in layers_node
            .children()
            .filter(|node| node.has_tag_name("DOMLayer"))
        {
            let name = attr_string(layer_node, "name").unwrap_or_else(|| "Ebene".into());
            let visible = attr_string(layer_node, "visible")
                .map(|value| value != "false")
                .unwrap_or(true);
            let locked = attr_string(layer_node, "locked")
                .map(|value| value == "true")
                .unwrap_or(false);
            let kind = match attr_string(layer_node, "layerType")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "folder" => LayerKind::Folder,
                "guide" => LayerKind::Guide,
                "mask" | "masked" => LayerKind::Mask,
                _ => LayerKind::Normal,
            };
            let mut frames: Vec<Frame> = Vec::new();
            if let Some(frames_node) = layer_node
                .children()
                .find(|node| node.has_tag_name("frames"))
            {
                for frame_node in frames_node
                    .children()
                    .filter(|node| node.has_tag_name("DOMFrame"))
                {
                    let index = attr_usize(frame_node, "index").unwrap_or(frames.len());
                    let key_mode = attr_usize(frame_node, "keyMode").unwrap_or(9728);
                    let label = attr_string(frame_node, "label");
                    let mut objects = Vec::new();
                    if let Some(elements) = frame_node
                        .children()
                        .find(|node| node.has_tag_name("elements"))
                    {
                        for element in elements.children().filter(Node::is_element) {
                            shape_count += parse_element(
                                element,
                                &mut objects,
                                &mut symbols,
                                &mut symbol_ids,
                                &mut report,
                            );
                        }
                    }
                    while frames.len() <= index {
                        frames.push(Frame::default());
                    }
                    frames[index] = Frame {
                        keyframe: key_mode != 0,
                        objects,
                        tween: None,
                        label,
                    };
                }
            }
            if frames.is_empty() {
                frames.push(Frame {
                    keyframe: true,
                    ..Default::default()
                });
            }
            let mut layer = Layer::new(name, frames.len());
            layer.visible = visible;
            layer.locked = locked;
            layer.kind = kind;
            layer.frames = frames;
            layers.push(layer);
        }
    }
    if layers.is_empty() {
        report.warning(
            IssueCategory::Timeline,
            "Keine DOMLayer gefunden; lege eine leere Ebene an.",
        );
        layers.push(Layer::new("Ebene 1", 1));
        layers[0].frames[0].keyframe = true;
    }
    scene.layers = layers;

    let mut project = Project::new_empty("FLA-Import");
    project.metadata.name = "FLA-Import".into();
    project.metadata.description =
        "Aus FLA/XFL importiert. Nicht unterstützte XFL-Elemente bleiben nur in der Originaldatei erhalten."
            .into();
    project.scenes = vec![scene];
    project.active_scene = project.scenes.first().map(|scene| scene.id);
    project.symbols = symbols;
    project.scripts.clear();
    project.source = Some(SourceInfo {
        format: SourceFormat::Fla,
        original_path: None,
        original_version: Some("XFL/ZIP".into()),
        note: "XFL-Teilimport; Text, Bitmaps, Sounds und komplexe Formen sind nicht editierbar."
            .into(),
        symbol_classes: Vec::new(),
    });

    let mut script_count = 0usize;
    for (name, data) in &files {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".as") {
            let source = String::from_utf8_lossy(data).into_owned();
            let base = name.rsplit('/').next().unwrap_or(name);
            let path = unique_script_path(&project, base);
            project.scripts.push(Script::new(path, source));
            script_count += 1;
        } else if !is_known_xfl_entry(name) {
            unsupported_count += 1;
            report.warning(
                IssueCategory::UnsupportedTag,
                format!("FLA-Eintrag '{name}' wird nicht in das Projekt übernommen."),
            );
        }
    }

    report.set_stat("Ebenen", project.scenes[0].layers.len().to_string());
    report.set_stat("Frames", project.scenes[0].frame_count().to_string());
    report.set_stat("Symbole", project.symbols.len().to_string());
    report.set_stat("Formen/Elemente", shape_count.to_string());
    report.set_stat("Skripte", script_count.to_string());
    report.set_stat("Nicht übernommene Einträge", unsupported_count.to_string());
    report.info(
        IssueCategory::Geometry,
        "XFL-Positionen werden als Mittelpunkt interpretiert; komplexe Edge-Geometrie wird als Bounding-Box angenähert.",
    );

    Ok(ImportedData {
        project,
        report,
        source_version: Some("XFL/ZIP".into()),
    })
}

fn parse_element(
    element: Node<'_, '_>,
    objects: &mut Vec<ObjectInstance>,
    symbols: &mut Vec<Symbol>,
    symbol_ids: &mut HashMap<String, Uuid>,
    report: &mut CompatibilityReport,
) -> usize {
    let tag = element.tag_name().name();
    match tag {
        "DOMSymbolInstance" => {
            let library_name = attr_string(element, "libraryItemName")
                .or_else(|| attr_string(element, "name"))
                .unwrap_or_else(|| "Symbol".into());
            let width = attr_f32(element, "width").unwrap_or(100.0).max(1.0);
            let height = attr_f32(element, "height").unwrap_or(100.0).max(1.0);
            let fill = attr_string(element, "tintColor")
                .and_then(|value| parse_color(&value))
                .unwrap_or(Rgba::rgb(120, 180, 240));
            let symbol_id = *symbol_ids.entry(library_name.clone()).or_insert_with(|| {
                let symbol = Symbol::new(library_name.clone(), width, height, fill);
                let id = symbol.id;
                symbols.push(symbol);
                id
            });
            let mut object = ObjectInstance::symbol(
                attr_string(element, "name").unwrap_or_else(|| library_name.clone()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                symbol_id,
                fill,
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            0
        }
        "DOMShape" => {
            let (width, height) = shape_bounds(element);
            let fill = shape_fill(element);
            let paths = vec![rectangle_path(width, height, fill)];
            let mut object = ObjectInstance::path(
                attr_string(element, "name").unwrap_or_else(|| "Form".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                width,
                height,
                paths,
                fill,
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            report.warning(
                IssueCategory::Geometry,
                "DOMShape wird als Rechteck mit den erkannten Grenzen importiert; XFL-Edge-Strings werden nicht vollständig dekodiert.",
            );
            1
        }
        "DOMRectangleObject" => {
            let width = attr_f32(element, "width").unwrap_or(100.0).max(1.0);
            let height = attr_f32(element, "height").unwrap_or(100.0).max(1.0);
            let fill = shape_fill(element);
            let mut object = ObjectInstance::rect(
                attr_string(element, "name").unwrap_or_else(|| "Rechteck".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                width,
                height,
                fill,
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            1
        }
        "DOMEllipseObject" => {
            let width = attr_f32(element, "width").unwrap_or(100.0).max(1.0);
            let height = attr_f32(element, "height").unwrap_or(100.0).max(1.0);
            let fill = shape_fill(element);
            let mut object = ObjectInstance::ellipse(
                attr_string(element, "name").unwrap_or_else(|| "Ellipse".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                width,
                height,
                fill,
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            1
        }
        "DOMGroup" => {
            let mut count = 0;
            for child in element.children().filter(Node::is_element) {
                count += parse_element(child, objects, symbols, symbol_ids, report);
            }
            count
        }
        "DOMBitmapInstance" => {
            report.warning(
                IssueCategory::Bitmap,
                "DOMBitmapInstance wird als grauer Platzhalter importiert; das Bitmap bleibt in der Originaldatei.",
            );
            let width = attr_f32(element, "width").unwrap_or(100.0).max(1.0);
            let height = attr_f32(element, "height").unwrap_or(100.0).max(1.0);
            let mut object = ObjectInstance::rect(
                attr_string(element, "name").unwrap_or_else(|| "Bitmap".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                width,
                height,
                Rgba::rgb(128, 128, 128),
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            1
        }
        "DOMTextInstance" => {
            report.warning(
                IssueCategory::Text,
                "DOMTextInstance wird als Platzhalter importiert; Textbearbeitung ist nicht Teil dieses Imports.",
            );
            let width = attr_f32(element, "width").unwrap_or(160.0).max(1.0);
            let height = attr_f32(element, "height").unwrap_or(40.0).max(1.0);
            let mut object = ObjectInstance::rect(
                attr_string(element, "name").unwrap_or_else(|| "Text".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                width,
                height,
                Rgba::rgba(200, 200, 200, 80),
            );
            apply_xfl_transform(element, &mut object.transform);
            objects.push(object);
            1
        }
        _ => {
            report.warning(
                IssueCategory::UnsupportedTag,
                format!("XFL-Element <{tag}> wird nicht importiert."),
            );
            0
        }
    }
}

fn shape_bounds(element: Node<'_, '_>) -> (f32, f32) {
    let width = attr_f32(element, "width")
        .or_else(|| attr_f32(element, "w"))
        .or_else(|| {
            element
                .descendants()
                .find(|node| node.has_tag_name("bounds"))
                .and_then(|bounds| {
                    let x_min = attr_f32(bounds, "xMin").unwrap_or(0.0);
                    let x_max = attr_f32(bounds, "xMax").unwrap_or(100.0);
                    Some((x_max - x_min).abs())
                })
        })
        .unwrap_or(100.0);
    let height = attr_f32(element, "height")
        .or_else(|| attr_f32(element, "h"))
        .or_else(|| {
            element
                .descendants()
                .find(|node| node.has_tag_name("bounds"))
                .and_then(|bounds| {
                    let y_min = attr_f32(bounds, "yMin").unwrap_or(0.0);
                    let y_max = attr_f32(bounds, "yMax").unwrap_or(100.0);
                    Some((y_max - y_min).abs())
                })
        })
        .unwrap_or(100.0);
    (width.max(1.0), height.max(1.0))
}

fn shape_fill(element: Node<'_, '_>) -> Rgba {
    attr_string(element, "fillColor")
        .or_else(|| attr_string(element, "color"))
        .or_else(|| {
            element
                .descendants()
                .find(|node| node.has_tag_name("SolidColor"))
                .and_then(|node| attr_string(node, "color"))
        })
        .and_then(|value| parse_color(&value))
        .unwrap_or(Rgba::rgb(120, 180, 240))
}

fn rectangle_path(width: f32, height: f32, fill: Rgba) -> PathShape {
    let (hw, hh) = (width / 2.0, height / 2.0);
    PathShape::new(
        fill,
        vec![
            PathCommand::MoveTo { x: -hw, y: -hh },
            PathCommand::LineTo { x: hw, y: -hh },
            PathCommand::LineTo { x: hw, y: hh },
            PathCommand::LineTo { x: -hw, y: hh },
            PathCommand::LineTo { x: -hw, y: -hh },
        ],
    )
}

fn apply_xfl_transform(element: Node<'_, '_>, transform: &mut Transform) {
    if let Some(scale_x) = attr_f32(element, "scaleX") {
        transform.scale_x = scale_x;
    }
    if let Some(scale_y) = attr_f32(element, "scaleY") {
        transform.scale_y = scale_y;
    }
    if let Some(rotation) = attr_f32(element, "rotation") {
        transform.rotation = rotation;
    }
    if let Some(x) = attr_f32(element, "x") {
        transform.x = x;
    }
    if let Some(y) = attr_f32(element, "y") {
        transform.y = y;
    }
}

fn unique_script_path(project: &Project, base: &str) -> String {
    let mut path = format!("scripts/{base}");
    if !path.to_ascii_lowercase().ends_with(".as") {
        path.push_str(".as");
    }
    let mut suffix = 1;
    let original = path.clone();
    while project.scripts.iter().any(|script| script.path == path) {
        suffix += 1;
        path = format!("scripts/{}{}.as", original.trim_end_matches(".as"), suffix);
    }
    path
}

fn is_known_xfl_entry(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "domdocument.xml"
        || lower.ends_with("/domdocument.xml")
        || lower.ends_with(".as")
        || lower.starts_with("library/")
        || lower.starts_with("bin/")
        || lower.starts_with("sounds/")
        || lower.starts_with("images/")
        || lower.starts_with("video/")
        || lower.ends_with(".xml")
        || lower.ends_with(".dat")
}

fn attr_string(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name).map(str::to_string)
}

fn attr_f32(node: Node<'_, '_>, name: &str) -> Option<f32> {
    node.attribute(name).and_then(|value| value.parse().ok())
}

fn attr_usize(node: Node<'_, '_>, name: &str) -> Option<usize> {
    node.attribute(name).and_then(|value| value.parse().ok())
}

fn child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.has_tag_name(name))
        .and_then(|child| child.text())
        .map(str::to_string)
}

fn child_text_f32(node: Node<'_, '_>, name: &str) -> Option<f32> {
    child_text(node, name).and_then(|value| value.parse().ok())
}

fn parse_color(value: &str) -> Option<Rgba> {
    let value = value.trim().trim_start_matches('#');
    if value.len() == 6 {
        let r = u8::from_str_radix(&value[0..2], 16).ok()?;
        let g = u8::from_str_radix(&value[2..4], 16).ok()?;
        let b = u8::from_str_radix(&value[4..6], 16).ok()?;
        Some(Rgba::rgb(r, g, b))
    } else if value.len() == 8 {
        let r = u8::from_str_radix(&value[0..2], 16).ok()?;
        let g = u8::from_str_radix(&value[2..4], 16).ok()?;
        let b = u8::from_str_radix(&value[4..6], 16).ok()?;
        let a = u8::from_str_radix(&value[6..8], 16).ok()?;
        Some(Rgba::rgba(r, g, b, a))
    } else {
        None
    }
}

pub fn export_xfl(project: &Project) -> FormatResult<ExportData> {
    let scene = project
        .active_scene()
        .ok_or_else(|| FormatError::Unsupported("Projekt enthält keine aktive Szene".into()))?;
    let mut report = CompatibilityReport::new(FormatKind::XflPackage);
    report.source_version = Some("XFL compatibility package".into());
    report.warning(
        IssueCategory::Export,
        "Dies ist kein nativer .fla-Export. Das ZIP/XFL-Paket wurde nicht in Adobe Animate getestet.",
    );
    if project.scripts.iter().any(|script| script.abc.is_some()) {
        report.warning(
            IssueCategory::Code,
            "Skripte mit erhaltenem ABC können nicht als XFL-Quelltext exportiert werden und werden ausgelassen.",
        );
    }

    let mut document = String::new();
    document.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    document.push_str(&format!(
        "<DOMDocument xmlns=\"http://ns.adobe.com/xfl/2008/\" width=\"{}\" height=\"{}\" frameRate=\"{}\" backgroundColor=\"{}\" xflVersion=\"2.0\">\n",
        scene.width,
        scene.height,
        scene.fps,
        color_hex(scene.background)
    ));
    document.push_str("  <timelines>\n");
    document.push_str(&format!(
        "    <DOMTimeline name=\"{}\">\n      <layers>\n",
        xml_escape(&scene.name)
    ));
    for (layer_index, layer) in scene.layers.iter().enumerate() {
        document.push_str(&format!(
            "        <DOMLayer name=\"{}\" color=\"#4FFF4F\" current=\"{}\" isSelected=\"{}\" visible=\"{}\">\n          <frames>\n",
            xml_escape(&layer.name),
            if layer_index == 0 { "true" } else { "false" },
            if layer_index == 0 { "true" } else { "false" },
            if layer.visible { "true" } else { "false" }
        ));
        for (frame_index, frame) in layer.frames.iter().enumerate() {
            document.push_str(&format!(
                "            <DOMFrame index=\"{}\" keyMode=\"{}\"{}>\n",
                frame_index,
                if frame.keyframe { 9728 } else { 0 },
                frame
                    .label
                    .as_ref()
                    .map(|label| format!(" label=\"{}\"", xml_escape(label)))
                    .unwrap_or_default()
            ));
            if !frame.objects.is_empty() {
                document.push_str("              <elements>\n");
                for object in &frame.objects {
                    write_xfl_object(&mut document, object, project, &mut report);
                }
                document.push_str("              </elements>\n");
            }
            document.push_str("            </DOMFrame>\n");
        }
        document.push_str("          </frames>\n        </DOMLayer>\n");
    }
    document.push_str("      </layers>\n    </DOMTimeline>\n  </timelines>\n");

    let exportable_scripts: Vec<_> = project
        .scripts
        .iter()
        .filter(|script| script.abc.is_none())
        .collect();
    if !exportable_scripts.is_empty() {
        document.push_str("  <scripts>\n");
        for script in &exportable_scripts {
            document.push_str(&format!(
                "    <Script source=\"{}\"/>\n",
                xml_escape(&script.path)
            ));
        }
        document.push_str("  </scripts>\n");
    }
    document.push_str("</DOMDocument>\n");

    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer
            .start_file("DOMDocument.xml", options)
            .map_err(|error| FormatError::Zip(error.to_string()))?;
        writer.write_all(document.as_bytes())?;
        for script in &exportable_scripts {
            let name = script.path.trim_start_matches('/').replace('\\', "/");
            writer
                .start_file(name, options)
                .map_err(|error| FormatError::Zip(error.to_string()))?;
            writer.write_all(script.source.as_bytes())?;
        }
        writer
            .start_file("README.txt", options)
            .map_err(|error| FormatError::Zip(error.to_string()))?;
        writer.write_all(
            "NOVA Flash Studio XFL compatibility package\n\
             ==========================================\n\
             This ZIP is NOT a native Adobe .fla file and has not been tested in\n\
             Adobe Animate or Flash. It contains a minimal DOMDocument.xml and\n\
             ActionScript sources. Shapes are approximated from the NØVA model.\n\
             Keep the original project and test the package before relying on it.\n"
                .as_bytes(),
        )?;
        writer
            .finish()
            .map_err(|error| FormatError::Zip(error.to_string()))?;
    }

    report.set_stat("Ebenen", scene.layers.len().to_string());
    report.set_stat("Frames", scene.frame_count().to_string());
    report.set_stat("Skripte", exportable_scripts.len().to_string());
    Ok(ExportData {
        bytes: cursor.into_inner(),
        report,
    })
}

fn write_xfl_object(
    document: &mut String,
    object: &ObjectInstance,
    project: &Project,
    report: &mut CompatibilityReport,
) {
    match &object.kind {
        ObjectKind::Rect {
            width,
            height,
            corner_radius,
        } => {
            let mut points = rectangle_points(*width, *height);
            if *corner_radius > 0.0 {
                report.warning(
                    IssueCategory::Geometry,
                    format!(
                        "Objekt '{}': Eckenradius wird im XFL-Paket nicht als Rundung exportiert.",
                        object.name
                    ),
                );
            }
            write_xfl_shape(document, object, &mut points, report);
        }
        ObjectKind::Ellipse { width, height } => {
            let mut points = ellipse_points(*width, *height);
            write_xfl_shape(document, object, &mut points, report);
        }
        ObjectKind::Text(text) => {
            let paths = text.paths(object.fill);
            document.push_str("                <DOMShape>\n                  <fills>\n");
            for (index, path) in paths.iter().enumerate() {
                document.push_str(&format!(
                    "                    <FillStyle index=\"{}\"><SolidColor color=\"{}\"/></FillStyle>\n",
                    index + 1,
                    color_hex(path.fill)
                ));
            }
            document.push_str("                  </fills>\n                  <edges>\n");
            for (index, path) in paths.iter().enumerate() {
                let points = flatten_path(path);
                if points.len() >= 2 {
                    document.push_str(&format!(
                        "                    <Edge fillStyle0=\"{}\" edges=\"{}\"/>\n",
                        index + 1,
                        edge_string(&points)
                    ));
                }
            }
            document.push_str("                  </edges>\n                </DOMShape>\n");
            report.warning(
                IssueCategory::Export,
                format!(
                    "Textobjekt '{}' wird im XFL-Paket als eingebrannte Vektorform exportiert, nicht als bearbeitbarer Text.",
                    object.name
                ),
            );
        }
        ObjectKind::Path { paths, .. } => {
            document.push_str("                <DOMShape>\n                  <fills>\n");
            for (index, path) in paths.iter().enumerate() {
                document.push_str(&format!(
                    "                    <FillStyle index=\"{}\"><SolidColor color=\"{}\"/></FillStyle>\n",
                    index + 1,
                    color_hex(path.fill)
                ));
            }
            document.push_str("                  </fills>\n                  <edges>\n");
            for (index, path) in paths.iter().enumerate() {
                let points = flatten_path(path);
                if points.len() >= 2 {
                    document.push_str(&format!(
                        "                    <Edge fillStyle0=\"{}\" edges=\"{}\"/>\n",
                        index + 1,
                        edge_string(&points)
                    ));
                }
            }
            document.push_str("                  </edges>\n                </DOMShape>\n");
        }
        ObjectKind::Symbol { symbol } => {
            let symbol_name = project
                .symbol(*symbol)
                .map(|symbol| symbol.name.clone())
                .unwrap_or_else(|| "Symbol".into());
            document.push_str(&format!(
                "                <DOMSymbolInstance libraryItemName=\"{}\" symbolType=\"graphic\" x=\"{}\" y=\"{}\" scaleX=\"{}\" scaleY=\"{}\" rotation=\"{}\"/>\n",
                xml_escape(&symbol_name),
                object.transform.x,
                object.transform.y,
                object.transform.scale_x,
                object.transform.scale_y,
                object.transform.rotation
            ));
            report.warning(
                IssueCategory::Export,
                format!(
                    "Symbol '{}' wird als Instanz geschrieben; die Bibliotheksdefinition ist minimal.",
                    symbol_name
                ),
            );
        }
    }
}

fn write_xfl_shape(
    document: &mut String,
    object: &ObjectInstance,
    points: &mut [(f32, f32)],
    report: &mut CompatibilityReport,
) {
    let transformed: Vec<(f32, f32)> = points
        .iter()
        .map(|(x, y)| {
            let sx = x * object.transform.scale_x;
            let sy = y * object.transform.scale_y;
            let rad = object.transform.rotation.to_radians();
            (
                object.transform.x + sx * rad.cos() - sy * rad.sin(),
                object.transform.y + sx * rad.sin() + sy * rad.cos(),
            )
        })
        .collect();
    if transformed.len() < 2 {
        return;
    }
    document.push_str("                <DOMShape>\n                  <fills>\n");
    document.push_str(&format!(
        "                    <FillStyle index=\"1\"><SolidColor color=\"{}\"/></FillStyle>\n",
        color_hex(object.fill)
    ));
    document.push_str("                  </fills>\n                  <edges>\n");
    document.push_str(&format!(
        "                    <Edge fillStyle0=\"1\" edges=\"{}\"/>\n",
        edge_string(&transformed)
    ));
    document.push_str("                  </edges>\n                </DOMShape>\n");
    report.info(
        IssueCategory::Export,
        format!(
            "Objekt '{}' wurde als XFL-DOMShape angenähert.",
            object.name
        ),
    );
}

fn rectangle_points(width: f32, height: f32) -> Vec<(f32, f32)> {
    let (hw, hh) = (width / 2.0, height / 2.0);
    vec![(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh), (-hw, -hh)]
}

fn ellipse_points(width: f32, height: f32) -> Vec<(f32, f32)> {
    let (rx, ry) = (width / 2.0, height / 2.0);
    (0..=32)
        .map(|index| {
            let angle = std::f32::consts::TAU * index as f32 / 32.0;
            (rx * angle.cos(), ry * angle.sin())
        })
        .collect()
}

fn flatten_path(path: &PathShape) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    let mut current = (0.0, 0.0);
    for command in &path.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                current = (x, y);
                out.push(current);
            }
            PathCommand::LineTo { x, y } => {
                current = (x, y);
                out.push(current);
            }
            PathCommand::CurveTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                let start = current;
                for step in 1..=12 {
                    let t = step as f32 / 12.0;
                    let mt = 1.0 - t;
                    let px = mt * mt * start.0 + 2.0 * mt * t * control_x + t * t * x;
                    let py = mt * mt * start.1 + 2.0 * mt * t * control_y + t * t * y;
                    out.push((px, py));
                }
                current = (x, y);
            }
        }
    }
    out
}

fn edge_string(points: &[(f32, f32)]) -> String {
    let mut out = String::new();
    for (index, (x, y)) in points.iter().enumerate() {
        if index == 0 {
            out.push('!');
        } else {
            out.push('|');
        }
        out.push_str(&format!(
            "{} {}",
            (x * 20.0).round() as i32,
            (y * 20.0).round() as i32
        ));
    }
    out
}

fn color_hex(color: Rgba) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn fixture() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer.start_file("DOMDocument.xml", options).unwrap();
            writer
                .write_all(
                    br##"<?xml version="1.0" encoding="utf-8"?>
<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="640" height="360" frameRate="24" backgroundColor="#102030">
  <timelines>
    <DOMTimeline name="Scene 1">
      <layers>
        <DOMLayer name="Layer 1" current="true" isSelected="true">
          <frames>
            <DOMFrame index="0" keyMode="9728">
              <elements>
                <DOMRectangleObject name="Box" x="100" y="80" width="120" height="60" fillColor="#FF8800"/>
              </elements>
            </DOMFrame>
          </frames>
        </DOMLayer>
      </layers>
    </DOMTimeline>
  </timelines>
</DOMDocument>"##,
                )
                .unwrap();
            writer.start_file("scripts/Main.as", options).unwrap();
            writer
                .write_all(b"package { public class Main {} }")
                .unwrap();
            writer.start_file("bin/asset.bin", options).unwrap();
            writer.write_all(b"asset").unwrap();
            writer.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn imports_minimal_xfl_fixture() {
        let imported = import(&fixture()).unwrap();
        assert_eq!(imported.project.scenes.len(), 1);
        assert_eq!(imported.project.scenes[0].width, 640.0);
        assert_eq!(imported.project.scenes[0].height, 360.0);
        assert_eq!(imported.project.scenes[0].layers.len(), 1);
        assert_eq!(
            imported.project.scenes[0].layers[0].frames[0].objects.len(),
            1
        );
        assert_eq!(imported.project.scripts.len(), 1);
        assert!(imported.report.stats.contains_key("Formen/Elemente"));
    }

    #[test]
    fn rejects_zip_without_domdocument() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            writer
                .start_file("other.txt", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        let error = import(&cursor.into_inner()).unwrap_err();
        assert!(matches!(error, FormatError::Fla(_)));
    }

    #[test]
    fn rejects_non_zip_fla_bytes() {
        let error = import(b"not a zip container").unwrap_err();
        assert!(matches!(error, FormatError::Zip(_) | FormatError::Fla(_)));
    }

    #[test]
    fn imports_nested_document_shape_symbol_and_unsupported_element() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer
                .start_file("project/DOMDocument.xml", options)
                .unwrap();
            writer
                .write_all(
                    br##"<?xml version="1.0" encoding="utf-8"?>
<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="800" height="600" frameRate="30" backgroundColor="#223344">
  <timelines>
    <DOMTimeline name="Nested Scene">
      <layers>
        <DOMLayer name="Artwork">
          <frames>
            <DOMFrame index="0" keyMode="9728">
              <elements>
                <DOMShape name="ImportedShape" x="40" y="50">
                  <fills><FillStyle index="1"><SolidColor color="#44CC88"/></FillStyle></fills>
                  <edges><Edge fillStyle1="1" edges="!0 0|100 0|100 100|0 100|0 0"/></edges>
                  <bounds xMin="0" yMin="0" xMax="120" yMax="90"/>
                </DOMShape>
                <DOMSymbolInstance name="Reused" libraryItemName="Library/Button" x="200" y="120" scaleX="2" scaleY="2" tintColor="#FFAA00"/>
                <DOMVideoInstance name="UnsupportedVideo"/>
              </elements>
            </DOMFrame>
          </frames>
        </DOMLayer>
      </layers>
    </DOMTimeline>
  </timelines>
</DOMDocument>"##,
                )
                .unwrap();
            writer
                .start_file("project/scripts/Nested.as", options)
                .unwrap();
            writer
                .write_all(b"package { public class Nested {} }")
                .unwrap();
            writer.finish().unwrap();
        }
        let imported = import(&cursor.into_inner()).unwrap();
        assert_eq!(imported.project.scenes[0].width, 800.0);
        assert_eq!(imported.project.scenes[0].height, 600.0);
        assert_eq!(imported.project.scenes[0].fps, 30.0);
        let objects = &imported.project.scenes[0].layers[0].frames[0].objects;
        assert_eq!(objects.len(), 2);
        assert!(objects.iter().any(|object| matches!(
            object.kind,
            ObjectKind::Path {
                ref paths,
                ..
            } if !paths.is_empty()
        )));
        assert!(
            objects
                .iter()
                .any(|object| matches!(object.kind, ObjectKind::Symbol { .. }))
        );
        assert_eq!(imported.project.symbols.len(), 1);
        assert!(imported.project.scripts.iter().any(|script| {
            script.path.ends_with("Nested.as") && script.source.contains("class Nested")
        }));
        assert!(imported.report.issues.iter().any(|issue| {
            issue.message.contains("DOMVideoInstance")
                && issue.category == IssueCategory::UnsupportedTag
        }));
        assert_eq!(
            imported.report.stats.get("Skripte").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn exports_and_reimports_xfl_package() {
        let project = Project::demo();
        let export = export_xfl(&project).unwrap();
        let imported = import(&export.bytes).unwrap();
        assert_eq!(imported.project.scenes[0].width, 550.0);
        assert!(imported.project.scenes[0].frame_count() >= 1);
        let mut archive = ZipArchive::new(Cursor::new(export.bytes)).unwrap();
        let mut readme = String::new();
        archive
            .by_name("README.txt")
            .unwrap()
            .read_to_string(&mut readme)
            .unwrap();
        assert!(readme.contains("NOT a native Adobe .fla"));
    }
}
