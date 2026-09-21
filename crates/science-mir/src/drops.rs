//! Drop elaboration — Decision 26, and the only storage the compiler adds.
//!
//! > **Decision 26.** A local that is conditionally moved gets a
//! > compiler-generated drop flag — one byte, set where the value is
//! > initialised, cleared where it is moved, tested at the drop point — and
//! > elaboration runs during MIR construction, before region inference.
//!
//! # 1. The three outcomes, and which local gets which
//!
//! [`crate::lower`] emits a `Drop` at every scope exit for every local whose
//! type [`crate::moves::needs_drop`] admits, unconditionally and without
//! looking at any path. This pass then reads [`crate::moves`] at each of those
//! drops and decides:
//!
//! | State at the drop | What happens | What it costs |
//! |---|---|---|
//! | [`State::Init`] | the drop stands, with no flag | nothing |
//! | [`State::Gone`] or [`State::Unreached`] | the terminator becomes a `Goto` | nothing |
//! | [`State::Maybe`] | a flag is allocated and the drop tests it | one byte and one branch |
//!
//! **The third row is the whole of Decision 26 and the first two are why it is
//! affordable.** §12: *"flags are generated only for locals the move analysis
//! proves conditionally moved, never for every local, which is the difference
//! between a rare cost and a tax on every function"*. A body with no
//! conditional move gets no flags at all, and [`Body::drop_flags`] is then
//! empty — which `tests/drops.rs` asserts on the ordinary case rather than
//! trusting this paragraph.
//!
//! # 2. Where the flag is written
//!
//! One flag per local, not per drop point: a local with two drops on two paths
//! has one byte, because the byte is a fact about the *value* and not about the
//! drop.
//!
//! The writes are placed at the events the analysis already identifies, and at
//! no others:
//!
//! - `SetDropFlag(false)` after every `StorageLive` and `StorageDead` of the
//!   guarded local — storage without a value is nothing to release.
//! - `SetDropFlag(true)` after every whole-local assignment to it, and at the
//!   start of a call's return block when the call's destination is it.
//! - `SetDropFlag(false)` after every statement that moves out of it, and at
//!   the start of a `Drop`'s successor.
//! - `SetDropFlag(true)` at the top of the entry block for a parameter, which
//!   arrives holding a value and has no `StorageLive` to hang the write on.
//!
//! **Why placement follows the analysis rather than the syntax.** The
//! alternative — writing the flag wherever a `let` or a move appears in the
//! source — is the same list computed twice, and the two copies disagree at
//! exactly the cases that matter: a move inside a branch that the lowering
//! turned into two blocks. One list, derived from the CFG the flag will be read
//! on, cannot disagree with itself.
//!
//! # 3. The cost this imposes on codegen, stated plainly
//!
//! `codegen-and-linking.md` Decision 5 says *"every MIR basic block becomes
//! exactly one LLVM basic block, and codegen merges nothing"*, so that a MIR
//! dump and an IR dump are diffable. A flagged `Drop` breaks that: it is one
//! MIR block and three LLVM ones — test, call, continue.
//!
//! The alternative is to elaborate the branch here, into MIR, which keeps that
//! note's property and costs this one: region inference would then see a
//! conditional drop as ordinary control flow and lose the fact that the two
//! paths are the same drop. [`crate::mir`]'s §4 is the argument. The amendment
//! owed to `codegen-and-linking.md` is recorded in `lib.rs`'s §7 rather
//! than left for that note's author to discover by trying to lower one.
//!
//! # 4. A record's `Drop`, elaborated per field. `moves`'s §3 amendment.
//!
//! [`crate::lower`] still emits exactly one `Drop` per local, for the whole
//! local, never a projection — that invariant is unchanged and this pass
//! still trusts it. What changed is what this pass does with that one
//! terminator when [`moves::analyse_fields`] has an opinion about it: instead
//! of reading one [`State`] off `local`, it reads one per direct field and
//! reaches one of three outcomes, chosen with the identical table §1 already
//! uses, applied per field rather than per local:
//!
//! - **Every owning field is [`State::Init`].** The terminator is left
//!   exactly as `lower` built it — one call that drops the whole record — because
//!   that call already does the right thing and a local this pass never edits
//!   cannot be a local this pass gets wrong.
//! - **Every owning field is [`State::Gone`] or [`State::Unreached`].** The
//!   terminator becomes a `Goto`, exactly like §1's second row, now decided
//!   at record granularity rather than local granularity.
//! - **Otherwise.** The one `Drop` is replaced by a chain of one `Drop` per
//!   *remaining* owning field — a field proven `Gone` is skipped outright, one
//!   proven `Init` gets an unconditional `Drop`, and one that is `Maybe` gets
//!   its own flag, allocated and written the same way §2 writes a whole
//!   local's. [`rewrite_record_drop`] builds the chain and [`write_field_flags`]
//!   is §2's [`write_flags`] read at field granularity, for the identical
//!   reason: two independently-written copies of "where does a flag get
//!   written" are two copies that can disagree, and this analysis already has
//!   one true answer to that question in [`moves::move_event`] and
//!   [`moves::assign_event`], shared by both.
//!
//! **This is the fix for the leak `moves.rs`'s §3 named**: `let p be
//! Pair(...)` then `let x be p.first` used to mark the whole of `p` `Gone`,
//! so `p`'s scope-exit `Drop` was deleted outright and `p.second` leaked. A
//! record local now answers `fields_of` with `Some([...])`, its `Drop` is
//! elaborated by this section instead of §1's, and the chain drops `second`
//! on its own — unconditionally, because in that program `second` is `Init`
//! at every reaching point and never merely `Maybe`.
//!
//! **What stays whole.** A local whose type is not a record
//! ([`moves::analyse_fields`]'s `fields_of` answers `None`), and any path
//! reached through more than one projection step off a record local — `p.a.b`
//! collapses to `p.a` going `Gone` whole, per `moves`'s own §3 amendment —
//! are exactly as imprecise as before this section existed. That is this
//! landing's declared boundary, not an oversight: `moves.rs`'s §3 already
//! prices one level of precision against the machinery a full tree of move
//! paths would cost, and one level is what this section spends.

