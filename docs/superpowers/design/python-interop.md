# Science — Python interoperability

Date: 2026-09-16
Status: draft
Phase: F2 (per the F0 spec §2 table), with one F0 obligation called out in §11
Depends on: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`, in
particular §8's commitment that the tensor layout is DLPack-compatible, §6 on
ownership and regions, and §5.5 on `Option` and `Result`.

---

## 1. The premise, and the one sentence this note is built on

Nobody adopts a scientific language whole. They adopt it one hot function at a
time, from inside the Python program they already have. Julia's decade-long
stall was largely that you had to leave Python to get anything back; Mojo's
central adoption lever is that you do not. So the direction that matters is
**Science called from Python**, and the measure of success is blunt: a person
with a working NumPy program can replace one function with a Science one in an
afternoon, keep everything else, and get the speed without a copy.

Everything below follows from one idea:

> **The boundary is where every static guarantee becomes exactly one dynamic
> check.** Types become an argument conversion. Shapes become a single
> dimension check at the door. Regions become a reference count plus an export
> counter. Effects become a decision about the GIL. `Result` becomes a raised
> exception. Inside the Science function, nothing is checked at runtime that the
> compiler already proved; outside it, Python's rules apply. The membrane is
> thin, it is generated, and the user does not write it.

Here is that mapping in one table, which is also the table of contents.

| Science-side guarantee | What it becomes at the boundary | Section |
|---|---|---|
| Static types in the signature | One conversion per argument, `TypeError` on failure | §3.4 |
| Static shapes | One dimension check per shape variable, `ValueError` on failure | §3.4, §8 |
| `Result of (T, E)` | Return `T`, or raise a generated exception class | §3.5 |
| `Option of T` | Return `T`, or `None` | §3.5 |
| Ownership of a buffer | A DLPack deleter that frees through the right owner | §4.4 |
| A borrow's region | A strong reference to the Python object that roots it | §4.5 |
| Exclusive borrow (`mutable borrowed`) | An export counter checked at the membrane | §4.5 |
| "This function does not touch Python" | Release the GIL around the call | §5.2 |
| `panic` | Unwind to the shim, raise `SciencePanic` | §7.2 |

---

## 2. Scope and non-goals

In scope: compiling a `.science` file into an importable CPython extension
module; the signature mapping; zero-copy array exchange in both directions via
DLPack and the buffer protocol; GIL policy including free-threaded builds;
errors and tracebacks in both directions; and a narrowly scoped answer on
calling Python from Science.

Not in scope here, and owned by sibling notes: the tensor type itself and its
shape language, the effect system's general design, GPU targets and device
placement policy, the package manager. Where this note needs something from
them it states the requirement and marks it as a dependency rather than
designing it. Tensor syntax in the examples is **indicative**; what interop
requires of the tensor design is only two things, stated once here:

1. A tensor value's runtime header is field-for-field a DLPack `DLTensor` —
   data pointer, device kind and id, rank, dtype, shape, strides, byte offset —
   as §8 of the F0 spec already commits.
2. Shape variables that are not compile-time constants are recoverable at
   runtime from that header, so the membrane can check them once.

Other languages and other runtimes (R, Julia, MATLAB, the JVM) are out of
scope. C ABI export is a separate and much simpler problem; this note assumes
it exists underneath, because the Python shim is written against it.

---

## 3. Science called from Python

### 3.1 Marking an export

A function becomes visible to Python by being declared so:

```science
public to python
function standardize(batch: borrowed Tensor of (F32, (rows, 768))) returns ...
```

`public to python` is a phrase, in the style of §4.3's multi-word keywords: the
lexer emits three tokens and the parser recognizes the sequence at item
position. Neither `to` nor `python` becomes a globally reserved word. This is
safe because after `public` at item position the only legal continuations are
`function`, `type`, `choice`, `trait`, `const` and `use`; an identifier there
cannot be anything else, so recognition is unambiguous without reservation.

The F0 spec reserved `extern` for something like this and I am deliberately not
using it, because `extern` reads as *"this thing is defined elsewhere"* — the
wrong direction, and exactly the direction §6 needs the word for. Rust carries
that ambiguity (`extern fn` exports, `extern { fn }` imports) and it confuses
people permanently. Two different spellings for two different directions is
worth one extra parser rule.

Why a language-level marker rather than a manifest listing exported names: the
signature has to be *checked* for exportability (§3.4), the check has to
produce a diagnostic with a span, and the span has to point at the function. A
manifest entry has no span into the code that is wrong.

Types and constants may also be exported:

```science
public to python type Calibration:
    offset: F32
    gain: F32

public to python const CHANNELS be 768
```

### 3.2 The build

```
$ sciencec build --python
   compiling spectra (3 files)
   emitting  spectra.cp312-win_amd64.pyd
   emitting  spectra.pyi
   emitting  spectra-0.1.0-cp311-abi3-win_amd64.whl
```

Three artifacts, and all three matter:

- **The extension module.** A native shared library (`.pyd` on Windows, `.so`
  elsewhere) with a `PyInit_spectra` entry point.
- **A type stub (`.pyi`).** Non-negotiable for adoption. Without it, editors
  show nothing, mypy sees `Any`, and the module feels like a black box. The
  stub is generated from the same signatures the shim is generated from, so it
  cannot drift.
- **A wheel.** `pip install .` must work. The toolchain ships a PEP 517 build
  backend (`science.build`) so a `pyproject.toml` of six lines is the whole
  packaging story:

```toml
[build-system]
requires = ["science-build>=0.1"]
build-backend = "science.build"

