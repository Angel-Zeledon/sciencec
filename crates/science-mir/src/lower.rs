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
//! **That accounting is drop elaboration's and there is now a second
//! consumer.** *"A drop flag on a local that does not need dropping"* is what
//! calling a copy a move costs [`crate::drops`]; what it costs a **check** is a
//! false positive, because whether a user type is `Copy` is Decision 11's
//! lookup and this compiler has none, so every record operand arrives here as a
//! move. `science-regions`' `moved` §3 item 4 pays that bill on its own side —
//! it reports rule 3 only where the type owns something — rather than asking
//! this rule to reverse, because reversing it would move the cost back onto
//! drop elaboration, where the direction above is right.
//!
//! **The exception, and it is deliberate.** Every argument of a call **whose
//! signature this crate cannot see** is `Copy`, whatever its type. A call whose
//! signature is unknown has unknown argument passing, and marking the arguments
//! moved would make Decision 11's absent method lookup *manufacture*
//! use-after-move errors in ordinary code. That is `ty`'s §5 discipline —
//! *"a hole costs nothing downstream and, in particular, cannot manufacture a
//! cascade"* — applied to operands. The cost is stated at [`Unresolved`]: a
//! genuine move through a method call is invisible until the lookup lands.
//!
//! **The exception is a condition and used to be spelled as one case of it.**
//! It was implemented as *"the callee is a [`Callee::Unresolved`]"*, which is
//! only the half of the condition where the *name* did not resolve. The other
//! half is a name that resolved to a definition carrying no
//! [`science_types::items::Signature`], and there is exactly one construct in
//! F0 that produces it — `print`, whose declaration `science-resolve`'s
//! `builtins.rs` writes out, measures and withdraws, leaving a `Callee::Def`
//! with nothing behind it. Read as *"no signature means by value"*, `print(s)`
//! moved a `String` the program still owned; drop elaboration then deleted the
//! binding's drop and `science-codegen-llvm` freed the buffer at the call, so
//! `print(s)` twice printed the string and then an empty line. That is not the
//! lowering compensating for an undeclared parameter: it is this rule applied
//! to the case it was written for. [`Builder::has_no_signature`] is the
//! predicate and states what it costs.
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
//! # 7. `for`: a borrow this level owns, and a callee it does not
//!
//! This section used to read *"`for`, whose callee does not exist"*, and the
//! reason it gave was that `Iterate` was not declared. It is declared now —
//! `science-resolve`'s `builtins` gives it `def next(mutable self) ->
//! Self.Item?`, and `Array of T implements Iterate: type Item is borrowed T` —
//! so the section splits into two questions with two different owners, and the
//! old wording answered neither of them.
//!
//! ## 7.1 The loop's borrow, which is this level's to get right
//!
//! `collections-and-chains.md` §4.2's AMENDMENT 11 is unconditional:
//! *"`for x in xs:` desugars to `xs.iterate()`* — *it borrows"*, and its §4.4
//! gives that borrow in one row: *"`iterate()` | borrows the source, shared,
//! for the chain's life"*. A loop holds it across every iteration.
//!
//! > **Decision. A `for` takes one shared borrow of its subject's place,
//! > created before the header and read once per turn, and the
//! > element-producing call reads through that reference rather than through
//! > the subject.**
//!
//! *What it replaces.* The subject used to be *moved* into a temporary
//! (`_2 = move _1`) and copied into the call. That is `iterate_consuming()`'s
//! row of §4.4 in the position §4.2 gives to `iterate()`'s, so the most
//! ordinary loop in the language consumed the thing it iterated, and
//! `for doc in docs:` followed by any use of `docs` was a use-after-move that
//! no phase reported because the callee was a hole.
//!
//! *Why shared, when `next` is declared `mutable self`.* The two borrows are of
//! two different things. §4.4's shared borrow is of the **source**; `next`'s
//! exclusive receiver is of the **chain**, whose cursor it advances. A bare
//! `for` is `iterate()`, and the three sources of §4.1 are distinguished in the
//! *subject expression* rather than in the loop — `xs.iterate_mutably()` and
//! `xs.iterate_consuming()` are ordinary method calls that this lowering
//! already handles, and what the `for` then borrows is the chain they returned.
//! So one rule covers all three rows, and it is §4.2's own word for the only
//! row a bare `for` can be.
//!
//! *Why not exclusive anyway, to be safe.* It is not the safe direction, it is
//! a different program. An exclusive borrow of the subject refuses
//! `for x in xs: print(xs.length())` — a read of the collection being read —
//! which §4.4 admits and which is the shape a `for` over a shared source is
//! for. The `Item` type is what settles it: `borrowed T` means the loop hands
//! out shared views, and a shared view needs no more than a shared borrow to
//! come from. `mutable borrowed T` would need an exclusive one, and that is
//! `iterate_mutably()`, which is a different call in the subject.
//!
//! *The cost*, stated: the chain is not materialised, so the exclusive borrow
//! of the chain that `next(mutable self)` implies is not in the IR. Nothing can
//! observe its absence today, because the only thing that would conflict with
//! it is a second use of the same chain and a bare `for` makes exactly one. It
//! arrives with `iterate()`, alongside the callee below.
//!
//! ## 7.2 The callee, which is not
//!
//! **Decision. The element-producing call keeps [`Unresolved::IterateNext`],
//! and the reason has changed.** It used to be that there was no `next` to
//! name. There is one, and `science-types`'s `check` already finds it:
//! `iterate_item` runs `Methods::lookup(key, "next", Form::Value)`, checks the
//! candidate came from `Iterate`, and reads the loop's binding out of the
//! candidate's return type. It then **discards the candidate and keeps only the
//! type**, so `thir::ExprKind::For` arrives here as `{ pattern, iter, body }`
//! with no callee in it.
//!
//! *Deciding which `next` a `for` calls is method lookup*, and method lookup is
//! `science-types`'s: it needs the receiver key, the candidate set, the
//! interface check and `block_substitution`, none of which this crate has or
//! should grow. A second implementation here would be a second answer to
//! Decision 11 living in the wrong crate, and the first one to drift would be
//! the one nobody ran.
//!
//! **The seam, precisely.** `thir::ExprKind::For` needs a fourth field,
//! `next: Option<DefId>`, filled from the `candidate.method` that
//! `check::iterate_item` already has in hand at the point it reads
//! `signature(candidate.method)?.ret`. `None` keeps exactly the cases that
//! function already returns `None` for — no `Iterate` implementation in the
//! index (`Map`, today), a type parameter or a tuple subject, a `next` from
//! somewhere other than `Iterate` — and those stay [`Unresolved::IterateNext`],
//! which is what the variant is for. With the field, the callee is
//! `Callee::Def(def)` at one line of `lower_for` and the hole closes for every
//! `for` over an `Array` or a `Chars` at once.
//!
//! *Why the shape is still lowered rather than refused.* The original reason
//! holds and is now smaller: refusing would mean
//! `examples/21_compiler_shapes.science` — the acceptance case this whole note
//! exists for — does not lower, because `Scopes.lookup` and `walk` are both
//! `for` loops. A CFG with a named hole in it is worth more to the phase that
//! consumes this than no CFG, and the borrow of §7.1 is in that CFG whether or
//! not the callee is.
//!
//! *The other cost* is unchanged: if Decision 15's `TryIterate` lands with a
//! failure edge, this shape acquires a third successor and the lowering
//! changes. It is written as one function, `Builder::lower_for`, so that the
//! change is one place.
//!
//! # 8. Closures: the captures are lowered, the body is not
//!
//! This section used to read *"a closure's body is not lowered, and its
//! captures are therefore not borrows MIR can see"*, and `lib.rs` §7 called it
//! the largest hole in the crate. The *therefore* was the mistake. The two
//! halves are separable, and the half that matters to everything downstream is
//! the first one.
//!
//! > **Decision. Every capture is a *borrow* of the captured place, taken at
//! > the point the closure value is created and held for as long as the closure
//! > value is live. Shared, unless the closure's body writes through the place,
//! > in which case exclusive. There is no by-value capture and no copy capture,
//! > not even for a `Copy` type.**
//!
//! ## 8.1 Why a borrow, and why uniformly
//!
//! A capture discipline is a *language* decision and
//! `collections-and-chains.md` owns it, so the thing this level must not do is
//! answer it by accident. What that note has already decided is one-sided and
//! it all points the same way: §2.3 says in as many words that *"a chain value
//! therefore **is** a borrow of its source"*, §4.4 gives the whole ownership
//! table for a chain without a by-value row anywhere in it, §2.4 says a chain
//! cannot leave the function in F0, and §7.6 item 6 asks for the capture set to
//! be *recorded* from F0 while saying the checks on it are F2's. What it has
//! **not** decided is whether a closure that names an owned local takes it or
//! borrows it.
//!
//! **A uniform borrow capture is the discipline that leaves that undecided.**
//! The two answers differ observably — under a by-value capture
//! `let mutable n be 0`, a closure reading `n`, then `n be 1`, then calling it
//! yields `0`; under a borrow capture it yields `1` — and under a uniform
//! borrow capture **that program does not compile**, because the write to `n`
//! conflicts with a live shared borrow (rule 4). So no F0 program can observe
//! which answer the language will pick, and the note can still pick either
//! without this file changing. A `Copy` exception would look free and would
//! quietly settle it for every integer, which is why there is not one.
//!
//! *Why not infer by-value from use, as Rust does.* Rust's inference is not
//! expensive because the lattice is hard; it is expensive because the answer is
//! **observable in the type** — `Fn`, `FnMut` and `FnOnce` are three traits and
//! choosing between them feeds back into type checking. §1.2 of that note has
//! already closed that door by making a closure type a bare arrow, `(A) -> B`,
//! with no capture set in it. With no trait to select, the only thing an
//! inference could buy here is permissiveness, and permissiveness is the one
//! thing a phase with no tests for its own new construct should not buy first.
//!
//! ## 8.2 Which borrow, and the doubt that is left
//!
//! [`crate::capture`]'s §5 classifies each capture as a read, a write or a
//! consume, syntactically. A write is a [`BorrowKind::Exclusive`] borrow. A
//! read is [`BorrowKind::Shared`]. **A consume is exclusive when the value
//! produced is not trivially copyable and shared when it is** — the same
//! `is_copy` §5 uses, pointed the same way: a consume is a move this discipline
//! has no spelling for, and the strongest borrow is the closest thing to it
//! that can be said.
//!
//! *Asked of the value produced, not of the capture's root.*
//! `each giving captured.port` produces an `Int`, which copies, so the capture
//! is shared although `Config` itself does not copy.
//! [`crate::capture::Capture::consumed`] is the list of nodes the question is
//! asked of, and asking it of the root instead would take an exclusive borrow
//! of every record a closure reads one integer field out of.
//!
//! **What that leaves is one hole and it is worth stating exactly.** A closure
//! body that genuinely *moves* a capture out is modelled as an exclusive borrow
//! of it, so the move is invisible to [`crate::moves`] and a use after the
//! closure's last use is a `SC0301` nobody reports. It is not *any* later use:
//! the capture borrow is live for the closure's whole life, so every use while
//! the closure can still be called is refused by rule 4. The unreported window
//! is exactly the one after the closure is dead — which in F0 is inside the
//! same frame, because §2.4 of that note keeps a chain from leaving one.
//!
//! ## 8.3 A capture names the referent, not the reference
//!
//! The place a capture borrows is [`Builder::auto_deref`] of the captured
//! local, which is §9's rule applied one construct further. A closure inside a
//! method whose `self` is already a reference captures `(*_1)` and never `_1`,
//! so the loan points at the caller's storage rather than at this frame's, and
//! rule 5 does not refuse every closure written inside a method.
//!
//! ## 8.4 A capture is never two-phase, and the reason is §6's own
//!
//! §6 makes an exclusive borrow two-phase *in argument position*, and a closure
//! is very often in argument position — `sort(by: line giving ...)` is the
//! corpus's own case. The capture borrows inside it are taken with
//! `in_argument: false` regardless, and the reason is the sentence §6 already
//! uses to refuse two-phasing a named borrow: **a two-phase borrow is one that
//! is used exactly once, at the call that consumes it.** A capture is used
//! however many times the closure is called, at points this body does not
//! contain. Reserving it and never activating it would make it an exclusive
//! borrow that rule 4 treats as a reservation forever.
//!
//! ## 8.5 What is left, and whose it is
//!
//! **The closure's body is still not lowered.** [`Rvalue::Closure`] keeps the
//! THIR [`science_types::thir::ExprId`], and the right end state is a separate
//! [`Body`] with the captures as parameters — which is what
//! `codegen-and-linking.md`'s *"a struct of `{ fn ptr, captures }`"* will want,
//! and what makes a region unable to cross the boundary except through a
//! summary, like any other call.
//!
//! **It is blocked on one thing and it is not in this crate.** A [`Body`] is
//! keyed by a [`DefId`]; [`crate::callgraph`] partitions by [`DefId`]; Decision
//! 8's cache boundary is a [`DefId`]. `science-resolve`'s `resolve_closure`
//! allocates a definition for a closure's *parameter* and none for the closure,
//! so there is no key to file one under, and minting one is that crate's to do.
//! Until it does, the seam is exactly this: **everything that crosses between
//! the closure's body and the enclosing body is a capture, every capture is a
//! borrow in [`Body::borrows`], and the borrow is live for the whole life of
//! the closure value.** A consumer that honours rule 4 and rule 5 over that
//! table is sound about closures without reading a closure body — it is
//! *imprecise*, because it refuses at the closure's creation what a real call
//! would only conflict with at the call, and imprecise in the direction that
//! refuses.
//!
//! **What is genuinely not checked** is the closure body's own interior: a
//! mistake between two of the closure's own locals is reported by nothing,
//! because those locals exist in no MIR. Nothing outside the closure can name
//! them, so the hole does not widen past the body.
//!
//! # 9. The receiver of a method call is reborrowed, not borrowed
//!
//! §4's rule is *"a field or an index taken through a `borrowed T` has a
//! [`Projection::Deref`] inserted from the base's revealed type"*, and the
//! receiver of a method call was the one construct that took a base through a
//! borrow and did not get one.
//!
//! **Decision. The borrow this lowering inserts for a `shared self` or
//! `mutable self` receiver is taken from the receiver place after §4's
//! dereferences, and its type is read off that place rather than off the
//! receiver's THIR node.** Inside a method whose own receiver is a reference,
//! `self.other()` is `_4 = borrowed (*_1)` — a reborrow of what `self` points
//! at — and not `_4 = borrowed _1`.
//!
//! **The reason is that the alternative is not a worse spelling of the same
//! loan, it is a different loan.** A borrow of `_1` points at this frame's own
//! storage, which ends at this body's exit, so read by the rule that says a
//! borrow may not outlive its referent, *every* method returning a borrow
//! derived from a call on `self` is refused — and the type that reaches the
//! callee is `borrowed (borrowed Table)` where its parameter is
//! `borrowed Table`. Science has no dereference operator (`AGENTS.md` §1), so
//! the author cannot write the reborrow and this is the only level that can
//! insert it. `science-regions`'s `generate`'s §6 is the same finding from
//! below, written as a workaround because it was found from there.
//!
//! **What it costs.** The borrow's destination temporary is now allocated
//! *after* the receiver's own place is built, because the type of the
//! reference is not known until the dereferences are. For a receiver that
//! brings temporaries of its own — the value for `f().len()`, the index for
//! `xs[i].len()` — that moves them ahead of the reference temporary, so those
//! locals are numbered in the other order from what this crate emitted before.
//! Point identity is unaffected (§10 item 6 is about editing *another* body),
//! each local is still written exactly once, and no consumer reads a local's
//! number for meaning.
//!
//! **What it deliberately does not change.** A borrow the author *wrote* over
//! a reference-typed local — `let s be borrowed r`, where `r` is
//! `borrowed Row` — is still a borrow of `r`'s own storage, because the
//! checker gave it the type `borrowed (borrowed Row)` and that is what it is.
//! The deref is inserted where a *type* demanded it, never where a written
//! borrow said otherwise.
//!
//! # 10. `f"…"`: the first construct in F0 that is a runtime call
//!
//! This section used to be a thirty-line comment on the arm, and what it said
//! was *"what it would emit, and why it does not"*. It emits it now. The
//! specification half of that comment was right and is kept, at
//! [`Builder::lower_fstring`], which is where the decision, the borrows and the
//! cost are stated; this is the part that belongs to the file rather than to
//! the function.
//!
//! > **Decision. An interpolation lowers to §1.7's builder — one
//! > [`Callee::Runtime`] call per fragment against a `String` the caller
//! > allocated — and the entry point for a hole is chosen from the hole's type
//! > here.**
//!
//! **What it settles that no other construct did.** [`Callee::Runtime`] existed
//! for Decision 5's array operations and nothing produced one, so its own
//! documentation said *"nothing in F0's THIR produces one"* and
//! `science-codegen-llvm` refused the variant outright. Both sentences are now
//! false, and the pair of them is why the two edits had to be one change: a MIR
//! that emitted the sequence against a backend that refused the variant would
//! have been built, analysed, and declined one crate later with a worse message
//! than the comment it replaced.
//!
//! **What it does *not* change is §4's invariant**, and that is worth saying
//! because §4 is a claim about this variant. `lib.rs` §4 reads
//! *"every loop in a body's CFG comes from a `loop` or a `for` the author
//! wrote; this crate synthesises no loop"*, and an f-string of `n` fragments is
//! `n + 1` straight-line calls and no back edge. A builder written as an
//! inlined loop over the fragments would have broken it, and there was never a
//! reason to write one: the fragment list is known at compile time and its
//! length is a property of the source text.
//!
//! **The one thing this level decides that reads like a backend decision**, and
//! the reason it is here: *which* `science_string_push_*` a hole gets is a
//! question about the hole's **type**, and this is the last phase that has one.
//! `science-codegen-llvm` sees a `Callee::Runtime` and a layout; by then an
//! `I32` and an `I64` are two integer widths and not two answers to *"does the
//! runtime render this"*. So a type the runtime cannot render is refused here,
//! as [`Unresolved::Display`], rather than lowered to the nearest width and
//! found by whoever ran the program.

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

