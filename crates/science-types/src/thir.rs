//! THIR — the typed tree, and the last IR that still looks like the program.
//!
//! > **Decision 3.** THIR is HIR with a type on every node, method calls
//! > resolved to a specific implementation, and implicit conversions made
//! > explicit.
//!
//! It earns its place by being where the *user's program still exists*. A
//! diagnostic about the user's code is best emitted from a tree that has the
//! user's structure, and once MIR has flattened `a and b` into blocks and
//! jumps, a message about it has to reconstruct what was written. §3.1 names
//! three passes that run here for that reason — narrowing, `SC0140` and
//! exhaustiveness — and **all three are now in this crate**
//! ([`crate::narrow`], [`crate::unchecked`], [`crate::exhaustive`]). The third
//! arrived last and is the one this file was shaped for; §1's next paragraph
//! is the promise it collected.
//!
//! # 1. Each of the three clauses, and what it earns
//!
//! **A type on every node.** Not on expressions only: [`Pat`] carries one too,
//! because the pass Decision 16 asks for needs the scrutinee's type at every
//! sub-pattern and a pattern without one would make exhaustiveness re-derive
//! what the checker already knew. The cost is that a type must exist even
//! where the checker failed, which is [`Ty::ERROR`] and `ty`'s §5 — the tree is
//! always complete, and a hole in it is a type rather than an absence.
//!
//! **That promise was collected, and it held in both directions.**
//! [`crate::exhaustive`]'s `field_tys` reads the payload types of `Ident(name)`
//! off the sub-patterns rather than substituting the choice's generics a second
//! time, and its `pattern_is_unanswerable` reads [`Ty::ERROR`] off a
//! sub-pattern to decide that a `match` is not worth an opinion — so the cost
//! paragraph above turned out to carry as much of the pass as the clause did.
//! **One place it does not reach**, named where it happens rather than left as
//! a surprise: a record pattern may *omit* a field, and an omitted field has no
//! node to carry a type, so a record's field types come from the declaration.
//!
//! **Method calls resolved.** [`ExprKind::MethodCall`] carries
//! `method: Option<DefId>`, and it is **filled**: [`crate::methods`] is
//! Decision 11's lookup and [`crate::check`]'s `method_call` writes what it
//! found. The slot was here before the lookup was, on the argument that the
//! node's *shape* is a commitment — *"a checker that acquires the lookup fills
//! a field rather than changing an IR"* — and that is what happened, which is
//! why this paragraph is the only thing in this file that changed.
//!
//! **`None` still occurs and still means one thing**: the receiver is a type
//! this crate holds no implementations for. The prelude registers no methods at
//! all — `builtins.rs` says so in as many words — so `"a".length()` resolves to
//! nothing, and a type parameter's bound is not searched
//! (`methods`'s §5). A consumer of THIR must handle it, and the honest
//! handling is the one this crate gives the node: the call's type is
//! [`Ty::ERROR`] and nothing downstream may assume more.
//!
//! **Implicit conversions explicit.** [`ExprKind::Coerce`] carries the
//! [`Coercion`] [`crate::assign`] returned, and [`ExprKind::Narrow`] carries
//! the other direction. Both are nodes rather than flags on the value node,
//! because both *change the representation*: a `Widen` adds a niche or a
//! discriminant, a `Box` makes two words out of however many there were, and a
//! `Narrow` takes the discriminant away again. A lowering that reads a flag has
//! to remember to look; a lowering that walks nodes cannot forget one.
//!
//! # 2. Indices, not pointers — and why here too
//!
//! Decision 24 is quoted twice in this crate already (`ty`'s §2, `infer`'s §4)
//! and it holds here for a third reason: **`region-inference.md` §10 item 6
//! wants stable point identity**, and a tree of `Box`es has no identity at all
//! to be stable. An [`ExprId`] is an index into one body's arena, assigned in
//! the order the checker visits the body and in no other order, so it is
//! stable under an edit to a *different* function — which is the half of item 6
//! that can be guaranteed before MIR exists. §5 says what the other half is.
//!
//! **What it costs** is that a [`Body`] is needed to read any node, so every
//! traversal threads one, and a node cannot be moved between bodies. The
//! second is a feature: a `ExprId` from another body is `ty`'s §1 cost one
//! level down, and there is no cross-body traversal in this phase to commit it.
//!
//! # 3. A place is a local and a chain of fields, and no more
//!
//! [`Place`] is the one thing in this file that is not a copy of the HIR's
//! shape. Decision 7 makes narrowing *"a flow-sensitive fact about a place"*,
//! and `region-inference.md` §10 item 2 requires that **two expressions
//! denoting the same place produce the same place**. This type is that
//! requirement met for the fragment THIR can meet it for: a root [`DefId`] and
//! a vector of field [`DefId`]s, compared and hashed structurally, so
//! `config.port` written twice is one key in the narrowing table.
//!
//! **What is deliberately not a place:** an index (`a[i]`), and a dereference.
//! An index place needs the *value* of the index, which is a MIR temporary and
//! not a name; two `a[i]` in a row are the same place only if `i` did not
//! change, and deciding that is the analysis MIR exists to make possible.
//! `a[i]?` therefore narrows nothing, which is a real restriction and is the
//! honest one: the alternative is a place that compares equal when the values
//! behind it differ, and that is unsoundness in the direction §4.2 says the
//! meet must never go.
//!
//! [`Place`] sees through [`ExprKind::Narrow`] and **not** through
//! [`ExprKind::Coerce`]. A narrowed read is the same storage seen at a smaller
//! type; a coerced value is a new value in a new representation, and treating
//! the two alike would let a write through the original invalidate a fact about
//! something that is no longer there — or, worse, fail to.
//!
//! # 4. What this file is not
//!
//! - **Not a lowering to MIR.** No places beyond §3, no temporaries, no blocks
//!   with successors, no `StorageLive`. §5.
//! - **Not a second definition table.** A binding is still a [`DefId`]; what
//!   this body adds is the *type* it was given, in [`Body::local_ty`]. A name,
//!   a span and a parent stay one lookup away in the `DefTable`, which is the
//!   rule the HIR exists to impose.
//! - **Not an evaluator.** [`ExprKind::Literal`] keeps the literal the parser
//!   read, unsuffixed and unwidened, because the value a `1` denotes is a fact
//!   about its type and the type is on the node beside it.
//!
//! # 5. The seam, for the phase that starts at it
//!
//! `lib.rs`'s §5 stated the seam this layer started at; this is the next one,
//! and it is addressed to MIR. `region-inference.md` §10 lists six things it
//! needs, `type-checking-and-mir.md` §12 promises all six on MIR's behalf, and
//! **THIR can guarantee one and a half of them**. Stating which is the point of
//! this section, because a promise inherited from a note is a promise nobody
//! checked.
//!
//! | §10 | THIR | Why |
//! |---|---|---|
//! | 1. An explicit CFG with `(block, statement)` points | **No** | Decision 3 is the refusal: this tree is expression-shaped so that a diagnostic can quote it. `a and b` is one node here and must be. |
//! | 2. Explicit places, equal expressions giving equal places | **For locals and field projections** | [`Place`], §3. Not for an index or a dereference, and §3 says why that cannot be fixed here. |
//! | 3. Explicit `StorageLive`/`StorageDead` | **No** | There is no storage statement in a tree of expressions. What THIR supplies is the *scope*: every binding is introduced by a [`StmtKind::Let`] in a known [`Block`], so the point MIR emits `StorageDead` at is derivable without a second analysis. |
//! | 4. Two-phase borrows exposed | **No** | Reservation and activation are two MIR points and there is one node here. THIR supplies the *set*: every exclusive borrow is an [`ExprKind::Borrow`] with `mutable: true` over a [`Place`], which is exactly the set [`crate::narrow`] already keys Decision 8's invalidation on — so MIR inherits the reservation points rather than rediscovering them. |
//! | 5. Drop points explicit *and elaborated* | **No** | Decision 26's drop flags need a move analysis, and there is none in this crate. Nothing here forecloses it. |
//! | 6. Stable point identity across the query boundary | **Half** | §2: [`ExprId`]s are per-body and deterministic, so editing one function does not renumber another. MIR point identity is MIR's, and the half THIR owes is the half it can keep. |
//!
//! **And one thing this phase hands MIR that §10 did not ask for**: the
//! narrowing has already been applied, as [`ExprKind::Narrow`] nodes. MIR does
//! not recompute it and must not — Decision 25 is explicit that narrowing's
//! *conclusion* is consumed after region inference runs, and a MIR pass that
//! re-derived it would be consuming it before. [`crate::narrow`]'s §4 is the
//! whole of that argument and it is a phase-ordering argument, not a proof.

