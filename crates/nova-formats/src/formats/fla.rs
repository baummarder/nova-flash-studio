//! FLA/XFL import and an explicitly experimental XFL compatibility export.
//!
//! Modern `.fla` files are ZIP containers with XFL XML. This module implements
//! a documented subset. It never pretends to support the entire XFL schema and
//! it never writes a `.fla` file: the exporter produces a clearly labelled
//! `.xfl.zip` package whose validity has not been tested in Adobe Animate.

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read, Write};
use std::path::Path;

use nova_core::{
    Asset, AssetKind, Frame, Layer, LayerKind, ObjectInstance, ObjectKind, PathCommand, PathShape,
    Project, Rgba, Scene, Script, SoundCue, SourceFormat, SourceInfo, Symbol, SymbolKind,
    Transform,
};
use roxmltree::{Document, Node};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::capabilities::{CompatibilityReport, FormatKind, IssueCategory};
use crate::conversion::{ExportData, ImportedData};
use crate::error::{FormatError, FormatResult};

/// Hard limits for untrusted archive input (M18 security pass).
const MAX_ZIP_ENTRIES: usize = 4096;
const MAX_ZIP_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ZIP_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_XFL_DIR_FILES: usize = 8192;
const MAX_XFL_DIR_BYTES: u64 = 512 * 1024 * 1024;

/// OLE2 / Compound File Binary signature — the container used by native
/// binary `.fla` files up to Flash CS4.
const OLE2_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Returns true when `name` is a safe relative archive path.
fn is_safe_entry_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains("..")
        && !name.contains('\0')
        && !name.contains(':')
}

pub fn import(bytes: &[u8]) -> FormatResult<ImportedData> {
    // Native binary FLA (pre-CS5, OLE2 compound document) is a proprietary,
    // undocumented format. Detect it explicitly instead of failing inside
    // the ZIP reader with a misleading error.
    if bytes.starts_with(OLE2_MAGIC) {
        return Err(FormatError::Fla(
            "Native binäre FLA (OLE2-Container, Flash bis CS4) wird nicht unterstützt. \
             In Adobe Animate über 'Speichern unter → XFL' konvertieren oder eine \
             CS5+-FLA (ZIP/XFL) verwenden."
                .into(),
        ));
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| FormatError::Zip(error.to_string()))?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(FormatError::Zip(format!(
            "ZIP enthält zu viele Einträge ({} > {MAX_ZIP_ENTRIES}).",
            archive.len()
        )));
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| FormatError::Zip(error.to_string()))?;
        if !file.is_file() {
            continue;
        }
        let name = file
            .name()
            .map_err(|error| FormatError::Zip(error.to_string()))?
            .replace('\\', "/");
        if !is_safe_entry_name(&name) {
            return Err(FormatError::Zip(format!(
                "Unsicherer ZIP-Eintragname: '{name}'"
            )));
        }
        if file.size() > MAX_ZIP_ENTRY_BYTES {
            return Err(FormatError::Zip(format!(
                "ZIP-Eintrag '{name}' ist zu groß ({} Bytes).",
                file.size()
            )));
        }
        total = total.saturating_add(file.size());
        if total > MAX_ZIP_TOTAL_BYTES {
            return Err(FormatError::Zip(
                "ZIP-Inhalt überschreitet das Größenlimit (512 MB).".into(),
            ));
        }
        let mut data = Vec::new();
        file.take(MAX_ZIP_ENTRY_BYTES + 1).read_to_end(&mut data)?;
        if data.len() as u64 > MAX_ZIP_ENTRY_BYTES {
            return Err(FormatError::Zip(format!(
                "ZIP-Eintrag '{name}' überschreitet die Größenbegrenzung."
            )));
        }
        files.insert(name, data);
    }
    let file_count = files.len();
    let mut imported = import_files(files, "XFL/ZIP")?;
    imported
        .report
        .set_stat("Container-Einträge", file_count.to_string());
    Ok(imported)
}

/// Imports an uncompressed XFL folder (loose files + `.xfl` marker or
/// `DOMDocument.xml`). Paths are stored relative to the folder root.
pub fn import_dir(root: &Path) -> FormatResult<ImportedData> {
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    collect_dir_files(root, root, &mut files, &mut total)?;
    import_files(files, "XFL-Ordner")
}

fn collect_dir_files(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
    total: &mut u64,
) -> FormatResult<()> {
    if files.len() > MAX_XFL_DIR_FILES {
        return Err(FormatError::Fla(format!(
            "XFL-Ordner enthält zu viele Dateien (>{MAX_XFL_DIR_FILES})."
        )));
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_files(root, &path, files, total)?;
        } else if path.is_file() {
            let size = entry.metadata()?.len();
            *total = total.saturating_add(size);
            if *total > MAX_XFL_DIR_BYTES {
                return Err(FormatError::Fla(
                    "XFL-Ordner überschreitet das Größenlimit (512 MB).".into(),
                ));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|_| FormatError::Fla("Ungültiger XFL-Pfad.".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, std::fs::read(&path)?);
        }
    }
    Ok(())
}

/// Shared import body for both ZIP containers and loose XFL folders.
fn import_files(
    files: BTreeMap<String, Vec<u8>>,
    container_label: &str,
) -> FormatResult<ImportedData> {
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
    report.source_version = Some(container_label.to_string());
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

    let mut assets: Vec<Asset> = Vec::new();
    let mut asset_ids: HashMap<String, Uuid> = HashMap::new();
    collect_xfl_assets(root, &files, &mut assets, &mut asset_ids, &mut report);

    let mut symbols: Vec<Symbol> = Vec::new();
    let mut symbol_ids: HashMap<String, Uuid> = HashMap::new();
    let mut shape_count = 0usize;
    let mut unsupported_count = 0usize;

    // Pre-create every library symbol before parsing any timeline. Nested
    // instances can then reference symbols that are defined in a later ZIP
    // entry and still resolve to stable ids.
    for (file_name, data) in &files {
        let lower = file_name.to_ascii_lowercase();
        if !lower.starts_with("library/")
            || !lower.ends_with(".xml")
            || lower.ends_with("domdocument.xml")
        {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        let Ok(document) = Document::parse(text) else {
            continue;
        };
        let Some(item) = document
            .descendants()
            .find(|node| node.has_tag_name("DOMSymbolItem"))
        else {
            continue;
        };
        let raw_name = attr_string(item, "name")
            .or_else(|| {
                file_name
                    .rsplit('/')
                    .next()
                    .map(|name| name.trim_end_matches(".xml").to_string())
            })
            .unwrap_or_else(|| "Symbol".into());
        let name = raw_name.rsplit('/').next().unwrap_or(&raw_name).to_string();
        let kind = match attr_string(item, "symbolType")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "movieclip" => SymbolKind::MovieClip,
            "button" => SymbolKind::Button,
            _ => SymbolKind::Graphic,
        };
        let width = attr_f32(item, "width").unwrap_or(100.0).max(1.0);
        let height = attr_f32(item, "height").unwrap_or(100.0).max(1.0);
        let mut symbol = Symbol::empty(name.clone(), kind, width, height);
        symbol.timeline.layers.clear();
        let id = symbol.id;
        symbols.push(symbol);
        symbol_ids.insert(raw_name.clone(), id);
        symbol_ids.insert(name.clone(), id);
    }

    // Now fill each placeholder with its real timeline.
    for (file_name, data) in &files {
        let lower = file_name.to_ascii_lowercase();
        if !lower.starts_with("library/")
            || !lower.ends_with(".xml")
            || lower.ends_with("domdocument.xml")
        {
            continue;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        let Ok(document) = Document::parse(text) else {
            continue;
        };
        let Some(item) = document
            .descendants()
            .find(|node| node.has_tag_name("DOMSymbolItem"))
        else {
            continue;
        };
        let raw_name = attr_string(item, "name")
            .or_else(|| {
                file_name
                    .rsplit('/')
                    .next()
                    .map(|name| name.trim_end_matches(".xml").to_string())
            })
            .unwrap_or_else(|| "Symbol".into());
        let key = raw_name.rsplit('/').next().unwrap_or(&raw_name).to_string();
        let Some(id) = symbol_ids
            .get(&raw_name)
            .or_else(|| symbol_ids.get(&key))
            .copied()
        else {
            continue;
        };
        let Some(index) = symbols.iter().position(|symbol| symbol.id == id) else {
            continue;
        };
        if let Some(timeline) = item
            .descendants()
            .find(|node| node.has_tag_name("DOMTimeline"))
        {
            let layers = parse_timeline_layers(
                timeline,
                &mut symbols,
                &mut symbol_ids,
                &assets,
                &asset_ids,
                &mut report,
                &mut shape_count,
            );
            if !layers.is_empty() {
                symbols[index].timeline.layers = layers;
            }
            symbols[index].timeline.width = symbols[index].width;
            symbols[index].timeline.height = symbols[index].height;
            symbols[index].timeline.background = Rgba::rgba(0, 0, 0, 0);
            symbols[index].timeline.name = symbols[index].name.clone();
            symbols[index].normalize();
        }
    }

    scene.layers = timeline_node
        .map(|timeline| {
            parse_timeline_layers(
                timeline,
                &mut symbols,
                &mut symbol_ids,
                &assets,
                &asset_ids,
                &mut report,
                &mut shape_count,
            )
        })
        .unwrap_or_default();
    if scene.layers.is_empty() {
        report.warning(
            IssueCategory::Timeline,
            "Keine DOMLayer gefunden; lege eine leere Ebene an.",
        );
        scene.layers.push(Layer::new("Ebene 1", 1));
        scene.layers[0].frames[0].keyframe = true;
    }

    let mut project = Project::new_empty("FLA-Import");
    project.metadata.name = "FLA-Import".into();
    project.metadata.description =
        "Aus FLA/XFL importiert. Nicht unterstützte XFL-Elemente bleiben nur in der Originaldatei erhalten."
            .into();
    project.scenes = vec![scene];
    project.active_scene = project.scenes.first().map(|scene| scene.id);
    project.symbols = symbols;
    project.assets = assets;
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

fn find_zip_entry<'a>(files: &'a BTreeMap<String, Vec<u8>>, href: &str) -> Option<&'a Vec<u8>> {
    let normalized = href.trim_start_matches('/').replace('\\', "/");
    files.get(&normalized).or_else(|| {
        files
            .iter()
            .find(|(name, _)| name.replace('\\', "/").ends_with(&normalized))
            .map(|(_, data)| data)
    })
}

