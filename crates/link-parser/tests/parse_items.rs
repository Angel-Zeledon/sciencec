//! Snapshot tests for the first batch of the parser: top-level `fn`, `struct`
//! and `enum` declarations, `use`, type expressions, block structure, and
//! error recovery.
//!
//! Statement and expression parsing lands in the next batch, so every block
//! below comes out empty. What these tests pin down is that the block's
//! *structure* is consumed correctly and that its span covers the right
//! source.

use link_lexer::TokenKind::*;

mod common;
use common::{id, int, parse_report};

// --- functions -----------------------------------------------------------

#[test]
fn empty_module() {
    insta::assert_snapshot!(parse_report(vec![]));
}

#[test]
fn fn_with_inline_body() {
    // fn main(): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("main"),
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn fn_with_indented_body() {
    // fn main():
    //     a
    //     b
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("main"),
        LParen,
        RParen,
        Colon,
        Newline,
        Indent,
        id("a"),
        Newline,
        id("b"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn fn_body_with_nested_block() {
    // A nested block must not close the outer one early.
    // fn main():
    //     while a:
    //         b
    //     c
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("main"),
        LParen,
        RParen,
        Colon,
        Newline,
        Indent,
        While,
        id("a"),
        Colon,
        Newline,
        Indent,
        id("b"),
        Newline,
        Dedent,
        id("c"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn fn_with_params_and_return_type() {
    // fn longest(a: &String, b: &String) -> &String: a
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("longest"),
        LParen,
        id("a"),
        Colon,
        Amp,
        id("String"),
        Comma,
        id("b"),
        Colon,
        Amp,
        id("String"),
        RParen,
        Arrow,
        Amp,
        id("String"),
        Colon,
        id("a"),
        Newline,
    ]));
}

#[test]
fn fn_with_generic_bound_inline() {
    // fn largest[T: Ord](items: &Array[T]) -> &T: items
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("largest"),
        LBracket,
        id("T"),
        Colon,
        id("Ord"),
        RBracket,
        LParen,
        id("items"),
        Colon,
        Amp,
        id("Array"),
        LBracket,
        id("T"),
        RBracket,
        RParen,
        Arrow,
        Amp,
        id("T"),
        Colon,
        id("items"),
        Newline,
    ]));
}

#[test]
fn fn_with_where_clause() {
    // fn describe[T](x: &T) -> String where T: Summarize + Clone, U: Eq: x
    //
    // The interesting part is the last `:`: a bound list ends at `,` (another
    // predicate) or at `:` (the block), and nothing else.
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("describe"),
        LBracket,
        id("T"),
        RBracket,
        LParen,
        id("x"),
        Colon,
        Amp,
        id("T"),
        RParen,
        Arrow,
        id("String"),
        Where,
        id("T"),
        Colon,
        id("Summarize"),
        Plus,
        id("Clone"),
        Comma,
        id("U"),
        Colon,
        id("Eq"),
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn fn_self_receivers() {
    // fn a(self): x
    // fn b(&self): x
    // fn c(&mut self, n: I32): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("a"),
        LParen,
        SelfValue,
        RParen,
        Colon,
        id("x"),
        Newline,
        Fn,
        id("b"),
        LParen,
        Amp,
        SelfValue,
        RParen,
        Colon,
        id("x"),
        Newline,
        Fn,
        id("c"),
        LParen,
        Amp,
        Mut,
        SelfValue,
        Comma,
        id("n"),
        Colon,
        id("I32"),
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn fn_signature_without_body() {
    // fn summarize(&self) -> String
    //
    // A bodiless `fn` is what a trait's required method looks like; the same
    // node covers it.
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("summarize"),
        LParen,
        Amp,
        SelfValue,
        RParen,
        Arrow,
        id("String"),
        Newline,
    ]));
}

