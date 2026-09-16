//! Rendering a `Diagnostic` as text, in the style of rustc.
//!
//! ```text
//! error[LK0301]: use of a moved value
//!   --> src/main.link:12:11
//!    |
//! 10 |     let b = d
//!    |             - value moved here
//! 11 |
//! 12 |     print(d)
//!    |           ^ used here after the move
//!    |
//!    = note: `Doc` does not implement `Copy`, so the assignment moves
//!    = help: clone the value if you need to keep the original: `d.clone()`
//! ```
//!
//! No color. UI tests (§10 of the design spec) compare this output literally,
//! so it has to stay byte-stable; escape codes will be added later as a
//! separate layer that wraps these same pieces.

use std::collections::BTreeMap;

use crate::source_map::{LineCol, SourceMap};
use crate::{Diagnostic, Diagnostics, FileId, Label, Severity, Span};

/// Unannotated lines printed between two annotated ones. Beyond this the gap
/// collapses into `...`, the way rustc does it: a couple of lines of context
/// help, twenty lines of unrelated code do not.
const MAX_CONTEXT_GAP: u32 = 1;

/// Renders one diagnostic. The result has no trailing newline.
pub fn render(map: &SourceMap, d: &Diagnostic) -> String {
    let mut out = String::new();
    out.push_str(&format!("{}[{}]: {}", severity_word(d.severity), d.code, d.message));

    let groups = group_by_file(map, d);

    // Gutter width is driven by the widest line number actually printed, so a
    // diagnostic confined to line 9 does not pay for a four-digit gutter.
    let widest = groups
        .iter()
        .flat_map(|g| g.lines.keys())
        .copied()
        .max()
        .unwrap_or(1);
    let width = decimal_width(widest);
    let bar = format!("{:width$} |", "", width = width);
    let eq = format!("{:width$} =", "", width = width);

    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            // A blank gutter line keeps a second file's header from reading as
            // a continuation of the snippet above it.
            out.push('\n');
            out.push_str(&bar);
        }
        render_group(map, group, width, &bar, &mut out);
    }

    if !d.notes.is_empty() || !d.suggestions.is_empty() {
        // The `|` separator only makes sense if there was a snippet above it.
        if !groups.is_empty() {
            out.push('\n');
            out.push_str(&bar);
        }
        for note in &d.notes {
            out.push('\n');
            out.push_str(&format!("{eq} note: {note}"));
        }
        for s in &d.suggestions {
            out.push('\n');
            // The replacement is part of the message: a fix the reader cannot
            // see is a fix they cannot check.
            if !s.replacement.is_empty() {
                out.push_str(&format!("{eq} help: {}: `{}`", s.message, s.replacement));
            } else {
                // An empty replacement is a deletion. Quoting the empty string
                // would read as "replace this with nothing visible", so we show
                // what disappears instead.
                let removed = map.span_text(s.span);
                if removed.is_empty() {
                    out.push_str(&format!("{eq} help: {}", s.message));
                } else {
                    out.push_str(&format!("{eq} help: {}: delete `{removed}`", s.message));
                }
            }
        }
    }

    out
}

/// Renders several diagnostics, separated by a blank line and ordered by file
/// and position so that the output of a run is stable whatever order the phases
/// pushed them in.
pub fn render_all(map: &SourceMap, diags: &Diagnostics) -> String {
    let mut ordered: Vec<&Diagnostic> = diags.iter().collect();
    // Stable sort: two diagnostics on the same span keep the order the phase
    // emitted them in, which is usually cause before consequence.
    ordered.sort_by_key(|d| sort_key(d));

    let mut out = String::new();
    for (i, d) in ordered.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&render(map, d));
    }
    out
}

/// Position a diagnostic sorts at: its primary span, or the first label it has,
/// or the very end if it has no labels at all.
fn sort_key(d: &Diagnostic) -> (u32, u32, u32) {
    let span = d.primary_span().or_else(|| d.labels.first().map(|l| l.span));
    match span {
        Some(s) => (s.file.0, s.start, s.end),
        None => (u32::MAX, u32::MAX, u32::MAX),
    }
}

/// A label with its span already translated to line/column.
struct Annotation<'a> {
    label: &'a Label,
    start: LineCol,
    end: LineCol,
}

/// All annotations belonging to one file, indexed by the line they start on.
struct FileGroup<'a> {
    file: FileId,
    /// Where the `-->` header points: the first primary label of this group,
    /// or its first label if the group has none.
    anchor: LineCol,
    /// Whether `anchor` already came from a primary label.
    anchored_by_primary: bool,
    lines: BTreeMap<u32, Vec<Annotation<'a>>>,
}

