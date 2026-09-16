//! Inference variables, and the union-find over them — Decision 24.
//!
//! `ty`'s §7 left this file the first decision: *"what an inference variable is
//! — a union-find index scoped to one body and discarded when the body is done
//! — is the inference context's first decision, and a variant added before that
//! decision fixes its representation from outside the file that owns it."*
//! Here it is, and it is the opposite of what that sentence expected.
//!
//! # 1. An inference variable is not a type
//!
//! **Decision. There is no `TyKind::Infer`. A variable is an index into
//! [`Inference`], and a type that is not yet known is an [`InferTy`], which
//! exists only inside one body and never enters the type table.**
//!
//! The argument for the variant is real and `ty`'s §7 states it: three arms,
//! all cheap. The argument against is that the table is **per compilation** and
//! a variable is **per body**, and interning would put them in one numbering:
//!
//! - `?0` in one body and `?0` in the next are the same index and therefore
//!   the *same interned type*, so a variable that leaks out of the body that
//!   made it — into a field's type, into a signature, into a
//!   [`MonoKey`](crate::MonoKey) — is not a crash and not a type error. It is a
//!   silently wrong answer, and it is `ty`'s §1 cost — *"a `Ty` means nothing
//!   without the `Types` that made it"* — reached one level down, where there is
//!   no per-compilation table to blame it on.
//! - Or variables are numbered globally and never reused, in which case the
//!   table grows by one node per literal in the program and *"discarded when the
//!   body is done"* becomes false: the nodes stay, interned, forever.
//! - And [`Types::compatible`] would have to answer questions about a variable.
//!   It is the relation everything rests on and it is one line; an arm saying
//!   *"an unbound variable is compatible with anything"* would make it silently
//!   weaker at every call site in the compiler, including the ones that have
//!   nothing to do with inference.
//!
//! Keeping variables out of the table makes all three impossible rather than
//! forbidden. Decision 1 says there is no unifier across function boundaries;
//! this is that sentence made structural.
//!
//! # 2. What it costs: a variable cannot be inside a type
//!
//! **This is the cost, it is the whole cost, and it is not small.** [`InferTy`]
//! is either a known [`Ty`] or a variable — there is no `Array of ?0`. A
//! partially known type cannot be written down.
//!
//! What still works, which is F0's inference as §5.2 and Decision 2 describe
//! it: a literal whose type is not yet fixed, a binding whose type comes from
//! its initialiser, a value flowing through a `match` whose arms are checked in
//! any order. Every one of those is a variable at the *root* of a type, which
//! is what this representation holds.
//!
//! What does not: `let xs be []` with no annotation, where the element type is
//! a hole inside a known constructor. In checking mode there is an expected
//! type and the hole never exists; in synthesis mode the checker must report
//! that it cannot infer, rather than deferring. That is a real restriction on a
//! real program, and it is the one this file buys the three impossibilities
//! above with.
//!
//! **The escape route, stated so the next phase does not have to invent it.**
//! When a nested hole is wanted, the thing to add is *not* `TyKind::Infer` — §1
//! is why — but a second, body-local structure over the same shape: a node
//! whose operands are [`InferTy`] rather than [`Ty`], collapsing into a [`Ty`]
//! once every hole in it is solved. The union-find below does not change, because
//! it is over *variables*, not over types.
//!
//! # 3. There is no occurs check, and today there cannot be one
//!
//! §2 of the note says there is *"no occurs check outside the local inference
//! variables of one body"*, which anticipates one inside. There is none here,
//! and not because it was skipped: a variable cannot occur inside a type in
//! this representation, so `?0 := Array of ?0` is not a thing that can be
//! written. The check arrives with the nested representation of §2 and belongs
//! to it.
//!
//! # 4. `find` takes `mutable self`
//!
//! **Decision 24**, quoted in `ty`'s §2: *"the checker is written
//! index-not-pointer throughout, and union-find takes `mutable self`"*, because
//! `region-inference.md` §9 says `RefCell` has no Science spelling and a `find`
//! that compresses through a shared borrow is a `find` that cannot be ported.
//!
//! So every field here is a `Vec` indexed by the variable's number, `find` is
//! `&mut self`, and the compression it does is **path halving** — each node on
//! the way up is pointed at its grandparent, in one pass, with no second walk
//! and no recursion. With union by size it is the same near-constant bound as
//! full compression and it is four lines.
//!
//! **What it costs** is what Decision 24 said it costs: a caller cannot hold a
//! borrow of the context across a `find`, so a checker that wants the type of
//! two variables at once asks twice and copies. [`Ty`] and [`InferVar`] are both
//! `Copy` and one word, so the copy is the same instruction the borrow would
//! have been.
//!
//! # 5. What is not here
//!
//! - **Defaulting.** Decision 2 makes an unconstrained numeric literal `I64` or
//!   `F64`, and that is a rule about what to do with a variable that is *still
//!   unbound when the body ends* — which means it needs the body's end, the
//!   literal's kind, and the prelude's ids for two primitives. The body is the
//!   next phase's. What this file owes it is [`Inference::unresolved`], which is
//!   the list to walk.
//! - **Diagnostics.** A variable that never resolves is *"type annotations
//!   needed"* and a unification failure is *"mismatched types"*, and both want
//!   the expression, not the variable. [`Inference::origin`] keeps the span each
//!   variable was created at so that the message has somewhere to point.
//! - **Coercion.** [`Inference::unify`] is *equality*, not assignability:
//!   `ty`'s §4 says a coercion is a fact about an assignment, and a unification
//!   is not an assignment. A checker that wants `T` to reach `T?` calls
//!   [`crate::assign::assignable`] with the site in hand. Making `unify` coerce
//!   would make the direction of every inference edge significant and make the
//!   result depend on the order the checker happened to visit two arms in.

