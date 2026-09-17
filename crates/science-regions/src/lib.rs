//! `science-regions` — ownership, checked, with no lifetime syntax.
//!
//! `region-inference.md` §0 says this is *"on the bootstrap's critical path"*
//! and that the language's single largest claim is one sentence of §6.1 of the
//! core spec: *"There is no lifetime syntax. The programmer never writes a
//! region."* That note is the design. This crate is the engine, run against the
//! file the note nominated as its acceptance case, and §§4–8 below are what
//! happened.
//!
//! | Decision | Here | Where |
//! |---|---|---|
//! | 1. A region is a set of MIR points, ordered by inclusion | **Yes** | [`points`] |
//! | 2. Every constraint carries its span and its cause; nothing is merged | **Yes** | [`constraints`] |
//! | 3. One region variable per borrowed field, related at each use site | **Yes** | [`regions`]'s §1, [`generate`]'s §3 |
//! | 4. A type may not be generic over a region | **Yes**, by omission | [`regions`]'s §2 item 2 |
//! | 5. A signature's regions are an analysis result | **Yes** | [`analysis::summarise`] |
//! | 6. An undetermined signature is `SC0340` | **Partly** — the condition is narrower than §5.2 expects | §4 below |
//! | 7. Interprocedural within a crate, summarised across | **Half** — the summary is built and nothing serialises it | [`summary`]'s §2 |
//! | 8. The query boundary is the SCC | **Half** — the order is honoured, the cache is `science-db`'s | [`analyse_crate`] |
//! | 9. Three spans, never a relation between variables | **Yes** | [`check`]'s §3 |
//! | 10. `unsafe` and indices are the escape hatch | **Not touched** — nothing here reads an `unsafe` marker, because MIR carries none | §8 below |
//! | 11. Index-not-pointer throughout | **Yes** | §9 below |
//!
//! # 1. What is in here
//!
//! | §3 step | What it is | Where |
//! |---|---|---|
//! | 1 | liveness, backward over the CFG | [`liveness`] |
//! | 2 | the forward walk that emits constraints | [`generate`] |
//! | 3 | the worklist fixpoint | [`solve`] |
//! | 4 | the conflict check, **over accesses and not only borrows** | [`check`] |
//! | — | what a place's regions are, per Decision 3 | [`regions`] |
//! | — | what Decision 7 would serialise | [`summary`] |
//! | — | the per-body record all three consumers read | [`analysis`] |
//! | — | a textual dump, for tests | [`dump`] |
//!
//! # 2. Diagnostics
//!
//! Four codes, all from the block §12 claims: `SC0330`, `SC0333`, `SC0334`,
//! `SC0340`, plus `SC0335` which is implemented and has never fired (§5).
//! [`codes`] says what is *not* allocated and why, and the free block
//! `SC0300`, `SC0303`–`SC0329`, `SC0399` is as free after this crate as before
//! it.
//!
//! # 3. What happened on `examples/21_compiler_shapes.science`
//!
//! The note's §14 names the cheapest early signal: *"if it needs `SC0340`
//! anywhere, the bet is in trouble"*. `tests/acceptance.rs` is that run, and
//! the result is that **the file passes with no diagnostic of any kind**.
//!
//! The numbers, so that a later change to any of them is visible: eighteen
//! bodies, 353 points, twelve loans, **50 region variables and 22
//! constraints**, four signatures with a region in the return
//! (`DefTable.get`, `Parser.new`, `Parser.peek`, `Node.new`), no strongly
//! connected component needing a second pass, and a solver fixpoint that
//! converges in two sweeps on the worst body.
//!
//! Three things have to be said about that before it is worth anything.
//!
//! 1. **Its §4 — `Node of T`, *"the case the whole claim turns on"* — is
//!    genuinely exercised and genuinely passes.** Two borrowed fields get two
//!    region variables, the construction ties each to its own parameter, and
//!    nothing anywhere unifies them. Rust's single `'a` there was, as §4.3
//!    says, *"a convenience that discarded information"*, and the inferred
//!    answer keeps it. This is the note's central bet and it is won on this
//!    program.
//! 2. **Sixteen of its calls have no callee**, and [`generate`]'s §5 says what
//!    this engine assumes about one. The assumption is load-bearing in a way
//!    the note did not anticipate: `DefTable.get`'s body is
//!    `self.defs.get(id.index)`, whose only call is an unresolved `Array`
//!    method, and the *only* reason its return is tied to `self` is the rule
//!    that an opaque callee may hand back a reference into anything reachable
//!    from its arguments — including through the `self` the argument was
//!    projected out of. **Decision 6's verdict on this file is currently being
//!    delivered by a conservatism about a hole**, and when `stdlib-core.md`'s
//!    `Array` becomes a declaration, that path is replaced by a real signature.
//!    It should give the same answer. Nobody has checked, because there is no
//!    declaration to check against.
//! 3. **The file exercises no closure and no interior mutability**, so §9's
//!    three unsupported shapes are not tested by it. Closures are no longer a
//!    hole — §6 — but the corpus cannot say so: every closure in `examples/`
//!    names only its own subject and captures nothing (`science-mir`'s
//!    `capture`'s §4), so `tests/closure_captures.rs` carries that burden on
//!    fixtures written for it, exactly as `tests/drops.rs` carries Decision
//!    26's.
//!
//! # 4. Is elision total? Yes — and §5.2 asks the wrong question
//!
//! Decision 5 removes elision rules entirely and §5 says the replacement *"has
//! to be total"*. It is, and the reason is not the one §5.2 expects.
//!
//! §5.2 frames the failure as *"the signature does not say whether it is
//! borrowed from `x` or `y`"*, and treats that as a genuine ambiguity because
//! Rust's answer has to be **one** name. Here the answer is not a name. It is
//! the **set** of parameter regions the constraint graph says must outlive the
//! result, a caller intersects them, and *"from `x` or `y`"* is `{x, y}` — a
//! perfectly good signature, sound, and less precise than either alternative
//! rather than wrong. The ambiguity §5.2 describes **is an artefact of having
//! one name to write, not of not having syntax.** Removing the syntax removed
//! the problem it was introduced to solve.
//!
//! So `SC0340` does not fire on *"`x` or `y`"*. What is left for it is the
//! **empty** set: a body that returns a reference and constrains it by nothing
//! at all. Four things could produce one, and three of them turn out to be
//! something else:
//!
//! - returning a borrow of a local is `SC0333`, and [`check`]'s §5 reports it
//!   as that;
//! - a reference from a call with no reference-carrying argument —
//!   [`generate`]'s §7's conservatism finds a source whenever the call has one
//!   to find, which in practice it does;
//! - **a reference that arrives through a container or a `for` loop.** This is
//!   the one that actually happens — four bodies in `examples/` — and it is
//!   not ambiguity: `Array.get` has no declaration, so the binding is typed
//!   `Ty::ERROR`, so the reference is *gone* rather than unconstrained.
//!   [`check`]'s §6 suppresses it, `tests/corpus.rs` counts it, and it closes
//!   when the containers do;
//! - **a function that cannot return.** Two mutually recursive functions whose
//!   only path to a result is each other converge, correctly, on *"the result
//!   borrows from nothing"*.
//!
//! **That last one is the whole of `SC0340`'s reachable domain today**, and it
//! is worth stating plainly against §14: the note's fear was that
//! *"the body does not determine the signature"* would *"fire in ordinary code
//! rather than on pathological signatures"*. Across `examples/` and every
//! fixture in this crate's suite, the only program that reaches it is one that
//! never produces a value at all.
//!
//! **The honest summary: elision is total, and it is total because the answer
//! was allowed to be a set.** The cost is precision — a result constrained by
//! two parameters is usable only where both are — and precision, unlike
//! totality, is not what the language's claim was about. §14's bet looks
//! **safer than the note thinks**, and for a reason the note does not give.
//!
//! **What would falsify this.** Two of the four routes above are currently
//! blocked by holes rather than answered: the container route is suppressed,
//! and [`generate`]'s §7 supplies a source for opaque calls that a real
//! signature might not. When `stdlib-core.md`'s containers land, both change at
//! once, and `tests/corpus.rs` is written so that the numbers move visibly.
//!
//! # 5. `SC0335` cannot fire, and Decision 3's second cost is misstated
//!
//! §4.3 lists three costs of Decision 3 and the second is: *"the intersection
//! can be empty, and an empty region means the value is dead at birth. That is
//! a legal outcome of the solver and a terrible error message, so §7.3 makes it
//! a named diagnostic"*.
//!
//! **In a solver with no upper bounds it is not a legal outcome.** Regions here
//! only ever grow, from a lower bound that includes every point where the value
//! is live (Decision 1, [`solve`]'s §3). A value's field regions therefore each
//! contain every point at which the value itself is live, including the point
//! it was constructed at — so the intersection contains that point and is never
//! empty. Nothing can empty it, because nothing ever removes a point from a
//! region. What §4.3 describes as an empty intersection surfaces instead as
//! `SC0333` against whichever source dies first, which is the better message
//! anyway: it names the field and the scope rather than the geometry.
//!
//! [`codes::NO_COMMON_REGION`] is implemented and checked at every record
//! construction regardless, because the argument above is about the shape of
//! the solver rather than about programs, and a later upper bound — Polonius,
//! or a region-generic type — falsifies it in silence.
//!
//! # 6. `type-checking-and-mir.md` Decision 8 survives, for a different reason
//! than it gives, and one hole is left
//!
//! > **Decision 8: narrowing relies on rule 4 and records the dependency.** An
//! > exclusive borrow of `x` is the only way to write `x`, rule 4 forbids it
//! > while any other borrow is live, and the narrowing is invalidated at the
//! > point the exclusive borrow is *created*, not where it writes.
//!
//! That note's §15 calls this *"a phase-ordering argument, not a proof"* and
//! names the way it could go wrong: a future THIR pass consuming narrowing for
//! something codegen depends on. **That is not the most immediate risk, and the
//! argument has a load-bearing step neither note states.**
//!
//! The dependency is not on rule 4 as §3 step 4 *writes* it. Consider a program
//! where the exclusive borrow is created **before** the narrowing:
//!
//! ```text
//! let r be mutable borrowed config
//! if config.port?:                    # narrowing established, after the borrow
//!     write_through(r)                # writes config.port; creates no borrow
//!     print(config.port.length())     # reads a fact that is now false
//! ```
//!
//! Invalidation-at-creation does not help — the creation is in the past. Rule 4
//! has to be what refuses it, and what rule 4 must refuse is the *read* of
//! `config.port` at a point where `r`'s region is live. **§3 step 4 as written
//! quantifies over borrows and says nothing about reads**, so an implementation
//! faithful to the note's own sentence accepts this program and Decision 8 is
//! silently false. [`access`]'s §1 is the amendment and `tests/narrowing.rs`
//! is the program, refused with `SC0330`.
//!
//! **So the argument survives, and the amendment is owed to §3 step 4 of
//! `region-inference.md` rather than to `type-checking-and-mir.md`.**
//!
//! **The hole that was left is closed, and it is rule 4 doing it now.** This
//! section used to say that a closure capturing a place exclusively produced no
//! borrow in MIR, that narrowing was safe only because `science-types` walks a
//! closure's body inline with the enclosing facts, and that *"whichever note
//! writes the capture discipline must decide whether captures become borrows
//! MIR can see"*.
//!
//! **They do.** `science-mir`'s `lower` §8: every place a closure's body names
//! from outside itself is borrowed where the closure value is created, shared
//! unless the body writes through it. That borrow is in
//! [`science_mir::mir::Body::borrows`] like any other, it gets a region like
//! any other, and [`access`]'s §1 refuses every conflicting access inside it.
//! `tests/narrowing.rs`'s
//! `an_exclusive_capture_invalidates_a_narrowing_by_rule_4` is the program, and
//! `tests/closure_captures.rs` is the rest.
//!
//! **So Decision 8's argument extends to closures for Decision 8's own
//! reason.** It is no longer resting on a stricter-than-necessary rule in a
//! different crate that happened to cover the gap. The inline walk in
//! `science-types` still runs and is still stricter; it is now a second line
//! rather than the only one.
//!
//! **What the engine needed in exchange**, and it is the one piece that was not
//! free: a capture's loan must be live for as long as the closure value is, and
//! there is nothing in a closure's *type* to hang that on, because
//! `collections-and-chains.md` §1.2 made a closure type a bare arrow with no
//! capture set in it. [`regions`]'s §5 recovers the positions from the
//! **rvalue** instead, at [`regions::Step::Capture`], and the rest of the
//! engine — the seeding in [`solve`], the propagation, the conflict check —
//! did not change by a line. §8 item 11 records what that says about
//! `region-inference.md` AMENDMENT 2.
//!
//! **What is left is smaller and it is a lowering seam, not a soundness one.**
//! The closure's *body* is still not lowered (`science-mir`'s `lower` §8.5: it
//! has no [`DefId`] to be a body of), so a mistake between two of the closure's
//! own locals is reported by nothing — and nothing outside the closure can name
//! those locals. And a closure that *moves* a capture out is modelled as an
//! exclusive borrow, so the move is invisible; the window that is unreported is
//! after the closure's last use, because rule 4 refuses every use before it.
//!
//! # 7. What this crate cannot do
//!
//! - **A closure's body.** §6. Its *captures* are checked — that is no longer
//!   the hole — and its interior is not lowered, so nothing checks a mistake
//!   between two of its own locals. `science-mir`'s `lower` §8.5 is the seam.
//! - **A move out of a capture.** §6's last paragraph, and `science-mir`'s
//!   `lower` §8.2.
//! - **A borrow inside a container.** [`regions`]'s §2 item 1. Under-
//!   approximating, so it loses errors rather than inventing them.
//! - **Cross-crate summaries.** [`summary`]'s §2. Decision 7's serialisation
//!   has no format to go into, so a callee with no body here is treated as
//!   opaque — [`generate`]'s §5 and §7 — which errs toward rejecting.
//! - **A move through an unresolved call.** `science-mir`'s `lower` §5 marks
//!   those arguments `Copy` deliberately, so `SC0334` cannot see them.
//! - **Move paths.** `science-mir`'s `moves` §3 tracks per local, so a partial
//!   move of one field is a move of the whole value here too.
//! - **`unsafe`.** Decision 10's escape hatch needs a marker MIR does not carry
//!   — its §5 lists that among what will bite — so nothing here relaxes
//!   anything inside an `unsafe` block, and Decision 10 is untouched rather
//!   than implemented. An arena written today is refused by this checker with
//!   `SC0333` and the `unsafe` does not help.
//! - **`SC0341`.** Reserved by §12 until there is an F5.
//!   [`Analysis::borrows_live_at`] is the query it would ask.
//! - **Polonius.** §2 and §13 keep it out of F0 on purpose, and the `intern`
//!   shape is refused here exactly as §2 predicts.
//!
//! # 8. What the note is wrong, underdetermined or optimistic about
//!
//! Recorded here rather than in a commit message, for `science-mir`'s §7's
//! reason: *"the notes are the authority and a correction that lives only in
//! code is a correction nobody reads"*.
//!
//! 1. **§3 step 4 is wrong, and it is the one that matters.** It quantifies
//!    over borrows; it has to quantify over accesses, or Decision 8 of
//!    `type-checking-and-mir.md` is false. §6 above, [`access`]'s §1.
//! 2. **§4.3's second cost cannot happen.** §5 above. `SC0335` is unreachable
//!    in an NLL solver, and the situation it names is `SC0333`.
//! 3. **§5.2 asks a question that removing the syntax already answered.** §4
//!    above. Ambiguity between two parameters is not ambiguity once the answer
//!    is a set, and `SC0340`'s real domain is much smaller than §14's risk
//!    assumes. The bet §14 describes is therefore **safer than the note
//!    thinks**, and for a reason the note does not give.
//! 4. **§3 step 1 says *"regions of live variables are live"* and stops one
//!    clause short.** A loan's region is *not* seeded from liveness — it has no
//!    variable of its own to be live — and a reading that seeds it produces
//!    either an empty region or a wrong one. [`solve`]'s §3 states the three
//!    different lower bounds; the note states one.
//! 5. **§10 item 3 is not sufficient for rule 5 and §4 of [`check`] says why.**
//!    A borrow taken through a reference has no storage-dead point to be
//!    checked against, and checking it against the *reference's* would refuse
//!    `DefTable.get`. The note's rule 5 needs the words *"whose place is rooted
//!    in a local of this body"*.
//! 6. **§11's audit of the engine misses the table this one is built on.** It
//!    lists the CFG, the liveness sets, the constraint set, the worklist, the
//!    region variables and the provenance back-references. It does not mention
//!    the map from a *place* to its region variables, which is Decision 3's own
//!    data structure and is the one thing in this crate that has to walk a type
//!    declaration while holding a table of variables it is growing. It is an
//!    index, so §11's answer is unchanged — but the audit did not cover it, and
//!    an audit that misses its own central decision's data structure is worth
//!    saying so about.
//! 7. **§8's table says `Node of T` is expressible *"because both constructions
//!    in the real dumper pass borrows of the same table and value"*.** That is
//!    a claim about the *sources* being related. It is not why it works. Under
//!    Decision 3 the two regions are never compared at all, so the table's
//!    reason would be equally satisfied by two unrelated sources — and the
//!    acceptance case has exactly one construction site, so its evidence for
//!    the stated reason is nil.
//! 8. **§3 step 4 has a second gap besides item 1's, and §10 item 4 only half
//!    closes it.** Neither note says what a two-phase borrow's *reservation*
//!    may coexist with. `science-mir`'s §5 states the obligation — the place
//!    may be read, not written — and stops there; what it does not say is that
//!    the reservation must be compatible with a **pre-existing shared borrow**
//!    that ends before the activation. That makes rule 4 a three-state question
//!    — shared, reserved, exclusive — rather than a two-state one, and
//!    [`access`]'s §4 is the program that separates them.
//! 9. **A finding about `science-mir`, not about the notes.** Inside a method
//!    whose receiver is already a reference, `self.other()` lowers to
//!    `borrowed _1` — a borrow of the local holding the reference — rather
//!    than to a reborrow of `*_1`. Read literally, every method that returns a
//!    borrow derived from calling another method on `self` outlives its
//!    referent. [`generate`]'s §6 works around it and says where the fix
//!    belongs. The acceptance case misses this by one step, which is why MIR's
//!    own suite did not find it.
//! 10. **A finding about `science-types`, found by running this over
//!     `examples/`, and since fixed.** `describe(doc)`, where `describe` takes
//!     `borrowed T` for a generic `T`, **moved** `doc`: auto-borrow fired for
//!     `borrowed any Summarize` and not for `borrowed T`.
//!     `00_kitchen_sink.science` therefore moved a value that an `Excerpt` was
//!     still borrowing and used it twice more afterwards, and `tests/corpus.rs`
//!     is where it stopped being clean. `science-types` now auto-borrows a
//!     `borrowed T` parameter and the `SC0334` is gone; the census moved from
//!     three diagnostics to two, and
//!     `the_kitchen_sink_no_longer_moves_a_value_that_is_still_borrowed` is
//!     both the record and the guard against it coming back. **This is the one
//!     entry in this list that a note was not the authority on**: the finding
//!     was made by running the engine, and the fix was made in the crate the
//!     finding was about.
//!
//! 11. **AMENDMENT 2 names three lower bounds and there was room for a
//!     fourth.** *"A local's region, bounded below by where the local is live;
//!     a parameter's, by every point; and a loan's, by nothing."* A closure's
//!     capture is a reference held by a value whose type does not mention it,
//!     so the obvious way to make its loan live for the closure's life is a
//!     fourth bound: *a capture loan's region, bounded below by where the
//!     closure local is live.* **That would have been the wrong answer**, and
//!     saying why is worth more than the amendment. A fourth lower bound is a
//!     special case in [`solve`] that every later reader has to keep in mind;
//!     giving the closure local a *position* per capture instead
//!     ([`regions`]'s §5) makes it the **first** of the three bounds, already
//!     implemented, and [`solve`] does not learn that closures exist. The note
//!     is right that there are three, and a phase that wants a fourth should
//!     check whether what it actually wants is a position.
//! 12. **`collections-and-chains.md` §1.2 and §7.6 item 6 contradict each
//!     other, and this engine is where it shows.** Item 6 requires the compiler
//!     to *"record each closure's capture set in its type from F0"*; §1.2 makes
//!     a closure type `(A) -> B`, which has nowhere to record one. [`regions`]'s
//!     §1 walk therefore finds nothing in a closure type however hard it looks,
//!     which is why §5 exists. `science-mir`'s §7 item 5 states the same
//!     finding from the other side.
//!
//! # 9. Can this engine be written in Science?
//!
//! §11 asks it and Decision 11 answers *"index-not-pointer throughout"*. Held,
//! and the audit is worth doing over what was actually built rather than over
//! what was planned:
//!
//! - [`points::Bits`] is a `Vec<u64>`. [`points::PointIndex`] is a `Vec<u32>`.
//! - [`constraints::Constraints`] is a `Vec` of four-field records, all
//!   `Copy` except the cause, which is an enum.
//! - [`regions::RegionTable`] is three `Vec`s and a [`regions::RegionVar`] is a
//!   `u32` into them; the back-reference from a variable to what it stands for
//!   is a lookup, not a pointer.
//! - [`solve::Solution`] is a `Vec<Bits>` and the worklist is a `Vec<usize>` of
//!   point indices.
//! - **Every graph walk in the crate is an explicit stack** — [`solve`]'s
//!   propagation and [`analysis`]'s two reachability sweeps — and
//!   [`liveness`] is not a walk at all but a repeated sweep over a `Vec`
//!   ([`liveness`]'s §3 says what that costs and why a work queue was
//!   refused). The reason is `science-mir`'s `callgraph`'s, one level down: a
//!   mutually recursive descent parser is the program this language most wants
//!   to compile and it is the shape that overflows.
//!
//! The one structure §11 did not anticipate is the place-to-regions map of §8
//! item 6, and it is a `Vec<Vec<(Position, RegionVar)>>` — owned, indexed,
//! no borrow held across a table that grows. **So the answer is still yes**,
//! and it is still yes for §11's own stated reason: *"it is achievable because
//! the same discipline was already imposed on the existing Rust compiler for
//! unrelated reasons"*. Nothing here needed inventiveness to keep it.

