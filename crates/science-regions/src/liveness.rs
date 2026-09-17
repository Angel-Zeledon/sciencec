//! §3 step 1: *"a standard backward dataflow over the CFG giving, per point,
//! the set of live variables. Regions of live variables are live."*
//!
//! # 1. Live on entry, and why that is the answer regions want
//!
//! **Decision. The table holds, for each point, the locals live *on entry* to
//! it**, where live means *"some path from here reaches a use before a
//! redefinition"*.
//!
//! The alternative — live on exit — differs at exactly the points that matter.
//! A region is grown to every point where a reference is live (§3 step 3), and
//! a reference used at point `p` must be valid *at* `p`, not after it. With
//! live-on-exit the region would exclude its own last use, which is the one
//! point §7.1 calls *"the load-bearing one"*.
//!
//! # 2. The transfer function, and the one asymmetry in it
//!
//! `live_in(p) = uses(p) ∪ (live_out(p) − defs(p))`, where `live_out(p)` is
//! the union over the points control can reach from `p`.
//!
//! The asymmetry is in `defs`: **only a write to a whole local defines it.**
//! A write to `x.f` needs `x` to already exist, so it is a use. [`crate::access`]
//! §3 states the same rule from the other side, and getting it backwards
//! shortens regions, which loses conflicts rather than inventing them — the
//! unsound direction.
//!
//! [`StatementKind::StorageLive`](science_mir::mir::StatementKind::StorageLive)
//! and `StorageDead` both define: nothing before a local's storage begins can
//! be its value, and nothing after its storage ends can be either.
//!
//! **A move is a use and not a def.** It reads the value, which is what makes
//! the local live at that point — and whether the local is dead *after* it is
//! `science_mir::moves`'s question, asked over a different lattice for a
//! different consumer. Calling it a def here was the one mistake this file
//! made, and its symptom was that every two-phase borrow's region was empty,
//! because the only thing that holds such a borrow is the `Move` of the
//! temporary at the call.
//!
//! # 3. The iteration order, and the cost of not having a work queue
//!
//! **Decision. Blocks are visited in the reverse of
//! [`reverse_postorder`](science_mir::mir::reverse_postorder), repeatedly,
//! until a sweep changes nothing.**
//!
//! That order visits every block after all of its successors except across a
//! back edge, so an acyclic body converges in one sweep and a loop costs one
//! extra sweep per nesting level. A worklist would visit fewer blocks on a
//! large CFG; it would also be a mutable queue with back-references into the
//! block table, which is §11's *"cyclic, aliasing, mutable"* shape and the one
//! structure Decision 11 asks this engine to avoid. A sweep over a `Vec` is
//! expressible in Science with no escape hatch, and that is worth more here
//! than the constant factor.

use science_mir::mir::{reverse_postorder, Body, Local, Place};

use crate::access::{Access, AccessKind, Accesses};
use crate::points::{Bits, PointIndex};

/// Which locals are live on entry to each point.
#[derive(Debug, Clone)]
pub struct Liveness {
    /// Indexed by point, a set indexed by local.
    live: Vec<Bits>,
    local_count: usize,
}

impl Liveness {
    /// The backward dataflow of §3 step 1.
    pub fn of(body: &Body, index: &PointIndex, accesses: &Accesses) -> Liveness {
        let local_count = body.local_count();
        let mut live = vec![Bits::empty(local_count); index.len()];

        // The reverse of reverse-postorder: §3.
        let mut order = reverse_postorder(body);
        order.reverse();

        loop {
            let mut changed = false;
            for &block in &order {
                let statements = body.block(block).statements.len();
                let first = index.entry_of(block);
                let terminator = first + statements;

                // The terminator's live-out is the union of its successors'
                // live-in.
                let mut out = Bits::empty(local_count);
                for successor in body.successors(block) {
                    out.union_with(&live[index.entry_of(successor)]);
                }
                changed |= transfer(accesses.at(terminator), &mut out, &mut live[terminator]);

                // Then backwards through the statements.
                for at in (0..statements).rev() {
                    let point = first + at;
                    let mut out = live[point + 1].clone();
                    changed |= transfer(accesses.at(point), &mut out, &mut live[point]);
                }
            }
            if !changed {
                break;
            }
        }

        Liveness { live, local_count }
    }

    /// Whether a local is live on entry to a point.
    pub fn is_live(&self, point: usize, local: Local) -> bool {
        self.live[point].contains(local.index())
    }

    /// The locals live on entry to a point.
    pub fn live_at(&self, point: usize) -> impl Iterator<Item = Local> + '_ {
        self.live[point].iter().map(Local::from_index)
    }

    pub fn local_count(&self) -> usize {
        self.local_count
    }
}

/// `live_in = uses ∪ (live_out − defs)`, written into `into`, answering whether
/// `into` changed.
fn transfer(accesses: &[Access], out: &mut Bits, into: &mut Bits) -> bool {
    for access in accesses {
        if defines(access) {
            // A def kills. It is applied before the uses so that a statement
            // which both writes and reads one local — `x be x + 1` in
            // three-address form does not, but a `Call` whose destination is
            // also an argument can — keeps it live.
            out.remove(access.place.local.index());
        }
    }
    for access in accesses {
        if uses(access) {
            out.insert(access.place.local.index());
        }
    }
    let changed = *into != *out;
    if changed {
        into.clone_from(out);
    }
    changed
}

/// Whether an access ends the local's current value. §2.
fn defines(access: &Access) -> bool {
    match access.kind {
        AccessKind::Write => is_whole_local(&access.place),
        AccessKind::StorageDead | AccessKind::StorageLive => true,
        // **A move does not kill.** It reads the value, so it is a use; and
        // whether the local is dead *after* it is `science_mir::moves`'s
        // question, not this one. Treating it as a def here shortens the
        // region of anything the moved value holds, which is the direction
        // §2 says loses conflicts.
        AccessKind::Move
        | AccessKind::Read
        | AccessKind::Borrow(_, _)
        | AccessKind::Drop { .. } => false,
    }
}

/// Whether an access needs the local's current value. §2.
fn uses(access: &Access) -> bool {
    match access.kind {
        AccessKind::Read | AccessKind::Move | AccessKind::Borrow(_, _) | AccessKind::Drop { .. } => {
            true
        }
        // A write to a projection needs the base; a write to the whole local
        // does not.
        AccessKind::Write => !is_whole_local(&access.place),
        AccessKind::StorageDead | AccessKind::StorageLive => false,
    }
}

fn is_whole_local(place: &Place) -> bool {
    place.is_local()
}
