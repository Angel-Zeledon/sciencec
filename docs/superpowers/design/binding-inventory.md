# Science — Design: how Science gets a binding inventory

Date: 2026-09-16
Status: draft for review
Phase: spans F0 (the tooling decisions) through F3 (the inventory itself)
Depends on: `c-binding-coverage.md`, `rust-binding-generation.md`,
`python-from-science.md`, `ffi-c-boundary.md`, `python-interop.md`,
`package-manager.md`, `native-dependencies.md`, `models-and-inference.md`,
`data-io.md`, `scientific-libraries.md`, `rust-interop.md`,
`stdlib-shape-and-packages.md`.
Syntax: `syntax-revision-2.md` throughout.

Diagnostics: **this note claims no code.** §9 asks for two, from ranges other
notes own, for the reason `c-binding-coverage.md` §7 gives — a diagnostic
belongs in the allocation table where a reader will look for it, not in the
block of whichever note happened to need it.

---

## 0. The question, and the answer up front

The project's owner has stated the requirement plainly:

> *every library written in C or Rust must be usable, the way Python does it.*

Three sibling notes have now measured the technical reachability and it is high.
`c-binding-coverage.md` puts C at 64.9% directly bindable, ~91% after four
`ffi` additions, ~95% as a defensible ceiling for stable-ABI C libraries.
`rust-binding-generation.md` puts the user-facing surface of `regex`, `polars`
and `serde_json` at 100% / 95.1% / 91.4% reachable **with no Rust written**.
`python-from-science.md` puts real Python *programs* at ~80% in hosted mode.

**That is not the question.** Python's advantage was never a better FFI. Python's
`ctypes` and `cffi` are worse than what `ffi-c-boundary.md` specifies, and
Python's *automatic* coverage of C is approximately zero. Python's advantage is
**thirty years of people hand-writing binding layers**. `numpy` is a hand-written
C extension. `scipy.linalg` is thousands of lines of Cython over LAPACK. `torch`
is a hand-maintained C++/Python membrane over two thousand operators. PyO3
requires a human annotation per exported function. Every one of those is human
work that did not generate and will not generate.

So: **Science can match the mechanism today. It cannot match the inventory.**
This note is about how it gets one.

The answer, in four sentences.

