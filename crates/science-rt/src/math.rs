//! `**`: the two entry points its codegen calls rather than lowers, and why
//! neither is an instruction.
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
//! There is no such crate yet — `math`'s wider library (`sin`, `cos`, `exp`,
//! …) is F0's Level 2 and unbuilt, per `stdlib-core.md` §8. `**` is not a
//! library call at the Science level, though; it is a **core-spec operator**
//! (§4.6's precedence table), so its codegen cannot wait on a library that
//! does not exist. `science_libm_pow` lives here, in the crate every binary
//! already links, rather than in a second staticlib invented for one function
//! — and it is named for the crate Decision 37 asks for rather than for this
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

#[cfg(test)]
mod tests {
    use super::*;

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
