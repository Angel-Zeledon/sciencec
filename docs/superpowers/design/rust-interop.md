# Science — Design: Rust interoperability

Date: 2026-09-16
Status: draft for review
Phase: F0 for the mechanism, F1 for the first consumers
Depends on: `ffi-c-boundary.md` in its entirety — this note adds no ownership
case, no `unsafe` power, no error mechanism and no `ffi` type that note does not
already have. It adds one contextual clause, one build step, and one
verification trick that C could not have offered.
Related: `syntax-revision-2.md` §8 (which this note develops rather than
re-derives), `python-interop.md` §2 and §6 (the sibling ecosystem),
`stdlib-shape-and-packages.md` §3 and §6, `stdlib-standard.md` §9,
`scientific-libraries.md` §15.

Syntax: revision 2 throughout (`->`, `is`/`is not`, `-> (T, Error?)`, `T?`,
`interface`, `Type has:`, `print`). Where this note quotes `ffi-c-boundary.md`,
whose samples predate the revision, it silently translates `returns` to `->` and
`Result of (T, E)` to `-> (T, E?)`. That is the drift the README's conventions
section describes, not a disagreement.

---

## 0. The instruction, and what this note does with it

The instruction is that **Rust compatibility is a primary goal of the project** —
a stated objective, not an FFI target among others.

This note does not argue with that. It argues that the phrase names five
different things which cost between "already done" and "a second compiler", that
four of the five are worth some amount of work and one is worth none, and that
the one everybody means — *can a Science program use a crate from crates.io* —
has an answer that is genuinely good and genuinely narrower than the phrase
implies.

The short version, stated at the top because a reader who stops here should stop
with the right impression:

> **Science can link Rust, and linking Rust is worth a great deal. Science
> cannot consume crates. The difference between those two sentences is a
> hand-written shim per library, and no tool will ever remove it, because the
> information a shim supplies is information Rust's type system has and Science's
> cannot receive.**

Everything below is the working-out. §1 separates the five meanings. §2 is the
ecosystem argument and the worked crate. §3 is where the two ownership models
agree, which is the technically interesting part and is the reason any of this is
cheap. §4 is the mechanism. §5 is the toolchain. §6 is the list of things this
note refuses and what it offers instead. §7–§9 are diagnostics, asks and risks.

---

## 1. Five meanings of "compatible with Rust"

### 1.1 The five

**ABI compatibility.** Can a Science function be called by Rust, and a Rust
function by Science, with no shim — passing each other's native types directly?

The answer is that **nobody has this, including Rust.** `repr(Rust)` is
explicitly unspecified: field order, padding, niche placement and enum
discriminant encoding are all free for the compiler to change, and rustc changes
them. Two crates compiled by different rustc versions are not guaranteed to agree
on the layout of a `struct` either of them defines. There is no versioned ABI, no
`extern "rust-v1"`, and the long-running work on a stable cross-language ABI has
produced no stable surface.

The only stable contract Rust offers is `extern "C"` plus `#[repr(C)]`, and that
contract is the one `ffi-c-boundary.md` already implements in full. So "ABI
compatibility with Rust" is not a thing Science can pursue harder; it is a thing
Science already has as much of as exists.

**Source compatibility.** Can `sciencec` compile Rust code? Out of scope, and one
paragraph is the right amount of attention:

> Compiling Rust means implementing trait resolution with coherence and
> negative-reasoning corner cases, explicit lifetimes and variance, the borrow
> checker's NLL formulation, `unsafe` and its aliasing model, `const` evaluation,
> declarative macros, and — the item that ends the conversation — **procedural
> macros, which are arbitrary Rust programs that run at compile time.** Compiling
> a crate that uses `serde_derive` requires being able to *execute* Rust, so a
> Rust front end is not a front end, it is a Rust implementation that must agree
> with rustc bug for bug on code the whole ecosystem depends on. The thing it
> would buy — the ability to read crate source — is available far more cheaply by
> the expedient of running rustc, which is what §4 does. Refused. It is not a
> question of budget.

**Ecosystem compatibility.** Can a Science program use crates from crates.io?
This is the one that matters and §2 is about it.

**Toolchain compatibility.** Target triples, cargo, rustup, cross-compilation,
LTO. A grab bag with wildly different prices inside it; §5 takes it apart and
buys two cheap pieces.

**Semantic compatibility.** Do the two ownership models agree? This is not a
deliverable at all — it is an analysis — and it is the highest-value item per
unit of work in the list, because its finding is *yes, to a startling degree*,
and that finding is what makes the boundary thin enough to be worth crossing.
§3.

### 1.2 The ranking

By value per unit of work, which is what was asked:

| Rank | Meaning | Value | Work | Verdict |
|---|---|---|---|---|
| 1 | **Semantic** | High — it is what makes every other item cheap | This note, plus one codegen constraint (§3.6) and one generated-assertion backend (§4.5) | **Pursue. Cheapest thing in the list and it is nearly free.** |
| 2 | **ABI** | High | **Already spent.** `ffi-c-boundary.md` §1–§2 is the whole of it | **Pursue exactly as far as `extern "C"` + `#[repr(C)]` reaches, and not one step further.** |
| 3 | **Toolchain** | Moderate | A thin slice is days; the rest is a project | **Pursue the slice: triple spelling, `cargo build` as a build step, `--locked`. Refuse rustup management, cross-compilation and cross-language LTO.** |
| 4 | **Ecosystem** | **Highest absolute value in the project, and the reason the goal was stated** | Largest. A shim per library, forever, plus a curated set and its maintenance | **Pursue, in the restricted form §2.6 defines. It is the prize and it is the only item whose cost is open-ended.** |
| 5 | **Source** | Low — it duplicates what running rustc gives | A Rust implementation | **Refuse.** |

The ordering is by ratio and the recommendation is not purely by ratio, so both
are stated. Items 1 and 2 should be done because they are nearly free. Item 4
should be done *although* its ratio is the second worst, because it is the stated
objective and because it is the only item that changes what a Science user can
do. Item 3's slice should be done now because two of its pieces are one-way doors
(§5.1, §5.3). Item 5 should never be done.

> **Decision 1. "Rust compatibility" is pursued as: semantic agreement exploited,
> the C ABI used as the only wire format, a hand-written shim crate as the only
> binding mechanism, and cargo borrowed as a dependency resolver for Rust
> dependencies only.**
>
> **Rejected: pursuing `repr(Rust)` ABI compatibility by pinning a rustc
> version and reverse-engineering its layout algorithm.** It is technically
> possible — the layout algorithm is in the compiler and can be read. It is
> refused because it would make Science's binary compatibility a property of a
> rustc version number, so that a `cargo update` that bumped the toolchain would
> silently change struct layout in a compiled scientific program. The failure
> mode is `ffi-c-boundary.md` §1.6's ILP64 bug with a wider blast radius, and the
> cost is re-paid every six weeks forever.
>
> **Rejected: a Rust front end for `sciencec`.** §1.1.
>
> **Cost of Decision 1.** Everything crossing the boundary is spelled twice —
> once in a Science `extern` declaration and once in a Rust `extern "C"`
> definition — and a human writes the Rust half. §4.5 makes the two halves
> checkable against each other, which removes the *danger* of writing it twice
> but not the *work*.

---

## 2. Ecosystem compatibility, which is the real prize

### 2.1 The strategic claim, stated at full strength

Science has no package manager and no ecosystem. `stdlib-shape-and-packages.md`
§6.4 says the quiet part out loud — *"Level 3 is a category whose defining
property, separate distribution, is not implemented"* — and
`scientific-libraries.md` §15 names the same seam as the one that will hurt most:
*"a scientific language whose `linalg` requires the user to install OpenBLAS
first will be judged on that first experience."* Three sibling notes
independently reach "we would need a package manager for this".

Rust has both. crates.io contains, with permissive licences and no C++
dependency, precisely the supply a scientific language needs: `regex`, `serde`,
`rayon`, `ndarray`, `polars`, `nalgebra`, `flate2`, `zstd`, `ring`, `rusqlite`,
`arrow`, `hdf5` bindings, `image`, `csv`, `chrono`. Cargo resolves versions, has
a lockfile, builds from source, and works offline with `cargo vendor`.

So the claim to test is:

> **If Science can consume Rust crates, it inherits a mature ecosystem and a
> package manager at the same time, and several Level-3 packages stop needing to
> be written at all.**

### 2.2 The claim is half true, and this is which half

The half that is true is worth having:

- **Cargo resolves and fetches the *heavy* dependency.** A Level 3 Science package
  that wraps `zstd` stops being "a Science package plus a C library the user must
  install" and becomes "a Science package plus a `Cargo.lock` line". That is a
  direct improvement on `scientific-libraries.md` §15's seam, and it is a real
  one: `cargo build --locked --offline` over a vendored tree is a better
  distribution story than anything Science has for a C dependency today.
- **`Cargo.lock` is a reproducibility artefact Science does not have.**
  `stdlib-shape-and-packages.md` §6.2's third failure — *"there is no way to
  record what a result was produced with"* — is repaired for the Rust half of a
  program's dependencies, exactly and mechanically. §4.2 makes `--locked`
  mandatory for this reason.
- **Several Level-3 candidates get cheaper.** `compress` (§5.1 of that note) is
  `flate2` + `zstd`. `crypto`'s libsodium binding could be `ring` or
  `RustCrypto`. `text.regex` is §2.4 below.

The half that is false is the half that hurts:

> **Cargo resolves *Rust* dependencies. It does nothing whatever for
> Science-to-Science dependencies, which is the hole
> `stdlib-shape-and-packages.md` §6 actually names.**

A Science user who wants to depend on somebody else's Science package is in
exactly the position §6.2 describes, before and after this note. Two versions
still cannot coexist; a Science package still cannot depend on another Science
package; `SCIENCE_PATH` is still the whole mechanism. Borrowing cargo moves the
*foreign* dependency problem into a solved state and leaves the *native*
dependency problem untouched, and those are different problems that sound alike.