#[test]
fn pub_fn() {
    // pub fn main(): x
    insta::assert_snapshot!(parse_report(vec![
        Pub,
        Fn,
        id("main"),
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

// --- types ---------------------------------------------------------------

#[test]
fn type_expressions() {
    // fn f(a: &dyn Summarize, b: (), c: (I32, Bool), d: &mut Array[I32],
    //      e: Box[dyn Summarize], g: &Self, h: text.parser.Token): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("f"),
        LParen,
        id("a"),
        Colon,
        Amp,
        Dyn,
        id("Summarize"),
        Comma,
        id("b"),
        Colon,
        LParen,
        RParen,
        Comma,
        id("c"),
        Colon,
        LParen,
        id("I32"),
        Comma,
        id("Bool"),
        RParen,
        Comma,
        id("d"),
        Colon,
        Amp,
        Mut,
        id("Array"),
        LBracket,
        id("I32"),
        RBracket,
        Comma,
        id("e"),
        Colon,
        id("Box"),
        LBracket,
        Dyn,
        id("Summarize"),
        RBracket,
        Comma,
        id("g"),
        Colon,
        Amp,
        SelfType,
        Comma,
        id("h"),
        Colon,
        id("text"),
        Dot,
        id("parser"),
        Dot,
        id("Token"),
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn parenthesised_type_is_not_a_tuple() {
    // fn f(a: (I32)): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("f"),
        LParen,
        id("a"),
        Colon,
        LParen,
        id("I32"),
        RParen,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

// --- structs -------------------------------------------------------------

#[test]
fn struct_with_fields() {
    // struct Doc:
    //     title: String
    //     body: String
    insta::assert_snapshot!(parse_report(vec![
        Struct,
        id("Doc"),
        Colon,
        Newline,
        Indent,
        id("title"),
        Colon,
        id("String"),
        Newline,
        id("body"),
        Colon,
        id("String"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn struct_generic_with_borrowed_field() {
    // pub struct Pair[A, B]:
    //     pub first: A
    //     source: &Doc
    insta::assert_snapshot!(parse_report(vec![
        Pub,
        Struct,
        id("Pair"),
        LBracket,
        id("A"),
        Comma,
        id("B"),
        RBracket,
        Colon,
        Newline,
        Indent,
        Pub,
        id("first"),
        Colon,
        id("A"),
        Newline,
        id("source"),
        Colon,
        Amp,
        id("Doc"),
        Newline,
        Dedent,
    ]));
}

// --- enums ---------------------------------------------------------------

#[test]
fn enum_with_positional_payloads() {
    // enum Result[T, E]:
    //     Ok(T)
    //     Err(E)
    insta::assert_snapshot!(parse_report(vec![
        Enum,
        id("Result"),
        LBracket,
        id("T"),
        Comma,
        id("E"),
        RBracket,
        Colon,
        Newline,
        Indent,
        id("Ok"),
        LParen,
        id("T"),
        RParen,
        Newline,
        id("Err"),
        LParen,
        id("E"),
        RParen,
        Newline,
        Dedent,
    ]));
}

#[test]
fn enum_with_unit_variant() {
    // enum Option[T]:
    //     Some(T)
    //     None
    insta::assert_snapshot!(parse_report(vec![
        Enum,
        id("Option"),
        LBracket,
        id("T"),
        RBracket,
        Colon,
        Newline,
        Indent,
        id("Some"),
        LParen,
        id("T"),
        RParen,
        Newline,
        id("None"),
        Newline,
        Dedent,
    ]));
}

// --- use -----------------------------------------------------------------

#[test]
fn use_declarations() {
    // use text.parser
    // use text.parser (Token, lex)
    insta::assert_snapshot!(parse_report(vec![
        Use,
        id("text"),
        Dot,
        id("parser"),
        Newline,
        Use,
        id("text"),
        Dot,
        id("parser"),
        LParen,
        id("Token"),
        Comma,
        id("lex"),
        RParen,
        Newline,
    ]));
}

// --- errors and recovery -------------------------------------------------

#[test]
fn error_missing_function_name() {
    // fn (): x
    // struct Doc:
    //     title: String
    //
    // The bad `fn` is dropped; the struct after it still parses.
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
        Struct,
        id("Doc"),
        Colon,
        Newline,
        Indent,
        id("title"),
        Colon,
        id("String"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn error_statement_at_top_level() {
    // let x = 1
    // struct Doc:
    //     title: String
    insta::assert_snapshot!(parse_report(vec![
        Let,
        id("x"),
        Eq,
        int(1),
        Newline,
        Struct,
        id("Doc"),
        Colon,
        Newline,
        Indent,
        id("title"),
        Colon,
        id("String"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn error_missing_colon_before_body() {
    // fn f()
    //     x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("f"),
        LParen,
        RParen,
        Newline,
        Indent,
        id("x"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn error_no_indent_after_colon() {
    // fn f():
    // fn g(): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("f"),
        LParen,
        RParen,
        Colon,
        Newline,
        Fn,
        id("g"),
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn error_field_without_type() {
    // struct S:
    //     title
    //     body: String
    //
    // The broken field is dropped and the next one still parses: one pass has
    // to be able to report more than one problem.
    insta::assert_snapshot!(parse_report(vec![
        Struct,
        id("S"),
        Colon,
        Newline,
        Indent,
        id("title"),
        Newline,
        id("body"),
        Colon,
        id("String"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn error_two_broken_items_then_a_good_one() {
    // A pass reports every item it stumbles on, not just the first.
    insta::assert_snapshot!(parse_report(vec![
        Struct,
        Colon,
        Newline,
        Indent,
        id("a"),
        Colon,
        id("I32"),
        Newline,
        Dedent,
        Enum,
        Colon,
        Newline,
        Indent,
        id("A"),
        Newline,
        Dedent,
        Use,
        id("text"),
        Newline,
    ]));
}

#[test]
fn error_unknown_token_in_type() {
    // fn f(a: %): x
    insta::assert_snapshot!(parse_report(vec![
        Fn,
        id("f"),
        LParen,
        id("a"),
        Colon,
        Percent,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}