/// The `science-rt` entry points §1.7's builder is made of, by symbol.
///
/// **Named here rather than spelled at the call sites**, because a
/// [`Callee::Runtime`] is a `&'static str` and a typo in one is a symbol
/// `science-codegen`'s `RUNTIME` does not have — which is a refusal from a
/// crate that cannot say which of five call sites wrote it.
/// `tests/fstring.rs` checks the eight against that table, which is the only
/// place the two lists can be compared.
const STRING_WITH_CAPACITY: &str = "science_string_with_capacity";
const PUSH_BYTES: &str = "science_string_push_bytes";
const PUSH_I64: &str = "science_string_push_i64";
const PUSH_U64: &str = "science_string_push_u64";
const PUSH_F64: &str = "science_string_push_f64";
const PUSH_F32: &str = "science_string_push_f32";
const PUSH_BOOL: &str = "science_string_push_bool";
const PUSH_CHAR: &str = "science_string_push_char";
const PUSH_STR: &str = "science_string_push_str";

/// How one interpolation hole reaches its entry point.
///
/// Three answers rather than `Option<&'static str>` plus a flag, because the
/// two questions — *which symbol* and *by value or by pointer* — have one
/// answer each per type and asking them separately is two tables to keep in
/// step. [`Builder::push_of`] is the only thing that builds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Push {
    /// The runtime takes the value in a register: the six scalars.
    ///
    /// `widen` is `Some(name)` when the hole's width is narrower than the
    /// parameter's, and names the prelude type the hole is cast to first —
    /// `I64` for `I8`/`I16`/`I32`, `U64` for `U8`/`U16`/`U32`. That is
    /// `science_string_push_i64`'s own sentence, *"`I8`…`I64` are
    /// sign-extended by codegen before the call"*, and
    /// [`Builder::widen_hole`] is the phase that does it.
    Value { symbol: &'static str, widen: Option<&'static str> },
    /// The runtime takes a pointer to the value: `science_string_push_str`.
    Pointer(&'static str),
    /// There is no entry point for this type. §1.6's borrow is still taken and
    /// the callee is [`Unresolved::Display`]; `Builder::lower_fstring`'s
    /// *"a hole whose type has no entry point"* is why.
    Missing,
}

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
                // §4.7's *"borrows auto-dereference for assignment"*, which is
                // §4's rule on the other side of `be`. See
                // [`Builder::assign_target`].
                let place = self.assign_target(place, self.thir.ty(*value));
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
            StmtKind::Assert { cond, message } => {
                block = self.lower_assert(*cond, *message, block, span);
            }
            // A statement the resolver could not lower. Nothing is emitted and
            // nothing is reported: `ty`'s §5 discipline, one level down.
            StmtKind::Error => {}
        }
        self.pop_scope(block, span)
    }

    /// `assert(cond)` / `assert(cond, message)`: one `If`, and a runtime call
    /// on the branch where `cond` is `false`.
    ///
    /// **Not `emit_call`.** That helper decides whether a call diverges by
    /// reading a [`Callee::Def`]'s declared signature, and there is no
    /// `Named::Function` behind `assert` for one to be. The failing branch
    /// always diverges — `science_panic`/`science_panic_bytes` are both
    /// `-> Never` — so the terminator is built directly with `target: None`,
    /// the same shape `emit_call` produces for a call it *does* recognise as
    /// one.
    ///
    /// **`message` is evaluated in a scope of its own, opened and discarded
    /// around it, and not the statement's ambient one.** The ambient scope's
    /// `pop_scope` runs once, against the *surviving* block — the one where
    /// `cond` held — so a temporary registered there for `message` would
    /// collect a drop on a path where it was never built: `cond` true never
    /// evaluates `message` at all, and freeing whatever garbage sits in that
    /// local would be a use of memory the true branch never initialised.
    /// Discarding the scope instead of popping it costs nothing on the
    /// failing branch either, because `science-rt`'s panic path already runs
    /// no destructors.
    ///
    /// **Which runtime entry point depends on the operand's shape, not on
    /// its type.** A literal message lowers straight to `(ptr, len)` — the
    /// pair `science_panic_bytes` takes — because `science-codegen-llvm`'s
    /// `lower_runtime_call` only expands a `Literal::Str` constant into that
    /// pair when the parameter list is shaped for it, and `science_panic`'s
    /// one parameter is not. A `String` that is not a literal lowers to
    /// `science_panic`, which takes the address of the value the same way
    /// `panic`'s own message would.
    fn lower_assert(
        &mut self,
        cond: ExprId,
        message: ExprId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let (cond, block) = self.operand(cond, block);
        let ok = self.new_block();
        let fail = self.new_block();
        self.terminate(block, TerminatorKind::If { cond, then_block: ok, else_block: fail }, span);

        self.push_scope();
        let (message, fail) = self.operand(message, fail);
        self.scopes.pop();

        let discard = Place::local(self.push_local(Ty::UNIT, LocalKind::Temp, span));
        let symbol = match &message {
            Operand::Const(Constant::Literal(Literal::Str(_))) => "science_panic_bytes",
            _ => "science_panic",
        };
        self.terminate(
            fail,
            TerminatorKind::Call {
                callee: Callee::Runtime(symbol),
                args: vec![message],
                destination: discard,
                target: None,
            },
            span,
        );
        ok
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
                // The operand's type is read **before** it is lowered, off
                // THIR, which is the only phase that knows it: `Rvalue::Cast`'s
                // own note is why it has to be carried rather than recovered.
                let from = self.thir.ty(*operand);
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Cast { operand: value, from, ty }, span);
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
            // §8. The captures are lowered; the body is not.
            ExprKind::Closure { param, body } => {
                let (param, body) = (*param, *body);
                let (captures, block) = self.lower_captures(param, body, block);
                let rvalue = Rvalue::Closure { param, thir_body: body, captures, ty };
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
            // `f"…"` — §10's builder, emitted.
            ExprKind::FString(parts) => self.lower_fstring(dest, parts, block, span),
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

    /// §7. The shape of a `for`: one shared borrow of its subject, and a
    /// named hole where the `next()` that reads through it goes.
    fn lower_for(
        &mut self,
        dest: Place,
        pattern: PatId,
        iter: ExprId,
        body: thir::BlockId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        // §7.1. The loop's own borrow. `borrow_source` is the same helper a
        // written `borrowed x` goes through, so a subject with no place — the
        // chain temporary of `for x in xs.iterate_consuming():` — lands in a
        // temporary with a storage-dead point rather than in a special case,
        // and `auto_deref` is §9's rule: the loan names the referent and never
        // the reference, so rule 5 compares it against the right storage.
        let (source, next) = self.borrow_source(iter, block, span);
        block = next;
        let source = self.auto_deref(source);
        let source_ty = self.place_ty(&source);
        let iterator_ty = self.context.types.borrowed(false, source_ty);
        let iterator = self.temp(iterator_ty, span, block);
        // `in_argument` is `false`: a loop's borrow is not an argument borrow.
        // It is created once, read once per turn, and lives across the whole
        // loop, which is the opposite of the single-use-at-one-call shape §6
        // reserves two phases for.
        block = self.borrow_place(Place::local(iterator), false, source, block, span, false);

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

        // §7.2. The one line the seam is behind: when `thir::ExprKind::For`
        // carries the `next` that `check`'s `iterate_item` already resolves,
        // this is `Callee::Def(def)` and nothing else here changes. The
        // argument is a `Copy` either way — it is a reference — so §5's rule
        // that every operand of an unresolved call is a copy is met without a
        // `force_copy` and stays met when the callee arrives.
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
        // §4 again, on the scrutinee. `match format:` where `format` is a
        // `borrowed Format` — which is how every `def name_of(format: borrowed
        // Format)` in the corpus spells it — asks about the *referent's*
        // discriminant, and there is no other thing it could be asking about:
        // a reference has no variants. Without the step, the tag read and
        // every [`Projection::Downcast`] under it name the local holding the
        // reference, which is the same mistake §9 fixed one construct over for
        // a receiver and §7 item 7 records.
        let place = self.auto_deref(place);
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
        // §10's builder again, for the one call in the language that renders
        // its argument. See [`Builder::lower_print_rendered`].
        if let Callee::Def(def) = callee {
            if let [only] = args {
                if self.prints_by_rendering(def, *only) {
                    return self.lower_print_rendered(dest, def, *only, block, span);
                }
            }
        }
        // §5's exception, asked as the condition it states rather than as the
        // one spelling of it that used to be checked.
        let opaque = self.has_no_signature(&callee);
        let mut operands = Vec::with_capacity(args.len());
        for arg in args {
            let (operand, next) = self.argument(*arg, block);
            operands.push(if opaque { force_copy(operand) } else { operand });
            block = next;
        }
        self.emit_call(dest, callee, operands, block, span)
    }

    /// Whether `print(x)` has to render `x` before it can print it.
    ///
    /// True for the prelude's `print` called on one argument whose type is not
    /// a `String` **and which [`Builder::push_of`] has an entry point for**.
    ///
    /// **Both exclusions are load-bearing and the second one was measured.** A
    /// `String` already prints without rendering and the builder would cost it
    /// an allocation and a copy. A type the builder *cannot* render would be
    /// rewritten into a call sequence that ends in an
    /// [`Unresolved::Display`] — the same refusal, reached through a longer
    /// program — and the rewrite is not free: §1.6 makes a hole a **borrow**,
    /// so `print(config.port)` would acquire a loan the author did not write.
    /// `science-regions`' `tests/narrowing.rs` is a fixture whose whole point
    /// is that its body contains *one* borrow, and it contains a `print` of an
    /// `Int?`; rendering that would have made the fixture stop demonstrating
    /// what it exists for. A rewrite this crate performs must not change the
    /// borrows of a program it cannot lower anyway.
    ///
    /// `write` is **not** included either: it is the same shape and
    /// `science-codegen-llvm` lowers no call to it at all, so routing it
    /// through the builder would replace one refusal with another that named a
    /// construct further from the one the author wrote.
    fn prints_by_rendering(&mut self, def: DefId, arg: ExprId) -> bool {
        let entry = self.context.defs.get(def);
        if !entry.is_builtin() || entry.name != "print" {
            return false;
        }
        let ty = self.thir.ty(arg);
        let stripped = self.stripped(ty);
        if self.context.decls.prelude().is(self.context.types, stripped, "String") {
            return false;
        }
        !matches!(self.push_of(ty), Push::Missing)
    }

    /// `print(x)` where `x` is not a `String`, as `print(f"{x}")`.
    ///
    /// # The decision
    ///
    /// The argument is rendered into a temporary `String` by §1.7's builder —
    /// the same [`Builder::lower_fstring`] an `f"…"` uses, given one part and
    /// that part a hole — and `print` is then called on the temporary.
    ///
    /// # The reason
    ///
    /// `strings-formatting-and-docs.md` §4.1 is `def print(value: borrowed any
    /// Display)`, so what `print(42)` means is *"render `42` through `Display`
    /// and write the result"*. There is exactly one renderer in this compiler
    /// and it is the f-string builder: `science-rt`'s seven
    /// `science_string_push_*` entry points, reached through
    /// [`Builder::push_of`], with [`Builder::widen_hole`]'s cast in front of
    /// the six narrow integer widths. Emitting anything else would be a second
    /// renderer, and §7 item 10 is what happens when two phases disagree about
    /// one of these entry points.
    ///
    /// **`print(f"{x}")` already worked and `print(x)` did not**, which is the
    /// measurement this closes: the two spellings mean the same thing and only
    /// one of them reached an executable. A `Display` this crate cannot render
    /// is still [`Unresolved::Display`], carried by the builder, and the
    /// refusal that names the type is `science-codegen-llvm`'s — so a user type
    /// with an `implements Display:` block is refused *by its type* rather than
    /// by *"a `print` of a value that is not a `String`"*, which named the
    /// construct and not the cause.
    ///
    /// # The cost
    ///
    /// **One allocation and one free per `print` of a number**, where a
    /// renderer that wrote into the sink would need neither. §1.7 permits that
    /// elision and calls it *"opt-in to the implementation"*;
    /// [`Builder::lower_fstring`]'s own note declines it for an `f"…"` written
    /// as `print`'s argument, and declining it here keeps the two spellings one
    /// lowering rather than two.
    ///
    /// **The temporary is `Copy`, not `Move`**, which is §5's rule for a callee
    /// with no signature and is what leaves the `String` for [`crate::drops`]
    /// to release. `science-codegen-llvm`'s `lower_print` then prints without
    /// freeing and the scope's `Drop` frees — which is finding 23's
    /// arrangement, reused rather than re-derived.
    fn lower_print_rendered(
        &mut self,
        dest: Place,
        print: DefId,
        arg: ExprId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(string) = self.context.decls.prelude().ty(self.context.types, "String") else {
            // No prelude means no `String` to build, which is every
            // hand-assembled definition table in this crate's own tests. The
            // call is emitted unrendered and refused downstream by the type it
            // carries, which is the same answer as before this function
            // existed.
            let (operand, next) = self.operand(arg, block);
            return self.emit_call(dest, Callee::Def(print), vec![force_copy(operand)], next, span);
        };
        let rendered = self.temp(string, span, block);
        let parts = [thir::FStringPart::Hole(arg)];
        let block = self.lower_fstring(Place::local(rendered), &parts, block, span);
        self.emit_call(
            dest,
            Callee::Def(print),
            vec![Operand::Copy(Place::local(rendered))],
            block,
            span,
        )
    }

    /// Whether this crate knows how the callee takes its arguments. §5.
    ///
    /// **The decision.** A [`Callee::Def`] whose [`science_types::items::
    /// Signature`] is missing is as opaque as a [`Callee::Unresolved`], and its
    /// arguments are read rather than consumed for the same reason.
    ///
    /// **The reason.** §5's exception is written about a *condition* — *"a call
    /// whose signature is unknown has unknown argument passing"* — and was
    /// implemented by matching one spelling of it. `print` is the other
    /// spelling: `science-resolve`'s `builtins.rs` resolves the name, so the
    /// callee is a `Def`, and declines to declare the parameter, so there is no
    /// signature behind it. Reading a missing signature as *"takes everything
    /// by value"* is this phase deciding what a declaration it cannot see says,
    /// and it decided wrong: `strings-formatting-and-docs.md` §4.1 gives
    /// `def print(value: borrowed any Display)`, so `print(s)` moved a `String`
    /// the program still owned, drop elaboration deleted the binding's drop,
    /// and `science-codegen-llvm`'s `lower_print` — which frees what it is
    /// handed the last reference to — freed it at the first call.
    ///
    /// **The cost.** A signature-less callee that really does consume gets a
    /// double release: nothing drops it here, the callee drops it there. That
    /// cost is not new and is not payable today — the only signature-less
    /// `Def`s in the language are `print` and `write`, and §4.1 declares both
    /// `borrowed` — and the direction is the one §5 already chose for
    /// [`Callee::Unresolved`]: a leak a profiler finds rather than a
    /// use-after-free a user finds.
    fn has_no_signature(&self, callee: &Callee) -> bool {
        match callee {
            Callee::Def(def) => self.context.decls.signature(*def).is_none(),
            // A closure's type *is* its signature — `collections-and-chains.md`
            // §1.2 makes it `(A) -> B` — so `borrowed T` in it is a declared
            // parameter like any other and there is nothing unknown to be
            // conservative about.
            Callee::Indirect(_) => false,
            // Unreachable from here — a runtime call is built by §10's builder
            // and never by a THIR `Call` — and answered rather than lumped in
            // with the holes, because `science_codegen::runtime::RUNTIME` is a
            // declared signature and §10 chooses each of its operands.
            Callee::Runtime(_) => false,
            Callee::Unresolved(_) => true,
        }
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
                let (place, next) = self.borrow_source(receiver, block, span);
                block = next;
                // §9. A receiver that is already a reference is reborrowed
                // through, and the type is read off the place the deref
                // produced rather than off the receiver's THIR node, so the
                // reference handed to the callee has the callee's parameter
                // type by construction.
                let place = self.auto_deref(place);
                let ty = self.place_ty(&place);
                let borrowed = self.context.types.borrowed(mutable, ty);
                let temp = self.temp(borrowed, span, block);
                block = self.borrow_place(Place::local(temp), mutable, place, block, span, true);
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

        // The same condition as [`Builder::has_no_signature`]: a method that
        // resolved to a definition with no declared signature is as opaque as
        // one that did not resolve at all, and the receiver arm above already
        // reads it that way — `self_kind` is `None` for both.
        let unresolved = self.has_no_signature(&callee);
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

    /// `f"…"` — §1.7's *"builder over the fragments"*, as a sequence of
    /// [`Callee::Runtime`] calls.
    ///
    /// **Decision. The accumulator is the destination itself; every fragment is
    /// one call that appends to it through a fresh `mutable borrowed String`;
    /// and the entry point is chosen from the hole's *type* here, with no entry
    /// point for a type meaning a hole in the IR rather than the nearest
    /// width.**
    ///
    /// The sequence for `f"n es {n} y x es {x}"` is `science_string_new` into
    /// the destination, then `science_string_push_bytes`,
    /// `science_string_push_i64`, `science_string_push_bytes`,
    /// `science_string_push_f64` — which is what `science-codegen-llvm`'s
    /// `tests/formatting_boundary.rs` built by hand and ran before anything
    /// produced it.
    ///
    /// # The borrows, and which is which
    ///
    /// 1. **The accumulator is borrowed once per call, exclusively, and the
    ///    borrow is §6's two-phase kind** — it is taken in argument position
    ///    and used exactly once, at the call that consumes it, which is §6's
    ///    own criterion and not an exception made for this construct. A single
    ///    borrow held across the whole sequence would be an exclusive loan live
    ///    over the evaluation of every hole, and a hole is an arbitrary
    ///    expression (§1.4), so `f"{v.pop()}{v.len()}"` would be refused by
    ///    rule 4 for a conflict this lowering invented.
    /// 2. **A hole the runtime renders from a register is *read*, never
    ///    moved.** All six — `Int`/`I64`, `U64`, `F64`, `F32`, `Bool`, `Char` —
    ///    are trivially copyable, so [`Builder::read`] gives [`Operand::Copy`]
    ///    and §1.6's *"an interpolation borrows its operands"* holds without a
    ///    loan being taken for it.
    /// 3. **A hole the runtime renders through a pointer is borrowed,
    ///    shared.** `science_string_push_str` takes `*const ScienceString`, and
    ///    a `String` is not copyable, so the only two spellings available are a
    ///    move and a shared borrow — and §1.6 says which: *"`f"{doc}"` does not
    ///    move `doc`"*. A move here is the footgun that note calls *"of the
    ///    first order"*: a debugging `print` that consumes the value the next
    ///    line reads, reported as an `SC0300`-range error against a correct
    ///    program.
    ///
    /// Both hole borrows are taken **after** the accumulator's, because the
    /// accumulator is the first argument and this file evaluates arguments in
    /// order everywhere else. That is the shape §6 reserves two phases for, and
    /// it is why the reservation and the activation are not the same point.
    ///
    /// # What drops the accumulator
    ///
    /// **Nothing here, and that is the point.** The destination is a place the
    /// caller allocated (§1), so it is registered in the caller's scope and
    /// [`Builder::emit_scope_exit`] emits its drop — which [`crate::drops`]
    /// then elaborates. For `print(f"…")` the destination is the temporary
    /// [`Builder::operand`] made, the `print` call moves it, and elaboration
    /// deletes the drop; the call site frees it, which is
    /// `science-codegen-llvm`'s `lower_print` rule and not a new one. For
    /// `let s be f"…"` the drop stands and becomes a `science_string_free`.
    /// A drop emitted from here would be a second one.
    ///
    /// # A hole whose type has no entry point
    ///
    /// The call is emitted with [`Unresolved::Display`] and its arguments are
    /// the ones a resolved push would have had. That is `lib.rs` §3's hole
    /// discipline and it is deliberately not [`Rvalue::Error`]: the borrows
    /// above are right whatever the renderer turns out to be, so region
    /// inference gets the loans of a correct program and codegen gets a refusal
    /// that can name the type. §5's *"every argument of a `Callee::Unresolved`
    /// call is `Copy`"* is **not** applied, because the reason for it does not
    /// hold: the argument passing of a push is known — it is in `RUNTIME` — and
    /// what is missing is a renderer for one type, not a signature.
    ///
    /// # The cost
    ///
    /// One local per fragment for the accumulator reference, one more per
    /// borrowed hole, and one [`BorrowId`] each: `f"a{b}c{d}"` is five calls,
    /// seven temporaries and six loans. §1.7's permitted elision, rendering
    /// straight into the sink for an `f"…"` written as `print`'s argument, is
    /// not done: it is *"opt-in to the implementation"* and this implementation
    /// has not opted in.
    ///
    /// # §1.7's capacity, and what it is worth
    ///
    /// The constructor is [`Builder::capacity_estimate`]'s number through
    /// `science_string_with_capacity`, which is §1.7's *"the capacity
    /// pre-computed from the literal fragments plus a per-type estimate for
    /// each hole, so the common case is one allocation"*. This entry used to
    /// say the opposite — *"is **not** done — there is no
    /// `science_string_with_capacity` in `RUNTIME`"* — and §7 item 9 is the
    /// finding that the ABI, not the lowering, was what the sentence was
    /// blocked on.
    ///
    /// **It is measured and not asserted.** `science-rt`'s `tests/capacity.rs`
    /// counts calls into the system allocator for exactly this call sequence:
    /// the acceptance case `f"n es {n} y x es {x}"` costs **three** growths
    /// starting from `science_string_new` and **one** allocation in total
    /// starting from `science_string_with_capacity(57)`. `tests/fstring.rs`
    /// asserts that 57 is the number this function computes, so the two halves
    /// of §1.7 meet at a constant a test on each side names.
    fn lower_fstring(
        &mut self,
        dest: Place,
        parts: &[thir::FStringPart],
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let capacity = self.capacity_estimate(parts);
        let mut block = self.emit_call(
            dest.clone(),
            Callee::Runtime(STRING_WITH_CAPACITY),
            vec![Operand::Const(Constant::Count(capacity))],
            block,
            span,
        );
        if parts.is_empty() {
            return block;
        }
        // **One discard slot for the whole sequence, not one per call.** Every
        // push returns nothing and [`TerminatorKind::Call`] has a destination
        // whether or not there is a value for it, so the slot is storage the
        // author did not write and the fewest of them is the right number. It
        // is `()` rather than [`Ty::ERROR`] because the return type of these
        // calls is genuinely unit; only a discriminant read has no Science type
        // at all. Re-assigning it once per fragment costs nothing:
        // [`crate::moves`] asks for a drop flag only where the type needs
        // dropping, and `()` does not.
        let discard = Place::local(self.temp(Ty::UNIT, span, block));
        for part in parts {
            match part {
                // The lexer emits one token per text run, so an empty one
                // cannot be written; the guard is here because
                // `science_string_push_bytes` returns early on a zero length
                // and a call known to do nothing is a call not worth emitting.
                thir::FStringPart::Text(text) if text.is_empty() => {}
                thir::FStringPart::Text(text) => {
                    let text = text.clone();
                    let (accumulator, next) = self.accumulator_ref(&dest, block, span);
                    block = next;
                    // The literal crosses two C parameters — `(ptr, len)` — and
                    // MIR has no operand that names a global, so it stays a
                    // `Literal::Str` here and `science-codegen-llvm`'s
                    // `lower_runtime_call` interns it and expands it. That
                    // convention is stated there, at the arm that applies it.
                    let bytes = Operand::Const(Constant::Literal(Literal::Str(text)));
                    block = self.emit_call(
                        discard.clone(),
                        Callee::Runtime(PUSH_BYTES),
                        vec![accumulator, bytes],
                        block,
                        span,
                    );
                }
                thir::FStringPart::Hole(hole) => {
                    let hole = *hole;
                    let hole_ty = self.thir.ty(hole);
                    let push = self.push_of(hole_ty);
                    let (accumulator, next) = self.accumulator_ref(&dest, block, span);
                    block = next;
                    let (value, next) = match push {
                        Push::Value { .. } => self.value_hole(hole, block),
                        Push::Pointer(_) | Push::Missing => self.borrow_hole(hole, block, span),
                    };
                    block = next;
                    // §10.1: the narrow widths reach their entry point through
                    // a cast, which is the sentence `science_string_push_i64`
                    // has always claimed and nothing used to do.
                    let (value, next) = match push {
                        Push::Value { widen: Some(wide), .. } => {
                            self.widen_hole(value, hole_ty, wide, block, span)
                        }
                        _ => (value, block),
                    };
                    block = next;
                    let callee = match push {
                        Push::Value { symbol, .. } | Push::Pointer(symbol) => {
                            Callee::Runtime(symbol)
                        }
                        Push::Missing => Callee::Unresolved(Unresolved::Display),
                    };
                    block = self.emit_call(
                        discard.clone(),
                        callee,
                        vec![accumulator, value],
                        block,
                        span,
                    );
                }
            }
        }
        block
    }

    /// The accumulator, as one call's first argument: `mutable borrowed String`
    /// into a fresh temporary.
    ///
    /// The reference's type is read off the *place* and not off the f-string's
    /// THIR node, which is §9's rule and has §9's reason: a destination that is
    /// a field of a record is a `String` at that field's declared type, and a
    /// reference built from the node's type would be a second `Ty` for one
    /// place.
    fn accumulator_ref(&mut self, dest: &Place, block: BlockId, span: Span) -> (Operand, BlockId) {
        let ty = self.place_ty(dest);
        let borrowed = self.context.types.borrowed(true, ty);
        let temp = self.temp(borrowed, span, block);
        let block = self.borrow_place(Place::local(temp), true, dest.clone(), block, span, true);
        (Operand::Move(Place::local(temp)), block)
    }

    /// A hole the runtime renders from a register, read rather than consumed.
    ///
    /// [`Builder::auto_deref`] is §4's rule and it is what makes `f"{n}"` work
    /// where `n` is a `borrowed Int` parameter: the place is dereferenced to
    /// the `Int` and the `Int` is what the entry point is given. Without it the
    /// operand would be a reference at a parameter declared `i64`, which is
    /// `tests/formatting_boundary.rs`'s whole subject one argument over.
    fn value_hole(&mut self, hole: ExprId, block: BlockId) -> (Operand, BlockId) {
        match self.as_place(hole, block) {
            Some((place, block)) => {
                let place = self.auto_deref(place);
                let ty = self.place_ty(&place);
                let operand = self.read(place, ty);
                (operand, block)
            }
            // A hole with no place — `f"{a + b}"`. The value goes into a
            // temporary that dies with the statement, exactly as
            // [`Builder::operand`] already arranges, and reading a temporary
            // nothing else names is a read whichever of §5's two operands it is
            // spelled with.
            None => self.operand(hole, block),
        }
    }

    /// A hole the runtime renders through a pointer, borrowed shared. §1.6.
    ///
    /// Reborrowed through, for §9's reason: a hole that is already a
    /// `borrowed String` gives `borrowed (*_1)` and not `borrowed _1`, so the
    /// pointer handed to `science_string_push_str` is the one the caller's
    /// `String` lives at rather than the address of this frame's parameter
    /// slot. That is not a spelling difference — it is the difference between a
    /// `ScienceString` and a pointer to one, read as a `ScienceString`.
    fn borrow_hole(&mut self, hole: ExprId, block: BlockId, span: Span) -> (Operand, BlockId) {
        let (place, block) = self.borrow_source(hole, block, span);
        let place = self.auto_deref(place);
        let ty = self.place_ty(&place);
        let borrowed = self.context.types.borrowed(false, ty);
        let temp = self.temp(borrowed, span, block);
        let block = self.borrow_place(Place::local(temp), false, place, block, span, true);
        (Operand::Move(Place::local(temp)), block)
    }

    /// Which `science_string_push_*` renders a hole of this type, and how it
    /// takes it.
    ///
    /// **The list is `science-rt`'s and it is closed.** §1.7 names the builder,
    /// `science-codegen`'s `RUNTIME` names the seven entry points that exist,
    /// and there is no eighth to be had.
    ///
    /// **The six narrow integer widths reach one of them through a cast**, and
    /// this entry used to say they could not. `science_string_push_i64`'s own
    /// note reads *"`I8`…`I64` are sign-extended by codegen before the call"*
    /// and `science_string_push_u64`'s reads the zero-extending version of the
    /// same sentence; §7 item 10 was that **no phase did it**, because
    /// `Rvalue::Cast` had no lowering in `science-codegen-llvm` and so there
    /// was nothing between this decision and the call that could. It has one
    /// now, so `I8`/`I16`/`I32` answer `push_i64` behind a cast to `I64` and
    /// `U8`/`U16`/`U32` answer `push_u64` behind a cast to `U64`.
    ///
    /// **The extension's signedness is the source's and that is the whole
    /// reason the two lists are separate.** `-1i32` widened as signed is `-1`
    /// and widened as unsigned is `4294967295`; both are `i64` bit patterns a
    /// verifier accepts and only one of them is the number the author wrote.
    /// Sending an `I32` through `push_u64` would print the second.
    ///
    /// `F16` and `BF16` are **not** widened to `F32`, although the instruction
    /// exists. §2.3 makes the default rendering *"the shortest decimal string
    /// that round-trips"*, which is a property of the width —
    /// `science_string_push_f32`'s own note says so about `F32` against `F64`
    /// — so rendering an `F16` through a wider entry point answers a question
    /// about a type the value does not have. They stay [`Push::Missing`].
    ///
    /// **A borrow is looked through.** A `borrowed Int` hole renders as an
    /// `Int`; [`Builder::value_hole`] and [`Builder::borrow_hole`] each insert
    /// §4's dereference to match. Anything else — `Float`, `F16`, a record, an
    /// `any Display` — is [`Push::Missing`], which is honest: the type checker
    /// accepts a hole whose type merely *implements* `Display`, and
    /// `science-types`'s `check` already records that *"the interpolation of a
    /// user type is therefore accepted here and refused by codegen"*.
    fn push_of(&mut self, ty: Ty) -> Push {
        let ty = self.revealed(ty);
        if let TyKind::Borrowed { inner, .. } = *self.context.types.kind(ty) {
            return self.push_of(inner);
        }
        let prelude = self.context.decls.prelude();
        let types = &*self.context.types;
        let is = |name: &str| prelude.is(types, ty, name);
        let direct = |symbol| Push::Value { symbol, widen: None };
        if is("Int") || is("I64") {
            direct(PUSH_I64)
        } else if is("I8") || is("I16") || is("I32") {
            Push::Value { symbol: PUSH_I64, widen: Some("I64") }
        } else if is("U64") {
            direct(PUSH_U64)
        } else if is("U8") || is("U16") || is("U32") {
            Push::Value { symbol: PUSH_U64, widen: Some("U64") }
        } else if is("F64") {
            direct(PUSH_F64)
        } else if is("F32") {
            direct(PUSH_F32)
        } else if is("Bool") {
            direct(PUSH_BOOL)
        } else if is("Char") {
            direct(PUSH_CHAR)
        } else if is("String") {
            Push::Pointer(PUSH_STR)
        } else {
            Push::Missing
        }
    }

    /// A narrow integer hole, cast to the width its entry point declares.
    ///
    /// One [`Rvalue::Cast`] into one temporary, which is the same shape every
    /// other implicit step in this file takes — a receiver borrow, a capture, a
    /// coercion. The cast's `from` is the hole's own type with its borrows
    /// stripped, because [`Builder::value_hole`] has already inserted §4's
    /// dereference and the value in hand is the referent's.
    ///
    /// **A hole whose type this cannot name is left alone rather than cast to
    /// a guess.** `prelude.ty` answers `None` for a hand-built definition table
    /// with no prelude in it — every test in this crate that does not go
    /// through the resolver — and a cast to `Ty::ERROR` would be a statement
    /// `science-codegen-llvm` refuses with a message naming a type the program
    /// does not contain. The un-widened operand is refused too, one call
    /// later, with the type the program *does* contain in the message.
    fn widen_hole(
        &mut self,
        value: Operand,
        hole_ty: Ty,
        wide: &str,
        block: BlockId,
        span: Span,
    ) -> (Operand, BlockId) {
        let from = self.stripped(hole_ty);
        let decls = self.context.decls;
        let Some(target) = decls.prelude().ty(self.context.types, wide) else {
            return (value, block);
        };
        let temp = self.temp(target, span, block);
        self.assign(
            block,
            Place::local(temp),
            Rvalue::Cast { operand: value, from, ty: target },
            span,
        );
        (Operand::Move(Place::local(temp)), block)
    }

    /// A type with its aliases revealed and every enclosing borrow removed.
    ///
    /// [`Builder::push_of`] recurses through borrows to choose an entry point
    /// and throws the stripped type away; this is the same walk, kept.
    fn stripped(&mut self, ty: Ty) -> Ty {
        let ty = self.revealed(ty);
        match *self.context.types.kind(ty) {
            TyKind::Borrowed { inner, .. } => self.stripped(inner),
            _ => ty,
        }
    }

    /// §1.7's capacity: the literal fragments' bytes plus a per-type estimate
    /// for each hole.
    ///
    /// **Every number below is an estimate and none is a bound**, which is the
    /// word §1.7 uses and the property `science_string_with_capacity`'s own
    /// note prices: too small costs the growth the estimate was meant to
    /// avoid, and too large is memory held for the life of the string, because
    /// `String` has no `shrink_to_fit` and §8 lists none. So each integer width
    /// gets **its own longest decimal spelling** rather than `I64`'s — `I8` is
    /// four bytes for `-128` and not twenty — and the two that cannot be
    /// bounded at all are guesses said to be guesses:
    ///
    /// | Hole | Estimate | Why |
    /// |---|---|---|
    /// | `I8` … `U64` | 4, 6, 11, 20 signed; 3, 5, 10, 20 unsigned | the longest value the width can hold, sign included |
    /// | `Bool` | 5 | `false` |
    /// | `Char` | 4 | the longest UTF-8 encoding of a scalar value |
    /// | `F32` | 16 | a **guess**. `{:?}` on an `f32` is shortest-round-trip, so `1e-30` is five bytes and `-1.1754944e-38` is fourteen; there is no short bound and the tail is rare |
    /// | `F64` | 24 | the same guess one width up. `0.1 + 0.2` renders `0.30000000000000004`, nineteen bytes, which is the case §2.3 exists to keep visible |
    /// | `String` | 16 | a **guess**, and the only hole whose true size is known at run time and not here |
    /// | anything else | 0 | there is no renderer, so there will be no bytes |
    ///
    /// The `F64` and `String` numbers are the two worth revisiting with a
    /// measurement; the rest are arithmetic.
    fn capacity_estimate(&mut self, parts: &[thir::FStringPart]) -> u64 {
        let mut total: u64 = 0;
        for part in parts {
            total += match part {
                thir::FStringPart::Text(text) => text.len() as u64,
                thir::FStringPart::Hole(hole) => {
                    let ty = self.thir.ty(*hole);
                    self.hole_estimate(ty)
                }
            };
        }
        total
    }

    /// One hole's contribution to [`Builder::capacity_estimate`].
    ///
    /// A table rather than a chain of comparisons, because the table is what
    /// the note above is: fourteen rows, each a type and a number, and a reader
    /// checking one against the other should not have to read control flow to
    /// do it. A type not in it contributes nothing, which is right for both
    /// kinds of absence — a hole with no renderer will produce no bytes, and a
    /// build with no prelude has no types to match.
    fn hole_estimate(&mut self, ty: Ty) -> u64 {
        const ESTIMATES: &[(&str, u64)] = &[
            ("Int", 20),
            ("I64", 20),
            ("U64", 20),
            ("I32", 11),
            ("U32", 10),
            ("I16", 6),
            ("U16", 5),
            ("I8", 4),
            ("U8", 3),
            ("F64", 24),
            ("F32", 16),
            ("String", 16),
            ("Bool", 5),
            ("Char", 4),
        ];
        let ty = self.stripped(ty);
        let prelude = self.context.decls.prelude();
        let types = &*self.context.types;
        ESTIMATES
            .iter()
            .find(|(name, _)| prelude.is(types, ty, name))
            .map(|(_, bytes)| *bytes)
            .unwrap_or(0)
    }

    /// §8's discipline, applied: one borrow per capture, in first-mention
    /// order, each into a temporary the closure aggregate then holds.
    ///
    /// A temporary and an ordinary [`Rvalue::Ref`] rather than a new statement
    /// kind, because that is what §3's three-address form already does for
    /// every other aggregate: `Doc(source: borrowed doc)` is a `Ref` into a
    /// temporary and a [`Rvalue::Record`] holding it. A capture that needed its
    /// own borrow-taking rvalue would be a second spelling of a loan, and
    /// [`index_borrows`], [`crate::drops`] and every consumer of
    /// [`Body::borrows`] would each need to learn it.
    ///
    /// **A capture whose binding this body has no local for is skipped.** That
    /// is a name the closure resolved to something that is not storage in this
    /// frame — which the resolver would have made an
    /// [`ExprKind::Item`] rather than an [`ExprKind::Local`], so it does not
    /// happen — and skipping is `ty`'s §5 discipline rather than an assertion:
    /// a hole is a missing capture, not a panic.
    fn lower_captures(
        &mut self,
        param: DefId,
        body: ExprId,
        mut block: BlockId,
    ) -> (Vec<Operand>, BlockId) {
        let found = crate::capture::captures_of(self.context.decls, self.thir, param, body);
        let mut captures = Vec::with_capacity(found.len());
        for capture in found {
            let Some(local) = self.bindings.get(&capture.def).copied() else { continue };
            // §8.3: the referent, not the reference.
            let place = self.auto_deref(Place::local(local));
            let ty = self.place_ty(&place);
            // §8.2. A consume is a move this discipline cannot spell, so it
            // takes the strongest borrow instead — unless every consumed node
            // copies, in which case there was no move to be strong about. The
            // question is asked of the *node* consumed and not of the capture's
            // root: `each giving c.port` produces an `Int`, whatever `c` is.
            let consumed = capture.consumed.clone();
            let consumed_moves = consumed.iter().any(|node| {
                let node_ty = self.thir.ty(*node);
                !self.is_copy(node_ty)
            });
            let mutable = match capture.use_kind {
                crate::capture::Use::Read => false,
                crate::capture::Use::Consume => consumed_moves,
                crate::capture::Use::Write => true,
            };
            let borrowed = self.context.types.borrowed(mutable, ty);
            let temp = self.temp(borrowed, capture.span, block);
            // §8.4: `in_argument` is false however the closure got here.
            block =
                self.borrow_place(Place::local(temp), mutable, place, block, capture.span, false);
            captures.push(Operand::Move(Place::local(temp)));
        }
        (captures, block)
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
        let (place, block) = self.borrow_source(operand, block, span);
        self.borrow_place(dest, mutable, place, block, span, in_argument)
    }

    /// The place a borrow of this expression is taken *from*.
    ///
    /// Split out of [`Builder::lower_borrow`] because §9's receiver reborrow
    /// needs the place before the borrow is taken, to project through it.
    fn borrow_source(&mut self, operand: ExprId, block: BlockId, span: Span) -> (Place, BlockId) {
        match self.as_place(operand, block) {
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
        }
    }

    /// Takes the borrow, once the referent is a place.
    ///
    /// The half that decides the [`BorrowKind`] — §6's reservation set — and so
    /// the half a caller must not reimplement: a receiver reborrow reaches
    /// here with the same `in_argument` a written borrow does, and is
    /// classified by the same two bits.
    fn borrow_place(
        &mut self,
        dest: Place,
        mutable: bool,
        place: Place,
        block: BlockId,
        span: Span,
        in_argument: bool,
    ) -> BlockId {
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

    /// An assignment's target, dereferenced as far as the value's type asks.
    ///
    /// # The decision
    ///
    /// `counter be 1`, where `counter` is `mutable borrowed Int`, assigns to
    /// `(*_1)` and not to `_1`. The walk stops as soon as the place's type is
    /// the value's, so `r be borrowed y` — where `r` is itself a reference and
    /// the value is one too — assigns to `r` and derefs nothing.
    ///
    /// # The reason
    ///
    /// The core spec's §4.7 is *"borrows auto-dereference for assignment"*, and
    /// **Science has no dereference operator at all**, so a write through a
    /// reference has no other spelling: `def bump(counter: mutable borrowed
    /// Int): counter be counter + 1` in `examples/01_functions.science` is what
    /// the rule exists for and the language gives the author nothing else to
    /// write. §4 of this file already inserts the same step on a *read* through
    /// a field; this is its other half, and `thir::StmtKind::Assign` carries
    /// the target expression exactly as written, so nothing above this crate
    /// supplies it.
    ///
    /// **It is half a repair and the other half is above Decision 42's line**,
    /// which is why this entry says what it can be held to rather than claiming
    /// the acceptance case. `science-types` types `counter be 5` as
    /// `SC0525`, *expected `mutable borrowed Int`, found an integer literal* —
    /// it compares the target's declared type against the value's and inserts
    /// no dereference — and it types `counter + 1` as `mutable borrowed Int`,
    /// so the one spelling that *does* check has a value whose type already
    /// equals the target's and the walk below stops immediately. So `bump` is
    /// not fixed by this function; it is refused by
    /// `science-codegen-llvm`'s `lower_binary`, which names the seam.
    ///
    /// **What this is for, then, is the shape the repair above will produce.**
    /// A checker that dereferences the target types the value at the
    /// *referent*, and a MIR that then wrote `_1 = <Int>` would be storing an
    /// `Int` into a slot holding a reference — with nothing to report it: this
    /// crate emits no diagnostics (§3), `science-regions` would read a write
    /// *to* the reference rather than *through* it, which is a different fact
    /// for rules 4 and 5, and `science-codegen-llvm` would stop it with *"a
    /// constant of a type this backend cannot build"*, a message about a
    /// constant for a mistake in a place. `tests/places.rs` pins it on a
    /// fixture that lowers without checking, which is what this crate's
    /// harness exists to allow.
    ///
    /// # The cost
    ///
    /// **A rule written against a phase that has not landed is a rule nothing
    /// exercises**, and that is exactly the shape §3 finding 24 of
    /// `science-codegen-llvm` is about — a guard for a case that does not
    /// arrive. The difference, and the reason this one is kept: that guard was
    /// a *refusal* and it blocked every correct program; this is a *lowering*
    /// and it blocks nothing, because a target whose type already equals the
    /// value's is returned untouched.
    ///
    /// The stopping condition is a type equality and not a subtyping question,
    /// so a target and a value that are *compatible* rather than *equal* —
    /// which `crate::assign`'s coercions decide and this crate cannot — stop
    /// the walk one step early or one step late. Every such pair in F0 arrives
    /// with a `thir::ExprKind::Coerce` around the value, whose own type is the
    /// target's, so the equality is exact for everything that reaches here.
    fn assign_target(&mut self, mut place: Place, value_ty: Ty) -> Place {
        let value_ty = self.revealed(value_ty);
        loop {
            let written = self.place_ty(&place);
            let ty = self.revealed(written);
            if ty == value_ty {
                return place;
            }
            let TyKind::Borrowed { inner, .. } = *self.context.types.kind(ty) else {
                return place;
            };
            place = place.project(Projection::Deref { ty: inner });
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
            //
            // **The guard is on the operand and not on the coercion**, which is
            // what keeps §4a's `Box of C` into `Box of any I` out of this arm
            // without naming it. That one's operand is a `Box.new` call, there
            // is no loan under it to reserve, and the ordinary `operand` path
            // below is the right one for it.
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
