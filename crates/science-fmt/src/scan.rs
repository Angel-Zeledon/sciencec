//! What the lexer throws away, recovered from the source text.
//!
//! The formatter needs three things the token stream does not carry:
//!
//! * **comments**, which `Lexer::skip_comment` consumes without emitting a
//!   token — there is no `TokenKind::Comment`, so a formatter that worked from
//!   the token stream alone would delete every comment in the file;
//! * **logical lines**, which are what the `Newline` / `Indent` / `Dedent`
//!   tokens delimit, and which are the unit the layout works on;
//! * **physical lines**, because whether a comment sits at the end of a line or
//!   on a line of its own is the only thing that says where it goes back.
//!
//! All three are derived here, and nowhere else.

use science_lexer::{Token, TokenKind};

/// One `#` comment, from the hash to the end of its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// Byte offset of the `#`.
    pub start: usize,
    /// Byte offset one past the last non-whitespace character. Trailing
    /// whitespace is dropped here rather than in the renderer, so that a
    /// comment's text is the same however it was spelled.
    pub end: usize,
    /// The column of the `#`, counted in characters from the start of the
    /// physical line.
    pub column: usize,
}

/// Every comment in `source`, in source order.
///
/// A `#` inside a string or character literal is not a comment, and the token
/// stream is what says where those literals are: the lexer has already decided
/// the question, so re-deciding it here with a second scanner would be a second
/// place for the answer to be wrong.
pub fn comments(source: &str, tokens: &[Token]) -> Vec<Comment> {
    let mut literals: Vec<(usize, usize)> = tokens
        .iter()
        // `FStrText` joins the two, and it has to: a `#` inside the literal
        // half of an `f"…"` is text, exactly as a `#` inside a `"…"` is, and
        // the lexer has already decided which bytes those are. A `#` inside a
        // *hole* is not masked and should not be — it is `SC0175`, and the
        // lexer reports it.
        .filter(|t| {
            matches!(t.kind, TokenKind::Str(_) | TokenKind::Char(_) | TokenKind::FStrText(_))
        })
        .map(|t| (t.span.start as usize, t.span.end as usize))
        .collect();
    literals.sort_unstable();

    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut next_literal = 0usize;
    let mut line_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        // Advance past any literal that ended before the cursor.
        while next_literal < literals.len() && literals[next_literal].1 <= i {
            next_literal += 1;
        }
        if let Some(&(start, end)) = literals.get(next_literal) {
            if i >= start {
                // Inside a literal: skip it whole, counting the lines it spans.
                // A literal cannot span lines today, but counting is cheaper
                // than relying on that.
                while i < end {
                    if bytes[i] == b'\n' {
                        line_start = i + 1;
                    }
                    i += 1;
                }
                continue;
            }
        }
        match bytes[i] {
            b'\n' => {
                line_start = i + 1;
                i += 1;
            }
            b'#' => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                let mut end = i;
                while end > start && matches!(bytes[end - 1], b' ' | b'\t' | b'\r') {
                    end -= 1;
                }
                out.push(Comment {
                    start,
                    end,
                    column: source[line_start..start].chars().count(),
                });
            }
            _ => i += 1,
        }
    }
    out
}

/// The byte offset at which each physical line begins.
pub fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in source.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// The zero-based physical line holding `pos`.
pub fn line_of(starts: &[usize], pos: usize) -> usize {
    match starts.binary_search(&pos) {
        Ok(i) => i,
        Err(i) => i - 1,
    }
}

/// One logical line: the tokens between two `Newline`s, and the block depth
/// they sit at.
///
/// `start..end` indexes the token slice and holds content tokens only —
/// `Indent`, `Dedent`, `Newline` and `Eof` are structure, not text, and the
/// formatter re-derives all four from `depth` rather than printing them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalLine {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
}

/// Splits the token stream into logical lines.
///
/// The depth of a line is the number of open `Indent`s when its *first* token
/// appeared, which is what makes the output's block structure identical to the
/// input's by construction: the formatter prints `depth * INDENT` spaces and
/// the lexer's indentation stack replays the same pushes and pops.
pub fn logical_lines(tokens: &[Token]) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut line_depth = 0usize;

    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Indent => depth += 1,
            TokenKind::Dedent => depth = depth.saturating_sub(1),
            TokenKind::Newline | TokenKind::Eof => {
                if let Some(s) = start.take() {
                    lines.push(LogicalLine { start: s, end: i, depth: line_depth });
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(i);
                    line_depth = depth;
                }
            }
        }
    }
    if let Some(s) = start {
        lines.push(LogicalLine { start: s, end: tokens.len(), depth: line_depth });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use science_diagnostics::FileId;

    fn lex(source: &str) -> Vec<Token> {
        science_lexer::lex(FileId(0), source).0
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        let source = "let a be \"# not a comment\"   # but this is\n";
        let found = comments(source, &lex(source));
        assert_eq!(found.len(), 1);
        assert_eq!(&source[found[0].start..found[0].end], "# but this is");
    }

    #[test]
    fn a_comment_loses_its_trailing_whitespace_but_not_its_hashes() {
        let source = "#### four hashes   \n";
        let found = comments(source, &lex(source));
        assert_eq!(&source[found[0].start..found[0].end], "#### four hashes");
    }

    #[test]
    fn a_comment_records_the_column_of_its_hash() {
        let source = "def f():\n        # deep\n    1\n";
        let found = comments(source, &lex(source));
        assert_eq!(found[0].column, 8);
    }

    #[test]
    fn a_broken_chain_is_one_logical_line() {
        let source = "def f():\n    a\n        .b()\n        .c()\n";
        let lines = logical_lines(&lex(source));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].depth, 0);
        assert_eq!(lines[1].depth, 1);
    }

    #[test]
    fn depth_is_the_depth_the_line_opened_at() {
        let source = "def f():\n    if a:\n        b\n    c\n";
        let lines = logical_lines(&lex(source));
        let depths: Vec<usize> = lines.iter().map(|l| l.depth).collect();
        assert_eq!(depths, vec![0, 1, 2, 1]);
    }
}
