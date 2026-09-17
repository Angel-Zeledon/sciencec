//! Substitution: the type half and the const half, designed together.
//!
//! `ty`'s §8 left this one seam deliberately undivided — *"substitution,
//! including the const half, which rewrites atoms inside a [`NormalForm`] and
//! has an overflow failure mode of its own"* — because the two halves meet
//! inside a single node. `Matrix of (T, N + 1)` instantiated at `T := F32` and
//! `N := 2*K` has a type argument and a const argument in one `of (..)` list,
//! and a substitution that could do one and not the other would be a
//! substitution every caller has to call twice and remember to.
//!
//! # 1. One map for four things
//!
//! **Decision. A [`Substitution`] carries type parameters, const parameters,
//! `Self` and `Self.Assoc` together, and [`Substitution::apply`] is one pass.**
//!
//! The reason is the caller that will have all four at once: a method call on
//! `Doc` reaching a generic method of an implementation block. Its signature
//! mentions `Self` (the block's), the implementation's type parameters, the
//! method's own, and a const parameter of either — and every one of them has to
//! be gone from the type the call site compares against, in the same pass,
//! because a type that is half substituted is a type that compares unequal to
//! itself under `==` and there is no representation that says it is not
//! finished.
//!
//! **What it costs.** Four maps exist where a caller usually populates one, and
//! an empty map is checked at every node it could apply to. [`Substitution`]
//! answers [`Substitution::is_empty`] so a caller can skip the walk entirely,
//! which is the case at every call to a non-generic function in a
//! non-generic block — that is to say, most of them.
//!
//! # 2. `Self` is matched on its owner
//!
//! [`TyKind::SelfType`](crate::TyKind::SelfType) carries the block that wrote
//! it, and this substitution replaces it only when that block is the one being
//! substituted for. A `Self`
//! from another implementation is another implementation's `Self` and keeps its
//! meaning. Nothing in F0 puts two in one type, and the check is one
//! comparison; the alternative — replacing every `Self` regardless — is a
//! silent wrong answer the day something does.
//!
//! **`Self.Item` inside an interface is not resolvable here, and that is not an
//! omission.** The resolver is explicit that `Self.Item` written in an
//! implementation resolves to *the implementation's* associated type, and one
//! written in an interface resolves to the interface's declaration, which has
//! no type at all until an implementation supplies it. Answering the second
//! means pairing an interface with an implementation, which is Decision 11's
//! lookup — [`crate::methods`] — and which this module still does not do: the
//! pairing is [`crate::items::Declarations::body_substitution`]'s and a
//! substitution is handed one already built. So an unbound
//! [`TyKind::SelfAssoc`](crate::TyKind::SelfAssoc) is left standing rather than
//! turned into [`Ty::ERROR`]: the type is not wrong, it is
//! not yet known, and the phase that knows is the phase that should say.
//!
//! # 3. The const half, and why it is a cancellation rather than a rebuild
//!
//! A const argument is a [`NormalForm`] — `k + Σ cᵢ·aᵢ` — and substituting
//! `N := f` means replacing every term whose atom is `N` with `cᵢ · f`, which is
//! `SCALE` followed by `MERGE`. Both are `normal`'s and both already exist.
//!
//! **Decision. A term that is *not* being substituted is never rebuilt.** The
//! form is cancelled down rather than reassembled: each substituted term is
//! removed by merging its own negation, which `MERGE` drops entirely because
//! the coefficient reaches zero, and then the replacements are merged in.
//!
//! The reason is [`crate::Term::provenance`], which is a *list* of spans and has
//! no public constructor — by design, since `normal`'s §3 makes the list the
//! content of §9.3's derivation block, one row per contributing operand. A
//! rebuild could carry at most one span per term and would therefore throw away
//! exactly the rows that block is made of. Cancellation touches only the terms
//! being replaced, and `MERGE` clones the rest with their provenance intact.
//!
//! **What the substituted terms' provenance becomes** is the replacement's,
//! scaled — which is right: after `Grid of (F32, 2*K)` the operand a reader has
//! to look at for that extent is `2*K` at the instantiation, not the `N` in the
//! declaration. When a replacement lands on an atom the form already mentions,
//! `MERGE` concatenates the two provenance lists, which is §9.3's rule
//! unchanged.
//!
//! **What it costs** is one extra `MERGE` pass per substituted term, and one
//! real oddity: cancelling a term whose coefficient is exactly `i128::MIN`
//! overflows on the negation and reports, although the value it denotes was
//! representable. Building the negation by an unchecked `-c` instead would be a
//! panic in a debug build, and `normal`'s §5 has already ruled on which of a
//! wrong answer and a refusal this crate prefers.
//!
//! # 4. Overflow is returned, not reported
//!
//! **Decision. [`Substitution::apply`] returns [`ConstEvalError`] and pushes no
//! diagnostic.**
//!
//! `SCALE` and `MERGE` are `i128` arithmetic and a substitution is where
//! arbitrary values meet arbitrary coefficients: `Window of (Int, N * 3)` at
//! `N := i128::MAX / 2` leaves the range that `normal`'s §5 refuses to wrap.
//!
//! It is returned rather than reported because **this crate cannot yet write
//! the message**. `const-expression-arithmetic.md` §9.4 makes the
//! *instantiation chain* the whole of what such a diagnostic says — which
//! generic was instantiated with what, at which call site — and §10.2 puts that
//! chain in F1 with `SC0262`. A caller with a span in hand can report it today
//! through [`crate::diagnostics::overflowed`] as `SC0260`, which is the same
//! sentence about the same `i128`; a caller without one has a bug, not a
//! diagnostic. Pushing from here would render whatever this crate happened to
//! keep, which is `lib`'s §3 stated as a rule and this is it applied.

