//! Unit tests for the lexer, one concrete behaviour per test.

mod common;

use common::*;
use link_lexer::IntBase::{Bin, Dec, Hex, Oct};
use link_lexer::NumSuffix::{F32, F64, I32, U8};
use link_lexer::ReservedWord;
use link_lexer::TokenKind::*;

// =========================================================================
// Indentation
// =========================================================================

#[test]
fn an_empty_file_yields_only_eof() {
    assert_eq!(kinds(""), vec![Eof]);
}

#[test]
fn the_eof_token_sits_at_the_end_of_the_source() {
    let t = tokens("abc\n");
    let eof = t.last().unwrap();
    assert_eq!(eof.kind, Eof);
    assert_eq!((eof.span.start, eof.span.end), (4, 4));
}

#[test]
fn a_file_of_comments_only_yields_eof() {
    assert_eq!(kinds("# one\n# two\n"), vec![Eof]);
    assert_eq!(kinds("      # an indented comment\n"), vec![Eof]);
    assert_eq!(kinds("# no trailing newline"), vec![Eof]);
}

#[test]
fn a_file_of_blank_lines_only_yields_eof() {
    assert_eq!(kinds("\n\n   \n\n"), vec![Eof]);
}

#[test]
fn blank_lines_do_not_produce_newline_tokens() {
    assert_eq!(kinds("a\n\n\nb\n"), vec![id("a"), Newline, id("b"), Newline, Eof]);
}

#[test]
fn comment_only_lines_do_not_touch_the_indentation_stack() {
    // The deeply indented comment must not open a block.
    let src = "a:\n    b\n            # comment\n    c\n";
    assert_eq!(
        kinds(src),
        vec![
            id("a"),
            Colon,
            Newline,
            Indent,
            id("b"),
            Newline,
            id("c"),
            Newline,
            Dedent,
            Eof,
        ]
    );
    assert!(codes(src).is_empty());
}

#[test]
fn three_levels_of_nesting_open_three_blocks() {
    let src = "a:\n  b:\n    c:\n      d\n";
    assert_eq!(
        kinds(src),
        vec![
            id("a"),
            Colon,
            Newline,
            Indent,
            id("b"),
            Colon,
            Newline,
            Indent,
            id("c"),
            Colon,
            Newline,
            Indent,
            id("d"),
            Newline,
            Dedent,
            Dedent,
            Dedent,
            Eof,
        ]
    );
}

#[test]
fn several_levels_close_at_once() {
    let src = "a:\n  b:\n    c:\n      d\ne\n";
    assert_eq!(
        kinds(src),
        vec![
            id("a"),
            Colon,
            Newline,
            Indent,
            id("b"),
            Colon,
            Newline,
            Indent,
            id("c"),
            Colon,
            Newline,
            Indent,
            id("d"),
            Newline,
            Dedent,
            Dedent,
            Dedent,
            id("e"),
            Newline,
            Eof,
        ]
    );
    assert!(codes(src).is_empty());
}

#[test]
fn indent_and_dedent_are_always_balanced() {
    for src in [
        "a:\n  b\n",
        "a:\n  b:\n    c\n",
        "a:\n    b:\n        c\n  d\n",
        "a:\n  b\nc:\n  d\n",
    ] {
        let ks = kinds(src);
        let indents = ks.iter().filter(|k| **k == Indent).count();
        let dedents = ks.iter().filter(|k| **k == Dedent).count();
        assert_eq!(indents, dedents, "unbalanced stream for {src:?}");
    }
}

#[test]
fn an_inconsistent_dedent_is_reported() {
    // 8 -> 2 closes both open levels and lands on no level at all.
    let src = "a:\n    b:\n        c\n  d\n";
    assert_eq!(codes(src), ["LK0004"]);
    assert!(messages(src)[0].contains("indentation"));
}

#[test]
fn an_inconsistent_dedent_recovers_by_opening_the_new_level() {
    let src = "a:\n    b:\n        c\n  d\n";
    assert_eq!(
        kinds(src),
        vec![
            id("a"),
            Colon,
            Newline,
            Indent,
            id("b"),
            Colon,
            Newline,
            Indent,
            id("c"),
            Newline,
            Dedent,
            Dedent,
            Indent,
            id("d"),
            Newline,
            Dedent,
            Eof,
        ]
    );
}

