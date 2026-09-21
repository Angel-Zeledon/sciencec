//! `lower`'s §8.5 follow-up: a closure with nothing captured gets a `Body` of
//! its own — the half `lib.rs` §7 item 5 used to call *"the largest hole in
//! the crate"* and `captures.rs`'s own header calls "the body is not".
//!
//! `captures.rs` is the capture discipline; this file is what the body itself
//! looks like once it is lowered, and what still refuses to be.

mod support;

use science_mir::mir::{Rvalue, StatementKind, TerminatorKind};
use support::lower;

/// The corpus form this whole effort was scoped around:
/// `def apply(f: (Int) -> Int) -> Int: f(1)`, called with `x giving x + 1`.
/// Neither `main` nor `apply` name the closure's body — it is reached only by
/// looking it up under its own `param`, which is what a caller two crates
/// down (`science_codegen::mono`) is expected to do too.
#[test]
fn a_capture_free_closures_body_is_its_own_lowered_body() {
    let source = "\
def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    print(apply(x giving x + 1))
";
    let lowered = lower(source);
    let closure = lowered.body("x");

    assert_eq!(closure.params().count(), 1, "a closure of one parameter takes one argument");

    let mut computed = false;
    for (_, block) in closure.blocks() {
        for statement in &block.statements {
            if let StatementKind::Assign { rvalue: Rvalue::Binary { .. }, .. } = &statement.kind {
                computed = true;
            }
        }
    }
    assert!(computed, "`x + 1` must be a statement in the closure's own body, not `apply`'s");

    // `apply`'s own body is unaffected: it still just calls `f`, indirectly.
    // §8.5's hole in *that* call — `Callee::Indirect` — is not this file's;
    // `science-codegen-llvm`'s `tests/closures.rs` is where that is exercised.
    let mut calls = 0;
    for (_, block) in lowered.body("apply").blocks() {
        if matches!(block.terminator.kind, TerminatorKind::Call { .. }) {
            calls += 1;
        }
    }
    assert_eq!(calls, 1, "`apply` calls `f` exactly once");
}

/// A closure that captures something is left exactly where §8.5 found it: no
/// second `Body`, because there is nowhere to put a captured value in the
/// `{ fn ptr, captures }` aggregate this crate cannot yet build.
#[test]
fn a_closure_that_captures_something_still_has_no_body_of_its_own() {
    let source = "\
def sink(f: (Int) -> Int) -> Int:
    1

def go(n: Int) -> Int:
    sink(item giving item + n)
";
    let lowered = lower(source);
    // The closure's own parameter is named `item`; no `Body` anywhere is
    // filed under it, because `n` crossing the boundary is a capture and
    // captures are still the hole.
    assert!(
        lowered.bodies.iter().all(|body| lowered.krate.defs.get(body.def()).name != "item"),
        "a capturing closure must not be given a body"
    );
}

