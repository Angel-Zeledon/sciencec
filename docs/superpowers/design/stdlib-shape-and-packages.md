# Science — Design: the shape of the standard library, and the policy it lives under

Date: 2026-09-16
Status: draft for review
Owns: the batteries-included question; what the error model does to every
library signature; the concurrency model the library is written against; naming
conventions and the reserved-word collision list; the Level 3 package set; and
what versioning can mean with no package manager.
Siblings: `stdlib-core.md` (Level 1) and `stdlib-standard.md` (Level 2)
catalogue the modules. **This note decides the policy they live under and should
be read before either of them.** `stdlib-core.md` landed during the writing of
this one; §2.4, §5.5, §5.6, §5.7 and §7 here record where the two converged independently,
where this note defers to it, and the one diagnostic collision it caused.
Related and depended on: `syntax-revision-2.md` §3 (the error model, already
decided), `reserved-words.md` in full (§4 here builds directly on it),
`collections-and-chains.md` §7 (parallelism), `ffi-c-boundary.md` §4.4, §5.2 and
§9 (reentrancy, optional libraries, linking), `python-interop.md` §5 (the GIL),
`scientific-libraries.md` §2 and §15 (link-or-write, and the missing package
manager), `data-io.md` §2–§3 (what ships with the toolchain).
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §8, whose
closed list this note gives a growth policy; §12, whose "no package manager"
this note prices rather than repeats; §13, to which §4 here adds eleven
collisions the scientific audit did not reach.

---

## 0. What this note decides, and why it has to come first

Three of the four decisions below are not library decisions. They are language
decisions that only become visible when somebody tries to write a library.

- **Whether the distribution is batteries-included** decides whether `stdlib-core`
  and `stdlib-standard` are catalogues of what exists or wish-lists of what
  someone might publish. With no package manager (§12 of the core spec) those are
  not nearly the same document.
- **The error model** was settled in `syntax-revision-2.md` §3, and settling it
  was a precondition for writing a single signature, because it decides the
  return type of every function in the language that can fail. §2 here does not
  reopen it; it states the consequences, which is the part that was left
  undone.
- **Concurrency** decides whether there is one `read_file` or two. Python shipped
  `asyncio` after the standard library was written and has been carrying two
  standard libraries ever since. This is the single most expensive thing to
  retrofit and the single cheapest thing to decide early.
- **Naming** decides whether the library can spell its own nouns. Today a
  statistics module cannot call a fitted model a `model`
  (`scientific-libraries.md` §3) and a chain cannot call a predicate scan `any`
  (`collections-and-chains.md` §1.4 spells it `has_any` for exactly this
  reason). Those are not cosmetic; they are the library working around the
  language.

The through-line is that Science has **no package manager**, and every decision
here is different because of it. That absence is not a footnote in this note; it
is the load-bearing fact, and §6 says plainly that it is also the weakest joint
in the whole three-note stdlib design.

---

## 1. Batteries included, or a minimal core plus an ecosystem

### 1.1 The levels, and what defines a boundary

The author's three-level structure is adopted, with the boundaries redefined.
The levels are **not** graded by usefulness or by how "standard" something feels.
They are graded by **distributability**: what a user has to do to get the module.

| Level | What it is | How a user gets it | Versioned with |
|---|---|---|---|
| **1 — core** | Linked into every binary by `science-rt`; §8's closed list plus what `data-io.md` §2 adds. Always present, cannot be absent, cannot be swapped. | Nothing. It is the language. | The language |
| **2 — standard** | Ships in the toolchain tarball, `use`-able with no install step, but not linked unless used. `math`, `data.*`, `text`, `time`, `os`, `test`. | Nothing. It is in the tarball. | The toolchain |
| **3 — official packages** | Written and maintained by the same team, shipped as separate artefacts, not present in a minimal toolchain. | Today: an extra directory on `SCIENCE_PATH` (§6). Eventually: the package manager that does not exist. | Themselves, with the caveats of §6 |

The definition has a consequence worth stating before the argument: **Level 3 is
today the level whose distribution story does not exist.** Everything in §5 is
predicated on §6, and §6's honest answer is uncomfortable.
`stdlib-standard.md` §0.1 reached the same conclusion independently and states it
in its opening section, which is the right place for it.

**Reconciling the numbering, since three notes now use two schemes.**
`stdlib-core.md` and this note number the levels 1–3. `stdlib-standard.md` §0.1
numbers them 0–3 and inserts a tier this note does not have. The mapping:

| This note / `stdlib-core.md` | `stdlib-standard.md` §0.1 |
|---|---|
| Level 1 — core, no `use` | Level 0 — the core |
| Level 2 — toolchain, with `use` | Levels 1 and 2 — "language-adjacent" and "toolchain" |
| Level 3 — official package | Level 3 — package |

The author's three names are kept, because they are the ones the brief and two of
the three notes use. But **`stdlib-standard.md`'s extra distinction is real and is
adopted here as a property rather than a level**: some Level 2 modules are
*compiler-known* — `ffi` (`ffi-c-boundary.md` §1.3), `data` and `data.*`, and the
`test` item form — meaning the compiler's own rules name them, so they can never
be replaced by a package even after §6's package manager exists. A
compiler-known module is versioned with the language, not with the toolchain, and
must be marked as such in its own documentation. That is one row in a table, not
a fourth level, and it captures everything the four-level scheme was buying.

### 1.2 Decision

> **Decision 1. Science is batteries-included at Levels 1 and 2, aggressively so
> by Rust's standards and about as far as Go's, and the batteries are chosen by
> one test: *is this something a scientist cannot start work without, on a
> machine where they cannot install anything?***

