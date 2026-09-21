//! The move analysis, and the one question it exists to answer.
//!
//! # 1. What this is for, and what it is not
//!
//! `type-checking-and-mir.md` Decision 26 asks for one thing: *"flags are
//! generated **only** for locals the move analysis proves conditionally
//! moved, never for every local, which is the difference between a rare cost
//! and a tax on every function"*. This module is that proof.
//!
//! **It is still not a use-after-move check**, and it is now what one is
//! written against. `SC0301` is the core spec's §6.1 rule 3 and belongs to a
//! phase that reports; this pass reports nothing, has no `Diagnostics` and
//! cannot be made to emit one, which is `lib.rs`'s §3 and not a property of
//! this module.
//!
//! **The second consumer arrived and the lattice did not move.** This entry
//! used to end *"sharing the dataflow later is the right move; sharing it now,
//! before there is a consumer, would fix the lattice around a second question
//! before that question has been asked"*. The consumer is
//! `science-regions`' `moved`, and the question it asks — *"what is this
//! local's state at the point the program reads it"* — needed one new reader,
//! [`Moves::walk`], and no change to [`State`], to the join, or to what a
//! statement does. So the bet §2 took is settled: three values were enough for
//! both questions, and the reason to record that is that it was not obvious in
//! advance and the alternative was irreversible.
//!
//! # 2. The lattice, which is three values and not a bitset pair
//!
//! **Decision. A local is `Init`, `Gone`, or `Maybe`, joined pointwise, with
//! `Unreached` as the bottom for a block no edge has reached yet.**
//!
//! rustc tracks two bitsets — *maybe-initialised* and *maybe-uninitialised* —
//! and reads the drop decision off the pair. Three values is the same
//! information: `Init` is the pair `(true, false)`, `Gone` is `(false, true)`
//! and `Maybe` is `(true, true)`. `(false, false)` is `Unreached`.
//!
//! The reason to name them is that the *output* of this pass is a
//! three-way decision — drop, do not drop, test the flag — and a
//! representation whose values are the decisions cannot be read off wrongly.
//! The cost is that a fourth question wanting a different join has to widen the
//! enum rather than add a bitset.
//!
//! # 3. The granularity, and the direction its imprecision leaks
//!
//! **Decision. The analysis tracks whole locals, not move paths.** A move out
//! of `doc.title` marks `doc` as moved, not `doc.title`.
//!
//! rustc tracks a tree of move paths, so that `doc` with `doc.title` moved is
//! dropped field by field with the moved field skipped. That is more precise
//! and it is more machinery than this crate should carry before anything
//! consumes the precision.
//!
//! **The direction matters and it was chosen.** Under whole-local tracking, a
//! partially moved local is `Maybe`, gets a flag, and the flag is *cleared* by
//! the field move — so the local is not dropped at all, and the fields that
//! were not moved leak. The alternative reading, dropping the whole local,
//! frees the moved field twice. `crates/science-rt`'s contract is explicit that
//! a double `_free` is a bug it will not catch, and §12 names this phase as
//! *"the phase that has to be right"*. A leak is a bug a profiler finds; a
//! double free is a bug a user finds. The imprecision leaks toward the leak.
//!
//! **And toward a false positive, for the consumer that arrived second.** The
//! paragraph above prices this against *drop elaboration*, which is the only
//! thing that read the analysis when it was written. A **check** reads the same
//! `Gone` the other way round: `Scopes.lookup` in
//! `examples/21_compiler_shapes.science` compares `binding.name` and then reads
//! `binding.definition`, and whole-local tracking hears the whole `binding` go.
//! `science-regions`' `moved` §3 item 3 therefore abandons any local whose
//! reaching move went through a projection, which loses the errors that are
//! real. **The direction of a conservatism is relative to its consumer**, and
//! the entry that closes both is move paths — the thing this section declined
//! to build, now with a second reason to.
//!
//! **The amendment: one field deep, for a record, and only for the leak.**
//! [`analyse_fields`] tracks each direct field of a record-typed local as its
//! own path, joined with the identical [`State::join`] §2 already has — not a
//! tree of move paths, one level of it. A move of `doc.title` now marks
//! `title`'s own path `Gone` and leaves `doc.body`'s alone; a move of
//! `doc.title.chars` (a path this still declines to build) collapses to
//! `title`'s path going `Gone` whole, which is this section's own direction
//! read one level down rather than a new one invented for the occasion.
//!
//! **Why a second, parallel analysis and not a wider [`State`] table.** §2's
//! bound is that the *values* [`State`] holds cannot grow past the
//! three-way decision without inventing a join for a fourth question; it says
//! nothing against asking the same three-way question about more things.
//! [`FieldMoves`] is exactly that: the same lattice, the same join, a
//! different index — one path per tracked field instead of one path per
//! local — computed by [`apply_statement_fields`] and
//! [`apply_terminator_fields`], which are [`apply_statement`] and
//! [`apply_terminator`] re-read at field granularity and not two new rules.
//! [`crate::drops`] is the only reader; [`analyse`] and everything that
//! already consumes it — `science-regions`' `moved` chief among them — is
//! unchanged, so this local's own false-negative (§3 item 3, above) is not
//! narrower today than it was before this paragraph.
//!
//! **What it does not do.** A local whose type is not a record — a `String`,
//! an `Array`, a choice, a type parameter — gets no fields at all
//! ([`record_fields`] answers `None`), and drop elaboration reads that as
//! "stay whole", the same answer it always got. A deeper path collapses to
//! its containing field going `Gone`, never to a guess about what is inside
//! it: the direction is still leak-over-double-free, one level down.
//!
//! # 4. `needs_drop`, and what it cannot know
//!
//! [`needs_drop`] answers *"does dropping this type run anything"*. It is
//! structural over the prelude's primitives and the declared fields of a
//! record, and it answers **true** for everything else — a choice type, an
//! interface object, a type parameter, a foreign union.
//!
//! **It cannot ask whether a type implements `Drop`**, because that is
//! Decision 11's implementation lookup and `science-types` does not have one.
//! Answering `true` where it cannot tell is the direction that costs a drop
//! that does nothing rather than a value that is never released, and the false
//! answers it produces disappear — without a change here — the day the lookup
//! lands and this function can ask.
//!
//! **It no longer lives here.** `science_types::ownership`'s module doc §1
//! says why: `science-types`' own checker gained a second question with the
//! identical answer, and this crate cannot be the one place that question is
//! implemented without `science-types` depending back on it. What is below is
//! a re-export, kept at this path so every existing caller — this module's own
//! `lower.rs`, and `science-regions`' `moved` and `deref_move` — is unchanged.

