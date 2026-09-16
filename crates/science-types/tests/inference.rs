//! The inference context: the union-find, and what it refuses to do.
//!
//! No source, no resolver and no fixture, because none of this is about a
//! program: a variable is an index and the claims are about the classes it
//! forms. The types it binds to are built directly with [`Types`], which is the
//! only consumer shape that matters here — the context never looks inside one.

use science_diagnostics::{FileId, Span};
use science_types::infer::{InferTy, Inference, UnifyError};
use science_types::ty::{Ty, Types};

const FILE: FileId = FileId(0);

fn span(at: u32) -> Span {
    Span::new(FILE, at, at + 1)
}

/// Two distinct types to bind to, and the table they came from.
fn table() -> (Types, Ty, Ty) {
    let mut types = Types::new();
    let pair = types.tuple(vec![Ty::UNIT, Ty::UNIT]);
    let triple = types.tuple(vec![Ty::UNIT, Ty::UNIT, Ty::UNIT]);
    (types, pair, triple)
}

// --- the union-find ------------------------------------------------------

#[test]
fn a_fresh_variable_is_its_own_class_and_is_unbound() {
    let mut inference = Inference::new();
    assert!(inference.is_empty());
    let var = inference.fresh(span(4));
    assert_eq!(inference.len(), 1);
    assert_eq!(inference.find(var), var);
    assert_eq!(inference.binding(var), None);
    assert_eq!(inference.origin(var), span(4));
}

#[test]
fn unifying_two_variables_puts_them_in_one_class() {
    let (types, _, _) = table();
    let mut inference = Inference::new();
    let (a, b) = (inference.fresh(span(0)), inference.fresh(span(2)));

    inference.unify(&types, InferTy::Var(a), InferTy::Var(b)).expect("two holes agree");
    assert_eq!(inference.find(a), inference.find(b));
    // Still unknown: joining two holes makes one hole, not an answer.
    assert_eq!(inference.binding(a), None);
    assert_eq!(inference.unresolved().len(), 1, "one class, one message");
}

#[test]
fn binding_one_member_binds_the_class() {
    // The property the whole structure exists for: a type learned at one
    // expression is known at every expression that joined it, whichever order
    // the checker visited them in.
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    let (a, b, c) = (inference.fresh(span(0)), inference.fresh(span(2)), inference.fresh(span(4)));

    inference.unify(&types, InferTy::Var(a), InferTy::Var(b)).unwrap();
    inference.unify(&types, InferTy::Var(b), InferTy::Var(c)).unwrap();
    inference.bind(&types, c, pair).unwrap();

    assert_eq!(inference.binding(a), Some(pair));
    assert_eq!(inference.resolve(InferTy::Var(a)), InferTy::Known(pair));
    assert!(inference.unresolved().is_empty());
}

#[test]
fn unifying_a_bound_class_with_a_bound_class_compares_the_types() {
    let (types, pair, triple) = table();
    let mut inference = Inference::new();
    let (a, b) = (inference.fresh(span(0)), inference.fresh(span(2)));
    inference.bind(&types, a, pair).unwrap();
    inference.bind(&types, b, triple).unwrap();

    let failure = inference
        .unify(&types, InferTy::Var(a), InferTy::Var(b))
        .expect_err("a pair is not a triple");
    assert_eq!(failure, UnifyError::Mismatch { left: pair, right: triple });
}

#[test]
fn binding_a_class_twice_to_two_types_is_a_mismatch_and_not_a_replacement() {
    // The silent failure this refuses: the second answer overwriting the first
    // means the body checks against whichever expression the checker happened
    // to visit last.
    let (types, pair, triple) = table();
    let mut inference = Inference::new();
    let var = inference.fresh(span(0));
    assert_eq!(inference.bind(&types, var, pair).unwrap(), pair);
    assert!(inference.bind(&types, var, triple).is_err());
    assert_eq!(inference.binding(var), Some(pair), "the first answer stands");
}

