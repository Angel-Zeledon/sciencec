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

use science_diagnostics::Span;
use science_types::ty::Ty;

use crate::mir::{
    BlockId, Body, Local, LocalDecl, LocalKind, Operand, Statement, StatementKind, TerminatorKind,
};
use crate::moves::{self, State};

/// Runs the elaboration in place. `flag_ty` is `Bool`, or [`Ty::ERROR`] in a
/// compilation with no prelude to find it in.
pub fn elaborate(body: &mut Body, flag_ty: Ty) {
    let analysis = moves::analyse(body);

    // Pass one: decide, for every `Drop`, which of §1's three rows it is in.
    // Decisions are collected before anything is edited, because editing
    // changes the statement numbering the analysis was computed against.
    let mut delete: Vec<BlockId> = Vec::new();
    let mut guarded: Vec<(BlockId, Local)> = Vec::new();
    for (id, block) in body.blocks() {
        let TerminatorKind::Drop { place, .. } = &block.terminator.kind else {
            continue;
        };
        let local = place.local;
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
