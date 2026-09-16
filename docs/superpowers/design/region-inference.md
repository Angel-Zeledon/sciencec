# Region inference

**Status.** Design. Nothing below is built. `crates/` ends at the resolver:
there is no type checker, no MIR, and no borrow checker.

**Why this note exists.** §6.1 of the core spec ends with one sentence —
*"There is no lifetime syntax. The programmer never writes a region."* — and
that sentence is the language's single largest claim. §6.2 gives half a page on
how it is meant to work. This note is the rest, and it is written now rather
than when the checker is built because the project has committed to
**self-hosting**, and a compiler is the program that breaks this claim if
anything does.

---

## 0. The frame: this is on the bootstrap's critical path

`self-hosting.md` sets the order. Codegen is what *blocks* a bootstrap; region
inference is what *risks* it. The two orderings are opposite, and a plan that
respects only the first discovers the risk at stage three, with the lexer and
the parser already ported.

So this note answers three questions at once, and the third is the one usually
left out:

1. **Is it sound?** The ordinary question.
2. **Can it be inferred without syntax?** The language's claim.
3. **Can the engine itself be written in Science?** Because it will have to be.

Question 3 is not decoration. A region solver is a worklist dataflow over a
control-flow graph with a constraint graph hanging off it — a mutable
work queue, a constraint arena, and back-references from constraints to the
points that produced them. That is a *cyclic, aliasing, mutable* structure, and
it is the exact shape §4 and §9 below are about. If the engine cannot be
written under its own rules, the bootstrap stops at stage four and the failure
is self-inflicted.

`examples/21_compiler_shapes.science` is the concrete half of this note. It
encodes the real compiler's structures in Science and it parses and resolves
clean today. **What it proves is that the shapes are sayable, not that they are
sound** — there is no checker to say more. This note is what would make the
second half checkable.

---

## 1. What §6.2 already decided, and the three holes it left

§6.2 is more specific than it is usually given credit for. It commits to:

- **A region is the set of program points where a borrow is live.** That is
  Rust's non-lexical lifetimes (NLL) formulation, not Polonius's. It is a
  liveness property over a control-flow graph.
- **A fresh region variable per borrow and per reference in a type.** Note the
  second half. It has a consequence §6.2 does not draw, and §4 draws it.
- **Outlives constraints, propagated to a fixed point, then a conflict check.**
- **Interprocedural**, by reverse topological order of the call graph, with a
  fixed point over each strongly connected component for mutual recursion.
- **Provenance on every constraint**, with the warning that a solver written
  without it must be rewritten entirely to gain it.
- **`borrows_live_at(point)` is a public query**, which F5 uses to reject a
  borrow live across a suspension point.

That is a real design and this note does not overturn it. What it leaves open:

| Hole | Where it bites |
|---|---|
| How many region parameters a **type** gets, and how they relate | `ffi-c-boundary.md` §2 depends on it by name; `FftwPlan` has two borrowed fields |
| What happens when inference is **ambiguous** | Rust reports an error and makes you write `'a`. Science cannot. |
| Whether interprocedural inference is compatible with **separate compilation** | §11.4 makes incremental recompilation part of "done" |

§4, §5 and §6 are those three.

---

## 2. What a region is here

**Decision 1. A region is a set of MIR points, and the lattice is set
inclusion.** A point is a `(basic block, statement index)` pair. `'a: 'b`
("`'a` outlives `'b`") is read as `'b ⊆ 'a`, and the solver's job is to grow
each variable to the least set satisfying every constraint.

This is NLL as RFC 2094 describes it, and the choice is deliberate. Polonius
restates the same problem as a Datalog fixpoint over `loan_live_at` and gets
better answers on a handful of programs — the famous one being a conditional
return of a borrow out of a map lookup. It costs a Datalog engine inside the
compiler, and it computes the answer for *every* point rather than the points
in question.

**Cost, stated.** The NLL formulation rejects programs that are actually sound.
The canonical one is the "get or insert" shape, which every symbol table wants:

```science
# Rejected under Decision 1, and sound.
function intern(mutable self, name: String) -> borrowed Symbol:
    let found be self.table.get(name)
    if found?:
        return found
    self.table.insert(name, Symbol.new(name))
    self.table.get(name)
```

The borrow taken by the first `get` is held by `found`, and under a points-based
liveness analysis it is live on the path where `found` is null too, because the
region is one set and the set contains the insert. Polonius separates the
*loan* from the *path* and accepts it.