**Rejected: a minimal core plus an ecosystem (Rust's model).**

The reason is not that the model is worse. It is better, and `rand`, `serde`,
`regex` and `rayon` are all better than anything a standard library committee
would have produced. The reason is that the model has a prerequisite Science
does not meet.

Rust's minimal core works because `cargo add serde` works. The standard library
can be small precisely because the gap is one command wide. §12 of the core spec
puts the package manager out of scope for F0, and `scientific-libraries.md` §15
names its absence as "the seam where its absence will hurt most". In a language
with no package manager, a minimal core is not a choice between a small standard
library and a large ecosystem. It is a choice between a small standard library
and **nothing**.

Three further facts from this language's specific situation, each of which is
not a general principle and does not transfer:

**The audience arrives from Python, where `import json` works and nobody thinks
about it.** §1 of the core spec names the audience: people currently using
Julia, Mojo, NumPy, PyTorch, JAX and pandas. Their calibration for "how much
comes with the language" is set by Python, and Python's answer is "a great
deal". A user who types `use data.json (parse)` and is told the language does
not have JSON will not conclude that Science has a tasteful minimal core. They
will conclude that Science is unfinished, and they will be right, because for
them it is.

**The install happens on a machine the user does not administer.** This is the
fact that distinguishes a scientific language from a systems language, and it is
worth spelling out because it is the whole argument. A scientist's Science code
runs on a cluster login node where they have no root, often no outbound network
from the compute nodes, a module system that manages software instead of a
package manager, a shared filesystem with a quota, and an administrator whose
answer to "can you install this" has a two-week latency. The Python ecosystem's
answer to that environment is a conda environment that takes forty minutes to
solve and breaks when the cluster's MPI is swapped. **Anything not in the
tarball the user `scp`s to the cluster effectively does not exist.** A large
Level 2 is the only form of "batteries" that survives contact with that
environment.

**With no package manager, the standard library is the only place a shared
abstraction can live.** If `Frame` is not standard, two libraries that both
define a data frame cannot interoperate, and no third party can fix that by
depending on both, because there is no dependency mechanism. This is the
argument that Go's standard library makes for `io.Reader` and `error`, and it is
much stronger here than it is in Go, because Go has a module system and Science
does not. Every type that appears in more than one library's signatures must be
at Level 1 or Level 2 or the ecosystem cannot be an ecosystem.

### 1.3 Against all that: what actually went wrong in Python and Go

The counter-argument is real and it should be stated at its strongest, because
the failure modes are famous and they are all in the same family.

- **`urllib2`** was not bad because it shipped. It was bad because it could never
  be removed, so Python spent fifteen years with two HTTP clients in the
  standard library and the good one (`requests`) outside it.
- **`asyncio`** was not bad because it shipped. It was bad because it shipped
  **before the design was settled**, so the standard library acquired a second
  colour for every I/O function, and a decade later `async` and sync are still
  two standard libraries wearing one name.
- **`distutils`** was not bad because it shipped. It was bad because it was
  load-bearing for a decade after everyone agreed it was wrong, and removing it
  in 3.12 broke packages that had been unmaintained since 2015.
- **Go's own team has publicly regretted parts of theirs** — most consistently
  the shape of `net/http`'s server-side API and the pre-generics container and
  `sort` interfaces, which the compatibility promise pins in place forever.

Read together, none of those is an argument against batteries. Every one of them
is an argument against **a battery that cannot be removed or replaced, or that
shipped before its design was settled**. So the rule that falls out is not "ship
less". It is:

> **Nothing enters Level 1 or Level 2 whose design is unsettled, and nothing
> enters at all without a removal path.**

That rule is the reason §3 of this note exists. Concurrency is exactly the
unsettled design that Python shipped anyway, and if the standard library is
written before the concurrency model is decided, Science acquires `asyncio`'s
problem by the same route Python did.

### 1.4 What "a removal path" means concretely

Three obligations, which are the whole price of Decision 1 and are cheap only
because they are adopted before anything is written.

1. **Every Level 2 module is `use`-gated and absent from a binary that does not
   name it.** A module nobody uses costs a directory in the tarball and nothing
   else. This is already how §8 and `data-io.md` §2 split the core's closed list
   from `data.*`, and it is why `data-io.md` §9 can say the base toolchain builds
   with no C++ compiler present.
2. **Every Level 2 module carries a stability marker in its own documentation:
   `stable`, `provisional`, or `deprecated`.** `provisional` means the API may
   change in the next toolchain release and the compiler says so at the `use`
   site. Python's mistake is entirely explained by the absence of this marker:
   `asyncio` was provisional in fact and stable by default.
3. **Deprecation is a compiler diagnostic with a named replacement and a named
   release, from the first deprecation, not from the first crisis.** For a
   language whose corpus is mostly machine-written (`llm-ergonomics.md`), a
   deprecation that is only in prose is a deprecation that does not exist: the
   model was trained on the old name and will keep emitting it until the
   compiler says otherwise.

### 1.5 The cost of Decision 1, stated plainly

- **Every module in the distribution is a compatibility obligation, forever.**
  This is the Go and Python bill and Science will pay it. §1.4's stability
  markers reduce it; nothing removes it.
- **The toolchain download grows**, and `data-io.md` §9's Arrow C++ dependency
  shows where that goes badly. The mitigation is already in that note: build
  features, so Parquet is in the tarball people ask for and not in the minimal
  one.
- **The compiler team owns a security response process** for anything in Levels
  1–3 with a CVE surface: zlib, zstd, Arrow C++, any TLS. A language team that
  ships a compression library has signed up to ship a patch release the week a
  zlib advisory lands, and nobody enjoys discovering that obligation
  retroactively.
- **A standard library is a graveyard of good-enough APIs.** The best third-party
  library for any given job will always be better than the standard one, and
  once the standard one exists, the better one has a smaller audience. Accepted,
  because the alternative here is not a better third-party library, it is no
  library.

---

## 2. What the error model does to every signature

`syntax-revision-2.md` §3 decided this: Go-style multiple returns, a nullable
error, `Error` as a one-method interface, explicit conversion with `From`
widening removed, and `SC0140` for an unchecked error. **This section does not
re-decide any of it.** It states what it does to the library, which is the part
that had to wait and the reason the error model had to be settled first.

### 2.1 A function that cannot fail returns `T`

> **Decision 2a. No stdlib function carries an error slot it does not use. A
> function that cannot fail returns `T`, and that is a contract, not an
> omission.**

```science
function length(self) -> Int:                      # cannot fail. No error slot.
function abs(x: F64) -> F64:                       # cannot fail.
function get(self, i: Int) -> T?:                  # can be absent, cannot fail.
function parse_int(text: borrowed String) -> (Int, ParseError?):   # can fail.
```

The distinction matters because the three shapes are genuinely three things and
Go blurs two of them:

| Outcome | Shape | Example |
|---|---|---|
| Always succeeds | `-> T` | `text.length()` |
| May legitimately have no answer | `-> T?` | `map.get(key)`, `chain.first()` |
| May fail, and the failure has a reason a caller can report | `-> (T, E?)` | `read_file(path)` |

**Absence is not failure.** A missing key is an answer; a disk error is not. The
temptation, once `(T, Error?)` exists, is to use it for both, and the result is
`if err?` blocks around lookups that cannot fail. `T?` stays the spelling for
absence, exactly as §3.1 defines it.

**The cost, stated.** Adding an error slot later is a breaking change to every
call site. So the infallibility of a stdlib function is a promise, and a wrong
promise is expensive. The rule the library follows: *if the operation can touch
the operating system, the network, a foreign library, or data the program did
not produce, it is fallible, even if today's implementation cannot fail.*
`String.to_uppercase()` is infallible forever. `Path.canonical()` is fallible
even where a naive implementation is pure string manipulation, because a real one
calls `realpath`.

**Rejected: a uniform `(T, Error?)` on everything, for consistency.** It would
make every signature look alike and every call site three lines. Go does not do
this and neither should Science.

### 2.2 The shape of a fallible constructor

This is where the model bites hardest, and it needs a rule rather than
case-by-case taste.

A constructor that fails has nothing sensible to put in the value slot. Go puts
a `nil` pointer there. Science has no null for a non-nullable type, so there are
exactly two options:

```science
# Option A — a zero value in the value slot
function open(path: borrowed Path) -> (File, IoError?)

# Option B — the value slot is nullable
function open(path: borrowed Path) -> (File?, IoError?)
```

> **Decision 2b. The value slot is `T?` exactly when the type has no safe zero
> value, which is precisely the resource-handle case; otherwise it is `T`. The
> pair carries the invariant that exactly one of the two slots is null, and the
> type checker knows it.**

Option A is rejected for handles. A `File` with no file descriptor is the "use
of closed file" bug class, and it is worse in Science than in Go, because Go's
`nil` at least panics on use, where a zero-valued `File` would be a type the
compiler considers perfectly good. A language whose thesis in §1 is "mistakes the
compiler catches before the run starts" should not manufacture a family of
values that are valid to the type system and invalid in fact.

Option A is *kept* where a zero value is real and useful — `Frame.empty()`,
`Array.new()`, `String.new()` — because for those the value slot after a failure
is a correct empty thing, not a trap. `syntax-revision-2.md` §3's own example
does exactly this, returning `Config.empty()` beside the error.

**The ergonomic problem, and the ask it generates.** Option B costs a second
check that the programmer knows is unnecessary:

```science
let file, err be File.open(path)
if err?:
    return (Report.empty(), err)
# `file` is still `File?` here, although it cannot be null.
```

§3.1 of `syntax-revision-2.md` already specifies flow-sensitive narrowing for
`if err?:`. What it does not specify is that narrowing the error slot narrows its
**companion**. This note asks for that and calls it **paired narrowing**: when a
single `let` destructures a call whose return type is a declared fallible pair,
proving one slot null narrows the other to non-null on that path. It is one rule
in the same analysis that already exists, it is what makes Decision 2b bearable,
and without it Option B is unusable and the library is pushed back to zero
values and the bug class they carry.

The companion diagnostic is `SC0276`: the value slot of a fallible pair used on a
path where the error was not tested. It is complementary to `SC0140`, not a
duplicate — `SC0140` fires at function exit for an error never tested at all,
`SC0276` fires at the use site of the value.

### 2.3 `panic` survives, and this is what it is for

> **Decision 2c. `panic` stays, it aborts, and it is reserved for broken
> invariants and impossible states — never for a failure caused by data from
> outside the program. No stdlib function panics on input.**

`panic` has to survive: §8 of the core spec makes it a free function,
`ffi-c-boundary.md` §4.5 depends on it aborting rather than unwinding (which is
what makes the callback boundary sound with no machinery at all), and
`indexing-and-array-literals.md` §1.2 already decided that an undischargeable
index obligation lowers to a runtime check that panics and is *declared*
(`SC0286`).

The line the library draws:

| Panics | Returns an error |
|---|---|
| `a[i]` out of range — a program bug, and `SC0286` warned about it | A malformed CSV row |
| Integer division by zero | A file that does not exist |
| An invariant in `science-rt` that cannot be false | A foreign library returning a status code |
| Reaching a state the type system says is unreachable | A number that will not parse |

**`.unwrap()` does not exist.** Rust's `unwrap` is the single most-copied bad
habit from that ecosystem, and in a language whose corpus is mostly written by a
model that has read a great deal of Rust, a method with that name would be
emitted constantly. Instead:

```science
let port be config.get("port").or_else(8080)          # supply a default
let key be secrets.get("key").expect_present("startup checked this")
```

`expect_present` takes a message that must state *why the absence is impossible*,
and its documentation says so. It is an assertion wearing an accessor's clothes,
and naming it that way is the whole mitigation. This sits beside
`reserved-words.md` §4.2's recommendation that `assert` be kept and specified as
a language construct rather than a library function; when that construct lands,
`expect_present` is the library-shaped sibling of it and should be documented
next to it.

### 2.4 How errors compose across module boundaries now that `From` is gone

`syntax-revision-2.md` §3.4 removed implicit widening and said the caller writes
the conversion. For a three-layer library call — `stats` over `linalg` over BLAS
— that means a conversion at every layer, written out. Left unmanaged, that is
the worst part of the Go model: hand-rolled `fmt.Errorf("reading config: %w",
err)` in ten thousand shapes.

> **Decision 2d. Every stdlib module exposes exactly one concrete error type, a
> `choice`, named `<Module>Error`, and that type owns a named constructor for
> each layer it sits above. The conversion a caller must write is therefore
> always a named function on the target type, never an ad-hoc wrap.**

```science
choice CsvError:
    Io(IoError)
    BadRow(Int, String)
    BadHeader(String)

CsvError has:
    function from_io(e: IoError) -> CsvError:
        CsvError.Io(e)

CsvError implements Error:
    function message(self) -> String:
        match self:
            CsvError.Io(e): f"reading the file: {e.message()}"
            CsvError.BadRow(n, why): f"row {n}: {why}"
            CsvError.BadHeader(why): f"header: {why}"
```

and the caller's conversion is one line with a name in it:

```science
function load(path: borrowed Path) -> (Frame of Row, CsvError?):
    let text, err be read_file(path)
    if err?:
        return (Frame.empty(), CsvError.from_io(err))
    ...
```

Three rules make this hold together:

1. **A module's public signatures name that module's concrete error type, never
   `Error?`.** The concrete form is what keeps `match` exhaustive, which §3.4
   already identifies as the reason to prefer it, and it is what lets a caller
   distinguish "the file was missing" from "row 412 was malformed" without string
   matching.
2. **`Error?` — the boxed `any Error` — appears only where a signature is generic
   over its failure**: application `main`, a callback contract, a plugin
   boundary. §3.4's cost note applies: `any Error` allocates, and a function that
   returns it on every iteration of a tight loop has chosen wrong.
3. **Cause chains exist and do not add a *required* method to `Error`.**
   `stdlib-core.md` §7.5 settles this, landing while this note was being written
   and reaching a better answer than the one drafted here: `Error` gains a
   **defaulted** `cause(self) -> (any Error)?` returning `null`, so it remains a
   one-required-method interface and §3.4 is respected. This note adopts that
   and withdraws its own proposal of a separate `Caused` interface, which would
   have meant two interfaces where a default does the job.

Two of the three rules above were reached independently by `stdlib-core.md` §7.3
and §7.6 — "a library exports a concrete error type, an application uses
`Error?`", with `from_*` associated functions as the conversion convention. They
are stated here as policy for Levels 2 and 3, there as the Level 1 enumeration,
and they agree; where the wording differs, `stdlib-core.md` is the one that
enumerates and should be followed.

A fourth rule is enforced by the compiler rather than by convention. Because
widening is gone, a `return (x, err)` where `err`'s concrete type is not the
signature's error type is a type error, and its message must name the conversion
the author should write, because for a library with the shape above there is
always exactly one. **That diagnostic is `stdlib-core.md` §7.7's `SC0259`**; this
note drafted the same check independently as `SC0278` and withdraws it (§7).

### 2.5 The five rules, in one place

Everything above, as the checklist `stdlib-core.md` and `stdlib-standard.md`
should apply to each signature they write:

1. Cannot fail → `-> T`. May be absent → `-> T?`. May fail → `-> (T, E?)`.
2. The value slot is `T?` iff the type has no safe zero value.
3. `E` is the module's one concrete `choice`, not `Error`, in any public
   signature.
4. Panics only on program bugs, never on input.
5. No conversion is implicit; every module provides the named constructors that
   make the explicit conversion one call.

---

## 3. Concurrency

### 3.1 Decision

> **Decision 3. Science gets OS threads and structured data parallelism, and no
> `async`/`await`. There is exactly one spelling of every I/O function in the
> standard library, and it blocks. `async` and `await` stay reserved words and
> are not implemented.**
>
> Concretely: **F2** brings data parallelism, which
> `collections-and-chains.md` §7 has already designed as `.parallel()` on a
> chain. **F3** brings message passing, which the core spec already schedules as
> actors — `spawn`, `send`, `receive`, supervision. A `concurrency` module with
> classic primitives sits under both and is deliberately small and deliberately
> unfashionable.

**`stdlib-standard.md` §10 landed while this was being written and reaches the
same decision, with the mechanics worked out further than they are here.** It
owns the module: `concurrency` in F2, scoped threads as the primary API,
`Share`/`ShareBorrowed` as the marker interfaces, sequentially-consistent atomics
only, and the diagnostics `SC0266`–`SC0270`. **Where the two differ, that note
governs the surface and this one governs the policy**; §3.3 and §3.6 below are
amended to its names and its API, and the arguments that remain distinct to this
note are the ones for *rejecting* the alternatives — colouring (§3.2), the
region interaction (§3.3), the foreign-call tax (§3.1) and the GIL (§3.5).

**Rejected: async/await with a runtime.** Reasons in §3.2–§3.5; the decisive
ones are the region interaction (§3.3) and the FFI interaction (§3.4), both of
which are specific to this language.

**Rejected: both.** "Both" is the choice Python made and it is the one with the
worst outcome, because it is the one that produces two standard libraries. In a
language with no package manager, where §1 has just committed to a large Level 2,
two standard libraries is not an abstraction cost, it is a doubling of the thing
§1.5 already named as the main bill.

**Rejected: green threads with a scheduler (Go's model).** This one deserves
more than a line, because it is genuinely attractive: Go gets colourless
concurrency, blocking-looking code, and no `async` keyword — apparently
everything this decision wants. It is rejected for a reason that is entirely
specific to Science: **Go's model requires the runtime to own every blocking
call and to move goroutines between stacks, and that is exactly what makes cgo
expensive.** A Go program pays a stack switch and scheduler bookkeeping at every
foreign call. `ffi-c-boundary.md` §0's opening argument is that essentially every
floating-point operation that matters already lives in a C or Fortran library, so
Science's hot path *is* the foreign call. A concurrency model whose central tax
falls on the foreign call is disqualified by the language's own premise. OS
threads have no such tax: a foreign call from a Science thread is a call
instruction.

### 3.2 Function colouring, and whether a numeric language needs any of this

Colouring is the real cost of async and it is usually described too gently. In
its concrete form it is this: `async` functions can only be called from `async`
functions, so the moment one exists, the standard library splits.

The evidence is not theoretical. Rust has `std::fs::read` and `tokio::fs::read`,
`std::sync::Mutex` and `tokio::sync::Mutex`, and a decade of "which one do I
use". Python has `open` and `aiofiles`, `requests` and `httpx`, `time.sleep` and
`asyncio.sleep`, and a `run_in_executor` escape hatch that exists solely to call
the other colour. C# has the `.Result` deadlock. Every language that added async
after its standard library ended up with two of everything.

For Science, ask what the second colour would buy. Async's entire value is
**holding many blocked operations cheaply** — ten thousand sockets, a web server,
a proxy. Now look at what this audience's programs actually do:

- a matrix multiply — CPU-bound, no blocking, async is irrelevant;
- reading a 40 GB Parquet file — one blocking call in a loop, a thread is fine;
- fitting a model — CPU-bound;
- a parameter sweep over 512 configurations — *parallelism*, not concurrency;
- an MPI rank exchanging halos — blocking, and MPI has its own progress engine;
- loading weights and running inference — CPU/GPU bound.

There is one real async-shaped workload in the whole audience, and it is
fetching many URLs to build a dataset. That is served adequately by a thread
pool with a bounded queue, which is a Level 2 module of a few hundred lines, and
served badly-but-adequately is the correct trade against splitting the standard
library in two.

> The strong claim in the brief is correct: **for a scientific language, data
> parallelism matters more than concurrency.** The programs are CPU-bound, the
> machines have 64 to 256 cores, and the thing users want is for all of them to
> be busy. `collections-and-chains.md` §7 has already designed that, down to the
> seven properties F0 must hold for `.parallel()` to be addable in F2 without a
> redesign. Async would contribute nothing to it.

### 3.3 The interaction with region inference, which is decisive

§3 of the core spec commits to **full region inference with no annotations**, and
§6 states flatly: "There is no lifetime syntax. The programmer never writes a
region." §6.4 exposes `borrows_live_at(point)` for F5, so that no borrow survives
a suspension point.

Those two facts together are what kills async, in two steps.

**First: async makes every `await` a suspension point, and pulls F5's machinery
forward to F2.** §2 of the core spec schedules `borrows_live_at` for F5
(durability), where agent bodies are lowered to serializable state machines.
Async in F2 needs the same query at every `await`, which means the region engine
must answer an interprocedural liveness query three phases earlier than planned,
for a feature the audience did not ask for. That is a concrete, datable cost,
and it lands on the single most expensive component in the compiler — the one
§6.2 warns "must be rewritten entirely" if provenance is not built in from the
start.

**Second, and worse: a future that holds a borrow across an `await` is a
self-referential value, and Science has no way to say so.** This is the problem
Rust needed `Pin` for, and `Pin` is famously the hardest thing in that language.
Rust could express it at all only because it has explicit lifetime syntax and an
`Unpin` auto-trait. Science has rejected lifetime syntax by design. So a Science
future that borrows across a suspension point is a value whose internal
references must not be invalidated by a move, expressed in a language that has
no notation for internal references and whose ownership rule 2 says assigning
moves. There is no small version of the fix. Either the language grows
annotations — reversing a core decision — or futures may capture nothing
borrowed, which makes async unusable for the numeric code it was supposed to
serve.

By contrast, **threads need no new region machinery at all** — provided the join
is a program point the compiler can name, which is precisely what
`stdlib-standard.md` §10.3 secures by making **scoped threads the primary API**.
Inside a scope, every thread is joined before the scope returns, so the scope
body is a call and the captured borrows are arguments to it, which region
inference already handles. A detached thread, which has no such point, may
capture no borrows at all — the same rule, word for word, as
`ffi-c-boundary.md` §4.4's for registered callbacks, and that note observes
correctly that they are the same problem.

The one addition threads do need is a pair of marker interfaces. This note
drafted them as `Send` and `Share`; `stdlib-standard.md` §10.2 names them
**`Share`** (may be moved across a thread boundary) and **`ShareBorrowed`** (may
be borrowed across one), rejecting Rust's `Send`/`Sync` because `send` is
reserved for F3's actors and a marker named `Send` beside an actor primitive
named `send` makes a reader confidently wrong. That reasoning is right and this
note adopts those names, withdrawing its own. They are auto-derived structurally
and are a type-system addition rather than a region-engine addition. The
diagnostic is that note's `SC0266`, not a new one — see §7.

### 3.4 The interaction with the FFI, where the hole is

`ffi-c-boundary.md` §4.4 names reentrancy as an unresolved hole and closes most
of it with a local check. Threads make the residue worse in a way that must be
named rather than discovered:

**Most scientific C libraries are not thread-safe, and the ones that are are
conditionally so.** Reference LAPACK is thread-safe; some vendor builds are not.
FFTW's plan creation is not thread-safe while plan execution is. HDF5's default
build is not thread-safe at all, and its thread-safe build serialises everything
behind one global lock. GSL's workspaces are per-thread objects that the API does
nothing to enforce. This is not an edge case; it is the normal state of the
libraries `scientific-libraries.md` §2 commits to linking.

Three consequences, two of which are asks of the FFI note rather than decisions
here:

1. **`when available` must become thread-safe.** §5.2 lowers an optional library
   to a lazily-initialised table of function pointers populated on first use.
   With threads, "populated on first use" is a data race, and a benign-looking
   one that will work in testing. It must be a once-cell with an acquire/release
   pair, not a flag. This is a small correction and it is stated here rather than
   left silent because the current text would be implemented as written.
2. **An `extern` block should be able to declare its thread-safety**, so the
   compiler can reject a call from a parallel region or serialise it behind a
   runtime-held lock. Something like `library "hdf5" single threaded` on the
   block. This is FFI's territory and is requested in §8, not decided here.
   `stdlib-standard.md` §10.5 files the identical ask in its own §16, from the
   same reasoning; it is one request with two customers and should be built once.
3. **A callback that may be invoked from a foreign thread is stricter than §4.4's
   registered-callback rule.** §4.4 already says a callback outliving its call
   may capture no borrows. A callback that may be invoked from a thread Science
   did not create must additionally capture only `Share` values, and
   `science-rt` must be initialisable on a foreign thread before any Science
   code runs on it.

None of this argues against threads; all of it argues that the FFI boundary and
the concurrency model must be specified together, which is another reason this
decision could not wait for the library.

### 3.5 The interaction with Python and the GIL

`python-interop.md` §5 holds the GIL for anything that touches a `PyObject`,
releases it around the Science body, and derives the release from a one-bit
`python` effect. §5.4 already rejects a `python`-effecting closure inside a
parallel region (`SC0454`), and §5.5 requires `science-rt` to be thread-safe for
free-threaded builds.

Three things follow, and one of them is a correction to a reading of §8 of the
core spec.

**`science-rt` is already required to be thread-safe.** §8 says "no threads in
F0", and that is true of the *library surface*. But `python-interop.md` §5.5
requires a concurrent-safe allocator, no non-atomic global mutable state, and
call-scoped per-call state — because in a free-threaded CPython build, two Python
threads are routinely inside Science at once, whether or not Science has a thread
API. So the runtime must be designed for threads from F0 regardless of this
decision. Deciding for threads costs the runtime nothing it did not already owe.

**A Science-spawned thread that touches Python must acquire the GIL itself.** In
a GIL build, that serialises, and the honest statement is that Science threads
calling back into Python do not scale, full stop. The rule follows from §5.4
rather than adding to it: the `python` effect bit already rejects such a closure
inside a parallel region, and the same bit should reject it inside a
`Thread.start` body unless the body opens with an explicit acquire.

**Free-threaded builds make this better, not different.** Under PEP 703 the
serialisation goes away, `Py_mod_gil = Py_MOD_GIL_NOT_USED` becomes meaningful,
and Science threads become genuinely parallel inside a Python process. That is
the scenario where the thread decision pays off best, and it is another argument
against async: an async Science would have to reconcile its own scheduler with
CPython's thread state, and there is no good version of that.

### 3.6 What the concurrency surface actually is

Small, and deliberately so. Level 1 gets the marker interfaces `Share` and
`ShareBorrowed`; Level 2 gets the `concurrency` module, whose surface is
`stdlib-standard.md` §10's and is not restated here: `scope`, `Thread`, `Mutex`,
`RwLock`, `Channel` and a sequentially-consistent `Atomic of T`. Channels spell
their operations `channel.send(v)` and `channel.receive()` — member position, so
`reserved-words.md` §0.1's dot rule is what makes them legal, and `put`/`take` is
the fallback if it is refused. Threads start through `scope.start(…)`, never
through a free `spawn`, which keeps F3's reserved word available.

Three policy constraints this note adds to that surface, which are level and
scheduler questions rather than API questions:

- **There is no second thread pool, and none is exposed.** `.parallel()` (F2)
  owns the pool. Exposing a second one is how a program ends up with 256 threads
  on a 64-core node, which on a shared cluster is an administrative incident
  rather than a performance bug. `stdlib-standard.md` §1 reaches the same place
  from the criterion — `concurrency` passes it only because it has no scheduler.
- **The pool's size defaults to the *allocation*, not the machine.** On a cluster
  that means `SLURM_CPUS_PER_TASK`, `OMP_NUM_THREADS` or a cgroup quota before
  `os.cpu_count()`. A scientific runtime that reads the core count and ignores the
  batch scheduler is the single most common way shared nodes get oversubscribed,
  and it is a runtime decision, so it belongs in policy.
- **`Mutex` exposes a closure form, not a guard value.** A guard whose `Drop`
  releases the lock works, but it makes lock scope a region question and turns
  "held the guard too long" into a deadlock explained by a borrow error. The
  closure form makes the critical section lexical. Flagged for
  `stdlib-standard.md` §10 rather than imposed on it, since that note owns the
  type.

### 3.7 What is lost, plainly

- **Science will be a bad language for writing a network server**, and for any
  program whose shape is ten thousand mostly-idle connections. That is accepted.
  It is not this audience's program.
- **Fan-out I/O is more expensive than it would be.** Fetching a thousand URLs
  costs a thread pool with a bounded queue rather than a thousand cheap tasks.
  The memory cost is real: a thousand OS threads is several gigabytes of stack
  reservation, so the pool must be bounded and the standard client must be
  written against a bounded pool from the start.
- **If F3's actors are ever implemented over I/O multiplexing**, the multiplexing
  (`epoll`, `kqueue`, IOCP) lives inside `science-rt` behind a thread pool, and is
  never surfaced as a function colour. That is a commitment this note makes on
  F3's behalf and F3 should confirm or reject it.
- **The door is not welded shut.** `async` and `await` stay reserved (§4), so a
  later phase may add them. What this note asserts is that they must not be added
  *before* the standard library is written, because that is the sequence that
  produced `asyncio`.

---

## 4. Naming, and the complete reserved-word collision list

### 4.1 Conventions

> **Decision 4a.** The conventions, which are not novel and are chosen for
> exactly that reason:

| Kind | Convention | Examples |
|---|---|---|
| Functions and methods | `snake_case` | `read_file`, `to_uppercase`, `available_parallelism` |
| Types, interfaces, choice variants | `UpperCamelCase` | `String`, `CsvError`, `IoError.NotFound` |
| Constants | `SCREAMING_SNAKE_CASE` | `const PI be 3.14159…` — matches §4.4's `const WIDTH be 768` |
| Modules | `snake_case`, one lowercase word where possible | `math`, `linalg`, `data.csv`, `text.regex` |
| Generic parameters | one capital, or `UpperCamelCase` when it reads better | `T`, `Item`, `Error` |
| Fields | `snake_case` | `doc.title`, `frame.row_count` |
| Argument labels | `snake_case`, and part of the name (`collections-and-chains.md` §3.3) | `sorted(by: …)`, `open(path, with: …)` |

Four rules with teeth:

- **Acronyms are capitalised as words**: `CsvError`, `JsonValue`, `HttpClient`,
  `RsaKey` — never `CSVError`. The rule that forces it is `HTTPSProxy` versus
  `HttpsProxy`, and every style guide that allowed the first regretted it.
- **No abbreviations, except a closed list.** §4.1 of the core spec makes this a
  language principle, and the library must not undercut it. The closed list is
  the mathematical notation the audience reads as words: `abs`, `min`, `max`,
  `sqrt`, `exp`, `log`, `sin`, `cos`, `tan`, `mod`, `gcd`, `lcm`, `std`, `var`,
  `cov`, `fft`, `svd`, `lu`, `qr`. Everything else is spelled out —
  `length()`, not `len()`; `count()`, not `cnt()`; `configuration`, not `config`
  in a type name. Note that `collections-and-chains.md` §1.4 already applies this,
  spelling `minimum()`/`maximum()` for the chain terminals while `min`/`max`
  remain the scalar functions in `math`; that split is correct and should be
  documented rather than tidied.
- **A method that consumes `self` and one that borrows it do not share a name.**
  `sorted()` returns a new one; `sort()` mutates in place. The past participle is
  the non-mutating form throughout, which is the convention
  `collections-and-chains.md` §1.4 already uses.
- **A boolean question is `is_*`, `has_*` or `can_*`.** This is safe despite `is`
  and `has` being keywords, because `is_empty` is one identifier under the
  lexer's maximal munch and never two tokens. Worth writing down because it looks
  like a collision and is not.

### 4.2 May the standard library use words the user cannot?

> **Decision 4b. No. The standard library is written in the language its users
> write, with no privileged vocabulary, and every collision below is resolved by
> renaming rather than by exemption.**

**Rejected: a privileged stdlib** (Python's `__`-prefixed internals, Rust's
`#[rustc_*]`, C's reserved `_`-prefixed namespace). The reasons here are
specific:

1. **The standard library is the teaching corpus.** §1 of the core spec bets on
   verification and `llm-ergonomics.md` observes that diagnostics are the only
   teaching channel a language with no training corpus has. Both make the
   library's source the primary worked example. A worked example the reader
   cannot imitate is worse than no example.
2. **For a machine-written corpus it is actively harmful.** A model that reads
   `function mod(a, b)` in the standard library will emit `mod` in user code,
   where it will not compile, and the diagnostic will say the word is reserved —
   which is true, unhelpful, and contradicted by the library the model just read.
   This failure repeats forever.
3. **It removes the pressure that fixes the language.** Every collision in §4.4
   is evidence for `reserved-words.md`'s recommendations. A stdlib exemption
   would hide exactly the evidence that argues for freeing the words.

One thing that looks like an exemption and is not: the `extern` block is a small
separate grammar with contextual keywords (`library`, `symbol`, `when
available`), per `ffi-c-boundary.md` §9. Those are not names in the Science
namespace; they are tokens in a sublanguage, and they are available to any user
who writes an `extern` block. No privilege is claimed and none should be.

### 4.3 A fifth position: argument labels

`reserved-words.md` §1 classifies collisions into four positions — member,
binding, free function, module. Writing a general-purpose library surfaces a
fifth, and the taxonomy should grow to hold it.

**Argument labels.** `collections-and-chains.md` §3.3 makes labels part of the
method name — `sorted(by: key)`, `unique(by: key)` — and `ffi-c-boundary.md` §1.7
permits them at extern call sites. A label is a word immediately followed by `:`
inside a call's argument list, and the words a library wants there are short
prepositions: `by:`, `with:`, `in:`, `of:`, `on:`, `at:`, `to:`, `from:`,
`giving:`. Four of those are reserved today.

The dot rule does not reach this position. But the same argument applies
verbatim: **after an opening parenthesis or a comma inside an argument list, a
word followed by `:` can only be a label.** No keyword can begin anything there,
because no statement, expression or type may start with `word :` in argument
position. So:

> **This note asks for the *label rule*, the sibling of `reserved-words.md`
> §0.1's dot rule: in an argument list, a word immediately followed by `:` is a
> label, not a keyword.** It is the same five lines of parser, in the same
> function family, and it closes the fifth position outright.

With the label rule, `open(path, with: options)`, `train(net, on: data)` and
`integrate(f, of: x)` all work with no renaming, and construction syntax
(`Doc(title: "a")`) gains the same freedom for field names.

### 4.4 The consolidated collision table

Every word the brief names, plus the ones a general-purpose library hits that the
scientific audit did not. **Status** is against
`crates/science-lexer/src/token.rs` as it stands: `K` in use as a keyword, `R`
reserved-not-used, `F` freed by `reserved-words.md` §5 or
`syntax-revision-2.md` §7 (pending those notes landing).

| Word | Status | What wants it | Position | Dot rule? | Label rule? | Alternative if kept |
|---|---|---|---|---|---|---|
| `mod` | R → F | `math.mod(a, b)`, `n.mod(m)` | member, free fn | member only | — | `remainder` |
| `union` | R → F | `Set.union(other)`, `ffi` C unions | member | **yes** | — | `merged` |
| `yield` | R → F | reaction/crop/bond yield; a `Generator` type | binding, field | field only | — | `produced` |
| `kernel` | R | KDE, convolution, GPU kernel; `signal.kernel` | binding, parameter | no | — | `window`, `smoother` |
| `any` | K | `values.any(p)`, `any(mask)`, and `any Trait` | member, free fn | member only | — | `has_any` (chain), `any_of` (mask) — see §4.7 |
| `match` | K | `re.match(s)`, `Pattern.match` | member | **yes** | — | `find`, `matches` |
| `const` | K | a `const` module of `PI`, `C`, `H` | **module** | no | — | `constants` |
| `at` | K → F | `poly.at(x)`, `series.at(i)`, `at:` label | member, label | **yes** | **yes** | — |
| `import` | R → F | `data.import()`, `json.import` | member | **yes** | — | — |
| `assert` | R | `assert(cond)` in a `test` module | free fn | no | — | `check` — and keep `assert` reserved as a construct (`reserved-words.md` §4.2) |
| `shape` | R → F | `x.shape`, a `Shape` type | member, binding | member only | — | `dims` |
| `model` | R → F | `stats` fitted model, `model.load` (`models-and-inference.md`) | binding, **module** | no | — | `Fit`, `weights` |
| `tensor` | R → F | parameter and local names throughout array code | binding | no | — | `input`, `t` |
| `type` | K | `value.type()` on a JSON value; `mime.type`; `let type be …` | member, binding, field | member and field | — | **`kind`** — recommended throughout, because binding position stays broken |
| `is` | K | nothing bare; `is_empty`, `is_dir` are single identifiers | — | n/a | — | none needed |
| `in` | K | `range.in(x)`; **`stdin` as `io.in`**; `in:` label | member, binding, label | member | **yes** | `input`, `stdin`, `contains` |
| `not` | K | `Not.not(self)` — an operator-trait **method declaration** | member, **declaration** | member only | — | see §4.5: the method is `invert` |
| `and` | K | `BitAnd.and(self, other)` — same shape | member, declaration | member only | — | `bitand` |
| `or` | K | `BitOr.or(self, other)`; `or_else` on a `T?` (§2.3) is one identifier | member, declaration | member only | — | `bitor` |
| `use` | K | `Resource.use(f)`, `counter.uses` | member | **yes** | — | `with_lock`, `borrow` |
| `loop` | K | `EventLoop` (capitalised, fine); `let loop be …` | binding | no | — | `events`, `runner` |
| `break` | K | `text.break_lines()` is one identifier; a `Break` control value | member | **yes** | — | `split_lines` |
| `continue` | K | nothing in a general library | — | — | — | — |
| `each` | K | a binding named `each`; nothing else after `for each` was removed | binding | no | — | `item`, `element` |
| `giving` | K | nothing — an unusually safe keyword, and that is a virtue | — | — | — | — |
| `has` | K | `Map.has(key)` (the JS spelling) | member | **yes** | — | **`contains_key`** — preferred anyway, for symmetry with `Set.contains` |
| `of` | K | `Array.of(1, 2, 3)`; an `of:` label | member, label | **yes** | **yes** | `from_items` — preferred anyway, since `(Array of Doc).of(…)` reads badly |
| `be` | K | nothing. (`Be` is beryllium, but element symbols are capitalised.) | — | — | — | — |
| `null` | **not yet reserved** | `json.null`, `sql.NULL`; `Value.Null` is capitalised and safe | member | **yes** | — | must be **added** to the reserved list as a literal, per `syntax-revision-2.md` §3.1 |
| `try` | K → F | `try_parse`, `Map.try_insert`, `try_reserve` — all single identifiers | member | **yes** | — | freed; `try_*` becomes the prefix for a non-panicking variant |
| `with` | R | `with_capacity` (one identifier); the `with:` label; `Mutex.with_lock` | label, member | **yes** | **yes** | — |
| `on` | R | the `on:` label (`train(net, on: data)`); `Event.on(handler)` | label, member | **yes** | **yes** | — |
| `move` | R | `moving_average`, `Vec.move_from` — single identifiers | — | — | — | no real collision |
| `static` | R | nothing in a general library; `from static` in the FFI sublanguage | — | — | — | — |
| `send` | R | `Sender.send(value)` — the channel method §3.6 needs | member | **yes** | — | `put`, `push` |
| `receive` | R | `Receiver.receive()` — same | member | **yes** | — | `take`, `next` |
| `spawn` | R | `spawn(f)` as a free function — the one §3.6 would want | **free fn** | **no** | — | **`scope.start(…)`** (`stdlib-standard.md` §10.3) — a member, which the dot rule covers, and F3 keeps the word |
| `parallel` | R | `.parallel()` — `collections-and-chains.md` §7's chain link | member | **yes** | — | none needed, but the dot rule is **load-bearing** for §7 |
| `async`, `await` | R | nothing, given Decision 3 | — | — | — | keep reserved; the cost is zero and the option is worth holding |
| `pure` | R | nothing | — | — | — | — |
| `tool`, `prompt` | R | `agent.tool(…)`; `let prompt be …` — what every F4 user will write | member, **binding** | member only | — | `instruction`, `template` |
| `checkpoint` | R | a training checkpoint — `let checkpoint be …`, `save_checkpoint` | **binding** | no | — | `snapshot` |
| `resume` | R | `Download.resume()` | member | **yes** | — | — |

**What the table shows.** Of 43 rows, the dot rule closes 17 outright. The label
rule closes the label position for `at:`, `in:`, `of:`, `with:` and `on:`, and
for `in:` it is the only rule that reaches it. Nine more are closed by words
`reserved-words.md` §5 and `syntax-revision-2.md` §7 already propose to free.
The genuine residue — words a general-purpose library wants in a position no
rule reaches, where it must rename — is **nine**:

`kernel`, `const` (module), `model` (binding and module), `tensor` (binding),
`type` (binding), `in` (binding), `loop` (binding), `spawn` (free function),
`checkpoint` (binding), plus `assert` and `any` as free functions.

Every one of those is binding, module or free-function position, which is
exactly `reserved-words.md` §0.2's finding, now confirmed against a much wider
vocabulary than the scientific one: **the dot rule is the single highest-value
change, and after it the remaining problem is entirely about declaration
positions.** Three of the nine — `kernel`, `model` and `tensor` — disappear if
`reserved-words.md` §5.2 is taken in full, and `assert` and `any` are separately
answered by that note's §4.

### 4.5 A collision the scientific audit could not see: operator-trait methods

§5.4 of the core spec lists the operator traits — `Add Sub Mul Div Rem Pow MatMul
Neg Index Eq Ord Copy Clone Drop Iterate From Display` — and the convention that
falls out of Rust is that each trait's method is the trait's name in lowercase:
`Add.add`, `Neg.neg`. That convention breaks the moment the list grows to cover
bitwise operators, and a general-purpose standard library needs them (bitmasks,
flags, `U8` manipulation in a decoder, `signal`'s bit-reversal).

`Not.not(self)`, `BitAnd.and(self, other)` and `BitOr.or(self, other)` are
**declarations** — `function not(self) -> Self` — and declaration position is
precisely what the dot rule does not reach. `not`, `and` and `or` are keywords
in use. So:

> **Decision 4c.** An operator trait's method takes the trait's lowercase name
> only when that name is free. Where it is not: `Not` declares `invert`,
> `BitAnd` declares `bitand`, `BitOr` declares `bitor`, `BitXor` declares
> `bitxor`, `Shl`/`Shr` declare `shift_left`/`shift_right`. `and` and `or` are
> **not** overloadable at all — they are short-circuiting control flow, and a
> user type that redefines them redefines evaluation order, which no library
> should be able to do.

This also flags a hole rather than filling one: §5.4's operator-trait list has no
bitwise traits although §4.6's precedence table has `& ^ | << >>`. That gap
belongs to the core spec and is requested in §8.

### 4.6 Two siblings disagree, and this note settles it

`collections-and-chains.md` §1.4 spells the predicate scan `has_any(p)` /
`has_all(p)`, explicitly "because `any` is reserved".
`scientific-libraries.md` §3 spells the reduction `any_of` / `all_of`, and says
to keep that name even if `any` is freed. Read together they look like drift.

They are not the same operation, and the resolution is to say so:

- **`chain.has_any(p)` / `chain.has_all(p)`** — a short-circuiting scan over a
  chain with a predicate. A method, a `Bool`, and it reads as a sentence:
  `if rows.iterate().has_any(each.is_missing()):`.
- **`any_of(mask)` / `all_of(mask)`** — a reduction over an existing array of
  `Bool`. A free function in `math`, no predicate, and the NumPy operation the
  scientific audience is reaching for.

Two names for two operations in two positions is not duplication. Both notes keep
their spelling, and neither needs the word `any`.

### 4.7 Module naming

- **One lowercase word wherever possible**: `math`, `linalg`, `stats`, `signal`,
  `text`, `time`, `os`, `net`, `thread`, `test`.
- **A dot introduces a submodule, never a package boundary**: `data.csv`,
  `text.regex`, `physics.constants`. The level a module lives at is not visible
  in its name, and deliberately so — a module that moves between Level 2 and
  Level 3 must not force a source change.
- **No module is named for its implementation.** `compress.zstd` is named for the
  format, which is a standard, not for `libzstd`, which is a library that may be
  replaced. This is §5.7's criterion applied to naming.
- **`const` is not a module name** (§4.4). `physics.constants`, per
  `reserved-words.md` §5.6.

---

## 5. Level 3 — the official separate packages

Each entry gives what it exposes, whether it is written in Science or linked
against C with the library named, and its dependencies.

### 5.1 `compress` — gzip, zlib, zstd

**Exposes.** Streaming compressors and decompressors over the chain vocabulary,
plus whole-buffer convenience:

```science
use compress.gzip (GzipReader, GzipWriter, decompress, compress)

function read_log(path: borrowed Path) -> (Array of String, CompressError?):
    let file, err be File.open(path)
    if err?:
        return (Array.new(), CompressError.from_io(err))
    let reader, err be GzipReader.over(file)
    if err?:
        return (Array.new(), err)
    return (reader.lines().collect(), null)
```

**Route.** Linked, with two libraries named. `zlib` and `gzip` are served by the
**`miniz`** single-file public-domain implementation that `data-io.md` §3 already
vendors for `.npz`, so the dependency is shared rather than new. `zstd` is served
by **`libzstd`** (BSD, Meta), linked with `when available` (`ffi-c-boundary.md`
§5.2) and vendored in the package's own tarball as the fallback. Not written in
Science: DEFLATE and Zstandard are performance artefacts, not formulas, and
`scientific-libraries.md` §2's rule — *link where the artefact is decades of
tuning, write where it is a formula* — puts them squarely on the link side.

**Depends on.** Level 1 (`File`, `Path`, `Array of U8`), Level 2 (`data`'s
`IoError`, the chain vocabulary). Nothing else. It is a leaf.

**Why Level 3 and not Level 2.** It is the textbook instance of §5.7's criterion:
a compression algorithm changes for performance (zlib-ng, libdeflate) and for
security (zlib has had CVEs, including one in 2022 in code from 1995) without
anything about Science changing.

### 5.2 Structured text: TOML and YAML — and why CSV is not here

The brief lists "CSV/TOML/YAML" as one Level 3 package. That contradicts
`data-io.md` §3, which ranks CSV second by audience-unblocked and puts it **in**
the toolchain. The reconciliation splits the three, because they are three
different risk profiles wearing one grammar-shaped hat.

> **Decision 5a. CSV stays where `data-io.md` §3 put it — Level 2, in the
> toolchain, as `data.csv`. TOML moves *up* to Level 2 as `data.toml`. Only YAML
> is a Level 3 package.**

**CSV stays at Level 2.** `data-io.md` §3's ranking is right and this note does
not contradict it: CSV is what every instrument, every survey, every teaching
dataset and every "export" button produces, its grammar has been frozen since RFC
4180, and a scientist who cannot read a CSV cannot start. Its parser can change
for performance, which is a criterion violation, and §5.7 handles that by
requiring its *contract* not to name an algorithm.

**TOML moves up, for a bootstrapping reason.** If TOML is the manifest format for
the package manager Science does not yet have — and it is the obvious
choice — then it cannot be delivered *by* that package manager. A package manager
whose manifest parser is a package is a circular dependency. Whatever format the
eventual manifest uses must be in the toolchain, and the cheapest way to be sure
is to put TOML there now. It is also two hundred lines: TOML's grammar is small,
frozen at 1.0, and deliberately unextensible.

**YAML is Level 3 and it is the reason the brief's grouping needed splitting.**
YAML's specification is eighty pages, its type-resolution rules are why `NO` is a
boolean in Norway, and `yaml.load` is one of the most-exploited APIs in Python's
history. It is the single clearest case in this whole note of an algorithm that
must change for security without the language changing.

**Route.** All three written in Science. None of them is a tuned artefact; all
three are parsers, and a parser in Science is memory-safe by construction, which
is the entire point for YAML.

**Depends on.** Level 1 and `data`'s error types. YAML additionally needs
`text.regex` if it implements the full type-resolution rules, which is one more
reason to keep it out of the toolchain.

### 5.3 Data parallelism is not a package, and cannot be

The brief lists "data parallelism (Rayon-style map/reduce over collections and
tensors)" as a Level 3 package. This note disagrees, explicitly rather than
silently, and the reason is structural rather than a matter of taste.

> **Decision 5b. Data parallelism is Level 1 and Level 2, not Level 3. The
> `Divide` trait and `.parallel()` are core; the worker pool is in `science-rt`;
> only the *scheduler policy* is swappable.**

Three reasons:

1. **The orphan rule forbids it.** §5.4 of the core spec: an implementation is
   legal only if the trait or the type belongs to the declaring module. A Level 3
   package cannot add `.parallel()` to `Array`, `Map`, `Set` and `Range`, because
   it owns neither the trait nor the types. Rayon can do this in Rust only
   because it defines its own `ParallelIterator` trait and every type
   re-implements it — which is precisely the "every source in existence needs a
   second implementation" outcome that `collections-and-chains.md` §7.1 designs
   to avoid.
2. **`collections-and-chains.md` §7 already committed the core to it**, listing
   seven properties that must hold in **F0** for `.parallel()` to be addable in
   F2 without a redesign — among them that every combinator's contract is
   specified as an input-to-output relation, and that each closure's capture set
   is recorded in its type from F0. Those are core obligations. A package cannot
   impose them and cannot benefit from them if it is not the consumer.
3. **Two thread pools is an operational incident.** §3.6 already refuses a second
   pool. A Level 3 parallelism package is by construction a second pool.

What *is* separable, and should be: the **scheduling policy** — grain size,
work-stealing strategy, NUMA placement, whether to use `hwloc`. Those are
performance algorithms in §5.7's sense and belong behind an interface. Note the
deliberate exception: `collections-and-chains.md` §7.1 pins the grain size into
the *semantics* of `.parallel().sum()`, so that results are bitwise reproducible.
That is a criterion violation bought knowingly, for a stated reason — published
results — and §5.7 lists it as such.

Parallelism over **tensors** is F1/F2 and belongs with the tensor type, not
here. A `.parallel()` over a `Tensor` is a scheduling decision inside the
array-level IR that §7.1 of the core spec reserves a slot for, not a chain over
elements.

### 5.4 Database drivers

**Exposes.** `db.sqlite` and `db.postgres`, over one small interface so that
application code is portable:

```science
use db (Connection, Rows)
use db.postgres (Postgres)

function counts(url: borrowed String) -> (Map of (String, Int), DbError?):
    let connection, err be Postgres.connect(url)
    if err?:
        return (Map.new(), err)
    let rows, err be connection.query("select name, n from counts", [])
    if err?:
        return (Map.new(), err)
    return (rows.iterate().tally(by: each.text(0)), null)
```

**Route, and it differs per driver.**

- **SQLite: linked**, against the public-domain **SQLite amalgamation**, vendored
  as the single `sqlite3.c` it ships as. There is no case for writing it: it is
  one file, it is the most-tested C in the world, and its file format is a
  thirty-year commitment.
- **PostgreSQL: written in Science**, implementing the v3 wire protocol
  directly, rather than linking `libpq`. `libpq` drags OpenSSL, a connection
  model Science's ownership would have to wrap anyway, and a second copy of
  everything the socket layer already does. The v3 protocol is stable since 2003
  and thoroughly documented. This is the rare case where writing beats linking,
  and the reason is dependencies rather than performance.

**Depends on.** SQLite: Level 1 only. PostgreSQL: a `net` module with TCP
sockets, and TLS.

> **Named as a prerequisite rather than assumed away.** This section was drafted
> when no note in the repository owned `net`. `stdlib-standard.md` §1 now
> specifies it — Level 2, `TcpListener`/`TcpStream`/`UdpSocket`, written in
> Science over the platform sockets API and `getaddrinfo`, F2 — and specifies
> `crypto.tls` as OpenSSL under `when available`. Both unblock this package, and
> both are inherited rather than chosen: the driver cannot ship before F2, and
> its TLS story is whatever `crypto` decides.

### 5.5 Advanced cryptography

**Exposes.** `crypto.aes`, `crypto.rsa`, `crypto.ecc` (Ed25519 signing, X25519
key agreement, P-256 for interoperability), and the hashing and AEAD primitives
that the rest depends on.

**Route. Linked, and this is not a close call.**

> **Decision 5c. Science does not implement cryptographic primitives. `crypto`
> is a binding to `libsodium` for the modern primitive set (Ed25519, X25519,
> AES-GCM, ChaCha20-Poly1305, Argon2, BLAKE2), and to BoringSSL where RSA,
> X.509 or TLS interoperability is required.**

Two reasons, the second of which is specific to this language and is the one that
should decide it:

1. Implementing primitives is how audited libraries get their CVEs, and Science
   has no cryptographers.
2. **Constant-time code is not expressible in a language with an optimiser.**
   Cryptographic correctness depends on the *machine code* having
   data-independent timing. Science compiles through LLVM with, per §3 of the
   core spec, "custom optimizations: none; everything is delegated to LLVM". LLVM
   is entitled to turn a branchless constant-time select into a branch, and does.
   A Science implementation of AES would be correct, auditable at source level,
   and potentially leaking at machine level, with no notation in the language for
   saying otherwise. Linking a library whose authors control its assembly is the
   only honest option.

**Depends on.** Level 1 and `ffi`. It is a leaf and should stay one.

**Which level it actually ships at, reconciled.** `stdlib-standard.md` §1 puts
`crypto` at **Level 2, labelled** — linked to libsodium, TLS as OpenSSL under
`when available` — with its Corollary A as the argument: a wrapper satisfies the
release-cycle rule that its contents violate, because libsodium's security
releases arrive on libsodium's schedule through the system package manager, and
Science ships a *name* for an algorithm rather than an implementation of one.
That is a better argument than this note's Level 3 placement, and it is the only
one available while §6 holds, since **Level 3 has no distribution mechanism and a
security-critical module is the worst possible thing to put behind one.** So:
crypto ships at Level 2, labelled with the reason it does not belong there and
the condition under which it leaves. The *route* decision above — linked, never
implemented, libsodium named — is unchanged and the two notes agree on it
exactly.

**What does *not* live here.** Non-cryptographic hashing (`Map`'s hash function,
checksums, CRC32) is Level 1 infrastructure and must not depend on `crypto`.
`random` is **not** a CSPRNG and must never be used as one; the two have opposite
requirements, and `scientific-libraries.md` §2 commits the scientific generator
to being splittable and reproducible, which is exactly what a cryptographic
generator must not be. `stdlib-standard.md` §13.4 goes further and makes the
confusion a diagnostic (`SC0265`, on passing one kind of key where the other is
wanted) and forbids any `crypto` type from being named bare `Key`. That is the
right treatment and this note defers to it.

### 5.6 Two more Level 3 candidates, named by a sibling

`stdlib-standard.md` applies the same criterion to its own eleven modules and
sends two of them in this direction. They belong in this section's list rather
than being discovered later.

**`time.zones`.** Evicted outright, and it is the cleanest case anyone has
produced: the IANA tz database is revised several times a year by governments
changing their minds about daylight saving, and a program compiled against last
year's tarball computes a wrong wall-clock time with no error anywhere. Exposes
`Zone` and `Zoned`; written in Science over the system tzdata where one exists;
depends on `time` and `os`. **It is the only module in the whole three-note
design that the criterion actually removes**, and with Level 3 undeliverable
(§6) the honest statement is that Science has no time zones until either the
package manager exists or the module ships labelled. That note says the same.

**`http`.** An HTTP/1.1 client, Level 2 but labelled with a stated expiry:
HTTP/2 and HTTP/3 are protocol work with their own security cadence, and a client
that only speaks 1.1 has a date on it. Exposes a request/response client only —
no server — over `net` and `crypto.tls`. The reason it is worth having at all is
the one async-shaped workload §3.2 identified: fetching many URLs to build a
dataset.

Neither is re-specified here. They are listed because §5 claims to be the Level 3
inventory, and an inventory that omits the two modules a sibling just evicted is
not one.

### 5.7 The transversal criterion, applied to the author's own list

The criterion:

> *If an algorithm may have to change for security or performance without the
> language changing, it does not belong in the core.*

The author asked to be told which modules violate it. The honest answer, before
the modules: **taken literally, the criterion empties the core.** It deletes
`Map`, `sorted()`, `print` of a float, and the allocator — all four of which are
in §8's closed list or implied by it. So the list below is the audit under the
*literal* reading, given in full because that is what was asked for, followed by
the refinement that makes the criterion usable rather than abandoned.

**`stdlib-core.md` §1 landed while this was being written and reaches the same
refinement from the other end**, sharpening the criterion to "Level 1 specifies
observable behaviour; where the algorithm is itself observable, the module is not
Level 1", and making it mechanical with five questions. Two notes converging on
the same correction from different premises is the strongest evidence available
that the correction is right. Where the two treatments differ in outcome, that
note's is the better one and is adopted:

- **`sorted()`** — listed below as a violator under the literal reading.
  `stdlib-core.md` §1.6 resolves it properly: `sorted()` is specified *stable*,
  stability is the observable, and the sort network is free. It is Level 1 and the
  criterion is satisfied.
- **Float formatting** — same. The shortest round-tripping decimal of an `F64` is
  unique, so the observable is pinned and Ryū may replace Grisu unobserved.
- **`Map`'s hash function** — `stdlib-core.md` §3.5 states that the hash is not
  part of `Map`'s specified behaviour, which is exactly compliance rule 1 of §5.7,
  applied before this note asked for it.

The remaining entries stand.

**Level 1 — core. Seven violations, and they are the serious ones.**

| Module / item | Why it violates | Changes for |
|---|---|---|
| **`Map`'s hash function** | The worst case in the whole list, because `Map` is in §8's closed list. Hash-flooding is a denial-of-service class that forced Python, Ruby, PHP and Rust all to change their hash function under pressure; and the layout question (chaining, Robin Hood, swiss tables) is pure performance. | **Both** |
| **`Array.sorted()` / `sorted(by:)`** | Rust's sort has been changed twice (introsort → pdqsort → driftsort) with no language change either time. | Performance |
| **The allocator** (§8: "allocation over the system allocator") | The spec names the algorithm. Every serious runtime eventually ships its own or adopts mimalloc/snmalloc. | Performance |
| **Float formatting in `Display` and `print`** | Grisu → Ryū → Dragonbox, twice in a decade, in every language. `strings-formatting-and-docs.md` owns the surface; the algorithm underneath is not the surface. | Performance |
| **`String` case mapping, normalisation and collation** | Unicode ships a new version every year and the tables change every time. A language that pins them pins a date. | Correctness-over-time, which is the criterion's spirit |
| **`Array`'s growth factor and `String`'s small-string optimisation** | Pure performance tuning, visible only in benchmarks. | Performance |
| **`.parallel()`'s grain size and scheduler** | `collections-and-chains.md` §7.1 deliberately pins the grain size into the *semantics*, to get bitwise reproducibility. A knowing violation, listed as one. | Performance |

**Level 2 — the toolchain. Six more.**

| Module | Why it violates | Changes for |
|---|---|---|
| **`data.json`** | Depth and size limits are a DoS surface; number parsing and SIMD scanning are performance. | Both |
| **`data.csv`** | Quoting dialects change with reality; parsing is a SIMD problem. | Performance |
| **`data.parquet` / `data.arrow`** | Arrow C++ is a large C++ dependency with its own CVE stream, and the Parquet format itself versions. | Both |
| **`miniz`, vendored for `.npz`** (`data-io.md` §3) | A zlib-family decompressor in the toolchain, inheriting the zlib CVE class. | Security |
| **`math`'s special functions** (`erf`, `gamma`, Bessel) | Cephes-derived approximations get replaced when someone finds a better polynomial. | Performance and accuracy |
| **`stats.random` (Philox / Threefry)** | Named here because it is a **genuine conflict, not an oversight**. See below. |  |

**The `stats.random` conflict, stated rather than resolved by fiat.**
`scientific-libraries.md` §2 says the generator "must be Science", because §1 of
the core spec advertises "random keys that cannot be reused" as a motivating
verification case, and that guarantee is unavailable if the generator is C state
behind a pointer. The criterion says the opposite: a PRNG is an algorithm that
changes for performance. Both are right, and the resolution is that **the
guarantee is about the type, and the criterion is about the algorithm.** The
splittable-key type and its ownership behaviour are core; the bit-mixing function
is not, and the module should expose the generator as a named, versioned choice
(`Philox4x32_10`) rather than as "the" generator, so that a future `stats.random`
can add one without changing any published result.

**What satisfies the criterion, and is the model to copy.** `linalg` links BLAS
and LAPACK, and `ffi-c-boundary.md` §5.1 makes the specific implementation a
*link-time* choice — OpenBLAS here, MKL there, whatever the cluster's module
system has loaded. The algorithm changes freely, the contract does not, and
nothing in Science knows the difference. That is what compliance looks like.

**The refinement that makes the criterion usable.**

> **Decision 5d. The criterion is about *contracts*, not about *code*. Restated:
> **no Level 1 or Level 2 API's observable contract may name or imply an
> algorithm.** An implementation may live in the core; a guarantee that pins it
> may not.**

Three compliance rules follow, and they are what `stdlib-core.md` and
`stdlib-standard.md` should check every entry against:

1. **No documented output may depend on the algorithm.** `Map` iteration order is
   the canonical trap — and `collections-and-chains.md` §5.2 has already chosen
   insertion order for `Map` and `Set`, which is a *stronger* contract than "no
   order", and a deliberate one. It is compatible with a swiss-table plus an
   index vector, so it does not pin the hash function; it does forbid a plain
   open-addressed table. That price should be visible in that note, and this note
   flags it rather than reversing it.
2. **Anything with a CVE surface is replaceable at link time**, the way BLAS is.
   If `miniz` has to become `zlib-ng` in a patch release, that must be a build
   flag, not a source change in anyone's program.
3. **Anything whose algorithm is a moving target is named and versioned in its
   own API** — `Philox4x32_10`, not "the RNG"; `Unicode 16.0` tables, with the
   version queryable — so that a program can state what it was compiled against.
   This is the same instinct as `ffi-c-boundary.md` §5.5's version-query
   initializer, applied inward.

The three deliberate exceptions, bought knowingly and listed so nobody has to
rediscover them: `Map`/`Set` insertion order, `.parallel()`'s pinned grain size,
and §5.1's defined reduction order. Each buys reproducibility for a published
result, which is worth more to this audience than implementation freedom.

---

## 6. What versioning means with no package manager

This is the weakest joint in the three-note stdlib design and it should be named
as such rather than assumed away.

### 6.1 What "their own release cycle" can mean today

The brief describes Level 3 packages as having "their own release cycle". Given
§12 — "Language server, formatter, package manager. An environment-reproducibility
story is the single most-cited reason scientists trust results across machines,
and it is planned, not built" — here is the complete list of what that phrase can
actually denote today:

1. **A separate directory in the same tarball, with its own version number.** The
   user gets it by downloading the toolchain. It has a release cycle in the sense
   that its version number can move independently of the compiler's.
2. **A separate tarball the user unpacks and points `SCIENCE_PATH` at.** This is
   `PYTHONPATH` with fewer features, and on an HPC cluster it is what a module
   file would set. It works, and it is how most scientific software is actually
   deployed.
3. **A git URL the user clones.** The same as (2) with a fetch step and no
   integrity checking.

That is the whole list, and none of the three is a release cycle in the sense the
phrase normally carries.

> **Decision 6. Today, "its own release cycle" means exactly one thing: *a Level
> 3 package may make a breaking change in a release that does not make a breaking
> change to the language.* It is a **deprecation** cycle, not a distribution
> mechanism. Distribution is (1) and (2), and both are manual.**

### 6.2 What that cannot do, stated plainly

The four failures, because each of them will be hit:

- **Two versions cannot coexist on one machine.** Whatever is on `SCIENCE_PATH`
  is what every program on that machine compiles against. A user with one project
  needing `db 0.3` and another needing `db 0.4` has no mechanism at all.
- **A security patch requires a whole-toolchain upgrade.** If `compress` ships in
  the tarball and `libzstd` has an advisory, the fix is a new tarball. On a
  cluster where `sciencec` is a module the administrator installed, that is a
  ticket with a two-week latency — the exact environment §1.2 used to argue *for*
  batteries-included. The argument cuts both ways and this is the cut.
- **There is no way to record what a result was produced with.** This is the one
  that matters most for the stated audience. A paper's methods section can say
  "Science 0.4.1" and cannot say which `stats` was in the tarball.
- **No third party can depend on a Level 3 package**, so the ecosystem §1.2 hoped
  the standard library would enable stops at one layer deep.

### 6.3 The two things that must be decided in F0 anyway

A package manager is correctly out of scope. But two of its preconditions are
one-way doors, they are nearly free today, and they are expensive-to-impossible
to retrofit:

1. **Module resolution must go through a search path, not a hard-wired root.**
   If `use data.csv` resolves by looking inside the toolchain installation, then
   adding a search path later changes the meaning of existing programs. If it
   resolves against an ordered list — `--module-path`, then `SCIENCE_PATH`, then
   the toolchain — then (2) above works today and a package manager later is a
   resolver that populates the list. This mirrors `ffi-c-boundary.md` §5.1's
   three-step library search, and it should mirror it deliberately, because a
   user who has learned one will guess the other.
2. **A module directory carries a manifest with a name and a version, even though
   nothing reads the version yet.** Recording it costs a file. Not recording it
   means that when the package manager arrives, no artefact in existence can be
   identified, and the first thing it has to do is guess.

Neither is a package manager. Both are the parts of one that cannot be added
afterwards.

### 6.4 The honest summary

The three-note stdlib design rests on a distribution mechanism that does not
exist. Levels 1 and 2 are safe, because the toolchain tarball is a real
distribution mechanism and §1's whole argument is that it is the only one this
audience reliably has. **Level 3 is a category whose defining property — separate
distribution — is not implemented.** Until it is, a Level 3 package is a Level 2
module in a different directory with a more honest version number, and this note
recommends saying that in the documentation rather than implying a package
ecosystem that users will go looking for and not find.

---

## 7. Diagnostics allocated

This note's block, per `README.md`'s allocation map, is `SC0276`–`SC0279`, in the
types-and-traits range. **Two of the four are claimed**; both are type-level
checks, which is what that range is for. Checked free against the map and against
the codes claimed by `indexing-and-array-literals.md` (`SC0280`–`SC0287`),
`strings-formatting-and-docs.md` (`SC0274`–`SC0275`),
`collections-and-chains.md` (`SC0271`–`SC0273`), `stdlib-core.md`
(`SC0257`–`SC0259`) and `stdlib-standard.md` (`SC0265`–`SC0270`).

| Code | Fires on | Severity | Suggestion |
|---|---|---|---|
| `SC0276` | The value slot of a fallible pair is used on a path where its error was not tested with `?` (§2.2). Primary span on the use, secondary on the binding | error | The `if err?:` guard, inserted before the use |
| `SC0279` | A parallel reduction over a combiner that is not declared associative, where the result would depend on the split (§3, `collections-and-chains.md` §7.1) | error | `.parallel()` removed, or the sequential terminal named |

`SC0276` is deliberately narrower than `syntax-revision-2.md` §3.3's `SC0140`
and complements it: `SC0140` fires at function exit for an error never tested at
all; `SC0276` fires at the use site of the *value* beside it. A program can pass
one and fail the other, which is why they are two codes.

**Two of the four are not claimed and return to the free pool**, both because a
sibling written the same afternoon had already drafted the identical check — the
fifth and sixth instances of the failure `README.md`'s allocation section exists
to stop, and neither was visible to either side.

- **`SC0278`** — the missing-conversion case of §2.4. `stdlib-core.md` §7.7 has
  `SC0259` for it, better specified (it carries the rendered output and the rule
  for when the `help` is offered). Released.
- **`SC0277`** — a value crossing a thread boundary without the marker interface.
  `stdlib-standard.md` §10.2 has `SC0266` for it, and that note also owns the
  interface names (§3.3). Released. Its companion `SC0267`, for a detached
  thread's closure capturing a borrow, is that note's too and this note does not
  duplicate it either.

**Codes actually claimed by this note: `SC0276` and `SC0279`. `SC0277` and
`SC0278` are returned to the free pool.**

---

## 8. What this note asks of the other notes

Ordered by how much is blocked behind each.

1. **Paired narrowing** (§2.2), as a small extension of
   `syntax-revision-2.md` §3.1's flow-sensitive narrowing: proving one slot of a
   declared fallible pair null narrows its companion on that path. Decision 2b is
   unusable without it, and every fallible constructor in the library is Decision
   2b.
2. **The label rule** (§4.3): in an argument list, a word immediately followed by
   `:` is a label, not a keyword. The same five lines of parser as
   `reserved-words.md` §0.1's dot rule, in the same function family, closing the
   fifth position that note's taxonomy does not have. It should be adopted
   together with the dot rule or not at all, since separately they look arbitrary.
3. **The dot rule itself** (`reserved-words.md` §0.1). §4.4's table is the widest
   evidence yet assembled for it: 17 of 43 collisions close outright, including
   `.parallel()`, which `collections-and-chains.md` §7 depends on, and
   `channel.send`/`channel.receive`, which §3.6 here and `stdlib-standard.md` §2
   both depend on — that note states outright that its channel API falls back to
   `put`/`take` if the dot rule is refused. This note adds no new argument; it
   adds a much larger denominator and a third dependent.
4. **The marker interfaces `Share` and `ShareBorrowed`** (§3.3), as an addition to
   §5.4 of the core spec's trait list. Auto-derived structurally, never written by
   hand, diagnosed by `stdlib-standard.md` §10.2's `SC0266`. That note owns the
   names and the surface; this one seconds the ask, because it is the only
   type-system addition Decision 3 needs and two notes now depend on it.
5. **Bitwise operator traits** (§4.5). §4.6's precedence table has `& ^ | << >>`
   and §5.4's operator-trait list has no traits for them. Whichever names are
   chosen, `and`, `or` and `not` cannot be the method names, and `and`/`or`
   should not be overloadable at all.
6. **`ffi-c-boundary.md` §5.2's `when available` table must become a once-cell**
   (§3.4), and an `extern` block should gain a way to declare that its library is
   not thread-safe. The first is a correction; the second is a request.
7. **~~A `net` module note~~ — answered.** `stdlib-standard.md` §1 specifies
   `net` and `crypto.tls` (§5.4). The residual ask is smaller: the database
   package needs to know whether `net`'s blocking calls are safe to make from
   inside a `scope`, which that note implies and does not state.
8. **`null` must be added to the reserved-word list** (§4.4).
   `syntax-revision-2.md` §3.1 introduces it as a literal and
   `crates/science-lexer/src/token.rs` does not have it. It is the one word in
   §4.4's table whose status is "missing" rather than "reserved" or "freed".
9. **A spelling for a zero-argument closure** (§3.6). §4.6 of the core spec gives
   `each.field` and `name giving expression`, both of which have a parameter, and
   `def-and-lambda.md` §4.5 proposes `(a, b) giving …` for the multi-parameter
   case while leaving the zero-parameter one open. A thread body, a deferred
   action and a scope body all want it. This is adjacent to the standing
   cross-note ask for closure *types* (`README.md`) and to that note's §11, and
   the three should be settled together.
10. **F3 should confirm or reject §3.7's commitment on its behalf**: that any I/O
   multiplexing under actors lives inside `science-rt` and is never surfaced as a
   function colour.
11. **`collections-and-chains.md` §5.2 should state the price of insertion
    order** (§5.7): it is a stronger contract than "unspecified", it forbids a
    plain open-addressed table, and it is worth what it costs — but the cost
    should be in the note rather than in this one.

---

## 9. Risks

**Decision 1 is the expensive one and its bill arrives late.** A large Level 2 is
right for this audience today and it is a compatibility obligation for as long as
the language exists. §1.4's stability markers and deprecation diagnostics are the
mitigation, and they only work if they are adopted *before* the first module
ships, because a marker added later marks nothing.

**Decision 3 is the one most likely to be regretted publicly.** "No async" is an
unfashionable position and it will be read as a limitation rather than a choice.
The defence is §3.3 — async is not merely unnecessary here, it is technically
blocked by a core decision (no lifetime syntax) that is much more valuable than
async is. That argument must be in the documentation, not only in this note, or
the decision will be relitigated every six months by people who have not read it.

**The reserved-word situation is currently a three-document disagreement.** §13
of the core spec, `reserved-words.md` §5 and `syntax-revision-2.md` §7 each
propose a different list, the lexer implements a fourth, and §4.4 of this note
is written against the *union* of the proposals. If `reserved-words.md`'s
recommendations are refused, five of §4.4's nine genuine collisions come back and
the library has to rename accordingly. §4.4's "alternative" column exists for
exactly that outcome and should be read as a live fallback, not as commentary.

**Three stdlib notes were written in parallel and converged, which is luck.**
This note, `stdlib-core.md` and `stdlib-standard.md` independently reached the
same sharpening of the criterion, the same error convention, the same
threads-not-async decision and the same FFI thread-safety ask — and also produced
two duplicate diagnostics (§7) and two incompatible level numberings (§1.1). The
agreement is evidence the design is sound; the duplicates are evidence that
parallel authorship catches neither collisions nor near-misses, exactly as
`README.md`'s allocation section warns. The three notes should be read once
together before any of them is treated as settled.

**§5.7's criterion, once adopted, is a standing review obligation.** It is easy
to apply to a list written today and easy to forget when the twentieth module is
added. It should become a line on whatever checklist a new module passes, or it
will be a paragraph in a design note that nothing enforces.

**§6 is the risk that is not this note's to mitigate.** Level 3 is defined by a
distribution mechanism that does not exist. Everything in §5 is honest about
what it exposes and dishonest, structurally, about how a user gets it. §6.3's two
one-way doors are the only part of this that F0 can act on, and if they are not
taken now the eventual package manager starts from a worse position than it needs
to.

---

## 10. Summary of decisions

| Decision | Alternatives rejected | Reason | Cost |
|---|---|---|---|
| **1.** Batteries included at Levels 1–2; levels defined by distributability | Minimal core plus ecosystem (Rust) | No package manager; HPC users cannot install; the standard library is the only place a shared abstraction can live | Every module is a compatibility obligation forever; a security-response duty; a bigger tarball |
| **1.4** Nothing ships unsettled; stability markers and compiler-diagnostic deprecation from day one | Ship and stabilise later | This is precisely what `asyncio` and `distutils` did | A marker on every module, forever |
| **2a.** Infallible → `T`; absent → `T?`; fallible → `(T, E?)` | A uniform `(T, Error?)` everywhere | Absence is not failure; three outcomes are three shapes | Infallibility becomes a promise, and a wrong one is breaking |
| **2b.** The value slot is `T?` iff the type has no safe zero | A zero value in every value slot (Go's `nil`) | A zero-valued `File` is valid to the type system and invalid in fact | Needs paired narrowing (§8, ask 1), or it is unusable |
| **2c.** `panic` survives, aborts, and never fires on input; no `.unwrap()` | Panicking accessors; `unwrap` | `ffi-c-boundary.md` §4.5 depends on abort; `unwrap` is the most-copied bad habit in the corpus a model has read | `expect_present(message)` is wordier |
| **2d.** One concrete `choice` error per module, with named constructors | `Error?` everywhere; ad-hoc wrapping | Keeps `match` exhaustive; makes the explicit conversion always one named call | A `<Module>Error` and its constructors in every module |
| **2d.** Cause chains via a *defaulted* `cause` on `Error` — adopted from `stdlib-core.md` §7.5, replacing this note's draft | A separate `Caused` interface; a second *required* method | `Error` stays one-required-method, so §3.4 holds, and no second interface is needed | A defaulted method every error inherits |
| **3.** Threads and data parallelism; no async/await | Async with a runtime; both; green threads (Go) | Colouring splits the stdlib in two; async needs `borrows_live_at` three phases early and self-referential futures the language cannot express; green threads tax the foreign call, which is this language's hot path | Science is a bad language for a network server; fan-out I/O costs a bounded pool |
| **3.6** No second thread pool; the pool sizes to the batch allocation; `Mutex` exposes a closure, not a guard | An exposed pool; `os.cpu_count()` as the default; a `Drop`-released guard | Two pools oversubscribe a shared node; a guard makes lock scope a region question | No early release without a nested block; `stdlib-standard.md` §10 owns the surface and may decide otherwise |
| **4a/4b.** Conventional naming; no privileged stdlib vocabulary | A reserved internal namespace | The library is the teaching corpus, and a machine-written corpus imitates what it reads | Nine genuine renames (§4.4), until `reserved-words.md` lands |
| **4c.** Operator-trait methods avoid keyword names; `and`/`or` not overloadable | `Not.not`, `BitAnd.and` | Declaration position, which the dot rule does not reach — and redefining `and` redefines evaluation order | Two method names differ from the trait name |
| **5a.** CSV stays Level 2; TOML moves up; only YAML is Level 3 | The brief's single CSV/TOML/YAML package | `data-io.md` §3 already ranked CSV in; a manifest parser cannot be delivered by the package manager that reads it; YAML is the security case | Disagrees with the brief's grouping, explicitly |
| **5b.** Data parallelism is core, not a package | A Rayon-style Level 3 package | The orphan rule forbids it; `collections-and-chains.md` §7 already put seven obligations in F0 for it | Only the scheduler policy is swappable |
| **5c.** Crypto is linked (libsodium, BoringSSL), never written; and it ships at Level 2 **labelled**, per `stdlib-standard.md` §1 | Write the primitives in Science; ship it at Level 3 | No cryptographers; constant-time code is not expressible in a language that delegates everything to LLVM; and Level 3 has no distribution mechanism, which is the worst place for a security module | A C dependency in a security-critical package, at a level it does not belong at |
| **5d.** The criterion constrains contracts, not code | The criterion as literally written | Literally, it deletes `Map`, `sorted`, float printing and the allocator | Three deliberate exceptions, each named and priced |
| **6.** "Own release cycle" means a deprecation cycle, not a distribution mechanism | Implying a package ecosystem | It is not built (§12), and saying otherwise sends users looking for something that does not exist | Level 3 is, today, Level 2 in another directory |
| **6.3** Freeze two one-way doors now: a module search path, and a version in the manifest | Defer everything with the package manager | Both are nearly free today and impossible to retrofit | One file per module directory, read by nothing yet |