use science_diagnostics::Span;
use science_resolve::hir::DefId;
use science_types::alias::Aliases;
use science_types::items::Declarations;
use science_types::ty::{Ty, Types};

use crate::mir::{
    BasicBlock, BlockId, Body, Local, LocalDecl, LocalKind, Operand, Place, Projection, Statement,
    StatementKind, Terminator, TerminatorKind,
};
use crate::moves::{self, FieldEvent, State};

/// Runs the elaboration in place. `flag_ty` is `Bool`, or [`Ty::ERROR`] in a
/// compilation with no prelude to find it in. `decls`, `types` and `aliases`
/// are §4's: [`moves::analyse_fields`] needs them to find a local's record
/// fields, the same table and the same two mutable tables
/// [`moves::needs_drop`] already reads through this module's other caller,
/// `crate::lower::Builder::emit_drop_if_needed`.
pub fn elaborate(body: &mut Body, flag_ty: Ty, decls: &Declarations, types: &mut Types, aliases: &mut Aliases) {
    let analysis = moves::analyse(body);
    let field_analysis = moves::analyse_fields(body, decls, types, aliases);

    // Pass one: decide, for every `Drop`, which of §1's three rows it is in —
    // or, for a decomposed local, route it to §4 instead. Decisions are
    // collected before anything is edited, because editing changes the
    // statement numbering the analysis was computed against.
    let mut delete: Vec<BlockId> = Vec::new();
    let mut guarded: Vec<(BlockId, Local)> = Vec::new();
    let mut records: Vec<RecordDrop> = Vec::new();
    for (id, block) in body.blocks() {
        let TerminatorKind::Drop { place, target, .. } = &block.terminator.kind else {
            continue;
        };
        let local = place.local;
        if let Some(decl_fields) = field_analysis.fields_of(local) {
            if !decl_fields.is_empty() {
                let field_states = field_analysis
                    .before_terminator(body, id, local)
                    .expect("`fields_of` and `before_terminator` agree on which locals are tracked");
                let owning: Vec<(DefId, Ty, State)> = decl_fields
                    .iter()
                    .zip(field_states)
                    .filter(|((_, ty), _)| moves::needs_drop(decls, types, aliases, *ty))
                    .map(|((field, ty), state)| (*field, *ty, state))
                    .collect();
                records.push(RecordDrop { block: id, local, target: *target, fields: owning });
                continue;
            }
        }
        let states = analysis.before_terminator(body, id);
        match states[local.index()] {
            State::Init => {}
            State::Gone | State::Unreached => delete.push(id),
            State::Maybe => guarded.push((id, local)),
        }
    }

    for block in delete {
        let span = body.blocks[block.index()].terminator.span;
        if let TerminatorKind::Drop { target, .. } = body.blocks[block.index()].terminator.kind {
            body.blocks[block.index()].terminator.kind = TerminatorKind::Goto { target };
            body.blocks[block.index()].terminator.span = span;
        }
    }

    // §4: rewrite every decomposed local's `Drop`, collecting the per-field
    // flags it allocates along the way.
    let mut field_flags: Vec<(Local, DefId, Local)> = Vec::new();
    for record in &records {
        rewrite_record_drop(body, record, flag_ty, &mut field_flags);
    }

    if !field_flags.is_empty() {
        write_field_flags(body, &field_flags);
    }

    if guarded.is_empty() {
        return;
    }

    // Pass two: one flag per guarded local, in the order the locals were
    // declared, so that two runs over the same body number them the same way.
    let mut flags: Vec<(Local, Local)> = Vec::new();
    let mut locals: Vec<Local> = guarded.iter().map(|(_, local)| *local).collect();
    locals.sort();
    locals.dedup();
    for local in locals {
        let span = body.local_decl(local).span;
        let flag = Local::from_index(body.locals.len());
        body.locals.push(LocalDecl { ty: flag_ty, kind: LocalKind::DropFlag(local), span });
        flags.push((local, flag));
    }

    for (block, local) in guarded {
        let flag = flags.iter().find(|(guarded, _)| *guarded == local).map(|(_, flag)| *flag);
        if let TerminatorKind::Drop { flag: slot, .. } = &mut body.blocks[block.index()].terminator.kind
        {
            *slot = flag;
        }
    }

    write_flags(body, &flags);
}

