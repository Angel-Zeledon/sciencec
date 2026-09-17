//! Assignability: `compatible`, plus the implicit conversions — four of them.
//!
//! [`Types::compatible`] is structural equality with an error type agreeing
//! with everything, and its own documentation says what it is not: *"it does
//! not widen `T` into `T?` (Decision 6), it does not box a concrete error type
//! into `any Error` (Decision 14), it does not look through an alias, and it
//! does not know what an interface is."* This module is the first three, plus
//! §7's fourth. What stays out is the alias, and §5 says why.
//!
//! **Both arguments must already be revealed** —
//! [`crate::alias::Aliases::reveal`] is the function that does it. Revealing
//! needs `&mut Types` because it interns what it builds; this relation is a
//! `&Types` question and stays one,
//! so that a caller can ask it with the table borrowed shared and so that the
//! relation itself has nothing in it that can allocate. The cost is one call
//! the caller must not forget, and the test that catches forgetting it is that
//! an alias and its expansion are one type.
//!
//! # 1. The relation, written out
//!
//! For a value of type `S` reaching a slot of type `T` at a site `site`:
//!
//! ```text
//!   compatible(S, T)                                    =>  Identity
//!   T = U?  and  compatible(S, U)                       =>  Widen
//!   T = U?  and  S = V?  and  copies(V, U)              =>  CopyWhenPresent
//!   T = U?  and  copies(S, U)                           =>  CopyThenWiden
//!   T = (any Error)?  and  may_box(S)  and  boxes(site) =>  BoxThenWiden
//!   T = borrowed[m] any I  and  S = borrowed[m] C
//!                and  unsizable(C)  and  C implements I =>  Unsize
//!   T = Box of any I  and  S = Box of C
//!                and  unsizable(C)  and  C implements I =>  UnsizeInBox
//!   T = any Error  and  may_box(S)  and  boxes(site)    =>  Box
//!   copies(S, T)                                        =>  Copy
//!   otherwise                                           =>  not assignable
//! ```
//!
//! read in that order, where `boxes(site)` is true at a `return` and at an
//! argument and nowhere else, `may_box(S)` is §3 — a structural predicate and
//! the implementation index, together — `unsizable(C)` is §4's structural half
//! and *`C` implements `I`* is its other one, `copies(S, T)` is §7 — *`S` is
//! `borrowed T` and `T` implements `Copy`* — and `borrowed[m]` is a borrow
//! whose mutability is `m` — the same `m` on both sides, because changing that
//! is not this table's question.
//!
//! **Three of those nine rules ask [`crate::methods`] a question** — counted by
//! decision rather than by row, because §4's two rows ask one question of two
//! indirections — and that is new: §3 and §4 both used to be *proposals with an
//! obligation attached*, admitted on shape alone and left for a caller to
//! check. Decision 11's index is the caller that can, so the relation asks it
//! directly and the obligation is gone rather than moved. §7's rule was built
//! after the index and has never been anything else.
//!
//! **The first rule subsumes every case where nothing has to happen**,
//! including `S = T?` into `T?` and anything involving [`Ty::ERROR`] — which is
//! what keeps one bad annotation to one diagnostic here as everywhere else.
//!
//! # 2. The coercions are top-level, and that is a decision
//!
//! **Decision. No conversion here recurses into a type, with the two
//! exceptions this section names and for the one reason it gives.** `Array of
//! T` is not assignable to `Array of T?`, `(A) -> B` is not assignable to `(A)
//! -> B?`, a tuple of a concrete error is not assignable to a tuple of `any
//! Error`, and an `Array of (borrowed Doc)` is not assignable to an `Array of
//! (borrowed any Summarize)`.
//!
//! The reason is that every one of them *changes the representation of what is
//! in the slot*: Decision 6's `T?` is a niche or a discriminant byte beside the
//! `T`, Decision 13's `any Error` is two words where the concrete type was
//! however many it was, and §4's unsizing makes a one-word pointer into a
//! pointer and a vtable. Applying any of them under a type constructor means
//! rewriting every element of a container that already exists, at a cost
//! proportional to its length, at an assignment that looks free. Rust refuses
//! the same thing for the same reason, and the refusal is what keeps a coercion
//! a fact about one value rather than a loop.
//!
//! **There are exactly two exceptions, they are exempt for the same two
//! reasons, and the paragraph above is written as a conjunction so that the
//! test is one test.** A conversion under a constructor is refused when the
//! rewrite is proportional to a length **and** the value changes. Neither
//! exception is either.
//!
//! - **[`Coercion::CopyWhenPresent`], which §7 argues for.** `(borrowed T)?`
//!   reaches `T?` when `T` is `Copy`. `T?` holds one optional element rather
//!   than a length, so the rewrite is `O(1)` and not proportional to anything;
//!   and a copy of a `Copy` type is the same value, so there is no *change* of
//!   value for `syntax-revision-2.md` §3.4 to forbid — only a duplication of
//!   one.
//! - **[`Coercion::UnsizeInBox`], which §4 argues for.** `Box of C` reaches
//!   `Box of any I` when `C` implements `I`. `Box of T` holds one element
//!   rather than a length, so again the rewrite is `O(1)`; and the element is
//!   not rewritten at all — the pointer that was in the box is the pointer that
//!   is in the result, and what is added beside it is a vtable constant. There
//!   is no allocation, nothing moves, and §3.4 has nothing to bite on.
//!
//! **This section used to grant the exemption to the first and withhold it from
//! the second, and that was the disagreement the corpus found.** The reason
//! stated here is a property of the *constructor* — one element, not rewritten
//! — and `Box` has it exactly as `?` does. Saying so about one of them and not
//! the other was an omission, not a distinction. Every other constructor keeps
//! the rule, and each exception is one variant so that a later reader can see
//! its whole extent by grepping for the name.
//!
//! **What it costs is the motivating example of Decision 14 itself.** That
//! decision was written against `return (doc, err)` with a concrete `err`
//! against an `Error?` signature — and the *tuple* `(Doc, MyError)` is not
//! assignable to `(Doc, Error?)` by the rules above. It works because the thing
//! in return position is a tuple **expression**, whose elements the checker
//! visits one at a time, each at [`Site::Return`]. That is the next phase's
//! obligation and §6 states it, because a checker that compares the tuple's
//! synthesised type against the signature and stops has implemented Decision 14
//! in a way that rejects the line that motivated it.
//!
//! # 3. The obligation, and who discharges it now
//!
//! **Decision 14 boxes *"a concrete error type"*, and the question that makes
//! it one is *does `S` implement `Error`*.** This relation used to be unable to
//! ask: the lookup that answers it is Decision 11's, it was not built, and
//! [`Coercion::Box`] was therefore *a proposal with an obligation attached* —
//! the caller owed the answer, no caller could give it, and this crate agreed
//! that an `Int` may be boxed into an `any Error`.
//!
//! **[`crate::methods`] is that lookup, and this rule now asks it.** The
//! relation takes the index and the check is in two halves, which stay two
//! because they refuse different things:
//!
//! - **`boxable`** is the *structural* half at the foot of this file — a
//!   nullable, a borrow and another interface object are refused whatever they
//!   implement, and each for a reason of its own.
//! - **[`Methods::implements`]** is the *declaration* half: the crate contains
//!   `S implements Error:`, or it does not.
//!
//! **What the second half cannot see is what it says no to**, and that is the
//! new cost in place of the old one. It looks for a written implementation and
//! nothing else: no blanket implementation, no supertrait, and no bound on a
//! type parameter. A type whose head this crate cannot find at all — a
//! parameter, `Self`, an erroneous type — is *admitted*, because refusing on an
//! unanswerable question is how a checker acquires a false positive, and that
//! is `check`'s §5 read across at the one place where it is a coercion rather
//! than a literal.
//!
//! # 4. Unsizing behind an indirection
//!
//! **Decision. `borrowed C` is assignable to `borrowed any I`, `mutable
//! borrowed C` to `mutable borrowed any I`, and `Box of C` to `Box of any I`,
//! for every interface `I` the crate declares `C` implements. The *bare* owning
//! forms — `C` into `any I`, and `C` into `Box of any I` — stay explicit.**
//! The first two are [`Coercion::Unsize`] and the third is
//! [`Coercion::UnsizeInBox`]; §4a below is the argument for the third and the
//! reason it is a variant rather than a widening of the first. All of them are
//! variants of their own rather than second spellings of [`Coercion::Box`]:
//! they are different operations, and a lowering that cannot tell them apart
//! emits an allocation for the free one.
//!
//! **Read the decision's second sentence precisely, because one word in it is
//! the whole of what did not change.** `C` into `Box of any I` is still
//! refused: an author writes `Box.new(doc)` and always did. What §4a admits is
//! what `Box.new(doc)` *produces* — a `Box of Doc` — reaching a `Box of any
//! Summarize`. The allocation is still written down at the place it happens,
//! which is the property §5's first bullet exists to protect.
//!
//! **The reason is that the refusal this replaces conflated two operations, and
//! only one of them is a coercion in the sense §6.2 is counting.**
//! `type-checking-and-mir.md` §6.2 calls Decision 14 *"the one implicit coercion
//! in the language besides `T` into `T?`"*, and what the ones it counts have in
//! common is that they change the value:
//!
//! - **`C` into `Box of any I` allocates.** The value moves to the heap, and
//!   `syntax-revision-2.md` §3.4's rule — no implicit change of *value* —
//!   bites. That is Decision 14's [`Coercion::Box`], which is why the variant
//!   is called Box.
//! - **`borrowed C` into `borrowed any I` allocates nothing.** It pairs a
//!   pointer that already exists with a vtable known at compile time. The value
//!   does not move and does not change; only the way it is being pointed at
//!   does. There is no change of value for §3.4 to forbid, so admitting it does
//!   not add to §6.2's count.
//!
//! **The corpus is the evidence for the distinction, in its own words.**
//! `examples/08_dyn_dispatch.science` comments that *"`any Summarize` is not a
//! type a value can have on its own; it is only ever reached through an
//! indirection"*, and then writes all three indirections — `borrowed any
//! Summarize`, `mutable borrowed any Reset` and `Box of any Summarize`. Every
//! line in that file the old refusal rejected went through a borrow; every
//! owning one is already spelled out, `Box.new(Doc(..))` and never `Doc(..)`.
//! `describe_any(doc)` works by composing §6.3's auto-borrow with this rule —
//! `BodyChecker::auto_borrow` is where the two meet — and `Box of any
//! Summarize` still has to be written.
//!
//! **It is not gated on the site, and [`Coercion::Box`] is.** `boxes(site)`
//! exists because Decision 14 names the two positions its conversion happens
//! at. Nothing names a position for a conversion that emits no code, and the
//! corpus needs one outside both: `Renderer(target: borrowed doc)` is a record
//! field initialiser, which is [`Site::Elsewhere`].
//!
//! **The obligation, discharged the way §3's is.** Whether `C` implements `I`
//! is the same question one interface wider — Box's target is the one known
//! `Error` and this one's is every interface a program declares — and it is
//! answered by the same index. So this rule checks the **shape** on both sides
//! — a borrow on the left, a borrow of an object on the right, the same
//! mutability, and a referent that is not structurally incapable of
//! implementing anything, the `unsizable` predicate at the foot of this file —
//! and then asks [`Methods::implements`] whether the crate declares `C
//! implements I:`. A `borrowed Int` no longer reaches a `borrowed any
//! Summarize`, which is what this paragraph promised would change.
//!
//! **The wider obligation is the one that pays for the index.** Box's could
//! have been faked with a list of error-shaped types; this one could not,
//! because the interface is whatever the program wrote.
//!
//! **Two things it deliberately does not reach**, each a separate decision, and
//! **neither of which the corpus writes** — a clause that is checked rather than
//! asserted, because `tests/corpus.rs` pins what the corpus says exactly.
//!
//! - **A mutability change alongside the unsizing** (§5's last bullet):
//!   `mutable borrowed Doc` does not reach `borrowed any Summarize` in one
//!   step, because weakening a borrow is `region-inference.md`'s question and
//!   nothing in this table knows what a region is.
//! - **`borrowed C` into `(borrowed any I)?`**, which would be an
//!   `UnsizeThenWiden` and is refused for the reason §5 refuses the other
//!   two-step forms — the note did not decide it, and refusing is the direction
//!   that can be reversed.
//!
//! **There were three, and the third was *an unsizing under a type constructor*
//! (§2).** It is now §4a's rule for the one constructor whose exemption §2
//! states, and refused for every other. The clause that stood here closed the
//! three with *"and none of which the corpus writes"*, and of the third that
//! was false: the corpus writes it six times, and it was invisible only because
//! `Box` had no declaration, so `Box.new(doc)` came back `Ty::ERROR` and agreed
//! with everything. What made the six appear is what made every `Box.new` in
//! the corpus checkable at all.
//!
//! # 4a. Unsizing under `Box`
//!
//! **Decision. `Box of C` is assignable to `Box of any I` when the crate
//! declares `C implements I:`.** This is [`Coercion::UnsizeInBox`].
//!
//! **The reason is that it is the same operation as [`Coercion::Unsize`], one
//! indirection over.** `science-rt`'s `boxed` module states the representation
//! and leaves nothing to infer: a `Box of C` **is** a `*mut C`, one word, and
//! *"`Box[dyn Trait]` (§4.3) is the one case that is wider: a pointer to the
//! value and a pointer to the vtable, in that order"*. So the conversion is a
//! pointer that already exists, paired with a vtable the compiler knows at this
//! site — no call to an allocator, nothing copied, and the pointee never
//! touched. That is [`Coercion::Unsize`]'s sentence with `borrowed` struck out,
//! and it is why §2's refusal does not reach it: §2 refuses a rewrite
//! proportional to a container's length, and `Box` holds one element which this
//! does not rewrite.
//!
//! **What this file got wrong, in its own words.** §5's first bullet says an
//! owned `any Summarize` *"is constructed where it is written — `Box.new(doc)`
//! — and the corpus already writes every one of them that way"*, and §4's
//! third refusal rejected exactly that. The file told an author to write the
//! form the file refused. Nothing in `examples/` was wrong and no signature was
//! wrong; two sentences here were, and a compiler that could type `Box.new` was
//! the first thing able to hold both at once.
//!
//! **It does not change `type-checking-and-mir.md` §6.2's count**, by §6.2's
//! own AMENDMENT 4 test: what that count counts is conversions that change a
//! *value*. [`Coercion::Box`] moves a value to the heap and is counted.
//! [`Coercion::Unsize`] changes only how a value is pointed at and is not.
//! This one changes only how a value is pointed at, does not move it, and does
//! not allocate — the allocation happened at the `Box.new` the author wrote —
//! so it is on [`Coercion::Unsize`]'s side of that line and adds nothing.
//!
//! **It is not gated on the site**, for [`Coercion::Unsize`]'s reason: nothing
//! names a position for a conversion that emits no code, and the corpus needs
//! it at both a `return` (`into_summary`'s two arms) and an argument
//! (`describe_boxed(Box.new(..))`).
//!
//! **The obligation is §4's, unchanged**: the shape on both sides — a `Box` on
//! the left, a `Box` of an object on the right, and a referent `unsizable`
//! admits — and then [`Methods::implements`] on whether the crate declares `C
//! implements I:`. A `Box of Int` does not reach a `Box of any Summarize`.
//!
//! **What it costs, stated plainly:**
//!
//! - **`Box` is now a name this module must know**, and it is the first
//!   non-interface on [`Coercions`]' list. That type's doc comment carries the
//!   argument; the short form is that §2's exemption is a property of one
//!   constructor, so the rule cannot be structural and the constructor has to
//!   be identified by name.
//! - **The free of the allocation becomes indirect.** A `Box of Doc` is
//!   released by `science_box_free` with `Doc`'s descriptor, known statically;
//!   a `Box of any Summarize` is released through the vtable, because codegen
//!   cannot see what is behind it. `science-codegen`'s `descriptor`'s
//!   `needs_drop` already says exactly this of `CgTy::Interface`, and
//!   `science-rt`'s `boxed` already says the vtable is codegen's to lay out.
//!   The cost is real and it is not new: it was incurred the moment `Box of any
//!   I` became a type a signature could name, which the corpus does in four
//!   places that have nothing to do with this rule.
//! - **The conversion consumes the box.** `Box of C` is not `Copy`, so the
//!   operand is moved, and an author who wanted to keep the concrete box no
//!   longer can at that expression. That is true of every use of a `Box` and is
//!   not this rule's doing, but it is the one respect in which this variant is
//!   unlike [`Coercion::Unsize`], whose operand is a shared borrow and survives.
//! - **`Array of (Box of C)` still does not reach `Array of (Box of any I)`.**
//!   The exemption is for `Box` and for `?`, one level, by §2's test; nesting it
//!   under a container puts the length back and the refusal with it.
//!
//! # 5. What is deliberately not a coercion
//!
//! - **`T` into `any I`, and `T` into `Box of any I` — the bare owning forms of
//!   §4.** Both change the value, which is what §6.2 is counting and what
//!   `syntax-revision-2.md` §3.4 refuses to do implicitly: the first is
//!   Decision 14's box, admitted for `Error` alone and only where
//!   `boxes(site)` holds; the second is an allocation, admitted nowhere. An
//!   owned `any Summarize` is constructed where it is written — `Box.new(doc)`
//!   — and the corpus already writes every one of them that way, **and §4a is
//!   what makes that sentence true rather than an instruction the file then
//!   refused**. The cost is that the two forms no longer look alike in the
//!   source: `describe_any(doc)` passes a borrow that §4 unsizes for free, and
//!   `describe_boxed(doc)` does not compile while `describe_boxed(Box.new(doc))`
//!   does. That is the intended reading, because the allocation is in the
//!   second spelling and the reader should be able to see where.
//! - **`T?` into `any Error`, and `E?` into `(any Error)?`.** Boxing a value
//!   that may be absent is a conversion that runs or does not depending on the
//!   value, and `syntax-revision-2.md` §3.4's rule — no implicit change of
//!   *value* — does not obviously cover it. The note did not decide it, so it
//!   is refused, which is the direction that can be reversed later: admitting it
//!   now and finding it wrong means unpicking programs that rely on it.
//! - **`mutable borrowed T` into `T`, for a `Copy` `T`.** §7 admits the shared
//!   borrow and not the exclusive one. Physically the read is the same read;
//!   what differs is what an exclusive borrow is *for*. It is held in order to
//!   write, it is the one borrow the language guarantees is unaliased, and a
//!   rule that silently turns one into a value makes `mutable borrowed T` usable
//!   wherever `T` is — which is a claim about exclusivity made by a table that,
//!   by the bullet below, knows nothing about exclusivity. The corpus needs the
//!   shared form and nothing needs this one, so it is refused in the direction
//!   that can be reversed.
//! - **`T?` into `T`.** Decision 6: *"`T?` never coerces to `T`"*. There is no
//!   rule above that produces it and there is a test that says so, because this
//!   is the direction that makes the null check optional and the whole of §4 of
//!   the note pointless.
//! - **Anything about regions, mutability or variance.** `mutable borrowed T`
//!   into `borrowed T` is a coercion in Rust and is `region-inference.md`'s
//!   question here, not this crate's: nothing in this table knows what a region
//!   is. §4's unsizing holds that line — it carries the mutability of the
//!   borrow through unchanged rather than weakening one, so `mutable borrowed
//!   Doc` does not reach `borrowed any Summarize` in a single step.
//!
//! # 7. A borrow of a `Copy` type reads as a value
//!
//! **Decision. `borrowed T` is assignable to `T` when `T` implements `Copy`.**
//! This is [`Coercion::Copy`], with [`Coercion::CopyThenWiden`] and
//! [`Coercion::CopyWhenPresent`] for the two shapes the corpus needs that rule
//! 1 does not reach on its own.
//!
//! **The reason is that the language has no other mechanism.**
//! `type-checking-and-mir.md` says in as many words that F0 has **no
//! dereference operator**, by design — every `Deref` in that IR is inferred and
//! none of them is written — and the prelude declares `Copy` *and* `Clone` as
//! interfaces with no methods on either, so there is no `.clone()` and no `*x`
//! to fall back on. Meanwhile `stdlib-core.md` §3.6 declares
//! `def get(self, index: Int) -> (borrowed T)?`, and
//! `collections-and-chains.md` §4.2's AMENDMENT 11 makes `for x in xs:`
//! desugar to `xs.iterate()`, which §4.1 says yields `borrowed Item`. So
//! `let x be data.get(i)` hands back a borrow and `for n in numbers:` binds
//! one, and `x * 2.0` — the most common line in the language this is for —
//! could not be written at all. Without this rule those two facts compose into
//! a language in which a scalar cannot be read out of a container.
//!
//! **It does not violate the rule `syntax-revision-2.md` §3.4 protects.** That
//! rule is *no implicit change of **value***, and the whole content of `Copy`
//! is that a copy of the value **is** the value: no allocation, no drop glue,
//! no observable second thing. That is what tells this coercion apart from
//! [`Coercion::Box`], which moves the value to the heap, and it is why this one
//! needs no `boxes(site)` gate — like [`Coercion::Unsize`] it is admitted
//! everywhere, and the corpus needs it at a `return`, at a block's tail and at
//! an operand of `+`.
//!
//! **The note that already assumed it.** `collections-and-chains.md` §4.3
//! writes two amendments that only make sense if this rule exists: AMENDMENT 6
//! makes a place expression *"yield the value when its type is `Copy`, and a
//! borrow otherwise"*, and AMENDMENT 7 says operator interfaces are implemented
//! *"for borrowed operands of the primitives, with an owned `Output`"*, so that
//! *"`borrowed F32 + borrowed F32` is `F32`"*. That second one is this rule
//! spelled as an implementation per primitive per operator; spelling it as one
//! coercion says the same thing once, and says it for every operator and every
//! `Copy` type at once, including the ones a user declares.
//!
//! **The three variants, and why the second and third exist.** Rule 1 reaches
//! `borrowed T` into `T`. It does not reach the two shapes `Array.get` and
//! `for` actually produce:
//!
//! - **`(borrowed T)?` into `T?`** is [`Coercion::CopyWhenPresent`], and it is
//!   §2's one exception. `Letters.next` in `examples/06_traits.science` returns
//!   `Self.Item?` — a `Char?` — from a `get` that gives `(borrowed Char)?`.
//!   The copy runs when the value is present and does not when it is absent,
//!   which is the *conditional* conversion §5's second bullet refuses for
//!   boxing. The refusal there is stated on §3.4's ground — boxing changes the
//!   value — and that ground is exactly what a copy of a `Copy` type does not
//!   stand on, so the two are refused and admitted for one reason rather than
//!   two.
//! - **`borrowed T` into `T?`** is [`Coercion::CopyThenWiden`], §7's rule then
//!   Decision 6's, in that order and never the reverse. `first_even` in
//!   `examples/10_loops.science` is `-> Int?` and returns the `item` a `for`
//!   bound, which is a `borrowed Int`. It is a variant for the reason
//!   [`Coercion::BoxThenWiden`] is one: the order is not symmetric, and a
//!   lowering handed two booleans has to learn it from somewhere else.
//!
//! **The obligation is discharged by [`Methods::declares`] and not by
//! [`Methods::implements`]**, and that is the one place this file's §3
//! discipline is deliberately inverted. §3 admits a type whose head it cannot
//! see — a parameter, `Self` — because *"refusing on an unanswerable question
//! is how a checker acquires a false positive"*. Here the yes **emits a copy**:
//! admitting one for a `T` this crate cannot classify would duplicate a value
//! in a language whose ownership model says a value has one owner, and the
//! failure would be a silent double drop rather than a message the author can
//! argue with. So this rule refuses what it cannot answer, `Methods::declares`
//! is the predicate that does, and its own documentation states the trade at
//! the other end.
//!
//! **What it costs, stated plainly:**
//!
//! - **A third implicit coercion**, where `type-checking-and-mir.md` §6.2 was
//!   counting *"the one implicit coercion in the language besides `T` into
//!   `T?`"*. §4 already argued that an unsizing behind a borrow does not add to
//!   that count because it changes no value; this one has the same defence and
//!   it is a weaker one, because a copy is at least a machine instruction where
//!   an unsizing is a pointer that was already there.
//! - **A silent copy at a site that reads like a borrow.** `let x be
//!   data.get(i)` followed by `x * 2.0` copies an `F64`, which is free, and the
//!   identical shape at a hypothetical `Copy` record of sixty-four bytes copies
//!   sixty-four bytes with nothing in the source to see. `Copy` is the thing
//!   that is supposed to make that acceptable, and the language's answer if it
//!   ever stops being acceptable is to stop declaring `Copy` for that type —
//!   not to change this rule.
//! - **`borrowed T` and `T` now behave as one type at an assignment when `T` is
//!   `Copy`**, and the direction is one-way: `T` does not become `borrowed T`
//!   here, because taking a borrow is §6.3's auto-borrow, is a question about a
//!   *place*, and lives in `check`.
//! - **A `Copy` bound on a type parameter does not license it**, per
//!   [`Methods::declares`]. `def total of T: Copy(xs: borrowed Array of T)` can
//!   read no element out, and that is a real program this refuses.
//!
//! # 6. What the next phase owes this one
//!
//! Every use of [`assignable`] must pass the site the value is at, and the two
//! that box are named: a `return`'s operand and a call's argument. A checker
//! that passes [`Site::Elsewhere`] everywhere compiles and silently removes
//! Decision 14 from the language; a checker that passes [`Site::Return`]
//! everywhere silently makes boxing universal. The enum has three cases so that
//! the choice is made at each site rather than defaulted.