use science_diagnostics::Span;

use crate::ty::{Ty, Types};

/// A type the checker has not pinned down yet: an index into [`Inference`].
///
/// One word and `Copy`, like [`Ty`], and like [`Ty`] it means nothing without
/// the context that made it — §1. Unlike [`Ty`] the context is discarded at the
/// end of the body, so a variable that outlives it is a bug with no symptom;
/// that is the trade §1 takes and states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InferVar(u32);

impl InferVar {
    /// The variable's number, for a dump and for a side table.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A type in progress: known, or a variable standing for one.
///
/// §2 is the whole of the shape: there is no third case, and in particular no
/// case with a variable *inside* a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferTy {
    /// An interned type. The common case, and the only one a finished body has.
    Known(Ty),
    /// A hole.
    Var(InferVar),
}

impl InferTy {
    /// The type, when it is known.
    pub fn known(self) -> Option<Ty> {
        match self {
            InferTy::Known(ty) => Some(ty),
            InferTy::Var(_) => None,
        }
    }
}

impl From<Ty> for InferTy {
    fn from(ty: Ty) -> InferTy {
        InferTy::Known(ty)
    }
}

/// Two types that had to be one and were not.
///
/// Carries both so the caller can render them; it carries no span, because the
/// spans that matter belong to the two *expressions*, which this context has
/// never seen. §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifyError {
    /// Neither type is the error type and they are not equal.
    Mismatch { left: Ty, right: Ty },
}

/// The inference context for **one body**.
///
/// Created when a body is entered and dropped when it is left. Nothing it
/// produces may outlive it except a [`Ty`], which belongs to the table and not
/// to this.
#[derive(Debug, Clone, Default)]
pub struct Inference {
    /// `parent[i] == i` marks a root. Index-not-pointer, Decision 24.
    parent: Vec<u32>,
    /// The size of the tree under each root, for union by size. Meaningless at
    /// a non-root, which is why nothing reads it there.
    size: Vec<u32>,
    /// What the *root* of each class is bound to, if anything. A non-root's
    /// entry is stale by construction: [`Inference::binding`] goes through
    /// [`Inference::find`] first, and that is the only reader.
    binding: Vec<Option<Ty>>,
    /// Where each variable was made, for the message §5 leaves to the next
    /// phase.
    origin: Vec<Span>,
}

impl Inference {
    /// An empty context. One per body.
    pub fn new() -> Inference {
        Inference::default()
    }

    /// A fresh variable, standing for the type of the expression at `span`.
    pub fn fresh(&mut self, span: Span) -> InferVar {
        let index = self.parent.len() as u32;
        self.parent.push(index);
        self.size.push(1);
        self.binding.push(None);
        self.origin.push(span);
        InferVar(index)
    }

    /// How many variables this body has made.
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Where the variable was made.
    pub fn origin(&self, var: InferVar) -> Span {
        self.origin[var.index()]
    }

    /// The representative of the variable's class, compressing on the way. §4.
    ///
    /// `&mut self` is Decision 24 and is the reason this method is not `&self`
    /// with a cell inside.
    pub fn find(&mut self, var: InferVar) -> InferVar {
        let mut current = var.0;
        while self.parent[current as usize] != current {
            // Path halving: point at the grandparent and step to it. One pass,
            // no recursion, no second walk to fix up.
            let grandparent = self.parent[self.parent[current as usize] as usize];
            self.parent[current as usize] = grandparent;
            current = grandparent;
        }
        InferVar(current)
    }

