# Science — Design: what the compiler can and cannot say about statistical validity

Date: 2026-09-16
Status: draft for review
Phase: F1, with `stats` — no F0 compiler work is required and none is asked for
Depends on: `scientific-libraries.md` §7 (the `stats` catalogue this constrains),
`effects.md` (§3.1's closed bit set and §6's pattern for refusing an effect),
`reproducibility.md` §5 (the provenance record §6 here writes into, and which
this note adds fields to rather than duplicating),
`stdlib-standard.md` §5 (`Key`, which is the precedent this note copies),
`native-dependencies.md` §5.4 (`.science.provenance`),
`data-io.md` (`Frame`, for §4.3), the core spec §1 (the verification bet) and §9
(diagnostics).
Syntax: `syntax-revision-2.md` throughout.
Diagnostics claimed: **`SC0288`–`SC0289`**, the block `README.md` allocates.
This note asks for no further codes and returns none.

---

## 0. The verdict up front

The question put to this note was whether a compiler could track how many
hypotheses a program tests and refuse to let an uncorrected p-value be reported
as significant.

**As posed, no.** The count is not static, the *family* the count belongs to is
not in the program at all, and the difference between exploration and
confirmation is not in the program either. A compiler that claimed to count
hypotheses would be claiming to read a study design out of a source file, and it
would be wrong about it often enough to be switched off.

**A smaller thing is true, cheap, and as far as I can find unprecedented.**

> A p-value that has passed through a correction and a p-value that has not are
> **different types**. Only the first one answers the question *is this
> significant*. Therefore a significance claim on an uncorrected p-value does not
> compile, and the number of tests the claim was corrected for is a value the
> compiler can put in the build's record.

That is the whole feature. It requires no new compiler machinery: it is two
library types in `stats` and two diagnostics, and it works because the type
checker Science is already building does the work. The headline is not *the
compiler refuses the error* — §2.5 shows that it cannot, because a p-value must
be convertible to a number and the conversion is one call away. The honest
headline is:

> **The compiler makes the error impossible to commit silently.**

The silence is what matters. The current state of the art in every language this
audience uses — R, Python, Julia, Stata, MATLAB — is that a p-value is a bare
float, `p < 0.05` is a comparison of two floats, and nothing anywhere in the
toolchain records how many of them there were. Making the family size a value
that must be written down, and that the compiler then prints in a record a
reviewer can read, is a real change even though it is a small one.

Everything the mechanism *cannot* see is inventoried in §5.4, and the inventory
is longer than the list of things it can. That asymmetry is not a defect of this
design; it is the honest shape of the problem, and §6 makes the compiler say so
in its own output rather than letting the user infer a guarantee that was never
made.

---

## 1. What is actually static

The claim being tested bundles four separate properties together. They have four
different answers and the bundle is why the idea looks more ambitious than it is.

### 1.1 The number of tests: not static, and not needed

In a straight-line program the number of calls to `t_test_two_sample` is a count
of AST nodes. In the program people actually write it is not:

```science
for column in frame.columns():
    let result, err be t_test_two_sample(control[column], treated[column], TwoSided)
    if err?:
        continue
    pvalues.push(result.p_value)
```

`frame.columns()` is data-dependent. The count is `Frame`'s column count at
runtime, and no analysis recovers it. Worse, the `continue` means the count is
not even the column count — it is the column count minus however many tests
errored, which depends on the values.

A design that needs the count is therefore dead on arrival for the most common
shape of real analysis code, and the failure mode is the bad one: it works on
toy examples and stops working on the first realistic program.

**But the count is not needed.** This is the central technical result of this
note and it dissolves the hard part.

`scientific-libraries.md` §7.4's corrections do not take a count. They take a
collection:

```science
function holm(pvalues: borrowed Array of PValue) -> Array of AdjustedPValue
```

`n` is `pvalues.length()`, computed at runtime, by the function that needs it,
from the collection the author handed it. The compiler never has to know `n`.
What the compiler has to know is something else entirely:

> **Did this p-value pass through a correction before it was compared to a
> threshold?**

That is a question about *flow*, not about *count*. Flow through a program is
what a type system tracks, and it tracks it exactly, in loops, across function
boundaries, and under data dependence. The loop above is not a problem for the
flow question at all — every `PValue` pushed into `pvalues` reaches `holm` on
every path, however many there are.

So: **counting is undecidable and unnecessary; flow is decidable and
sufficient.** The whole design follows from that sentence.

### 1.2 Whether a p-value was corrected: static, and it is type checking

Given two types — one for a raw p-value, one for an adjusted one — the property
"this value was corrected before it was thresholded" is not a dataflow analysis
at all. It is the ordinary question *does this expression typecheck*, and the
answer comes from the machinery the core spec §5 already specifies. There is no
new pass, no fixed point, no lattice, no interprocedural summary.

That is worth dwelling on, because it is the reason the verdict in §5 is
*feature* and not *lint*. A lint that checked this property would have to
reimplement, badly and on the AST, what the type checker gets for free from the
signature of `holm`.

### 1.3 Whether the tests are independent: not static, and not even the right kind of question

The choice between Bonferroni and Benjamini-Hochberg turns on whether the tests
are independent or positively dependent, and on whether the author wants to
control the family-wise error rate or the false discovery rate. The first is a
claim about the data-generating process. The second is a claim about what kind
of mistake the author would rather make.

Neither is in the source, neither is recoverable from the source, and neither
should be guessed at. The compiler's entire contribution here is to **record
which correction was chosen**, so that a reviewer can ask why. §6 does that.

There is a tempting middle position — the compiler warns when `bonferroni` is
applied to tests over columns of the same `Frame`, since columns of one table are
usually correlated — and it should be named so it can be rejected. It is a
heuristic over a proxy (shared provenance) for a fact (statistical dependence)
that the proxy does not determine. Columns of one frame are often independent and
columns of two frames are often not. A check that is right most of the time about
a question that only matters in the minority of cases where it is wrong is worse
than no check, because it trains people to dismiss its output.

### 1.4 Whether the hypotheses were chosen before seeing the data: not in the program, ever

This is the limit, and it needs stating without hedging.

HARKing — hypothesising after the results are known — leaves no trace in a source
file. The program that tests one pre-registered hypothesis and the program that
tests the one hypothesis that survived an unrecorded afternoon of looking at
scatter plots are, character for character, the same program. So are the program
that reports the analysis that worked and the nineteen programs that were deleted.

The same is true of the garden of forking paths, of optional stopping, of seed
shopping, and of the file drawer. Each of them is a decision made *between* runs,
or *before* the first line was written, and the compiler sees one program at one
moment.

**This note therefore claims nothing about HARKing and its output must not imply
otherwise.** That is not a footnote — it is the reason §6 makes the record's
disclaimer mandatory and non-suppressible, and it is the first thing §5.1 uses to
argue against this note's own conclusion.

