//! `f"…"` from source, **built, linked, run**, with stdout and the exit code
//! both asserted.
//!
//! # What this file closes
//!
//! `tests/formatting_boundary.rs` opened with *"there is no integer-to-string
//! entry point in the runtime … so there is no way for any program to print a
//! number"*, then with *"there is one; what there is not is a MIR lowering for
//! `f"{n}"`"*. There is one. So the sentence that file's note ends on — *"the
//! test to write then is a source-level one that replaces this file's first
//! test"* — is this file, and the one it replaces is the *first* test, not the
//! whole file: `formatting_boundary.rs` still drives the seven entry points at
//! values chosen so that a width error changes them, and no source-level
//! program can pass `u64::MAX` and `2^53 + 1` through the same call in one
//! line. The two are the shape and the extremes.
//!
//! # Why every test here runs the program
//!
//! §10's discipline, and this crate's §3 is the evidence. The failure this file
//! was written around is finding 18, and it is worth reading before changing
//! anything in it:
//!
//! > An `f"…"` hole whose type is `borrowed String` is a **pointer**, and
//! > `science_string_push_str`'s second parameter is a **pointer**. With
//! > `science-mir`'s §4 dereference removed from the hole's borrow, the
//! > argument becomes the address of this frame's parameter slot rather than
//! > the address of the caller's `String` — a pointer to a pointer, where a
//! > `{ ptr, len, cap }` was wanted. **It compiles, it links, and
//! > `LLVMVerifyModule` passes it**, because opaque pointers make the two
//! > indistinguishable in the IR. What it does is abort with
//! > `panic: science-rt: out of memory`, from the byte pattern of a pointer
//! > read as a length.
//!
//! That is the third time this crate has met a store or a load that is wrong by
//! one indirection and verifies. An IR assertion would not have found it: the
//! IR is `call void @science_string_push_str(ptr %a, ptr %b)` either way.
//!
//! # What is asserted, and in what order
//!
//! [`the_acceptance_case`] first, because it is the line the whole feature is
//! for. Then the fragment shapes, then every entry point the runtime has, then
//! the two ownership facts — a hole is still usable after the interpolation
//! (§1.6), and a bound `f"…"` is freed exactly once — and last the refusal for
//! a type with no renderer.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build and run one program, and give back what it printed.
///
/// `-O2` rather than `-O0`, for the reason `tests/past_stage_three.rs` gives:
/// the optimiser is where a slot that is written and never read, or a load of
/// something that was never stored, stops being invisible. An interpolation is
/// mostly `alloca`s and addresses, which is `SROA`'s subject exactly.
fn output(name: &str, source: &str) -> harness::Ran {
    let dir = scratch("interp", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

/// What a program printed, having exited 0 with nothing on stderr.
fn prints(name: &str, source: &str) -> String {
    let ran = output(name, source);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The IR of a program that built, for the two assertions about shape.
fn ir(name: &str, source: &str) -> String {
    let dir = scratch("interp", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// **The line.** The three-program acceptance case, printed by a compiled
/// executable that exits 0.
///
/// Until `science-mir`'s `ExprKind::FString` arm was written, this program
/// checked clean and `sciencec build` refused it with *"an expression the front
/// end replaced with a hole"*. Nothing else in the language had that shape: the
/// front end was complete, the runtime entry points existed and had been run,
/// and the two ends were not joined.
#[test]
fn the_acceptance_case() {
    let source = "let n be 42\nlet x be 0.5\nprint(f\"n es {n} y x es {x}\")\n";
    assert_eq!(prints("acceptance", source), "n es 42 y x es 0.5\n");
}

/// **Every shape a fragment list can have.** One program, so that a partial
/// failure names which shape broke by which line is missing.
///
/// `f""` is the one worth stating: it has no fragments at all and still builds
/// an accumulator, because the expression's type is `String` and `print` needs
/// one to point at. It prints an empty line.
#[test]
fn the_shapes_of_a_fragment_list() {
    let source = "let a be 1\nlet b be 2\n\
                  print(f\"solo texto\")\n\
                  print(f\"{a} al final\")\n\
                  print(f\"al inicio {a}\")\n\
                  print(f\"{a}{b}\")\n\
                  print(f\"\")\n\
                  print(f\"{a + b}\")\n";
    assert_eq!(
        prints("shapes", source),
        "solo texto\n1 al final\nal inicio 1\n12\n\n3\n",
        "one of the fragment shapes rendered wrong, or not at all"
    );
}

/// **All seven entry points, from source.**
///
/// The values are ordinary ones — `tests/formatting_boundary.rs` owns the
/// extremes — and what this adds is that `science-mir` picked the right entry
/// point for each type rather than that the entry point renders. `1u64` and
/// `42` are the same digits through two different symbols, and the test that
/// would catch them being swapped is that file's `u64::MAX`.
#[test]
fn every_entry_point_the_runtime_has() {
    let source = "let i be 42\n\
                  let u be 18446744073709551615u64\n\
                  let d be 2.5\n\
                  let f be 0.5f32\n\
                  let b be true\n\
                  let c be '!'\n\
                  let s be \"texto\"\n\
                  print(f\"i={i} u={u} d={d} f={f} b={b} c={c} s={s}\")\n";
    assert_eq!(
        prints("types", source),
        "i=42 u=18446744073709551615 d=2.5 f=0.5 b=true c=! s=texto\n"
    );
}

/// **§1.6, run.** The `String` the interpolation read is still the program's
/// afterwards.
///
/// This is the decision's own reason, as a program: *"a debugging `print` that
/// moves the value you were about to use is a diagnostic in the `SC0300` range
/// caused by a line the user added to understand a different problem"*. If the
/// hole were a move, `print(s)` on the last line would be a use-after-move —
/// and because `print` also *frees* what it prints, the failure past the
/// checker is a double free rather than a diagnostic.
///
/// The interpolation reads `s` twice on purpose: two shared loans of one place
/// at once is what §1.6's borrow makes legal and what a move would not.
#[test]
fn a_hole_is_still_the_programs_afterwards() {
    let source = "let s be \"uno\"\nlet t be f\"{s} y {s}\"\nprint(t)\nprint(s)\n";
    assert_eq!(prints("borrowed", source), "uno y uno\nuno\n");
}

/// **A bound `f"…"` is freed exactly once, and by the drop MIR emitted.**
///
/// The accumulator is a `String` and a `String` owns memory. Two shapes, two
/// owners: bound and never printed, it is released by `TerminatorKind::Drop`;
/// bound and printed, `print` moves it, drop elaboration deletes the drop, and
/// [`crate::lower::Lowerer::lower_print`]'s own call site frees it. Either way
/// the IR calls `science_string_free` once per accumulator, and the count is
/// what a double free or a leak changes.
#[test]
fn a_bound_interpolation_is_freed_once() {
    let dropped = ir("freed", "let s be f\"hola\"\nlet n be 1\n");
    assert_eq!(
        dropped.matches("@science_string_free(").count() - 1,
        1,
        "a bound and unprinted `f\"…\"` should be freed once:\n{dropped}"
    );

    let printed = ir("freed_printed", "print(f\"hola\")\n");
    assert_eq!(
        printed.matches("@science_string_free(").count() - 1,
        1,
        "an `f\"…\"` passed to `print` should be freed once, at the call site:\n{printed}"
    );

    // And both run, which is the half a count cannot see: a free of a slot
    // nothing built is a crash and not a different count.
    assert_eq!(prints("freed_runs", "let s be f\"hola\"\nprint(s)\n"), "hola\n");
}

/// **A `borrowed` hole renders what it points at.** Finding 18's program, the
/// right way round.
///
/// Both parameters are references and neither is what the entry point wants:
/// `science_string_push_str` wants the `ScienceString`, and
/// `science_string_push_i64` wants the `i64`. `science-mir`'s §4 dereference is
/// what turns one into the other, and this is the program that tells you
/// whether it fired — by printing `hola mundo x3`, or by aborting inside the
/// runtime, which is what it did when it did not.
#[test]
fn a_borrowed_hole_renders_its_referent() {
    let source = "def saluda(quien: borrowed String, veces: borrowed Int) -> Int:\n\
                  \x20   print(f\"hola {quien} x{veces}\")\n\
                  \x20   return 0\n\
                  \n\
                  let nombre be \"mundo\"\n\
                  let n be 3\n\
                  let r be saluda(nombre, n)\n\
                  print(nombre)\n";
    assert_eq!(prints("borrowed_param", source), "hola mundo x3\nmundo\n");
}

/// **The accumulator reaches every push as one pointer, and the pushes are in
/// source order.**
///
/// The cheaper reading of the same fact the tests above establish by running,
/// and the one that says *where* it went wrong: a `getelementptr` in this
/// sequence would mean a projection nobody asked for, and a second
/// `science_string_with_capacity` would mean a fragment built its own
/// accumulator.
///
/// **§1.7's capacity is asserted here as the constant in the IR**, which is the
/// third of the three places that number is pinned: `science-mir`'s
/// `tests/fstring.rs` asserts what the compiler computes, `science-rt`'s
/// `tests/capacity.rs` measures what a capacity buys, and this asserts the
/// number survives the trip through `RUNTIME`'s `usize` parameter into the
/// emitted call.
#[test]
fn the_sequence_in_the_ir_is_one_accumulator_and_one_call_per_fragment() {
    let text = ir("sequence", "let n be 42\nprint(f\"a{n}b\")\n");
    assert_eq!(text.matches("@science_string_with_capacity(").count() - 1, 1, "{text}");
    assert_eq!(
        text.matches("@science_string_new(").count(),
        0,
        "the accumulator is built with a capacity now, not empty:\n{text}"
    );
    // `"a"` and `"b"` are one byte each and an `Int` hole estimates twenty.
    // The call carries the hidden `sret` slot first, so the capacity is the
    // last argument rather than the only one.
    assert!(
        text.contains("@science_string_with_capacity(ptr sret")
            && text.contains("%l3, i64 22)"),
        "§1.7's pre-computed capacity is not in the call:\n{text}"
    );
    assert_eq!(text.matches("@science_string_push_bytes(").count() - 1, 2, "{text}");
    assert_eq!(text.matches("@science_string_push_i64(").count() - 1, 1, "{text}");
    let bytes = text.find("@science_string_push_bytes(ptr").expect("the first text run");
    let number = text.find("@science_string_push_i64(ptr").expect("the hole");
    assert!(bytes < number, "the fragments are out of source order:\n{text}");
}

/// **A hole whose type has no renderer is refused by name, with the type in
/// the message.**
///
/// **This used to be two, and one of them is gone.** An `I32` was a *width*
/// the runtime had no entry point for — its own note says *"`I8`…`I64` are
/// sign-extended by codegen before the call"* and no phase sign-extended
/// anything — and it now reaches `science_string_push_i64` through a cast;
/// [`a_narrow_integer_hole_renders_its_own_value`] is the program.
///
/// What is left is the refusal no cast can close: a user record is §3.1's
/// `Formatter`, which no note specifies and no prelude declares.
///
/// **It checks clean**, which is the point of refusing here rather than
/// guessing: `science-types`'s own note says *"the interpolation of a user type
/// is therefore accepted here and refused by codegen, which is a worse place to
/// find out"*, and the least this crate can do about that is say which type.
#[test]
fn a_hole_with_no_renderer_is_refused_and_the_type_is_named() {
    let (name, ty) = ("record", "Punto");
    let source = "type Punto:\n\x20   x: Int\n\nPunto implements Display\n\n\
                  let p be Punto(x: 1)\nprint(f\"p={p}\")\n";
    let dir = scratch("interp", name);
    let refused = lower(source)
        .try_build(&executable(&dir, name), OptLevel::O0)
        .err()
        .unwrap_or_else(|| panic!("`{ty}` in a hole should not have built"));
    let _ = std::fs::remove_dir_all(&dir);
    let message = refused
        .iter()
        .map(|diagnostic| format!("{} {}", diagnostic.code, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(message.contains("SC0400"), "the wrong code:\n{message}");
    assert!(
        message.contains(&format!("hole of type `{ty}`")),
        "the refusal does not name `{ty}`:\n{message}"
    );
}

/// **A narrow integer hole renders its own value, at values where widening it
/// the wrong way is visible.**
///
/// This is `science-mir`'s §7 item 10 discharged and run.
/// `science_string_push_i64`'s note has always said *"`I8`…`I64` are
/// sign-extended by codegen before the call"*; until `Rvalue::Cast` had a
/// lowering nothing did it, and the entry point was refused rather than fed an
/// `i32` in an `i64` parameter.
///
/// **Every value here is chosen so that a wrong extension changes it.**
/// `-1i8` zero-extended is `255`; `-1i16` is `65535`; `-1i32` is `4294967295`;
/// and `255u8` sign-extended is `-1`. A test on `7i32` would pass with the two
/// instructions swapped, which is exactly the bug this pair of entry points
/// exists to make impossible.
#[test]
fn a_narrow_integer_hole_renders_its_own_value() {
    let source = "let a be -1i8\nlet b be -1i16\nlet c be -1i32\n\
                  let d be 255u8\nlet e be 65535u16\nlet f be 4294967295u32\n\
                  print(f\"{a} {b} {c} {d} {e} {f}\")\n";
    assert_eq!(
        prints("narrow", source),
        "-1 -1 -1 255 65535 4294967295\n",
        "a narrow hole was widened with the wrong signedness, or not at all"
    );
}
