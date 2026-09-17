//! `assign`'s §4 and §4a, lowered: the two unsizings, and what tells them apart
//! once they are here.
//!
//! **Why this file exists.** A `Coercion` variant nothing lowers is worse than
//! the refusal it replaces, so `assign`'s §4a is not landed by `assignable`
//! returning it — it is landed by this crate producing an `Rvalue::Coerce` for
//! it, with the right type and the right operand, on the program the corpus
//! actually writes. `examples/08_dyn_dispatch.science` is that program: it
//! holds both unsizings, it holds them in one body, and it is the file six of
//! the checker's seven pinned diagnostics came from.
//!
//! **Nothing in this crate was taught the new variant, and that is the finding
//! rather than an omission.** `lower`'s `ExprKind::Coerce` arm carries whatever
//! the checker decided into `Rvalue::Coerce` without inspecting it, because
//! Decision 3 makes the *checker* the phase that decides which conversion this
//! is and this phase the one that writes it down. So the tests below are not
//! *"the code I added works"*; they are the claim that the generic path is
//! adequate for this variant — a claim that can be false, and
//! [`the_two_unsizings_differ_only_in_the_type_they_produce`] is the shape of
//! it being false.
//!
//! **What a backend reads.** Both variants are one statement, no terminator and
//! no call: neither allocates. What a backend must not confuse is what the
//! result *owns*, and at this level that is the rvalue's `ty` — `borrowed any
//! Summarize` against `Box of any Summarize` — beside the variant itself.

mod support;

use science_mir::mir::{Operand, Rvalue, StatementKind, TerminatorKind};
use science_types::assign::Coercion;
use support::{Lowered, lower};

fn dispatch() -> Lowered {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/08_dyn_dispatch.science");
    let source = std::fs::read_to_string(path).expect("the dynamic-dispatch example");
    lower(&source)
}

/// Every coercion in a body: the variant, how its operand is read, and the type
/// the rvalue produces.
fn coercions(lowered: &Lowered, name: &str) -> Vec<(Coercion, &'static str, String)> {
    let mut out = Vec::new();
    for (_, block) in lowered.body(name).blocks() {
        for statement in &block.statements {
            let StatementKind::Assign { rvalue: Rvalue::Coerce { coercion, operand, ty }, .. } =
                &statement.kind
            else {
                continue;
            };
            let mode = match operand {
                Operand::Move(_) => "move",
                Operand::Copy(_) => "copy",
                Operand::Const(_) => "const",
            };
            out.push((*coercion, mode, lowered.types.render(&lowered.krate.defs, *ty)));
        }
    }
    out
}

/// The corpus file this change was made for checks clean and reaches this
/// crate.
///
/// `lower` requires resolution to be clean and deliberately does not require
/// the checker to be silent — `support`'s own note says why. Here it must be,
/// because six of the checker's diagnostics on this file were the whole point.
#[test]
fn the_dispatch_example_checks_clean_and_lowers() {
    let lowered = dispatch();
    let codes: Vec<u16> = lowered.diagnostics.iter().map(|it| it.code.0).collect();
    assert!(codes.is_empty(), "08_dyn_dispatch no longer checks clean: {codes:?}");
    assert!(!lowered.bodies.is_empty());
}

/// §4a at a `return`: `into_summary`'s two arms are `Box.new(C(..))` against a
/// `-> Box of any Summarize`, and each is one coercion producing the object
/// type.
#[test]
fn a_boxed_unsizing_is_one_coerce_rvalue_carrying_the_object_type() {
    let lowered = dispatch();
    let found = coercions(&lowered, "into_summary");
    assert_eq!(
        found,
        vec![
            (Coercion::UnsizeInBox, "move", "Box of any Summarize".to_string()),
            (Coercion::UnsizeInBox, "move", "Box of any Summarize".to_string()),
        ],
    );
}

/// **It emits no code of its own**, which is the property §4a's cost paragraph
/// rests on: the allocation is the `Box.new` the author wrote, and the
/// conversion adds no second one.
///
/// `into_summary` has exactly two calls — the two `Box.new`s — and every
/// coercion in it is a statement beside them. A lowering that had decided to
/// allocate would show a third call here.
#[test]
fn a_boxed_unsizing_calls_nothing() {
    let lowered = dispatch();
    let calls = lowered
        .body("into_summary")
        .blocks()
        .filter(|(_, block)| matches!(block.terminator.kind, TerminatorKind::Call { .. }))
        .count();
    assert_eq!(calls, 2, "`into_summary` should call `Box.new` twice and nothing else");
}