    /// What the variable's class is bound to, if it is bound.
    pub fn binding(&mut self, var: InferVar) -> Option<Ty> {
        let root = self.find(var);
        self.binding[root.index()]
    }

    /// The variable resolved as far as it goes: a type, or the representative
    /// of its class.
    pub fn resolve(&mut self, ty: InferTy) -> InferTy {
        match ty {
            InferTy::Known(known) => InferTy::Known(known),
            InferTy::Var(var) => match self.binding(var) {
                Some(known) => InferTy::Known(known),
                None => InferTy::Var(self.find(var)),
            },
        }
    }

    /// Binds the variable's class to a type.
    ///
    /// A class that is already bound is unified with what it is bound to rather
    /// than overwritten, so binding twice is a mismatch and not a silent
    /// replacement of the first answer by the second.
    pub fn bind(&mut self, types: &Types, var: InferVar, ty: Ty) -> Result<Ty, UnifyError> {
        let root = self.find(var);
        match self.binding[root.index()] {
            None => {
                self.binding[root.index()] = Some(ty);
                Ok(ty)
            }
            Some(existing) => {
                let settled = agree(types, existing, ty)?;
                self.binding[root.index()] = Some(settled);
                Ok(settled)
            }
        }
    }

    /// Makes two types one, or says they are not.
    ///
    /// **Equality, not assignability** — §5. The result is what the two are
    /// now: a type when either side knew one, and the class's representative
    /// when neither did.
    pub fn unify(
        &mut self,
        types: &Types,
        left: InferTy,
        right: InferTy,
    ) -> Result<InferTy, UnifyError> {
        match (left, right) {
            (InferTy::Known(a), InferTy::Known(b)) => Ok(InferTy::Known(agree(types, a, b)?)),
            (InferTy::Var(var), InferTy::Known(ty)) | (InferTy::Known(ty), InferTy::Var(var)) => {
                Ok(InferTy::Known(self.bind(types, var, ty)?))
            }
            (InferTy::Var(a), InferTy::Var(b)) => {
                let (left_root, right_root) = (self.find(a), self.find(b));
                if left_root == right_root {
                    return Ok(self.resolve(InferTy::Var(left_root)));
                }
                let settled = match (
                    self.binding[left_root.index()],
                    self.binding[right_root.index()],
                ) {
                    (Some(x), Some(y)) => Some(agree(types, x, y)?),
                    (Some(x), None) => Some(x),
                    (None, Some(y)) => Some(y),
                    (None, None) => None,
                };
                let root = self.union(left_root, right_root);
                self.binding[root.index()] = settled;
                Ok(match settled {
                    Some(ty) => InferTy::Known(ty),
                    None => InferTy::Var(root),
                })
            }
        }
    }

    /// Every class that is still unbound, one entry per class.
    ///
    /// This is the list Decision 2's defaulting walks and the list *"type
    /// annotations needed"* is reported from — one message per class, because a
    /// class is one unknown however many expressions joined it. §5.
    pub fn unresolved(&mut self) -> Vec<InferVar> {
        let mut roots = Vec::new();
        for index in 0..self.parent.len() as u32 {
            let root = self.find(InferVar(index));
            if root.0 == index && self.binding[root.index()].is_none() {
                roots.push(root);
            }
        }
        roots
    }

    /// Union by size. Both arguments must be roots.
    fn union(&mut self, left: InferVar, right: InferVar) -> InferVar {
        let (larger, smaller) = if self.size[left.index()] >= self.size[right.index()] {
            (left, right)
        } else {
            (right, left)
        };
        self.parent[smaller.index()] = larger.0;
        self.size[larger.index()] += self.size[smaller.index()];
        larger
    }
}

/// Two known types, made one.
///
/// [`Types::compatible`] is the relation — structural equality, with an
/// already-reported type agreeing with whatever it meets. When one side is
/// erroneous the *other* is kept, so that a mistake in one place does not
/// spread the error type through every variable it touches: the class ends up
/// standing for the type that is still known, and the one diagnostic that was
/// already reported stays one.
fn agree(types: &Types, left: Ty, right: Ty) -> Result<Ty, UnifyError> {
    if left == right {
        return Ok(left);
    }
    if types.references_error(left) {
        return Ok(right);
    }
    if types.references_error(right) {
        return Ok(left);
    }
    Err(UnifyError::Mismatch { left, right })
}
