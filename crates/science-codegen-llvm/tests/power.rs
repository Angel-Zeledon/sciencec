//! `**`, built, linked, run — closing the refusal `describe_binary` used to
//! give unconditionally: *"this `sciencec` cannot build `**`, which needs
//! `llvm.pow` and Decision 37's intrinsic whitelist"*.
//!
//! # What this file closes
//!
//! `examples/12_operators.science`'s `let power be 2 ** 3 ** 2` used to refuse
//! at that message, for `Int ** Int` — the only shape the example exercises.
//! `Float ** Float` reaches a different call, so both are asserted here rather
//! than inferring one from the other.
//!
//! # Why a call and not an instruction, briefly
//!
//! `codegen-and-linking.md` §7.3's Decision 37 lists `llvm.pow` off
//! `intrinsics-math-physics.md` §3.1's whitelist, because LLVM constant-folds
//! that intrinsic against the *build host's* libm rather than
//! `science-libm`'s. `Lowerer::lower_pow`'s own documentation is the account,
//! including why `Int ** Int` needs a call too — no instruction computes one at
//! all — and both land on `science-rt`'s `math.rs`.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::{OptLevel, first_fast_math_flag};

fn output(name: &str, source: &str) -> harness::Ran {
    let dir = scratch("power", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O3);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    ran
}

fn prints(name: &str, source: &str) -> String {
    let ran = output(name, source);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The line `examples/12_operators.science` could not build: `Int ** Int`,
/// right-associative, so `2 ** 3 ** 2` is `2 ** (3 ** 2)` — `2 ** 9`, `512` —
/// and not `(2 ** 3) ** 2`, `64`. Getting the associativity backwards is a
/// silent wrong answer, not a build failure, which is why the value and not
/// only the exit code is the assertion.
///
/// Written with an explicit `def main():`, matching
/// `examples/12_operators.science`'s own shape, rather than a bare script
/// body — the two are different entry points below this crate and this file
/// is about the operator, not about which of the two reaches it.
#[test]
fn int_pow_is_right_associative_and_prints_the_right_answer() {
    let source = "def main():\n    let power be 2 ** 3 ** 2\n    print(power)\n";
    assert_eq!(prints("int-assoc", source), "512\n");
}

/// A negative exponent, per `Lowerer::lower_pow`'s documentation: not
/// mathematically meaningful over the integers, but required to be
/// *deterministic*, the same bargain §5.1 already makes for overflow. Asserted
/// against `science-rt`'s own `science_ipow_i64` unit test rather than an
/// independent computation, because that call's shape is the specification.
#[test]
fn int_pow_wraps_on_overflow_like_every_other_int_op() {
    let source =
        "def main():\n    let base be 2\n    let exponent be 63\n    print(base ** exponent)\n";
    // `2i64.wrapping_pow(63)` is `i64::MIN`, the same wraparound `IntOp::Mul`
    // already gives `2 * (1 << 62)` two multiplies earlier — `**` is not a
    // special case of §5.1's "wraps in release", it is the same rule.
    assert_eq!(prints("int-overflow", source), "-9223372036854775808\n");
}

/// `Float ** Float`, at a value where a wrong lowering (an `llvm.pow` folded
/// at compile time against a *different* host, or a wrong argument order into
/// the runtime call) and a right one would print different digits.
#[test]
fn float_pow_calls_science_libm_pow() {
    let source =
        "def main():\n    let base be 2.0\n    let exponent be 0.5\n    print(base ** exponent)\n";
    assert_eq!(prints("float-sqrt", source), "1.4142135623730951\n");
}

/// `F32 ** F32`: the narrower width, which `Lowerer::lower_pow` widens to
/// `F64` for the call and narrows back — a second path through the same
/// function, not a copy of it for a second width.
#[test]
fn float32_pow_widens_calls_and_narrows() {
    let source = "def main():\n    let base be 2.0 as F32\n    let exponent be 3.0 as F32\n    \
                  print(base ** exponent)\n";
    assert_eq!(prints("float32", source), "8.0\n");
}

/// Decision 35, restated for the one operator with the sharpest reason to
/// need it: `**` is the operator this crate could most easily have reached
/// for a fast-math-flagged instruction to build, since there is none — and it
/// still emits none, because it is a call, and a call carries no fast-math
/// flag to set.
#[test]
fn power_emits_no_fast_math_flag() {
    let dir = scratch("power", "fast-math");
    require_runtime();
    let source = "def main():\n    let power be 2 ** 3 ** 2\n    let fp be 2.0 ** 0.5\n    \
                  print(power)\n    print(fp)\n";
    let built = lower(source).build_at(&executable(&dir, "fast-math"), OptLevel::O3);
    let ir = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(first_fast_math_flag(&ir), None, "{ir}");
}
