//! Snapshot tests for declarations: top-level `function`, `type` in both of its
//! readings, `choice`, `const`, `use`, type expressions, block structure, and
//! error recovery.
//!
//! These are the tests written against hand-built token streams, so what they
//! pin down is the exact sequence a declaration consumes. `interface` and the
//! two implementation heads live in `parse_traits.rs`, and everything inside a
//! block in `parse_exprs.rs`, `parse_stmts.rs` and `parse_patterns.rs`, all of
//! them written against real source through the lexer.

use science_lexer::TokenKind::*;

mod common;
use common::{id, int, parse_report, parse_source, parse_source_allowing_errors, text};

// --- functions -----------------------------------------------------------

#[test]
fn empty_module() {
    insta::assert_snapshot!(parse_report(vec![]));
}

#[test]
fn function_with_inline_body() {
    // def main(): x
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("main"),
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn function_with_indented_body() {
    // def main():
    //     a
    //     b
    insta::assert_snapshot!(parse_report(vec![
        Function,
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
fn function_body_with_nested_block() {
    // A nested block must not close the outer one early.
    // def main():
    //     loop:
    //         b
    //     c
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("main"),
        LParen,
        RParen,
        Colon,
        Newline,
        Indent,
        Loop,
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
fn function_with_params_and_return_type() {
    // def longest(a: borrowed String, b: borrowed String) -> borrowed String: a
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("longest"),
        LParen,
        id("a"),
        Colon,
        Borrowed,
        id("String"),
        Comma,
        id("b"),
        Colon,
        Borrowed,
        id("String"),
        RParen,
        Arrow,
        Borrowed,
        id("String"),
        Colon,
        id("a"),
        Newline,
    ]));
}

#[test]
fn function_with_generic_bound_inline() {
    // def largest of T: Ord(items: borrowed Array of T) -> borrowed T: items
    //
    // The bound ends at the `(` that opens the parameter list: a single
    // generic parameter needs no parentheses of its own (§4.3).
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("largest"),
        Of,
        id("T"),
        Colon,
        id("Ord"),
        LParen,
        id("items"),
        Colon,
        Borrowed,
        id("Array"),
        Of,
        id("T"),
        RParen,
        Arrow,
        Borrowed,
        id("T"),
        Colon,
        id("items"),
        Newline,
    ]));
}

