//! `**` and `stdlib-core.md` §8's Level 1 math: the entry points codegen
//! calls rather than lowers, and why none of them is an instruction.
//!
//! # The decision
//!
//! **`science_libm_pow` and `science_ipow_i64` are ordinary calls, never an
//! LLVM instruction and never an LLVM intrinsic.** `F64 ** F64` has no
//! hardware instruction and no IEEE-754-exact intrinsic to reach for — unlike
//! `sqrt` or `fma`, `pow` cannot be computed to the last bit from a handful of
//! multiplies, so any lowering is a *library*, and `codegen-and-linking.md`
//! §7.3's Decision 37 says which one: *"a direct call to `science-libm`'s
//! symbol, never to an LLVM intrinsic, precisely so that LLVM cannot
//! constant-fold it against the build host's libm."* `Int ** Int` has the same
//! shape for a different reason — exponentiation by squaring is a loop, and
//! `science-codegen-llvm`'s Decision 8 makes every MIR basic block exactly one
//! LLVM basic block, so a loop cannot appear inside a lowered instruction
//! sequence at all. Both problems have the same fix: hide the control flow
//! inside a called function, the same way `science_string_push_i64` hides a
//! `write!` loop and `science_array_push` hides a growth check.
//!
//! **Decision 37's name is `science-libm`, and this crate is not that crate.**
//! There is no such crate yet. `**` is not a library call at the Science
//! level, though; it is a **core-spec operator** (§4.6's precedence table), so
//! its codegen could not wait on a library that does not exist — and neither
//! could §8.1's Level 1 methods (`sin`, `exp`, `ln`, …) when they landed, which
//! is why their sixteen symbols sit below beside it, under the same naming.
//! `science_libm_pow` lives here, in the crate every binary already links,
//! rather than in a second staticlib invented for one function — and it is
//! named for the crate Decision 37 asks for rather than for this
//! one, so the day `science-libm` exists, moving the symbol is a rename and
//! not a redesign.
//!
//! **`science_ipow_i64` is not a `science-rt` entry point of `codegen-and-linking.md`
//! Decision 14's fifty-five, and it is right that the count moved.** Decision
//! 14's "no entry point is added to `science-rt` to make codegen simpler"
//! is a rule against *convenience* — a call standing in for a handful of
//! instructions codegen could have emitted directly. Neither symbol here is
//! that: nothing below this crate can emit `pow` as an instruction sequence at
//! all, so a call is not a convenience, it is the only lowering that exists.
//! `science_codegen::runtime::RUNTIME` records both as the fifty-sixth and
//! fifty-seventh entries for exactly that reason.
//!
//! # Why the exponent is read as bits, not as a sign
//!
//! `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §5.4 makes
//! `**` an operator interface — `Self.pow(Self) -> Self` — the same shape as
//! `+`, `-` and `*`, and §5.1 numeric semantics for `Int` says only that
//! "integer overflow panics in debug builds and wraps in release, as Rust
//! does" — nothing about a negative right-hand side, because an interface
//! that takes `Self` on both sides has no narrower type to put a `U32`-shaped
//! restriction on the way Rust's own `i64::pow` does. `science-codegen-llvm`'s
//! `IntBinary` lowering for `+`, `-`, `*` already emits only the wrapping half
//! of §5.1's rule — no `nsw`/`nuw`, and no debug-mode guard, because nothing
//! above it distinguishes a debug build — so `science_ipow_i64` matches the
//! sibling it stands beside: the exponent's bit pattern drives exponentiation
//! by squaring regardless of its sign, and the multiplications that pattern
//! selects wrap exactly as `*` already does. A negative exponent is therefore
//! not a special case reached by a branch; it is `-1i64`'s bit pattern read as
//! `u64::MAX`, which the loop below runs to completion on like any other
//! `u64`, in at most 64 iterations, producing a wrapped and deterministic —
//! if not mathematically meaningful — `Int`. That is the same bargain §5.1
//! already made for overflow: a defined answer on every machine rather than a
//! guard nothing above this crate emits.

/// `F64 ** F64` (and, after codegen extends the narrower widths up and
/// truncates the result back down, `F16 ** F16`, `BF16 ** BF16` and
/// `F32 ** F32`). Named `science_libm_pow` rather than `pow` so that LLVM's
/// `TargetLibraryInfo` — which recognises `pow` as a well-known libm call and
/// folds a constant argument through the *build host's* `pow` at `-O3` — has
/// no name to recognise here at all. `opt -S -O3` on the symbol `pow`
/// constant-folds `pow(2.0, 3.0)` to `8.0`; the same optimisation pipeline
/// leaves a call to this symbol as a `call`, which is the entire content of
/// Decision 37 demonstrated rather than asserted.
#[no_mangle]
pub extern "C" fn science_libm_pow(base: f64, exponent: f64) -> f64 {
    base.powf(exponent)
}

