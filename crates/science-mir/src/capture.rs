//! What a closure captures, and how — the walk [`crate::lower`]'s §8 refused
//! to write.
//!
//! # 1. The question this module answers, and the one it does not
//!
//! [`crate::lower`]'s §8 decides the *capture discipline*: every capture is a
//! borrow, shared unless the body writes through the place. This module decides
//! that rule's input — **which bindings the closure's body names that the
//! closure's body did not introduce, and what it does with each one.** It
//! computes a [`Use`] and stops; turning a [`Use`] into a
//! [`crate::mir::BorrowKind`] needs the place's *type*, and this walk has no
//! types in it on purpose (§5).
//!
//! # 2. A capture is a whole binding, not a place
//!
//! **Decision. The capture set is keyed on the [`DefId`] of a binding, never on
//! a projection out of one.** A body that names `config.port` captures
//! `config`.
//!
//! Rust went the other way in RFC 2229 and captures `config.port` precisely.
//! The reason not to here is that the granularity has to match the thing it
//! feeds: [`crate::moves`]'s §3 tracks moves **per local**, and
//! [`crate::mir::Place::may_overlap`] answers *may* for two places under one
//! local. A capture set finer than either would be a third granularity in a
//! crate that already documents two, and the finer answer would be coarsened
//! again by the first consumer that asked `may_overlap`.
//!
//! **What it costs** is a false conflict: a closure reading `config.port` and
//! an enclosing write to `config.host` are two disjoint fields and one
//! refusal. That is the direction [`crate::lower`]'s §5 points the doubt in —
//! an over-wide borrow refuses a program that is fine, a too-narrow one accepts
//! a program that is not — and the upgrade, when somebody wants it, is to key
//! [`Capture`] on a [`science_types::thir::Place`] and let `lower`'s `as_place`
//! build the projection. Nothing here forecloses it.
//!
//! # 3. Scoping is by definition identity, and that is not a shortcut
//!
//! A binding introduced *inside* the closure — the parameter, a `let` in the
//! body, a `match` arm's pattern, a nested closure's own parameter — is not a
//! capture. This module finds them by collecting every [`DefId`] the body
//! binds, in one pass, and then treating any name outside that set as free.
//!
//! **A set rather than a scope stack, because the resolver already made the ids
//! unique.** `science-resolve`'s `resolve_closure` allocates a fresh [`DefId`]
//! per binder, so two `let x` in sibling blocks are two definitions and a
//! shadowed name is a different id from the one it shadows. The stack that
//! would otherwise keep them apart was paid for two phases up, and rebuilding
//! it here would be a second implementation of a question the HIR already
//! answered.
//!
//! # 4. `each` is an ordinary parameter, and the corpus is capture-free
//!
//! `docs.iterate().map(each.title)` captures nothing. The parser wraps the
//! argument in a closure and the resolver binds `each` as that closure's
//! parameter (`resolve_closure`: *"the implicit form binds `each`, which the
//! programmer did not write"*), so by THIR `each` is an [`ExprKind::Local`] at
//! a [`DefId`] this walk finds in its own bound set.
//!
//! **Every closure in `examples/` is capture-free for that reason** —
//! `each.title`, `doc giving doc.summarize()`, `line giving line.length()` all
//! name only the subject — so the corpus exercises the *machinery* (a closure
//! now lowers, with an empty capture list) and not the *discipline*. Saying so
//! is the point: the acceptance material for this module is
//! `tests/captures.rs`, and the corpus's contribution is the guarantee that
//! nothing moved.
//!
//! # 5. Three uses, and what decides which
//!
//! [`Use`] is a three-point lattice and the strongest use of a binding wins.
//! The classification is **syntactic**, read off the node that encloses the
//! name:
//!
//! | The body writes | Use | Because |
//! |---|---|---|
//! | `captured be e`, `captured.f be e` | [`Use::Write`] | an assignment target is written |
//! | `mutable borrowed captured` | [`Use::Write`] | the author asked for exclusive access |
//! | `captured.push(x)`, `push` being `mutable self` | [`Use::Write`] | the receiver borrow is exclusive |
//! | `captured.title`, `captured[i]`, `borrowed captured` | [`Use::Read`] | a projection or a shared borrow reads |
//! | `captured.len()`, `len` being `shared self` | [`Use::Read`] | the receiver borrow is shared |
//! | `f(captured)` | [`Use::Consume`] | the value is produced, which is a move unless the type copies |
//!
//! **[`Use::Consume`] is the row that is not a decision about borrowing**, and
//! it is separate from [`Use::Read`] because the two differ by a fact this walk
//! does not have: whether the type is trivially copyable. [`crate::lower`]'s §8
//! resolves it with the same `is_copy` its §5 uses, and states what the
//! remaining doubt costs.
//!
//! **A method whose lookup failed is [`Use::Read`].** `MethodCall::method` is
//! `None` for every container method in the corpus, so there is no `self` to
//! read a [`SelfKind`] off. Calling it a read rather than a write is the choice
//! [`crate::lower`]'s §5 makes about an unresolved call's arguments and errs
//! the same way: a hole above this crate must not manufacture a refusal in
//! ordinary code. [`crate::lower`]'s §8.5 lists it among what is left.

