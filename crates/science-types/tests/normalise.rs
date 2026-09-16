//! `NORMALISE` and `EQUAL` — §10.1 item 3.
//!
//! The algebra, tested against the properties `const-expression-arithmetic.md`
//! §3 promises rather than against a handful of expressions that happen to
//! work. Each section names the decision it holds to.

mod common;
use common::atom;

use common::Scope;
use science_types::{equal, normalise, Atom, ConstEvalError, ConstExpr, NormalForm};

/// The normal form of an expression, for a test that expects one to exist.
fn form(expr: &ConstExpr) -> NormalForm {
    normalise(expr).expect("this expression has a normal form")
}

/// `(k, [(coefficient, name)])`, which is what a test wants to assert on.
fn shape(scope: &Scope, expr: &ConstExpr) -> (i128, Vec<(i128, String)>) {
    let form = form(expr);
    let terms = form
        .terms()
        .iter()
        .map(|term| (term.coefficient(), term.atom().render(&scope.defs)))
        .collect();
    (form.constant(), terms)
}

// --- the shape of the form -----------------------------------------------

#[test]
fn a_literal_is_a_constant_with_no_terms() {
    let scope = Scope::new();
    assert_eq!(shape(&scope, &scope.lit(7)), (7, vec![]));
}

#[test]
fn a_parameter_is_one_term_with_coefficient_one() {
    let mut scope = Scope::new();
    let n = scope.param("N");
    assert_eq!(shape(&scope, &scope.param_expr(n)), (0, vec![(1, "N".into())]));
}

#[test]
fn negation_negates_the_constant_and_every_coefficient() {
    let mut scope = Scope::new();
    let n = scope.param("N");
    let expr = scope.neg(scope.add(scope.param_expr(n), scope.lit(3)));
    assert_eq!(shape(&scope, &expr), (-3, vec![(-1, "N".into())]));
}

// --- Decision 3.3: commutativity and associativity ------------------------

#[test]
fn addition_commutes() {
    // §3.3: `Quantity of (T, L1 + L2, ..)` and `Quantity of (T, L2 + L1, ..)`
    // are the same type, and the library is usable because of it.
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let left = scope.add(scope.param_expr(a), scope.param_expr(b));
    let right = scope.add(scope.param_expr(b), scope.param_expr(a));

    assert!(equal(&form(&left), &form(&right)));
    assert_eq!(form(&left).render(&scope.defs), "a + b");
    assert_eq!(form(&right).render(&scope.defs), "a + b");
}

#[test]
fn addition_associates() {
    let mut scope = Scope::new();
    let (a, b, c) = (scope.param("a"), scope.param("b"), scope.param("c"));

    let grouped_left =
        scope.add(scope.add(scope.param_expr(a), scope.param_expr(b)), scope.param_expr(c));
    let grouped_right =
        scope.add(scope.param_expr(a), scope.add(scope.param_expr(b), scope.param_expr(c)));

    assert!(equal(&form(&grouped_left), &form(&grouped_right)));
}

#[test]
fn a_literal_sum_reassociates_with_the_parameter() {
    // §4.3's first row: `N / 2 + 1` versus `1 + N / 2` is "the ordinary case".
    // Without division that is `N + 1` versus `1 + N`, and it is the pair
    // §9.2 says a user must be able to see is *not* the problem.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let left = scope.add(scope.param_expr(n), scope.lit(1));
    let right = scope.add(scope.lit(1), scope.param_expr(n));

    assert!(equal(&form(&left), &form(&right)));
    assert_eq!(form(&left).render(&scope.defs), "1 + N");
}

#[test]
fn scaling_distributes_over_a_sum() {
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let factored = scope.scale(scope.add(scope.param_expr(a), scope.param_expr(b)), 2);
    let expanded =
        scope.add(scope.scale(scope.param_expr(a), 2), scope.scale(scope.param_expr(b), 2));

    assert!(equal(&form(&factored), &form(&expanded)));
    assert_eq!(form(&factored).render(&scope.defs), "2*a + 2*b");
}

#[test]
fn subtraction_is_addition_of_the_negation() {
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let subtracted = scope.sub(scope.param_expr(a), scope.param_expr(b));
    let added = scope.add(scope.param_expr(a), scope.neg(scope.param_expr(b)));

    assert!(equal(&form(&subtracted), &form(&added)));
}

// --- the no-zero-coefficient invariant ------------------------------------

#[test]
fn a_cancelled_term_leaves_the_form() {
    // `N - N + 3` is the constant 3 and not `0*N + 3`. If it were the second,
    // it would be a different type from `3` and a different monomorphisation
    // key, which is §10.1 item 4's bug with the sign flipped.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let expr = scope.add(scope.sub(scope.param_expr(n), scope.param_expr(n)), scope.lit(3));

    assert_eq!(shape(&scope, &expr), (3, vec![]));
    assert!(equal(&form(&expr), &form(&scope.lit(3))));
    assert_eq!(form(&expr).as_constant(), Some(3));
}

