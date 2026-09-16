# Science — Design: the C foreign function interface

Date: 2026-09-16
Status: draft for review
Depends on: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` (the F0 core),
and the runtime ABI as it exists in `crates/science-rt` (§2–§8 of that crate's
documentation, which this note treats as settled).

Scope: the C boundary. Declaration syntax, the ownership rules that govern
memory crossing it, what `unsafe` means and does not mean, callbacks from C into
Science, linking, error conversion, and whether bindings should be generated.

---

## 0. The claim this note is built on

Science is a language for scientific computing, and essentially every floating
point operation that matters in scientific computing already happens inside a C
or Fortran library somebody else wrote and somebody else tuned. `dgemm` is
thirty years of hand-written assembly per microarchitecture. cuDNN is a closed
binary. HDF5 is a file format with an implementation, not a file format with a
specification anyone reimplements for fun. A language that cannot call these is
not a scientific language, whatever its type system can prove.

So the FFI is not a peripheral feature. It is, alongside the tensor type, the
load-bearing part of the surface. The design goal is accordingly specific:

> **Make the boundary narrow, explicit, and checkable on the Science side, and
> be honest that the C side cannot be checked at all.**

Everything below follows from that. The recurring shape is: a declaration
records a claim, the compiler verifies the half of the claim that lives in
Science, `unsafe` marks the half that does not, and a hand-written safe wrapper
is the place where the unverifiable half is discharged by an argument a human
wrote down.

---

## 1. Declaration syntax

### 1.1 The block

Foreign functions are declared in an `extern` block. The block carries `unsafe`,
names an ABI, and names the library it comes from.

```science
unsafe extern "C" library "openblas" via pkg-config "openblas":

    type CblasLayout is CInt
    const CBLAS_ROW_MAJOR be 101 as CblasLayout
    const CBLAS_COL_MAJOR be 102 as CblasLayout

    type CblasTranspose is CInt
    const CBLAS_NO_TRANS be 111 as CblasTranspose
    const CBLAS_TRANS be 112 as CblasTranspose

    def cblas_dgemm(
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
```

Reading the header left to right:

- **`unsafe`** is on the block, not on each item. An `extern` declaration is a
  claim about code the compiler will never see: that a symbol of this name
  exists, that it takes these arguments in this order with these widths, that it
  does not retain the pointers it is given, that it does not free them. None of
  that is checkable. Writing the declaration is therefore itself the unsafe act,
  and the keyword belongs where the act is. Rust reached the same conclusion in
  its 2024 edition after a decade of `extern` blocks that looked safe; there is
  no reason to relearn it.
- **`"C"`** names the ABI. In this phase `"C"` is the only accepted string, and
  it means the platform's C calling convention — System V AMD64, Windows x64,
  AAPCS64, as the target dictates. Fortran BLAS and LAPACK are also `"C"`: their
  difference is not the convention but the argument passing, which is expressed
  in the types (§1.5).
- **`library "openblas"`** names a link target, not a file. §5 covers what the
  driver does with it.
- **`via pkg-config "openblas"`** is optional and says how to discover the flags.
  Also §5.

`library`, `via`, `pkg-config`, `symbol`, `from`, `when` and `available` are
**contextual**: they are recognized only inside an `extern` block's grammar and
remain ordinary identifiers everywhere else. `extern`, `unsafe`, `static`,
`with` and `move` are already in the F0 reserved list (§13 of the core spec) and
this design uses all five. Adding no new globally reserved word was a constraint,
not an accident: `from` in particular is a name working scientists use, and
reserving it to buy FFI syntax would be a bad trade.

### 1.2 Items an `extern` block may contain

Three, and only three.

| Item | Form | Meaning |
|---|---|---|
| Function | `def name(params) returns T` | An undefined symbol resolved at link |
| Type alias | `type Name is T` | A spelling for a C typedef over an FFI-representable type |
| Constant | `const NAME be literal as T` | A `#define` or enumerator, transcribed |

Records and handle types are **not** declared inside the block. They are
ordinary Science `type` declarations outside it, marked with a marker trait
(§1.4). The reason is the orphan rule: a handle needs a `Drop` implementation,
and `Drop` can only be implemented on a type the declaring module owns. Putting
the type outside the block keeps it an ordinary Science type with ordinary
Science rules, and the `extern` block stays what it should be — a list of
symbols.

### 1.3 The FFI type vocabulary

`ffi` is a module in the standard library. Its contents are a **closed set**, in
the same sense §8 of the core spec closes the F0 library: what is listed exists
and nothing else does, and adding to it is a spec change.

**Scalars.** Target-dependent aliases, because C's integer types are.

| `ffi` type | C type | Note |
|---|---|---|
| `CChar` | `char` | Signedness is target-dependent; `as` to `I8`/`U8` is always written |
| `CInt`, `CUInt` | `int`, `unsigned int` | 32 bits on every target Science targets |
| `CLong`, `CULong` | `long`, `unsigned long` | **64 on Unix, 32 on Windows.** See §1.6 |
| `CLongLong`, `CULongLong` | `long long` | |
| `CSizeT` | `size_t` | |
| `CPtrDiff` | `ptrdiff_t` | |
| `CFloat`, `CDouble` | `float`, `double` | Aliases of `F32`, `F64` |
| `CVoid` | `void` as a pointee | Only ever appears under a pointer |

`F16` and `BF16` have **no** entry, deliberately. C has no agreed by-value ABI
for half precision: `_Float16` is passed differently by GCC, Clang and MSVC, and
CUDA's `__half` is a struct wrapping a `unsigned short`. Half-precision scalars
therefore cross the boundary **by reference only**, or as a `U16` bit pattern the
caller reinterprets. Passing an `F16` by value to an extern function is
`SC0431`. The cost is a small ergonomic wart in exactly the place — mixed
precision training — where it will be felt most; the alternative is an ABI that
is right on one compiler and silently wrong on another, which in a language whose
users publish is not an alternative.

**Pointers and views.** Four types, and the distinctions between them are the
whole design.

| Type | Lowers to | Region-tracked | Usable outside `unsafe` |
|---|---|---|---|
| `borrowed T` | `const T*`, one element | yes | yes |
| `mutable borrowed T` | `T*`, one element, `noalias` | yes | yes |
| `ffi.Span of T` | `const T*`, many elements | yes | yes |
| `ffi.MutableSpan of T` | `T*`, many elements, `noalias` | yes | yes |
| `ffi.Pointer of T` | `T*`, nullable, unknown validity | **no** | no |
| `ffi.OpaqueHandle` | `void*`, non-null, never dereferenced | no | as a field only |
| `ffi.FunctionPointer of (extern "C" def(...) returns R)` | function pointer | no | as a field only |

`ffi.Span of T` is `{ borrowed T, Int }` — a pointer with a length, holding a
borrow whose region is inferred exactly as §4.4 of the core spec infers the
region in `type View: source: borrowed Doc`. **At the ABI only the pointer
crosses.** The length exists solely on the Science side, and it exists for one
reason: so that the safe wrapper's precondition — that `lda * n` does not exceed
the buffer — is an expression the compiler type-checks rather than a comment.

`borrowed Array of T` is **rejected** in an extern signature (`SC0421`). C wants
the elements; Science's `Array` is a three-word `{ptr, len, cap}` header
(`crates/science-rt/src/array.rs`); passing a pointer to the header would be
wrong and silently taking `.ptr` would hide which of the two the author meant.
The diagnostic names `.span()` as the fix.

For ergonomics, an argument of type `borrowed Array of T` at an extern call site
**coerces** to `ffi.Span of T`, on the same principle as auto-borrow (§6.3 of the
core spec): the signature keeps the information, the call site loses the noise.
This is one more implicit conversion in a language that has been sparing with
them, and it is recorded here as a cost accepted for a specific reason — every
BLAS call has between two and four array arguments, and `.span()` four times per
call would be the most-typed token in numerical Science code.

**Strings.** C strings are NUL-terminated; Science's `String` is UTF-8 bytes with
a length and is not NUL-terminated. They are not the same thing and no coercion
between them exists.

| Type | Meaning |
|---|---|
| `ffi.CString` | Owned, NUL-terminated, Science's allocator, `Drop` frees it |
| `ffi.CStr` | `borrowed`, NUL-terminated, someone else's memory |

`ffi.CString.from(text: borrowed String) -> (CString, NulError?)` — fallible,
because a Science `String` may contain an interior NUL, which is valid UTF-8 and
not a valid C string. `CStr.to_string() -> (String, Utf8Error?)` — fallible,
because C strings are byte strings and nothing guarantees a library hands back
UTF-8. Both allocate and copy. There is no zero-copy path and there should not be
one.

**Uninitialized memory.** `ffi.Uninitialized of T` is a slot of the right size and
alignment that holds no value. It exists because the out-parameter idiom
(`cudnnCreate(cudnnHandle_t *handle)`) requires passing a pointer to a place that
does not yet contain anything, and Science otherwise has no way to name such a
place. `mutable borrowed ffi.Uninitialized of T` lowers to `T*`. Reading it
requires `unsafe` and the operation `assume_initialized`, which is where the
author asserts the C function wrote it.

### 1.4 C-compatible records

An ordinary Science `type` is given C layout by implementing a marker trait:

```science
type DLTensor:
    data: ffi.Pointer of CVoid
    device: DLDevice
    ndim: I32
    dtype: DLDataType
    shape: ffi.Pointer of I64
    strides: ffi.Pointer of I64
    byte_offset: U64

DLTensor implements ffi.CLayout
```

`ffi.CLayout` means: fields in declaration order at the offsets the platform C
ABI gives them, no field reordering, no niche optimization, size rounded to
alignment by the C rule. It is a marker trait implemented on one line, exactly as
§4.4 of the core spec implements `Copy`, so this costs no new syntax. The orphan
rule does the access control for free: you cannot give C layout to a type you do
not own.

`ffi.CLayout` is checked, not assumed. Every field type must itself be
FFI-representable, transitively (`SC0424` otherwise). A type implementing it may
not hold a `String`, an `Array`, a `Map`, a `T?` whose payload has no niche, or
any `choice` — those are Science layouts and the C side does not know them.

**A single-field record over an FFI-representable type is transparent**: it is
passed and returned as that field, with no wrapper. This is what makes handle
types free:

```science
type CudnnHandle:
    raw: ffi.OpaqueHandle

CudnnHandle implements ffi.CLayout
```

`CudnnHandle` is one word at the ABI, and because `ffi.OpaqueHandle` is non-null
it inherits the null niche of §5.2 of the runtime ABI — so `CudnnHandle?` is also
one word, with the null pointer as `null`, costing not one bit more than the
handle itself. That falls out of rules that already exist.

**Bitfields and unions are not representable.** A C struct containing either is
imported as an opaque byte array of the right size and alignment, and the only
way to read it is a generated accessor compiled by a C compiler. This is
unpleasant and it is stated rather than hidden: bitfield layout is
implementation-defined in ways that differ between GCC and MSVC on the same
hardware, and a language that guessed would be wrong on one of them. No library
in the target set — BLAS, LAPACK, cuBLAS, cuDNN, HDF5, GSL, MPI, NetCDF — needs
a bitfield in its public interface.

**Variadic functions are not representable.** No `printf`. The variadic calling
convention is per-platform and Science has nothing to type it with. The fix is a
one-line C shim. Nothing in the target set needs this either.

### 1.5 C enums are not `choice`

A C enum is imported as a newtype over its underlying integer with associated
constants, never as a Science `choice`:

```science
type CudnnStatus:
    raw: CInt

CudnnStatus implements ffi.CLayout

CudnnStatus has methods:
    const SUCCESS be CudnnStatus(raw: 0)
    const NOT_INITIALIZED be CudnnStatus(raw: 1)
    const ALLOC_FAILED be CudnnStatus(raw: 2)
    # ...
```

The reason is that a `choice` value must be one of its declared variants — the
runtime ABI pins the discriminant encoding, codegen relies on exhaustive `match`,
and a value outside the declared set is undefined behaviour. A C library is under
no obligation to return a value your header knew about. cuDNN 9 returns statuses
cuDNN 8's header does not declare; HDF5 error codes are open-ended by design.
Importing as a `choice` means that a newer library than the one you generated
against produces an invalid enum, and everything after that is undefined.

Converting to a real Science `choice` is then an ordinary total function with a
catch-all arm, written once in the safe layer, and the catch-all carries the
unrecognized integer so that a bug report contains the number.

### 1.6 Integer width, and the worst FFI bug in scientific computing

`BlasInt` appeared in §1.1 without explanation. It is the reference case for why
this design refuses to paper over C's integer types.

BLAS and LAPACK are distributed in two incompatible builds. The reference build
uses 32-bit integers for dimensions and leading dimensions; the ILP64 build uses
64-bit. They have identical function names, identical argument counts, identical
semantics, and differ only in the width of eight of `dgemm`'s thirteen
arguments. Link a program that thinks `m` is 32 bits against a library that
thinks it is 64 and the result is not a crash — it is correct answers for small
matrices and silently wrong answers for large ones, which is the single worst
failure mode available to a numerical program.

Science's answer is to make the choice explicit and to make the mistake a link
error:

```science
# 32-bit integers: the reference and default OpenBLAS build.
unsafe extern "C" library "openblas":
    type BlasInt is I32
    def dgemm(...) symbol "dgemm_"

# 64-bit integers: OpenBLAS built with INTERFACE64=1 SYMBOLSUFFIX=64_.
unsafe extern "C" library "openblas64_":
    type BlasInt is I64
    def dgemm(...) symbol "dgemm_64_"
```

`symbol` decouples the Science name from the linker name, which is what makes
this work: the ILP64 builds that matter ship distinct symbols, so choosing the
wrong block produces an undefined symbol at link time rather than a wrong answer
at run time. Where a vendor ships ILP64 under the *same* symbol names — Intel MKL
does, in `libmkl_intel_ilp64` — nothing can detect the mismatch, and the binding
must say so in a comment because it is the only warning available.

`CLong` carries the same hazard in smaller form: 64 bits on Linux and macOS, 32
on Windows. Science does not alias it to `I64`. Anyone reaching for `CLong` gets
a type whose width they have to think about, which is the correct amount of
friction.

### 1.7 Calling `dgemm`

The declaration is not the interface. The interface is a safe wrapper whose body
is a check and an `unsafe` call, and whose signature contains no `ffi` type at
all.

```science
type MatrixView:
    data: ffi.Span of F64
    rows: Int
    columns: Int
    leading: Int        # the "leading dimension" — stride between columns

public choice BlasError:
    Shape(Int, Int)
    Stride(Int, Int)
    Overflow

public def gemm(
        a: borrowed MatrixView,
        b: borrowed MatrixView,
        c: mutable borrowed MatrixView,
        alpha: F64,
        beta: F64)
        -> ((), BlasError?):

    if a.columns is not b.rows:
        return ((), BlasError.Shape(a.columns, b.rows))
    if c.rows is not a.rows or c.columns is not b.columns:
        return ((), BlasError.Shape(c.rows, a.rows))

    # The obligation the borrow checker cannot discharge, discharged here.
    let _, err be covers(a, a.columns)
    if err?:
        return ((), err)
    let _, err be covers(b, b.columns)
    if err?:
        return ((), err)
    let _, err be covers(c, c.columns)
    if err?:
        return ((), err)

    unsafe:
        cblas_dgemm(
            layout: CBLAS_COL_MAJOR,
            transpose_a: CBLAS_NO_TRANS,
            transpose_b: CBLAS_NO_TRANS,
            m: a.rows as BlasInt,
            n: b.columns as BlasInt,
            k: a.columns as BlasInt,
            alpha: alpha,
            a: a.data, lda: a.leading as BlasInt,
            b: b.data, ldb: b.leading as BlasInt,
            beta: beta,
            c: c.data, ldc: c.leading as BlasInt)
    ((), null)

def covers(m: borrowed MatrixView, columns: Int) -> ((), BlasError?):
    let needed be m.leading * (columns - 1) + m.rows
    if m.leading < m.rows: return ((), BlasError.Stride(m.leading, m.rows))
    if needed > m.data.length(): return ((), BlasError.Overflow)
    ((), null)
```

Two things in that call site deserve their reasons.

**Named arguments at extern call sites are permitted, and in any order.**
Ordinary Science calls are positional (§4.6 of the core spec reserves named
arguments for construction). Extern calls are the exception. `cblas_dgemm` takes
thirteen arguments, two of which are transposition flags that look identical and
whose confusion produces a wrong answer rather than an error. The domain argument
is decisive: this is the call every numerical Science program makes, and it must
be readable at the call site by someone checking a formula against a paper. The
cost is two call conventions in one language, which is design debt of the same
kind §4.6 already accepts twice over (`each` versus `giving`, phrases versus
symbols), and it is recorded here as deliberate. If the core team would rather
have named arguments everywhere or nowhere, this section defers.

**The `as BlasInt` casts are written out.** §5.1 of the core spec forbids
implicit numeric conversion, and the FFI is the last place to make an exception:
the whole point of §1.6 is that the width is load-bearing.

---

## 2. The ownership boundary

This is the part worth getting right, so it is worked case by case. Science has
single ownership, inferred regions, no garbage collector, and no reference
counting. Every question about memory crossing the boundary is a question about
which of four things is happening, and each has exactly one spelling.

### 2.1 The four cases

| Case | Who frees | Spelling | Verified by the compiler |
|---|---|---|---|
| A — C borrows ours for the call | Science | `borrowed T`, `ffi.Span of T`, and their mutable forms | The pointer is valid and unaliased for the call; it is not freed during it |
| B — C hands us something it allocated | Science, by calling C's destructor | A handle type with `implements Drop` | Destroyed exactly once, at a program point the compiler names |
| C — C takes ownership of ours | C | `move ffi.CBuffer of T` | The value is dead in Science after the call |
| D — C lends us something it owns | C | `returns borrowed T from p` / `from static` | The borrow does not outlive `p` |

Everything else is `ffi.Pointer` and `unsafe`, and §2.7 says what "everything
else" contains.

### 2.2 Case A — C borrows ours for the duration of the call

This is the common case and the whole of BLAS. `cblas_dgemm` reads `A` and `B`,
writes `C`, and retains nothing.

Declaring a parameter `borrowed T` or `ffi.Span of T` makes the extern function,
for the region engine's purposes, an ordinary Science function that takes a
borrow and returns nothing borrowing it. Rule 5 of §6.1 then applies with no
modification: the region of the borrow is the call, no borrow outlives its
referent, and a shared and an exclusive borrow of the same value cannot coexist.
So `gemm(a, b, c)` where `c` is the same matrix as `a` is `SC0302`, caught by
machinery that already exists for reasons unrelated to FFI. That is the design
working: the aliasing rule BLAS states in its own documentation ("C must not
overlap A or B") turns out to be the rule the borrow checker already enforces.

**`mutable borrowed` and `ffi.MutableSpan` lower with LLVM's `noalias`.** Rule 4
of §6.1 guarantees exclusivity on the Science side, so the attribute is justified
by the language. But it is also a claim about the C function: if the callee
internally aliases the pointer with another of its arguments, the attribute is a
lie and the optimizer is entitled to anything. For BLAS the claim matches the
published contract. For a library whose contract is silent, the binding author
must use `ffi.Pointer`, which carries no attribute. `SC0433` warns when a single
extern declaration takes two `MutableSpan`s of the same element type and the
library was not marked as promising non-overlap — a heuristic, but it fires on
exactly the shape that goes wrong.

**What this does not verify, and cannot.** Two obligations survive.

1. **That C does not retain the pointer past the call.** There is no way to
   state it and no way to check it. Writing `borrowed` in an extern declaration
   *is* the assertion, and the block's `unsafe` is what marks that an assertion
   was made. Where a library does retain — `cudnnSetTensorNdDescriptor` does not,
   but `cusparseCreateDnMat` holds the values pointer until the descriptor is
   destroyed — the parameter must not be `borrowed`. It must be a handle whose
   type holds the borrow as a field, so that the region engine keeps the buffer
   alive as long as the descriptor. That works, and §2.4 shows it.
2. **That the C function stays inside the buffer.** `cblas_dgemm` is given a
   pointer and, separately, `m`, `n`, `k` and `lda`. Nothing connects them. The
   length in `ffi.Span` is the raw material for a check, not the check itself;
   `covers` in §1.7 is the check, and it is ordinary Science code that a human
   wrote and that could be wrong. This is the single largest unverified surface
   in the design and it is not closable without dependent types over the C side,
   which do not exist.

### 2.3 Case B — C returns memory it expects us to give back

This is handles: `cudnnCreate`/`cudnnDestroy`, `H5Fopen`/`H5Fclose`,
`cudaMalloc`/`cudaFree`, `gsl_integration_workspace_alloc`/`_free`,
`fftw_plan_dft`/`fftw_destroy_plan`.

The claim worth making loudly: **C's "call the destructor exactly once" is
precisely Science's ownership, and the compiler verifies it.** This is not an
analogy. An affine value that must be consumed exactly once, whose consumption
happens at a program point the compiler can name, that cannot be used after
consumption and cannot be forgotten by accident, is what `Drop` plus §6.1 rules
2, 3 and 6 already produce. The FFI does not need new machinery for the case that
causes most of the resource bugs in real C code; it needs a type declaration.

```science
unsafe extern "C" library "cudnn" when available:
    def cudnnCreate(handle: mutable borrowed ffi.Uninitialized of CudnnHandle)
        returns CudnnStatus
    def cudnnDestroy(handle: CudnnHandle) returns CudnnStatus
    def cudnnGetErrorString(status: CudnnStatus)
        returns ffi.CStr from static

type CudnnHandle:
    raw: ffi.OpaqueHandle

CudnnHandle implements ffi.CLayout

CudnnHandle implements Drop:
    def drop(mutable self):
        # cudnnDestroy returns a status. There is no caller left to tell.
        unsafe: cudnnDestroy(self)

CudnnHandle has:
    public def new() -> (CudnnHandle?, CudnnError?):
        let mutable slot be ffi.Uninitialized of CudnnHandle .new()
        let status be unsafe: cudnnCreate(slot)
        let _, err be status.check()
        if err?:
            return (null, err)
        unsafe: (slot.assume_initialized(), null)
```

What the compiler now guarantees about every `CudnnHandle` in the program:

- It is destroyed exactly once, at a program point printable in a diagnostic.
- It is never used after destruction (`SC0301`).
- It is never copied — `CudnnHandle` implements neither `Copy` nor `Clone`, so
  there is no second handle to double-destroy.
- On every path out of a function, including early `return` and the error check
  of a later call, it is destroyed. There is no `goto cleanup` and no way to
  forget one.

`CudnnHandle?` costs one word, with the null pointer as `null` (§1.4), which is
what a fallible constructor wants and what the runtime ABI's niche rule already
gives.

**Handles over integers work the same way and are worth showing**, because HDF5's
`hid_t` is an `int64_t`, not a pointer, and its invalid value is negative rather
than zero:

```science
unsafe extern "C" library "hdf5":
    type Hid is I64
    def H5Fopen(name: ffi.CStr, flags: CUInt, fapl: Hid) returns Hid
    def H5Fclose(file: Hid) returns Herr

type H5File:
    id: Hid

H5File implements ffi.CLayout

H5File implements Drop:
    def drop(mutable self):
        unsafe: H5Fclose(self.id)

H5File has:
    public def open(path: borrowed String) -> (H5File?, H5Error?):
        let name, err be ffi.CString.from(path)
        if err?:
            return (null, H5Error.from_nul(err))
        let id be unsafe: H5Fopen(name.as_cstr(), H5F_ACC_RDONLY, H5P_DEFAULT)
        if id < 0: return (null, H5Error.from_stack())
        (H5File(id: id), null)
```

Note what the safe constructor is doing beyond wrapping: it turns a sentinel into
an `H5Error?` *before* the value ever becomes an `H5File`, so no `H5File` in the
program ever holds a negative id, so `drop` never calls `H5Fclose(-1)`. The
invariant lives in the one place that can establish it. `H5File?` costs two words
here rather than one, because an integer has no niche the compiler knows about —
the runtime ABI is explicit that only never-null pointers carry niches, and
inventing a "negative is the niche" rule for one library's typedef is
not worth a word.

**Device memory is this case, and it is where the core spec's central claim gets
cashed.** §6.5 argues that ownership matters for a scientific language because a
GC cannot see device memory pressure and ownership frees a device buffer at a
point the compiler can name. That claim is entirely about FFI, because device
memory is always allocated by a vendor library:

```science
unsafe extern "C" library "cudart":
    def cudaMalloc(out: mutable borrowed ffi.Uninitialized of ffi.DevicePointer of CVoid,
                        bytes: CSizeT) returns CudaError
    def cudaFree(pointer: ffi.DevicePointer of CVoid) returns CudaError

type DeviceBuffer of T:
    pointer: ffi.DevicePointer of T
    length: Int

DeviceBuffer of T implements Drop:
    def drop(mutable self):
        unsafe: cudaFree(self.pointer.erase())
```

`ffi.DevicePointer of T` is a distinct type from `ffi.Pointer of T` and neither
converts to the other. This is not fastidiousness: `cublasDgemm` takes device
pointers, has the same signature shape as `cblas_dgemm`, and handing it a host
pointer is accepted by the compiler, accepted by the linker, and produces either
a crash or — under unified memory — a correct answer on the development machine
and a wrong one in the cluster. The type distinction is the only thing that
catches it, and it costs nothing at run time because both are one word.

F2 will want `DeviceBuffer` to carry a device *id* as well, since a pointer from
device 0 is invalid on device 1. That is F2's problem; this design must only not
preclude it, and a const generic parameter on `DeviceBuffer` does the job.

### 2.4 The retaining case, which is Case A and Case B at once

Some libraries take a pointer and keep it. cuSPARSE descriptors hold their value
arrays. FFTW plans hold the input and output buffers they were planned against.
This is the case that looks like a borrow and is not a call-scoped one.

It is expressible, and the expression is: **the handle type holds the borrow as a
field.**

```science
type FftwPlan:
    raw: ffi.OpaqueHandle
    input: ffi.MutableSpan of F64      # region inferred; keeps the buffer alive
    output: ffi.MutableSpan of ffi.Complex64

FftwPlan implements Drop:
    def drop(mutable self):
        unsafe: fftw_destroy_plan(self.raw)
```

The region engine now knows that the plan borrows both buffers, so the buffers
cannot be moved, freed, or exclusively reborrowed while a plan exists, and
`fftw_execute(plan)` is sound because the plan's own borrows are what keep them
alive. This is §4.4's `type View: source: borrowed Doc` doing exactly what it was
designed for, applied to a resource that lives in another language.

The cost is that the plan is no longer `'static`-shaped: it cannot be returned
past the buffers' scope, cannot be stored in a long-lived structure that outlives
them, and will produce region errors that are correct and initially baffling. The
diagnostic obligation of §6.2 ("reconstruct where the borrow was born") applies
with full force here, because the borrow was born in a field of a type the user
did not write.

### 2.5 Case C — C takes ownership of what was ours

Rare, and the design's answer is restrictive on purpose.

The hazard is allocators. Science's heap is `science_alloc` over the system
allocator (`crates/science-rt/src/mem.rs`), and it frees with a matching size and
alignment. C frees with `free`, or with `cudaFree`, or with the library's own
deallocator. Handing a Science `Array`'s buffer to something that will call
`free` on it is heap corruption on every platform where the two allocators are
not literally the same one, which includes Windows whenever the library was built
against a different CRT.

**The rule: only memory that came from a foreign allocator may be given back to
one.** `ffi.CBuffer of T` is memory obtained from a named foreign allocator, owned
by Science, with a `Drop` that calls that allocator's free. It may be moved to C
with `move`:

```science
unsafe extern "C" library "somelib":
    def somelib_adopt(data: move ffi.CBuffer of F64, count: CSizeT) returns CInt
```

`move` is already reserved (§13, reserved-not-yet-used) and this is what it is
for. At the call site the argument is moved in the ordinary Science sense: the
binding is dead afterwards, using it is `SC0301`, and — the part that matters —
its `Drop` does **not** run, because it was moved. The ownership checker needs no
special case for this; `move` in a parameter position means the parameter is
taken by value, which it already understands.

**A Science `Array of T` cannot be moved to C.** Its allocation belongs to
`science_alloc`, and its representation is a three-word header C has never heard
of. Transferring one requires a copy into an `ffi.CBuffer`, and that copy is
visible in the source. The cost is one pass over the data on a transfer that is
already rare; the benefit is that the catastrophic version of this mistake is not
expressible.

The alternative considered and rejected was parameterizing `Array` by an
allocator, which would let a Science array be allocated from `malloc` and handed
over with no copy. It was rejected because it changes the F0 ABI — `Array` is
three words with an implicit global allocator, pinned by `crates/science-rt`'s
layout tests, and every container entry point takes a `ScienceTypeInfo` that has
no allocator slot. Adding one costs a word in every array in every program to buy
a zero-copy path for a case that arises in a handful of libraries. If F2 needs
arena allocation for device memory it should be a separate container type, not a
parameter on this one.

### 2.6 Case D — C lends us something it owns

Two subcases, distinguished by how long the loan lasts, and Science has no
lifetime syntax to say which. The `extern` grammar therefore has the one region
construct in the language, and it exists only here:

```science
    def cudnnGetErrorString(status: CudnnStatus) returns ffi.CStr from static
    def gsl_matrix_ptr(m: mutable borrowed GslMatrix, i: CSizeT, j: CSizeT)
        returns mutable borrowed F64 from m
```

- **`from static`** — the returned borrow outlives everything. `cudaGetErrorString`
  and `cudnnGetErrorString` return pointers to string literals in the library's
  data segment. Safe to hold forever; safe to copy from at leisure.
- **`from p`**, naming a parameter — the returned borrow's region is the region of
  `p`'s borrow. `gsl_matrix_ptr` returns an interior pointer into the matrix, and
  the region engine will now refuse to let it outlive the matrix, refuse a second
  exclusive borrow while it is live, and refuse to let the matrix be moved. All
  of that is rule 4 and rule 5 doing their normal work on a borrow that was born
  in C.

**Elision.** If the declaration returns a borrow and has exactly one borrowed
parameter, `from` may be omitted and that parameter is used. If it has zero or
more than one, `from` is **required** and its absence is `SC0426`. This is Rust's
elision rule minus the `self` case, chosen for the same reason: it is right
almost always, and where it is not, silence would be dangerous rather than merely
wrong.

The declaration is still a claim. `from static` on something that is not static
is undefined behaviour and the compiler cannot tell. But the claim is now written
in a place a reviewer can find, and on the Science side of it the region engine
is doing real work.

### 2.7 What cannot be expressed, and therefore lives behind `unsafe`

Stated as a list rather than dissolved into prose, because a reader should be
able to check whether their problem is on it.

1. **Retention.** That the callee does not keep a `borrowed` pointer past the
   call. Unstatable, unverifiable. The `borrowed` keyword in an extern
   declaration is the assertion.
2. **Bounds.** That `m`, `n`, `k`, `lda` are consistent with the buffer. The
   `ffi.Span` length is the material for a check the wrapper must write.
3. **Internal aliasing.** That the callee does not alias two arguments it was
   told were exclusive. `noalias` is emitted on the strength of the declaration.
4. **Signature agreement.** That the declaration matches the real header — the
   argument count, the widths, the struct layout, the calling convention. §5.3
   explains why the linker cannot catch this and §7 is the mitigation.
5. **Reentrancy through C.** §4.4. Partially checked; the check is local and
   incomplete.
6. **`setjmp`/`longjmp` across Science frames.** A `longjmp` past a Science frame
   skips every destructor between here and there: handles leak, and any value
   whose `Drop` released something the program later reuses is worse than leaked.
   Undetectable. Forbidden by convention, which is the only tool available.
   MINPACK-style error handlers and some Fortran `XERBLA` replacements do this.
7. **The library calling `exit` or `abort`.** MPI does on rank failure; MKL does
   on some parameter errors; reference `XERBLA` prints and stops. Nothing can be
   done and nothing is attempted.
8. **Signal handlers and global initialization order.** `MPI_Init` before
   anything, `cublasCreate` before any cuBLAS call, HDF5's error stack being
   process-global. These are sequencing obligations the type system does not
   model. The mitigation is that handle types make most of them into
   constructor/destructor pairs, which is why §2.3's shape is worth using even
   when the handle is a formality.
9. **Unions, bitfields, variadics** (§1.4).
10. **Thread safety.** F0 has no threads, so there is nothing to say yet. Note in
    passing that OpenBLAS and MKL spawn their *own* threads inside a call, which
    is invisible to Science and harmless: the threads live and die inside the
    call, and the exclusive borrow covers the whole of it.

---

## 3. What `unsafe` means

`unsafe` is a block (`unsafe:` with an indented body, or an inline `unsafe: expr`
following the rule in §4.5 of the core spec), a modifier on an `extern` block, and
a modifier on a function declaration.

### 3.1 The closed list of extra powers

Inside an `unsafe` block, and only there, six things become possible. The list is
closed; adding to it is a spec change, in the same spirit as §8's closed method
sets.

1. Calling a function declared in an `extern` block.
2. Calling a function declared `unsafe def`.
3. Dereferencing an `ffi.Pointer` or `ffi.DevicePointer`, and converting one to a
   borrow.
4. `assume_initialized` on an `ffi.Uninitialized`.
5. Constructing a handle type from a raw value obtained outside Science — the
   operation that asserts a pointer is a valid live handle.
6. `ffi.reinterpret`, the bit-level type pun, with an equal-size requirement
   checked at compile time.

That is all. Notably absent: `unsafe` does not permit aliasing a `mutable
borrowed`, using a moved value, indexing out of bounds, or skipping a `Drop`.

### 3.2 What the compiler still guarantees inside `unsafe`

Everything else. In full, because the value of the keyword depends on the list
being short and the guarantees being long:

- **Type checking and trait resolution are unchanged.** `unsafe` is not a cast
  and does not weaken inference. A type error inside an `unsafe` block is the
  same error it would be outside.
- **Ownership is unchanged.** Rules 1, 2, 3 and 6 of §6.1 hold. Use after move is
  `SC0301` inside `unsafe` exactly as outside, and values still drop at scope
  exit.
- **Borrow checking on Science values is unchanged.** Rules 4 and 5 hold.
  `unsafe` does not let you take two exclusive borrows. This is the guarantee
  most worth stating, because it is the one users of other languages assume is
  suspended.
- **Region inference is unchanged**, including on borrows returned from C under
  §2.6.
- **Arithmetic checks are unchanged.** Integer overflow panics in debug exactly
  as §5.1 says.
- **`Array` bounds checks are unchanged.** `ffi.Pointer` is unchecked; `Array` is
  not, and being inside `unsafe` does not make it so.
- **Exhaustiveness is unchanged.**

So `unsafe` in Science means one narrow thing: *the compiler's model of memory it
can see is intact; what you are about to do involves memory it cannot see.* The
block delimits the region of source a reviewer must read against the C
documentation. If an `unsafe` block is long, the binding is written wrong.

### 3.3 `unsafe def`

A function whose *preconditions* cannot be checked is declared `unsafe def`,
and calling it requires an `unsafe` block at the call site. This is how a binding
layer exposes something that genuinely cannot be made safe — a routine taking a
raw pointer and a length the caller must guarantee.

**An `unsafe def`'s body is not implicitly an unsafe block.** Inside it,
calling an extern function still requires writing `unsafe:`. Rust shipped the
opposite for eleven years and changed it, because the implicit version means the
riskiest functions in a codebase are the ones with no visible markers inside
them, which inverts the intent. Science takes the fixed version from the start;
the cost is one extra line in functions that are rare by construction.

`unsafe` is not part of a function's type and does not propagate. A safe function
may contain `unsafe` blocks; that is the normal case and the whole point.

---

## 4. Callbacks into Science from C

Every iterative numerical library needs this. GSL's integrators and root finders
take a `gsl_function`. SUNDIALS' CVODE takes a `CVRhsFn` for the right-hand side.
NLopt takes an objective. MINPACK takes `fcn`. LAPACK's `dgees` takes a `SELECT`
predicate. A language that cannot pass a closure to these can call the
convenience routines and nothing else.

### 4.1 The shape C uses

C's closure idiom is a pair: a function pointer with a fixed signature, and a
`void *user_data` the library passes back untouched.

```c
struct gsl_function {
    double (*function)(double x, void *params);
    void *params;
};
```

Science's answer is a matching pair type, `ffi.Callback`, with a generated
trampoline.

### 4.2 Function pointer types

The type of a C function pointer is spelled with reserved words only:

```science
ffi.FunctionPointer of (extern "C" def(F64, ffi.Pointer of CVoid) returns F64)
```

A plain Science function may be given the C ABI and a stable symbol by declaring
it `extern "C" def`, which makes it addressable from C. That is the
low-level path and it takes no captures.

### 4.3 Closures, and the trampoline

`ffi.Callback of ((A...), R)` is a two-field value `{ code, data }` that **borrows
a closure**:

```science
ffi.Callback.of(closure: mutable borrowed F) returns ffi.Callback of ((A...), R)
    where F: def(A...) returns R
```

`code` is a monomorphized trampoline — one per closure type, which
monomorphization (§5.3 of the core spec) provides for free — that casts `data`
back to the closure's type and calls it. `data` is the address of the closure.

The load-bearing property is the region: **the `Callback` holds a `mutable
borrowed` to the closure, so it cannot outlive it.** The obligation "C must not
retain this callback past the call" is thereby half-checked, on the half that
lives in Science: the callback value is region-bounded by the closure, and if a
user tries to store it somewhere longer-lived they get a region error.

```science
public def integrate(
        integrand: mutable borrowed (def(F64) returns F64),
        lower: F64,
        upper: F64,
        tolerance: F64)
        -> (Integral?, GslError?):

    let mutable workspace, err be GslWorkspace.new(1000)
    if err?:
        return (null, err)
    let entry be ffi.Callback.of(integrand)          # borrows `integrand`
    let f be GslFunction(function: entry.code(), params: entry.data())

    let mutable value be 0.0
    let mutable absolute_error be 0.0
    let status be unsafe:
        gsl_integration_qags(f, lower, upper, 0.0, tolerance, 1000,
                             workspace, value, absolute_error)
    let _, err be status.check()
    if err?:
        return (null, err)
    (Integral(value: value, absolute_error: absolute_error), null)
```

Used:

```science
let mutable gaussian be x giving exp(0.0 - x * x)
let result, err be integrate(gaussian, 0.0, 10.0, 1e-9)
if err?:
    panic(err.message())
```

### 4.4 Reentrancy, which is the hole

C calls back into Science while Science's frames are suspended below it. That
path is invisible to the borrow checker, and it means a closure can reach state
the same C call is simultaneously mutating.

The concrete failure: the closure captures `workspace`, and `workspace` is also
passed to `gsl_integration_qags` as `mutable borrowed`. Both are live at once.
Science's rule 4 was never violated in any single frame, because the second
access happens through C.

**A local check closes most of it, and it is cheap enough to specify.** At a call
to an extern function that takes both an `ffi.Callback` argument and a `mutable
borrowed` argument, the compiler checks that no place reachable from the
callback's closure captures overlaps the referent of any `mutable borrowed`
argument to the same call. That is a place-overlap query the region engine
already answers for ordinary calls; the only novelty is asking it across a
callback's capture set. Violations are `SC0410`.

**What the check does not cover**, and there is no version of it that does:

- A callback registered on one call and invoked from a *different* call
  (`H5Eset_auto`, `atexit`, `cudaStreamAddCallback`, a SUNDIALS right-hand side
  installed once and driven by many `CVode` steps). The two calls are separate
  program points and the overlap is not local.
- Reentrancy through a captured handle: the closure calls back into the same
  library, which touches the state the outer call is in the middle of. No type
  system sees this.

For registered callbacks the rule is different and stricter: a callback that
outlives the call **may capture no borrows at all.** It may capture owned values,
which are moved into a foreign-owned allocation whose pointer becomes
`user_data`. Recovering them requires an explicit unregistration that hands the
pointer back; not doing so leaks. Leaking is safe — Science guarantees `Drop`
runs *at most* once, not *at least* once — and leaking a solver's user data is
the correct trade against the alternative, which is a dangling pointer inside a
library's callback table.

### 4.5 Panics do not cross, and F0 made that free

§8 of the core spec says `panic` prints a message and **aborts**. There is no
unwinding in the language.

This is the property that makes the callback boundary sound with no machinery at
all. A Science callback that panics — an index out of bounds in a user's
objective function, an overflow in debug — aborts the process from inside the C
frame. It does not unwind through frames a C compiler generated without unwind
tables, which is undefined behaviour in every language that has tried it. The
message and exit code are what a user gets, and they are correct.

Recorded so that it is not lost: **if Science ever gains unwinding panics, every
`extern "C"` Science function and every generated trampoline must grow a catch at
the boundary that converts an unwind to an abort.** That is a codegen change, and
the place it belongs is the trampoline generator described in §4.3, so the design
does not preclude it. But while `panic` aborts, nothing is needed and nothing
should be built.

---

## 5. Linking

Science compiles ahead of time through LLVM to a native binary. An `extern`
declaration becomes an LLVM `declare` with the C calling convention and the
target's parameter attributes; no definition is emitted; the symbol is undefined
in the object file and the system linker resolves it. That is the whole mechanism,
and everything in this section is about the three decisions it leaves open.

### 5.1 Naming the library

`library "openblas"` names a link target. At the link step, `sciencec` collects
the set of library names reachable from the compiled program and passes each to
the system linker as `-lopenblas` (`openblas.lib` on MSVC). Search paths come
from, in order:

1. `--library-path DIR`, repeatable, highest priority.
2. The `SCIENCE_LIBRARY_PATH` environment variable, colon- or
   semicolon-separated per platform.
3. The system default paths.

`via pkg-config "openblas"` replaces steps 1–3 for that block with the output of
`pkg-config --libs`. This is supported, rather than left to the user, for one
reason: BLAS is the case where the correct flags are `-lopenblas` on one machine,
`-lblas -llapack` on another, and `-lmkl_intel_lp64 -lmkl_sequential -lmkl_core
-lpthread -lm -ldl` on a third, and no amount of source-level cleverness gets
that right. HPC sites ship `.pc` files precisely so that this works. The cost is
that `pkg-config` is not universally present, notably on Windows, so a
`--link-arg` escape hatch exists and a block using `via pkg-config` must still
declare a plain `library` name as the fallback.

`kind static` on the library clause forces static linking. Dynamic is the
default, because vendor libraries ship as shared objects and because HPC module
systems work by swapping the shared object under a fixed name — statically
linking OpenBLAS defeats the mechanism the cluster administrator is relying on.

### 5.2 Optional libraries: `when available`

A program that can use a GPU must still start on a machine without one. If
`libcudnn.so` is a hard link dependency, the binary fails to load on every
CPU-only node, which is most of them.

`library "cudnn" when available` changes the lowering for that block: instead of
direct `declare`s, the compiler emits a lazily-initialized table of function
pointers, populated on first use by `dlopen`/`dlsym` (`LoadLibrary`/
`GetProcAddress` on Windows) inside `science-rt`, and every call becomes an
indirect call through the table. The block gains a generated
`is_available() returns Bool` and every function in it becomes a call that
panics with a clear message if the library is missing.

- **Cost**: one load, one branch, one indirect call per foreign call. Against a
  `dgemm` this is noise. Against a per-element function it would not be, which is
  why it is opt-in rather than the default.
- **Rejected alternative**: weak symbols with `-Wl,--as-needed`. It works on
  Linux, behaves differently on macOS, and does not exist in the same form on
  Windows. A runtime loader in `science-rt` is the same code on all three.

### 5.3 What linking checks, and what it does not

**Checked**: symbol existence. A typo in a `symbol` string, a missing library, an
ILP64/LP64 mismatch where the vendor used distinct symbol names (§1.6) — all are
undefined-symbol errors. `sciencec` must catch the linker's output and render
them as Science diagnostics (`SC0461`) naming the `extern` declaration's span and
the library clause, not pass `ld`'s output through. For a language whose spec has
a section on diagnostics and a testing layer dedicated to their exact rendered
text, forwarding a raw linker error would be a visible inconsistency.

**Not checked**: everything about the signature. C object files carry no type
information. A declaration with the wrong argument count, the wrong integer
width, the wrong struct layout, or a by-value aggregate the target ABI classifies
differently links cleanly and corrupts the stack at run time. This is the largest
practical risk in the whole design, and it is the entire argument of §7.

### 5.4 Aggregate passing and returning

The runtime ABI documentation already establishes the rule for Science's own
runtime calls: aggregate returns follow the platform C ABI, not LLVM's structural
return, and codegen must emit an `sret` parameter for anything the target
classifies as MEMORY. Extern declarations inherit that rule without modification,
and also inherit the harder half of it: **by-value aggregate *arguments* must be
classified per the target ABI too** — the System V eightbyte classification,
Windows x64's "aggregates larger than 8 bytes go by hidden pointer", AAPCS64's
HFA rules.

This is where FFI implementations go wrong most often, and the implementation
note is: do not write a classifier. Reuse an existing one. A wrong classification
is not a compile error and not a link error; it is a program that works for
structs of two floats and fails for structs of three.

The mitigation available in the meantime is a restriction, and it is worth
taking: **in the first implementation, `extern` functions may not take or return
`ffi.CLayout` aggregates by value** (`SC0429`). Pointer-to-aggregate only. Every
library in the target set can be used under that restriction — BLAS, LAPACK,
HDF5, cuDNN and MPI pass aggregates by pointer throughout; GSL's `gsl_function`
is passed as `const gsl_function *`. The restriction is lifted when a classifier
is in place, and lifting it is source-compatible.

### 5.5 Cross-language LTO, and version drift

LTO across the boundary would require the C library to have been compiled to
bitcode by a compatible LLVM. Vendor libraries are shipped as binaries. Not
supported, and the spec already puts cross-compilation out of scope, so this is
consistent rather than an additional retreat.

Nothing prevents linking against a library whose ABI differs from the headers the
declarations were written against. The available mitigation is cheap and should
be adopted by generated bindings (§7): record the header version at generation
time, and emit an initializer that calls the library's own version query
(`cudnnGetVersion`, `H5get_libversion`, `openblas_get_config`) and panics on a
mismatch beyond the library's stated compatibility range. It catches the common
case — a cluster module load that picked up a different build — and it costs one
call at startup.

---

## 6. Errors

C reports failure in at least five different ways, and Science has `Error?`. The
decision is about where the conversion happens.

### 6.1 The compiler converts nothing

An `extern` declaration returns exactly what the C function returns. No status
code becomes an `Error?` automatically, no null pointer becomes `null`, no
negative becomes a failure.

The reason is that the mapping is a property of the library, not of C:

| Library | Convention | What "failure" means |
|---|---|---|
| CBLAS | none — `void` | errors go to `xerbla`, which prints and stops |
| LAPACK | `info` out-parameter | `< 0` is a bad argument; `> 0` is **domain information** |
| cuDNN, cuBLAS | `cudnnStatus_t` enum, `0` is success | |
| CUDA runtime | `cudaError_t`, `0` is success, and a *sticky* global last-error | |
| HDF5 | `herr_t < 0`, or `hid_t < 0`, plus a process-global error stack | |
| POSIX | `-1` or `NULL` plus `errno` | |
| GSL | `int` status plus an installable error handler that `abort`s by default | |

LAPACK settles the argument on its own. `dgesv` returns `info > 0` to mean the
factorization completed and `U(i,i)` is exactly zero — the matrix is singular.
That is not an error in the library's sense; it is the answer. Whether it becomes
a `Singular` error or a value with a flag is a decision about what the *Science*
API means, and the compiler has no basis for making it. `dgeev` uses `info > 0`
for partial convergence, where the eigenvalues in the first `info` positions are
valid and the caller may well want them. Any automatic rule is wrong for at least
one of these.

### 6.2 The convention: a trait, and a wrapper

The `ffi` module provides:

```science
interface ForeignStatus:
    type Error
    def check(self) -> ((), Self.Error?)
```

A binding implements it once per status type, and every wrapper then reads the
same way:

```science
CudnnStatus implements ForeignStatus:
    type Error is CudnnError
    def check(self) -> ((), CudnnError?):
        if self.raw is 0: return ((), null)
        ((), CudnnError(status: self, message: cudnn_message(self)))
```

and at every call site, the same three lines:

```science
let _, err be status.check()
if err?:
    return (null, err)
```

That propagation is the point: the FFI does not introduce an error-handling
mechanism, it feeds the one the language has. But this is the place in the note
where revision 2 costs something, and it should be said plainly. Under `try` the
call site was one token. Under the Go model it is three lines, at every one of
the hundreds of foreign calls a real binding wraps, and the three lines are
identical every time. That is §3.3's verbosity tax, and the FFI is where the
language pays most of it: nothing in `ffi.ForeignStatus` can shorten the check,
and nothing in this note pretends otherwise.

`ffi.ForeignStatus` is the only error machinery in the `ffi` module. Nothing
converts, nothing is implicit, and the only thing gained over writing `if` by
hand is a uniform name so that generated bindings and hand-written ones look the
same.

### 6.3 `errno`, and the guarantee that makes it usable

`errno` is a thread-local that any intervening call may clobber, including a
destructor Science ran on the way out of a scope. Code like

```science
let r be unsafe: some_posix_call(...)
if r < 0: return (null, ffi.errno())           # WRONG
```

is a real bug, because between the call and the read there may be drop glue, a
bounds check's panic path, or in a future phase an allocation.

The `extern` grammar therefore has an errno clause, and the compiler makes a
guarantee about it:

```science
unsafe extern "C" library "c":
    def open(path: ffi.CStr, flags: CInt) returns CInt with errno
```

A declaration `with errno` changes the Science-level return type to
`ffi.Errno of CInt`, a two-field record `{ value, errno }`. **The compiler
guarantees that the `errno` read is emitted immediately after the call, with no
intervening call of any kind — no destructor, no allocation, no panic path.**
That is a codegen constraint, not a convention, and it is the only way the value
is worth reading.

`with last error` is the Win32 analogue, lowering to `GetLastError()` under the
same adjacency guarantee. A declaration may use one or the other, not both.

### 6.4 Sentinel returns

Null and negative sentinels get no language support. `ffi.Pointer of T` has
`.to_borrow()` returning `(borrowed T)?`, which is the null check written once;
the negative-`hid_t` check in §2.3 is three characters of Science. Both belong in
the safe constructor, where they establish the invariant that the rest of the
type depends on. Putting them in the language would mean the language deciding what
`-1` means, which is the mistake §6.1 declined to make in a larger form.

### 6.5 Errors the boundary cannot report

A C library that calls `abort` or `exit` produces no error value, and no wrapper
is possible. MPI's default error handler aborts; MKL aborts on some parameter
errors; reference BLAS's `XERBLA` prints and stops. Where a library allows
installing a handler — GSL's `gsl_set_error_handler`, MPI's
`MPI_Comm_set_errhandler`, HDF5's `H5Eset_auto` — the binding **should** install
one during initialization, because the alternative is a scientific program that
dies at hour nineteen of a run with a message on stderr and no stack. The handler
is a registered callback under §4.4's stricter rule: it captures nothing
borrowed.

---

## 7. Binding generation

The question as posed: writing `extern` declarations by hand for cuDNN is not
viable. It is worth being precise about the size. cuDNN's public headers declare
roughly 1,500 functions across about 200 types. CUDA's runtime API is about 500.
LAPACK is about 1,700 routines. HDF5's public C interface is over 400. Nobody
transcribes that, and more to the point, nobody *reviews* a transcription of
that, so the errors §5.3 says the linker cannot catch would all be present.

### 7.1 The decision

**Generate, but out of band.** A separate tool, `sciencec bindgen`, reads C
headers with libclang and emits `.science` source that is **checked into the
repository** and compiled like any other source. The compiler never reads a C
header.

Three reasons, each with a cost.

**Reason one: making libclang a dependency of every compilation breaks two things
the core spec already committed to.** `sciencec` is meant to produce
self-contained native binaries and to be a self-contained tool; linking libclang
into it, or requiring a matching system libclang, is a large dependency for a
feature most compilations do not use. Worse, compilation results would then
depend on the system's header set — on `/usr/include` and on whatever CUDA
version the module system loaded — which directly attacks the
environment-reproducibility story §12 of the core spec names as *the single
most-cited reason scientists trust results across machines*. A compiler whose
output depends on unpinned system headers is not a compiler that story can be
built on.

**Reason two: the salsa query graph would have to model the C preprocessor.**
§7.3 makes every phase a memoized query whose invalidation is exact, and tests
that assert on execution counts. Adding header parsing means the dependency graph
must track every transitively `#include`d file, every `-D` on the command line,
and the include search order, or incremental compilation gives stale answers. It
is achievable and it is a large amount of machinery in service of a phase the
design does not otherwise need.

**Reason three, and the one that actually decides it: C headers do not contain
what the ownership boundary needs.** This is worth stating flatly, because it is
the reason "generate bindings" is not the end of the problem:

> `double *a` does not say whether it is read or written, whether it points at
> one element or a million, whether the callee retains it, or who frees it. Every
> distinction in §2 is absent from the header.

Therefore the generated output must be **maximally conservative**: every pointer
becomes `ffi.Pointer of T`, never `borrowed` and never `ffi.Span`; every function
is in an `unsafe extern` block; no `Drop` is implemented; no `from` clause is
emitted; every enum is a newtype over `CInt` per §1.5. Generation produces the
*shape* — names, arity, scalar widths, struct layouts, enumerator values,
`#define` constants — which is exactly the part that is mechanical, voluminous,
and where hand transcription introduces silent stack corruption. It does not
produce the *contract*, and it must not pretend to.

The safe layer on top — §1.7's `gemm`, §2.3's `CudnnHandle`, §4.3's `integrate` —
is hand-written, small, and reviewable. For cuDNN, 1,500 generated declarations
support perhaps 60 hand-written safe wrappers, because a Science program does not
call 1,500 cuDNN functions. That ratio is the argument: generation handles the
part whose size is the problem, humans handle the part whose difficulty is the
problem.

### 7.2 Costs of this choice, stated

- **Generated source is checked in and must be regenerated on a version bump**,
  and the diff is large and unreadable. Mitigation: generated files carry a
  header naming the tool version, the library version, the header path and the
  flags, so that regeneration is reproducible and a reviewer can tell at a glance
  what changed underneath.
- **`bindgen` must exist and be maintained**, including the parts of C that are
  hostile: `#define` constants that are expressions, anonymous structs, function
  macros (`H5T_NATIVE_DOUBLE` is a macro expanding to a global variable
  dereference, and no header parser recovers it as a constant). These need a
  per-library allowlist and some hand-written supplements, which is what every
  binding generator in every language ends up doing.
- **Hand annotations must survive regeneration.** The tool takes a sidecar file
  keyed by function name, recording the things headers do not carry — "parameter
  3 is a span of length parameter 5", "the return borrows parameter 1", "this
  handle's destructor is `H5Fclose`". Regeneration reapplies it. Without a
  sidecar, every version bump loses the annotations, which is the failure that
  makes binding generators get abandoned.

### 7.3 Where better sources than C headers exist, use them

**LAPACK is the case that rewards this.** LAPACK's reference implementation is
Fortran, and its subroutine declarations carry `INTENT(IN)`, `INTENT(OUT)` and
`INTENT(INOUT)` — which is precisely the `borrowed` / out-parameter / `mutable
borrowed` distinction that C headers lack. The documentation comments also state,
in a fixed machine-parseable format that LAPACK has maintained for decades, each
array argument's dimension in terms of the other arguments: `A is DOUBLE
PRECISION array, dimension (LDA,N)`. That is the span length, in the source, for
1,700 routines.

So: **generate LAPACK bindings from the Fortran sources, not from `lapacke.h`,**
and they come out with real `borrowed`/`mutable borrowed` annotations and real
span lengths, which means the safe layer for LAPACK can be largely generated too.
That is a much better outcome than cuDNN's, and it is available only because
somebody else wrote the annotations down thirty years ago.

The general rule: prefer any machine-readable interface description the vendor
ships over the C header, because the header is the one artifact guaranteed to
have thrown the contract away.

### 7.4 The alternative that was rejected, and what it cost

**Runtime FFI with libffi — Julia's `ccall`, Python's `ctypes`.** No bindings at
all: name the library, name the symbol, name the types at the call site, and a
runtime assembles the call frame.

Rejected for three reasons. It defeats AOT compilation, since the call frame is
built at run time. It defeats the verification thesis completely — there is
nothing to check, and a wrong signature is discovered by a segfault. And it costs
a dynamic dispatch and a frame construction on every call, which is invisible on
`dgemm` and ruinous on a per-element kernel.

But the cost of rejecting it should be named, because it is real: **Julia users
call a C library by typing one line, and Science users generate a binding first.**
That is a genuine ergonomic loss at exactly the moment a new user is deciding
whether the language is worth it. The compensations are that the signature is
written down once in a place a reviewer can find rather than at every call site,
that the mistake is a link error rather than a segfault where the symbol is
misspelled, and that the safe wrapper layer — which `ccall` has no place to put —
is where all the ownership checking in §2 lives. On balance this is the right
trade for a compiled language whose thesis is verification. It is not a free one.

---

## 8. Diagnostics allocated

The core spec assigns `SC0300`–`SC0399` to ownership and regions and
`SC0400`–`SC0499` to codegen and linking. FFI errors split across both, so
sub-ranges are reserved here.

| Range | Contents |
|---|---|
| `SC0380`–`SC0399` | Ownership and regions at the foreign boundary |
| `SC0420`–`SC0459` | `extern` declaration and FFI type errors |
| `SC0460`–`SC0479` | Linking |

Named in this document:

| Code | Meaning |
|---|---|
| `SC0301` | (existing) use after move — applies unchanged inside `unsafe` |
| `SC0302` | (existing) conflicting borrows — what catches `gemm(a, b, a)` |
| `SC0410` | A closure passed as a callback captures a place also passed `mutable borrowed` to the same call |
| `SC0421` | `Array of T` in an extern signature; suggests `.span()` |
| `SC0424` | A type implementing `ffi.CLayout` has a field that is not FFI-representable |
| `SC0426` | A returned borrow needs a `from` clause and has none |
| `SC0429` | By-value aggregate argument or return, not yet supported (§5.4) |
| `SC0431` | `F16`/`BF16` passed by value across the boundary |
| `SC0433` | Two `MutableSpan`s of the same element type in one declaration (warning) |
| `SC0461` | Undefined symbol at link, rendered against the `extern` declaration's span |

---

## 9. Summary of decisions

| Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|
| `unsafe` on the `extern` block | Safe-looking `extern` blocks | The declaration is itself an unverifiable claim | One keyword on every block |
| Extern block is a small separate grammar with contextual keywords | Reusing Science's declaration grammar unchanged | C's type system is not Science's | A sublanguage to specify and document |
| `ffi.Span` carries a length that does not cross | `borrowed T` alone | The length is the material for the bounds check the wrapper must write | A two-word value where C has one |
| `borrowed Array of T` coerces to `Span` at extern call sites | Explicit `.span()` | Same argument as auto-borrow (§6.3) | One more implicit conversion |
| Handles are ordinary types with `implements Drop` | A dedicated `extern type` with a destructor clause | C's exactly-once destructor obligation *is* Science's ownership | None |
| C enums import as integer newtypes, never `choice` | Import as `choice` | A newer library returns values the header did not declare | No exhaustive `match` on a C status |
| Only foreign-allocated memory may be given to C | Allocator-parameterized `Array` | Mismatched allocators corrupt the heap; the ABI has no allocator slot | One copy on transfer-out |
| `from p` / `from static` on returned borrows | Inference | With no body there is nothing to infer from | The one region construct in the language |
| No automatic error conversion | Status codes become `Error?` automatically | LAPACK's `info > 0` is an answer, not an error | Every binding writes `check()` |
| `with errno` guarantees adjacency | A library `errno()` function | Any intervening call clobbers it, including drop glue | A codegen constraint |
| `when available` lowers to a dlopen table | Weak symbols and `--as-needed` | GPU libraries must be optional; weak symbols differ on three platforms | An indirect call per foreign call |
| Named arguments permitted at extern call sites | Positional only | Thirteen-argument `dgemm` with two identical-looking flags | Two call conventions in one language |
| Generate bindings out of band, conservatively | Compiler reads headers; or runtime FFI | Reproducibility, and headers do not carry the contract | Users generate before calling, where Julia types one line |
| Generate LAPACK from Fortran, not `lapacke.h` | Uniform C-header generation | Fortran `INTENT` and dimension comments *are* the contract | A second generator backend |
| No by-value aggregates in the first implementation | Write an ABI classifier now | Misclassification is neither a compile nor a link error | A restriction, source-compatibly lifted later |

---

## 10. Open questions for the core team

Four places where this design assumes something outside its territory.

1. **Closure type syntax.** §4.3 writes `def(F64) returns F64` as a type. The
   core spec defines closure *expressions* and never their types. If the core
   settles on a different spelling, `ffi.Callback`'s bound follows it.
2. **Named arguments at call sites.** §1.7 permits them for extern calls only.
   The core may prefer them everywhere or nowhere.
3. **`ffi` as a module in a language whose F0 library is closed.** Everything here
   requires a new standard-library module with about twenty items. That is a spec
   change to §8 and is requested as one, not assumed.
4. **`borrows_live_at(point)`.** §6.4 of the core spec exposes it for F5's
   suspension points. §4.4's reentrancy check wants a related query — the set of
   places reachable from a closure's captures — and it would be worth building
   both against one interface rather than two.
