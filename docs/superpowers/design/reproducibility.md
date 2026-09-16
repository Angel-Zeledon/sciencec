# Science — Design: reproducibility as a language property

Date: 2026-09-16
Status: draft for review
Owns: the reproducibility claim itself — what may be promised, what may not, the
`--deterministic` build profile, the provenance record's format and lifecycle,
and the audit of every channel through which a Science program can produce a
different answer twice.
Assembles, and does not re-specify: `stdlib-standard.md` §5 (`random`),
`native-dependencies.md` §5 (tiers, `.science.provenance`), `package-manager.md`
§2 (the lockfile, `[build]`, no build scripts), `scientific-libraries.md` §2 (the
generator is written in Science), `collections-and-chains.md` §5.2 and §7.1
(insertion order, pinned reduction shape), `stdlib-shape-and-packages.md` §5.7
(the three deliberate criterion violations), `stdlib-core.md` §8.3 (the 1-ULP
contract), **`effects.md` §3 (the `ambient` bit) and §11 ask 9, which hands this
note the policy that note deliberately declines to set**.
Written in the syntax of `syntax-revision-2.md`.
Claims `SC0237`–`SC0240` (resolution). Adopts `effects.md`'s `SC0217`.

> **`effects.md` landed while this note was being drafted, and it changes §4.**
> An earlier draft of §4 designed a reachability analysis and claimed
> `SC0214`–`SC0221`. That note has now built the analysis properly, as an
> inferred `ambient` bit with witness chains, and claims `SC0214`–`SC0229`. The
> claim is withdrawn, the analysis is adopted wholesale, and §4 is reduced to the
> **policy over it** — which is exactly what that note's §11 ask 9 asks for:
> *"somebody else must decide whether a reproducible build forbids `ambient` or
> merely reports it, and that is a packaging decision, not a language one."*
> This note is that somebody. The convergence is recorded in §8, because two
> notes reaching the same conclusion from opposite ends is the strongest evidence
> in this file.

---

## 0. The sentence this note is trying to earn

> **Science is the language that can tell you what produced a result, and prove
> the same inputs give the same answer.**

That sentence is not currently anybody's. Four notes have each built a piece of
it while solving a different problem, and none of them names it. This note's job
is to decide whether the sentence is true, to bound it so that it stays true, to
find what is still missing, and to say whether it is worth the cost of saying out
loud.

The short answers, so the rest can be read against them:

1. **A version of the sentence is true.** The version that is true is narrower
   than the version that will be printed on a slide, and §2 fixes the wording.
2. **Five channels are open** that would falsify it, and one of them —
   §3's **G1** — sits inside the `random` contract that the whole pitch rests on.
3. **The enforcing mode should exist, and it is a build profile *over*
   `effects.md`'s `ambient` bit** — not an effect of its own, not an attribute.
   The mechanism is that note's; the policy is this one's. §4.
4. **This is a corollary of the core spec's bet, not a second bet** — it asks for
   no new language mechanism — but it is a *second pitch*, and it is the better
   one for the audience. `effects.md` §9.2 reached the same conclusion from the
   other end on the same day. §8.

---

## 1. The pieces, and the fact that none of them knows what it is building

| Note | Decision | The reproducibility property it actually delivers |
|---|---|---|
| `stdlib-standard.md` §5.3 | No ambient generator; no `random.seed()`; every draw names its generator because the generator is an argument | A program's random stream cannot be perturbed by a dependency. This is structural — there is no switch that turns it off. |
| `stdlib-standard.md` §5.5 | Generator, counter layout, `split` derivation, bit-to-float mapping and test vectors are contract | The same seed gives the same bits on every machine, forever |
| `scientific-libraries.md` §2 | The generator is **written in Science, never linked** | The above survives the FFI; C state behind a pointer would end it |
| `stdlib-standard.md` §5.7 | `Key.from_seed` is the only constructor — *"a key whose seed nobody recorded is a result nobody can reproduce"* | The seed is always in the source or in the run's recorded configuration |
| `collections-and-chains.md` §5.2 | `Map` and `Set` iterate in insertion order | Printing a `group(by:)` result is stable. Also frees the hash function (`stdlib-core.md` §3.5) |
| `collections-and-chains.md` §7.1 | `.parallel().sum()` is a tree reduction whose shape depends on length and a pinned grain size only | A parallel float reduction is not a function of the thread count |
| `data-io.md` §7 | `glob` returns byte-sorted results, always | Input file order does not depend on the host filesystem |
| `stdlib-core.md` §6.8 | `String: Ord` is byte order, never collation | `sorted()` over sample names is the same on a laptop and a cluster |
| `package-manager.md` §2.2 | Dependencies identified by SHA-256 of a canonical archive | The exact source bytes, or the build fails |
| `package-manager.md` §2.3 | A `[build]` table pins compiler, LLVM, target, flags | The toolchain is part of what a result was produced with, and is recorded |
| `package-manager.md` §2.6 | No build scripts, ever | `sciencec add` does not mean "run a stranger's code", and a build is a function of its declared inputs |
| `package-manager.md` §2.8 | No timestamp, hostname, user or absolute path in a binary | `sha256sum` on two binaries means something |
| `native-dependencies.md` §5.1 | Three tiers, named in the lockfile | A description is labelled as a description |
| `native-dependencies.md` §5.4 | `.science.provenance` in every binary, readable **without running it**; `os.provenance()` to the program | The artefact knows what produced it |
| `effects.md` §3.1 | An inferred `ambient` bit: *"reads state supplied by the machine rather than by an argument"*, with a witness chain per §3.6 | The analysis §4 needs, built for F5's replay policy and available here for free |

`effects.md` is the exception to the paragraph above: it is the one sibling that
*did* know what it was building. Its §3.2 splits `ambient` from `external`
precisely on the reproducibility question — *"`ambient` is what breaks
reproducibility across runs"* — and its §9.2 concludes that the language's real
claim is **"reproducibility the compiler checks"**. This note and that one are
the same argument approached from the mechanism and from the claim.

Read as a list it is a set of hygiene decisions. Read together it is one claim,
and the claim is made of three distinct things that are worth separating now
because §2 depends on the separation:

- **Structural determinism** — properties a program cannot opt out of, because
  the alternative is not expressible. There is no ambient generator to reach. A
  `Map` has no hash-ordered iteration to ask for. These are the strongest pieces
  and they cost the user nothing.
- **Recorded environment** — the lock, the `[build]` table, the tiers, the
  provenance section. These do not constrain anything; they make a substitution
  *visible*, which `package-manager.md` §2.4 correctly identifies as the entire
  difference between a reproducibility failure and a reproducibility finding.
- **Enforced discipline** — `--locked`, and §4's proposed `--deterministic`.
  These are flags, which means they are optional, which means §6 has to take
  seriously the objection that an optional mechanism is a discipline wearing a
  compiler's clothes.

---

## 2. The claim, bounded

### 2.1 Four different things are called "reproducible", and conflating them is how claims fail

The literature's vocabulary is contested — ACM swapped the definitions of
*reproducibility* and *replicability* in 2020 and half the field did not follow
— so this note defines its rungs by **mechanism** and uses the words only
informally.

| Rung | Preconditions | What is identical | Status |
|---|---|---|---|
| **R0 — provenance** | none | Nothing. The artefact states what produced it. | **Unconditional.** Every binary, every output. |
| **R1 — program determinism** | the program does not read the clock, the environment, the network, a thread schedule or entropy | The program's output is a function of its declared inputs | **Structural for the library's reproducible half; enforced by §4's mode for the rest** |
| **R2 — build identity** | same `science.lock`, same toolchain artefact, same target triple | The compiled binary, byte for byte | **Guaranteed**, contingent on §3's G8 (the compiler is itself deterministic) |
| **R3 — output identity, one target** | R1 + R2, Tier A native dependencies only, no `when available` table reached | The program's output, byte for byte, on any machine of that target triple | **Guaranteed within that envelope** |
| **R4 — output identity, across microarchitectures and across a substituted BLAS** | — | **Not the floats.** See §2.3. | **Refused. Will not be claimed.** |

### 2.2 Decision 1 — the headline claim is R0 + R3, and R4 is answered with a *named-channel* property rather than a tolerance

> **Decision 1.** Science's public reproducibility claim is exactly two
> sentences, and neither may be shortened in documentation, marketing or a
> conference talk:
>
> 1. **Given the archive and the recorded toolchain, a Science program rebuilt
>    for the same target produces a byte-identical binary, and that binary
>    produces byte-identical output on any machine of that target.**
> 2. **Where the target, the microarchitecture or a system library differs, every
>    value that is not derived from floating-point arithmetic is still identical
>    — the same draws, the same resamples, the same splits, the same records in
>    the same order — and every channel through which a float could have changed
>    is enumerated in the binary itself.**

Sentence 2 is the one worth arguing for, because it is the sentence the
competitors cannot write and because it is the one that survives the physics.