### 1.5 The scoreboard

| Property | Static? | Where it lands |
|---|---|---|
| How many tests this program runs | No, in any loop | Not needed — §1.1 |
| Whether a p-value reached a correction before a threshold | **Yes** — it is typing | `SC0288`, §2.1 |
| Whether a correction was applied twice | **Yes** — it is typing | `SC0289`, §2.4 |
| How many p-values a given correction covered | At runtime, by the correction | The record, §6 |
| Whether the tests are independent | No | Recorded, never checked — §1.3 |
| Whether the family is correctly scoped | No | Recorded, never checked — §2.2 |
| Whether the hypotheses preceded the data | **Never** | §1.4, and said out loud in §6.3 |

---

## 2. The mechanism

### 2.1 Decision 1: a corrected p-value and an uncorrected one are different types

> **Decision 1.** `stats` defines two types, `PValue` and `AdjustedPValue`. Every
> hypothesis test in §7.4 yields a `PValue`. Every correction in §7.4 maps
> `Array of PValue` to `Array of AdjustedPValue`. **Only `AdjustedPValue`
> answers a question about significance.** Neither type has an ordering against
> `F64`.

```science
type PValue
type AdjustedPValue

PValue has:
    ## The number, with no claim attached. Printing a raw p-value is correct
    ## and common; this is what `Display` uses.
    function unadjusted(self) -> F64

AdjustedPValue has:
    function value(self) -> F64
    function is_significant_at(self, alpha: F64) -> Bool

    ## What it was adjusted against, carried for the record (§6).
    function correction(self) -> Correction
    function family_size(self) -> U64
```

Both implement `Display` and `Ord` — a p-value must print, and Holm, Hochberg and
Benjamini-Hochberg all sort. Neither implements any comparison against `F64`.

`TestResult` from §7.9 gains `p_value: PValue` rather than `p_value: F64`, which
is the only change forced on the ~40 test functions in §7.4: their return type is
already `(TestResult, StatsError?)` and it is unchanged.

What this makes impossible:

```science
let result, err be t_test_two_sample(control, treated, TwoSided)
if result.p_value < 0.05:            # error[SC0288]
    print("significant")
```

What it makes necessary:

```science
let mutable pvalues be Array of PValue.new()
for column in frame.columns():
    let result, err be t_test_two_sample(control[column], treated[column], TwoSided)
    if err?:
        continue
    pvalues.push(result.p_value)

let adjusted be benjamini_hochberg(pvalues)
for p in adjusted:
    if p.is_significant_at(0.05):
        print(f"{p}")
```

Three things fall out that are worth naming separately, because each is doing
work the type distinction alone does not obviously buy.

**The family becomes a value with a name.** `pvalues` is the family. It is a
variable, the author built it, and its size is `pvalues.length()`. The author
cannot claim significance without having assembled that collection, which means
the family is something written down rather than something assumed.

**`alpha` becomes an argument rather than a literal in a comparison.**
`is_significant_at(0.05)` has exactly one syntactic shape, so there is exactly
one kind of site the record in §6 has to find. `p < 0.05`, `0.05 > p`,
`p <= alpha` and `!(p >= alpha)` are four shapes and a record that hunted for
them would miss the fifth.

**The dynamic case is not a special case.** The loop above has an unknown trip
count, a `continue`, and an error path, and none of it matters. Every `PValue` in
`pvalues` reaches `benjamini_hochberg`, and the type of what comes out is what
licenses the claim. This is the §1.1 result made concrete: nothing counts.

**Rejected: a single `PValue` type with a boolean `adjusted` field.** A runtime
flag checked at `is_significant_at`, panicking or erroring when false. It moves
the check from compile time to run time, which is the direction the core spec §1
exists to move things away from, and it makes the failure appear in the middle of
an eight-hour job rather than in `sciencec`.

**Rejected: `PValue` as a newtype over `F64` implementing `Float`.** Convenient —
`mean(pvalues)` would work, and §5.9's reductions would apply. It is also
self-defeating: if `PValue` is a `Float`, then `p < 0.05` typechecks and there is
no check. The cost of the rejection is real and is priced in §7.

**Rejected: making the correction a parameter of the test.**
`t_test_two_sample(a, b, TwoSided, correct: Holm)` is tidier at the call site and
wrong, because a correction is a function of the whole family and a single test
does not know the family. It would produce a function that could only ever
correct for one, which is the exact error the note exists to prevent, wearing the
costume of a fix.

### 2.2 Decision 2: no `study` scope, and the reason is a cost-benefit argument rather than a technical one

A scope construct is the obvious next step and it was worked through before being
rejected. It would look like this:

```science
study "primary endpoint":
    let a be t_test_two_sample(control, low, TwoSided)
    let b be t_test_two_sample(control, high, TwoSided)
    let adjusted be holm([a.p_value, b.p_value])
```

with a compiler check that **every `PValue` created inside the scope reaches the
correction on every path**. That check is decidable: it is a forward escape
analysis of the same shape as the region inference core spec §3 already commits
to, conservative where it cannot prove reachability, interprocedural by a summary
per function ("creates *k*, returns *j*; the difference escaped").

It closes a gap Decision 1 leaves open. Under Decision 1 alone, an author who ran
twenty tests may pass one of them to `bonferroni` and satisfy the type checker
while correcting for a family of one. The scope would catch that.

> **Decision 2. Rejected. Science does not get a `study` scope, or any other
> construct that declares a family to the compiler.**

Four reasons, in descending order of how much they weigh.

**The marginal check is worth much less than the marginal cost.** The scope does
not distinguish an honest author from a dishonest one; it only moves where the
dishonest one writes the small number. An author who would scope the family
correctly is an author who would have passed the right array to `holm`. An author
who would not, declares a small `study`. So the check costs every honest user and
catches nobody, which is the signature of a rule that should not exist. Compare
`Key`: consuming a key by value stops reuse *even when the author is not trying
to be careful*, because the ownership rule fires whether or not anyone is paying
attention. The scope has no such property.

**The family is not the compiler's to know.** §1.3 and §1.4 establish that family
membership is a scientific judgement about a study, not a fact about a
compilation unit. A construct named `study` invites the reading that the compiler
has validated the study. It has validated a block's internal consistency with a
declaration, which is a much weaker thing wearing a much stronger name.

**It costs a reserved word or forty signatures, and both are bad.** `study` as a
keyword collides with an obvious identifier — a meta-analysis over studies wants
`let study be ...`, and `reserved-words.md`'s dot rule saves `x.study` but not a
binding. The value-shaped alternative — a `Family` value that tests register into
and a correction consumes, which is genuinely elegant and reuses the `Key`
ownership pattern exactly — requires an `into: family` parameter on every one of
the ~40 tests in §7.4, or a parallel set of forty signatures. Decision 1 costs one
field on one type.

