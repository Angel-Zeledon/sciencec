//! Decision 1's lattice and §3 step 3's fixpoint, held to the properties the
//! rest of the crate assumes.
//!
//! Two of these exist because the implementation got them wrong once and the
//! symptom was a *missing* diagnostic, which no other test in the suite can
//! see: a checker that reports nothing passes every test written to assert
//! that a good program is accepted.

mod support;

use science_regions::points::{Bits, PointIndex};
use support::check;

/// [`science_regions::solve`]'s §2: the propagation starts at the constraint
/// point's *successors*, so a loan's region is more than the point it was taken
/// at. Started at the point itself, every region here would be empty.
#[test]
fn a_loans_region_is_more_than_the_point_it_was_taken_at() {
    let source = "\
type Doc:
    title: Int

def look(d: &Doc) -> Int:
    d.title

def go():
    let doc be Doc(title: 0)
    let s be &doc
    print(look(s))
";
    let checked = check(source);
    let body = checked.body("go");
    let analysis = checked.analysis_of("go");
    // **The loan of `doc`, and not the only loan in the body.** It was the only
    // one until `science-mir`'s §7 item 17 made `print(look(s))` render its
    // `Int` through §1.7's builder, which takes a `mutable borrowed String` of
    // an accumulator no name in this fixture reaches. The loan this test is
    // about is the one the author wrote, so it is selected rather than indexed.
    let loans = support::borrows_of(&checked, body, "doc");
    assert_eq!(loans.len(), 1, "the fixture takes one borrow of `doc`");
    let region = analysis.loan_region(loans[0].id);
    assert!(region.len() > 1, "the loan's region is {} point(s)", region.len());
    assert!(
        !region.contains(analysis.index.index(loans[0].reserved)),
        "the reservation point itself is in the region, so the region starts one point early"
    );
}

/// The visited-set fix of [`science_regions::solve`]'s §2. A loan whose region
/// is grown across two blocks by a constraint applied on an early pass must
/// still grow when the constrained region grows later.
///
/// The shape: a borrow flows into a value that is returned two blocks away.
/// With `'a` used as the visited set, the walk stops at the first point it
/// added on the earlier pass and the region never leaves the first block.
#[test]
fn a_region_keeps_growing_after_the_pass_that_first_touched_it() {
    let source = "\
type Def:
    name: Int

type Table:
    items: Array[Def]

Table has:
    def get(self, at: Int) -> &Def:
        self.items.get(at)

    def first(self) -> &Def:
        let found be self.get(0)
        found
";
    let checked = check(source);
    let body = checked.body("first");
    let analysis = checked.analysis_of("first");
    assert_eq!(body.borrows().len(), 1, "one receiver borrow");
    let region = analysis.loan_region(body.borrows()[0].id);
    let blocks: std::collections::BTreeSet<_> =
        region.iter().map(|at| analysis.index.point(at).block).collect();
    assert!(
        blocks.len() > 1,
        "the loan's region never left its block, so the fixpoint stopped early: {blocks:?}"
    );
}

/// §3's *"linear in practice"*, as a number rather than a claim.
#[test]
fn the_fixpoint_converges_in_a_handful_of_sweeps() {
    let source = "\
def count(n: Int) -> Int:
    let mutable total be 0
    let mutable i be 0
    loop:
        if i >= n:
            break
        total be total + i
        i be i + 1
    total
";
    let checked = check(source);
    assert!(checked.analysis_of("count").solution.iterations() <= 4);
}

/// Decision 1's lattice, directly. The order is inclusion and nothing else:
/// two incomparable regions are incomparable in both directions.
#[test]
fn the_lattice_order_is_set_inclusion() {
    let mut a = Bits::empty(8);
    let mut b = Bits::empty(8);
    a.insert(1);
    a.insert(2);
    b.insert(2);
    b.insert(3);
    assert!(!a.contains_all(&b));
    assert!(!b.contains_all(&a));

    let mut both = a.clone();
    assert!(both.union_with(&b));
    assert!(both.contains_all(&a) && both.contains_all(&b));
    assert!(!both.union_with(&b), "a second union changed something");

    let mut common = a.clone();
    common.intersect_with(&b);
    assert_eq!(common.iter().collect::<Vec<_>>(), vec![2]);
}

/// [`PointIndex`] is a bijection, which every [`Bits`] in the crate assumes.
#[test]
fn the_point_numbering_round_trips() {
    let source = "\
def branchy(n: Int) -> Int:
    if n > 0:
        return 1
    loop:
        if n is 0:
            break
        n be n + 1
    0
";
    let checked = check(source);
    let body = checked.body("branchy");
    let index = PointIndex::of(body);
    assert_eq!(index.len(), body.point_count());
    for point in body.points() {
        assert_eq!(index.point(index.index(point)), point);
    }
    for at in index.all() {
        assert_eq!(index.index(index.point(at)), at);
    }
}

/// A parameter's region holds everywhere — [`science_regions::generate`]'s §4 —
/// and the return place's does not, which is what makes the summary an answer
/// rather than a tautology.
#[test]
fn a_parameter_region_holds_everywhere_and_a_return_region_does_not() {
    let source = "\
def first(x: &Int, n: Int) -> &Int:
    let mutable i be 0
    loop:
        if i >= n:
            break
        i be i + 1
    x
";
    let checked = check(source);
    let body = checked.body("first");
    let analysis = checked.analysis_of("first");

    let param = body.params().next().expect("one parameter");
    let (_, var) = analysis.table.local(param)[0].clone();
    assert_eq!(analysis.solution.region(var).len(), body.point_count());

    let (_, ret) = analysis.table.local(science_mir::mir::RETURN_PLACE)[0].clone();
    assert!(
        analysis.solution.region(ret).len() < body.point_count(),
        "the return region covers the whole body, so nothing was inferred"
    );
}
