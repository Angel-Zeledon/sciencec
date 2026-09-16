# Science — Design: how much of C is actually reachable

Date: 2026-09-16
Status: draft for review
Parent: `ffi-c-boundary.md`. This note re-decides nothing in it and adds one ask
to its grammar (§4.2). Where it disagrees, it says so by section number.
Siblings: `native-dependencies.md` (how the libraries arrive),
`data-io.md` §9 (the no-C++-compiler guarantee), `python-interop.md` §3.2 (the
limited API), `stdlib-standard.md` §13 (libsodium),
`scientific-libraries.md` §2 (what links).
Syntax: `syntax-revision-2.md`.

Scope: an audit of **what fraction of a real C library gets through
`ffi-c-boundary.md`'s boundary, what the residue is, and what it would cost to
close it.** Not the design of the boundary — that is settled and this note
assumes it.

Diagnostics: `SC0490`–`SC0499`, per `README.md`'s allocation table.

---

## 0. The question, and the answer up front

The project's owner has asked that **all C code be usable from Science**. C is
the best of the three ecosystems Science must reach — it has a stable ABI, and
headers that describe the interface — so if "all" is achievable anywhere, it is
achievable here.

`ffi-c-boundary.md` designed the boundary well. Nobody has measured it. This note
takes five header sets Science's own design already commits to, classifies every
exported entity, and reports the number.

> **Of 5,265 public entities across the five libraries, 65% are directly
> bindable under `ffi-c-boundary.md` as written; 91% become bindable with four
> additions that require no C compiler; 99% become bindable with a generated C
> shim; and 0.8% — variadics — are irreducible.**

Three things about that number matter more than the number.

**First, the residue is not evenly distributed and not where the parent note
expected.** §7.2 named function-like macros as a cost and proposed "a per-library
allowlist and some hand-written supplements". The audit finds 612 non-symbol
entities, of which roughly a third are *not* macro-expansion problems at all:
they are **exported global variables**, which `ffi-c-boundary.md` §1.2's
three-item list cannot declare. HDF5 is unusable for that reason alone — not
90%-usable, *unusable*, because you cannot read a dataset without
`H5T_NATIVE_DOUBLE` and `H5T_NATIVE_DOUBLE` is a macro over a global. So is
`Py_None`. One new declaration item recovers 7.6% of the surface and is the highest
yield per line of specification anywhere in this note.

**Second, the reachable fraction is a property of the library, not of C.** The
same CPython is 65% reachable through its full headers and 99% reachable through
its limited API, because the limited API turns `Py_INCREF` into a call to
`Py_IncRef`. Four of the five libraries audited ship a symbol-level form of their
macro surface, because every other language's FFI hit this wall first and the
vendors responded. §4.5 makes "look for the vendor's own shim before generating
one" a rule.

**Third, and this is the honest answer to the owner's question: reachable is not
usable.** A conservative generated binding per §7.1 makes 100% of the symbols
*callable* and 0% of them *safe* — every pointer is `ffi.Pointer`, every call is
inside `unsafe`. The number that decides whether a scientist can use a library is
the fraction with a hand-written safe wrapper, and §7.1 states that ratio itself:
1,500 generated cuDNN declarations support about 60 wrappers. **4%.** §5 returns
to this, because it is the ceiling that actually binds.

---

## 1. Method, and its limits

Four categories, applied to every entity a public header exports or defines:

| | Category | Test |
|---|---|---|
| **A** | Directly bindable | A real exported symbol whose whole signature is expressible in `ffi`'s closed vocabulary (§1.3) today |
| **B** | Bindable after a declared policy | A real symbol, blocked on a decision Science has not made — complex scalars, exported globals, alignment, unions, runtime-sized slots, by-value aggregates |
| **C** | No symbol exists | A function-like macro or a `static inline` function. Nothing to link against |
| **D** | Unreachable | Variadic, `va_list`, or C++-only |

"Entity" means a thing a caller writes: an exported function, an exported global,
a function-like macro, a `static inline` function. Object-like macros that are
plain integer or string constants are excluded throughout — `ffi-c-boundary.md`
§1.2's `const NAME be literal as T` already handles them and they are not
interesting. Object-like macros that are *not* plain constants are counted,
because they are the problem.

**What was measured and what was estimated.** Only CPython was counted from real
headers, because CPython 3.14's headers are the only ones present on the machine
this note was written on — 79 top-level and 59 `cpython/` headers, `internal/`
excluded as not public. Every CPython figure below is a count. The other four are
estimated from published symbol inventories and from the headers' documented
structure, and the aggregate is correspondingly soft. **This is stated rather
than hidden, and §10 records it as the note's first risk: the audit should be
re-run under `sciencec bindgen --classify` against real headers before any of
these numbers is quoted outside this document.** The shape of the finding does
not depend on the precision; the third decimal place does.

The libraries, and why each is here:

| Library | Why it is in the audit |
|---|---|
| OpenBLAS / LAPACK | `ffi-c-boundary.md` §1.6–1.7 writes the binding; `scientific-libraries.md` §2 calls it "not negotiable" |
| libsodium | `stdlib-standard.md` §13.1 puts all of `crypto` on it |
| HDF5 | `ffi-c-boundary.md` §0 and §2.3. See the seam in §2.4 below |
| zlib | The **control**. Not a Science dependency — `data-io.md` §9 vendors `miniz` instead — but `miniz` is zlib's API, and zlib is the oldest, smallest, most conservatively written C library in wide use. It is the ceiling for what a well-behaved C API looks like |
| CPython | `python-interop.md` depends on it entirely. Measured |

---

## 2. The audit

### 2.1 zlib — the control, and what a good C API looks like

**~88 exported functions, ~6 function-like macros, 94 entities.**

| | Count | Share |
|---|---|---|
| A | 79 | 84% |
| B | 8 | 9% |
| C | 6 | 6% |
| D | 1 | 1% |

**A** is almost everything: `deflate`, `inflate`, `compress2`, `adler32`,
`crc32`, the whole `gz*` family. Flat scalar arguments, one struct passed by
pointer, no by-value aggregates, no `_Complex`.

**B, and it is instructive.** `z_stream` is a *transparent* struct the caller
allocates and fills — the case §3.7 calls "binding is possible and wrong" — and
here it is possible and **right**, because zlib documents the layout as part of
its ABI and has not changed it since 1995. The `alloc_func`/`free_func` fields
are C's callback idiom and §4.2 of the parent note covers them exactly. `gzFile`
is an opaque handle with `gzclose` as its destructor: Case B of §2.1, verbatim.

The transparent struct is load-bearing rather than incidental — a zlib caller
writes `next_in` and `avail_in` between calls, and there is no functional
interface that would let it not:

```science
def inflate_all(source: borrowed Array of U8) -> (Array of U8, Error?):
    let mutable stream be ZStream.new()
    let mutable out be Array of U8 .with_capacity(source.length() * 4)
    let mutable window be Array of U8 .zeroed(65536)

    if inflate_init(stream) is not Z_OK:
        return (out, ZlibError.Init)

    loop:
        let produced, status be stream.step(source, window)
        for byte in window.span(0, produced):
            out.push(byte)
        if status is Z_STREAM_END:
            break
        if status is not Z_OK:
            return (out, ZlibError.Stream(status))

    return (out, null)
```

Every `ffi` type in that program is inside `ZStream` and `stream.step`, which is
what §5.3 means by wrapping.

**C is six macros and none of them needs a shim.** Every one is an alias over a
real symbol with defaulted arguments:

```c
#define deflateInit(strm, level) \
    deflateInit_((strm), (level), ZLIB_VERSION, (int)sizeof(z_stream))
```

The generator rewrites that into a Science function — class **M1** in §3.1.

**D is one function: `gzprintf`.** Variadic. Unreachable, and nobody will miss it.

**The one real hazard is not in any of the four categories.** Under
`_LARGEFILE64_SOURCE` or `Z_LARGE64`, zlib.h redefines `gzopen` to `gzopen64`,
`gzseek` to `gzseek64` and `z_off_t` to 64 bits — an object-like macro that
changes *which symbol you link against* based on how the header was configured.
That is `ffi-c-boundary.md` §1.6's `BlasInt` hazard, in a compression library,
and it belongs to `native-dependencies.md`'s variant vocabulary (§4.1) rather
than to bindgen. §3.1 calls this macro class **M5** and §9 asks for it.

**Reading**: a C library written in 1995 by people who expected to be called from
other languages is 84% directly bindable and 99% bindable with rewriting alone.
That is the ceiling, and it is high.

### 2.2 libsodium — the library that already solved this

**~700 exported functions, ~0 function-like macros, 700 entities.**

| | Count | Share |
|---|---|---|
| A | 690 | 99% |
| B | 10 | 1% |
| C | 0 | 0% |
| D | 0 | 0% |

libsodium is the best case in the audit and the reason is a deliberate policy:
**every compile-time constant is also available as a function.**
`crypto_box_PUBLICKEYBYTES` is an object-like macro, and `crypto_box_publickeybytes()`
is an exported symbol returning the same value. The library did this explicitly
so that FFI bindings in dynamic languages would work. `crypto_generichash_statebytes()`
returns `sizeof(crypto_generichash_state)` at run time, for the same reason.

That is a C shim, written by the vendor, shipped in the library, and maintained
against every version. It is the single most important observation in this note
and §4.5 turns it into a rule.

**B is ten entities and two `ffi` gaps.** The hashing state types are declared
`CRYPTO_ALIGN(64)`, and `ffi.CLayout` (§1.4) has no way to state an alignment
stronger than the fields imply. And the `_statebytes()` idiom means the state's
size is known only at run time, while `ffi.Uninitialized of T` (§1.3) is "a slot
of the right size and alignment" — a compile-time size. Both are small `ffi`
additions, asked for in §9.