/// `Int ** Int` (and, after codegen sign- or zero-extends the narrower widths
/// up and truncates the result back down, every other integer width — the bit
/// pattern of a wrapping multiply does not depend on signedness, so one
/// function serves both, the way `IntOp::Mul` already does one instruction for
/// both).
///
/// Exponentiation by squaring over `exponent`'s bits, wrapping on every
/// multiply exactly as `*` wraps — see this module's own documentation for
/// why a negative `exponent` is read as a `u64` rather than refused.
#[no_mangle]
pub extern "C" fn science_ipow_i64(base: i64, exponent: i64) -> i64 {
    let mut base = base;
    let mut exponent = exponent as u64;
    let mut result: i64 = 1;
    while exponent != 0 {
        if exponent & 1 == 1 {
            result = result.wrapping_mul(base);
        }
        exponent >>= 1;
        if exponent != 0 {
            base = base.wrapping_mul(base);
        }
    }
    result
}

// `stdlib-core.md` §8.1's transcendental methods: the sixteen that are
// library code on every target, one entry point each.
//
// # The decision
//
// **Each is a direct call to a symbol named `science_libm_<method>`, over
// Rust's `f64` method of the same meaning, which calls the platform's libm.**
// `science_libm_pow` above is the precedent and its doc comment is the
// argument, repeated per name: none of these has an instruction
// (`intrinsics-math-physics.md` §3.5's Decision 5a: *"every elementary function
// except `sqrt` is library code that happens to be fast"*), and none is on
// Decision 37's whitelist, so an `llvm.sin` would be folded against the
// build host's libm — the symbol name is what stops that, because LLVM's
// `TargetLibraryInfo` knows `sin` and does not know `science_libm_sin`.
// `science_codegen::target::lower_math_call` already spells the symbol this
// way for every name off the whitelist; this is that spelling given bodies.
//
// **`F64` only, and `F32` widens.** `science-codegen-llvm`'s
// `Lowerer::lower_math_method` extends an `F32` argument, calls the `F64`
// symbol and narrows the result — the shape `**` already has for every float
// width. That is within §8.3's 1 ULP for `F32` and not merely close to it: the
// `f64` result is within one `f64` ULP of the true value, and rounding that to
// `f32` lands within half an `f32` ULP plus a vanishing fraction of one.
// Sixteen more entry points for `sinf` and its siblings would buy speed and
// lose that.
//
// # The cost, which is §8.3's and is not fixed here
//
// **These are not bit-identical across targets.** Rust's `f64::sin` is the
// platform libm's `sin`, and macOS's and glibc's differ in the last bit for
// some arguments. §8.3 states exactly that contract — *"within 1 ULP of the
// correctly-rounded result … not guaranteed bit-identical across targets"* —
// and asks codegen to decide the reproducibility question. Decision 37's
// `science-libm` crate, pinning one implementation, is the answer it points
// at; these sixteen are named for it, so that crate is a change of body and
// not of symbol.

/// `F64.cbrt()`. The cube root, defined for a negative argument:
/// `(-8.0).cbrt()` is `-2.0`.
#[no_mangle]
pub extern "C" fn science_libm_cbrt(x: f64) -> f64 {
    x.cbrt()
}

/// `F64.exp()`. `e` raised to the receiver.
#[no_mangle]
pub extern "C" fn science_libm_exp(x: f64) -> f64 {
    x.exp()
}

/// `F64.ln()`. The natural logarithm; NaN below zero, `-inf` at zero.
#[no_mangle]
pub extern "C" fn science_libm_ln(x: f64) -> f64 {
    x.ln()
}

/// `F64.log2()`. The base-2 logarithm.
#[no_mangle]
pub extern "C" fn science_libm_log2(x: f64) -> f64 {
    x.log2()
}

/// `F64.log10()`. The base-10 logarithm.
#[no_mangle]
pub extern "C" fn science_libm_log10(x: f64) -> f64 {
    x.log10()
}

/// `F64.sin()`. The sine of an angle in radians.
#[no_mangle]
pub extern "C" fn science_libm_sin(x: f64) -> f64 {
    x.sin()
}

/// `F64.cos()`. The cosine of an angle in radians.
#[no_mangle]
pub extern "C" fn science_libm_cos(x: f64) -> f64 {
    x.cos()
}

/// `F64.tan()`. The tangent of an angle in radians.
#[no_mangle]
pub extern "C" fn science_libm_tan(x: f64) -> f64 {
    x.tan()
}

/// `F64.asin()`. The arcsine, in `[-π/2, π/2]`; NaN outside `[-1, 1]`.
#[no_mangle]
pub extern "C" fn science_libm_asin(x: f64) -> f64 {
    x.asin()
}

/// `F64.acos()`. The arccosine, in `[0, π]`; NaN outside `[-1, 1]`.
#[no_mangle]
pub extern "C" fn science_libm_acos(x: f64) -> f64 {
    x.acos()
}

