//! A `String` comparison reads; `lower.rs`'s §5 exception for the six
//! comparison operators.
//!
//! `self.operand`'s ordinary rule — [`Builder::read`], §5 — spells any
//! non-`Copy` type as [`Operand::Move`], which is correct for an assignment,
//! a call that takes ownership, or a `return`, and wrong for a comparison: `s
//! is ""` produces a fresh `Bool` and leaves `s` exactly where it was.
//! Without the fix, `science-codegen-llvm`'s `string_pointer` refuses the
//! whole-value `Move` outright (`SC0400`, *"a comparison that MIR spells as a
//! `move` of a whole `String`"*) rather than compiling a program that would
//! otherwise silently leak the buffer `crate::drops` no longer frees, because
//! `crate::moves` reads a `Move` as consuming the binding and deletes its
//! `Drop`.
//!
//! This crate cannot run the program `science-codegen-llvm` would refuse, so
//! the assertion below is at MIR's own level, the one the fix actually
//! changes: every place operand of a comparison is `Operand::Copy`, not
//! `Operand::Move`. `crates/sciencec/tests/cli.rs` and the example corpus
//! (`examples/04_enums.science`) are what pin the end-to-end behaviour this
//! unblocks.

mod support;

use science_mir::mir::{Operand, Rvalue, StatementKind};
use support::lower;

/// Every operand of every `Rvalue::Binary` in a body, in block order.
fn binary_operands(lowered: &support::Lowered, name: &str) -> Vec<(&'static str, &'static str)> {
    let body = lowered.body(name);
    let mut out = Vec::new();
    let describe = |operand: &Operand| -> &'static str {
        match operand {
            Operand::Copy(_) => "copy",
            Operand::Move(_) => "move",
            Operand::Const(_) => "const",
        }
    };
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            if let StatementKind::Assign { rvalue: Rvalue::Binary { lhs, rhs, .. }, .. } =
                &statement.kind
            {
                out.push((describe(lhs), describe(rhs)));
            }
        }
    }
    out
}

/// **The regression this file exists to catch.** Before the fix, this
/// program lowered `s is ""` with `lhs: Operand::Move(_)` — the binding's own
/// last read of itself, spelled as if the comparison consumed it — and
/// `science-codegen-llvm` refused to build it rather than compile a leak.
#[test]
fn a_string_equality_reads_both_operands_rather_than_moving_them() {
    let lowered = lower("def f(s: String) -> Bool:\n    s is \"\"\n");
    // The right-hand side is a literal, which was never a `Move` — it is
    // `Operand::Const` on both sides of the fix. `s`, the left-hand side and
    // the binding whose `Drop` is at stake, is the operand the fix changes.
    assert_eq!(
        binary_operands(&lowered, "f"),
        vec![("copy", "const")],
        "`s is \"\"` must read `s`, not move it: {}",
        lowered.dump("f")
    );
}

/// The same operator, both operands owned bindings rather than one literal —
/// so neither side can be dismissed as "well, a literal was never going to be
/// a `Move` anyway".
#[test]
fn a_string_equality_between_two_bindings_reads_both() {
    let lowered = lower("def f(a: String, b: String) -> Bool:\n    a is b\n");
    assert_eq!(
        binary_operands(&lowered, "f"),
        vec![("copy", "copy")],
        "`a is b` must read both operands: {}",
        lowered.dump("f")
    );
}

/// `is not`, `Ne`'s own arm of the same fix.
#[test]
fn a_string_inequality_reads_rather_than_moves() {
    let lowered = lower("def f(s: String) -> Bool:\n    s is not \"\"\n");
    assert_eq!(
        binary_operands(&lowered, "f"),
        vec![("copy", "const")],
        "`s is not \"\"` must read `s`, not move it: {}",
        lowered.dump("f")
    );
}

/// The exception is for the *operator*, not the type: an ordinary move of a
/// `String` — binding it to a fresh `let` — is untouched, still a whole-value
/// `Move`, so the fix does not quietly turn every `String` `Copy`.
#[test]
fn an_ordinary_string_rebinding_is_still_a_move() {
    let lowered = lower("def f(s: String) -> String:\n    let t be s\n    t\n");
    let body = lowered.body("f");
    let moved = body.blocks().any(|(_, block)| {
        block.statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign { rvalue: Rvalue::Use(Operand::Move(_)), .. }
            )
        })
    });
    assert!(moved, "`let t be s` must still move `s`: {}", lowered.dump("f"));
}
