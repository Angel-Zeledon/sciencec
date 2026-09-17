//! §5.1's `as`, at every boundary the language has — each one a program that is
//! **built, linked, run**, with stdout and the exit code asserted.
//!
//! # Why every value here is an extreme
//!
//! A cast is the construct where *"it compiles and it is the wrong number"* is
//! the normal failure rather than the exotic one. LLVM integers carry no sign,
//! so `sext` and `zext` produce the same type from the same operand and differ
//! only in the bits they write; `fptosi` and `fptoui` likewise; `sitofp` and
//! `uitofp` likewise. **Every one of those six swaps verifies.** So a test that
//! casts `1` proves nothing at all: `1i32 as I64` is `1` through either
//! extension, and a compiler that had them backwards would pass it.
//!
//! The values below are chosen so that each swap changes the printed text:
//!
//! | Value | What a wrong answer looks like |
//! |---|---|
//! | `-1i32 as I64` | `4294967295` if the extension is unsigned |
//! | `4294967295u32 as I64` | `-1` if it is signed |
//! | `u64::MAX as I64` | `18446744073709551615` if the bits are re-read rather than kept |
//! | `u64::MAX as F64` | `-1` if the conversion is `sitofp` |
//! | `2^53 + 1 as F64` | `9007199254740993` if `F64` had that many bits, which it does not |
//! | `-1i32 as U8` | `0` if a narrowing cast saturated instead of truncating |
//! | `1e300 as I64` | undefined — a bare `fptosi` is `poison` here, which is the failure §5.1's *"saturates"* exists to name |
//! | `0.0 / 0.0 as I64` | anything at all, unless NaN becomes zero |
//!
//! # The three that are not arithmetic
//!
//! `Char`, `Bool`, and the refusals. A `Char` is a `u32` holding a Unicode
//! scalar value, so widening *out* of one is total and narrowing *into* one is
//! not; a `Bool` is a byte with two valid values out of 256, so widening out is
//! total and narrowing in is not. [`what_no_cast_may_do`] is the list, and its
//! note in [`crate::lower::Lowerer::lower_cast`] is the argument for each row.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build and run one program, and give back what it printed.
///
/// `-O2`, for `tests/interpolation.rs`'s reason: the optimiser is where a
/// conversion that produced `poison` stops being a number that happens to look
/// plausible and starts being whatever constant folding decided. §5.1's
/// saturation claim is only worth testing at an optimisation level that would
/// have exploited its absence.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("casts", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Why a program was refused, as `SC0400`'s text.
fn refusal(name: &str, source: &str) -> String {
    let dir = scratch("casts", name);
    let output = executable(&dir, name);
    let diagnostics = lower(source)
        .try_build(&output, OptLevel::O0)
        .err()
        .unwrap_or_else(|| panic!("`{name}` built, and this test exists because it must not"));
    assert!(!output.is_file(), "a refused build left an executable behind");
    let _ = std::fs::remove_dir_all(&dir);
    diagnostics
        .iter()
        .map(|diagnostic| format!("{} {}", diagnostic.message, diagnostic.notes.join(" ")))
        .collect::<Vec<_>>()
        .join("\n")
}

// --- integer to integer ---------------------------------------------------

/// **Widening: the extension's signedness is the source's.**
///
/// The first two lines are the same thirty-two bits and must print different
/// numbers. If they print the same one, the compiler is reading the
/// destination's signedness — which is what a backend that had not been given
/// `Rvalue::Cast`'s `from` would have to do.
#[test]
fn a_widening_cast_extends_by_the_sources_signedness() {
    let source = "let a be -1i32\nlet b be 4294967295u32\n\
                  let c be -1i8\nlet d be 200u8\n\
                  print(f\"{a as I64} {b as I64} {c as I64} {d as I64}\")\n";
    assert_eq!(
        prints("widen", source),
        "-1 4294967295 -1 200\n",
        "a widening cast used the wrong extension"
    );
}

