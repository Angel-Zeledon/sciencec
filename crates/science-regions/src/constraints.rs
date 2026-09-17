//! Decision 2: every constraint carries its span and its cause, and nothing
//! is ever merged.
//!
//! > **Decision 2.** Every constraint carries its span and its cause, and the
//! > solver never merges two constraints. … A solver that unions constraints
//! > for speed has thrown away the only material an error message can be built
//! > from, and — this is the part §6.2 does not say — **Science needs the
//! > material more than Rust does**, because Rust can fall back on printing
//! > `'a` and Science has nothing to print.
//!
//! # 1. What is not done here, and it is the whole decision
//!
//! [`Constraints`] is a `Vec` and **it is not deduplicated**. Two assignments
//! that produce the same `'a: 'b` produce two entries, and the solver visits
//! both. That costs one pass over a longer vector per fixpoint iteration and it
//! buys the only thing §7.1's third span can be built out of: *which* statement
//! made the region reach that far.
//!
//! The temptation is real and specific. `propagate` in [`crate::solve`] does
//! the same work twice for a duplicate pair, and a `BTreeSet<(sup, sub, point)>`
//! would remove it in four lines. The four lines would also delete the answer to
//! *"why is this borrow still alive at line 90"*, which is the question §7.1
//! says a solver without provenance cannot answer, and which — unlike the
//! speed — cannot be recovered afterwards.
//!
//! # 2. The cause, which `science-mir` deliberately did not precompute
//!
//! That crate's §5: *"the cause — `AssignedFrom`, `PassedTo`, `ReturnedFrom`,
//! `StoredInField` — is readable off the statement kind and is not pre-computed
//! here, because a cause is a fact about a constraint and there are no
//! constraints in this crate"*. [`Cause`] is those four and three more that
//! the note did not anticipate, each named at its variant.
//!
//! # 3. The point on a constraint is not decoration
//!
//! `'a: 'b @ p` is *location-sensitive*: it says `'a` must contain the part of
//! `'b` that is reachable from `p`, not all of `'b`. That is what makes NLL
//! work at all — a reference assigned into a variable inside a loop body
//! constrains the loan only from that point on — and it is why [`Constraint`]
//! holds a [`Point`] rather than being a bare pair. [`crate::solve`]'s §2 is
//! the propagation that reads it.

use science_diagnostics::Span;
use science_mir::mir::Point;
use science_resolve::hir::{DefId, DefTable};

use crate::regions::RegionVar;

/// Why a constraint exists. §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// `x be y`, where both carry references. §3 step 2's first name.
    AssignedFrom,
    /// A reference handed to a call as an argument.
    PassedTo(DefId),
    /// A reference a call handed back, related to the arguments it may point
    /// into. The callee is `None` for a [`science_mir::mir::Callee::Unresolved`]
    /// and for one with no summary — [`crate::generate`]'s §7 says which way
    /// that errs.
    ReturnedFrom(Option<DefId>),
    /// A reference stored into a field of a record being built. Decision 3's
    /// constraint, and the one `Node of T` is made of.
    StoredInField(DefId),
    /// The loan a `Ref` takes, related to the place it was stored in. The
    /// constraint that makes a region grow along a reference's liveness at all.
    Borrowed,
    /// A reference reaching the return place. Decision 5's whole analysis is
    /// read off the constraints with this cause.
    Returned,
    /// A borrow taken out of a place that is itself reached through a
    /// reference: the loan cannot outlive the reference it was taken through.
    /// Rule 5 for a place with a [`science_mir::mir::Projection::Deref`] in it,
    /// which [`crate::check`]'s §4 says is checked here and not against a
    /// storage-dead point.
    ReborrowedFrom,
    /// A coercion — `science_types::assign::Coercion` — carried through.
    Coerced,
}

impl Cause {
    /// What §7.1's narrative calls this.
    pub fn described(self, defs: &DefTable) -> String {
        match self {
            Cause::AssignedFrom => "assigned from a borrow here".to_string(),
            Cause::PassedTo(def) => format!("passed to `{}` here", defs.get(def).name),
            Cause::ReturnedFrom(Some(def)) => {
                format!("returned by `{}` here", defs.get(def).name)
            }
            Cause::ReturnedFrom(None) => {
                "returned by a call whose signature is not known here".to_string()
            }
            Cause::StoredInField(field) => {
                format!("stored in `{}` here", defs.get(field).name)
            }
            Cause::Borrowed => "the borrow is taken here".to_string(),
            Cause::Returned => "returned here".to_string(),
            Cause::ReborrowedFrom => "borrowed through a reference here".to_string(),
            Cause::Coerced => "coerced here".to_string(),
        }
    }
}

/// `'sup: 'sub @ point` — read as `sub ⊆ sup`, Decision 1's direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    /// The region that must be at least as large.
    pub sup: RegionVar,
    /// The region it must contain, from `point` onwards.
    pub sub: RegionVar,
    /// §3. Where the requirement starts.
    pub point: Point,
    pub cause: Cause,
    pub span: Span,
}

/// Every constraint one body generates, in generation order and undeduplicated.
/// §1.
#[derive(Debug, Clone, Default)]
pub struct Constraints {
    all: Vec<Constraint>,
}

impl Constraints {
    pub fn new() -> Constraints {
        Constraints::default()
    }

    /// Records `'sup: 'sub @ point`.
    pub fn push(
        &mut self,
        sup: RegionVar,
        sub: RegionVar,
        point: Point,
        cause: Cause,
        span: Span,
    ) {
        self.all.push(Constraint { sup, sub, point, cause, span });
    }

    pub fn all(&self) -> &[Constraint] {
        &self.all
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}