> **First: the three routes rank in the inverse order of their necessity.** Going
> through Python's existing bindings delivers the largest inventory for the least
> work and is needed least, because it delivers that inventory to a *Python
> process*. Direct C binding delivers the least per unit of work and is needed
> most, because BLAS, CUDA and ONNX Runtime have no other route. So the ranking
> by yield is not the allocation.
>
> **Second: routes 1 and 2 are not alternatives, they are a routing rule,** and
> the rule turns on one question that is not the one it looks like — not *what
> language is this written in* but **does the compiled core exist as a library
> you can link without CPython**. §3 works that question over the ML stack, and
> it reclassifies three of the libraries a straight reading would get wrong.
>
> **Third: the 4% is real, and it is not the binding constraint on the seed
> inventory,** because the seed inventory was chosen from libraries where it is
> not. It is the binding constraint on the long tail, and the answer to the long
> tail is to ship the raw layer as a first-class, distributable, documented
> artefact — and to generate the safe layer from the sidecar instead of from the
> header, which is the single highest-leverage unbuilt thing in the project.
>
> **Fourth: "like Python does it" decomposes into four claims with four
> different answers** — *I can call any C library* (yes, roughly now), *I don't
> have to think about memory* (no, and never, and that is the language's thesis),
> *the library I want is already bound* (no, and not this decade), *and it is
> fast* (yes, and better than Python's, because there is no interpreter between
> the call sites). §8 says exactly that.

---

## 1. Method, and what is measured versus recollected

`c-binding-coverage.md` §1 sets the practice this note follows: say which numbers
were counted and which were not, and say it before the numbers rather than after.

**Counted by a sibling note, and quoted here:** every percentage in
`c-binding-coverage.md` §2 and `rust-binding-generation.md` §2, with those notes'
own caveats attached — CPython was measured from real headers, the other four C
libraries were estimated, and the rustdoc classifier was a throwaway script.
Every engineer-week figure in `models-and-inference.md` §1 and `data-io.md` §3.

**Recollected by this note and not measured:** the implementation language and
build shape of each Python package in §3.2's table, and the ecosystem-size
figures in §8.3. These are the author's knowledge of these projects as of late
2026, not a survey. They are the kind of fact that is easy to check and easy to
be a version out of date on. **§3.2's table should be re-derived by reading each
project's build configuration before any row of it is quoted outside this
document.** The three reclassifications in §3.3 are the load-bearing part, and
each of them is argued from a fact a sibling note already states, so they survive
an error in the table around them.

**Not measured by anybody, and this is the note's weakest joint:** the ratio in
§4.3 — how much cheaper a safe wrapper gets when it is generated from a sidecar
entry rather than written by hand. §4.3 offers an order-of-magnitude estimate
with its reasoning exposed, and §10 records that the estimate is the thing the
whole recommendation turns on.

---

## 2. The three routes

### 2.1 Route 1 — through Python's existing bindings

`import numpy` from Science and you inherit every wrapper anyone ever wrote.

**What it delivers.** The largest inventory available to any language on earth,
at a marginal binding cost of zero per library. `python-from-science.md` §1 states
why, and the statement is the strongest single argument in these three notes for
any route:

> *the unit of binding work is **nothing***

because `PyObject_GetAttr`, `PyObject_Call`, `PyObject_GetItem` and
`PyObject_GetIter` are "uniform, total, and closed". One binding, written once,
reaches every package on PyPI — including packages published after Science ships,
which is a property no native route has and never will.

**What it costs.** `python-from-science.md` §1 also states the price, and states
it as an identity rather than a trade:

> *The binding cost and the deployment cost are not two facts about Python. They
> are one fact seen from two sides, and any design that takes the first without
> paying the second is lying about one of them.*

Concretely, four costs.

1. **Python owns the process, or the binary is not self-contained.** In hosted
   mode the Science code is a CPython extension and the interpreter is already
   there. In standalone mode — reopened by `python-from-science.md` §4 and
   permitted under Decisions 10–15 — the binary declares `self_contained = false`
   in its provenance record and needs an interpreter, of a declared version and
   GIL variant, with every `[python.requires]` distribution resolvable, on the
   machine where it runs.
2. **Nothing is checked.** `python-from-science.md` §1's table gives "What the
   compiler can check about a call" as "nothing, ever". And §5.3 states the
   consequence that should worry the project most: *"the fraction of a Science
   program that is untyped is proportional to how successful this feature is.
   The better §2 works, the worse §6.1 gets."*
3. **Two ceilings, both structural.** `python-from-science.md` §6: Python code
   that introspects or serialises a type Science defined, and Python code that
   expects to be the process. The first is close to universal in ML code — every
   custom `sklearn` estimator under `clone`, every `torch.nn.Module` subclass.
4. **Two native dependency graphs in one address space.** A program that links
   `[native.openblas]` and imports NumPy has two BLAS builds and two thread pools
   (`python-from-science.md` §4.7). Science's resolver cannot see the second.

**How fast.** Available now in design, F2 in the phase table. No per-library work
at any point, ever.

**What it forecloses.** Nothing permanently — the route is additive and its
default is off. But every hour a user spends happy inside route 1 is an hour the
native inventory did not get written, and §7's falsification criterion exists
partly because of that.

### 2.2 Route 2 — rustdoc-driven generation

`sciencec foreign bind` reads `cargo rustdoc --output-format json`, and a policy
file of a dozen-odd entries, and emits the Rust shim, the Science declarations
and the safe Science layer. **No Rust is written by a human.**

**What it delivers.** `rust-binding-generation.md` §2: 100% of `regex`'s
user-facing surface from 15 policy entries; 91.4% of `serde_json`'s from 13, of
which one covers 52; 95.1% of `polars`' from 111, of which five cover 80% of the
demand and one covers 633 functions. `rust-interop.md` §2.4 priced the same
`regex` binding at *~215 lines of Rust written by a human*. Fifteen TOML entries
is not 215 lines of Rust.

**And this is something Python does not have.** There is no widely-used tool that
takes an unmodified Rust crate and produces a Python module. PyO3 is
hand-annotated — `#[pyfunction]`, `#[pymethods]`, one human annotation per
exported item. `maturin` builds; it does not generate. `pyo3-stub-gen` emits
`.pyi` from code that is already annotated. On this axis Science would be ahead
of Python, and it is worth stating plainly.

**It is also worth testing for overclaim, and four things cut against it.**

1. **The generator does not exist.** `rust-binding-generation.md` §1.3 says so:
   *"The classifier is a throwaway research script and is deliberately **not**
   proposed as a deliverable."* Every number above is measured reachability of an
   unbuilt tool. Nothing has been generated and nothing has been run.
2. **Hand-written bindings are better bindings, and that is why they are hand
   written.** PyO3's annotation buys a Python-idiomatic API — `__iter__`,
   operator overloads, docstrings, exceptions mapped to the right Python class.
   A generated Science binding to `regex` gives you `regex`'s API shape, not a
   Science-idiomatic one. **Science would be ahead on coverage per unit of human
   effort and behind on the quality of any individual binding**, and for the two
   dozen libraries a user actually touches, per-binding quality is what matters —
   which is why §6's seed set is curated by hand anyway.
3. **Nightly.** `rust-binding-generation.md` Decision 15 confines it to
   contributors and checks every artefact in, which is right. The residual is
   that a `format_version` bump costs a day and regenerates every curated
   binding, on rustc's schedule and nobody else's.
4. **The room is smaller.** This is the sharpest caveat. Winning at Rust bindings
   wins `polars`, `tokenizers`, `safetensors`, `image`, `arrow-rs`, `ndarray`,
   `candle`, `regex`, `serde_json`, `zstd`. That is a real and growing set and
   **it contains no equivalent of `scipy`, `scikit-learn`, `astropy`,
   `statsmodels`, `matplotlib` or `sympy`.** Being ahead at binding Rust is being
   ahead in a smaller room.

**How fast.** F1. `rust-binding-generation.md` §4 specifies the tool; §6 of this
note prices it at ~5 person-months.

**What it forecloses.** `rust-interop.md` Decision 16 — `async` crates are out,
permanently in the first version, which removes most of modern Rust networking
and database access. And `rust-binding-generation.md` §5.7 — a panicking rayon
worker aborts the process, with no remedy under `panic = "abort"`.

### 2.3 Route 3 — direct C binding

`sciencec bindgen` over headers, per `ffi-c-boundary.md` §7 and
`c-binding-coverage.md`.

**What it delivers.** Comparable to what `ctypes` and `cffi` give Python, and
better on two axes and worse on one. Better: the signature is written down once
where a reviewer can find it rather than at every call site, and a misspelled
symbol is a link error rather than a segfault. Worse, and
`ffi-c-boundary.md` §7.4 says so without flinching: *"Julia users call a C library
by typing one line, and Science users generate a binding first."*

**What it costs.** §4 of this note is entirely about this. The short form:
`c-binding-coverage.md` §5.3's finding that a 99%-reachable library is 99%
callable inside `unsafe` and 0% callable from safe Science, and that the observed
wrapper ratio for cuDNN is 60 wrappers to 1,500 declarations. **4%.**

Python users reach for `ctypes` rarely and grudgingly, and for exactly the same
reason. The safe wrapper is the work; in Python it is called "writing a C
extension" and everybody who has done it says the same thing about it.

**How fast.** F0 for the mechanism; the inventory is §6 and §7.

**What it forecloses.** `c-binding-coverage.md` Decision 6 — Eigen, OpenCV,
TensorRT, libtorch's C++ API and anything header-only are out permanently, and
the honest framing there is the right one: *"Science's C++ story is 'use a
library that publishes a C ABI', which is a real answer and a narrow one."*

### 2.4 The ranking, and why it is not the allocation

| | Route | Inventory per person-month | Time to first useful | Needed because there is no alternative |
|---|---|---|---|---|
| 1 | Python's bindings | **Unbounded** — the unit of work is nothing | F2 | `scikit-learn`, `astropy`, `matplotlib`, `sympy`, `statsmodels`, `transformers` |
| 2 | rustdoc generation | **High** — 15 policy entries for a crate | F1, after ~5 pm of tooling | `polars`, `tokenizers`, `image`, `arrow-rs` |
| 3 | direct C binding | **Low** — the 4% | F0 mechanism, F1 inventory | **BLAS, LAPACK, CUDA, ONNX Runtime, HDF5, MPI** |

> **Decision 1. The three routes are ranked 1 > 2 > 3 by inventory delivered per
> person-month, and funded in the order 3 > 2 > 1 by necessity. Effort goes to
> route 3 only where no other route reaches the library; to route 2 wherever a
> Rust crate exists; and route 1 is the standing fallback that makes the gap
> survivable, not a strategy.**
>
> **Rejected: fund by yield, i.e. build route 1 first and most.** It is the
> highest-yield route and it produces a language whose answer to "what can it do"
> is "whatever Python can do, if Python is installed". That is
> `python-from-science.md` §5.3's proportionality problem taken to its limit: a
> Science program made mostly of `python err:` regions is a Python program with
> extra steps, and `python-from-science.md` §11 is right that the default —
> *"no `[python]` table, `SC0455`, a binary that embeds nothing"* — has to stay
> the default forever.
>
> **Rejected: fund by necessity alone, i.e. route 3 only.** This is the trap the
> 4% describes. A language whose only route is the one with the worst yield does
> not have an inventory in ten years, and §4 exists because that outcome has to
> be argued against rather than hoped away.
>
> **Cost.** The project maintains three binding mechanisms rather than one. That
> is three tools, three failure modes, three sets of diagnostics and three
> maintenance surfaces, and §3.5 prices what it does to a *user* rather than to
> the project, which is the part that is usually left out.

---

## 3. Routes 1 and 2 combined: a routing rule, not a menu

The observation that makes this more than "use both": a large and growing share
of the scientific Python ecosystem is **a thin Python wrapper over a Rust crate
or a C library**. If that is true, then routes 1 and 2 frequently point at the
same artefact, and Science does not need the wrapper — it can bind what is
underneath.

The observation is true. The rule that falls out of it is not the obvious one.

### 3.1 The question the rule actually turns on

The obvious rule sorts by implementation language: Rust underneath → route 2,
C underneath → route 3, Python all the way down → route 1. That rule gets three
important libraries wrong, and the reason it gets them wrong identifies the
question that matters.

> **The load-bearing question is not *what language is the compiled core written
> in*. It is: *does the compiled core exist as a library you can link without
> CPython?***

Some compiled cores are ordinary libraries that happen to have a Python wrapper.
The `polars` crate is a crate; `py-polars` is a PyO3 layer on top. Remove the
layer and the crate is still there.

Some compiled cores **are** the Python extension. There is no `libnumpy`. NumPy's
compiled code is written against the CPython C API — it allocates `PyObject`s,
raises Python exceptions, and participates in the reference count. Linking it
without an interpreter is not hard, it is meaningless. The same is true of
scikit-learn's Cython kernels and of most of scipy's Cython layer.

That distinction is what decides the route, and it does not line up with the
implementation language at all.

### 3.2 The ecosystem, classified

Recollected, not surveyed — see §1. The final column is the finding.

| Package | Compiled core | Separable from CPython? | Route |
|---|---|---|---|
| `polars` | the `polars` Rust crate, PyO3 wrapper | **Yes** | **2**, plus Arrow for the data |
| `tokenizers` | the HF `tokenizers` Rust crate, PyO3 | **Yes** | **2** |
| `safetensors` | the HF `safetensors` Rust crate, PyO3 | **Yes** — and the format is 200 lines | **write** (`models-and-inference.md` §5.1) |
| `pydantic-core` | Rust, PyO3 | Yes | 2, and not ML-relevant |
| `pyarrow` | Arrow C++ | **Yes**, via the C Data Interface | **bind the format** |
| `duckdb` | C++ with a published `duckdb.h` | **Yes** | **3** + Arrow |
| `xgboost`, `lightgbm` | C++ with a published `c_api.h` | **Yes** | **3**, or the artefact route (§3.4) |
| `h5py` | libhdf5, Cython wrapper | **Yes** | **3** — deferred by `data-io.md` §3 |
| `onnxruntime` | ONNX Runtime, stable `OrtApi` | **Yes** | **3** — `models-and-inference.md` Rank 1 |
| **`numpy`** | C, written against the CPython C API | **No. There is no `libnumpy`.** | **1**, or reimplement |
| **`scipy`** | Cython + Fortran + C++ | **Partly** — the Fortran cores are; the Cython is not | **3** for the cores, **write** the rest |
| **`torch`** | libtorch (C++), no stable C ABI | **Technically yes, practically no** | **1**, or ONNX export |
| `scikit-learn` | Cython + C++ kernels, CPython-bound | **No** for the estimator API | **1**, or ONNX export |
| `astropy` | mostly Python; vendored `wcslib`, `cfitsio`, `erfa` | The C libraries are; the object model is not | **1** |
| `matplotlib` | Agg (C++), not a usable standalone API | **No** | **1** |
| `statsmodels` | Python + Cython over numpy/scipy | **No** | **1** |
| `sympy` | pure Python | n/a | **1** |
| `transformers` | pure Python over torch | n/a | **1** |

### 3.3 Three reclassifications, each argued from a sibling note

**NumPy is not a native target, and putting it in the native column is the most
consequential error the obvious rule makes.** NumPy's compiled core is a CPython
extension, not a library. What is underneath NumPy that Science *can* reach is
BLAS and LAPACK, which route 3 binds directly and `scientific-libraries.md` §2
calls "not negotiable" — plus NumPy's array semantics, which Science does not
bind but *reimplements*, as `Tensor`, in `broadcasting.md` and
`indexing-and-array-literals.md`. **So the answer to "how does Science get
NumPy" is: it already decided not to, three notes ago, and built the thing
instead.** What survives is the interchange, and the interchange is DLPack
(`python-interop.md` §4.1), which is free.

**Torch is not a native target either, and two sibling notes say so
independently.** `models-and-inference.md` Rank 4: *"There is no stable C ABI.
The public interface is C++ with templates and `at::Tensor` by value"*, priced at
~12 engineer-weeks plus permanent upkeep over roughly two thousand operators, and
deferred past F3. `c-binding-coverage.md` Decision 6 forbids it structurally:
Science binds C++ only through a C ABI the library publishes and versions, and
libtorch does not publish one. **Torch's route is the interpreter, or ONNX
export, and those are the only two.**

**scikit-learn is the interesting one, and its complication is a strengthening
rather than a weakening of route 1.** sklearn's hot paths are compiled — libsvm,
liblinear, coordinate descent, tree building, k-means — so a Science program that
calls sklearn through the interpreter is paying interpreter overhead **per call,
not per element.** One `PyObject_Call` into `fit` on a million-row matrix is one
crossing. That is the case where route 1 is not merely adequate but genuinely
cheap, and it generalises:

> **Route 1's overhead is proportional to the number of boundary crossings, not
> to the amount of work. It is cheapest exactly where the Python package is a
> thin wrapper over compiled code — which is where the native route looked most
> attractive. The two considerations point the same way and they should not.**

The counterweight, and it is the one that decides sklearn against route 1 anyway:
`python-from-science.md` §6's first ceiling clause is *Python introspecting a type
Science defined*, and its worked example is `sklearn.base.clone` calling
`inspect.signature` on a Science-defined estimator. So sklearn works from Science
right up to the moment you write your own estimator, which is the moment a
scientist reaches for sklearn in the first place.

### 3.4 The fourth route the rule was missing: bind the artefact

The classification above reveals a row that is not a binding at all, and it is
the highest-value row for machine learning.

`skl2onnx`, `onnxmltools` and `torch.onnx.export` turn a *fitted model* into a
file in a standard format. Science binds the format's runtime — ONNX Runtime,
~40 functions behind a stable `OrtApi` struct-of-function-pointers — and reaches
the model **without binding sklearn or torch at all, and without an interpreter
at run time.** The Python was needed once, at export, on somebody's laptop.

This is `models-and-inference.md` Rank 1's entire argument, and it is
`rust-binding-generation.md` Decision 10's rule (*bind the interchange format,
not the crate*) applied one level up — to an artefact rather than to a data
structure. The family is already large and every member of it was chosen
independently by a sibling note:

