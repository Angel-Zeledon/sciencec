//! One body, analysed: the five steps of §3 held together, and the two queries
//! everything above asks of the result.
//!
//! # 1. What an analysis is
//!
//! [`BodyAnalysis`] is the record of running §3 over one [`Body`]: the point
//! numbering, the region variables, liveness, the constraints, the solution,
//! and the [`Summary`] Decision 7 would serialise. It holds no diagnostics —
//! [`crate::check`] reads it and produces those — because the same analysis
//! serves the borrow check, the `borrows_live_at` query §6.4 reserves for F5,
//! and the summary a *caller's* analysis reads. Three consumers, one result.
//!
//! # 2. Reading the signature off the solution — Decision 5
//!
//! [`summarise`] is *"analyse the body, compute what relations actually hold
//! between the parameters' regions and the return's, and **that** is the
//! signature"*, and it is nine lines because the work was done by the time it
//! runs. It walks the constraint graph from each return position **towards its
//! suppliers** — `sub → sup`, the direction that asks *"what must outlive
//! this?"* — and keeps the parameters it reaches.
//!
//! **Nothing about the solved point sets is read.** That is worth stating,
//! because it is what makes the summary a fact about the *function* rather than
//! about this body's block numbering: the points are §10 item 6's stable-only-
//! locally numbering, and a summary that mentioned them could not cross the
//! query boundary Decision 8 draws.
//!
//! # 3. The two-phase window
//!
//! `science-mir` marks a borrow reserved at one point and activated at another
//! *"and decides nothing about what may happen in between"*. The obligation it
//! hands over is: between the two, the place may be read and may not be written
//! or exclusively reborrowed.
//!
//! **Decision. The window is the set of points reachable from the reservation
//! that can also reach the activation, and the activation itself is outside
//! it.** Reachability in both directions rather than "every point between them
//! in the numbering", because a two-phase borrow's arguments can contain an
//! `if` — `v.push(if c: v.len() else: 0)` — and the numbering is not a path.
//!
//! **What it costs** is two reachability sweeps per two-phase borrow. What it
//! buys is that `v.push(v.len())` compiles, which §14 calls a silent
//! prerequisite and `science-mir`'s §7 item 3 says has a third failure mode:
//! going inert in silence.

use science_mir::mir::{Body, BorrowId, BorrowKind, Callee, Local, Point, RETURN_PLACE};
use science_resolve::hir::DefId;

use crate::access::Accesses;
use crate::constraints::Constraints;
use crate::generate::constraints_of;
use crate::liveness::Liveness;
use crate::points::{Bits, PointIndex};
use crate::regions::{Context, Path, RegionKind, RegionTable, RegionVar};
use crate::solve::Solution;
use crate::summary::{ParamRegion, Summaries, Summary};

/// §3, run over one body.
#[derive(Debug, Clone)]
pub struct BodyAnalysis {
    pub def: DefId,
    pub index: PointIndex,
    pub table: RegionTable,
    pub accesses: Accesses,
    pub liveness: Liveness,
    pub constraints: Constraints,
    pub solution: Solution,
    /// Decision 5's answer for this function. §2.
    pub summary: Summary,
    /// Per [`BorrowId`], the reservation window of §3, or `None` for a borrow
    /// that is not two-phase.
    pub windows: Vec<Option<Bits>>,
    /// Whether the body calls anything this compilation cannot name.
    /// [`crate::check`]'s §6.
    pub calls_a_hole: bool,
}

impl BodyAnalysis {
    /// Runs steps 1 to 3 of §3 and reads the signature off the result.
    ///
    /// `summaries` must already hold every callee outside this body's own
    /// strongly connected component, which
    /// [`science_mir::CallGraph::components`] guarantees by handing the
    /// components over leaves first.
    pub fn of(context: &mut Context<'_>, body: &Body, summaries: &Summaries) -> BodyAnalysis {
        let index = PointIndex::of(body);
        let table = RegionTable::of(context, body);
        let accesses = Accesses::of(body, &index);
        let liveness = Liveness::of(body, &index, &accesses);
        let constraints = constraints_of(body, &table, summaries);
        let solution = Solution::of(body, &index, &table, &liveness, &constraints);
        let summary = summarise(body, &table, &constraints);
        let windows = windows_of(body, &index);
        let calls_a_hole = body.blocks().any(|(_, block)| {
            matches!(
                block.terminator.kind,
                science_mir::mir::TerminatorKind::Call { callee: Callee::Unresolved(_), .. }
            )
        });
        BodyAnalysis {
            def: body.def(),
            index,
            table,
            accesses,
            liveness,
            constraints,
            solution,
            summary,
            windows,
            calls_a_hole,
        }
    }

    /// §6.2's public query: which borrows are live at a point.
    ///
    /// `SC0341` — *"a borrow is live across a suspension point"* — is this
    /// query and a comparison, and §12 reserves the code until there is an F5
    /// to ask it.
    pub fn borrows_live_at(&self, point: Point) -> Vec<BorrowId> {
        let at = self.index.index(point);
        self.table
            .vars()
            .filter_map(|var| match self.table.kind(var) {
                RegionKind::Loan(borrow) if self.solution.region(var).contains(at) => Some(*borrow),
                _ => None,
            })
            .collect()
    }

