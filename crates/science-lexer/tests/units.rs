//! Unit tests for the lexer, one concrete behaviour per test.

mod common;

use common::*;
use science_lexer::IntBase::{Bin, Dec, Hex, Oct};
use science_lexer::NumSuffix::{F32, F64, I32, U8};
use science_lexer::ReservedWord;
use science_lexer::TokenKind::*;

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
    assert_eq!(codes(src), ["SC0004"]);
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
    assert_eq!(codes(src), ["SC0003"]);
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
    assert_eq!(codes(src), ["SC0003", "SC0003"]);
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
    assert_eq!(codes(src), ["SC0005"]);
    // Still a token, so the parser keeps going.
    assert_eq!(bare(src).len(), 1);
}

#[test]
fn a_float_too_large_to_represent_is_reported() {
    // `str::parse` saturates to infinity rather than failing, so without an
    // explicit check this would compile into a silent `inf`.
    assert_eq!(codes("1e400"), ["SC0011"]);
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
    assert_eq!(codes("0b1012"), ["SC0010"]);
    assert_eq!(codes("0o789"), ["SC0010"]);
}

#[test]
fn a_base_prefix_with_no_digits_is_reported() {
    assert_eq!(codes("0x"), ["SC0010"]);
}

#[test]
fn an_unknown_numeric_suffix_is_reported() {
    assert_eq!(codes("42foo"), ["SC0009"]);
    assert_eq!(codes("2.5i32"), ["SC0009"]);
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
    assert_eq!(codes("1e"), ["SC0009"]);
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
    assert_eq!(codes(src), ["SC0006"]);
    // Recovery keeps the escaped character so the rest of the string survives.
    assert_eq!(kind0(src), text("aqb"));
}