That this example is a symbol table is not a coincidence and it is why the case
is in this note rather than a footnote. The workaround is the one
`examples/21_compiler_shapes.science` §2 already takes for a different reason:
**return an id, not a borrow.** The note takes that as the answer for F0 and
records Polonius as an F2 upgrade that changes no syntax and no signature —
only which programs are accepted. Upgrading a checker to accept *more* is
always compatible; that is why this order is safe.

---

## 3. The algorithm, and what it costs

Per function, over its MIR:

1. **Liveness.** A standard backward dataflow over the CFG giving, per point,
   the set of live variables. Regions of live variables are live.
2. **Constraint generation.** A forward walk emitting `'a: 'b @ p` for every
   assignment, call, return and coercion. Each constraint carries the span and
   a cause — `AssignedFrom`, `PassedTo`, `ReturnedFrom`, `StoredInField`.
3. **Propagation.** Grow each region along the CFG from the points where it is
   required to be live, respecting the constraints. This is the standard
   worklist fixpoint.
4. **Conflict check.** For every exclusive borrow, no other borrow of an
   overlapping place is live at any point in its region; for every shared
   borrow, no exclusive one is. Rule 5 — no borrow outlives its referent — falls
   out as a constraint against the referent's storage-dead point.

**Complexity.** Steps 1 and 3 are `O(points × regions)` in the worst case with a
bitset representation, and linear in practice because the constraint graph is
sparse. This is the part of a compiler that is well understood; the design risk
here is nil and the engineering risk is provenance, not speed.

**Decision 2. Every constraint carries its span and its cause, and the solver
never merges two constraints.** §6.2 requires this and it is worth restating as
a decision because it is the one that is cheap now and impossible later. A
solver that unions constraints for speed has thrown away the only material an
error message can be built from, and — this is the part §6.2 does not say —
**Science needs the material more than Rust does**, because Rust can fall back
on printing `'a` and Science has nothing to print. See §7.

---

## 4. Regions on types, and the case the claim turns on

This is the section the note exists for.

§6.2 says a fresh region variable is assigned to "every reference in a type".
Read literally, a type with two borrowed fields gets **two** region variables.
The core spec never says so, `ffi-c-boundary.md` §2 depends on the answer by
name, and `self-hosting.md` §4.4 records the hole.

### 4.1 One borrowed field is uncontroversial

```science
type Parser:
    tokens: borrowed Array of Token
    position: Int
```

One field, one source, one region. There is nothing to name because there is
nothing to relate: the type outlives nothing and is outlived by one thing.
`Parser<'t>` in the real compiler is exactly this and its `'t` carries no
information a solver could not recover.

### 4.2 Two borrowed fields: the evidence said this never happens, and the design already has one

I checked the existing compiler before writing this. Seven types carry a
lifetime parameter — `Annotation`, `FileGroup`, `Lexer`, `Parser`, `Node`,
`Input`, `Change` — and **not one of them carries two**. Outside a debug log
there is no shared ownership or interior mutability in non-test code at all.
That is real evidence and it is what made "one region is enough" look like a
safe bet.

It is not a safe bet, and the counterexample is already in this repository.
`ffi-c-boundary.md` §2 writes:

```science
type FftwPlan:
    raw: ffi.OpaqueHandle
    input: ffi.MutableSpan of F64
    output: ffi.MutableSpan of ffi.Complex64
```

**Two borrowed fields, from two unrelated buffers**, and that note's whole
argument is that "the region engine now knows that the plan borrows both". The
same shape recurs wherever a foreign library keeps a pointer it was handed:
cuSPARSE descriptors, cuDNN workspaces, BLAS batched-operation handles. It is
not exotic; it is the FFI case, and FFI is the thing this project has invested
most heavily in.

### 4.3 The decision

**Decision 3. A type gets one region variable per borrowed field, and the
relation between them is inferred at every use site rather than declared once
at the type.**

The type declaration says only *that* the fields are borrowed. What Rust
expresses by writing `FftwPlan<'a>` and putting `'a` on both fields — an
assertion that the two outlive the same thing — Science does not assert
anywhere. Instead, each construction of an `FftwPlan` generates constraints
from *that* construction's arguments, and the plan's own region is the
intersection: it is live only where both of its borrows are.

**What this buys.** The `Node of T` case in
`examples/21_compiler_shapes.science` §4 — two borrowed fields that in Rust
were deliberately given the *same* name — is accepted without anyone deciding
they are the same, because at every real construction site they are. Rust's
single `'a` there was a convenience that discarded information; inference keeps
it.

