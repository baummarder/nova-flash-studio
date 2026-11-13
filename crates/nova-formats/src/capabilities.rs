//! Capability descriptions and compatibility reports.
//!
//! These types are intentionally plain data: the UI, the CLI and any future AI
//! agent can render or inspect them without depending on egui or on a specific
//! importer.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatKind {
    Nova,
    Fla,
    Swf,
    XflPackage,
}

impl FormatKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Nova => "NØVA",
            Self::Fla => "Adobe FLA/XFL",
            Self::Swf => "SWF",
            Self::XflPackage => "XFL-Kompatibilitätspaket",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Nova => "nova",
            Self::Fla => "fla",
            Self::Swf => "swf",
            Self::XflPackage => "zip",
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("nova") => Some(Self::Nova),
            Some("fla") => Some(Self::Fla),
            Some("swf") => Some(Self::Swf),
            Some("xfl") | Some("zip") => Some(Self::XflPackage),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    /// Imported/exported with the structure the editor can edit.
    Full,
    /// Imported/exported for a documented subset; unsupported data is reported.
    Partial,
    /// Only preserved as opaque source data; not editable or re-exported.
    PreservedOnly,
    /// Not supported.
    Unsupported,
}

impl SupportLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => "vollständig",
            Self::Partial => "teilweise",
            Self::PreservedOnly => "nur erhalten",
            Self::Unsupported => "nicht unterstützt",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub name: String,
    pub level: SupportLevel,
    pub detail: String,
}

