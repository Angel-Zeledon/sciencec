# Science — Design: generating Rust bindings from rustdoc JSON

Date: 2026-09-16
Status: draft for review
Phase: F1 — the mechanism it generates into is `rust-interop.md`'s, which is F0
Depends on: `rust-interop.md` in its entirety. This note adds no ownership case,
no `unsafe` power, no error mechanism and no `ffi` type that note does not
already have. It adds one tool, one manifest section, two narrowings of existing
decisions, and a set of numbers.
Related: `ffi-c-boundary.md` §7 (binding generation) and especially §7.3, whose
rule this note applies; `stdlib-shape-and-packages.md` §3.6 (the thread-pool
decision, which §5 narrows); `data-io.md` §9 (the Arrow C Data Interface binding,
which §5 reuses); `c-binding-coverage.md` §3.8 (the same rule applied to C++);
`python-interop.md` §4.1 (DLPack, the same rule applied to tensors);
`native-dependencies.md` §2.1; `stdlib-standard.md` §9.3;
`syntax-revision-2.md`.

Syntax: revision 2 throughout (`function` … `->`, `of`, `borrowed`, `interface`,
`Type has:`, `T?`, `-> (T, Error?)`, `is` / `is not`, `> < >= <=`,
`for x in xs`, `loop:` / `break`, `print`).

---

## 0. Why this note exists, and what it is allowed to conclude

`rust-interop.md` §6 lists, among the things it refuses:

> **Automatic binding generation over a crate's public API** — *Impossible, not
> expensive.* Zero of a typical crate is `extern "C"`-callable, and the missing
> contract is *present* in Rust's types and *not representable* in Science's.

That conclusion was reached without considering `rustdoc --output-format json`.
The string `rustdoc` does not appear anywhere in that note. The omission matters,
because `cargo +nightly rustdoc -- -Z unstable-options --output-format json`
emits the crate's entire public API as structured data — every item, signature,
generic parameter, trait bound, **named lifetime**, associated type, variant and
field, with resolved cross-crate paths. It is the machine-readable interface
description that a C header is a poor imitation of, and `ffi-c-boundary.md` §7.3
already contains the rule that applies to it:

> Prefer any machine-readable interface description the vendor ships over the C
> header, because the header is the one artifact guaranteed to have thrown the
> contract away.

§7.3 wrote that about LAPACK's Fortran `INTENT(IN)` annotations. Rust's type
system is the same argument at an order of magnitude more strength: `INTENT` tells
you read-versus-write, and a Rust signature tells you read-versus-write *and*
ownership *and* nullability *and* fallibility *and* the exact provenance of every
borrowed return.

**This note re-tests the parent's conclusion. It is not here to overturn it for
its own sake, and it does not.** The distinction it lands on is the one the
question turns on:

> **A binding that requires no *Rust* to be written is achievable. A binding that
> requires no *declaration* to be written is not, and never will be. The parent
> note found the second and called it the first.**

And one finding, arrived at through `polars`, turns out to matter more than the
generator does:

> **For the crates that matter most, the right answer is not to generate a
> binding to the crate at all. It is to bind the interchange format the crate
> already speaks. The generator is what you use for the rest.**

§1 is the input. §2 is the coverage question answered with numbers from three real
crates, four rustdoc documents and roughly four thousand public functions. §3 is
the taxonomy of what does not cross. §4 is the generator. §5 is `polars`, which
forces four decisions the generator cannot make — an interchange rule, an
embedded-language rule, and two thread-budget rules — plus an honest verdict on
whether it is reachable at all. §6 is the ceiling.
§7 says what this does to `rust-interop.md` and `stdlib-standard.md` §9.3. §8
prices nightly. §9–§13 are diagnostics, asks, risks, decisions and open
questions.

---

## 1. The input

### 1.1 What rustdoc JSON is

One JSON document per crate. `index` maps an item id to an item; `paths` maps an
id to a fully-qualified path, a kind and an owning crate; `root` names the crate
module. Every public item of the crate is in `index` with `crate_id` 0. For a
function, `inner.function.sig` carries `inputs` as `(name, Type)` pairs and
`output` as a `Type`; `inner.function.generics` carries the type and lifetime
parameters with their bounds and the `where` predicates; `inner.function.header`
carries `abi`, `is_unsafe` and `is_const`. For an `impl`, `inner.impl` carries the
implementing type, the implemented trait (or null for an inherent impl), the
impl's own generics, and flags for synthetic and blanket impls so a consumer can
discard the auto-derived noise.

`Type` is a closed tagged union: `primitive`, `resolved_path`, `borrowed_ref`,
`raw_pointer`, `slice`, `array`, `tuple`, `generic`, `impl_trait`, `dyn_trait`,
`function_pointer`, `qualified_path`, `infer`. That closure is the property that
makes a generator possible at all — a lowering function over thirteen cases is a
finite piece of work, and every case that has no lowering is a case the generator
can *name*, which is the difference between a tool that is wrong and a tool that
is incomplete.

### 1.2 The one fact that decides the note

`borrowed_ref` carries a `lifetime` field, and `resolved_path` carries lifetime
arguments. So for

```rust
pub fn find_iter<'r, 'h>(&'r self, haystack: &'h str) -> Matches<'r, 'h>
```

rustdoc JSON states, mechanically and without inference, that the returned value
borrows *the receiver* and *the haystack*, and which is which. Measured (§3.3), it
does this for **every** borrowed return in the crates examined, with zero
ambiguous cases.

That is precisely the information `ffi-c-boundary.md` §7.1 says a C header has
thrown away —

> `double *a` does not say whether it is read or written, whether it points at one
> element or a million, whether the callee retains it, or who frees it. Every
> distinction in §2 is absent from the header.

— and every one of those four distinctions is present, by construction, in a Rust
signature. `&mut` is written-not-read. `&[T]` is the span with its length.
Ownership is `T` versus `&T`. Retention is the returned lifetime. The reason
`ffi-c-boundary.md` §7.1 forced generated C bindings to be *maximally
conservative* does not apply here, and carrying that conservatism across would be
imitation rather than reasoning.

### 1.3 Method, so the numbers below can be re-derived

Crates were fetched from crates.io and documented with
`cargo +nightly rustdoc -p <crate> -- -Z unstable-options --output-format json`
on `rustc 1.100.0-nightly (215a8af4b 2026-09-15)`, producing `format_version`
**61**. Every item with `crate_id == 0` and kind `function` was extracted,
together with its enclosing impl (discarding `is_synthetic` and `blanket_impl`
impls), and its parameter and return types were walked through the thirteen-case
union and classified. The classifier is a throwaway research script and is
deliberately **not** proposed as a deliverable; §4 specifies the real tool.

