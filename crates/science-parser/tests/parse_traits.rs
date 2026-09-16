//! `trait` and `impl`, which §4.3 gives five shapes between them: a trait with
//! required and default methods, a generic trait, an `impl Trait for Type`, an
//! inherent `impl Type` with associated functions, and the block-less marker
//! form.

mod common;
use common::{parse_source, parse_source_allowing_errors};

/// A trait with one required method and one with a default body. The two are
/// the same node; only `body` differs.
#[test]
fn trait_with_a_default_method() {
    insta::assert_snapshot!(parse_source(
        r#"trait Summarize:
    fn summarize(&self) -> String

    fn preview(&self) -> String:
        self.summarize().truncate(80)
"#
    ));
}

/// §4.3: "a trait may take type parameters. With no associated types in F0,
/// this is the only way to write `Iterate` or `From`."
#[test]
fn generic_trait() {
    insta::assert_snapshot!(parse_source(
        r#"trait From[T]:
    fn from(value: T) -> Self
"#
    ));
}

/// Supertraits, written `trait A: B + C`. The `:` that introduces them and the
/// `:` that opens the body are told apart by what follows: a name against the
/// end of the line.
#[test]
fn trait_with_supertraits() {
    insta::assert_snapshot!(parse_source(
        r#"trait Pretty[T]: Summarize + Clone:
    fn pretty(&self) -> T
"#
    ));
}

/// A trait whose methods take each receiver form.
#[test]
fn trait_receivers() {
    insta::assert_snapshot!(parse_source(
        r#"trait Shape:
    fn area(self) -> Int
    fn scale(&self) -> Int
    fn reset(&mut self)
"#
    ));
}

/// `impl Trait for Type`, the form §4.3 writes down first.
#[test]
fn trait_impl() {
    insta::assert_snapshot!(parse_source(
        r#"impl Summarize for Doc:
    fn summarize(&self) -> String:
        self.body.truncate(200)
"#
    ));
}

/// A generic trait implemented for a generic type, both sides carrying
/// arguments.
#[test]
fn generic_trait_impl() {
    insta::assert_snapshot!(parse_source(
        r#"impl Swap[Pair[B, A]] for Pair[A, B]:
    fn swapped(&self) -> Pair[B, A]:
        Pair(first: self.second, second: self.first)
"#
    ));
}

/// §4.3's inherent impl: "a type may have methods that come from no trait. A
/// function in an `impl` with no `self` receiver is an associated function."
#[test]
fn inherent_impl_with_an_associated_function() {
    insta::assert_snapshot!(parse_source(
        r#"impl Doc:
    fn new(title: String) -> Doc:
        Doc(title: title, body: "")

    fn is_empty(&self) -> Bool:
        self.body.len() == 0
"#
    ));
}

/// An inherent impl on a generic type, which is how `Array[T].new()` gets its
/// associated function.
#[test]
fn inherent_impl_on_a_generic_type() {
    insta::assert_snapshot!(parse_source(
        r#"impl Array[T]:
    fn new() -> Array[T]:
        empty()
"#
    ));
}

/// The call side of an associated function: `Doc.new("a")`. The parser cannot
/// tell it from a method call, and §4.4's rule is that resolution decides.
#[test]
fn an_associated_function_call_is_a_method_call_until_resolution() {
    insta::assert_snapshot!(parse_source("fn main():\n    let d = Doc.new(\"a\")\n"));
}

/// §4.3's marker trait: "a trait with no methods is implemented by a single
/// line with no block, since there is nothing to indent." Without this rule
/// `Copy` would be unimplementable.
#[test]
fn marker_trait_impl() {
    insta::assert_snapshot!(parse_source("impl Copy for Point\n"));
}

/// The marker form sits among ordinary items without disturbing them.
#[test]
fn marker_trait_impl_between_other_items() {
    insta::assert_snapshot!(parse_source(
        r#"struct Point:
    x: Int

impl Copy for Point

impl Clone for Point:
    fn clone(&self) -> Point:
        Point(x: self.x)

fn main():
    println(1)
"#
    ));
}

/// A `where` clause on an `impl`.
#[test]
fn impl_with_a_where_clause() {
    insta::assert_snapshot!(parse_source(
        r#"impl Summarize for Pair[A, B] where A: Clone, B: Clone:
    fn summarize(&self) -> String:
        "pair"
"#
    ));
}

/// §4.3: "where a `where` clause ends." A bound list ends only at `+` or `,`,
/// so the first `:` that no bound consumes is the one that opens the block.
#[test]
fn a_where_clause_ends_at_the_colon_that_opens_the_block() {
    insta::assert_snapshot!(parse_source(
        r#"fn f[T]() -> T where T: A + B:
    body()
"#
    ));
}

/// A trait body that contains something other than a method is reported, and
/// the methods around it still parse.
#[test]
fn a_non_method_in_a_trait_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"trait T:
    let x = 1
    fn f(&self) -> Int
"#
    ));
}

/// `impl` of something that is not a path as a trait.
#[test]
fn a_non_path_trait_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"impl &Doc for Point:
    fn f(&self) -> Int:
        1
"#
    ));
}

/// An `impl` body that is indented without a `:` gets the same targeted
/// message a function does, rather than a complaint about stray indentation.
#[test]
fn an_impl_body_without_a_colon_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"impl Summarize for Doc
    fn summarize(&self) -> String:
        "a"
"#
    ));
}
