//! The multi-parameter closure `(acc, x) giving acc + x` --
//! `def-and-lambda.md` section 4.5 and `collections-and-chains.md` AMENDMENT 3,
//! which `reduce` and `accumulate` need for their second argument.
//!
//! The form is `giving` unconditionally: `each` names one subject and has no
//! spelling for two. The parser tells it from a parenthesised tuple by
//! scanning `( name , name ... )` and then `giving`, with no backtracking.

mod common;
use common::shape_of_expr;

/// Two names: `param` is the first and a second `param` child follows it, in
/// source order, before the body.
#[test]
fn two_names_before_giving_are_one_closure() {
    insta::assert_snapshot!(shape_of_expr("(acc, x) giving acc + x"));
}

/// Three, to show the list is not special-cased to two.
#[test]
fn three_names_are_three_parameters() {
    insta::assert_snapshot!(shape_of_expr("(a, b, c) giving a + b + c"));
}

/// As a call argument, where `reduce` takes it, beside an ordinary argument.
#[test]
fn a_multi_parameter_closure_as_an_argument() {
    insta::assert_snapshot!(shape_of_expr("xs.reduce(0, (acc, x) giving acc + x)"));
}

/// A tuple of names with no `giving` after it is still a tuple.
#[test]
fn a_tuple_of_names_is_still_a_tuple() {
    insta::assert_snapshot!(shape_of_expr("(a, b)"));
}

/// The body gives its whole right-hand side to the closure, as a one-name
/// `giving` does.
#[test]
fn the_body_extends_to_the_end_of_the_expression() {
    insta::assert_snapshot!(shape_of_expr("(a, b) giving if a > b: a else: b"));
}