use science_diagnostics::Span;
use science_resolve::hir::{DefId, SelfKind};
use science_types::items::Declarations;
use science_types::thir::{self, ExprId, ExprKind, FStringPart, PatId, PatKind, StmtKind};

/// How a closure's body uses a binding it did not introduce. §5.
///
/// Ordered, because a binding used twice is captured at the stronger of the two
/// and [`Ord::max`] is that sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Use {
    /// Projected out of, indexed, shared-borrowed, or the receiver of a
    /// `shared self` method. A shared borrow covers all of it.
    Read,
    /// Named where its *value* is produced — an argument, an operand, the
    /// closure's own result. A move if the type does not copy.
    Consume,
    /// Assigned to, exclusively borrowed, or the receiver of a `mutable self`
    /// method.
    Write,
}

/// One capture: the binding, the strongest use the body makes of it, and the
/// span of the first place the body named it.
///
/// The span is the *first* occurrence and not the strongest one, because a
/// message about a capture wants to point at where the closure reached out of
/// itself, and that is the first mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub def: DefId,
    pub use_kind: Use,
    /// Every node whose *value* was produced out of this binding — the
    /// outermost place expression of each [`Use::Consume`] occurrence.
    ///
    /// **The list, and not just the count, because copyability is asked of the
    /// node and not of the root.** `each giving captured.port` produces the
    /// value of `captured.port`; if `port` is an `Int` that is a copy and the
    /// capture is shared, and asking whether *`captured`* copies would answer
    /// `Config` and take an exclusive borrow of a record every read of which is
    /// a copy. [`crate::lower`]'s §8.2 takes the strongest borrow if **any**
    /// entry here is a non-`Copy` type.
    pub consumed: Vec<ExprId>,
    pub span: Span,
}

/// Every binding a closure's body names from outside itself, in the order the
/// body first names them.
///
/// **First-mention order and not definition order**, so that the captures a
/// [`crate::mir::Rvalue::Closure`] carries are a function of this body's own
/// THIR and of nothing else. That is `mir`'s §2 read at the one place a closure
/// could have broken it: a definition table is shared across the crate, a
/// body's THIR is not, and a capture list ordered by [`DefId`] would have made
/// this body's statement numbering depend on how many definitions came first.
pub fn captures_of(
    decls: &Declarations,
    thir: &thir::Body,
    param: DefId,
    body: ExprId,
) -> Vec<Capture> {
    let mut walker = Walker { thir, decls, bound: vec![param], found: Vec::new() };
    walker.bind_expr(body);
    walker.expr(body, Ctx::Consume(body));
    walker.found
}

/// What the node enclosing a name does with it. §5.
/// [`Ctx::Consume`] carries the node whose value is being produced, which is
/// the outermost expression of the place chain and *not* the name at the bottom
/// of it. [`Capture::consumed`] is why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Read,
    Consume(ExprId),
    Write,
}

