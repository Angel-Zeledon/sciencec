# Science — Design: train/test leakage, and how much of it is a type error

Date: 2026-09-16
Status: draft for review
Phase: F1, with `stats` and `data` — no F0 compiler work is required and none is asked for
Depends on: `statistical-validity.md` §4.3 (which sketched this design, handed it
over, and asked that it be ranked above itself — this note takes the sketch and
corrects three things in it), `scientific-libraries.md` §7.5 and §7.7 (the
catalogue this constrains), `data-io.md` §4 (`Frame of R`, `Rows of R`, and the
`Record` derivation), `stdlib-standard.md` §5 (`Key`, which is the precedent both
notes copy), `effects.md` §3.1 (the closed bit set, which stays closed),
`reproducibility.md` §5 (the record this writes into and the run-record fields it
adds), `models-and-inference.md` §7 (which says Science does not train neural
networks yet, and which §5 here does not contradict).
Syntax: `syntax-revision-2.md` throughout.
Diagnostics claimed: **`SC0500`–`SC0503`**, of the `SC0500`–`SC0504` block.
`SC0504` is returned unused. §9 states a phase-range problem with that block and
asks for a ruling rather than resolving it unilaterally.

---

## 0. The verdict up front

The question is whether a compiler can stop a program from evaluating a model on
data the model was fitted on, or on data that information about the model's
fitting reached.

**More of it than I expected, and less than the sibling note's sketch claimed.**

> Which rows a value came from is a property of the program. Nothing outside the
> source decides whether the array handed to `roc_auc` is an array the fit saw.
> So the roles a dataset's parts play — fitted on, selected on, reported on —
> can be **three distinct types**, and an evaluation that reads the wrong one
> does not compile.

That is the part that works, and it is ordinary type checking again: no new
analysis, no lattice, no interprocedural summary. But three things had to change
before the sketch in `statistical-validity.md` §4.3 could be built, and the
second is the whole design:

1. **The types are not enough on their own; the *API shape* is the feature.**
   The commonest leakage is not evaluating on training data. It is fitting a
   scaler, an imputer, a feature selector or a PCA on the whole dataset and
   *then* splitting. No type distinction prevents that, because at the moment the
   transform is fitted there is no split yet and therefore no wrong type to pass.
   It is prevented by **deleting the one-call form** of every data-dependent
   transform from the library, so that fitting is a separate step that needs
   something only a split can produce. §3.3 is the centre of this note and it is
   a library decision the type system then enforces, not a type-system trick.

2. **`Test` is the wrong name and the wrong count of roles.** `test` is a
   reserved word in `stdlib-standard.md` §7.1, and, more importantly, a
   validation set is a *third* role and not a shade of the second. The roles are
   **`Train`**, **`Tune`** and **`Holdout`**, and their asymmetry is the design:
   `Train` may be fitted on and re-split, `Tune` may be scored any number of
   times and the number is recorded, `Holdout` is **consumed by the evaluation
   that reads it**, exactly as `Key` is consumed by the draw that reads it.

3. **The sketch's assessment rule is wrong as written.** It said that
   `r_squared`, `confidence_interval` "and the rest of §7.5's assessment
   vocabulary accept a `Test of R` and nothing else". Cook's distance on
   held-out data is meaningless; AIC on held-out data is not AIC; in-sample R² is
   the definition of R². §3.6 corrects it: §7.5's "model assessment" list is two
   different vocabularies that were lumped together, and only one of them is
   about unseen data. The other one *requires* the training part.

**And one half of the honest answer is not a type at all.** Real pipelines split
in one program and fit in another; the type is lost at the file boundary. That
half is a **runtime** check against `reproducibility.md` §5's record, and it is
the half that covers the way people actually work. §3.9.

The headline, in the manner the sibling note settled on:

> **In Science, the part of a dataset you fitted on and the part you report on
> are different types, the reported-on part is spent when you read it, and a
> preprocessing step cannot be fitted on data that has not been split.**

What this cannot see is inventoried in §6.4 and it includes two things that are
common and one that is currently unsolved by anybody (§5.3). The inventory is
shorter than the sibling note's, which is the honest reason this note ranks above
it, and is not a claim that the inventory is empty.

---

## 1. What leakage actually is, enumerated

It is not one mistake. It is at least seven, they have different causes, and they
are visible to different machinery. Grouping them under one word is part of why
the field keeps committing them.

### 1.1 Evaluating on data the model was fitted on

The textbook case, and the rarest in code written by anyone who has been warned
once:

```science
let fit, err be least_squares(design, response)
let r2 be r_squared(fit, design, response)
```

**Visible to a type system**, completely, and it is the case the sketch was
written for. It survives loops, helper functions and data dependence, because it
is flow.

It has a variant that is not rare at all and that people do not recognise as this
mistake: **evaluating several models on the same held-out set and reporting the
best.** The set was not fitted on, but it was *selected* on, and the reported
number is optimistic by an amount that grows with the number of models. §3.4
treats selection as the same problem in a different position, which is what the
third role is for.

### 1.2 Preprocessing fitted before the split

```
scaler  = StandardScaler().fit(X)        # all the rows
X       = scaler.transform(X)
Xtr, Xte = train_test_split(X, y)
```

The mean and standard deviation the scaler subtracts were computed from the test
rows, so the test rows influenced the training features. The same is true of mean
imputation, of feature selection by correlation with the target, of PCA, of
target encoding of categoricals, of discretisation by quantile, and of
oversampling.

This is **the commonest form and the one people least recognise**, and the reason
is instructive: in every library that has the fit/transform distinction, the
convenient call — `fit_transform(X)` — does both at once and typechecks perfectly
well before any split exists. The mistake is not passing the wrong value. It is
*ordering*, and ordering is what a type system is bad at.

**Visible to a type system only after the API changes.** With the one-call form
present, no. With the one-call form deleted and a fitted transform reachable only
from a split part, yes and completely. §3.2 and §3.3.

### 1.3 Target leakage: a feature that encodes the label

A column that could only have been recorded after the outcome was known — a
discharge code in a mortality model, `account_closed_date` in a churn model, a
post-treatment measurement in a trial. The model is not wrong about the data; the
data is not about the question.

**Visible to neither.** It is a claim about what the columns *mean*, which is
nowhere in the program. A runtime scan can *suggest* — a single feature with
near-perfect mutual information with the target is worth a look — and §3.7 offers
one as a function the user calls, never as a check the compiler runs, on the
sibling note §1.3's reasoning about heuristics over proxies.

### 1.4 Temporal leakage

A model that forecasts tomorrow from today, evaluated on a uniformly random split
of a time series, has been trained on the future. The correct split is by time,
and often with a gap the length of the forecast horizon.

**Visible to neither statically**, because the compiler does not know that a
column is a time or that the question is a forecast. What it can do is cheap and
worth doing: the splitter is a named function, `split_by_time` exists beside
`split`, and **which one was used is in the record**. A reviewer who reads
"uniform random, 80/20" beside an ARIMA fit has a question to ask. That is the
entire contribution and it should not be described as more.

### 1.5 Group leakage

Ten photographs of the same patient, five on each side of the split. Three
readings from one instrument. Two rows from one household. The model memorises
the group and the evaluation measures the memorisation.

**Visible to neither statically.** Same treatment as temporal: `split_by_group`
exists, takes the grouping key explicitly, and the key's name goes in the record.
The compiler cannot know that `patient_id` was the right key or that there was a
key at all.

### 1.6 Leakage across program boundaries

The split is made by `prepare.science`, which writes `train.parquet` and
`holdout.parquet`. The fit is done by `fit.science` three weeks later. Between
them the type is gone: both files are a `Frame of R` to whoever opens them, and
nothing stops `fit.science` reading the holdout.

**Visible to a runtime check**, and this is where the design earns most of its
keep in practice, because this is the shape of every pipeline that is not a
tutorial. `data-io.md`'s writers already stamp `reproducibility.md` §5's run
record into Parquet, Arrow, npz and safetensors. A split writes its identity into
that record; the reader checks it. §3.9.

### 1.7 Duplicate and near-duplicate rows across the split

The same image twice under two filenames; the same patient entered twice; a
dataset assembled from two overlapping exports. The split is honest and the rows
are not distinct.

**Visible to a runtime check** only, and only for exact duplicates cheaply. Near
duplicates need a distance and a threshold, which is a modelling decision. `stats`
can offer an exact-duplicate count at split time — it costs one hash pass over
the rows and the number goes in the record. Anything beyond that is the user's.

### 1.8 The scoreboard

