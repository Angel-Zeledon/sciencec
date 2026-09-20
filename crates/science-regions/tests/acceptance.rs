//! `examples/21_compiler_shapes.science`, region-checked.
//!
//! That file says of itself: *"The day regions land, this file is the first
//! thing they are run against."* This is that run.
//!
//! Each test names the section of the example or of `region-inference.md` it
//! stands for. Where the answer is *"the example does not exercise this"*, the
//! test says so rather than being omitted, because a missing assertion and a
//! negative one look the same in a passing run and only one of them is a
//! finding.

mod support;

use support::{acceptance, codes};

/// §14's *"cheapest early signal"*: **if it needs `SC0340` anywhere, the bet is
/// in trouble.**
#[test]
fn the_acceptance_case_needs_no_diagnostic_at_all() {
    let checked = acceptance();
    assert_eq!(
        checked.reported(),
        Vec::<u16>::new(),
        "region inference reported: {:?}",
        codes(&checked.regions)
    );
}

#[test]
fn every_body_is_analysed() {
    let checked = acceptance();
    assert_eq!(checked.bodies.len(), 18, "the body count changed");
    assert_eq!(checked.analysis.len(), 18, "a body was lowered and not analysed");
}

/// §4 of the example — `Node of T`, *"the case the whole claim turns on"*.
///
/// Two borrowed fields, two region variables, and **no constraint between
/// them**. Decision 3 in one assertion: the answer is two regions and nobody
/// was asked.
#[test]
fn node_gets_two_region_variables_and_relates_neither_to_the_other() {
    let checked = acceptance();
    let body = checked.body_with("new", 2);
    let analysis = checked.analysis.body(body.def()).expect("analysed");

    let returns = &analysis.summary.returns;
    assert_eq!(returns.len(), 2, "`Node[T]` did not get one region per &field");

    // Each return position is tied to exactly one parameter, and they are
    // different parameters.
    let sources: Vec<Vec<usize>> = returns
        .iter()
        .map(|(_, from)| from.iter().map(|it| it.param).collect())
        .collect();
    assert_eq!(sources, vec![vec![0], vec![1]], "the two fields were not kept apart: {sources:?}");
}

/// §4.3's *"the relation between them is inferred at every use site rather than
/// declared once at the type"*, from the other side: nothing in `Node.new`'s
/// constraint set relates the two field regions to each other.
#[test]
fn nothing_unifies_the_two_borrowed_fields() {
    let checked = acceptance();
    let body = checked.body_with("new", 2);
    let analysis = checked.analysis.body(body.def()).expect("analysed");
    let returns: Vec<_> =
        analysis.table.local(science_mir::mir::RETURN_PLACE).iter().map(|(_, v)| *v).collect();
    assert_eq!(returns.len(), 2);
    for constraint in analysis.constraints.all() {
        let relates_the_two = returns.contains(&constraint.sup) && returns.contains(&constraint.sub);
        assert!(!relates_the_two, "a constraint relates `Node`'s two field regions: {constraint:?}");
    }
}

/// §3 of the example — `Parser`, *"one borrowed field, one source, one
/// region"*. `Parser.new` ties the field to its only parameter, which is the
/// signature Rust spells `Parser<'t>`.
#[test]
fn parser_new_ties_its_one_field_to_its_one_parameter() {
    let checked = acceptance();
    let body = checked.body_with("new", 1);
    let analysis = checked.analysis.body(body.def()).expect("analysed");
    assert_eq!(analysis.summary.returns.len(), 1, "`Parser` did not get exactly one region");
    let (_, from) = &analysis.summary.returns[0];
    assert_eq!(from.len(), 1, "the field is tied to {} sources, not one", from.len());
    assert_eq!(from[0].param, 0);
}