[project]
name = "spectra"
version = "0.1.0"
requires-python = ">=3.11"
```

Honest cost: **a source build requires the Science toolchain on the machine
doing the build.** That is the same deal maturin/PyO3 offers, and the same deal
every non-pure-Python package offers. The mitigation is the same too: publish
binary wheels, built in CI across platforms, and almost nobody ever builds from
source. The build backend downloads a pinned toolchain when one is absent
rather than failing with a compiler-not-found error, because a failed
`pip install` is where adoption dies.

**Minimum CPython is 3.11.** That buys PEP 678 exception notes (§7.1), decent
default error messages, and a stable enough C API surface. It costs users on
3.9 and 3.10, which in 2026 is an acceptable trade for a new language.

**ABI.** The default build targets the **stable ABI (abi3)**, tagged
`cp311-abi3`, so one wheel per platform covers every 3.11+ interpreter. The
practical consequences are real constraints on the code generator, not
incidental:

- No direct access to CPython struct fields; types are created with
  `PyType_FromSpec` / `PyType_FromModuleAndSpec`, not static struct literals.
- Module state goes in per-module state (`PyModule_GetState`), not in C
  globals — which is also what §5.6 needs for subinterpreters.
- On Windows we link `python3.dll`, the stable-ABI forwarder, which is exactly
  one link for all versions. On Linux we link nothing and let the interpreter
  resolve the symbols. On macOS the same, with `-undefined dynamic_lookup`.
- `METH_FASTCALL` (vectorcall) is only in the limited API on recent versions.
  The generator uses it when the target permits and falls back to
  `METH_VARARGS | METH_KEYWORDS` otherwise; this is a build-time selection, not
  a runtime one.

Free-threaded builds need their own wheel and their own tag (`cp313t`,
`cp314t`); see §5.5. `sciencec build --python --free-threaded` produces it, and
CI builds both.

### 3.3 The generated module

Module initialization is **multi-phase (PEP 489)**, with these slots:

- `Py_mod_exec` — creates the wrapper types, installs the exception classes,
  registers the functions. Nothing else. In particular the module **imports
  nothing** at init, and has no Python dependencies at all: not NumPy, not
  anything. It speaks DLPack and the buffer protocol, which is what NumPy,
  PyTorch, JAX, CuPy and MLX all speak, so it works identically in a torch-only
  environment and in a NumPy-only one. This is the single most important
  packaging decision in this note.
- `Py_mod_multiple_interpreters` — set to *not supported* in the first version
  (§5.6).
- `Py_mod_gil` — set to `Py_MOD_GIL_NOT_USED` only for free-threaded builds we
  have actually made thread-safe (§5.5).

One `.science` file maps to one module of the same name. A directory module
(`mod.science` plus siblings, per F0 §4.4) maps to one extension module for the
whole package, with submodules created in `Py_mod_exec`, rather than one shared
library per file. One library per file would multiply load time, duplicate the
runtime, and turn a package into a pile of `.so` files.

**If two independently built Science extension modules are loaded into one
process**, each has its own statically linked `science-rt`, its own wrapper
classes, and its own exception hierarchy. A tensor produced by module A and
passed to module B therefore crosses as if it came from NumPy: **through
DLPack, never through a private pointer.** That rule is what makes a shared
runtime unnecessary, and it is why a buffer allocated by A can only ever be
freed by A's deleter, which travels with the capsule.

### 3.4 Signature mapping

A Science function becomes a Python function whose parameters are
positional-or-keyword, in declaration order, with the declared names. F0 has no
default arguments and no varargs, so neither appears; when defaults arrive they
map directly.

| Science | Python argument accepted | Python value returned |
|---|---|---|
| `I8`…`I64`, `U8`…`U64` | anything with `__index__` (so NumPy integers work); range-checked | `int` |
| `F16`, `BF16`, `F32`, `F64` | anything with `__float__` | `float` |
| `Bool` | `bool` only | `bool` |
| `Char` | `str` of length 1 | `str` |
| `String` (owned) | `str` — copied, UTF-8 encoded | `str` — copied |
| `borrowed String` | `str` — **not copied**, see below | — |
| `()` | — | `None` |
| `(A, B, …)` tuple | `tuple` | `tuple` |
| `Array of T` | `list`/any sequence — **copied**, element-wise | `list` — copied |
| `Tensor of (…)` | anything with `__dlpack__` or a buffer — **not copied** (§4) | wrapper object — not copied |
| `Option of T` | `T` or `None` | `T` or `None` |
| `Result of (T, E)` | *not accepted as an argument* | `T`, or raise |
| exported `type` | an instance of the generated wrapper class | the wrapper class |
| exported `choice` | the generated variant classes | the matching variant |
| anything else | rejected at compile time, `SC0450` | |

Notes that are not incidental:

- **Integers go through `__index__`, not `__int__`.** That accepts `np.int64`
  and rejects `3.7` silently truncating, which is the behavior a scientific
  audience should get. Out of range raises `OverflowError`.
- **`bool` is rejected for integer parameters** even though Python's `bool`
  subclasses `int`. Passing `True` where a count is expected is almost always a
  bug, and the error message is cheap.
- **Floats narrow silently.** Python's `float` is a double; an `F32` parameter
  loses precision. Refusing would be absurd; the stub says `float` and the
  documentation says so once.
- **`borrowed String` does not copy.** `PyUnicode_AsUTF8AndSize` returns a
  buffer cached on the `str` object, valid as long as the object is alive, and
  the shim holds a strong reference for the duration of the call. An *owned*
  `String` parameter copies, because Science will own it afterwards. This is
  the same owned-versus-borrowed distinction the language already has, and it
  has a visible performance meaning at the boundary — a nice accident, not a
  designed one.
- **`Array of T` copies, and that is a trap.** A Python `list` is a vector of
  pointers to boxed objects; there is no representation Science and Python
  share. Passing a million-element list costs a million conversions. The
  generated documentation says so, and the compiler emits a *warning* on an
  exported `Array of T` parameter where `T` is numeric, naming `Tensor` as the
  fix. Warning, not error: small lists are fine and common.
- **Shape variables become one check each.** For
  `batch: borrowed Tensor of (F32, (rows, 768))`, the shim checks that rank is
  2 and extent 1 is 768, and binds `rows` from extent 0. If a second parameter
  also mentions `rows`, the shim checks they agree. Failures raise `ValueError`
  with both values named. This is the payoff of static shapes at the boundary:
  one check at the door, zero checks inside the loop.
- **Returning several values** returns a `tuple`, positionally.

### 3.5 `Result` and `Option` at the boundary

Python has exceptions and it has `None`, and pretending otherwise would produce
a module that feels foreign. So:

**`Result of (T, E)` returns `T` on `Ok` and raises on `Err`.** Not a Result
object, not a `(value, error)` pair. A Python caller writes `try:` because that
is what Python callers write.

The exception classes are generated from `E`:

- The module defines `spectra.ScienceError(Exception)` as the root.
- If `E` is an exported `choice`, each variant becomes a subclass:
  `spectra.SpectrumError(ScienceError)` and
  `spectra.WrongChannelCount(SpectrumError)`, so `except spectra.SpectrumError`
  catches the family and `except spectra.WrongChannelCount` catches the case.
- Variant payload fields become attributes on the instance, with their declared
  names, converted by the same rules as return values. Structured errors stay
  structured; a Python caller reads `e.found` and `e.expected`.
- `__str__` is the `Display` implementation if `E` has one, and a generated
  rendering otherwise.
- Where a Python builtin is the obvious match, the generated class inherits
  from both: a shape mismatch is `class ShapeError(ScienceError, ValueError)`,
  a dtype mismatch is `class DtypeError(ScienceError, TypeError)`. Both
  `except ValueError` and `except spectra.ScienceError` then work, which is
  what a Python user expects, and it costs one extra base class.

**`Option of T` returns `T` on `Some` and `None` on `None`.** In argument
position, `None` maps to the `None` variant and anything else to `Some`.

Two cases are rejected at compile time rather than resolved by a rule, because
any rule would be a guess:

- `Option of (Option of T)` in an exported signature — `SC0451`. There is no
  Python value that distinguishes `Some(None)` from `None`. The error names the
  fix: define a `choice` and export that.
- `Option of ()` — same code, same reason.

**`Result` in argument position is rejected** (`SC0453`). Python callers do not
construct `Result` values, and a function that wants to accept failure should
accept `Option` or take the successful value.

`Result of (T, E)` where `E` is not exportable is rejected (`SC0450`) — an
error type that cannot cross is an error that cannot be reported.

### 3.6 Exported record and choice types

An exported `type` becomes a Python class, created with `PyType_FromSpec`,
holding a Science value inline:

- Fields become read-only properties by default; mutating access is exposed
  only through exported methods, because a Python-side attribute write would
  need to prove exclusive access and there is no syntax on the Python side to
  ask for it.
- `__repr__` from `Display` if present, a generated one otherwise.
- `__eq__` from `Eq` if present; `__hash__` only when the type is immutable and
  `Eq` is present, otherwise `None` (unhashable), because a hashable mutable
  wrapper is a bug factory.
- `__dlpack__` and the buffer protocol when the type *is* a tensor or wraps one
  (§4).
- Instances **cannot be pickled** by default, and say so in a clear `TypeError`
  naming the reason, because the buffer may be device memory or a borrow. Types
  all of whose fields are plain data get a generated `__reduce__` and pickle
  fine. This matters more than it looks: `multiprocessing` with the spawn start
  method pickles everything it sends, and a user who hits an inscrutable
  pickling error blames the language.

Construction from Python uses keyword arguments matching the field names, which
lines up with F0 §4.4's named-argument construction.

---

## 4. Arrays without copying

This is the section the language lives or dies on. A pipeline is glue; if the
glue copies a two-gigabyte array on the way in and again on the way out, the
hot function being fast is irrelevant.

### 4.1 Why DLPack, and precisely what it is

DLPack is the protocol NumPy (`np.from_dlpack`), PyTorch
(`torch.from_dlpack`), JAX, CuPy, MLX, TensorFlow and the Python array API
standard all implement. It is deliberately tiny: a dense strided buffer, a
device, a dtype, and a deleter. It has no notion of ragged data, sparse data,
object dtype, or Python-level semantics, and that is why everyone could agree
on it.

The parts this design depends on, stated exactly, because getting them subtly
wrong is how zero-copy becomes silent corruption:

- **`DLTensor`** holds: `void* data`; `DLDevice device`; `int32_t ndim`;
  `DLDataType dtype`; `int64_t* shape`; `int64_t* strides`;
  `uint64_t byte_offset`.
- **Strides are in elements, not bytes.** The buffer protocol's strides are in
  *bytes*. This is the single easiest way to write a corrupting bug in this
  area, and the conversion appears in exactly one place in the code generator.
- `strides == NULL` means compact row-major. We always emit explicit strides on
  export and always handle `NULL` on import.
- **`byte_offset` is in bytes**, and the effective base is `data + byte_offset`.
  Several implementations historically assumed it was zero; we emit zero by
  folding it into `data` on export, and honor it on import.
- **`DLDevice`** is `{device_type, device_id}`, with `kDLCPU = 1`,
  `kDLCUDA = 2`, `kDLCUDAHost = 3`, `kDLCUDAManaged = 13`, `kDLVulkan = 7`,
  `kDLMetal = 8`, `kDLROCM = 10`.
- **`DLDataType`** is `{code, bits, lanes}` with `kDLInt = 0`, `kDLUInt = 1`,
  `kDLFloat = 2`, `kDLOpaqueHandle = 3`, `kDLBfloat = 4`, `kDLComplex = 5`,
  `kDLBool = 6`. `lanes` is always 1 for us.
- **Two container versions exist.** The legacy `DLManagedTensor`
  (`{dl_tensor, manager_ctx, deleter}`) travels in a `PyCapsule` named
  `"dltensor"`. The versioned `DLManagedTensorVersioned` adds a `version` and a
  `flags` word — including `DLPACK_FLAG_BITMASK_READ_ONLY` — and travels in a
  capsule named `"dltensor_versioned"`. **We produce and consume both**, and
  negotiate: if the consumer passes `max_version` and it is at least `(1, 0)`
  we produce versioned, otherwise legacy. Older PyTorch and older NumPy only
  understand the legacy form, and dropping it would exclude them.
- **Capsule consumption is by renaming.** The consumer renames the capsule to
  `"used_dltensor"` (or `"used_dltensor_versioned"`) and takes ownership of the
  deleter. The producer's capsule destructor calls the deleter **only if** the
  name is unchanged — that is what makes an unconsumed capsule not leak and a
  consumed one not double-free. Our destructor implements exactly that check.

The Python-side protocol is two methods: `__dlpack_device__()` returning
`(device_type, device_id)`, and
`__dlpack__(*, stream=None, max_version=None, dl_device=None, copy=None)`
returning the capsule.

### 4.2 Export: a Science tensor becomes a Python array

The wrapper object returned by an exported function implements the protocol.
Step by step, when a consumer does `np.from_dlpack(result)`:

1. The consumer calls `result.__dlpack_device__()`. We read the device kind and
   id straight out of the tensor header and return them.
2. The consumer calls `result.__dlpack__(max_version=(1,0), dl_device=…,
   copy=…, stream=…)`.
3. If `dl_device` names a device we are not on: if `copy` is not `False`, we
   materialize a copy there; if `copy is False`, we raise `BufferError`, which
   is what the protocol specifies. Same for `copy=True` — the consumer is
   asking for a writable private buffer, and we give it one.
4. We allocate one **manager block** — a single allocation holding the
   `DLManagedTensorVersioned`, *its own copy of the shape array*, and *its own
   copy of the strides array*. This matters: the shape and strides of a Science
   tensor may live inline in a value that is about to be moved or dropped, and
   DLPack's `shape`/`strides` are raw pointers that must outlive the capsule.
   Copying 2·rank `int64`s is not a data copy and is not what "zero-copy"
   means.
5. `flags` gets `DLPACK_FLAG_BITMASK_READ_ONLY` if the tensor was exported from
   a shared borrow.
6. `manager_ctx` points at a small ownership record — see §4.4 — and `deleter`
   is a Science function that releases through it and decrements the export
   counter (§4.5).
7. We return `PyCapsule_New(block, "dltensor_versioned", destructor)`.
8. NumPy renames the capsule and builds an array whose deallocation now drives
   our deleter.

Cost, honestly: one small allocation, a header copy of roughly 64 bytes plus
2·rank 64-bit integers, one capsule object, and on the consumer side one array
object. Call it a couple of microseconds per array, dominated by Python object
creation rather than by us. It does not scale with the data, which is the whole
point. These are estimates from the shape of the work, not measurements; the
first thing the implementation should do is measure them and put the numbers in
this file.

**On GPU, the stream argument is not optional and not decorative.** The
consumer passes the stream it intends to read on. We must make the data
available on that stream: record an event on the Science stream that produced
the tensor and have the consumer's stream wait on it. The sentinel values
matter — `None` means no synchronization is requested, `1` is the legacy
default stream, `2` is the per-thread default stream, and `-1` means the
consumer explicitly wants no sync. Getting this wrong produces a race that
shows up as wrong numbers under load and nowhere else, so it is tested with a
deliberately delayed producer kernel.

### 4.3 Import: a Python array becomes a Science tensor

For a parameter of tensor type, the shim tries, in order:

1. **`__dlpack__`**, if the object has it. Call `__dlpack_device__()` first; if
   the device does not match what the parameter requires, and the parameter is
   not device-polymorphic, raise rather than silently transferring — a silent
   host-to-device copy in the middle of a "zero-copy" story is worse than an
   error. Then call `__dlpack__`, consume the capsule by renaming it, and
   validate.
2. **The buffer protocol**, via `PyObject_GetBuffer` with
   `PyBUF_STRIDES | PyBUF_FORMAT` (plus `PyBUF_WRITABLE` when the parameter is
   `mutable borrowed`). This is the path for plain NumPy arrays through
   `memoryview`, for `array.array`, for `bytes`, and for anything else that
   predates DLPack.
3. **`__cuda_array_interface__`** for older CuPy and Numba objects that have
   not adopted DLPack.
4. Otherwise: raise `TypeError` naming the three protocols and suggesting
   `np.asarray`.

**When the parameter is `mutable borrowed` and the device is CPU, the buffer
protocol is preferred over DLPack.** The reason is concrete: the buffer
protocol has a `readonly` field that NumPy sets accurately and that we can
trust, whereas DLPack's read-only flag only exists in the versioned container
and is widely ignored. If we are about to write into someone's buffer we want
the reliable signal. That is why both protocols are supported rather than one;
they are not redundant.

Validation on import, each with its own message and each a hard refusal rather
than a silent copy:

- **Negative strides** — refused. A reversed view (`a[::-1]`) cannot be
  expressed in DLPack. Message names `np.ascontiguousarray`.
- **Non-native byte order** — refused. DLPack has no endianness field at all,
  so a big-endian NumPy dtype (`>f4`) has no representation. Message names
  `arr.astype(arr.dtype.newbyteorder('='))`.
- **Misaligned data** — refused. A field view into a structured array can be
  unaligned; Science's codegen assumes natural alignment.
- **Non-simple dtypes** — structured, `object`, and datetime dtypes are
  refused.
- **Suboffsets present** (PIL-style indirect buffers) — refused.
- **Rank mismatch, extent mismatch** — refused, per §3.4.
- **Zero-size arrays** — accepted, with a null or dangling data pointer
  tolerated; the Science side must never dereference. This is a real case
  (`arr[arr > 1e9]` returning nothing) and crashing on it would be
  embarrassing.
- **Rank 0** — accepted, mapped to a scalar-shaped tensor.

And the release: for a DLPack import, the deleter must be called exactly once
when we are done; for a buffer-protocol import, `PyBuffer_Release` must be
called exactly once. Both live in the same ownership record so there is one
release path, not two.

### 4.4 Who owns the buffer afterwards

Three cases, and they are the whole ownership story.

**Case A — Science returns an owned tensor.** Ownership *moves* to Python. The
compiler sees the value moved into the boundary, so Science will never free it;
the manager block's ownership record holds the Science box, and the deleter
runs Science's drop glue — the same drop glue the compiler would have run, so
device memory is freed by the allocator that allocated it, at a point that is
now determined by Python's reference count rather than by a scope. The buffer
outlives the Science function and dies when the last Python reference to the
array dies. Nothing is copied.

**Case B — Science returns a borrow into a value Python already holds.** For
example, a method on a wrapper object returning a row view. The buffer is owned
by the Science value inside the wrapper, and the wrapper is a Python object. So
the manager block's ownership record holds a **strong reference to the
wrapper** (`Py_INCREF` on export, `Py_DECREF` in the deleter). The view cannot
outlive the wrapper because it holds the wrapper alive. This is the same "base
object" pattern NumPy has used for thirty years, and it works because Python's
reference count is a perfectly good implementation of "outlives".

**Case C — Python passes an array into Science.** The buffer stays owned by
Python. The shim holds a strong reference (or the consumed capsule's deleter)
for the duration of the call, and Science *borrows*. Region inference gives
that borrow a region no longer than the call, so it cannot escape — that is not
a new rule, it is exactly F0 §6.1 rule 5 applied to a parameter.

Case C has a real limitation and a real escape hatch. If the Science function
wants to *keep* the Python buffer — store it in a value it returns — the borrow
would have to outlive the call, and the compiler correctly refuses. Two ways
out:

- `.to_owned()` — copies. Honest, simple, sometimes right.
- `.adopt()` — **does not copy.** It builds a Science tensor whose drop glue
  calls the DLPack deleter (which usually decrefs the source array). The
  Science value now owns a foreign buffer and keeps the Python object alive for
  exactly as long as it needs it. This is how a Science object can hold a NumPy
  array's memory for hours without a copy and without a leak.

`adopt` is only sound when the source is not mutated from Python for the
adoption's lifetime, which we cannot check. It is therefore the one place in
the interop surface where the user takes on an obligation, and it is spelled
out in its diagnostic and its documentation. I considered making it
`unsafe adopt` (the word is reserved); I land on plain `adopt` with the
obligation documented, because the alternative is that everyone copies and the
language gets a reputation for copying.

### 4.5 How the borrow checker survives contact with Python

The borrow checker cannot see Python's heap. The conversion is stated once:

> **A region becomes a reference count.** A borrow exported to Python is kept
> alive by a strong reference to the Python object that roots the borrowed
> value. Exporting a borrow whose root is *not* rooted in Python is rejected at
> compile time.

The rejection needs no new analysis. If an exported function returns
`borrowed T` and region inference ties that region to a local, the existing
ownership checker already rejects it, with the existing diagnostic and the
existing explanation of where the borrow was born. If it ties the region to a
parameter, the shim knows which parameter, and increfs that parameter's Python
object. If it ties it to `self` on a wrapper method, the shim increfs the
wrapper. The only genuinely new case is an exported function returning a borrow
tied to a `const` or a `static` — always alive, no reference needed.

That handles the borrow outliving its referent. The other direction is harder,
and this is the honest part.

**The problem.** Python holds a read-only view into a Science value inside a
wrapper. Later, Python calls a method on that wrapper that takes
`mutable borrowed self`. Inside Science, exclusive access is proved. Across the
membrane, it is false: there is a live shared view. The compiler cannot see it,
because the view lives in a Python variable.

**The mechanism.** Each wrapper carries an **export counter**: the number of
live exports of the value it holds. `__dlpack__` and the buffer protocol's
`bf_getbuffer` increment it; the DLPack deleter and `bf_releasebuffer`
decrement it. Any wrapper method whose `self` is `mutable borrowed` checks the
counter first and raises `ScienceBorrowError` if it is non-zero, naming how
many views are outstanding. One integer, checked once per mutating call, never
in a loop. It is `RefCell` at the membrane and nowhere else: **static inside,
dynamic at the door.**

**The cost, stated plainly.** The counter drops when Python's garbage collector
gets around to it, which is not necessarily when the user stopped using the
view. So:

```python
v = np.asarray(obj)          # counter -> 1
obj.normalize_in_place()     # ScienceBorrowError: 1 outstanding view
del v                        # counter -> 0 (CPython: immediately, usually)
obj.normalize_in_place()     # fine
```

In CPython with the GIL, `del v` usually drops it immediately, because
refcounting is prompt. In a reference cycle, or under the free-threaded build's
deferred reclamation, or when the view was captured by a traceback (which
happens on every exception), it will not. **This is the most annoying cost in
this design** and I am not going to pretend otherwise. Three mitigations, in
order of how much I expect them to be used:

1. A context manager: `with obj.view() as v:` releases deterministically on
   exit. Documented as the recommended form whenever a mutation follows.
2. An explicit `v.release()` on the wrapper's own views.
3. A build flag `--python-borrow=permissive` that logs instead of raising, for
   people migrating a large codebase who want it running first. Off by default,
   because a silent aliasing bug in a scientific result is exactly the class of
   failure the language exists to prevent.

The counter is a plain integer under the GIL and an atomic under free-threaded
builds, with the check-and-mark done as a single compare-and-swap; see §5.5.

**A soundness hole I am accepting deliberately.** When Science exports a shared
borrow as a DLPack tensor, it sets the read-only flag. Consumers that predate
the versioned container, or that ignore flags, will happily hand the user a
writable array. Python can then write through a shared borrow that Science
believes is immutable. The options were: refuse to export shared borrows at all
(which means copying, which destroys the value proposition); copy on export
(same); or export and mark. I take export-and-mark, because the buffer-protocol
path — which is what plain NumPy uses via `np.asarray` — *does* have an
enforced `readonly` flag, so the most common case is actually protected, and
because DLPack consumers are converging on the versioned container. A
`--strict-interop` build flag copies instead, for anyone who wants the
guarantee more than the performance.

### 4.6 The buffer protocol specifically

PEP 3118 is older, CPU-only, and still how a plain NumPy array talks to
anything that is not array-aware. It is supported on both sides, not as a
fallback but as a first-class path with its own strengths.

**Consuming**: `PyObject_GetBuffer` with `PyBUF_STRIDES | PyBUF_FORMAT`, plus
`PyBUF_WRITABLE` when we intend to write. `Py_buffer` gives `buf`, `len`,
`itemsize`, `readonly`, `ndim`, `format`, `shape`, `strides`, `suboffsets`. We
parse only the simple format codes — `b B h H i I l L q Q n N e f d ?` — and
refuse everything else, including any struct syntax. Non-`NULL` `suboffsets` is
refused. `PyBuffer_Release` is paired with the acquire in the same ownership
record as everything else.

**Producing**: the wrapper implements `bf_getbuffer` and `bf_releasebuffer`, so
`memoryview(x)` works and `np.asarray(x)` is zero-copy without anyone
mentioning DLPack. `readonly` is set from whether the value was exported from a
shared borrow, and NumPy honors it — writing raises
`ValueError: assignment destination is read-only`. The export counter is
incremented and decremented by these two slots exactly as by DLPack.

Two things the buffer protocol cannot do, which is why DLPack is the primary:

- **No device concept.** A GPU tensor has no buffer-protocol representation;
  `bf_getbuffer` raises `BufferError` naming the device.
- **No `bfloat16`.** `e` is IEEE float16, and there is no format code for
  bfloat16 at all. A `BF16` tensor is DLPack-only, and the error says so.

And one thing it does better: **writability is reliably expressed and reliably
enforced.**

### 4.7 dtype mapping

| Science | DLPack `(code, bits)` | Buffer format | NumPy |
|---|---|---|---|
| `I8` / `I16` / `I32` / `I64` | `(kDLInt, 8/16/32/64)` | `b` / `h` / `i` / `q` | `int8`…`int64` |
| `U8` / `U16` / `U32` / `U64` | `(kDLUInt, 8/16/32/64)` | `B` / `H` / `I` / `Q` | `uint8`…`uint64` |
| `F16` | `(kDLFloat, 16)` | `e` | `float16` |
| `BF16` | `(kDLBfloat, 16)` | — (refused) | `bfloat16` (ml_dtypes/torch) |
| `F32` / `F64` | `(kDLFloat, 32/64)` | `f` / `d` | `float32` / `float64` |
| `Bool` | `(kDLBool, 8)` | `?` | `bool_` |

`lanes` is 1 always. Complex is absent from F0's primitives (§5.1) and so is
absent here; when it arrives it maps to `kDLComplex`. The mapping is total in
both directions except for `BF16` over the buffer protocol, and a dtype outside
this table raises `DtypeError` naming the received dtype and the accepted set.

---

## 5. The GIL

The goal is that the user never types anything about the GIL and never has to
know it exists, while getting the behavior an expert would have written.

### 5.1 Where it must be held

Everywhere a `PyObject` is touched. Concretely, in the generated shim: argument
parsing, reference counting, capsule creation and consumption, buffer acquire
and release, attribute access, exception raising and formatting, and building
the return value. That is the prologue and the epilogue of every exported
function, and every line of the `use python` support in §6.

It must *not* be held during anything that blocks or computes for a long time,
because holding it means no other Python thread runs at all.

### 5.2 Where it must be released, and how that is decided

The shape of every generated shim is:

```
  hold GIL      : parse arguments, acquire buffers, bind shape variables
  release GIL   : call the Science function
  hold GIL      : release buffers, build the result or raise
