//! Helpers shared by the parser's integration tests.
//!
//! There are two ways in. `stream` builds a token stream by hand: it is what a
//! test about one exact token sequence wants, and it keeps that test honest by
//! testing the parser rather than the lexer. `parse_source` and its relatives
//! go through the real lexer, which is what the expression, statement and
//! pattern tests use — a precedence test written as forty `TokenKind`s is
//! unreadable, and a typo in the stream is indistinguishable from a bug in the
//! parser. Those helpers assert the input lexes clean, so the distinction
//! never silently blurs.

#![allow(dead_code)]

use science_diagnostics::{Diagnostics, FileId, Span};
use science_lexer::{IntBase, NumSuffix, Token, TokenKind};
use science_parser::{ast::Module, Dump};

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

        If | In | Or | As | Of | Be | Is => 2,
        Let | For | Use | And | Not | Any | Has => 3,
        Null => 4,
        Question => 1,
        Else | Loop | Each | Type | True | SelfValue | SelfType => 4,
        Match | Break | Where | False | Const => 5,
        Return | Choice | Public | Giving | Extern | Unsafe => 6,
        Mutable => 7,
        Function | Borrowed => 8,
        Continue => 8,
        Interface => 9,
        Implements => 10,
        Reserved(_) => 5,

        Arrow | FatArrow | StarStar | DotDot | Shl | Shr | EqEq | NotEq | LtEq | GtEq => 2,
        DotDotEq => 3,
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
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    report(&module, &diagnostics)
}

// --- source-driven helpers ------------------------------------------------

/// Lexes `source` and parses it, rendering the result for a snapshot.
///
/// The hand-built streams above stay where a test is about one exact token
/// sequence. For expressions, statements and patterns they stop paying: a
/// precedence test is unreadable as forty `TokenKind`s, and a mistake in the
/// stream looks exactly like a bug in the parser. These go through the real
/// lexer instead, so what the test shows is what a programmer would write.
///
/// The lexer's own diagnostics are folded in, so a test can never pass by
/// silently mis-lexing its input.
pub fn parse_source(source: &str) -> String {
    let (tokens, lex_diagnostics) = science_lexer::lex(FILE, source);
    assert!(
        lex_diagnostics.is_empty(),
        "the source of this test does not lex cleanly: {:?}",
        lex_diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    report(&module, &diagnostics)
}

/// The same, for sources that are *meant* to produce diagnostics: the lexer's
/// are kept apart so a snapshot shows which phase complained.
pub fn parse_source_allowing_errors(source: &str) -> String {
    let (tokens, _lex_diagnostics) = science_lexer::lex(FILE, source);
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    report(&module, &diagnostics)
}

/// Parses `source` and returns only the dump of the first function's body,
/// which is where a statement or expression test actually looks.
pub fn parse_body(source: &str) -> String {
    let (tokens, lex_diagnostics) = science_lexer::lex(FILE, source);
    assert!(lex_diagnostics.is_empty(), "the source of this test does not lex cleanly");
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    let mut out = String::new();
    for item in &module.items {
        if let science_parser::ast::ItemKind::Fn(decl) = &item.kind {
            if let Some(body) = &decl.body {
                out.push_str(&body.dump());
            }
        }
    }
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
    }
    out
}

/// The dump of the single expression `source` is a binding for, with spans
/// stripped. Precedence and associativity are about *shape*, and a span on
/// every line buries the shape the test is checking.
pub fn shape_of_expr(source: &str) -> String {
    let wrapped = format!("def f():\n    let x be {source}\n");
    let (tokens, lex_diagnostics) = science_lexer::lex(FILE, &wrapped);
    assert!(lex_diagnostics.is_empty(), "the source of this test does not lex cleanly");
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    assert!(
        diagnostics.is_empty(),
        "`{source}` did not parse: {:?}",
        diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );

    let value = let_value(&module)
        .unwrap_or_else(|| panic!("`{source}` did not produce a `let` value"));
    strip_spans(&value)
}

/// The same as `shape_of_expr`, but for a source the *lexer* complains about.
///
/// `syntax-revision-2.md` §1 removed `==` and `!=` without removing the tokens
/// they lex to: the lexer reports them and emits `EqEq`/`NotEq` anyway, so the
/// expression parses exactly as the corrected source would. Checking that
/// needs a helper that tolerates a lexical diagnostic while still insisting
/// the *parser* had nothing to say.
pub fn shape_of_expr_despite_lexical_errors(source: &str) -> String {
    let wrapped = format!("def f():\n    let x be {source}\n");
    let (tokens, _lexical) = science_lexer::lex(FILE, &wrapped);
    let (module, diagnostics) = science_parser::parse_module(&tokens, FILE);
    assert!(
        diagnostics.is_empty(),
        "`{source}` should still parse: {:?}",
        diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );

    let value = let_value(&module)
        .unwrap_or_else(|| panic!("`{source}` did not produce a `let` value"));
    strip_spans(&value)
}

/// The dump of the value of the first `let` in the first function.
fn let_value(module: &Module) -> Option<String> {
    use science_parser::ast::{ItemKind, StmtKind};
    let ItemKind::Fn(decl) = &module.items.first()?.kind else { return None };
    let StmtKind::Let(binding) = &decl.body.as_ref()?.stmts.first()?.kind else { return None };
    Some(binding.value.dump())
}

/// Drops the `@start..end` suffix from every line of a dump.
pub fn strip_spans(dump: &str) -> String {
    dump.lines()
        .map(|line| match line.rfind(" @") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