/// §1 of the example — `DefTable.get`, *"a shared borrow out of the table. One
/// region: the result borrows `self` and nothing else"*.
///
/// **And the finding.** The only call in that body is an unresolved `Array`
/// method, so the answer comes entirely from `generate`'s §5: an opaque callee
/// may hand back a reference into anything reachable from its arguments,
/// *including through the reference the argument was projected out of*. Without
/// that rule this signature is `SC0340` and the acceptance case fails.
#[test]
fn deftable_get_borrows_from_self_through_a_call_with_no_callee() {
    let checked = acceptance();
    let analysis = checked.analysis_of("get");
    assert_eq!(analysis.summary.returns.len(), 1, "`get` returns one borrow");
    let (_, from) = &analysis.summary.returns[0];
    assert!(!from.is_empty(), "`get`'s result is tied to nothing, which is SC0340");
    assert!(from.iter().all(|it| it.param == 0), "`get` borrows from something other than `self`");
}

/// The same shape one level deeper: `Parser.peek` reads through `self.tokens`.
#[test]
fn parser_peek_borrows_from_self() {
    let checked = acceptance();
    let analysis = checked.analysis_of("peek");
    assert_eq!(analysis.summary.returns.len(), 1);
    let (_, from) = &analysis.summary.returns[0];
    assert!(!from.is_empty(), "`peek`'s result is tied to nothing");
    assert!(from.iter().all(|it| it.param == 0));
}

/// §2 of the example — *"`lookup` returns `DefId?`, not `borrowed Binding?`
/// … the single most load-bearing signature decision in the file"*.
///
/// The decision was made on design grounds. This asserts what it bought: an
/// index has no region, so the signature has nothing to determine and Decision
/// 6 has nothing to ask.
#[test]
fn an_index_returning_signature_has_no_region_at_all() {
    let checked = acceptance();
    for name in ["lookup", "alloc", "parent_of"] {
        let analysis = checked.analysis_of(name);
        assert!(
            analysis.summary.returns.is_empty(),
            "`{name}` returns something with a region, which the example says it does not"
        );
    }
}

/// §10 item 4, from this side of the seam: the two-phase borrows that
/// `science-mir` created are checked, and their reservation windows are not
/// empty — which is what `v.push(v.len())` needs and what that crate's §7 item
/// 3 warns can go *inert* in silence.
#[test]
fn the_two_phase_borrows_have_a_reservation_window() {
    let checked = acceptance();
    let mut two_phase = 0;
    for body in &checked.bodies {
        let analysis = checked.analysis.body(body.def()).expect("analysed");
        for data in body.borrows() {
            if data.kind != science_mir::mir::BorrowKind::TwoPhase {
                continue;
            }
            two_phase += 1;
            let activation = data.activation.expect("a two-phase borrow is activated");
            let reserved = analysis.index.index(data.reserved);
            assert!(
                analysis.is_reserved_at(data.id, reserved),
                "a two-phase borrow is not reserved at its own reservation point"
            );
            assert!(
                !analysis.is_reserved_at(data.id, analysis.index.index(activation)),
                "a two-phase borrow is still reserved at its activation"
            );
        }
    }
    assert!(two_phase >= 5, "only {two_phase} two-phase borrows; the lowering regressed");
}

/// §6.2's public query, on the real program. A borrow is live somewhere, and
/// live nowhere after everything that holds it is gone.
#[test]
fn borrows_live_at_answers_on_the_real_program() {
    let checked = acceptance();
    let body = checked.body("main");
    let analysis = checked.analysis.body(body.def()).expect("analysed");
    let anywhere: usize =
        body.points().map(|point| analysis.borrows_live_at(point).len()).sum();
    assert!(anywhere > 0, "no borrow is live anywhere in `main`, so nothing was solved");

    let last = body
        .points()
        .filter(|point| {
            matches!(
                body.block(point.block).terminator.kind,
                science_mir::mir::TerminatorKind::Return
            ) && body.is_terminator(*point)
        })
        .last()
        .expect("`main` returns");
    assert!(
        analysis.borrows_live_at(last).is_empty(),
        "a borrow is still live at `main`'s return"
    );
}