```

The release is conditional on one fact: **does the Science function, or
anything it transitively calls, touch Python?** If not, releasing is safe and
correct. If so, releasing is a crash.

That fact is an effect. The F0 spec names an effect discipline as part of the
gap Science aims at (§1), and this is the first place it earns its keep, in its
minimal one-bit form: a function carries the `python` effect if it calls a
Python-touching primitive, and effects propagate through calls. The shim
releases the GIL exactly when the callee's effect set excludes `python`.

If the general effect system is not ready when F2 ships, the same bit is
recoverable from the post-monomorphization call graph, which is available: a
function touches Python iff it transitively calls one of the runtime's
Python-touching primitives. The analysis must be conservative through dynamic
dispatch (`any Trait`) and function pointers — assume they touch Python unless
the concrete set is known. Conservative means "hold the GIL", which is slow but
never wrong. I would rather ship the conservative version than ship a wrong
release.

**The default is to release**, and the opt-out is an annotation for micro
functions where the release dominates. The default matters: a user who writes a
Science function and calls it from four threads should get four cores without
having read anything.

### 5.3 What releasing costs

Dropping and reacquiring the GIL is two operations on a mutex plus, if another
thread is waiting, a handoff. Uncontended, that is on the order of a hundred
nanoseconds; contended, reacquisition can wait for the other thread's switch
interval, which defaults to 5 ms — so a function that runs for 200 ns and
releases the GIL can, under contention, take far longer than one that did not.
(Order-of-magnitude figures inferred from the shape of the work; they must be
measured and this paragraph updated with real numbers.)

The rule that falls out: releasing is right whenever the Science body does real
work, and wrong for trivial accessors. Since "real work" is not statically
knowable, the generated code releases by default and the annotation exists for
the handful of hot tiny functions where a benchmark says otherwise. The
annotation is a build-level concern, not a language feature — it does not
change what the function means.

### 5.4 Callbacks and parallel regions

If a Python callable is passed into Science, calling it requires the GIL. If
the body were running GIL-released, each call would need
`PyGILState_Ensure`/`PyGILState_Release`, which serializes every worker thread
on the same lock and is slower than not releasing at all. So: **a function
whose parameters include a Python callable carries the `python` effect, the GIL
is held for its duration, and no release happens.** This is not a special case;
it falls out of §5.2 for free.

The same bit does a second job. F2 brings data parallelism; a parallel loop
body that touches Python from a worker thread is a deadlock waiting to happen.
The compiler rejects a `python`-effecting closure inside a parallel region
(`SC0454`). One analysis, two guarantees.

One discipline for the code generator, worth writing down because violating it
is a use-after-free that appears once a month in production: **never hold a
borrowed Python reference across any point that can release the GIL or call
arbitrary Python code.** Every reference the shim holds is a strong one.

### 5.5 Free-threaded Python

Free-threaded CPython (PEP 703) is real: experimental in 3.13, officially
supported from 3.14. It changes several things, and this design must not assume
the GIL exists.

What changes:

- **The module must declare itself.** Multi-phase init must include
  `Py_mod_gil = Py_MOD_GIL_NOT_USED`. Without it, importing our module makes
  CPython re-enable the GIL for the whole process and print a warning — which
  would make a Science module the reason a user's free-threaded program got
  slow. We declare `NOT_USED` only for builds we have actually verified, and
  verification means this checklist, not optimism.
- **`Py_BEGIN_ALLOW_THREADS` still matters** and is still emitted. In a
  free-threaded build it detaches the thread, which is what lets the
  stop-the-world phases of garbage collection proceed. A thread that computes
  for a minute without detaching stalls every collection in the process.
- **`science-rt` must be thread-safe.** Under the GIL, two Python threads could
  never be inside Science at the same time unless one had released it; without
  the GIL, they routinely are. So: an allocator safe for concurrent use, no
  non-atomic global mutable state anywhere in the runtime, and per-call state
  on the stack or in a call-scoped arena. This is a requirement on the runtime,
  and it is why this note flags it rather than discovering it later.
- **The export counter becomes atomic**, and the mutating path's check-then-act
  becomes a single compare-and-swap rather than a load followed by a store.
  Otherwise two threads each see zero outstanding views and both take exclusive
  access.
- **Borrowed references get more dangerous**, because deferred and biased
  reference counting make a borrowed reference's validity harder to reason
  about. The discipline from §5.4 — strong references only — is what makes this
  a non-event for us.
- **Container mutation needs critical sections.** Our surface barely touches
  containers, but where it does (building a returned `list`), the object is one
  we just created and no other thread can see it, which is the easy case.
- **Wheels.** Free-threaded builds have their own ABI tag (`cp313t`, `cp314t`)
  and, until stable-ABI support for them is settled, need a wheel per minor
  version rather than one abi3 wheel. CI builds both families; `pip` picks the
  right one. This roughly doubles the wheel matrix, which is annoying and not
  interesting.

What gets better: releasing the GIL stops being the point. Multiple threads can
be inside Science simultaneously without any handoff, so a thread pool
dispatching Science calls actually scales, and `asyncio.to_thread` around a
Science call becomes genuinely parallel rather than merely non-blocking.

### 5.6 Subinterpreters

Per-interpreter GIL (PEP 684) is the other concurrency story. A module opts in
with `Py_mod_multiple_interpreters = Py_MOD_PER_INTERPRETER_GIL_SUPPORTED`, and
the price is that the module must have no process-global mutable state — all
state per-module, reachable only through `PyModule_GetState`.

The first version declares subinterpreters **not supported**. The stable-ABI
discipline in §3.2 already pushes our state into per-module state, so the gap
is small, but "small" is not "verified", and declaring support we have not
tested produces crashes in someone else's process. It is a follow-up with a
clear test: run the same module in several subinterpreters concurrently and
exchange nothing between them.

### 5.7 asyncio

Nothing special is offered, and nothing needs to be. Because the shim releases
the GIL, `await asyncio.to_thread(spectra.standardize, x)` does exactly the
right thing and does not block the event loop. Calling a long Science function
directly from a coroutine blocks it, exactly as calling any long C function
does. The documentation says this in one line rather than the module growing an
async surface it cannot honor.

---

## 6. Python called from Science

### 6.1 What it costs

The reverse direction reaches libraries with no C API — which is most of
scientific Python above the array layer: pandas, scikit-learn, astropy,
matplotlib. The pull is obvious. The costs are also obvious and they are large:

- **A Science binary that embeds Python is not self-contained.** It needs an
  interpreter, of a compatible version, with the right packages, findable at
  runtime. The AOT story in F0 §3 — "self-contained binaries" — is gone for any
  program that uses this. The deployment story becomes Python's deployment
  story, which is the thing users are running toward Science to escape.
- **Everything is dynamic.** A `PyObject` is `any`. Nothing about a call into
  Python can be checked before the run starts, which is precisely the bet the
  language is making (§1). Every Python call is a hole in the verification
  story.
- **The GIL is held for the whole time.** By §5.4, a function calling Python
  carries the `python` effect, so it cannot be in a parallel region and its
  enclosing exported function will not release the GIL. One Python call in a
  hot loop serializes the program.
- **Errors become dynamic too** — a `PyError` carrying a type name and a
  message, not a `choice` the compiler can check a `match` against.

### 6.2 Where I land

**Yes in hosted mode, no in standalone mode, in the first version.**

- **Hosted**: the Science code was compiled into an extension module and is
  running inside a Python process that already exists. Here every cost above
  except the dynamic ones evaporates — the interpreter is already initialized,
  the packages are already installed, the deployment story is already Python's,
  and we are merely making a call. Calling back into Python from a Science
  extension module is ordinary and should be allowed.
- **Standalone**: an AOT binary that starts an interpreter of its own.
  Rejected for the first version (`SC0455` if a `use python` appears in a
  standalone build). Interpreter discovery, virtual-environment resolution,
  version compatibility and packaging are a project of their own, and doing it
  badly is worse than not doing it.

This split is not a compromise; it is the honest shape of the problem. The
adoption argument that motivates all of this note — *people adopt from inside
the Python program they already have* — is an argument about hosted mode
specifically. In hosted mode, calling Python is nearly free of consequences. In
standalone mode, it is the whole deployment story. Different situations,
different answers.

### 6.3 What it looks like

```science
use python "numpy" as numpy
use python "sklearn.decomposition" (PCA)

