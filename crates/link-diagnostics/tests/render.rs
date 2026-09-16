//! Tests for the diagnostic renderer.
//!
//! The format is compared literally: UI tests depend on it byte for byte, so
//! the important cases are spelled out here instead of only snapshotted.

use link_diagnostics::render::{render, render_all};
use link_diagnostics::source_map::SourceMap;
use link_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span, Suggestion};

/// Byte span of the first occurrence of `needle` in `text`.
fn span_of(f: FileId, text: &str, needle: &str) -> Span {
    let start = text.find(needle).unwrap_or_else(|| panic!("`{needle}` not found")) as u32;
    Span::new(f, start, start + needle.len() as u32)
}

const MOVED: &str = r#"fn main():
    let d = Doc(title: "a", body: "b")
    let x = 1
    let y = 2
    let z = 3
    let w = 4
    let v = 5
    let u = 6
    let t = 7
    let b = d

    print(d)
"#;

#[test]
fn full_diagnostic_matches_the_reference_layout() {
    let mut map = SourceMap::new();
    let f = map.add_file("src/main.link".into(), MOVED.into());

    let assignment = span_of(f, MOVED, "let b = d");
    let moved = Span::new(f, assignment.end - 1, assignment.end);
    let call = span_of(f, MOVED, "print(d)");
    let used = Span::new(f, call.start + 6, call.start + 7);

    let d = Diagnostic::error(Code(301), "use of a moved value")
        .with_label(Label::secondary(moved, "value moved here"))
        .with_label(Label::primary(used, "used here after the move"))
        .with_note("`Doc` does not implement `Copy`, so the assignment moves")
        .with_suggestion(Suggestion {
            span: moved,
            replacement: "d.clone()".into(),
            message: "clone the value if you need to keep the original".into(),
        });

    let expected = "\
error[LK0301]: use of a moved value
  --> src/main.link:12:11
   |
10 |     let b = d
   |             - value moved here
11 |
12 |     print(d)
   |           ^ used here after the move
   |
   = note: `Doc` does not implement `Copy`, so the assignment moves
   = help: clone the value if you need to keep the original: `d.clone()`";

    assert_eq!(render(&map, &d), expected);
}

#[test]
fn a_diagnostic_without_labels_is_only_a_header() {
    let map = SourceMap::new();
    let d = Diagnostic::error(Code(402), "linking failed")
        .with_note("the system linker reported an unresolved symbol");

    let expected = "\
error[LK0402]: linking failed
  = note: the system linker reported an unresolved symbol";

    assert_eq!(render(&map, &d), expected);
}

#[test]
fn an_empty_span_gets_a_single_caret() {
    let mut map = SourceMap::new();
    let text = "fn main()\n";
    let f = map.add_file("t.link".into(), text.into());
    let d = Diagnostic::error(Code(110), "expected `:` after the signature")
        .with_label(Label::primary(Span::at(f, 9), "insert `:` here"));

    let expected = "\
error[LK0110]: expected `:` after the signature
 --> t.link:1:10
  |
1 | fn main()
  |          ^ insert `:` here";

    assert_eq!(render(&map, &d), expected);
}