**What it costs, and it is not small.**

1. **A type's region is no longer a property of the type.** Two `FftwPlan`
   values have different regions. Anything that wants to speak about "the
   region of an `FftwPlan`" — an error message, a public API summary, a
   cross-crate metadata record — has to speak about a *value* instead.
2. **The intersection can be empty**, and an empty region means the value is
   dead at birth. That is a legal outcome of the solver and a terrible error
   message, so §7.3 makes it a named diagnostic rather than a generic
   conflict.
3. **It pushes work to the use site**, which is precisely what makes §6
   hard.

**Decision 4. A type may not be *generic* over a region.** There is no way to
write, and no way to infer, "a `View` whose borrow outlives this other thing"
as a bound on a type parameter. `where T: 'a` has no spelling and gets none.
Where Rust would need it, Science requires the value to be owned. This forecloses
a real class of API — a generic container parameterised by how long its
contents live — and the honest statement is that nobody has yet found a
scientific-computing use for one.

---

## 5. Signatures with no syntax

Rust's lifetime elision rules exist because most signatures have one obvious
answer and writing it out was noise. They are a *convenience over* a syntax that
remains available when elision guesses wrong. Science has no syntax to fall back
on, so elision is not a convenience here — it is the whole mechanism, and it has
to be total.

### 5.1 The elision rules

**Decision 5. A function signature's regions are an analysis result, not a
declaration, and there are no elision rules at all.**

This sounds like a non-answer and it is the opposite. Rust's rules ("each
elided input lifetime gets its own parameter; if there is exactly one, it is
assigned to all elided output lifetimes; if there is a `&self`, its lifetime is
assigned to all elided output lifetimes") are a *syntactic guess* at what the
body will need, made before the body is looked at. §6.2 already decided to look
at the body: analysis is interprocedural and a function's parameter and return
regions come out of its own analysis. Once you are willing to read the body,
guessing from the signature is strictly worse.

So: analyse the body, compute what relations actually hold between the
parameters' regions and the return's, and *that* is the signature.

**The cost is §6, and it is the largest cost in this note.**

### 5.2 Ambiguity: what happens when there is no single answer

Rust's rules can produce a signature the body then contradicts, and the compiler
says "this function's return type contains a borrowed value, but the signature
does not say whether it is borrowed from `x` or `y`" and makes you write `'a`.
Science cannot make you write anything.

There are three possible answers and only one survives.

- **Pick a default** — say, the first parameter. Rejected: it makes a program's
  meaning depend on parameter order, and the failure is silent. A function that
  should borrow from `y` compiles, and the error surfaces at a call site in
  another file as a borrow that does not live long enough.
- **Infer across the call graph** and let each call site constrain it. This is
  §6.2's stated design taken to its conclusion, and §6 is about why it is
  expensive. It is also not always decisive: a function called once has its
  answer chosen by its only caller, which is inference by accident.
- **Refuse the program.**

**Decision 6. When a function body does not determine the relation between its
regions, the program is rejected with `SC0340`, and the fix offered is to change
the signature to return an owned value or an index.**

That is uncomfortable — the compiler is rejecting a program for being ambiguous
rather than wrong — and it is the honest consequence of removing the syntax. The
mitigating fact, and the reason this is tolerable, is that the ambiguous
signatures are overwhelmingly ones that *should* return an owned value anyway.
`examples/21_compiler_shapes.science` §2 makes exactly this choice for
`Scopes.lookup`, and makes it on design grounds rather than to please a checker:
a symbol-table lookup that hands back a borrow is wrong because the caller
almost always goes on to mutate.

**The risk this creates is stated in §14 and it is the note's biggest:** if
`SC0340` fires often in real code, the no-syntax decision is wrong and the
language needs a way to say what it means.

---

## 6. Interprocedural inference against separate compilation

**This is a contradiction between the core spec and itself, and neither section
knows about the other.**

§6.2 makes region inference interprocedural: functions are analysed in reverse
topological order of the call graph, and a function's parameter and return
regions are part of its analysis result.

§11.4 makes incremental recompilation part of the definition of done, "proved by
execution counts". §7.3 says the same query database serves F1's interactive
tier.

These pull in opposite directions, and the conflict is not a detail:

- If a function's signature regions are computed from its body, then **changing
  a function body can change its signature**, which invalidates every caller's
  region analysis transitively.