pub mod access;
pub mod analysis;
pub mod check;
pub mod codes;
pub mod constraints;
pub mod dump;
pub mod generate;
pub mod liveness;
pub mod points;
pub mod regions;
pub mod solve;
pub mod summary;

use std::collections::BTreeMap;

use science_diagnostics::Diagnostics;
use science_mir::mir::Body;
use science_mir::CallGraph;
use science_resolve::hir::DefId;

pub use analysis::BodyAnalysis;
pub use points::{Bits, PointIndex, RegionSet};
pub use regions::{Context, Position, RegionTable, RegionVar};
pub use summary::{ParamRegion, Summaries, Summary};

/// Every body's analysis, and every summary Decision 7 would serialise.
#[derive(Debug, Clone, Default)]
pub struct Analysis {
    bodies: BTreeMap<DefId, BodyAnalysis>,
    summaries: Summaries,
    /// How many times a strongly connected component had to be re-analysed
    /// before its summaries stopped changing. One per component with more than
    /// one member, or with a self-call. [`analyse_crate`]'s §2.
    fixpoints: Vec<(Vec<DefId>, usize)>,
}

impl Analysis {
    pub fn body(&self, def: DefId) -> Option<&BodyAnalysis> {
        self.bodies.get(&def)
    }

    pub fn summaries(&self) -> &Summaries {
        &self.summaries
    }

