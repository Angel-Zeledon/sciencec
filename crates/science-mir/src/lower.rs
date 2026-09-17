//! THIR to MIR: the flattening, and every decision it had to take.
//!
//! # 1. Destination passing, and why not a value-returning walk
//!
//! **Decision. Every expression is lowered *into* a place the caller allocated,
//! and the walk returns the block control reached rather than a value.**
//!
//! The reason is the one thing a value-returning walk cannot do: an `if` and a
//! `match` have two blocks that both produce the answer, and a walk that
//! returns an [`Operand`] has to invent a merge — which is a φ, which is SSA,
//! which §3.3 of `type-checking-and-mir.md` refuses in as many words
//! (*"No SSA. LLVM does that, and doing it twice buys nothing"*). With a
//! destination, both arms store to the same place and there is nothing to
//! merge.
//!
//! **What it costs** is one temporary per non-trivial sub-expression and a
//! `dest: Place` parameter threaded through forty match arms. The temporaries
//! are what Decision 4 asked for anyway — *"over explicit places and
//! temporaries"* — so the cost is paid to the thing that wanted it.
//!
//! # 2. Scopes, and how `StorageDead` is derived rather than invented
//!
//! THIR's §5 promised MIR the material for §10 item 3 and not the item:
//! *"every binding is introduced by a `StmtKind::Let` in a known `Block`, so
//! the point MIR emits `StorageDead` at is derivable without a second
//! analysis"*. This is that derivation.
//!
//! **Decision. The lowering keeps a stack of scopes; a local is registered in
//! the innermost one when it is created; leaving a scope emits, for every local
//! it holds and in reverse order of creation, a drop and then a
//! `StorageDead`.** A `return` leaves every scope, a `break` leaves every scope
//! down to the loop's, a `continue` the same.
//!
//! Scopes are pushed for a THIR block, for each statement inside one, and for
//! each arm of a branch — an `if` branch, a `match` arm, the right-hand side of
//! an `and`. The last of those is the one that is easy to forget and the one
//! that matters: a temporary created on the path that was not taken must not be
//! `StorageDead`-ed at the join, because its `StorageLive` never ran.
//!
//! **Three things get no storage statements, and each for its own reason.**
//! The return place is live for the whole body and is not storage a borrow can
//! outlive. A parameter is live from entry — there is no point before the body
//! at which to say `StorageLive` — so it gets a `StorageDead` at every exit and
//! no `StorageLive`. A drop flag is compiler storage with no user meaning and
//! no borrow can name it.
//!
//! **The cost of leaving by every exit** is that the drop-and-storage sequence
//! is duplicated once per path out of a scope. A `return` inside two nested
//! `if`s emits it three times. rustc shares them with a drop tree; this does
//! not, because a shared drop tree is a second graph whose blocks the region
//! engine would have to be told are not ordinary control flow.
//!
//! # 3. `a and b` becomes blocks and edges — §10 item 1, concretely
//!
//! Decision 3 made `a and b` one THIR node *"so that a diagnostic can quote
//! it"*, and named that as the reason THIR cannot supply a CFG. Here it is four
//! blocks: evaluate the left, branch, evaluate the right on one side, store the
//! constant on the other, join. [`Rvalue::Binary`] has no `And` and no `Or` arm
//! reachable, and `tests/cfg.rs` asserts it.
//!
//! # 4. Auto-dereference, which the user never wrote
//!
//! Science has no dereference operator. A field or an index taken through a
//! `borrowed T` therefore has a [`Projection::Deref`] that appears in no
//! source text, inserted here by looking at the base's *revealed* type.
//!
//! Revealed, not written: [`science_types::alias::Aliases::reveal`] is called
//! before the base's type is inspected, for `lib.rs` §5's reason one level up —
//! *"`Embedding` failing to match `Array of F32` at one site in ten, wherever
//! the author happened to write the alias"*. A place built from the written
//! type at one site and the revealed type at another would be two places, and
//! §10 item 2 would be false.
//!
//! # 5. `Copy` or `Move`, and the direction the doubt goes
//!
//! **Decision. An operand is `Copy` only where the type is one this phase can
//! prove trivially copyable; everything else is `Move`.** The proof available
//! is the prelude's: the numeric primitives, `Bool`, `Char`, a shared borrow,
//! unit, and tuples and nullables built out of those.
//!
//! The asymmetry is the point. Calling a move a copy hides a use-after-move,
//! which is `SC0301` not reported — a soundness hole. Calling a copy a move
//! makes the move analysis think an `I64` was consumed, which costs at worst a
//! drop flag on a local that does not need dropping, and [`crate::moves`] then
//! declines to flag it because its type needs no drop at all. One direction
//! costs a byte that is then not spent; the other costs the language's claim.
//!
//! **The exception, and it is deliberate.** Every argument of a
//! [`Callee::Unresolved`] call is `Copy`, whatever its type. A call whose
//! signature is unknown has unknown argument passing, and marking the arguments
//! moved would make Decision 11's absent method lookup *manufacture*
//! use-after-move errors in ordinary code. That is `ty`'s §5 discipline —
//! *"a hole costs nothing downstream and, in particular, cannot manufacture a
//! cascade"* — applied to operands. The cost is stated at [`Unresolved`]: a
//! genuine move through a method call is invisible until the lookup lands.
//!
//! # 6. Two-phase borrows — §10 item 4, answered rather than deferred
//!
//! The note asks for *"two-phase borrows, or an explicit statement of their
//! absence"*, and §14 calls them *"a silent prerequisite"*. They are built.
//!
//! **Decision. An exclusive borrow taken in argument position is two-phase: it
//! is reserved at the `Ref` and activated by an explicit
//! [`StatementKind::Activate`] immediately before the call that consumes it.
//! Every other exclusive borrow is single-phase.**
//!
//! *Why argument position is the right set.* THIR's §5 hands over *"every
//! exclusive borrow is an `ExprKind::Borrow` with `mutable: true` over a
//! `Place`"*, and `check`'s `auto_borrow` only ever inserts one at
//! `Site::Argument`. So the borrows that need two phases — the ones created,
//! carried past the evaluation of later arguments, and used exactly once at the
//! call — are exactly the ones in argument position, and they are
//! syntactically identifiable here without an analysis. A borrow bound to a
//! name (`let r be mutable borrowed x`) is used an unknown number of times, and
//! two-phasing it would be unsound.
//!
//! *Why the activation is a statement.* rustc derives it: the activation is the
//! unique later use of the borrow temporary. Deriving it is an analysis that
//! must be right for `v.push(v.len())` to compile at all, and §10 item 4 asks
//! MIR to *expose* the reservation point. Exposing one point and hiding the
//! other would have met the letter. The cost is one statement per two-phase
//! borrow, which codegen treats as a `Nop`.
//!
//! **What MIR does not decide, and says so.** The *rule* — that between
//! reservation and activation the place may be read but not written or
//! exclusively reborrowed — is region inference's, and is stated in
//! `lib.rs`'s §5 as an obligation rather than implemented here. MIR
//! supplies the two points; what may happen between them is a question about
//! regions.
//!
//! **Where the acceptance case is today.** `v.push(v.len())` needs the receiver
//! to be auto-borrowed, which needs Decision 11's method lookup. This lowering
//! reads `MethodCall::method` and takes the borrow when the method resolves and
//! its receiver is `mutable self`; where it does not resolve, there is no
//! borrow to reserve and the case is not exercised. That is a finding about the
//! phase above, not a gap here, and `lib.rs`'s §7 records it.
//!
//! # 7. `for`, whose callee does not exist
//!
//! `Iterate` is not declared — `science-types`'s `check`'s §6 says a `for`
//! binds its pattern at `Ty::ERROR` — so there is no `next()` to call.
//!
//! **Decision. A `for` is lowered to the CFG shape a `for` has, with the
//! element-producing call marked [`Unresolved::IterateNext`].** A header block,
//! a presence test, a body, a back edge, an exit.
//!
//! *The alternative was to refuse*, and refusing would mean
//! `examples/21_compiler_shapes.science` — the acceptance case this whole note
//! exists for — does not lower, because `Scopes.lookup` and `walk` are both
//! `for` loops. A CFG with a named hole in it is worth more to the phase that
//! consumes this than no CFG.
//!
//! *The cost* is that if Decision 15's `TryIterate` lands with a failure edge,
//! this shape acquires a third successor and the lowering changes. It is
//! written as one function, `Builder::lower_for`, so that the change is one
//! place.
//!
//! # 8. Closures, refused by name
//!
//! **Decision. A closure's body is not lowered, and its captures are therefore
//! not borrows MIR can see.** [`Rvalue::Closure`] keeps the THIR
//! [`science_types::thir::ExprId`] of the body — the one place in this crate
//! where an id from another IR survives — and nothing walks it.
//!
//! The reason is that a capture discipline is a *language* decision:
//! `collections-and-chains.md` §1.2 owns closures, and what `each.title`
//! captures — the subject by borrow, by move, by field — is that note's to
//! decide. Inventing one here would put the answer in the IR, where the note
//! that owns it could not change it without changing MIR.
//!
//! **The cost is real and is the largest hole in this crate.** A program that
//! borrows through a closure is not checked by whatever runs on this. `lib.rs`
//! §7 lists it first among the findings for that reason.

