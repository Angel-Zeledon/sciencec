//! Decision 3, made concrete: one region variable per borrowed field, and
//! where in a value each one sits.
//!
//! > **Decision 3.** A type gets one region variable per borrowed field, and
//! > the relation between them is inferred at every use site rather than
//! > declared once at the type.
//!
//! # 1. A region position, and why the type is walked and not the value
//!
//! `science-mir`'s §5 says what this has to do: *"a region variable per borrow
//! and per reference in a type … a [`Place`]'s type is `Place::ty` and every
//! projection carries its own, so the references in a type are reachable
//! without a declaration lookup"*.
//!
//! **That last clause is true of the `borrowed T` at the top of a type and
//! false of the one inside a record.** `Parser`'s `tokens` field is
//! `borrowed Array of Token`, and a local of type `Parser` has a [`Ty`] that
//! is a [`TyKind::Named`] at `Parser` — the borrow is in the *declaration*, one
//! lookup away, and nothing in the `Ty` mentions it. So this module does the
//! lookup, and a [`Position`] is the path through fields at which a `borrowed`
//! was found.
//!
//! ```text
//! Parser          → [ Field(tokens) ]                          one position
//! borrowed Parser → [ ], [ Deref, Field(tokens) ]              two
//! Node of T       → [ Field(defs) ], [ Field(value) ]        two — §4's case
//! DefId?          → (none)
//! ```
//!
//! **The third line is the one the note is about.** Both of `Node`'s positions
//! get their own variable, nothing here relates them, and every construction
//! site relates them separately. `examples/21_compiler_shapes.science` §4 says
//! *"does inference conclude one region here, or does it need to be told?"* —
//! it concludes two, and never asks.
//!
//! # 2. The four places the walk stops, each a stated cost
//!
//! 1. **Generic arguments are not descended into.** `Array of (borrowed T)`
//!    yields no position. Two reasons, and the second is the one that matters:
//!    Decision 4 forbids a type generic over a region, so a container
//!    parameterised by how long its contents live is not a thing that can be
//!    written; and `Array` has no declaration the checker can see at all, which
//!    `science-mir`'s §7 item 6 calls the largest thing between the acceptance
//!    case and its purpose. **A borrow stored inside a container is invisible
//!    to this walk**, and that is an under-approximation — the unsound
//!    direction. It closes when `stdlib-core.md`'s containers become
//!    declarations.
//! 2. **A [`TyKind::Param`] yields no position**, which is Decision 4 restated:
//!    *"a type may not be generic over a region"*, so a `T` cannot be
//!    instantiated at a borrowed type and carries no region to speak of.
//! 3. **Recursion is cut**, by the set of record and choice definitions already
//!    on the path. A type that contains itself has infinitely many positions
//!    and finitely many useful ones; the cut keeps the first occurrence.
//! 4. **Depth is capped** at [`MAX_DEPTH`]. Without it a chain of records ten
//!    deep costs `2^10` positions on a type nobody borrows through. The cap is
//!    a second under-approximation and [`Positions::truncated`] records
//!    whether it was ever reached, so that *"no program hit it"* is a measured
//!    fact rather than a hope.
//!
//! # 3. A local the checker could not type has no region, and no diagnostic
//!
//! [`science_types::ty::Types::references_error`] documents itself as the
//! question *"a phase that wants to stay silent about a type it cannot trust
//! asks this"*, and this is such a phase. [`RegionTable::errored`] records the
//! answer per local.
//!
//! **Why it matters here specifically.** `let line be self.lines.get(cursor)`
//! binds a reference — except that `Array.get` has no declaration, so the
//! checker types `line` at [`science_types::ty::Ty::ERROR`], so §1's walk finds
//! no reference in it, so `line` carries no region. The reference is *gone*,
//! not merely unconstrained, and `§7.3`'s diagnostics about where a reference
//! points then describe a hole rather than the program.
//! [`crate::check`]'s §6 is where the flag is read and what it suppresses.
//!
//! # 4. What a region variable is
//!
//! An index. §11's Decision 11 — *"the engine is written index-not-pointer
//! throughout"* — and [`RegionVar`] is a `u32` into one body's [`RegionTable`].
//! There is no back-pointer from a variable to the place that made it: the
//! table holds a [`RegionKind`] per variable and the lookup goes that way.
//!
//! # 5. A closure's captures come from the rvalue, not from the type
//!
//! §1's whole method is *walk the type*, and it is the right method for every
//! value in the language except one. `science-mir`'s `lower` §8 makes each of a
//! closure's captures a borrow, and the closure value holds those references —
//! but [`TyKind::Closure`] is `(A) -> B` and says nothing about them, because
//! `collections-and-chains.md` §1.2 decided a closure type is a bare arrow with
//! no capture set in it.
//!
//! **Decision. A local a closure value is assigned into gets one position per
//! capture, at [`Step::Capture`], found by scanning the body's statements for
//! [`science_mir::mir::Rvalue::Closure`] and walking each capture operand's
//! own type beneath it.**
//!
//! *Why this and not a change to the type.* A capture set in the type is what
//! Rust has, and it is what forces `Fn`/`FnMut`/`FnOnce` to be three traits
//! whose selection feeds back into inference. §1.2 refused it. The information
//! still has to exist — `collections-and-chains.md` §7.6 item 6 asks for it in
//! as many words, *"the compiler must therefore record each closure's capture
//! set … from F0"* — so the only question is where, and the MIR aggregate is
//! where it already is.
//!
//! *Why it needs no change to [`crate::solve`].* A capture position is an
//! ordinary [`RegionKind::Local`], so it is seeded from the closure local's
//! liveness by the rule that seeds every other local position. That is what
//! makes a capture loan live for exactly as long as the closure value is —
//! `'loan ⊇ 'capture-temp ⊇ 'closure-local`, three ordinary constraints — and
//! it is why this is a position and not a fourth lower bound bolted onto the
//! solver. `region-inference.md`'s AMENDMENT 2 names three lower bounds and
//! would have needed a fourth had the closure's regions been modelled any other
//! way.
//!
//! **Three costs.** A local assigned *two different* closures keeps the first
//! path found at each index, so two captures at index 0 with different
//! mutability share one variable and the shared one is whichever came first in
//! block order — an over-relation, which makes regions bigger and loses no
//! conflict. A capture whose operand is not a place — which cannot happen
//! today, since `lower` always builds a temporary — contributes nothing.
//!
//! And the third is a genuine hole, stated rather than papered over: **a
//! closure that crosses a function boundary loses its capture relation at the
//! caller.** A [`Step::Capture`] position reaches [`crate::summary`] like any
//! other, but a caller's local receives the closure from a *call* and not from
//! a [`science_mir::mir::Rvalue::Closure`], so this scan gives it no capture
//! positions to relate the summary to, and the constraint is dropped. That is
//! the **unsound** direction — a lost constraint is a lost conflict — and the
//! only reason it is not reachable today is `collections-and-chains.md` §2.4:
//! *"a function returns a collection, not a chain"*, so a closure type does not
//! cross a boundary in F0. When F1 adds the opaque `some Iterate of Item = T`
//! return that §2.4 asks for, this is the line that has to change, and the
//! change is to give the positions to the *type* — which needs §1.2 to have
//! grown somewhere to put them.

