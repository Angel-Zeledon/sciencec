# Science — Design: the effect discipline

Date: 2026-09-16
Status: draft for review
Area: what an effect is in Science, how it is obtained, what it is allowed to
decide, and what F0's type checker must reserve room for.
Depends on: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
(§1 the claim, §2 the phases, §3 the query architecture, §5 types, §6 ownership
and regions, §12 what is out of scope, §13 the reserved list).
Reconciles rather than re-specifies: `python-interop.md` §5.2 and §6.3 (the
`python` bit and the GIL), `stdlib-standard.md` §5.3 and §8.2 (the ambient-state
test, and `SC0270`), `collections-and-chains.md` §7 (what `.parallel()` must
reject), `ffi-c-boundary.md` §3 (what `unsafe` means),
`stdlib-shape-and-packages.md` §3 (threads, no async),
`scientific-libraries.md` §2 (why the RNG must be written in Science).
Syntax: `syntax-revision-2.md`.

---

## 0. The claim this note is built on

§1 of the core spec makes a five-part competitive claim:

> None of them has all of static shapes, **an effect discipline**,
> ownership-tracked device memory, an interactive tier, and zero-copy Python
> interop. That combination is the gap Science aims at.

Four notes have already spent that claim. None of them designed it.

| Note | What it assumed | What it needs the effect to decide |
|---|---|---|
| `python-interop.md` §5.2, §5.4, §6.3 | *"a function carries the `python` effect if it calls a Python-touching primitive, and effects propagate through calls"* | Whether the generated shim releases the GIL; whether a closure may enter a parallel region (`SC0454`) |
| `stdlib-standard.md` §5.3 | *"`mutable borrowed random.Stream` in a signature is a compiler-checked declaration that a function is not deterministic […] an effect discipline obtained for free from §6's borrows"* | Nothing, yet — it is an observation, and it is the best idea in this area |
| `collections-and-chains.md` §1.5, §7 | *"§1 names an effect discipline as part of what Science is betting on. Standardising a side-effect combinator before the effect system arrives is the wrong order"* | Which closures may run under `.parallel()` |
| `scientific-libraries.md` §2 | *"That property is unavailable if the generator is C state behind a pointer. **This one must be Science.**"* | Nothing directly — but it is the reason the free discipline of §2 below holds at all |

So the pieces exist and the seams are visible. This note draws them together, and
it does so under one constraint that shapes every decision below:

> **Science is not getting an effect system. It is getting the smallest thing
> that answers six specific questions, most of which turn out not to be effect
> questions at all.**

§12 of the core spec puts higher-kinded types out of scope for F0. An effect
system in the Koka or Frank sense — effect rows, effect polymorphism, handlers —
is a larger feature than higher-kinded types and would arrive in a language that
has explicitly declined them. That is not a close call, and the rest of this note
is about what to build instead.

### 0.1 Syntax this note assumes

- `pure` moves from §13's *reserved, not yet used* list to the in-use list. It is
  the only word this note spends, and it is the only one it needs.
- `parallel` stays reserved and unspent here; `collections-and-chains.md` §7 owns
  it as `.parallel()`, a method, not a keyword.
- No new reserved word is requested. That was a constraint, not an outcome: an
  effect discipline that costs the language a keyword per effect is how effect
  systems get to be resented.

---

## 1. The six questions, and which of them are the same mechanism

The temptation is to design "effects" and then check what they answer. The
opposite order is much stronger here, because the six questions in play turn out
to have three different answers, and only one of them is an effect.

| # | Question | Who asks | Answered by |
|---|---|---|---|
| 1 | May this closure run in a parallel region? | `collections-and-chains.md` §7, F2 | **Ownership**, mostly (§2), plus one effect bit |
| 2 | Does this function touch Python, and therefore hold the GIL? | `python-interop.md` §5.2 | **An effect bit** (§3) |
| 3 | Is this deterministic — cacheable, and permitted in a reproducible run? | F1's interactive tier, F5's replay, the package manager's reproducibility story | **Ownership** (§2), plus two effect bits (§3) |
| 4 | Does it allocate on a device, and which? | F2 | **The type.** Not an effect (§6.1) |
| 5 | Is it differentiable? | F2's `grad` | **The body, at the transformation site.** Not an effect (§6.2) |
| 6 | Does it touch the filesystem or the network? | F5, a reproducible-run mode | **An effect bit** (§3) |

Three observations follow, and they are the whole analysis.

**Questions 2 and 6 are the same mechanism.** Both ask *"does anything reachable
from this function call a primitive of a named kind?"* That is a reachability
property over the call graph: one bit per kind, seeded at a closed list of
runtime primitives, propagated through calls, conservative at dynamic dispatch.
It needs no types, no polymorphism, no handlers, and no syntax. It is the only
thing in the six that a new mechanism has to supply.

**Questions 1 and 3 are mostly *not* that mechanism.** They are dominated by
facts the ownership checker and the type checker already have, and §2 is about
how much. The effect bits contribute a corner of each.

**Questions 4 and 5 are not effects and putting them in an effect system makes
them worse.** §6 argues both, and the arguments are different from each other.

---

## 2. What Science already has, before a single new line of compiler

`stdlib-standard.md` §5.3 made the observation this section develops:

> **`mutable borrowed random.Stream` in a signature is a compiler-checked
> declaration that a function is not deterministic.** A function that draws must
> say so in its type, because it cannot reach a generator any other way. This is
> an effect discipline obtained for free from §6's borrows.

The operative clause is *because it cannot reach a generator any other way*. That
is not a property of borrows. It is a property of **a language with no ambient
state**, and Science's is unusually complete. It is worth enumerating, because
the enumeration is the answer to "how much of the effect discipline is already
paid for".

### 2.1 The ambient-state inventory

Every channel through which a function in a mainstream language reaches state its
signature does not mention, and where Science closed it:

| Ambient channel | Status in Science | Closed by |
|---|---|---|
| Mutable globals | Do not exist. `const` only; `static` is reserved and unused | Core spec §4.4, §13 |
| A process-global RNG | Does not exist. No `random.seed`, no free `random.uniform()` | `stdlib-standard.md` §5.3 |
| Aliased mutable state reached through a parameter | Rule 4: exclusive borrow, once | Core spec §6.1 |
| Silent view aliasing (NumPy's) | A compile error | Core spec §6.5 |
| A garbage collector's timing | No GC | Core spec §3 |
| An exception unwinding past your frame | `panic` aborts; there is no `catch` | Core spec §8, `python-interop.md` §7.2 |
| Implicit numeric coercion changing a result | `as` is always written | Core spec §5.1 |
| Unspecified reduction order | Defined unless marked `unordered` | Core spec §5.1 |
| A hidden async scheduler | No `async`; threads only, scoped | `stdlib-shape-and-packages.md` §3 |
| A process-global logger | **Exists, deliberately** | `stdlib-standard.md` §8.2 |

One row is not closed, and that note closed it correctly rather than pretending:

> **Ambient state is acceptable exactly when it cannot change the program's
> computed result.** A global RNG changes the answer. A global logger changes
> what is printed.

That sentence is the criterion this note formalises. It is why §3 has two
separate bits where a lazier design would have one.

### 2.2 What the signature already tells you

Given §2.1, a Science signature is already a far stronger statement than the same
signature in Python, Julia, C++ or Rust. Read one:

```science
def fit(
        data: borrowed Array of Measurement,
        weights: mutable borrowed Array of F64,
        key: Key,
        tolerance: F64)
        -> (Parameters, Error?)
```

Without any effect machinery, and **with no annotation the author had to write**,
the compiler and a human reader both already know:

1. `data` is not modified. Rule 4 and the absence of `mutable`.
2. `weights` **is** modified, and nothing else in the program can observe it
   during the call, because the exclusive borrow is exclusive.
3. The function is non-deterministic in the RNG sense, because it consumes a
   `Key` — and consuming it means the caller cannot reuse it, which is core spec
   §6.5's "random keys that cannot be reused", `SC0301`.
4. It reads no other generator, because `random` has no ambient one to reach.
5. It reaches no global state, because there is none.
6. It cannot spawn a thread that outlives it holding these borrows, because a
   detached thread may capture no borrows (`stdlib-shape-and-packages.md` §3.3).
7. It may fail, and the caller must test `err?` before using the value
   (`SC0140`, `SC0276`).

That is seven properties an effect system would have to declare, obtained from
ownership, from types, and from a standard library that refuses ambient state.

**This is the large majority of the value.** A reasonable estimate: of the
verification an effect discipline is supposed to buy a scientific language,
ownership and the no-ambient-state rule deliver most of it, and the residue — the
part §3 designs — is three bits.

### 2.3 Decision: the free discipline is the discipline, and it is not extended

> **Decision 1. Mutation, aliasing, non-determinism-from-randomness, exclusive
> access, and cross-thread safety are ownership and type questions, and this note
> adds nothing to them. No effect bit duplicates a fact a signature already
> carries.**

**Rejected: a `mutation` effect.** Every language with an effect system has one,
and in Science it would be exactly the set of functions with a `mutable borrowed`
parameter or receiver. A bit that is a syntactic function of the signature is not
information; it is a second spelling, and §4.6 of the core spec spent a whole
revision removing second spellings for the same reason a formatter cannot
canonicalise between them.

**Rejected: a `random` or `nondeterministic` effect.** `stdlib-standard.md` §5.3
already gets it from the type of the parameter, and gets something a bit could
not: it distinguishes `Key` (reproducible, splittable, consumed) from `Stream`
(sequential, order-dependent, exclusively borrowed) and warns at the published
boundary (`SC0270`). A single bit would flatten that distinction, which is the
whole design.

**Rejected: a `thread` or `send` effect.** `Share` and `ShareBorrowed`
(`stdlib-standard.md` §10.2) are auto-derived marker interfaces over *types*, and
the question "may this value cross a thread boundary" is a question about the
value, not about the function. Rust's `Send`/`Sync` are the same shape and the
shape is correct.

The cost of Decision 1 is stated in §9: it means Science's effect *system* is
small enough that calling it one oversells it, and §9 recommends what §1 of the
core spec should say instead.

---

## 3. Decision: what an effect is in Science

> **Decision 2. An effect is one of three bits, from a closed set, attached to
> every function by inference over the call graph. The set is not part of any
> type, participates in no unification, and has no polymorphism.**

### 3.1 The three bits

| Bit | Means | Seeded by |
|---|---|---|
| `python` | Reaches a primitive that touches a `PyObject` or the CPython C API | The `python` runtime module; any `use python` call; any function with a Python callable in its parameters (`python-interop.md` §5.4) |
| `ambient` | Reads state supplied by **the machine** rather than by an argument — the wall clock, the OS entropy source, the environment, the process identity, the scheduler, or an address | `time.now`, `Monotonic.read`, `Stream.from_entropy`, `crypto.random_bytes`, `os.env`, `os.pid`, `os.cpu_count`, thread identity |
| `external` | Reads or writes state supplied by **the world** — the filesystem, the network, standard input and output, or a foreign library's state | `read_file`, `write_file`, `File`, `Rows`, `net`, `http`, `print`, `write`, `logging`, every `extern` function that has not declared otherwise (§7) |

**The set is closed.** Adding a fourth bit is a spec change, in the same spirit as
`ffi-c-boundary.md` §3.1's closed list of `unsafe` powers and §8's closed method
sets. This is the sentence that stops the discipline from growing into an effect
system by accretion, and it is the most load-bearing sentence in the note.

### 3.2 Why `ambient` and `external` are two bits and not one

Because they answer different questions, and a design that merges them fails
immediately on the commonest program in the audience.

- A program reads its input file. That is `external` and it is completely fine in
  a reproducible run: the path is in the program, the bytes are in the lockfile's
  world, and two runs over the same file agree. Reproducibility must not forbid
  reading data.
- A program reads the wall clock and seeds from it. That is `ambient`, and two
  runs never agree.

So: **`ambient` is what breaks reproducibility across runs. `external` is what
breaks memoisation and reordering within one.** A single bit would either forbid
`read_file` in a reproducible run, which is absurd, or permit `time.now`, which
defeats the purpose.

This is `stdlib-standard.md` §8.2's test — *ambient state is acceptable exactly
when it cannot change the program's computed result* — turned into two bits
rather than one. The logger's global state is `external` (it changes what is
printed) and not `ambient` (it does not change the answer), which is precisely
the distinction that note drew in prose.

### 3.3 Why `python` is its own bit and not a specialisation of `external`

It is, semantically, a foreign call. It gets a bit of its own for one reason:
it is the only bit whose consumer is **code generation** rather than diagnostics.
`python-interop.md` §5.2 turns it directly into "emit `Py_BEGIN_ALLOW_THREADS`
here or do not", and a wrong answer is a crash rather than a warning. A bit whose
falsity segfaults deserves not to be inferred as a side effect of a coarser
question.

### 3.4 Decision: effects are not in the type

> **Decision 3. A function's effect set is a property of its `DefId`, stored
> beside it, and is never a component of its type. `(F64) -> F64` is one
> type regardless of what the function does.**

This is the most consequential decision in the note and it is a negative one, so
it is worth the space.

If effects were in the type, then:

- The cross-note ask for **closure types spelled `(T) -> U`**
  (`ffi-c-boundary.md` §10.1, `scientific-libraries.md` §14.2,
  `broadcasting.md` — three customers, spelling now decided by
  `collections-and-chains.md` §1.2) would become an ask for
  `(T) -> U does {…}`, and every generic function taking a closure would
  need to be polymorphic over the effect set or else reject half its callers.
  That is effect-row polymorphism, which is Koka, which is the feature §12
  declined.
