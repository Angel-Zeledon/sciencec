//! A closure, called directly, with nothing captured — the boundary
//! `hello.rs`'s `a_program_past_the_boundary_is_refused_by_name` and
//! `stage_two_and_three.rs`'s `nothing_past_the_boundary_produces_an_executable`
//! used to sit at, and no longer does. Their own doc comments narrate why; this
//! file is the execution test §10's own discipline asks for whenever a
//! construct crosses that line — *"every stage below produces a program that
//! runs and prints something, and no stage is finished until an execution test
//! asserts its output and exit code."*
//!
//! **What moved.** `science-mir`'s `lower.rs` §8.5 gives a capture-free
//! closure's body its own `Body`, keyed on the closure's `param`;
//! `science_codegen::mono`'s `Mono::walk_rvalue` enqueues it exactly as it
//! enqueues any other address-taken function; and here, `cg_ty_in`'s
//! `TyKind::Closure` arm gives every closure value the layout that is honest
//! for that case, `CgTy::Ptr(PtrKind::Fn)`, `Lowerer::lower_rvalue`'s
//! `Rvalue::Closure` arm materialises it as `Operand::GlobalAddr` of the
//! closure's own symbol, and `Lowerer::lower_indirect_closure_call` calls
//! through it exactly as `Lowerer::lower_dispatch` calls through a vtable slot
//! — a value and a signature, with no symbol either.
//!
//! **What did not.** A closure that captures something is still refused, by
//! name, at the point it would be built: there is no concrete layout for
//! `{ fn ptr, captures }` and no aggregate this crate can construct one at.
//! `a_closure_that_captures_something_is_still_refused_by_name` is that
//! refusal, run.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// The corpus form this whole effort was scoped around, built and run.
#[test]
fn a_capture_free_closure_is_called_and_prints_its_answer() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    print(apply(x giving x + 1))
",
    );
    let dir = scratch("closures", "apply");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "apply"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "2\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Two closures in one module, both naming their parameter `x` — which two
/// unnamed `giving`s would too, since the implicit form always binds `each`.
/// Before `path_of`'s `DefKind::Param` arm, both would mangle to the same
/// symbol and one body would silently replace the other in the object file;
/// this is that collision, built and run rather than asserted about a string.
#[test]
fn two_closures_with_the_same_parameter_name_are_two_symbols() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    print(apply(x giving x + 1))
    print(apply(x giving x * 10))
",
    );
    let dir = scratch("closures", "two_closures_named_x");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "two_closures_named_x"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "2\n10\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A closure called a hundred thousand times, to catch what a single call
/// cannot: a function pointer materialised wrong, or a call built with the
/// wrong calling convention, both pass once and fail (or corrupt) on repeat.
#[test]
fn a_capture_free_closure_runs_a_hundred_thousand_times() {
    let lowered = lower(
        "def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    let mutable total be 0
    for i in 0..100000:
        total be total + apply(x giving x + 1)
    print(total)
",
    );
    let dir = scratch("closures", "loop");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "loop"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "200000\n", "stderr was: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// §8.5's hole, unmoved: a closure that captures something still has no
/// concrete layout, and this is that refusal, produced by an actual build
/// rather than asserted about the IR.
#[test]
fn a_closure_that_captures_something_is_still_refused_by_name() {
    let lowered = lower(
        "def sink(f: (Int) -> Int) -> Int:
    1

def go(n: Int) -> Int:
    sink(item giving item + n)

def main():
    print(go(1))
",
    );
    let dir = scratch("closures", "captures");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a closure that captures something is not lowered");
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("closure") && first.message.contains("captures"),
        "the refusal must name the construct, and it said: {}",
        first.message
    );
    let _ = std::fs::remove_dir_all(&dir);
}
