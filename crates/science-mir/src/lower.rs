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
//! false positive. `science-regions`' `moved` §3 used to pay part of that bill
//! on its own side, by reporting rule 3 only where the type owns something. It
//! no longer does: a record of scalars that does not declare `Copy` *is* moved
//! by rule 2, so the "copy called a move" that bill paid for was, for a user
//! record, a move called a move. What is left of the bill is this crate's own
//! mistakes — a `Coercion::Copy` load through a `&mut` used to come out as a
//! move of the borrow, and `ExprKind::Coerce`'s arm now reads it instead.
//! Reversing this rule *wholesale* would still move the cost back onto drop
//! elaboration, where the direction above is right.
//!
//! # 5.1. `T implements Copy` is asked, and this paragraph used to say it could
//! not be
//!
//! §5 above used to end: *"whether a user type is `Copy` is Decision 11's
//! lookup and this compiler has none, so every record operand arrives here as a
//! move."* **The second half of that sentence was false when it was written and
//! the first half is now false too.** `science-types`' `assign.rs` §7 needs the
//! same id for the borrowed-to-owned coercion and finds it with
//! [`Coercions::of`]; [`Builder::copy_interface`] reads it from there, beside
//! the `Box` this file was already reading out of the same call.
//!
//! So [`Builder::is_copy`]'s `Named` arm asks it, and §6.1 rule 2's own
//! exception — *"assigning, passing, or returning moves ownership, **unless the
//! type is `Copy`**"* — reaches the operand rule that is supposed to implement
//! rule 2. [`Builder::declares_copy`] is the predicate.
//!
//! **The doubt still goes §5's way, and the guard is not decoration.** Nothing
//! in this compiler checks that a `Copy` implementation is well formed: `type
//! P: text: String` followed by `P implements Copy` is a program the resolver
//! and the checker both accept today. Believing that declaration would copy a
//! `String`'s three words, leave the source's drop standing and free the buffer
//! twice — a miscompile manufactured out of a front-end gap. So
//! [`Builder::declares_copy`] requires the type to own nothing as well as to
//! declare `Copy`, which makes the copy and [`crate::drops`]'s elaboration agree
//! by construction and keeps §5's asymmetry exactly where §5 wants it: wherever
//! there is doubt.
//!
//! **What it cost `science-regions` was nothing and what it bought was a
//! program.** `a is a` — `is` dispatching to a user `eq`, whose bare `self` is a
//! *shared borrow* of the receiver while `other` was a `Move` of the same local
//! — was refused by rule 4 on a `Copy` record. The rule was right; the operand
//! was wrong. `tests/copy_operands.rs` measures all four corners and
//! `science-regions`' `check` §3.1 is what is left of the message.
//!
//! **A second exception: the six comparison operators never move.**
//! `ExprKind::Binary`'s arm for `Eq`, `Ne`, `Lt`, `Gt`, `Le` and `Ge` runs
//! [`force_copy`] over both operands `self.operand` already read, the same
//! correction [`ExprKind::Present`] takes for the same reason — a comparison
//! produces a fresh `Bool` and leaves both inputs exactly where they were.
//! Without it, `s is ""` on an owned `String` arrives at
//! `science-codegen-llvm` as a whole-value `Move`: `crate::moves` reads that
//! as consuming `s`, `crate::drops` deletes the binding's `Drop`, and nothing
//! downstream frees the buffer — a leak, not the double free rule 3 usually
//! answers, because a comparison never writes the moved-from place back. The
//! fix is this one after-the-fact correction rather than teaching `read` that
//! `String` is `Copy`: every *other* caller of `read` — an assignment, a call
//! that takes ownership, a `return` — is asking the type's real answer, and
//! `is_copy` still has to give it to them.
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
//! **Closed. `thir::ExprKind::For` carries `next: Option<DefId>` and this
//! crate reads it.** What follows is the argument that produced the field,
//! kept because it is still the reason the field is where it is rather than
//! a lookup repeated here.
//!
//! `science-types`'s `check` finds the callee: `iterate_item` runs
//! `Methods::lookup(key, "next", Form::Value)`, checks the candidate came
//! from `Iterate`, and reads the loop's binding out of the candidate's return
//! type. It used to then **discard the candidate and keep only the type**, so
//! `ExprKind::For` arrived here as `{ pattern, iter, body }` with no callee
//! in it. It now keeps both.
//!
//! *Deciding which `next` a `for` calls is method lookup*, and method lookup is
//! `science-types`'s: it needs the receiver key, the candidate set, the
//! interface check and `block_substitution`, none of which this crate has or
//! should grow. A second implementation here would be a second answer to
//! Decision 11 living in the wrong crate, and the first one to drift would be
//! the one nobody ran.
//!
//! **The seam, as built.** The fourth field is filled from the
//! `candidate.method` that `check::iterate_item` already has in hand at the
//! point it reads `signature(candidate.method)?.ret`. `None` keeps exactly
//! the cases that function already returns `None` for — no `Iterate`
//! implementation in the index (`Map`, and `Array`, which has no `iterate`
//! yet), a type parameter or a tuple subject, a `next` from somewhere other
//! than `Iterate` — and those stay [`Unresolved::IterateNext`], which is what
//! the variant is still for. The callee is `Callee::Def(def)` at one line of
//! `lower_for`, and `owner_is_a_type` is the filter that decides between the
//! two.
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
//! ## 8.5 The body, and the key it is filed under
//!
//! This section used to open *"the closure's body is still not lowered"* and
//! say it was blocked on `science-resolve` minting a [`DefId`] for the closure
//! itself. It is not blocked and it is not unlowered. [`Rvalue::Closure`] still
//! keeps the THIR [`science_types::thir::ExprId`], and beside it there is now a
//! separate [`Body`] — *"with the captures as parameters"*, which is what that
//! same paragraph already named as the right end state.
//!
//! **The key is the closure's `param`, not a [`DefId`] of its own.** A [`Body`]
//! is keyed by a [`DefId`]; [`crate::callgraph`] partitions by [`DefId`];
//! Decision 8's cache boundary is a [`DefId`]. `science-resolve`'s
//! `resolve_closure` allocates a definition for a closure's *parameter* and
//! none for the closure — and a parameter's definition is already unique per
//! closure, because that function mints a fresh one every time, including for
//! the implicit `each`. So the key existed the whole time under another name.
//! `science_codegen::mono`'s `path_of` gives it the symbol `closure$name$id`
//! for the same reason.
//!
//! **The seam is unchanged and still the thing to hold onto**: everything that
//! crosses between the closure's body and the enclosing body is a capture,
//! every capture is a borrow in [`Body::borrows`], and the borrow is live for
//! the whole life of the closure value. A consumer that honours rule 4 and rule
//! 5 over that table is sound about closures — *imprecise*, because it refuses
//! at the closure's creation what a real call would only conflict with at the
//! call, and imprecise in the direction that refuses.
//!
//! **The closure body's own interior is checked now**, which this section used
//! to list as what is *"genuinely not checked"*. Those locals exist in MIR,
//! [`crate::callgraph`] gives the body a node whether or not a
//! [`Callee::Def`] edge reaches it, and `science-regions` checks it as an
//! unreferenced singleton component.
//!
//! ## 8.7 A `Copy` capture of a closure that can leave the function is a value
//!
//! > **Decision. In a function whose return type mentions a closure type, a
//! > capture the body only reads and whose type is `Copy` is captured *by value*:
//! > the closure's environment holds a copy of it, made where the closure is
//! > created, and no loan is taken. Everything else keeps §8's borrow.**
//!
//! §8.1 left the language question open on purpose and said why no F0 program
//! could tell the two answers apart. That stopped being true the day a function
//! could return a closure: `def make(k: Int) -> (Int) -> Int: x giving x * k`
//! borrows `k`, a slot of `make`'s own frame, and returns the borrow. Rule 5
//! refuses it (`SC0333`), correctly, and the program is the most ordinary way to
//! write a closure factory. A `Copy` value has no identity to preserve, so
//! copying it is not a decision about ownership — it is the one reading under
//! which the program means something.
//!
//! *Why only a function that returns a closure.* The by-value environment has to
//! outlive the frame, so the backend gives it heap storage; a closure's third
//! word is the destructor that releases it, and any owned capture, when the
//! closure is dropped. Confining the cost to functions whose
//! result can carry a closure keeps a chain's `each giving each + n` in a loop
//! exactly as cheap as before, and a closure factory pays one small allocation
//! per closure it makes. §8.1's observable difference (a write to the captured
//! variable after the closure is made) is likewise confined to those functions,
//! where it now compiles and the closure sees the value as of its creation.
//!
//! *An owned capture moves in.* A whole local that is not `Copy`, only read by
//! the closure, in a function that may return it, is `Operand::Move`d into the
//! environment: the caller's drop elaboration sees the move, and the closure's
//! destructor (its third word) releases the value when the closure is dropped.
//! A place reached through a borrow or a field is not the closure's to take and
//! stays a borrow, as does anything the closure writes or moves out: a write
//! would make the copy and the original diverge.
//!
//! *Representation.* The capture's operand is the value (`Operand::Copy` of the
//! captured place) rather than the reference temporary §8 builds; the closure's
//! body is unchanged and still receives a reference, which the backend points at
//! the copy inside the heap environment.
//!
//! ## 8.6 A capture reaches the body as a parameter
//!
//! > **Decision. A closure's body takes one trailing parameter per capture,
//! > holding the reference §8's discipline took, in [`crate::capture`]'s
//! > first-mention order. Inside the body the binding is that parameter behind
//! > a [`Projection::Deref`].**
//!
//! **Why a parameter and not a new local kind.** §8.5's own sentence asked for
//! *"a separate [`Body`] with the captures as parameters"*, and the reason it
//! is the right shape is that a capture is already a reference by the time it
//! crosses: [`Builder::lower_captures`] borrowed it into a temporary in the
//! enclosing frame, and a reference is the one thing this crate already knows
//! how to pass. Nothing about [`Body::params`], [`crate::drops`] or
//! [`crate::moves`] needed a new case — a `borrowed T` parameter drops nothing
//! and moves nothing, which is exactly what §8 promised a capture would be.
//!
//! **Why the `Deref` goes in [`Builder::as_place`].** The body's THIR names the
//! binding at the *referent's* type: `x giving x + n` types `n` as `Int` while
//! the parameter is a `borrowed Int`. Putting the projection at the one place a
//! name becomes a place keeps every reader — `operand`, `argument`,
//! `lower_method_call`, the f-string holes — reading the referent without
//! knowing a capture is involved. [`Builder::captured`] is the table, and it
//! holds the referent type rather than deducing it, because §9's `auto_deref`
//! peels *every* borrow and a capture of something that was already a reference
//! must stop after one.
//!
//! **What this does not make lowerable.** A closure whose body *moves* a
//! capture out. §8.2 models that as an exclusive borrow because a borrow
//! discipline has no spelling for a move, and while no body was lowered the
//! mismatch was an unreported mistake in a program that could not run. A body
//! lowered against the same model *would* run it — the owning value copied out
//! of storage the enclosing frame still drops, which is a double free rather
//! than a missing diagnostic. So [`Builder::lower_captures`] flags it, no body
//! is built, and `science-codegen-llvm` refuses the closure by name. The right
//! repair is above this crate: a `SC0301` from a checker that can see the move.
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
use science_lexer::IntBase;
use science_resolve::hir::{BinaryOp, DefId, DefKind, DefTable, Literal, SelfKind};
use science_types::alias::Aliases;
use science_types::assign::{Coercion, Coercions};
use science_types::items::Declarations;
use science_types::methods::{Form, Found};
use science_types::thir::{self, Arm, ExprId, ExprKind, PatId, PatKind, StmtKind};
use science_types::ty::{GenericArg, Ty, TyKind, Types};
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
/// Decision 10's array literal, as Decision 5's *"call to a runtime entry
/// point, never an inlined MIR loop"*: one constructor and one push per
/// element. The descriptor each of these also takes is **not** passed from
/// here — `science-codegen-llvm`'s `lower_runtime_call` fills it from the
/// destination's own element type, because a descriptor is a global that
/// crate interns and this one has no way to name one.
const ARRAY_WITH_CAPACITY: &str = "science_array_with_capacity";
const ARRAY_PUSH: &str = "science_array_push";
const ARRAY_LEN: &str = "science_array_len";
/// `for entry in counts:` and `for x in set:`: the loop's extent, and the
/// entry at a position — null for a removed entry's hole. See
/// [`Builder::lower_for_over_map`].
const MAP_EXTENT: &str = "science_map_extent";
const MAP_ENTRY_AT: &str = "science_map_entry_at";
/// `collections-and-chains.md` §1.4's one **barrier**, as the one call a
/// fused chain still makes: `sorted(by: key)` buffers, sorts and yields.
/// The keys are an `Array[Int]` the same loop computed — see the entry
/// point's own note for why they are not computed inside the comparison.
const ARRAY_SORT_BY_INT_KEY: &str = "science_array_sort_by_int_key";
/// `reverse()`'s other barrier: the buffer turned round in place.
const ARRAY_REVERSE: &str = "science_array_reverse";
const PANIC_BYTES: &str = "science_panic_bytes";
const PUSH_STR: &str = "science_string_push_str";
/// `IoError implements Display` and `TextError implements Display`, as two
/// more pushes the builder reaches by pointer. See [`Builder::push_of`].
const PUSH_IO_ERROR: &str = "science_string_push_io_error";
const PUSH_TEXT_ERROR: &str = "science_string_push_text_error";
/// `strings-formatting-and-docs.md` §3.1's `Formatter`, built over the
/// accumulator: `science_formatter_init(&mut formatter, &mut accumulator)`.
/// It takes an out-pointer rather than returning, which is `science-rt`'s
/// decision and its own doc comment's reason; what it buys *here* is that the
/// `Formatter` is an ordinary local this file can borrow twice.
const FORMATTER_INIT: &str = "science_formatter_init";
/// §2.1's spec, applied: a hole that writes one renders through a
/// `Formatter` built with it, and then through the one `Formatter` entry
/// point its type has. See [`Builder::render_with_spec`].
const FORMATTER_INIT_SPEC: &str = "science_formatter_init_spec";
const FORMATTER_TEXT: &str = "science_formatter_text";
const FORMATTER_NUMBER: &str = "science_formatter_number";
const FORMATTER_NUMBER32: &str = "science_formatter_number32";
const FORMATTER_INTEGER: &str = "science_formatter_integer";
const FORMATTER_UNSIGNED: &str = "science_formatter_unsigned";

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

