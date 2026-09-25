//! §5.1: `T implements Copy` reaches the operand rule, which is §6.1 rule 2's
//! *"unless the type is `Copy`"*.
//!
//! # What this file is the record of
//!
//! `lower`'s §5 used to say that *"whether a user type is `Copy` is Decision
//! 11's lookup and this compiler has none, so every record operand arrives here
//! as a move"*, and two crates were built on that sentence: `science-regions`'
//! `moved` §3 item 4 declines rule 3 for a type that owns nothing, and its
//! `check` §3.1 is what the rule-4 half looked like.
//!
//! **The sentence was stale.** `science-types`' `assign.rs` §7 needs the same
//! id and finds it with [`science_types::assign::Coercions::of`]; `lower` now
//! reads it from there, beside the `Box` it was already reading. So the
//! measurement below is of a lookup that exists, and each test names the half
//! of §5.1 it pins.
//!
//! The program that made it worth doing is `a is a`: `is` dispatches to the
//! user's `eq`, `eq`'s bare `self` is a *shared borrow* of the receiver, and
//! `other` was a `Move` of the same local — so rule 4 refused a comparison on a
//! `Copy` record. The fix is here and not in the checker that reported it.

mod support;

use science_mir::mir::{Operand, TerminatorKind};
use support::lower;

/// The operands of the one call in `f`, in argument order.
fn call_operands(lowered: &support::Lowered, function: &str) -> Vec<String> {
    let body = lowered.body(function);
    for (_, block) in body.blocks() {
        if let TerminatorKind::Call { args, .. } = &block.terminator.kind {
            return args
                .iter()
                .map(|arg| match arg {
                    Operand::Copy(place) => format!("copy _{}", place.local.index()),
                    Operand::Move(place) => format!("move _{}", place.local.index()),
                    Operand::Const(_) => "const".to_string(),
                })
                .collect();
        }
    }
    panic!("`{function}` has no call in it");
}

const COPY_RECORD: &str = "\
type P:
    x: Int

P has:
    def same(self, other: P) -> Bool:
        self.x is other.x

P implements Copy
";

const PLAIN_RECORD: &str = "\
type P:
    x: Int

P has:
    def same(self, other: P) -> Bool:
        self.x is other.x
";

/// §5.1, the whole of it. The receiver is a borrow — `self` is
/// [`science_resolve::hir::SelfKind::Shared`] — and the argument is a **copy**,
/// because the type says `Copy` and owns nothing.
#[test]
fn a_copy_records_by_value_argument_is_a_copy() {
    let source = format!(
        "{COPY_RECORD}
def f() -> Bool:
    let a be P(x: 1)
    a.same(a)
"
    );
    // `_2` is the temporary holding the receiver's borrow; `_1` is `a`.
    assert_eq!(call_operands(&lower(&source), "f"), vec!["move _2", "copy _1"]);
}

/// The same program with the `implements Copy` line removed. Rule 2 says
/// passing *moves* unless the type is `Copy`, so this one still moves — and the
/// `SC0334` `science-regions` reports about it is that rule working.
#[test]
fn a_plain_records_by_value_argument_is_still_a_move() {
    let source = format!(
        "{PLAIN_RECORD}
def f() -> Bool:
    let a be P(x: 1)
    a.same(a)
"
    );
    assert_eq!(call_operands(&lower(&source), "f"), vec!["move _2", "move _1"]);
}

/// **The guard, and it is not redundant.** Nothing in the compiler checks that
/// a `Copy` implementation is well formed, so `type P: text: String` followed
/// by `P implements Copy` is a program the resolver and the checker both
/// accept. Believing it would copy a `String`'s three words, leave the source's
/// drop standing, and free the buffer twice. §5's asymmetry therefore still
/// holds wherever there is doubt: a type that owns something keeps its `Move`
/// whatever it declares.
#[test]
fn a_copy_declaration_on_a_type_that_owns_something_is_not_believed() {
    let source = "\
type P:
    text: String

P has:
    def len(self, other: P) -> Int:
        self.text.length() + other.text.length()

P implements Copy

def f() -> Int:
    let a be P(text: \"hi\")
    let b be P(text: \"yo\")
    a.len(b)
";
    // `_3` is the receiver's borrow temporary, `_2` is `b`.
    assert_eq!(call_operands(&lower(source), "f"), vec!["move _3", "move _2"]);
}

/// `a is a` is the program this was found on, and it lowers to the method call
/// the two tests above measure — `science-types`' §6 dispatches `is` to `eq`.
#[test]
fn is_on_a_copy_record_copies_its_right_operand() {
    let source = "\
type P:
    x: Int

P implements Eq:
    def eq(self, other: P) -> Bool:
        self.x is other.x

P implements Copy

def f() -> Bool:
    let a be P(x: 1)
    a is a
";
    assert_eq!(call_operands(&lower(source), "f"), vec!["move _2", "copy _1"]);
}
