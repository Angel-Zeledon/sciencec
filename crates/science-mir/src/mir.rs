//! The IR itself — Decision 4, and the six properties `region-inference.md`
//! §10 asks of it.
//!
//! > **Decision 4.** MIR is a control-flow graph of basic blocks over explicit
//! > places and temporaries, in three-address form.
//!
//! # 1. Why a second IR at all, stated once
//!
//! THIR's §5 is the honest version of the answer: of `region-inference.md`
//! §10's six requirements, a typed *tree* can meet one and a half. Four of the
//! six are properties of a graph over storage locations, and a tree has neither
//! a graph nor a storage location. This file is those four.
//!
//! What it costs is the thing `type-checking-and-mir.md` §3.1 already priced:
//! **the user's program stops existing here.** `a and b` is one node in THIR
//! and four blocks below, and a diagnostic emitted from this level has to
//! reconstruct what was written. That is why the THIR analyses stayed in THIR
//! and why this crate reports nothing at all (`lib.rs` §6).
//!
//! # 2. Indices, not pointers — Decision 24, and §10 item 6
//!
//! Every reference in this file is an index into one [`Body`]: a [`Local`], a
//! [`BlockId`], a [`BorrowId`]. There is no `Box`, no `Rc` and no back-pointer.
//!
//! The reason is `region-inference.md` §11's Decision 11 read one level early —
//! the engine that consumes this is to be written index-not-pointer, and an IR
//! full of pointers would hand it a structure it cannot express in the language
//! it is to be written in — and §10 item 6, which wants **stable point
//! identity**. A [`Point`] is `(block, statement)` and both halves are numbers
//! this body assigned, from this body's THIR, in a deterministic order. Editing
//! another function cannot change them, because nothing outside this body was
//! read to produce them.
//!
//! **The half that is genuinely stable, and the half that is not.** Block
//! numbering is a pure function of the body's own THIR, so item 6 holds across
//! an edit to a *different* function — which is exactly the property
//! Decision 8's per-SCC cache needs, since a cache miss only matters when the
//! edit was elsewhere. It does **not** hold across an edit to *this* body:
//! inserting a statement renumbers everything after it. Nothing can fix that
//! and nothing needs to — an edited body is re-analysed by definition.
//!
//! # 3. Three-address form, and where it is broken on purpose
//!
//! Every [`Rvalue`] operand is an [`Operand`], which is a place read or a
//! constant and never a nested computation. `f(g(x))` is two statements and two
//! terminators, not one tree.
//!
//! The one deliberate exception is [`Rvalue::Record`] and [`Rvalue::Tuple`],
//! which take a vector of operands rather than being built by field-wise
//! assignment into a destination. A record literal is one *initialisation* of
//! one place, and splitting it into `n` field assignments would make the
//! destination partially initialised at `n-1` intermediate points — which the
//! move analysis of [`crate::moves`] would then have to understand, for a
//! construct that cannot fail in between. The cost is that a consumer must
//! iterate a vector at those two arms; the alternative was a lattice.
//!
//! # 4. Drop is a terminator, and the flag is a field on it
//!
//! A drop is a call to `Drop.drop(mutable self)`, so it is a call, and calls
//! end blocks here. Decision 26's flag is a *field* on [`TerminatorKind::Drop`]
//! rather than an elaborated `if flag: drop(x)` in the CFG, and that is the one
//! shape decision in this file that a reader is likely to disagree with.
//!
//! **The reason** is that the fact Decision 26 exists to record is *"this drop
//! is conditional"*, and a branch on an opaque byte records it in a form every
//! later consumer has to pattern-match back out. Region inference wants to know
//! that an exclusive borrow happens *maybe* here; codegen wants to know whether
//! to emit a test. Both read one `Option`.
//!
//! **The cost** is that codegen must expand the flag test itself, so
//! `codegen-and-linking.md` Decision 5's *"every MIR basic block becomes
//! exactly one LLVM basic block"* is false for a flagged drop — it becomes
//! three. That is a real amendment owed to that note and `lib.rs`'s §7
//! records it as a finding rather than burying it here.

use science_diagnostics::Span;
use science_resolve::hir::{BinaryOp, DefId, Literal, UnaryOp};
use science_types::assign::Coercion;
use science_types::ty::Ty;

// --- locals ---------------------------------------------------------------