use std::collections::HashMap;

use science_resolve::hir::DefId;
use science_types::alias::Aliases;
use science_types::items::Declarations;
use science_types::ty::{Ty, TyKind, Types};
use science_types::Substitution;

use crate::mir::{
    reverse_postorder, BlockId, Body, Local, Operand, Place, Projection, Rvalue, StatementKind,
    TerminatorKind, ENTRY_BLOCK,
};

/// What is known about a local's value at a point. §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No edge has reached this point yet. The bottom of the lattice, and the
    /// identity of [`State::join`].
    Unreached,
    /// The local definitely holds a value.
    Init,
    /// The local definitely holds nothing: never initialised, moved out, or
    /// already dropped.
    Gone,
    /// It depends on the path. **This, and only this, is what Decision 26
    /// generates a flag for.**
    Maybe,
}

impl State {
    pub fn join(self, other: State) -> State {
        match (self, other) {
            (State::Unreached, value) | (value, State::Unreached) => value,
            (left, right) if left == right => left,
            _ => State::Maybe,
        }
    }
}

/// The per-block entry states, at a fixed point.
#[derive(Debug, Clone)]
pub struct Moves {
    /// Indexed by block, then by local.
    entry: Vec<Vec<State>>,
}

impl Moves {
    /// The states on entry to a block.
    pub fn on_entry(&self, block: crate::mir::BlockId) -> &[State] {
        &self.entry[block.index()]
    }

    /// The states just before a block's terminator.
    ///
    /// Recomputed by replaying the block rather than stored per point: a body
    /// has far more points than blocks, and this consumer — drop elaboration —
    /// asks about terminators, which is where every [`TerminatorKind::Drop`]
    /// is.
    pub fn before_terminator(&self, body: &Body, block: crate::mir::BlockId) -> Vec<State> {
        let mut states = self.walk(body, block);
        states.pop().expect("`walk` always ends with the terminator's states")
    }

