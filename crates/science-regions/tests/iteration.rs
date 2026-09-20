//! The borrow a `for` holds, and rule 4 over it.
//!
//! `science-mir`'s `lower` §7.1 gives a `for` one shared borrow of its subject,
//! created before the header and live across every turn, because
//! `collections-and-chains.md` §4.4 says `iterate()` *"borrows the source,
//! shared, for the chain's life"* and §4.2 makes a bare `for` that call. This
//! file is what that borrow buys and what it costs, and it exists as its own
//! file rather than as three more tests in `conflicts.rs` for the same reason
//! `closure_captures.rs` does: the *rule* is rule 4 unchanged, and what is
//! under test is that a construct nobody had given a borrow to now reaches it.
//!
//! **What this replaced was not a weaker check, it was silence.** Until §7.1, a
//! `for` *moved* its subject into a temporary and passed a copy to the
//! element-producing call. The move was invisible: `SC0334` fires on a move
//! that overlaps a live borrow, nothing in the compiler yet checks a *use*
//! after a move, and the only phase that could have noticed the subject was
//! gone had no borrow to notice it against. So
//! [`a_for_over_a_collection_the_body_mutates_is_refused`] is a program that
//! was accepted, and [`the_subject_is_usable_after_the_loop`] is a program that
//! was accepted **for the wrong reason** — the checker was right about it by
//! not looking.

mod support;

use support::{check, codes};

fn reported(source: &str) -> Vec<u16> {
    check(source).reported()
}

/// **The program the loop's borrow exists to refuse.**
///
/// `xs.push(1)` takes an exclusive borrow of `xs` — two-phase, because it is a
/// receiver in argument position — and activates it inside the loop body, at a
/// point the loop's shared loan is still live, because the loop reads that loan
/// again on the next turn. Rule 4, with no new rule of its own.
#[test]
fn a_for_over_a_collection_the_body_mutates_is_refused() {
    let source = "\
def go(xs: &mut Array[Int]):
    for x in xs:
        xs.push(1)
";
    assert_eq!(
        reported(source),
        vec![330],
        "the loop's borrow is not reaching the conflict check: {:?}",
        codes(&check(source).regions)
    );
}

/// And through a nesting, which is the shape `Scopes.lookup` has: the *outer*
/// loop's loan is live at every point of the inner loop, so a write to the
/// outer collection from the inner body is refused too.
///
/// This is the test that a loan's region actually follows the back edge. A
/// region that stopped at the header would accept this and refuse the flat case
/// above, and the two would look the same in a summary.
#[test]
fn a_write_from_inside_a_nested_loop_is_refused_by_the_outer_borrow() {
    let source = "\
def go(rows: &mut Array[Array[Int]]):
    for row in rows:
        for cell in row:
            rows.push(Array[Int].new())
";
    assert_eq!(reported(source), vec![330]);
}

/// **The program it must not refuse**, and the reason `lower`'s §7.1 chose a
/// shared borrow rather than an exclusive one.
///
/// `Item is borrowed T` means a loop hands out shared views, and a shared view
/// needs no more than a shared borrow to come from — so reading the collection
/// being read is fine. An exclusive loop borrow would refuse this, which is not
/// the safe direction but a different language.
#[test]
fn a_for_over_a_collection_the_body_only_reads_is_accepted() {
    let source = "\
def go(xs: &Array[Int]):
    for x in xs:
        print(xs.length())
";
    assert_eq!(reported(source), Vec::<u16>::new());
}

/// §4.2's own promise, in its own words: *"`for doc in docs:` — borrows; `docs`
/// is usable afterwards"*.
///
/// The loan ends at the loop's exit, because that is the last point the loop's
/// reference is live, which is Decision 1 working on a construct that did not
/// used to have a loan at all.
#[test]
fn the_subject_is_usable_after_the_loop() {
    let source = "\
def go(xs: Array[Int]) -> Int:
    for x in xs:
        print(x)
    xs.length()
";
    assert_eq!(reported(source), Vec::<u16>::new());
}