/// **Narrowing truncates, in two's complement, silently.**
///
/// This is the decision §5.1 did not take and
/// [`crate::lower::Lowerer::lower_cast`] does: `300 as U8` is `44` and not a
/// panic, not `255`, and not a diagnostic. `-1i32 as U8` is `255` and not `0`,
/// which is what a saturating rule would have given and is the reason
/// saturation was rejected for integers — the author who writes that cast is
/// reinterpreting a bit pattern, and turning it into `0` would be answering a
/// different question.
#[test]
fn a_narrowing_cast_truncates_and_says_nothing() {
    let source = "let a be 300\nlet b be -1i32\nlet c be 65535\nlet d be -129i32\n\
                  print(f\"{a as U8} {b as U8} {c as U8} {d as I8}\")\n";
    assert_eq!(
        prints("narrow", source),
        "44 255 255 127\n",
        "a narrowing cast is not two's-complement truncation"
    );
}

/// **Same width is no instruction, and the bits are kept.**
///
/// `u64::MAX as I64` is `-1`: the eight bytes do not move and only the name
/// changes. A compiler that emitted a conversion here — any conversion — would
/// either fail the verifier or, for `zext`/`trunc` of equal widths, be an
/// instruction LLVM rejects outright.
#[test]
fn a_same_width_cast_keeps_the_bits() {
    let source = "let a be 18446744073709551615u64\nlet b be -1\n\
                  print(f\"{a as I64} {b as U64}\")\n";
    assert_eq!(prints("same-width", source), "-1 18446744073709551615\n");
}

// --- integer to float and back --------------------------------------------

/// **Integer to float: `sitofp` against `uitofp`, and the rounding §5.1 admits.**
///
/// `u64::MAX` through the signed instruction is `-1`; through the unsigned one
/// it is about `1.8e19`. And `2^53 + 1` is the value an `F64` does not have —
/// the mantissa runs out one bit short — so it renders as `2^53`, which is
/// §5.1's *"including where it loses precision"* as an observable number rather
/// than as a sentence.
#[test]
fn an_integer_to_float_cast_rounds_and_reads_the_sources_sign() {
    let source = "let a be 18446744073709551615u64\nlet b be 9007199254740993\n\
                  let c be -1i32\n\
                  print(f\"{a as F64} {b as F64} {c as F64}\")\n";
    assert_eq!(
        prints("to-float", source),
        "1.8446744073709552e19 9007199254740992.0 -1.0\n",
        "an integer-to-float cast used the wrong instruction or the wrong rounding"
    );
}

/// **Float to integer saturates, and NaN is zero.** §5.1, in as many words.
///
/// Every line here is `poison` under a bare `fptosi`, which is not "a large
/// number" — it is a licence for `-O2` to assume the case does not happen. The
/// answers are the destination's extremes, and the NaN is built at run time
/// from `0.0 / 0.0` so that no constant folder can have decided it earlier.
#[test]
fn a_float_to_integer_cast_saturates_and_nan_is_zero() {
    let source = "let big be 1.0e300\nlet small be -1.0e300\n\
                  let zero be 0.0\nlet nan be zero / zero\n\
                  print(f\"{big as I64} {small as I64} {nan as I64}\")\n";
    assert_eq!(
        prints("sat-signed", source),
        "9223372036854775807 -9223372036854775808 0\n",
        "§5.1: \"`as` from float to integer saturates, and NaN becomes zero\""
    );
}

/// The unsigned half of the same rule, where the lower bound is zero rather
/// than a negative number.
///
/// `-1.5 as U8` is `0` and `1e300 as U8` is `255`, which is what saturation
/// means when the destination has no negative values at all.
#[test]
fn an_unsigned_float_to_integer_cast_saturates_at_both_ends() {
    let source = "let a be -1.5\nlet b be 1.0e300\nlet c be 254.9\n\
                  let zero be 0.0\nlet nan be zero / zero\n\
                  print(f\"{a as U8} {b as U8} {c as U8} {nan as U64}\")\n";
    assert_eq!(
        prints("sat-unsigned", source),
        "0 255 254 0\n",
        "an unsigned saturating conversion is wrong at one of its four cases"
    );
}