    /// The states at **every** point of a block, in point order: one entry per
    /// statement and a last one for the terminator.
    ///
    /// **The decision. The replay is public, per point, and still not stored.**
    /// [`before_terminator`](Moves::before_terminator) is one element of this
    /// and is now written in terms of it, so the two cannot disagree about what
    /// a statement does.
    ///
    /// **The reason it is here at all** is that §1's *"sharing the dataflow
    /// later is the right move; sharing it now, before there is a consumer,
    /// would fix the lattice around a second question before that question has
    /// been asked"* has come due. The second consumer is
    /// `science-regions`' use-after-move check, which asks *"what is this
    /// local's state where the program reads it"* — a question about a
    /// statement, not about a terminator — and the lattice did not have to
    /// change to answer it, which is the outcome §1 was holding out for.
    ///
    /// **The cost** is one `Vec<State>` per point of the block, allocated per
    /// call and not cached. A caller that wants the whole body calls this once
    /// per block, which is the shape both consumers have.
    pub fn walk(&self, body: &Body, block: crate::mir::BlockId) -> Vec<Vec<State>> {
        let statements = &body.block(block).statements;
        let mut out = Vec::with_capacity(statements.len() + 1);
        let mut states = self.entry[block.index()].clone();
        for statement in statements {
            out.push(states.clone());
            apply_statement(&statement.kind, &mut states);
        }
        out.push(states);
        out
    }
}

/// Runs the analysis to a fixed point.
///
/// Reverse post-order with a change flag, which is the standard worklist made
/// cheap: a forward analysis over a reducible CFG converges in one pass per
/// loop nesting level, and the bodies this runs over are shallow.
pub fn analyse(body: &Body) -> Moves {
    let count = body.local_count();
    let mut entry = vec![vec![State::Unreached; count]; body.block_count()];

    // On entry to the body: the parameters hold values, and nothing else does.
    for local in body.params() {
        entry[crate::mir::ENTRY_BLOCK.index()][local.index()] = State::Init;
    }
    for slot in entry[crate::mir::ENTRY_BLOCK.index()].iter_mut() {
        if *slot == State::Unreached {
            *slot = State::Gone;
        }
    }

    let order = reverse_postorder(body);
    let mut changed = true;
    while changed {
        changed = false;
        for block in &order {
            let mut states = entry[block.index()].clone();
            for statement in &body.block(*block).statements {
                apply_statement(&statement.kind, &mut states);
            }
            let terminator = &body.block(*block).terminator.kind;
            for successor in terminator.successors() {
                let mut outgoing = states.clone();
                apply_terminator(terminator, successor, &mut outgoing);
                let target = &mut entry[successor.index()];
                for (slot, incoming) in target.iter_mut().zip(outgoing) {
                    let joined = slot.join(incoming);
                    if *slot != joined {
                        *slot = joined;
                        changed = true;
                    }
                }
            }
        }
    }
    Moves { entry }
}

/// A local's state after one statement.
fn apply_statement(kind: &StatementKind, states: &mut [State]) {
    match kind {
        StatementKind::Assign { place, rvalue } => {
            for moved in moved_locals(rvalue) {
                states[moved.index()] = State::Gone;
            }
            // Only a whole-local assignment initialises. A write through a
            // projection leaves the local's state alone: §3's granularity, and
            // a partially written local is already `Maybe` or `Init` from the
            // assignment that made it whole.
            if place.is_local() {
                states[place.local.index()] = State::Init;
            }
        }
        // Storage beginning or ending both mean *nothing is there*. The first
        // is the honest reading of `StorageLive`: it gives the local room, not
        // a value.
        StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
            states[local.index()] = State::Gone;
        }
        StatementKind::SetDropFlag { .. }
        | StatementKind::Activate(_)
        | StatementKind::Nop => {}
    }
}