#[test]
fn several_labels_on_one_line_are_grouped_under_it() {
    let mut map = SourceMap::new();
    let text = "let total = first + second\n";
    let f = map.add_file("t.link".into(), text.into());
    let d = Diagnostic::error(Code(220), "mismatched types in `+`")
        .with_label(Label::primary(span_of(f, text, "second"), "this is a `String`"))
        .with_label(Label::secondary(span_of(f, text, "first"), "this is an `Int`"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn a_multiline_span_marks_its_first_line() {
    let mut map = SourceMap::new();
    let text = "fn f():\n    let x = compute(\n        1,\n        2,\n    )\n    x\n";
    let f = map.add_file("t.link".into(), text.into());
    let start = text.find("compute").unwrap() as u32;
    let end = text.find(")\n    x").unwrap() as u32 + 1;
    let d = Diagnostic::error(Code(230), "too many arguments")
        .with_label(Label::primary(Span::new(f, start, end), "this call takes 1 argument"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn distant_lines_are_elided_and_a_single_gap_is_printed() {
    let mut map = SourceMap::new();
    let mut text = String::new();
    for i in 1..=30 {
        text.push_str(&format!("let v{i} = {i}\n"));
    }
    let f = map.add_file("wide.link".into(), text.clone());
    let d = Diagnostic::warning(Code(205), "unused bindings")
        .with_label(Label::secondary(span_of(f, &text, "v2 "), "bound here"))
        .with_label(Label::secondary(span_of(f, &text, "v4 "), "and here"))
        .with_label(Label::primary(span_of(f, &text, "v28 "), "never used"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn a_deleting_suggestion_shows_the_text_it_removes() {
    let mut map = SourceMap::new();
    let text = "let mut x = 1\n";
    let f = map.add_file("t.link".into(), text.into());
    let d = Diagnostic::warning(Code(206), "variable does not need to be mutable")
        .with_label(Label::primary(span_of(f, text, "mut"), "never reassigned"))
        .with_suggestion(Suggestion {
            span: span_of(f, text, "mut "),
            replacement: String::new(),
            message: "remove `mut`".into(),
        });

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn labels_in_another_file_get_their_own_header() {
    let mut map = SourceMap::new();
    let a = map.add_file("a.link".into(), "fn helper(x: Int):\n    x\n".into());
    let b = map.add_file("b.link".into(), "fn main():\n    helper(\"hi\")\n".into());
    let d = Diagnostic::error(Code(221), "mismatched types")
        .with_label(Label::primary(Span::new(b, 22, 26), "expected `Int`, found `String`"))
        .with_label(Label::secondary(Span::new(a, 10, 16), "parameter declared here"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn render_all_orders_by_file_and_position() {
    let mut map = SourceMap::new();
    let a = map.add_file("a.link".into(), "one\ntwo\nthree\n".into());
    let b = map.add_file("b.link".into(), "alpha\nbeta\n".into());

    let mut diags = Diagnostics::new();
    diags.push(
        Diagnostic::error(Code(2), "later in b")
            .with_label(Label::primary(Span::new(b, 6, 10), "here")),
    );
    diags.push(
        Diagnostic::error(Code(3), "later in a")
            .with_label(Label::primary(Span::new(a, 8, 13), "here")),
    );
    diags.push(Diagnostic::error(Code(4), "no labels at all"));
    diags.push(
        Diagnostic::error(Code(1), "earlier in a")
            .with_label(Label::primary(Span::new(a, 0, 3), "here")),
    );

    insta::assert_snapshot!(render_all(&map, &diags));
}

#[test]
fn render_all_of_nothing_is_empty() {
    let map = SourceMap::new();
    assert_eq!(render_all(&map, &Diagnostics::new()), "");
}

#[test]
fn accented_text_before_the_span_keeps_the_caret_aligned() {
    let mut map = SourceMap::new();
    let text = "let café = \"☕\"\nprint(café)\n";
    let f = map.add_file("t.link".into(), text.into());
    let start = text.rfind("café").unwrap() as u32;
    let d = Diagnostic::error(Code(310), "borrow of a moved value")
        .with_label(Label::primary(Span::new(f, start, start + 5), "borrowed here"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn a_span_at_the_end_of_a_file_without_a_newline_is_rendered() {
    let mut map = SourceMap::new();
    let text = "fn main():\n    let x = ";
    let f = map.add_file("t.link".into(), text.into());
    let d = Diagnostic::error(Code(120), "expected an expression")
        .with_label(Label::primary(Span::at(f, text.len() as u32), "the file ends here"));

    insta::assert_snapshot!(render(&map, &d));
}

#[test]
fn a_warning_says_warning() {
    let mut map = SourceMap::new();
    let text = "let x = 1\n";
    let f = map.add_file("t.link".into(), text.into());
    let d = Diagnostic::warning(Code(207), "unused variable `x`")
        .with_label(Label::primary(span_of(f, text, "x"), "unused"));

    insta::assert_snapshot!(render(&map, &d));
}
