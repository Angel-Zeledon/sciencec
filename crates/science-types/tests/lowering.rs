//! `SC0260` — the clause this crate owns: what a const argument's literal may
//! be.
//!
//! §2.3's house pattern is that the parser accepts whatever `parse_type` and
//! `literal_of` read in a const-argument position and *"lets a later phase say
//! so with a better message"*. This is that phase, and [`lower`] is the seam.
//!
//! The other clauses of §9's `SC0260` row are elsewhere by design: an operator
//! outside §2.1 and an operator in the bare `of X` form are the parser's, a
//! bad const-parameter kind is the resolver's `SC0220`, and `*` or `/` with no
//! literal operand cannot be represented in this crate's tree at all (§2.1 —
//! the productions, not a check).

use science_diagnostics::{FileId, Span};
use science_lexer::{IntBase, NumSuffix};
use science_resolve::hir::{ConstExpr as AstConstExpr, ConstExprKind, Literal};
use science_types::{AtomOrder, codes, lower, normalise, ConstExprKind as Lowered};

const FILE: FileId = FileId(0);

fn span() -> Span {
    Span::new(FILE, 10, 14)
}

fn node(kind: ConstExprKind) -> AstConstExpr {
    AstConstExpr { kind, span: span() }
}

fn int(value: u128) -> AstConstExpr {
    node(ConstExprKind::Lit(Literal::Int { value, base: IntBase::Dec, suffix: None }))
}

// --- what lowers ----------------------------------------------------------

#[test]
fn an_unsuffixed_integer_lowers_to_its_value() {
    let lowered = lower(&int(768), &AtomOrder::default()).expect("an integer is a const expression");
    assert_eq!(lowered.kind, Lowered::Lit(768));
    assert_eq!(normalise(&lowered).unwrap().as_constant(), Some(768));
}

#[test]
fn a_negated_integer_lowers_to_a_signed_value() {
    // §10.1 item 2, and the reason `scientific-libraries.md` §12.3's nine unit
    // aliases can be written: roughly half of every SI dimension vector is
    // negative.
    let lowered = lower(&node(ConstExprKind::Neg(Box::new(int(1)))), &AtomOrder::default())
        .expect("a negated integer is a const expression");
    assert_eq!(lowered.kind, Lowered::Lit(-1));
}

#[test]
fn the_most_negative_value_survives_the_boundary() {
    // `-(2^127)` is representable although `+2^127` is not, so a negated
    // literal folds at the boundary rather than being negated afterwards
    // through an `i128` that cannot hold it.
    let magnitude = 1u128 << 127;
    let lowered = lower(&node(ConstExprKind::Neg(Box::new(int(magnitude)))), &AtomOrder::default())
        .expect("i128::MIN is in range");
    assert_eq!(lowered.kind, Lowered::Lit(i128::MIN));
}

#[test]
fn a_hexadecimal_literal_is_the_same_const_expression_as_its_decimal() {
    // §2.1: "`IntLiteral` is an unsuffixed integer literal in any base." The
    // base is a spelling and the normal form is a value, so `0x10` and `16`
    // are one type.
    let hex = node(ConstExprKind::Lit(Literal::Int {
        value: 16,
        base: IntBase::Hex,
        suffix: None,
    }));
    assert_eq!(lower(&hex, &AtomOrder::default()).unwrap().kind, lower(&int(16), &AtomOrder::default()).unwrap().kind);
}

// --- what does not --------------------------------------------------------

#[test]
fn a_suffixed_integer_is_refused_with_a_fix() {
    let suffixed = node(ConstExprKind::Lit(Literal::Int {
        value: 4,
        base: IntBase::Dec,
        suffix: Some(NumSuffix::I32),
    }));
    let diagnostic = lower(&suffixed, &AtomOrder::default()).expect_err("a suffix names a type");

    assert_eq!(diagnostic.code, codes::NOT_A_CONST_EXPRESSION);
    assert_eq!(diagnostic.primary_span(), Some(span()));
    assert_eq!(diagnostic.suggestions.len(), 1, "deleting the suffix is always the fix");
    assert!(diagnostic.suggestions[0].replacement.is_empty());
}

#[test]
fn every_literal_that_is_not_an_integer_is_refused() {
    // The parser has always accepted `Window of (Int, "a")` here. §2.3 refused
    // `Bool` because `broadcasting.md` §7.3 rejected its only caller, and
    // `F64` because float equality is not equality.
    let cases = [
        Literal::Float { value: 1.5, suffix: None },
        Literal::Str("a".to_string()),
        Literal::Char('a'),
        Literal::Bool(true),
        Literal::Null,
    ];
    for literal in cases {
        let diagnostic =
            lower(&node(ConstExprKind::Lit(literal.clone())), &AtomOrder::default()).expect_err("not an integer");
        assert_eq!(diagnostic.code, codes::NOT_A_CONST_EXPRESSION, "for {literal:?}");
        assert_eq!(diagnostic.labels.len(), 1);
        assert_eq!(diagnostic.notes.len(), 1);
    }
}

#[test]
fn a_negated_non_integer_is_refused_at_the_literal() {
    // The blame lands on the literal, not on the `-`: that is the token the
    // reader has to change.
    let inner = node(ConstExprKind::Lit(Literal::Float { value: 1.5, suffix: None }));
    let inner_span = inner.span;
    let diagnostic =
        lower(&node(ConstExprKind::Neg(Box::new(inner))), &AtomOrder::default()).expect_err("not an integer");

    assert_eq!(diagnostic.primary_span(), Some(inner_span));
}

#[test]
fn an_integer_too_large_for_i128_is_refused_rather_than_truncated() {
    let diagnostic = lower(&int(u128::MAX), &AtomOrder::default()).expect_err("outside i128");
    assert_eq!(diagnostic.code, codes::NOT_A_CONST_EXPRESSION);
}

#[test]
fn a_negated_integer_one_past_the_floor_is_refused() {
    let diagnostic = lower(&node(ConstExprKind::Neg(Box::new(int((1u128 << 127) + 1)))), &AtomOrder::default())
        .expect_err("one past i128::MIN");
    assert_eq!(diagnostic.code, codes::NOT_A_CONST_EXPRESSION);
}
