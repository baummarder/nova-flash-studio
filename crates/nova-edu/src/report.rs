//! Exportable reports (M20.6): CSV, Markdown, static HTML. Deterministic
//! ordering, RFC-4180-style CSV escaping, no external assets.

use std::fmt::Write as _;

use crate::error::EduResult;
use crate::progress::{ClassOverview, ProgressEntry, assignment_progress};
use crate::workspace::Workspace;

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn unix_to_date(unix: u64) -> String {
    // Days since epoch → civil date (Howard Hinnant algorithm), UTC.
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// CSV report for one assignment's progress table.
pub fn progress_csv(rows: &[ProgressEntry], assignment_title: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# NØVA Edu Fortschrittsbericht — {}\n",
        csv_escape(assignment_title)
    ));
    out.push_str("Mitglied;Gruppe;Status;Revision;Abgabe;Verspätet;Kommentare;Offene;Rubrik\n");
    for row in rows {
        let rubric = row
            .rubric_marks
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("|");
        let submitted = row.submitted_unix.map(unix_to_date).unwrap_or_default();
        out.push_str(&format!(
            "{};{};{};{};{};{};{};{};{}\n",
            csv_escape(&row.member_name),
            csv_escape(row.group_name.as_deref().unwrap_or("")),
            csv_escape(row.status.label()),
            row.revision,
            csv_escape(&submitted),
            if row.late { "ja" } else { "nein" },
            row.feedback_comments,
            row.unacknowledged,
            csv_escape(&rubric),
        ));
    }
    out
}

/// Markdown report for class overview.
pub fn class_overview_markdown(overview: &ClassOverview) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Klassenübersicht: {}", overview.class_name);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Mitglieder: {} · Aufgaben: {}",
        overview.members,
        overview.assignments.len()
    );
    let _ = writeln!(out);
    let _ = writeln!(out, "| Aufgabe | Abgegeben | Gesehen | Fällig |");
    let _ = writeln!(out, "| --- | --- | --- | --- |");
    for a in &overview.assignments {
        let due = a.due_unix.map(unix_to_date).unwrap_or_else(|| "—".into());
        let _ = writeln!(
            out,
            "| {} | {}/{} | {} | {} |",
            a.title, a.submitted, a.total_members, a.reviewed, due
        );
    }
    out
}

/// Minimal static HTML report (no external assets, inline CSS).
pub fn class_overview_html(overview: &ClassOverview) -> String {
    let mut out = String::new();
    out.push_str(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>NØVA Edu Bericht</title>",
    );
    out.push_str("<style>body{font-family:system-ui,sans-serif;max-width:52rem;margin:2rem auto;padding:0 1rem}table{border-collapse:collapse;width:100%}td,th{border:1px solid #ccc;padding:.4rem .6rem;text-align:left}</style>");
    out.push_str("</head><body>");
    let _ = write!(
        out,
        "<h1>Klassenübersicht: {}</h1>",
        html_escape(&overview.class_name)
    );
    let _ = write!(
        out,
        "<p>Mitglieder: {} · Aufgaben: {}</p>",
        overview.members,
        overview.assignments.len()
    );
    out.push_str(
        "<table><tr><th>Aufgabe</th><th>Abgegeben</th><th>Gesehen</th><th>Fällig</th></tr>",
    );
    for a in &overview.assignments {
        let due = a.due_unix.map(unix_to_date).unwrap_or_else(|| "—".into());
        let _ = write!(
            out,
            "<tr><td>{}</td><td>{}/{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&a.title),
            a.submitted,
            a.total_members,
            a.reviewed,
            html_escape(&due)
        );
    }
    out.push_str("</table></body></html>");
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Writes a progress CSV into the workspace reports dir and returns path.
pub fn export_progress_csv(
    workspace: &Workspace,
    rows: &[ProgressEntry],
    assignment_title: &str,
    filename: &str,
) -> EduResult<std::path::PathBuf> {
    let dir = workspace.reports_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(filename);
    std::fs::write(&path, progress_csv(rows, assignment_title))?;
    Ok(path)
}

/// Convenience: build rows for an assignment then export.
pub fn export_assignment_report(
    workspace: &Workspace,
    class: &crate::classroom::Classroom,
    assignment: &crate::assignment::Assignment,
    filename: &str,
) -> EduResult<std::path::PathBuf> {
    let rows = assignment_progress(workspace, class, assignment)?;
    export_progress_csv(workspace, &rows, &assignment.title, filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::ProgressStatus;

    #[test]
    fn csv_escapes_commas_and_quotes() {
        let row = ProgressEntry {
            member_id: uuid::Uuid::nil(),
            member_name: "Anna, \"A.B.\"".into(),
            group_name: Some("G1".into()),
            submission_state: None,
            revision: 0,
            submitted_unix: None,
            late: false,
            feedback_comments: 0,
            unacknowledged: 0,
            rubric_marks: Default::default(),
            status: ProgressStatus::NotStarted,
        };
        let csv = progress_csv(&[row], "Test, Aufgabe");
        assert!(csv.contains("\"Anna, \"\"A.B.\"\"\""));
        assert!(csv.contains("\"Test, Aufgabe\""));
    }

    #[test]
    fn html_escapes() {
        assert_eq!(html_escape("<b>"), "&lt;b&gt;");
    }

    #[test]
    fn date_conversion() {
        assert_eq!(unix_to_date(0), "1970-01-01");
        // 2024-02-29 12:00 UTC
        assert_eq!(unix_to_date(1_709_208_000), "2024-02-29");
    }
}