fn asset_mime(file: &str, kind: AssetKind) -> &'static str {
    let lower = file.to_ascii_lowercase();
    match kind {
        AssetKind::Bitmap => {
            if lower.ends_with(".png") {
                "image/png"
            } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                "image/jpeg"
            } else if lower.ends_with(".gif") {
                "image/gif"
            } else {
                "application/octet-stream"
            }
        }
        AssetKind::Audio => {
            if lower.ends_with(".wav") {
                "audio/wav"
            } else if lower.ends_with(".mp3") {
                "audio/mpeg"
            } else {
                "application/octet-stream"
            }
        }
        AssetKind::Font => "font/ttf",
    }
}

fn probe_image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((
            u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
            u32::from_be_bytes([data[20], data[21], data[22], data[23]]),
        ));
    }
    if data.len() >= 10 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return Some((
            u16::from_le_bytes([data[6], data[7]]) as u32,
            u16::from_le_bytes([data[8], data[9]]) as u32,
        ));
    }
    if data.len() >= 4 && data.starts_with(&[0xFF, 0xD8]) {
        let mut offset = 2usize;
        while offset + 9 <= data.len() {
            if data[offset] != 0xFF {
                offset += 1;
                continue;
            }
            let marker = data[offset + 1];
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                return Some((
                    u16::from_be_bytes([data[offset + 7], data[offset + 8]]) as u32,
                    u16::from_be_bytes([data[offset + 5], data[offset + 6]]) as u32,
                ));
            }
            let length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
            if length < 2 {
                break;
            }
            offset += 2 + length;
        }
    }
    None
}

fn probe_audio(data: &[u8], file: &str) -> (u32, u16, f32) {
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let mut offset = 12usize;
        let mut channels = 1u16;
        let mut sample_rate = 44_100u32;
        let mut bits = 16u16;
        let mut data_len = 0usize;
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
                channels = u16::from_le_bytes([data[body + 2], data[body + 3]]).max(1);
                sample_rate = u32::from_le_bytes([
                    data[body + 4],
                    data[body + 5],
                    data[body + 6],
                    data[body + 7],
                ])
                .max(1);
                bits = u16::from_le_bytes([data[body + 14], data[body + 15]]).max(8);
            } else if id == b"data" {
                data_len = size;
            }
            offset = body + size + (size & 1);
        }
        let duration =
            data_len as f32 / (sample_rate as f32 * channels as f32 * (bits as f32 / 8.0));
        return (sample_rate, channels, duration);
    }
    if data.len() >= 4 {
        let start = data
            .windows(2)
            .position(|window| window[0] == 0xFF && (window[1] & 0xE0) == 0xE0)
            .unwrap_or(0);
        let mp3 = &data[start..];
        if mp3.len() >= 4 {
            let header = u32::from_be_bytes([mp3[0], mp3[1], mp3[2], mp3[3]]);
            let sample_rate = match (header >> 10) & 0x03 {
                0 => 44_100,
                1 => 48_000,
                2 => 32_000,
                _ => 22_050,
            };
            let channels = if ((header >> 6) & 0x03) == 3 { 1 } else { 2 };
            let duration = if file.to_ascii_lowercase().ends_with(".mp3") {
                let bitrate_index = ((header >> 12) & 0x0F) as usize;
                const MP3_BITRATES: [u32; 16] = [
                    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
                ];
                let bitrate = MP3_BITRATES.get(bitrate_index).copied().unwrap_or(128) * 1000;
                if bitrate > 0 {
                    (mp3.len() as f64 * 8.0 / bitrate as f64) as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            return (sample_rate, channels, duration);
        }
    }
    (0, 0, 0.0)
}

fn parse_timeline_layers(
    timeline: Node<'_, '_>,
    symbols: &mut Vec<Symbol>,
    symbol_ids: &mut HashMap<String, Uuid>,
    assets: &[Asset],
    asset_ids: &HashMap<String, Uuid>,
    report: &mut CompatibilityReport,
    shape_count: &mut usize,
) -> Vec<Layer> {
    let layers_node = timeline
        .children()
        .find(|node| node.has_tag_name("layers"))
        .or_else(|| {
            timeline
                .descendants()
                .find(|node| node.has_tag_name("layers"))
        });
    let Some(layers_node) = layers_node else {
        return Vec::new();
    };
    let mut layers = Vec::new();
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
            "mask" => LayerKind::Mask,
            "sound" => LayerKind::Sound,
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
                        *shape_count += parse_element(
                            element,
                            &mut objects,
                            symbols,
                            symbol_ids,
                            assets,
                            asset_ids,
                            report,
                        );
                    }
                }
                let sound = frame_node
                    .descendants()
                    .find(|node| {
                        node.has_tag_name("SoundItem") || node.has_tag_name("DOMSoundItem")
                    })
                    .or_else(|| {
                        (attr_string(frame_node, "soundName").is_some()).then_some(frame_node)
                    })
                    .and_then(|node| {
                        let sound_name = attr_string(node, "name")
                            .or_else(|| attr_string(node, "libraryItemName"))
                            .or_else(|| attr_string(frame_node, "soundName"))?;
                        let asset = asset_ids.get(&sound_name).or_else(|| {
                            assets
                                .iter()
                                .find(|asset| asset.name == sound_name)
                                .map(|asset| &asset.id)
                        });
                        let Some(asset) = asset.copied() else {
                            report.warning(
                                IssueCategory::Sound,
                                format!("Sound '{sound_name}' ist im XFL-Paket nicht vorhanden."),
                            );
                            return None;
                        };
                        let event = attr_string(node, "event")
                            .map(|value| value == "true")
                            .unwrap_or(false);
                        Some(SoundCue {
                            asset,
                            offset: attr_f32(node, "offset").unwrap_or(0.0),
                            duration: attr_f32(node, "duration").unwrap_or(0.0),
                            event,
                            loop_count: attr_usize(node, "loopCount")
                                .unwrap_or(1)
                                .clamp(1, u16::MAX as usize)
                                as u16,
                        })
                    });
                let tween = attr_string(frame_node, "tweenType").map(|value| {
                    let kind = if value.eq_ignore_ascii_case("shape") {
                        nova_core::TweenKind::Shape
                    } else {
                        nova_core::TweenKind::Motion
                    };
                    nova_core::Tween {
                        kind,
                        ..Default::default()
                    }
                });
                while frames.len() <= index {
                    frames.push(Frame::default());
                }
                frames[index] = Frame {
                    keyframe: key_mode != 0,
                    objects,
                    tween,
                    label,
                    sound,
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
    layers
}

