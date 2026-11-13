//! Line-oriented diff preview used before an agent change is applied.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPreview {
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<DiffLine>,
}

impl DiffPreview {
    pub fn summary(&self) -> String {
        format!("+{} / -{} Zeilen", self.added, self.removed)
    }
}

pub fn diff_text(old: &str, new: &str) -> DiffPreview {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // LCS is quadratic. Source files in this editor are small; for very large
    // files fall back to a clearly labelled whole-file replacement instead of
    // allocating an unbounded table.
    if old_lines.len().saturating_mul(new_lines.len()) > 2_000_000 {
        return whole_file_diff(&old_lines, &new_lines);
    }

    let rows = old_lines.len() + 1;
    let columns = new_lines.len() + 1;
    let mut table = vec![0u32; rows * columns];
    for i in (0..old_lines.len()).rev() {
        for j in (0..new_lines.len()).rev() {
            let value = if old_lines[i] == new_lines[j] {
                table[(i + 1) * columns + j + 1] + 1
            } else {
                table[(i + 1) * columns + j].max(table[i * columns + j + 1])
            };
            table[i * columns + j] = value;
        }
    }

    let mut lines = Vec::new();
    let mut added = 0;
    let mut removed = 0;
    let (mut i, mut j) = (0, 0);
    while i < old_lines.len() && j < new_lines.len() {
        if old_lines[i] == new_lines[j] {
            lines.push(DiffLine {
                kind: DiffKind::Context,
                old_line: Some(i + 1),
                new_line: Some(j + 1),
                text: old_lines[i].to_string(),
            });
            i += 1;
            j += 1;
        } else if table[(i + 1) * columns + j] >= table[i * columns + j + 1] {
            lines.push(DiffLine {
                kind: DiffKind::Removed,
                old_line: Some(i + 1),
                new_line: None,
                text: old_lines[i].to_string(),
            });
            removed += 1;
            i += 1;
        } else {
            lines.push(DiffLine {
                kind: DiffKind::Added,
                old_line: None,
                new_line: Some(j + 1),
                text: new_lines[j].to_string(),
            });
            added += 1;
            j += 1;
        }
    }
    while i < old_lines.len() {
        lines.push(DiffLine {
            kind: DiffKind::Removed,
            old_line: Some(i + 1),
            new_line: None,
            text: old_lines[i].to_string(),
        });
        removed += 1;
        i += 1;
    }
    while j < new_lines.len() {
        lines.push(DiffLine {
            kind: DiffKind::Added,
            old_line: None,
            new_line: Some(j + 1),
            text: new_lines[j].to_string(),
        });
        added += 1;
        j += 1;
    }
    DiffPreview {
        added,
        removed,
        lines,
    }
}

fn whole_file_diff(old: &[&str], new: &[&str]) -> DiffPreview {
    let mut lines = Vec::with_capacity(old.len() + new.len());
    for (index, line) in old.iter().enumerate() {
        lines.push(DiffLine {
            kind: DiffKind::Removed,
            old_line: Some(index + 1),
            new_line: None,
            text: (*line).to_string(),
        });
    }
    for (index, line) in new.iter().enumerate() {
        lines.push(DiffLine {
            kind: DiffKind::Added,
            old_line: None,
            new_line: Some(index + 1),
            text: (*line).to_string(),
        });
    }
    DiffPreview {
        added: new.len(),
        removed: old.len(),
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_marks_added_and_removed_lines() {
        let preview = diff_text("a\nb\nc\n", "a\nB\nc\nd\n");
        assert!(preview.added >= 2);
        assert!(preview.removed >= 1);
        assert!(
            preview
                .lines
                .iter()
                .any(|line| line.kind == DiffKind::Added)
        );
        assert!(
            preview
                .lines
                .iter()
                .any(|line| line.kind == DiffKind::Removed)
        );
    }

    #[test]
    fn identical_text_has_no_changes() {
        let preview = diff_text("same\n", "same\n");
        assert_eq!(preview.added, 0);
        assert_eq!(preview.removed, 0);
    }
}