| Artefact | Format | Bound by |
|---|---|---|
| a fitted model | ONNX | `models-and-inference.md` Rank 1 |
| weights | safetensors | `models-and-inference.md` §5.1 — written, not bound |
| a quantised LLM | GGUF | `models-and-inference.md` Rank 2 |
| a table | Arrow C Data / Parquet | `data-io.md` §9 |
| a tensor, in memory | DLPack | `python-interop.md` §4.1, core spec §8 |
| a tokenizer | `tokenizer.json` | `models-and-inference.md` §6.1 |

> **Decision 2. The routing rule has four rows, and the order is the order of
> preference, not a classification:**
>
> | If the thing you want | Route | What you get |
> |---|---|---|
> | is reachable as an **artefact in a published format** | bind the format's runtime | native, no interpreter, and the Python ran once at export time |
> | is a **Rust crate** with a Python wrapper on top | `sciencec foreign bind` | native, statically typed, no Rust written |
> | is a **C library** with a Python wrapper on top | `extern` + the interchange format | native, plus zero-copy for the data |
> | **is** the Python extension, or is written in Python | the interpreter | dynamic, available immediately, not self-contained |
>
> **Rejected: sort by implementation language.** It puts NumPy, torch and half of
> scipy on the wrong side of the line, for the reason §3.1 gives: the compiled
> core of a CPython extension is not a library.
>
> **Rejected: a single route per ecosystem** — "native for Rust, interpreter for
> Python". It cannot express the row that matters most, because ONNX Runtime is a
> C library and the model inside it came from PyTorch.
>
> **Cost.** Four routes is more than three, the artefact row depends on the
> upstream ecosystem continuing to publish exporters, and
> `models-and-inference.md` Rank 1 already names the cost honestly: *"When a
> user's model does not export, Science's answer is 'it does not export', and
> that is not an answer they will like."*

### 3.5 Migration per library, and why nothing should hide the seam

`python-interop.md` §1's adoption thesis — *one hot function at a time, from
inside the Python program they already have* — applies to the bindings
themselves. A user starts on route 1 for a library, because route 1 is available
with no work by anybody, and moves to the native route when that library becomes
a bottleneck or when the binding lands. **No library is ever blocked waiting for
a native binding**, which is the property that makes the inventory gap survivable
rather than fatal.

The obvious next move is a facade: make `use python "polars" as pl` and
`use polars` present the same surface, so the migration is a one-line edit.

**Do not.** Three things differ across that seam and the third is decisive.

1. **The error model differs.** Route 1 is `python-from-science.md`'s
   `python err:` region, which is a deliberate and bounded retreat from
   `syntax-revision-2.md` §3.2's write-the-error-where-it-happens rule. The
   native routes are `-> (T, Error?)` per call. A facade would have to pick one
   and lie about the other.
2. **The type differs.** Route 1 gives `PyObject`, about which the compiler can
   check "nothing, ever". The native routes give a type. Hiding that hides the
   entire reason to use the native route.
3. **The deployment property differs, and this is the one that must never be
   hidden.** `native-dependencies.md` Decision 12 defines self-contained as
   naming the interpreter and `site-packages` explicitly;
   `python-from-science.md` Decision 15 puts a `self_contained` boolean in the
   provenance record. A facade would make it impossible to tell, **from the
   source**, whether the program needs Python installed. That fact shows up on a
   cluster, in a batch job, two days after submission. `python-from-science.md`
   §4.5 has the sentence: *"'Science needs Python installed' is a sentence that,
   said once in the wrong place, is very hard to unsay."*

> **Decision 3. There is no facade. Migrating a library from the interpreter
> route to a native route is a visible, mechanical source edit, and the toolchain
> helps by *naming* it rather than by hiding it.**
>
> Two mechanisms, both cheap and neither of which changes what compiles:
>
> - **A migration note.** When a program contains `use python "<name>"` and a
>   native binding for `<name>` is resolvable in the graph or present in the
>   configured registry, the compiler emits a warning-level note naming the
>   native package and the one thing that changes: the binary becomes
>   self-contained. This is `llm-ergonomics.md`'s thesis — diagnostics as the
>   teaching channel — applied to a routing decision. §9 asks
>   `package-manager.md` for the code rather than claiming one.
> - **A `doctor` line.** `sciencec doctor` lists every `use python` import in the
>   project alongside whether a native binding exists, which is the same fact
>   presented where a user goes looking for it rather than where the compiler
>   happens to notice.
>
> **Rejected: a compatibility shim module per library**, so that `use polars`
> works whether or not the native binding is installed and silently falls back to
> the interpreter. This is `native-dependencies.md` Decision 4's rejected case
> exactly — a silent provider fall-through — and that note's reasoning transfers
> without modification: a fall-through that is good enough to be tolerable is a
> fall-through users run on without noticing, and then the first public benchmark
> of Science is a benchmark of the fallback. Here it would be worse than a
> benchmark: the fall-through would silently make a binary not self-contained.
>
> **Cost, stated plainly, because this is the cost the user pays rather than the
> project.** A Science program that uses both routes has two error models in one
> file, two type disciplines, and one deployment property determined by whichever
> route is present. A user has to know which one they are on. The mitigation is
> that **being on route 1 is always visibly spelled `use python`**, which is one
> line at the top of a file and is the same line the provenance record reports.
> That is not nothing to learn, and it is much less than the alternative, which
> is learning it from a failed batch job.

---

## 4. The 4%, and whether the wrapper discipline is right

This is the section the note exists for.

### 4.1 What the 4% is, and what it is not

`c-binding-coverage.md` §5.3:

> *A 99% reachable library is 99% callable from inside an `unsafe` block and 0%
> callable from safe Science. [...] for cuDNN, 1,500 generated declarations
> support about 60 wrappers. **4%.***

Two corrections before the decision, because the number is doing two different
jobs and only one of them is honest.

**Correction one: 4% is a ratio of wrappers to declarations, not of demand met to
demand.** `ffi-c-boundary.md` §7.1's own argument is that *a program does not
call 1,500 cuDNN functions*. Demand over a C library's symbol set is extremely
skewed, and `rust-binding-generation.md` §2.3 measured the same skew from the
other side: five policy entries cover 80% of polars' demand and one covers 633
functions. LAPACK has 1,700 routines and essentially every user calls `gesv`,
`gels`, `getrf`, `potrf`, `syev`, `gesdd` — thirty routines cover most of the
field. Against *demand*, 60 well-chosen wrappers may be 95% and not 4%.

**Correction two: cuDNN is the worst possible library to measure this against,
and Science has already decided it does not need it.** cuDNN is only required if
Science is implementing a training framework. `models-and-inference.md` §7:
*"Science should not attempt training in its first versions. Not in F1, not in
F2, and the documentation should say so on the front page rather than in a
footnote."* So the 4% is measured against a library that is a dependency of a
project this project has refused. §5 works the real ML list and cuDNN is not on
it.