#[test]
fn a_tab_in_the_indentation_is_an_error() {
    let src = "a:\n\tb\n";
    assert_eq!(codes(src), ["LK0003"]);
    assert!(messages(src)[0].contains("tab"));
}

#[test]
fn the_tab_diagnostic_explains_that_link_does_not_interpret_tabs() {
    let (_, diags) = run("\tx\n");
    let d = diags.iter().next().unwrap();
    let text = format!("{} {:?}", d.message, d.notes);
    assert!(text.contains("tab"), "{text}");
    assert!(text.contains("space"), "{text}");
}

#[test]
fn a_tab_in_the_indentation_does_not_cascade_into_further_errors() {
    // One error per offending line, and nothing else.
    let src = "a:\n\tb\n\tc\n";
    assert_eq!(codes(src), ["LK0003", "LK0003"]);
}

#[test]
fn a_tab_after_the_indentation_is_not_an_error() {
    let src = "a\tb\n";
    assert!(codes(src).is_empty());
    assert_eq!(bare(src), vec![id("a"), id("b")]);
}

#[test]
fn a_file_without_a_trailing_newline_still_closes_its_line_and_blocks() {
    let src = "a:\n    b";
    assert_eq!(
        kinds(src),
        vec![id("a"), Colon, Newline, Indent, id("b"), Newline, Dedent, Eof]
    );
}

#[test]
fn crlf_line_endings_are_accepted() {
    assert_eq!(kinds("a\r\nb\r\n"), vec![id("a"), Newline, id("b"), Newline, Eof]);
}

#[test]
fn a_file_that_starts_indented_opens_a_block() {
    // Lexically legal even though the parser will reject it.
    assert_eq!(kinds("    a\n"), vec![Indent, id("a"), Newline, Dedent, Eof]);
}

// =========================================================================
// Implicit line continuation
// =========================================================================

#[test]
fn newlines_inside_brackets_are_ignored() {
    let src = "f(\n  a,\n  b,\n)\n";
    assert_eq!(
        kinds(src),
        vec![id("f"), LParen, id("a"), Comma, id("b"), Comma, RParen, Newline, Eof]
    );
    assert!(codes(src).is_empty());
}

#[test]
fn misleading_indentation_inside_brackets_produces_no_indent_tokens() {
    // Wild indentation inside the parentheses, including a dedent past the
    // level the call started at: none of it may reach the stack.
    let src = "a:\n    f(\n            x,\n  y,\n    )\n    b\n";
    let ks = kinds(src);
    assert_eq!(ks.iter().filter(|k| **k == Indent).count(), 1);
    assert_eq!(ks.iter().filter(|k| **k == Dedent).count(), 1);
    assert!(codes(src).is_empty());
}

#[test]
fn a_tab_inside_a_bracket_continuation_is_not_an_indentation_error() {
    let src = "f(\n\ta,\n)\n";
    assert!(codes(src).is_empty());
}

#[test]
fn every_bracket_shape_opens_a_continuation() {
    for (open, close) in [("(", ")"), ("[", "]"), ("{", "}")] {
        let src = format!("f{open}\n1\n{close}\n");
        assert!(
            !kinds(&src).contains(&Indent),
            "{open}{close} did not suppress indentation"
        );
    }
}

#[test]
fn brackets_nest_before_the_continuation_ends() {
    let src = "f([\n1,\n],\n)\nx\n";
    let ks = kinds(src);
    assert!(!ks.contains(&Indent));
    assert_eq!(ks.iter().filter(|k| **k == Newline).count(), 2);
}

#[test]
fn blank_lines_inside_a_continuation_are_ignored_too() {
    let src = "f(\n\n\n  a\n)\n";
    assert_eq!(kinds(src), vec![id("f"), LParen, id("a"), RParen, Newline, Eof]);
}

// =========================================================================
// Integer literals
// =========================================================================

