# Type checking and the mid-level IR

**Status.** Design. Nothing below is built. `crates/` ends at the resolver;
`crates/science-types` does not exist, and `sciencec` has no `build`.

**What this note is.** Everything between the resolver and code generation: the
type system, the intermediate representations, monomorphisation, interface
resolution, and the two analyses the error model shipped without — flow
narrowing and `SC0140`.

**What it is not.** Region inference and ownership checking are
`region-inference.md`'s. This note produces the IR that note consumes and
guarantees the properties it asked for; where the two meet, §12 says which is
which.

---

## 0. Two goals, and the type system answers to both

This note is not free to be designed on its own terms. Two commitments constrain
it and they pull in different directions.

**The project self-hosts.** The compiler is to be written in Science, so this
checker eventually runs on itself. That is a constraint on the *design*, not
only the schedule: a type system that needs constructs Science cannot express is
a type system that cannot check its own implementation.

**The ML ecosystem must be usable, or the language has no reason to exist.**
scikit-learn, PyTorch, the whole stack. And this constraint is sharper than it
first sounds, because **most of that ecosystem is not a C library**:

| Target | What is actually there |
|---|---|
| BLAS, LAPACK, FFTW, HDF5 | A real C ABI. `extern "C"` reaches it. |
| PyTorch | C++ with a partial C API. `libtorch` links; the *usable* PyTorch is Python. |
| **scikit-learn** | **No C API at all.** Python and Cython over NumPy. Estimators are Python classes. |
| NumPy, pandas, Arrow | Data crosses as a *format* — DLPack, the Arrow C Data Interface — not as a library. |

So "use the C code of scikit-learn" is not a thing that can be done, because
there is no such C code to link. A `Pipeline`, a `GridSearchCV`, a fitted
`RandomForestClassifier` are Python objects, and reaching them means embedding
CPython. `python-from-science.md` §2.2 measures what the naive approach gets:
**46% of elementary operations and approximately 0% of real programs.** §10
below is what this note owes that design.

The two goals agree more than they conflict, and where they conflict the
ecosystem wins, because self-hosting is a milestone and the ecosystem is the
reason.

---

## 1. What the spec already fixed

§7.1 gives the pipeline and it is not negotiable here:

```
HIR  --types + interfaces-->  THIR  --lowering-->  MIR  --mono--> codegen
```

§5.2 fixes the inference regime: **signatures are fully annotated, and inference
is local to a body.** That single sentence is worth more than any other decision
in this note and §2 is about why.

§7.1 also carries a warning for F1: tensor-typed code will want an array-level
IR between THIR and MIR, because lowering whole-array operations straight to
loops makes fusion an LLVM loop-pass problem and LLVM is bad at it. **F0 does
not build that IR and must not make it impossible.** §3.4 says what that costs
today.

---

## 2. The type system

**Decision 1. Type checking is bidirectional, and there is no unifier across
function boundaries.**

Because signatures are fully annotated, every call site knows its callee's types
before it looks at the arguments, and every function body has a known expected
return type. That gives checking mode where the type is known and synthesis mode
where it is not, which is the whole algorithm. There is no Hindley-Milner, no
generalisation, no let-polymorphism, no principal types, and no occurs check
outside the local inference variables of one body.

**What that buys, in the order it matters:**

1. **Error messages can name a declared type.** The failure mode of global
   inference is an error reported far from the mistake, blaming a type nobody
   wrote. Every `SC02xx` in this note can point at an annotation a human typed.
2. **Bodies check in parallel and in any order.** Which is what makes §11's
   query boundaries a function rather than a component.
3. **It is expressible in Science.** A bidirectional checker is a recursive walk
   with an environment; a unification-based one wants a mutable union-find with
   interior mutability, which `region-inference.md` §9 says has no spelling.
   This is goal one of §0 paying off immediately, and it is not a coincidence:
   §5.2 chose annotated signatures for readability, and readable-by-a-human and
   expressible-in-a-simple-language are usually the same property.

**Cost.** Local inference variables still exist — `let xs be (Array of Int).new()`
has to infer the element type from a later `push` — so there is a small
unification engine inside one body. It is bounded by the body and it is
discarded when the body is done.

**Decision 2. Numeric literals are inferred, not defaulted, within a body, and
default to `I64` and `F64` when unconstrained.** A scientific language that
silently makes `1` an `I32` will be wrong on somebody's index arithmetic.

