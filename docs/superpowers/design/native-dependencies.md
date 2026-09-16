# Science — Design: native dependencies

Date: 2026-09-16
Status: draft for review
Depends on: `ffi-c-boundary.md` (§0, §1.6, §5, §7 — this note is its sibling and
re-decides none of it), `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
§1, §3 and §12, `syntax-revision-2.md` for the syntax of every sample.
Related: `scientific-libraries.md` §2 and §15, `stdlib-shape-and-packages.md` §5
and §6, `stdlib-standard.md` §13, `data-io.md` §9.
Blocked on, in part: `package-manager.md`, which is being written in parallel and
did not exist when this note was drafted. §10 states what this note needs from it
as asks rather than assuming its design.

Scope: **how a Science program gets the C and Fortran libraries it links
against.** Declaration, resolution strategy, ABI variants, reproducibility,
offline and HPC operation, the self-contained-binary promise, and GPU stacks.

Out of scope: anything about *Science* packages — manifest format, version
resolution, the lockfile's Science half, the registry, naming. Those belong to
`package-manager.md`.

---

## 0. The one-sentence position

> **The default is to use what the machine already has, because the machine is
> usually a cluster and the cluster administrator has already made the choice
> better than we can. Everything else in this note exists so that "what the
> machine already has" can be named, verified, and written down.**

---

## 1. Why this is a separate problem from the package manager

It is routinely conflated with it, so it is worth separating carefully.

A Science package manager resolves *Science* packages. Those are source, they are
compiled by `sciencec`, they have one representation, and two of them differ only
by version. A version is a total order, resolution is a solver over it, and the
whole problem is well understood.

A Science package that wraps BLAS is not like that. Consider `linalg`, whose
implementation `scientific-libraries.md` §2 already decided is a safe wrapper
over a linked OpenBLAS. To build a program that uses it, the following must all
be true, and **not one of them is a property of a Science package version**:

1. A BLAS implementation exists on the machine — OpenBLAS, or MKL, or Accelerate,
   or BLIS, or the site's own build.
2. It was built with the integer width the binding assumes. `ffi-c-boundary.md`
   §1.6: get this wrong and you get **correct answers for small matrices and
   silently wrong ones for large**.
3. It is for this platform and this architecture — and, unlike a Science package,
   an OpenBLAS built for Skylake and one built for Zen 4 are *both correct* and
   differ by a factor of two in throughput, which is the reason the audience
   cares about compute in the first place.
4. Its threading model does not collide with the program's. An OpenBLAS built
   with its own pthread pool, called from inside a Science parallel loop,
   oversubscribes the node.
5. Its licence permits whatever the user intends to do with the resulting binary,
   which depends on whether the link is static or dynamic (§7.3).

None of (1)–(5) is expressible as "a version of a Science package", and a
resolver that treats them as one produces a lockfile that is a lie. That is the
whole argument for a second mechanism, and it is why conda exists as a separate
universe from pip rather than as a feature of it.

There is a second, sharper reason. A Science package's identity is its source
hash. A native dependency's identity, in the common case on a cluster, is *a path
to a file that the module system will point somewhere else tomorrow* — and that
is not a defect of the design, it is the mechanism `ffi-c-boundary.md` §5.1 made
dynamic linking the default **in order to support**. The two problems have
different notions of identity. §5 is about being honest about the second one.

---

## 2. The precedents, read for what they teach

This is the most useful section of the note, because every one of these systems
was built by people who understood the problem and each of them failed somewhere
specific.

### 2.1 pip / PyPI and manylinux wheels

The first fifteen years of `pip install scipy` were broken, and the reason is
exact: PyPI's package format had no way to express a dependency on a C library,
so the only two things a package could do were *build it from source at install
time* (requiring a Fortran compiler on every scientist's laptop, which is why the
error message everyone remembers is `error: library dfftpack has Fortran sources
but no Fortran compiler found`) or *bundle a copy inside the wheel*. The
manylinux standard eventually made the second one work: a wheel declares a glibc
floor, the build happens in a controlled old container, and `auditwheel`
rewrites the binary to carry its native dependencies inside the wheel with mangled
`SONAME`s so that two wheels bundling different OpenBLAS builds do not collide in
one process.

**Copy:** the glibc-floor idea — a prebuilt artefact declares the oldest
platform it runs on, rather than the newest it was built on. And copy the
discipline that a binary artefact must be *audited* for what it actually depends
on, not trusted to declare it.

**Do not copy:** bundling as the only strategy. NumPy and SciPy wheels each carry
their own OpenBLAS, so a scientific Python environment routinely has two or three
copies of BLAS in one process, each with its own thread pool, and the resulting
oversubscription is a well-documented performance bug that ordinary users cannot
diagnose. Science's audience runs on machines where that matters more, not less.
Also do not copy the absence of a capability concept: there is no way in a wheel
to say "I need *a* BLAS", only "I contain *this* BLAS".

### 2.2 conda

Conda was created for exactly this problem and it is the only system on this list
that was. Its insight — that the unit of distribution should be a **binary package
that carries its native dependencies as ordinary packages**, so that `libopenblas`
is a package like `numpy` is a package and the solver sees both — is correct, and
Science should adopt the shape of it.

**Copy:** three things. Native libraries as first-class packages so that a real
dependency solver sees them. Environments, so that two projects on one machine can
disagree. And **build variants** — conda's `blas=*=mkl` versus `blas=*=openblas`,
and the `__glibc` and `__cuda` virtual packages that let the solver reason about
properties of the *machine*. That variant machinery is the closest existing thing
to §4 of this note, and it is there because conda hit the same hazard.

**Do not copy:** the cost, which is large and which conda users pay daily. A
parallel universe of packages means everything must be repackaged, so the
ecosystem is always a subset and always behind, and `conda` and `pip` in one
environment is a known way to break an installation. The solver is a full SAT
solver over a repository index of hundreds of thousands of records and it was, for
years, so slow that a competing implementation (mamba) was written and eventually
absorbed. And channels — conda-forge versus defaults versus a vendor channel —
produce conflicts that present as unsolvable environments with no actionable
message. Science cannot afford to build and operate a package universe; it has no
NumPy to bootstrap from and no Anaconda Inc. to fund the build farm.

### 2.3 Spack and EasyBuild

The HPC answer, and the one a large part of Science's audience actually uses.
Spack builds from source, and its central idea is that a package is not a version
but a **spec**: `openblas@0.3.26 %gcc@13.2 +ilp64 threads=openmp target=zen4`.
Compiler, compiler version, build options, and *microarchitecture* are all part of
the identity, every distinct spec gets its own installation prefix, and the whole
thing integrates with Lmod so that a build becomes a `module load`.

**Copy:** the spec model, and specifically that **the ABI-relevant build options
are part of the identity, not metadata attached to it**. `+ilp64` being in the
hash is what makes it impossible to accidentally get the wrong one. §4 takes this
directly. Copy also the acknowledgement that microarchitecture matters: a
generically-built OpenBLAS is not the same product as one tuned for the node.

**Do not copy:** build-everything-from-source as the default path for a
*language toolchain*. Spack's first-run experience is measured in hours and
occasionally days, it requires a full working compiler toolchain including
Fortran, and its failure mode is a build log. That is acceptable for a facility
installation performed once by a staff member; it is not acceptable as what
happens when a graduate student types `sciencec run` for the first time.
`data-io.md` §9 already committed to protecting a machine with **no C++ compiler
present**, and a source-first design breaks that commitment outright.

### 2.4 Nix and Guix

Content-addressed, exact. Every package's identity is a hash of its complete
build closure — sources, compiler, every transitive dependency, every build flag
— so two machines with the same hash have provably the same bytes. This is the
only system on the list that delivers what "reproducible" actually means, and it
is the correct answer to §5's question in the abstract.

**Copy:** the notion that an artefact's identity should be a hash of its inputs
rather than a name and a version, and the notion that a build environment can be
described completely enough to be reproduced. Where this note offers a real
guarantee (§5.1, Tier A), that guarantee is Nix's guarantee, weakened.

**Do not copy:** the totalising requirement. Nix is exact because it owns
*everything*, down to the C library, and it achieves that by refusing to
interoperate with the host's filesystem layout. On an HPC system the user cannot
create `/nix`, cannot run a daemon, and — decisively — the site's tuned MPI and
its GPU driver stack are outside the store and must be brought in through escape
hatches that discard the guarantee anyway. It is also steeply unfamiliar: a
functional language must be learned before a package can be added, which for this
audience is a wall. Science should take the idea and not the ontology.

### 2.5 vcpkg and Conan

The two C++ answers, and instructive mainly for what they concede.

vcpkg is a curated port tree that builds from source, with **triplets**
(`x64-linux`, `x64-windows-static-md`) encoding platform, architecture, and
linkage. Binary caching was added later because source builds were too slow, and
that ordering is the lesson. Conan is a package manager with real binary
packages, where a package's **settings and options** hash into a package ID and
a missing binary falls back to a source build.

**Copy:** Conan's package-ID model — settings plus options hash to an identity,
and the same recipe produces many binaries — because it is the same idea as
Spack's spec arriving from the commercial side, which is evidence it is right.
And copy vcpkg's triplet discipline of making *linkage* part of the identity
rather than a flag, since a static and a dynamic build of the same library are
different artefacts with different obligations (§7.3).

**Do not copy:** either project's port/recipe surface area as a thing Science
maintains. vcpkg has over two thousand ports; the maintenance of a port tree is
the actual cost of these systems and it is unbounded. Science should curate a
deliberately small recipe set (§3.4) and be explicit that it is small.

### 2.6 Go's cgo and Rust's `-sys` convention

These are the two compiled-language precedents closest to Science's position, and
the `-sys` convention is the most directly applicable prior art in existence. It
deserves to be treated seriously rather than mentioned.

**Go's cgo** made one decision that Science has already made differently and one
that it should study. It has no native dependency mechanism at all: `#cgo
pkg-config: openblas` in a comment, and the rest is the user's problem. The
consequence is the interesting part — because cgo is awkward, expensive to call,
and breaks cross-compilation, **the Go ecosystem systematically rewrote C
libraries in Go**, and Go's culture of pure-Go implementations is a downstream
effect of its native dependency story being bad. Science cannot take that route:
`ffi-c-boundary.md` §0 is built on the claim that the floating-point work already
lives in somebody else's tuned library, and `scientific-libraries.md` §2 already
decided to link rather than rewrite for exactly the cases where it matters. Where
Go could escape the problem, Science has committed not to. **Copy:** nothing
structural. **Learn:** that a bad native story does not stay contained in the
build system; it deforms the library ecosystem around it.

**Rust's `-sys` crates** are the model. The convention is unlegislated and near
universal: a thin crate, named `foo-sys`, contains nothing but the `extern`
declarations for `libfoo`, plus a `build.rs` that locates the library — usually
via `pkg-config` — or, failing that, builds a vendored copy, and then prints
`cargo:rustc-link-lib=` and `cargo:rustc-link-search=` lines back to the build
system. A separate safe crate, `foo`, wraps it. Two further pieces make it work:
`links = "foo"` in the manifest, which declares that the crate owns the *native*
library `foo` and which Cargo uses to **reject any dependency graph containing two
crates claiming the same native library**; and the `DEP_FOO_*` environment
variables, by which a `-sys` crate publishes metadata (include paths, the variant
it chose) to its dependents.

**Copy, and take almost entirely:**
- The two-layer split — generated unsafe declarations in one unit, hand-written
  safe wrapper in another — which `ffi-c-boundary.md` §7.1 has already adopted
  independently, under the same reasoning. The convergence is worth noting.
- **`links`, which is the single most important line of prior art here.** It is
  the mechanism that turns "two packages disagree about the native library" from
  a silent runtime corruption into a resolver error. §4 is `links` with variants
  added.
- The ordered fallback inside one package: find it, else build the vendored copy.
- The vendoring *feature flag* convention (`openssl-sys` with and without
  `vendored`), which lets the same package serve the cluster user who wants the
  system library and the laptop user who wants it to just work.

**Do not copy:** `build.rs` itself. An arbitrary program that runs at build time
with full filesystem and network access is a supply-chain hole, it is the reason
Rust builds cannot be sandboxed by default, and it makes the build
non-reproducible in a way nothing downstream can detect. It is also a resolution
mechanism that cannot be queried without running it, so nothing can plan a build,
report what it will do, or pre-fetch for an offline machine — which §6 makes a
hard requirement. Science's equivalent must be **declarative data the toolchain
interprets**, not code the toolchain executes. That is the one substantive
departure from the `-sys` model and it is deliberate.

### 2.7 What the reading adds up to

Four things, which the rest of the note implements:

1. **Native libraries need a capability-and-variant identity, not a version.**
   Spack's spec, Conan's package ID, conda's build string, vcpkg's triplet and
   Cargo's `links` are five independent inventions of the same thing.
2. **Nobody has made source-first work as a default for a language toolchain**,
   and the two systems that tried (Spack, early vcpkg) added binary caching.
3. **Nobody has made binary-first work for tuned numerical code without
   accepting a large, permanent operational cost** (conda's build farm, manylinux's
   container discipline) *and* a performance loss on the exact workload this
   audience runs.
4. **Every system that tried to own the whole world had to add an escape hatch
   for the host's MPI, GPU driver, and site-tuned libraries** — and that escape
   hatch is where its guarantees end. Science should start where the others ended:
   with the escape hatch as the default path, made legible.

---

## 3. The core decision: source, binary, or system

### 3.1 Decision 1 — a declared, ordered provider list, keyed by the link name, with `system` first

> **Decision 1.** A package declares each native dependency in a `[native.<name>]`
> table in its manifest, where `<name>` is exactly the string used in
> `ffi-c-boundary.md` §5.1's `library "<name>"` clause. The table names a
> **capability**, an **ABI variant** (§4), a version range, a probe, and an
> **ordered list of providers**. The toolchain tries each provider in order and
> uses the first that succeeds. **The default list is `["system"]`**, and a
> package must opt in to anything else.

The four providers:

| Provider | What it is | Needs a network? | Needs a C toolchain? |
|---|---|---|---|
| `system` | `ffi-c-boundary.md` §5.1's search, unchanged: `--library-path`, `SCIENCE_LIBRARY_PATH`, system defaults, or `via pkg-config` | No | No |
| `prebuilt` | A content-addressed binary artefact for a platform triple, fetched once into the cache | At `fetch` time only | No |
| `source` | A content-addressed source tarball plus a declarative build recipe, compiled into the cache | At `fetch` time only | Yes |
| `vendored` | C sources shipped inside the Science package itself, compiled by the toolchain with no external build system | No | Yes, a C compiler only |

```toml
# In the manifest of the package that wraps BLAS.
[native.openblas]
capability = "blas"
variant    = "lp64"
version    = ">= 0.3.20"
provider   = ["system"]
pkg-config = "openblas"
probe      = "openblas_get_config"

