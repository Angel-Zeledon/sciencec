//! Tests for `SourceMap`: file registration and offset translation.

use link_diagnostics::source_map::{LineCol, SourceMap};
use link_diagnostics::{FileId, Span};

fn lc(line: u32, col: u32) -> LineCol {
    LineCol { line, col }
}

/// Shorthand: a map holding a single file.
fn map_of(text: &str) -> (SourceMap, FileId) {
    let mut m = SourceMap::new();
    let f = m.add_file("test.link".into(), text.into());
    (m, f)
}

#[test]
fn empty_file_has_one_line() {
    let (m, f) = map_of("");
    assert_eq!(m.line_count(f), 1);
    assert_eq!(m.line_col(Span::at(f, 0)), lc(1, 1));
    assert_eq!(m.line_text(f, 1), "");
    assert_eq!(m.text(f), "");
}

#[test]
fn each_file_gets_its_own_id() {
    let mut m = SourceMap::new();
    let a = m.add_file("a.link".into(), "one".into());
    let b = m.add_file("b.link".into(), "two".into());
    assert_ne!(a, b);
    assert_eq!(m.path(a), "a.link");
    assert_eq!(m.path(b), "b.link");
    assert_eq!(m.text(a), "one");
    assert_eq!(m.text(b), "two");
}

#[test]
fn columns_on_the_first_line() {
    let (m, f) = map_of("abc\ndef\n");
    assert_eq!(m.line_col(Span::at(f, 0)), lc(1, 1));
    assert_eq!(m.line_col(Span::at(f, 1)), lc(1, 2));
    assert_eq!(m.line_col(Span::at(f, 3)), lc(1, 4)); // right on the \n
}

#[test]
fn newlines_advance_the_line() {
    let (m, f) = map_of("abc\ndef\nghi");
    assert_eq!(m.line_col(Span::at(f, 4)), lc(2, 1));
    assert_eq!(m.line_col(Span::at(f, 6)), lc(2, 3));
    assert_eq!(m.line_col(Span::at(f, 8)), lc(3, 1));
    assert_eq!(m.line_count(f), 3);
}

#[test]
fn end_of_file_without_a_trailing_newline() {
    let (m, f) = map_of("abc\ndef");
    // Offset 7 == text length: the point just past the last `f`.
    assert_eq!(m.line_col(Span::at(f, 7)), lc(2, 4));
    assert_eq!(m.line_count(f), 2);
}

#[test]
fn a_trailing_newline_opens_an_empty_last_line() {
    let (m, f) = map_of("abc\n");
    assert_eq!(m.line_count(f), 2);
    assert_eq!(m.line_col(Span::at(f, 4)), lc(2, 1));
    assert_eq!(m.line_text(f, 2), "");
}

#[test]
fn offset_past_the_end_is_clamped() {
    let (m, f) = map_of("abc");
    assert_eq!(m.line_col(Span::at(f, 9999)), lc(1, 4));
}

#[test]
fn columns_count_characters_not_bytes() {
    // `é` takes two bytes; `=` is the 10th character, so column 10.
    let text = "let café = 1\n";
    let (m, f) = map_of(text);
    let byte = text.find('=').unwrap() as u32;
    // Counting bytes would give column 11 (`byte + 1`); the accent costs an
    // extra byte but only one character.
    assert_eq!(byte + 1, 11);
    assert_eq!(m.line_col(Span::at(f, byte)), lc(1, 10));
}

#[test]
fn an_emoji_before_the_span_counts_as_one_character() {
    let text = "# 🎉 party\nlet x = 1\n";
    let (m, f) = map_of(text);
    let byte = text.find("party").unwrap() as u32;
    assert_eq!(m.line_col(Span::at(f, byte)), lc(1, 5));
    let byte_x = text.find("x =").unwrap() as u32;
    assert_eq!(m.line_col(Span::at(f, byte_x)), lc(2, 5));
}

#[test]
fn an_offset_inside_a_character_does_not_panic() {
    let (m, f) = map_of("é");
    // Offset 1 lands inside the `é`; we walk back to the character start.
    assert_eq!(m.line_col(Span::at(f, 1)), lc(1, 1));
}

#[test]
fn line_text_drops_the_newline() {
    let (m, f) = map_of("one\ntwo\nthree");
    assert_eq!(m.line_text(f, 1), "one");
    assert_eq!(m.line_text(f, 2), "two");
    assert_eq!(m.line_text(f, 3), "three");
}

#[test]
fn line_text_out_of_range_is_empty() {
    let (m, f) = map_of("one\n");
    assert_eq!(m.line_text(f, 0), "");
    assert_eq!(m.line_text(f, 99), "");
}

#[test]
fn line_text_drops_the_carriage_return() {
    let (m, f) = map_of("one\r\ntwo\r\n");
    assert_eq!(m.line_text(f, 1), "one");
    assert_eq!(m.line_text(f, 2), "two");
}

#[test]
fn span_text_returns_the_source_of_the_range() {
    let text = "let café = 1";
    let (m, f) = map_of(text);
    let start = text.find("café").unwrap() as u32;
    let span = Span::new(f, start, start + "café".len() as u32);
    assert_eq!(m.span_text(span), "café");
    assert_eq!(m.span_text(Span::at(f, start)), "");
}

#[test]
fn span_text_clamps_out_of_range_ends() {
    let (m, f) = map_of("abc");
    assert_eq!(m.span_text(Span::new(f, 1, 900)), "bc");
}

#[test]
fn line_col_uses_the_start_and_line_col_end_the_end() {
    let (m, f) = map_of("one\ntwo\nthree");
    let span = Span::new(f, 1, 9);
    assert_eq!(m.line_col(span), lc(1, 2));
    assert_eq!(m.line_col_end(span), lc(3, 2));
}

#[test]
fn many_lines_translate_correctly() {
    // Each line is "line N". We check a handful spread across the file.
    let mut text = String::new();
    for i in 1..=500 {
        text.push_str(&format!("line {i}\n"));
    }
    let (m, f) = map_of(&text);
    assert_eq!(m.line_count(f), 501);
    for target in [1u32, 2, 37, 250, 499, 500] {
        let needle = format!("line {target}\n");
        let byte = text.find(&needle).unwrap() as u32;
        assert_eq!(m.line_col(Span::at(f, byte)), lc(target, 1));
    }
}
