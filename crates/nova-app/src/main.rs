//! NØVA Flash Studio native desktop entry point.

mod ai_editor;
mod ai_providers;
mod app;
mod audio;
mod autosave;
mod branding;
mod code_view;
mod console;
mod edition;
mod flash;
mod highlight;
mod history;
mod properties;
mod splash;
mod stage_view;
mod state;
mod theme;
mod tilemap_view;
mod timeline_view;
mod tree;
mod util;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if handle_cli(&args) {
        return Ok(());
    }
    let startup_project = startup_project_arg(&args);

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1060.0, 660.0])
        .with_app_id(branding::bundle_id())
        .with_title(branding::app_name());
    if let Ok(icon) = branding::window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        branding::app_name(),
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            match app::NovaApp::try_new(cc, startup_project.as_deref()) {
                Ok(app) => Ok(Box::new(app)),
                Err(error) => Ok(Box::new(StartupFailure::new(error))),
            }
        }),
    )
}

/// A shell association passes the `.nova` or `.swf` file as a positional
/// argument. The explicit `--open <path>` form is useful for scripts and avoids
/// ambiguity.
fn startup_project_arg(args: &[String]) -> Option<PathBuf> {
    let mut iterator = args.iter();
    while let Some(argument) = iterator.next() {
        if argument == "--open" {
            return iterator.next().map(PathBuf::from);
        }
        if !argument.starts_with('-') {
            return Some(PathBuf::from(argument));
        }
    }
    None
}

/// Return true when a command-line option was handled without opening a window.
fn handle_cli(args: &[String]) -> bool {
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("{} {}", branding::app_name(), branding::VERSION);
        return true;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "{}\nVersion {}\n\nUSAGE:\n    nova-flash-studio [OPTIONS] [PROJECT.nova]\n\nOPTIONS:\n    -h, --help             Diese Hilfe anzeigen\n    -V, --version          Version ausgeben\n        --open <PFAD>      NØVA-Projekt oder SWF öffnen\n        --startup-check    Branding, Konfiguration und Runtime-Status prüfen, ohne ein Fenster zu öffnen\n\nENVIRONMENT:\n    NOVA_FLASH_STUDIO_CONFIG_DIR   Konfigurationsverzeichnis überschreiben\n\nDie GUI startet ohne weitere Argumente. Eine SWF wird über „Open in Flash Player“ in der konfigurierten Adobe- oder Ruffle-Runtime geöffnet.",
            branding::app_name(),
            branding::VERSION
        );
        return true;
    }
    if args.iter().any(|arg| arg == "--startup-check") {
        println!("{} {}", branding::app_name(), branding::VERSION);
        match branding::window_icon() {
            Ok(icon) => println!("window-icon: {}x{}", icon.width, icon.height),
            Err(error) => println!("window-icon: FEHLER: {error}"),
        }
        match flash::load_settings() {
            Ok(settings) => {
                let status = flash::status(&settings);
                println!("runtime: {} (bereit: {})", status.summary(), status.ready);
                println!(
                    "first-launch-setup: {}",
                    flash::needs_first_launch_setup(&settings)
                );
                println!(
                    "adobe-projector: {}",
                    settings
                        .adobe_projector_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "nicht konfiguriert".to_string())
                );
                println!(
                    "ruffle: {}",
                    settings
                        .ruffle_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "nicht konfiguriert".to_string())
                );
            }
            Err(error) => println!("runtime: FEHLER: {error}"),
        }
        match flash::settings_path() {
            Ok(path) => println!("config: {}", path.display()),
            Err(error) => println!("config: FEHLER: {error}"),
        }
        return true;
    }
    false
}

/// Minimal fallback used only if core application state cannot be created. It
/// still presents a readable diagnostic instead of closing silently.
struct StartupFailure {
    error: String,
}

impl StartupFailure {
    fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

impl eframe::App for StartupFailure {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(48.0);
            ui.heading(
                egui::RichText::new(branding::app_name())
                    .color(egui::Color32::from_rgb(92, 234, 255)),
            );
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new("Die Anwendung konnte nicht initialisiert werden.")
                    .strong()
                    .color(egui::Color32::from_rgb(245, 184, 92)),
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical()
                .max_height(360.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(&self.error).monospace().small())
                            .wrap(),
                    );
                });
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Fehler kopieren").clicked() {
                    ui.ctx().copy_text(self.error.clone());
                }
                if ui.button("Beenden").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_project_argument_is_accepted() {
        let args = vec!["C:/Projekte/Mein Film.nova".to_string()];
        assert_eq!(
            startup_project_arg(&args),
            Some(PathBuf::from("C:/Projekte/Mein Film.nova"))
        );
    }

    #[test]
    fn explicit_open_argument_is_accepted() {
        let args = vec![
            "--open".to_string(),
            "C:/Projekte/Ø mit Leerzeichen.nova".to_string(),
        ];
        assert_eq!(
            startup_project_arg(&args),
            Some(PathBuf::from("C:/Projekte/Ø mit Leerzeichen.nova"))
        );
    }

    #[test]
    fn options_without_a_project_do_not_create_a_path() {
        assert_eq!(startup_project_arg(&["--startup-check".to_string()]), None);
        assert_eq!(startup_project_arg(&["--open".to_string()]), None);
    }
}
