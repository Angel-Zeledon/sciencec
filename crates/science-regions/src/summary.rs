//! Decisions 5, 6 and 7: a signature's regions as an analysis result, and the
//! thing that crosses a crate boundary.
//!
//! # 1. Decision 5, and what "there are no elision rules at all" turns out to
//! mean
//!
//! > **Decision 5.** A function signature's regions are an analysis result, not
//! > a declaration, and there are no elision rules at all.
//!
//! A [`Summary`] is that result. For each reference position in the return
//! type, it records **the set of parameter regions that constrain it** — read
//! off the solved constraint graph by reachability, not guessed from the
//! signature.
//!
//! **The set is the finding, and it is the answer to §5.2.** That section frames
//! ambiguity as *"the signature does not say whether it is borrowed from `x` or
//! `y`"*, and treats it as a failure Rust escapes by making you write `'a`. But
//! *"from `x` or `y`"* is only a failure if the answer has to be **one** name.
//! Here the answer is `{x, y}`, a caller intersects the two regions, and the
//! result is sound, total and — at the cost of precision, never of soundness —
//! never ambiguous. [`crate`]'s §4 is the full account and the measurement.
//!
//! So `SC0340` does not fire on *"`x` or `y`"*. What is left for it is the
//! **empty** set with a non-empty return: a body that returns a reference and
//! does not say where it points. [`Summary::undetermined`] is that predicate
//! and [`crate::check`] is where it becomes a diagnostic.
//!
//! # 2. Decision 7, and the sentence the note already wrote
//!
//! > **Decision 7.** Region inference is interprocedural *within* a crate and
//! > signature-summarised *across* crates. … It is not a lifetime annotation —
//! > nobody writes it and nobody reads it — but it is **exactly the same
//! > information**.
//!
//! [`Summary`] is that record, and the note is right about what it is: a
//! `returns` entry naming one parameter is `fn f<'a>(x: &'a T) -> &'a U` with
//! the name deleted. It is built here and **it is not serialised**, because
//! there is no crate metadata format to serialise it into —
//! `codegen-and-linking.md` owns one and there is none. What that costs is
//! stated at [`Summaries::lookup`]: a callee with no body in this crate is
//! treated exactly as an unresolved one.
//!
//! # 3. What a summary does *not* carry, and why that is a hole
//!
//! **Only the return.** A callee can also store one argument's reference into
//! another argument — `f(out: mutable borrowed Sink of borrowed T, value:
//! borrowed T)` — and a summary with only `returns` would miss it.
//! [`Summary::escapes`] is that half and it is computed the same way, from the
//! parameter regions reachable into a position *underneath a `mutable
//! borrowed` parameter*, which is the only way a callee writes back into a
//! caller's storage.
//!
//! It is exercised by no program in the corpus, which is the honest state of
//! it: the shape needs a container generic over a borrowed element, and §2
//! item 1 of [`crate::regions`] cannot see inside a container. **The two holes
//! cover each other and that is not the same as neither existing.**
//!
//! # 4. A declaration with no body is neither of the two cases above
//!
//! Decision 5 says a signature's regions are *"an analysis result, not a
//! declaration"*, and that is right for a function with a body: the body is
//! better evidence than any annotation could be. The prelude has no bodies.
//! So `Map.get` fell to [`Summary::opaque`] — *"a reference into every argument
//! it was given"* — and [`crate::generate`]'s §7 measured what that costs: two
//! false positives in `examples/`, both of them a map lookup that appeared to
//! borrow its key.
//!
//! > **Decision. A callee that `science-types` declares and no body defines
//! > carries the parameter set its *declaration* implies.**
//!
//! `Declarations::borrow_sources` computes that set and states its own cost.
//! It is deliberately not computed here: *which parameters a return type can
//! point into* is a question about types, and the crate that owns types should
//! answer it. What this crate keeps is what to do with the answer —
//! [`crate::generate`]'s `call` narrows §5's source list to it and changes
//! nothing else, so the escape half stays maximal and the direction §7 argues
//! for is unchanged.
//!
//! **It is not a [`Summary`].** There is no `returns` map and no [`Position`]:
//! a declaration says which *arguments* the result may point into and nothing
//! about the paths beneath them, so the constraint is still §5's reachability
//! over those arguments rather than §1's position-to-position relation.
//! Promoting it to a summary would mean inventing paths no body produced.

use std::collections::BTreeMap;

use science_resolve::hir::DefId;

use crate::regions::{Path, Position};

/// A region belonging to one of a function's parameters.
///
/// `param` is the index into [`science_mir::mir::Body::params`], so `0` is the
/// receiver of a method and the first declared parameter of a free function.
/// An index rather than a [`science_mir::mir::Local`] because a summary
/// outlives the body it was computed from — that is the whole of Decision 7 —
/// and a local numbering is a fact about a body.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ParamRegion {
    pub param: usize,
    pub path: Path,
}

