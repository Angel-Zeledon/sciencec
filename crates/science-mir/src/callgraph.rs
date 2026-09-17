//! The call graph, and the strongly connected components Decision 8 caches on.
//!
//! # 1. Why this is here and not in the region engine
//!
//! `science-db`'s `pending.rs` already says so: *"the call graph, built from
//! every function's MIR"*, and its `call_graph` query is stubbed with
//! `unimplemented!("science-mir: the call graph over every function's MIR")`.
//! The graph is a property of the IR, not of the analysis that walks it, and
//! putting it in the analysis would make a second analysis build a second one.
//!
//! # 2. Strongly connected components, and what they are for
//!
//! `region-inference.md` **Decision 8**: *"The query boundary for incremental
//! recompilation is the strongly connected component of the call graph, not the
//! function"*, reusing the SCCs §6.2 already needs for mutual recursion.
//!
//! **Decision. Tarjan's algorithm, iterative, and the components come out in
//! reverse topological order.** That order is not a convenience — it is exactly
//! the order §6.2 prescribes, *"reverse topological order of the call graph,
//! leaves first"*, so a caller walking [`CallGraph::components`] front to back
//! analyses every callee before its caller with no sorting of its own.
//!
//! **Iterative, not recursive**, for `region-inference.md` §11's reason one
//! level early: the engine that consumes this is to be written in Science, and
//! a deeply recursive graph algorithm over a mutually recursive descent parser
//! — *"the Science compiler will contain one"* — is the shape that overflows.
//!
//! # 3. What the graph does not have
//!
//! - **Indirect calls.** A [`Callee::Indirect`](crate::mir::Callee::Indirect) has no edge, because the
//!   callee is a value and this phase does not know which function it holds.
//!   The consequence for Decision 8 is a real one and it is stated rather than
//!   hidden: a change to a function only ever called through a closure
//!   invalidates nothing, so its callers' region results are stale. Closing it
//!   needs a call-target analysis, and [`crate::lower`]'s §8 is already the
//!   reason there is nothing to analyse yet.
//! - **Unresolved callees.** [`Callee::Unresolved`](crate::mir::Callee::Unresolved) is Decision 11's hole; an
//!   edge to *"the method this might be"* would be an edge to a guess.
//! - **Variant constructors.** [`crate::mir::Rvalue::Variant`] is not a call,
//!   for the reason stated there: a constructor runs no code and an edge to one
//!   would put a non-function in the partition Decision 8 caches on.
//! - **Weights or call counts.** A caller that wants them can count
//!   [`crate::mir::Body::callees`], which keeps repeats for that reason.

use std::collections::BTreeMap;

use science_resolve::hir::DefId;

use crate::mir::Body;

/// Who calls whom, over one crate's bodies.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// Callees per caller, deduplicated and in first-call order.
    ///
    /// A [`BTreeMap`] and not a `HashMap`: the iteration order of this map is
    /// observable in [`CallGraph::components`], and §10 item 6 wants an answer
    /// that does not move when nothing moved.
    edges: BTreeMap<DefId, Vec<DefId>>,
}

impl CallGraph {
    /// Builds the graph from every lowered body.
    pub fn of(bodies: &[Body]) -> CallGraph {
        let mut edges: BTreeMap<DefId, Vec<DefId>> = BTreeMap::new();
        for body in bodies {
            let slot = edges.entry(body.def()).or_default();
            for callee in body.callees() {
                if !slot.contains(&callee) {
                    slot.push(callee);
                }
            }
        }
        CallGraph { edges }
    }

    /// Every function with a body, in definition order.
    pub fn functions(&self) -> impl Iterator<Item = DefId> + '_ {
        self.edges.keys().copied()
    }

    /// The functions this one calls, in first-call order.
    pub fn callees(&self, def: DefId) -> &[DefId] {
        self.edges.get(&def).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether this function calls itself, directly.
    pub fn is_directly_recursive(&self, def: DefId) -> bool {
        self.callees(def).contains(&def)
    }

    /// The strongly connected components, leaves first. §2.
    ///
    /// A function with no body — a callee in another crate, an `extern` — is
    /// not a node: it has no MIR here and Decision 7 says a cross-crate callee
    /// is read as a *summary* rather than analysed.
    pub fn components(&self) -> Vec<Vec<DefId>> {
        Tarjan::new(self).run()
    }

    /// The component containing a function, if it has a body here.
    pub fn component_of(&self, def: DefId) -> Option<Vec<DefId>> {
        self.components().into_iter().find(|scc| scc.contains(&def))
    }
}

/// Tarjan's algorithm, with the recursion made a stack. §2.
struct Tarjan<'a> {
    graph: &'a CallGraph,
    nodes: Vec<DefId>,
    index_of: BTreeMap<DefId, usize>,
    /// The depth-first index assigned to each node, or `None` if unvisited.
    number: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    components: Vec<Vec<DefId>>,
}

impl<'a> Tarjan<'a> {
    fn new(graph: &'a CallGraph) -> Tarjan<'a> {
        let nodes: Vec<DefId> = graph.functions().collect();
        let index_of = nodes.iter().enumerate().map(|(at, def)| (*def, at)).collect();
        let count = nodes.len();
        Tarjan {
            graph,
            nodes,
            index_of,
            number: vec![None; count],
            low: vec![0; count],
            on_stack: vec![false; count],
            stack: Vec::new(),
            next: 0,
            components: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Vec<DefId>> {
        for start in 0..self.nodes.len() {
            if self.number[start].is_none() {
                self.visit(start);
            }
        }
        self.components
    }

    fn visit(&mut self, start: usize) {
        // Each frame is a node and how many of its successors have been taken.
        let mut frames: Vec<(usize, usize)> = vec![(start, 0)];
        self.enter(start);
        while let Some((node, taken)) = frames.pop() {
            let successors = self.successors(node);
            if taken < successors.len() {
                frames.push((node, taken + 1));
                let successor = successors[taken];
                match self.number[successor] {
                    None => {
                        self.enter(successor);
                        frames.push((successor, 0));
                    }
                    Some(number) => {
                        if self.on_stack[successor] {
                            self.low[node] = self.low[node].min(number);
                        }
                    }
                }
                continue;
            }
            // Every successor is finished: this node is a component root when
            // nothing below it reached higher.
            if self.low[node] == self.number[node].expect("entered") {
                let mut component = Vec::new();
                while let Some(member) = self.stack.pop() {
                    self.on_stack[member] = false;
                    component.push(self.nodes[member]);
                    if member == node {
                        break;
                    }
                }
                // Within a component the order is the stack's, which is an
                // artefact; sorting makes two runs agree.
                component.sort();
                self.components.push(component);
            }
            if let Some((parent, _)) = frames.last().copied() {
                self.low[parent] = self.low[parent].min(self.low[node]);
            }
        }
    }

    fn enter(&mut self, node: usize) {
        self.number[node] = Some(self.next);
        self.low[node] = self.next;
        self.next += 1;
        self.stack.push(node);
        self.on_stack[node] = true;
    }

    fn successors(&self, node: usize) -> Vec<usize> {
        self.graph
            .callees(self.nodes[node])
            .iter()
            .filter_map(|callee| self.index_of.get(callee).copied())
            .collect()
    }
}