/// A hole's spec as `science_formatter_init_spec`'s four scalars: fill,
/// flags, width, precision. `science-rt`'s `spec_flags` is the other half of
/// this table, and each field is its runtime tag plus one, zero for absent.
///
/// **The `0` flag becomes a fill**, because §3.1's `FormatSpec` has no field
/// for it: `fill: '0'` with the alignment left as written. When no alignment
/// was written either, that combination is one §2.1's grammar cannot produce
/// any other way, and `science-rt` reads it as the sign-aware flag — `{-7:04}`
/// is `-007`, where `{-7:0>4}` is `00-7`.
///
/// An absent width or precision is `-1`, which a `Constant::Count` spells as
/// `u64::MAX`: the backend materialises a count at the parameter's own width,
/// and at `i64` those are the same bits.
fn spec_operands(spec: &science_lexer::FormatSpec) -> Vec<Operand> {
    use science_lexer::format_spec::{Align, Grouping, Sign};
    let fill = match (spec.fill, spec.zero) {
        (Some(fill), _) => fill,
        (None, true) => '0',
        (None, false) => ' ',
    };
    let align = match spec.align {
        None => 0,
        Some(Align::Left) => 1,
        Some(Align::Right) => 2,
        Some(Align::Center) => 3,
    };
    let sign = match spec.sign {
        None => 0,
        Some(Sign::Plus) => 1,
        Some(Sign::Minus) => 2,
        Some(Sign::Space) => 3,
    };
    let code = spec.code.map_or(0, |code| u64::from(code.index()) + 1);
    let grouping = match spec.grouping {
        None => 0,
        Some(Grouping::Comma) => 1,
        Some(Grouping::Underscore) => 2,
    };
    let flags = align | sign << 2 | code << 4 | u64::from(spec.alternate) << 8 | grouping << 9;
    let number = |n: Option<u32>| n.map_or(u64::MAX, u64::from);
    [u64::from(u32::from(fill)), flags, number(spec.width), number(spec.precision)]
        .into_iter()
        .map(|n| Operand::Const(Constant::Count(n)))
        .collect()
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

/// Lowers every checked body, in the order the checker produced them, **plus**
/// every capture-free closure body found lowering them.
///
/// The order is the checker's and not this crate's, deliberately: §10 item 6
/// wants point identity stable under an edit elsewhere, and a numbering that
/// depended on how many bodies came first would not be. A closure's body is
/// appended right after the body it was found in — never interleaved with a
/// *later* checked body — so that property extends to it: editing a function
/// after the one a closure lives in still leaves the closure's own points
/// alone.
pub fn lower_crate(context: &mut Context<'_>, bodies: &[thir::Body]) -> Vec<Body> {
    let mut out = Vec::with_capacity(bodies.len());
    for thir in bodies {
        let (body, closures) = lower_body_and_closures(context, thir);
        out.push(body);
        out.extend(closures);
    }
    out
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
///
/// **A capture-free closure lowered from inside this body is dropped on the
/// floor here.** Every caller with a use for one — [`lower_crate`], and
/// `tests/captures.rs` where it matters — reaches
/// [`lower_body_and_closures`] instead. This wrapper survives because it is
/// the entry point every existing test and every other crate already calls,
/// and a body's own shape does not change: adding a second return value would
/// have moved every one of those call sites for a fact only some of them need.
pub fn lower_body(context: &mut Context<'_>, thir: &thir::Body) -> Body {
    lower_body_and_closures(context, thir).0
}

/// [`lower_body`], plus every capture-free closure lowered along the way.
///
/// §8.5's follow-up, kept out of `lower_body` itself: see that function's own
/// note for why the two are separate entry points rather than one changed
/// signature.
pub fn lower_body_and_closures(context: &mut Context<'_>, thir: &thir::Body) -> (Body, Vec<Body>) {
    let flag_ty = context
        .decls
        .prelude()
        .ty(context.types, "Bool")
        .unwrap_or(Ty::ERROR);
    let mut builder = Builder::new(context, thir);
    builder.run();
    let raw_closures = std::mem::take(&mut builder.closures);
    let mut body = builder.finish();
    crate::drops::elaborate(&mut body, flag_ty, context.decls, context.types, context.aliases);
    index_borrows(&mut body);
    body.predecessors = predecessors_of(&body.blocks);

    let mut closures = Vec::with_capacity(raw_closures.len());
    for mut closure_body in raw_closures {
        // The same five arguments the enclosing body's elaboration takes.
        // A closure's body owns and drops exactly as any other does, so
        // per-field elaboration has to reach it too — a closure that
        // partially moves a record would otherwise leak the fields it did
        // not move, which is the defect `moves.rs` §3 names.
        crate::drops::elaborate(
            &mut closure_body,
            flag_ty,
            context.decls,
            context.types,
            context.aliases,
        );
        index_borrows(&mut closure_body);
        closure_body.predecessors = predecessors_of(&closure_body.blocks);
        closures.push(closure_body);
    }
    (body, closures)
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
    /// Where the loop's test lives, and where a body that falls off the end
    /// goes back to.
    head: BlockId,
    /// Where `continue` goes, which is **not always the head**.
    ///
    /// A `loop:` has nothing between the end of its body and its test, so the
    /// two are the same block. A `for` has an increment: the cursor of
    /// `lower_for_over_range`, the index of `lower_for_over_array`. That
    /// increment sits after the body, so a `continue` that jumped to the head
    /// would skip it and the loop would never advance — `for i in 0..5:` with
    /// a `continue` in it hung forever, and both `for` forms did.
    ///
    /// So a `for` puts its increment in a block of its own and points this at
    /// it. The body's fall-through and every `continue` both reach it, which
    /// is the property that was missing: there was only ever one way out of
    /// the body and the increment was on it.
    continue_to: BlockId,
    /// Where `break` goes.
    exit: BlockId,
    /// How deep the scope stack was when the loop was entered: a `break` leaves
    /// every scope above this.
    depth: usize,
}

/// One capture, as the closure's own body will receive it. §8.6.
///
/// [`Builder::lower_captures`] produces these beside the operands the
/// [`Rvalue::Closure`] aggregate holds, so that the two lists are the same list
/// in the same order: entry *i* of the aggregate is entry *i* of the body's
/// trailing parameters, and the environment the backend builds out of one is
/// read back by the other without a second decision about ordering.
struct CaptureParam {
    /// The binding the closure's body names.
    def: DefId,
    /// The parameter's own type: the reference §8's discipline takes.
    reference_ty: Ty,
    /// What that reference points at, which is the type the body's THIR gives
    /// the name and so the type of the [`Projection::Deref`] between them.
    referent_ty: Ty,
    span: Span,
    /// Whether the body *moves* the captured value out — §8.2's named hole,
    /// which §8.6 turns from an unreported unsoundness into a refusal.
    moves: bool,
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
    /// A closure body lowered while building *this* body, keyed on the
    /// closure's own `param` (§8.5's follow-up: see [`Builder::run_closure`]).
    ///
    /// A closure's body shares this body's THIR arena — [`ExprKind::Closure`]'s
    /// `body` is an [`ExprId`] into the very [`thir::Body`] this `Builder` is
    /// already walking — so lowering one costs a second, short-lived `Builder`
    /// over the same `context` and `thir` rather than a second walk of the
    /// crate. Collected here rather than returned from `expr_into` because
    /// nothing on that call chain has a `Vec<Body>` to thread back, and a
    /// closure can be lowered from inside another closure's body (a nested
    /// `giving`) before either of theirs is known to be worth keeping.
    closures: Vec<Body>,
    /// [`mir::Body::closure_captures`], while the body is still being built.
    closure_captures: Option<usize>,
    /// Every capture this body is a closure *for*, and the type of the place
    /// each one borrows. §8.6.
    ///
    /// A capture reaches the closure's body as a parameter holding a
    /// *reference*, and the body names it at the referent's type — `x giving
    /// x + n` types `n` as `Int` while the parameter is a `borrowed Int`. This
    /// table is what [`Builder::as_place`] consults to put the
    /// [`Projection::Deref`] between the two; the value is the referent's type,
    /// which is the projection's own.
    captured: HashMap<DefId, Ty>,
    /// The crate's `Box`, found once by [`Coercions::of`] the way
    /// `science-types`' own `assign.rs` finds it — `None` for a table built by
    /// hand with no prelude. [`Builder::box_deref`] is the one reader:
    /// Decision 28's transparency reaches MIR as one more layer to peel off a
    /// method call's receiver, and this is the id that says which
    /// [`TyKind::Named`] that layer is.
    box_type: Option<DefId>,
    /// The crate's `Copy`, found once by [`Coercions::of`] beside
    /// [`Builder::box_type`] and for the same reason: §6.1 rule 2's exception
    /// is a *named interface*, so the operand rule has to be able to ask
    /// whether a type declares it. `None` for a table built by hand with no
    /// prelude, which is the admission §5.1 is happy with — the doubt then goes
    /// §5's way, as it did everywhere before this field existed.
    ///
    /// **It is `Coercions`' id and not the [`science_types::items::Prelude`]'s
    /// because `Copy` is not one of the prelude's `WANTED` names.** `Drop` is,
    /// and `Copy` is the interface with the same standing and no entry.
    /// `science-types`' `assign.rs` §7 needed it first and found it this way;
    /// this is the second reader, so the two agree on which `Copy` they mean by
    /// sharing the lookup rather than by adding a name to a list in a third
    /// crate.
    copy_interface: Option<DefId>,
}

impl<'a, 'ctx> Builder<'a, 'ctx> {
    fn new(context: &'a mut Context<'ctx>, thir: &'a thir::Body) -> Builder<'a, 'ctx> {
        let span = thir.block(thir.root()).span;
        let bool_ty = context.decls.prelude().ty(context.types, "Bool").unwrap_or(Ty::ERROR);
        let coercions = Coercions::of(context.defs);
        let box_type = coercions.box_type();
        let copy_interface = coercions.copy_interface();
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
            closures: Vec::new(),
            closure_captures: None,
            captured: HashMap::new(),
            box_type,
            copy_interface,
        }
    }

    fn finish(self) -> Body {
        let def = self.thir.def();
        self.finish_with(def)
    }

    /// [`Builder::finish`], for a body with no [`thir::Body::def`] of its
    /// own — a closure, identified by its `param` (§8.5's follow-up).
    fn finish_with(self, def: DefId) -> Body {
        Body {
            def,
            locals: self.locals,
            blocks: self.blocks,
            borrows: self.borrows,
            arg_count: self.arg_count,
            closure_captures: self.closure_captures,
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

    /// [`Builder::run`], for a closure with nothing captured.
    ///
    /// **The one difference from an ordinary body is where the parameter comes
    /// from.** `run` reads a signature out of [`science_types::items::Declarations`];
    /// a closure has none — it is not a declared item — so its one parameter is
    /// `param` itself, and its type is read the same way every other binding's
    /// is, off [`thir::Body::local_ty`]. Everything after that is `run`
    /// unchanged: one scope holding the parameter, one expression lowered into
    /// `_0`, one `Return`.
    ///
    /// **The body it lowers is an [`ExprId`], not a [`thir::BlockId`].** A
    /// `giving` closure's body is a single expression — `collections-and-chains.md`
    /// §1.2's arrow type takes it as one — so this calls [`Builder::expr_into`]
    /// directly rather than [`Builder::lower_block`], which is the entry point
    /// every function body uses because a `def`'s is a block.
    fn run_closure(
        &mut self,
        params: &[DefId],
        body: ExprId,
        ret_ty: Ty,
        captures: &[CaptureParam],
    ) {
        let span = self.thir.expr(body).span;
        self.span = span;
        self.push_local(ret_ty, LocalKind::Return, span);
        let mut param_locals = Vec::with_capacity(params.len() + captures.len());
        for param in params {
            let param_ty = self.thir.local_ty(*param).unwrap_or(Ty::ERROR);
            let param_span = self.context.defs.get(*param).span;
            let local = self.push_local(param_ty, LocalKind::Param(*param), param_span);
            self.bindings.insert(*param, local);
            param_locals.push(local);
        }
        // §8.6. Each capture is one more parameter, holding the reference the
        // enclosing body took, in [`crate::capture`]'s first-mention order —
        // the same order [`Builder::lower_captures`] wrote the aggregate in, so
        // that the two halves of one calling convention are one list read
        // twice.
        for capture in captures {
            let local = self.push_local(
                capture.reference_ty,
                LocalKind::Param(capture.def),
                capture.span,
            );
            self.bindings.insert(capture.def, local);
            self.captured.insert(capture.def, capture.referent_ty);
            param_locals.push(local);
        }
        self.arg_count = params.len() + captures.len();
        self.closure_captures = Some(captures.len());

        let entry = self.new_block();
        debug_assert_eq!(entry, ENTRY_BLOCK);
        self.scopes.push(Scope { locals: param_locals });

        let block = self.expr_into(Place::local(RETURN_PLACE), body, entry);
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
        let block = self.emit_drop_if_needed(local, block, span);
        self.push_statement(block, StatementKind::StorageDead(local), span);
        block
    }

    /// A `Drop` of `local`, if its type owns anything — with no
    /// `StorageDead`, unlike [`Builder::emit_scope_exit`]. The two callers
    /// want different halves of the same question: a scope's own locals die
    /// with the scope, storage and all, but [`Builder::lower_loop`]'s
    /// discarded tail keeps its storage live across the back edge and only
    /// needs the *value* released each time the tail overwrites it.
    fn emit_drop_if_needed(&mut self, local: Local, block: BlockId, span: Span) -> BlockId {
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
        block
    }

    // --- blocks and statements --------------------------------------------

    /// A block's statements, then its tail into `dest`.
    ///
    /// # A discarded tail gets a slot of its own type
    ///
    /// **A tail whose value the enclosing construct throws away is lowered
    /// into a temporary typed like the tail, and `dest` is assigned unit.**
    /// The tail is written into `dest` directly only when `dest` can hold it.
    ///
    /// The reason is that `dest`'s type is the *enclosing construct's*, not
    /// the tail's, and the two part company exactly where a value is
    /// discarded: `check`'s §6 types an `if` with no `else`, a `loop` and a
    /// `for` as `Ty::UNIT` however their bodies end, and says nothing about
    /// them to the body. (A `()`-returning *function* body is the case that
    /// never reaches here: the checker refuses a non-unit tail under it with
    /// `SC0525`, *"expected `()`, found `I64?`"*, so the disagreement is a
    /// diagnostic long before MIR. The three above are the ones where
    /// discarding is the language's own rule rather than a mistake.) So
    /// `if ready: m.insert(k, v)` used to hand
    /// [`Builder::expr_into`] a `()` place for a call that produces a `V?`,
    /// and the disagreement travelled all the way to codegen, where §5.3's
    /// bool-plus-out-parameter convention has nowhere to write the payload
    /// and refuses — while the same call one line earlier, as a
    /// `StmtKind::Expr`, was fine, because [`Builder::lower_stmt`] already
    /// builds its discard out of the *expression's* own type.
    ///
    /// This is that same rule, moved to the other place a value is dropped on
    /// the floor. It also puts the discarded value somewhere a destructor can
    /// find it: the temporary belongs to this block's scope, so
    /// [`Builder::pop_scope`] drops it, and the displaced `V` a
    /// `Map[K, String].insert` hands back is released rather than leaked.
    fn lower_block(&mut self, dest: Place, block_id: thir::BlockId, mut block: BlockId) -> BlockId {
        let thir = self.thir;
        let thir_block = thir.block(block_id);
        self.push_scope();
        for statement in &thir_block.stmts {
            block = self.lower_stmt(statement, block);
        }
        match thir_block.tail {
            Some(tail) if self.tail_is_discarded(&dest, tail) => {
                let ty = self.thir.ty(tail);
                let span = self.thir.expr(tail).span;
                let discard = self.temp(ty, span, block);
                block = self.expr_into(Place::local(discard), tail, block);
                self.assign(
                    block,
                    dest,
                    Rvalue::Use(Operand::Const(Constant::Unit)),
                    thir_block.span,
                );
            }
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

    /// Whether the value `tail` produces has nowhere to go in `dest`.
    ///
    /// **The one question asked is whether the destination is `()` and the
    /// tail is not.** Both types are revealed through their aliases first, so
    /// a `type Nothing be ()` on either side answers the same as the spelling
    /// it stands for.
    ///
    /// Any *other* disagreement between the two is left alone and handed to
    /// [`Builder::expr_into`] as before: a `T` tail under a `T?` destination
    /// is `check`'s widening and not a discard, and inventing a slot for it
    /// here would put a `T` where a `T?` belongs. `Ty::ERROR` is excluded for
    /// the same reason [`Builder::temp`] is never given one deliberately —
    /// the mistake is already reported, and a temporary of the error type
    /// would ask the layout engine a question with no answer.
    fn tail_is_discarded(&mut self, dest: &Place, tail: ExprId) -> bool {
        let dest_ty = self.place_ty(dest);
        if self.revealed(dest_ty) != Ty::UNIT {
            return false;
        }
        let tail_ty = self.thir.ty(tail);
        let tail_ty = self.revealed(tail_ty);
        tail_ty != Ty::UNIT && tail_ty != Ty::ERROR
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
                // **The value is evaluated before an indexed target is taken.**
                // `v[0] be v[2]` reads `v` on the right of a write into `v`;
                // with the target's place built first, the read happens while
                // the element's exclusive access is open, and the region
                // check refuses it (`SC0330`). Evaluating the right-hand side
                // into a temporary first, as Rust's assignment does, ends the
                // read before the write begins. Only an indexed target needs
                // it, so every other assignment keeps its order.
                let mut early = None;
                if self.target_is_indexed(*target) {
                    let vty = self.thir.ty(*value);
                    let temp = self.temp(vty, span, block);
                    block = self.expr_into(Place::local(temp), *value, block);
                    early = Some((temp, vty));
                }
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
                        // `a[i, j] be v` on a user type is a call to
                        // `index_mutably` whose `&mut` result the assignment
                        // writes through (`science-types`' `index_expr`), so
                        // here the call has to run: its reference lands in the
                        // temporary and `assign_target` dereferences it.
                        let block = if matches!(
                            self.thir.expr(*target).kind,
                            ExprKind::MethodCall { .. }
                        ) {
                            self.expr_into(Place::local(temp), *target, block)
                        } else {
                            block
                        };
                        (Place::local(temp), block)
                    }
                };
                // `deref_to_hole` is the narrow's half of the same rule, and
                // this is its fourth caller for its fourth symptom: `p be 999`
                // where `p` is a `(mutable borrowed Int)?` narrowed by `if p?:`
                // stored the `Int` into the pointer-or-null slot itself.
                // `as_place` sees through the `ExprKind::Narrow` wrapping
                // `target` and hands back that slot's own place, while
                // `target`'s type is the narrowed `mutable borrowed Int`.
                // Left alone, `assign_target` below reads the place's
                // *written* type as `Nullable`, never matches
                // `TyKind::Borrowed`, and returns it untouched — legal IR
                // (a `ptr` will hold any bit pattern) and wrong, which is why
                // the linker's opaque-pointer check is what caught it rather
                // than the type checker or a verifier.
                //
                // `auto_deref` is not run first, unlike those three callers.
                // It peels every literal `Borrowed` layer, and `assign_target`
                // below must see the first one itself to decide whether to
                // stop there (`r be borrowed y`, where `r` is already a
                // reference, stops without dereferencing). Calling
                // `deref_to_hole` unconditionally first is still safe for
                // that case and every other non-narrowed target: its own
                // guard requires the place's *written* type to still be
                // `Nullable`, which a literal borrow's is not, so it returns
                // such a place unchanged.
                //
                // **Only through an exclusive borrow, which is the checker's
                // own line.** `science-types`' `StmtKind::Assign` arm writes
                // *through* a narrowed `mutable borrowed T` and *rebinds* a
                // narrowed shared one — `largest`'s `best be item`, where a
                // shared borrow cannot be written through at all, so the only
                // thing it can mean is *this local now points at `item`*. This
                // used to project the `Deref` for both, so `best be item`
                // stored `item`'s pointer *into* the `Int` `best` pointed at:
                // `local _17 is i64 and the value stored into it is ptr`, at
                // every generic `largest[T: Ord]` instantiated at `Int`. A
                // rebinding keeps the nullable's own slot, and a `(&T)?` is
                // its niched pointer, so the pointer is stored where it
                // belongs.
                let hole_ty = self.revealed(self.thir.expr(*target).ty);
                let exclusive = matches!(
                    self.context.types.kind(hole_ty),
                    TyKind::Borrowed { mutable: true, .. }
                );
                let place = if exclusive { self.deref_to_hole(place, *target) } else { place };
                // §4.7's *"borrows auto-dereference for assignment"*, which is
                // §4's rule on the other side of `be`. See
                // [`Builder::assign_target`].
                let place = self.assign_target(place, self.thir.ty(*value));
                // **Drop and replace.** An assignment overwrites a value, and
                // a value that owns something has to be released first or it
                // leaks: `name be make()` in a loop leaked one `String` per
                // iteration, because drops were emitted at scope exits and
                // nowhere else. The new value is built into a temporary
                // *before* the old one is dropped — `name be name.clone()`
                // reads the value it replaces — and then moved in. The `Drop`
                // is elaborated like any other: `crate::drops` deletes it on a
                // path where the old value was already moved out, flags it
                // where it may have been, and leaves it standing where it is
                // certainly there.
                let ty = self.place_ty(&place);
                if let Some((temp, vty)) = early {
                    let replace = self.new_block();
                    if crate::moves::needs_drop(
                        self.context.decls,
                        self.context.types,
                        self.context.aliases,
                        ty,
                    ) {
                        self.terminate(
                            next,
                            TerminatorKind::Drop { place: place.clone(), flag: None, target: replace },
                            span,
                        );
                    } else {
                        self.terminate(next, TerminatorKind::Goto { target: replace }, span);
                    }
                    let read = self.read(Place::local(temp), vty);
                    self.assign(replace, place, Rvalue::Use(read), span);
                    block = replace;
                } else if crate::moves::needs_drop(
                    self.context.decls,
                    self.context.types,
                    self.context.aliases,
                    ty,
                ) {
                    let temp = self.temp(ty, span, next);
                    let evaluated = self.expr_into(Place::local(temp), *value, next);
                    let replace = self.new_block();
                    self.terminate(
                        evaluated,
                        TerminatorKind::Drop { place: place.clone(), flag: None, target: replace },
                        span,
                    );
                    let read = self.read(Place::local(temp), ty);
                    self.assign(replace, place, Rvalue::Use(read), span);
                    block = replace;
                } else {
                    block = self.expr_into(place, *value, next);
                }
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
                let (head, depth) = (target.continue_to, target.depth);
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
            | ExprKind::TupleField { .. }
            | ExprKind::Index { .. } => match self.as_place(expr, block) {
                Some((place, block)) => {
                    let (operand, block) = self.read_ergonomic(place, ty, block, span);
                    self.assign(block, dest, Rvalue::Use(operand), span);
                    block
                }
                None => {
                    self.assign(block, dest, Rvalue::Error, span);
                    block
                }
            },
            ExprKind::Item(def) => {
                // A `const` is a value known at compile time, so a reference
                // to one substitutes that value rather than naming the
                // `const` the way a call names a function. `Constant::Item`
                // is a def *no backend can build a value from* — it exists
                // for a function or a unit variant named as a value, and
                // every backend refuses it for anything else — so a `const`
                // that reached here as one would fail two phases later with a
                // message about "a value", for a program that named none.
                //
                // `science_types::items::Declarations::const_value` is
                // `crate::science-types`'s `constant` module's answer to
                // "what does this `const` denote", computed once when
                // declarations are lowered and not re-derived per use, which
                // is `items.rs`'s own §1 argument applied to a fourth table.
                // It is `None` for every def that is not a `const` — a
                // function or a variant included — so this check costs
                // nothing on the paths [`DefKind::Variant`] below already
                // owns; it is also `None` for a `const` whose initialiser
                // `constant::lower` could not evaluate to a literal or could
                // not confirm against the `const`'s type, and for both this
                // arm falls back to the pre-existing [`Constant::Item`],
                // exactly as if this substitution did not exist.
                let rvalue = if self.context.defs.get(*def).kind == DefKind::Variant {
                    Rvalue::Variant { variant: *def, payload: Vec::new() }
                } else if let Some(literal) = self.context.decls.const_value(*def) {
                    Rvalue::Use(Operand::Const(Constant::Literal(literal.clone())))
                } else {
                    Rvalue::Use(Operand::Const(Constant::Item(*def)))
                };
                self.assign(block, dest, rvalue, span);
                block
            }
            // Decision 10's `[1, 2, 3]`. **The same shape as `lower_fstring`
            // one construct over**, and for the same reason: §4's *"in F0, a
            // whole-array operation lowers to a call to a runtime or library
            // entry point, never to an inlined MIR loop"* — so this is n + 1
            // straight-line calls and no back edge, which is what
            // `tests/no_invented_loops.rs` holds this crate to.
            //
            // **The accumulator is borrowed once per push, exclusively, and
            // the borrow is §6's two-phase kind** — `lower_fstring`'s §1 is
            // the argument and it transfers unchanged: one borrow held across
            // the whole sequence would be an exclusive loan live over the
            // evaluation of every element, and an element is an arbitrary
            // expression.
            //
            // **Each element is borrowed *shared* and not moved**, because
            // `science_array_push` takes a pointer and copies `size` bytes out
            // of it. What owns the element afterwards is the array; the
            // element's own slot is dead after the push, which is
            // `crate::moves`' question and not this one.
            ExprKind::Array(elements) => {
                let count = elements.len() as u64;
                let mut block = self.emit_call(
                    dest.clone(),
                    Callee::Runtime(ARRAY_WITH_CAPACITY),
                    vec![Operand::Const(Constant::Count(count))],
                    block,
                    span,
                );
                if elements.is_empty() {
                    return block;
                }
                // One discard slot for the whole sequence, for
                // `lower_fstring`'s reason: every push returns nothing and a
                // `TerminatorKind::Call` has a destination whether or not
                // there is a value for it.
                let discard = Place::local(self.temp(Ty::UNIT, span, block));
                for element in elements {
                    let (accumulator, next) = self.accumulator_ref(&dest, block, span);
                    block = next;
                    let (value, next) = self.element_move(*element, block, span);
                    block = next;
                    block = self.emit_call(
                        discard.clone(),
                        Callee::Runtime(ARRAY_PUSH),
                        vec![accumulator, value],
                        block,
                        span,
                    );
                }
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
                let operand_ty = self.thir.ty(*lhs);
                let (left, block) = self.operand(*lhs, block);
                let (right, block) = self.operand(*rhs, block);
                // §4.6's `/`, `%`, `<<` and `>>`: the only operators in the
                // language with an input they are not defined on.
                // `division_check`'s own note is the shape; `shift_check`'s is
                // why a shift needs it too and what the guard tests.
                let (left, right, block) = match op {
                    BinaryOp::Div | BinaryOp::Rem => {
                        self.division_check(left, right, operand_ty, block, span)
                    }
                    BinaryOp::Shl | BinaryOp::Shr => {
                        self.shift_check(left, right, operand_ty, block, span)
                    }
                    // §5's exception, the same one `ExprKind::Present` above
                    // already takes. `Eq`/`Ord`'s six operators (`check.rs`'s
                    // `binary` groups all six behind one dispatch) each read
                    // both operands and produce a fresh `Bool`; neither
                    // operand is consumed, so `x is y` on two `String`s must
                    // not be the whole-value `Move` `self.operand`'s `read`
                    // hands back for anything `is_copy` refuses. `read` has no
                    // way to know the *operator* rather than the *type* is
                    // what decides this here, so the decision is corrected
                    // after the fact with `force_copy` rather than taught to
                    // `read` itself, which every other caller still needs to
                    // answer off the type alone (an assignment, a call that
                    // takes ownership, or a `return` really do consume).
                    // Leaving drop elaboration a live `Drop` at the binding's
                    // scope exit — rather than deleting it, the way a real
                    // `Move` would — is exactly what keeps a `String` compared
                    // in a loop freed once per iteration and not zero times.
                    BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le
                    | BinaryOp::Ge => (force_copy(left), force_copy(right), block),
                    // `stdlib-core.md` §6.3's `String + String` reads both
                    // operands and builds a fresh buffer, so neither is
                    // consumed: the bindings keep their `Drop`s. On a number
                    // every operand is `Copy` already and this is the identity.
                    BinaryOp::Add => (force_copy(left), force_copy(right), block),
                    _ => (left, right, block),
                };
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
            ExprKind::Present(operand) if self.borrowed_nullable_payload(*operand).is_some() => {
                // `item?` on a `&T?`: ask the nullable behind the pointer.
                let (place, block) = match self.as_place(*operand, block) {
                    Some(found) => found,
                    None => {
                        let operand_ty = self.thir.ty(*operand);
                        let temp = self.temp(operand_ty, span, block);
                        let block = self.expr_into(Place::local(temp), *operand, block);
                        (Place::local(temp), block)
                    }
                };
                let place = self.auto_deref(place);
                self.assign(block, dest, Rvalue::IsPresent(Operand::Copy(place)), span);
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
                self.lower_borrow(dest, *mutable, *operand, ty, block, span, false)
            }
            ExprKind::Range { start, end, inclusive } => {
                let (low, block) = self.operand(*start, block);
                let (high, block) = self.operand(*end, block);
                let rvalue = Rvalue::Range { start: low, end: high, inclusive: *inclusive };
                self.assign(block, dest, rvalue, span);
                block
            }
            ExprKind::Coerce { operand, coercion: Coercion::BorrowThroughBox } => {
                self.borrow_through_box(dest, *operand, block, span, false)
            }
            ExprKind::Coerce { operand, coercion: Coercion::Reborrow } => {
                self.shared_reborrow(dest, *operand, block, span, false)
            }
            ExprKind::Coerce { operand, coercion } => {
                let coercion = *coercion;
                let (value, block) = self.operand(*operand, block);
                // **`Coercion::Copy` is a load through the borrow, so the
                // borrow is read and not consumed.** `assign.rs` §7's own
                // sentence: *"this one is a load"*. A *shared* borrow already
                // comes back from `operand` as a copy, because `is_copy` says
                // so of `&T`; an *exclusive* one does not, because duplicating
                // a `&mut` is aliasing it — which is true of the pointer and
                // beside the point here, where nothing keeps the duplicate.
                // Left a `Move`, `counter be counter + 1` on a
                // `counter: &mut Int` (`examples/01_functions.science`'s
                // `bump`) read as moving `counter` and then writing through
                // it, and `science-regions`' `moved` reported `SC0301` against
                // a line that moves nothing. §5's exception, the same
                // `force_copy` the comparison operators take.
                let value = match coercion {
                    Coercion::Copy => force_copy(value),
                    _ => value,
                };
                self.assign(block, dest, Rvalue::Coerce { operand: value, coercion, ty }, span);
                block
            }
            ExprKind::Narrow(operand) => {
                if self.borrowed_nullable_payload(*operand).is_some() {
                    let (place, block) = self.as_place(expr, block).expect("a narrowed borrow has a place");
                    return self.borrow_place(dest, false, place, block, span, false);
                }
                let (value, block) = self.operand(*operand, block);
                self.assign(block, dest, Rvalue::Narrow { operand: value, ty }, span);
                block
            }
            // §8. The captures are lowered, and — new — a capture-free
            // closure's body now is too.
            ExprKind::Closure { param, rest, body } => {
                let (param, body) = (*param, *body);
                let all_params: Vec<DefId> =
                    std::iter::once(param).chain(rest.iter().copied()).collect();
                let (captures, capture_params, block) =
                    self.lower_captures(&all_params, body, block);
                // §8.5's follow-up, and §8.6's: a closure gets the [`Body`]
                // every other definition gets — keyed on `param`, since a
                // closure has no `DefId` of its own to be one of (§8.5's own
                // sentence) and `param` is already unique per closure
                // (`science-resolve`'s `resolve_closure` mints one every
                // time) — and its captures reach that body as the trailing
                // parameters §8.6 describes.
                //
                // **Two closures are still left without one**, and the reason
                // is different in each case. A *generic* closure is left
                // because a `TyKind::Param` anywhere in `ty` means some
                // instantiation this walk cannot see is still owed a copy
                // (Decision 42's order: types before mono, and mono is two
                // crates down). A closure that **moves** a capture out is
                // left because §8.2 has no spelling for the move — the
                // discipline models it as an exclusive borrow, and a body
                // lowered against that model would memcpy an owning value out
                // of storage the enclosing frame still drops. §8.2 called that
                // *"one hole and it is worth stating exactly"* while no body
                // was lowered at all; lowering one would turn it from an
                // unreported mistake into a double free, so the body is
                // withheld and `science-codegen-llvm` refuses the closure by
                // name instead.
                let closure_ret = match self.context.types.kind(ty) {
                    TyKind::Closure { ret, .. } => Some(*ret),
                    _ => None,
                };
                let moves_a_capture = capture_params.iter().any(|capture| capture.moves);
                if !moves_a_capture && !ty_mentions_param(self.context.types, ty) {
                    if let Some(ret) = closure_ret {
                        let mut nested = Builder::new(self.context, self.thir);
                        nested.run_closure(&all_params, body, ret, &capture_params);
                        let mut nested_closures = std::mem::take(&mut nested.closures);
                        self.closures.push(nested.finish_with(param));
                        self.closures.append(&mut nested_closures);
                    }
                }
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
            ExprKind::For { pattern, iter, body, next } => {
                self.lower_for(dest, *pattern, *iter, *body, *next, block, span)
            }
            ExprKind::Match { scrutinee, arms } => {
                self.lower_match(dest, *scrutinee, arms, block, span)
            }
            ExprKind::Call { callee, args, self_ty } => {
                self.lower_call(dest, *callee, args, *self_ty, block, span)
            }
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
        // **`Ty::UNIT`, because a `loop:` *is* `Ty::UNIT` (`check`'s §6) and
        // nobody reads what the body produces.**
        //
        // This used to be typed from the body's *tail* instead — a `T?` a
        // `science-rt` call builds out of a `bool` and an out-parameter (§5.3)
        // has nowhere else to write its answer, and a `()` destination reached
        // codegen as a call whose destination disagreed with its own callee.
        // That was the right diagnosis and the wrong place: the same
        // disagreement arrives through an `if` with no `else` and through a
        // `for`, neither of which has a `tail_ty` line to copy, so
        // [`Builder::lower_block`] now answers it once for every construct
        // that discards a tail. A second answer here would only decide which
        // of the two got there first.
        //
        // Still allocated *outside* the loop, so its `StorageLive` runs once
        // rather than once per iteration — a temporary whose storage began
        // inside the loop and outlived it would pair a `StorageDead` with no
        // `StorageLive` on the zero-more-writes path. The tail's own value now
        // lives in a temporary of `lower_block`'s making, which belongs to the
        // body's scope and is therefore dropped on every trip round the back
        // edge rather than surviving into the next one.
        let discard = self.temp(Ty::UNIT, span, block);
        let head = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);
        self.loops.push(LoopScope { head, continue_to: head, exit, depth: self.scopes.len() });
        let after = self.lower_block(Place::local(discard), body, head);
        // **Dropped every iteration the tail is reached, not just once at the
        // enclosing scope's exit**, because the discard's storage is shared
        // across every trip around the back edge and overwriting it on
        // iteration `n + 1` without first releasing iteration `n`'s answer
        // would leak whatever it owned. `Ty::UNIT` owns nothing, so today this
        // emits no drop at all and the leak it guards against is
        // `lower_block`'s to prevent — `Array[String].pop()` discarded in a hot
        // loop is released there, in the body's own scope, once per trip. The
        // call stays because the *slot* is still loop-carried: anything that
        // ever gives this local an owning type again needs this line, and
        // `crate::moves`' elaboration deletes it for free while it does not.
        // `break`/`continue` skip this block entirely, by construction (§5's
        // terminators leave from wherever they are written, never falling
        // through to a block's own tail), so a value is only ever here to drop
        // on the path that just wrote one.
        let after = self.emit_drop_if_needed(discard, after, span);
        self.terminate(after, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();
        // A `loop` is `Ty::UNIT` (`check`'s §6), so the exit stores unit and
        // `break e` stores nothing.
        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    /// §7. The shape of a `for`: one shared borrow of its subject, and a
    /// named hole where the `next()` that reads through it goes.
    #[allow(clippy::too_many_arguments)]
    fn lower_for(
        &mut self,
        dest: Place,
        pattern: PatId,
        iter: ExprId,
        body: thir::BlockId,
        next_method: Option<DefId>,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        // **`for i in a..b:` is the counting loop it names, and the `Range`
        // is never built.**
        //
        // The decision. A `for` whose subject is written as a range lowers to
        // a cursor, a comparison and a back edge. No `Rvalue::Range` is
        // emitted and no `Range[T]` value exists at run time.
        //
        // The reason. `collections-and-chains.md`'s AMENDMENT 14 says `Range`
        // implements `Iterate` *"directly, which is what makes `for i in 0..n:`
        // the same construct as everything else rather than a special case in
        // the parser"* — and it says so on the ground that a range **is not a
        // container**: it holds two ends and *computes* each element. So there
        // is nothing to iterate over, only arithmetic, and the honest lowering
        // of the construct the note describes is the arithmetic.
        //
        // §4.1 makes the difference from the array case explicit: a `Range`'s
        // `Item` is `T` and not `&T`, because there is no element in memory
        // for a borrow to point at. So the pattern binds a **value**, and the
        // loop takes no borrow of anything — which is also why §4.4's question
        // about the source does not arise.
        //
        // The cost. A range bound to a name first — `let r be 0..n` then
        // `for i in r:` — is not this shape and still needs `Range[T]` as a
        // value, which no backend lowers. That is one construct rather than
        // the whole family, and it is the one the notes call *"a value of
        // §2.6's runtime containers"*.
        if let thir::ExprKind::Range { start, end, inclusive } = self.thir.expr(iter).kind {
            return self
                .lower_for_over_range(dest, pattern, start, end, inclusive, body, block, span);
        }

        // **A chain is a `for`'s subject whole, and the loop is its terminal.**
        //
        // `for k in m.keys():` and `for x in xs.iterate().keep(…):` are the
        // chain a terminal would have fused, with the body in the place of
        // `collect`'s push: one loop, no adapter built, and `break` and
        // `continue` ending or advancing it. §2.1's laziness is what makes
        // that the same program — a chain does nothing until something pulls
        // it, and a `for` is something that pulls.
        if let Some(chain) = self.chain_of(iter) {
            let terminal = Terminal::Each { pattern, body };
            return self.lower_chain_terminal(dest, terminal, &chain, block, span);
        }

        // **AMENDMENT 11's desugaring, taken for the one subject that cannot
        // express it any other way.**
        //
        // The decision. `for x in xs:` over an `Array[T]` is lowered here as
        // the indexed loop `xs.iterate()` would compile to, rather than as a
        // call to a `next` that does not exist.
        //
        // The reason. `collections-and-chains.md` §4.2's AMENDMENT 11 says
        // *"`for x in xs:` desugars to `xs.iterate()`"*, and §4.1 gives the
        // three sources. §8's closed library names **no type for `iterate()`
        // to return**, and `Array[T] implements Iterate` — declared with
        // `next(mutable self)` and no cursor field — cannot advance anything,
        // whoever calls it. So the construct the notes specify has no
        // spelling, and the loop the notes describe has an exact lowering.
        //
        // What this is *not* is an invented loop.
        // `tests/no_invented_loops.rs` states the claim it guards — *"every
        // loop in a body's CFG comes from a `loop` or a `for` the author
        // wrote; this crate synthesises no loop"* — and one written `for`
        // produces one loop header here.
        //
        // **§4.4's shared borrow is the right one here and costs nothing.**
        // The contradiction that forces `for c in text.chars():` to borrow
        // exclusively — `Iterate.next` is `def next(mutable self)` — does not
        // arise, because no `next` is called: the cursor is a local of this
        // loop's own, and the array is only read.
        //
        // The cost, and the repair. `Array` is the one subject with a
        // lowering of its own, so a user type implementing `Iterate` goes
        // through the general path and a `for` over an `Array` does not. The
        // repair is the type §8 is missing — an iterator `Array[T].iterate()`
        // returns, holding the cursor `Array` has nowhere to keep, with
        // `Iterate` implemented on *it* rather than on the container. That is
        // a spec amendment, and until it lands this is what the amendment
        // would compile to.
        // `Map` and `Set`, for `Array`'s reason one container over — and asked
        // of the subject's *type* before anything is evaluated, so a subject
        // that is neither is evaluated once, by whichever path does take it.
        if self.iterates_in_insertion_order(self.thir.ty(iter)) {
            let (place, next) = self.loop_source(iter, block, span);
            return self.lower_for_over_map(dest, pattern, place, body, next, span);
        }
        if let Some((place, next)) = self.array_subject(iter, block, span) {
            return self.lower_for_over_array(dest, pattern, place, body, next, span);
        }

        // §7.1. The loop's own borrow. `borrow_source` is the same helper a
        // written `borrowed x` goes through, so a subject with no place — the
        // chain temporary of `for x in xs.iterate_consuming():` — lands in a
        // temporary with a storage-dead point rather than in a special case,
        // and `auto_deref` is §9's rule: the loan names the referent and never
        // the reference, so rule 5 compares it against the right storage.
        let (source, next) = self.loop_source(iter, block, span);
        block = next;
        let source_ty = self.place_ty(&source);
        // **Shared, on §4.4's authority, and that is now in open conflict with
        // the callee this loop calls.** The prelude declares `Iterate.next` as
        // `def next(mutable self)`, so a call through a shared reference is
        // precisely the mismatch §6 exists to catch — and §4.4 says in as many
        // words that a `for`'s source is *borrowed shared*, which is what
        // `tests/iteration.rs` pins on that citation. Both cannot be right.
        //
        // **It is left at shared, deliberately.** A spec section is not
        // overridden from inside a lowering to make one construct convenient,
        // and nothing observable turns on it today: every `implements Iterate`
        // block in `science-resolve`'s `builtins` declares `methods: &[]`, so
        // all three subjects resolve to the *interface's* bodiless `next` and
        // no `for` loop reaches an executable in any case.
        //
        // **The contradiction is deeper than a borrow kind and is written here
        // because this is where it surfaces.** `Array of T implements Iterate`
        // directly, with `next(mutable self)` and no cursor field — so there is
        // nowhere for the iteration state to live, and `next` on an array
        // cannot advance no matter which borrow it is called through.
        // `collections-and-chains.md`'s AMENDMENT 14 says `Range` implements
        // `Iterate` *"directly"* precisely because it is **not a container**,
        // which reads as containers being meant to hand out an iterator the way
        // `String.chars()` hands out a `Chars`. Whoever reconciles that either
        // gives `Array`'s `Iterate` a cursor or takes the implementation off
        // `Array` and puts it on an iterator type; until then this borrow's
        // kind follows the note that exists.
        // Exclusive exactly where the callee is real and the subject is the
        // loop's own temporary; shared everywhere else, which is §4.4's case.
        let exclusive = next_method.is_some_and(|def| self.owner_is_a_type(def))
            && self.as_place(iter, block).is_none();
        let iterator_ty = self.context.types.borrowed(exclusive, source_ty);
        let iterator = self.temp(iterator_ty, span, block);
        // `in_argument` is `false`: a loop's borrow is not an argument borrow.
        // It is created once, read once per turn, and lives across the whole
        // loop, which is the opposite of the single-use-at-one-call shape §6
        // reserves two phases for.
        block = self.borrow_place(Place::local(iterator), exclusive, source, block, span, false);

        // The element the loop produces, and the presence test over it. Both
        // are allocated outside the loop for `lower_loop`'s reason.
        //
        // **The slot is `Item?` and the binding is `Item`, which used to be one
        // type doing both jobs.** `next` returns `Self.Item?` — that is what
        // `Rvalue::IsPresent` below asks about — while the pattern binds the
        // `Item` inside it. While the callee was unresolved nothing checked the
        // destination against a signature and one slot went unnoticed; a real
        // call makes it visible immediately, because `Char?` is a discriminant
        // and a payload where `Char` is four bytes.
        let element_ty = self.thir.pat(pattern).ty;
        let slot_ty = self.context.types.nullable(element_ty);
        let element = self.temp(slot_ty, span, block);
        let present = self.temp(self.bool_ty, span, block);
        let discard = self.temp(Ty::UNIT, span, block);

        let head = self.new_block();
        let test = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        // §7.2, and the seam is closed. `thir::ExprKind::For` now carries the
        // `next` that `check`'s `iterate_item` resolves, so this is
        // `Callee::Def` for every subject that implements `Iterate` and
        // `Unresolved::IterateNext` only for the ones that do not — which is
        // the same set whose element type is `Ty::ERROR`, because both answers
        // come out of that one lookup. The argument is a `Copy` either way —
        // it is a reference — so §5's rule that every operand of an unresolved
        // call is a copy is met without a `force_copy`.
        // **The field is here and it is deliberately not used, which is a
        // result and not an omission.** Building `Callee::Def(next_method)` is
        // one line, it compiles, and it was measured: it unblocks **no**
        // program and breaks a correct one. Four things stand between the
        // resolved callee and a `for` loop that runs, and none of them is in
        // this crate:
        //
        // 1. **No `next` has a body.** All three `implements Iterate` blocks in
        //    `science-resolve`'s `builtins` declare `methods: &[]`, so every
        //    subject resolves to the *interface's* `next`, which is a default
        //    body with no implementation — the same monomorphisation gap that
        //    blocks `examples/06` and `08`. A resolved callee with no body and
        //    an unresolved one are equally un-runnable.
        //
        // 2. **`Array of T implements Iterate` has nowhere to keep a cursor.**
        //    `next(mutable self)` on an array would have to advance something,
        //    and an array is a buffer, a length and a capacity — so `next`
        //    cannot advance no matter what calls it.
        //    `collections-and-chains.md`'s AMENDMENT 14 says `Range`
        //    implements `Iterate` *"directly"* precisely because it is **not a
        //    container**, which reads as containers being meant to hand out an
        //    iterator the way `String.chars()` hands out a `Chars`. This is a
        //    specification question, not an implementation one.
        //
        // 3. **`next(mutable self)` contradicts §4.4**, which says a `for`'s
        //    source is borrowed *shared* and which `tests/iteration.rs` pins on
        //    that citation. §7.1's borrow above follows the note.
        //
        // 4. **`def next(mutable self) -> Self.Item?` relates no region.** The
        //    returned borrow is given no connection to `self`'s lifetime, so a
        //    function that returns an element of its own parameter —
        //    `examples/19_stdlib.science`'s `find` is exactly this — has a
        //    genuinely undetermined signature once region inference can see the
        //    loop at all. `science-regions`' `check` §6 suppresses `SC0340`
        //    there today *because* the callee is unresolved, and says the
        //    suppression *"should be removed when Decision 11's method lookup
        //    reaches the container types"*. Removing it now turns a correct
        //    program into an error, which is the measurement that sent this
        //    line back.
        //
        // So the seam §7.2 named is closed on the THIR side — `ExprKind::For`
        // carries the `DefId`, `check`'s `iterate_item` returns it beside the
        // element type it was already computing, and neither is looked up twice
        // — and this is the one line that flips when the four above are
        // answered.
        // **Resolved only when the `next` belongs to a *type*, and that
        // discriminator is the whole of why this line can be flipped at all.**
        //
        // The decision. A `next` whose owner is an implementation block becomes
        // a `Callee::Def`; one whose owner is the `interface` stays
        // `Unresolved::IterateNext`.
        //
        // The reason. The four things standing between the resolved callee and
        // a loop that runs were listed here, and the discriminator answers
        // three of them at once. A `next` declared on the interface has **no
        // body anywhere** and needs one copy per implementor, which is
        // monomorphisation; `Array of T implements Iterate` has nowhere to keep
        // a cursor, so its `next` could not advance whatever called it; and
        // `science-regions` suppresses `SC0340` for a body that calls something
        // unresolved, so resolving those would turn `examples/19_stdlib`'s
        // `find` into a false positive. All three are true of exactly the
        // blocks that declare `methods: &[]`, and of none that declare a `next`
        // of their own — today that is `Chars`, whose `next` is
        // `science_chars_next` and whose state is its own offset.
        //
        // The cost, and it is the fourth item unchanged. `Iterate.next` is
        // `def next(mutable self)`, and §4.4 says a `for`'s source is borrowed
        // *shared*. They still disagree, and this takes the exclusive borrow
        // only where the subject is a temporary the loop itself owns — see the
        // borrow above. A subject that is a named collection keeps §4.4's
        // shared borrow, because that is the case §4.4 is about:
        // `collections-and-chains.md` §4.2's AMENDMENT 11 says `for x in xs:`
        // desugars to `xs.iterate()`, so the collection is borrowed shared by
        // `iterate` and the *iterator* is what the loop advances. `Array` has
        // no `iterate` yet, which is why it is still on the unresolved side.
        let callee = match next_method.filter(|def| self.owner_is_a_type(*def)) {
            Some(def) => Callee::Def { def, self_ty: None },
            None => Callee::Unresolved(Unresolved::IterateNext),
        };
        self.terminate(
            head,
            TerminatorKind::Call {
                callee,
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

        self.loops.push(LoopScope { head, continue_to: head, exit, depth: self.scopes.len() });
        self.push_scope();
        // Decision 7's narrowing, as a statement: the `If` above established
        // that the `Item?` holds a value and this is the read of it. Same shape
        // `ExprKind::Narrow` lowers to one construct over, and it works for
        // both of Decision 18's and 19's layouts because `Rvalue::Narrow` now
        // has a lowering for each.
        //
        // **A move when the item owns something.** `Chars` and `Range` hand
        // out a `Char` and an `Int`, and a copy of either is the value; `Lines`
        // hands out an owned `String`, and a copy of that is a second owner of
        // one buffer — the option releasing it at the end of the iteration and
        // the binding releasing it again. Moving is §5's answer for a
        // non-`Copy` read, and it is the one the backend's `Rvalue::Narrow`
        // can prove sound: the drop flag on `element` is cleared here, so only
        // the binding releases the line.
        let narrowed = self.temp(element_ty, span, body_block);
        let read = self.read(Place::local(element), element_ty);
        self.assign(
            body_block,
            Place::local(narrowed),
            Rvalue::Narrow { operand: read, ty: element_ty },
            span,
        );
        let bound = self.bind_pattern(&Place::local(narrowed), pattern, body_block);
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
            Some((place, block)) => {
                // A narrowed `(&T)?` is a niche: the place is still the
                // nullable, the scrutinee is the borrow, and the tag to read
                // is the referent's, so step through the pointer first.
                let place = self.narrowed_place(place, scrutinee);
                (self.deref_to_hole(place, scrutinee), block)
            }
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
                // Decision 28's AMENDMENT 6: a variant pattern under a `Box`
                // tests the payload's tag, never the box's pointer.
                let place = &self.box_deref(place.clone());
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
                let (place, record_ty) = self.pattern_payload(place, pat.ty);
                let place = &place;
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
    ///
    /// **A one-element variant payload is tested directly against `place`,
    /// with no `TupleField` in between.** `Lowerer::choice_ty` on the codegen
    /// side never wraps a single payload element in a struct of its own — *"a
    /// one-element payload is its element"*, `Lowerer::lower_variant`'s words
    /// for the matching decision on construction — so `place`'s layout,
    /// post-`Downcast`, already **is** that element's layout. Projecting
    /// `TupleField { index: 0 }` on top of it asks `place_address` for *field
    /// 0 of the element*, not the element itself, and every non-scalar
    /// element (`Ident(String)`, `Circle(Point)`) has a field 0 of its own —
    /// `String`'s first field is its byte pointer, so `Ident(name)` bound
    /// `name` to that pointer, typed as though it were the whole `String`,
    /// which is `emit.rs`'s own store-width check catching a mismatch it
    /// cannot explain and this crate reports as `SC0402`. A plain tuple has
    /// no such collapsing — `positional_ty`'s `variant: None` arm is untouched
    /// and every element still gets its own `TupleField` — so the guard below
    /// reads `variant.is_some()` and not merely `elements.len() == 1`.
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
        if variant.is_some() && elements.len() == 1 {
            self.test_pattern(place, elements[0], block, success, fail);
            return;
        }
        let mut current = block;
        for (index, element) in elements.iter().enumerate() {
            let next = self.new_block();
            let ty = self.positional_ty(place, variant, index, *element);
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
                let (operand, block) = self.read_ergonomic(place.clone(), pat.ty, block, span);
                self.assign(block, Place::local(local), Rvalue::Use(operand), span);
                block
            }
            PatKind::Variant { def: Some(variant), elems } => {
                let variant = *variant;
                let place = &self.box_deref(place.clone());
                let choice_ty = self.variant_payload_ty(variant, None).unwrap_or(pat.ty);
                let down = place.project(Projection::Downcast { variant, ty: choice_ty });
                // A one-element payload is bound straight off `down`, with no
                // `TupleField` — `test_sequence`'s doc comment above gives the
                // reason, and it is the same rule here: `Ident(name)`'s `name`
                // is the `String` `down` already names, not field 0 of it.
                if let [only] = elems.as_slice() {
                    return self.bind_pattern(&down, *only, block);
                }
                for (index, element) in elems.iter().enumerate() {
                    let ty = self.positional_ty(&down, Some(variant), index, *element);
                    let sub = down.project(Projection::TupleField { index: index as u32, ty });
                    block = self.bind_pattern(&sub, *element, block);
                }
                block
            }
            PatKind::Tuple(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    let ty = self.positional_ty(place, None, index, *element);
                    let sub = place.project(Projection::TupleField { index: index as u32, ty });
                    block = self.bind_pattern(&sub, *element, block);
                }
                block
            }
            PatKind::Record { def: Some(def), fields } => {
                let owner = *def;
                let (place, record_ty) = self.pattern_payload(place, pat.ty);
                let place = &place;
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

    /// A record pattern's place and the type its fields are substituted by,
    /// once any `Box` above it is crossed — Decision 28's AMENDMENT 6.
    ///
    /// **The type comes off the place when a `Box` was crossed, and off the
    /// pattern otherwise.** `science-types` types a pattern at its scrutinee,
    /// so a record pattern under a `Box[Pair of Int]` carries `Box[Pair of
    /// Int]`, and [`Builder::field_ty`] would substitute `Pair`'s parameters
    /// with `Box`'s one argument. The place past the `Deref` is typed by the
    /// box's own argument, which is the record at its real arguments. Every
    /// pattern that crosses no `Box` keeps exactly the type it always had.
    fn pattern_payload(&mut self, place: &Place, pat_ty: Ty) -> (Place, Ty) {
        let peeled = self.box_deref(place.clone());
        if peeled.projection.len() == place.projection.len() {
            return (peeled, pat_ty);
        }
        let ty = self.place_ty(&peeled);
        (peeled, ty)
    }

    /// Decision 28's AMENDMENT 6 at an argument: `borrowed Box of T` into
    /// `borrowed T`, lowered as a shared reborrow of the box's payload.
    ///
    /// **Not an `Rvalue::Coerce`, deliberately.** A borrow of a box is a
    /// pointer to a pointer, and the `borrowed T` a callee wants is the inner
    /// one; the MIR that says so is `&(*box)` — a [`Projection::Deref`]
    /// through the box, exactly [`Builder::box_deref`]'s projection — and a
    /// `Ref` of that place is a loan `science-regions` already checks, of the
    /// payload, reached through the box. A coercion rvalue would say the same
    /// thing in a form nothing downstream reads as a loan.
    ///
    /// **The operand is either the borrow of a box or a value of `&Box[T]`.**
    /// The first is the auto-borrow (`bump(boxed)`, §6.3) or a written
    /// `&boxed`, and the place borrowed is the box itself — no temporary is
    /// made for the intermediate `&Box[T]`, which would be a loan of the box
    /// whose only use is to be read through. The second is a parameter, a
    /// Decision 27 binding (`Neg(inner)` under a borrowed scrutinee), or
    /// anything else already typed `&Box[T]`: [`Builder::auto_deref`] takes
    /// its place to the box, and one `Deref` more reaches the payload.
    fn borrow_through_box(
        &mut self,
        dest: Place,
        operand: ExprId,
        block: BlockId,
        span: Span,
        in_argument: bool,
    ) -> BlockId {
        let thir = self.thir;
        let source = match thir.expr(operand).kind {
            ExprKind::Borrow { operand: boxed, .. } => boxed,
            _ => operand,
        };
        let (place, block) = match self.as_place(source, block) {
            Some(found) => found,
            None => {
                let ty = thir.ty(source);
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), source, block);
                (Place::local(temp), block)
            }
        };
        let place = self.auto_deref(place);
        let boxed = self.revealed(self.place_ty(&place));
        let payload = match self.context.types.kind(boxed) {
            TyKind::Named { args, .. } => args.first().and_then(|arg| arg.as_type()),
            _ => None,
        }
        .unwrap_or(Ty::ERROR);
        let place = place.project(Projection::Deref { ty: payload });
        self.borrow_place(dest, false, place, block, span, in_argument)
    }

    /// `Coercion::Reborrow`: the exclusive borrow `operand` evaluates to is not
    /// moved but looked through, and what it points at is borrowed shared for
    /// as long as the result lives. `r` stays usable afterwards, and the region
    /// engine sees an ordinary shared loan of `*r`.
    fn shared_reborrow(
        &mut self,
        dest: Place,
        operand: ExprId,
        block: BlockId,
        span: Span,
        in_argument: bool,
    ) -> BlockId {
        let (place, block) = match self.as_place(operand, block) {
            Some(found) => found,
            None => {
                let ty = self.thir.ty(operand);
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), operand, block);
                (Place::local(temp), block)
            }
        };
        let held = self.revealed(self.place_ty(&place));
        let place = match *self.context.types.kind(held) {
            TyKind::Borrowed { inner, .. } => place.project(Projection::Deref { ty: inner }),
            _ => place,
        };
        self.borrow_place(dest, false, place, block, span, in_argument)
    }

    // --- calls ------------------------------------------------------------

    /// `self_ty` is [`thir::ExprKind::Call`]'s own field: the type an
    /// associated call was reached through, and `None` for every other call.
    /// It is carried onto [`Callee::Def`] unchanged — this crate does not
    /// compute it and does not check it, because the only thing it could
    /// check it against is the lookup that produced it.
    fn lower_call(
        &mut self,
        dest: Place,
        callee: ExprId,
        args: &[ExprId],
        self_ty: Option<Ty>,
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
            ExprKind::Item(def) => Callee::Def { def, self_ty },
            _ => {
                let (operand, next) = self.operand(callee, block);
                block = next;
                Callee::Indirect(operand)
            }
        };
        // §10's builder again, for the one call in the language that renders
        // its argument. See [`Builder::lower_print_rendered`].
        if let Callee::Def { def, .. } = callee {
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
            block = next;
            let operand = self.pin_literal_argument(&callee, *arg, operand, block);
            operands.push(if opaque { force_copy(operand) } else { operand });
        }
        self.emit_call(dest, callee, operands, block, span)
    }

    /// A literal passed to a **generic** parameter, given a local so that it
    /// has a type.
    ///
    /// **The fact that was missing.** An `Operand::Const` carries no type —
    /// *"the value a `1` denotes is a fact about its type, and the type is on
    /// the statement beside it"* — and `science_codegen::mono`'s `solve_call`
    /// solves a callee's type parameters by matching each declared parameter
    /// against the type of the argument's *place*. A literal has no place, so
    /// `run(3, 1)` where `run[T](x: T, k: Int)` left `T` with nothing to be
    /// solved from but the destination, which is `Int` and says nothing about
    /// `T`: the instance was never named, and the backend refused the generic
    /// body with *"a value whose type is still a type parameter"*. It looked
    /// like a closure's fault when the second argument was one, because that
    /// is how it was found; `run(3, 1)` fails identically.
    ///
    /// **Only for a callee that has a parameter to solve, and only for a
    /// scalar literal.** Every other call keeps the constant as it was, so no
    /// body without a generic callee changes and `science-mir`'s snapshots
    /// stay as they are. A string literal is not pinned: it is not `Copy`,
    /// the temporary would own a `String`, and a drop that nothing asked for is
    /// worse than a parameter left to the destination. The temporary is
    /// `thir`'s own type for the argument, which is the checker's answer for
    /// the literal after inference (`3` is `U8` where the parameter said so).
    fn pin_literal_argument(
        &mut self,
        callee: &Callee,
        arg: ExprId,
        operand: Operand,
        block: BlockId,
    ) -> Operand {
        let Operand::Const(Constant::Literal(
            Literal::Int { .. } | Literal::Float { .. } | Literal::Bool(_) | Literal::Char(_),
        )) = &operand
        else {
            return operand;
        };
        let Callee::Def { def, .. } = callee else { return operand };
        let Some(signature) = self.context.decls.signature(*def) else { return operand };
        let generic = !signature.generics.is_empty()
            || signature
                .owner
                .and_then(|owner| self.context.decls.block_generics(owner))
                .is_some_and(|generics| !generics.is_empty());
        if !generic {
            return operand;
        }
        let span = self.thir.expr(arg).span;
        let ty = self.thir.ty(arg);
        let temp = self.temp(ty, span, block);
        self.assign(block, Place::local(temp), Rvalue::Use(operand), span);
        self.read(Place::local(temp), ty)
    }

    /// Whether `print(x)` or `write(x)` has to render `x` before it can print
    /// it.
    ///
    /// True for the prelude's `print` or `write` called on one argument whose
    /// type is not a `String` **and which [`Builder::push_of`] has an entry
    /// point for**. The two are one condition and not two:
    /// `strings-formatting-and-docs.md` §4.1 gives both the same
    /// `(value: borrowed any Display)`, so whatever makes `print(42)` need
    /// rendering makes `write(42)` need it identically, and `science-codegen-
    /// llvm`'s `lower_print` reads which runtime symbol to call off the
    /// def's own name — so once that crate lowers a call to `write` at all,
    /// keeping this condition at `entry.name != "print"` would just move the
    /// gap from *"no lowering"* to *"no rendering"* for the same call.
    ///
    /// **`print_error` and `write_error` are the same condition, one stream
    /// over.** §4.2's table gives them `print`'s and `write`'s shape exactly —
    /// one `borrowed any Display`, a newline or none — and differing only in
    /// the file descriptor, which is `lower_print`'s business and not this
    /// crate's. Leaving them off this list would make `print_error(42)` reach
    /// the backend as an `Int` handed to a `String` writer, refused there by
    /// type, while `print(42)` beside it builds: the gap the paragraph above
    /// describes, reopened for two new names.
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
    fn prints_by_rendering(&mut self, def: DefId, arg: ExprId) -> bool {
        let entry = self.context.defs.get(def);
        if !entry.is_builtin()
            || !matches!(entry.name.as_str(), "print" | "write" | "print_error" | "write_error")
        {
            return false;
        }
        // `Write.write` is a builtin named `write` too, and it is a method:
        // the free function's parent is the prelude module, a method's is its
        // interface.
        if entry.parent.is_some_and(|parent| self.context.defs.get(parent).kind != DefKind::Module) {
            return false;
        }
        let ty = self.thir.ty(arg);
        let stripped = self.stripped(ty);
        if self.context.decls.prelude().is(self.context.types, stripped, "String") {
            return false;
        }
        if !matches!(self.push_of(ty), Push::Missing) {
            return true;
        }
        // **And a type the builder cannot render but the *program* can**,
        // which is the exclusion above with its second half restored.
        // [`Builder::lower_fstring`] now has two answers for a
        // [`Push::Missing`] hole — §3.1's `display`, or
        // [`Unresolved::Display`] — so *"a type the builder cannot render
        // would be rewritten into a call sequence that ends in an
        // `Unresolved::Display`"* is only still true of the second. A type
        // with a `display` reaches an executable through the rewrite and
        // nothing else, so refusing to rewrite it would be refusing the whole
        // feature.
        //
        // The cost the exclusion was protecting against is paid exactly where
        // it was described and nowhere new: §1.6 makes a hole a borrow, so
        // `print(v)` on a type that implements `Display` acquires a shared
        // loan of `v` the author did not write. That is the loan `display`'s
        // own `self` parameter would have taken had the author written
        // `v.display(into)` by hand, and `science-regions`' `tests/narrowing.rs`
        // — the fixture whose whole point is that its body contains one borrow
        // — prints an `Int?`, which has no `display` and is unaffected.
        self.display_of(ty).is_some()
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
    /// and it is the f-string builder: `science-rt`'s nine
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
            return self.emit_call(
                dest,
                Callee::Def { def: print, self_ty: None },
                vec![force_copy(operand)],
                next,
                span,
            );
        };
        let rendered = self.temp(string, span, block);
        let parts = [thir::FStringPart::Hole(arg, None)];
        let block = self.lower_fstring(Place::local(rendered), &parts, block, span);
        self.emit_call(
            dest,
            Callee::Def { def: print, self_ty: None },
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
    /// `Def`s in the language are `print`, `write`, `print_error` and
    /// `write_error`, and §4.1 and §4.2 declare all four `borrowed` — and the direction is the one §5 already chose for
    /// [`Callee::Unresolved`]: a leak a profiler finds rather than a
    /// use-after-free a user finds.
    fn has_no_signature(&self, callee: &Callee) -> bool {
        match callee {
            Callee::Def { def, .. } => self.context.decls.signature(*def).is_none(),
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
        // **A terminal is the whole chain**, and it is taken before the
        // ordinary path because there is no receiver *value* to lower: every
        // adapter between here and the source is a type the note says does no
        // work (§2.1), and building one would mean giving `MapOver` a layout
        // nothing ever reads. See [`Builder::lower_chain_terminal`].
        if let Some(def) = method {
            let terminal = self
                .chain_method(def)
                .and_then(|(_, name)| Terminal::named(name, args));
            if let Some(terminal) = terminal {
                if let Some(chain) = self.chain_of(receiver) {
                    return self.lower_chain_terminal(dest, terminal, &chain, block, span);
                }
            }
        }
        // **`a.wrapping_mul(b)` is `a * b`**, and is built as exactly that
        // `Rvalue::Binary`: `builtins.rs`' `wrapping_block!` comment is the
        // decision, and `science-codegen-llvm`'s `lower_binary` is why the
        // plain operator already is the wrapping instruction. Both operands
        // are integer scalars, so both are copies and nothing is consumed.
        if let Some(op) = method.and_then(|def| self.wrapping_operator(def)) {
            if let [other] = args {
                let (left, next) = self.operand(receiver, block);
                let (right, next) = self.operand(*other, next);
                self.assign(next, dest, Rvalue::Binary { op, lhs: left, rhs: right }, span);
                return next;
            }
        }
        if let (Some(def), [key]) = (method, args) {
            if self.is_array_sort_by(def) {
                return self.lower_array_sort_by(dest, receiver, *key, block, span);
            }
        }
        let callee = match method {
            Some(def) => Callee::Def { def, self_ty: None },
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
                //
                // `deref_to_hole` is the narrow's half of the same rule, and
                // this is its third caller for its third symptom: `a.length()`
                // where `a` is a `(borrowed String)?` inside `if a?:` reborrowed
                // the *slot*, so `science_string_len` read a length field out of
                // the address of a pointer and printed it. No verifier objects
                // — the argument is a `ptr` either way — which is why this one
                // had to be read off a program's stdout.
                let place = self.auto_deref(place);
                let place = self.deref_to_hole(place, receiver);
                // Decision 28. `science-types`' `check::BodyChecker::lookup`
                // resolved this call by peeling through a `Box` as well as a
                // borrow, so the address this receiver borrows is one layer
                // further in than `auto_deref` — which only ever peels a
                // `TyKind::Borrowed` — goes on its own. `box_deref` is that one
                // more step.
                let place = self.box_deref(place);
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
            block = next;
            let operand = self.pin_literal_argument(&callee, *arg, operand, block);
            operands.push(if unresolved { force_copy(operand) } else { operand });
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
        // **A place read is evaluated before the activation, not at the call.**
        // `fail(c, "msg", c.pos)` reserves `c`, then reads `c.pos` for the third
        // argument; left as `Operand::Copy(c.pos)` in the call, that read
        // happens at the terminator, *after* the activation, and is an access
        // to a place an exclusive borrow holds (`SC0330`). Rust evaluates every
        // argument into a temporary first, and this does the same for the
        // operands that read through a projection, which are the ones that can
        // name a place a two-phase borrow reserved. Only when there is an
        // activation to come before them.
        let mut args = args;
        if !activations.is_empty() {
            for arg in args.iter_mut() {
                let Operand::Copy(place) = arg else { continue };
                if place.projection.is_empty() {
                    continue;
                }
                let place = place.clone();
                let ty = self.place_ty(&place);
                let temp = self.temp(ty, span, block);
                self.assign(block, Place::local(temp), Rvalue::Use(Operand::Copy(place)), span);
                *arg = Operand::Copy(Place::local(temp));
            }
        }
        for borrow in activations {
            self.push_statement(block, StatementKind::Activate(borrow), span);
        }

        let diverges = match &callee {
            Callee::Def { def, .. } => {
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
        // **A call writes a whole local, and a field is moved into after.**
        // `b.buffer be Array[U8].new()` put the call's destination on a
        // projection, and `science-codegen-llvm` has one slot per local for a
        // return to land in — `sret_slot` names a local, not an address — so it
        // refused *"a call whose result goes through a field"*. A temporary of
        // the field's type takes the result and a move puts it in place, which
        // is how every other rvalue already reaches a field.
        let (destination, into_field) = if dest.projection.is_empty() {
            (dest, None)
        } else {
            let ty = self.place_ty(&dest);
            let temp = self.temp(ty, span, block);
            (Place::local(temp), Some((dest, temp, ty)))
        };
        self.terminate(block, TerminatorKind::Call { callee, args, destination, target }, span);
        if let (Some(target), Some((field, temp, ty))) = (target, into_field) {
            let read = self.read(Place::local(temp), ty);
            self.assign(target, field, Rvalue::Use(read), span);
        }
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
                thir::FStringPart::Hole(hole, Some(spec)) => {
                    block = self.render_with_spec(&dest, &discard, *hole, spec, block, span);
                }
                thir::FStringPart::Hole(hole, None) => {
                    let hole = *hole;
                    let hole_ty = self.thir.ty(hole);
                    let push = self.push_of(hole_ty);
                    // §3.1's own answer to a hole the builder has no entry
                    // point for, tried before [`Unresolved::Display`] is
                    // reached for: the user wrote a `display`, so call it.
                    if push == Push::Missing {
                        if let Some(display) = self.display_of(hole_ty) {
                            block = self.render_through_display(
                                &dest,
                                &discard,
                                hole,
                                display,
                                block,
                                span,
                            );
                            continue;
                        }
                    }
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
        self.exclusive_ref(dest, block, span)
    }

    /// `mutable borrowed T` into a fresh temporary, for a place of any type.
    ///
    /// [`Builder::accumulator_ref`] was this function under a name that said
    /// which place it was for; [`Builder::render_through_display`] needs the
    /// same three statements for a `Formatter` local, twice, so the shape is
    /// named once rather than written three times. The reference's type is
    /// read off the *place* for §9's reason, unchanged.
    fn exclusive_ref(&mut self, place: &Place, block: BlockId, span: Span) -> (Operand, BlockId) {
        let ty = self.place_ty(place);
        let borrowed = self.context.types.borrowed(true, ty);
        let temp = self.temp(borrowed, span, block);
        let block = self.borrow_place(Place::local(temp), true, place.clone(), block, span, true);
        (Operand::Move(Place::local(temp)), block)
    }

    /// The `Display.display` a hole of this type would render through, when
    /// the program wrote one with a body.
    ///
    /// # The decision
    ///
    /// A hole [`Builder::push_of`] has no entry point for is looked up in
    /// [`science_types::methods::Methods`] for a `display` reached **through
    /// the prelude's `Display`**, and only a candidate whose owner is a *type*
    /// is answered with.
    ///
    /// # The reason
    ///
    /// `strings-formatting-and-docs.md` §4.1 makes `print` take `&any
    /// Display`, and §3.1 says what `Display` is:
    /// `def display(self, into: mutable borrowed Formatter)`. Until that
    /// signature was declared there was no method to call and every such hole
    /// was [`Unresolved::Display`]; it is declared now, in
    /// `science-resolve`'s `builtins.rs`, so the honest answer for a type that
    /// implements it is the call and not the refusal.
    ///
    /// **Three filters, each load-bearing.**
    ///
    /// *Through `Display` and not by name.* A type may have a `display` of its
    /// own in a `has:` block that has nothing to do with the interface, and
    /// calling it would be `print` dispatching on a name. [`Candidate::
    /// interface`] is what tells the two apart, and it is compared against the
    /// prelude's own `DefId` — `for` resolves `Iterate` the same way, and
    /// `examples/06_traits.science`'s own comment explains why a name would
    /// not do.
    ///
    /// *Exactly one candidate.* [`Found::Ambiguous`] and [`Found::Instances`]
    /// are the checker's to resolve at a call site the author wrote; this is
    /// not one, so there is nothing here to disambiguate with and the refusal
    /// is the honest answer.
    ///
    /// *Owner is a type.* [`Builder::owner_is_a_type`]'s question, for
    /// `lower_for`'s reason: a `display` whose owner is the `interface` is a
    /// declaration with no body, or a default that needs monomorphising
    /// against each implementor before it is a callee anything can emit.
    /// `Display` declares no default, so today this filter only ever excludes
    /// the bare declaration — which is what a `T: Display` bound inside a
    /// generic body would find.
    ///
    /// # The cost
    ///
    /// **`any Display` is still [`Unresolved::Display`].** An interface object
    /// has no head type for [`Methods::receiver`] to answer with; its
    /// `display` is in a vtable slot, and reaching it is Decision 13's
    /// dispatch rather than this lookup. So `print` of a `&any Display`
    /// refuses where `print` of the concrete type behind it succeeds, and the
    /// refusal names the construct.
    fn display_of(&mut self, ty: Ty) -> Option<DefId> {
        let ty = self.stripped(ty);
        let interface = self.context.decls.prelude().get("Display")?;
        let methods = self.context.decls.methods();
        let head = methods.receiver(self.context.defs, self.context.types, ty)?;
        let Found::One(candidate) = methods.lookup(head, "display", Form::Value) else {
            return None;
        };
        if candidate.interface() != Some(interface) {
            return None;
        }
        self.owner_is_a_type(candidate.method).then_some(candidate.method)
    }

    /// One hole, rendered by the user's own `display`: §3.1's three statements.
    ///
    /// ```text
    /// _f = Formatter                       // a local, laid out by the backend
    /// science_formatter_init(&mut _f, &mut accumulator)
    /// Vector2::display(&hole, &mut _f)
    /// ```
    ///
    /// **The `Formatter` is a local of this frame and not a value that
    /// travels.** §2.6 of the note puts *"a `Formatter` the user can pass
    /// around as a value in F0"* in the deliberately-absent column — it exists
    /// as a parameter to `display` and nowhere else — so the shortest thing
    /// that satisfies §3.1 is a temporary borrowed twice: once to be
    /// initialised, once to be handed to `display`. Two borrows and not one
    /// because the first ends with the `init` call; a single long-lived loan
    /// would make the second a reborrow of a loan `science-regions` has no
    /// reason to keep alive.
    ///
    /// **The accumulator is borrowed exclusively, which the `init` call then
    /// stores.** That is the one place this lowering hands a borrow to
    /// something that outlives the call: `science_formatter_init` writes the
    /// pointer into the `Formatter`, and every `into.text(…)` inside `display`
    /// writes through it. The loan is live for exactly as long as the
    /// `Formatter` is, and both die at the end of this hole's statements,
    /// which is what makes it sound without a region annotation nothing here
    /// could express.
    ///
    /// **The receiver is a *shared* borrow**, because §3.1 writes bare `self`
    /// and not `mutable self`: rendering a value does not change it. That is
    /// [`Builder::borrow_hole`] unchanged — the same call the `Push::Pointer`
    /// path makes for a `String` hole — so a `Copy` record and a borrowed one
    /// reach `display` the same way.
    fn render_through_display(
        &mut self,
        dest: &Place,
        discard: &Place,
        hole: ExprId,
        display: DefId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(formatter_ty) = self.context.decls.prelude().ty(self.context.types, "Formatter")
        else {
            // No prelude means no `Formatter` to build, which is every
            // hand-assembled definition table in this crate's own tests. There
            // is no call to emit, so the hole falls back to the refusal it had
            // before this function existed — reached by the caller, because
            // `display_of` cannot have answered `Some` without a prelude
            // either.
            let (accumulator, block) = self.accumulator_ref(dest, block, span);
            let (value, block) = self.borrow_hole(hole, block, span);
            return self.emit_call(
                discard.clone(),
                Callee::Unresolved(Unresolved::Display),
                vec![accumulator, value],
                block,
                span,
            );
        };
        let formatter = Place::local(self.temp(formatter_ty, span, block));
        let (accumulator, block) = self.accumulator_ref(dest, block, span);
        let (sink, block) = self.exclusive_ref(&formatter, block, span);
        let block = self.emit_call(
            discard.clone(),
            Callee::Runtime(FORMATTER_INIT),
            vec![sink, accumulator],
            block,
            span,
        );
        let (value, block) = self.borrow_hole(hole, block, span);
        let (into, block) = self.exclusive_ref(&formatter, block, span);
        // `self_ty: None`, for the reason [`Callee::Def`]'s own doc gives: the
        // receiver is `args[0]` and its type is readable from the operand.
        // `display` takes a `self`, so this is a method call and not the
        // associated-call shape that field exists for.
        self.emit_call(
            discard.clone(),
            Callee::Def { def: display, self_ty: None },
            vec![value, into],
            block,
            span,
        )
    }

    /// One hole with §2.1's spec: a `Formatter` initialised with the spec over
    /// the accumulator, then the one `Formatter` entry point the hole's type
    /// renders through.
    ///
    /// ```text
    /// _f = Formatter
    /// science_formatter_init_spec(&mut _f, &mut accumulator, fill, flags, width, precision)
    /// science_formatter_number(&mut _f, x)          // an F64
    /// ```
    ///
    /// # The decision
    ///
    /// **Every spec'd hole goes through the `Formatter`, and no
    /// `science_string_push_*` takes a spec.** The `Formatter` entry points
    /// already render §2.2's whole table — `science-rt` built them for §3.1's
    /// `display` — so the spec'd path is the unspec'd `display` path with a
    /// non-default spec, and the table lives in one place. The types split
    /// four ways:
    ///
    /// | Hole | Entry point |
    /// |---|---|
    /// | `F64`, `Float` | `science_formatter_number` |
    /// | `F32` | `science_formatter_number32` |
    /// | `I8`…`I64`, `Int` | `science_formatter_integer`, widened to `I64` |
    /// | `U8`…`U64` | `science_formatter_unsigned`, widened to `U64` |
    /// | `String` | `science_formatter_text`, by borrow |
    /// | `Bool`, `Char`, `IoError`, `TextError` | rendered by their own push into a temporary `String`, then `science_formatter_text` |
    /// | a type with a `display` | that `display`, handed the spec'd `Formatter` — §3.1's `into.spec()` |
    ///
    /// # The reason for the temporary
    ///
    /// A `Bool`, a `Char` and the two error types take only fill, alignment
    /// and width — §2.4's *"anything `Display`"* row — and padding is a
    /// question about the rendered text, not the value. Rendering into a
    /// scratch `String` with the push that already exists and padding that
    /// costs one allocation per spec'd hole of those four types and **no new
    /// entry point**; the alternative is four `science_formatter_*` twins of
    /// four pushes, each differing from its twin by one call to `pad`.
    ///
    /// # The cost
    ///
    /// **A user `display` is trusted with the spec.** §3.3 says so in as many
    /// words — *"`DisplayNumber` says the numeric codes are meaningful; it
    /// does not prove the implementation honours them"* — and the same holds
    /// for width: `into.text` pads, `into.raw` does not, and a `display` that
    /// writes two `text` calls pads each. That is §3.1's contract and not a
    /// gap here.
    fn render_with_spec(
        &mut self,
        dest: &Place,
        discard: &Place,
        hole: ExprId,
        spec: &science_lexer::FormatSpec,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let hole_ty = self.thir.ty(hole);
        let push = self.push_of(hole_ty);
        let display = if push == Push::Missing { self.display_of(hole_ty) } else { None };
        let Some(formatter_ty) = self.context.decls.prelude().ty(self.context.types, "Formatter")
        else {
            // No prelude, no `Formatter`: the hand-built tables of this
            // crate's own tests, none of which writes a spec. The refusal is
            // the one an unrenderable hole gets, for `render_through_display`'s
            // reason.
            let (value, block) = self.borrow_hole(hole, block, span);
            return self.emit_call(
                discard.clone(),
                Callee::Unresolved(Unresolved::Display),
                vec![value],
                block,
                span,
            );
        };
        let formatter = Place::local(self.temp(formatter_ty, span, block));
        let (accumulator, block) = self.accumulator_ref(dest, block, span);
        let (sink, block) = self.exclusive_ref(&formatter, block, span);
        let mut args = vec![sink, accumulator];
        args.extend(spec_operands(spec));
        let block =
            self.emit_call(discard.clone(), Callee::Runtime(FORMATTER_INIT_SPEC), args, block, span);

        if let Some(display) = display {
            let (value, block) = self.borrow_hole(hole, block, span);
            let (into, block) = self.exclusive_ref(&formatter, block, span);
            return self.emit_call(
                discard.clone(),
                Callee::Def { def: display, self_ty: None },
                vec![value, into],
                block,
                span,
            );
        }

        let (symbol, value, block) = match push {
            Push::Value { symbol, widen } if symbol != PUSH_BOOL && symbol != PUSH_CHAR => {
                let entry = match symbol {
                    PUSH_I64 => FORMATTER_INTEGER,
                    PUSH_U64 => FORMATTER_UNSIGNED,
                    PUSH_F32 => FORMATTER_NUMBER32,
                    _ => FORMATTER_NUMBER,
                };
                let (value, block) = self.value_hole(hole, block);
                let (value, block) = match widen {
                    Some(wide) => self.widen_hole(value, hole_ty, wide, block, span),
                    None => (value, block),
                };
                (Callee::Runtime(entry), value, block)
            }
            Push::Pointer(PUSH_STR) => {
                let (value, block) = self.borrow_hole(hole, block, span);
                (Callee::Runtime(FORMATTER_TEXT), value, block)
            }
            Push::Value { symbol, .. } | Push::Pointer(symbol) => {
                let Some(string) = self.context.decls.prelude().ty(self.context.types, "String")
                else {
                    let (value, block) = self.borrow_hole(hole, block, span);
                    return self.emit_call(
                        discard.clone(),
                        Callee::Unresolved(Unresolved::Display),
                        vec![value],
                        block,
                        span,
                    );
                };
                let text = Place::local(self.temp(string, span, block));
                let capacity = self.hole_estimate(hole_ty);
                let block = self.emit_call(
                    text.clone(),
                    Callee::Runtime(STRING_WITH_CAPACITY),
                    vec![Operand::Const(Constant::Count(capacity))],
                    block,
                    span,
                );
                let (scratch, block) = self.exclusive_ref(&text, block, span);
                let (value, block) = match push {
                    Push::Pointer(_) => self.borrow_hole(hole, block, span),
                    _ => self.value_hole(hole, block),
                };
                let block = self.emit_call(
                    discard.clone(),
                    Callee::Runtime(symbol),
                    vec![scratch, value],
                    block,
                    span,
                );
                let (rendered, block) = self.shared_ref(&text, block, span);
                (Callee::Runtime(FORMATTER_TEXT), rendered, block)
            }
            Push::Missing => {
                let (value, block) = self.borrow_hole(hole, block, span);
                (Callee::Unresolved(Unresolved::Display), value, block)
            }
        };
        let (into, block) = self.exclusive_ref(&formatter, block, span);
        self.emit_call(discard.clone(), symbol, vec![into, value], block, span)
    }

    /// `borrowed T` into a fresh temporary — [`Builder::exclusive_ref`]'s
    /// shared twin, for the scratch `String` [`Builder::render_with_spec`]
    /// hands to `science_formatter_text`.
    fn shared_ref(&mut self, place: &Place, block: BlockId, span: Span) -> (Operand, BlockId) {
        let ty = self.place_ty(place);
        let borrowed = self.context.types.borrowed(false, ty);
        let temp = self.temp(borrowed, span, block);
        let block = self.borrow_place(Place::local(temp), false, place.clone(), block, span, true);
        (Operand::Move(Place::local(temp)), block)
    }

    /// §2.4's bounds check, emitted as statements before the place that needs
    /// it.
    ///
    /// # The decision
    ///
    /// `xs[i]` keeps its [`Projection::Index`], and the check that makes the
    /// projection safe is emitted **here**, in MIR, as an ordinary
    /// `science_array_len`, a comparison, and a branch to
    /// `science_panic_bytes`. A backend then lowers the projection itself as
    /// one unconditional call.
    ///
    /// # The reason, which is Decision 42's line
    ///
    /// A bounds check is three basic blocks and a place projection is not a
    /// statement — so a backend reading `Projection::Index` has nowhere to put
    /// the branch, which is the sentence `science-codegen-llvm` refused the
    /// construct with. MIR is where blocks are made. The alternative was to
    /// replace the projection with a `science_array_get` and a null test, which
    /// is fewer instructions and would have taken `Projection::Index` out of
    /// the language: `science-regions` reads it — §2 item 1 does not descend
    /// into an index — and `places.rs` pins its existence. Keeping the
    /// projection keeps the borrow checker's view of `xs[i]` intact, and the
    /// check rides in front of it.
    ///
    /// **One comparison, unsigned, and that is not an optimisation.** An `Int`
    /// index has to be refused for being negative as well as for being too
    /// large; comparing as `U64` does both at once, because a negative `i64`
    /// reinterprets as a `u64` above any length an array can have. The
    /// alternative is two comparisons joined by an `and`, which is two more
    /// blocks for an answer that is the same. `science_array_len` never returns
    /// a negative, so its own cast is faithful.
    ///
    /// # The cost
    ///
    /// **A call per index, and no hoisting.** `for i in 0..xs.length():` pays
    /// `science_array_len` every turn, where the length is loop-invariant and a
    /// real compiler would read it once. That is the cost of the length living
    /// behind Decision 5's runtime call rather than in a field this crate can
    /// name, and it is left to LLVM, which can hoist the call only if it can
    /// prove the array is not written to — which it cannot, today, because
    /// nothing marks `science_array_len` as reading no more than its argument.
    ///
    /// **Anything that is not an `Array of T` is left alone**, which today is
    /// every index into a `Map` and into a user type with an `Index`
    /// implementation. Those are refused below this crate for reasons of their
    /// own, and inventing a length call for them would name an entry point they
    /// do not have.
    fn bounds_check(
        &mut self,
        array: &Place,
        index: Local,
        index_ty: Ty,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(u64_ty) = self.context.decls.prelude().ty(self.context.types, "U64") else {
            return block;
        };
        let Some(bool_ty) = self.context.decls.prelude().ty(self.context.types, "Bool") else {
            return block;
        };
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        if !self.is_array(array) {
            return block;
        }

        // The length, through a shared borrow of the array: `science_array_len`
        // takes a pointer and reads a header field, so the borrow lives exactly
        // as long as the call does.
        let array_ty = self.place_ty(array);
        let borrowed = self.context.types.borrowed(false, array_ty);
        let reference = self.temp(borrowed, span, block);
        let mut block =
            self.borrow_place(Place::local(reference), false, array.clone(), block, span, true);
        let length = self.temp(int_ty, span, block);
        block = self.emit_call(
            Place::local(length),
            Callee::Runtime(ARRAY_LEN),
            vec![Operand::Move(Place::local(reference))],
            block,
            span,
        );

        // Both sides to `U64`, then one `<`.
        let wide_index = self.temp(u64_ty, span, block);
        self.assign(
            block,
            Place::local(wide_index),
            Rvalue::Cast {
                operand: Operand::Copy(Place::local(index)),
                from: index_ty,
                ty: u64_ty,
            },
            span,
        );
        let wide_length = self.temp(u64_ty, span, block);
        self.assign(
            block,
            Place::local(wide_length),
            Rvalue::Cast {
                operand: Operand::Copy(Place::local(length)),
                from: int_ty,
                ty: u64_ty,
            },
            span,
        );
        let ok = self.temp(bool_ty, span, block);
        self.assign(
            block,
            Place::local(ok),
            Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Operand::Copy(Place::local(wide_index)),
                rhs: Operand::Copy(Place::local(wide_length)),
            },
            span,
        );

        let in_bounds = self.new_block();
        let out_of_bounds = self.new_block();
        self.terminate(
            block,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(ok)),
                then_block: in_bounds,
                else_block: out_of_bounds,
            },
            span,
        );
        // **The message names the construct and not the numbers.** A panic that
        // read *"index 7 out of bounds for length 3"* would be the better
        // sentence and needs a formatted `String` built on the failing path,
        // which is `science_string_from_bytes`, three pushes and a free on a
        // path that ends in `science_panic` — every one of which can itself
        // fail while the program is already failing. A constant costs nothing
        // and says which check fired.
        let discard = Place::local(self.push_local(Ty::UNIT, LocalKind::Temp, span));
        self.terminate(
            out_of_bounds,
            TerminatorKind::Call {
                callee: Callee::Runtime(PANIC_BYTES),
                args: vec![Operand::Const(Constant::Literal(Literal::Str(
                    "index out of bounds".to_string(),
                )))],
                destination: discard,
                target: None,
            },
            span,
        );
        in_bounds
    }

    /// §4.6's division guard, emitted as statements before the `/` or the `%`
    /// that needs it.
    ///
    /// # The decision
    ///
    /// `a / b` keeps its [`Rvalue::Binary`], and the two tests that make the
    /// instruction defined are emitted **here**, in MIR, as comparisons, a
    /// [`TerminatorKind::If`] apiece, and a diverging `science_panic_bytes` on
    /// the failing edge — exactly the shape [`Builder::bounds_check`] takes for
    /// `xs[i]`, and for the same reason: a guard is basic blocks, an rvalue is
    /// not a block, and MIR is where blocks are made.
    ///
    /// # The reason, which is `science_codegen::backend::IntOp`'s own sentence
    ///
    /// `IntOp::SDiv` says *"division by zero is a panic the caller has already
    /// guarded, not a trap the backend inserts."* Until now no caller guarded
    /// it, and `science-codegen-llvm` refused integer `/` and `%` outright with
    /// a message naming this missing half. This is that half. It is the caller
    /// the sentence was written for.
    ///
    /// **There are two failing inputs and this refuses both.** Division by zero
    /// is the one everybody names. `Int.min / -1` is the other: its answer is
    /// `2^63`, which is not an `Int`, and it is not merely undefined in the
    /// abstract — `idiv` on x86-64 raises `#DE` and `sdiv` on AArch64 is
    /// unspecified, so the program that reaches it dies with `SIGFPE` and no
    /// message, or worse, silently. LLVM calls *both* of them immediate
    /// undefined behaviour for `sdiv`/`srem`, which at `-O2` licenses deleting
    /// whatever branch was about to test them — so a guard that covered only
    /// the zero would still leave the second input free to delete code around
    /// it. Checking one and not the other would be the more comfortable
    /// half-measure and it is not a correct compiler. The cost of the second
    /// test is paid only where it can fire: unsigned division has no overflow,
    /// and the whole second test is skipped for it.
    ///
    /// **The tests are bit-pattern comparisons, not casts.** `b == 0`,
    /// `b == -1` and `a == T.min` are each one [`BinaryOp::Eq`] against an
    /// integer constant whose *bits* are written out at the operand's width —
    /// all-ones for `-1`, the sign bit alone for the minimum. Equality does not
    /// read signedness, so this needs no widening cast and no
    /// signed-versus-unsigned comparison; the alternative, casting both sides
    /// to `U64` the way [`Builder::bounds_check`] does, would be two more
    /// statements for a question that is already answered at the natural width,
    /// and `bounds_check` only pays them because *its* question is an ordering
    /// and orderings do read signedness.
    ///
    /// **The numerator is tested last and only on the `-1` edge**, so the
    /// common path through a division is one comparison and one branch, not
    /// three. The two tests are chained as separate [`TerminatorKind::If`]s
    /// rather than joined by an `and` for the reason §10 item 1 already gives
    /// for `and`: it is blocks and edges either way, and this way the second
    /// comparison is not evaluated when the first has already decided.
    ///
    /// # The cost
    ///
    /// **A branch per division, and no hoisting.** `n / 2` pays a test against
    /// zero that a constant folder ought to delete at MIR level; nothing here
    /// folds it, and this crate leaves it to LLVM, which does delete it when
    /// the divisor is a literal because it can see the comparison's answer.
    /// Where the divisor is a loop-invariant variable the test stays in the
    /// loop, which is the same cost `bounds_check` records.
    ///
    /// **Four extra basic blocks per signed division**, two per unsigned. That
    /// breaks the *"every MIR basic block becomes exactly one LLVM basic
    /// block"* reading of the MIR-dump-to-IR-dump correspondence in the same
    /// way `bounds_check` already breaks it — the correspondence survives, it
    /// is just that one *expression* is now several blocks, which was already
    /// true of `and`, `or` and `if`.
    ///
    /// **The messages name the check and not the numbers**, for the reason
    /// [`Builder::bounds_check`]'s note gives in full: formatting the operands
    /// means building a `String` on the path that is already failing.
    ///
    /// **A type this cannot name in the prelude is left unguarded** rather than
    /// guessed at — a hand-built definition table with no prelude in it, which
    /// is every test in this crate that does not go through the resolver. Those
    /// never reach a backend. `Float` division is left alone on purpose: IEEE
    /// 754 division by zero is an infinity, which is a value, and `frem`'s note
    /// in `science-codegen-llvm` says the same of the remainder.
    fn division_check(
        &mut self,
        lhs: Operand,
        rhs: Operand,
        ty: Ty,
        block: BlockId,
        span: Span,
    ) -> (Operand, Operand, BlockId) {
        let ty = self.stripped(ty);
        let Some((bits, signed)) = self.integer_width(ty) else { return (lhs, rhs, block) };
        let Some(bool_ty) = self.context.decls.prelude().ty(self.context.types, "Bool") else {
            return (lhs, rhs, block);
        };

        // Both operands into temporaries first. The guard reads each of them
        // twice — once to test, once to divide — and an [`Operand`] handed in
        // may be a `Move` out of a place, which is readable exactly once.
        let numerator = self.temp(ty, span, block);
        self.assign(block, Place::local(numerator), Rvalue::Use(lhs), span);
        let divisor = self.temp(ty, span, block);
        self.assign(block, Place::local(divisor), Rvalue::Use(rhs), span);

        // `b != 0`.
        let nonzero = self.temp(bool_ty, span, block);
        self.assign(
            block,
            Place::local(nonzero),
            Rvalue::Binary {
                op: BinaryOp::Ne,
                lhs: Operand::Copy(Place::local(divisor)),
                rhs: Self::bits(0),
            },
            span,
        );
        let divides = self.new_block();
        let by_zero = self.new_block();
        self.terminate(
            block,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(nonzero)),
                then_block: divides,
                else_block: by_zero,
            },
            span,
        );
        self.panic_with(by_zero, "divide by zero", span);

        let mut block = divides;
        if signed {
            // `b == -1`, as all-ones at this width.
            let all_ones = (1u128 << bits) - 1;
            let minus_one = self.temp(bool_ty, span, block);
            self.assign(
                block,
                Place::local(minus_one),
                Rvalue::Binary {
                    op: BinaryOp::Eq,
                    lhs: Operand::Copy(Place::local(divisor)),
                    rhs: Self::bits(all_ones),
                },
                span,
            );
            let maybe_overflow = self.new_block();
            let ok = self.new_block();
            self.terminate(
                block,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(minus_one)),
                    then_block: maybe_overflow,
                    else_block: ok,
                },
                span,
            );

            // `a == T.min`, as the sign bit alone. Reached only when the
            // divisor is already known to be `-1`.
            let least = self.temp(bool_ty, span, maybe_overflow);
            self.assign(
                maybe_overflow,
                Place::local(least),
                Rvalue::Binary {
                    op: BinaryOp::Eq,
                    lhs: Operand::Copy(Place::local(numerator)),
                    rhs: Self::bits(1u128 << (bits - 1)),
                },
                span,
            );
            let overflows = self.new_block();
            self.terminate(
                maybe_overflow,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(least)),
                    then_block: overflows,
                    else_block: ok,
                },
                span,
            );
            self.panic_with(overflows, "division overflows", span);
            block = ok;
        }

        (
            Operand::Copy(Place::local(numerator)),
            Operand::Copy(Place::local(divisor)),
            block,
        )
    }

    /// §4.6's shift guard, emitted as statements before the `<<` or `>>` that
    /// needs it — [`Builder::division_check`]'s shape, and for the identical
    /// reason: `science_codegen::backend::IntOp`'s note on `SDiv` is *"a panic
    /// the caller has already guarded, not a trap the backend inserts"*, and
    /// `IntOp::Shl`/`AShr`/`LShr` are the same sentence again. LLVM's
    /// `shl`/`lshr`/`ashr` are poison at or past the operand's bit width —
    /// not a wrong value, no value — exactly as `sdiv` is undefined at a zero
    /// divisor, so `science-codegen-llvm` refused both outright until this
    /// guard existed to be the caller the sentence was written for.
    ///
    /// **Two rejected answers, before this one.** Masking the amount
    /// (`n & (width - 1)`, the rule x86's own `shl` instruction already
    /// applies in hardware) turns `1i64 << 64` into `1i64 << 0`, which is
    /// `1` — and that is not a wrapped *value* in §2.1's sense, because
    /// nothing about `+`'s two's-complement wraparound licenses silently
    /// changing *which bit gets set*; a wrapped `Int` still answers the
    /// question the program asked, and a masked shift answers a different
    /// one. Producing zero unconditionally reads as the friendlier
    /// "or-else" a reader might guess `<<` falls back to at the boundary,
    /// and is rejected on the same principle from the other side: this
    /// language's numeric behaviour is decided once, in §2.1, and an
    /// operator does not get a second, unwritten rule for its edge because a
    /// trap felt like the wrong tone. `division_check`'s own note makes the
    /// general case: checking one edge and not the other, or answering with
    /// a comfortable guess instead of a panic, is a half-measure and not a
    /// correct compiler.
    ///
    /// **One test, or two, and never three.** A shift has one operand that
    /// can be out of range — division has two, the divisor and the
    /// numerator together — so there is no analogue here of `division_check`'s
    /// `Int.min / -1`. The amount is tested against the width always, and
    /// against zero only when it is signed: an unsigned amount has no
    /// negative to catch, and `division_check`'s own reason for skipping its
    /// second test on unsigned division is this one's reason too.
    ///
    /// **Both tests are bit-pattern comparisons at the amount's own
    /// declared width and signedness, not a cast to a common width.**
    /// `division_check`'s tests take this shape because `==`/`!=` do not read
    /// signedness; this one's take it because the amount and the bound being
    /// compared are already the *same* type — unlike [`Builder::bounds_check`],
    /// where an index and a length start as two different types and a cast
    /// to `U64` is what makes one comparison do the work of two. A shift's
    /// amount is never a different type from the value it shifts (§4.6: no
    /// operator dispatch, one structural unification), so there is no second
    /// type here to cast either side to.
    ///
    /// **Why an ordering, unsigned, cannot do both this comparison's job and
    /// negative's in one test the way `bounds_check`'s does.** `bounds_check`
    /// reads a negative index and an over-long one with a single unsigned
    /// `<`, because casting the index to `U64` first turns a negative bit
    /// pattern into a huge one on the *same* side of the comparison it was
    /// already on. Doing that here would mean comparing the amount's bits as
    /// unsigned while the language's own comparison operators read
    /// signedness off the operand's *declared* type — `science-codegen-llvm`'s
    /// `lower_binary` does exactly that — so asking an `I64` amount an
    /// unsigned question needs an operator this IR does not have. Two
    /// ordinary signed comparisons read like every other comparison this
    /// file emits and need nothing new.
    fn shift_check(
        &mut self,
        lhs: Operand,
        rhs: Operand,
        ty: Ty,
        block: BlockId,
        span: Span,
    ) -> (Operand, Operand, BlockId) {
        let ty = self.stripped(ty);
        let Some((bits, signed)) = self.integer_width(ty) else { return (lhs, rhs, block) };
        let Some(bool_ty) = self.context.decls.prelude().ty(self.context.types, "Bool") else {
            return (lhs, rhs, block);
        };

        // Both operands into temporaries, for `division_check`'s reason: the
        // amount is read more than once and an `Operand` handed in may be a
        // `Move` out of a place, readable exactly once.
        let value = self.temp(ty, span, block);
        self.assign(block, Place::local(value), Rvalue::Use(lhs), span);
        let amount = self.temp(ty, span, block);
        self.assign(block, Place::local(amount), Rvalue::Use(rhs), span);

        let mut block = block;
        let trap = self.new_block();

        if signed {
            // `amount < 0`.
            let negative = self.temp(bool_ty, span, block);
            self.assign(
                block,
                Place::local(negative),
                Rvalue::Binary {
                    op: BinaryOp::Lt,
                    lhs: Operand::Copy(Place::local(amount)),
                    rhs: Self::bits(0),
                },
                span,
            );
            let check_width = self.new_block();
            self.terminate(
                block,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(negative)),
                    then_block: trap,
                    else_block: check_width,
                },
                span,
            );
            block = check_width;
        }

        // `amount >= width`. One shared trap block for both tests: unlike
        // `division_check`'s two edges, which are two different failures
        // worth naming separately, a negative amount and an over-width one
        // are the same fact read off the same bits, and the message says so.
        let too_wide = self.temp(bool_ty, span, block);
        self.assign(
            block,
            Place::local(too_wide),
            Rvalue::Binary {
                op: BinaryOp::Ge,
                lhs: Operand::Copy(Place::local(amount)),
                rhs: Self::bits(u128::from(bits)),
            },
            span,
        );
        let in_range = self.new_block();
        self.terminate(
            block,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(too_wide)),
                then_block: trap,
                else_block: in_range,
            },
            span,
        );
        self.panic_with(trap, "shift amount out of range", span);

        (Operand::Copy(Place::local(value)), Operand::Copy(Place::local(amount)), in_range)
    }

    /// An integer constant written as the bits it is, at whatever width the
    /// other operand of the comparison has.
    ///
    /// **A [`Literal`] and not a [`Constant::Count`]**, although this number is
    /// one the lowering computed rather than one the author wrote, which is
    /// `Count`'s whole distinction. `Count` carries the second half of that
    /// claim too — *"its width is the parameter's and never `Int`'s"*, a
    /// `usize`, which `dump` prints as `0usize`. The constant here has the
    /// width of the value it is compared against, which is an `I8` as readily
    /// as an `I64`, so `usize` would be the wrong sentence in the dump and the
    /// wrong one for a consumer that believed it. `science-codegen-llvm`
    /// materialises a literal at the layout of the expression it sits in, which
    /// is exactly the behaviour these bit patterns need.
    fn bits(value: u128) -> Operand {
        Operand::Const(Constant::Literal(Literal::Int {
            value,
            base: IntBase::Dec,
            suffix: None,
        }))
    }

    /// Terminates `block` with a diverging `science_panic_bytes(message)`.
    ///
    /// The destination is a fresh unit local and the target is `None`, which is
    /// how [`Builder::bounds_check`] spells the same thing: the callee returns
    /// `Never`, so there is no value and no successor, and a
    /// [`TerminatorKind::Call`] has a destination whether or not there is
    /// anything to put in it.
    fn panic_with(&mut self, block: BlockId, message: &str, span: Span) {
        let discard = Place::local(self.push_local(Ty::UNIT, LocalKind::Temp, span));
        self.terminate(
            block,
            TerminatorKind::Call {
                callee: Callee::Runtime(PANIC_BYTES),
                args: vec![Operand::Const(Constant::Literal(Literal::Str(
                    message.to_string(),
                )))],
                destination: discard,
                target: None,
            },
            span,
        );
    }

    /// The width in bits of an integer type, and whether it is signed.
    ///
    /// `None` for everything else — a `Float`, a record, a type this
    /// [`Declarations`]' prelude has no name for — which is what leaves those
    /// divisions unguarded. `Int` is `I64`: §2.1 says so, and
    /// [`Builder::push_of`] already reads the two as one type.
    fn integer_width(&mut self, ty: Ty) -> Option<(u32, bool)> {
        const WIDTHS: &[(&str, u32, bool)] = &[
            ("Int", 64, true),
            ("I64", 64, true),
            ("I32", 32, true),
            ("I16", 16, true),
            ("I8", 8, true),
            ("U64", 64, false),
            ("U32", 32, false),
            ("U16", 16, false),
            ("U8", 8, false),
        ];
        let ty = self.stripped(ty);
        let prelude = self.context.decls.prelude();
        let types = &*self.context.types;
        WIDTHS
            .iter()
            .find(|(name, _, _)| prelude.is(types, ty, name))
            .map(|(_, bits, signed)| (*bits, *signed))
    }


    /// `for i in a..b:` as the counting loop AMENDMENT 14 describes.
    ///
    /// **Both ends are evaluated once, before the loop.** `for i in 0..xs.
    /// length():` must not call `length()` on every turn, and more than
    /// performance rests on it: an end that is re-evaluated is a different
    /// loop from the one the author wrote whenever the expression can change.
    ///
    /// **The comparison is signed and follows the spelling.** `a..b` stops
    /// before `b` and `a..=b` includes it, which is the only thing
    /// `inclusive` decides. Signed because the ends are the author's own
    /// values and an `Int` is `I64`: `for i in -3..0:` is an ordinary loop,
    /// where `bounds_check`'s unsigned comparison one construct over is about
    /// an index that must not be negative at all.
    ///
    /// **A range that is already empty runs zero times**, because the test is
    /// at the head. `for i in 5..5:` and `for i in 5..0:` both fall straight
    /// to the exit.
    #[allow(clippy::too_many_arguments)]
    fn lower_for_over_range(
        &mut self,
        dest: Place,
        pattern: PatId,
        start: ExprId,
        end: ExprId,
        inclusive: bool,
        body: thir::BlockId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let element_ty = self.thir.pat(pattern).ty;

        // Both ends into temporaries of the loop variable's own type, so the
        // comparison and the increment are all at one width.
        let low = self.temp(element_ty, span, block);
        block = self.expr_into(Place::local(low), start, block);
        let high = self.temp(element_ty, span, block);
        block = self.expr_into(Place::local(high), end, block);

        let cursor = self.temp(element_ty, span, block);
        self.assign(
            block,
            Place::local(cursor),
            Rvalue::Use(Operand::Copy(Place::local(low))),
            span,
        );

        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        let more = self.temp(self.bool_ty, span, head);
        self.assign(
            head,
            Place::local(more),
            Rvalue::Binary {
                op: if inclusive { BinaryOp::Le } else { BinaryOp::Lt },
                lhs: Operand::Copy(Place::local(cursor)),
                rhs: Operand::Copy(Place::local(high)),
            },
            span,
        );
        self.terminate(
            head,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(more)),
                then_block: body_block,
                else_block: exit,
            },
            span,
        );

        // The increment's own block, so that the body's fall-through and every
        // `continue` both reach it. See `LoopScope::continue_to`.
        let step = self.new_block();
        self.loops.push(LoopScope { head, continue_to: step, exit, depth: self.scopes.len() });
        self.push_scope();
        // The binding is a fresh local holding the cursor's *value*, not the
        // cursor: §4.1's `Item is T`, and a body that shadows or rebinds it
        // must not move the loop's own counter.
        let bound_to = self.temp(element_ty, span, body_block);
        self.assign(
            body_block,
            Place::local(bound_to),
            Rvalue::Use(Operand::Copy(Place::local(cursor))),
            span,
        );
        let mut after = self.bind_pattern(&Place::local(bound_to), pattern, body_block);
        let discard = self.temp(Ty::UNIT, span, after);
        after = self.lower_block(Place::local(discard), body, after);
        after = self.pop_scope(after, span);
        self.terminate(after, TerminatorKind::Goto { target: step }, span);
        let stepped = self.temp(element_ty, span, step);
        let one = Self::bits(1);
        self.assign(
            step,
            Place::local(stepped),
            Rvalue::Binary { op: BinaryOp::Add, lhs: Operand::Copy(Place::local(cursor)), rhs: one },
            span,
        );
        self.assign(
            step,
            Place::local(cursor),
            Rvalue::Use(Operand::Copy(Place::local(stepped))),
            span,
        );
        self.terminate(step, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();

        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    /// Whether a `for` over a value of this type walks a `Map` or a `Set` —
    /// directly or through any number of borrows.
    fn iterates_in_insertion_order(&mut self, ty: Ty) -> bool {
        let mut ty = self.revealed(ty);
        while let TyKind::Borrowed { inner, .. } = *self.context.types.kind(ty) {
            ty = self.revealed(inner);
        }
        let TyKind::Named { def, args } = self.context.types.kind(ty) else { return false };
        let (def, arity) = (*def, args.len());
        let prelude = self.context.decls.prelude();
        (prelude.get("Map") == Some(def) && arity == 2)
            || (prelude.get("Set") == Some(def) && arity == 1)
    }

    /// `for entry in counts:` over a `Map[K, V]`, and `for x in set:` over a
    /// `Set[T]`, in insertion order.
    ///
    /// # The decision
    ///
    /// **The array loop, with the index replaced by a runtime accessor and a
    /// hole test added.** One shared borrow of the map, taken before the loop
    /// and live across every turn; the extent — `science_map_extent`, every
    /// entry position written since the last rebuild — read once; a cursor
    /// that is a local of this loop; and on every turn
    /// `science_map_entry_at(&map, cursor)`, whose answer is the item as an
    /// `Item?`: null for a removed entry's hole, which the loop skips, and
    /// otherwise the `&Entry[K, V]` — or the `&T`, for a set — that the
    /// pattern binds.
    ///
    /// # The reason
    ///
    /// `collections-and-chains.md` §5.2's AMENDMENT 9 fixes the order, §4.2's
    /// AMENDMENT 11 fixes the borrow — `for x in xs:` is `xs.iterate()`, which
    /// borrows shared — and a `Map`, like an `Array`, has nowhere to keep a
    /// cursor, so [`Builder::lower_for_over_array`]'s argument for a lowering
    /// of its own applies unchanged.
    ///
    /// **The item is reached through the loop's loan, and that is what makes
    /// this sound rather than merely working.** The accessor is a
    /// [`Callee::Runtime`], which `science-regions` treats as opaque: its
    /// result may point into anything its arguments reach. The loan is one of
    /// the arguments on every turn, so (1) the loan is live for the whole loop
    /// — `for e in m: m.insert(…)` conflicts with it by rule 4, and is
    /// refused rather than left to reallocate the entries under the item —
    /// and (2) an item stored somewhere that outlives the loop keeps the loan
    /// alive with it, which is the array case's `(*reference)[at]` projection
    /// reached by a different road.
    ///
    /// **Mutation during iteration is refused, not defined.** The note does
    /// not say what inserting into a map one is walking means, and the only
    /// answers a table can give are Python's runtime error or an order that
    /// depends on when the insertion happened. The borrow makes it a compile
    /// error instead, which is the same answer `Array` gives.
    ///
    /// # The cost
    ///
    /// A call per turn where the array loop has a projection, and a turn per
    /// removed entry since the last rebuild. Both are bounded by the table's
    /// own load factor: holes are at most a quarter of the slot count before a
    /// rebuild compacts them.
    fn lower_for_over_map(
        &mut self,
        dest: Place,
        pattern: PatId,
        map: Place,
        body: thir::BlockId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };

        // The loop's loan, for `lower_for_over_array`'s reason: taken once,
        // with `in_argument` false, and copied — never consumed — by every
        // call below.
        let map_ty = self.place_ty(&map);
        let borrowed = self.context.types.borrowed(false, map_ty);
        let reference = self.temp(borrowed, span, block);
        block = self.borrow_place(Place::local(reference), false, map, block, span, false);
        let extent = self.temp(int_ty, span, block);
        block = self.emit_call(
            Place::local(extent),
            Callee::Runtime(MAP_EXTENT),
            vec![Operand::Copy(Place::local(reference))],
            block,
            span,
        );

        let cursor = self.temp(int_ty, span, block);
        self.assign(block, Place::local(cursor), Rvalue::Use(Self::bits(0)), span);

        // The item, and its `Item?` slot, outside the loop for `lower_loop`'s
        // reason.
        let element_ty = self.thir.pat(pattern).ty;
        let slot_ty = self.context.types.nullable(element_ty);
        let slot = self.temp(slot_ty, span, block);
        let present = self.temp(self.bool_ty, span, block);

        let head = self.new_block();
        let fetch = self.new_block();
        let body_block = self.new_block();
        let step = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        // The cursor never goes below zero, so the signed comparison is the
        // same question the array loop's unsigned one asks.
        let more = self.temp(self.bool_ty, span, head);
        self.assign(
            head,
            Place::local(more),
            Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Operand::Copy(Place::local(cursor)),
                rhs: Operand::Copy(Place::local(extent)),
            },
            span,
        );
        self.terminate(
            head,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(more)),
                then_block: fetch,
                else_block: exit,
            },
            span,
        );

        // A position the loop body cannot reassign: the cursor is written
        // twice, and a single-assignment copy says which entry this turn is.
        let at = self.temp(int_ty, span, fetch);
        self.assign(fetch, Place::local(at), Rvalue::Use(Operand::Copy(Place::local(cursor))), span);
        let fetched = self.emit_call(
            Place::local(slot),
            Callee::Runtime(MAP_ENTRY_AT),
            vec![Operand::Copy(Place::local(reference)), Operand::Copy(Place::local(at))],
            fetch,
            span,
        );
        self.assign(
            fetched,
            Place::local(present),
            Rvalue::IsPresent(Operand::Copy(Place::local(slot))),
            span,
        );
        // A hole goes straight to the increment.
        self.terminate(
            fetched,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(present)),
                then_block: body_block,
                else_block: step,
            },
            span,
        );

        self.loops.push(LoopScope { head, continue_to: step, exit, depth: self.scopes.len() });
        self.push_scope();
        // The general path's narrowing. The item is a borrow, so the read is
        // a copy and nothing here owns anything.
        let narrowed = self.temp(element_ty, span, body_block);
        let read = self.read(Place::local(slot), element_ty);
        self.assign(
            body_block,
            Place::local(narrowed),
            Rvalue::Narrow { operand: read, ty: element_ty },
            span,
        );
        let mut after = self.bind_pattern(&Place::local(narrowed), pattern, body_block);
        let discard = self.temp(Ty::UNIT, span, after);
        after = self.lower_block(Place::local(discard), body, after);
        after = self.pop_scope(after, span);
        self.terminate(after, TerminatorKind::Goto { target: step }, span);

        let stepped = self.temp(int_ty, span, step);
        self.assign(
            step,
            Place::local(stepped),
            Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Operand::Copy(Place::local(cursor)),
                rhs: Self::bits(1),
            },
            span,
        );
        self.assign(
            step,
            Place::local(cursor),
            Rvalue::Use(Operand::Copy(Place::local(stepped))),
            span,
        );
        self.terminate(step, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();

        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    /// The loop subject as an array place, when it is one.
    ///
    /// Evaluated through `borrow_source` for its reason: a subject with no
    /// place of its own lands in a temporary with a storage-dead point rather
    /// than in a special case here.
    fn array_subject(
        &mut self,
        iter: ExprId,
        block: BlockId,
        span: Span,
    ) -> Option<(Place, BlockId)> {
        let (place, next) = self.loop_source(iter, block, span);
        self.is_array(&place).then_some((place, next))
    }

    /// The place a `for` walks: the subject evaluated, then dereferenced as
    /// far as the *subject's* type says — the hole's, not only the place's.
    ///
    /// **Decision. [`Builder::deref_to_hole`] runs after
    /// [`Builder::auto_deref`] here, as it does for a method receiver.** A
    /// subject narrowed out of a `(&T)?` — `for x in found:` inside `if
    /// found?:`, where `found` is a `Map.get` — is the same storage as the
    /// nullable, so its place is still written `(&T)?` while the subject is a
    /// `&T`. `auto_deref` stops at the nullable, and both containers were
    /// refused as `SC0400`: an `Array` was not recognised as one and fell
    /// through to the general path — *"no `Iterate` implementation this
    /// compiler can name"*, for a type that has one — and a `Map` took its own
    /// path with the nullable as the map, which the backend's
    /// `science_map_entry_at` could read no key and value type off.
    ///
    /// **Reason.** A narrowed borrow is a pointer behind a niche (Decision
    /// 19): the payload *is* the slot, so one `Deref` reaches the referent,
    /// exactly as it does for `a.length()` in the same position.
    fn loop_source(&mut self, iter: ExprId, block: BlockId, span: Span) -> (Place, BlockId) {
        let (place, next) = self.borrow_source(iter, block, span);
        let place = self.auto_deref(place);
        (self.deref_to_hole(place, iter), next)
    }

    /// `for x in xs:` as the indexed loop AMENDMENT 11's `xs.iterate()` would
    /// compile to.
    ///
    /// **The comparison is unsigned and the bound is read once.** `i` starts
    /// at zero and only ever grows, so `i < len` as `U64` is the same question
    /// `bounds_check` asks one construct over, asked with the same
    /// instruction. Reading `length()` outside the loop is correct here where
    /// it would not be in general, because the borrow this loop holds is
    /// shared and nothing inside it can push.
    ///
    /// **The binding is a borrow, and that is the note's decision rather than
    /// this crate's.** §4.3 says `iterate()` yields `borrowed Item`
    /// *"uniformly"*, and §4's own argument is that the alternative copies
    /// every element of every loop — for an `Array[Array[F64]]` a heap
    /// allocation per row per turn. So the element place is borrowed into the
    /// pattern rather than read out of it.
    fn lower_for_over_array(
        &mut self,
        dest: Place,
        pattern: PatId,
        array: Place,
        body: thir::BlockId,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        let Some(u64_ty) = self.context.decls.prelude().ty(self.context.types, "U64") else {
            return block;
        };
        let element_ty = self.element_ty(&array);

        // **§7.1's loop borrow, and it is load-bearing rather than
        // decorative.** The loan is taken before the loop with `in_argument`
        // false, so it lives across every turn, and the length call *copies*
        // it rather than consuming it — a shared borrow is `Copy`.
        //
        // The first version of this took the borrow for the length call and
        // let it die there, on the reasoning that the element projection would
        // carry its own loan per turn. That is a **soundness hole**, and
        // `science-regions`' `a_for_over_a_collection_the_body_mutates_is_
        // refused` caught it: with no loan spanning the loop,
        // `for x in xs: xs.push(1)` was accepted, and a push that reallocates
        // the buffer leaves the element reference pointing at freed memory.
        // §7.1's borrow is what makes the body's write conflict, and it has to
        // outlive the header to do it.
        let array_ty = self.place_ty(&array);
        let borrowed = self.context.types.borrowed(false, array_ty);
        let reference = self.temp(borrowed, span, block);
        block =
            self.borrow_place(Place::local(reference), false, array.clone(), block, span, false);
        let length = self.temp(int_ty, span, block);
        block = self.emit_call(
            Place::local(length),
            Callee::Runtime(ARRAY_LEN),
            vec![Operand::Copy(Place::local(reference))],
            block,
            span,
        );

        let cursor = self.temp(int_ty, span, block);
        let zero = Self::bits(0);
        self.assign(block, Place::local(cursor), Rvalue::Use(zero), span);

        let head = self.new_block();
        let body_block = self.new_block();
        let exit = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        let wide_cursor = self.temp(u64_ty, span, head);
        self.assign(
            head,
            Place::local(wide_cursor),
            Rvalue::Cast { operand: Operand::Copy(Place::local(cursor)), from: int_ty, ty: u64_ty },
            span,
        );
        let wide_length = self.temp(u64_ty, span, head);
        self.assign(
            head,
            Place::local(wide_length),
            Rvalue::Cast { operand: Operand::Copy(Place::local(length)), from: int_ty, ty: u64_ty },
            span,
        );
        let more = self.temp(self.bool_ty, span, head);
        self.assign(
            head,
            Place::local(more),
            Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Operand::Copy(Place::local(wide_cursor)),
                rhs: Operand::Copy(Place::local(wide_length)),
            },
            span,
        );
        self.terminate(
            head,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(more)),
                then_block: body_block,
                else_block: exit,
            },
            span,
        );

        // The increment's own block, for `LoopScope::continue_to`'s reason.
        let step = self.new_block();
        self.loops.push(LoopScope { head, continue_to: step, exit, depth: self.scopes.len() });
        self.push_scope();
        // **The projection's index is a body-local temporary, not the
        // cursor.** `Projection::Index`'s own note says the temporary is
        // *"assigned exactly once and never reassigned … which is what makes
        // structural equality on this variant mean the same element rather
        // than the same spelling"*, and `Body::index_temps_are_single_
        // assignment` checks it. A loop's cursor is assigned twice — once at
        // zero and once by the increment — so projecting through it would make
        // two different elements compare equal, and the acceptance test caught
        // exactly that. Copying it into a temporary the body writes once says
        // what is true: within one turn, this names one element.
        let at = self.temp(int_ty, span, body_block);
        self.assign(
            body_block,
            Place::local(at),
            Rvalue::Use(Operand::Copy(Place::local(cursor))),
            span,
        );
        // **The element is reached *through* the loop's loan, not beside it.**
        //
        // `(*reference)[at]` rather than `xs[at]`, and the difference is the
        // whole of whether `for x in xs: xs.push(1)` is refused. A loan whose
        // last use is the length call before the loop is *dead* by the time
        // the body runs — region inference computes liveness, not scope — so
        // the push conflicted with nothing and a reallocation would have left
        // the element reference dangling. Reading the loan on every turn is
        // what the general path gets for free from calling `next` through it,
        // and projecting through it here buys the same thing without a call
        // per iteration.
        let through = Place::local(reference).project(Projection::Deref { ty: array_ty });
        let element = through.project(Projection::Index { index: at, ty: element_ty });
        let binding_ty = self.context.types.borrowed(false, element_ty);
        let bound_to = self.temp(binding_ty, span, body_block);
        let mut after =
            self.borrow_place(Place::local(bound_to), false, element, body_block, span, false);
        after = self.bind_pattern(&Place::local(bound_to), pattern, after);
        let discard = self.temp(Ty::UNIT, span, after);
        after = self.lower_block(Place::local(discard), body, after);
        after = self.pop_scope(after, span);
        self.terminate(after, TerminatorKind::Goto { target: step }, span);
        let stepped = self.temp(int_ty, span, step);
        let one = Self::bits(1);
        self.assign(
            step,
            Place::local(stepped),
            Rvalue::Binary { op: BinaryOp::Add, lhs: Operand::Copy(Place::local(cursor)), rhs: one },
            span,
        );
        self.assign(
            step,
            Place::local(cursor),
            Rvalue::Use(Operand::Copy(Place::local(stepped))),
            span,
        );
        self.terminate(step, TerminatorKind::Goto { target: head }, span);
        self.loops.pop();

        self.assign(exit, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        exit
    }

    /// Whether a place is an `Array of T`, which is what [`Builder::bounds_check`]
    /// has an entry point for.
    fn is_array(&mut self, place: &Place) -> bool {
        let written = self.place_ty(place);
        let ty = self.revealed(written);
        let TyKind::Named { def, args } = self.context.types.kind(ty) else { return false };
        // `Prelude::is` is no help: it answers for a primitive, and requires
        // the arguments to be empty, which `Array of T` never is.
        let Some(array) = self.context.decls.prelude().get("Array") else { return false };
        *def == array && args.len() == 1
    }

    /// An array literal's element, **consumed** into a slot the push takes the
    /// address of.
    ///
    /// # The decision
    ///
    /// The element is evaluated into a temporary of its own and handed over as
    /// `Operand::Move`, not as a borrow.
    ///
    /// # The reason, which is an ownership fact about the runtime
    ///
    /// `science_array_push(array, info, value)` copies `info.size` bytes out of
    /// `value` into the buffer and the array owns them from then on — it is a
    /// **move** through a pointer, which is exactly what
    /// `science_codegen::abi`'s indirect-argument rule already means by one.
    /// This was written as [`Builder::borrow_hole`] first, which is the same
    /// address and the opposite ownership: `["a", "bb"]` pushed each temporary
    /// `String`, then dropped the temporary at the end of its statement, and
    /// `science_array_free` freed the same buffers a second time. The program
    /// printed its length correctly and trapped on the way out, which is the
    /// shape of defect that a length assertion cannot see.
    ///
    /// Spelling it as a move is not decoration: it is what makes MIR's own
    /// move analysis stop emitting the drop, so the fix is one MIR fact rather
    /// than a suppression somewhere below.
    ///
    /// # The cost
    ///
    /// One temporary per element, where a borrow of an existing place would
    /// have needed none. A copy type does not care — `[1, 2, 3]` stores three
    /// integers into three slots the optimiser folds away — and an owning
    /// element has to be moved somewhere anyway.
    fn element_move(&mut self, element: ExprId, block: BlockId, span: Span) -> (Operand, BlockId) {
        let ty = self.thir.expr(element).ty;
        let temp = self.temp(ty, span, block);
        let block = self.expr_into(Place::local(temp), element, block);
        (Operand::Move(Place::local(temp)), block)
    }

    /// Whether a method's owner is a type's block rather than an `interface`'s.
    ///
    /// The question `lower_for` needs answered is *"does this `next` have a
    /// body"*, and the honest proxy for it is *"was it declared on the
    /// implementor"*: an `interface` block's method is a declaration or a
    /// default, and a default needs monomorphising before it is a callee
    /// anything can emit. `Signature::owner` is the block, and only an
    /// `interface` block has [`DefKind::Interface`].
    fn owner_is_a_type(&self, def: DefId) -> bool {
        let Some(owner) = self.context.decls.signature(def).and_then(|s| s.owner) else {
            return false;
        };
        self.context.defs.get(owner).kind != DefKind::Interface
    }

    /// Whether this hole is a narrow whose option is laid out as a
    /// discriminant and a payload rather than as a niche.
    ///
    /// The question is asked of *types* and not of layouts, because layout is
    /// `science-codegen`'s and this crate must not learn it: a `T?` gets
    /// Decision 19's niche exactly when `T` is a borrow, which is a fact about
    /// the type. Anything else is Decision 18's tagged pair, where the place
    /// and the value are not the same bytes.
    fn narrowed_payload_is_not_a_borrow(&mut self, place: &Place, hole: ExprId) -> bool {
        let hole_ty = self.revealed(self.thir.expr(hole).ty);
        if matches!(self.context.types.kind(hole_ty), TyKind::Borrowed { .. }) {
            return false;
        }
        let written = self.revealed(self.place_ty(place));
        let TyKind::Nullable(payload) = *self.context.types.kind(written) else { return false };
        let payload = self.revealed(payload);
        !matches!(self.context.types.kind(payload), TyKind::Borrowed { .. })
    }

    /// §4's dereference, applied against the **hole's** type and not only the
    /// place's, because a narrow is the one case where the two disagree.
    ///
    /// # The decision
    ///
    /// After [`Builder::auto_deref`] has taken the place as far as its own
    /// written type says, one more `Deref` is projected if the *hole* is a
    /// borrow and the place is not.
    ///
    /// # The reason
    ///
    /// [`Builder::as_place`] sees through `ExprKind::Narrow` on the stated
    /// ground that *"a narrowed read is the same storage seen at a smaller
    /// type, so it is the same place"*. That is true, and it means the place
    /// behind `f"{found}"` inside `if found?:` is still written `(borrowed
    /// I64)?` while the hole itself is a `borrowed I64`. `auto_deref` matches
    /// on the place's type, sees a nullable rather than a borrow, and stops —
    /// so the pointer reached the entry point in place of the referent.
    ///
    /// **It was a different defect in each of the two callers, from one cause.**
    /// [`Builder::value_hole`] handed `science_string_push_i64` a pointer where
    /// it declares an `i64`, which `LLVMVerifyModule` rejects only because the
    /// two happen to be different LLVM types — a referent of pointer width
    /// would have printed an address and verified. [`Builder::borrow_hole`]
    /// reborrowed the *slot* rather than the string, so
    /// `science_string_push_str` read a `ScienceString` out of the address of a
    /// pointer to one and took SIGBUS on the length field. Both are fixed here
    /// rather than twice, because a second copy is a second thing to get wrong.
    ///
    /// # The cost
    ///
    /// It trusts the hole's type over the place's, which is the right way round
    /// for a narrow and would be the wrong way round for a coercion — a coerced
    /// value is a new value in a new representation, and `as_place` already
    /// declines to see through one for exactly that reason. So this is sound
    /// only as long as that stays true, and it is named here so the day it
    /// changes there is something to find.
    fn deref_to_hole(&mut self, place: Place, hole: ExprId) -> Place {
        let hole_ty = self.revealed(self.thir.expr(hole).ty);
        if !matches!(self.context.types.kind(hole_ty), TyKind::Borrowed { .. }) {
            return place;
        }
        // **The place must still be the nullable, and "not a borrow" is not the
        // same test.** `auto_deref` ends on the referent by design — a
        // `borrowed Doc` receiver comes back as `(*_1)`, of type `Doc`, which is
        // not a borrow and is already right. Keying on that alone projected a
        // second deref onto every such receiver and gave `(*(*_1))`, which
        // `science-mir`'s own `places.rs` catches. What distinguishes the narrow
        // is that the place is a `T?` whose payload is the borrow: the storage
        // holds a pointer, the hole names the referent, and nothing has stepped
        // through it yet.
        let written = self.revealed(self.place_ty(&place));
        let TyKind::Nullable(payload) = *self.context.types.kind(written) else {
            return place;
        };
        let payload = self.revealed(payload);
        let TyKind::Borrowed { inner, .. } = *self.context.types.kind(payload) else {
            return place;
        };
        place.project(Projection::Deref { ty: inner })
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
                // **A narrow is the same place only when the payload is a
                // borrow, and this is where that stops being true.**
                //
                // `as_place` sees through `ExprKind::Narrow` because *"a
                // narrowed read is the same storage seen at a smaller type"*.
                // For a `(&T)?` that is exact: Decision 19 puts the niche in
                // the pointer, so the option and the payload are the same word
                // and `deref_to_hole` steps through it. For an `I64?` it is
                // false — Decision 18 lays that out as a discriminant *and* a
                // payload, two words where the payload is one — so the place
                // is not the value at any offset the hole knows about.
                //
                // The value path is always right: `Builder::operand` emits
                // `Rvalue::Narrow`, which is the representation change stated
                // as a statement, and codegen lowers it. So a hole whose
                // narrowed payload is not a borrow takes that path instead.
                //
                // Found by `f"{old}"` where `old` came from `Map.insert`'s
                // `V?`: the whole two-word option reached
                // `science_string_push_i64`, which the verifier caught only
                // because `[2 x i64]` and `i64` are different LLVM types.
                if self.narrowed_payload_is_not_a_borrow(&place, hole) {
                    // **`element_move` and not `operand`**, and the difference
                    // is the whole of why the first attempt at this changed
                    // nothing: `Builder::operand` asks `as_place` too, so it
                    // hands back the very place this arm is trying to get away
                    // from. Evaluating into a temporary is what forces
                    // `expr_into`, which lowers the `ExprKind::Narrow` to the
                    // `Rvalue::Narrow` that states the representation change.
                    let span = self.thir.expr(hole).span;
                    return self.element_move(hole, block, span);
                }
                let place = self.auto_deref(place);
                let place = self.deref_to_hole(place, hole);
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
        // `borrow_source` already answers §1.6's own hazard — a narrowed
        // payload that is not a borrow has no place to borrow without one
        // more step — for every one of its callers and not just this one; see
        // its own doc for the argument. What is left here is §9's ordinary
        // reborrow, unchanged.
        let (place, block) = self.borrow_source(hole, block, span);
        let place = self.auto_deref(place);
        let place = self.deref_to_hole(place, hole);
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
    /// `science-codegen`'s `RUNTIME` names the entry points that exist, and a
    /// type with none is [`Push::Missing`].
    ///
    /// **`IoError` and `TextError` are the eighth and ninth, by pointer.**
    /// *Decision:* a hole of either type is `science_string_push_io_error` or
    /// `science_string_push_text_error`, taking the hole by shared borrow as a
    /// `String` hole is taken, and appending the sentence the error's
    /// `message()` returns — `science-rt` reads both from one table. *Reason:*
    /// the prelude lists both as implementing `Display` and neither has a
    /// `display` with a body, so `print(err)` and `f"{err}"` were refused
    /// while `err.message()` beside them built; a push is the builder's one
    /// mechanism, so `print`, `write`, `print_error`, `write_error` and the
    /// f-string all follow without a line of their own, where composing
    /// `message()`, `push_str` and a free here would have been three calls and
    /// an allocation per hole. *Cost:* two symbols in `RUNTIME` and its count
    /// tests. A format specification (`{err:>20}`) needs nothing more:
    /// [`Builder::render_with_spec`] renders the error into a scratch `String`
    /// with this same push and pads that, which is the one place width and
    /// alignment are applied to a `Bool`, a `Char` and both error types.
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
        } else if is("F64") || is("Float") {
            // **`Float` is a prelude name of its own, not a spelling of
            // `F64`.** Decision 2 defaults a floating-point literal to `F64`
            // and the prelude also carries `Float` as the name a user writes,
            // exactly as it carries `Int` beside `I64` — and the `Int` row
            // above has always named both while this one named one. So
            // `let x: Float be 2.5` checked, lowered, and was refused at the
            // interpolation as a hole *"of type `Float`"*, which is a sentence
            // about a type the language does have.
            direct(PUSH_F64)
        } else if is("F32") {
            direct(PUSH_F32)
        } else if is("Bool") {
            direct(PUSH_BOOL)
        } else if is("Char") {
            direct(PUSH_CHAR)
        } else if is("String") {
            Push::Pointer(PUSH_STR)
        } else if is("IoError") {
            Push::Pointer(PUSH_IO_ERROR)
        } else if is("TextError") {
            Push::Pointer(PUSH_TEXT_ERROR)
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
    /// | `IoError` | 18 | `input/output error`, the longest of its five sentences |
    /// | `TextError` | 24 | `not a character boundary`, the longest of its four |
    /// | anything else | 0 | there is no renderer, so there will be no bytes |
    ///
    /// The `F64` and `String` numbers are the two worth revisiting with a
    /// measurement; the rest are arithmetic.
    fn capacity_estimate(&mut self, parts: &[thir::FStringPart]) -> u64 {
        let mut total: u64 = 0;
        for part in parts {
            total += match part {
                thir::FStringPart::Text(text) => text.len() as u64,
                // A width is a floor on what the hole writes, so a spec'd hole
                // is the larger of the two. Precision is not: `.2` on a
                // `String` shortens it and `.9` on a float lengthens it, and
                // neither is worth a branch in an estimate.
                thir::FStringPart::Hole(hole, spec) => {
                    let ty = self.thir.ty(*hole);
                    let width = spec.as_ref().and_then(|s| s.width).map_or(0, u64::from);
                    self.hole_estimate(ty).max(width)
                }
            };
        }
        total
    }

    /// One hole's contribution to [`Builder::capacity_estimate`].
    ///
    /// A table rather than a chain of comparisons, because the table is what
    /// the note above is: sixteen rows, each a type and a number, and a reader
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
            ("IoError", 18),
            ("TextError", 24),
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
        params: &[DefId],
        body: ExprId,
        mut block: BlockId,
    ) -> (Vec<Operand>, Vec<CaptureParam>, BlockId) {
        let found = crate::capture::captures_of(self.context.decls, self.thir, params, body);
        let escapes = ty_contains_closure(self.context.types, self.thir.ret());
        let mut captures = Vec::with_capacity(found.len());
        let mut params = Vec::with_capacity(found.len());
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
            // §8.7. A `Copy` capture read by a closure that may leave the
            // function is the value itself, not a loan of this frame's slot.
            if escapes && !mutable && self.is_copy(ty) {
                captures.push(Operand::Copy(place));
                params.push(CaptureParam {
                    def: capture.def,
                    reference_ty: borrowed,
                    referent_ty: ty,
                    span: capture.span,
                    moves: false,
                });
                continue;
            }
            // §8.7 (moving). An owned value that is not `Copy`, only read by a
            // closure that may leave the function, moves into the closure's
            // environment; the closure's destructor releases it. Only a whole
            // local qualifies: a place reached through a borrow or a field is
            // not the closure's to take.
            let revealed = self.revealed(ty);
            if escapes
                && matches!(capture.use_kind, crate::capture::Use::Read)
                && place.projection.is_empty()
                && !matches!(self.context.types.kind(revealed), TyKind::Borrowed { .. })
                && !self.is_copy(ty)
            {
                captures.push(Operand::Move(place));
                params.push(CaptureParam {
                    def: capture.def,
                    reference_ty: borrowed,
                    referent_ty: ty,
                    span: capture.span,
                    moves: false,
                });
                continue;
            }
            let temp = self.temp(borrowed, capture.span, block);
            // §8.4: `in_argument` is false however the closure got here.
            block =
                self.borrow_place(Place::local(temp), mutable, place, block, capture.span, false);
            captures.push(Operand::Move(Place::local(temp)));
            params.push(CaptureParam {
                def: capture.def,
                reference_ty: borrowed,
                referent_ty: ty,
                span: capture.span,
                moves: matches!(capture.use_kind, crate::capture::Use::Consume) && consumed_moves,
            });
        }
        (captures, params, block)
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
        result: Ty,
        block: BlockId,
        span: Span,
        in_argument: bool,
    ) -> BlockId {
        let (place, block) = self.borrow_source(operand, block, span);
        // **`&r` where `r` is already a borrow is a reborrow of what `r` points
        // at.** The checker types it as the single borrow (`&&T` is collapsed:
        // the author wrote one `&` and the language has no pointer to a
        // pointer), but the place is still `r`'s own slot, so borrowing it as
        // it stands builds a pointer to a pointer typed as a pointer to the
        // referent, and the callee reads the slot's address as the value.
        let place = self.collapse_borrow_of_a_borrow(place, operand, result);
        self.borrow_place(dest, mutable, place, block, span, in_argument)
    }

    /// The place `&operand` borrows when `operand` is itself a borrow and the
    /// node's own type is the single borrow: one `Deref` further in.
    ///
    /// Asked of the *place's* type as well as the expression's, because a field
    /// read through a borrow is typed as a borrow (Decision 27) while its place
    /// is the field itself, and that one is borrowed as it stands.
    fn collapse_borrow_of_a_borrow(&mut self, place: Place, operand: ExprId, result: Ty) -> Place {
        let result = self.revealed(result);
        let TyKind::Borrowed { inner: referent, .. } = *self.context.types.kind(result) else {
            return place;
        };
        let written = self.revealed(self.thir.expr(operand).ty);
        let TyKind::Borrowed { inner: held, .. } = *self.context.types.kind(written) else {
            return place;
        };
        if held != referent {
            return place;
        }
        let place_ty = self.revealed(self.place_ty(&place));
        if matches!(self.context.types.kind(place_ty), TyKind::Borrowed { .. }) {
            return place.project(Projection::Deref { ty: held });
        }
        // A narrowed `(&T)?` is still written as the nullable.
        self.deref_to_hole(place, operand)
    }

    /// The place a borrow of this expression is taken *from*.
    ///
    /// Split out of [`Builder::lower_borrow`] because §9's receiver reborrow
    /// needs the place before the borrow is taken, to project through it.
    ///
    /// **Every caller wants a place to borrow, and a narrowed owning payload
    /// has none without one more step.** `as_place` sees through
    /// `ExprKind::Narrow` on the ground that a narrowed read is the same
    /// storage seen at a smaller type — exact for Decision 19's niche, where
    /// the option and its payload are the same word, and false for Decision
    /// 18's tagged pair, where the payload sits behind a discriminant at its
    /// own offset. Handing that place to `borrow_place` unchanged borrows the
    /// option's own bytes — tag included — and reads them back as the
    /// payload's, which is `print(s)`'s segfault inside `if s?:` for a
    /// `String?` and a silent wrong answer for anything the verifier does not
    /// happen to catch by width.
    ///
    /// [`Projection::Payload`] is the repair, and it belongs in
    /// [`Builder::narrowed_place`] rather than in each caller: `lower_borrow`'s
    /// ordinary `&expr` — which is what `print(s)`'s call-site auto-borrow
    /// lowers to, `AGENTS.md` §4's "borrows are automatic at call sites" — a
    /// method receiver (line ~2241), `borrow_hole`'s f-string hole, and
    /// [`Builder::lower_match`]'s scrutinee all reach a narrowed option
    /// through a place, and the bug is one bug wherever a place is what a
    /// caller ends up with. `science-codegen`'s `place_address` is where the
    /// offset actually lives — zero for the niche, `payload_offset` for the
    /// tagged pair — the way it already was for `Projection::Downcast`; this
    /// crate only names the step.
    fn borrow_source(&mut self, operand: ExprId, block: BlockId, span: Span) -> (Place, BlockId) {
        match self.as_place(operand, block) {
            Some((place, block)) => (self.narrowed_place(place, operand), block),
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

    /// [`Projection::Payload`], projected onto a place already in hand,
    /// whenever `narrow`'s narrowing is Decision 18's tagged pair rather than
    /// Decision 19's niche.
    ///
    /// **Every caller that wants a place to *address* — rather than a value
    /// to copy — needs this**, and there turned out to be three of them
    /// reached by three different routes: a `print(s)` argument's call-site
    /// auto-borrow, a method receiver, and — the third, found after this
    /// function's first two callers already existed — [`Builder::lower_match`]'s
    /// scrutinee. Before this, `match err:` inside `if err?:` read
    /// `Rvalue::Discriminant` off the *option's* own tag, which is always
    /// "present" from that point on and never the payload's own discriminant,
    /// so every arm but one was unreachable regardless of which variant `err`
    /// actually held. One bug, three call sites, and the reason this is a
    /// function of its own rather than three copies of `borrow_source`'s
    /// former body.
    fn narrowed_place(&mut self, place: Place, narrow: ExprId) -> Place {
        if self.narrowed_payload_is_not_a_borrow(&place, narrow) {
            let payload_ty = self.revealed(self.thir.expr(narrow).ty);
            place.project(Projection::Payload { ty: payload_ty })
        } else {
            place
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

    /// Whether an assignment target reaches its storage through an index,
    /// looking through the projections `as_place` looks through.
    fn target_is_indexed(&self, expr: ExprId) -> bool {
        match &self.thir.expr(expr).kind {
            ExprKind::Index { .. } => true,
            ExprKind::Narrow(inner) => self.target_is_indexed(*inner),
            ExprKind::Field { base, .. } | ExprKind::TupleField { base, .. } => {
                self.target_is_indexed(*base)
            }
            _ => false,
        }
    }

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
                // §8.6. Inside a closure's own body a capture is a parameter
                // holding a *reference*, and every use of it means the
                // referent — so the `Deref` goes in here, at the one place
                // that turns a name into a place, rather than at each of the
                // dozen readers that would each have had to remember.
                if let Some(referent) = self.captured.get(def).copied() {
                    let place = Place::local(local).project(Projection::Deref { ty: referent });
                    return Some((place, block));
                }
                Some((Place::local(local), block))
            }
            // A narrowed read is the same storage seen at a smaller type, so it
            // is the same place. THIR's §3 made exactly this call and MIR keeps
            // it; seeing through a `Coerce` would not, because a coerced value
            // is a new value in a new representation.
            ExprKind::Narrow(operand) => {
                // `&T?` narrowed to `&T`: the payload, where it sits behind
                // the pointer. The place is the payload itself; the node's own
                // type is the borrow of it.
                if let Some(payload) = self.borrowed_nullable_payload(*operand) {
                    let (place, block) = self.as_place(*operand, block)?;
                    let place = self.auto_deref(place);
                    return Some((place.project(Projection::Payload { ty: payload }), block));
                }
                self.as_place(*operand, block)
            }
            // A call's result has no storage until something gives it some —
            // every other arm here recurses to storage that already exists,
            // and a call does not. It is materialised into a temporary the
            // same way `operand`'s fallback already does for any place-less
            // expression: `expr_into` lowers `ExprKind::Call` exactly as it
            // would at the top level, so the call still runs once and its
            // result becomes the temporary's whole value. `Builder::temp`
            // registers that temporary and marks it storage-live the same as
            // any other, so it is dropped exactly once, at its own scope's
            // exit, by the elaboration `drops` already does for every local —
            // a call's result asks that machinery for nothing new.
            //
            // **This arm is deliberately not widened to
            // `ExprKind::MethodCall`, and `text.chars()` is why.** A `None`
            // from here is not only "there is no place"; `borrow_source` reads
            // it as *"this value is the loop's own, so give it a temporary
            // with a storage-dead point"*, and §4.4's shared borrow of a `for`
            // subject is taken only down the place path. Answering `Some` for
            // a method call routes `for c in text.chars():` down that path and
            // the `Chars` temporary the chain produced is then borrowed
            // *shared* — for a `def next(mutable self)`. `iteration.rs`'s
            // `a_subject_with_no_place_is_borrowed_through_a_temporary` and
            // `the_next_call_reads_through_the_loops_reference` both fail on
            // exactly that `Shared`/`Exclusive` swap, and they are right to.
            // A base that genuinely needs storage asks for it where it needs
            // it — see the `Field` and `Index` arms below.
            ExprKind::Call { .. } => {
                let ty = thir.expr(expr).ty;
                let span = thir.expr(expr).span;
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), expr, block);
                Some((Place::local(temp), block))
            }
            ExprKind::Field { base, field } => {
                let field = (*field)?;
                // **A field projection needs storage for its base, so a base
                // that has none of its own is given some here.** The arm above
                // says why a call's result is materialised; `a.scaled(2.0).x`
                // is the same sentence about a *method* call, and `V2(x: 1.0,
                // y: 2.0).x`, `(if c: p else: q).x` and every other
                // place-less base are the same sentence again. Materialising
                // in this arm rather than widening the arm above is what keeps
                // the answer local: `as_place` still says `None` for a method
                // call to every other caller, so `borrow_source` still reads
                // that `None` the way `for c in text.chars():` needs it read.
                //
                // **This cannot regress a program that compiles today**,
                // because every route out of the `?` this replaces ends in a
                // refusal and not in a behaviour: `expr_into`'s
                // `Local | SelfValue | Field | Index` arm emits
                // `Rvalue::Error`, `value_hole` and `borrow_source` fall back
                // to lowering the same `Field` node and reach that same
                // `Rvalue::Error`, and `SC0400` is what the author sees. The
                // only programs this arm can change are the ones the backend
                // refuses.
                let (place, block) = match self.as_place(*base, block) {
                    Some(found) => found,
                    None => {
                        let base_ty = thir.expr(*base).ty;
                        let base_span = thir.expr(*base).span;
                        let temp = self.temp(base_ty, base_span, block);
                        let block = self.expr_into(Place::local(temp), *base, block);
                        (Place::local(temp), block)
                    }
                };
                // The `Narrow` arm above sees a narrowed base through to its
                // storage because that is exact for storage — the same bytes
                // at a smaller type. `record_of` and `field_ty` below ask a
                // different question, though: which record owns this field,
                // and at what type. Reading that off the place's *declared*
                // type recomputes what Decision 25 already concluded on the
                // `Narrow` node itself, and for Decision 18's tagged layout it
                // recomputes it wrong — the place is still the whole `T?`,
                // and `record_of` finds a `Nullable` where it wants a `Named`.
                //
                // `narrowed_payload_is_not_a_borrow` is `value_hole`'s own
                // test for this, reused rather than restated: a niched narrow
                // (Decision 19, a borrowed payload — `Array.get`'s `(&T)?` is
                // the corpus's own example of one) is the same storage at a
                // smaller type, and `auto_deref` plus `deref_to_hole` is
                // `value_hole`'s own pair of steps to reach it: the place is
                // still written `(&T)?`, a `Nullable` and not itself a
                // `Borrowed`, so `auto_deref` alone stops before it, and
                // `deref_to_hole` is the one further step keyed on the
                // *hole's* type rather than the place's.
                //
                // A tagged narrow is not the same storage at all, because the
                // payload sits at an offset inside a `Repr::Tagged` value and
                // `science-codegen-llvm`'s `place_address` only walks a
                // `Field` projection over a `Repr::Aggregate` one — there is
                // no projection for a nullable's payload (`Rvalue::Narrow`'s
                // own tagged arm says so). So the tagged case is materialised
                // instead, the same way a call's result is above: `expr_into`
                // on the `Narrow` node emits the `Rvalue::Narrow` that states
                // the representation change, into a fresh temporary whose
                // declared type *is* Decision 25's narrowed conclusion — read
                // off the node, never recomputed — and the field is
                // projected from there.
                //
                // **Projected, not materialised.** Materialising moved the
                // whole payload out of a non-`Copy` option into a temporary
                // that owned it: `best.tag` inside `if best?:` then dropped the
                // record at the end of the statement, and a second read was a
                // use after move. `Projection::Payload` names the payload
                // where it is, and the field is read from there.
                let (place, block) = if self.narrowed_payload_is_not_a_borrow(&place, *base) {
                    (self.narrowed_place(place, *base), block)
                } else {
                    let place = self.auto_deref(place);
                    let place = self.deref_to_hole(place, *base);
                    (place, block)
                };
                let owner = self.record_of(&place)?;
                let base_ty = self.place_ty(&place);
                let ty = self.field_ty(owner, field, base_ty);
                Some((place.project(Projection::Field { field, ty }), block))
            }
            ExprKind::TupleField { base, index } => {
                // Storage for the base, as the `Field` arm gives it: `f().0`
                // has none of its own, so the call lands in a temporary.
                let (place, block) = match self.as_place(*base, block) {
                    Some(found) => found,
                    None => {
                        let base_ty = thir.expr(*base).ty;
                        let base_span = thir.expr(*base).span;
                        let temp = self.temp(base_ty, base_span, block);
                        let block = self.expr_into(Place::local(temp), *base, block);
                        (Place::local(temp), block)
                    }
                };
                let place = self.auto_deref(place);
                let base_ty = self.place_ty(&place);
                let ty = self.tuple_field_ty(base_ty, *index as usize);
                Some((place.project(Projection::TupleField { index: *index, ty }), block))
            }
            ExprKind::Index { base, index } => {
                let index = *index;
                // **An index needs storage for its base too, for the `Field`
                // arm's reason and by the `Field` arm's means.** `h.items()[0]`
                // was the same `Rvalue::Error` as `a.scaled(2.0).x`, because
                // this arm's `?` gave up on a base with no place of its own.
                // Materialising here rather than widening the `ExprKind::Call`
                // arm above keeps the same locality: `as_place` still answers
                // `None` for a method call to every *other* caller, so
                // `for c in text.chars():` still reads that `None` the way
                // `borrow_source` needs it read.
                //
                // **Unlike the `Field` arm, this one is not free**, and the
                // difference is worth stating where it lives. Every route out
                // of the `Field` arm's `?` ended in a refusal; one route out of
                // this one does not. `h.items()[0] be 5` — an assignment whose
                // *target* is an index into a place-less base — passes
                // `sciencec check` and reaches `StmtKind::Assign`'s `None`
                // branch, which evaluates the value into a discarded temporary
                // and drops the target on the floor: it builds, it runs, and it
                // does nothing, silently. With this fallback the target becomes
                // a real place — the element of the temporary the base was
                // materialised into — so the base is evaluated, `bounds_check`
                // runs over it, and the write lands in storage that dies at the
                // end of the statement. Still no lasting effect, because a
                // method's returned array *is* a temporary and there is nothing
                // else for it to mean, but now the index is checked, so an
                // out-of-range one panics where it used to be discarded. That
                // is a behaviour change on a program that compiles today, and
                // it is the right way round: an index that is evaluated is
                // bounds-checked, everywhere, and the front end accepting such
                // a target at all is the separate question.
                let (place, block) = match self.as_place(*base, block) {
                    Some(found) => found,
                    None => {
                        let base_ty = thir.expr(*base).ty;
                        let base_span = thir.expr(*base).span;
                        let temp = self.temp(base_ty, base_span, block);
                        let block = self.expr_into(Place::local(temp), *base, block);
                        (Place::local(temp), block)
                    }
                };
                let place = self.auto_deref(place);
                let index_ty = thir.ty(index);
                let span = thir.expr(index).span;
                // A fresh temporary per index expression, assigned once. That
                // is what `Projection::Index`'s equality rests on.
                let temp = self.temp(index_ty, span, block);
                let block = self.expr_into(Place::local(temp), index, block);
                let block = self.bounds_check(&place, temp, index_ty, block, span);
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

    /// [`Builder::auto_deref`]'s Decision 28 sibling: inserts a `Deref` for
    /// every `Box` between a place and its payload.
    ///
    /// **Two callers now, and the second is Decision 28's AMENDMENT 6.**
    /// [`Builder::test_pattern`] and [`Builder::bind_pattern`] run it at the
    /// top of their variant and record arms, so that a pattern under a `Box` —
    /// at the scrutinee or one payload down — tests and binds the payload
    /// rather than the box's pointer; `science-types` has already typed every
    /// owning binding under it as a shared borrow, so what that caller builds
    /// from the place is a `Ref`, never a `Move`. [`Builder::borrow_through_box`]
    /// takes the same projection by hand, exactly one layer, because rule 6a
    /// crosses exactly one `Box`.
    ///
    /// **`lower_method_call`'s Shared/Mutable arm is the first caller**, run
    /// after [`Builder::auto_deref`] and [`Builder::deref_to_hole`] have taken
    /// the place as far as *those* peel — which is never through a `Box`,
    /// because [`TyKind::Named`] is not [`TyKind::Borrowed`] and neither
    /// function's loop matches it. `science-types`' `check::BodyChecker::lookup`
    /// already resolved the call by peeling the identical layer at the type
    /// level (`methods::receiver_head`'s own note), and by the time this runs
    /// the checker has also refused every receiver this function sees that is
    /// not a shared-self call (`check::BodyChecker::guard_box_receiver`) — so
    /// a place reached here is a value nothing downstream may write to or move
    /// out of, only borrow.
    ///
    /// **Why this reuses [`Projection::Deref`] rather than a projection of its
    /// own.** A `Box[T]` and a `borrowed T`/`mutable borrowed T` are the
    /// identical machine value — a bare pointer to `T`
    /// (`science_codegen::layout::CgTy::Ptr` gives `PtrKind::Box`,
    /// `PtrKind::Borrow` and `PtrKind::MutBorrow` the same one-word
    /// representation) — so the projection that turns a borrow's pointer into
    /// its referent's place is representation-correct for a `Box`'s pointer
    /// too, and `science-codegen-llvm`'s lowering of `Projection::Deref` never
    /// asks *why* the base is pointer-shaped, only that it is. Reusing the
    /// variant also means `science-regions`' `deref_move.rs` — which refuses
    /// every `Move` whose place contains a `Deref`, regardless of what the
    /// base was — reaches a `Box` for free. That refusal is a second,
    /// structural line behind the type checker's, not a decision of its own:
    /// moving a value out of a `Box` through this same transparency is the
    /// open question Decision 28 leaves exactly where it leaves the identical
    /// question for a borrow, and reusing `Deref` inherits that answer instead
    /// of getting a permissive one by omission.
    ///
    /// **One payload shape stops the loop rather than taking it: an interface
    /// object.** `Box[any Summarize]` does not lower to a pointer *to* a
    /// two-word value the way `Box[Doc]` lowers to a pointer to a record —
    /// `science_codegen::descriptor::needs_drop`'s own comment says the two
    /// words plainly: *"`Box of any I` and `borrowed any I` are both
    /// `CgTy::Interface` — two words, same layout, same ABI class, same
    /// niche"*. A `Box` around an interface object **is** the fat pointer,
    /// not a box holding one, so the place this receiver already denotes is
    /// exactly the `borrowed any Summarize` a shared-self call needs, and
    /// projecting a `Deref` onto it hands the interface method a load of the
    /// data pointer's first word in place of the vtable it also carries.
    /// This was found by the ratchet `corpus_output.rs` exists for: peeling
    /// unconditionally passed every unit test in this crate — none of them
    /// builds and runs a `Box` of an interface object end to end — and only
    /// failed where `examples/08_dyn_dispatch.science` did, with
    /// `SC0400`'s *"a dereference of a value that is not a pointer"*, which is
    /// this exact case caught rather than miscompiled.
    fn box_deref(&mut self, mut place: Place) -> Place {
        loop {
            let Some(boxed) = self.box_type else { return place };
            let written = self.place_ty(&place);
            let ty = self.revealed(written);
            let TyKind::Named { def, args } = self.context.types.kind(ty) else { return place };
            if *def != boxed || args.len() != 1 {
                return place;
            }
            let GenericArg::Type(inner) = &args[0] else { return place };
            let inner = *inner;
            let revealed_inner = self.revealed(inner);
            if matches!(self.context.types.kind(revealed_inner), TyKind::Object { .. }) {
                return place;
            }
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

    /// [`Self::read`], with `type-checking-and-mir.md` Decision 27 answered.
    ///
    /// **The gap this closes.** `science-types`' checker can now type a field
    /// or a pattern's binding `borrowed T` where the place underneath is the
    /// same storage a plain `T` field always was — nothing about the record's
    /// layout changed, only what the type checker calls the value read out of
    /// it. `read` alone cannot tell: it asks `is_copy` of `ty`, and a *shared*
    /// borrow answers `true` regardless of what it borrows, so it handed back
    /// `Operand::Copy(place)` — a bitwise copy of the place's actual bytes,
    /// which are a `String`'s three words and not a pointer. Passed anywhere a
    /// pointer was expected, that is `local _2 is ptr and the value stored
    /// into it is an aggregate` out of the linker, which is how this was
    /// found: the reproducer in `65ee44c` failed to *link* the first time this
    /// function's caller was changed to read a borrowed field, rather than
    /// running the double free `SC0303` used to refuse — the miscompile Decision
    /// 27 exists to prevent had simply moved one phase down.
    ///
    /// **The fix.** When `ty` reveals a borrow and the place's *own* type
    /// (`place_ty`, which is always the record's true declared type — see
    /// [`Self::as_place`]'s `ExprKind::Field` arm) does not, this is exactly
    /// that situation, and what is owed is what an explicit `&d.title` already
    /// gets: a real address, taken into a temporary with [`Self::borrow_place`]
    /// and moved out of that. Every other case — `ty` was never a borrow, or
    /// it was and the place already holds one (a field the record itself
    /// declares `borrowed`, or a parameter) — falls through to `read`
    /// unchanged, which is the overwhelming majority of every call this
    /// replaces.
    ///
    /// **Not two-phase.** `in_argument` two-phasing is reserved for a written
    /// `ExprKind::Borrow` at `Site::Argument`, which `Builder::argument`'s own
    /// first arm already handles; a Decision 27 reborrow reaching this
    /// function has no such node; it is an ordinary field or pattern read that
    /// happens to need an address. `v.push(v.borrowed_field)` on a `&mut`
    /// receiver would want one and does not get it — named rather than
    /// silently guessed at, and no corpus site is this shape.
    ///
    /// **Was believed never to cross a `Nullable`, and does.** `crate::check`'s
    /// `borrow_ergonomics` excludes a field behind one from Decision 27's
    /// ergonomic marking, and that is real — but a **niched narrow** reaches
    /// here by an entirely different door: `let cell be items.get(index)`
    /// followed by `if cell?: cell else: 0` types `cell` as `&Int` inside the
    /// `if`, `Coercion::Copy` needs that `&Int` as its operand, and
    /// `Builder::operand`'s default arm hands this function a *place* — `as_
    /// place` sees straight through `ExprKind::Narrow` — whose own `place_ty`
    /// is `(&Int)?` and not `&Int`. That is not a field record declared
    /// `borrowed`; it is Decision 19's niche, and the two look identical to
    /// the test below: `ty` reveals a borrow, the place's own type does not.
    ///
    /// **Handled first, and separately, because the repair is the opposite of
    /// Decision 27's.** A field genuinely needs a *new* address taken — the
    /// record's storage holds a `T`, not a pointer, so `&d.title` is the only
    /// way to a `&T`. A niched narrow's storage already *is* the pointer —
    /// that is what makes it niched — so taking `&cell` borrows the slot and
    /// hands back the address of the pointer, one indirection too many; the
    /// verifier does not catch it because both are `ptr`, and the result is
    /// `value_at`'s own finding, a different number printed on every run.
    /// What is owed here is `Builder::read`'s plain copy of the eight bytes
    /// already sitting in `cell`'s storage — no borrow, because there is
    /// nothing left to address that a fresh one would reach any better.
    fn read_ergonomic(
        &mut self,
        place: Place,
        ty: Ty,
        block: BlockId,
        span: Span,
    ) -> (Operand, BlockId) {
        let revealed = self.revealed(ty);
        if let TyKind::Borrowed { mutable, .. } = *self.context.types.kind(revealed) {
            let stored = self.revealed(self.place_ty(&place));
            if let TyKind::Nullable(payload) = *self.context.types.kind(stored) {
                let payload = self.revealed(payload);
                if matches!(self.context.types.kind(payload), TyKind::Borrowed { .. }) {
                    return (self.read(place, ty), block);
                }
            }
            if !matches!(self.context.types.kind(stored), TyKind::Borrowed { .. }) {
                let temp = self.temp(revealed, span, block);
                let block =
                    self.borrow_place(Place::local(temp), mutable, place, block, span, false);
                return (Operand::Move(Place::local(temp)), block);
            }
        }
        (self.read(place, ty), block)
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
            // A narrow taken straight as an operand — `print(x)` inside
            // `if x?:`, not `x.field` inside it — needs the same check
            // [`Self::as_place`]'s own `ExprKind::Field` arm already makes
            // before trusting its recursion into `ExprKind::Narrow`: seeing
            // through to the un-narrowed storage is exact for Decision 19's
            // niche and wrong for Decision 18's tagged pair, whose payload
            // sits at an offset `as_place` does not know to add. `let y be x`
            // inside the same `if` never had this bug — `expr_into`'s own
            // `ExprKind::Narrow` arm always builds the `Rvalue::Narrow` that
            // states the offset — and this arm is what makes an argument
            // agree with a `let`, by reaching for the same rvalue rather than
            // the place shortcut below.
            ExprKind::Narrow(operand) if self.narrow_needs_materialising(expr, *operand) => {
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), expr, block);
                (Operand::Move(Place::local(temp)), block)
            }
            _ => {
                if let Some((place, block)) = self.as_place(expr, block) {
                    return self.read_ergonomic(place, ty, block, span);
                }
                let temp = self.temp(ty, span, block);
                let block = self.expr_into(Place::local(temp), expr, block);
                (Operand::Move(Place::local(temp)), block)
            }
        }
    }

    /// [`Self::narrowed_payload_is_not_a_borrow`], asked off THIR types alone
    /// rather than off a [`Place`] already in hand.
    ///
    /// [`Self::operand`] is the one caller that has not fetched a place yet
    /// when it needs the answer — fetching one first to ask would mean
    /// lowering the narrow's operand twice if it is ever anything costlier
    /// than a bare read, which THIR's own rule (a narrow's operand is always
    /// a place, never a call or anything with an effect) makes safe today but
    /// this function declines to lean on. A bare local's own declared type is
    /// exactly [`Self::place_ty`]'s answer for it, so reading the operand
    /// expression's type off THIR — before any place is built — is the same
    /// question asked one step earlier.
    /// The payload type when `expr` is a shared or exclusive borrow of a
    /// nullable -- `&T?` -- and not a nullable of a borrow.
    fn borrowed_nullable_payload(&mut self, expr: ExprId) -> Option<Ty> {
        let ty = self.revealed(self.thir.expr(expr).ty);
        let TyKind::Borrowed { inner, .. } = *self.context.types.kind(ty) else { return None };
        let inner = self.revealed(inner);
        let TyKind::Nullable(payload) = *self.context.types.kind(inner) else { return None };
        Some(self.revealed(payload))
    }

    fn narrow_needs_materialising(&mut self, narrow: ExprId, operand: ExprId) -> bool {
        let hole_ty = self.revealed(self.thir.expr(narrow).ty);
        if matches!(self.context.types.kind(hole_ty), TyKind::Borrowed { .. }) {
            return false;
        }
        let written = self.revealed(self.thir.expr(operand).ty);
        let TyKind::Nullable(payload) = *self.context.types.kind(written) else { return false };
        let payload = self.revealed(payload);
        !matches!(self.context.types.kind(payload), TyKind::Borrowed { .. })
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
                    self.lower_borrow(Place::local(temp), *mutable, *operand, ty, block, span, true);
                (Operand::Move(Place::local(temp)), block)
            }
            // Decision 28's AMENDMENT 6: a reborrow through a `Box`, never an
            // `Rvalue::Coerce`. Shared, so `in_argument` changes nothing about
            // its kind; passed anyway, so the call reads like its neighbours.
            ExprKind::Coerce { operand, coercion: Coercion::BorrowThroughBox } => {
                let temp = self.temp(ty, span, block);
                let block =
                    self.borrow_through_box(Place::local(temp), *operand, block, span, true);
                (Operand::Move(Place::local(temp)), block)
            }
            // `mutable borrowed T` into `borrowed T`: a shared loan of `*r`.
            ExprKind::Coerce { operand, coercion: Coercion::Reborrow } => {
                let temp = self.temp(ty, span, block);
                let block = self.shared_reborrow(Place::local(temp), *operand, block, span, true);
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
            // **An exclusive borrow already held in a place is reborrowed, not
            // moved.** `bump(c)` with `c: &mut Counter` used to pass `c` itself
            // — a `Move`, since `&mut T` is not `Copy` — and once a use after
            // move was reported for every non-`Copy` value, the second
            // `bump(c)` in a loop was `SC0301`: a program that is plainly
            // fine, and the shape every `&mut` parameter handed on takes.
            // Rust's answer is the implicit reborrow, and this is it: a fresh
            // exclusive loan of `*c` for the duration of the call, which the
            // region engine checks like any other, so `c` is usable again the
            // moment the callee returns and two overlapping uses are still
            // refused.
            _ => {
                let revealed = self.revealed(ty);
                if let TyKind::Borrowed { mutable: true, inner } = *self.context.types.kind(revealed)
                {
                    // `as_place` may emit statements (an index's bounds
                    // check), so the place it hands back is used whichever
                    // way the question below goes, never lowered twice.
                    if let Some((place, block)) = self.as_place(expr, block) {
                        // When the place *holds* the borrow it is reborrowed
                        // through one `Deref`. A field read through `mutable
                        // self` — `print(self.tag)` — is typed `&mut String`
                        // by Decision 27 while its place is the `String`
                        // itself, and the borrow to take is of that place.
                        let place_ty = self.revealed(self.place_ty(&place));
                        let holds = matches!(
                            self.context.types.kind(place_ty),
                            TyKind::Borrowed { mutable: true, .. }
                        );
                        let target =
                            if holds { place.project(Projection::Deref { ty: inner }) } else { place };
                        let temp = self.temp(ty, span, block);
                        let block =
                            self.borrow_place(Place::local(temp), true, target, block, span, true);
                        return (Operand::Move(Place::local(temp)), block);
                    }
                }
                self.operand(expr, block)
            }
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
    ///
    /// **A tuple's element is read off the place, not off the sub-pattern.**
    /// Decision 27 types a binding under a borrowed tuple — `match t:` over
    /// `t: &(String, Int)`, or the `&(String, Int)` a `for` over an array of
    /// tuples binds — as `&String`, because it borrows what it names. The
    /// bytes at that position are still the `String`, and a `TupleField`
    /// typed `&String` made `bind_pattern` store the aggregate into a pointer
    /// slot — `SC0402`'s *"local is ptr and the value stored into it is an
    /// aggregate"*. It is [`Builder::field_ty`]'s rule for a record — the
    /// declaration's type and never the binding's — at the other shape a
    /// pattern projects through; `read_ergonomic` takes the borrow from the
    /// place the same way it does for a field.
    fn positional_ty(
        &mut self,
        place: &Place,
        variant: Option<DefId>,
        index: usize,
        element: PatId,
    ) -> Ty {
        let fallback = self.thir.pat(element).ty;
        match variant {
            Some(variant) => self.variant_payload_ty(variant, Some(index)).unwrap_or(fallback),
            None => {
                let mut tuple = self.revealed(self.place_ty(place));
                while let TyKind::Borrowed { inner, .. } = *self.context.types.kind(tuple) {
                    tuple = self.revealed(inner);
                }
                match self.tuple_field_ty(tuple, index) {
                    Ty::ERROR => fallback,
                    ty => ty,
                }
            }
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
                    // §5.1. `T implements Copy` is §6.1 rule 2's own exception,
                    // and it is asked last because the three above are a
                    // comparison and this is an index lookup plus a walk.
                    || self.declares_copy(ty)
            }
            _ => false,
        }
    }

    /// §5.1. Whether the crate writes `T implements Copy:` for this type **and**
    /// the type owns nothing.
    ///
    /// The second half is not redundant, and it is the whole reason this is a
    /// function rather than a `declares` call inlined into [`Self::is_copy`].
    /// Nothing in the compiler checks that a `Copy` implementation is
    /// well-formed: `type P: text: String` followed by `P implements Copy` is
    /// accepted by the resolver and the checker today. Believing that
    /// declaration would make `f(p)` a bitwise copy of a `String`'s three
    /// words, leave the source's drop standing, and free the buffer twice —
    /// turning a mistake the front end has not learned to refuse into a
    /// miscompile. [`crate::moves::needs_drop`] is the same predicate
    /// [`crate::drops`] elaborates against, so gating on it makes the copy and
    /// the drop agree by construction.
    ///
    /// **So the doubt still goes §5's way wherever it is doubt.** A type that
    /// owns something keeps its `Move` whatever it declares; what changes is
    /// only the case where the answer is not in doubt at all.
    fn declares_copy(&mut self, ty: Ty) -> bool {
        let Some(interface) = self.copy_interface else {
            return false;
        };
        if !self.context.decls.methods().declares(self.context.types, ty, interface) {
            return false;
        }
        !crate::moves::needs_drop(
            self.context.decls,
            self.context.types,
            self.context.aliases,
            ty,
        )
    }
}