| Form | Static? | Where it lands |
|---|---|---|
| Evaluating on fitted rows | **Yes** — it is typing | `SC0501`, §3.1 |
| Fitting on a part reserved for evaluation | **Yes** — it is typing | `SC0500`, §3.1 |
| Reading a spent `Holdout` a second time | **Yes** — it is ownership | `SC0502`, §3.4 |
| Preprocessing fitted before the split | **Yes, after §3.3's API change**; no before it | §3.2, §3.3 |
| A published function that fits from an unsplit frame | **Yes**, at the boundary | `SC0503`, §3.3 |
| Selection pressure on the evaluation set | Counted at runtime, never refused | §3.4, the record |
| Split made in another program | At runtime, against the record | §3.9 |
| Duplicate rows across the split | At runtime, exact only | §1.7 |
| Target leakage | **Never** | §1.3, offered as a function |
| Temporal ordering ignored | **Never** | §1.4, the splitter is recorded |
| Group structure ignored | **Never** | §1.5, the key is recorded |
| Whether the holdout is representative | **Never** | Out of range for any compiler |

Four of the five type-checkable rows are the ordinary type checker applied to
library types. The fifth is ownership, which Science already has for `Key`.

---

## 2. What is actually static, and why this is a better target than its sibling

`statistical-validity.md` §4.3 gives the reason in one sentence and it is worth
restating because everything follows from it:

> Leakage is not about the values in the data at all. It is about *which values
> flowed where*, and provenance is a program property in a way that family
> membership never is.

The sibling note's §1.4 — hypotheses chosen after seeing the data — has no
analogue here. There is no fact about the scientist's afternoon that decides
whether the array passed to `roc_auc` is the array `least_squares` was fitted on.
The two programs are not character-for-character identical; they differ in which
binding appears at the call site, and that difference is exactly what a type
checker reads.

That is the whole reason this note's inventory of failures (§6.4) is shorter than
its sibling's, and it is why the sibling asked for this one to be ranked above
itself.

**Three caveats, stated here so that §6 does not look like it discovered them.**

**The property is static only within a compilation unit.** §1.6 is not a corner
case; it is the normal shape of a pipeline. The type system covers the
single-program case completely and the multi-program case not at all, and the
record covers the multi-program case at runtime. A design that shipped only the
types would cover the tutorials and miss the work.

**The ordering problem (§1.2) is not static in the form people write it.** It
becomes static only because §3.3 removes the function that expresses it. That is
a library decision with a type-shaped enforcement, and describing it as a
compiler feature would be a stretch that §6.1 makes at full strength.

**Fold identity is not static without machinery this language should not buy.**
§3.5.

---

## 3. The mechanism

### 3.1 Decision 1: three roles, three types, and `Holdout` rather than `Test`

> **Decision 1.** `stats` defines `Train of R`, `Tune of R` and `Holdout of R`,
> plus `Whole of R` for work that has no held-out evaluation. They are distinct
> types, not interconvertible, and they share an interface `Part` that carries
> the read-only half of `Frame`'s surface. **Only a `Train` or a `Whole` may be
> fitted on. Only a `Tune` or a `Holdout` may carry an out-of-sample claim.**

```science
interface Part:
    type Record

    ## Forwarded from `Frame` so that reading, summarising and plotting a part
    ## never needs the escape of §3.7. The role survives all of these.
    function length(self) -> U64
    function columns(self) -> borrowed Self.Record.Columns
    function row(self, index: U64) -> Self.Record
    function rejected_count(self) -> U64

## May be fitted on.
interface Fitting:
    type Record

## May carry a claim about data the fit did not see.
interface Assessing:
    type Record

Train of R implements Part, Fitting
Whole of R implements Part, Fitting
Tune of R implements Part, Assessing
Holdout of R implements Part, Assessing
```

`Whole of R` is the role for work where there is no held-out evaluation at all —
a k-means over a sample, a PCA for a figure, a description of everything that was
measured. It exists because §7.7's methods are mostly of that kind and a design
that forced them through a split would be wrong about what they are for. It is
obtained by `stats.whole(frame)`, which asserts nothing and needs no reason
string: the author is saying *there is no evaluation here*, which is a true
statement about their own program and not a claim the compiler would want to
check.

**Why `Holdout` and not `Test`, departing from `statistical-validity.md` §4.3's
sketch.** Two reasons, and the note is named because a silent departure from a
sibling is what `README.md`'s conventions forbid.

- `stdlib-standard.md` §7.1 introduces a `test "name":` item form and §7.3 prices
  the reserved word. `Test of R` beside `test "fits a line":` is legal — the
  lexer distinguishes case — and it is the kind of legal that makes a reader
  hesitate on every occurrence. The word is spent.
- More substantially: "test set" is the name of the *final* evaluation set, and
  the design needs a third role for the set you are allowed to look at
  repeatedly. Calling the final one `Holdout` and the repeated one `Tune` makes
  the asymmetry lexical. Calling them `Test` and `Validation` would preserve the
  field's vocabulary and would put the two words that everybody confuses next to
  each other, which is the situation the design exists to fix.

The documentation says, once, in the first paragraph of the module: *`Holdout` is
what the literature calls the test set; `Tune` is what it calls the validation
set.* That is the migration cost and it is one sentence.

**Rejected: one `Part of (R, Role)` type parameterised by a marker.** Tidier —
`apply` would be generic in the role without an interface — and it makes every
signature carry two type arguments where one would do, and it makes the
diagnostic for a role mismatch a generic-argument mismatch rather than a type
mismatch between two named types. `llm-ergonomics.md`'s thesis applies: the
message a reader gets from `expected Train of Measurement, found Holdout of
Measurement` is better than the one from `expected Part of (Measurement, Train)`.

**Rejected: a `role` field checked at runtime.** Same objection the sibling note
made to a `adjusted: Bool` field. It moves the check into the eight-hour job.

**Rejected: roles as an effect.** §3.8.

### 3.2 Decision 2: the frame is consumed by whichever role assignment happens first

> **Decision 2.** `stats.split`, `stats.split_by_time`, `stats.split_by_group`
> and `stats.whole` all take the `Frame of R` **by value**. A frame is spent by
> the first of them to touch it, and there is exactly one.

```science
function split of R(frame: Frame of R, key: Key, holdout_fraction: F64)
    -> (Train of R, Holdout of R)

function split_tune of R(train: Train of R, key: Key, tune_fraction: F64)
    -> (Train of R, Tune of R)

function whole of R(frame: Frame of R) -> Whole of R
```

This is the `Key` move in the one place where it does the most work. `Key` is not
safe because it is hard to misuse; it is safe because consuming it makes reuse a
compile error *whether or not the author is paying attention*
(`statistical-validity.md` §2.2 makes exactly this point about why its own `study`
scope was worth less than it looked). Here, consuming the frame means:

- **You cannot treat the data as a whole and then split it.** `whole(frame)`
  spends the frame; `split(frame, …)` afterwards does not compile.
- **You cannot split it twice** and get two inconsistent partitions of the same
  rows.
- **The key is consumed too**, so a split inside a loop must either carry a fresh
  key per iteration — visible, and the keys are derived by `split_many`, which is
  recorded — or fail to compile. Re-splitting to obtain a fresh holdout is
  therefore not something that happens by accident.

Note carefully what Decision 2 does **not** do on its own. It does not prevent

```science
let scaled be frame.fill_missing(Mean)        # if this function exists
let train, holdout be split(scaled, key, 0.2)
```

because the transform ran before the frame was spent. Decision 2 closes the
ordering hole only in combination with Decision 3, which removes the function on
the first line. Said the other way: **Decision 2 makes the role assignment
unique, and Decision 3 makes it unavoidable.** Neither is sufficient alone, and
the note would be wrong to present Decision 2 as the mechanism.

**Cost.** A program that genuinely wants the same rows in two roles — an
exploratory pass over everything and then a modelling pass — must load twice, or
clone the frame, and cloning a frame is expensive and explicit. That is the
intended friction and it is the same friction `Key` imposes. It will be
irritating in notebooks.

**Rejected: borrowing the frame.** Every other design in this note survives it;
this one does not. A borrowed frame is still there afterwards, and "fit on the
whole, then split" becomes writable again.

### 3.3 Decision 3: a transform that estimates anything from the data is a fitted transform, and the one-call form is deleted

This is the central case, the commonest leak, and the largest bill in the note.

> **Decision 3.** In `stats`, **any transformation whose behaviour depends on
> statistics of the data it is given is two steps**: a constructor that takes
> something `Fitting`, and an `apply` that is generic over the role and preserves
> it. The single-call form that fits and applies at once does not exist, for any
> of them, at any convenience level.

