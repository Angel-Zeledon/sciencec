# Science — Design: named axes

Date: 2026-09-18
Status: draft for review
Phase: **F1**, with `broadcasting.md`. What F0 must not preclude is in §9.
Owns: axis names in a tensor's type, the `over:` and `along:` reduction
arguments, name-directed alignment, and the rule that decides when two axes of
equal length are nonetheless different axes.
Extends, and does not re-decide: `broadcasting.md` — §2–§5 (the alignment
algorithm and the dotted operators), §6.2 (`with … as …`), §7 (reductions).
**Every decision in that note holds unchanged for unnamed axes, which remain the
default.** This note adds a name to a dimension and says what the name does.
Also depends on: `const-expression-arithmetic.md` §2.3 and §10.1 (the normal
form and type-level lists — this note is the third customer and extends neither),
`indexing-and-array-literals.md` §9 (slices), `scientific-libraries.md` §12
(units in the type, the closest existing precedent), `unit-literals.md` §4 (the
namespace pattern §2.1 copies), `reserved-words.md` (which records `axis`,
`dim`, `dims` as **free** and keeps them so), `llm-ergonomics.md` §1.
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0168`–`SC0169`** (syntax) and **`SC0635`–`SC0659`**
(types).

---

## 0. The verdict up front

`broadcasting.md` §0 says the productive idea in scientific computing is also one
of its most reliable sources of silent wrong answers, and takes the silence away
by making the programmer write a dot. This note makes the same argument about the
*second* such source, which that note's §1.1 example contains and does not name:

```science
let error be predict(weights, features) .- labels
```

Nothing here says which axis is samples and which is features. When both happen
to be 768 long, transposing one of them is not a type error, is not a shape
error, and is not a runtime error. It is a number, in a paper.

> **Decision 1. A dimension may carry a name. Two dimensions of the same length
> and different names do not align, do not broadcast, and do not unify. The name
> is part of the type and is checked at compile time exactly as the length is.**

```science
type Spectrum is Tensor of (F32, [wavelength: 2048, sample: 96])