/// Turns an operand into one that reads rather than consumes. §5's exception.
fn force_copy(operand: Operand) -> Operand {
    match operand {
        Operand::Move(place) => Operand::Copy(place),
        other => other,
    }
}

/// Whether a closure type appears anywhere in `ty`: §8.7's test for *a
/// function whose result may carry a closure out of its frame*.
fn ty_contains_closure(types: &Types, ty: Ty) -> bool {
    match types.kind(ty) {
        TyKind::Closure { .. } => true,
        TyKind::Named { args, .. } | TyKind::Object { args, .. } => args.iter().any(|arg| {
            matches!(arg, GenericArg::Type(inner) if ty_contains_closure(types, *inner))
        }),
        TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
            ty_contains_closure(types, *inner)
        }
        TyKind::Tuple(elements) => elements.iter().any(|element| ty_contains_closure(types, *element)),
        _ => false,
    }
}

/// Whether `ty` mentions a [`TyKind::Param`] anywhere inside it.
///
/// §8's gate on lowering a closure's body: a type built from one of the
/// *enclosing* function's generic parameters is not concrete, and this crate
/// runs before monomorphisation (`lib.rs` §2's *"Decision 42's order is types →
/// THIR analyses → MIR → regions → mono → codegen"*) — so a closure whose
/// parameter or return mentions one has no single body to lower yet, only one
/// body per instantiation nobody has computed. Left as §8.5's hole, exactly
/// like a captured closure, rather than guessed at.
fn ty_mentions_param(types: &Types, ty: Ty) -> bool {
    match types.kind(ty) {
        TyKind::Param { .. } => true,
        TyKind::Named { args, .. } | TyKind::Object { args, .. } => args.iter().any(|arg| {
            matches!(arg, GenericArg::Type(inner) if ty_mentions_param(types, *inner))
        }),
        TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
            ty_mentions_param(types, *inner)
        }
        TyKind::Tuple(elements) => elements.iter().any(|element| ty_mentions_param(types, *element)),
        TyKind::Closure { params, ret } => {
            params.iter().any(|param| ty_mentions_param(types, *param))
                || ty_mentions_param(types, *ret)
        }
        TyKind::Error | TyKind::Unit | TyKind::SelfType { .. } | TyKind::SelfAssoc { .. } => false,
    }
}

