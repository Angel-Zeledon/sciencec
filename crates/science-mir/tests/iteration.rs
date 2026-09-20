//! `lower`'s §7: what a `for` borrows, and what it still cannot name.
//!
//! The section has two halves with two different owners and this file keeps
//! them apart. §7.1's borrow is this crate's, and every test about it is an
//! assertion; §7.2's callee is `science-types`', and the two tests about it
//! assert the *hole* — that it is named, and that the shape around it is whole
//! — so that the day THIR carries the resolved `next` they fail and say what to
//! change.
//!
//! Until this file existed, a `for` was the one construct in the language whose
//! ownership behaviour nothing tested. It *moved* its subject, which is
//! `collections-and-chains.md` §4.4's `iterate_consuming()` row in the position
//! its §4.2 gives to `iterate()`'s, and no test anywhere said either thing.

mod support;

use science_mir::mir::{BorrowKind, Callee, Operand, Projection, TerminatorKind, Unresolved};
use support::lower;

/// The borrow a `for` took, found the way a consumer would have to find it:
/// through the element-producing call, whose one argument is the local the
/// loop's reference was stored into.
///
/// Written as a lookup rather than *"the first shared borrow"* because a
/// subject like `text.chars()` takes a shared receiver borrow of its own, and a
/// test that picked the first shared borrow would assert about that instead.
/// Whether a callee is the `Iterate.next` a `for` header calls.
///
/// **Two spellings, because both are reachable.** A subject whose implementor
/// declares a `next` resolves to a [`Callee::Def`] naming it — `Chars` is the
/// one that does — and a subject that implements `Iterate` without declaring
/// one is still [`Unresolved::IterateNext`]. A `for` over an `Array` produces
/// neither: it calls no `next` at all.
fn is_next(lowered: &support::Lowered, callee: &Callee) -> bool {
    match callee {
        Callee::Unresolved(Unresolved::IterateNext) => true,
        Callee::Def(def) => lowered.krate.defs.get(*def).name == "next",
        _ => false,
    }
}

fn loop_borrows<'a>(
    lowered: &'a support::Lowered,
    body: &'a science_mir::mir::Body,
) -> Vec<&'a science_mir::mir::BorrowData> {
    let mut out = Vec::new();
    for (_, block) in body.blocks() {
        let TerminatorKind::Call { callee, args, .. } = &block.terminator.kind else { continue };
        // **Two spellings now, because a `for`'s `next` resolves when the
        // implementor declares one.** `Chars` does — its `next` is
        // `science_chars_next` — so that loop's header is a `Callee::Def`;
        // every other `implements Iterate` block declares `methods: &[]`, so
        // its `next` is the interface's and is still unresolved. Matching only
        // the second is what this helper used to do, and it made every test
        // that used it report *zero* borrows rather than a wrong one.
        if !is_next(lowered, callee) {
            continue;
        }
        let place = args[0].place().expect("the chain is a place");
        out.push(
            body.borrows()
                .iter()
                .find(|data| data.destination.local == place.local)
                .expect("the chain is a reference the loop took"),
        );
    }
    out
}

/// The subject that still goes through `Iterate.next`.
///
/// **This used to be an `Array` and had to stop being one.** Every test below
/// that uses it is about §7.1's loop borrow *and the call it feeds*, and a
/// `for` over an `Array` no longer makes that call: `collections-and-chains.md`
/// §4.2's AMENDMENT 11 makes it `xs.iterate()`, §8 names no type for
/// `iterate()` to return, and `lower_for_over_array` emits the indexed loop
/// that desugaring compiles to. `Chars` is the one subject with a real `next`,
/// so it is the one that can still be asked these questions.
///
/// The array's own shape is pinned by
/// [`a_for_over_an_array_is_an_indexed_loop`] instead, which is a different
/// question and now has its own test rather than sharing one.
const OVER_CHARS: &str =
    "def f(text: &String):\n    for c in text.chars():\n        print(c)\n";

