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
//! It answers **true** — needs a drop, owns something — everywhere it cannot
//! tell: a choice type, an interface object, a type parameter, a foreign
//! union, `Self`. The false answers this produces (`ffi.Span` chief among
//! them, [`crate::check`]'s field rule inherits the same conservatism its
//! refuser already lived with) disappear the day the missing lookups land.
//!
//! # 3. The `Drop` lookup, which has landed
//!
//! This module used to open by saying it *"cannot ask whether a type
//! implements `Drop`, because that lookup does not exist yet"*. It does
//! exist: [`crate::methods::Methods::declares`] is the index §4 builds out of
//! every `T implements I:` block in the crate, and [`crate::items::Prelude`]
//! can hold `Drop`'s id the same way it already holds `Iterate`'s and
//! `Display`'s. So the question is asked now, first, before any structural
//! answer.
//!
//! **It has to be first, and the cost of its having been absent was a
//! destructor that never ran.** A record of two `Int`s is structurally inert,
//! so this predicate said `false`, so `science-mir` emitted no `Drop`
//! terminator for it at all — and a `Doc implements Drop:` on that record was
//! a block the compiler read, checked, and then dropped on the floor with no
//! diagnostic. Nothing downstream could repair that: codegen's drop glue is
//! only ever reached *through* a `Drop` terminator, and there was none to
//! reach it through. Decision 12 of `codegen-and-linking.md` describes glue
//! that *"drops fields in reverse declaration order and then calls the type's
//! own `Drop` implementation if it has one"*; the second half of that
//! sentence had no way of being true for a type whose fields own nothing, and
//! this is the half of the repair that lives above codegen.
//!
//! **What it costs is a `false` becoming a `true` for [`crate::check`]'s
//! field rule too**, which is the other caller: a field read out of a
//! `borrowed T` where `T` has a `Drop` implementation is now typed as a
//! borrow rather than as an owned copy. That is the conservative direction
//! and it is the *right* direction — a type with a destructor is exactly a
//! type two owners must not both free — so the two callers still want the one
//! answer, which is §1's whole argument for there being one function.

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

/// Whether the crate writes `T implements Drop:` for this type — §3.
///
/// **A borrow answers `false`, and that guard is the whole reason this is a
/// function rather than two lines at each call site.**
/// [`crate::methods::Methods::declares`] reads a type's *head*, and a head is
/// transparent through a borrow: asked about `mutable self` inside
/// `H.drop`, the index would answer *"yes, `H` implements `Drop`"* and the
/// receiver would acquire a destructor of its own — a second release of the
/// value the caller is in the middle of releasing. The `Borrowed` arm of
/// [`needs_drop`] already says `false` for the structural reason (*"a borrow
/// releases nothing"*), and this says it again for the nominal one, because
/// §3's lookup has to run before that arm is reached.
///
/// A compilation with no prelude has no `Drop` id and answers `false`, which
/// is the same admission every other [`crate::items::Prelude`] lookup makes
/// for a hand-built definition table.
pub fn implements_drop(decls: &Declarations, types: &Types, ty: Ty) -> bool {
    if matches!(types.kind(ty), TyKind::Borrowed { .. }) {
        return false;
    }
    decls.prelude().get("Drop").is_some_and(|interface| {
        decls.methods().declares(types, ty, interface)
    })
}

fn needs_drop_inner(
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    ty: Ty,
    visiting: &mut Vec<DefId>,
) -> bool {
    let ty = aliases.reveal(types, ty).unwrap_or(ty);
    // §3. A user's own `Drop` implementation, asked before any structural
    // answer because it overrides every one of them: a record of two `Int`s
    // with a `drop` of its own still has to run it, and asking the fields
    // would say `false` and lose the destructor entirely.
    if implements_drop(decls, types, ty) {
        return true;
    }
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
            // **A prelude iterator over a string owns nothing**: `Chars` and
            // `Lines` are `science-rt`'s `{ ptr, len, offset }` *into* a
            // string somebody else owns, which `science-codegen-llvm` already
            // knows (its drop-glue list puts both beside `Char`). Answering
            // `true` here was §2's conservatism and it was not free:
            // `science-regions` gives both types a borrow of their receiver
            // (its `regions` §6), a `Drop` is a use of everything a value
            // borrows, and the iterator a `for c in make().chars():` builds
            // was dropped *after* the temporary string it reads — so the
            // loop's own clean-up kept the string's loan alive past the
            // string, and the corpus's form was refused with `SC0333`.
            if prelude.is(types, ty, "Chars") || prelude.is(types, ty, "Lines") {
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
