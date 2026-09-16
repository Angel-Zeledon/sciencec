//! One-variable linear matching — §10.1 item 7.
//!
//! > **Decision 7.1.** A const parameter `p` is **determined** by an argument
//! > position whose normal form is `c·p + k` with `c ≠ 0` and no other unbound
//! > parameter, given a concrete value `v` at that position: `p = (v − k) / c`,
//! > and it is an error (`SC0262`) if `c` does not divide `v − k`. A position
//! > mentioning two or more unbound parameters is **not** an inference site. A
//! > parameter determined at no site must be given explicitly at the call.
//!
//! # 1. It is a division with a remainder check, and the remainder check is
//! the error
//!
//! That is the whole algorithm, and the restriction is the design. Solving the
//! general case — several positions, several unknowns — is integer linear
//! systems with unknown structure, which §5.4 identifies as what Rust wanted
//! and Science declines. The core spec already refused global inference for
//! ordinary types on the same ground (*"local; signatures are fully
//! annotated"*), and this is that decision applied to const parameters.
//!
//! # 2. Three consumers, and the third is the interesting one
//!
//! - **`Mul` on quantities.** `self: Quantity of (T, L1, M1, …)` determines
//!   every exponent at `c = 1, k = 0`. Nothing is written at the call site,
//!   which is the entire point of `scientific-libraries.md` §12.4.
//! - **Every `linalg` signature.** `a: Tensor of (F32, (M, K))` determines `M`
//!   and `K`; a repeated `K` in a second parameter is an `EQUAL` check against
//!   an already-determined parameter, not a second inference.
//! - **`sqrt` on a `Quantity`** (§7.2). The signature carries *doubled*
//!   exponents in its parameter type and plain ones in its return type, so
//!   `sqrt` on an `Area` matches `L * 2` against `2` and gets `L = 1`, and
//!   `sqrt` on a `Volume` matches `L * 2` against `3` and gets
//!   [`MatchError::Indivisible`]. **That is how units get halving without
//!   getting division** — §4.4's reason for keeping `/` out of the units half
//!   entirely.
//!
//! # 3. The same inversion, used for a message instead of a check
//!
//! `intrinsics-chem-bio.md` §5.2 declares
//! `RateConstant of (T, const ORDER: Int)` with an amount exponent of
//! `1 - ORDER`, and §5.3 wants a diagnostic that says *"`k` is a second-order
//! rate constant"*. That sentence is [`match_linear`] run on `1 - ORDER`
//! against `-1`, which succeeds with `ORDER = 2`. The coefficient is `-1` and
//! the offset is `1`, which makes it this project's first caller needing an
//! integer coefficient other than `+1` — its Finding A, and `tests/matching.rs`
//! names it.
//!
//! # 4. `SC0262` is not built here
//!
//! [`MatchError::Indivisible`] *is* `SC0262`'s condition and carries the
//! parameter, the coefficient, the offset and the value its message needs. The
//! diagnostic itself is F1 (§10.2), because §9.4 makes the **instantiation
//! chain** the whole of the message — *"a post-monomorphization error without
//! one is the failure mode §5.1 says Rust designed its type system to
//! avoid"* — and there is no monomorphiser to walk for a chain yet. §9.1 also
//! routes this condition to `unit-literals.md`'s `SC0254` where that code
//! describes it better, which is a second reason the error is data here and a
//! diagnostic somewhere with more context.

use science_resolve::hir::DefId;

use crate::normal::{Atom, NormalForm};

/// A parameter and the value an argument position determined for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub param: DefId,
    pub value: i128,
}

/// Why an argument position determined nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchError {
    /// The position is not an inference site at all.
    ///
    /// Either it mentions no parameter — `Tensor of (F32, 768)` determines
    /// nothing and is a *check* — or it mentions two or more, which §7.1
    /// declines rather than guessing at. `atoms` is how many it mentions, so
    /// a caller can tell the two apart and say the right thing: "this is
    /// already concrete" and "write the const arguments at the call" are
    /// different messages.
    NotAnInferenceSite { atoms: usize },
    /// `c` does not divide `v − k`. **This is `SC0262`'s condition** (§7.1),
    /// and `unit-literals.md`'s `SC0254` where the site is a unit exponent
    /// (§9.1).
    ///
    /// Everything a message needs is here: the parameter that would have been
    /// determined, its coefficient, the offset, and the concrete value. The
    /// sentence §7.2 wants — *"`2` does not divide `3`"* — is
    /// `coefficient` and `value − offset`.
    Indivisible { param: DefId, coefficient: i128, offset: i128, value: i128 },
    /// `v − k` or the quotient left the range of `i128`.
    ///
    /// Reachable only at the extremes — `v − k` overflowing, or
    /// `i128::MIN / -1` — and refused for the same reason
    /// [`crate::ConstEvalError::Overflow`] is: a wrapped answer here is an
    /// inferred const argument that is silently the wrong number.
    Overflow,
}

/// Recovers `p` from `c·p + k` and a concrete `v`.
///
/// `pattern` is the *signature's* normal form at this argument position, and
/// `value` is the caller's concrete integer there.
///
/// ```text
/// pattern = k + c·p        p = (v − k) / c,  exactly
/// ```
///
/// The division is exact or it is an error, so which way it would have
/// truncated never arises — §4.1's objection to rational coefficients does not
/// reach this function, because this function never rounds.
pub fn match_linear(pattern: &NormalForm, value: i128) -> Result<Match, MatchError> {
    let [term] = pattern.terms() else {
        return Err(MatchError::NotAnInferenceSite { atoms: pattern.terms().len() });
    };
    // Irrefutable only because `Atom` has one variant. When the quotient atom
    // of §4 arrives this stops compiling, which is the right moment to decide
    // what inverting `⌊e/d⌋` means — §7.1 says nothing about it, and guessing
    // silently would be worse than a build failure.
    let Atom::Param { def: param, .. } = term.atom();
    let coefficient = term.coefficient();

    let numerator = value.checked_sub(pattern.constant()).ok_or(MatchError::Overflow)?;
    // `checked_rem` rather than `%`: `i128::MIN % -1` overflows, and a panic in
    // the type checker is not a diagnostic.
    if numerator.checked_rem(coefficient).ok_or(MatchError::Overflow)? != 0 {
        return Err(MatchError::Indivisible {
            param,
            coefficient,
            offset: pattern.constant(),
            value,
        });
    }
    let solved = numerator.checked_div(coefficient).ok_or(MatchError::Overflow)?;
    Ok(Match { param, value: solved })
}