This is the single most important honest correction in the note, and §9 repeats
it as the risk most likely to be misread.

### 2.3 What consuming a crate actually requires

Take the question literally and answer each part.

**Who writes the `Cargo.toml`?** The user, in a sidecar directory, as an ordinary
Rust crate. `sciencec` validates four things about it and mutates nothing. §4.1
argues this against the alternative of generating it.

**Who invokes cargo?** `sciencec`, as a build step, before the link step, with
`--locked` and with the target triple it was given. §4.2.

**Who resolves versions?** Cargo, from the user's `Cargo.toml`, recorded in the
user's `Cargo.lock`, which is checked in. Science has no opinion and expresses
none.

**What fraction of a crate is reachable?** This is where the answer stops being
comfortable.

> **Zero.**

Not "a small fraction". Zero, for essentially every crate in the registry, and
the reason is structural rather than incidental. A crate's public API is made of
`&str`, `String`, `Vec<T>`, `Option<T>`, `Result<T, E>`, `Cow<'a, str>`,
`impl Iterator`, `impl Trait`, closures with generic bounds, and structs carrying
lifetime parameters. Not one of those has a defined C ABI. `#[repr(Rust)]` covers
all of them. There is no signature in `regex`'s public API — not one — that is
`extern "C"`-callable, and `regex` is a deliberately simple, dependency-light,
C-friendly crate by the standards of the registry.

The crates that *are* directly callable are the `*-sys` crates, and a `*-sys`
crate is a thin wrapper over a C library. Reaching one through Rust is strictly
worse than reaching the C library directly through `ffi-c-boundary.md`, because
it adds a build dependency and a layer to be wrong in.

**So what does the binding look like?** A hand-written Rust crate that exposes an
`extern "C"` surface over the crate's Rust surface. That is not a workaround; it
is the only thing that has ever worked. `cbindgen` does not generate bindings for
Rust APIs — it *transcribes* an `extern "C"` surface a human already wrote, into
a C header, and it can check nothing because nothing reads the header.
`wasm-bindgen`, `PyO3` and UniFFI all work the same way: the crate author, or a
wrapper author, annotates. UniFFI is the closest analogue to what this note
designs, and its model *is* the sidecar — a wrapper crate whose exported surface
is declared and whose bindings are generated from the declaration.

That is strong external evidence, from three projects with more resources than
this one, that there is no automatic path. §6 refuses the automatic path in
those terms.

### 2.4 The worked crate: `regex`, end to end

`regex` is chosen because the outcome is decidable. `stdlib-standard.md` §9.3
decides to **write** an RE2-style engine in Science and prices it as *"the
largest write-rather-than-link bet in the whole standard library — a few thousand
lines of real, subtle code, competing for the same engineer as `linalg`."* The
Rust `regex` crate is, line for line, the engine §9.2 specifies: Thompson NFA
with a lazy DFA, linear-time guaranteed, no backreferences, no lookaround, with
memchr and Teddy prefilters. It is MIT/Apache-2.0, has no C dependency, and is
the most-depended-upon crate in the registry. If a crate can ever be consumed,
this is the one.

**The target.** `stdlib-standard.md` §9.5 asks for exactly five functions:
`compile`, `matches`, `find`, `find_all`, `replace_all`.

**What crosses, function by function.**

`Regex::new(&str) -> Result<Regex, Error>`. Neither `&str` nor `Result` nor
`Regex` crosses. The shim flattens all three: pattern as pointer plus length,
status as an `int`, handle as an out-parameter.

```rust
// foreign/rust/src/regex_shim.rs
use core::ffi::{c_int, c_void};
use regex::Regex;

pub const OK: c_int = 0;
pub const BAD_UTF8: c_int = 1;
pub const BAD_PATTERN: c_int = 2;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn science_regex_new(
    pattern: *const u8,
    pattern_len: usize,
    out: *mut *mut c_void,
) -> c_int {
    let bytes = unsafe { core::slice::from_raw_parts(pattern, pattern_len) };
    let Ok(text) = core::str::from_utf8(bytes) else { return BAD_UTF8 };
    match Regex::new(text) {
        Ok(re) => {
            unsafe { *out = Box::into_raw(Box::new(re)).cast::<c_void>() };
            OK
        }
        Err(_) => BAD_PATTERN,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn science_regex_free(handle: *mut c_void) {
    if handle.is_null() { return }
    drop(unsafe { Box::from_raw(handle.cast::<Regex>()) });
}
```

That is `ffi-c-boundary.md` §2.3's **Case B**, unchanged: C hands us something it
allocated, Science frees it by calling the destructor, the compiler proves
exactly-once. The only Rust-specific detail is that the destructor's body is
`Box::from_raw` — which runs Rust's own `Drop` for `Regex` on the Rust side, so
the two languages' destructors compose through exactly one hop. §3.4.

`Regex::is_match(&self, &str) -> bool`. **Case A**, and the interesting thing is
what happens to the lifetimes:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn science_regex_matches(
    handle: *const c_void, text: *const u8, text_len: usize,
) -> c_int {
    let re = unsafe { &*handle.cast::<Regex>() };
    let bytes = unsafe { core::slice::from_raw_parts(text, text_len) };
    let Ok(hay) = core::str::from_utf8(bytes) else { return 0 };
    re.is_match(hay) as c_int
}
```

Both `&`s are *invented inside the shim body*, and rustc's own inference bounds
them by that body. Nothing about a region crossed the boundary; the call is the
common region. §3.2 generalizes this.

`Regex::find(&self, &str) -> Option<Match<'h>>`. `Match<'h>` borrows the
haystack, and `Option<Match>` is `repr(Rust)`. Both problems disappear at once
because what the caller actually wants is two integers:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn science_regex_find_from(
    handle: *const c_void, text: *const u8, text_len: usize,
    start_at: usize, out_start: *mut usize, out_end: *mut usize,
) -> c_int {
    let re = unsafe { &*handle.cast::<Regex>() };
    let bytes = unsafe { core::slice::from_raw_parts(text, text_len) };
    let Ok(hay) = core::str::from_utf8(bytes) else { return 0 };
    if start_at > hay.len() || !hay.is_char_boundary(start_at) { return 0 }
    match re.find_at(hay, start_at) {
        Some(m) => {
            unsafe { *out_start = m.start(); *out_end = m.end(); }
            1
        }
        None => 0,
    }
}
```

`find_all` is the same function. It is **not** a boxed `regex::Matches<'r, 'h>`,
and §3.7 explains at length why boxing it would be the one genuinely unsound
thing available here.

`Regex::replace_all(&self, &str, R) -> Cow<'t, str>`. `R` is a generic trait
bound (`Replacer`) so the shim picks one instantiation — a `&str` replacement —
and `Cow` is flattened by the two-call length protocol: call once with a null
buffer to learn the length, allocate on the Science side, call again to fill it.
That avoids handing a Rust `String` across, which would require a matching
Rust-side free function and re-open the allocator question of §3.4.

**What it costs, counted.**

| Piece | Rust | Science |
|---|---|---|
| `compile` + `free` + error text recovery | ~60 | — |
| `matches`, `find_from` | ~30 | — |
| `replace_all`, two-call protocol | ~35 | — |
| capture groups: count, offsets, names (Case D) | ~50 | — |
| generated `science_abi.rs` | ~40 (generated) | — |
| `extern "C"` declaration block | — | ~30 |
| safe layer: `Pattern`, `Found`, `Matches` | — | ~110 |
| **Total** | **~215** | **~140** |

Roughly **350 lines, of which 215 are Rust**, against *"a few thousand lines of
real, subtle code"*. One engineer, one week, including the tests.

**The outcome.**

> **This does reverse `stdlib-standard.md` §9.3, and it is worth being exact
> about what reversed it.** Not "Science can consume crates" — nothing about the
> general mechanism was used. What reversed it is that somebody wrote 215 lines
> of Rust. Zero of `regex`'s public API crossed the boundary unmodified; every
> single function was re-expressed. The saving is real and large, and it is the
> saving of *not implementing a lazy DFA*, not the saving of *not writing a
> binding*.

Two details the count hides, and they are the two that every shim
under-budgets:

- **Error text.** `Err(_)` above throws away `regex::Error`'s message, which is
  excellent and which `stdlib-standard.md` §9.4 depends on for its compile-time
  pattern check. Recovering it needs a second call and a caller-supplied buffer.
  Budget 25 lines per error type, per shim, always.
- **Capture group names.** `Regex::capture_names()` yields `Option<&str>` pointing
  into the compiled `Regex`'s own storage. This is `ffi-c-boundary.md` §2.6's
  **Case D** and it gets a real `from` clause: `-> ffi.CStr from pattern`. It is
  the only place in the whole `regex` shim where a borrow genuinely crosses, and
  §3.2's recommendation is to avoid even this one by returning offsets into the
  pattern string instead.

### 2.5 The Science side, in full

```science
unsafe extern "C" crate "regex_shim":
    const REGEX_OK be 0 as CInt
    const REGEX_BAD_UTF8 be 1 as CInt
    const REGEX_BAD_PATTERN be 2 as CInt

    function science_regex_new(
        pattern: ffi.Span of U8,
        pattern_len: CSizeT,
        out: mutable borrowed ffi.Uninitialized of ffi.OpaqueHandle) -> CInt

    function science_regex_free(handle: ffi.OpaqueHandle)

    function science_regex_find_from(
        handle: ffi.OpaqueHandle,
        text: ffi.Span of U8, text_len: CSizeT,
        start_at: CSizeT,
        out_start: mutable borrowed ffi.Uninitialized of CSizeT,
        out_end: mutable borrowed ffi.Uninitialized of CSizeT) -> CInt

type Pattern:
    raw: ffi.OpaqueHandle

Pattern implements ffi.CLayout

Pattern implements Drop:
    function drop(mutable self):
        unsafe: science_regex_free(self.raw)

Pattern has:
    ## Fallible at run time; checked at compile time when `source` is a
    ## literal, per stdlib-standard.md §9.4.
    function compile(source: borrowed String) -> (Pattern?, PatternError?):
        let mutable slot be ffi.Uninitialized of ffi.OpaqueHandle .new()
        let status be unsafe:
            science_regex_new(source.bytes(), source.length() as CSizeT, slot)
        if status is not REGEX_OK:
            return (null, PatternError.from_status(status))
        let raw be unsafe: slot.assume_initialized()
        return (Pattern(raw: raw), null)

    function find(self, text: borrowed String) -> Found?:
        self.find_from(text, 0)

    function find_from(self, text: borrowed String, start_at: Int) -> Found?:
        let mutable s be ffi.Uninitialized of CSizeT .new()
        let mutable e be ffi.Uninitialized of CSizeT .new()
        let hit be unsafe:
            science_regex_find_from(self.raw, text.bytes(),
                text.length() as CSizeT, start_at as CSizeT, s, e)
        if hit is 0:
            return null
        unsafe:
            return Found(start: s.assume_initialized() as Int,
                         end: e.assume_initialized() as Int)
```

