//! Expressions: §4.4's precedence table, the postfix chain, and every
//! expression form the grammar has.
//!
//! The precedence tests are plain assertions over a span-stripped dump rather
//! than snapshots. A precedence rule is a claim about *shape* — that
//! `1 + 2 * 3` groups as `1 + (2 * 3)` — and a claim is clearer written down
//! than diffed.

mod common;
use common::{parse_body, parse_source, parse_source_allowing_errors, shape_of_expr};

/// `expr` parses to exactly `shape`, ignoring spans.
#[track_caller]
fn assert_shape(expr: &str, shape: &str) {
    let expected: String = shape
        .trim_matches('\n')
        .lines()
        .map(|line| line.strip_prefix("        ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(shape_of_expr(expr).trim_end(), expected.trim_end(), "while parsing `{expr}`");
}

// --- one row of the precedence table at a time ---------------------------

/// The top row: call, index, field access and `?` bind tighter than anything.
#[test]
fn postfix_binds_tighter_than_unary() {
    assert_shape(
        "-point.x",
        "
        Unary `-`
          Field `x`
            base: Path `point`
        ",
    );
    assert_shape(
        "not flag.enabled",
        "
        Unary `not`
          Field `enabled`
            base: Path `flag`
        ",
    );
    assert_shape(
        "&point.x",
        "
        Ref
          Field `x`
            base: Path `point`
        ",
    );
    assert_shape(
        "&mut owner.y",
        "
        Ref mut
          Field `y`
            base: Path `owner`
        ",
    );
}

/// Unary binds tighter than `as`: `-a as F64` is `(-a) as F64`.
#[test]
fn unary_binds_tighter_than_as() {
    assert_shape(
        "-a as F64",
        "
        Cast
          expr: Unary `-`
            Path `a`
          type: Path `F64`
        ",
    );
}

/// `as` binds tighter than `*`, and chains left to right.
#[test]
fn as_binds_tighter_than_multiplication() {
    assert_shape(
        "a as F64 * 2.0",
        "
        Binary `*`
          lhs: Cast
            expr: Path `a`
            type: Path `F64`
          rhs: Float 2
        ",
    );
    assert_shape(
        "a as I32 as I64",
        "
        Cast
          expr: Cast
            expr: Path `a`
            type: Path `I32`
          type: Path `I64`
        ",
    );
}

/// `* / %` share a row, above `+ -`.
#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_shape(
        "1 + 2 * 3",
        "
        Binary `+`
          lhs: Int 1
          rhs: Binary `*`
            lhs: Int 2
            rhs: Int 3
        ",
    );
    // `%` sits with `*` and `/`, and the row is left-associative.
    assert_shape(
        "1 + 10 % 4 * 2",
        "
        Binary `+`
          lhs: Int 1
          rhs: Binary `*`
            lhs: Binary `%`
              lhs: Int 10
              rhs: Int 4
            rhs: Int 2
        ",
    );
}

/// Parentheses override precedence, and leave no node of their own behind.
#[test]
fn parentheses_group_without_a_node() {
    assert_shape(
        "(1 + 2) * 3",
        "
        Binary `*`
          lhs: Binary `+`
            lhs: Int 1
            rhs: Int 2
          rhs: Int 3
        ",
    );
}

/// `+ -` above `<< >>` above `&` above `^` above `|`.
///
/// This is the example `examples/12_operators.science` spells out in a comment:
/// `1 + 1 << 2 | 0b1 ^ 0b10 & 0b11` is `((1 + 1) << 2) | (0b1 ^ (0b10 & 0b11))`.
#[test]
fn the_bitwise_rows_stack_in_order() {
    assert_shape(
        "1 + 1 << 2 | 0b1 ^ 0b10 & 0b11",
        "
        Binary `|`
          lhs: Binary `<<`
            lhs: Binary `+`
              lhs: Int 1
              rhs: Int 1
            rhs: Int 2
          rhs: Binary `^`
            lhs: Int 1 bin
            rhs: Binary `&`
              lhs: Int 2 bin
              rhs: Int 3 bin
        ",
    );
}

/// Arithmetic binds tighter than comparison.
#[test]
fn arithmetic_binds_tighter_than_comparison() {
    assert_shape(
        "a + b < a * b",
        "
        Binary `<`
          lhs: Binary `+`
            lhs: Path `a`
            rhs: Path `b`
          rhs: Binary `*`
            lhs: Path `a`
            rhs: Path `b`
        ",
    );
}

