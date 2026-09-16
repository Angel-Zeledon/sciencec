//! Spans, diagnostics, and how they are rendered.
//!
//! Every other compiler phase depends on this crate. The types in this module
//! are the shared contract: changing them ripples through the whole pipeline,
//! so change them deliberately.

use std::fmt;

/// Identifies a source file within a compilation session.
///
/// Deliberately opaque: only `SourceMap` knows how to turn one back into a
/// path and its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(pub u32);

/// A byte range within a file.
///
/// Offsets are in **bytes**, not characters: source is UTF-8, and translating
/// to line and column is the `SourceMap`'s job at render time rather than the
/// job of every phase that produces a span.
///
/// Invariant: `start <= end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "inverted span: {start} > {end}");
        Span { file, start, end }
    }

    /// An empty span at a position. Useful for pointing at something missing.
    pub fn at(file: FileId, pos: u32) -> Self {
        Span { file, start: pos, end: pos }
    }

    /// The smallest span containing both.
    ///
    /// # Panics
    /// If the spans belong to different files.
    pub fn merge(self, other: Span) -> Span {
        assert_eq!(self.file, other.file, "cannot merge spans from different files");
        Span {
            file: self.file,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// A value paired with where it came from in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

impl<T> Spanned<T> {
    pub fn new(node: T, span: Span) -> Self {
        Spanned { node, span }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned { node: f(self.node), span: self.span }
    }
}

/// A stable error code of the form `SC0142`.
///
/// Ranges are divided by phase; see §9 of the design spec:
/// 0001-0099 lexical, 0100-0199 syntax, 0200-0299 resolution and types,
/// 0300-0399 ownership and regions, 0400-0499 codegen and linking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Code(pub u16);

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SC{:04}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// A span annotated within a diagnostic.
///
/// The primary label is where the problem is; secondary labels are the context
/// that explains it. For region errors (§6.3 of the spec) the chain of
/// secondary labels *is* the explanation, so they are not decoration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
    pub primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Label { span, message: message.into(), primary: true }
    }

    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Label { span, message: message.into(), primary: false }
    }
}

/// A fix a tool can apply without human intervention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub span: Span,
    /// Text to substitute for the contents of `span`.
    pub replacement: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    pub labels: Vec<Label>,
    /// Footnotes, with no span attached.
    pub notes: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    pub fn error(code: Code, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn warning(code: Code, message: impl Into<String>) -> Self {
        Diagnostic { severity: Severity::Warning, ..Self::error(code, message) }
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// The *first* primary span, if any.
    ///
    /// The order of `labels` is load-bearing: this is what orders diagnostics
    /// against each other, and the renderer anchors its `-->` header on the
    /// same label. Pushing labels in a different order changes output.
    pub fn primary_span(&self) -> Option<Span> {
        self.labels.iter().find(|l| l.primary).map(|l| l.span)
    }
}

/// Collects the diagnostics produced by a phase.
///
/// Phases do not abort on the first error: they gather everything they can and
/// keep going, so one compilation reports several problems at once.
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    pub fn extend(&mut self, other: Diagnostics) {
        self.items.extend(other.items);
    }

    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    pub fn into_vec(self) -> Vec<Diagnostic> {
        self.items
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

pub mod render;
pub mod source_map;

pub use render::{render, render_all};
pub use source_map::{LineCol, SourceMap};