#[test]
fn scaling_by_zero_is_the_constant_zero() {
    // §3.2: `SCALE(f, 0)`: the constant 0.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let expr = scope.scale(scope.add(scope.param_expr(n), scope.lit(9)), 0);

    assert_eq!(shape(&scope, &expr), (0, vec![]));
    assert!(form(&expr).is_constant());
}

#[test]
fn coefficients_accumulate_before_they_cancel() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    let expr = scope.add(scope.scale(scope.param_expr(n), 3), scope.scale(scope.param_expr(n), -3));

    assert_eq!(shape(&scope, &expr), (0, vec![]));
}

// --- integer coefficients other than ±1 -----------------------------------

#[test]
fn the_rate_constant_alias_of_intrinsics_chem_bio_normalises() {
    // `intrinsics-chem-bio.md` §5.2:
    //
    //     type RateConstant of (T, const ORDER: Int) is
    //         Quantity of (T, -3 + 3 * ORDER, 0, -1, 0, 0, 1 - ORDER, 0)
    //
    // Its Finding A: "this is the project's first consumer that needs an
    // integer coefficient other than ±1", and it asks for a test case naming
    // this note. This is it.
    let mut scope = Scope::new();
    let order = scope.param("ORDER");

    let length = scope.add(scope.lit(-3), scope.scale(scope.param_expr(order), 3));
    let amount = scope.sub(scope.lit(1), scope.param_expr(order));

    assert_eq!(shape(&scope, &length), (-3, vec![(3, "ORDER".into())]));
    assert_eq!(shape(&scope, &amount), (1, vec![(-1, "ORDER".into())]));

    assert_eq!(form(&length).render(&scope.defs), "-3 + 3*ORDER");
    assert_eq!(form(&amount).render(&scope.defs), "1 - ORDER");
}

#[test]
fn a_negative_coefficient_survives_every_operation() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    let negated = scope.neg(scope.scale(scope.param_expr(n), 5));
    assert_eq!(shape(&scope, &negated), (0, vec![(-5, "N".into())]));

    let scaled_again = scope.scale(scope.neg(scope.param_expr(n)), 5);
    assert!(equal(&form(&negated), &form(&scaled_again)));

    // And a negative coefficient is not the same form as its positive twin.
    let positive = scope.scale(scope.param_expr(n), 5);
    assert!(!equal(&form(&negated), &form(&positive)));
}

#[test]
fn the_sqrt_signature_of_section_7_2_normalises() {
    // §7.2 declares `sqrt` with *doubled* exponents in the parameter type:
    // `Quantity of (F64, L * 2, M * 2, ..)`. The whole mechanism rests on
    // `L * 2` being a form with coefficient 2, which `tests/matching.rs` then
    // inverts.
    let mut scope = Scope::new();
    let l = scope.param("L");
    let doubled = scope.scale(scope.param_expr(l), 2);
    assert_eq!(shape(&scope, &doubled), (0, vec![(2, "L".into())]));
}

// --- Decision 3.1: atoms are compared by identity, not by name ------------

#[test]
fn two_parameters_with_the_same_name_are_two_atoms() {
    // The property that makes the `DefId` in `ConstExprKind::Param` earn its
    // place: `N` in one signature and `N` in another are different parameters,
    // and calling them equal would make two unrelated generics one type.
    let mut scope = Scope::new();
    let first = scope.param("N");
    let second = scope.param("N");

    let left = scope.param_expr(first);
    let right = scope.param_expr(second);

    assert!(!equal(&form(&left), &form(&right)));
    // And they render identically, which is exactly why §9.2 needs a legend
    // pointing at where each was bound.
    assert_eq!(form(&left).render(&scope.defs), form(&right).render(&scope.defs));
}

#[test]
fn expressions_differing_only_in_which_parameter_they_name_are_not_equal() {
    let mut scope = Scope::new();
    let (a, b, c) = (scope.param("a"), scope.param("b"), scope.param("c"));

    let left = scope.add(scope.param_expr(a), scope.param_expr(b));
    let right = scope.add(scope.param_expr(a), scope.param_expr(c));

    assert!(!equal(&form(&left), &form(&right)));
}

#[test]
fn a_differing_coefficient_is_a_differing_form() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    let two = scope.scale(scope.param_expr(n), 2);
    let three = scope.scale(scope.param_expr(n), 3);

    assert!(!equal(&form(&two), &form(&three)));
}

#[test]
fn a_differing_constant_is_a_differing_form() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    let plus_one = scope.add(scope.param_expr(n), scope.lit(1));
    let plus_two = scope.add(scope.param_expr(n), scope.lit(2));

    assert!(!equal(&form(&plus_one), &form(&plus_two)));
}

// --- the terms are sorted, and there are no duplicates --------------------

