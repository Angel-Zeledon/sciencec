//! A query result paired with the diagnostics its phase produced.

use science_diagnostics::{Diagnostic, Diagnostics, Severity};

/// What a phase produced, together with what it complained about.
///
/// Every phase of Science recovers from errors instead of aborting (see the
/// `science-lexer` and `science-parser` crate docs), so "the result" and "the
/// diagnostics" are not alternatives: a query returns both, always. Making
/// the diagnostics part of the memoized value rather than a side channel is
/// deliberate:
///
/// * they are invalidated and recomputed with the value they belong to, so a
///   stale error can never outlive the code that caused it;
/// * they take part in the equality check that drives salsa's backdating, so
///   a change that alters only an error message still propagates.
///
/// Salsa does offer accumulators for exactly this job. They are not used here
/// because an accumulated value does *not* participate in the query's result
/// equality: adding or removing a diagnostic would not, by itself, count as a
/// change. For diagnostics that is the wrong default.
#[derive(Debug, Clone, PartialEq)]
pub struct WithDiagnostics<T> {
    value: T,
    /// A `Vec<Diagnostic>` and not a [`Diagnostics`]: salsa compares the old
    /// and new result of a query with [`PartialEq`] to decide whether the
    /// change needs to propagate, and `Diagnostics` does not implement it.
    diagnostics: Vec<Diagnostic>,
}

impl<T> WithDiagnostics<T> {
    pub fn new(value: T, diagnostics: Diagnostics) -> Self {
        WithDiagnostics { value, diagnostics: diagnostics.into_vec() }
    }

    /// A result nothing was reported about.
    pub fn clean(value: T) -> Self {
        WithDiagnostics { value, diagnostics: Vec::new() }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the phase reported anything of severity `Error`.
    ///
    /// A phase that reported errors still produced a value: it is a recovered
    /// one, fit for reporting further problems but not for code generation.
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn into_parts(self) -> (T, Vec<Diagnostic>) {
        (self.value, self.diagnostics)
    }
}
