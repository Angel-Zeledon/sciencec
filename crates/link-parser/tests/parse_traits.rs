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
        "trait Summarize:\n    fn summarize(&self) -> String\n\n \
         fn preview(&self) -> String:\n        self.summarize().truncate(80)\n"
    ));
}

/// §4.3: "a trait may take type parameters. With no associated types in F0,
/// this is the only way to write `Iterate` or `From`."
#[test]
fn generic_trait() {
    insta::assert_snapshot!(parse_source(
        "trait From[T]:\n    fn from(value: T) -> Self\n"
    ));
}

/// Supertraits, written `trait A: B + C`, and a `where` clause on a trait.
#[test]
fn trait_with_supertraits() {
    insta::assert_snapshot!(parse_source(
        "trait Pretty[T]: Summarize + Clone:\n    fn pretty(&self) -> T\n"
    ));
}

/// A trait whose methods take each receiver form.
#[test]
fn trait_receivers() {
    insta::assert_snapshot!(parse_source(
        "trait Shape:\n    fn area(self) -> Int\n    fn scale(&self) -> Int\n    fn reset(&mut self)\n"
    ));
}

/// `impl Trait for Type`, the form §4.3 writes down first.
#[test]
fn trait_impl() {
    insta::assert_snapshot!(parse_source(
        "impl Summarize for Doc:\n    fn summarize(&self) -> String:\n        self.body.truncate(200)\n"
    ));
}

/// A generic trait implemented for a generic type, both sides carrying
/// arguments.
#[test]
fn generic_trait_impl() {
    insta::assert_snapshot!(parse_source(
        "impl Swap[Pair[B, A]] for Pair[A, B]:\n    fn swapped(&self) -> Pair[B, A]:\n \
         Pair(first: self.second, second: self.first)\n"
    ));
}

/// §4.3's inherent impl: "a type may have methods that come from no trait. A
/// function in an `impl` with no `self` receiver is an associated function."
#[test]
fn inherent_impl_with_an_associated_function() {
    insta::assert_snapshot!(parse_source(
        "impl Doc:\n    fn new(title: String) -> Doc:\n        Doc(title: title, body: \"\")\n\n \
         fn is_empty(&self) -> Bool:\n        self.body.len() == 0\n"
    ));
}

/// The call side of an associated function: `Doc.new(\"a\")`. The parser cannot
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
        "struct Point:\n    x: Int\n\nimpl Copy for Point\n\nimpl Clone for Point:\n \
         fn clone(&self) -> Point:\n        Point(x: self.x)\n\nfn main():\n    println(1)\n"
    ));
}

/// A `where` clause on an `impl`.
#[test]
fn impl_with_a_where_clause() {
    insta::assert_snapshot!(parse_source(
        "impl Summarize for Pair[A, B] where A: Clone, B: Clone:\n \
         fn summarize(&self) -> String:\n        \"pair\"\n"
    ));
}

/// A trait body that contains something other than a method is reported, and
/// the methods around it still parse.
#[test]
fn a_non_method_in_a_trait_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "trait T:\n    let x = 1\n    fn f(&self) -> Int\n"
    ));
}

/// `impl` of something that is not a path as a trait.
#[test]
fn a_non_path_trait_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "impl &Doc for Point:\n    fn f(&self) -> Int:\n        1\n"
    ));
}

/// An `impl` body that is indented without a `:` gets the same targeted
/// message a function does, rather than a complaint about stray indentation.
#[test]
fn an_impl_body_without_a_colon_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "impl Summarize for Doc\n    fn summarize(&self) -> String:\n        \"a\"\n"
    ));
}