# A small, self-contained C library. Vendoring is honest here.
[native.miniz]
capability = "deflate"
variant    = "default"
provider   = ["vendored"]
sources    = ["native/miniz.c"]

# A library taken for its security release cadence. See Decision 4.
[native.sodium]
capability       = "libsodium"
variant          = "default"
version          = ">= 1.0.18"
provider         = ["system"]
security-tracked = true
probe            = "sodium_library_version_major"
```

**Why `system` first.** Five reasons, and they compound:

- It is what `ffi-c-boundary.md` §5.1 already specifies, so the default path
  requires no new mechanism and no new failure modes.
- It costs no download and no build, so it is the only strategy that works
  unconditionally on the machines described in §6.
- **It is the only strategy compatible with a module system**, which is the
  mechanism §5.1 made dynamic linking the default to support. A cluster where
  `module load openblas/0.3.26-zen4` is how software is selected is a cluster
  where the right answer is to link against whatever that leaves under
  `libopenblas.so`, and any design that downloads its own copy is silently
  overriding a decision the site administrator made on purpose.
- **A prebuilt OpenBLAS is slower than the node's.** This is specific to this
  audience and it is the reason the ordering is not the obvious one. A portable
  binary is built for a baseline microarchitecture or, at best, ships runtime
  kernel dispatch with a generic blocking strategy; the site's build is tuned for
  the node, and for a `dgemm`-bound workload the gap is routinely 1.5–3×. For
  every other language's audience this is an abstract concern. For Science's it is
  the reason the cluster exists.
- It keeps security patches on the operating system's schedule, which
  `stdlib-standard.md` §13 and `stdlib-shape-and-packages.md` Decision 5c already
  rely on for `crypto`.

**Rejected alternative: prebuilt-first, the conda/wheel shape.** Rejected as the
*default* for the performance reason above, and for the operational one: it
commits the project to a build farm producing platform × architecture × variant ×
threading-model artefacts for every library in the catalogue, forever. That is
conda's actual cost and Science has no organisation behind it. It remains
available per-package and is the right choice in specific places (§3.3).

**Rejected alternative: source-first, the Spack shape.** Rejected because
`data-io.md` §9 explicitly protects a build with no C++ compiler present, because
a first run measured in hours loses the user before the language is evaluated,
and because building OpenBLAS correctly requires a Fortran compiler — which is
the exact wall §2.1 identifies as `pip install scipy`'s fifteen-year failure.

**Rejected alternative: one global strategy chosen by the user, not the package.**
Considered seriously, because it is simpler to explain. Rejected because the right
answer genuinely differs per library: `miniz` is one public-domain C file and
should always be vendored; OpenBLAS should always come from the system where a
system has one; libsodium must come from the system for a security reason;
Arrow C++ must be prebuilt because building it needs a C++17 toolchain the design
promised not to require. A single global knob would force all four to the same
answer. The user *does* get an override (§3.5), but the default is per-package
because the knowledge is per-package.

**Cost of Decision 1, stated.** The default path can fail. `sciencec build` on a
machine with no BLAS produces a diagnostic, not a download, and that is exactly
the first-experience problem `scientific-libraries.md` §15 warns about. Decision 3
is the answer, and without it Decision 1 would be wrong.

### 3.2 Decision 2 — the search order is refined, not replaced

> **Decision 2.** `ffi-c-boundary.md` §5.1's step 1 splits into **1a**, paths the
> user supplied with `--library-path`, and **1b**, paths contributed by resolved
> `prebuilt`, `source` and `vendored` providers. Steps 2 and 3 —
> `SCIENCE_LIBRARY_PATH`, then system defaults — are unchanged. A program with no
> `[native.*]` entries resolves exactly as §5.1 specifies.

This is named explicitly because it is a modification of a sibling note's
mechanism. The reason for the ordering within step 1 is that a user who passes
`--library-path` on the command line is making a deliberate, local override and
must win over anything the toolchain resolved on their behalf — otherwise there
is no way to test a replacement library without editing a manifest, which on a
cluster is a common and legitimate thing to need.

### 3.3 Where each provider is the right answer

Applying Decision 1 to the libraries the sibling notes have already named:

| Library | Provider | Why |
|---|---|---|
| OpenBLAS / MKL / Accelerate | `system` | §3.1's five reasons, all of them at once. Decision 3 covers the miss. |
| LAPACK | `system`, with BLAS | Same object on most installations. |
| libsodium | `system`, `security-tracked` | `stdlib-standard.md` §13.1's whole argument is that patches must arrive on libsodium's schedule. Vendoring it would discard that argument. |
| OpenSSL (`libssl`, TLS only) | `system`, `security-tracked`, and `when available` | `stdlib-standard.md` §13.3. Same reason, more urgently. |
| miniz | `vendored` | One public-domain C file with no build system. `data-io.md` §9 already vendors it. |
| PocketFFT | `vendored` | BSD, single file, what SciPy ships. `scientific-libraries.md` §2. |
| FFTW | `system`, `when available` | GPL-or-pay. Science must not distribute it, and PocketFFT is the fallback. §2 of that note already decided this. |
| libzstd | `system`, then `vendored` | `stdlib-shape-and-packages.md` §5.1 already specifies exactly this pair. |
| Arrow C++ (`libarrow`, `libparquet`) | `prebuilt`, then `source` | The one place prebuilt-first is right: `data-io.md` §9 promises a build with no C++ compiler, and the only way to keep that promise while offering Parquet is a binary artefact. Falling through to `source` is an error, not a silent fallback (§3.6). |
| SuiteSparse | `system` | Same argument as LAPACK, one rung less universal. |
| CUDA, cuDNN, ROCm | `system` only, `when available`, probe mandatory | §8. |

`data-io.md` §9 chose a **toolchain build feature** for Arrow explicitly because
no package manager existed, and said in terms that "when the package manager
arrives, this becomes an ordinary dependency and the module path does not change".
This note is that arrival. The `[native.arrow]` entry with
`provider = ["prebuilt", "source"]` is the ordinary dependency §9 anticipated, and
the guarantee it was protecting — no C++ compiler required — is preserved by
prebuilt-first plus Decision 6.

### 3.4 Decision 3 — nothing in the catalogue may be *unbuildable* without a native dependency

> **Decision 3.** Every standard or official Science package whose native
> dependency has provider `system` must also build and run with that dependency
> absent, on a vendored fallback path, and **must say so at run time the first
> time the fallback does work where the difference is material**.

This is the answer to `scientific-libraries.md` §15:

> *"A scientific language whose `linalg` requires the user to install OpenBLAS
> first will be judged on that first experience."*

Concretely, for `linalg`: a blocked `dgemm` written in Science, used when no BLAS
is found. It is correct, it is generic over `F32`/`F64` in a way a linked `dgemm`
is not, and it is slow — a careful blocked implementation without
per-microarchitecture assembly reaches perhaps 10–30% of a tuned OpenBLAS, and a
naive one reaches 2%. That is a real loss and it is the right trade, because the
alternative is a first run that fails.

**The fallback must announce itself.** On the first multiplication above a size
threshold, the fallback writes one line to stderr naming what it is, roughly what
it costs, and the command that would diagnose the machine:

```
science: linalg is using the built-in reference kernels; no BLAS was found.
science: expect 3-10x lower throughput on large problems. Run `sciencec doctor`.
```

**Rejected alternative: fail the build with a good diagnostic.** This is the
purist's answer and it is what `scientific-libraries.md` §2's "link, do not write"
rule implies if read narrowly. Rejected: a diagnostic on a first run is a
diagnostic the user reads as "this language does not work", and the note's own §15
predicts precisely that outcome. This is an *addition* to §2 rather than a
disagreement with it — §2 decides what the production implementation is, and this
decides what happens when the production implementation is absent.

**Rejected alternative: silently use the slow path.** Rejected because a
benchmark run against the fallback and published as "Science is 20× slower than
NumPy" is a permanent, unfixable reputational cost, and the only defence is that
the program said so while it was happening. **Cost:** a warning users will
eventually learn to ignore, and a line on stderr that a batch job's log will
contain forever.

**Cost of Decision 3, stated plainly.** It commits the project to maintaining a
second implementation of the numerical kernels that matter. That is not free and it
should be scoped tightly: the fallback exists for `gemm`, a few level-2 and level-1
operations, and the decompositions `linalg` cannot do without. It is not a second
LAPACK, and where no fallback is reasonable — sparse direct solvers, Parquet — the
package fails to build with `SC0477` naming exactly what is missing and how to
get it.

### 3.5 The user's overrides

Per-package defaults are defaults. The user gets, in increasing order of bluntness:

- `--native <name>=<provider>` to force one dependency's provider.
- `--native-provider <provider>` to set the whole program's preference order,
  which is how "build everything from source, I am on a machine with a compiler
  and I want it tuned" is expressed in one flag.
- `--library-path` and `SCIENCE_LIBRARY_PATH`, which are `ffi-c-boundary.md`
  §5.1's and are unchanged and still win (Decision 2).
- The manifest's own `[native.<name>]` table in the *root* package, which
  overrides a dependency's declaration for the whole build. This is the mechanism
  for "the library I depend on says `system`, but on this machine I need the
  vendored copy", and it must exist because the alternative is forking a package
  to change one line.

### 3.6 Decision 4 — falling through a provider list is announced, never silent

> **Decision 4.** When a provider list has more than one entry and the first fails,
> the toolchain **reports which provider it fell through to and why the earlier
> one failed**, once, at build time. Under `--locked` (§5.3), falling through is
> an error rather than a report.

The failure this prevents is the one that makes native dependencies infuriating:
a build that quietly took ten minutes longer and produced a different binary
because `pkg-config` was not installed. `security-tracked = true` additionally
forbids a fall-through to `prebuilt` or `vendored` entirely — a vendored copy of
libsodium is a copy that never receives a patch, and that directly contradicts
`stdlib-standard.md` §13.1's stated reason for choosing libsodium at all.

---

## 4. The ABI variant, which is the thing that must not be got wrong

`ffi-c-boundary.md` §1.6 documents the LP64/ILP64 hazard and solves it **at the
source level**, with two `extern` blocks, distinct `BlasInt` widths, and distinct
`symbol` strings so that the mistake becomes an undefined symbol at link time. That
solution is correct and this note does not touch it. It has one gap, which §1.6
itself names:

> *"Where a vendor ships ILP64 under the same symbol names — Intel MKL does, in
> `libmkl_intel_ilp64` — nothing can detect the mismatch, and the binding must say
> so in a comment because it is the only warning available."*

A comment is the only warning available **at the source level**. The packaging
layer has two more, and this section adds both. It is the single most dangerous
area in the design: the failure is silent, it is numerical, and it appears only at
scale, which means it appears after the code has been trusted.

### 4.1 Decision 5 — capability and variant are part of a native dependency's identity

> **Decision 5.** Every `[native.*]` entry declares a `capability` and a
> `variant`. The variant vocabulary belongs to the capability, not to the
> package: the capability `blas` defines exactly `lp64` and `ilp64`, and a
> manifest naming any other variant for `blas` is `SC0478`. The pair
> (capability, variant) is part of the cache key, part of any prebuilt
> artefact's identity, and part of the lock entry.

This is Cargo's `links`, Spack's `+ilp64`, Conan's package ID and conda's build
string, all of which exist because their authors hit this. An LP64 and an ILP64
OpenBLAS are different artefacts and must never share a cache slot, a lock entry,
or a download.

### 4.2 Decision 6 — two static rules, checked at resolution

> **Decision 6.** Within one program:
>
> - **Rule A.** Two packages that declare the same **link name** with different
>   variants is an error, `SC0471`. The diagnostic names both packages, both
>   manifest lines, and the `extern` blocks that use the name.
> - **Rule B.** Two packages that declare the same **capability** with different
>   variants is *permitted* only when their link names differ. It is the
>   `openblas` / `openblas64_` case `ffi-c-boundary.md` §1.6 writes out, which is
>   legitimate and must keep working.

Rule A is the important one and it is cheap: it is a comparison over the resolved
dependency graph, it requires no analysis of Science code, and it makes the most
dangerous mismatch in scientific computing a resolution failure with a readable
message.

**Rejected alternative: infer the variant from the `extern` block's `type BlasInt
is I32` line.** It is tempting, because the information is genuinely there in the
source and would need no manifest entry. Rejected for three reasons: the compiler
would have to know that `BlasInt` is special, which is a hard-coded library name
in a language core; it does not generalise to variants that are not integer widths
(threading model, complex-return convention, Fortran name mangling); and it cannot
be checked before the source is compiled, which means it cannot be a *resolver*
input, and the whole value of Rule A is that the resolver can reject the graph
before anything is built.

**Rejected alternative: no declaration, detect at link.** This is the status quo
`ffi-c-boundary.md` §1.6 already achieves via distinct symbols, and §1.6 already
states where it fails. It is not rejected so much as insufficient.

### 4.3 Decision 7 — the variant is verified at startup, and the check is not optional

Rule A cannot catch the case that actually kills people: the declaration says
`lp64`, and the `libopenblas.so` the module system resolved *today* was built with
`INTERFACE64=1` and without `SYMBOLSUFFIX`, so every symbol has the name the LP64
binding expects and the wrong width behind it. Nothing at compile time can see
this. Something at run time can.

> **Decision 7.** A `[native.*]` entry with a variant other than `default` must
> declare a `probe`: a function in the library that reports its own configuration.
> `science-rt` calls every declared probe once, before `main`, compares the result
> against the declared variant and version range, and **aborts with `SC0475` on a
> mismatch**. A library with no way to report its configuration declares
> `probe = "none"`, which is legal, is recorded in the provenance section as
> *unverified*, and is reported by `sciencec doctor`.

The probes exist and are cheap. `openblas_get_config()` returns a string
containing `USE64BITINT` when and only when the build is ILP64, which verifies the
variant exactly. `mkl_get_version()` fills a struct that reports the interface
layer. `cudnnGetVersion()`, `H5get_libversion()` and `cudaDriverGetVersion()` give
the version half. This is `ffi-c-boundary.md` §5.5's proposal — *"record the header
version at generation time, and emit an initializer that calls the library's own
version query and panics on a mismatch"* — **strengthened from a recommendation for
generated bindings into a requirement for anything with a variant**, and named as
such because it is a sibling note's mechanism being made mandatory.

**Cost:** one call and a string comparison per native dependency at startup, paid
once by a program that then runs for a week. For a short-lived program invoked in
a loop by a shell script it is a few microseconds. `--no-native-probe` exists for
the person who has measured it and cares; using it is recorded in the provenance
section, because a result produced with the check disabled should say so.

**Rejected alternative: a behavioural probe** — call `dgemm` with dimensions
constructed so that LP64 and ILP64 disagree about the answer. It works, it is
clever, and it is rejected: the probe writes memory, so a probe that is wrong about
the ABI corrupts the heap of a program that had not yet done anything, and
diagnosing *that* is worse than the bug it was looking for.

### 4.4 What is still not covered, stated

Three holes remain and they should be visible rather than implied:

1. **A library with no configuration query and a variant that matters.** `probe =
   "none"` is honest, not safe. The provenance record marks it and `doctor`
   reports it, and that is all that is available.
2. **A variant distinction the vocabulary does not have a name for.** Fortran
   name mangling (trailing underscore or not), complex return conventions (by
   value or by hidden pointer — the `f2c` versus `gfortran` split on `zdotc` is a
   real, current source of wrong answers), and 32- versus 64-bit `size_t` in a
   library's own callbacks are all ABI-relevant and none is in the initial
   vocabulary. The vocabulary must be extensible; §13 asks who arbitrates it.
3. **Threading model.** Two BLAS builds with different threading (pthreads,
   OpenMP, sequential) are ABI-compatible and *behaviourally* incompatible under
   a parallel Science loop, per §2.1's oversubscription problem. This note treats
   threading as a variant dimension so that it is at least recorded, but recording
   it does not fix the oversubscription, which belongs to whoever owns the
   parallel runtime.

---

## 5. Reproducibility, and its honest limit

`package-manager.md` owns the lockfile. This note owns the hard question: **what
can a lock entry for a native dependency honestly say?**

### 5.1 Three tiers, and the design should name them in the lockfile

> **Decision 8.** A native lock entry carries an explicit `tier`, and the tier is
> a first-class field rather than an implementation detail, because the
> guarantees differ by kind and a lockfile that hides that is worse than no
> lockfile.

**Tier A — content-addressed.** Providers `prebuilt`, `source` and `vendored`.
The lock records the SHA-256 of the exact artefact or source tarball, the hash of
the build recipe, the compiler identity used to build it, and the (capability,
variant) pair. Given the same inputs the same artefact is produced, and the
artefact is verified by hash before use. This is Nix's guarantee, weakened only in
that Science does not own the C compiler or the C library underneath.

**Tier B — resolved and recorded.** Provider `system`. The lock **cannot pin**
the library, and must not pretend to. What it can record is what was *found*:

```toml
[[native]]
name       = "openblas"
capability = "blas"
variant    = "lp64"
tier       = "system-recorded"
path       = "/opt/software/OpenBLAS/0.3.26-GCC-13.2.0/lib/libopenblas.so.0"
soname     = "libopenblas.so.0"
sha256     = "..."           # of the file, at the moment of the build
probe      = "OpenBLAS 0.3.26 USE64BITINT= MAX_THREADS=128 Zen4"
resolved   = "2026-09-16T10:41:02Z"
```

Every field there is true and none of it is a constraint. A build on another
machine will find a different file, and the strongest thing the toolchain can do
is **compare and report** (`SC0479`: a warning by default, an error under
`--locked`). That is a description, not a lock.

**Tier C — nothing at build time.** `ffi-c-boundary.md` §5.2's `when available`
libraries are resolved by `dlopen` during the run. At build time the lock can
record only that the table exists and what it expects. What was actually loaded is
knowable only at run time, and §5.2 below is the only place it can be captured.

### 5.2 The strongest honest guarantee, stated in one sentence

> **Science can guarantee that a build is reproducible up to its Tier A
> dependencies and the host's C library and compiler; for Tier B dependencies it
> guarantees only that what was used is *recorded*, and for Tier C only that what
> was loaded is *reported by the run itself*.**

This is weaker than conda's and much weaker than Nix's, and it should be stated in
the documentation in those words rather than softened. It is weaker **on purpose**,
because the alternative is to refuse the module system, and refusing the module
system means refusing the machines the language is for. conda buys Tier A by
declining to interoperate with the host; Spack buys it by rebuilding the world;
Science declines both and pays here. That is a defensible trade and it is not a
free one.

### 5.3 `--locked`, and what it means for each tier

`--locked` is the mode for a published artefact, a CI run, or a rerun of a
result. It means:

- Tier A: hashes must match. Any mismatch is an error.
- Tier B: the recorded `soname` must still be found, the probe's reported version
  must still satisfy the range, and the variant must still verify (§4.3). A
  changed `sha256` is `SC0479` as an *error*, with a message that names the old and
  new probe strings, because that is the actionable difference — "0.3.26 Zen4"
  versus "0.3.21 Haswell" tells a scientist something; two hashes do not.
- Tier C: nothing can be enforced at build time; the run's provenance output is
  where it is observed.
- Provider fall-through (Decision 4) is an error, not a report.

### 5.4 Decision 9 — every binary carries a provenance section

This is the part worth developing, because it is cheap and it is a real
contribution: **a scientific binary that can report the native libraries its
numbers came out of.**

> **Decision 9.** `sciencec` emits a `.science.provenance` section into every
> binary it links (`__SCIENCE,__provenance` on Mach-O, a named section on PE).
> It is a small, versioned, uncompressed record containing:
>
> - the compiler version and the target triple;
> - every Science package: name, version, source hash;
> - every native dependency: link name, capability, variant, provider, tier, and
>   the tier-appropriate identity from §5.1 — hash for Tier A, path and `SONAME`
>   and file hash and probe string for Tier B;
> - whether each variant was probe-verified, unverified (`probe = "none"`), or
>   skipped (`--no-native-probe`);
> - the contents of every `when available` table, marked *resolved at run time*;
> - the link mode (§7).

Two properties make it useful rather than ceremonial.

**It is readable without running the binary.** `sciencec provenance ./a.out`
prints it, and so does `readelf -p .science.provenance`. This matters more than it
sounds: on a cluster, running a binary means requesting an allocation and waiting
in a queue, and a reviewer reconstructing a two-year-old result may not have
access to a node with the right GPU at all. A record you must execute the program
to read is a record you often cannot read.

**Rejected alternative: a `--science-provenance` command-line flag handled by the
runtime before `main`.** Rejected twice over. A scientific program's argument
parser belongs to its author, and a compiler that reserves a flag name will
eventually collide with somebody's real option — and the collision will be
discovered by a user whose analysis script silently stopped doing what it did. And
it only works when the binary can be run, which is the case §5.4 above says cannot
be assumed.

**It is available to the program, so it can be stamped into outputs.** A runtime
function returns the static record *plus* the run-time resolution of every
`when available` table that has been initialised — which is the only place Tier C
information exists:

```science
use os (provenance, Provenance, NativeRecord)
use data.json (write_json)

