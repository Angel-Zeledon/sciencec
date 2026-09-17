//! The move analysis, and the one question it exists to answer.
//!
//! # 1. What this is for, and what it is not
//!
//! `type-checking-and-mir.md` Decision 26 asks for one thing: *"flags are
//! generated **only** for locals the move analysis proves conditionally
//! moved, never for every local, which is the difference between a rare cost
//! and a tax on every function"*. This module is that proof, and nothing else
//! consumes it.
//!
//! **It is not a use-after-move check.** `SC0301` and `SC0302` are the core
//! spec's and belong to whatever runs after region inference; this pass reports
//! nothing, has no `Diagnostics` and cannot be made to emit one. Sharing the
//! dataflow later is the right move; sharing it *now*, before there is a
//! consumer, would fix the lattice around a second question before that
//! question has been asked.
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
    /// has far more points than blocks, and the only consumer — drop
    /// elaboration — asks about terminators, which is where every
    /// [`TerminatorKind::Drop`] is.
    pub fn before_terminator(&self, body: &Body, block: crate::mir::BlockId) -> Vec<State> {
        let mut states = self.entry[block.index()].clone();
        for statement in &body.block(block).statements {
            apply_statement(&statement.kind, &mut states);
        }
        states
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
