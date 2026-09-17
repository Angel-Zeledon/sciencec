//! `science-types` — the type checker, starting with its representation.
//!
//! This crate holds four layers, and the first three were built before the
//! fourth walked an expression. The first is the one thing
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
//! The third is **the relations over that representation** — everything
//! `ty`'s §8 said the next phase owns, up to but not including an expression:
//!
//! | Note | What it is | Where it lives |
//! |---|---|---|
//! | Decision 24 | inference variables, and a union-find with `mutable self` | [`infer`] |
//! | Decisions 6, 14 | assignability: `compatible` plus two coercions and no more | [`assign`] |
//! | §6 of [`ty`] | alias expansion, with a cycle check that is `SC0524` | [`alias`] |
//! | §8 of [`ty`] | substitution, the type half and the const half together | [`subst`] |
//!
//! [`fold`] is the traversal the last two share, and §5 below is the seam the
//! fourth layer started at.
//!
//! The fourth is **the checking itself** — everything §5 said the next phase
//! owned, which is now this one:
//!
//! | Note | What it is | Where it lives |
//! |---|---|---|
//! | Decision 3 | THIR: a type on every node, conversions explicit | [`thir`] |
//! | Decision 1 | bidirectional checking, one [`Inference`] per body | [`check`] |
//! | Decisions 7, 8 | flow narrowing over places, and rule 4's dependency | [`narrow`] |
//! | Decisions 9, 10 | `SC0140`, with §5's four exclusions | [`unchecked`] |
//! | Decision 11 | method lookup, the ambiguity that is an error, and the instance selection it does not name | [`methods`] |
//! | §12 | the seam for MIR, stated as §5 states this one | [`thir`]'s §5 |
//!
//! [`items`] is the table of lowered declarations the fourth layer checks
//! against, and it exists so that an annotation is lowered — and complained
//! about — once rather than once per caller.
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
//! - **MIR and monomorphisation.** None of it. [`thir`]'s §5
//!   states the seam MIR starts at to the standard §5 below sets, and names
//!   which of `region-inference.md` §10's six requirements THIR can guarantee
//!   and which are MIR's by construction.
//!
//!   **Exhaustiveness used to be third on that line**, and it is
//!   [`exhaustive`]: Decision 16's usefulness and exhaustiveness by Maranget's
//!   algorithm, `SC0250` with the witness in the message, and `SC0537` for an
//!   arm no value reaches. It is the third of the three THIR passes §3.1 named,
//!   so that line is now empty of everything except the two IRs this crate does
//!   not build.
//!
//!   **Method lookup used to be listed here** — *"the hole everything else in
//!   the fourth layer is shaped around"*, the reason `doc.title()` had no type
//!   where `doc.title` did. It is [`methods`], and what it closed is recorded
//!   below rather than here, because four separate conservatisms in this crate
//!   were written against its absence and each of them said so in its own
//!   words.
//!
//! # 4. What calls this
//!
//! The first three layers were written before there was a caller, which is the
//! point of §10.1: they are the things that are cheap now and expensive later.
//! The fourth layer is that caller. [`check::check_crate`] is the entry point —
//! declarations, then bodies, then the THIR analyses — and every obligation §5
//! raised below is now discharged by a named function, or is still open and
//! said so in [`check`]'s §6.
//!
//! # 5. The seam, as it was stated — and as it was taken up
//!
//! [`ty`]'s §8 stated the seam the third layer was written against. This was the
//! next one, and it is left standing rather than rewritten, because a seam is
//! worth more as a record of what was promised than as a description of what
//! was built. Each item below now names the thing that took it up; where the
//! answer is *"not yet"*, it says so.
//!
//! **The order of operations at an annotation** is fixed and is three calls:
//! [`TypeLowerer::lower`] turns a `hir::Type` into a [`Ty`];
//! [`Aliases::reveal`] turns that into the alias-free type the relations
//! compare; [`assignable`] answers whether a value fits, given the
//! [`Site`]. Skipping the middle call is not a compile error and not a wrong
//! type — it is `Embedding` failing to match `Array of F32` at one site in ten,
//! wherever the author happened to write the alias.
//!
//! **What the next phase owns, and what took each of them up:**
//!
//! - **`check_expr` and `synth_expr`** call [`assignable`], and owe it the
//!   [`Site`] at every use. Passing one everywhere removes a decision from the
//!   language in silence; [`assign`]'s §6 names both directions of that.
//!   — *Taken up by [`check`]'s §1, which funnels every call through one
//!   function so that there is one thing to audit.*
//! - **A `return` and a call argument** are the two sites that box, and the
//!   coercion applies to *each element* of a returned tuple rather than to the
//!   tuple — [`assign`]'s §2, which is Decision 14's own example.
//!   — *Taken up by [`check`]'s §2, and tested by name.*
//! - **The body's variables** are an [`Inference`], one per body, dropped with
//!   it. [`infer`]'s §1. — *[`check`]'s §3.*
//! - **Decision 2's defaulting** walks [`Inference::unresolved`] at the end of
//!   the body and needs the prelude's ids for `I64` and `F64`.
//!   — *[`check`]'s §4, with [`items::Prelude`] finding the ids the way
//!   [`Coercions`] already found `Error`.*
//! - **A call to a generic** builds a [`Substitution::of_generics`] and owes
//!   the **arity and kind check**, which `lowering`'s §1 defers to whoever
//!   holds the declaration and the use at once. This layer zips and does not
//!   check. — ***Still open.*** [`items`] holds the declaration and [`check`]
//!   holds the use, and neither checks: [`check`]'s §6 says why, and the
//!   inference of an omitted type argument is root-level only for the reason
//!   [`infer`]'s §2 gives.
//! - **Method lookup (Decision 11)** supplies [`Substitution::with_self`] with
//!   the implementation block, which is the `owner` a `SelfType` already
//!   carries. — *Taken up by [`methods`], which is the index, and by
//!   [`check`]'s `method_call`, which is the call site. A method call carries
//!   the definition it resolved to, its arguments are checked against that
//!   definition's parameters, and `Self` at the call becomes the receiver's
//!   type. Four things in this crate were conservative because it did not
//!   exist and three of them are now not; the fourth is in [`check`]'s §6.*
//! - **`SC0140`, `SC0521`, `SC0522`, narrowing and exhaustiveness** need an
//!   expression, which this crate has never had. — *`SC0140` is [`unchecked`],
//!   narrowing is [`narrow`]. **`SC0522` shipped, and not from here** — it needs
//!   the monomorphiser's view of which instantiations cross into C, so
//!   `science-codegen` emits it and this crate's reservation test, which
//!   asserts the code is absent from its own list, still passes. **Decision
//!   16's exhaustiveness is [`exhaustive`]**, which reports `SC0250` — the one
//!   code this crate takes from the low half of the shared types band, and the
//!   `codes` module below says which three documents put it there. `SC0521` is
//!   the last of this line still open, and it is waiting on Decision 15's
//!   `TryIterate`, which does not exist.*
//!
//! **Four obligations this layer raised, and the two that are now
//! discharged**, each named where it is raised rather than collected into a
//! list nobody reads:
//!
//! 1. *"`S` implements `Error`"*, owed by every [`Coercion::Box`]. [`assign`]'s
//!    §3. — **Discharged** by [`Methods::implements`]: the rule asks whether
//!    the crate declares `S implements Error:`, and this crate no longer
//!    agrees that an `Int` may be boxed.
//! 2. *"`C` implements `I`"*, owed by every [`Coercion::Unsize`] and by every
//!    [`Coercion::UnsizeInBox`]. [`assign`]'s §4 and §4a, which admit
//!    `borrowed C` into `borrowed any I` and `Box of C` into `Box of any I`
//!    because those conversions allocate nothing and change no value. It is the
//!    same obligation as the one above and wider — one interface there, every
//!    interface a program declares here — which is why it was listed
//!    separately. — **Discharged by the same predicate**, and the width is why
//!    the predicate had to be an index rather than a list of names. The second
//!    rule raised no new obligation, which is the clearest statement of what it
//!    is: one question, asked about two indirections.
//! 3. *"this implementation supplies every associated type its interface
//!    declares"* — §5.4's completeness check, which `science-resolve` names as
//!    this crate's. An unbound `Self.Item` is left standing by [`subst`]'s §2
//!    precisely so that the phase which can see both blocks reports it. —
//!    ***Still open***: [`items::Declarations::body_substitution`] now pairs the
//!    two blocks, so the *information* is here; what is missing is the check
//!    and a code for it.
//! 4. *"this instantiation's const arguments are in range"*. [`Substitution`]
//!    returns [`ConstEvalError`] and reports nothing ([`subst`]'s §4); a caller
//!    with a span turns it into `SC0260` through
//!    [`diagnostics::overflowed`], and F1's `SC0262` replaces that with the
//!    instantiation chain §9.4 asks for. — ***Still open***.
//!
//! **And one thing the next phase must not do.** It must not put an inference
//! variable in the type table. [`infer`]'s §1 is the argument and §2 is the
//! cost it buys; the escape route for a nested hole is written there, and it is
//! not `TyKind::Infer`.

