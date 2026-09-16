//! Snapshot tests over whole token streams.
//!
//! Unit tests pin down single behaviours; these pin down the shape of the
//! entire stream, which is where indentation regressions actually show up.

mod common;

use common::dump;

#[test]
fn empty_file() {
    insta::assert_snapshot!(dump(""));
}

#[test]
fn comments_only() {
    insta::assert_snapshot!(dump("# a header\n\n      # an indented comment\n"));
}

#[test]
fn three_level_nesting() {
    insta::assert_snapshot!(dump(
        "\
fn main():
    if a:
        while b:
            c()
"
    ));
}

#[test]
fn several_levels_close_at_once() {
    insta::assert_snapshot!(dump(
        "\
fn main():
    if a:
        while b:
            c()
done()
"
    ));
}

#[test]
fn blank_and_comment_lines_between_blocks() {
    insta::assert_snapshot!(dump(
        "\
fn main():
    a

            # not an indent

    b
"
    ));
}

#[test]
fn inconsistent_dedent() {
    insta::assert_snapshot!(dump(
        "\
a:
    b:
        c
  d
"
    ));
}

#[test]
fn bracket_continuation_with_misleading_indentation() {
    insta::assert_snapshot!(dump(
        "\
fn main():
    call(
            first,
  second,
        [1, 2,
         3],
    )
    after
"
    ));
}

#[test]
fn tab_in_the_indentation() {
    insta::assert_snapshot!(dump("fn main():\n\tlet x = 1\n\tlet y = 2\n"));
}

#[test]
fn integer_literals() {
    insta::assert_snapshot!(dump("42 1_000_000 0xFF 0xdead_beef 0o777 0b1010 42i32 7u8\n"));
}

#[test]
fn float_literals() {
    insta::assert_snapshot!(dump("3.14 1e-9 2.5f32 1E9 1_000.5 1.foo()\n"));
}

#[test]
fn string_and_character_literals() {
    insta::assert_snapshot!(dump(
        "\"hello\" \"a\\nb\" \"\\t\\r\\\\\\\"\\0\" \"\\u{1F600}\" 'a' '\\n'\n"
    ));
}

#[test]
fn literal_errors() {
    insta::assert_snapshot!(dump(
        "\
let a = \"open
let b = 'x
let c = \"bad \\q escape\"
let d = 340282366920938463463374607431768211456
let e = 0b1012
"
    ));
}

#[test]
fn operators_and_punctuation() {
    insta::assert_snapshot!(dump("+ - * / % & | ^ << >> = == != < > <= >= -> => ? _ @ . , : ;\n"));
}

#[test]
fn unknown_characters_do_not_stop_the_lexer() {
    insta::assert_snapshot!(dump("let x = 1 $ 2\nlet y = `z`\n"));
}

/// The `trait Summarize` example from section 4.3 of the design spec.
#[test]
fn trait_summarize_program() {
    insta::assert_snapshot!(dump(
        "\
trait Summarize:
    fn summarize(&self) -> String

    fn preview(&self) -> String:
        self.summarize().truncate(80)

impl Summarize for Doc:
    fn summarize(&self) -> String:
        self.body.truncate(200)
"
    ));
}

/// A program that exercises most of the grammar at once.
#[test]
fn mixed_program() {
    insta::assert_snapshot!(dump(
        "\
use text.parser (Token, lex)

struct Doc:
    title: String
    body: String

fn largest[T: Ord](items: &Array[T]) -> &T:
    let mut best = items.get(0)
    for item in items:
        if item > best: best = item
    best

fn read_config(path: &String) -> Result[Config, Error]:
    let text = read_file(path)?   # early return on error
    match parse(&text):
        Ok(value): value
        Err(e): panic(e)
"
    ));
}
