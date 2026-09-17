//! Expressions: §4.6's precedence table, the postfix chain, the closures of
//! §4.6, and every expression form the grammar has.
//!
//! The precedence tests are plain assertions over a span-stripped dump rather
//! than snapshots. A precedence rule is a claim about *shape* — that
//! `1 + 2 * 3` groups as `1 + (2 * 3)` — and a claim is clearer written down
//! than diffed.

mod common;
use common::{
    parse_body, parse_source, parse_source_allowing_errors, shape_of_expr,
    shape_of_expr_despite_lexical_errors,
};

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

/// The top row: call, index, field access and `try` bind tighter than anything.
///
/// `borrowed` and `mutable borrowed` are the unary row now that the sigils are
/// gone (§4.3), so `borrowed point.x` borrows the *field*.
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
        "borrowed point.x",
        "
        Borrowed
          Field `x`
            base: Path `point`
        ",
    );
    assert_shape(
        "mutable borrowed owner.y",
        "
        Borrowed mutable
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

/// `as` binds tighter than `**` and than `*`, and chains left to right.
#[test]
fn as_binds_tighter_than_power_and_multiplication() {
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
    // `as` is the row directly above `**`, so the cast is the power's base.
    assert_shape(
        "a as F64 ** 2",
        "
        Binary `**`
          lhs: Cast
            expr: Path `a`
            type: Path `F64`
          rhs: Int 2
        ",
    );
}

/// `**` is the one right-associative row of §4.6, and it sits above `* / % @`.
///
/// `examples/12_operators.science` spells both halves out: `2 ** 3 ** 2` is
/// `2 ** (3 ** 2)`, and `2 * 3 ** 2` is `2 * (3 ** 2)`.
#[test]
fn power_is_right_associative_and_binds_tighter_than_multiplication() {
    assert_shape(
        "2 ** 3 ** 2",
        "
        Binary `**`
          lhs: Int 2
          rhs: Binary `**`
            lhs: Int 3
            rhs: Int 2
        ",
    );
    assert_shape(
        "2 * 3 ** 2",
        "
        Binary `*`
          lhs: Int 2
          rhs: Binary `**`
            lhs: Int 3
            rhs: Int 2
        ",
    );
    // The same row on the left: `**` still wins, and the `*` takes the result.
    assert_shape(
        "2 ** 3 * 2",
        "
        Binary `*`
          lhs: Binary `**`
            lhs: Int 2
            rhs: Int 3
          rhs: Int 2
        ",
    );
    // The unary row is *above* `**` in §4.6's table, so the minus is applied
    // first: `-a ** 2` is `(-a) ** 2`, not `-(a ** 2)`.
    assert_shape(
        "-a ** 2",
        "
        Binary `**`
          lhs: Unary `-`
            Path `a`
          rhs: Int 2
        ",
    );
}

/// `* / % @` share a row, above `+ -`. `@` is matrix multiply (§4.6).
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
    // `@` shares the row, so it binds tighter than `+`: (left @ right) + left.
    assert_shape(
        "left @ right + left",
        "
        Binary `+`
          lhs: Binary `@`
            lhs: Path `left`
            rhs: Path `right`
          rhs: Path `left`
        ",
    );
    // And it is level with `*`, so the two nest left to right.
    assert_shape(
        "a @ b * c",
        "
        Binary `*`
          lhs: Binary `@`
            lhs: Path `a`
            rhs: Path `b`
          rhs: Path `c`
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
    // `is` sits on the same row: (a + b) is (a * b).
    assert_shape(
        "a + b is a * b",
        "
        Binary `==`
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
    // `not` is unary and binds tightest of the three: `(not flag) and other`.
    assert_shape(
        "not flag and other",
        "
        Binary `and`
          lhs: Unary `not`
            Path `flag`
          rhs: Path `other`
        ",
    );
}

/// Every binary row is left-associative but `**`, so a repeated operator nests
/// left. `**` is excluded here and pinned right-associative above.
#[test]
fn binary_operators_are_left_associative() {
    for op in ["-", "/", "%", "@", "<<", "&", "^", "|", "and", "or"] {
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

    // `is` is on the same row and associates the same way. It cannot join the
    // loop above because it is spelled as a word and dumps under the
    // operator's own name.
    assert_shape(
        "a is b is c",
        "
        Binary `==`
          lhs: Binary `==`
            lhs: Path `a`
            rhs: Path `b`
          rhs: Path `c`
        ",
    );
}

/// Revision 2 §1: `is` is equality, `is not` is inequality, and ordering is
/// written with the symbols. There is one spelling for each.
#[test]
fn every_comparison_parses() {
    for (phrase, symbol) in [
        ("is", "=="),
        ("is not", "!="),
        (">=", ">="),
        ("<=", "<="),
        (">", ">"),
        ("<", "<"),
    ] {
        assert_shape(
            &format!("a + 1 {phrase} b"),
            &format!(
                "
        Binary `{symbol}`
          lhs: Binary `+`
            lhs: Path `a`
            rhs: Int 1
          rhs: Path `b`
        "
            ),
        );
    }
}

/// The removed symbols still parse, and they parse as the words they were
/// replaced by.
///
/// This test used to assert that the word and the symbol were the same
/// operator, because §4.6 carried both spellings. Revision 2 §1 removed the
/// symbols from the language, so the claim is now a different one: the lexer
/// reports `==` and `!=` but emits `EqEq` and `NotEq` anyway, which means the
/// parser builds *exactly* the tree the corrected source would build. That is
/// what stops one stale symbol from cascading, and it is only observable by
/// comparing the two trees.
#[test]
fn the_removed_symbols_still_parse_as_the_words_that_replaced_them() {
    for (phrase, symbol) in [("is", "=="), ("is not", "!=")] {
        let words = shape_of_expr(&format!("a + 1 {phrase} b * 2"));
        let symbols = shape_of_expr_despite_lexical_errors(&format!("a + 1 {symbol} b * 2"));
        assert_eq!(
            words, symbols,
            "`{symbol}` must recover to `{phrase}`, not to something the parser invents"
        );
    }
}


/// `&` used to be both the reference operator and bitwise and; the reference
/// is the word `borrowed` now (§4.3), so `&` has exactly one reading left.
#[test]
fn ampersand_is_only_bitwise_and_now_that_borrows_are_words() {
    assert_shape(
        "a & b",
        "
        Binary `&`
          lhs: Path `a`
          rhs: Path `b`
        ",
    );
    // A borrow on the right of the operator: `borrowed` is unary, so it takes
    // only the operand after it.
    assert_shape(
        "a & borrowed b",
        "
        Binary `&`
          lhs: Path `a`
          rhs: Borrowed
            Path `b`
        ",
    );
    assert_shape(
        "borrowed a",
        "
        Borrowed
          Path `a`
        ",
    );
    assert_shape(
        "mutable borrowed a",
        "
        Borrowed mutable
          Path `a`
        ",
    );
}

// --- the postfix chain ---------------------------------------------------

/// `?` is postfix on the tightest row (revision 2 §3.1), so it applies to the
/// whole postfix chain to its *left* — the opposite direction from the `try`
/// it replaced, which covered the chain to its right.
///
/// The note says only "it is a postfix operator" and every example in it
/// applies `?` to a bare name, so the rung is decided here and not there.
#[test]
fn the_presence_test_applies_to_the_whole_chain_to_its_left() {
    assert_shape(
        "a.b().c?",
        "
        Present
          Field `c`
            base: Method `b`
              receiver: Path `a`
        ",
    );
    // On the tightest row, so a binary operator does not end up inside it:
    // this is `(a.b()?) + 1`.
    assert_shape(
        "a.b()? + 1",
        "
        Binary `+`
          lhs: Present
            Method `b`
              receiver: Path `a`
          rhs: Int 1
        ",
    );
    // Covering *less* than the chain takes parentheses, exactly as it did
    // before, and for the same reason.
    assert_shape(
        "(read_config(path)?).port",
        "
        Field `port`
          base: Present
            Call
              callee: Path `read_config`
              args
                Path `path`
        ",
    );
}

/// The test the error model is actually written with: a name, a `?`, and an
/// `if`. Every example in §3.1 is this shape and nothing more.
#[test]
fn the_presence_test_on_a_bare_binding_is_the_shape_that_matters() {
    assert_shape(
        "err?",
        "
        Present
          Path `err`
        ",
    );
    // `not err?` can only be `not (err?)`: `not` is a prefix on a row above
    // the postfix chain, so the chain binds first.
    assert_shape(
        "not err?",
        "
        Unary `not`
          Present
            Path `err`
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

/// §4.6: "a line whose continuation begins with `.` continues too". The chain
/// broken across lines is the same tree as the chain written on one.
#[test]
fn a_chain_may_be_broken_by_a_leading_dot() {
    insta::assert_snapshot!(parse_body(
        r#"def headlines(docs: borrowed Array of Doc) -> Array of String:
    docs
        .iterate()
        .discard(each.is_empty())
        .map(each.title)
        .take(5)
        .collect()
"#
    ));
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

// --- calls, record literals and the ambiguity between them ---------------

/// §4.4: "named arguments mean a record, positional arguments mean a call or
/// variant." That is the whole rule, and it is decided here and nowhere else.
#[test]
fn named_arguments_make_a_record_literal_and_positional_ones_a_call() {
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

/// The record may be named through a path, in which case the named arguments
/// still make it a construction rather than a method call.
#[test]
fn a_qualified_name_with_named_arguments_is_still_a_record_literal() {
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

/// §4.7: `Some(x)` and `Option.Some(x)` are the same thing. The qualified form
/// is written exactly like a method call, so the parser produces one and leaves
/// resolution to reclassify it — the same answer `Doc.new("a")` gets.
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

/// A record literal nested in a call argument, and a call nested in a field's
/// value: neither form leaks into the other.
#[test]
fn record_literals_and_calls_nest_in_each_other() {
    assert_shape(
        "print(Doc(title: f(1), body: g()))",
        "
        Call
          callee: Path `print`
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

/// Trailing commas are allowed in every bracketed list (§4.7).
#[test]
fn trailing_commas_are_allowed() {
    insta::assert_snapshot!(parse_source(
        r#"def main():
    let a be f(
        1,
        2,
    )
    let b be Doc(
        title: "a",
    )
    let c be (1, 2,)
"#
    ));
}

// --- closures ------------------------------------------------------------

/// §4.6's implicit form: `each` names the subject of the enclosing call, and
/// the argument that mentions one *is* the closure.
#[test]
fn the_implicit_closure_form_wraps_the_argument() {
    assert_shape(
        "docs.map(each.title)",
        "
        Method `map`
          receiver: Path `docs`
          args
            Closure
              body: Field `title`
                base: Each
        ",
    );
}

/// §4.6's named form: `name giving expression` declares the parameter, and
/// gives the closure its whole right-hand side.
#[test]
fn the_named_closure_form_declares_its_parameter() {
    assert_shape(
        "docs.map(doc giving doc.title)",
        "
        Method `map`
          receiver: Path `docs`
          args
            Closure
              param: Ident `doc`
              body: Field `title`
                base: Path `doc`
        ",
    );
}

/// Two `each` in the *same* argument are the same subject, so they are fine
/// and they make one closure, not two.
#[test]
fn two_each_in_one_argument_make_one_closure() {
    assert_shape(
        "docs.discard(each.title.is_empty() or each.body.is_empty())",
        "
        Method `discard`
          receiver: Path `docs`
          args
            Closure
              body: Binary `or`
                lhs: Method `is_empty`
                  receiver: Field `title`
                    base: Each
                rhs: Method `is_empty`
                  receiver: Field `body`
                    base: Each
        ",
    );
}

/// `outer.map(inner.map(each.x))` is legal: the inner `each` is inside no
/// other `each`, so only one subject is ever named. §4.6 rejects *nesting*,
/// not two calls.
#[test]
fn an_each_inside_an_unclaimed_argument_is_fine() {
    assert_shape(
        "outer.map(inner.map(each.x))",
        "
        Method `map`
          receiver: Path `outer`
          args
            Method `map`
              receiver: Path `inner`
              args
                Closure
                  body: Field `x`
                    base: Each
        ",
    );
}

/// §4.6: in `outer.map(each.inner.map(each.x))` the two `each` refer to
/// different subjects and the inner shadows the outer irrecoverably. Science
/// rejects it (`SC0115`) rather than picking a rule.
#[test]
fn a_nested_each_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f(outer: borrowed Array of Doc):\n    outer.map(each.inner.map(each.x))\n"
    ));
}

/// A named argument on a receiver that cannot name a record: the name stays on
/// the argument, which is the `Arg { name: Some(..) }` of §4.6.
#[test]
fn a_named_call_argument_keeps_its_name() {
    assert_shape(
        "docs.iterate().sort(by: line giving line.length())",
        "
        Method `sort`
          receiver: Method `iterate`
            receiver: Path `docs`
          args
            Arg `by`
              value: Closure
                param: Ident `line`
                body: Method `length`
                  receiver: Path `line`
        ",
    );
}

/// The same call on a *path* receiver is written exactly like `text.Doc(title:
/// "a")`, and the parser cannot tell them apart: only resolution knows whether
/// `docs` is a module. It produces the construction and lets resolution
/// reclassify, which is the same answer §4.4 gives `Doc()`.
#[test]
fn a_named_argument_on_a_path_receiver_reads_as_a_construction() {
    assert_shape(
        "docs.sort(by: line giving line.length())",
        "
        StructLit `docs.sort`
          fields
            FieldInit `by`
              value: Closure
                param: Ident `line`
                body: Method `length`
                  receiver: Path `line`
        ",
    );
}

// --- ranges --------------------------------------------------------------

/// §4.5: `0..n` is half-open, `0..=n` inclusive, and both endpoints are
/// required.
#[test]
fn both_range_forms_parse() {
    assert_shape(
        "0..n",
        "
        Range
          start: Int 0
          end: Path `n`
        ",
    );
    assert_shape(
        "0..=n",
        "
        Range inclusive
          start: Int 0
          end: Path `n`
        ",
    );
}

/// `..` is looser than every operator in §4.6's table, so `0..n - 1` counts to
/// `n - 1` rather than subtracting from a range.
#[test]
fn a_range_endpoint_takes_the_whole_arithmetic_expression() {
    assert_shape(
        "0..n - 1",
        "
        Range
          start: Int 0
          end: Binary `-`
            lhs: Path `n`
            rhs: Int 1
        ",
    );
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
        r#"def main():
    let a be 42
    let b be 0xFF
    let c be 0b1010
    let d be 0o777
    let e be 42i32
    let f be 3.14
    let g be 2.5f32
    let h be "text"
    let i be 'x'
    let j be true
    let k be false
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
/// through it. §4.3 requires the parenthesised form — `(Array of Doc).new()` —
/// so that the `.` cannot attach to the last type argument instead.
#[test]
fn a_name_may_be_instantiated_before_an_associated_call() {
    assert_shape(
        "(Array of Doc).new()",
        "
        Method `new`
          receiver: Path `Array`
            generics of `Array`
              Path `Doc`
        ",
    );
    // Two or more arguments take parentheses of their own (§4.3).
    assert_shape(
        "(Map of (String, Int)).new()",
        "
        Method `new`
          receiver: Path `Map`
            generics of `Map`
              Path `String`
              Path `Int`
        ",
    );
    assert_shape(
        "(Array of Box of any Summarize).new()",
        "
        Method `new`
          receiver: Path `Array`
            generics of `Array`
              Path `Box`
                generics of `Box`
                  Any
                    Bound `Summarize`
        ",
    );
}

/// The bare form is the one §4.3 rules on: `.new()` could belong to `Doc` or
/// to `Array of Doc`, and rather than make a space load-bearing Science
/// reports the ambiguity (`SC0116`), says which reading it took, and offers
/// the parentheses as the fix.
#[test]
fn a_bare_generic_before_an_associated_call_is_ambiguous() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f():\n    let a be Array of Doc.new()\n"
    ));
}

/// The index row of §4.6's table is still reachable, on a receiver that is not
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

/// §4.5's inline form, as the spec writes it. The `then` body ends at `else`,
/// mid-line, because `else` cannot continue an expression.
#[test]
fn inline_if_else() {
    insta::assert_snapshot!(parse_source(
        "def longest(a: borrowed String, b: borrowed String) -> borrowed String:\n    if a.length() > b.length(): a else: b\n"
    ));
}

/// The same construct in block form produces the same shape.
#[test]
fn block_if_else_matches_the_inline_form() {
    let inline = parse_body("def f() -> Int:\n    if c: 1 else: 2\n");
    let block =
        parse_body("def f() -> Int:\n    if c:\n        1\n    else:\n        2\n");
    assert_eq!(
        common::strip_spans(&inline),
        common::strip_spans(&block),
        "the inline and block forms of the same `if` must produce the same tree"
    );
}

/// §4.5: "dangling `else` binds to the innermost `if`". In
/// `if a: if b: x else: y` the `else` belongs to `if b`.
#[test]
fn dangling_else_binds_to_the_innermost_if() {
    insta::assert_snapshot!(parse_body("def f():\n    if a: if b: x else: y\n"));
}

/// And the block form is how you say the other thing.
#[test]
fn the_block_form_binds_else_to_the_outer_if() {
    insta::assert_snapshot!(parse_body(
        "def f():\n    if a:\n        if b:\n            x\n    else:\n        y\n"
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
        r#"def f(flag: Bool, n: Int) -> String:
    let chosen be if flag: 1 else: 0
    print(if flag: "yes" else: "no")
    if n < 10: "small" else: if n < 100: "medium" else: "large"
"#
    ));
}

/// `match`, with inline arms and with block arms in the same expression.
#[test]
fn match_with_inline_and_block_arms() {
    insta::assert_snapshot!(parse_source(
        r#"def describe(format: borrowed Format) -> String:
    match format:
        Plain: "plain"
        Markdown:
            let prefix be "marked"
            prefix.append("down")
"#
    ));
}

/// A `match` nested inside a `match` arm, which is where the arm terminator
/// and the block terminator have to agree.
#[test]
fn nested_match() {
    insta::assert_snapshot!(parse_source(
        r#"def render(token: borrowed Token, format: borrowed Format) -> String:
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
        r#"def f() -> String:
    let label be match x:
        A: "a"
        B: "b"
    print(label)
"#
    ));
}

/// `loop` and `for`, in both block and inline form, and the counting loop
/// §4.5 builds out of a range.
#[test]
fn loops_in_both_forms() {
    insta::assert_snapshot!(parse_source(
        r#"def f(stack: mutable borrowed Array of Int, lines: borrowed Array of String):
    for _ in 0..stack.length(): stack.pop()
    for line in lines: print(line)
    loop: break
    loop:
        if a >= b:
            break
        a be a + 1
    for x in xs:
        print(x)
    for i in 0..n:
        print(i)
    loop:
        break
"#
    ));
}

// --- errors --------------------------------------------------------------

/// §4.5: a statement in an inline body is an error, and a `let` is the spec's
/// own example of one — it binds a name nothing could then use.
#[test]
fn a_let_in_an_inline_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("def f():\n    if c: let x be 1\n"));
}

/// The same, on a function body, which is where §4.5 writes it down.
#[test]
fn a_let_as_a_whole_inline_function_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("def f(): let x be 1\n"));
}

/// Named arguments are how a record is built, so they are meaningless on
/// anything that is not a name. A call's result is the clearest case: there is
/// nothing there for the field names to belong to.
#[test]
fn named_arguments_on_a_non_path_are_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors("def f():\n    f()(title: 1)\n"));
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
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f():\n    let a be *\n    let b be 1\n"
    ));
}

