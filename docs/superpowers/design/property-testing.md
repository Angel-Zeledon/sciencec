# Science — Design: the `property` item

Date: 2026-09-18
Status: draft for review
Owns: the `property` item form, the `Arbitrary` interface, the shrinking
contract, and the rule that binds a failing case to a reproducible seed.
Extends, and does not re-decide: `stdlib-standard.md` §7, which owns the `test`
item, the `testing` module and `SC0269`. **Everything that note decided about
tests holds here unchanged** — this note adds a second item form beside `test`
and reuses its runner, its report and its `check*` vocabulary wholesale.
Also depends on: `reproducibility.md` §5.2 (the run record, which §6 adds one
field to), `stdlib-standard.md` §5 (`random`, `Key`, and the counter-based
generator), `effects.md` §4.1 (`pure def`, which §4 requires of a generator),
`collections-and-chains.md` §3 (naming), `llm-ergonomics.md` §1 (diagnostics as
the teaching channel).
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0161`–`SC0162`** (syntax) and **`SC0580`–`SC0589`**
(types). Free at the time of writing, checked by recomputing `README.md`'s free
table against `grep -rhoE 'Code\([0-9]+\)' crates/*/src/` rather than by reading
it — see §9.

---

## 0. The verdict up front

A `test` states one case. A scientist's code is almost never wrong on the case
its author thought of; it is wrong on the empty input, the singleton, the value
that is exactly at the boundary, the negative weight, the duplicated row. The
`test` item as `stdlib-standard.md` §7 designed it is correct and is not
sufficient, and the gap is a known and solved one: QuickCheck solved it in 2000
and every language has copied it as a *library*.

> **Decision 1. Science copies it as an item form, for exactly the four reasons
> `stdlib-standard.md` §7.2 gave for `test`, and adds one that is specific to
> this project: a failing property is worthless unless it reproduces, and only
> the compiler-plus-runner knows the seed.**

```science
property "sorting preserves length" for xs: Array of Int:
    let ys be sorted(xs)
    testing.check_equal(ys.length(), xs.length())
```

## 1. Why this is not a library

Rust, Python and Haskell all ship property testing as a library, so the burden of
proof is on the item form. §7.2 of `stdlib-standard.md` already made four of the
five arguments and they transfer without modification: the name is a sentence and
not an identifier; the item does not enter the release binary; the compiler knows
it is a test, which is what makes `testing.check*` legal inside it under
`SC0269`; and each item is an entry point, so region inference has a root.

The fifth argument is this note's own and it is the one that decides it.

**A property that fails and does not reproduce is not a test result, it is a
rumour.** The whole value of the construct is that the runner generated an input
the author did not think of; the whole cost is that the author has never seen
that input. Every library implementation bridges this by printing a seed and
asking the user to paste it into a configuration file or an environment variable.
That is a manual channel, and this project has a note — `reproducibility.md` —
whose entire thesis is that manual channels for reproducibility do not hold.

Science already has the machinery: `stdlib-standard.md` §5 makes the generator
counter-based and seeded by an explicit `Key`, and `reproducibility.md` §5.2
stamps a run record onto outputs. A property's seed belongs in that record, and
putting it there requires that the runner — not a library the user called — own
the seed. §6 does this.

## 2. The grammar

```
property-item := "property" string-literal generator-clause? block
generator-clause := "for" binder ("," binder)*
binder := identifier ":" type ("where" expression)?
```

`property` is followed by a string literal, which no other item form is, so it
is **contextual rather than reserved**, at the one-match-arm cost
`reserved-words.md` §3.2 prices. This matters more than it did for `test`:
`property` is an ordinary word in chemistry, materials science and optics — a
*physical property* — and `let property be …` must keep compiling. It does.

> **Decision 2. `property` is a contextual keyword, not a reserved word. It is
> recognised only at item position and only when the next token is a string
> literal. `reserved-words.md` §2's list is unchanged and §13 gains nothing.**

This is the opposite call from `test`, which `stdlib-standard.md` §7.3 left to
§13's owner while observing that the contextual version was affordable. The two
should be decided together and this note recommends contextual for both, for one
reason that note could not have weighed: *two* item forms taking reserved words
to get tests is a worse trade than one, and the audience-vocabulary collision is
materially worse for `property` than for `test`.

### 2.1 The `where` clause filters, and is bounded

```science
property "log of a positive is finite" for x: F64 where x > 0.0:
    testing.check(x.log().is_finite())
```

A `where` clause **discards** generated inputs rather than constraining the
generator. That is the cheap implementation and it has a failure mode — a filter
that rejects almost everything makes a property that silently tests nothing,
which is `stdlib-standard.md` §7.2's own worst failure mode wearing a different
hat. So:

> **Decision 3. A `where` clause that rejects more than 90% of a generator's
> output over the first 1,000 draws fails the property with `SC0580` rather
> than running it.** The message names the clause and offers the fix, which is
> to generate the constrained thing directly.

`x: F64 where x > 0.0` is the case that motivates the diagnostic and also the
case where the fix is real: `x: Positive of F64` generates only positives and
rejects nothing. §4.2 gives the type.

## 3. Decision 4 — the shrink is part of the contract, not a quality of the library

> **Decision 4. `Arbitrary` requires both `generate` and `shrink`. A type that
> can be generated and not shrunk is not `Arbitrary`.**

Every property-testing library treats shrinking as optional and a nicety. It is
neither. The raw counterexample for a list property is typically forty elements
of seven digits each, and it teaches nothing; the shrunk one is `[0, 0]` and it
names the bug. A property tester without shrinking produces work rather than
answers, and making the method optional is how a library ends up with a dozen
types that have it and a dozen that do not.

```science
interface Arbitrary:
    def generate(key: borrowed Key, size: Int) -> Self
    def shrink(self) -> Array of Self
```

`shrink` returns candidates that are *strictly smaller* by a well-founded order
the implementation chooses; returning `self` in the array is a non-terminating
shrink and is the one thing the runner checks for (`SC0581`, at runtime, with the
type named). An empty array is the correct answer for an atom and costs nothing.

## 4. Decision 5 — generators are `pure def`, and the size parameter is not a suggestion

> **Decision 5. `Arbitrary.generate` is declared `pure def`. A generator that
> reads the clock, the environment, the filesystem or the network is `SC0582` at
> the implementation.**

This falls straight out of `effects.md` §4.1 and costs this note nothing to
build: the bit is inferred already and `pure def` is the existing declaration
that pins it. It buys the thing the whole note is for. A generator whose output
depends on `time.now()` produces a counterexample that cannot be replayed from a
seed, and the failure is invisible — the property passes for a year and then
fails once on a machine in another timezone.

`key: borrowed Key` is `stdlib-standard.md` §5's counter-based key and not a
mutable RNG handle, which is what makes `generate` purely a function of
`(key, size)` and therefore replayable by construction.

### 4.1 `size` and the growth schedule

`size` runs from 0 to 100 across a run, and a generator is obliged to be
*monotone* in it only in the informal sense that bigger `size` should mean
bigger structures. This is not checked and cannot be. What is fixed is the
schedule — the runner draws at sizes `0, 1, 2, 3, 5, 8, 13, …` capped at 100 —
because an unfixed schedule is a second channel through which a rerun differs
from a run, and `reproducibility.md` §3 exists to close those.

### 4.2 The library's generators

`testing` gains `Positive of T`, `NonZero of T`, `InRange of (T, LO, HI)` and
`Sized of (T, N)` — four wrappers, each a record with one field and a `.value()`,
each `Arbitrary`. They exist because §2.1's diagnostic needs somewhere to point,
and the list is closed for the same reason `collections-and-chains.md` §1 closes
the chain vocabulary.

Implementations are provided for the primitives, `String`, `Array of T`,
`Map of (K, V)` and tuples up to arity 8. **Not** for `Frame`, `File`, `Path` or
anything in `data-io.md`: a generated filesystem is a fixture, not a property,
and the note that owns fixtures is `stdlib-standard.md` §7.

## 5. Decision 6 — the counterexample is written down, and it becomes a `test`

> **Decision 6. A failing property writes a regression file next to the source,
> and the runner replays every case in it before generating anything new.**

```
tests/regressions/sorting_preserves_length.science
```

The file is **generated Science source** — a `test` item with the shrunk input
written as a literal — and not a serialised blob:

```science
# Generated by `sciencec test` on a failing property. Safe to edit, move, or
# keep after the property is deleted. Seed: k:7f3a1c04e7b2, size 13.
test "sorting preserves length — regression from 2026-09-18":
    let xs be [0, 0]
    let ys be sorted(xs)
    testing.check_equal(ys.length(), xs.length())
```

Three things follow, and they are the argument for generating source over a blob:
the file is reviewable in a pull request; it survives the deletion of the
property that produced it; and it needs no format, no parser and no
canonicalisation surface — which is the warning `package-manager.md` §2.2 gives
about inventing one. Hypothesis's `.hypothesis` database is the alternative and
it is a cache directory that is routinely `.gitignore`d, which means the
regression is lost exactly when the CI machine is not the author's machine.

The cost, stated: a generated file in the source tree is a file someone must
review, and a property that fails on a hundred distinct shapes generates a
hundred of them. The runner caps the file at the **eight** most recently shrunk
distinct cases and says so in the header.

## 6. Decision 7 — the seed is a field of the run record

> **Decision 7. `reproducibility.md` §5.2's run record gains one field,
> `test.property_seed`, a 16-hex-character string. `sciencec test --seed <k>`
> reproduces a run exactly, and the value printed on failure is that string.**

This is the field the whole note exists to earn and it is one line in a JSON
object that already exists. It is a string, not a float, so Decision 10's
no-floats rule is unaffected; it is absent from the record of a non-test run.

`--deterministic` (`reproducibility.md` §4) and property testing interact in
exactly one way and it is worth stating: a property run is *not* deterministic
across runs by default, because the seed is drawn from entropy, and that is
correct — a property tester that draws the same thousand inputs every night is a
slow `test`. `--deterministic` forces the seed to a constant and the runner says
so in its report, so a reader cannot mistake one for the other.

## 7. What this does not do

Three limits, so no reader has to discover them:

- **No stateful or model-based testing.** A property over a sequence of
  operations against a reference model is the other half of QuickCheck and is
  where most of its remaining value is. It needs a command type, a shrinker over
  sequences and a notion of a model, and it is a second note. Nothing here
  precludes it.
- **No coverage guidance.** The generator does not know what the code did.
  Fuzzers do and it is why they find more; hooking one up is F2 and needs
  instrumentation this project has no design for.
- **No proof.** A property that passes ten thousand times is evidence. §1 of the
  core spec claims verification, and this note is not part of that claim — it is
  the part that catches what the type system cannot. `contracts.md` is the one
  that tries to discharge things statically.

## 8. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0161` | Syntax | `property` with no string literal, or a `for` clause with no binder |
| `SC0162` | Syntax | A `property` item nested inside another item |
| `SC0580` | Types | A `where` clause rejects more than 90% of draws (§2.1) |
| `SC0581` | Types | `shrink` returned a candidate that is not smaller (§3) |
| `SC0582` | Types | `Arbitrary.generate` is not pure (§4) |
| `SC0583` | Types | A binder's type does not implement `Arbitrary`, with the four wrappers offered |
| `SC0584` | Types | `testing.check*` called outside a `test` or `property` item — **an amendment to `SC0269`, not a new code**; see below |
| `SC0585`–`SC0589` | Types | Held for the stateful form of §7 |

`SC0584` is the one to look at. `stdlib-standard.md` §8.2 defines `SC0269` as
*"a `testing.check*` called outside a `test` item"*, and this note adds a second
legal context. **The right fix is to amend `SC0269`'s wording, not to add a
code**, per `README.md`'s convention that amending is fine and claiming is what
needs checking. `SC0584` is therefore **returned to the free pool** and this row
records the amendment instead.

## 9. On the free table

`README.md`'s free-code table warns that it is *"a cache with no invalidation"*
and tells a note to recompute it. Doing so found it stale a second time, in a
way that does not affect this note's allocation but should be recorded:
`SC0304` is shipped in `crates/science-regions` while the Ownership row offers
`SC0303`–`SC0329` as free, and `SC0538`–`SC0541` are shipped in
`crates/science-types` while the shipped row stops at `SC0537`. The Syntax and
Types ranges this note takes were free under both the recomputation and the
table.

## 10. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| Amend `SC0269` to permit `property` items | `stdlib-standard.md` §8.2 | One sentence |
| Add `test.property_seed` to the run record | `reproducibility.md` §5.2 | One field |
| Decide `test` and `property` as contextual together | §13's owner, via `reserved-words.md` | One decision, two match arms |
| `Arbitrary` implementations for the primitives | `stdlib-core.md` | Library work, F0-shaped |
| The runner learns a second item form and `--seed` | `stdlib-standard.md` §7.5 | The runner exists; this is an arm |

## 11. Risks

- **The 90% filter threshold in Decision 3 is a number chosen by taste.** It is
  the same number Hypothesis uses and the failure it prevents is real, but a
  legitimate property with an 8% acceptance rate now fails to run. The escape is
  to generate the constrained type, and §4.2 exists so that the escape is always
  available. If the threshold proves wrong it is a constant, not a design.
- **Generated regression files in the source tree will be resented** by someone.
  The cap of eight is the mitigation and it may be the wrong number.
- **Shrinking as a required method raises the bar on `Arbitrary`** and will make
  some user types not bother. That is the intended trade and it is the one
  decision here most likely to be argued.