/// A storage location in one body: a parameter, a binding, a temporary, or a
/// drop flag.
///
/// The name is Rust's and so is the numbering: [`RETURN_PLACE`] is `_0`, the
/// parameters follow in declaration order, and everything after is allocated
/// in the order the lowering reached it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Local(u32);

impl Local {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The local at an index.
    ///
    /// Public because a consumer that holds a bitset over locals — which
    /// liveness is — indexes it by number and has to get back.
    pub fn from_index(index: usize) -> Local {
        Local(index as u32)
    }
}

impl std::fmt::Display for Local {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "_{}", self.0)
    }
}

/// `_0`, which every `return` assigns to and nothing drops.
pub const RETURN_PLACE: Local = Local(0);

/// Where a local came from, which is what tells a consumer whether the *user*
/// can name it.
///
/// Carried rather than derived: `region-inference.md` §7.1's error message
/// quotes the name the borrow was taken from, and a local that has no name is
/// one a message must describe instead of quote. Deriving that from a side
/// table would make the message's quality depend on a lookup succeeding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    /// `_0`.
    Return,
    /// A declared parameter, or the `self` receiver.
    Param(DefId),
    /// A `let` binding, or a binding introduced by a pattern.
    Binding(DefId),
    /// A temporary this lowering introduced. The user cannot name it.
    Temp,
    /// Decision 26's drop flag for the local it names. One byte, and the only
    /// storage in a Science program that the author did not write.
    DropFlag(Local),
}

/// One local's declaration.
#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: Ty,
    pub kind: LocalKind,
    /// The span of the thing that introduced it: the binding for a
    /// [`LocalKind::Binding`], the expression for a [`LocalKind::Temp`].
    pub span: Span,
}

impl LocalDecl {
    /// The definition this local stands for, when the user wrote a name for it.
    pub fn def(&self) -> Option<DefId> {
        match self.kind {
            LocalKind::Param(def) | LocalKind::Binding(def) => Some(def),
            LocalKind::Return | LocalKind::Temp | LocalKind::DropFlag(_) => None,
        }
    }
}

// --- places ---------------------------------------------------------------

/// One step of a projection off a local.
///
/// **This is what MIR buys, and THIR's §3 is the statement of the debt.** That
/// file's `Place` is a root and a chain of fields and stops there, *"because an
/// index place needs the value of the index, which is a MIR temporary and not a
/// name"*. Here there are temporaries, so [`Projection::Index`] holds one and
/// the place is expressible.
///
/// **Every variant carries the type it projects to**, and that type comes from
/// the *declaration* — [`science_types::items::Record`] for a field,
/// [`science_types::items::Variant`] for a payload — and never from the
/// expression that was lowered. That is the whole of §10 item 2's *"two
/// expressions denoting the same place must produce the same place"*: two
/// spellings of `config.port` reach the same `Record` entry, so they produce
/// the same `Ty`, so the two places compare equal under the derived [`Eq`].
/// Taking the type off the THIR node instead would have made an alias written
/// at one site and not the other into two different places.
///
/// **What it costs** is a `Ty` per projection step — four bytes, interned —
/// and one invariant a reader has to trust: that the lowering never builds a
/// projection out of a node's type. [`crate::lower`] has one function per
/// projection kind and they all take the declaration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Projection {
    /// `*p`, which the user never writes: it is inserted wherever a field or an
    /// index is taken through a `borrowed T`.
    ///
    /// Science has no dereference operator (`AGENTS.md` §1), so every `Deref`
    /// here was inferred from the base's type. That makes it the projection a
    /// reader is most likely to be surprised by and the one rule 5 is most
    /// often about.
    Deref { ty: Ty },
    /// `base.name`, with the field resolved.
    Field { field: DefId, ty: Ty },
    /// `base.0` — a tuple element, and the payload of a variant after a
    /// [`Projection::Downcast`].
    TupleField { index: u32, ty: Ty },
    /// `base[i]`, where `i` is a temporary holding the index *value*.
    ///
    /// The temporary is assigned exactly once and never reassigned — the
    /// lowering allocates a fresh one per index expression — which is what
    /// makes structural equality on this variant mean *"the same element"*
    /// rather than *"the same spelling"*. [`Body::index_temps_are_single_assignment`]
    /// is that invariant, checkable.
    Index { index: Local, ty: Ty },
    /// `base as Variant` — narrowing a choice value to one variant so its
    /// payload can be projected. Emitted only by `match` lowering.
    Downcast { variant: DefId, ty: Ty },
}

