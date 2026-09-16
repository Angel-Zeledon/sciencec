//! Statements: `let`, assignment, `return`, `break`, `continue`, and the
//! expression statement, plus how a block picks its tail.

mod common;
use common::{parse_body, parse_source_allowing_errors, strip_spans};

/// `let`, with and without `mut`, with and without an annotation.
#[test]
fn let_bindings() {
    insta::assert_snapshot!(parse_body(
        r#"fn main():
    let greeting = "hola"
    let mut count = 0
    let annotated: Int = 42
    let mut typed: &mut String = &mut owner
"#
    ));
}

/// §4.4: "assignment is not in this table because it is not an expression."
/// The parser parses an expression and, on finding a `=`, makes the whole
/// thing an assignment statement.
#[test]
fn assignment_is_a_statement() {
    insta::assert_snapshot!(parse_body(
        r#"fn main():
    a = b
    a = a + 1
    self.text = ""
    editor.target.title = "edited"
"#
    ));
}

/// The right-hand side of an assignment is a full expression, including an
/// inline `if`.
#[test]
fn assignment_takes_a_whole_expression() {
    insta::assert_snapshot!(parse_body("fn main():\n    a = if flag: 1 else: 2\n"));
}

/// An assignment inside an inline body. §4.2 rejects a *statement* there, but
/// §4.3 writes this very line — `if item > best: best = item` — so assignment
/// is not the statement it means; see the note on `parse_inline_block`.
#[test]
fn an_assignment_is_allowed_in_an_inline_body() {
    insta::assert_snapshot!(parse_body(
        r#"fn largest(items: &Array[Int]) -> Int:
    let mut best = 0
    for item in items:
        if item > best: best = item
    best
"#
    ));
}

/// `return` with and without a value, `break` with and without one, and
/// `continue`.
#[test]
fn jumps() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(items: &Array[Int]) -> Option[Int]:
    for item in items:
        if item % 2 != 0:
            continue
        return Some(item)
    loop:
        break
    return
"#
    ));
}

/// `return` and `break` in an inline body, which the corpus writes as
/// `if n < 0: return -1` and `loop: break`.
#[test]
fn jumps_in_an_inline_body() {
    insta::assert_snapshot!(parse_body(
        r#"fn sign(n: Int) -> Int:
    if n < 0: return -1
    if n > 0: return 1
    loop: break
    0
"#
    ));
}

/// A block's value is its last expression (§4.3), so the last statement, when
/// it is an expression, becomes the block's tail rather than a statement.
#[test]
fn the_last_expression_becomes_the_blocks_tail() {
    insta::assert_snapshot!(parse_body(
        "fn add(a: Int, b: Int) -> Int:\n    let c = a + b\n    c\n"
    ));
}

/// A block whose last statement is not an expression has no tail.
#[test]
fn a_block_ending_in_a_statement_has_no_tail() {
    insta::assert_snapshot!(parse_body("fn f():\n    let a = 1\n    a = 2\n"));
}

/// A statement ending in an indented block needs no newline of its own: the
/// block's `DEDENT` is what ends it.
#[test]
fn a_statement_ending_in_a_block_needs_no_newline() {
    insta::assert_snapshot!(parse_body(
        r#"fn f():
    if c:
        a
    b
    while d:
        e
    f
"#
    ));
}

/// Deep nesting closed by several `DEDENT`s at once, which is the case a naive
/// block loop gets wrong.
#[test]
fn many_levels_closed_at_once() {
    insta::assert_snapshot!(parse_body(
        r#"fn f() -> Int:
    for row in rows:
        for cell in row:
            if cell != 0:
                if cell > 0:
                    hits = hits + 1
    hits
"#
    ));
}

/// A `let` with no initialiser is rejected, and the next statement still
/// parses: one pass reports more than one problem.
#[test]
fn a_let_without_a_value_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "fn f():\n    let a: Int\n    let b = 1\n"
    ));
}

/// Two statements crammed onto one line is an error at the second one.
#[test]
fn two_statements_on_one_line_are_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("fn f():\n    let a = 1 let b = 2\n"));
}

/// The inline and the block form of a function body differ only in the spans.
#[test]
fn inline_and_block_bodies_agree() {
    let inline = parse_body("fn f() -> Int: 1\n");
    let block = parse_body("fn f() -> Int:\n    1\n");
    assert_eq!(strip_spans(&inline), strip_spans(&block));
}
