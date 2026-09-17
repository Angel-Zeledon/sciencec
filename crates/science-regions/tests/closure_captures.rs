//! The hole [`science_regions`]'s §6 and §7 named first, closed and measured.
//!
//! > **The hole that is left is closures**, and it is exactly `science-mir`'s
//! > `lower` §8. A closure that captures a place exclusively produces **no
//! > borrow in MIR**, so no region, so rule 4 cannot forbid anything about it.
//! > … **It is not rule 4 doing it**, and when closures acquire a capture
//! > discipline … whichever note writes it must decide whether captures become
//! > borrows MIR can see.
//!
//! They do. `science-mir`'s `lower` §8 makes every capture a borrow at the
//! point the closure value is created, and this file is what that buys:
//! **rule 4 now covers a closure, flow-sensitively, with no rule of its own.**
//!
//! The corpus exercises none of this — `capture`'s §4: every closure in
//! `examples/` names only its own subject — so these programs are the
//! acceptance material, and `tests/corpus.rs` is the guarantee that the census
//! did not move.

mod support;

use science_regions::regions::{RegionKind, Step};
use support::{check, codes};

const PRE: &str = "\
type Config:
    port: Int
    host: Int

def sink(f: (Int) -> Int) -> Int:
    1

def bump(c: mutable borrowed Config) -> Int:
    1

def peek(c: borrowed Config) -> Int:
    1
";

/// **Rule 4, across a closure.** The closure holds a shared borrow of `c` from
/// the point it is built until its last use, and the exclusive borrow in the
/// middle is refused.
///
/// This is the program §6 said nothing could refuse. Nothing here is closure-
/// specific: the conflict check does not know what a closure is.
#[test]
fn a_write_while_a_captured_borrow_is_live_is_refused() {
    let source = format!(
        "{PRE}
def go(c: mutable borrowed Config) -> Int:
    let f be item giving c.port
    let n be bump(mutable borrowed c)
    sink(f)
"
    );
    let checked = check(&source);
    assert_eq!(
        checked.reported(),
        vec![330],
        "a capture is not stopping a conflicting write: {:?}",
        codes(&checked.regions)
    );
}

/// **And flow-sensitively.** The same three lines in a different order compile,
/// because the capture's region ends at the closure's last use and NLL is
/// points and not scopes. Had this been answered with a lexical rule, a stored
/// chain would be close to unusable — `collections-and-chains.md` §2.3's own
/// warning.
#[test]
fn the_same_write_after_the_closure_is_dead_is_accepted() {
    let source = format!(
        "{PRE}
def go(c: mutable borrowed Config) -> Int:
    let f be item giving c.port
    let n be sink(f)
    bump(mutable borrowed c)
"
    );
    let checked = check(&source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
}

/// The capture loan is live over the closure value's whole life and not over
/// the one statement that built it. That is the property the whole of §6 turns
/// on, asserted directly rather than through a diagnostic.
#[test]
fn a_capture_loan_outlives_the_statement_that_took_it() {
    let source = format!(
        "{PRE}
def go(c: borrowed Config) -> Int:
    let f be item giving c.port
    let n be peek(c)
    sink(f)
"
    );
    let checked = check(&source);
    let body = checked.body("go");
    let analysis = checked.analysis_of("go");
    assert_eq!(body.borrows().len(), 1, "one capture, one loan");
    let loan = analysis.loan_region(body.borrows()[0].id);
    let reserved = analysis.index.index(body.borrows()[0].reserved);
    let live: Vec<usize> = loan.iter().collect();
    assert!(live.len() > 1, "the loan is live at one point only: {live:?}");
    assert!(
        live.iter().all(|point| *point > reserved),
        "a loan starts after its reservation, §2 of `solve`"
    );
}

/// **Decision 3, at the one aggregate whose type does not describe itself.**
/// Two captures get two region variables and nothing relates them — the same
/// answer `Node of T` gets, reached through [`Step::Capture`] instead of
/// through a record's fields.
#[test]
fn two_captures_get_two_unrelated_regions() {
    let source = format!(
        "{PRE}
def go(c: borrowed Config, d: borrowed Config) -> Int:
    let f be item giving c.port + d.host
    sink(f)
"
    );
    let checked = check(&source);
    let analysis = checked.analysis_of("go");
    let closure = checked
        .body("go")
        .local_of(checked.def("f", science_resolve::hir::DefKind::Local))
        .expect("`f` has a local");
    let positions = analysis.table.local(closure);
    assert_eq!(positions.len(), 2, "one region variable per capture");
    assert_eq!(positions[0].0.path, vec![Step::Capture(0)]);
    assert_eq!(positions[1].0.path, vec![Step::Capture(1)]);
    let (left, right) = (positions[0].1, positions[1].1);
    assert!(
        !analysis
            .constraints
            .all()
            .iter()
            .any(|it| (it.sup == left && it.sub == right) || (it.sup == right && it.sub == left)),
        "nothing may relate one capture's region to another's"
    );
}

/// A closure that captures nothing adds no region and no loan, so the machinery
/// is free on the shape the corpus actually writes.
#[test]
fn a_capture_free_closure_adds_no_region() {
    let source = format!(
        "{PRE}
def go() -> Int:
    sink(item giving item)
"
    );
    let checked = check(&source);
    let analysis = checked.analysis_of("go");
    assert_eq!(checked.body("go").borrows().len(), 0);
    assert!(
        !analysis.table.vars().any(|var| matches!(
            analysis.table.kind(var),
            RegionKind::Local { position, .. }
                if position.path.iter().any(|step| matches!(step, Step::Capture(_)))
        )),
        "a closure with no captures has no capture positions"
    );
    assert_eq!(checked.reported(), Vec::<u16>::new());
}

/// **Rule 5, across a closure — and the half a conservative rule gets wrong.**
///
/// A closure capturing a *local* is an ordinary borrow of storage that dies at
/// the end of the body, and the closure dies before it does. Nothing escapes,
/// so nothing is reported. `collections-and-chains.md` §2.4 is why there is no
/// companion test for the escaping case: a closure type cannot cross a function
/// boundary in F0, so a capture has no way out of the frame to be refused for.
#[test]
fn a_closure_capturing_a_local_is_not_refused() {
    let source = format!(
        "{PRE}
def go() -> Int:
    let c be Config(port: 1, host: 2)
    let f be item giving c.port
    sink(f)
"
    );
    let checked = check(&source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
    assert_eq!(checked.body("go").borrows().len(), 1, "one capture of `c`");
}