#[test]
fn a_malformed_unicode_escape_is_reported() {
    assert_eq!(codes(r#""\u{110000}""#), ["SC0006"]); // not a scalar value
    assert_eq!(codes(r#""\uFFFF""#), ["SC0006"]); // missing braces
    assert_eq!(codes(r#""\u{12""#), ["SC0006"]); // unclosed
    assert_eq!(codes(r#""\u{zz}""#), ["SC0006"]); // not hex
}

#[test]
fn a_string_left_open_at_the_end_of_the_line_is_reported() {
    let src = "\"abc\nx\n";
    assert_eq!(codes(src), ["SC0007"]);
    // The newline is not swallowed: the next line lexes normally.
    assert_eq!(kinds(src), vec![text("abc"), Newline, id("x"), Newline, Eof]);
}

#[test]
fn a_string_left_open_at_the_end_of_the_file_is_reported() {
    assert_eq!(codes("\"abc"), ["SC0007"]);
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
    assert_eq!(codes("'a"), ["SC0008"]);
    assert_eq!(codes("'a\n"), ["SC0008"]);
}

#[test]
fn an_empty_character_literal_is_reported() {
    assert_eq!(codes("''"), ["SC0008"]);
}

#[test]
fn a_character_literal_with_more_than_one_character_is_reported() {
    assert_eq!(codes("'ab'"), ["SC0008"]);
}

#[test]
fn lexing_continues_after_a_broken_character_literal() {
    let src = "'a\nlet x be 1\n";
    assert_eq!(codes(src), ["SC0008"]);
    assert!(kinds(src).contains(&Let));
}

// =========================================================================
// Identifiers, keywords and comments
// =========================================================================

#[test]
fn keywords_are_resolved() {
    assert_eq!(bare("function"), vec![Function]);
    assert_eq!(bare("let be mutable"), vec![Let, Be, Mutable]);
    assert_eq!(bare("if else match"), vec![If, Else, Match]);
    assert_eq!(bare("for each in loop"), vec![For, Each, In, Loop]);
    assert_eq!(bare("return break continue"), vec![Return, Break, Continue]);
    assert_eq!(bare("type choice interface"), vec![Type, Choice, Interface]);
    assert_eq!(bare("implements has"), vec![Implements, Has]);
    assert_eq!(bare("of borrowed any"), vec![Of, Borrowed, Any]);
    assert_eq!(bare("use public const"), vec![Use, Public, Const]);
    assert_eq!(bare("giving null"), vec![Giving, Null]);
    assert_eq!(bare("true false"), vec![True, False]);
    assert_eq!(bare("self Self"), vec![SelfValue, SelfType]);
    assert_eq!(bare("as where"), vec![As, Where]);
    assert_eq!(bare("and or not"), vec![And, Or, Not]);
}

#[test]
fn the_words_the_old_symbol_syntax_used_are_no_longer_keywords() {
    // `fn`, `mut`, `struct`, `enum`, `impl`, `dyn` and `pub` were replaced by
    // English words, so they are ordinary identifiers again. Nothing may keep
    // resolving them out of habit.
    assert_eq!(
        bare("fn mut struct enum impl dyn pub"),
        vec![
            id("fn"),
            id("mut"),
            id("struct"),
            id("enum"),
            id("impl"),
            id("dyn"),
            id("pub"),
        ]
    );
}

#[test]
fn identity_is_one_word_and_inequality_is_two() {
    // Revision 2 §1: `is` and `is not` are the whole of the comparisons
    // written as words. The lexer emits each word on its own and the parser
    // pairs `is` with a following `not`; nothing here knows that.
    assert_eq!(bare("a is b"), vec![id("a"), Is, id("b")]);
    assert_eq!(bare("a is not b"), vec![id("a"), Is, Not, id("b")]);
}

#[test]
fn the_comparison_phrase_words_are_identifiers_again() {
    // Revision 2 §1.2 deleted `is at least`, `is at most`, `is above` and
    // `is below`, which frees all five words. `poly.at(x)` and `least_squares`
    // are ordinary names now, and nothing may keep resolving the old spelling.
    assert_eq!(
        bare("at above below most least"),
        vec![id("at"), id("above"), id("below"), id("most"), id("least")]
    );
    assert_eq!(
        bare("a is at least b"),
        vec![id("a"), Is, id("at"), id("least"), id("b")]
    );
    assert_eq!(bare("a is above b"), vec![id("a"), Is, id("above"), id("b")]);
}

#[test]
fn the_words_the_first_revision_used_are_no_longer_keywords() {
    // Revision 2 removed `while` (§2.2), `trait` (§6) and `methods` (§5).
    // All three are ordinary identifiers; the parser reports them by name.
    assert_eq!(bare("while trait methods"), vec![id("while"), id("trait"), id("methods")]);
    assert_eq!(bare("interface Summarize:"), vec![Interface, id("Summarize"), Colon]);
}

#[test]
fn the_multi_word_keywords_are_sequences_and_not_single_tokens() {
    // §4.3: `let ... be` and `mutable borrowed` are sequences of reserved
    // words, not new tokens. `for each` and `has methods` were two more until
    // revision 2 §2.1 and §5 cut each down to one word.
    assert_eq!(bare("for x in xs:"), vec![For, id("x"), In, id("xs"), Colon]);
    assert_eq!(bare("let mutable x be 0"), vec![Let, Mutable, id("x"), Be, int(0, Dec, None)]);
    assert_eq!(bare("mutable borrowed Doc"), vec![Mutable, Borrowed, id("Doc")]);
    assert_eq!(bare("Doc has:"), vec![id("Doc"), Has, Colon]);
}

#[test]
fn each_is_still_a_keyword_outside_the_loop() {
    // §2.1: `each` left the loop and kept its other job, naming the subject
    // of the enclosing call. It is still a token, not an identifier.
    assert_eq!(
        bare("docs.map(each.title)"),
        vec![id("docs"), Dot, id("map"), LParen, Each, Dot, id("title"), RParen]
    );
}

#[test]
fn words_reserved_for_later_phases_are_not_identifiers() {
    use ReservedWord::*;
    // Every word in §13's "reserved, not yet used" list, so that adding one to
    // the contract without adding it here is a failing test and not a silent
    // identifier.
    let expected = [
        ("agent", Agent),
        ("tool", Tool),
        ("prompt", Prompt),
        ("spawn", Spawn),
        ("send", Send),
        ("receive", Receive),
        ("durable", Durable),
        ("checkpoint", Checkpoint),
        ("resume", Resume),
        ("supervise", Supervise),
        ("async", Async),
        ("await", Await),
        ("tensor", Tensor),
        ("shape", Shape),
        ("model", Model),
        ("mod", Mod),
        ("pure", Pure),
        ("parallel", Parallel),
        ("on", On),
        ("with", With),
        ("yield", Yield),
        ("assert", Assert),
        ("move", Move),
        ("static", Static),
        ("macro", Macro),
        ("union", Union),
        ("kernel", Kernel),
        ("import", Import),
    ];
    for (word, expected) in expected {
        assert_eq!(bare(word), vec![Reserved(expected)], "`{word}` did not lex as reserved");
    }
}

#[test]
fn the_newly_reserved_words_lex_in_a_sentence_too() {
    // `on` and `with` are common enough that they will turn up as variable
    // names. They must still be reserved tokens wherever they appear, not
    // identifiers in some positions and keywords in others.
    assert_eq!(
        bare("let on be 1"),
        vec![Let, Reserved(ReservedWord::On), Be, int(1, Dec, None)]
    );
    assert_eq!(
        bare("f(with: 2)"),
        vec![id("f"), LParen, Reserved(ReservedWord::With), Colon, int(2, Dec, None), RParen]
    );
    // And the lexer says nothing about them: `Reserved` is a token, not an
    // error. Whoever needs an identifier here is the one that reports it.
    assert!(codes("let on be 1").is_empty());
}

#[test]
fn extern_and_unsafe_moved_from_a_reservation_to_keywords() {
    // The FFI note's §1.1 puts both on the `extern` block, and §3 puts
    // `unsafe` on a block expression too, so neither is held back for a later
    // phase any more. They are keywords everywhere, not contextual words: a
    // variable named `unsafe` was never legal and still is not.
    assert_eq!(bare("unsafe extern \"C\":"), vec![Unsafe, Extern, text("C"), Colon]);
    assert_eq!(bare("let extern be 1"), vec![Let, Extern, Be, int(1, Dec, None)]);
    // The words the FFI grammar leaves contextual stay ordinary identifiers,
    // which is what `reserved-words.md` asks of anything that is not
    // load-bearing everywhere.
    assert_eq!(
        bare("library via kind when available symbol size align"),
        vec![
            id("library"),
            id("via"),
            id("kind"),
            id("when"),
            id("available"),
            id("symbol"),
            id("size"),
            id("align"),
        ]
    );
    // `static` and `union` are used by the block's grammar and stay reserved:
    // the FFI note takes them from the list §13 already froze rather than
    // lengthening it.
    assert_eq!(bare("static"), vec![Reserved(ReservedWord::Static)]);
    assert_eq!(bare("union"), vec![Reserved(ReservedWord::Union)]);
}

#[test]
fn mod_moved_from_a_keyword_to_a_reservation() {
    assert_eq!(bare("mod"), vec![Reserved(ReservedWord::Mod)]);
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
    // The English keywords are short and common, so this matters more than it
    // used to: several of them are prefixes of ordinary words.
    assert_eq!(
        bare("atlas below_zero mostly island"),
        vec![id("atlas"), id("below_zero"), id("mostly"), id("island")]
    );
    assert_eq!(
        bare("onset without typed beam"),
        vec![id("onset"), id("without"), id("typed"), id("beam")]
    );
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
    // `==` and `!=` are no longer Science (§1), but the lexer still munches
    // them maximally so that it can report them as one thing rather than two.
    assert_eq!(bare("=="), vec![EqEq]);
    assert_eq!(bare("=>"), vec![FatArrow]);
    assert_eq!(bare("= ="), vec![Eq, Eq]);
    assert_eq!(bare("!="), vec![NotEq]);
    assert_eq!(bare("<="), vec![LtEq]);
    assert_eq!(bare(">="), vec![GtEq]);
}

#[test]
fn the_arrow_is_one_token() {
    // §4.4 writes the return type `-> T`, so `-` has to look ahead: longest
    // match wins and the two characters are a single `Arrow`.
    assert_eq!(bare("->"), vec![Arrow]);
    assert!(codes("->").is_empty());
    assert_eq!(bare("() -> Int"), vec![LParen, RParen, Arrow, id("Int")]);
}

#[test]
fn only_the_two_characters_together_are_an_arrow() {
    // The lookahead is for `>` immediately after `-`. A space between them
    // leaves subtraction followed by a greater-than, and `-` on its own is
    // still subtraction or negation.
    assert_eq!(bare("- >"), vec![Minus, Gt]);
    assert_eq!(bare("a-b"), vec![id("a"), Minus, id("b")]);
    assert_eq!(bare("-1"), vec![Minus, int(1, Dec, None)]);
    assert_eq!(bare("a - > b"), vec![id("a"), Minus, Gt, id("b")]);
}

#[test]
fn the_arrow_does_not_disturb_the_fat_arrow() {
    // `=>` is lexed by the `=` arm, which the new `-` arm never reaches.
    assert_eq!(bare("=>"), vec![FatArrow]);
    assert_eq!(bare("-> =>"), vec![Arrow, FatArrow]);
    assert_eq!(bare("=> ->"), vec![FatArrow, Arrow]);
    // `-` followed by `=` is still two tokens: only `>` makes an arrow.
    assert_eq!(bare("-="), vec![Minus, Eq]);
}

#[test]
fn all_punctuation() {
    assert_eq!(
        bare("()[]{},:;.@"),
        vec![
            LParen, RParen, LBracket, RBracket, LBrace, RBrace, Comma, Colon, Semi, Dot,
            AtSign,
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
fn operator_spans_cover_exactly_the_operator() {
    let t = tokens("a<<b");
    assert_eq!(t[1].kind, Shl);
    assert_eq!((t[1].span.start, t[1].span.end), (1, 3));
}

// =========================================================================
// Error recovery
// =========================================================================

#[test]
fn an_unrecognised_character_becomes_unknown_and_lexing_continues() {
    let src = "a $ b\n";
    assert_eq!(bare(src), vec![id("a"), Unknown('$'), id("b")]);
    assert_eq!(codes(src), ["SC0001"]);
}

#[test]
fn a_lone_bang_is_unknown_because_negation_is_spelled_not() {
    assert_eq!(bare("!"), vec![Unknown('!')]);
    assert_eq!(codes("!"), ["SC0001"]);

    // The message used to say `!` was "only valid as part of `!=`". §1 took
    // `!=` away too, so the only thing left to name is `not`.
    let (_, diags) = run("!");
    let d = diags.iter().next().unwrap();
    let rendered = format!("{} {:?} {:?}", d.message, d.labels, d.notes);
    assert!(rendered.contains("`not`"), "{rendered}");
    assert!(!rendered.contains("!="), "the `!=` advice is stale: {rendered}");
    assert_eq!(d.suggestions.first().unwrap().replacement, "not ");
}

// --- the comparison symbols §1 removed -----------------------------------

/// `==` is reported as SC0016 and still lexes as `EqEq`.
///
/// Emitting the token is the point: the parser reads the expression exactly
/// as it would have read `is`, so a stale symbol costs one diagnostic and the
/// tree is the one applying the fix would produce.
#[test]
fn the_equality_symbol_is_reported_but_still_lexes_as_equality() {
    let src = "a == b";
    assert_eq!(bare(src), vec![id("a"), EqEq, id("b")]);
    assert_eq!(codes(src), ["SC0016"]);

    let (_, diags) = run(src);
    let d = diags.iter().next().unwrap();
    assert_eq!(d.message, "equality is written `is`");
    let fix = d.suggestions.first().expect("SC0016 should offer a fix");
    assert_eq!(&src[fix.span.start as usize..fix.span.end as usize], "==");
    assert_eq!(fix.replacement, "is");
}

/// `!=` is SC0017, and it is SC0017 *alone*: the lone-`!` arm must never see
/// the character, or one stale symbol would cost two diagnostics.
#[test]
fn the_inequality_symbol_is_reported_once_and_still_lexes_as_inequality() {
    let src = "a != b";
    assert_eq!(bare(src), vec![id("a"), NotEq, id("b")]);
    assert_eq!(codes(src), ["SC0017"]);

    let (_, diags) = run(src);
    let d = diags.iter().next().unwrap();
    assert_eq!(d.message, "inequality is written `is not`");
    let fix = d.suggestions.first().expect("SC0017 should offer a fix");
    assert_eq!(&src[fix.span.start as usize..fix.span.end as usize], "!=");
    assert_eq!(fix.replacement, "is not");
}

/// The fix spans cover the two characters and nothing around them, which is
/// what makes them applicable rather than merely advisory.
#[test]
fn the_comparison_fixes_replace_exactly_the_two_characters() {
    let src = "alpha==beta\n";
    let (_, diags) = run(src);
    let fix = diags.iter().next().unwrap().suggestions.first().unwrap().clone();
    assert_eq!((fix.span.start, fix.span.end), (5, 7));

    let src = "alpha!=beta\n";
    let (_, diags) = run(src);
    let fix = diags.iter().next().unwrap().suggestions.first().unwrap().clone();
    assert_eq!((fix.span.start, fix.span.end), (5, 7));
}

/// Nothing next to the removed symbols is caught up in them: `=`, `=>` and
/// the ordering symbols lex exactly as before, with no diagnostic.
#[test]
fn removing_the_comparison_symbols_leaves_the_neighbouring_operators_alone() {
    assert!(codes("a < b").is_empty());
    assert!(codes("a > b").is_empty());
    assert!(codes("a <= b").is_empty());
    assert!(codes("a >= b").is_empty());
    assert!(codes("a = b").is_empty(), "a single `=` is the parser's to complain about");
    assert!(codes("x => y").is_empty());
    assert_eq!(bare("a = b"), vec![id("a"), Eq, id("b")]);
}

/// One diagnostic per stale symbol, and the stream stays usable: a file with
/// several of them reports several times and still lexes to the end.
#[test]
fn every_stale_comparison_symbol_is_reported_and_none_stops_the_lexer() {
    let src = "a == b != c == d\n";
    assert_eq!(codes(src), ["SC0016", "SC0017", "SC0016"]);
    assert_eq!(
        bare(src),
        vec![id("a"), EqEq, id("b"), NotEq, id("c"), EqEq, id("d")]
    );
}

#[test]
fn a_question_mark_is_a_token_because_it_is_the_presence_test() {
    // `?` spent one revision rejected outright: it had been error propagation,
    // the design removed it, and the lexer reported it as a character Science
    // does not have. Revision 2 §3.1 gave it back, as a postfix test for
    // presence, and the rejection went with the meaning it was rejecting.
    let src = "read_file(path)?";
    assert!(codes(src).is_empty(), "{:?}", codes(src));
    assert_eq!(
        bare(src),
        vec![id("read_file"), LParen, id("path"), RParen, Question]
    );

    // One character, one token: there is no `??`, `?.` or `?:` to scan into.
    assert_eq!(bare("a??"), vec![id("a"), Question, Question]);
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
    assert_eq!(codes(src), ["SC0001", "SC0003", "SC0007"]);
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
    // §4.4's `largest`. Generic arguments are spelled `of T` and borrows
    // `borrowed T`, so nothing here needs brackets at all.
    assert_eq!(
        bare("function largest of T(items: borrowed Array of T) -> borrowed T:"),
        vec![
            Function,
            id("largest"),
            Of,
            id("T"),
            LParen,
            id("items"),
            Colon,
            Borrowed,
            id("Array"),
            Of,
            id("T"),
            RParen,
            Arrow,
            Borrowed,
            id("T"),
            Colon,
        ]
    );
}

#[test]
fn several_generic_arguments_are_parenthesised() {
    assert_eq!(
        bare("Map of (String, Int)"),
        vec![id("Map"), Of, LParen, id("String"), Comma, id("Int"), RParen]
    );
}

#[test]
fn try_is_an_ordinary_identifier_now() {
    // Revision 2 §3 removed `try` with the `Result` it unwrapped. The lexer
    // does not know that: it hands back an `Ident` and the parser reports the
    // migration, which is how `while`, `trait` and `println` are handled too.
    assert_eq!(
        bare("let text be try read_file(path)"),
        vec![Let, id("text"), Be, id("try"), id("read_file"), LParen, id("path"), RParen]
    );
    assert!(codes("let text be try read_file(path)").is_empty());
}

#[test]
fn the_error_model_lexes() {
    assert_eq!(
        bare("let value, err be f()"),
        vec![Let, id("value"), Comma, id("err"), Be, id("f"), LParen, RParen]
    );
    assert_eq!(bare("if err?:"), vec![If, id("err"), Question, Colon]);
    assert_eq!(bare("Error?"), vec![id("Error"), Question]);
    assert_eq!(bare("return (parsed, null)"), vec![
        Return,
        LParen,
        id("parsed"),
        Comma,
        Null,
        RParen
    ]);
    // `?` was freed when it stopped being error propagation, so it now
    // produces a token rather than the diagnostic that used to reject it.
    assert!(codes("if err?:").is_empty());
}

#[test]
fn the_two_closure_forms_lex_as_ordinary_words() {
    // §4.6: `each.title` and `doc giving doc.title`. Neither needs a token of
    // its own; both are words the parser recognises.
    assert_eq!(
        bare("docs.map(each.title)"),
        vec![id("docs"), Dot, id("map"), LParen, Each, Dot, id("title"), RParen]
    );
    assert_eq!(
        bare("docs.map(doc giving doc.title)"),
        vec![
            id("docs"),
            Dot,
            id("map"),
            LParen,
            id("doc"),
            Giving,
            id("doc"),
            Dot,
            id("title"),
            RParen,
        ]
    );
}

#[test]
fn a_small_program_produces_no_diagnostics() {
    let src = "\
function main():
    let greeting be \"hola\"
    let mutable count be 0
    count be count + 1
";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
}

#[test]
fn a_declaration_of_every_shape_produces_no_diagnostics() {
    let src = "\
public type Doc:
    title: String
    body: borrowed Doc

choice Result of (T, E):
    Ok(T)
    Err(E)

const WIDTH be 768
type Embedding is Array of F32

Doc implements Summarize:
    function summarize(self) -> String:
        truncate(self.body, 200)

Doc has:
    function is_empty(self) -> Bool:
        self.body.len() is 0

function report(docs: borrowed Array of any Summarize) -> String:
    for doc in docs:
        if doc.score >= 5 and doc.rank < 10:
            return try doc.summarize()
    \"\"
";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
}

#[test]
fn token_spans_never_go_backwards() {
    // A parser that walks the stream, and a renderer that groups diagnostics by
    // position, both rely on this. Error recovery must not break it.
    for src in [
        "a:\n  b:\n    c\n",
        "a:\n    b:\n        c\n  d\n",       // SC0004 recovery
        "function main():\n\tlet x be 1\n",   // SC0003 recovery
        "f(\n        a,\n  b,\n)\nx\n",
        "let a be \"open\nlet b be 'x\n",
        "let a be b? # SC0001 recovery\n",
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

// --- Ranges (§4.5) ---------------------------------------------------------

#[test]
fn ranges_are_two_dots_and_three_for_the_inclusive_one() {
    // §4.5: `0..n` is half-open, `0..=n` inclusive. Longest match, so `..=`
    // never comes out as `..` followed by `=`.
    assert_eq!(bare("0..n"), vec![int(0, Dec, None), DotDot, id("n")]);
    assert_eq!(bare("0..=n"), vec![int(0, Dec, None), DotDotEq, id("n")]);
    assert_eq!(bare("a..b"), vec![id("a"), DotDot, id("b")]);
    assert_eq!(bare(".."), vec![DotDot]);
    assert_eq!(bare("..="), vec![DotDotEq]);
}

#[test]
fn a_range_after_a_float_is_still_a_range() {
    // `number` stops at a `.` that no digit follows, so the float never eats
    // the first dot of the range. This is the case that would silently lex
    // wrong if the two scanners disagreed.
    assert_eq!(bare("1.0..2.0"), vec![float(1.0, None), DotDot, float(2.0, None)]);
    assert_eq!(bare("1..2"), vec![int(1, Dec, None), DotDot, int(2, Dec, None)]);
}

#[test]
fn a_single_dot_is_still_field_access() {
    // The range tokens must not have cost the language its `.`.
    assert_eq!(bare("doc.title"), vec![id("doc"), Dot, id("title")]);
    assert_eq!(bare("for i in 0..n:"),
        vec![For, id("i"), In, int(0, Dec, None), DotDot, id("n"), Colon]);
}

// --- Chains broken by a leading dot (§4.6) ---------------------------------

#[test]
fn a_line_beginning_with_a_dot_continues_the_line_above() {
    // §4.6: a chain broken this way is one logical line, so there is no
    // `Newline` between the links and no `Indent` for the indentation.
    let src = "docs\n    .iterate()\n    .collect()\n";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
    assert_eq!(
        kinds(src),
        vec![
            id("docs"),
            Dot, id("iterate"), LParen, RParen,
            Dot, id("collect"), LParen, RParen,
            Newline,
            Eof,
        ]
    );
}

#[test]
fn blank_and_comment_lines_inside_a_chain_take_no_part() {
    // They take no part in the indentation computation, so they take no part
    // here either: the chain is still one line.
    let src = "docs\n    .iterate()\n\n    # why we discard\n    .discard(each.empty())\n";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
    assert_eq!(
        kinds(src),
        vec![
            id("docs"),
            Dot, id("iterate"), LParen, RParen,
            // `each` is the keyword that introduces the implicit-subject
            // closure of §4.6, not an identifier.
            Dot, id("discard"), LParen, Each, Dot, id("empty"), LParen, RParen, RParen,
            Newline,
            Eof,
        ]
    );
}

#[test]
fn a_chain_continuation_needs_no_indentation_and_tolerates_a_tab() {
    // Like the inside of a bracket, the leading whitespace of a continuation
    // line means nothing, so a tab there is not the SC0003 of `start_of_line`.
    let src = "docs\n.collect()\n";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
    assert_eq!(
        kinds(src),
        vec![id("docs"), Dot, id("collect"), LParen, RParen, Newline, Eof]
    );
    assert!(codes("docs\n\t.collect()\n").is_empty());
}

#[test]
fn a_line_beginning_with_a_range_is_not_a_continuation() {
    // `..` is not a chain link. The line structure applies as usual, which
    // here means an `Indent`, because the parser must see the mistake.
    let src = "docs\n    ..n\n";
    assert_eq!(
        kinds(src),
        vec![id("docs"), Newline, Indent, DotDot, id("n"), Newline, Dedent, Eof]
    );
}

#[test]
fn a_dot_cannot_continue_a_line_that_produced_no_token() {
    // Nothing precedes it, so there is nothing to continue and the ordinary
    // line structure applies.
    let src = "    .collect()\n";
    assert_eq!(
        kinds(src),
        vec![Indent, Dot, id("collect"), LParen, RParen, Newline, Dedent, Eof]
    );
}

#[test]
fn a_chain_continuation_does_not_close_the_block_it_sits_in() {
    // The continuation is dedented relative to the body it belongs to, which
    // under the ordinary rule would emit a `Dedent`. It must not.
    let src = "function main():\n    let names be docs\n        .collect()\n    names\n";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
    assert_eq!(
        kinds(src),
        vec![
            Function, id("main"), LParen, RParen, Colon, Newline,
            Indent,
            Let, id("names"), Be, id("docs"),
            Dot, id("collect"), LParen, RParen,
            Newline,
            id("names"), Newline,
            Dedent,
            Eof,
        ]
    );
}

#[test]
fn the_power_operator_is_one_token_and_not_two_stars() {
    // §4.6 gives `**` its own precedence level, so it has to arrive as one
    // token. Science has no prefix `*`, so the longest match costs nothing.
    assert_eq!(bare("a ** b"), vec![id("a"), StarStar, id("b")]);
    assert_eq!(bare("a**b**c"), vec![id("a"), StarStar, id("b"), StarStar, id("c")]);
    assert_eq!(bare("a * b"), vec![id("a"), Star, id("b")]);
    // Three stars is `**` then `*`, which is a syntax error and not a
    // lexical one: the lexer takes the longest match and moves on.
    assert_eq!(bare("a *** b"), vec![id("a"), StarStar, Star, id("b")]);
}

#[test]
fn a_line_beginning_with_where_continues_the_signature_above() {
    // §4.4 writes a long signature with its bounds on the next line, indented.
    // Under the ordinary rule that indentation would open a block the body
    // could then never match, so `where` continues the line instead.
    let src = "function best_of of T(x: borrowed T) -> String\n        where T: Ord:\n    x.preview()\n";
    assert!(codes(src).is_empty(), "{:?}", messages(src));
    assert_eq!(
        kinds(src),
        vec![
            Function, id("best_of"), Of, id("T"),
            LParen, id("x"), Colon, Borrowed, id("T"), RParen,
            Arrow, id("String"),
            Where, id("T"), Colon, id("Ord"), Colon,
            Newline,
            Indent,
            id("x"), Dot, id("preview"), LParen, RParen, Newline,
            Dedent,
            Eof,
        ]
    );
}

#[test]
fn only_the_whole_word_where_continues_a_line() {
    // `whereabouts` is an identifier and starts a line of its own, block and
    // all. Matching a prefix here would swallow the next statement.
    let src = "a\n    whereabouts\n";
    assert_eq!(
        kinds(src),
        vec![id("a"), Newline, Indent, id("whereabouts"), Newline, Dedent, Eof]
    );
}

#[test]
fn a_where_on_the_same_line_is_an_ordinary_keyword() {
    // The continuation rule is about where the line breaks, not about `where`
    // itself: nothing changes when the clause fits on one line.
    assert_eq!(
        bare("-> String where T: Ord:"),
        vec![Arrow, id("String"), Where, id("T"), Colon, id("Ord"), Colon]
    );
}

#[test]
fn equation_is_reserved_and_its_neighbours_are_not() {
    // Reserved for the construct `equations.md` designs: an equation checked
    // dimensionally at compile time and rendered to the paper from the same
    // definition that ran. Reserving costs nothing today and is impossible
    // later, which is the whole argument.
    assert_eq!(bare("equation"), vec![Reserved(ReservedWord::Equation)]);

    // `formula` was rejected for the name and must stay an ordinary word:
    // `chem` spells a chemical formula that way, so reserving it would take a
    // noun from the audience it is for. `equations` and `hill_equation` are
    // different identifiers and the reservation does not reach them.
    assert_eq!(bare("formula"), vec![id("formula")]);
    assert_eq!(bare("equations"), vec![id("equations")]);
    assert_eq!(bare("hill_equation"), vec![id("hill_equation")]);
}

// --- doc comments ---------------------------------------------------------

/// `##` survives lexing and reaches the token it documents.
///
/// `strings-formatting-and-docs.md` §5.3 requires this in F0 and says why it
/// cannot wait: once the lexer discards them, everything downstream is built
/// assuming they are gone. It did discard them until this test existed.
#[test]
fn a_doc_comment_reaches_the_token_it_documents() {
    let t = tokens("## Lists the runs.\nfunction list_runs():\n    print(\"x\")\n");
    let documented: Vec<_> = t.iter().filter(|t| t.doc.is_some()).collect();
    assert_eq!(documented.len(), 1, "exactly one token carries the run");
    assert_eq!(documented[0].kind, Function, "and it is the declaration, not a newline");
    assert_eq!(documented[0].doc.as_deref(), Some("Lists the runs."));
}

/// A run of several lines joins with line feeds, and the blank `##` is kept.
#[test]
fn a_doc_run_joins_its_lines() {
    let t = tokens("## Summary line.\n##\n## Body paragraph.\nfunction f():\n    print(\"x\")\n");
    let doc = t.iter().find_map(|t| t.doc.as_deref()).unwrap();
    assert_eq!(doc, "Summary line.\n\nBody paragraph.");
}

/// An ordinary `#` comment is not documentation and leaves nothing behind.
#[test]
fn an_ordinary_comment_documents_nothing() {
    let t = tokens("# just a comment\nfunction f():\n    print(\"x\")\n");
    assert!(t.iter().all(|t| t.doc.is_none()));
}

/// `###` is documentation, because it is a Markdown heading inside one.
#[test]
fn three_hashes_are_still_documentation() {
    let t = tokens("### A heading\nfunction f():\n    print(\"x\")\n");
    assert_eq!(t.iter().find_map(|t| t.doc.as_deref()), Some("# A heading"));
}

/// Indentation inside a doc comment is the author's and is kept, because a
/// doc comment holds code samples that §5.5 makes compile.
#[test]
fn indentation_inside_a_doc_comment_survives() {
    let t = tokens("## Example:\n##     let x be 1\nfunction f():\n    print(\"x\")\n");
    let doc = t.iter().find_map(|t| t.doc.as_deref()).unwrap();
    assert_eq!(doc, "Example:\n    let x be 1");
}

/// A run is consumed by the token it documents and does not leak onto the
/// next declaration — the failure that would attach one function's
/// documentation to the one after it.
#[test]
fn a_doc_run_is_consumed_and_does_not_leak() {
    let t = tokens("## First.\nfunction a():\n    print(\"x\")\nfunction b():\n    print(\"y\")\n");
    let docs: Vec<_> = t.iter().filter_map(|t| t.doc.as_deref()).collect();
    assert_eq!(docs, vec!["First."], "the second function carries nothing");
}