fn collect_xfl_assets(
    root: Node<'_, '_>,
    files: &BTreeMap<String, Vec<u8>>,
    assets: &mut Vec<Asset>,
    asset_ids: &mut HashMap<String, Uuid>,
    report: &mut CompatibilityReport,
) {
    let mut seen: HashMap<String, Uuid> = HashMap::new();
    let mut add = |name: String,
                   file: String,
                   kind: AssetKind,
                   data: Vec<u8>,
                   width: u32,
                   height: u32,
                   assets: &mut Vec<Asset>| {
        if seen.contains_key(&name) || seen.contains_key(&file) {
            return;
        }
        let asset = match kind {
            AssetKind::Bitmap => {
                let mime = asset_mime(&file, kind);
                let (width, height) = if width > 0 && height > 0 {
                    (width, height)
                } else {
                    probe_image_dimensions(&data).unwrap_or((0, 0))
                };
                Asset::bitmap(name.clone(), file.clone(), mime, width, height, data)
            }
            AssetKind::Audio => {
                let (sample_rate, channels, duration) = probe_audio(&data, &file);
                Asset::audio(
                    name.clone(),
                    file.clone(),
                    asset_mime(&file, kind),
                    sample_rate,
                    channels,
                    duration,
                    data,
                )
            }
            AssetKind::Font => Asset::new(name.clone(), kind, file.clone()),
        };
        let id = asset.id;
        assets.push(asset);
        asset_ids.insert(name.clone(), id);
        asset_ids.insert(file.clone(), id);
        if let Some(base) = file.rsplit('/').next() {
            asset_ids.insert(base.to_string(), id);
        }
        seen.insert(name, id);
        seen.insert(file, id);
    };

    for node in root
        .descendants()
        .filter(|node| node.has_tag_name("DOMBitmapItem") || node.has_tag_name("DOMSoundItem"))
    {
        let kind = if node.has_tag_name("DOMSoundItem") {
            AssetKind::Audio
        } else {
            AssetKind::Bitmap
        };
        let name = attr_string(node, "name")
            .or_else(|| attr_string(node, "href"))
            .unwrap_or_else(|| {
                if kind == AssetKind::Audio {
                    "Sound".into()
                } else {
                    "Bitmap".into()
                }
            });
        let href = attr_string(node, "href")
            .or_else(|| attr_string(node, "source"))
            .unwrap_or_else(|| name.clone());
        let data = find_zip_entry(files, &href).cloned().unwrap_or_default();
        if data.is_empty() {
            report.warning(
                IssueCategory::Assets,
                format!("Asset '{name}' ist im XFL-Paket nicht auffindbar ({href})."),
            );
        }
        add(
            name,
            href,
            kind,
            data,
            attr_usize(node, "width").unwrap_or(0) as u32,
            attr_usize(node, "height").unwrap_or(0) as u32,
            assets,
        );
    }

    for (file, data) in files {
        let lower = file.to_ascii_lowercase();
        let kind = if lower.ends_with(".png")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".gif")
        {
            AssetKind::Bitmap
        } else if lower.ends_with(".wav") || lower.ends_with(".mp3") {
            AssetKind::Audio
        } else {
            continue;
        };
        if lower.contains("domdocument") {
            continue;
        }
        let name = file
            .rsplit('/')
            .next()
            .unwrap_or(file)
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_string())
            .unwrap_or_else(|| file.clone());
        add(name, file.clone(), kind, data.clone(), 0, 0, assets);
    }
    report.set_stat("Assets", assets.len().to_string());
}