- **A module cannot be checked without its dependencies' bodies.** Rust
  deliberately put lifetimes *in the signature* so that a crate can be compiled
  against another crate's interface. Science has thrown that property away as a
  consequence of removing the syntax, and no note has noticed.
- Across a package boundary this is worse than a rebuild cost: a compiled
  Science library would have to ship the MIR of every function that returns a
  borrow, or ship a summary that is isomorphic to the lifetime annotations the
  language refuses to let anyone write.

**Decision 7. Region inference is interprocedural *within* a crate and
signature-summarised *across* crates. A function that crosses a crate boundary
has its inferred region relation serialised into crate metadata, and callers in
other crates read the summary rather than the body.**

The summary is a small relation: for each return region, which parameter
regions it is constrained by. It is not a lifetime annotation — nobody writes it
and nobody reads it — but it is **exactly the same information**, and this note
should say so plainly rather than pretend otherwise. The language's claim is
that a *programmer* never writes a region. It was never that the compiler does
not need one.

**Decision 8. The query boundary for incremental recompilation is the strongly
connected component of the call graph, not the function.** §6.2 already needs
SCCs for mutual recursion; this reuses them. Changing a body invalidates its
SCC's analysis and the analyses of every SCC downstream of it in the call graph,
and nothing else.

**Cost.** A change inside a large SCC re-analyses the whole component, and the
component can be large — mutually recursive descent parsers are one big SCC, and
the Science compiler will contain one. That is a real, measurable incremental
recompilation cost, incurred by exactly the program this language most wants to
compile. §14 records it as a risk rather than pretending the SCC boundary is
free.

---

## 7. Errors, with nothing of the user's to blame

Rust's borrow-checker messages are the hardest part of that language to read
*even though* they can point at a name the programmer wrote. Science cannot
print `'a`. This section is either the strongest argument against the no-syntax
decision or the best thing about it, and which one depends entirely on the
effort spent here.

### 7.1 The shape of the message

**Decision 9. A borrow error is reported as a narrative over three spans — where
the borrow was born, what keeps it alive, and where the conflict is — and never
as a relation between region variables.**

```
error[SC0330]: `defs` is borrowed here and modified before the borrow ends
  --> resolve.science:88:18
   |
86 |     let def be defs.get(id)
   |                ---- `defs` is borrowed here, shared
87 |
88 |     defs.alloc(DefKind.Local, name, null)
   |     ^^^^ ...and modified here, which needs it exclusively
89 |
90 |     print(def.name)
   |           --- the first borrow is still needed here
   |
   = note: a shared borrow and an exclusive one cannot overlap (§6.1 rule 4)
   = help: the borrow ends at its last use; moving line 90 above line 88 is
           enough, or bind `def.name` before the call
```

The three spans are exactly what the provenance chain of Decision 2 records.
The "still needed here" span is the load-bearing one and it is the one a solver
without provenance cannot produce: it is the *last use*, which is what makes
the region end where it does, and without it the message is "these two
conflict" with no explanation of why the first one is still around.

### 7.2 What is strictly better than Rust here

A named lifetime lets a message say `'1` and `'2` and leaves the reader to work
out what those are. With no names available, the message has no choice but to
describe the program. That constraint is worth accepting on purpose: **the
error message is forced to be the narrative, because it cannot be the algebra.**

### 7.3 The named failures

Generic "these conflict" messages are where borrow checkers go to die. Four
shapes get their own diagnostic and their own explanation:

| Code | Shape |
|---|---|
| `SC0330` | Shared and exclusive borrows of the same place overlap |
| `SC0333` | A borrow outlives its referent (rule 5) |
| `SC0334` | A value is moved while borrowed |
| `SC0335` | A type's fields are borrowed from sources with no common region — Decision 3's empty intersection |
| `SC0340` | The body does not determine the signature's regions — Decision 6 |
| `SC0341` | A borrow is live across a suspension point — §6.4's F5 query, reserved |

`SC0301` and `SC0302` (use after move) are the core spec's, `SC0380` is
`ffi-c-boundary.md`'s, and `SC0331`/`SC0332` are `collections-and-chains.md`'s.
The block claimed here is `SC0330`, `SC0333`–`SC0379`.