    /// §6.4's public query, per function. `SC0341` is this and a comparison.
    pub fn borrows_live_at(
        &self,
        def: DefId,
        point: science_mir::mir::Point,
    ) -> Vec<science_mir::mir::BorrowId> {
        self.bodies.get(&def).map(|body| body.borrows_live_at(point)).unwrap_or_default()
    }

    /// What each mutually recursive component cost. §2 of [`analyse_crate`].
    pub fn fixpoints(&self) -> &[(Vec<DefId>, usize)] {
        &self.fixpoints
    }

    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
}

/// How many times one strongly connected component is re-analysed before the
/// analysis gives up and calls every member opaque.
///
/// The fixpoint terminates on its own — summaries only grow and a function has
/// finitely many parameter regions — so this is a tripwire rather than a
/// termination argument. It has never been reached; `tests/interprocedural.rs`
/// asserts the count on the corpus so that *"never"* is measured.
pub const MAX_COMPONENT_PASSES: usize = 16;

/// Decisions 7 and 8: analyse every body, leaves first, and check it.
///
/// # 1. The order is `science-mir`'s and not this crate's
///
/// [`CallGraph::components`] hands the components over *"in reverse topological
/// order — leaves first, which is §6.2's prescribed order with no sorting of
/// your own"*. So a callee's [`Summary`] exists by the time a caller is
/// analysed, and the one case where it cannot — mutual recursion — is exactly
/// the case a component with more than one member identifies.
///
/// # 2. A recursive component is a fixpoint over its summaries
///
/// Every member starts at [`Summary::empty`] — *"no relation at all"* — and the
/// component is re-analysed until no summary changes. Summaries only grow, so
/// it terminates; starting from [`Summary::opaque`] instead would also
/// terminate and would conclude that every mutually recursive function returns
/// a borrow of everything.
///
/// **This is Decision 8's cost, made visible.** §14: *"a mutually recursive
/// descent parser is one strongly connected component, the Science compiler
/// will contain one, and Decision 8 re-analyses the whole component on any
/// change inside it"*. [`Analysis::fixpoints`] reports how many passes each
/// component took, which is the multiplier on that cost and is not the same
/// number as the component's size.
///
/// # 3. What is not here
///
/// The *cache*. Decision 8 is about a query boundary for incremental
/// recompilation and `science-db` owns the query database; this function
/// computes what would be cached and memoises nothing. Wiring it in is that
/// crate's, and doing it here would be a second answer to a question salsa
/// already answers.
pub fn analyse_crate(
    context: &mut Context<'_>,
    bodies: &[Body],
    graph: &CallGraph,
    diagnostics: &mut Diagnostics,
) -> Analysis {
    let by_def: BTreeMap<DefId, &Body> = bodies.iter().map(|body| (body.def(), body)).collect();
    let mut analysis = Analysis::default();

    // `summary`'s §4, before any body: a callee that is declared and has no
    // body — every prelude method, and every `def` a later phase will supply —
    // is not a callee nothing is known about. This has to run first because the
    // components are handed over leaves first and a *declaration* is below
    // every leaf.
    for def in context.decls.without_bodies() {
        if let Some(sources) = context.decls.borrow_sources(context.types, def) {
            analysis.summaries.declare(def, sources);
        }
    }

    for component in graph.components() {
        let recursive = component.len() > 1
            || component.first().is_some_and(|def| graph.is_directly_recursive(*def));

        if !recursive {
            for def in &component {
                let Some(body) = by_def.get(def) else { continue };
                let one = BodyAnalysis::of(context, body, &analysis.summaries);
                analysis.summaries.insert(one.summary.clone());
                analysis.bodies.insert(*def, one);
            }
            continue;
        }

        // §2.
        for def in &component {
            if let Some(body) = by_def.get(def) {
                analysis.summaries.insert(Summary::empty(*def, body.params().count()));
            }
        }
        let mut passes = 0;
        loop {
            passes += 1;
            let mut changed = false;
            for def in &component {
                let Some(body) = by_def.get(def) else { continue };
                let one = BodyAnalysis::of(context, body, &analysis.summaries);
                let previous = analysis.summaries.lookup(*def);
                changed |= previous.is_none_or(|it| one.summary.differs_from(it));
                analysis.summaries.insert(one.summary.clone());
                analysis.bodies.insert(*def, one);
            }
            if !changed || passes >= MAX_COMPONENT_PASSES {
                break;
            }
        }
        analysis.fixpoints.push((component.clone(), passes));
    }

    // A body whose definition is in no component cannot happen — the graph is
    // built from these same bodies — but a body that lowered while its
    // definition did not is a hole the checker above would have reported, and
    // skipping it silently would make this function's output depend on a
    // diagnostic elsewhere.
    for body in bodies {
        if analysis.bodies.contains_key(&body.def()) {
            continue;
        }
        let one = BodyAnalysis::of(context, body, &analysis.summaries);
        analysis.summaries.insert(one.summary.clone());
        analysis.bodies.insert(body.def(), one);
    }

    for body in bodies {
        if let Some(one) = analysis.bodies.get(&body.def()) {
            check::check_body(context.defs, body, one, diagnostics);
        }
    }

    analysis
}