**It is the only part of the design that produces false positives.** An escape
analysis that must be sound will refuse programs that are fine, and §3's argument
that this checker never fires on legitimate exploration depends on not having it.

**It is source-compatible to add later, and the record makes the case for it
measurable.** If §6's records, gathered across real projects, show people
routinely correcting for families of one or two while the surrounding code runs
dozens of tests, that is evidence and the scope can be designed against it. The
weaker mechanism generates the data that would justify the stronger one. Building
the stronger one first, on no evidence, is the mistake `effects.md` §2.3 refuses
in a different domain and for the same reason.

### 2.3 Decision 3: this is not an effect, and `effects.md`'s closed set stays closed

`effects.md` §3.1 says its three-bit set is closed and calls that "the most
load-bearing sentence in the note". A hypothesis count looks like an effect —
something that accumulates up a call graph — and the temptation to add a fourth
bit is real. It should be refused, and this note is the one that has to refuse it,
because otherwise the effect-system author has to guess.

> **Decision 3. There is no `hypothesis` effect, no `statistical` bit, and no
> request for a fourth. The mechanism is in the type, in the manner of
> `effects.md` §6.1's device residence and §6.2's differentiability.**

Three reasons, each sufficient.

**An effect answers a question nobody asked.** A bit says "somewhere below this
call, a hypothesis test happened". That is true of nearly every function in a
statistics program and it licenses no decision. The question that matters —
*which* p-values, in *which* family — has a payload, and an effect with a payload
is an effect row with a label, which is Koka, which `effects.md` §3.4 declined.

**Effects propagate monotonically; families do not compose.** This is
`effects.md` §6.2's first reason about differentiability, transplanted and still
correct. Two functions that each test a family of three, called from a third, do
not test a family of six unless the author says so — they may be two independent
pre-registered analyses. Monotone accumulation up the call graph would give the
wrong answer and give it confidently.

**The type already carries it, and Decision 1 of `effects.md` forbids the
duplicate.** *"No effect bit duplicates a fact a signature already carries."*
`-> Array of AdjustedPValue` is the fact. A bit beside it would be a second
spelling.

There is one thing this note would *use* if the effect infrastructure offers it
cheaply, and it is a convenience rather than a dependency: §6's record needs to
know which functions in `stats` are hypothesis tests, and a registry is one way.
A `##` doc annotation or a hard-coded list in the `stats` crate is another, and
either is fine. **This is not an ask.** It is written down so that it is on record
as not being one.

### 2.4 Decision 4: a second correction is an error with its own message

Applying two corrections in sequence — `bonferroni(holm(raw))` — controls nothing
with a name. It is already rejected by Decision 1's signatures, since
`bonferroni` wants `Array of PValue` and `holm` produced `Array of
AdjustedPValue`. But the diagnostic that falls out of that is a generic type
mismatch, and `llm-ergonomics.md`'s thesis is that a generic mismatch teaches
nothing to the reader who most needs teaching.

> **Decision 4.** `SC0289` is a dedicated diagnostic for a correction applied to
> an already-adjusted p-value, specialising the type error and naming the
> statistical reason.

### 2.5 Decision 5: the escape is named, and the name is the feature

`PValue` must be convertible to `F64`. Fisher's method combines raw p-values;
meta-analysis needs them; a histogram of p-values is a standard diagnostic plot;
a user calibrating a novel test needs the number. Refusing the conversion would
make `stats` unusable and would push the work into scipy through the FFI, which
is the adoption failure §10 names.

So the conversion exists, and therefore **the check is a speed bump, not a wall.**
`p.unadjusted() < 0.05` typechecks and always will.

> **Decision 5.** The conversion out of `PValue` is spelled `unadjusted()`, the
> escape hatch for a genuinely single pre-specified hypothesis is
> `stats.without_correction(p, because: String) -> AdjustedPValue`, and **every
> use of either appears in the record of §6 with its span and, where given, its
> reason.** Neither is suppressible and neither is silent.

```science
## A single pre-registered endpoint. The family is one, and saying so is the
## point — the string is not read by the compiler, it is carried to the record.
let primary be stats.without_correction(
    result.p_value,
    because: "pre-registered primary endpoint, OSF registration abc123",
)
if primary.is_significant_at(0.05):
    print("primary endpoint met")
```

Three things about this.

**`without_correction` returning an `AdjustedPValue` is a visible contradiction,
and that is deliberate.** A reader who notices it has understood what the line
claims. A name like `preregistered` would read as a credential and would be
slapped on everything.

**The `because:` string is never parsed.** The compiler does not check it, cannot
check it, and must not appear to. It is carried verbatim into the record, where a
reviewer reads it. That is the same division of labour as the whole note: the
compiler moves the claim somewhere it can be seen; a human judges it.

**Rejected: a heuristic on `unadjusted()`.** The obvious tightening is to fire
`SC0288` when the result of `unadjusted()` is compared against a literal in the
0.001–0.1 range, which would catch the lazy escape. It is a magic-number
heuristic about the author's intent, it would fire on calibration code that is
entirely correct, and the first time it does the project acquires a reputation
for a checker that guesses. Rejected. The record is the answer instead: an
`unadjusted()` on a path that reaches a branch is listed, and a reviewer sees it.

### 2.6 What the diagnostics look like

`SC0288`, the one that matters. Note what its secondary span does *not* say: it
does not claim to know how many tests ran, because §1.1 says it cannot.

```
error[SC0288]: a significance claim on an uncorrected p-value
  --> analysis/endpoints.science:34:12
   |
31 |         let result, err be t_test_two_sample(control[c], treated[c], TwoSided)
   |                            ---------------------------------------------------
   |                            `result.p_value` is a `PValue` — it has not been
   |                            corrected for multiple comparisons
...
34 |         if result.p_value < 0.05:
   |            ^^^^^^^^^^^^^^^^^^^^^ a `PValue` has no ordering against `F64`
   |
   = only an `AdjustedPValue` answers `is_significant_at`, and the only way to
     obtain one is a correction from `stats` (`holm`, `benjamini_hochberg`, …)
   = if you are claiming significance, collect the family and correct it:
         let adjusted be holm(pvalues)
         if adjusted.get(i).is_significant_at(0.05):
   = if you are screening candidates for a later confirmatory analysis:
         let candidates be stats.screen(pvalues, below: 0.05)
   = if this is one pre-specified hypothesis, say so and say why:
         stats.without_correction(result.p_value, because: "…")
   = this check does not establish that the hypotheses were specified before the
     data were seen, nor that the family is correctly scoped. `sciencec methods`
     prints what was and was not checked.
```

The last line is not decoration. It is `SC0288`'s share of the anti-halo work
that §6.3 does for the record, and it belongs in the diagnostic because the
diagnostic is what a language model reads (`llm-ergonomics.md` §1) and a model
that reads only "corrected" will report "corrected".