use science_resolve::hir::{DefId, DefKind, DefTable};

use crate::methods::Methods;
use crate::ty::{Ty, TyKind, Types};

/// Where a value is being put.
///
/// Decision 14's coercion holds *"at a `return` and at an argument position,
/// and nowhere else"*. The third case is every other position — a `let`, an
/// assignment, a field initialiser, an element of a literal — which the
/// relation treats identically, so they are one variant rather than six that
/// would have to be kept in step with a syntax that is still moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    /// The operand of a `return`, or the trailing expression of a body.
    Return,
    /// An argument at a call.
    Argument,
    /// Everywhere else.
    Elsewhere,
}

impl Site {
    /// Whether Decision 14's boxing applies here.
    pub fn boxes(self) -> bool {
        matches!(self, Site::Return | Site::Argument)
    }
}

/// What has to happen for the value to fit the slot.
///
/// This is what THIR carries: Decision 3 makes it *"HIR with a type on every
/// node, method calls resolved to a specific implementation, and implicit
/// conversions made explicit"*, and these are the implicit conversions. The
/// two-step variant is a variant rather than a pair of booleans because the
/// order is not symmetric — the value is boxed and then made present, never the
/// reverse — and a lowering that reads a struct of flags has to know that from
/// somewhere else.
///
/// **[`Coercion::Box`], [`Coercion::Unsize`] and [`Coercion::UnsizeInBox`] are
/// three variants**, although all three produce an interface object. §4 is the
/// argument for the first split: one allocates and two do not, and a lowering
/// handed a single "make an object" variant has to decide which by re-inspecting
/// the types — which is exactly the inspection this enum exists to have already
/// done. The failure mode of getting it wrong is a heap allocation emitted for a
/// conversion that is a pointer and a constant.
///
/// **The second split — [`Coercion::Unsize`] from [`Coercion::UnsizeInBox`] —
/// is about ownership rather than about cost.** Both build the same fat
/// pointer from the same two words. What differs is what the result owns: the
/// first produces a borrow, which owns nothing and drops to nothing, and the
/// second produces a box, whose drop releases the allocation through the
/// vtable. A lowering handed one variant for both would have to re-derive that
/// from the target type, and the failure mode is a leak in one direction and a
/// double free in the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coercion {
    /// The types already agree. Nothing is emitted.
    Identity,
    /// Decision 6: `T` into `T?`.
    Widen,
    /// Decision 14: a concrete error type into `any Error`, subject to §3's
    /// obligation.
    ///
    /// **This one allocates.** The value moves to the heap.
    Box,
    /// Decision 14 then Decision 6: a concrete error type into `(any Error)?`,
    /// which is the type in the signature of every fallible function in the
    /// language.
    BoxThenWiden,
    /// §4: `borrowed C` into `borrowed any I`, at the same mutability, subject
    /// to §4's obligation.
    ///
    /// **This one does not allocate.** The pointer already exists; what is
    /// added beside it is a vtable the compiler knows at this site. Lowering
    /// emits a pair, not a call to an allocator.
    Unsize,
    /// §4a: `Box of C` into `Box of any I`, subject to §4's obligation.
    ///
    /// **This one does not allocate either.** The allocation was made by the
    /// `Box.new` the author wrote; this pairs the pointer that came out of it
    /// with the same vtable [`Coercion::Unsize`] would have used. It is §2's
    /// second exception, and the reason it is not [`Coercion::Unsize`] is that
    /// the result **owns** what it points at: its drop releases the allocation
    /// through the vtable, where a borrow's drops to nothing.
    ///
    /// **The operand is consumed.** A `Box` is not `Copy`, so the box the
    /// conversion reads is moved out of, and MIR lowers the operand as a move
    /// without being told to — `lower`'s `is_copy` answers no for a
    /// `TyKind::Named` that is not a primitive.
    UnsizeInBox,
    /// §7: `borrowed T` into `T`, where `T` implements `Copy`.
    ///
    /// **This one is a load.** The value behind the borrow is duplicated, and
    /// `Copy` is the promise that duplicating it is the same value rather than
    /// a second one — no allocation, no drop glue, nothing to run.
    Copy,
    /// §7 then Decision 6: `borrowed T` into `T?`, where `T` implements `Copy`.
    ///
    /// The order is the order of the name: the value is copied out and then
    /// made present, never the reverse.
    CopyThenWiden,
    /// §7 under Decision 6's constructor: `(borrowed T)?` into `T?`, where `T`
    /// implements `Copy`. §2's one exception.
    ///
    /// **The copy runs only when the value is present.** That is what makes
    /// this a conversion with a branch in it rather than a load, and it is why
    /// it is not [`Coercion::Copy`] with a note attached.
    CopyWhenPresent,
}

