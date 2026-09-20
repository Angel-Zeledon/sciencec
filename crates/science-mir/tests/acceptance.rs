//! `examples/21_compiler_shapes.science`, lowered.
//!
//! That file says of itself: *"The day regions land, this file is the first
//! thing they are run against."* Regions have not landed, so what can be
//! asserted is one step short of what it exists for — **that the MIR it lowers
//! to has the six things `region-inference.md` §10 asks for**, on the program
//! the note chose as its acceptance case.
//!
//! Each test below therefore names the §10 item it stands for. Where the answer
//! is *"not on this program"*, the test says so rather than being omitted,
//! because a missing assertion and a negative one look the same in a passing
//! run and only one of them is a finding.

mod support;

use science_mir::mir::{BorrowKind, Callee, TerminatorKind, Unresolved};
use support::lower;

fn acceptance() -> support::Lowered {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/21_compiler_shapes.science");
    let source = std::fs::read_to_string(path).expect("the acceptance example");
    lower(&source)
}

#[test]
fn it_lowers_at_all() {
    let lowered = acceptance();
    // Eighteen functions with bodies: the methods of `DefTable`, `Scopes`,
    // `Parser`, `Node` and `Diagnostics`, plus `walk` and `main`.
    assert_eq!(lowered.bodies.len(), 18, "the body count changed");
    for body in &lowered.bodies {
        assert!(body.block_count() >= 1);
        assert!(body.check_predecessors());
        assert!(body.index_temps_are_single_assignment());
    }
}

/// §10 item 1.
#[test]
fn every_body_has_a_cfg_with_points() {
    let lowered = acceptance();
    let points: usize = lowered.bodies.iter().map(|body| body.point_count()).sum();
    assert!(points > 100, "{points} points is too few for this program");
    for body in &lowered.bodies {
        assert_eq!(body.points().count(), body.point_count());
    }
}

/// §10 item 2, on the two structures the example's §3 and §4 exist for.
#[test]
fn a_borrowed_field_is_reached_through_a_dereference() {
    let lowered = acceptance();
    let peek = lowered.dump("peek");
    assert!(peek.contains("(*_1).tokens"), "`self.tokens` did not deref: {peek}");
}

/// §10 item 3.
#[test]
fn every_binding_has_a_storage_dead_point() {
    let lowered = acceptance();
    for body in &lowered.bodies {
        for (local, decl) in body.locals() {
            if matches!(decl.kind, science_mir::LocalKind::Binding(_)) {
                assert!(
                    !body.storage_dead_points(local).is_empty(),
                    "a binding in `{}` has no storage-dead point, so rule 5 has \
                     nothing to compare against",
                    lowered.krate.defs.get(body.def()).name
                );
            }
        }
    }
}

/// §10 item 4, and the finding: this program *does* exercise two-phase borrows,
/// through `defs.alloc(..)` and `scopes.push(..)`, whose receivers are
/// `mutable self`.
#[test]
fn the_mutable_self_receivers_are_two_phase() {
    let lowered = acceptance();
    let two_phase: usize = lowered
        .bodies
        .iter()
        .flat_map(|body| body.borrows())
        .filter(|data| data.kind == BorrowKind::TwoPhase)
        .count();
    assert!(two_phase >= 5, "only {two_phase} two-phase borrows");
    for body in &lowered.bodies {
        for data in body.borrows() {
            if data.kind == BorrowKind::TwoPhase {
                assert!(
                    data.activation.is_some(),
                    "a two-phase borrow in `{}` was never activated",
                    lowered.krate.defs.get(body.def()).name
                );
            }
        }
    }
}

/// §10 item 5, and the second finding: **this program has no conditional move,
/// so it generates no drop flags at all.**
///
/// That is Decision 26 working as §12 says it should — *"the difference between
/// a rare cost and a tax on every function"* — and it is also a gap in the
/// evidence: the acceptance case does not exercise the mechanism. `tests/drops.rs`
/// does, on a fixture written for it.
#[test]
fn the_acceptance_example_needs_no_drop_flags() {
    let lowered = acceptance();
    let flags: usize = lowered.bodies.iter().map(|body| body.drop_flags().len()).sum();
    assert_eq!(flags, 0, "the example acquired {flags} drop flags");
    let drops: usize = lowered
        .bodies
        .iter()
        .flat_map(|body| body.blocks())
        .filter(|(_, block)| matches!(block.terminator.kind, TerminatorKind::Drop { .. }))
        .count();
    assert!(drops > 0, "no drop at all, which would mean the elaboration deleted them wrongly");
}

/// §10 item 6, on the real program.
#[test]
fn lowering_it_twice_gives_the_same_numbering() {
    let first = acceptance();
    let second = acceptance();
    for name in ["main", "lookup", "walk", "alloc"] {
        assert_eq!(first.dump(name), second.dump(name), "`{name}` renumbered between runs");
    }
}

/// Decision 8's partition, on the program Decision 8's cost is about.
#[test]
fn the_call_graph_has_no_cycle_in_this_program() {
    let lowered = acceptance();
    let graph = lowered.call_graph();
    let components = graph.components();
    assert_eq!(
        components.len(),
        graph.functions().count(),
        "a cycle appeared where the example has none"
    );
}

