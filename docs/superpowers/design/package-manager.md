# Science — Design: the package manager

Date: 2026-09-16
Status: draft for review
Owns: the manifest file, version resolution, the lockfile, the registry, package
naming, publishing, yanking, and what reproducibility can honestly mean for a
Science build. The **native/system-library half is a sibling's** —
`ffi-c-boundary.md` §5 owns finding and linking `libopenblas`, and
`native-dependencies.md` owns providers, ABI variants and the native half of the
lock. This note owns only the requirement that whatever they found be *recorded*,
and the file it is recorded in.
Phase: **F0** for the manifest, the search path and the lockfile format; **F1**
for resolution, `git` dependencies, vendoring and offline builds; **F2+** for the
registry, publishing and signing. §7 stages it.
Depends on: `stdlib-shape-and-packages.md` §6 in full — that section states the
problem this note answers, and its §6.3 names the two preconditions this note
turns into F0 work. `rust-interop.md` §2.2, §2.6, §4.1 and §4.2, which settled
the question §0 was written to hedge against. `native-dependencies.md` §5, §6 and
§10, which landed during the drafting of this one; §0.1 records the six asks it
made of this note and answers all six.
Related: `scientific-libraries.md` §15 (the seam), `data-io.md` §9 (the toolchain
feature this note eventually deletes), `script-mode.md` §4.2 (the entry file —
**contradicted, in §4.6**), `syntax-revision-2.md` §8.5 (the `foreign/` sidecar's
manifest), `stdlib-standard.md` §5.5 (the RNG freeze, which is the reproducibility
standard this note is measured against), the core spec §1, §3, §4.4, §9 and §12.
Amends: the core spec §12, which lists the package manager as out of scope for
F0. This note agrees with three quarters of that and asks for the other quarter
back; §1.4 says which quarter and why.

Syntax: revision 2 throughout — `function`, `->`, `of`, `borrowed`, `is` / `is
not`, `> < >= <=`, `interface`, `Type has:`, `T?`, `-> (T, Error?)` with `?` as
the presence test, and `print`.

---

## 0. The dependency this note was told to hedge against, and how it resolved

This note was commissioned with an explicit instruction: `rust-interop.md` was
being written in parallel, and if it concluded that Science can consume crates
from crates.io, then Science would inherit a mature ecosystem *and* a mature
package manager at once, and this note would change substantially. It was to be
written for both outcomes.

**It landed while this note was being drafted, and the hedge is not needed.**
Its answer, quoted from its §0:

> **Science can link Rust, and linking Rust is worth a great deal. Science
> cannot consume crates.**

and, stated as the risk that note most wants named, its §9 and §2.2:

> Cargo resolves *Rust* dependencies. It does nothing whatever for
> Science-to-Science dependencies, which is the hole
> `stdlib-shape-and-packages.md` §6 actually names.

So the outcome is the one under which this note is *most* necessary, not least.
Recording what the two branches would have been, because a reader who assumes the
other branch will misread §7's staging:

| | Outcome A — no crate consumption (**this is what happened**) | Outcome B — crate consumption |
|---|---|---|
| The manifest (§4) | Needed. Science's own. | **Still needed**, and still Science's own: `entry`, `modules` and `language` are keys a `Cargo.toml` cannot carry. |
| The lockfile (§2) | Needed. Science's own. | **Still needed.** `Cargo.lock` records no Science source, no Science compiler version and no native observation. |
| The resolver (§3) | Needed. | Needed for the Science half; cargo's SAT solver would run beside it on a disjoint graph. Two resolvers, workable and ugly. |
| The registry (§5) | Needed, eventually. | **Much less urgent** — the ecosystem that matters would already be hosted. |
| Machine-checked semver (§3.2) | Worth building. | Worth much less; the API surface that matters would be Rust's. |
| No build scripts (§2.6) | Holds cleanly. | **Would have to be re-argued.** crates.io ships `build.rs`, and inheriting the ecosystem means inheriting arbitrary build-time code execution. |

What actually landed is a third thing, and it is better than either: a **sidecar
crate** (`rust-interop.md` §4.1) that is an ordinary Rust crate with its own
`Cargo.toml` and a mandatory checked-in `Cargo.lock`, built with `cargo build
--locked` and vendored for offline builds. That gives Science a *bounded*,
*opt-in*, *content-hashed* window onto cargo, per program rather than per
dependency. §2.6 and §2.7 below reconcile it with the no-build-scripts rule, and
§2.4 puts its `Cargo.lock` hash in Science's lockfile, which is the one thing
that note asked for without naming it.

### 0.1 The native sibling also landed, and its six asks are answered

`native-dependencies.md` was drafted in the same window, and states plainly that
this note *"did not exist when this note was drafted"*. Its §10 makes six asks of
this one. All six are accepted, and each is accepted *in its vocabulary* rather
than in one this note would have invented:

| Its ask | Where answered |
|---|---|
| 1. A `[native.<name>]` manifest table keyed by the `library "<name>"` link name | §4.5, Decision 14a |
| 2. `capability` and `variant` as **resolver inputs**, not metadata | §3.5, Decision 11a |
| 3. A `[[native]]` lock array carrying an explicit `tier`, with per-tier required fields | §2.4, Decision 4 |
| 4. `sciencec fetch` as the only networked command | §6.2, Decision 20 |
| 5. `--locked` semantics admitting tier-dependent enforcement | §2.3, Decision 3 |
| 6. A root-package override of a dependency's `[native.*]` table | §4.3 |

Two places where the two notes would otherwise have collided silently, both
resolved in the sibling's favour, because the contested ground is its half:

- **`--reproduce` is retired.** An earlier draft of this note had `--locked` for
  Science dependency coverage and a second flag, `--reproduce`, for the build
  environment. `native-dependencies.md` §5.3 already defines `--locked` as *"the
  mode for a published artefact, a CI run, or a rerun of a result"*, with
  tier-dependent enforcement. That is the broader and the better meaning; this
  note adopts it and has one strict flag instead of two.
- **`bundle` belongs to that note.** Its Decision 11 makes `sciencec bundle` a
  *relocatable directory* — a binary, the shared objects it needs and a launcher —
  for the reasons its §6.4 gives. This note's source-side artefact is therefore
  named `sciencec archive` (§6.4). The two are different objects: `bundle` is what
  you hand to somebody who wants to *run* the program, `archive` is what you
  attach to a paper so that somebody can *rebuild* it.

Its §1 also makes an argument this note accepts rather than restates: **a native
dependency's identity is not a version**, and a resolver that treats it as one
produces a lockfile that is a lie. That is why §3's algorithm resolves Science
packages only, and why §2.4 records rather than constrains.

---

## 1. The argument for doing it, and for doing it now

### 1.1 Three notes hit this independently, which is the evidence

§12 of the core spec puts the package manager out of scope for F0. That was
defensible when F0 was a lexer and a parser. It has since been hit by three notes
that were not looking for it:

- `scientific-libraries.md` §15 — *"The package manager is out of scope for both
  notes and this is the seam where its absence will hurt most."*
- `stdlib-shape-and-packages.md` §6 — an entire section titled *what versioning
  means with no package manager*, whose §6.4 concludes that **Level 3 is a
  category whose defining property is not implemented**, and recommends saying so
  in the documentation rather than implying an ecosystem users will go looking for
  and not find.
- `data-io.md` §9 — which could not express "Parquet needs Arrow" as a
  dependency, and so invented a *toolchain build feature* (`sciencec --features
  parquet`) instead, with a link-time diagnostic when it is missing.

A fourth hit it obliquely: `syntax-revision-2.md` §8.2, listing the costs of an
inline `rust { … }` block, ends with *"a fragment that wants `serde` needs Cargo,
which means Science needs a package manager to express that, which it does not
have."*

Three notes independently reaching for a mechanism is the standard this project
already uses for its standing cross-note asks (`README.md`, "Standing cross-note
asks"): *two customers, build it once*. This has three, plus a fourth that routed
around it.

### 1.2 What becomes possible the day it exists

`stdlib-shape-and-packages.md` §6.2 lists four failures of the present
arrangement. Each maps to something concrete:

1. **Two versions on one machine.** Resolution is per-project and the build
   directory is per-project, so a user with one project on `db 0.3` and another
   on `db 0.4` has both, with no environment variable to get wrong. Today they
   have *no mechanism at all* — whatever is on `SCIENCE_PATH` is what every
   program on the machine compiles against.
2. **A security patch stops requiring a whole-toolchain upgrade.** Today, a
   `libzstd` advisory means a new `sciencec` tarball, which on a cluster is a
   ticket with a two-week latency. With packages, `compress` moves on its own.
   This is the failure that most directly contradicts the argument
   batteries-included was chosen for, and §6.2 of that note names the cut.
3. **A methods section can say what a result was produced with.** `science.lock`
   plus `sciencec archive` (§6.4) is a single artefact that names every source
   hash, the compiler, the target and the native libraries that were linked. This
   is the one that matters most for the audience, and §2 makes it the thesis.
4. **A third party can depend on a Level 3 package**, so the ecosystem
   `stdlib-shape-and-packages.md` §1.2 hoped the standard library would seed
   reaches further than one layer.

Plus three that note does not list:

5. **`data-io.md` §9's toolchain feature is deleted.** `--features parquet` is
   compiler surface area invented to work around a missing dependency mechanism.
   That note already pre-committed to the migration — *"When the package manager
   arrives, this becomes an ordinary dependency and the module path does not
   change"* — and §9 asks only one thing back (§10, ask 6).
6. **`rust-interop.md`'s curated shim crates become packages instead of tarball
   contents.** That note's §6 proposes four maintained shims (`regex`, `flate2` +
   `zstd`, `serde_json`, a blocking HTTP client) shipped *in the toolchain*, and
   its §2.6 records the resulting wart: the toolchain's own `Cargo.lock` becomes a
   release artefact, and one machine can hold two versions of `regex`. Shipping
   each shim as a Science package moves its `Cargo.lock` inside that package's
   content hash, where it belongs.
7. **The `foreign/manifest.toml` of `rust-interop.md` §4.1 stops being a
   second, parallel manifest format.** It becomes a table in `science.toml`.

### 1.3 What delaying costs, stated as a rate rather than a lump

The cost of delay is not that users are annoyed. It is that **the standard
library absorbs the package manager's job, and absorption is irreversible.**

`stdlib-shape-and-packages.md` §1.2 argues for a large Level 2 with an argument
this note fully accepts: *"Anything not in the tarball the user `scp`s to the
cluster effectively does not exist."* That argument is correct **and it is
strictly a function of there being no package manager.** Every month without one,
the correct answer to "should this be in Level 2?" shifts further toward yes,
because the alternative is nothing. And Level 2 is versioned with the toolchain
**forever**: moving a module out of Level 2 is a breaking change to every program
that `use`s it, so the ratchet turns one way.

That is the compounding cost, and it has a second head. §12 of the core spec
already names environment reproducibility as *"the single most-cited reason
scientists trust results across machines"* — and then defers the only mechanism
that could deliver it. Every result computed with a pre-package-manager Science
is a result that cannot be pinned. Those results do not get retrofitted.

### 1.4 The decision, which is a split rather than a yes