fn parse_element(
    element: Node<'_, '_>,
    objects: &mut Vec<ObjectInstance>,
    symbols: &mut Vec<Symbol>,
    symbol_ids: &mut HashMap<String, Uuid>,
    assets: &[Asset],
    asset_ids: &HashMap<String, Uuid>,
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
            let symbol_id = if let Some(id) = symbol_ids.get(&library_name).copied().or_else(|| {
                library_name
                    .rsplit('/')
                    .next()
                    .and_then(|name| symbol_ids.get(name))
                    .copied()
            }) {
                id
            } else {
                let base = library_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&library_name)
                    .to_string();
                let symbol = Symbol::new(base, width, height, fill);
                let id = symbol.id;
                symbols.push(symbol);
                symbol_ids.insert(library_name.clone(), id);
                id
            };
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
            let name = attr_string(element, "name").unwrap_or_else(|| "Form".into());
            if let Some(paths) = decode_dom_shape(element) {
                // Decoded edges are absolute coordinates; centre them on
                // the bounds midpoint like other path objects.
                let mut min_x = f32::INFINITY;
                let mut min_y = f32::INFINITY;
                let mut max_x = f32::NEG_INFINITY;
                let mut max_y = f32::NEG_INFINITY;
                for path in &paths {
                    for command in &path.commands {
                        let mut points: Vec<(f32, f32)> = Vec::with_capacity(3);
                        match *command {
                            PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                                points.push((x, y));
                            }
                            PathCommand::CurveTo {
                                control_x,
                                control_y,
                                x,
                                y,
                            } => {
                                points.push((control_x, control_y));
                                points.push((x, y));
                            }
                            PathCommand::CubicTo {
                                c1_x,
                                c1_y,
                                c2_x,
                                c2_y,
                                x,
                                y,
                            } => {
                                points.push((c1_x, c1_y));
                                points.push((c2_x, c2_y));
                                points.push((x, y));
                            }
                        }
                        for (px, py) in points {
                            min_x = min_x.min(px);
                            min_y = min_y.min(py);
                            max_x = max_x.max(px);
                            max_y = max_y.max(py);
                        }
                    }
                }
                let (width, height) = if min_x.is_finite() {
                    ((max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
                } else {
                    shape_bounds(element)
                };
                let (cx, cy) = if min_x.is_finite() {
                    ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
                } else {
                    (0.0, 0.0)
                };
                let paths: Vec<PathShape> = paths
                    .into_iter()
                    .map(|mut path| {
                        for command in &mut path.commands {
                            match command {
                                PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                                    *x -= cx;
                                    *y -= cy;
                                }
                                PathCommand::CurveTo {
                                    control_x,
                                    control_y,
                                    x,
                                    y,
                                } => {
                                    *control_x -= cx;
                                    *control_y -= cy;
                                    *x -= cx;
                                    *y -= cy;
                                }
                                PathCommand::CubicTo {
                                    c1_x,
                                    c1_y,
                                    c2_x,
                                    c2_y,
                                    x,
                                    y,
                                } => {
                                    *c1_x -= cx;
                                    *c1_y -= cy;
                                    *c2_x -= cx;
                                    *c2_y -= cy;
                                    *x -= cx;
                                    *y -= cy;
                                }
                            }
                        }
                        path
                    })
                    .collect();
                let fill = paths
                    .first()
                    .map(|path| path.fill)
                    .unwrap_or_else(|| shape_fill(element));
                let object = ObjectInstance::path(name, cx, cy, width, height, paths, fill);
                objects.push(object);
                return 1;
            }
            let (width, height) = shape_bounds(element);
            let fill = shape_fill(element);
            let paths = vec![rectangle_path(width, height, fill)];
            let mut object = ObjectInstance::path(
                name,
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
                "DOMShape ohne dekodierbare Edge-Daten wird als Rechteck mit den erkannten Grenzen importiert.",
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
                count += parse_element(
                    child, objects, symbols, symbol_ids, assets, asset_ids, report,
                );
            }
            count
        }
        "DOMBitmapInstance" => {
            let library_name = attr_string(element, "libraryItemName")
                .or_else(|| attr_string(element, "name"))
                .or_else(|| attr_string(element, "href"))
                .unwrap_or_else(|| "Bitmap".into());
            let asset_id = asset_ids
                .get(&library_name)
                .or_else(|| {
                    library_name
                        .rsplit('/')
                        .next()
                        .and_then(|name| asset_ids.get(name))
                })
                .copied()
                .or_else(|| {
                    assets
                        .iter()
                        .find(|asset| asset.kind == AssetKind::Bitmap && asset.name == library_name)
                        .map(|asset| asset.id)
                });
            if let Some(asset_id) = asset_id {
                let asset = assets
                    .iter()
                    .find(|asset| asset.id == asset_id)
                    .expect("asset id resolved");
                let width = attr_f32(element, "width")
                    .unwrap_or(asset.width.max(1) as f32)
                    .max(1.0);
                let height = attr_f32(element, "height")
                    .unwrap_or(asset.height.max(1) as f32)
                    .max(1.0);
                let mut object = ObjectInstance::bitmap(
                    attr_string(element, "name").unwrap_or_else(|| asset.name.clone()),
                    attr_f32(element, "x").unwrap_or(0.0),
                    attr_f32(element, "y").unwrap_or(0.0),
                    asset_id,
                    width,
                    height,
                );
                apply_xfl_transform(element, &mut object.transform);
                objects.push(object);
            } else {
                report.warning(
                    IssueCategory::Bitmap,
                    format!(
                        "DOMBitmapInstance '{library_name}' hat kein passendes Asset; als Platzhalter importiert."
                    ),
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
            }
            1
        }
        "DOMText" => {
            // Editable XFL text: <p><span size face color>…</span></p> rows.
            let mut lines: Vec<String> = Vec::new();
            let mut size = 12.0f32;
            let mut bold = false;
            let mut italic = false;
            let mut align = nova_core::TextAlign::Left;
            let mut color = shape_fill(element);
            for span in element.descendants().filter(|n| n.has_tag_name("span")) {
                size = attr_f32(span, "size").unwrap_or(size).max(1.0);
                bold |= attr_string(span, "bold").as_deref() == Some("true")
                    || span.attribute("bold").is_some();
                italic |= attr_string(span, "italic").as_deref() == Some("true")
                    || span.attribute("italic").is_some();
                if let Some(value) =
                    attr_string(span, "color").and_then(|value| parse_color(&value))
                {
                    color = value;
                }
                let p_align = span
                    .ancestors()
                    .find(|n| n.has_tag_name("p"))
                    .and_then(|p| attr_string(p, "align"));
                align = match p_align.as_deref() {
                    Some("center") => nova_core::TextAlign::Center,
                    Some("right") => nova_core::TextAlign::Right,
                    _ => align,
                };
                if let Some(text) = span.text() {
                    lines.push(text.to_string());
                }
            }
            if lines.is_empty() {
                // Fall back to raw text content of the element.
                if let Some(text) = element.text() {
                    lines.push(text.to_string());
                }
            }
            let mut text_obj = nova_core::TextObject::new(lines.join("\n"), size);
            text_obj.bold = bold;
            text_obj.italic = italic;
            text_obj.align = align;
            let mut object = ObjectInstance::text(
                attr_string(element, "name").unwrap_or_else(|| "Text".into()),
                attr_f32(element, "x").unwrap_or(0.0),
                attr_f32(element, "y").unwrap_or(0.0),
                text_obj.text.clone(),
                size,
                color,
            );
            if let ObjectKind::Text(imported_text) = &mut object.kind {
                imported_text.bold = bold;
                imported_text.italic = italic;
                imported_text.align = align;
            }
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

/// Parsed XFL fill: solid colour plus an optional gradient.
struct XflFill {
    color: Rgba,
    gradient: Option<nova_core::Gradient>,
}

/// Index → fill map from a DOMShape `<fills>` block.
fn parse_fill_styles(element: Node<'_, '_>) -> HashMap<usize, XflFill> {
    let mut fills = HashMap::new();
    for style in element
        .descendants()
        .filter(|node| node.has_tag_name("FillStyle"))
    {
        let Some(index) = attr_usize(style, "index") else {
            continue;
        };
        let mut color = Rgba::WHITE;
        let mut gradient = None;
        for child in style.children().filter(Node::is_element) {
            match child.tag_name().name() {
                "SolidColor" => {
                    if let Some(value) =
                        attr_string(child, "color").and_then(|value| parse_color(&value))
                    {
                        color = value;
                    }
                    if let Some(alpha) = attr_f32(child, "alpha") {
                        color.a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
                "LinearGradient" | "RadialGradient" => {
                    let is_linear = child.tag_name().name() == "LinearGradient";
                    let mut stops = Vec::new();
                    for entry in child.children().filter(|n| n.has_tag_name("GradientEntry")) {
                        let ratio = attr_f32(entry, "ratio").unwrap_or(0.0).clamp(0.0, 1.0);
                        let entry_color = attr_string(entry, "color")
                            .and_then(|value| parse_color(&value))
                            .unwrap_or(color);
                        stops.push(nova_core::GradientStop {
                            position: ratio,
                            color: entry_color,
                        });
                    }
                    if stops.len() >= 2 {
                        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
                        gradient = Some(nova_core::Gradient {
                            kind: if is_linear {
                                nova_core::GradientKind::Linear
                            } else {
                                nova_core::GradientKind::Radial
                            },
                            stops,
                            angle: gradient_angle(child),
                            center_x: attr_f32(child, "centerX").unwrap_or(0.0),
                            center_y: attr_f32(child, "centerY").unwrap_or(0.0),
                            radius: attr_f32(child, "radius")
                                .or_else(|| attr_f32(child, "r"))
                                .unwrap_or(50.0),
                        });
                    }
                }
                _ => {}
            }
        }
        fills.insert(index, XflFill { color, gradient });
    }
    fills
}

/// Linear gradient angle in degrees derived from start/end point
/// attributes (`x1,y1,x2,y2` or `startX` style names).
fn gradient_angle(node: Node<'_, '_>) -> f32 {
    let (x1, y1) = (
        attr_f32(node, "x1").or_else(|| attr_f32(node, "startX")),
        attr_f32(node, "y1").or_else(|| attr_f32(node, "startY")),
    );
    let (x2, y2) = (
        attr_f32(node, "x2").or_else(|| attr_f32(node, "endX")),
        attr_f32(node, "y2").or_else(|| attr_f32(node, "endY")),
    );
    match (x1, y1, x2, y2) {
        (Some(x1), Some(y1), Some(x2), Some(y2)) => {
            let dy = y2 - y1;
            let dx = x2 - x1;
            if dx.abs() + dy.abs() < 1e-6 {
                0.0
            } else {
                dy.atan2(dx).to_degrees()
            }
        }
        _ => 0.0,
    }
}

/// Parses one XFL `<Edge edges="...">` attribute into path commands.
///
/// Supported encodings:
/// - compact point lists: `!x y|x y|...` (twips, `!` = new sub-path)
/// - SVG-style commands: `M/L/C/Q/Z` with twip coordinates
///
/// Anything else returns an empty command list (caller falls back).
fn parse_edge_commands(edges: &str) -> Vec<PathCommand> {
    let trimmed = edges.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    // Compact `!`/`|` format first.
    if trimmed.contains('!') || trimmed.contains('|') {
        let mut commands = Vec::new();
        let mut pen = (0.0f32, 0.0f32);
        for part in trimmed.split(['!', '|']) {
            if part.trim().is_empty() {
                continue;
            }
            let mut nums = part
                .split_whitespace()
                .filter_map(|v| v.parse::<f32>().ok());
            let (Some(x), Some(y)) = (nums.next(), nums.next()) else {
                continue;
            };
            // Coordinates are twips in XFL; convert to pixels.
            let (x, y) = (x / 20.0, y / 20.0);
            if part.starts_with('!') || commands.is_empty() {
                commands.push(PathCommand::MoveTo { x, y });
            } else {
                commands.push(PathCommand::LineTo { x, y });
            }
            pen = (x, y);
        }
        let _ = pen;
        return commands;
    }
    // SVG-style command format: letters M/L/C/Q/Z followed by twip
    // coordinate pairs. Tokenize into (command, Vec<f32>) groups.
    let mut commands = Vec::new();
    let mut subpath_start: Option<(f32, f32)> = None;
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    let mut cmd = 'M';
    let read_number = |i: &mut usize| -> Option<f32> {
        while *i < bytes.len() && (bytes[*i] as char).is_whitespace()
            || *i < bytes.len() && bytes[*i] == b','
        {
            *i += 1;
        }
        let start = *i;
        if *i < bytes.len() && (bytes[*i] == b'-' || bytes[*i] == b'+') {
            *i += 1;
        }
        let mut seen_digit = false;
        while *i < bytes.len() {
            let c = bytes[*i];
            if c.is_ascii_digit() {
                seen_digit = true;
                *i += 1;
            } else if c == b'.' {
                *i += 1;
            } else {
                break;
            }
        }
        if !seen_digit || *i == start {
            return None;
        }
        trimmed[start..*i].parse::<f32>().ok()
    };
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() {
            cmd = c.to_ascii_uppercase();
            i += 1;
            continue;
        }
        if !(c == '-' || c == '+' || c == '.' || c.is_ascii_digit()) {
            i += 1;
            continue;
        }
        // One command group: read its numeric arity.
        let arity = match cmd {
            'M' | 'L' => 2,
            'Q' => 4,
            'C' => 6,
            'Z' => 0,
            _ => 2,
        };
        if arity == 0 {
            if let Some(start) = subpath_start.take() {
                commands.push(PathCommand::LineTo {
                    x: start.0,
                    y: start.1,
                });
            }
            i += 1;
            continue;
        }
        let mut vals = Vec::with_capacity(arity);
        let mut ok = true;
        for _ in 0..arity {
            match read_number(&mut i) {
                Some(v) => vals.push(v),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            break;
        }
        let px = |v: f32| v / 20.0;
        match cmd {
            'M' => {
                let (x, y) = (px(vals[0]), px(vals[1]));
                commands.push(PathCommand::MoveTo { x, y });
                subpath_start = Some((x, y));
                cmd = 'L'; // implicit lineto pairs after moveto
            }
            'L' => {
                commands.push(PathCommand::LineTo {
                    x: px(vals[0]),
                    y: px(vals[1]),
                });
            }
            'Q' => {
                commands.push(PathCommand::CurveTo {
                    control_x: px(vals[0]),
                    control_y: px(vals[1]),
                    x: px(vals[2]),
                    y: px(vals[3]),
                });
            }
            'C' => {
                commands.push(PathCommand::CubicTo {
                    c1_x: px(vals[0]),
                    c1_y: px(vals[1]),
                    c2_x: px(vals[2]),
                    c2_y: px(vals[3]),
                    x: px(vals[4]),
                    y: px(vals[5]),
                });
            }
            _ => {}
        }
    }
    commands
}
/// Decodes a DOMShape element into path shapes using `<Edge>` strings
/// and `<FillStyle>` colours/gradients. Returns `None` when no edge data
/// is present (caller keeps the bounding-box fallback).
fn decode_dom_shape(element: Node<'_, '_>) -> Option<Vec<PathShape>> {
    let fills = parse_fill_styles(element);
    let mut shapes = Vec::new();
    let mut any_edge = false;
    for edge in element
        .descendants()
        .filter(|node| node.has_tag_name("Edge"))
    {
        let Some(edges_attr) = attr_string(edge, "edges") else {
            continue;
        };
        let commands = parse_edge_commands(&edges_attr);
        if commands.len() < 2 {
            continue;
        }
        any_edge = true;
        let fill_index = attr_usize(edge, "fillStyle0").or_else(|| attr_usize(edge, "fillStyle1"));
        let fill = fill_index
            .and_then(|index| fills.get(&index))
            .cloned()
            .unwrap_or(XflFill {
                color: Rgba::rgb(120, 180, 240),
                gradient: None,
            });
        let mut path = PathShape::new(fill.color, commands);
        path.gradient = fill.gradient;
        shapes.push(path);
    }
    if any_edge { Some(shapes) } else { None }
}

// XflFill needs Clone for the unwrap_or path.
impl Clone for XflFill {
    fn clone(&self) -> Self {
        Self {
            color: self.color,
            gradient: self.gradient.clone(),
        }
    }
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

/// A tilemap baked to a PNG for XFL export (keyed by object id).
struct BakedTilemap {
    /// `DOMBitmapItem`/`libraryItemName` used by the instance.
    lib_name: String,
    /// ZIP path of the PNG inside the package.
    file_name: String,
    width: u32,
    height: u32,
    png: Vec<u8>,
}

/// Bakes every `ObjectKind::Tilemap` reachable from the active scene and all
/// symbol timelines into PNG library items. Failures degrade to `None` so a
/// single broken tileset cannot abort the whole export.
fn bake_tilemaps(project: &Project) -> HashMap<Uuid, BakedTilemap> {
    let mut baked = HashMap::new();
    let mut warnings = Vec::new();
    let mut collect = |object: &ObjectInstance, baked: &mut HashMap<Uuid, BakedTilemap>| {
        let ObjectKind::Tilemap(map) = &object.kind else {
            return;
        };
        let Ok(Some((w, h, rgba))) = nova_build::bake_tilemap(project, map, &mut warnings) else {
            return;
        };
        let Ok(png) = rgba_to_png(w, h, rgba) else {
            return;
        };
        let lib_name = format!(
            "Tilemap_{}_{}",
            sanitize_file_component(&object.name),
            &object.id.to_string()[..8]
        );
        baked.insert(
            object.id,
            BakedTilemap {
                file_name: format!("LIBRARY/{lib_name}.png"),
                lib_name,
                width: w,
                height: h,
                png,
            },
        );
    };
    if let Some(scene) = project.active_scene() {
        for layer in &scene.layers {
            for frame in &layer.frames {
                for object in &frame.objects {
                    collect(object, &mut baked);
                }
            }
        }
    }
    for symbol in &project.symbols {
        for layer in &symbol.timeline.layers {
            for frame in &layer.frames {
                for object in &frame.objects {
                    collect(object, &mut baked);
                }
            }
        }
    }
    baked
}

/// Encodes an RGBA buffer as PNG bytes.
fn rgba_to_png(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, FormatError> {
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| FormatError::Unsupported("Kachelkarte: ungültige Pixeldaten".into()))?;
    let mut buffer = Cursor::new(Vec::new());
    image
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|error| FormatError::Unsupported(format!("Kachelkarte PNG: {error}")))?;
    Ok(buffer.into_inner())
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
    let baked_tilemaps = bake_tilemaps(project);
    if !baked_tilemaps.is_empty() {
        report.info(
            IssueCategory::Export,
            format!(
                "{} Kachelkarte(n) wurden als Bitmap in die XFL-Bibliothek gebacken.",
                baked_tilemaps.len()
            ),
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
        let layer_type = match layer.kind {
            LayerKind::Folder => " layerType=\"folder\"",
            LayerKind::Guide => " layerType=\"guide\"",
            LayerKind::Mask => " layerType=\"mask\"",
            LayerKind::Sound => " layerType=\"sound\"",
            LayerKind::Normal => "",
        };
        document.push_str(&format!(
            "        <DOMLayer name=\"{}\" color=\"#4FFF4F\" current=\"{}\" isSelected=\"{}\" visible=\"{}\"{}>\n          <frames>\n",
            xml_escape(&layer.name),
            if layer_index == 0 { "true" } else { "false" },
            if layer_index == 0 { "true" } else { "false" },
            if layer.visible { "true" } else { "false" },
            layer_type
        ));
        for (frame_index, frame) in layer.frames.iter().enumerate() {
            let tween = frame
                .tween
                .as_ref()
                .map(|tween| {
                    format!(
                        " tweenType=\"{}\"",
                        match tween.kind {
                            nova_core::TweenKind::Shape => "shape",
                            nova_core::TweenKind::Motion => "motion",
                        }
                    )
                })
                .unwrap_or_default();
            document.push_str(&format!(
                "            <DOMFrame index=\"{}\" keyMode=\"{}\"{}{}>\n",
                frame_index,
                if frame.keyframe { 9728 } else { 0 },
                frame
                    .label
                    .as_ref()
                    .map(|label| format!(" label=\"{}\"", xml_escape(label)))
                    .unwrap_or_default(),
                tween
            ));
            if !frame.objects.is_empty() {
                document.push_str("              <elements>\n");
                for object in &frame.objects {
                    write_xfl_object(&mut document, object, project, &baked_tilemaps, &mut report);
                }
                document.push_str("              </elements>\n");
            }
            write_xfl_sound(&mut document, frame.sound, project, &mut report);
            document.push_str("            </DOMFrame>\n");
        }
        document.push_str("          </frames>\n        </DOMLayer>\n");
    }
    document.push_str("      </layers>\n    </DOMTimeline>\n  </timelines>\n");

    if !project.symbols.is_empty() || !project.assets.is_empty() || !baked_tilemaps.is_empty() {
        document.push_str("  <library>\n");
        for asset in &project.assets {
            let href = library_asset_path(asset);
            match asset.kind {
                AssetKind::Bitmap => {
                    document.push_str(&format!(
                        "    <DOMBitmapItem name=\"{}\" href=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                        xml_escape(&asset.name),
                        xml_escape(&href),
                        asset.width,
                        asset.height
                    ));
                }
                AssetKind::Audio => {
                    document.push_str(&format!(
                        "    <DOMSoundItem name=\"{}\" href=\"{}\" duration=\"{}\"/>\n",
                        xml_escape(&asset.name),
                        xml_escape(&href),
                        asset.duration
                    ));
                }
                AssetKind::Font => {}
            }
        }
        for baked in baked_tilemaps.values() {
            document.push_str(&format!(
                "    <DOMBitmapItem name=\"{}\" href=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                xml_escape(&baked.lib_name),
                xml_escape(&baked.file_name),
                baked.width,
                baked.height
            ));
        }
        document.push_str("  </library>\n");
    }

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
        for symbol in &project.symbols {
            let file_name = format!("LIBRARY/{}.xml", sanitize_file_component(&symbol.name));
            let xml = write_symbol_document(symbol, project, &baked_tilemaps, &mut report);
            writer
                .start_file(file_name, options)
                .map_err(|error| FormatError::Zip(error.to_string()))?;
            writer.write_all(xml.as_bytes())?;
        }
        for asset in &project.assets {
            if asset.data.is_empty() {
                report.warning(
                    IssueCategory::Assets,
                    format!(
                        "Asset '{}' ist nicht eingebettet und fehlt daher im XFL-Paket.",
                        asset.name
                    ),
                );
                continue;
            }
            writer
                .start_file(library_asset_path(asset), options)
                .map_err(|error| FormatError::Zip(error.to_string()))?;
            writer.write_all(&asset.data)?;
        }
        for baked in baked_tilemaps.values() {
            writer
                .start_file(&baked.file_name, options)
                .map_err(|error| FormatError::Zip(error.to_string()))?;
            writer.write_all(&baked.png)?;
        }
        writer
            .start_file("README.txt", options)
            .map_err(|error| FormatError::Zip(error.to_string()))?;
        writer.write_all(
            "NOVA Flash Studio XFL compatibility package\n\
             ==========================================\n\
             This ZIP is NOT a native Adobe .fla file and has not been tested in\n\
             Adobe Animate or Flash. It contains DOMDocument.xml, symbol\n\
             timelines under LIBRARY/, embedded bitmap/audio files and\n\
             ActionScript sources. Shapes are approximated from the NØVA model\n\
             and the sound/mask encoding is experimental. Keep the original\n\
             project and test the package before relying on it.\n"
                .as_bytes(),
        )?;
        writer
            .finish()
            .map_err(|error| FormatError::Zip(error.to_string()))?;
    }

    report.set_stat("Ebenen", scene.layers.len().to_string());
    report.set_stat("Frames", scene.frame_count().to_string());
    report.set_stat("Skripte", exportable_scripts.len().to_string());
    report.set_stat("Symbole", project.symbols.len().to_string());
    report.set_stat("Assets", project.assets.len().to_string());
    Ok(ExportData {
        bytes: cursor.into_inner(),
        report,
    })
}

fn write_symbol_document(
    symbol: &Symbol,
    project: &Project,
    baked_tilemaps: &HashMap<Uuid, BakedTilemap>,
    report: &mut CompatibilityReport,
) -> String {
    let mut document = String::new();
    document.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    document.push_str(&format!(
        "<DOMSymbolItem xmlns=\"http://ns.adobe.com/xfl/2008/\" name=\"{}\" symbolType=\"{}\" width=\"{}\" height=\"{}\">\n",
        xml_escape(&symbol.name),
        match symbol.kind {
            SymbolKind::Graphic => "graphic",
            SymbolKind::MovieClip => "movieclip",
            SymbolKind::Button => "button",
        },
        symbol.width,
        symbol.height
    ));
    document.push_str("  <timeline>\n");
    document.push_str(&format!(
        "    <DOMTimeline name=\"{}\">\n      <layers>\n",
        xml_escape(&symbol.timeline.name)
    ));
    for layer in &symbol.timeline.layers {
        let layer_type = match layer.kind {
            LayerKind::Folder => " layerType=\"folder\"",
            LayerKind::Guide => " layerType=\"guide\"",
            LayerKind::Mask => " layerType=\"mask\"",
            LayerKind::Sound => " layerType=\"sound\"",
            LayerKind::Normal => "",
        };
        document.push_str(&format!(
            "        <DOMLayer name=\"{}\" visible=\"{}\"{}>\n          <frames>\n",
            xml_escape(&layer.name),
            if layer.visible { "true" } else { "false" },
            layer_type
        ));
        for (frame_index, frame) in layer.frames.iter().enumerate() {
            let tween = frame
                .tween
                .as_ref()
                .map(|tween| {
                    format!(
                        " tweenType=\"{}\"",
                        match tween.kind {
                            nova_core::TweenKind::Shape => "shape",
                            nova_core::TweenKind::Motion => "motion",
                        }
                    )
                })
                .unwrap_or_default();
            document.push_str(&format!(
                "            <DOMFrame index=\"{}\" keyMode=\"{}\"{}{}>\n",
                frame_index,
                if frame.keyframe { 9728 } else { 0 },
                frame
                    .label
                    .as_ref()
                    .map(|label| format!(" label=\"{}\"", xml_escape(label)))
                    .unwrap_or_default(),
                tween
            ));
            if !frame.objects.is_empty() {
                document.push_str("              <elements>\n");
                for object in &frame.objects {
                    write_xfl_object(&mut document, object, project, baked_tilemaps, report);
                }
                document.push_str("              </elements>\n");
            }
            write_xfl_sound(&mut document, frame.sound, project, report);
            document.push_str("            </DOMFrame>\n");
        }
        document.push_str("          </frames>\n        </DOMLayer>\n");
    }
    document.push_str("      </layers>\n    </DOMTimeline>\n  </timeline>\n");
    document.push_str("</DOMSymbolItem>\n");
    report.set_stat("Symbole", project.symbols.len().to_string());
    document
}

fn write_xfl_sound(
    document: &mut String,
    cue: Option<SoundCue>,
    project: &Project,
    report: &mut CompatibilityReport,
) {
    let Some(cue) = cue else {
        return;
    };
    let Some(asset) = project.asset(cue.asset) else {
        report.warning(
            IssueCategory::Sound,
            "Ein Sound-Cue verweist auf ein unbekanntes Asset und wurde ausgelassen.",
        );
        return;
    };
    if !cue.event {
        report.warning(
            IssueCategory::Sound,
            format!(
                "Timeline-Sound '{}' wird als experimenteller XFL-Sound-Cue exportiert; Stream-Synchronität ist nicht garantiert.",
                asset.name
            ),
        );
    }
    document.push_str(&format!(
        "              <sound event=\"{}\" offset=\"{}\" duration=\"{}\" loopCount=\"{}\">\n                <SoundItem name=\"{}\" libraryItemName=\"{}\"/>\n              </sound>\n",
        cue.event,
        cue.offset,
        cue.duration,
        cue.loop_count,
        xml_escape(&asset.name),
        xml_escape(&asset.name)
    ));
}

fn sanitize_file_component(value: &str) -> String {
    let mut out: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("asset");
    }
    out
}

fn library_asset_path(asset: &Asset) -> String {
    let extension = asset
        .file
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .or_else(|| {
            Some(match asset.mime.as_str() {
                "image/png" => "png".to_string(),
                "image/jpeg" => "jpg".to_string(),
                "image/gif" => "gif".to_string(),
                "audio/wav" => "wav".to_string(),
                "audio/mpeg" => "mp3".to_string(),
                _ => "bin".to_string(),
            })
        })
        .unwrap_or_else(|| "bin".to_string());
    format!(
        "LIBRARY/{}-{}.{}",
        sanitize_file_component(&asset.name),
        &asset.id.to_string()[..8],
        sanitize_file_component(&extension)
    )
}

fn write_xfl_object(
    document: &mut String,
    object: &ObjectInstance,
    project: &Project,
    baked_tilemaps: &HashMap<Uuid, BakedTilemap>,
    report: &mut CompatibilityReport,
) {
    // Game/particle metadata has no XFL representation; warn once per object
    // instead of silently dropping the component data.
    if object.particles.is_some() {
        report.warning(
            IssueCategory::Export,
            format!(
                "Objekt '{}': Partikelemitter wird im XFL-Paket nicht exportiert; die Daten bleiben im .nova-Projekt erhalten.",
                object.name
            ),
        );
    }
    if let Some(game) = &object.game {
        if game.collision.is_some() || game.physics.is_some() {
            report.warning(
                IssueCategory::Export,
                format!(
                    "Objekt '{}': Game-Metadaten (Kollision/Physik) werden im XFL-Paket nicht exportiert.",
                    object.name
                ),
            );
        }
    }
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
            // Real editable XFL text (DOMText) instead of baked outlines.
            let (tw, th) = text.metrics();
            document.push_str(&format!(
                "                <DOMText name=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">\n                  <matrix><Matrix a=\"1\" d=\"1\" tx=\"{}\" ty=\"{}\"/></matrix>\n                  <text>\n                    <matrix><Matrix a=\"1\" d=\"1\" tx=\"0\" ty=\"0\"/></matrix>\n                    <div>\n",
                xml_escape(&object.name),
                object.transform.x,
                object.transform.y,
                tw,
                th,
                object.transform.x,
                object.transform.y,
            ));
            for line in text.text.split('\n') {
                document.push_str(&format!(
                    "                      <p><span size=\"{}\" face=\"{}\" color=\"{}\"{}{}{}>{}</span></p>\n",
                    text.size,
                    match text.font {
                        nova_core::TextFont::NoveSans => "Nove Sans",
                        nova_core::TextFont::NoveMono => "Nove Mono",
                    },
                    color_hex(object.fill),
                    if text.bold { " bold=\"true\"" } else { "" },
                    if text.italic { " italic=\"true\"" } else { "" },
                    match text.align {
                        nova_core::TextAlign::Center => " align=\"center\"",
                        nova_core::TextAlign::Right => " align=\"right\"",
                        _ => "",
                    },
                    xml_escape(line),
                ));
            }
            document.push_str("                    </div>\n                  </text>\n                </DOMText>\n");
        }
        ObjectKind::Path { paths, .. } => {
            document.push_str("                <DOMShape>\n                  <fills>\n");
            for (index, path) in paths.iter().enumerate() {
                write_xfl_fill_style(document, index + 1, path);
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
        ObjectKind::Bitmap {
            asset,
            width,
            height,
            ..
        } => {
            let asset = project.asset(*asset);
            let name = asset
                .map(|asset| asset.name.clone())
                .unwrap_or_else(|| "Bitmap".into());
            document.push_str(&format!(
                "                <DOMBitmapInstance libraryItemName=\"{}\" name=\"{}\" x=\"{}\" y=\"{}\" scaleX=\"{}\" scaleY=\"{}\" rotation=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                xml_escape(&name),
                xml_escape(&object.name),
                object.transform.x,
                object.transform.y,
                object.transform.scale_x,
                object.transform.scale_y,
                object.transform.rotation,
                width,
                height
            ));
            report.info(
                IssueCategory::Export,
                format!("Bitmap '{}' wurde als DOMBitmapInstance exportiert.", name),
            );
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
            report.info(
                IssueCategory::Export,
                format!(
                    "Symbol '{}' wurde mit Name, Transform und Bibliotheksverweis exportiert.",
                    symbol_name
                ),
            );
        }
        ObjectKind::Tilemap(map) => {
            match baked_tilemaps.get(&object.id) {
                Some(baked) => {
                    // The tilemap was baked to a PNG library bitmap; the
                    // instance mirrors the authored transform. Tile collision
                    // and layer data remain editor metadata.
                    document.push_str(&format!(
                        "                <DOMBitmapInstance libraryItemName=\"{}\" name=\"{}\" x=\"{}\" y=\"{}\" scaleX=\"{}\" scaleY=\"{}\" rotation=\"{}\" width=\"{}\" height=\"{}\"/>\n",
                        xml_escape(&baked.lib_name),
                        xml_escape(&object.name),
                        object.transform.x,
                        object.transform.y,
                        object.transform.scale_x,
                        object.transform.scale_y,
                        object.transform.rotation,
                        baked.width,
                        baked.height
                    ));
                    report.info(
                        IssueCategory::Export,
                        format!(
                            "Kachelkarte '{}' wurde als Bitmap '{}' in die XFL-Bibliothek gebacken.",
                            object.name, baked.lib_name
                        ),
                    );
                }
                None => {
                    report.warning(
                        IssueCategory::Export,
                        format!(
                            "Kachelkarte '{}' ({}×{} Kacheln) konnte nicht als Bitmap gebacken werden und fehlt im XFL-Paket; die Daten bleiben im .nova-Projekt erhalten.",
                            object.name, map.columns, map.rows
                        ),
                    );
                }
            }
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
            PathCommand::CubicTo {
                c1_x,
                c1_y,
                c2_x,
                c2_y,
                x,
                y,
            } => {
                let start = current;
                for step in 1..=16 {
                    let t = step as f32 / 16.0;
                    let mt = 1.0 - t;
                    let px = mt * mt * mt * start.0
                        + 3.0 * mt * mt * t * c1_x
                        + 3.0 * mt * t * t * c2_x
                        + t * t * t * x;
                    let py = mt * mt * mt * start.1
                        + 3.0 * mt * mt * t * c1_y
                        + 3.0 * mt * t * t * c2_y
                        + t * t * t * y;
                    out.push((px, py));
                }
                current = (x, y);
            }
        }
    }
    out
}

/// Emits one `<FillStyle>` for a path: solid colour or a linear/radial
/// gradient with `<GradientEntry>` stops (XFL subset).
fn write_xfl_fill_style(document: &mut String, index: usize, path: &PathShape) {
    match &path.gradient {
        Some(gradient) => {
            let tag = match gradient.kind {
                nova_core::GradientKind::Linear => "LinearGradient",
                nova_core::GradientKind::Radial => "RadialGradient",
            };
            let geometry = match gradient.kind {
                nova_core::GradientKind::Linear => {
                    let rad = gradient.angle.to_radians();
                    let (dx, dy) = (rad.cos() * 50.0, rad.sin() * 50.0);
                    format!(" startPoint=\"{} {}\" endPoint=\"{} {}\"", -dx, -dy, dx, dy)
                }
                nova_core::GradientKind::Radial => {
                    format!(
                        " centerPoint=\"{} {}\" radius=\"{}\"",
                        gradient.center_x, gradient.center_y, gradient.radius
                    )
                }
            };
            document.push_str(&format!(
                "                    <FillStyle index=\"{index}\"><{tag}{geometry} spreadMethod=\"pad\">\n"
            ));
            for stop in &gradient.stops {
                document.push_str(&format!(
                    "                      <GradientEntry ratio=\"{}\" color=\"{}\"/>\n",
                    stop.position,
                    color_hex(stop.color)
                ));
            }
            document.push_str(&format!("                    </{tag}></FillStyle>\n"));
        }
        None => {
            document.push_str(&format!(
                "                    <FillStyle index=\"{index}\"><SolidColor color=\"{}\"/></FillStyle>\n",
                color_hex(path.fill)
            ));
        }
    }
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

    /// XFL -> NØVA -> XFL -> NØVA structural roundtrip for the M9 model:
    /// movieclip symbols with nested instances, bitmap/audio assets, mask and
    /// guide layers must survive two export/import cycles.
    #[test]
    fn m9_structural_roundtrip_preserves_symbols_assets_and_layers() {
        use nova_core::{
            Asset, Layer, LayerKind, ObjectInstance, Scene, SoundCue, Symbol, SymbolKind,
        };

        let mut project = Project::demo();
        // Add a movieclip symbol whose timeline instances a graphic symbol.
        let graphic = Symbol::with_kind("Radial", SymbolKind::Graphic, 40.0, 40.0, Rgba::WHITE);
        let mut clip = Symbol::with_kind(
            "Clip",
            SymbolKind::MovieClip,
            80.0,
            80.0,
            Rgba::rgba(0, 0, 0, 0),
        );
        clip.timeline.ensure_frame_count(3);
        let graphic_id = graphic.id;
        let clip_id = clip.id;
        let nested = ObjectInstance::symbol("Verschachtelt", 0.0, 0.0, graphic_id, Rgba::WHITE);
        clip.timeline.layers[0].frames[0].objects.push(nested);
        clip.timeline.layers[0].frames[2].keyframe = true;
        project.symbols.push(graphic);
        project.symbols.push(clip);

        // Embedded assets.
        let png = tiny_png_for_xfl();
        let bitmap = Asset::bitmap("Raster", "assets/raster.png", "image/png", 1, 1, png);
        let bitmap_id = bitmap.id;
        project.assets.push(bitmap);
        let wav = tiny_wav_for_xfl();
        let audio = Asset::audio("Ton", "assets/tone.wav", "audio/wav", 8000, 1, 0.1, wav);
        let audio_id = audio.id;
        project.assets.push(audio);

        // Bitmap object, symbol instance, sound layer and mask/guide layers
        // on the active scene.
        let scene = project.active_scene_mut().unwrap();
        scene.layers.push(Layer::new("Guide", scene.frame_count()));
        let guide_index = scene.layers.len() - 1;
        scene.layers[guide_index].kind = LayerKind::Guide;
        scene.layers.push(Layer::new("Inhalt", scene.frame_count()));
        let content_index = scene.layers.len() - 1;
        scene.layers.push(Layer::new("Maske", scene.frame_count()));
        let mask_index = scene.layers.len() - 1;
        scene.layers[mask_index].kind = LayerKind::Mask;
        scene.layers.push(Layer::new("Ton", scene.frame_count()));
        let sound_index = scene.layers.len() - 1;
        scene.layers[sound_index].kind = LayerKind::Sound;
        scene.ensure_frame_count(3);
        for layer in scene.layers.iter_mut() {
            layer.ensure_frames(3);
        }
        scene.layers[content_index].frames[0].keyframe = true;
        scene.layers[content_index].frames[0]
            .objects
            .push(ObjectInstance::bitmap(
                "Bild", 100.0, 100.0, bitmap_id, 8.0, 8.0,
            ));
        scene.layers[content_index].frames[0]
            .objects
            .push(ObjectInstance::symbol(
                "ClipInstanz",
                200.0,
                200.0,
                clip_id,
                Rgba::WHITE,
            ));
        scene.layers[sound_index].frames[0].keyframe = true;
        scene.layers[sound_index].frames[0].sound = Some(SoundCue::event(audio_id));
        scene.layers[mask_index].frames[0].keyframe = true;
        scene.layers[mask_index].frames[0]
            .objects
            .push(ObjectInstance::ellipse(
                "Loch",
                320.0,
                180.0,
                200.0,
                120.0,
                Rgba::WHITE,
            ));

        project.validate().unwrap();

        let first = export_xfl(&project).unwrap();
        let mut once = import(&first.bytes).unwrap();
        let mut twice = export_xfl(&once.project).unwrap();
        let twice_imported = import(&twice.bytes).unwrap();

        for roundtrip in [&once.project, &twice_imported.project] {
            assert_eq!(roundtrip.symbols.len(), project.symbols.len());
            assert!(
                roundtrip
                    .symbols
                    .iter()
                    .any(|symbol| symbol.kind == SymbolKind::MovieClip)
            );
            assert!(
                roundtrip
                    .symbols
                    .iter()
                    .any(|symbol| symbol.kind == SymbolKind::Graphic)
            );
            assert_eq!(roundtrip.assets.len(), project.assets.len());
            assert!(roundtrip.assets.iter().any(|asset| {
                asset.kind == nova_core::AssetKind::Bitmap && asset.is_embedded()
            }));
            assert!(
                roundtrip.assets.iter().any(|asset| {
                    asset.kind == nova_core::AssetKind::Audio && asset.is_embedded()
                })
            );
            let scene = roundtrip.active_scene().unwrap();
            assert!(
                scene
                    .layers
                    .iter()
                    .any(|layer| layer.kind == LayerKind::Guide)
            );
            assert!(
                scene
                    .layers
                    .iter()
                    .any(|layer| layer.kind == LayerKind::Mask)
            );
            assert!(
                scene
                    .layers
                    .iter()
                    .any(|layer| layer.kind == LayerKind::Sound && layer.frames[0].sound.is_some())
            );
        }
        // The second export must also parse cleanly.
        twice.bytes.shrink_to_fit();
    }

    fn tiny_png_for_xfl() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn xfl_exports_tilemap_as_baked_bitmap() {
        use nova_core::model::{Tileset, Transform};
        let mut project = Project::demo();
        let png = tiny_png_for_xfl();
        let bitmap = Asset::bitmap("Raster", "assets/raster.png", "image/png", 1, 1, png);
        let asset_id = bitmap.id;
        project.assets.push(bitmap);
        project.tilesets.push(Tileset {
            name: "Terrain".into(),
            asset: asset_id,
            tile_width: 1,
            tile_height: 1,
            columns: 1,
            solid: vec![true],
            ..Default::default()
        });
        let tileset_id = project.tilesets[0].id;
        let mut map = nova_core::tilemap::new_tilemap(tileset_id, 1.0, 1.0, 4, 4, &["Ebene 1"]);
        nova_core::tilemap::paint(
            &mut map,
            0,
            nova_core::tilemap::PaintTool::Rect,
            0,
            0,
            3,
            3,
            1,
            0,
            0,
        )
        .unwrap();
        let tile_object = ObjectInstance::new(
            "Map",
            ObjectKind::Tilemap(map),
            Transform::at(50.0, 60.0),
            Rgba::WHITE,
        );
        project.active_scene_mut().unwrap().layers[0].frames[0]
            .objects
            .push(tile_object);

        let export = export_xfl(&project).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&export.bytes)).unwrap();
        let mut document_xml = String::new();
        archive
            .by_name("DOMDocument.xml")
            .unwrap()
            .read_to_string(&mut document_xml)
            .unwrap();
        assert!(
            document_xml.contains("DOMBitmapInstance"),
            "tilemap should export as bitmap instance"
        );
        assert!(document_xml.contains("Tilemap_Map_"));
        let png_entry = archive
            .file_names()
            .filter_map(|name| name.ok().map(|n| n.into_owned()))
            .find(|name| name.starts_with("LIBRARY/Tilemap_Map_") && name.ends_with(".png"))
            .expect("baked tilemap PNG in package");
        let mut png_bytes = Vec::new();
        archive
            .by_name(&png_entry)
            .unwrap()
            .read_to_end(&mut png_bytes)
            .unwrap();
        assert!(png_bytes.starts_with(b"\x89PNG"));
    }

    fn tiny_wav_for_xfl() -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&44u32.to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8u32.to_le_bytes());
        wav.extend_from_slice(&[0u8; 8]);
        wav
    }
}
