//! Helpers shared by the parser's integration tests.
//!
//! The lexer is being written in parallel, so these tests never call it. They
//! build token streams by hand and hand them straight to the parser. That also
//! keeps the parser's tests honest: they test the parser, not the lexer.

#![allow(dead_code)]

use link_diagnostics::{Diagnostics, FileId, Span};
use link_lexer::{IntBase, NumSuffix, Token, TokenKind};
use link_parser::{ast::Module, Dump};

/// Every synthetic token stream lives in file 0.
pub const FILE: FileId = FileId(0);

/// Builds a token stream, giving each token a plausible, increasing span.
///
/// Spans are laid out back to back with no gaps: the exact numbers are
/// arbitrary, what matters is that they are deterministic and that a merged
/// span visibly covers the tokens it should.
///
/// An `Eof` is appended automatically unless the stream already ends with one.
pub fn stream(mut kinds: Vec<TokenKind>) -> Vec<Token> {
    if kinds.last() != Some(&TokenKind::Eof) {
        kinds.push(TokenKind::Eof);
    }
    let mut pos = 0u32;
    let mut out = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let width = width_of(&kind);
        out.push(Token::new(kind, Span::new(FILE, pos, pos + width)));
        pos += width;
    }
    out
}

/// How wide a token would plausibly be in real source.
///
/// The block-structure tokens are zero width because they stand for a change
/// in indentation rather than for any text.
fn width_of(kind: &TokenKind) -> u32 {
    use TokenKind::*;
    match kind {
        Newline | Indent | Dedent | Eof => 0,

        Ident(name) => name.chars().count() as u32,
        Str(value) => value.chars().count() as u32 + 2,
        Char(_) => 3,
        Int { value, .. } => value.to_string().len() as u32,
        Float { value, .. } => value.to_string().len() as u32,

        Fn | If | In | Or | As => 2,
        Let | Mut | For | Use | Mod | Pub | And | Not | Dyn => 3,
        Else | Loop | Enum | True | Impl | SelfValue | SelfType => 4,
        Match | While | Break | Trait | Where | False => 5,
        Return | Struct => 6,
        Continue => 8,
        Reserved(_) => 5,

        Arrow | FatArrow | Shl | Shr | EqEq | NotEq | LtEq | GtEq => 2,
        Underscore => 1,

        _ => 1,
    }
}

// --- short token constructors -------------------------------------------

pub fn id(name: &str) -> TokenKind {
    TokenKind::Ident(name.to_string())
}

pub fn int(value: u128) -> TokenKind {
    TokenKind::Int { value, base: IntBase::Dec, suffix: None }
}

pub fn int_suffixed(value: u128, suffix: NumSuffix) -> TokenKind {
    TokenKind::Int { value, base: IntBase::Dec, suffix: Some(suffix) }
}

pub fn text(value: &str) -> TokenKind {
    TokenKind::Str(value.to_string())
}

// --- snapshot rendering --------------------------------------------------

/// The AST dump followed by the diagnostics, which is what a snapshot records.
///
/// Both halves always appear, so a snapshot that loses its diagnostics is as
/// visible as one that gains them.
pub fn report(module: &Module, diagnostics: &Diagnostics) -> String {
    let mut out = module.dump();
    out.push_str("--- diagnostics ---\n");
    if diagnostics.is_empty() {
        out.push_str("(none)\n");
    }
    for diagnostic in diagnostics.iter() {
        let span = diagnostic
            .primary_span()
            .map(|s| format!("@{}..{}", s.start, s.end))
            .unwrap_or_else(|| "@?".to_string());
        out.push_str(&format!("{} {} {}\n", diagnostic.code, span, diagnostic.message));
        for label in diagnostic.labels.iter().filter(|l| !l.primary) {
            out.push_str(&format!(
                "  note @{}..{} {}\n",
                label.span.start, label.span.end, label.message
            ));
        }
        for note in &diagnostic.notes {
            out.push_str(&format!("  note {note}\n"));
        }
    }
    out
}

/// Parses a token stream and renders the result for a snapshot.
pub fn parse_report(kinds: Vec<TokenKind>) -> String {
    let tokens = stream(kinds);
    let (module, diagnostics) = link_parser::parse_module(&tokens, FILE);
    report(&module, &diagnostics)
}