**What survives both corrections, and it is enough.** The skew is *per library*,
and Science needs *many* libraries. Sixty wrappers × two hundred libraries is
twelve thousand wrappers. And the tail matters more for this audience than for
most, because a scientist reaches for the obscure LAPACK driver precisely because
they are a scientist. **The 4% overstates the gap for one well-chosen library and
understates it across the long tail of libraries, and both errors are real.**

### 4.2 Is the discipline right? Yes, and it is not the thing that needs fixing

`ffi-c-boundary.md` §0 asks for a boundary that is *"narrow, explicit, and
checkable on the Science side"*, and §7.1's conservatism — every pointer
`ffi.Pointer`, every call `unsafe`, no `borrowed`, no `Drop`, no `from` — follows
from three reasons, of which the third is decisive: **a C header does not contain
the ownership contract.** `c-binding-coverage.md` Decision 4 then closes the
obvious escape: mapping `const T*` to `borrowed` automatically is unsound in both
directions, and in one of them it emits an LLVM `noalias` claim on the strength
of a keyword the callee may cast away.

Both are right and this note does not reopen either. The four options the brief
names, weighed:

**(a) Generate `unsafe` bindings wholesale and let safe wrappers accrete.** This
is not an alternative — **it is already what happens.** §7.1 already generates
wholesale conservative `unsafe` declarations. The problem is not that the unsafe
layer is missing. The problem is that **there is no sanctioned, distributable,
documented way to *use* it, and no gradient from "I called it in `unsafe`" to
"there is a safe wrapper".** A user who needs cuDNN function 61 is, under the
notes as they stand, blocked — the binding package has 60 wrappers and no
articulated way to reach past them.

**(b) A marked "thin" mode** that generates `borrowed`/`Span` heuristically.
Rejected, on `c-binding-coverage.md` Decision 4's argument, which this note will
not contradict and would strengthen: the failure mode is not a crash, it is a
wrong answer at scale, which is the exact class `ffi-c-boundary.md` §1.6 spends a
section preventing.

**(c) Tiered trust** — a `reviewed` / `audited` marker per wrapper. Rejected as a
*type-system* mechanism: it grades "has a human argued this is sound", which is
already exactly what `unsafe` marks, and it adds a taxonomy without adding a
check. A weaker form survives and is adopted below, as a sidecar field, which is
documentation and costs nothing.

**(d) Accept the cost and curate.** Adopted as *part* of the answer — §6 is the
curated set — and rejected as the *whole* answer, because it caps the inventory
at whatever the core team writes, which is the thing that cannot scale.

> **Decision 4. A binding package ships in two modules: `<name>.raw`, the
> generated conservative `unsafe extern` declarations, complete and mechanical;
> and `<name>`, the hand-written or sidecar-generated safe layer. The safe layer
> is never required to be complete, its incompleteness is not a defect, and
> reaching into `.raw` is a documented and normal act rather than a failure.**
>
> The precedent is Rust's `*-sys` convention, which is the only case in which a
> language with an ownership model and no bindings acquired an inventory. In 2015
> Rust had zero; by 2020 it had `libc`, `openssl`, `hdf5`, `netcdf`, `blas-src`,
> `arrow`, `cuda`. The mechanism was exactly this: `bindgen` generates the `-sys`
> crate wholesale and publishes it; someone else publishes a safe crate on top;
> the safe crate covering 5% of the `-sys` crate is fine, because the `-sys`
> crate is still there for the other 95%. `native-dependencies.md` §2.6 reads
> cargo the same way and lists *"generated unsafe declarations in one unit,
> hand-written safe wrapper in another"* under **"Copy, and take almost
> entirely"**, noting that `ffi-c-boundary.md` §7.1 reached it independently.
>
> **The split is two modules in one package, not two packages, and that is a
> deliberate departure from Rust.** `package-manager.md` Decision 16 makes
> package names `[a-z][a-z0-9]*` — no hyphens, no underscores — so `openblas-sys`
> and `openblas_sys` are both illegal names, and §4.4 of that note explains why:
> the package name *is* the root module name. More substantially, Rust needs a
> separate crate because cargo's `links` key is per-crate;
> `package-manager.md` Decision 11a gives Science `capability` and `variant` as
> *resolver inputs* on `[native.<name>]`, which does that job without a second
> package. So one package, two modules, versioned together — which for a layer
> generated from one header set is the correct coupling anyway.
>
> A third party who wants to publish a better safe layer over the same raw layer
> depends on the package and uses only its `.raw` module. That works, and it is
> the escape hatch that keeps the curated set from being a monopoly.
>
> **Rejected: keep `.raw` private to the package.** It is the tidy choice and it
> recreates the block. A user who needs function 61 would have to fork the
> binding, at which point the package's whole value — that it is maintained and
> regenerated against version bumps — is gone.
>
> **Cost, and it is the real one: this makes `unsafe` ordinary.** If reaching for
> `.raw` becomes the normal thing, Science's verification thesis is hollowed out
> from the inside, one convenience at a time. Rust has exactly this problem and
> lives with it; `unsafe` in downstream crates is a known and tolerated feature of
> that ecosystem rather than a scandal. Three mitigations, none of which is a
> guarantee: `bindgen` reports the wrapped-to-declared ratio per package so it is
> a visible number; `bindgen --check` names calls into `.raw` from a package that
> advertises a safe layer; and the documentation frames `.raw` as an escape hatch
> in `native-dependencies.md` §4.4's four words — **honest, not safe**.

### 4.3 The thing that actually moves the number: generate the wrapper from the sidecar

`c-binding-coverage.md` §5.3 states the strategic consequence and then nobody
acts on it:

> *effort spent raising the 65% toward 99% is worth much less than effort spent
> lowering the cost of writing a safe wrapper.*

Nobody has priced lowering that cost. Here is the mechanism, and it requires no
new concept — only the use of one that already exists.

**`ffi-c-boundary.md` §7.2 already specifies a sidecar** keyed by function name,
recording *"the things headers do not carry"*, and gives three examples:
*"parameter 3 is a span of length parameter 5", "the return borrows parameter 1",
"this handle's destructor is `H5Fclose`"*. `c-binding-coverage.md` §6.3 adds
three more fields — the macro class, the layout warrant, the const proposal.

**Those six facts are exactly and precisely what a safe wrapper needs.** The
sidecar already holds the contract. The generator currently records it and then
throws it away at the safe boundary, emits `ffi.Pointer` for everything, and
leaves a human to retype the same facts as forty lines of Science.

Meanwhile `rust-binding-generation.md` built a generator that *consumes* a
declared policy file and emits the safe layer from it — and reports 100% of
`regex` from 15 entries. **The asymmetry between the two notes is not that Rust
carries more information. It is that one note's generator consumes the
declaration and the other's does not.**

> **Decision 5. `sciencec bindgen` emits the safe wrapper from the sidecar, not
> only the `unsafe` declarations from the header. The sidecar becomes a
> wrapper-generation input with a declared schema, in exact analogy to
> `rust-binding-generation.md` §4.2's policy file.**
>
> **This narrows `c-binding-coverage.md` Decision 4 rather than contradicting
> it, and the narrowing should be stated so nobody reads it as a reversal.**
> Decision 4 says the generator emits `ffi.Pointer` regardless of `const` and
> that *"promotion is always a human edit"*. Both halves survive. The human still
> makes the judgement and it is still unverifiable. What changes is **where the
> edit lands**: in a reviewable sidecar entry rather than in a hand-written
> `.science` file, and once per function rather than once per line of wrapper.
> The generated wrapper is exactly as unsound as a hand-written one, because both
> rest on the same human claim. What is bought is not soundness. It is that the
> claim is one line instead of forty, so a human will actually write 1,700 of
> them.
>
> **The parent note already predicted this in one case and did not generalise
> it.** `ffi-c-boundary.md` §7.3 on LAPACK: the Fortran sources carry
> `INTENT(IN)` / `INTENT(OUT)` / `INTENT(INOUT)` and machine-parseable dimension
> comments, *"which means the safe layer for LAPACK can be largely generated
> too."* That is Decision 5 for one library, with the sidecar pre-filled by
> somebody else in 1992. This note's move is to make the same machinery available
> when the sidecar is filled in by us.
>
> **What makes it tractable: five wrapper shapes, not free-form emission.** Every
> wrapper in all five audited libraries is one of:
>
> | Shape | Example | What the sidecar entry names |
> |---|---|---|
> | scalar-and-span call with a return-code error | all of BLAS, LAPACK, libsodium | which params are spans, their length params, the error convention |
> | handle with constructor and destructor | `H5Fopen`/`H5Fclose`, `cudnnCreate`/`cudnnDestroy` | the pair, and the handle type |
> | iteration through a callback and `op_data` | `H5Literate`, `inflateBack` | the callback param, the data param, the stop convention |
> | two-call length-then-fill | every "how big a buffer do I need" API | the two calls and the shared length |
> | accessor over an opaque handle | `H5Rget_type`, `Py_REFCNT` | the handle and the return |
>
> A sidecar entry naming a shape plus its parameters is one line. A wrapper
> emitted from it is twenty to forty.
>
> **The estimate, and §10 records that everything here rests on it.** A
> hand-written wrapper of the first shape is perhaps one to two hours including
> the test. A sidecar entry naming the shape is perhaps five to ten minutes of
> the same judgement. Order of magnitude: **twelve-fold**. Applied to the ratio:
> the same effort that produces 60 cuDNN wrappers produces something like 600 —
> **not 1,500, because the tail genuinely does need thought that no shape
> captures.** So the 4% becomes roughly 40% for a well-annotated library, and
> approaches 100% for LAPACK where the vendor wrote the annotations, and
> **never needs to be 100%, because `.raw` is there.**
>
> **Rejected: infer the shape from the signature.** A generator can guess that
> `(double *a, int n)` is a span and it will be right most of the time, and the
> times it is wrong produce `noalias` on an aliased pointer. `SC0495`'s existing
> discipline applies: refuse and name, never guess.
>
> **Cost.** `bindgen` grows a wrapper emitter and the sidecar grows a schema —
> priced at 3 person-months in §6. The emitted wrapper is only as good as the
> annotation, and a wrong annotation now produces a *safe-looking* wrapper, which
> is worse than a wrong `unsafe` declaration because it looks reviewed. The
> mitigation is `bindgen --check`, which `c-binding-coverage.md` §6.3 already
> makes the keystone of the whole design and which §10 of that note already flags
> as the thing most likely to be cut for schedule. **It must not be cut. Under
> Decision 5 it is load-bearing twice over.**

