//! What happens to a place at a point, and the correction §3 step 4 needs.
//!
//! # 1. The amendment this module exists for
//!
//! `region-inference.md` §3 step 4 reads, in full:
//!
//! > **Conflict check.** For every exclusive borrow, no other *borrow* of an
//! > overlapping place is live at any point in its region; for every shared
//! > borrow, no exclusive one is.
//!
//! **That is borrow-against-borrow, and it is not enough.** A program can read
//! and write a place without borrowing it — an assignment is a write, an
//! operand is a read, an elaborated [`TerminatorKind::Drop`] is an exclusive
//! borrow the author never wrote — and none of those is a *borrow* in the
//! table §3 step 4 quantifies over. Implemented literally, the step accepts:
//!
//! ```text
//! let r be mutable borrowed config    # an exclusive borrow, live to the end
//! if config.port?:                    # a read of a place `r` exclusively holds
//!     write_through(r)                # writes `config.port`, creates no borrow
//!     print(config.port.length())     # reads a narrowing that is now false
//! ```
//!
//! There is exactly one borrow in that program, so *"no other borrow overlaps"*
//! holds, so §3 step 4 as written passes it. And `type-checking-and-mir.md`
//! Decision 8 — *"narrowing relies on rule 4"* — is then false, because the
//! rule narrowing relies on did not forbid the read. [`crate`]'s §6 is that
//! finding in full; this module is the fix, and the fix is that the check
//! ranges over **accesses**, of which a borrow is one kind.
//!
//! # 2. What an access is
//!
//! One walk over a statement or a terminator, producing every place it touches
//! and how. The walk is here rather than inside [`crate::liveness`] and
//! [`crate::check`] because those two want the same answer for opposite
//! purposes — liveness wants the locals, the conflict check wants the places —
//! and two walks would be two chances to forget the same arm.
//!
//! **What it costs** is a `Vec<Access>` per point. [`Accesses`] computes the
//! table once per body; the diagnostic path calls [`accesses_at`] directly,
//! because a message is rendered once and a cache it had to be handed would be
//! a parameter on every rendering function.
//!
//! # 3. The three arms whose classification is a decision, not a reading
//!
//! - **A write to a *projected* place is also a use of its base.** `x.f be 1`
//!   needs `x` to exist, so `x` is used and not defined. Only a write to a
//!   whole local defines it, and [`crate::liveness`] is where that split is
//!   made. Getting it backwards makes liveness kill a variable that is still
//!   needed, which shortens a region, which misses a conflict.
//! - **[`TerminatorKind::Drop`] is [`AccessKind::Drop`] and not
//!   [`AccessKind::Write`]**, although both are exclusive. The distinction
//!   buys one thing and it is the thing §7.1 is about: a message that says
//!   *"...and dropped here"* about a line the author did not write has to say
//!   so, and *"modified here"* about the same line is a lie. `science-mir`'s
//!   Decision 26 flag rides along for the same reason — a conditional drop
//!   happens on *some* paths and the message says which.
//! - **[`StatementKind::StorageDead`] is an access of its own kind**, because
//!   rule 5 is checked against it and against nothing else (§10 item 3), and
//!   because what it produces is `SC0333` rather than `SC0330`.
//!
//! # 4. The two-phase split, and the program it exists for
//!
//! `science-mir`'s §5 hands over the obligation: *"between a `TwoPhase`
//! borrow's `reserved` and its `activation`, the borrowed place may be read and
//! may not be written or exclusively reborrowed; from the activation on, it is
//! an ordinary exclusive borrow"*. Two halves, and they are two different
//! accesses here:
//!
//! - the reservation is [`AccessKind::Borrow`] with
//!   [`BorrowKind::TwoPhase`], and [`AccessKind::is_exclusive`] answers
//!   **false** for it;
//! - the activation is a *separate* access, synthesised at
//!   [`StatementKind::Activate`], and it is an ordinary exclusive borrow.
//!
//! **The split is what makes `let s be borrowed v` followed by
//! `v.push(s.len())` compile.** With the reservation counted as exclusive, `s`
//! — which is live at the reservation and dead by the activation — would
//! conflict, and the program would be refused for a borrow that no longer
//! exists when the mutation happens. That is the case two-phase borrows were
//! introduced for, and it is the one a checker gets wrong by treating the
//! reservation as the borrow.