```science
Standardiser has:
    function fit of P(data: borrowed P) -> Standardiser where P: Fitting

    ## Generic in the role and returns the same role it was given: this is the
    ## one signature the whole design turns on. `apply` takes the part by value
    ## and returns it, so the transform may work in place.
    function apply of P(self, data: P) -> P where P: Part
```

```science
use stats (split, Standardiser, Imputer, least_squares, evaluate)
use random (Key)

function main():
    let frame, err be read_csv of Measurement(path, CsvOptions.new()).collect()
    if err?:
        return

    let train, holdout be split(frame, Key.from_seed(20260916), 0.2)

    ## Fitted on the training rows only — there is no other kind of value
    ## `fit` accepts — and applied to both.
    let scaler be Standardiser.fit(train)
    let imputer be Imputer.fit(train, strategy: Median)

    let train be imputer.apply(scaler.apply(train))
    let holdout be imputer.apply(scaler.apply(holdout))

    let fit, err be least_squares(train.examples(
        features: [each.temperature, each.humidity],
        target: each.yield,
    ))
    if err?:
        return

    let report be evaluate(fit, holdout)      # consumes `holdout` — §3.4
    print(f"held-out RMSE {report.rmse:.4f} over {report.length} rows")
```

The Python that everyone writes has no spelling here. `Standardiser.fit(frame)`
does not compile, because `Frame of R` does not implement `Fitting` — that is the
entire point of making `whole` an explicit call rather than a property of frames.
`Standardiser.fit(holdout)` does not compile either, and that is `SC0500`.

**What is in scope for Decision 3, concretely.** From `scientific-libraries.md`
§7.6 and §7.7, and from what those sections are missing:

| Becomes fit / apply | Stays one call | Why |
|---|---|---|
| Standardisation, min-max scaling, robust scaling, normalisation | — | Estimates a location and a scale |
| Mean, median, mode, model-based imputation | Constant fill, forward fill, `drop_missing` | The first group estimates; the second does not |
| `principal_components`, `kernel_pca`, `factor_analysis`, `independent_components`, `partial_least_squares` | — | Estimates a basis, and the basis becomes model input |
| Feature selection by variance, correlation, mutual information or model weight | — | The commonest severe leak after scaling |
| Target/impact encoding, quantile discretisation, power transforms | One-hot with a declared level set | The first group reads the target or the distribution |
| `interpolate_missing` over a whole column | `fill_missing(0.0)` | Interpolation over a column crosses the split boundary |
| — | `tsne`, `umap`, `multidimensional_scaling`, `isomap`, `locally_linear` | Transductive by construction; there is no "apply to new rows". §3.3.1 |
| — | `k_means`, `dbscan`, `hierarchical_cluster` and the rest of §7.7 | Take a `Whole`, or a `Train` when the labels feed a model |

#### 3.3.1 The limit of Decision 3, stated where it is not convenient

A transform that has no out-of-sample form cannot be given one, and t-SNE and
UMAP are the examples. They are fitted on everything by construction; the
embedding of a point depends on the other points. There is no version of them
that respects a split.

That is fine when the output is a figure and wrong when the output becomes a
model's input, and **the compiler cannot tell the difference** — the array is an
array either way. The documentation says so at each of those functions and the
record lists their call sites. This is a documentation limit inside a note whose
thesis is that documentation limits do not work, and it is stated rather than
hidden because the alternative is to claim a coverage the design does not have.

**Cost of Decision 3, plainly.** Roughly twenty to thirty functions in `stats`
change shape, and the shape they change to is longer at every call site. A user
who wants a scaled matrix writes two lines where scikit-learn writes one, and
they write them for the rest of their lives. This is the cost the note is most
likely to be judged on and §6.1 argues it is fatal.

**Rejected: keeping the one-call form and warning on it.** A warning on
`fit_transform`-shaped code has the property `statistical-validity.md` §1.3
rejects: it is right most of the time about a question that only matters when it
is wrong. It also does not fire in the case that matters, because the call is
correct in the many programs where there is no split at all.

**Rejected: a taint analysis over frames.** A provenance lattice threaded through
every array-returning function in `stats` and `data`, so that the compiler could
see that the scaler's parameters descend from rows that later landed in the
holdout. The sibling note rejected the same thing in a sentence and the sentence
is right: it is a large annotation burden and it is defeated by the first
`Array.concat`. It is also the one part of the design that would produce false
positives, and §6 turns on not having any.

### 3.4 Decision 4: the holdout is consumed by the evaluation; the tuning set is borrowed and counted

> **Decision 4.** `evaluate` takes `Holdout of R` **by value**. A `Holdout` may be
> read once in the life of a program. `score` takes `borrowed Tune of R` and may
> be called any number of times; the number is counted at runtime and appears in
> the run record.

```science
## Consumes the holdout. One pass, and every metric comes out of it.
function evaluate of (R, P)(fit: borrowed Fit of P, data: Holdout of R)
    -> Assessment of R

## Comparing several models is a different act and has a different name: it
## consumes one holdout, names the count, and the count is what a reviewer wants.
function evaluate_all of (R, P)(fits: borrowed Array of Fit of P, data: Holdout of R)
    -> Array of Assessment of R

## Repeatable, by design, because early stopping and hyperparameter search are
## legitimate and are also selection. Counted.
function score of (R, P)(fit: borrowed Fit of P, data: borrowed Tune of R, metric: Metric)
    -> F64
```

`Assessment of R` carries the predictions and the truths, so `rmse`, `mae`,
`roc_auc`, `log_loss`, `brier`, `calibration_curve` and the per-class breakdown
are all ordinary functions over it. **The holdout is spent once and yields every
metric**, which is the resolution of the obvious objection that consuming it
would allow only one number.

What consumption buys, precisely:

- **Hyperparameter search against the holdout does not compile.** The loop body
  moves the holdout on its first iteration; the second is `SC0502`. To search,
  the author must carve a `Tune` out of the `Train` — which is the correct
  protocol, and it is now the path of least resistance rather than the
  path of most discipline.
- **Comparing five models has to say that it is comparing five models.**
  `evaluate_all` takes the array and the array's length is the count. That count
  is the multiplicity `statistical-validity.md` exists for: five models on one
  holdout is a family of five, and if the author then tests whether model A beats
  model B, the `PValue` from that test belongs in an array that reaches a
  correction. **The two notes join here** and neither had to be told about the
  other.
- **A three-way split stops being advice.** It is what the types make convenient.

What it does not buy, and this is the most important limit in the note: **it is
one program.** A person who evaluates, looks at the number, edits the file and
runs it again has tuned against the holdout, and the compiler saw two clean
programs. This is `statistical-validity.md` §1.4's problem in a new costume, and
it is the dominant real-world mechanism for holdout overfitting in machine
learning — the leaderboard effect. §6.4 lists it first.

**Rejected: a scoring budget on `Tune`.** `Tune.with_budget(50)`, failing the run
on the fifty-first score. It is a number nobody knows, it fails in the middle of a
job, and exceeding it is not an error — a thousand evaluations during a
hyperparameter sweep is normal and the optimism it induces is a matter of degree.
**Recording the count is the right response to a quantity nobody can threshold**,
which is the same judgement `statistical-validity.md` §1.3 made about
independence.

**Rejected: consuming the `Tune` as well.** It would make early stopping
impossible to write, which is not a bug in early stopping.

**Rejected: making `Holdout` `Clone`.** One line, and it deletes Decision 4.

### 3.5 Decision 5: folds are scopes, not type indices — cross-validation is higher-order

Cross-validation refits on every fold, so the parts must be per-fold. The
question put to this note was whether that works without dependent types. It
does, and the reason is that Science already has the machinery under a different
name.

> **Decision 5.** `k_fold`, `leave_one_out` and `stratified_k_fold` do not return
> a collection of parts. They take a function and call it once per fold, handing
> it a `borrowed Train` and a `borrowed Tune` that **cannot outlive the call**.
> Fold identity is a region, not a type parameter.

```science
function k_fold of (R, S)(
    data: borrowed Train of R,
    key: Key,
    folds: U64,
    each_fold: function(borrowed Train of R, borrowed Tune of R) -> S,
) -> Array of S
```

```science
let scores be k_fold(train, key, 10, fold giving:
    let scaler be Standardiser.fit(fold.train)
    let fitted, err be least_squares(scaler.apply_borrowed(fold.train).examples(…))
    score(fitted, fold.tune, Metric.Rmse)
)
```

Three things fall out.

**No dependent types, and no const generics.** There is no `Fold of k` and
nothing counts. Region inference — which the core spec §3 already commits to and
which `effects.md` §3.5 says shares a traversal with the effect solver — refuses
a borrow that escapes the callback. That is the same machinery that stops a
`borrowed` row escaping a `Rows` iteration, and it needs no extension.