/// The whole of §4.2's AMENDMENT 11 — *"`for x in xs:` desugars to
/// `xs.iterate()` — it borrows"* — as one assertion.
///
/// The old lowering emitted `_2 = move _1`. A `Move` of the subject anywhere in
/// the body is that behaviour back, and the assertion is written against the
/// *operand* rather than against the borrow so that adding a borrow beside the
/// move would not pass it.
#[test]
fn a_for_borrows_its_subject_rather_than_moving_it() {
    let lowered = lower("def f(xs: Array[Int]):\n    for x in xs:\n        print(x)\n");
    let body = lowered.body("f");
    let subject = body.params().next().expect("`xs` is the parameter");

    for (_, block) in body.blocks() {
        for statement in &block.statements {
            let science_mir::StatementKind::Assign { rvalue, .. } = &statement.kind else {
                continue;
            };
            if let science_mir::Rvalue::Use(Operand::Move(place)) = rvalue {
                assert_ne!(
                    place.local, subject,
                    "the loop moved its subject, which is `iterate_consuming()`'s row \
                     of §4.4 in `iterate()`'s position"
                );
            }
        }
    }

    let borrows: Vec<_> =
        body.borrows().iter().filter(|data| data.place.local == subject).collect();
    // **One, shared, and spanning the loop**, which is what this test is
    // about — neither a move nor a loan that dies before the body.
    // `lower_for_over_array` takes §7.1's borrow once, reads `length()`
    // through a copy of it, and projects each element through it; a second
    // borrow of the subject would mean the loan was being retaken per turn,
    // which is the shape that let `xs.push(1)` slip through before
    // `science-regions` caught it.
    assert_eq!(borrows.len(), 1, "the loop's borrow of its subject");
    assert!(
        borrows.iter().all(|data| data.kind == BorrowKind::Shared),
        "§4.4 says the source is borrowed shared, and an indexed loop needs nothing more"
    );
}

/// §7.1's second sentence, and one of the two invariants `lower`'s §6 rests on.
///
/// A loop's borrow is created once, outside the header, and read once per turn.
/// That is the opposite of the created-carried-used-once shape two phases exist
/// for, so `borrow_place` is called with `in_argument` false and the kind is
/// never [`BorrowKind::TwoPhase`]. If it ever were, one reservation would face
/// an activation that runs on every turn.
#[test]
fn a_loops_borrow_is_never_two_phase() {
    let lowered =
        lower("def f(text: &mut String):\n    for c in text.chars():\n        text.push_str(\"x\")\n");
    let body = lowered.body("f");
    // The `push` receiver *is* two-phase — it is an argument borrow — so this
    // fixture has one of each and the test is not vacuous.
    assert!(body.borrows().iter().any(|data| data.kind == BorrowKind::TwoPhase));

    let [loop_borrow] = loop_borrows(&lowered, body)[..] else { panic!("one loop, one borrow") };
    // **Exclusive, not shared, and never two-phase** — which is the invariant
    // this test is named for. `Iterate.next` is `def next(mutable self)`, so a
    // loop over a `Chars` borrows the iterator exclusively; §4.4's shared
    // borrow is of the *source*, `text`, which `chars()` takes and which is
    // not this loan. What must never happen is a second phase: a reservation
    // activated once would face an activation that runs on every turn.
    assert_ne!(loop_borrow.kind, BorrowKind::TwoPhase, "the loop's borrow acquired a second phase");
    assert_eq!(loop_borrow.activation, None, "a loop's borrow is never activated");
}

