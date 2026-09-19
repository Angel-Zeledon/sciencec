//! `SC0542` and Decision 2's inference, applied to a module-level `const`
//! (§4.4), through [`science_types::constant::lower`].
//!
//! Before this file's own fix, an unannotated `const` took [`Ty::ERROR`]
//! silently (`items.rs`'s `Const` arm read a comment promising a "body walk"
//! that did not exist), and `const BAD be side_effecting_call()` passed
//! `check` clean whatever it was annotated with. Every test below is a claim
//! this crate could not have made before that fix: an unannotated `const`'s
//! type is real, it is what MIR substitutes at a reference, and a `const`
//! whose initialiser is not a literal is now an error rather than a silence.

mod support;

use science_resolve::hir::{DefKind, Literal};
use support::check;

/// The two constants `examples/02_bindings.science` actually declares —
/// unannotated, and of two different literal kinds.
const FIXTURE: &str = "\
const WIDTH be 768
const GREETING be \"hola\"

def side_effecting_call() -> Int:
    42
";

#[test]
fn an_unannotated_integer_const_infers_decision_2s_default() {
    let checked = check(FIXTURE);
    checked.assert_clean();
    let width = checked.def("WIDTH", DefKind::Const);
    let ty = checked.decls.const_ty(width).expect("WIDTH has a type");
    assert_eq!(checked.render(ty), "I64");
}

#[test]
fn an_unannotated_string_const_infers_string() {
    let checked = check(FIXTURE);
    checked.assert_clean();
    let greeting = checked.def("GREETING", DefKind::Const);
    let ty = checked.decls.const_ty(greeting).expect("GREETING has a type");
    assert_eq!(checked.render(ty), "String");
}

#[test]
fn an_unannotated_consts_literal_is_ready_for_mir_to_substitute() {
    let checked = check(FIXTURE);
    let width = checked.def("WIDTH", DefKind::Const);
    let value = checked.decls.const_value(width).expect("WIDTH folds to a literal");
    assert!(matches!(value, Literal::Int { value: 768, .. }));

    let greeting = checked.def("GREETING", DefKind::Const);
    let value = checked.decls.const_value(greeting).expect("GREETING folds to a literal");
    assert_eq!(value, &Literal::Str("hola".to_string()));
}

#[test]
fn an_annotated_literal_const_still_substitutes() {
    let checked = check("const GOOD: I64 be 768\n");
    checked.assert_clean();
    let good = checked.def("GOOD", DefKind::Const);
    assert_eq!(checked.render(checked.decls.const_ty(good).unwrap()), "I64");
    assert!(matches!(
        checked.decls.const_value(good),
        Some(Literal::Int { value: 768, .. })
    ));
}

#[test]
fn a_call_as_a_consts_initialiser_is_sc0542_not_a_silence() {
    let checked = check(&(FIXTURE.to_string() + "\nconst BAD be side_effecting_call()\n"));
    assert_eq!(checked.codes(), vec![science_types::codes::CONST_INITIALISER_NOT_A_LITERAL.0]);
    let bad = checked.def("BAD", DefKind::Const);
    // No annotation and no literal to infer from: the pre-existing admission.
    assert_eq!(checked.render(checked.decls.const_ty(bad).unwrap()), "{unknown}");
    assert!(checked.decls.const_value(bad).is_none());
}

#[test]
fn an_annotated_call_initialiser_is_also_sc0542() {
    // Before this fix, `check` reported nothing at all for this program —
    // claim 3 of the audit this file closes, named by example in its message.
    let checked = check(&(FIXTURE.to_string() + "\nconst BAD: Int be side_effecting_call()\n"));
    assert_eq!(checked.codes(), vec![science_types::codes::CONST_INITIALISER_NOT_A_LITERAL.0]);
    let bad = checked.def("BAD", DefKind::Const);
    // The annotation still lowers: a bad initialiser must not swallow it.
    assert_eq!(checked.render(checked.decls.const_ty(bad).unwrap()), "I64");
    assert!(checked.decls.const_value(bad).is_none());
}

#[test]
fn a_literal_that_disagrees_with_its_annotation_does_not_substitute() {
    // `5` is an integer literal and `F64` is not one of §5.1's integer types:
    // `constant`'s module doc §4 is why this must not become a `Literal::Int`
    // read at an `F64` slot. No diagnostic either — the mismatch is exactly
    // as unchecked as it always was outside this module — but nothing here
    // must claim a value it cannot confirm.
    let checked = check("const MISMATCH: F64 be 5\n");
    checked.assert_clean();
    let def = checked.def("MISMATCH", DefKind::Const);
    assert_eq!(checked.render(checked.decls.const_ty(def).unwrap()), "F64");
    assert!(checked.decls.const_value(def).is_none());
}