/// The three definitions this relation has to know by name.
///
/// `any Error` is a type mentioning the prelude's `Error` interface, `Copy` is
/// the interface §7's rule is conditioned on, `Box` is the one type constructor
/// §4a looks through, and the prelude does not export
/// its ids: `resolve_module` hands back a
/// [`hir::Crate`](science_resolve::hir::Crate) whose `DefTable` contains the
/// prelude's definitions but names none of them. So [`Coercions::of`] finds
/// them by the only property that identifies them — a builtin definition, of a
/// particular kind, of that name — once, at the top of a compilation.
///
/// **What it costs is a scan of the definition table**, which is the lookup the
/// HIR exists to abolish, performed once. The honest fix is for the resolver to
/// publish the handful of prelude ids that later phases need by name; that is
/// `science-resolve`'s decision and this crate should not make it by reaching
/// into `builtins`. The cost has not changed shape now that there are three
/// names rather than one — it is the same single pass.
///
/// **The third name was to be an argument rather than a habit, and this is the
/// argument.** `Box` is not an interface and it is not a question about
/// implementations; it is the one *type constructor* this relation looks
/// through, and §2 says that looking through a constructor is precisely what
/// this relation refuses. The exemption is therefore a property of one named
/// constructor and cannot be structural: a [`TyKind::Named`] carries a
/// [`DefId`] and nothing in a [`Ty`] says which [`DefId`] is the prelude's
/// `Box`. A rule that guessed — by arity, or by "one type argument" — would
/// admit `Vec of C` into `Vec of any I` for any one-parameter type a user
/// declared, which is §2's refusal with the name filed off.
///
/// **It also widens what `of` looks for**, and that is the second half of the
/// cost. `Error` and `Copy` are [`DefKind::Interface`]; `builtins.rs` allocates
/// `Box` as a [`DefKind::Primitive`], alongside `Array`, `Map`, `String` and
/// `Chars`. So the scan can no longer filter on one kind, and the kind is now
/// part of each name's own query rather than a precondition of the pass.
///
/// All three are [`Option`] because a table built by hand — every test in this
/// crate that does not go through the resolver — has no prelude at all. With no
/// `Error` in it Decision 14's rule cannot fire, with no `Copy` in it §7's
/// cannot, with no `Box` in it §4a's cannot, and the relation is `compatible`
/// plus Decision 6, which is exactly right for a table with no prelude in it.
#[derive(Debug, Clone, Copy, Default)]
pub struct Coercions {
    error: Option<DefId>,
    copy: Option<DefId>,
    boxed: Option<DefId>,
}