### 4.4 One sidecar field, which is the surviving half of tiered trust

> **Decision 6. The sidecar gains one field per entity: whether a safe wrapper
> exists, and who reviewed it. It is documentation, not a tier, it has no effect
> on what compiles, and `bindgen` aggregates it into the wrapped-to-declared
> ratio Decision 4 makes visible.**
>
> This is option (c) reduced to the part that costs nothing. It does not grade
> trust — `unsafe` already does that — it records coverage, which is the number
> this entire note is about and which no artefact currently reports.

---

## 5. What machine learning and deep learning actually need

The owner's stated focus. The list, checked against the four routes of Decision 2
and against what siblings already decided.

### 5.1 The list, in three tiers by what its absence blocks

**Tier 1 — without these, a Science program cannot participate in an ML workflow
at all.**

| Need | Route | Status |
|---|---|---|
| Tensor interchange | DLPack — a struct, not a library | `python-interop.md` §4.1; core spec §8. **Free** |
| Table interchange | Arrow C Data / Stream Interface | `data-io.md` §9; `rust-binding-generation.md` Decision 10 |
| Weights | safetensors — **written**, not bound | `models-and-inference.md` §5.1, ~3 days |
| Inference | ONNX Runtime, stable `OrtApi`, ~40 functions | `models-and-inference.md` Rank 1, ~5 weeks |
| Tokenization | HF `tokenizers` — **route 2** (see §5.2) | `models-and-inference.md` §6.1 |
| Image decode and resize | the Rust `image` crate — **route 2** (§5.2) | `models-and-inference.md` §6.2 |
| Columnar files | Parquet, via Arrow | `data-io.md` §3 rank 6 |

**Tier 2 — these make Science a faster participant.**

| Need | Route | Status |
|---|---|---|
| BLAS, LAPACK | route 3; safe layer generated from Fortran `INTENT` | `ffi-c-boundary.md` §1.6–1.7, §7.3; `scientific-libraries.md` §2 |
| CUDA runtime — alloc, memcpy, streams, device query | route 3, ~40 functions | **unowned by any note.** §9 asks |
| Dataframe operations at scale | polars, via Arrow + SQL | `rust-binding-generation.md` §5.8 |
| Local LLM inference | llama.cpp / GGUF | `models-and-inference.md` Rank 2, ~6 weeks + upkeep |
| Compression | miniz, libzstd | `stdlib-shape-and-packages.md` §5.1 |
| Gradient-boosted trees | XGBoost/LightGBM `c_api.h`, or ONNX | ONNX is cheaper — Rank 1 covers both |

**Tier 3 — these are only needed to *implement a training framework*, which
`models-and-inference.md` §7 has refused.**

cuDNN. cuBLAS beyond `gemm`. NCCL. LibTorch. TensorRT. MPI for data-parallel
training.

**cuDNN is a Tier 3 dependency of a project Science has decided not to do.** That
is the whole finding of §4.1's second correction, and it is why the 4% is a much
smaller practical problem than the number suggests.

### 5.2 Two routing corrections to `models-and-inference.md`

Both because that note was written before `rust-binding-generation.md` existed.

**Tokenizers.** §6.1 says: *"bind Hugging Face `tokenizers` through a C shim"*.
`tokenizers` is a Rust crate. It has rustdoc JSON. **Route 2 reaches it with no
Rust written and no C shim at all**, and the surface is small — `Tokenizer`,
`Encoding`, `encode`, `encode_batch`, `decode`, plus projections for `ids`,
`tokens`, `offsets` and `attention_mask`, which is a handful of policy entries by
`rust-binding-generation.md` §2's measure. The hand-written C shim is strictly
more work and strictly more maintenance for the same result.

That note's §6.1 is right about everything else, and the part it is most right
about is the part that does not change: *"The conformance suite is the
deliverable, not the binding."* The route changes; the token-for-token
conformance corpus against reference ids does not, and it is still the larger
half of the cost.

**Image codecs.** §6.2 proposes `libjpeg-turbo`, `libpng`, or single-header
`stb_image` with *"a documented quality caveat"*. The Rust `image` crate decodes
JPEG, PNG, WebP and TIFF in safe Rust, is reachable by route 2, and
`rust-binding-generation.md` §5.2 already names *"`image` through a raw buffer
descriptor"* as an instance of its bind-the-format rule. That removes three C
dependencies and the quality caveat at once. The resize-filter conformance
requirement of §6.2 — *"PIL's bilinear and OpenCV's bilinear are different
functions"* — is unaffected and is still where the work is.

### 5.3 The question that actually decides adoption

The brief asks: for a PyTorch user, is the thing that matters calling cuDNN from
Science, or being callable from Python with DLPack zero-copy?
`python-interop.md` §1 has a strong opinion. Tested:

**Neither, and both halves of the question are the wrong target.**

**cuDNN is the wrong target because nobody calls it.** A PyTorch user calls
`torch.nn.Conv2d`. cuDNN is three layers below anything they touch, and PyTorch's
own cuDNN wrapper is a piece of code nobody outside the PyTorch team reads.
Binding cuDNN from Science buys the ability to *reimplement PyTorch*, which is a
several-hundred-person-year project that `models-and-inference.md` §7 already
refused on the front page.

**DLPack is the wrong target because it is not an inventory, it is an
interface.** It is one struct, roughly 200 lines of handling, already specified
in two notes, and it costs nothing. Asking whether it matters more than cuDNN is
comparing a precondition to a route. It matters enormously and it is free, which
means it is not a strategic choice at all.

**What actually decides adoption is the part of the program that is not
PyTorch.** Data loading. Preprocessing. Tokenization. Augmentation. The eval
loop. Metric computation. Feature engineering. The simulator that generates the
training data. That is where Python is slow, where the GIL bites, where the bugs
are silent — `models-and-inference.md` §6's governing observation is
*"preprocessing failures are silent"* — and where the inventory Science needs is
**small, enumerable and mostly already decided**: it is Tier 1 of §5.1, and it is
about four person-months of work.

So the honest test of `python-interop.md` §1 is this. **The claim is half right
and the half it gets wrong is the strategic half.**

The half that is right: nobody adopts a scientific language whole, and the
first contact will be a Science function inside somebody's Python program. That
is true and §7's first milestone is built entirely on it.

The half that is wrong: that is a **beachhead, not a strategy**. Three arguments.

1. **The cited evidence does not support the causal claim.** §1 attributes
   Julia's stall to *"you had to leave Python to get anything back"*. Julia has
   had `PyCall` since 2013 and `PythonCall` since 2021; the ability to call
   Python did not save it. Julia's real problems were time-to-first-plot,
   invalidation, and a package ecosystem that was excellent in three domains and
   thin everywhere else. And where Julia's *native* inventory was best in the
   world — `DifferentialEquations.jl`, `JuMP` — adoption happened. That is
   evidence for inventory and against interop.
2. **Go is the counterexample.** Go had no incremental path from anything, a
   hostile FFI story, and was adopted anyway, on a batteries-included standard
   library and a single-binary deployment story. The two things Go had are the
   two things Science's sibling notes have already chosen —
   `stdlib-shape-and-packages.md` Decision 1 and `native-dependencies.md`
   Decision 12.
3. **The beachhead is contested ground where Science's advantages are
   invisible.** A "faster function inside your Python program" competes with
   Cython, numba, pybind11 and Rust+PyO3 — four incumbents with a decade of
   tooling — and competes on their turf, where units, typed shapes, region
   checking and self-contained deployment are all invisible because the
   surrounding program is still Python.