/// **Float to integer truncates toward zero for the values that do fit**, which
/// is the other half of the conversion and the half saturation says nothing
/// about.
#[test]
fn a_float_to_integer_cast_truncates_toward_zero() {
    let source = "let a be 2.9\nlet b be -2.9\nlet c be -0.5\n\
                  print(f\"{a as I64} {b as I64} {c as I64}\")\n";
    assert_eq!(prints("trunc-toward-zero", source), "2 -2 0\n");
}

// --- float to float -------------------------------------------------------

/// **`F64` to `F32` rounds and overflows to an infinity, and back is exact.**
///
/// `0.1` is the value that makes the round trip visible: as an `F32` it is
/// `0.1`, and widened back to an `F64` it is `0.10000000149011612` — which is
/// `science_string_push_f32`'s own note, *"the shortest string that
/// round-trips is a property of the width"*, arriving as a program.
///
/// And `1e300 as F32` is `inf` rather than poison: `fptrunc` is total where
/// `fptosi` is not, which is why the float-to-float direction needs no
/// saturating intrinsic.
#[test]
fn a_float_to_float_cast_rounds_and_overflows_to_infinity() {
    let source = "let a be 0.1\nlet b be 1.0e300\nlet c be 0.5f32\n\
                  print(f\"{a as F32} {b as F32} {c as F64}\")\n";
    assert_eq!(
        prints("float-float", source),
        "0.1 inf 0.5\n",
        "a float-to-float conversion rounded or overflowed wrong"
    );
}

/// The `F32`-to-`F64` widening, at a value where "exact" and "the same digits"
/// are different answers.
///
/// `0.1f32 as F64` is `0.10000000149011612`. A compiler that rendered the
/// widened value through the `F32` entry point, or that did not widen at all,
/// prints `0.1`.
#[test]
fn widening_a_float_is_exact_and_that_is_visible() {
    let source = "let a be 0.1f32\nprint(f\"{a as F64}\")\n";
    assert_eq!(prints("widen-float", source), "0.10000000149011612\n");
}

// --- `Char` and `Bool` ----------------------------------------------------

/// **A `Char` widens out to any integer and a `U8` widens in.**
///
/// `'ñ'` is `U+00F1`, which is 241 — one byte as a code point and two as UTF-8,
/// so a compiler that took the encoded length or the first byte prints
/// something else. `'€'` is `U+20AC`, 8364, which does not fit in a byte at
/// all: `'€' as U8` is `172`, the truncation, for the same reason
/// `300 as U8` is `44`.
#[test]
fn a_char_converts_as_an_unsigned_code_point() {
    let source = "let a be 'A'\nlet b be '\u{f1}'\nlet c be '\u{20ac}'\nlet d be 66u8\n\
                  print(f\"{a as Int} {b as Int} {c as Int} {c as U8} {d as Char}\")\n";
    assert_eq!(
        prints("char", source),
        "65 241 8364 172 B\n",
        "a `Char` conversion is not the unsigned code point"
    );
}

/// **A `Bool` widens to `0` or `1`.**
///
/// §3.1 gives `Bool` two forms — `i1` in a register and `i8` in memory — and
/// the cast reads the memory form, whose byte is 0 or 1. A `sext` here would
/// print `-1` for `true`, which is the same mistake `not` avoids one operator
/// over and which `crate::emit`'s `widen_bool` calls out by name.
#[test]
fn a_bool_widens_to_zero_or_one() {
    let source = "let t be true\nlet f be false\n\
                  print(f\"{t as Int} {f as Int} {t as U8} {t as I8}\")\n";
    assert_eq!(prints("bool", source), "1 0 1 1\n");
}

// --- what is refused ------------------------------------------------------

