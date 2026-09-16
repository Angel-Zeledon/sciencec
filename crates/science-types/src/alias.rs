//! Alias expansion: making `Embedding` and `Array of F32` one type.
//!
//! `ty`'s §6 named this hole and priced it: *"until then, `Embedding` and
//! `Array of F32` are two `Ty`s denoting one type, and `compatible` says they
//! differ. Nothing in F0 relies on them agreeing, and the first thing that does
//! is the change that has to land expansion."* This is that change.
//!
//! # 1. Revealing, not rewriting
//!
//! **Decision. An alias keeps its own [`TyKind::Named`] in the table, and
//! [`Aliases::reveal`] computes the alias-free type beside it. Nothing
//! rewrites the type the lowering produced.**
//!
//! The alternative — expanding in [`crate::lowering`], so that `Embedding`
//! never becomes a [`Ty`] of its own — is one line shorter and loses the
//! diagnostic. `ty`'s [`Types::render`] prints what it is given, so an eagerly
//! expanded program reports *"expected `Array of F32`, found `Array of F64`"*
//! at a site where the reader wrote `Embedding`, and the name they chose
//! disappears from the compiler's vocabulary. Keeping both means the relation
//! compares revealed types and the message quotes written ones.
//!
//! **What it costs** is that every relation over types has to say which it
//! takes. [`crate::assign`] says so in its first paragraph: it takes revealed
//! types, and revealing is the caller's step, because revealing needs `&mut
//! Types` to intern what it builds and assignability wants to be a `&Types`
//! question. Two functions and one sentence, against a diagnostic that cannot
//! say `Embedding`.
//!
//! # 2. Termination is a property of the table, not of the walk
//!
//! **Decision. Cycles are found once, when the table is built, and every alias
//! on a cycle has its body replaced by [`Ty::ERROR`]. [`Aliases::reveal`]
//! therefore has no visiting set, no depth limit and no cycle case at all.**
//!
//! Two facts make that a termination argument rather than a hope:
//!
//! - **A [`Ty`] is a finite tree.** A recursive record is a cycle through
//!   [`DefId`]s — `type Node: next: Node?` puts `Node`'s definition inside its
//!   own field type, and the field types are not in the type table at all. The
//!   only edge that leaves a [`Ty`] is an alias mention, which is the one
//!   [`Aliases`] follows.
//! - **The alias-mention graph is acyclic after construction**, because every
//!   cycle in it was cut. So each expansion step moves strictly forward in a
//!   topological order that exists, and the recursion is bounded by the length
//!   of the longest alias chain the author wrote.
//!
//! Reporting at construction rather than at use also gets the diagnostic right:
//! `type A is B` / `type B is A` is one mistake, in two declarations, and it is
//! reported once whether it is used a hundred times or never. A cycle check
//! that ran during expansion would report per use and say nothing at all about
//! an alias nobody happened to mention.
//!
//! **What it costs** is that a cyclic alias becomes [`Ty::ERROR`], which `ty`'s
//! §5 makes compatible with everything — so a program with a cyclic alias
//! type-checks silently *after* the cycle is reported. That is the same
//! recovery every other phase here uses, and the alternative is a second
//! diagnostic at every use of a name whose declaration is already wrong.
//!
//! # 3. Memoisation is what makes a chain of aliases affordable
//!
//! `type A1 is (A0, A0)` repeated ten times denotes a type with 1024 leaves.
//! Interned, it is ten nodes, because [`Types`] gives the two occurrences of
//! `A0` one index — and a fold that walks the *tree* rather than the graph
//! visits all 1024. The memo in [`Aliases`] is what keeps expansion linear in
//! the size of the table instead of exponential in the depth of the chain, and
//! it is not an optimisation: without it, fifty aliases is a compiler that does
//! not finish.
//!
//! The memo is sound because the alias table is immutable after construction —
//! [`Aliases::of`] is the only writer of a body. A phase that could add an
//! alias later would have to clear it, and there is no such phase.
//!
//! # 4. `SC0524`, which the note did not name
//!
//! `type-checking-and-mir.md` §13 claims `SC0520`–`SC0579` and names three;
//! `lowering`'s §4 already took `SC0523` from the same band for a hole of the
//! same shape. A cyclic alias is not any of the three, is not a resolution
//! error — every name in `type A is B` resolves — and is not reportable by any
//! phase before this one, because it is a property of the *expansion*, which is
//! this phase.

use std::collections::{HashMap, HashSet};

use science_diagnostics::{Diagnostic, Diagnostics, Label};
use science_resolve::hir::{self, DefId, DefTable};

use crate::codes;
use crate::fold::{fold_children, TypeFolder};
use crate::lowering::TypeLowerer;
use crate::normal::{AtomOrder, ConstEvalError};
use crate::subst::Substitution;
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// One `type Embedding is Array of F32`, lowered.
#[derive(Debug, Clone)]
struct AliasBody {
    /// The alias's own generic parameters, in declaration order, for
    /// [`Substitution::of_generics`].
    params: Vec<hir::GenericParam>,
    /// The right-hand side, lowered. [`Ty::ERROR`] when the alias is on a
    /// cycle (§2).
    body: Ty,
}