impl Projection {
    /// The type this step projects to.
    pub fn ty(&self) -> Ty {
        match *self {
            Projection::Deref { ty }
            | Projection::Field { ty, .. }
            | Projection::TupleField { ty, .. }
            | Projection::Index { ty, .. }
            | Projection::Downcast { ty, .. } => ty,
        }
    }
}

/// A place: a local and a chain of projections off it. §10 item 2, completed.
///
/// Ordered as well as hashed, for the reason [`science_types::thir::Place`]
/// gives one level up: a diagnostic that lists places lists them the same way
/// twice.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<Projection>,
}

impl Place {
    /// The place that is a whole local.
    pub fn local(local: Local) -> Place {
        Place { local, projection: Vec::new() }
    }

    /// This place with one more step projected off it.
    pub fn project(&self, step: Projection) -> Place {
        let mut projection = self.projection.clone();
        projection.push(step);
        Place { local: self.local, projection }
    }

    /// Whether this place is the whole of a local, with nothing projected.
    pub fn is_local(&self) -> bool {
        self.projection.is_empty()
    }

    /// Whether `self` is `other` or is reachable from it by projection.
    ///
    /// The prefix predicate [`science_types::thir::Place::starts_with`] is, one
    /// level down, and it means the same thing: *"writing to `config`
    /// invalidates a fact about `config.port`; writing to `config.other` does
    /// not"*.
    ///
    /// **This is not the aliasing predicate.** Use [`Place::may_overlap`].
    pub fn starts_with(&self, other: &Place) -> bool {
        self.local == other.local
            && self.projection.len() >= other.projection.len()
            && self.projection[..other.projection.len()] == other.projection[..]
    }

    /// Whether the two places **may** denote overlapping storage.
    ///
    /// **Decision. Equality means *definitely the same storage*; this predicate
    /// means *possibly*, and they differ at exactly one place — an index whose
    /// temporary differs.** `a[i]` and `a[j]` are not equal, because `i` and
    /// `j` are different locals, and they may still be the same element,
    /// because nothing here knows what those locals hold.
    ///
    /// **The reason the two are separate functions** is that they are used in
    /// opposite directions and merging them makes one of the two unsound.
    /// Narrowing and the move analysis want *definitely the same* — being wrong
    /// there admits a program that should be refused, which THIR's §3 calls
    /// *"unsoundness in the direction §4.2 says the meet must never go"*. The
    /// conflict check of `region-inference.md` §3 step 4 wants *possibly the
    /// same* — being wrong there misses a conflict. One predicate cannot be
    /// conservative in both directions at once.
    ///
    /// **What it costs** is that `a[0]` and `a[1]` are reported as overlapping,
    /// so a program that borrows two distinct constant elements of one array is
    /// refused. That is Rust's behaviour too, and the fix in both languages is
    /// a slice-splitting library function rather than a cleverer predicate.
    pub fn may_overlap(&self, other: &Place) -> bool {
        if self.local != other.local {
            return false;
        }
        let common = self.projection.len().min(other.projection.len());
        for step in 0..common {
            let (left, right) = (&self.projection[step], &other.projection[step]);
            if left == right {
                continue;
            }
            // Two indices of the same container may be the same element; the
            // values are not known here. Everything else that differs — a
            // different field, a different variant, one deref against one
            // field — is disjoint storage.
            match (left, right) {
                (Projection::Index { .. }, Projection::Index { .. }) => continue,
                _ => return false,
            }
        }
        true
    }

    /// The type this place has, given the body it belongs to.
    pub fn ty(&self, body: &Body) -> Ty {
        match self.projection.last() {
            Some(step) => step.ty(),
            None => body.local_decl(self.local).ty,
        }
    }
}

// --- operands and rvalues -------------------------------------------------

/// A constant: something with no storage to read.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// A literal, exactly as THIR kept it. The value a `1` denotes is a fact
    /// about its type, and the type is on the statement beside it.
    Literal(Literal),
    /// A function, a constant, or a unit variant named as a value.
    Item(DefId),
    Unit,
}

/// What a computation reads.
///
/// **`Copy` and `Move` are the move analysis's entire input**, so getting the
/// split wrong is the one lowering mistake with a soundness consequence rather
/// than a quality one. The rule is in [`crate::lower`]'s §5 and the direction
/// is stated there: *not* `Copy` unless the type is one this phase can prove
/// trivially copyable, because calling a move a copy hides a use-after-move and
/// calling a copy a move costs a drop flag.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Copy(Place),
    Move(Place),
    Const(Constant),
}