impl Coercions {
    /// No boxing and no copying out: a table with no prelude, or a caller that
    /// wants Decision 6 alone.
    pub fn new() -> Coercions {
        Coercions::default()
    }

    /// Finds the prelude's `Error` and `Copy` interfaces and its `Box`.
    pub fn of(defs: &DefTable) -> Coercions {
        let find = |kind: DefKind, name: &str| {
            defs.iter()
                .find(|def| def.kind == kind && def.name == name && def.is_builtin())
                .map(|def| def.id)
        };
        Coercions {
            error: find(DefKind::Interface, "Error"),
            copy: find(DefKind::Interface, "Copy"),
            boxed: find(DefKind::Primitive, "Box"),
        }
    }

    /// The interface `any Error` names, when there is one.
    pub fn error_interface(self) -> Option<DefId> {
        self.error
    }

    /// The interface §7's rule is conditioned on, when there is one.
    pub fn copy_interface(self) -> Option<DefId> {
        self.copy
    }

    /// The prelude's `Box`, when there is one: §4a's constructor.
    pub fn box_type(self) -> Option<DefId> {
        self.boxed
    }

    /// What a `Box of T` holds, when `ty` is one.
    ///
    /// Exactly one type argument: `Box` takes one, so a `Box of (A, B)` is a
    /// mistake the arity check reports and not a thing to look through. A
    /// `GenericArg::Const` or a `GenericArg::Error` in that position answers
    /// `None` for the same reason — it is not a type, so there is nothing for
    /// §4a to ask [`Methods::implements`] about.
    pub fn box_element(self, types: &Types, ty: Ty) -> Option<Ty> {
        let boxed = self.boxed?;
        let TyKind::Named { def, args } = types.kind(ty) else {
            return None;
        };
        if *def != boxed || args.len() != 1 {
            return None;
        }
        args[0].as_type()
    }