use science_resolve::hir::{DefId, DefKind, DefTable};
use science_types::alias::Aliases;
use science_types::items::Declarations;
use science_types::ty::{Ty, TyKind, Types};

use science_mir::mir::{Body, BorrowId, Local, Place, Projection, Rvalue, StatementKind};

/// How deep into a type's fields the walk of §1 goes. §2 item 4.
pub const MAX_DEPTH: usize = 6;

/// One step of a [`Position`]: how to get from a value to a field of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// Through a reference, into what it points at.
    ///
    /// **A step and not nothing**, although a [`Projection::Deref`] is
    /// invisible in Science source (`AGENTS.md` §1: there is no dereference
    /// operator). Without it `borrowed (mutable borrowed T)` has two references
    /// at the same path, the second is silently dropped by every lookup keyed
    /// on the path, and the inner one is never tied to anything — which is a
    /// missed constraint, the unsound direction. The shape is not exotic: it is
    /// what `science-mir` lowers `self.f()` to inside a method whose own `self`
    /// is already a reference.
    Deref,
    /// A record's field, by definition.
    Field(DefId),
    /// A tuple's element.
    Tuple(u32),
    /// A choice variant's positional payload.
    Variant { variant: DefId, index: u32 },
    /// One capture of a closure value, by its index in
    /// [`science_mir::mir::Rvalue::Closure`]'s list.
    ///
    /// **The one step that does not come from a type.** Every other variant is
    /// found by §1's walk of a declaration; this one is read off the *rvalue*,
    /// because `science-types`'s closure type is a bare arrow — `(A) -> B`,
    /// `collections-and-chains.md` §1.2 — with no capture set in it, so
    /// [`TyKind::Closure`] yields no positions however hard the walk looks.
    ///
    /// §5 below is why the type not carrying them is a decision rather than a
    /// gap, and why this is the right place to recover them.
    Capture(u32),
}