impl Operand {
    /// The place read, for an operand that reads one.
    pub fn place(&self) -> Option<&Place> {
        match self {
            Operand::Copy(place) | Operand::Move(place) => Some(place),
            Operand::Const(_) => None,
        }
    }

    /// The place moved out of, for an operand that moves.
    pub fn moved_place(&self) -> Option<&Place> {
        match self {
            Operand::Move(place) => Some(place),
            Operand::Copy(_) | Operand::Const(_) => None,
        }
    }
}

/// A borrow's index in its body. §10 item 4's set, named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BorrowId(u32);

impl BorrowId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The borrow at an index. Public for [`Local::from_index`]'s reason.
    pub fn from_index(index: usize) -> BorrowId {
        BorrowId(index as u32)
    }
}

/// What kind of borrow a [`Rvalue::Ref`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    /// `borrowed x`. Many at once; rule 4's permissive half.
    Shared,
    /// `mutable borrowed x`, live from the statement that takes it.
    Exclusive,
    /// `mutable borrowed x` in argument position: **reserved** here, and
    /// **activated** at the call. `region-inference.md` §10 item 4.
    ///
    /// [`crate::lower`]'s §6 is the whole of the argument for which borrows get
    /// this kind and what the two points oblige the region engine to check.
    TwoPhase,
}

/// Everything §10 item 4 asks MIR to expose about one borrow.
///
/// Collected into [`Body::borrows`] so that region inference reads a table
/// rather than re-scanning the CFG for `Ref` rvalues — which is the same
/// argument THIR's §5 makes when it says MIR *"inherits the reservation points
/// rather than rediscovering them"*, one level on.
#[derive(Debug, Clone)]
pub struct BorrowData {
    pub id: BorrowId,
    pub kind: BorrowKind,
    /// The place borrowed *from*.
    pub place: Place,
    /// The place the reference was assigned *to*, which is the local whose
    /// liveness the region is grown along.
    pub destination: Place,
    /// Where the borrow is taken. For a [`BorrowKind::TwoPhase`] this is the
    /// reservation.
    pub reserved: Point,
    /// Where a two-phase borrow becomes exclusive. `None` for every other kind,
    /// where reservation and activation are the same point.
    pub activation: Option<Point>,
    pub span: Span,
}

/// A computation, in three-address form. §3.
#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    Use(Operand),
    /// `borrowed p` / `mutable borrowed p`.
    Ref { kind: BorrowKind, place: Place, borrow: BorrowId },
    Unary { op: UnaryOp, operand: Operand },
    /// A binary operator. **`and` and `or` never appear here** — Decision 3
    /// made them one THIR node so a diagnostic could quote them, and §10 item 1
    /// is the reason they are blocks and edges by the time they reach this
    /// file. [`crate::lower`]'s §3.
    Binary { op: BinaryOp, lhs: Operand, rhs: Operand },
    /// `e as T`.
    Cast { operand: Operand, ty: Ty },
    /// `Doc(title: "a")`. §3 says why this is one rvalue and not `n` stores.
    Record { def: DefId, fields: Vec<(DefId, Operand)> },
    /// `Ok(1)` — a choice variant with its positional payload.
    ///
    /// Not a [`TerminatorKind::Call`], although THIR spells it as a call of an
    /// [`science_types::thir::ExprKind::Item`]: a constructor runs no code, and
    /// an edge to it in [`crate::callgraph`] would put a non-function in the
    /// SCC decomposition Decision 8 partitions the incremental cache by.
    Variant { variant: DefId, payload: Vec<Operand> },
    Tuple(Vec<Operand>),
    Range { start: Operand, end: Operand, inclusive: bool },
    /// `e?` — the presence test, a `Bool`, and total.
    IsPresent(Operand),
    /// Which variant a choice value holds. Opaque: the *numbering* is a layout
    /// question and `science-codegen` owns layout, so a
    /// [`TerminatorKind::Switch`] branches on a [`DefId`] and not on an integer.
    Discriminant(Place),
    /// Decision 3's third clause, carried down one level unchanged.
    Coerce { operand: Operand, coercion: Coercion, ty: Ty },
    /// A read of a place THIR's Decision 7 narrowed.
    ///
    /// **Carried, never recomputed.** Decision 25 is legal only because
    /// narrowing's conclusion is consumed after region inference runs, and a
    /// MIR pass that re-derived it would be consuming it before.
    /// `lib.rs`'s §5 is that argument in full.
    Narrow { operand: Operand, ty: Ty },
    /// A closure value: its captures, and a pointer back to the body that was
    /// not lowered.
    ///
    /// **An aggregate, exactly as [`Rvalue::Record`] and [`Rvalue::Tuple`] are**
    /// — `codegen-and-linking.md`'s own table says a closure value is *"a struct
    /// of `{ fn ptr, captures }`"*, and this is that struct with the function
    /// pointer left as `thir_body`. `captures[i]` is a reference into the
    /// enclosing frame, taken by an ordinary [`Rvalue::Ref`] in the statement
    /// before this one, so **a capture is a borrow every consumer of this IR
    /// already knows how to read**: it is in [`Body::borrows`], it has a
    /// [`BorrowId`], and nothing had to learn a new kind of loan.
    /// [`crate::lower`]'s §8 is the discipline and its price;
    /// [`crate::capture`] is the walk that finds the set.
    ///
    /// `thir_body` is the one place in this crate where an id from another IR
    /// survives, and it survives because the closure's body has no [`DefId`] to
    /// be a [`Body`] of. §8's *"what is left"* says whose that is.
    Closure {
        param: DefId,
        thir_body: science_types::thir::ExprId,
        captures: Vec<Operand>,
        ty: Ty,
    },
    /// A value this phase could not build, because THIR had
    /// [`science_types::thir::ExprKind::Error`] there.
    ///
    /// `ty`'s §5 one level down: a hole is a value rather than an absence, so
    /// no consumer has to branch on whether the statement exists.
    Error,
}

