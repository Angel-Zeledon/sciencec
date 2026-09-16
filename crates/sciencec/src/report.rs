//! Counting what was reported, and saying so.
//!
//! The driver never formats a diagnostic: `science-diagnostics::render_all`
//! already produces the exact text the UI suite pins, and a second renderer
//! would be a second thing to keep in step. What lives here is only the
//! bookkeeping around it — the tally that decides the exit code and the one
//! summary line printed after everything else.

use science_diagnostics::{Diagnostic, Diagnostics, Severity, SourceMap};

/// How many errors and warnings a run has seen so far.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub errors: usize,
    pub warnings: usize,
}

impl Tally {
    /// Counts a batch of diagnostics. Notes are printed but not counted: they
    /// are always attached to something that already was.
    pub fn count(&mut self, diagnostics: &[Diagnostic]) {
        for d in diagnostics {
            match d.severity {
                Severity::Error => self.errors += 1,
                Severity::Warning => self.warnings += 1,
                Severity::Note => {}
            }
        }
    }

    /// Counts something that went wrong outside any source file — a file that
    /// could not be read, an argument that made no sense.
    pub fn error(&mut self) {
        self.errors += 1;
    }

    /// Whether the process should exit non-zero. Warnings alone do not fail.
    pub fn failed(self) -> bool {
        self.errors > 0
    }

    /// The trailing line, or `None` when there is nothing to say.
    ///
    /// A clean run prints nothing at all, so that `sciencec check` in a script
    /// is silent on success.
    pub fn summary(self) -> Option<String> {
        match (self.errors, self.warnings) {
            (0, 0) => None,
            (0, w) => Some(plural(w, "warning")),
            (e, 0) => Some(plural(e, "error")),
            (e, w) => Some(format!("{}, {}", plural(e, "error"), plural(w, "warning"))),
        }
    }
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// Renders a batch of diagnostics to stderr, or does nothing if it is empty.
///
/// `render_all` takes a [`Diagnostics`] rather than a slice, and the queries
/// hand back a slice, so the batch is rebuilt here. That is the whole of the
/// conversion: no diagnostic is altered, reordered or re-rendered — the sort
/// that makes the output stable is `render_all`'s own.
pub fn emit(map: &SourceMap, diagnostics: &[Diagnostic]) {
    if diagnostics.is_empty() {
        return;
    }
    let mut batch = Diagnostics::new();
    for d in diagnostics {
        batch.push(d.clone());
    }
    eprintln!("{}", science_diagnostics::render_all(map, &batch));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_run_says_nothing() {
        assert_eq!(Tally::default().summary(), None);
        assert!(!Tally::default().failed());
    }

    #[test]
    fn singular_and_plural() {
        let one = Tally { errors: 1, warnings: 0 };
        assert_eq!(one.summary().as_deref(), Some("1 error"));
        let three = Tally { errors: 3, warnings: 0 };
        assert_eq!(three.summary().as_deref(), Some("3 errors"));
        let both = Tally { errors: 3, warnings: 1 };
        assert_eq!(both.summary().as_deref(), Some("3 errors, 1 warning"));
    }

    #[test]
    fn warnings_alone_do_not_fail_but_are_still_reported() {
        let warned = Tally { errors: 0, warnings: 2 };
        assert!(!warned.failed());
        assert_eq!(warned.summary().as_deref(), Some("2 warnings"));
    }

    #[test]
    fn notes_are_not_counted() {
        use science_diagnostics::{Code, Diagnostic};
        let mut tally = Tally::default();
        let mut note = Diagnostic::error(Code(1), "x");
        note.severity = Severity::Note;
        tally.count(&[
            Diagnostic::error(Code(1), "x"),
            Diagnostic::warning(Code(2), "y"),
            note,
        ]);
        assert_eq!(tally, Tally { errors: 1, warnings: 1 });
    }
}