/// Comparison above `and` above `or`.
#[test]
fn and_binds_tighter_than_or() {
    assert_shape(
        "a < b or a > b and flag",
        "
        Binary `or`
          lhs: Binary `<`
            lhs: Path `a`
            rhs: Path `b`
          rhs: Binary `and`
            lhs: Binary `>`
              lhs: Path `a`
              rhs: Path `b`
            rhs: Path `flag`
        ",
    );
}

/// Every binary row is left-associative, so a repeated operator nests left.
#[test]
fn binary_operators_are_left_associative() {
    for op in ["-", "/", "%", "<<", "&", "^", "|", "and", "or", "=="] {
        assert_shape(
            &format!("a {op} b {op} c"),
            &format!(
                "
        Binary `{op}`
          lhs: Binary `{op}`
            lhs: Path `a`
            rhs: Path `b`
          rhs: Path `c`
        "
            ),
        );
    }
}

/// Every comparison operator parses, and they share one row.
#[test]
fn every_comparison_operator_parses() {
    for op in ["==", "!=", "<", ">", "<=", ">="] {
        assert_shape(
            &format!("a + 1 {op} b"),
            &format!(
                "
        Binary `{op}`
          lhs: Binary `+`
            lhs: Path `a`
            rhs: Int 1
          rhs: Path `b`
        "
            ),
        );
    }
}

/// `&` is both the reference operator and bitwise and. Nothing but position
/// tells them apart: in front of an operand it is a reference, between two it
/// is an operator.
#[test]
fn ampersand_is_a_reference_in_prefix_position_and_an_operator_between_operands() {
    assert_shape(
        "&a",
        "
        Ref
          Path `a`
        ",
    );
    assert_shape(
        "a & b",
        "
        Binary `&`
          lhs: Path `a`
          rhs: Path `b`
        ",
    );
    // Both at once: a reference on the right of the binary operator.
    assert_shape(
        "a & &b",
        "
        Binary `&`
          lhs: Path `a`
          rhs: Ref
            Path `b`
        ",
    );
    assert_shape(
        "&mut a",
        "
        Ref mut
          Path `a`
        ",
    );
}

// --- the postfix chain ---------------------------------------------------

/// `?` is in the tightest row, so it applies to what precedes it and the chain
/// continues through it.
#[test]
fn try_chains_with_field_access_and_calls() {
    assert_shape(
        "read_config(path)?.port + 1u16",
        "
        Binary `+`
          lhs: Field `port`
            base: Try
              Call
                callee: Path `read_config`
                args
                  Path `path`
          rhs: Int 1 u16
        ",
    );
    assert_shape(
        "a?.b()?.c?",
        "
        Try
          Field `c`
            base: Try
              Method `b`
                receiver: Try
                  Path `a`
        ",
    );
}

/// A method chain is left-nested: each call's receiver is everything so far.
#[test]
fn method_chains_nest_to_the_left() {
    assert_shape(
        "self.summarize().truncate(80)",
        "
        Method `truncate`
          receiver: Method `summarize`
            receiver: Self
          args
            Int 80
        ",
    );
    assert_shape(
        "registry.points.get(0).y",
        "
        Field `y`
          base: Method `get`
            receiver: Field `points`
              base: Path `registry`
            args
              Int 0
        ",
    );
}

/// Indexing is postfix and composes with field access in either order.
#[test]
fn indexing_composes_with_field_access() {
    assert_shape(
        "registry.points[0].x",
        "
        Field `x`
          base: Index
            base: Field `points`
              base: Path `registry`
            index: Int 0
        ",
    );
}

// --- calls, struct literals and the ambiguity between them ---------------

/// §4.4: "named arguments mean a struct, positional arguments mean a call or
/// variant." That is the whole rule, and it is decided here and nowhere else.
#[test]
fn named_arguments_make_a_struct_literal_and_positional_ones_a_call() {
    assert_shape(
        "Doc(title: \"a\", body: \"b\")",
        "
        StructLit `Doc`
          fields
            FieldInit `title`
              value: Str \"a\"
            FieldInit `body`
              value: Str \"b\"
        ",
    );
    assert_shape(
        "f(1, 2)",
        "
        Call
          callee: Path `f`
          args
            Int 1
            Int 2
        ",
    );
}

/// The struct may be named through a path, in which case the named arguments
/// still make it a construction rather than a method call.
#[test]
fn a_qualified_name_with_named_arguments_is_still_a_struct_literal() {
    assert_shape(
        "text.Doc(title: \"a\")",
        "
        StructLit `text.Doc`
          fields
            FieldInit `title`
              value: Str \"a\"
        ",
    );
}

