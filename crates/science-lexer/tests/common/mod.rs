//! Shared helpers for the lexer test binaries.
//!
//! Each integration test file compiles this module separately, so not every
//! helper is used by every binary.
#![allow(dead_code)]

use science_diagnostics::{Diagnostics, FileId, Severity};
use science_lexer::{lex, NumSuffix, Token, TokenKind};

/// Every test lexes a single anonymous file.
pub const FILE: FileId = FileId(0);

pub fn run(source: &str) -> (Vec<Token>, Diagnostics) {
    lex(FILE, source)
}

pub fn tokens(source: &str) -> Vec<Token> {
    lex(FILE, source).0
}

pub fn kinds(source: &str) -> Vec<TokenKind> {
    lex(FILE, source).0.into_iter().map(|t| t.kind).collect()
}

/// Kinds with the structural tail (`Newline`, `Dedent`, `Eof`) stripped, so a
/// one-line fragment can be compared against just the tokens it spells out.
pub fn bare(source: &str) -> Vec<TokenKind> {
    let mut v = kinds(source);
    while matches!(
        v.last(),
        Some(TokenKind::Newline) | Some(TokenKind::Dedent) | Some(TokenKind::Eof)
    ) {
        v.pop();
    }
    v
}

/// The first token of a fragment. Handy for literal tests.
pub fn kind0(source: &str) -> TokenKind {
    kinds(source).into_iter().next().expect("the stream always has at least Eof")
}

pub fn codes(source: &str) -> Vec<String> {
    lex(FILE, source).1.iter().map(|d| d.code.to_string()).collect()
}

pub fn messages(source: &str) -> Vec<String> {
    lex(FILE, source).1.iter().map(|d| d.message.clone()).collect()
}

/// Spans of the primary label of each diagnostic, as `(start, end)`.
pub fn error_spans(source: &str) -> Vec<(u32, u32)> {
    lex(FILE, source)
        .1
        .iter()
        .filter_map(|d| d.primary_span())
        .map(|s| (s.start, s.end))
        .collect()
}

// --- shorthands for building expected token kinds ------------------------

pub fn id(name: &str) -> TokenKind {
    TokenKind::Ident(name.to_string())
}

pub fn text(value: &str) -> TokenKind {
    TokenKind::Str(value.to_string())
}

pub fn int(value: u128, base: science_lexer::IntBase, suffix: Option<NumSuffix>) -> TokenKind {
    TokenKind::Int { value, base, suffix }
}

pub fn float(value: f64, suffix: Option<NumSuffix>) -> TokenKind {
    TokenKind::Float { value, suffix }
}

// --- snapshot rendering --------------------------------------------------

/// A readable dump of a whole token stream plus its diagnostics.
///
/// The source is echoed first so a snapshot can be reviewed without opening
/// the test that produced it.
pub fn dump(source: &str) -> String {
    let (tokens, diags) = lex(FILE, source);
    let mut out = String::new();

    out.push_str("== source ==\n");
    if source.is_empty() {
        out.push_str("(empty)\n");
    } else {
        for line in source.split('\n') {
            out.push('|');
            out.push_str(&line.replace('\t', "\\t"));
            out.push('\n');
        }
    }

    out.push_str("\n== tokens ==\n");
    for t in &tokens {
        out.push_str(&format!("{:>4}..{:<4}  {:?}\n", t.span.start, t.span.end, t.kind));
    }

    out.push_str("\n== diagnostics ==\n");
    if diags.is_empty() {
        out.push_str("(none)\n");
    }
    for d in diags.iter() {
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        };
        out.push_str(&format!("{} {}: {}\n", d.code, severity, d.message));
        for label in &d.labels {
            let role = if label.primary { "primary  " } else { "secondary" };
            out.push_str(&format!(
                "    {} {}..{}: {}\n",
                role, label.span.start, label.span.end, label.message
            ));
        }
        for note in &d.notes {
            out.push_str(&format!("    note: {note}\n"));
        }
    }

    out
}