/// Every alias in the crate, and the expansion they define.
///
/// Built once by [`Aliases::of`] and then read. `reveal` takes `&mut self`
/// because it memoises and because interning takes `&mut Types`; that is
/// Decision 24's `mutable self` twice over, and `ty`'s §2 is the argument.
#[derive(Debug, Clone, Default)]
pub struct Aliases {
    bodies: HashMap<DefId, AliasBody>,
    /// The alias-free form of each alias's body, computed on first use. §3.
    revealed_bodies: HashMap<DefId, Ty>,
    /// The alias-free form of each type revealed so far. §3.
    memo: HashMap<Ty, Ty>,
}

impl Aliases {
    /// A table with no aliases in it. Revealing is the identity.
    pub fn new() -> Aliases {
        Aliases::default()
    }

    /// Lowers every alias in the crate and cuts every cycle.
    ///
    /// The bodies are lowered here rather than by the caller because an alias's
    /// right-hand side is a `hir::Type` like any other and must reach the table
    /// through the one door: `SC0520` for a `T??` in an alias, `SC0523` for a
    /// value in one, and the interning that makes two spellings one type are
    /// all [`TypeLowerer`]'s, and a second path into the table would be a
    /// second place for them to be forgotten.
    ///
    /// Diagnostics produced here belong to the *declarations*, not to any use,
    /// which is why this takes the crate and not an expression.
    pub fn of(
        krate: &hir::Crate,
        types: &mut Types,
        order: &AtomOrder,
        diagnostics: &mut Diagnostics,
    ) -> Aliases {
        let mut aliases = Aliases::new();
        for module in &krate.modules {
            for item in &module.items {
                let hir::ItemKind::Alias(alias) = &item.kind else {
                    continue;
                };
                let mut lowerer = TypeLowerer::new(types, &krate.defs, order, diagnostics);
                let body = lowerer.lower(&alias.ty);
                aliases.bodies.insert(
                    alias.def,
                    AliasBody { params: alias.generics.clone(), body },
                );
            }
        }
        aliases.cut_cycles(types, &krate.defs, diagnostics);
        aliases
    }

    /// Whether this definition is an alias this table expands.
    pub fn is_alias(&self, def: DefId) -> bool {
        self.bodies.contains_key(&def)
    }

    /// How many aliases the crate declares.
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }

    /// The type with every alias in it expanded.
    ///
    /// Idempotent — revealing a revealed type is the identity — and total
    /// except for the const half's arithmetic range, which is
    /// [`crate::subst`]'s §4 reached through a generic alias:
    /// `type Big of (const N: Int) is Grid of (F32, N * 3)` at an `N` large
    /// enough that `N * 3` is not an `i128`.
    pub fn reveal(&mut self, types: &mut Types, ty: Ty) -> Result<Ty, ConstEvalError> {
        if self.bodies.is_empty() {
            return Ok(ty);
        }
        self.fold_ty(types, ty)
    }

    /// The alias's right-hand side, itself revealed. §3's cache.
    fn revealed_body(&mut self, types: &mut Types, def: DefId) -> Result<Ty, ConstEvalError> {
        if let Some(&hit) = self.revealed_bodies.get(&def) {
            return Ok(hit);
        }
        let body = self.bodies[&def].body;
        let revealed = self.fold_ty(types, body)?;
        self.revealed_bodies.insert(def, revealed);
        Ok(revealed)
    }

    // --- §2's cycle check -------------------------------------------------

    /// Reports every cycle in the alias-mention graph and poisons its members.
    ///
    /// Depth-first, in [`DefId`] order so that a program with two cycles
    /// reports them in the same order every time. Ordering by id is allocation
    /// order and `normal`'s §2 says why that is not a canonical order — but
    /// here it decides only which of two independent diagnostics is emitted
    /// first, and both are emitted; it is not a key and nothing downstream
    /// reads it.
    fn cut_cycles(&mut self, types: &Types, defs: &DefTable, diagnostics: &mut Diagnostics) {
        let mut marks: HashMap<DefId, Mark> = HashMap::new();
        let mut stack: Vec<DefId> = Vec::new();
        let mut roots: Vec<DefId> = self.bodies.keys().copied().collect();
        roots.sort_unstable_by_key(|def| def.index());
        for root in roots {
            self.visit(root, types, defs, &mut marks, &mut stack, diagnostics);
        }
    }

    fn visit(
        &mut self,
        def: DefId,
        types: &Types,
        defs: &DefTable,
        marks: &mut HashMap<DefId, Mark>,
        stack: &mut Vec<DefId>,
        diagnostics: &mut Diagnostics,
    ) {
        match marks.get(&def) {
            Some(Mark::Done) => return,
            Some(Mark::Visiting) => {
                // A back edge. The cycle is the part of the stack from this
                // alias to the top; everything below it merely reaches the
                // cycle and is not itself cyclic.
                let at = stack.iter().position(|entry| *entry == def).unwrap_or(0);
                let cycle: Vec<DefId> = stack[at..].to_vec();
                diagnostics.push(cyclic_alias(defs, &cycle));
                for member in cycle {
                    if let Some(entry) = self.bodies.get_mut(&member) {
                        entry.body = Ty::ERROR;
                    }
                }
                return;
            }
            None => {}
        }

        marks.insert(def, Mark::Visiting);
        stack.push(def);
        let mut mentioned = Vec::new();
        mentions(types, &self.bodies, self.bodies[&def].body, &mut mentioned, &mut HashSet::new());
        for next in mentioned {
            self.visit(next, types, defs, marks, stack, diagnostics);
        }
        stack.pop();
        marks.insert(def, Mark::Done);
    }
}