let mean_spectrum be spectra.mean(over: sample)
```

## 1. Why a name is worth a type-system feature

`axis: 0` is the status quo of the entire ecosystem and it has three failures
that are not fixable by care:

1. **It is positional, so it is invalidated by any operation that reorders.** A
   transpose, a squeeze, a reduction — each renumbers every axis after it, and
   the `axis: 0` written three lines down still compiles.
2. **It is unreadable at review time.** Nobody can check `axis: 1` without
   reconstructing the whole pipeline in their head. `over: sample` is checkable
   locally, by someone who did not write it.
3. **It is unchecked at composition.** Passing a `(samples, features)` tensor to
   a function expecting `(features, samples)` type-checks whenever the two happen
   to be equal, and *silently produces the transpose of the right answer* when
   they are not equal in a way the broadcaster can stretch.

The third is the one that justifies a type-system change rather than a linter.
`xarray` solved this in Python and is beloved for it, and it cannot check
anything at compile time because Python cannot. Science has the shape in the type
already — `broadcasting.md` §2 put it there — so the name costs one more field in
a structure that exists, and buys a static check that xarray can only do at
runtime. **This is the clearest case in the corpus of Science being able to do a
known-good idea properly because of a decision it already made.**

## 2. The grammar

```
shape-list := "[" dim ("," dim)* "]"
dim := (identifier ":")? const-expression
```

A shape is already a type-level list (`broadcasting.md` §5,
`const-expression-arithmetic.md` §10.1). This note allows each element to carry a
label. Mixed shapes are legal and useful:

```science
Tensor of (F32, [batch: n, 3, 3])      # named batch, anonymous 3×3 matrix
```

> **Decision 2. The existing unnamed spelling `Tensor of (F32, (n, 768))` stays
> valid and unchanged, and is exactly equivalent to a shape whose every axis is
> anonymous. Named axes are opt-in and nothing in the corpus needs rewriting.**

This is the decision that keeps the note compatible. `broadcasting.md`'s examples,
`indexing-and-array-literals.md`'s, and every tensor signature in
`scientific-libraries.md` continue to mean what they meant.

### 2.1 Axis names are declared, not conjured

> **Decision 3. An axis name is an item, declared with `axis`, and resolved by
> the ordinary name resolver. A bare undeclared identifier in a shape position is
> `SC0635`, not an implicit declaration.**

```science
axis sample
axis wavelength
axis batch
```

The alternative — any identifier in a shape position declares an axis on first
use, as xarray's strings do — was rejected for the reason `reserved-words.md`
§0.1 gives about guessing: a typo becomes a new axis, and `wavelenght` silently
fails to align with `wavelength`. That is the exact bug class the note exists to
kill, reintroduced through the naming channel.

Declaring them also means axis names live in the module namespace, are exported
with `public`, and are imported with `use`, so two libraries agree on `sample`
only when they mean the same `sample`. This is `unit-literals.md` §4's namespace
pattern, copied deliberately.

**`axis` is currently a free word and `reserved-words.md` lists it among §13's
"best decision" to leave free.** This note needs it only in item position
followed by an identifier, so it is **contextual**, and `let axis be 0` keeps
compiling. The audit stands.

## 3. Decision 4 — what the name does to alignment

> **Decision 4. `broadcasting.md` §5's right-alignment algorithm runs unchanged
> over anonymous axes. When either operand has named axes, alignment is **by
> name** and position is ignored. A name present in one operand and absent in the
> other is a stretch candidate exactly as a length-1 axis is.**

This is the whole semantic content of the feature and it is short because the
existing algorithm does the work:

```science
let centred be readings .- baseline
#  readings: Tensor of (F32, [sample: 96, wavelength: 2048])
#  baseline: Tensor of (F32, [wavelength: 2048])
#  aligns on `wavelength` by name; `sample` stretches. Position never consulted.
```

Under `broadcasting.md` §5 alone this works by luck — the shapes right-align
correctly. Write `baseline` as `[sample: 96]` instead and positional alignment
still succeeds and computes nonsense; named alignment gives `SC0636`.

### 3.1 Two axes of equal length are different axes

```science
let g be gram .+ covariance
#  gram:       Tensor of (F32, [feature: 768, feature_t: 768])
#  covariance: Tensor of (F32, [feature: 768, feature: 768])
#  SC0636: axis 1 is `feature_t` on the left and `feature` on the right
```

Nothing but a name can catch this, and every square matrix in machine learning
has this shape.

### 3.2 Mixing named and anonymous is allowed, once

> **Decision 5. An operand whose axes are all anonymous aligns positionally
> against a named operand and **inherits** the names. An operand with *some*
> named axes aligns by name for those and positionally for the rest, and the two
> groups may not interleave — named axes bind leftmost. Interleaving is
> `SC0637`.**

The inheritance rule is what lets a literal and a library function that predates
this note be used with named tensors without ceremony. The no-interleaving rule
is what keeps the algorithm decidable without constraint solving, which
`broadcasting.md` §11 explicitly refuses to require.

## 4. Decision 6 — reductions take a name, and `axis:` stays

> **Decision 6. Reductions gain `over:` — the axis or axes consumed — and
> iteration gains `along:` — the axis walked. The existing `axis: 0` spelling
> remains legal for anonymous axes and is `SC0638` for named ones.**

```science
spectra.mean(over: wavelength)            # one axis
spectra.mean(over: (wavelength, sample))  # several
for row in table.along(sample):           # iterate
```

Two words rather than one because they are different operations: `over` consumes
an axis and the result does not have it; `along` walks an axis and each element
still has all the others. English makes the distinction naturally and a single
`axis:` for both is how `numpy.apply_along_axis` came to need a paragraph of
documentation.

Making `axis: 0` an error on a *named* tensor (`SC0638`) is the point of the
feature — a positional index into a named shape is the thing that goes stale.
The diagnostic offers the name at that position, so the fix is one click.

### 4.1 Transposition becomes a permutation of names, and mostly disappears

```science
let m2 be m.transpose(sample, feature)   # names, not indices
```

More usefully, most transposes stop being necessary: `a @ b` under Decision 7
contracts on the shared name, so an operand that "needs transposing" is one
whose names already say what to do.

## 5. Decision 7 — matrix multiplication contracts on the shared name

> **Decision 7. For operands with named axes, `@` contracts over the unique axis
> name they share. Zero shared names is `SC0639`; two or more is `SC0640`, and
> the fix is to say which, with `a.matmul(b, over: feature)`.**

```science
#  x: [sample: n, feature: 768]
#  w: [feature: 768, output: 10]
let y be x @ w                 #  [sample: n, output: 10]
```

This is the feature's best moment. The contraction axis is *named by both
operands*, so the operation states its own meaning, `x @ w` and `w @ x` are the
same computation, and the classic bug — a transposed weight matrix that happens
to be square — is `SC0639` rather than a number.

Requiring uniqueness rather than picking the first shared name is deliberate:
Einstein summation earns its ambiguity with an explicit index string, and
inferring one silently is guessing.

## 6. Decision 8 — indexing by name

> **Decision 8. `t[sample: 3]` selects along a named axis. Positional indexing
> `t[3, 7]` remains legal on fully anonymous shapes and is `SC0641` on named
> ones. The two forms may not be mixed in one subscript.**

This extends `indexing-and-array-literals.md` §9's slice, which stays contiguous
and one-dimensional; a named slice is `t[wavelength: 100..200]` and lowers to the
same `Slice of T`. Zero-based indexing (§10 of that note) is unchanged, because
this note changes *which* axis, never *how far along* it.

`SC0641` is the aggressive one: positional indexing into a named tensor is
forbidden rather than discouraged. The softer rule — allow it, warn — was
rejected because it leaves the stale-index bug alive in exactly the code that
opted into naming to avoid it.

## 7. The interaction with `with … as …`

`broadcasting.md` §6.2 promotes a runtime shape into the type. Named axes bind
there too, and the head reads better than the positional form ever did:

```science
    with table.shape as [sample: rows, feature: 768]:
        let z be (table .- table.mean(over: sample)) ./ table.deviation(over: sample)
