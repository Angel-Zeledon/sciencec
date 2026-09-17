//! §3 step 3: *"grow each region along the CFG from the points where it is
//! required to be live, respecting the constraints"*.
//!
//! # 1. The solver, in one paragraph
//!
//! Every region starts at its **lower bound** — the points where the thing it
//! belongs to is live — and grows. A constraint `'a: 'b @ p` says `'a` must
//! contain every point of `'b` that control can reach from `p`. Applying every
//! constraint until nothing changes is the least solution, and it exists
//! because the sets only grow and the point count is finite.
//!
//! There is no upper bound anywhere in this file, and that is the shape of NLL
//! rather than an omission. A region that grew too far is not truncated; it is
//! *reported*, by [`crate::check`], against the access it reached. §6 of
//! [`crate`] is what that costs Decision 3.
//!
//! # 2. Propagation is location-sensitive, and that is the whole algorithm
//!
//! ```text
//! for 'a: 'b @ p:
//!     seen  = {}
//!     stack = successors(p)
//!     while q = stack.pop():
//!         if q ∈ seen:      continue
//!         seen ← seen ∪ {q}
//!         if q ∉ 'b:        continue      # 'b does not reach here
//!         'a ← 'a ∪ {q}
//!         stack.extend(successors(q))
//! ```
//!
//! **The visited set is `seen` and not `'a`, and the difference is a bug this
//! file had.** Skipping a point because `'a` already contains it is correct
//! only on the pass that put it there. On the *next* pass — the one that runs
//! because `'b` grew — the walk stops at the first point it added last time and
//! never reaches the new ones. The symptom is an under-approximated region,
//! which is a *missed* error: the acceptance case passed either way, and the
//! `intern` shape of §2 reported the wrong diagnostic. A separate visited set
//! costs one bitset per constraint application and is the only version that is
//! a fixpoint.
//!
//! **The first line of the loop is the difference between NLL and a
//! flow-insensitive union.** `'a ⊇ 'b` outright would make a reference
//! assigned inside one arm of an `if` live in the other arm too, and the
//! canonical NLL example — a borrow that ends at its last use rather than at
//! the end of the block — would not work.
//!
//! The consequence is that `'b` must be grown before `'a` can be, which is why
//! this is a fixpoint over the whole constraint list rather than one pass in
//! topological order: the constraint graph has cycles whenever the CFG does.
//!
//! **The stack starts at `p`'s *successors* and not at `p`.** A statement's
//! effect is visible after it, not at it: `x be borrowed v` writes `x`, so `x`
//! is not live on entry to that very point ([`crate::liveness`]'s §1 is the
//! definition), so a propagation that started at `p` would find `p ∉ 'x` and
//! stop before it began — and every loan's region would be empty. This is one
//! line and it is the difference between a solver and a solver-shaped object;
//! `tests/solver.rs` holds a loan's region to more than the point it was taken
//! at, so that the line cannot be lost in silence.
//!
//! # 3. Where the lower bounds come from
//!
//! - **A local's region** is seeded with every point at which that local is
//!   live. §3 step 1's *"regions of live variables are live"*, and
//!   [`crate::liveness`] is the dataflow.
//! - **A parameter's region** is seeded with *every* point.
//!   [`crate::generate`]'s §4 argues that this is not an approximation of the
//!   answer Decision 5 wants.
//! - **A loan's region** is seeded with nothing at all. It is grown entirely by
//!   the `'loan: 'destination` constraint of [`crate::generate`]'s §2, which is
//!   what makes a borrow end at its last use: the loan is live exactly where
//!   something that holds it is.
//!
//! # 4. The cost, measured rather than claimed
//!
//! §3 says *"`O(points × regions)` in the worst case with a bitset
//! representation, and linear in practice because the constraint graph is
//! sparse"*. What this implementation actually does is `O(iterations ×
//! constraints × points)`, and [`Solution::iterations`] reports the first
//! factor so that *"linear in practice"* is a number a test can assert rather
//! than a claim. On `examples/21_compiler_shapes.science` it is small, and
//! `tests/solver.rs` holds it to a bound.

use science_mir::mir::Body;

use crate::constraints::Constraints;
use crate::liveness::Liveness;
use crate::points::{PointIndex, RegionSet};
use crate::regions::{RegionKind, RegionTable, RegionVar};

/// The least solution of one body's constraints.
#[derive(Debug, Clone)]
pub struct Solution {
    regions: Vec<RegionSet>,
    iterations: usize,
}

impl Solution {
    /// Seeds, then propagates to a fixed point. §1.
    pub fn of(
        body: &Body,
        index: &PointIndex,
        table: &RegionTable,
        liveness: &Liveness,
        constraints: &Constraints,
    ) -> Solution {
        let mut regions = vec![RegionSet::empty(index.len()); table.len()];

        // §3.
        for var in table.vars() {
            if let RegionKind::Local { local, .. } = table.kind(var) {
                for point in index.all() {
                    if liveness.is_live(point, *local) {
                        regions[var.index()].insert(point);
                    }
                }
            }
        }
        for var in table.universal() {
            regions[var.index()] = RegionSet::full(index.len());
        }

        // §2, to a fixed point.
        let successors = successor_table(body, index);
        let mut iterations = 0;
        loop {
            iterations += 1;
            let mut changed = false;
            for constraint in constraints.all() {
                changed |= propagate(
                    &mut regions,
                    &successors,
                    constraint.sup,
                    constraint.sub,
                    &successors[index.index(constraint.point)].clone(),
                );
            }
            if !changed {
                break;
            }
        }

        Solution { regions, iterations }
    }

    pub fn region(&self, var: RegionVar) -> &RegionSet {
        &self.regions[var.index()]
    }

    /// How many sweeps the fixpoint took. §4.
    pub fn iterations(&self) -> usize {
        self.iterations
    }

    pub fn len(&self) -> usize {
        self.regions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// One constraint, applied once. §2.
fn propagate(
    regions: &mut [RegionSet],
    successors: &[Vec<usize>],
    sup: RegionVar,
    sub: RegionVar,
    from: &[usize],
) -> bool {
    if sup == sub {
        return false;
    }
    let mut changed = false;
    // An explicit stack, for `science-mir`'s reason and Decision 11's: a deep
    // CFG is the shape that overflows, and this engine is to be ported.
    let mut seen = RegionSet::empty(successors.len());
    let mut stack: Vec<usize> = from.to_vec();
    while let Some(point) = stack.pop() {
        if !seen.insert(point) {
            continue;
        }
        if !regions[sub.index()].contains(point) {
            continue;
        }
        changed |= regions[sup.index()].insert(point);
        stack.extend(successors[point].iter().copied());
    }
    changed
}

/// The points control can reach from each point, in the dense numbering.
///
/// Computed once. [`science_mir::mir::Body::successors`] is per *block*, and a
/// propagation that called it per point would rebuild the terminator's
/// successor vector once per point per constraint per iteration.
fn successor_table(body: &Body, index: &PointIndex) -> Vec<Vec<usize>> {
    let mut table = vec![Vec::new(); index.len()];
    for (block, basic) in body.blocks() {
        let first = index.entry_of(block);
        for at in 0..basic.statements.len() {
            table[first + at] = vec![first + at + 1];
        }
        let terminator = first + basic.statements.len();
        table[terminator] = body
            .successors(block)
            .into_iter()
            .map(|successor| index.entry_of(successor))
            .collect();
    }
    table
}