use std::collections::HashMap;

use science_diagnostics::Span;
use science_resolve::hir::{BinaryOp, DefId, DefKind, DefTable, Literal, SelfKind};
use science_types::alias::Aliases;
use science_types::items::Declarations;
use science_types::thir::{self, Arm, ExprId, ExprKind, PatId, PatKind, StmtKind};
use science_types::ty::{Ty, TyKind, Types};
use science_types::Substitution;

use crate::mir::{
    predecessors_of, BasicBlock, BlockId, Body, BorrowData, BorrowId, BorrowKind, Callee, Constant,
    Local, LocalDecl, LocalKind, Operand, Place, Point, Projection, Rvalue, Statement,
    StatementKind, Terminator, TerminatorKind, Unresolved, ENTRY_BLOCK, RETURN_PLACE,
};

/// Everything the lowering reads that is not the body itself.
///
/// A struct rather than four parameters, because all four are threaded through
/// every function in this file and a fifth would otherwise be a signature
/// change in forty places.
pub struct Context<'a> {
    pub defs: &'a DefTable,
    pub decls: &'a Declarations,
    pub types: &'a mut Types,
    pub aliases: &'a mut Aliases,
}

/// Lowers every checked body, in the order the checker produced them.
///
/// The order is the checker's and not this crate's, deliberately: §10 item 6
/// wants point identity stable under an edit elsewhere, and a numbering that
/// depended on how many bodies came first would not be.
pub fn lower_crate(context: &mut Context<'_>, bodies: &[thir::Body]) -> Vec<Body> {
    bodies.iter().map(|body| lower_body(context, body)).collect()
}

/// Lowers one body: construction, then drop elaboration, then the borrow index.
///
/// **The order is Decision 26's**: *"elaboration runs during MIR construction,
/// before region inference"*. It is inside this function rather than exposed as
/// a pass a caller could forget, because a body handed out before elaboration
/// has drops that are wrong on one path, and no caller has a use for one.
///
/// The borrow index is built last, after elaboration has finished inserting
/// statements, because [`BorrowData::reserved`] is a [`Point`] and a point is
/// only meaningful once the statement numbering has stopped moving.
pub fn lower_body(context: &mut Context<'_>, thir: &thir::Body) -> Body {
    let flag_ty = context
        .decls
        .prelude()
        .ty(context.types, "Bool")
        .unwrap_or(Ty::ERROR);
    let mut builder = Builder::new(context, thir);
    builder.run();
    let mut body = builder.finish();
    crate::drops::elaborate(&mut body, flag_ty);
    index_borrows(&mut body);
    body.predecessors = predecessors_of(&body.blocks);
    body
}

/// Fills in every borrow's reservation and activation point by one scan.
///
/// A scan rather than bookkeeping during construction, because construction and
/// then elaboration both insert statements, and a point recorded before either
/// would name a different statement afterwards.
fn index_borrows(body: &mut Body) {
    let mut reserved: HashMap<BorrowId, Point> = HashMap::new();
    let mut activation: HashMap<BorrowId, Point> = HashMap::new();
    for (at, block) in body.blocks.iter().enumerate() {
        let block_id = BlockId::from_index(at);
        for (index, statement) in block.statements.iter().enumerate() {
            let point = Point { block: block_id, statement: index as u32 };
            match &statement.kind {
                StatementKind::Assign { rvalue: Rvalue::Ref { borrow, .. }, .. } => {
                    reserved.insert(*borrow, point);
                }
                StatementKind::Activate(borrow) => {
                    activation.insert(*borrow, point);
                }
                _ => {}
            }
        }
    }
    for data in &mut body.borrows {
        if let Some(point) = reserved.get(&data.id) {
            data.reserved = *point;
        }
        data.activation = activation.get(&data.id).copied();
    }
}

/// A scope: the locals whose storage it owns, in creation order.
struct Scope {
    locals: Vec<Local>,
}

/// One enclosing loop, for `break` and `continue`.
struct LoopScope {
    /// Where `continue` goes.
    head: BlockId,
    /// Where `break` goes.
    exit: BlockId,
    /// How deep the scope stack was when the loop was entered: a `break` leaves
    /// every scope above this.
    depth: usize,
}

struct Builder<'a, 'ctx> {
    context: &'a mut Context<'ctx>,
    thir: &'a thir::Body,
    locals: Vec<LocalDecl>,
    blocks: Vec<BasicBlock>,
    borrows: Vec<BorrowData>,
    bindings: HashMap<DefId, Local>,
    scopes: Vec<Scope>,
    loops: Vec<LoopScope>,
    arg_count: usize,
    span: Span,
    /// `Bool`, for the temporaries a branch condition is read out of, or
    /// [`Ty::ERROR`] in a compilation with no prelude to find it in.
    ///
    /// A discriminant temporary is **not** given this type: which variant a
    /// value holds has no Science type at all, and [`Ty::ERROR`] is the honest
    /// answer rather than a `Bool` it is not.
    bool_ty: Ty,
}

impl<'a, 'ctx> Builder<'a, 'ctx> {
    fn new(context: &'a mut Context<'ctx>, thir: &'a thir::Body) -> Builder<'a, 'ctx> {
        let span = thir.block(thir.root()).span;
        let bool_ty = context.decls.prelude().ty(context.types, "Bool").unwrap_or(Ty::ERROR);
        Builder {
            context,
            thir,
            locals: Vec::new(),
            blocks: Vec::new(),
            borrows: Vec::new(),
            bindings: HashMap::new(),
            scopes: Vec::new(),
            loops: Vec::new(),
            arg_count: 0,
            span,
            bool_ty,
        }
    }

