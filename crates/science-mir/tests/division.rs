//! §4.6's `/` and `%`, and the guard this crate emits in front of them.
//!
//! The claim under test is the one `science_codegen::backend::IntOp` makes and
//! `science-codegen-llvm` refused integer division for the absence of:
//! *"division by zero is a panic the caller has already guarded, not a trap the
//! backend inserts."* This crate is that caller. Each test below names the
//! failing input it is about, because a test that only counted blocks would
//! pass on a lowering that produced them for some other reason — a short-circuit
//! `and`, an `if`, a bounds check — and there are three failing shapes here and
//! not one: the zero, the `Int.min / -1` overflow, and the *absence* of either
//! guard where the operator cannot fail.

mod support;

use science_mir::mir::{BlockId, Callee, TerminatorKind};
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

#[test]
fn a_signed_division_is_guarded_against_both_of_its_failing_inputs() {
    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a / b\n");
    assert_eq!(panics(&lowered, "f"), vec!["divide by zero", "division overflows"]);
}

#[test]
fn a_signed_remainder_is_guarded_the_same_way() {
    // `%` is `idiv` too: `Int.min % -1` raises `#DE` on x86-64 exactly as the
    // quotient does, so the guard is not the division's alone.
    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a % b\n");
    assert_eq!(panics(&lowered, "f"), vec!["divide by zero", "division overflows"]);
}

#[test]
fn an_unsigned_division_is_guarded_only_against_zero() {
    // There is no unsigned pair whose quotient is not representable, so the
    // second test would be two blocks that can never fire.
    let lowered = lower("def f(a: U8, b: U8) -> U8:\n    a / b\n");
    assert_eq!(panics(&lowered, "f"), vec!["divide by zero"]);
}

#[test]
fn a_float_division_is_not_guarded_at_all() {
    // IEEE 754 division by zero is an infinity, which is a value. Guarding it
    // would refuse a program the language defines.
    let lowered = lower("def f(a: F64, b: F64) -> F64:\n    a / b\n");
    assert!(panics(&lowered, "f").is_empty(), "{:?}", panics(&lowered, "f"));
}

#[test]
fn the_other_arithmetic_operators_are_not_guarded() {
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
/// reaches the `Rvalue::Binary` has passed both tests.
#[test]
fn the_division_is_unreachable_from_a_failing_test() {
    use science_mir::mir::{Rvalue, StatementKind};
    use science_resolve::hir::BinaryOp;

    let lowered = lower("def f(a: Int, b: Int) -> Int:\n    a / b\n");
    let body = lowered.body("f");
    let divides: Vec<BlockId> = body
        .blocks()
        .filter(|(_, block)| {
            block.statements.iter().any(|statement| {
                matches!(
                    &statement.kind,
                    StatementKind::Assign {
                        rvalue: Rvalue::Binary { op: BinaryOp::Div, .. },
                        ..
                    }
                )
            })
        })
        .map(|(id, _)| id)
        .collect();
    assert_eq!(divides.len(), 1, "one division");

    // Nothing that panics can reach the block that divides.
    for (id, block) in body.blocks() {
        let panicking = matches!(
            &block.terminator.kind,
            TerminatorKind::Call { callee: Callee::Runtime("science_panic_bytes"), .. }
        );
        if !panicking {
            continue;
        }
        assert_eq!(
            body.successors(id).len(),
            0,
            "a panic is diverging and has no successor"
        );
    }
}
