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

use science_types::alias::Aliases;
use science_types::items::Declarations;
use science_types::ty::{Ty, TyKind, Types};
use science_resolve::hir::DefId;

use crate::mir::{
    reverse_postorder, Body, Local, Operand, Rvalue, StatementKind, TerminatorKind,
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

/// Every local an rvalue moves out of.
///
/// A borrow is not one of them, which is the whole reason `Ref` is an rvalue
/// rather than an operand: `borrowed x` reads `x` without consuming it, and a
/// representation that made it an operand would have had to invent a third
/// operand kind to say so.
pub fn moved_locals(rvalue: &Rvalue) -> Vec<Local> {
    let mut out = Vec::new();
    let mut take = |operand: &Operand| {
        if let Some(place) = operand.moved_place() {
            out.push(place.local);
        }
    };
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::Unary { operand, .. }
        | Rvalue::Cast { operand, .. }
        | Rvalue::IsPresent(operand)
        | Rvalue::Coerce { operand, .. }
        | Rvalue::Narrow { operand, .. } => take(operand),
        Rvalue::Binary { lhs, rhs, .. } => {
            take(lhs);
            take(rhs);
        }
        Rvalue::Range { start, end, .. } => {
            take(start);
            take(end);
        }
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                take(operand);
            }
        }
        Rvalue::Variant { payload, .. } => {
            for operand in payload {
                take(operand);
            }
        }
        // A closure's captures are operands like any other aggregate's:
        // `lower`'s §8 makes each one a reference taken in the statement
        // before, so the move seen here is the move of that reference into the
        // closure value. The *capture* is a borrow and moves nothing, which is
        // the discipline's whole point.
        Rvalue::Tuple(operands) | Rvalue::Closure { captures: operands, .. } => {
            for operand in operands {
                take(operand);
            }
        }
        Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Error => {}
    }
    out
}

/// Whether dropping a value of this type runs anything. §4.
pub fn needs_drop(
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    ty: Ty,
) -> bool {
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
        // own storage-dead point, which is §10 item 3.
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
                // interface's associated type: §4's *"true where it cannot
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
        // that knows, and it runs after this one (§12's order).
        TyKind::Param { .. }
        | TyKind::Object { .. }
        | TyKind::SelfType { .. }
        | TyKind::SelfAssoc { .. }
        | TyKind::Closure { .. } => true,
    }
}
