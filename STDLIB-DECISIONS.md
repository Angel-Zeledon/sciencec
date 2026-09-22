# Standard library decisions: three contradictions, resolved on paper

Scope of this note: the three contradictions named in the current session's
brief, blocking further work on `INTERFACE_DECLS` in
`crates/science-resolve/src/builtins.rs`. Each section gives the evidence
(quoted, with file:line citations), the recommended resolution, and exactly
what would have to change to implement it. Per the brief: a written decision
is the deliverable; no functional code changes are made for any of the three,
because none of the three is unambiguous *and* small *and* confined to
`crates/science-resolve`. One factual correction to a stale comment in
`builtins.rs` is made (§4) and is not a resolution of any contradiction.

All syntax below is written in the compiler's **current** syntax, confirmed
empirically against the built binary (`cargo build -p sciencec --features
llvm`, then `sciencec check`/`sciencec build` on hand-written snippets). Several
design notes still use an **old** borrow spelling the parser now rejects by
name — see §2.2 — and I have translated their intent to current syntax rather
than quoting the old spelling as if it still parsed.

---

## 1. `Display` and `Formatter`

### 1.1 The premise, checked

The brief states: *"`Formatter` is specified by no note and declared by no
prelude."* The second half is true; the first half is not.

`docs/superpowers/design/strings-formatting-and-docs.md` §3.1 (lines 486–538)
gives `Display` a full signature and gives `Formatter` a complete, closed
method set:

> ```science
> interface Display:
>     def display(self, into: mutable borrowed Formatter)
> ```
>
> `Formatter` is a new library type with a small closed method set, in the
> spirit of §8:
>
> ```science
> Formatter has:
>     def text(mutable self, value: borrowed String)
>     def raw(mutable self, value: borrowed String)
>     def number(mutable self, value: F64)
>     def integer(mutable self, value: I64)
>     def spec(self) -> FormatSpec
> ```