use science_diagnostics::Span;
use science_resolve::hir::{BinaryOp, DefId, Literal, UnaryOp};

use crate::assign::Coercion;
use crate::ty::Ty;

/// An expression in one [`Body`]'s arena. §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExprId(u32);

impl ExprId {
    /// The arena index, for a dump and for a side table keyed on nodes.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A pattern in one [`Body`]'s arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PatId(u32);

impl PatId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A block in one [`Body`]'s arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId(u32);

impl BlockId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A typed expression. Decision 3's first clause is the `ty` field.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Ty,
    pub span: Span,
}

/// What a typed expression is.
///
/// The shape is the HIR's, node for node, with three differences and they are
/// Decision 3's three clauses: a name that resolved to a local is
/// [`ExprKind::Local`] rather than a path, a field is a [`DefId`] rather than
/// an [`Ident`](science_resolve::hir::Ident), and two nodes exist that the HIR
/// has no spelling for — [`ExprKind::Coerce`] and [`ExprKind::Narrow`].
/// One piece of an [`ExprKind::FString`].
#[derive(Debug, Clone, PartialEq)]
pub enum FStringPart {
    /// Literal text, escapes and doubled braces already resolved.
    Text(String),
    /// `{ expression }`. §1.6: an interpolation *borrows* its operand, so the
    /// expression here is the operand itself and the borrow is the ordinary
    /// auto-borrow the lowering applies.
    Hole(ExprId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A literal, as the parser read it. §4.
    Literal(Literal),
    /// `f"mean {μ}"`. Its type is `String` and its holes are checked.
    ///
    /// **The decision. The node survives type checking; it is not desugared
    /// here.** What this phase adds is the type of every hole and the check
    /// that the type implements `Display`.
    ///
    /// **The reason.** `strings-formatting-and-docs.md` §1.7 makes the
    /// expansion *"a builder over the fragments, with the capacity
    /// pre-computed"*, and the builder is a sequence of runtime calls —
    /// `science_string_new` and one `science_string_push_*` per part. None of
    /// those has a Science spelling, and inventing one so that this phase could
    /// desugar into a `Call` would put a name in the prelude that no note
    /// gives: `stdlib-core.md` §6.9 has no `to_string`, and §3.1's `Formatter`
    /// is the design `builtins.rs` has refused three times. So the node carries
    /// the shape to the phase that has a symbol table, which is codegen.
    ///
    /// **The cost, stated exactly.** `science-mir` cannot lower this, and says
    /// so where it meets it. Nothing below this line can print a number until
    /// the builder exists, and the builder is two edits in crates this one does
    /// not own: a MIR lowering that emits the calls, and a codegen arm that
    /// accepts a `Callee::Runtime`.
    FString(Vec<FStringPart>),
    /// A read of a local, a parameter, or a closure's subject.
    Local(DefId),
    /// `self`.
    SelfValue(DefId),
    /// A function, a constant or a unit variant, named as a value.
    Item(DefId),
    Call {
        callee: ExprId,
        args: Vec<ExprId>,
    },
    /// `receiver.method(args)`.
    ///
    /// `method` is the definition Decision 11's lookup found, and `None` where
    /// it found nothing — a receiver of prelude type, a type parameter, a type
    /// that was already wrong. §1.
    ///
    /// An *associated* function is not one of these: `Doc.blank()` has no
    /// receiver value, so it is an [`ExprKind::Call`] at the method's own
    /// definition and never a `MethodCall` with a receiver standing for a
    /// type.
    MethodCall {
        receiver: ExprId,
        method: Option<DefId>,
        args: Vec<ExprId>,
    },
    /// `base.name`, with the field resolved — which is the thing the HIR
    /// deliberately could not do, because the answer depends on the receiver's
    /// type and this is the phase that has one.
    ///
    /// `field` is `None` where the type has no such field, which is `SC0528`
    /// and is already reported. It is an [`Option`] rather than a placeholder
    /// id for the reason [`PatKind::Variant`] is: a `DefId` standing for
    /// *"there is no answer"* is a lie a later pass can dereference, and the
    /// only honest stand-in is the absence.
    Field {
        base: ExprId,
        field: Option<DefId>,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    /// `Doc(title: "a")`.
    Record {
        def: DefId,
        fields: Vec<(DefId, ExprId)>,
    },
    Tuple(Vec<ExprId>),
    Unit,
    Unary {
        op: UnaryOp,
        operand: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    /// `e as T`. The target type is this node's own `ty`.
    Cast {
        operand: ExprId,
    },
    /// `e?` — the presence test. A `Bool`, and total.
    Present(ExprId),
    Borrow {
        mutable: bool,
        operand: ExprId,
    },
    Range {
        start: ExprId,
        end: ExprId,
        inclusive: bool,
    },
    Closure {
        param: DefId,
        body: ExprId,
    },
    If {
        cond: ExprId,
        then_branch: BlockId,
        else_branch: Option<ExprId>,
    },
    Match {
        scrutinee: ExprId,
        arms: Vec<Arm>,
    },
    Loop {
        body: BlockId,
    },
    For {
        pattern: PatId,
        iter: ExprId,
        body: BlockId,
    },
    Block(BlockId),
    Unsafe(BlockId),
    /// Decision 3's third clause: an implicit conversion, made explicit.
    ///
    /// This node's `ty` is the conversion's *target*; the operand's is its
    /// source. [`Coercion::Identity`] never appears — nothing is emitted when
    /// nothing happens, because a node that means "no change" is a node every
    /// later pass has to see through.
    Coerce {
        operand: ExprId,
        coercion: Coercion,
    },
    /// The other direction: a read of a place Decision 7 narrowed.
    ///
    /// The operand's type is `T?` and this node's is `T`. It is a node and not
    /// a retyping of the read for the reason §1 gives — dropping the
    /// discriminant is a representation change — and because MIR needs the
    /// point at which the narrowing was *used*, not the point at which it was
    /// established.
    Narrow(ExprId),
    /// An expression whose type could not be found, because the mistake was
    /// already reported or because this phase cannot answer. `ty` is
    /// [`Ty::ERROR`] and `ty`'s §5 is what keeps that to one diagnostic.
    Error,
}

/// One arm of a `match`.
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pattern: PatId,
    pub body: ExprId,
    pub span: Span,
}

/// A typed pattern. §1 says why it has a type.
#[derive(Debug, Clone, PartialEq)]
pub struct Pat {
    pub kind: PatKind,
    pub ty: Ty,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatKind {
    Wildcard,
    Literal(Literal),
    Binding { mutable: bool, def: DefId },
    /// `Ok(value)`; `def` is `None` when the variant did not resolve.
    Variant { def: Option<DefId>, elems: Vec<PatId> },
    Record { def: Option<DefId>, fields: Vec<(DefId, PatId)> },
    Tuple(Vec<PatId>),
    Unit,
    Or(Vec<PatId>),
    Error,
}

/// A block: statements, and the expression it evaluates to (§4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<ExprId>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `let a, b be f()`. One binding, or several for revision 2 §3.1's pair.
    ///
    /// The bindings are `DefId`s and their types are in [`Body::local_ty`];
    /// `value` is the whole initialiser, not one per binding, because the
    /// tuple is evaluated once.
    Let { bindings: Vec<DefId>, value: ExprId },
    Expr(ExprId),
    Assign { target: ExprId, value: ExprId },
    Return(Option<ExprId>),
    Break(Option<ExprId>),
    Continue,
    /// A statement the resolver could not lower.
    Error,
}

/// A place: a local, and a chain of field projections from it. §3.
///
/// Ordered as well as hashed, so a diagnostic that lists places lists them the
/// same way twice — the property `DefId`'s own documentation calls out for the
/// snapshots.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Place {
    /// The local, parameter or `self` the place is rooted at.
    pub root: DefId,
    /// Field projections, outermost last: `config.port` is `[port]`.
    pub fields: Vec<DefId>,
}

impl Place {
    /// The place that is a whole local.
    pub fn local(root: DefId) -> Place {
        Place { root, fields: Vec::new() }
    }