> **Decision 1. Build the manifest, the module search path and the lockfile
> *format* in F0. Build resolution, git dependencies and offline builds in F1.
> Build the registry no earlier than F2, and not until there is an ecosystem
> asking for it.**
>
> **This is not "build a package manager in F0".** §12 of the core spec is right
> that a registry with signing and a resolver is not F0 work, and nothing here
> disputes it. What it is wrong about is the implied atomicity: the three pieces
> have wildly different costs and exactly one of them is a one-way door.
>
> **Rejected: keep the whole thing out of F0** (the status quo). Rejected because
> `stdlib-shape-and-packages.md` §6.3 already identified two preconditions that
> *cannot be retrofitted* — the search path and a per-directory manifest — and
> arguing they are out of scope means arguing to build them wrong first. That
> note calls them "the parts of one that cannot be added afterwards" and it is
> correct.
>
> **Rejected: build the whole thing now**, registry included. A registry is a
> *service*, not a feature: uptime, moderation, abuse handling, a legal entity,
> and a cost line that never ends. Shipping one before there are packages to host
> is how a language acquires an obligation it cannot meet. §11 records it as the
> largest risk in the note.
>
> **Cost of the split, stated.** There will be a period — F1 — in which Science
> has a real dependency mechanism with no registry, so every dependency is a git
> URL or a path. That is Go 2015 and it was survivable. It is *worse* than
> nothing in exactly one respect: it creates lockfiles in the wild pointing at git
> revisions, and §5.4's yank semantics have no authority over a repository
> somebody deletes. §2.5's vendoring is the answer and it must ship in the same
> phase, not after.

---

## 2. Reproducibility is not a feature here; it is the thesis

This section shapes the rest of the note, so it comes before the mechanism.

### 2.1 Why the bar is higher for this language than for most

§1 of the core spec bets the language on **verification** — *"the mistakes a
compiler can catch before a run starts"* — and names three targets: static
shapes, units, and **random keys that cannot be reused**. The third one is not a
type-system property in the way the other two are. It is a *reproducibility*
property: a `Key`'s output must be the same number on every machine, forever.

`stdlib-standard.md` §5.5 takes that seriously to a degree that is unusual. The
generator algorithm, the counter layout, `split`'s exact derivation, the
bits-to-float mapping and a set of shipped test vectors are all declared part of
the language's published contract, and *"a new generator is a new type, never a
new version of `Key`"*. It even rejects Box–Muller for `normal()` on the grounds
that a rejection method consumes a variable number of words and so makes a draw
depend on history.

A language that will not let `normal()` change its sampling method, and then
ships a build system that silently picks up whatever `stats` was published this
morning, is not making one strong claim and one weak one. It is contradicting
itself. **A package manager that cannot reproduce a build exactly is off-brand
for Science in a way it would not be for a web framework**, and the design has to
be read in that light rather than as a set of nice-to-haves.

Here is the shape of the problem in source. Nothing in this program is unusual;
every line of it is a thing that has to come back the same next year.

```science
use data.csv (read_csv)
use random (Key)
use spectra.fit (Fit, fit_peaks)

function run(path: String) -> (Fit, Error?):
    let frame, err be read_csv(path)
    if err?:
        return (Fit.empty(), err)

    let key be Key.from_seed(20260916)
    let fit, err be fit_peaks(frame.column("counts"), key)
    if err?:
        return (Fit.empty(), err)

    if fit.residual <= 1.0e-6 and fit.label?:
        print(f"{fit.label} converged in {fit.iterations} steps")

    return (fit, null)
```

Four separate things must be pinned for that to give the same answer in 2031:
`data.csv`'s parser (toolchain), `random`'s generator (frozen by contract),
`spectra`'s source (a package), and whatever BLAS `fit_peaks` ends up calling
(the system). The language has an answer for the second. This section supplies
answers for the first, third and fourth, and is honest about which of them is a
guarantee.

### 2.2 A lockfile is table stakes, and it is not enough

> **Decision 2. Every dependency is identified in `science.lock` by the
> **SHA-256 of its canonical archive**, not by a version number and a source
> URL. The version is metadata. The hash is the identity.**

The canonical archive is a deterministic tar of the package's source: entries
sorted by path, mtimes zeroed, modes normalised to `0644`/`0755`, no
platform-specific attributes, no compression in the hashed form. This is
well-trodden — it is what Go, Nix and Sigstore all do — and the determinism rules
are the part that must be written down once and never changed.

What content addressing buys that a version-plus-URL lock does not:

- **A registry compromise cannot change what a locked build compiles.** With a
  version lock, "fetch `linalg 0.7.3` from the registry" trusts the registry every
  time. With a content lock, a swapped artefact is a hash mismatch (`SP0021`) and
  the build stops.
- **A git dependency, a path dependency and a registry dependency become the same
  kind of thing.** All three produce source; source hashes. A moved git tag is
  detected the same way a swapped registry artefact is.
- **A mirror is trivially correct.** §6.5's site-local registry does not need to
  be trusted, because the lock already says what the bytes must be. This is what
  makes the HPC story cheap rather than a second security design.
- **It is the same primitive the supply-chain section needs** (§5.3), so the
  reproducibility machinery and the security machinery are one mechanism. That is
  the most useful fact in this note.

**Cost.** Hashes are long and lockfiles become unreadable to humans. Accepted:
nobody reads a lockfile, and `sciencec why` (§8.4) exists for the questions people
actually ask of one. A second cost is real: the canonicalisation rules are a
compatibility surface, and getting them wrong once (a mode bit, a symlink, a
Windows line ending) means every hash in existence is invalid. It must be
specified and tested from the first commit, in the same spirit as
`stdlib-standard.md` §5.5's test vectors.

### 2.3 Does the compiler version belong in the lock?

**Yes, and the interesting question is what happens on a mismatch.**

It belongs because Science compiles through LLVM with monomorphization, and
codegen is not reproducibility-neutral. FMA contraction, vectorisation width, the
order of a reduction, and the LLVM version itself all change floating-point
output. A user who pins every source hash and then upgrades `sciencec` has
changed their numbers, and nothing told them.

> **Decision 3. `science.lock` records a `[build]` table: the `sciencec`
> version, the LLVM version, the target triple, the optimisation level, and the
> hash of the toolchain's own `Cargo.lock` when the toolchain embeds Rust
> (`rust-interop.md` §2.6). A mismatch is a **warning** by default and an
> **error** under `--locked` (`SP0022`).**
>
> **`--locked` is the single strict mode**, with the meaning
> `native-dependencies.md` §5.3 gives it — *"the mode for a published artefact, a
> CI run, or a rerun of a result"* — and with that section's tier-dependent
> enforcement for the native half. Its ask 5 is granted by adopting its
> definition wholesale rather than by adding a second flag beside it (§0.1).
>
> **Rejected: leave the toolchain out of the lock.** It is what cargo does — a
> `Cargo.lock` says nothing about rustc — and it is defensible for software whose
> output is behaviour. It is not defensible for software whose output is a number.
>
> **Rejected: hard-error on any mismatch.** A lockfile that refuses to build after
> a routine compiler upgrade is a lockfile people delete. The default must be
> usable during ordinary development or the mechanism gets worked around, and a
> worked-around mechanism protects nobody.
>
> **Cost.** Two modes to document, and a warning that people will learn to ignore.
> Mitigated by making `--locked` the documented mode for anything attached to a
> publication, and by `sciencec archive` (§6.4) recording the `[build]` table into
> the artefact so that the intended environment travels with the code.

`rust-interop.md` §2.6 independently arrived at half of this: it notes that the
toolchain's `Cargo.lock` *"becomes a release artefact that must be published
alongside the version number, because it is now part of what a result was
produced with"*. Decision 3 is where that artefact is recorded on the consuming
side.

### 2.4 Do the native libraries belong in the lock?

`ffi-c-boundary.md` §5 owns the mechanism for finding them and
`native-dependencies.md` owns providers, ABI variants and what a native lock entry
can honestly say. The **file it is written in** is this note's, and the
requirement is uncomfortable.

> **Decision 4. `science.lock` carries a `[[native]]` array whose entries have a
> **tier** field, with different required fields per tier, exactly as
> `native-dependencies.md` Decision 8 specifies. The lockfile schema does **not**
> force a native entry into the same shape as a Science package entry.**
>
> That last clause is its ask 3, and the reason it gave for asking is the right
> one: *"a lockfile schema that forces a native entry into the same shape as a
> Science entry will force Tier B to fabricate a version."* A fabricated version
> is worse than an honest absence, and it is the specific failure §2.5's tier
> table exists to avoid.
>
> Its three tiers, restated only far enough to make this note readable on its own:
> **Tier A** (`prebuilt`, `source`, `vendored`) is content-addressed and behaves
> like a Science dependency; **Tier B** (`system`) records what was *found* —
> path, `SONAME`, file hash, probe string, resolution time — and constrains
> nothing; **Tier C** (`when available`) can record only that the `dlopen` table
> exists and what it expects.
>
> The **Science-side requirement** this note adds on top: Tier B's recorded hash
> is compared on every later build, and a difference emits a warning naming the
> library, both probe strings and both hashes. Under `--locked` it is an error.
> That is `native-dependencies.md`'s `SC0479`; this note allocates no code for it
> and `SP0023` is released to the free pool.
>
> **Rejected: constrain the native libraries, i.e. refuse to link a `libopenblas`
> whose hash differs from the lock.** This fights the design on purpose.
> `ffi-c-boundary.md` §5.1 makes dynamic linking the default *specifically*
> because *"HPC module systems work by swapping the shared object under a fixed
> name — statically linking OpenBLAS defeats the mechanism the cluster
> administrator is relying on."* A lockfile that forbids the swap forbids the
> supported deployment.
>
> **Rejected: record nothing**, on the grounds that it cannot be guaranteed
> anyway. This is the real temptation and it is wrong. The difference between "the
> BLAS changed and nobody knows" and "the BLAS changed and the build said so" is
> the entire difference between a reproducibility failure and a reproducibility
> *finding*. Making a substitution **visible** is achievable; making it impossible
> is not.
>
> **Cost.** Hashing every linked shared object on every build. On a login node
> with a cold NFS cache and a 200 MB MKL, that is seconds. Mitigated by caching
> the hash keyed on `(path, size, mtime, inode)` and rehashing only on a change —
> the same trick every build system uses, with the same small hole.

The sidecar's `Cargo.lock` hash (`rust-interop.md` §4.2) is recorded in the same
table, alongside the sidecar directory's content hash. That note already keys its
`foreign_archive` query on both; this decision only asks that the same two values
be written down where a human and a reviewer can see them.

### 2.5 The strongest honest guarantee, in three tiers and one disclaimer

This is the part not to oversell. Science's reproducibility promise is:

| Tier | Claim | Strength |
|---|---|---|
| **1. Source identity** | Given `science.lock`, the exact bytes of every Science source file compiled are the same, or the build fails. | **Guaranteed.** Content addressing, §2.2. |
| **2. Build identity** | The compiler, LLVM, target and flags are recorded, and `--locked` refuses to proceed if any differs. | **Guaranteed given the same toolchain artefact**, which is a tarball the user can keep. §2.3. |
| **3. Link identity** | Every native library that was linked is recorded at its tier: pinned by hash at Tier A, described at Tier B, expected at Tier C. | **Guaranteed at Tier A; observed, not guaranteed, at Tier B; reported by the run itself at Tier C.** §2.4, and deliberately so. |

Tier 3 is `native-dependencies.md` §5.2's sentence, and that note's wording is the
canonical one: *"Science can guarantee that a build is reproducible up to its Tier
A dependencies and the host's C library and compiler; for Tier B dependencies it
guarantees only that what was used is recorded, and for Tier C only that what was
loaded is reported by the run itself."* This note states the same thing from the
lockfile's side and defers to that one on every detail.