**The transform is refitted per fold because it has to be.** The scaler is
constructed inside the callback from `fold.train`, which is the fold's training
part and not the outer one. This is the single most valuable thing the design
does for cross-validation, because **fitting the scaler once outside the loop is
the standard mistake** and here it is not writable: a `Standardiser` fitted
outside has no way in — it would have to be captured, and a captured
`Standardiser` is fine to *apply*, so the honest statement is narrower: fitting
outside and applying inside compiles, and is wrong, and is exactly what
`cross_validate` exists to prevent by doing the whole thing for you. See below.

**`cross_validate` remains, and is still the right answer for most people.**
§7.5 already has it; it takes the data and a fitting function and does everything
internally, and under this design its fitting parameter takes a `borrowed Train`
so that a user-supplied fitting closure cannot reach anything else. The types
matter for the hand-rolled loop — and people hand-roll, constantly, because the
built-in `cross_validate` never quite fits the pipeline they have.

**Rejected: generative brands.** The complete answer to fold identity is the
`runST` trick: parameterise every part by a fresh existential brand,
`Train of (R, B)`, and give `k_fold` a rank-2 signature
`for<B> function(Train of (R, B), Tune of (R, B)) -> S`, so that fold *k*'s train
and fold *j*'s tune do not unify. It is sound, it is known to work (Haskell's
`ST`, Rust's `GhostCell`), and it would close the residual hole in §6.4.

It is rejected on price. It needs higher-rank polymorphism, and Science does not
yet have closure *types* at all — `function(T) -> U` is a standing cross-note ask
in `README.md` with three customers and no owner. Asking for `for<B> function(…)`
on top of an unresolved ask is asking a type-checker author to build System F
where they were planning to build Hindley-Milner with annotated signatures (core
spec §5.2). The residual hole is narrow — it is confusing one fold's parts for
another's *inside a single callback*, where both are in scope — and it is not the
error anybody makes. Priced, refused, and written down so that the possibility is
visibly declined.

### 3.6 Decision 6: in-sample diagnostics take the training part, and §7.5's "model assessment" list is two lists

> **Decision 6.** The assessment vocabulary splits in two. **In-sample
> diagnostics** take the part the fit was made from. **Out-of-sample assessment**
> takes an `Assessing` part. A function in the first group given a `Holdout`, or
> one in the second given a `Train`, does not compile.

`scientific-libraries.md` §7.5's "Model assessment" line lumps these together:

> `Fit` `residuals` `fitted_values` `leverage` `cooks_distance` `dffits`
> `variance_inflation` `r_squared` `adjusted_r_squared` `aic` `aicc` `bic`
> `deviance` `log_likelihood` `confidence_interval` `prediction_interval`
> `cross_validate` `k_fold` `leave_one_out` `bootstrap_confidence`

Every name up to and including `log_likelihood` is **in-sample by definition**.
Cook's distance measures the influence of an observation *on the fit*, so it is
undefined for an observation the fit never saw. AIC is a penalised in-sample
log-likelihood; computed on held-out data it is not AIC and has no theory behind
it. R² is the fraction of *the fitted data's* variance explained — held-out R² is
a different quantity with the same name, which is precisely why it should have a
different name.

So `statistical-validity.md` §4.3's sketch — "`score`, `r_squared`,
`confidence_interval` and the rest of §7.5's assessment vocabulary accept a
`Test of R` and nothing else" — is wrong, and building it as written would produce
a library in which the classical regression diagnostics cannot be computed. This
note contradicts that sentence explicitly, which is what `README.md`'s conventions
require, and the sibling's §9.2 already hands this section over for exactly this
kind of revision.

The corrected division:

| In-sample — takes the fitting part | Out-of-sample — takes an `Assessing` part |
|---|---|
| `residuals`, `fitted_values`, `leverage`, `cooks_distance`, `dffits`, `variance_inflation` | `evaluate`, `evaluate_all`, `score` |
| `r_squared`, `adjusted_r_squared`, `deviance`, `log_likelihood` | `rmse`, `mae`, `accuracy`, `roc_auc`, `log_loss`, `brier` |
| `aic`, `aicc`, `bic` | `calibration_curve`, `confusion_matrix` |
| `confidence_interval` (of coefficients) | `prediction_interval` coverage, on held-out rows |

**And the right-hand column mostly does not exist yet.** §7.5 has no `accuracy`,
no `roc_auc`, no `rmse`, no `log_loss` — it is a classical-statistics catalogue
with the machine-learning assessment vocabulary missing entirely. That is an ask
of §7.5 in its own right (§10.1), independent of leakage, and it is the reason
this note's API bill is smaller than it first looks: half the functions the design
constrains have to be written anyway.

**The subtlety Decision 6 does not catch.** Nothing checks that the `Train`
handed to `cooks_distance` is the *same* `Train` the fit was made from. A program
with two training parts could mix them. This needs the brands of §3.5 and is
refused with them; the error is not one anybody makes, because the binding is
right there.

### 3.7 Decision 7: two escapes, no suppression, and the same reasoning as the sibling

`statistical-validity.md` §3.3 has no suppression attribute and argues that a
hatch which silences a diagnostic is one only the author sees. **The reasoning
applies here with more force, not less**, because the audience for this record is
a reviewer deciding whether to believe a reported accuracy, and a source-local
`#[allow]` is invisible to them by construction.

> **Decision 7.** There is no suppression attribute, no `--no-leakage-checks`
> flag, and no manifest setting. There are two escapes. **Both are available
> without friction and neither can be used invisibly.**

**The conversion out: `part.frame()`.** A `Part` yields the `Frame` it wraps and
the role is gone. It must exist — every function in `data`, in `signal`, in the
user's own code and in every package takes a `Frame` — and therefore, exactly as
in the sibling note, **the check is a speed bump and not a wall.**

The design narrows it rather than restricting it: `Part` forwards `length`,
`columns`, `row` and `rejected_count` (§3.1), so summarising, plotting and
inspecting a part never reach for `frame()`. What remains is handing the rows to
something that fits or scores, which is exactly the case worth seeing.

**The declaration: `stats.declare_split(…, because:)`.** The Kaggle case, and the
case of two files from a collaborator:

```science
## The organiser split these; I did not, and nothing here can check that they
## are disjoint. The string is not read by the compiler.
let train, holdout be stats.declare_split(
    train_frame,
    holdout_frame,
    because: "provided as separate files by the data collection; see README §2",
)
```

Three things about it, following the sibling's treatment of `without_correction`
closely enough that the parallel should be obvious:

- **The name is a visible contradiction and that is deliberate.** `declare_split`
  says a split happened somewhere the compiler cannot see. A name like
  `trusted_split` would read as a credential and would be slapped on everything.
- **The `because:` string is never parsed.** It is carried verbatim to the
  record, where a reviewer reads it. The compiler does not check it, cannot check
  it, and must not appear to.
- **It is required exactly where an unverifiable claim is made, and nowhere
  else.** `stats.whole(frame)` takes no reason, because it asserts nothing about
  disjointness — it says there is no evaluation here, which is a true statement
  about the program. Asking for a justification string on every k-means would
  train people to write `because: "x"`, which is the failure mode of every
  mandatory-rationale mechanism ever shipped.

**What is deliberately absent: a single-sided `declare_holdout`.** The claim
being made is always about a *pair* — these rows and those rows are disjoint —
and a function that mints one side alone would let an author declare a holdout
without ever naming what it is held out from. The multi-program case, which is
the one that seems to need it, is served properly by §3.9.

**The cost of having no suppression.** A user who is genuinely stuck writes
`frame()`, which is seven characters and appears in a record they may not want it
in. That is the intended pressure and it is mild. The larger cost is that
`frame()` will be common in early code and the record will be noisy until users
learn the forwarded methods exist; §11 lists that as a risk.

### 3.8 Decision 8: this is not an effect, and `effects.md`'s closed set stays closed

`effects.md` §3.1 calls "the set is closed" the most load-bearing sentence in that
note. A `leakage` bit, or a `fitted` bit that accumulates up the call graph, is
the obvious next thought and it should be refused here so that the effect
system's author does not have to guess.

> **Decision 8. There is no leakage effect and no fourth bit. The mechanism is in
> the types, in the manner of `effects.md` §6.1's device residence and §6.2's
> differentiability.**

- **An effect answers a question nobody asked.** "Somewhere below this call, a
  fit happened" is true of almost every function in a modelling program and
  licenses no decision. The question that matters — *which rows* — has a payload,
  and an effect with a payload is an effect row with a label, which is Koka, which
  `effects.md` §3.4 declined.