The usual way to answer "the last bits differ" is to state a tolerance: *the same
to 1e-9*. **That is rejected.** A tolerance claim is unfalsifiable without naming
the quantity it applies to, and in an iterative computation it is simply false:
a convergence test `if residual <= 1.0e-6` on a residual that differs in the last
bit takes a different branch, and the iteration count, the final parameters and
the reported χ² then differ by far more than a tolerance. Float differences do
not stay small. They amplify through every branch that reads a float —
convergence tests, adaptive step size, rejection sampling, k-means
reassignment, any `if` over a computed quantity. A language that promises a
tolerance is promising something it cannot bound, and **a reproducibility claim
that fails once is worse than none.**

What *can* be bounded is the set of ways a float can differ at all:

| Channel | Where it comes from | Recorded where |
|---|---|---|
| **libm** | `sin`, `exp`, `erf` lowered to a target intrinsic; 1 ULP, not bit-identical (`stdlib-core.md` §8.3) | `[build].float.libm` — and §3's G1 closes it for the functions `random` depends on |
| **FMA contraction and reassociation** | LLVM's licence to fuse `a*b+c`, and to vectorise a reduction | `[build].float` — and §3's G2 decides the policy |
| **BLAS kernel dispatch** | OpenBLAS choosing an AVX-512 kernel here and a Haswell kernel there. `native-dependencies.md` Tier B. | The Tier B probe string, which is *exactly* why that note kept it |
| **Thread count in a linked library** | `OMP_NUM_THREADS` changing a BLAS reduction tree | The Tier B probe (`MAX_THREADS=128`) and §3's G4 |
| **A `when available` library** | `native-dependencies.md` Tier C. A GPU path taken on one node and not another. | The run record only. §5. |

**Five channels, closed and enumerable.** That is the claim: not *the numbers are
the same*, but *there are exactly five ways they can differ and the binary tells
you which ones were live*. A reviewer who sees `float.contract = "off"`,
`libm = science-libm 0.4.1`, one Tier A BLAS and no Tier C table knows the
numbers are bit-identical without running anything. A reviewer who sees a Tier B
OpenBLAS with a different probe string knows precisely what to suspect.

**Rejected: claim bitwise reproducibility, unqualified.** It is what the slide
wants and it is false the first time somebody runs on a different cluster. The
failure would be public, it would be in a paper, and it would be attributed to
the language.

**Rejected: claim "reproducible to a stated tolerance".** Unfalsifiable, and
false through branches, as above.

**Rejected: claim only "the same inputs and the same recorded environment".**
This is true and it is what `package-manager.md` §2.5 states. It is also what
conda claims, and it concedes the differentiating half. R3 is stronger and the
design backs it.

**Rejected: buy R4 by refusing the host, as Nix and Guix do.** §6.1.

### 2.3 What Decision 1 costs

- **The scope travels with the claim or the claim is a lie.** Every statement of
  sentence 1 must carry "for the same target". This is an ongoing editorial
  burden across documentation, the website and every talk, and it is the most
  likely thing in this note to be quietly dropped by somebody who is not hostile,
  just brief. §10 records it as the top risk.
- **Sentence 2 requires §3's G1 and G2 to be closed.** As the design stands
  today, sentence 2 is *false*: `normal()` is specified bit-exact by
  `stdlib-standard.md` §5.5 and is built on a special function that
  `stdlib-core.md` §8.3 specifies to 1 ULP. Until that is fixed, the random
  stream is not identical across targets and the differentiating sentence does
  not hold.
- **A conformance suite is a precondition, not a follow-up.** §9 ask 8.

---

## 3. What is missing — the channel audit

The four assembled notes closed the channels they happened to walk past. This
section walks the rest. A channel is listed as **closed** only if a sibling note
decided it on purpose; "nobody has done it yet, but it would probably be fine" is
open.

### 3.1 Closed, and by whom

Recorded here so that the open list below is readable as what remains, rather
than as an indictment.

| Channel | Closed by |
|---|---|
| Ambient RNG | `stdlib-standard.md` §5.3 — structurally, not by convention |
| Reuse of a key | Core spec §6.1 rule 4 → `SC0301` |
| Hash-map iteration order | `collections-and-chains.md` §5.2 (insertion order) |
| Hash seed | `stdlib-core.md` §3.5 — free *because* of insertion order, with the caveat in G6 |
| Sort stability | `stdlib-core.md` §1.6 — `sorted()` is specified stable |
| String ordering | `stdlib-core.md` §6.8 — byte order, not collation |
| Unicode table drift | `stdlib-core.md` §6.7 — ASCII forms pinned at Level 1; Unicode forms in `text`, versioned |
| Time-zone data | `stdlib-standard.md` §3.2 — evicted to a versioned Level 3 package |
| Float *formatting* | Shortest round-tripping decimal is unique; the algorithm is free (`stdlib-shape-and-packages.md` §5.7) |
| Parallel reduction order | `collections-and-chains.md` §7.1 — pinned grain size, tree shape a function of length only |
| Input file order from a pattern | `data-io.md` §7 — `glob` sorts in byte order, always |
| Build-time code execution | `package-manager.md` §2.6 — no build scripts |
| Incidental binary non-determinism | `package-manager.md` §2.8 — no timestamp, hostname, user, absolute path |
| Source identity | `package-manager.md` §2.2 — content addressing |

That is a strong list, and it is the reason this note concludes the pitch is
viable at all. Nine of those fourteen were decided for reproducibility explicitly
by a note that was not writing about reproducibility.

### 3.2 Open — the five that matter

---

**G1. `normal()` is specified bit-exact and is built on a function specified to 1 ULP.**
*This is the most serious finding in the note.*

`stdlib-standard.md` §5.5 freezes the bits-to-float mapping and ships test
vectors, and chooses the **inverse CDF** for `normal()` specifically so that a
draw consumes a fixed number of words and is a pure function of the key. That
argument is right. But the inverse normal CDF is `erfinv`, a special function,
and `stdlib-core.md` §8.3 states:

> Level 1 math functions are specified to within **1 ULP** of the
> correctly-rounded result. They are **not** guaranteed bit-identical across
> targets. […] **this note states the contract and does not fix the problem**,
> because the fix is a codegen decision.

Both notes are internally consistent and together they are a contradiction.
`uniform()` is safe — integer mixing and a scale by 2⁻⁵³ is exact. `normal()`,
`exponential()`, `gamma()`, and every distribution sampler in `stats` that goes
through an inverse CDF, is **not** bit-exact across targets today, and it is the
function a scientist actually calls.

> **Decision 2. The functions the `random` contract depends on are frozen at the
> same weight as the contract. `random` does not call Level 1 `math`. It carries
> its own `erfinv` and its own `ln`, `exp` and `sqrt` where a distribution needs
> them, written in Science with a published polynomial, specified
> **correctly-rounded** rather than 1 ULP, and covered by the same shipped test
> vectors as `split` and the bit mapping.**
>
> **Rejected: relax `random`'s contract to 1 ULP.** It retracts the one claim
> `§1` of the core spec advertises, and it does so in the function everyone uses.
> The whole reason the generator is written in Science rather than linked
> (`scientific-libraries.md` §2) is that a C generator's state cannot be tracked;
> it would be absurd to protect the generator and then route its output through
> an unpinned libm.
>
> **Rejected: make all of Level 1 `math` correctly-rounded.** Correctly-rounded
> `sin` over the full range is an order of magnitude slower than a good 1-ULP
> implementation and is a research-grade undertaking; `stdlib-core.md` §8.3 is
> right to decline it. The scope here is one to four functions on a bounded
> domain, which is tractable.
>
> **Cost.** `random`'s inverse-CDF path is slower than a libm call — this is on
> top of the slowdown §5.5 already accepted against Box–Muller — and `random`
> grows an implementation it would rather have borrowed. Priced at low single
> weeks. It is also a second implementation of `erfinv` in the toolchain, which
> is duplication, and `math` should be the one to *move toward* the frozen
> version rather than the reverse.

This is a correction to `stdlib-core.md` §8.3 and an addition to
`stdlib-standard.md` §5.5, filed as asks in §9 rather than decided over them.

---

**G2. Value-changing float transforms are nowhere forbidden, and §3 of the core spec delegates everything to LLVM.**

Core spec §3: *"Custom optimizations; everything is delegated to LLVM."* LLVM,
given permission, will contract `a*b + c` to a single `fma`, reassociate a
reduction so it vectorises, and turn `x/c` into `x * (1/c)`. Each of those
changes the answer. Whether permission is given is a compiler flag nobody has
chosen, and `package-manager.md` §2.3's `[build]` table records the optimisation
level but not the float mode.

Worse: the `-ffp-contract` default differs between C compilers and between LLVM
versions, so *doing nothing* means the policy changes underneath the language
without anyone deciding it.