/// One `Drop` of a decomposed record local, and the per-field decision §4
/// reached for it — collected in pass one, applied in
/// [`rewrite_record_drop`], for the same reason the whole-local passes above
/// collect before they edit.
struct RecordDrop {
    block: BlockId,
    local: Local,
    target: BlockId,
    /// Only the fields [`moves::needs_drop`] admits — a field that drops
    /// nothing never enters this list, so it can never make the chain longer
    /// and never needs a state at all.
    fields: Vec<(DefId, Ty, State)>,
}

/// Applies one [`RecordDrop`]'s decision. §4's three rows.
fn rewrite_record_drop(
    body: &mut Body,
    record: &RecordDrop,
    flag_ty: Ty,
    field_flags: &mut Vec<(Local, DefId, Local)>,
) {
    let RecordDrop { block, local, target, fields } = record;
    let span = body.blocks[block.index()].terminator.span;

    if fields.is_empty() || fields.iter().all(|(_, _, state)| matches!(state, State::Gone | State::Unreached)) {
        body.blocks[block.index()].terminator.kind = TerminatorKind::Goto { target: *target };
        body.blocks[block.index()].terminator.span = span;
        return;
    }
    if fields.iter().all(|(_, _, state)| *state == State::Init) {
        // Every owning field is fully there: the whole-record `Drop`
        // `crate::lower` already built is already the cheapest correct
        // answer, and this pass's whole job here is to leave it alone.
        return;
    }

    // Mixed: one `Drop` per remaining field, chained in declaration order —
    // a field proven `Gone` is skipped outright, never given a block of its
    // own.
    let decl_span = body.local_decl(*local).span;
    let base = Place::local(*local);
    let mut chain: Vec<(Place, Option<Local>)> = Vec::new();
    for (field, ty, state) in fields {
        match state {
            State::Gone | State::Unreached => {}
            State::Init => {
                chain.push((base.project(Projection::Field { field: *field, ty: *ty }), None));
            }
            State::Maybe => {
                let flag = Local::from_index(body.locals.len());
                body.locals.push(LocalDecl {
                    ty: flag_ty,
                    kind: LocalKind::DropFlag(*local),
                    span: decl_span,
                });
                field_flags.push((*local, *field, flag));
                chain.push((
                    base.project(Projection::Field { field: *field, ty: *ty }),
                    Some(flag),
                ));
            }
        }
    }

    if chain.is_empty() {
        body.blocks[block.index()].terminator.kind = TerminatorKind::Goto { target: *target };
        body.blocks[block.index()].terminator.span = span;
        return;
    }

    // One block per link but the first, which reuses `block` itself — the
    // same "reuse what already exists, invent only what does not" shape
    // `science-codegen-llvm`'s `emit_flagged_drop` already reads a single
    // flagged `Drop` as.
    let mut ids: Vec<BlockId> = vec![*block];
    for _ in 1..chain.len() {
        let id = BlockId::from_index(body.blocks.len());
        body.blocks.push(BasicBlock {
            statements: Vec::new(),
            terminator: Terminator { kind: TerminatorKind::Unreachable, span },
        });
        ids.push(id);
    }
    for (index, (place, flag)) in chain.into_iter().enumerate() {
        let next = ids.get(index + 1).copied().unwrap_or(*target);
        body.blocks[ids[index].index()].terminator =
            Terminator { kind: TerminatorKind::Drop { place, flag, target: next }, span };
    }
}