// --- statements and terminators -------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    Assign { place: Place, rvalue: Rvalue },
    /// §10 item 3. The local's storage begins.
    StorageLive(Local),
    /// §10 item 3. The local's storage ends, and rule 5 — *"no borrow outlives
    /// its referent"* — is checked against this point and no other.
    StorageDead(Local),
    /// Decision 26. Sets a drop flag to a constant.
    SetDropFlag { flag: Local, value: bool },
    /// The activation point of a [`BorrowKind::TwoPhase`] borrow.
    ///
    /// **Explicit rather than derived**, which is where this file goes further
    /// than Rust: rustc computes the activation as the borrow temporary's
    /// unique later use, and §10 item 4 asks that MIR *expose* the reservation
    /// point rather than leave it to be found. A statement costs one `Nop` at
    /// codegen and saves the consumer an analysis it would otherwise have to
    /// get right to compile `v.push(v.len())`.
    Activate(BorrowId),
    Nop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: Span,
}

/// Who a call calls.
#[derive(Debug, Clone, PartialEq)]
pub enum Callee {
    /// A function or a method, resolved to its definition.
    Def(DefId),
    /// A value of closure type, called.
    Indirect(Operand),
    /// Decision 5's runtime entry point: *"in F0, a whole-array operation
    /// lowers to a call to a runtime or library entry point, never to an
    /// inlined MIR loop"*.
    ///
    /// **Nothing in F0's THIR produces one**, and the variant is here so that
    /// the array IR §3.4 keeps possible has somewhere to land that is not a
    /// loop. `lib.rs`'s §4 states the invariant that keeps the hole open,
    /// and `tests/no_invented_loops.rs` checks it.
    Runtime(&'static str),
    /// A callee this phase cannot name. [`Unresolved`] says which hole.
    Unresolved(Unresolved),
}

/// Which hole a [`Callee::Unresolved`] stands in.
///
/// Every one of these is a hole *above* this crate, carried rather than
/// papered over, and each disappears when the phase that owns it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unresolved {
    /// `doc.title()` where `MethodCall::method` is `None`. Decision 11's
    /// lookup; `science-types`'s `check`'s §6 calls it *"the largest hole in
    /// the layer"*.
    Method,
    /// The `next()` of a `for` loop. `Iterate` does not exist, so the loop's
    /// *shape* is lowered and its callee is not. [`crate::lower`]'s §7.
    IterateNext,
    /// An operator or an index on a user type, which is Decision 11 again.
    Operator,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminatorKind {
    Goto { target: BlockId },
    /// The only conditional branch. Science's conditions are `Bool` (§4.2 of
    /// the core spec), so there is no integer switch to be had here and
    /// [`TerminatorKind::Switch`] is the separate, variant-keyed one.
    If { cond: Operand, then_block: BlockId, else_block: BlockId },
    /// A `match` on which variant a choice value holds.
    ///
    /// Keyed on the variant's [`DefId`]; see [`Rvalue::Discriminant`].
    Switch { discr: Operand, arms: Vec<(DefId, BlockId)>, otherwise: BlockId },
    /// A call. `target` is `None` when the callee cannot return — which in F0
    /// means `panic`.
    Call { callee: Callee, args: Vec<Operand>, destination: Place, target: Option<BlockId> },
    /// §10 item 5. A drop is an exclusive borrow at a point the user did not
    /// write, and §4 of this file says why the flag is a field rather than a
    /// branch.
    Drop { place: Place, flag: Option<Local>, target: BlockId },
    Return,
    /// Reached by no execution. The `otherwise` arm of an exhaustive `match`,
    /// and the successor of a diverging call.
    Unreachable,
}

impl TerminatorKind {
    /// The blocks control can reach from here, in a deterministic order.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            TerminatorKind::Goto { target } => vec![*target],
            TerminatorKind::If { then_block, else_block, .. } => vec![*then_block, *else_block],
            TerminatorKind::Switch { arms, otherwise, .. } => {
                let mut out: Vec<BlockId> = arms.iter().map(|(_, block)| *block).collect();
                out.push(*otherwise);
                out
            }
            TerminatorKind::Call { target, .. } => target.iter().copied().collect(),
            TerminatorKind::Drop { target, .. } => vec![*target],
            TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}

// --- blocks and points ----------------------------------------------------

/// A basic block's index in its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(u32);

impl BlockId {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    /// The block at an index. Public for [`Local::from_index`]'s reason.
    pub fn from_index(index: usize) -> BlockId {
        BlockId(index as u32)
    }
}

impl std::fmt::Display for BlockId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bb{}", self.0)
    }
}

