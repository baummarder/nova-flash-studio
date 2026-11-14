//! M18 integration tests: asset workflow operations, compatibility audit
//! and XFL/FLA format handling.

use nova_ai::{ProjectOperation, apply_operations};
use nova_core::{AssetKind, ObjectInstance, ObjectKind, Project, Rgba};

fn asset_project() -> Project {
    let mut project = Project::new_empty("Asset-Test");
    // Tiny valid PNG (1x1) embedded directly.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    project.assets.push(nova_core::Asset::bitmap(
        "BG",
        "assets/bg.png",
        "image/png",
        1,
        1,
        png.to_vec(),
    ));
    project
}

#[test]
fn rename_and_move_asset_keep_references() {
    let mut project = asset_project();
    let asset_id = project.assets[0].id;
    let scene = project.active_scene_mut().unwrap();
    scene.layers[0].frames[0]
        .objects
        .push(ObjectInstance::bitmap(
            "BGInst", 10.0, 10.0, asset_id, 50.0, 50.0,
        ));

    apply_operations(
        &mut project,
        &[
            ProjectOperation::RenameAsset {
                asset: "BG".into(),
                name: "Hintergrund".into(),
            },
            ProjectOperation::MoveAsset {
                asset: "Hintergrund".into(),
                folder: "UI/Sprites".into(),
            },
        ],
    )
    .expect("ops apply");

    let asset = project.asset(asset_id).unwrap();
    assert_eq!(asset.name, "Hintergrund");
    assert_eq!(asset.folder, "UI/Sprites");
    // Reference via stable id still resolves.
    let scene = project.active_scene().unwrap();
    let object = &scene.layers[0].frames[0].objects[0];
    assert!(matches!(object.kind, ObjectKind::Bitmap { asset, .. } if asset == asset_id));
}

#[test]
fn audit_assets_reports_unused_and_broken() {
    let mut project = asset_project();
    let summary = apply_operations(&mut project, &[ProjectOperation::AuditAssets])
        .expect("audit runs")
        .remove(0);
    assert!(summary.contains("1 Assets"), "{summary}");
    assert!(summary.contains("ungenutzt"), "{summary}");
}

#[test]
fn analyze_project_compatibility_reports_content() {
    let mut project = asset_project();
    let summary = apply_operations(
        &mut project,
        &[ProjectOperation::AnalyzeProjectCompatibility],
    )
    .expect("analysis runs")
    .remove(0);
    assert!(summary.contains("Projekt-Kompatibilität"), "{summary}");
    assert!(summary.contains("1 Assets"), "{summary}");
}

#[test]
fn replace_asset_rejects_unsafe_path() {
    let mut project = asset_project();
    let result = apply_operations(
        &mut project,
        &[ProjectOperation::ReplaceAsset {
            asset: "BG".into(),
            path: "../escape.png".into(),
        }],
    );
    assert!(result.is_err(), "path traversal must be rejected");
}

