//! One generic function, several functions in the image.
//!
//! # What this file is evidence of
//!
//! `codegen-and-linking.md` Decision 42 puts the monomorphisation walk above
//! this crate, and `science_codegen::mono` has been that walk — 1753 lines and
//! forty tests — since long before anything called it. Two things were missing
//! between the walk and a running program, and this file is the proof that
//! both are now there:
//!
//! 1. **The bodies.** `science_mir::instantiate` applies an instance's
//!    substitution to every type its body mentions, so what reaches the
//!    backend is concrete MIR rather than a body with a `T` in it. The refusal
//!    that used to stand in `science_signature` — *"a call to the generic
//!    function `{name}`, which nothing has monomorphised"* — is gone because
//!    there is nothing left for it to refuse.
//! 2. **The call sites.** A `science_mir::mir::Callee::Def` names a
//!    *definition*, and after the walk one definition is several functions. So
//!    `MonoSet` now carries which instance each call site calls, keyed by the
//!    **calling instance's** symbol, and `Lowerer::symbol_for_call` is what
//!    reads it.
//!
//! # Why the assertions are on stdout and on `nm`
//!
//! A test that only checked the output would pass on a compiler that emitted
//! one `identity` and called it from both sites — for `identity` specifically,
//! the wrong answer and the right one agree, because the function does
//! nothing. That is exactly the bug monomorphisation exists to prevent and it
//! would be invisible. So [`two_instantiations_are_two_functions`] reads the
//! symbol table: two symbols, or the claim in this file's title is false.
//!
//! The reverse assertion is [`one_instantiation_is_one_function`]: a generic
//! called at one type is *one* symbol, because a walk that emitted an instance
//! per call site rather than per distinct argument list would pass every
//! output assertion here and quietly double the image.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build, run, and return `(stdout, the symbols in the image)`.
///
/// The symbols come from `nm` rather than from the emitted IR, because what is
/// under test is what ended up in the **executable**: an instance the lowerer
/// emitted and the linker then dropped is not a function the program has.
fn built(name: &str, source: &str) -> (String, Vec<String>) {
    let dir = scratch("generics", name);
    require_runtime();
    let path = executable(&dir, name);
    // `O2` like every other execution test here. It is also the setting under
    // which a duplicate instance is most likely to be folded away, so the
    // symbol assertions below are being made against the harder case.
    let image = lower(source).build_at(&path, OptLevel::O2);
    let ran = run(&image);
    let symbols = std::process::Command::new("nm")
        .arg(&path)
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|line| line.split_whitespace().last())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    (ran.stdout, symbols)
}

/// How many symbols in the image mention `name`.
///
/// A substring test rather than an exact one: Decision 16's mangling wraps the
/// name in a length prefix and an argument encoding, and Mach-O puts a leading
/// underscore on top of that. Pinning the exact spelling here would make this
/// file a second copy of `tests/symbols.rs`, which is where the mangling
/// itself is under test.
fn mentioning(symbols: &[String], name: &str) -> usize {
    symbols.iter().filter(|symbol| symbol.contains(name)).count()
}

/// **The whole claim, in one program.**
///
/// `identity` is called at `Int` and at `F64`. Those are two different
/// machine types — an `i64` and a `double`, passed in different register
/// classes — so one function genuinely cannot serve both, and a compiler that
/// emitted one would either return the wrong bits or fail the verifier.
#[test]
fn two_instantiations_are_two_functions() {
    let (out, symbols) = built(
        "two",
        "def identity[T](x: T) -> T:\n\
         \x20   x\n\
         \n\
         def main():\n\
         \x20   let a: Int be identity(7)\n\
         \x20   let b: F64 be identity(2.5)\n\
         \x20   print(f\"{a} {b}\")\n",
    );
    assert_eq!(out, "7 2.5\n");
    assert_eq!(
        mentioning(&symbols, "identity"),
        2,
        "`identity` at two types must be two functions in the image, not one: {symbols:?}"
    );
}

/// The control, and it is the assertion that the walk keys on the **argument
/// list** and not on the call site.
///
/// Three calls, one type. A walk that enqueued an instance per call would emit
/// three `identity`s, every one of them identical, and no output assertion
/// anywhere would notice. `science_codegen::mono`'s set is a map keyed by the
/// mangled symbol, so the three collapse by construction — this is that
/// property observed in an executable rather than in the set.
#[test]
fn one_instantiation_is_one_function() {
    let (out, symbols) = built(
        "one",
        "def identity[T](x: T) -> T:\n\
         \x20   x\n\
         \n\
         def main():\n\
         \x20   let a: Int be identity(1)\n\
         \x20   let b: Int be identity(2)\n\
         \x20   let c: Int be identity(3)\n\
         \x20   print(f\"{a} {b} {c}\")\n",
    );
    assert_eq!(out, "1 2 3\n");
    assert_eq!(
        mentioning(&symbols, "identity"),
        1,
        "three calls at one type are one function: {symbols:?}"
    );
}

/// **A generic body calling a generic callee**, which is the case the call map
/// exists for and the one nothing else here exercises.
///
/// `twice[Int]`'s body contains two calls to `identity`, and the MIR they were
/// lowered from is `twice`'s *generic* body — its `Callee::Def` says
/// `identity` and nothing more. What decides that those two calls go to
/// `identity[Int]` rather than to `identity[F64]` is `MonoSet::callee_at`,
/// asked with the **caller's instance symbol**: from `twice[F64]` the very same
/// block would answer `identity[F64]`.
///
/// So this is the test that would fail if `symbol_for_call` fell back to
/// `by_def`, which holds one symbol per definition and would hand both
/// instantiations of `twice` whichever `identity` was entered last.
#[test]
fn a_generic_calling_a_generic_reaches_the_right_instance() {
    let (out, symbols) = built(
        "nested",
        "def identity[T](x: T) -> T:\n\
         \x20   x\n\
         \n\
         def twice[T](x: T) -> T:\n\
         \x20   identity(identity(x))\n\
         \n\
         def main():\n\
         \x20   let a: Int be twice(41)\n\
         \x20   let b: F64 be twice(0.5)\n\
         \x20   print(f\"{a} {b}\")\n",
    );
    assert_eq!(out, "41 0.5\n");
    assert_eq!(
        mentioning(&symbols, "twice"),
        2,
        "`twice` at two types must be two functions: {symbols:?}"
    );
    assert_eq!(
        mentioning(&symbols, "identity"),
        2,
        "and each must have reached its own `identity`, not shared one: {symbols:?}"
    );
}
