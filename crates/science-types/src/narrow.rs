//! Flow narrowing — Decision 7, and the half of the error model that shipped
//! without its meaning.
//!
//! `syntax-revision-2.md` §3.1 requires that inside `if err?:` the compiler
//! knows `err` is not null, and §10 of that note says the model *"lands worse
//! than what it replaces"* without it. The syntax has been in the compiler
//! since revision 2 landed; this is the meaning.
//!
//! # 1. A fact about a place, not a type of an expression
//!
//! > **Decision 7.** Narrowing is a flow-sensitive fact about a *place*,
//! > computed over THIR, and it is invalidated by any write to that place or to
//! > a prefix of it.
//!
//! **Decision. The fact is held about a [`Place`] and nothing else, and the
//! only thing it does is change the type of a *read*.** Two consequences, and
//! the second is the reason:
//!
//! - `config.port?` narrows `config.port` and not `config`, because they are
//!   two places. [`Place::starts_with`] is the whole of *"or to a prefix of
//!   it"*: writing `config` invalidates the fact about `config.port`; writing
//!   `config.other` does not.
//! - **A narrowed read is a node** — [`ExprKind::Narrow`]
//!   — not a rewritten type on the [`ExprKind::Local`]
//!   node. `thir`'s §1 is why: dropping a discriminant is a representation
//!   change, and MIR needs the point at which the narrowing was used.
//!
//! **What it costs** is that nothing which is not a place can be narrowed.
//! `f()?` tells you nothing about the next `f()`, `a[i]?` tells you nothing
//! about `a[i]`, and both are right: the first calls twice, and the second is
//! `thir`'s §3 — an index place needs a value, and a place that compares equal
//! when the values behind it differ is unsoundness in the direction §4.2 says
//! the meet must never go.
//!
//! # 2. The rules, one for one with §4.2
//!
//! | §4.2 | Here |
//! |---|---|
//! | `if err?:` narrows to non-null in the then-branch and to null in the else | [`condition`], [`ExprKind::Present`] |
//! | `not` composes | [`condition`], which swaps the two halves |
//! | `and` narrows both operands in the then-branch | [`condition`], `Outcome::when_true` is the union |
//! | `or` narrows neither | [`condition`], and §5 — the note stated one half of this and the other half is derivable |
//! | an early `return` inside `if err?:` narrows `err` to null for the rest of the function | [`Facts::join`] with a diverging branch, in [`crate::check`] |
//! | `config.port?` narrows `config.port`, not `config` | [`Place`], §1 |
//! | a loop's meet is intersection | §3 |
//!
//! # 3. The loop, and why one pass is enough
//!
//! §4.2: *"A narrowing established before a loop does not survive the back edge
//! unless it survives every path through the body. This is a standard forward
//! dataflow with a meet at the loop head, and getting it wrong in the
//! permissive direction is unsound, so the meet is intersection."*
//!
//! **Decision. The meet is computed by invalidating, before the body is
//! entered, every fact rooted at a binding the body could write — and the body
//! is then checked once.** [`clobbered_roots`] is that set, and it is read off
//! the *HIR*, before the body has been checked, because the pre-invalidation
//! has to happen before the first read inside the loop is typed.
//!
//! The argument that one pass is a fixed point: a fact surviving into the body
//! is one whose place the body cannot write, so it is true on every back edge;
//! a fact the body *establishes* is dropped rather than propagated, which is
//! the intersection erring in the safe direction. No iteration converges on
//! anything the second pass would find, because the second pass can only
//! subtract and everything subtractable was subtracted first.
//!
//! **What it costs is precision, in one specific and visible way.** The set is
//! over *roots*, not places: `loop: config.other = compute()` invalidates the
//! narrowing of `config.port` as well, because the roots are read off HIR where
//! a field is still an `Ident` and the `DefId` that would tell `port` from
//! `other` is exactly what the type checker has not computed yet. A
//! field-precise version needs the body checked before the loop head is
//! decided, which is the two-pass structure this decision refuses. The refusal
//! is cheap to reverse and the direction is the safe one.
//!
//! # 4. Decision 8, kept honest
//!
//! > **Decision 8: narrowing relies on rule 4 and records the dependency.** An
//! > exclusive borrow of `x` is the only way to write `x`, rule 4 forbids it
//! > while any other borrow is live, and the narrowing is invalidated at the
//! > point the exclusive borrow is *created*, not where it writes.
//!
//! [`Facts::invalidate`] is called from two places in [`crate::check`] and the
//! second one is that sentence: at an
//! [`ExprKind::Borrow`] with `mutable: true`
//! over a place, not at any write through it — because this phase cannot see
//! the writes, and rule 4 is what says it does not have to.
//!
//! **This is a phase-ordering argument and not a proof, and §15 of the note
//! records it as one.** The region checker runs *after* this phase and proves
//! rule 4; what makes the order legal is that narrowing's *conclusion* is not
//! consumed until after regions have run (Decision 25). Nothing here upgrades
//! that to a guarantee, and a future THIR pass that consumes narrowing for
//! something codegen depends on makes it silently false. The one place this
//! crate could have quietly relied on it — `SC0140` — does not:
//! [`crate::unchecked`]'s §2 reads the THIR's *structure*, not its narrowing.
//!
//! **And the conservatism Decision 11 retired.** A method call may take
//! `mutable self`, and until [`crate::methods`] existed there was no way to ask
//! whether this one does — so every method call invalidated its receiver, and
//! `if doc?: doc.title()` narrowed nothing after it. The lookup answers the
//! question now: [`Candidate::writes_receiver`](crate::methods::Candidate::writes_receiver)
//! is the one bit of the signature this decision needs, and
//! [`crate::check`]'s `method_call` invalidates on `mutable self` and on
//! nothing else. A `self` or `borrowed self` method keeps the narrowing, which
//! is what the rule always should have said.
//!
//! **What survives is the unresolved call**, and it is the same
//! conservatism with a smaller domain: a receiver whose type this crate holds
//! no implementations for — a prelude type, a type parameter — resolves to no
//! candidate, there is still no `SelfKind` to read, and the receiver is still
//! invalidated. `methods`'s §5 names what closing that needs, and it is a
//! method on a *bound* rather than anything narrowing owns.
//!
//! # 5. What §4.2 left underdetermined
//!
//! **`or`.** The note says *"`or` narrows neither, because either may be the
//! reason"*, next to *"`and` narrows both operands in the then-branch"* — so
//! the sentence is about the *then*-branch, and it is right there. The
//! else-branch of an `or` is not mentioned at all, and it is the exact dual: if
//! `a? or b?` is false then both are false, so both are narrowed to null.
//! [`condition`] implements the dual, because leaving it out would make `not (a?
//! or b?)` weaker than the `not a? and not b?` that means the same thing, and a
//! narrowing that depends on which of two equivalent spellings the author chose
//! is worse than either answer.
//!
//! **A comparison against `null`.** `a == null` is not a narrowing form here.
//! `?` is the spelling revision 2 chose and the note names no other; admitting
//! a second one is a language decision, and the direction that can be reversed
//! is to refuse it.