> **AMENDMENT 2: this note never mentioned the auto-borrow, and the core spec
> requires it.** §6.3 of the core spec: *"if a parameter is declared
> `borrowed T`, the caller writes `compare(a, b)"*. So a call site holding a
> `T` against a `borrowed T` parameter is not an error — the borrow is inserted.
>
> The omission was not academic. The first run of the finished checker over
> `examples/` produced **147 diagnostics, and roughly 60% of them were a borrow
> the language had told the author not to write.** A rule the spec states and
> the design note forgets is worse than one neither has, because the
> implementation follows the note.
>
> **It is an elaboration, not a coercion, and the distinction decides where it
> lives.** The value is unchanged; what changes is that the checker writes down
> a `Borrow` node that the author did not. So it belongs in the body checker
> and not in the assignability relation, whose own §4 refuses to know anything
> about regions — correctly, since nothing in that table knows what a region is.
> An auto-borrow that is exclusive invalidates narrowing exactly as a written
> one does, per Decision 8.
>
> **A comparison is not an assignment, and this is the same mistake one layer
> down.** §5.4 makes `is` one operator dispatching to `Eq`, whose method takes
> `self` — the bare receiver *is* the shared borrow, which is the whole reason
> the mismatch arises. So `name is ""` has a `borrowed String` and a `String`
> in the source text and two `String`s in the call. Demanding that the written types
> agree reports on the borrow §6.3 exists to remove. Comparisons are checked
> through borrows for that reason.
>
> **Cost:** two places where the checker's tree no longer matches the token
> stream one-to-one, which is a cost MIR pays rather than the reader.

---

## 3. THIR and MIR

Two lowerings, and each must earn its stage.

### 3.1 THIR: typed, still expression-shaped

**Decision 3. THIR is HIR with a type on every node, method calls resolved to a
specific implementation, and implicit conversions made explicit.**

It earns its place by being where the *user's* program still exists. A
diagnostic about the user's code is best emitted from a tree that has the user's
structure; once MIR has flattened `a and b` into blocks and jumps, a message
about it has to reconstruct what was written. Exhaustiveness checking (§7),
`SC0140` (§5) and narrowing (§4) all run here, on a tree whose shape a message
can quote.

### 3.2 MIR: places, temporaries, a CFG

**Decision 4. MIR is a control-flow graph of basic blocks over explicit places
and temporaries, in three-address form.**

It earns its place twice over: `region-inference.md` cannot exist without it —
its lattice is literally defined over MIR points — and codegen wants a CFG
anyway because LLVM IR is one.

§12 lists what the region note requires and every item is a MIR property.

### 3.3 What is *not* a stage

**No SSA.** LLVM does that, and doing it twice buys nothing.

**No separate borrow-check IR.** Region inference runs on MIR directly.

### 3.4 The hole F1 will need, named now

§7.1 asks that an array-level IR remain possible. The thing that would foreclose
it is lowering an elementwise tensor operation directly to a MIR loop in F0,
because then F1's fusion pass has loops to reverse-engineer rather than array
operations to schedule — which is precisely the wound §7.1 says Julia carries.

**Decision 5. In F0, a whole-array operation lowers to a *call* to a runtime or
library entry point, never to an inlined MIR loop.** That keeps the operation
visible as one node right up to codegen, so an array IR can be inserted between
THIR and MIR later without touching anything below it. The cost is that F0's
array performance is whatever the callee does, with no fusion at all. That is
the right trade: F0 has no tensors, and the alternative is cheap now and
unaffordable later.

---

## 4. Nullable types and narrowing

This is the half of the error model that shipped without its meaning.
`syntax-revision-2.md` §3.1 requires that inside `if err?:` the compiler knows
`err` is not null, and §10 of that note says the model "lands worse than what it
replaces" without this. The syntax is in the compiler today; none of the
following is.

### 4.1 What `T?` is

**Decision 6. `T?` is a distinct type, not a union and not a subtype of `T`.**
`T` coerces into `T?` implicitly; `T?` never coerces to `T`. `T??` does not
exist — the parser builds two `Nullable` nodes and this phase collapses them
with `SC0520`, because a presence test on a presence test is never what anyone
meant.

**Representation.** A pointer-like `T` uses the null niche, so `(borrowed T)?`
is one word. A `T` with no niche gets a discriminant byte. `crates/science-rt`'s
`io.rs` currently carries a stale ABI from the removed `Result` and is marked in
place; this decision is what unblocks fixing it.

### 4.2 The narrowing rules

**Decision 7. Narrowing is a flow-sensitive fact about a *place*, computed over
THIR, and it is invalidated by any write to that place or to a prefix of it.**

A place is a local, or a field projection from a narrowed place. So:

- `if err?:` narrows `err` to non-null in the then-branch, and to null in the
  else-branch.
- `if not err?:` narrows the other way round. `not` composes; `and` narrows both
  operands in the then-branch; `or` narrows neither, because either may be the
  reason.
- An early `return` inside `if err?:` narrows `err` to **null** for the rest of
  the function, which is what makes the Go model bearable — after the check-and-
  return, the *value* half of the pair is known good.
- `config.port?` narrows `config.port`, not `config`. Writing to `config`
  invalidates it; writing to `config.other` does not.

> **AMENDMENT 1: the `or` rule above is the *then*-branch rule, and the
> else-branch narrows both operands.** Read literally, "`or` narrows neither"
> says nothing survives either way, and that is wrong in the direction that
> loses information. If `a? or b?` is false then neither was present, so the
> else-branch knows both are null.
>
> The reason it has to be said rather than left to the implementation: without
> it, `not (a? or b?)` narrows less than `not a? and not b?`, which is the same
> claim written differently. A narrowing that depends on which of two equivalent
> spellings the author happened to pick is worse than either answer, because the
> author cannot see why one worked.
>
> Implemented as the dual of the `and` rule, and tested as one. **Cost:** none
> beyond the symmetry — the branch was already being computed for `and`.

**Loops.** A narrowing established before a loop does not survive the back edge
unless it survives every path through the body. This is a standard forward
dataflow with a meet at the loop head, and getting it wrong in the permissive
direction is unsound, so the meet is intersection.

**Interaction with borrows, and this is the subtle one.** A narrowing of
`x` while `x` is borrowed exclusively elsewhere would be unsound, because the
borrow can write through and un-narrow it. Two ways to be safe: recompute after
every possible write, or rely on rule 4. **Decision 8: narrowing relies on rule
4 and records the dependency.** An exclusive borrow of `x` is the only way to
write `x`, rule 4 forbids it while any other borrow is live, and the narrowing
is invalidated at the point the exclusive borrow is *created*, not where it
writes.

**This makes the type checker depend on a fact the region checker proves, and
the two run in that order.** §12 makes it a stated contract rather than an
accident.

### 4.3 The hardest case found

```science
def pick(a: Doc?, b: Doc?) -> Doc?:
    if a?:
        return a
    b
```

`return a` in a function returning `Doc?` with `a` narrowed to non-null: the
narrowed type is `Doc` and the return type is `Doc?`, so this works by Decision
6's implicit widening. Now invert it:

```science
def first_title(a: Doc?, b: Doc?) -> String?:
    if a?:
        return a.title      # `a` is `Doc` here, so `.title` resolves
    if b?:
        return b.title
    null