`SC0289`:

```
error[SC0289]: this p-value has already been corrected
  --> analysis/endpoints.science:41:30
   |
39 |     let once be holm(raw)
   |                 --------- `holm` returned `Array of AdjustedPValue`
40 |
41 |     let twice be bonferroni(once)
   |                             ^^^^ expected `Array of PValue`
   |
   = two corrections in sequence control neither the family-wise error rate at
     α nor the false discovery rate at q, and the rate they do control has no
     name and no closed form
   = to compare two corrections of the same family, correct the raw p-values
     twice:
         let by_holm be holm(raw)
         let by_bonferroni be bonferroni(raw)
```

---

## 3. The false-positive problem, which is what decides adoption

A checker that fires on legitimate exploratory analysis is one that everyone
switches off, and it takes one bad experience per user. So this section is the
one the design lives or dies on.

### 3.1 The answer is structural, and it is the best property this design has

> **The check fires on the act of *claiming* significance, not on the act of
> *running* a test.**

Exploratory work runs many tests and claims nothing. That is what makes it
exploratory. It is therefore invisible to this checker **for free, with no flag,
no annotation, no mode, and nothing for the user to declare.** Consider the
honest exploratory script:

```science
for column in frame.columns():
    let result, err be t_test_two_sample(control[column], treated[column], TwoSided)
    if err?:
        continue
    print(f"{column}: p = {result.p_value}, d = {result.effect_size}")
```

Zero diagnostics. `PValue` implements `Display`; printing is correct; nothing is
thresholded. Two hundred tests, no output from the compiler.

This matters because the usual design for this problem is a mode — `--strict`, or
an `exploratory:` block, or a flag in the manifest — and every such design asks
the user to tell the compiler which kind of work they are doing. That is exactly
the question §1.4 established is not answerable from the program, and asking it
produces one of two outcomes: everyone sets the flag to exploratory, or people
set it to confirmatory and lie. **The design that does not ask the question
cannot be lied to about it.**

### 3.2 The hard case, stated honestly

There is one case where the checker fires on work that is not wrong, and it
should not be hidden:

```science
    if result.p_value < 0.05:          # error[SC0288]
        print(column)
```

The author means *show me the interesting columns*, not *these columns are
significant*. That is legitimate screening and the checker fires on it.

The answer is not a suppression. It is a differently named operation that says
what is happening:

```science
let candidates be stats.screen(pvalues, below: 0.05)
```

`screen` returns `Array of PValue` — **screening does not launder.** A screened
p-value is still uncorrected, still unthresholdable, and still cannot carry a
significance claim into a figure caption. All `screen` does is let the author
filter without asserting, and let the record say that filtering happened at 0.05
over a family of that size — which is information a reviewer of a two-stage
design wants and currently has no way to obtain.

The fix is one call, the call is honest, and nobody has to say anything untrue to
the compiler. That is the bar §3 was asked to clear.

### 3.3 The escape hatch, and whether using it is visible

Every system of this kind needs a hatch. Two exist —
`stats.without_correction(p, because:)` and `p.unadjusted()` — and Decision 5
settles their character: **available without friction, impossible to use
invisibly.**

| | Silenceable? | In the record? | Reads as what it is? |
|---|---|---|---|
| `stats.screen(…, below:)` | n/a — not an escape | yes, with the threshold and family size | yes |
| `stats.without_correction(…, because:)` | no | yes, with the span and the reason string | yes — the name contradicts the return type |
| `p.unadjusted()` | no | yes, with the span | yes |
| A `#[allow]`-style suppression | — | — | **does not exist; see below** |

**Rejected: a suppression attribute.** Rust's `#[allow]`, C's `-Wno-`, and every
lint-disable comment in every language share a property this design cannot
afford: the suppression is *local to the source*, so the person reading the
paper never sees it. A hatch that appears in the record is a hatch a reviewer can
find. A hatch that silences a diagnostic is a hatch that only the author knows
about. Since the entire value of this feature is that a third party can see what
was done, a silencing mechanism would negate it. There is no `--no-statistical-
checks` flag either, for the same reason.

The cost of having no suppression is that a user who is genuinely stuck must
write `unadjusted()`, which is three characters longer than they would like and
appears in a record they may not want it in. That is the intended pressure and it
is mild.

---

## 4. Whether this generalises

Four candidates were put to this note. One dissolves into an API decision, one
should not be attempted, one is out of scope but is the strongest idea in the
neighbourhood, and one is already answered elsewhere.

### 4.1 A p-value reported without an effect size or a confidence interval — dissolves into `stats`'s API

Is it static? Only in a useless form. "Reporting" is `print`, or writing a CSV, or
returning a value to a notebook cell, and the compiler cannot distinguish a
formatted results table from a debug trace. The weaker checkable form — *was an
effect size ever computed from the same two samples* — is answerable and nearly
worthless, since computing is not reporting.

But the problem has a much better home.

> **Ask of `scientific-libraries.md` §7.4.** `TestResult` carries `effect_size`
> and `confidence_interval` as **non-optional fields**, computed by the test
> function, for every test where the quantity is defined.

Then a p-value without an effect size does not exist as a value — the author has
both in hand whether or not they wanted them, the marginal cost of reporting both
is zero, and §6's record emits all three automatically. A design problem became a
struct definition, which is the best outcome available. **Verdict: not a compiler
check; a library decision; recommended.**

### 4.2 A test applied to data violating its assumptions — should not be attempted

Is it static? No. Normality is a property of values.

And the natural approximation is actively harmful. A compiler rule of the form
*call `shapiro_wilk` before `t_test_two_sample`* would mandate a preliminary
assumption test on the same data, which is a well-documented statistical error in
its own right: conditioning the choice of test on a pre-test over the same sample
changes the operating characteristics of the subsequent test, and the resulting
procedure has a Type I error rate that is neither the nominal α nor anything
easily computed. The compiler would be enforcing bad practice with the authority
of a type error.

> **Verdict: out of range, and worse than out of range — a rule here would be
> wrong.** The correct home is runtime. `TestResult` can carry an `assumptions`
> field recording what the test observed about its own input (sample sizes, a
> skewness figure, a variance ratio), the run writes it into the record, and a
> reader judges. That is a `stats` decision, it is cheap, and it belongs in
> §7.4's `TestResult` beside §4.1's fields.

### 4.3 Fitting and evaluating on the same data — out of scope here, and the best idea in the neighbourhood

This one is genuinely different, and the assessment of it is the most useful thing
in this section.

**It is more static than multiple comparisons, not less.** Leakage is not about
the values in the data at all. It is about *which values flowed where*, and
provenance is a program property in a way that family membership never is. There
is no analogue of §1.4 here: no judgement outside the source decides whether the
array passed to `r_squared` is the array that `least_squares` was fitted on.