**`SC0331` is a specialisation of `SC0330` and should be implemented as one.**
That note's `SC0331` is *"`docs` is modified while a chain is still reading
it"* — which is Decision 9's three-span narrative with the middle span being a
chain rather than a call. It is the same analysis, the same provenance chain and
the same message shape, reported with a code that names the situation the reader
is actually in. That is the right relationship between a general checker and a
domain diagnostic, and it is worth recording because the alternative — a second
analysis that happens to agree — is how borrow checkers acquire two answers to
one question. The same is true of `SC0332`, which that note already describes as
a specialisation of `SC0301`.

---

## 8. The compiler's own shapes

Six structures, from `self-hosting.md` §4.6 and
`examples/21_compiler_shapes.science`.

| Structure | Expressible | Why |
|---|---|---|
| Flat `DefTable`, `DefId(u32)` indices | **Yes** | An index borrows nothing. The graph has no region at all. |
| `Rib` scope stack | **Yes** | Owns its names; borrows nothing. |
| `Parser` holding a token slice | **Yes** | §4.1: one borrowed field, one region. |
| `Node of T` — two borrowed fields | **Yes**, under Decision 3 | Both constructions in the real dumper pass borrows of the same table and value; the intersection is non-empty. |
| Diagnostics accumulator | **Yes** | Exclusive borrow of the sink, shared of the tree; different objects, no aliasing. |
| Bump arena handing out borrows | **No** | §9. |

**Five of six, and the sixth is not used by the existing compiler.** That is a
genuinely encouraging result and it must be read with the caveat
`examples/21_compiler_shapes.science` §5 states: the hardest structure in a
compiler was already written index-not-pointer, for resolution reasons, which is
the one style with no region question. The test is easier than the general case
by exactly the amount its original authors happened to avoid.

**The interner is the unresolved one.** `Ident` holds an owned `String` today,
so the front end has no interner and the question has never been asked. The
shape — one table owning every string, handing out shared borrows valid for the
whole compilation — is §9's shape, and a self-hosted compiler will want it for
the same reason every compiler does.

---

## 9. What is not supported, and the escape hatch

`rust-interop.md` §3.7.2 records that Rust has `Cell`, `RefCell`, `Mutex` and
`UnsafeCell`, and that **Science's rule 4 admits none of them**. Rule 4 is
absolute: shared many, or exclusive one, never both.

That leaves three shapes with no spelling:

1. **Shared mutable state.** No `RefCell`.
2. **Cyclic ownership.** No `Rc`, so no doubly-linked list, no back-pointer from
   child to parent.
3. **An arena handing out borrows.** `fn alloc(&self) -> &T` — the arena is
   borrowed shared and the result borrows from it, which requires the arena to
   promise it will not move or free what it has handed out. In Rust that
   promise is `UnsafeCell` plus a lifetime.

**Decision 10. The escape hatch is `unsafe` and the indices pattern, in that
order of last resort.** Shapes 1 and 2 are answered by indices — which is what
the existing compiler already does, and which is a better answer than `Rc` for a
compiler regardless. Shape 3 is answered by `unsafe` and a hand-written
invariant, exactly as Rust's own arenas are.

**The cost is a hole in the story.** A language that says "ownership verified at
compile time, no annotations" and then requires `unsafe` for an interner has an
asterisk on its headline. This note does not remove it. What it does is name
where the asterisk is, so that a decision to close it later — region-generic
types, an `arena` primitive in the standard library with the `unsafe` written
once — is a decision and not a rescue.

---

## 10. What this needs from MIR

Addressed to `type-checking-and-mir.md`, which is unwritten. These are
requirements, not suggestions; every one of them is something a dataflow pass
cannot add afterwards.

1. **An explicit CFG** with basic blocks and a statement index per point. The
   region lattice is defined over these; there is no analysis without them.
2. **Explicit places.** Every borrow names a *place* — a local, a field
   projection, an index — not an expression. Aliasing is decided by comparing
   places, and two expressions that denote the same place must produce the same
   place.
3. **Explicit `StorageLive` / `StorageDead`.** Rule 5 is checked against the
   referent's storage-dead point. Without it, "no borrow outlives its referent"
   has nothing to compare against.
4. **Two-phase borrows, or an explicit statement of their absence.** `v.push(v.len())`
   needs the exclusive borrow of `v` to be reserved at the call and activated
   after the arguments are evaluated. This is not a region question but it is
   decided in the same pass, and if MIR does not expose the reservation point,
   the common case does not compile.
5. **Drop points made explicit before this pass runs.** A `Drop` implementation
   takes `mutable self`, so a drop is an exclusive borrow at a point the user
   did not write. Implicit drops that appear after region inference would
   invalidate its result.