Nothing in libsodium is variadic. Nothing is `static inline`. Nothing is C++.
`stdlib-standard.md` §13 is, by this measure, resting on the safest foundation
any note in the project has chosen.

### 2.3 OpenBLAS, CBLAS and LAPACKE — the residue is ABI, not syntax

**~160 CBLAS functions + ~1,700 LAPACKE routines, ~1,860 entities.**

| | Count | Share |
|---|---|---|
| A | 930 | 50% |
| B | 930 | 50% |
| C | 0 | 0% |
| D | 0 | 0% |

There are **no function-like macros and no `static inline` functions** in
`cblas.h`. `lapacke.h` has a large family of `LAPACK_GLOBAL(dgetrf,DGETRF)`
name-mangling macros, which are object-like, resolve to real symbols, and are
class **M5** — a build-time variant, not a binding problem.

**So where does the other half go?** Into **B**, and for exactly one reason:
`ffi`'s scalar vocabulary (§1.3) has no complex type, and half of LAPACK's 1,700
routines are the `c` and `z` variants. BLAS and LAPACK come in four precisions
and two of them are complex; that is not a corner of the library, it is the
library.

**This is a real inconsistency in the parent note and it should be named.**
§1.3's scalar table is declared a *closed set* — "what is listed exists and
nothing else does" — and it has no complex entry. §2.4 of the same note then
writes:

```science
output: ffi.MutableSpan of ffi.Complex64
```

`ffi.Complex64` is used and never defined. §9 asks for it.

The complex residue has three separable parts, and only the first is a language
question:

1. **The type.** `ffi.Complex32` / `ffi.Complex64` as two-field `ffi.CLayout`
   records over `F32`/`F64` pairs. This is what LAPACKE means by
   `lapack_complex_double` when `LAPACK_COMPLEX_STRUCTURE` is defined, and it is
   layout-compatible with `double _Complex` on every ABI Science targets. Cheap.
2. **Which representation the header was built for.** `lapack_complex_double` is
   `double _Complex` by default and a struct under `LAPACK_COMPLEX_STRUCTURE` —
   a header-time switch that changes a type's identity, class **M5** again.
   Layout-identical in practice; **not** identical in argument-passing
   classification on every ABI, which matters the moment §5.4's by-value
   restriction is lifted.
3. **The complex return convention.** `cdotc` and `zdotu` return a complex value,
   and `f2c`-derived builds return it through a hidden pointer while `gfortran`
   returns it in registers. `native-dependencies.md` §4.4(2) already lists this
   as a variant dimension the vocabulary does not have a name for, and calls it
   "a real, current source of wrong answers". CBLAS's own answer is to ship
   `cblas_cdotc_sub`, which takes an out-parameter and returns `void` — the
   vendor's shim again (§4.5). **The binding should use the `_sub` forms and
   never the value-returning ones**, and that is a sidecar annotation, not a
   language feature.

**Reading**: BLAS and LAPACK are 100% reachable and 50% reachable *today*, and
the gap is one two-field record. This is the cheapest 930 entities in the audit.

### 2.4 HDF5 — 90% of the functions, 0% of the workflows

**~600 public functions + ~200 non-constant object-like macros, ~800 entities.**

| | Count | Share |
|---|---|---|
| A | 545 | 68% |
| B | 45 | 6% |
| C | 200 | 25% |
| D | 10 | 1% |

**A seam first.** HDF5 is named by `ffi-c-boundary.md` §0 as the motivating
example and its handle binding is written out in §2.3, but `data-io.md` §3 puts
HDF5 **out** of the format catalogue — "the most painful call in this note" —
and `scientific-libraries.md` §2 does not bind it either. So HDF5 is at present a
library the FFI note uses to explain itself and no consumer note has taken. It is
audited here anyway, for two reasons: the parent note's §2.3 `H5File` example is
the canonical Case-B binding and should be known to work, and **the mechanism
HDF5 exposes as missing is needed by CPython too**, where it is not optional.

**The finding.** Count the functions and HDF5 looks fine: 545 of 600 are directly
bindable, the `hid_t` handle model is exactly §2.3's shape, and `H5Literate`'s
`op_data` callback is exactly §4.3's shape. Now try to write a program.

```c
#define H5OPEN                H5open(),
#define H5T_NATIVE_DOUBLE     (H5OPEN H5T_NATIVE_DOUBLE_g)
```

`H5T_NATIVE_DOUBLE` is not a constant. It is a comma expression that calls
`H5open()` and then reads an exported `hid_t` global. There are roughly 200 of
these — every `H5T_*` datatype, every `H5P_*` property-list class, every `H5E_*`
error identifier. `ffi-c-boundary.md` §7.2 already spotted this exact macro and
drew the right conclusion about header parsers:

> `H5T_NATIVE_DOUBLE` is a macro expanding to a global variable dereference, and
> no header parser recovers it as a constant

— and then proposed an allowlist and hand-written supplements, which is the wrong
remedy, because the thing that cannot be written by hand is not the *value* (it
has none at compile time) but the *declaration*. `ffi-c-boundary.md` §1.2 admits
three items — function, type alias, constant — and **there is no way to declare
an exported global variable in an `extern` block.**

Consequence, stated plainly: under the boundary as specified, a Science program
can open an HDF5 file (`H5P_DEFAULT` is genuinely `(hid_t)0`) and cannot read a
dataset from it, because `H5Dread` needs a memory type and every memory type is a
global. 545 bindable functions, zero usable workflows. §4.2 is the fix and it is
four lines of grammar.

**B is 45 entities and includes the union case the parent note said it did not
have.** `ffi-c-boundary.md` §1.4 states:

> No library in the target set — BLAS, LAPACK, cuBLAS, cuDNN, HDF5, GSL, MPI,
> NetCDF — needs a bitfield or a union in its public interface.

HDF5 is in that list and it does. `H5L_info2_t`, filled by `H5Lget_info2` and by
every `H5Literate` callback, contains a union of an `H5O_token_t` and a
`size_t`; `H5R_ref_t` is a union over a fixed byte buffer. The claim is wrong for
unions and right for bitfields, and §3.4 amends it.

**D is a handful of variadic entry points** in the VOL "optional" dispatch and
the H5LT convenience layer. Avoidable; the non-variadic form exists in each case.

### 2.5 CPython — measured, and the clearest result in the audit

Counted from CPython 3.14's public headers on this machine:

| Measurement | Count |
|---|---|
| `PyAPI_FUNC` declarations (public) | **1,206** |
| of which variadic (`...`) | 26 |
| of which take `va_list` | 7 |
| `PyAPI_DATA` exported globals | **199** |
| unique function-like macros (public) | **435**, of which 116 are `_Py`-private → **319 public** |
| unique `static inline` functions (public) | **101**, of which 14 are private → **87 public** |
| macros that also have a same-named exported symbol | 31 |
| public entities, total | **1,811** |

| | Count | Share |
|---|---|---|
| A | 1,173 | 64.8% |
| B | 199 | 11.0% |
| C | 406 | 22.4% |
| D | 33 | 1.8% |

**C is the largest residue in the audit** — 319 macros plus 87 `static inline`
functions, and it contains the hottest names in the API. `Py_INCREF` and
`Py_DECREF` are `static inline` in 3.14. `Py_TYPE` is `static inline`.
`PyList_GET_ITEM`, `PyTuple_GET_ITEM`, `PyBytes_AS_STRING` are macros that index
a struct field. `PyLong_Check` is a macro over `PyObject_TypeCheck`, which is
itself `static inline`.

**B is 199 exported globals** — `Py_None` (as `&_Py_NoneStruct`), every
`PyExc_*` exception type, every `PyXxx_Type` type object. Same mechanism HDF5
needs, same fix, and here it is unconditional: a Python extension that cannot
name `Py_None` or `PyExc_TypeError` is not a Python extension.

**And then the finding that redeems all of it.** Reading `refcount.h`:

```c
static inline Py_ALWAYS_INLINE void Py_INCREF(PyObject *op)
{
#if defined(Py_LIMITED_API) && (Py_LIMITED_API+0 >= 0x030c0000 || defined(Py_REF_DEBUG))
    // Stable ABI implements Py_INCREF() as a function call on limited C API
    ...
    _Py_IncRef(op);
```

and `object.h`:

```c
// Py_TYPE() implementation for the stable ABI
PyAPI_FUNC(PyTypeObject*) Py_TYPE(PyObject *ob);
```

**Under `Py_LIMITED_API`, CPython converts its own hot macros into exported
function calls.** Counting `PyAPI_FUNC` declarations outside `#ifndef
Py_LIMITED_API` regions in the top-level headers gives **614 functions** — about
half the surface — and within that half the macro residue is close to zero,
because the limited API's whole purpose is to be callable across an ABI boundary.

So CPython is two libraries:

| Target | Entities | A | C (no symbol) |
|---|---|---|---|
| Full C API | 1,811 | 65% | 22% |
| Limited API / abi3 | ~614 | ~99% | ~1% |

**`python-interop.md` §3.2 already chose abi3**, and it chose it for wheel-matrix
reasons — one wheel per platform for every 3.11+ interpreter. It did not know it
was also choosing the only CPython target that Science can call at all. That note
contains **zero occurrences of the word "macro"**, never mentions
`Py_LIMITED_API` as a define, and commits in §4.4 and §5.5 to emitting
`Py_INCREF`, `Py_DECREF` and `Py_BEGIN_ALLOW_THREADS` — one `static inline`
function and two macros, none of which a code generator emitting LLVM IR can
call. The mitigation is already in that note's §3.2 and the reasoning is not.
§9 asks for the paragraph.