/// A local's state after a terminator, on the edge to `successor`.
fn apply_terminator(kind: &TerminatorKind, successor: crate::mir::BlockId, states: &mut [State]) {
    match kind {
        TerminatorKind::Call { args, destination, target, .. } => {
            for arg in args {
                if let Some(place) = arg.moved_place() {
                    states[place.local.index()] = State::Gone;
                }
            }
            if *target == Some(successor) && destination.is_local() {
                states[destination.local.index()] = State::Init;
            }
        }
        TerminatorKind::Drop { place, .. } => {
            states[place.local.index()] = State::Gone;
        }
        TerminatorKind::If { cond, .. } => {
            if let Some(place) = cond.moved_place() {
                states[place.local.index()] = State::Gone;
            }
        }
        TerminatorKind::Switch { discr, .. } => {
            if let Some(place) = discr.moved_place() {
                states[place.local.index()] = State::Gone;
            }
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

/// Every place an rvalue moves out of.
///
/// A borrow is not one of them, which is the whole reason `Ref` is an rvalue
/// rather than an operand: `borrowed x` reads `x` without consuming it, and a
/// representation that made it an operand would have had to invent a third
/// operand kind to say so.
///
/// **Places, not locals** — [`moved_locals`] is this with the projection
/// thrown away, kept as the reduction whole-local tracking always used. The
/// field analysis below needs the projection, to tell `p.first` moving from
/// `p` moving.
pub fn moved_places(rvalue: &Rvalue) -> Vec<&Place> {
    // A named function rather than a capturing closure: a closure's
    // parameter gets its own, higher-ranked lifetime, which is exactly wrong
    // here — every place pushed has to outlive `rvalue` itself, not just one
    // call to `take`.
    fn take<'a>(out: &mut Vec<&'a Place>, operand: &'a Operand) {
        if let Some(place) = operand.moved_place() {
            out.push(place);
        }
    }

    let mut out: Vec<&Place> = Vec::new();
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::Unary { operand, .. }
        | Rvalue::Cast { operand, .. }
        | Rvalue::IsPresent(operand)
        | Rvalue::Coerce { operand, .. }
        | Rvalue::Narrow { operand, .. } => take(&mut out, operand),
        Rvalue::Binary { lhs, rhs, .. } => {
            take(&mut out, lhs);
            take(&mut out, rhs);
        }
        Rvalue::Range { start, end, .. } => {
            take(&mut out, start);
            take(&mut out, end);
        }
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                take(&mut out, operand);
            }
        }
        Rvalue::Variant { payload, .. } => {
            for operand in payload {
                take(&mut out, operand);
            }
        }
        // A closure's captures are operands like any other aggregate's:
        // `lower`'s §8 makes each one a reference taken in the statement
        // before, so the move seen here is the move of that reference into the
        // closure value. The *capture* is a borrow and moves nothing, which is
        // the discipline's whole point.
        Rvalue::Tuple(operands) | Rvalue::Closure { captures: operands, .. } => {
            for operand in operands {
                take(&mut out, operand);
            }
        }
        Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Error => {}
    }
    out
}

/// Every local an rvalue moves out of. [`moved_places`] with the projection
/// dropped, which is §3's original reduction and is unchanged: every existing
/// caller of this function still gets whole-local answers.
pub fn moved_locals(rvalue: &Rvalue) -> Vec<Local> {
    moved_places(rvalue).into_iter().map(|place| place.local).collect()
}

// --- the field amendment, §3 ------------------------------------------------

/// What moving or writing through `place` does to its local's fields: every
/// field together, or one field by name.
///
/// Shared between [`analyse_fields`]'s own dataflow and `crate::drops`' flag
/// writer, so the two answer the same question about the same statement by
/// construction — `drops`' own §2 already makes this argument about the
/// whole-local flags and it applies here without a word changed: *"one list,
/// derived from the CFG the flag will be read on, cannot disagree with
/// itself."*
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEvent {
    /// The whole local: every tracked field is caught up in one event.
    Whole,
    /// One field, named, projected directly off the local.
    Field(DefId),
}

/// The event a move (or a move-shaped read — `TerminatorKind::If`'s
/// condition among them) through `place` fires.
///
/// Anything past one field step, or a first step that is not a field at all
/// (`Deref`, `TupleField`, `Index`, `Downcast`), collapses to
/// [`FieldEvent::Whole`] — §3's declined-to-build direction, read one level
/// down: the alternative is inventing a state for a path this crate does not
/// track, and the guess that cannot double-free is "all of it moved".
pub fn move_event(place: &Place) -> FieldEvent {
    match place.projection.as_slice() {
        [] => FieldEvent::Whole,
        [Projection::Field { field, .. }] => FieldEvent::Field(*field),
        _ => FieldEvent::Whole,
    }
}

/// The event a write through `place` fires, or `None` when the write is
/// through a projection this analysis does not track — deeper than one
/// field, or not a field. A write initialises only the exact storage it
/// touches, so unlike [`move_event`] there is no direction to guess in:
/// claiming a field is whole when only part of it was written is simply
/// wrong, not conservative.
pub fn assign_event(place: &Place) -> Option<FieldEvent> {
    match place.projection.as_slice() {
        [] => Some(FieldEvent::Whole),
        [Projection::Field { field, .. }] => Some(FieldEvent::Field(*field)),
        _ => None,
    }
}

