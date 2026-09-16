//! Assignability: `compatible`, plus exactly two implicit coercions.
//!
//! [`Types::compatible`] is structural equality with an error type agreeing
//! with everything, and its own documentation says what it is not: *"it does
//! not widen `T` into `T?` (Decision 6), it does not box a concrete error type
//! into `any Error` (Decision 14), it does not look through an alias, and it
//! does not know what an interface is."* This module is the first three. The
//! fourth stays out, and §4 says why.
//!
//! **Both arguments must already be revealed** —
//! [`crate::alias::Aliases::reveal`] is the function that does it. Revealing
//! needs `&mut Types` because it interns what it builds; this relation is a
//! `&Types` question and stays one,
//! so that a caller can ask it with the table borrowed shared and so that the
//! relation itself has nothing in it that can allocate. The cost is one call
//! the caller must not forget, and the test that catches forgetting it is that
//! an alias and its expansion are one type.
//!
//! # 1. The relation, written out
//!
//! For a value of type `S` reaching a slot of type `T` at a site `site`:
//!
//! ```text
//!   compatible(S, T)                                    =>  Identity
//!   T = U?  and  compatible(S, U)                       =>  Widen
//!   T = any Error  and  boxable(S)  and  boxes(site)    =>  Box
//!   T = (any Error)?  and  boxable(S)  and  boxes(site) =>  BoxThenWiden
//!   otherwise                                           =>  not assignable
//! ```
//!
//! read in that order, where `boxes(site)` is true at a `return` and at an
//! argument and nowhere else, and `boxable(S)` is §3. The first rule subsumes
//! every case where nothing has to happen, including `S = T?` into `T?` and
//! anything involving [`Ty::ERROR`] — which is what keeps one bad annotation to
//! one diagnostic here as everywhere else.
//!
//! # 2. The coercions are top-level, and that is a decision
//!
//! **Decision. Neither coercion recurses into a type.** `Array of T` is not
//! assignable to `Array of T?`, `(A) -> B` is not assignable to `(A) -> B?`,
//! and a tuple of a concrete error is not assignable to a tuple of `any Error`.
//!
//! The reason is that both coercions *change the representation of the value*:
//! Decision 6's `T?` is a niche or a discriminant byte beside the `T`, and
//! Decision 13's `any Error` is two words where the concrete type was however
//! many it was. Applying either under a type constructor means rewriting every
//! element of a container that already exists, at a cost proportional to its
//! length, at an assignment that looks free. Rust refuses the same thing for
//! the same reason, and the refusal is what keeps a coercion a fact about one
//! value rather than a loop.
//!
//! **What it costs is the motivating example of Decision 14 itself.** That
//! decision was written against `return (doc, err)` with a concrete `err`
//! against an `Error?` signature — and the *tuple* `(Doc, MyError)` is not
//! assignable to `(Doc, Error?)` by the rules above. It works because the thing
//! in return position is a tuple **expression**, whose elements the checker
//! visits one at a time, each at [`Site::Return`]. That is the next phase's
//! obligation and §5 states it, because a checker that compares the tuple's
//! synthesised type against the signature and stops has implemented Decision 14
//! in a way that rejects the line that motivated it.
//!
//! # 3. What this cannot check, and refuses to pretend about
//!
//! **Decision 14 boxes *"a concrete error type"*, and this relation cannot tell
//! whether a type is one.** That question is *does `S` implement `Error`*, and
//! answering it needs the implementation lookup of Decision 11, which is not
//! built. So the rule here checks the **target** exactly — the prelude's
//! `Error` interface, as an object, with no arguments — and admits any source
//! that is not structurally incapable of being one — the `boxable` predicate at
//! the foot of this file.
//!
//! **[`Coercion::Box`] is therefore a proposal with an obligation attached**:
//! the caller owes *"`S` implements `Error`"* and must discharge it before the
//! coercion is real. Until a caller does, this crate will say that `Int` may be
//! boxed into `any Error`. That is stated here rather than hidden, and it is
//! the one place in this module where the answer is not the whole answer. The
//! alternative — inventing a list of types that may be errors — would be a
//! guess with no caller, and it would be consulted instead of the real check
//! once the real check existed.
//!
//! # 4. What is deliberately not a coercion
//!
//! - **`T` into `any I` for an interface that is not `Error`.** §6.2 says
//!   Decision 14 is *"the one implicit coercion in the language besides `T` into
//!   `T?`"*, and a general concrete-into-object coercion is a second one. An
//!   `any Summarize` is constructed where it is written.
//! - **`T?` into `any Error`, and `E?` into `(any Error)?`.** Boxing a value
//!   that may be absent is a conversion that runs or does not depending on the
//!   value, and `syntax-revision-2.md` §3.4's rule — no implicit change of
//!   *value* — does not obviously cover it. The note did not decide it, so it
//!   is refused, which is the direction that can be reversed later: admitting it
//!   now and finding it wrong means unpicking programs that rely on it.
//! - **`T?` into `T`.** Decision 6: *"`T?` never coerces to `T`"*. There is no
//!   rule above that produces it and there is a test that says so, because this
//!   is the direction that makes the null check optional and the whole of §4 of
//!   the note pointless.
//! - **Anything about regions, mutability or variance.** `mutable borrowed T`
//!   into `borrowed T` is a coercion in Rust and is `region-inference.md`'s
//!   question here, not this crate's: nothing in this table knows what a region
//!   is.
//!
//! # 5. What the next phase owes this one
//!
//! Every use of [`assignable`] must pass the site the value is at, and the two
//! that box are named: a `return`'s operand and a call's argument. A checker
//! that passes [`Site::Elsewhere`] everywhere compiles and silently removes
//! Decision 14 from the language; a checker that passes [`Site::Return`]
//! everywhere silently makes boxing universal. The enum has three cases so that
//! the choice is made at each site rather than defaulted.