impl Capability {
    pub fn new(name: impl Into<String>, level: SupportLevel, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapabilities {
    pub format: FormatKind,
    pub versions: String,
    pub import: Vec<Capability>,
    pub export: Vec<Capability>,
    pub limitations: Vec<String>,
}

pub fn capabilities(format: FormatKind) -> FormatCapabilities {
    match format {
        FormatKind::Nova => FormatCapabilities {
            format,
            versions: "nova-flash-studio formatVersion 1".into(),
            import: vec![
                Capability::new(
                    "Projektmodell",
                    SupportLevel::Full,
                    "Szenen, Ebenen, Frames, Keyframes, Tweens, Symbole, Skripte, Assets und Exporteinstellungen.",
                ),
                Capability::new(
                    "Quelltext",
                    SupportLevel::Full,
                    "ActionScript-Quelltext wird als Originalquelle gelesen.",
                ),
            ],
            export: vec![Capability::new(
                "Projektdatei",
                SupportLevel::Full,
                "Deterministisches JSON mit Formatversion und Migration älterer Dateien.",
            )],
            limitations: vec![
                "Das NØVA-Format ist kein Adobe-Austauschformat.".into(),
                "Assets werden referenziert, nicht eingebettet.".into(),
            ],
        },
        FormatKind::Fla => FormatCapabilities {
            format,
            versions: "ZIP/XFL-Container moderner FLA-Dateien; dokumentierte Teilmenge".into(),
            import: vec![
                Capability::new(
                    "Dokumenteinstellungen",
                    SupportLevel::Full,
                    "Bühnenbreite/-höhe, FrameRate und Hintergrundfarbe aus DOMDocument.xml.",
                ),
                Capability::new(
                    "Timeline",
                    SupportLevel::Partial,
                    "DOMTimeline/DOMLayer/DOMFrame mit Keyframes, Labels und einfachen Elementen.",
                ),
                Capability::new(
                    "Symbole",
                    SupportLevel::Partial,
                    "DOMSymbolInstance wird als Bibliothekssymbol rekonstruiert; verschachtelte Symbol-Timelines werden nicht vollständig abgebildet.",
                ),
                Capability::new(
                    "Vektorgrafik",
                    SupportLevel::Partial,
                    "DOMShape-Grenzen und einfache Füllungen; komplexe XFL-Edge-Strings werden als Warnung gemeldet.",
                ),
                Capability::new(
                    "ActionScript",
                    SupportLevel::Partial,
                    "Skripte aus scripts/*.as werden als Quelltext importiert, wenn sie im Container vorhanden sind.",
                ),
                Capability::new(
                    "Text, Bitmaps, Sounds, Filter",
                    SupportLevel::PreservedOnly,
                    "Werden nicht in das editierbare Modell übernommen; die Originaldatei bleibt erhalten und der Bericht nennt die Einträge.",
                ),
            ],
            export: vec![
                Capability::new(
                    "XFL-Kompatibilitätspaket",
                    SupportLevel::Partial,
                    "Erzeugt ein klar gekennzeichnetes ZIP mit DOMDocument.xml und Quelltexten. Kein verifizierter nativer .fla-Export.",
                ),
                Capability::new(
                    "Nativer .fla-Export",
                    SupportLevel::Unsupported,
                    "Wird nicht angeboten, weil ohne Adobe-Animate-Test keine belastbare Gültigkeit behauptet werden kann.",
                ),
            ],
            limitations: vec![
                "FLA ist ein ZIP/XFL-Container; die genaue XFL-Schemaabdeckung ist herstellerabhängig.".into(),
                "Komplexe Formen, Filter, Text, Bitmaps, Sounds und verschachtelte Symbol-Timelines sind nicht vollständig editierbar.".into(),
                "Die XFL-Ausgabe wurde nicht in Adobe Animate/Flash geöffnet; sie ist ausdrücklich unverifiziert.".into(),
            ],
        },
        FormatKind::Swf => FormatCapabilities {
            format,
            versions: "SWF-Header-Versionen werden gelesen; getestet sind SWF 9, 10 und 13. FWS, CWS und ZWS/LZMA werden importiert.".into(),
            import: vec![
                Capability::new(
                    "Header",
                    SupportLevel::Full,
                    "Version, Kompression, Bühnenmaße, FrameRate und Frameanzahl.",
                ),
                Capability::new(
                    "Timeline",
                    SupportLevel::Partial,
                    "ShowFrame, PlaceObject/RemoveObject, Frames, Ebenen nach Tiefe und FrameLabels.",
                ),
                Capability::new(
                    "Formen",
                    SupportLevel::Partial,
                    "DefineShape/DefineShape2/DefineShape3 werden als Pfadobjekte mit Füllungen rekonstruiert; Linien, Gradienten und Bitmap-Füllungen werden berichtet.",
                ),
                Capability::new(
                    "Sprites",
                    SupportLevel::Partial,
                    "DefineSprite-Timelines werden in die Haupt-Timeline gebacken; verschachtelte Sprite-Strukturen bleiben nicht als Bibliothekssymbol erhalten.",
                ),
                Capability::new(
                    "ActionScript/ABC",
                    SupportLevel::PreservedOnly,
                    "DoABC/DoABC2-Bytes werden unverändert erhalten. Es wird kein Quelltext dekompiliert und keine Originalität behauptet.",
                ),
                Capability::new(
                    "Text, Bitmaps, Sounds, Filter, Video",
                    SupportLevel::PreservedOnly,
                    "Werden nicht in das editierbare Modell übernommen; die Originaldatei bleibt erhalten und der Bericht nennt die Tags.",
                ),
            ],
            export: vec![
                Capability::new(
                    "SWF",
                    SupportLevel::Full,
                    "NØVA-Projekte, importierte FLA-Projekte und importierte SWF-Projekte werden als echtes SWF geschrieben.",
                ),
                Capability::new(
                    "Preserved ABC",
                    SupportLevel::Partial,
                    "Vorhandene ABC-Bytes werden unverändert eingebettet; automatische Klassenverknüpfung ist ohne Klassentabelle nicht möglich.",
                ),
            ],
            limitations: vec![
                "Nicht jede SWF-Tag-Art ist editierbar; unbekannte Tags werden im Bericht aufgeführt.".into(),
                "Der Exporter ist kein Byte-für-Byte-Rekonstruktor; er vergleicht Struktur und sichtbare Platzierungen.".into(),
                "Adobe-Flash-Player-Wiedergabe ist nur mit einem realen Adobe-Projektor verifizierbar.".into(),
            ],
        },
        FormatKind::XflPackage => FormatCapabilities {
            format,
            versions: "Experimentelles ZIP/XFL-Paket, nicht als .fla deklariert".into(),
            import: vec![Capability::new(
                "Rückimport",
                SupportLevel::Partial,
                "Das Paket kann mit demselben XFL-Teilparser gelesen werden.",
            )],
            export: vec![Capability::new(
                "XFL-Paket",
                SupportLevel::Partial,
                "DOMDocument.xml, scripts/ und eine README mit Einschränkungen.",
            )],
            limitations: vec![
                "Kein Ersatz für eine von Adobe getestete .fla-Datei.".into(),
                "Nicht alle XFL-Elemente werden erzeugt.".into(),
            ],
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
}

impl IssueSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Warning => "Warnung",
            Self::Error => "Fehler",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueCategory {
    Structure,
    Geometry,
    Timeline,
    Code,
    Assets,
    Text,
    Sound,
    Bitmap,
    Filter,
    UnsupportedTag,
    Export,
}

impl IssueCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Structure => "Struktur",
            Self::Geometry => "Geometrie",
            Self::Timeline => "Timeline",
            Self::Code => "Code",
            Self::Assets => "Assets",
            Self::Text => "Text",
            Self::Sound => "Sound",
            Self::Bitmap => "Bitmap",
            Self::Filter => "Filter",
            Self::UnsupportedTag => "Nicht unterstütztes Tag",
            Self::Export => "Export",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl CompatibilityIssue {
    pub fn new(
        severity: IssueSeverity,
        category: IssueCategory,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            category,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityReport {
    pub source: FormatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    #[serde(default)]
    pub issues: Vec<CompatibilityIssue>,
    #[serde(default)]
    pub stats: BTreeMap<String, String>,
}

impl CompatibilityReport {
    pub fn new(source: FormatKind) -> Self {
        Self {
            source,
            source_version: None,
            issues: Vec::new(),
            stats: BTreeMap::new(),
        }
    }

    pub fn info(&mut self, category: IssueCategory, message: impl Into<String>) {
        self.issues.push(CompatibilityIssue::new(
            IssueSeverity::Info,
            category,
            message,
        ));
    }

    pub fn warning(&mut self, category: IssueCategory, message: impl Into<String>) {
        self.issues.push(CompatibilityIssue::new(
            IssueSeverity::Warning,
            category,
            message,
        ));
    }

    pub fn error(&mut self, category: IssueCategory, message: impl Into<String>) {
        self.issues.push(CompatibilityIssue::new(
            IssueSeverity::Error,
            category,
            message,
        ));
    }

    pub fn push(&mut self, issue: CompatibilityIssue) {
        self.issues.push(issue);
    }

    pub fn merge(&mut self, other: CompatibilityReport) {
        self.issues.extend(other.issues);
        self.stats.extend(other.stats);
    }

    pub fn has_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == IssueSeverity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity >= IssueSeverity::Warning)
    }

    pub fn count(&self, severity: IssueSeverity) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity == severity)
            .count()
    }

    pub fn set_stat(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.stats.insert(key.into(), value.into());
    }

    pub fn to_text(&self) -> String {
        let mut out = format!(
            "{}: {} Info, {} Warnung(en), {} Fehler\n",
            self.source.label(),
            self.count(IssueSeverity::Info),
            self.count(IssueSeverity::Warning),
            self.count(IssueSeverity::Error)
        );
        if let Some(version) = &self.source_version {
            out.push_str(&format!("Version: {version}\n"));
        }
        for (key, value) in &self.stats {
            out.push_str(&format!("{key}: {value}\n"));
        }
        for issue in &self.issues {
            out.push_str(&format!(
                "[{} / {}] {}\n",
                issue.severity.label(),
                issue.category.label(),
                issue.message
            ));
            if let Some(detail) = &issue.detail {
                out.push_str(&format!("  {detail}\n"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts_and_text() {
        let mut report = CompatibilityReport::new(FormatKind::Swf);
        report.info(IssueCategory::Structure, "header ok");
        report.warning(IssueCategory::Geometry, "gradient approximated");
        assert!(report.has_warnings());
        assert!(!report.has_errors());
        assert!(report.to_text().contains("gradient approximated"));
    }

    #[test]
    fn extensions_are_case_insensitive() {
        assert_eq!(
            FormatKind::from_path(Path::new("A.SWF")),
            Some(FormatKind::Swf)
        );
        assert_eq!(
            FormatKind::from_path(Path::new("A.FLA")),
            Some(FormatKind::Fla)
        );
        assert_eq!(
            FormatKind::from_path(Path::new("A.NOVA")),
            Some(FormatKind::Nova)
        );
    }
}