use std::collections::HashMap;

use science_resolve::hir::{self, DefId, GenericParamKind, BUILTIN_SPAN};

use crate::fold::{fold_children, TypeFolder};
use crate::normal::{Atom, ConstEvalError, NormalForm};
use crate::ty::{GenericArg, Ty, Types};

/// What a substitution replaces.
///
/// Built with the `with_*` methods or from a declaration's parameters and a
/// use's arguments by [`Substitution::of_generics`], then applied with
/// [`Substitution::apply`].
#[derive(Debug, Clone, Default)]
pub struct Substitution {
    /// [`TyKind::Param`](crate::TyKind::Param) at this definition becomes this
    /// type.
    types: HashMap<DefId, Ty>,
    /// A const parameter atom becomes this normal form, scaled by whatever
    /// coefficient it carried. §3.
    consts: HashMap<DefId, NormalForm>,
    /// `Self` inside this block becomes this type. §2.
    self_ty: Option<(DefId, Ty)>,
    /// `Self.Item` naming this associated type becomes this type.
    assoc: HashMap<DefId, Ty>,
}

impl Substitution {
    /// A substitution that replaces nothing. Applying it is the identity.
    pub fn new() -> Substitution {
        Substitution::default()
    }

    /// Replaces the type parameter `param`.
    pub fn with_type(mut self, param: DefId, ty: Ty) -> Substitution {
        self.types.insert(param, ty);
        self
    }

    /// Replaces the const parameter `param` with a normal form.
    pub fn with_const(mut self, param: DefId, form: NormalForm) -> Substitution {
        self.consts.insert(param, form);
        self
    }

    /// Replaces `Self` inside the block `owner`. §2.
    pub fn with_self(mut self, owner: DefId, ty: Ty) -> Substitution {
        self.self_ty = Some((owner, ty));
        self
    }

    /// Replaces `Self.Item` where `Item` is the associated type `assoc`.
    pub fn with_assoc(mut self, assoc: DefId, ty: Ty) -> Substitution {
        self.assoc.insert(assoc, ty);
        self
    }

    /// The substitution a declaration's parameters and a use's arguments make.
    ///
    /// Zips, and **does not check arity**: `lowering`'s §1 defers that to the
    /// phase holding both the declaration and the use, and this function is
    /// below it. A parameter with no argument is left free — the type keeps a
    /// [`TyKind::Param`](crate::TyKind::Param) in it, which is what was
    /// written — and an argument with no parameter is dropped. Neither is a
    /// silent wrong answer: both are the too-few-or-too-many the arity check
    /// exists to report, still visible in the type.
    ///
    /// A [`GenericArg::Error`] in a *type* position becomes [`Ty::ERROR`], so
    /// that `ty`'s §5 keeps one bad argument to one diagnostic. In a const
    /// position it has no analogue — there is no erroneous normal form — so the
    /// parameter is left free, which is the same as too few arguments and is
    /// reported as that.
    pub fn of_generics(params: &[hir::GenericParam], args: &[GenericArg]) -> Substitution {
        let mut subst = Substitution::new();
        for (param, arg) in params.iter().zip(args) {
            match (&param.kind, arg) {
                (GenericParamKind::Type { .. }, GenericArg::Type(ty)) => {
                    subst.types.insert(param.def, *ty);
                }
                (GenericParamKind::Type { .. }, GenericArg::Error) => {
                    subst.types.insert(param.def, Ty::ERROR);
                }
                (GenericParamKind::Const { .. }, GenericArg::Const(form)) => {
                    subst.consts.insert(param.def, form.clone());
                }
                // A const argument at a type parameter, or the reverse. The
                // kinds disagree, which is a mistake in the use and belongs to
                // the arity and kind check; binding it to something here would
                // hide it.
                _ => {}
            }
        }
        subst
    }