// --- the chain vocabulary, fused ------------------------------------------

/// The prelude types a chain link may be reached through: `science-resolve`'s
/// own list, read rather than copied.
///
/// A method is a chain link when its own definition is the prelude's and the
/// block it was declared in is one of these. Matching on the *name* rather
/// than on a `DefId` is what `science-resolve`'s `builtins` already does for
/// every other prelude shape reached from this crate — `Builder::push_of`'s
/// widths, `Lowerer::prelude_method`'s table — and it costs a lookup the
/// prelude cannot get wrong, because these names are declared in exactly
/// one place.
const CHAIN_TYPES: &[&str] = science_resolve::builtins::CHAIN_TYPES;

/// The prelude types `builtins.rs`' `wrapping_block!` declares the three
/// wrapping operations on: every fixed-width integer. `Int` is `I64`.
const WRAPPING_TYPES: &[&str] = &["I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64"];

/// One link of a chain, with the argument the author wrote at it.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// `discard(p)` — drop the items `p` holds for.
    Discard(ExprId),
    /// `keep(p)` — drop the items `p` does *not* hold for.
    Keep(ExprId),
    /// `map(f)` — replace each item with `f` of it.
    Map(ExprId),
    /// `take(n)` — stop after `n` have passed.
    Take(ExprId),
    /// `skip(n)` — let the first `n` that reach it go by.
    Skip(ExprId),
    /// `sorted(by: key)` — §1.4's **barrier**: buffer, sort, yield.
    Sorted(ExprId),
    /// `numbered()` — wrap each item that reaches it in §1.3's `Numbered`,
    /// with the position it arrived at, counted from zero.
    Numbered,
    /// `take_while(p)` — pass items while `p` holds; the first one it does not
    /// hold for ends the chain, as a `take` at its limit does.
    TakeWhile(ExprId),
    /// `skip_while(p)` — swallow items while `p` holds; from the first one it
    /// does not hold for, everything passes and `p` is not asked again.
    SkipWhile(ExprId),
    /// `every(n)` — pass the first item and every `n`th after it.
    Every(ExprId),
    /// `zip(other)` — pair each item with the next element of `other`, an
    /// `Array`'s `iterate()` whose array this holds; the chain ends when
    /// `other` does.
    Zip(ExprId),
    /// `reverse()` — §1.4's second barrier: buffer, turn round, yield.
    Reverse,
    /// `accumulate(initial, f)` — the running fold: each item is folded into
    /// the state with `f(state, item)` and the chain yields the new state. The
    /// two expressions are the initial state and the two-parameter closure.
    Accumulate(ExprId, ExprId),
    /// `keep_some()` — items `T?` are dropped when absent and unwrapped to `T`
    /// when present.
    KeepSome,
    /// `unique()` — each distinct item once, the first of its equals: a `Set`
    /// of the keys seen so far, and an item whose key is already in it is
    /// dropped. The key is the item read through one borrow.
    Unique,
    /// `windows(n)` — every run of `n` consecutive items, as an `Array` of
    /// them: a ring of the last `n` items that is cloned each time it is full.
    Windows(ExprId),
    /// `batches(n)` — disjoint runs of `n` items as arrays, the last one
    /// shorter when the chain does not divide. **A barrier in the pass that
    /// reads it**: see [`Builder::lower_chain_over`].
    Batches(ExprId),
    /// `flatten()` — each item is an array, or a borrow of one, and the chain
    /// yields their elements in turn. **A barrier**: see
    /// [`Builder::lower_chain_over`].
    Flatten,
    /// `followed_by(other)` — the chain's items, then a borrow of each element
    /// of `other`, an `Array`'s `iterate()` whose array this holds. **A
    /// barrier**: see [`Builder::lower_chain_over`].
    FollowedBy(ExprId),
    /// `owned()` — each item is a borrow of a `T`, and the chain yields a
    /// copy of it: a bitwise one for a `Copy` type, `clone()` for the rest.
    Owned,
}

