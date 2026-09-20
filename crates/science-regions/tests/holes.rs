//! What this crate cannot do, asserted as facts rather than left in prose.
//!
//! `region-inference.md` §7.3 names failures, §9 names what is unsupported, and
//! §14 names risks. [`science_regions`]'s §7 is this crate's version of that
//! list. **A hole recorded only in a doc comment is a hole nobody notices
//! closing**, so each one below is a test that fails the day it is fixed.

mod support;

use science_mir::mir::RETURN_PLACE;
use support::{acceptance, check, codes};

/// [`science_regions::regions`]'s §2 item 1: the walk does not descend into a
/// generic argument, so a borrow stored inside a container has no region
/// variable.
///
/// **This is an under-approximation** — a missed constraint, so a missed error
/// — and it closes when `stdlib-core.md`'s containers become declarations the
/// checker can see.
#[test]
fn a_borrow_inside_a_container_gets_no_region_variable() {
    let source = "\
type Doc:
    title: Int

type Holder:
    items: Array[&Doc]

def hold(h: &Holder) -> Int:
    1
";
    let checked = check(source);
    let body = checked.body("hold");
    let analysis = checked.analysis_of("hold");
    let param = body.params().next().expect("one parameter");
    let positions = analysis.table.local(param);
    assert_eq!(
        positions.len(),
        1,
        "the walk found {} positions; if it found two, it now sees inside `Array` and \
         `regions`'s §2 item 1 is stale",
        positions.len()
    );
}

/// `science-mir`'s `lower` §5: *"every argument of a
/// [`science_mir::mir::Callee::Unresolved`] call is `Copy`, whatever its
/// type"*. So a move through a method call is invisible and `SC0334` cannot
/// see it.
///
/// The program below moves a value into a container while a borrow of it is
/// live. It is accepted, and it should not be.
///
/// **The hole is narrower than it was, and the fixture moved to say so.** It
/// used to call `list.push(doc)`, and `Array.push` now has a declaration — so
/// that program is a resolved call, the argument is a real move, and `SC0334`
/// fires on it correctly. The claim is unchanged and its *reach* has shrunk to
/// exactly the calls that are still holes, which is what the fixture now uses:
/// `Array.insert` is in `stdlib-core.md` §3.6 and not in the prelude. Each
/// declaration that lands takes another program out of this hole, and the day
/// there is no undeclared container method left to write, the hole is closed
/// and this test should be deleted rather than re-pointed.
#[test]
fn a_move_through_an_unresolved_call_is_invisible() {
    let source = "\
type Doc:
    title: Int

def look(d: &Doc) -> Int:
    d.title

def go():
    let mutable list be Array[Doc].new()
    let doc be Doc(title: 0)
    let s be &doc
    list.insert(0, doc)
    print(look(s))
";
    let checked = check(source);
    assert_eq!(
        checked.reported(),
        Vec::<u16>::new(),
        "`SC0334` can now see through an unresolved call, which means either the method \
         lookup landed or `lower`'s §5 changed: {:?}",
        codes(&checked.regions)
    );
}

/// [`science_regions::codes::NO_COMMON_REGION`] has never fired, and
/// [`science_regions`]'s §5 argues it cannot in a solver with no upper bounds.
///
/// The fixture is the shape §4.3 describes: two borrowed fields from two
/// unrelated sources, one of which dies first. What is reported is `SC0333`
/// against the source that dies, not `SC0335` against the geometry.
#[test]
fn decision_threes_empty_intersection_surfaces_as_rule_five() {
    let source = "\
type Plan:
    input: &Int
    output: &Int

def build(a: &Int, b: &Int) -> Plan:
    Plan(input: a, output: b)

def go(long: &Int) -> Plan:
    let short be 1
    build(long, &short)
";
    let checked = check(source);
    assert_eq!(
        checked.reported(),
        vec![333],
        "the empty-intersection case reported something other than rule 5: {:?}",
        codes(&checked.regions)
    );
}

/// And `SC0335` fires nowhere in the corpus.
#[test]
fn no_common_region_never_fires_on_the_acceptance_case() {
    let checked = acceptance();
    assert!(!checked.reported().contains(&335));
}

/// [`science_regions::regions`]'s §2 item 4: the depth cap is a tripwire, and
/// *"no program hit it"* should be measured.
#[test]
fn the_depth_cap_is_not_reached_by_the_acceptance_case() {
    let checked = acceptance();
    for body in &checked.bodies {
        let analysis = checked.analysis.body(body.def()).expect("analysed");
        assert!(
            !analysis.table.truncated(),
            "`{}` hit MAX_DEPTH, so its regions are under-approximated",
            checked.krate.defs.get(body.def()).name
        );
    }
}

/// Decision 4: *"a type may not be generic over a region"*. A `T` carries no
/// region, so a generic container of borrows is not expressible and nothing
/// here pretends otherwise.
#[test]
fn a_type_parameter_carries_no_region() {
    let source = "\
type Box[T]:
    value: T

def hold[T](b: &Box[T]) -> Int:
    1
";
    let checked = check(source);
    let body = checked.body("hold");
    let analysis = checked.analysis_of("hold");
    let param = body.params().next().expect("one parameter");
    assert_eq!(
        analysis.table.local(param).len(),
        1,
        "a `T` acquired a region, which would mean Decision 4 has been reversed"
    );
}

/// Decision 10's escape hatch is untouched: MIR carries no `unsafe` marker
/// (`science-mir`'s §5 lists it among what will bite), so an arena that hands
/// out borrows is refused here and writing `unsafe` around it changes nothing.
///
/// Asserted on the shape §9 item 3 names, so that the day a marker arrives this
/// test says where to look.
#[test]
fn unsafe_does_not_relax_anything_because_mir_carries_no_marker() {
    let source = "\
type Arena:
    items: Array[Int]

Arena has:
    def alloc(self) -> &Int:
        let value be 0
        &value
";
    let checked = check(source);
    assert_eq!(checked.reported(), vec![333]);

    let wrapped = "\
type Arena:
    items: Array[Int]

Arena has:
    def alloc(self) -> &Int:
        unsafe:
            let value be 0
            &value
";
    let checked = check(wrapped);
    assert_eq!(
        checked.reported(),
        vec![333],
        "an `unsafe` block changed the answer, which it cannot: there is nothing in MIR to read"
    );
}

/// `SC0341` is reserved by §12 and the query it would ask is public.
#[test]
fn borrows_live_at_is_available_for_the_suspension_check_that_does_not_exist() {
    let checked = acceptance();
    let body = checked.body("main");
    let live: usize = body
        .points()
        .map(|point| checked.analysis.borrows_live_at(body.def(), point).len())
        .sum();
    assert!(live > 0, "the query answers nothing, so `SC0341` would have nothing to check");
}

/// The return place's regions are what Decision 5 computes, and a body with no
/// reference in its return has none — which is most of them.
#[test]
fn most_bodies_have_no_return_region_at_all() {
    let checked = acceptance();
    let with_regions = checked
        .bodies
        .iter()
        .filter(|body| {
            !checked
                .analysis
                .body(body.def())
                .expect("analysed")
                .table
                .local(RETURN_PLACE)
                .is_empty()
        })
        .count();
    assert_eq!(
        with_regions, 4,
        "the acceptance case has four borrow-returning bodies: `DefTable.get`, \
         `Parser.new`, `Parser.peek` and `Node.new`"
    );
}