#[test]
fn terms_are_strictly_increasing_whatever_order_they_were_written_in() {
    let mut scope = Scope::new();
    let ids: Vec<_> = ["a", "b", "c", "d"].iter().map(|name| scope.param(name)).collect();

    // Written backwards, and with a repeat that has to merge.
    let expr = scope.add(
        scope.add(scope.param_expr(ids[3]), scope.param_expr(ids[1])),
        scope.add(scope.param_expr(ids[2]), scope.param_expr(ids[1])),
    );
    let form = form(&expr);

    let atoms: Vec<Atom> = form.atoms().collect();
    assert_eq!(atoms, vec![atom(ids[1]), atom(ids[2]), atom(ids[3])]);
    assert!(atoms.windows(2).all(|pair| pair[0] < pair[1]), "strictly increasing");
    assert_eq!(form.coefficient_of(atom(ids[1])), 2);
    assert_eq!(form.coefficient_of(atom(ids[0])), 0, "an absent atom has coefficient 0");
}

// --- provenance (§9.3) ----------------------------------------------------

#[test]
fn every_term_carries_the_span_of_every_operand_that_contributed_it() {
    // §9.3's derivation block is one row per contributing operand, so a merged
    // term has to remember all of them: `density * speed * speed` contributes
    // three rows to one length exponent, and printing two of them is printing
    // the wrong sum.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let first = scope.param_expr(n);
    let second = scope.param_expr(n);
    let (first_span, second_span) = (first.span, second.span);

    let expr = scope.add(first, second);
    let form = form(&expr);

    assert_eq!(form.terms().len(), 1);
    assert_eq!(form.terms()[0].coefficient(), 2);
    assert_eq!(form.terms()[0].provenance(), &[first_span, second_span]);
}

#[test]
fn scaling_carries_provenance_through_unchanged() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    let operand = scope.param_expr(n);
    let span = operand.span;
    let expr = scope.scale(operand, 4);

    assert_eq!(form(&expr).terms()[0].provenance(), &[span]);
}

#[test]
fn provenance_is_not_part_of_equality() {
    // The claim `mono.rs` rests on: two spellings of one expression, written
    // in different places, are one type and one monomorphisation key.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let here = scope.add(scope.param_expr(n), scope.lit(1));
    let there = scope.add(scope.lit(1), scope.param_expr(n));

    assert_ne!(
        form(&here).terms()[0].provenance(),
        form(&there).terms()[0].provenance(),
        "the test is only meaningful if the provenances actually differ"
    );
    assert!(equal(&form(&here), &form(&there)));
}

// --- overflow -------------------------------------------------------------

#[test]
fn a_constant_that_leaves_i128_is_refused_rather_than_wrapped() {
    let scope = Scope::new();
    let expr = scope.scale(scope.lit(i128::MAX), 2);

    let error = normalise(&expr).expect_err("this does not fit in an i128");
    assert_eq!(error, ConstEvalError::Overflow { span: Some(expr.span) });
}

#[test]
fn a_coefficient_that_leaves_i128_is_refused() {
    let mut scope = Scope::new();
    let n = scope.param("N");
    let expr = scope.scale(scope.scale(scope.param_expr(n), i128::MAX), 2);

    assert!(normalise(&expr).is_err());
}

#[test]
fn overflow_is_blamed_on_the_node_that_overflowed() {
    // The inner scale is fine and the outer one is not, so the blame has to
    // land on the outer one: the factor a reader would change.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let inner = scope.scale(scope.param_expr(n), 1 << 100);
    let inner_span = inner.span;
    let outer = scope.scale(inner, 1 << 100);

    let error = normalise(&outer).expect_err("overflows");
    assert_eq!(error.span(), Some(outer.span));
    assert_ne!(error.span(), Some(inner_span));
}

// --- rendering ------------------------------------------------------------

#[test]
fn the_written_form_parenthesises_only_where_precedence_needs_it() {
    // §9.2's left-hand column. A rendering that parenthesises everything is a
    // rendering a reader cannot compare against what they typed.
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let sum = scope.add(scope.param_expr(a), scope.param_expr(b));
    assert_eq!(sum.render(&scope.defs), "a + b");

    let scaled_sum = scope.scale(scope.add(scope.param_expr(a), scope.param_expr(b)), 2);
    assert_eq!(scaled_sum.render(&scope.defs), "(a + b) * 2");

    let sum_of_scaled = scope.add(scope.scale(scope.param_expr(a), 2), scope.param_expr(b));
    assert_eq!(sum_of_scaled.render(&scope.defs), "a * 2 + b");

    let negated_sum = scope.neg(scope.add(scope.param_expr(a), scope.param_expr(b)));
    assert_eq!(negated_sum.render(&scope.defs), "-(a + b)");

    let subtracted_sum =
        scope.sub(scope.param_expr(a), scope.add(scope.param_expr(b), scope.lit(1)));
    assert_eq!(
        subtracted_sum.render(&scope.defs),
        "a - (b + 1)",
        "the right operand of a left-associative `-` keeps its parentheses"
    );
}

#[test]
fn a_constant_form_renders_as_its_constant_even_when_it_is_zero() {
    let scope = Scope::new();
    assert_eq!(form(&scope.lit(0)).render(&scope.defs), "0");
    assert_eq!(form(&scope.lit(-4)).render(&scope.defs), "-4");
}