/// What Decision 7 would serialise: a function's region relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub def: DefId,
    /// How many parameters the body had, so a call site with the wrong arity —
    /// which the checker has already reported — cannot index out of range.
    pub params: usize,
    /// For each reference position in the return type, the parameter regions
    /// that constrain it. §1.
    pub returns: Vec<(Position, Vec<ParamRegion>)>,
    /// For each writable position reachable through a parameter, what may be
    /// stored into it. §3.
    pub escapes: Vec<(ParamRegion, Vec<ParamRegion>)>,
    /// True when nothing is known about this function and every caller must
    /// assume the worst. §2.
    pub opaque: bool,
}

impl Summary {
    /// The summary of a function this crate cannot see. §2.
    ///
    /// **Maximal rather than minimal**, and that is the direction the whole
    /// crate errs in: an opaque callee is assumed to return a reference into
    /// every argument it was given. [`crate::generate`]'s §7 states the cost.
    pub fn opaque(def: DefId) -> Summary {
        Summary { def, params: 0, returns: Vec::new(), escapes: Vec::new(), opaque: true }
    }

    /// The bottom of the fixpoint an SCC starts from: no relation at all.
    ///
    /// Sound to start from *because it only grows*. A mutually recursive pair
    /// begins by assuming neither returns a borrow of anything, and every
    /// iteration adds constraints; the fixpoint is the least relation that
    /// explains both bodies. Starting from [`Summary::opaque`] instead would
    /// also terminate and would conclude that every recursive function returns
    /// a borrow of everything, which is sound and useless.
    pub fn empty(def: DefId, params: usize) -> Summary {
        Summary { def, params, returns: Vec::new(), escapes: Vec::new(), opaque: false }
    }

    /// The parameter regions constraining a return position.
    pub fn returns_from(&self, path: &Path) -> &[ParamRegion] {
        self.returns
            .iter()
            .find(|(position, _)| &position.path == path)
            .map(|(_, from)| from.as_slice())
            .unwrap_or(&[])
    }

    /// The return positions the body did not determine. Decision 6's condition,
    /// narrowed to what §1 says is left of it.
    pub fn undetermined(&self) -> Vec<&Position> {
        self.returns
            .iter()
            .filter(|(_, from)| from.is_empty())
            .map(|(position, _)| position)
            .collect()
    }

    /// Whether this summary says strictly more than another — the fixpoint's
    /// termination test.
    pub fn differs_from(&self, other: &Summary) -> bool {
        self.returns != other.returns || self.escapes != other.escapes
    }
}

/// Every summary computed so far, keyed by definition.
///
/// A [`BTreeMap`] and not a `HashMap` for [`science_mir::CallGraph`]'s reason:
/// the iteration order is observable in a dump and §10 item 6 wants an answer
/// that does not move when nothing moved.
#[derive(Debug, Clone, Default)]
pub struct Summaries {
    known: BTreeMap<DefId, Summary>,
    /// For a callee that is **declared and has no body**, the parameters its
    /// returned references may point into. Section 4.
    declared: BTreeMap<DefId, Vec<usize>>,
}

impl Summaries {
    pub fn new() -> Summaries {
        Summaries::default()
    }

    pub fn insert(&mut self, summary: Summary) {
        self.known.insert(summary.def, summary);
    }

    /// Records what a declaration says about its own return. Section 4.
    pub fn declare(&mut self, def: DefId, sources: Vec<usize>) {
        self.declared.insert(def, sources);
    }

    /// The parameters a declared callee's return may borrow from, or `None`
    /// when nothing declared it. Section 4.
    pub fn declared_sources(&self, def: DefId) -> Option<&[usize]> {
        self.declared.get(&def).map(|sources| sources.as_slice())
    }

    /// What a call site should assume about a callee.
    ///
    /// **`None` means "assume the worst"**, and it is returned for three
    /// different situations that a reader should not have to tell apart at the
    /// call site: a function in another crate (Decision 7's serialised summary,
    /// which nothing serialises yet), a builtin with no body, and a callee this
    /// crate has not analysed because it is above the current one in the call
    /// graph and is not in its component — which cannot happen, because
    /// [`science_mir::CallGraph::components`] hands the components over leaves
    /// first, and is checked rather than assumed by
    /// `tests/interprocedural.rs`.
    pub fn lookup(&self, def: DefId) -> Option<&Summary> {
        self.known.get(&def)
    }

    pub fn len(&self) -> usize {
        self.known.len()
    }

    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Summary> {
        self.known.values()
    }
}