`Py_BEGIN_ALLOW_THREADS` deserves its own line because it is the interesting
case: it is a macro that **opens a brace** —
`{ PyThreadState *_save; _save = PyEval_SaveThread();` — so it is not a call and
never will be, in any API mode. But both halves of it are exported symbols, so
the answer is to write the two calls out:

```science
let saved be unsafe: PyEval_SaveThread()
let result be compute(problem)
unsafe: PyEval_RestoreThread(saved)
```

That is class **M1** in §3.1 — a macro whose parts are symbols — and it is
handled by rewriting, not by a shim.

### 2.6 The aggregate

| Library | Entities | A | B | C | D |
|---|---:|---:|---:|---:|---:|
| zlib (control) | 94 | 79 | 8 | 6 | 1 |
| libsodium | 700 | 690 | 10 | 0 | 0 |
| CBLAS + LAPACKE | 1,860 | 930 | 930 | 0 | 0 |
| HDF5 | 800 | 545 | 45 | 200 | 10 |
| CPython 3.14 (full) | 1,811 | 1,173 | 199 | 406 | 33 |
| **Total** | **5,265** | **3,417** | **1,192** | **612** | **44** |
| | | **64.9%** | **22.6%** | **11.6%** | **0.84%** |

Cumulative:

| Reachable after | Share |
|---|---|
| `ffi-c-boundary.md` as written | **64.9%** |
| + the four additions in §4.2 and §9 (no C compiler needed) | **91.3%** |
| + a macro rewriter in `bindgen` (no C compiler needed) | **~96.5%** |
| + a generated C shim (C compiler needed) | **99.2%** |
| irreducible | **0.84%** |

The middle two rows are the decision §4 has to make and they are separated
deliberately: **the two mechanisms that need no C compiler buy 31.5 points; the
shim buys 2.7.**

---

## 3. The residue

### 3.1 Preprocessor macros — five classes, four of them mechanical

This is the largest part of the residue and it is not one problem. Treating it as
one is why `ffi-c-boundary.md` §7.2 reached for an allowlist. Classified by what
the expansion *contains*, the 612 non-symbol entities sort into five classes with
five different answers.

**M1 — a macro over one or more real symbols.** Aliases, defaulted arguments,
casts, and statement macros whose parts are calls.

```c
#define PyObject_DelAttr(O, A)   PyObject_SetAttr((O), (A), NULL)
#define deflateInit(strm, level) deflateInit_((strm),(level),ZLIB_VERSION,(int)sizeof(z_stream))
#define Py_BEGIN_ALLOW_THREADS   { PyThreadState *_save; _save = PyEval_SaveThread();
```

**Answer: the generator rewrites it into a Science function.** No C compiler, no
new language feature. What it costs is that `bindgen` must evaluate a subset of C
expression syntax — calls, integer literals, `sizeof`, casts, `NULL`, string
literals, and identifiers that resolve to other declared entities — and must
refuse anything outside that subset rather than guess. Refusing is `SC0495`.
Estimated share of the 612: **~38%** (231 entities).

**M2 — a macro over an exported global.**

```c
#define H5T_NATIVE_DOUBLE  (H5open(), H5T_NATIVE_DOUBLE_g)
#define Py_None            ((PyObject *)&_Py_NoneStruct)
```

**Answer: declare the global.** `ffi-c-boundary.md` §1.2 cannot, and §4.2 below
adds the item. No C compiler. Estimated share: **~33%**, and it is the share
that decides whether HDF5 and CPython work at all.

**M3 — a macro that reads a struct field.**

```c
#define PyList_GET_ITEM(op, i)  (_PyList_CAST(op)->ob_item[i])
#define PyDateTime_GET_YEAR(o)  ((((PyDateTime_Date*)(o))->data[0] << 8) | ...)
```