> **Decision 3. `sciencec` never enables a value-changing floating-point
> transform. FMA contraction is **off** by default; fast-math is not exposed as a
> flag at all; reassociation of a float reduction is forbidden, which is what
> makes `collections-and-chains.md` §7.1's *defined order* mean something at the
> machine level rather than only at the source level. The float policy is a field
> in the `[build]` table and in the provenance record.**
>
> **`fma(a, b, c)` is a function**, in `math`, lowering to the intrinsic. A user
> who wants the fused operation asks for it and gets it everywhere, which is both
> faster *and* more reproducible than letting the optimiser decide per call site.
>
> **Rejected: contraction on by default, as most C toolchains do.** It is
> typically a 10–30% win in a tight numeric loop and it is real money. It is
> refused because it makes the same source produce different numbers on two
> machines *for a reason not visible in the source or the flags*, which is the
> precise failure mode this note exists to remove. A user who needs the win writes
> `fma` and keeps determinism.
>
> **Rejected: `--fast-math` as an opt-in flag.** Every language that ships one
> discovers it in somebody's published build. If it is ever added it must be
> recorded in the `[build]` table and must make §4's mode a hard error, and it
> should not be added.
>
> **Cost.** Measurable, and it will be measured against Julia and against C in a
> benchmark somebody publishes. The mitigation is `fma` as an explicit function
> and the honest observation that vectorising a float reduction was never
> something a scientific user should get without saying so.

---

**G3. `time.now()` and `Monotonic.now()` are ambient, in exactly the shape `stdlib-standard.md` §5.3 rejects for randomness.**

§5.3 of that note makes the sharpest argument in the whole corpus:

> The problem is **not** that NumPy has a seeding function. The problem is that
> `np.random.uniform()` *works without one*.

`time.now()` works without one. It takes no argument, names no capability, and
its result depends on the world. A library function can call it and the caller's
signature says nothing. This is the same hazard as an ambient generator, applied
to a different global, and the note that diagnosed the hazard did not apply its
own diagnosis one section earlier in the same file.

Two ways it reaches a published number, both common:

- **Seeding.** `Key.from_seed(now().nanoseconds())` — the single most common way
  a scientist destroys their own reproducibility, and `Key.from_seed`'s whole
  design (`stdlib-standard.md` §5.7: *"a key whose seed nobody recorded is a
  result nobody can reproduce"*) is defeated by one call.
- **Wall-clock budgets.** `loop: if Monotonic.now().since(start) > budget: break`
  makes the iteration count a function of machine speed and of what else was on
  the node. Early stopping on a time budget is standard practice in optimisation,
  MCMC and hyperparameter search.

> **Decision 4. The clock is not removed and is not made unsafe. It is *seeded*,
> and one dataflow is diagnosed by default.**
>
> - `time.now`, `Monotonic.now` and `Monotonic.since` are `ambient` seeds. The
>   first two already are, in `effects.md` §3.1's table; `Monotonic.since` should
>   join them, since a duration between two machine readings is as machine-derived
>   as the readings. Under §4's mode they are an error unless allowed explicitly.
> - **`SC0237`**, a **warning by default and outside the mode**: a value derived
>   from an `ambient` seed reaches `Key.from_seed`. This is a two-hop local
>   dataflow over a bit the compiler already computes; it is cheap, it fires on
>   the exact idiom, and it is the highest-value diagnostic in this note.
>
> **Rejected: make `now()` unsafe**, as `os.env.set` is. `unsafe` in this language
> means memory or ABI hazard (`ffi-c-boundary.md` §3.1); overloading it with
> "irreproducible" would make the word mean two things and would make every
> progress bar unsafe.
>
> **Rejected: a `Clock` capability value threaded through calls**, which is the
> principled answer and is what `Key` does. Rejected because the clock is read by
> logging, by progress reporting and by timing code that has no bearing on a
> result, so the threading cost falls overwhelmingly on code that does not need
> it — the opposite of `Key`, where the cost falls on exactly the code that does.
> `effects.md` Decision 1 reaches the same place by a different route: it refuses
> to add a bit that duplicates what a signature already says, and the inferred
> `ambient` bit is what it gives instead of a threaded value. Both notes decline
> the capability, and the inferred bit is the better answer.

Rendered:

```
warning[SC0237]: this random key is seeded from the clock
  --> fit.science:22:24
   |
22 |     let key be Key.from_seed(now().nanoseconds())
   |                              ^^^^^ `time.now` is an `ambient` seed
   |
note: `Key.from_seed` is the only key constructor precisely so that every
      result has a seed somebody wrote down
help: write the seed, or take it from the run's configuration
   |     let key be Key.from_seed(20260916)
   |     let key be Key.from_seed(config.seed)
help: if the draw is genuinely incidental, `Stream` is the face that says so
   |     let mutable stream be Stream.from_entropy()
```

---

**G4. `thread` has no reproducibility story at all, and `.parallel()` has a careful one.**

`collections-and-chains.md` §7.1 pins the grain size *into the semantics* and
`stdlib-shape-and-packages.md` §5.7 lists that as a criterion violation bought
knowingly for published results. That care stops at the chain API.
`stdlib-standard.md` §10 specifies `Thread.start`, `Channel.bounded`, `Mutex`,
`RwLock`, `Atomic` and `Barrier` across eighty lines and does not use the word
"reproducible" once.

Every one of them is a channel:

- `Receiver.receive` delivers in arrival order, which is a schedule.
- `Atomic.fetch_add` returns the *previous* value, which is a schedule; the final
  total is deterministic and the per-thread sequence is not. A worker that
  accumulates into a float under a `Mutex` has a nondeterministic summation
  order and therefore a nondeterministic answer.
- `Thread.available_parallelism()` is a machine fact that will be used to size a
  work split, making the split — and hence a float reduction over it — a function
  of the core count.

`effects.md` catches part of this and not the rest. Its `ambient` bit is seeded
by *"the scheduler"* and *"thread identity"* in the prose of §3.1, and
`os.cpu_count` is in the seed list — so `Thread.available_parallelism` is
covered. `Channel` receive order and lock acquisition order are not seeded, and
they are the two that change a float sum. That note's Decision 1 declines a
`thread` *effect* on the correct grounds that `Send`/`Share` are questions about
values; this is a different question — not *may this cross a thread boundary* but
*is the order the machine chose observable* — and it is exactly what `ambient`
already means.

> **Decision 5. Raw `thread` primitives are `ambient` for §4's purposes, and
> `.parallel()` is not.** `Receiver.receive`, `Sender.send`, `Mutex.with_lock`,
> `RwLock.with_read`, `RwLock.with_write`, `Atomic`'s read-modify-write
> operations and `Barrier.wait` are asked of `effects.md` as additions to the
> `ambient` seed set (§9 ask 9); `Thread.start`, `Thread.join` and
> `Thread.available_parallelism` are already there or follow trivially.
> `.parallel()` is permitted, because `collections-and-chains.md` §7.1 already
> bought its determinism, and the worker pool underneath it is `science-rt`'s
> rather than the user's.
>
> This makes `--deterministic` a *reason* to prefer the chain API over hand-rolled
> threads, which is the direction `stdlib-shape-and-packages.md` §3.6 already
> wants the ecosystem to go, and it turns a style preference into a checkable one.
>
> **Rejected: a deterministic-scheduler mode** that replays a recorded interleaving.
> It is what `rr` and deterministic-replay debuggers do, it is a large systems
> project, and it buys reproducibility at a cost in throughput that defeats the
> reason threads were used.
>
> **Cost.** A legitimate producer/consumer pipeline over a bounded channel — a
> reader thread feeding a compute thread — is forbidden under the mode even when
> its output is order-independent. That is real over-rejection, and the escape is
> the recorded allow-list of §4.3, not a weakening of the rule.

---

**G5. Run-time inputs that are not the program are not recorded.**

The provenance record of `native-dependencies.md` §5.4 describes the *program*
completely and the *run* not at all. A paper's claim is about both. Nothing
currently records:

- **`os.env.get`** — the configuration a run was given. A run is a function of
  its environment and no artefact says which variables were read or what they
  held.
- **`os.args`** — the command line. Trivially recordable and currently not.
- **The input files' contents.** `data-io.md`'s readers open a `Path` and return
  a `Frame`. Two runs over "the same CSV" that is not the same CSV are the single
  most common real-world reproducibility failure in science, and it is invisible.
- **`os.run`** — a subprocess, which is arbitrary.
- **`net` / `http.download`** — `stdlib-standard.md` §12.5 names streaming a
  dataset to disk as the module's most common use by far. A dataset fetched at
  run time from a URL that has since changed is the second most common failure.

> **Decision 6. The run record (§5.2) carries the argv, the name and value of
> every environment variable actually read, the digest of every file opened
> through `data-io`, and the digest of every body fetched through `http`. The
> program does not opt in; the reader records as it reads.**
>
> **Digest policy, because hashing a 200 GB input is not free.** Every input is
> recorded as `(path, bytes, mtime, digest)` where `digest` is the SHA-256 for a
> file under a threshold (default 1 GiB) and the literal string `"unhashed"`
> above it. **The absence is recorded, never inferred** — a reviewer must be able
> to tell "not hashed" from "hash matched". `--hash-inputs=always|auto|never`.
>
> **Rejected: hash nothing and record the path.** A path is not an identity;
> `/scratch/az/run3.csv` says nothing a year later.
>
> **Rejected: hash everything unconditionally.** It makes the language slower than
> Python on a job whose real work is one pass over a large file, which is a large
> fraction of this audience's jobs.

### 3.3 Open — the four smaller ones

**G6. `Hash` is a user-visible interface and the seed is per-process.**
`stdlib-core.md` §3.5 seeds the hasher per process and argues correctly that
nothing observes it, *because* `Map` iterates in insertion order. But
`collections-and-chains.md` §5.2 also requires `Hash` to exist as an interface,
which means a program can call it and print the result, and that result differs
between runs on the same machine. The invariant `stdlib-core.md` asks to have
written down needs one more clause: **`Hash`'s output is not observable — it has
no `Display`, it is not convertible to an integer, and it is not `Ord`.** If a
program needs a stable digest it uses `crypto.sha256`, which is what that module
is for. Cheap; an ask on `stdlib-core.md`.

**G7. `list_directory` is not sorted and `glob` is.** `data-io.md` §7 gives
`glob` byte-order sorting with a paragraph explaining that filesystem order is a
correctness property for an audience that publishes — and leaves
`list_directory` directly above it returning `Array of Path` in whatever order
the platform gave. The argument applies verbatim. `list_directory` should sort in
byte order. This costs a sort and closes a channel that will otherwise be found
by a user whose results reordered when they moved from ext4 to Lustre.

**G8. The compiler must itself be deterministic, and nothing tests it.**
`package-manager.md` Decision 7 claims byte-identical binaries. That claim rests
on `sciencec` being deterministic — monomorphization order, symbol emission
order, any iteration over a Rust `HashMap` in the query graph, and parallel
codegen unit partitioning. rustc took years to get this right and still has
flags for it. It is not a design decision; it is a test, and it is the cheapest
possible one: **build twice, compare hashes, in CI, from the first commit.**
Without it R2 is an aspiration.

**G9. Text-to-float parsing must be correctly rounded.**
`stdlib-shape-and-packages.md` §5.7 lists `data.csv` number parsing as a
performance surface and it is — but *correct rounding is the observable*, not the
algorithm, and a SIMD parser can be correctly rounded (`fast_float` is). If two
`sciencec` versions parse `0.1000000000000000055511151231257827` differently, the
lock protects nothing. Specify the observable, free the algorithm. Zero cost, and
it is `Decision 5d`'s compliance rule 1 applied to a case that note did not list.

**Also considered and dismissed as non-issues:** address-dependent behaviour (no
`Display` in the library prints an address, and `ffi.Pointer` is not `Ord`; worth
one sentence in the reference, not a decision); struct padding in `npy` and
`safetensors` writers (those formats are dense arrays, not structs); `panic`
messages (deterministic, and the bounds-check panic of
`indexing-and-array-literals.md` §1.3 is a function of the data); `logging`
timestamps — the log is not an output, nothing can read it back, and
`effects.md` §3.2 already classes the global logger as `external` rather than
`ambient` for exactly this reason; note that this does **not** rescue a program
that *reads* a clock in order to log it, which §4.3 addresses and does not
solve).