/// Nothing after the `:` of an inline block.
#[test]
fn an_empty_inline_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f(flag: Bool) -> Int:\n    if flag:\n"
    ));
}

// --- negation in expression position, which did not change --------------

/// Unary and binary `-` in expressions, pinned because a *const argument* may
/// now be negated too (`const-expression-arithmetic.md` §2.1) and that change
/// must be invisible from here.
///
/// The const-argument rule lives in `parse_generic_arg`, which only an `of`
/// list ever reaches, so nothing below shares a line of code with it. These
/// assertions exist so that a future edit which tries to unify the two has to
/// break something visible first.
#[test]
fn negation_in_expressions_is_untouched_by_const_arguments() {
    // `let x be -1` is unary negation of a literal, not a negative literal:
    // the AST has no such thing, and it did not grow one.
    assert_shape(
        "-1",
        "
        Unary `-`
          Int 1
        ",
    );
    // A space changes nothing here either, which is the precedent the const
    // argument's rule follows rather than the lexer's `->`.
    assert_shape(
        "- 1",
        "
        Unary `-`
          Int 1
        ",
    );
    // `a - 1` is still subtraction, not `a` applied to a negative literal.
    assert_shape(
        "a - 1",
        "
        Binary `-`
          lhs: Path `a`
          rhs: Int 1
        ",
    );
    // And the two in one expression still associate the way §4.6 says.
    assert_shape(
        "-a - 1",
        "
        Binary `-`
          lhs: Unary `-`
            Path `a`
          rhs: Int 1
        ",
    );
}