function reduce(points: borrowed Tensor of (F32, (n, d)))
        returns Result of (Tensor of (F32, (n, 8)), PyError):
    let model be try PCA(n_components: 8)
    let fitted be try model.fit_transform(points)
    Ok(try fitted.into_tensor())
```

The design is deliberately minimal:

- One opaque type, `PyObject`, with traits `FromPython` and `ToPython` for
  conversion. No attempt to type Python.
- Every call returns `Result of (PyObject, PyError)`, so `try` (F0 §4.5) is the
  ergonomics and the existing error propagation does all the work.
- Attribute access and calls go through the dynamic protocol. Keyword arguments
  use Science's named-argument syntax, which lines up exactly.
- Tensors cross by the same DLPack path as everything else (§4), in both
  directions — `into_tensor()` is the import path of §4.3 and passing `points`
  is the export path of §4.2, so a Python library call does not copy arrays.
  That is the part that makes this worth having at all.
- Every one of these carries the `python` effect (§5.2), which is what keeps
  them out of parallel regions and keeps the GIL held.

### 6.4 What it does not get

No typed stubs consumed from `.pyi` files to give Python calls static types.
That is attractive, and it is a large project (a second type system, one that
does not agree with ours), and it would build the verification story on a
foundation Python itself does not enforce. Later, or never.

---

## 7. Errors and tracebacks

A Python user who triggers a Science failure must see something they can act
on, in the place they are used to looking. This is a usability problem
masquerading as a plumbing problem, and the plumbing is where it is usually
lost.

### 7.1 Science → Python: failures

`Err(e)` becomes a raised exception of the generated class (§3.5), with the
variant's fields as attributes. The Python traceback naturally ends at the call
into the extension module, showing the user's own frames and then nothing —
correct, but unhelpful when the failure is three Science functions deep.

So the shim attaches the Science-side context. The mechanism is PEP 678
exception notes:

```
Traceback (most recent call last):
  File "analyze.py", line 12, in <module>
    clean, kept = spectra.standardize(x, saturation=0.98)
