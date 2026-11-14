//! Headless companion for NØVA Flash Studio.
//!
//! Everything the GUI can build, the CLI can build too. This keeps the build
//! pipeline testable without a windowing system and provides the projector
//! smoke test artifact.

use std::error::Error;
use std::path::{Path, PathBuf};

use nova_build::build_swf;
use nova_core::{Project, ProjectTemplate, template_by_id};
use nova_formats::{FormatKind, analyze_path, export_xfl, load_path};
use nova_swf::inspect_file;

fn main() {
    if let Err(error) = run() {
        eprintln!("Fehler: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "demo" => {
            let out = args
                .get(1)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("samples").join("nova-demo.swf"));
            let project = Project::demo();
            export(&project, &out)
        }
        "import" => {
            let Some(input) = args.get(1) else {
                return Err("Aufruf: nova import <datei.nova|.fla|.swf> [ausgabe.nova]".into());
            };
            let input = Path::new(input);
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| input.with_extension("nova"));
            let imported = load_path(input)?;
            imported.project.save(&out)?;
            println!(
                "Importiert: {} -> {} ({} Szene(n), {} Skript(e))",
                input.display(),
                out.display(),
                imported.project.scenes.len(),
                imported.project.scripts.len()
            );
            println!("{}", imported.report.to_text());
            Ok(())
        }
        "analyze" => {
            let Some(path) = args.get(1) else {
                return Err("Aufruf: nova analyze <datei.nova|.fla|.swf>".into());
            };
            let result = analyze_path(Path::new(path))?;
            println!("{}", serde_json::to_string_pretty(&result.analysis)?);
            println!();
            println!("{}", result.report.to_text());
            Ok(())
        }
        "export" => {
            let Some(input) = args.get(1) else {
                return Err(
                    "Aufruf: nova export <datei.nova|.fla|.swf> [ausgabe.swf|.zip|.nova]".into(),
                );
            };
            let input = Path::new(input);
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| default_export_path(input));
            if out
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("fla"))
            {
                return Err(
                    "Nativer FLA-Export wird nicht angeboten. Nutze eine .zip-Ausgabe für das \
                     ausdrücklich unverifizierte XFL-Kompatibilitätspaket."
                        .into(),
                );
            }
            let imported = load_path(input)?;
            if out
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
            {
                let export = export_xfl(&imported.project)?;
                write_output(&out, &export.bytes)?;
                println!(
                    "XFL-Kompatibilitätspaket geschrieben: {} ({} Bytes)",
                    out.display(),
                    export.bytes.len()
                );
                println!("{}", export.report.to_text());
                return Ok(());
            }
            if out
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("nova"))
            {
                imported.project.save(&out)?;
                println!("NØVA-Projekt geschrieben: {}", out.display());
                return Ok(());
            }
            if imported.source_format != FormatKind::Nova {
                println!(
                    "Hinweis: {} wird als rekonstruiertes SWF exportiert, nicht bytegenau.",
                    imported.source_format.label()
                );
            }
            export(&imported.project, &out)
        }
        "probe" => {
            let Some(path) = args.get(1) else {
                return Err("Aufruf: nova probe <datei.swf>".into());
            };
            let path = Path::new(path);
            if !path.is_file() {
                return Err(format!("Datei nicht gefunden: {}", path.display()).into());
            }
            let info = inspect_file(path)?;
            println!("{}", info.summary());
            println!(
                "Signatur: {} · deklariert: {} Bytes · tatsächlich: {} Bytes",
                info.signature, info.declared_length, info.actual_length
            );
            println!("Tags:");
            for (index, tag) in info.tags.iter().enumerate() {
                if index >= 60 {
                    println!("  … {} weitere Tags", info.tags.len() - index);
                    break;
                }
                println!(
                    "  {:>3}  {:<22} code={:<3} len={}",
                    index, tag.name, tag.code, tag.length
                );
            }
            for warning in &info.warnings {
                println!("Warnung: {warning}");
            }
            if !info.has_end {
                return Err("Datei enthält kein End-Tag".into());
            }
            Ok(())
        }
        "new" => {
            let mut path: Option<PathBuf> = None;
            let mut template = ProjectTemplate::Empty;
            let mut index = 1;
            while index < args.len() {
                if args[index] == "--template" {
                    let Some(id) = args.get(index + 1) else {
                        return Err(
                            "Aufruf: nova new <projekt.nova> [--template empty|animation|platformer|ui]"
                                .into(),
                        );
                    };
                    template = template_by_id(id).ok_or_else(|| {
                        format!(
                            "Unbekannte Vorlage '{id}'. Verfügbar: empty, animation, platformer, ui"
                        )
                    })?;
                    index += 2;
                    continue;
                }
                if args[index].starts_with("--") {
                    return Err(format!("Unbekannte Option '{}'", args[index]).into());
                }
                if path.is_some() {
                    return Err(
                        "Aufruf: nova new <projekt.nova> [--template empty|animation|platformer|ui]"
                            .into(),
                    );
                }
                path = Some(PathBuf::from(&args[index]));
                index += 1;
            }
            let Some(path) = path else {
                return Err(
                    "Aufruf: nova new <projekt.nova> [--template empty|animation|platformer|ui]"
                        .into(),
                );
            };
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unbenannt");
            let project = template.create(name);
            project.validate()?;
            project.save(&path)?;
            println!(
                "Projekt erstellt: {} · Vorlage: {} · {} Szene(n)",
                path.display(),
                template.label(),
                project.scenes.len()
            );
            Ok(())
        }
        "check" => {
            let Some(path) = args.get(1) else {
                return Err("Aufruf: nova check <projekt.nova>".into());
            };
            let project = load_path(Path::new(path))?.project;
            project.validate()?;
            let symbols: usize = project.symbols.len();
            println!(
                "OK · '{}' · {} Szene(n) · {} Symbol(e) · {} Skript(e)",
                project.metadata.name,
                project.scenes.len(),
                symbols,
                project.scripts.len()
            );
            for scene in &project.scenes {
                println!(
                    "  Szene '{}': {}×{} px, {} fps, {} Ebenen, {} Frames",
                    scene.name,
                    scene.width,
                    scene.height,
                    scene.fps,
                    scene.layers.len(),
                    scene.frame_count()
                );
            }
            Ok(())
        }
        "as-check" => {
            let Some(path) = args.get(1) else {
                return Err("Aufruf: nova as-check <datei.as>".into());
            };
            let source = std::fs::read_to_string(path)?;
            let unit = nova_as::compile(&source);
            for diagnostic in &unit.diagnostics {
                println!("{}", diagnostic.render(&source, path));
            }
            let errors = unit
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.severity == nova_as::Severity::Error)
                .count();
            if errors > 0 {
                return Err(format!("{errors} Fehler gefunden").into());
            }
            println!(
                "OK: {} · {} Klasse(n) · {} Byte ABC",
                path,
                unit.classes.len(),
                unit.abc.len()
            );
            Ok(())
        }
        "ai-status" => {
            let config_dir = nova_ai::default_config_dir()?;
            let manager = nova_ai::ProviderManager::load(&config_dir)?;
            println!("KI-Konfiguration: {}", config_dir.display());
            println!("Anmeldedatenspeicher: {}", manager.credential_backend());
            if manager.settings.providers.is_empty() {
                println!("Keine KI-Anbieter konfiguriert.");
                return Ok(());
            }
            for provider in &manager.settings.providers {
                let key_present = manager.has_credential(&provider.id).unwrap_or(false);
                println!(
                    "- {} [{}] {}{}{}",
                    provider.name,
                    provider.kind.label(),
                    if provider.is_cloud() {
                        "CLOUD"
                    } else {
                        "LOKAL"
                    },
                    if provider.enabled {
                        ""
                    } else {
                        " · deaktiviert"
                    },
                    if manager.settings.default_provider.as_deref() == Some(provider.id.as_str()) {
                        " · Standard"
                    } else {
                        ""
                    }
                );
                println!(
                    "  URL: {} · Modell: {} · Schlüssel gespeichert: {}",
                    provider.base_url,
                    if provider.model.is_empty() {
                        "(nicht gewählt)"
                    } else {
                        provider.model.as_str()
                    },
                    if key_present { "ja" } else { "nein" }
                );
                if provider.needs_credential() && !key_present {
                    println!("  Warnung: Für diesen Anbieter fehlt ein API-Schlüssel.");
                }
            }
            Ok(())
        }
        "build" => {
            let Some(input) = args.get(1) else {
                return Err("Aufruf: nova build <datei.nova|.fla|.swf> [ausgabe.swf]".into());
            };
            let input = Path::new(input);
            let imported = load_path(input)?;
            let report = nova_ai::build_project(&imported.project);
            println!("{}", report.text());
            if !report.ok {
                return Err("Build fehlgeschlagen.".into());
            }
            let output = report.output.ok_or("Build lieferte keine SWF-Ausgabe.")?;
            let out = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| default_export_path(input));
            write_output(&out, &output.bytes)?;
            println!(
                "SWF geschrieben: {} ({} Bytes)",
                out.display(),
                output.bytes.len()
            );
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("Unbekannter Befehl '{other}'. Nutze 'nova help'.").into()),
    }
}

