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
//! | — | what a closure captures, and how | [`capture`] |
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
//! **And the variant is no longer unreached.** `f"…"` lowers to §1.7's builder,
//! which is one [`mir::Callee::Runtime`] per fragment ([`lower`]'s §10), so the
//! first thing to land there was an interpolation rather than an array
//! operation. That is a fact about this section and not a change to it: an
//! f-string of `n` fragments is `n + 1` straight-line calls, its fragment list
//! is known from the source text, and a builder written as an inlined loop over
//! it would have broken the claim above for no reason. `tests/fstring.rs` is
//! the sequence; `tests/no_invented_loops.rs` is still the invariant.
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
//! - **A closure's captures are borrows in that same table.** [`lower`]'s §8:
//!   every place a closure's body names from outside itself is borrowed where
//!   the closure value is created — shared unless the body writes through it —
//!   into a temporary the [`mir::Rvalue::Closure`] aggregate then holds. So
//!   rule 4 and rule 5 cover a closure with **no rule of their own**, and the
//!   obligation on you is only the one you already have.
//!
//!   **What you owe in exchange** is that the capture's loan must be live for
//!   as long as the closure value is. There is nothing in the closure's *type*
//!   to hang that on — `collections-and-chains.md` §1.2 made a closure type a
//!   bare arrow, `(A) -> B` — so the relation is readable only off the rvalue's
//!   capture list. `science-regions`'s `regions`'s §5 is one way to do it and
//!   costs one new position step; a consumer that ignores the list gets loans
//!   whose regions are one point wide, which is a *missing* diagnostic.
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
//! - **A closure's *body* is not lowered.** Its captures are ([`lower`]'s §8),
//!   so nothing crosses the boundary unseen, but a mistake between two of the
//!   closure's own locals is reported by nothing. §8.5 says what that is
//!   blocked on and it is a [`science_resolve::hir::DefId`] this crate cannot
//!   mint.
//! - **A move out of a capture is invisible.** [`lower`]'s §8.2. The strongest
//!   thing a borrow discipline can say about it is an exclusive borrow, and
//!   that is what it says.
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
//! **Thirteen, and two of them are now closed rather than open.** Items 9 and
//! 10 were both *"a note prescribes something the ABI below it cannot
//! express"*, and both were closed by changing the ABI rather than by softening
//! the note: `science_string_with_capacity` for §1.7's capacity, and an
//! `Rvalue::Cast` lowering for `science_string_push_i64`'s sign extension. They
//! are left in the list with their history because the list is a record of what
//! reading could not establish, not a list of open bugs.
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
//! 4. **Neither note says what a `for` lowers to, and the half of that which
//!    is a *language* question was answered somewhere else entirely.** This
//!    entry used to end *"and `Iterate` does not exist"*. It does now, and with
//!    it the question splits in two.
//!
//!    **The callee is still a hole and it is now a seam rather than an
//!    absence.** `science-types`'s `check` resolves `Iterate.next` — that is
//!    where a loop's binding type comes from — and then discards the candidate,
//!    so `thir::ExprKind::For` arrives here with `{ pattern, iter, body }` and
//!    nothing to call. [`lower`]'s §7.2 says exactly what THIR must add
//!    (`next: Option<DefId>`, from the `candidate.method` `iterate_item`
//!    already holds) and why this crate must not answer it itself.
//!    [`mir::Unresolved::IterateNext`] carries the hole in the IR rather than
//!    hiding it as an assumption, as before.
//!
//!    **The borrow was never a hole — it was a wrong answer, and nothing said
//!    so.** A `for` *moved* its subject into a temporary. The acceptance case's
//!    `Scopes.lookup` therefore lowered `for rib in self.ribs:` to
//!    `_4 = move (*_1).ribs` — a move of a field out of a *shared borrow* of
//!    `self` — and no phase reported it, because `SC0334` fires on a move that
//!    overlaps a live borrow and there was no borrow to overlap. That is item
//!    3's shape for the third time: **a lowering gap that does not fail, it
//!    goes quiet.** [`lower`]'s §7.1 is the fix and it is read out of
//!    `collections-and-chains.md` §4.2 and §4.4 rather than decided here.
//! 5. **Neither note mentions closures at all**, and a closure is where a
//!    borrow escapes a body. This entry used to say *"[`lower`]'s §8 refuses it
//!    by name; this is the largest hole in the crate"*, and the word doing the
//!    damage was **therefore**: *the body is not lowered, and its captures are
//!    therefore not borrows MIR can see*. The two are separable. [`lower`]'s §8
//!    is now the capture discipline — every capture is a borrow, shared unless
//!    the body writes through the place — and the body is still not lowered.
//!
//!    **What that settles and what it does not.** It settles
//!    `region-inference.md`'s AMENDMENT 3, which said *"whoever writes the
//!    capture discipline has to decide whether a capture becomes a borrow MIR
//!    can see"*: it does, so rule 4 covers closures for rule 4's own reason and
//!    `type-checking-and-mir.md` Decision 8's argument extends to them instead
//!    of resting on `science-types` walking the body inline. It does **not**
//!    settle the language question, and that is the point of the discipline
//!    chosen: a uniform borrow capture refuses every program that could tell a
//!    borrow capture from a by-value one, so `collections-and-chains.md` can
//!    still decide either way without changing this IR. §8.1 is that argument
//!    and §8.5 is the seam that is left.
//!
//!    **What the note is still silent about** is worth recording separately.
//!    `collections-and-chains.md` §7.6 item 6 asks that *"the compiler must
//!    record each closure's capture set in its type from F0"* — and §1.2 of the
//!    same note makes a closure type `(A) -> B`, which cannot hold one. Those
//!    two sentences are in the same document and contradict each other. The
//!    capture set is recorded here, in the MIR aggregate, which satisfies item
//!    6's *purpose* (F2 must be able to reject a closure capturing anything
//!    `mutable borrowed`, and [`mir::BorrowKind::Exclusive`] on a capture is
//!    exactly that fact) and not its *letter*. Item 6's last sentence — *"this
//!    is the requirement most likely to be missed, because nothing in F0 reads
//!    that information"* — is now false twice over: something reads it, and the
//!    place it was asked to be put is a place it cannot go.
//! 6. **`examples/21_compiler_shapes.science` lowers, and what it is missing is
//!    a standard library and not a MIR.** Eighteen bodies, 89 blocks, 353
//!    points, twelve borrows of which seven are two-phase, ten drops and
//!    **zero drop flags** — the program has no conditional move, so Decision
//!    26's mechanism is not exercised by the acceptance case and
//!    `tests/drops.rs` carries that burden on a fixture written for it.
//!
//!    **The hole count has more than halved and what is left is one seam and
//!    one absence.** It read *"sixteen of its calls have no callee"*; it is
//!    seven, because the prelude gained `Array` and `Map`. Four are
//!    [`mir::Unresolved::Method`] — the container methods the prelude still
//!    stops short of — and **three are the three `for` loops**, one each, and
//!    those are no longer blocked by anything a note has to decide: `Iterate`
//!    is declared, `check` resolves it, and `thir::ExprKind::For` does not
//!    carry the answer. §7 item 4 and [`lower`]'s §7.2 are the seam.
//!
//!    **What region inference gets out of the file did change**, and not
//!    through the callee. Its three `for` loops now contribute three shared
//!    loans — `(*_1).ribs`, `(*_8).bindings`, and `walk`'s — where before they
//!    contributed three moves nobody checked. Twenty-one borrows rather than
//!    eighteen, four drops rather than nine (a borrowed subject is not a
//!    temporary that has to be dropped), and 366 points rather than 371.
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
//! 8. **A hole in this crate was also a *suppression* in the next one, and
//!    nothing said so.** While `ExprKind::FString` assigned
//!    [`mir::Rvalue::Error`], `science-regions`'s `analysis`'s `calls_a_hole`
//!    read that rvalue as *"something above gave up here"* and silenced **every
//!    region finding in the whole body**. So a program with one `f"…"` in it
//!    was not borrow-checked at all, and the only visible symptom was that
//!    `sciencec check` printed nothing — which is what it prints when a program
//!    is correct.
//!
//!    That is §3's discipline meeting a consumer that reads a hole as a reason
//!    to stop. The discipline is still right — a hole cannot manufacture a
//!    cascade — but the entry that states it should say the other half: **a
//!    hole here costs the next phase's diagnostics for the body it is in**, so
//!    the scope of an [`mir::Rvalue::Error`] is a decision and not a detail.
//!    [`lower`]'s §10 emits a [`mir::Unresolved::Display`] rather than an
//!    `Rvalue::Error` for a hole it genuinely cannot render, which keeps the
//!    suppression (it is a hole) and keeps the borrows (they are right), and
//!    the difference between the two spellings is the whole reason to prefer
//!    the second.
//! 9. **`strings-formatting-and-docs.md` §1.7 prescribed a performance
//!    property its own ABI could not express**, and the ABI was the thing to
//!    fix. *"It lowers to a builder over the fragments, with the capacity
//!    pre-computed from the literal fragments plus a per-type estimate for each
//!    hole, so the common case is one allocation."* `science-codegen`'s
//!    `RUNTIME` was fifty-four entry points and **not one of them took a
//!    capacity**: no `science_string_with_capacity`, no
//!    `science_string_reserve`, so `science_string_new` was the only way to
//!    start a `String` and it started it empty. The common case was one
//!    allocation *per growth*. The estimate §1.7 asks for was computable here —
//!    the literal fragments are in the node and the holes have types — and
//!    there was nowhere to send it.
//!
//!    **The entry point exists now and the number is measured rather than
//!    claimed.** `science_string_with_capacity` is `RUNTIME`'s fifty-fifth row
//!    and [`lower`]'s `Builder::capacity_estimate` is the arithmetic;
//!    `science-rt`'s `tests/capacity.rs` counts calls into the system
//!    allocator and the acceptance case `f"n es {n} y x es {x}"` goes from
//!    **three** growths to **one** allocation in total.
//!
//!    **What it cost, stated because it is the shape §9.2 warns about.** A
//!    `ScienceString` returned by value is three words, therefore MEMORY,
//!    therefore an `sret` return — so the derived set went from nine to ten.
//!    Nobody decided that: the signature was written, `runtime.rs` classified
//!    it, and three tests that assert the count changed together. `format.rs`'s
//!    own note records the same question being answered the *other* way seven
//!    entry points earlier, which is the pair worth reading together.
//!
//!    **And the estimate is an estimate.** Too small costs the growth it was
//!    meant to avoid; too large is memory held for the life of the string,
//!    because `String` has no `shrink_to_fit` and §8 lists none. That is why
//!    each integer width gets its own longest spelling rather than `I64`'s, and
//!    why the two numbers that cannot be bounded — an `F64` hole and a `String`
//!    hole — are documented as guesses at the function that makes them.
//! 10. **`science-rt` documented a caller that did not exist.**
//!     `science_string_push_i64`'s own note read *"every signed width renders
//!     through this one: `I8`…`I64` are sign-extended by codegen before the
//!     call"*. Nothing sign-extended anything: [`mir::Rvalue::Cast`] had no
//!     lowering in `science-codegen-llvm`, so there was no phase between this
//!     one and the call that could. Choosing the entry point is what surfaced
//!     it, because the choice has only two answers and both are visible —
//!     `push_i64` with an `i32` in an `i64` parameter, or a refusal.
//!     [`lower`]'s `Builder::push_of` refused, and `push_u64` had the same
//!     three narrow widths behind it with no note at all.
//!
//!     **The caller exists now and it is this crate.** `Rvalue::Cast` has a
//!     lowering, so `push_of` emits one cast into a temporary in front of the
//!     call — to `I64` for `I8`/`I16`/`I32`, to `U64` for `U8`/`U16`/`U32` —
//!     and both entry points' notes have been corrected to name the phase.
//!     The two lists stay separate because the extension's signedness is the
//!     *source's*: `-1i32` through `push_u64` renders `4294967295`, which is a
//!     legal `i64` and a different number.
//!
//!     **This one was loud rather than quiet**, which is worth keeping in the
//!     record because items 3, 4 and 7 were all the other shape:
//!     `LLVMVerifyModule` rejects `call void @science_string_push_i64(ptr,
//!     i32)` against its own `declare`, so the wrong answer here failed the
//!     build. The *pointer* version of the same mistake does not — see
//!     `science-codegen-llvm`'s §3 finding 18, which is this crate's §4
//!     dereference not firing on a `borrowed String` hole, and which verifies,
//!     links, and aborts inside the runtime.
//! 11. **§1.6 says an interpolation *borrows* its operands, and half of them
//!     cannot be borrowed.** Six of the seven `science_string_push_*` entry
//!     points take their argument **in a register**; only
//!     `science_string_push_str` takes a pointer. A loan handed to a parameter
//!     declared `i64` is not a conservative version of the right answer, it is
//!     a different one, so the rule the implementation needs is two rules and
//!     the note gives one word.
//!
//!     What §1.6 is actually deciding is *"does not **consume**"* — its own
//!     reason says so: *"a debugging `print` that moves the value you were
//!     about to use is a diagnostic in the `SC0300` range caused by a line the
//!     user added to understand a different problem"*. For a trivially copyable
//!     type a read satisfies that with no loan at all, and for a `String` the
//!     only other spelling is a move, so the borrow is forced. §1.6 should say
//!     *"an interpolation does not consume its operands"*; the borrow is how
//!     that is achieved for the types that have no other way of achieving it.
//! 12. **A cast is the one rvalue whose meaning is not recoverable from the
//!     statement it is on, and [`mir::Rvalue::Cast`] used not to carry it.**
//!     Every other rvalue can be read from the destination's type and the
//!     operand's place: a `Binary` takes its width from whichever side is a
//!     place, a `Use` from the slot it writes. `1 as U8` is a constant on both
//!     sides of the arrow — no place, no local declaration, nothing to ask —
//!     and `-1i32 as U64` and `0xffffffffu32 as U64` are the same thirty-two
//!     bits with two different answers, decided by the **source's** signedness.
//!     So the variant now carries `from`, taken from THIR at lowering time.
//!
//!     **The consumer that needed it could not have noticed it was missing.**
//!     LLVM integers are signless, so a backend that guessed from the
//!     destination emits `zext` where `sext` was wanted, which verifies, links,
//!     runs and prints `18446744073709551615` where `-1` was meant. This is the
//!     shape items 3, 4 and 7 have — a gap that does not fail, it goes quiet —
//!     met one phase earlier and closed before it could go quiet.
//! 13. **The capacity of §1.7's builder is not a `Literal`, and pretending it
//!     was would have been this crate claiming the program contains a number it
//!     does not.** [`mir::Constant::Count`] is the variant; a `Literal::Int`
//!     carries a base and a suffix because it is source text, and `dump` prints
//!     it back the way it was written. The estimate is arithmetic this lowering
//!     did. It is also the only constant in the IR whose width is the C
//!     parameter's rather than a Science type's — §9.3's *"a length is a
//!     `usize` and an index is an `Int`"* — so there is no Science type for it
//!     to disagree with, which is a second reason it is not a literal.

pub mod callgraph;
pub mod capture;
pub mod drops;
pub mod dump;
pub mod lower;
pub mod mir;
pub mod moves;

pub use callgraph::CallGraph;
pub use capture::{Capture, Use};
pub use lower::{lower_body, lower_crate, Context};
pub use mir::{
    BasicBlock, BlockId, Body, BorrowData, BorrowId, BorrowKind, Callee, Local, LocalKind, Operand,
    Place, Point, Projection, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
    Unresolved,
};
pub use moves::State;