Note what is *not* in that Science code: no lifetime, no `from` clause, no region
construct at all. `Found` holds two integers; the caller's `text` is borrowed by
Science's own region engine in the ordinary way, because the borrow never left
Science. That is the shape §3.2 argues for everywhere.

### 2.6 Is Rust a build dependency of every Science program?

Three things are being confused when this is asked, and they have three different
answers.

**The compiled binary.** Still self-contained. A Rust `staticlib` links in exactly
like any other `.a`; the resulting executable has no runtime dependency on
anything Rust. Core spec §3's decision — *"self-contained binaries"* — survives
completely intact. Nothing in this note touches it.

**The compiler.** `sciencec` is itself written in Rust (`crates/science-lexer`,
`crates/science-parser`, `crates/science-rt`). That is a fact about building
`sciencec`, not about using it. A shipped `sciencec` is a binary and its users do
not have rustc. **These two facts are routinely conflated and they must not be.**

**The user's build.** This is the real question, and the answer is a decision:

> **Decision 2. Rust is a *conditional* build dependency. A Science project needs
> cargo on the build machine if and only if it contains a `foreign/` sidecar.
> Standard-library and toolchain-level Rust dependencies are resolved and
> compiled when the *toolchain* is built and ship inside the tarball as object
> code, so a user of `text.regex` needs no Rust.**
>
> **Rejected: Rust as an unconditional build dependency**, i.e. always shelling
> out to cargo. It would mean a scientist on a cluster login node cannot compile
> a five-line Science program without a rustup install, which trades away exactly
> the first-experience property `scientific-libraries.md` §15 warns about.
>
> **Rejected: no toolchain-level Rust at all**, i.e. the standard library may
> never wrap a crate. It would forbid §2.4's result, which is the best single
> outcome in this note.
>
> **Cost.** The toolchain tarball is built per target and grows by the size of
> every vendored crate's object code — for `regex` with its default features,
> roughly 1.5 MB per target. The toolchain's own `Cargo.lock` becomes a
> release artefact that must be published alongside the version number, because
> it is now part of what a result was produced with. And there is a two-tier
> world: the stdlib's `regex` is pinned at toolchain build time while a user's
> sidecar `regex` is pinned by their own lockfile, so one machine can hold two
> different regex engines. §9 records that as a risk.

Second-order consequence worth stating, because it is a linker fact people
discover the hard way: **a Rust `staticlib` bundles Rust's `std`, including its
allocator shim and panic runtime, and linking two Rust staticlibs into one binary
produces duplicate-symbol errors.** So there is exactly one sidecar crate per
Science program, with each `foreign/*.rs` a module inside it. That is an argument
for the single-crate sidecar and against per-file compilation, and §4.1 takes it.

---

## 3. Where the two ownership models agree, and where they do not

Nobody has written this down and it is the reason the boundary is cheap.

### 3.1 The agreement, enumerated

| Property | Science | Rust |
|---|---|---|
| Exactly one owner | §6.1 rule 1 | yes |
| Assignment moves unless `Copy` | rule 2 | yes |
| Use after move is an error | rule 3, `SC0301` | yes, E0382 |
| Shared many, or exclusive one, never both | rule 4 | yes |
| No borrow outlives its referent | rule 5 | yes |
| Scope exit destroys, running `Drop` | rule 6 | yes |
| No garbage collector, no refcount by default | yes | yes |
| Monomorphized generics | core spec §5.3 | yes |
| Coherence by an orphan rule | `ffi-c-boundary.md` §1.2 | yes |
| Marker traits auto-derived structurally | `Share`/`ShareBorrowed` | `Send`/`Sync` |
| Native AOT through LLVM | core spec §3 | yes |

**That is a nearly complete correspondence, and it is the only such pair in
production.** C shares none of it: C has no ownership, no destructors, no move
semantics, no aliasing discipline the compiler enforces. `ffi-c-boundary.md` §2
exists to *impose* a model on a language that has none. Against Rust the work is
different in kind — not imposing a model, but noticing that the two models are
the same one and arranging for neither compiler to have to see the other's
notation.

That observation generates the design principle the whole of §3 turns on:

> **Neither region system should ever see the other's regions. They meet at a
> pointer, and the call is the common region. Every mechanism below is a way of
> arranging that.**

### 3.2 `borrowed T` crossing into Rust, and `&'a T` coming back

**Going in.** A Science `borrowed T` lowers to a bare `const T*` at the ABI
(`ffi-c-boundary.md` §1.3). No lifetime crosses because there is nothing at the
ABI for one to ride on. The `&'a T` the Rust callee needs is **invented inside
the shim body** by a `&*p`, and rustc's own inference bounds that lifetime by the
shim function's body — which is the duration of the call, which is exactly
`ffi-c-boundary.md` §2.2's **Case A**.

So the answer to "what does `borrowed T` become when the callee needs a `'a`" is:
*the callee's `'a` is manufactured locally and cannot escape, and Science's
region for the same borrow is the call.* The two are the same region by
construction and neither compiler learns of the other.

What can go wrong, and it is the same thing that goes wrong in C: a shim that
lets the invented `&'a` escape — stores it in a `static`, puts it in a
thread-local, hands it to a registered callback — has left Case A, and *neither
compiler will say so*. rustc will not, because the shim wrote `transmute` or
`'static` to make it compile; `sciencec` will not, because it cannot see the
shim. **The `borrowed` keyword in the `extern` declaration is the assertion, and
the block's `unsafe` marks that an assertion was made.** That is
`ffi-c-boundary.md` §2.7 item 1, verbatim, with no departure.

**Coming back.** A Rust function returning `&'a T` maps onto Case D and gets a
`from` clause. Here there is a piece of genuine luck worth preserving
deliberately:

> `ffi-c-boundary.md` §2.6 defines `from` elision as *"Rust's elision rule minus
> the `self` case"*. So for any Rust signature `fn f<'a>(x: &'a X) -> &'a Y`,
> whose `'a` Rust itself elided, the Science declaration elides `from` and means
> exactly the same thing. **The one region construct in the entire language was,
> by accident, already spelled to agree with Rust's.** That should be recorded as
> a property to keep rather than a coincidence to forget.

Where the luck runs out is multi-lifetime signatures. `fn f<'a, 'b>(x: &'a A,
y: &'b B) -> (&'a C, &'b D)` has no Science spelling — `from` names one
parameter, Science returns a tuple with no per-element region syntax, and
inventing one would be the lifetime annotation §6 of the core spec exists to
avoid. Such a signature is not expressible and the shim must flatten it.

> **Decision 3. A shim's job is to erase Rust lifetimes into indices, not to
> translate them into `from` clauses. A returned borrow is used only where the
> borrowed data has no index representation — an interned string in the library's
> own storage, a pointer into a long-lived arena.**
>
> **Rejected: mapping Rust lifetimes onto `from` clauses wherever they appear.**
> It works for the single-parameter case and fails for every other, so it would
> be a rule with a ragged edge that users would hit without warning. It also
> maximizes the number of unverifiable claims, when §2.4 shows the alternative
> costs two `usize` out-parameters.
>
> **Rejected: adding a multi-region return syntax to the `extern` grammar.** That
> is lifetime annotation with a different spelling, reversing a core decision to
> serve the FFI.
>
> **Cost.** The wrapper does slightly more work: a `&str` return becomes either a
> pair of offsets into a string the caller already holds, or a two-call
> length-then-fill protocol. Both are a few lines. Against that, `regex`'s entire
> shim contains exactly one `from` clause (capture-group names) and §2.4
> recommends removing even that.

### 3.3 Auto-borrow

Core spec §6.3 auto-borrows at call sites; Rust requires an explicit `&`. This is
purely a calling-convention difference and it is absorbed entirely by the shim: at
the Science call site the borrow is implicit, at the ABI it is a pointer, and in
the shim body it is an explicit `&*p`. The same applies to `borrowed Array of T`
coercing to `ffi.Span of T` at extern call sites (`ffi-c-boundary.md` §1.3) —
what arrives in Rust is `(ptr, len)` and `slice::from_raw_parts` is the explicit
`&` that Rust wanted. Nothing to decide.

### 3.4 `Drop`, and whether the two compose

Both languages run destructors at scope exit, deterministically, exactly once.
They do not share a scope, so nothing composes automatically. Taking
`ffi-c-boundary.md` §2.1's four cases in order:

**Case A — Science lends to Rust for the call.** No destructor crosses. Science's
value drops after the call in Science; the Rust borrow ends with the shim body.
No interaction at all.

**Case B — Rust allocates, Science frees.** This composes, through exactly one
hop, and it is the strongest result in this note:

> **A `Box<T>` and a Science handle type with `implements Drop` are the same
> object viewed from two sides.** `Box::into_raw` hands Science a thin pointer and
> relinquishes Rust's obligation; the Science handle takes it up under rules 2, 3
> and 6; `Box::from_raw` in the free shim hands it back and runs `T`'s Rust `Drop`.
> Each compiler proves its own half. The composed property — destroyed exactly
> once, at a program point each side can name — holds because neither side can
> drop twice and the handle is neither `Copy` nor `Clone`.

