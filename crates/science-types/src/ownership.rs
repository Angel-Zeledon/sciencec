//! Whether a type owns something — the one predicate two different callers
//! need the same answer to.
//!
//! # 1. Where this used to live, and why it moved
//!
//! [`needs_drop`] was `science-mir`'s, written for Decision 26's drop
//! elaboration and documented there as *"does dropping this type run
//! anything… structural over the prelude's primitives and the declared fields
//! of a record, and it answers **true** for everything else"*. Nothing about
//! that question is about MIR — every type it inspects is a
//! [`crate::ty::Ty`], every declaration it walks is a [`crate::items::Declarations`]
//! record, and `science-mir` depended on this crate for both. It lived there
//! anyway because it had one caller and the caller was there.
//!
//! It now has a second caller, and the second caller is [`crate::check`]:
//! §9's match-ergonomics rule (`scrutinee_substitution`'s doc, and
//! [`crate::check::BodyChecker::field`]) needs the identical question —
//! *would giving this binding its declared, owned type let two owners free
//! the same value* — to decide whether a field or payload read through a
//! shared or exclusive borrow should be typed as a borrow instead. `science-types`
//! cannot depend on `science-mir` (the dependency runs the other way), so the
//! predicate itself moved to the crate both callers already stand on, and
//! `science-mir`'s `moves` module now re-exports this rather than keeping a
//! second copy. Two implementations of "does this own something" is two
//! answers, and the second one drifting from the first is exactly
//! `science-types`'s own §1 argument against a second implementation of const
//! equality — the reasoning is not new here, only the predicate is.
//!
//! # 2. What it cannot know, unchanged from where it was written
//!
//! It cannot ask whether a type implements `Drop`, because that lookup does
//! not exist yet, and it answers **true** — needs a drop, owns something —
//! everywhere it cannot tell: a choice type, an interface object, a type
//! parameter, a foreign union, `Self`. The false answers this produces
//! (`ffi.Span` chief among them, [`crate::check`]'s field rule inherits the
//! same conservatism its refuser already lived with) disappear the day that
//! lookup lands, with no change here.

use science_resolve::hir::DefId;

use crate::alias::Aliases;
use crate::items::Declarations;
use crate::ty::{Ty, TyKind, Types};

/// Whether dropping a value of this type runs anything.
///
/// A borrow answers `false` unconditionally — *"a borrow releases nothing;
/// releasing the referent is the referent's own storage-dead point"* — which
/// is exactly why [`crate::check::BodyChecker::field`] asks this question
/// about the **field's** type and not the borrow it is read through: asking
/// it of the borrow itself would always answer `false` and the rule would
/// never fire.
pub fn needs_drop(decls: &Declarations, types: &mut Types, aliases: &mut Aliases, ty: Ty) -> bool {
    let mut visiting = Vec::new();
    needs_drop_inner(decls, types, aliases, ty, &mut visiting)
}

fn needs_drop_inner(
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    ty: Ty,
    visiting: &mut Vec<DefId>,
) -> bool {
    let ty = aliases.reveal(types, ty).unwrap_or(ty);
    let kind = types.kind(ty).clone();
    match kind {
        // A hole drops nothing. `ty`'s §5: an erroneous type must not
        // manufacture work any more than it manufactures a diagnostic.
        TyKind::Error | TyKind::Unit => false,
        // A borrow releases nothing; releasing the referent is the referent's
        // own storage-dead point.
        TyKind::Borrowed { .. } => false,
        TyKind::Tuple(elements) => elements
            .iter()
            .any(|element| needs_drop_inner(decls, types, aliases, *element, visiting)),
        TyKind::Nullable(inner) => needs_drop_inner(decls, types, aliases, inner, visiting),
        TyKind::Named { def, .. } => {
            let prelude = decls.prelude();
            if prelude.is_numeric(types, ty)
                || prelude.is_bool(types, ty)
                || prelude.is(types, ty, "Char")
                || prelude.is_never(types, ty)
            {
                return false;
            }
            // A recursive record cannot have a finite layout, so this guard
            // never fires on a program that will compile. It is here because a
            // program that will *not* compile still reaches this pass, and a
            // stack overflow is a worse answer than a conservative `true`.
            if visiting.contains(&def) {
                return true;
            }
            let Some(record) = decls.record(def) else {
                // A choice type, `String`, `Array`, a foreign union, an
                // interface's associated type: §2's *"true where it cannot
                // tell"*.
                return true;
            };
            let fields: Vec<Ty> = record.fields.iter().map(|(_, ty)| *ty).collect();
            visiting.push(def);
            let answer = fields
                .into_iter()
                .any(|field| needs_drop_inner(decls, types, aliases, field, visiting));
            visiting.pop();
            answer
        }
        // A type parameter could be anything; monomorphisation is the phase
        // that knows, and it runs after both of this predicate's callers.
        TyKind::Param { .. }
        | TyKind::Object { .. }
        | TyKind::SelfType { .. }
        | TyKind::SelfAssoc { .. }
        | TyKind::Closure { .. } => true,
    }
}