#[test]
fn integer_literals_in_every_base() {
    assert_eq!(kind0("42"), int(42, Dec, None));
    assert_eq!(kind0("0"), int(0, Dec, None));
    assert_eq!(kind0("1_000_000"), int(1_000_000, Dec, None));
    assert_eq!(kind0("0xFF"), int(255, Hex, None));
    assert_eq!(kind0("0xdead_beef"), int(0xdead_beef, Hex, None));
    assert_eq!(kind0("0o777"), int(0o777, Oct, None));
    assert_eq!(kind0("0b1010"), int(0b1010, Bin, None));
}

#[test]
fn separators_are_allowed_anywhere_inside_the_digits() {
    assert_eq!(kind0("1_2_3"), int(123, Dec, None));
    assert_eq!(kind0("0xFF_FF"), int(0xFFFF, Hex, None));
    assert_eq!(kind0("0b1_0_1"), int(0b101, Bin, None));
}

#[test]
fn integer_suffixes_are_recognised() {
    assert_eq!(kind0("42i32"), int(42, Dec, Some(I32)));
    assert_eq!(kind0("7u8"), int(7, Dec, Some(U8)));
    assert_eq!(kind0("0xFFu8"), int(255, Hex, Some(U8)));
    assert_eq!(kind0("1_000i32"), int(1000, Dec, Some(I32)));
}

#[test]
fn the_integer_value_carries_no_prefix_and_no_separators() {
    let t = tokens("0xFF_FFu8");
    assert_eq!(t[0].kind, int(0xFFFF, Hex, Some(U8)));
    // The span still covers the whole written literal.
    assert_eq!((t[0].span.start, t[0].span.end), (0, 9));
}

#[test]
fn u128_max_is_not_an_overflow() {
    assert_eq!(
        kind0("340282366920938463463374607431768211455"),
        int(u128::MAX, Dec, None)
    );
}

#[test]
fn an_integer_that_does_not_fit_in_u128_is_reported() {
    let src = "340282366920938463463374607431768211456";
    assert_eq!(codes(src), ["LK0005"]);
    // Still a token, so the parser keeps going.
    assert_eq!(bare(src).len(), 1);
}

#[test]
fn a_float_too_large_to_represent_is_reported() {
    // `str::parse` saturates to infinity rather than failing, so without an
    // explicit check this would compile into a silent `inf`.
    assert_eq!(codes("1e400"), ["LK0011"]);
    // Still a token, so the parser keeps going.
    assert_eq!(bare("1e400").len(), 1);
}

#[test]
fn a_float_at_the_edge_of_the_range_is_not_an_error() {
    assert!(codes("1e308").is_empty());
    assert!(codes("1e-308").is_empty());
}

#[test]
fn a_digit_outside_the_base_is_reported() {
    assert_eq!(codes("0b1012"), ["LK0010"]);
    assert_eq!(codes("0o789"), ["LK0010"]);
}

#[test]
fn a_base_prefix_with_no_digits_is_reported() {
    assert_eq!(codes("0x"), ["LK0010"]);
}

#[test]
fn an_unknown_numeric_suffix_is_reported() {
    assert_eq!(codes("42foo"), ["LK0009"]);
    assert_eq!(codes("2.5i32"), ["LK0009"]);
}

// =========================================================================
// Float literals
// =========================================================================

#[test]
// `3.14` is the spec example; clippy would rather see `PI`.
#[allow(clippy::approx_constant)]
fn float_literals() {
    assert_eq!(kind0("3.14"), float(3.14, None));
    assert_eq!(kind0("1e-9"), float(1e-9, None));
    assert_eq!(kind0("1E9"), float(1e9, None));
    assert_eq!(kind0("2.5e+3"), float(2.5e3, None));
    assert_eq!(kind0("1_000.5"), float(1000.5, None));
}

#[test]
fn float_suffixes_are_recognised() {
    assert_eq!(kind0("2.5f32"), float(2.5, Some(F32)));
    assert_eq!(kind0("2.5f64"), float(2.5, Some(F64)));
    assert_eq!(kind0("1e9f32"), float(1e9, Some(F32)));
}

