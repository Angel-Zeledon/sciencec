//! `stdlib-core.md` §8's Level 1 math, **built, linked, run**: the thirty-five
//! methods on `F64` and `F32`, the seven on `Int`, and the constants, each on
//! values chosen to sit on the edge its lowering decides.
//!
//! # What the edges are, and where they were decided
//!
//! `science-codegen-llvm`'s `Lowerer::lower_math_method` carries the decisions
//! and their reasons; each test below names the one it pins. In short:
//! `min`/`max` propagate NaN and order `-0.0` below `+0.0` (IEEE-754
//! `minimum`/`maximum`); `round` is half-away-from-zero; `sign` of a zero is
//! that zero and of NaN is NaN; `fract` keeps the receiver's sign;
//! `rem_euclid` is never negative; `is_close` is relative at `√EPSILON`;
//! `Int.abs()` of `Int.MIN` wraps.
//!
//! # Why some inputs come out of `parse_float`
//!
//! Every test builds at `-O2`, where a whitelisted intrinsic of a constant is
//! folded by LLVM before the program runs — correctly, since Decision 37's
//! whitelist is exactly the set whose folding is bit-identical, but it means a
//! constant-only fixture tests the folder and not the instruction. The tests
//! that matter for the emitted sequence read their operands out of a string,
//! which no optimiser can see through.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("math", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The IR of a program that built, unoptimised, for the assertions that are
/// about which call a method became.
fn ir(name: &str, source: &str) -> String {
    let dir = scratch("math", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

fn lines(text: &[&str]) -> String {
    text.iter().map(|line| format!("{line}\n")).collect()
}

/// §8.1's elementary group on `F64`, at the edges each lowering decides.
#[test]
fn elementary_methods_on_f64_at_their_edges() {
    let out = prints(
        "elementary_f64",
        "def main():
    let x be -2.5
    print(x.abs())
    print(x.sign())
    print((0.0).sign())
    print((-0.0).sign())
    print(F64.NAN.sign())
    print(x.floor())
    print(x.ceil())
    print(x.trunc())
    print(x.round())
    print((2.5).round())
    print((-0.5).round())
    print((-0.5).floor())
    print((-1.25).fract())
    print((3.0).min(F64.NAN))
    print(F64.NAN.max(3.0))
    print((-0.0).min(0.0))
    print((0.0).max(-0.0))
    print((1.0).max(2.0))
    print((5.0).clamp(0.0, 1.0))
    print((-5.0).clamp(0.0, 1.0))
    print((0.5).clamp(1.0, 0.0))
    print((-7.5).rem_euclid(2.0))
    print((7.5).rem_euclid(-2.0))
    print((1.0).rem_euclid(0.0))
",
    );
    assert_eq!(
        out,
        lines(&[
            "2.5", "-1.0", "0.0", "-0.0", "NaN", "-3.0", "-2.0", "-2.0",
            // `round`: half away from zero, in both directions.
            "-3.0", "3.0", "-1.0", "-1.0", "-0.25",
            // `min`/`max`: NaN on either side wins, and the zeros are ordered.
            "NaN", "NaN", "-0.0", "0.0", "2.0",
            // `clamp`, including a reversed range, which answers `high`.
            "1.0", "0.0", "0.0",
            // `rem_euclid`: never negative; a zero divisor is NaN, as `%` is.
            "0.5", "1.5", "NaN",
        ])
    );
}

/// The same inline sequences on operands the optimiser cannot see, so what
/// runs is the emitted `select`s and compares and not LLVM's folding of them.
#[test]
fn the_inline_sequences_run_on_values_the_optimiser_cannot_see() {
    let out = prints(
        "opaque_f64",
        "def main():
    let x, e1 be \"-2.5\".parse_float()
    let zero, e2 be \"-0.0\".parse_float()
    let nan, e3 be \"NaN\".parse_float()
    let inf, e4 be \"inf\".parse_float()
    if e1? or e2? or e3? or e4?:
        return
    print(x.sign())
    print(zero.sign())
    print(nan.sign())
    print(x.round())
    print(x.fract())
    print(x.min(nan))
    print(zero.max(0.0))
    print(x.clamp(-1.0, 1.0))
    print(x.rem_euclid(2.0))
    print(x.sqrt())
    print(nan.is_nan())
    print(x.is_nan())
    print(inf.is_infinite())
    print(inf.is_finite())
    print(nan.is_finite())
    print(x.is_finite())
    print(x.is_close(-2.5000000001))
    print(inf.is_close(inf))
    print(nan.is_close(nan))
",
    );
    assert_eq!(
        out,
        lines(&[
            "-1.0", "-0.0", "NaN", "-3.0", "-0.5", "NaN", "0.0", "-1.0", "1.5", "NaN",
            "true", "false", "true", "false", "false", "true", "true", "true", "false",
        ])
    );
}

/// `sqrt` and the library group on `F64`, and the trigonometry. Values whose
/// result is exact in binary, so the assertion is about the call and not about
/// one libm's last bit (§8.3: *"not guaranteed bit-identical across
/// targets"*).
#[test]
fn roots_exponentials_and_trigonometry_on_f64() {
    let out = prints(
        "library_f64",
        "def main():
    print((16.0).sqrt())
    print((-1.0).sqrt())
    print((-0.0).sqrt())
    print((27.0).cbrt())
    print((-8.0).cbrt())
    print((3.0).hypot(4.0))
    print((0.0).exp())
    print((1.0).ln())
    print((0.0).ln())
    print((-1.0).ln())
    print((8.0).log2())
    print((1000.0).log10())
    print((2.0).pow(10.0))
    print((2.0).pow(-1.0))
    print((0.0).sin())
    print((0.0).cos())
    print((0.0).tan())
    print((1.0).asin() * 2.0 is F64.PI)
    print((1.0).acos())
    print((0.0).atan())
    print((1.0).atan2(0.0) * 2.0 is F64.PI)
    print((0.0).atan2(-1.0) is F64.PI)
    print((0.0).sinh())
    print((0.0).cosh())
    print((0.0).tanh())
    print(F64.INFINITY.tanh())
    print(F64.PI.to_degrees())
    print((180.0).to_radians() is F64.PI)
",
    );
    assert_eq!(
        out,
        lines(&[
            "4.0", "NaN", "-0.0", "3.0", "-2.0", "5.0", "1.0", "0.0", "-inf", "NaN", "3.0",
            "3.0", "1024.0", "0.5", "0.0", "1.0", "0.0", "true", "0.0", "0.0", "true", "true",
            "0.0", "1.0", "0.0", "1.0", "180.0", "true",
        ])
    );
}

/// `is_close`: equal, or both finite and within `√EPSILON` of the larger
/// magnitude. Relative only, so `0.0` is close to nothing but itself.
#[test]
fn is_close_is_relative_and_refuses_infinities_and_nan() {
    let out = prints(
        "is_close",
        "def main():
    print((1.0).is_close(1.0 + 1e-12))
    print((1.0).is_close(1.0001))
    print((1e300).is_close(1e300 * (1.0 + 1e-10)))
    print((0.0).is_close(1e-300))
    print((0.0).is_close(-0.0))
    print(F64.INFINITY.is_close(F64.INFINITY))
    print(F64.INFINITY.is_close(F64.MAX))
    print(F64.NAN.is_close(F64.NAN))
    let f: F32 be 1.0
    print(f.is_close(1.0001))
    print(f.is_close(1.01))
",
    );
    assert_eq!(
        out,
        lines(&[
            "true", "false", "true", "false", "true", "true", "false", "false", "true", "false",
        ])
    );
}

/// Every one of the thirty-five on `F32`: the intrinsics at `.f32`, the
/// inline sequences at `float`, and the library calls widened to the `F64`
/// symbol and narrowed back. The printed digits are `F32`'s shortest
/// round-trip form, so a result left at `F64` width would print more of them.
#[test]
fn every_method_on_f32() {
    let out = prints(
        "every_f32",
        "def main():
    let x: F32 be -2.5
    let two: F32 be 2.0
    let half: F32 be 0.5
    print(x.abs())
    print(x.sign())
    print(x.floor())
    print(x.ceil())
    print(x.round())
    print(x.trunc())
    print(x.fract())
    print(x.min(two))
    print(x.max(two))
    print(x.clamp(-1.0, 1.0))
    print(x.rem_euclid(two))
    print(two.sqrt())
    print((27.0 as F32).cbrt())
    print((3.0 as F32).hypot(4.0))
    print(half.exp())
    print(two.ln())
    print(two.log2())
    print((100.0 as F32).log10())
    print(two.pow(half))
    print(half.sin())
    print(half.cos())
    print(half.tan())
    print(half.asin())
    print(half.acos())
    print(half.atan())
    print(half.atan2(two))
    print(half.sinh())
    print(half.cosh())
    print(half.tanh())
    print(F32.PI.to_degrees())
    print((90.0 as F32).to_radians())
    print(F32.NAN.is_nan())
    print(F32.INFINITY.is_infinite())
    print(x.is_finite())
    print(x.is_close(-2.5001))
",
    );
    assert_eq!(
        out,
        lines(&[
            "2.5", "-1.0", "-3.0", "-2.0", "-3.0", "-2.0", "-0.5", "-2.5", "2.0", "-1.0", "1.5",
            "1.4142135", "3.0", "5.0", "1.6487212", "0.6931472", "1.0", "2.0", "1.4142135",
            "0.47942555", "0.87758255", "0.5463025", "0.5235988", "1.0471976", "0.4636476",
            "0.24497867", "0.5210953", "1.127626", "0.46211717", "180.0", "1.5707964", "true",
            "true", "true", "true",
        ])
    );
}

/// `Int`'s seven, including the two edges that are decided rather than
/// obvious: `Int.MIN.abs()` wraps, as `-Int.MIN` does, and a reversed
/// `clamp` answers `high`.
#[test]
fn the_seven_methods_on_int() {
    let out = prints(
        "int",
        "def main():
    let n be -7
    print(n.abs())
    print(n.sign())
    print((0).sign())
    print((12).sign())
    print(n.min(3))
    print(n.max(3))
    print(n.clamp(-5, 5))
    print((9).clamp(-5, 5))
    print((0).clamp(5, -5))
    print(n.rem_euclid(3))
    print(n.rem_euclid(-3))
    print((7).rem_euclid(-3))
    print(Int.MIN.rem_euclid(-1))
    print((2).pow(10))
    print((-3).pow(3))
    print(Int.MIN.abs())
",
    );
    assert_eq!(
        out,
        lines(&[
            "7", "-1", "0", "1", "-7", "3", "-5", "5", "-5", "2", "2", "1", "0", "1024", "-27",
            "-9223372036854775808",
        ])
    );
}

/// `Int.rem_euclid(0)` panics with `%`'s own message, from inside
/// `science_rem_euclid_i64` — a method call is one block, so the guard that
/// `division_check` emits as MIR blocks for `%` lives in the runtime here.
#[test]
fn an_integer_euclidean_remainder_by_zero_panics_like_percent() {
    let dir = scratch("math", "rem_euclid_zero");
    require_runtime();
    let built = lower(
        "def main():
    let n, err be \"0\".parse_int()
    if err?:
        return
    print((7).rem_euclid(n))
",
    )
    .build_at(&executable(&dir, "rem_euclid_zero"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_ne!(ran.status, Some(0), "a remainder by zero exited cleanly");
    assert_eq!(ran.stdout, "");
    assert!(ran.stderr.contains("panic: divide by zero"), "stderr: {}", ran.stderr);
}

/// §8.1's eight constants per float width and `Int`'s two, spelled as §8
/// spells them — `F64.PI`, a field read on the type — and reached through
/// both spellings of each aliased primitive.
#[test]
fn the_constants_on_each_type() {
    let out = prints(
        "constants",
        "def main():
    print(F64.PI)
    print(F64.E)
    print(F64.TAU)
    print(F64.INFINITY)
    print(F64.NAN)
    print(F64.EPSILON)
    print(F64.MIN)
    print(F64.MAX)
    print(F32.PI)
    print(F32.E)
    print(F32.TAU)
    print(F32.INFINITY)
    print(F32.NAN)
    print(F32.EPSILON)
    print(F32.MIN)
    print(F32.MAX)
    print(Int.MIN)
    print(Int.MAX)
    let widest be I64.MAX
    print(widest is Int.MAX)
    let pi be Float.PI
    print(pi is F64.PI)
    print(-F64.INFINITY)
",
    );
    assert_eq!(
        out,
        lines(&[
            "3.141592653589793",
            "2.718281828459045",
            "6.283185307179586",
            "inf",
            "NaN",
            "2.220446049250313e-16",
            "-1.7976931348623157e308",
            "1.7976931348623157e308",
            "3.1415927",
            "2.7182817",
            "6.2831855",
            "inf",
            "NaN",
            "1.1920929e-7",
            "-3.4028235e38",
            "3.4028235e38",
            "-9223372036854775808",
            "9223372036854775807",
            "true",
            "true",
            "-inf",
        ])
    );
}

/// §8.5's `rms`, verbatim but for revision 3's `&Array[F64]`: the example the
/// note gives for why the surface is methods, with no `use` line.
#[test]
fn stdlib_cores_own_example_runs() {
    let out = prints(
        "rms",
        "def rms(values: &Array[F64]) -> F64:
    if values.is_empty():
        return 0.0
    let mutable total be 0.0
    for v in values:
        total be total + v * v
    (total / values.length() as F64).sqrt()

def main():
    print(rms([3.0, 4.0, 12.0, 84.0]))
    print(rms((Array[F64]).new()))
",
    );
    assert_eq!(out, "42.5\n0.0\n");
}

/// Decision 37, read off the IR: the whitelisted eight are `llvm.*`
/// intrinsics at the receiver's width, every library method is a direct call
/// to its `science_libm_*` symbol, and no transcendental intrinsic appears —
/// `llvm.sin` would be folded against the build host's libm.
#[test]
fn the_whitelist_is_an_intrinsic_and_the_library_is_a_call() {
    let text = ir(
        "lowering",
        "def main():
    let x, err be \"0.5\".parse_float()
    if err?:
        return
    let f be x as F32
    print(x.sqrt() + x.abs() + x.floor() + x.ceil() + x.trunc() + x.round())
    print(x.min(1.0) + x.max(1.0))
    print(f.sqrt())
    print(x.sin() + x.exp() + x.pow(2.0))
    print(f.sin())
",
    );
    for intrinsic in [
        "llvm.sqrt.f64",
        "llvm.fabs.f64",
        "llvm.floor.f64",
        "llvm.ceil.f64",
        "llvm.trunc.f64",
        "llvm.round.f64",
        "llvm.minimum.f64",
        "llvm.maximum.f64",
        "llvm.sqrt.f32",
    ] {
        assert!(text.contains(&format!("@{intrinsic}(")), "no call to `{intrinsic}`:\n{text}");
    }
    for symbol in ["science_libm_sin", "science_libm_exp", "science_libm_pow"] {
        assert!(text.contains(&format!("@{symbol}(")), "no call to `{symbol}`:\n{text}");
    }
    for forbidden in ["@llvm.sin", "@llvm.exp", "@llvm.pow", "@llvm.roundeven", "@llvm.minnum"] {
        assert!(!text.contains(forbidden), "`{forbidden}` is off Decision 37's whitelist:\n{text}");
    }
    // `F32` reaches the one `F64` symbol through a widening, not a second one.
    assert!(text.contains("fpext float"), "an `F32` library call did not widen:\n{text}");
}
