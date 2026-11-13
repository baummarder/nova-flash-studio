//! Undo/redo history based on project snapshots.
//!
//! The project model is compact and cloneable, so storing complete snapshots
//! is both simpler and safer than applying inverse commands to every part of
//! the document. A snapshot is recorded only when the project actually changed.
//! Continuous gestures (dragging a slider, typing in a field, dragging on the
//! stage) are grouped by the application update loop into one transaction.

use nova_core::Project;

#[derive(Debug, Clone)]
struct Edit {
    label: String,
    before: Project,
    after: Project,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Records one transaction if `before` and `after` differ.
    ///
    /// Returns `true` when an entry was added. Any redo entries are discarded,
    /// which is the expected behaviour after a new edit.
    pub fn record(&mut self, label: impl Into<String>, before: Project, after: &Project) -> bool {
        if before == *after {
            return false;
        }
        self.undo.push(Edit {
            label: label.into(),
            before,
            after: after.clone(),
        });
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|edit| edit.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|edit| edit.label.as_str())
    }

    /// Restores the previous snapshot and returns its label.
    pub fn undo(&mut self, current: &Project) -> Option<(String, Project)> {
        let edit = self.undo.pop()?;
        self.redo.push(Edit {
            label: edit.label.clone(),
            before: edit.before.clone(),
            after: current.clone(),
        });
        Some((edit.label, edit.before))
    }

    /// Re-applies the next snapshot and returns its label.
    pub fn redo(&mut self, current: &Project) -> Option<(String, Project)> {
        let edit = self.redo.pop()?;
        self.undo.push(Edit {
            label: edit.label.clone(),
            before: current.clone(),
            after: edit.after.clone(),
        });
        Some((edit.label, edit.after))
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nova_core::{Metadata, Project};

    fn project(name: &str) -> Project {
        let mut project = Project::new_empty(name);
        project.metadata = Metadata::new(name);
        project
    }

    #[test]
    fn records_only_real_changes() {
        let mut history = History::new(16);
        let before = project("A");
        let same = before.clone();
        assert!(!history.record("No-op", before, &same));
        assert!(!history.can_undo());
    }

    #[test]
    fn undo_and_redo_restore_snapshots() {
        let mut history = History::new(16);
        let mut project = project("A");
        let before = project.clone();
        project.metadata.name = "B".into();
        assert!(history.record("Umbenennen", before, &project));

        let (label, restored) = history.undo(&project).expect("undo");
        assert_eq!(label, "Umbenennen");
        assert_eq!(restored.metadata.name, "A");
        assert!(history.can_redo());

        let (_, redone) = history.redo(&restored).expect("redo");
        assert_eq!(redone.metadata.name, "B");
        assert!(!history.can_redo());
    }

    #[test]
    fn new_edit_discards_redo() {
        let mut history = History::new(16);
        let project_a = project("A");
        let mut project_b = project_a.clone();
        project_b.metadata.name = "B".into();
        history.record("B", project_a.clone(), &project_b);
        history.undo(&project_b).unwrap();

        let mut project_c = project_a.clone();
        project_c.metadata.name = "C".into();
        history.record("C", project_a, &project_c);
        assert!(!history.can_redo());
        assert_eq!(history.undo_label(), Some("C"));
    }

    #[test]
    fn limit_drops_oldest_entries() {
        let mut history = History::new(2);
        let mut current = project("A");
        for name in ["B", "C", "D"] {
            let before = current.clone();
            current.metadata.name = name.into();
            history.record(name, before, &current);
        }
        assert_eq!(history.undo_len(), 2);
        let (label, _) = history.undo(&current).unwrap();
        assert_eq!(label, "D");
        let (label, _) = history.undo(&current).unwrap();
        assert_eq!(label, "C");
        assert!(!history.can_undo());
    }
}