#[test]
fn function_with_parenthesised_generics() {
    // def paired of (A, B)(first: A, second: B) -> (A, B): x
    //
    // Two or more generic parameters take parentheses, and the parenthesised
    // list must not be mistaken for the parameter list that follows it.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("paired"),
        Of,
        LParen,
        id("A"),
        Comma,
        id("B"),
        RParen,
        LParen,
        id("first"),
        Colon,
        id("A"),
        Comma,
        id("second"),
        Colon,
        id("B"),
        RParen,
        Arrow,
        LParen,
        id("A"),
        Comma,
        id("B"),
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn function_with_where_clause() {
    // def describe of T(x: borrowed T) -> String
    //     where T: Summarize + Clone, U: Eq: x
    //
    // The interesting part is the last `:`: a bound list ends at `,` (another
    // predicate) or at `:` (the block), and nothing else.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("describe"),
        Of,
        id("T"),
        LParen,
        id("x"),
        Colon,
        Borrowed,
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

/// §4.4 writes the return type `-> T`. An earlier draft spelled it `returns T`,
/// and that word is an ordinary identifier now, so without a diagnostic of its
/// own the signature would fail as "expected end of line, found `returns`" —
/// the symptom rather than the mistake. `SC0118` says how the return type is
/// written and offers the one-word fix. The snapshot format does not render
/// suggestions, so the fix is asserted directly.
#[test]
fn the_word_returns_is_reported_where_an_arrow_belongs() {
    let source = r#"def longest(a: borrowed String) returns borrowed String:
    a
"#;
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (tokens, _) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let diagnostic =
        diagnostics.iter().next().expect("`returns` should have produced a diagnostic");
    assert_eq!(diagnostic.code.to_string(), "SC0118");
    assert_eq!(diagnostic.message, "the return type is written `->`");
    let fix = diagnostic.suggestions.first().expect("the diagnostic should offer a fix");
    assert_eq!(fix.replacement, "->");
    assert_eq!(&source[fix.span.start as usize..fix.span.end as usize], "returns");
}

/// Recovering as if `->` had been written keeps the return type in the tree, so
/// the mistake costs one diagnostic rather than a cascade and the rest of the
/// file parses as it was meant to.
#[test]
fn the_word_returns_does_not_derail_the_rest_of_the_file() {
    let source = r#"def f() returns Int:
    1

def g() -> Int:
    2
"#;
    let codes: Vec<String> = {
        let (tokens, _) = science_lexer::lex(common::FILE, source);
        let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
        diagnostics.iter().map(|d| d.code.to_string()).collect()
    };
    assert_eq!(codes, ["SC0118"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

#[test]
fn function_self_receivers() {
    // def a(self: Self): x
    // def b(self): x
    // def c(mutable self, n: I32): x
    //
    // The three receivers of §4.4, in the order value, shared, exclusive.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("a"),
        LParen,
        SelfValue,
        Colon,
        SelfType,
        RParen,
        Colon,
        id("x"),
        Newline,
        Function,
        id("b"),
        LParen,
        SelfValue,
        RParen,
        Colon,
        id("x"),
        Newline,
        Function,
        id("c"),
        LParen,
        Mutable,
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
fn function_signature_without_body() {
    // def summarize(self) -> String
    //
    // A bodiless `function` is what an interface's required method looks like;
    // the same node covers it.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("summarize"),
        LParen,
        SelfValue,
        RParen,
        Arrow,
        id("String"),
        Newline,
    ]));
}

#[test]
fn public_function() {
    // public def main(): x
    insta::assert_snapshot!(parse_report(vec![
        Public,
        Function,
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
    // def f(a: borrowed any Summarize, b: (), c: (I32, Bool),
    //            d: mutable borrowed Array of I32, e: Box of any Summarize,
    //            g: borrowed Self, h: text.parser.Token): x
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("f"),
        LParen,
        id("a"),
        Colon,
        Borrowed,
        Any,
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
        Mutable,
        Borrowed,
        id("Array"),
        Of,
        id("I32"),
        Comma,
        id("e"),
        Colon,
        id("Box"),
        Of,
        Any,
        id("Summarize"),
        Comma,
        id("g"),
        Colon,
        Borrowed,
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
fn generic_argument_forms() {
    // def f(a: Map of (String, Int), b: Array of (Map of (String, Int)),
    //            c: Window of (Int, 4), d: Self.Item): x
    //
    // Two or more arguments take parentheses (§4.3), a nested application
    // takes its own, `4` is a const argument, and `Self.Item` names an
    // associated type rather than a field of anything.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("f"),
        LParen,
        id("a"),
        Colon,
        id("Map"),
        Of,
        LParen,
        id("String"),
        Comma,
        id("Int"),
        RParen,
        Comma,
        id("b"),
        Colon,
        id("Array"),
        Of,
        LParen,
        id("Map"),
        Of,
        LParen,
        id("String"),
        Comma,
        id("Int"),
        RParen,
        RParen,
        Comma,
        id("c"),
        Colon,
        id("Window"),
        Of,
        LParen,
        id("Int"),
        Comma,
        int(4),
        RParen,
        Comma,
        id("d"),
        Colon,
        SelfType,
        Dot,
        id("Item"),
        RParen,
        Colon,
        id("x"),
        Newline,
    ]));
}

#[test]
fn parenthesised_type_is_not_a_tuple() {
    // def f(a: (I32)): x
    insta::assert_snapshot!(parse_report(vec![
        Function,
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

// --- records -------------------------------------------------------------

#[test]
fn record_with_fields() {
    // type Doc:
    //     title: String
    //     body: String
    insta::assert_snapshot!(parse_report(vec![
        Type,
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
fn record_generic_with_borrowed_field() {
    // public type Pair of (A, B):
    //     public first: A
    //     source: borrowed Doc
    insta::assert_snapshot!(parse_report(vec![
        Public,
        Type,
        id("Pair"),
        Of,
        LParen,
        id("A"),
        Comma,
        id("B"),
        RParen,
        Colon,
        Newline,
        Indent,
        Public,
        id("first"),
        Colon,
        id("A"),
        Newline,
        id("source"),
        Colon,
        Borrowed,
        id("Doc"),
        Newline,
        Dedent,
    ]));
}

#[test]
fn record_with_const_generic_params() {
    // type Grid of (T, const ROWS: Int, const COLS: Int):
    //     cells: Array of T
    //
    // §5.3's const parameters: the leading `const` is what tells `ROWS: Int`
    // from a bound on a type parameter.
    insta::assert_snapshot!(parse_report(vec![
        Type,
        id("Grid"),
        Of,
        LParen,
        id("T"),
        Comma,
        Const,
        id("ROWS"),
        Colon,
        id("Int"),
        Comma,
        Const,
        id("COLS"),
        Colon,
        id("Int"),
        RParen,
        Colon,
        Newline,
        Indent,
        id("cells"),
        Colon,
        id("Array"),
        Of,
        id("T"),
        Newline,
        Dedent,
    ]));
}

// --- choices -------------------------------------------------------------

#[test]
fn choice_with_positional_payloads() {
    // choice Result of (T, E):
    //     Ok(T)
    //     Err(E)
    insta::assert_snapshot!(parse_report(vec![
        Choice,
        id("Result"),
        Of,
        LParen,
        id("T"),
        Comma,
        id("E"),
        RParen,
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
fn choice_with_unit_variant() {
    // choice Option of T:
    //     Some(T)
    //     None
    //
    // The `:` after `of T` opens the body; it does not introduce a bound,
    // because the end of the line follows it.
    insta::assert_snapshot!(parse_report(vec![
        Choice,
        id("Option"),
        Of,
        id("T"),
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

// --- aliases and constants -----------------------------------------------

#[test]
fn type_aliases() {
    // type Embedding is Array of F32
    // public type Handle of T is Box of T
    //
    // `type` introduces two declarations, and the word after the name — and
    // after any generic parameters — is the whole of what tells them apart.
    insta::assert_snapshot!(parse_report(vec![
        Type,
        id("Embedding"),
        Is,
        id("Array"),
        Of,
        id("F32"),
        Newline,
        Public,
        Type,
        id("Handle"),
        Of,
        id("T"),
        Is,
        id("Box"),
        Of,
        id("T"),
        Newline,
    ]));
}

#[test]
fn constant_declarations() {
    // const WIDTH be 768
    // public const NAME: String be "science"
    insta::assert_snapshot!(parse_report(vec![
        Const,
        id("WIDTH"),
        Be,
        int(768),
        Newline,
        Public,
        Const,
        id("NAME"),
        Colon,
        id("String"),
        Be,
        text("science"),
        Newline,
    ]));
}

#[test]
fn associated_type_in_an_interface() {
    // interface Iterate:
    //     type Item
    //     def next(mutable self) -> Option of Self.Item
    //
    // The third reading of `type`: inside an interface body it declares an
    // associated type rather than a record or an alias. What else an
    // `interface` may hold is `parse_traits.rs`'s business; this is here
    // because it is part of what the word `type` may mean.
    insta::assert_snapshot!(parse_report(vec![
        Interface,
        id("Iterate"),
        Colon,
        Newline,
        Indent,
        Type,
        id("Item"),
        Newline,
        Function,
        id("next"),
        LParen,
        Mutable,
        SelfValue,
        RParen,
        Arrow,
        id("Option"),
        Of,
        SelfType,
        Dot,
        id("Item"),
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
    // function (): x
    // type Doc:
    //     title: String
    //
    // The bad `function` is dropped; the record after it still parses.
    insta::assert_snapshot!(parse_report(vec![
        Function,
        LParen,
        RParen,
        Colon,
        id("x"),
        Newline,
        Type,
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

/// A statement and a declaration in one file, which is the first thing a
/// reader who has just met script mode writes.
///
/// It used to be `SC0101`, *expected a declaration, found `let`*, and the test
/// was called `error_statement_at_top_level`. `script-mode.md` §1.3 makes the
/// two coexist: the `let` becomes the body of the generated `def main() ->
/// Error?` that the dump below shows appended to the module, the `type` stays
/// exactly where it was, and **there is no diagnostic at all** — which is the
/// property this case now pins, because the failure mode a new top-level
/// production has is not rejecting the statement, it is rejecting the
/// declaration beside it.
///
/// The generated `main` is the desugaring made visible. Its span is the
/// statements it holds; its name, its return type and its `null` tail have
/// zero-width spans, because none of the three is in the file.
#[test]
fn a_statement_and_a_declaration_share_a_file() {
    // let x be 1
    // type Doc:
    //     title: String
    insta::assert_snapshot!(parse_report(vec![
        Let,
        id("x"),
        Be,
        int(1),
        Newline,
        Type,
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
    // def f()
    //     x
    insta::assert_snapshot!(parse_report(vec![
        Function,
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
    // def f():
    // def g(): x
    insta::assert_snapshot!(parse_report(vec![
        Function,
        id("f"),
        LParen,
        RParen,
        Colon,
        Newline,
        Function,
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
    // type S:
    //     title
    //     body: String
    //
    // The broken field is dropped and the next one still parses: one pass has
    // to be able to report more than one problem.
    insta::assert_snapshot!(parse_report(vec![
        Type,
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
        Type,
        Colon,
        Newline,
        Indent,
        id("a"),
        Colon,
        id("I32"),
        Newline,
        Dedent,
        Choice,
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
    // def f(a: %): x
    insta::assert_snapshot!(parse_report(vec![
        Function,
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

#[test]
fn error_type_neither_aliased_nor_opened() {
    // type Doc
    // type Other:
    //     title: String
    //
    // A `type` declaration has to say which of the two it is. Reporting it
    // here, rather than letting the line end quietly, is what keeps the next
    // declaration parsing.
    insta::assert_snapshot!(parse_report(vec![
        Type,
        id("Doc"),
        Newline,
        Type,
        id("Other"),
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
fn error_public_on_a_use_declaration() {
    // public use text.parser
    //
    // `public` has no meaning here, and saying so must not stop the `use`
    // itself from parsing.
    insta::assert_snapshot!(parse_report(vec![
        Public,
        Use,
        id("text"),
        Dot,
        id("parser"),
        Newline,
    ]));
}

// --- const generic arguments (§5.3) --------------------------------------
//
// `const-expression-arithmetic.md` §2.1 gives a const argument a five-operator
// grammar, and §10.1 commits F0 to the unary-negation half of it. These tests
// are that half: what parses, what does not, what the sign survives as, and
// what did not change.

/// Parses `source`, insisting both phases are quiet, and returns the module.
#[track_caller]
fn parse_clean(source: &str) -> science_parser::ast::Module {
    let file = science_diagnostics::FileId(0);
    let (tokens, lexical) = science_lexer::lex(file, source);
    assert!(
        lexical.is_empty(),
        "`{source}` does not lex cleanly: {:?}",
        lexical.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
    );
    let (module, diagnostics) = science_parser::parse_module(&tokens, file);
    assert!(
        diagnostics.is_empty(),
        "`{source}` did not parse cleanly: {:?}",
        diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
    );
    module
}

/// The generic arguments of the type the module's first alias aliases.
#[track_caller]
fn alias_arguments(module: &science_parser::ast::Module) -> Vec<science_parser::ast::Type> {
    use science_parser::ast::{ItemKind, TypeKind};
    let ItemKind::Alias(alias) = &module.items[0].kind else { panic!("not a type alias") };
    let TypeKind::Path(path) = &alias.ty.kind else { panic!("the alias target is not a path") };
    path.segments.last().unwrap().generics.clone()
}

/// The nine unit aliases of `scientific-libraries.md` §12.3, each parsed, and
/// each exponent vector read back as signed integers.
///
/// This is the bug those aliases had: before unary negation existed, six of
/// the nine failed with `SC0104: expected a type, found -` — every velocity,
/// every force, every pressure. Reading the vector back through `as_i128`
/// rather than eyeballing a dump is what pins the *sign*, as opposed to
/// pinning that a `-` was somewhere in the tree.
#[test]
fn the_nine_unit_aliases_of_scientific_libraries_12_3() {
    use science_parser::ast::TypeKind;

    // (name, the seven SI exponents: length, mass, time, current,
    //  temperature, amount, luminous)
    let aliases: &[(&str, [i128; 7])] = &[
        ("Length", [1, 0, 0, 0, 0, 0, 0]),
        ("Mass", [0, 1, 0, 0, 0, 0, 0]),
        ("Time", [0, 0, 1, 0, 0, 0, 0]),
        ("Velocity", [1, 0, -1, 0, 0, 0, 0]),
        ("Acceleration", [1, 0, -2, 0, 0, 0, 0]),
        ("Force", [1, 1, -2, 0, 0, 0, 0]),
        ("Energy", [2, 1, -2, 0, 0, 0, 0]),
        ("Power", [2, 1, -3, 0, 0, 0, 0]),
        ("Pressure", [-1, 1, -2, 0, 0, 0, 0]),
    ];

    for (name, exponents) in aliases {
        let vector = exponents.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
        let source = format!("type {name} of T is Quantity of (T, {vector})\n");
        let module = parse_clean(&source);

        let arguments = alias_arguments(&module);
        assert_eq!(arguments.len(), 8, "`{name}` takes a type and seven exponents");
        let read: Vec<Option<i128>> = arguments[1..]
            .iter()
            .map(|argument| match &argument.kind {
                TypeKind::Const(value) => value.as_i128(),
                other => panic!("`{name}`: {other:?} is not a const argument"),
            })
            .collect();
        let expected: Vec<Option<i128>> = exponents.iter().copied().map(Some).collect();
        assert_eq!(read, expected, "the exponent vector of `{name}`");
    }
}

/// Two of those aliases as the parser actually shapes them: `Velocity`, whose
/// negative exponent sits in the middle of the vector, and `Pressure`, whose
/// very first exponent is negative — the two positions a leading `-` can
/// occupy in an argument list.
#[test]
fn a_dimension_vector_with_negative_exponents() {
    insta::assert_snapshot!(common::parse_source(concat!(
        "type Velocity of T is Quantity of (T, 1, 0, -1, 0, 0, 0, 0)\n",
        "type Pressure of T is Quantity of (T, -1, 1, -2, 0, 0, 0, 0)\n",
    )));
}

/// `-0`, and the boundary of what a const argument's value can be.
///
/// The value a const argument denotes is an `i128` (F0 commitment 2 of
/// `const-expression-arithmetic.md` §10.1), so `-(2^127)` is the most negative
/// one there is. One past it is *syntax* the parser accepts, because the tree
/// keeps its shape whatever the magnitude is; `as_i128` is what says the value
/// is not representable, in one place, rather than a second range check in the
/// parser that could drift from the first. One past `u128::MAX` is not
/// expressible at all: the lexer rejects the literal and hands the parser a
/// `0`, which is where the accumulator's limit belongs.
#[test]
fn negative_zero_and_the_value_boundary() {
    use science_parser::ast::TypeKind;

    /// The const argument of `Window of (Int, <literal>)`, as a value, paired
    /// with whether the lexer was happy.
    #[track_caller]
    fn const_arg_value(literal: &str) -> (Option<i128>, bool) {
        let source = format!("type Row is Window of (Int, {literal})\n");
        let file = science_diagnostics::FileId(0);
        let (tokens, lexical) = science_lexer::lex(file, &source);
        let (module, diagnostics) = science_parser::parse_module(&tokens, file);
        assert!(
            diagnostics.is_empty(),
            "the parser complained about `{literal}`: {:?}",
            diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>(),
        );
        let arguments = alias_arguments(&module);
        let TypeKind::Const(value) = &arguments[1].kind else {
            panic!("`{literal}` did not parse as a const argument");
        };
        (value.as_i128(), lexical.is_empty())
    }

    // `-0` is `0`, and it parses: the grammar negates an atom, and `0` is one.
    assert_eq!(const_arg_value("-0"), (Some(0), true));
    assert_eq!(const_arg_value("0"), (Some(0), true));

    // `i128::MIN`, the most negative value there is, and `i128::MAX`.
    assert_eq!(
        const_arg_value("-170141183460469231731687303715884105728"),
        (Some(i128::MIN), true),
    );
    assert_eq!(
        const_arg_value("170141183460469231731687303715884105727"),
        (Some(i128::MAX), true),
    );

    // One past each end: a tree, but not a value.
    assert_eq!(const_arg_value("-170141183460469231731687303715884105729"), (None, true));
    assert_eq!(const_arg_value("170141183460469231731687303715884105728"), (None, true));

    // One past `u128::MAX` is not expressible: the *lexer* reports it, so the
    // parser still sees a well-formed const argument over the `0` it was
    // handed, and this test sees a lexical diagnostic.
    assert_eq!(const_arg_value("-340282366920938463463374607431768211456"), (Some(0), false));
}

/// `- 1`, with a space, **parses**, and means `-1`.
///
/// The lexer's rule that `->` is one token only when its two characters touch
/// does not reach here, and should not: that rule is about a lexeme that two
/// characters spell, while this is a parser production over two tokens whose
/// operand grows to `-(N + 1)`, where there is no character pair for adjacency
/// to be measured between. `- 1` is already legal unary negation in expression
/// position, and making whitespace significant in one of the two positions and
/// not the other would be a rule with exactly one instance in the language.
#[test]
fn a_space_after_the_minus_does_not_change_the_const_argument() {
    let spaced = common::parse_source("type Row is Window of (Int, - 1)\n");
    let tight = common::parse_source("type Row is Window of (Int, -1)\n");
    // Only the spans differ, so the shapes are what get compared.
    assert_eq!(common::strip_spans(&spaced), common::strip_spans(&tight));
    insta::assert_snapshot!(spaced);
}

/// A negative literal is a const *argument*, and nothing made it a type.
///
/// `def f(x: -1)` is a position where an ordinary type belongs, and it
/// stays the `SC0104` it has always been. So does a negative float, string or
/// bool inside an argument list: `const-expression-arithmetic.md` §2.3 admits
/// the const-parameter kinds `Int` and (in F1) `Shape`, and negating a string
/// is not a phrase in either, so the `-` falls through to the same "expected a
/// type" the parser reports for every other token that cannot start one.
#[test]
fn a_negative_literal_is_not_a_type() {
    insta::assert_snapshot!(parse_source_allowing_errors(concat!(
        "def f(x: -1): x\n",
        "type Bad is Window of (Int, -1.5)\n",
        "type Worse is Window of (Int, -\"a\")\n",
        "type Worst is Window of (Int, -true)\n",
    )));
}

// --- const-expression arithmetic -----------------------------------------

/// `const-expression-arithmetic.md` §2.1's grammar, which the parser accepted
/// only a literal and a negation of until now.
///
/// Everything `science-types` normalises was unreachable from a `.science`
/// file before this: its `NORMALISE`, its monomorphisation key and its linear
/// matching had no input, because no const argument in source could name a
/// const parameter at all.
#[test]
fn a_const_argument_may_name_a_parameter_and_do_arithmetic() {
    insta::assert_snapshot!(parse_source(
        "def f of (const N: Int)(w: Grid of (Int, N + 1)):
    print(\"x\")
"
    ));
}

/// §4.6's precedence, unchanged: `*` binds tighter than `+`, and the binaries
/// are left-associative.
#[test]
fn const_arithmetic_keeps_the_languages_precedence() {
    insta::assert_snapshot!(parse_source(
        "def f of (const N: Int)(w: Grid of (Int, N * 3 + 1)):
    print(\"x\")
"
    ));
}

/// The refusal that is the whole design: `*` needs a literal on one side, so
/// two parameters can be added and never multiplied.
///
/// §2.1 calls this the note's most important property — linearity enforced by
/// the productions rather than by a check afterwards — which is what lets the
/// normaliser have no case for a non-linear input at all.
#[test]
fn two_const_parameters_cannot_be_multiplied() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f of (const N: Int, const M: Int)(w: Grid of (Int, N * M)):
    print(\"x\")
"
    ));
}
