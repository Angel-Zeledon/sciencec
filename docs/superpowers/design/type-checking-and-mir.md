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
- **Subtyping**, apart from the two implicit coercions of Decisions 6 and 14.
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
