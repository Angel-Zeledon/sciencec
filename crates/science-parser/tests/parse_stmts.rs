//! Statements: `let`, assignment, `return`, `break`, `continue`, and the
//! expression statement, plus how a block picks its tail.

mod common;
use common::{parse_body, parse_source_allowing_errors, strip_spans};

/// `let`, with and without `mutable`, with and without an annotation. The
/// annotation goes before the value, and the value after `be` (§4.3).
#[test]
fn let_bindings() {
    insta::assert_snapshot!(parse_body(
        r#"def main():
    let greeting be "hola"
    let mutable count be 0
    let annotated: Int be 42
    let mutable typed: mutable borrowed String be mutable borrowed owner
"#
    ));
}

/// §4.6: "assignment is not in this table because it is not an expression."
/// The parser parses an expression and, on finding a following `be`, makes the
/// whole thing an assignment statement — to a name, to a field, or to an
/// index.
#[test]
fn assignment_is_a_statement() {
    insta::assert_snapshot!(parse_body(
        r#"def main():
    a be b
    a be a + 1
    self.text be ""
    editor.target.title be "edited"
    cells[0] be 1
    grid[row][column] be cells[0]
"#
    ));
}

/// `=` does not assign in Science (§4.3): it survives only as the tail of
/// `<=`, `>=`, `=>` and `..=`. After an expression it can only be a missed
/// `be`, and
/// the diagnostic says that and offers the one-token fix. The snapshot format
/// does not render suggestions, so the fix is asserted directly.
#[test]
fn an_equals_sign_is_not_assignment() {
    let source = "def main():\n    a = 3\n    a be 4\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (tokens, _) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let diagnostic =
        diagnostics.iter().next().expect("`a = 3` should have produced a diagnostic");
    assert_eq!(diagnostic.message, "assignment is written `be`");
    let fix = diagnostic.suggestions.first().expect("the diagnostic should offer a fix");
    assert_eq!(fix.replacement, "be");
    assert_eq!(&source[fix.span.start as usize..fix.span.end as usize], "=");
}

/// The right-hand side of an assignment is a full expression, including an
/// inline `if`.
#[test]
fn assignment_takes_a_whole_expression() {
    insta::assert_snapshot!(parse_body("def main():\n    a be if flag: 1 else: 2\n"));
}

/// An assignment inside an inline body. §4.5 rejects a *statement* there, but
/// the spec writes this very line — `if item > best: best be item` — so
/// assignment is not the statement it means; see the note on
/// `parse_inline_block`.
#[test]
fn an_assignment_is_allowed_in_an_inline_body() {
    insta::assert_snapshot!(parse_body(
        r#"def largest(items: borrowed Array of Int) -> Int:
    let mutable best be 0
    for item in items:
        if item > best: best be item
    best
"#
    ));
}

/// A `let` is the one statement an inline body may not be (`SC0109`): it binds
/// a name in a scope, and an inline body has none.
#[test]
fn a_let_in_an_inline_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f(flag: Bool):\n    if flag: let x be 1\n"
    ));
}

/// `return` with and without a value, `break` with and without one, and
/// `continue`.
#[test]
fn jumps() {
    insta::assert_snapshot!(parse_body(
        r#"def f(items: borrowed Array of Int) -> Option of Int:
    for item in items:
        if item % 2 is not 0:
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
        r#"def sign(n: Int) -> Int:
    if n < 0: return -1
    if n > 0: return 1
    loop: break
    0
"#
    ));
}

/// The counting loop of §4.5: `0..n` stops one short of `n`, `0..=n` does not.
/// The range is an ordinary expression in the iterator position.
#[test]
fn for_over_a_range() {
    insta::assert_snapshot!(parse_body(
        r#"def f(n: Int) -> Int:
    let mutable total be 0
    for i in 0..n:
        total be total + i
    for i in 0..=n:
        total be total + i
    total
"#
    ));
}

/// A block's value is its last expression (§4.5), so the last statement, when
/// it is an expression, becomes the block's tail rather than a statement.
#[test]
fn the_last_expression_becomes_the_blocks_tail() {
    insta::assert_snapshot!(parse_body(
        "def add(a: Int, b: Int) -> Int:\n    let c be a + b\n    c\n"
    ));
}

/// A block whose last statement is not an expression has no tail.
#[test]
fn a_block_ending_in_a_statement_has_no_tail() {
    insta::assert_snapshot!(parse_body("def f():\n    let a be 1\n    a be 2\n"));
}

/// A statement ending in an indented block needs no newline of its own: the
/// block's `DEDENT` is what ends it.
#[test]
fn a_statement_ending_in_a_block_needs_no_newline() {
    insta::assert_snapshot!(parse_body(
        r#"def f():
    if c:
        a
    b
    loop:
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
        r#"def f() -> Int:
    for row in rows:
        for cell in row:
            if cell is not 0:
                if cell > 0:
                    hits be hits + 1
    hits
"#
    ));
}

/// A `let` with no value is rejected, and the next statement still parses: one
/// pass reports more than one problem.
#[test]
fn a_let_without_a_value_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f():\n    let a: Int\n    let b be 1\n"
    ));
}

/// Two statements crammed onto one line is an error at the second one.
#[test]
fn two_statements_on_one_line_are_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f():\n    let a be 1 let b be 2\n"
    ));
}

/// The inline and the block form of a function body differ only in the spans.
#[test]
fn inline_and_block_bodies_agree() {
    let inline = parse_body("def f() -> Int: 1\n");
    let block = parse_body("def f() -> Int:\n    1\n");
    assert_eq!(strip_spans(&inline), strip_spans(&block));
}

/// `return` and `break` decide whether they carry a value by asking
/// `starts_expr`, so an array literal after either one is the case that says
/// `[` is in that list.
///
/// This is half of the bug the literal was added to fix: before `[` was in
/// `starts_expr`, `return [1, 2]` returned *nothing* and then failed on the
/// `[` it had left behind, which is one mistake and two diagnostics.
#[test]
fn return_and_break_carry_an_array_literal() {
    insta::assert_snapshot!(parse_body(
        "def pick(ready: Bool) -> Array of Int:
    if ready: return [0]
    loop:
        break [1, 2]
"
    ));
}

/// `assert(cond)` and `assert(cond, message)` — spelled and parsed like a
/// call, but `TokenKind::Assert`'s own statement, not a call to a name.
#[test]
fn assert_statement() {
    insta::assert_snapshot!(parse_body(
        r#"def f(count: Int):
    assert(count >= 0)
    assert(count < 100, "count out of range")
"#
    ));
}

/// An `assert` with no message reads naturally in an inline body, the same
/// way `return`/`break`/`continue` do — it binds no name, so `let`'s
/// objection to an inline body does not apply to it.
#[test]
fn assert_in_an_inline_body() {
    insta::assert_snapshot!(parse_body(
        "def f(n: Int):
    if n > 0: assert(n > 0)
"
    ));
}

/// `assert` is a statement, not a call, but it is still spelled with
/// parentheses — leaving them off is the generic "expected `(`", the same
/// diagnostic any other required token misses with.
#[test]
fn assert_without_parentheses_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f(ready: Bool):
    assert ready
"
    ));
}
