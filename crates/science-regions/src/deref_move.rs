//! A move whose place is reached through a borrow, refused outright.
//!
//! # 1. What this is not
//!
//! This is not `SC0301` — [`crate::moved`] answers *"is this value used after
//! it was moved"*, which presupposes the move itself was legal. Nothing in the
//! workspace has ever asked the prior question, *"was there anything to move
//! here at all, or only something to read"*, and this module is that question,
//! asked once, structurally, with no dataflow.
//!
//! # 2. The gap, named
//!
//! `science-types`'s `check.rs` peels a borrow to reach a field or a match
//! payload — `field`'s own comment, and `scrutinee_substitution`'s — and hands
//! back the *declared* type of what is underneath, not a borrowed one. That
//! comment says outright: *"match ergonomics are a decision no note has
//! taken"*. Whatever the eventual answer, `science-mir`'s `lower` §5 reads the
//! type it was given and picks `Move` for anything that is not `Copy` —
//! correctly, given what it was told. The place it moves still has a `Deref`
//! in it wherever the base was a `&T` (`mir.rs`'s [`Projection::Deref`] doc:
//! *"inserted wherever a field or index is taken through a `&T`"*), and
//! nothing downstream has ever asked whether a `Deref`-rooted place is a
//! legal thing to move. [`science_mir::moves`] tracks whole locals and cannot
//! see the projection at all; [`crate::access`]'s conflict check asks whether
//! some *other* live borrow overlaps, which is a question about a second loan
//! and has nothing to say when there is no second loan to find — exactly the
//! shape of `take(d: &Doc)` reading `d.title`, where the only borrow in sight
//! is the one `d` already is.
//!
//! The result: a program moves the payload of somebody else's value out from
//! under a shared or exclusive reference, the mover drops it once, and the
//! lender's copy of the pointer is now dangling. `crates/science-rt`'s
//! contract calls a double free the one bug it will not catch, and this is how
//! one is produced without a diagnostic anywhere in front of it.
//!
//! # 3. Decision. Refuse every `Move` access whose place contains a `Deref`
//!
//! No dataflow, no liveness, no question of whether the value is read again.
//! A `Deref` step means the projection's root is not this frame's to give
//! away (§2), and that is true of the place on its own, before anything
//! downstream happens to it.
//!
//! **The reason this is unconditional rather than "only when something is
//! still watching"** is that this module cannot answer the conditional
//! version. Whether the lender still needs the value is a fact about the
//! *caller's* frame when the root is a parameter, and about a live region when
//! it is a local borrow — both real questions, neither one this structural a
//! check can ask. Refusing every occurrence over-refuses relative to a
//! points-to analysis that does not exist; it does not over-refuse relative to
//! what this compiler can currently prove safe, which is nothing.
//!
//! **The cost, stated rather than hidden.** §9's crux is genuinely open:
//! reading a non-`Copy` place through a *shared* borrow could instead type as
//! a borrow (Rust's match ergonomics), which would make several of this
//! module's refusals disappear because the operand would never have become a
//! `Move` to begin with. Until that decision — or the narrower one, that this
//! is simply an illegal move — is made in a note, this check is the
//! difference between refusing a program and silently corrupting its heap.
//! `docs/superpowers/design/collections-and-chains.md`'s AMENDMENT 6 (the
//! copy-out rule) decides the analogous question for a closure's return value
//! and nothing else; it is not authority for this site, and this module does
//! not treat it as such.
//!
//! # 4. What is not refused
//!
//! - **A borrow of a place behind a `Deref`.** `&d.title` is
//!   [`science_mir::mir::Rvalue::Ref`], never an [`science_mir::mir::Operand::Move`], and
//!   reborrowing is exactly what §3's second note recommends instead.
//! - **A write through a `Deref`**, such as `doc.title be title` on a
//!   `&mut Doc` — [`crate::access::AccessKind::Write`], not `Move`, and it is
//!   the in-place update rule 4 exists to allow.
//! - **A move whose place is rooted in a local this frame owns outright.**
//!   `consume(doc: Doc) -> String: doc.title` has no `Deref` anywhere in
//!   `doc.title`'s projection, because `doc` is not a reference — it is the
//!   value.
//! - **A copy.** `is_copy` already sends a shared borrow's `mutable: false`
//!   case and every scalar through [`science_mir::mir::Operand::Copy`], which
//!   this module never looks at.
//! - **A move of a type that owns nothing, when this compiler can tell.**
//!   [`moved`]'s §3 item 4 makes the identical exemption for the identical
//!   reason and this module asks the identical query,
//!   [`science_mir::moves::needs_drop`], before reporting anything.
//!   `examples/21_compiler_shapes.science`'s `DefTable.alloc` is the program
//!   that check is for: a record of one `Int` has a declaration this compiler
//!   can walk, `needs_drop` walks it and answers `false`, and nothing here
//!   fires.
//!
//! # 5. What is refused and should not be — recorded rather than hidden
//!
//! `examples/20_extern.science`'s `gemm` moves `a.data`, `b.data` and
//! `c.data` — each of type `ffi.Span[F64]` or `ffi.MutableSpan[F64]` — out
//! of `&`/`&mut` parameters, once each, into an `extern` call. A `Span` is
//! *"a pointer and a length"* (`science-resolve`'s
//! `builtins.rs`), nothing it owns needs releasing, and moving it twice would
//! free nothing twice. **This module still refuses it**, and the reason is
//! upstream of anything it decides: `FFI_TYPES` are names with no declared
//! fields — `builtins.rs`'s own comment is *"an ask, not a fact"* — so
//! `decls.record` finds nothing to walk and `needs_drop`'s catch-all, *"true
//! where it cannot tell"*, answers the only way it can. This is the same
//! conservatism §4's `DefTable.alloc` exemption depends on, pointed the other
//! way by a type with no declaration to be walked at all, and it is not new
//! here: had any program already used an FFI span twice, `SC0301` would carry
//! the identical false positive. This module is the first to make it visible
//! because it fires on one access rather than two. The fix belongs to
//! whichever note gives `Span`/`MutableSpan`/`Pointer`/`OpaqueHandle` a real
//! `Copy` declaration — `ffi-c-boundary.md`, by its own `builtins.rs` account —
//! and not to a structural check that has no way to ask a question about an
//! implementation this compiler cannot look up.