The common bug is coarse enough to be syntactic:

```science
let fit, err be least_squares(design, response)
let r2 be r_squared(fit, design, response)      # the same two bindings
```

and the next-coarsest — fitting a standardiser on everything and then splitting —
is a reachability question over binding chains, which is also static.

**The mechanism should not be taint analysis.** A provenance lattice threaded
through every array-returning function in `stats` and `data` is a large
annotation burden and it will be defeated by the first `Array.concat`. The
cheaper design copies `Key` exactly:

> **Sketch, handed off.** A `Split of R` is the only way to obtain disjoint train
> and test views of a `Frame of R`. It is produced by `stats.split(frame, key,
> fraction)` or by `k_fold`, and it yields two *distinct types* —
> `Train of R` and `Test of R` — that are not interconvertible. `fit` accepts a
> `Train of R` and nothing else. `score`, `r_squared`, `confidence_interval` and
> the rest of §7.5's assessment vocabulary accept a `Test of R` and nothing else.
> Leakage stops being an error the compiler detects and becomes a sentence the
> author cannot write.

That is the same move as `Key`: `Key` does not prove that randomness was used
correctly, it makes reuse impossible by consuming the value. `Split` would not
prove that an evaluation is honest, it would make the wrong argument
ill-typed. It is cheaper than taint analysis, it catches the common case
completely, and it is more valuable than this note's own subject for an
ML-facing audience, which is a larger audience than the hypothesis-testing one.

> **This note does not design it and claims no codes for it.** §7.5 already has
> `cross_validate`, `k_fold`, `leave_one_out` and `bootstrap_confidence`, so the
> primitives exist; what is missing is that nothing forces their use, and fixing
> that is a redesign of §7.5's type surface plus an interaction with `data-io.md`'s
> `Frame`. It deserves its own note and its own diagnostic block.
> **Recommendation: write that note, and rank it above this one.**

### 4.4 A result depending on an arbitrary seed — already answered, and the remainder is a record problem

`stdlib-standard.md` §5 has done this work and `reproducibility.md` has finished
it. `Key` is consumed by value so it cannot be reused (`SC0301`); `Stream` is
exclusively borrowed so two call sites cannot interleave; §5.4's rule — *will
anyone ever need this exact number again* — decides which to reach for; `SC0270`
warns when a published signature takes a `Stream`; `effects.md`'s `ambient` bit
marks `Stream.from_entropy`; and `reproducibility.md`'s `SC0237` fires by
default when a value derived from an ambient seed reaches `Key.from_seed`, which
is the clock-seeding case exactly.

**This note therefore asks for nothing here and adds nothing.** The seed appears
in `reproducibility.md` §5.2's run record already, and §6 below reads that record
rather than proposing a second one.

What is left over is not a check in anybody's design. Seed shopping — running
with fifty seeds and reporting the best — is a §1.4 problem: fifty runs, one
program, forty-nine deletions no compiler sees. It is named in §5.4's inventory
and it stays there.

### 4.5 The pattern, since three of four came out the same way

The candidates that survived did so by becoming **types that make the wrong
sentence unwriteable**, not by becoming analyses that detect a wrong sentence
after it is written. `PValue`/`AdjustedPValue`, `Train`/`Test`, `Key`. The ones
that failed — assumption checking, effect-size reporting — failed because the
property was about values or about prose rather than about which value reached
which function.

That is a usable rule for anyone extending this work: **if the candidate error can
be expressed as "the wrong value reached this function", it is in range. If it
requires knowing what the values are, or what the author meant, or what happened
before the file was written, it is not.**

---

## 5. The honest verdict

The instruction was to argue the strongest case against before stating a
conclusion. §5.1 is that case, made as well as I can make it. §5.2 answers it.
§5.3 is the verdict and §5.4 is the inventory of what it does not cover.

### 5.1 The case against: this is a category error and the language should stay out

**The property is about the claim, not about the program.** A program that
computes twenty p-values and prints them is correct. A program that computes
twenty and thresholds one is *also* correct if nineteen were robustness checks
around a single pre-registered hypothesis. The compiler is being asked to
adjudicate a question — what is the family — whose answer is nowhere in the
source, and §2.2 concedes this by refusing to build the scope that would have
asked it.

**The errors it cannot see dominate the ones it can.** §1.4's list — HARKing, the
garden of forking paths, optional stopping, selective reporting, the file drawer
— is, on the best evidence available about the replication crisis, quantitatively
larger than uncorrected multiple comparisons. Each of them is invisible to any
compiler, permanently, for structural reasons that no cleverness removes.

**And therefore the halo is a real harm, not a hypothetical one.** A tool that
catches the visible error while the dominant invisible ones pass conveys a
credential it has not earned. "Compiled with Science's statistical checks" reads,
to a referee skimming a methods section at eleven at night, as *this analysis was
checked*. If the checker's existence causes one referee in a hundred to look less
hard, the feature is net negative regardless of how many `p < 0.05` typos it
caught, because the thing it caught was the cheap error and the thing it
displaced was human scrutiny of the expensive one.

**Goodhart applies with full force.** The check becomes a target. Authors satisfy
it — call `bonferroni`, pass the smallest defensible family — feel certified, and
move on. The measure ceases to be a measure of the thing.

**And a competent statistician does not need it.** The multiple-comparisons
correction is taught in every introductory course. The people making this error
are not making it because their tools did not stop them; they are making it
because of publication incentives that a type system cannot reach. Fixing a tool
to address an incentive problem is a category error twice over.

This is a serious case. Parts of it are correct and are not answered below.

### 5.2 The answer

**The category-error argument, taken at full strength, defeats type checking in
general.** A type system does not establish that a program is correct; it
establishes that the program is consistent with declarations the author made. The
unit system in `unit-literals.md` does not know whether the physics is right —
dimensional analysis never told anyone their equation was true, only that it was
not nonsense. `Key` does not establish that a simulation is sound. Nobody thinks
these are category errors, because the standard they are held to is not *does it
catch the scientific error* but *does it turn an undeclared assumption into a
declared one and then check the declaration mechanically*.

By that standard the multiple-comparisons case has a genuinely checkable core, and
§1.2 identified it precisely: **given a family the author assembled, was every
value that crossed a significance threshold one that passed through a
correction.** That is not a judgement. It is flow, and it is exact.

**The halo argument is the strongest one and it is an argument about output, not
about the check.** It is answered by making the compiler say what it did not
check, in the diagnostic (§2.6's last note) and in the record (§6.3's mandatory,
non-suppressible disclaimer), rather than by declining to check. A tool that
states its own limits is not the tool that produces a halo; a tool that reports
"validated" and stops is. This design has no place that says "validated".