6. **Stable point identity across the query boundary.** Decision 8 memoises per
   SCC; if MIR point numbering changes when an unrelated function is edited, the
   cache never hits.

---

## 11. Can the engine be written in Science?

§0 said this question is not decoration. Taking the algorithm of §3 apart:

- **The CFG** is a vector of blocks with integer successors. Indices, no
  regions. Fine.
- **Liveness sets** are bitsets — owned `Array of U64` per point. Fine.
- **The constraint set** is a vector of `(region, region, point, cause)`
  records, all owned. Fine.
- **The worklist** is an `Array of RegionId`, owned. Fine.
- **The region variables** are a flat table indexed by `RegionId`, which is the
  `DefTable` shape of §8 exactly. Fine.
- **Provenance back-references** are the one risk: a constraint wants to point
  at the span and cause that produced it. If that is a `borrowed Constraint`,
  the solver holds borrows into a table it is also growing — the
  `intern`-shaped problem of §2. If it is a `ConstraintId` index, it is fine.

**Decision 11. The engine is written index-not-pointer throughout, and this is
recorded as a constraint on its implementation rather than discovered during the
port.** Every structure above is then expressible under the rules this note
specifies, with no `unsafe` and no arena.

That is a real answer to question 3 of §0, and it comes with a real caveat: it
is achievable *because the same discipline was already imposed on the existing
Rust compiler for unrelated reasons*. A region engine written in the natural
pointer-rich style would need §9's escape hatch, and the reason this one does
not is a habit, not a guarantee.

---

## 12. Diagnostics

Claimed: **`SC0330`, `SC0333`–`SC0379`** in the Ownership range.

Not claimed and not reused: `SC0301`, `SC0302` (use after move, core spec §6.1
rule 3), and `SC0331`, `SC0332`, `SC0380`, which are `ffi-c-boundary.md`'s.
`SC0341` is allocated but reserved: it is F5's suspension-point check and there
is no F5.

---

## 13. Deliberately not built

- **Polonius.** §2. An F2 upgrade that accepts strictly more programs and
  changes no syntax.
- **Region-generic types.** Decision 4.
- **Variance.** Rust needs subtyping between regions because it has
  `&'long T <: &'short T`. Decision 1's lattice gives the same effect by set
  inclusion, and with no region syntax there is no user-visible variance to
  specify. If a later phase adds region-generic types, variance comes back with
  them and this line is where to start.
- **A `borrow` checker for `static`s.** F0 has no mutable globals.
- **Anything about threads.** `stdlib-standard.md` §10 gives `Mutex`, `RwLock`
  and `Atomic`, and `rust-interop.md` §3.7.2 says rule 4 admits none of them.
  These are reconcilable — Science's `Mutex` *owns* its contents rather than
  providing interior mutability over a shared borrow — but the two sentences
  read as contradicting and neither note has noticed. **Not resolved here**; it
  belongs to whichever note owns concurrency.

---

## 14. Risks

**`SC0340` is the language's bet.** If "the body does not determine the
signature" fires in ordinary code rather than on pathological signatures, the
no-syntax promise is unkeepable and the fix is a syntax. There is no way to know
before there is a checker and a corpus to run it over. The cheapest early
signal is `examples/21_compiler_shapes.science`: if it needs `SC0340` anywhere,
the bet is in trouble.

**Separate compilation was traded away without anyone deciding to.** §6 is a
contradiction between two spec sections written at different times. Decision 7
resolves it by serialising a signature summary that is *isomorphic to the
lifetime annotations the language refuses to let anyone write*. That is not a
defeat — the claim was always about the programmer, not the compiler — but it
is a fact the marketing should never be allowed to contradict.

**The SCC query boundary is coarse where it hurts most.** A mutually recursive
descent parser is one strongly connected component, the Science compiler will
contain one, and Decision 8 re-analyses the whole component on any change inside
it. The language's flagship self-hosted program is the worst case for its own
incremental recompilation.

**Two-phase borrows are a silent prerequisite.** §10 item 4. If MIR ships
without them, `v.push(v.len())` does not compile, the failure looks like a
region bug rather than a lowering gap, and it is found by users rather than by
tests.

**The interner remains unanswered.** §8. Every compiler wants one, this one does
not have one yet, and the shape it needs is the shape §9 says has no spelling.
The bootstrap will meet this question at stage two or three, which is late.