/// The entry block. Reserved first, so it is always index zero — the property
/// [`science_types::thir::Body::root`] already has one level up.
pub const ENTRY_BLOCK: BlockId = BlockId(0);

/// A basic block: statements, and the one terminator that ends it.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

impl BasicBlock {
    /// The point of the terminator, which is one past the last statement.
    pub fn terminator_point(&self, block: BlockId) -> Point {
        Point { block, statement: self.statements.len() as u32 }
    }
}

/// `region-inference.md` Decision 1's point: *"a `(basic block, statement
/// index)` pair"*, and the whole of what a region is a set of.
///
/// `statement == statements.len()` is the terminator. That convention rather
/// than a separate `Terminator` variant, because the region lattice is set
/// inclusion over points and a lattice element that is a sum type has two
/// orderings to keep in step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub block: BlockId,
    pub statement: u32,
}

impl std::fmt::Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}]", self.block, self.statement)
    }
}

// --- the body -------------------------------------------------------------

/// One function body, lowered.
///
/// Self-contained the way [`science_types::thir::Body`] is: every [`Local`],
/// [`BlockId`] and [`BorrowId`] in it indexes this body and no other.
#[derive(Debug, Clone)]
pub struct Body {
    pub(crate) def: DefId,
    pub(crate) locals: Vec<LocalDecl>,
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) borrows: Vec<BorrowData>,
    /// How many of [`Body::locals`] are the return place and the parameters,
    /// in that order. Everything at or after this index is a binding, a
    /// temporary or a drop flag.
    pub(crate) arg_count: usize,
    pub(crate) span: Span,
    /// Predecessors, computed once when the body is finished. A cache, not a
    /// second definition: [`Body::check_predecessors`] says so and is a test.
    pub(crate) predecessors: Vec<Vec<BlockId>>,
}