/// Where the depth-first search has been. `Visiting` is "on the stack", which
/// is what makes a back edge a cycle rather than a second path.
enum Mark {
    Visiting,
    Done,
}

impl TypeFolder for Aliases {
    fn fold_ty(&mut self, types: &mut Types, ty: Ty) -> Result<Ty, ConstEvalError> {
        if let Some(&hit) = self.memo.get(&ty) {
            return Ok(hit);
        }
        let revealed = match types.kind(ty).clone() {
            TyKind::Named { def, args } if self.bodies.contains_key(&def) => {
                // The arguments are revealed first, so `Pair of Embedding`
                // substitutes an alias-free `Array of F32` into an alias-free
                // body and the result needs no second pass.
                let mut revealed_args = Vec::with_capacity(args.len());
                for arg in &args {
                    revealed_args.push(self.fold_arg(types, arg)?);
                }
                let body = self.revealed_body(types, def)?;
                let subst = Substitution::of_generics(&self.bodies[&def].params, &revealed_args);
                subst.apply(types, body)?
            }
            _ => fold_children(self, types, ty)?,
        };
        self.memo.insert(ty, revealed);
        Ok(revealed)
    }
}

/// Every alias the type mentions, directly.
///
/// `seen` is over types and not over aliases: the table is a graph, so one
/// `Ty` can be reached twice and walking it twice is the exponential §3
/// describes, here in the cycle check instead of in the expansion.
fn mentions(
    types: &Types,
    bodies: &HashMap<DefId, AliasBody>,
    ty: Ty,
    out: &mut Vec<DefId>,
    seen: &mut HashSet<Ty>,
) {
    if !seen.insert(ty) {
        return;
    }
    match types.kind(ty) {
        TyKind::Error
        | TyKind::Unit
        | TyKind::Param { .. }
        | TyKind::SelfType { .. }
        | TyKind::SelfAssoc { .. } => {}
        TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
            mentions(types, bodies, *inner, out, seen)
        }
        TyKind::Tuple(elements) => {
            for element in elements.clone() {
                mentions(types, bodies, element, out, seen);
            }
        }
        TyKind::Closure { params, ret } => {
            let (params, ret) = (params.clone(), *ret);
            for param in params {
                mentions(types, bodies, param, out, seen);
            }
            mentions(types, bodies, ret, out, seen);
        }
        TyKind::Named { def, args } => {
            let (def, args) = (*def, args.clone());
            if bodies.contains_key(&def) {
                out.push(def);
            }
            mention_args(types, bodies, &args, out, seen);
        }
        TyKind::Object { args, .. } => {
            let args = args.clone();
            mention_args(types, bodies, &args, out, seen);
        }
    }
}

fn mention_args(
    types: &Types,
    bodies: &HashMap<DefId, AliasBody>,
    args: &[GenericArg],
    out: &mut Vec<DefId>,
    seen: &mut HashSet<Ty>,
) {
    for arg in args {
        if let GenericArg::Type(ty) = arg {
            mentions(types, bodies, *ty, out, seen);
        }
    }
}

/// `SC0524` — a type alias that expands into itself. §4.
///
/// Every member of the cycle is labelled, because a two-alias cycle has two
/// declarations and neither one is the mistake on its own. The note spells the
/// chain out in order, since the labels are sorted by file and position by the
/// renderer and the order that matters here is the order of expansion.
fn cyclic_alias(defs: &DefTable, cycle: &[DefId]) -> Diagnostic {
    let names: Vec<String> = cycle.iter().map(|def| defs.get(*def).name.clone()).collect();
    let mut chain = String::new();
    for name in &names {
        if !chain.is_empty() {
            chain.push_str(", which expands into ");
        }
        chain.push('`');
        chain.push_str(name);
        chain.push('`');
    }
    if let Some(first) = names.first() {
        chain.push_str(", which expands into `");
        chain.push_str(first);
        chain.push('`');
    }

    let mut diagnostic =
        Diagnostic::error(codes::CYCLIC_ALIAS, "this type alias expands into itself")
            .with_label(Label::primary(
                defs.get(cycle[0]).span,
                "expanding this alias reaches it again",
            ));
    for member in &cycle[1..] {
        diagnostic = diagnostic
            .with_label(Label::secondary(defs.get(*member).span, "and through this one"));
    }
    diagnostic.with_note(chain).with_note(
        "an alias is another name for the type on its right, so a cycle names no type at \
         all — a type that contains itself is a record or a choice, which has a field to \
         hold it and a size the compiler can compute",
    )
}