/// Where in a type a `borrowed` sits: the path of fields to reach it.
///
/// Empty for `borrowed T` itself, which is the ordinary case and the one §4.1
/// calls *"uncontroversial"*.
pub type Path = Vec<Step>;

/// One reference inside a type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub path: Path,
    /// Whether the reference at this position is `mutable borrowed`.
    pub mutable: bool,
}

/// Every reference inside one type, with the flag §2 item 4 wants.
#[derive(Debug, Clone, Default)]
pub struct Positions {
    pub positions: Vec<Position>,
    /// Whether [`MAX_DEPTH`] was reached while walking. §2 item 4.
    pub truncated: bool,
}

/// Everything the walk reads that is not the type.
///
/// A struct rather than four parameters, for `science_mir::lower::Context`'s
/// reason one level up. `types` and `aliases` are exclusive because
/// [`Aliases::reveal`] interns: an alias expanding to `borrowed T` produces a
/// type the table may not hold yet, and a walk that declined to reveal would
/// miss every reference behind an alias — which is the under-approximating
/// direction.
pub struct Context<'a> {
    pub defs: &'a DefTable,
    pub decls: &'a Declarations,
    pub types: &'a mut Types,
    pub aliases: &'a mut Aliases,
}

impl Context<'_> {
    /// The references inside a type. §1.
    pub fn positions(&mut self, ty: Ty) -> Positions {
        let mut out = Positions::default();
        let mut path = Path::new();
        let mut seen = Vec::new();
        self.walk(ty, &mut path, &mut seen, 0, &mut out);
        out
    }

    fn walk(
        &mut self,
        ty: Ty,
        path: &mut Path,
        seen: &mut Vec<DefId>,
        depth: usize,
        out: &mut Positions,
    ) {
        if depth >= MAX_DEPTH {
            out.truncated = true;
            return;
        }
        let revealed = self.aliases.reveal(self.types, ty).unwrap_or(ty);
        match self.types.kind(revealed).clone() {
            TyKind::Borrowed { mutable, inner } => {
                out.positions.push(Position { path: path.clone(), mutable });
                // A `borrowed Parser` has the outer reference *and* whatever
                // `Parser` itself borrows, which is why this recurses rather
                // than stopping at the reference. The [`Step::Deref`] is what
                // keeps the two apart.
                path.push(Step::Deref);
                self.walk(inner, path, seen, depth + 1, out);
                path.pop();
            }
            // `T?` is not a step: a `borrowed T?` and a `borrowed T` are
            // reached by the same projection and must produce the same path,
            // or §10 item 2's *"equal expressions give equal places"* fails one
            // level up.
            TyKind::Nullable(inner) => self.walk(inner, path, seen, depth, out),
            TyKind::Tuple(elements) => {
                for (at, element) in elements.into_iter().enumerate() {
                    path.push(Step::Tuple(at as u32));
                    self.walk(element, path, seen, depth + 1, out);
                    path.pop();
                }
            }
            TyKind::Named { def, .. } => self.walk_named(def, path, seen, depth, out),
            // §2 items 1 and 2, and the three types with no fields to walk.
            TyKind::Param { .. }
            | TyKind::Closure { .. }
            | TyKind::Object { .. }
            | TyKind::SelfType { .. }
            | TyKind::SelfAssoc { .. }
            | TyKind::Unit
            | TyKind::Error => {}
        }
    }

    fn walk_named(
        &mut self,
        def: DefId,
        path: &mut Path,
        seen: &mut Vec<DefId>,
        depth: usize,
        out: &mut Positions,
    ) {
        // §2 item 3.
        if seen.contains(&def) {
            return;
        }
        seen.push(def);
        match self.defs.get(def).kind {
            DefKind::Record => {
                if let Some(record) = self.decls.record(def) {
                    for (field, ty) in record.fields.clone() {
                        path.push(Step::Field(field));
                        self.walk(ty, path, seen, depth + 1, out);
                        path.pop();
                    }
                }
            }
            DefKind::Choice => {
                for variant in self.variants_of(def) {
                    let payload =
                        self.decls.variant(variant).map(|it| it.payload.clone()).unwrap_or_default();
                    for (at, ty) in payload.into_iter().enumerate() {
                        path.push(Step::Variant { variant, index: at as u32 });
                        self.walk(ty, path, seen, depth + 1, out);
                        path.pop();
                    }
                }
            }
            // A primitive, an `extern union`, an interface named as a type:
            // nothing declared to walk into.
            _ => {}
        }
        seen.pop();
    }

    /// A choice's variants, in definition order.
    ///
    /// A scan of the definition table rather than a child list, because
    /// `hir::DefTable` is flat and a variant's parent is the choice — which is
    /// `examples/21_compiler_shapes.science` §1's own shape, read the other
    /// way round.
    fn variants_of(&self, choice: DefId) -> Vec<DefId> {
        self.defs
            .iter()
            .filter(|def| def.kind == DefKind::Variant && def.parent == Some(choice))
            .map(|def| def.id)
            .collect()
    }
}