    /// Whether this type is exactly `any Error`.
    ///
    /// Exactly: an object at that interface with no arguments. `Error` takes
    /// none, so an `any Error of T` is a mistake the arity check reports and
    /// not a thing to box into.
    pub fn is_any_error(self, types: &Types, ty: Ty) -> bool {
        let Some(error) = self.error else {
            return false;
        };
        matches!(
            types.kind(ty),
            TyKind::Object { interface, args } if *interface == error && args.is_empty()
        )
    }
}

/// Whether a value of type `source` may be put in a slot of type `target`, and
/// what has to happen to it if so.
///
/// `None` is *not assignable*. The diagnostic for that is not here: a mismatch
/// is a fact about an expression, it wants the spans of both sides and the name
/// of the slot, and the phase holding those is the phase that reports. What
/// this hands back is the verdict and, when the verdict is yes, the conversion
/// THIR has to make explicit.
///
/// Both types must be revealed; see this module's opening paragraph.
///
/// **`methods` is how §3's and §4's obligations are discharged**, and it is a
/// parameter rather than a field of [`Coercions`] so that every call site shows
/// that the question is being asked. An empty index — every test in this crate
/// that builds no declarations — answers *"nothing implements anything"*, and
/// the two rules that depend on it do not fire; that is the honest answer for a
/// compilation with no implementations in it, and it is the reason the
/// predicate admits rather than refuses wherever it cannot see a head.
pub fn assignable(
    types: &Types,
    methods: &Methods,
    coercions: Coercions,
    site: Site,
    source: Ty,
    target: Ty,
) -> Option<Coercion> {
    // Rule 1. Structural equality, and an already-reported type agreeing with
    // whatever it meets.
    if types.compatible(source, target) {
        return Some(Coercion::Identity);
    }

    if let TyKind::Nullable(inner) = *types.kind(target) {
        // Rule 2. Decision 6, and only in this direction.
        if types.compatible(source, inner) {
            return Some(Coercion::Widen);
        }
        // Rule 3. §7 under the `?`, which is §2's one exception: `Array.get`
        // gives `(borrowed T)?` and a signature saying `T?` is the same
        // answer for a `Copy` `T`.
        if let TyKind::Nullable(source_inner) = *types.kind(source) {
            if copies(types, methods, coercions, source_inner, inner) {
                return Some(Coercion::CopyWhenPresent);
            }
        }
        // Rule 4. §7 then Decision 6: a `for` binding returned from a `-> T?`.
        if copies(types, methods, coercions, source, inner) {
            return Some(Coercion::CopyThenWiden);
        }
        // Rule 5. `-> (T, Error?)`, which is the signature Decision 14 was
        // written against.
        if site.boxes()
            && coercions.is_any_error(types, inner)
            && may_box(types, methods, coercions, source)
        {
            return Some(Coercion::BoxThenWiden);
        }
        return None;
    }

    // Rule 6. §4's unsizing. Not gated on the site, because it emits no code
    // and changes no value, and because the corpus needs it at a record field.
    if let TyKind::Borrowed { mutable, inner: object } = *types.kind(target) {
        // Nothing below this can apply to a borrowed target — rule 5's target
        // is `any Error`, unborrowed — so this arm answers for all of them.
        let TyKind::Object { interface, .. } = *types.kind(object) else {
            return None;
        };
        let TyKind::Borrowed { mutable: source_mutable, inner: referent } =
            *types.kind(source)
        else {
            return None;
        };
        // The same `m` on both sides: weakening an exclusive borrow is §5's
        // last bullet, and it belongs to whoever owns regions.
        if source_mutable != mutable || !unsizable(types, referent) {
            return None;
        }
        // §4's obligation, discharged: the interface is the one the target
        // names, and the index is asked whether the referent implements it.
        if !methods.implements(types, referent, interface) {
            return None;
        }
        return Some(Coercion::Unsize);
    }

    // Rule 7. §4a's unsizing, one indirection over. Not gated on the site for
    // rule 6's reason, and reached only through the prelude's `Box`: §2's
    // exemption is a property of that one constructor and of Decision 6's `?`,
    // and of nothing structural.
    if let Some(object) = coercions.box_element(types, target) {
        if let TyKind::Object { interface, .. } = *types.kind(object) {
            if let Some(referent) = coercions.box_element(types, source) {
                // §4's obligation, asked about the boxed type exactly as rule 6
                // asks it about the borrowed one.
                if unsizable(types, referent) && methods.implements(types, referent, interface) {
                    return Some(Coercion::UnsizeInBox);
                }
            }
        }
        // No `return None` here, deliberately: a `Box of any I` target that
        // this rule refuses is still a target rules 8 and 9 may answer about,
        // and both of them answer no on their own terms. Rule 6 returns early
        // because a *borrowed* target is a shape nothing below it can match;
        // a `Box of T` is a `TyKind::Named` and that is not true of it.
    }

    // Rule 8. Decision 14.
    if site.boxes()
        && coercions.is_any_error(types, target)
        && may_box(types, methods, coercions, source)
    {
        return Some(Coercion::Box);
    }

    // Rule 9. §7. Not gated on the site: a copy of a `Copy` type is the same
    // value, so there is no position at which it would be a surprise, and the
    // corpus needs it at a block's tail as well as at a `return`.
    if copies(types, methods, coercions, source, target) {
        return Some(Coercion::Copy);
    }

    None
}