impl Body {
    /// The function this is the body of.
    pub fn def(&self) -> DefId {
        self.def
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn block(&self, id: BlockId) -> &BasicBlock {
        &self.blocks[id.index()]
    }

    pub fn blocks(&self) -> impl Iterator<Item = (BlockId, &BasicBlock)> {
        self.blocks.iter().enumerate().map(|(at, block)| (BlockId::from_index(at), block))
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn local_decl(&self, local: Local) -> &LocalDecl {
        &self.locals[local.index()]
    }

    pub fn locals(&self) -> impl Iterator<Item = (Local, &LocalDecl)> {
        self.locals.iter().enumerate().map(|(at, decl)| (Local::from_index(at), decl))
    }

    pub fn local_count(&self) -> usize {
        self.locals.len()
    }

    /// The parameters, in declaration order, with `self` first when there is
    /// one.
    pub fn params(&self) -> impl Iterator<Item = Local> + '_ {
        (1..self.arg_count + 1).map(Local::from_index)
    }

    /// The local a binding was given, if this body introduced it.
    ///
    /// A scan, for [`science_types::thir::Body::local_ty`]'s reason one level
    /// up: a body's bindings are few and the iteration order is declaration
    /// order, which is what a diagnostic that walks them wants.
    pub fn local_of(&self, def: DefId) -> Option<Local> {
        self.locals()
            .find(|(_, decl)| decl.def() == Some(def))
            .map(|(local, _)| local)
    }

    /// Every borrow this body takes, with its reservation and — for a
    /// two-phase borrow — its activation. §10 item 4.
    pub fn borrows(&self) -> &[BorrowData] {
        &self.borrows
    }

    pub fn borrow_data(&self, id: BorrowId) -> &BorrowData {
        &self.borrows[id.index()]
    }

    /// The blocks control can reach from `block`.
    pub fn successors(&self, block: BlockId) -> Vec<BlockId> {
        self.block(block).terminator.kind.successors()
    }

    /// The blocks control can reach `block` from.
    ///
    /// Region inference's liveness (§3 step 1) is a backward dataflow and wants
    /// this at every step; computing it per query would make the fixpoint
    /// quadratic in the CFG for no reason.
    pub fn predecessors(&self, block: BlockId) -> &[BlockId] {
        &self.predecessors[block.index()]
    }

    /// Every point in the body, in block order then statement order.
    ///
    /// The order is the one a bitset over points wants: a point's position in
    /// this iteration is a stable dense index, so a region *is* a bitset and
    /// `region-inference.md` §3's complexity claim holds.
    pub fn points(&self) -> impl Iterator<Item = Point> + '_ {
        self.blocks().flat_map(|(id, block)| {
            (0..=block.statements.len() as u32).map(move |statement| Point { block: id, statement })
        })
    }

    pub fn point_count(&self) -> usize {
        self.blocks.iter().map(|block| block.statements.len() + 1).sum()
    }

    /// The statement at a point, or `None` when the point is the terminator.
    pub fn statement_at(&self, point: Point) -> Option<&Statement> {
        self.block(point.block).statements.get(point.statement as usize)
    }

    /// Whether a point is a block's terminator.
    pub fn is_terminator(&self, point: Point) -> bool {
        point.statement as usize == self.block(point.block).statements.len()
    }

    /// Every point at which a local's storage ends. §10 item 3, and the only
    /// thing rule 5 is checked against.
    ///
    /// A local can have more than one: a `return` out of a nested block ends
    /// the storage of every enclosing scope's locals on *that* path, and the
    /// fallthrough ends them again on the other.
    pub fn storage_dead_points(&self, local: Local) -> Vec<Point> {
        let mut out = Vec::new();
        for (id, block) in self.blocks() {
            for (at, statement) in block.statements.iter().enumerate() {
                if statement.kind == StatementKind::StorageDead(local) {
                    out.push(Point { block: id, statement: at as u32 });
                }
            }
        }
        out
    }

    /// Decision 26's flags: the local each stands for, and the flag itself.
    ///
    /// Empty for a body with no conditionally moved local, which is *"the
    /// difference between a rare cost and a tax on every function"*.
    pub fn drop_flags(&self) -> Vec<(Local, Local)> {
        self.locals()
            .filter_map(|(flag, decl)| match decl.kind {
                LocalKind::DropFlag(guarded) => Some((guarded, flag)),
                _ => None,
            })
            .collect()
    }

    /// Every definition this body calls, in the order the calls appear.
    ///
    /// [`crate::callgraph`] is built out of this; a repeat is kept rather than
    /// deduplicated, because a caller that wants the set can build one and a
    /// caller that wants the count cannot recover it from a set.
    pub fn callees(&self) -> Vec<DefId> {
        let mut out = Vec::new();
        for (_, block) in self.blocks() {
            if let TerminatorKind::Call { callee: Callee::Def(def), .. } = block.terminator.kind {
                out.push(def);
            }
        }
        out
    }