/// **The casts §5.1 does not define, each refused with both type names in the
/// message.**
///
/// Every one of these **checks clean**: `science-types`'s `Cast` arm types
/// `e as T` as `T` and asks nothing about `e`, so the type checker will accept
/// `"hola" as Int`. That makes this backend the only phase that can refuse
/// them, which is the argument for refusing loudly rather than lowering
/// something plausible: `1 as Bool` has an obvious `trunc` behind it that would
/// put a `2` in a `Bool`'s byte, and every later `trunc i8 to i1` reads that as
/// `true`.
#[test]
fn what_no_cast_may_do() {
    let cases: &[(&str, &str)] = &[
        // A `Bool` has two valid bytes out of 256 and a `trunc` can write any
        // of them.
        ("to-bool", "let a be 2\nlet b be a as Bool\nprint(\"x\")\n"),
        // `Char` is not a dense range: `0xD800`..`0xDFFF` are not scalar
        // values, so "saturating" has no meaning and there is no check
        // specified.
        ("int-to-char", "let a be 300\nlet b be a as Char\nprint(\"x\")\n"),
        ("float-to-char", "let a be 65.0\nlet b be a as Char\nprint(\"x\")\n"),
        // What a `Bool` *is* as a number is a decision §5.1 does not take.
        ("bool-to-float", "let a be true\nlet b be a as F64\nprint(\"x\")\n"),
        ("char-to-float", "let a be 'A'\nlet b be a as F64\nprint(\"x\")\n"),
        // And the one that shows the type checker is not the phase asking:
        // this is not a numeric conversion at all and it checks clean.
        ("string-to-int", "let a be \"hola\"\nlet b be a as Int\nprint(\"x\")\n"),
    ];
    for (name, source) in cases {
        let text = refusal(name, source);
        assert!(text.contains("a cast from"), "`{name}`'s refusal does not name a cast:\n{text}");
    }
}

/// **A cast to `Bool` is refused and a cast *of* a `Bool` is not**, so the
/// refusal above is about the destination rather than about `Bool` generally.
///
/// Worth its own test because a table of refusals is easy to widen by accident,
/// and the widening that would happen here — refusing `Bool` on either side —
/// takes [`a_bool_widens_to_zero_or_one`]'s program with it.
#[test]
fn the_refusal_is_about_the_destination_and_not_the_type() {
    assert_eq!(prints("bool-out", "let t be true\nprint(f\"{t as I64}\")\n"), "1\n");
    let text = refusal("bool-in", "let a be 1\nlet b be a as Bool\nprint(\"x\")\n");
    assert!(text.contains("`Bool`"), "{text}");
}

// --- the contract this closes ---------------------------------------------

/// **`science_string_push_i64`'s documented caller now exists.**
///
/// The entry point's note reads *"`I8`…`I64` are sign-extended by codegen
/// before the call"*, and `science-mir`'s §7 item 10 was that **no phase did
/// it** — there was no `Rvalue::Cast` lowering, so there was nothing between
/// the choice of entry point and the call that could. The cast is what makes
/// the sentence true, and this is the shortest program that depends on it.
///
/// `tests/interpolation.rs` runs the six narrow widths at values where the
/// wrong extension is visible; this asserts the mechanism is a cast by reading
/// the IR, so that a regression to *"refuse the narrow widths again"* fails
/// here with something to read rather than by one program printing nothing.
#[test]
fn a_narrow_hole_reaches_its_entry_point_through_an_extension() {
    let dir = scratch("casts", "push-extension");
    require_runtime();
    let built = lower("let n be -1i32\nprint(f\"{n}\")\n")
        .build_at(&executable(&dir, "push-extension"), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("sext i32"), "no sign extension in front of the push:\n{text}");
    assert!(
        text.contains("@science_string_push_i64("),
        "the narrow hole did not reach `push_i64`:\n{text}"
    );
}

/// **The saturating intrinsics are declared by name, and the name is right.**
///
/// [`crate::emit::LlvmBackend::saturating_float_to_int`]'s first cost: a
/// misspelled intrinsic is an ordinary external function, which verifies and
/// then fails at *link* time. Every test above links, so the names are already
/// exercised — this reads them back so that the failure, when it comes, says
/// which name rather than `LNK2019`.
#[test]
fn the_saturating_intrinsics_are_declared_with_the_spelling_llvm_knows() {
    let dir = scratch("casts", "intrinsics");
    require_runtime();
    let built =
        lower("let a be 1.5\nlet b be a as I64\nlet c be a as U32\nprint(f\"{b}\")\n")
            .build_at(&executable(&dir, "intrinsics"), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(text.contains("llvm.fptosi.sat.i64.f64"), "{text}");
    assert!(text.contains("llvm.fptoui.sat.i32.f64"), "{text}");
}
