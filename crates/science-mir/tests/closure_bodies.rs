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

/// A closure that captures something gets a `Body` too, and the capture
/// reaches it as a trailing parameter.
///
/// **This test asserted the opposite until §8.6, and the replacement is not a
/// weakening.** It was `a_closure_that_captures_something_still_has_no_body_
/// of_its_own`, over this same fixture, and its body was:
///
/// ```ignore
/// assert!(
///     lowered.bodies.iter().all(|body| lowered.krate.defs.get(body.def()).name != "item"),
///     "a capturing closure must not be given a body"
/// );
/// ```
///
/// Its stated reason was *"there is nowhere to put a captured value in the
/// `{ fn ptr, captures }` aggregate this crate cannot yet build"*. That
/// reason was about the closure's **type** — a bare arrow `(A) -> B` with no
/// room to say how many captures or of what — and §8.6's decision is that the
/// type never needed to say. Every capture is a *borrow* by §8's own
/// discipline, so the environment is `N` pointers, a shape derivable from the
/// count alone; the body takes one trailing `borrowed T` parameter per
/// capture in `capture::captures_of`'s first-mention order, and
/// `science-codegen-llvm`'s `lower_closure` lays the environment out from
/// `captures.len()` without consulting the type at all.
///
/// So the old assertion was pinning a limitation, not an invariant, and the
/// limitation is gone. What is pinned here instead is the *shape* that
/// replaced it — which is a stronger claim than "no body exists", because a
/// body with the wrong parameter count is exactly the silent miscompile the
/// old refusal was protecting against.
#[test]
fn a_closure_that_captures_something_has_a_body_with_the_capture_as_a_parameter() {
    let source = "\
def sink(f: (Int) -> Int) -> Int:
    1

def go(n: Int) -> Int:
    sink(item giving item + n)
";
    let lowered = lower(source);
    let closure = lowered.body("item");

    // One declared parameter, plus one trailing capture parameter for `n`.
    assert_eq!(
        closure.params().count(),
        2,
        "`item giving item + n` takes its own parameter and a capture of `n`"
    );
    assert_eq!(
        closure.closure_captures(),
        Some(1),
        "exactly one of those parameters is a capture"
    );

    // And the addition is in the closure's own body, not in `go`'s.
    let mut computed = false;
    for (_, block) in closure.blocks() {
        for statement in &block.statements {
            if let StatementKind::Assign { rvalue: Rvalue::Binary { .. }, .. } = &statement.kind {
                computed = true;
            }
        }
    }
    assert!(computed, "`item + n` must be a statement in the closure's own body");
}

/// A closure whose body *moves* a capture out still has no `Body`, and this is
/// the half of the old test's sentence that survives §8.6 — for a different
/// and much sharper reason than the one it was written with.
///
/// §8.2 models a consuming capture as an **exclusive borrow**, because a
/// borrow discipline has no spelling for a move. While no body was lowered at
/// all, that mismatch was an unreported mistake in a program that could not
/// run either way. A body lowered against the same model *would* run it: the
/// owning `String` copied out of storage the enclosing frame still drops,
/// which is a double free rather than a missing diagnostic. Withholding the
/// body is therefore strictly the safer of the two, and it keeps the refusal
/// in the one place — `science-codegen-llvm` — that can name the construct.
///
/// The right repair is above this crate: an `SC0301` from a checker that can
/// see the move. Until then this test is what stops §8.6 from being extended
/// to the case it must not cover.
#[test]
fn a_closure_that_moves_a_capture_out_still_has_no_body_of_its_own() {
    let source = "\
def take(s: String) -> Int:
    s.length()

def sink(f: (Int) -> Int) -> Int:
    1

def go(name: String) -> Int:
    sink(item giving item + take(name))
";
    let lowered = lower(source);
    assert!(
        lowered.bodies.iter().all(|body| lowered.krate.defs.get(body.def()).name != "item"),
        "a closure that moves a capture out must not be given a body"
    );
}