/// §4.5: `Some(x)` and `Option.Some(x)` are the same thing. The qualified form
/// is written exactly like a method call, so the parser produces one and leaves
/// resolution to reclassify it — the same answer §4.3's `Doc.new("a")` gets.
#[test]
fn a_qualified_variant_is_a_method_call_until_resolution() {
    assert_shape(
        "Option.Some(x)",
        "
        Method `Some`
          receiver: Path `Option`
          args
            Path `x`
        ",
    );
}

/// §4.4 names `Doc()` as undecidable and settles it in favour of the variant
/// form, leaving resolution to reclassify. The parser must not invent a rule.
#[test]
fn an_empty_argument_list_parses_as_a_call() {
    assert_shape(
        "Doc()",
        "
        Call
          callee: Path `Doc`
        ",
    );
}

/// A struct literal nested in a call argument, and a call nested in a field's
/// value: neither form leaks into the other.
#[test]
fn struct_literals_and_calls_nest_in_each_other() {
    assert_shape(
        "println(Doc(title: f(1), body: g()))",
        "
        Call
          callee: Path `println`
          args
            StructLit `Doc`
              fields
                FieldInit `title`
                  value: Call
                    callee: Path `f`
                    args
                      Int 1
                FieldInit `body`
                  value: Call
                    callee: Path `g`
        ",
    );
}

/// Trailing commas are allowed in every bracketed list (§4.5).
#[test]
fn trailing_commas_are_allowed() {
    insta::assert_snapshot!(parse_source(
        r#"fn main():
    let a = f(
        1,
        2,
    )
    let b = Doc(
        title: "a",
    )
    let c = (1, 2,)
"#
    ));
}

// --- primaries -----------------------------------------------------------

/// Tuples, unit, and the fact that `(e)` is only grouping.
#[test]
fn tuple_unit_and_grouping() {
    assert_shape(
        "(1, 2, 3)",
        "
        Tuple
          Int 1
          Int 2
          Int 3
        ",
    );
    assert_shape("()", "Unit");
    assert_shape("(a)", "Path `a`");
}

/// Every literal form reaches the tree with the lexer's own representation.
#[test]
fn literals_keep_their_base_and_suffix() {
    insta::assert_snapshot!(parse_body(
        r#"fn main():
    let a = 42
    let b = 0xFF
    let c = 0b1010
    let d = 0o777
    let e = 42i32
    let f = 3.14
    let g = 2.5f32
    let h = "text"
    let i = 'x'
    let j = true
    let k = false
"#
    ));
}

/// `self` is its own expression, and the chain starts from it.
#[test]
fn self_is_an_expression() {
    assert_shape(
        "self.body",
        "
        Field `body`
          base: Self
        ",
    );
}

/// A name may carry generic arguments before an associated function is called
/// through it: `Array[Int].new()` (§4.3). §8 leaves the library collections
/// with no indexing operator, so a `[` after a name is always this form.
#[test]
fn a_name_may_be_instantiated_before_an_associated_call() {
    assert_shape(
        "Map[String, Int].new()",
        "
        Method `new`
          receiver: Path `Map`
            generics of `Map`
              Path `String`
              Path `Int`
        ",
    );
    assert_shape(
        "Array[Box[dyn Summarize]].new()",
        "
        Method `new`
          receiver: Path `Array`
            generics of `Array`
              Path `Box`
                generics of `Box`
                  Dyn
                    Bound `Summarize`
        ",
    );
}

/// The index row of §4.4's table is still reachable, on a receiver that is not
/// a name and so could never be a generic instantiation.
#[test]
fn indexing_applies_to_a_receiver_that_is_not_a_name() {
    assert_shape(
        "f()[0]",
        "
        Index
          base: Call
            callee: Path `f`
          index: Int 0
        ",
    );
}

// --- control flow as an expression ---------------------------------------

/// §4.2's inline form, as the spec writes it. The `then` body ends at `else`,
/// mid-line, because `else` cannot continue an expression.
#[test]
fn inline_if_else() {
    insta::assert_snapshot!(parse_source(
        "fn longest(a: &String, b: &String) -> &String:\n    if a.len() > b.len(): a else: b\n"
    ));
}