- `map`'s bound would have to read `F: (Self.Item) -> U` for *some*
  effect row, which means the bound is no longer a bound but a schema.
- Two implementations of the same interface method with different effect sets
  would not satisfy the same signature, so `Doc implements Summarize` would
  depend on what `summarize`'s body happens to call — and adding a `print` to a
  debug build would change whether a type implements an interface.
- Core spec §5.2's inference — *"local, signatures are fully annotated, inside a
  body everything is inferred by unification"* — would have to carry effect
  variables through unification, which is the change that makes Hindley-Milner
  error messages point far from the fault, and §5.2 rejected global
  Hindley-Milner on exactly that ground.

**Rejected: effects as a row in the function type (Koka, Eff, Frank).** More
expressive, and it costs effect polymorphism, effect variables in unification,
and a second kind of generic parameter, for a language that has declined
higher-kinded types. The expressiveness buys nothing any of the six questions
in §1 asked for.

**Rejected: effects as auto-derived marker interfaces on function types, in the
manner of `Share`.** Attractive, because Science already has that machinery and
`Send`/`Sync` prove it works. It fails because a marker interface is on a *type*,
and every closure with the same signature has the same type once §7.6 of
`collections-and-chains.md` records captures; the bit belongs to the *body*, not
to the shape. It would also make the bit participate in bounds resolution, which
is Decision 3's problem again wearing different clothes.

**The cost of Decision 3, stated.** A higher-order function cannot constrain its
argument's effects. `map` cannot say "`f` must be `python`-free"; only
`.parallel()` can say it, at the call site where the closure is a known,
monomorphised body. That is a real expressiveness loss and it is why `SC0454`
fires where it does. It is the right loss: the error at the `.parallel()` call
site can name the offending call chain, which a bound violation could not.

### 3.5 How the bits are computed

A salsa query, `effects(DefId) -> EffectSet`, with the same shape as §6.2's
region inference:

1. Seed the closed list of runtime primitives in §3.1.
2. Walk the resolved call graph in reverse topological order.
3. Resolve mutual recursion by a fixed point over each strongly connected
   component.
4. A closure's set is its body's set, plus the sets of anything it calls through
   its captures.
5. **Conservative at every call whose target is not known**: a call through
   `any Interface`, through a function value of unknown provenance, or through an
   `ffi.Callback` handed back by C, contributes the **full set**. This is
   `python-interop.md` §5.2's rule — *conservative means "hold the GIL", which is
   slow but never wrong* — generalised to all three bits.
6. Where the resolver knows the complete set of implementations of an interface
   in the program, the dynamic call contributes the union of those, not the full
   set. This is an optimisation, not a soundness requirement, and it is the
   difference between "every program that uses `any Error` holds the GIL" and a
   usable release policy.

Steps 2 and 3 are the same traversal the region solver already performs. **They
should share it**, and §10 asks for that explicitly.

### 3.6 Provenance is not optional, for the same reason it was not for regions

Core spec §6.2 says of regions:

> With regions inferred rather than annotated, the compiler has nothing of the
> user's to blame; an error must reconstruct where the borrow was born, what
> keeps it alive, and where it conflicts. […] A solver written without provenance
> must be rewritten entirely to gain it.

Word for word, that applies here, and for a sharper reason: an inferred effect
error has *no user-written thing at all* at the fault site. The offending call is
four functions away in a file the user did not open.

> **Decision 4. Every bit carries a witness: the shortest call chain from the
> function under test to the seeding primitive, with a span per hop. A diagnostic
> that says only "this function carries the `python` effect" is a bug, not a
> diagnostic.**

The rendered form, which is what `llm-ergonomics.md` says is the only teaching
channel this language has:

```
error[SC0214]: `log_likelihood` is declared `pure` but carries the `ambient` effect
  --> model/fit.science:12:1
   |
12 | pure def log_likelihood(data: borrowed Array of F64, mu: F64) -> F64:
   | ^^^^ declared pure here
   |
note: the effect enters through this call chain
   |
17 |     let w be weighted_sum(data, mu)
   |              ------------- here
   |
  ::: model/weights.science:34:12
   |
34 |     current_weights(self.window)
   |     --------------- and here
   |
  ::: model/cache.science:8:18
   |
 8 |     let now be time.now()
   |                --------- `time.now` reads the machine's clock
   |
   = `ambient` means the function reads state the machine supplies rather than
     state its arguments carry: the clock, the OS entropy source, the
     environment, or the scheduler.
help: take the timestamp as a parameter, or remove `pure` from the declaration
```

The cost of Decision 4 is memory and query-invalidation granularity: the witness
must be stored, and it must invalidate when any hop's body changes. That is
already true of the effect set itself, so the incremental cost is the chain, not
the dependency.

---

## 4. Decision: inferred, with exactly one declaration

This is the central question, and the honest starting position is that both
answers are bad.

- **Declared on every signature** is noise in a language whose audience writes
  twenty-line functions, and whose §4.1 spent its whole argument on readability.
  The overwhelming majority of Science functions have an empty effect set; making
  each of them say so is a tax levied on the common case to inform the rare one.
- **Inferred everywhere and invisible** means a signature does not say what the
  function does, which is the thing effects exist to communicate. Worse, it means
  a library can add a Python call in a patch release and silently break a
  downstream `.parallel()` that compiled yesterday.

### 4.1 The decision

> **Decision 5. Effect sets are inferred everywhere and written nowhere, except
> that a function may be declared `pure def`, which is a compiler-checked
> assertion that its inferred set is empty. There is no syntax for declaring a
> non-empty set anywhere in Science source. `extern` is the exception and §7 owns
> it.**

```science
pure def log_likelihood(
        data: borrowed Array of F64,
        mu: F64,
        sigma: F64) -> F64:
    let mutable total be 0.0
    for x in data:
        let z be (x - mu) / sigma
        total be total - 0.5 * z * z
    total - (data.length() as F64) * sigma.log()
```

The shape is `unsafe def` inverted, and the symmetry is the argument:

| | `unsafe def` | `pure def` |
|---|---|---|
| Says | "I claim extra powers" | "I claim no effects" |
| Checked by | Nobody — it is the author's claim | The compiler, against the inferred set |
| Obliges | The **caller**, who must write `unsafe:` | Nobody. Callers are unaffected |
| Purpose | Delimit what a reviewer must read | Pin a contract so it cannot rot |
| Cost if wrong | Undefined behaviour | `SC0214` at the declaration |

`ffi-c-boundary.md` §3.3 is explicit that *"`unsafe` is not part of a function's
type and does not propagate."* `pure` is not part of a function's type and does
not propagate either. Both are properties of a definition, recorded at the
definition. One vocabulary, one mechanism, two directions.

### 4.2 Why this over "inferred within a crate, declared at published boundaries"

That middle is the obvious design and it was the strongest alternative. It is
rejected, with the reasons stated because the rejection is close.