```

Field access on a narrowed nullable is the operation the whole feature exists
for. Without it the model is cast-ridden and §3.1's claim that narrowing "is
what makes the model bearable" is false. **It is therefore the acceptance test
for this section**, and it is worth stating that Kotlin's smart casts and
TypeScript's narrowing both do exactly this and neither is controversial.

---

## 5. `SC0140`, the unchecked error

`syntax-revision-2.md` §3.3 is explicit: the new model loses `Result`'s
guarantee that a failure cannot be ignored, and this diagnostic is what recovers
most of it. It "should ship with the feature, not after it". It did not; this is
the specification.

**Decision 9. If a binding of nullable error type is never tested with `?` on
any path from its definition to a return, that is `SC0140`.**

It is a forward reachability question over THIR, not a full dataflow: mark every
binding whose declared or inferred type is `E?` where `E` implements `Error`,
mark every `?` test that reads it, and report the bindings with no reader. Linear
in the size of the body.

**What it must not report**, and each of these is a false positive that would
make the diagnostic hated:

- A binding that is **returned**. `return (value, err)` passes the obligation to
  the caller; that is the model working.
- A binding that is **passed to a function** taking `E?`. Same reason.
- A binding whose value is **statically `null`** — `let missing: Error? be null`
  is §3.1's own example and has nothing to check.
- A binding in a function that **cannot return** (`-> Never`) or that panics on
  every path.

**Decision 10. Deliberately ignoring an error is spelled by binding it to a name
beginning with an underscore.** The alternative — a `discard` keyword, or an
attribute — is a new surface for one diagnostic. `_err` is the convention every
language with this problem converges on, it needs no grammar, and it is
greppable, which matters because "show me every ignored error" is a question
reviewers actually ask.

**Cost.** `_` as a prefix acquires meaning it did not have. §13 records that the
resolver does not treat it specially today.

---

## 6. Interfaces, trait objects, and the fallible iterator

### 6.1 Resolution

**Decision 11. Method lookup is: inherent methods on the type, then interface
methods from interfaces the type implements that are in scope. Ambiguity is an
error, never a priority ordering.**

**Decision 12. Coherence is the orphan rule** — an implementation is legal only
in the crate defining the type or the crate defining the interface. Without it,
two crates implement `Display` for `Doc` differently and linking decides which
runs. This is a package-level rule and `package-manager.md` should know about it;
it does not.

**Decision 28. A `Box` is transparent to a method call, scoped to a receiver
the call only ever borrows.** `value.summarize()` resolves through a `Box[Doc]`
or a `&Box[Doc]` exactly as it would through a `Doc`, and does so only when the
method it finds takes `self` by shared borrow — never `mutable self`, never
`self: Self`.

*Reason.* Without it the construct is not merely inconvenient, it is
**unwritable**: `Box has:` declares exactly one member, the associated function
`Box.new`, and Science has no dereference operator (`AGENTS.md` §1). There is
no other spelling in the language that reaches `T`'s methods through a
`Box[T]`, and the corpus already needs five call sites across three files plus
`examples/08_dyn_dispatch.science`'s `describe_boxed` and
`examples/18_ownership.science`'s pair of the same name — an owned `Box[T]`
parameter in one, a `&Box[T]` one in the other.

**The borrow's transparency does not extend here, and this is a new rule
rather than a wider reading of the old one.** `methods.rs`'s own account of why
a borrow is transparent is that it *"has no nominal identity of its own to stop
at"*; a `Box[T]` is an ordinary `TyKind::Named` with its own `DefId`, ahead
of `T`'s, so the argument that licenses one does not license the other. Two
things make the narrower rule sound anyway. First, a method call's `self`
parameter is never kept past the call — the receiver is borrowed for the
duration of one call expression and nothing here lets that borrow outlive it,
so the question a wider transparency would raise (does a `Box` field, an
`implements` block, or a bound solve see through it too) never has to be asked
of *this* rule; every other reader of a receiver's head
(`methods::head`, `Methods::implements`, `Methods::declares`,
`science_codegen::mono`'s redirect of a default body's `self`) still stops at
`Box`, unchanged. Second, the scope is drawn at exactly the boundary the corpus
needs and no further: all five sites call a `self` method, never `mutable
self`, never by value, so restricting the decision to a shared-borrow receiver
costs nothing measured and leaves the harder question closed.

**The harder question is moving out of a `Box` through the same transparency,
and it is not decided here.** `crates/science-regions/src/deref_move.rs`
refuses every `Move` whose place contains a `Deref`, unconditionally,
*because* the alternative is a double free it cannot rule out any other way —
right for a borrow, and a genuinely open question for an owned `Box`, whose
payload this frame really does have the right to move. `mutable self` and
`self: Self` through a `Box` are refused for the same reason a move through a
borrow is refused: not because they are wrong, but because nothing has shown
them safe, and `SC0543` says so by name rather than reporting the ordinary
*"no such method"* on a method the index plainly contains.

*Implementation, and what MIR needed.* `science_types::methods::receiver_head`
peels a `Box` unconditionally when computing a method call's lookup key —
`Methods::receiver_for_call` is the entry point, and it is used only by
`check::BodyChecker::lookup`, never by `Methods::receiver`'s other five
callers. The scope is enforced one step later, once a candidate is in hand:
`methods::crosses_a_box` answers whether the receiver crossed a `Box` at all,
and `check::BodyChecker::guard_box_receiver` refuses the call — `SC0543` — when
it did and the candidate's `self_kind` is not `Shared`. The type level is not
the whole of it: a receiver reached through a `Box` has to become a real
address in MIR, and `science_mir::lower::Builder::box_deref` is a second
`Projection::Deref`, inserted after the ordinary `auto_deref` for a borrow, one
layer further in. It reuses the existing variant rather than adding one,
because a `Box[T]` and a `borrowed T` are the same machine value — a bare
pointer (`science_codegen::layout::CgTy::Ptr` gives `PtrKind::Box`,
`PtrKind::Borrow` and `PtrKind::MutBorrow` the identical one-word
representation), so `science-codegen-llvm`'s lowering of `Projection::Deref`,
which only ever asks whether the base is pointer-shaped and never why, needs no
change at all. Reusing the variant also means `deref_move.rs`'s blanket
refusal of a `Move` through any `Deref` reaches a `Box` for free, which is
exactly the conservative answer the paragraph above asks for and not an
incidental one.

**Rejected alternative: keep `Box` opaque and add an accessor.** A method —
`Box.get(self) -> borrowed T`, or a field the corpus does not have a spelling
for — would reach the payload without a new transparency rule. It was rejected
because it does not fit the language as declared: `Box has:` is the complete
member list `stdlib-core.md` gives, adding to it is the note's decision and not
a type-checker's, and every one of the five corpus call sites would still need
rewriting to call it — `value.summarize()` becoming `value.get().summarize()`
at every site, forever, for a value the language already treats as owning
exactly one `T`. The cost is not a one-time migration; it is a permanent
second spelling for "call a method" that exists nowhere else a `Box` is used
and that a reader has to learn is `Box`-specific. Transparency costs one new
rule, stated once, with a scope narrow enough to defend; an accessor costs a
standing tax on every call site the corpus already has.

**Cost.** Field access and interface-implementation questions through a `Box`
stay exactly as undecided as before — `boxed.title` and `Doc: Summarize`
questions about a `Box[Doc]` are untouched, because `methods::head` (unlike
`receiver_head`) does not peel a `Box`, and this decision does not ask it to. A
method that returns a borrow derived from `self` — `def peek(self) -> borrowed
String: self.title`, called through a `Box` — gets a region-checked borrow
whose outlives obligation is not tied to the `Box`'s own storage:
`science-regions`' per-type walk (`regions.rs`'s §1) assigns a region position
only at a `TyKind::Borrowed`, and a `Box` is `TyKind::Named`, so nothing here
gives it one. No corpus site returns a borrow through a `Box`, so nothing
exercises the gap today; closing it is either giving `Box` its own position in
that walk or narrowing this decision further, to a method whose return type
does not mention `Self`, and it is named here rather than risked silently.

### 6.2 `any Error` became common overnight

The error model made boxed trait objects appear in the return position of most
fallible functions, where before they were rare. `Error?` is `(any Error)?` and
`mcp-servers.md` §4 already leans on `Error` having exactly one method.

**Decision 13. `(any I)?` is a nullable fat pointer, and the null niche is in
the data pointer, so it is two words and not three.** The vtable pointer of a
null trait object is undefined and never read.

**Decision 14. A concrete error type coerces to `any Error` implicitly at a
`return` and at an argument position, and nowhere else.** This is the one
implicit coercion in the language besides `T` into `T?`, and it needs stating
because `syntax-revision-2.md` §3.4 removed `From` widening on the grounds that
conversion should be visible — and then wrote `return (doc, err)` with a
concrete `err` against an `Error?` signature, which is a coercion. The
distinction the note wanted is real but it is not "no implicit conversions": it
is **no implicit change of *value***. Boxing preserves the value; `From` did not.

> **AMENDMENT 4: there is now a third relation, and the count above survives
> only because of the sentence right before it.** `borrowed C` coerces to
> `borrowed any I` for any interface `I`, not just `Error`.
>
> §5 of `assign.rs` refused exactly this, on the grounds that it would be a
> second implicit coercion. The refusal put two operations under one name.
> `C` into `Box of any I` **allocates** — the value moves to the heap, and that
> is what "no implicit change of *value*" forbids, which is why Decision 14 is
> boxing and is counted. `borrowed C` into `borrowed any I` allocates nothing:
> it pairs a pointer that already exists with a vtable known at the site. The
> value does not move and does not change; only the way it is pointed at does.
>
> So the thing this decision counts is **conversions that change a value**, and
> by that measure there are still two. **State what is being counted**, because
> a reader counting `Coercion` variants in the implementation finds seven and
> concludes the note is stale.
>
> **AMENDMENT 4a: seven, not five — and the second addition tests the rule
> harder than the first.** `borrowed T` coerces to `T` when `T: Copy`, in three
> shapes: the plain copy, the copy widened into `T?`, and the copy through a
> nullable. The count above survives, on the same measure: a copy of a `Copy`
> type is the same value. But the defence is weaker than unsizing's and the
> implementation says so — unsizing emits no code at all, while this one emits
> a copy, and "the same value" is doing work that "no work happens" did before.
>
> It earns the weaker defence because the alternative was nothing. The language
> has **no dereference operator**, by design, and `Array.get` returns
> `(borrowed T)?` — so before this there was no spelling in the language that
> read a `Char` out of a container. A rule admitted to avoid an expressive dead
> end is a different kind of rule from one admitted for convenience, and the
> count is not the thing to protect if protecting it costs that. The owning forms — `C` into `any I` and `C` into
> `Box of any I` — stay written out, and that is the visible half of the trade:
> the two spellings no longer look alike in the source, which is the point,
> because one of them allocates.
>
> **Cost:** the new relation carries the same undischarged obligation Decision
> 14's does — *does `C` implement `I`* — and it is the wider of the two, because
> Decision 14's target is one known interface and this one's is every interface
> a program declares. Decision 11's lookup discharges both.

### 6.3 `Iterate` must model a `next()` that can fail

`python-from-science.md` §2 needs this and says so: a sizeable fraction of
scikit-learn cannot be used without iterating, Python iteration raises, and
`collections-and-chains.md`'s `Iterate` has no failing `next`. That note asks
this one to confirm a shape rather than invent one.

**Decision 15. `Iterate` is unchanged and gains a sibling, `TryIterate`, whose
`next` returns `(Self.Item?, Error?)`. `for x in xs:` accepts either; over a
`TryIterate` it requires the loop to be inside a construct that has a failure
edge, and outside one it is `SC0521` with a help line naming the `python:`
region and the explicit `next` call.**

Widening `Iterate` itself was rejected: every existing implementor would acquire
an error channel it can never use, and `for` over an `Array` would grow a check
that is always null. A second interface costs one name and leaves the common
case alone.

---

## 7. Exhaustiveness

**Decision 16. Usefulness and exhaustiveness by Maranget's algorithm** (*Warnings
for pattern matching*, JFP 2007), which is what rustc uses. A non-exhaustive
`match` is an error and reports a concrete witness — the shape of a value no arm
covers — rather than "not exhaustive".

Nullable types participate: `match doc:` over a `Doc?` has `null` as a
constructor, and matching `null` plus the non-null case is exhaustive.

**Decision 27. Reading a place that owns something out of a borrow types the
binding as a borrow, not as the declared owned type — Rust's match ergonomics,
and not only at a `match`.** `def take(d: &Doc) -> Int: let t be d.title; t.length()`
binds `t` at `borrowed String`, not at `String`, and the identical rule applies
to `Left(n): …` under a `match` over a `borrowed E of (I64, String)`: `n` binds
at `borrowed String` and would have bound at `I64` unchanged, because `I64` is
`Copy` and the rule only ever widens an owned type into a borrow, never the
reverse.

This closes a hole `crates/science-regions/src/deref_move.rs` opened on
purpose and named as temporary: that module refuses `SC0303`, *"cannot move a
value out of a borrow"*, on every one of a move's places that projects through
a `Deref`, and its own §3 says outright that it is *"a safety net and not the
answer"* — the answer is this decision, stated in a note rather than left in a
commit message. Before it, `crate::check`'s `field` and `scrutinee_substitution`
peeled a borrow to reach a field or a payload and handed back the *declared*
type of what was underneath, so `science-mir`'s lowering — correctly, given
what it was told — picked `Move` for anything not `Copy`, and the place it
moved still had the `Deref` the borrow inserted. The result was a value moved
out from under somebody else's reference with no diagnostic anywhere in front
of it, caught only by measuring: 20 sites across the corpus took this path,
and every one of them used the binding exactly as a borrow — compared with
`is`, passed to a parameter typed `borrowed String`, appended with
`push_str` — which is the evidence that the rule belongs at the type and not
at the use.

**The reason this is a type rule and not a wider move-analysis.** The
alternative considered was to leave `SC0303`'s refusal as the permanent
answer and require the author to write `&d.title` or `d.title.clone()` at
every one of those twenty sites. That is what literal Rust does for a bare
field expression — `let t = d.title;` where `d: &Doc` is `E0507` there too,
match ergonomics notwithstanding, because Rust's default binding modes are a
*pattern*-matching rule and a plain field access is not a pattern. Rejected,
because every site this refuses is a program that already reads as a borrow
and a language whose own core spec makes borrows automatic at a call site
(§6.3, AMENDMENT 2 above) has no consistent story for demanding one be
spelled out by hand at a field. The cost of the wider rule is that "moved"
stops being a safe guess about a binding's type from its declaration alone —
reading `d.title`'s field type off `Doc`'s declaration is no longer enough;
whether `d` was reached through a borrow now matters too — and that cost is
paid once, in `crate::check`, rather than at every call site in the corpus.

**`Copy` types are unaffected, and the predicate is `needs_drop`, not a new
one.** `let n be d.count` through a `&Doc` stays an owned `Int`: the rule
tests [`science_types::ownership::needs_drop`] on the field's *own* type, not
on the borrow, and a borrow releases nothing so a `Doc` of one `Int` field
still answers `false`. This is deliberately the identical predicate
`deref_move.rs` refuses by, stated once — `science-types` cannot depend on
`science-mir`, so the predicate moved to the crate both callers already stand
on rather than being written twice, which is `lib.rs` §1's argument against a
second implementation of one question applied one layer down.

**`&mut` binds by `&mut`.** A field or payload read through a `mutable
borrowed T` is a `mutable borrowed` binding, propagating the same mutability
the outer borrow already carried, which is what Rust's default binding modes
do and the only answer consistent with `mutable borrowed`'s own purpose
(§6.3's `retitle`, which writes through the referent precisely because the
parameter is exclusive). Nothing here introduces mixed-mode chains — a shared
borrow downgrading a later exclusive one, the way Rust's binding modes do when
a `&` and a `&mut` are nested in one scrutinee — because one field or pattern
read only ever peels the one borrow immediately around it; a chain of reads
each re-enters this rule with whatever the previous read produced, so a
`&mut` reached only after a `&` is never on offer to begin with.

**What this makes obsolete.** `crate::check`'s `scrutinee_substitution` used
to say, in as many words, that *"match ergonomics are a decision no note has
taken, and this change is a substitution rather than a new binding mode"* —
this decision is the one that comment was waiting on, and the comment is
retired rather than left stale. `deref_move.rs`'s §3 amendment note — *"until
that decision … is made in a note, this check is the difference between
refusing a program and silently corrupting its heap"* — is satisfied the same
way: `SC0303` is unchanged and still refuses a genuine move out of a borrow
(a `Deref`-rooted place whose type needs a drop *after* this rule has already
run), it simply has far less to refuse, because most of what used to reach it
as a `Move` now never leaves `crate::check` as anything but a `Ref`.

> **AMENDMENT 5: the type rule alone miscompiles, and the fix is one layer
> down too.** The first attempt at Decision 27 changed only what
> `crate::check` calls the type of a field or a payload, on the ground that
> `science-mir`'s lowering reads the type off the node and would follow along.
> It does — for `is_copy`, which now answered `true` for a shared, non-`Copy`
> field exactly as it always has for a shared borrow — and that is the bug.
> `Builder::operand`'s ordinary place read asked `is_copy` of the widened
> type and copied the *place* underneath unchanged: the same `String`'s three
> words a plain field read always was, now labelled a pointer. Handed to
> anything that wanted an actual pointer — a parameter, a comparison's other
> side — the linker's own verifier caught it: *"local is ptr and the value
> stored into it is an aggregate"*, on the exact reproducer this decision
> exists to fix, the first time it was tried end to end rather than only
> through `sciencec check`.
>
> **The fix.** `science_mir::lower::Builder::read_ergonomic` compares the
> node's type against the place's own type (`Builder::place_ty`, which is
> always the record's true declared field type, never the checker's
> widening); where they disagree — the type says borrow, the place is the
> owned storage a field or a payload always was — it builds a real address
> with `Builder::borrow_place` into a fresh temporary and reads that, instead
> of the place itself. Where they agree — an ordinary value, or a field the
> record itself declares `&T` — nothing changes. Both of Decision 27's
> positions reach this: `Builder::operand`'s catch-all (a field read, in a
> `let` or an operand) and `Builder::bind_pattern`'s `PatKind::Binding` arm (a
> match arm's binding).
>
> **The cost is named rather than hidden: it is not two-phase.** A written
> `&expr` at an argument gets `BorrowKind::TwoPhase` from `Builder::argument`'s
> own first arm; a Decision 27 reborrow reaching `read_ergonomic` has no such
> node behind it and is always `Shared` or `Exclusive`. `v.push(v.borrowed_field)`
> on a `&mut` receiver would want the two-phase reservation and does not get
> it here — no corpus site is this shape, and the day one is, the fix is
> threading `in_argument` through rather than a new decision.
>
> **A second, adjacent bug, found by the same reproducer with the `&` put
> back in.** `examples/07_generics.science`'s `return &found.inner` and
> `examples/18_ownership.science`'s `&doc.title` — both explicit, both
> pre-existing — turned into `&&String` once `found.inner`/`doc.title` were
> themselves already `&String`. The explicit `&` still has to build a real,
> separate `ExprKind::Borrow` node — `&mut c` where `c` is *itself* already a
> `&mut Config` parameter is a fresh reservation of `c`'s own slot, not a
> no-op, which is what a capture-and-reborrow test in
> `crates/science-regions/tests/closure_captures.rs` measured the wrong way
> to fix: collapsing the node away moved `c` instead of reborrowing it, and
> `SC0334` fired where `SC0330` should have. The right fix only narrows the
> *type* the fresh node is given — `referent` is the operand's own referent
> when the mutability already matches its outer `&`, so `&expr` widens once
> and not twice — and leaves the node, and the reservation, in place.
>
> **What is deliberately not covered: `Nullable`.** A presence test narrows
> *inside* a nullable's own representation, past the discriminant to the
> payload, and `read_ergonomic` — like `borrow_place` beneath it — can only
> take the address of a place that exists; there is no projection in this IR
> for a nullable's payload, the identical gap `Builder::borrow_hole`'s own
> comment names for a different caller. So `crate::check::BodyChecker::
> borrow_ergonomics` excludes a `Nullable` field or payload outright, and a
> `String?` field through a borrow reads exactly as it did before this
> decision. None of the twenty real sites are this shape.
>
> **One construct outside either named position turned out to need the same
> fix, and is worth naming because it was not designed in.** `Index.index`
> is declared `-> &Self.Output` (`indexing-and-array-literals.md` §1.1), so
> `xs[0]` was already typed as a borrow of an owned element for a reason
> that predates this decision entirely, and `let a be xs[0]` had the identical
> mismatch `read_ergonomic` now closes as a side effect of fixing it generally
> rather than only at a field or a pattern.
> `crates/science-codegen-llvm/tests/past_stage_three.rs`'s `refuse-index` is
> the record of what that mismatch used to do to the backend, and
> `crates/science-codegen-llvm/tests/arrays.rs`'s
> `an_indexed_element_bound_by_a_plain_let_builds_and_runs` is the same
> program, run.

---

## 8. The const-generic commitments this note owes

`const-expression-arithmetic.md` §10.1 lists eight things F0 must settle before
the checker is written. Items 1, 2, 5 and 6 are implemented and landed. The
other four are here.

**Item 3 — `NORMALISE` and `EQUAL`.** The normal form is `k + Σ cᵢ·aᵢ` over the
quotient-free fragment, with a compilation-stable atom order and provenance
spans on every term. **Decision 17: the atom order is by `DefId`, and `DefId`s
are assigned in source order by the resolver**, which makes the order stable
across runs without a hash and stable across incremental rebuilds as long as the
declaration order does not change.

> **AMENDMENT 3: Decision 17 is withdrawn. The atom order is by a rank derived
> from the declaring item's canonical path and its position in that item's
> `of`-list, which is what `const-expression-arithmetic.md` §3.1 always said.**
>
> The claim above — "stable across runs without a hash" — is false, and the
> sentence that makes it false is its own second clause. `DefId`s are assigned
> in source order *within one compilation*, and which source comes first is the
> order the files were named on the command line. So `sciencec check a.science
> b.science` and `sciencec check b.science a.science` number the same
> declarations differently, produce different atom orders, and therefore
> different normal forms.
>
> **Read that against item 4, three paragraphs down: the normal form is the
> monomorphisation key.** An unstable atom order is an unstable mono key, which
> is the exact failure item 4 describes — two symbols for one function — arrived
> at from the other direction, and it would not have been caught by anything
> item 4 proposes, because both spellings are the *same syntax*.
>
> The fix is `AtomOrder` in `science-types`: one pass over the `DefTable`
> sorting by (canonical path, index) and materialising integer ranks, built once
> per crate. `Atom::Param` carries the rank *before* the `DefId` so that the
> derived `Ord` sorts by rank, which is the whole mechanism.
>
> **Cost:** the order now depends on names rather than on numbers, so renaming a
> const parameter can reorder a normal form where renumbering a file no longer
> can. That is the right trade — a rename is something the author did, and a
> command-line order is not.
**Item 4 — the normal form is the monomorphisation key.** `Matrix of (T, a * b)`
and `Matrix of (T, b * a)` must be one instantiation. Keying on the *syntax*
emits two symbols for one function, which is a codegen bug found late and fixed
by rewriting the key. §9.1 is the consequence at the FFI boundary.

**Item 7 — one-variable linear matching.** From a call site's concrete argument
and a signature's `k + c·N`, recover `N`. Without it every call to a
const-generic function spells its const arguments explicitly and
`scientific-libraries.md` §6.7's signatures become unwritable at the call site.
It is division with a remainder check, and the remainder check is the error.

**Item 8 — `SC0260`/`SC0261` with the normal-form-and-legend block.** The
diagnostic prints both sides in normal form with a legend mapping each atom back
to where it was bound. `intrinsics-chem-bio.md` §5 depends on this: it can print
*"`k` is a second-order rate constant"* by inverting `1 - ORDER = -1`, and that
inversion is item 7's matching used for a message instead of a check.

---

## 9. What the ML boundary needs from this phase

§0 said the ecosystem wins where the goals conflict. Here is the bill.

### 9.1 Monomorphisation must not cross the C boundary

An `extern "C"` function is not generic and cannot be. **Decision 18: a generic
Science function may be *called* from an FFI wrapper, but a function declared in
an `extern` block is monomorphic, and a generic function may not be passed as a
C callback without an explicit instantiation.** The error is `SC0522` and it
names the instantiation to write.

### 9.2 DLPack and Arrow are types, not conversions

`data-io.md` and the tensor notes bind *formats* rather than libraries: DLPack
for tensors, the Arrow C Data Interface for dataframes. Both are `#[repr(C)]`
structs with a strict field order.

