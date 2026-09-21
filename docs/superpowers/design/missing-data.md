# Science — Design: missing data, and the reductions that refuse to guess

Date: 2026-09-18
Status: draft for review
Owns: the rule that a reduction over a nullable collection must be told what to
do about absence; the `skipping` / `propagating` policy arguments; the
`missing_count` obligation on a reduction's report; and the interaction between
absence and the units, uncertainty and statistics layers.
**Does not own, and does not reopen, how absence is spelled.** `data-io.md`
§4 decided that — *"Nullability is `T?`, and there is no other absence"* — and
§1 below adopts it and argues the case that was made *against* this note's
first draft.
Depends on: `data-io.md` §4 (`Frame of R`, `Rows of R`, nullable columns, the
Arrow validity bitmap, `null_value(...)` and the rejected third policy),
core spec §5.5 (absence is `T?`, `null` is its value),
`collections-and-chains.md` §1 and §5 (the chain vocabulary this extends and the
closed-set rule it must respect), `scientific-libraries.md` §7 (`stats`, the
consumer), `statistical-validity.md` §6 (the provenance fields this writes
beside), `uncertainty.md` §3, `unit-literals.md` §6, `broadcasting.md` §7
(reductions over tensors), `reproducibility.md` §5.2 (the run record).
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0610`–`SC0619`** (types).

---

## 0. The verdict up front, and the idea this note rejected on the way

The question put to this note was whether Science should have a distinct `NA` —
an *observational* absence, separate from `T?`, contagious through arithmetic,
the way R has it and the way pandas fails to have it.

**No.** `data-io.md` §4 already settled it and the settlement is correct:

> Nullability is `T?`, and there is no other absence. §5.5 of the core spec says
> absence is `T?`, whose one absent value is the literal `null`; the data layer
> inherits that rather than inventing a NULL of its own.

A second absence would mean two nullable types for every `T`, two sets of
patterns in every `match`, two validity representations at the Arrow boundary,
and a conversion between them that every user gets wrong in the same direction.
R's `NA` is not a second absence *within* an option type — R has no option type,
so `NA` is its only one, and copying the symptom without the cause is how a
language ends up with `null`, `None`, `NaN` and `NA` all at once. `data-io.md`
§8 already records that `T?` plus the validity bitmap is what *resolves* the
NaN-versus-NA confusion rather than adding to it.

So the spelling stays. What is missing is one layer up, and it is the part that
actually costs people their afternoons:

> **Decision 1. A reduction over a collection whose element type is nullable
> does not compile unless it is told what absence means. There is no default,
> and the two policies are spelled `skipping null` and `propagating null`.**

```science
let heights: Array of F64? be [1.72, null, 1.81]