    /// Whether every [`Projection::Index`] temporary is assigned exactly once.
    ///
    /// The invariant [`Projection::Index`]'s documentation rests on, written as
    /// a predicate so that a test can hold the lowering to it rather than a
    /// comment claiming it.
    pub fn index_temps_are_single_assignment(&self) -> bool {
        let mut indices: Vec<Local> = Vec::new();
        for (_, block) in self.blocks() {
            for statement in &block.statements {
                if let StatementKind::Assign { rvalue, .. } = &statement.kind {
                    collect_index_temps(rvalue, &mut indices);
                }
            }
        }
        indices.sort();
        indices.dedup();
        for index in indices {
            let mut writes = 0;
            for (_, block) in self.blocks() {
                for statement in &block.statements {
                    if let StatementKind::Assign { place, .. } = &statement.kind {
                        if place.local == index {
                            writes += 1;
                        }
                    }
                }
                if let TerminatorKind::Call { destination, .. } = &block.terminator.kind {
                    if destination.local == index {
                        writes += 1;
                    }
                }
            }
            if writes != 1 {
                return false;
            }
        }
        true
    }

    /// Whether the cached predecessor table agrees with the terminators.
    pub fn check_predecessors(&self) -> bool {
        self.predecessors == predecessors_of(&self.blocks)
    }
}

fn collect_index_temps(rvalue: &Rvalue, out: &mut Vec<Local>) {
    let mut from_place = |place: &Place| {
        for step in &place.projection {
            if let Projection::Index { index, .. } = step {
                out.push(*index);
            }
        }
    };
    let mut from_operand = |operand: &Operand| {
        if let Some(place) = operand.place() {
            from_place(place);
        }
    };
    match rvalue {
        Rvalue::Use(operand)
        | Rvalue::Unary { operand, .. }
        | Rvalue::Cast { operand, .. }
        | Rvalue::IsPresent(operand)
        | Rvalue::Coerce { operand, .. }
        | Rvalue::Narrow { operand, .. } => from_operand(operand),
        Rvalue::Binary { lhs, rhs, .. } => {
            from_operand(lhs);
            from_operand(rhs);
        }
        Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) => from_place(place),
        Rvalue::Record { fields, .. } => {
            for (_, operand) in fields {
                from_operand(operand);
            }
        }
        Rvalue::Variant { payload, .. } => {
            for operand in payload {
                from_operand(operand);
            }
        }
        Rvalue::Tuple(operands) | Rvalue::Closure { captures: operands, .. } => {
            for operand in operands {
                from_operand(operand);
            }
        }
        Rvalue::Range { start, end, .. } => {
            from_operand(start);
            from_operand(end);
        }
        Rvalue::Error => {}
    }
}

/// The predecessor table of a CFG. One pass, and the graph is written here
/// rather than imported: `region-inference.md` §11 makes the CFG *"a vector of
/// blocks with integer successors"*, and a dependency that made it anything
/// else would be a dependency the self-hosted engine cannot port.
pub(crate) fn predecessors_of(blocks: &[BasicBlock]) -> Vec<Vec<BlockId>> {
    let mut table = vec![Vec::new(); blocks.len()];
    for (at, block) in blocks.iter().enumerate() {
        let from = BlockId::from_index(at);
        for successor in block.terminator.kind.successors() {
            let slot: &mut Vec<BlockId> = &mut table[successor.index()];
            if !slot.contains(&from) {
                slot.push(from);
            }
        }
    }
    table
}

/// The blocks in reverse post-order from the entry.
///
/// The order a forward dataflow wants — every block after all the predecessors
/// it can be reached from without a back edge — and the order the move analysis
/// of [`crate::moves`] iterates in. Written here for
/// `predecessors_of`'s reason.
pub fn reverse_postorder(body: &Body) -> Vec<BlockId> {
    let mut seen = vec![false; body.blocks.len()];
    let mut order = Vec::new();
    // An explicit stack rather than recursion: a long function is a deep CFG,
    // and `region-inference.md` §11 wants this expressible without one.
    let mut stack = vec![(ENTRY_BLOCK, 0usize)];
    seen[ENTRY_BLOCK.index()] = true;
    while let Some((block, next)) = stack.pop() {
        let successors = body.successors(block);
        if next < successors.len() {
            stack.push((block, next + 1));
            let successor = successors[next];
            if !seen[successor.index()] {
                seen[successor.index()] = true;
                stack.push((successor, 0));
            }
        } else {
            order.push(block);
        }
    }
    order.reverse();
    order
}
