//! The three literal kinds of `strings-formatting-and-docs.md` §1.3, and the
//! part of §7's block the lexer owns.
//!
//! The interesting assertions here are the ones about *shape*: an `f"…"` is
//! five kinds of token rather than one, and the three that delimit it are
//! emitted on every path, including the ones that report. A parser that can
//! assume the shape needs no recovery of its own, which is the whole reason
//! the error paths are tested here rather than left to the parser's.

mod common;

use common::{bare, codes, error_spans, kinds};
use science_lexer::TokenKind;

/// The tokens of an `f"…"`, with the structural tail stripped.
fn parts(source: &str) -> Vec<TokenKind> {
    bare(source)
}

// --- `f"…"`, the shape ---------------------------------------------------

#[test]
fn a_plain_string_is_untouched_by_any_of_this() {
    assert_eq!(parts(r#""{a}""#), vec![TokenKind::Str("{a}".to_string())]);
}

#[test]
fn an_f_string_with_one_hole_is_five_tokens() {
    assert_eq!(
        parts(r#"f"{n}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("n".to_string()),
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

#[test]
fn text_around_a_hole_becomes_fragments() {
    assert_eq!(
        parts(r#"f"n is {n} rows""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::FStrText("n is ".to_string()),
            TokenKind::InterpStart,
            TokenKind::Ident("n".to_string()),
            TokenKind::InterpEnd,
            TokenKind::FStrText(" rows".to_string()),
            TokenKind::FStrEnd,
        ]
    );
}

#[test]
fn an_empty_f_string_is_the_two_delimiters() {
    assert_eq!(parts(r#"f"""#), vec![TokenKind::FStrStart, TokenKind::FStrEnd]);
}

/// §1.4: an arbitrary expression, not a bare name. This is the case the note
/// says kills the bare-name rule — `frame.len()` is a method call, and a rule
/// that admits `{n}` and not this "teaches users a boundary they will hit
/// within minutes".
#[test]
fn a_hole_holds_an_arbitrary_expression() {
    assert_eq!(
        parts(r#"f"{frame.len() + 1}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("frame".to_string()),
            TokenKind::Dot,
            TokenKind::Ident("len".to_string()),
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::Plus,
            TokenKind::Int { value: 1, base: science_lexer::IntBase::Dec, suffix: None },
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

/// §1.5's first worked example: a `"` inside a hole that does not end the
/// literal. It only works because the interior is tokenized.
#[test]
fn a_string_inside_a_hole_does_not_end_the_literal() {
    assert_eq!(
        parts(r#"f"{m["a"]}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("m".to_string()),
            TokenKind::LBracket,
            TokenKind::Str("a".to_string()),
            TokenKind::RBracket,
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

/// §1.5's second: nested `f"…"` to any depth, which §1.4 admits because once
/// the interior is lexed properly there is no reason to forbid it.
#[test]
fn an_f_string_nests_inside_a_hole() {
    assert_eq!(
        parts(r#"f"{f"{x}"}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("x".to_string()),
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

/// A brace inside a nested bracket is not the hole's terminator.
#[test]
fn a_brace_inside_a_bracket_does_not_close_the_hole() {
    assert_eq!(
        parts(r#"f"{Doc(title: "a")}""#).len(),
        // start, `{`, Doc, (, title, :, "a", ), `}`, end
        10
    );
}

// --- §1.3's doubled braces ----------------------------------------------

#[test]
fn doubled_braces_are_one_literal_brace_each() {
    assert_eq!(
        parts(r#"f"{{a}}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::FStrText("{a}".to_string()),
            TokenKind::FStrEnd,
        ]
    );
    assert!(codes(r#"f"{{a}}""#).is_empty());
}

#[test]
fn escapes_still_run_inside_an_f_string() {
    assert_eq!(
        parts("f\"a\\tb\\n\""),
        vec![
            TokenKind::FStrStart,
            TokenKind::FStrText("a\tb\n".to_string()),
            TokenKind::FStrEnd,
        ]
    );
}

// --- `r"…"` ---------------------------------------------------------------

#[test]
fn a_raw_string_processes_no_escapes() {
    assert_eq!(parts(r#"r"\frac{a}{b}""#), vec![TokenKind::Str(r"\frac{a}{b}".to_string())]);
    assert!(codes(r#"r"\frac{a}{b}""#).is_empty());
}

#[test]
fn a_raw_string_does_not_interpolate() {
    assert_eq!(parts(r#"r"{n}""#), vec![TokenKind::Str("{n}".to_string())]);
}

/// §1.3 states the limitation: a raw string has no way to contain a `"`.
#[test]
fn a_raw_string_ends_at_the_first_quote_even_after_a_backslash() {
    assert_eq!(parts(r#"r"a\""#), vec![TokenKind::Str(r"a\".to_string())]);
}

// --- §1.3: `f` and `r` do not become reserved words ----------------------

#[test]
fn f_and_r_are_still_ordinary_identifiers() {
    assert_eq!(
        parts("let f be 1"),
        vec![
            TokenKind::Let,
            TokenKind::Ident("f".to_string()),
            TokenKind::Be,
            TokenKind::Int { value: 1, base: science_lexer::IntBase::Dec, suffix: None },
        ]
    );
    assert!(codes("let r be radius").is_empty());
}

/// A space between the word and the quote means it was never a prefix.
#[test]
fn a_space_after_the_word_is_not_a_prefix() {
    assert_eq!(
        parts(r#"f "a""#),
        vec![TokenKind::Ident("f".to_string()), TokenKind::Str("a".to_string())]
    );
}

/// The rule is `f` and `r` and nothing else, which is what keeps this lexing
/// as it always did.
#[test]
fn a_keyword_against_a_quote_is_still_a_keyword_and_a_string() {
    assert_eq!(
        parts(r#"extern"C""#),
        vec![TokenKind::Extern, TokenKind::Str("C".to_string())]
    );
    assert!(codes(r#"extern"C""#).is_empty());
}

// --- §7's block ----------------------------------------------------------

#[test]
fn an_unclosed_interpolation_is_sc0170_and_keeps_the_shape() {
    let source = r#"f"{n""#;
    assert_eq!(codes(source), vec!["SC0170"]);
    // The primary span is the `{`, as §7 requires.
    assert_eq!(error_spans(source)[0], (2, 3));
    assert_eq!(
        parts(source),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("n".to_string()),
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

#[test]
fn an_unpaired_closing_brace_is_sc0171() {
    assert_eq!(codes(r#"f"a}b""#), vec!["SC0171"]);
    assert_eq!(
        parts(r#"f"a}b""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::FStrText("a}b".to_string()),
            TokenKind::FStrEnd,
        ]
    );
}

#[test]
fn a_format_specification_is_sc0173() {
    assert_eq!(codes(r#"f"{x:.3f}""#), vec!["SC0173"]);
    assert_eq!(codes(r#"f"{x!i}""#), vec!["SC0173"]);
    // The expression before the `:` still lexes, so the hole is usable and the
    // one diagnostic is about the part that is not built.
    assert_eq!(
        parts(r#"f"{x:.3f}""#),
        vec![
            TokenKind::FStrStart,
            TokenKind::InterpStart,
            TokenKind::Ident("x".to_string()),
            TokenKind::InterpEnd,
            TokenKind::FStrEnd,
        ]
    );
}

#[test]
fn an_empty_interpolation_is_sc0174() {
    assert_eq!(codes(r#"f"{}""#), vec!["SC0174"]);
    assert_eq!(codes(r#"f"{:.3f}""#), vec!["SC0173"]);
}

#[test]
fn a_comment_inside_an_interpolation_is_sc0175() {
    assert_eq!(codes("f\"{n # why}\""), vec!["SC0175"]);
}

#[test]
fn a_prefix_combination_is_sc0177() {
    assert_eq!(codes(r#"rf"{a}""#), vec!["SC0177"]);
    assert_eq!(codes(r#"fr"{a}""#), vec!["SC0177"]);
    // Recovery reads it as the one literal kind that always exists.
    assert_eq!(parts(r#"rf"{a}""#), vec![TokenKind::Str("{a}".to_string())]);
}

#[test]
fn an_unterminated_f_string_is_still_sc0007() {
    assert_eq!(codes("f\"abc"), vec!["SC0007"]);
}

// --- line structure ------------------------------------------------------

/// §1.5: the indentation machine is suspended inside an interpolation. It has
/// nothing to suspend, because a hole cannot span a line — but a hole inside a
/// call inside a block must not disturb the depth the bracket set up.
#[test]
fn an_f_string_inside_a_call_does_not_disturb_the_line_structure() {
    let source = "def main():\n    print(f\"{n}\")\n";
    let kinds = kinds(source);
    assert_eq!(kinds.iter().filter(|k| matches!(k, TokenKind::Indent)).count(), 1);
    assert_eq!(kinds.iter().filter(|k| matches!(k, TokenKind::Dedent)).count(), 1);
    assert!(codes(source).is_empty());
}