spectra.WrongChannelCount: expected 768 channels, found 512

Science call chain:
  standardize            spectra/pipeline.science:31
  check_channels         spectra/validate.science:8
```

`add_note` is available from 3.11, which is our floor (§3.2). Where it is not,
the same text lands on a `__science_traceback__` attribute and the module
installs a display hook that prints it.

The frames themselves come from a shadow stack the runtime maintains **only in
builds compiled with `--interop-traceback`**, which is the default for debug
builds and off for release. A shadow stack costs a store and a decrement per
call; in a hot loop that is not free. The alternative — reconstructing frames
from unwind tables — is free at runtime and considerably more work to
implement, and is the right eventual answer. First version: shadow stack,
opt-in for release, and say so.

Synthesizing *real* Python frames — building code objects with the Science
filename and line number so that Science frames appear inline in the normal
traceback and in every traceback-consuming tool — is better than notes and is
what this should eventually do. It needs one code object per Science function
appearing in a traceback, cached. Deferred; listed in §10.

### 7.2 Science → Python: panics

F0 §8 says `panic` prints a message and **aborts**. Aborting inside an
extension module kills the host interpreter — the user's notebook, the user's
web worker, the user's eight-hour job. A language that can kill the interpreter
on a bounds error is not adoptable, no matter what else is true about it. So:

> **A `panic` crossing into Python unwinds to the shim and raises
> `SciencePanic` instead of aborting.**

Concretely: exported functions and everything reachable from them are compiled
with unwind tables; `panic` in a hosted build raises a Science-internal unwind
rather than calling `abort`; the generated shim is the only landing pad; it
catches, formats the message and the shadow-stack backtrace, and raises
`SciencePanic(Exception)` with `.message`, `.location` and `.backtrace`
attributes.

Three honest costs:

- **Unwind tables inhibit some optimizations** and grow the binary, across the
  whole call tree reachable from an export. In a module where everything is
  exported, that is everything.
- **Destructors must run during unwinding**, or a panic leaks — including
  device memory, which is the leak that matters. This is the part that must be
  implemented properly rather than approximated: drop glue on the unwind path,
  tested.
- **This does not give Science a `catch`.** Panics remain unrecoverable *within
  Science*; there is no expression that catches one. The membrane is the only
  place a panic is observable, and it is observable there because the
  alternative is killing someone's process. That asymmetry is deliberate and
  worth defending: inside the language, a panic is still a bug with no handler;
  at the boundary, it is a fault reported to the host.

`SciencePanic` inherits `Exception`, not `BaseException`, so `except Exception`
catches it. A web handler that catches broadly should survive a Science bug.
Since Science has essentially no mutable global state, the process is in a
defensible state afterwards; the documentation says exactly that rather than
implying more.

### 7.3 Python → Science

A Python exception raised inside a callback, or inside a `use python` call
(§6), becomes `Err(PyError)`. `PyError` holds:

- a **strong reference to the original exception object**;
- the exception type's name and the `str()` of it, as Science `String`s, so
  Science code can match on them without touching Python;
- the **formatted traceback**, captured eagerly.

Eagerly is the operative word. The traceback must be formatted while we
certainly hold the GIL and while the frames are certainly alive; deferring it
to whenever Science happens to look produces either a crash or an empty string.
We pay the formatting cost on every captured exception, which is fine, because
exceptions are exceptional.

### 7.4 Round trip: Python → Science → Python

This is the case that is usually botched. A Python callback raises `KeyError`;
Science propagates the `Err` with `try`; the exported function returns it; the
shim raises. What should the user see?

**The original exception object, re-raised.** Not a `ScienceError` wrapping a
description of it. `except KeyError:` in the caller must still work, and the
original traceback must still be there. Since `PyError` kept the strong
reference, the shim restores it and raises it directly. Science's own context
is added as a note, so nothing is lost:

```
Traceback (most recent call last):
  File "analyze.py", line 20, in <module>
    out = spectra.map_rows(x, my_callback)
  File "analyze.py", line 16, in my_callback
    return table[name]