/// One insertion: a statement to place before `index` in `block`. §2.
struct Insertion {
    block: BlockId,
    index: usize,
    statement: Statement,
}

fn write_flags(body: &mut Body, flags: &[(Local, Local)]) {
    let mut insertions: Vec<Insertion> = Vec::new();
    let set = |flag: Local, value: bool, span: Span| Statement {
        kind: StatementKind::SetDropFlag { flag, value },
        span,
    };

    // A parameter arrives holding a value and has no `StorageLive`.
    let params: Vec<Local> = body.params().collect();
    for (guarded, flag) in flags {
        if params.contains(guarded) {
            insertions.push(Insertion {
                block: crate::mir::ENTRY_BLOCK,
                index: 0,
                statement: set(*flag, true, body.local_decl(*guarded).span),
            });
        }
    }

    for (id, block) in body.blocks() {
        for (at, statement) in block.statements.iter().enumerate() {
            let span = statement.span;
            match &statement.kind {
                StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                    if let Some(flag) = flag_of(flags, *local) {
                        insertions.push(Insertion {
                            block: id,
                            index: at + 1,
                            statement: set(flag, false, span),
                        });
                    }
                }
                StatementKind::Assign { place, rvalue } => {
                    for moved in moves::moved_locals(rvalue) {
                        if let Some(flag) = flag_of(flags, moved) {
                            insertions.push(Insertion {
                                block: id,
                                index: at + 1,
                                statement: set(flag, false, span),
                            });
                        }
                    }
                    if place.is_local() {
                        if let Some(flag) = flag_of(flags, place.local) {
                            insertions.push(Insertion {
                                block: id,
                                index: at + 1,
                                statement: set(flag, true, span),
                            });
                        }
                    }
                }
                StatementKind::SetDropFlag { .. }
                | StatementKind::Activate(_)
                | StatementKind::Nop => {}
            }
        }

        let span = block.terminator.span;
        match &block.terminator.kind {
            TerminatorKind::Call { args, destination, target, .. } => {
                for arg in args {
                    if let Operand::Move(place) = arg {
                        if let Some(flag) = flag_of(flags, place.local) {
                            insertions.push(Insertion {
                                block: id,
                                index: block.statements.len(),
                                statement: set(flag, false, span),
                            });
                        }
                    }
                }
                if let Some(target) = target {
                    if destination.is_local() {
                        if let Some(flag) = flag_of(flags, destination.local) {
                            insertions.push(Insertion {
                                block: *target,
                                index: 0,
                                statement: set(flag, true, span),
                            });
                        }
                    }
                }
            }
            TerminatorKind::Drop { place, target, .. } => {
                if let Some(flag) = flag_of(flags, place.local) {
                    insertions.push(Insertion {
                        block: *target,
                        index: 0,
                        statement: set(flag, false, span),
                    });
                }
            }
            _ => {}
        }
    }

    // Applied back to front within each block, so that an earlier insertion
    // does not move the index of a later one.
    insertions.sort_by(|left, right| {
        left.block
            .index()
            .cmp(&right.block.index())
            .then(right.index.cmp(&left.index))
    });
    for insertion in insertions {
        body.blocks[insertion.block.index()]
            .statements
            .insert(insertion.index, insertion.statement);
    }
}

fn flag_of(flags: &[(Local, Local)], local: Local) -> Option<Local> {
    flags.iter().find(|(guarded, _)| *guarded == local).map(|(_, flag)| *flag)
}

