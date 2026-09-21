//! §4.6's `<<` and `>>`, and the guard this crate emits in front of them.
//!
//! The claim under test is `science_codegen::backend::IntOp`'s and
//! `science-codegen-llvm` refused both shifts for the absence of it: an
//! unguarded `shl`/`lshr`/`ashr` is poison at or past the operand's bit width —
//! not a wrong value, no value — and this crate is the caller that makes it
//! safe to emit. `tests/division.rs` is the model this file follows
//! statement for statement, because `shift_check` is `division_check`'s shape;
//! the one structural difference — one shared trap block here, two distinct
//! ones there — gets its own test rather than being asserted by accident.

mod support;

use science_mir::mir::{BlockId, Callee, Rvalue, StatementKind, TerminatorKind};
use science_resolve::hir::BinaryOp;
use support::lower;

/// Every message a body hands to `science_panic_bytes`, in block order.
fn panics(lowered: &support::Lowered, name: &str) -> Vec<String> {
    use science_mir::mir::{Constant, Operand};
    use science_resolve::hir::Literal;
    lowered
        .body(name)
        .blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Runtime("science_panic_bytes"), args, .. } => {
                match args.first() {
                    Some(Operand::Const(Constant::Literal(Literal::Str(text)))) => {
                        Some(text.clone())
                    }
                    _ => Some(String::from("<not a literal>")),
                }
            }
            _ => None,
        })
        .collect()
}

/// How many `TerminatorKind::If` blocks a body has — the guard's tests,
/// counted rather than named, because a signed and an unsigned shift share
/// one panic message (`panics` cannot tell them apart) and differ only in how
/// many comparisons lead to it.
fn if_count(lowered: &support::Lowered, name: &str) -> usize {
    lowered
        .body(name)
        .blocks()
        .filter(|(_, block)| matches!(block.terminator.kind, TerminatorKind::If { .. }))
        .count()
}

#[test]
fn a_signed_shift_left_is_guarded_against_width_and_negative() {
    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a << b\n");
    assert_eq!(panics(&lowered, "f"), vec!["shift amount out of range"]);
}

#[test]
fn a_signed_shift_right_is_guarded_the_same_way() {
    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a >> b\n");
    assert_eq!(panics(&lowered, "f"), vec!["shift amount out of range"]);
}

#[test]
fn an_unsigned_shift_is_guarded_only_against_width() {
    let lowered = lower("def f(a: U8, b: U8) -> U8:\n    a << b\n");
    assert_eq!(panics(&lowered, "f"), vec!["shift amount out of range"]);
}

/// **The one place a shift's guard is not `division_check`'s shape**: one
/// panic block serves both tests here, where division gives the zero and the
/// overflow their own messages. Counting the comparisons rather than the
/// messages is what makes the difference between a signed and an unsigned
/// amount visible at all.
#[test]
fn a_signed_shift_pays_for_two_tests_and_an_unsigned_one_for_one() {
    let signed = lower("def f(a: Int, b: Int) -> Int:\n    a << b\n");
    assert_eq!(if_count(&signed, "f"), 2, "a signed amount: negative, then width");

    let unsigned = lower("def f(a: U8, b: U8) -> U8:\n    a << b\n");
    assert_eq!(if_count(&unsigned, "f"), 1, "an unsigned amount: width only");
}

#[test]
fn the_other_bitwise_operators_are_not_guarded() {
    for source in [
        "def f(a: Int, b: Int) -> Int:\n    a & b\n",
        "def f(a: Int, b: Int) -> Int:\n    a | b\n",
        "def f(a: Int, b: Int) -> Int:\n    a ^ b\n",
    ] {
        let lowered = lower(source);
        assert!(panics(&lowered, "f").is_empty(), "{source}");
    }
}

#[test]
fn the_other_arithmetic_operators_are_not_guarded_by_this_crate() {
    for source in [
        "def f(a: Int, b: Int) -> Int:\n    a + b\n",
        "def f(a: Int, b: Int) -> Int:\n    a - b\n",
        "def f(a: Int, b: Int) -> Int:\n    a * b\n",
    ] {
        let lowered = lower(source);
        assert!(panics(&lowered, "f").is_empty(), "{source}");
    }
}

/// The guard is in *front* of the operator, not beside it: every path that
/// reaches the `Rvalue::Binary` has passed both tests — `division.rs`'s
/// `the_division_is_unreachable_from_a_failing_test`, one construct over.
#[test]
fn the_shift_is_unreachable_from_a_failing_test() {
    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a << b\n");
    let body = lowered.body("f");
    let shifts: Vec<BlockId> = body
        .blocks()
        .filter(|(_, block)| {
            block.statements.iter().any(|statement| {
                matches!(
                    &statement.kind,
                    StatementKind::Assign {
                        rvalue: Rvalue::Binary { op: BinaryOp::Shl, .. },
                        ..
                    }
                )
            })
        })
        .map(|(id, _)| id)
        .collect();
    assert_eq!(shifts.len(), 1, "one shift");

    // Nothing that panics can reach the block that shifts.
    for (id, block) in body.blocks() {
        let panicking = matches!(
            &block.terminator.kind,
            TerminatorKind::Call { callee: Callee::Runtime("science_panic_bytes"), .. }
        );
        if !panicking {
            continue;
        }
        assert_eq!(body.successors(id).len(), 0, "a panic is diverging and has no successor");
    }
}
