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

// --- generic aggregates ------------------------------------------------------
//
// A generic *function* is monomorphised above this crate: `science-mir`
// substitutes its body and what arrives is concrete. A generic *type* is not,
// and cannot be by the same route — `Pair[A, B]` is **declared** once, in `A`
// and `B`, and instantiating a body never rewrites a declaration.
//
// So its layout is computed by binding its parameters at each use and walking
// the declared field list with the binding in hand. `Lowerer::aggregate_env`
// is that, and it carries the binding down the walk rather than substituting,
// because substituting interns a `Ty` and this crate is handed a `&Types` on
// purpose. The tests below are what that buys and, in the last one, what it
// costs.

/// Two argument lists, two layouts, and the second is the one that would
/// silently share the first's.
///
/// `Pair[Int, F64]` is `{i64, double}` and `Pair[Int, Int]` is `{i64, i64}` —
/// different sizes and a different offset for the second field.
/// `science_codegen::layout::LayoutCache` is keyed by a struct's **name** and
/// answers `SC0404` when one name gets two layouts, so the name has to be a
/// function of the arguments. If it were not, one of these two would be read
/// at the other's offsets, and `p.second` would print garbage rather than
/// fail to build.
#[test]
fn a_generic_record_at_two_argument_lists_has_two_layouts() {
    let (out, _) = built(
        "pair",
        "type Pair[A, B]:\n\
         \x20   first: A\n\
         \x20   second: B\n\
         \n\
         def main():\n\
         \x20   let p: Pair[Int, F64] be Pair(first: 3, second: 4.5)\n\
         \x20   let q: Pair[Int, Int] be Pair(first: 10, second: 20)\n\
         \x20   print(f\"{p.first} {p.second} {q.first} {q.second}\")\n",
    );
    assert_eq!(out, "3 4.5 10 20\n");
}

/// **The drop path, which is where a wrong binding leaks or double-frees
/// rather than printing the wrong number.**
///
/// `Holder[String]` owns its field and `Holder[Int]` owns nothing. Both are
/// the one declaration `Holder[T]`, so a compiler that keyed drop glue on the
/// bare name would emit one function for both — and whichever arrived second
/// would get the other's glue: either a `String` never freed, or
/// `science_string_free` called on the integer `9`.
///
/// Running under `O2` with both in one program is the test. A double free
/// aborts and the exit-status assertion in [`built`] catches it.
#[test]
fn a_generic_record_drops_by_its_argument_and_not_by_its_name() {
    let (out, _) = built(
        "holder",
        "type Holder[T]:\n\
         \x20   value: T\n\
         \n\
         def main():\n\
         \x20   let owns: Holder[String] be Holder(value: \"cadena\")\n\
         \x20   let plain: Holder[Int] be Holder(value: 9)\n\
         \x20   print(f\"{owns.value} {plain.value}\")\n",
    );
    assert_eq!(out, "cadena 9\n");
}

/// A generic `choice`, whose parameters live on its **variants** rather than
/// on the type.
///
/// `science_types::items::Variant`'s own comment says why they are there: a
/// use writes `Just(5)` and never names `Maybe`. So the binding for a
/// `choice` is read off whichever variant the declaration table has an entry
/// for, and this is the test that it is read off any of them rather than off
/// a type-level list that does not exist.
#[test]
fn a_generic_choice_carries_its_parameters_on_its_variants() {
    let (out, _) = built(
        "maybe",
        "choice Maybe[T]:\n\
         \x20   Nothing\n\
         \x20   Just(T)\n\
         \n\
         def main():\n\
         \x20   let m: Maybe[Int] be Just(5)\n\
         \x20   let n: Maybe[String] be Just(\"texto\")\n\
         \x20   print(\"ambos\")\n",
    );
    assert_eq!(out, "ambos\n");
}

/// A method declared on a generic block, called on an instantiated receiver.
///
/// `Wrapper[T] has:` gives `Self` the type `Wrapper[T]`, which used to reach
/// `layout_of_ty` and be refused. `crates/science-codegen-llvm/tests/
/// methods.rs` carries the positive version of this as
/// `a_method_on_a_generic_block_runs`, which replaced the refusal it used to
/// pin; this is here so that the generic story is readable in one file.
#[test]
fn a_method_on_a_generic_type_runs() {
    let (out, _) = built(
        "method",
        "type Wrapper[T]:\n\
         \x20   inner: T\n\
         \n\
         Wrapper[T] has:\n\
         \x20   def get(self) -> Int:\n\
         \x20       1\n\
         \n\
         def main():\n\
         \x20   let w: Wrapper[Int] be Wrapper(inner: 1)\n\
         \x20   print(w.get())\n",
    );
    assert_eq!(out, "1\n");
}

