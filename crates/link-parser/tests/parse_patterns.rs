//! Patterns: every form §4.4 lists, plus the two ambiguities it deliberately
//! leaves for name resolution.

mod common;
use common::{parse_body, parse_source_allowing_errors};

/// Literal patterns of every kind, and the wildcard.
#[test]
fn literal_and_wildcard_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(x: Int) -> String:
    match x:
        0: "zero"
        'a': "letter"
        "hello": "greeting"
        true: "on"
        3.5: "float"
        _: "other"
"#
    ));
}

/// §4.4: "a bare name is a binding until resolution says otherwise." The
/// parser cannot tell `None` from a new binding, and does not try.
#[test]
fn a_bare_name_is_a_binding() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(x: Int) -> Int:
    match x:
        None: 0
        other: other
"#
    ));
}

/// Enum variant patterns, with and without a payload, and qualified (§4.5:
/// `Some(x)` and `Option.Some(x)` are the same thing).
#[test]
fn variant_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(x: Int) -> Int:
    match x:
        Ok(value): value
        Err(NotFound(key)): key
        Option.Some(v): v
        Option.None: 0
        Rect(w, h): w
"#
    ));
}

/// §4.4's struct pattern: it mirrors construction, with named fields.
#[test]
fn struct_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(p: &Point) -> String:
    match p:
        Point(x: 0, y: 0): "origin"
        Point(x: 0, y: y): "on the Y axis"
        Doc(title: t, body: _): t
"#
    ));
}

/// Tuple patterns, and the unit pattern.
#[test]
fn tuple_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(p: (Int, Int)) -> String:
    match p:
        (0, 0): "origin"
        (x, 0): "on the X axis"
        (_, _): "somewhere else"
        (): "unit"
"#
    ));
}

/// Alternatives with `|`, over literals and over variants alike.
#[test]
fn or_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(n: Int) -> String:
    match n:
        0 | 1 | 2: "small"
        Symbol(',') | Symbol(';'): "punctuation"
        _: "large"
"#
    ));
}

/// An empty argument list in a pattern takes the variant form, exactly as it
/// does in an expression (§4.4).
#[test]
fn an_empty_argument_list_in_a_pattern_is_a_variant() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(x: Int) -> Int:
    match x:
        Doc(): 0
"#
    ));
}

/// The pattern of a `for` loop goes through the same grammar.
#[test]
fn for_loop_patterns() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(pairs: &Array[(Int, Int)]):
    for (a, b) in pairs:
        println(a)
"#
    ));
}

/// §4.4: a `match` arm's `:` "cannot be found by scanning". The pattern is
/// parsed by the pattern grammar and whatever follows it is the separator —
/// which is why the colons inside a struct pattern do not end the arm.
#[test]
fn the_arm_separator_is_whatever_follows_the_pattern() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(p: &Point) -> Int:
    match p:
        Point(x: x, y: y): x
"#
    ));
}

/// A `mut` binding, which `PatternKind::Binding` carries a flag for.
#[test]
fn a_mutable_binding_pattern() {
    insta::assert_snapshot!(parse_body(
        r#"fn f(x: Int) -> Int:
    match x:
        mut n: n
"#
    ));
}

/// A pattern that cannot be parsed is reported, and the next arm still parses.
#[test]
fn a_broken_pattern_does_not_eat_the_next_arm() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"fn f(x: Int) -> Int:
    match x:
        *: 0
        1: 1
"#
    ));
}
