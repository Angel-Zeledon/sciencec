//! Registry of source files and translation from byte offsets to positions.
//!
//! Every phase of the compiler produces spans as byte ranges (see `Span` in
//! `lib.rs`). Turning those into something a person can read — a path, a line
//! and a column — happens here, once, at render time.

use crate::{FileId, Span};

/// A human-readable position. Both `line` and `col` start at **1**.
///
/// `col` counts **characters**, not bytes: the source is UTF-8 and an accent
/// must not push the caret out of place.
///
/// Known limitation: a character is counted as one column even when the
/// terminal draws it two cells wide (CJK, most emoji) or zero cells wide
/// (combining marks, zero-width joiners). Fixing that needs a width table
/// (`unicode-width`), and it would also have to agree with whatever the user's
/// editor does. It is deliberately left alone: a caret that is off under a CJK
/// string is a cosmetic problem, a caret on the wrong line is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}

/// One registered file plus the index that makes lookups cheap.
#[derive(Debug)]
struct SourceFile {
    path: String,
    text: String,
    /// Byte offset where each line starts, ascending.
    ///
    /// Always holds at least one element (`0`), even for an empty file, so
    /// every offset lands on some line. A file ending in `\n` gets a final
    /// entry equal to the text length: the empty last line, which is where a
    /// span pointing at the end of the file belongs.
    line_starts: Vec<u32>,
}

/// All the source text of one compilation session.
///
/// Files are added once and never modified, so the line index computed in
/// [`SourceMap::add_file`] stays valid for the lifetime of the map.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a file and returns its identifier.
    ///
    /// The line index is computed here, once, instead of on every lookup:
    /// scanning the text per query would make rendering quadratic, and the
    /// renderer queries a lot.
    pub fn add_file(&mut self, path: String, text: String) -> FileId {
        let id = FileId(self.files.len() as u32);
        let line_starts = compute_line_starts(&text);
        self.files.push(SourceFile { path, text, line_starts });
        id
    }

    /// How many files are registered.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Path of a file, as it was registered.
    ///
    /// # Panics
    /// If the `FileId` does not come from this map. A map hands out every id it
    /// accepts, so an unknown one is a bug in the caller, not bad input.
    pub fn path(&self, file: FileId) -> &str {
        &self.file(file).path
    }

    /// Full source text of a file.
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn text(&self, file: FileId) -> &str {
        &self.file(file).text
    }

    /// Number of lines in a file. An empty file has one (empty) line.
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn line_count(&self, file: FileId) -> u32 {
        self.file(file).line_starts.len() as u32
    }

    /// Text of a 1-based line, without its line terminator.
    ///
    /// Returns `""` when the line does not exist, so the renderer can print a
    /// context line without checking bounds first.
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn line_text(&self, file: FileId, line: u32) -> &str {
        let f = self.file(file);
        if line == 0 {
            return "";
        }
        let index = (line - 1) as usize;
        let Some(&start) = f.line_starts.get(index) else {
            return "";
        };
        let end = f
            .line_starts
            .get(index + 1)
            .map_or(f.text.len(), |&next| next as usize);
        let raw = &f.text[start as usize..end];
        // CRLF: strip the `\n` first, then the `\r` it left behind.
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        raw.strip_suffix('\r').unwrap_or(raw)
    }

    /// Position of the **start** of a span.
    ///
    /// This is the signature every other crate already calls; the end is
    /// available through [`SourceMap::line_col_end`].
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn line_col(&self, span: Span) -> LineCol {
        self.position(span.file, span.start)
    }

    /// Position of the **end** of a span (exclusive, so it points just past the
    /// last character).
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn line_col_end(&self, span: Span) -> LineCol {
        self.position(span.file, span.end)
    }

    /// The source text a span covers.
    ///
    /// Out-of-range or mid-character ends are clamped rather than rejected: a
    /// diagnostic with a slightly wrong span should still print something.
    ///
    /// # Panics
    /// If the `FileId` does not come from this map.
    pub fn span_text(&self, span: Span) -> &str {
        let f = self.file(span.file);
        let start = clamp_to_boundary(&f.text, span.start);
        let end = clamp_to_boundary(&f.text, span.end).max(start);
        &f.text[start..end]
    }

    /// Translates a byte offset into a 1-based line and column.
    ///
    /// The line comes from a binary search over the precomputed line starts;
    /// only the characters of the one line before the offset are counted.
    fn position(&self, file: FileId, offset: u32) -> LineCol {
        let f = self.file(file);
        let offset = clamp_to_boundary(&f.text, offset);
        // `partition_point` gives the first index whose start is *past* the
        // offset, so the line containing it is the one before.
        let index = f.line_starts.partition_point(|&s| s as usize <= offset) - 1;
        let start = f.line_starts[index] as usize;
        let col = f.text[start..offset].chars().count() as u32 + 1;
        LineCol { line: index as u32 + 1, col }
    }

    fn file(&self, file: FileId) -> &SourceFile {
        self.files
            .get(file.0 as usize)
            .unwrap_or_else(|| panic!("FileId({}) does not belong to this SourceMap", file.0))
    }
}

/// Byte offset where each line starts. Always begins with `0`.
fn compute_line_starts(text: &str) -> Vec<u32> {
    let mut starts = Vec::with_capacity(text.len() / 32 + 1);
    starts.push(0);
    for (i, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(i as u32 + 1);
        }
    }
    starts
}

/// Clamps an offset into `text` and walks it back to a character boundary.
///
/// A span that lands mid-character means some phase built it wrong, but the
/// renderer is the worst place to find that out, so we round down instead of
/// panicking while trying to report another error.
fn clamp_to_boundary(text: &str, offset: u32) -> usize {
    let mut offset = (offset as usize).min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}