/// Applies one field event to a local's field-state slice.
fn apply_event(event: FieldEvent, fields: &[(DefId, Ty)], states: &mut [State], value: State) {
    match event {
        FieldEvent::Whole => states.iter_mut().for_each(|slot| *slot = value),
        FieldEvent::Field(field) => match fields.iter().position(|(known, _)| *known == field) {
            Some(index) => states[index] = value,
            // A field the record does not declare cannot happen for a
            // well-typed body. The fallback still keeps §3's direction — a
            // local that might leak rather than one that might double-free —
            // instead of panicking on what would be a bug upstream of here.
            None if value == State::Gone => states.iter_mut().for_each(|slot| *slot = value),
            None => {}
        },
    }
}

/// A local's field state after one statement, mirroring [`apply_statement`]
/// at field granularity.
fn apply_statement_fields(
    kind: &StatementKind,
    local: Local,
    fields: &[(DefId, Ty)],
    states: &mut [State],
) {
    match kind {
        StatementKind::Assign { place, rvalue } => {
            for moved in moved_places(rvalue) {
                if moved.local == local {
                    apply_event(move_event(moved), fields, states, State::Gone);
                }
            }
            if place.local == local {
                if let Some(event) = assign_event(place) {
                    apply_event(event, fields, states, State::Init);
                }
            }
        }
        StatementKind::StorageLive(l) | StatementKind::StorageDead(l) if *l == local => {
            states.iter_mut().for_each(|slot| *slot = State::Gone);
        }
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::SetDropFlag { .. }
        | StatementKind::Activate(_)
        | StatementKind::Nop => {}
    }
}

/// A local's field state after a terminator, on the edge to `successor`,
/// mirroring [`apply_terminator`] at field granularity.
fn apply_terminator_fields(
    kind: &TerminatorKind,
    successor: BlockId,
    local: Local,
    fields: &[(DefId, Ty)],
    states: &mut [State],
) {
    match kind {
        TerminatorKind::Call { args, destination, target, .. } => {
            for arg in args {
                if let Some(place) = arg.moved_place() {
                    if place.local == local {
                        apply_event(move_event(place), fields, states, State::Gone);
                    }
                }
            }
            if *target == Some(successor) && destination.local == local {
                if let Some(event) = assign_event(destination) {
                    apply_event(event, fields, states, State::Init);
                }
            }
        }
        // A `Drop` terminator's place is always a whole local —
        // `crate::lower`'s `emit_drop_if_needed` is the only place one is
        // built, and it never projects — so this is the whole local going,
        // fields and all, exactly like [`apply_terminator`]'s own arm.
        TerminatorKind::Drop { place, .. } => {
            if place.local == local {
                states.iter_mut().for_each(|slot| *slot = State::Gone);
            }
        }
        TerminatorKind::If { cond, .. } => {
            if let Some(place) = cond.moved_place() {
                if place.local == local {
                    apply_event(move_event(place), fields, states, State::Gone);
                }
            }
        }
        TerminatorKind::Switch { discr, .. } => {
            if let Some(place) = discr.moved_place() {
                if place.local == local {
                    apply_event(move_event(place), fields, states, State::Gone);
                }
            }
        }
        TerminatorKind::Goto { .. } | TerminatorKind::Return | TerminatorKind::Unreachable => {}
    }
}

/// A local's direct fields, if its type resolves to a record with at least
/// one — `None` for everything else, which is every non-record type and a
/// record declared with no fields.
///
/// **Substituted, not declared.** A generic `Pair of (T, U)` field is
/// `Ty::Param` in the declaration; `local`'s own type carries the concrete
/// arguments, so the substitution is the same one `crate::lower`'s
/// `field_ty` already applies when it builds a [`Projection::Field`] for a
/// real read — a different substitution here would make this analysis's
/// field type and a genuine field projection's type disagree about the same
/// field.
fn record_fields(
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    ty: Ty,
) -> Option<Vec<(DefId, Ty)>> {
    let ty = aliases.reveal(types, ty).unwrap_or(ty);
    let TyKind::Named { def, args } = types.kind(ty).clone() else {
        return None;
    };
    let record = decls.record(def)?;
    if record.fields.is_empty() {
        return None;
    }
    let generics = record.generics.clone();
    if generics.is_empty() || args.is_empty() {
        return Some(record.fields.clone());
    }
    let subst = Substitution::of_generics(&generics, &args);
    let fields = record.fields.clone();
    Some(
        fields
            .into_iter()
            .map(|(field, declared)| (field, subst.apply(types, declared).unwrap_or(Ty::ERROR)))
            .collect(),
    )
}