/// **The third and largest finding — and it has halved.**
///
/// It read: every method the example calls on `Array` — `new`, `len`, `get`,
/// `push`, `pop` — is a method on a type with no declaration, so the callee is
/// a hole, and region inference over this program would be reasoning about
/// calls whose signatures it does not have. Sixteen of them. *"The number is
/// asserted so that it goes down visibly when it does."*
///
/// It went down, to eight, because the prelude gained declarations for `Array`
/// and `Map`, and then to seven. What remains is the `Map`/`Array` methods the
/// transcription deliberately stopped short of, plus the three `for` loops.
///
/// **The three `for` loops are no longer waiting on a language decision.** This
/// comment used to say they were blocked on *"whether `Array of T`'s `Item` is
/// `T` or `borrowed T`"*. `collections-and-chains.md` §4.1 and §4.3 had already
/// answered that, the prelude now says `type Item is borrowed T`, and
/// `science-types`'s `check`'s `iterate_item` resolves `Iterate.next` against
/// it — which is where a loop's binding type comes from today. It then keeps
/// the type and discards the candidate, so `thir::ExprKind::For` carries no
/// callee for `science-mir`'s `lower` to use. **The blocker is a THIR field,
/// and `lower`'s §7.2 names it**: `next: Option<DefId>`.
///
/// What did change is the *other* half of a `for`, which was never a hole and
/// was wrong: `lower`'s §7.1 gives each loop a shared borrow of its subject
/// where it used to move it, so this file's three loops now reach region
/// inference as three loans instead of three unchecked moves. The count below
/// is unmoved by that, deliberately — a borrow is not a callee.
///
/// The `Method` bound is kept as a bound rather than pinned exactly, for the
/// reason it was written: it is here to go down. `IterateNext` is pinned at
/// three because it is one per `for` and the file writes three.
#[test]
fn the_calls_it_cannot_resolve_are_the_container_methods() {
    let lowered = acceptance();
    let unresolved: Vec<Unresolved> = lowered
        .bodies
        .iter()
        .flat_map(|body| body.blocks())
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Unresolved(which), .. } => Some(*which),
            _ => None,
        })
        .collect();
    let methods = unresolved.iter().filter(|which| **which == Unresolved::Method).count();
    let iterate = unresolved.iter().filter(|which| **which == Unresolved::IterateNext).count();
    assert!(
        (1..=8).contains(&methods),
        "expected at most eight unresolved method calls, found {methods}; {unresolved:?}"
    );
    // **Zero, and the sentence that pinned three is what changed.** It read
    // *"`IterateNext` is pinned at three because it is one per `for` and the
    // file writes three"*, which was true while every `for` reached MIR as a
    // call to a `next` nothing could name. Two things have happened since. A
    // `for` over a **`Chars`** resolves its `next`, because `Chars` declares
    // one and `science_chars_next` implements it. A `for` over an **`Array`**
    // calls no `next` at all: `collections-and-chains.md` §4.2's AMENDMENT 11
    // makes it `xs.iterate()`, §8 names no type for `iterate()` to return, and
    // `lower_for_over_array` emits the indexed loop that desugaring compiles
    // to. This file's three loops are all over arrays.
    //
    // **What the row was for survives.** `Unresolved::IterateNext` is still
    // reachable — a `for` over a subject that implements `Iterate` and
    // declares no `next` of its own still produces one — and this file simply
    // writes no such loop. The count is pinned rather than dropped so that a
    // regression putting one back is visible.
    assert_eq!(
        iterate, 0,
        "every `for` here is over an `Array`, which lowers to an indexed loop and calls no \
         `next`; {unresolved:?}"
    );
}

/// And the fourth: the example's own §5 says the bump arena and the interner
/// are absent, and its §4 says `Node of T` is the case the claim turns on.
/// `Node.new` lowers, and what it lowers to is two shared borrows stored into
/// one record — which is Decision 3's *"one region variable per borrowed
/// field"* with nothing here to relate them. Relating them is region
/// inference's, and this asserts only that both borrows survive to it.
#[test]
fn node_new_stores_two_borrows_into_one_record() {
    let lowered = acceptance();
    let body = lowered.body("new");
    let _ = body;
    // There are three `new`s; the one with two parameters is `Node`'s.
    let node_new = lowered
        .bodies
        .iter()
        .filter(|body| lowered.krate.defs.get(body.def()).name == "new")
        .find(|body| body.params().count() == 2)
        .expect("`Node.new` has two parameters");
    let records = node_new
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|statement| {
            matches!(
                &statement.kind,
                science_mir::StatementKind::Assign { rvalue: science_mir::Rvalue::Record { .. }, .. }
            )
        })
        .count();
    assert_eq!(records, 1, "`Node.new` did not build one record");
    assert_eq!(
        node_new.params().count(),
        2,
        "both &fields must arrive as separate parameters, or Decision 3 \
         has nothing to infer a relation between"
    );
}
