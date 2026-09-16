//! Queries for the phases that do not exist yet.
//!
//! §7.3 of the design spec lists the whole pipeline as queries. The ones whose
//! crates are not written yet are declared here, with the signatures the spec
//! gives them, so that `link-resolve`, `link-types`, `link-mir`,
//! `link-regions` and `link-codegen` plug into a shape that is already fixed
//! instead of inventing one each.
//!
//! Every function below panics. What is real is the signature, the key type
//! and the place it occupies in the dependency graph; the body is the next
//! crate's job.
//!
//! Two things are expected to change as they land, and neither is a redesign:
//!
//! * the placeholder result types (`Hir`, `Signature`, ...) are empty structs
//!   defined here only so the signatures compile. Each moves to the crate that
//!   owns the phase.
//! * the spec's return types are bare — `hir(ModuleId) -> Hir`. The phases
//!   that report diagnostics should return [`WithDiagnostics<T>`] like
//!   [`crate::query::tokens`] and [`crate::query::ast`] do, for the reasons
//!   given on that type. The signatures are left exactly as the spec writes
//!   them so the difference is a deliberate decision rather than a silent one.
//!
//! [`WithDiagnostics<T>`]: crate::WithDiagnostics

use link_diagnostics::FileId;

use crate::db::Db;

// --- keys ----------------------------------------------------------------
//
// Interned rather than plain integers: salsa keys a query on a value that can
// be turned into an id, and interning is also what gives these ids their
// meaning — two paths to the same definition must produce the same `DefId`, or
// the phase runs twice and its results do not unify.
//
// `unsafe(no_lifetime)` keeps the `'db` lifetime out of the signatures. It is
// sound here because these structs have no field borrowing from the database
// (`FileId` and `u32` are `'static`) and because `revisions = usize::MAX`
// disables reclamation, so an id stays valid for the life of the database.

/// A module: one file, or one directory with a `mod.link` (§12 of the spec —
/// F0 has no `mod` declaration, so a module is a file).
///
/// Placeholder. `link-resolve` owns the real one.
#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
pub struct ModuleId {
    #[returns(copy)]
    pub file: FileId,
}

/// A definition — a function, struct, enum, trait or impl item — identified
/// independently of the syntax that produced it.
///
/// Placeholder. `link-resolve` owns the real one.
#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
pub struct DefId {
    #[returns(copy)]
    pub module: ModuleId,
    /// Index of the item within its module, in source order.
    #[returns(copy)]
    pub index: u32,
}

/// A unit of code generation: the set of monomorphized items compiled into one
/// LLVM module.
///
/// Placeholder. `link-codegen` owns the real one.
#[salsa::interned(unsafe(no_lifetime), revisions = usize::MAX, debug)]
pub struct CodegenUnit {
    #[returns(copy)]
    pub index: u32,
}

// --- placeholder results -------------------------------------------------

macro_rules! placeholder_result {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        ///
        /// Placeholder: an empty type standing in for the real one until the
        /// crate that owns this phase exists.
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        #[non_exhaustive]
        pub struct $name;
    };
}

placeholder_result!(
    /// The module graph: which files are modules and how they import each
    /// other.
    ModuleTree
);
placeholder_result!(
    /// A module's HIR: its items with every name resolved to a unique id.
    Hir
);
placeholder_result!(
    /// A definition's signature: parameter and return types, generics, bounds.
    Signature
);
placeholder_result!(
    /// The type of a definition.
    Type
);
placeholder_result!(
    /// A definition's THIR: its body, fully typed.
    Thir
);
placeholder_result!(
    /// A definition's MIR: its body as a control-flow graph over explicit
    /// places and temporaries.
    Mir
);
placeholder_result!(
    /// Who calls whom. Built from every function's MIR.
    CallGraph
);
placeholder_result!(
    /// What region inference concluded about one function: the region of every
    /// borrow it makes, the regions of its parameters and return, and the
    /// ownership errors found on the way.
    RegionResult
);
placeholder_result!(
    /// One monomorphized item: a definition together with the type arguments
    /// it was instantiated with.
    MonoItem
);
placeholder_result!(
    /// The LLVM IR of one codegen unit.
    LlvmIr
);

// --- queries -------------------------------------------------------------

/// The module graph of the whole compilation.
///
/// Depends on the *set* of files through [`crate::workspace_files`], and on
/// each file's syntax tree for its imports. Editing a file's body must not
/// invalidate it; adding or removing one must.
#[salsa::tracked]
pub fn module_tree(db: &dyn Db) -> ModuleTree {
    let _ = db;
    unimplemented!("link-resolve: AST -> module graph")
}