/// The per-field states this crate tracks, for the locals it tracks them for.
/// §3's amendment: a second analysis, parallel to and reusing [`analyse`]'s
/// own [`State`] and join, indexed by field instead of by local.
///
/// **Read by [`crate::drops`] alone.** [`analyse`] is unchanged, so every
/// existing reader of it — `science-regions`' `moved` chief among them — sees
/// exactly what it saw before this type existed.
pub struct FieldMoves {
    fields: HashMap<Local, Vec<(DefId, Ty)>>,
    entry: HashMap<Local, Vec<Vec<State>>>,
}

impl FieldMoves {
    /// The fields tracked for `local`, in declaration order — `None` when
    /// `local` is not decomposed and its drop stays whole, exactly as it was
    /// before this analysis existed.
    pub fn fields_of(&self, local: Local) -> Option<&[(DefId, Ty)]> {
        self.fields.get(&local).map(Vec::as_slice)
    }

    /// The per-field states just before `block`'s terminator, replayed from
    /// the block's entry exactly as [`Moves::before_terminator`] replays the
    /// whole-local table — the two cannot disagree about what a statement
    /// does, because both are one function call away from the same
    /// per-statement rule.
    pub fn before_terminator(&self, body: &Body, block: BlockId, local: Local) -> Option<Vec<State>> {
        let fields = self.fields.get(&local)?;
        let mut states = self.entry.get(&local)?[block.index()].clone();
        for statement in &body.block(block).statements {
            apply_statement_fields(&statement.kind, local, fields, &mut states);
        }
        Some(states)
    }
}

/// Runs the field analysis for every record-typed local in `body`.
///
/// **One small fixed point per decomposed local, not one combined one.** A
/// body with no record locals — most of them — builds an empty [`FieldMoves`]
/// and runs no dataflow at all; a body with one pays for one small lattice
/// over that local's own field count rather than one large lattice over every
/// local's fields at once. The bodies this crate lowers are shallow (§1), so
/// the repeated CFG walk this costs is the same trade [`analyse`]'s own
/// worklist already makes.
pub fn analyse_fields(
    body: &Body,
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
) -> FieldMoves {
    let mut fields: HashMap<Local, Vec<(DefId, Ty)>> = HashMap::new();
    for (local, decl) in body.locals() {
        if let Some(decl_fields) = record_fields(decls, types, aliases, decl.ty) {
            fields.insert(local, decl_fields);
        }
    }

    let order = reverse_postorder(body);
    let params: Vec<Local> = body.params().collect();
    let mut entry: HashMap<Local, Vec<Vec<State>>> = HashMap::new();
    for (&local, decl_fields) in &fields {
        let count = decl_fields.len();
        let mut local_entry = vec![vec![State::Unreached; count]; body.block_count()];
        let initial = if params.contains(&local) { State::Init } else { State::Gone };
        for slot in local_entry[ENTRY_BLOCK.index()].iter_mut() {
            *slot = initial;
        }

        let mut changed = true;
        while changed {
            changed = false;
            for block in &order {
                let mut states = local_entry[block.index()].clone();
                for statement in &body.block(*block).statements {
                    apply_statement_fields(&statement.kind, local, decl_fields, &mut states);
                }
                let terminator = &body.block(*block).terminator.kind;
                for successor in terminator.successors() {
                    let mut outgoing = states.clone();
                    apply_terminator_fields(
                        terminator, successor, local, decl_fields, &mut outgoing,
                    );
                    let target = &mut local_entry[successor.index()];
                    for (slot, incoming) in target.iter_mut().zip(outgoing) {
                        let joined = slot.join(incoming);
                        if *slot != joined {
                            *slot = joined;
                            changed = true;
                        }
                    }
                }
            }
        }
        entry.insert(local, local_entry);
    }

    FieldMoves { fields, entry }
}

/// Whether dropping a value of this type runs anything. §4.
///
/// **Moved to [`science_types::ownership`], and re-exported rather than
/// implemented here.** Every type this walks is a `science-types` `Ty`, and
/// [`science_types::check`]'s match-ergonomics rule needs the identical
/// question — `science_types::ownership`'s own module doc §1 is the argument,
/// and every caller in this crate still spells it `moves::needs_drop`, which
/// this re-export keeps true.
pub use science_types::ownership::needs_drop;