```

Decision 6.3 of that note — `else` required exactly when the pattern is
refutable — applies unchanged. A **name** in the pattern is never refutable on
its own: names are static, so a mismatch is a compile error, not a runtime one,
and only the lengths can fail at runtime. This makes some `with` heads
irrefutable that were refutable before, which strictly reduces the number of
`else` branches a user must write.

## 8. What this note does not do

- **No units on axes.** A `wavelength` axis measured in nm is expressible as
  `Tensor of (Quantity of (F32, nm), …)` on the *element*; a unit on the *axis
  coordinate* means coordinate arrays, which is the next paragraph.
- **No coordinates.** xarray's other half is that an axis carries a vector of
  labels — actual wavelengths, actual timestamps — enabling label-based joins and
  resampling. That is a data structure, not a type feature, and it belongs with
  `Frame` in `data-io.md`. Named axes are the part that can be checked statically
  and this note takes exactly that part.
- **No inference of names across FFI.** A tensor from `extern` or from DLPack
  arrives anonymous. `python-interop.md` §4's field-for-field `DLTensor` has no
  room for names and must not grow one; the names are attached on the Science
  side by a `with` head, which is the existing mechanism for exactly this.
- **No generic-over-axis-name functions.** `def normalise of A(t: Tensor of (F32,
  [A: n, feature: 768]))` needs axis-name *variables*, which is constraint
  solving over the same lists `broadcasting.md` §11 declines. Anonymous axes are
  the escape and they are why Decision 2 keeps them.

## 9. What F0 must not preclude

Three things, all cheap now and expensive later:

1. **The shape type must be a list of `(name: Option, extent)` pairs, not a list
   of extents.** Adding a field to a type-level list after the unifier is written
   is the expensive version.
2. **`axis` must stay free** and must not be taken as an identifier-position
   keyword by any other note.
3. **The type-level list operations in `broadcasting.md` §5.3 —
   `pad_left`, `zip`, `delete_at` — must be defined over the pair, not the
   extent.** `delete_at` is what a reduction uses and it is where a name is lost
   if the pair is not threaded through.

## 10. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0168` | Syntax | `axis` item with no identifier; or a shape with a `:` and no name before it |
| `SC0169` | Syntax | Named and positional indices mixed in one subscript (§6) |
| `SC0635` | Types | An undeclared identifier in a shape position, with the `axis` item offered |
| `SC0636` | Types | Axis names disagree at an aligned position (§3) |
| `SC0637` | Types | Named and anonymous axes interleave (§3.2) |
| `SC0638` | Types | `axis: <int>` on a tensor with named axes; the name is offered |
| `SC0639` | Types | `@` with no shared axis name (§5) |
| `SC0640` | Types | `@` with more than one shared axis name (§5) |
| `SC0641` | Types | Positional index into a named tensor (§6) |
| `SC0642` | Types | A reduction's `over:` names an axis the operand does not have |
| `SC0643` | Types | `along:` on a reduction, or `over:` on an iteration |
| `SC0644`–`SC0659` | Types | Held for the axis-variable form of §8 |

`SC0636` is the note's teaching diagnostic and shows both shapes with the
disagreeing axis underlined in each — the same rendering `broadcasting.md` §10
specifies for a shape mismatch, with the name substituted for the index.

## 11. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| Shape lists carry an optional name per dimension | `broadcasting.md` §5, `const-expression-arithmetic.md` §10.1 | **The one real ask.** A field on a structure, threaded through three list operations |
| Alignment consults names first | `broadcasting.md` §5 | One branch at the head of an existing algorithm |
| `over:` / `along:` on the reduction vocabulary | `collections-and-chains.md` §1, `broadcasting.md` §7 | Signature change, no new method names |
| Named patterns in a `with` head | `broadcasting.md` §6.2 | Grammar only; §7 shows it reduces `else` branches |
| Keep `axis` free and contextual | `reserved-words.md` | One row |
| Named slices lower to `Slice of T` | `indexing-and-array-literals.md` §9 | Lowering only |

## 12. Risks

- **Two libraries will declare different `sample` axes** and users will have to
  convert between them. This is the `newtype` tax and it is the same one
  `unit-literals.md` pays; the mitigation is that the standard axis names should
  ship in one place in the standard library rather than being invented per
  package.
- **Decision 8's strictness (`SC0641`) will be argued about.** It is the
  difference between a checked feature and an advisory one and it should not be
  softened without a concrete case.
- **The feature is invisible until someone uses it**, so it will be judged on its
  first tutorial. If the documentation's first tensor example is anonymous, the
  feature is dead; if it is named, every user gets the check for free.