    fn finish(self) -> Body {
        Body {
            def: self.thir.def(),
            locals: self.locals,
            blocks: self.blocks,
            borrows: self.borrows,
            arg_count: self.arg_count,
            span: self.span,
            predecessors: Vec::new(),
        }
    }

    // --- the body ---------------------------------------------------------

    fn run(&mut self) {
        // `_0` first, so that [`RETURN_PLACE`] is index zero and is right.
        let ret = self.thir.ret();
        let span = self.span;
        self.push_local(ret, LocalKind::Return, span);

        // The parameters, in the order the signature declared them, with the
        // receiver first. Reading them off the signature rather than off the
        // body's local list is what makes [`Body::params`] a dense prefix.
        let mut params: Vec<DefId> = Vec::new();
        if let Some(signature) = self.context.decls.signature(self.thir.def()) {
            if let Some((receiver, _)) = signature.self_param {
                params.push(receiver);
            }
            params.extend(signature.params.iter().map(|param| param.def));
        }
        let mut param_locals = Vec::new();
        for def in &params {
            let ty = self.thir.local_ty(*def).unwrap_or(Ty::ERROR);
            let span = self.context.defs.get(*def).span;
            let local = self.push_local(ty, LocalKind::Param(*def), span);
            self.bindings.insert(*def, local);
            param_locals.push(local);
        }
        self.arg_count = params.len();

        let entry = self.new_block();
        debug_assert_eq!(entry, ENTRY_BLOCK);

        // The body's own scope holds the parameters, so that they are dropped
        // and storage-dead at every exit. §2.
        self.scopes.push(Scope { locals: param_locals });

        let root = self.thir.root();
        let block = self.lower_block(Place::local(RETURN_PLACE), root, entry);
        let block = self.exit_scopes(0, block, span);
        self.scopes.pop();
        self.terminate(block, TerminatorKind::Return, span);
    }

    // --- locals, blocks, scopes -------------------------------------------

    fn push_local(&mut self, ty: Ty, kind: LocalKind, span: Span) -> Local {
        let local = Local::from_index(self.locals.len());
        self.locals.push(LocalDecl { ty, kind, span });
        local
    }

    /// A temporary, registered in the innermost scope and storage-live now.
    fn temp(&mut self, ty: Ty, span: Span, block: BlockId) -> Local {
        let local = self.push_local(ty, LocalKind::Temp, span);
        self.register(local);
        self.push_statement(block, StatementKind::StorageLive(local), span);
        local
    }