/// A region variable's index in one body's [`RegionTable`]. §3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionVar(u32);

impl RegionVar {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn from_index(index: usize) -> RegionVar {
        RegionVar(index as u32)
    }
}

impl std::fmt::Display for RegionVar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}", self.0)
    }
}

/// What a region variable stands for.
///
/// Kept so that a diagnostic can describe a variable without printing it.
/// §7.2: *"the error message is forced to be the narrative, because it cannot
/// be the algebra"* — and a narrative needs to know whether `'7` is a loan or a
/// field of a local.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionKind {
    /// A reference inside a local's type, at a position.
    Local { local: Local, position: Position },
    /// The loan a [`science_mir::mir::Rvalue::Ref`] takes.
    Loan(BorrowId),
}

/// Every region variable in one body, and how to find the ones a place has.
#[derive(Debug, Clone)]
pub struct RegionTable {
    kinds: Vec<RegionKind>,
    /// Per local, its positions and their variables, in walk order.
    locals: Vec<Vec<(Position, RegionVar)>>,
    /// Per [`BorrowId`], the loan's variable.
    loans: Vec<RegionVar>,
    /// The variables that come from outside this body: §4 of
    /// [`crate::generate`], and the ones seeded with every point.
    universal: Vec<RegionVar>,
    /// Whether any local's type hit [`MAX_DEPTH`]. §2 item 4.
    truncated: bool,
    /// Per local, whether its type mentions [`science_types::ty::Ty::ERROR`].
    ///
    /// §5: a local the checker could not type has lost the reference it was
    /// supposed to carry, so a diagnostic about where that reference points
    /// would be a diagnostic about a hole.
    errored: Vec<bool>,
}

impl RegionTable {
    /// One variable per reference in every local's type, and one per loan.
    pub fn of(context: &mut Context<'_>, body: &Body) -> RegionTable {
        let mut kinds = Vec::new();
        let mut locals = Vec::with_capacity(body.local_count());
        let mut truncated = false;
        let mut errored = Vec::with_capacity(body.local_count());

        for (local, decl) in body.locals() {
            errored.push(context.types.references_error(decl.ty));
            let found = context.positions(decl.ty);
            truncated |= found.truncated;
            let mut slots = Vec::with_capacity(found.positions.len());
            for position in found.positions {
                let var = RegionVar::from_index(kinds.len());
                kinds.push(RegionKind::Local { local, position: position.clone() });
                slots.push((position, var));
            }
            locals.push(slots);
        }

        // §5. The positions no type walk can find, read off the rvalue that
        // holds them. Before the loans, because a `RegionVar` is an index into
        // `kinds` and the loans' indices must stay contiguous at the end.
        for (_, basic) in body.blocks() {
            for statement in &basic.statements {
                let StatementKind::Assign { place, rvalue: Rvalue::Closure { captures, .. } } =
                    &statement.kind
                else {
                    continue;
                };
                // A closure value is built into a whole local; there is no
                // spelling for assigning one into a field.
                if !place.is_local() {
                    continue;
                }
                for (at, operand) in captures.iter().enumerate() {
                    let Some(from) = operand.place() else { continue };
                    let found = context.positions(from.ty(body));
                    truncated |= found.truncated;
                    for position in found.positions {
                        let mut path = vec![Step::Capture(at as u32)];
                        path.extend(position.path);
                        let slots = &mut locals[place.local.index()];
                        if slots.iter().any(|(existing, _)| existing.path == path) {
                            continue;
                        }
                        let position = Position { path, mutable: position.mutable };
                        let var = RegionVar::from_index(kinds.len());
                        let kind =
                            RegionKind::Local { local: place.local, position: position.clone() };
                        kinds.push(kind);
                        slots.push((position, var));
                    }
                }
            }
        }

        let mut loans = Vec::with_capacity(body.borrows().len());
        for data in body.borrows() {
            let var = RegionVar::from_index(kinds.len());
            kinds.push(RegionKind::Loan(data.id));
            loans.push(var);
        }

        // A parameter's regions belong to the caller, so within this body they
        // hold everywhere. The return place is *not* one of them: its regions
        // are what the analysis computes.
        let universal =
            body.params().flat_map(|local| locals[local.index()].iter().map(|(_, var)| *var)).collect();

        RegionTable { kinds, locals, loans, universal, truncated, errored }
    }

    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    pub fn kind(&self, var: RegionVar) -> &RegionKind {
        &self.kinds[var.index()]
    }