    /// This place with one more field projected off it.
    pub fn field(&self, field: DefId) -> Place {
        let mut fields = self.fields.clone();
        fields.push(field);
        Place { root: self.root, fields }
    }

    /// Whether `self` is `other`, or reachable from it by projection.
    ///
    /// This is the predicate Decision 7's invalidation is written in: *"writing
    /// to `config` invalidates it; writing to `config.other` does not"*. A
    /// write to `config` is a write to a prefix of `config.port`, so the fact
    /// about `config.port` goes; a write to `config.other` is neither a prefix
    /// nor an extension, so it stays.
    pub fn starts_with(&self, other: &Place) -> bool {
        self.root == other.root
            && self.fields.len() >= other.fields.len()
            && self.fields[..other.fields.len()] == other.fields[..]
    }
}

/// One function body, checked.
///
/// Arena-allocated (§2) and self-contained: every [`ExprId`], [`PatId`] and
/// [`BlockId`] in it indexes this body and no other.
#[derive(Debug, Clone)]
pub struct Body {
    pub(crate) def: DefId,
    pub(crate) exprs: Vec<Expr>,
    pub(crate) pats: Vec<Pat>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) root: BlockId,
    /// Every binding the body introduces — parameters, `self`, `let`s, pattern
    /// bindings — with the type it was given.
    ///
    /// A `Vec` and not a map: a body's bindings are few, the lookup is a scan,
    /// and the iteration order is declaration order, which is what a
    /// diagnostic that walks them wants.
    pub(crate) locals: Vec<(DefId, Ty)>,
    pub(crate) ret: Ty,
    /// Whether the end of the body is unreachable — every path leaves through a
    /// `return`, a diverging call, or a `loop` with no `break`.
    ///
    /// Recorded rather than recomputed because the checker already knew it: it
    /// is the flag that decides whether an `if` branch's narrowing survives
    /// (§4.2's third rule), and deriving it a second time over THIR would be
    /// two implementations of one question — which is `lib.rs` §1's argument
    /// about four spellings of const equality, one level down.
    pub(crate) diverges: bool,
}

