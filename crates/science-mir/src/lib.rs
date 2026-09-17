//! `science-mir` — the IR region inference runs on.
//!
//! `region-inference.md` §10 lists six things a region engine needs and says of
//! them: *"these are requirements, not suggestions; every one of them is
//! something a dataflow pass cannot add afterwards."*
//! `type-checking-and-mir.md` §12 promises all six on MIR's behalf.
//! `science-types`'s `thir`'s §5 then checked that promise against what a typed
//! *tree* can do and answered **one and a half of six**. This crate is the
//! other four and a half.
//!
//! | §10 | Here | Where |
//! |---|---|---|
//! | 1. An explicit CFG with `(block, statement)` points | **Yes** | [`mir::BasicBlock`], [`mir::Point`]; [`lower`]'s §3 is `a and b` becoming edges |
//! | 2. Explicit places, equal expressions giving equal places | **Yes**, with index and dereference | [`mir::Place`], [`mir::Projection`] |
//! | 3. Explicit `StorageLive`/`StorageDead` | **Yes** | [`mir::StatementKind`], derived from THIR's scopes by [`lower`]'s §2 |
//! | 4. Two-phase borrows exposed | **Yes**, and the activation is a statement rather than a derivation | [`mir::BorrowKind::TwoPhase`], [`lower`]'s §6 |
//! | 5. Drop points explicit *and elaborated*, with Decision 26's flags | **Yes** | [`drops`], over [`moves`] |
//! | 6. Stable point identity across the query boundary | **Yes**, to the degree anything can be | [`mir`]'s §2 |
//!
//! Every one of those rows has a cost and each cost is stated at the item, not
//! here. §7 below is the separate list: what this crate found that the two
//! notes were wrong or silent about.
//!
//! # 1. What is in here
//!
//! | Note | What it is | Where |
//! |---|---|---|
//! | Decision 4 | the CFG over places and temporaries, in three-address form | [`mir`] |
//! | Decision 4 | THIR to that, in one walk | [`lower`] |
//! | Decision 26 | the move analysis that decides which local is conditionally moved | [`moves`] |
//! | Decision 26 | drop elaboration, and the flags | [`drops`] |
//! | Decision 8 | the call graph and its strongly connected components | [`callgraph`] |
//! | — | a textual dump, for tests | [`dump`] |
//!
//! # 2. What is deliberately not in here
//!
//! - **Region inference.** This is what it runs on. §5 is the seam.
//! - **Borrow checking, of any kind.** No `SC0301`, no `SC0330`. The move
//!   analysis of [`moves`] is *only* Decision 26's input, and [`moves`]'s §1
//!   says why sharing it with a use-after-move check now — before that check
//!   exists — would fix the lattice around a question nobody has asked.
//! - **Optimisation.** No constant folding, no block merging, no dead-code
//!   elimination. A `Goto` chain that a peephole would collapse is left alone,
//!   because `codegen-and-linking.md` Decision 5 wants a MIR dump and an IR
//!   dump to be diffable and every merge here is a line that does not appear
//!   there.
//! - **Codegen.** `science-codegen` is the consumer below, and Decision 42
//!   already put the line between them.
//! - **Monomorphisation.** §12's order is types → THIR analyses → MIR →
//!   regions → mono → codegen. A [`science_types::ty::TyKind::Param`] in a
//!   local's type here is correct, not a hole.
//!
//! # 3. Diagnostics: none, and that is a decision
//!
//! **This crate reports nothing.** It has no `codes` module, holds no
//! [`science_diagnostics::Diagnostics`], and takes none as a parameter.
//!
//! The reason is §3.1's: *"a diagnostic about the user's code is best emitted
//! from a tree that has the user's structure, and once MIR has flattened
//! `a and b` into blocks and jumps, a message about it has to reconstruct what
//! was written."* Every mistake this lowering could notice has already been
//! reported by the resolver or the checker — an assignment to a non-place, a
//! field that does not exist, a `break` outside a loop — and a second report
//! from here would be a second spelling of one mistake, from the level with the
//! worst span to say it from.
//!
//! The ownership range `SC0300`–`SC0399` therefore has **nothing new allocated
//! to it by this crate**. `SC0300`, `SC0303`–`SC0329` and `SC0399` were free
//! before this crate existed and are free after it.
//!
//! **Where the lowering meets something it cannot represent** it emits a hole —
//! [`mir::Rvalue::Error`], [`mir::Callee::Unresolved`] — and says which hole.
//! That is `ty`'s §5 discipline carried down: a hole is a value rather than an
//! absence, so no consumer has to branch on whether the statement exists, and a
//! hole cannot manufacture a cascade.
//!
//! # 4. Decision 5's hole, kept open
//!
//! > **Decision 5.** In F0, a whole-array operation lowers to a *call* to a
//! > runtime or library entry point, never to an inlined MIR loop.
//!
//! §3.4's reason is that F1's fusion pass must have array operations to
//! schedule rather than loops to reverse-engineer — *"precisely the wound §7.1
//! says Julia carries"*.
//!
//! **Nothing in F0's THIR is a whole-array operation**, so the decision is
//! vacuous today and the only thing this crate can do about it is not
//! foreclose it. What it does is a stronger, checkable statement of the same
//! thing:
//!
//! > **Every loop in a body's CFG comes from a `loop` or a `for` the author
//! > wrote. This crate synthesises no loop.**
//!
//! Counted as *loop headers* — the targets of back edges — and not as back
//! edges, because a `continue` is a second edge into a header the author wrote
//! once.
//!
//! `tests/no_invented_loops.rs` is that claim, run over every example in the
//! corpus. [`mir::Callee::Runtime`] is where an array operation lands when
//! there is one.
//!
//! # 5. The seam, for region inference
//!
//! `science-types`'s `lib.rs` §5 and `thir`'s §5 each stated the seam the next
//! phase started at. This is the next one, and it is addressed to the region
//! engine.
//!
//! **What you are given.**
//!
//! - **A point** is [`mir::Point`], a `(block, statement)` pair, and
//!   `statement == statements.len()` is the terminator.
//!   [`mir::Body::points`] enumerates them in a dense, stable order, so a
//!   region *is* a bitset indexed by position in that iteration and §3's
//!   `O(points × regions)` claim holds. [`mir::Body::point_count`] is its
//!   width.
//! - **The graph** is [`mir::Body::successors`] and
//!   [`mir::Body::predecessors`], the second cached because liveness is a
//!   backward dataflow that asks at every step.
//!   [`mir::reverse_postorder`] is the forward order.
//! - **Every borrow** is in [`mir::Body::borrows`], with its kind, the place it
//!   is taken from, the place the reference was stored in, its reservation
//!   point and — for a two-phase borrow — its activation point. You do not have
//!   to scan for `Ref` rvalues, and you should not: the table is built after
//!   elaboration, so its points are the final ones.
//!
//!   **A borrow this lowering inserted names the referent, not the reference.**
//!   The receiver borrow of `self.other()`, inside a method whose own receiver
//!   is already a reference, is `borrowed (*_1)` and never `borrowed _1`
//!   ([`lower`]'s §9). So a borrow whose place is a whole local is a borrow of
//!   *that local's storage*, and rule 5's comparison against
//!   [`mir::Body::storage_dead_points`] of `place.local` is the right question
//!   to ask about it. A consumer does not have to special-case a reference-
//!   typed local to avoid refusing every method that returns a borrow derived
//!   from a call on `self`.
//!
//!   The one borrow left whose place is a whole local of reference type is one
//!   the *author wrote* — `let s be borrowed r`, where `r` is `borrowed Row` —
//!   and there the loan really is of `r`'s own storage: the checker typed it
//!   `borrowed (borrowed Row)`, and refusing it when it escapes is correct,
//!   not a false positive.
//! - **Aliasing** is [`mir::Place::may_overlap`], which is *may*;
//!   [`PartialEq`] on a [`mir::Place`] is *definitely*. That documentation
//!   argues that one predicate cannot be conservative in both directions, and
//!   step 4 of §3 wants the first.
//! - **Rule 5's comparison point** is [`mir::Body::storage_dead_points`]. A
//!   local can have several, one per path out of its scope.
//! - **Drops** are [`mir::TerminatorKind::Drop`], already elaborated. A `flag`
//!   of `Some` means the drop is conditional — the exclusive borrow it takes
//!   happens on some paths and not others — and [`mir::Body::drop_flags`] is
//!   the whole list.
//! - **Narrowing** is [`mir::Rvalue::Narrow`], carried from THIR. **Do not
//!   recompute it.** Decision 25 is legal only because narrowing's conclusion
//!   is consumed *after* regions run; a pass here that re-derived it would be
//!   consuming it before, and `narrow`'s §4 is that argument in full.
//! - **The call graph** is [`callgraph::CallGraph`], and
//!   [`callgraph::CallGraph::components`] gives Decision 8's SCCs in reverse
//!   topological order — leaves first, which is §6.2's prescribed order with no
//!   sorting of your own.
//!
//! **What you owe.**
//!
//! - **The two-phase rule.** MIR marks a borrow reserved at one point and
//!   activated at another and decides nothing about what may happen in between.
//!   The obligation is: between a [`mir::BorrowKind::TwoPhase`] borrow's
//!   `reserved` and its `activation`, the borrowed place may be *read* and may
//!   not be written or exclusively reborrowed; from the activation on, it is an
//!   ordinary exclusive borrow. If that rule is not implemented,
//!   `v.push(v.len())` does not compile and §14's *"the failure looks like a
//!   region bug rather than a lowering gap"* has come true one level lower than
//!   that note expected.
//! - **A region variable per borrow and per reference in a type.** §6.2's
//!   second half, which Decision 3 turns into *one variable per borrowed
//!   field*. A [`mir::Place`]'s type is [`mir::Place::ty`] and every projection
//!   carries its own, so the references in a type are reachable without a
//!   declaration lookup.
//! - **Provenance.** Decision 2 is *"every constraint carries its span and its
//!   cause, and the solver never merges two constraints"*. Every
//!   [`mir::Statement`], [`mir::Terminator`] and [`mir::BorrowData`] here
//!   carries a [`science_diagnostics::Span`] for exactly that. The *cause* —
//!   `AssignedFrom`, `PassedTo`, `ReturnedFrom`, `StoredInField` — is readable
//!   off the statement kind and is not pre-computed here, because a cause is a
//!   fact about a constraint and there are no constraints in this crate.
//!
//! **What is missing, and will bite.**
//!
//! - **A closure's captures are not borrows you can see.** [`lower`]'s §8.
//! - **An unresolved callee's arguments are all copies.** [`lower`]'s §5. A
//!   move through a method call is invisible, so a use-after-move through one
//!   is not there to be found.
//! - **Move tracking is per local, not per move path.** [`moves`]'s §3.
//! - **`extern` and `unsafe` carry no marker.** An `unsafe` block lowers to its
//!   contents, so a pass that wants to relax a rule inside one has nothing to
//!   read. Nothing needed it yet; adding it is a statement kind, not a change
//!   of shape.
//!
//! # 6. Can this be written in Science?
//!
//! `region-inference.md` §11 asks the question of the engine and Decision 11
//! answers *"index-not-pointer throughout"*. The same question about this crate
//! has the same answer and it is worth writing down, because the IR is the
//! thing the engine holds while it runs.
//!
//! Every structure here is a `Vec` indexed by a newtype: blocks, locals,
//! borrows, the predecessor table. There is no `Box`, no `Rc`, no back-pointer
//! and no borrow held across a table that is growing. The one aliasing
//! structure a lowering usually has — the parent link from a scope to its
//! enclosing scope — is a stack and an integer depth ([`lower`]'s §2), which is
//! §8's `Rib` shape exactly.
//!
//! **One external dependency and no more.** The CFG's reachability, the
//! predecessor table and Tarjan's algorithm are written out
//! ([`mir::reverse_postorder`], `mir::predecessors_of`,
//! [`callgraph`]) rather than taken from a graph library, and that is
//! the same decision one level up: a dependency here is a dependency the
//! self-hosted compiler would have to port.
//!
//! # 7. What the notes were wrong or silent about
//!
//! Recorded here rather than left in a commit message, because the notes are
//! the authority and a correction that lives only in code is a correction
//! nobody reads.
//!
//! 1. **`region-inference.md` §10 item 5 asks for less than §12 delivers, and
//!    §12 is right.** Item 5 asks only that drop points be *explicit*; §12's
//!    Decision 26 adds *elaborated*, and says that making the points explicit
//!    *"is not enough on its own, and saying only that was a hole
//!    `codegen-and-linking.md` found by trying to lower against this section"*.
//!    §10 item 5 should be amended to say elaborated, because a reader who
//!    implements the item as written builds a MIR that double-frees.
//! 2. **A flagged drop breaks `codegen-and-linking.md` Decision 5.** That note
//!    says *"every MIR basic block becomes exactly one LLVM basic block"*. A
//!    [`mir::TerminatorKind::Drop`] with a flag becomes three. [`drops`]'s §3
//!    argues the alternative is worse; the note needs the amendment either way.
//! 3. **§10 item 4's acceptance case works, and it depends on a lookup that
//!    did not exist when this crate was started.** `v.push(v.len())` needs the
//!    receiver auto-borrowed, which needs Decision 11's method lookup — which
//!    `science-types` grew while this was being written. `tests/two_phase.rs`
//!    runs §10 item 4's own sentence and it passes. If that lookup ever
//!    regresses to `None`, the two-phase machinery does not break: it goes
//!    *inert* on method calls, silently, and the only thing that notices is
//!    that test. §14's *"the failure looks like a region bug rather than a
//!    lowering gap"* has a third possibility, which is that it looks like
//!    nothing at all.
//! 4. **Neither note says what a `for` lowers to, and `Iterate` does not
//!    exist.** [`lower`]'s §7 decides; the decision is visible in the IR as
//!    [`mir::Unresolved::IterateNext`] rather than hidden as an assumption.
//! 5. **Neither note mentions closures at all**, and a closure is where a
//!    borrow escapes a body. [`lower`]'s §8 refuses it by name. This is the
//!    largest hole in the crate and it is a language question, not a lowering
//!    one.
//! 6. **`examples/21_compiler_shapes.science` lowers, and what it is missing is
//!    a standard library and not a MIR.** Eighteen bodies, 89 blocks, 353
//!    points, twelve borrows of which seven are two-phase, ten drops and
//!    **zero drop flags** — the program has no conditional move, so Decision
//!    26's mechanism is not exercised by the acceptance case and
//!    `tests/drops.rs` carries that burden on a fixture written for it.
//!
//!    Sixteen of its calls have no callee: every `Array` and `Map` method it
//!    uses — `new`, `len`, `get`, `push`, `pop` — is a method on a type with no
//!    declaration the checker can see, and its three `for` loops call an
//!    `Iterate` that does not exist. **Region inference run over this file
//!    today would be reasoning about calls whose signatures are absent**, and
//!    that is the honest state of the acceptance case: the *shapes* are all
//!    here and expressible, and `stdlib-core.md`'s containers are not.
//!
//!    Its §4 — `Node of T` with two borrowed fields, *"the case the whole claim
//!    turns on"* — lowers to two shared-borrow parameters stored into one
//!    record, which is Decision 3's *"one region variable per borrowed field"*
//!    with both borrows reaching the solver separately and nothing here
//!    relating them. That is the right answer for MIR to give and it is
//!    untested until there is a solver.
//! 7. **Neither note says what a method call's receiver borrow is taken
//!    *from*, and the obvious answer is wrong.** §12 asks for the receiver to
//!    be auto-borrowed and stops there. This crate borrowed the receiver's
//!    place, which for a method called on a `self` that is itself a reference
//!    is the local holding the reference — so the loan pointed at the callee's
//!    own frame, and read by rule 5 every method returning a borrow derived
//!    from a call on `self` outlives its referent. [`lower`]'s §9 is the fix:
//!    the same dereference §4 already inserts for a *field* of `self`.
//!
//!    **The evidence finding is why this crate's own suite walked past it.**
//!    `examples/21_compiler_shapes.science` reaches the borrow-inserting arm
//!    nowhere: every `self.foo()` in it is a container method with no
//!    declaration, so the receiver is copied rather than borrowed, and
//!    `parent_of` — the one call on `self` to a method that *is* declared —
//!    lowers to [`mir::Rvalue::Error`], because its own value depends on an
//!    `Array.get` that does not resolve. The fixtures that do take receiver
//!    borrows (`tests/two_phase.rs`'s `v.push(v.len())`) call them on *owned*
//!    locals, where borrowing the local is right. And the one test that looks
//!    through a `self` at all asserts `(*_1).tokens` — a *field* of the
//!    receiver, one construct short of the receiver itself. Sixty tests, and
//!    the gap between them was one step wide.
//!
//!    It was found from `science-regions`, which reached it by asking what a
//!    loan points at rather than what a place looks like. That is the second
//!    time item 3's shape has occurred: a lowering gap that does not fail, it
//!    goes quiet.

pub mod callgraph;
pub mod drops;
pub mod dump;
pub mod lower;
pub mod mir;
pub mod moves;

pub use callgraph::CallGraph;
pub use lower::{lower_body, lower_crate, Context};
pub use mir::{
    BasicBlock, BlockId, Body, BorrowData, BorrowId, BorrowKind, Callee, Local, LocalKind, Operand,
    Place, Point, Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
    Unresolved,
};
pub use moves::State;
