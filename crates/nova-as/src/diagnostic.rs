use crate::token::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub severity: Severity,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            severity: Severity::Error,
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            severity: Severity::Warning,
        }
    }

    /// Renders a compiler-style message with the offending source line.
    pub fn render(&self, source: &str, file: &str) -> String {
        let line_text = source
            .lines()
            .nth(self.span.line.saturating_sub(1))
            .unwrap_or("");
        let expanded: String = line_text
            .chars()
            .map(|c| if c == '\t' { ' ' } else { c })
            .take(self.span.column.saturating_sub(1))
            .collect();
        let caret_len = self.span.len().clamp(1, 40).min(
            line_text
                .chars()
                .count()
                .saturating_sub(self.span.column - 1)
                .max(1),
        );
        let label = match self.severity {
            Severity::Error => "Fehler",
            Severity::Warning => "Warnung",
        };
        format!(
            "{file}:{}:{}: {label}: {}\n  {line_text}\n  {}{}",
            self.span.line,
            self.span.column,
            self.message,
            expanded,
            "^".repeat(caret_len)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_caret() {
        let source = "var x = ;\n";
        let diagnostic = Diagnostic::error("Ausdruck erwartet", Span::new(8, 9, 1, 9));
        let text = diagnostic.render(source, "Test.as");
        assert!(text.contains("Test.as:1:9"));
        assert!(text.contains("var x = ;"));
        assert!(text.contains('^'));
    }
}