pub mod alias;
pub mod assign;
pub mod check;
pub mod conform;
pub mod const_expr;
pub mod diagnostics;
pub mod exhaustive;
pub mod fold;
pub mod infer;
pub mod items;
pub mod lowering;
pub mod matching;
pub mod methods;
pub mod mono;
pub mod narrow;
pub mod normal;
pub mod subst;
pub mod thir;
pub mod ty;
pub mod unchecked;

pub use alias::Aliases;
pub use assign::{assignable, Coercion, Coercions, Site};
pub use check::{check_crate, check_fn};
pub use const_expr::{lower, ConstExpr, ConstExprKind};
pub use fold::{fold_children, TypeFolder};
pub use infer::{InferTy, InferVar, Inference, UnifyError};
pub use items::{Declarations, Prelude, Signature};
pub use lowering::TypeLowerer;
pub use matching::{match_linear, Match, MatchError};
pub use methods::{Candidate, Form, Found, Methods, Source};
pub use mono::MonoKey;
pub use narrow::{Fact, Facts};
pub use normal::{equal, normalise, Atom, AtomOrder, ConstEvalError, NormalForm, Term};
pub use subst::Substitution;
pub use thir::{Body, ExprId, ExprKind, Place};
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

    /// A `match` with a value no arm covers — Decision 16.
    ///
    /// **The only code this crate takes from the low half of the shared types
    /// band, and the reason is that three documents put it there.** §9 of
    /// `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` settles a
    /// disagreement between two specs in a sentence written for exactly this:
    /// *"Exhaustiveness errors belong to the type checker and so take a code in
    /// the `SC0250` range, not the `SC0210` the previous spec assigned them."*
    /// `docs/superpowers/design/README.md`'s free table lists `SC0250` as
    /// unclaimed, and `examples/00_kitchen_sink.science` — acceptance material
    /// no compiler change may edit — tells its reader in a comment that *"a
    /// missing variant is an error in the `SC0250` range"*.
    ///
    /// **`SC0210` stays reserved and stays unused.** `science-resolve`'s
    /// `codes` module holds it open *"for `science-types` to use as §4.4 spells
    /// it"* and its reservation test asserts the resolver never takes it; both
    /// are still true. That comment is now stale in the direction of guarding a
    /// number nobody will spend, which is the harmless direction, and the note
    /// it belongs to is not this crate's to edit.
    ///
    /// The message **is the witness** — [`crate::exhaustive`]'s §1 — because a
    /// diagnostic that says only *"not exhaustive"* makes the reader re-derive
    /// what they missed.
    pub const NON_EXHAUSTIVE_MATCH: Code = Code(250);

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

    /// A type alias that expands into itself.
    ///
    /// **Also not one of §13's three**, and from the same band for the same
    /// reason [`CONST_WHERE_TYPE_EXPECTED`] is: no earlier phase can report it.
    /// Every name in `type A is B` and `type B is A` resolves, so the resolver
    /// has nothing to say; the mistake is a property of the *expansion*, and
    /// [`crate::alias`] is the first thing that expands. Its §2 and §4 are the
    /// argument, and the check runs over the declarations rather than over the
    /// uses so that one cycle is one diagnostic.
    pub const CYCLIC_ALIAS: Code = Code(524);

    // `SC0521` (`TryIterate` outside a failure context) and `SC0522` (a
    // generic function across the C boundary) are §13's and are **still**
    // deliberately not defined here, although this crate now has expressions.
    //
    // `SC0521` needs Decision 15's `TryIterate`, which does not exist: the
    // prelude declares `Iterate` and no sibling, so there is no interface for a
    // `for` loop to be over and no failure edge for it to be outside of.
    // `SC0522` is a *declaration* check — a generic function reached through an
    // `extern` block — and needs the monomorphiser's view of which
    // instantiations cross. Both conditions are now one phase closer and
    // neither is one phase away; a code defined before the check is a code
    // whose message is written against a guess, which was the reason before and
    // is the reason now.

    // --- the expression checker, `SC0525`-`SC0530` -----------------------
    //
    // §13's three names stop at `SC0522`, and `SC0523`/`SC0524` were taken by
    // `lowering` and `alias` from the same band for holes the note had not
    // foreseen. These six continue that: each one is a condition the note
    // assumes a checker reports and does not number, because §13 was written
    // before anything walked an expression.

    /// A value that does not fit the slot it is in.
    ///
    /// The central diagnostic of Decision 1 and the reason that decision was
    /// taken: *"error messages can name a declared type"*, because every
    /// expectation in a bidirectional checker came from an annotation a human
    /// wrote. [`crate::check`]'s §1 makes every report of this code come from
    /// one function, so that the site — and therefore Decision 14's boxing — is
    /// decided in one place.
    pub const MISMATCHED_TYPES: Code = Code(525);

    /// A value whose type is a hole nothing filled.
    ///
    /// Decision 2 defaults an unconstrained *numeric* literal to `I64` or
    /// `F64`; this is everything else, and `infer`'s §2 names the shape it
    /// covers: *"`let xs be []` with no annotation, where the element type is a
    /// hole inside a known constructor … the checker must report that it cannot
    /// infer, rather than deferring"*. Reported once per inference class, not
    /// once per expression, because *"a class is one unknown however many
    /// expressions joined it"*.
    pub const TYPE_ANNOTATIONS_NEEDED: Code = Code(526);

    /// A call with the wrong number of arguments.
    pub const WRONG_ARGUMENT_COUNT: Code = Code(527);

    /// A field the receiver's type does not have.
    ///
    /// The resolver cannot report it — `ExprKind::Field` keeps a bare `Ident`
    /// precisely because *"the answer depends on the type of the receiver"* —
    /// so this is the first phase that can, and `SC0205` (a field a record
    /// *literal* names and the record lacks) is its sibling one phase up.
    pub const NO_SUCH_FIELD: Code = Code(528);

    /// `let a, b be f()` where the value is not a pair.
    ///
    /// Handed here by name: `hir::LetBinding` says *"whether the value actually
    /// is a tuple of the right width is not checked here … the arity check
    /// belongs to `science-types`"*.
    pub const BINDING_COUNT_MISMATCH: Code = Code(529);

    /// `e?` where `e` is not nullable.
    ///
    /// Always true, so it is never what anyone meant — the same argument
    /// [`DOUBLE_NULLABLE`] makes about `T??`, one level down at the value.
    pub const PRESENCE_TEST_ON_NON_NULLABLE: Code = Code(530);

    // --- Decision 11's three, `SC0531`-`SC0533` -------------------------
    //
    // §13 numbers none of them, for the reason it numbers none of the six
    // above: it was written before anything walked an expression, and a method
    // call is the construct it assumed a checker resolved without saying what
    // happens when it cannot. The third is newer than the other two and is
    // `methods`'s §6 — the case where the lookup finds one method at several
    // instantiations of one interface and the arguments are what tell them
    // apart.

    /// One method name, two implementations, and no rule that picks.
    ///
    /// **The load-bearing half of Decision 11**: *"an ambiguity is an error,
    /// never a priority ordering"*. A priority ordering resolves the call
    /// silently and the author finds out at run time that the wrong code ran;
    /// this makes them say which they meant. The message names **both**
    /// candidates and the block each came from, because that is the whole value
    /// of the decision — [`crate::methods`]'s §3, and its §5 for the one thing
    /// the message cannot offer, which is a spelling for the answer.
    pub const AMBIGUOUS_METHOD: Code = Code(531);

    /// A method the receiver's type does not have.
    ///
    /// [`NO_SUCH_FIELD`]'s sibling, and reported under the same restraint: only
    /// where the question is answerable. **What changed is how much that
    /// restraint excused.**
    ///
    /// It used to excuse every receiver of prelude type, because
    /// [`crate::methods::Methods::surface_is_closed`] read openness off
    /// `Def::is_builtin` — so `"hi".no_such_method()` checked clean, and with
    /// it every misspelling of every method in the standard library. That is
    /// not a narrow silence: it is the entire library surface exempt from the
    /// check a `Doc` gets, and the unresolved call carries [`crate::Ty::ERROR`]
    /// into whatever reads its type next.
    ///
    /// [`crate::methods::Methods::name_is_answerable`] is the restraint now,
    /// and `methods`' §8a is the decision: a name a note gives a prelude type
    /// and `builtins.rs` has not transcribed is *"not written down"* and stays
    /// silent — `text.slice(0..4)` is still a correct program and is still not
    /// reported — and every other name on a prelude receiver is this code.
    /// `builtins.rs`' `UNWRITTEN` is the list, cited entry by entry, and it
    /// shrinks as `BLOCKS` grows.
    ///
    /// **One half of it is still open and is named rather than absorbed.**
    /// [`crate::methods::Methods::receiver`] answers `None` for a builtin head
    /// with no block at all, one step before the predicate above, so
    /// `(1).no_such_method()` on an `I64` reports nothing. `methods`' §8b is
    /// why: `stdlib-core.md` §8.2 specifies the numeric surface as *"roughly
    /// thirty-five"* methods and enumerates seventeen, so a list built from the
    /// note would report on `x.floor()`, which the note intends and never
    /// writes. Trading that silence for a false positive is the trade the whole
    /// restraint exists to refuse.
    ///
    /// **Every arm is reachable through a type as well as through a value**
    /// now that [`crate::check`]'s `type_receiver` accepts a
    /// [`DefKind::Primitive`] receiver, which is the same judgement arriving
    /// through one more spelling and not a second decision.
    ///
    /// [`DefKind::Primitive`]: science_resolve::hir::DefKind::Primitive
    pub const NO_SUCH_METHOD: Code = Code(532);

    /// The arguments fit no implementation of the interface. `methods`'s §6.
    ///
    /// **Decision 11's third case, which the decision does not have.** Where
    /// every candidate for a name is an implementation of one interface at
    /// different type arguments — `LoadError implements From of IoError:` and
    /// `LoadError implements From of ParseError:` — the name is one method at
    /// several instantiations and the *arguments* select among them. One
    /// survivor resolves; several is [`AMBIGUOUS_METHOD`] with a message of its
    /// own; none is this, and it is a type error rather than a lookup failure —
    /// the method exists and nothing was passed that any instantiation of it
    /// takes.
    ///
    /// **Not [`MISMATCHED_TYPES`]**, although it is a value that does not fit a
    /// slot. That code is Decision 1's and its whole shape is *one* expected
    /// type, taken from *one* annotation, which is what makes its message
    /// nameable; here there are as many expected types as there are
    /// implementations and the message has to list them. Reporting it as
    /// `SC0525` would mean choosing one implementation to blame the argument
    /// against, which is the choice this call could not make.
    pub const NO_MATCHING_IMPLEMENTATION: Code = Code(533);

    /// A generic argument that does not satisfy the bound the callee declared.
    ///
    /// **Decision 11's fourth, and the one the decision assumes rather than
    /// numbers.** `def describe of T: Summarize(..)` is a promise the body is
    /// checked against — `value.preview()` resolves because `T` implements
    /// `Summarize` — and until this code existed nothing held a *call* to that
    /// promise. `describe(n)` at an `I64` checked clean, which made `T` unify
    /// with anything and made the bound decoration.
    ///
    /// **Not [`MISMATCHED_TYPES`]**, and the argument is
    /// [`NO_MATCHING_IMPLEMENTATION`]'s one step further: `SC0525`'s shape is
    /// one *expected type* taken from one annotation, and there is no expected
    /// type here. An interface is not a type a value can have (Decision 13
    /// makes `any Summarize` the type, and the argument is not one), so the
    /// message names the interface, the type that does not implement it, and
    /// the bound that required it. Reporting it as `SC0525` would mean printing
    /// `expected Summarize` at a slot no `Summarize` fits.
    ///
    /// **Reported only where the question is answerable**, which is the
    /// restraint [`NO_SUCH_METHOD`] is under and for the same reason one level
    /// out: `builtins.rs` declares seventeen interfaces and no implementation
    /// of any, so a bound at a prelude interface — `T: Ord`, `T: Clone` — is a
    /// question this compiler cannot ask rather than one it answers with no.
    /// [`crate::methods::Methods::answers_for`] is where the two are told
    /// apart, and its §7 says what that costs.
    pub const UNSATISFIED_BOUND: Code = Code(534);

    // --- the operators, `SC0535` -----------------------------------------

    /// An operator whose operand's type does not implement the interface the
    /// operator dispatches to.
    ///
    /// **`check`'s §6 used to list `a + b` and `a[i]` as unchecked and price
    /// the hole; this is the code that closes it.** §5.4 makes every arithmetic
    /// operator, `is`, and `[` an interface method, so `a + b` on a type that
    /// implements nothing is not an unknown operation — it is a program that
    /// names an implementation that does not exist, which is the same sentence
    /// [`UNSATISFIED_BOUND`] says one construct along.
    ///
    /// **Not [`MISMATCHED_TYPES`]**, for [`UNSATISFIED_BOUND`]'s reason: there
    /// is no expected type at an operator. The two operands of `a + b` are not
    /// a value and a slot, and `expected Add, found Row` would be printing an
    /// interface where a type goes. The message names the operator, the type,
    /// and the implementation block the author would have to write.
    ///
    /// **Not [`NO_SUCH_METHOD`]** either, although the lookup that fails is the
    /// same one. `SC0532` is a name the author typed and the type does not
    /// have; here the author typed `+`, and telling them that `Row` has no
    /// method `add` would be explaining the desugaring rather than the mistake.
    ///
    /// **Reported only where the question is answerable**, which is the
    /// restraint [`NO_SUCH_METHOD`] and [`UNSATISFIED_BOUND`] are under and for
    /// the same reason: a prelude head whose implementations
    /// [`crate::methods::Methods::receiver`] cannot speak for is silence, not a
    /// no, so `"a" + "b"` is still unreported although `String implements Add`
    /// carries no method the prelude has transcribed.
    pub const NO_OPERATOR_IMPLEMENTATION: Code = Code(535);

    // --- the associated call, `SC0536` -----------------------------------

    /// A generic type reached through its bare name at an associated call,
    /// whose own type arguments nothing at the call fixes.
    ///
    /// **`Box.new(doc)` names the type `Box`, and `Box` is `Box of T`.** An
    /// associated function has no receiver *value*, so the block's parameters
    /// are not read off one; they come from the instantiation the author wrote
    /// — `(Array of Int).new()`, which is how `examples/07_generics.science`
    /// spells it — or from the call's arguments by
    /// [`crate::check`]'s §6 root-level match, and from nowhere else. This is
    /// the case where neither gave an answer.
    ///
    /// **What it replaces is a type with a free parameter in it.** Before this
    /// code, an unwritten, unsolved `T` stayed a [`TyKind::Param`](crate::TyKind::Param)
    /// in the call's result, so `let xs be Array.new()` bound `xs` at `Array of
    /// T` for a `T` bound in no scope the author can see, and the first
    /// mismatch downstream printed that `T` at the author. A parameter that
    /// escapes its binder is not a type, and the sentence to say about one is
    /// not `expected Array of Int, found Array of T`.
    ///
    /// **Not [`TYPE_ANNOTATIONS_NEEDED`]**, although both are *"nothing fixed
    /// this"*. `SC0526` is `infer`'s hole inside a known constructor and its
    /// fix is an annotation on the binding; this one's fix is on the
    /// **receiver**, it is available whatever the binding says, and the
    /// language already has the spelling — so the message points at the
    /// receiver and offers it.
    ///
    /// **`docs/superpowers/design/README.md` is not edited**, which is the
    /// convention `science-codegen`'s `diagnostics` states: *"the row belongs
    /// to whoever maintains the allocation record"*. Its table reads
    /// `SC0523`–`SC0535` for this crate and that range is now one short. It is
    /// inside `type-checking-and-mir.md` §13's band either way, and the report
    /// that landed this code names the row as the thing to extend.
    pub const UNINFERABLE_RECEIVER: Code = Code(536);

    // --- usefulness, `SC0537` --------------------------------------------

    /// An arm of a `match` that no value reaches.
    ///
    /// **Decision 16's other half, which the decision does not mention.**
    /// Maranget's algorithm answers *usefulness*, and exhaustiveness is one
    /// question asked of it; an arm useless against the arms above it is the
    /// same answer about a different row. [`crate::exhaustive`]'s §4 is the
    /// argument for it being an error rather than a warning — no macros, no
    /// attribute that silences a diagnostic, and a body that is checked and
    /// lowered although nothing runs it — and the same section prices it, both
    /// in compile time and in the escape hatch the language cannot offer.
    ///
    /// **Not [`NON_EXHAUSTIVE_MATCH`]**, although one recursion produces both.
    /// That code is about a value the program has no answer for, and its
    /// message is a value; this one is about a line that cannot run, and its
    /// message is a place. Sharing a code would mean one of the two messages
    /// had to stop being about its own subject.
    pub const UNREACHABLE_ARM: Code = Code(537);

    // --- the range index, `SC0538` ---------------------------------------

    /// A range used as an index, which would be a slice.
    ///
    /// **Decision. `xs[1..3]` is refused, and `Slice of T` is not built.**
    /// `indexing-and-array-literals.md` §2.3 makes `a[1..5]` a `Slice of T` —
    /// `{ borrowed T, Int }`, a view that does not copy — and that type exists
    /// nowhere: the prelude declares no `Slice`, so `Slice of Int` in a
    /// signature is `SC0200` from the resolver, and there is no implementation
    /// of `Index` at a range for anything.
    ///
    /// **The reason it is a refusal and not a type.** What was there before was
    /// worse than either: a range expression carries no type (`check`'s
    /// `synth` gives it [`crate::Ty::ERROR`] because §6 has no `Range` in the
    /// prelude to give it), and `Index.index` wants an `Int`, so the untyped
    /// index and the `Int` **agreed by cancelling** — `ty`'s §5 absorption
    /// working exactly as designed on two values neither of which was wrong —
    /// and `xs[1..3]` came out as one *element*. A wrong type with no
    /// diagnostic is the one outcome this crate's §6 is written to keep from
    /// happening silently.
    ///
    /// **What it costs, stated plainly.** Slicing is specified in full and
    /// does not work, and this code is where the author finds out. Building
    /// `Slice of T` instead is a prelude type with a region parameter, an
    /// `Index` implementation at a range for every container, a `MutableSlice`
    /// beside it through `IndexMutably`, §2.4's borrow rules over a view that
    /// pins its source, and `SC0285` for a provably out-of-range slice — five
    /// things in four crates, of which this one owns one. Reporting is the
    /// smaller honest answer and the larger one stays available: when `Slice`
    /// exists this code is deleted rather than narrowed, because its whole
    /// condition is *"the type it would produce does not exist"*.
    ///
    /// **Not [`NO_OPERATOR_IMPLEMENTATION`]**, although the lookup that fails
    /// is `Index`'s. `SC0535` says *"write the `implements Index:` block"*, and
    /// there is no block anyone can write: the argument type has no spelling.
    /// Offering a fix that cannot be taken is worse than saying what is
    /// missing.
    ///
    /// **The domain is syntactic and that is deliberate.** It is a range
    /// *written in an index bracket*, which is the only place one can be: both
    /// ends are required outside brackets (§2.2, still unimplemented), so
    /// `let r be 1..3` types at [`crate::Ty::ERROR`] and there is no range
    /// value to reach an index through a name. A method that takes a range —
    /// `text.slice(0..4)` — is a call and not an index, and is untouched.
    pub const RANGE_INDEX_NEEDS_SLICE: Code = Code(538);

    /// A value interpolated into an `f"…"` that does not implement `Display`.
    ///
    /// **From `strings-formatting-and-docs.md`'s block, not from this crate's
    /// own.** `docs/superpowers/design/README.md` allocates `SC0274`-`SC0275`
    /// to that note and §7 of the note explains why the pair is in the types
    /// band rather than the syntax one: *"they cannot be decided before type
    /// checking — the spec is syntactically well-formed and the question is
    /// whether the *argument* fits it"*. This is the second of the pair.
    ///
    /// **`SC0274` is deliberately not defined**, and the reason is the same
    /// discipline `SC0521` and `SC0522` get above. `SC0274` is the format spec
    /// failing to match its argument's type — *"a float code on an integer, an
    /// integer code on a float, a precision on a non-float non-`String`"* —
    /// and there are no format specs: the lexer refuses every one with
    /// `SC0173`. A code whose condition cannot arise is a code whose message
    /// is written against a guess.
    ///
    /// **§7's row has three clauses and this code now carries two of them.**
    /// The row also names *"`print` given more than one argument"* and *"a
    /// `T?` interpolated without narrowing"*.
    ///
    /// The nullable falls out of this check rather than needing its own arm:
    /// the prelude gives no `T?` a `Display` implementation, so `f"{x}"` on a
    /// nullable is this code with the nullable's rendering in it.
    ///
    /// **`print("rows:", n)` used to be `SC0527` and is now this code**, which
    /// is the correction rather than a widening. It was never `SC0527` in
    /// fact: `print` has no declared signature at all (see `science-resolve`'s
    /// `builtins` for the measurement that keeps it that way), so the ordinary
    /// arity check had nothing to compare against and the call **reported
    /// nothing**, while §7's table said this code covered it.
    /// [`crate::check`]'s `output_is_unary` is the arm, and the message is
    /// §4.1's — *"`print` takes one value"*, with the interpolation as the fix.
    /// Splitting §4.1's decision this way is deliberate: the arity needs no
    /// parameter type, and the `Display` half needs a declaration the back half
    /// of the compiler cannot yet lower.
    ///
    /// **What this code still does not cover** is the `Display` obligation at a
    /// `print` argument, as opposed to at an interpolation. That is the
    /// withdrawn declaration, and [`crate::check`]'s `requires_display` is the
    /// only place the question is asked today.
    ///
    /// **The constant is named for the clause it leads with**, and the arity
    /// message is built by `print_takes_one_value` rather than by
    /// `not_displayable`. One code, because §7 is one row; two constructors,
    /// because they are two sentences.
    pub const NOT_DISPLAYABLE: Code = Code(275);

    // --- the array literal, `SC0281`-`SC0282` -----------------------------
    //
    // **From `indexing-and-array-literals.md`'s block, not from this crate's
    // own.** `docs/superpowers/design/README.md` allocates `SC0280`-`SC0287`
    // to that note; §7.2 is the table and §3 is the decision each of these two
    // implements. They are here rather than in a module of their own for
    // [`NOT_DISPLAYABLE`]'s reason: a code is defined where the check that
    // emits it lives, and both of these are `check`'s `array_lit`.
    //
    // **The other six are deliberately not defined**, under the discipline
    // `SC0521`, `SC0522` and `SC0274` get above — a code whose condition
    // cannot arise is a code whose message is written against a guess.
    // `SC0280` (indexing a type that implements neither `Index` nor
    // `IndexMutably`) is already reported as `SC0535`, which says the same
    // sentence for every operator; `SC0283` (index arity) needs `m[i, j]`,
    // which the parser accepts and §2.5 puts in F1; `SC0284` needs
    // `Tensor.from`; `SC0285` and `SC0286` are the bounds obligation, which is
    // a pass over a bounds fact and not a type (`check`'s §6, item 3 of
    // `index_expr`); and `SC0287` is `SC0535` at `IndexMutably`.

    /// Elements of an array literal that do not agree in type. §3.2.
    ///
    /// **Decision 10 fixes the literal's type as `Array of T` where `T` is the
    /// unification of the element types, and §5.1 admits no implicit numeric
    /// conversion**, so `[1, 2.0]` is an error for the same reason `1 + 2.0`
    /// is. That is the note's own defence of this code and it is why the code
    /// exists rather than the first element simply winning.
    ///
    /// **Not [`MISMATCHED_TYPES`]**, although it is a value that does not fit.
    /// `SC0525`'s shape is one expected type taken from *one annotation a
    /// human wrote*, which is what makes its message nameable; here the
    /// expectation was taken from a sibling element, and the message has to
    /// show that element as well as the one that disagrees. §7.2 asks for
    /// exactly that — *"primary on the first element that differs, secondary
    /// on the element that fixed the type"* — and a code whose message is
    /// `expected Int, found F64` with one span would have thrown the secondary
    /// away.
    ///
    /// **An *annotated* literal is [`MISMATCHED_TYPES`] and not this**, and
    /// the split is the same argument read the other way. `let xs: Array of
    /// String be [1, 2, 3]` pushes `String` inward through Decision 11's arm,
    /// so there *is* one expected type, it did come from one annotation a
    /// human wrote, and there is no sibling element to point at — which is
    /// `SC0525`'s shape exactly. Reporting it here instead would mean printing
    /// a secondary label at an element that fixed nothing.
    ///
    /// So the division is by where the expectation came from: from a sibling,
    /// this; from an annotation, `SC0525`. §3.2's examples are all the first
    /// kind and §3.3's are all the second.
    pub const ARRAY_ELEMENT_MISMATCH: Code = Code(281);

    /// `[]` with no expected type. Decision 11, §3.3.
    ///
    /// **Not [`TYPE_ANNOTATIONS_NEEDED`]**, although both are *"nothing fixed
    /// this"*, and the difference is the one §3.3 is written to preserve.
    /// `SC0526` is `infer`'s hole *inside a known constructor* and is reported
    /// once per inference class at the end of a body; this is a syntactic
    /// property of one expression, known the instant it is reached, and its fix
    /// is stated in the note — *"the annotation, naming a candidate type when
    /// exactly one is visible"*. Folding it into `SC0526` would defer a
    /// question that has already been answered and would lose the span of the
    /// `[]`.
    ///
    /// **The bidirectional push it is the failure case of is bounded on
    /// purpose.** §3.3 names the cost itself: an expected type is driven into
    /// one expression form and no other, because §5.2 describes inference as
    /// unification and this is not that.
    pub const EMPTY_ARRAY_NO_TYPE: Code = Code(282);

    // --- conformance, `SC0539`-`SC0541` -----------------------------------
    //
    // [`crate::conform`]'s three, and they are a band of their own rather than
    // a reuse of Decision 11's because **Decision 11's codes are all reported
    // at a call and these are all reported at a declaration**. `SC0531`-`SC0533`
    // answer *"what does this call site resolve to"*; a block that does not
    // conform is wrong before anything calls it, and the span every one of
    // these points at is a line in a declaration.
    //
    // `type-checking-and-mir.md` §13 numbers none of them, for the reason it
    // numbers none of `SC0523`-`SC0538`: it was written before anything walked
    // a declaration, and it assumed an implementation block was checked
    // without saying what happens when it is not.

    /// An interface method that an `implements` block never implements.
    ///
    /// **Not [`NO_SUCH_METHOD`]**, although the two are the same absence seen
    /// from opposite ends. `SC0532` is reported at a *call*, its subject is a
    /// name the author typed, and its answer is *"the type does not have it"*.
    /// This is reported at a *block*, its subject is a name the author did
    /// **not** type, and its answer is *"the interface requires it"*. Sharing a
    /// code would mean one of the two messages stopped being about its own
    /// subject — which is the argument [`UNREACHABLE_ARM`] makes against
    /// folding into [`NON_EXHAUSTIVE_MATCH`], one construct over.
    ///
    /// **Not [`NO_MATCHING_IMPLEMENTATION`]** either. `SC0533` is about
    /// arguments that fit no instantiation of a method that *exists*; here the
    /// method does not exist and no argument is involved.
    ///
    /// **A defaulted method is not missing.** An interface method with a body
    /// is Decision 15's default and [`crate::methods`]' §2 already makes it
    /// callable on an implementor that never mentioned it, so requiring the
    /// block to restate it would report on `examples/06_traits.science`'s
    /// `Note implements Summarize:` — a block whose own comment says *"only the
    /// required method: the defaults are inherited as written"*.
    ///
    /// **The fix is the declaration itself**, substituted into this block and
    /// rendered in surface syntax, which is a line the author pastes.
    /// [`crate::conform`]'s §5 is the rule that keeps it writable.
    pub const UNIMPLEMENTED_INTERFACE_METHOD: Code = Code(539);

    /// A method in an `implements` block that the interface does not declare.
    ///
    /// **Reported only where the question is answerable**, which is the
    /// restraint [`NO_SUCH_METHOD`] is under and for the identical reason at
    /// the other end of the same table: `builtins.rs` declares fourteen prelude
    /// interfaces as *names with no methods* — deliberately, because `Ord`'s
    /// method would need an `Ordering` and `Display`'s a `Formatter`, and *"a
    /// signature invented in passing is how a language acquires a design nobody
    /// argued for"* — so `Doc implements Clone:` writing a `clone` is a correct
    /// program that this rule must not see. [`crate::conform`]'s §3 is the
    /// line, and it is `Def::is_builtin` there as it is in
    /// [`crate::methods`]' §8.
    ///
    /// **Not a lookup failure at all**, which is why it is not
    /// [`NO_SUCH_METHOD`]: the method is found, and will be found by
    /// [`crate::methods`]' index, on the type and through the interface alike.
    /// What is wrong is that an interface's method set would then differ
    /// between its implementations, so `any Summarize` would mean a different
    /// thing per implementor and Decision 13's dispatch would have nothing to
    /// dispatch on.
    ///
    /// **The fix is the inherent block**, `Doc has:` — the language's own place
    /// for a method that answers no interface, which `AGENTS.md`'s table and
    /// `examples/06_traits.science` both spell.
    pub const NOT_AN_INTERFACE_METHOD: Code = Code(540);

    /// A method whose signature is not the one its interface declared.
    ///
    /// **Not [`MISMATCHED_TYPES`]**, and it is [`UNSATISFIED_BOUND`]'s argument
    /// at a declaration. `SC0525`'s shape is one expected type taken from one
    /// annotation at one value; here there is no value, and what disagrees may
    /// be the receiver, the arity or a type parameter count — none of which is
    /// a type at all. The message names the whole signature because the whole
    /// signature is what the author rewrites.
    ///
    /// **One code for the receiver, the parameters and the return**, although
    /// each is a different word in the message. They are one line in the source
    /// and one edit to fix, and a code per position would be three codes whose
    /// only difference is which word of one sentence changed — the opposite of
    /// the case [`UNREACHABLE_ARM`] makes for a code of its own, where the
    /// *subject* differs and not the wording.
    ///
    /// **The comparison is against the substituted declaration.** `Row
    /// implements Index of Int:` writing `def index(self, at: Int) -> borrowed
    /// F64` disagrees with `Index`'s declaration at every position and is
    /// correct; [`crate::conform`]'s §2 lists the three substitutions that have
    /// to happen first, and all three are functions the body checker already
    /// uses.
    pub const MISMATCHED_IMPLEMENTATION: Code = Code(541);

    /// Every code this crate emits from its own bands, for the test that keeps
    /// them inside those bands and distinct.
    ///
    /// **`SC0140` is not here**, and that is the point of the list: it is
    /// `syntax-revision-2.md`'s code, implemented by
    /// [`crate::unchecked`](crate::unchecked::UNCHECKED_ERROR) and borrowed
    /// rather than claimed.
    pub const ALL: &[Code] = &[
        NON_EXHAUSTIVE_MATCH,
        NOT_A_CONST_EXPRESSION,
        NOT_PROVABLY_EQUAL,
        DOUBLE_NULLABLE,
        CONST_WHERE_TYPE_EXPECTED,
        CYCLIC_ALIAS,
        MISMATCHED_TYPES,
        TYPE_ANNOTATIONS_NEEDED,
        WRONG_ARGUMENT_COUNT,
        NO_SUCH_FIELD,
        BINDING_COUNT_MISMATCH,
        PRESENCE_TEST_ON_NON_NULLABLE,
        AMBIGUOUS_METHOD,
        NO_SUCH_METHOD,
        NO_MATCHING_IMPLEMENTATION,
        UNSATISFIED_BOUND,
        NO_OPERATOR_IMPLEMENTATION,
        UNINFERABLE_RECEIVER,
        UNREACHABLE_ARM,
        RANGE_INDEX_NEEDS_SLICE,
        NOT_DISPLAYABLE,
        ARRAY_ELEMENT_MISMATCH,
        EMPTY_ARRAY_NO_TYPE,
        UNIMPLEMENTED_INTERFACE_METHOD,
        NOT_AN_INTERFACE_METHOD,
        MISMATCHED_IMPLEMENTATION,
    ];

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn every_code_is_in_a_band_this_crate_owns() {
            for code in ALL {
                assert!(
                    (250..=299).contains(&code.0) || (520..=579).contains(&code.0),
                    "{code} is outside SC0250-SC0299 and SC0520-SC0579"
                );
            }
        }

        #[test]
        fn no_code_is_used_twice() {
            let mut seen = ALL.to_vec();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), ALL.len(), "two diagnostics share a code");
        }

        #[test]
        fn the_two_codes_section_13_named_and_this_crate_cannot_report_are_free() {
            // `SC0521` and `SC0522`. Taking either for something else would
            // make the note's own table wrong about the compiler.
            for reserved in [521u16, 522] {
                assert!(
                    !ALL.iter().any(|code| code.0 == reserved),
                    "SC0{reserved} is reserved by §13 for a check this crate does not make"
                );
            }
        }

        #[test]
        fn the_unchecked_error_code_is_borrowed_and_not_claimed() {
            assert!(
                !ALL.iter().any(|code| code.0 == 140),
                "SC0140 belongs to `syntax-revision-2.md`; this crate implements it"
            );
            assert_eq!(crate::unchecked::UNCHECKED_ERROR.0, 140);
        }
    }
}