**What it gets right.** It is exactly the shape of §6.3's auto-borrow — *the
signature still says `borrowed`, so a reader keeps the information; only the
noise at the call site goes* — and the analogy is fair. A published signature
genuinely is part of a contract in a way an internal one is not.

**Why it is rejected anyway, in three parts.**

1. **The declaration would have to be written by a human and it would be
   generated by a tool within a week.** Every language that has required a
   mechanically derivable annotation at a module boundary — OCaml's `.mli`,
   Haskell's export lists, Rust's `#[must_use]` audits — grows a tool that writes
   it, and then the annotation is not information, it is a checksum. A checksum
   belongs in a file the compiler writes, not in the source a person reads.
2. **"Published boundary" does not exist in F0.** `package-manager.md` is F0
   staged, and until it lands there is no boundary to declare at. A rule that
   cannot be enforced in the phase it is specified in will be specified wrong.
3. **The real requirement is change detection, not declaration.** The hazard is
   not "a library's effects are unknown"; the compiler can always compute them.
   The hazard is "a library's effects *changed* and nobody noticed". A
   declaration catches that only if a human remembers to look; a recorded
   interface catches it always.

So the boundary problem is solved by recording rather than by declaring:

> **Decision 6. `sciencec` emits the inferred effect set of every `public`
> function into the package's generated interface record, alongside the types.
> A build that changes a `public` function's effect set against the recorded
> interface is `SC0216`, a warning, naming the function and the witness chain.**

This is `cargo-semver-checks` for effects, and it is the correct precedent:
Rust's `Send` and `Sync` are inferred auto-traits whose accidental removal is a
breaking change, and the ecosystem catches it with a tool over a recorded
interface rather than with an annotation on every type.

The cost of Decision 6 is stated plainly: **reading the source of a library
function does not tell you its effects.** You need `sciencec doc`, an LSP hover,
or the interface record. For a language whose §4.1 is about readability that is a
real concession, and it is bought with the observation from §2.2 that the
signature already tells you the six things that matter most, and that the seventh
is best rendered by a tool that cannot be wrong.

### 4.3 Why `pure` exists at all, given inference

Because a library author sometimes wants the contract *locked*, and Decision 6's
warning is a warning. `pure def` turns "this changed" into "this does not
compile", at the author's discretion, on the functions where it matters:

- a statistical distribution's density,
- a special function in `math`,
- a kernel a user will pass to `grad` in F2,
- anything a caller will put under `.parallel()`.

`scientific-libraries.md` §2 commits to writing special functions, distributions,
optimisers and integrators **in Science** partly so they are generic and
differentiable. `pure` is the annotation that makes that commitment checkable:
a `pure def erf(x: F64) -> F64` cannot quietly acquire a call to a logging
helper three releases later.

**Rejected: `pure` as the default, with an `impure` marker.** This is the Haskell
shape, and it is wrong here for the same reason declaring everywhere is wrong:
it puts the annotation on the code that does the work, and scientific code does
I/O constantly. It also makes `main` un-writable without a marker, which is a
bad first page of a tutorial.

**Rejected: allowing `pure` to be written with a non-empty set,
`def f(...) does external:`.** It needs a grammar for effect sets, and once
there is a grammar for effect sets somebody will want them in bounds, and then
Decision 3 is gone. The absence of that syntax is a load-bearing absence.

**The cost, stated.** You cannot write "this function may read files but must
never touch Python." The only expressible assertion is "no effects at all". For
the closure case that matters — a `.parallel()` body — the check happens at the
call site with a better error anyway; for the library case, a partial assertion
would be genuinely useful and Science does not have it. This is the largest thing
this design gives up, and it is listed again in §12.

---

## 5. What "pure" means here, and what it actually buys

Purity in a language with ownership and in-place mutation of owned values is not
purity in Haskell, and stating the difference imprecisely is how a design like
this produces a wrong answer later.

### 5.1 The definition

> **A function is `pure` iff its inferred effect set is empty.**

Unpacked, that means: **a `pure` function's result and its writes are a function
of the values reachable from its arguments.** Specifically it may:

- **mutate through a `mutable borrowed` parameter**, in place, as much as it
  likes. Rule 4 guarantees nothing else observes the referent during the call, so
  the mutation is a state transition from arguments to arguments, not an effect
  on the world. Haskell would need `ST` and a rank-2 type to say this; Science
  says it with a borrow, and that is core spec §6.5's Futhark argument
  (*"uniqueness types exist solely to let a pure language write `x[i] = v` in
  place safely; ownership is the same guarantee with a better surface"*) arriving
  where it was always going.
- **consume a `Key` and draw from it.** The key is an argument. `normal(key,
  0.0, 1.0)` is a deterministic function of `key`, which is exactly why
  `stdlib-standard.md` §5.5 freezes the algorithm and `split`'s derivation
  forever.
- **draw from a `mutable borrowed Stream`.** Also an argument, also
  deterministic given the stream's state — but see §5.2, because this one is pure
  and still not reorderable.
- **panic.** `panic` aborts (core spec §8); it produces no value and the
  abstraction it breaks is the program, not the function's mathematics. Treating
  `panic` as an effect would make every function that indexes an `Array` impure,
  which would leave the word meaning nothing.
- **allocate**, including on a device. §6.1.
- **overflow, or take an `unordered` reduction.** Both are defined-by-the-spec
  behaviours, not ambient reads.

It may not read the clock, the environment, the entropy source, a file, a socket,
standard input, or Python.

### 5.2 Purity is necessary and not sufficient, and the difference matters

The folklore is that purity buys memoisation, reordering, parallelism and
differentiation. In Science, exactly one of those is bought by purity alone.

| Claim | What the compiler must prove | Purity's contribution |
|---|---|---|
| **Evaluate at compile time** (const evaluation, and the const-expression arithmetic three notes ask for) | Empty effect set, **and** no `mutable borrowed` parameter, **and** all arguments literal or const | Necessary, not sufficient |
| **Memoise a call** (F1's interactive tier caching a cell's result) | Empty effect set, **and** no `mutable borrowed` parameter or capture, **and** every parameter `Eq + Hash` | Necessary, not sufficient |
| **Reorder or eliminate a call** | The same conjunction as memoisation | Necessary, not sufficient |
| **Run two calls concurrently without a data race** | **Nothing from purity.** Rule 4 already forbids two live exclusive borrows of one place, and `Share` already governs what crosses a thread | **Nothing** |
| **Differentiate** (F2's `grad`) | Every operation in the body has a registered derivative, with respect to a named argument | **Nothing.** §6.2 |

Two lines in that table deserve to be said out loud.

**The `mutable borrowed` exclusion is why `pure` is not enough for memoisation.**
`def draw(s: mutable borrowed Stream) -> F64` has an empty effect set and is
`pure` by §5.1. It is also order-dependent: swap two calls and both results
change. A memoiser that keyed on "the arguments" would key on a borrow, which is
an address, and produce nonsense. So the compiler's internal *memoisable*
property is `pure ∧ no mutable borrowed ∧ arguments hashable`, and it is derived,
not declared. Nobody writes it.

**Purity contributes nothing to data-race freedom, and this is the single most
important honest statement in the note.** In Haskell, purity is what makes
`par` safe. In Science, **ownership already did that**, three phases before the
effect query exists. Rule 4 makes two exclusive borrows of one place a compile
error; `collections-and-chains.md` §7.6 already requires `.parallel()` to reject
a closure capturing anything `mutable borrowed`; `Share` already governs what
may move across a thread. A `.parallel()` closure that prints is a data race in
no sense at all — its output interleaves, which is annoying and is not
unsoundness.

Which is why:

> **Decision 7. `.parallel()` rejects a closure carrying `python`, and no other
> effect bit is grounds for rejecting a parallel region.**

`external` in a parallel closure — a `print`, a `logging.info`, a
progress counter written to a file — is permitted, and the documentation says
that output order is unspecified. Rejecting it would make debugging a parallel
loop impossible, which is the behaviour that makes people hate effect systems,
and it would buy nothing, because the safety was already bought.

`collections-and-chains.md` §7.7's *"no combinator has an observable effect"* is
untouched by this: that is a rule about the **closed combinator set**, which is
why `for_each` and `inspect` do not exist. It was never a rule about user
closures, and Decision 7 does not weaken it.

### 5.3 What the query database actually memoises

`python-interop.md` and the brief both reach for "a pure function can be
memoised by the query database". That is worth correcting, because the correction
is the concrete payoff.

**`salsa` memoises compilation, not execution.** Core spec §7.3's queries are
`parse(file)`, `types(def)`, `regions(def)`. No user function call is a salsa
query and none will be. So purity does not make a program's calls cached at
runtime by virtue of §3 of the core spec.

What purity actually buys against the query architecture is two specific things:

1. **Compile-time evaluation.** A `pure` function with const arguments may be
   executed by the compiler, and *that* execution is a query and is memoised.
   This is the mechanism the standing cross-note ask for **const-expression
   arithmetic in type position** (`scientific-libraries.md` §12.4,
   `broadcasting.md` §11, `unit-literals.md` — the largest ask on the board) will
   need the moment it grows past addition: a shape computation written as a
   Science function rather than a built-in requires the compiler to know the
   function is safe to run at compile time, and `pure` is that permission.
2. **F1's interactive tier.** Core spec §7.3: *"re-executing a notebook cell is
   invalidating a query."* A cell whose computation is memoisable by §5.2's
   conjunction can have its **result** cached across re-executions, not merely
   its compilation. That is the difference between a notebook that recomputes a
   forty-minute fit when you edit a plot label and one that does not, and it is
   the most user-visible thing in this note.

Both are real, both are F1, and neither is "the query database memoises your
pure functions".

---

## 6. What is deliberately not an effect

### 6.1 Device allocation

> **Decision 8. Device residence is in the type. There is no `device` effect.**

`models-and-inference.md` §4.2 already puts the device in the tensor's type, and
core spec §6.5 makes ownership-tracked device memory one of §1's five claims. The
question "does this function allocate on a device" is answered by its return
type, and the question that actually matters — **which** device — is answered
only by the type.

A bit could say "some device". It could not say `cuda:1`, and you cannot free a
`cuda:1` buffer from a `cuda:0` context. An effect that cannot answer the
question the user is asking is worse than no effect, because it looks like an
answer.

**Rejected: a parameterised `device(d)` effect.** That is an effect with an
argument, which is an effect row with a label payload, which is Koka. It would
also duplicate a type-level fact, violating Decision 1.

### 6.2 Differentiability

> **Decision 9. Differentiability is checked at the transformation site, over the
> body, with respect to a named argument. It is not an effect and it is not a
> property of a signature.**

Three reasons, and they are independent.

1. **It is not reachability.** A function can call only differentiable functions
   and still not be differentiable: it branches on a value derived from the input,
   it indexes with a computed integer, it calls `floor`, it accumulates into an
   integer. Effects propagate monotonically up the call graph; differentiability
   does not, because the *composition* can fail where every part succeeds.
2. **It is relative to an argument.** `grad(f, of: 0)` differentiates with
   respect to the first parameter. `def loss(params: borrowed Array of F64,
   labels: borrowed Array of I64) -> F64` is differentiable in `params` and
   meaningless in `labels`. A bit has no argument position; a check at the
   transformation site does.
3. **JAX proved the transformation-site check is the right shape, and proved the
   alternative fails.** JAX traces at `grad`, discovers the non-differentiable
   operation, and names it. What JAX does *not* do well is side effects — a
   `print` inside a `jit` prints once, at trace time, which is the single most
   confusing thing about the library. So the honest reading of JAX is: the
   transformation-site check for differentiability is right, and the silent
   handling of effects is wrong. Science should take the first and fix the
   second, which is exactly what Decisions 2 and 9 do together.

**Rejected: a `differentiable` marker on signatures.** It would be wrong in both
directions — functions marked differentiable that are not, at particular
arguments; functions unmarked that are.

**What F0 must reserve for this is in §10**, and it is not an effect-system
requirement: it is that the body of a monomorphised function, and of a closure,
must still be reachable from the call site that wants to transform it.

### 6.3 Thread-safety of a foreign library

> **Decision 10. Whether a C library may be called from two threads is a
> declaration on the `extern` block, owned by `ffi-c-boundary.md`, and it is not
> an effect and not implied by `pure`.**

`stdlib-shape-and-packages.md` §3.4 and `stdlib-standard.md` §16 both ask for
`library "hdf5" single threaded` — *"one request with two customers and it should
be built once."* This note is a third voice for it and adds nothing to the
design, but it must say clearly that **purity does not subsume it**, because the
merge is tempting and wrong: FFTW's plan creation is a deterministic function of
its arguments and is not thread-safe. Two axes, two declarations.

---

## 7. The FFI boundary

`ffi-c-boundary.md` §3 defines `unsafe` as a closed list of six extra powers.
That is already an effect system in miniature, and its structure is the precedent
this section builds on:

> a declaration records a claim, the compiler verifies the half of the claim that
> lives in Science, `unsafe` marks the half that does not, and a hand-written
> safe wrapper is the place where the unverifiable half is discharged by an
> argument a human wrote down.

A foreign function's effects are the unverifiable half, in full.

### 7.1 The default

> **Decision 11. An `extern` function carries `{ambient, external}` by default,
> and `{python, ambient, external}` if the block is the Python runtime's. Silence
> means "assume everything".**

Conservative by default is the only defensible choice, and it is the one
`python-interop.md` §5.2 already made for the `python` bit — *conservative means
"hold the GIL", which is slow but never wrong.*

This has a consequence worth naming: **essentially every scientific Science
program is `external` end to end at first**, because `linalg` wraps BLAS. That is
correct and not useless, because the bits the program cares about — `python` for
the GIL, `ambient` for reproducibility — remain clear, and §7.2 fixes the rest.

### 7.2 The narrowing

> **Decision 12. An `extern` function declaration may be prefixed `pure`, which
> declares its effect set empty. The declaration is unchecked. The author of the
> `extern` block is responsible, and `unsafe` on the block is already where that
> responsibility lives.**

```science
unsafe extern "C" library "openblas" via pkg-config "openblas":

    pure def cblas_dgemm(
        layout: CblasLayout,
        transpose_a: CblasTranspose,
        transpose_b: CblasTranspose,
        m: BlasInt, n: BlasInt, k: BlasInt,
        alpha: F64,
        a: ffi.Span of F64, lda: BlasInt,
        b: ffi.Span of F64, ldb: BlasInt,
        beta: F64,
        c: ffi.MutableSpan of F64, ldc: BlasInt,
    )

    def openblas_set_num_threads(n: CInt)
```

`cblas_dgemm` writes through `c`, which is a `mutable borrowed` by another name,
and §5.1 says that is compatible with purity. `openblas_set_num_threads` mutates
library-global state and is left at the default, which is right.

**The responsibility argument is the whole of why this is cheap.**
`ffi-c-boundary.md` §1.1 already says:

> An `extern` declaration is a claim about code the compiler will never see: that
> a symbol of this name exists, that it takes these arguments in this order with
> these widths, that it does not retain the pointers it is given, that it does
> not free them. None of that is checkable. Writing the declaration is therefore
> itself the unsafe act, and the keyword belongs where the act is.

"and that it reads nothing but its arguments" is a fifth clause on a list of
four unverifiable claims. It adds **no new conceptual burden**, it needs no new
keyword, and the reviewer who is already reading the block against the C
documentation is already the right reviewer for it. That is the strongest
argument in this note for doing something rather than nothing.

### 7.3 What a wrapper can and cannot do

A safe Science wrapper's effect set is **inferred from its body**, like every
other Science function. It therefore contains its callee's set, and a wrapper can
never be narrower than what it wraps. There is no `unsafe: assume_pure`.

> **Decision 13. Effects narrow at the `extern` declaration and nowhere else. A
> wrapper cannot assert an effect set its body does not support.**

**Rejected: an `unsafe` purity assertion available in any function body.** It
would be used, it would be used wrongly, and unlike the six powers in
§3.1 its falsity is silent: a wrongly-pure function produces a wrong GIL release
or a wrong memoisation, both of which look like flaky behaviour rather than a
crash. The `extern` block is the right and only place, because it is the one
place a reviewer is already obliged to check a claim against a document.

There is one thing a wrapper genuinely can do, and it is not narrowing: it can
**supply** an ambient input so that its own callers do not have to. A wrapper
that takes a seed as a parameter and passes it to a C generator is `external` but
not `ambient`, because the entropy came from its caller. That composition falls
out of the inference with nothing added, and it is the shape
`scientific-libraries.md` §2 wants for every wrapped library.

### 7.4 Callbacks

`ffi-c-boundary.md` §4.4 names reentrancy as the hole. Effects make one corner of
it visible: a `pure` `extern` function that takes an `ffi.Callback` parameter is
not decidable from its own declaration, because the callback's effects are part
of the call's effects, and the callback is Science code the compiler can see.

> **Decision 14. `pure` on an `extern` function with an `ffi.Callback` parameter
> is `SC0215`. The call's effect set is the declared set of the foreign function
> unioned with the effect set of the callback actually passed, computed at the
> call site.**

The second sentence is the useful half: it means a SUNDIALS right-hand side
written in Science that touches Python makes the `CVode` call carry `python`, so
the GIL policy is right without anyone declaring anything.

---

## 8. Diagnostics allocated

This note's block, per `README.md`'s allocation map, is **`SC0214`–`SC0229`**, in
the resolution range of core spec §9.

**A seam, named rather than hidden.** Effect inference runs after resolution and
after type checking, because step 5 of §3.5 needs to know what an
`any Interface` call can reach. The codes therefore sit in the *resolution* range
for allocation hygiene — it is the only contiguous block of sixteen left — while
the pass is closer to types. §11 asks for one of two fixes.

| Code | Phase | Meaning |
|---|---|---|
| `SC0214` | Effects | A function declared `pure def` has a non-empty inferred effect set. Names the bit, and renders the witness chain of §3.6. Fix: remove `pure`, or take the ambient input as a parameter. |
| `SC0215` | Effects | `pure` on an `extern` function that takes an `ffi.Callback`. §7.4. Fix: remove `pure`; the call site computes the union. |
| `SC0216` | Effects | **Warning.** A `public` function's inferred effect set differs from the one recorded in the package's interface. Names the function, the bit gained or lost, and the witness. §4.2. |
| `SC0217` | Effects | `ambient` is reachable from `main` in a build declared reproducible. Names every distinct witness chain, not the first. F1+; see §11. |
| `SC0218` | Effects | A call required to be evaluated at compile time is not memoisable — non-empty effect set, or a `mutable borrowed` parameter, or a parameter type without `Eq + Hash`. Names which of the three, and the first offending call. |
| `SC0219` | Effects | A `pure` function's set could not be shown empty because it calls through `any Interface` or a function value whose targets are unknown. Distinct from `SC0214` because the fix is different: make the call static, or seal the interface. |
| `SC0220`–`SC0229` | — | Reserved: F2's `.parallel()` and `grad` checks, F5's replay checks. |

**Codes this note does not claim and endorses where they are:**

- **`SC0454`** (`python-interop.md`) — Python-effecting code in a parallel region.
  It stays in the codegen range and stays that note's. This note supplies its
  input and changes nothing about it. Decision 7 confirms it is the *only* effect
  bit that rejects a parallel region.
- **`SC0270`** (`stdlib-standard.md`) — a `public` function taking
  `mutable borrowed random.Stream`, at warning severity. It is the prototype for
  `SC0216`: an inferred property, warned about at the published boundary,
  obtained from a signature rather than an annotation. It is cited here as
  precedent, not amended.
- **`SC0279`** (`stdlib-shape-and-packages.md`) — a parallel reduction over a
  non-associative combiner. Associativity is an algebraic property of an
  operator, not an effect, and it belongs where it is.

---

## 9. The competitive claim, tested

§1 says no other language has the combination. This section checks the effect
third of that honestly, because a claim the language cannot back is worse than a
narrower one it can.

### 9.1 Against each

**Koka, Eff, Frank.** These have real effect systems: row-polymorphic effect
types, handlers, `resume`, effect inference that is principal. Koka can express
"this function reads state `s`, may raise `e`, and does nothing else", and can
abstract over the row. **Science has strictly less.** Not a different trade —
less. Anyone who reads "an effect discipline" as "algebraic effects" will open
Science and find three bits and a `pure` keyword, and will be right to be
disappointed. What Science has that Koka does not is everything *outside* the
effect system: ownership, no ambient state, static shapes.

**Haskell.** `IO` is an older, stronger and better-understood discipline than
anything proposed here, with the additional virtue that it is enforced by the
type system rather than by a side table. `ST` and `runST` give exactly §5.1's
"mutate in place inside something pure", and give it with a proof, via rank-2
types, where Science gives it with a borrow. Haskell's weaknesses are elsewhere:
`unsafePerformIO` is a hole of the same kind Decision 13 refuses to open, and
lazy evaluation makes "when did this run" a question the effect system cannot
answer — which for reproducibility is a real defect Science does not have. But as
an *effect system*, Haskell's is better.

**Rust.** `unsafe`, `Send`, `Sync`, `&mut`, and `const fn` are, taken together,
most of what Science is proposing. `Send`/`Sync` are inferred auto-traits, which
is the same mechanism as Decision 5, and the ecosystem catches accidental changes
with a tool over a recorded interface, which is the same mechanism as Decision 6.
`const fn` is Rust's `pure`, and it is **declared** and **checked** — which makes
it stronger than what Science proposes in one respect, since a `const fn` is
enforced constructively. What Rust does not have is `python`, `ambient`, or any
reproducibility story at all. Science's genuine addition over Rust here is
narrow: two bits and a keyword.

**Mojo.** `@parameter` is compile-time evaluation, not an effect discipline; the
closest thing Mojo has to an effect is `raises`, a one-bit declared effect
inherited from Python's exception model, which Science does not need because
`panic` aborts and failure is a return value. Mojo's ownership model is Science's
model's parent (core spec §3 adopts its auto-borrow over Rust's explicit `&`).
Mojo has less here than Rust does.

**JAX.** The most instructive comparison, because JAX's audience *is* Science's
audience and JAX's discipline is the one they already have. JAX enforces
functional purity at trace time, by construction: a traced function that does I/O
does it once, at trace time, silently and wrongly. Reused PRNG keys are a silent,
undetectable bug — core spec §6.5 names exactly this. **This is the one
comparison Science wins outright**, and it wins it with ownership (`SC0301` on a
reused key) plus one static bit, not with an effect system.

### 9.2 The verdict

**§1's phrase is the weakest of its five claims, and it should be narrowed.**

Taken as written — "an effect discipline" beside "static shapes" and
"ownership-tracked device memory" — it invites a comparison Science loses to four
languages. Taken as what this note actually designs, it is true but small: three
inferred bits and a checked `pure`.

What is genuinely Science's, and what no other language has, is not the effect
system. It is **the conjunction**:

> A language with no ambient state, where the generator is a consumed value, the
> mutation is a borrow, the reduction order is in the specification, the device
> is in the type, and a static bit says whether the machine was consulted.

No other language has all of those. Koka has an effect system and a garbage
collector and global variables. Haskell has `IO` and an evaluation order nobody
can predict. Rust has the ownership half and no reproducibility story. JAX has
the transformation model and no static check at all. Julia has none of it and
`Random.seed!`. The claim Science can back is not "an effect discipline"; it is
**"reproducibility the compiler checks"**, and that is a better claim anyway,
because it is the one the audience actually wants and the one they can test.

> **Ask of the core spec (§11).** Replace *"an effect discipline"* in §1 with
> *"compiler-checked reproducibility"*, or if a mechanism must be named,
> *"inferred effects"* — never *"an effect system"*. The surrounding sentence
> then reads:
>
> > None of them has all of static shapes, compiler-checked reproducibility,
> > ownership-tracked device memory, an interactive tier, and zero-copy Python
> > interop.
>
> That sentence is true, it survives contact with Koka, and it names the thing
> the other four claims are also in service of.

---

## 10. Phasing: what F0 must not preclude

Core spec §2 lists three constraints from later phases. This note adds to that
list in the same manner, and every item below is something the type checker —
which is about to be written — has to reserve room for.

**1. Effects must be a salsa query over the call graph, built in F0 even though
nothing in F0 reads it.** The bits themselves can wait. What cannot wait is that
`effects(DefId)` is a *query*, with the call graph as a query beneath it. Bolting
a whole-program pass onto a query compiler afterwards is precisely the thing
core spec §7.3 exists to prevent, and it is the same shape as §6.2's warning
about provenance. **Cost in F0: the call graph becomes a named query rather than
a traversal inside monomorphisation. Small. Retrofit cost: large.**

**2. It should share the region solver's SCC machinery.** §6.2 already does
reverse-topological traversal with a fixed point per strongly connected
component, and §3.5 needs the identical traversal. Writing it twice is how the
two disagree about what a call is.

**3. Provenance from the first line, per Decision 4.** The witness chain is not
an enhancement; a solver written without it must be rewritten to gain it, which
§6.2 already says about the other inferred thing in this compiler.

**4. Effects must stay out of unification, per Decision 3.** This is the negative
constraint and it is the one most easily lost, because the first person to add
`(T) -> U` as a closure type — which three notes ask for — will be
tempted to put an effect slot in it "for later". There must be no slot. If a
later phase wants effect polymorphism, it should be a reopened decision with its
own note, not a field that was left empty and then filled in.

**5. Closure bodies must remain reachable from their use sites.**
`collections-and-chains.md` §7.6 already requires that a closure's capture set be
in its type from F0, and observes that *"nothing in F0 reads that information"*.
Effects need the same thing for a different reason: a closure's effect set is its
body's, and F2's `grad` needs the body itself. One requirement, three customers.

**6. Dynamic dispatch must record what it can reach.** At every `any Interface`
call site the resolver should record the known implementation set where the set
is closed. F0 need not use it; F2's GIL policy and `.parallel()` will be
unusable without it, because "every program using `any Error` holds the GIL" is
not a shippable release policy.

**7. `pure` moves to the in-use reserved list.** One word.

**8. `extern` blocks need one contextual modifier position.**
`ffi-c-boundary.md` owns the grammar; this note asks only that `pure def` be
accepted as an item form inside the block.

### 10.1 F5, which is the strongest argument for building the `ambient` bit

Core spec §2 schedules durability for F5: *"agent bodies lowered to serializable
state machines"*, with §6.4's `borrows_live_at(point)` as the mechanism that
keeps captured state serializable.

A durable agent that resumes on another machine must decide, for every call it
made before the checkpoint, whether to **replay** it or to **restore a recorded
result**. Replaying `average(xs)` is correct and cheap. Replaying `time.now()` is
wrong — it returns a different answer and the agent's history becomes
inconsistent. Replaying `read_file(path)` is correct if the file has not changed
and is the pragmatic default. Replaying a Python call in a process that no longer
has an interpreter is a crash.

That is a three-way decision, and **the three bits of §3.1 are exactly its
inputs**:

| Bit | Replay policy at resume |
|---|---|
| empty | Replay. Cheaper than recording. |
| `external` | Replay by default; recordable if the durability level asks. |
| `ambient` | **Must be recorded at checkpoint and restored at resume.** Replay is incorrect. |
| `python` | Neither; the boundary must be re-established before resume, or the agent cannot resume in this process. |

F5 would otherwise have to record *everything*, which makes every checkpoint the
size of the program's whole I/O history. So the `ambient` bit — the one with the
weakest F0 and F1 justification — has a concrete, expensive F5 customer, and that
is the reason to define it now rather than to define one bit and split it later.
Splitting a bit after a package ecosystem has recorded interfaces against it
(Decision 6) is a breaking change to every recorded interface.

### 10.2 F2, in one line each

- **`.parallel()`** consumes the `python` bit and nothing else (Decision 7); the
  rest of its check is ownership, which `collections-and-chains.md` §7 already
  specified.
- **`grad`** consumes no bit (Decision 9); it needs reachable bodies (§10, item 5).
- **GPU** consumes no bit (Decision 8); it needs device in the type, which
  `models-and-inference.md` §4.2 already has.

---

## 11. What this note asks of others

1. **Of the core spec, §1**: narrow *"an effect discipline"* to
   *"compiler-checked reproducibility"*. §9.2 gives the replacement sentence and
   the reasoning. This is the only place this note asks for the spec's claim to
   change, and it asks for it to be made *smaller*.
2. **Of the core spec, §9 and of `README.md`**: the diagnostic range table has six
   phases and effect checking is a seventh. Either (a) extend the *Types and
   interfaces* description to "types, interfaces and effects" and re-home
   `SC0214`–`SC0219` into a types block when one frees up, or (b) leave them in
   the resolution range and record in §9 that the range is "resolution and
   whole-program analysis". This note prefers (a) and will take (b). Until then
   the codes above are allocated where `README.md`'s free list allows.
   **`README.md`'s notes index needs a row for this note**, which this note
   cannot add without editing a sibling's file.
3. **Of `README.md`'s standing cross-note asks**: add *"effect sets recorded in
   the package interface"* (§4.2, Decision 6) as a customer of whatever
   `package-manager.md` produces as a published-interface artefact. It is the
   second customer after types, which is what makes it worth building.
4. **Of `ffi-c-boundary.md`**: accept `pure def` as a fourth item form
   inside an `extern` block (§7.2), and record §7.4's callback union rule beside
   §4.4's reentrancy hole, which it belongs to. This note claims `SC0215` for the
   `pure`-plus-callback case; if that note would rather own it in the `SC0410`
   range, it should, and this note will amend.
5. **Of `python-interop.md`**: §5.2's fallback — *"if the general effect system
   is not ready when F2 ships, the same bit is recoverable from the
   post-monomorphization call graph"* — can be retired. §3.5 is that analysis,
   named, queried and shared with two other consumers. §5.2 should point here for
   the mechanism and keep `SC0454` and the GIL policy, which are unchanged. §6.3
   of that note also still needs its `Result`/`try` rewrite per
   `syntax-revision-2.md` §9.4, which is not this note's business but is the
   section this note quoted.
6. **Of `collections-and-chains.md`**: §7's seven properties are confirmed and
   one is sharpened. Property 7 — *"no combinator has an observable effect"* —
   should say explicitly that it constrains the **combinator set**, not user
   closures, because Decision 7 permits a `print` inside a `.parallel().map()`
   body and a reader of §7.7 alone would conclude otherwise. That is a
   clarification of that note's intent, not a disagreement with it.
7. **Of `stdlib-standard.md`**: §5.3's observation is adopted as this note's §2
   and is the reason the note is short. Two small requests: that the primitives
   listed in §3.1 above be confirmed as the complete seeding set for `time`,
   `os`, `random`, `logging`, `net` and `crypto`; and that `Stream.from_entropy`
   be documented as the module's one `ambient` constructor, which strengthens the
   §5.3 argument it already makes about the name saying so at the call site.
8. **Of `stdlib-shape-and-packages.md` and `stdlib-standard.md` jointly**: this
   note is a third voice for `library "…" single threaded` (§6.3). It is now one
   request with three customers.
9. **Of whichever note owns a reproducible-run mode** — `package-manager.md` by
   its reproducibility tiers, or `data-io.md` by §546's ordering argument:
   `SC0217` is offered and is not claimed as policy. This note supplies the bit;
   somebody else must decide whether a reproducible build *forbids* `ambient` or
   merely *reports* it, and that is a packaging decision, not a language one.

---

## 12. Risks

**The honest verdict in §9 is a claim the project has been making for a day and
this note retracts half of.** That is the point of writing it down, and it is
still a retraction. If §1 is not amended, the gap between what the spec promises
and what this design delivers is a gap a reviewer will find, and finding it
themselves is much worse than reading it here.

**Three bits may be two too many or one too few, and there is no way to know
yet.** The closed-set rule (§3.1) is what makes the mistake survivable in one
direction — an unneeded bit is dead weight, not a design flaw. It is *not*
survivable in the other: adding a fourth bit after Decision 6 has recorded
interfaces against three is a breaking change to every published package. §10.1
is the argument that `ambient` and `external` must be separate *now*; if that
argument is wrong, the cost lands in F5.

**Inference with no declaration means a library's effects can change in a patch
release.** `SC0216` is a warning over a recorded interface, and a warning is a
warning. The failure mode is concrete: a downstream user's `.parallel()` stops
compiling after `sciencec update`, with an error pointing into a dependency. The
diagnostic must therefore name the *dependency version that introduced the bit*,
not merely the call chain, and that is a requirement on whatever produces the
interface record.

**The witness chain is the whole usability of this feature and it is the part
most likely to be cut.** An effect error with no chain is unactionable, because
the fault is in a file the user did not write. §6.2's warning about regions
applies verbatim and should be taken more seriously here, because a region error
at least has a borrow in the user's own function to point at.

**`pure` will be asked to mean more than it does.** Users will read it as
"thread-safe" (§5.2 says ownership already did that), as "differentiable"
(§6.2 says no), as "memoised" (§5.2 says necessary-not-sufficient), and as "fast".
The documentation must lead with the definition in §5.1 and with the table in
§5.2, in that order, and the diagnostic for `SC0214` must explain the bit rather
than merely name it.

**Conservative dynamic dispatch may make the GIL policy useless in practice.**
§3.5 step 5 assumes the full set at every `any Interface` call, and `Error?` is
`(any Error)?` in the signature of every fallible function in the language
(core spec §5.5). If a fallible function is therefore `python`-effecting, the
shim never releases the GIL and `python-interop.md` §5.2's default is dead. The
mitigation is step 6 — the known-implementation-set optimisation — and it is
therefore not an optimisation but a requirement. This is the risk most likely to
be discovered late, and it should be tested on the first program that returns an
error from an exported function.

**Compile-time evaluation of `pure` functions (§5.3) is a second interpreter.**
The const-expression ask is already the largest on the board, and §5.3 quietly
makes `pure` the permission slip for it. Whoever builds that must not conclude
from this note that the hard part is decided; the hard part is the evaluator, and
this note supplies only the predicate that says which calls it may run.
