//! §10's stage 1 and its gate: `print("hello, world")` becomes a program that
//! prints `hello, world` and exits 0.
//!
//! **The decision.** This test runs the whole pipeline — lex, parse, resolve,
//! check, MIR, lower, LLVM, object, link — and then **runs the executable** and
//! asserts its stdout and its exit code.
//!
//! **The reason** is §10's own discipline, which is the reason the staging
//! exists at all: *"every stage below produces a program that runs and prints
//! something, and no stage is finished until an execution test asserts its
//! output and exit code."* Every check short of running the program passes on a
//! backend that emits a correct-looking module and a wrong one. Two of the three
//! defects this crate had were exactly that shape: `_S4main` wrote its `null`
//! into a private `alloca` instead of the caller's `sret` slot, which verifies,
//! links, and takes the error branch on whatever the stack held; and
//! `Reloc::Static` on Windows x64 emitted an `ADDR32` relocation the linker
//! rejects. Neither is visible in the IR.
//!
//! **The cost, and it is what makes this the only kind of test in the crate
//! with prerequisites.** It needs LLVM, a `clang` to drive the link, and
//! `target/<profile>/science_rt.lib` — which `cargo build -p science-rt`
//! produces and building `sciencec` does not, because Cargo builds a
//! dependency's `rlib` and not its `staticlib`. A `cargo test --workspace
//! --features llvm` builds every member, `science-rt` among them, so the archive
//! is there; a `cargo test -p science-codegen-llvm --features llvm` on a cold
//! tree may not have it, and the failure says so rather than reporting a linker
//! error.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

#[test]
fn hello_world_runs_and_exits_zero() {
    let lowered = lower("print(\"hello, world\")
");
    let dir = scratch("hello", "run");
    require_runtime();
    let built = lowered.build_at(&executable(&dir, "hello"), OptLevel::O2);

    let ran = run(&built);
    assert_eq!(ran.stdout, "hello, world
", "stderr was: {}", ran.stderr);
    assert_eq!(
        ran.status,
        Some(0),
        "`script-mode.md` §2.3: a script body that returns no error exits 0. stderr: {}",
        ran.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The IR, pinned at the four shapes that were wrong and are the ones a future
/// change would get wrong again.
///
/// **Not a golden file.** A whole-module snapshot of LLVM IR breaks on every
/// LLVM release for reasons that are not this crate's, and the version pin is
/// `codegen-and-linking.md` Decision 2's job rather than a test's. What is
/// pinned is four sentences, each of which was false at some point.
#[test]
fn the_module_says_what_stage_one_says_it_should() {
    let lowered = lower("print(\"hello, world\")
");
    let dir = scratch("hello", "ir");
    require_runtime();
    // **`-O0`, and the reason is a fact about `Built::ir` worth knowing.** That
    // field holds the module *after* Decision 33's pipeline has run: `build`
    // prints it once, after `emit_object_to`, because `--emit=llvm-ir` wants the
    // optimised form. At `-O2` the inliner folds `_S4main` into `main` and the
    // null test disappears entirely — correctly, since the value is a constant
    // `null` — so a test that pinned the emitter's output at the default level
    // would be pinning the optimiser's instead. `-O0` is the emitter's own text.
    let ir = lowered.build_at(&executable(&dir, "hello"), OptLevel::O0).ir;

    // Decision 15: the bytes are a `private unnamed_addr constant`, not
    // NUL-terminated, and the length travels beside the pointer.
    assert!(
        ir.contains("private unnamed_addr constant [12 x i8] c\"hello, world\""),
        "the literal is not Decision 15's global:
{ir}"
    );
    // §9.2's finding, on the symbol hello world calls first: the construction is
    // an `sret` call and not a structural return.
    assert!(
        ir.contains("@science_string_from_bytes(ptr") && ir.contains("sret({ ptr, i64, i64 })"),
        "`science_string_from_bytes` is not declared with an `sret` slot:
{ir}"
    );
    // The drop is not optional: the temporary `String` is owned by the call site
    // and nothing else frees it.
    assert!(ir.contains("@science_string_free("), "the temporary is leaked:
{ir}");
    // §3.4: the data pointer and **only** the data pointer. A `load { ptr, ptr }`
    // here is the bug `ExtInst::LoadNiche` exists to have stopped.
    assert!(ir.contains("icmp eq ptr"), "the null test is not against a bare pointer:
{ir}");
    assert!(
        !ir.contains("load { ptr, ptr }"),
        "the vtable word of a possibly-null `(any Error)?` was loaded, which §3.4 forbids          even on the path that tests for null:
{ir}"
    );
    // Decision 6, on every function without exception.
    assert!(ir.contains("nounwind"), "Decision 6's attribute is missing:
{ir}");
    assert!(
        !ir.contains("invoke ") && !ir.contains("landingpad"),
        "Decision 6: there is no unwinding anywhere in this language:
{ir}"
    );
    // Decision 35's own check, restated as a test rather than trusted to the
    // compiler's self-check.
    assert_eq!(science_codegen::target::first_fast_math_flag(&ir), None, "{ir}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The boundary is sharp, and the refusal names the construct.
///
/// A program past what this backend lowers is `SC0400` and not an internal
/// error. This is the half of `lower`'s contract that a user meets first.
#[test]
fn a_program_past_the_boundary_is_refused_by_name() {
    let lowered = lower("def helper() -> Int:
    return 1

print(\"hi\")
");
    let dir = scratch("hello", "refused");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a second function is not lowered");
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("helper"),
        "the refusal must name the construct, and it said: {}",
        first.message
    );
    let _ = std::fs::remove_dir_all(&dir);
}