/// Splits the labels per file.
///
/// A diagnostic can point at more than one file (a call site here, the
/// signature it violates over there). The file of the primary label comes
/// first; the rest follow in the order their labels appear. Unlike rustc we
/// give every group a full `-->` header instead of the `:::` continuation
/// marker — one rule is easier to read and easier to keep stable.
fn group_by_file<'a>(map: &SourceMap, d: &'a Diagnostic) -> Vec<FileGroup<'a>> {
    let mut groups: Vec<FileGroup<'a>> = Vec::new();

    let primary_file = d.primary_span().map(|s| s.file);
    let mut ordered: Vec<&'a Label> = d.labels.iter().collect();
    if let Some(pf) = primary_file {
        // Stable partition: the primary file first, everything else untouched.
        ordered.sort_by_key(|l| l.span.file != pf);
    }

    for label in ordered {
        let start = map.line_col(label.span);
        let end = map.line_col_end(label.span);
        let index = match groups.iter().position(|g| g.file == label.span.file) {
            Some(i) => i,
            None => {
                groups.push(FileGroup {
                    file: label.span.file,
                    anchor: start,
                    anchored_by_primary: label.primary,
                    lines: BTreeMap::new(),
                });
                groups.len() - 1
            }
        };
        if label.primary && !groups[index].anchored_by_primary {
            groups[index].anchor = start;
            groups[index].anchored_by_primary = true;
        }
        groups[index]
            .lines
            .entry(start.line)
            .or_default()
            .push(Annotation { label, start, end });
    }

    for group in &mut groups {
        for annotations in group.lines.values_mut() {
            // Left to right, so the markers under a line read in source order.
            annotations.sort_by_key(|a| a.start.col);
        }
    }

    groups
}

fn render_group(
    map: &SourceMap,
    group: &FileGroup<'_>,
    width: usize,
    bar: &str,
    out: &mut String,
) {
    out.push('\n');
    out.push_str(&format!(
        "{:width$}--> {}:{}:{}",
        "",
        map.path(group.file),
        group.anchor.line,
        group.anchor.col,
        width = width
    ));
    out.push('\n');
    out.push_str(bar);

    let mut previous: Option<u32> = None;
    for (&line, annotations) in &group.lines {
        match previous {
            Some(prev) if line > prev + 1 => {
                let gap = line - prev - 1;
                if gap <= MAX_CONTEXT_GAP {
                    for n in prev + 1..line {
                        push_source_line(map, group.file, n, width, out);
                    }
                } else {
                    out.push('\n');
                    out.push_str("...");
                }
            }
            _ => {}
        }
        previous = Some(line);

        push_source_line(map, group.file, line, width, out);
        for annotation in annotations {
            push_marker_line(map, group.file, annotation, bar, out);
        }
    }
}

fn push_source_line(map: &SourceMap, file: FileId, line: u32, width: usize, out: &mut String) {
    let text = map.line_text(file, line);
    out.push('\n');
    let rendered = format!("{line:>width$} | {text}", width = width);
    out.push_str(rendered.trim_end());
}

/// One marker row: the carets (or dashes) plus the label message.
///
/// Every label gets its own row, even when several share a line. rustc packs
/// them onto one row joined by vertical bars; that drawing is hard to get right
/// and harder to read once the spans overlap, so each label is simply listed
/// under the shared source line, left to right.
fn push_marker_line(
    map: &SourceMap,
    file: FileId,
    annotation: &Annotation<'_>,
    bar: &str,
    out: &mut String,
) {
    let marker = if annotation.label.primary { '^' } else { '-' };
    let span = annotation.label.span;
    let count = marker_count(map, file, annotation, span);

    let mut message = annotation.label.message.clone();
    if annotation.end.line > annotation.start.line {
        // Multiline spans: rustc draws a vertical bar down the gutter from the
        // opening line to the closing one. We only mark the first line and say
        // in the label where the span ends. The bar drawing is a lot of
        // machinery for something a sentence conveys, and it interacts badly
        // with the `...` elision above.
        message.push_str(&format!(" (continues to line {})", annotation.end.line));
    }

    out.push('\n');
    let indent = " ".repeat((annotation.start.col - 1) as usize);
    let markers: String = marker.to_string().repeat(count);
    let rendered = if message.is_empty() {
        format!("{bar} {indent}{markers}")
    } else {
        format!("{bar} {indent}{markers} {message}")
    };
    out.push_str(rendered.trim_end());
}

/// How many marker characters a label needs on its first line.
fn marker_count(map: &SourceMap, file: FileId, annotation: &Annotation<'_>, span: Span) -> usize {
    if span.is_empty() {
        // An empty span means "something is missing right here"; a single caret
        // points at the gap between two characters.
        return 1;
    }
    let chars = if annotation.end.line > annotation.start.line {
        // Underline to the end of the first line only.
        let line_len = map.line_text(file, annotation.start.line).chars().count() as u32;
        line_len.saturating_sub(annotation.start.col - 1)
    } else {
        annotation.end.col - annotation.start.col
    };
    (chars as usize).max(1)
}

fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
    }
}

fn decimal_width(n: u32) -> usize {
    let mut width = 1;
    let mut n = n;
    while n >= 10 {
        n /= 10;
        width += 1;
    }
    width
}
