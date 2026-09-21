# Code generation and linking

**Status.** **Stage 1 is emitted.** `sciencec build hello.science` produces a
native executable that prints `hello, world` and exits 0, through every phase
from the lexer to the linker.

Every clause of what this line used to say has been overtaken:
`crates/science-codegen` exists and holds layout, the C ABI classifier,
mangling, descriptors, the runtime boundary and the monomorphisation walk;
`crates/science-codegen-llvm` holds the backend; and `sciencec build` is a
command. **No `Cargo.toml` names `inkwell`, `llvm-sys` or `cranelift`, and that
clause is now a commitment rather than an observation** — the backend is
`extern "C"` against `LLVM-C`, which is what Decision 1 requires of both
implementations, and the machine it was built on ships no `llvm-config` and no
static archives, so `llvm-sys` was not available even had it been wanted.

§10's stage list is amended in place where it named two things that do not
exist. Decisions 5, 8, 25 and 36 each acquired an amendment from being built
against; §11's free-code claim went stale; and three silent miscompiles — a
`main` that never wrote its return value, a null test that loaded a fat
pointer, and a relocation model that cannot link on Windows x64 — are recorded
at the sections that were silent about them.

**What this note is.** Everything below monomorphisation: the LLVM binding, the
lowering of MIR to LLVM IR, data layout, the C ABI, linking, debug info,
optimisation, the project's first written-down performance targets, and the
order in which the above is built so that each step produces a program that
runs.

**What it is not.** It does not redesign anything
`type-checking-and-mir.md` decided. That note hands this one monomorphised,
fully typed MIR with an explicit CFG, explicit places, explicit storage markers
and explicit drop points, and this note consumes it as given. It does not
re-litigate `region-inference.md` either: the program arriving here is proved
memory-safe, and no lifetime check, no drop flag for use-after-free, and no
aliasing check is emitted. §2.6 is careful about what that guarantee does and
does not cover, because the most expensive mistake available in this section is
to read "memory-safe" as "no runtime checks".

**The one sentence that matters.** `self-hosting.md` §1.2:

> **Science cannot produce an executable. Until it can, self-hosting is not a
> plan, it is a preference.**

This note is what removes that sentence. It is also, per §2 of that note, the
*blocking* item and not the *risky* one — which is a licence to be boring
everywhere except §1 and §4, and this note takes it.

---

## 0. The frame, and the three constraints that are not negotiable

Three notes bracket this one and each hands over something fixed.

`type-checking-and-mir.md` fixes the pipeline above: types → THIR analyses →
MIR → regions → mono → codegen (its Decision 25). Its Decision 5 says a
whole-array operation lowers to a *call*, never an inlined MIR loop, which
means codegen never sees an array operation as a loop and F1's array IR stays
possible. Its Decision 6 fixes `T?`: a null niche for pointer-like `T`, a
discriminant byte otherwise. Its Decision 13 fixes `(any I)?` at two words.
Its §8 item 4 fixes the monomorphisation key at the const-expression normal
form, and §15 warns that getting that key wrong "is not a type error, not a
link error on most platforms, and shows up as a performance mystery or a
pointer-equality failure long after the cause". §11 below turns that warning
into a diagnostic.

`region-inference.md` guarantees that every borrow is live where it is used,
that rule 4 holds, and that no borrow outlives its referent. §4.4 below spends
that guarantee on an LLVM `noalias` attribute, which is the one place in this
note where a bug in *that* note becomes a wrong answer here rather than a
missed error there.

`crates/science-rt/src/lib.rs` is the floor. Its module documentation says it
is written so that "codegen can be emitted from this page alone, without
reading a single function body", and `self-hosting.md` §1.3 calls it "the most
bootstrap-ready artefact in the repository". §9 of this note tests that claim
by trying to do exactly what it invites, and reports what happened.

And three constraints that this note answers to. Two of them it did not choose;
the third did not exist until this note wrote it down, which is its own kind of
problem.

**The C ABI is the product.** `ffi-c-boundary.md` §0 and the whole of
`c-binding-coverage.md` exist because linking against BLAS, LAPACK, HDF5 and
cuDNN is the reason the language has a reason. A codegen that is *almost* right
about struct passing does not produce an error; it produces a `dgemm` that
returns plausible numbers. §4 is about that and it is the section to read
twice.

**Performance is a target this note is the first to write down.** Nothing in
`docs/` says what Science is supposed to be fast enough for, and every note
that trades performance for something — `reproducibility.md`'s float policy,
`type-checking-and-mir.md`'s call-not-a-loop rule for arrays,
`region-inference.md`'s SCC invalidation boundary — has been making that trade
against an unstated baseline. §8 states it, in two halves that point in
opposite directions, and §8.4 makes the one architectural decision that silence
would have foreclosed.

**Reproducibility is a language property, not a flag.** `reproducibility.md`
Decision 3 forbids every value-changing floating-point transform. That decision
is *implemented here or nowhere*: it is four lines of target configuration and
one whitelist, and every one of them is a line that is correct by *never being
written*, which is the worst kind of obligation. §7 makes them explicit and
testable.

---

## 1. The LLVM binding, and the consequence nobody has written down

This is the section the note exists for. Everything else in it is engineering.

### 1.1 What §7.1 says, and what it collides with

Core spec §7.1 draws the pipeline and the second-to-last line reads:

```
    |  codegen            inkwell -> LLVM IR
```

That was written when the question was "which Rust crate", and under that
question `inkwell` is a perfectly good answer: it is the safe, maintained,
idiomatic Rust wrapper over LLVM, it is what most Rust-hosted compilers use,
and it catches a real class of bug — passing an `IntValue` where a
`FloatValue` belongs — at Rust compile time rather than at LLVM verify time.

Two things collide with it. The first is small and the project has already
taken a position on it. `crates/sciencec/Cargo.toml` contains this comment, and
it is the only dependency-policy statement in the workspace:

> *No argument-parsing dependency. The surface is four subcommands and two
> flags, which is less code than the `clap` derive would be, and the workspace
> has exactly one third-party dependency today (salsa, in `science-db`) — a
> driver is the wrong place to double that.*

One third-party dependency, and the driver was refused a second. `inkwell`
brings `llvm-sys`, `libc`, `either`, `once_cell` and `static_assertions`; it
would take the workspace from one to six. That is an argument worth making and
it is not the argument that decides this.

### 1.2 The consequence: inkwell cannot survive the bootstrap

The project has committed to self-hosting. `science-codegen` will eventually be
written in Science. And:

> **`inkwell` is Rust. A Science program cannot call it.**

This has not been written down anywhere and it is the single most
consequential fact about the backend. `rust-interop.md`'s mechanism for
reaching Rust from Science is a shim crate exposing `extern "C"` functions with
`#[repr(C)]` types; it cannot reach a generic, lifetime-parameterised, safe
Rust API, and `inkwell`'s API is exactly that. `Context`, `Module<'ctx>`,
`Builder<'ctx>`, `BasicValueEnum<'ctx>` — every one of them is a borrow of the
context with a Rust lifetime parameter, which is the `'a` that
`region-inference.md` Decision 4 says Science has no way to write and no way to
infer. Even if Science could call Rust generics, `inkwell`'s ownership model is
the arena-handing-out-borrows shape that `region-inference.md` §9 names as the
one shape with no spelling in the language.

So a self-hosted `science-codegen` must call **LLVM's C API** — `LLVM-C`, the
stable, versioned, `extern "C"` surface that `llvm-sys` transliterates and that
`inkwell` wraps. That is not a compromise; it is the good case. LLVM-C is a
C library with opaque handles, `const char *` names and pointer-plus-length
array parameters, which is precisely the bucket `c-binding-coverage.md` §3
classifies as fully reachable through an `extern` block.

And now the observation that decides the section:

> **`inkwell` is not access to LLVM. It is ergonomics over the same C API
> Science will call.** `inkwell` wraps `llvm-sys` wraps `LLVM-C`. Anything
> `inkwell` can do, an `extern "C"` block can do; anything `inkwell` cannot
> reach — parts of the new pass manager's options, some `DIBuilder` corners,
> a handful of attribute kinds that live only in the C++ API — it cannot reach
> *because LLVM-C cannot*, and choosing `inkwell` does not buy a single call
> that the Science port will not have.

So the choice is not "safe wrapper versus raw C". It is "pay for a rewrite at
stage four, or don't".

### 1.3 Decision 1

> **Decision 1. `science-codegen` is written against LLVM's C API from the
> first line, in both implementations. The Rust implementation depends on
> `llvm-sys` and nothing else; `inkwell` is rejected. Core spec §7.1's
> `inkwell -> LLVM IR` is amended to `LLVM-C -> LLVM IR`.**

**The reason** is the bootstrap, stated as a cost comparison rather than a
preference.

`science-codegen` divides in two. There is the part that walks MIR, computes
layout, mangles symbols, builds type-info descriptors and decides what is a
call — which ports like any other compiler crate, under the index-not-pointer
discipline `region-inference.md` Decision 11 and `type-checking-and-mir.md`
Decision 24 already impose on their own crates. And there is the LLVM-facing
part, which is roughly half the crate by line count and is a long sequence of
calls of the shape `LLVMBuildGEP2(builder, ty, base, indices, n, name)`.

- **Under Decision 1**, that half is a *transliteration*. The Rust
  `LLVMBuildGEP2(builder, ty, base, indices.as_ptr(), indices.len() as u32, c"".as_ptr())`
  becomes the Science `LLVMBuildGEP2(builder, ty, base, indices.span(), indices.len(), "")`
  against an `extern` block with the same 200-odd symbol names in the same
  order with the same argument lists. The port is mechanical, reviewable
  line-by-line against the original, and differentially testable by comparing
  emitted IR text.
- **Under `inkwell`**, that half is a *rewrite*. Every call site changes shape,
  the ownership model changes from lifetime-parameterised borrows to raw
  handles held by value, and there is no line-by-line correspondence to review
  against. `self-hosting.md` Gate J requires that the two compilers produce
  byte-identical output, and a rewrite is the worst possible input to that gate
  because a discrepancy has no obvious place to look.

**The second reason, which is a dividend and not a cost.** Writing the Rust
codegen against LLVM-C makes `science-codegen` the project's largest and
earliest `extern "C"` consumer — before any Science program has ever called a C
function. The binding file that the Rust implementation uses is, literally, the
draft of the `extern` block the Science implementation will use. The project
that claims C interop as its thesis gets to exercise the thesis on its own
backend, in Rust, years before the thesis is testable in Science. That is
strictly better than discovering at stage four that the extern grammar cannot
express an array-of-pointers-plus-count parameter.

**The cost, stated plainly, and it is real.**

1. **You give up Rust's type checking over LLVM value kinds.** Everything is
   an `LLVMValueRef`. `LLVMBuildAdd` on an `i64` and a `double` is not a
   compile error; it is a verifier failure at best and a miscompile at worst.
   *Mitigation, and it is not optional:* **`LLVMVerifyModule` runs on every
   module in every build at every optimisation level, and a verifier failure is
   an internal compiler error that prints the offending function's IR.** That
   moves `inkwell`'s compile-time check to a run-time check that fires on the
   compiler's own test suite, which is where it would have fired anyway.
2. **You give up `inkwell`'s ownership discipline.** LLVM-C has real ownership
   transfers — `LLVMDisposeBuilder`, who owns a `Module` after it is added to
   an execution engine, when a `Type` is context-owned and must not be freed.
   *Mitigation:* a thin newtype layer, about 200 lines in each implementation,
   wrapping the half-dozen owning handles. It is the same 200 lines on both
   sides and it ports with everything else.
3. **`llvm-sys` is the workspace's second third-party dependency,** against a
   policy that refused a first. The argument for the exception is that
   `llvm-sys` is *not a library*: it is a transliterated header plus a build
   script that runs `llvm-config`. It contains no abstraction and adds only
   `libc`. It is the same artefact `ffi-c-boundary.md` §7.1 says a Science
   program obtains by generating bindings out of band and committing them —
   this note is adopting the project's own binding doctrine and letting
   somebody else have already done the generation.
4. **Decision 1 chooses LLVM as *a* backend, not *the* backend.** It is a
   decision about which library the first backend calls, and it deliberately
   does not weld LLVM into the compiler. **§8.4 is the decision that keeps it
   that way**, and it is the one that would have been foreclosed by writing
   this section and stopping.
5. **`llvm-config` becomes a build-time requirement for building `sciencec`.**
   On Linux and macOS this is a package; on Windows it means an LLVM
   installation and an `LLVM_SYS_181_PREFIX` environment variable. That is a
   real barrier for contributors and it is the price of a native backend. The
   alternative — vendoring LLVM — is a multi-gigabyte submodule and a C++
   toolchain requirement, which is worse.

**Rejected: write the `extern "C"` declarations by hand and depend on nothing.**
It is tempting, it is consistent with the policy, and it is about 4,000 lines
of transcription that must be re-transcribed on every LLVM upgrade. `llvm-sys`
is that transcription, maintained by someone else, versioned against LLVM
releases. Doing it by hand buys purity and costs the LLVM upgrade path.

**Rejected: Cranelift.** No debug info story worth the name, a much weaker
optimiser, no `-O2` for numeric code, and — decisively — it is a Rust library
with the same portability problem as `inkwell` and none of LLVM's C API. A
scientific language whose loops are 3× slower than C has no argument left.

### 1.4 Which LLVM, and what pins it

> **Decision 2. F0 targets LLVM 18.1.x, and the tested version is 18.1.8.**

**Why 18 and not newer.** Opaque pointers are settled (typed pointers were
removed in 17, so there is no `getPointerElementType` legacy to write around),
the legacy pass manager is gone so there is exactly one pipeline to configure,
and the C API surface for the new `PassBuilder` (`LLVMRunPasses` with a
pipeline string) is present and stable. A backend written against 18 has one
way to do each thing.

**Why 18.1.8 specifically, and this is the load-bearing half.**
`reproducibility.md` §5.2 prints the build record as a worked example and it
contains `"llvm": "18.1.8"`. Nobody decided that; it was an illustration. It is
now the only LLVM version written down anywhere in the corpus, and a note that
picks a different number silently invalidates a published example in the
document that defines what a reviewer checks. Ratifying the illustration is
free; contradicting it is not.

**What pins it, in three layers:**