/// What a chain reads: §5.4's source methods, each of which a fused loop
/// walks by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// `Array.iterate()` — `&T` for each element.
    Array,
    /// `Map.iterate()` — the `&Entry[K, V]` `for entry in m:` hands out.
    MapEntries,
    /// `Map.keys()` — a borrow of each entry's key.
    MapKeys,
    /// `Map.values()` — a borrow of each entry's value.
    MapValues,
    /// What follows a barrier: the buffer the barrier filled, popped from the
    /// end, each item moved out owned. See [`Builder::lower_chain_over`].
    Drain,
}

/// What a chain ends in: `collections-and-chains.md` §1.4's terminals that
/// the prelude declares, with the predicate the author wrote where there is
/// one.
#[derive(Debug, Clone, Copy)]
enum Terminal {
    /// `collect()` — every item, into an `Array`.
    Collect,
    /// `count()` — how many items arrived.
    Count,
    /// `sum()` — their total, from zero. `science-types`' `chain_total` has
    /// already made the result a number and taken any borrow off it.
    Sum,
    /// `product()` — their product, from one, as `Sum` is their total from
    /// zero.
    Product,
    /// `reduce(initial, f)` — `f(acc, item)` folded over every item, from
    /// `initial`. `f` is the two-parameter closure `(acc, x) giving ...`;
    /// the accumulator goes in by value and the result is the new one.
    Reduce { initial: ExprId, f: ExprId },
    /// `has_any(p)` — whether one item satisfies `p`; leaves at the first.
    HasAny(ExprId),
    /// `has_all(p)` — whether every item does; leaves at the first that
    /// does not.
    HasAll(ExprId),
    /// `first()` — the first item, or `null`; leaves at once.
    First,
    /// `find(p)` — the first item `p` holds for, or `null`.
    Find(ExprId),
    /// `last()` — the final item, or `null`; walks the whole chain.
    Last,
    /// `minimum(by: key)` — the item with the least key, or `null`. The first
    /// of equal keys wins.
    Minimum(ExprId),
    /// `maximum(by: key)` — the item with the greatest key, or `null`. The
    /// first of equal keys wins, as it does for `Minimum`.
    Maximum(ExprId),
    /// The body of a written `for`, run once per item. Not a method at all:
    /// `for x in chain:` is a chain whose terminal is the loop.
    Each { pattern: PatId, body: thir::BlockId },
}