KeyError: 'pressure'

Raised inside a Science call:
  map_rows               spectra/pipeline.science:57
```

If Science *wrapped* the error into one of its own (mapped it into a different
`choice`), the shim raises the Science exception with the Python one as the
`__cause__` — `raise … from …` — so the chain prints both.

**Fatal exceptions do not get swallowed.** `KeyboardInterrupt`, `SystemExit`
and `MemoryError` mean the program is trying to stop or is out of resources. If
Science receives one as an `Err` and *discards* it — which the language
permits, since `Result` is an ordinary value — the intent is lost. So the shim
checks, on the way out, whether a fatal Python exception was captured and not
re-raised, and re-raises it even when Science returned `Ok`. It is a
belt-and-braces check on a rare path, and the alternative is a program that
cannot be interrupted.

### 7.5 Ctrl-C during a long call

With the GIL released and a Science function running for ten minutes, Ctrl-C
does nothing until it returns, because CPython only runs signal handlers in the
main thread while holding the GIL. This is the same behavior every C extension
has and every user hates.

The mitigation: the module installs a small signal handler that sets an atomic
flag. Science code that has cancellation points — the runtime's parallel loop
driver, and any loop the user marks — polls the flag with a relaxed atomic load
(a few cycles, negligible next to a loop body doing real work) and unwinds with
a cancellation, which the shim turns into `KeyboardInterrupt`. Functions with
no cancellation points remain uninterruptible, and the documentation says which
those are rather than promising otherwise.

This is opt-in and imperfect, and it is listed here because "your Ctrl-C does
not work" is a complaint that arrives in week one, and an answer of "that is
how C extensions work" is not good enough for a language whose pitch is that it
is better than the alternatives.

---

## 8. What the user actually types

A real pipeline step: a batch of 768-channel spectra comes in, rows that
saturated the detector are dropped, the rest are standardized per row, and the
indices that survived come back alongside.

### 8.1 `spectra/pipeline.science`

```science
use tensor (Tensor)