use science_resolve::hir::{DefId, DefKind, DefTable};

use crate::ty::{Ty, TyKind, Types};

/// Where a value is being put.
///
/// Decision 14's coercion holds *"at a `return` and at an argument position,
/// and nowhere else"*. The third case is every other position — a `let`, an
/// assignment, a field initialiser, an element of a literal — which the
/// relation treats identically, so they are one variant rather than six that
/// would have to be kept in step with a syntax that is still moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// The operand of a `return`, or the trailing expression of a body.
    Return,
    /// An argument at a call.
    Argument,
    /// Everywhere else.
    Elsewhere,
}

impl Site {
    /// Whether Decision 14's boxing applies here.
    pub fn boxes(self) -> bool {
        matches!(self, Site::Return | Site::Argument)
    }
}

/// What has to happen for the value to fit the slot.
///
/// This is what THIR carries: Decision 3 makes it *"HIR with a type on every
/// node, method calls resolved to a specific implementation, and implicit
/// conversions made explicit"*, and these are the implicit conversions. The
/// two-step variant is a variant rather than a pair of booleans because the
/// order is not symmetric — the value is boxed and then made present, never the
/// reverse — and a lowering that reads a struct of flags has to know that from
/// somewhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coercion {
    /// The types already agree. Nothing is emitted.
    Identity,
    /// Decision 6: `T` into `T?`.
    Widen,
    /// Decision 14: a concrete error type into `any Error`, subject to §3's
    /// obligation.
    Box,
    /// Decision 14 then Decision 6: a concrete error type into `(any Error)?`,
    /// which is the type in the signature of every fallible function in the
    /// language.
    BoxThenWiden,
}

/// The one definition this relation has to know by name.
///
/// `any Error` is a type mentioning the prelude's `Error` interface, and the
/// prelude does not export its ids: `resolve_module` hands back a
/// [`hir::Crate`](science_resolve::hir::Crate) whose `DefTable` contains the
/// prelude's definitions but names none of them. So [`Coercions::of`] finds it
/// by the only property that identifies it — a builtin definition, of interface
/// kind, named `Error` — once, at the top of a compilation.
///
/// **What it costs is a string comparison over the definition table**, which is
/// the lookup the HIR exists to abolish, performed once. The honest fix is for
/// the resolver to publish the handful of prelude ids that later phases need by
/// name; that is `science-resolve`'s decision and this crate should not make it
/// by reaching into `builtins`.
///
/// `error` is an [`Option`] because a table built by hand — every test in this
/// crate that does not go through the resolver — has no prelude at all. With no
/// `Error` in it, Decision 14's rule cannot fire and the relation is
/// `compatible` plus Decision 6, which is exactly right for a table with no
/// interfaces in it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Coercions {
    error: Option<DefId>,
}