1. **`llvm-sys`'s crate version** is the LLVM major version — `llvm-sys = "181"`
   for the 18.1 line. An LLVM upgrade is a code change, not a lockfile change,
   in exactly the way `science-db`'s `Cargo.toml` already argues for `salsa`
   ("the requirement is deliberately restricted to one minor: an upgrade is a
   code change, not a lockfile change"). This note adopts that sentence
   verbatim for `llvm-sys`.
2. **`package-manager.md` Decision 3's `[build]` table** records the LLVM
   version in `science.lock`, and a mismatch is a warning by default and
   `SP0022` under `--locked`. That decision's stated reason is this one's:
   *"codegen is not reproducibility-neutral"*.
3. **`reproducibility.md` §3.2's G2** observes that *"the `-ffp-contract`
   default differs between C compilers and between LLVM versions, so doing
   nothing means the policy changes underneath the language without anyone
   deciding it."* That makes the LLVM version a **reproducibility input**, not
   a build detail. It is why (2) is an error and not a note.

**Cost.** The compiler is pinned to a version of a dependency that ships a new
major every six months, and the pin is load-bearing for numbers rather than
merely for compilation. Upgrading LLVM is therefore a change that requires
re-running the float-policy tests of §7.4 and a `[build]` schema bump, not a
dependency bump. That is the correct amount of friction and it should not be
reduced.

### 1.5 How the self-hosted compiler links LLVM, and the promise it breaks

The Rust `sciencec` links LLVM's static archives, which is `llvm-sys`'s
default. The self-hosted one cannot comfortably do the same: Science's `extern`
grammar names *one* library per block (`ffi-c-boundary.md` §5.1), and a static
LLVM is ~100 archives with a link-order dependency that the grammar has no way
to express.

> **Decision 3. The self-hosted `science-codegen` declares
> `unsafe extern "C" library "LLVM-18":` and links `libLLVM` dynamically —
> `libLLVM-18.so`, `libLLVM.dylib`, `LLVM-C.dll`. The Rust implementation links
> statically. The asymmetry is deliberate and is survivable only because
> Decision 2 pins the version.**

The two implementations then call *the same code* at the same version through
the same C entry points, one resolved at link time and one at load time. Gate
J's byte-identical requirement survives because static and dynamic linkage of
the same LLVM produce the same IR and the same object code.

**The cost is a broken promise and it should be named.** Core spec §3 lists
"self-contained binaries" as a reason for choosing native AOT. The self-hosted
compiler is the one Science program in existence that is not self-contained:
it has a hard dynamic dependency on a 100-megabyte shared library. That is a
contradiction with §3, it is unavoidable, and every self-hosted compiler that
uses LLVM has it. It should appear in the compiler's own
`.science.provenance` as an ordinary native dependency under
`native-dependencies.md`'s rules, which is the honest way to carry it.

### 1.6 What the LLVM-facing surface actually is

For scale, because "call the C API directly" sounds larger than it is. An F0
code generator touches roughly:

| Area | Entry points, approximately |
|---|---|
| Context, module, target, target machine, emit | 25 |
| Types — integer, float, pointer, struct, function, array | 20 |
| Constants — ints, floats, strings, structs, arrays, globals | 25 |
| Functions, parameters, attributes, linkage, calling convention | 30 |
| Basic blocks and the builder's instruction set | 60 |
| Metadata and `DIBuilder` (line tables only, §6) | 20 |
| Pass manager, verifier, disposal | 10 |
| **Total** | **~190** |

One hundred and ninety `extern "C"` declarations, every one of them
handle-in/handle-out with scalar and pointer parameters. `c-binding-coverage.md`
measures this shape as the case that gets through cleanly. There are no C
macros in the path, no exported globals, no by-value aggregates, no varargs,
and no callbacks except the diagnostic handler, which F0 does not install.

---

## 2. Lowering MIR to LLVM IR

Nothing in this section is novel and that is the point. `self-hosting.md` §2
calls codegen *"a large, well-understood, heavily-precedented piece of
engineering with a written contract already sitting in front of it"*, and the
right response to that is to make conventional choices loudly rather than
clever ones quietly.

### 2.1 The unit of emission

> **Decision 4. One LLVM module per crate. No codegen units, no parallel
> codegen, and the module is populated in a deterministic order: every
> monomorphised item is emitted in sorted order of its mangled symbol name.**

**Reason.** `self-hosting.md` §15 names Gate J — "the first time anyone finds
out whether codegen is deterministic enough" — as an open risk, and identifies
hash-map iteration order inside the compiler as the unknown source. Sorting the
mono worklist by mangled name before emission closes that source by
construction rather than by testing, and it is three lines. One module also
makes §11's symbol-collision check a single pass over one symbol table.

**Cost, and it is the ordinary one.** No parallel codegen, so a large crate's
LLVM time is serial and LLVM holds the whole crate in memory. This is the
single largest compile-time lever F0 declines to pull, which makes it §8.3's
business; §8.5 says where codegen units sit, why they are F1, and why the CGU
count becomes a `[build]` table field the day they arrive.

### 2.2 Functions, the CFG and terminators

**Decision 5. Every MIR basic block becomes exactly one LLVM basic block, and
codegen merges nothing.** A MIR dump and an IR dump are then diffable against
each other, which is what Gate I and Gate J need. LLVM's `simplifycfg` cleans
up at `-O1` and above; at `-O0` the MIR structure stays visible in the IR,
which is a debugging feature and not an accident.

Terminators map straight through: `Goto` → `br`; `SwitchInt` → `switch`;
`Return` → `ret`; `Unreachable` → `unreachable`; a call terminator → `call`
followed by `br` to the success block; a `Drop` terminator → a call to drop
glue (§2.5) followed by `br`.

> **Amendment, owed since `type-checking-and-mir.md` Decision 26 and now paid.**
> A **flagged** `Drop` — `flag: Some(_)`, the local was conditionally moved —
> is the one terminator this section understated: it is a call *guarded* by a
> load and a branch, not a call followed by a `br`, and it costs a second LLVM
> block for one MIR block. `science-mir`'s §4 item 2 named the shape in
> advance — *"a flagged drop breaks the decision by becoming three blocks"* —
> and `science-codegen-llvm`'s `lower.rs` builds exactly those three: the MIR
> block's own terminator becomes the test, a block the crate invents holds the
> release call, and the MIR terminator's own `target` is reused as the third,
> because it already exists as a block and inventing a second one for
> "continue" would be the same block with a different name. An **unflagged**
> `Drop` is unaffected and still the one block this section describes: the
> local was never conditionally moved, so there is nothing to test.

> **Decision 6. Every Science function is emitted `nounwind`, and codegen never
> emits `invoke` or `landingpad`.**

This is not a choice so much as a consequence. `crates/science-rt/src/panic.rs`
is explicit: *"no destructor runs, no frame is popped, no landing pad is
emitted anywhere in a Science binary"*, and it gives the reason — F0 has no way
to catch a panic, so no program can observe an unwind. A panic edge in MIR
lowers to `call science_panic_bytes(@.panicmsg.N, len)` followed by
`unreachable`.

**Cost, and it is a one-way door for F1.** That note's own module documentation
says F1 will want unwinding, for a supervisor to restart a failed actor.
Adding it later means `invoke` at every call that might panic, cleanup paths
through every MIR block, unwind tables in every binary, and — the part that
touches this note hardest — the region checker acquiring a whole new set of
edges to reason about. The `nounwind` attribute is therefore an assertion that
is cheap to make and expensive to withdraw, and it is being made deliberately
because the alternative is building the machinery before the first Science
program runs.

> **Decision 7. Every Science function uses the C calling convention (`ccc`)
> with the platform's parameter attributes. There is no private fast
> convention.**

**Reason.** `ffi-c-boundary.md` §0: the C ABI is why the project exists. Two
calling conventions means two classifiers, and §4 is about how expensive one
classifier is. LLVM is free to change the convention of an `internal` function
during IPO, which recovers most of what `fastcc` would have bought without
codegen having a second convention to be wrong about.

### 2.3 Locals, places and projections

> **Decision 8. Every MIR local becomes an `alloca` in the function's entry
> block. Reads are `load`, writes are `store`. Codegen constructs no SSA.**

`type-checking-and-mir.md` §3.3 already decided this from the other side — *"No
SSA. LLVM does that, and doing it twice buys nothing."* This is that decision's
consequence in the backend, and it is what makes codegen a transliteration of
MIR rather than an SSA construction algorithm, which matters enormously for
§1.3's port: an SSA builder with a dominance frontier computation is the part
of a backend that is genuinely hard to port correctly.

**Cost.** `-O0` output is alloca-heavy and slow — every local round-trips
through memory. That is accepted: `-O0` exists for stepping through a program,
and §7 makes `-O2` the default so that nobody encounters `-O0` by accident.

A MIR place is a base local plus a projection list. Lowering starts from the
local's `alloca` and applies, per projection:

| Projection | Lowering |
|---|---|
| `.field` | `getelementptr inbounds` with the struct's field index |
| `[index]` | a bounds check (below), then `getelementptr inbounds` on the element type |
| `deref` | `load ptr` |
| enum downcast | `getelementptr inbounds` to the payload offset per §3.3 |

**Decision 9. Codegen lowers each place expression once per use and performs no
common-subexpression elimination.** GVN is LLVM's job; a codegen that tries to
be clever about redundant GEPs is a codegen with a second, untested optimiser
in it.

### 2.4 The bounds check, and what "memory-safe" does not cover

This needs saying because it is the easiest sentence in the project to
misread.

`region-inference.md` proves that no borrow outlives its referent and that rule
4 holds. **It does not prove that `i < xs.len()`.** Ownership and regions are
about *lifetimes and aliasing*; an index out of range is neither. A codegen
that reads "the program is memory-safe, emit no checks" and omits the bounds
check produces an out-of-bounds write, which is a memory-safety failure that
the language claims cannot happen.

> **Decision 10. An index projection emits a bounds check — `icmp ult` against
> the container's length, branching to a panic block that calls
> `science_panic_bytes` with a static message — unless a const-expression proof
> discharges it.**

The proof mechanism is `const-expression-arithmetic.md`'s: where the index and
the length are both const expressions in the `k + Σ cᵢ·aᵢ` normal form and the
comparison is decidable, the check is not emitted. In F0 that covers exactly
the cases where a shape is in the type, which is most of what F1 will care
about and very little of what F0 does.

**Cost.** A compare and a branch per index, which LLVM hoists out of a loop
when the trip count is provable and does not when it is not. This is the same
cost Rust pays and the same cost it is worth paying. `indexing-and-array-literals.md`
owns the surface syntax and should confirm this reading; it does not currently
say what an out-of-range index does at run time, and the answer needs to be in
that note as well as this one.

**What is genuinely not emitted, on the strength of region inference:** no drop
flags for use-after-free, no reference counts, no liveness bitmap, no
allocation canary, no poisoning of freed handles. The runtime's §1 says the
same from its side — *"calling a `_free` entry point twice, or not at all, is a
bug this crate will not catch […] a runtime safety net would hide the very
failures that prove it is working."*

### 2.5 Aggregates and drops

**Decision 11. A Science `type` lowers to a named LLVM struct with fields in
declaration order and C padding (§3.2). Assignment of an aggregate is
`llvm.memcpy` with the layout's size and alignment; assignment of a scalar is
`load`/`store`.**

Uniform, never wrong, and SROA turns small memcpys into element copies at `-O1`
and above. At `-O0` it is slow, per Decision 8's cost.

**Decision 12. Drop glue is an emitted `internal` function per monomorphised
type, named by the mangling of §2.7, whose body drops fields in reverse
declaration order and then calls the type's own `Drop` implementation if it has
one. A type whose fields transitively need no destructor gets no glue function
and no call.** For the library types, the glue is a single runtime call:
`science_string_free(%p)`, `science_array_free(%p, @typeinfo.T)`,
`science_map_free(%p, @mapinfo.KV)`, `science_box_free(@typeinfo.T, %p)` — and
note that the descriptor is the *second* argument for `Array` and `Map` and the
*first* for `Box`, which is §9.3's finding 4 and is not derivable from the
runtime's contract page.

**Decision 13. Drop elaboration is MIR's, not codegen's. Codegen sees only
unconditional `Drop` terminators.**

This is a genuine gap in `type-checking-and-mir.md` and it is worth being
precise about. That note's §12 guarantees *"drop points explicit before region
inference runs"*. It says nothing about the case that makes drops hard:

```science
def maybe_consume(flag: Bool, doc: Doc):
    if flag:
        consume(doc)
    # `doc` is dropped here on one path and moved on the other.
```

`doc` is live-on-drop along one edge and moved-from along the other. Something
must insert a boolean local, set it at the move, and test it at the drop —
this is rustc's drop elaboration and it is not optional. Codegen cannot invent
it, because by the time codegen sees a `Drop` terminator the branch structure
is already flattened and the information about which paths moved is gone. It
must be a MIR pass, and it must run *before* region inference, because
`region-inference.md` §10 requirement 5 says an implicit drop appearing after
region inference invalidates the result — and a drop flag's test is an
exclusive borrow at a point the user did not write.

**Recorded as an ask on `type-checking-and-mir.md` §12** rather than decided
here, because it belongs to whoever owns MIR. What this note commits to is the
consumer side: **codegen reads a drop flag as an ordinary `i1` local and emits
`br` on it. It does not know it is a drop flag.**

### 2.6 The runtime boundary: what is a call and what is inline

`crates/science-rt` has 45 `science_`-prefixed entry points. They are the whole
list.

> **Decision 14. Those 45 symbols are the only runtime calls F0 emits.
> Everything else is inline. No entry point is added to `science-rt` to make
> codegen simpler; the runtime page's §9 already states the principle —
> *"putting them here would cost a function call for something codegen emits as
> a handful of instructions."***

**Emitted inline, with no call at all:**

| Operation | Lowering |
|---|---|
| Integer and float arithmetic, comparisons, bit operations | The corresponding LLVM instruction |
| `and` / `or` | Short-circuit: a `br` and a `phi`, never `and i1` |
| `not` | `xor i1 %x, true` |
| `T?` construction and the `?` test | The null-niche compare or the discriminant load of §3.4 |
| Postfix `?` and the early return it implies | The same test plus a `br` to the return block. The runtime page's §9 places it here deliberately, and §5.4's pair layout is what the test reads |
| `match` | `switch` on the discriminant, arms in declaration order |
| Field projection, enum downcast | `getelementptr inbounds` |
| Indexing | Decision 10 |
| `null` construction | `null` for a pointer-like payload; a store of the discriminant otherwise |
| A closure value | A struct of `{ fn ptr, captures }`; a call through it is an indirect `call` |
| Static and dynamic interface dispatch | A direct `call` or a vtable `load` + indirect `call` |
| Vtables | `private unnamed_addr constant` arrays of function pointers |
| `ScienceTypeInfo` / `ScienceMapInfo` descriptors | One `private unnamed_addr constant` per monomorphised instantiation (§3.5 — this read §3.6, which does not exist) |

**Emitted as a runtime call:** every operation on `String`, `Array`, `Map`,
`Box`; `print`; `read_file` / `write_file`; `panic`; and allocation. That is
the whole boundary and it is exactly the 45.

**The one that looks inline and is not: string literals.**

> **Decision 15. A string literal lowers to
> `call science_string_from_bytes(@.str.N, len)` against a
> `private unnamed_addr constant [N x i8]`. Every evaluation of the literal
> allocates.**

**Reason, and it is a finding rather than a preference.** The obvious
optimisation is to emit a static `ScienceString` — `{ ptr: @.str.N, len: N,
cap: 0 }` — pointing at read-only data, and rely on the fact that
`science_string_free` calls `science_dealloc(ptr, cap, 1)` and `science_dealloc`
with `size == 0` is documented as a no-op. Freeing it would be harmless. It
looks free.

It is unsound, and the runtime's contract page does not say so. `ScienceString`
carries the invariant `cap == 0 ⇒ len == 0`, established by
`ScienceString::empty()` and relied on by the internal `reserve`: growing from
`cap == 0` routes through `science_realloc` with `old_size == 0`, which is
documented as *"nothing was allocated, so there is nothing to copy or
release"* — it allocates fresh and **copies nothing**. A static literal with
`len > 0, cap == 0` would therefore survive `free`, survive `len`, survive
`print`, and then, on the first `push_str`, silently lose its first `len` bytes
to uninitialised memory. The page's §7 says an empty container's pointer is
dangling and non-null; it never says that `cap == 0` also asserts `len == 0`.

**Cost.** A literal in a loop allocates on every iteration. That is a real
performance defect in exactly the place a scientific program writes formatted
output, and the fix is a runtime change this note is not permitted to make:
either a `science_string_from_static` returning a String the runtime will not
free, or a `reserve` that copies when `cap == 0 && len > 0`, plus the invariant
written on the contract page either way. **Filed as an ask in §13.**

### 2.7 Symbol names

> **Decision 16. Mangled names are `_S` followed by length-prefixed path
> components, followed by a length-prefixed encoding of the type and const
> arguments in the normal form of `type-checking-and-mir.md` §8 item 4. No
> hashes. Names are not shortened.**

**Reason for no hashes.** `package-manager.md` Decision 7 removes timestamps,
hostnames and absolute paths from output because they are reproducibility
hazards. A hash of a path is the same hazard wearing a disguise: it is stable
only if the path is, and the path is the thing that is not. A length-prefixed
encoding is longer, uglier, and deterministic from the source alone.

**Reason for keying on the normal form** rather than the written syntax:
`Matrix of (T, a * b)` and `Matrix of (T, b * a)` are one instantiation, and
keying on syntax emits two symbols for one function. That note's §15 calls this
"a correctness trap with a late failure". §11's `SC0404` is the trap-door.

**Cost.** Symbol names in the tens to low hundreds of characters, a `nm` dump
that is unpleasant to read, and no demangler in F0. `sciencec demangle` is in
§14's not-built list and should be roughly fifty lines whenever somebody wants
it.

> **AMENDMENT 4: Decision 16 says "path components" and never says whether a
> module is one.** It was written when a crate was one file, so the question
> did not arise. When `use` started loading files it arose immediately, and
> the two obvious answers each break something:
>
> - **Include every module.** Then one source compiled as `a.science` and as
>   `z.science` produces different symbols, because F0 has no `mod`
>   declaration and a module's name is its file's stem. That is the
>   reproducibility hazard this decision's own *"deterministic from the source
>   alone"* exists to refuse.
> - **Exclude every module.** Then `helper.value` and `deep.inner.value` in one
>   crate both mangle to `_S5value`. The monomorphisation set is keyed by the
>   symbol, so one definition silently replaces the other and both call sites
>   reach whichever survived. This is not hypothetical: it was found by a
>   program that built, linked, ran, exited 0 and printed `2` where the author
>   wrote `1 + 2`.
>
> **The decision: the entry module contributes no component; every other
> module contributes its name.**
>
> The two answers conflict only while *"a module's name"* means one thing, and
> it means two. The **entry** module is named after the file given on the
> command line — nobody wrote that name, renaming the file changes nothing
> else, and it is precisely what *"from the source alone"* excludes. Every
> **other** module is named by the `use` that reaches it: `use deep.inner` is
> source the author wrote, and renaming that file without editing the `use`
> does not compile. So the first is excluded and the second included, and both
> requirements hold.
>
> **Cost.** A symbol depends on which file is the entry, so compiling
> `helper.science` directly gives its definitions different symbols from
> compiling a `main.science` that imports it. That is the same fact as a crate
> having an entry at all. When F0 gains a `mod` declaration, or a package name
> the author writes, this amendment should be revisited: both would give the
> entry module a name from the source and remove the exception.
>
> **Where the rule lives.** Twice, unavoidably —
> `science_codegen::mono::Mono::path_of` and
> `science_codegen_llvm::Lowerer::path_components` — because the dependency
> runs the wrong way and `science-codegen` cannot call into the backend. The
> guard against drift is `science-codegen`'s `tests/mono.rs`, whose
> `the_walk_and_the_backend_agree_on_a_plain_symbol` compares the two
> directly; it is what caught the first attempt at this amendment, which had
> included the entry module.
>
> This also settles a gap the backend had documented and declined to fix —
> *"`hello.science` and a byte-identical copy called `prog.science` produce
> different symbols for the same source … it is not this crate's to repair"*.
> It was the same question and this is the answer.
>
> **§11's `SC0404` is the trap-door and it is now wired.** `Mono::collect` had
> always detected a collision and pushed the diagnostic; nothing in the
> compiler read the field it pushed to, which is why the wrong answer above was
> silent. `sciencec`'s driver reports it.

---

## 3. Data layout

The runtime's §5 already fixes most of this, and this section's job is to
generalise it to types the runtime never sees and to reconcile it with
`type-checking-and-mir.md` Decision 6.

### 3.1 Scalars

`Int` is `I64`, per the runtime's §4 and core spec §5.1. `I8`/`I16`/`I32`/`I64`
and their unsigned forms are the LLVM integers of that width; `F32` and `F64`
are `float` and `double`; `Bool` is `i1` in registers and `i8` in memory;
`Char` is a Unicode scalar value in a `u32`, which is what
`science_chars_next` writes. A pointer is `ptr` — LLVM 18 has only opaque
pointers, which removes an entire category of bug from §2.3's GEP lowering and
is one of Decision 2's reasons.

### 3.2 Aggregates

> **Decision 17. A Science `type` has C layout: fields in declaration order,
> each at the next offset that is a multiple of its alignment, the whole
> rounded up to the maximum field alignment. No field reordering, ever.**

**Reason.** The runtime's §2 already commits to it — *"Field order is
declaration order, padding is the C rule"* — for every type in the standard
library, and `rust-interop.md`'s `science_abi.rs` layout assertions and
`type-checking-and-mir.md` Decision 19's `ffi.CLayout` both depend on the rule
being the language's rather than an FFI annotation's. A language with two
layout algorithms has to explain which one applies, and the explaining is worth
more than the padding.

**Cost, and it is measurable.** Rust reorders fields to minimise padding and
wins real bytes on real structs — `{ u8, u64, u8 }` is 24 bytes under the C
rule and 16 under Rust's. Science pays the 8 bytes on every such type, forever,
in exchange for never having to say "except when". For a language whose data is
mostly `Array of F64`, the trade is cheap; for a language of small mixed
records it would not be.

A zero-sized type — `()`, an empty `type`, `Array of ()` — has size 0 and
alignment 1, contributes nothing to a struct, and is passed as nothing. The
runtime's §6 already handles `size == 0` without allocating.

### 3.3 The general enum rule

The runtime's §5.1 states it for the library's own types. Generalised to every
`choice` a user can declare:

> **Decision 18. A `choice` with more than one payload-carrying variant, or
> with no niche available, is a struct of a discriminant followed by a union of
> the variant payloads. The discriminant is the smallest unsigned integer that
> holds the variant count — `u8` to 256 variants, `u16` beyond — and sits at
> offset 0. The payload union begins at the next offset that is a multiple of
> the union's alignment. Discriminant values follow declaration order from
> zero. A variant with no payload contributes nothing to the union. An enum
> whose variants all carry no payload is the discriminant alone.**

That is the runtime's §5.1 with the variant count generalised. It applies
unchanged to `choice` declarations and to `T?` where `T` has no niche. It does
**not** apply to the `(T, Error?)` pair, which is not an enum at all but an
ordinary two-field struct laid out by Decision 17 — and that difference is
exactly the one the runtime's §8.1 records as *"a different representation — a
tagged union with one live payload against a struct with two"*, which is why
its §5.4 is a separate rule rather than a case of §5.1.

### 3.4 Niches: where they apply, and where they stop

> **Decision 19. The niche rule applies when exactly one variant carries a
> payload, every other variant carries none, and the payload's type has a
> niche. The enum is then the payload alone, and the payload-free variants take
> the first `n` values of the niche in declaration order.**

The niche-carrying types in F0, and the list is short:

| Type | Representation | Niche |
|---|---|---|
| `Box of T` | `ptr` | null |
| `borrowed T` | `ptr` | null |
| `mutable borrowed T` | `ptr` | null |
| A function pointer | `ptr` | null |
| `any I` (fat pointer) | `{ ptr, ptr }` | null in the **data** pointer |

Therefore `(Box of T)?`, `(borrowed T)?` and `(mutable borrowed T)?` are one
word with `null` meaning `null`, costing not one bit more than the payload —
which is `type-checking-and-mir.md` Decision 6's "null niche for pointer-like
`T`" and the runtime's §5.2, agreeing.

And `(any I)?` is two words, with the null in the data pointer, which is that
note's Decision 13. **This note ratifies it and adds the codegen obligation:
the vtable slot of a null trait object is undefined and codegen must never
load it — including on the path that tests for null, which must test the data
pointer and only the data pointer.** A codegen that loaded both words into a
register pair to compare against a zeroed pair would be reading uninitialised
memory, and it would work on every machine until it didn't.

**Where niches stop, and this is the part that surprises people:**

- **`Int?` is two words**: an `i8` discriminant, seven bytes of padding, an
  `i64`. There is no niche in an integer and F0 does not invent one by
  restricting ranges.
- **`Bool?` is two bytes.** A `Bool` is `i8` in memory with two valid values
  and 254 spare, which *is* a niche — but exploiting it means the layout
  algorithm must know per-type valid-value sets, which is the beginning of
  rustc's `Scalar::valid_range` machinery. **Not built in F0**, and the cost is
  one byte on a rare type.
- **`String?`, `(Array of T)?` and `(Map of K, V)?` are tagged.** The runtime's
  §7 deliberately makes an empty container's pointer non-null so that
  `(borrowed T)?`'s niche is unambiguous, and the consequence is that these
  three types offer no niche of their own. The runtime page states this and it
  is worth repeating because it looks like an oversight: a `String` with a null
  pointer would be a perfectly good `null`, and the reason it is not used is
  that the never-null invariant is doing more valuable work elsewhere.
- **`T??` does not exist.** `type-checking-and-mir.md` Decision 6 collapses it
  with `SC0520` before codegen sees it.

**Cost of the whole rule.** `Int?` at 16 bytes where a range-restricted niche
would give 8 is the common case in a language whose error model puts `T?`
everywhere. It is accepted because the alternative is a valid-range lattice
that every layout query, every FFI check and every debug-info type must agree
with, and F0 has neither the callers nor the test surface for it.

### 3.5 The descriptor, and the one place a codegen bug is silent

The runtime's §6 is the element-descriptor convention: one implementation of
`Array` and `Map`, parameterised at run time by `{ size, align, drop_fn }`, so
that monomorphisation does not multiply the container. The page states the
precondition in a block quote and it is the sharpest sentence in the crate:

> *Every call touching a given container must pass the same descriptor contents
> that were passed when it was created. Passing a descriptor for a different
> type reinterprets the container's bytes as that type. Nothing detects this.*

> **Decision 20. Codegen emits exactly one `private unnamed_addr constant
> ScienceTypeInfo` per monomorphised element type, keyed by the
> monomorphisation key of §2.7, and every call site loads the address of that
> one global. A descriptor is never constructed at a call site and never
> constructed twice.**

That turns the runtime's unchecked precondition into a property of the symbol
table: two call sites pass the same descriptor because they pass the same
symbol. It also makes the precondition *testable* — a test can assert that the
module contains exactly one `ScienceTypeInfo` per distinct mono key — which is
better than a convention nobody can check. `unnamed_addr` lets LLVM merge two
descriptors that happen to be bit-identical, which is harmless because the
runtime compares contents and not addresses.

The `drop_fn` slot is the drop glue of Decision 12, or `null` when the type
needs none. **The `null` case matters for performance and the page says why**:
it lets the runtime skip the per-element destructor loop entirely when freeing
an `Array of F64`, which is the common case in this language. A codegen that
emits a do-nothing glue function instead of `null` is correct and is quietly
slower on the hottest container in the language.

---

## 4. The C ABI

### 4.1 Why this is the dangerous section

`ffi-c-boundary.md` §5.3 says it in one sentence and it is worth quoting
because the whole section is a response to it:

> *A declaration with the wrong argument count, the wrong integer width, the
> wrong struct layout, or a by-value aggregate the target ABI classifies
> differently links cleanly and corrupts the stack at run time. This is the
> largest practical risk in the whole design.*

Three conventions, three different wrong answers:

- **System V x86-64** classifies an aggregate by splitting it into eightbytes
  and classifying each as INTEGER, SSE or MEMORY, with a merge rule, a
  size cut-off at 16 bytes, and special cases for unions, bitfields, `__m256`
  and `long double`. A `{ double, double }` goes in `xmm0`/`xmm1`; a
  `{ double, double, double }` goes on the stack. A classifier that gets the
  merge rule right for two members and wrong for three produces a library that
  works on `Complex64` and fails on a 3-vector.
- **AArch64 (AAPCS64)** has the HFA and HVA rule: an aggregate of up to four
  members, all the same floating-point type, is passed in consecutive `v`
  registers *as members*, not as bytes. `{ float × 4 }` goes in
  `v0`–`v3`; `{ float × 5 }` goes indirectly. Nested structs flatten into the
  count. Apple's variant differs again on argument extension and on varargs.
- **Windows x64** is the simplest and therefore the easiest to get wrong by
  assuming it is like the others: anything that is not exactly 1, 2, 4 or 8
  bytes is passed **by hidden reference**, and the callee may not modify it.
  `{ double, double }` — sixteen bytes, register-passed on SysV — goes by
  pointer here.

And the failure mode is the one this note's brief names: not a compile error,
not a link error, a `dgemm` that returns numbers.

### 4.2 What F0 implements, and what it refuses

The decision splits on a line that has not been drawn before, and drawing it is
what makes F0 affordable.

> **Decision 21. F0 implements **return** classification for all three targets.
> F0 implements **no argument classifier at all**, because it does not need
> one: every aggregate argument in a Science-to-Science call is passed by
> pointer, and an aggregate argument across the `extern` boundary is refused as
> `SC0429`.**

**Why returns must be implemented.** The runtime boundary forces it. The
contract page's §2 says so directly:

> *Several entry points return a struct by value […] Codegen must emit those
> calls with an `sret` parameter rather than an LLVM `ret { ptr, i64, i64 }`,
> or the two sides will disagree about where the value lives.*

There is no way to refuse this: `science_string_new`, `science_string_clone`
and six others return `ScienceString`/`ScienceArray`/`ScienceMap`/`ScienceChars`
by value, and a Science program that calls `String.new()` calls one of them.
So the return classifier is a stage-one requirement, not a stage-five one.

**Why return classification is tractable and argument classification is not.**
The return rules are short enough to state in full:

| Target | Rule |
|---|---|
| System V x86-64 | ≤ 16 bytes: classify the one or two eightbytes into `rax`/`rdx`/`xmm0`/`xmm1`. > 16 bytes, or any MEMORY eightbyte: hidden pointer in `rdi`, returned in `rax`. |
| AArch64 | An HFA of ≤ 4 identical float members: `v0`–`v3`. Otherwise ≤ 16 bytes: `x0`/`x1`. Otherwise: hidden pointer in `x8`. |
| Windows x64 | Exactly 1, 2, 4 or 8 bytes: `rax` (or `xmm0` for a single float member). Everything else: hidden pointer in `rcx`, returned in `rax`. |

That is three rules with three special cases. The argument rules are the same
classification plus register exhaustion, plus stack alignment, plus the
`al`-holds-the-vector-count varargs convention, plus the Windows shadow space,
plus AAPCS64's separate NGRN/NSRN/NSAA counters. It is an order of magnitude
more surface and it is the surface where the three-members-versus-two bugs
live.

**Why refusing aggregate arguments costs nothing measurable.**
`ffi-c-boundary.md` §5.4 already measured it and reached the same restriction
independently:

> *Every library in the target set can be used under that restriction — BLAS,
> LAPACK, HDF5, cuDNN and MPI pass aggregates by pointer throughout; GSL's
> `gsl_function` is passed as `const gsl_function *`.*

This note ratifies `SC0429` and extends it in one place that note does not
cover: **`SC0429` applies to function-pointer types too.** `ffi-c-boundary.md`
§4.2 defines function pointer types for callbacks and says nothing about
aggregate parameters in them. A Science closure installed as a C callback whose
signature takes or returns a by-value aggregate is the same classification
problem with the compiler on the *callee* side, where a mistake corrupts the
caller's stack instead of the callee's. Refusing it is one check in the same
place.

**And the private convention that makes the split legal:**

> **Decision 22. In a Science-to-Science call, every aggregate argument is
> passed by pointer to a caller-owned slot, and every aggregate return uses
> `sret`. The Science-to-Science calling convention is therefore not the
> platform C convention for aggregates, it is not a stable ABI, and it is not a
> compatibility surface. Only the `extern "C"` boundary and the `science-rt`
> boundary are.**

This is legal because both sides are compiled by the same compiler at the same
version, and it is safe because it is the *same* convention the runtime already
uses for arguments: every one of the 45 entry points takes aggregates as
`*const`/`*mut`, never by value. Codegen's private convention and the runtime's
published one agree, which means there is one rule to implement for arguments
and it is "pass a pointer".

**Cost, and it is real and it is paid in the hot path.** A Science function
taking a `Point` of two `F64` passes it through memory where SysV would pass it
in `xmm0`/`xmm1`, and a Science function returning `(Int, Error?)` — two words,
which SysV returns in `rax`/`rdx` — goes through `sret`. `(T, Error?)` is the
return shape of *every fallible function in the language*, so this is a tax on
the most common signature there is. Two mitigations and one honest admission:
LLVM's `ArgumentPromotion` and IPSCCP recover most of it for `internal`
functions at `-O2`; the convention is unstable by Decision 22, so it can be
changed the day the classifier exists without breaking anything; and until
then, the language's most common call shape is slower than C's for no reason
except that the classifier is not written yet.

### 4.3 When the classifier is written, how it is tested

Not in F0. But the test is what makes it safe to write, so it belongs here.

> **Decision 23. The ABI classifier is validated by differential test against
> `clang`, in two layers, and it is not merged without both.**
>
> 1. **IR comparison.** A generated corpus of every aggregate of one to four
>    members drawn from `{i8, i16, i32, i64, f32, f64, ptr}`, plus the nested
>    and HFA shapes, compiled by `clang -S -emit-llvm` and by `sciencec`, with
>    the parameter and return attributes compared. That is a few thousand
>    shapes and it runs in seconds.
> 2. **Execution.** The same corpus compiled to a C object file and a Science
>    object file, linked together, each side passing and returning values and
>    checking the bytes. IR comparison catches a wrong attribute; only
>    execution catches a right attribute applied to the wrong parameter.

**And the implementation note `ffi-c-boundary.md` §5.4 already gives, which
this note agrees with and will not improve on:** *"do not write a classifier.
Reuse an existing one."* If a vendored, tested classifier can be reused, reuse
it. The differential test above is what tells you whether you did.

### 4.4 Parameter attributes, and the one place a region bug becomes a wrong answer

`ffi-c-boundary.md` §2.2 commits to it and this note implements it:

> **Decision 24. A `mutable borrowed T` parameter is emitted `noalias
> nocapture` and aligned. A `borrowed T` parameter is emitted `readonly
> nocapture` and aligned, and **not** `noalias`.**

`borrowed` does not get `noalias` because two shared borrows of the same place
are legal — that is rule 4's *"shared many, or exclusive one"* — so a `noalias`
claim on a shared borrow would be a lie the optimiser is entitled to act on.
`mutable borrowed` does get it, because rule 4 forbids any other live borrow.

> **This is the single place in the language where a bug in region inference
> produces a wrong answer rather than a missed error.**

Everywhere else, a soundness hole in the borrow checker means a program is
accepted that should have been rejected, and it runs and does something. Here,
`noalias` tells LLVM it may reorder, cache and duplicate loads and stores
across the call on the assumption that nothing else touches that memory, and
if the assumption is false the optimiser produces code that computes something
else. It is the same attribute whose miscompiles took rustc years to shake out,
and rustc had lifetime annotations to reason from. `region-inference.md` §14
lists its own risks and this is not among them; it should be.

**The available mitigation, and it should be taken:** `--no-noalias` as an
unsupported debugging flag that suppresses the attribute, so that "is this a
region bug or a codegen bug" is one recompile rather than a week. It is three
lines and it is the flag every backend eventually adds after not having it.

### 4.5 Integer width, varargs, and the smaller refusals

- **`ffi.Span of T` lowers to a bare pointer.** `ffi-c-boundary.md` §1.3: the
  length does not cross. The length exists so that the wrapper's bounds check
  is an expression the compiler checks.
- **`CInt`, `CLong`, `CSizeT` and friends resolve per target triple**, and
  `BlasInt` is whatever the `extern` block declared it to be. §1.6 of that note
  is the ILP64/LP64 hazard and codegen's only obligation is to emit exactly the
  declared width and never to widen silently.
- **`F16`/`BF16` by value across the boundary is `SC0431`**, which is that
  note's code, and the reason is that the C ABI for half-precision is not
  settled across the three targets.
- **Varargs are refused.** The `extern` grammar of `ffi-c-boundary.md` §1.2 has
  no varargs form, so this is a ratification rather than a new restriction, but
  it should be said out loud: **`printf` is not callable from Science.** The
  SysV varargs convention requires `al` to hold the vector register count, and
  Apple's AArch64 variant puts all variadic arguments on the stack where the
  ordinary convention would use registers. Three conventions, three special
  cases, for a family of functions whose entire membership in the target set is
  `printf` and `sprintf`, both of which have `v`-suffixed non-variadic siblings.

---

## 5. Linking

`ffi-c-boundary.md` §5.1 specifies `library`, `via pkg-config`, `kind static`
and `when available`. None is implemented. This section says who drives the
linker and what the three platforms do differently.

### 5.1 Which linker

> **Decision 25. `sciencec` does not invoke a linker directly. It invokes the
> platform's C compiler driver as the linker driver: `cc` on Linux, `cc` on
> macOS, and `link.exe` on Windows, discovered in that order through
> `SCIENCE_LINKER`, then `CC`, then the platform default.**

**Reason.** A C compiler driver knows where `crt1.o`, `crti.o` and `crtn.o`
live, what the dynamic loader path is, which libc variant is installed, how to
spell `-rpath` on this platform, and which of `ld.bfd`/`ld.gold`/`ld.lld` is
actually present. Invoking `ld` directly means reimplementing all of that for
three platforms and getting it wrong on the fourth distribution. Every
production compiler that is not a C compiler does this — rustc, Go's external
linking mode, Swift — and the ones that tried the alternative regret it.

**Windows is the exception and it is not one.** MSVC has no `cc`-shaped driver;
`cl.exe` in link mode is awkward and `link.exe` is the actual tool. The
substantive difference is flag syntax — `/OUT:` rather than `-o`, `foo.lib`
rather than `-lfoo`, `/LIBPATH:` rather than `-L` — which is a translation
table, not a second design.

**Cost.** `sciencec` needs a C toolchain present to produce a binary, which is
a real install-time requirement and a real annoyance on Windows. Rejected
alternative: bundle `lld`. It removes the toolchain dependency and replaces it
with the job of knowing where the platform's CRT objects and system libraries
are, which is the part the C driver was doing. `--linker-driver` exists as an
escape hatch for anyone who has already solved that.

### 5.2 The three platforms

Cross-compilation is out of scope for F0 (core spec §12), so the target is the
host and there are three of them, with the triples `rust-interop.md` §7 already
names:

| Triple | Driver | Link name | Static form | Runtime |
|---|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `cc` | `-lopenblas` | `-Wl,-Bstatic -lopenblas -Wl,-Bdynamic` | `libscience_rt.a`, whole-archive not needed |
| `aarch64-apple-darwin` | `cc` | `-lopenblas` | `-Wl,-force_load,/path/libopenblas.a` | same |
| `x86_64-pc-windows-msvc` | `link.exe` | `openblas.lib` | the import library versus the static library are different files and the choice is made by which `.lib` is named | `science_rt.lib` |

**The Windows row is the one that is different in kind rather than in
spelling.** On ELF and Mach-O, `-lfoo` finds `libfoo.so` or `libfoo.a` and
`kind static` is a flag that changes which. On MSVC there is no such flag:
`foo.lib` is *either* an import library for `foo.dll` *or* a static archive,
and which one you get is decided by which file is on the library path.
`kind static` on Windows therefore means "require the named `.lib` to be a
static archive and fail if it is an import library", which is a check on the
file rather than a flag to the linker.

> **Decision 26. `kind static` is implemented on all three platforms and its
> meaning on MSVC is a check, not a flag. Dynamic remains the default
> everywhere**, per `ffi-c-boundary.md` §5.1's argument — HPC module systems
> work by swapping a shared object under a fixed name, and statically linking
> OpenBLAS defeats the mechanism the cluster administrator is relying on.

### 5.3 The runtime archive, and the self-contained promise

`crates/science-rt/Cargo.toml` already produces a `staticlib`, and its comment
says why: *"so `sciencec` can hand the archive to the system linker when
producing a native Science binary."* That is the mechanism; it is already
built.

> **Decision 27. `science-rt` is linked statically into every Science binary,
> always, with no option to link it dynamically. Its symbols are not
> re-exported from the resulting binary.**

Static because core spec §3 promises self-contained binaries and a Science
program that needs `libscience_rt.so` on the target machine is not one.
Not re-exported because two Science binaries in one process — which happens the
moment `python-interop.md`'s extension-module build exists — would otherwise
have interposing `science_alloc` symbols, and a `String` allocated by one and
freed by the other is a heap corruption that nothing in this design would
catch.

**What is *not* statically linked:** libc. `native-dependencies.md` §9 already
argues this at length and its two reasons stand — a statically linked binary
cannot be retargeted to a different CPU by the module system, and glibc cannot
be fully statically linked safely because the name service switch `dlopen`s
`libnss_*` at run time. So "self-contained" means "no Science or vendor
libraries at run time", not "static". That is a weaker promise than core spec
§3's wording suggests and the wording should be tightened.

### 5.4 `via pkg-config`, `when available`, and the order of flags

**`via pkg-config "openblas"`** runs `pkg-config --libs` and substitutes its
output for the search-path chain. `ffi-c-boundary.md` §5.1 gives the reason —
the correct BLAS flags are `-lopenblas` on one machine, `-lblas -llapack` on
another, and a five-library MKL incantation on a third — and requires that a
block using it still declare a plain `library` name as the fallback, because
`pkg-config` is not present on Windows. Codegen's obligation is to run it once
per block at link time, not at compile time, and to record the resolved flags
in the `[build]` table so the resolution is reproducible even where
`pkg-config` is not.

**`when available`** lowers to a lazily-initialised table of function pointers
in `science-rt`, populated by `dlopen`/`dlsym` or `LoadLibrary`/`GetProcAddress`
on first use, with every call in the block becoming an indirect call and the
block gaining a generated `is_available() -> Bool`. **That is a runtime feature
that `science-rt` does not have.** There is no `dlopen` wrapper among the 45
entry points. It is a stage-five item and it is an ask on the runtime, filed in
§13.

**Link order is the thing that will go wrong first.** On ELF, `ld` resolves
left to right and an archive only contributes objects that satisfy already-seen
undefined symbols, so the order is: the program's object files, then
`libscience_rt.a`, then the declared libraries in *dependency* order, then the
system libraries. Science's `extern` grammar records no dependencies between
libraries, so a block that needs `-llapack -lblas` in that order has to declare
them in that order.

> **Decision 28. Declared library names are passed to the linker in the source
> order of their `library` clauses, and this is documented as significant.**

It is unsatisfying — an ordering constraint expressed by source position is
exactly the kind of implicit coupling this project dislikes — and the
alternative is a dependency graph in the `extern` grammar that nobody has asked
for and that `pkg-config` already provides for the libraries that care.

### 5.5 Rendering the linker's output

`ffi-c-boundary.md` §5.3 is emphatic and this note agrees without amendment:

> *`sciencec` must catch the linker's output and render them as Science
> diagnostics (`SC0461`) naming the `extern` declaration's span and the library
> clause, not pass `ld`'s output through.*

The mechanism is a symbol table built at codegen time mapping every mangled
external name back to the `extern` declaration's span, and a parser for three
linkers' undefined-symbol messages. That parser is the ugly part: GNU `ld`,
`ld64` and `link.exe` each say it differently and each changes its wording
between versions.

> **Decision 29. `SC0461` is emitted for an undefined symbol that codegen's own
> table can attribute to an `extern` declaration. Any other linker failure is
> `SC0402`, which prints the linker's command line and its output verbatim and
> says that it is doing so.**

The honesty is the point. A diagnostic that pretends to have understood a
linker error it did not understand is worse than one that hands over the raw
text with the command that produced it, and `SC0402` exists so that the failure
to parse is visible rather than silent.

### 5.6 Deliberately not built

- **LTO, in either direction.** Cross-language LTO needs the C library
  compiled to compatible bitcode and vendor libraries ship as binaries
  (`ffi-c-boundary.md` §5.5). Science-internal ThinLTO needs codegen units,
  which Decision 4 does not have.
- **Cross-compilation.** Core spec §12. Consequence: the target triple is the
  host triple, and `SC0406` is the diagnostic for asking for anything else.
- **Dynamic Science libraries.** There is no stable Science ABI (Decision 22),
  so a `.so` of Science code has no consumer that could survive a compiler
  upgrade.
- **Incremental codegen.** Object-file caching keyed on the mono key is the
  obvious next step and it interacts with `region-inference.md` Decision 8's
  SCC-granular invalidation in ways nobody has worked out. F1.

---

## 6. Debug info

> **Decision 30. F0 emits line tables only: `DW_TAG_compile_unit`,
> `DW_TAG_subprogram` and the line-number program on ELF and Mach-O, CodeView
> equivalents on MSVC. No variable locations, no `DIType` for any user type, no
> lexical scopes beyond the function.**

**What that buys**, which is most of what a scientific user wants: a backtrace
with file and line from a profiler, `perf` and `Instruments` attributing time
to Science source lines, a crash report that names a function, and `addr2line`.

**What it does not buy:** you cannot inspect a variable in a debugger. `gdb`
will stop at a line and be unable to tell you what `x` is.

**Why not more.** A `DIType` per user type must mirror §3's layout rules
exactly — offsets, sizes, the niche encoding for `T?`, the discriminant
position for a `choice`. A `DIType` that disagrees with the layout produces a
debugger that confidently prints the wrong value, which is worse than a
debugger that prints nothing, and the disagreement is not detectable by any
test short of scripting a debugger. That is a second implementation of the
layout algorithm, kept in sync by hand, for a feature whose F0 audience is
compiler developers.

**What it costs to defer, and this is the part that is usually got wrong.**
Deferring debug info costs *nothing* if the hook is left, and costs a refactor
of the layout code if it is not. The hook is small:

> **Decision 31. The layout computation returns a layout record carrying field
> offsets, field names, the discriminant position and the niche encoding —
> everything a `DIType` needs — from the first version, even though nothing in
> F0 reads it.**

Without that, adding debug info later means threading a `DIBuilder` and a
`DIScope` through every layout query, which is the shape of refactor that
touches every file in the crate. With it, it is a new module that consumes a
record that already exists. The record is also what `rust-interop.md`'s
`science_abi.rs` layout assertions and the `SC0424` `ffi.CLayout` check want, so
it has two other customers before the debugger arrives.

**One thing that must *not* depend on debug info.** `script-mode.md` §2.3's
exit table says a panic prints "the panic message and location". A location
read from debug info would vanish when debug info is off.

> **Decision 32. A panic's source location is a static string emitted at the
> panic site and passed to `science_panic_bytes` as part of the message.
> Panics carry their location at every `-g` level, including none.**

---

## 7. Optimisation, and the trap

### 7.1 What the levels are

> **Decision 33. `-O0`, `-O1`, `-O2` and `-O3`, mapping to LLVM's
> `default<O0>`, `default<O1>`, `default<O2>` and `default<O3>` pipelines by
> name, with no custom passes.** Core spec §12 already says so — *"Custom
> optimizations; everything is delegated to LLVM"* — and this note adds only
> that the pipeline is selected **by string**, which means the pipeline name is
> a value that can go in the `[build]` table and a pipeline change is a visible
> change rather than a diff in the compiler.
>
> **`-O2` is the default.** There is no separate debug profile in F0, because
> there is no package manager in F0 to have profiles. `--debug` is shorthand
> for `-O0 -g`.
>
> **No `-Os` or `-Oz`.** Nobody in the target audience has asked for a smaller
> binary and each level is another value in the reproducibility record.

**Why `-O2` and not `-O0` by default.** A scientific user who benchmarks the
language on first contact and finds it 20× slower than NumPy concludes the
language is slow, and is not wrong about what they measured. `-O0` output under
Decision 8 is alloca-heavy and genuinely bad. Defaulting to the fast thing and
naming the slow thing `--debug` puts the surprise in the right place.

**Cost.** Compile times are `-O2` compile times by default, which is the
tradeoff Go declined and Rust made. For a language whose users iterate in a
notebook, this is a real objection, and the answer is F1's interactive tier
running over the query database rather than an `-O0` default that makes every
benchmark a lie.

### 7.2 One non-optional pass

> **Decision 34. `LLVMVerifyModule` runs at every level including `-O0`, and a
> verifier failure is an internal compiler error that prints the offending
> function's IR.** This is Decision 1's mitigation: the type safety `inkwell`
> would have provided at Rust compile time, relocated to a check that fires on
> the compiler's own test suite. It costs a fraction of a second per module and
> it is the difference between "wrong IR" and "segfault in the optimiser".

### 7.3 Would any optimisation level violate `reproducibility.md`?

**No — and only because of three things codegen must do, none of which is an
LLVM default, and at least one of which LLVM actively defaults against.**

The question matters because `reproducibility.md` Decision 3 is unconditional:
*"`sciencec` never enables a value-changing floating-point transform. FMA
contraction is off by default; fast-math is not exposed as a flag at all;
reassociation of a float reduction is forbidden."* If `-O3` quietly violated
that, the language's headline claim would be false at its own default settings
and nobody would find out until a reviewer diffed two machines.

**The good news first, because it is the substance of the answer.** LLVM's
value-changing float transforms are gated on **per-instruction fast-math
flags** — `nnan`, `ninf`, `nsz`, `arcp`, `contract`, `afn`, `reassoc` — and the
optimisation level does not set them. An `fadd double` with no flags is not
reassociated at `-O3`, the loop vectoriser refuses to vectorise a float
reduction without `reassoc`, and `x/c` is not turned into `x * (1/c)` without
`arcp`. So `-O3` is exactly as bit-reproducible as `-O0`, and the answer to the
question as asked is **no level violates it**.

**The three obligations, and the second one is the trap.**

**1. Never set a fast-math flag on any instruction.** The default is no flags,
so this is an obligation to never opt in — a line that is correct by not being
written, which means nothing tests it. **Decision 35: a test greps the emitted
IR of the entire corpus at `-O3` for the fast-math flag tokens and fails on any
hit.** Three lines of test for the language's headline property.

**2. Set `AllowFPOpFusion = Strict` explicitly on the target machine.**

This is the trap and it is worth the space. LLVM's `TargetOptions` defaults
FP contraction to `Standard`, not `Strict` — meaning the *backend* may fuse an
`fmul` feeding an `fadd` into an `fma` even when neither instruction carries
the `contract` flag, because C's standard permits contraction within a single
expression and LLVM's default matches C. A code generator that creates a target
machine and does not touch that field gets contraction, at every optimisation
level including `-O0`, on any target with an FMA unit — which is every AArch64
chip and every x86 since Haswell.

**That is precisely the failure `reproducibility.md` G2 predicts:** *"the
`-ffp-contract` default differs between C compilers and between LLVM versions,
so doing nothing means the policy changes underneath the language without
anyone deciding it."* That note predicted it as a policy hazard. It is also a
*literal one-line* hazard in this note's implementation, and doing nothing does
not mean "no contraction", it means "contraction, silently, on the machines
that have FMA and not on the ones that don't" — the exact shape of bug where
the same source produces different numbers on two machines for a reason not
visible in the source or the flags.

> **Decision 36. Codegen sets `AllowFPOpFusion = Strict` on every target
> machine it creates, and a test asserts that a Science program computing
> `a * b + c` emits `fmul` followed by `fadd` and no `fma`, on every target.**
>
> `math.fma(a, b, c)` remains a function lowering to `llvm.fma`, per
> `reproducibility.md` Decision 3: a user who wants the fused operation asks for
> it and gets it everywhere, which is both faster and more deterministic than
> letting the backend decide per call site.

**3. Do not emit LLVM intrinsics for transcendental functions.**

This one has not been written down anywhere and it is subtler than the other
two. LLVM constant-folds `llvm.sin.f64` of a constant argument **by calling the
host's `sin()` at compile time**. So `let x be sin(0.5)` compiled on a macOS
build host and on a Linux build host can differ in the last bit, from the same
source, at the same optimisation level, with no fast-math flag anywhere —
because the folding used two different libms.

`reproducibility.md`'s build record already anticipates the run-time half of
this: it carries `"libm": "science-libm 0.4.1"`, and `stdlib-core.md` §8.3's
1-ULP contract is what makes the run-time answer reproducible. The compile-time
half needs codegen's cooperation.

> **Decision 37. A transcendental function lowers to a direct call to
> `science-libm`'s symbol, never to an LLVM intrinsic, precisely so that LLVM
> cannot constant-fold it against the build host's libm.**
>
> **The exception is the whitelist**, and `intrinsics-math-physics.md` §3.1's
> table already is it: `llvm.sqrt`, `llvm.fabs`, `llvm.copysign`, `llvm.fma`,
> `llvm.floor`, `llvm.ceil`, `llvm.trunc`, `llvm.roundeven`, `llvm.round`,
> `llvm.minimum`, `llvm.maximum`. That note marks every one of them
> "IEEE-754 exact", which is exactly the property that makes constant-folding
> them bit-identical on any host. **Nothing outside that table is emitted as an
> intrinsic.**

That note wrote its table to decide which operations are tier 1 and which are
libcalls. This note is reading the same table as a *constant-folding safety
whitelist*, which is a use it did not anticipate and should be told about.

### 7.4 Target CPU, which is the fourth channel

`-mcpu` is not a float transform, but it changes numbers by changing which
libcalls happen: `intrinsics-math-physics.md` §3.1's table shows `floor`,
`ceil`, `trunc` and `roundeven` as **libcalls** at the x86-64 baseline and as
single `roundsd` instructions at x86-64-v2. Same answer, very different speed —
and if the libcall and the instruction ever disagree on a corner case, the same
answer stops being the same.

> **Decision 38. The default target CPU is the triple's baseline —
> `x86-64` (SSE2) on both x86 triples, `apple-m1` on
> `aarch64-apple-darwin` — never `native`. `--target-cpu=NAME` selects
> another. `--target-cpu=native` is accepted, resolved to a concrete CPU name
> at build time, and **recorded as that concrete name** in the `[build]` table,
> so that a `native` build's record is still reproducible even though the flag
> is not.**

Resolving `native` to a name before recording it is the small idea in this
section and it is worth stating: a build record that says `"cpu": "native"` is
a record of nothing, and one that says `"cpu": "znver4"` is a record somebody
can rebuild from.

### 7.5 What the float policy costs, restated with the specific loss

`reproducibility.md` priced Decision 3 at "a benchmark deficit against
toolchains that contract by default", typically 10–30% in a tight numeric loop.
This note can name the specific loss, because it is the one that will show up
first:

**The loop vectoriser will not vectorise a floating-point reduction.** `sum` of
an `Array of F64` is scalar, one `fadd` at a time, because vectorising it means
summing four partial sums and adding them at the end, which is a different
order, which needs `reassoc`. Against a C compiler at `-O3`, that is a 4× to 8×
loss on the single most common operation in scientific computing.

**And the constructive way out, which is an ask rather than a decision here.**
`collections-and-chains.md` §7.1 promises a *defined order* for a reduction.
A defined order does not have to be strictly sequential — it has to be
*defined*. A blocked reduction that sums lanes 0, 4, 8, … into one accumulator,
1, 5, 9, … into another, and combines them in a fixed order at the end is
perfectly deterministic, gives the same answer on every machine, is numerically
*better* than the sequential sum, and vectorises. It just has to be the defined
order rather than a transform the optimiser is allowed to apply.

> **Asked of `collections-and-chains.md`: define the reduction order as an
> explicit 8-way blocked order rather than as strict left-to-right.** It
> recovers most of the vectorisation loss without giving up one bit of
> determinism, and it is a decision that must be made before anyone depends on
> the current order, because changing a reduction's order later changes every
> published number.

---

## 8. Performance targets

### 8.1 There was no target until this section

Nothing in `docs/` states what Science is supposed to be *fast enough for*. Not
the core spec, not `reproducibility.md` — which prices the float policy at "a
benchmark deficit" without saying against what — not `broadcasting.md`, not
`scientific-libraries.md`. Every one of those notes trades performance for
something and none of them knows what it is trading against.

The target handed down is **"similar to Go or a little better"**, and the
useful thing to do with it is to refuse it in one direction and accept it in
the other, because "Go performance" names two opposite reputations:

| | Go's reputation | Where Science should land |
|---|---|---|
| **Run time** | Fast enough, garbage-collected, a deliberately simple backend | **Near C and Rust. Go is the wrong target and we should beat it structurally.** |
| **Compile time** | The fastest compiler of its generation | **Go is the right target and we will miss it badly.** |

The reason the two answers differ is that almost every one of Science's
existing decisions is a *run-time* win bought with *compile-time* money, and
nobody has been adding up the second column.

### 8.2 Run time: Go is too low a target, and the reasons are structural

> **Decision 39. The run-time target is C and Rust, not Go: scalar,
> non-vectorised, non-FFI Science code should land within roughly 20% of an
> equivalent Rust program, and Go is expected to be behind both. This is not an
> aspiration about effort; it is a consequence of four decisions that have
> already been made.**

**One — there is no garbage collector, and the cost that matters is not the
pauses.** Go's sub-millisecond pause times are the famous number and they are
the wrong number for this audience; a scientific batch job does not care about
a 500-microsecond pause. What it cares about is **the write barrier**, which is
a branch and a conditional store on *every pointer write* while the collector
is marking, plus the allocation-rate tax that keeps the collector running at
all. On pointer-heavy work — a tree, a graph, a symbol table, a compiler — that
is a steady tax on the hot path that no tuning removes, because it is in the
emitted code rather than in the runtime's scheduling. Science's runtime says it
from the other side in its §1: *"the runtime never decides when to free
anything. It frees when told, and only then."* There is no barrier to emit
because there is nothing watching.

**Two — LLVM, against a backend designed to trade optimisation for compile
speed.** Go's backend is a defensible choice for Go's goals and it is not close
to LLVM at `-O2` on numeric code. This is the same decision §8.3 is about to
bill us for, and it is worth noticing early that the run-time argument and the
compile-time argument are *one decision seen from two ends*.

**Three — monomorphisation against dictionary passing.** Core spec §3 chose
monomorphisation "for numeric specialization", against a uniform
representation. Go compiles one copy per GC shape and passes a dictionary, so a
generic function over a pointer-shaped type is not specialised and its interior
calls are indirect. For a `sum of T` at `F64`, that is the difference between a
fused loop body and an indirect call per element.

**Four, and for this project it is not a nicety — auto-vectorisation.** Go does
not meaningfully auto-vectorise. A scientific language's own loops are largely
what vectorisation is *for*: the entire reason to write `for x in xs:` rather
than call into BLAS is that the operation is not one BLAS has. LLVM's loop
vectoriser and SLP vectoriser are a large part of what §7's `-O2` buys, and
they are most of the distance between a language you prototype in and a
language you keep the result in.

**The honest asterisk, and it is §7's.** `reproducibility.md` Decision 3 costs
exactly the vectorisation win on exactly the operation where it matters most: a
floating-point reduction does not vectorise without `reassoc`, so `sum` over an
`Array of F64` is scalar (§7.5). That is a 4–8× loss on the single most common
kernel in scientific computing, taken deliberately, in the one place where
Decision 39 is hardest to hit. It is recoverable without giving up one bit of
determinism if `collections-and-chains.md` §7.1 defines the reduction order as
blocked rather than strictly sequential, which is why §13 asks for it.
**Until it does, Decision 39 should be read as excluding reductions, and the
exclusion should be stated in any benchmark this project publishes rather than
discovered in one somebody else publishes.**

**And the part of the hot path that is nobody's to win.** A large share of a
real Science program's time is inside somebody else's C: `dgemm` runs at
OpenBLAS's speed whether it was called from Python, Julia, Rust or Science.
Nothing in this note makes a BLAS kernel faster. So the performance question at
that boundary is not the kernel — it is **whether the crossing is free** — and
that reframes two interop decisions as performance decisions wearing interop
clothes:

> **Decision 40. An `extern "C"` call is a direct `call` to the declared symbol:
> no wrapper frame, no marshalling, no thunk, no runtime bookkeeping.
> `ffi.Span of T` is already a bare pointer at the ABI (`ffi-c-boundary.md`
> §1.3); a `borrowed Array of F64` coerces to one without copying; and DLPack
> and the Arrow C Data Interface cross as pointers to `ffi.CLayout` structs with
> no conversion at all (`type-checking-and-mir.md` Decision 19). The crossing
> costs one `call` instruction and nothing else.**

That is the sentence Python cannot say — every NumPy call is a boxed object
crossing an interpreter boundary — and for a real program it is worth more time
than any loop this compiler emits. It is also why `when available`'s `dlopen`
table (§5.4) is opt-in rather than the default: an indirect call per foreign
call is noise against `dgemm` and is not noise against a per-element function,
which `ffi-c-boundary.md` §5.2 already argues.

**What Decision 39 costs, and two places where F0 is knowingly below it.**
Stating a target creates an obligation to meet it, and F0 does not, in two
named places:

1. **§4.2's by-pointer aggregate convention.** Every aggregate argument goes
   through memory and every aggregate return through `sret`, including
   `(Int, Error?)` — the return shape of every fallible function in the
   language, which C returns in two registers. This is the largest known gap
   between F0 and Decision 39. It is bounded by writing the ABI classifier, and
   Decision 22 deliberately keeps the convention unstable so that closing it
   breaks nothing.
2. **§2.6's allocating string literals.** A literal in a loop calls the
   allocator on every iteration. Bounded by one runtime change, asked for in
   §13.

Naming them is the point. A performance target with its known violations
written down beside it is a target; one without them is a slogan.

### 8.3 Compile time: Go is the right target and we will miss it badly

> **Decision 41. The compile-time target is Go's, it will not be met, and the
> gap is the sum of three decisions that are each defensible on their own. This
> note records the bill rather than pretending it is small.**

The received figure for Rust against Go is roughly an order of magnitude and
often considerably worse. Science has made the same three structural choices,
and the third is worse here than it is in Rust.

**One — LLVM.** rustc spends a commonly-cited 40–60% of a release build inside
LLVM. Decision 1 chose LLVM deliberately and §8.2 is the reason. There is no
version of "fast compiles and LLVM `-O2`", and the trade is being made with
both eyes open.

**Two — monomorphisation and its code explosion.** One copy per instantiation
buys §8.2's third argument, and it costs a generic function used at twenty
types being twenty functions for LLVM to optimise. This is the most direct
example in the project of a run-time win billed to compile time.

**Three, and this one is ours rather than inherited.** `region-inference.md`
Decision 8 makes the incremental invalidation boundary the **strongly connected
component of the call graph**, not the function. That note's own §14 then says
what that means:

> *A change inside a large SCC re-analyses the whole component, and the
> component can be large — mutually recursive descent parsers are one big SCC,
> and the Science compiler will contain one.*

Put beside `self-hosting.md`, that sentence says something sharper than either
note noticed:

> **The self-hosted Science compiler is the worst case for its own incremental
> recompilation.** A recursive-descent parser is one SCC; editing one function
> inside it re-runs region inference over the whole parser and over every SCC
> downstream of it. The program this language most wants to compile is the
> program its incrementality design handles worst, and it is the program its own
> developers will edit every day.

That is not an argument against Decision 8. The SCC boundary falls out of
interprocedural inference, which falls out of having no lifetime syntax, which
is the language's headline claim; the boundary is the claim's price and the
price is real. It is an argument for finding out the number early:

> **`self-hosting.md` Gate I should measure re-analysis time for a
> one-character edit inside the parser's SCC, the moment `science-regions`
> first works.** That number is what "no lifetime syntax" costs per keystroke,
> and nobody has ever seen it.

**What is *not* on this list, and it is worth saying so:** type checking.
`type-checking-and-mir.md` Decision 23 memoises per *function body*, which is as
fine a granularity as exists, and its Decision 1 makes bodies independent so
that editing one invalidates one. "Compile times" is usually assumed to mean
the type checker. Here it does not, and §8.6 is why that matters.

### 8.4 Is LLVM *the* backend or *a* backend?

This is the decision that silence would foreclose, which is why it is stated
rather than assumed.

Rust is paying to retrofit exactly this right now.
`rustc_codegen_cranelift` exists to make debug builds fast, and it has been
expensive precisely because MIR-to-LLVM was not an interface from the start:
`rustc_codegen_ssa` had to be extracted from a backend that had already grown
into the compiler, years after the fact, and the seam is still visible. That is
an observable price for the thing this note is about to decide, paid in public
by a project with more resources than this one.

> **Decision 42. `MIR → backend` is an interface, and LLVM is its first
> implementation. `science-codegen` splits in two: a target-independent half
> owning layout, the ABI classification of §4, mangling, the monomorphisation
> walk and drop-glue construction; and a backend half owning nothing but the
> translation of an already-decided lowering into a particular IR. Everything
> two backends would have to agree on lives above the line.**

**That last sentence is the whole decision**, and it is the part that is
genuinely expensive to retrofit. A second backend need not agree with LLVM
about instruction selection. It *must* agree about struct offsets, enum
discriminant positions, niche encodings, `sret` classification, symbol names
and descriptor contents — because an object file from one has to link against
an object file from the other, and because `science-rt` is a fixed C ABI that
both have to satisfy. **If layout and ABI are computed inside the LLVM backend,
adding a second backend means reimplementing them and hoping the two agree,
which is §4's silent-corruption failure mode with two more places for it to
happen.**

The interface is small, and what crosses it is worth writing down now:

| Above the line — target-independent | Below the line — per backend |
|---|---|
| Layout: sizes, alignments, offsets, niches (§3) | Type construction in the backend's IR |
| ABI classification: `sret`, by-pointer arguments, parameter attributes (§4) | Emitting those as the IR's attributes |
| Mangling and the monomorphisation walk (§2.7, Decision 4) | Symbol and linkage creation |

> **AMENDMENT 2: this row is the only sentence in the corpus that places the
> monomorphisation walk, and no note describes it.** This note's own opening
> says it is "everything *below* monomorphisation" and Decision 25 puts the walk
> upstream; `type-checking-and-mir.md` names the key — the const-expression
> normal form — and stops there. So the walk was built with no roots, no
> worklist, no termination rule and no diagnostic specified, and every decision
> below had to be taken rather than implemented.
>
> What was decided, recorded here so the next note does not re-decide it: the
> emitted map **is** the dedup set, so the two cannot disagree; the call graph
> is deliberately unused, because it is keyed on a definition while the walk is
> keyed on a definition *and its arguments*, so `f` calling `f` is one edge
> there and either one instance or infinitely many here; termination is
> containment plus a depth backstop, because containment is sufficient for
> growth and not necessary — const arguments do not nest, so `step of (N)`
> calling `step of (N+1)` is caught only by depth; and reproducibility rests on
> a `BTreeMap` keyed by mangled symbol, no identifier and no type reaching a
> symbol, and no hashes, tested by lowering the whole corpus twice from fresh
> tables and comparing bytes.
>
> **And it found this note's own §2.7 lossy.** The mangler encodes a *lowered*
> type, which drops a pointer's pointee and an interface object's interface — so
> `f of (Box of Int)` and `f of (Box of String)` were one symbol, as were
> `g of (any Summarize)` and `g of (any Report)`. That is `SC0404` raised
> against a program with nothing wrong with it, and unlike §15's
> one-key-two-symbols it is the **detectable** direction and it fires on
> ordinary code. Symbols now encode the checker's type nominally and share only
> the grammar. Drop glue (Decision 12) and descriptors (Decision 20) inherit the
> fix when they are built.
| Drop-glue construction and descriptor contents (§2.5, §3.5) | Emitting them as functions and constants |
| Which operations are runtime calls (§2.6) | Emitting the call |
| The layout record for debug info (Decision 31) | Line tables in the backend's format |
| The float policy's obligations (§7.3) | Applying them to the backend's knobs |

**What it costs today.** One layer of indirection in a crate nobody has written
yet, and the discipline not to reach through it — which is the part that
actually fails, because the fastest way to fix a bug at eleven at night is
always to let the backend peek at something above the line. Concretely: perhaps
5% of the crate in plumbing, and a rule that has to be enforced in review
rather than by the compiler.

**Except that it can be enforced by the compiler, cheaply, and should be.**
Core spec §7.2 names one crate, `science-codegen`. Make it two —
`science-codegen` above the line and `science-codegen-llvm` below it, with the
dependency pointing downward only — and "the target-independent half does not
name LLVM" becomes a fact `cargo` checks on every build rather than a rule a
reviewer remembers. That is the cheapest structural enforcement available and
it costs one line in a manifest. **It is an amendment to §7.2's crate list and
this note is asking for it** (§13).

**What it buys, in order of likelihood.**

1. **A fast non-optimising backend for debug builds becomes additive rather
   than a rewrite.** That is §8.3's only real lever short of abandoning LLVM,
   and it is the lever Rust is pulling now at great expense.
2. **Codegen units become cheap** (§8.5): a CGU is a unit of work handed to a
   backend instance.
3. **A GPU target in F1+** — which `intrinsics-math-physics.md` and
   `native-dependencies.md` both contemplate and neither owns — becomes a
   backend below this line rather than a second compiler.
4. **The bootstrap gets an extra seam.** §1.3's port takes the two halves
   independently: the target-independent half ports like the resolver, the
   backend half transliterates against LLVM-C. Two well-defined porting units
   beat one called "the code generator".

**What is deliberately not promised:** F0 builds no second backend, and one is
not in §14's plans. This decision is about where the line is drawn, not about
crossing it. **A second backend is a feature; drawing the line is insurance,
and insurance is bought before the fire.**

### 8.5 Where parallel codegen units sit

Decision 4 emits one LLVM module per crate. That is right for F0 — it makes
emission order sortable and therefore deterministic, which is Gate J's problem
— and it is the single largest compile-time lever the project is declining to
pull.

> **Decision 43. Codegen units are F1, they sit below §8.4's interface, and the
> day they arrive the CGU count becomes a field in the `[build]` table.**

The `[build]` field is not bureaucracy. Splitting a crate into codegen units
changes which functions the inliner can see in each unit, which changes
inlining, which changes instruction selection. It changes no *answer* under
§7's float policy — inlining is not a value-changing transform — but it changes
the binary, and `package-manager.md` Decision 3's table exists precisely to
record the toolchain settings that changed a binary. Adding a field to a table
that already exists is free; adding the concept after somebody has published a
number is not.

**The ordering risk, named:** parallel codegen and deterministic emission pull
against each other. Decision 4's sorted single module is deterministic by
construction; N units emitted on N threads and linked are deterministic only if
the partition is deterministic and the link order is fixed. Whoever builds CGUs
inherits Gate J along with them.

### 8.6 `sciencec check` already exists, and it is the answer people want

When someone says "it should compile as fast as Go", what they usually mean is
"the edit, compile, read-the-error loop should be instant". That loop needs no
code generator, and Science already ships the command:

```
sciencec check FILE...     lex, parse and resolve; report what is wrong
```

It runs the front end over the salsa database, does no codegen at all, and
under `type-checking-and-mir.md` Decision 23 it will invalidate per function
body. **A language whose type checker runs without a backend has most of Go's
interactive feel available already** — and it has it in a subcommand that
shipped before the backend was designed.

Two consequences, both cheap and both worth committing to:

- **`sciencec check` stays codegen-free and stays fast, and it is benchmarked
  as a first-class artefact rather than as a side effect of `build`.** The
  moment it starts doing a little codegen "just for one check", the property is
  gone and nobody notices for a year.
- **`--emit` splits the expensive half off.** `sciencec build --emit=llvm-ir`
  and `--emit=obj` let a user stop before the optimiser, and
  `type-checking-and-mir.md`'s MIR dump — required before those crates are
  written, per `self-hosting.md` Decision 4 — is already the artefact that
  makes stopping early useful.

This does not close §8.3's gap for a full build. It closes it for the loop
people are actually complaining about, and distinguishing the two is most of
what an honest compile-time story consists of.

### 8.7 How the targets are measured

A target with no measurement is a slogan, and this project has a whole note
about that failure mode.

> **Decision 44. A benchmark suite exists from §10's stage 3 onward and reports
> two numbers on every run: run time against C and Rust implementations of the
> same program, and wall-clock compile time against a Go implementation of it.
> Both are tracked per commit. The suite contains at least one reduction, so
> that §8.2's asterisk appears in the numbers rather than in a footnote.**

Stage 3 rather than stage 6, because a benchmark suite started after the
compiler works measures whatever the compiler already happens to do. Started at
stage 3, it measures the first loop the compiler ever emits, and every
regression after that has a commit attached to it.

**Cost.** Benchmark infrastructure is an ongoing maintenance burden of the kind
that rots quietly. The mitigation is to keep it small — five programs, not
fifty — and to accept that five tracked programs beat fifty untracked ones.

---

## 9. Testing `science-rt`'s claim

The runtime's module documentation says codegen "can be emitted from this page
alone, without reading a single function body", and `self-hosting.md` §1.3
calls the crate the most bootstrap-ready artefact in the repository. This note
was asked to test that. Here is what happened.

### 9.1 The method

I read the page, wrote §§2–4 of this note from it, and then read the 45
signatures and the type definitions to see where I had been wrong. The claim's
own wording permits reading signatures — it excludes *function bodies* — so the
fair test is: **where does the page assert something the crate does not do, or
leave something out that a code generator must know and cannot guess?** Six
findings, in descending order of how much damage they do.

### 9.2 Finding 1 — §2's `sret` list is incomplete, and the omission is silent

§2 names the entry points that return an aggregate and must be called with an
`sret` parameter: `science_string_new`, `science_string_clone`,
`science_string_truncate`, `science_array_new`, `science_map_new`,
`science_string_chars`, `science_read_file`. Seven, plus
`ScienceNullableIoError` named as the two-byte exception.

**`science_array_with_capacity` is missing.** It returns `ScienceArray` by
value — three words, MEMORY on every one of the three targets — and it is not
in the list. A codegen emitted from §2 as the authority emits
`%a = call { ptr, i64, i64 } @science_array_with_capacity(ptr %info, i64 %n)`,
the runtime writes through a return slot pointer that the caller never passed,
and the program corrupts whatever was in that register.

The page mentions the function elsewhere — §8 says "the capacity hints" are
codegen-support entry points — so cross-referencing two paragraphs on the same
page would catch it. Reading §2 as the specification of the return convention,
which is what §2 is, would not. **This is exactly the failure mode the claim
invites and it is the one defect on this list that produces silent memory
corruption.**

### 9.3 Findings 2 to 6

**Finding 2 — the function-pointer ABI is not on the page.** §6 names
`drop_fn`, `hash_fn` and `eq_fn` and gives `ScienceTypeInfo` as
`{ size, align, drop_fn }`. Codegen must *emit all three functions*, and the
page never states their C signatures. They are
`void (*)(uint8_t *)`, `uint64_t (*)(const uint8_t *)` and
`bool (*)(const uint8_t *, const uint8_t *)`, and they are documented in
`abi.rs`'s item docs, not on the page. This is the most consequential gap
because the three functions are codegen's own output and a wrong signature is a
wrong calling convention.

**Finding 3 — §4's `usize` rule is false of the interface it describes.** §4
says *"Sizes and capacities internal to the runtime, which never surface in a
Science signature, are `usize`."* They surface in eight places:
`science_alloc`, `science_realloc` and `science_dealloc` take `usize` sizes and
alignments; `science_array_with_capacity` takes a `usize` capacity;
`science_array_reserve` a `usize` count; `science_string_from_bytes` and
`science_panic_bytes` a `usize` length; and `ScienceTypeInfo`'s `size` and
`align` are `usize` in a struct **codegen emits**. Worse, `ScienceString.len`,
`ScienceArray.len`, `ScienceMap.len` and every `cap` field are `usize` fields,
while `science_array_len` *returns* `i64`. A codegen that believed §4 would
emit `i64` throughout and be correct by accident on all three 64-bit targets
and wrong the first time anyone builds for a 32-bit one. The rule as written is
not "usize does not surface"; it is "usize and i64 are the same width on every
target F0 supports", which is a different and much weaker statement.

**Finding 4 — the descriptor's parameter position is unstated and
inconsistent.** §6 says codegen "passes a pointer to it on every call" and never
says where. `science_array_free(array, info)` and `science_map_free(map, info)`
put the receiver first; `science_box_new(info, value)` and
`science_box_free(info, ptr)` put the descriptor first. From the page alone you
would pick one convention and get `Box` backwards — and since both parameters
are pointers, that is not a type error anywhere, it is `science_box_free`
treating a `ScienceTypeInfo` as a value pointer.

**Finding 5 — there is no program entry point, anywhere.** This is the largest
gap and it is a gap in the crate rather than only in the page. There is no
`science_main`, no runtime initialisation, no teardown, no `argv` access, and
no `science_exit`. Codegen must emit `main` itself, and the page never says so
— which is survivable, because that is what a code generator does.

What is not survivable is the exit contract. `script-mode.md` §2.3 requires
that a script body returning a non-null error exit with status **1** after
printing `error: ` and the error's `Display` to **stderr**. The runtime has
`science_print`, which writes to stdout — **`science_println` no longer
exists**; the runtime's §2 records that it and `science_print` were the wrong
way round and that the survivor is named for the Science spelling it
implements — and
`science_panic_bytes`, which writes to stderr and then calls
`std::process::abort()` — the platform's abort status, which is `SIGABRT` on
POSIX and `3` on Windows, and is not 1.

> ~~**A hello world can be emitted from this page. A `main` that returns an
> error cannot.** There is no symbol that writes to stderr without aborting, and
> no symbol that exits with a chosen code.~~
>
> **Both halves are now false.** `science_write_error_bytes` writes and returns;
> `science_exit` flushes and exits with a chosen code. The emitted `main` reports
> and exits 1, verified as a compiled program with both its stderr and its status
> asserted. The runtime's entry-point count went 45 to 47 and the `sret` set
> stayed at nine — derived, not decided, which is the check that list has failed
> twice.
>
> **What remains is the rendering, and it is not this page's.** §2.3 of
> `script-mode.md` asks for the error's `Display`, and `Display` is declared in
> the prelude with **no methods** — naming `display(Formatter)` would invent a
> Level 1 type no note specifies. So the message says a script failed and cannot
> say which error, and says so in the message itself rather than pretending. The
> alternative was for the *backend* to invent `Formatter`, which is a design
> decision arriving sideways from the component with the least standing to make
> it.

**Finding 6 — a representation the page permits and the crate does not
survive.** This is §2.6's static-string-literal case. The page's §7 establishes
that an empty container's pointer is dangling and non-null; nothing on it
forbids `len > 0` with `cap == 0`; and `science_dealloc` with `size == 0` is
documented as a no-op, which makes such a value look free-safe. It is not: the
internal `reserve` grows from `cap == 0` through `science_realloc` with
`old_size == 0`, which the page's own module documentation says "allocates
afresh" — and copies nothing, silently losing the first `len` bytes on the
first `push_str`. The invariant `cap == 0 ⇒ len == 0` is real, is relied on,
and is not written down.

### 9.4 The verdict

> **The claim survives in its letter and fails in its spirit, in six places,
> one of which is silent memory corruption and one of which blocks the language's
> own entry-point contract.**

That is a better result than it sounds, and it deserves the qualifier. Every
one of these six was found in a day, by one reader, by doing exactly what the
page invites — and *that is the claim working*. A runtime whose seam was never
written down produces the same six defects and produces them at stage four of a
bootstrap, in a miscompiled binary, with nothing to check against. `self-hosting.md`
§1.3 is right that this is the most bootstrap-ready artefact in the repository.
It is not right that it is finished.

Three of the six (1, 4, 6) are one sentence each on the page. Two (2, 3) are a
table. One (5) is new runtime code and a design decision about what a Science
program's entry point *is*, which belongs to `script-mode.md` and this note
jointly. All six are in §13.

**And two that were fixed while this note was being written, which is worth
recording as evidence rather than tidying away.** `self-hosting.md` Gate A3
asks for two things: the `link_` versus `science_` prefix defect, and the
`Result`/`Option` residue in §5, §9 and `io.rs`. Both are now done. The prefix
was corrected in §8 in place, with the reasoning kept — *"it is recorded rather
than quietly corrected because this page invites a code generator to be emitted
from it alone"* — and §5 has since been rewritten against the revision-2 error
model, gaining a §5.4 for the pair return, with §8.1 kept as the record of what
was wrong. `io.rs` implements it: `ScienceIoResultString` and
`ScienceIoResultUnit` are gone, replaced by `ScienceStringAndIoError` and
`ScienceNullableIoError`.

> **Gate A3 is discharged.** The six findings above are all *new*, all found
> after that repair, and all found by the same method. That is the useful
> reading of §9.4's verdict: the page is good enough that reading it carefully
> is productive, and not yet good enough that reading it carelessly is safe.

**One consequence for §3.4 worth naming**, because it is the first time
`type-checking-and-mir.md` Decision 6 has been applied by anyone:
`ScienceNullableIoError` takes the **discriminant byte** rather than the niche
that `IoError`'s 251 unused code points appear to offer, and the runtime's
§8.1 gives the reasoning. That agrees with Decision 19 of this note — a niche
exists only where the *type* guarantees the bit pattern is unreachable, and an
integer newtype with five named constants guarantees nothing of the sort,
because the next version of the enum has a sixth. **It is the same argument
`ffi-c-boundary.md` §1.5 makes for importing C enums as integer newtypes rather
than as `choice`**, arrived at independently in a different note about a
different problem, which is the strongest kind of agreement available here.

---

## 10. What ships first

`self-hosting.md` §15 warns that a plausible staged plan "invites somebody to
begin at stage one", and its own answer is that stage one is a corpus file
rather than a lexer. This section is the codegen half of the same discipline:
**every stage below produces a program that runs and prints something, and no
stage is finished until an execution test asserts its output and exit code.**
That harness is Gate A2 and it does not exist either; it is stage zero's only
deliverable.

### Stage 0 — the harness and the subcommand

`sciencec build FILE` exists, produces an object file for a program whose body
is empty, drives the linker, and produces an executable that exits 0. Nothing
of the language works. What is proved: the LLVM binding links, a target machine
is created with `AllowFPOpFusion = Strict`, the module verifies, the linker
driver is found on all three platforms, and `science-rt.a` links. The execution
test harness of core spec §10 layer 4 exists in `science-testkit`'s
closure-parameterised style.

Not a running program in any interesting sense, and it is the only stage that
is not.

### Stage 1 — `hello world`

```science
print("hello, world")
```

Requires, and this is the whole list: the script body of `script-mode.md` §2.1
lowered to `def main() -> Error?`; a string literal as a
`private unnamed_addr constant` plus a `science_string_from_bytes` call
(Decision 15); a `science_print` call; a `science_string_free` on the
temporary; and the
`sret` convention, immediately, because `science_string_from_bytes` returns
`ScienceString` by value. One basic block, no CFG, no generics, no layout beyond
`ScienceString`.

> **AMENDMENT 3: this list had two things in it that do not exist, and the
> second is the one that would have cost a day.**
>
> `science_println` was renamed to `science_print`; the runtime's §2 records
> that the two were the wrong way round and that the survivor is named for the
> Science spelling it implements.
>
> **And stage 1 needs no drop glue.** Glue is a *function* codegen defines, per
> Decision 12, for a type whose fields need dropping. A `String` is not such a
> type: the temporary is freed by a direct `science_string_free` at the site.
> "Drop glue for one `String`" tells the reader to write a function that must
> not exist — and to write it in the very first thing they emit, before anything
> else works well enough to show them it is unnecessary.
>
> The list is otherwise exact, and stage 1 has since been emitted from it:
> `sciencec build hello.science` produces a program that prints and exits 0.

**It goes through MIR even though it does not need to.** A THIR-to-IR shortcut
for stage 1 is a second lowering that has to be deleted at stage 3, and the
deletion never happens.

**Gate:** the binary runs, prints `hello, world\n`, exits 0, and the emitted IR
at `-O3` contains no fast-math flag.

### Stage 2 — a call into a C library, and the argument for it being second

```science
unsafe extern "C" library "m":
    def cos(x: F64) -> F64

let x be unsafe: cos(0.0)
print(f"{x}")
```

**This is the note's most contestable decision and it is deliberate.**

**The case against putting it second** is real: hello world exercises no
control flow and a C call exercises no control flow, so the CFG lowering
arrives at stage 3 with two features already built on top of a backend that has
never emitted a branch. The natural stage 2 is arithmetic and a loop.

**The case for, which wins.** The project's thesis is the C boundary.
`type-checking-and-mir.md` §15 names as the largest untested assumption in the
project that *"no Science program has ever called a Python function, because
there is no code generator"* — and the same sentence is true, today, of C.
`ffi-c-boundary.md`, `c-binding-coverage.md`, `binding-inventory.md`,
`native-dependencies.md` and `scientific-libraries.md` are five notes and
roughly four hundred kilobytes of design resting on the proposition that an
`extern` declaration becomes an LLVM `declare` and the system linker resolves
it. **The cheapest possible test of that proposition is four lines and `-lm`,
and it is available the day after hello world.**

And the library is `libm`, not BLAS, and that choice is the point. `cos` takes
an `F64` and returns an `F64`: no aggregates, no spans, no arrays, no
descriptors, no `pkg-config`, no ILP64 question. What it *does* exercise is the
entire declaration-to-link path — the `declare`, the symbol name, the `library`
clause becoming `-lm`, and `SC0461` when you typo the symbol. A stage that
proves the thesis with no infrastructure is worth more than one that proves it
with all of it.

**Gate:** the program prints `1`, exits 0; changing `cos` to `cosinus` produces
`SC0461` naming the declaration's span and the library clause, and not `ld`'s
output.

### Stage 3 — control flow and integers

A loop that computes something, a comparison, a `break`, a `match` over a
`choice` with no payloads, an `if err?:` over an `Int?`.

```science
let mut total be 0
for i in 0..10:
    if i > 5:
        break
    total = total + i
print(f"{total}")
```

Adds: the CFG (Decision 5), `alloca`-per-local (Decision 8), `switch` for
`match`, the tagged enum layout (§3.3), the null-niche test, bounds checks
(Decision 10), and the `(T, Error?)` pair as a two-field struct.

**Gate:** `tests/ui/` gains its first execution tests; the MIR dump and the IR
dump have the same block structure at `-O0`; and **the benchmark suite of
Decision 44 starts here**, with this loop as its first entry, measured against
C, Rust and Go from the day the compiler can emit a loop at all.

### Stage 4 — the runtime boundary in full

`String`, `Array of T`, `Map of (K, V)`, `Box of T`, drops, and therefore
monomorphisation and the descriptor convention. This is Gate C1 —
*"builds a `Map of (String, Int)`, a `Set of DefId`, an `Array of (Box of Expr)`,
splits a file into lines, formats a dump through a `BufferedWriter`, and writes
it"* — and it is by a wide margin the largest stage.

Adds: all 45 runtime entry points, `ScienceTypeInfo`/`ScienceMapInfo` emission
(Decision 20), drop glue (Decision 12), drop elaboration's consumer side
(Decision 13), the mangler and the collision check (`SC0404`), and generics.

**Gate:** Gate C1's output is byte-identical across two runs and two machines,
which is the first time the reproducibility claim is tested by execution rather
than by argument.

### Stage 5 — the `extern` boundary in earnest

`ffi.Span of F64` from a `borrowed Array of F64`, `via pkg-config "openblas"`,
`kind static`, `when available` and its `dlopen` table, `SC0429`'s refusal of
by-value aggregates, `SC0431`, and the `noalias` attribute of Decision 24.

`examples/20_extern.science` is the acceptance file and it already exists.

**Gate:** a `cblas_ddot` against a real OpenBLAS returns the right number on all
three platforms, under both `pkg-config` and a bare `library` name; a program
with a `when available` block for `cudnn` starts and reports
`is_available()` as false on a CPU-only machine.

### Stage 6 — the acceptance program

`examples/00_kitchen_sink.science`, which is core spec §11's definition of done
condition 1 and `self-hosting.md` Gate A1, and which already exists and already
carries its ten requirements numbered in comments. Adds what remains: interfaces
dispatched both statically and dynamically, `any Error` as a fat pointer, const
generics through the mangler, closures and the callback trampoline, and a type
holding a borrow in a field.

**Gate:** `sciencec build examples/00_kitchen_sink.science` produces an
executable that runs and exits 0. **Gate A closes. F0's backend is done.**

### What the staging is buying

Stages 1, 2 and 3 are, together, a small fraction of the work and they answer
three independent questions — does the toolchain link, does the FFI thesis
hold, does the CFG lowering work — each in a program small enough that a
failure has one possible cause. Stages 4 and 5 are the bulk. Stage 6 is
assembly.

The ordering's one real bet is stage 2, and the bet is that finding out the
`extern` path is broken in week two is worth more than the tidiness of building
control flow before foreign calls.

---

## 11. Diagnostics

Claimed: **`SC0400`–`SC0409`**, which is the whole of the free General codegen
sub-range in the README's partition, and nothing else. The partition gives
`SC0410`–`SC0449` to `ffi-c-boundary.md`, `SC0450`–`SC0459` to the Python
boundary, `SC0460`–`SC0461` to linking, `SC0462`–`SC0470` to `rust-interop.md`,
`SC0471`–`SC0479` to `native-dependencies.md`, and leaves `SC0480`–`SC0499`
free. This note takes none of those.

> **AMENDMENT 1: `SC0480`–`SC0499` are not free, and this table is one code
> short.**
>
> The README gives `SC0480`–`SC0489` to `rust-binding-generation.md` and
> `SC0490`–`SC0499` to `c-binding-coverage.md`. The partition quoted above was
> accurate when written and the README's claims table has since moved; the
> README is the allocation record and this paragraph is a copy of it, which is
> the failure mode that section warns about in as many words — a derived list
> maintained by hand is a cache with no invalidation.
>
> **The practical consequence is that the codegen band has no free code at
> all**, which is why `SC0407` — infinite instantiation, from the
> monomorphisation walk — was taken from the three this note reserves for §4.3's
> ABI classifier rather than from a free pool that does not exist. Two of those
> three remain, which is what the reserving sentence asks for. The table below
> should gain its row.

| Code | Meaning |
|---|---|
| `SC0400` | A toolchain feature required to build this program is not compiled into this `sciencec`. Names the feature and how to obtain a build that has it. **This discharges `data-io.md` §7's ask** for *"a diagnostic in the `SC0400` range that names the feature and how to get it"*, which was the only outstanding claim on this sub-range. |
| `SC0401` | No linker driver found. Names the candidates searched, in order, and the `SCIENCE_LINKER` variable. |
| `SC0402` | The linker failed for a reason that is not an attributable undefined symbol. Prints the command line and the linker's output verbatim, and says that it is doing so (§5.5). |
| `SC0403` | `sciencec build` was given a file with no entry point — neither top-level statements nor a `function main`. `script-mode.md` §4.2 rule 3 defines this situation as a library build and does not say what happens when somebody asks for a binary. |
| `SC0404` | Two distinct monomorphisation keys produced the same mangled symbol. An internal consistency check, not a user error, and it exists because `type-checking-and-mir.md` §15 says the dual failure — one key producing two symbols — *"is not a type error, not a link error on most platforms, and shows up as a performance mystery or a pointer-equality failure long after the cause"* (§2.7). |
| `SC0405` | A `static` or `const` initialiser is not a constant expression codegen can emit into a data section. |
| `SC0406` | The requested target triple is not the host's. Cross-compilation is out of scope for F0 per core spec §12, and the diagnostic says so rather than failing obscurely in the linker. |
| `SC0407`–`SC0409` | **Reserved, unused.** Held for the ABI classifier of §4.3, which will want at least one code for an aggregate shape it cannot classify. Reserving three is cheaper than reopening the partition. |

**Amended, not claimed:** `SC0429` (`ffi-c-boundary.md`), which §4.2 extends to
function-pointer types. `SC0461` (`ffi-c-boundary.md`), whose rendering
obligation §5.5 specifies and whose fallback is `SC0402`.

**Referenced:** `SC0424`, `SC0431`, `SC0433`, `SC0461` — all
`ffi-c-boundary.md`'s; `SC0520`, `SC0522` — `type-checking-and-mir.md`'s;
`SP0022` — `package-manager.md`'s, which is what an LLVM-version mismatch
raises.

**Not edited:** `docs/superpowers/design/README.md`. The row belongs to whoever
maintains the allocation record.

---

## 12. Contradictions found

Recorded rather than smoothed, per the convention.

1. **Core spec §7.1 names `inkwell`.** Decision 1 amends it to LLVM-C. The spec
   line should read `LLVM-C -> LLVM IR`.

2. **`science-rt` §2's `sret` list omits `science_array_with_capacity`.** §9.2.
   Silent memory corruption for a codegen that treats §2 as the specification.

3. **`science-rt` §4's `usize` rule is false of eight entry points and of every
   container's length and capacity fields.** §9.3, finding 3. It is true only
   because all three F0 targets are 64-bit.

4. **`science-rt` gives no parameter-position rule for the element descriptor,
   and `Box` disagrees with `Array` and `Map`.** §9.3, finding 4.

5. **`science-rt` has no program entry point, no stderr writer that does not
   abort, and no `science_exit`, so `script-mode.md` §2.3's exit-code table
   cannot be emitted.** §9.3, finding 5. This is a contradiction between two
   existing artefacts that neither one knows about.

6. **`science-rt`'s §7 permits a `ScienceString` with `len > 0, cap == 0`, and
   the crate's `reserve` does not survive one.** §9.3, finding 6. The invariant
   `cap == 0 ⇒ len == 0` is real and unwritten.

7. **`type-checking-and-mir.md` §12 guarantees explicit drop points and says
   nothing about drop elaboration or drop flags for conditionally moved
   values.** §2.5, Decision 13. Codegen cannot synthesise them and region
   inference requires them to exist before it runs, so it is a MIR obligation
   with no owner.

8. **`region-inference.md` §14 does not list `noalias` among its risks, and it
   is the one place where a bug in that note produces a wrong answer rather
   than a missed error.** §4.4, Decision 24.

9. **`indexing-and-array-literals.md` does not say what an out-of-range index
   does at run time**, and this note decides it emits a bounds check and a
   panic. The surface note should confirm, because "region inference proves the
   program memory-safe" is a sentence that reads, wrongly, as "no bounds
   check".

10. **Core spec §3's "self-contained binaries" is contradicted by the
    self-hosted compiler itself**, which has a hard dynamic dependency on
    `libLLVM` (§1.5), and is weakened generally by
    `native-dependencies.md` §9's correct refusal to statically link glibc
    (§5.3). The promise as worded is not one the project keeps and the wording
    should be tightened to "no Science or vendor libraries at run time".

11. **The README's allocation table double-claims `SC0458`.**
    `script-mode.md`'s row claims `SC0458`; `python-interop.md`'s row claims
    `SC0450`–`SC0458`. `python-from-science.md` §9 has already noticed —
    it says `SC0459` is *"the last free code in `python-interop.md`'s codegen
    block, `SC0458` having gone to `script-mode.md`"* — so the fix is to narrow
    the `python-interop.md` row to `SC0450`–`SC0457`. That is a fifth collision
    in the space the README says has produced four. **This note does not edit
    that file.**

12. **`reproducibility.md`'s `[build]` table records `opt` but not the pass
    pipeline name or the target CPU.** §7.1 and §7.4 make both into values
    that change numbers. The table should gain `"pipeline"` and `"cpu"`, and
    `cpu` must be the resolved concrete name, never the string `native`.

13. **`intrinsics-math-physics.md` §3.1's IEEE-exactness column is being used
    here for a purpose that note did not anticipate** — as the whitelist of
    intrinsics that are safe for LLVM to constant-fold against the build host's
    libm (§7.3, Decision 37). That note should know, because if a row's
    exactness claim is ever relaxed, a reproducibility guarantee moves with it.

14. **`collections-and-chains.md` §7.1's "defined order" is being read as
    strictly sequential and costs the vectoriser 4–8× on `sum`.** §7.5 asks for
    it to be defined as a blocked order instead, which is deterministic,
    numerically better, and vectorisable.

15. **No performance target existed anywhere in `docs/` before §8.** That is
    not a contradiction between two notes; it is a hole underneath all of them.
    `reproducibility.md` prices its float policy at "a benchmark deficit
    against toolchains that contract by default" without naming the
    toolchains; `type-checking-and-mir.md` Decision 5 accepts that "F0's array
    performance is whatever the callee does, with no fusion at all";
    `region-inference.md` §14 accepts an SCC-sized re-analysis. Each is a
    defensible trade against an unstated baseline, and three defensible trades
    against no baseline is how a language ends up slow by consensus.

16. **`region-inference.md` Decision 8 and `self-hosting.md` together imply
    something neither states: the self-hosted Science compiler is the worst
    case for its own incremental recompilation.** §8.3. A recursive-descent
    parser is one SCC, that note already says so, and `self-hosting.md` plans
    to write one in Science. Gate I should measure it.

---

## 13. What this note asks of the others

| Ask | Of | Why |
|---|---|---|
| **Add `science_array_with_capacity` to §2's `sret` list** | `crates/science-rt/src/lib.rs` | §9.2. One line. Silent corruption without it. |
| **Put the three function-pointer signatures on the page** | same | §9.3 finding 2. Codegen emits all three and the page does not say what they are. |
| **Restate §4's `usize` rule as "usize is 64 bits on every F0 target", and list the entry points that take it** | same | §9.3 finding 3. The rule as written is false of its own interface. |
| **State the descriptor's parameter position, and say that `Box` differs** | same | §9.3 finding 4. Two pointers, no type error, wrong call. |
| **State the invariant `cap == 0 ⇒ len == 0`, or make `reserve` copy when it does not hold** | same | §9.3 finding 6. The second option unlocks zero-allocation string literals (§2.6). |
| **Add a stderr writer that does not abort, and `science_exit(code: I32)`** | same, and `script-mode.md` §2.3 | §9.3 finding 5. Without them a `main` returning an error cannot be lowered, and it is the language's own error idiom. |
| **Add a `dlopen`/`dlsym` table for `when available`** | same, and `ffi-c-boundary.md` §5.2 | §5.4. That note specifies the lowering as living "inside `science-rt`", and there is no such symbol. |
| **Own drop elaboration: say where drop flags for conditional moves are inserted** | `type-checking-and-mir.md` §12 | §2.5 Decision 13. Codegen cannot invent them; region inference needs them to already exist. |
| **Add `noalias` to §14's risk list** | `region-inference.md` | §4.4. It is the one place a soundness bug becomes a miscompile. |
| **Say what an out-of-range index does at run time** | `indexing-and-array-literals.md` | §2.4 Decision 10. |
| **Add `"pipeline"` and a resolved `"cpu"` to the build record** | `reproducibility.md` §5.2 | §7.1, §7.4. Both change numbers; neither is recorded. |
| **Define the reduction order as blocked rather than sequential** | `collections-and-chains.md` §7.1 | §7.5, §8.2. It is the largest single performance item the float policy costs, it is the one exclusion on Decision 39, and the fix gives up no determinism. |
| **Add a CGU-count field to the build record when codegen units land** | `reproducibility.md` §5.2, `package-manager.md` Decision 3 | §8.5. Cheap now, retroactive after somebody publishes a number. |
| **Measure re-analysis time for a one-character edit inside the parser's SCC, at Gate I** | `self-hosting.md` §8 Gate I | §8.3. It is what "no lifetime syntax" costs per keystroke, and nobody has seen the number. |
| **Amend §7.1 to `LLVM-C -> LLVM IR`; tighten §3's "self-contained binaries"; split `science-codegen` into two crates in §7.2** | the core spec | §1.3, §5.3, §8.4. The crate split is what turns Decision 42's line from a review rule into a `cargo` check. |
| **Narrow `python-interop.md`'s codegen row to `SC0450`–`SC0457`; add this note's row with `SC0400`–`SC0409`** | `docs/superpowers/design/README.md` | §12 item 11. **This note does not edit that file.** |

---

## 14. Deliberately not built

- **A JIT.** Core spec §3 puts the interactive tier in F1 and the query
  database is what it runs on. An ORC JIT over the same module builder is
  additive.
- **Cross-compilation.** Core spec §12. `SC0406`.
- **Codegen units and parallel codegen.** Decisions 4 and 43, and the day they
  arrive the CGU count becomes a `[build]` field (§8.5).
- **A second backend of any kind** — a fast non-optimising one for debug
  builds, a GPU one, a Cranelift one. Decision 42 draws the line that makes
  adding one additive; it does not add one, and F0 should not. The insurance is
  the deliverable.
- **LTO in either direction.** §5.6.
- **Unwinding, `invoke`, landing pads, and any personality routine.**
  Decision 6, following `panic.rs`'s own argument. F1's supervisors will want
  it and should get to decide what a failed actor's state means before the
  mechanism is fixed.
- **Variable-level debug info.** Decision 30, with Decision 31 as the hook that
  makes adding it a new module rather than a refactor.
- **`-Os`, `-Oz`, PGO, BOLT, and any custom pass.** Core spec §12.
- **Sanitizers.** ASan would be genuinely useful for testing `unsafe` blocks
  and the `extern` boundary, and it is a whole instrumentation pass plus a
  runtime plus three platforms' shadow-memory setups. F1, if `unsafe` turns
  out to be load-bearing.
- **A demangler.** Decision 16 makes names deterministic and long.
  `sciencec demangle` is fifty lines whenever anyone wants it.
- **A stable Science ABI.** Decision 22. Only the `extern "C"` boundary and the
  `science-rt` boundary are compatibility surfaces.
- **Incremental codegen / object-file caching.** §5.6.
- **Varargs.** §4.5.
- **Any target that is not one of the three triples.** No wasm, no GPU
  backends; `intrinsics-math-physics.md` and `native-dependencies.md` own what
  a GPU target would mean and both put it past F0.
- **A niche in `Bool` or any range-restricted scalar.** §3.4.
- **Field reordering.** Decision 17, and it costs real bytes.

---

## 15. Risks

**The ABI is where this goes wrong and §4 mitigates rather than solves.**
Decision 21's split — implement returns, refuse aggregate arguments — is
defensible and it is still a bet that the target libraries really do pass
aggregates by pointer. `ffi-c-boundary.md` §5.4 checked five of them. The sixth
library somebody wants is the risk, and the failure mode is a `SC0429` that
blocks a user rather than a miscompile, which is the right direction to fail.
The worse risk is that the *return* classifier — which cannot be refused — is
wrong for some shape, and a wrong return classification is exactly as silent as
a wrong argument one. §4.3's differential test against `clang` should be built
at stage 2, not at stage 5, even though the classifier is not needed until
stage 4.

**`noalias` is the one attribute that converts a design bug into a wrong
number.** §4.4. `region-inference.md` is a specification of an engine that does
not exist, the engine is the hardest thing in the project, and the first time
anyone finds out whether it is sound is after codegen has been emitting
`noalias` on its conclusions for a year. `--no-noalias` is three lines and it
should exist from stage 4.

**The float policy is three obligations that are satisfied by not writing
something.** §7.3. Two of them (no fast-math flags, no transcendental
intrinsics) are absences, and absences are not tested by anything unless
somebody writes the test that greps for them. The third (`AllowFPOpFusion =
Strict`) is worse, because LLVM defaults *against* the policy: a code generator
written correctly in every other respect, that simply does not touch that
field, silently contracts on every FMA-capable machine. **That is the single
highest-value line in this note and it is one line, which is exactly why it
will be the one that is missing.**

**Decision 1 trades compile-time safety for portability and the trade is not
free.** Raw `LLVMValueRef` everywhere means a class of bug that `inkwell` would
have made impossible is now caught by the verifier, at run time, on whatever
inputs the test suite happens to contain. The mitigation is Decision 34 and a
newtype layer, and neither is as good as a type system. If the Rust
implementation accumulates verifier failures during stages 3 and 4, that is the
signal that the newtype layer was too thin, not that Decision 1 was wrong — the
bootstrap argument does not weaken.

**Nobody has ever linked a Science program against anything.** §10's stage 2
exists because of this. Five notes and several hundred kilobytes of FFI design
rest on `declare` plus `-lfoo`, and the proposition is almost certainly true
and has never once been executed. If it is false in some way — the extern
grammar cannot express a parameter shape, `pkg-config`'s output contains a flag
the driver mishandles, the `ffi.Span`-to-pointer lowering loses something — it
is better to find out in a four-line program than in `cblas_dgemm`.

**The compile-time target will not be met and the plan for missing it is thin.**
§8.3 names three causes and offers two answers: `sciencec check` for the edit
loop, and Decision 42's interface so that a fast debug backend is additive
later. Neither makes a full `-O2` build fast, and there is no third answer.
The specific thing that could make this worse than predicted is §8.3's third
cause, because it is the only one that is *ours*: if SCC-granular invalidation
turns out to mean a multi-second re-analysis on every keystroke in the
self-hosted parser, the language's own developers hit it first and hardest, and
the fix is either Polonius-style per-loan analysis, or a syntax, or living with
it. That is `region-inference.md`'s decision to revisit and it needs the
measurement before it can be revisited at all.

**Decision 42's line will be crossed under deadline pressure.** An interface
enforced by review rather than by the compiler is an interface that erodes, and
the first time something above the line needs a fact that only the backend
has — a target detail, an LLVM type — the cheap fix is a peek and the correct
fix is a plumbing change. The erosion is invisible until somebody tries to
write the second backend, which under §14 is nobody, for years. **If Decision 42
is worth having, it is worth a test that asserts the target-independent half
does not name LLVM at all**, which in Rust is a dependency direction that
`cargo` can check for free.

**Gate J's determinism is asserted, not demonstrated.** Decision 4's sorted
emission order closes the source `self-hosting.md` §15 names. It does not close
the ones nobody has named, and LLVM's own output is deterministic given
deterministic input in a way that is widely believed and occasionally false
across versions. The cheapest early signal is to build the corpus twice at
stage 1 and compare hashes, and to keep doing it at every stage, because the
build that first differs tells you which stage introduced it.

**This note specifies a backend nobody has started.** Every decision above is a
reading of three design notes, one runtime crate and an LLVM API, and not one
line of it has been compiled. The runtime contract is the only part with a test
suite, and §9 found six defects in it — in the artefact the project considers
its most bootstrap-ready. That ratio is the honest estimate of how much of this
note survives contact with a working `sciencec build`.