use science_diagnostics::Span;
use science_mir::mir::{
    Body, BorrowId, BorrowKind, Callee, Operand, Place, Point, Rvalue, StatementKind, Terminator,
    TerminatorKind, RETURN_PLACE,
};

use crate::points::PointIndex;

/// How a point touches a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    /// The place is read: an operand, a condition, a call argument.
    Read,
    /// The place is written to.
    Write,
    /// The place is moved out of. `SC0334`'s condition, when it overlaps a
    /// borrow that is still live.
    Move,
    /// A borrow of the place is taken here.
    Borrow(BorrowKind, BorrowId),
    /// An elaborated drop: an exclusive borrow at a point the author did not
    /// write. `conditional` is `science-mir`'s Decision 26 flag.
    Drop { conditional: bool },
    /// §10 item 3's comparison point, and the only thing rule 5 is checked
    /// against.
    StorageDead,
    /// The local's storage begins. **Not a conflict of any kind** — nothing can
    /// name storage that has not started — and it is an access only because
    /// [`crate::liveness`] needs it to kill: a value from the previous turn of
    /// a loop is not live across the `StorageLive` that restarts the storage.
    StorageLive,
}

impl AccessKind {
    /// Whether the access needs the place exclusively.
    pub fn is_exclusive(self) -> bool {
        match self {
            AccessKind::Write | AccessKind::Move | AccessKind::Drop { .. } => true,
            // **A two-phase borrow's reservation is not exclusive**, and its
            // activation is a separate access of its own. §4.
            AccessKind::Borrow(kind, _) => kind == BorrowKind::Exclusive,
            AccessKind::Read | AccessKind::StorageDead | AccessKind::StorageLive => false,
        }
    }

    /// What §7.1's middle span calls this.
    pub fn described(self) -> &'static str {
        match self {
            AccessKind::Read => "read here",
            AccessKind::Write => "modified here, which needs it exclusively",
            AccessKind::Move => "moved here",
            AccessKind::Borrow(BorrowKind::Shared, _) => "borrowed here, shared",
            AccessKind::Borrow(_, _) => "borrowed here, exclusively",
            AccessKind::Drop { conditional: false } => "dropped here",
            AccessKind::Drop { conditional: true } => "dropped here, on some paths",
            AccessKind::StorageDead => "goes out of scope here",
            AccessKind::StorageLive => "comes into scope here",
        }
    }
}

/// One place, touched one way, at one point.
#[derive(Debug, Clone)]
pub struct Access {
    pub place: Place,
    pub kind: AccessKind,
    /// Decision 2's other half. Carried on the access rather than recovered
    /// from the point, because §7.1's middle span *is* this span and a message
    /// that had to re-find it would be a message that could fail to.
    pub span: Span,
}

/// Every access at every point, computed once per body.
#[derive(Debug, Clone)]
pub struct Accesses {
    at: Vec<Vec<Access>>,
}

impl Accesses {
    pub fn of(body: &Body, index: &PointIndex) -> Accesses {
        let mut at = vec![Vec::new(); index.len()];
        for point in body.points() {
            at[index.index(point)] = accesses_at(body, point);
        }
        Accesses { at }
    }

    pub fn at(&self, point: usize) -> &[Access] {
        &self.at[point]
    }
}