/// Decision 8's partition, on the program Decision 8's cost is about. The
/// example has no cycle, so no component needed a second pass — and §14's
/// *"the language's flagship self-hosted program is the worst case for its own
/// incremental recompilation"* is **not** demonstrated by this file.
#[test]
fn no_component_of_the_acceptance_case_needs_a_fixpoint() {
    let checked = acceptance();
    assert!(
        checked.analysis.fixpoints().is_empty(),
        "a recursive component appeared: {:?}",
        checked.analysis.fixpoints()
    );
}

/// §5 of `region-inference.md`'s §4.3 cost 2, and `lib.rs`'s §5: the
/// intersection is never empty, so `SC0335` never fires. Asserted on the one
/// program in the corpus with a two-borrowed-field type.
#[test]
fn the_intersection_of_nodes_two_regions_is_not_empty() {
    let checked = acceptance();
    let body = checked.body_with("new", 2);
    let analysis = checked.analysis.body(body.def()).expect("analysed");
    let positions: Vec<_> =
        analysis.table.local(science_mir::mir::RETURN_PLACE).iter().map(|(_, v)| *v).collect();
    let mut common = analysis.solution.region(positions[0]).clone();
    common.intersect_with(analysis.solution.region(positions[1]));
    assert!(!common.is_empty(), "SC0335's condition was reached, and lib.rs's §5 is wrong");
}

/// The solver converges, and §3's *"linear in practice"* is a number.
///
/// **The number moved from three to seven, and the body it moved on is
/// `Scopes.lookup`.** `science-mir`'s `lower` §7.1 gave a `for` a shared borrow
/// of its subject, and `lookup` is two nested `for`s over a field of a
/// reference, so the constraint graph acquired a chain: `self`'s region bounds
/// the outer loan, the outer loan bounds the element `rib`, `rib` bounds the
/// inner loan taken through it, the inner loan bounds `binding`, and
/// `binding.definition` is returned. [`science_regions::solve`]'s §2 sweeps the
/// constraint list in the order it was generated and a sub-region must grow
/// before its super-region can, so a chain of *n* dependent constraints costs
/// up to *n* sweeps whenever the list order runs against it.
///
/// **So the factor this asserts is the depth of the deepest borrow chain, not
/// the size of the body** — `main` is 137 points and converges in two. The
/// bound is eight because seven is the worst in all of `examples/`, and it is
/// kept as a bound for the reason `solve`'s §4 states: *"linear in practice"*
/// is worth having as a number a test can fail on rather than as a claim. A
/// solver that applied constraints in dependency order would flatten it, and
/// that is a change to [`science_regions::solve`] and not to this bound.
#[test]
fn the_solver_converges_quickly_on_the_whole_file() {
    let checked = acceptance();
    let worst = checked
        .bodies
        .iter()
        .map(|body| checked.analysis.body(body.def()).expect("analysed").solution.iterations())
        .max()
        .expect("bodies");
    // **The bound moved from 8 to 9, and the sweep it bought is accounted for
    // rather than absorbed.** `for c in text.chars():` now calls a real
    // `Chars.next`, which is `def next(mutable self)`, so the loop takes an
    // **exclusive** borrow of the chars temporary where it used to take a
    // shared one. An exclusive loan constrains more than a shared one does, and
    // `examples/10_loops.science` has the file's deepest nesting, so it is the
    // body that pays.
    //
    // The bound is still tight on purpose. This number is a canary for the
    // solver's *shape* — the comment above says a solver applying constraints
    // in dependency order would flatten it — not a budget, so it is raised by
    // exactly the sweep the change costs and no more. A jump to 20 would stop
    // being able to fail.
    assert!(worst <= 9, "the fixpoint took {worst} sweeps on a body with no cycle worth that");
}

/// Two runs agree. §10 item 6, one level on: a summary that moved between runs
/// could not be the thing Decision 7 serialises.
#[test]
fn analysing_it_twice_gives_the_same_answer() {
    let first = acceptance();
    let second = acceptance();
    for name in ["get", "peek", "walk", "main", "alloc"] {
        assert_eq!(first.dump(name), second.dump(name), "`{name}` moved between runs");
    }
}