---

## 4. `--deterministic`

### 4.1 Decision 7 — the analysis is `effects.md`'s; this note supplies only the policy over it

`effects.md` §11 ask 9 ends with the sentence this section exists to answer:

> **Of whichever note owns a reproducible-run mode** […] `SC0217` is offered and
> is not claimed as policy. This note supplies the bit; somebody else must decide
> whether a reproducible build *forbids* `ambient` or merely *reports* it, and
> that is a packaging decision, not a language one.

> **Decision 7. It forbids. `sciencec build --deterministic` is an error when the
> `ambient` bit is reachable from `main`, unless the offending seed is named in an
> explicit, recorded allow-list. The diagnostic is that note's `SC0217`, with its
> witness chain, and this note claims no code for it.**

**Why forbid rather than report.** A report is what the language already does
everywhere else — `SC0286` reports an undischarged bounds check,
`native-dependencies.md` `SC0479` reports a changed Tier B library — and the
argument for reporting is strong: a warning is usable during development and a
worked-around mechanism protects nobody (`package-manager.md` §2.3's reasoning for
`[build]` mismatches). It loses here for one reason: **the mode is opt-in
already.** Nobody builds with `--deterministic` by accident, so the usability
argument that saves a default warning does not apply to a flag a user typed. A
`--deterministic` build that prints warnings and produces a binary anyway is a
flag with no effect, and a flag with no effect is worse than no flag, because the
artefact records that it was used.

And the mode is *reported* by default for everybody who did not type it, because
the bit is inferred either way and `.science.provenance` records whether the build
was deterministic (§5.2). So the design gets both: report always, forbid on
request.

**This makes `--deterministic` a sibling of `--verified`.**
`indexing-and-array-literals.md` §8 ask 9 asks whether such a profile is planned,
because `SC0286`'s whole argument rests on it. **This note answers yes and makes
them one family**: profiles under which a diagnostic the compiler emits as a
warning becomes an error. `--verified` turns "I could not prove this index is in
range" into an error; `--deterministic` turns "I found a path to the machine" into
one. Same shape, same rendering, same argument — the compiler tells you what it
proved, and a lab that wants the guarantee takes a build flag rather than a
dialect.

**Rejected: a fourth effect bit.** An earlier draft of this note proposed a
`nondeterministic` effect and asked `effects.md` to reserve the name. That note's
Decision 2 closes the set at three bits and calls the closure *"the most
load-bearing sentence in the note"*, and it is right: a fourth bit would be the
first accretion, and `ambient` already means what the fourth bit would have meant.
The proposal is withdrawn.

**Rejected: an attribute on `main`.** The language has **no attribute syntax at
all** — nothing in twenty-seven design notes uses `@` or `#[…]`. Introducing one
here would be a grammar change, a new lexical form, and a precedent every
subsequent note would draw on; and it would put a whole-program property in one
file, invisible to anyone reading a library. A profile is a property of a build,
which is what this is. (`pure function` is not a counter-example: `effects.md`
Decision 5 makes it a *modifier on a definition*, in `unsafe function`'s existing
position, and it asserts something about that one function rather than about the
program.)

**Rejected: folding it into `--locked`.** A direct contradiction with
`package-manager.md` §2.3 — *"`--locked` is the single strict mode"* — and it is
named rather than smuggled. The two must stay separate because they constrain
different things and are independently useful:

| | Constrains | Answers | Useful alone? |
|---|---|---|---|
| `--locked` | the **environment** — source hashes, `[build]`, native tiers | "is this the recorded environment?" | Yes — for any CI build |
| `--deterministic` | the **program text** — what the call graph can reach | "is this program a function of its inputs?" | Yes — a lab can require it of its own code long before it has a lockfile |

They compose, and a published artefact uses both. That sentence of
`package-manager.md` is right within its scope — there is one strict mode over the
environment — and should be amended to say so rather than reversed (§9 ask 5).

### 4.2 What the mode rejects, and the one place `ambient` is not enough

The rejected set is **`ambient`, plus a short supplementary list**, and the
supplement is a refinement of `effects.md` §3.2 rather than a disagreement with
it. That section argues, correctly, that `external` must not be forbidden:

> A program reads its input file. That is `external` and it is completely fine in
> a reproducible run: the path is in the program, the bytes are in the lockfile's
> world, and two runs over the same file agree. **Reproducibility must not forbid
> reading data.**

That is right for a local `Path`. It is **not** right for two members of the
`external` set:

- **`net` and `http`.** `http.download` fetches bytes from a URL. Two runs do not
  agree, because the remote resource changes, and — decisively — the bytes are not
  in any artefact this design produces. A local input is at least *recordable*
  (Decision 6 digests it); a URL fetched at run time is recordable only after the
  fact, and by then the evidence is gone. `stdlib-standard.md` §12.5 names
  streaming a dataset to disk as `http`'s most common use by far, so this is the
  central case, not an edge.
- **`os.run`.** A subprocess is arbitrary, and its `external`-ness understates it:
  the program it runs is not in the lock, not in the archive, and not described
  anywhere.

So:

> **Decision 8. `--deterministic` rejects the `ambient` bit, plus `net`, `http`
> and `os.run` from the `external` set. Nothing else in `external` is rejected —
> a program may read files, write files, print, and log.**
>
> **Rejected: reject all of `external`.** It forbids reading the data, which is
> the program's whole purpose, and `effects.md` §3.2 already gives the reason.
>
> **Rejected: ask `effects.md` for a fourth bit to separate "the world, recorded"
> from "the world, not recorded".** Same answer as §4.1: the set is closed, and a
> three-item supplementary list in a build profile is a much smaller thing than a
> bit in a published interface. **But the distinction is real and that note should
> record it in §3.2 as a limit of the two-bit split** (§9 ask 9), because a reader
> of §3.2 alone would conclude that `http.get` is safe in a reproducible run.

Three link-time and source-level additions that are not function calls and so
cannot be a bit at all:

- a reachable `when available` table — `native-dependencies.md` Tier C, which
  resolves by `dlopen` at run time and whose availability is a machine fact;
- an `unordered` reduction (core spec §5.1), which is an explicit opt-out written
  in the source;
- `list_directory`, until G7 lands and it sorts.

Note what is **not** rejected: `.parallel()`, `Map`/`Set` iteration, `sorted()`,
`glob`, `read_file`, `print`, `logging`, `Key` and every `Key`-taking draw. Those
are the reproducible half of the library, and the mode is a reason to stay in it.

**Diagnostics.** The mode's core diagnostic is `effects.md`'s, adopted unchanged;
this note claims four codes for what that note does not cover, from the free
resolution block `SC0237`–`SC0249`.