Two failure modes, both worth naming because both are common:

- **`Box::into_raw` on an unsized type produces a fat pointer.** `Box<dyn Trait>`
  and `Box<[T]>` are two words, and `ffi.OpaqueHandle` is one. The shim must box
  a concrete sized type, or double-box (`Box::new(boxed_dyn)`) to get a thin
  pointer. Getting this wrong truncates the pointer and the vtable is lost;
  rustc's `improper_ctypes_definitions` lint catches it, which is why §4.5 makes
  that lint mandatory.
- **Science guarantees `Drop` runs *at most* once, not *at least* once**
  (`ffi-c-boundary.md` §4.4). A leaked Science handle leaks the Rust value. That
  is safe on both sides and consistent with both models.

**Case C — Science hands ownership to Rust.** `ffi-c-boundary.md` §2.5's rule
holds unchanged — *only memory that came from a foreign allocator may be given
back to one* — and the reason is **stronger** against Rust than against C, not
weaker. Rust's deallocation requires the exact `Layout` (size *and* alignment)
that allocated the block, and `#[global_allocator]` may have replaced the
allocator entirely. So `Vec::from_raw_parts` over a Science-allocated buffer is
undefined behaviour even when both sides happen to sit on the system malloc.

> **Decision 4. Transfer of ownership from Science to Rust is copy-only in the
> first version. Either the shim allocates (`Vec::with_capacity`) and Science
> fills it through an `ffi.MutableSpan`, or Science copies into an `ffi.CBuffer`
> obtained from the Rust side's allocator.**
>
> **Rejected: making Science's allocator Rust's `#[global_allocator]`**, so the
> two heaps are literally one. It would make the transfer free. It is refused
> because it makes every crate in the dependency tree allocate through
> `science_alloc`, which is a change of behaviour for code Science did not write
> and cannot test, and because it constrains `crates/science-rt`'s allocator
> design forever in service of a rare operation. It is also fragile in exactly
> the way that is hardest to detect: it would work until one crate in the tree
> set its own allocator.
>
> **Cost.** One pass over the data on transfer-out. Identical to
> `ffi-c-boundary.md` §2.5's accepted cost, for the same reason.

**Case D — Rust lends to Science.** `from p` / `from static`, per §3.2, used
sparingly.

**Drop *order* across the boundary is not observable and must not be relied
upon.** If a Rust value holds a callback into Science while a Science value holds
the handle, the interleaving is whatever the two scope exits happen to produce,
and neither side detects the cycle. The same is true across the C boundary and
this note adds nothing to it.

### 3.5 `Send`/`Sync` against `Share`/`ShareBorrowed`, and the trick this makes possible

`stdlib-standard.md` §10.2 names Science's markers `Share` (may be moved across a
thread boundary) and `ShareBorrowed` (may be borrowed across one), rejecting
Rust's names because `send` is reserved for F3's actors.
`stdlib-shape-and-packages.md` §3.3 adopts them and records that they are
auto-derived structurally.

**The mapping is a bijection: `Share` ↔ `Send`, `ShareBorrowed` ↔ `Sync`.** Two
languages arrived independently at the same two auto-derived markers with the same
meanings. That is more agreement than the four ownership cases, and it sets up
the most valuable mechanism in this note.

The problem first. An `ffi.OpaqueHandle` is one word. Science cannot see what is
behind it, so it cannot know whether the `Box<Regex>` it points at is `Send`, and
the hazard is concrete: a Rust type that is `!Sync` because it holds a `Cell` or a
non-atomic cache, presented to Science as an ordinary handle, will be handed out
as many shared borrows as anyone asks for, because Science's rule 4 permits that
and Science has no interior mutability to be suspicious of. The result is a data
race that neither compiler saw.

Now the trick. Science cannot check the claim — but **rustc can**, and Science
can make it do so:

```rust
// generated into foreign/rust/src/science_abi.rs
const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    let _ = assert_send::<regex::Regex>;
    let _ = assert_sync::<regex::Regex>;
};
```

`sciencec` emits that from a claim written in the sidecar manifest. If the claim
is false, the sidecar crate does not compile, and the error arrives before a link
step ever runs.

> **Decision 5. A handle type over a foreign Rust value implements `Share` or
> `ShareBorrowed` only if the sidecar manifest claims it, and every such claim is
> discharged by a `sciencec`-generated static assertion that rustc checks.
> `SC0470` is the diagnostic for using such a handle across a thread boundary
> with no claim.**
>
> **Rejected: assuming handles are neither.** Safe, and it makes every wrapped
> crate unusable from a `scope.start(…)` body, which for `regex` — whose `Regex`
> is `Send + Sync` and is meant to be shared across a thread pool — is the
> common case rather than an edge one.
>
> **Rejected: assuming handles are both.** This is what a C binding is forced to
> do, and it is unsound for exactly the `!Sync` case above.
>
> **Rejected: trusting the manifest without the assertion.** It costs four
> generated lines to turn an unverifiable claim into a compile error. Declining
> that is declining the only thing Rust offers over C.
>
> **Cost.** The manifest gains a per-handle field; `sciencec` gains a code path
> that emits Rust source. Both are small and both are reused by §4.5.

**This is the argument for Rust as a binding target over C, and it is not an
ergonomic argument.** It is that a second compiler is standing on the other side
of the boundary, and unlike a C compiler it is capable of discharging obligations
Science states. §4.5 generalizes the trick from thread markers to layout, and the
generalization closes a hole `ffi-c-boundary.md` §5.3 calls *"the largest
practical risk in the whole design"*.

### 3.6 Panics

`ffi-c-boundary.md` §4.5 says panics do not cross, and observes that F0 made this
free because core spec §8's `panic` aborts. Rust's default is to unwind.

An unwind out of a Rust `extern "C"` function into a Science frame is undefined
behaviour: Science emits no unwind tables and has no landing pads. Since Rust
1.81 rustc inserts an abort shim at `extern "C"` boundaries, so the *default* is
already not-catastrophic — but relying on an implicit shim for a safety property
is the wrong posture, and there is a better setting available.

> **Decision 6. The sidecar crate is built with `panic = "abort"`, and
> `sciencec` refuses to build one whose release profile says otherwise
> (`SC0464`).**
>
> **Rejected: `panic = "unwind"` with a `catch_unwind` in every shim.** It works,
> it is what PyO3 does, and it is refused for the first version because it means
> every shim gains a wrapper, the unwinder is linked into every Science binary
> that touches Rust, and the resulting failure mode — a Rust panic converted to a
> Science `Error?` — is *different from what the same failure would do in Science*,
> where core spec §8 says it aborts. Two failure models in one binary is worse
> than one.
>
> **Rejected: leaving the profile to the user.** The implicit abort shim makes
> the unsafe case merely wasteful rather than fatal, so nothing would visibly
> break, which is precisely why it must be checked rather than left.
>
> **The real reason, stated positively.** Forcing `panic = "abort"` is not a
> workaround for an incompatibility. It is the setting under which **the two
> languages' failure models are the same model**: an unrecoverable error prints
> and stops the process, on both sides of the boundary, with no path that unwinds
> through frames the other language generated. Under any other setting they
> differ.
>
> **Cost, and it is a real one.** `catch_unwind` stops catching. Crates that rely
> on it degrade: `rayon` propagates a worker panic to the joining thread via
> `catch_unwind`, and under `panic = "abort"` that becomes a process abort
> instead. `std::thread::JoinHandle::join` stops returning `Err`. And a crate's
> own test suite runs under unwinding while the linked build does not, so tested
> behaviour and shipped behaviour differ on the panic path. That last point is
> the one that will surprise somebody, and it should be in the sidecar
> documentation rather than discovered.

**Forward compatibility.** `python-interop.md` §11 asks that F0 keep `panic`
*implementable* as an unwind. If that ever happens, this decision flips: the
sidecar profile becomes `panic = "unwind"` and every shim grows a
`catch_unwind` converting to a status code, which is the mirror of the obligation
`ffi-c-boundary.md` §4.5 already records for Science's trampolines. Recorded here
so it is not rediscovered.

### 3.7 Where the models genuinely disagree

Six places, and each one is a constraint on what a shim may do.