let m be heights.mean()                     # SC0610
let m be heights.mean(skipping null)        # 1.765, over 2 of 3
let m be heights.mean(propagating null)     # null
```

## 1. Why the *default* is the bug, and not the representation

Every language in the audience's hands has a default here and each one is wrong
in a different direction:

| System | `mean` of a column with absences | Failure mode |
|---|---|---|
| pandas | Skips, silently | The denominator is not what the author thinks. Two columns of different completeness are compared as if they were the same sample |
| NumPy with `NaN` | Propagates, silently | The answer is `NaN` and the author adds `nan_to_num` somewhere upstream, which turns absence into zero |
| R | Propagates, loudly | Correct, and the user types `na.rm = TRUE` so reflexively that it stops being a decision |
| SQL | Skips, silently | `AVG` ignores NULL and `COUNT(*)` does not, in the same query |

R is the closest to right and its failure is instructive: the policy is a
*default argument* the user overrides, so it becomes muscle memory, and muscle
memory is not a decision. Making it mandatory is the only version that survives
contact with a tired person at 11pm.

**The cost, stated plainly: this is more typing, on a very common line.** That is
the whole price, it is paid by exactly the people this note is for, and it buys
the thing `data-io.md` §6 refused to let a parser do — *"coercing an unparseable
value into missingness is exactly how a unit error becomes a number in a paper"*.
The same sentence applies one layer up, to coercing missingness into a mean.

## 2. Decision 2 — the type carries it, so almost nobody pays

> **Decision 2. The obligation attaches to the *element type*, not to the
> collection. `Array of F64` reduces with no policy and always did. Only
> `Array of F64?` is affected.**

This is what makes Decision 1 affordable. A column that the schema declares
non-nullable cannot contain absence — `data-io.md` §4 makes an empty field in an
`F32` column a row error at read time — so every reduction over it is unchanged,
and that is the overwhelming majority of code. The user pays exactly where there
is something to decide, and the type is the thing that knows.

It also means the fix for a noisy pipeline is a real fix rather than an
annotation: `frame.column("height")` typed `F64?` becomes `F64` after a
`.drop_missing()` or a `.fill(0.0)`, and every downstream reduction quietly stops
asking. `data-io.md` §8 already requires those two methods for the DLPack
boundary, so this note adds no library surface to get its own escape hatch.

## 3. What each policy means, exactly

### 3.1 `skipping null`

The absent elements are removed and the reduction runs over what is left. **The
denominator is the count of present values**, which is the only defensible
reading and the one pandas takes.

Two consequences are specified rather than left to the implementation:

- **An all-absent input is not an error.** `mean(skipping null)` of an array with
  no present values is `null`, not a panic and not `NaN`. The result type of a
  reduction under `skipping null` over `T?` is **`T?`**, for this case alone.
  A user who wants the panic writes `.expect(...)`, which `stdlib-core.md`
  already owns.
- **The count is reported.** §4.

### 3.2 `propagating null`

One absent element makes the result `null`. The result type is `T?`. This is R's
`na.rm = FALSE` and it is the right default for a physicist and the wrong one for
a survey statistician, which is the reason neither is the default.

`propagating null` is **short-circuiting**: the reduction may stop at the first
absence. This is observable only through `effects.md`'s bits — a fold with a side
effect runs fewer times — and since a reduction's operator must be pure anyway
(§6), it is not observable at all.

### 3.3 The two spellings, and why they are not `Bool`

`mean(skipping: true)` and `mean(na_rm: true)` were both considered. They lose to
`skipping null` for one reason: a boolean argument at a call site is unreadable
without the signature in front of you, and `mean(x, true)` is a line that appears
in a code review with no way to check it. `skipping null` and `propagating null`
each read as English at the call site, which is §4.1 of the core spec's standard
for how far the English goes.

Grammatically these are **not** new keywords. `skipping` and `propagating` are
ordinary identifiers used as named-argument labels, the same way `axis: 0` and
`tolerance: 1e-12` already are throughout the corpus, and `null` is the existing
literal. §13's list gains nothing. The call is `mean(skipping: null)` in strict
named-argument syntax; the corpus's convention of writing a label and a value as
two words is followed here.

## 4. Decision 3 — the count travels with the answer

> **Decision 3. Every reduction under `skipping null` records how many values it
> skipped, and the pair is available as `.missing_count()` on the reduction's
> report. A `stats` routine that takes a nullable input carries the count into
> the provenance record.**

A mean over 340 of 1,000 rows and a mean over 998 of 1,000 rows are different
claims about the world and they print identically. This is the actual harm —
not the arithmetic, which is fine, but that the sample size silently changed and
no artefact records it.

`statistical-validity.md` §6 already writes analysis facts into the run record
and this is one more of the same shape: `missing: {"height": 660}`, integers
only, so `reproducibility.md`'s Decision 10 no-floats rule holds.

This is the part of the note a reviewer benefits from, and it is why the note is
worth more than a lint.

## 5. Which operations are affected, and which are deliberately not

> **Decision 4. The obligation applies to **reductions** — operations that
> collapse a collection to a smaller thing. It does not apply to element-wise
> operations, to filters, or to anything whose output is the same shape.**

| Operation | Affected | Why |
|---|---|---|
| `sum`, `mean`, `deviation`, `min`, `max`, `median`, `quantile`, `count` | **Yes** | The denominator or the extremum depends on the answer |
| `fold`, `reduce` | **Yes**, when the element type is nullable | It is the general case of the above |
| `map`, `filter`, `zip`, `take`, `sorted` | No | The absence is passed through and stays visible in the type |
| `correlation`, `covariance`, a regression fit | **Yes, and more strictly** — §5.1 | Pairwise deletion is a different decision from either policy |
| Tensor reductions (`broadcasting.md` §7) | **Yes**, same rule | A nullable tensor cannot reach DLPack anyway (`data-io.md` §8) |
| `length` | No | It counts slots, and it always did |

`count` is in the first row and it is the one that catches people: `xs.count()`
under `skipping null` is the number of present values and under
`propagating null` it is `null`. `xs.length()` is the number of slots and needs
no policy. Those are two different questions and SQL's famous
`COUNT(col)`/`COUNT(*)` divergence is what happens when one word does both.

### 5.1 Two-sample operations need a third word

`correlation` over two nullable columns has three defensible policies, not two,
and the extra one is the one everybody means:

```science
let r be correlation(height, weight, skipping null pairwise)
```

**Pairwise deletion** drops row *i* from both columns when either is absent, so
the two inputs stay aligned. Plain `skipping null` on two columns of different
completeness would silently compare values from different rows, which is a worse
bug than any this note started with. So:

> **Decision 5. For an operation over two or more aligned collections,
> `skipping null` alone is `SC0613`. The policy must be `skipping null pairwise`,
> which is the only alignment-preserving reading, or `propagating null`.**

`pairwise` is a third identifier label and not a keyword. Listwise deletion —
dropping row *i* from *every* column in a frame — is a `Frame` operation,
`frame.drop_missing()`, and belongs to `data-io.md` rather than to a call-site
policy.

## 6. The interaction with the other numeric layers

Four notes put something in the type beside the number, and each one composes
with absence in a way worth writing down once.

- **Units** (`unit-literals.md`). `Quantity of (F64, m)?` is a present or absent
  length. Absence does not have a dimension and does not need one; the policy
  applies to the reduction and the dimensional check is unchanged. Nothing to
  decide.
- **Uncertainty** (`uncertainty.md`). `Uncertain of F64` is a value that is
  *imprecisely known*; `F64?` is a value that is *not known at all*. They are
  genuinely different and both are useful in the same column. `Uncertain of F64?`
  is the well-formed spelling and `skipping null` reduces it with correlation
  tracking intact over the present values.
- **Statistics** (`statistical-validity.md`). A `PValue` computed under
  `skipping null` from a sample whose size changed is exactly the situation that
  note's flow analysis is about. The `missing_count` of §4 travels into the same
  record and no new mechanism is needed.
- **Broadcasting** (`broadcasting.md`). A nullable tensor keeps its shape — the
  validity bitmap is per element and does not change the type-level shape — so
  §5's alignment algorithm is untouched.

> **Decision 6. A reduction's combining operator must be pure
> (`effects.md` §5.1). `skipping null` changes how many times it runs, and an
> impure operator would make the two policies observably different in a way that
> has nothing to do with absence.**

## 7. What this note does not do

- **No `NA`.** §0. This is the note's largest decision and it is a decision to
  add nothing.
- **No imputation.** Mean-filling, forward-filling, multiple imputation and the
  rest are `stats` routines, not language or chain features. `.fill(v)` exists
  because DLPack requires it, and it is not an imputation strategy.
- **No missingness mechanism in the type.** MCAR, MAR and MNAR are the
  statistically meaningful distinctions and no compiler can check which one a
  dataset exhibits — this is the same verdict `statistical-validity.md` §0
  reached about its own question, and for the same reason.
- **No change to how files are read.** `null_value("NA")` in `data-io.md` §6 is
  a property of a file and stays there.

## 8. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0610` | Types | A reduction over a nullable element type with no policy. The message names the two policies and the `.drop_missing()` escape |
| `SC0611` | Types | Both policies given at one call site |
| `SC0612` | Types | A policy given where the element type is not nullable — the argument is inert and the type says so |
| `SC0613` | Types | `skipping null` on a two-or-more-collection operation without `pairwise` (§5.1) |
| `SC0614` | Types | `pairwise` on a single-collection reduction |
| `SC0615` | Types | A reduction's operator is not pure (§6) |
| `SC0616`–`SC0619` | Types | Free; held for the imputation surface if §7 is ever reopened |

