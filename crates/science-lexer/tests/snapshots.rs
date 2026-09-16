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
function main():
    if a:
        loop:
            c()
"
    ));
}

#[test]
fn several_levels_close_at_once() {
    insta::assert_snapshot!(dump(
        "\
function main():
    if a:
        loop:
            c()
done()
"
    ));
}

#[test]
fn blank_and_comment_lines_between_blocks() {
    insta::assert_snapshot!(dump(
        "\
function main():
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
function main():
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
    insta::assert_snapshot!(dump("function main():\n\tlet x be 1\n\tlet y be 2\n"));
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
let a be \"open
let b be 'x
let c be \"bad \\q escape\"
let d be 340282366920938463463374607431768211456
let e be 0b1012
"
    ));
}

/// Every operator and punctuation mark in one line.
///
/// `==` and `!=` stay in it although §1 removed them from the language: they
/// are still lexed, and the snapshot is where their two diagnostics — one
/// each, and nothing over the ordering symbols beside them — are visible.
#[test]
fn operators_and_punctuation() {
    insta::assert_snapshot!(dump("+ - * / % & | ^ << >> = == != < > <= >= => _ @ . , : ;\n"));
}

#[test]
fn unknown_characters_do_not_stop_the_lexer() {
    insta::assert_snapshot!(dump("let x be 1 $ 2\nlet y be `z`\n"));
}

/// The `interface Summarize` example from section 4.3 of the design spec.
#[test]
fn interface_summarize_program() {
    insta::assert_snapshot!(dump(
        "\
interface Summarize:
    function summarize(self) -> String

    function preview(self) -> String:
        truncate(self.summarize(), 80)

Doc implements Summarize:
    function summarize(self) -> String:
        truncate(self.body, 200)
"
    ));
}

/// A program that exercises most of the grammar at once.
#[test]
fn mixed_program() {
    insta::assert_snapshot!(dump(
        "\
use text.parser (Token, lex)

public type Doc:
    title: String
    body: String

function largest of T(items: borrowed Array of T) -> borrowed T where T: Ord:
    let mutable best be items.get(0)
    for item in items:
        if item > best: best be item
    best

function read_config(path: borrowed String) -> (Config, Error?):
    let text, err be read_file(path)   # the error comes back beside the value
    if err?:
        return (Config.empty(), err)
    parse(borrowed text)
"
    ));
}