| Code | Owner | Fires on | Severity |
|---|---|---|---|
| `SC0217` | `effects.md` | The `ambient` bit is reachable from `main` under `--deterministic`. Renders **every distinct witness chain, not the first**, which is that note's Decision 4 and is what makes it usable | error under the mode |
| `SC0237` | here | A value derived from an `ambient` seed reaches `Key.from_seed` (G3) | **warning by default**; error under the mode |
| `SC0238` | here | `net`, `http` or `os.run` is reachable under the mode — separate from `SC0217` because the fix and the explanation differ: fetch it once and commit the digest, rather than take it as a parameter | error under the mode |
| `SC0239` | here | A `when available` (Tier C) table is reachable under the mode | error under the mode |
| `SC0240` | here | An allow-list entry names a seed that is not reachable — a stale allowance, which is how an allow-list rots into a rubber stamp | warning |

`SC0241`–`SC0249` are left free. `unordered` under the mode is `SC0217`'s message
with a different witness and needs no code of its own; it is distinct from
`stdlib-shape-and-packages.md`'s `SC0279`, which is about a non-associative
combiner in a parallel reduction and is an algebraic property rather than a
reachability one.

```
error[SC0217]: `spectra.fit` carries the `ambient` effect, and this build is
               --deterministic
  --> fit.science:41:18
   |
41 |     let result be fit_peaks(counts, key)
   |                   ^^^^^^^^^ reaches the machine through this call
   |
note: the effect enters through this call chain
   |
   |  fit_peaks -> anneal -> within_budget -> time.now
   |
  ::: spectra/anneal.science:88:12
   |
88 |     if Monotonic.now().since(started) > budget:
   |        --------------- `Monotonic.now` reads the machine's clock
   |
   = `anneal` stops on a wall-clock budget, so its iteration count depends on how
     fast this machine is and on what else was running on it
help: an iteration budget is reproducible; a time budget is not
help: or allow it, and the allowance travels with the artefact
   |  sciencec build --deterministic --allow-ambient=time.monotonic_now
```

The witness *chain* rather than the call is the requirement that makes this
usable, and it is already `effects.md` Decision 4: *"a diagnostic that says only
'this function carries the `python` effect' is a bug, not a diagnostic."* The
same applies here with more force, because under this mode the offending call is
usually inside a dependency the user did not write and cannot immediately see.

### 4.3 The escape is an allow-list, and the allow-list travels

> **Decision 9. `--allow-ambient=<seed>[,…]` permits named seeds. The full list is
> recorded verbatim in the `[build]` table, in `.science.provenance`, and in
> `sciencec archive`'s output. An allowance is never silent and is never local to
> one machine.**

This is the language's own philosophy applied to itself. `SC0286` does not ban an
undischarged bounds check, it *declares* one. `Stream.from_entropy()` is not
banned, it is *named at the call site*. An allowance converts "forbidden" into
"declared", and a declaration a reviewer can read is worth more than a prohibition
that gets worked around.

It also answers the obvious objection, which is that the mode over-rejects — and
the objection is worse than it first looks, so it is worth stating exactly.
`effects.md` §3.5 propagates a bit from its **seed**, not to its **sink**. A
function that reads the clock and does nothing with it but print a progress
message still carries `ambient`, and so does everything that calls it, all the way
to `main`. **A progress bar with an ETA is therefore rejected**, and no amount of
care about `external` rescues it: the `ambient`/`external` split says reading an
input file is fine, not that printing a timestamp is.

The honest options were:

- **Information-flow analysis** — track whether an `ambient` value *reaches* an
  output rather than whether a call reaches a seed. This is the correct analysis
  and it is a research-grade addition to a compiler that does not have one.
  Rejected for F0–F2 on cost, and named in §4.4 as the thing a richer effect
  design would buy.
- **Exempt `logging` and `print_error` as sinks.** Attractive, and it does not
  work, for the reason just given: the bit is on the caller before it reaches any
  sink. Exempting a sink would require the flow analysis that was just rejected.
  So this is **not** adopted, and the earlier draft of this note that claimed it
  was wrong.
- **Per-call-site markers.** Rejected in §4.1 — a new keyword, and it puts a
  whole-program property in one line of one file.

Which leaves the allow-list carrying more weight than is comfortable, and that is
stated rather than minimised: **a program that wants an ETA allows
`time.monotonic_now`, and the artefact says so.** That is a worse outcome than a
flow analysis would give and a better one than either a silent exemption or a
mode nobody can use.

**Cost.** An allow-list is a place for a lab to write "allow everything" and stop
thinking, which is how `# type: ignore` and `#[allow(…)]` rot. Three mitigations,
all cheap: **`*` is not accepted** — every entry is a named seed from
`effects.md` §3.1's closed list; `SC0240` fires on a stale entry; and the archive
prints the list where a reviewer sees it, which is the only enforcement that has
ever worked.

### 4.4 What a richer effect design would add, stated as a dependency and not a request

`effects.md` Decision 2 closes the set at three bits and Decision 3 keeps effects
out of the type. Both are right for the reasons that note gives, and this note
asks for neither to be reopened. Recorded here is only what the mode would gain if
they ever were, so that the cost of the current design is visible rather than
assumed away:

1. **Information flow would remove the over-rejection of §4.3** — the ETA case —
   by distinguishing a value that reaches an output from a call that reaches a
   seed. This is the only real defect in the mode.
2. **Effect *polymorphism* would let a higher-order function be deterministic when
   its closure is.** Today `map`'s bit is the union over its callers' closures,
   which §3.5 computes correctly, so this is a precision question rather than a
   soundness one.

Two things the mode does **not** need and that already exist: `effects.md`
Decision 5's **`pure function`** already lets a library assert a checked empty
effect set at a definition, which is the "a library can declare itself clean"
property; and its Decision 6 records effect sets in the published package
interface with `SC0216` warning on drift, which is how a dependency that acquires
a clock read in a patch release becomes visible at `sciencec add` time rather than
at the next `--deterministic` build. Both were on an earlier draft's wish list and
both are already decided.

### 4.5 Who uses it, and what it costs them

- **A lab publishing a paper.** The target user. Turns it on for the analysis
  pipeline, not for the exploratory scripts. Cost: rewriting a wall-clock budget
  as an iteration budget, which is a better program anyway, and allowing
  `time.monotonic_now` for the progress bar.
- **A shared scientific library.** Turns it on in CI so it cannot acquire a clock
  read by accident — or, more precisely, marks its entry points `pure function`,
  which is stronger and is checked at the definition. Cost: near zero.
- **A benchmark harness, a data downloader, an interactive session.** Never turns
  it on, correctly. `--deterministic` is not a quality bar and the documentation
  must not present it as one, or people will turn it on for programs it makes no
  sense for and then hate it.
- **The compiler's own test suite.** Turns it on as a UI-test fixture, which is
  how the seed list stays honest.

**Engineering cost, now that `effects.md` exists: most of it is already being
paid.** The `effects(DefId)` query, the reverse-topological traversal, the
fixed point per SCC and the witness chains are that note's §3.5 and §10 item 1,
built in F0 for F5's replay policy (§10.1) whether this mode ships or not. What is
left here is the policy: the supplementary list, the allow-list plumbing into the
`[build]` table and the provenance record, four diagnostics, and `SC0237`'s
two-hop dataflow. **One to two weeks**, against the two to three an earlier draft
of this note estimated when it thought it had to build the analysis itself.
## 5. The artefact a reviewer is handed

`native-dependencies.md` §10 asks `data-io.md` to stamp the provenance record
into output metadata by default and calls it *"the single highest-value thing in
this note"*. This section picks that up and makes it concrete.

### 5.1 Decision 10 — one record, canonical JSON, with an identity

> **Decision 10. The provenance record is **canonical JSON**: UTF-8, keys sorted
> by byte order, no insignificant whitespace, no floating-point values anywhere,
> `\n` normalised. It carries a `schema` integer. Its **provenance id** is the
> SHA-256 of the canonical bytes with the `id` field absent, and it is quoted in
> short form as twelve hex characters.**

Format alternatives, and why each loses:

- **CBOR or a bespoke binary form.** Smaller, and it breaks
  `native-dependencies.md` Decision 9's best property: `readelf -p
  .science.provenance` prints it with no Science toolchain present. A reviewer in
  2031 with the binary and no `sciencec` is exactly the case that decision was
  written for.
- **TOML.** The manifest's language, and it has no specified canonical form;
  arrays of tables nest badly for the native tier entries; and `data-io.md`
  already ships a JSON writer while TOML is a Level 3 package
  (`stdlib-shape-and-packages.md` §5.2).
- **A line-oriented text format of our own.** One more parser, one more
  canonicalisation surface to get wrong once, per `package-manager.md` §2.2's own
  warning about its archive rules.

**No floats in the record**, deliberately: a record about floating-point
reproducibility that is itself sensitive to float formatting would be an
embarrassment, and every field it needs is a string or an integer.

**The provenance id is the quotable artefact.** A methods section writes:

> Analysis performed with Science (provenance `pv:9f3a1c04e7b2`); the full
> record and archive are deposited at …

A reviewer pastes it into `sciencec provenance --id pv:9f3a1c04e7b2 ./a.out` and
gets yes or no. That is a smaller, harder thing to cite than a Docker digest and
it addresses the program rather than the filesystem.

### 5.2 Decision 11 — two records, because only the run knows some of it

> **Decision 11. The **build record** is what `native-dependencies.md` Decision 9
> emits into `.science.provenance`; it is static, readable without execution, and
> identical for every run of the binary. The **run record** embeds the build
> record verbatim and adds what only a run can know. Outputs are stamped with the
> run record; a binary carries the build record.**

The build record gains three fields on top of `native-dependencies.md`
Decision 9's list, all from this note:

```json
{
  "schema": 1,
  "build": {
    "compiler": "sciencec 0.4.1",
    "llvm": "18.1.8",
    "target": "x86_64-unknown-linux-gnu",
    "opt": "2",
    "float": { "contract": "off", "fast_math": false, "libm": "science-libm 0.4.1" },
    "deterministic": true,
    "allow_ambient": ["time.now"],
    "provenance_mode": "minimal"
  },
  "packages": [ { "name": "spectra", "version": "0.3.2", "sha256": "…" } ],
  "native":   [ { "link_name": "openblas", "capability": "blas", "variant": "lp64",
                  "tier": "system-recorded", "soname": "libopenblas.so.0",
                  "sha256": "…", "probe": "OpenBLAS 0.3.26 USE64BITINT= MAX_THREADS=128 Zen4" } ],
  "id": "9f3a1c04e7b2…"
}
```

`float`, `deterministic` and `allow_ambient` are the three new ones, and
they are what make §2's *named-channel* claim checkable from the binary alone.

The run record adds the argv, the environment actually read, the input digests
(G5), the Tier C resolutions, and the exit status:

```json
{
  "schema": 1,
  "build_id": "9f3a1c04e7b2…",
  "build": { "…": "the build record, embedded verbatim" },
  "run": {
    "started": "2026-09-16T10:41:02Z",
    "argv": ["fit", "--input", "runs/a.csv", "--seed", "20260916"],
    "env": [ { "name": "EXPERIMENT_ROOT", "value": "/scratch/az" } ],
    "inputs": [ { "path": "runs/a.csv", "bytes": 84213, "sha256": "…" } ],
    "resolved": [ { "link_name": "cudnn", "loaded": false } ],
    "allowed_used": ["time.now"],
    "exit": 0
  }
}
```

`allowed_used` is worth the field: an allowance that was permitted and never
exercised is different from one that was, and a reviewer should see which.

### 5.3 Decision 12 — where it goes in each format, including the two that cannot take it

> **Decision 12. `data-io.md`'s writers stamp the run record by default.
> `--no-provenance` suppresses it, and the suppression is not recoverable, so the
> flag exists and is documented as a choice with a consequence.**

| Format | Carrier |
|---|---|
| Parquet | key-value metadata under `science.provenance` |
| Arrow IPC | schema custom metadata, same key |
| npz | a member file `science-provenance.json` |
| safetensors | the `__metadata__` header map |
| HDF5, when it lands | a root-group attribute `science.provenance` |
| **CSV, JSON Lines** | **a sidecar** `<output>.provenance.json`, written atomically beside the file |

The last row is the honest part. A comment line at the head of a CSV breaks
downstream parsers — including `data-io.md`'s own reader, which has no comment
syntax — and a JSON Lines file whose first line is not a record is not JSON Lines.
A sidecar is worse than embedding: it gets separated from its data by the first
person who copies one file. It is chosen anyway because the alternative is
corrupting the format, and a sidecar that is sometimes lost beats an output that
no tool can read. The writer logs the sidecar's path so the separation is at
least noticed once.

**Size.** `native-dependencies.md` §5.4 caps the section at 8 KB and degrades by
dropping Science package entries first. The run record inherits the cap; input
digests degrade after native entries and before packages, because a truncated
input list is the least useful truncation.

**Privacy.** `--provenance=minimal` from that note's §5.4 drops absolute paths;
this note extends it to drop **environment variable *values*** while keeping
their names, and to record input paths as basenames. A cluster path discloses a
username; the fact that `EXPERIMENT_ROOT` was read does not. The mode is recorded,
so a reader can tell stripped from absent.

### 5.4 Decision 13 — what a reviewer does, at three costs

> **Decision 13. Three verbs, with decreasing availability and increasing
> strength, and each is useful without the next.**

1. **Read — seconds, no toolchain.** The record is text in a named section.
   `readelf -p .science.provenance ./a.out`, or `unzip -p results.npz
   science-provenance.json`, or any Parquet metadata viewer. This is the rung
   that works for a reviewer with a laptop, an artefact and no access to the
   cluster the work was done on — which `native-dependencies.md` §5.4 correctly
   identifies as the common case and is the reason the record must not need
   executing.
2. **Compare — seconds, needs `sciencec`.** `sciencec verify results.parquet`
   reads the stamped run record and prints what differs *from this machine*: a
   different BLAS probe string, a missing Tier C library, a compiler version this
   toolchain is not. It does not rebuild and does not need the sources. This is
   the rung a reviewer will actually use, and it is the one that converts the
   record from an archive into a tool.
3. **Rebuild — needs the archive, the toolchain tarball, and time.**
   `sciencec build --locked --deterministic` inside `sciencec archive`'s tarball
   (`package-manager.md` §6.4), then rerun and compare output digests. Within the
   R3 envelope this is byte-for-byte and the comparison is `sha256sum`. Outside
   it, `sciencec verify` names which of §2.2's five channels differ, which is the
   actionable output.

**Decision: the record is not signed in F0–F1.** Signing needs a key
distribution story, which needs the registry, which is F2 (`package-manager.md`
§7.3). Until then the content hash plus a deposited archive is what makes
tampering detectable, and claiming more would be security theatre. Named as F2
work, alongside the registry's signing.

```science
use os (provenance, Provenance)
use data.parquet (write_parquet)

## The stamping is automatic. This is what a program does when it wants to
## *check* rather than record — the pattern for a pipeline that refuses to
## write results it cannot stand behind.
function write_results(frame: borrowed Frame of Fit, out: borrowed Path)
        -> ((), Error?):
    let record: Provenance be provenance()

    for dep in record.native:
        if dep.tier is "system-recorded":
            print(f"note: {dep.link_name} is {dep.probe}, recorded not pinned")

    if record.allow_ambient.length() > 0:
        for name in record.allow_ambient:
            print(f"note: this build allows {name}")

    let err be write_parquet(frame, out, ParquetOptions.default())
    if err?:
        return ((), err)

    print(f"wrote {out} under provenance {record.id.truncate(12)}")
    return ((), null)