impl Coercions {
    /// No boxing: a table with no prelude, or a caller that wants Decision 6
    /// alone.
    pub fn new() -> Coercions {
        Coercions::default()
    }

    /// Finds the prelude's `Error` interface.
    pub fn of(defs: &DefTable) -> Coercions {
        let error = defs
            .iter()
            .find(|def| {
                def.kind == DefKind::Interface && def.name == "Error" && def.is_builtin()
            })
            .map(|def| def.id);
        Coercions { error }
    }

    /// The interface `any Error` names, when there is one.
    pub fn error_interface(self) -> Option<DefId> {
        self.error
    }

    /// Whether this type is exactly `any Error`.
    ///
    /// Exactly: an object at that interface with no arguments. `Error` takes
    /// none, so an `any Error of T` is a mistake the arity check reports and
    /// not a thing to box into.
    pub fn is_any_error(self, types: &Types, ty: Ty) -> bool {
        let Some(error) = self.error else {
            return false;
        };
        matches!(
            types.kind(ty),
            TyKind::Object { interface, args } if *interface == error && args.is_empty()
        )
    }
}

/// Whether a value of type `source` may be put in a slot of type `target`, and
/// what has to happen to it if so.
///
/// `None` is *not assignable*. The diagnostic for that is not here: a mismatch
/// is a fact about an expression, it wants the spans of both sides and the name
/// of the slot, and the phase holding those is the phase that reports. What
/// this hands back is the verdict and, when the verdict is yes, the conversion
/// THIR has to make explicit.
///
/// Both types must be revealed; see this module's opening paragraph.
pub fn assignable(
    types: &Types,
    coercions: Coercions,
    site: Site,
    source: Ty,
    target: Ty,
) -> Option<Coercion> {
    // Rule 1. Structural equality, and an already-reported type agreeing with
    // whatever it meets.
    if types.compatible(source, target) {
        return Some(Coercion::Identity);
    }

    if let TyKind::Nullable(inner) = *types.kind(target) {
        // Rule 2. Decision 6, and only in this direction.
        if types.compatible(source, inner) {
            return Some(Coercion::Widen);
        }
        // Rule 4. `-> (T, Error?)`, which is the signature Decision 14 was
        // written against.
        if site.boxes() && coercions.is_any_error(types, inner) && boxable(types, source) {
            return Some(Coercion::BoxThenWiden);
        }
        return None;
    }

    // Rule 3. Decision 14.
    if site.boxes() && coercions.is_any_error(types, target) && boxable(types, source) {
        return Some(Coercion::Box);
    }

    None
}

/// Whether `source` is a shape that could be a concrete error type.
///
/// This is the *structural* half of §3's question, and it is all of it that can
/// be answered without the implementation lookup. Three shapes are refused
/// outright:
///
/// - **A nullable.** §4: boxing a value that may be absent is a conditional
///   conversion the note did not decide.
/// - **A borrow.** `any Error` owns its value; a borrow does not, and boxing one
///   would be a lifetime question in a crate with no notion of one.
/// - **Another interface object.** Decision 14 boxes a *concrete* type. An
///   `any Summarize` into an `any Error` is an upcast between objects, which
///   needs a vtable the compiler would have to build and a subinterface
///   relation nobody has specified.
///
/// Everything else — a named type, a type parameter with a bound, `Self` — is
/// admitted here and owes the obligation.
fn boxable(types: &Types, source: Ty) -> bool {
    !matches!(
        types.kind(source),
        TyKind::Nullable(_) | TyKind::Borrowed { .. } | TyKind::Object { .. }
    )
}