**Goodhart is answered by what satisfying the check requires.** It requires
assembling the family as a named value and choosing a correction by name, and
both then appear in the record. An author who games it by declaring a family of
one has *written down* that they declared a family of one, in a record a reviewer
reads. The check does not reduce the reviewer's work; it makes the reviewer's
question answerable in a way it currently is not. That is a smaller claim than
prevention and it is a true one.

**"A competent statistician does not need it" is the weakest of the objections,**
and it is the argument made against every safety feature by the people least
likely to need it. It also mistakes the audience: the audience for a scientific
programming language is overwhelmingly domain scientists who took two statistics
courses and use the tools their field uses, and `scientific-libraries.md` §7.4's
catalogue exists precisely because they will reach for these functions.

**What is not answered:** the objection that the invisible errors dominate.
That is simply true, and this note does not claim otherwise. It bounds the
feature's value rather than eliminating it, and §5.3 is stated within that bound.

### 5.3 The verdict

> **A feature — and it is a feature precisely because it is small.**
>
> The ambitious version, in which the compiler counts hypotheses and validates a
> study design, is an over-reach and is rejected in §2.2 and §2.3. The modest
> version — two library types, two diagnostics, and a record — is a genuine
> compiler feature, it is novel, and it is nearly free.

On each of the three axes:

**Not a lint, and the reason is unusual.** The check requires *no compiler
analysis at all*. It is the ordinary type checker applied to two library types.
A lint implementing the same property would have to reimplement, on the AST and
imprecisely, what `holm`'s signature states exactly — so building it as a lint
would be strictly more work and strictly less sound. Type systems are how you get
this property cheaply; a linter is how you get it expensively. `SC0288` and
`SC0289` sit in the `SC0250`–`SC0299` *types* range for that reason and not by
courtesy.

**Novel, as far as I can establish.** In R, Python with scipy or statsmodels,
Julia with HypothesisTests, Stata, SAS and MATLAB, a p-value is a bare
floating-point number and `p < 0.05` is a comparison of two floats. The
corrections all exist in every one of those libraries; none of them changes the
*type* of what comes out, and so none of them can distinguish a corrected value
from an uncorrected one at any point afterwards. Making that distinction a type,
and putting the family size in the build's record, is not something any of them
does.

**Worth building, at the price asked.** §7 prices it: one field on `TestResult`,
two types, nine correction signatures, two diagnostics, and a
`sciencec methods` subcommand. Nothing in F0 changes and nothing in the type
checker changes. That is a small enough price that the bounded value in §5.2
clears it comfortably.

The claim to make in public, in the manner of `effects.md` §9.2 narrowing §1's
"effect discipline", is **not** "Science refuses statistically invalid programs".
It is:

> **In Science, a corrected p-value and an uncorrected one are different types,
> and the compiler writes down which one you claimed significance on.**

That sentence is true, it survives a hostile reading, and it is the one a referee
can actually use.

### 5.4 What it cannot catch, stated as a list because the list is the point

- **HARKing.** Hypotheses chosen after seeing the data. Not in the program (§1.4).
- **The garden of forking paths.** Analytic decisions taken after looking. Each
  compiled program is clean; the ones that were deleted were never compiled.
- **Optional stopping.** Collecting until significance appears. Between runs.
- **Seed shopping.** Fifty runs, one reported. Between runs (§4.4).
- **Selective reporting and the file drawer.** Across programs and across projects.
- **Whether the declared family is the right family.** §2.2 refuses to pretend.
- **Whether the correction chosen is the right correction.** Bonferroni versus
  Benjamini-Hochberg turns on independence, which is a claim about the world
  (§1.3). The compiler records the choice and never evaluates it.
- **Power.** An underpowered study that survives a correction is still a bad study,
  and nothing here looks at sample size.
- **Whether the data are what they claim to be.** Out of scope for any compiler.
- **Anything spanning compilation units.** Two programs in one project testing
  related hypotheses are two families to the compiler and one to a reviewer.
- **The `unadjusted()` escape,** which typechecks and always will (§2.5). It is
  recorded, not prevented.

---

## 6. The record, and how it reaches a methods section

The owner's requirement is results that go straight into a publication. If the
compiler knows the correction and the run knows the family size, that is a
methods paragraph nobody has to write by hand — and, more importantly, one that
cannot drift from the code.

### 6.1 Decision 6: no new artefact — this is two fields in the record `reproducibility.md` already designed

`reproducibility.md` §5 designs the record in full: canonical JSON, a `schema`
integer, a build record in `.science.provenance` readable with `readelf` and no
toolchain, a run record embedding it verbatim and stamped into outputs by
`data-io.md`'s writers, an 8 KB cap with a stated degradation order, a
`--provenance=minimal` privacy mode, and the three reviewer verbs of its
Decision 13.

> **Decision 6. There is no statistical-methods artefact. The build record gains
> a `statistics` object and the run record gains a `statistics` object, and every
> rule that note states about format, canonicalisation, size, privacy and
> stamping applies to them unchanged.**

A second mechanism for the same kind of fact is the mistake `effects.md`
Decision 1 names in another domain, and a methods record that lived in a separate
file would be separated from its data by the first person who copied one file —
which is the exact failure `reproducibility.md` §5.3 accepts reluctantly for CSV
and would be inexcusable to choose voluntarily.

**Build record — the structure, and it is float-free.** Which corrections are
called and where, which tests feed them, every `without_correction` with its
`because:` string, every `unadjusted()` call site, every `screen` threshold, and
every `is_significant_at` site. Spans, names and integers only.

**Run record — what only a run knows, and it is also float-free.** The family
sizes as executed, per correction call site. Integers.

**The numbers are not in the record, and that is a decision rather than an
oversight.** `reproducibility.md` §5.1 forbids floating-point values anywhere —
*"a record about floating-point reproducibility that is itself sensitive to float
formatting would be an embarrassment"* — and that rule bites here, because
p-values, effect sizes and confidence-interval bounds are floats. The resolution
is not to encode them as decimal strings. It is that **the record records the
method, not the results.** The results are the program's output and already live
in whatever the program wrote; α is a `F64` literal in the source and is recorded
as the source text of that literal, not as a float.

That constraint improves the design. A record of the method is what a methods
section needs and what a reviewer can check against the code; a record that also
carried the numbers would be a second, unverified copy of the results, and the
first time it disagreed with the paper nobody would know which to believe.

**Size.** The `statistics` object is O(call sites), not O(tests run), so it does
not grow with the data. A program with a hundred correction call sites is
pathological and would be truncated under the 8 KB cap. Its degradation order:
`is_significant_at` sites first, then `screen`, then correction sites; **never
the escapes**, because `without_correction` and `unadjusted()` are the entries a
reviewer most needs and a record that drops its own caveats first is worse than
no record.

### 6.2 `sciencec methods`, a fourth verb over the same bytes