> **Decision 7. The adoption sequence is: be callable from Python with DLPack
> zero-copy *first*, because it is how the first user arrives; and have the Tier 1
> inventory *second*, because it is what the second program is written in. The
> first is a beachhead and the note says so; the second is the strategy.
> `python-interop.md` §1's sentence is narrowed accordingly, not overturned.**
>
> **Rejected: lead with the native inventory and treat Python interop as
> secondary.** It is what a language with a strong thesis wants to do and it
> loses the first user, who has a working Python program and no reason to port
> it. `python-interop.md` §1's measure of success — *"replace one function with a
> Science one in an afternoon"* — is the right measure for month three.
>
> **Rejected: treat the beachhead as sufficient.** It produces Cython with a
> better type system, which is a real thing to be and is not what any of these
> notes are designing.
>
> **Cost.** Saying "beachhead" out loud sets an expectation the project then has
> to meet. §7's 36-month falsification criterion exists to make that expectation
> checkable rather than rhetorical.

---

## 6. The seed inventory

If wrappers are the work and they do not generate, somebody writes the first
ones. This section says who, which, how chosen, and what it costs.

### 6.1 The criterion

A library enters the seed inventory if all four hold:

1. **Its absence blocks a whole audience.** `data-io.md` §3's ranking criterion —
   audience unblocked per unit of work — applied unchanged.
2. **It is reachable by one of Decision 2's four routes** with a stable ABI, an
   interchange format, rustdoc JSON, or a published artefact format.
3. **Nobody else will write it**, because it is infrastructure rather than a
   domain library. A domain library belongs in a package somebody else maintains.
4. **Its wrapper surface is bounded and knowable.** This is the criterion this
   note adds, and it is the one that does the work: **if you cannot say how many
   wrappers "done" is, it is not a seed library, it is a project.**

Criterion 4 excludes HDF5 by `data-io.md` §3's own words — *"a faithful binding
is not a binding, it is a second library"* — and excludes cuDNN, LibTorch and
TensorRT for the same reason at larger scale.

### 6.2 The set, and the price

Estimates attributed. Where a sibling gave engineer-weeks, they are converted at
four weeks to the month and marked; the rest are this note's and are marked so.

| # | Package | Route | pm | Source |
|---|---|---|---|---|
| 1 | `blas` + `lapack` (raw + safe) | 3, safe layer generated from Fortran `INTENT` | 3.0 | this note; mechanism from `ffi-c-boundary.md` §7.3 |
| 2 | `data.arrow`, `data.parquet` | bind the format | 0.5 + already-priced 0.5 | `data-io.md` §3 (~2 weeks) |
| 3 | `safetensors` | written in Science | 0.2 | `models-and-inference.md` §5.1 (~3 days) |
| 4 | `inference` (ONNX Runtime) | 3, `OrtApi` | 1.5 | `models-and-inference.md` Rank 1 (5–6 weeks) |
| 5 | `tokenizers` + conformance corpus | **2** | 1.0 + 0.5 | this note; corpus per `models-and-inference.md` §6.1 |
| 6 | `image` (decode, resize, presets) | **2** | 1.0 | this note; presets per `models-and-inference.md` §6.2 |
| 7 | `compress` (miniz, libzstd) | 3, mostly vendored | 0.5 | `stdlib-shape-and-packages.md` §5.1 |
| 8 | `regex` | 2, 15 policy entries | 0.3 | `rust-binding-generation.md` §2.2 |
| 9 | `cuda` (alloc, memcpy, streams, device) | 3, ~40 functions | 1.5 | this note |
| 10 | `llama` (llama.cpp / GGUF) | 3, churns | 1.5 | `models-and-inference.md` Rank 2 (~6 weeks) |
| 11 | `polars` (Arrow + SQL) | 2 + bind the format | 0.5 | `rust-binding-generation.md` §5.8 (~6 shim functions) |
| 12 | `sodium` | 3 | 0.5 | this note; `libsodium` is 99% bucket A |
| | **Seed inventory subtotal** | | **~13** | |

And the tooling without which none of it repeats:

| | Tool | pm | Source |
|---|---|---|---|
| T1 | `sciencec bindgen` — M1–M5 classification, `extern static`, sidecar, `--check` | 4.0 | `c-binding-coverage.md` §4 |
| T2 | **the sidecar-driven safe-wrapper emitter** (Decision 5) | 3.0 | this note |
| T3 | `sciencec foreign bind` — rustdoc JSON, policy file, shim, safe layer | 5.0 | `rust-binding-generation.md` §4 |
| | **Tooling subtotal** | **12** | |

> **Total to a seed inventory plus the machinery to grow it: ~25 person-months,
> or a little over two person-years.**

**And then the number that should be quoted more than the first one.** A bound
library costs roughly 10% of its build cost per year in upkeep — version bumps,
`--check` diffs, ABI-variant surprises, a `format_version` bump. llama.cpp is
worse and `models-and-inference.md` says so: *"budgeting recurring maintenance
forever"*, call it 50%. That is **~2 person-months per year standing, from the
day the seed inventory lands, rising linearly with the inventory.**

> **Decision 8. The seed inventory is staffed at 2 FTE for the first 18 months
> and 1 FTE steady-state thereafter, and if that cannot be committed the
> inventory is not attempted and the plan is route 1 plus route 2's generator
> alone.** A rotting binding is worse than no binding, because it produces a
> correct-looking answer against an ABI that moved.
>
> **Rejected: build the inventory with contributors.** There are none, and
> `package-manager.md` §7.3 is right that there is no point hosting a registry
> *"before ... at least one third party is asking to publish"*. Contributors are
> the 36-month goal, not the 6-month resource.
>
> **Rejected: build tooling first and inventory later, or inventory first and
> tooling later.** Tooling-first means two person-years before anyone can do
> anything, which is how a compiler project dies. Inventory-first means twelve
> hand-written bindings that must all be rewritten when the tool lands. §7 stages
> them together: one binding by hand per tool built, and the binding is what
> specifies the tool.

### 6.3 Where it ships, and the constraint that decides it

`package-manager.md` Decision 6's cost paragraph is blunt:

> *A Science package published to the registry that carries a sidecar is a
> package whose installation runs `build.rs`. §5.3 handles this by refusing
> sidecars in published packages until F2's review process exists, **which makes
> the curated shims of `rust-interop.md` §6 a *toolchain* artefact for longer
> than §1.2 item 6 hoped.***

> **Decision 9. Through F0 and F1, every route-2 binding in the seed inventory —
> items 5, 6, 8, 11 — ships inside the toolchain tarball, as Level 2 in
> `stdlib-shape-and-packages.md` §1.1's sense. Route-3 bindings ship as Level 3
> packages in a directory registry from F1. At F2, when sidecars are permitted in
> published packages under review, the route-2 bindings move to Level 3 **and
> their module paths do not change**, exactly as `data-io.md` §9 arranges for
> `data.parquet`.**
>
> **Rejected: put the whole seed inventory in Level 2 permanently.** This is the
> ratchet `package-manager.md` §1.3 names: *"Level 2 is versioned with the
> toolchain **forever**: moving a module out of Level 2 is a breaking change to
> every program that `use`s it, so the ratchet turns one way."* Twelve bindings
> in Level 2 is twelve permanent toolchain-versioned commitments, and it is the
> single easiest irreversible mistake available here.
>
> **Rejected: wait for F2 and ship nothing.** The Rust half of the inventory would
> be unavailable for the whole of F1, which is where §7's most important
> milestone lives.
>
> **Cost.** The two-tier world `package-manager.md` §11 already records as a wart
> persists through F1 and now has more inhabitants. And the F2 move is a real
> migration with real risk, mitigated only by the module path being stable.

### 6.4 What Decision 2's routing does to the count, and how that reconciles with §4

This is the reconciliation the seed inventory and the 4% problem require.

Under the routing rule, the hand-written-safe-wrapper work in the entire seed
inventory is:

| Package | Hand-written wrappers |
|---|---|
| BLAS | ~20 |
| LAPACK | **~1,700, generated from Fortran `INTENT` under Decision 5** |
| ONNX Runtime | ~40 |
| CUDA runtime | ~40 |
| compression | ~15 |
| libsodium | ~30 |
| llama.cpp | ~25, deliberately narrow |
| **everything else** | **zero — route 2, or bind the format, or written in Science** |

**That is roughly 170 hand-written wrappers plus one generated family.** A few
person-months, not a few person-years.

> **So the 4% is not the binding constraint on the seed inventory, because the
> seed inventory was chosen — by criterion 4 — from libraries where it is not.
> It is the binding constraint on the long tail.**

Which sharpens §4's recommendation rather than weakening it. Decision 4's raw
module split is **not** there to make the seed inventory possible; the seed
inventory does not need it. It is there to make the long tail survivable, which
is a smaller claim and an honest one. And Decision 5's wrapper emitter earns most
of its keep on one library — LAPACK, where the annotations already exist — which
is an unusually good position to be in for a three-person-month tool.

---

## 7. The staged plan

Each stage names something that can be demonstrated, not something that can be
claimed.

### 7.1 Three months — M1: the skeptical PyTorch user's afternoon

A Science function, compiled as an abi3 wheel, imported into an existing PyTorch
training script, receiving a `torch.Tensor` through DLPack with no copy, doing
something the user's Python loop was doing slowly, returning a tensor, measurably
faster.