/// [`write_flags`] read at field granularity — one flag per `(local, field)`
/// pair rather than one per local, driven by [`moves::move_event`] and
/// [`moves::assign_event`] instead of the whole-local reduction
/// [`moves::moved_locals`] makes, so that this pass's idea of where a flag
/// changes cannot drift from [`moves::analyse_fields`]'s. §4.
fn write_field_flags(body: &mut Body, flags: &[(Local, DefId, Local)]) {
    let mut insertions: Vec<Insertion> = Vec::new();
    let set = |flag: Local, value: bool, span: Span| Statement {
        kind: StatementKind::SetDropFlag { flag, value },
        span,
    };

    let flags_for = |local: Local| -> Vec<Local> {
        flags.iter().filter(|(guarded, _, _)| *guarded == local).map(|(_, _, flag)| *flag).collect()
    };
    let flag_for_field = |local: Local, field: DefId| -> Option<Local> {
        flags
            .iter()
            .find(|(guarded, guarded_field, _)| *guarded == local && *guarded_field == field)
            .map(|(_, _, flag)| *flag)
    };

    // A record parameter arrives holding a value: every field flag it has
    // starts true, §2's rule for a whole local's flag applied to each field's.
    let params: Vec<Local> = body.params().collect();
    for local in &params {
        for flag in flags_for(*local) {
            insertions.push(Insertion {
                block: crate::mir::ENTRY_BLOCK,
                index: 0,
                statement: set(flag, true, body.local_decl(*local).span),
            });
        }
    }

    for (id, block) in body.blocks() {
        for (at, statement) in block.statements.iter().enumerate() {
            let span = statement.span;
            match &statement.kind {
                StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                    for flag in flags_for(*local) {
                        insertions.push(Insertion {
                            block: id,
                            index: at + 1,
                            statement: set(flag, false, span),
                        });
                    }
                }
                StatementKind::Assign { place, rvalue } => {
                    for moved in moves::moved_places(rvalue) {
                        match moves::move_event(moved) {
                            FieldEvent::Whole => {
                                for flag in flags_for(moved.local) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: at + 1,
                                        statement: set(flag, false, span),
                                    });
                                }
                            }
                            FieldEvent::Field(field) => {
                                if let Some(flag) = flag_for_field(moved.local, field) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: at + 1,
                                        statement: set(flag, false, span),
                                    });
                                }
                            }
                        }
                    }
                    if let Some(event) = moves::assign_event(place) {
                        match event {
                            FieldEvent::Whole => {
                                for flag in flags_for(place.local) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: at + 1,
                                        statement: set(flag, true, span),
                                    });
                                }
                            }
                            FieldEvent::Field(field) => {
                                if let Some(flag) = flag_for_field(place.local, field) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: at + 1,
                                        statement: set(flag, true, span),
                                    });
                                }
                            }
                        }
                    }
                }
                StatementKind::SetDropFlag { .. }
                | StatementKind::Activate(_)
                | StatementKind::Nop => {}
            }
        }

        let span = block.terminator.span;
        match &block.terminator.kind {
            TerminatorKind::Call { args, destination, target, .. } => {
                for arg in args {
                    if let Operand::Move(place) = arg {
                        match moves::move_event(place) {
                            FieldEvent::Whole => {
                                for flag in flags_for(place.local) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: block.statements.len(),
                                        statement: set(flag, false, span),
                                    });
                                }
                            }
                            FieldEvent::Field(field) => {
                                if let Some(flag) = flag_for_field(place.local, field) {
                                    insertions.push(Insertion {
                                        block: id,
                                        index: block.statements.len(),
                                        statement: set(flag, false, span),
                                    });
                                }
                            }
                        }
                    }
                }
                if let Some(target) = target {
                    if let Some(event) = moves::assign_event(destination) {
                        match event {
                            FieldEvent::Whole => {
                                for flag in flags_for(destination.local) {
                                    insertions.push(Insertion {
                                        block: *target,
                                        index: 0,
                                        statement: set(flag, true, span),
                                    });
                                }
                            }
                            FieldEvent::Field(field) => {
                                if let Some(flag) = flag_for_field(destination.local, field) {
                                    insertions.push(Insertion {
                                        block: *target,
                                        index: 0,
                                        statement: set(flag, true, span),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            // The successor of a field's own flagged `Drop`, cleared for the
            // same reason §2 clears a whole local's there: a loop's back edge
            // can reach this same `Drop` again before the field is
            // reinitialised, and the byte has to say `false` when it does.
            TerminatorKind::Drop { place, flag: Some(_), target } => {
                if let [Projection::Field { field, .. }] = place.projection.as_slice() {
                    if let Some(flag) = flag_for_field(place.local, *field) {
                        insertions.push(Insertion {
                            block: *target,
                            index: 0,
                            statement: set(flag, false, span),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    insertions.sort_by(|left, right| {
        left.block
            .index()
            .cmp(&right.block.index())
            .then(right.index.cmp(&left.index))
    });
    for insertion in insertions {
        body.blocks[insertion.block.index()]
            .statements
            .insert(insertion.index, insertion.statement);
    }
}