**Decision 19. A type marked `implements ffi.CLayout` has its field order,
offsets and padding fixed by the platform C ABI, and this phase verifies that
every field's type is itself C-representable.** The parser already rejects the
obvious cases in signatures (`crates/science-parser`'s `check_ffi_type`); this
phase does the transitive check the parser cannot, because it needs to look
through a type name to its definition.

### 9.3 The `python:` region, which is where scikit-learn actually lives

`python-from-science.md` Decision 2 makes subscripting and the arithmetic
operators available on `PyObject` **only inside a `python:` region**, and its
reason is a type-system reason: under the revision-2 error model an operator
cannot be fallible, because `a + b` is an expression that produces a value and
there is no syntax for it to produce `(PyObject, PyError?)`. Inside a region the
failure edge is the region's exit, so the operator can produce a plain value.

This phase owes that design three things:

**Decision 20.** Inside a `python:` region, `PyObject` gains implementations of
`Index`, `Add`, `Sub`, `Mul`, `Div` and the rest of the operator interfaces,
which lower to `PyObject_GetItem`, `PyNumber_Add` and so on. Outside a region
they do not exist, and the "no implementation of `Add` for `PyObject`" error
carries a help line naming the region and the always-available explicit form
(`a.item(k)`, `a.add(b)`).

**Decision 21.** A `python:` region has one failure edge and it is the region's
exit. Every operation inside it that can raise transfers there on a Python
exception. In type terms the region is an expression of type `(T, PyError?)`
whose body is checked as if every fallible operation were infallible — which is
the whole ergonomic point, and the reason `est.named_steps["pca"].explained_variance_ratio_`
is one line instead of nine.

**Decision 22.** `PyObject` is not `Copy`, is not `Send`, and its ownership is
ordinary Science ownership over a reference-counted CPython handle. The refcount
is CPython's business; the *handle* is a Science value with one owner, and drop
glue calls `Py_DECREF`. This is the one place in the language where a
reference-counted object exists, and it exists because CPython's object model is
not negotiable.

**Cost, stated plainly.** Decisions 20 and 21 give one type — `PyObject` —
operators that no other type gets and a control-flow construct that exists for
it alone. That is a special case in the type system for one foreign language,
and the justification is §0's: 46% of elementary operations and ~0% of real
programs without it, and scikit-learn is the ecosystem the project exists to
reach. If a cleaner construct is found, this is the thing to replace.

---

## 10. Incremental recompilation

§11.4 makes it part of done and §7.3 says the same database serves F1's
interactive tier. `crates/science-db` already wraps salsa.

**Decision 23. The memoised unit for type checking is the function body, keyed
on its HIR hash plus the signatures it references.** Bodies do not affect each
other — that is Decision 1's dividend — so editing a body invalidates that body
and nothing else. Editing a *signature* invalidates every body that calls it.

This is strictly finer than `region-inference.md`'s boundary, which is the
strongly connected component of the call graph, and the difference is not a
mistake: type checking is local because signatures are annotated, and region
inference is not because regions are not. **The two phases have different
invalidation granularity and the database must support both.** Neither note
would have noticed this alone.

---

## 11. Self-hosting: is this checker expressible in Science?

Taking §2's algorithm apart against `region-inference.md`'s rules:

- **The environment** is a stack of scopes mapping names to types — the `Rib`
  shape of `examples/21_compiler_shapes.science` §2. Owned, no borrows.
- **Types** are a flat table with `TypeId` indices, exactly the `DefTable`
  shape. A recursive type is a cycle *through indices*, which is not a cycle in
  the ownership graph.
- **Local inference variables** want union-find, which wants path compression,
  which wants interior mutability. **This is the one problem.** Path compression
  mutates through a shared borrow. The fix is the same as everywhere else in
  this project: a flat `Array of TypeId` with explicit `mutable self` on `find`,
  which costs nothing because the array is owned by the inference context.
- **THIR and MIR** are arenas of nodes addressed by index.
- **The query database** is `region-inference.md` §11's open question and is not
  made worse here.

**Decision 24. The checker is written index-not-pointer throughout, and union-
find takes `mutable self`.** Recorded now, because discovering it during the
port means rewriting the inference context.

---

## 12. The contract with `region-inference.md`

That note listed six requirements. This note guarantees them:

1. **An explicit CFG** with `(block, statement)` points. Decision 4.
2. **Explicit places**, with two expressions denoting the same place producing
   the same place.
3. **Explicit `StorageLive`/`StorageDead`**, so rule 5 has something to compare
   against.
4. **Two-phase borrows exposed**, so `v.push(v.len())` compiles. MIR marks the
   reservation point of an exclusive borrow separately from its activation.
5. **Drop points explicit before region inference runs**, *and elaborated*.

   Making the points explicit is not enough on its own, and saying only that
   was a hole `codegen-and-linking.md` found by trying to lower against this
   section. A value may be moved on one path and not another:

   ```science
   let doc be Doc.new(path)
   if urgent?:
       consume(doc)          # moved here, on this path only
   # is `doc` dropped at the end of the scope, or was it already?
   ```

   Neither answer is right for both paths, and neither region inference nor
   codegen can invent one: the region engine needs the drop to already be a
   point it can constrain against, and codegen needs to know whether to emit
   the call. **Decision 26: a local that is conditionally moved gets a
   compiler-generated drop flag — one byte, set where the value is
   initialised, cleared where it is moved, tested at the drop point — and
   elaboration runs during MIR construction, before region inference.**

   It is Rust's solution and it is Rust's for the reason that applies here
   too: the alternative is to reject the program, which forbids an idiom that
   is ordinary in a language with moves, or to drop unconditionally, which
   double-frees. `crates/science-rt`'s contract is explicit that a double
   `_free` is a bug it will not catch, so this is the phase that has to be
   right.

   **The cost is a byte and a branch per conditionally moved local**, and it
   is the one place in the language where the compiler adds a runtime check
   the author did not write. LLVM eliminates the flag entirely when the move
   is unconditional or the paths rejoin trivially, which is most of them; it
   cannot when the condition is genuinely dynamic, and then the branch is
   real. **Flags are generated only for locals the move analysis proves
   conditionally moved**, never for every local, which is the difference
   between a rare cost and a tax on every function.
6. **Stable point identity across the query boundary**, so §10's cache hits.

And one requirement in the other direction, which that note did not anticipate:

**Decision 25. Region inference runs after type checking and before
monomorphisation, and narrowing (§4.2) depends on rule 4 being enforced.** The
order is: types → THIR analyses → MIR → regions → mono → codegen. Decision 8's
soundness argument is a dependency on a later phase, which is legal only because
narrowing's *conclusion* is not used until after regions have run. If a future
change makes a THIR analysis consume narrowing results that codegen relies on,
this argument has to be revisited.

---

## 13. Diagnostics

Claimed: **`SC0520`–`SC0579`** in the Types second band.

Amended, not claimed: `SC0260`–`SC0262`, which are
`const-expression-arithmetic.md`'s and are implemented here (§8).

Referenced: `SC0140`, which is `syntax-revision-2.md`'s (§5).

Named above: `SC0520` (`T??`), `SC0521` (`TryIterate` outside a failure
context), `SC0522` (generic function across the C boundary).

---

## 14. Deliberately not built

- **An array-level IR.** §3.4 keeps it possible and F1 builds it.
- **Higher-kinded types, associated constants, specialisation, negative
  reasoning.** None has a caller.
- **Subtyping**, apart from the implicit coercions of Decisions 6 and 14 and
  the unsizing of §6.2's amendment — which *is* a subtyping relation, narrow
  and behind a borrow, and saying otherwise would be a word game.
- **Global type inference.** §2, and it stays rejected.
- **Effect checking.** `effects.md` owns it; this note only notes that §9.3's
  `python:` region is an effect boundary in everything but name, and the two
  designs should be reconciled before either ships.

---

## 15. Risks

**Narrowing's dependency on rule 4 is a phase-ordering argument, not a proof.**
Decision 8 is sound because the conclusion is consumed after regions run. It is
the kind of argument that stays true until someone adds a THIR pass that uses
narrowing for something codegen depends on, and then it is silently false.

**`SC0140`'s false-positive list is a guess.** Four exclusions are named in §5
and they came from reading the corpus, not from running the analysis over it.
The analysis is cheap; the list is what makes it tolerable, and the only way to
find the fifth exclusion is to ship it and be wrong.

**`python:` is a special case for one foreign language.** §9.3 states it. It is
justified by the ecosystem goal and it is the largest exception in the type
system, and a reviewer who has not read `python-from-science.md` §2.2 will think
it is unprincipled — because from inside this note alone, it is.

**The const-generic monomorphisation key is a correctness trap with a late
failure.** Item 4 of §8: getting it wrong emits two symbols for one function,
which is not a type error, not a link error on most platforms, and shows up as a
performance mystery or a pointer-equality failure long after the cause.

**Nobody has checked that scikit-learn actually works this way.** §9.3 is
designed from `python-from-science.md`'s analysis of 26 elementary operations.
That note reports reaching 46% without the region and claims the region closes
it, but no Science program has ever called a Python function, because there is
no code generator. **This is the largest untested assumption in the project**,
and it sits underneath the goal the project says it cannot do without.