/// Whether `source` is a shared borrow of `target` and `target` is `Copy`:
/// §7's rule, in the one place its three variants share.
///
/// Three things have to hold and each refuses something different:
///
/// - **`source` is a *shared* borrow.** §5's new bullet: the exclusive form is
///   refused, because a rule that reads a value out of one makes `mutable
///   borrowed T` usable wherever `T` is.
/// - **Its referent is the target.** Structural equality, which is rule 1's
///   relation — so this rule takes a borrow off and does nothing else. It does
///   not compose with any other conversion; `borrowed MyError` does not reach
///   `any Error` by copying out and then boxing, and that is §2 holding.
/// - **The referent is `Copy`**, which is `copyable` for the shape and
///   [`Methods::declares`] for the declaration, exactly as §3 and §4 are two
///   halves — and with §7's inversion at the second half.
fn copies(types: &Types, methods: &Methods, coercions: Coercions, source: Ty, target: Ty) -> bool {
    let Some(copy) = coercions.copy_interface() else {
        return false;
    };
    let TyKind::Borrowed { mutable: false, inner } = *types.kind(source) else {
        return false;
    };
    types.compatible(inner, target)
        && copyable(types, inner)
        && methods.declares(types, inner, copy)
}

/// Whether `source` may be boxed into `any Error`: §3's two halves together.
fn may_box(types: &Types, methods: &Methods, coercions: Coercions, source: Ty) -> bool {
    let Some(error) = coercions.error_interface() else {
        return false;
    };
    boxable(types, source) && methods.implements(types, source, error)
}