choice SpectrumError:
    WrongChannelCount(found: Int, expected: Int)
    Empty
    AllSaturated

public to python
function standardize(
        batch: borrowed Tensor of (F32, (rows, 768)),
        saturation: F32)
        returns Result of ((Tensor of (F32, (kept, 768)), Array of Int),
                           SpectrumError):
    if batch.extent(0) is 0:
        return Err(SpectrumError.Empty)

    let mutable keep be Array of Int .new()
    for each r in 0..batch.extent(0):
        if batch.row(r).maximum() is below saturation:
            keep.push(r)

    if keep.len() is 0:
        return Err(SpectrumError.AllSaturated)

    let mutable out be Tensor of F32 .zeros((keep.len(), 768))
    for each i in 0..keep.len():
        let row be batch.row(keep.get(i))
        let centre be row.mean()
        let spread be row.standard_deviation().maximum(1e-6f32)
        out.row_mutable(i).assign((row - centre) / spread)

    Ok((out, keep))
```

Note what is *not* there: no annotation about the GIL, no reference counting,
no mention of NumPy, no lifetime, no decoration beyond the three words
`public to python`. The shape `(rows, 768)` is checked once at the door and
never again; inside the loop, `row(r)` needs no check against the channel count
because the type carries it. (`extent(0)` rather than `.shape` because F0 §13
reserves `shape` as a Science keyword — see §8.3 for why that has no effect on
the Python side.)

### 8.2 Build

```
$ sciencec build --python
   compiling spectra (1 file)
   emitting  spectra.cp312-win_amd64.pyd
   emitting  spectra.pyi
   emitting  spectra-0.1.0-cp311-abi3-win_amd64.whl

$ pip install spectra-0.1.0-cp311-abi3-win_amd64.whl
```

### 8.3 The generated stub, `spectra.pyi`

```python
from typing import Any

CHANNELS: int

class ScienceError(Exception): ...
class SpectrumError(ScienceError): ...
class WrongChannelCount(SpectrumError):
    found: int
    expected: int
class Empty(SpectrumError): ...
class AllSaturated(SpectrumError): ...
class SciencePanic(Exception):
    message: str
    location: str
    backtrace: str
class ScienceBorrowError(ScienceError): ...

class Tensor:
    shape: tuple[int, ...]
    dtype: Any
    ndim: int
    def __dlpack__(self, *, stream: Any = None, max_version: Any = None,
                   dl_device: Any = None, copy: bool | None = None) -> Any: ...
    def __dlpack_device__(self) -> tuple[int, int]: ...
    def __buffer__(self, flags: int) -> memoryview: ...