**Answer: it depends on whether the layout is a promise.** Where the vendor
publishes the layout as ABI (zlib's `z_stream`), bind the struct with
`ffi.CLayout` and write the accessor in Science — no C compiler. Where the layout
is an implementation detail (`PyListObject` is, and abi3 forbids touching it),
**use the safe function instead** — `PyList_GetItem` exists and is exported, and
the only thing the macro buys is a bounds check elided. §3.7 is the general rule.
Where neither is available, this is the shim. Estimated share: **~13%** (80 entities), of which
maybe a third genuinely needs the shim.

**M4 — a macro that is computation or syntax with no symbol underneath.**
`assert`, `offsetof`, `container_of`, `errno` as a thread-local lvalue, the
`GL_*` dispatch macros, OpenMP pragmas, GTK's `G_OBJECT` cast chain. There is no
function, no field, and nothing to rewrite into.

**Answer: a generated C shim, or nothing.** This is the only class where a C
compiler is the answer, and it is the smallest class. Estimated share: **~16%** (101 entities),
which with the un-warranted part of M3 is **~2.7% of all entities**.

**M5 — a macro that renames a symbol based on a header-time flag.**

```c
#  define gzopen gzopen64                       /* zlib, under Z_LARGE64 */
#  define LAPACK_dgetrf LAPACK_GLOBAL(dgetrf,DGETRF)
   typedef double _Complex lapack_complex_double; /* or a struct, under a flag */
```

**Answer: this is not a binding problem, it is an ABI variant.** It belongs in
`native-dependencies.md` §4.1's `(capability, variant)` identity, checked by
Decision 6 at resolution and by Decision 7's startup probe — because the failure
mode is `ffi-c-boundary.md` §1.6's exactly: a program that links cleanly and is
wrong at scale. §9 asks for the vocabulary entries. Estimated share: **~2%** of
the macro residue and rather more than 2% of the danger.

> **Decision 1.** `sciencec bindgen` classifies every function-like macro into
> M1–M5 and records the class in the sidecar file `ffi-c-boundary.md` §7.2
> already requires. M1 and M2 are emitted as Science automatically. M3 is emitted
> only where the sidecar carries a layout warrant (§3.7); otherwise the generator
> emits nothing and names the exported function that supersedes it. M4 is emitted
> only if the package opts into a shim (§4.4). M5 is **never** emitted as a
> binding; it is reported as a variant that the manifest must declare, and a
> binding generated against an unverified M5 flag is `SC0497`.
>
> **Rejected: expand every macro with the preprocessor and bind the result.**
> This is what a libclang-based generator does by default and it produces two
> failures. Expanded M2 macros lose the global and become a fabricated constant
> — a generator that reads `H5T_NATIVE_DOUBLE` as an integer bakes in whatever
> address the global had in *its* process, which is not a number that means
> anything. Expanded M5 macros freeze a build-time choice into checked-in source,
> which is `SC0497`'s whole reason for existing.
>
> **Rejected: `ffi-c-boundary.md` §7.2's per-library allowlist.** It is not
> wrong, it does not scale, and it is the wrong shape: an allowlist is a list of
> exceptions, and M1 and M2 together are 70% of the residue and the normal case.
> A classifier with five outcomes is less code than an allowlist with 600
> entries, and it transfers to the next library.
>
> **Cost.** `bindgen` grows a C expression evaluator with a deliberately small
> grammar, and the boundary of that grammar is a place where the tool will
> refuse work a human can see is fine. `SC0495` must therefore name the header
> line and the expansion, so that the human can supply it in the sidecar in one
> line rather than reverse-engineering the refusal.

### 3.2 `static inline` functions in headers

Same problem as M4 — no symbol — and a different population. Measured: **87
public `static inline` functions in CPython 3.14**, which is 5% of its surface.
Modern C99-era numerical headers are full of them, and the C committee's
direction of travel is toward more of them, because an inline function type-checks
where a macro does not.

They split exactly as the macros do. A `static inline` that calls exported
functions is M1 and is rewritten. One that reads a documented struct field is M3.
One that is genuine computation is M4 and needs a shim.

CPython's `Py_INCREF` is worth the example, because it is all three at once
depending on build: under abi3 ≥ 3.12 it is a call to `_Py_IncRef` (M1); under a
normal build it is `op->ob_refcnt++` (M3, on a struct whose layout is not a
promise); under a free-threading build it is an atomic operation on a field that
is a **union** (M4, plus §3.4's problem).

> **Decision 2.** `static inline` is classified by the same M1–M5 machinery as
> macros and gets no separate mechanism. Where a `static inline` function and an
> exported function of the same name both exist — which is how a library signals
> "this is the ABI-safe form" — the generator binds the **exported** one and
> records that it did.
>
> **Rejected: always shim `static inline`, since a `.c` file that calls it is
> trivially correct.** It is trivially correct and it is not free: it makes the
> C compiler a requirement for a class that is mostly M1, and it discards the
> vendor's own signal. CPython declares `PyAPI_FUNC(PyTypeObject*) Py_TYPE(...)`
> for the limited API precisely so that binders do not have to shim it.
>
> **Cost.** The generator must resolve a name that is both an inline definition
> and an extern declaration, and pick by ABI mode, which means bindgen's
> invocation must record which `Py_LIMITED_API` / feature-test macros were
> defined. That goes in the generated file's provenance header, which §7.2
> already requires.

### 3.3 Varargs — the irreducible 0.84%

**Measured: 26 variadic and 7 `va_list` functions in CPython's 1,206.**
`gzprintf` in zlib. A handful in HDF5's dispatch layers. None in libsodium, none
in CBLAS, none in LAPACKE.

`ffi-c-boundary.md` §1.4 already decided this — "Variadic functions are not
representable. No `printf`." — and the audit confirms both halves of the
reasoning: the C variadic convention genuinely is not expressible in a fixed
signature, and nothing in the target set needs it. The 0.84% is the whole
irreducible residue of this audit.

> **Decision 3.** Variadics stay unreachable. `bindgen` emits no declaration for
> a variadic function and records it in the sidecar with the reason. Where a
> variadic function has a `va_list` sibling — `PyErr_FormatV` beside
> `PyErr_Format`, `vfprintf` beside `fprintf` — the generator names the sibling
> in the diagnostic, because a shim over the `va_list` form is the one shim a
> human can write correctly in three lines and a generator cannot write at all.
>
> **Rejected: a fixed-arity shim per call shape** — generate
> `science_shim_printf_ii(const char *f, int a, int b)` for each arity and type
> tuple a program uses. It works, every FFI eventually considers it, and it is
> rejected because the set of shapes is unbounded, it makes the shim a function
> of the *program* rather than of the library (so it cannot be checked in, and
> §4.4's whole decision collapses), and the thing being bought is `printf`.
>
> **Rejected: libffi for variadic calls only.** §7.4 rejected runtime FFI
> wholesale, and this is the one place its argument is weakest, since libffi
> handles variadics and nothing else here does. It is still rejected: the cost is
> a runtime dependency and a second calling mechanism, and the benefit is 0.84%
> of a surface whose non-variadic half always exists. **Named as a cost anyway**,
> because it is the only place in this audit where the rejected alternative is
> strictly more capable.
>
> **Cost.** No `printf`-family formatting from Science through C, and no HDF5 VOL
> "optional" dispatch. Both have non-variadic routes.

### 3.4 Layout: bitfields, unions, anonymous structs, flexible arrays, `_Complex`

`ffi-c-boundary.md` §1.4 covers bitfields and unions together and says neither is
representable, and that no library in the target set needs either. The audit
splits that claim.

**Bitfields: the claim holds.** Not one bitfield in any public interface of the
five libraries. The reasoning in §1.4 — GCC and MSVC lay them out differently on
the same hardware — is also why library authors avoid them at ABI boundaries. No
change.

**Unions: the claim is wrong**, and §1.4 should be amended.

| Library | Union in the public interface |
|---|---|
| HDF5 | `H5L_info2_t` (a union of `H5O_token_t` and `size_t`, filled by every `H5Literate` callback); `H5R_ref_t` |
| CPython | `PyObject.ob_refcnt` is a union of `Py_ssize_t` and an atomic pair in free-threading builds |

The remedy §1.4 already names — "imported as an opaque byte array of the right
size and alignment" — is correct and sufficient for both, and it should simply be
stated as the normal path for unions rather than as an unpleasantness that does
not arise. What it costs is an accessor per variant, and in both cases above the
library provides one (`H5Rget_type`, `Py_REFCNT`), so the accessor is a binding,
not a shim.

**Anonymous structs and unions** are a naming problem, not a layout problem: the
fields flatten into the parent at the offsets C gives them, which `ffi.CLayout`
already specifies. `bindgen` synthesizes a name. No decision needed.

**Flexible array members** (`char data[];` as a final field) are a *sizing*
problem: `sizeof` excludes them, the allocation does not, and Science's
`ffi.Uninitialized of T` has a compile-time size. This is the same gap
libsodium's `_statebytes()` exposes from the other side, and §9 asks for the same
fix — an `ffi.Uninitialized` constructible from a runtime byte count.

**`_Complex` is the big one** and §2.3 has it: half of LAPACK. §9 asks for
`ffi.Complex32` / `ffi.Complex64` as two-field `ffi.CLayout` records, which is
also what `ffi-c-boundary.md` §2.4 already assumes exists.

### 3.5 Function-like typedefs and callbacks — confirming §4

`ffi-c-boundary.md` §4 is the part of the parent note the audit confirms most
completely. Every callback in all five libraries is the function-pointer +
`void *user_data` pair §4.1 describes:

| Library | Callback | Shape |
|---|---|---|
| zlib | `alloc_func` / `free_func` in `z_stream` | pointer + `opaque` field |
| zlib | `in_func` / `out_func` in `inflateBack` | pointer + `void *desc` |
| HDF5 | `H5L_iterate2_t` in `H5Literate` | pointer + `void *op_data` |
| HDF5 | `H5E_auto2_t` in `H5Eset_auto2` | pointer + `void *client_data` |
| CPython | `PyCFunction`, `destructor`, `getter`/`setter` | pointer + `self` |
| CPython | `PyCapsule_Destructor` | pointer + capsule context |

§4.3's `ffi.Callback` handles the call-scoped ones and §4.4's stricter rule — a
registered callback captures no borrows — handles `H5Eset_auto2` and
`PyCapsule_New`. Two things are left over and neither is large.

**First: a callback whose contract is "return non-zero to stop".** `H5Literate`'s
`herr_t` return is tri-valued: zero continues, positive stops with success,
negative stops with failure. That is not an error convention, it is control flow,
and `ffi-c-boundary.md` §6.1's decision to convert nothing is right for it — but
it means the safe wrapper around an iterating API has to translate a Science loop
into a return code. This is fine and it is worth an example, because it is the
shape every iteration binding takes:

```science
def each_link(group: borrowed H5Group, visit: mutable borrowed (def(borrowed String) -> Bool)) -> Error?:
    let entry be ffi.Callback.of(visit)
    let status be unsafe:
        H5Literate2(group.id, H5_INDEX_NAME, H5_ITER_NATIVE, null,
                    entry.code(), entry.data())
    if status < 0:
        return H5Error.from_stack()
    return null
```

**Second: a callback stored in a struct the caller allocates.** `z_stream.zalloc`
and Arrow's `ArrowSchema.release` are function pointers in a record, not
arguments to a call. §1.3 permits `ffi.FunctionPointer` "as a field only", so
this is expressible — but the *region* argument that makes §4.3 sound, that the
`Callback` borrows the closure and cannot outlive it, does not reach a raw
`ffi.FunctionPointer` in a field. Storing a trampoline's `code` in a struct
discards the borrow. That is a genuine hole and §4.4's registered-callback rule
is the right place for it: **a closure whose trampoline is stored in a foreign
struct is a registered callback and may capture no borrows.** §9 asks the parent
note to say so.

### 3.6 `const`, which C states and does not enforce

C's `const T*` is a statement about what *this function* promises not to do. It
is not a property of the memory, it is routinely cast away inside
implementations, and — decisively for this audience — its *absence* means
nothing at all, because Fortran has no `const` and every C interface derived from
a Fortran one is `double *` throughout. LAPACKE's `_work` forms pass read-only
arrays as non-const pointers for exactly this reason.

So the mapping onto `borrowed` / `mutable borrowed` is asymmetric:

| Header says | What it licenses |
|---|---|
| `const T *` | A *reasonable default proposal* that this is `borrowed T` or `ffi.Span of T` |
| `T *` | **Nothing.** Not "mutable", not "in-out". The parameter may be read-only |

> **Decision 4.** `bindgen` emits `ffi.Pointer of T` for every pointer regardless
> of `const`, per `ffi-c-boundary.md` §7.1's conservatism, and records the `const`
> qualifier in the sidecar as the generator's *proposal* for a human promoting
> the parameter to `borrowed`. Promotion is always a human edit.
>
> **Rejected: map `const T*` to `borrowed T` and `T*` to `mutable borrowed T`
> automatically.** This is what makes a generated binding look usable, and it is
> unsound in both directions. In one direction it marks LAPACK's read-only arrays
> as exclusive, which is not merely imprecise: §2.2 lowers `mutable borrowed`
> with LLVM `noalias`, so an automatic promotion would emit an aliasing claim on
> the strength of a keyword the header did not write. In the other direction, C
> code casts `const` away, and a `borrowed` that the callee writes through is
> undefined behaviour with no diagnostic anywhere.
>
> **Cost.** Generated bindings are entirely `ffi.Pointer`, so every generated
> declaration is `unsafe`-only and the safe layer is 100% hand-written. §7.1
> accepted this cost already; this decision is the reason it cannot be walked
> back later by "just using const".

### 3.7 Opaque handles, and transparent structs whose layout is a rumour

`ffi-c-boundary.md` §2.3 handles opaque handles completely: `gzFile`, `hid_t`,
`PyThreadState *`, `crypto_*` contexts. Pointer or integer, never dereferenced,
destructor known, Case B, `implements Drop`. The audit found no opaque handle in
any of the five libraries that the mechanism does not cover.

The dangerous case is the other one: **a struct whose fields are visible in the
header and whose layout is not a promise.** Binding it is possible, the compiler
accepts it, the linker accepts it, and it is wrong the day the library is
rebuilt. Four examples from the audit, and they are not the same:

| Struct | Layout status | Warrant |
|---|---|---|
| `z_stream` | Documented ABI, unchanged since 1995 | **documented** |
| `H5O_info2_t` | Grew between 1.10 and 1.12; the library kept both entry points and versioned the type name | **versioned** |
| `crypto_generichash_state` | Opaque by policy; size available only from `crypto_generichash_statebytes()` | **probed** |
| `PyObject`, `PyTypeObject` | Changes every minor version; `ob_refcnt` becomes a union under free-threading; abi3 forbids access | **none — do not bind** |

> **Decision 5.** A generated `ffi.CLayout` record over a foreign struct carries a
> **layout warrant** in the sidecar, one of `documented`, `versioned`, `probed`,
> or `none`. A struct with warrant `none` is not emitted as a record; it is
> emitted as `ffi.OpaqueHandle` and its fields are reached only through exported
> accessor functions. `bindgen --check` reports a struct whose warrant is `none`
> and which a checked-in binding nonetheless treats as a record, as `SC0497`.
>
> **Rejected: make the warrant a clause in the `extern` grammar.** It was drafted
> as `ZStream implements ffi.CLayout with layout documented`. Rejected on
> `native-dependencies.md` §10's precedent, which explicitly declined to add a
> clause to a sibling's grammar to carry information a sidecar already has. The
> warrant is generator metadata; it does not change what the compiler emits.
>
> **Rejected: treat every foreign struct as opaque.** Safe, and it deletes zlib —
> `z_stream` must be caller-allocated and its `next_in`/`avail_in` fields must be
> written by the caller between `deflate` calls. There is no functional interface
> to it. Opaque-by-default would make zlib's API unusable in order to protect
> against CPython's, and the two cases differ by a fact that is knowable.
>
> **Cost.** Somebody must supply the warrant, once per struct, and getting it
> wrong produces exactly the silent corruption the warrant exists to prevent. It
> is a human judgement written in a file, which is the same kind of thing as the
> `borrowed` in an `extern` declaration (§2.7.1) and is defensible for the same
> reason: it is written down where a reviewer can find it.

### 3.8 C++, and what `data-io.md` §9's guarantee costs

`data-io.md` §9 makes a hard promise that three sibling notes lean on: the base
toolchain builds with **no C++ compiler present**. `native-dependencies.md` §2.3
and §3.1 both cite it by name when rejecting source-first provisioning. It is not
this note's to weaken, and this note does not weaken it. It does price it.

**C++ libraries divide into three, and only the first is reachable.**

**One: a C ABI the library itself publishes and versions.** `data-io.md` §9's
Arrow binding is the model and its description is the specification of the whole
category:

> It is to the **Arrow C Data Interface** and **C Stream Interface** — three
> plain C structs (`ArrowSchema`, `ArrowArray`, `ArrowArrayStream`) with a
> release callback, a published stable ABI, and **no C++ symbols crossing the
> boundary.**

Others in the same shape: ONNX Runtime's `OrtApi` (a struct of function
pointers, versioned, C), oneDNN's C API, gRPC's C core, libtorch's limited C
surface. These are fully reachable and the C++ inside is irrelevant — what the
binding sees is C.

**Two: a C++ library with no C façade.** Eigen, OpenCV's modern API, TensorRT,
Ceres, dlib, ITK/VTK, Trilinos, ROOT, Protobuf, Abseil. Reachable only through a
façade somebody writes in C++ and compiles with a C++ compiler.

**Three: header-only C++ templates.** Eigen again, Boost.Math, xtensor, Thrust,
CUB. **There is no object file.** The code does not exist until a C++ compiler
instantiates it at a concrete type. This is the hardest floor in the entire
audit: not "needs a shim" but "needs a C++ compiler and a decision about which
instantiations, forever". Nothing a binding generator does reaches it.

> **Decision 6.** Science binds a C++ library only through a C ABI **the library
> itself publishes and versions**. Science does not generate, ship, or maintain
> an `extern "C"` façade over a C++ library, and `bindgen` refuses a header whose
> declarations are not `extern "C"`-linkable (`SC0498`). A mangled C++ symbol in
> a `symbol "…"` string is the same diagnostic.
>
> **Rejected: generate a façade with a C++-aware libclang pass.** It is what SWIG
> and cxx do and it is genuinely capable. Rejected for three reasons, in
> increasing weight. It makes a C++ compiler a requirement wherever it is used,
> against `data-io.md` §9. It requires Science to model C++ semantics it has no
> reason to model — overload sets, default arguments, templates, exceptions
> crossing the boundary, which `ffi-c-boundary.md` §4.5's abort-only panic model
> has no answer to. And the façade is *Science's* code wrapping *their* library,
> so every upstream release is Science's maintenance burden, forever, per
> library.
>
> **Cost, stated in libraries.** Science reaches essentially the whole numerical
> C stack — BLAS, LAPACK, FFTW, SuiteSparse, HDF5, NetCDF, GSL, SUNDIALS, MPI,
> PETSc, CUDA, cuBLAS, cuDNN, libsodium, zlib, zstd, SQLite, libcurl, OpenSSL —
> plus everything in category one. It does not reach Eigen, OpenCV, TensorRT,
> libtorch's C++ API, Ceres, or anything header-only. For Science's stated
> audience that is a good trade, because the second list is the "modern C++
> scientific computing" niche and the first list is the field. For a user coming
> from C++ it will not feel like a good trade, and the honest framing is that
> **Science's C++ story is "use a library that publishes a C ABI", which is a
> real answer and a narrow one.**

---

## 4. What raises coverage, ranked by what it buys

Four mechanisms. They are presented in yield order because the decision that
matters is the ordering, not the inventory.

| Mechanism | Buys | Needs a C compiler? | Cost |
|---|---|---|---|
| §4.2 An `extern` global item | **+7.6%** (and 17.7% more via the complex type, §4.1) | No | Four lines of grammar in a sibling's note |
| §4.5 Prefer the vendor's own shim | large, unquantifiable | No | A sidecar convention and a habit |
| §4.3 A macro rewriter in `bindgen` | **+5.1%** | No | A C expression evaluator with a small grammar |
| §4.4 A generated C shim | **+2.7%** | **Yes** | A C toolchain in a path that had none |

### 4.1 The four `ffi` additions that need no tooling and buy 26.4%

Category B — 1,192 entities, 22.6% — plus HDF5's 200 M2 macros in category C
(3.8%) is not blocked on any machinery. It is blocked on four decisions Science has not made, three of which the parent note
already assumes. They are listed here and asked for in §9:

1. **`ffi.Complex32` / `ffi.Complex64`** as two-field `ffi.CLayout` records. Buys
   the 930 complex LAPACK and CBLAS routines. `ffi-c-boundary.md` §2.4 already
   uses the name.
2. **An exported-global declaration item** — §4.2 below. Buys HDF5's 200 type and
   property constants and CPython's 199 globals.
3. **An alignment statement on `ffi.CLayout`.** Buys libsodium's
   `CRYPTO_ALIGN(64)` states.
4. **An `ffi.Uninitialized` with a runtime size.** Buys libsodium's
   `_statebytes()` idiom and C's flexible array members.

### 4.2 Decision 7 — `extern` blocks may declare an exported global

> **Decision 7.** `ffi-c-boundary.md` §1.2's list of items an `extern` block may
> contain gains a fourth:
>
> | Item | Form | Meaning |
> |---|---|---|
> | Global | `static NAME: T` | An exported data symbol, resolved at link |
>
> `static` is already reserved and `ffi-c-boundary.md` §1.1 already uses it in
> `from static`. `T` must be FFI-representable by §1.4's transitive rule.
> **Reading or writing an `extern static` requires an `unsafe` block**, and it is
> added to §3.1's closed list of powers as a seventh entry — because the memory
> belongs to C, may be uninitialized until an initialization function has run,
> and may be mutated by C between two reads with nothing in Science that models
> it.

```science
unsafe extern "C" library "hdf5":
    type Hid is I64
    type Herr is I32

    def H5open() -> Herr

    # `H5T_NATIVE_DOUBLE` is not a constant. The header defines it as
    #   (H5open(), H5T_NATIVE_DOUBLE_g)
    # and this is the global half of that expression.
    static H5T_NATIVE_DOUBLE_g: Hid
    static H5T_NATIVE_INT_g: Hid
    static H5P_CLS_FILE_ACCESS_ID_g: Hid
```

The Science half restores the macro's meaning — the `H5open()` call is the reason
the macro exists, and it is an ordinary initialization obligation of the kind
`ffi-c-boundary.md` §2.7(8) already catalogues:

```science
type H5Types:
    native_double: Hid
    native_int: Hid

H5Types has:
    def get() -> (H5Types, Error?):
        unsafe:
            if H5open() < 0:
                return (H5Types(native_double: -1, native_int: -1), H5Error.from_stack())
            return (H5Types(native_double: H5T_NATIVE_DOUBLE_g,
                            native_int: H5T_NATIVE_INT_g), null)
```

and CPython's, which is the case that is not optional:

```science
unsafe extern "C" library "python3":
    def Py_IncRef(object: ffi.Pointer of PyObject)
    def Py_DecRef(object: ffi.Pointer of PyObject)
    def PyErr_SetString(kind: ffi.Pointer of PyObject, message: ffi.CStr)

    static _Py_NoneStruct: PyObject
    static PyExc_TypeError: ffi.Pointer of PyObject
    static PyExc_ValueError: ffi.Pointer of PyObject
```

**Rejected: emit a generated accessor function per global, in a C shim.** It
works — one `hid_t science_H5T_NATIVE_DOUBLE(void) { return H5T_NATIVE_DOUBLE; }`
per constant — and it is what a binding tool with no language support does. It is
rejected because it puts a C compiler on the critical path of the two libraries
that most need this, to express something the linker already does natively, and
because 400 generated accessor functions is a worse artefact than four lines of
grammar.

**Rejected: a `const NAME be extern_global(...)` library function.** Rejected
because a global is a *link-time* entity and `const` in §1.2 means a compile-time
literal. Conflating them would make the constant item mean two different things
and would put an address where a value is expected.

**Cost.** A seventh power in §3.1's closed list, which that section's own
argument says should stay short — and this note is the first thing to lengthen
it. The justification is that the power being added is exactly as unverifiable as
the six already there and for the same reason: the memory is C's. A smaller cost:
`extern static` gives Science a way to *write* to a C global, which nothing in
the audit needs and which is the obvious source of future trouble. A read-only
form was considered and rejected because `errno`-style APIs and CPython's
`Py_OptimizeFlag`-style settings are written by callers, and a read-only item
would send those back to a shim.

### 4.3 Decision 8 — the macro rewriter lives in `bindgen`, not in the compiler

> **Decision 8.** Macro classification (§3.1) and M1/M2 rewriting happen in
> `sciencec bindgen` and emit ordinary checked-in Science source. The compiler
> has no knowledge of macros, of the preprocessor, or that a declaration came
> from one.

This is `ffi-c-boundary.md` §7.1's decision applied to the one part of C the
generator cannot read off the header directly, and it inherits §7.1's three
reasons unchanged. It is stated separately because §7.2 left the impression that
macros would be handled by hand-written supplements, and a rewriter is a
different artefact with a different maintenance cost.

**Rejected: a macro-expansion step in the compiler for `extern` blocks only.**
Rejected on §7.1's reason two verbatim — the salsa query graph would have to model
the preprocessor, tracking every transitively included file and every `-D`.

**Cost.** A C expression evaluator, and a hard boundary where it stops. The
boundary must be visible: `SC0495` names the macro, the header line, the full
expansion, and the construct that defeated the evaluator, so that supplying the
binding by hand in the sidecar is a one-line edit. A generator that refuses
quietly is a generator whose output silently lacks the function a user needs.

### 4.4 Decision 9 — the C shim is generated once, checked in, and built as a `vendored` native dependency

This is the mechanism every other language's binding tooling converges on, and
the audit says it is worth **2.7%** — the M4 class and the un-warranted third of
M3. Small, and not zero: it contains `PyList_GET_ITEM`, `errno` as an lvalue,
and every OpenGL-shaped API Science has not yet been asked for.

> **Decision 9.** A binding package may declare a shim. `sciencec bindgen` emits
> a single `.c` file per package that wraps each M4 entity in a real function with
> a `science_shim_` prefix. **The `.c` file is checked into the repository beside
> the generated `.science` source**, carrying the same provenance header §7.2
> requires. It is built by the toolchain as a `vendored` native dependency in the
> sense of `native-dependencies.md` §3.1 — "no network, a C compiler only" — and
> so inherits that provider's cache key, its Tier A content-addressed lock entry
> (§5.1) and its provenance record (Decision 9 of that note).

```c
/* generated by sciencec bindgen 0.1.0
 * from /usr/include/python3.14/listobject.h, Python 3.14.0, Py_LIMITED_API undefined
 * DO NOT EDIT — regenerate with `sciencec bindgen --sidecar python.toml` */
#include <Python.h>

PyObject *science_shim_PyList_GET_ITEM(PyObject *list, Py_ssize_t index) {
    return PyList_GET_ITEM(list, index);
}
```

**The shim `#include`s the library's header, and that is the decision.** Two
forms were considered.

**Rejected: a header-free shim**, in which the generator expands the macro and
emits a `.c` that re-declares only what it needs, so that no headers are required
on the user's machine. It is attractive — the `system` provider currently needs
only the shared object, and requiring a `-dev` package is a new burden. It is
rejected because it **freezes the macro's expansion at generation time**, and a
macro is the mechanism a C library uses to change something without changing its
symbols. A frozen expansion is an undetectable wrong answer of exactly the class
`ffi-c-boundary.md` §1.6 spends a section preventing. The header-including form
is at worst a build failure.

**Rejected: build the shim per user, from the headers, at every build.** This is
the same thing as "the compiler reads headers", arriving through the side door,
and §7.1 rejected it for reproducibility: compilation output would depend on
`/usr/include`. The checked-in `.c` is the artefact; the user's C compiler
compiles a file the project reviewed, not a file the user's headers generated.

**Rejected: ship the shim as a prebuilt object or library.** It would remove the
C compiler requirement entirely, and it is rejected because a prebuilt shim
compiled against header version X and linked against library version Y is the
version-drift hazard of `ffi-c-boundary.md` §5.5 hidden inside Science's own
object file, where no probe can see it. Compiling the shim locally is what keeps
the shim and the library in agreement.

**So: generated once and checked in, compiled per user.** The `.c` is a reviewed
project artefact; the object file is local.

**Costs, and they are real.**

- **A C compiler becomes required for any program that depends on a shimmed
  package.** `data-io.md` §9's guarantee is about **C++** and is untouched; this
  is C, and `native-dependencies.md` §3.1 already priced a C-compiler-only
  requirement for the `vendored` provider (`miniz`, PocketFFT). But the default
  path today needs no compiler at all, and a shim on a widely-used package would
  change that for many users. **Therefore: a shim is opt-in per binding package,
  never the generator's default, and a package that can reach its API without one
  must not have one.** `SC0493` when a shimmed package is in the graph and no C
  compiler is found, naming the package and the entities that need it.
- **The library's development headers become a build requirement**, not just its
  shared object. On a cluster `module load hdf5` supplies both; on a
  runtime-only container it does not.
- **`bindgen` grows a C-compiler dependency of its own**, at generation time. This
  is cheap — generation already requires headers, so it already requires a
  development environment — but it means `bindgen` cannot run in the minimal
  environment `sciencec` targets, and that should be said in its documentation
  rather than discovered.
- **Shim drift.** The shim records the header version it was generated from;
  `native-dependencies.md` Decision 7's startup probe compares the library's
  self-reported version against the declaration. `SC0494` extends that comparison
  to the shim's recorded header version, because a shim is the one place where a
  version mismatch is invisible to both the linker and the probe.

**Is it worth it?** For the five libraries audited: **no, not yet.** Of the 612
non-symbol entities, roughly 141 need a shim, and every one of those 141 is in
CPython, and every one of *those* disappears under the limited API that
`python-interop.md` §3.2 has already chosen. **The shim should be specified now,
built later, and the first package that genuinely needs it should be the thing
that justifies building it.** §4.2 and §4.3 come first and they are not close.

### 4.5 Decision 10 — prefer the vendor's own symbol-level form, always

The strongest finding in the audit is that this problem has been solved four
times already, by the library authors, because every other language's FFI hit it
first.

| Library | The macro | The vendor's symbol-level form |
|---|---|---|
| CPython | `Py_INCREF`, `Py_TYPE` | `Py_IncRef`, `PyAPI_FUNC(...) Py_TYPE(...)`, under `Py_LIMITED_API` |
| libsodium | `crypto_box_PUBLICKEYBYTES` | `crypto_box_publickeybytes()` |
| libsodium | `sizeof(crypto_generichash_state)` | `crypto_generichash_statebytes()` |
| CBLAS | complex-returning `cdotc` | `cblas_cdotc_sub`, with an out-parameter |
| HDF5 | `H5T_NATIVE_DOUBLE` | the exported global `H5T_NATIVE_DOUBLE_g` |

> **Decision 10.** Where a library exports a symbol equivalent to a macro or a
> `static inline` function, `bindgen` binds the **symbol** and records in the
> sidecar that it did and why. The generated file's header names the ABI mode it
> was generated under. Preferring the macro over an available symbol requires an
> explicit sidecar entry.

This generalizes `ffi-c-boundary.md` §7.3, which says to prefer any
machine-readable interface description the vendor ships over the C header —
LAPACK's Fortran `INTENT` annotations being the example. The audit adds the
symbol-level half of the same rule: **prefer any symbol-level form the vendor
ships over the macro form**, for the same underlying reason, which is that the
vendor knows something the header's syntax has thrown away.

**Rejected: prefer the macro, since it is what C programmers write and it is
faster.** The speed argument is real — `Py_INCREF` inlined is one instruction and
`Py_IncRef` is a call — and it is rejected because Science cannot inline across
the boundary in any case (§5.5 rules out cross-language LTO), so the inline form
buys nothing Science can collect. The familiarity argument is answered by the
safe wrapper, which is where a Science user reads the name anyway.

**Cost.** The binding's performance is the symbol-level form's, which for
refcounting in a hot Python loop is measurably worse than a C extension's. That
is a cost `python-interop.md` should know it is paying, and §9 asks for it to say
so.

---

## 5. The ceiling, honestly

### 5.1 The number, and what it covers

For the five libraries audited — all C, all with stable ABIs, all chosen because
Science's own design already commits to them:

| | Share |
|---|---|
| Directly bindable today | **64.9%** |
| Bindable with four `ffi` additions and no new tooling | **91.3%** |
| Bindable with a macro rewriter in `bindgen` | **~96.5%** |
| Bindable with a generated C shim | **99.2%** |
| **Irreducible (variadics)** | **0.84%** |

### 5.2 What that number does not cover, which is most of C

The five libraries are the best case and were chosen to be. "All C code" is a
larger set and the number falls for four reasons, none of which the audit's
method can see:

- **Header-only C++ presented as a C-adjacent library.** Zero percent reachable,
  forever (§3.8). Not a residue, a floor.
- **Libraries whose entire idiom is macros.** OpenGL through GLEW, Xlib, GTK's
  `G_OBJECT` cast chain, the Linux UAPI `ioctl` encoding macros. These are 60–90%
  M3 and M4, not 12%, and for them the shim is not a 2.7% improvement but the
  whole binding.
- **Libraries with no stable ABI**, which are bindable and should not be bound —
  BoringSSL says so in its own documentation, and `stdlib-standard.md` §13.1
  rejects it for that reason.
- **`setjmp`/`longjmp` APIs**, which `ffi-c-boundary.md` §2.7(6) already lists as
  forbidden by convention because a `longjmp` past a Science frame skips every
  destructor between here and there. libpng's default error handling is this.
  MINPACK-style handlers are this.

**A defensible figure for "all C": roughly 95% of the exported symbols of C
libraries with a stable ABI; 0% of header-only C++; and, weighted across what a
scientist actually reaches for, about 90% after shims and about 75% without.**

### 5.3 The ceiling that actually binds, which is not this one

Every number above counts entities that can be *declared*. Under
`ffi-c-boundary.md` §7.1 a generated declaration is maximally conservative:
`unsafe extern`, every pointer an `ffi.Pointer`, no `borrowed`, no `Drop`, no
`from` clause. That is the right decision and it means:

> **A 99% reachable library is 99% callable from inside an `unsafe` block and 0%
> callable from safe Science.**

The fraction that is usable is the fraction with a hand-written safe wrapper, and
§7.1 supplies the observed ratio itself: for cuDNN, 1,500 generated declarations
support about 60 wrappers. **4%.**

What a wrapper contains is worth one example, because it is the thing that does
not generate. Every line below is a decision a header cannot state: which
`interface` the type joins, what the error type is, that the state is owned and
destroyed exactly once, and that the size came from a runtime call:

```science
interface Hash:
    def update(mutable self, bytes: borrowed Array of U8)
    def finish(mutable self) -> Array of U8

type Blake2b:
    state: ffi.CBuffer of U8      # size from crypto_generichash_statebytes()

Blake2b implements Drop:
    def drop(mutable self):
        unsafe: sodium_memzero(self.state.span_mut(), self.state.length())

Blake2b has:
    def new(digest_length: Int, key: (borrowed Array of U8)?) -> (Blake2b?, Error?):
        if digest_length < 16 or digest_length > 64:
            return (null, CryptoError.DigestLength(digest_length))
        let size be unsafe: crypto_generichash_statebytes()
        let mutable state be ffi.CBuffer of U8 .allocate(size as Int)
        let status be unsafe:
            crypto_generichash_init(state.span_mut(), key_span(key), key_length(key),
                                    digest_length as CSizeT)
        if status is not 0:
            return (null, CryptoError.Init)
        return (Blake2b(state: state), null)

Blake2b implements Hash:
    def update(mutable self, bytes: borrowed Array of U8):
        unsafe: crypto_generichash_update(self.state.span_mut(), bytes, bytes.length() as CULongLong)
```

That ratio is not a defect — §7.1's argument
is precisely that a program does not call 1,500 cuDNN functions, and the 60 are
the ones it calls. But it is the honest answer to "all C code is usable from
Science":

> **All of it is reachable. Almost none of it is *wrapped*, and wrapping is
> human work that does not scale with generation. The binding generator solves
> the problem whose size is the difficulty; it does not touch the problem whose
> difficulty is the difficulty, and that problem is the ceiling.**

The strategic consequence is worth stating because it is not obvious: **effort
spent raising the 65% toward 99% is worth much less than effort spent lowering
the cost of writing a safe wrapper.** Of the mechanisms in §4, only §4.2 and
§4.1's complex type unblock libraries that are currently at zero. The rest raise
a number that was not the binding constraint.

---

## 6. What this does to §7 of the parent note

§7 of `ffi-c-boundary.md` chose conservative out-of-band generation, and §7.2
listed three costs. The audit was run in part to check whether they were priced
right.

### 6.1 §7.1's decision is confirmed, and one of its reasons is understated

The three reasons — libclang as a compile-time dependency breaks reproducibility;
the salsa graph would have to model the preprocessor; headers do not carry the
ownership contract — all survive. Reason three survives with a strengthening:

> §7.1: `double *a` does not say whether it is read or written, whether it points
> at one element or a million, whether the callee retains it, or who frees it.

The audit adds that **headers do not carry the *entity* either, for 11.6% of the
surface.** An `extern` block is a list of symbols, and 612 of the 5,265 things a
C programmer writes are not symbols. §7.1's conclusion — generate the *shape*,
not the *contract* — is right, and the shape is a smaller fraction of the header
than §7 assumed.

Nothing in the audit argues for reopening the decision. §7.4's rejected runtime
FFI gains exactly one point: libffi handles variadics, which is the entire
irreducible residue. 0.84% does not overturn three reasons.

### 6.2 §7.2's second cost was underpriced, and should be split

§7.2's middle bullet reads:

> **`bindgen` must exist and be maintained**, including the parts of C that are
> hostile: `#define` constants that are expressions, anonymous structs, function
> macros (`H5T_NATIVE_DOUBLE` is a macro expanding to a global variable
> dereference, and no header parser recovers it as a constant). These need a
> per-library allowlist and some hand-written supplements, which is what every
> binding generator in every language ends up doing.

Three corrections, in order of weight.

**One: the proposed remedy does not fit the largest sub-problem.** The bullet's
own example, `H5T_NATIVE_DOUBLE`, is not fixable by an allowlist or a
hand-written supplement, because the thing that cannot be hand-written is the
*declaration*, not the value — §1.2 has no item for an exported global. An
allowlist can only list things the generator should skip or substitute; it cannot
supply a language feature that is missing. **§4.2 is the correction and it is a
grammar change, not a tooling change.**

**Two: the volume was not estimated, and it is the hottest 11.6%.** 612 entities,
including `Py_INCREF`, `Py_TYPE`, `Py_None`, every `PyExc_*`, and every HDF5
datatype. "Some hand-written supplements" describes a tail; this is not a tail.

**Three: it is not one problem.** §3.1's M1–M5 have five different answers, three
of which need no C compiler, one of which belongs to `native-dependencies.md`'s
variant machinery rather than to bindgen at all, and only one of which is the
shim the bullet implicitly anticipates. **§7.2's second bullet should be replaced
by a pointer to §3.1 and §4.**

### 6.3 §7.2's first and third costs were priced right

**"Generated source is checked in and must be regenerated on a version bump."**
Correct, and the mitigation — a provenance header naming tool version, library
version, header path and flags — is exactly right. The audit adds one required
field: **the ABI mode**, meaning the feature-test macros in force at generation
(`Py_LIMITED_API`, `Z_LARGE64`, `LAPACK_COMPLEX_STRUCTURE`, `INTERFACE64`).
Without it, a generated file does not record which of two incompatible libraries
it describes, which is §3.1's M5 class and `SC0497`.

**"Hand annotations must survive regeneration."** Correct, and the audit
sharpens the failure mode. §7.2 says a sidecar keyed by function name records
what headers do not carry. The audit adds three more things it must carry: the
macro classification (§3.1), the layout warrant (§3.7), and the const proposal
(§3.6). And it adds the mechanism that keeps it honest: **`bindgen --check`, which
regenerates against the current headers and reports every sidecar entry that no
longer matches** — a function whose signature changed, a macro whose expansion
changed, a struct whose warrant is now violated, an annotation naming a function
that no longer exists. `SC0496` and `SC0499`. §7.2 identifies losing annotations
as "the failure that makes binding generators get abandoned"; a check mode is
what turns that from a slow rot into a diff.

### 6.4 §7.3 generalizes

§7.3's rule — prefer any machine-readable interface description the vendor ships
over the C header — is confirmed and extended by §4.5 to symbol-level forms. §7.3
found one instance (LAPACK's Fortran `INTENT`); the audit found five more, in
four of the five libraries. It is not a special case for LAPACK; it is the normal
way a well-maintained C library accommodates the people binding it.

---

## 7. Diagnostics allocated

`README.md` allocates `SC0490`–`SC0499` to this note, inside the codegen range
the core spec's §9 fixes and inside the `SC0480`–`SC0499` block the README's
partition lists as free.

Several of these are rendered by `sciencec bindgen` rather than by the compiler.
That follows `native-dependencies.md` §9's precedent, where `SC0475` is "rendered
by `science-rt` before `main`" and the text is owned by the note so that it
matches the compile-time diagnostics. The `SC` namespace is the project's, not
the compiler's.

| Code | Rendered by | Meaning |
|---|---|---|
| `SC0490` | compiler | An `extern static` global is read or written outside an `unsafe` block (§4.2) |
| `SC0491` | compiler | An `extern static` is read from a library whose declared initialization function is never called on any path reaching the read (§4.2, HDF5's `H5open`). Warning |
| `SC0492` | compiler | An `ffi.CLayout` type requires an alignment stronger than its fields imply and none is stated (§4.1(3)) |
| `SC0493` | driver | A package in the graph declares a shim and no C compiler was found. Names the package and every entity that needs it (§4.4) |
| `SC0494` | `science-rt` | A shim's recorded header version disagrees with the resolved library's self-reported version (§4.4). Extends `native-dependencies.md` Decision 7 |
| `SC0495` | `bindgen` | An entity could not be bound. Names the category (A–D), the macro class (M1–M5), the header line, and the full expansion (§3.1, §3.3) |
| `SC0496` | `bindgen --check` | A macro's expansion, a function's signature, or a struct's layout changed since the checked-in binding was generated (§6.3) |
| `SC0497` | `bindgen --check` | A binding depends on a header-time build flag that the manifest does not declare as a variant, or on a struct whose layout warrant is `none` (§3.1 M5, §3.7). Warning |
| `SC0498` | `bindgen` | A header's declarations are not `extern "C"`-linkable, or a mangled C++ symbol appears in a `symbol "…"` string (§3.8) |
| `SC0499` | `bindgen --check` | A sidecar annotation names an entity the header no longer declares (§6.3) |

**Not allocated here, and asked for instead.** A variadic function named in a
hand-written `extern` block is an `extern`-declaration error and belongs in
`ffi-c-boundary.md` §8's own `SC0420`–`SC0459` range, where `SC0434` is free.
Taking a code from this note's block for an error in a sibling's grammar would
put the diagnostic in the wrong place for anyone reading the allocation table.
§9 asks for it.

---

## 8. Summary of decisions

| Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|
| 1. Classify macros into M1–M5, rewrite M1 and M2 automatically | A per-library allowlist (§7.2); or expand every macro with the preprocessor | M1+M2 are 70% of the residue and are the normal case, not exceptions; expansion fabricates constants from addresses | A C expression evaluator with a hard, visible boundary |
| 2. `static inline` uses the same M1–M5 machinery; prefer an exported twin | Always shim `static inline` | Mostly M1; shimming discards the vendor's own ABI-safe signal | Generation must record the ABI mode it ran under |
| 3. Variadics stay unreachable | A fixed-arity shim per call shape; libffi for variadics only | The shape set is unbounded and program-dependent; libffi is a second calling mechanism for 0.84% | No `printf` family. libffi is strictly more capable here and is still rejected |
| 4. `bindgen` emits `ffi.Pointer` regardless of `const`; `const` is a sidecar proposal | Map `const T*` → `borrowed`, `T*` → `mutable borrowed` | `T*` licenses nothing (Fortran-derived C has no `const`), and auto-promotion would emit `noalias` on a keyword the callee may cast away | Generated bindings are 100% `unsafe`; the safe layer is all hand-written |
| 5. A foreign struct carries a layout warrant: documented / versioned / probed / none | A clause in the `extern` grammar; or treat every struct as opaque | The warrant is generator metadata, not codegen input; opaque-by-default deletes zlib to protect against CPython | A human judgement per struct, written where a reviewer finds it |
| 6. Bind C++ only through a C ABI the library publishes and versions | Generate an `extern "C"` façade with a C++-aware pass | Needs a C++ compiler (`data-io.md` §9), needs C++ semantics Science does not model, and makes every upstream release Science's burden | Eigen, OpenCV, TensorRT, header-only C++ are out. Permanently |
| 7. `extern` blocks gain `static NAME: T` for exported globals | A generated accessor per global in a C shim; or a `const` form | 400 accessor functions and a C compiler, to express what the linker already does | A seventh power in §3.1's closed list, and a way to write to a C global that nothing needs |
| 8. Macro rewriting lives in `bindgen`, never in the compiler | A preprocessor step in the compiler for `extern` blocks | §7.1's reason two: the salsa graph would have to model the preprocessor | A tool boundary users will hit and must be told about clearly |
| 9. A C shim is generated once, checked in as `.c`, compiled per user as a `vendored` dependency | A header-free shim; a per-user generated shim; a prebuilt shim object | A frozen expansion is silently wrong; a per-user generated shim is "the compiler reads headers" by the side door; a prebuilt object hides version drift where no probe sees it | A C compiler and dev headers for any program using a shimmed package. Opt-in, and worth only 2.7% today |
| 10. Prefer the vendor's symbol-level form over the macro form | Prefer the macro, which is faster and familiar | Science cannot inline across the boundary (§5.5), so the inline form buys nothing it can collect | The binding runs at the call-based speed; `python-interop.md` should say so |

---

## 9. What this note asks of others

**Of `ffi-c-boundary.md`** — one grammar change, three library additions, three
corrections:

1. **§1.2: a fourth item, `static NAME: T`** (§4.2). This is the only grammar
   change asked for and it is the highest-yield item in the note: without it HDF5
   is unusable and `python-interop.md` cannot name `Py_None`.
2. **§3.1: a seventh power** — reading or writing an `extern static` requires
   `unsafe`.
3. **§1.3: `ffi.Complex32` and `ffi.Complex64`**, as two-field `ffi.CLayout`
   records. §2.4 of that note already uses `ffi.Complex64`, which its own closed
   scalar table does not define. Half of LAPACK is behind this.
4. **§1.4: an alignment statement on `ffi.CLayout`** (libsodium's
   `CRYPTO_ALIGN(64)`), and **§1.3: an `ffi.Uninitialized` constructible from a
   runtime byte count** (libsodium's `_statebytes()`, and C's flexible array
   members).
5. **§1.4's union claim is wrong and should be amended.** "No library in the
   target set needs a bitfield or a union in its public interface" — HDF5's
   `H5L_info2_t` and `H5R_ref_t` do, and CPython's `PyObject.ob_refcnt` does
   under free-threading. The bitfield half of the claim holds. The remedy §1.4
   already gives (an opaque byte array plus accessors) is correct; it should be
   stated as the normal path rather than as a case that does not arise.
6. **§4.4: extend the registered-callback rule to a trampoline stored in a
   foreign struct** (§3.5). `z_stream.zalloc` and `ArrowSchema.release` outlive
   the call that installed them, so a closure behind them may capture no borrows.
7. **§7.2's second bullet should be replaced** by a pointer to §3.1 and §4 of
   this note (§6.2), and **§7.2's provenance header should gain an ABI-mode
   field** recording the feature-test macros in force (§6.3).
8. **A diagnostic code for a variadic function in a hand-written `extern`
   block**, from §8's own `SC0420`–`SC0459` range; `SC0434` is free (§7).

**Of `python-interop.md`:**

9. **A paragraph saying why abi3 is the only target that works.** That note
   contains no occurrence of the word "macro", never mentions `Py_LIMITED_API`
   as a define, and commits in §4.4 and §5.5 to emitting `Py_INCREF`, `Py_DECREF`
   and `Py_BEGIN_ALLOW_THREADS` — one `static inline` function and two macros,
   none of which a generator emitting LLVM IR can call. Its §3.2 choice of abi3
   is what saves it, and it should say so, with the consequence: **refcounting
   goes through `Py_IncRef`/`Py_DecRef` as calls, which is measurably slower than
   a C extension's inlined increment.**
10. **`Py_BEGIN_ALLOW_THREADS` is a brace-opening macro in every API mode** and
    must be emitted as the pair `PyEval_SaveThread()` / `PyEval_RestoreThread()`
    (§2.5).
11. **A `[native.python3]` entry.** `native-dependencies.md` §3.3's table has no
    libpython row, so the Python boundary is currently outside that note's
    framework entirely.
12. **§9's claimed range should be narrowed to `SC0450`–`SC0459`**, per the
    README's authoritative partition, which that note predates.

**Of `native-dependencies.md`:**

13. **Variant vocabulary entries for the M5 macros the audit found** (§3.1):
    `Z_LARGE64` / `z_off_t` width for the `deflate` capability, and
    `LAPACK_COMPLEX_STRUCTURE` and the complex-return convention for `blas` and
    `lapack`. §4.4(2) already names the last of these as missing.
14. **A `shim` field on `[native.<name>]`** naming the generated `.c`, so that a
    shim is an ordinary `vendored` provider entry with a Tier A lock record and a
    provenance line (§4.4).
15. **Extend Decision 7's startup probe to the shim's recorded header version**
    (`SC0494`), because a shim is the one place a version mismatch is invisible
    to both the linker and the probe.

**Of `data-io.md`:** nothing that changes its text. §9's no-C++-compiler
guarantee is untouched by everything here, and §3.8 confirms its Arrow C Data
Interface choice as the model for the whole C++ category. One thing named so it
is not silent: **a shim needs a C compiler, which §9's guarantee does not
forbid** and which `native-dependencies.md` §3.1 already prices for the
`vendored` provider.

**Of `README.md`:** a row for this note in the index and in the allocation table,
claiming `SC0490`–`SC0499`. The README's convention is that a note adds its row
before writing; this note could not, because its brief forbade editing any file
but its own, and other notes were being edited at the same time. **The row is
missing and should be added by whoever lands this.**

---

## 10. Risks

**The numbers are soft, and only one library was counted.** CPython was measured
from real headers on the machine this note was written on; the other four were
estimated from published symbol inventories and documented header structure. The
aggregate inherits that. **No figure in this note should be quoted outside it
until `sciencec bindgen --classify` has been run against real headers for all
five.** The shape of the finding — that the residue is dominated by exported
globals and by one library's macro surface — does not depend on precision. The
percentages do, and they are the part people will quote.

**One version, one platform.** CPython 3.14, on Windows, in September 2026.
CPython's macro surface has been shrinking for a decade, deliberately; a count
against 3.11, which `python-interop.md` §3.2 sets as the minimum, would find more
macros and fewer functions. The direction of travel is favourable and the
measurement is a snapshot of a moving target.

**`extern static` lengthens a list whose value is its shortness.**
`ffi-c-boundary.md` §3.1 argues that `unsafe`'s meaning depends on the list of
extra powers being short, and this note is the first thing to add to it. The
justification — the new power is exactly as unverifiable as the six already
there — is good, and it is also exactly the justification the seventh, eighth and
ninth additions will use. Somebody should own that list's length.

**The shim is specified and not built, which is the state binding generators go
to die in.** §4.4 concludes that the shim is worth 2.7% and should wait. The risk
is that "specified, not built" becomes permanent, and the first library that
genuinely needs it — an OpenGL-shaped API, where the shim is not an improvement
but the whole binding — arrives as an emergency rather than as a planned build.
The mitigation is that Decision 9 fixes the *shape* now: a checked-in `.c`
compiled as a `vendored` dependency. Building it later is then work, not design.

**`bindgen --check` is where all of this actually lives, and it is the least
glamorous thing in the note.** Decisions 1, 4, 5 and 10 all produce sidecar
annotations, and §6.3 makes the case that annotations that do not survive
regeneration are what kills binding generators. Four of the ten diagnostics
allocated here are `--check` diagnostics. If `--check` is cut for schedule, the
sidecar rots silently and the generated bindings become wrong in exactly the way
that produces correct answers for small matrices.

**The 4% is the real number and this note spends most of its length on the other
one.** §5.3 is the honest answer to the question that was asked, and it is one
section out of ten. The risk is that "99% of C is reachable" is the sentence that
travels and "4% of it is wrapped" is the sentence that does not. If one line from
this note is quoted, it should be: **all of C is reachable; the wrappers are the
work, and the wrappers do not generate.**
