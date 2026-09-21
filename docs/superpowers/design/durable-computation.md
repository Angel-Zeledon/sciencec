# Science — Design: `durable`, and the six-hour run that fell over at hour five

Date: 2026-09-18
Status: draft for review
Owns: the `durable def` declaration, the `keyed by` clause, `checkpoint every`,
`resume`, the cache-key construction, the eviction policy, and the rule that
decides when a cached answer may be reused.
Spends four words already reserved: `durable`, `checkpoint`, `resume` and
`supervise` — `crates/science-lexer/src/token.rs` reserves all four for F5 and
no note has ever designed one. This note designs the first three and leaves
`supervise` to the actors note.
Builds on, and does not re-decide: `effects.md` §3.1 (the three inferred bits),
§4.1 (`pure def`, the one declaration) and §5.2 (*"purity is necessary and not
sufficient"*, which §2 here is the concrete case of); `reproducibility.md` §5.1
(canonical JSON and the provenance id, whose construction §3 copies exactly) and
§4 (`--deterministic`); `package-manager.md` §2 (the content-addressed lockfile,
the other consumer of the same hashing discipline).
Also depends on: `codegen-and-linking.md` (the MIR hash of §3.2),
`data-io.md` §4 (`Frame.digest()`), `contracts.md` §7 (profiles),
`stdlib-standard.md` §5 (`Key`), `native-dependencies.md` §5 (tiers).
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0166`–`SC0167`** (syntax) and **`SC0620`–`SC0634`**
(types).

---

## 0. The verdict up front

The universal experience of the audience this language is for: a fit runs for six
hours, the machine is preempted at hour five, and the work is gone. The universal
workaround is a hand-rolled cache — a `pickle` file, a path built out of a
parameter string, and a comment saying *delete this if you change the model*.

That workaround has one bug and it is always the same bug: **the key describes
the inputs and not the code.** The author changes the body of the function,
reruns, gets the old answer back in four milliseconds, and believes it.

> **Decision 1. `durable def` is a function whose result is cached against a key
> that covers both its arguments and its compiled body. Changing the body
> invalidates the cache with no action from the author, because the body is in
> the key.**

```science
durable def fit(
        data: borrowed Frame,
        epochs: Int) -> Model
    keyed by (data.digest(), epochs)
    checkpoint every 10 epochs:
    ...
```

## 1. Why this is in the language and not a library

A library can memoise. Python has `functools.cache`, `joblib.Memory` and a dozen
others, and they are widely used, so the burden of proof is here.

Three things a library cannot do, and the third is decisive:

1. **A library cannot see the code.** `joblib` hashes the function's source text
   when it can find it, which misses every change in a callee, every change to a
   constant, and every change in a linked C library. The compiler has the MIR and
   the dependency graph and `native-dependencies.md` has the native tier — the
   one place in the system that can compute an honest key is the compiler.
2. **A library cannot check purity.** §2. Caching an impure function is a silent
   wrong answer and no decorator can refuse it.
3. **A library cannot checkpoint a running loop.** §5. Resuming mid-function
   requires the compiler to know where the loop's state lives, which is a MIR
   property.

## 2. Decision 2 — `durable` implies `pure`, and this is where `effects.md` §5.2 gets its concrete case

> **Decision 2. A `durable def` must have an empty inferred effect set. It is
> `pure def` with a cache; declaring both is redundant and is `SC0620`, a
> warning with a fix.**

Caching is replacing a call with a remembered answer. That is sound exactly when
the call had no other reason to happen, which is what `effects.md`'s three bits
already compute. This note therefore builds no analysis at all and inherits one,
which is the cheapest kind of feature and the reason this note is short.

`effects.md` §5.2 says purity is *necessary and not sufficient* and gives the
general argument. The sufficient conditions here are two and both are this note's
own:

- **The key must be complete.** A pure function of a `borrowed Frame` is a
  function of that frame's *contents*, and the key must cover the contents, not
  the borrow. §3.1.
- **The result must be serialisable.** A pure function returning a value holding
  a borrow cannot be cached — the borrow's referent is gone by the next run.
  `SC0621`, and the fix is to return an owned value.

### 2.1 The diagnostic that does the teaching

The common first attempt is `durable` on a function that reads a file, and the
diagnostic has to be good because `external` is inferred rather than written:

```
SC0622: `load_and_fit` cannot be `durable` because it is not pure
  --> pipeline.science:31:1
   |
31 | durable def load_and_fit(path: Path) -> Model
   | ^^^^^^^ this function's inferred effect set is { external }
   |
note: the `external` bit comes from `read_file`
  --> pipeline.science:34:16
   |
34 |     let raw, err be read_file(path)
   |                     ^^^^^^^^^ reads the filesystem
   |
   = a cached result would not notice the file changing
   = read the file in the caller and pass the data in:
     `durable def fit(data: borrowed Frame, ...)`
```

The witness chain is `effects.md` §3.6's and is not built here.

## 3. Decision 3 — the key is three hashes, and the third is the one that matters

> **Decision 3. The cache key is the SHA-256 of the canonical concatenation of:
> (a) the `keyed by` tuple, (b) the **body hash** — the MIR of the function and
> of everything it transitively calls — and (c) the **environment hash**, which
> is `reproducibility.md`'s build record minus its timestamp fields.**

Canonicalisation is `reproducibility.md` §5.1's, unchanged and not re-specified:
UTF-8, sorted keys, no floats, `\n` normalised. The key is quoted as twelve hex
characters with a `dk:` prefix, beside that note's `pv:`.

### 3.1 `keyed by` is explicit, and that is a real decision

The arguments are not hashed automatically. The author writes what identifies the
call:

```science
    keyed by (data.digest(), epochs)
```

**Automatic hashing was rejected**, for a reason specific to this audience: the
argument is often a 40-gigabyte `Frame`, and hashing it takes longer than the fit.
`data.digest()` may be a content hash, or it may be the dataset's DOI, or a run
id, or a file's mtime and size — the author knows which is sound and the compiler
does not.

The obligation this creates is stated rather than hidden: **a wrong `keyed by` is
a wrong answer, and it is the one unsound thing in the note.** Two mitigations,
neither of which is a proof:

- Every parameter must appear in the clause or be named in a
  `keyed by (..., ignoring weights)` list. A parameter that is silently absent is
  `SC0623`. So forgetting one is an error; deciding one is irrelevant is a line
  of source.
- `sciencec test --verify-cache` reruns durable functions and compares against
  the cache, reporting any disagreement. It is slow and it is meant to be run in
  CI nightly, which is where this class of bug is affordable to find.

### 3.2 The body hash is what makes it worth building

The MIR hash is already needed: `codegen-and-linking.md` computes MIR, and
incremental compilation needs a fingerprint of it. This note consumes that
fingerprint and adds the transitive closure over the call graph — which the query
architecture in §3 of the core spec computes as a side effect of being a query
architecture.

So the feature's central claim costs one traversal of a graph the compiler
already has. That is the whole engineering argument for putting this in the
compiler.

## 4. Decision 4 — where it goes, and what removes it

> **Decision 4. The cache is a content-addressed directory. Default location is
> `.science/cache/` in the workspace, overridable by `[cache] dir` in
> `science.toml` and by `SCIENCE_CACHE_DIR`. Entries are immutable and named by
> their key.**

Immutable and content-addressed means concurrent runs need no lock — two
processes computing the same key write the same bytes, and a rename is atomic.
This is `package-manager.md` §2's discipline and it is reused rather than
re-argued.

Eviction is **size-bounded LRU, defaulting to 10 GB**, with `sciencec cache`
subcommands `list`, `size`, `clear` and `keep <key>`. A cache with no bound is a
disk that fills, and a scientist's cache entries are large.

`--deterministic` (`reproducibility.md` §4) **does not disable the cache**, and
this is worth stating because the instinct is that it should. A cache hit returns
bytes that a previous run of the identical program in the identical environment
produced; that is precisely what determinism asserts. If a cache hit could differ
from a recomputation, the program was not deterministic and the flag has a bigger
problem. What the flag *does* is record the cache hit in the run record, so a
reviewer can tell that a result was replayed rather than recomputed.

## 5. Decision 5 — `checkpoint every`, which is the other half

> **Decision 5. `checkpoint every <n> <loop-binder>` saves the function's live
> state at that cadence. On a rerun, `resume` restores the latest checkpoint
> whose key matches and continues from it.**

```science
durable def fit(data: borrowed Frame, epochs: Int) -> Model
    keyed by (data.digest(), epochs)
    checkpoint every 10 epochs:
    let mutable model be Model.initial()
    for epoch in 0..epochs:
        model be step(model, data)
    model
```

The clause names a loop binder — `epochs` here refers to the range that `epoch`
walks — so the compiler knows which loop to instrument, and the state to save is
the loop's live-out set, which is a MIR liveness query.

Three constraints, all falling out of what a checkpoint is:

- **Every live value must be serialisable**, by the same rule as §2's return
  value. `SC0624` names the offending binding rather than the function.
- **The loop must be a `for` over a range or a collection**, not a bare `loop:`.
  A `loop:` has no notion of "how far through" and the resumed run cannot know
  what it already did. `SC0625`, with the fix.
- **The body must not depend on iteration order having side effects**, which
  purity already gives.

`resume` is not written by the user in the common case — it is what the runtime
does automatically on a cache-key match with a partial entry. The word is
reserved and is spelled explicitly only in the manual form,
`sciencec run --resume dk:9f3a1c04e7b2`, for the case where an author wants to
resume one specific run of several.

### 5.1 What a checkpoint costs

Serialising a model every ten epochs is not free and the number is the author's
to choose. The default when `every` is omitted is **no checkpointing** — the
function is cached at its boundary and nothing more. This is the right default
because the boundary cache is the cheap 80% and checkpointing is the expensive
20%, and a feature that silently writes gigabytes is not one anybody asked for.

## 6. The grammar

```
durable-def := "durable" "def" signature keyed-clause? checkpoint-clause? block
keyed-clause := "keyed" "by" "(" key-item ("," key-item)* ")"
key-item := expression | "ignoring" identifier
checkpoint-clause := "checkpoint" "every" expression identifier
```

`durable`, `checkpoint` and `resume` are already on §13's reserved list and this
note spends no new word. `keyed`, `by`, `every` and `ignoring` are **contextual**,
recognised only in these clauses, which follows the precedent `contracts.md` §2
sets for `requires` and `ensures` and keeps `reserved-words.md`'s budget intact.

## 7. What this note does not do

- **No distributed or shared cache.** A team cache behind HTTP is the obvious
  next thing and it needs an authentication story, a trust story and a garbage
  collection story. The content-addressed layout is chosen so that a remote
  backend is a fetch in front of the same directory, which is what
  `package-manager.md` did for packages.
- **No `supervise`.** Restart policies belong to the actors note in F3.
- **No automatic durability.** Nothing is cached that is not declared. Inferring
  which functions are expensive enough to cache is a profiling question wearing a
  language-feature costume.
- **No cache of impure functions under any escape hatch.** §2 has no `unsafe`
  door and should not get one.

## 8. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0166` | Syntax | `durable` on something that is not a `def` |
| `SC0167` | Syntax | `checkpoint every` naming a binder that is not a loop in the body |
| `SC0620` | Types | *Warning.* `durable pure def` — `pure` is implied |
| `SC0621` | Types | A `durable` function returns a value holding a borrow |
| `SC0622` | Types | The function is not pure, with `effects.md`'s witness chain (§2.1) |
| `SC0623` | Types | A parameter appears in neither `keyed by` nor `ignoring` |
| `SC0624` | Types | A value live across a checkpoint is not serialisable |
| `SC0625` | Types | `checkpoint every` on a bare `loop:` (§5) |
| `SC0626` | Types | `keyed by` is absent on a function with at least one parameter |
| `SC0627`–`SC0634` | Types | Held for the distributed cache of §7 |

## 9. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| Expose the transitive MIR fingerprint as a query | `codegen-and-linking.md`, core spec §3 | The fingerprint exists; the closure is a traversal |
| `Frame.digest()` | `data-io.md` §4 | One method, content hash over the Arrow buffers |
| A `Serialise` interface | `stdlib-core.md` | **The largest ask here**, and it has a second customer in `models-and-inference.md` §4's weight formats. It belongs with the derive mechanism in `README.md`'s standing asks |
| `[cache]` in the manifest | `package-manager.md` §2 | One table |
| Record cache hits in the run record | `reproducibility.md` §5.2 | One list of keys |
| Confirm `durable`/`checkpoint`/`resume` leave F5's reservation | `reserved-words.md` | One row; the words stay reserved, the owner changes |

## 10. Risks

- **A wrong `keyed by` is a silent wrong answer**, and §3.1 says so rather than
  claiming otherwise. This is the only unsound construct in the note and it is
  unsound in the direction the hand-rolled version already was — the feature is
  strictly better than the status quo and is not safe.
- **`Serialise` is a genuine dependency and does not exist.** Without it §5 is
  not buildable and §4 is limited to types the library can already write. The
  honest phasing is that the boundary cache lands first and checkpointing waits
  for derive.
- **Disk exhaustion on shared HPC filesystems** is the operational failure, and a
  10 GB default in a home directory with a quota will make somebody angry. The
  `[cache] dir` key and the environment variable are the mitigation and they need
  to be in the first page of the documentation, not the ninth.