#[test]
fn xfl_import_decodes_edge_strings_and_gradients() {
    // Build a minimal XFL zip with a DOMShape containing a decoded edge
    // string and a linear gradient fill.
    let document = r##"<?xml version="1.0" encoding="utf-8"?>
<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="640" height="360" frameRate="24" backgroundColor="#102030">
  <timelines>
    <DOMTimeline name="Scene 1">
      <layers>
        <DOMLayer name="Ebene 1" color="#4FFF4F" current="true" isSelected="true" locked="false" visible="true" autoNamed="false">
          <frames>
            <DOMFrame index="0" keyMode="9728" duration="1">
              <elements>
                <DOMShape name="Dreieck">
                  <fills>
                    <FillStyle index="1"><LinearGradient startPoint="-50 -50" endPoint="50 50" spreadMethod="pad">
                      <GradientEntry ratio="0" color="#FF0000"/>
                      <GradientEntry ratio="1" color="#0000FF"/>
                    </LinearGradient></FillStyle>
                  </fills>
                  <edges>
                    <Edge fillStyle0="1" edges="!0 0|100 0|0 100|0 0"/>
                  </edges>
                </DOMShape>
                <DOMText name="Titel" x="50" y="60" width="200" height="40">
                  <matrix><Matrix a="1" d="1" tx="50" ty="60"/></matrix>
                  <text>
                    <matrix><Matrix a="1" d="1" tx="0" ty="0"/></matrix>
                    <div>
                      <p><span size="24" face="Nove Sans" color="#112233" bold="true">Hallo XFL</span></p>
                    </div>
                  </text>
                </DOMText>
              </elements>
            </DOMFrame>
          </frames>
        </DOMLayer>
      </layers>
    </DOMTimeline>
  </timelines>
</DOMDocument>"##;
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("DOMDocument.xml", options).unwrap();
        std::io::Write::write_all(&mut writer, document.as_bytes()).unwrap();
        writer.finish().unwrap();
    }
    let imported =
        nova_formats::import_bytes(nova_formats::FormatKind::Fla, None, cursor.into_inner())
            .expect("xfl imports");

    let scene = imported.project.active_scene().unwrap();
    let objects = &scene.layers[0].frames[0].objects;
    assert_eq!(objects.len(), 2, "shape + text: {objects:?}");

    let shape = objects
        .iter()
        .find(|o| o.name == "Dreieck")
        .expect("shape present");
    let ObjectKind::Path { paths, .. } = &shape.kind else {
        panic!("expected path object");
    };
    assert_eq!(paths.len(), 1);
    assert!(
        paths[0].gradient.is_some(),
        "linear gradient must be decoded"
    );
    let commands = &paths[0].commands;
    assert!(commands.len() >= 4, "edge string decoded: {commands:?}");

    let text = objects
        .iter()
        .find(|o| o.name == "Titel")
        .expect("text present");
    let ObjectKind::Text(t) = &text.kind else {
        panic!("expected text object");
    };
    assert_eq!(t.text, "Hallo XFL");
    assert!(t.bold);
    assert_eq!(text.fill, Rgba::rgb(0x11, 0x22, 0x33));
}

#[test]
fn ole2_binary_fla_is_rejected_with_guidance() {
    // OLE2 compound document signature (pre-CS5 binary FLA).
    let bytes = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1, 0, 0, 0, 0];
    let format = nova_formats::detect_format(&bytes);
    assert_eq!(format, nova_formats::FormatKind::Fla);
    let result = nova_formats::import_bytes(format, None, bytes.to_vec());
    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("OLE2") || message.contains("binäre"),
        "{message}"
    );
}

#[test]
fn xfl_directory_imports() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("DOMDocument.xml"),
        r##"<?xml version="1.0" encoding="utf-8"?>
<DOMDocument xmlns="http://ns.adobe.com/xfl/2008/" width="320" height="240" frameRate="12">
  <timelines>
    <DOMTimeline name="Szene">
      <layers>
        <DOMLayer name="Ebene 1" color="#4FFF4F" current="true" isSelected="true" locked="false" visible="true" autoNamed="false">
          <frames>
            <DOMFrame index="0" keyMode="9728" duration="1">
              <elements>
                <DOMRectangleObject name="Box" x="10" y="10" width="50" height="40"/>
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
    std::fs::write(root.join("Projekt.xfl"), "PROXY-CS5").unwrap();

    let imported = nova_formats::load_path(root).expect("dir imports");
    assert_eq!(imported.source_format, nova_formats::FormatKind::XflPackage);
    let scene = imported.project.active_scene().unwrap();
    assert_eq!(scene.width, 320.0);
    assert_eq!(scene.fps, 12.0);
    assert_eq!(scene.layers[0].frames[0].objects.len(), 1);
}

#[test]
fn zip_slip_entries_are_rejected() {
    // ZIP containing a traversal entry must be refused before use.
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("../evil.xml", options).unwrap();
        std::io::Write::write_all(&mut writer, b"x").unwrap();
        writer.finish().unwrap();
    }
    let result =
        nova_formats::import_bytes(nova_formats::FormatKind::Fla, None, cursor.into_inner());
    assert!(result.is_err());
}

#[test]
fn asset_kind_used_in_fixture() {
    let project = asset_project();
    assert_eq!(project.assets[0].kind, AssetKind::Bitmap);
}