Crates and versions: `regex` 1.13.1 (the parent note's own worked example, kept
for comparability with its zero), `polars-core` and `polars-lazy` 0.51.0 (the
mandatory case — a large API, heavy generics, a central mutable data type, and an
internal `rayon` pool), `serde_json` 1.0.151 (kept as a third data point because
it is the derive-driven shape, which is a distinct failure mode from polars').

**The bucket boundaries are judgement calls and the contestable ones are named in
§2.5 rather than buried.** A reader who disagrees with one of them can move the
number, and the shape of the result does not depend on any of them.

---

## 2. The coverage question, answered with a number

### 2.1 The four buckets

| Bucket | Meaning |
|---|---|
| **A — directly mappable** | Callable across the boundary with no shim at all: already `extern "C"`, or a signature whose every type is FFI-safe and whose ABI could be re-tagged. |
| **B — mechanically shimmable** | A generator emits the Rust shim, the Science declaration and the safe Science wrapper with **no human input of any kind**. |
| **C — shimmable with a declared policy** | A generator emits all three once it is told **one** fact: how an error type surfaces, which fields a borrowed value projects to, which type arguments to instantiate, or which input a returned borrow is tied to. One policy entry may cover many items. |
| **D — unreachable** | No shim exists that a generator could write, because the contract is a trait the *caller* must implement, or an associated type with no runtime representation. |

**"Reachable" means B + C.** A is the parent note's question, and §2.2 confirms
the parent note's answer.

### 2.2 `regex` 1.13.1 — the comparability case

Three denominators, because which one you pick changes the headline and hiding
that would be dishonest.

| Surface | n | A | B | C | D | reachable |
|---|---|---|---|---|---|---|
| Inherent methods + free functions | 167 | **0** | 108 | 59 | 0 | **100%** |
| …plus impls of `regex`'s own traits (`Replacer`) | 201 | **0** | 108 | 93 | 0 | **100%** |
| …plus impls of foreign traits (`Debug`, `Clone`, `Iterator`, `Display`, `From`, `Index`, …) | 326 | **0** | 154 | 168 | 4 | **98.8%** |

Public type-level items: 36 structs, 1 enum, 2 traits, 1 macro. **`#[repr(C)]` on
zero of them.**

The 93 C-bucket items are covered by **15 distinct policy entries**:

| Policy entry | Items it covers |
|---|---|
| error surface for `regex::Error` | 8 |
| projection for `Captures` | 24 |
| projection for `Match` | 14 |
| projection for `Matches`, `CaptureMatches`, `CaptureNames`, `Split`, `SplitN`, `SetMatchesIter`, `SubCaptureMatches`, `ReplacerRef` (8 entries) | 16 |
| `Cow<str>` return convention | 24 |
| generic instantiation (`R: Replacer` ×6, `I: IntoIterator, S: AsRef<str>` ×4) | 16 |
| owned `String` argument convention | 10 |
| borrowed `&str` return provenance | 4 |

Counts exceed the item total because several items need two entries, and the
`regex` / `regex::bytes` split doubles most method names.

**The parent note's zero is confirmed, exactly.** Bucket A is 0, and it is 0 for a
structural reason that no tool changes: every function in the crate has
`abi: "Rust"`, so even `fn len(&self) -> usize` is not callable from C, and no
crate type is `#[repr(C)]`. `rust-interop.md` §2.3's "*Zero. Not 'a small
fraction'*" is correct as written and this note does not soften it.

**What is new is the next column.** 100% of the surface a user would actually
call is reachable by a generator holding 15 declared facts. `rust-interop.md`
§2.4 priced the same crate at **~215 lines of Rust written by a human**. Fifteen
TOML entries is not 215 lines of Rust.

### 2.3 `polars` 0.51.0 — the mandatory case

Measured over `polars-core` and `polars-lazy` together, which is where
`DataFrame`, `Series`, `Column`, `ChunkedArray` and `LazyFrame` live. The
`polars` facade crate itself is almost entirely re-exports and its own index is
nearly empty — a fact §8 returns to, because it is why a source-parsing generator
would see nothing at all here.

| Surface | n | A | B | C | D | reachable |
|---|---|---|---|---|---|---|
| Inherent methods + free functions | 1676 | 10 | 603 | 981 | 82 | **95.1%** |
| …plus impls of polars' own traits | 2486 | 11 | 849 | 1409 | 217 | **91.3%** |
| …plus impls of foreign traits (`Clone`, `From`, `FromIterator`, `Debug`, `Serialize`, …) | 3021 | 11 | 1045 | 1583 | 382 | **87.4%** |

Inherent methods by receiver: `Column` 182, `ChunkedArray` 170, `DataFrame` 161,
`Series` 134, `LazyFrame` 93, `DataType` 60, `LazyCsvReader` 32, `AnyValue` 31,
free functions 437.

**The A column is not zero here, and that is worth one paragraph of honesty.**
Eleven `polars-core` items — `verbose()`, `get_file_prefetch_size()`,
`set_global_random_seed(u64)`, `_set_check_length(bool)` and similar
configuration hooks — have signatures whose every type is FFI-safe. They are
still not callable from C, because their ABI tag is Rust's, so each needs a
one-line re-export shim. **Bucket A in the strict sense — callable with no shim
at all — is 0 across all four crates measured, exactly as `rust-interop.md` §2.3
says.** The eleven are recorded because they are the only place in 4,000 public
functions where the *types* were not the obstacle.

**The 1,409 C items are covered by 111 distinct policy entries**, and the shape of
that distribution matters more than the count:

| | |
|---|---|
| Entries covering ≥10 functions each | **15** |
| Entries covering exactly one function | **45** |
| Share of all policy demand met by the top 5 entries | **79.9%** |
| Functions covered by the single `PolarsError` entry | **633** |
| Functions covered by the `PlSmallStr` projection entry | **264** |

One error-surface entry for `PolarsResult` — polars' `Result<T, PolarsError>`
alias — discharges 633 functions. One projection entry for `PlSmallStr`, polars'
interned column-name string, discharges 264. **So "111 entries" overstates the
work by a wide margin: five entries get you four fifths of the way, and the
45 singletons are the long tail you write only if you want that exact function.**
This is the property that makes a policy file scale to a crate of this size at
all, and it is not a property a hand-written shim has — 1,409 hand-written shims
is not 111 of anything.

**The 82 unreachable inherent functions, and the 382 unreachable overall, have
one dominant cause and it is not generics.** It is **associated types** — 180 of
the 217 in the inherent-plus-own-trait surface, and 139 of the 165 among foreign
traits. These are polars' `ChunkedArray` iteration traits and the `Iterator` /
`IntoIterator` impls, whose `Item` is projected through a `qualified_path` the
generator cannot resolve to a concrete type without instantiating the impl. A
further 37 are `dyn Trait` in argument position, and 51 are serde's `Serializer` /
`Deserializer` impls, which are `serde_json`'s failure mode appearing again in a
crate that is not about serialisation.

**And now the number that matters, which is that none of the above is the
answer.** 95.1% is a real measurement and it is close to useless, because
`LazyFrame::filter(self, predicate: Expr) -> LazyFrame` counts as bucket B — a
handle in, a handle out — while being unusable by anyone who cannot construct an
`Expr`. §5.1 takes this apart. **The polars coverage number is in this note
because it was asked for and because leaving it out would look like evasion. It
should not be quoted.**

### 2.4 `serde_json` 1.0.151 — the derive-driven shape

Kept because it fails differently from polars, and the difference is the one that
decides what to build next.

| Surface | n | A | B | C | D | reachable |
|---|---|---|---|---|---|---|
| Inherent methods + free functions | 139 | **0** | 66 | 61 | 12 | **91.4%** |
| …plus impls of `serde_json`'s own traits | 148 | **0** | 66 | 70 | 12 | **91.9%** |
| …plus impls of foreign traits | 616 | **0** | 179 | 158 | 279 | **54.7%** |

Public type-level items: 25 structs, 6 enums, 6 traits, 1 type alias. **`#[repr(C)]`
on zero of them.** The 70 C items are covered by **13 policy entries**, of which
one — the `serde_json::Error` surface — covers 52.

**The 12 unreachable inherent functions are exactly the famous ones.**
`to_string`, `to_string_pretty`, `to_vec`, `to_vec_pretty`, `to_writer`,
`to_writer_pretty`, `from_str`, `from_slice`, `from_reader`, `to_value`,
`from_value`, and `StreamDeserializer::into_iter`. Every one is generic over
`T: Serialize` or `T: Deserialize`/`DeserializeOwned`. They are unreachable not
because of the ABI and not because of lifetimes, but because the bound is
satisfied by types *the downstream user has not written yet*. §3.1 separates this
from ordinary generics because the cure is different: it is Science's derive
mechanism, not the binding generator. `rust-interop.md` §3.7 item 3 reached the
same place — *"the README's standing ask for a derive mechanism gains a third
customer"* — and this note supplies the count that makes the ask concrete: 12
functions, 8.6% of the user-facing surface, and they are the 12 that appear in
every `serde_json` tutorial.

And 279 of the 616 are unreachable, of which 217 are the methods of
`impl Deserializer for &mut Deserializer<R>`. That impl exists so that
`serde_derive`'s generated code can drive the parser. Nobody calls
`deserialize_i8(visitor)` by hand. But it *is* public, and any coverage number
quoted without saying which denominator it used can be moved by a factor of two.

### 2.5 The contestable boundaries, named

Five classifications could reasonably be argued the other way:

1. **`String` and `Vec<u8>` in return position were counted B, not C.**
   `rust-interop.md` §2.4 already fixes the convention — the two-call
   length-then-fill protocol — so the generator applies a *standing* policy rather
   than a per-item one. If you think a standing convention is still a policy, move
   ~34 `regex` items and ~6 `serde_json` items from B to C. It does not change any
   reachable total.
2. **`&mut Self`-returning builder methods were counted B.** `RegexBuilder` and
   `RegexSetBuilder` contribute 56 of `regex`'s 108 B items; polars' `Expr` and
   `LazyFrame` builders contribute a much larger share of its B count. Lowering
   them is a genuinely trivial *return the same pointer*, but a reader who thinks
   builder chaining is not really API can subtract them.
3. **Foreign-trait impls were counted at all.** `Debug::fmt` is bindable (it is a
   to-string call) and it is not something anyone asked for. That is why every
   table gives the row with and without.
4. **`char` was counted a primitive.** It is not FFI-safe — it is a four-byte
   scalar with invalid bit patterns — but `rust-interop.md` §4.5 rule 5 already
   fixes the lowering (a `u32` crosses, the shim validates), so it is mechanical.
5. **A handle-to-handle method was counted B even when the handle is enormous.**
   `LazyFrame::group_by(self, exprs) -> LazyGroupBy` is B by the classifier's
   rules: both sides are opaque pointers. That is *true at the ABI* and it is the
   single most misleading thing in the whole audit, because it counts as
   mechanically bound an operation whose meaning lives entirely in the `Expr`
   values the caller could not construct. §5.1 is about exactly this.

None of the first four moves the shape of the result. The fifth does, and it is
why §5 exists.

---

## 3. What does not cross, and why — the causes separated

The parent note's §3.7 lists six disagreements between the two models. This
section takes each one and asks a different question: *not* "can a human write a
shim for it" but "can a generator, holding rustdoc JSON, write a shim for it, and
if not, what is the smallest declaration that lets it".

### 3.1 Generics

A generic function has no symbol until it is instantiated. That is true and it is
not the obstacle it looks like, because **the generator is emitting Rust source**,
and emitting `pub unsafe extern "C" fn science_regex_replace_all_str(...)` whose
body calls `re.replace_all(hay, rep)` with `rep: &str` *is* the instantiation.
rustc monomorphizes it, the symbol exists, and the ABI is `extern "C"`.

So the real question is the one the brief asks: is instantiating for a declared
set of type arguments useful, or a combinatorial trap? Measured:

| | fns with 0 type params | 1 | 2 | 3+ | max | distinct bound-sets |
|---|---|---|---|---|---|---|
| `regex` (inherent + own-trait, n=201) | 191 | 6 | 4 | 0 | 2 | 2 |
| `serde_json` (inherent + own-trait, n=148) | 84 | 61 | 3 | 0 | 2 | ~6 |
| `polars-core` + `polars-lazy` (n=2486) | 2198 | 185 | 55 | 48 | **7** | **73** |

**For `regex` and `serde_json` it is not a combinatorial trap, because the
exponent never exceeds two. For polars it is a trap and the exponent reaches
seven, which is the single strongest argument in this note for not binding
polars' API at all (§5).** The bounds actually seen:

- `regex`: `R: Replacer` (6 functions), `I: IntoIterator<Item = S>, S: AsRef<str>`
  (4). Two declared instantiations cover all ten.
- `serde_json`: `W: Write` (42 functions — every `Formatter` method), `Q: Borrow +
  Ord + Hash + Eq` (6), `T: Serialize` (7), `T: Deserialize`/`DeserializeOwned`
  (5), `F: FnMut` (1), `R: Read` (1). `W: Write` needs exactly one instantiation —
  a Science writer reached through `ffi-c-boundary.md` §4.3's callback trampoline
  — and it covers 42 functions.
- `polars`: 73 distinct bound-sets across 288 generic functions, led by `Fn` (25),
  `IntoIterator` (24), `Iterator` (17), `AsRef` (17), `Into` (17),
  `Copy + Fn` (14), `PolarsDataType` (12), `Into + IntoIterator` (12),
  `FnMut` (11), and `ArrayFromIter + FnMut + PolarsDataType` (8). Two of those
  are benign — `PolarsDataType` is a **closed** set the crate enumerates itself,
  and `AsRef<str>` over column names needs one instantiation. The rest are not:
  `Into<Expr>` and `IntoIterator<Item: Into<Expr>>` are open in practice, and a
  function with seven type parameters over bounds like those has no instantiation
  a generator could choose and no small set a human would want to enumerate.

The trap is real and polars is standing in it: **a cross product over
independently declared value sets.** If the policy file said
`R ∈ {&str, String, NoExpand}` and `S ∈ {&str, String}` and the generator emitted
every pair, a seven-parameter API would produce hundreds of symbols for one
wanted call. So it must not.

> **Decision 1. Generic instantiation is by explicitly enumerated *tuples* of type
> arguments, one emitted symbol per tuple, with one exception: a bound whose
> implementing set is closed *within the crate and its dependencies* may be
> declared as a set and expanded, because the generator can enumerate it from the
> rustdoc documents and check that it did not miss one.**
>
> **Rejected: a cross product over per-parameter value sets in the general case.**
> It is the obvious convenience and it turns a two-parameter function with three
> candidates each into nine symbols, of which the user wanted one. Binary size and
> compile time are the visible cost; the invisible one is that eight of the nine
> are never called and never tested, and they are in the public surface.
>
> **Rejected: inferring instantiations from the crate's own examples or doctests.**
> rustdoc JSON carries the doc text, so this is *available*, and it is refused
> because it makes the generated surface a function of what a crate author happened
> to write in a doc comment — which changes on a patch release, silently, and
> removes symbols a Science program was calling.
>
> **Rejected: refusing generics entirely**, which is where a generator over C
> headers would have to stop. It would put 10 `regex` items, 64 `serde_json` items
> and most of polars in D for no reason other than timidity.
>
> **Cost.** One policy line per instantiation, and the generated symbol name must
> encode the arguments (`science_regex_replace_all__str`), which is ugly and is in
> the public C surface forever. For the closed-set exception, the cost is that the
> symbol count is a function of the upstream crate's type list, so a new element
> type upstream silently adds symbols — visible in the coverage report's diff,
> which is why Decision 12 makes that report mandatory.

**The one generic case that is not reachable at all** is the open-world bound:
`T: Serialize`. There is no finite set of instantiations, because the set is
"every type any downstream Science user will ever define", and the mechanism that
serves it in Rust is a procedural macro that runs at compile time over the user's
own type. §3.2 and Decision 2 handle it.

### 3.2 Traits and `impl Trait`

Three sub-cases with three different answers, and collapsing them is how the
parent note's flat refusal happened.

**(a) The crate implements a trait — bindable.** `Display`, `Debug`, `Clone`,
`PartialEq`, `Default`, `From`, `Hash`, `Index`, `Iterator`,
`ExactSizeIterator`, `FromStr`. These are named traits with known shapes, and a
generator can carry a fixed table mapping each to a shim template. This is where
46 of `regex`'s 154 all-in B items and 113 of `serde_json`'s 179 come from. There
is no judgement involved: `Clone for Regex` becomes
`science_regex_clone(handle) -> handle`, and `Display for Error` becomes the
two-call string protocol.

**(b) The caller must implement a trait — unreachable, except through a
trampoline.** `serde::Serialize` on a Science type, `regex::Replacer` by a Science
closure, `serde::de::Visitor`. A Rust trait implemented by a Science type requires
a Rust vtable whose entries call back into Science, and while
`ffi-c-boundary.md` §4.3's `ffi.Callback` can build one entry, a trait like
`Deserializer` has 30 of them with associated types between them.

**(c) `impl Trait` in return position — no runtime representation, but it has a
concrete type.** rustdoc JSON gives the bounds, not the hidden type, so the
generator cannot box it without knowing what it is. This is a policy case, not an
unreachable one: naming the concrete type in the policy file resolves it.

> **Decision 2. A generic parameter whose bound is an *open-world* trait — one
> whose impls are expected to come from crates that do not exist yet — is
> classified unreachable and is reported, not faked. The generator ships a fixed,
> closed list of such traits (`serde::Serialize`, `serde::Deserialize`,
> `serde::de::DeserializeOwned`, `serde::Serializer`, `serde::Deserializer`,
> `serde::de::Visitor`) and a policy switch to add to it.**
>
> **Rejected: generating one instantiation per Science type in the program.** It
> is technically possible — the generator knows the program's types — and it is
> refused because it makes the sidecar crate's contents a function of the Science
> program's type declarations, so adding a field to a Science `type` triggers a
> full rebuild of a Rust crate. It is also a derive mechanism wearing a false
> beard, and building a derive mechanism badly, inside a binding generator, is
> worse than not having one.
>
> **Rejected: binding them against a dynamic value tree only** — i.e. always
> instantiating `T = serde_json::Value` and making Science users assemble a
> `Value`. This is `rust-interop.md` §3.7 item 3's *"affordable version"*, and it
> is not rejected as a *technique*; it is rejected as a substitute for saying the
> entry points are unreachable. The generator does emit the `Value` path — it is
> bucket B — and it must still report that `to_string::<T>` was not bound.
>
> **Cost.** Named plainly in §6: the twelve functions every `serde_json` user
> reaches for first are not generated. The real fix is the README's standing ask
> for a derive mechanism, which now has a fourth customer and a measured price.

### 3.3 Lifetimes — and a narrowing of the parent's Decision 7

`rust-interop.md` Decision 7 says:

> A shim may not box a value carrying a lifetime parameter. Where a Rust API
> returns one, the shim exposes a resumable stateless call and Science owns the
> iteration state.

with the reason that erasing the lifetime — `Box<Matches<'static, 'static>>` via
`transmute` — makes *both* compilers wrong at once. **That is half right, and the
half that is wrong is worth a decision.**

First, the measurement. Across all four crates, 204 public functions return a
value that is borrowed or carries a lifetime parameter. Where does that lifetime
come from?

| | n | return lifetimes explicitly named among the inputs | resolvable by Rust's elision (one borrowed input) | resolvable only from the impl's own lifetime parameter (the `self` case) | ambiguous |
|---|---|---|---|---|---|
| `regex` | 90 | 37 | 51 | 2 | **0** |
| `serde_json` | 15 | 0 | 10 | 5 | **0** |
| `polars-core` + `polars-lazy` | 99 | 10 | 81 | 8 | **0** |
| **total** | **204** | **47** | **142** | **15** | **0** |

Return-lifetime arity: 78 of `regex`'s 90 carry one lifetime and 12 carry two;
every one of `serde_json`'s 15 and polars' 99 carries exactly one.

**Provenance is mechanically determined in 204 cases out of 204.** There is no
ambiguous case in any crate. The fifteen `self`-case entries are the ones
`ffi-c-boundary.md` §2.6 deliberately excluded when it defined `from` elision as
*"Rust's elision rule minus the `self` case"* — and the exclusion costs nothing
here, because in an `extern` block the receiver is an ordinary named parameter, so
the generator emits an explicit `from handle` where Rust's elision would have used
the self rule. **A generator can always emit an explicit `from`, so the elision
gap that a human writing declarations by hand would trip over does not exist for
generated ones.**

That 204-for-204 result is the single most encouraging measurement in this note,
and it is worth naming what it is evidence *of*: not that lifetimes are easy, but
that **the information a human would have had to supply by reading the docs is
already in the machine-readable form, exhaustively, for every crate tried.**

Now the substantive point. The parent note's claim is that boxing
`Matches<'r, 'h>` makes both compilers wrong. rustc is indeed wrong locally: after
the `transmute` its knowledge about that value is false. But `sciencec` need not
be wrong, because `ffi-c-boundary.md` §2.4 — *"the retaining case, which is Case A
and Case B at once"* — already specifies the construct that makes it right:

```science
type FftwPlan:
    raw: ffi.OpaqueHandle
    input: ffi.MutableSpan of F64      # region inferred; keeps the buffer alive
    output: ffi.MutableSpan of ffi.Complex64
```

*The handle type holds the borrow as a field.* Applied to `Matches<'r, 'h>`, whose
two lifetimes rustdoc JSON binds to the receiver and the haystack:

```science
type Matches:
    raw: ffi.OpaqueHandle
    pattern: borrowed Pattern
    text: borrowed String

Matches implements Drop:
    function drop(mutable self):
        unsafe: science_regex_matches_free(self.raw)
```

Science's region engine now refuses to let the `Pattern` or the `String` be moved,
freed or exclusively reborrowed while the `Matches` is alive — which is exactly
the obligation rustc gave up. The claim is not unverified; it is verified on the
other side of the boundary, which is the same trade `ffi-c-boundary.md` §2.6
already accepts for `from p`, and the same trade `rust-interop.md` §3.5 accepts in
the other direction when it makes rustc discharge a `Send` claim Science cannot
check.

What makes this newly *safe to automate* rather than merely *possible* is §1.2:
the generator does not guess which fields the handle needs. It reads them.

> **Decision 3. `rust-interop.md` Decision 7 is narrowed. A generated shim may box
> a value carrying lifetime parameters if and only if the generator can bind every
> one of those parameters to a specific input of the constructing function, and
> emits a Science handle type carrying one `borrowed` field per bound lifetime,
> per `ffi-c-boundary.md` §2.4. If any lifetime cannot be bound — it is `'static`
> from a source the generator cannot confirm, it is higher-ranked, or it appears
> only in the return — the box is refused and `SC0488` is emitted.**
>
> **Rejected: keeping the flat ban.** It costs `regex` its entire iterator surface
> and forces every borrowed return through the resumable-call rewrite, which
> `rust-interop.md` §3.7 item 5 itself prices as *"re-checking a prefilter offset"*
> per match. That is a real cost paid to avoid a hazard that §2.4 already has a
> construct for. More importantly, the flat ban was justified by *"neither
> compiler is watching"*, and under §2.4 one of them is.
>
> **Rejected: boxing with erased lifetimes and a documented discipline**, which is
> the parent note's own rejected alternative and stays rejected for its own reason:
> an unwritten obligation is the thing `ffi-c-boundary.md` §2.7 exists to
> enumerate rather than accept. The difference here is that the obligation is
> *written*, in a Science type, and checked.
>
> **Cost, and it is not small.** Three things.
> (i) The handle is no longer `'static`-shaped, so it cannot be returned past its
> referents' scope or stored in a long-lived structure — `ffi-c-boundary.md` §2.4
> names this and says the resulting region errors are *"correct and initially
> baffling"*. Generated code producing baffling region errors is worse than
> hand-written code producing them, because the user did not write the type that
> the error is about. The diagnostic obligation lands on `SC0486`.
> (ii) The `transmute` to `'static` is still in the generated Rust, so if Science's
> region engine has a hole, the hole is now reachable from generated code rather
> than from code somebody reviewed.
> (iii) The parent note's `find_from` rewrite remains the better shape where an
> index representation exists, and the generator should prefer it — Decision 3
> permits boxing, it does not recommend it.
>
> **The parent note's `regex::Matches` example is therefore reversed on its facts
> and upheld on its instinct**: the value *can* be boxed safely, and `find_from`
> is still the better binding.

### 3.4 `&str`, `String`, `Vec`, `Cow`, `Option`, `Result`

Each has a known representation question with a known answer. Which are
mechanical:

| Rust shape | Position | Lowering | Class |
|---|---|---|---|
| `&str`, `&[u8]`, `&[T: FFI-safe]` | argument | pointer + length; UTF-8 validated in the shim for `str` | **B** |
| `&str` | return | offsets into an input the caller already holds, or the two-call protocol; provenance from §3.3 | **C** — one entry per crate, not per item |
| `String`, `Vec<u8>` | return | two-call length-then-fill (`rust-interop.md` §2.4) | **B** under the standing convention |
| `String` | argument | pointer + length + a shim-side `to_owned()`; Case C never arises because the shim allocates | **B**, counted **C** here since it is a copy the caller cannot see |
| `Vec<T>` where `T` is a crate type | return | array of handles, or a batch-fill; needs to be told which | **C** |
| `Cow<'a, str>` | return | two-call protocol, collapsing both variants | **C** — one entry per crate |
| `Option<T>` | either | a presence flag plus an out-parameter, or the niche when `T` is `&T`/`Box<T>`/`NonNull<T>`/`fn` (`rust-interop.md` §4.5 rule 3) | **B** |
| `Result<T, E>` | return | status `c_int` plus an out-parameter, plus a message accessor for `E` | **C** — one entry per *error type*, covering every function that returns it |
| `bool` | either | one byte, 0 or 1 | **B** |
| `char` | either | `u32`, validated in the shim | **B** |
| integers | either | `c_int` / `c_long` / `c_size_t`, **never** `i32`/`i64` (`rust-interop.md` §4.5 rule 6) | **B** |

The leverage in that table is in the two **C** rows that are per-type rather than
per-item. One `regex::Error` entry covers 8 functions. One `serde_json::Error`
entry covers 52. One `PolarsError` entry covers most of polars' fallible surface.
That ratio is the whole economic argument for the generator, and it is the reason
the parent note's "215 lines of Rust" and this note's "15 TOML entries" describe
the same binding.

`Option<T>` deserves one specific note, because it is where Science and Rust agree
for free. `rust-interop.md` §4.5 rule 3 observed that Rust's guaranteed
null-pointer optimization for `Option<&T>` is *"exactly `ffi-c-boundary.md` §1.4's
niche rule, arrived at independently"*. So `Option<&Regex>` and Science's
`Pattern?` are the same bits, and the generator emits nothing at all for the
conversion.

### 3.5 Closures and iterators

**Closures.** `F: FnMut(&Captures) -> String` is a generic parameter with a
callable bound. `ffi-c-boundary.md` §4.3's trampoline handles the Rust side —
`extern "C" fn(*mut c_void, …)` plus a data pointer, wrapped in
`move |x| unsafe { f(data, x) }` — and the generator can emit it, because
rustdoc JSON gives the closure's argument and return types in the bound. That puts
closures in **C** with one entry (`callback`), not in D. Two constraints carry over
unchanged from `rust-interop.md` §3.7 item 6: the wrapping closure is not `Send`,
so a thread-pooled callee needs the §3.5 `Share` claim — **which is exactly the
case a polars `map` or `apply` hits, because polars will run it on a rayon
worker** — and §4.4's reentrancy hole and `SC0410` apply. The generator must not
silently bind a callback into an API that stores it, and it can tell, because
storing it shows up as a lifetime bound on the parameter.

**Iterators.** `Iterator` is a trait with a generic `next` and an associated
`Item`, so it does not cross as a trait. But the generator does not need the
trait; it needs the *impl*, and the impl names `Item` concretely. Two lowerings
exist and `rust-interop.md` §3.7 item 4's rule decides between them — **cross the
boundary once per batch, never once per element.**

> **Decision 4. An `Iterator` impl is bound as a batch-fill call, never as a
> per-element `next` shim.**
>
> ```science
> function science_regex_matches_next_batch(
>     handle: ffi.OpaqueHandle,
>     out: mutable ffi.MutableSpan of ffi.Found,
>     out_count: mutable borrowed ffi.Uninitialized of CSizeT) -> CInt
> ```
>
> **Rejected: a per-element `next` shim**, which is the obvious transcription of
> the trait. It costs an indirect foreign call per item, which `ffi-c-boundary.md`
> §5.2 prices at an order of magnitude worse than the `when available` dispatch it
> already calls expensive. For a regex over a large log file, or a polars
> `Series` iterated element-wise, that is the whole runtime.
>
> **Rejected: materializing the whole iterator into an array on the Rust side.**
> It is one call and it is unbounded memory; `Matches` over a 40 GB haystack is
> the same failure `rust-interop.md` Decision 7 refuses a copy for.
>
> **Cost.** The Science side of the batch call is a loop the user did not write and
> must still understand when it appears in a profile, and the batch size is a
> tuning knob with no obviously right default, living in generated code.

### 3.6 Panics

`ffi-c-boundary.md` §4.5 says panics do not cross, and `rust-interop.md`
Decision 6 forces `panic = "abort"` on the sidecar, under which both languages
have the same failure model. **The generator changes nothing here and must be
careful not to appear to.** §5.7 is where this stops being a formality, because
polars is the crate on which it bites.

One thing the generator *can* do that a human writing shims will not do
consistently: it can flag the functions whose documentation says they panic.
rustdoc JSON carries `docs` verbatim, and Rust's convention is a `# Panics`
section. That is a text heuristic over a prose convention, so it must not gate
anything.

> **Decision 5. The generator scans each item's doc text for a `# Panics` section
> and records the finding in the coverage report as advisory information. It never
> refuses to bind on that basis, and it never emits a guard.**
>
> **Rejected: refusing to bind items documented as panicking.** `Regex::new`'s
> documentation mentions panics in the context of `unwrap`; `Map::index` panics on
> a missing key; polars' `Index` impls panic on a missing column. Refusing them
> would remove correct bindings on the strength of prose, and prose is not a
> contract.
>
> **Rejected: wrapping each such call in `catch_unwind`.** Under Decision 6 of the
> parent note there is nothing to catch, and adding the wrapper would be the first
> step of reversing a decision that note made deliberately.
>
> **Cost.** A user reading the coverage report sees an advisory that may be stale
> or wrong, because it came from a doc comment. Said plainly in the report's header.

---

## 4. The generator

### 4.1 Shape and placement

> **Decision 6. `sciencec` gains `sciencec foreign bind`, a sibling of
> `rust-interop.md` §4.4's `foreign init` and `foreign sync`. It runs *out of
> band*, on demand, never as part of a compilation. Its outputs are checked into
> the sidecar.**
>
> **Rejected: invoking rustdoc during compilation.** This is `ffi-c-boundary.md`
> §7.1's argument transplanted, and all three of its reasons survive the move to
> Rust intact: it would make a nightly toolchain a dependency of every build (§8),
> it would make compilation output depend on what the machine's rustdoc emitted
> that day, and — the salsa reason — `foreign_archive`'s query key would have to
> include a rustdoc invocation whose output is not reproducible across nightlies.
>
> **Rejected: a separate binary outside `sciencec`.** The generator's output must
> agree with `sciencec`'s own `ffi.CLayout` computation to emit the `science_abi.rs`
> assertions of `rust-interop.md` §4.5, and two binaries that must agree on layout
> are two binaries that will one day not.
>
> **Cost.** Another subcommand, another checked-in artefact, another staleness
> check. `SC0487` is the staleness diagnostic and `sciencec foreign bind` is its
> fix, mirroring `SC0465` / `foreign sync`.

### 4.2 Inputs

1. **The rustdoc JSON** for the target crate and, transitively, for any dependency
   crate whose types appear in the target's public signatures. `serde_json`'s
   surface mentions `serde::Error`; `polars-lazy`'s mentions `polars-core`'s
   `DataFrame` and `polars-plan`'s `Expr`. Without the dependency documents those
   are opaque paths, and for a workspace-split crate like polars that is most of
   the surface.
2. **A policy section in `foreign/manifest.toml`** — the same file
   `rust-interop.md` Decision 5 already introduced for `Share`/`ShareBorrowed`
   claims, extended rather than duplicated:

```toml
[bind.regex]
version          = "1.13.1"
rustdoc-format   = 61
include          = ["regex::Regex", "regex::Error", "regex::Match", "regex::Captures"]

[[bind.regex.handle]]
type  = "regex::Regex"
share = ["Share", "ShareBorrowed"]        # discharged by rust-interop.md §3.5

[[bind.regex.error]]
type    = "regex::Error"
surface = "status-and-message"
message = "Display"

[[bind.regex.project]]
type   = "regex::Match"
fields = [["start", "CSizeT"], ["end", "CSizeT"]]
prefer = "stateless"                       # find_from over a boxed Matches (§3.3)

[[bind.regex.instantiate]]
item = "regex::Regex::replace_all"
args = { R = "&str" }
```

3. **The Science declarations already written by hand**, if any, so that
   regeneration does not clobber a user's own names. This is
   `ffi-c-boundary.md` §7.2's third cost — *"hand annotations must survive
   regeneration"* — and it is the failure that *"makes binding generators get
   abandoned"*. The policy file is the sidecar that note asks for, keyed by
   fully-qualified Rust path rather than by C function name.

### 4.3 Outputs

Four artefacts, all checked in.

**(a) The Rust shim**, `foreign/rust/src/<crate>_shim.rs`. One
`#[unsafe(no_mangle)] pub unsafe extern "C" fn` per bound item, obeying
`rust-interop.md` §4.5's six rules. This is the file the parent note says a human
must write.

**(b) The Science `extern` block**, an `unsafe extern "C" crate "…"` declaration
per `rust-interop.md` Decision 10, with `borrowed`, `ffi.Span`, `mutable borrowed`
and `from` clauses filled in from the Rust types — *not* the maximally
conservative `ffi.Pointer of T` that `ffi-c-boundary.md` §7.1 forces on C headers.

**(c) The safe Science layer.** This is the departure from `ffi-c-boundary.md`
§7.1 and it needs its own decision.

> **Decision 7. The generator emits the safe Science layer — handle types, `Drop`
> implementations, `Type has:` blocks, `-> (T, Error?)` signatures, `T?` returns —
> for every bucket-B item and for every bucket-C item its policy covers. It is
> *not* restricted to emitting the unsafe declarations.**
>
> **Rejected: the `ffi-c-boundary.md` §7.1 posture — generate the shape, never the
> contract.** That rule exists because *"C headers do not contain what the
> ownership boundary needs"*, and §1.2 establishes that rustdoc JSON does contain
> it: `&T` versus `T` is the ownership, `&mut` is the mutability, `&[T]` is the
> span with its length, the return lifetime is the retention, and `Result` is the
> fallibility. Applying a rule whose stated premise is false here would be
> imitation. `ffi-c-boundary.md` §7.3 says as much in advance.
>
> **Rejected: generating the safe layer for C-bucket items with a *default*
> policy.** Then the policy file becomes optional, and an absent policy produces a
> plausible wrong binding instead of a refusal. Decision 13 forbids exactly this.
>
> **Cost, and it is the one a reviewer should press on.** A generated safe layer is
> a large diff nobody reads, in idiomatic-looking Science, that a user will trust
> more than they trust a wall of `unsafe extern` declarations — and the trust is
> only as good as the generator's lowering table. `ffi-c-boundary.md` §7.2's
> mitigation applies unchanged and is mandatory: every generated file carries a
> header naming the tool version, the crate version, the rustdoc `format_version`,
> the policy file hash and the item's source path. The names are also worse than a
> human's: `find_at` stays `find_at`, and `Regex` becomes `Regex` rather than
> `Pattern`. A `rename` key in the policy file is the affordable answer and it is
> one more thing to maintain.

**(d) `science_abi.rs`**, exactly as `rust-interop.md` §4.5 already specifies —
signature coercions and `size_of` / `align_of` / `offset_of!` assertions, generated
from the Science declarations, discharged by rustc. Nothing new is asked for here,
and this is the point of the whole arrangement: **the generator's output is
checked by the same second compiler that the parent note already recruited.** A
generator whose output is checked by rustc is categorically different from
`cbindgen`, whose output is checked by nobody.

Here is what (c) looks like for `regex`, with the generated header Decision 7
requires:

```science
## Generated by `sciencec foreign bind` 0.4.1
## crate: regex 1.13.1 · rustdoc format_version 61
## policy: foreign/manifest.toml (hash 4c1e9a02) · source: regex/src/regex/string.rs
## Do not edit. Run `sciencec foreign bind` to regenerate.

interface Error:
    function message(self) -> String

type PatternError:
    text: String

PatternError implements Error:
    function message(self) -> String:
        self.text

type Pattern:
    raw: ffi.OpaqueHandle

Pattern implements Drop:
    function drop(mutable self):
        unsafe: science_regex_free(self.raw)

Pattern has:
    function compile(source: borrowed String) -> (Pattern?, PatternError?)

    function matches(self, text: borrowed String) -> Bool

    function find_from(self, text: borrowed String, start_at: Int) -> Found?
```

and here is a use of it, which is the thing a user actually writes and which the
generator does not touch:

```science
function count_matches(pattern_text: borrowed String,
                       text: borrowed String) -> (Int, Error?):
    let pattern, err be Pattern.compile(pattern_text)
    if err?:
        return (0, err)

    let mutable total be 0
    let mutable at be 0
    loop:
        let hit be pattern.find_from(text, at)
        if not hit?:
            break
        total be total + 1
        at be hit.end
        if at >= text.length():
            break
    return (total, null)
```

### 4.4 Where the human still intervenes

Four places, and the list is closed.

1. **The policy file.** 15 entries for `regex`, 13 for `serde_json`. Each is one
   fact the Rust type system genuinely does not carry — how an error should be
   surfaced to a language with a different error model, which projection of a
   borrowed value the caller wants, which instantiations are worth a symbol.
2. **The `Cargo.toml`.** Unchanged from `rust-interop.md` Decision 8.
3. **Naming and shaping**, if the generated names are not the ones the Science API
   should have. Optional, and the moment the user does it they own a hand-written
   layer over a generated one, which is the normal arrangement.
4. **Reviewing the unreachable report** and deciding what to do about it.

**What the human no longer does is write Rust.** That is the whole of the change
this note makes to `rust-interop.md`'s mechanism, and it is worth stating in
exactly those words, because it is smaller than "Science can consume crates" and
larger than nothing.

### 4.5 What it does when it meets something unreachable

Silence is not acceptable. A generator that emits 66 of 139 functions and says
nothing has told the user that `serde_json` has 66 functions.

> **Decision 8. The generator emits a coverage report as a required output
> artefact, listing every public item it did not bind and why, classified by
> cause. A run that binds nothing still produces the report. The report is checked
> in alongside the generated sources.**
>
> **Rejected: a summary count.** *"91.4% bound"* is the number this note leads
> with and it is the wrong thing for a tool to print, because the 8.6% is not a
> random sample — it is `to_string` and `from_str`.
>
> **Rejected: failing the run on any unreachable item.** Then no crate binds,
> since every crate has a `Debug` impl over something. Unreachable surface is the
> normal condition, not an error.
>
> **Rejected: a warning per item.** 279 warnings for `serde_json` is a wall
> nobody reads, and it would train users to pass `--quiet`.
>
> **Cost.** Another checked-in file whose diff is large on a version bump. In
> exchange, the diff of that file *is* the semver review: an item moving from bound
> to unreachable is a breaking change in the crate, and it shows up in a pull
> request instead of at link time.

The report is data, and a Science program is the natural thing to read it with:

```science
type ItemCoverage:
    path: String
    bucket: String
    reason: String

function report_unbound(items: borrowed Array of ItemCoverage):
    let mutable unbound be 0
    for item in items:
        if item.bucket is not "bound":
            print(f"{item.path}: {item.reason}")
            unbound be unbound + 1
    print(f"{unbound} of {items.length()} public items were not bound")
```

The causes the report distinguishes are the §3 taxonomy:
`open-world-bound`, `caller-implements-trait`, `associated-type`,
`unresolvable-lifetime`, `missing-policy`, `explicitly-excluded`. Only
`missing-policy` is actionable by editing the policy file; the first four are the
ceiling, and telling the two apart is the report's main job.

> **Decision 9. The generator never invents a policy. An item that the user asked
> for and that needs an undeclared policy is `SC0483`, not a default.**
>
> **Rejected: a default per policy kind** — errors default to a status code with
> the message dropped, projections default to boxing, generics default to no
> instantiation. Every one of those defaults is right often enough to be trusted
> and wrong often enough to be dangerous, and the wrong ones are silent:
> `rust-interop.md` §2.4 already records that dropping `regex::Error`'s message is
> the mistake *"every shim under-budgets"*, and a default would make it the norm.
>
> **Cost.** Binding a crate is an iterative loop — run, read the errors, add
> policy, run again — rather than one command. It is also, unavoidably, the reason
> a crate cannot be bound by someone who does not read Rust: the policy questions
> are Rust questions even though the answers are TOML.

---

## 5. Polars, and what it takes to make it reachable

`polars` is the crate the project owner named, alongside scikit-learn, as
something that must be usable. It is also the crate on which the generator's
coverage number is most misleading and the surrounding policy questions are most
binding. This section answers them rather than filing them.

### 5.1 Why polars' coverage number is the wrong number

§2.3 reports 95.1% reachable over 1,676 user-facing functions, and that number is
nearly meaningless, for the reason §2.5 item 5 flags. Consider the shape of the
lazy API:

```rust
impl LazyFrame {
    pub fn filter(self, predicate: Expr) -> LazyFrame
    pub fn select<E: AsRef<[Expr]>>(self, exprs: E) -> LazyFrame
    pub fn group_by<E: AsRef<[Expr]>>(self, by: E) -> LazyGroupBy
    pub fn collect(self) -> PolarsResult<DataFrame>
}
```

Every one of those is bucket B or a one-entry C: handle in, handle out. A
generator binds all of them without breaking a sweat, and a Science user who has
them can do **nothing**, because the entire meaning is carried in the `Expr`
values, and `Expr` is a tree built by a fluent API of several hundred methods
(`col`, `lit`, `gt`, `alias`, `sum`, `over`, `when/then/otherwise`, the `dt`,
`str` and `list` namespaces) whose composition is the language.

The measurement makes the point quantitatively as well. Of polars' 2,486
inherent-plus-own-trait functions, **2,198 have no type parameter at all** — they
are handle-shuffling — while the 288 that *are* generic are the ones carrying
`Into<Expr>`, `IntoIterator`, `Fn` and `PolarsDataType`, with arity up to seven
and 73 distinct bound-sets. **The easy 88% is the plumbing and the hard 12% is the
API.** A coverage number weights them equally; a user does not.

So polars presents two separable problems and they have different answers:

- **Getting data in and out.** Answered by §5.2 — do not bind the crate, bind the
  interchange format — and it is the larger half by value.
- **Expressing an operation.** Answered by §5.4, and the honest answer is a
  narrower one than binding the expression API.

**This generalises past polars**, and it is the main methodological finding of
this note: *a coverage percentage over a crate whose API is a builder for an
embedded language measures the builder, not the language.* Any future audit of
`ndarray`, `nalgebra` or a plotting crate must state whether its B count is load-
bearing or whether, like polars', it is a count of handle-shuffling methods.

### 5.2 The rule: bind the interchange format, not the crate

Polars is Arrow-backed natively. The **Arrow C Data Interface** is a stable,
versioned C ABI consisting of two `#[repr(C)]` structs plus a streaming one:

```c
struct ArrowSchema { const char* format; const char* name; const char* metadata;
                     int64_t flags; int64_t n_children; struct ArrowSchema** children;
                     struct ArrowSchema* dictionary;
                     void (*release)(struct ArrowSchema*); void* private_data; };

struct ArrowArray  { int64_t length; int64_t null_count; int64_t offset;
                     int64_t n_buffers; int64_t n_children; const void** buffers;
                     struct ArrowArray** children; struct ArrowArray* dictionary;
                     void (*release)(struct ArrowArray*); void* private_data; };
```

`ArrowArrayStream` adds a pull-based iterator over the pair. Each struct owns
itself through a `release` callback, which is `ffi-c-boundary.md` §2.3's **Case B**
with the destructor travelling inside the value.

**Verified against polars 0.51.0 rather than assumed.** `polars_arrow::ffi`
declares all three structs `#[repr(C)]` — they are `rust-bindgen` output from
Arrow's own `abi.h`, which is as strong a statement of ABI intent as exists — and
exports `export_array_to_c`, `export_field_to_c`, `import_array_from_c`,
`import_field_from_c`, `export_iterator` and `ArrowArrayStreamReader`. Every field
of `ArrowArray` and `ArrowSchema` is a primitive integer, a raw pointer or an
`Option<unsafe extern "C" fn>`, which is `rust-interop.md` §4.5 rule 3's permitted
niche.

**So the *data* crosses as bucket A: no translation, no policy, no shim** — the
crate author already did the work §2 says nobody does. What still needs a shim is
the handful of Rust-ABI calls that hand the structs over (`export_array_to_c`,
`export_field_to_c`, `export_iterator`, and their import counterparts), and those
are bucket B, unconditionally. **Three structs and roughly six functions, against
the 3,021 of §2.3.** That ratio is the argument.

**Three sibling notes have already made this exact bet, and this note is the
fourth instance of one rule rather than a new idea:**

- `data-io.md` §9: *"The binding is not to Arrow's C++ API. It is to the **Arrow C
  Data Interface** and **C Stream Interface** — three plain C structs … a published
  stable ABI, and no C++ symbols crossing the boundary."*
- `data-io.md` §4.2: a `Frame` column *"**is** the buffer, laid out to satisfy both
  Arrow's C Data Interface and the DLPack layout §8 of the core spec already
  committed to"*, and §9 says a `Frame` can *adopt* an Arrow chunk without copying,
  *"holding the release callback and calling it on `Drop`"*.
- `data-io.md` §3 names the Arrow IPC handoff as *"the zero-copy handoff to pandas,
  **polars**, DuckDB"*. Polars is already the named consumer.
- `python-interop.md` §4.1 does the same for tensors with DLPack, and
  `c-binding-coverage.md` §3.8 Decision 6 does it for C++ — *"Science binds a C++
  library only through a C ABI **the library itself publishes and versions**."*

> **Decision 10. Where a crate publishes or consumes a stable C-ABI interchange
> format for its central data type, Science binds the format, not the crate. For
> polars that is the Arrow C Data Interface and C Stream Interface, and the Science
> side of it is `data-io.md` §9's existing `Frame` adoption path, unchanged.**
>
> **Rejected: generating a binding to polars' `DataFrame` and `Series` API.** The
> coverage number says it would largely work (§2.3), and it is refused for three
> reasons in increasing weight. It is an enormous surface with a fast-moving
> pre-1.0 semver, so the binding is a permanent treadmill against a crate that
> renames methods between minor versions. It duplicates `data-io.md`'s `Frame`,
> so a Science program would hold two dataframe types with a conversion between
> them. And it is a *copy* at every boundary crossing unless the buffers are
> shared, which is the thing Arrow already solves.
>
> **Rejected: exchanging through Arrow IPC / Feather files or buffers.** It is
> correct, it needs no new ABI, and `data-io.md` already plans the reader. It is
> refused as the *primary* path because it serializes: a group-by over a 40 GB
> frame that round-trips through IPC has paid for the data twice, and the whole
> reason to reach for polars is that it is fast.
>
> **Rejected: making it a polars-specific special case rather than a rule.** The
> same move is available for `arrow-rs`, `duckdb`, `parquet`, any crate that
> implements `__arrow_c_stream__`'s Rust equivalent, and — outside Arrow — for
> `candle` and `ndarray` through DLPack, `image` through a raw buffer descriptor,
> and `zstd` through a byte slice. Stating it as a rule is what makes the next
> crate cheap.
>
> **Cost, and it is a real one.** The rule only applies where a format exists.
> Where it does not — `regex` has no interchange format, and neither does an
> optimiser or a solver — the generator is the whole answer and §2's numbers are
> the relevant ones. The rule also buys *data* movement and not *operations*: see
> §5.3.

**The generalised statement, for §10's ask:**

> **When a crate has a stable interchange format, bind the format. When it does
> not, generate a binding to the crate. Never bind an API whose data type has a
> published ABI, because the ABI is smaller, more stable, and shared with every
> other consumer.**

### 5.3 What Arrow buys, and what it does not

Stated plainly, because the temptation is to claim the whole problem is solved.

**It buys, completely:**

- A polars `DataFrame` becomes a Science `Frame` with **no copy**: each column's
  buffers are adopted, the `release` callback becomes the Science column's `Drop`,
  and the validity bitmap becomes `data-io.md` §9's `Option of T` column. Both
  directions, because the interface is symmetric.
- **The dependency direction inverts, which is the underrated part.** Science does
  not need to model polars' type system; it needs to model Arrow's, which it has
  already committed to modelling for Parquet. So the marginal cost of polars over
  what `data-io.md` §9 already buys is close to zero on the data path.
- Schema, dtypes, nullability and nested types come across as data, not as a
  binding.

**It does not buy:**

- **Operations.** `group_by`, `join`, the lazy optimiser, `sort`, window functions
  — these are polars functions and reaching them is a shim. What Arrow changes is
  the shim's *shape*: every one of those takes and returns an Arrow-backed frame,
  so the shim is `ArrowArrayStream in → operation → ArrowArrayStream out`, which is
  narrow and mechanical, rather than an API translation over `DataFrame`.
- **The expression language.** §5.4.
- **Streaming/out-of-core execution.** Polars' streaming engine produces results
  incrementally; `ArrowArrayStream` matches that shape exactly, which is fortunate,
  but the back-pressure and cancellation story is the shim's problem and is not
  specified here.
- **Anything about threads.** §5.5.

### 5.4 The expression language, and the SQL escape

`Expr` is the real API surface, and binding it is a few hundred methods — some
handle-to-handle and mechanical, and the rest carrying exactly the `Into<Expr>` /
`IntoIterator` / `Fn` bounds §3.1 measures at arity up to seven. It is also a
maintenance surface that moves every polars release.

There is a cheaper first version, and polars ships it: a **SQL front end**
(`polars-sql`'s `SQLContext`, behind the `sql` feature) whose whole relevant
surface is three calls —

```rust
pub struct SQLContext { /* … */ }
impl SQLContext {
    pub fn register(&mut self, name: &str, lf: LazyFrame)
    pub fn execute(&mut self, query: &str) -> PolarsResult<LazyFrame>
}
```

— of which `register` is bucket B and `execute` is bucket B plus the one
`PolarsError` policy entry that §2.3 already counts. **The entire expression
language collapses to one `&str` parameter, and the binding for it is two
functions.**

> **Decision 11. In the first version, polars operations are reached through its
> SQL front end plus a small fixed set of frame-level calls, not through a
> generated binding to `Expr`.**
>
> **Rejected: generating the `Expr` builder binding.** It is mechanical and it
> would work. It is refused for the first version because it is several hundred
> symbols on a pre-1.0 semver, because every one of them needs a Science-side safe
> wrapper to be usable, and because the resulting Science code is a transliteration
> of polars' fluent API that neither reads like Science nor like polars.
>
> **Rejected: Science growing its own expression language that lowers to `Expr`.**
> This is the *right* long-term answer and it is a language feature, not a binding:
> it needs `broadcasting.md`'s deferred-expression machinery and a lowering pass.
> Refused here as out of scope, and named so it is not rediscovered.
>
> **Rejected: SQL only, with no frame-level calls.** Some operations are not
> expressible in polars' SQL dialect, and a binding that cannot `sort` without a
> `SELECT` is a binding people work around.
>
> **Cost, and it is the largest single concession in this note.** SQL is less
> expressive than polars' expression API — no `over` with arbitrary window
> expressions, a smaller function set, no user closures — and its errors are
> strings, so a malformed query is a run-time `PolarsError` rather than a Science
> type error. A Science user gets "polars through SQL", which is a genuinely
> smaller thing than "polars". It is also the version that fits on one page and
> works on the day it ships.

### 5.5 The thread pool: from prohibition to a budget

`stdlib-shape-and-packages.md` §3.6 says:

> **There is no second thread pool, and none is exposed.** `.parallel()` (F2) owns
> the pool. Exposing a second one is how a program ends up with 256 threads on a
> 64-core node, which on a shared cluster is an administrative incident rather
> than a performance bug.

`rust-interop.md` §6 files the consequence as an open question: *"`polars` and
anything else built on `rayon` carries a persistent thread pool, which §3.6
forbids — while `ffi-c-boundary.md` §2.7 item 10 records that OpenBLAS and MKL
spawn their own threads inside a call and calls it harmless… The rule's boundary
is genuinely unclear."*

**Answering it requires correcting a factual premise first, and the correction is
what resolves the question.**

`ffi-c-boundary.md` §2.7 item 10 says of OpenBLAS and MKL that *"the threads live
and die inside the call"*. **That is not what those libraries do.** OpenBLAS
creates its worker threads on first use and keeps them parked between calls; MKL
does the same; an OpenMP-backed build keeps the OpenMP runtime's pool alive for
the process. This is not a subtlety — it is why `openblas_set_num_threads` and
`mkl_set_num_threads` exist as *runtime* setters, why `OMP_NUM_THREADS` is read at
init and matters for the whole job, and why Python's `threadpoolctl` exists at
all. `native-dependencies.md` §2.1 describes the symptom in its own words: NumPy
and SciPy wheels *"each carry their own OpenBLAS, so a scientific Python
environment routinely has two or three copies of BLAS in one process, each with
its own thread pool, and the resulting oversubscription is a well-documented
performance bug that ordinary users cannot diagnose."* Two *transient* pools do
not produce a chronic, undiagnosable bug; two persistent pools sized to the
machine do.

**So the transient/persistent axis does not separate the accepted case from the
refused one, because the accepted case is not transient.** Two axes actually do
the work, and separating them is the whole answer:

| Axis | Bounded / acceptable | Unbounded / refused |
|---|---|---|
| **Cardinality** | A **process-wide singleton** pool, created once. Total threads are `N + M`, a constant. | A pool **per call, per object or per nesting level** — OpenMP's nested parallelism, a library that spawns on entry. Total threads are a product and grow with call sites. |
| **Sizing** | The size is **settable from Science's budget**, by a runtime setter or by an environment variable read at a known init point. | The size is derived from the machine with no knob at all. |

This matters more than it looks, because it changes the arithmetic everyone
assumes. If Science's own pool has `N` workers and each of them calls polars, the
process does **not** acquire `N × M` threads. Polars' pool is one process-wide
singleton (`polars_core::POOL`, a `LazyLock<ThreadPool>`), so `N` concurrent
callers contend for the *same* `M` workers. The worst case is `N + M`, which a
budget can simply split. The `N × M` disaster — the one §3.6 is really describing
with *"256 threads on a 64-core node"* — comes from per-call spawning, not from a
singleton.

And that is the *correct* reason `ffi-c-boundary.md` §2.7 item 10's conclusion
(harmless) is right: not that OpenBLAS's threads die inside the call, which they
do not, but that there is exactly one OpenBLAS pool in the process and its size is
settable. **The conclusion survives; the stated reason does not, and the stated
reason is what `rust-interop.md` §6 was reasoning from when it filed the question
as unresolvable.**

That reframing also shows that §3.6 read literally is already violated by a
decision the project has made. `scientific-libraries.md` requires `linalg`, which
requires a BLAS, which holds a persistent pool. A rule that forbids polars on
threading grounds forbids `dgemm` on the same grounds. A rule with that property
is not a rule anyone is enforcing; it is a sentence about Science's *own* API that
has been read as a sentence about linked libraries.

> **Decision 12. `stdlib-shape-and-packages.md` §3.6's prohibition is narrowed to
> what it was actually about, and a coordination mechanism replaces the ban for
> foreign libraries:**
>
> > **Science code may not create or expose a second thread pool. A *linked
> > foreign library* may hold one, provided it is (i) a process-wide singleton,
> > (ii) declared in the sidecar manifest, and (iii) sized from Science's single
> > thread budget at a declared init point. A library that spawns per call, or
> > that is undeclared, or whose pool size cannot be set at all, is refused
> > (`SC0489`).**
>
> Science owns one integer, `N`, derived exactly as §3.6 already specifies —
> `SLURM_CPUS_PER_TASK`, `OMP_NUM_THREADS` or a cgroup quota before
> `os.cpu_count()`. The manifest descriptor has to distinguish **two kinds of
> knob**, because the two behave differently under Decision 13:
>
> ```toml
> # kind = "setter": re-sizable at any time.
> [[native.openblas.threads]]
> kind   = "setter"
> symbol = "openblas_set_num_threads"
>
> # kind = "env-at-init": sized once, on first touch. The shim must set the
> # variable and then FORCE the pool, so the timing is deterministic.
> [[bind.polars.threads]]
> kind     = "env-at-init"
> variable = "POLARS_MAX_THREADS"
> force    = "science_polars_force_pool"   # touches polars_core::POOL at startup
> ```
>
> The `force` entry is not decoration. `polars_core::POOL` is a `LazyLock` built
> on first use from `POLARS_MAX_THREADS`, falling back to
> `std::thread::available_parallelism()`. Setting the variable without forcing the
> pool leaves the moment of sizing at the mercy of whichever polars call happens
> first, and a `set_var` racing a live thread is unsound in Rust 2024. Forcing it
> in the sidecar's init — at process startup, single-threaded, before any Science
> thread exists — is the one point where both problems are absent.
>
> **Rejected: keeping the prohibition.** It excludes polars, `rayon`, and — read
> honestly — OpenBLAS and MKL, and therefore `linalg`. A rule that excludes the
> project's own numerical stack is a rule that will be quietly ignored, and a
> quietly ignored rule is worse than a narrower enforced one.
>
> **Rejected: allowing pools with no coordination**, i.e. deleting §3.6's
> constraint. It reproduces exactly the failure `native-dependencies.md` §2.1
> refuses to copy from the wheel ecosystem, on machines where §3.6 correctly says
> it matters more, not less.
>
> **Rejected: Science installing itself as the executor for every foreign
> library** — one pool that rayon, OpenBLAS and Science all submit to. It is the
> theoretically right answer, it is what a single-runtime language would do, and it
> is refused because it requires each library to accept a foreign executor. rayon
> can *almost* do it (`ThreadPoolBuilder::spawn_handler`); OpenBLAS cannot at all.
> A mechanism available for one library in the set is not a mechanism.
>
> **Rejected: requiring a runtime setter**, which was this note's first draft of
> the rule. It would refuse polars, because `polars_core::POOL` has no public
> re-sizing path — and refusing polars is the outcome this section exists to avoid
> and cannot honestly reach, since an env-var-plus-force *does* size the pool from
> Science's budget. The cost of admitting `env-at-init` is Decision 13.
>
> **Cost, itemised.**
> - **`native-dependencies.md`'s provider machinery gains a field.** Every native
>   library entry needs a `threads` descriptor with the `kind` distinction above.
>   §10 asks for it.
> - **The budget must be computed before any foreign init runs**, which puts it in
>   the runtime's startup path ahead of lazy library initialization. That is a
>   `crates/science-rt` ordering constraint and it is the kind discovered by a bug.
> - **Detection of undeclared pools is heuristic.** A crate that transitively pulls
>   in another pool — polars depends on rayon, and a `*-sys` crate underneath might
>   hold its own — is only as coordinated as its declaration is complete. The
>   coverage report lists every pool-holding crate the generator can detect, and it
>   detects by dependency name, which is a guess.

### 5.6 Nesting: static partition, because polars cannot be re-sized

If Science's own pool has `N` workers and polars' has `M`, the process has `N + M`
threads (§5.5), not `N × M`. That is a budget-splitting problem, not a
catastrophe — but it is still a problem, and the shape of its solution depends
entirely on which kind of knob the library has.

For a **`kind = "setter"`** library the answer is the established one, which is
`threadpoolctl`'s and scikit-learn's: collapse the foreign pool to one thread
while Science's own parallel region is live, restore it afterwards. Dynamic,
correct, and available for OpenBLAS, MKL and FFTW.

For a **`kind = "env-at-init"`** library — polars — it is **not available**, and
no amount of design makes it available. `polars_core::POOL` is built once. There
is no public API to resize it, and building a second one would be a second pool,
which is the thing being avoided.

> **Decision 13. The thread budget is partitioned. For a `setter`-kind library the
> partition is dynamic: the foreign pool is set to 1 while a Science parallel
> region is active and restored on exit. For an `env-at-init`-kind library the
> partition is **static and declared at bind time** — the manifest says whether
> that library or Science's own pool receives the budget, and the runtime sizes
> both accordingly at startup.**
>
> ```toml
> [[bind.polars.threads]]
> kind     = "env-at-init"
> variable = "POLARS_MAX_THREADS"
> budget   = "library"       # or "science", or an explicit integer share
> ```
>
> **Rejected: leaving it to the user via an environment variable.** It is what the
> Python ecosystem did for a decade and it is the source of the bug
> `native-dependencies.md` §2.1 calls undiagnosable. The difference here is that
> Science computes the value and sets it; the user declares a *policy*, not a
> number.
>
> **Rejected: a dynamic partition for `env-at-init` libraries anyway**, by
> re-`set_var`-ing and hoping. The pool is already built; the variable is not read
> again; and mutating the environment of a running multi-threaded process is
> unsound. It would be a mechanism that appears to work and does nothing.
>
> **Rejected: dividing the budget `N/k` per Science worker.** Much harder to
> reason about, interacts badly with setters that have a minimum of 1, and makes
> performance a function of two nesting levels the user cannot see.
>
> **Rejected: refusing `env-at-init` libraries outright** so that the partition is
> always dynamic. That refuses polars, for the benefit of a tuning knob.
>
> **Cost, and it is the honest limit of this section.** A Science program that uses
> both its own parallelism and polars must choose, once, at build time, which one
> gets the cores — and the right choice depends on the data, which the user knows
> and the manifest does not. A parallel Science loop over many small frames wants
> `budget = "science"`; a single query over one large frame wants
> `budget = "library"`. Getting it wrong is a factor-of-`N` performance bug with no
> diagnostic, because both configurations are correct. §13 records this as the
> gap most likely to need a language-level answer.

**The library with no knob at all is refused, and that class must be named.** Some
libraries hold a pool with no setter and no environment variable, or read one only
at `dlopen` time before Science's runtime exists. For those:

- The value must be set by the job launcher. The toolchain should print the
  required setting rather than pretend to handle it.
- The library is otherwise refused registration and therefore refused binding,
  with `SC0489` naming it. An escape hatch (`threads = "unmanaged"`) exists so a
  user who knows what they are doing can proceed; it is a claim in a file a
  reviewer can find, which is the same posture as `borrowed` in an `extern`
  declaration.

**Polars is not in that class, but it is one step closer to it than this note's
first draft assumed.** Polars does *not* use rayon's global pool, so
`ThreadPoolBuilder::build_global()` — the obvious mechanism, and the one
`rust-interop.md` §6's framing implies — has no effect on it whatsoever. The only
knob is `POLARS_MAX_THREADS`, read once. That is enough for Decision 12 and not
enough for a dynamic Decision 13, which is exactly why the two decisions are
separate.

### 5.7 `panic = "abort"` and rayon — the third condition, and the one with no good answer

`rust-interop.md` Decision 6 builds the sidecar with `panic = "abort"` and names
the cost in advance:

> `rayon` propagates a worker panic to the joining thread via `catch_unwind`, and
> under `panic = "abort"` that becomes a process abort instead.

Polars inherits this. Polars is generally disciplined about returning
`PolarsResult`, but panics do occur — in `Index` impls, in some cast and schema
paths, and in any user closure passed to `map`/`apply`. Under the sidecar's
profile, one of those in a worker aborts the whole process.

For a scientific batch job that is a bad failure mode: an eight-hour run ends with
no result and no recoverable error, where the same operation in Python's polars
would have raised. The options are all unattractive:

- **Accept it.** The failure model is then the same as Science's own (`panic`
  aborts, core spec §8), which is the parent note's whole argument for the
  setting, and the user's protection is that polars' *documented* fallible
  operations return `PolarsResult`, which does cross as an error.
- **Reverse Decision 6 for this sidecar.** Two failure models in one binary, which
  the parent note refuses for a reason this note has no standing to overturn.
- **Wait for Science to gain unwinding panics**, at which point
  `rust-interop.md` §3.6's forward-compatibility note applies and the sidecar flips
  to `panic = "unwind"` with a `catch_unwind` per shim.

> **This note accepts the first and records that it is the weakest point in the
> polars story.** It is the only one of the three conditions that has no fix
> available today, and it should be in the polars binding's own documentation
> rather than discovered. §12 lists it as a risk and §13 as an open question.

### 5.8 Verdict: is polars reachable?

**Yes, under three conditions, two of which this note discharges and one of which
it accepts with a named cost.**

| Condition | Status |
|---|---|
| The data path must not be an API binding | **Discharged.** Decision 10: Arrow C Data Interface, three `#[repr(C)]` structs and about six shim functions, reusing `data-io.md` §9's existing `Frame` adoption path. Marginal cost over Parquet support close to zero. |
| The operation path must be expressible | **Discharged, narrowly.** Decision 11: `SQLContext::register` + `execute` — two bucket-B functions — plus a small fixed frame-level surface. Materially less than polars' full expression API, and it fits on a page. |
| The pool must be coordinated | **Discharged, statically.** Decisions 12 and 13: `stdlib-shape-and-packages.md` §3.6 narrows from prohibition to a declared, budgeted, process-wide singleton. Polars is sized through `POLARS_MAX_THREADS` plus a forced init, **not** through `ThreadPoolBuilder::build_global`, which does not touch it. |
| The budget must be re-partitionable per parallel region | **Not discharged.** Polars' pool is built once and cannot be resized, so the split between Science's pool and polars' is fixed at build time (§5.6). A performance gap, not a correctness one. |
| A panicking rayon worker aborts the process | **Accepted, not fixed.** §5.7. The weakest point, with no available remedy under `rust-interop.md` Decision 6. |

**Neither of the two leads failed.** The Arrow route works and is better than
expected, because polars' own `#[repr(C)]` structs put the data path in bucket A.
The thread-budget route works, but one step less well than it first appeared: the
obvious mechanism (`build_global`) is inapplicable, the working mechanism is an
environment variable forced at init, and the consequence is that Decision 13's
partition is static rather than dynamic for polars specifically.

**What a Science user gets**, stated so nobody is surprised: read a Parquet or
Arrow file into a `Frame` with no copy; hand that `Frame` to polars with no copy;
run a SQL query or one of a fixed set of frame operations; get a `Frame` back with
no copy. What they do not get: polars' expression API, `map`/`apply` with a Science
closure across the thread boundary without a `Share` claim, or a recoverable error
from a polars panic.

```science
function mean_by_station(path: borrowed Path) -> (Frame, Error?):
    let frame, err be data.parquet.read(path)
    if err?:
        return (Frame.empty(), err)

    let session be polars.Session.new()
    session.register("readings", frame)

    let result, query_err be session.query(
        "select station, avg(temperature) as mean_t
         from readings group by station")
    if query_err?:
        return (Frame.empty(), query_err)

    for column in result.columns():
        print(f"{column.name}: {column.length()} rows")
    return (result, null)
```

Nothing in that sample copies the data, and nothing in it mentions Rust.

---

## 6. The ceiling, stated without flattery

The project's owner asked that *all* Rust code be usable. Here is the real answer.

**The ceiling is not "every crate". It has two different shapes depending on
whether the crate has an interchange format.**

**For a crate with a stable interchange format** — polars, `arrow`, `duckdb`,
`candle`, anything Arrow- or DLPack-shaped — the ceiling is high on the data path
and low on the operation path. The data crosses with no copy and no binding; the
operations cross through a narrow shim you choose the size of. The generator is
barely involved. **This is the better outcome and it applies to fewer crates.**
For polars the whole data path is three `#[repr(C)]` structs and about six shim
functions, against a full-API binding of 3,021 functions and 111 policy entries.

**For a crate without one** — `regex`, `serde_json`, a solver, an optimiser — the
ceiling is what §2 measures: **of the functions a user would actually call,
`regex` 100% and `serde_json` 91.4%**, both requiring a policy file and neither
requiring a human to write Rust. Directly mappable with no shim at all is **0% of
every crate measured** — 4,000-odd public functions across four crates, and not
one of them callable without a shim — confirming the parent note exactly.

**And for a crate whose API is a builder for an embedded language** — polars is
the case in hand, and a plotting or query crate would be another — the coverage
number measures the builder and not the language, so the ceiling is whatever the
embedded language's own entry point offers. For polars that is SQL, which is
smaller than `Expr` and is two functions wide.

And the things that are still out, unchanged by anything here:

- **`async` crates** — `rust-interop.md` Decision 16. A generator does not touch
  this: `async fn` returns an anonymous `impl Future`, unreachable by §3.2(c) as
  well as by policy.
- **Anything whose contract is a trait the Science side must implement.** One
  callback entry, yes; a thirty-method vtable with associated types, no.
- **Anything reached through a procedural macro.** `serde_derive` runs arbitrary
  Rust over the *user's* types. The substitute is Science having its own derive
  mechanism — the README's standing ask, now with a measured price.
- **A library whose thread pool has no setter** (§5.6).
- **Recoverable failure from a panicking rayon worker** (§5.7).
- **The C library underneath.** `rust-interop.md` §9's last risk holds in full: the
  guarantee is one layer deep.

> **The sentence that should be used publicly, replacing `rust-interop.md` §9's
> "Science links Rust": *Science exchanges data with Rust crates through the
> interchange formats they already speak, and generates bindings to the rest from
> the crate's own type information, so a human writes a page of declarations
> instead of a few hundred lines of Rust.* It is true, it is specific, and it is
> still narrower than "Science can use any crate", which remains false.**

---

## 7. What this does to `rust-interop.md`, and to `stdlib-standard.md` §9.3

### 7.1 The parent's refusal: narrowed, in one specific row

> **Decision 14. `rust-interop.md` §6's table row — "Automatic binding generation
> over a crate's public API: **Impossible, not expensive**" — is *narrowed*, not
> overturned and not confirmed. The narrowed form:**
>
> > *Automatic generation of a binding **that requires no declaration to be
> > written** is impossible, for the reason that note gives. Automatic generation
> > of **the shim crate**, from rustdoc JSON plus a policy file of 13–15 entries,
> > is possible and covers 100% and 91.4% of the user-facing surface of the two
> > format-less crates measured. For a crate with a stable interchange format the
> > question does not arise, because Decision 10 says not to bind its API at all.*
>
> **The part that is confirmed, exactly as written:** §2.3's *"What fraction of a
> crate is reachable? **Zero.**"* Bucket A is 0 for every crate measured,
> `#[repr(C)]` appears on none of their public types, and every function carries
> `abi: "Rust"`. Nothing here softens that sentence.
>
> **The part that is narrowed:** §6's parenthetical — *"the missing contract is
> **present in Rust's types** and **not representable in Science's**"*. The first
> half is true and is the premise of this whole note. The second half is true of a
> specific and small residue — the 15 and 13 policy facts — and false of the rest,
> because `borrowed`, `ffi.Span`, `Drop`, `T?`, `-> (T, Error?)` and
> `ffi-c-boundary.md` §2.4's borrowed-field handle between them represent most of
> what a Rust signature says. The parent note's §3.1 correspondence table is the
> evidence against its own §6 row: two ownership models that agree on eleven
> properties do not have an unrepresentable gap between them.
>
> **The part that is overturned outright:** §6's *"Nearest affordable thing"*
> column, *"a hand-written shim crate per library"*, and Decision 1's *"a
> hand-written shim crate as the only binding mechanism"*. Both become "a generated
> shim crate plus a hand-written policy file, or an interchange-format binding
> where one exists".
>
> **Also narrowed: Decision 7** (§3.3), and **`stdlib-shape-and-packages.md` §3.6**
> (§5.5).
>
> **Why this is narrowing rather than overturning:** the parent note's load-bearing
> claim is that a tool cannot supply information the source does not contain. That
> claim is correct and this note does not contest it. What this note contests is
> the empirical premise that the source does not contain it — assessed against
> `extern "C"` signatures and C headers, not against the crate's own type
> information. The conclusion moves because the input moved.
>
> **Cost of the narrowing.** §6 currently reads as a clean refusal, which is a good
> property for a note to have; after the amendment it reads as a refusal with two
> exceptions, which is a worse property and is the accurate one. And the project
> acquires a tool it must maintain against a nightly compiler's unstable output
> format (§8), which the flat refusal did not.

### 7.2 `stdlib-standard.md` §9.3 and `text.regex`

`rust-interop.md` §2.4 is careful about what reversed §9.3's decision to write an
RE2-style engine in Science:

> Not "Science can consume crates" — nothing about the general mechanism was used.
> What reversed it is that **somebody wrote 215 lines of Rust**.

Does that change if a generator writes those lines? It does, in three ways, and
one of them cuts the other way.

**Stronger on the one-time cost.** The 215 lines become a policy file with 15
entries plus a review of generated output. §9.3's comparison was *"a few thousand
lines of real, subtle code"* against 350; it is now a few thousand lines against a
page of declarations.

**Stronger on the recurring cost, which was the weaker half of the original
argument.** `rust-interop.md` §9 names *"the curated set is a commitment surface…
four semver treadmills"* as a standing risk. A semver bump under a generated
binding is: regenerate, diff the coverage report, fix any `missing-policy` entries
the new version introduced. Under a hand-written binding it is: read the changelog,
find the affected shims, edit Rust. The first is a review task and the second an
engineering task, and that is the difference between a maintenance burden that
scales to a curated set of twenty crates and one that does not.

**Weaker on one axis, and it must be said.** The generator introduces a nightly
toolchain as a dependency of *regenerating* the standard library's own bindings
(§8). §9.3's decision to write the engine in Science has the property that the
standard library depends on nothing. That property is worth something real, and a
generated binding does not have it — it swaps a bounded implementation cost for a
permanent external dependency on two moving things.

> **This note therefore restates `rust-interop.md` §8's ask 5 rather than
> answering it, with the price changed.** The decision belongs to
> `stdlib-standard.md`. What changes is the number it should weigh: not "350 lines
> of which 215 are Rust, forever", but "15 policy entries, a regeneration step, and
> a nightly toolchain in the toolchain's own build".

---

## 8. Nightly, priced

rustdoc JSON is nightly-only and unstable: `-Z unstable-options
--output-format json`. That is a real dependency for a compiler whose core spec
§12 names environment reproducibility as *"the single most-cited reason scientists
trust results across machines"*. It must be priced, not waved at.

**The format moves.** The document carries a `format_version` integer — **61** as
measured here, on `rustc 1.100.0-nightly (215a8af4b 2026-09-15)`. It is bumped on
breaking changes with no deprecation window and no compatibility shim: fields are
renamed, variants are added to the `Type` union, and consumers break at the
`serde` layer rather than at a place that explains itself. The `rustdoc-types`
crate exists precisely because every consumer needs a pinned mirror of the schema,
and it publishes a new version per bump.

**The consequence is not "nightly is required to build a Science program".** It is
required to *regenerate a binding*, which is a different and much smaller
population.

> **Decision 15. rustdoc JSON is a **contributor** dependency, never a user one.
> Every generated artefact — the Rust shim, the Science declarations, the safe
> layer, `science_abi.rs`, the coverage report — is checked into the sidecar. A
> user building a Science program that consumes a bound crate needs stable cargo
> (`rust-interop.md` Decision 2's conditional dependency) and never needs nightly,
> never needs rustdoc, and never needs the target crate's source beyond what cargo
> already fetches.**
>
> **Rejected: running rustdoc during compilation.** §4.1, and it would convert a
> contributor dependency into a user one, which is the entire question.
>
> **Rejected: vendoring the rustdoc JSON documents into the sidecar** so that
> regeneration needs no nightly either. `regex`'s is 1.3 MB and `serde_json`'s
> 1.4 MB of uncompressed JSON; polars' workspace is an order of magnitude more.
> The checked-in *outputs* are a fraction of that and are the thing a reviewer
> reads.
>
> **Rejected: parsing the crate's source with `syn` instead**, which needs no
> nightly. It is the approach that looks like it avoids the problem and it
> reintroduces `rust-interop.md` §1.1's refusal in miniature: `syn` gives you the
> tokens, not the resolved types — a `pub use` re-export, a type alias, a
> `cfg`-gated item, a trait's associated type, or anything a macro generated is
> either wrong or absent. Rustdoc JSON is produced *after* name resolution, which
> is the whole of its value. For a workspace-split crate like polars, whose facade
> is almost entirely re-exports, `syn` would see nothing at all.
>
> **Cost, itemised.**
> - **A pinned nightly.** The toolchain repository gains a `rust-toolchain.toml`
>   naming one nightly date, used only by `sciencec foreign bind`. Pinning is
>   mandatory, not advisory: an unpinned nightly makes the generator's output a
>   function of the day it ran.
> - **`format_version` is checked and refused, not coerced.** A document declaring
>   another version is `SC0481`, with the pinned nightly named in the message.
>   Best-effort parsing of an unknown version is the failure mode where a renamed
>   field silently becomes `None` and an item silently leaves the coverage report.
> - **A format bump costs a day of work and a regeneration of every curated
>   binding**: bump the pin, update the schema mirror, re-run the generator over the
>   curated set, diff the coverage reports. The coverage report is what makes the
>   diff reviewable.
> - **The bump is not on anyone's schedule but rustc's**, and the project can defer
>   it indefinitely because the pinned nightly keeps working. The only forcing
>   function is wanting to bind a crate that uses a language feature the pinned
>   nightly cannot document.
> - **One thing this does not cost: the user's build.** A downstream scientist on a
>   cluster login node with stable Rust, or with no Rust at all if the binding is
>   toolchain-level per `rust-interop.md` Decision 2, is unaffected by every line of
>   this section.

**The cheap answer is the right one here, and it is worth saying why rather than
just taking it.** Checking generated artefacts in is `ffi-c-boundary.md` §7.2's
existing rule and `rust-interop.md` §4.5's existing rule for `science_abi.rs`;
this note adds a third customer to a practice the project already has.

---

## 9. Diagnostics allocated

This note claims **`SC0480`–`SC0489`** from the codegen range. The README records
`SC0480`–`SC0499` as free; this note takes the first ten and leaves
`SC0490`–`SC0499` free.

| Code | Meaning |
|---|---|
| `SC0480` | `sciencec foreign bind` could not obtain rustdoc JSON: no nightly toolchain, `--output-format json` rejected, or `cargo rustdoc` failed. Names the pinned nightly and the exact command (§8) |
| `SC0481` | The rustdoc document's `format_version` is not the one this `sciencec` understands. Names both versions and the pinned toolchain; never attempts a best-effort parse (§8) |
| `SC0482` | A policy entry names a Rust path that is not in the crate's public API — typically a stale entry after a version bump. Names the entry's line in `manifest.toml` (§4.2) |
| `SC0483` | An item in the `include` list needs a policy the file does not supply. Names the item, the policy kind required, and a skeleton entry to paste (§4.5, Decision 9) |
| `SC0484` | *Warning.* The coverage report is absent or stale with respect to the generated sources (§4.5) |
| `SC0485` | A declared generic instantiation does not satisfy the bound. Discharged by rustc against the generated shim and re-rendered against the policy entry's span, per `rust-interop.md` §4.6 case (ii) (§3.1) |
| `SC0486` | A generated borrowed-field handle produced a region error in user code. Re-rendered against the *user's* span with the generated type's borrows reconstructed, per `ffi-c-boundary.md` §2.4's diagnostic obligation (§3.3) |
| `SC0487` | The generated sources are stale with respect to the crate version in `Cargo.lock` or the policy file's hash. Names `sciencec foreign bind` as the fix (§4.1) |
| `SC0488` | A policy entry asks to box a value one of whose lifetime parameters cannot be bound to an input. Names the lifetime and why (§3.3, Decision 3) |
| `SC0489` | A sidecar crate holds a thread pool that is not declared in the manifest's `threads` section, or is declared with no setter. Names the crate, the detected runtime, and `threads = "unmanaged"` as the explicit escape (§5.5, §5.6) |

`SC0486` is the one to get right, because it is the only code in this note that a
user meets while writing ordinary Science rather than while running a tool, and
because `ffi-c-boundary.md` §2.4 predicts its errors will be *"correct and
initially baffling"*. The message must reconstruct the whole chain: this handle
borrows that `Pattern` and that `String`, because the Rust type it wraps carries
two lifetimes, which the crate binds to these two parameters — and it must say
that the type was generated, and from where.

`SC0489` is arguably `native-dependencies.md`'s rather than this note's, since the
thread budget applies to C libraries as much as Rust ones and that note owns
`SC0471`–`SC0479`. It is placed here because the check that raises it runs in the
sidecar build. If that note prefers to own it, this note does not resist.

**An allocation hazard, flagged not fixed.** `rust-interop.md` §7 already records
a live three-way overlap in `SC0450`–`SC0479` between `ffi-c-boundary.md` §8,
`python-interop.md` §9 and itself, and `c-binding-coverage.md` uses `SC0498`,
which is inside the block the README lists as free. `SC0480`–`SC0489` is outside
every declared sub-range and outside every named code in all four notes, but
`SC0490`–`SC0499` is now contested and the next editor should reconcile it.

---

## 10. What this note asks of the others

1. **`rust-interop.md` §6** — amend the "Automatic binding generation" row to
   Decision 14's narrowed form, and change its "Nearest affordable thing" column
   from *"a hand-written shim crate per library"* to *"a generated shim crate plus
   a hand-written policy file, or an interchange-format binding where one exists"*.
   Leave §2.3's "Zero" untouched; it is correct and §2.2 confirms it.
2. **`rust-interop.md` Decision 1** — *"a hand-written shim crate as the only
   binding mechanism"* becomes *"a generated shim crate, from rustdoc JSON and a
   declared policy, as the binding mechanism"*.
3. **`rust-interop.md` Decision 7** — narrow per §3.3, with `SC0488` as the refusal
   when a lifetime cannot be bound.
4. **`rust-interop.md` §6's open item on polars and rayon** — answered by §5.5 and
   §5.8. That note's live tension can be closed, citing Decision 12.
5. **`rust-interop.md` §4.4** — `sciencec foreign bind` joins `foreign init` and
   `foreign sync`, with `SC0487` mirroring `SC0465`.
6. **`rust-interop.md` §3.5 and §4.1** — the sidecar manifest gains `[bind.*]` and
   `[[bind.*.threads]]` sections. Extensions of the file Decision 5 already
   introduced, not a second file.
7. **`stdlib-shape-and-packages.md` §3.6** — adopt Decision 12's narrowing. This is
   the ask with the widest blast radius and the one this note is least entitled to
   make alone: it changes a policy decision that note made for cluster-specific
   reasons. The argument offered is that the prohibition, read as binding on linked
   libraries, already excludes OpenBLAS and therefore `linalg`, and that a rule
   nobody can enforce is worse than a narrower one that is enforced. That note owns
   the decision.
8. **`ffi-c-boundary.md` §2.7 item 10** — **correct the stated reason, keep the
   conclusion.** OpenBLAS and MKL do *not* confine their threads to the call; they
   hold persistent pools, which is why their `set_num_threads` functions exist and
   why `native-dependencies.md` §2.1's oversubscription bug is chronic rather than
   transient. The item's conclusion — harmless — is right, and the correct reason
   is that each library holds **one process-wide pool whose size is settable**, so
   the cost is `N + M` rather than `N × M` (§5.5). The reason as written is
   load-bearing in `rust-interop.md` §6's open question, which is why the question
   looked unresolvable.
9. **`ffi-c-boundary.md` §7.3** — say that rustdoc JSON is the best-available
   machine-readable interface description for a Rust crate, as Fortran `INTENT` is
   for LAPACK, and that §7.1's conservatism requirement is explicitly *not*
   inherited where the source carries the contract. Without that sentence,
   Decision 7 reads as a contradiction of §7.1 rather than as its §7.3 exception.
10. **`ffi-c-boundary.md` §2.4** — confirm that a handle type may carry `borrowed`
    fields *and* implement `Drop`, which Decision 3 requires and which the
    `FftwPlan` example implies but does not state.
11. **`ffi-c-boundary.md` §2.6** — confirm that the `from` elision rule's exclusion
    of the `self` case costs nothing for generated declarations, since the generator
    always emits `from` explicitly.
12. **`data-io.md` §9** — confirm that the `Frame` Arrow-adoption path is
    reusable by a Rust sidecar and not specific to the Arrow C++ Parquet reader.
    Decision 10 depends on it entirely, and if the adoption path is written against
    `libarrow` specifically rather than against the C Data Interface, §5.2's
    marginal cost claim is wrong.
13. **`native-dependencies.md`** — the provider manifest gains a `threads`
    descriptor per native library: the setter symbol, whether it must be called
    before first use, and whether the count is changeable later (§5.5). It should
    also decide whether `SC0489` belongs in its range rather than this one.
14. **`stdlib-standard.md` §9.3** — §7.2. The ask is `rust-interop.md` §8's ask 5
    with the price restated; the decision stays with that note.
15. **The README** — add a row for this note in the notes index and in the
    allocation table, claiming `SC0480`–`SC0489`, and note that `SC0490`–`SC0499`
    is now contested by `c-binding-coverage.md`'s `SC0498`. This note does not edit
    the README because sibling notes are being edited concurrently.
16. **The standing ask for a derive mechanism** — a fourth customer and, for the
    first time, a measured price: 12 of `serde_json`'s 139 user-facing functions,
    8.6%, and they are the twelve on the crate's front page.
17. **No new `ffi` items are requested**, and no grammar change. Stated positively,
    as `rust-interop.md` §8 item 11 does, because it is the evidence that this note
    is a tool and a policy narrowing rather than a language change.

---

## 11. Risks

**The coverage number will be quoted without its denominator.** Every table in §2
gives three. A number quoted alone is a number that will be wrong within one
retelling, and the correction sounds like a walk-back.

**The polars coverage number is the most misleading number in the note, and it is
in the note.** §2.3 reports 95.1% for a crate whose bound methods do nothing
without `Expr` values the user cannot construct — 2,198 of its 2,486 functions
have no type parameter at all and are pure handle-shuffling. §2.3's own closing
paragraph, §2.5 item 5 and §5.1 all say so, and a reader who skims will still
quote the table.

**The missing fraction is never a random sample.** 91.4% of `serde_json` sounds
like a good tool; the 8.6% is `to_string` and `from_str`. A user's first ten
minutes are spent entirely inside the unreachable part.

**A generated safe layer invites more trust than a wall of `unsafe extern`
declarations, and is exactly as trustworthy as the lowering table.**
`ffi-c-boundary.md` §7.1's posture has the virtue that it *looks* untrustworthy.
Decision 7 gives that up deliberately, and none of its mitigations catches a
systematic error in the generator's own lowering of, say, `&mut [T]`.

**`rust-interop.md` §9's "the assertions check agreement, not correctness" gets
worse.** With a generated shim, the *same generator* produces the Rust shim, the
Science declaration and the assertion that checks them against each other — three
artefacts from one source of error, all agreeing. The defence is unchanged and now
load-bearing: a layout test suite against a known-good C compiler.

**The thread-budget narrowing is a policy change made by the wrong note.** §5.5
changes `stdlib-shape-and-packages.md` §3.6 on the strength of an argument about
OpenBLAS. If that note disagrees, polars is blocked again and so, on a strict
reading, is `linalg`. Ask 7 is the highest-priority item in §10 for that reason.

**Decision 13's partition is static for polars and the user cannot see it.** The
choice between `budget = "science"` and `budget = "library"` is made once, in a
manifest, and both settings are correct; the wrong one is a factor-of-`N`
slowdown with no error and no diagnostic. This is the most likely way a Science
polars program disappoints someone without anyone finding out why.

**The obvious thread mechanism does not work and looked like it did.** This note's
own first draft specified `ThreadPoolBuilder::build_global()` for polars, and
`rust-interop.md` §6's framing of the question implies the same. Polars does not
use rayon's global pool. Anyone reasoning about a rayon-based crate from the
outside will make this mistake, and the general lesson — that "uses rayon" does
not tell you which pool — should be in the manifest documentation.

**A panicking rayon worker aborts the process** (§5.7). No fix is available under
`rust-interop.md` Decision 6, nothing detects it, and the first time it matters
will be an eight-hour job.

**Decision 11's SQL escape will be mistaken for the whole binding.** "Polars
works" and "polars works through SQL" are different claims, and the second will
compress into the first.

**The nightly pin will rot quietly.** Nothing forces a bump, which is a feature
until the day a crate worth binding uses a construct the pinned nightly cannot
document — at which point the bump is urgent and touches every curated binding.

**Decision 3's borrowed-field handles will produce region errors in generated
types that users did not write.** When a tool wrote the type, the user cannot read
the definition to work out why. `SC0486` is the entire mitigation.

**The policy file is a Rust artefact in TOML clothing.** The claim that a user no
longer writes Rust is true and narrower than it sounds: they no longer *write* it,
they still *read* it. A Science user with no Rust cannot bind a new crate, which is
why `rust-interop.md` §6's curated set remains the real answer for most users.

**Four crates is a small sample.** `ndarray`, `nalgebra` and `rayon` are
unmeasured, and scikit-learn — named alongside polars by the project owner — is not
a Rust crate at all and is `python-from-science.md`'s problem, not this note's. A
reader could take this note as covering it. It does not.

**The polars measurement is of `polars-core` and `polars-lazy`, not of `polars`.**
The facade crate's own index is nearly empty because it is re-exports, so the
denominators here are the workspace crates a binding would actually target. A
future audit that runs rustdoc on the facade and reports a small number will be
measuring the wrong thing, in the opposite direction from §2.3's error.

---

## 12. Summary of decisions

| # | Decision | Alternative rejected | Reason | Cost |
|---|---|---|---|---|
| 1 | Generic instantiation by enumerated tuples; closed sets may be expanded | A general cross product; inferring from doctests; refusing generics | A cross product emits eight unused untested symbols for one wanted; doctest inference makes the ABI a function of doc comments | A policy line per instantiation; argument-encoded symbol names |
| 2 | Open-world trait bounds are unreachable and reported, not faked | One instantiation per Science type; silently substituting the dynamic-value path | It is a derive mechanism in disguise, and it makes the sidecar rebuild when a Science type gains a field | `serde_json`'s twelve headline entry points are not generated |
| 3 | Narrow `rust-interop.md` Decision 7: box a lifetime-parameterized value iff every lifetime binds to an input and becomes a `borrowed` field per `ffi-c-boundary.md` §2.4 | Keeping the flat ban; boxing with erased lifetimes plus a documented discipline | The obligation rustc gives up is taken up by Science's region engine, the same trade `from p` already makes; rustdoc JSON supplies the binding mechanically for 204 of 204 measured cases | Handles are not `'static`-shaped; baffling region errors in generated types |
| 4 | `Iterator` impls are bound as batch-fill calls | A per-element `next` shim; materializing the whole iterator | Per-element crossing is the mistake `ffi-c-boundary.md` §5.2 prices; materializing is unbounded memory | A batch size knob inside generated code |
| 5 | `# Panics` doc sections are advisory in the report only | Refusing to bind them; wrapping in `catch_unwind` | Prose is not a contract, and under `panic = "abort"` there is nothing to catch | An advisory that may be stale |
| 6 | `sciencec foreign bind` runs out of band; outputs are checked in | Invoking rustdoc during compilation; a separate binary | `ffi-c-boundary.md` §7.1's three reasons survive the move to Rust; layout agreement requires one binary | A subcommand, an artefact, a staleness check |
| 7 | The generator emits the safe Science layer, not only the unsafe declarations | `ffi-c-boundary.md` §7.1's maximal conservatism; defaults for uncovered policy | That rule's premise — the source has thrown the contract away — is false of rustdoc JSON, per §7.3 | A large trusted diff; generated names are worse than chosen ones |
| 8 | The unreachable surface is a required, checked-in report | A summary percentage; failing on any unreachable item; a warning per item | The missing fraction is not a random sample; every crate has unreachable surface; 279 warnings train users to silence them | Another file with a large version-bump diff |
| 9 | The generator never invents a policy; a missing one is `SC0483` | A default per policy kind | Every default is right often enough to be trusted and wrong silently | Binding a crate is an iterative loop |
| 10 | Where a crate publishes a stable C-ABI interchange format, bind the format, not the crate | Generating a binding to polars' `DataFrame`/`Series` API; exchanging through Arrow IPC files; treating it as a polars special case | An enormous pre-1.0 surface, a duplicate `Frame` type, and a copy at every crossing; and the rule already has three instances in sibling notes | Applies only where a format exists; buys data movement, not operations |
| 11 | Polars operations are reached through its SQL front end plus a fixed frame-level set, not a generated `Expr` binding | Generating the `Expr` builder; a Science expression language lowering to `Expr`; SQL only | Several hundred symbols on a pre-1.0 semver, each needing a safe wrapper; the language feature is out of scope | "Polars through SQL" is materially less than polars; errors are strings |
| 12 | `stdlib-shape-and-packages.md` §3.6 narrows: Science creates no pool; a linked library may hold a process-wide **singleton** pool if declared and sized from Science's budget at a declared init point | Keeping the prohibition; allowing pools with no coordination; Science as every library's executor; requiring a runtime setter | The prohibition read literally excludes OpenBLAS and therefore `linalg`; a singleton costs `N + M`, not `N × M`; no common executor interface exists; requiring a setter would refuse polars, whose pool has none | A `threads` descriptor with a `kind` field in `native-dependencies.md`; a runtime ordering constraint; heuristic detection of transitive pools |
| 13 | The budget is partitioned: **dynamically** for setter-kind libraries (collapse to 1 inside a Science parallel region), **statically at bind time** for env-at-init libraries like polars | Leaving it to the user; re-`set_var` for an already-built pool; dividing `N/k`; refusing env-at-init libraries | `polars_core::POOL` is a `LazyLock` built once with no resize path, so a dynamic partition for it would be a mechanism that does nothing | A program using both Science parallelism and polars must choose once, at build time, which gets the cores — a factor-of-`N` performance bug with no diagnostic if chosen wrong |
| 14 | `rust-interop.md` §6's refusal is **narrowed**: §2.3's zero confirmed, §6's "impossible" restricted to bindings needing no declaration | Overturning it; confirming it unchanged | Bucket A is 0 as that note says; B+C is 100% and 91.4%, which that row denies | A clean refusal becomes a refusal with two exceptions, and a tool to maintain |
| 15 | rustdoc JSON is a contributor dependency; every artefact is checked in; `format_version` is pinned and refused, not coerced | Running rustdoc at compile time; vendoring the JSON; parsing source with `syn` | Users must never need nightly; `syn` gives tokens, not resolved types, and would see nothing at all in a re-export facade like polars | A pinned nightly, a schema mirror, a regeneration per format bump |

---

## 13. Open questions

1. **Does `stdlib-shape-and-packages.md` accept Decision 12?** Everything about
   polars depends on it, and so — on a strict reading of the current §3.6 — does
   `linalg`. This is the single question whose answer changes the most.
2. **Is `data-io.md` §9's `Frame` adoption path written against the Arrow C Data
   Interface or against `libarrow`?** Decision 10's "marginal cost close to zero"
   claim holds only for the former.
3. **What happens to a panicking rayon worker** (§5.7)? The options are accept,
   fork the panic policy per sidecar, or wait for unwinding panics. This note
   accepts and is not confident that is right.
4. **How should a user say "this parallel region should let polars use the whole
   budget"?** Decision 13 makes the split static for polars because the pool
   cannot be resized, and the right dynamic answer is probably a scope annotation
   plus a library that supports resizing — neither of which exists. This is
   `stdlib-standard.md` §10's to design and polars' upstream to enable.
5. **Should Science ask polars upstream for a resize path?** One public function
   that rebuilds or resizes `polars_core::POOL` would convert Decision 13 from
   static to dynamic for the most important crate in the set. It is a small
   upstream ask and it is the only item in this note whose fix lies outside the
   project.
6. **`ndarray`, `nalgebra` and `rayon` are unmeasured.** An API generic over element
   type *and* dimensionality is where Decision 1's closed-set exception will first
   be tested against a real need — and polars' arity-7 signatures suggest the
   answer will look more like polars' than like `regex`'s.
7. **Who owns the pinned nightly and the schema mirror?** §8 prices the bump at a
   day and names no owner, which is how `rust-interop.md` §9's *"publishing the
   curated set without publishing who owns it"* risk reproduces itself one level
   down.
8. **Should the generated safe layer be a separate, optional output?** A project
   that wants the `ffi-c-boundary.md` §7.1 posture should be able to have it, and a
   flag is cheap; the argument against is that two supported postures is two things
   to test.
9. **What is the right unit for a coverage claim?** §2's three denominators are all
   defensible and this note declines to pick one for public use. Someone will have
   to.