    fn register(&mut self, local: Local) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.locals.push(local);
        }
    }

    /// Registers a local in a named scope rather than the innermost one.
    ///
    /// A `let` binding outlives the statement that introduced it — it is
    /// readable by every later statement in the same block — so it belongs to
    /// the *block's* scope while the initialiser's temporaries belong to the
    /// statement's. Registering both in the innermost scope would make
    /// `let a be 1` emit `StorageDead(a)` on the next line, and every read of
    /// `a` after it would be a read of dead storage — which is §10 item 3
    /// answered with the wrong point.
    fn register_at(&mut self, depth: usize, local: Local) {
        match self.scopes.get_mut(depth) {
            Some(scope) => scope.locals.push(local),
            None => self.register(local),
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId::from_index(self.blocks.len());
        let span = self.span;
        self.blocks.push(BasicBlock {
            statements: Vec::new(),
            // Patched by `terminate`. A block still holding this at the end of
            // the lowering is genuinely unreachable, so an un-patched block is
            // not a silent bug.
            terminator: Terminator { kind: TerminatorKind::Unreachable, span },
        });
        id
    }

    fn push_statement(&mut self, block: BlockId, kind: StatementKind, span: Span) {
        self.blocks[block.index()].statements.push(Statement { kind, span });
    }

    fn terminate(&mut self, block: BlockId, kind: TerminatorKind, span: Span) {
        self.blocks[block.index()].terminator = Terminator { kind, span };
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope { locals: Vec::new() });
    }

    /// Leaves and discards the innermost scope.
    fn pop_scope(&mut self, block: BlockId, span: Span) -> BlockId {
        let depth = self.scopes.len() - 1;
        let block = self.exit_scopes(depth, block, span);
        self.scopes.pop();
        block
    }

    /// Emits the drops and `StorageDead`s for every scope at or above `depth`,
    /// innermost first, without popping any of them. §2.
    fn exit_scopes(&mut self, depth: usize, mut block: BlockId, span: Span) -> BlockId {
        for level in (depth..self.scopes.len()).rev() {
            let locals: Vec<Local> = self.scopes[level].locals.iter().rev().copied().collect();
            for local in locals {
                block = self.emit_scope_exit(local, block, span);
            }
        }
        block
    }

    fn emit_scope_exit(&mut self, local: Local, block: BlockId, span: Span) -> BlockId {
        let ty = self.locals[local.index()].ty;
        let mut block = block;
        // The drop is emitted unconditionally and then *elaborated*:
        // `crate::drops` decides whether it stands, is deleted, or acquires
        // Decision 26's flag. Deciding here would need the move analysis, which
        // needs the finished CFG.
        let drops = crate::moves::needs_drop(
            self.context.decls,
            self.context.types,
            self.context.aliases,
            ty,
        );
        if drops {
            let next = self.new_block();
            self.terminate(
                block,
                TerminatorKind::Drop { place: Place::local(local), flag: None, target: next },
                span,
            );
            block = next;
        }
        self.push_statement(block, StatementKind::StorageDead(local), span);
        block
    }

    // --- blocks and statements --------------------------------------------

    fn lower_block(&mut self, dest: Place, block_id: thir::BlockId, mut block: BlockId) -> BlockId {
        let thir = self.thir;
        let thir_block = thir.block(block_id);
        self.push_scope();
        for statement in &thir_block.stmts {
            block = self.lower_stmt(statement, block);
        }
        match thir_block.tail {
            Some(tail) => block = self.expr_into(dest, tail, block),
            None => {
                self.assign(
                    block,
                    dest,
                    Rvalue::Use(Operand::Const(Constant::Unit)),
                    thir_block.span,
                );
            }
        }
        self.pop_scope(block, thir_block.span)
    }

    fn lower_stmt(&mut self, statement: &thir::Stmt, mut block: BlockId) -> BlockId {
        let span = statement.span;
        // A scope per statement: a temporary built for this statement dies with
        // it, which is what makes `let r be borrowed f()` a rule-5 question
        // rather than a silently extended lifetime. §2.
        self.push_scope();
        match &statement.kind {
            StmtKind::Let { bindings, value } => {
                block = self.lower_let(bindings, *value, block, span);
            }
            StmtKind::Expr(expr) => {
                let ty = self.thir.ty(*expr);
                let temp = self.temp(ty, span, block);
                block = self.expr_into(Place::local(temp), *expr, block);
            }
            StmtKind::Assign { target, value } => {
                let (place, next) = match self.as_place(*target, block) {
                    Some(found) => found,
                    // The resolver and the checker both report an assignment to
                    // something that is not a place; a third spelling of one
                    // mistake would be a third diagnostic. The value is still
                    // evaluated, into a temporary, so that a mistake in it is
                    // not hidden behind a mistake in the target.
                    None => {
                        let ty = self.thir.ty(*target);
                        let temp = self.temp(ty, span, block);
                        (Place::local(temp), block)
                    }
                };
                block = self.expr_into(place, *value, next);
            }
            StmtKind::Return(value) => {
                block = match value {
                    Some(value) => self.expr_into(Place::local(RETURN_PLACE), *value, block),
                    None => {
                        self.assign(
                            block,
                            Place::local(RETURN_PLACE),
                            Rvalue::Use(Operand::Const(Constant::Unit)),
                            span,
                        );
                        block
                    }
                };
                let block = self.pop_scope(block, span);
                let block = self.exit_scopes(0, block, span);
                self.terminate(block, TerminatorKind::Return, span);
                // Everything after a `return` in the same THIR block is
                // unreachable, and is lowered into a block nothing jumps to
                // rather than skipped: skipping would make the lowering depend
                // on a reachability judgement it has no reason to make.
                return self.new_block();
            }
            StmtKind::Break(value) => {
                if let Some(value) = value {
                    // `check`'s §6: *"`break e` is checked and its type
                    // discarded; a `loop` is `Ty::UNIT`"*. The value is still
                    // evaluated — it can have effects — and then discarded, so
                    // that MIR agrees with the type the checker gave the loop.
                    let ty = self.thir.ty(*value);
                    let temp = self.temp(ty, span, block);
                    block = self.expr_into(Place::local(temp), *value, block);
                }
                let Some(target) = self.loops.last() else {
                    // A `break` outside a loop, which the parser rejects.
                    return self.pop_scope(block, span);
                };
                let (exit, depth) = (target.exit, target.depth);
                let block = self.pop_scope(block, span);
                let block = self.exit_scopes(depth, block, span);
                self.terminate(block, TerminatorKind::Goto { target: exit }, span);
                return self.new_block();
            }
            StmtKind::Continue => {
                let Some(target) = self.loops.last() else {
                    return self.pop_scope(block, span);
                };
                let (head, depth) = (target.head, target.depth);
                let block = self.pop_scope(block, span);
                let block = self.exit_scopes(depth, block, span);
                self.terminate(block, TerminatorKind::Goto { target: head }, span);
                return self.new_block();
            }
            // A statement the resolver could not lower. Nothing is emitted and
            // nothing is reported: `ty`'s §5 discipline, one level down.
            StmtKind::Error => {}
        }
        self.pop_scope(block, span)
    }

    /// `let a, b be f()`. One initialiser, however many bindings.
    fn lower_let(
        &mut self,
        bindings: &[DefId],
        value: ExprId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        // The block's scope, not the statement's: `register_at` says why.
        let scope = self.scopes.len().saturating_sub(2);
        if bindings.len() == 1 {
            let local = self.declare_binding(bindings[0], block, scope);
            return self.expr_into(Place::local(local), value, block);
        }
        // The pair of revision 2 §3.1: the tuple is evaluated once and then
        // destructured, which is why THIR keeps one `value` for `n` bindings.
        let ty = self.thir.ty(value);
        let pair = self.temp(ty, span, block);
        block = self.expr_into(Place::local(pair), value, block);
        for (index, binding) in bindings.iter().enumerate() {
            let local = self.declare_binding(*binding, block, scope);
            let element_ty = self.tuple_field_ty(ty, index);
            let source = Place::local(pair)
                .project(Projection::TupleField { index: index as u32, ty: element_ty });
            let operand = self.read(source, element_ty);
            self.assign(block, Place::local(local), Rvalue::Use(operand), span);
        }
        block
    }

    /// Introduces a binding: a local, registered in the innermost scope, and
    /// storage-live from here. §2's derivation of item 3.
    fn declare_binding(&mut self, def: DefId, block: BlockId, scope: usize) -> Local {
        if let Some(local) = self.bindings.get(&def) {
            return *local;
        }
        let ty = self.thir.local_ty(def).unwrap_or(Ty::ERROR);
        let span = self.context.defs.get(def).span;
        let local = self.push_local(ty, LocalKind::Binding(def), span);
        self.bindings.insert(def, local);
        self.register_at(scope, local);
        self.push_statement(block, StatementKind::StorageLive(local), span);
        local
    }

    // --- expressions ------------------------------------------------------

    fn expr_into(&mut self, dest: Place, expr: ExprId, mut block: BlockId) -> BlockId {
        // Copied out of `self` so that the match below borrows the THIR and not
        // the builder, and every arm can call `&mut self` freely.
        let thir = self.thir;
        let node = thir.expr(expr);
        let span = node.span;
        let ty = node.ty;
        match &node.kind {
            ExprKind::Literal(literal) => {
                let rvalue = Rvalue::Use(Operand::Const(Constant::Literal(literal.clone())));
                self.assign(block, dest, rvalue, span);
                block
            }
            ExprKind::Unit => {
                self.assign(block, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
                block
            }
            ExprKind::Local(_)
            | ExprKind::SelfValue(_)
            | ExprKind::Field { .. }
            | ExprKind::Index { .. } => match self.as_place(expr, block) {
                Some((place, block)) => {
                    let operand = self.read(place, ty);
                    self.assign(block, dest, Rvalue::Use(operand), span);
                    block
                }
                None => {
                    self.assign(block, dest, Rvalue::Error, span);
                    block
                }
            },
            ExprKind::Item(def) => {
                let rvalue = if self.context.defs.get(*def).kind == DefKind::Variant {
                    Rvalue::Variant { variant: *def, payload: Vec::new() }
                } else {
                    Rvalue::Use(Operand::Const(Constant::Item(*def)))
                };
                self.assign(block, dest, rvalue, span);
                block
            }
            ExprKind::Tuple(elements) => {
                let mut operands = Vec::with_capacity(elements.len());
                for element in elements {
                    let (operand, next) = self.operand(*element, block);
                    operands.push(operand);
                    block = next;
                }
                self.assign(block, dest, Rvalue::Tuple(operands), span);
                block
            }
            ExprKind::Record { def, fields } => {
                let mut operands = Vec::with_capacity(fields.len());
                for (field, value) in fields {
                    let (operand, next) = self.operand(*value, block);
                    operands.push((*field, operand));
                    block = next;
                }
                self.assign(block, dest, Rvalue::Record { def: *def, fields: operands }, span);
                block
            }
            ExprKind::Unary { op, operand } => {
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Unary { op: *op, operand: value }, span);
                block
            }
            // §3. The one place Decision 3's *"one node"* becomes a graph.
            ExprKind::Binary { op: op @ (BinaryOp::And | BinaryOp::Or), lhs, rhs } => {
                self.lower_short_circuit(dest, *op, *lhs, *rhs, block, span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (left, block) = self.operand(*lhs, block);
                let (right, block) = self.operand(*rhs, block);
                self.assign(block, dest, Rvalue::Binary { op: *op, lhs: left, rhs: right }, span);
                block
            }
            ExprKind::Cast { operand } => {
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Cast { operand: value, ty }, span);
                block
            }
            ExprKind::Present(operand) => {
                // `e?` is total and reads: it produces a `Bool` and leaves `e`
                // where it was. `force_copy` rather than `self.read`, because
                // §5's rule is about *consuming* reads and a presence test is
                // not one — and `if err?: return err` would otherwise move
                // `err` at the test and read it again in the branch.
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::IsPresent(force_copy(value)), span);
                block
            }
            ExprKind::Borrow { mutable, operand } => {
                self.lower_borrow(dest, *mutable, *operand, block, span, false)
            }
            ExprKind::Range { start, end, inclusive } => {
                let (low, block) = self.operand(*start, block);
                let (high, block) = self.operand(*end, block);
                let rvalue = Rvalue::Range { start: low, end: high, inclusive: *inclusive };
                self.assign(block, dest, rvalue, span);
                block
            }
            ExprKind::Coerce { operand, coercion } => {
                let coercion = *coercion;
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Coerce { operand: value, coercion, ty }, span);
                block
            }
            ExprKind::Narrow(operand) => {
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Narrow { operand: value, ty }, span);
                block
            }
            // §8. The body is not lowered.
            ExprKind::Closure { param, body } => {
                let rvalue = Rvalue::Closure { param: *param, thir_body: *body, ty };
                self.assign(block, dest, rvalue, span);
                block
            }
            ExprKind::Block(inner) => self.lower_block(dest, *inner, block),
            // `unsafe` changes what the checker permits and nothing about
            // control flow, so it is the block and no node of its own.
            ExprKind::Unsafe(inner) => self.lower_block(dest, *inner, block),
            ExprKind::If { cond, then_branch, else_branch } => {
                self.lower_if(dest, *cond, *then_branch, *else_branch, block, span)
            }
            ExprKind::Loop { body } => self.lower_loop(dest, *body, block, span),
            ExprKind::For { pattern, iter, body } => {
                self.lower_for(dest, *pattern, *iter, *body, block, span)
            }
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(dest, *scrutinee, arms, block, span)
            }
            ExprKind::Call { callee, args } => self.lower_call(dest, *callee, args, block, span),
            ExprKind::MethodCall { receiver, method, args } => {
                self.lower_method_call(dest, *receiver, *method, args, block, span)
            }
            ExprKind::Error => {
                self.assign(block, dest, Rvalue::Error, span);
                block
            }
        }
    }

    /// `a and b`, `a or b`. §3.
    fn lower_short_circuit(
        &mut self,
        dest: Place,
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let (cond, block) = self.operand(lhs, block);
        let rhs_block = self.new_block();
        let short_block = self.new_block();
        let join = self.new_block();
        // `and` evaluates the right operand when the left is true; `or` when it
        // is false. The short-circuit block stores the constant the operator
        // already knows.
        let (then_block, else_block, shortcut) = match op {
            BinaryOp::And => (rhs_block, short_block, false),
            _ => (short_block, rhs_block, true),
        };
        self.terminate(block, TerminatorKind::If { cond, then_block, else_block }, span);

        self.push_scope();
        let after_rhs = self.expr_into(dest.clone(), rhs, rhs_block);
        let after_rhs = self.pop_scope(after_rhs, span);
        self.terminate(after_rhs, TerminatorKind::Goto { target: join }, span);

        let rvalue = Rvalue::Use(Operand::Const(Constant::Literal(Literal::Bool(shortcut))));
        self.assign(short_block, dest, rvalue, span);
        self.terminate(short_block, TerminatorKind::Goto { target: join }, span);
        join
    }

    fn lower_if(
        &mut self,
        dest: Place,
        cond: ExprId,
        then_branch: thir::BlockId,
        else_branch: Option<ExprId>,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let (cond, block) = self.operand(cond, block);
        let then_block = self.new_block();
        let else_block = self.new_block();
        let join = self.new_block();
        self.terminate(block, TerminatorKind::If { cond, then_block, else_block }, span);

        let after_then = self.lower_block(dest.clone(), then_branch, then_block);
        self.terminate(after_then, TerminatorKind::Goto { target: join }, span);

        let after_else = match else_branch {
            Some(branch) => {
                self.push_scope();
                let after = self.expr_into(dest, branch, else_block);
                self.pop_scope(after, span)
            }
            None => {
                self.assign(else_block, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
                else_block
            }
        };
        self.terminate(after_else, TerminatorKind::Goto { target: join }, span);
        join
    }

    fn lower_loop(
        &mut self,
        dest: Place,
        body: thir::BlockId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        // The discarded destination of the body is allocated *outside* the
        // loop, so that its `StorageLive` runs once rather than once per
        // iteration. A temporary whose storage begins inside a loop and ends
        // outside it is the shape that makes a `StorageDead` run without its
        // `StorageLive`.
        let discard = self.temp(Ty::UNIT, span, block);
        let head = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);
        self.loops.push(LoopScope { head, exit, depth: self.scopes.len() });
        let after = self.lower_block(Place::local(discard), body, head);
        self.terminate(after, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();
        // A `loop` is `Ty::UNIT` (`check`'s §6), so the exit stores unit and
        // `break e` stores nothing.
        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    /// §7. The shape of a `for`, with a named hole where `next()` goes.
    fn lower_for(
        &mut self,
        dest: Place,
        pattern: PatId,
        iter: ExprId,
        body: thir::BlockId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let iter_ty = self.thir.ty(iter);
        let iterator = self.temp(iter_ty, span, block);
        block = self.expr_into(Place::local(iterator), iter, block);

        // The element the loop produces, and the presence test over it. Both
        // are allocated outside the loop for `lower_loop`'s reason.
        let element_ty = self.thir.pat(pattern).ty;
        let element = self.temp(element_ty, span, block);
        let present = self.temp(self.bool_ty, span, block);
        let discard = self.temp(Ty::UNIT, span, block);

        let head = self.new_block();
        let test = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        self.terminate(
            head,
            TerminatorKind::Call {
                callee: Callee::Unresolved(Unresolved::IterateNext),
                args: vec![Operand::Copy(Place::local(iterator))],
                destination: Place::local(element),
                target: Some(test),
            },
            span,
        );

        self.assign(
            test,
            Place::local(present),
            Rvalue::IsPresent(Operand::Copy(Place::local(element))),
            span,
        );
        self.terminate(
            test,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(present)),
                then_block: body_block,
                else_block: exit,
            },
            span,
        );

        self.loops.push(LoopScope { head, exit, depth: self.scopes.len() });
        self.push_scope();
        let bound = self.bind_pattern(&Place::local(element), pattern, body_block);
        let after = self.lower_block(Place::local(discard), body, bound);
        let after = self.pop_scope(after, span);
        self.terminate(after, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();

        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    fn lower_match(
        &mut self,
        dest: Place,
        scrutinee: ExprId,
        arms: &[Arm],
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let (place, mut block) = match self.as_place(scrutinee, block) {
            Some(found) => found,
            None => {
                let ty = self.thir.ty(scrutinee);
                let temp = self.temp(ty, span, block);
                let next = self.expr_into(Place::local(temp), scrutinee, block);
                (Place::local(temp), next)
            }
        };
        let join = self.new_block();
        for arm in arms {
            let body_block = self.new_block();
            let fail_block = self.new_block();
            self.test_pattern(&place, arm.pattern, block, body_block, fail_block);

            self.push_scope();
            let bound = self.bind_pattern(&place, arm.pattern, body_block);
            let after = self.expr_into(dest.clone(), arm.body, bound);
            let after = self.pop_scope(after, arm.span);
            self.terminate(after, TerminatorKind::Goto { target: join }, arm.span);

            block = fail_block;
        }
        // No arm matched. Exhaustiveness is Decision 16's and runs on THIR;
        // manufacturing a panic here would be this crate deciding what an
        // inexhaustive match does, which is that pass's answer to give.
        self.terminate(block, TerminatorKind::Unreachable, span);
        join
    }

    /// Terminates `block` so that control reaches `success` when `pattern`
    /// matches `place` and `fail` when it does not.
    ///
    /// **Decision. Arms are tested in order, with no decision tree.** rustc
    /// builds one so that an `n`-arm match costs one switch rather than `n`
    /// tests. That is a performance decision and this is not the phase that
    /// pays for it: `codegen-and-linking.md` has LLVM's switch formation below
    /// it, and a decision tree built here would be a second place the match
    /// semantics are decided. It is reversible without changing this file's
    /// interface, which is the property that makes deferring it safe.
    fn test_pattern(
        &mut self,
        place: &Place,
        pattern: PatId,
        block: BlockId,
        success: BlockId,
        fail: BlockId,
    ) {
        let thir = self.thir;
        let pat = thir.pat(pattern);
        let span = pat.span;
        match &pat.kind {
            // A hole admits, for `ty`'s §5 reason: refusing on an unanswerable
            // question is how a checker acquires a false positive.
            PatKind::Wildcard | PatKind::Binding { .. } | PatKind::Unit | PatKind::Error => {
                self.terminate(block, TerminatorKind::Goto { target: success }, span);
            }
            PatKind::Literal(literal) => {
                // A test reads and never consumes, whatever the type: a `match`
                // that moved its scrutinee before an arm was chosen would move
                // it on the paths where no arm matches.
                let operand = Operand::Copy(place.clone());
                let test = self.temp(self.bool_ty, span, block);
                // `null` is not a value to compare against; it is the absent
                // case, and the test for it is the presence test the language
                // already has — negated, which is what `inverted` records.
                let inverted = matches!(literal, Literal::Null);
                let rvalue = if inverted {
                    Rvalue::IsPresent(operand)
                } else {
                    Rvalue::Binary {
                        op: BinaryOp::Eq,
                        lhs: operand,
                        rhs: Operand::Const(Constant::Literal(literal.clone())),
                    }
                };
                self.assign(block, Place::local(test), rvalue, span);
                let (then_block, else_block) =
                    if inverted { (fail, success) } else { (success, fail) };
                let cond = Operand::Copy(Place::local(test));
                self.terminate(block, TerminatorKind::If { cond, then_block, else_block }, span);
            }
            PatKind::Variant { def: Some(variant), elems } => {
                let variant = *variant;
                let discr = self.temp(Ty::ERROR, span, block);
                let rvalue = Rvalue::Discriminant(place.clone());
                self.assign(block, Place::local(discr), rvalue, span);
                let matched = self.new_block();
                self.terminate(
                    block,
                    TerminatorKind::Switch {
                        discr: Operand::Copy(Place::local(discr)),
                        arms: vec![(variant, matched)],
                        otherwise: fail,
                    },
                    span,
                );
                let choice_ty = self.variant_payload_ty(variant, None).unwrap_or(pat.ty);
                let down = place.project(Projection::Downcast { variant, ty: choice_ty });
                self.test_sequence(&down, elems, matched, success, fail, span, Some(variant));
            }
            PatKind::Tuple(elements) => {
                self.test_sequence(place, elements, block, success, fail, span, None);
            }
            PatKind::Record { def: Some(def), fields } => {
                let owner = *def;
                let record_ty = pat.ty;
                let mut current = block;
                for (field, sub) in fields {
                    let next = self.new_block();
                    let field_ty = self.field_ty(owner, *field, record_ty);
                    let sub_place = place.project(Projection::Field { field: *field, ty: field_ty });
                    self.test_pattern(&sub_place, *sub, current, next, fail);
                    current = next;
                }
                self.terminate(current, TerminatorKind::Goto { target: success }, span);
            }
            // A variant or a record that did not resolve names no place to test
            // against, so the arm is admitted rather than refused.
            PatKind::Variant { def: None, .. } | PatKind::Record { def: None, .. } => {
                self.terminate(block, TerminatorKind::Goto { target: success }, span);
            }
            PatKind::Or(alternatives) => {
                if alternatives.is_empty() {
                    self.terminate(block, TerminatorKind::Goto { target: fail }, span);
                    return;
                }
                let mut current = block;
                for (at, alternative) in alternatives.iter().enumerate() {
                    let last = at + 1 == alternatives.len();
                    let next = if last { fail } else { self.new_block() };
                    self.test_pattern(place, *alternative, current, success, next);
                    current = next;
                }
            }
        }
    }

    /// Tests a positional sequence of sub-patterns, all of which must match.
    #[allow(clippy::too_many_arguments)]
    fn test_sequence(
        &mut self,
        place: &Place,
        elements: &[PatId],
        block: BlockId,
        success: BlockId,
        fail: BlockId,
        span: Span,
        variant: Option<DefId>,
    ) {
        let mut current = block;
        for (index, element) in elements.iter().enumerate() {
            let next = self.new_block();
            let ty = self.positional_ty(variant, index, *element);
            let sub = place.project(Projection::TupleField { index: index as u32, ty });
            self.test_pattern(&sub, *element, current, next, fail);
            current = next;
        }
        self.terminate(current, TerminatorKind::Goto { target: success }, span);
    }

    /// Assigns the sub-places a pattern names into the locals it binds.
    fn bind_pattern(&mut self, place: &Place, pattern: PatId, mut block: BlockId) -> BlockId {
        let thir = self.thir;
        let pat = thir.pat(pattern);
        let span = pat.span;
        match &pat.kind {
            PatKind::Binding { def, .. } => {
                let scope = self.scopes.len().saturating_sub(1);
                let local = self.declare_binding(*def, block, scope);
                let operand = self.read(place.clone(), pat.ty);
                self.assign(block, Place::local(local), Rvalue::Use(operand), span);
                block
            }
            PatKind::Variant { def: Some(variant), elems } => {
                let variant = *variant;
                let choice_ty = self.variant_payload_ty(variant, None).unwrap_or(pat.ty);
                let down = place.project(Projection::Downcast { variant, ty: choice_ty });
                for (index, element) in elems.iter().enumerate() {
                    let ty = self.positional_ty(Some(variant), index, *element);
                    let sub = down.project(Projection::TupleField { index: index as u32, ty });
                    block = self.bind_pattern(&sub, *element, block);
                }
                block
            }
            PatKind::Tuple(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    let ty = self.positional_ty(None, index, *element);
                    let sub = place.project(Projection::TupleField { index: index as u32, ty });
                    block = self.bind_pattern(&sub, *element, block);
                }
                block
            }
            PatKind::Record { def: Some(def), fields } => {
                let owner = *def;
                let record_ty = pat.ty;
                for (field, sub) in fields {
                    let ty = self.field_ty(owner, *field, record_ty);
                    let sub_place = place.project(Projection::Field { field: *field, ty });
                    block = self.bind_pattern(&sub_place, *sub, block);
                }
                block
            }
            // An `or` pattern binds the same names on every alternative, so the
            // first is enough to introduce them. Which alternative matched is
            // not known at this point in the CFG, and binding from all of them
            // would assign twice.
            PatKind::Or(alternatives) => match alternatives.first() {
                Some(first) => self.bind_pattern(place, *first, block),
                None => block,
            },
            PatKind::Wildcard
            | PatKind::Literal(_)
            | PatKind::Unit
            | PatKind::Variant { def: None, .. }
            | PatKind::Record { def: None, .. }
            | PatKind::Error => block,
        }
    }

    // --- calls ------------------------------------------------------------

    fn lower_call(
        &mut self,
        dest: Place,
        callee: ExprId,
        args: &[ExprId],
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        // A variant constructor is a value, not a call. `Rvalue::Variant` says
        // why the distinction is kept here rather than at the call graph.
        if let ExprKind::Item(def) = self.thir.expr(callee).kind {
            if self.context.defs.get(def).kind == DefKind::Variant {
                let mut payload = Vec::with_capacity(args.len());
                for arg in args {
                    let (operand, next) = self.operand(*arg, block);
                    payload.push(operand);
                    block = next;
                }
                self.assign(block, dest, Rvalue::Variant { variant: def, payload }, span);
                return block;
            }
        }
        let callee = match self.thir.expr(callee).kind {
            ExprKind::Item(def) => Callee::Def(def),
            _ => {
                let (operand, next) = self.operand(callee, block);
                block = next;
                Callee::Indirect(operand)
            }
        };
        let mut operands = Vec::with_capacity(args.len());
        for arg in args {
            let (operand, next) = self.argument(*arg, block);
            operands.push(operand);
            block = next;
        }
        self.emit_call(dest, callee, operands, block, span)
    }

    fn lower_method_call(
        &mut self,
        dest: Place,
        receiver: ExprId,
        method: Option<DefId>,
        args: &[ExprId],
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let callee = match method {
            Some(def) => Callee::Def(def),
            // Decision 11's lookup. `Unresolved::Method` is the price, and §5's
            // exception is why every operand below is a copy.
            None => Callee::Unresolved(Unresolved::Method),
        };
        let mut operands = Vec::with_capacity(args.len() + 1);

        // The receiver. When the method resolved, its declared `self` decides
        // whether the receiver is borrowed — and a `mutable self` receiver in
        // argument position is §6's two-phase case, which is the whole of
        // `v.push(v.len())`.
        let self_kind = method
            .and_then(|def| self.context.decls.signature(def))
            .and_then(|signature| signature.self_param)
            .map(|(_, kind)| kind);
        match self_kind {
            Some(kind @ (SelfKind::Shared | SelfKind::Mutable)) => {
                let mutable = kind == SelfKind::Mutable;
                let ty = self.thir.ty(receiver);
                let borrowed = self.context.types.borrowed(mutable, ty);
                let temp = self.temp(borrowed, span, block);
                block =
                    self.lower_borrow(Place::local(temp), mutable, receiver, block, span, true);
                operands.push(Operand::Move(Place::local(temp)));
            }
            Some(SelfKind::Value) => {
                let (operand, next) = self.argument(receiver, block);
                operands.push(operand);
                block = next;
            }
            None => {
                let (operand, next) = self.operand(receiver, block);
                operands.push(force_copy(operand));
                block = next;
            }
        }

        let unresolved = method.is_none();
        for arg in args {
            let (operand, next) =
                if unresolved { self.operand(*arg, block) } else { self.argument(*arg, block) };
            operands.push(if unresolved { force_copy(operand) } else { operand });
            block = next;
        }
        self.emit_call(dest, callee, operands, block, span)
    }

    fn emit_call(
        &mut self,
        dest: Place,
        callee: Callee,
        args: Vec<Operand>,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        // Every two-phase borrow among the arguments is activated here, in
        // argument order, immediately before the call. §6.
        let activations: Vec<BorrowId> = args
            .iter()
            .filter_map(|operand| operand.place())
            .filter_map(|place| self.two_phase_of(place.local))
            .collect();
        for borrow in activations {
            self.push_statement(block, StatementKind::Activate(borrow), span);
        }

        let diverges = match &callee {
            Callee::Def(def) => {
                let signature = self.context.decls.signature(*def);
                match signature {
                    Some(signature) => {
                        let prelude = self.context.decls.prelude();
                        !signature.can_return(prelude, self.context.types)
                    }
                    None => false,
                }
            }
            _ => false,
        };
        let target = if diverges { None } else { Some(self.new_block()) };
        self.terminate(
            block,
            TerminatorKind::Call { callee, args, destination: dest, target },
            span,
        );
        match target {
            Some(target) => target,
            // A diverging call has no successor, and the statements after it
            // are lowered into a block nothing jumps to. `StmtKind::Return`
            // says why that is better than skipping them.
            None => self.new_block(),
        }
    }

    /// The two-phase borrow whose reference lives in this local, if any.
    fn two_phase_of(&self, local: Local) -> Option<BorrowId> {
        self.borrows
            .iter()
            .find(|data| data.kind == BorrowKind::TwoPhase && data.destination.local == local)
            .map(|data| data.id)
    }

    // --- borrows ----------------------------------------------------------

    fn lower_borrow(
        &mut self,
        dest: Place,
        mutable: bool,
        operand: ExprId,
        block: BlockId,
        span: Span,
        in_argument: bool,
    ) -> BlockId {
        let (place, block) = match self.as_place(operand, block) {
            Some(found) => found,
            // `borrowed f()` — a borrow of a value with no place. The value
            // goes into a temporary, which is then the referent, and the
            // temporary dies at the end of the statement: rule 5 gets a real
            // storage-dead point to compare against, which is the answer rather
            // than a special case.
            None => {
                let ty = self.thir.ty(operand);
                let temp = self.temp(ty, span, block);
                let next = self.expr_into(Place::local(temp), operand, block);
                (Place::local(temp), next)
            }
        };
        let kind = match (mutable, in_argument) {
            (false, _) => BorrowKind::Shared,
            (true, true) => BorrowKind::TwoPhase,
            (true, false) => BorrowKind::Exclusive,
        };
        let id = BorrowId::from_index(self.borrows.len());
        self.borrows.push(BorrowData {
            id,
            kind,
            place: place.clone(),
            destination: dest.clone(),
            // Both patched by `index_borrows`, once the numbering has stopped
            // moving.
            reserved: Point { block: ENTRY_BLOCK, statement: 0 },
            activation: None,
            span,
        });
        self.assign(block, dest, Rvalue::Ref { kind, place, borrow: id }, span);
        block
    }

    // --- places and operands ----------------------------------------------

    /// The place an expression denotes, when it denotes one.
    ///
    /// This is [`science_types::thir::Body::place_of`] with the two projections
    /// that file refuses — an index and a dereference — and it is the single
    /// clearest thing MIR buys.
    fn as_place(&mut self, expr: ExprId, block: BlockId) -> Option<(Place, BlockId)> {
        let thir = self.thir;
        match &thir.expr(expr).kind {
            ExprKind::Local(def) | ExprKind::SelfValue(def) => {
                let local = self.bindings.get(def).copied()?;
                Some((Place::local(local), block))
            }
            // A narrowed read is the same storage seen at a smaller type, so it
            // is the same place. THIR's §3 made exactly this call and MIR keeps
            // it; seeing through a `Coerce` would not, because a coerced value
            // is a new value in a new representation.
            ExprKind::Narrow(operand) => self.as_place(*operand, block),
            ExprKind::Field { base, field } => {
                let field = (*field)?;
                let (place, block) = self.as_place(*base, block)?;
                let place = self.auto_deref(place);
                let owner = self.record_of(&place)?;
                let base_ty = self.place_ty(&place);
                let ty = self.field_ty(owner, field, base_ty);
                Some((place.project(Projection::Field { field, ty }), block))
            }
            ExprKind::Index { base, index } => {
                let index = *index;
                let (place, block) = self.as_place(*base, block)?;
                let place = self.auto_deref(place);
                let index_ty = thir.ty(index);
                let span = thir.expr(index).span;
                // A fresh temporary per index expression, assigned once. That
                // is what `Projection::Index`'s equality rests on.
                let temp = self.temp(index_ty, span, block);
                let block = self.expr_into(Place::local(temp), index, block);
                let ty = self.element_ty(&place);
                Some((place.project(Projection::Index { index: temp, ty }), block))
            }
            _ => None,
        }
    }

    /// Inserts a `Deref` for every borrow between a place and the thing it
    /// projects into. §4.
    fn auto_deref(&mut self, mut place: Place) -> Place {
        loop {
            let written = self.place_ty(&place);
            let ty = self.revealed(written);
            let TyKind::Borrowed { inner, .. } = *self.context.types.kind(ty) else {
                return place;
            };
            place = place.project(Projection::Deref { ty: inner });
        }
    }

    /// Reads a place as an operand. §5 decides which of the two it is.
    fn read(&mut self, place: Place, ty: Ty) -> Operand {
        if self.is_copy(ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    /// An operand in an ordinary, non-argument position.
    fn operand(&mut self, expr: ExprId, block: BlockId) -> (Operand, BlockId) {
        let thir = self.thir;
        let node = thir.expr(expr);
        let ty = node.ty;
        let span = node.span;
        match &node.kind {
            ExprKind::Literal(literal) => {
                (Operand::Const(Constant::Literal(literal.clone())), block)
            }
            ExprKind::Unit => (Operand::Const(Constant::Unit), block),
            _ => {
                if let Some((place, block)) = self.as_place(expr, block) {
                    let operand = self.read(place, ty);
                    return (operand, block);
                }
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), expr, block);
                (Operand::Move(Place::local(temp)), block)
            }
        }
    }

    /// An operand in argument position, which is the only place §6 makes a
    /// borrow two-phase.
    fn argument(&mut self, expr: ExprId, block: BlockId) -> (Operand, BlockId) {
        let thir = self.thir;
        let node = thir.expr(expr);
        let span = node.span;
        let ty = node.ty;
        match &node.kind {
            ExprKind::Borrow { mutable, operand } => {
                let temp = self.temp(ty, span, block);
                let block =
                    self.lower_borrow(Place::local(temp), *mutable, *operand, block, span, true);
                (Operand::Move(Place::local(temp)), block)
            }
            // A coercion over a borrow — `assign`'s §4 unsizing — is still a
            // borrow in argument position, and the borrow underneath it is the
            // one that wants two phases.
            ExprKind::Coerce { operand, coercion }
                if matches!(thir.expr(*operand).kind, ExprKind::Borrow { .. }) =>
            {
                let coercion = *coercion;
                let (inner, block) = self.argument(*operand, block);
                let temp = self.temp(ty, span, block);
                let rvalue = Rvalue::Coerce { operand: inner, coercion, ty };
                self.assign(block, Place::local(temp), rvalue, span);
                (Operand::Move(Place::local(temp)), block)
            }
            _ => self.operand(expr, block),
        }
    }

    fn assign(&mut self, block: BlockId, place: Place, rvalue: Rvalue, span: Span) {
        self.push_statement(block, StatementKind::Assign { place, rvalue }, span);
    }

    // --- types ------------------------------------------------------------

    fn revealed(&mut self, ty: Ty) -> Ty {
        self.context.aliases.reveal(self.context.types, ty).unwrap_or(ty)
    }

    fn place_ty(&self, place: &Place) -> Ty {
        match place.projection.last() {
            Some(step) => step.ty(),
            None => self.locals[place.local.index()].ty,
        }
    }

    /// The record a place's type names, after aliases and borrows.
    fn record_of(&mut self, place: &Place) -> Option<DefId> {
        let written = self.place_ty(place);
        let ty = self.revealed(written);
        let named = match self.context.types.kind(ty) {
            TyKind::Named { def, .. } => Some(*def),
            // `self.tokens` inside a method: the receiver's type is `Self`, and
            // the implementation block it was written in is what names the
            // record. `check`'s substitution does this for types; a place has
            // to do it for fields.
            TyKind::SelfType { owner } => {
                let owner = *owner;
                let self_ty = self.context.decls.self_ty(owner)?;
                match self.context.types.kind(self_ty) {
                    TyKind::Named { def, .. } => Some(*def),
                    _ => None,
                }
            }
            _ => None,
        };
        named
    }

    /// A field's type, taken from the *declaration* and substituted with the
    /// owner's generic arguments.
    ///
    /// Never from the expression. [`Projection`]'s documentation is the whole
    /// of why, and it is §10 item 2.
    fn field_ty(&mut self, owner: DefId, field: DefId, base: Ty) -> Ty {
        let Some(record) = self.context.decls.record(owner) else {
            return Ty::ERROR;
        };
        let Some((_, declared)) = record.fields.iter().find(|(id, _)| *id == field).copied() else {
            return Ty::ERROR;
        };
        let generics = record.generics.clone();
        if generics.is_empty() {
            return declared;
        }
        let base = self.revealed(base);
        let args = match self.context.types.kind(base) {
            TyKind::Named { args, .. } => args.clone(),
            _ => Vec::new(),
        };
        if args.is_empty() {
            return declared;
        }
        let subst = Substitution::of_generics(&generics, &args);
        subst.apply(self.context.types, declared).unwrap_or(Ty::ERROR)
    }

    /// A variant's payload type at one position, or the choice's own type when
    /// no position is asked for.
    fn variant_payload_ty(&mut self, variant: DefId, index: Option<usize>) -> Option<Ty> {
        let declaration = self.context.decls.variant(variant)?;
        match index {
            Some(index) => declaration.payload.get(index).copied(),
            None => {
                let choice = declaration.choice;
                Some(self.context.types.named(choice, Vec::new()))
            }
        }
    }

    /// The type at one position of a variant's payload or a tuple, falling back
    /// to the sub-pattern's own type.
    fn positional_ty(&mut self, variant: Option<DefId>, index: usize, element: PatId) -> Ty {
        let fallback = self.thir.pat(element).ty;
        match variant {
            Some(variant) => self.variant_payload_ty(variant, Some(index)).unwrap_or(fallback),
            None => fallback,
        }
    }

    fn tuple_field_ty(&mut self, tuple: Ty, index: usize) -> Ty {
        let tuple = self.revealed(tuple);
        match self.context.types.kind(tuple) {
            TyKind::Tuple(elements) => elements.get(index).copied().unwrap_or(Ty::ERROR),
            _ => Ty::ERROR,
        }
    }

    /// The element type of an indexable place.
    ///
    /// `Array of T` gives `T` by reading the type argument. Anything else is
    /// the `Index` interface, which is Decision 11's lookup, so the answer is
    /// [`Ty::ERROR`] — which `ty`'s §5 makes harmless rather than cascading.
    fn element_ty(&mut self, place: &Place) -> Ty {
        let written = self.place_ty(place);
        let ty = self.revealed(written);
        match self.context.types.kind(ty) {
            TyKind::Named { args, .. } => {
                args.first().and_then(|arg| arg.as_type()).unwrap_or(Ty::ERROR)
            }
            _ => Ty::ERROR,
        }
    }

    /// Whether reading a value of this type leaves the original behind. §5.
    fn is_copy(&mut self, ty: Ty) -> bool {
        let ty = self.revealed(ty);
        let kind = self.context.types.kind(ty).clone();
        match kind {
            TyKind::Unit | TyKind::Error => true,
            // A shared borrow is a value that can be duplicated; an exclusive
            // one is not, because duplicating it is aliasing it.
            TyKind::Borrowed { mutable, .. } => !mutable,
            TyKind::Tuple(elements) => elements.iter().all(|element| self.is_copy(*element)),
            TyKind::Nullable(inner) => self.is_copy(inner),
            TyKind::Named { .. } => {
                let prelude = self.context.decls.prelude();
                prelude.is_numeric(self.context.types, ty)
                    || prelude.is_bool(self.context.types, ty)
                    || prelude.is(self.context.types, ty, "Char")
            }
            _ => false,
        }
    }
}

/// Turns an operand into one that reads rather than consumes. §5's exception.
fn force_copy(operand: Operand) -> Operand {
    match operand {
        Operand::Move(place) => Operand::Copy(place),
        other => other,
    }
}
