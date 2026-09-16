//! One-variable linear matching — §10.1 item 7.
//!
//! > **Decision 7.1.** `p = (v − k) / c`, and it is an error if `c` does not
//! > divide `v − k`.
//!
//! A division with a remainder check. The remainder check is the error, so it
//! gets the most tests.

mod common;

use common::Scope;
use science_types::{match_linear, normalise, ConstExpr, Match, MatchError, NormalForm};

fn form(expr: &ConstExpr) -> NormalForm {
    normalise(expr).expect("normalises")
}

// --- the determined cases -------------------------------------------------

#[test]
fn a_bare_parameter_is_determined_at_c_one_k_zero() {
    // `Mul` on quantities: `self: Quantity of (T, L1, M1, ..)` determines
    // every exponent, and nothing is written at the call site.
    let mut scope = Scope::new();
    let l = scope.param("L1");

    let solved = match_linear(&form(&scope.param_expr(l)), -3).expect("determined");
    assert_eq!(solved, Match { param: l, value: -3 });
}

#[test]
fn the_sqrt_of_an_area_is_a_length() {
    // §7.2: `sqrt` carries doubled exponents in its parameter type. An `Area`
    // has `L = 2`, the site is `L * 2`, so `c = 2, k = 0, v = 2` and `L = 1`.
    let mut scope = Scope::new();
    let l = scope.param("L");
    let site = form(&scope.scale(scope.param_expr(l), 2));

    assert_eq!(match_linear(&site, 2), Ok(Match { param: l, value: 1 }));
}

#[test]
fn an_offset_site_subtracts_before_it_divides() {
    // `concatenate`-shaped: `k + c·p`.
    let mut scope = Scope::new();
    let n = scope.param("N");
    let site = form(&scope.add(scope.scale(scope.param_expr(n), 3), scope.lit(4)));

    assert_eq!(match_linear(&site, 19), Ok(Match { param: n, value: 5 }));
}

#[test]
fn a_negative_coefficient_inverts() {
    // `intrinsics-chem-bio.md` §5.2 Finding B: solving `1 - ORDER = -1` gives
    // `ORDER = 2`, which is how §5.3's diagnostic gets to say "`k` is a
    // second-order rate constant" without the compiler knowing any chemistry.
    // Coefficient `-1`, offset `1`.
    let mut scope = Scope::new();
    let order = scope.param("ORDER");
    let amount_exponent = form(&scope.sub(scope.lit(1), scope.param_expr(order)));

    assert_eq!(match_linear(&amount_exponent, -1), Ok(Match { param: order, value: 2 }));

    // And the length exponent of the same alias, `-3 + 3 * ORDER`, agrees:
    // a second-order rate constant has length exponent 3.
    let length_exponent =
        form(&scope.add(scope.lit(-3), scope.scale(scope.param_expr(order), 3)));
    assert_eq!(match_linear(&length_exponent, 3), Ok(Match { param: order, value: 2 }));
}

#[test]
fn a_negative_solution_is_a_solution() {
    // Nothing in §7.1 says a const parameter is non-negative, and §4.2
    // explicitly refuses to add sign information. A negative `ORDER` is a
    // strange reaction and a well-formed match.
    let mut scope = Scope::new();
    let n = scope.param("N");
    let site = form(&scope.scale(scope.param_expr(n), 2));

    assert_eq!(match_linear(&site, -6), Ok(Match { param: n, value: -3 }));
}

// --- the remainder check, which is the error ------------------------------

#[test]
fn the_sqrt_of_a_volume_fails_the_divisibility_test() {
    // §7.2: a `Volume` has `L = 3`, `2` does not divide `3`, and that is
    // `SC0262` — reported as `unit-literals.md`'s `SC0254` where that code
    // describes it better (§9.1). This is the mechanism §12.6's
    // rational-exponent exclusion is enforced by.
    let mut scope = Scope::new();
    let l = scope.param("L");
    let site = form(&scope.scale(scope.param_expr(l), 2));

    assert_eq!(
        match_linear(&site, 3),
        Err(MatchError::Indivisible { param: l, coefficient: 2, offset: 0, value: 3 })
    );
}

#[test]
fn the_remainder_check_accounts_for_the_offset() {
    // `4 + 3·N` against `18`: the numerator is `14`, not `18`, and `3` divides
    // neither. Checking `v` rather than `v − k` would be the same answer here
    // and the wrong one at `v = 18, k = 3`.
    let mut scope = Scope::new();
    let n = scope.param("N");
    let site = form(&scope.add(scope.scale(scope.param_expr(n), 3), scope.lit(4)));

    assert_eq!(
        match_linear(&site, 18),
        Err(MatchError::Indivisible { param: n, coefficient: 3, offset: 4, value: 18 })
    );
    // And one away is exact.
    assert_eq!(match_linear(&site, 19), Ok(Match { param: n, value: 5 }));
}

#[test]
fn a_negative_coefficient_still_has_a_remainder_check() {
    let mut scope = Scope::new();
    let order = scope.param("ORDER");
    let site = form(&scope.sub(scope.lit(1), scope.scale(scope.param_expr(order), 2)));

    assert_eq!(
        match_linear(&site, 0),
        Err(MatchError::Indivisible { param: order, coefficient: -2, offset: 1, value: 0 })
    );
    assert_eq!(match_linear(&site, -1), Ok(Match { param: order, value: 1 }));
}

// --- the positions that are not inference sites ---------------------------

#[test]
fn a_concrete_position_is_not_an_inference_site() {
    // `Tensor of (F32, 768)` determines nothing. It is a *check*, and §7.1
    // says so: a parameter determined at no site must be given explicitly.
    let scope = Scope::new();
    assert_eq!(
        match_linear(&form(&scope.lit(768)), 768),
        Err(MatchError::NotAnInferenceSite { atoms: 0 })
    );
}

#[test]
fn a_position_mentioning_two_parameters_is_not_an_inference_site() {
    // §7.1 declines rather than guessing, because the general case is integer
    // linear systems with unknown structure — §5.4's refusal.
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));
    let site = form(&scope.add(scope.param_expr(a), scope.param_expr(b)));

    assert_eq!(match_linear(&site, 10), Err(MatchError::NotAnInferenceSite { atoms: 2 }));
}

#[test]
fn a_cancelled_parameter_leaves_no_inference_site_behind() {
    // `N - N + 5` is the constant `5`, so it is not a site — the same
    // no-zero-coefficient invariant, seen from here.
    let mut scope = Scope::new();
    let n = scope.param("N");
    let site = form(&scope.add(scope.sub(scope.param_expr(n), scope.param_expr(n)), scope.lit(5)));

    assert_eq!(match_linear(&site, 5), Err(MatchError::NotAnInferenceSite { atoms: 0 }));
}

// --- the extremes ---------------------------------------------------------

#[test]
fn matching_does_not_panic_at_the_edges_of_i128() {
    let mut scope = Scope::new();
    let n = scope.param("N");

    // `v − k` leaves the range.
    let offset = form(&scope.add(scope.param_expr(n), scope.lit(-1)));
    assert_eq!(match_linear(&offset, i128::MAX), Err(MatchError::Overflow));

    // `i128::MIN / -1` leaves the range.
    let negated = form(&scope.neg(scope.param_expr(n)));
    assert_eq!(match_linear(&negated, i128::MIN), Err(MatchError::Overflow));
}