/// **The limit that used to be a refusal, run as a program instead.**
///
/// `Nest[A]`'s field is `Holder[A]` — a compound mentioning the type's own
/// parameter. Binding it at a use means building the `Ty` for `Holder[Int]`
/// or `Holder[String]`, and building a `Ty` is interning, which
/// `science-codegen-llvm` cannot do — it is handed a `&Types` precisely so
/// that it cannot invent one. `Lowerer::member_ty` used to refuse this by
/// name, under a test — `a_generic_field_mentioning_a_parameter_is_refused_by_name`,
/// replaced here — whose own comment said what would retire it: *"the repair
/// is for the substitution to happen above Decision 42's line, where `&mut
/// Types` lives … this is the test that fails the day something does, which
/// is when it should be replaced by the program running."*
///
/// The repair is `science_codegen::mono::Mono::intern_aggregate_fields`:
/// every generic record or `choice` a monomorphised body mentions is
/// substituted once, above the line, and the backend looks the result up
/// instead of trying to build it. Two instantiations — `Nest[Int]` and
/// `Nest[String]` — are asserted here rather than one, because the interning
/// is keyed by `(DefId, arguments)` and a table that collapsed the two into
/// one entry would print `9 9` or `hondo hondo` instead of failing to build
/// at all, which is a worse bug than the refusal this replaces.
#[test]
fn a_generic_field_mentioning_a_parameter_runs() {
    let (out, _) = built(
        "member_param",
        "type Holder[T]:\n\
         \x20   value: T\n\
         \n\
         type Nest[A]:\n\
         \x20   inner: Holder[A]\n\
         \n\
         def main():\n\
         \x20   let deep: Nest[String] be Nest(inner: Holder(value: \"hondo\"))\n\
         \x20   let shallow: Nest[Int] be Nest(inner: Holder(value: 9))\n\
         \x20   print(f\"{deep.inner.value} {shallow.inner.value}\")\n",
    );
    assert_eq!(out, "hondo 9\n");
}

/// **`T.clone()` at a builtin scalar, which `examples/07_generics.science`
/// calls and which used to refuse.**
///
/// `duplicate[T: Clone + Eq](value: &T)` calls `value.clone()` twice.
/// Monomorphised at `T = Int`, `Clone::clone`'s receiver is a concrete `Int`
/// — `Lowerer::declaring_interface` says the call is through `Clone`, and
/// `Lowerer::prelude_method`'s table has a row for `("String", "clone", ...)`
/// and none for `Int`, because `science-rt` has no `science_int_clone` and
/// never will: an `Int` owns nothing for a runtime call to duplicate. Lacking
/// a row, the call used to fall to `Lowerer::declaring_interface`'s dispatch
/// arm, which needs a vtable slot nothing here builds — `duplicate(5i64)`
/// refused with *"a value whose type is still a type parameter, which
/// nothing has monomorphised"*, a message about monomorphisation for a
/// program monomorphisation had already finished with. This is the case
/// `Lowerer::trivial_scalar_clone` closes: a bitwise copy of the receiver,
/// with no call and no vtable.
///
/// Run at both `Int` and `String` in the one program, so that a fix which
/// only widened the `Int` case rather than genuinely falling through to
/// `prelude_method` first would still be caught: `String`'s `.clone()` must
/// keep going through `science_string_clone` and not through this new path.
#[test]
fn clone_on_a_bounded_type_parameter_runs_at_a_builtin_scalar() {
    let (out, _) = built(
        "duplicate",
        "type Pair[A, B]:\n\
         \x20   first: A\n\
         \x20   second: B\n\
         \n\
         def duplicate[T: Clone + Eq](value: &T) -> Pair[T, T]:\n\
         \x20   Pair(first: value.clone(), second: value.clone())\n\
         \n\
         def main():\n\
         \x20   let by_int be duplicate(5i64)\n\
         \x20   let by_string be duplicate(\"hi\")\n\
         \x20   print(f\"{by_int.first} {by_int.second} {by_string.first} {by_string.second}\")\n",
    );
    assert_eq!(out, "5 5 hi hi\n");
}