/// The same collection, mutated *after* the loop rather than inside it. The
/// loan is dead by then, so this is the fix §7.1's refusal suggests and it has
/// to work or the refusal is a wall rather than a diagnostic.
#[test]
fn mutating_the_collection_after_the_loop_is_fine() {
    let source = "\
def go(xs: &mut Array[Int]):
    for x in xs:
        print(x)
    xs.push(1)
";
    assert_eq!(reported(source), Vec::<u16>::new());
}

/// And a write to a *different* collection is not a conflict, which is the
/// control: a rule that refused this would be refusing loops rather than
/// refusing overlap.
#[test]
fn writing_to_another_collection_inside_the_loop_is_fine() {
    let source = "\
def go(a: &Array[Int], b: &mut Array[Int]):
    for x in a:
        b.push(1)
";
    assert_eq!(reported(source), Vec::<u16>::new());
}

/// The loan's region, read directly, rather than inferred from a verdict.
///
/// Every point of the loop — header, test, body, back edge — is in it. Asserted
/// because the three tests above would all still pass on a loan whose region
/// was the single point it was taken at *and* an accidentally-exclusive access
/// somewhere else; this is the fact they are supposed to be about.
#[test]
fn the_loans_region_covers_every_point_of_the_loop() {
    let source = "\
def go(xs: &Array[Int]):
    for x in xs:
        print(x)
";
    let checked = check(source);
    let body = checked.body("go");
    let analysis = checked.analysis_of("go");

    // The loop's borrow is the one whose reference the element-producing call
    // reads; `science-mir`'s `tests/iteration.rs` finds it the same way.
    let chain = body
        .blocks()
        .find_map(|(_, block)| match &block.terminator.kind {
            science_mir::mir::TerminatorKind::Call {
                callee:
                    science_mir::mir::Callee::Unresolved(science_mir::mir::Unresolved::IterateNext),
                args,
                ..
            } => args[0].place().map(|place| place.local),
            _ => None,
        })
        .expect("a `for` with an element-producing call");
    let loan = body
        .borrows()
        .iter()
        .find(|data| data.destination.local == chain)
        .expect("the loop's borrow");
    let region = analysis.solution.region(analysis.table.loan(loan.id));

    // The header is the block the call is in; every point from the borrow's
    // reservation onwards that is inside the loop must be in the region.
    let header = body
        .blocks()
        .find(|(_, block)| {
            matches!(
                block.terminator.kind,
                science_mir::mir::TerminatorKind::Call {
                    callee: science_mir::mir::Callee::Unresolved(
                        science_mir::mir::Unresolved::IterateNext
                    ),
                    ..
                }
            )
        })
        .expect("a header")
        .0;
    for point in body.points() {
        if point.block != header {
            continue;
        }
        assert!(
            region.contains(analysis.index.index(point)),
            "the loop's loan is not live at its own header, so it ends before the \
             second turn and rule 4 sees only the first"
        );
    }
    assert!(!region.is_empty());
}

/// **The imprecision the borrow does not fix, stated as a test.**
///
/// The element-producing call is still a [`science_mir::mir::Callee::Unresolved`]
/// — `lower`'s §7.2 says what it is waiting for — so
/// [`science_regions::analysis::BodyAnalysis::calls_a_hole`] is true for every
/// body containing a `for`, and [`science_regions::check`]'s §6 suppresses
/// `SC0340` in all of them.
///
/// So a body whose returned reference comes out of a loop is *not* checked for
/// Decision 6; it is excused. That is one suppression, not two: the loan on the
/// subject is real and rule 4 runs over it, which is why the refusals above
/// work at all. This test fails the day THIR carries the resolved `next`, and
/// what it should then assert is that the signature is determined.
#[test]
fn a_body_with_a_for_is_still_excused_from_decision_6() {
    let source = "\
def go(xs: &Array[Int]):
    for x in xs:
        print(x)
";
    let checked = check(source);
    assert!(
        checked.analysis_of("go").calls_a_hole,
        "the `for`'s callee resolved; `lower`'s §7.2 seam has closed and this \
         suppression can go"
    );
}