/// §4a at an argument, which is the other site the corpus needs, beside the
/// borrow unsizings in the same body.
///
/// `main` writes `describe_boxed(Box.new(Doc(..)))` once and `describe_any(doc)`,
/// `describe_any(note)` and `clear(note)` around it, so this is the test that
/// shows the two rules coexisting in one body and staying apart — which is the
/// failure mode the `Coercion` enum's documentation names for folding them into
/// one variant.
#[test]
fn both_unsizings_appear_in_main_and_are_told_apart() {
    let lowered = dispatch();
    let found = coercions(&lowered, "main");
    let boxed = found.iter().filter(|(kind, _, _)| *kind == Coercion::UnsizeInBox).count();
    let borrowed = found.iter().filter(|(kind, _, _)| *kind == Coercion::Unsize).count();
    assert_eq!(boxed, 1, "expected `describe_boxed(Box.new(..))` alone: {found:?}");
    assert!(borrowed >= 2, "the borrow unsizings vanished: {found:?}");
    assert!(
        found
            .iter()
            .all(|(kind, _, _)| matches!(kind, Coercion::Unsize | Coercion::UnsizeInBox)),
        "an unexpected coercion in `main`: {found:?}",
    );
}

/// **The two differ in the type they produce and in nothing else this IR
/// records**, and stating it as an assertion is the honest version of a claim
/// about ownership that would otherwise be read off the wrong field.
///
/// In particular the operand of both is a `Move`, and that is *not* the
/// distinction §4a's cost paragraph draws. A borrow reaching an argument is
/// first materialised into a temporary by `lower`'s `argument`, and the
/// coercion then moves out of *that temporary*; the loan the author took is
/// still live behind it. So a consumer that tried to read ownership off the
/// operand mode would read the same answer for both, and the rvalue's `ty` — or
/// the variant — is the only place the answer is.
#[test]
fn the_two_unsizings_differ_only_in_the_type_they_produce() {
    let lowered = dispatch();
    for (kind, mode, ty) in coercions(&lowered, "main") {
        assert_eq!(mode, "move", "both unsizings move out of a temporary: {kind:?}");
        match kind {
            // Three shared and one exclusive: `clear(note)` is a `mutable
            // borrowed any Reset`, and §4 carries the mutability through
            // unchanged rather than weakening it.
            Coercion::Unsize => assert!(
                ty == "borrowed any Summarize" || ty == "mutable borrowed any Reset",
                "an unexpected borrow unsizing: {ty}",
            ),
            Coercion::UnsizeInBox => assert_eq!(ty, "Box of any Summarize"),
            other => panic!("unexpected coercion in `main`: {other:?}"),
        }
    }
}

/// Every unsizing in the corpus, by variant, so that a change to the rule shows
/// up as a number here rather than as a file nobody reopened.
#[test]
fn the_corpus_unsizes_in_exactly_these_places() {
    let mut borrowed = 0;
    let mut boxed = 0;
    for (_, source) in support::corpus() {
        let Some(lowered) = support::lower_if_clean(&source) else {
            continue;
        };
        for body in &lowered.bodies {
            for (_, block) in body.blocks() {
                for statement in &block.statements {
                    if let StatementKind::Assign { rvalue: Rvalue::Coerce { coercion, .. }, .. } =
                        &statement.kind
                    {
                        match coercion {
                            Coercion::Unsize => borrowed += 1,
                            Coercion::UnsizeInBox => boxed += 1,
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    // Six: `00_kitchen_sink`'s three `as_summary` arms, `08_dyn_dispatch`'s two
    // `into_summary` arms and its one `describe_boxed(Box.new(..))`. They are
    // the six diagnostics `science-types`' `tests/corpus.rs` used to pin.
    assert_eq!(boxed, 6, "the six sites §4a was admitted for");
    assert!(borrowed >= 6, "the borrow unsizings are still there: {borrowed}");
}