Requires: `python-interop.md` §3.2's extension build, §4's DLPack in both
directions, §5's GIL policy. **It requires no binding inventory at all**, which is
the point — nothing in this note is on M1's critical path.

Demonstrable as: `pip install`, run the user's own script, print a before-and-after
wall clock. A skeptic can run it on their own model on their own machine.

```science
@export
function normalize_batch(
        batch: mutable borrowed Tensor of (F32, (Dyn of "b", 3, 224, 224)),
        mean: borrowed Array of F32,
        std: borrowed Array of F32) -> Error?:
    if mean.length() is not 3 or std.length() is not 3:
        return ShapeError.Channels(mean.length())
    for image in batch.rows():
        image.subtract_channelwise(mean)
        image.divide_channelwise(std)
    return null
```

### 7.2 Six months — M2: the raw layer is distributable

`sciencec bindgen` ships with M1–M5 classification, `extern static`
(`c-binding-coverage.md` Decision 7), the sidecar and `--check`. `blas` and
`lapack` publish with complete `.raw` modules and a small safe layer.

Demonstrable as: **a Science program that calls a LAPACK routine nobody wrapped**,
in six lines, today.

```science
use lapack.raw (dsyevr)

function eigenvalues_in_range(
        matrix: mutable borrowed Array of F64, n: Int,
        low: F64, high: F64) -> (Array of F64, Error?):
    let mutable values be Array of F64 .zeroed(n)
    let mutable found be 0
    let status be unsafe:
        dsyevr(...)                       # every argument an ffi.Pointer
    if status is not 0:
        return (values, LapackError.Info(status))
    return (values.slice(0, found), null)
```

That program is ugly, it is inside `unsafe`, and **it runs**. Under the notes as
they stood before Decision 4 it could not be written at all.

### 7.3 Twelve months — M3: the safe layer generates, and Science is better at something

Decision 5's emitter ships. LAPACK's safe layer is generated from annotations
somebody else wrote in 1992. ONNX Runtime, safetensors, `tokenizers` and `image`
land.

Two demonstrations, and the second is the one that matters.

**First**: a model, end to end, in one self-contained binary with no Python at
run time.

```science
use inference (Session)
use tokenizers (Tokenizer)

function main():
    let session, err be Session.open("sentiment.onnx")
    if err?:
        print(f"could not open the model: {err.message()}")
        return

    let tok, tok_err be Tokenizer.open("tokenizer.json")
    if tok_err?:
        print(f"could not open the tokenizer: {tok_err.message()}")
        return

    let encoded be tok.encode_batch(["the film was wonderful", "I want my money back"])
    let scores, run_err be session.run(ids: encoded.ids, mask: encoded.mask)
    if run_err?:
        print(f"inference failed: {run_err.message()}")
        return

    for row in scores.rows():
        print(row.argmax())
```

**Second**: a dimension mismatch in `linalg` that is a **compile** error. That is
the first milestone at which Science is *better* rather than merely present, and
it is a thing NumPy structurally cannot do.

### 7.4 Thirty-six months — M4: the inventory grows without the core team

Not a demo. A metric, declared in advance so it cannot be moved afterwards:

> **The number of published binding packages whose maintainer is not the language
> team.**

Supporting conditions: `sciencec foreign bind` stable; a registry per
`package-manager.md` §7.3; `bindgen --check` has survived two upstream major
versions of something real.

> **Decision 10. If the count of third-party-maintained binding packages is zero
> at 36 months, the inventory strategy has failed, and the note says so now
> rather than after. The fallback is explicit: route 1 becomes the primary
> answer, the `[python]` table stops being exceptional, and Science's public
> claim narrows to "a fast, checked language for the hot part of your Python
> program" — which is a smaller and still honest thing to be.**
>
> Declaring the failure condition in advance is the only mechanism available here.
> There is no test that fails.

---

## 8. The honest ceiling, and what "like Python does it" can mean

### 8.1 Four claims, four answers

"Every library written in C or Rust must be usable, the way Python does it"
decomposes into four things, and they have four different answers.