fn export(project: &Project, out: &Path) -> Result<(), Box<dyn Error>> {
    let output = build_swf(project)?;
    write_output(out, &output.bytes)?;
    println!(
        "SWF geschrieben: {} ({} Bytes, {} Frames, {} Platzierungen, {} Shapes, {} Sprites)",
        out.display(),
        output.stats.bytes,
        output.stats.frames,
        output.stats.placed_objects,
        output.stats.defined_shapes,
        output.stats.defined_sprites
    );
    for warning in &output.warnings {
        println!("Hinweis: {warning}");
    }
    let info = nova_swf::inspect(&output.bytes)?;
    println!("Verifikation: {}", info.summary());
    for warning in &info.warnings {
        println!("SWF-Warnung: {warning}");
    }
    Ok(())
}

fn default_export_path(input: &Path) -> PathBuf {
    if input
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("swf"))
    {
        let stem = input
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("film");
        input.with_file_name(format!("{stem}.reconstructed.swf"))
    } else {
        input.with_extension("swf")
    }
}

fn write_output(out: &Path, bytes: &[u8]) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    // Write via a sibling temp file so a failed write cannot truncate or
    // corrupt an already existing output file.
    let tmp = out.with_file_name(format!(
        "{}.tmp",
        out.file_name().and_then(|n| n.to_str()).unwrap_or("output")
    ));
    std::fs::write(&tmp, bytes)?;
    if out.exists() {
        std::fs::remove_file(out)?;
    }
    std::fs::rename(&tmp, out)?;
    Ok(())
}