// --- where the postfix row has to stop ------------------------------------

/// A parenthesised expression on the line after an indented block is that
/// line's expression, not a call on the block above it.
///
/// This is a regression test for a silent misparse. Every other line ends with
/// a `Newline`, and that token is what stops the postfix row; an indented
/// block ends with a `Dedent` instead, which `parse_indented_block` eats,
/// leaving the cursor on the next line with no boundary in between. So the row
/// carried on and read `(x, y)` as an argument list.
///
/// It reached six functions in `examples/` and the §1.7 `dgemm` binding, whose
/// `unsafe:` block was followed by `((), null)` and came out as a **call on the
/// `unsafe` block** with `()` and `null` for arguments. Nothing caught it,
/// because the only thing watching that file was a snapshot, and a snapshot
/// records whatever it is given. The type checker found it — a call whose
/// callee is an `if` is the first thing that fails to type.
#[test]
fn a_parenthesised_line_after_a_block_is_not_a_call_on_it() {
    insta::assert_snapshot!(parse_body(
        "def split(c: Bool) -> (I64, I64):
    if c:
        return (0, 0)
    (1, 2)
"
    ));
}

/// The same for `unsafe:`, which is how the bug actually shipped.
#[test]
fn a_tuple_after_an_unsafe_block_is_not_a_call_on_it() {
    insta::assert_snapshot!(parse_body(
        "def run() -> ((), Error?):
    unsafe:
        go()
    ((), null)
"
    ));
}

/// The fix keys on the `Dedent`, not on which primary was parsed, so a chain
/// broken over several lines must still be one expression.
///
/// §4.6 makes a leading `.` continue the logical line, and the lexer
/// implements that by emitting no layout token at all — so there is no
/// `Dedent` in front of `.iterate()` for the fix to trip over. That is the
/// reason the two rules do not collide, and it is worth a test because it is a
/// property of the *lexer* that this parser change silently depends on.
#[test]
fn a_chain_broken_over_lines_is_still_one_expression() {
    insta::assert_snapshot!(parse_body(
        "def titles(docs: Array of Doc) -> Array of String:
    docs
        .iterate()
        .map(each.title)
        .collect()
"
    ));
}