def standardize(batch: Any, saturation: float) -> tuple[Tensor, list[int]]: ...
```

`shape` is a perfectly good attribute name *in Python*. The F0 spec reserves
`shape` as a Science keyword, which means `x.shape` does not compile in
Science — a cost §13 accepted deliberately. It has no effect on the Python
surface, which is worth noting because it is the kind of thing that looks like
a leak and is not.

### 8.4 Calling it

```python
import numpy as np
import spectra

x = np.load("run_0417.npy")          # (2048, 768) float32, C-contiguous
clean, kept = spectra.standardize(x, saturation=0.98)

len(kept)                            # 1993
clean.shape                          # (1993, 768)
```

`x` was not copied on the way in: the shim took it through the buffer protocol,
handed Science a borrowed tensor pointing at NumPy's memory, and released the
`Py_buffer` when the call returned. `clean` was not copied on the way out:
Science allocated it, ownership moved to Python, and the wrapper's deleter will
run Science's drop glue when the last reference dies.

Turning the result into a NumPy array is also free:

```python
a = np.asarray(clean)
a.base is clean                      # True — a view, no copy
a[0, 0] = 0.0                        # fine; `clean` was an owned return
```

Proving there is no copy on the way in, with an in-place variant:

```python
spectra.clip_in_place(x, 0.98)       # takes `mutable borrowed`
x.max()                              # 0.98 — Science wrote into NumPy's buffer
```

`clean` is a `spectra.Tensor` wrapper, not a NumPy array. That is a deliberate
choice and it has a cost: `clean.mean()` does not work, and users will be
briefly annoyed. The reason is §3.3 — a module that hard-depends on NumPy is
the wrong neighbor for a torch-only or JAX-only user, and privileging one
framework is exactly what DLPack exists to avoid. `np.asarray(clean)` is one
free call, and `torch.from_dlpack(clean)` works identically. For teams who
disagree, `--python-return=numpy` in `pyproject.toml` makes exported functions
return NumPy arrays directly. I land on the wrapper as the default and I expect
to be argued with about it.

### 8.5 What the failures look like

Wrong number of channels:

```python
>>> spectra.standardize(np.zeros((4, 512), dtype=np.float32), 0.98)
Traceback (most recent call last):
  File "<stdin>", line 1, in <module>
ValueError: standardize(): argument 'batch' has extent 512 on axis 1,
but the signature requires 768
```

Wrong dtype — note that it is a `TypeError`, and note that it names the
conversion:

```python
>>> spectra.standardize(np.zeros((4, 768)), 0.98)
Traceback (most recent call last):
  File "<stdin>", line 1, in <module>
TypeError: standardize(): argument 'batch' has dtype float64,
but the signature requires float32.
Convert with arr.astype(np.float32), or pass a float32 array to avoid a copy.
```

A domain failure, which is an `Err` in Science and an exception here:

```python
>>> spectra.standardize(np.ones((4, 768), dtype=np.float32), 0.5)
Traceback (most recent call last):
  File "<stdin>", line 1, in <module>
spectra.AllSaturated: every row exceeded the saturation threshold

Science call chain:
  standardize            spectra/pipeline.science:24
```

And the borrow check at the membrane, which is the one genuinely novel error a
Python user will meet:

```python
>>> cal = spectra.Calibration.load("cal.json")
>>> v = np.asarray(cal.gains)        # a view into the Science value
>>> cal.rescale(1.05)                # wants exclusive access
Traceback (most recent call last):
  File "<stdin>", line 1, in <module>
spectra.ScienceBorrowError: rescale() needs exclusive access to this
Calibration, but 1 view of it is still alive.
Drop it (`del v`), or use `with cal.view() as v:` to bound its lifetime.
>>> del v
>>> cal.rescale(1.05)                # fine
```

That message is the whole design in one paragraph: the guarantee is the same
one the compiler enforces statically inside Science, converted at the membrane
into a check a Python user can understand and a suggestion they can act on.

---

## 9. Diagnostics allocated

Per F0 §9, codegen and linking own `SC0400`–`SC0499`. This note claims
`SC0450`–`SC0479` for the Python boundary. **This range needs reconciling with
the sibling design notes**, which may also be allocating in `SC0400`+.

| Code | Meaning |
|---|---|
| `SC0450` | Type in an exported signature has no Python representation |
| `SC0451` | `Option of (Option of T)` or `Option of ()` in an exported signature |
| `SC0452` | Exported function returns a borrow whose root is not rooted in Python |
| `SC0453` | `Result` in argument position of an exported function |
| `SC0454` | Python-effecting code inside a parallel region |
| `SC0455` | `use python` in a standalone (non-hosted) build |
| `SC0456` | Exported name collides with a Python keyword or a generated name |
| `SC0457` | `Array of T` with numeric `T` in an exported signature (warning) |

---

## 10. Deferred, with reasons

- **Synthetic Python frames** for Science call sites, so Science appears in
  ordinary tracebacks and in every tool that reads them. Better than notes;
  more work. §7.1.
- **Unwind-table backtraces** replacing the shadow stack, removing the runtime
  cost of `--interop-traceback`. §7.1.
- **Subinterpreter support.** Small gap; needs testing before it is claimed.
  §5.6.
- **Standalone embedding of Python.** Interpreter discovery and packaging is a
  project. §6.2.
- **Typed Python stubs consumed by Science**, giving `use python` calls static
  types. A second type system. §6.4.
- **Complex dtypes**, following F0 §5.1 gaining them.
- **A NumPy-returning build mode** exists as a flag from day one
  (`--python-return=numpy`), but the default is the wrapper, for the reason in
  §8.4.

## 11. The one thing F0 must not preclude

F0 §2 lists three constraints from later phases. This note adds a fourth, and
it is the only demand it makes on F0:

> **`panic` must be implementable as an unwind, not only as an abort.**

F0 §8 specifies "panic with a message and abort", and that is the right default
for a standalone binary. But if the runtime's panic path is written as a
straight call to `abort`, with no unwind tables, no drop-on-unwind, and no
landing-pad concept anywhere, then §7.2 is not a change to the panic path — it
is a rewrite of it, plus a retrofit of drop glue onto an unwind path that does
not exist. The same shape as F0 §6.2's warning about provenance: a solver
written without it must be rewritten entirely to gain it.

The ask is small and costs F0 nothing to honor: route `panic` through a single
runtime entry point; keep the decision of what that entry point does (abort, or
unwind to a registered landing pad) a property of the build rather than a fact
compiled into every call site; and make sure drop glue is emitted in a form an
unwind path can call. F0 can still always abort. It just must not make
unwinding impossible.

## 12. Risks worth naming

- **The export counter's non-determinism** (§4.5) is the most likely source of
  user frustration, and its severity depends on garbage collection behavior we
  do not control. If the context manager does not become the idiom people
  actually use, this will be the top complaint.
- **The DLPack read-only hole** (§4.5) is a real unsoundness, accepted for
  adoption. If it produces a wrong scientific result in public, that is a bad
  day; the honest mitigation is that the common NumPy path goes through the
  buffer protocol, where the flag is enforced.
- **Unwind tables across every exported call tree** (§7.2) may cost more
  performance than expected. It needs measuring early, because if it is
  expensive the answer is a per-function opt-out, and that changes the surface.
- **The wheel matrix** (abi3 × platforms × free-threaded) is the unglamorous
  thing that actually determines whether `pip install` works for a stranger,
  and it is easy to underinvest in.
- **The performance figures in §4.2 and §5.3 are estimates**, stated as such
  deliberately. The first implementation task after the shim works is to
  replace them with measurements taken on real hardware.