1. **Lifetime syntax.** Science has none by decision (core spec §6.1). Any Rust
   API whose *contract* lives in a lifetime — `RefCell::borrow`'s `Ref<'a>`, an
   arena's `&'arena T`, a `Cursor<'a>` — has no Science spelling beyond Case D's
   single-parameter `from`. Constraint: flatten or wrap.

2. **Interior mutability.** Rust has `Cell`, `RefCell`, `Mutex`, `UnsafeCell`;
   Science's rule 4 admits none. This is asymmetric in a way that matters: a Rust
   type that is `Sync` *because* its interior mutation is behind a lock is
   perfectly safe to share, and Science's rule 4 would have permitted the sharing
   anyway — so the permissive direction is sound. The hazard is the other
   direction (§3.5) and the static assertion is the answer.

3. **Generics and traits.** Neither side's generics reach the other. Every generic
   parameter in a crate's API becomes a **hand-chosen instantiation in the shim**,
   and this is the single largest recurring cost of consuming a crate.
   `serde`'s `T: Serialize` is the extreme case: a shim would need one
   instantiation per Science type, which is a *derive* problem, and the README's
   standing ask for a derive mechanism gains a third customer. The affordable
   version for `serde` is to use it only for the format (to and from a dynamic
   value tree, or to and from bytes) and never for the derive.

4. **Iterators.** `Iterator` is a trait with a generic `next`; it does not cross.
   An iterator-shaped API must either be boxed into a handle with a `next` shim —
   which costs an indirect call per item — or, better, re-expressed as a
   resumable stateless call as §2.4's `find_from` is. The rule that follows:
   **cross the boundary once per batch, never once per element.** A per-element
   foreign call is the same mistake `ffi-c-boundary.md` §5.2 prices for
   `when available`, an order of magnitude worse.

5. **Boxing a lifetime-parameterized value, which is the genuinely unsound
   thing.** `regex::Matches<'r, 'h>` borrows both the regex and the haystack.
   Putting it behind an `ffi.OpaqueHandle` requires erasing both lifetimes —
   `Box<Matches<'static, 'static>>` via `transmute` — and from that moment rustc's
   guarantees about that value are false, while `sciencec` never had any. **This
   is the one operation that makes both compilers wrong at once, and it is the
   obvious thing to reach for.**

   > **Decision 7. A shim may not box a value carrying a lifetime parameter.
   > Where a Rust API returns one, the shim exposes a resumable stateless call and
   > Science owns the iteration state.**
   >
   > **Rejected: boxing with erased lifetimes plus a discipline that the owner is
   > kept alive.** It is what every naive Rust FFI wrapper does; it is
   > `transmute`-based; and the discipline is exactly the kind of unwritten
   > obligation `ffi-c-boundary.md` §2.7 exists to enumerate rather than accept
   > silently.
   >
   > **Rejected: a self-referential handle that owns both the regex and a copy of
   > the haystack.** It works and it copies the input, which for a regex over a
   > 40 GB log file is not a cost, it is a failure.
   >
   > **Cost.** The Rust side re-enters per match instead of resuming a saved
   > automaton state, which for `regex` means re-checking a prefilter offset. It
   > is measurable and it is small; the alternative is unsound.

6. **Closures.** `ffi-c-boundary.md` §4.3's `ffi.Callback` trampoline works
   unchanged: the Rust side receives `extern "C" fn(*mut c_void, …)` plus a
   `*mut c_void` and wraps it in `move |x| unsafe { f(data, x) }`. Two
   Rust-specific additions: that closure is not `Send`, so handing it to anything
   thread-pooled requires the §3.5 claim; and §4.4's reentrancy hole and
   `SC0410` apply with no change.

---

## 4. The mechanism

`syntax-revision-2.md` §8.5 lands on three steps — keep declarations as the
interface, add a `foreign/` sidecar of real files, and make any inline block pure
sugar over the sidecar. **That ordering is adopted in full and is the starting
position.** This section makes it a design and adds one thing §8.5 did not have:
that the sidecar is checkable in a way an inline block is not.

### 4.1 The sidecar

```
myproject/
  main.science
  foreign/
    manifest.toml               # Science's side: names, handles, Share claims
    rust/
      Cargo.toml                # the user's, validated not generated
      Cargo.lock                # checked in, --locked
      src/
        lib.rs
        regex_shim.rs
        science_abi.rs          # generated by sciencec, checked in