`reproducibility.md` Decision 13 gives a reviewer three verbs — **read** (no
toolchain), **compare** (`sciencec verify`), **rebuild**. `sciencec methods` is a
rendering of the `statistics` object for the first rung's benefit: the record is
already readable with `readelf`, and this is what makes it readable by a
statistician rather than by a build engineer. It reads the same bytes, proves
nothing new, and is a view rather than an artefact.

```
$ sciencec methods ./analysis
Science 0.1.0 · stats 0.1.0 · analysis/endpoints.science

Hypothesis tests reached from `main`
  t_test_two_sample          endpoints.science:31
  mann_whitney               endpoints.science:52

Corrections
  holm                       endpoints.science:38   family assembled at :29
  benjamini_hochberg         endpoints.science:59   family assembled at :50

Significance claims
  is_significant_at(0.05)    endpoints.science:40   on holm output
  is_significant_at(0.10)    endpoints.science:61   on benjamini_hochberg output

Screening (no claim attached)
  screen(below: 0.05)        explore.science:14

Uncorrected escapes
  without_correction         endpoints.science:71
    because: "pre-registered primary endpoint, OSF registration abc123"
  unadjusted()               calibrate.science:22

Randomness                                       (from `reproducibility.md` §5)
  Key.from_seed(20260916)    endpoints.science:12
  no ambient-seeded generator is reachable from `main`

Not checked by this record
  whether the hypotheses were specified before the data were seen;
  whether each family is correctly scoped; whether the tests within a
  family are independent, which is what distinguishes the correction
  applied above from the alternatives; statistical power; anything
  occurring in a run other than this one.
```

Joined with the runtime half, `sciencec methods --draft` emits a paragraph:

> Statistical procedures were generated from the compiled analysis
> (provenance `sc:8f3a21d0`). Group comparisons were performed with Welch's
> two-sample *t*-test. *P*-values
> were adjusted across a family of 17 comparisons using Holm's step-down
> procedure and evaluated at α = 0.05. Effect sizes are reported as Cohen's *d*
> with 95% confidence intervals. Pseudo-random draws derive from a single seed
> (20260916) recorded in the binary. One comparison — the pre-registered primary
> endpoint (OSF registration abc123) — was evaluated without adjustment as a
> family of one.
>
> *This paragraph was generated from the compiled program. It states the family
> sizes and corrections as written in the code. It does not establish that the
> hypotheses were specified before the data were seen, that the families are
> correctly scoped, or that the tests within a family are independent.*

### 6.3 Decision 7: the disclaimer is emitted by the compiler and cannot be suppressed

> **Decision 7.** Both the `Not checked` block and the italicised sentence in the
> draft paragraph are emitted by `sciencec`. There is no flag to remove them, and
> the phrase "family of 17" never appears without the sentence saying what about
> that 17 was not verified.
>
> **Amended.** The paragraph additionally carries the provenance identifier in
> its **first sentence**, not only in the trailing italics.

`publishable-output.md` §7.5 disagreed with the original form of this decision,
and it is right, for a reason that is about people rather than about software:
**authors delete caveats and keep citations.** A trailing italicised sentence
does not survive a paste into a manuscript — it reads as tooling noise, it is
the last thing on the block, and trimming it costs one keystroke.

The two mechanisms defend against different threats and the note now takes both:

| Threat | Defence |
|---|---|
| The *tool* is asked to print the numbers without the limits | Decision 7's non-suppressibility. No flag does it. |
| The *author* trims the paragraph on the way into the manuscript | The identifier in the first sentence, where it reads as a citation. |

Non-suppressible-by-`sciencec` was never non-suppressible-by-a-human, and the
original decision quietly conflated the two. An identifier in the opening
sentence survives because it looks like the thing an author is used to keeping,
and because deleting it visibly damages a sentence rather than removing a
footnote. It is also what makes the claim *checkable*: a reader with the
identifier can run `sciencec verify` against the artefact, which a paragraph of
prose alone never permits.

This is narrower than `reproducibility.md` §5.3's `--no-provenance`, which
suppresses stamping entirely and is documented there as *"a choice with a
consequence"*. That flag continues to work and removes the whole record,
statistics object included; what cannot be done is keep the numbers and drop the
caveat. Suppressing everything is an honest act. Suppressing only the limits is
the one this design has to refuse.

This is the single most important decision in §6 and it is a negative one.
§5.1's halo objection is the strongest argument against this whole note, and this
is where it is paid. A record that says *adjusted for 17 comparisons* and stops is
precisely the credential §5.1 warns about. A record that says the same thing and
then names three things it did not check is a record that makes a referee's job
easier rather than shorter.

**Rejected: prose generated by a model.** The draft paragraph is a template with
holes, filled from the record. A compiler that generated fluent English about a
statistical analysis would generate claims, and the claims would be the ones a
language model finds plausible rather than the ones the compiler verified. A
template can only say what it has slots for.

**Rejected: a machine-readable format of this note's own invention.** Settled by
Decision 6 — `reproducibility.md` §5.1's canonical JSON is the format, and this
note adds fields to it rather than choosing one. What this note does choose is
that the *primary* artefact for this particular consumer is the human-readable
rendering above, because the consumer is a referee and no referee reads canonical
JSON. The field names are chosen to line up with reporting items CONSORT and
STROBE already ask for, so that filling a checklist becomes transcription rather
than recall.

---

## 7. What this costs, in one place

| Item | Cost | Who pays |
|---|---|---|
| `PValue`, `AdjustedPValue` | Two types, ~8 methods | `stats` |
| `TestResult.p_value: PValue` | One field's type | `stats` §7.4 |
| `TestResult.effect_size`, `.confidence_interval`, `.assumptions` | Three non-optional fields, computed per test (§4.1, §4.2) | `stats` §7.4 |
| Nine correction signatures | `Array of PValue -> Array of AdjustedPValue` | `stats` §7.4 |
| `stats.screen`, `stats.without_correction` | Two functions | `stats` |
| `SC0288`, `SC0289` | Two diagnostics with rendered notes and suggestions | Type checker — **no new analysis** |
| A `statistics` object in the build and run records | Two objects of spans, names and integers — no new artefact, no new format (§6.1) | `reproducibility.md` §5's existing record |
| `sciencec methods` | A rendering subcommand over bytes that already exist | Tooling, F1 |

**The cost that is not in the table.** `PValue` is deliberately not a `Float`
(§2.1), so it does not compose with `scientific-libraries.md` §5.9's reductions.
`mean(pvalues)` does not compile; `mean(pvalues.map(each.unadjusted()))` does. The
`.map(each.unadjusted())` idiom will appear in every histogram of p-values and
every Fisher combination anyone writes, and it is mildly annoying every time.
That is the price of the whole feature and there is no version of it that is both
checked and free. It is stated here rather than buried because it is the first
thing a user will complain about.

**Phase.** F1, with `stats`. Nothing here is needed in F0 and nothing here
constrains F0 — which is worth saying explicitly, because a note arguing for a
novel compiler feature that turned out to block the type checker would be a
different and much worse proposal.