    /// Whether a two-phase borrow is still in its reservation window at a
    /// point — where it behaves as a shared borrow. §3.
    pub fn is_reserved_at(&self, borrow: BorrowId, point: usize) -> bool {
        match &self.windows[borrow.index()] {
            Some(window) => window.contains(point),
            None => false,
        }
    }

    /// What a borrow demands at a point: `true` for exclusive.
    pub fn is_exclusive_at(&self, body: &Body, borrow: BorrowId, point: usize) -> bool {
        match body.borrow_data(borrow).kind {
            BorrowKind::Shared => false,
            BorrowKind::Exclusive => true,
            BorrowKind::TwoPhase => !self.is_reserved_at(borrow, point),
        }
    }

    /// The region of a loan, as a set of dense point indices.
    pub fn loan_region(&self, borrow: BorrowId) -> &Bits {
        self.solution.region(self.table.loan(borrow))
    }
}

/// Decision 5, read off the constraint graph. §2.
pub fn summarise(body: &Body, table: &RegionTable, constraints: &Constraints) -> Summary {
    let params: Vec<Local> = body.params().collect();
    let param_index = |local: Local| params.iter().position(|it| *it == local);

    let mut returns = Vec::new();
    for (position, var) in table.local(RETURN_PLACE) {
        let from = suppliers(table, constraints, *var, &param_index);
        returns.push((position.clone(), from));
    }

    // §3 of `summary`: what the callee may store back through a parameter.
    let mut escapes = Vec::new();
    for (at, param) in params.iter().enumerate() {
        let positions = table.local(*param);
        let mutable: Vec<&Path> = positions
            .iter()
            .filter(|(position, _)| position.mutable)
            .map(|(position, _)| &position.path)
            .collect();
        for (position, var) in positions {
            let writable = mutable.iter().any(|prefix| {
                position.path.len() > prefix.len() && position.path.starts_with(prefix)
            });
            if !writable {
                continue;
            }
            let target = ParamRegion { param: at, path: position.path.clone() };
            let mut from = suppliers(table, constraints, *var, &param_index);
            from.retain(|source| *source != target);
            if !from.is_empty() {
                escapes.push((target, from));
            }
        }
    }

    Summary { def: body.def(), params: params.len(), returns, escapes, opaque: false }
}

/// The parameter regions that must outlive one region.
///
/// A breadth-first walk of the constraint graph from `start` in the `sub → sup`
/// direction, which reads *"what is required to contain me"*. Sorted and
/// deduplicated so that two runs of the compiler produce the same summary —
/// §10 item 6 applied to the thing Decision 7 would serialise.
fn suppliers(
    table: &RegionTable,
    constraints: &Constraints,
    start: RegionVar,
    param_index: &impl Fn(Local) -> Option<usize>,
) -> Vec<ParamRegion> {
    let mut seen = vec![false; table.len()];
    let mut queue = vec![start];
    seen[start.index()] = true;
    let mut out = Vec::new();
    while let Some(var) = queue.pop() {
        for constraint in constraints.all() {
            if constraint.sub != var || seen[constraint.sup.index()] {
                continue;
            }
            seen[constraint.sup.index()] = true;
            if let RegionKind::Local { local, position } = table.kind(constraint.sup) {
                if let Some(at) = param_index(*local) {
                    out.push(ParamRegion { param: at, path: position.path.clone() });
                }
            }
            queue.push(constraint.sup);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// §3: the reservation window of every two-phase borrow.
fn windows_of(body: &Body, index: &PointIndex) -> Vec<Option<Bits>> {
    let successors = point_successors(body, index);
    let predecessors = invert(&successors);
    body.borrows()
        .iter()
        .map(|data| {
            let activation = data.activation?;
            let forward = sweep(&successors, index.index(data.reserved), index.len());
            let backward = sweep(&predecessors, index.index(activation), index.len());
            let mut window = forward;
            window.intersect_with(&backward);
            // The activation is where the borrow becomes exclusive, so it is
            // outside the window it ends.
            window.remove(index.index(activation));
            Some(window)
        })
        .collect()
}

fn point_successors(body: &Body, index: &PointIndex) -> Vec<Vec<usize>> {
    let mut table = vec![Vec::new(); index.len()];
    for (block, basic) in body.blocks() {
        let first = index.entry_of(block);
        for at in 0..basic.statements.len() {
            table[first + at] = vec![first + at + 1];
        }
        table[first + basic.statements.len()] = body
            .successors(block)
            .into_iter()
            .map(|successor| index.entry_of(successor))
            .collect();
    }
    table
}

fn invert(graph: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new(); graph.len()];
    for (from, edges) in graph.iter().enumerate() {
        for to in edges {
            out[*to].push(from);
        }
    }
    out
}

fn sweep(graph: &[Vec<usize>], start: usize, len: usize) -> Bits {
    let mut seen = Bits::empty(len);
    let mut stack = vec![start];
    seen.insert(start);
    while let Some(point) = stack.pop() {
        for next in &graph[point] {
            if seen.insert(*next) {
                stack.push(*next);
            }
        }
    }
    seen
}
