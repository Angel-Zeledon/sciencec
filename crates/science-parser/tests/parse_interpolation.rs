//! `f"…"` in the grammar: `strings-formatting-and-docs.md` §1.1 and §1.4.
//!
//! The parser has no recovery of its own for an interpolating literal, because
//! the lexer emits the delimiters on every path. So the claims here are about
//! *shape* — where the parts are, and that a hole is an ordinary expression at
//! full precedence — plus the two places the shape survives a diagnostic.

mod common;

use common::{shape_of_expr, shape_of_expr_despite_lexical_errors};

#[test]
fn one_hole() {
    assert_eq!(
        shape_of_expr(r#"f"{n}""#),
        "\
FString
  hole: Path `n`"
    );
}

#[test]
fn text_and_holes_alternate_in_order() {
    assert_eq!(
        shape_of_expr(r#"f"a{x}b{y}""#),
        "\
FString
  Text \"a\"
  hole: Path `x`
  Text \"b\"
  hole: Path `y`"
    );
}

#[test]
fn a_literal_with_no_holes_is_still_an_f_string() {
    assert_eq!(
        shape_of_expr(r#"f"plain""#),
        "\
FString
  Text \"plain\""
    );
}

#[test]
fn an_empty_f_string_has_no_parts() {
    assert_eq!(shape_of_expr(r#"f"""#), "FString");
}

/// §1.4: an arbitrary expression, at full precedence. `a + b * c` inside a
/// hole associates the way it does outside one, because it *is* the same
/// parser.
#[test]
fn a_hole_parses_at_full_precedence() {
    assert_eq!(
        shape_of_expr(r#"f"{a + b * c}""#),
        "\
FString
  hole: Binary `+`
    lhs: Path `a`
    rhs: Binary `*`
      lhs: Path `b`
      rhs: Path `c`"
    );
}

#[test]
fn a_hole_may_hold_a_method_chain() {
    assert_eq!(
        shape_of_expr(r#"f"{frame.rows().len()}""#),
        "\
FString
  hole: Method `len`
    receiver: Method `rows`
      receiver: Path `frame`"
    );
}

/// The `{` and `}` of a hole are not brackets in the grammar, so a record
/// literal or an array inside one closes with its own bracket and the hole
/// closes with the brace after it.
#[test]
fn a_bracket_inside_a_hole_does_not_end_it() {
    assert_eq!(
        shape_of_expr(r#"f"{xs[0]}""#),
        "\
FString
  hole: Index
    base: Path `xs`
    index: Int 0"
    );
}

/// §1.4: nested literals to any depth.
#[test]
fn an_f_string_nests() {
    assert_eq!(
        shape_of_expr(r#"f"{f"{x}"}""#),
        "\
FString
  hole: FString
    hole: Path `x`"
    );
}

/// An f-string is an ordinary primary, so a postfix binds to it.
#[test]
fn a_method_may_be_called_on_an_f_string() {
    assert_eq!(
        shape_of_expr(r#"f"{n}".len()"#),
        "\
Method `len`
  receiver: FString
    hole: Path `n`"
    );
}

// --- the shape survives the lexer's diagnostics -------------------------

/// `SC0174` is the lexer's. The parser puts an error node in the hole and adds
/// nothing, because a second complaint about one empty pair of braces helps
/// nobody.
#[test]
fn an_empty_hole_becomes_an_error_node_and_no_second_diagnostic() {
    assert_eq!(
        shape_of_expr_despite_lexical_errors(r#"f"{}""#),
        "\
FString
  hole: Error"
    );
}

/// `SC0173` is the lexer's too, and the expression before the `:` still
/// parses, so the tree is the one the corrected source would give.
#[test]
fn a_format_specification_leaves_the_expression_parsed() {
    assert_eq!(
        shape_of_expr_despite_lexical_errors(r#"f"{x:.3f}""#),
        "\
FString
  hole: Path `x`"
    );
}

// --- `r"…"` carries no marker past the lexer ----------------------------

/// A raw literal is a `String` like any other by the time the parser sees it:
/// nothing downstream needs to know how it was spelled.
#[test]
fn a_raw_string_is_an_ordinary_string_literal() {
    assert_eq!(shape_of_expr(r#"r"\d+""#), "Str \"\\\\d+\"");
}