- **Effects propagate monotonically; roles do not.** A function that fits on its
  argument and a function that scores on its argument, called from a third, do
  not make the third a leak. The composition is decided by which *values* flow in,
  which is the type's job.
- **The signature already carries it**, and `effects.md` Decision 1 forbids the
  duplicate: `-> (Train of R, Holdout of R)` is the fact.

This note asks `effects.md` for nothing and says so in §10.3, so that the
possibility is visibly declined rather than silently unconsidered.

### 3.9 Decision 9: across programs the check is the record, and it is a runtime check

This is the half of the design that is not a type, and it is the half that covers
how people work.

> **Decision 9.** A split writes its identity into the run record that
> `data-io.md`'s writers already stamp. A reader that opens a part by role
> checks the stamp and fails if it does not match. **This is a runtime check, it
> is the only thing that works across program boundaries, and the note does not
> pretend it is a type.**

```science
## prepare.science
let train, holdout be split(frame, Key.from_seed(20260916), 0.2)
write_parquet(train.frame(), Path.from("train.parquet"), options)
write_parquet(holdout.frame(), Path.from("holdout.parquet"), options)

## fit.science, three weeks later
let train, err be stats.read_train of Measurement(Path.from("train.parquet"))
if err?:
    return
```

`read_train` opens the file, reads the `science.provenance` metadata that
`reproducibility.md` §5.3 already puts there, and returns a `Train of R` if and
only if the record says this file is the training part of a split. Opening
`holdout.parquet` with `read_train` is `StatsError.WrongPart`, naming the split id
and the role the file actually carries, at open, before a row is parsed — which is
`data-io.md` §5.1's own standard for schema errors and the reason that standard
was set.

The split object in the record is strings and integers only, per
`reproducibility.md` §5.1's no-floats rule:

```json
"split": {
  "id": "sp:4c1e90ab77d3",
  "of": "sha256:…",
  "method": "uniform",
  "holdout_fraction": "1/5",
  "key": "20260916",
  "group_key": null,
  "time_column": null,
  "rows": { "train": 41210, "holdout": 10303 },
  "exact_duplicates_across_parts": 0
}
```

`holdout_fraction` is a ratio string rather than a float, for that rule's reason.
`exact_duplicates_across_parts` is §1.7's one hash pass, computed at split time,
and it is the only number in this design that can catch a leak the types cannot
see at all.

**What this does not do.** It does not stop anyone calling `read_csv` on
`holdout.parquet` and getting an ordinary `Frame`. Nothing can, and nothing
should — the file is data and a person may legitimately want to look at it.
The check fires on the path where somebody is *trying* to do the right thing and
has the files confused, which is a real and boring error that costs real days,
and it makes the split's identity a thing that exists in the artefacts rather
than in somebody's memory of what they ran in March.

---

## 4. What this does to the API surface

The question was whether a train-typed parameter on every fitting function is
affordable, and whether generics absorb it. Three answers, in order of how much
they change the bill.

### 4.1 Generics absorb the transforms completely

`apply of P(self, data: P) -> P where P: Part` is written once per transform and
serves all four roles. There is no per-role duplication anywhere in the design —
the role-generic signature is the *default* spelling, and a concrete role appears
only in the few places where the restriction is the point: `fit` (wants
`Fitting`), `evaluate`/`score` (want `Assessing`), the in-sample diagnostics of
§3.6 (want the fitting part). That is perhaps forty signatures out of several
hundred in `stats`.

So the naive fear — that §7.5's thirty fitting functions and §7.7's twenty-five
unsupervised ones each need a new parameter — does not materialise. Most of §7.7
takes a `Whole` or is generic, and most of §7.5's fitting functions take one
`Examples` where they previously took a design matrix and a response vector.

### 4.2 The real bill is a shape change on about thirty functions, not a type on three hundred

| What changes | How many | Who pays |
|---|---|---|
| Data-dependent transforms become fit / apply (§3.3) | ~20–30 | `stats`, §7.6 and §7.7 |
| §7.5's fitting functions take `Examples of (Rl, N, P)` | ~30 | `stats` §7.5 |
| §7.5's assessment list splits in two, and the out-of-sample half is written (§3.6) | ~15 new functions | `stats` §7.5 |
| `k_fold`, `leave_one_out`, `cross_validate` become higher-order | 4 | `stats` §7.5 |
| `split`, `split_by_time`, `split_by_group`, `whole`, `declare_split`, `read_train`/`read_tune`/`read_holdout` | 8 new | `stats` |
| Four types, one interface trio, `Assessment`, `Examples` | ~8 types | `stats` |

The fifteen new out-of-sample metrics are owed regardless — a statistics library
for a machine-learning audience with no `roc_auc` in it is incomplete on its own
terms — so the marginal cost of this note is the shape change and the four role
types.

### 4.3 The frame-to-matrix boundary, which is where the role nearly gets lost

`scientific-libraries.md` §7.9 fits from matrices, not frames:

```science
function least_squares of (const N: Int, const P: Int)(
    design: borrowed Matrix of (F64, N, P),
    response: borrowed Vector of (F64, N),
) -> (Fit of P, LinalgError?)
```

and `data-io.md` §4.2's column access hands out a `borrowed Array of F32`. The
role dies at that step, and if nothing is done the design protects the frame layer
and leaves the layer where fitting actually happens unguarded.

> **Decision 10.** The role rides to the matrix layer on one `stats`-owned type.
> `Examples of (Rl, N, P)` pairs a design matrix with a response and carries the
> role. **No type in `linalg` and no type in `broadcasting` gains a role
> parameter.**

```science
Train of R has:
    function examples of (N, P)(self, features: …, target: …)
        -> Examples of (Train, N, P)

function least_squares of (N, P)(data: borrowed Examples of (Train, N, P))
    -> (Fit of P, LinalgError?)
```

This is the alternative the brief asked to be compared against: **a smaller
marked boundary.** It is smaller in exactly the way that matters — the role
appears in `stats` and stops there, so a `Matrix` is a `Matrix` everywhere else in
the language and `linalg`'s several hundred signatures are untouched. Making
`Matrix` role-parameterised was the other option and it is rejected: it would put
a statistics concept into the numerical core, infect `broadcasting.md`'s alignment
rules, and appear in the signature of every function that multiplies two matrices
for any reason.

**The cost of Decision 10** is that a user who builds a design matrix by hand —
from a simulation, from an instrument, from `linalg` directly — has no role to
carry, and must say so with `Examples.from_matrix(design, response, because:)`,
which is `declare_split`'s sibling and is recorded the same way. For simulation
work, which is a large fraction of this language's intended audience, that call
will be routine, and a record full of routine escapes is a record nobody reads.
§11 names this as the risk most likely to be underestimated.

**Also unresolved**: the column-selection spelling — `features: [each.temperature,
each.humidity]` — depends on `data-io.md`'s and `broadcasting.md`'s treatment of
`each`, and on `README.md`'s standing ask for explicit type arguments at call
sites. This note uses a plausible spelling and does not claim to own it (§10.2).

---

## 5. Deep learning specifically

The owner's focus, and the section where the design's claims have to be cut down
to what is true.

### 5.1 The validation set is a third role, not a shade of the second

A training loop sees the validation set every epoch, chooses the stopping point
from it, and that is both correct practice and selection pressure on a set the
model is not supposed to have learned from. It is not a mistake to fix; it is a
quantity to disclose.

`Tune` is that role: borrowed rather than consumed, scoreable without limit, and
**counted**. The run record gets

```json
"assessment": {
  "tune_scorings": 3120,
  "holdout_evaluations": 1,
  "models_in_final_comparison": 4
}
```

Three integers. `tune_scorings: 3120` on a 3120-epoch run says early stopping;
`tune_scorings: 3120` beside `models_in_final_comparison: 1` and a
hyperparameter sweep says something else. Neither is refused and neither is
judged. The compiler's contribution is that the number exists, which it currently
does not in any framework: **nobody reports how many times they looked at the
validation set**, and it is the quantity that governs how optimistic the selected
model's validation score is.

That, rather than the type checking, may turn out to be the most useful thing in
this note for a deep-learning audience, and it costs one counter and one integer
in a record that already exists.

### 5.2 Nested cross-validation, which is where careful people still get it wrong

The correct protocol is: an outer split gives `Train` and `Holdout`; the outer
`Train` is k-folded for model selection; within each outer fold, an inner k-fold
selects hyperparameters; the `Holdout` is touched once at the very end.

The design expresses it directly, and the role lattice is what prevents the two
common errors:

- **From a `Train` you may carve a `Train` and a `Tune`** (`split_tune`,
  `k_fold`). From a `Holdout` you may carve nothing: it has no splitter, no
  `k_fold`, and exactly one consuming operation. So the outer holdout cannot
  become an inner tuning set, which is the error that produces published numbers
  nobody can reproduce.
- **The inner loop's evaluation part is a `Tune` and not a `Holdout`**, so a
  score computed inside the inner loop cannot be reported as held-out
  performance: `evaluate` does not accept it, and the function that would produce
  the reportable number is the one that consumes the outer holdout.

What it does **not** prevent, and this is §3.5's refused brand showing through:
inside a single callback that has both an outer and an inner part in scope, the
two have the same types and can be confused. Narrow, real, and priced.

### 5.3 Pretrained models, where the honest answer is that nobody has one

A model whose weights somebody else trained may have been trained on your test
set. This is not hypothetical — benchmark contamination is measured, widespread,
and in the case of models trained on web-scale corpora it is effectively certain
for any benchmark published before the model's cutoff.

**The design does not solve this and must not appear to.** The training corpus of
a downloaded checkpoint is not a property of your program, not a property of the
file in the general case, and frequently not known to the people who produced it.
There is no type, no record, and no analysis that recovers it.

What can be done is small and worth doing anyway:

- `models-and-inference.md` §5.1 reads safetensors' `__metadata__` and §5.3 parses
  enough ONNX for a signature. **If the file declares its training data, Science
  records it. If it does not, Science records that it did not**, which is a
  different and more useful thing than silence.
- Evaluating a pretrained model on a `Holdout` is ordinary use of the design: the
  holdout is spent, the evaluation is recorded, and the record says the model's
  training provenance is undeclared. A methods paragraph that reads *"evaluated a
  model of undeclared training provenance on a holdout of 10,303 rows"* is more
  honest than every leaderboard entry currently published, and it took no
  cleverness to produce.
- **No claim is made that the evaluation is clean.** The `Not checked` block of
  the record (§7) says so in as many words, on `statistical-validity.md` §6.3's
  reasoning: a record that states its own limits is not the record that produces
  a halo.

If a convention for declaring training-corpus digests in model metadata ever
emerges, Science should read it and check it. Until then this is a limitation of
the field and the note reports it as one.

### 5.4 Where the payoff actually lands, given that Science does not train

`models-and-inference.md` §7 is unambiguous: Science should not attempt neural
network training in F1 or F2, and should say so on the front page. **This note
does not reopen that**, and a design that assumed a Science training loop would be
designing for a program nobody can write.

So the deep-learning payoff lands in three places, and they are not the training
loop:

1. **Tabular machine learning**, which is where most data science lives and where
   most leakage is actually committed. §7.3's XGBoost and LightGBM bindings, the
   generalised linear models of §7.5, and the preprocessing of §3.3 are the whole
   pipeline, and every one of them is in scope for this design today.
2. **The data boundary of a delegated fine-tune.** `models-and-inference.md` §7.3
   fine-tunes by binding the vendor's training API. The data going in is a
   `Train`, the data it is evaluated on is a `Holdout`, and what happens in
   between is ONNX Runtime's business. The roles survive the FFI boundary because
   they are enforced on this side of it.
3. **Evaluation of any model, trained anywhere.** §5.3.

Stated plainly: for the deep-learning audience this design constrains the
*data*, not the *training*, and it does so because the training is not Science's
to constrain yet. When §7.4's four tripwires are met and training arrives, the
roles are already in place and the loop's signature — `borrowed Train`, `borrowed
Tune`, a `Holdout` it cannot reach — is the one this design has been asking for.
That is worth building now for the same reason `effects.md` reserves in F0 what it
does not implement until F2.

---

## 6. The honest verdict

### 6.1 The case against, at full strength

**This is a library API with a type annotation, not a language feature.** The
mechanism that stops the commonest leak is Decision 3, which *deletes a
function*. Any library in any language can delete `fit_transform`; scikit-learn
could ship a version tomorrow in which the whole-dataset call does not exist. The
type distinction is what makes the deletion enforceable rather than advisory, but
the deletion is doing the work, and a note that claims a compiler feature when the
feature is a naming convention with a type-shaped fence has overstated itself.

**The friction is much higher than `Key`'s, and that is the comparison the design
invites.** `Key` costs one parameter at the call site and nothing else; the
correct program and the incorrect one are the same length. Here the correct
program is genuinely longer — two calls per transform, a role type in every
signature, a `whole()` call before any unsupervised work, an `Examples` wrapper
between the frame and the matrix — and the incorrect program is the shorter one
that every user has already written a thousand times in Python. Friction at that
level does not produce discipline; it produces `frame()` at the top of every
function and a record full of escapes that nobody reads. `statistical-validity.md`
§10 names this as its own adoption risk and prices it at "one
`.map(each.unadjusted())` in the rare case". Here it is not the rare case.

**The half that matters is a runtime check, which any library could have built.**
§3.9 covers the multi-program pipeline, which is how real work is organised, and
it is a metadata convention plus a check at open. It needs no type system and no
compiler. If that is the valuable half — and §1.6 argues it is the common shape —
then the type work is the decoration on the part that is not novel.

**Goodhart, and `declare_split` is where it bites.** Every user with two CSV files
from a collaborator reaches for `declare_split`, writes a true sentence in the
`because:` string, and has satisfied the design without any disjointness having
been established by anybody. The measure is satisfiable by assertion, and the
population that most needs the check is the population most likely to assert.

**And it is a partial fence around a problem with an open side.** §6.4's first
entry — the person who runs the program, reads the holdout number, edits and runs
again — is the dominant mechanism for holdout overfitting in machine learning, and
it passes through every part of this design without touching it.

This is a serious case and the first and second points are the strongest against
anything in the design notes so far.

### 6.2 The answer

**"It is a library API" proves too much, in the same way the sibling's
category-error objection did.** `Key` is a library type. `PValue` is a library
type. `Option` is a library type in most languages that have it. The standard a
type-system feature is held to is not *did the compiler invent the concept* but
*does the language turn an undeclared assumption into a declared one and check the
declaration mechanically, with no new analysis*. This does: the assumption "these
rows were not used to fit" becomes a type, and the check is the ordinary type
checker. The precise novel claim is narrower than "Science prevents leakage" and
it is falsifiable:

> In scikit-learn, pandas, R's caret and tidymodels, Julia's MLJ, and PyTorch,
> `scaler.fit(X)` before the split is a correct, idiomatic, warning-free line of
> code. In Science it does not compile, because there is no value of the right
> type until a split exists.

No library that keeps `fit_transform` can make that claim, and no library without
a type system can make it at all — deleting the function in Python moves the error
to `AttributeError` at run time, which arrives after the data has loaded.

**The friction objection is the real one and it is answered by scope, not by
argument.** The design's friction falls almost entirely on *predictive modelling
with a held-out claim*. A physicist fitting a curve writes `whole(frame)` once and
never sees another role. A program that reads a CSV and prints a summary sees
nothing at all. The friction is proportional to the strength of the claim being
made, which is the right shape for it to have — and it is why `Whole` exists and
why the `Part` interface forwards the read-only half of `Frame` (§3.7). If the
friction is still too high in practice, that is evidence against the feature and
should be read as such rather than answered with more enforcement — the sibling
note's §10 sentence, and it applies here verbatim.

**"The runtime half is not novel" is conceded and is not a defect.** The design's
position is that leakage needs both halves because the work is organised across
programs, and a note that shipped only the elegant half would be designing for
tutorials. That the record half is unglamorous engineering is an argument for
building it, not against.

**Goodhart is answered the way the sibling answered it, and no further.** An
author who games `declare_split` has written down, in a record a reviewer reads,
that the disjointness of their data is an assertion and not a check, together with
their reason for asserting it. That does not reduce the reviewer's work; it makes
the reviewer's question answerable, which it currently is not. That is a smaller
claim than prevention and it is true.

**What is not answered:** §6.4's first entry. Between-run tuning against the
holdout is invisible, it is common, and no version of this design sees it. It
bounds the feature's value and §6.3 is stated inside that bound.

### 6.3 The verdict

> **A feature — larger than its sibling, more valuable than its sibling, and with
> a bill several times its sibling's. Two thirds of it is a library shape that the
> type checker enforces for free; one third is a runtime provenance check that
> carries the multi-program case and is not a type at all.**

On each axis:

**Not a lint.** Like the sibling, the check requires *no compiler analysis*: it is
the ordinary type checker applied to four library types, plus ownership for
`Holdout` and region inference for folds, both of which Science has for other
reasons. A lint computing the same property would need the taint analysis §3.3
rejects, and would be unsound at the first `concat`.

**Novel, as far as I can establish, in one specific respect.** The fit/transform
distinction is universal; scikit-learn invented it and everybody copied it.
`Pipeline` and `cross_val_score` exist precisely to make the correct composition
convenient, and R's recipes/tidymodels goes further than anything else in making
preprocessing part of the resampling unit. **None of them make the incorrect
composition impossible**, because in all of them the whole dataset and a split
part have the same type. Making them different types, and deleting the one-call
form so there is nothing to fit on before a split exists, is not something any of
them does. Neither is consuming the holdout, and neither is counting how many
times the validation set was scored.

**Worth building, at the price asked** — with one qualification the sibling did
not need. Its price was two types and two diagnostics. This one's price is a shape
change across two sections of `stats`, and it must be paid **before** those
sections are written, because retrofitting it means breaking every program written
against the first version. `scientific-libraries.md` §7.5 and §7.7 are catalogues,
not implementations, which is exactly the moment when this costs nothing and two
releases later it costs everything. That timing argument is the strongest
practical reason to act on this note now, and it is the reason its phase is F1 and
not later.

The claim to make in public, narrowed in the manner of `effects.md` §9.2:

> **In Science, a preprocessing step cannot be fitted on data that has not been
> split, and the part of your data you report on is spent when you read it.**

That sentence survives a hostile reading. "Science prevents data leakage" does
not, and should not be written anywhere.

### 6.4 What it cannot catch, stated as a list because the list is the point

- **Tuning against the holdout across runs.** Evaluate, read the number, edit,
  rerun. Two clean programs, one contaminated result. **This is the dominant
  mechanism in practice** and nothing here sees it. It is
  `statistical-validity.md` §1.4's problem and it is unfixable by a compiler.
- **Target leakage** (§1.3). A feature that encodes the label is a fact about
  meaning. Offered as a function the user calls, never as a check.
- **Temporal and group structure** (§1.4, §1.5). The compiler does not know a
  column is a time or a subject. The splitter's identity and the grouping key are
  recorded; whether they were the right ones is not checkable.
- **Near-duplicate rows.** Exact duplicates are counted at split time; near
  duplicates need a distance the compiler cannot choose.
- **Loading the same file twice.** Decision 2 spends a frame, but two `read_csv`
  calls produce two frames, and one can be `whole`d and the other split. This is
  the `unadjusted()` of this design: it typechecks and always will. Both loads
  appear in the run record with the same input digest, which is a signal and not a
  check.
- **Which fold a part came from** (§3.5). Refused with generative brands.
- **Whether a transform fitted on one split was applied to another's parts.** Same
  cause, same refusal.
- **Whether `declare_split`'s claim is true** (§3.7). It is an assertion, recorded
  as one.
- **Pretrained contamination** (§5.3). Unsolved by anybody.
- **Anything outside the program.** A holdout that was already used in the paper
  the dataset came from; a colleague who told you the answer; a leaderboard.
- **Whether the holdout is representative at all.** A random split of a biased
  sample is two biased samples, and the evaluation is honest about the wrong
  population.

---

## 7. The record

### 7.1 Decision 11: no new artefact, and no new subcommand

> **Decision 11.** The build record and the run record of `reproducibility.md` §5
> gain the `split` and `assessment` objects shown in §3.9 and §5.1, under every
> rule that note states — canonical JSON, no floats, the 8 KB cap,
> `--provenance=minimal`, stamped by `data-io.md`'s writers. **`sciencec methods`,
> which `statistical-validity.md` §6.2 introduces, gains a `Data` section.** This
> note proposes no artefact, no format, no stamping policy and no verb of its
> own.

A second mechanism for the same kind of fact is the mistake `effects.md`
Decision 1 names in another domain, and a second subcommand for the same reviewer
would split the one output a referee might actually open.

```
$ sciencec methods ./fit

