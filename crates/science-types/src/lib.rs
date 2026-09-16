//! `science-types` — the type checker, starting with its representation.
//!
//! This crate holds two layers and no checking. The first is the one thing
//! `const-expression-arithmetic.md` §10.1 says has to exist *before* a type
//! checker is written: **the const-expression normal form**, `k + Σ cᵢ·aᵢ`,
//! and the four commitments that hang off it.
//!
//! | §10.1 | What it is | Where it lives |
//! |---|---|---|
//! | item 3 | `NORMALISE` and `EQUAL` over the quotient-free fragment | [`normal`] |
//! | item 4 | the normal form *is* the monomorphisation key | [`mono`] |
//! | item 7 | one-variable linear matching for const-argument inference | [`matching`] |
//! | item 8 | `SC0260` and `SC0261`, with §9.2's normal-form-and-legend block | [`diagnostics`] |
//!
//! The second is **the type representation and the lowering into it** —
//! `type-checking-and-mir.md` §2 and Decision 24 — which is what inference and
//! bidirectional checking will both stand on:
//!
//! | Note | What it is | Where it lives |
//! |---|---|---|
//! | Decision 24 | an interned, index-addressed [`Ty`] over a `mutable self` table | [`ty`] |
//! | Decision 6 | `T?` as a distinct type, and `T??` collapsed with `SC0520` | [`lower`] |
//! | §8 item 4 | a const generic argument interned as a [`NormalForm`], not as syntax | [`ty`] |
//!
//! The second layer is the first layer's first consumer, and that is the point
//! of the ordering: `Matrix of (T, a + 1)` and `Matrix of (T, 1 + a)` become
//! one interned type because [`normalise`] runs before the hash key is built.
//! An interner keyed on syntax would have been cheap today and a rewrite of the
//! monomorphisation key later.
//!
//! Items 1, 2, 5 and 6 landed in the parser and the resolver before this crate
//! existed. [`const_expr`] is where the two halves meet: it is the checker's
//! own tree for §2.1's grammar, and [`const_expr::lower`] is the one function
//! that turns a resolved [`hir::ConstExpr`](science_resolve::hir::ConstExpr)
//! into it.
//!
//! # 1. Why the normal form comes before the checker
//!
//! Because the checker cannot be written without it and cannot be retrofitted
//! with it. Type equality on `Matrix of (T, ROWS, COLS)` is const equality on
//! day one, and §3.5 names four callers that must all be *the same* procedure:
//! type equality, shape equality, the index-obligation discharger of
//! `indexing-and-array-literals.md` §1.3, and the monomorphisation key. Four
//! implementations of "are these two extents the same?" is four answers, and
//! the fourth one is a linker that emits two symbols for one function.
//!
//! # 2. The one property everything rests on
//!
//! **Linearity is enforced by the productions, not by a check after parsing**
//! (§2.1). `*` requires a literal on one side, so the grammar cannot write
//! `L * K` for two parameters, so [`normalise`] never receives a non-linear
//! input and has no case for one. [`ConstExprKind::Scale`] carries its literal
//! factor *in the node* for exactly that reason: a `Mul(Box, Box)` node would
//! move linearity from the shape of the tree to a check run afterwards, and
//! the check is the thing §2.1 is refusing.
//!
//! The consequence is §3.4, which is the whole theory and is two lines: a
//! quotient-free normal form denotes a function `ℤⁿ → ℤ`; evaluating at the
//! origin recovers `k` and at the `i`-th basis vector recovers `cᵢ`; so the map
//! from normal forms to denoted functions is injective and structural equality
//! on the normal form is sound *and complete*.
//!
//! # 3. What is deliberately not here
//!
//! - **Quotient atoms and `/`** (§4). F0 is the quotient-free fragment and this
//!   crate is F0. [`Atom`] is an enum with one variant so that `Quotient`
//!   arrives as an added arm — and so that `Param` sorting before it is already
//!   true, which is §3.1's atom order. The grammar's `/` production has no
//!   node in [`ConstExprKind`] yet, because a node with no normal form to go
//!   to would be a node every match arm has to pretend about.
//! - **`SC0262` and the monomorphisation-time obligations** (§5.3, §9.4).
//!   [`matching::MatchError::Indivisible`] *is* `SC0262`'s condition and carries
//!   everything its message needs, but the instantiation chain that makes the
//!   diagnostic survivable is F1's, and a chain rendered before there is a
//!   monomorphiser to walk would render whatever this crate happened to keep.
//! - **Inference, bidirectional checking, THIR, MIR, monomorphisation,
//!   interface resolution, narrowing.** None of it. [`ty`]'s §8 states that
//!   seam precisely, because the next phase starts at it and a seam a reader
//!   has to guess at is a seam that moves.
//!
//! # 4. Almost nothing calls any of this yet
//!
//! That is expected and it is the point of §10.1: these are the things that
//! are cheap now and expensive later. The test suite is the only consumer, and
//! it is written as the consumer the checker will be.

pub mod const_expr;
pub mod diagnostics;
pub mod lowering;
pub mod matching;
pub mod mono;
pub mod normal;
pub mod ty;