def stamp_results(out: borrowed Path) -> ((), Error?):
    let record: Provenance be provenance()

    print(f"science {record.compiler_version} on {record.target}")
    for dep in record.native:
        if dep.verified is not "verified":
            print(f"  {dep.link_name} {dep.variant} ({dep.verified})")

    let err be write_json(out, record)
    if err?:
        return ((), err)
    return ((), null)
```

The intended use is that `data-io.md`'s writers do this without being asked: an
HDF5 file, an `.npz`, or a Parquet file written by a Science program should carry
the provenance record in its metadata, so that the artefact a paper cites knows
what produced it. That is an ask on `data-io.md` (§10) and it is the highest-value
thing in this note.

**Costs, stated.**

- **Size.** A few kilobytes for a typical program; the record should be capped
  (8 KB is generous) and should degrade by dropping Science package entries before
  native ones, because the native ones are the irreproducible half.
- **Information leak.** Tier B records absolute paths, which disclose a cluster's
  directory layout and sometimes a username. `--provenance=minimal` keeps hashes,
  `SONAME`s and probe strings and drops absolute paths; a published binary should
  use it. The mode itself is recorded, so a reader can tell that paths were
  stripped rather than absent.
- **It is not a bill of materials.** It describes what this link step saw, not the
  transitive native closure of the system libraries it found. `libopenblas.so` may
  itself pull in `libgomp` and `libgfortran` and the record will not say so unless
  the platform's loader is walked. §13 asks whether it should be.

---

## 6. The HPC constraint

A large share of the audience works on machines with these properties, and any
design that fails one of them fails for that share entirely:

- The compute node has **no network**. The login node does.
- The user **cannot write to system directories**, and often has a small `$HOME`
  quota with the real space on a separate `$SCRATCH` filesystem.
- **The module system swaps shared objects under fixed names**, which
  `ffi-c-boundary.md` §5.1 made dynamic linking the default in order to support.
- `/tmp` is small, node-local, and sometimes mounted `noexec`.
- A job that fails may have queued for two days first.

### 6.1 Decision 10 — the build never touches the network

> **Decision 10.** **`sciencec build` makes no network request, ever.** Fetching
> is a separate, explicit command — `sciencec fetch` — which is the only command
> in the toolchain that opens a socket. A build that needs an artefact which is
> not in the cache fails with `SC0472`, naming the artefacts and printing the
> exact `fetch` command that would obtain them.

This is a stronger statement than "offline mode is supported", and the strength is
the point. An offline *mode* is a mode you can forget to be in, and the place you
discover you forgot is inside a batch job on a node with no route to the internet,
two days after you submitted it. Making the invariant unconditional means the
failure is discovered on the login node, in the second before you type `sbatch`.

**Rejected alternative: fetch on demand, with `--offline`.** This is what Cargo,
pip and vcpkg all do. Rejected for the reason above, and for a second: a build that
*may* fetch cannot be sandboxed, cannot be audited, and cannot have its duration
predicted, and all three matter on a shared facility.

**Cost:** two commands where other toolchains have one, and a diagnostic the first
time a user adds a dependency and types `build`. `SC0472`'s text should be written
to make that first encounter instructive, since it will be most users' introduction
to the split.

### 6.2 The cache, and where it lives

- `SCIENCE_HOME` (default `~/.science`) holds configuration.
- **`SCIENCE_CACHE` is separately settable** and holds fetched and built native
  artefacts. It is separate precisely because `$HOME` on a cluster commonly has a
  quota measured in gigabytes and a prebuilt Arrow is tens of megabytes; pointing
  the cache at `$SCRATCH` must be one environment variable, not a reinstallation.
- Nothing is ever written outside those two, and nothing requires elevated
  privileges. This is not a guideline; a toolchain that wants to write to
  `/usr/local` is a toolchain a facility will not install.

### 6.3 `sciencec vendor` — making the project tree self-sufficient

Copies every source tarball and prebuilt artefact the lockfile names into a
directory inside the project, and rewrites the lock to point at them. The result
is a tree that can be `rsync`ed or carried on a disk to an air-gapped machine and
built with no cache and no network. This is the mechanism for facilities with no
outbound route at all, which is a real and growing category.

### 6.4 `sciencec bundle` — the artefact you hand to somebody else

> **Decision 11.** `bundle` produces a **relocatable directory**, not a single
> file: the binary, every shared object it actually depends on except a declared
> host-provided set, and a launcher that sets the loader search path.

The host-provided set is `libc`, `libm`, `libpthread`, `libdl`, `librt`,
`libstdc++` where it is the system's, and the entire GPU driver stack — because
`libcuda.so` comes from the kernel driver and bundling it produces a directory
that works only on the machine it was built on, which is the opposite of the
point.

**Rejected alternative: a single self-extracting binary**, AppImage-style.
Attractive, and rejected on the environment: it unpacks to a temporary directory,
`/tmp` on a compute node is small, node-local and sometimes `noexec`, and the
failure is an exec error inside a job rather than anything legible. A directory
plus a launcher has none of those properties and is what module files already
expect to point at.

**What bundle does not promise.** It is not a container and it is not reproducible
in Tier A's sense: it captures what *this* machine resolved. Its purpose is
portability to a similar machine and archival alongside a paper, and the provenance
section (§5.4) travels inside it so that a recipient can see exactly what they
received.

### 6.5 `sciencec doctor`

A single command that reports what the machine has: which BLAS was found and
where, which variant it declares and whether the probe agreed, which CUDA runtime
and driver and whether their versions are compatible, whether `pkg-config` exists,
what is in the cache, and which native dependencies of the current project would
resolve and how.

It is listed as a decision rather than a nicety because native dependency problems
are the single hardest class of user support question in every language on §2's
list, and the difference between a question that can be answered and one that
cannot is whether the reporter can paste one command's output.

---

## 7. `self-contained binaries`, revisited

§3 of the core spec lists **self-contained binaries** as a headline decision, in
the row that chose native AOT via LLVM over a VM. §3.3 of this note commits the
default `linalg` build to dynamically linking OpenBLAS, and a binary that
dynamically links OpenBLAS is, by any plain reading, not self-contained. Both
cannot be true as written, and the contradiction must be resolved rather than left
for a user to find.

### 7.1 Decision 12 — what the promise means

> **Decision 12.** "Self-contained" means: **no runtime to install, no
> interpreter, no VM, no garbage collector, no `site-packages`, and no Science-level
> dependency resolved at run time.** A Science binary needs no Science installed on
> the machine that runs it. It does **not** mean, and was never able to mean,
> statically linked.

The comparison the promise is against is Python's, because that is the audience's
baseline: shipping a Python analysis means shipping an interpreter version, a
`requirements.txt`, an environment that must be recreated, and a prayer. A Science
binary is one file that runs, whose dynamic dependencies are the platform's C
library and whatever tuned numerical libraries the user explicitly asked to link
against. That is a large, real improvement and it is what the core spec's row was
buying. Restating the promise in these words costs nothing and removes a claim the
project cannot keep.

**This is an amendment to the core spec's §3 and is offered as one**, in the sense
the README's conventions require: the note names the section, and the reason is
that the sibling `ffi-c-boundary.md` §5.1's default — dynamic, deliberately,
because HPC module systems swap the shared object under a fixed name — is
incompatible with the strong reading, and §5.1's reason is better than the strong
reading's.

### 7.2 Decision 13 — static linking is a supported mode, not the default

> **Decision 13.** `--link-mode=static-native` sets `kind static`
> (`ffi-c-boundary.md` §5.1) for every native dependency that permits it, and
> resolves `system` providers that cannot be statically linked as an error naming
> each one. The default remains dynamic.

It is offered because there is a real use for it: a binary archived with a paper,
a container image where the module system is irrelevant, a collaborator on a
machine you cannot inspect.

### 7.3 What static linking costs, in full

Four costs, and the first is decisive for the default:

1. **It defeats the module system.** `ffi-c-boundary.md` §5.1 says it in one
   sentence — *"statically linking OpenBLAS defeats the mechanism the cluster
   administrator is relying on"* — and that is the entire reason this is a mode
   and not the default. A statically linked binary cannot be retargeted to a
   different node type by a `module swap`, which on a heterogeneous cluster means
   it runs at the speed of whichever partition it was built on.
2. **glibc cannot be fully statically linked safely.** `getaddrinfo` and the name
   service switch `dlopen` `libnss_*` at run time, so a "static" glibc binary
   still needs matching shared objects and fails in ways that look like network
   errors. If genuinely static is required, the answer is musl or a glibc
   *floor* — building against the oldest glibc that must be supported, which is
   manylinux's technique (§2.1) — not `-static`.
3. **Licensing obligations change.** This is the cost most likely to be
   discovered too late, and it is checkable, so the toolchain should check it.
   Dynamically linking an LGPL library and distributing the result is ordinarily
   unproblematic; statically linking it imposes obligations on the distributed
   binary. GPL libraries in the catalogue's orbit — FFTW without a commercial
   licence, GSL, and `libgfortran` (GPL with a runtime exception whose conditions
   depend on how the program was compiled) — each behave differently.

   > **Decision 14.** Each `[native.*]` entry carries a `license` field, and
   > `--link-mode=static-native` emits `SC0473`, a warning, for every library
   > whose licence is on a "static linking changes your obligations" list. The
   > licence is also recorded in the provenance section.

   `sciencec` is not a lawyer and the diagnostic must not pretend to be one; its
   text should name the library, the licence, and the fact that the obligation
   differs, and stop there. The value is that the user learns it at link time
   rather than from a compliance email.
4. **Size, and the loss of shared pages.** A statically linked OpenBLAS with
   runtime kernel dispatch is tens of megabytes, and an MPI job launching 512
   ranks of it on a node loses the page sharing that a shared object gives for
   free. On a fat node this is measurable in gigabytes of resident memory.

### 7.4 What static linking cannot do at all

`ffi-c-boundary.md` §5.2's `when available` libraries are `dlopen`ed by
construction. **`--link-mode=static-native` therefore cannot make a CUDA program
self-contained**, and must say so rather than silently producing a binary that is
static except for the part that matters. A program with any `when available`
dependency gets a note in the link output naming them, and they appear in the
provenance record as Tier C regardless of link mode.

---

## 8. GPU

CUDA, cuDNN and ROCm are enormous, heavily versioned, licence-encumbered, and
must be optional. `ffi-c-boundary.md` §5.2 already handles the calling side: the
block is `library "cudnn" when available`, it lowers to a lazily-initialised
`dlopen` table, and the block gains an `is_available()`. What remains is the
packaging side, and it is short because the answer is mostly "no".

### 8.1 Decision 15 — Science ships no GPU library, ever

> **Decision 15.** GPU native dependencies are **`system` provider only**,
> **`when available` always**, and **probe mandatory**. A manifest that declares
> `prebuilt`, `source` or `vendored` for a library on the non-redistributable list
> is `SC0474`.

Three reasons:

- **The driver half cannot be shipped at all.** `libcuda.so` and the AMD
  equivalents belong to the kernel driver and are a property of the machine. No
  packaging system on §2's list ships them, including conda, which ships the
  toolkit and declares the driver as a virtual package the solver may only read.
- **The toolkit half is licence-encumbered** in ways a language toolchain should
  not be in the business of interpreting. cuDNN's redistribution terms in
  particular have changed more than once, and the cost of getting it wrong is
  borne by the project rather than the user.
- **Size.** A CUDA toolkit plus cuDNN is several gigabytes, versioned along at
  least two axes, and the matrix of (CUDA version × cuDNN version ×
  architecture) is large enough that hosting it is a serious infrastructure
  commitment with no relationship to designing a language.

### 8.2 The three layers, and where each comes from

Being explicit about the split is what makes the decision coherent:

| Layer | Example | Where it comes from |
|---|---|---|
| Driver | `libcuda.so`, `amdgpu` | The host. Never packaged, never bundled, never in `bundle`'s output. |
| Toolkit | cuBLAS, cuFFT, cuRAND, cuDNN, hipBLAS | User-installed — module system, vendor installer, or conda. **Found**, not fetched. |
| Compiler and IR | PTX, and whatever F2 emits | The Science toolchain's own problem, not a native dependency. |

### 8.3 Decision 16 — the version check happens where the table is already lazy

> **Decision 16.** A GPU `[native.*]` entry declares a version range and a probe,
> and the probe runs **when the `when available` table is first initialised**,
> not at startup. Failure is a panic naming the version found, the range wanted,
> and the library's path — never an undefined symbol and never a wrong answer.

This is a deliberate exception to Decision 7's "before `main`", and the reason is
that the table is already lazily initialised by `ffi-c-boundary.md` §5.2's design:
checking at startup would force the `dlopen` to happen on every run including the
CPU-only ones, which is exactly what §5.2 exists to avoid. Piggy-backing on the
existing lazy initialisation costs nothing.

The checks matter because GPU version compatibility is genuinely intricate: cuDNN
8 and 9 differ in API, CUDA minor-version compatibility means a toolkit works
against a range of drivers but not all of them, and a mismatch currently surfaces
as a status code deep inside a training loop. Turning it into a startup-of-first-use
panic with three named versions is a large usability improvement for a small amount
of work.

### 8.4 What is deferred, and why

A registry recipe that *points at* a vendor download and records its hash without
redistributing it is legally straightforward and is what Spack does. It is deferred
because it requires a licence acceptance step, which is interactive and networked,
and Decision 10 makes the build neither. If it is added later it belongs in
`sciencec fetch`, on the login node, with the acceptance recorded in the cache —
which is a coherent design and simply not one this note needs yet.

---

## 9. Diagnostics allocated

The README allocates `SC0471`–`SC0479` to this note. That block sits inside the
`SC0460`–`SC0479` sub-range `ffi-c-boundary.md` §8 reserved for **Linking**, which
is named here rather than left to be discovered: this is a coordinated
sub-allocation within a sibling's reserved range, not an independent claim, and
`SC0461` (undefined symbol) remains that note's. `SC0460`, `SC0462`–`SC0470`
remain free within it.

| Code | Meaning |
|---|---|
| `SC0471` | Two packages declare the same native link name with different ABI variants (§4.2, Rule A). Names both packages, both manifest lines, and the `extern` blocks. |
| `SC0472` | A native dependency is not in the cache and the build makes no network request (§6.1). Prints the `sciencec fetch` command that would obtain it. |
| `SC0473` | Static linking a library whose licence makes that a distribution obligation (§7.3). Warning. |
| `SC0474` | A manifest declares `prebuilt`, `source` or `vendored` for a library Science does not redistribute (§8.1). |
| `SC0475` | A native dependency's probe disagreed with its declared variant or version range (§4.3). Rendered by `science-rt` before `main`; the text is owned here so that it matches the compile-time diagnostics. |
| `SC0476` | The resolved system library's version is outside the declared range, detected at resolution rather than by the probe. |
| `SC0477` | No provider in the list succeeded. Names every provider tried and why each failed, in order. |
| `SC0478` | A `[native.*]` entry names a variant that is not in its capability's vocabulary (§4.1). |
| `SC0479` | The lockfile's Tier B record does not match what was found on this machine (§5.3). Warning by default, error under `--locked`. Reports the probe strings, not the hashes. |

---

## 10. What this note asks of others

`package-manager.md` did not exist when this was drafted. These are stated as
asks, and each is a place where the two notes must agree or one is wrong.

**Of `package-manager.md`:**

1. **A `[native.<name>]` table in the manifest**, keyed by exactly the string in
   `ffi-c-boundary.md` §5.1's `library "<name>"` clause. The key is the link name
   because the link name is what the linker and the loader see, and any other key
   introduces a mapping that can be wrong.
2. **`capability` and `variant` must be resolver inputs, not metadata.** The
   resolver has to be able to *fail* a dependency graph on Rule A (§4.2). If they
   are inert fields attached to a resolved graph, `SC0471` becomes a post-hoc
   check that runs too late to explain which dependency edge caused it. This is
   the most important of these asks and it is Cargo's `links`.
3. **A `[[native]]` array in the lockfile carrying the `tier` field** (§5.1), with
   different required fields per tier. A lockfile schema that forces a native
   entry into the same shape as a Science entry will force Tier B to fabricate a
   version.
4. **`sciencec fetch` as the only networked command** (§6.1), which is a property
   of the whole tool and not of this note's half of it.
5. **`--locked` semantics that admit tier-dependent enforcement** (§5.3).
6. **A root-package override of a dependency's `[native.*]` table** (§3.5).

**Of `ffi-c-boundary.md`:** nothing that changes its text, and two things named
so they are not silent — §5.1's step 1 is refined into 1a/1b (Decision 2), and
§5.5's version-query recommendation is promoted to a requirement wherever a
variant is declared (Decision 7). Deliberately *not* asked for: any new syntax in
the `extern` block. The capability and variant live in the manifest and the
diagnostics point at both the manifest line and the `library` clause, because
adding a clause to a sibling's grammar to carry information that a manifest
already has is a bad trade.

**Of `data-io.md`:** that the writers stamp the provenance record (§5.4) into
output metadata by default — HDF5 attributes, `.npz` as a member, Parquet
key-value metadata — with a flag to suppress it. This is the single highest-value
consumer of Decision 9 and it is nearly free at the point of writing.

**Of the core spec:** the amendment in §7.1 to §3's "self-contained binaries",
offered as a restatement of what the promise means rather than a retraction of it.

**Of `README.md`:** a row for this note in the index and in the diagnostic
allocation table, claiming `SC0471`–`SC0479`. The README's convention is that a
note adds its row before writing; this note could not, because its brief forbade
editing any file but its own. **That row is missing and should be added by
whoever lands this**, and the omission is exactly the kind of invisible collision
the README's §"Diagnostic code allocation" was written to prevent.

---

## 11. Risks

**Tier B is the common case, and Tier B is not reproducible.** The default path
for the most important library in the language produces a lock entry that is a
description rather than a constraint. This is the design accepting a real loss for
a real reason (§5.2) and it will be criticised by anyone who arrives from Nix or
conda. The defence is the provenance record, and the defence is only as good as
whether people actually read it.

**The recipe set is a build farm in disguise.** The moment `source` and `prebuilt`
are more than a fallback, the project has implicitly promised that its recipes
build on every platform it claims, which is conda's and vcpkg's actual cost
centre and is unbounded. Mitigation: `system` is the default, the recipe set stays
deliberately small and curated, and the documentation says it is small rather than
implying a catalogue.

**Windows is much weaker and it is not fixable here.** There is no `pkg-config`,
no system library directory convention, and no equivalent of a module system, so
the `system` provider — the default, and the load-bearing one — mostly does not
work. `prebuilt` becomes the de facto default there, which means the platform where
Science most needs a binary distribution is the one where its audience is smallest
and the investment is hardest to justify. vcpkg is the only real answer and
integrating with it is a decision this note does not make.

**macOS has two specific traps.** Accelerate provides BLAS and LAPACK but at a
LAPACK 3.9-era API surface with its own quirks, so `capability = "blas"` resolving
to Accelerate is not equivalent to resolving to OpenBLAS in the way the capability
concept implies. And `libgfortran` from Homebrew versus a different toolchain's is
a live, current source of link failures. Both need explicit handling that §3.3's
table does not yet have.

**The reference fallback could become load-bearing.** If Decision 3's fallback is
good enough to be tolerable, users will run on it without noticing, and the first
public benchmark of Science will be a benchmark of the fallback. The stderr warning
is the entire mitigation and it is thin. It is worth revisiting whether `doctor`
should be run automatically on a first build.

**A variant vocabulary that is wrong is worse than none**, because it looks like a
guarantee. §4.4 lists three holes already; each one is a case where a user could
reasonably believe the system checked something it did not. The documentation must
be as explicit about what `variant` does not cover as §4.1 is about what it does.

**Two providers of one capability in one process.** §2.1's oversubscription
problem is not solved by anything here. If a Science program links OpenBLAS and
also loads a Python extension that bundled its own, there are two BLAS libraries
and two thread pools in one address space, and neither this note nor
`python-interop.md` currently owns that.

---

## 12. Summary of decisions

| # | Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | Ordered provider list per native dependency, keyed by the link name, `system` first | Prebuilt-first (conda/wheels); source-first (Spack); one global strategy | Module systems; a tuned system BLAS beats a portable one by 1.5–3×; no build farm; the right answer differs per library | The default path can fail on a bare machine — Decision 3 exists to cover it |
| 2 | §5.1's step 1 refined into 1a (user paths) then 1b (resolved providers) | Resolved providers ahead of `--library-path` | A deliberate local override must win, or a library cannot be swapped without editing a manifest | A named modification of a sibling's mechanism |
| 3 | Everything in the catalogue builds and runs with its `system` dependency absent, on an announced fallback | Fail the build with a good diagnostic; fall back silently | `scientific-libraries.md` §15's first-experience warning; and a silent slow path becomes a published benchmark | A second implementation of the kernels that matter, at 10–30% of tuned speed |
| 4 | Provider fall-through is reported, and is an error under `--locked` | Silent fall-through | A build that quietly changed strategy is a build nobody can explain | One more line of build output |
| 5 | Capability and variant are part of a native dependency's identity | Version alone | Five prior systems independently invented this; LP64/ILP64 is not a version | A vocabulary to define and arbitrate |
| 6 | Same link name with two variants is `SC0471`; same capability with two link names is legal | Infer the variant from `BlasInt`; detect only at link | The resolver must be able to reject the graph before anything is built; §1.6's two-block pattern must keep working | A manifest field that can be declared wrong — Decision 7 covers it |
| 7 | The variant is probe-verified before `main`, mandatorily | Trust the declaration; a behavioural probe | `ffi-c-boundary.md` §1.6 names the MKL case that source-level symbols cannot catch | One call at startup; `probe = "none"` is honest but unsafe |
| 8 | Lock entries carry an explicit `tier` | One uniform native lock entry shape | Tier B cannot pin anything and a schema that hides that forces it to fabricate a version | The lockfile admits it is not a lock for the common case |
| 9 | A `.science.provenance` section in every binary, readable without running it, plus a runtime accessor | A `--science-provenance` flag handled before `main` | The author owns the argument parser, and on a cluster a binary often cannot be run | A few KB, an absolute-path leak that `--provenance=minimal` handles |
| 10 | `build` never touches the network; `fetch` is the only networked command | Fetch-on-demand with `--offline` | An offline mode is a mode you can forget to be in, and the place you find out is a queued batch job | Two commands where others have one |
| 11 | `bundle` exports a relocatable directory, not a single file | A self-extracting single binary | `/tmp` on a compute node is small, node-local and sometimes `noexec` | Not one file, and not reproducible in Tier A's sense |
| 12 | "Self-contained" means no runtime, not statically linked | Keep the strong reading and static-link by default | `ffi-c-boundary.md` §5.1's reason for dynamic-by-default is better than the strong reading | A headline promise restated more narrowly, as an amendment to core spec §3 |
| 13 | `--link-mode=static-native` exists as a mode | Static by default; no static option at all | Archival and container use are real; module systems are more common | Four costs, §7.3 |
| 14 | A `license` field, and `SC0473` when static linking changes the obligation | Say nothing about licences | It is checkable, and the alternative is learning it from a compliance email | A list to maintain, and a warning that must not pretend to be legal advice |
| 15 | Science ships no GPU library; `system` only, `when available` always, probe mandatory | Ship a CUDA toolkit; vendor cuDNN | The driver cannot be shipped; the toolkit is licence-encumbered and gigabytes | GPU users must install the toolkit themselves, as they already do |
| 16 | The GPU probe runs at first table initialisation, not at startup | Probe everything before `main` | §5.2's table is lazy precisely so CPU-only nodes never `dlopen` | A named exception to Decision 7 |

---

## 13. Open questions for the core team

1. **Who owns the recipe registry's platform matrix?** §11 names this as the thing
   that kills these projects. A curated recipe set needs someone to notice when
   OpenBLAS 0.3.30 changes a build flag, and that is an operational commitment with
   no natural owner in a language design document.
2. **Who arbitrates the variant vocabulary, and can third parties extend it?**
   §4.1 makes the vocabulary belong to the capability. If a third-party package
   introduces `capability = "mpi"` with variants for the ABI splits between MPICH
   and Open MPI — which is a real and painful distinction — it needs a namespace
   and a way to be registered. If the vocabulary is closed, Science's own catalogue
   is the only thing that can use variants at all.
3. **Should the provenance record be a recognised SBOM format?** §5.4 specifies a
   small native format. SPDX or CycloneDX would be read by tools that already
   exist, at the cost of a much larger record. A plausible answer is the small
   format in the binary plus `sciencec provenance --format=spdx` on the way out,
   but that is a decision, not an obvious default.
4. **Should the provenance record walk the transitive native closure?** Today it
   records what the link step saw. Walking the loader's view would capture
   `libgomp` and `libgfortran` underneath `libopenblas`, which is genuinely part of
   what produced the numbers, and it costs a platform-specific traversal.
5. **Should `system` be permitted at all in a "published artefact" mode?** There is
   a coherent stricter position — that a binary intended for archival must have no
   Tier B dependencies — which would give a real reproducibility guarantee to the
   people who need one, at the price of forcing a source or prebuilt build. This
   note does not take that position, and someone should decide whether it should be
   available as a mode.
6. **Who owns thread-pool coordination across native libraries?** §4.4 hole 3 and
   §11's last risk are the same problem seen from two sides, and neither this note
   nor `collections-and-chains.md` nor `python-interop.md` currently claims it.