Data
  split                      prepare.science:14   uniform, 1/5 holdout, key 20260916
                                                  sp:4c1e90ab77d3 over 51,513 rows
                                                  0 exact duplicates across parts
  fitted transforms          fit.science:22       Standardiser, Imputer(Median)
                                                  fitted on the training part
  tuning set scored          3,120 times
  holdout evaluated          once, fit.science:61, 4 models compared
  model provenance           resnet50.onnx declares no training data

Declared, not checked
  (none)

Role discarded
  part.frame()               plots.science:8

Not checked by this record
  whether the holdout was used to guide edits between runs of this program;
  whether any feature encodes the target; whether a random split respects the
  time order or the group structure of these data; whether a model trained
  elsewhere has seen these rows.
```

### 7.2 Decision 12: the disclaimer is emitted by the compiler and cannot be suppressed

> **Decision 12.** The `Not checked by this record` block is emitted by
> `sciencec`, there is no flag to remove it, and the phrase "held-out" never
> appears in generated output without it.

This is `statistical-validity.md` Decision 7 applied to a stronger credential.
"Adjusted for 17 comparisons" is a modest-sounding phrase; "evaluated on held-out
data" is the sentence that decides whether a reviewer believes a number, and a
tool that prints it without its limits has done something worse than print
nothing. `--no-provenance` continues to drop the whole record, which is an honest
act; dropping the caveats while keeping the numbers is the one thing that must not
be possible.

**If this disclaimer is ever made suppressible, the feature should be removed with
it.** That sentence is borrowed deliberately from the sibling note's §10 and it is
meant as literally here.

---

## 8. What this costs, in one place

| Item | Cost | Who pays |
|---|---|---|
| `Train`, `Tune`, `Holdout`, `Whole`; `Part`, `Fitting`, `Assessing` | 4 types, 3 interfaces, ~10 forwarded methods | `stats` |
| `Examples of (Rl, N, P)`, `Assessment of R`, `Fit` unchanged | 2 types | `stats` |
| `split`, `split_by_time`, `split_by_group`, `split_tune`, `whole`, `declare_split`, `read_train`/`read_tune`/`read_holdout` | 9 functions | `stats` |
| Data-dependent transforms become fit / apply (§3.3) | ~20–30 shape changes | `stats` §7.6, §7.7 |
| §7.5's fitting functions take `Examples` | ~30 signatures | `stats` §7.5 |
| §7.5's assessment list split in two; the out-of-sample half written | ~15 new functions, owed anyway | `stats` §7.5 |
| `k_fold`, `leave_one_out`, `stratified_k_fold`, `cross_validate` higher-order | 4 signatures; needs closure types (a standing ask) | `stats` §7.5 |
| `SC0500`–`SC0503` | Four diagnostics — **no new compiler analysis** | Type checker, ownership, resolution |
| `split` and `assessment` objects in the records | Strings and integers; no new artefact | `reproducibility.md` §5's record |
| A `Data` section in `sciencec methods` | A rendering, over bytes that exist | Tooling, F1 |
| The exact-duplicate count at split time | One hash pass over the rows | Runtime, at `split` |

**The cost that is not in the table.** Every notebook-shaped program pays one
extra line — `whole(frame)` or a `split` — before it can fit anything, and the
first thing a new user meets is a type error on the most familiar line of code
they know. `statistical-validity.md`'s equivalent cost was
`.map(each.unadjusted())` in a rare case. This one is in the common case, on line
three, and it is the first thing anybody will complain about.

**Phase.** F1, with `stats` and `data`. Nothing here is needed in F0 and nothing
here constrains F0. It does constrain `scientific-libraries.md` §7.5 and §7.7
before they are implemented, which §6.3 argues is the reason to settle it now.

---

## 9. Diagnostics allocated, and a problem with the block

| Code | True phase | Fires on |
|---|---|---|
| `SC0500` | Types | A fitting function given a part it may not be fitted on — a `Tune` or a `Holdout` reaching `fit`, or a `Frame` reaching one without `whole` or a split |
| `SC0501` | Types | An out-of-sample claim on a part the fit was made from — a `Train` or a `Whole` reaching `evaluate`, `score` or an out-of-sample metric |
| `SC0502` | Ownership | A `Holdout` read a second time — specialises `SC0301` use-after-move with the statistical reason |
| `SC0503` | Resolution, warning | A **published** signature that takes a `Frame of R` and returns a `Fit` — a library function that forces its callers to fit on unsplit data |
| `SC0504` | — | **Returned unused.** |

`SC0503` is modelled on `stdlib-standard.md` §5.3's `SC0270`, which warns when a
published signature takes a `Stream`, and for the same reason: a library that
takes unsplit data makes every caller's split meaningless, and the author of the
library is the one person who can fix it.

`SC0500`'s rendered form, which is the one that carries the teaching, because
`llm-ergonomics.md` §1 says the diagnostic is the only teaching channel this
language has and because a model trained on scikit-learn will write the first line
of it every time:

```
error[SC0500]: a preprocessing step fitted on data that has not been split
  --> pipeline/prepare.science:18:31
   |
