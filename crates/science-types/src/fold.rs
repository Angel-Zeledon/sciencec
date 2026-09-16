//! Rebuilding one interned type into another — the walk substitution, alias
//! expansion and `Self` resolution all share.
//!
//! # 1. Why a trait and not three walks
//!
//! **Decision. There is one traversal over [`TyKind`], written once, and a
//! folder supplies only the arms it changes.**
//!
//! The three callers differ in exactly one node each — [`crate::subst`]
//! rewrites [`TyKind::Param`], [`crate::alias`] rewrites a [`TyKind::Named`] at
//! an alias, and `Self` resolution rewrites [`TyKind::SelfType`] — and agree on
//! all nine others. Three copies of the nine would be three places to forget
//! [`TyKind::Closure`]'s return type, and forgetting it is not a crash: it is a
//! type that kept a parameter nobody substituted, compared by `==` against one
//! that did not, and reported as a mismatch between two types that render
//! identically. That is the worst diagnostic this crate could produce, and the
//! only defence against it is that the arm exists once.
//!
//! # 2. The table is an argument, not a field
//!
//! **Decision. [`TypeFolder::fold_ty`] takes `&mut Types` as a parameter and
//! the folder holds no reference to it.**
//!
//! `ty`'s §2 is why: interning takes `&mut self`, so a folder that *held* the
//! table would hold it exclusively for the folder's whole lifetime, and the
//! caller — a checker that owns the table and lends it out — could not read a
//! type between two folds. Passing it per call is `mutable self` seen from the
//! other side, which is the shape Decision 24 wants ported.
//!
//! **What it costs.** Every recursive call carries a word it could have found
//! in `self`, and a folder that needs both the table and its own state writes
//! `self.memo` and `types` as two names rather than one. That is visible, and
//! it is meant to be.
//!
//! # 3. Unchanged means unchanged, by identity
//!
//! [`fold_children`] hands back **the same [`Ty`]** when no operand moved,
//! rather than re-interning an equal one. Interning would return the same index
//! anyway — that is §1 of `ty` — so this is not about correctness. It is about
//! not hashing a cloned `Vec<GenericArg>` at every node of every signature at
//! every call site, when the overwhelmingly common answer is that a type with
//! no parameter in it substitutes to itself.
//!
//! **What it costs** is a comparison per node, paid even when the fold does
//! change something. One `u32` comparison against one hash of a cloned operand
//! list is not a trade that needs measuring.
//!
//! # 4. The one node that is not structural
//!
//! [`TyKind::Nullable`] goes back through [`Types::nullable`], which is
//! idempotent, so substituting `T := U?` into `T?` produces `U?` and not the
//! `U??` the language does not have. `ty`'s §4 predicted exactly this case and
//! said what it means: **this collapse is not `SC0520`.** That diagnostic is
//! for a `?` somebody wrote twice, and here there is one `?` in the source and
//! one in the argument. A folder that reported it would blame a reader for a
//! substitution the compiler performed.
//!
//! # 5. Failure is arithmetic, and only arithmetic
//!
//! A fold fails in one way: the const half of a substitution multiplies or adds
//! its way out of `i128` ([`crate::subst`]'s §3). So the error type is
//! [`ConstEvalError`] itself rather than a wrapper around it — a one-variant
//! enum wrapping a one-variant enum is the same refusal written twice, and
//! [`crate::diagnostics::overflowed`] already takes this type and produces the
//! `SC0260` that reports it.
//!
//! The type half cannot fail. There is no occurs check to run: a [`Ty`] is a
//! finite tree over an append-only table, and a fold cannot build a cycle in
//! it, because a cycle would be a type containing its own index and
//! [`Types::intern`] hands out an index only after its operands have theirs.

use crate::normal::ConstEvalError;
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// A rewrite from types to types.
///
/// Implementors override [`TypeFolder::fold_ty`] for the nodes they change and
/// call [`fold_children`] for everything else, which is the recursion. A folder
/// that overrides nothing is the identity.
pub trait TypeFolder {
    /// The whole of the rewrite, for one type.
    ///
    /// An implementation that wants the default behaviour for a node calls
    /// [`fold_children`], which calls back into this method for the operands.
    /// Nothing calls `fold_children` on the folder's behalf: a folder that
    /// intercepts a node decides whether its operands are still there to fold,
    /// and only the folder knows.
    fn fold_ty(&mut self, types: &mut Types, ty: Ty) -> Result<Ty, ConstEvalError>;