impl Terminal {
    /// The terminal a prelude chain method's name is, when it is one.
    fn named(name: &str, arguments: &[ExprId]) -> Option<Terminal> {
        let first = arguments.first().copied();
        Some(match (name, first) {
            ("collect", _) => Terminal::Collect,
            ("count", _) => Terminal::Count,
            ("sum", _) => Terminal::Sum,
            ("product", _) => Terminal::Product,
            ("reduce", Some(initial)) if arguments.len() == 2 => {
                Terminal::Reduce { initial, f: arguments[1] }
            }
            ("first", _) => Terminal::First,
            ("last", _) => Terminal::Last,
            ("has_any", Some(predicate)) => Terminal::HasAny(predicate),
            ("has_all", Some(predicate)) => Terminal::HasAll(predicate),
            ("find", Some(predicate)) => Terminal::Find(predicate),
            ("minimum", Some(key)) => Terminal::Minimum(key),
            ("maximum", Some(key)) => Terminal::Maximum(key),
            _ => return None,
        })
    }

    /// The terminal's own predicate, evaluated once before the loop exactly as
    /// a link's closure is.
    fn predicate(self) -> Option<ExprId> {
        match self {
            Terminal::HasAny(predicate)
            | Terminal::HasAll(predicate)
            | Terminal::Find(predicate)
            | Terminal::Minimum(predicate)
            | Terminal::Maximum(predicate) => Some(predicate),
            Terminal::Reduce { f, .. } => Some(f),
            Terminal::Collect
            | Terminal::Count
            | Terminal::Sum
            | Terminal::Product
            | Terminal::First
            | Terminal::Last
            | Terminal::Each { .. } => None,
        }
    }
}

/// A whole chain, read off the THIR spine of a terminal's receiver.
struct Chain {
    /// The `Array` or `Map` the source method was called on.
    source: ExprId,
    /// Which of §5.4's source methods it was.
    kind: Source,
    /// Each link, and the `Item` the chain has *after* it.
    ///
    /// The item *before* the first link is not carried: it is a borrow of the
    /// source's element by `collections-and-chains.md` §4.3, and
    /// [`Builder::chain_pass`] reads it off the source array it is already
    /// holding rather than off a field that could disagree with it.
    steps: Vec<(Step, Ty)>,
}

impl Builder<'_, '_> {
    /// `(the block's type name, the method's name)` when this definition is a
    /// prelude chain method.
    fn chain_method(&self, def: DefId) -> Option<(&str, &str)> {
        let entry = self.context.defs.get(def);
        if !entry.is_builtin() {
            return None;
        }
        let owner = entry.parent?;
        let owner_name = self.context.defs.get(owner).name.as_str();
        CHAIN_TYPES
            .contains(&owner_name)
            .then_some((owner_name, entry.name.as_str()))
    }

    /// The operator a prelude `wrapping_add`, `wrapping_sub` or `wrapping_mul`
    /// on a fixed-width integer is, or `None` for any other definition — a
    /// user's own method of one of those names among them, which
    /// `is_builtin` is what rules out.
    fn wrapping_operator(&self, def: DefId) -> Option<BinaryOp> {
        let entry = self.context.defs.get(def);
        if !entry.is_builtin() {
            return None;
        }
        let op = match entry.name.as_str() {
            "wrapping_add" => BinaryOp::Add,
            "wrapping_sub" => BinaryOp::Sub,
            "wrapping_mul" => BinaryOp::Mul,
            _ => return None,
        };
        let owner = entry.parent?;
        WRAPPING_TYPES.contains(&self.context.defs.get(owner).name.as_str()).then_some(op)
    }

    /// Which of §5.4's source methods this definition is, when it is one:
    /// the prelude's `Array.iterate`, or `Map`'s `iterate`, `keys` and
    /// `values`.
    fn chain_source_kind(&self, def: DefId) -> Option<Source> {
        let entry = self.context.defs.get(def);
        if !entry.is_builtin() {
            return None;
        }
        let owner = self.context.defs.get(entry.parent?).name.as_str();
        match (owner, entry.name.as_str()) {
            ("Array", "iterate") => Some(Source::Array),
            ("Map", "iterate") => Some(Source::MapEntries),
            ("Map", "keys") => Some(Source::MapKeys),
            ("Map", "values") => Some(Source::MapValues),
            _ => None,
        }
    }

    /// Read a chain off the receiver of a terminal, or answer `None` when the
    /// receiver is not one.
    ///
    /// Walks the THIR spine from the terminal downwards and reverses, so the
    /// steps come out in the order the author wrote them.
    fn chain_of(&self, mut expr: ExprId) -> Option<Chain> {
        let mut steps: Vec<(Step, Ty)> = Vec::new();
        loop {
            let node = self.thir.expr(expr);
            let ExprKind::MethodCall { receiver, method: Some(method), args } = &node.kind else {
                return None;
            };
            let (receiver, method, args) = (*receiver, *method, args.clone());
            if let Some(kind) = self.chain_source_kind(method) {
                steps.reverse();
                return Some(Chain { source: receiver, kind, steps });
            }
            let (_, name) = self.chain_method(method)?;
            let item = self.thir.expr(expr).ty;
            let item = self.chain_item(item)?;
            let step = match (name, args.first()) {
                ("discard", Some(arg)) => Step::Discard(*arg),
                ("keep", Some(arg)) => Step::Keep(*arg),
                ("map", Some(arg)) => Step::Map(*arg),
                ("take", Some(arg)) => Step::Take(*arg),
                ("skip", Some(arg)) => Step::Skip(*arg),
                ("sorted", Some(arg)) => Step::Sorted(*arg),
                ("numbered", _) => Step::Numbered,
                ("keep_some", _) => Step::KeepSome,
                ("reverse", _) => Step::Reverse,
                ("unique", _) => Step::Unique,
                ("windows", Some(arg)) => Step::Windows(*arg),
                ("batches", Some(arg)) => Step::Batches(*arg),
                ("flatten", _) => Step::Flatten,
                ("owned", _) => Step::Owned,
                // Only an `Array.iterate()` can follow, as only one can be
                // zipped: the declaration takes an `ArrayIterate`.
                ("followed_by", Some(arg)) => {
                    let ExprKind::MethodCall { receiver: other, method: Some(source), .. } =
                        self.thir.expr(*arg).kind
                    else {
                        return None;
                    };
                    if self.chain_source_kind(source) != Some(Source::Array) {
                        return None;
                    }
                    Step::FollowedBy(other)
                }
                ("accumulate", Some(initial)) if args.len() == 2 => {
                    Step::Accumulate(*initial, args[1])
                }
                ("take_while", Some(arg)) => Step::TakeWhile(*arg),
                ("skip_while", Some(arg)) => Step::SkipWhile(*arg),
                ("every", Some(arg)) => Step::Every(*arg),
                // Only an `Array.iterate()` can be walked beside the loop's
                // own cursor, and the prelude's declaration says so by taking
                // an `ArrayIterate`: the receiver is the array.
                ("zip", Some(arg)) => {
                    let ExprKind::MethodCall { receiver: other, method: Some(source), .. } =
                        self.thir.expr(*arg).kind
                    else {
                        return None;
                    };
                    if self.chain_source_kind(source) != Some(Source::Array) {
                        return None;
                    }
                    Step::Zip(other)
                }
                _ => return None,
            };
            steps.push((step, item));
            expr = receiver;
        }
    }

    /// The `Item` an adapter type carries: the second argument of
    /// `Discard[S, I]` and its five siblings.
    fn chain_item(&self, ty: Ty) -> Option<Ty> {
        match self.context.types.kind(ty) {
            TyKind::Named { args, .. } => args.get(1).and_then(|arg| arg.as_type()),
            _ => None,
        }
    }

    /// `chain.terminal()` — the only link that does work.
    ///
    /// # Why the chain is fused here rather than built as values
    ///
    /// `collections-and-chains.md` §2.1 makes every adapter lazy and every
    /// terminal eager: *"A chain does nothing until a terminal pulls it"*.
    /// The note's own implementation of that is a struct per adapter owning
    /// its source, with a `next()` each — and §7 point 2 is the sentence that
    /// makes a different one legal: *"Every combinator is specified by its
    /// input-to-output relation, not by what `next()` does"*, precisely so
    /// that a form which is not a pull loop can satisfy the contract. One
    /// fused loop computes the same relation, and it is what the note says
    /// F2's `.parallel()` will need anyway.
    ///
    /// It also buys the laziness §2.1 is arguing *for* rather than
    /// approximating it: `take(5)` after a `map` over a million rows does
    /// five units of work here, because the loop leaves at the fifth push,
    /// and `has_any` and `find` leave at the first match — the short-circuit
    /// §1.4 promises them.
    ///
    /// # What this is not, and the decision it sits next to
    ///
    /// `type-checking-and-mir.md` Decision 5 — *"a whole-array operation
    /// lowers to a call to a runtime or library entry point, never to an
    /// inlined MIR loop"* — is about `a + b` over arrays, and its reason is
    /// that F1 wants an array IR to schedule and fuse. A chain is the other
    /// thing: its source may be a file, its links carry user closures, and no
    /// runtime entry point can call a Science closure. The one piece that
    /// *can* be a call is the barrier, and it is one:
    /// `science_array_sort_by_int_key`.
    ///
    /// # The shapes that are refused, and why they are refused here
    ///
    /// Anything **after** `sorted(by:)` other than `collect()` — a link, or
    /// one of the scalar terminals. The buffer a barrier leaves holds owned
    /// items, so a second pass over it either moves them out — and then a
    /// `discard` after a `sorted` leaks every item it drops, and a `first()`
    /// every item it does not return — or borrows them, and the chain's
    /// `Item` says it does not. That is an ownership question the note does
    /// not answer and this commit does not invent; `Unresolved::Chain`
    /// carries the refusal so that the backend names it.
    fn lower_chain_terminal(
        &mut self,
        dest: Place,
        terminal: Terminal,
        chain: &Chain,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let (source, block) = self.chain_source(chain.source, block, span);
        self.lower_chain_over(dest, terminal, chain.kind, &source, &chain.steps, block, span)
    }

    /// A chain over `source`, split at its first barrier.
    ///
    /// # What a barrier is here
    ///
    /// `sorted(by:)` and `reverse()` (§1.4's two *barriers*) need every item
    /// before they can yield one. The links before the barrier run in one
    /// fused pass that **buffers** what survives into an array (and, for a
    /// sort, the keys beside it); the buffer is sorted or reversed in place by
    /// a runtime call; and whatever follows the barrier runs over the buffer
    /// as a chain of its own, recursively, so `sorted(by: k).reverse().take(10)`
    /// and a second barrier further on are the same code.
    ///
    /// # How the second half reads the buffer: it **pops** it
    ///
    /// The buffer holds owned items. A second pass that borrowed them would
    /// hand `take` and `map` a `&T` where the chain's `Item` says `T`; one that
    /// moved them out by index would leak every item a `take` never reached or
    /// double-free the ones it did. Popping is the one reading that is both
    /// honest and leak-free: each turn moves the last item out of the buffer
    /// (`science_array_pop`, the owned-`T?` convention), and whatever the
    /// chain leaves unread — a `take` that stops early, a `first()` — is still
    /// in the buffer, which is a temporary of the statement and is dropped
    /// with it. That is [`Source::Drain`].
    ///
    /// Popping reads the buffer **from the end**, so a buffer that a pass will
    /// pop is stored in the *reverse* of the order the chain yields it: a sort
    /// that feeds a pass is followed by a `reverse`, and a `reverse()` that
    /// feeds one is the buffer as it arrived. A barrier that is the chain's
    /// last link and ends in `collect()` is the exception, and is built in
    /// place in `dest` in the order the chain yields.
    ///
    /// # The cost
    ///
    /// A barrier is **eager** by definition, so a `take` after it saves the
    /// downstream work and not the upstream work, which §1.4 says in as many
    /// words (*"buffers every item once"*). The links after a barrier run
    /// after the whole upstream pass has finished, so a side effect in an
    /// upstream closure and one in a downstream closure no longer interleave
    /// by item.
    #[allow(clippy::too_many_arguments)]
    fn lower_chain_over(
        &mut self,
        dest: Place,
        terminal: Terminal,
        kind: Source,
        source: &Place,
        steps: &[(Step, Ty)],
        block: BlockId,
        span: Span,
    ) -> BlockId {
        // `batches(n)` is a barrier too: it needs to know which item is the
        // last to flush the short batch. Once a barrier has made the source a
        // buffer (`Source::Drain`) the length is known, so a `batches` at the
        // head of *that* pass is read in place, not split off again.
        let barrier = steps.iter().enumerate().position(|(index, (step, _))| match step {
            Step::Sorted(_) | Step::Reverse | Step::Flatten | Step::FollowedBy(_) => true,
            Step::Batches(_) => !(kind == Source::Drain && index == 0),
            _ => false,
        });
        let Some(at) = barrier else {
            return self.chain_pass(dest, terminal, None, kind, source, steps, block, span);
        };
        let batching = matches!(steps[at].0, Step::Batches(_));
        let rest = if batching { &steps[at..] } else { &steps[at + 1..] };
        let finishing = !batching && rest.is_empty() && matches!(terminal, Terminal::Collect);
        // What the buffer holds: the item before a `batches`, which is the
        // element of the arrays it makes, and the barrier's own item otherwise.
        let item_ty = if batching {
            match self.context.types.kind(steps[at].1).clone() {
                TyKind::Named { args, .. } => {
                    match args.first().and_then(|arg| arg.as_type()) {
                        Some(element) => element,
                        None => return block,
                    }
                }
                _ => return block,
            }
        } else {
            steps[at].1
        };
        let buffer = if finishing {
            dest.clone()
        } else {
            let ty = self.array_of(item_ty);
            Place::local(self.temp(ty, span, block))
        };

        let mut block = block;
        match steps[at].0 {
            Step::Sorted(key) => {
                // **The keys are computed in the same pass that fills the
                // buffer**, not in a second pass over it, and that is a
                // correctness point rather than an optimisation. A second
                // pass would have to re-read the buffer, and what it reads
                // there is a *borrow* of an item the chain's own type says is
                // a value — so the key closure would be handed eight bytes of
                // pointer where its parameter declares a twenty-four-byte
                // `String`, and would then drop it. Computing the key where
                // the item is still in hand asks the closure for exactly what
                // it was declared to take. It is also what
                // `science_array_sort_by_int_key`'s own note says the two
                // parallel arrays are for: one key per element, once.
                let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int")
                else {
                    return block;
                };
                let keys_ty = self.array_of(int_ty);
                let keys = Place::local(self.temp(keys_ty, span, block));
                block = self.chain_pass(
                    buffer.clone(),
                    Terminal::Collect,
                    Some((keys.clone(), key)),
                    kind,
                    source,
                    &steps[..at],
                    block,
                    span,
                );
                let (values_ref, next) = self.exclusive_ref(&buffer, block, span);
                block = next;
                let (keys_ref, next) = self.exclusive_ref(&keys, block, span);
                block = next;
                let discard = Place::local(self.temp(Ty::UNIT, span, block));
                block = self.emit_call(
                    discard,
                    Callee::Runtime(ARRAY_SORT_BY_INT_KEY),
                    vec![values_ref, keys_ref],
                    block,
                    span,
                );
                // Ascending now; a pass that pops reads it from the end.
                if !finishing {
                    block = self.chain_reverse_buffer(&buffer, block, span);
                }
            }
            Step::Reverse => {
                block = self.chain_pass(
                    buffer.clone(),
                    Terminal::Collect,
                    None,
                    kind,
                    source,
                    &steps[..at],
                    block,
                    span,
                );
                // As it arrived is what a pass that pops wants; a `collect()`
                // that ends here wants it turned round.
                if finishing {
                    block = self.chain_reverse_buffer(&buffer, block, span);
                }
            }
            Step::FollowedBy(other) => {
                block = self.chain_pass(
                    buffer.clone(),
                    Terminal::Collect,
                    None,
                    kind,
                    source,
                    &steps[..at],
                    block,
                    span,
                );
                block = self.chain_append_borrows(&buffer, other, block, span);
                // Arrival order; a pass that pops wants it turned round.
                if !finishing {
                    block = self.chain_reverse_buffer(&buffer, block, span);
                }
            }
            Step::Flatten => {
                // The items before it, collected: arrays (owned) or borrows
                // of arrays, whichever the chain's item was.
                let (element, borrowed) = match *self.context.types.kind(item_ty) {
                    TyKind::Borrowed { inner, .. } => (inner, true),
                    _ => (item_ty, false),
                };
                let array_ty = self.array_of(element);
                let outer_item = if borrowed {
                    self.context.types.borrowed(false, array_ty)
                } else {
                    array_ty
                };
                let outer_ty = self.array_of(outer_item);
                let outer = Place::local(self.temp(outer_ty, span, block));
                block = self.chain_pass(
                    outer.clone(),
                    Terminal::Collect,
                    None,
                    kind,
                    source,
                    &steps[..at],
                    block,
                    span,
                );
                block = self.chain_flatten(&outer, &buffer, element, borrowed, block, span);
                if !finishing {
                    block = self.chain_reverse_buffer(&buffer, block, span);
                }
            }
            Step::Batches(_) => {
                block = self.chain_pass(
                    buffer.clone(),
                    Terminal::Collect,
                    None,
                    kind,
                    source,
                    &steps[..at],
                    block,
                    span,
                );
                // Arrival order, popped from the end: turn it round.
                block = self.chain_reverse_buffer(&buffer, block, span);
            }
            _ => unreachable!("`position` found a barrier"),
        }
        if finishing {
            return block;
        }
        self.lower_chain_over(dest, terminal, Source::Drain, &buffer, rest, block, span)
    }