use std::collections::BTreeMap;

use science_resolve::hir::{self, DefId, Res};

use crate::thir::{Body, ExprId, ExprKind, Place};

/// What is known about a place on this path.
///
/// Two variants and no third: there is no "maybe", because an absent entry is
/// the maybe and a table that spelled it would have two ways to say the same
/// thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fact {
    /// The place holds a value. This is the one that changes a read's type.
    NonNull,
    /// The place is `null`. It changes no type — there is no type inhabited
    /// only by `null` — and it is kept because `SC0140`'s third exclusion and
    /// the else-branch of every test are written in terms of it.
    Null,
}

/// What is known about every place, on one path.
///
/// A `BTreeMap` rather than a `HashMap` so that iteration order is the places'
/// own order, which makes anything derived from it — a dump, a diagnostic that
/// lists what it knows — the same twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    known: BTreeMap<Place, Fact>,
}

impl Facts {
    /// Nothing known. The state at the top of a body.
    pub fn new() -> Facts {
        Facts::default()
    }

    /// What is known about this place.
    pub fn get(&self, place: &Place) -> Option<Fact> {
        self.known.get(place).copied()
    }

    /// Records a fact, replacing whatever was known.
    pub fn set(&mut self, place: Place, fact: Fact) {
        self.known.insert(place, fact);
    }

    /// How many places are known about.
    pub fn len(&self) -> usize {
        self.known.len()
    }

    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Forgets everything known about this place and everything projected from
    /// it. §1, and Decision 7's *"or to a prefix of it"*.
    pub fn invalidate(&mut self, place: &Place) {
        self.known.retain(|known, _| !known.starts_with(place));
    }

    /// Forgets everything rooted at this binding. §3's loop meet.
    pub fn invalidate_root(&mut self, root: DefId) {
        self.known.retain(|known, _| known.root != root);
    }

    /// Adds everything the other side knows, overwriting on a clash.
    ///
    /// Used where two conditions must *both* hold — the two operands of an
    /// `and` in a then-branch. A clash cannot arise from [`condition`], because
    /// a single expression cannot narrow one place two ways, and overwriting is
    /// the answer that keeps the later test's word rather than the earlier
    /// one's.
    pub fn absorb(&mut self, other: &Facts) {
        for (place, fact) in &other.known {
            self.known.insert(place.clone(), *fact);
        }
    }