impl Ctx {
    fn to_use(self) -> Use {
        match self {
            Ctx::Read => Use::Read,
            Ctx::Consume(_) => Use::Consume,
            Ctx::Write => Use::Write,
        }
    }
}

struct Walker<'a> {
    thir: &'a thir::Body,
    decls: &'a Declarations,
    /// Every definition the closure's body binds. §3.
    bound: Vec<DefId>,
    found: Vec<Capture>,
}

impl Walker<'_> {
    // --- pass one: what the body binds ------------------------------------

    fn bind_expr(&mut self, id: ExprId) {
        let thir = self.thir;
        match &thir.expr(id).kind {
            ExprKind::Literal(_)
            | ExprKind::Local(_)
            | ExprKind::SelfValue(_)
            | ExprKind::Item(_)
            | ExprKind::Unit
            | ExprKind::Error => {}
            ExprKind::Call { callee, args } => {
                self.bind_expr(*callee);
                for arg in args {
                    self.bind_expr(*arg);
                }
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.bind_expr(*receiver);
                for arg in args {
                    self.bind_expr(*arg);
                }
            }
            ExprKind::Field { base, .. } => self.bind_expr(*base),
            ExprKind::Index { base, index } => {
                self.bind_expr(*base);
                self.bind_expr(*index);
            }
            ExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.bind_expr(*value);
                }
            }
            // An array literal's elements are ordinary expressions, exactly
            // as a tuple's are; the two differ in what is built from them and
            // not in what they bind.
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.bind_expr(*element);
                }
            }
            // An interpolation's holes are ordinary expressions and bind
            // ordinary names.
            ExprKind::FString(parts) => {
                for part in parts {
                    if let FStringPart::Hole(hole) = part {
                        self.bind_expr(*hole);
                    }
                }
            }
            ExprKind::Unary { operand, .. }
            | ExprKind::Cast { operand }
            | ExprKind::Present(operand)
            | ExprKind::Borrow { operand, .. }
            | ExprKind::Coerce { operand, .. }
            | ExprKind::Narrow(operand) => self.bind_expr(*operand),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.bind_expr(*lhs);
                self.bind_expr(*rhs);
            }
            ExprKind::Range { start, end, .. } => {
                self.bind_expr(*start);
                self.bind_expr(*end);
            }
            // A nested closure's parameter is bound inside this body too, so a
            // name the inner closure introduced can never be mistaken for one
            // the outer body has to reach out for.
            ExprKind::Closure { param, body } => {
                self.bound.push(*param);
                self.bind_expr(*body);
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.bind_expr(*cond);
                self.bind_block(*then_branch);
                if let Some(branch) = else_branch {
                    self.bind_expr(*branch);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.bind_expr(*scrutinee);
                for arm in arms {
                    self.bind_pat(arm.pattern);
                    self.bind_expr(arm.body);
                }
            }
            ExprKind::Loop { body } => self.bind_block(*body),
            ExprKind::For { pattern, iter, body, .. } => {
                self.bind_pat(*pattern);
                self.bind_expr(*iter);
                self.bind_block(*body);
            }
            ExprKind::Block(block) | ExprKind::Unsafe(block) => self.bind_block(*block),
        }
    }

    fn bind_block(&mut self, id: thir::BlockId) {
        let thir = self.thir;
        let block = thir.block(id);
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Let { bindings, value } => {
                    self.bound.extend(bindings.iter().copied());
                    self.bind_expr(*value);
                }
                StmtKind::Expr(value) => self.bind_expr(*value),
                StmtKind::Assign { target, value } => {
                    self.bind_expr(*target);
                    self.bind_expr(*value);
                }
                StmtKind::Return(Some(value)) | StmtKind::Break(Some(value)) => {
                    self.bind_expr(*value)
                }
                StmtKind::Assert { cond, message } => {
                    self.bind_expr(*cond);
                    self.bind_expr(*message);
                }
                StmtKind::Return(None)
                | StmtKind::Break(None)
                | StmtKind::Continue
                | StmtKind::Error => {}
            }
        }
        if let Some(tail) = block.tail {
            self.bind_expr(tail);
        }
    }

    fn bind_pat(&mut self, id: PatId) {
        let thir = self.thir;
        match &thir.pat(id).kind {
            PatKind::Binding { def, .. } => self.bound.push(*def),
            PatKind::Variant { elems, .. } => {
                for elem in elems {
                    self.bind_pat(*elem);
                }
            }
            PatKind::Record { fields, .. } => {
                for (_, pat) in fields {
                    self.bind_pat(*pat);
                }
            }
            PatKind::Tuple(elems) | PatKind::Or(elems) => {
                for elem in elems {
                    self.bind_pat(*elem);
                }
            }
            PatKind::Wildcard | PatKind::Literal(_) | PatKind::Unit | PatKind::Error => {}
        }
    }

    // --- pass two: what it does with what it did not bind ------------------

    fn record(&mut self, def: DefId, ctx: Ctx, span: Span) {
        if self.bound.contains(&def) {
            return;
        }
        let consumed = match ctx {
            Ctx::Consume(node) => Some(node),
            Ctx::Read | Ctx::Write => None,
        };
        if let Some(found) = self.found.iter_mut().find(|capture| capture.def == def) {
            found.use_kind = found.use_kind.max(ctx.to_use());
            found.consumed.extend(consumed);
            return;
        }
        self.found.push(Capture {
            def,
            use_kind: ctx.to_use(),
            consumed: consumed.into_iter().collect(),
            span,
        });
    }

    fn expr(&mut self, id: ExprId, ctx: Ctx) {
        let thir = self.thir;
        let span = thir.expr(id).span;
        match &thir.expr(id).kind {
            ExprKind::Local(def) | ExprKind::SelfValue(def) => self.record(*def, ctx, span),
            ExprKind::Literal(_) | ExprKind::Item(_) | ExprKind::Unit | ExprKind::Error => {}
            // An indirect call's callee is a value, and so are its arguments.
            ExprKind::Call { callee, args } => {
                self.expr(*callee, Ctx::Consume(*callee));
                for arg in args {
                    self.expr(*arg, Ctx::Consume(*arg));
                }
            }
            // §5's third and fifth rows. The receiver is a *place* the callee
            // borrows, and which borrow it is, is the declared `self`'s — the
            // same lookup `lower_method_call` makes to decide the same thing.
            ExprKind::MethodCall { receiver, method, args } => {
                let self_kind = method
                    .and_then(|def| self.decls.signature(def))
                    .and_then(|signature| signature.self_param)
                    .map(|(_, kind)| kind);
                let receiver_ctx = match self_kind {
                    Some(SelfKind::Mutable) => Ctx::Write,
                    Some(SelfKind::Shared) => Ctx::Read,
                    Some(SelfKind::Value) => Ctx::Consume(*receiver),
                    // §5's last paragraph: an unresolved method reads.
                    None => Ctx::Read,
                };
                self.expr(*receiver, receiver_ctx);
                for arg in args {
                    self.expr(*arg, Ctx::Consume(*arg));
                }
            }
            // A projection carries the enclosing context down to the root,
            // because a write to `c.f` writes `c` and a move out of `c.f` moves
            // part of `c` — and §2's per-local granularity has no way to say
            // *part*.
            ExprKind::Field { base, .. } => self.expr(*base, ctx),
            ExprKind::Index { base, index } => {
                self.expr(*base, ctx);
                self.expr(*index, Ctx::Consume(*index));
            }
            ExprKind::Narrow(operand) => self.expr(*operand, ctx),
            // §5's second and fourth rows.
            ExprKind::Borrow { mutable, operand } => {
                self.expr(*operand, if *mutable { Ctx::Write } else { Ctx::Read })
            }
            ExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.expr(*value, Ctx::Consume(*value));
                }
            }
            // An array literal consumes each element: `science_array_push`
            // copies the value into the buffer and the literal is the only
            // owner afterwards, which is a move and not a read.
            ExprKind::Tuple(elements) | ExprKind::Array(elements) => {
                for element in elements {
                    self.expr(*element, Ctx::Consume(*element));
                }
            }
            // `e?` is total and leaves its operand where it was — `lower`'s own
            // `Present` arm forces the operand to a copy for the same reason —
            // so it reads.
            ExprKind::Present(operand) => self.expr(*operand, Ctx::Read),
            ExprKind::Unary { operand, .. }
            | ExprKind::Cast { operand }
            | ExprKind::Coerce { operand, .. } => self.expr(*operand, Ctx::Consume(*operand)),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(*lhs, Ctx::Consume(*lhs));
                self.expr(*rhs, Ctx::Consume(*rhs));
            }
            ExprKind::Range { start, end, .. } => {
                self.expr(*start, Ctx::Consume(*start));
                self.expr(*end, Ctx::Consume(*end));
            }
            // §1.6: *"an interpolation borrows its operands. `f"{doc}"` does
            // not move `doc`."* That is the whole reason this is `Ctx::Read`
            // and not `Ctx::Consume`, and the note gives the reason in one
            // line: a debugging `print` that moves the value you were about to
            // use is an ownership error caused by a line the author added to
            // understand a different problem.
            ExprKind::FString(parts) => {
                for part in parts {
                    if let FStringPart::Hole(hole) = part {
                        self.expr(*hole, Ctx::Read);
                    }
                }
            }
            // A nested closure's captures are this closure's captures too,
            // wherever they reach past both. The inner walk classifies them and
            // this one merges the answer, so `giving (giving outer.f be 1)`
            // captures `outer` exclusively at both levels.
            ExprKind::Closure { param, body } => {
                for capture in captures_of(self.decls, thir, *param, *body) {
                    match capture.use_kind {
                        Use::Read => self.record(capture.def, Ctx::Read, capture.span),
                        Use::Write => self.record(capture.def, Ctx::Write, capture.span),
                        Use::Consume => {
                            for node in capture.consumed {
                                self.record(capture.def, Ctx::Consume(node), capture.span);
                            }
                        }
                    }
                }
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                self.expr(*cond, Ctx::Consume(*cond));
                self.block(*then_branch, ctx);
                if let Some(branch) = else_branch {
                    self.expr(*branch, ctx);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                // A scrutinee is matched *through*; a pattern binding that
                // moves out of it is a binding this body introduced and so is
                // not a capture at all.
                self.expr(*scrutinee, Ctx::Read);
                for arm in arms {
                    self.expr(arm.body, ctx);
                }
            }
            ExprKind::Loop { body } => self.block(*body, Ctx::Read),
            ExprKind::For { iter, body, .. } => {
                // `for x in xs:` borrows — `collections-and-chains.md` §4.2's
                // AMENDMENT 11 — so the iterable is read and not consumed.
                self.expr(*iter, Ctx::Read);
                self.block(*body, Ctx::Read);
            }
            ExprKind::Block(block) | ExprKind::Unsafe(block) => self.block(*block, ctx),
        }
    }

    fn block(&mut self, id: thir::BlockId, ctx: Ctx) {
        let thir = self.thir;
        let block = thir.block(id);
        for stmt in &block.stmts {
            match &stmt.kind {
                StmtKind::Let { value, .. } | StmtKind::Expr(value) => {
                    self.expr(*value, Ctx::Consume(*value))
                }
                StmtKind::Assign { target, value } => {
                    self.expr(*target, Ctx::Write);
                    self.expr(*value, Ctx::Consume(*value));
                }
                StmtKind::Return(Some(value)) | StmtKind::Break(Some(value)) => {
                    self.expr(*value, Ctx::Consume(*value))
                }
                StmtKind::Assert { cond, message } => {
                    self.expr(*cond, Ctx::Consume(*cond));
                    self.expr(*message, Ctx::Consume(*message));
                }
                StmtKind::Return(None)
                | StmtKind::Break(None)
                | StmtKind::Continue
                | StmtKind::Error => {}
            }
        }
        if let Some(tail) = block.tail {
            self.expr(tail, ctx);
        }
    }
}