/// `F64.atan()`. The arctangent, in `[-π/2, π/2]`.
#[no_mangle]
pub extern "C" fn science_libm_atan(x: f64) -> f64 {
    x.atan()
}

/// `F64.sinh()`. The hyperbolic sine.
#[no_mangle]
pub extern "C" fn science_libm_sinh(x: f64) -> f64 {
    x.sinh()
}

/// `F64.cosh()`. The hyperbolic cosine.
#[no_mangle]
pub extern "C" fn science_libm_cosh(x: f64) -> f64 {
    x.cosh()
}

/// `F64.tanh()`. The hyperbolic tangent.
#[no_mangle]
pub extern "C" fn science_libm_tanh(x: f64) -> f64 {
    x.tanh()
}

/// `F64.hypot(other)`: `√(x² + y²)` without the intermediate overflow — §4.2's
/// rule P2 names it for exactly that, `(x*x + y*y).sqrt()` overflowing past
/// `1.3e154` where this does not.
#[no_mangle]
pub extern "C" fn science_libm_hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

/// `y.atan2(x)`: the arctangent of `y / x` in the quadrant the two signs
/// name, in `[-π, π]`. The receiver is `y`, as §8.4 writes it.
#[no_mangle]
pub extern "C" fn science_libm_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// `Int.rem_euclid(divisor)`: the remainder that is never negative.
///
/// # Why this is a call, when its float twin is three instructions
///
/// **The divisor can be zero, and a zero divisor is a panic, which is a
/// branch.** `srem` by zero is immediate undefined behaviour in LLVM, and
/// `science-mir`'s `division_check` is how `%` guards it — in MIR, as blocks,
/// because `science-codegen-llvm`'s Decision 8 makes one MIR block one LLVM
/// block and a method call is one statement. A method has no MIR of its own
/// to put the guard in, so the guard goes where `science_ipow_i64` put its
/// loop: inside a function. Decision 14's carve-out is the same one —
/// nothing below this crate can emit a panicking edge in a single block.
///
/// The message is `division_check`'s, so `x % 0` and `x.rem_euclid(0)` fail
/// identically. `Int.MIN.rem_euclid(-1)` is `0`, which is the true answer and
/// representable — the overflow `%` refuses there is in the *quotient*, which
/// this never forms.
#[no_mangle]
pub extern "C" fn science_rem_euclid_i64(value: i64, divisor: i64) -> i64 {
    if divisor == 0 {
        let message = b"divide by zero";
        // SAFETY: a static byte string, read for exactly its length.
        unsafe { crate::panic::science_panic_bytes(message.as_ptr(), message.len()) }
    }
    value.wrapping_rem_euclid(divisor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_libm_entry_points_are_the_methods_they_name() {
        assert_eq!(science_libm_cbrt(27.0), 3.0);
        assert_eq!(science_libm_exp(0.0), 1.0);
        assert_eq!(science_libm_ln(1.0), 0.0);
        assert_eq!(science_libm_log2(8.0), 3.0);
        assert_eq!(science_libm_log10(1000.0), 3.0);
        assert_eq!(science_libm_hypot(3.0, 4.0), 5.0);
        assert_eq!(science_libm_atan2(1.0, 1.0), std::f64::consts::FRAC_PI_4);
        assert!(science_libm_ln(-1.0).is_nan());
    }

    #[test]
    fn the_euclidean_remainder_is_never_negative() {
        assert_eq!(science_rem_euclid_i64(-7, 3), 2);
        assert_eq!(science_rem_euclid_i64(-7, -3), 2);
        assert_eq!(science_rem_euclid_i64(7, -3), 1);
        assert_eq!(science_rem_euclid_i64(i64::MIN, -1), 0);
    }

    #[test]
    fn small_powers_agree_with_repeated_multiplication() {
        assert_eq!(science_ipow_i64(2, 10), 1024);
        assert_eq!(science_ipow_i64(3, 0), 1);
        assert_eq!(science_ipow_i64(-2, 3), -8);
        assert_eq!(science_ipow_i64(2, 3), 2i64.wrapping_pow(3));
    }

    #[test]
    fn a_negative_exponent_terminates_and_is_deterministic() {
        // Not a claim that the answer is mathematically meaningful — the
        // module doc says it is not — only that it is the same answer every
        // time, on every machine, which is the property §5.1 asks overflow
        // to have.
        assert_eq!(science_ipow_i64(3, -1), science_ipow_i64(3, -1));
        assert_eq!(science_ipow_i64(1, -1), 1);
    }

    #[test]
    fn float_pow_matches_libm() {
        assert_eq!(science_libm_pow(2.0, 10.0), 1024.0);
        assert_eq!(science_libm_pow(2.0, 0.5), 2.0_f64.sqrt());
    }
}