**1. *I can call any C library.*** **Yes, roughly now.**
`c-binding-coverage.md` puts stable-ABI C at ~95% of exported symbols, and
Science's mechanism is better than `ctypes` on two axes (checked at compile time;
a link error rather than a segfault) and worse on one (`ffi-c-boundary.md` §7.4:
*"Julia users call a C library by typing one line, and Science users generate a
binding first"*). Header-only C++ is 0% and always will be. **Parity to slight
edge.**

**2. *I do not have to think about memory.*** **No, and never.** This is the real
difference and it is not an inventory problem at all. CPython refcounts; every
binding author gets ownership for free, at the cost of the GIL and of the
interpreter being present. Science trades that for ownership checking, which is
the language's entire thesis. **The safe wrapper exists because Python's runtime
does at run time what Science asks a human to write down once.** That is the
honest answer to "the way Python does it": Python does it by having a runtime
that makes the question not arise. Science cannot have that and keep what it is
for.

**3. *The library I want is already bound.*** **No, and not this decade.** This
is the inventory, and it is §6 and §7. At 36 months on the plan above, Science
has perhaps twenty to forty bound libraries plus whatever route 2 generated and
whatever third parties added.

**4. *And it is fast.*** **Yes, and better than Python's**, because there is no
interpreter between the call sites. Python's bindings are fast *inside* the
library and slow between calls; that is why `numba`, Cython and `torch.compile`
exist. Science's are fast in both places. **This is the one axis where the answer
is unambiguously better and it is the one to lead with.**

### 8.2 The three ceilings, named

- **The C ceiling** is `c-binding-coverage.md` §5.2's: ~95% of exported symbols
  of stable-ABI C libraries, 0% of header-only C++, and about 75–90% weighted by
  what a scientist reaches for. Below that sits §5.3's wrapper ceiling, which
  §4.3 of this note argues moves from ~4% to ~40% under Decision 5 and to ~100%
  where the vendor pre-annotated.
- **The Rust ceiling** is `rust-binding-generation.md` §6's: 100% of `regex` and
  91.4% of `serde_json` from a policy file; the data path of an
  interchange-format crate at essentially 100% and its operation path at whatever
  narrow shim you choose; 0% of `async`; and a room that contains no `scipy`.
- **The Python ceiling** is `python-from-science.md` §6's two clauses: Python
  code that introspects a type Science defined, and Python code that expects to
  be the process. *"Science can call all of Python. It cannot be Python."*

### 8.3 The scale comparison, stated so nobody has to guess

Recollected, not surveyed — see §1 — and given to order of magnitude only, which
is all the argument needs.

PyPI holds something on the order of half a million projects; conda-forge, which
is closer to the relevant denominator, holds tens of thousands. crates.io holds
something like 150–200 thousand crates, of which the scientific and ML subset is
a few hundred. Julia's General registry holds roughly twelve thousand packages
after twelve years.

**The point of the comparison is not the ratio. It is that the ratio has never
been the thing that decided adoption.** A working scientist touches maybe thirty
to fifty libraries. Julia, at twelve thousand packages, is not thirty times more
adopted than Julia at four hundred was. What decided it in every case was whether
the twenty libraries that person needs are there and good.

**So the strategically correct target is not "an inventory the size of Python's".
It is "the thirty libraries a computational scientist touches, bound well".**
That target is §5.1's Tier 1 and Tier 2, it is about thirteen person-months of
binding plus twelve of tooling, and it is reachable. Saying so is the difference
between a plan and an aspiration.

### 8.4 The sentence

> **Science matches Python's binding mechanism today and will not match its
> inventory this decade. On two axes it is already ahead — the wrapper is checked
> rather than merely written, and the Rust half generates from the crate's own
> type information, which Python has no equivalent of. The shortest path to a
> usable inventory is not to write more wrappers: it is to ship the raw layer as
> a first-class distributable artefact, generate the safe layer from declarations
> instead of from headers, route every library to whichever of four mechanisms
> reaches its compiled core, and take Python's inventory whole, through the
> interpreter, for everything not yet reached — visibly, so that a reader of the
> source knows which one they are on.**

---

## 9. What this note asks of the others

**Of `models-and-inference.md`** — two routing corrections, both because that
note predates `rust-binding-generation.md`:

1. **§6.1: change the tokenizer route from "a C shim" to `sciencec foreign
   bind`.** `tokenizers` is a Rust crate with rustdoc JSON. The conformance
   suite, which that section correctly calls the deliverable, is unaffected.
2. **§6.2: consider the Rust `image` crate in place of `libjpeg-turbo` /
   `libpng` / `stb_image`.** It removes three C dependencies and the documented
   quality caveat, and `rust-binding-generation.md` §5.2 already names it.
3. **An explicit answer to `python-from-science.md` §10's ask** about whether
   `transformers` is reached through the interpreter, and therefore whether a
   program that uses it is self-contained. §3.2 of this note says it must be —
   `transformers` is pure Python over torch and has no other route — and that
   note is the one that should say it.

**Of `ffi-c-boundary.md`:**

4. **§7.2: the sidecar becomes a wrapper-generation input, with a declared
   schema** (Decision 5). This is the note's largest ask and it is a change of
   purpose rather than of mechanism: §7.2 already specifies the sidecar as
   holding the span lengths, the borrow provenance and the destructor pairings.
5. **One field in that schema recording whether a safe wrapper exists and who
   reviewed it** (Decision 6).
6. **§7.3's LAPACK paragraph should be marked as the general case rather than
   the special one.** It already predicts Decision 5 for one library.

**Of `c-binding-coverage.md`:**

7. **§5.3 should note that its 4% is measured against cuDNN**, which is the
   largest surface with the smallest realistic demand in the whole audit, and
   against a library `models-and-inference.md` §7 has already declined to need.
   The ratio is also wrappers-to-declarations rather than demand-met-to-demand.
   Neither observation weakens the section's conclusion; both bound it.
8. **Decision 4 should record its narrowing under Decision 5 here** — the human
   edit survives, and moves from a hand-written file into the sidecar.
9. **§10's warning that `bindgen --check` must not be cut for schedule gains a
   second reason.** Under Decision 5, `--check` guards generated *safe* wrappers,
   where a stale annotation produces a reviewed-looking artefact.

**Of `package-manager.md`:**

10. **A `.raw` module convention**, or a statement that none is needed because a
    module path inside a package is already free. Decision 4 assumes the latter.
11. **One `SP` code for the migration note of Decision 3** — a `use python`
    import for which a native binding is resolvable. `SP0060`–`SP0999` is free
    and this note claims none of it.
12. **Confirmation of Decision 9's F1/F2 staging** for route-2 bindings in the
    toolchain, which is that note's own Decision 6 cost paragraph applied to a
    named set.

**Of `native-dependencies.md`:**

13. **A `[native.cuda]` row in §3.3's table for the CUDA *runtime*** — allocation,
    memcpy, streams, device query — as distinct from the cuDNN and cuBLAS rows.
    §5.1 Tier 2 lists it as unowned by any note, and it is a prerequisite for
    every GPU story, including the ones that go no further than moving a tensor.
14. **§13 open question 1 — "who owns the recipe registry's platform matrix" —
    is the same question as "who maintains the seed inventory", and Decision 8
    here is this note's answer to it: 2 FTE for 18 months, 1 FTE thereafter, or
    do not attempt it.** That note asks for an owner; this one names a shape and
    a price. Somebody still has to name a person.

**Of `python-interop.md`:**

15. **§1's adoption thesis should be narrowed to a beachhead claim** (Decision 7),
    with the Julia counter-evidence and the Go counterexample named. The section's
    measure of success is right for month three and is not a strategy for year
    three.

**Of `python-from-science.md`:**

16. **A `doctor` line** listing every `use python` import alongside whether a
    native binding exists (Decision 3).

**Of `data-io.md`:**

17. **Consider `arrow-rs` plus the `parquet` crate in place of Arrow C++.** Both
    export `#[repr(C)]` Arrow C Data Interface structs, both are reachable by
    route 2 with cargo only, and neither needs a C++ compiler — which would
    *discharge* §9's own hard guarantee rather than route around it with a
    build feature. **This note does not decide it; that note owns the format
    catalogue.** The named risks are that compression codecs pull further
    dependencies, and that the Rust implementation's coverage of Parquet
    encryption and some rarer encodings lags the C++ one. Worth an hour of
    checking before the C++ toolchain requirement is locked in.

**Of `README.md`:** a row in the index, and a row in the allocation table
recording that **this note claims no diagnostic code.** Per that file's
convention the row is added before writing; this note could not, because its
brief forbade editing any file but its own and two other agents were editing the
repository at the time.

---

## 10. Risks

**Everything in §4.3 rests on one unmeasured ratio.** The claim that a sidecar
entry costs a twelfth of a hand-written wrapper is this note's own estimate, with
its reasoning exposed and no data behind it. If the true figure is two-fold
rather than twelve-fold, Decision 5 is a three-person-month tool that moves the
4% to 8% and the whole recommendation is wrong. **This is testable cheaply and
should be tested before T2 is funded**: annotate thirty libsodium functions in a
sidecar, write thirty by hand, and time both.

**Both generators are unbuilt.** Every coverage number in §2 and every route
assignment in §3 describes tools that exist only in design notes.
`rust-binding-generation.md` §1.3 says its classifier was a throwaway script;
`sciencec bindgen` has never been run. A note that ranks three routes by yield is
ranking two unbuilt things against one that requires an interpreter.

**The seed inventory is a permanent staffing commitment and it is the most likely
thing on this list to be underfunded.** Thirteen person-months to build and two
per year to keep, forever, rising with the inventory.
`native-dependencies.md` §11 already names the shape of this failure —
*"the maintenance of a port tree is the actual cost of these systems and it is
unbounded"* — and Decision 8's answer is a staffing level, which is a decision
nobody in a design document can actually make.

**Decision 4 makes `unsafe` ordinary, and this is the cost the note is least able
to mitigate.** Three mitigations are named and none is a guarantee. Rust lives
with exactly this and the Rust ecosystem's tolerance for `unsafe` in downstream
crates is a real, visible, recurring argument in that community. Science will
have the same argument, and the fact that this note chose it deliberately does
not make the argument wrong.

**The routing rule of §3 depends on a recollected table.** §3.2 is the author's
knowledge of twenty projects' build shapes, not a survey. Three of its rows are
reclassifications and each is argued from a sibling note, so they survive an
error; the rest should be re-derived before the table is quoted.

**Route 1's success is corrosive in proportion to its success, and nothing here
fixes that.** `python-from-science.md` §5.3: *"The better §2 works, the worse
§6.1 gets."* The migration note of Decision 3 is a nudge, not a mechanism. A
plausible ten-year outcome is that Science becomes a well-liked language for
writing fast functions inside Python programs and never acquires an inventory,
and every individual decision along that path is locally correct.

**The Windows problem is unowned and it undercuts the seed inventory
specifically.** `native-dependencies.md` §11: *"There is no `pkg-config`, no
system library directory convention, and no equivalent of a module system, so the
`system` provider — the default, and the load-bearing one — mostly does not
work."* A curated inventory defaulting to `system` is a curated inventory that is
largely broken on the platform most new users will try it on first.

**"Science can call any C library" is the sentence that travels.** This is
`c-binding-coverage.md` §10's last risk, restated for this note because it
applies harder here: §8's four-part decomposition is what should be quoted, and
§8.1's first claim is the part that will be. If one line from this note is
quoted, it should be §8.4's, and if only a fragment survives it should be:
**Science matches Python's mechanism and will not match its inventory this
decade; the thirty libraries a scientist touches is a reachable target and the
right one.**

---

## 11. Summary of decisions

| | Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | Routes rank 1>2>3 by yield, fund 3>2>1 by necessity | Fund by yield; or fund route 3 alone | Yield-first produces a Python program with extra steps; necessity-alone is the 4% trap | Three binding mechanisms maintained, not one |
| 2 | A four-row routing rule, keyed on whether the compiled core is separable from CPython | Sort by implementation language; one route per ecosystem | Language-sorting misplaces NumPy, torch and half of scipy; per-ecosystem cannot express the artefact row | Four routes, and the artefact row depends on upstream exporters |
| 3 | No facade across the seam; a migration note and a `doctor` line instead | A per-library compatibility shim with silent fallback | A fallback would silently make a binary not self-contained — `native-dependencies.md` Decision 4's rejected case | The user must know which route they are on |
| 4 | Two modules per binding package: `<name>.raw` (generated, complete) and `<name>` (safe, never required to be complete) | Keep `.raw` private; a heuristic "thin" mode; type-system trust tiers | Rust's `-sys` convention is the only case of a language with an ownership model acquiring an inventory; heuristics emit `noalias` on a keyword the callee may cast away | **`unsafe` becomes ordinary**, which is the thesis eroding from inside |
| 5 | `bindgen` emits the safe wrapper from the sidecar, from five named shapes | Infer the shape from the signature | The sidecar already holds the contract and currently discards it; guessing produces wrong answers at scale | 3 pm; a wrong annotation now yields a reviewed-*looking* wrapper, so `--check` is load-bearing twice |
| 6 | One sidecar field: does a safe wrapper exist, and who reviewed it | A trust tier in the type system | `unsafe` already grades trust; this records coverage, which nothing currently reports | None |
| 7 | Python-callability is the beachhead; the Tier 1 inventory is the strategy | Lead with the inventory; treat the beachhead as sufficient | Julia had PyCall and stalled; Go had no incremental path and did not. Beachhead-only is Cython with a better type system | An expectation set out loud, hence Decision 10 |
| 8 | 2 FTE for 18 months, 1 FTE thereafter — or do not attempt an inventory | Build it with contributors; tooling-first; inventory-first | There are no contributors yet; tooling-first is two years before anything works; inventory-first is twelve rewrites | A staffing commitment a design note cannot actually make |
| 9 | Route-2 bindings ship in the toolchain through F1, move to Level 3 at F2 with stable module paths | Level 2 permanently; wait for F2 | `package-manager.md` §1.3's one-way ratchet; waiting loses the whole of F1 | The two-tier world persists through F1, with more inhabitants |
| 10 | Falsification declared in advance: zero third-party binding packages at 36 months means the strategy failed | Leave it unstated | There is no test that fails; a declared metric is the only mechanism available | Committing in public to a number that can be missed |