/// Whether `source` is a shape that could be a concrete error type.
///
/// This is the *structural* half of §3's question, and it is all of it that can
/// be answered without the implementation lookup. Three shapes are refused
/// outright:
///
/// - **A nullable.** §4: boxing a value that may be absent is a conditional
///   conversion the note did not decide.
/// - **A borrow.** `any Error` owns its value; a borrow does not, and boxing one
///   would be a lifetime question in a crate with no notion of one.
/// - **Another interface object.** Decision 14 boxes a *concrete* type. An
///   `any Summarize` into an `any Error` is an upcast between objects, which
///   needs a vtable the compiler would have to build and a subinterface
///   relation nobody has specified.
///
/// Everything else — a named type, a type parameter with a bound, `Self` — is
/// admitted here and goes on to §3's second half, which is the implementation
/// index.
fn boxable(types: &Types, source: Ty) -> bool {
    !matches!(
        types.kind(source),
        TyKind::Nullable(_) | TyKind::Borrowed { .. } | TyKind::Object { .. }
    )
}

/// Whether `referent` — the type behind the source indirection, which is a
/// borrow for rule 6 and a `Box` for rule 7 — is a shape that could implement
/// an interface.
///
/// **One predicate for both rules, and that is deliberate**, where `boxable`
/// and `copyable` are separate from it. The three bullets below are reasons
/// about *what is being pointed at*, and they do not mention what is doing the
/// pointing: `borrowed (Doc?)` and `Box of (Doc?)` are refused by the same
/// sentence, so splitting them would be two copies of one argument rather than
/// two arguments. The other two predicates stayed separate because their
/// reasons genuinely differ; this one has nothing to differ about.
///
/// This is the *structural* half of §4's question, and it is all of it that can
/// be answered without Decision 11's lookup. It refuses the same three shapes
/// `boxable` refuses, and for reasons that survive the change of operation:
///
/// - **A nullable.** `borrowed (Doc?)` is a borrow of an optional, and the
///   question *does `Doc?` implement `Summarize`* is not the question the
///   author meant. §5's second bullet refuses the boxing form for the same
///   reason: the note decided nothing about implementations on `T?`.
/// - **Another borrow.** `borrowed (borrowed Doc)` would be asking whether a
///   reference implements the interface, which is a decision about auto-deref
///   that nothing in the language has taken. `Box of (borrowed Doc)` is the
///   same question and gets the same answer.
/// - **Another interface object.** `borrowed any Summarize` into
///   `borrowed any Reset` is an upcast between objects, and `Box of any
///   Summarize` into `Box of any Reset` is the same upcast owned. It is cheap —
///   it rewrites the vtable half of a pair — but *which* vtable needs the
///   subinterface relation nobody has specified, so it is refused here exactly
///   as `boxable` refuses it.
///
///   **This one also refuses the identity that is already rule 1's.** `Box of
///   any Summarize` into `Box of any Summarize` is `compatible` and never
///   reaches rule 7, so the refusal here costs nothing a program writes.
///
/// Everything else — a named type, a type parameter with a bound, `Self` — is
/// admitted here and goes on to [`Methods::implements`].
///
/// **The shapes are the same three as `boxable`'s and the predicates are still
/// two**, because the reasons are not the same three: `boxable` refuses a
/// borrow because `any Error` owns its value, and this refuses one because of
/// auto-deref. Folding them into one predicate would make a later change to
/// either reason silently change the other rule.
fn unsizable(types: &Types, referent: Ty) -> bool {
    !matches!(
        types.kind(referent),
        TyKind::Nullable(_) | TyKind::Borrowed { .. } | TyKind::Object { .. }
    )
}

