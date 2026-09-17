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
    let body = checked.body("go");
    // The borrows **of `config`**, for the reason
    // [`a_closure_capture_is_a_borrow_this_engine_can_see`] gives one test
    // over: `print(config.port)` renders through §1.7's builder now and the
    // builder borrows its own accumulator. What this test is about is that
    // nothing borrows `config` a second time, so a conflict check quantified
    // over borrows *of the narrowed place* finds nothing to compare.
    let borrows: usize = support::borrows_of(&checked, body, "config").len();
    assert_eq!(
        borrows, 1,
        "the fixture no longer demonstrates what it exists for: `config` has {borrows} borrows"
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

/// **The hole that was left, closed — and the guard that noticed.**
///
/// This test used to assert the opposite: that a closure capture produced *no*
/// [`science_mir::mir::BorrowData`], with the note *"the day captures become
/// borrows the assertion fails and somebody reads §6"*. The day came;
/// `science-mir`'s `lower` §8 is the discipline and [`crate`]'s §6 is rewritten
/// around it.
///
/// What it asserts now is the fact §6 turns on: a closure that names a place
/// from outside itself takes a real borrow of it, at the point the closure is
/// built.
#[test]
fn a_closure_capture_is_a_borrow_this_engine_can_see() {
    let source = "type Doc:
    title: Int

def sink(f: (Int) -> Int) -> Int:
    1

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
    // **The borrows *of `doc`*, and not every borrow in the body.** It was the
    // total until `science-mir`'s §7 item 17 made `print(n)` render `n` through
    // §1.7's builder, which takes a `mutable borrowed String` of an accumulator
    // this fixture does not contain a name for. That loan is real and it is
    // nothing to do with the capture, so counting it here would make this test
    // fail for a reason that is not its subject — and dropping the count
    // altogether would let the capture's borrow disappear unnoticed, which is
    // the thing the test exists to prevent.
    let of_doc = support::borrows_of(&checked, body, "doc");
    assert_eq!(of_doc.len(), 1, "the capture of `doc` is a borrow");
    assert_eq!(of_doc[0].kind, science_mir::BorrowKind::Shared);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
}

/// **Decision 8, extended to closures for Decision 8's own reason.**
///
/// The narrowing on `config.port` is established, and a closure then takes an
/// exclusive borrow of `config` while the fact is still being read. Before §8
/// there was no borrow here at all and [`crate`]'s §6 had to say the safety was
/// coming from somewhere else — `science-types` walking the closure body inline.
/// It is rule 4 now, over an access and a loan, exactly as
/// `region-inference.md`'s AMENDMENT 3 writes it.
#[test]
fn an_exclusive_capture_invalidates_a_narrowing_by_rule_4() {
    let source = format!(
        "{CONFIG}
def write_it(c: mutable borrowed Config) -> Bool:
    c.port be 1
    true

def sink(f: (Bool) -> Bool) -> Bool:
    true

def go():
    let mutable config be Config(port: null)
    if config.port?:
        let f be item giving write_it(mutable borrowed config)
        print(config.port)
        let b be sink(f)
"
    );
    let checked = check(&source);
    assert_eq!(
        checked.reported(),
        vec![330],
        "an exclusive capture did not conflict with the narrowed read: {:?}",
        codes(&checked.regions)
    );
}