/// `lib.rs`'s §5: *"a borrow this lowering inserted names the referent, not the
/// reference"*. §9's rule, applied to the one construct §9 did not cover.
///
/// Without it the loan points at the *parameter's* storage, which ends at the
/// function's exit, and rule 5 has the wrong thing to compare against — the
/// finding `science-regions`'s §6 had to work around for method receivers.
#[test]
fn the_borrow_names_the_referent_and_not_the_reference() {
    // **An array subject, because that is where the rule still bites.** A
    // `for` over a `Chars` borrows the temporary the chain produced, which has
    // no reference to see through; a `for` over a `&Array[Int]` borrows the
    // parameter's *referent*, and getting that wrong points the loan at
    // storage that ends at the function's exit.
    let lowered = lower("def f(xs: &Array[Int]):\n    for x in xs:\n        print(x)\n");
    let dump = lowered.dump("f");
    assert!(dump.contains("(*_1)"), "the loop borrowed the reference itself: {dump}");

    let body = lowered.body("f");
    let subject: Vec<_> = body
        .borrows()
        .iter()
        .filter(|data| matches!(data.place.projection.last(), Some(Projection::Deref { .. })))
        .collect();
    assert!(!subject.is_empty(), "no loan was reborrowed through the parameter: {dump}");
}

/// The call reads through the loop's reference, and through nothing else.
///
/// This is what ties the elements to the source for
/// [`science_regions`]'s `generate`'s §5: an opaque callee may return a
/// reference into every argument it was given, so handing it the reference is
/// what makes an element's region bounded by the loan rather than by a
/// temporary that dies with the body.
#[test]
fn the_next_call_reads_through_the_loops_reference() {
    let lowered = lower(OVER_CHARS);
    let body = lowered.body("f");
    let header = body
        .blocks()
        .find(|(_, block)| {
            matches!(&block.terminator.kind,
                TerminatorKind::Call { callee, .. } if is_next(&lowered, callee))
        })
        .expect("the element-producing call")
        .1;
    let TerminatorKind::Call { args, .. } = &header.terminator.kind else { unreachable!() };
    assert_eq!(args.len(), 1, "`next` takes one argument and it is the chain");
    let place = args[0].place().expect("the argument is a place");
    let borrow = body
        .borrows()
        .iter()
        .find(|data| data.destination.local == place.local)
        .expect("the argument is the local the loop's borrow was stored into");
    // Exclusive, because `Iterate.next` is `def next(mutable self)` and the
    // iterator this advances is the loop's own temporary. §4.4's shared borrow
    // is of `text`, which `chars()` took and which is not this loan.
    assert_eq!(borrow.kind, BorrowKind::Exclusive);
}

/// §7.1's *"`borrow_source` is the same helper a written `borrowed x` goes
/// through"*, on the subject that has no place.
///
/// `text.chars()` is a value, so the borrow's referent is a temporary — and the
/// temporary has a storage-dead point, which is what rule 5 needs and what a
/// special case here would have had to invent.
#[test]
fn a_subject_with_no_place_is_borrowed_through_a_temporary() {
    let lowered =
        lower("def f(text: &String):\n    for c in text.chars():\n        print(c)\n");
    let body = lowered.body("f");
    let [loop_borrow] = loop_borrows(&lowered, body)[..] else { panic!("one loop, one borrow") };
    // **Exclusive here and shared everywhere else, and the difference is what
    // the subject *is*.** `Iterate.next` is `def next(mutable self)`, and §4.4
    // says a `for`'s *source* is borrowed shared. Both hold: the source is
    // `text`, which `chars()` borrows shared, and what this loop advances is
    // the `Chars` temporary the chain produced — a value the loop itself owns
    // and nobody else can see. `collections-and-chains.md` §4.2's AMENDMENT 11
    // is the reading that makes them agree: *"`for x in xs:` desugars to
    // `xs.iterate()`"*, so the collection is borrowed and the iterator is
    // advanced. A subject that is a named collection still takes §4.4's shared
    // borrow, which is what the tests above assert.
    assert_eq!(loop_borrow.kind, BorrowKind::Exclusive);
    let referent = loop_borrow.place.local;
    assert_eq!(
        body.local_decl(referent).kind,
        science_mir::LocalKind::Temp,
        "the chain's value did not land in a temporary"
    );
    assert!(
        !body.storage_dead_points(referent).is_empty(),
        "the referent has no storage-dead point, so rule 5 has nothing to compare against"
    );
}