/// The accesses one point performs.
pub fn accesses_at(body: &Body, point: Point) -> Vec<Access> {
    let mut out = Vec::new();
    match body.statement_at(point) {
        Some(statement) => {
            let span = statement.span;
            match &statement.kind {
                StatementKind::Assign { place, rvalue } => {
                    from_rvalue(rvalue, span, &mut out);
                    out.push(Access { place: place.clone(), kind: AccessKind::Write, span });
                }
                StatementKind::StorageDead(local) => out.push(Access {
                    place: Place::local(*local),
                    kind: AccessKind::StorageDead,
                    span,
                }),
                StatementKind::StorageLive(local) => out.push(Access {
                    place: Place::local(*local),
                    kind: AccessKind::StorageLive,
                    span,
                }),
                // §4. The activation is where a two-phase borrow becomes an
                // ordinary exclusive one, so it is an exclusive access to the
                // place it was reserved on — and it is the *only* access there
                // is, because the reservation is shared.
                StatementKind::Activate(borrow) => {
                    let data = body.borrow_data(*borrow);
                    out.push(Access {
                        place: data.place.clone(),
                        kind: AccessKind::Borrow(BorrowKind::Exclusive, *borrow),
                        span,
                    })
                }
                // A drop flag is compiler storage no borrow can reach.
                StatementKind::SetDropFlag { .. } | StatementKind::Nop => {}
            }
        }
        None => from_terminator(&body.block(point.block).terminator, &mut out),
    }
    out
}

fn from_terminator(terminator: &Terminator, out: &mut Vec<Access>) {
    let span = terminator.span;
    match &terminator.kind {
        TerminatorKind::Goto { .. } | TerminatorKind::Unreachable => {}
        TerminatorKind::If { cond, .. } => from_operand(cond, span, out),
        TerminatorKind::Switch { discr, .. } => from_operand(discr, span, out),
        TerminatorKind::Call { callee, args, destination, .. } => {
            if let Callee::Indirect(operand) = callee {
                from_operand(operand, span, out);
            }
            for arg in args {
                from_operand(arg, span, out);
            }
            out.push(Access { place: destination.clone(), kind: AccessKind::Write, span });
        }
        TerminatorKind::Drop { place, flag, .. } => out.push(Access {
            place: place.clone(),
            kind: AccessKind::Drop { conditional: flag.is_some() },
            span,
        }),
        // The caller reads the return place, so it is live at the `Return` and
        // every region in its type is live there too. Without this the return
        // region of every borrow-returning function is empty and Decision 5's
        // whole analysis has nothing to conclude.
        TerminatorKind::Return => {
            out.push(Access { place: Place::local(RETURN_PLACE), kind: AccessKind::Read, span })
        }
    }
}

fn from_operand(operand: &Operand, span: Span, out: &mut Vec<Access>) {
    match operand {
        Operand::Copy(place) => {
            out.push(Access { place: place.clone(), kind: AccessKind::Read, span })
        }
        Operand::Move(place) => {
            out.push(Access { place: place.clone(), kind: AccessKind::Move, span })
        }
        Operand::Const(_) => {}
    }
}

fn from_rvalue(rvalue: &Rvalue, span: Span, out: &mut Vec<Access>) {
    match rvalue {
        Rvalue::Ref { kind, place, borrow } => out.push(Access {
            place: place.clone(),
            kind: AccessKind::Borrow(*kind, *borrow),
            span,
        }),
        Rvalue::Use(operand)
        | Rvalue::Unary { operand, .. }
        | Rvalue::Cast { operand, .. }
        | Rvalue::IsPresent(operand)
        | Rvalue::Coerce { operand, .. }
        | Rvalue::Narrow { operand, .. } => from_operand(operand, span, out),
        Rvalue::Binary { lhs, rhs, .. } => {
            from_operand(lhs, span, out);
            from_operand(rhs, span, out);
        }
        Rvalue::Discriminant(place) => {
            out.push(Access { place: place.clone(), kind: AccessKind::Read, span })
        }
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                from_operand(operand, span, out);
            }
        }
        Rvalue::Variant { payload, .. } => {
            for operand in payload {
                from_operand(operand, span, out);
            }
        }
        // A closure's captures are operands here now: `science-mir`'s `lower`
        // §8 makes each one a reference taken in the statement before, so the
        // access this records is the move of that reference into the closure
        // value, and the *borrow* was recorded at the `Ref` that took it.
        Rvalue::Tuple(operands) | Rvalue::Closure { captures: operands, .. } => {
            for operand in operands {
                from_operand(operand, span, out);
            }
        }
        Rvalue::Range { start, end, .. } => {
            from_operand(start, span, out);
            from_operand(end, span, out);
        }
        Rvalue::Error => {}
    }
}