---

## 8. Diagnostics allocated

| Code | Phase | Fires on |
|---|---|---|
| `SC0288` | Types | A significance claim on an uncorrected `PValue` — §2.6 |
| `SC0289` | Types | A correction applied to an already-adjusted p-value — §2.4 |

That is the `SC0288`–`SC0289` block `README.md` allocates, used in full, with
nothing further requested. `README.md` invited an ask for more; §2.2 and §2.3
removed the need by rejecting the two mechanisms that would have consumed them,
and §4.3's leakage work should claim its own block in its own note rather than
being pre-reserved here. Given the four collisions `README.md` records from one
day of parallel notes, claiming a range against a design that does not exist yet
is the wrong habit to establish.

**One declared range overlaps, and no code does.** `indexing-and-array-literals.md`
§7.2 is headed *"Types and traits — `SC0280`–`SC0289`"*, and `broadcasting.md`
§9 repeats that range when describing what that note holds. The codes that note
actually defines stop at `SC0287`, which is what `README.md`'s allocation table
records and why its free list offers `SC0288`–`SC0289`. So this is the same
situation `README.md` describes for the codegen range — *"No code actually in use
collided. Only the declared ranges overlapped, which is the cheapest moment to
find such a thing"* — and it is written down here rather than left for someone to
rediscover.

> **Ask.** `indexing-and-array-literals.md` §7.2's heading and
> `broadcasting.md` §9's sentence should read `SC0280`–`SC0287`. Neither note
> loses a code. Neither can be edited from here.

---

## 9. What this note asks of others

1. **Of `scientific-libraries.md` §7.4 — the one substantive ask.** `TestResult`
   carries `p_value: PValue` rather than `p_value: F64`, plus non-optional
   `effect_size`, `confidence_interval` and `assumptions` fields (§4.1, §4.2).
   The nine corrections take `Array of PValue` and return
   `Array of AdjustedPValue`. **This does not contradict §7.9**: the signature
   `t_test_two_sample(...) -> (TestResult, StatsError?)` is unchanged and only
   `TestResult`'s fields move. That section is still written in pre-revision-2
   syntax (`returns`, `Result of (T, E)`); this note uses revision-2 throughout,
   which is drift and not disagreement, per `README.md`'s convention.
2. **Of `scientific-libraries.md` §7.5 and `data-io.md`, jointly.** §4.3's
   `Split` / `Train` / `Test` sketch is handed over and not designed here. It
   needs a note, and this note's recommendation is that it be ranked **above**
   this one: the property is more static, the audience is larger, and the
   mechanism is cheaper.
3. **Of `effects.md`.** §2.3 here is a request for **nothing** — specifically,
   that no fourth bit be added for hypothesis counting, and that §3.1's "the set
   is closed" stand unamended on this account. It is written down so that the
   possibility is visibly declined rather than silently unconsidered. §6.1's
   `ambient` marking in the record uses the bit as it already is.
4. **Of `reproducibility.md` §5.** That the build record and the run record each
   gain a `statistics` object, per §6.1 here, under every rule Decisions 10–13
   already state — canonical JSON, no floats, the 8 KB cap, `--provenance=minimal`,
   stamped by `data-io.md`'s writers. **This note proposes no artefact, no format
   and no stamping policy of its own**; it adds two objects to a record that note
   owns, and §6.1 states the degradation order for them. The one new constraint is
   §6.3's: `--no-provenance` may drop the whole record, and nothing may drop the
   caveats while keeping the family sizes.
5. **Of `indexing-and-array-literals.md` §7.2 and `broadcasting.md` §9.** The
   declared-range correction in §8. No code moves.
6. **Of `stdlib-standard.md` §5 and `reproducibility.md` §3.** Nothing. §4.4 was
   drafted as an ask about seeds in the record and `reproducibility.md`'s
   `SC0237` and §5.2 had already discharged it. Recorded as withdrawn so that
   nobody builds it twice.
7. **Of `reserved-words.md`.** Nothing. §2.2 considered `study` as a keyword and
   rejected it, partly on that note's grounds. Recorded as a positive because a
   note that adds no reserved word should say so.
8. **Of the core spec §1.** Nothing yet. If this ships, the sentence to add is
   §5.3's, and it should be added *only* in that wording — narrow, checkable, and
   about types rather than about validity. `effects.md` §9.2 is the precedent for
   why the narrow claim is also the better one.
9. **Of `README.md`.** A row in the notes index and a row in the allocation
   table, which this note cannot add without editing a sibling's file —
   `effects.md` §11.2 has the same problem and states it the same way.

---

## 10. Risks

**The halo, which is the risk that would make this net negative.** §5.1 states it
at full strength and §6.3 is the mitigation. The mitigation is a sentence the
compiler prints, and sentences the compiler prints are exactly the kind of thing
that gets trimmed later for looking noisy. **If the disclaimer is ever made
suppressible, this feature should be removed with it.** That is the strongest
sentence in this note and it is meant literally.

**`stats` becomes harder to use than scipy, and people route around it.** This is
the real adoption risk and it is not the compiler's to control. Science has an FFI
and a Python story; a user who finds `PValue` annoying calls `scipy.stats` and
gets a float. Nothing stops them and nothing should. The defence is that the
friction is genuinely small — one `.map(each.unadjusted())` in the rare case, one
named call in the common one — and that the record is worth something on its own.
If the friction turns out not to be small in practice, that is evidence against
the feature and it should be read as such rather than answered with more
enforcement.

**Two p-value types leak into generic numeric code.** Priced in §7. The failure
mode to watch for is `unadjusted()` becoming idiomatic — appearing in tutorials,
in generated code, in the first answer on a forum — at which point the type
distinction has been routed around by convention and only the record survives.
The record surviving is not nothing, but it is a weaker feature than the one
proposed, and §6's records are how the project would find out.

**A language model writing Science will reach for `p < 0.05`, because that is what
a hundred million lines of Python taught it.** `llm-ergonomics.md` §1 is the
relevant argument and `SC0288`'s three suggestions (§2.6) are the response —
correct, screen, or declare — each mechanically applicable. This is a case where
the diagnostic is doing the entire teaching, and if its suggestions are not
applicable-in-the-`Suggestion`-sense the feature will read to a model as an
obstacle rather than a lesson.

**This note is about one error among many, and the many are larger.** §5.2 does
not answer that objection because it cannot. The feature is worth its price; it
is not worth an outsized place in how the language describes itself, and §9.8
constrains the sentence that may be said about it for exactly that reason.

**The record's value depends on somebody reading it.** A methods paragraph nobody
opens is a build artefact. The compiler's contribution ends at making the
information exist in a form a referee could use; whether the practice of science
uses it is outside this note, outside this language, and should not be claimed by
either.