    /// Whether this substitution replaces anything.
    ///
    /// A caller that asks this before walking skips the walk at every use of a
    /// non-generic type, which is most uses.
    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
            && self.consts.is_empty()
            && self.self_ty.is_none()
            && self.assoc.is_empty()
    }

    /// The type with every parameter, `Self` and `Self.Assoc` this substitution
    /// knows replaced.
    ///
    /// Fails only on §4's arithmetic range, and only through the const half.
    pub fn apply(&self, types: &mut Types, ty: Ty) -> Result<Ty, ConstEvalError> {
        if self.is_empty() {
            return Ok(ty);
        }
        Substituter { subst: self }.fold_ty(types, ty)
    }

    /// The normal form with every substituted atom replaced. §3.
    ///
    /// Public because the const half has callers of its own: a shape
    /// obligation, a monomorphisation key and an index check all hold a
    /// [`NormalForm`] with no type around it.
    pub fn apply_const(&self, form: &NormalForm) -> Result<NormalForm, ConstEvalError> {
        if self.consts.is_empty() {
            return Ok(form.clone());
        }

        // Which terms move, and to what. The atom order is not disturbed by
        // collecting here: `MERGE` restores it, and every write below goes
        // through `MERGE`.
        let touched: Vec<(&crate::normal::Term, &NormalForm)> = form
            .terms()
            .iter()
            .filter_map(|term| match term.atom() {
                // When §4's quotient atom arrives it holds a normalised
                // expression of its own, and substituting into *that* is a
                // second case with a second normalisation. It is an added arm
                // here, which is why `Atom` is an enum.
                Atom::Param { def, .. } => {
                    self.consts.get(&def).map(|replacement| (term, replacement))
                }
            })
            .collect();
        if touched.is_empty() {
            return Ok(form.clone());
        }

        let mut result = form.clone();
        for (term, _) in &touched {
            // The span is never read: this form exists to cancel a term, and
            // `MERGE` drops a cancelled term's provenance with it. `BUILTIN_SPAN`
            // is the one that points at no text, which is what this is.
            let span = term.provenance().first().copied().unwrap_or(BUILTIN_SPAN);
            let cancel = NormalForm::atom(term.atom(), span).scale(term.coefficient())?.negate()?;
            result = result.merge(&cancel)?;
        }
        for (term, replacement) in &touched {
            result = result.merge(&replacement.scale(term.coefficient())?)?;
        }
        Ok(result)
    }
}

/// The folder [`Substitution::apply`] runs. Holds the maps and nothing else;
/// the table arrives per call, which is `fold`'s §2.
struct Substituter<'a> {
    subst: &'a Substitution,
}

impl TypeFolder for Substituter<'_> {
    fn fold_ty(&mut self, types: &mut Types, ty: Ty) -> Result<Ty, ConstEvalError> {
        match types.kind(ty) {
            crate::TyKind::Param { def } => {
                if let Some(&replacement) = self.subst.types.get(def) {
                    return Ok(replacement);
                }
                Ok(ty)
            }
            crate::TyKind::SelfType { owner } => {
                if let Some((block, replacement)) = self.subst.self_ty {
                    if block == *owner {
                        return Ok(replacement);
                    }
                }
                Ok(ty)
            }
            crate::TyKind::SelfAssoc { assoc } => {
                if let Some(&replacement) = self.subst.assoc.get(assoc) {
                    return Ok(replacement);
                }
                // §2: not known, rather than wrong.
                Ok(ty)
            }
            _ => fold_children(self, types, ty),
        }
    }

    fn fold_arg(
        &mut self,
        types: &mut Types,
        arg: &GenericArg,
    ) -> Result<GenericArg, ConstEvalError> {
        match arg {
            GenericArg::Type(ty) => Ok(GenericArg::Type(self.fold_ty(types, *ty)?)),
            GenericArg::Const(form) => Ok(GenericArg::Const(self.subst.apply_const(form)?)),
            GenericArg::Error => Ok(GenericArg::Error),
        }
    }
}