```

> **Decision 8. One Rust crate per Science program, of `crate-type =
> ["staticlib"]`, whose `Cargo.toml` is written by the user and validated by
> `sciencec`, which mutates nothing.**
>
> **Rejected: `sciencec` generates the `Cargo.toml`.** It would let the compiler
> control `crate-type`, the profile and the dependency list directly. Refused for
> §8.4's own argument: a generated manifest is not a crate the Rust ecosystem can
> see. With a user-written one, `cargo test`, `cargo clippy`, `rustfmt` and
> rust-analyzer all work on the sidecar with no Science toolchain in the loop,
> which is the entire point of choosing files over inline blocks. It also means
> cargo features, `[patch]`, workspaces and `build.rs` remain available, and
> `sciencec` does not have to model any of them.
>
> **Rejected: `sciencec` mutates the user's manifest to enforce its
> constraints**, or sets `CARGO_PROFILE_RELEASE_PANIC` behind their back. Then
> the build the user tests differs from the build that links, which is a
> reproducibility wart introduced to avoid printing an error message. Validate,
> do not mutate: `SC0464` names the constraint and prints the two lines to add.
>
> **Rejected: one crate per `foreign/*.rs` file.** §2.6's linker fact: two Rust
> staticlibs in one binary is a duplicate-symbol error on `std`'s internals.
>
> **Cost.** The user learns a small amount of cargo. Against that, they learn
> *cargo*, which is documented, rather than a Science-specific manifest format
> that is not.

`sciencec` validates exactly four things, and the list is closed:

1. `[lib] crate-type = ["staticlib"]`.
2. `[profile.release] panic = "abort"` (§3.6).
3. `#![deny(improper_ctypes, improper_ctypes_definitions)]` in the crate root.
4. `Cargo.lock` present (§4.2).

### 4.2 What `sciencec` does with it

The build gains one phase, expressed as salsa queries because §7.3 of the core
spec requires exact invalidation:

| Query | Input | Output |
|---|---|---|
| `foreign_crates(program)` | the set of `crate "…"` clauses reachable from the compiled program | sidecar directories |
| `foreign_abi_module(crate)` | the `extern` block's declarations and the mirror types' layouts | the text of `science_abi.rs` |
| `foreign_manifest_check(crate)` | `Cargo.toml` | the four checks, or `SC0464`/`SC0466` |
| `foreign_archive(crate)` | everything below | a path to `lib<name>.a` |

`foreign_archive` is the one impure, long-running query, and it must be keyed on
**all** of: the sidecar directory's content hash, `Cargo.lock`'s hash, the target
triple, `rustc -vV`'s output, and `foreign_abi_module`'s hash. Anything less and
incremental builds go stale in the way §7.3's execution-count tests exist to
catch.

Sidecar resolution follows **the same ordered search path as module resolution** —
`--module-path`, then `SCIENCE_PATH`, then the toolchain — deliberately rather
than by coincidence. `stdlib-shape-and-packages.md` §6.3 names that ordering as
one of the two preconditions of a package manager that cannot be retrofitted, and
`ffi-c-boundary.md` §5.1 already gives library search the same three-step shape.
Three mechanisms with one spelling is worth more than three optimal spellings.

The link step adds the archive plus Rust's own native dependencies. Those are not
hard-coded: `cargo rustc -- --print native-static-libs` prints the exact list for
the target (`-lgcc_s -lutil -lrt -lpthread -lm -ldl -lc` on a typical Linux;
`ws2_32.lib userenv.lib bcrypt.lib ntdll.lib` and friends on MSVC), and `sciencec`
uses what it prints.

> **Decision 9. `cargo build --locked` always. A missing `Cargo.lock` is an
> error (`SC0469`), not an invitation to resolve.**
>
> **Rejected: resolving on the fly.** A build that resolves is a build whose
> output depends on what crates.io contained that morning. Core spec §12 calls
> environment reproducibility *"the single most-cited reason scientists trust
> results across machines"*, and `ffi-c-boundary.md` §7.1 refuses to let
> `sciencec` read system headers for precisely this reason. Letting cargo resolve
> would reintroduce the same hole through a door with a nicer name on it.
>
> **Cost.** The user must run `cargo generate-lockfile` once, and must
> deliberately run `cargo update` to move. That friction is the feature.

**Air-gapped builds.** HPC compute nodes frequently have no network. `cargo
vendor` plus a `.cargo/config.toml` in the sidecar is the supported path, and
`sciencec --foreign-offline` passes `--offline`. This is unglamorous and it is
the difference between "works on a cluster" and "works on a laptop", so it ships
with the feature rather than after it.

### 4.3 Declaration form: the `extern` block, with one clause

> **Decision 10. A Rust sidecar is reached through `ffi-c-boundary.md` §1.1's
> `extern` block, with one new contextual clause, `crate "name"`, replacing the
> `library` clause. No new ownership case, no new `unsafe` power, no new callback
> mechanism, no new error convention, and no new `ffi` type.**
>
> ```science
> unsafe extern "C" crate "regex_shim":
>     function science_regex_free(handle: ffi.OpaqueHandle)
> ```
>
> **Rejected: a distinct `foreign "rust"` declaration form.** Three reasons.
> First, what actually crosses is `extern "C"` with `#[repr(C)]` — bit for bit
> identical to C — so a second form would imply a second set of rules where there
> is one set. Second, every mechanism in `ffi-c-boundary.md` §2, §3, §4 and §6
> applies unchanged, and duplicating them into a parallel grammar is how two
> specifications drift apart. Third, and most important, it would be tempting to
> spell the Rust boundary as *safe* on the grounds that rustc is watching. It is
> not safe: `sciencec` still cannot see the shim, the `borrowed` claim is still an
> assertion, and retention is still unstatable. `unsafe` stays where the act is.
>
> **Rejected: `library "regex_shim"` with no new clause**, i.e. treating the
> staticlib as an ordinary C library. It would work, and it would lose every
> Rust-specific behaviour: the cargo build step, the generated ABI module, the
> forced static link, and the ability to reject clauses that make no sense.
>
> **Cost.** One contextual keyword. `crate` is recognized only inside the
> `extern` block grammar and stays an ordinary identifier everywhere else, which
> is `ffi-c-boundary.md` §1.1's rule for `library`, `via`, `symbol`, `from`,
> `when` and `available`. No word is added to §13's global list.

What the `crate` clause changes relative to `library`:

| Behaviour | `library "x"` | `crate "x"` |
|---|---|---|
| Link kind | dynamic by default, `kind static` available | **static, always** |
| `via pkg-config` | supported | `SC0467` |
| `when available` | supported, dlopen table | `SC0467` — a linked archive is always available |
| Build step | none | `cargo build --locked` |
| Signature agreement | unchecked (§5.3 of that note) | **checked by rustc** (§4.5) |

Static is forced because there is no stable Rust `cdylib` ABI story and no
distribution mechanism for one; `ffi-c-boundary.md` §5.1's argument *for* dynamic
linking — that HPC module systems swap the shared object under a fixed name —
does not apply, because no cluster administrator ships a Science sidecar as a
module.

### 4.4 Inline `rust` blocks

§8.5 recommends inline blocks as pure sugar over the sidecar. Working out the
mechanics changes the recommendation's *timing*, not its shape.

**Extraction.** An item-level block whose body is carried to a generated file.
The first problem is the lexer: finding the end of a `rust { … }` block by
counting braces is wrong, because Rust has raw strings (`r#"…"#`), byte strings,
nested block comments and `'}'` char literals. A correct extractor needs a
token-level Rust scanner inside `science-lexer`, which §8.2 did not price. The
cheaper form uses Science's own block rule, which the lexer already implements:

```science
rust "regex_shim":
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn science_regex_free(handle: *mut c_void) {
        if handle.is_null() { return }
        drop(unsafe { Box::from_raw(handle.cast::<Regex>()) });
    }
```

Indentation delimits it; the lexer needs a mode that emits the body as one raw
token rather than a Rust lexer. That is affordable.

**Span mapping is the problem, and it is a hard one.** **Rust has no `#line`
directive.** C does, so an inline-C design could tell the C compiler where the
fragment really came from; Rust cannot, and rustc will therefore report errors
against `foreign/rust/src/generated/main_L47.rs:12:5` — a file the user never
wrote and cannot open meaningfully.

The affordable repair is to translate rather than re-render: rustc's
`--message-format=json` carries structured spans, so `sciencec` rewrites
`file_name`, `line_start`, `line_end` and the byte offsets to point into the
`.science` file, keeps rustc's message text verbatim, and renders the result
through Science's own diagnostic renderer against Science's source. Columns
survive only if the extractor dedents by exactly the block indent and records the
offset. All of that is mechanical; none of it is free.

> **Decision 11. Inline `rust` blocks are specified and reserved, and not
> shipped in the first version. The sidecar is the whole mechanism. `SC0146`
> rejects a `rust` block with a message naming `sciencec foreign init` as the
> fix, and `SC0145`, `SC0147`–`SC0149` are held for the syntax when it lands.**
>
> **Rejected: shipping inline blocks with the sidecar.** They deliver **zero**
> additional capability — §8.5's own framing is that anything one can express the
> other can — and they cost a lexer mode, a JSON diagnostic translator, a
> line/column offset table, and all four of §8.4's tooling losses (no rustfmt, no
> clippy, no rust-analyzer, and a Science diagnostic for an error Science did not
> produce). §8.5's argument that *"sugar over a mechanism that works is cheap and
> reversible"* is an argument for doing it **second**, and this note takes it that
> way.
>
> **Rejected: ruling them out permanently.** The ergonomic moment
> `ffi-c-boundary.md` §7.4 identifies is real — *"Julia users call a C library by
> typing one line, and Science users generate a binding first"* — and an inline
> block is the natural answer to it. The door stays open and the diagnostics are
> allocated so that adding it later is a pure addition.
>
> **The nearest affordable thing, which is what actually closes the gap:** the
> friction an inline block removes is not writing Rust, it is creating a
> directory, a six-line `Cargo.toml` and a `lib.rs`. That is a scaffolding
> command, not a syntax. **`sciencec foreign init [crate-name]`** creates the
> sidecar, writes a valid manifest, adds the dependency, and prints the `extern`
> block skeleton. One command against a lexer mode and a span translator.
>
> **Cost of deferring.** For the first release, the smallest possible Rust
> interop is a directory rather than four lines in a file, and somebody will say
> so in a review of the language.

### 4.5 The `#[repr(C)]` / `extern "C"` discipline, and what can be checked

The rules the Rust side must follow:

1. Every function Science calls is `#[unsafe(no_mangle)] pub unsafe extern "C"
   fn`, or carries `#[export_name]`.
2. Every aggregate whose fields Science reads is `#[repr(C)]`.
3. No `Vec`, `String`, `Box<dyn …>`, `&str`, `Result`, `Cow`, or any
   `repr(Rust)` type in a crossing signature. `Option<&T>`, `Option<Box<T>>`,
   `Option<NonNull<T>>` and `Option<fn>` *are* permitted: Rust guarantees the
   null-pointer optimization for them, which is exactly
   `ffi-c-boundary.md` §1.4's niche rule, arrived at independently.
4. `panic = "abort"` (§3.6).
5. `bool` crosses as one byte holding 0 or 1 (guaranteed). **`char` never
   crosses** — it is a four-byte Unicode scalar value with invalid bit patterns,
   so a `u32` crosses and the shim validates.
6. Integer types in crossing signatures are `core::ffi::c_int`, `c_long`,
   `c_size_t` and so on, **never** `i32`/`i64`. This is
   `ffi-c-boundary.md` §1.6's `CLong` hazard in Rust clothing: `c_long` is 32
   bits on Windows and 64 on Unix, and `i64` is neither. `usize` ↔ `CSizeT` and
   `isize` ↔ `CPtrDiff` are exact on every supported target.

Now the part that has no C analogue. `sciencec` knows the layout it computed for
every `ffi.CLayout` type, and it knows every `extern` declaration's signature. It
can emit both as Rust source that rustc must accept:

```rust
// foreign/rust/src/science_abi.rs — generated by sciencec 0.4.1
// source: src/regex.science:12  hash: 9f3a1c7e
use core::ffi::{c_int, c_void};

pub type ScienceRegexFreeFn = unsafe extern "C" fn(*mut c_void);
const _: ScienceRegexFreeFn = crate::regex_shim::science_regex_free;

// mirror of Science type `Match`, declared at src/regex.science:31
const _: () = {
    assert!(core::mem::size_of::<crate::regex_shim::Match>() == 16);
    assert!(core::mem::align_of::<crate::regex_shim::Match>() == 8);
    assert!(core::mem::offset_of!(crate::regex_shim::Match, start) == 0);
    assert!(core::mem::offset_of!(crate::regex_shim::Match, end) == 8);
};
```

> **Decision 12. `sciencec` generates a signature-and-layout agreement module
> into the sidecar crate, from the Science declarations, and rustc discharges it.
> The Science declaration is the source of truth; the Rust definition is checked
> against it.**
>
> **Rejected: generating a C header from the Rust side (the `cbindgen`
> direction).** It checks nothing, because nothing reads the header.
>
> **Rejected: trusting the two sides to agree, as C requires.**
> `ffi-c-boundary.md` §5.3 calls the resulting hazard *"the largest practical
> risk in the whole design"* and says of a mismatched signature that it *"links
> cleanly and corrupts the stack at run time"*. Against Rust that hazard is
> avoidable for four generated lines per item, and declining to avoid it would be
> indefensible.
>
> **Cost.** `sciencec` gains a backend that serializes its `ffi.CLayout` layout
> computation into Rust source — a few hundred lines, reused by §3.5's `Share`
> assertions. The generated file is **checked in**, following
> `ffi-c-boundary.md` §7.2's rule for generated bindings, so that `cargo test`
> and rust-analyzer work with no Science toolchain present; a content hash in its
> header makes staleness `SC0465`, with `sciencec foreign sync` as the fix.

**What this closes, and what it does not.** Three tiers, honestly separated:

- **Closed by rustc, for free:** arity, every parameter and return type,
  `repr(Rust)` types in crossing signatures (via the mandatory
  `improper_ctypes_definitions` lint), fat pointers where thin ones were
  promised, `Send`/`Sync` claims, and — the headline — **aggregate size,
  alignment and every field offset.** For the Rust half of the boundary,
  `ffi-c-boundary.md` §5.3's largest practical risk is not mitigated, it is
  *closed*.
- **Closed by validation:** the four manifest constraints.
- **Not closed by anything:** retention, bounds, internal aliasing, reentrancy
  through a registered callback, and `longjmp` — `ffi-c-boundary.md` §2.7 items
  1, 2, 3, 5 and 6, all unchanged. And one that is specific to this mechanism and
  must be said: **the assertions check *agreement*, not *correctness*.** If
  `sciencec`'s layout computation for `ffi.CLayout` is wrong in the same way as
  the assertion it emits, the assertion passes and the program corrupts. The
  mechanism protects against the human writing the shim; it does not protect
  against the compiler.

### 4.6 Diagnostics from rustc, without lying about who found them

Core spec §10.3 compares rendered diagnostics byte for byte, so passing rustc's
output through unattributed is not acceptable. Neither, it turns out, is
re-rendering all of it. Three classes:

**(i) Errors in the user's own `foreign/rust/src/*.rs`.** A type error in code
Science did not write, did not parse and does not understand.

> **Decision 13. `sciencec` emits one Science diagnostic (`SC0462`) naming the
> crate, the `extern` block's span and the exact `cargo` command that reproduces
> the failure, then prints rustc's rendered output verbatim below a separator,
> explicitly attributed to rustc.**
>
> **Rejected: re-rendering rustc's diagnostics in Science's format.** It looks
> like the disciplined choice and it is the dishonest one: it would claim Science
> understood an error it did not, in a file whose language Science does not
> parse, with multi-span explanations and suggestions Science cannot verify.
> Attribution is more informative than imitation.
>
> **Rejected: passing cargo's output through with no Science diagnostic.** Then
> the compilation fails with no code, no span, and nothing connecting the failure
> to the `extern` block that caused the crate to be built at all.
>
> **This departs from `ffi-c-boundary.md` §5.3, which says linker errors must be
> rendered as `SC0461` "not pass `ld`'s output through", and the departure needs
> the rule that reconciles them:**
>
> > **Re-render when Science knows the cause. Attribute when it does not.**
>
> A linker error is one fact — symbol X is undefined — with a Science-side cause
> that Science can name, so re-rendering *adds* information. A rustc type error
> is an explanation of code Science did not write, so re-rendering *subtracts*
> it. `SC0461` stays re-rendered, `SC0462` attributes, and the rule covers both.
> This is offered as a generalization of §5.3 rather than a contradiction of it,
> and §8 asks that note to adopt the wording.
>
> **Cost for §10.3's byte-exact UI tests.** A test for `SC0462` asserts on
> Science's portion and on the presence of the attributed separator, not on
> rustc's bytes, which move between rustc releases. That is a weaker test and it
> is the only honest one available.

**(ii) Errors in the generated `science_abi.rs`.** Science *does* understand
these, because it generated the file: a failed `const _` coercion means the
definition's signature differs from the declaration's, and a failed `offset_of!`
assertion means a field moved. **Re-rendered as `SC0463`**, against the Science
declaration's span, saying what the declaration promised and what the definition
provides. The mapping works because `sciencec` emits a span table alongside the
generated file, and every span in it traces to one declaration.

**(iii) Errors from cargo itself** — a missing lockfile, a resolution that
`--locked` forbids, a violated manifest constraint, cargo absent. These are
Science's own preconditions and are Science's own diagnostics: `SC0464`,
`SC0466`, `SC0468`, `SC0469`.

---

## 5. Toolchain compatibility

### 5.1 Target triples — take this now, it is a one-way door

Both compilers go through LLVM and both name targets with a triple. Rust's
spellings are the widely known ones (`x86_64-unknown-linux-gnu`,
`aarch64-apple-darwin`, `x86_64-pc-windows-msvc`).

> **Decision 14. `sciencec --target` accepts and means exactly what `rustc
> --target` means, using Rust's spelling as canonical.**
>
> **Rejected: Science's own target naming.** It costs nothing today and it means
> that the day cross-compilation arrives, `cargo build --target $T` and
> `sciencec --target $T` take different strings for the same machine, and every
> build script grows a translation table.
>
> **Cost.** Essentially none today. Cross-compilation is out of scope per core
> spec §12, so this is a decision about *spelling*, not capability — which is
> exactly why it should be made now, while it is free.

### 5.2 rustup, and the minimum rustc

`sciencec` does not manage a Rust toolchain. It finds `cargo` on `PATH`, records
`rustc -vV` in the build metadata (so the recorded provenance of a result
includes it), and fails with `SC0468` if cargo is absent or too old. A
`rust-toolchain.toml` in the sidecar is the user's business and rustup honours it
with no involvement from Science.

A minimum version must be declared because the mechanism uses it:
`core::mem::offset_of!` (1.77), `#[unsafe(no_mangle)]` and the 2024 edition
(1.82), and the `extern "C"` abort-on-unwind shim (1.81). **Minimum supported
rustc: 1.82.** It is named in `SC0468`'s message.

### 5.3 LTO — refused, and for a different reason than `ffi-c-boundary.md` §5.5

That note refuses cross-language LTO because vendor libraries ship as binaries.
Rust is different: the sidecar is compiled from source, to LLVM bitcode, by an
LLVM that might match Science's. `-Clinker-plugin-lto` exists and would work.

> **Decision 15. Cross-language LTO between Science and a sidecar crate is not
> supported in the first version.**
>
> **Rejected: supporting it.** It requires the sidecar's rustc and `sciencec` to
> embed the *same* LLVM major version, which couples Science's release cadence to
> Rust's — a six-week cadence Science does not control, over a component
> (`sciencec`'s LLVM) that it cannot upgrade casually.
>
> **Cost.** Foreign calls are not inlined across the boundary. §3.7's rule makes
> that small: crossing happens once per batch, and an un-inlined call around a
> batch of work is noise. It would not be noise for a per-element crossing, which
> the same rule forbids for other reasons.
>
> This reaches `ffi-c-boundary.md` §5.5's conclusion by a different argument,
> and both arguments should be kept, because the C one does not apply here and
> would look like an oversight if it were the only one on record.

### 5.4 Cargo features and workspaces

The sidecar is an ordinary crate; features, `[patch]` and workspace membership
are the user's. Science models none of them, and does not need to: every one of
them is expressed in files inside the sidecar directory, whose content hash is
already part of `foreign_archive`'s key (§4.2). A feature flip invalidates the
build exactly as a source edit does.

---

## 6. What this note refuses, and what it offers instead

Collected in one place, because a note that promises Rust compatibility and
cannot deliver it is worse than one that delivers a narrow version honestly.

| Refused | Why | Nearest affordable thing | Price of the affordable thing |
|---|---|---|---|
| **`repr(Rust)` ABI compatibility** | Nobody has it, including Rust across its own versions | `extern "C"` + `#[repr(C)]`, which exists | Everything crosses as C |
| **Compiling Rust source** | Procedural macros make it a Rust implementation | Run rustc (§4.2) | A second compiler in the build |
| **Automatic binding generation over a crate's public API** | **Impossible, not expensive.** Zero of a typical crate is `extern "C"`-callable, and the missing contract is *present* in Rust's types and *not representable* in Science's | A hand-written shim crate per library, plus `sciencec foreign init`, plus §4.5's generated assertions | 150–400 lines of Rust per library, maintained against the crate's semver |
| **A Science user with no Rust writing `use crate regex`** | Requires a shim somebody wrote | A **curated set** of maintained shim crates shipped with the toolchain — this note proposes exactly four to start: `regex`, a compression pair (`flate2` + `zstd`), `serde_json`, and one blocking HTTP client | A standing maintenance obligation, the same shape as `scientific-libraries.md` §15's |
| **Rust as a build dependency of every program** | First-experience cost on a cluster | Decision 2's split: toolchain-level Rust is compiled into the tarball, user-level Rust needs cargo | A bigger tarball, and two versions of a crate on one machine |
| **Zero-copy transfer of a Science `Array` into a Rust `Vec`** | Rust's dealloc needs the exact `Layout`; `#[global_allocator]` may be replaced | `ffi.Span` in (free), copy out (§3.4) | One pass over the data |
| **Panics crossing in either direction** | `ffi-c-boundary.md` §4.5 | `panic = "abort"`, under which the two failure models are *the same model* | `catch_unwind` stops working; rayon's panic propagation becomes an abort |
| **Cross-language LTO** | Couples Science's LLVM to Rust's release cadence | Cross once per batch | Un-inlined foreign calls |
| **Boxing a lifetime-parameterized Rust value** | Requires erasing a lifetime, which makes both compilers wrong at once | A resumable stateless call; Science owns the iteration state (§3.7) | Slight re-entry cost per item |
| **`async` crates** | See below — this is the consequential one | A blocking crate: `ureq` not `reqwest`, `rusqlite` not `sqlx` | A materially smaller slice of the modern ecosystem |

**The async refusal deserves more than a table row,** because it removes one of
the crates the strategic claim named.

`stdlib-shape-and-packages.md` Decision 3 gives Science threads and no async, for
reasons specific to this language (§3.3's region interaction, §3.4's foreign-call
tax). A crate whose API is `async fn` — `reqwest`, `tokio-postgres`, `sqlx`, and
most of the modern networking ecosystem — is reachable only by the shim
constructing a `tokio` runtime and calling `block_on`. That works, and it drags
an executor plus **a second persistent thread pool** into the binary, which §3.6
of that note forbids outright: *"There is no second thread pool, and none is
exposed. Exposing a second one is how a program ends up with 256 threads on a
64-core node, which on a shared cluster is an administrative incident rather than
a performance bug."*

> **Decision 16. Crates whose public API is `async` are not wrapped in the first
> version. The curated set contains blocking crates only.**
>
> **Rejected: wrapping them with an internal runtime.** It works technically and
> violates a policy decision a sibling note made for cluster-specific reasons that
> this note has no standing to overturn.
>
> **Cost, named because it is the brief's own example:** `reqwest` is out.
> `ureq` is the blocking replacement and it is a smaller, less capable library.
> The same refusal touches most of the database ecosystem.

**A live tension this note cannot settle, and files instead.** `polars` and
anything else built on `rayon` carries a *persistent* thread pool, which §3.6
forbids — while `ffi-c-boundary.md` §2.7 item 10 records that OpenBLAS and MKL
spawn their own threads inside a call and calls it harmless, on the grounds that
*"the threads live and die inside the call"*. Rayon's pool does not die inside the
call. The rule's boundary is genuinely unclear and §8 asks for it.

---

## 7. Diagnostics allocated

This note claims **`SC0462`–`SC0470`** (codegen and linking) and
**`SC0145`–`SC0149`** (syntax, held for the deferred inline form).

| Code | Meaning |
|---|---|
| `SC0462` | The sidecar crate failed to build. Names the crate, the `extern` block's span and the reproducing `cargo` command; rustc's own output follows, attributed (§4.6) |
| `SC0463` | The Rust definition does not match the Science declaration — signature, size, alignment or field offset — rendered against the Science declaration's span (§4.5) |
| `SC0464` | The sidecar's `Cargo.toml` violates a required constraint; the message names which of the four and prints the lines to add (§4.1) |
| `SC0465` | The generated `science_abi.rs` is absent or stale; names `sciencec foreign sync` |
| `SC0466` | The sidecar crate root does not deny `improper_ctypes` and `improper_ctypes_definitions` |
| `SC0467` | A clause inapplicable to a `crate` block — `via pkg-config`, `when available`, `kind` (§4.3) |
| `SC0468` | `cargo` was not found, or the toolchain is older than the minimum (1.82) |
| `SC0469` | `Cargo.lock` is missing, or `--locked` resolution would change it (§4.2) |
| `SC0470` | A handle over a foreign Rust value crosses a thread boundary with no `Share` claim in the sidecar manifest (§3.5) |
| `SC0145` | *(held)* A `rust` block outside item position |
| `SC0146` | A `rust` block; not implemented in the first version, names `sciencec foreign init` as the fix (§4.4) |
| `SC0147`–`SC0149` | *(held)* for the inline form's remaining syntax errors |

`SC0470` is arguably an ownership-and-regions code rather than a codegen one,
since it is about thread safety. It is placed here because the check it performs
is the emission of a Rust static assertion, which happens at the codegen and
build step. If the core team prefers it in `SC03xx`, this note does not resist.

**An allocation hazard, flagged not fixed** (two other notes are being edited as
this is written, and the README's §"Diagnostic code allocation" warns that exactly
this is how the four existing collisions happened):

- `ffi-c-boundary.md` §8 reserves `SC0460`–`SC0479` as a **sub-range** for
  linking, while the README's allocation table records that note as claiming
  `SC0410`–`SC0461` by *named* code. `SC0462`–`SC0470` sits inside the declared
  sub-range and outside every named code. This note takes it on the README's
  authority and §8 asks that `ffi-c-boundary.md` §8's table be narrowed to
  `SC0460`–`SC0461` plus `SC0471`–`SC0479`.
- `python-interop.md` §9 claims `SC0450`–`SC0479` in its own text while the README
  records only `SC0450`–`SC0457`, and that note itself says the range *"needs
  reconciling with the sibling design notes"*. That is a live three-way overlap
  between two existing notes; this note does not touch it and names it so the
  next editor of either does.

---

## 8. What this note asks of the others

1. **`ffi-c-boundary.md` §1.1 and §5.1** — add the `crate "name"` clause as a
   contextual keyword inside the `extern` grammar, with forced static linking and
   `SC0467` for the clauses it makes meaningless. This is the only grammar change
   the note requests.
2. **`ffi-c-boundary.md` §5.3** — adopt §4.6's generalization: *re-render when
   Science knows the cause, attribute when it does not.* `SC0461` stays
   re-rendered under it; `SC0462` attributes. Without the wording, this note's
   §4.6 reads as a contradiction of that section rather than an extension.
3. **`ffi-c-boundary.md` §7** — `sciencec bindgen` gains **no** Rust backend, and
   §6 explains why one is impossible rather than unbudgeted. What it gains
   instead is a sibling command, `sciencec foreign init` / `foreign sync`, in the
   same territory.
4. **`ffi-c-boundary.md` §8** — narrow the linking sub-range to `SC0460`–`SC0461`
   plus `SC0471`–`SC0479`.
5. **`stdlib-standard.md` §9.3** — reconsider "written in Science, not linked"
   for `text.regex`. That decision was correct given no way to consume a Rust
   crate; §2.4 supplies one and prices it at ~350 lines against "a few thousand
   lines of real, subtle code". The reversal is **conditional** on the curated
   foreign-package mechanism existing and on someone owning the shim, and that
   note owns the module, so this note asks rather than decides. Both branches
   should be priced there: writing it is a one-time cost with no external
   dependency; wrapping it is a smaller one-time cost plus a permanent one.
6. **`stdlib-shape-and-packages.md` §3.6** — does *"there is no second thread
   pool"* bind a **linked foreign library**? OpenBLAS already violates the letter
   and `ffi-c-boundary.md` §2.7 item 10 calls that harmless because the threads
   die inside the call. A rayon-based crate's pool does not. The answer decides
   whether `polars` is ever wrappable and it is that note's to give.
7. **`stdlib-shape-and-packages.md` §6.3** — confirm that sidecar resolution uses
   the **same** ordered search path as module resolution rather than a parallel
   one. §4.2 assumes it, for that section's own reason.
8. **The core spec §12** — cross-compilation stays out of scope, but the
   `--target` **spelling** should be fixed to Rust's now (§5.1). It is free today
   and it is a one-way door.
9. **The core spec §8, with `python-interop.md` §11** — if `panic` ever becomes
   an unwind, the sidecar profile flips to `panic = "unwind"` and every shim
   grows a `catch_unwind`. This is the mirror of the obligation
   `ffi-c-boundary.md` §4.5 already records; recorded here so the Rust half is
   not missed.
10. **`reserved-words.md`** — `crate` is contextual inside the `extern` block
    grammar only and joins no global list. Confirm the dot rule covers a member
    named `crate`.
11. **The `ffi` module, §8 of the core spec** — **no new `ffi` items are
    requested.** Stated positively because it is the main evidence that this note
    is an extension of `ffi-c-boundary.md` rather than a rival to it. One
    temptation was declined: a `ffi.CBool`, since Rust's `bool` and C99's `_Bool`
    are both one byte holding 0 or 1. It is declined because the `ffi` set is
    closed by design and a one-line conversion in the shim costs less than a spec
    change.

---

## 9. Risks

**The strategic claim will be misread, and this is the risk I most want named.**
Cargo resolves *Rust* dependencies. The package-manager hole
`stdlib-shape-and-packages.md` §6 identifies — Science packages depending on
Science packages, two versions on one machine, recording what a result was
produced with — is **not closed by this note**, and §2.2 says so at length
precisely because a reader will assume it is. "Science inherits a package
manager" is half true and it is not the half that hurts.

**"Rust compatibility" as a public goal oversells what ships.** A reader of the
phrase expects to type a crate name and get a library. What they get is a
directory, a `Cargo.toml`, and 200 lines of Rust they must write and maintain.
The gap between the phrase and the artefact is a reputational risk, and the
mitigation is to restate the goal as **"Science links Rust"** — which is true,
specific, and still a bigger claim than any comparable language makes.

**The curated set is a commitment surface**, exactly the shape
`scientific-libraries.md` §15 warns about for the module catalogue. Four shim
crates is four semver treadmills and four things that break when a maintainer
yanks a version. Publishing the set without publishing who owns it invites the
reading that it will grow.

**Three regex engines can exist on one machine.** Decision 2 pins the standard
library's crates at toolchain build time while a user's sidecar pins its own via
`Cargo.lock`. A program that uses both `text.regex` and its own `regex` sidecar
links two copies of the engine, at two versions, with two behaviours on a
pathological pattern. Statically this is fine — the symbols are distinct — and
scientifically it is a result that cannot be attributed to one version.

**The generated assertions check agreement, not correctness.** §4.5 says it and
it bears repeating in a risk list: if `sciencec`'s `ffi.CLayout` layout algorithm
is wrong, the assertion it emits is wrong in the same way, both compilers agree,
and the program corrupts memory. The mechanism defends against the human, not
against the compiler. The only defence against the compiler is a layout test
suite against a known-good C compiler, which is `crates/science-rt`'s existing
discipline extended.

**`panic = "abort"` makes tested behaviour differ from shipped behaviour** on the
panic path, because a crate's own test suite runs under unwinding. Nothing
detects it, and the first time it matters will be a crate that relies on
`catch_unwind` in a way its documentation did not mention.

**rustc is a second compiler in the build, with its own version, its own build
time and its own failure modes.** A cold `cargo build` of a shim over a large
crate is minutes. Science's incremental story (core spec §7.3) stops at the
sidecar's boundary; inside it, the best available is cargo's own incrementality,
which `sciencec` neither controls nor models. Build-time complaints will arrive
before correctness complaints.

**The clearest technical win is also the most fragile.** §4.5's layout assertions
close `ffi-c-boundary.md` §5.3's largest risk — for the Rust half. Underneath
almost every wrapped crate there is still a C library (`zstd`, `libz`, a database
client) reached by a `*-sys` crate, and *that* boundary is unchecked exactly as
before. The guarantee is one layer deep and it is easy to describe as though it
were total.

---

## 10. Summary of decisions

| # | Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | Pursue semantic agreement, the C ABI, hand-written shims, and cargo for Rust deps only | Pinning a rustc and reverse-engineering `repr(Rust)`; a Rust front end | Rust has no stable ABI; procedural macros make source compatibility a Rust implementation | Everything crossing is spelled twice |
| 2 | Rust is a *conditional* build dependency; toolchain-level crates ship as objects | Unconditional cargo; no toolchain-level Rust | A scientist must be able to compile without rustup; the stdlib must still be able to wrap `regex` | Bigger tarball; two versions of a crate on one machine |
| 3 | Shims erase Rust lifetimes into indices, not into `from` clauses | Mapping lifetimes onto `from`; a multi-region return syntax | `from` names one parameter and fails for every other shape; the alternative is lifetime annotation under another name | Two `usize` out-parameters per borrowed return |
| 4 | Science→Rust ownership transfer is copy-only | Making `science_alloc` Rust's `#[global_allocator]` | Rust dealloc needs the exact `Layout`; a crate may replace the allocator | One pass over the data |
| 5 | `Share`/`ShareBorrowed` on a foreign handle is claimed in the manifest and asserted in Rust | Assuming neither; assuming both; trusting the claim | A `!Sync` Rust value handed to Science's rule 4 is a data race; four generated lines make it a compile error | A manifest field and a Rust-emitting backend |
| 6 | The sidecar is built `panic = "abort"`, checked | `panic = "unwind"` with `catch_unwind` shims; leaving it to the user | It is the setting under which both languages have the *same* failure model | `catch_unwind` stops working; rayon panics abort; tested ≠ shipped on that path |
| 7 | A shim may not box a lifetime-parameterized value | Boxing with erased lifetimes; a self-referential handle owning a copy | Erasure makes both compilers wrong at once; copying a 40 GB haystack is a failure, not a cost | Slight per-item re-entry |
| 8 | One user-written `staticlib` crate per program, validated not generated | Generated manifest; one crate per file | Real files mean real Rust tooling (§8.4); two staticlibs duplicate `std` | The user learns a little cargo |
| 9 | `cargo build --locked`, always | Resolving at build time | A build that resolves depends on what crates.io held that morning | `cargo generate-lockfile` once |
| 10 | Rust is reached through the existing `extern` block plus a `crate` clause | A distinct `foreign "rust"` form; plain `library` | What crosses is `extern "C"` bit for bit; a second form drifts; and the boundary is not *safe* just because rustc watches | One contextual keyword |
| 11 | Inline `rust` blocks specified, reserved, not shipped | Shipping them now; ruling them out | Zero added capability against a lexer mode, a span translator and four tooling losses; Rust has no `#line` | Smallest interop is a directory, not four lines — mitigated by `foreign init` |
| 12 | `sciencec` generates signature-and-layout assertions; rustc discharges them | `cbindgen`'s direction; trusting agreement as C must | It closes §5.3's largest practical risk for four generated lines per item | A Rust-emitting backend; a checked-in generated file |
| 13 | `SC0462` names the failure and attributes rustc's output verbatim | Re-rendering rustc's diagnostics; passing them through bare | Re-rendering claims Science understood an error it did not | A weaker UI test for that one code |
| 14 | `--target` means what `rustc --target` means | Science's own target naming | Free today, and a one-way door | None |
| 15 | No cross-language LTO in the first version | Supporting `-Clinker-plugin-lto` | It couples Science's LLVM to Rust's six-week cadence | Foreign calls are not inlined |
| 16 | No `async` crates in the curated set | Wrapping them with an internal `tokio` runtime | A second persistent thread pool, which `stdlib-shape-and-packages.md` §3.6 forbids | `reqwest` is out; most of the database ecosystem with it |