14 |     let frame, err be read_csv of Measurement(path, options).collect()
   |         ----- a `Frame of Measurement` — every row, including the rows
   |               you are about to hold out
...
18 |     let scaler be Standardiser.fit(frame)
   |                                    ^^^^^ expected something `Fitting`;
   |                                          `Frame of Measurement` is not
   |
   = a standardiser fitted here would subtract a mean computed from the held-out
     rows, so the held-out rows would have influenced the training features —
     this is the commonest form of train/test leakage and it is silent
   = split first, then fit on the training part and apply to both:
         let train, holdout be split(frame, key, 0.2)
         let scaler be Standardiser.fit(train)
         let train be scaler.apply(train)
         let holdout be scaler.apply(holdout)
   = if there is no held-out evaluation in this program — an unsupervised
     description of the whole sample, or a figure — say so:
         let all be stats.whole(frame)
   = if the split was made elsewhere and these are its parts:
         stats.declare_split(train_frame, holdout_frame, because: "…")
   = this check does not establish that the split respects the time order or the
     group structure of these data, nor that any feature encodes the target.
     `sciencec methods` prints what was and was not checked.
```

The last note is `SC0500`'s share of the anti-halo work that §7.2 does for the
record, and it belongs in the diagnostic for the sibling note's reason: a language
model that reads only "leakage prevented" will report "leakage prevented".

### 9.1 The block is in the wrong phase range, and this note cannot fix it

`README.md` allocates `SC0500`–`SC0504` from the **resolution** range
(`SC0200`–`SC0249`). Three of the four codes above are not resolution
diagnostics: two are type errors and one is an ownership error. The types range
`SC0250`–`SC0299` has exactly two free codes, `SC0250` and `SC0299`, so the block
could not have been taken from there.

`statistical-validity.md` §5.3 makes a point of its codes sitting in the types
range "for that reason and not by courtesy", so this note cannot quietly do the
opposite.

> **Ask.** Either bless the exception — recording in `README.md` that `SC0500`–
> `SC0503` are numbered in the resolution range and reported in the phases named
> in the table above — or extend the types range and renumber. This note does not
> choose, because the allocation table is `README.md`'s and four collisions in one
> day is the argument against a note deciding such a thing for itself. Until it is
> settled, the numbers above are the ones allocated and the phases above are the
> ones that are true.

---

## 10. What this note asks of others

1. **Of `scientific-libraries.md` §7.5 — the largest ask in the note.** The
   fitting functions take `Examples of (Rl, N, P)`; `k_fold`, `leave_one_out` and
   `cross_validate` become higher-order (§3.5); the "Model assessment" list splits
   into in-sample diagnostics and out-of-sample assessment (§3.6); and the
   out-of-sample half — `rmse`, `mae`, `accuracy`, `roc_auc`, `log_loss`,
   `brier`, `confusion_matrix`, `calibration_curve` — is written, because it is
   currently absent and a machine-learning audience will notice its absence
   before it notices anything in this note.
2. **Of `scientific-libraries.md` §7.6 and §7.7.** The transforms in §3.3's table
   become two-step. `fill_missing`'s statistical strategies move off `Frame` and
   into `Imputer`; the constant and directional fills stay. §7.7's clustering and
   manifold methods take a `Whole` or a `Train` and keep their one-call form,
   with the §3.3.1 caveat in their documentation.
3. **Of `data-io.md` §4 and §8.** Nothing structural. `Part` wraps `Frame of R`
   and forwards a read-only subset of its surface; the writers already stamp the
   run record and the `split` object rides in it unchanged. The one new thing is
   that `stats`'s `read_train`/`read_tune`/`read_holdout` read the stamp at open,
   in the manner of §5.1's schema check, and fail there rather than later.
4. **Of `reproducibility.md` §5.** A `split` object in the run record and an
   `assessment` object beside it, under Decisions 10–13 unchanged. Both are
   strings and integers; `holdout_fraction` is a ratio string, for §5.1's
   no-floats rule. The one new constraint is §7.2's: nothing may drop the caveats
   while keeping the numbers.
5. **Of `statistical-validity.md`.** Three corrections, stated rather than made,
   since that file belongs to another note. §4.3's sketch names `Test of R`
   (§3.1 renames it `Holdout of R`), omits the validation role (§3.1 adds
   `Tune`), and assigns §7.5's whole assessment vocabulary to the held-out part
   (§3.6 shows that most of it is in-sample by definition and requires the
   training part). §4.3's core judgement — that this is more static than multiple
   comparisons, that the mechanism is `Key`'s and not taint analysis, and that it
   should be ranked above that note — is adopted without change.
6. **Of `effects.md`.** **Nothing**, specifically: no fourth bit for fitting or
   for leakage, and §3.1's "the set is closed" stands unamended on this account.
   Written down so the refusal is visible rather than unconsidered (§3.8).
7. **Of `models-and-inference.md`.** Two small things. §5.1 and §5.3 already
   parse model metadata; record a declared training corpus if the file has one and
   record its absence if it does not (§5.3). And §7.3's delegated fine-tuning
   takes a `Train` and is evaluated on a `Holdout`, which costs that section a
   parameter type and nothing else. **§7's decision that Science does not train
   neural networks is not reopened** (§5.4).
8. **Of `stdlib-standard.md` §5.** Nothing. `Key` is used exactly as specified,
   consumed by `split`, and §5.4's rule — *will anyone ever need this exact number
   again* — answers "yes" for every split, which is why `split` takes a `Key` and
   not a `Stream`. Recorded as a positive: the precedent this note copies needed
   no changes to be copied.
9. **Of `reserved-words.md`.** Nothing. §3.1 adds no reserved word, and renames
   `Test` to `Holdout` partly to stay clear of `stdlib-standard.md` §7.1's new
   `test` item form.
10. **Of the core spec §1.** Nothing yet. If this ships, the sentence to add is
    §6.3's, in that wording only.
11. **Of `README.md`.** A row in the notes index, a row in the allocation table,
    `SC0504` returned to the free list, and the phase-range ruling §9.1 asks for.
    None of them can be made from here.
12. **Of whoever owns closure types.** `k_fold` and `cross_validate` need
    `function(T, U) -> S` in a signature, which is `README.md`'s standing
    cross-note ask with three existing customers. This note is the fourth and its
    §3.5 does not work without it.

---

## 11. Risks

**The friction is the feature's biggest enemy and the note may have
underestimated it.** §6.1 states the case; §6.2 answers it with scope. The
measurable form of the worry is `Examples.from_matrix(…, because:)` (§4.3): if
simulation-heavy users reach for it on every fit, the record fills with routine
escapes, the escapes stop carrying information, and what remains is a tax. **That
would be evidence against the feature and should be read as such**, not answered
with more enforcement.

**`part.frame()` becoming idiomatic.** The failure mode to watch for is
`frame()` appearing in tutorials, in generated code and in the first answer on a
forum, at which point the roles have been routed around by convention and only the
record survives. The forwarded methods on `Part` (§3.1) exist to make that
unnecessary; if they are incomplete, users will reach for the escape and the
design will have lost by ergonomics rather than by argument.

**A language model writing Science will write `scaler.fit(X)` before the split,
because ten million lines of Python taught it to.** This is
`llm-ergonomics.md` §1's case and `SC0500`'s four suggestions are the whole
response. If those suggestions are not applicable in the `Suggestion` sense — one
keystroke to apply — the feature reads to a model as an obstacle rather than a
lesson, and models write a large and growing fraction of the code this language
will ever contain.

**The halo, which is what would make this net negative.** "Evaluated on held-out
data, checked by the compiler" is a stronger-sounding credential than anything the
sibling note could produce, and it is true of a narrower thing than it sounds.
§7.2 is the mitigation and it is a sentence the compiler prints, which is exactly
the kind of thing that gets trimmed later for looking noisy.

**The timing risk runs the other way from the usual one.** Most design notes can
be deferred cheaply. This one cannot: `scientific-libraries.md` §7.5 and §7.7 are
unwritten catalogues today and a shape change costs nothing; once they are
implemented and have users, the same change breaks every program. Deferring this
note is not a neutral act, and that should be said to whoever schedules it.

**The record's value depends on somebody reading it.** The compiler's
contribution ends at making the split's identity, the scoring counts and the
escapes exist in a form a referee could use. Whether the practice of machine
learning uses them is outside this note, outside this language, and should not be
claimed by either.