/// A module's HIR: names resolved.
#[salsa::tracked]
pub fn hir(db: &dyn Db, module: ModuleId) -> Hir {
    let _ = (db, module);
    unimplemented!("link-resolve: AST -> HIR")
}

/// A definition's signature.
///
/// Separate from [`hir`] on purpose: a caller that only needs to type-check a
/// call site depends on the callee's signature, not on its body, so editing a
/// function body does not invalidate its callers.
#[salsa::tracked]
pub fn signature(db: &dyn Db, def: DefId) -> Signature {
    let _ = (db, def);
    unimplemented!("link-types: signature of a definition")
}

/// The type of a definition.
#[salsa::tracked]
pub fn type_of(db: &dyn Db, def: DefId) -> Type {
    let _ = (db, def);
    unimplemented!("link-types: type of a definition")
}

/// A definition's typed body.
#[salsa::tracked]
pub fn thir(db: &dyn Db, def: DefId) -> Thir {
    let _ = (db, def);
    unimplemented!("link-types: HIR -> THIR")
}

/// A definition's body as a control-flow graph.
#[salsa::tracked]
pub fn mir(db: &dyn Db, def: DefId) -> Mir {
    let _ = (db, def);
    unimplemented!("link-mir: THIR -> MIR")
}

/// The call graph, built from every function's MIR.
///
/// Exists as its own query because [`region_result`] needs it; see the comment
/// there.
#[salsa::tracked]
pub fn call_graph(db: &dyn Db) -> CallGraph {
    let _ = db;
    unimplemented!("link-mir: the call graph over every function's MIR")
}

/// Region inference and ownership checking for one function.
///
/// **This is the one query that does not depend on its own inputs alone, and
/// the shape is deliberate.** §6.2 of the design spec: a Link signature carries
/// no lifetime annotations, so the regions of a function's parameters and
/// return value are part of its *analysis result*, not of its declaration.
/// Checking a caller therefore requires the conclusions drawn about its
/// callees, which makes region inference interprocedural: functions have to be
/// analyzed in reverse topological order of the call graph, leaves first.
///
/// It is not implemented by sorting the call graph and walking it. It is
/// implemented by asking for `region_result` of each callee from inside the
/// body, and letting salsa derive the order from the dependencies that
/// creates. Two consequences follow, and whoever implements this should keep
/// both in mind:
///
/// 1. **Recursion is a cycle in the query graph, not a bug.** Mutual recursion
///    makes `region_result(a)` ask for `region_result(b)` and back. Salsa
///    detects that and resolves it with a fixpoint over the cycle — which is
///    exactly the fixpoint §6.2 prescribes over each strongly connected
///    component of the call graph, starting from the most permissive
///    approximation and tightening until it stabilizes. The salsa spelling is
///    the `cycle_fn` / `cycle_initial` options on this attribute: the initial
///    value is the most permissive approximation, and the recovery function
///    decides when the iteration has converged. Implementing it by computing
///    the SCCs by hand instead would duplicate machinery salsa already has,
///    and would recompute components that did not change.
/// 2. **Incrementality follows the call graph.** Editing a leaf function
///    invalidates the region results of everything that transitively calls it,
///    and nothing else. That is the correct answer, and it is why the
///    dependency must be a real query call rather than a lookup into a
///    pre-computed table: a table read would make every function depend on
///    every other.
///
/// [`call_graph`] is still needed to find the callees to ask about — the MIR
/// gives them, and the graph is where a caller looks them up — but it is the
/// per-callee `region_result` calls, not the graph, that carry the
/// dependencies.
///
/// §6.3: every constraint this collects carries its provenance, and the solver
/// preserves that chain, because a region error has no annotation to blame and
/// has to reconstruct its own explanation. That is a property of the value
/// this query returns, so it cannot be added later without rewriting it.
///
/// §6.4: the region engine also exposes `borrows_live_at(point) -> Set<Borrow>`
/// for F3. It is a query over this one's result and is not declared here,
/// because F0 has no suspension points to ask about.
#[salsa::tracked]
pub fn region_result(db: &dyn Db, def: DefId) -> RegionResult {
    let _ = (db, def);
    unimplemented!("link-regions: region inference over the call graph")
}

/// Every monomorphized item the program needs.
#[salsa::tracked]
pub fn mono_items(db: &dyn Db) -> Vec<MonoItem> {
    let _ = db;
    unimplemented!("link-codegen: monomorphization")
}

/// The LLVM IR for one codegen unit.
#[salsa::tracked]
pub fn llvm_module(db: &dyn Db, unit: CodegenUnit) -> LlvmIr {
    let _ = (db, unit);
    unimplemented!("link-codegen: LLVM IR emission via inkwell")
}