    /// Open a loop over `0..length`: the cursor, the head, the body's first
    /// block and the block after the loop.
    fn chain_open_count(
        &mut self,
        length: Local,
        block: BlockId,
        span: Span,
    ) -> (Local, BlockId, BlockId, BlockId) {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            unreachable!("the prelude declares `Int`")
        };
        let cursor = self.temp(int_ty, span, block);
        self.assign(block, Place::local(cursor), Rvalue::Use(Self::bits(0)), span);
        let (head, body, exit) = (self.new_block(), self.new_block(), self.new_block());
        self.terminate(block, TerminatorKind::Goto { target: head }, span);
        let more = self.temp(self.bool_ty, span, head);
        self.assign(
            head,
            Place::local(more),
            Rvalue::Binary {
                op: BinaryOp::Lt,
                lhs: Operand::Copy(Place::local(cursor)),
                rhs: Operand::Copy(Place::local(length)),
            },
            span,
        );
        self.terminate(
            head,
            TerminatorKind::If {
                cond: Operand::Copy(Place::local(more)),
                then_block: body,
                else_block: exit,
            },
            span,
        );
        (cursor, head, body, exit)
    }

    /// Close a loop [`Builder::chain_open_count`] opened: step the cursor and
    /// go round.
    fn chain_close_count(&mut self, cursor: Local, head: BlockId, end: BlockId, span: Span) {
        self.chain_add(&Place::local(cursor), Self::bits(1), end, span);
        self.terminate(end, TerminatorKind::Goto { target: head }, span);
    }

    /// A borrow of every element of the array `other` pushed onto `buffer`,
    /// an `Array` of such borrows.
    fn chain_append_borrows(
        &mut self,
        buffer: &Place,
        other: ExprId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        let (place, mut block) = self.chain_source(other, block, span);
        let array_ty = self.place_ty(&place);
        let element = self.element_ty(&place);
        let shared_ty = self.context.types.borrowed(false, array_ty);
        let shared = self.temp(shared_ty, span, block);
        block = self.borrow_place(Place::local(shared), false, place, block, span, false);
        let length = self.temp(int_ty, span, block);
        block = self.emit_call(
            Place::local(length),
            Callee::Runtime(ARRAY_LEN),
            vec![Operand::Copy(Place::local(shared))],
            block,
            span,
        );
        let (cursor, head, mut body, exit) = self.chain_open_count(length, block, span);
        let at = self.temp(int_ty, span, body);
        self.assign(body, Place::local(at), Rvalue::Use(Operand::Copy(Place::local(cursor))), span);
        let through = Place::local(shared)
            .project(Projection::Deref { ty: array_ty })
            .project(Projection::Index { index: at, ty: element });
        let item_ty = self.context.types.borrowed(false, element);
        let item = self.temp(item_ty, span, body);
        body = self.borrow_place(Place::local(item), false, through, body, span, false);
        let (buffer_ref, next) = self.exclusive_ref(buffer, body, span);
        let discard = Place::local(self.temp(Ty::UNIT, span, next));
        body = self.emit_call(
            discard,
            Callee::Runtime(ARRAY_PUSH),
            vec![buffer_ref, Operand::Move(Place::local(item))],
            next,
            span,
        );
        self.chain_close_count(cursor, head, body, span);
        exit
    }

    /// `flat` filled with the elements of every array in `outer`, in order.
    ///
    /// `borrowed` says what `outer` holds: borrows of arrays, whose elements
    /// are then borrowed one by one by index, or the arrays themselves, owned,
    /// which are popped (the buffer was collected in arrival order, so this
    /// reverses it first) and moved onto `flat` whole with `extend`.
    fn chain_flatten(
        &mut self,
        outer: &Place,
        flat: &Place,
        element: Ty,
        borrowed: bool,
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        let outer_ty = self.place_ty(outer);
        let array_ty = self.array_of(element);
        block = self.emit_call(
            flat.clone(),
            Callee::Runtime(ARRAY_WITH_CAPACITY),
            vec![Operand::Const(Constant::Count(0))],
            block,
            span,
        );
        if !borrowed {
            block = self.chain_reverse_buffer(outer, block, span);
        }
        let shared_ty = self.context.types.borrowed(false, outer_ty);
        let shared = self.temp(shared_ty, span, block);
        block = self.borrow_place(Place::local(shared), false, outer.clone(), block, span, false);
        let length = self.temp(int_ty, span, block);
        block = self.emit_call(
            Place::local(length),
            Callee::Runtime(ARRAY_LEN),
            vec![Operand::Copy(Place::local(shared))],
            block,
            span,
        );
        let (cursor, head, mut body, exit) = self.chain_open_count(length, block, span);
        if borrowed {
            let outer_item = self.element_ty(outer);
            let at = self.temp(int_ty, span, body);
            self.assign(
                body,
                Place::local(at),
                Rvalue::Use(Operand::Copy(Place::local(cursor))),
                span,
            );
            let through = Place::local(shared)
                .project(Projection::Deref { ty: outer_ty })
                .project(Projection::Index { index: at, ty: outer_item });
            let inner = self.temp(outer_item, span, body);
            self.assign(body, Place::local(inner), Rvalue::Use(Operand::Copy(through)), span);
            let inner_length = self.temp(int_ty, span, body);
            body = self.emit_call(
                Place::local(inner_length),
                Callee::Runtime(ARRAY_LEN),
                vec![Operand::Copy(Place::local(inner))],
                body,
                span,
            );
            let (inner_cursor, inner_head, mut inner_body, inner_exit) =
                self.chain_open_count(inner_length, body, span);
            let inner_at = self.temp(int_ty, span, inner_body);
            self.assign(
                inner_body,
                Place::local(inner_at),
                Rvalue::Use(Operand::Copy(Place::local(inner_cursor))),
                span,
            );
            let element_place = Place::local(inner)
                .project(Projection::Deref { ty: array_ty })
                .project(Projection::Index { index: inner_at, ty: element });
            let item_ty = self.context.types.borrowed(false, element);
            let item = self.temp(item_ty, span, inner_body);
            inner_body = self.borrow_place(
                Place::local(item),
                false,
                element_place,
                inner_body,
                span,
                false,
            );
            let (flat_ref, next) = self.exclusive_ref(flat, inner_body, span);
            let discard = Place::local(self.temp(Ty::UNIT, span, next));
            inner_body = self.emit_call(
                discard,
                Callee::Runtime(ARRAY_PUSH),
                vec![flat_ref, Operand::Move(Place::local(item))],
                next,
                span,
            );
            self.chain_close_count(inner_cursor, inner_head, inner_body, span);
            self.chain_close_count(cursor, head, inner_exit, span);
        } else {
            let slot_ty = self.context.types.nullable(array_ty);
            let slot = self.temp(slot_ty, span, body);
            let (outer_ref, next) = self.exclusive_ref(outer, body, span);
            let callee = match self.array_method(outer_ty, "pop") {
                Some(def) => Callee::Def { def, self_ty: None },
                None => Callee::Unresolved(Unresolved::Chain),
            };
            body = self.emit_call(Place::local(slot), callee, vec![outer_ref], next, span);
            let taken = self.temp(array_ty, span, body);
            let read = self.read(Place::local(slot), slot_ty);
            self.assign(
                body,
                Place::local(taken),
                Rvalue::Narrow { operand: read, ty: array_ty },
                span,
            );
            let (flat_ref, next) = self.exclusive_ref(flat, body, span);
            let callee = match self.array_method(array_ty, "extend") {
                Some(def) => Callee::Def { def, self_ty: None },
                None => Callee::Unresolved(Unresolved::Chain),
            };
            let discard = Place::local(self.temp(Ty::UNIT, span, next));
            body = self.emit_call(
                discard,
                callee,
                vec![flat_ref, Operand::Move(Place::local(taken))],
                next,
                span,
            );
            self.chain_close_count(cursor, head, body, span);
        }
        exit
    }

    /// `science_array_reverse(&mut buffer)`: the buffer turned round in place.
    fn chain_reverse_buffer(&mut self, buffer: &Place, block: BlockId, span: Span) -> BlockId {
        let (buffer_ref, block) = self.exclusive_ref(buffer, block, span);
        let discard = Place::local(self.temp(Ty::UNIT, span, block));
        self.emit_call(discard, Callee::Runtime(ARRAY_REVERSE), vec![buffer_ref], block, span)
    }

    /// The prelude's `Array` method of this name, for a call this crate builds
    /// itself rather than reads off a `MethodCall`.
    fn array_method(&mut self, array_ty: Ty, name: &str) -> Option<DefId> {
        self.type_method(array_ty, name, Form::Value)
    }

    /// The method of this name on a prelude type, in the given form: an
    /// instance method (`Form::Value`) or an associated function such as
    /// `Set.new` (`Form::Type`).
    fn type_method(&mut self, ty: Ty, name: &str, form: Form) -> Option<DefId> {
        let ty = self.revealed(ty);
        let methods = self.context.decls.methods();
        let head = methods.receiver(self.context.defs, self.context.types, ty)?;
        match methods.lookup(head, name, form) {
            Found::One(candidate) => Some(candidate.method),
            _ => None,
        }
    }

    /// `Set[element]`, interned.
    fn set_of(&mut self, element: Ty) -> Option<Ty> {
        let set = self.context.decls.prelude().get("Set")?;
        Some(self.context.types.named(set, vec![GenericArg::Type(element)]))
    }

    /// The operand a chain's closure is called with, read off the closure's
    /// **declared** parameter rather than assumed.
    ///
    /// `science-resolve`'s `builtins` gives a predicate and a key closure
    /// `(borrowed Item) -> …` and `map` `(Item) -> U` — §1.2 and §1.4
    /// respectively — and collapses the first spelling on `ArrayIterate[T]`,
    /// where §4.3 has already made the item a borrow. Two spellings, so this
    /// asks which one is in front of it: when the declaration wants a borrow
    /// of what is in hand, one is taken; otherwise the item goes as it is,
    /// moved when `map` consumes something owned and copied when it does not.
    fn chain_argument(
        &mut self,
        value: &Place,
        closure_ty: Ty,
        consuming: bool,
        block: BlockId,
        span: Span,
    ) -> (Operand, BlockId) {
        self.chain_argument_at(value, closure_ty, consuming, 0, block, span)
    }

    /// [`Builder::chain_argument`] for the closure's `at`th parameter: `reduce`
    /// takes the item second, after the accumulator.
    fn chain_argument_at(
        &mut self,
        value: &Place,
        closure_ty: Ty,
        consuming: bool,
        at: usize,
        block: BlockId,
        span: Span,
    ) -> (Operand, BlockId) {
        let item_ty = self.place_ty(value);
        let revealed = self.revealed(closure_ty);
        let want = match self.context.types.kind(revealed) {
            TyKind::Closure { params, .. } => params.get(at).copied(),
            _ => None,
        };
        let borrows_it = want.is_some_and(|want| {
            matches!(
                self.context.types.kind(want),
                TyKind::Borrowed { mutable: false, inner } if *inner == item_ty
            )
        });
        if borrows_it {
            let borrowed = self.context.types.borrowed(false, item_ty);
            let temp = self.temp(borrowed, span, block);
            let block =
                self.borrow_place(Place::local(temp), false, value.clone(), block, span, true);
            return (Operand::Move(Place::local(temp)), block);
        }
        (self.chain_operand(value, consuming), block)
    }

    /// The item as an operand: moved when it owns something and the reader
    /// consumes it, copied otherwise.
    fn chain_operand(&mut self, value: &Place, consuming: bool) -> Operand {
        let item_ty = self.place_ty(value);
        if consuming && !self.is_copy(item_ty) {
            Operand::Move(value.clone())
        } else {
            Operand::Copy(value.clone())
        }
    }

    /// The source array of a chain, as a place this crate can borrow.
    ///
    /// `docs.iterate()` is written on a place in every corpus site, but
    /// nothing says it has to be — `make_docs().iterate()` is legal and its
    /// source is a temporary. [`Builder::borrow_source`] answers both and is
    /// the same helper `for` uses on its subject.
    fn chain_source(&mut self, source: ExprId, block: BlockId, span: Span) -> (Place, BlockId) {
        let (place, block) = self.borrow_source(source, block, span);
        (self.auto_deref(place), block)
    }

    /// `Array[element]`, interned.
    fn array_of(&mut self, element: Ty) -> Ty {
        let Some(array) = self.context.decls.prelude().get("Array") else { return Ty::ERROR };
        self.context.types.named(array, vec![GenericArg::Type(element)])
    }

    /// One pass of the fused loop: read `source`, apply `steps`, hand what
    /// survives to `terminal`, and leave the answer in `dest`.
    ///
    /// `dest` is **written** here, on every path to the block this returns,
    /// so a caller hands over an uninitialised place of the terminal's type.
    ///
    /// # Every item is a scope, and every way out of it drops what it holds
    ///
    /// The per-item temporaries — the element borrow, each `map`'s result,
    /// each predicate's borrow — live in a scope pushed for the body, and
    /// every edge that leaves the body — to the next item, to the end because
    /// a `take` ran out, to the end because a `find` matched — runs that
    /// scope's exits first ([`Builder::chain_leave`]). An item a `discard`
    /// drops, a `skip` passes over, or a `count` counts is a value the chain
    /// owns when it is owned at all (anything after a `map` that allocates),
    /// and the drop elaboration of `crate::drops` is what decides, per path,
    /// whether it is still there to release: a `collect` has moved it into the
    /// array and a `first` into the result, and those drops go away.
    ///
    /// Before this, the body's temporaries lived in the statement's scope and
    /// were dropped once, after the loop, so an owned item a `discard` passed
    /// over was overwritten by the next turn's `map` rather than released —
    /// and a scalar terminal, which moves nothing, would have made that every
    /// item. `chains.rs`' `a_discarded_owned_item_is_released` holds the
    /// resident size flat across a quarter of a million of them.
    #[allow(clippy::too_many_arguments)]
    fn chain_pass(
        &mut self,
        dest: Place,
        terminal: Terminal,
        keys: Option<(Place, ExprId)>,
        kind: Source,
        source: &Place,
        steps: &[(Step, Ty)],
        mut block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        let Some(u64_ty) = self.context.decls.prelude().ty(self.context.types, "U64") else {
            return block;
        };
        let element_ty = self.element_ty(source);
        // A buffer a barrier filled is popped rather than indexed, and has no
        // extent to loan out or compare a cursor against.
        let drain = kind == Source::Drain;

        // What `dest` holds before the first item, for the three terminals
        // that accumulate into it. The other four write it once, at the edge
        // that decides it.
        match terminal {
            Terminal::Collect => {
                block = self.emit_call(
                    dest.clone(),
                    Callee::Runtime(ARRAY_WITH_CAPACITY),
                    vec![Operand::Const(Constant::Count(0))],
                    block,
                    span,
                );
            }
            Terminal::Count => {
                self.assign(block, dest.clone(), Rvalue::Use(Self::bits(0)), span);
            }
            Terminal::Sum => {
                let zero = self.zero_of(self.place_ty(&dest));
                self.assign(block, dest.clone(), Rvalue::Use(zero), span);
            }
            Terminal::Product => {
                let one = self.one_of(self.place_ty(&dest));
                self.assign(block, dest.clone(), Rvalue::Use(one), span);
            }
            // The accumulator starts as what the author wrote, evaluated once
            // and before the loop, as every closure is.
            Terminal::Reduce { initial, .. } => {
                block = self.expr_into(dest.clone(), initial, block);
            }
            // `last` and the extremes hold `null` until an item replaces it,
            // which is what an empty chain answers.
            Terminal::Last | Terminal::Minimum(_) | Terminal::Maximum(_) => {
                let null = Operand::Const(Constant::Literal(Literal::Null));
                self.assign(block, dest.clone(), Rvalue::Use(null), span);
            }
            Terminal::HasAny(_)
            | Terminal::HasAll(_)
            | Terminal::First
            | Terminal::Find(_)
            | Terminal::Each { .. } => {}
        }
        // What `minimum` and `maximum` carry between items: whether one has
        // been seen, and the key it had.
        let extreme = if matches!(terminal, Terminal::Minimum(_) | Terminal::Maximum(_)) {
            let seen = self.temp(self.bool_ty, span, block);
            self.chain_assign_bool(&Place::local(seen), false, block, span);
            let best = self.temp(int_ty, span, block);
            self.assign(block, Place::local(best), Rvalue::Use(Self::bits(0)), span);
            Some((seen, best))
        } else {
            None
        };
        // The barrier's parallel key array, built the same way and in step
        // with the buffer, so that index `i` of one describes index `i` of
        // the other for `science_array_sort_by_int_key`.
        let mut key_closure = None;
        if let Some((keys, key)) = &keys {
            block = self.emit_call(
                keys.clone(),
                Callee::Runtime(ARRAY_WITH_CAPACITY),
                vec![Operand::Const(Constant::Count(0))],
                block,
                span,
            );
            let ty = self.thir.expr(*key).ty;
            let temp = self.temp(ty, span, block);
            block = self.expr_into(Place::local(temp), *key, block);
            key_closure = Some((temp, ty));
        }

        // §2.3: *"a chain value **is** a borrow of its source"*, and the loan
        // is taken before the loop and read on every turn for exactly the
        // reason `lower_for_over_array`'s is — without it,
        // `for` rule 4 has nothing to conflict with and a `push` that
        // reallocates leaves every element reference dangling.
        let array_ty = self.place_ty(source);
        let borrowed = self.context.types.borrowed(false, array_ty);
        let reference = self.temp(borrowed, span, block);
        let length = self.temp(int_ty, span, block);
        if !drain {
            block = self.borrow_place(
                Place::local(reference),
                false,
                source.clone(),
                block,
                span,
                false,
            );
            let extent = if kind == Source::Array { ARRAY_LEN } else { MAP_EXTENT };
            block = self.emit_call(
                Place::local(length),
                Callee::Runtime(extent),
                vec![Operand::Copy(Place::local(reference))],
                block,
                span,
            );
        }
        // What a drain pops into: the owned-`T?` slot `Array.pop` answers in.
        let drain_slot = if drain {
            let slot_ty = self.context.types.nullable(element_ty);
            Some(self.temp(slot_ty, span, block))
        } else {
            None
        };
        // A map is walked by entry, and an entry can be a hole (a removed
        // pair): the slot `science_map_entry_at` answers in, and its test.
        // Outside the loop for `lower_for_over_map`'s reason.
        let (entry_slot, entry_present) = if matches!(kind, Source::Array | Source::Drain) {
            (None, None)
        } else {
            let entry_ty = self.map_entry_ty(source);
            let item = self.context.types.borrowed(false, entry_ty);
            let slot_ty = self.context.types.nullable(item);
            (
                Some((self.temp(slot_ty, span, block), item, entry_ty)),
                Some(self.temp(self.bool_ty, span, block)),
            )
        };

        // **Every closure is evaluated once, before the loop.** A closure
        // expression allocates its capture set, and building it per turn
        // would be both slower and wrong for a capture that owns anything.
        // `take` and `skip` get a limit and a counter the same way, and the
        // terminal's own predicate is evaluated beside the links'.
        let mut closures: Vec<Option<(Local, Ty)>> = Vec::with_capacity(steps.len());
        let mut limits: Vec<Option<(Local, Local)>> = Vec::with_capacity(steps.len());
        // A `zip`'s second array: the loan on it, its type and its element's.
        let mut zips: Vec<Option<(Local, Ty, Ty)>> = vec![None; steps.len()];
        // A `batches`' count of what the batch holds and the buffer's length.
        let mut batch_state: Vec<Option<(Local, Local)>> = vec![None; steps.len()];
        for (step, item_after) in steps {
            match step {
                // The keys seen so far, in a `Set` made before the loop.
                Step::Unique => {
                    let key_ty = match *self.context.types.kind(*item_after) {
                        TyKind::Borrowed { inner, .. } => inner,
                        _ => *item_after,
                    };
                    let seen = match self.set_of(key_ty) {
                        Some(set_ty) => {
                            let seen = self.temp(set_ty, span, block);
                            let callee = match self.type_method(set_ty, "new", Form::Type) {
                                Some(def) => Callee::Def { def, self_ty: Some(set_ty) },
                                None => Callee::Unresolved(Unresolved::Chain),
                            };
                            block = self.emit_call(Place::local(seen), callee, vec![], block, span);
                            seen
                        }
                        None => return block,
                    };
                    closures.push(None);
                    limits.push(Some((seen, seen)));
                }
                // The other array is borrowed for the chain's life, as the
                // source is, and read at the position each item reaches the
                // link — which is not the source's cursor after a filter.
                Step::Zip(other) => {
                    let (place, next) = self.chain_source(*other, block, span);
                    block = next;
                    let other_ty = self.place_ty(&place);
                    let other_element = self.element_ty(&place);
                    let borrowed = self.context.types.borrowed(false, other_ty);
                    let reference = self.temp(borrowed, span, block);
                    block = self.borrow_place(
                        Place::local(reference),
                        false,
                        place,
                        block,
                        span,
                        false,
                    );
                    let other_length = self.temp(int_ty, span, block);
                    block = self.emit_call(
                        Place::local(other_length),
                        Callee::Runtime(ARRAY_LEN),
                        vec![Operand::Copy(Place::local(reference))],
                        block,
                        span,
                    );
                    let counted = self.temp(int_ty, span, block);
                    self.assign(block, Place::local(counted), Rvalue::Use(Self::bits(0)), span);
                    zips[closures.len()] = Some((reference, other_ty, other_element));
                    closures.push(None);
                    limits.push(Some((other_length, counted)));
                }
                Step::Discard(arg)
                | Step::Keep(arg)
                | Step::Map(arg)
                | Step::Sorted(arg)
                | Step::TakeWhile(arg) => {
                    let ty = self.thir.expr(*arg).ty;
                    let temp = self.temp(ty, span, block);
                    block = self.expr_into(Place::local(temp), *arg, block);
                    closures.push(Some((temp, ty)));
                    limits.push(None);
                }
                // A closure and a flag: still swallowing, until the first
                // item `p` does not hold for.
                Step::SkipWhile(arg) => {
                    let ty = self.thir.expr(*arg).ty;
                    let temp = self.temp(ty, span, block);
                    block = self.expr_into(Place::local(temp), *arg, block);
                    closures.push(Some((temp, ty)));
                    let skipping = self.temp(self.bool_ty, span, block);
                    self.chain_assign_bool(&Place::local(skipping), true, block, span);
                    limits.push(Some((skipping, skipping)));
                }
                Step::KeepSome
                | Step::Reverse
                | Step::Flatten
                | Step::FollowedBy(_)
                | Step::Owned => {
                    closures.push(None);
                    limits.push(None);
                }
                // The state, from what the author wrote, and the fold's
                // closure: both evaluated once, before the loop.
                Step::Accumulate(initial, f) => {
                    let state_ty = self.thir.expr(*initial).ty;
                    let state = self.temp(state_ty, span, block);
                    block = self.expr_into(Place::local(state), *initial, block);
                    let ty = self.thir.expr(*f).ty;
                    let temp = self.temp(ty, span, block);
                    block = self.expr_into(Place::local(temp), *f, block);
                    closures.push(Some((temp, ty)));
                    limits.push(Some((state, state)));
                }
                // The position the next item arrives at.
                Step::Numbered => {
                    let counted = self.temp(int_ty, span, block);
                    self.assign(block, Place::local(counted), Rvalue::Use(Self::bits(0)), span);
                    closures.push(None);
                    limits.push(Some((counted, counted)));
                }
                // The size, the batch being filled, how many it holds, and how
                // many items the buffer has in all.
                Step::Batches(arg) => {
                    let limit = self.temp(int_ty, span, block);
                    block = self.expr_into(Place::local(limit), *arg, block);
                    let TyKind::Named { args, .. } = self.context.types.kind(*item_after).clone()
                    else {
                        return block;
                    };
                    let Some(element) = args.first().and_then(|arg| arg.as_type()) else {
                        return block;
                    };
                    let batch_ty = self.array_of(element);
                    let batch = self.temp(batch_ty, span, block);
                    block = self.emit_call(
                        Place::local(batch),
                        Callee::Runtime(ARRAY_WITH_CAPACITY),
                        vec![Operand::Const(Constant::Count(0))],
                        block,
                        span,
                    );
                    let held = self.temp(int_ty, span, block);
                    self.assign(block, Place::local(held), Rvalue::Use(Self::bits(0)), span);
                    let source_ty = self.place_ty(source);
                    let shared_ty = self.context.types.borrowed(false, source_ty);
                    let shared = self.temp(shared_ty, span, block);
                    block = self.borrow_place(
                        Place::local(shared),
                        false,
                        source.clone(),
                        block,
                        span,
                        false,
                    );
                    let total = self.temp(int_ty, span, block);
                    block = self.emit_call(
                        Place::local(total),
                        Callee::Runtime(ARRAY_LEN),
                        vec![Operand::Copy(Place::local(shared))],
                        block,
                        span,
                    );
                    batch_state[closures.len()] = Some((held, total));
                    closures.push(None);
                    limits.push(Some((limit, batch)));
                }
                // The width, and the ring of the last `width` items.
                Step::Windows(arg) => {
                    let limit = self.temp(int_ty, span, block);
                    block = self.expr_into(Place::local(limit), *arg, block);
                    let TyKind::Named { args, .. } = self.context.types.kind(*item_after).clone()
                    else {
                        return block;
                    };
                    let Some(element) = args.first().and_then(|arg| arg.as_type()) else {
                        return block;
                    };
                    let ring_ty = self.array_of(element);
                    let ring = self.temp(ring_ty, span, block);
                    block = self.emit_call(
                        Place::local(ring),
                        Callee::Runtime(ARRAY_WITH_CAPACITY),
                        vec![Operand::Const(Constant::Count(0))],
                        block,
                        span,
                    );
                    closures.push(None);
                    limits.push(Some((limit, ring)));
                }
                Step::Take(arg) | Step::Skip(arg) | Step::Every(arg) => {
                    let limit = self.temp(int_ty, span, block);
                    block = self.expr_into(Place::local(limit), *arg, block);
                    let counted = self.temp(int_ty, span, block);
                    self.assign(block, Place::local(counted), Rvalue::Use(Self::bits(0)), span);
                    closures.push(None);
                    limits.push(Some((limit, counted)));
                }
            }
        }
        let mut predicate = None;
        if let Some(arg) = terminal.predicate() {
            let ty = self.thir.expr(arg).ty;
            let temp = self.temp(ty, span, block);
            block = self.expr_into(Place::local(temp), arg, block);
            predicate = Some((temp, ty));
        }

        let cursor = self.temp(int_ty, span, block);
        self.assign(block, Place::local(cursor), Rvalue::Use(Self::bits(0)), span);

        let head = self.new_block();
        let body_block = self.new_block();
        let step_block = self.new_block();
        // An array's element is read in the body; a map's entry is fetched
        // first, and a hole goes straight to the increment.
        let fetch =
            if matches!(kind, Source::Array | Source::Drain) { body_block } else { self.new_block() };
        // Two ends, not one: `exhausted` is reached when the source or a
        // `take` runs out, and `done` also from a terminal that has its
        // answer early. `exhausted` writes the answer running out means —
        // `false` for `has_any`, `null` for `find` — and falls into `done`.
        let exhausted = self.new_block();
        let done = self.new_block();
        self.terminate(block, TerminatorKind::Goto { target: head }, span);

        if let Some(slot) = drain_slot {
            // `slot = buffer.pop()`; absent is the end of the chain.
            let (buffer_ref, popping) = self.exclusive_ref(source, head, span);
            let callee = match self.array_method(array_ty, "pop") {
                Some(def) => Callee::Def { def, self_ty: None },
                None => Callee::Unresolved(Unresolved::Chain),
            };
            let popped = self.emit_call(Place::local(slot), callee, vec![buffer_ref], popping, span);
            let present = self.temp(self.bool_ty, span, popped);
            self.assign(
                popped,
                Place::local(present),
                Rvalue::IsPresent(Operand::Copy(Place::local(slot))),
                span,
            );
            self.terminate(
                popped,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(present)),
                    then_block: body_block,
                    else_block: exhausted,
                },
                span,
            );
        }
        // `cursor < length`, compared unsigned, which is
        // `lower_for_over_array`'s own widening and its reason: a negative
        // cursor is not reachable and the comparison should not depend on it.
        if !drain {
            let wide_cursor = self.temp(u64_ty, span, head);
            self.assign(
                head,
                Place::local(wide_cursor),
                Rvalue::Cast { operand: Operand::Copy(Place::local(cursor)), from: int_ty, ty: u64_ty },
                span,
            );
            let wide_length = self.temp(u64_ty, span, head);
            self.assign(
                head,
                Place::local(wide_length),
                Rvalue::Cast { operand: Operand::Copy(Place::local(length)), from: int_ty, ty: u64_ty },
                span,
            );
            let more = self.temp(self.bool_ty, span, head);
            self.assign(
                head,
                Place::local(more),
                Rvalue::Binary {
                    op: BinaryOp::Lt,
                    lhs: Operand::Copy(Place::local(wide_cursor)),
                    rhs: Operand::Copy(Place::local(wide_length)),
                },
                span,
            );
            self.terminate(
                head,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(more)),
                    then_block: fetch,
                    else_block: exhausted,
                },
                span,
            );
        }
        if let (Some((slot, _, _)), Some(present)) = (entry_slot, entry_present) {
            let at = self.temp(int_ty, span, fetch);
            self.assign(
                fetch,
                Place::local(at),
                Rvalue::Use(Operand::Copy(Place::local(cursor))),
                span,
            );
            let fetched = self.emit_call(
                Place::local(slot),
                Callee::Runtime(MAP_ENTRY_AT),
                vec![Operand::Copy(Place::local(reference)), Operand::Copy(Place::local(at))],
                fetch,
                span,
            );
            self.assign(
                fetched,
                Place::local(present),
                Rvalue::IsPresent(Operand::Copy(Place::local(slot))),
                span,
            );
            self.terminate(
                fetched,
                TerminatorKind::If {
                    cond: Operand::Copy(Place::local(present)),
                    then_block: body_block,
                    else_block: step_block,
                },
                span,
            );
        }

        // The item's scope. Everything the body makes is registered here.
        self.push_scope();
        let depth = self.scopes.len() - 1;

        let mut current = body_block;
        let mut value;
        if let Some(slot) = drain_slot {
            // The item the pop found, moved out of its slot when it owns
            // something: from here it is the chain's, as a `map`'s result is.
            let slot_ty = self.place_ty(&Place::local(slot));
            let narrowed = self.temp(element_ty, span, body_block);
            let read = self.read(Place::local(slot), slot_ty);
            self.assign(
                body_block,
                Place::local(narrowed),
                Rvalue::Narrow { operand: read, ty: element_ty },
                span,
            );
            value = Place::local(narrowed);
        } else if let Some((slot, entry_ref_ty, entry_ty)) = entry_slot {
            // The entry the fetch found, narrowed out of its slot: a borrow,
            // so the read is a copy and nothing here owns anything.
            let narrowed = self.temp(entry_ref_ty, span, body_block);
            let read = self.read(Place::local(slot), entry_ref_ty);
            self.assign(
                body_block,
                Place::local(narrowed),
                Rvalue::Narrow { operand: read, ty: entry_ref_ty },
                span,
            );
            value = Place::local(narrowed);
            if kind != Source::MapEntries {
                // `keys()` and `values()` hand out a borrow of one field of
                // the entry, reached through the entry's own borrow.
                let fields = crate::moves::record_fields(
                    self.context.decls,
                    self.context.types,
                    self.context.aliases,
                    entry_ty,
                )
                .expect("`Entry` is a record with two fields");
                let (field, field_ty) = fields[if kind == Source::MapKeys { 0 } else { 1 }];
                let element = Place::local(narrowed)
                    .project(Projection::Deref { ty: entry_ty })
                    .project(Projection::Field { field, ty: field_ty });
                let item_ty = self.context.types.borrowed(false, field_ty);
                let item = self.temp(item_ty, span, body_block);
                current =
                    self.borrow_place(Place::local(item), false, element, current, span, false);
                value = Place::local(item);
            }
        } else {
            // The element, reached *through* the loan, at an index temporary
            // the body assigns once — `Projection::Index`'s own contract.
            let at = self.temp(int_ty, span, body_block);
            self.assign(
                body_block,
                Place::local(at),
                Rvalue::Use(Operand::Copy(Place::local(cursor))),
                span,
            );
            let through = Place::local(reference).project(Projection::Deref { ty: array_ty });
            let element = through.project(Projection::Index { index: at, ty: element_ty });
            let item_ty = self.context.types.borrowed(false, element_ty);
            let item = self.temp(item_ty, span, body_block);
            current = self.borrow_place(Place::local(item), false, element, current, span, false);
            value = Place::local(item);
        }

        for (index, (step, item_after)) in steps.iter().enumerate() {
            match step {
                Step::Discard(_) | Step::Keep(_) | Step::TakeWhile(_) => {
                    let (closure, closure_ty) =
                        closures[index].expect("a filter carries a closure");
                    let (argument, next) =
                        self.chain_argument(&value, closure_ty, false, current, span);
                    current = next;
                    let matched = self.temp(self.bool_ty, span, current);
                    current = self.emit_call(
                        Place::local(matched),
                        Callee::Indirect(Operand::Copy(Place::local(closure))),
                        vec![argument],
                        current,
                        span,
                    );
                    // §3.1: `discard` drops what matches, so a `true` is the
                    // *skip*; `keep` is the same test the other way round.
                    // That direction is the whole reason the note carries two
                    // verbs instead of one and a negation.
                    let (pass, skip) = (self.new_block(), self.new_block());
                    //
                    // `take_while` is `keep` whose first miss is not a skip
                    // but the end: it leaves the loop as a `take` at its
                    // limit does, and nothing after it is read.
                    let (then_block, else_block) = match step {
                        Step::Discard(_) => (skip, pass),
                        _ => (pass, skip),
                    };
                    let onward = if matches!(step, Step::TakeWhile(_)) { exhausted } else { step_block };
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(matched)),
                            then_block,
                            else_block,
                        },
                        span,
                    );
                    self.chain_leave(skip, depth, onward, span);
                    current = pass;
                }
                Step::SkipWhile(_) => {
                    let (closure, closure_ty) =
                        closures[index].expect("a `skip_while` carries a closure");
                    let (skipping, _) = limits[index].expect("a `skip_while` carries its flag");
                    // Still swallowing? Only then is the predicate asked.
                    let (ask, through) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(skipping)),
                            then_block: ask,
                            else_block: through,
                        },
                        span,
                    );
                    let (argument, next) =
                        self.chain_argument(&value, closure_ty, false, ask, span);
                    let matched = self.temp(self.bool_ty, span, next);
                    let asked = self.emit_call(
                        Place::local(matched),
                        Callee::Indirect(Operand::Copy(Place::local(closure))),
                        vec![argument],
                        next,
                        span,
                    );
                    let (swallow, begin) = (self.new_block(), self.new_block());
                    self.terminate(
                        asked,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(matched)),
                            then_block: swallow,
                            else_block: begin,
                        },
                        span,
                    );
                    self.chain_leave(swallow, depth, step_block, span);
                    // The first item it does not hold for ends the skipping
                    // and is itself the first to pass.
                    self.chain_assign_bool(&Place::local(skipping), false, begin, span);
                    self.terminate(begin, TerminatorKind::Goto { target: through }, span);
                    current = through;
                }
                Step::Accumulate(..) => {
                    let (closure, closure_ty) =
                        closures[index].expect("an `accumulate` carries its closure");
                    let (state, _) = limits[index].expect("an `accumulate` carries its state");
                    let state_ty = *item_after;
                    let (item, next) =
                        self.chain_argument_at(&value, closure_ty, true, 1, current, span);
                    current = next;
                    let folded = self.temp(state_ty, span, current);
                    current = self.emit_call(
                        Place::local(folded),
                        Callee::Indirect(Operand::Copy(Place::local(closure))),
                        vec![Operand::Copy(Place::local(state)), item],
                        current,
                        span,
                    );
                    self.assign(
                        current,
                        Place::local(state),
                        Rvalue::Use(Operand::Move(Place::local(folded))),
                        span,
                    );
                    // The chain yields a copy of the state, which owns nothing
                    // (the checker has said so), and the state stays for the
                    // next turn.
                    let yielded = self.temp(state_ty, span, current);
                    self.assign(
                        current,
                        Place::local(yielded),
                        Rvalue::Use(Operand::Copy(Place::local(state))),
                        span,
                    );
                    value = Place::local(yielded);
                }
                Step::Batches(_) => {
                    debug_assert!(drain, "a `batches` is read from a buffer");
                    let (limit, batch) = limits[index].expect("a `batches` carries its batch");
                    let (held, total) = batch_state[index].expect("a `batches` carries its count");
                    // The item joins the batch...
                    let (batch_ref, next) = self.exclusive_ref(&Place::local(batch), current, span);
                    current = next;
                    let discard = Place::local(self.temp(Ty::UNIT, span, current));
                    current = self.emit_call(
                        discard,
                        Callee::Runtime(ARRAY_PUSH),
                        vec![batch_ref, Operand::Move(value.clone())],
                        current,
                        span,
                    );
                    self.chain_add(&Place::local(held), Self::bits(1), current, span);
                    // ...which is handed on when it is full, or when this was
                    // the last item the buffer had. A size below one is a
                    // batch of one.
                    let full = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(full),
                        Rvalue::Binary {
                            op: BinaryOp::Ge,
                            lhs: Operand::Copy(Place::local(held)),
                            rhs: Operand::Copy(Place::local(limit)),
                        },
                        span,
                    );
                    let (flush, check_last) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(full)),
                            then_block: flush,
                            else_block: check_last,
                        },
                        span,
                    );
                    let consumed = self.temp(int_ty, span, check_last);
                    self.assign(
                        check_last,
                        Place::local(consumed),
                        Rvalue::Binary {
                            op: BinaryOp::Add,
                            lhs: Operand::Copy(Place::local(cursor)),
                            rhs: Self::bits(1),
                        },
                        span,
                    );
                    let last = self.temp(self.bool_ty, span, check_last);
                    self.assign(
                        check_last,
                        Place::local(last),
                        Rvalue::Binary {
                            op: BinaryOp::Ge,
                            lhs: Operand::Copy(Place::local(consumed)),
                            rhs: Operand::Copy(Place::local(total)),
                        },
                        span,
                    );
                    let wait = self.new_block();
                    self.terminate(
                        check_last,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(last)),
                            then_block: flush,
                            else_block: wait,
                        },
                        span,
                    );
                    self.chain_leave(wait, depth, step_block, span);
                    let handed = self.temp(*item_after, span, flush);
                    self.assign(
                        flush,
                        Place::local(handed),
                        Rvalue::Use(Operand::Move(Place::local(batch))),
                        span,
                    );
                    current = self.emit_call(
                        Place::local(batch),
                        Callee::Runtime(ARRAY_WITH_CAPACITY),
                        vec![Operand::Const(Constant::Count(0))],
                        flush,
                        span,
                    );
                    self.assign(current, Place::local(held), Rvalue::Use(Self::bits(0)), span);
                    value = Place::local(handed);
                }
                Step::Windows(_) => {
                    let (limit, ring) = limits[index].expect("a `windows` carries its ring");
                    let ring_ty = self.place_ty(&Place::local(ring));
                    // The item joins the ring...
                    let (ring_ref, next) = self.exclusive_ref(&Place::local(ring), current, span);
                    current = next;
                    let discard = Place::local(self.temp(Ty::UNIT, span, current));
                    current = self.emit_call(
                        discard,
                        Callee::Runtime(ARRAY_PUSH),
                        vec![ring_ref, Operand::Move(value.clone())],
                        current,
                        span,
                    );
                    // ...and the oldest leaves it once the ring is wider than
                    // the window.
                    let shared_ty = self.context.types.borrowed(false, ring_ty);
                    let shared = self.temp(shared_ty, span, current);
                    current = self.borrow_place(
                        Place::local(shared),
                        false,
                        Place::local(ring),
                        current,
                        span,
                        false,
                    );
                    let length = self.temp(int_ty, span, current);
                    current = self.emit_call(
                        Place::local(length),
                        Callee::Runtime(ARRAY_LEN),
                        vec![Operand::Copy(Place::local(shared))],
                        current,
                        span,
                    );
                    let too_wide = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(too_wide),
                        Rvalue::Binary {
                            op: BinaryOp::Gt,
                            lhs: Operand::Copy(Place::local(length)),
                            rhs: Operand::Copy(Place::local(limit)),
                        },
                        span,
                    );
                    let (trim, trimmed) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(too_wide)),
                            then_block: trim,
                            else_block: trimmed,
                        },
                        span,
                    );
                    let (ring_ref, after_ref) = self.exclusive_ref(&Place::local(ring), trim, span);
                    let ring_element = self.element_ty(&Place::local(ring));
                    let oldest_ty = self.context.types.nullable(ring_element);
                    let oldest = self.temp(oldest_ty, span, after_ref);
                    let callee = match self.array_method(ring_ty, "remove") {
                        Some(def) => Callee::Def { def, self_ty: None },
                        None => Callee::Unresolved(Unresolved::Chain),
                    };
                    let removed = self.emit_call(
                        Place::local(oldest),
                        callee,
                        vec![ring_ref, Self::bits(0)],
                        after_ref,
                        span,
                    );
                    self.terminate(removed, TerminatorKind::Goto { target: trimmed }, span);
                    // Full: `length >= width` before the trim, which is `width`
                    // items after it. A window of nothing is not a window, so a
                    // width below one yields none.
                    let positive = self.temp(self.bool_ty, span, trimmed);
                    self.assign(
                        trimmed,
                        Place::local(positive),
                        Rvalue::Binary {
                            op: BinaryOp::Gt,
                            lhs: Operand::Copy(Place::local(limit)),
                            rhs: Self::bits(0),
                        },
                        span,
                    );
                    let (asked, none) = (self.new_block(), self.new_block());
                    self.terminate(
                        trimmed,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(positive)),
                            then_block: asked,
                            else_block: none,
                        },
                        span,
                    );
                    self.chain_leave(none, depth, step_block, span);
                    let full = self.temp(self.bool_ty, span, asked);
                    self.assign(
                        asked,
                        Place::local(full),
                        Rvalue::Binary {
                            op: BinaryOp::Ge,
                            lhs: Operand::Copy(Place::local(length)),
                            rhs: Operand::Copy(Place::local(limit)),
                        },
                        span,
                    );
                    let (yield_block, wait) = (self.new_block(), self.new_block());
                    self.terminate(
                        asked,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(full)),
                            then_block: yield_block,
                            else_block: wait,
                        },
                        span,
                    );
                    self.chain_leave(wait, depth, step_block, span);
                    // The window is a copy of the ring, so the ring goes on.
                    let shared = self.temp(shared_ty, span, yield_block);
                    let cloning = self.borrow_place(
                        Place::local(shared),
                        false,
                        Place::local(ring),
                        yield_block,
                        span,
                        true,
                    );
                    let window = self.temp(*item_after, span, cloning);
                    let callee = match self.array_method(ring_ty, "clone") {
                        Some(def) => Callee::Def { def, self_ty: None },
                        None => Callee::Unresolved(Unresolved::Chain),
                    };
                    current = self.emit_call(
                        Place::local(window),
                        callee,
                        vec![Operand::Move(Place::local(shared))],
                        cloning,
                        span,
                    );
                    value = Place::local(window);
                }
                Step::Owned => {
                    let owned_ty = *item_after;
                    let item_ty = self.place_ty(&value);
                    let TyKind::Borrowed { inner, .. } = *self.context.types.kind(item_ty) else {
                        unreachable!("`owned()` reads a chain of borrows")
                    };
                    let copy = self.temp(owned_ty, span, current);
                    if self.is_copy(owned_ty) {
                        self.assign(
                            current,
                            Place::local(copy),
                            Rvalue::Use(Operand::Copy(
                                value.clone().project(Projection::Deref { ty: inner }),
                            )),
                            span,
                        );
                    } else {
                        let callee = match self.type_method(owned_ty, "clone", Form::Value) {
                            Some(def) => Callee::Def { def, self_ty: None },
                            None => Callee::Unresolved(Unresolved::Chain),
                        };
                        current = self.emit_call(
                            Place::local(copy),
                            callee,
                            vec![Operand::Copy(value.clone())],
                            current,
                            span,
                        );
                    }
                    value = Place::local(copy);
                }
                Step::Unique => {
                    let (seen, _) = limits[index].expect("a `unique` carries its set");
                    let item_ty = self.place_ty(&value);
                    // The key is the item read through one borrow.
                    let (key_ty, source, through_borrow) = match *self.context.types.kind(item_ty) {
                        TyKind::Borrowed { inner, .. } => {
                            (inner, value.clone().project(Projection::Deref { ty: inner }), true)
                        }
                        _ => (item_ty, value.clone(), false),
                    };
                    let key = self.temp(key_ty, span, current);
                    if self.is_copy(key_ty) {
                        self.assign(
                            current,
                            Place::local(key),
                            Rvalue::Use(Operand::Copy(source)),
                            span,
                        );
                    } else {
                        // A `String`: an owned copy, since the set owns what it
                        // files and the chain keeps the item.
                        let shared = if through_borrow {
                            Operand::Copy(value.clone())
                        } else {
                            let borrowed = self.context.types.borrowed(false, key_ty);
                            let temp = self.temp(borrowed, span, current);
                            current = self.borrow_place(
                                Place::local(temp),
                                false,
                                value.clone(),
                                current,
                                span,
                                true,
                            );
                            Operand::Move(Place::local(temp))
                        };
                        let callee = match self.type_method(key_ty, "clone", Form::Value) {
                            Some(def) => Callee::Def { def, self_ty: None },
                            None => Callee::Unresolved(Unresolved::Chain),
                        };
                        current =
                            self.emit_call(Place::local(key), callee, vec![shared], current, span);
                    }
                    let set_ty = self.place_ty(&Place::local(seen));
                    let (set_ref, next) = self.exclusive_ref(&Place::local(seen), current, span);
                    current = next;
                    let fresh = self.temp(self.bool_ty, span, current);
                    let callee = match self.type_method(set_ty, "insert", Form::Value) {
                        Some(def) => Callee::Def { def, self_ty: None },
                        None => Callee::Unresolved(Unresolved::Chain),
                    };
                    current = self.emit_call(
                        Place::local(fresh),
                        callee,
                        vec![set_ref, Operand::Move(Place::local(key))],
                        current,
                        span,
                    );
                    let (pass, skip) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(fresh)),
                            then_block: pass,
                            else_block: skip,
                        },
                        span,
                    );
                    self.chain_leave(skip, depth, step_block, span);
                    current = pass;
                }
                Step::KeepSome => {
                    // The item is a `T?`, or a borrow of one over a `T` that
                    // owns nothing (the checker's `chain_item_shape` says
                    // which); either way the test reads the nullable itself.
                    let nullable = match *self.context.types.kind(self.place_ty(&value)) {
                        TyKind::Borrowed { inner, .. } => {
                            (value.clone().project(Projection::Deref { ty: inner }), true)
                        }
                        _ => (value.clone(), false),
                    };
                    let (slot, through_a_borrow) = nullable;
                    let present = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(present),
                        Rvalue::IsPresent(Operand::Copy(slot.clone())),
                        span,
                    );
                    let (some, none) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(present)),
                            then_block: some,
                            else_block: none,
                        },
                        span,
                    );
                    self.chain_leave(none, depth, step_block, span);
                    let payload_ty = *item_after;
                    let narrowed = self.temp(payload_ty, span, some);
                    let slot_ty = self.place_ty(&slot);
                    // Moved when the payload owns something, which only an
                    // owned `T?` can: a borrow's payload owns nothing.
                    let read = if through_a_borrow {
                        Operand::Copy(slot)
                    } else {
                        self.read(slot, slot_ty)
                    };
                    self.assign(
                        some,
                        Place::local(narrowed),
                        Rvalue::Narrow { operand: read, ty: payload_ty },
                        span,
                    );
                    value = Place::local(narrowed);
                    current = some;
                }
                Step::Numbered => {
                    let (counted, _) = limits[index].expect("a `numbered` carries its counter");
                    let position = self.temp(int_ty, span, current);
                    self.assign(
                        current,
                        Place::local(position),
                        Rvalue::Use(Operand::Copy(Place::local(counted))),
                        span,
                    );
                    self.chain_add(&Place::local(counted), Self::bits(1), current, span);
                    let record_ty = *item_after;
                    let fields = crate::moves::record_fields(
                        self.context.decls,
                        self.context.types,
                        self.context.aliases,
                        record_ty,
                    )
                    .expect("`Numbered` is a record with two fields");
                    let TyKind::Named { def, .. } = *self.context.types.kind(record_ty) else {
                        unreachable!("`numbered()` yields a `Numbered`")
                    };
                    let carried = self.chain_operand(&value, true);
                    let made = self.temp(record_ty, span, current);
                    self.assign(
                        current,
                        Place::local(made),
                        Rvalue::Record {
                            def,
                            fields: vec![
                                (fields[0].0, Operand::Copy(Place::local(position))),
                                (fields[1].0, carried),
                            ],
                        },
                        span,
                    );
                    value = Place::local(made);
                }
                Step::Zip(_) => {
                    let (length, counted) = limits[index].expect("a `zip` carries its position");
                    let (reference, other_ty, other_element) =
                        zips[index].expect("a `zip` carries its second array");
                    // The other array running out ends the chain, as a `take`
                    // at its limit does. The item in hand is released by the
                    // exit, because nothing took it.
                    let remaining = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(remaining),
                        Rvalue::Binary {
                            op: BinaryOp::Lt,
                            lhs: Operand::Copy(Place::local(counted)),
                            rhs: Operand::Copy(Place::local(length)),
                        },
                        span,
                    );
                    let (paired, over) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(remaining)),
                            then_block: paired,
                            else_block: over,
                        },
                        span,
                    );
                    self.chain_leave(over, depth, exhausted, span);
                    let at = self.temp(int_ty, span, paired);
                    self.assign(
                        paired,
                        Place::local(at),
                        Rvalue::Use(Operand::Copy(Place::local(counted))),
                        span,
                    );
                    let through =
                        Place::local(reference).project(Projection::Deref { ty: other_ty });
                    let element =
                        through.project(Projection::Index { index: at, ty: other_element });
                    let right_ty = self.context.types.borrowed(false, other_element);
                    let right = self.temp(right_ty, span, paired);
                    current = self.borrow_place(
                        Place::local(right),
                        false,
                        element,
                        paired,
                        span,
                        false,
                    );
                    self.chain_add(&Place::local(counted), Self::bits(1), current, span);
                    let record_ty = *item_after;
                    let fields = crate::moves::record_fields(
                        self.context.decls,
                        self.context.types,
                        self.context.aliases,
                        record_ty,
                    )
                    .expect("`Pair` is a record with two fields");
                    let TyKind::Named { def, .. } = *self.context.types.kind(record_ty) else {
                        unreachable!("`zip()` yields a `Pair`")
                    };
                    let left = self.chain_operand(&value, true);
                    let made = self.temp(record_ty, span, current);
                    self.assign(
                        current,
                        Place::local(made),
                        Rvalue::Record {
                            def,
                            fields: vec![
                                (fields[0].0, left),
                                (fields[1].0, Operand::Copy(Place::local(right))),
                            ],
                        },
                        span,
                    );
                    value = Place::local(made);
                }
                Step::Every(_) => {
                    let (limit, phase) = limits[index].expect("an `every` carries a count");
                    // The first item of each run of `n` passes. The phase is
                    // stepped on every item and wraps at the limit, so an
                    // `n` below two passes everything.
                    let at_start = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(at_start),
                        Rvalue::Binary {
                            op: BinaryOp::Eq,
                            lhs: Operand::Copy(Place::local(phase)),
                            rhs: Self::bits(0),
                        },
                        span,
                    );
                    self.chain_add(&Place::local(phase), Self::bits(1), current, span);
                    let wrapped = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(wrapped),
                        Rvalue::Binary {
                            op: BinaryOp::Ge,
                            lhs: Operand::Copy(Place::local(phase)),
                            rhs: Operand::Copy(Place::local(limit)),
                        },
                        span,
                    );
                    let (wrap, settled) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(wrapped)),
                            then_block: wrap,
                            else_block: settled,
                        },
                        span,
                    );
                    self.assign(wrap, Place::local(phase), Rvalue::Use(Self::bits(0)), span);
                    self.terminate(wrap, TerminatorKind::Goto { target: settled }, span);
                    let (pass, skip) = (self.new_block(), self.new_block());
                    self.terminate(
                        settled,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(at_start)),
                            then_block: pass,
                            else_block: skip,
                        },
                        span,
                    );
                    self.chain_leave(skip, depth, step_block, span);
                    current = pass;
                }
                Step::Map(_) => {
                    let (closure, closure_ty) = closures[index].expect("a map carries a closure");
                    let (argument, next) =
                        self.chain_argument(&value, closure_ty, true, current, span);
                    current = next;
                    let mapped = self.temp(*item_after, span, current);
                    current = self.emit_call(
                        Place::local(mapped),
                        Callee::Indirect(Operand::Copy(Place::local(closure))),
                        vec![argument],
                        current,
                        span,
                    );
                    value = Place::local(mapped);
                }
                Step::Take(_) | Step::Skip(_) => {
                    let (limit, counted) = limits[index].expect("a take or a skip carries a count");
                    let below = self.temp(self.bool_ty, span, current);
                    self.assign(
                        current,
                        Place::local(below),
                        Rvalue::Binary {
                            op: BinaryOp::Lt,
                            lhs: Operand::Copy(Place::local(counted)),
                            rhs: Operand::Copy(Place::local(limit)),
                        },
                        span,
                    );
                    // Below the limit, both count the item; what differs is
                    // which side of the limit the item *passes* on. A `take`
                    // passes what it counts and, at the limit, **leaves the
                    // loop rather than skipping** — the laziness §2.1 is
                    // arguing for: nothing after the nth item is read at all.
                    // A `skip` swallows what it counts and passes everything
                    // after.
                    let (counting, over) = (self.new_block(), self.new_block());
                    self.terminate(
                        current,
                        TerminatorKind::If {
                            cond: Operand::Copy(Place::local(below)),
                            then_block: counting,
                            else_block: over,
                        },
                        span,
                    );
                    let stepped = self.temp(int_ty, span, counting);
                    self.assign(
                        counting,
                        Place::local(stepped),
                        Rvalue::Binary {
                            op: BinaryOp::Add,
                            lhs: Operand::Copy(Place::local(counted)),
                            rhs: Self::bits(1),
                        },
                        span,
                    );
                    self.assign(
                        counting,
                        Place::local(counted),
                        Rvalue::Use(Operand::Copy(Place::local(stepped))),
                        span,
                    );
                    if matches!(step, Step::Take(_)) {
                        self.chain_leave(over, depth, exhausted, span);
                        current = counting;
                    } else {
                        self.chain_leave(counting, depth, step_block, span);
                        current = over;
                    }
                }
                Step::Sorted(_) | Step::Reverse | Step::Flatten | Step::FollowedBy(_) => {
                    unreachable!("a barrier is split off before the pass")
                }
            }
        }

        // The terminal, with the item that survived every link in hand.
        match terminal {
            Terminal::Collect => {
                // **The key before the push**, because the push moves the
                // item and the key closure only borrows it: reversing the two
                // would compute a key from a place the array now owns.
                if let (Some((keys, _)), Some(closure)) = (&keys, key_closure) {
                    current = self.chain_push_key(keys, closure, &value, int_ty, current, span);
                }
                let (accumulator, next) = self.accumulator_ref(&dest, current, span);
                current = next;
                let discard = Place::local(self.temp(Ty::UNIT, span, current));
                current = self.emit_call(
                    discard,
                    Callee::Runtime(ARRAY_PUSH),
                    vec![accumulator, Operand::Move(value)],
                    current,
                    span,
                );
                self.chain_leave(current, depth, step_block, span);
            }
            Terminal::Count => {
                // `Array.sort(by:)`'s pass: no buffer, only the keys, one per
                // element and in step with the array it will reorder. See
                // [`Builder::lower_array_sort_by`].
                if let (Some((keys, _)), Some(closure)) = (&keys, key_closure) {
                    current = self.chain_push_key(keys, closure, &value, int_ty, current, span);
                }
                self.chain_add(&dest, Self::bits(1), current, span);
                self.chain_leave(current, depth, step_block, span);
            }
            Terminal::Sum => {
                // §1.5: a chain of borrowed numbers sums to a number. The
                // checker has already said the total is one; the item may
                // still be a borrow of it, and is read through when it is.
                let operand = match *self.context.types.kind(self.place_ty(&value)) {
                    TyKind::Borrowed { inner, .. } => {
                        Operand::Copy(value.project(Projection::Deref { ty: inner }))
                    }
                    _ => Operand::Copy(value),
                };
                self.chain_add(&dest, operand, current, span);
                self.chain_leave(current, depth, step_block, span);
            }
            Terminal::Product => {
                let operand = match *self.context.types.kind(self.place_ty(&value)) {
                    TyKind::Borrowed { inner, .. } => {
                        Operand::Copy(value.project(Projection::Deref { ty: inner }))
                    }
                    _ => Operand::Copy(value),
                };
                self.chain_arith(&dest, BinaryOp::Mul, operand, current, span);
                self.chain_leave(current, depth, step_block, span);
            }
            Terminal::Reduce { .. } => {
                let (closure, closure_ty) = predicate.expect("a reduce closure was evaluated");
                let (item, next) =
                    self.chain_argument_at(&value, closure_ty, true, 1, current, span);
                current = next;
                // The accumulator is moved into the call and the result moved
                // back: `dest` is uninitialised for exactly the call, and
                // drop elaboration sees the re-initialisation.
                let acc_ty = self.place_ty(&dest);
                let acc = if self.is_copy(acc_ty) {
                    Operand::Copy(dest.clone())
                } else {
                    Operand::Move(dest.clone())
                };
                let folded = self.temp(acc_ty, span, current);
                current = self.emit_call(
                    Place::local(folded),
                    Callee::Indirect(Operand::Copy(Place::local(closure))),
                    vec![acc, item],
                    current,
                    span,
                );
                self.assign(
                    current,
                    dest.clone(),
                    Rvalue::Use(Operand::Move(Place::local(folded))),
                    span,
                );
                self.chain_leave(current, depth, step_block, span);
            }
            Terminal::First => {
                self.chain_found(&dest, &value, current, span);
                self.chain_leave(current, depth, done, span);
            }
            Terminal::Last => {
                // Every item replaces the one before it, which is released
                // first: the chain keeps only the final item it saw.
                let replaced = self.chain_replace(&dest, &value, current, span);
                self.chain_leave(replaced, depth, step_block, span);
            }
            Terminal::Minimum(_) | Terminal::Maximum(_) => {
                let (closure, closure_ty) = predicate.expect("a terminal key was evaluated");
                let (seen, best) = extreme.expect("an extreme carries its state");
                let (argument, next) =
                    self.chain_argument(&value, closure_ty, false, current, span);
                current = next;
                let key = self.temp(int_ty, span, current);
                current = self.emit_call(
                    Place::local(key),
                    Callee::Indirect(Operand::Copy(Place::local(closure))),
                    vec![argument],
                    current,
                    span,
                );
                // The first item is the extreme so far. After it, an item
                // replaces the holder only when its key is *strictly* better,
                // so the first of equal keys wins.
                let (compare, take, keep) = (self.new_block(), self.new_block(), self.new_block());
                self.terminate(
                    current,
                    TerminatorKind::If {
                        cond: Operand::Copy(Place::local(seen)),
                        then_block: compare,
                        else_block: take,
                    },
                    span,
                );
                let better = self.temp(self.bool_ty, span, compare);
                let op = if matches!(terminal, Terminal::Minimum(_)) {
                    BinaryOp::Lt
                } else {
                    BinaryOp::Gt
                };
                self.assign(
                    compare,
                    Place::local(better),
                    Rvalue::Binary {
                        op,
                        lhs: Operand::Copy(Place::local(key)),
                        rhs: Operand::Copy(Place::local(best)),
                    },
                    span,
                );
                self.terminate(
                    compare,
                    TerminatorKind::If {
                        cond: Operand::Copy(Place::local(better)),
                        then_block: take,
                        else_block: keep,
                    },
                    span,
                );
                let taken = self.chain_replace(&dest, &value, take, span);
                self.assign(
                    taken,
                    Place::local(best),
                    Rvalue::Use(Operand::Copy(Place::local(key))),
                    span,
                );
                self.chain_assign_bool(&Place::local(seen), true, taken, span);
                self.chain_leave(taken, depth, step_block, span);
                self.chain_leave(keep, depth, step_block, span);
            }
            Terminal::Each { pattern, body } => {
                // A written `for`: the loop's own body is the terminal, with
                // `continue` going to the next item and `break` out of the
                // chain. The pattern binds in the item's scope, so what it
                // owns is released with everything else the item made.
                self.loops.push(LoopScope { head, continue_to: step_block, exit: done, depth });
                let bound = self.bind_pattern(&value, pattern, current);
                let discard = self.temp(Ty::UNIT, span, bound);
                let after = self.lower_block(Place::local(discard), body, bound);
                self.loops.pop();
                self.chain_leave(after, depth, step_block, span);
            }
            Terminal::HasAny(_) | Terminal::HasAll(_) | Terminal::Find(_) => {
                let (closure, closure_ty) = predicate.expect("a terminal predicate was evaluated");
                let (argument, next) =
                    self.chain_argument(&value, closure_ty, false, current, span);
                current = next;
                let matched = self.temp(self.bool_ty, span, current);
                current = self.emit_call(
                    Place::local(matched),
                    Callee::Indirect(Operand::Copy(Place::local(closure))),
                    vec![argument],
                    current,
                    span,
                );
                let (hit, miss) = (self.new_block(), self.new_block());
                self.terminate(
                    current,
                    TerminatorKind::If {
                        cond: Operand::Copy(Place::local(matched)),
                        then_block: hit,
                        else_block: miss,
                    },
                    span,
                );
                // `has_all` is decided by the first item that *fails*, the
                // other two by the first that passes. Whichever edge decides
                // it writes the answer and leaves; the other goes on.
                let (decided, next_item) = match terminal {
                    Terminal::HasAll(_) => (miss, hit),
                    _ => (hit, miss),
                };
                match terminal {
                    Terminal::HasAny(_) => self.chain_assign_bool(&dest, true, decided, span),
                    Terminal::HasAll(_) => self.chain_assign_bool(&dest, false, decided, span),
                    _ => self.chain_found(&dest, &value, decided, span),
                }
                self.chain_leave(decided, depth, done, span);
                self.chain_leave(next_item, depth, step_block, span);
            }
        }
        // Every path out of the body has run the item scope's exits.
        self.scopes.pop();

        let stepped = self.temp(int_ty, span, step_block);
        self.assign(
            step_block,
            Place::local(stepped),
            Rvalue::Binary {
                op: BinaryOp::Add,
                lhs: Operand::Copy(Place::local(cursor)),
                rhs: Self::bits(1),
            },
            span,
        );
        self.assign(
            step_block,
            Place::local(cursor),
            Rvalue::Use(Operand::Copy(Place::local(stepped))),
            span,
        );
        self.terminate(step_block, TerminatorKind::Goto { target: head }, span);

        // What running out means, per terminal.
        match terminal {
            Terminal::HasAny(_) => self.chain_assign_bool(&dest, false, exhausted, span),
            Terminal::HasAll(_) => self.chain_assign_bool(&dest, true, exhausted, span),
            Terminal::First | Terminal::Find(_) => {
                let null = Operand::Const(Constant::Literal(Literal::Null));
                self.assign(exhausted, dest.clone(), Rvalue::Use(null), span);
            }
            Terminal::Collect
            | Terminal::Count
            | Terminal::Sum
            | Terminal::Product
            | Terminal::Reduce { .. }
            | Terminal::Last
            | Terminal::Minimum(_)
            | Terminal::Maximum(_)
            | Terminal::Each { .. } => {}
        }
        self.terminate(exhausted, TerminatorKind::Goto { target: done }, span);
        // A `for` is a statement of type `()`, and both ways out of it — the
        // chain running out and a `break` — arrive here.
        if matches!(terminal, Terminal::Each { .. }) {
            self.assign(done, dest, Rvalue::Use(Operand::Const(Constant::Unit)), span);
        }
        done
    }

    /// Call the barrier's key closure on the item in hand and push the `Int`
    /// it returns onto `keys` — the parallel array
    /// `science_array_sort_by_int_key` reorders beside the values.
    fn chain_push_key(
        &mut self,
        keys: &Place,
        (closure, closure_ty): (Local, Ty),
        value: &Place,
        int_ty: Ty,
        mut current: BlockId,
        span: Span,
    ) -> BlockId {
        let (argument, next) = self.chain_argument(value, closure_ty, false, current, span);
        current = next;
        let key_value = self.temp(int_ty, span, current);
        current = self.emit_call(
            Place::local(key_value),
            Callee::Indirect(Operand::Copy(Place::local(closure))),
            vec![argument],
            current,
            span,
        );
        let (key_accumulator, next) = self.accumulator_ref(keys, current, span);
        current = next;
        let discard = Place::local(self.temp(Ty::UNIT, span, current));
        self.emit_call(
            discard,
            Callee::Runtime(ARRAY_PUSH),
            vec![key_accumulator, Operand::Move(Place::local(key_value))],
            current,
            span,
        )
    }

    /// `xs.sort(by: key)`: the chain's `sorted(by:)` barrier, applied to an
    /// array in place.
    ///
    /// **Built out of the chain's own pieces**, because it is the same work
    /// with nothing after it. One [`Builder::chain_pass`] over the array with
    /// no links and a counting terminal computes the keys — once per element,
    /// in a pass that borrows the array exactly as a chain's source is
    /// borrowed — into a parallel `Array[Int]`; then the array and the keys
    /// are borrowed exclusively and handed to
    /// `science_array_sort_by_int_key`, which reorders both. The key closure is
    /// `(borrowed T) -> Int`, the type `sorted(by:)` gives it on an
    /// `ArrayIterate`, so `chain_argument` hands it the element borrow the
    /// pass already took. The count is thrown away and the keys are a
    /// temporary, dropped with the statement.
    fn lower_array_sort_by(
        &mut self,
        dest: Place,
        receiver: ExprId,
        key: ExprId,
        block: BlockId,
        span: Span,
    ) -> BlockId {
        let Some(int_ty) = self.context.decls.prelude().ty(self.context.types, "Int") else {
            return block;
        };
        let keys_ty = self.array_of(int_ty);
        let keys = Place::local(self.temp(keys_ty, span, block));
        let counted = Place::local(self.temp(int_ty, span, block));
        let (source, block) = self.chain_source(receiver, block, span);
        let block = self.chain_pass(
            counted,
            Terminal::Count,
            Some((keys.clone(), key)),
            Source::Array,
            &source,
            &[],
            block,
            span,
        );
        let (values_ref, block) = self.exclusive_ref(&source, block, span);
        let (keys_ref, block) = self.exclusive_ref(&keys, block, span);
        self.emit_call(
            dest,
            Callee::Runtime(ARRAY_SORT_BY_INT_KEY),
            vec![values_ref, keys_ref],
            block,
            span,
        )
    }

    /// Whether this definition is the prelude's `Array.sort(by:)` — the
    /// labelled one of the two `sort`s, which is the one with a parameter.
    fn is_array_sort_by(&self, def: DefId) -> bool {
        let entry = self.context.defs.get(def);
        entry.is_builtin()
            && entry.name == "sort"
            && entry.parent.is_some_and(|owner| self.context.defs.get(owner).name == "Array")
            && self.context.decls.signature(def).is_some_and(|sig| sig.params.len() == 1)
    }

    /// `Entry[K, V]` for a map place, from the map's own arguments.
    fn map_entry_ty(&mut self, map: &Place) -> Ty {
        let written = self.place_ty(map);
        let ty = self.revealed(written);
        let args = match self.context.types.kind(ty) {
            TyKind::Named { args, .. } => args.clone(),
            _ => return Ty::ERROR,
        };
        let Some(entry) = self.context.decls.prelude().get("Entry") else { return Ty::ERROR };
        self.context.types.named(entry, args)
    }

    /// `dest = value?` over whatever `dest` held, which is released first
    /// when it owns something: `last()` and the extremes overwrite their
    /// answer, and a `String` they displaced would otherwise leak.
    ///
    /// Returns the block the new value was written in. The `Drop` is
    /// elaborated like any other, as an assignment's is.
    fn chain_replace(&mut self, dest: &Place, value: &Place, block: BlockId, span: Span) -> BlockId {
        let ty = self.place_ty(dest);
        let block = if crate::moves::needs_drop(
            self.context.decls,
            self.context.types,
            self.context.aliases,
            ty,
        ) {
            let next = self.new_block();
            self.terminate(
                block,
                TerminatorKind::Drop { place: dest.clone(), flag: None, target: next },
                span,
            );
            next
        } else {
            block
        };
        self.chain_found(dest, value, block, span);
        block
    }

    /// Leave the item's scope from `from` and go to `target`: the drops and
    /// `StorageDead`s of everything the body made, then the edge.
    fn chain_leave(&mut self, from: BlockId, depth: usize, target: BlockId, span: Span) {
        let block = self.exit_scopes(depth, from, span);
        self.terminate(block, TerminatorKind::Goto { target }, span);
    }

    /// `dest = dest + operand`, through a temporary, which is how `take`'s
    /// counter has always been stepped.
    fn chain_add(&mut self, dest: &Place, operand: Operand, block: BlockId, span: Span) {
        self.chain_arith(dest, BinaryOp::Add, operand, block, span);
    }

    /// `dest = dest op operand`, through a temporary.
    fn chain_arith(
        &mut self,
        dest: &Place,
        op: BinaryOp,
        operand: Operand,
        block: BlockId,
        span: Span,
    ) {
        let ty = self.place_ty(dest);
        let stepped = self.temp(ty, span, block);
        self.assign(
            block,
            Place::local(stepped),
            Rvalue::Binary { op, lhs: Operand::Copy(dest.clone()), rhs: operand },
            span,
        );
        self.assign(block, dest.clone(), Rvalue::Use(Operand::Copy(Place::local(stepped))), span);
    }

    /// `dest = value?` — Decision 6's widening, the item moved in when it owns
    /// something, which is what makes its drop at the scope exit go away.
    fn chain_found(&mut self, dest: &Place, value: &Place, block: BlockId, span: Span) {
        let ty = self.place_ty(dest);
        let operand = self.chain_operand(value, true);
        self.assign(
            block,
            dest.clone(),
            Rvalue::Coerce { operand, coercion: Coercion::Widen, ty },
            span,
        );
    }

    fn chain_assign_bool(&mut self, dest: &Place, answer: bool, block: BlockId, span: Span) {
        let constant = Operand::Const(Constant::Literal(Literal::Bool(answer)));
        self.assign(block, dest.clone(), Rvalue::Use(constant), span);
    }

    /// The zero `sum()` starts from, spelled for the total's type: a float
    /// literal for a float, so that nothing downstream has to reinterpret an
    /// integer's bits as one.
    fn one_of(&self, ty: Ty) -> Operand {
        if self.context.decls.prelude().is_float(self.context.types, ty) {
            Operand::Const(Constant::Literal(Literal::Float { value: 1.0, suffix: None }))
        } else {
            Self::bits(1)
        }
    }

    /// The zero `sum()` starts from. See [`Builder::one_of`] for `product()`'s.
    fn zero_of(&self, ty: Ty) -> Operand {
        if self.context.decls.prelude().is_float(self.context.types, ty) {
            Operand::Const(Constant::Literal(Literal::Float { value: 0.0, suffix: None }))
        } else {
            Self::bits(0)
        }
    }
}