    /// The loan's region variable.
    pub fn loan(&self, borrow: BorrowId) -> RegionVar {
        self.loans[borrow.index()]
    }

    /// The regions a whole local has.
    pub fn local(&self, local: Local) -> &[(Position, RegionVar)] {
        &self.locals[local.index()]
    }

    /// The regions belonging to the caller. §4 of [`crate::generate`].
    pub fn universal(&self) -> &[RegionVar] {
        &self.universal
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Whether the checker could not give this local a type it can be trusted
    /// about. §5.
    pub fn errored(&self, local: Local) -> bool {
        self.errored[local.index()]
    }

    /// The regions a *place* has, with the projection's own path stripped off.
    ///
    /// `(*_1).tokens` where `_1: borrowed Parser` has one region — the `tokens`
    /// field's — and not `_1`'s own, because the outer reference is behind the
    /// [`Projection::Deref`] rather than in front of it.
    ///
    /// **`None` when the projection cannot be named**, which today means it
    /// contains a [`Projection::Index`]: §2 item 1 does not descend into a
    /// container, so there is no position for an element and a caller must
    /// treat the place as carrying unknown regions rather than none.
    pub fn place(&self, place: &Place) -> Option<Vec<(Position, RegionVar)>> {
        let prefix = projection_path(place)?;
        Some(
            self.local(place.local)
                .iter()
                .filter(|(position, _)| position.path.starts_with(&prefix))
                .map(|(position, var)| {
                    (
                        Position {
                            path: position.path[prefix.len()..].to_vec(),
                            mutable: position.mutable,
                        },
                        *var,
                    )
                })
                .collect(),
        )
    }

    /// Every variable, for a dump.
    pub fn vars(&self) -> impl Iterator<Item = RegionVar> {
        (0..self.kinds.len()).map(RegionVar::from_index)
    }
}

/// The [`Path`] a projection walks, or `None` when a step has no position.
///
/// One step here per step of §1's walk, including [`Step::Deref`]: going
/// through a reference and looking inside the type it points at must produce
/// the same path, or a place's regions are looked up under a key nothing was
/// filed under.
pub fn projection_path(place: &Place) -> Option<Path> {
    let mut path = Path::new();
    let mut downcast = None;
    for step in &place.projection {
        match step {
            Projection::Deref { .. } => path.push(Step::Deref),
            Projection::Field { field, .. } => path.push(Step::Field(*field)),
            Projection::Downcast { variant, .. } => downcast = Some(*variant),
            Projection::TupleField { index, .. } => match downcast.take() {
                Some(variant) => path.push(Step::Variant { variant, index: *index }),
                None => path.push(Step::Tuple(*index)),
            },
            // §2 item 1: there is no position for an element.
            Projection::Index { .. } => return None,
        }
    }
    Some(path)
}

/// The place in front of each [`Projection::Deref`], innermost first.
///
/// What a caller borrows *through*. `(*_1).defs` is reached through `_1`, so a
/// reference the callee hands back out of `(*_1).defs` may point into `*_1` and
/// must be outlived by `_1`'s region. [`crate::generate`]'s §5 is the rule this
/// exists for and the direction it errs in.
pub fn deref_prefixes(place: &Place) -> Vec<Place> {
    let mut out = Vec::new();
    for (at, step) in place.projection.iter().enumerate() {
        if matches!(step, Projection::Deref { .. }) {
            out.push(Place {
                local: place.local,
                projection: place.projection[..at].to_vec(),
            });
        }
    }
    out
}

/// A rendering of a path, for a dump and for the `SC0335` message.
pub fn describe_path(defs: &DefTable, path: &Path) -> String {
    if path.is_empty() {
        return "the value itself".to_string();
    }
    let mut out = String::new();
    for step in path {
        if *step == Step::Deref {
            continue;
        }
        out.push('.');
        match step {
            Step::Deref => {}
            Step::Field(field) => out.push_str(&defs.get(*field).name),
            Step::Tuple(index) => out.push_str(&index.to_string()),
            Step::Variant { variant, index } => {
                out.push_str(&defs.get(*variant).name);
                out.push('.');
                out.push_str(&index.to_string());
            }
            // A capture has no name in the source — the author wrote no field
            // and no argument — so the narrative §7.2 asks for has to describe
            // it rather than quote it.
            Step::Capture(at) => {
                out.pop();
                out.push_str(&format!("capture {at}"));
            }
        }
    }
    out
}