use science_diagnostics::{Diagnostic, Diagnostics, Label};
use science_mir::mir::{Body, Projection};
use science_mir::moves;

use crate::access::AccessKind;
use crate::analysis::BodyAnalysis;
use crate::check::name_of;
use crate::codes;
use crate::regions::Context;

/// §3, run over one body's already-computed accesses.
pub fn move_out_of_borrow(
    context: &mut Context<'_>,
    body: &Body,
    analysis: &BodyAnalysis,
    diagnostics: &mut Diagnostics,
) {
    for point in body.points() {
        let at = analysis.index.index(point);
        for access in analysis.accesses.at(at) {
            if access.kind != AccessKind::Move {
                continue;
            }
            if !access
                .place
                .projection
                .iter()
                .any(|step| matches!(step, Projection::Deref { .. }))
            {
                continue;
            }
            // §4's last bullet: a value that drops nothing cannot be
            // double-freed by being duplicated.
            let ty = access.place.ty(body);
            if !moves::needs_drop(context.decls, context.types, context.aliases, ty) {
                continue;
            }
            let name = name_of(context.defs, body, &access.place);
            diagnostics.push(
                Diagnostic::error(
                    codes::MOVE_OUT_OF_BORROW,
                    "cannot move a value out of a borrow",
                )
                .with_label(Label::primary(
                    access.span,
                    format!("{name} is moved here, and the place it is moved out of is a borrow"),
                ))
                .with_note(
                    "a borrow does not own its referent, so moving through one would leave two \
                     owners of the same value — whoever lent the borrow, and this move's target — \
                     and each would free it",
                )
                .with_note(
                    "borrow the field instead of moving it (`&d.title`), or move the whole \
                     value before any borrow of it is taken",
                ),
            );
        }
    }
}
