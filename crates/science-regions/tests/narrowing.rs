//! `type-checking-and-mir.md` Decision 8, and whether its phase-ordering
//! argument survives.
//!
//! > **Decision 8: narrowing relies on rule 4 and records the dependency.** An
//! > exclusive borrow of `x` is the only way to write `x`, rule 4 forbids it
//! > while any other borrow is live, and the narrowing is invalidated at the
//! > point the exclusive borrow is *created*, not where it writes.
//!
//! That note's §15 records the argument as *"not a proof"* and names one way it
//! could fail: a future THIR pass that consumes narrowing for something codegen
//! depends on. [`science_regions`]'s §6 is the finding that there is a nearer
//! one, and these are the programs.

mod support;

use support::{check, codes};

const CONFIG: &str = "\
type Config:
    port: Int?

def write_through(c: mutable borrowed Config):
    c.port be 1

def read_it(c: borrowed Config) -> Bool:
    true
";

/// **The program Decision 8 rests on, and the one §3 step 4 as written would
/// accept.**
///
/// The exclusive borrow is created *before* the narrowing, so
/// invalidation-at-creation does not help: the creation is in the past when
/// `config.port?` establishes the fact. What has to refuse it is rule 4 — and
/// what rule 4 has to refuse is the **read** of `config.port` at a point where
/// `r`'s region is live.
///
/// There is exactly one borrow in this program. A conflict check that
/// quantified over *borrows*, which is what §3 step 4 says, would find no
/// second borrow, accept, and leave the narrowing on line 3 true of a value
/// line 4 has overwritten.
#[test]
fn a_read_through_a_place_held_exclusively_is_refused() {
    let source = format!(
        "{CONFIG}
def go():
    let mutable config be Config(port: null)
    let r be mutable borrowed config
    if config.port?:
        write_through(r)
        print(config.port)
"
    );
    let checked = check(&source);
    assert_eq!(
        checked.reported(),
        vec![330],
        "Decision 8's dependency is not enforced: {:?}",
        codes(&checked.regions)
    );
}

/// The count that makes the point: **one** borrow, so borrow-against-borrow has
/// nothing to compare.
#[test]
fn that_program_contains_exactly_one_borrow() {
    let source = format!(
        "{CONFIG}
def go():
    let mutable config be Config(port: null)
    let r be mutable borrowed config
    if config.port?:
        write_through(r)
        print(config.port)
"
    );
    let checked = check(&source);
    let borrows: usize = checked.body("go").borrows().len();
    assert_eq!(
        borrows, 1,
        "the fixture no longer demonstrates what it exists for: it has {borrows} borrows"
    );
}

/// The ordinary case still compiles: a narrowing with no borrow anywhere near
/// it is not made harder by the amendment.
#[test]
fn a_narrowing_with_no_borrow_in_sight_compiles() {
    let source = format!(
        "{CONFIG}
def go():
    let config be Config(port: null)
    if config.port?:
        print(config.port)
"
    );
    let checked = check(&source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
}

/// A *shared* borrow held across the narrowing is fine, because a shared borrow
/// cannot write. Rule 4's permissive half is what keeps the amendment from
/// being a tax on every narrowing.
#[test]
fn a_shared_borrow_does_not_disturb_a_narrowing() {
    let source = format!(
        "{CONFIG}
def go():
    let config be Config(port: null)
    let r be borrowed config
    if config.port?:
        print(read_it(r))
        print(config.port)
"
    );
    let checked = check(&source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
}

/// **The hole that is left, asserted as a hole.**
///
/// `science-mir`'s `lower` §8 does not lower a closure's body, so a closure
/// that captures a place produces no [`science_mir::mir::BorrowData`] and rule
/// 4 has nothing to say about it. Narrowing is nevertheless safe today because
/// `science-types`'s checker walks the closure body inline with the enclosing
/// facts — which is a *different* mechanism from the one Decision 8 names.
///
/// This test asserts the MIR-level fact, so that the day captures become
/// borrows the assertion fails and somebody reads §6.
#[test]
fn a_closure_capture_is_not_a_borrow_this_engine_can_see() {
    let source = "\
type Doc:
    title: Int

def sink(v: Int) -> Int:
    v

def go():
    let doc be Doc(title: 0)
    let n be sink(item giving doc.title)
    print(n)
";
    let checked = check(source);
    let body = checked.body("go");
    let closures = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|statement| {
            matches!(
                &statement.kind,
                science_mir::StatementKind::Assign {
                    rvalue: science_mir::Rvalue::Closure { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(closures, 1, "the fixture no longer builds a closure");
    assert_eq!(
        body.borrows().len(),
        0,
        "a closure capture became a borrow; `region-inference.md`'s §6 finding needs revisiting"
    );
    assert_eq!(checked.reported(), Vec::<u16>::new());
}