fn print_help() {
    println!(
        "NØVA Flash Studio CLI

Aufrufe:
  nova demo [ausgabe.swf]            Demo-Projekt bauen (Standard: samples/nova-demo.swf)
  nova import <datei> [ausgabe.nova] NØVA, FLA/XFL oder SWF importieren und als .nova speichern
  nova analyze <datei>               Formatanalyse, Code-Herkunft und Kompatibilitätsbericht ausgeben
  nova export <datei> [out]          Als SWF, XFL-Paket (.zip) oder .nova exportieren
  nova probe <datei.swf>             SWF-Header und Tags prüfen
  nova new <projekt.nova> [--template <id>]
                                     Projekt aus Vorlage anlegen
                                     (empty, animation, platformer, ui)
  nova check <datei>                 NØVA/FLA/SWF-Projekt validieren
  nova build <datei> [ausgabe.swf]   Projekt mit Compiler-/Build-Diagnostik bauen
  nova as-check <datei.as>           AS3-Teilmenge prüfen und ABC kompilieren
  nova ai-status                     KI-Anbieter ohne Schlüsselwerte diagnostizieren
  nova help                          Diese Hilfe

Der native .fla-Export wird bewusst nicht angeboten. Das XFL-Paket ist
unverifiziert und kein Ersatz für eine in Adobe Animate getestete .fla-Datei.
"
    );
}