impl Body {
    /// The function this is the body of.
    pub fn def(&self) -> DefId {
        self.def
    }

    /// The body's own block. Reserved first, so it is always index zero.
    pub fn root(&self) -> BlockId {
        self.root
    }

    /// The declared return type, already lowered.
    pub fn ret(&self) -> Ty {
        self.ret
    }

    pub fn expr(&self, id: ExprId) -> &Expr {
        &self.exprs[id.index()]
    }

    pub fn pat(&self, id: PatId) -> &Pat {
        &self.pats[id.index()]
    }

    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id.index()]
    }

    /// The type of a node, which is the whole of Decision 3's first clause.
    pub fn ty(&self, id: ExprId) -> Ty {
        self.exprs[id.index()].ty
    }

    /// How many expression nodes the body has.
    pub fn len(&self) -> usize {
        self.exprs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exprs.is_empty()
    }

    /// Every node, in the order the checker visited them. §2.
    pub fn exprs(&self) -> impl Iterator<Item = (ExprId, &Expr)> {
        self.exprs.iter().enumerate().map(|(at, expr)| (ExprId(at as u32), expr))
    }

    /// Every binding the body introduced, in declaration order.
    pub fn locals(&self) -> impl Iterator<Item = (DefId, Ty)> + '_ {
        self.locals.iter().copied()
    }

    /// Every block, in the order they were reserved.
    ///
    /// The arena is flat and every block in it belongs to this body, so a pass
    /// that wants all the statements — [`crate::unchecked`] does — iterates
    /// this rather than walking the tree looking for blocks it has an arm for.
    /// A block reachable only through a node kind the walk forgot is the bug
    /// this abolishes.
    pub fn blocks(&self) -> impl Iterator<Item = (BlockId, &Block)> {
        self.blocks.iter().enumerate().map(|(at, block)| (BlockId(at as u32), block))
    }

    /// Whether the end of the body is unreachable.
    pub fn diverges(&self) -> bool {
        self.diverges
    }

    /// The type a binding was given, if this body introduced it.
    pub fn local_ty(&self, def: DefId) -> Option<Ty> {
        self.locals.iter().find(|(id, _)| *id == def).map(|(_, ty)| *ty)
    }

    /// The place this expression denotes, when it denotes one. §3.
    ///
    /// `None` for anything that is not a name or a field of one — a call, a
    /// literal, an index, a coerced value. That is the whole of §3's
    /// restriction, expressed once here rather than at each caller.
    pub fn place_of(&self, id: ExprId) -> Option<Place> {
        match &self.expr(id).kind {
            ExprKind::Local(def) | ExprKind::SelfValue(def) => Some(Place::local(*def)),
            // A field that did not resolve denotes no place: there is nothing
            // for two spellings of it to agree on.
            ExprKind::Field { base, field } => Some(self.place_of(*base)?.field((*field)?)),
            // A narrowed read is the same storage at a smaller type. A coerced
            // value is a different value. §3.
            ExprKind::Narrow(operand) => self.place_of(*operand),
            _ => None,
        }
    }

    // --- construction, for the checker -----------------------------------

    pub(crate) fn new(def: DefId, ret: Ty) -> Body {
        Body {
            def,
            exprs: Vec::new(),
            pats: Vec::new(),
            blocks: Vec::new(),
            root: BlockId(0),
            locals: Vec::new(),
            ret,
            diverges: false,
        }
    }

    pub(crate) fn push_expr(&mut self, kind: ExprKind, ty: Ty, span: Span) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(Expr { kind, ty, span });
        id
    }

    pub(crate) fn push_pat(&mut self, kind: PatKind, ty: Ty, span: Span) -> PatId {
        let id = PatId(self.pats.len() as u32);
        self.pats.push(Pat { kind, ty, span });
        id
    }

    /// Takes a block's index before the block exists.
    ///
    /// The body's own block is reserved before anything in it is visited, so
    /// [`Body::root`] can be `BlockId(0)` and be right — a block filled in
    /// afterwards is the only way to have the id and the contents both, given
    /// that the contents mention no block at all.
    pub(crate) fn reserve_block(&mut self, span: Span) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(Block { stmts: Vec::new(), tail: None, span });
        id
    }

    pub(crate) fn fill_block(&mut self, id: BlockId, block: Block) {
        self.blocks[id.index()] = block;
    }

    pub(crate) fn declare_local(&mut self, def: DefId, ty: Ty) {
        self.locals.push((def, ty));
    }

    /// Rewrites a node's type, for the writeback of [`crate::check`]'s §4.
    pub(crate) fn set_ty(&mut self, id: ExprId, ty: Ty) {
        self.exprs[id.index()].ty = ty;
    }

    pub(crate) fn set_local_ty(&mut self, def: DefId, ty: Ty) {
        if let Some(slot) = self.locals.iter_mut().find(|(id, _)| *id == def) {
            slot.1 = ty;
        }
    }
}