/// The same construct in block form produces the same shape.
#[test]
fn block_if_else_matches_the_inline_form() {
    let inline = parse_body("fn f() -> Int:\n    if c: 1 else: 2\n");
    let block = parse_body("fn f() -> Int:\n    if c:\n        1\n    else:\n        2\n");
    assert_eq!(
        common::strip_spans(&inline),
        common::strip_spans(&block),
        "the inline and block forms of the same `if` must produce the same tree"
    );
}

/// §4.2: "dangling `else` binds to the innermost `if`". In
/// `if a: if b: x else: y` the `else` belongs to `if b`.
#[test]
fn dangling_else_binds_to_the_innermost_if() {
    insta::assert_snapshot!(parse_body("fn f():\n    if a: if b: x else: y\n"));
}

/// And the block form is how you say the other thing.
#[test]
fn the_block_form_binds_else_to_the_outer_if() {
    insta::assert_snapshot!(parse_body(
        "fn f():\n    if a:\n        if b:\n            x\n    else:\n        y\n"
    ));
}

/// An `if` with no `else` has type `()` and simply has no else branch.
#[test]
fn if_without_else() {
    assert_shape(
        "if c: 1",
        "
        If
          cond: Path `c`
          then: Block
            tail: Int 1
        ",
    );
}

/// An inline `if` used where a value is wanted: as a binding, as an argument,
/// and chained through its own `else`.
#[test]
fn if_as_a_value() {
    insta::assert_snapshot!(parse_source(
        r#"fn f(flag: Bool, n: Int) -> String:
    let chosen = if flag: 1 else: 0
    println(if flag: "yes" else: "no")
    if n < 10: "small" else: if n < 100: "medium" else: "large"
"#
    ));
}

/// `match`, with inline arms and with block arms in the same expression.
#[test]
fn match_with_inline_and_block_arms() {
    insta::assert_snapshot!(parse_source(
        r#"fn describe(format: &Format) -> String:
    match format:
        Plain: "plain"
        Markdown:
            let prefix = "marked"
            prefix.append("down")
"#
    ));
}

/// A `match` nested inside a `match` arm, which is where the arm terminator
/// and the block terminator have to agree.
#[test]
fn nested_match() {
    insta::assert_snapshot!(parse_source(
        r#"fn render(token: &Token, format: &Format) -> String:
    match token:
        Number(value):
            match format:
                Json: value.json()
                Plain: value.text()
        Eof: ""
"#
    ));
}

/// `match` as the value of a binding: the arms dedent and the statement ends
/// with them, without a newline of its own.
#[test]
fn match_as_the_value_of_a_binding() {
    insta::assert_snapshot!(parse_source(
        r#"fn f() -> String:
    let label = match x:
        A: "a"
        B: "b"
    println(label)
"#
    ));
}

/// `while`, `loop` and `for`, in both block and inline form.
#[test]
fn loops_in_both_forms() {
    insta::assert_snapshot!(parse_source(
        r#"fn f(stack: &mut Array[Int], lines: &Array[String]):
    while not stack.is_empty(): stack.pop()
    for line in lines: println(line)
    loop: break
    while a < b:
        a = a + 1
    for x in xs:
        println(x)
    loop:
        break
"#
    ));
}

// --- errors --------------------------------------------------------------

/// §4.2: a statement in an inline body is an error, and `fn f(): let x = 1` is
/// the spec's own example of one.
#[test]
fn a_let_in_an_inline_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("fn f():\n    if c: let x = 1\n"));
}

/// The same, on a function body, which is where §4.2 writes it down.
#[test]
fn a_let_as_a_whole_inline_function_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("fn f(): let x = 1\n"));
}

/// Named arguments are how a struct is built, so they are meaningless on
/// anything that is not a name. A call's result is the clearest case: there is
/// nothing there for the field names to belong to.
#[test]
fn named_arguments_on_a_non_path_are_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("fn f():\n    f()(title: 1)\n"));
}

/// Parentheses group and nothing more, so a parenthesised name is still a name
/// and still constructs.
#[test]
fn a_parenthesised_name_still_constructs() {
    assert_shape(
        "(Doc)(title: 1)",
        "
        StructLit `Doc`
          fields
            FieldInit `title`
              value: Int 1
        ",
    );
}

/// A token that can neither start nor continue an expression is reported once,
/// and the next statement still parses.
#[test]
fn a_broken_expression_does_not_eat_the_next_statement() {
    insta::assert_snapshot!(parse_source_allowing_errors("fn f():\n    let a = *\n    let b = 1\n"));
}

/// Nothing after the `:` of an inline block.
#[test]
fn an_empty_inline_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "fn f(flag: Bool) -> Int:\n    if flag:\n"
    ));
}