pub use const_expr::{lower, ConstExpr, ConstExprKind};
pub use lowering::TypeLowerer;
pub use matching::{match_linear, Match, MatchError};
pub use mono::MonoKey;
pub use normal::{equal, normalise, Atom, AtomOrder, ConstEvalError, NormalForm, Term};
pub use ty::{GenericArg, Ty, TyKind, Types};

/// The diagnostics this crate emits.
///
/// `science-resolve`'s own `codes` module records the split of the shared
/// `SC0200`-`SC0299` range: `SC0200`-`SC0249` is resolution, `SC0250`-`SC0299`
/// is this crate. Within it, `const-expression-arithmetic.md` §9 claims
/// `SC0260`-`SC0262`, and §10.1 item 8 puts the first two in F0.
///
/// **`SC0260` is smaller here than §9's table makes it look, and that is
/// correct.** The table lists six conditions; three of them are the parser's
/// (an operator outside §2.1, an operator in the bare `of X` form, `*` or `/`
/// with no literal operand — which this crate's tree cannot even represent),
/// one is the resolver's and already reported as `SC0220`
/// (`NOT_A_CONST_PARAM_KIND`), and one is division, which is F1. What is left
/// for this crate is the literal clause — *"a non-integer or suffixed
/// literal"* — plus the arithmetic range, which §9 does not name and §5 of
/// this module's [`normal`] documentation argues belongs here.
///
/// **`SC0261` is reported only when no higher-level code owns the site**
/// (Decision 9.1). `broadcasting.md`'s `SC0294` and `unit-literals.md`'s
/// `SC0256` are the same failure seen from a level up, and three codes for one
/// user-visible mistake would be a diagnostic disaster. That is why
/// [`diagnostics::normal_form_block`] is public separately from
/// [`diagnostics::cannot_show_equal`]: the block is the explanation, and a
/// higher-level code attaches it to *its* diagnostic rather than this crate
/// pushing a second one.
///
/// **The second band is `SC0520`-`SC0579`**, which `type-checking-and-mir.md`
/// §13 claims for the type checker. That note names three — `SC0520` for
/// `T??`, `SC0521` for a `TryIterate` loop outside a failure context, and
/// `SC0522` for a generic function across the C boundary — and only the first
/// belongs to a phase that exists. [`CONST_WHERE_TYPE_EXPECTED`] is allocated
/// from the same band and is **not** in §13's list; [`lowering`]'s §4 says
/// what it covers and why the hole it fills is real rather than invented.
pub mod codes {
    use science_diagnostics::Code;

    /// A const expression the language does not admit.
    ///
    /// The clauses this crate reports: a const argument whose literal is not
    /// an unsuffixed integer, and a const expression whose value leaves the
    /// range of `i128`.
    pub const NOT_A_CONST_EXPRESSION: Code = Code(260);

    /// Two const expressions cannot be shown equal.
    ///
    /// Prints both normal forms and the legend binding each symbol to its
    /// definition site (§9.2). On the quotient-free fragment this is never a
    /// "cannot prove" — §3.4's injectivity makes it a definite inequality —
    /// but the code and the rendering are the ones that will carry the
    /// incompleteness when quotient atoms arrive (§4.3), so the message is
    /// worded to survive that.
    pub const NOT_PROVABLY_EQUAL: Code = Code(261);

    // `SC0262` — a const expression inadmissible at an *instantiation*: a
    // negative extent, or no integer solution at an inference site — is
    // deliberately not defined here. §10.2 puts it in F1 with the
    // monomorphisation-time obligations, and §9.4 makes the instantiation
    // chain the whole of its message. The condition this crate can already
    // detect is `matching::MatchError::Indivisible`, which carries the
    // parameter, the coefficient and the offset that message needs.

    // --- the types band, SC0520-SC0579 -----------------------------------

    /// `T??` — Decision 6.
    ///
    /// The parser builds one `Nullable` node per `?` deliberately, *"so that
    /// `T??` … is rejected by the phase that can say why"*. This is that
    /// phase; [`crate::lowering`]'s §3 is the why.
    pub const DOUBLE_NULLABLE: Code = Code(520);

    /// A const expression, or a const generic parameter, where a type is
    /// expected.
    ///
    /// **Not one of the three codes `type-checking-and-mir.md` §13 names**,
    /// and allocated from the band that note claims. The hole is real:
    /// `parse_type_atom` accepts a const expression wherever it parses a type
    /// because inside an `of (..)` list there is no ambiguity, and outside one
    /// there is nobody before this phase who can say so. See
    /// [`crate::lowering`]'s §4.
    pub const CONST_WHERE_TYPE_EXPECTED: Code = Code(523);

    // `SC0521` (`TryIterate` outside a failure context) and `SC0522` (a
    // generic function across the C boundary) are §13's and are deliberately
    // not defined here. Both are conditions about an *expression* — a `for`
    // loop, a callback argument — and this crate has no expressions. A code
    // defined before the phase that reports it is a code whose message is
    // written against a guess.
}