    /// One entry of an `of (..)` list.
    ///
    /// The default folds a type argument and leaves a const argument alone,
    /// which is right for every folder that does not touch the const half.
    /// [`crate::subst::Substitution`] is the one that does, and it overrides
    /// this.
    fn fold_arg(
        &mut self,
        types: &mut Types,
        arg: &GenericArg,
    ) -> Result<GenericArg, ConstEvalError> {
        match arg {
            GenericArg::Type(ty) => Ok(GenericArg::Type(self.fold_ty(types, *ty)?)),
            GenericArg::Const(form) => Ok(GenericArg::Const(form.clone())),
            GenericArg::Error => Ok(GenericArg::Error),
        }
    }
}

/// The structural recursion: fold every operand and rebuild.
///
/// Returns `ty` itself when nothing moved (§3). A leaf — the error type, unit,
/// a parameter, a `Self` — has no operands, so this is the identity on it, and
/// a folder that means to rewrite a leaf intercepts it in
/// [`TypeFolder::fold_ty`] before calling here.
pub fn fold_children<F: TypeFolder + ?Sized>(
    folder: &mut F,
    types: &mut Types,
    ty: Ty,
) -> Result<Ty, ConstEvalError> {
    // The kind is cloned before anything is interned, because interning takes
    // the table exclusively and a borrow of a kind cannot survive it. `ty`'s §2
    // states that cost; this is one of the sites it meant.
    match types.kind(ty).clone() {
        TyKind::Error
        | TyKind::Unit
        | TyKind::Param { .. }
        | TyKind::SelfType { .. }
        | TyKind::SelfAssoc { .. } => Ok(ty),
        TyKind::Borrowed { mutable, inner } => {
            let folded = folder.fold_ty(types, inner)?;
            if folded == inner {
                return Ok(ty);
            }
            Ok(types.borrowed(mutable, folded))
        }
        TyKind::Nullable(inner) => {
            let folded = folder.fold_ty(types, inner)?;
            if folded == inner {
                return Ok(ty);
            }
            // §4: idempotent, and deliberately not `SC0520`.
            Ok(types.nullable(folded))
        }
        TyKind::Tuple(elements) => {
            let mut folded = Vec::with_capacity(elements.len());
            let mut moved = false;
            for element in &elements {
                let next = folder.fold_ty(types, *element)?;
                moved |= next != *element;
                folded.push(next);
            }
            if !moved {
                return Ok(ty);
            }
            Ok(types.tuple(folded))
        }
        TyKind::Closure { params, ret } => {
            let mut folded = Vec::with_capacity(params.len());
            let mut moved = false;
            for param in &params {
                let next = folder.fold_ty(types, *param)?;
                moved |= next != *param;
                folded.push(next);
            }
            let folded_ret = folder.fold_ty(types, ret)?;
            if !moved && folded_ret == ret {
                return Ok(ty);
            }
            Ok(types.closure(folded, folded_ret))
        }
        TyKind::Named { def, args } => {
            let (folded, moved) = fold_args(folder, types, &args)?;
            if !moved {
                return Ok(ty);
            }
            Ok(types.named(def, folded))
        }
        TyKind::Object { interface, args } => {
            let (folded, moved) = fold_args(folder, types, &args)?;
            if !moved {
                return Ok(ty);
            }
            Ok(types.object(interface, folded))
        }
    }
}

/// Folds an argument list, reporting whether any entry changed.
fn fold_args<F: TypeFolder + ?Sized>(
    folder: &mut F,
    types: &mut Types,
    args: &[GenericArg],
) -> Result<(Vec<GenericArg>, bool), ConstEvalError> {
    let mut folded = Vec::with_capacity(args.len());
    let mut moved = false;
    for arg in args {
        let next = folder.fold_arg(types, arg)?;
        moved |= next != *arg;
        folded.push(next);
    }
    Ok((folded, moved))
}