    /// The meet: what both paths agree on.
    ///
    /// Intersection, which §4.2 requires in as many words — *"getting it wrong
    /// in the permissive direction is unsound, so the meet is intersection"*.
    pub fn meet(&self, other: &Facts) -> Facts {
        let mut known = BTreeMap::new();
        for (place, fact) in &self.known {
            if other.known.get(place) == Some(fact) {
                known.insert(place.clone(), *fact);
            }
        }
        Facts { known }
    }

    /// The state after an `if`, given whether each branch fell through.
    ///
    /// This is §4.2's third rule and it is the one that *"makes the Go model
    /// bearable"*: a branch that diverges contributes nothing, so
    /// `if err?: return err` leaves the else-branch's facts — `err` is null —
    /// standing for the rest of the function.
    pub fn join(
        then_facts: Facts,
        then_diverges: bool,
        else_facts: Facts,
        else_diverges: bool,
    ) -> Facts {
        match (then_diverges, else_diverges) {
            (true, true) => Facts::new(),
            (true, false) => else_facts,
            (false, true) => then_facts,
            (false, false) => then_facts.meet(&else_facts),
        }
    }

    /// Every place known about, in order. For a dump and for a test.
    pub fn iter(&self) -> impl Iterator<Item = (&Place, Fact)> {
        self.known.iter().map(|(place, fact)| (place, *fact))
    }
}

/// What a condition tells the two branches it guards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// What holds where the condition is true.
    pub when_true: Facts,
    /// What holds where it is false.
    pub when_false: Facts,
}

impl Outcome {
    /// The two halves the other way round. `not`, and nothing else. §2.
    fn inverted(self) -> Outcome {
        Outcome { when_true: self.when_false, when_false: self.when_true }
    }
}

/// What this condition narrows, in each direction. §2 is the table.
///
/// Reads the *checked* condition, because a place is a THIR notion (`thir`'s
/// §3) and because the operands have already been typed by the time a branch
/// needs this.
pub fn condition(body: &Body, cond: ExprId) -> Outcome {
    match &body.expr(cond).kind {
        // `err?`. The place is the operand's, which sees through a narrow and
        // not through a coercion — so `(err as T)?` narrows nothing.
        ExprKind::Present(operand) => match body.place_of(*operand) {
            Some(place) => {
                let mut when_true = Facts::new();
                let mut when_false = Facts::new();
                when_true.set(place.clone(), Fact::NonNull);
                when_false.set(place, Fact::Null);
                Outcome { when_true, when_false }
            }
            None => Outcome::default(),
        },
        ExprKind::Unary { op: hir::UnaryOp::Not, operand } => condition(body, *operand).inverted(),
        ExprKind::Binary { op: hir::BinaryOp::And, lhs, rhs } => {
            let (left, right) = (condition(body, *lhs), condition(body, *rhs));
            // True means both were true. False means at least one was, and
            // which one is not knowable here.
            let mut when_true = left.when_true;
            when_true.absorb(&right.when_true);
            Outcome { when_true, when_false: Facts::new() }
        }
        ExprKind::Binary { op: hir::BinaryOp::Or, lhs, rhs } => {
            let (left, right) = (condition(body, *lhs), condition(body, *rhs));
            // The dual, and §5 is the argument for having it at all.
            let mut when_false = left.when_false;
            when_false.absorb(&right.when_false);
            Outcome { when_true: Facts::new(), when_false }
        }
        // A parenthesised condition is not a node — the parser drops the
        // parentheses — and a block's value is not a place, so there is
        // nothing further to see through.
        _ => Outcome::default(),
    }
}

/// Every binding a loop body could write, read off the HIR. §3.
///
/// Read off the HIR and not the THIR because the pre-invalidation happens
/// before the body is checked, and read as *roots* rather than places for the
/// reason §3 prices: a field is still an `Ident` here.
///
/// Two things count as a write, and the second is Decision 8:
///
/// - an assignment whose target is rooted at the binding;
/// - the creation of an exclusive borrow of it, `mutable borrowed x`.
///
/// A `let` inside the body is not a write: it introduces a *different*
/// binding, with a `DefId` of its own, which no fact from outside the loop can
/// be about.
pub fn clobbered_roots(block: &hir::Block) -> Vec<DefId> {
    let mut roots = Vec::new();
    walk_block(block, &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

fn walk_block(block: &hir::Block, out: &mut Vec<DefId>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            hir::StmtKind::Let(binding) => walk_expr(&binding.value, out),
            hir::StmtKind::Expr(expr) => walk_expr(expr, out),
            hir::StmtKind::Assign { target, value } => {
                if let Some(root) = root_of(target) {
                    out.push(root);
                }
                walk_expr(target, out);
                walk_expr(value, out);
            }
            hir::StmtKind::Return(value) | hir::StmtKind::Break(value) => {
                if let Some(value) = value {
                    walk_expr(value, out);
                }
            }
            hir::StmtKind::Continue | hir::StmtKind::Error => {}
        }
    }
    if let Some(tail) = &block.tail {
        walk_expr(tail, out);
    }
}