(`strings-formatting-and-docs.md`, lines 495–532.) It also specifies
`FormatSpec` as "a plain record of nullable fields — `fill: Char`, `align:
Align?`, `sign: Sign?`, `width: Int?`, `precision: Int?`, `code: Code?`,
`alternate: Bool`, `grouping: Grouping?` — with `Align`, `Sign`, `Code` and
`Grouping` as `choice` types" (same file, lines 534–536).

`docs/superpowers/design/stdlib-core.md` §0 and §6.1 name this note as the
**owner** of `Display`/`Formatter` and say explicitly it is "not reopened"
(`stdlib-core.md` line 727: *"All of it is Level 1 and none of it is
reopened."*). `stdlib-core.md` §4.5's own table lists `Formatter, FormatSpec
(from strings-formatting-and-docs.md §3.1)` as Level 1 types (line 547).

So the actual gap is not a missing note. It is that **`crates/science-resolve/
src/builtins.rs` never transcribed it**, and the comment explaining why is
itself wrong about the reason:

> "Writing `Ord.compare -> Ordering` or `Display.display(Formatter)` here
> would invent `Ordering` and `Formatter` — two Level 1 types no note has
> specified, arriving as a side effect of a bound check"
> — `crates/science-resolve/src/builtins.rs`, lines 424–428 (as of this
> session; see §4 below, where the `Formatter` half of this sentence is
> corrected).

`Ordering` really is unspecified anywhere in the design notes (grep across
`docs/superpowers/design/*.md` for `Ordering` and `interface Ord` turns up
nothing but the `T: Ord` bound). `Formatter` is not in that position — it has
a citable, complete signature. The comment conflates the two.

### 1.2 What the corpus does

Only one example implements `Display`: `examples/06_traits.science`, lines
185–191:

```science
# `Display` is what `print` requires, and it is the same interface F1's notebook
# rendering will extend (§5.4).
Vector2 implements Display:
    def display(self) -> String:
        let mutable out be String.new()
        out.push_str("a vector")
        out
```

(The brief also names `examples/00_kitchen_sink.science` as writing this
shape; checked directly — that file has no `Display` implementation at all,
by name or by grep. This appears to be inaccurate in the brief; noting it
rather than inventing a second citation.)

### 1.3 What actually happens today

Confirmed by reading (not editing) `crates/science-codegen-llvm/src/lower.rs`,
lines 199–201 and 269, 288:

> "`science-resolve`'s `builtins.rs` declares `Display` as an interface
> **with no methods**, and says why: naming `Display.display(Formatter)`
> would [invent Formatter] … `Display` has a method; `EXIT_CONTRACT.
> display_is_renderable` is `false`"

`INTERFACE_DECLS` in `builtins.rs` (lines 155–171, 436–594) confirms: `Display`
is one of the fourteen interfaces declared as "a name with implementations and
no methods." Since `Display` has no declared method, `print` never calls
`Vector2.display`; it only knows how to render primitives directly. **The
`display` method in `examples/06_traits.science` is dead code** — it
type-checks as an ordinary block member (because `implements Display:` with
no interface method has nothing to conform to), but nothing ever calls it.

### 1.4 Recommendation

**Adopt `strings-formatting-and-docs.md` §3.1 verbatim, translated to current
syntax**, i.e.

```science
interface Display:
    def display(self, into: &mut Formatter)
```

(`mutable borrowed Formatter` → `&mut Formatter`; see §2.2 for why.) This is
not a judgment call between two competing designs — it is the one note that
actually specifies the mechanism, cross-referenced by the note that governs
what's Level 1 (`stdlib-core.md`), against an example that predates the
decision and was never updated. `examples/06_traits.science`'s
`def display(self) -> String` is the thing that should change, not the spec.

### 1.5 What it would cost to implement, and why it is not done here

This is **not confined to `crates/science-resolve`**, so it is not made under
the brief's rule (d):

- **`crates/science-resolve/src/builtins.rs`**: add `Formatter`, `FormatSpec`,
  `Align`, `Sign`, `Code`, `Grouping` to `LIBRARY_TYPES`/`CHOICES`; give
  `Formatter` a `BLOCKS` entry with its five methods; give `Display` (and
  `Inspect`, while at it — same shape, same note, §3.2) a declared method in
  `INTERFACE_DECLS`. This part alone is plausibly small, following the
  pattern the `Clone.clone` commit (`dbb7da4`) already established for one
  methodless interface.
- **But declaring `Display.display`/`Inspect.inspect` has the same
  side-effect `Clone.clone` had, multiplied**: `crates/science-types/src/
  methods.rs`'s `WHOLLY_OPEN` list had to grow by sixteen names (`Chars`,
  every numeric primitive, `Bool`, `Char`) the moment `Clone` got *one*
  method, because giving any builtin interface a method flips
  `surface_is_closed` for every type that implements it, converting every
  other silent `.foo()` on those types into `SC0532`. `Display` is
  implemented by *every* numeric primitive, `Bool`, `Char`, `String`,
  `IoError` and `TextError` (`IMPLEMENTS` table, `builtins.rs` lines 625–651)
  — declaring its method requires re-auditing every one of those heads for
  what silently resolves today and would stop resolving.
- **`science-rt`**: needs a real `Formatter` representation and the five
  entry points (`text`, `raw`, `number`, `integer`, `spec`) — none exist
  today (checked: `grep -rn "Formatter" crates/science-rt/src/` finds
  nothing).
- **`science-codegen-llvm`**: `print`'s lowering (`lower_print`,
  `lower.rs` line ~7820) special-cases known types today and has no call
  site for a user `Display` implementation at all; wiring one in is exactly
  the "Decision 11 lookup, recorded but not reachable by the backend" gap
  `NEXT-SESSION.md` names as blocker #2 for the whole corpus (unrelated to
  this note, but the same missing plumbing).
- **`examples/06_traits.science`**: needs its `display` body rewritten
  against the new signature. (Examples are outside my territory to edit for
  this task.)

Four crates, one runtime, and one example. Not a `science-resolve`-only
change under any reading of the brief's threshold.

---

## 2. `Eq` and `Ord`: is `other` borrowed?

### 2.1 The evidence, both spellings

Four corpus implementations exist, and they disagree:

```
examples/06_traits.science:180:    def eq(self, other: Vector2) -> Bool:
examples/06_traits.science:214:    def eq(self, other: Note) -> Bool:
examples/06_traits.science:218:    def less(self, other: Note) -> Bool:
examples/00_kitchen_sink.science:204:    def eq(self, other: Counts) -> Bool:
examples/19_stdlib.science:240:    def eq(self, other: &Record) -> Bool:
examples/19_stdlib.science:244:    def less(self, other: &Record) -> Bool:
```

Three implementations (`Vector2`, `Note`, `Counts`) take `other` **owned** —
no sigil at all. One (`Record`) takes it **borrowed** — `&Record`. This is a
real disagreement inside the corpus, not a spelling artifact: `Vector2` and
`Counts` both also `implements Copy`, so passing them by value is cheap and
harmless; `Note` does **not** implement `Copy` (only `Clone`), so
`def eq(self, other: Note)` would *move* `other` into every equality check —
a real ownership cost the `Record` version does not pay, since `&Record`
borrows.

### 2.2 No note gives `Eq`/`Ord` a method signature at all

Searched every file under `docs/superpowers/design/` and
`docs/superpowers/specs/` for `interface Eq`, `interface Ord`, `def eq(`,
`def compare(`, `def less(`: none exists. `builtins.rs`'s own
`INTERFACE_DECLS` comment (lines 416–430) confirms this is deliberate: `Eq`
and `Ord` are two of "the other fourteen" interfaces "declared as names with
implementations and no methods," specifically because `Ord`'s method "would
need an `Ordering`… two Level 1 types no note has specified" (this part of
the sentence *is* accurate for `Ordering`, unlike for `Formatter` — see §1.1).
So, strictly, there is no *spec* contradiction here — there is nothing to
contradict. The contradiction is entirely within the corpus, and it is real:
four call sites, two conventions.

### 2.3 The nearest precedent: comparison-like binary methods elsewhere in the design notes

Two notes give a binary method of exactly this shape — "compare or combine
`self` with one other value of the same type, without consuming either" — and
both borrow the second operand:

> `def atan2(self, other: borrowed Self) -> Self`
> — `docs/superpowers/design/intrinsics-math-physics.md:2993`,
>   `docs/superpowers/design/uncertainty.md:836` (identical signature, two
>   notes)

> `def union(self, other: borrowed Set of T) -> Set of T where T: Eq + Hash`
> — `docs/superpowers/design/stdlib-core.md:402`

Both are written in the **old** borrow spelling, `borrowed T`. The current
parser rejects that spelling by name:

> `"`borrowed T` is now written `&T`"` / `"`mutable borrowed T` is now written
> `&mut T`"`
> — `crates/science-parser/src/parser.rs:3405`, `:3403` (type position);
>   `:4135`, `:4145` (expression position, for `borrowed e`/`mutable borrowed
>   e`)

Confirmed empirically: `borrowed Record` is UNEXPECTED_TOKEN in the current
build; `&Record` is what `examples/19_stdlib.science` already writes and what
the parser accepts. So translating the two notes' intent into current syntax
gives `other: &Self` / `other: &T` — which is exactly `examples/
19_stdlib.science`'s `&Record`, and is *not* what `06_traits.science` or
`00_kitchen_sink.science` write.

### 2.4 Recommendation

**`other` is borrowed: `def eq(self, other: &Self) -> Bool`, `def less(self,
other: &Self) -> Bool`.** Three reasons:

1. It is the only convention with any precedent at all in the design notes
   (`atan2`, `Set.union`), once translated out of the old borrow spelling.
2. It is correct in general and merely *harmless* in the owned examples: for
   a `Copy` type (`Vector2`, `Counts`) owned-vs-borrowed is unobservable, but
   for a non-`Copy` type (`Note`) `def eq(self, other: Note)` silently
   consumes `other` on every comparison — a trap that sorts and searches
   would hit immediately (`xs.iterate().sorted(by: each)` calling `less`
   repeatedly on the same values cannot work if `less` consumes its
   argument).
3. `examples/19_stdlib.science`'s `&Record` is already the form that would
   satisfy this signature unchanged; the other three files would need a
   one-word edit each (`other: Vector2` → `other: &Vector2`, etc.).

### 2.5 What it would cost to implement, and why it is not done here

Declaring `Eq.eq`/`Ord.less` in `INTERFACE_DECLS` looks, superficially, like
the same small, self-contained move `Clone.clone` was (`dbb7da4`). It is not,
for two reasons specific to `Eq`/`Ord`:

- **The `Ordering` problem is real and unsolved.** `Ord` cannot get a
  faithful method today without inventing `Ordering` (§2.2) or committing to
  `less`-as-the-required-method with `<`/`>`/`<=`/`>=` all derived from it —
  which is a design decision (what does `F64`'s NaN do to a total order?)
  that belongs in a design note, not in a transcription pass. `builtins.rs`'s
  own comment already declines to make this call, correctly.
- **The `WHOLLY_OPEN` blast radius is bigger than `Clone`'s.** Every numeric
  primitive, `Bool`, `Char`, `String`, `IoError` and `TextError` all
  implement both `Eq` and `Ord` (or `Eq` alone) per the `IMPLEMENTS` table.
  Declaring either method flips `surface_is_closed` for all of them
  simultaneously — the same mechanism the `dbb7da4` commit had to patch for
  `Clone` (`methods.rs`'s `WHOLLY_OPEN` list, currently `Chars` plus sixteen
  primitive names) would need re-auditing again, for a different set of
  false positives.
- **It would also require fixing the corpus**, which is outside this
  session's territory (`examples/` is not `crates/science-resolve/` or a
  spec/notes/docs file), and three of four existing `eq`/`less`
  implementations would stop satisfying the interface the moment it gained a
  real signature.

Left alone. The corpus disagreement is named here so the next session does
not have to re-discover it.

---

## 3. `String` concatenation: `"a" + "b"` and `SC0535`

### 3.1 What actually happens, and why (traced, not guessed)

Verified empirically against the built compiler:

```
$ sciencec check concat_test.science
error[SC0535]: `String` does not implement `Add`
 --> concat_test.science:4:14
  |
4 |     let c be a + b
  |              ^^^^^ `+` needs `Add` and `String` has no implementation of it
```

The diagnostic's wording is misleading in isolation. `builtins.rs`'s
`IMPLEMENTS` table (line 645) **does** list `("String", &["Clone", "Eq",
"Ord", "Add", "Display"])` — `String` genuinely implements `Add` as a
*relation*. The reason `+` still fails is exactly named in `crates/
science-types/src/check.rs`, lines 5290–5296:

> "`"a" + "b"` is exactly this: `String implements Add` (§5.4) and `Add`
> declares no method — `INTERFACE_DECLS`' own comment says why — so nothing
> upstream of this line ever names the interface. Left unreported this used
> to become `Ty::ERROR` with no diagnostic at all, which is `sciencec check`
> exiting 0 on `"a" + "b"` and the build failing later with a generic
> `SC0400`."

So: `Add` is (like `Eq`, `Ord`, `Display`) one of the fourteen methodless
interfaces. `check.rs`'s binary-operator dispatch first tries the
named-method path (`Decision 11`'s lookup, for user types whose `+` maps to a
concrete `add` method); that has nothing to find, because `Add` has no
declared method anywhere — not just for `String`. It falls through to a
*structural* path meant for numeric primitives (unify both operand types,
accept if the result `is_operand_type`); `String` unifies with itself but is
not a numeric operand type, so this path reports `SC0535` rather than
silently producing `Ty::ERROR` — which is what commit `dbb7da4` (the
immediately preceding commit on this branch) deliberately changed, on
purpose, to turn a silent wrong-exit-code build into an honest compile error.
This is confirmed correct and current behaviour, not a bug to fix in
isolation.

Separately, and additionally: even if `Add` had a method and dispatch found
it, there is no runtime entry point to call. `crates/science-rt/src/
string.rs` has `science_string_push_str` (mutates in place, `&mut self`) and
no `science_string_concat`/`science_string_add` that allocates a *new*
`String` from two borrowed operands (checked: `grep -rn "concat" crates/
science-rt/src/string.rs` finds nothing; the nearest function, line
304–318, is explicitly the in-place append used by `push_str`, and its own
safety comment forbids aliasing the same string, which a `self + self`
concatenation would need to handle differently).

### 3.2 Recommendation

`String implements Add` should mean concatenation, and the current SC0535 is
the **correct interim behavior**, not a defect — it is honest about a real
gap rather than silently wrong. The path to making `"a" + "b"` actually work
is, in order:

1. A design decision for `Add`'s method signature and name (this note
   recommends following `stdlib-shape-and-packages.md` §4.5's Decision 4c,
   the same "operator trait's method takes the trait's own name in
   lowercase" rule `Clone`/`Index` already use, translated to current
   syntax: `def add(self, other: &Self) -> Self`).
2. Declaring that method in `INTERFACE_DECLS`.
3. Adding `science_string_concat` (or equivalent) to `science-rt`.
4. Wiring `science-codegen-llvm`'s method-dispatch table to route `String`'s
   `Add.add` to that entry point.

### 3.3 What it would cost to implement, and why it is not done here

This is the same shape as §1 and §2, with one **additional, more serious**
risk specific to `Add`: declaring `Add.add` in `INTERFACE_DECLS` would not
merely flip `surface_is_closed` for numeric primitives (the `Clone`
precedent's cost) — it risks **breaking numeric arithmetic outright**.
`check.rs`'s own comment (lines 5246–5249) states the current design in as
many words:

> "§6, closed: an operator on a user type is the matching interface's method,
> which is Decision 11's lookup. On the prelude's numerics it is not —
> nothing declares `I64.add` — so those fall through to the structural
> answer below."

That is: numeric `+` deliberately bypasses method dispatch today *because*
`Add` has no declared method, and falls through to the structural
`unify`-based path that has nothing to do with methods at all. If
`INTERFACE_DECLS` declared `Add.add`, the named-method lookup (`Decision
11`'s path, `self.operator(...)` in `check.rs`) would start finding a match
for `I64 + I64` too, and would try to dispatch `I64.add` as a method call —
a call site that does not exist in `science-codegen-llvm`'s runtime table
today (`RUNTIME` has no `science_i64_add`; integer addition is a native LLVM
instruction, not a runtime call). This is not a hypothetical: it is the exact
same class of regression the `Clone.clone` commit had to catch and patch for
(`WHOLLY_OPEN`), except where `Clone` merely made some `.foo()` calls newly
strict, a wrongly-declared `Add.add` would change what *every* numeric `+`
in the language compiles to. Declaring `Add`'s method safely would require
either:

- Special-casing numeric primitives in `check.rs`'s dispatch so they keep
  using the structural path even though `Add` now has a method (a
  `science-types` change, outside my territory and not confined to
  `science-resolve` regardless), or
- Giving every numeric primitive its own `add` in `BLOCKS` that routes to a
  native instruction rather than a runtime call — a `science-codegen-llvm`
  concern this crate cannot settle.

Left alone, for the same reason as §1 and §2: real, understood precisely, and
not a change any single crate — let alone `science-resolve` alone — can make
safely in isolation.

---

## 4. The one change made

`crates/science-resolve/src/builtins.rs`'s `INTERFACE_DECLS` doc comment
(originally lines 424–428, quoted in §1.1 above) asserted that `Formatter` is
"a Level 1 type no note has specified." That clause is factually wrong —
`strings-formatting-and-docs.md` §3.1 specifies `Formatter` completely — and
since this note now cites that comment as evidence, leaving the inaccuracy
in place would let a future reader repeat it. The comment is corrected to
name where `Formatter` *is* specified and to narrow the claim to `Ordering`,
which is genuinely unspecified. This is a **documentation-only** correction:
no `const`, no `struct`, no function signature, no test-observable behaviour
changes. No test is added for it, because a doc comment has no independently
checkable behaviour to pin — the correction is verified by re-reading the
cited note, which is quoted in full in §1.1 above.

This is not a resolution of contradiction §1; `Display` remains methodless,
`Formatter` remains undeclared, and `examples/06_traits.science`'s `display`
remains dead code, exactly as before this note.

---

## Summary

| # | Contradiction | Resolution | Code changed |
|---|---|---|---|
| 1 | `Display`/`Formatter`: note gives `display(self, into: &mut Formatter)`; corpus (`06_traits.science` only — not `00_kitchen_sink.science`, contra the brief) writes `display(self) -> String` | Adopt the note's signature; the example is outdated, not the spec. `Formatter` **is** specified (`strings-formatting-and-docs.md` §3.1) — the brief's premise that no note specifies it is incorrect, and `builtins.rs`'s own comment repeated that error. | No (comment-only correction, §4) |
| 2 | `Eq`/`Ord`: `other` owned (`06_traits.science`, `00_kitchen_sink.science`) vs. borrowed (`19_stdlib.science`) | `other: &Self`, following `atan2`/`Set.union` precedent translated out of the old `borrowed T` spelling. No note actually specifies `Eq`/`Ord`'s method (that part of `builtins.rs`'s comment is accurate), so this is a corpus-only disagreement. | No |
| 3 | `"a" + "b"` → `SC0535` | Correct and intentional current behavior (commit `dbb7da4`), not a bug. Fixing it needs a design decision for `Add`'s method, a new `science-rt` entry point, and `science-codegen-llvm` wiring — and risks breaking numeric `+` if done carelessly, per `check.rs`'s own comment on why numerics currently bypass method dispatch entirely. | No |