/// Two nested loops are two borrows, and the inner one is not a second borrow
/// of the outer one's subject.
///
/// This is the shape `Scopes.lookup` is, and the constraint chain it produces
/// is why `science-regions`'s acceptance case now takes seven solver sweeps
/// rather than three.
#[test]
fn nested_loops_take_one_borrow_each() {
    let source = concat!(
        "def f(rows: &Array[Array[Int]]):\n",
        "    for row in rows:\n",
        "        for cell in row:\n",
        "            print(cell)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    // **Found by the subjects rather than by a `next` call.** `loop_borrows`
    // locates a loop's borrow through the `Iterate.next` it feeds, and an
    // array's loop calls none, so the loans are read off the body directly.
    //
    // What the test is for is unchanged and is the second assertion: the inner
    // loop borrows the element the outer one bound, **not** the outer loop's
    // own subject. Getting that wrong is how a nested loop aliases the
    // collection it is walking.
    let shared: Vec<_> =
        body.borrows().iter().filter(|data| data.kind == BorrowKind::Shared).collect();
    assert!(shared.len() >= 2, "two loops, at least two loans, found {}", shared.len());
    let locals: std::collections::BTreeSet<_> =
        shared.iter().map(|data| data.place.local).collect();
    assert!(
        locals.len() >= 2,
        "every loan names one local, so the inner loop borrowed the outer loop's subject"
    );
}

/// §7.2, asserted as the hole it is.
///
/// `Iterate.next` is declared and `check`'s `iterate_item` resolves it — it
/// binds `x` at `borrowed Int` here, which is only possible if the lookup
/// succeeded — and `thir::ExprKind::For` carries `{ pattern, iter, body }` and
/// no callee, so this crate cannot name what that lookup found.
///
/// **This test was written to fail and it did its job.** Its instruction was
/// *"the day `ExprKind::For` gains `next: Option<DefId>`, the callee is a
/// [`Callee::Def`] and this assertion is the thing that says so"*. The field
/// exists, `check`'s `iterate_item` returns the `DefId` it was already
/// computing, and `lower_for` builds the call — for a subject whose
/// implementor declares a `next`, which `Chars` does and no other prelude
/// block yet does.
///
/// The name is kept so that `git log -S` finds the sentence that changed.
#[test]
fn the_callee_is_still_the_hole_thir_did_not_fill() {
    let lowered = lower(OVER_CHARS);
    assert_eq!(
        lowered.unresolved("f"),
        Vec::<Unresolved>::new(),
        "`Chars` declares a `next`, so this loop's callee is resolved"
    );
}

/// And the shape around the hole is whole, which is why lowering it was worth
/// more than refusing it: a header, a presence test, a body, a back edge and an
/// exit, with §7.1's borrow outside all of them.
#[test]
fn the_shape_is_lowered_whether_or_not_the_callee_is() {
    let lowered = lower(OVER_CHARS);
    let body = lowered.body("f");
    let header = body
        .blocks()
        .find(|(_, block)| {
            matches!(&block.terminator.kind,
                TerminatorKind::Call { callee, .. } if is_next(&lowered, callee))
        })
        .expect("a header")
        .0;
    assert!(
        body.blocks().any(|(id, _)| id != header && body.successors(id).contains(&header)),
        "the loop has no back edge"
    );
    let [loop_borrow] = loop_borrows(&lowered, body)[..] else { panic!("one loop, one borrow") };
    assert_ne!(loop_borrow.reserved.block, header, "the borrow is retaken on every turn");
    assert!(body.check_predecessors());
    assert!(body.index_temps_are_single_assignment());
}