fn walk_expr(expr: &hir::Expr, out: &mut Vec<DefId>) {
    match &expr.kind {
        hir::ExprKind::Borrowed { mutable: true, expr: inner } => {
            if let Some(root) = root_of(inner) {
                out.push(root);
            }
            walk_expr(inner, out);
        }
        // A method call may take `mutable self` and this phase cannot tell.
        // §4's conservatism, applied to the loop head as well as to the
        // straight-line case, so that the two agree.
        hir::ExprKind::MethodCall { receiver, args, .. } => {
            if let Some(root) = root_of(receiver) {
                out.push(root);
            }
            walk_expr(receiver, out);
            for arg in args {
                walk_expr(&arg.value, out);
            }
        }
        hir::ExprKind::Borrowed { expr: inner, .. }
        | hir::ExprKind::Field { base: inner, .. }
        | hir::ExprKind::Unary { operand: inner, .. }
        | hir::ExprKind::Cast { expr: inner, .. }
        | hir::ExprKind::Present(inner)
        | hir::ExprKind::Closure { body: inner, .. } => walk_expr(inner, out),
        hir::ExprKind::Call { callee, args } => {
            walk_expr(callee, out);
            for arg in args {
                walk_expr(&arg.value, out);
            }
        }
        hir::ExprKind::Index { base, index } => {
            walk_expr(base, out);
            walk_expr(index, out);
        }
        hir::ExprKind::StructLit { fields, .. } => {
            for field in fields {
                walk_expr(&field.value, out);
            }
        }
        // A tuple and an array literal are the same walk: a sequence of
        // ordinary operand positions, none of which invalidates anything by
        // being one. What is being looked for is inside the elements.
        hir::ExprKind::Tuple(elements) | hir::ExprKind::ArrayLit(elements) => {
            for element in elements {
                walk_expr(element, out);
            }
        }
        hir::ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, out);
            walk_expr(rhs, out);
        }
        hir::ExprKind::Range { start, end, .. } => {
            walk_expr(start, out);
            walk_expr(end, out);
        }
        hir::ExprKind::If(if_expr) => {
            walk_expr(&if_expr.cond, out);
            walk_block(&if_expr.then_branch, out);
            if let Some(otherwise) = &if_expr.else_branch {
                walk_expr(otherwise, out);
            }
        }
        hir::ExprKind::Match(match_expr) => {
            walk_expr(&match_expr.scrutinee, out);
            for arm in &match_expr.arms {
                walk_expr(&arm.body, out);
            }
        }
        hir::ExprKind::Loop { body } => walk_block(body, out),
        hir::ExprKind::For { iter, body, .. } => {
            walk_expr(iter, out);
            walk_block(body, out);
        }
        hir::ExprKind::Unsafe(block) | hir::ExprKind::Block(block) => walk_block(block, out),
        // A hole may hold a method call, which may take `mutable self`, so an
        // interpolation is walked like any other compound expression. §1.6
        // makes the *interpolation* borrow rather than move, which is a fact
        // about the operand and not about what the operand does.
        hir::ExprKind::FString(parts) => {
            for part in parts {
                if let hir::FStringPart::Hole(expr) = part {
                    walk_expr(expr, out);
                }
            }
        }
        hir::ExprKind::Literal(_)
        | hir::ExprKind::Path { .. }
        | hir::ExprKind::SelfValue(_)
        | hir::ExprKind::Each(_)
        | hir::ExprKind::Unit
        | hir::ExprKind::Error => {}
    }
}

/// The binding an assignment target or a borrowed expression is rooted at.
///
/// The HIR mirror of [`Body::place_of`], and deliberately only the root: §3
/// says why the field chain is not available here and what the imprecision
/// costs.
fn root_of(expr: &hir::Expr) -> Option<DefId> {
    match &expr.kind {
        hir::ExprKind::Path { res: Res::Def(def), .. } => Some(*def),
        hir::ExprKind::SelfValue(Res::Def(def)) => Some(*def),
        hir::ExprKind::Field { base, .. } | hir::ExprKind::Index { base, .. } => root_of(base),
        _ => None,
    }
}