`SC0610`'s wording is the one that matters, per `llm-ergonomics.md` §1. It is
the diagnostic a new user meets on their first real dataset and it has to teach
the whole idea in four lines:

```
SC0610: `mean` over `Array of F64?` needs to know what absence means
  --> analysis.science:14:19
   |
14 |     let m be heights.mean()
   |                     ^^^^^^ this collection can contain `null`
   |
   = `heights.mean(skipping null)` averages the 2 present values
   = `heights.mean(propagating null)` gives `null` if any value is absent
   = `heights.drop_missing().mean()` removes the absence from the type first
   = Science has no default here because the two answers differ and both are
     defensible. See `missing-data.md` §1.
```

## 9. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| The policy argument on the reduction vocabulary | `collections-and-chains.md` §1 | Signature change on ~8 methods; **no new method names**, which is what §1's closed-set rule protects |
| The same on tensor reductions | `broadcasting.md` §7 | Same shape |
| `pairwise` on the two-sample routines | `scientific-libraries.md` §7 | Signature change |
| `missing: {…}` in the run record | `reproducibility.md` §5.2, `statistical-validity.md` §6 | Integers, one map |
| Confirm `Uncertain of T?` is well-formed | `uncertainty.md` §3 | One sentence |
| Record that the `NA` question is closed here | `data-io.md` §4 | One cross-reference |

## 10. Risks

- **The verbosity is real and it lands on the audience's most-typed line.** If
  users respond by writing `.drop_missing()` reflexively at the top of every
  pipeline, the note has made things worse than pandas, because silent listwise
  deletion is a stronger assumption than silent skipping. The mitigation is that
  `.drop_missing()` is visible in the source and a policy-free `.mean()` was not;
  the risk is that visibility is not the same as thought.
- **Decision 5 makes a correct-looking line an error** (`skipping null` on a
  correlation) and that will read as pedantry until the first time it catches a
  misaligned pair.
- **The whole note is F1 and depends on `Frame`.** Nothing in F0 is blocked by
  it and nothing in F0 must preclude it; the one F0-shaped question is whether
  `collections-and-chains.md`'s signatures can take an optional policy argument
  at all, which is ordinary named-argument work.