/// Whether `referent` — the type behind the source borrow — is a shape that
/// could be `Copy`.
///
/// The *structural* half of §7's question, and the third predicate in this file
/// to refuse the same three shapes. It stays a third one for the reason the
/// second stayed a second: the shapes agree and the reasons do not, and folding
/// them together would make a later change to one reason silently change two
/// other rules.
///
/// - **A nullable.** `borrowed (T?)` into `T?` would be asking whether `T?` is
///   `Copy`, which nothing declares and which is a decision about whether
///   Decision 6's constructor preserves an interface — a rule about
///   *conditional* implementations that `methods`' §4 says this index does not
///   express. The shape a program actually writes, `(borrowed T)?` into `T?`,
///   is rule 3 and is answered about `T` rather than about `T?`.
/// - **Another borrow.** `borrowed (borrowed T)` into `borrowed T` is a copy of
///   a pointer, which is harmless, and admitting it is still a decision about
///   auto-deref that nothing in the language has taken — the same one
///   [`unsizable`] refuses for the same shape.
/// - **An interface object.** `borrowed any I` into `any I` would copy a value
///   whose size the compiler does not know, which is the one case here that is
///   not a question of taste: `any I` is unsized, and `Copy` reachable through
///   an object would also mean `head == interface` answering
///   [`Methods::declares`] yes for an `any Copy` nobody implemented.
///
/// Everything else — a named type, a type parameter, `Self` — is admitted here
/// and goes on to [`Methods::declares`], which refuses the last two.
fn copyable(types: &Types, referent: Ty) -> bool {
    !matches!(
        types.kind(referent),
        TyKind::Nullable(_) | TyKind::Borrowed { .. } | TyKind::Object { .. }
    )
}