#[test]
fn a_dot_followed_by_a_name_is_field_access_not_a_float() {
    assert_eq!(
        bare("1.foo()"),
        vec![int(1, Dec, None), Dot, id("foo"), LParen, RParen]
    );
    assert!(codes("1.foo()").is_empty());
}

#[test]
fn a_float_needs_a_digit_after_the_dot() {
    // `1.` is an integer followed by a dot, not a malformed float.
    assert_eq!(bare("1."), vec![int(1, Dec, None), Dot]);
}

#[test]
fn an_e_that_is_not_an_exponent_is_treated_as_a_suffix() {
    // `1e` has no exponent digits, so `e` is read as a (bad) suffix rather
    // than silently producing a float.
    assert_eq!(codes("1e"), ["LK0009"]);
}

// =========================================================================
// String literals
// =========================================================================

#[test]
fn plain_strings() {
    assert_eq!(kind0(r#""hello""#), text("hello"));
    assert_eq!(kind0(r#""""#), text(""));
    assert_eq!(kind0(r#""with accents: ñ é""#), text("with accents: ñ é"));
}

#[test]
fn every_escape_from_the_spec_is_resolved() {
    assert_eq!(kind0(r#""a\nb""#), text("a\nb"));
    assert_eq!(kind0(r#""a\tb""#), text("a\tb"));
    assert_eq!(kind0(r#""a\rb""#), text("a\rb"));
    assert_eq!(kind0(r#""a\\b""#), text("a\\b"));
    assert_eq!(kind0(r#""a\"b""#), text("a\"b"));
    assert_eq!(kind0(r#""a\0b""#), text("a\0b"));
    assert_eq!(kind0(r#""\u{1F600}""#), text("\u{1F600}"));
    assert_eq!(kind0(r#""\u{41}""#), text("A"));
}

#[test]
fn an_unknown_escape_is_reported() {
    let src = r#""a\qb""#;
    assert_eq!(codes(src), ["LK0006"]);
    // Recovery keeps the escaped character so the rest of the string survives.
    assert_eq!(kind0(src), text("aqb"));
}

#[test]
fn a_malformed_unicode_escape_is_reported() {
    assert_eq!(codes(r#""\u{110000}""#), ["LK0006"]); // not a scalar value
    assert_eq!(codes(r#""\uFFFF""#), ["LK0006"]); // missing braces
    assert_eq!(codes(r#""\u{12""#), ["LK0006"]); // unclosed
    assert_eq!(codes(r#""\u{zz}""#), ["LK0006"]); // not hex
}

#[test]
fn a_string_left_open_at_the_end_of_the_line_is_reported() {
    let src = "\"abc\nx\n";
    assert_eq!(codes(src), ["LK0007"]);
    // The newline is not swallowed: the next line lexes normally.
    assert_eq!(kinds(src), vec![text("abc"), Newline, id("x"), Newline, Eof]);
}

#[test]
fn a_string_left_open_at_the_end_of_the_file_is_reported() {
    assert_eq!(codes("\"abc"), ["LK0007"]);
    assert_eq!(kind0("\"abc"), text("abc"));
}

// =========================================================================
// Character literals
// =========================================================================

#[test]
fn character_literals() {
    assert_eq!(kind0("'a'"), Char('a'));
    assert_eq!(kind0("'ñ'"), Char('ñ'));
    assert_eq!(kind0(r"'\n'"), Char('\n'));
    assert_eq!(kind0(r"'\\'"), Char('\\'));
    assert_eq!(kind0(r"'\''"), Char('\''));
    assert_eq!(kind0(r"'\u{41}'"), Char('A'));
}

#[test]
fn a_character_literal_left_open_is_reported() {
    assert_eq!(codes("'a"), ["LK0008"]);
    assert_eq!(codes("'a\n"), ["LK0008"]);
}

#[test]
fn an_empty_character_literal_is_reported() {
    assert_eq!(codes("''"), ["LK0008"]);
}

#[test]
fn a_character_literal_with_more_than_one_character_is_reported() {
    assert_eq!(codes("'ab'"), ["LK0008"]);
}

#[test]
fn lexing_continues_after_a_broken_character_literal() {
    let src = "'a\nlet x = 1\n";
    assert_eq!(codes(src), ["LK0008"]);
    assert!(kinds(src).contains(&Let));
}

// =========================================================================
// Identifiers, keywords and comments
// =========================================================================

#[test]
fn keywords_are_resolved() {
    assert_eq!(bare("fn let mut if else match"), vec![Fn, Let, Mut, If, Else, Match]);
    assert_eq!(bare("for in while loop"), vec![For, In, While, Loop]);
    assert_eq!(bare("return break continue"), vec![Return, Break, Continue]);
    assert_eq!(bare("struct enum trait impl"), vec![Struct, Enum, Trait, Impl]);
    assert_eq!(bare("use mod pub"), vec![Use, Mod, Pub]);
    assert_eq!(bare("true false"), vec![True, False]);
    assert_eq!(bare("self Self"), vec![SelfValue, SelfType]);
    assert_eq!(bare("as dyn where"), vec![As, Dyn, Where]);
    assert_eq!(bare("and or not"), vec![And, Or, Not]);
}

#[test]
fn words_reserved_for_later_phases_are_not_identifiers() {
    assert_eq!(
        bare("agent tool prompt"),
        vec![
            Reserved(ReservedWord::Agent),
            Reserved(ReservedWord::Tool),
            Reserved(ReservedWord::Prompt),
        ]
    );
    assert_eq!(
        bare("spawn send receive"),
        vec![
            Reserved(ReservedWord::Spawn),
            Reserved(ReservedWord::Send),
            Reserved(ReservedWord::Receive),
        ]
    );
    assert_eq!(bare("tensor"), vec![Reserved(ReservedWord::Tensor)]);
}

#[test]
fn identifiers_admit_unicode_letters_and_underscores() {
    assert_eq!(
        bare("año _x x1 índice_2"),
        vec![id("año"), id("_x"), id("x1"), id("índice_2")]
    );
}

#[test]
fn a_lone_underscore_is_the_wildcard_not_an_identifier() {
    assert_eq!(bare("_"), vec![Underscore]);
    assert_eq!(bare("_a"), vec![id("_a")]);
}

#[test]
fn a_word_that_merely_starts_with_a_keyword_is_an_identifier() {
    assert_eq!(bare("iffy letter selfish"), vec![id("iffy"), id("letter"), id("selfish")]);
}

#[test]
fn identifier_spans_are_byte_offsets() {
    let t = tokens("añadir\n");
    assert_eq!(t[0].kind, id("añadir"));
    assert_eq!((t[0].span.start, t[0].span.end), (0, 7));
}

#[test]
fn a_comment_runs_to_the_end_of_the_line() {
    assert_eq!(kinds("a # comment\nb\n"), vec![id("a"), Newline, id("b"), Newline, Eof]);
}

#[test]
fn hash_is_never_emitted_as_a_token() {
    assert!(!kinds("a # b\n").contains(&Hash));
}

#[test]
fn a_hash_inside_a_string_is_not_a_comment() {
    assert_eq!(kind0(r##""a # b""##), text("a # b"));
}

// =========================================================================
// Operators and punctuation
// =========================================================================

#[test]
fn maximal_munch_prefers_the_longest_operator() {
    assert_eq!(bare("<<"), vec![Shl]);
    assert_eq!(bare("< <"), vec![Lt, Lt]);
    assert_eq!(bare(">>"), vec![Shr]);
    assert_eq!(bare("->"), vec![Arrow]);
    assert_eq!(bare("- >"), vec![Minus, Gt]);
    assert_eq!(bare("=="), vec![EqEq]);
    assert_eq!(bare("=>"), vec![FatArrow]);
    assert_eq!(bare("= ="), vec![Eq, Eq]);
    assert_eq!(bare("!="), vec![NotEq]);
    assert_eq!(bare("<="), vec![LtEq]);
    assert_eq!(bare(">="), vec![GtEq]);
}

#[test]
fn all_punctuation() {
    assert_eq!(
        bare("()[]{},:;.?@"),
        vec![
            LParen, RParen, LBracket, RBracket, LBrace, RBrace, Comma, Colon, Semi, Dot,
            Question, At,
        ]
    );
}

#[test]
fn all_operators() {
    assert_eq!(
        bare("+ - * / % & | ^"),
        vec![Plus, Minus, Star, Slash, Percent, Amp, Pipe, Caret]
    );
}

#[test]
fn a_reference_to_a_mutable_place_is_two_tokens() {
    assert_eq!(bare("&mut x"), vec![Amp, Mut, id("x")]);
}

#[test]
fn operator_spans_cover_exactly_the_operator() {
    let t = tokens("a->b");
    assert_eq!(t[1].kind, Arrow);
    assert_eq!((t[1].span.start, t[1].span.end), (1, 3));
}

// =========================================================================
// Error recovery
// =========================================================================

#[test]
fn an_unrecognised_character_becomes_unknown_and_lexing_continues() {
    let src = "a $ b\n";
    assert_eq!(bare(src), vec![id("a"), Unknown('$'), id("b")]);
    assert_eq!(codes(src), ["LK0001"]);
}

#[test]
fn a_lone_bang_is_unknown_because_negation_is_spelled_not() {
    assert_eq!(bare("!"), vec![Unknown('!')]);
    assert_eq!(codes("!"), ["LK0001"]);
}

#[test]
fn the_unknown_token_span_covers_the_whole_character() {
    let t = tokens("€");
    assert_eq!(t[0].kind, Unknown('€'));
    assert_eq!((t[0].span.start, t[0].span.end), (0, 3));
}

#[test]
fn a_single_pass_reports_several_errors() {
    let src = "a $ b\n\tc\n\"open\n";
    assert_eq!(codes(src), ["LK0001", "LK0003", "LK0007"]);
}

#[test]
fn errors_come_out_in_source_order() {
    let spans = error_spans("$ $ $\n");
    assert_eq!(spans, vec![(0, 1), (2, 3), (4, 5)]);
}

// =========================================================================
// Whole constructs from the spec
// =========================================================================

#[test]
fn the_inline_block_form_stays_on_one_logical_line() {
    let src = "if a.len() > b.len(): a else: b\n";
    let ks = kinds(src);
    assert!(!ks.contains(&Indent));
    assert!(!ks.contains(&Dedent));
    assert_eq!(ks.iter().filter(|k| **k == Newline).count(), 1);
}

#[test]
fn a_generic_signature_lexes_without_turbofish_ambiguity() {
    assert_eq!(
        bare("fn largest[T: Ord](items: &Array[T]) -> &T:"),
        vec![
            Fn,
            id("largest"),
            LBracket,
            id("T"),
            Colon,
            id("Ord"),
            RBracket,
            LParen,
            id("items"),
            Colon,
            Amp,
            id("Array"),
            LBracket,
            id("T"),
            RBracket,
            RParen,
            Arrow,
            Amp,
            id("T"),
            Colon,
        ]
    );
}

#[test]
fn the_question_mark_operator_lexes_on_its_own() {
    assert_eq!(bare("read_file(path)?"), vec![id("read_file"), LParen, id("path"), RParen, Question]);
}

#[test]
fn a_small_program_produces_no_diagnostics() {
    let src = "\
fn main():
    let greeting = \"hola\"
    let mut count = 0
    count = count + 1
";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
}

#[test]
fn token_spans_never_go_backwards() {
    // A parser that walks the stream, and a renderer that groups diagnostics by
    // position, both rely on this. Error recovery must not break it.
    for src in [
        "a:\n  b:\n    c\n",
        "a:\n    b:\n        c\n  d\n", // LK0004 recovery
        "fn main():\n\tlet x = 1\n",    // LK0003 recovery
        "f(\n        a,\n  b,\n)\nx\n",
        "let a = \"open\nlet b = 'x\n",
    ] {
        let t = tokens(src);
        for pair in t.windows(2) {
            assert!(
                pair[0].span.start <= pair[1].span.start,
                "{src:?}: {:?} then {:?}",
                pair[0],
                pair[1]
            );
        }
    }
}