#[test]
fn a_long_chain_resolves_and_the_path_is_compressed() {
    // `infer`'s §4: path halving, on `&mut self`, which is Decision 24. The
    // observable part is that the answer is the same and that asking twice is
    // not asking twice as far.
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    let chain: Vec<_> = (0..64).map(|at| inference.fresh(span(at * 2))).collect();
    for window in chain.windows(2) {
        inference.unify(&types, InferTy::Var(window[0]), InferTy::Var(window[1])).unwrap();
    }
    inference.bind(&types, chain[0], pair).unwrap();

    for var in &chain {
        assert_eq!(inference.binding(*var), Some(pair));
    }
    let root = inference.find(chain[63]);
    for var in &chain {
        assert_eq!(inference.find(*var), root);
    }
    assert_eq!(inference.unresolved(), Vec::new());
}

// --- what unification is, and is not -------------------------------------

#[test]
fn unification_is_equality_and_not_assignability() {
    // `infer`'s §5. A `T` reaching a `T?` is a coercion, which is a fact about
    // an *assignment*; making `unify` accept it would make the direction of
    // every inference edge significant and the result depend on the order two
    // arms were visited in.
    let (mut types, pair, _) = table();
    let nullable_pair = types.nullable(pair);
    let mut inference = Inference::new();

    let failure = inference
        .unify(&types, InferTy::Known(pair), InferTy::Known(nullable_pair))
        .expect_err("`T` and `T?` are two types");
    assert_eq!(failure, UnifyError::Mismatch { left: pair, right: nullable_pair });
}

#[test]
fn an_already_reported_type_agrees_with_whatever_it_meets_and_keeps_the_other() {
    // `ty`'s §5 through this relation, with one addition of its own: the class
    // ends up standing for the type that is still *known*, so a mistake in one
    // expression does not spread the error type through every variable the
    // body joined to it.
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    let var = inference.fresh(span(0));

    inference.bind(&types, var, Ty::ERROR).unwrap();
    assert_eq!(inference.bind(&types, var, pair).unwrap(), pair);
    assert_eq!(inference.binding(var), Some(pair));

    let settled = inference
        .unify(&types, InferTy::Known(Ty::ERROR), InferTy::Known(pair))
        .expect("an erroneous type agrees");
    assert_eq!(settled, InferTy::Known(pair));
}

#[test]
fn unifying_a_variable_with_a_type_answers_it_from_either_side() {
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    let (left, right) = (inference.fresh(span(0)), inference.fresh(span(2)));

    assert_eq!(
        inference.unify(&types, InferTy::Var(left), InferTy::Known(pair)).unwrap(),
        InferTy::Known(pair)
    );
    assert_eq!(
        inference.unify(&types, InferTy::Known(pair), InferTy::Var(right)).unwrap(),
        InferTy::Known(pair)
    );
    assert_eq!(inference.binding(left), Some(pair));
    assert_eq!(inference.binding(right), Some(pair));
}

#[test]
fn a_known_type_is_an_infer_ty_and_resolving_one_is_the_identity() {
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    assert_eq!(inference.resolve(InferTy::from(pair)), InferTy::Known(pair));
    assert_eq!(InferTy::from(pair).known(), Some(pair));
    assert_eq!(InferTy::Var(inference.fresh(span(0))).known(), None);
    let _ = types;
}

#[test]
fn every_unresolved_class_is_listed_once_and_a_bound_one_is_not_listed() {
    // This is Decision 2's defaulting list and the "type annotations needed"
    // list, which `infer`'s §5 leaves to the phase that has the body.
    let (types, pair, _) = table();
    let mut inference = Inference::new();
    let (a, b, c, d) = (
        inference.fresh(span(0)),
        inference.fresh(span(2)),
        inference.fresh(span(4)),
        inference.fresh(span(6)),
    );
    inference.unify(&types, InferTy::Var(a), InferTy::Var(b)).unwrap();
    inference.bind(&types, c, pair).unwrap();

    let unresolved = inference.unresolved();
    assert_eq!(unresolved.len(), 2, "the `a`/`b` class, and `d`");
    assert!(unresolved.contains(&inference.find(a)));
    assert!(unresolved.contains(&inference.find(d)));
    assert!(!unresolved.contains(&inference.find(c)));
    // The span each message points at survives the union.
    assert_eq!(inference.origin(d), span(6));
}