And the disclaimer, which must travel with the three tiers rather than be found
later:

> **None of this guarantees the same floating-point result.** A binary built
> from identical sources against an identical OpenBLAS will still dispatch a
> different kernel on a machine with AVX-512 than on one without, and a different
> reduction order gives a different last bit. Science does not fix this and no
> language does. What Science fixes is narrower and worth having: **the random
> stream is bit-exact by contract** (`stdlib-standard.md` §5.5), **the source is
> bit-exact by lock**, and **everything else is written down**.

A build that satisfies tiers 1–3 is *bit-reproducible on the same machine* and
*auditable across machines*. Claiming more would be the kind of claim §1 of the
core spec exists to beat competitors on, and losing it on a technicality would be
worse than never making it.

### 2.6 No build scripts. This is the largest single decision in the note.

> **Decision 5. A Science package cannot execute code at build time. There is no
> `build.science`, no `build.rs` equivalent, no `postinstall`, no hook of any
> kind. Everything a package needs at build time is *declared* in the manifest
> and *performed* by `sciencec`.**

This forecloses cargo's `build.rs`, npm's `postinstall` and Python's `setup.py`,
which between them account for a large fraction of every real supply-chain
incident in those ecosystems, and for most of what makes their builds
irreproducible. It is worth the whole section because it is the decision that
makes §5.3's security story short instead of long.

**What a build script is actually used for**, and where each goes:

| Use | Where it goes in Science |
|---|---|
| Compile a vendored C file (`data-io.md` §3's `miniz`) | `[native] sources = [...]`, a declarative list `sciencec` compiles. Bounded, common, supported. |
| Find a system library, emit link flags | `ffi-c-boundary.md` §5.1's `library` / `via pkg-config`, already declarative. |
| Probe for an optional library | `ffi-c-boundary.md` §5.2's `when available`, which is a *runtime* check and strictly better. |
| Generate bindings (`bindgen`, `protoc`) | **Not supported.** Check the generated file into the repository. |
| Download something | **Not supported, ever.** |
| Set `cfg` flags from the environment | **Not supported.** There are no feature flags either (§3.4). |

> **Rejected: build scripts with a capability sandbox** (no network, no
> filesystem outside the package). Attractive, and it is where Deno and modern
> npm proposals have gone. Rejected for F0–F2 on cost: a sandbox that is actually
> a sandbox is a security boundary with a threat model, on three platforms, and
> it is a bigger project than the resolver. Rejected also because the declarative
> list covers the honest cases, and the case it does not cover — codegen — has a
> working answer that costs a `git add`.
>
> **Rejected: allow build scripts, unsandboxed, like cargo.** It is the path of
> least resistance and it would end this note's §5 as a serious document. A
> language whose users publish numbers cannot have `sciencec add` mean "run a
> stranger's code".
>
> **Cost, stated plainly and not minimised.** Checking a generated file into a
> repository is genuinely worse than generating it: it goes stale, it bloats
> diffs, and it makes a `.proto` change a two-step process. Some ecosystem will
> find that unacceptable within a year, and §11 records it as a bet rather than a
> certainty. A second cost: without a build script there is no way to compile a C
> file whose flags depend on a probe, so a package that needs `-DHAVE_SSE4` must
> either always pass it or do the check at runtime. The runtime check is the
> better engineering anyway; it is not always available.

### 2.7 The sidecar is the one exception, and it must be visible

`rust-interop.md` Decision 8 deliberately keeps the sidecar's `Cargo.toml` under
the user's control precisely so that *"cargo features, `[patch]`, workspaces and
`build.rs` remain available, and `sciencec` does not have to model any of them."*
That is right for the reason it gives — a generated manifest is not a crate the
Rust ecosystem can see — and it means **a Science program that contains a
`foreign/` directory does execute arbitrary build-time code**, in the form of
whatever `build.rs` its crates carry.

The two decisions are compatible, and the seam is worth naming rather than
leaving for someone to find:

> **Decision 6. Decision 5 binds Science packages. The Rust sidecar is an
> explicit, bounded exception, and it is made visible in three places: the
> package's manifest declares `sidecar = true`, `science.lock` records the
> sidecar directory's content hash and its `Cargo.lock` hash (§2.4), and
> `sciencec add` prints a one-line notice when a package being added carries a
> sidecar.**
>
> The reasoning for the asymmetry: a sidecar is **opt-in per program**, its source
> is **in the repository** where code review sees it, and cargo's own
> `--locked` + `cargo vendor` story (`rust-interop.md` §4.2) already gives it
> content pinning. A registry dependency has none of those three properties. The
> exception is therefore about *visibility*, not about trusting Rust more.
>
> **Cost.** A Science package published to the registry that carries a sidecar is
> a package whose installation runs `build.rs`. §5.3 handles this by refusing
> sidecars in published packages until F2's review process exists, which makes the
> curated shims of `rust-interop.md` §6 a *toolchain* artefact for longer than
> §1.2 item 6 hoped.

### 2.8 Two small things that make binaries bit-reproducible

Cheap, and they are the difference between "reproducible in principle" and
"`sha256sum` matches".

> **Decision 7. The compiler embeds no build timestamp, no hostname, no user name
> and no absolute source path in an output binary by default. Source paths are
> remapped to package-relative form; `--embed-build-info` opts back in.**

Rejected alternative: embed them, as most toolchains do, because a debugger wants
absolute paths. Answered by a debug-info path map rather than by giving up
determinism. Cost: a stripped-down default that surprises anyone expecting
`__FILE__` to be absolute, and one more flag.

**This does not contradict `native-dependencies.md` Decision 9**, which emits a
`.science.provenance` section into every binary containing, among other things,
Tier B's resolved absolute paths. The distinction is between *incidental* and
*deliberate* non-determinism. Decision 7 bans facts that are about the machine and
not about the build — a clock reading, a user name, the compiler's own source
tree. Decision 9 records facts that are about the build and that a scientist needs,
and it is deterministic *given the same resolution*: two builds on two machines
that resolved different BLAS files produce different provenance sections because
**they are different builds**, and a binary hash that concealed that would be
concealing the thing §2.4 exists to surface. Same-machine bit-reproducibility,
which is what Decision 7 is for, survives intact.

---

## 3. Resolution

### 3.1 Minimal version selection, not a SAT solver

> **Decision 8. Version resolution is **minimal version selection** (Go's
> algorithm). A dependency declares a *minimum*; the selected version of a
> package is the maximum of the minimums required anywhere in the graph. There
> are no ranges, no carets, no tildes, and no solver.**

The case for MVS in *this* language, in order of how much it matters:

1. **It is a pure function of its inputs.** No heuristics, no tiebreaks, no
   "the resolver preferred this". For a language whose §1 bet is verification and
   whose §2.1 above makes reproducibility the thesis, a resolution algorithm whose
   output cannot be explained as a function of the manifests is the wrong shape.
   MVS is reproducible *without* a lockfile — Science ships one anyway, for
   content hashes and native observations (§2), not for version selection.
2. **Its failures are explainable.** §8 requires that a resolution failure render
   at the same quality bar as a compiler diagnostic. MVS's only failure mode is
   *"package P requires at least version V of Q, and V does not exist"* — a single
   constraint, with a single owner, at a single line of a single manifest. A SAT
   resolver's failure is an unsatisfiable core, and minimising one into a readable
   sentence is a research-grade problem; pubgrub exists because cargo's and pip's
   messages were unreadable, and pubgrub's output is good rather than good enough.
3. **You do not get an upgrade you did not ask for.** For this audience that is
   the point, not the cost. A build that changes because a transitive dependency
   published a patch this morning is a build that changed for a reason the user
   cannot see, which is the thing §2 exists to eliminate.
4. **It is a few hundred lines** with no backtracking and a trivial termination
   argument, and it is a graph walk, which means it is a `salsa` query like
   everything else in §3 of the core spec.
5. **`stdlib-shape-and-packages.md` §1.2 already names the alternative as part of
   the pain the audience is escaping** — *"a conda environment that takes forty
   minutes to solve and breaks when the cluster's MPI is swapped"*. Choosing the
   same class of algorithm would be choosing that experience.

> **Rejected: SAT / PubGrub resolution** (cargo, pip, uv, conda). It is more
> expressive: it can find that `A 1.2` and `B 3.0` are jointly satisfiable only
> by downgrading `C`. Rejected because the expressiveness buys a behaviour —
> automatic selection of the newest compatible version — that this note does not
> want, at the price of the two properties it wants most (explainability and
> determinism), and at an implementation cost that would make resolution the
> hardest part of the tool.
>
> **Rejected: no resolution at all** — pin every dependency exactly, transitively,
> by hand (the "lockfile only" model). It is even simpler and it does not compose:
> two dependencies pinning different patch versions of the same package is an
> unresolvable conflict with no principled fix.

**The costs of MVS, none of which are hidden:**

- **No ranges.** `linalg = "0.7.0"` means *at least 0.7.0, and the same major*. A
  user arriving from cargo will write `"^0.7"` and must be told, by `SP0005`, that
  the caret means nothing here and that the plain version already means what they
  wanted.
- **Security patches do not propagate by themselves.** If your graph selects
  `linalg 0.7.0` and `0.7.3` fixes an advisory, you stay on `0.7.0`. Two answers,
  and together they are arguably better than silent upgrade: `sciencec update`
  raises minimums *in the manifest*, so the change is a diff in version control
  rather than an invisible event; and `sciencec audit` (§5.3) reports selected
  versions under advisory and is the mechanism by which you learn.
- **Manifests must be fetchable for versions that are not selected**, because MVS
  reads the requirements of the version it picks, transitively. Offline builds
  therefore need manifests in the vendor directory, not only sources. §6.3 handles
  it; it is a real constraint on the vendor format.
- **MVS relies on semver holding more than SAT does**, because there is no
  backtracking to a different version when a minor release breaks you. That is
  uncomfortable — and it is exactly why §3.2's machine-checked semver is worth
  more here than it would be in a cargo-shaped world. The two decisions hold each
  other up.

### 3.2 Semver: declared by the author, computed by the compiler, gated at publish

Science can do something Python and JavaScript structurally cannot. It is
statically typed and compiled, its `public` items are exactly its API surface,
and the compiler already knows how to emit canonical textual dumps of its own
data structures — that is what `crates/science-parser/src/dump.rs` and the
snapshot suite are.

> **Decision 9. A published package carries an **API signature file**: a
> canonical, sorted, textual rendering of every `public` item's type. `sciencec
> publish` diffs it against the previous release's, computes the **minimum legal
> version bump**, and refuses a publish that claims less (`SP0050`). The check is
> **conservative** — it refuses only unambiguous breakage — and it runs **at
> publish only**, never at compile time and never for a `git` or `path`
> dependency.**

**What it catches**, which is most real breakage in a statically typed language:
a removed `public` function or type; a changed parameter or return type; a
changed field type or a removed field on a public record; a removed `choice`
arm; a narrowed generic bound; an interface gaining a required method with no
default; a method moving between `has` and an `implements` block.

**What it cannot catch, and this is the honest half:**

> **It is blind to behaviour**, which for this audience is the breakage that
> matters most. A `sort` that becomes unstable, a solver that changes its
> convergence criterion, a `normal()` that switches from the inverse CDF to the
> ziggurat — `stdlib-standard.md` §5.5's entire section is about exactly this
> class of change, and every instance of it is a patch release under a
> machine-checked semver. **Machine-checked semver checks the half of semver that
> was never the problem.**

That is not an argument against building it. It is an argument against *marketing*
it, and against ever letting a green check on a version bump be read as "this
release is safe".

**Where it is a trap, and the price of each:**

- **`match` is exhaustive (core spec §4.5), so adding a `choice` arm is
  breaking.** Every new error variant in a library's `ConfigError` becomes a major
  version. That is correct by the letter and intolerable in practice; it is why
  Rust has `#[non_exhaustive]`. §10 asks the core spec for an **open `choice`**,
  declared by the author, which requires a catch-all arm in every downstream
  `match`. Without it, Decision 9 makes error types unevolvable and should not
  ship.
- **Defining API equality precisely is where `semverver` died in Rust.** Generics,
  inference, defaulted methods and auto-derived interfaces all make "the same
  type" a judgement call. Mitigation is the conservatism in the decision: the
  checker refuses only when it is *certain*, warns when it suspects, and is silent
  otherwise. A false negative costs a wrong version number; a false positive
  costs a support ticket, so the asymmetry is chosen deliberately.
- **It needs the previous release**, so it works only against a registry and is
  therefore an F2+ feature by construction.
- **The API dump format becomes a compatibility surface of its own.** If its
  rendering changes, every stored signature is invalid. Same class of problem as
  §2.2's canonicalisation and it takes the same discipline.

> **Rejected: enforce semver at compile time**, i.e. refuse to build against a
> dependency whose API disagrees with its declared version. It sounds stronger and
> it is worse: it turns a publisher's mistake into every consumer's broken build,
> and it puts a heavyweight analysis in the hot path of every compile.
>
> **Rejected: semver as pure convention**, checked by nobody (npm, PyPI). It is
> what the ecosystems with the worst dependency pain do, and MVS in particular
> cannot afford it.

### 3.3 Duplicate versions: no, and a major bump renames the package

> **Decision 10. One version of a package name per build graph. A major version
> of 2 or higher is part of the package name — `linalg2` — so two majors can
> coexist, but only under distinct names that are visible in the source.**

This is Go's `/v2` rule, adapted to §4.7's one-lowercase-word module naming.
`use linalg (solve)` and `use linalg2 (solve)` in one program is legal and
obvious; two `linalg`s at two versions in one lockfile is not expressible.

Reasons, in order:

1. **MVS produces one version by construction.** Permitting duplicates means
   reintroducing the thing the algorithm exists to avoid, and then explaining
   when it happens.
2. **cargo's rule is a type error at a distance.** cargo allows two majors unless
   they meet in a public API, and when they do, the message is the famous
   *expected `Url`, found `Url`*. §3 of the core spec chooses its whole syntax for
   *"an audience that is not primarily made of systems programmers"*; that error
   is unaffordable for them.
3. **Monomorphization plus duplicates is binary size**, and §3 promises
   self-contained binaries, so the duplication is a thing the user downloads.
4. **It preserves `stdlib-shape-and-packages.md` §1.2's interop argument.** That
   note's rule is that *"every type that appears in more than one library's
   signatures must be at Level 1 or Level 2 or the ecosystem cannot be an
   ecosystem."* A type existing twice at two versions breaks interop in precisely
   the way that argument is trying to prevent — and under Decision 10 it still
   can, but the two types have *different names*, so the compiler's message is
   `linalg.Matrix` versus `linalg2.Matrix` and a reader can act on it.

> **Rejected: cargo's rule** (two majors unless they meet in a public API). It
> buys a genuinely valuable thing — an ecosystem can migrate to 2.0 gradually
> instead of all at once — and Decision 10 buys the same thing more honestly, by
> making the transitional duplication a visible name rather than an invisible
> lockfile entry.
>
> **Cost.** `use linalg2 (solve)` is ugly, and so is a package called `arrow3`.
> Accepted. Second cost: a major bump is a *rename*, so every consumer edits every
> `use` line, which is more friction than cargo's `linalg = "2.0"`. That friction
> is the correct price signal — a major version is supposed to be expensive — but
> it is friction, and a `sciencec fix --major linalg` rewrite is the obvious
> mitigation and is not budgeted here.

### 3.4 Feature flags: out

> **Decision 11. There are no feature flags. A package's compilation does not
> depend on who is consuming it.**

cargo's `features` are genuinely loved and they are also the main source of its
resolution complexity and of the *works-in-my-build* class of bug. The mechanism
that causes both is **feature unification**: a package is compiled with the union
of the features any consumer requested, so adding an unrelated dependency to your
project can change the behaviour of a dependency you already had. In a language
whose §2 makes reproducibility the thesis, a dependency whose compiled form is a
function of the rest of the graph is a reproducibility bug wearing a feature's
clothes.

Two further reasons specific to this design:

- **Features are what turn MVS back into a SAT problem.** The version graph and
  the feature graph are not independent; solving them together is the expensive
  part of cargo's resolver. Go has no module-level feature mechanism and this is
  why.
- **The one live use case is already solved better.** The case features actually
  serve for this audience is an optional heavyweight native dependency —
  `data-io.md` §9's Arrow, a CUDA path. `ffi-c-boundary.md` §5.2's `when
  available` handles it as a **runtime** capability with a generated
  `is_available()`, which is strictly better: it is one binary that works on a GPU
  node and a CPU node, instead of two builds that must be kept straight. A
  compile-time combinatorial mechanism is the wrong shape for a problem whose
  real variability is at run time on a heterogeneous cluster.

> **Rejected: cargo-style features with unification.** Non-local behaviour;
> resolution complexity; the reproducibility objection above.
>
> **Rejected: features without unification**, i.e. each consumer gets its own
> compilation of the dependency. That is duplicate versions under another name and
> Decision 10 already refused it.
>
> **Cost, stated.** A package that could *optionally* use `crypto` must either
> always depend on it or split into two packages, and splitting is the thing cargo
> users complain about when a crate does it. Second cost: without `cfg(feature)`
> there is no source-level way to compile a path out, so binaries carry code that
> is never called. LLVM's dead-code elimination recovers much of it and `when
> available` covers the link-time half, but "much" is not "all".
>
> **The reopening condition, named in advance.** If `when available` proves
> insufficient for a real case — a dependency that must be *absent from the
> graph*, not merely unlinked — the minimal thing to add is an `optional = true`
> dependency enabled by the *root* package only, with no transitive enabling and
> therefore no unification. That is a strictly smaller feature than cargo's and it
> is where this decision should relent if it must. §11 records the pressure.

### 3.5 The one thing the resolver checks that is not a version

`native-dependencies.md` §10 ask 2 is the sharpest request either sibling made,
and it is granted:

> **Decision 11a. `capability` and `variant` (§4.5) are **resolver inputs**, not
> metadata on a resolved graph. Two packages in one graph that require the same
> capability at incompatible variants — `blas` at `lp64` and `blas` at `ilp64` —
> are a **resolution failure**, reported with the dependency edge that introduced
> each, not a link-time or run-time surprise.**

This is cargo's `links` key, and the reason to take it is the reason that note
gives: if the check runs after resolution, its diagnostic cannot say *which
dependency edge* caused the conflict, and a user staring at
`ffi-c-boundary.md` §1.6's failure mode — **correct answers for small matrices and
silently wrong ones for large** — deserves better than a post-hoc assertion.

**What it costs the algorithm.** MVS stays a version algorithm; this is a second,
independent check over the same graph walk, and it does not backtrack. If two
requirements conflict, the resolution *fails* — it does not try a different
version of one of them to make them agree. That is a real limitation compared to a
SAT resolver, which could search for a combination that agrees, and it is accepted
for §3.1's reasons plus one more: a graph where a *different version* of `stats`
would have wanted a different BLAS variant is a graph whose numerical behaviour
depends on version selection, and silently finding such a combination is worse
than reporting the conflict.

**What it costs the manifest.** The capability and variant of a native dependency
must be readable *without building anything*, which §4.1's "the manifest is data,
not code" already guarantees, and must be present in the vendored manifests of
§6.3, which is one more reason vendoring copies manifests and not only sources.

---

## 4. The manifest, and the bootstrapping problem

### 4.1 The bootstrapping claim, checked

`stdlib-shape-and-packages.md` §5.2 moved TOML *up* from Level 3 to Level 2 for
this reason:

> If TOML is the manifest format for the package manager Science does not yet
> have — and it is the obvious choice — then it cannot be delivered *by* that
> package manager. A package manager whose manifest parser is a package is a
> circular dependency.

**The conclusion is right. The stated reason is not the binding one, and the real
one is stronger.** `sciencec` is written in Rust (`crates/science-lexer`,
`crates/science-parser`), so the manifest parser the *compiler* uses is a Rust
crate and was never going to be a Science package. The circularity that section
describes would not have bitten.

The binding reasons are two, and both survive:

1. **Science tooling must be able to read what the compiler reads.** A linter, a
   CI script, a lab's own bookkeeping, and `data-io.md`-shaped analysis of a
   directory of projects are all Science programs that need to parse
   `science.toml`. If the format's only parser is inside `sciencec`, every such
   program shells out to the compiler. That is the real circularity and Level 2 is
   the real fix.
2. **The manifest format must be freezable independently of Science's release
   cycle.** TOML 1.0 is frozen and deliberately unextensible, which is the
   property being bought. A format that evolved with the language would make
   old manifests unparseable, which is §2's problem by another route.

> **Decision 12. The manifest is TOML, named `science.toml`, at the package root.
> The compiler's own parser is **normative**; `data.toml` (Level 2) is tested
> against the same conformance corpus, and a divergence between them is a bug in
> `data.toml`, never a second dialect.**
>
> **Rejected: JSON.** No comments, and a manifest is hand-edited. A dependency
> list nobody can annotate is a dependency list nobody maintains.
>
> **Rejected: YAML.** `stdlib-shape-and-packages.md` §5.2 catalogues why — an
> eighty-page specification, type-resolution rules that make `NO` a boolean, and
> the single clearest case in that note of an algorithm that must change for
> security. It is also Level 3, which *would* be the circular dependency.
>
> **Rejected: a Science program, `build.science`.** This is `build.rs`, and §2.6
> already refused it. The deeper reason is that **the manifest must be data, not
> code**: an offline mirror, `sciencec audit`, a registry index and a
> security reviewer all need to know a package's dependencies *without running
> anything*. A manifest that is a program cannot be read, only executed.
>
> **Rejected: a new, minimal, Science-flavoured format.** Nothing in the world
> has an editor mode for it, and the only thing it would buy is not depending on
> TOML, which Level 2 already delivers.

Nothing in Decision 12 contradicts `rust-interop.md` Decision 8's refusal to
generate a `Cargo.toml`. That refusal is about **Rust's** manifest, which must
stay the user's so that cargo's tooling can see it. §4.5 folds the sidecar's
Science-side manifest — `rust-interop.md` §4.1's `foreign/manifest.toml`, which
carries names, handles and `Share` claims — into `science.toml`, and leaves
`Cargo.toml` entirely alone.

### 4.2 The manifest, concretely

```toml
[package]
name        = "spectra"
version     = "0.4.1"
language    = "0.1"
description = "Peak fitting for time-of-flight spectra"
license     = "BSD-3-Clause"
entry       = "src/main.science"     # §4.6
modules     = "src"                  # §4.4
sidecar     = false                  # §2.7

[dependencies]
linalg = "0.7.0"                                    # at least 0.7.0, major 0
stats  = { version = "0.3.2" }                      # the same thing, long form
plot   = { git = "https://git.example.org/plot.science", rev = "9f2c1a…" }
shared = { path = "../shared" }

[[binary]]
name  = "fit"
entry = "src/bin/fit.science"

[native.openblas]                                   # keyed by the link name, §4.5
capability = "blas"
variant    = "lp64"
providers  = ["system", "prebuilt"]
pkg-config = "openblas"                             # ffi-c-boundary.md §5.1
kind       = "dynamic"

[native.sources]                                    # declarative C, §2.6
files   = ["vendor/miniz.c"]
include = ["vendor"]
```

**`[package]` keys.** `name` and `version` are required; everything else has a
default. `language` records the language revision the source conforms to. F0
records and compares it and implements no multi-revision support — this is
`stdlib-shape-and-packages.md` §6.3's argument applied once more: **recording it
costs a line and not recording it means that when a revision arrives, no artefact
in existence can say which syntax it was written in.** The language has had two
syntax revisions in one day; the probability of a third is not small.

**`[dependencies]` value forms.** A bare string is `{ version = "…" }`. Exactly
one of `version`, `git` or `path` may be present (`SP0006`). A `git` dependency
requires `rev` to be a full commit hash in the *lockfile*; a `branch` or `tag` in
the manifest is resolved once and pinned, and the lock's content hash (§2.2) is
what detects a moved tag. **A `git` dependency is a pin, not a constraint**: MVS
has no version set to maximise over, so two packages depending on the same name
at two different revisions is an error (`SP0012`), not a resolution.

### 4.3 What the manifest deliberately does not have

- **No build script key** (§2.6).
- **No `[features]`** (§3.4).
- **No `[profile]`.** Optimisation level is a command-line property of the *build*,
  not of a package, and letting a dependency specify its own profile is how cargo
  ends up with a debug build containing a release-optimised crate. It is also a
  §2.3 lock field, and a field that two packages can both set is a field that can
  conflict.
- **No `[patch]` / `[replace]` for Science dependencies.** The equivalent is a
  `path` dependency in the root package's manifest, which is visible at the top
  of the tree instead of buried. cargo's `[patch]` is a genuinely useful tool for
  a monorepo and its cost is that a build's dependency graph is not a function of
  the manifests it names; §3.1 item 1 will not pay that.
- **One exception: the root package may override a dependency's `[native.*]`
  table**, and only the root package, and only for native entries. This is
  `native-dependencies.md` ask 6, granted, and it is a different thing from
  `[patch]` in the way that matters: it changes *which file on this machine gets
  linked*, not *which source gets compiled*, so the dependency graph stays a
  function of the manifests. It is also unavoidable — a site whose BLAS is at a
  path no provider list anticipated has no other recourse, and the alternative is
  that they fork the package. Cost: an override is a per-machine fact recorded in
  a per-project file, so a manifest with one in it is not portable. `SP0007`
  warns when an override is present in a package being *published*, which is
  almost always a mistake.
- **No `[workspace]` in F0–F1.** Path dependencies already give a monorepo a
  working arrangement; a shared lockfile across a workspace is a real convenience
  and it is additive later.

### 4.4 How the manifest relates to §4.4's modules

§4.4 of the core spec: *"A file is a module; a directory with `mod.science` is a
module containing its siblings. `use text.parser (Token, lex)`."* The manifest
adds exactly one thing: where the root of that tree is, and what it is called.

> **Decision 13. The `modules` directory is mounted at the package name. The
> package name is therefore the root module name, and a package name must be a
> legal Science module name.**

For the manifest above:

| Path | Module |
|---|---|
| `src/mod.science` | `spectra` |
| `src/fit.science` | `spectra.fit` |
| `src/text/mod.science` | `spectra.text` |
| `src/text/parser.science` | `spectra.text.parser` |

which makes the core spec's own example, `use text.parser (Token, lex)`, the
import of a package named `text`. Nothing about `use` changes; the manifest only
says which directory is the root.

The naming consequence is sharp and §5.1 depends on it: by
`stdlib-shape-and-packages.md` §4.7, a module name is **one lowercase word**, a
dot is a submodule separator and never a package boundary, and no module is named
for its implementation. So a package name is `[a-z][a-z0-9]*` — **no hyphens** (so
none of cargo's `serde-json` / `serde_json` papercut), **no dots** (a dot already
means something), **no underscores**, and digits only as the major-version suffix
of Decision 10.

Here is the package's own source, so the mapping is concrete:

```science
# src/fit.science  —  module `spectra.fit`

use linalg (Matrix, solve)
use random (Key)

public type Fit:
    centres: Array of F64
    residual: F64
    iterations: Int
    label: String?

public interface Converge:
    function is_converged(self) -> Bool

    function report(self) -> String:
        if self.is_converged(): "converged" else: "did not converge"

Fit implements Converge:
    function is_converged(self) -> Bool:
        self.residual <= 1.0e-6

Fit has:
    function empty() -> Fit:
        Fit(centres: Array.new(), residual: 0.0, iterations: 0, label: null)

public function fit_peaks(counts: borrowed Array of F64, key: Key) -> (Fit, Error?):
    let design, err be Matrix.vandermonde(counts.length(), 3)
    if err?:
        return (Fit.empty(), err)

    let coefficients, err be solve(design, counts)
    if err?:
        return (Fit.empty(), err)

    print(f"fitted {coefficients.length()} coefficients")
    return (Fit(centres: coefficients, residual: 0.0, iterations: 1, label: null), null)
```

and a consumer, in a different package, whose only knowledge of `spectra` is one
line in its own `[dependencies]`:

```science
use spectra.fit (Fit, fit_peaks, Converge)
use data.csv (read_csv)
use random (Key)

function main():
    let frame, err be read_csv("run-0912.csv")
    if err?:
        print("could not read the run")
        return

    let fit, err be fit_peaks(frame.column("counts"), Key.from_seed(20260916))
    if err?:
        print(f"fit failed: {err.message()}")
        return

    print(fit.report())

    if fit.iterations >= 3 and fit.label is not "blank":
        print(f"{fit.centres.length()} centres")
```

### 4.5 The `[native.<name>]` and `[sidecar]` tables

`ffi-c-boundary.md` §5.1 owns the search semantics of a `library` clause,
`native-dependencies.md` owns providers and ABI variants, and this note owns only
where they are *written down*.

> **Decision 14a. Native dependencies are declared as `[native.<name>]`, where
> `<name>` is exactly the string in `ffi-c-boundary.md` §5.1's `library "<name>"`
> clause.**
>
> This is `native-dependencies.md` ask 1, granted with its reason: the link name
> is what the linker and the loader see, and any other key introduces a mapping
> that can be wrong. It also means the diagnostic for a native failure can point
> at two places — the manifest line and the `extern` block's `library` clause —
> which is that note's stated reason for declining to add any syntax to
> `ffi-c-boundary.md`'s grammar.

Fields inside the table are that note's — `capability`, `variant`, `providers`,
`pkg-config`, `kind`, `when-available` — and `sciencec` applies them unchanged.
`capability` and `variant` are the two that §3.5 makes resolver inputs.

`[native.sources]` is the declarative C-compilation list Decision 5 promised, and
it is deliberately narrow: a list of files, a list of include directories, a list
of flags, no conditionals and no probes. It is the mechanism by which
`data-io.md` §3's vendored `miniz` is compiled without a build script.

`syntax-revision-2.md` §8.5 asks for a `foreign/` sidecar *"bound to Science names
by a manifest"*, and `rust-interop.md` §4.1 gives that manifest a file of its own,
`foreign/manifest.toml`, carrying names, handles and `Share` claims.

> **Decision 14. The sidecar's Science-side manifest is a `[sidecar]` table in
> `science.toml`, not a second file. `foreign/Cargo.toml` remains the user's and
> is untouched, exactly as `rust-interop.md` Decision 8 requires.**
>
> **Reason:** two manifest files in one package, in the same format, describing
> the same build, is how a project acquires a question with no good answer
> ("which one has the dependency?"). The Rust-side/Science-side split is real and
> it is already expressed by the *file it lives in* — `Cargo.toml` is cargo's,
> `science.toml` is Science's — so a third file adds a boundary where there is no
> third party.
>
> **Cost.** A package with several sidecars would want per-sidecar tables, which
> `[[sidecar]]` handles; and `rust-interop.md` §2.6's linker fact means there is
> exactly one Rust sidecar per *program* anyway, so the array is for the
> non-Rust case rather than a real Rust need. This is a small contradiction of
> that note's §4.1 layout and it is named here rather than left silent.

### 4.6 The entry file, where this note contradicts `script-mode.md` §4.2

That note fixes the order in which the compiler finds the entry, and adds:

> When a manifest arrives it should gain an `entry` key that takes precedence
> over rules 1 and 2, and nothing else changes.

Rule 1 is *the file named on the command line*. **Taking precedence over rule 1
is wrong**, and this note disagrees explicitly rather than silently:
`sciencec run scripts/plot.science` must run `scripts/plot.science`. A command
line that can be overruled by a configuration file is not a command line, and it
would break the single most useful thing in that note — running a script directly.

> **Decision 15. The order is: (1) the file named on the command line; (2)
> `[package] entry`; (3) `src/main.science`; (4) otherwise, a library build.
> `entry` takes precedence over rule 2 of `script-mode.md` §4.2 and over nothing
> else.**
>
> Two further points that note's §4.2 leaves open and this one closes:
>
> **The manifest is optional.** A `.science` file with no `science.toml` anywhere
> above it compiles. It is treated as a single-file package named after the file,
> with no dependencies, resolving modules through
> `stdlib-shape-and-packages.md` §6.3's search path. Script mode's whole thesis
> survives the arrival of a manifest untouched, which is the property that note
> was protecting when it declined to introduce the key.
>
> **"Crate root" should become "package root."** That note uses `crate root` once,
> for the directory holding `main.science`. Science has no `crate`; it now has a
> *package*, and the package root is the directory containing `science.toml`.
> `rust-interop.md` uses `crate` for Rust's crates, where it belongs, and two
> meanings of one word across two notes is avoidable for the price of one edit.

---

## 5. Naming, and the registry

### 5.1 Flat, because the name is a module name

> **Decision 16. Package names are **flat** — one lowercase word, `[a-z][a-z0-9]*`
> — with no scoping, no organisation prefix and no URL. New names are not
> first-come-first-served (§5.3).**

The argument is structural rather than aesthetic, which is why it is short.
Decision 13 makes the package name the root module name. `@nasa/spectra` is not a
legal module path; adopting it would force a mangling (`nasa_spectra`?
`nasa.spectra`, where the dot already means submodule?), and
`stdlib-shape-and-packages.md` §4.7 exists to prevent exactly that kind of
name-shaped compromise. **Scoping is not available without changing what a module
name is**, and changing what a module name is costs more than every problem
scoping solves.

> **Rejected: npm-style scopes (`@org/name`).** They do give free namespacing and
> they do defeat squatting on a name you own. They also produce names nobody
> types, and they demonstrably do not prevent the incident they are cited for —
> `left-pad` was unscoped, on a registry that had scopes.
>
> **Rejected: Go's model, where the import path is a URL** (`github.com/x/y`).
> This is the strongest alternative: free namespacing, no central registry, no
> squatting, and ownership that maps to something real. It is rejected because it
> makes module paths into URLs, which collides head-on with §4.4 of the core spec
> (`use text.parser`) and with §4.7's one-lowercase-word convention. Adopting it
> would mean `use github_com_lab_spectra.fit`, and no amount of aliasing makes
> that the language's normal case.
>
> **Cost.** Flat namespaces produce squatting, impersonation and the typosquatting
> class of attack, and Science has no structural defence against them. §5.3 buys
> the defence with policy instead, and §11 records that policy does not scale as
> well as structure.

### 5.2 Registry, git and path — all three, with git as the primitive

> **Decision 17. Three dependency sources, permanently: `path`, `git`, and
> `registry`. The registry is an *index over content-addressed archives*, not a
> source of truth for code, and a **directory registry** — a local directory of
> archives plus an index file — is a first-class registry.**

- `path` is how a monorepo and a two-package lab project work, and it needs
  nothing.
- `git` is how an unpublished package is shared, how a package is forked for a
  paper, and how F1 has a dependency mechanism before there is a registry. It
  never goes away.
- `registry` is (name, version, archive hash, signature, manifest) rows in a
  signed index, plus archives fetched by hash. Because the lock already pins the
  hash (§2.2), the *archive host* need not be trusted at all, which is what makes
  §6.5's site mirror a `cp -r` rather than a security design.

> **Rejected: registry only** (crates.io's model until `[patch]`). It makes a
> pre-registry phase impossible and it makes forking a dependency for a paper —
> which is a normal thing for a scientist to do — a publishing act.
>
> **Rejected: git only** (Go's original model). It works, and it puts the
> reliability of every build at the mercy of the availability of arbitrary
> repositories. Go added a module proxy and a checksum database for exactly this
> reason, and those are a registry with a different name.
>
> **Cost.** Three sources means three code paths, three failure modes, and three
> sets of diagnostics. The content-addressing decision collapses most of that: all
> three produce a canonical archive and a hash, and everything downstream of the
> hash is one code path.

### 5.3 Supply chain, as a first-order concern

The framing matters and it is specific to this language. In most ecosystems a
compromised package exfiltrates a credential or mines a coin, and the victim
eventually notices. **Here, a compromised package perturbs a number.** The user
does not notice, publishes it, and the corruption propagates into the literature
with a citation attached. There is no cleanup path for that, which is why this
section is not a footnote.

The measures, in the order they are worth:

1. **No build scripts** (§2.6). The single largest one, and it is already decided.
   A malicious Science package cannot run code when it is *added*; it runs code
   only when the program *calls it*, and at that point ownership, the effect
   discipline and code review are the language's ordinary business rather than a
   packaging problem. The sidecar is the bounded exception and §2.7 makes it
   visible.
2. **Content addressing plus the lockfile** (§2.2). A registry compromise cannot
   alter a locked build. This is the reproducibility machinery paying for itself
   twice, and it is worth saying out loud: **the mechanism that lets a reviewer
   reproduce an analysis is the same mechanism that lets them diff exactly what
   changed between two runs of it.**
3. **Vendoring and `--offline`** (§6.3). The default posture for a cluster
   project is a vendor directory committed to the repository, where a dependency
   update is a reviewable diff rather than an invisible fetch. This is the
   strongest available defence and it is free, because §6 needs it anyway.
4. **Index signing, required from the registry's first day.** The registry signs
   its index with a root of trust shipped in the toolchain tarball, TUF-shaped,
   with a specified rotation path. It protects every user with zero user burden.
5. **Publisher signing, optional and displayed.** A publisher may sign an archive;
   the registry records and displays it; consumers may require it per-dependency.
   **Not mandatory**, because mandatory publisher keys are how an ecosystem
   acquires thousands of unrotatable secrets in CI, and because it only helps
   against a registry compromise, which (4) and (2) already cover for the common
   case.
6. **New names are reviewed.** A *first* publish under a name is gated on a human
   look; subsequent versions are not. This is cheap at the scale where squatting
   actually happens — the first few thousand packages — and §11 says what to do
   when it stops being cheap.
7. **Confusable names are refused at publish** (`SP0051`). Unicode-confusable
   normalisation, digit/letter confusion (`l`/`1`, `O`/`0`), and edit distance 1
   against any existing name. Because Decision 16 forbids hyphens and
   underscores, the largest real class — `python-dateutil` versus
   `python_dateutil` — cannot exist here at all. That is a genuine benefit of the
   naming rule and it was not the reason for it.
8. **A client-side warning too**, not only a publish-side refusal: `sciencec add`
   warns when a name is within edit distance 1 of a substantially more-used
   package. Publish-side refusal cannot catch a name registered before its
   victim became popular.
9. **`sciencec audit`** against an advisory database that is itself a signed,
   mirrorable artefact. Because MVS does not auto-upgrade (§3.1), this is *the*
   mechanism by which a user learns that a selected version is vulnerable, and it
   must ship with the registry rather than after it.

### 5.4 Yanking, deprecation, and a lockfile pointing at a yanked version

> **Decision 18. A **yank** removes a version from consideration in new
> resolutions. It does not delete the artefact and **it does not break a
> lockfile**. A locked build of a yanked version succeeds and warns (`SP0040`),
> naming the advisory if there is one.**

The non-negotiable part for this audience: a paper's build must keep working. An
`unpublish` that breaks an existing lock — npm's original behaviour, and the
`left-pad` incident — is not an option for artefacts attached to the literature.

- **Fresh resolution** onto a yanked version is an error, not a warning. There is
  no reason to newly select one.
- **Deletion** exists only for legal reasons and for confirmed malware. Then the
  registry keeps the hash and serves a **tombstone**, so a locked build fails
  with a diagnostic that says *why* — and `SP0040`'s message distinguishes
  "yanked", "withdrawn for cause" and "never existed", because those call for
  three different actions by the user.
- **Deprecation** is metadata on a *package*, not a version: a message and an
  optional successor name, warned once per build (`SP0042`) and never once per
  `use`. This is `stdlib-shape-and-packages.md` Decision 6's *deprecation cycle*
  finally having a mechanism to live in.
- **A yanked version carrying an advisory** makes `sciencec audit` exit non-zero,
  which is the CI hook.

---

## 6. The audience constraint, which is what makes this different

`stdlib-shape-and-packages.md` §1.2 states the environment better than this note
could: a login node with no root, often no outbound network from the compute
nodes, a module system instead of a package manager, a shared filesystem with a
quota, and an administrator whose answer to "can you install this" has a two-week
latency. `ffi-c-boundary.md` §5.1 designed the library search for it explicitly.
**A package manager that assumes a writable global location and an internet
connection is unusable for a large share of the audience.** Five consequences.

### 6.1 No writable global location, ever

> **Decision 19. Nothing in a build requires a writable location outside the
> package directory. The cache is `--cache DIR`, else `$SCIENCE_CACHE`, else
> `$XDG_CACHE_HOME/science`, else `~/.cache/science`; and if **no** cache is
> writable, a build from a vendor directory still succeeds.**

A read-only home directory is a configuration, not a failure. `SP0031` reports an
unwritable cache as a *warning* when a vendor directory covers the build.

> **Rejected: a global content-addressed store** shared across projects, in the
> style of Nix or pnpm. It is attractive — content addressing makes sharing safe —
> and it is rejected as the default because a shared HPC home directory has a
> quota measured in gigabytes and, more sharply, an **inode limit**, and a
> content-addressed store of extracted sources is the classic way to exhaust one.
> It remains available as a cache location for a site that wants it.
>
> **The chosen shape**, for the same reason: **the cache holds compressed
> archives, one file per package version; extraction happens into the project's
> own build directory.** Few inodes, one quota to watch, and the archive is the
> thing the lock hashes anyway.

### 6.2 The build never touches the network

> **Decision 20. `sciencec build` makes no network request, ever. Fetching is a
> separate command, `sciencec fetch`, which is the only command in the toolchain
> that opens a socket. `git` dependencies are fetched by invoking the system
> `git` as a subprocess; registry fetch (F2) is HTTPS in one isolated crate.
> `sciencec` performs no network I/O at all in F0 or F1 — `git` is a subprocess,
> not a client.**

This is `native-dependencies.md` Decision 10 extended to the Science half, which
is its ask 4, and that note's argument is the one that decides it: *"an offline
mode is a mode you can forget to be in, and the place you discover you forgot is
inside a batch job on a node with no route to the internet, two days after you
submitted it."* An unconditional invariant moves that discovery to the login node,
in the second before `sbatch`. `SP0030` names the missing artefacts and prints the
exact `fetch` command.

The Science-specific half of the reasoning: `git` is on every machine this
audience uses and is what they already use to move code onto a cluster; embedding
a git implementation is a large dependency for no benefit; and keeping TLS out of
the compiler for two phases keeps `sciencec` small, `scp`-able, and free of a
whole class of CVE. The F0/F1 compiler has *no code path* that reaches the
network, which is a stronger statement than a flag that disables one.

Cost: two commands where other toolchains have one, and a diagnostic the first
time a user adds a dependency and types `build` — which is most users' first
encounter with the split, so `SP0030`'s wording matters more than its position in
§8.2 suggests. Second cost: `git` becomes a build dependency for any project with
a `git` dependency, and its version behaviour (partial clones, `--filter`) has to
be probed rather than assumed.

`--offline` therefore does not mean "do not fetch" — nothing fetches. It means
"do not consult the cache either; build from `vendor/` alone", which is the
air-gapped case of §6.3.

### 6.3 Vendoring, and offline as the normal mode

> **Decision 21. `sciencec vendor` writes every locked dependency — source **and
> manifest** — into `vendor/`, verified against the lock. `--offline` then
> requires no network and no cache. An offline build on a machine with no network
> is a test in the suite from the day the resolver exists, not after.**

The manifests must be vendored, not only the sources, because MVS reads the
requirements of every selected version (§3.1). Vendoring only sources produces a
tree that builds today and cannot be re-resolved tomorrow, which is a trap worth
naming.

**Offline is not a degraded mode; it is the mode the audience's compute nodes
use.** Treating it as a fallback is how it acquires the bugs that only appear on
the machine where it matters. `rust-interop.md` §4.2 reached the same conclusion
for the sidecar independently — *"this is unglamorous and it is the difference
between 'works on a cluster' and 'works on a laptop', so it ships with the
feature rather than after it"* — and `sciencec vendor` shells out to `cargo
vendor` for the sidecar half so that one command produces one self-contained tree.

### 6.4 `sciencec archive` — the artefact for the methods section

> **Decision 22. `sciencec archive` produces one tarball containing: the package
> source, `science.lock`, the vendored dependencies with their manifests, the
> `[build]` table (§2.3) and the native lock entries (§2.4). It is what you attach
> to a paper.**

This is the direct answer to `stdlib-shape-and-packages.md` §6.2's third failure —
*"there is no way to record what a result was produced with"* — and it is the
reason that failure is the one this note treats as most important. An archive plus
a published `sciencec` tarball of the recorded version is a complete, offline,
verifiable reconstruction of tiers 1 and 2 of §2.5, and an audit trail for tier 3.

**It is deliberately not called `bundle`.** `native-dependencies.md` Decision 11
already owns that name for a *relocatable directory* — a binary, the shared
objects it needs and a launcher — and the two artefacts have opposite purposes:
`bundle` is for a recipient who wants to **run** the program without rebuilding
it, `archive` is for a recipient who wants to **rebuild** it and check that they
get the same numbers. A scientist submitting a paper usually wants both, and
attaching one when they meant the other is a failure worth two names to avoid.

Cost: archives are large, because vendored sources are duplicated per project. For
a scientific artefact that is the correct trade, and `--no-vendor` exists for
people who disagree.

### 6.5 An installed toolchain works with no registry at all

> **Decision 23. A **directory registry** — a directory of archives plus a signed
> index file — is a first-class dependency source, selected with
> `--registry DIR` or `SCIENCE_REGISTRY`. A site administrator can mirror the
> public registry, or curate a subset, by copying files.**

This is how conda-forge mirrors and how Spack are actually used on real clusters,
and it costs almost nothing because Decision 17 already defines a registry as an
index plus content-addressed archives. The lock's hashes mean the mirror need not
be trusted.

The combination of Decisions 19, 21 and 23 is the answer to the section's
question: **yes, an installed Science toolchain works with no registry, no
network and no writable global location**, and that configuration is a tested
one rather than a claim.

### 6.6 The module system will swap the shared object, and that is allowed

Restating §2.4's consequence here because this is where it will be read: a
cluster `module load openblas/0.3.27` replacing `libopenblas.so` under a fixed
name is the **supported** mechanism, per `ffi-c-boundary.md` §5.1. Therefore the
native lock entry is Tier B, a warning by default and an error only under
`--locked`.
A package manager that errored here would be a package manager the site
administrator had to be worked around, and that is a losing position.

---

## 7. What ships when

### 7.1 F0 — the one-way doors, and nothing else

Small, and all of it is `stdlib-shape-and-packages.md` §6.3's list made concrete.

| Item | §|
|---|---|
| `science.toml` parsed by `sciencec`: `[package] name version language entry modules sidecar`, `[dependencies]` accepting **`path` only** | §4 |
| The module search path order fixed: `--module-path`, then `SCIENCE_PATH`, then the toolchain — and sidecar resolution uses the same one | §6.3 of that note; `rust-interop.md` ask 7 |
| Package name = root module name, and the naming rule enforced (`SP0004`) | §4.4 |
| `science.lock` written, even when it records only this package and the `[build]` table. **The format ships before the resolver does** | §2.2, §2.3 |
| No build scripts, stated in the manifest's specification rather than implemented as a refusal | §2.6 |
| `SP` diagnostics rendered through `science-diagnostics` | §8 |
| No network code in the compiler | §6.2 |

**Why these are F0 and not F1.** Each is a decision that changes the meaning of
existing artefacts if it lands later. A search path added after `use` resolves
against a hard-wired root changes what existing programs mean. A manifest added
after packages exist means the first thing the package manager does is guess. A
lockfile format added after builds exist means no build before it can be
identified. Everything else in this note is additive.

Effort: small. The manifest is a TOML parse and five field validations; the lock
is a serialiser; the search path is an ordered list where a constant is today.

### 7.2 F1 — the minimum viable package manager

| Item | § |
|---|---|
| `git` and `directory` dependency sources | §5.2 |
| MVS resolution | §3.1 |
| The lockfile filled in: content hashes, `[build]`, native observations | §2.2–§2.4 |
| `sciencec fetch`; `--locked` and `--offline` | §2.3, §6.2, §6.3 |
| `sciencec vendor`, `sciencec archive`, `sciencec why`, `sciencec add`, `sciencec update` | §6 |
| Level 3 becomes a real category — `stdlib-shape-and-packages.md` §6.4 gets a date | §1.2 |
| `data-io.md` §9's `--features parquet` becomes an ordinary dependency | §1.2 |

**This is the week of work the brief describes, and it is the item to defend.**
git dependencies plus a lockfile close three of
`stdlib-shape-and-packages.md` §6.2's four failures outright and most of the
fourth: two versions coexist, a result can be recorded, third parties can depend
on Level 3, and a security patch to a Level 3 package becomes a version bump
rather than a toolchain release. It does this with no registry, no service, no
signing and no operational obligation whatsoever.

It is also where MVS earns its keep: the entire resolver at this stage is a graph
walk over vendored manifests.

### 7.3 F2 and later — the registry

| Item | § |
|---|---|
| The hosted registry: signed index, content-addressed archives, mirrors | §5.2 |
| `sciencec publish`, ownership, new-name review, confusable refusal | §5.1, §5.3 |
| Yanking, withdrawal, deprecation, tombstones | §5.4 |
| The advisory database and `sciencec audit` | §5.3 |
| Machine-checked semver at publish, and the API signature file | §3.2 |
| Sidecars permitted in published packages, gated on review | §2.7 |

**Do not start this until there is something to host.** Concretely: not before
`stdlib-shape-and-packages.md` §5's Level 3 set exists as packages and at least
one third party is asking to publish. A registry with forty packages in it is a
service obligation with no users, and it is much easier to start one than to stop
one.

Effort: a year, including the operational side, and the operational side is most
of it.

---

## 8. Errors

### 8.1 A separate namespace

> **Decision 24. Manifest, resolution, lockfile and registry errors take codes in
> a **separate `SP` namespace**, `SP0001`–`SP0999`, rendered by
> `science-diagnostics` to the same standard as every `SC` code.**

Three reasons:

1. **§9's ranges are allocated by *compiler phase*, and every one of them has a
   span into a `.science` file.** These diagnostics have either a span into a TOML
   file — a different language, in a file the compiler does not otherwise read —
   or **no span at all**: a version conflict is a property of a graph, not of a
   location. Putting them in `SC0500`+ would make the range table's organising
   principle false for one block, and that table is the thing the README asks
   every note to check before allocating.
2. **These fire before any Science source is read.** In F0 the manifest is parsed
   before the compiler has an opinion about a program at all, and an error that
   stops a build before lexing is not a lexical error with a bigger number.
3. **It removes this note from the contention entirely.** The README records four
   collisions among five notes written in parallel, and `rust-interop.md` §7 has
   just documented a live three-way overlap between `ffi-c-boundary.md` §8,
   `python-interop.md` §9 and itself in the `SC04xx` range. A disjoint prefix
   costs one more thing to remember and is immune.

> **Rejected: `SC0500`–`SC0599`.** The simplest option, and it would work. It is a
> close call, and it loses on (1): the `SC` table means "phase", and the package
> layer is not a phase.
>
> **Rejected: uncoded messages.** `llm-ergonomics.md` makes diagnostics the
> primary teaching channel for a language with no training corpus, and an uncoded
> error cannot be looked up, cannot be suppressed, and — decisively — cannot be a
> UI test under §10's third layer.

`SP` keeps the two-letter-plus-four-digit shape, so `science-diagnostics`, the
renderer and the UI-test harness need no change.

### 8.2 The block

| Code | Fires on | Severity |
|---|---|---|
| `SP0001` | `science.toml` is not valid TOML. Span into the manifest, rendered against it as a source file | error |
| `SP0002` | Unknown key in the manifest. Suggestion: the nearest known key | error |
| `SP0003` | A required key is missing (`name`, `version`) | error |
| `SP0004` | `name` is not a legal module name (§4.4). Suggestion: the normalised name | error |
| `SP0005` | `version` is not a semantic version, or carries a range operator. The message says the caret means nothing here and that the plain version already means "at least" | error |
| `SP0006` | A dependency names more than one of `version`, `git`, `path` | error |
| `SP0007` | A package being published carries a root-only `[native.*]` override (§4.3) | warning |
| `SP0010` | Two packages in the graph declare the same `name` from different sources | error |
| `SP0011` | MVS: a required minimum exceeds every published version | error |
| `SP0012` | A `git` dependency is required at two different revisions (§4.2) | error |
| `SP0013` | A cycle among `path` dependencies | error |
| `SP0020` | `--locked` was given and the lock does not cover a dependency in the manifest | error |
| `SP0021` | Content hash mismatch against the lock (§2.2) | error |
| `SP0022` | The `[build]` table differs from the lock (§2.3) | warning; error under `--locked` |
| `SP0030` | A dependency is in neither the cache nor `vendor/`. Names the artefacts and prints the exact `sciencec fetch` command (§6.2) | error |
| `SP0031` | The cache directory is not writable | warning if `vendor/` covers the build, else error |
| `SP0040` | A selected version is yanked, withdrawn for cause, or tombstoned; the three are distinguished | warning if locked, error if resolving fresh |
| `SP0041` | A selected version is under advisory (`sciencec audit`) | error in `audit`, warning in `build` |
| `SP0042` | The package is deprecated; names the successor if declared. Once per build | warning |
| `SP0043` | A package being added carries a sidecar, and therefore build-time code (§2.7) | note |
| `SP0050` | Publish refused: the API diff requires a larger version bump. Renders the specific items | error |
| `SP0051` | Publish refused: the name is confusable with an existing package (§5.3) | error |
| `SP0052` | `sciencec add` brings in a name within edit distance 1 of a much more used package | warning |

`SP0023` and `SP0060`–`SP0999` are free. This note claims **no `SC` code**, which
is the point of Decision 24; the README's allocation table gains a row saying so.
The native half's diagnostics are `native-dependencies.md`'s `SC0471`–`SC0479`
and are not duplicated here — in particular its `SC0479` is the native-mismatch
code that an earlier draft of this note had allocated as `SP0023`.

### 8.3 The rendering standard

A resolution failure that prints a wall of version constraints with no
explanation would be the worst error message in the toolchain, and it is the most
likely place for one. Four rules, and they are requirements rather than
aspirations:

1. **Name at most three constraints, as *packages asking*, never as a list of
   ranges.** A user reads "`stats 0.4.0` asks for at least 0.8.0"; nobody reads
   `>=0.8.0, <0.9.0-0`.
2. **Always point at a file and a line** — including a *dependency's* manifest.
   This has an implementation consequence: dependency manifests must stay in the
   cache after resolution, so the diagnostic has something to render against.
3. **Always offer a concrete next command.** MVS makes this tractable in a way
   SAT does not: the failing constraint is a single maximum, so "the newest
   version of X that works" is a linear scan rather than a second solve.
4. **Never print the graph** unless asked. `sciencec why linalg` prints the path
   from the root to a selected version; `--explain` prints the whole thing.

The exemplar, which is the bar:

```
error[SP0011]: no published version of `linalg` satisfies every requirement
  ┌─ science.toml:7:10
  │
7 │ linalg = "0.6.0"
  │          ^^^^^^^ this package asks for at least 0.6.0
  │
  ├─ stats 0.4.0 asks for at least 0.8.0
  │   ┌─ ~/.cache/science/stats-0.4.0/science.toml:9:10
  │
  = the newest published `linalg` in the 0.x series is 0.7.3
  = so `stats 0.4.0` cannot be built against any `linalg` that exists
help: `stats 0.3.6` is the newest release that works with `linalg 0.7.3`
      sciencec add stats@0.3.6
```

Everything in it is computable from MVS's state at the moment of failure. That
is not a coincidence; it is §3.1 item 2 being cashed in.

---

## 9. What this note does not own

Named so that nobody reads a gap as a decision.

- **Finding and linking native libraries.** `ffi-c-boundary.md` §5. This note owns
  only the file the result is recorded in.
- **Shipping native libraries** — the "somebody has to distribute OpenBLAS"
  problem of `scientific-libraries.md` §15. `native-dependencies.md` owns it in
  full: providers, ABI variants and capabilities, the three reproducibility tiers,
  the provenance section, `sciencec bundle`, `sciencec doctor`, and the
  `SC0471`–`SC0479` diagnostics. This note supplies the manifest table those
  facts are declared in (§4.5), the lock array they are recorded in (§2.4), and
  the resolver hook its ask 2 needed (§3.5). **Where the two disagree about
  anything native, that note is right.**
- **Rust.** `rust-interop.md` in full. §0, §2.7 and §4.5 are the only points of
  contact and all three defer to it.
- **What is *in* the standard library.** `stdlib-shape-and-packages.md` §5.
  This note makes Level 3 distributable and expresses no opinion on its contents.
- **The build cache and incremental compilation.** §3 of the core spec's `salsa`
  architecture. Resolution is a query and its inputs are named in §4.2 of
  `rust-interop.md`'s table; how they are cached is not this note's.
- **Cross-compilation**, which §12 of the core spec keeps out of scope.
  `rust-interop.md` Decision 14 fixes the `--target` *spelling* to Rust's, and
  the lock records the triple (§2.3), so nothing here blocks it later.

---

## 10. What this note asks of the other notes

Ordered by how much is blocked behind each.

1. **The core spec §12** — amend the line *"Language server, formatter, package
   manager … planned, not built"* to carry Decision 1's split: the manifest, the
   search path and the lockfile format are F0; resolution is F1; the registry is
   F2+. Three quarters of that sentence stands.
2. **The core spec §4.4, or wherever `choice` is specified** — an **open
   `choice`**, declared by the author, requiring a catch-all arm in every
   downstream `match`. Without it, §4.5's exhaustiveness makes every new error
   variant a major version bump (§3.2), and Decision 9 should not ship. This is
   the only language-level feature this note requests, and it is small.
3. **`script-mode.md` §4.2** — `entry` takes precedence over rule 2, **not** rule
   1. The file named on the command line always wins. Also: "crate root" becomes
   "package root", since `rust-interop.md` now uses `crate` for Rust's crates.
   This is an explicit contradiction and §4.6 gives the reason.
4. **`stdlib-shape-and-packages.md` §6.3** — both preconditions are accepted and
   specified here (§7.1), which means §6.4's *"Level 3 is a category whose
   defining property is not implemented"* can be given a date rather than a
   caveat. §5.2's TOML argument is seconded with a sharpened reason (§4.1), and
   the `data.toml`-is-not-normative clause of Decision 12 is a request of that
   note rather than a decision over it.
5. **`native-dependencies.md`** — its six asks are granted (§0.1) and this note
   asks two things back, both small. **(a)** Its `--locked` definition (§5.3) is
   adopted as the toolchain's *only* strict mode, which means it now also governs
   the `[build]` table of §2.3; that note should say so, since a reader of its
   §5.3 would otherwise think `--locked` is native-only. **(b)** Its Decision 9
   provenance section and this note's Decision 7 (no incidental non-determinism in
   a binary) should be stated as complementary where they meet — §2.8 gives the
   reconciliation and neither note currently cross-references the other. Also
   noted, not asked: its §6.4 owns `bundle` and this note takes `archive` (§6.4),
   so the two names must not drift back together.
6. **`data-io.md` §9** — the migration from `--features parquet` to an ordinary
   dependency is pre-agreed by that note and confirmed here. One addition it does
   not have: **until F1, the feature must be recorded in the `[build]` table**,
   because two toolchains of the same version with and without Parquet produce
   different binaries, and §2.3's whole point is that such a difference is
   visible.
7. **`rust-interop.md` §4.1** — Decision 14 folds `foreign/manifest.toml` into a
   `[sidecar]` table in `science.toml`. Its `Cargo.toml` is untouched and its
   Decision 8 is unaffected. That note's §4.2 keys `foreign_archive` on the
   sidecar hash and `Cargo.lock`'s hash already; the ask is that both also be
   *written into* `science.lock` (§2.4), so a human and a reviewer see what a
   query already knows. Its §8 ask 7 — that sidecar resolution use the same
   ordered search path as module resolution — is confirmed here (§7.1).
8. **`stdlib-standard.md`** — `time.zones`'s open item, *"a package manager"*, is
   answered: F1, as a data package. Content addressing (§2.2) is exactly the
   pinning mechanism tzdata needs, and tzdata is the clearest case in the library
   of a dependency that is *data* rather than code.
9. **`README.md`** — add a row for this note (`package-manager.md`, "the manifest,
   resolution, the lockfile, the registry, naming and reproducibility", F0 / F1 /
   F2+) and a line recording that it claims **no `SC` code** and owns the
   `SP0001`–`SP0999` namespace instead. This note does not edit the README,
   because several agents are writing at the moment; the row is an ask. Two
   siblings landed in the same window with the same problem and the same request
   — `rust-interop.md` §7 (`SC0462`–`SC0470`, `SC0145`–`SC0149`) and
   `native-dependencies.md` §10 (`SC0471`–`SC0479`) — so **whoever lands these
   three should add all three rows in one edit**, which is also the only way to
   see whether `rust-interop.md`'s flagged three-way overlap with
   `ffi-c-boundary.md` §8 and `python-interop.md` §9 survived.
10. **The compiler** — a `public` **API signature dump**, canonical and sorted, as
    input to §3.2's semver check. It is `dump.rs`-shaped and should reuse the
    snapshot machinery rather than acquire a second serialiser. Not needed before
    F2.

---

## 11. Risks

**The registry is a service, and a language project can fail at operating one.**
Uptime, moderation, abuse reports, a legal entity to receive a takedown, storage
costs that never end, and a security response process. This is the largest risk
in the note and it is why §7.3 says not to start until there is something to
host. A language with only git dependencies is in a worse position than one with
a healthy registry and a *much* better one than a language with a registry it
cannot operate.

**MVS is a minority position and it will be argued about**, repeatedly, by every
contributor arriving from cargo. The argument that should settle it: **MVS is the
reversible choice.** Moving from MVS to a SAT resolver later is possible — every
existing manifest is still a valid constraint. Moving from SAT to MVS is not,
because manifests in the wild will carry ranges that MVS cannot interpret. Choose
the reversible one first.

**No feature flags is the decision most likely to be revisited under pressure**,
and the pressure will come from exactly one place: a dependency that must be
absent from the *graph*, not merely unlinked. §3.4 names the minimal relenting
position in advance so that the first person to hit it does not reach for cargo's
full mechanism.

**No build scripts is a bet.** It is the right bet and it will be tested within a
year by protobuf, by `bindgen`, or by a C library whose flags depend on a probe.
The escape hatch is "check in the generated file", and some ecosystem will find
that unacceptable and route around Science rather than around the rule. The
sandbox option (§2.6) is the fallback and it is a project.

**Flat names plus human review does not scale.** It works at 10³ packages and
fails at 10⁵. The fallback is crates.io's actual answer — verified publishers,
mandatory two-factor, and living with squatting — and it is worth knowing now
that this is the fallback, because it is not a good one and structure would have
been better. Decision 16 chose the module-name constraint over it deliberately;
this is the bill.

**Machine-checked semver could become a support burden.** A publish gate that
fires falsely costs a maintainer a day and an explanation. The conservatism in
Decision 9 is the mitigation and conservatism is not a proof. Worse: a green
check on a version bump will be *read* as "this release is safe", which §3.2 says
it is not, and there is no technical fix for a misread — only wording.

**Reproducibility will be oversold by someone other than this note.** §2.5's
three tiers and the disclaimer under them are the honest claim; "Science builds
are reproducible" is the sentence that will end up on a slide. The mitigation is
that the disclaimer travels with the tiers everywhere they are quoted, including
in `sciencec --version --verbose`.

**A resolution bug is a compiler release.** Putting the package manager inside
`sciencec` (rather than shipping a second binary) is right for a user who has to
`scp` one file onto a cluster, and the cost is that the two ship together and
cannot be patched apart. Accepted, and named because the alternative will look
attractive the first time it bites.

**Three notes now define parts of one lockfile and one manifest.** This note owns
the file, `native-dependencies.md` owns the `[[native]]` entries and the
`[native.*]` tables inside it, and `rust-interop.md` owns the sidecar whose two
hashes go in it. Each is right about its half and none of the three can validate
the schema alone. The mitigation is that the schema should be written down **once,
in one place, before any of the three is implemented** — and the place is this
note's §2 and §4, which means an error in them propagates to two siblings that
will reasonably assume it was checked.

**The two-tier crate world.** `rust-interop.md` §2.6 already records it: the
standard library's `regex` is pinned when the toolchain is built while a user's
sidecar `regex` is pinned by their own `Cargo.lock`, so one machine holds two.
§1.2 item 6 proposes fixing this by shipping the curated shims as packages — and
§2.7 then delays that to F2, because a published package with a sidecar runs
build-time code. So the wart persists through F1 by this note's own decision, and
that is a cost Decision 6 imposes on a sibling rather than on itself.

---

## 12. Summary of decisions

| # | Decision | §|
|---|---|---|
| 1 | Manifest, search path and lock *format* in F0; resolution in F1; registry F2+ | §1.4 |
| 2 | Dependencies identified by SHA-256 of a canonical archive, not by version + URL | §2.2 |
| 3 | The `[build]` table — compiler, LLVM, target, flags — in the lock; warning by default, error under `--locked`, which is the one strict mode | §2.3 |
| 4 | A `[[native]]` lock array with a per-entry `tier`; Tier B records, never constrains | §2.4 |
| 5 | **No build scripts.** Build-time needs are declared, not executed | §2.6 |
| 6 | The Rust sidecar is the bounded exception, made visible in three places | §2.7 |
| 7 | No timestamps, hostnames or absolute paths in binaries by default | §2.8 |
| 8 | **Minimal version selection**, not a SAT solver | §3.1 |
| 9 | Semver machine-checked at publish, conservatively, blind to behaviour and honest about it | §3.2 |
| 10 | One version per package name; a major bump renames the package (`linalg2`) | §3.3 |
| 11 | **No feature flags** | §3.4 |
| 11a | `capability` and `variant` are resolver inputs; a variant conflict is a resolution failure | §3.5 |
| 12 | The manifest is TOML, `science.toml`; the compiler's parser is normative | §4.1 |
| 13 | The `modules` directory mounts at the package name; the name is the root module | §4.4 |
| 14 | The sidecar's Science-side manifest is a `[sidecar]` table, not a second file | §4.5 |
| 14a | Native dependencies are declared as `[native.<name>]`, keyed by the link name | §4.5 |
| 15 | Entry order: command line, `entry`, `src/main.science`, library. Manifest optional | §4.6 |
| 16 | **Flat names**, one lowercase word, because the name is a module name | §5.1 |
| 17 | `path`, `git` and `registry` permanently; a directory registry is first-class | §5.2 |
| 18 | A yank never breaks a lockfile; withdrawal leaves a tombstone | §5.4 |
| 19 | No writable global location required; the cache holds archives, not extractions | §6.1 |
| 20 | `sciencec build` never touches the network; `sciencec fetch` is the only networked command | §6.2 |
| 21 | `vendor/` carries sources **and manifests**; offline is a tested mode, not a fallback | §6.3 |
| 22 | `sciencec archive` is the source artefact for the methods section; `bundle` is the sibling's | §6.4 |
| 23 | A toolchain works with no registry, no network and no writable global location | §6.5 |
| 24 | A separate `SP0001`–`SP0999` diagnostic namespace; no `SC` code claimed | §8.1 |