```

---

## 6. Testing the differentiation claim

The claim to test: *these tools are all outside the language and therefore
optional, while a consumed `Key` and an enforced build mode are not.* Half of it
survives.

### 6.1 Nix and Guix — strictly stronger where they apply, and they cannot apply here

Nix and Guix reproduce a build exactly, and they do it by owning the entire
closure down to the C compiler and libc. That is stronger than anything in §2.
Science cannot follow, for a reason that is already decided:

`ffi-c-boundary.md` §5.1 makes dynamic linking the default **specifically**
because *"HPC module systems work by swapping the shared object under a fixed
name — statically linking OpenBLAS defeats the mechanism the cluster
administrator is relying on"*, and `package-manager.md` §6.6 confirms `module
load openblas/0.3.27` is the **supported** mechanism. A Nix-shaped Science would
forbid the site's tuned BLAS, the site's MPI and the site's CUDA driver — and the
site's BLAS is often 3–10× the generic build. **Refusing the host means refusing
the machines the language is for**, and it also means refusing a performance
factor that changes which science is possible. `native-dependencies.md` §5.2 made
this trade explicitly and this note does not reopen it.

But the comparison cuts the other way too, and this is the part worth having:

> **Nix reproduces the build and says nothing about the program.** A Nix-built
> Python script that calls `np.random.uniform()` without a seed is irreproducible,
> and Nix cannot tell, because nondeterminism inside a derivation's *runtime* is
> outside its model entirely. Nix's output is a byte-identical `/nix/store` path;
> what the program in it does when run is not its subject.

That is the seam. Nix and Science are solving adjacent halves, and Science's half
is the one nobody is solving. A user could reasonably run both.

### 6.2 Docker — records a filesystem, not a derivation

An image is a bag of bytes with no recipe attached; the Dockerfile that made it
is not in it, and `apt-get install` inside a Dockerfile is not reproducible by
construction. Base image tags move, registries garbage-collect, and a 2 GB opaque
blob is *harder* to audit than a 4 KB text record — a reviewer cannot read a
Docker image, they can only run it, which returns us to needing the cluster.
Docker also says nothing at all about the program, exactly as Nix does not.

Where Docker wins outright: it works for every language today and Science works
for Science. That is not a small objection.

### 6.3 conda, `renv`, `packrat` — the right instinct, one layer

`environment.yml` is a solver *input*, not a solution; `conda-lock` fixes that
and is a separate tool people do not use by default. `renv` and `packrat` are the
closest in spirit — per-project library, lockfile, restore — and they are
genuinely good. All three share three limits: they pin packages and not the
compiler; they are outside the language, so a project can simply not have one;
and they say nothing about the program's own nondeterminism. R's `set.seed()` is
precisely the process-global hazard `stdlib-standard.md` §5.3 rejects, and no
`renv.lock` can detect a missing one.

### 6.4 Jupyter — a cautionary case, and an ask on this project

A notebook is the dominant artefact in computational science and it is
*actively* anti-reproducible: cells execute in whatever order the user pressed
shift-enter, so the notebook's text is not the program that ran, and a notebook
can be in a state no linear execution can reach. `nbconvert --execute` and
`papermill` exist because of this.

This matters here because F1 brings the interactive tier and `script-mode.md`
owns the entry rules. **The lesson to take is that an interactive session must be
able to emit the linear program that would reproduce it**, and that the cell
execution order must be recoverable. That is an ask on the interactive-tier note
before it is written, and it is cheap then and impossible later (§9 ask 7).

### 6.5 JAX — the honest comparison, and where Science is actually stronger

`Key` is modelled on JAX's design and the note says so. JAX got the hard part
right: splittable, counter-based, no global state, explicit threading. Three
things Science adds, in decreasing confidence:

1. **A JAX key can be reused and the language cannot tell.** `jax.random.normal(key,
   …)` twice with the same `key` is valid Python that silently returns the same
   numbers — the single best-known JAX footgun. In Science it is `SC0301`, use
   after move, and it is a compile error because the key is neither `Copy` nor
   `Clone`. **This is the one place Science is strictly stronger than the best
   existing design in the field**, and it is exactly what §1 of the core spec
   claims when it names *"random keys that cannot be reused"*. It is not a
   marketing claim; it falls out of ownership. `effects.md` §9.1 reaches the same
   verdict independently — *"this is the one comparison Science wins outright, and
   it wins it with ownership (`SC0301` on a reused key) plus one static bit, not
   with an effect system"* — which is worth recording because it is the only place
   in either note where a competitor is beaten rather than traded against.
2. **JAX's compilation is not bit-stable and the artefact does not say so.** XLA
   fuses differently across versions and backends, and the numbers move. There is
   no record in the output of which jaxlib, which XLA, which backend. Science's
   `[build]` table and `.science.provenance` are that record.
3. **JAX inherits Python's environment problem entirely.** The key design is
   excellent and it sits on top of pip.

Against Science: JAX has the ecosystem, the GPU story and thousands of users, and
Science has none of those. The comparison is about the design, not about which
one a scientist should use in 2026.

### 6.6 What survives, and the two counter-arguments that do not lose

**What survives.** All six are *outside the program*. Every one of them can be
perfectly correct while the program is nondeterministic, and none of them can
detect that it is. Science's contribution is the inside half — the program's own
determinism as a compiler-checked property — plus a record that joins the inside
half to the outside half in one artefact a reviewer can read without running
anything.

**Counter-argument 1: `--deterministic` is a flag, so it is a discipline too.**
Largely fair, and the "not optional" half of the claim has to be narrowed. What is
genuinely not optional:

- **There is no ambient generator.** It cannot be turned on, because it does not
  exist. `stdlib-standard.md` §5.3's design is structural.
- **A `Key` cannot be reused.** `SC0301` is not a flag.
- **`Map` has no hash-ordered iteration** to ask for.
- **`SC0237`** (seeded from the clock) fires by default, outside the mode.
- **`pure function`** is checked, and a library that asserts it cannot quietly
  acquire a clock read (`effects.md` Decision 5, `SC0216`).
- **Provenance is emitted by default**, into every binary and every stamped
  output.

Those five are on for everyone, and they cover the failures that actually happen.
The flag buys the last mile. So the accurate statement is not "the language
guarantees reproducibility" but:

> **Science moves reproducibility from entirely a discipline to mostly
> structural, with a flag for the last mile.**

Smaller than the pitch, still a claim nobody else can make.

**Counter-argument 2: the audience may want the artefact more than the mode.**
Journal and funder requirements are about *availability* — deposit the code and
the data — which a Zenodo DOI and a Docker image satisfy on paper. Very few
reviewers rerun anything. If that is the real market, then §5 is the valuable
half and §4 is engineering that mostly does not get used.

This note takes that seriously and does not dismiss it. The answer is that the
two are not independent: **the artefact is only worth depositing if the thing it
describes is reproducible.** A provenance record attached to a program that reads
the clock and shuffles a `Map` is a precise description of something that will
not come back. §5 is the sellable half and §4 is what makes §5 worth trusting,
and if only one ships, ship §5 first — §7's sequencing does exactly that.

---

## 7. Cost, and sequencing

| Item | Cost | Phase |
|---|---|---|
| Decision 3 — float policy, `fma` as a function | Days of compiler work; an ongoing benchmark deficit against toolchains that contract by default | **F0** — it is a one-way door; changing it later changes every number |
| Decision 2 — frozen `erfinv` and friends in `random`, with test vectors | Low single weeks | **F0** — same reason; `stdlib-standard.md` §5.5's vectors ship from the first commit |
| G8 — build-twice-compare-hashes in CI | Hours | **F0** — trivial now, archaeology later |
| G7, G9, G6 — sort `list_directory`, correctly-rounded parse, `Hash` not observable | Days total | **F0** |
| `SC0237` — clock-seeded key | Days, on top of a bit `effects.md` computes anyway | **F0/F1** |
| Decision 10–12 — the record, canonical JSON, the id, the stamping | 3–4 weeks, most of it already costed by `native-dependencies.md` Decision 9 and `data-io.md`'s writers | **F1** |
| Decision 13 — `sciencec verify` | 1–2 weeks on top | **F1** |
| Decision 6 — input digests | Days; the policy question is the work | **F1** |
| Decision 7–9 — `--deterministic`: the policy, the supplementary list, the allow-list, four diagnostics | **1–2 weeks**, because `effects.md` §3.5 and §10 item 1 build the analysis in F0 for F5's replay policy regardless | **F1**, behind the `ambient` bit |
| §4.4 — information flow, effect polymorphism | Not this note's, and not asked for | **Not scheduled** |

The ordering is deliberate and it is the answer to §6.6's second counter-argument:
**the one-way doors are in F0, the artefact is F1, and the enforcing mode is F1
after it.** Nothing here blocks F0's critical path; the F0 items are three
decisions and four small fixes, and every one of them is much cheaper today than
after a single published result exists.

Costs not in the table, stated because they are the ones that get forgotten:

- **A conformance suite** — cross-platform bit-identity CI (x86-64 with and
  without AVX-512, aarch64, macOS, Windows), the `Key` vectors, the `erfinv`
  vectors, and a build-twice check. Ongoing, and the rule in §10 is that a claim
  not covered by a test in it is a claim the documentation does not make.
- **Editorial cost.** Every statement of the claim carries its scope, forever.
- **Documentation surface.** Two build profiles (`--verified`,
  `--deterministic`) plus `--locked`, a seed list, a supplementary list, an
  allow-list and two record shapes. That is a lot of concepts for a language that
  is otherwise proud of removing them, and §4.2's supplementary list is the one
  that is hardest to justify to somebody who has just read `effects.md` §3.2.

---

## 8. Is this a second bet, a corollary, or a distraction?

**A corollary of the first bet, and a second pitch.**

**Why it is a corollary and not a second bet.** The test is whether it asks for a
new language mechanism. It does not. §1 of the core spec names *"random keys that
cannot be reused"* as one of three targets of the verification bet, and that
target *is* a reproducibility property — it is delivered by ownership and regions,
which F0 is building for other reasons. Everything in this note is either (a) a
library decision made at the right weight, (b) a build profile over a reachability
query the compiler's call graph already supports, or (c) a record written into a
section. **It asks for no language-level feature at all** — the earlier draft's
one request, a reserved effect name, was answered before this note landed, by an
`ambient` bit that `effects.md` is building for F5's replay policy. A bet costs a
mechanism; this costs discipline applied to mechanisms already bought.

**And a second note reached the same conclusion from the other end, on the same
day.** `effects.md` §9.2 tests §1 of the core spec's *"an effect discipline"*
against Koka, Haskell, Rust, Mojo and JAX, concludes that Science loses that
comparison to four languages, and recommends replacing the phrase with
**"compiler-checked reproducibility"** — *"a better claim anyway, because it is
the one the audience actually wants and the one they can test."* That note started
from a mechanism and this one started from a claim, and they met. Two independent
derivations is the strongest evidence available in a corpus where every note is
written in parallel, and it is the reason this section's verdict is stated with
confidence rather than as a suggestion.

**Why it is nonetheless a second pitch, and the better one.** "Catches your
mistakes before the run starts" is a claim about a *programmer*. "Tells you what
produced this number, and gives you the same answer from the same inputs" is a
claim about a *result*. The audience's institutional pressure is on the second:
journals and funders are adding artefact requirements, and nobody's toolchain
delivers them from inside the language. The verification pitch competes directly
with Julia, Mojo and JAX on ground they also occupy; the reproducibility pitch
does not compete with anybody, because nobody is standing there.

**The recommendation.** Verification stays the headline in the engineering
material — it is what the compiler is. Reproducibility becomes the headline in the
audience-facing material, with §2.2's two sentences as the fixed wording. And §1
of the core spec takes `effects.md`'s amendment: *"static shapes,
compiler-checked reproducibility, ownership-tracked device memory, an interactive
tier, and zero-copy Python interop"*. This note seconds that edit rather than
proposing a different one (§9 ask 1), which also means the five-part claim gains
this note's subject without gaining a sixth part.

**Where it becomes a distraction.** Exactly one place: if §4's mode is allowed to
turn into an effect-system project. The moment `--deterministic` requires effects
to ship, it is blocking on F2 and competing with the shape checker for the same
people. Decision 7 exists to prevent that, and the risk is now much lower than it
was twelve hours ago: `effects.md` builds the analysis for F5's benefit and keeps
it to three inferred bits, so §4 is a week of policy rather than a research
project. If that ever inverts — if the mode starts needing information flow to be
usable — the mode should be cut and §5 shipped alone.

---

## 9. What this note asks of the others

1. **The core spec §1** — **seconded, not re-proposed.** `effects.md` §11 ask 1
   already asks for *"an effect discipline"* to become *"compiler-checked
   reproducibility"*. This note supports that edit without amendment and asks for
   one thing more: that §1's *"random keys that cannot be reused"* be identified
   as the **first instance** of that property rather than only as a verification
   target, with a pointer here for what the property is bounded to (§2.2). Two
   notes now ask for the same sentence to change; it should change once.
2. **`stdlib-core.md` §8.3 and §12** — the 1-ULP contract is correct for `math`
   and is a contradiction where `random` depends on it (G1). That section already
   flags this as *"the one place where Level 1 makes a promise it cannot fully
   keep"* and as the most likely thing in the note to be found wrong; this is the
   specific way it is wrong. Decision 2 is the proposed resolution and it does not
   require `math` to change. Also §3.5: **`Hash`'s output must be unobservable**
   (G6), which turns that section's coincidence into an invariant.
3. **`stdlib-standard.md` §5.5** — add the inverse-CDF implementation and its
   polynomial to the frozen contract, and ship test vectors for `normal` and not
   only for `uniform` and `split`. Add `time.now`, `Monotonic.now` and
   `Stream.from_entropy` as `ambient` seeds by reference to `effects.md` §3.1.
   **And §10 needs a
   reproducibility paragraph**: the `thread` surface is specified without one
   (G4), while `.parallel()` has a careful one.
4. **`data-io.md`** — four things, in order of value. **(a)** Stamp the run record
   by default, which is `native-dependencies.md` §10's ask of that note, picked up
   here with the per-format carriers of Decision 12 including the CSV/JSONL
   sidecar. **(b)** `list_directory` sorts in byte order, for the reason §7 gives
   `glob` (G7). **(c)** Text-to-`F64` is specified **correctly rounded**; the
   algorithm stays free (G9). **(d)** Readers record input digests per Decision 6.
5. **`package-manager.md` §2.3** — amend *"`--locked` is the single strict mode"*
   to *"the single strict mode over the environment"*, and name `--deterministic`
   as the orthogonal one over the program. §4.1 gives the reason and this is
   stated as a contradiction rather than assumed. Also: the `[build]` table gains
   `float` and `allow_ambient` (§5.2), and `sciencec archive` records
   both. A `SP` code is requested from the free pool (`SP0060`+) for **`sciencec
   verify` finding a stamped record that does not match this machine** — it has no
   span into a `.science` file and by §8.1's own reasoning belongs in `SP`.
6. **`native-dependencies.md` §5.4** — adopt canonical JSON and the provenance id
   (Decision 10), the build/run split (Decision 11), and the three new `[build]`
   fields. Its §13 open question — whether to walk the loader's transitive closure
   — should be answered *no* for F1 and *yes behind a flag* later; the record must
   stay readable in a terminal. Its `--provenance=minimal` extends to environment
   values (§5.3).
7. **`script-mode.md`, and the F1 interactive-tier note when it is written** —
   §6.4's Jupyter lesson. An interactive session must be able to emit the linear
   program that reproduces it, and cell execution order must be recoverable. Cheap
   to design in, impossible to retrofit.
8. **The compiler, and CI** — build twice, compare hashes, from the first commit
   (G8); and a cross-platform bit-identity matrix that the `Key` and `erfinv`
   vectors run in. `package-manager.md` Decision 7's claim is untested without it.
9. **`effects.md`** — four things, none of which reopens a decision. **(a)** Its
   §11 ask 9 is answered: a reproducible build **forbids** `ambient`, and the
   reasoning is §4.1. `SC0217` is accepted as offered and this note claims no code
   for it. **(b)** Add to §3.1's `ambient` seed list: `Monotonic.since`, and the
   thread primitives whose *order* the machine chooses — `Receiver.receive`,
   `Sender.send`, `Mutex.with_lock`, `RwLock.with_read`, `RwLock.with_write`,
   `Atomic`'s read-modify-write operations, `Barrier.wait` (G4). Its §3.1 prose
   already says `ambient` covers *"the scheduler"*; this is that sentence made
   mechanical, and it does not reopen Decision 1's refusal of a `thread` effect,
   which is about values crossing threads rather than about observable order.
   **(c)** Record in §3.2 that the `ambient`/`external` split has one limit: `net`,
   `http` and `os.run` are `external` and are **not** reproducible in the way
   `read_file` is, because the bytes are not in any artefact. §4.2 handles this in
   a build profile rather than asking for a fourth bit, and that note's §3.2 should
   say so, since a reader of it alone would conclude `http.get` is safe in a
   reproducible run. **(d)** Noted, not asked: its earlier-drafted `SC0214`–`SC0229`
   claim and this note's withdrawn one were a live collision of the exact kind the
   README's allocation section exists to catch, found within an hour because both
   notes wrote their claim down.
10. **`indexing-and-array-literals.md` §8 ask 9** — answered: yes, `--verified` is
    planned, as one of a family with `--deterministic`, sharing the
    warning-becomes-error shape. `SC0286` is not a warning forever.
11. **`README.md`** — a row for this note (`reproducibility.md`, "the
    reproducibility claim, its bounds, `--deterministic`, the provenance record",
    F0 / F1) and `SC0237`–`SC0240` in the Resolution column. This note does not
    edit the README because two other agents are writing there now. Three other
    notes are queued with the same ask; whoever lands them should land all four
    rows in one edit.

---

## 10. Risks

**The scope gets dropped from the claim, and then the claim fails in public.**
The top risk, by a distance. Sentence 1 of Decision 1 is fourteen words longer
than "Science is reproducible", and the shorter version is what ends up on a
slide. The first time somebody reruns on a different cluster and gets a different
χ², the language is the thing that was wrong. Mitigation: the two sentences are
fixed text, the conformance suite gates them, and **a claim not covered by a test
in the suite is not made in the documentation** — which is a rule somebody has to
enforce, and rules like that decay.

**G1 and G2 are the specific way it fails.** If `normal()` ships on a 1-ULP
`erfinv`, the random stream — the one thing the language says is bit-exact
forever — is not, and it is not on the machine a reviewer is most likely to use.
This is not hypothetical; it is the current state of the design as two notes
wrote it.

**The mode over-rejects and gets turned off.** Reachability cannot see that an
elapsed time never reaches an output, so an honest program with a progress bar is
rejected — and §4.3 establishes that no sink exemption can fix this, because the
bit propagates from the seed rather than to the sink. The allow-list is the only
mitigation, which is more weight than an allow-list should carry. The failure mode
is that a lab turns `--deterministic` on, hits three
`SC0217`s in a dependency they did not write, and turns it off permanently. The
ETA case of §4.3 is the specific first encounter.

**The allow-list rots.** Every ignore mechanism in every language has. `*` being
unaccepted, `SC0217` on stale entries, and the archive printing the list are the
defences, and the third is the only one with a track record.

**Provenance becomes ceremony nobody reads.** 4 KB of JSON in every output file
that no reviewer ever opens. The mitigation is `sciencec verify` — a record is
read when a tool reads it, not when a human is asked to — which is why Decision
12's second rung matters more than its first.

**The compiler is not deterministic and nobody notices for a year.** G8 is one
CI job and it is the kind of job that gets written after the first time it would
have helped.

**The audience may want the artefact and not the mode**, in which case §4 is
2–3 weeks spent on something few people turn on. §7's sequencing hedges this by
putting §5 first, and §6.6 argues the two are not separable in value even if they
are in effort. That argument could be wrong.

**One more note is now in the reproducibility business, and there are six.**
`stdlib-standard.md` §5, `native-dependencies.md` §5, `package-manager.md` §2,
`collections-and-chains.md` §7.1 and now `effects.md` §3 each own a piece and each
is correct. This note claims the *claim* and not the pieces, which is the only
division that does not create a second copy — but it means seven notes must stay
consistent about what is promised, and the README's own history is four invisible
collisions among five notes written in one day. This note and `effects.md` were
the fifth and sixth: they collided on `SC0214`–`SC0221` and converged on the
verdict, within the same hour, and neither could see the other when it started.
The collision was cheap to fix only because both wrote their allocation down
before writing anything else, which is the README's rule and is the argument for
it.
