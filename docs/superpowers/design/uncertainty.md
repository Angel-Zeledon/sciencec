# Science — Design: uncertainty

Date: 2026-09-16
Status: draft for review
Owns: `Uncertain of T`, the `9.80665(15)` literal, correlation tracking, the
`Real` interface, and `9.81 ± 0.02 m/s²`.
Builds on: `scientific-libraries.md` §12 (the `Quantity` representation and its
three deliberate exclusions) and §14.6 (the ask this note answers),
`unit-literals.md` §2 and §8 (the numeric-literal tail, which this note extends
by one production), `strings-formatting-and-docs.md` §2 and §3
(`Display`/`Inspect`/`DisplayNumber` and the format mini-language),
`publishable-output.md` §6 (which landed while this note was being drafted, owns
the significant-figure rule and the `u` format code, and which §7 below **adopts
rather than re-decides**), `const-expression-arithmetic.md` §2.3 and §10.1 (which
this note reads and then declines to depend on — see §8.4).
Written in: `syntax-revision-2.md`. Every sample here is revision-2 Science.
Diagnostic block: **`SC0277`–`SC0278`**, per `README.md`'s allocation map. Those
two codes appear in `README.md`'s *claimed* table against
`stdlib-shape-and-packages.md`; that note's §11 explicitly returns them —
*"Codes actually claimed by this note: `SC0276` and `SC0279`. `SC0277` and
`SC0278` are returned to the free pool."* — and this note takes them on that
basis. `README.md`'s free-list row should be corrected to match.

---

## 0. The one sentence

**Uncertainty is a value, not a type-system feature: `Uncertain of T` is an
ordinary record carrying a nominal value and a gradient over the independent
sources that produced it, arithmetic merges those gradients so that correlation
is exact rather than approximated, the literal `9.80665(15)<m/s^2>` is forty
lines of lexer, and the only language-level ask is one interface — `Real` —
which the catalogue already needed under a different name.**

The two claims that follow from it, and that the rest of the note argues:

> **Correlation cannot be tracked in the type system, because cancellation is
> arithmetic on runtime coefficients and not on identities.** `x - x` and
> `x - y` have the same type under every static scheme anybody has proposed.
> The expensive design does not buy the property that makes the feature real.

> **The feature is a scalar feature for the summary stage of a calculation, and
> saying so is what makes it affordable.** An `Uncertain of F64` is eighty bytes
> and an addition is a merge. It belongs on the fifteen numbers that go into a
> table, not on the ten million that go into a fit. Naming that boundary removes
> the performance objection instead of arguing with it.

---

## 1. Why this note exists

Four notes independently reached the same hole, which is this project's own
evidence that something wants building.

- **`scientific-libraries.md` §14.6** asks for an uncertainty type, observes that
  every CODATA constant carries one and so does every measured quantity in `chem`
  and `physics`, and says it *"interacts with `Quantity` and it is much cheaper
  to design them together than to add one to the other later."*
- **`unit-literals.md` §13** records that the question is **lexical** as much as
  it is a type question: CODATA writes a value and its uncertainty as one token,
  `9.80665(15)`, and the lexer either admits the parenthesised digits or it does
  not. It also prices the delay: `physics.constants` is wave 2.
- **`strings-formatting-and-docs.md` §3.3 and §10** leave `9.81 ± 0.02 m/s²`
  open, correctly observing that it is *"a value with an uncertainty, not a
  formatting of a value."*
- **`publishable-output.md`**, which landed while this note was being drafted, is
  the fourth caller and the most exposed. Its §13 says so in one sentence:
  *"The whole design is worthless without the uncertainty type, and the
  uncertainty type is not designed."* Its §6 — the significant-figure rule that
  decides whether a table is publishable — is entirely conditional on a value
  that carries a propagated uncertainty, and its §14 asks the question this note
  answers: *"Does `Uncertain of T` propagate, and how?"*

There is a fifth reason, and it is the one that sets the deadline. `physics.constants`
ships in wave 2 with, in §12.1's own words, *"CODATA values, each a `Quantity`
with its unit and its uncertainty"*. There is no representation for the second
half of that sentence. Wave 2 therefore ships one of two things: constants whose
uncertainty is dropped on the floor, or constants whose uncertainty lives in a
doc comment where nothing can check it and nothing can propagate it. Both are
the pre-`<m/s^2>` state of units, and the second is worse, because a number in a
comment looks maintained.

**What this note must not do.** It must not make the catalogue's several hundred
signatures cost twice. That is the practical objection, it is fatal if unanswered,
and §6 answers it head-on before §10 gives a verdict.

---

## 2. Decision 1 — uncertainty is in the value

> **Decision 2.** `Uncertain of T` is an ordinary record type, defined in
> Science, in the prelude. There is no uncertainty modality, no effect, no
> checker-tracked property, and no second kind of `Quantity`. What the checker
> knows is exactly what it knows about any other record: that `Uncertain of F64`
> is not `F64`, and that converting between them is a call you can see.

### 2.1 The three designs, as they were actually considered

**A — in the value.** `Uncertain of T` is a record with arithmetic that
propagates. Python's `uncertainties` is this. Cheap, dynamic, and nothing stops
you dropping it.

**B — in the type.** A quantity is uncertain or it is not, the checker knows
which, and "you published a number with no error bar" becomes a compile-time
fact. Two sub-designs were considered under this heading and they are different
enough to be rejected for different reasons (§2.3).

**C — neither.** A library outside the language, like every other language has.

### 2.2 Why A

**The gap between A and B is one warning wide, and that is the whole argument.**

B's unique selling point is stated in the brief for this note as *"the only
version in which 'you published a number with no error bar' is a compile-time
fact."* That is not quite right, and noticing why decides the section. Under A,
`Uncertain of F64` **is a type**. A function declared `-> Uncertain of F64`
cannot return an `F64`. A `Frame` column typed `Uncertain of F64` cannot be
filled with bare floats. The only way to lose the uncertainty is to call
`.nominal()`, which is four words long, greppable, and syntactically visible at
the exact point the information is discarded.

So what B adds over A is not *knowing* where uncertainty is dropped — A knows
that too — it is *forbidding* the drop. And forbidding it outright is wrong:
`u.nominal()` is the correct and necessary call at an FFI boundary, at a plot
axis, and in every `if` condition. What is wanted is not a prohibition but a
question asked at the one place it matters, which is a warning. That warning is
`SC0278` (§9), it is a local syntactic check, and it costs one checker arm
against B's new kind.

**`publishable-output.md` §12 item 5 makes the same point from the other end and
makes it better**, so it is quoted rather than paraphrased:

> *"An `Uncertain of T` that reaches a table without having propagated through
> the computation that produced it prints a `±` that is a lie. The type is the
> guarantee — if every operation on `Uncertain` propagates, then a value's being
> `Uncertain` is the claim that its uncertainty is the propagated one."*

That is the property design B was supposed to provide, and A already provides it,
because A's `Uncertain` is a type whose every operation propagates. The claim is
carried by the *type* either way; what B adds is only that you may not put the
claim down.

**A is the design every working implementation uses**, and the working
implementations are the evidence. Python's `uncertainties`, Julia's
`Measurements.jl`, C++'s `boost::math` propagation — all value-level. No
production language puts uncertainty in the type. That is not proof, but a
unanimous prior is worth stating before overturning it.

**A composes with `Quantity` for free** (§5), and B does not. §12.3 already
parameterises `Quantity` by its representation type `T`. Under A, an uncertain
quantity is `Quantity of (Uncertain of F64, 1, 0, -2, …)` and **§12.3 does not
change by one character**. Under B, `Quantity` acquires either a second modality
parameter or a second spelling, and §12.6's careful list of what is deliberately
excluded gains an entry it did not choose.

**Cost of A, stated.** Nothing prevents `let g be constants.STANDARD_GRAVITY.nominal()`
followed by a paper table with no error column. `SC0278` catches the common
shape of that and does not catch all of it. A user determined to publish a bare
number can. That is accepted, and §10 weighs it.

### 2.3 Why not B, in two forms

**B1 — uncertainty as a modality or effect on the type.** `uncertain F64`,
tracked by the checker, contagious through arithmetic, requiring an explicit
`assume_exact` to leave.

Rejected on three grounds, the third decisive:

1. **It does not solve the problem it appears to solve.** §3 is the section that
   decides whether this feature is real, and B1 contributes nothing to it. A
   modality says *that* a value is uncertain. Correctness requires knowing
   *which independent sources* it came from and *with what coefficients*, and
   the coefficients are runtime floats. See §3.4.
2. **It is viral and it has no off-switch that is not a lie.** Once
   `STANDARD_GRAVITY` is uncertain, every expression touching it is uncertain,
   every signature that might see one grows a modality variable, and the
   `assume_exact` escape becomes the single most-typed token in the language.
   Effects that are inferred rather than written can survive this;
   `effects.md` makes exactly that argument for its three bits. But its three
   bits are properties of a *function*, inferred from its body. Uncertainty is a
   property of a *value*, flowing through data, which is the expensive kind.
3. **There is no second customer, so §12.5's affordability argument is
   unavailable.** This project's standard for a language feature is the one
   `scientific-libraries.md` §12.5 set and `const-expression-arithmetic.md`
   accepted: *build it once, because two things need it*. Const-expression
   arithmetic is affordable because shapes and units both require it.
   A value modality has one customer, and it is this note. By the project's own
   test it does not qualify.

**B2 — the source set in the type.** `Uncertain of (T, {A, B})`, where the type
carries the *identity* of the independent sources, so that combining two values
that share a source is a compile-time event.

This is the interesting one, because it is the only static design that engages
with §3 at all. Rejected, and the reason is worth writing down because it is not
obvious:

- **It would need type-level sets**, which nothing else in the language wants.
  `const-expression-arithmetic.md` §2.3 closes the kind list at `Int` and
  `Shape` and gives a reason for each exclusion; a `SourceSet` kind would be a
  third, asked for by one note.
- **It would fire on every real program.** In a calculation with six measured
  inputs, essentially every pair of intermediate values shares a source. A check
  that fires on the common case is not a check, it is a tax.
- **It still does not decide the question.** Knowing that `x` and `x` share a
  source tells you the naive formula is wrong. It does not tell you the answer,
  which is that the coefficients cancel exactly. The runtime machinery of §3.3
  is still needed, in full, and B2 sits on top of it charging rent.

### 2.4 Why not C, exactly

C is half right and it is worth being precise about which half, because this
note is mostly agreeing with it.

**The propagation is a library.** All of it. `Uncertain`, its operators, the
derivative rules, the covariance entry point, the rendering — Science, no
compiler changes, no new kind, no new phase.

**Two things are not a library and cannot be.**

1. **The literal.** `9.80665(15)` is a lexical question (§4). A library cannot
   add a production to the numeric literal. Without it, the CODATA table is
   hand-transcribed into constructor calls with the exponent arithmetic done by
   the author, which is exactly the class of error the feature exists to remove.
   `unit-literals.md` §1 made this argument for units and it transfers without
   modification: *a feature reached only through a constructor call is a feature
   nobody reaches.*
2. **The `Real` interface** (§6). Not because it is exotic, but because it
   determines the shape of several hundred signatures in `scientific-libraries.md`,
   and those signatures are being written now. Deciding it later means editing
   them all later. §14.4 of that note already asks for `Float` on the same
   grounds; this note asks that the ask be widened by one layer before it is
   granted, which costs nothing today and cannot be done cheaply in a year.

So: **the language-level part of uncertainty is one lexer production and one
interface.** Everything else is C.

---

## 3. Decision 2 — correlation, which is where naive designs are wrong

This is the section that decides whether the feature is real or a toy.

### 3.1 The failure, concretely

Take the naive design — a value and a magnitude, with the textbook formula
`σ_f² = Σ (∂f/∂xᵢ)² σᵢ²`:

```science
let x be 10.0(1)
print(x - x)
```

The naive answer is `0.00 ± 0.14`. The correct answer is `0`, exactly, with no
uncertainty at all, because `x - x` is zero for every value `x` could have taken.

That looks like a curiosity. It is not. It is the same arithmetic as every case
a working scientist actually meets:

```science
# One ruler, two readings. The ruler's calibration is one source, shared.
let width be 12.40(5)<cm>
let height be 8.30(5)<cm>
let ratio be width / height           # the shared calibration partly cancels

# A difference of two temperatures from the same probe.
let before be 21.30(15)<K>
let after be 24.10(15)<K>
let rise be after - before            # the probe's zero-point offset cancels
```

Whenever the same instrument, the same calibration, or the same reference value
appears twice in a formula, the naive answer is wrong. It is wrong **in the
direction of over-stating the uncertainty** for differences and under-stating it
for sums, and the error is a factor of √2 or worse.

> **A propagation that silently gets correlated quantities wrong is worse than
> no propagation at all**, because a printed error bar is believed. A missing
> error bar prompts the reader to ask; a wrong one does not.

That sentence is the acceptance criterion for this section.

### 3.2 What correctness actually requires

First-order propagation of `f(x₁ … xₙ)` is exact for correlations if — and only
if — each value carries, alongside its nominal, **the vector of its partial
derivatives with respect to the independent sources**, and arithmetic composes
those vectors by the chain rule. Then:

- `x - x` subtracts a gradient from itself, every coefficient cancels to zero,
  and the result is exactly zero. Not approximately.
- `covariance(a, b)` is `Σᵢ (∂a/∂sᵢ)(∂b/∂sᵢ) σᵢ²`, computable from the two
  gradients with no extra bookkeeping — so correlations between *derived*
  quantities come out for free, which is the other thing scientists need and
  never get.
- `σ_a` is `√(Σᵢ (∂a/∂sᵢ)² σᵢ²)`, the marginal, recovered as the special case
  `covariance(a, a)`.

This is forward-mode automatic differentiation with the independent measurements
as the seeds. Once seen that way, the whole design is a known object with known
costs, and every question below is a data-structure question.

### 3.3 The decision

> **Decision 3.3.** An `Uncertain of T` carries a nominal value and a **gradient**:
> a sequence of `(source, coefficient)` pairs, sorted by source, holding one
> entry per independent source that the value depends on. Sources are identified
> by an opaque 64-bit id minted at construction. Arithmetic merges two sorted
> gradients in linear time. **A coefficient that reaches exactly zero is
> dropped.** Correlation is therefore exact to first order, not approximated,
> and `x - x` is `0`.

```science
type Uncertain of T:
    nominal: T
    gradient: Gradient of T
```

`Gradient of T` is a small sorted sequence — inline up to four terms, spilling to
the heap beyond — with three operations: merge-add, scale, and dot. It is not a
map; there is no hashing. Sortedness is what makes the merge linear and the
representation canonical, and canonicality is what makes "the coefficient reached
zero" a decidable local fact rather than a search.

**Sources are minted at construction, from a process-global atomic counter.**
Every literal `10.0(1)`, every `Uncertain.of(v, s)`, and every column of
`Uncertain.from_covariance` mints fresh ids. Two threads never collide because
the counter is atomic; `stdlib-standard.md`'s thread model therefore imposes no
extra rule. A 64-bit counter does not wrap in any run this language will see.

**A flat gradient, not a graph — which answers the objection
`publishable-output.md` §14 raised.** That note's open question says a
correlation-tracking implementation *"makes every value carry a reference to its
derivation graph, which sits badly with ownership"*, and it is right about
Python's design, which is a graph of `AffineScalarFunc` nodes keeping their
operands alive. **This design has no graph and no references.** The gradient is
*eagerly* reduced at each operation to a flat, owned, sorted sequence of
`(source, coefficient)` pairs. A value owns its gradient outright, borrows
nothing, and keeps nothing else alive. The ownership objection is an objection to
the Python representation, not to correlation tracking, and it dissolves with the
data structure.

**The operators borrow.** `Add`, `Sub`, `Mul`, `Div` and `Neg` on `Uncertain`
take `self` and `borrowed other`, and return a new value. This is not a detail:
`Uncertain` is not `Copy` once its gradient spills, so operators taking the
receiver **by value** — which is spelled `self: Self` — would make this note's
own headline example, `x - x`, a use-after-move. The same requirement propagates
to `Quantity`'s operators (§5.3).

> **This paragraph used to say `borrowed self`, and the fix is not cosmetic.**
> `ast::SelfKind` has exactly three receivers and the bare one is already the
> borrow: `self` borrows shared, `mutable self` borrows exclusively, and only
> the annotated `self: Self` takes the value away from the caller.
> `borrowed self` is not a spelling the parser has ever accepted — it is
> `SC0102`, *expected an identifier, found `borrowed`*. Re-spelled
> mechanically, the sentence would have claimed that operators "taking `self` by value" cause the
> use-after-move, which is the opposite of what `self` means, and the paragraph
> would have argued against itself while still reading perfectly. So the claim
> is restated against the receiver that actually moves rather than quietly
> carried over.

### 3.4 What is static, and what cannot be

**Static: nothing about correlation. Static: everything about dimension.**

The temptation is to put source identity in the type, and §2.3's B2 is the
design that does. The reason it fails is worth isolating, because it is the
single most useful thing this note found:

> **Cancellation is arithmetic on the coefficients, and the coefficients are
> runtime floats.** `x - x` and `x - y` have the same *set* of contributing
> sources under any reasonable static approximation of that set. What
> distinguishes them is that in the first case the two coefficients are `+1` and
> `−1` and sum to zero. A type system that tracked identity perfectly would still
> have to be told the answer by the runtime.

The corollary matters for sequencing: **this note asks nothing of
`const-expression-arithmetic.md`.** It is, as far as this set of notes goes, the
first design that does not queue behind it. §8.4 develops that.

What *is* static is the part that was already static: `Quantity`'s seven
exponents. An uncertain length is still a length, and §12.4's dimension check
runs on the outside of the uncertainty (§5) and is entirely unaffected by it.

### 3.5 What it costs, in bytes and instructions

Stated plainly, because the cost is the reason §3.6 draws a boundary.

| | `F64` | `Uncertain of F64`, 1 source | 4 sources | spilled |
|---|---|---|---|---|
| Size | 8 B | 80 B | 80 B | 32 B + heap |
| `a + b` | 1 instruction | ~15 | ~40 | allocates |
| `a * b` | 1 instruction | ~25 | ~60 | allocates |
| `Copy` | yes | yes (inline) | yes (inline) | **no** |

So: **one to two orders of magnitude, and an allocation past four sources.**

Two consequences that must be said out loud rather than discovered:

**Reduction over an array of uncertains is quadratic.** Summing `n` independent
uncertain values by a left fold produces gradients of length 1, 2, 3 … n, so the
work is O(n²) and the final value holds `n` terms. For `n` in the tens this is
invisible; for `n` in the millions it is a hang. `stats` must therefore provide
analytic entry points — `stats.mean_with_uncertainty(xs)` computing the standard
error in one pass over plain floats — rather than letting `math.sum` be reached
with an uncertain element type. That is an ask on `scientific-libraries.md` §7.1
(§11).

**`Uncertain` is not `Copy` once it spills.** The rejected alternative was a
fixed-capacity gradient with no spill, where the smallest terms are pooled into
one anonymous source when the capacity is exceeded. It is tempting — fixed size,
`Copy`, no allocation — and it is rejected precisely because **pooling loses
correlation silently**, which is §3.1's acceptance criterion inverted. A design
whose whole justification is getting correlation right may not have a mode where
it quietly stops. The non-`Copy` spill is the honest cost of the honest answer.

### 3.6 The boundary this cost buys

> **Decision 3.6.** `Uncertain` is a **scalar** feature. It is not a tensor
> element type, it does not enter `linalg`, and `broadcasting.md`'s element-wise
> machinery is not expected to carry it. Bulk-data uncertainty is a covariance
> matrix produced by a fit, which is `stats`' object and not this one's.

An `Array of Uncertain of F64` is legal — it is an array of ordinary values and
nothing forbids it — and it is the wrong tool past a few dozen elements. What is
*refused* is the C-linked path: `linalg.solve` on a matrix of uncertains cannot
work, because the matrix is handed to LAPACK (§2 of that note), and LAPACK takes
doubles. That refusal is `SC0277` (§9) and it carries the covariance alternative
in its help text.

Drawing this line is what makes the performance numbers in §3.5 acceptable
rather than disqualifying. Eighty bytes and a merge is a fine price for the
fifteen numbers that go in a table. It is an absurd price for a million-point
spectrum, and the design says so instead of leaving users to find out.

### 3.7 Correlated *inputs*, and how they get in

A fit does not produce independent parameters; it produces a covariance matrix.
Entering that correctly is a whitening transform and it belongs in the library
from the first version, because without it the feature cannot consume the output
of `stats` — which is where most real uncertainties come from.

```science
Uncertain has:
    ## Independent: one fresh source, standard uncertainty `sigma`.
    def of(nominal: F64, sigma: F64) -> Uncertain of F64

    ## Exact: no sources at all. Arithmetic with it is exact.
    def exact(nominal: F64) -> Uncertain of F64

    ## Correlated: `n` values with an `n`×`n` covariance. Internally a Cholesky
    ## factor of `cov` over `n` fresh independent unit sources, so that the
    ## resulting values reproduce `cov` exactly under §3.2's formula.
    def from_covariance(
        nominals: borrowed Array of F64,
        cov: borrowed Matrix of F64,
    ) -> (Array of Uncertain of F64, Error?)
```

`from_covariance` fails when `cov` is not positive semi-definite, which is an
ordinary `Error?` per `syntax-revision-2.md` §3. The dense gradient is `n` terms
wide for each of the `n` outputs, which is the one place §3.5's cost is paid at
scale deliberately: a ten-parameter fit gives ten values with ten terms each, and
that is exactly right.

### 3.8 The second honesty: linearisation

First-order propagation is a linearisation, and it is wrong when the function
curves appreciably over the width of the uncertainty.

```science
let x be 1.0(5)
let y be x * x         # 1.00 ± 1.00 by first order
```

The true standard deviation of `X²` for `X ~ N(1, 0.5)` is about `1.12`, and its
mean is `1.25`, not `1.00`. First order sees neither.

**This is not a defect of this design; it is the definition of the quantity the
GUM specifies and every paper reports.** But it must be documented at the top of
the module rather than in a footnote, with a named escape:
`stats.monte_carlo_propagate(f, inputs, draws)`, which samples and is exact for
any nonlinearity at the cost of being slow and stochastic.

Two things this note refuses to do about it: a second-order term (it needs the
Hessian, which triples the representation to fix the rarer of the two problems),
and a heuristic warning when the linearisation "looks bad" (there is no local
criterion, and a wrong warning is worse than none).

---

## 4. Decision 3 — the literal

### 4.1 The token

> **Decision 4.1.** A numeric literal's significand may be immediately followed
> by a parenthesised uncertainty group, with no whitespace. The group is part of
> the number, it precedes any exponent, and it composes with
> `unit-literals.md` §8's width suffix and unit literal in that fixed order.

```science
let g          be 9.80665(15)<m/s^2>
let big_g      be 6.67430(15)e-11<m^3/kg/s^2>
let m_e        be 9.1093837015(28)e-31<kg>
let resistor   be 100(1%)<ohm>
let reading    be 21.30(15)<K>
let plain      be 10.0(1)
```

The grammar, as an amendment to `unit-literals.md` §2.1 and core spec §4.2:

```
literal       := significand uncertainty? exponent? width-suffix? unit-literal?
uncertainty   := "(" ( digit+ | decimal "%" ) ")"
```

with the same absolute side condition `unit-literals.md` imposes: **no
whitespace and no comment anywhere from the first digit to the closing `>`**, and
no newline crossed by any tentative scan.

### 4.2 The lexical argument, and why it is cheaper than `<`

`unit-literals.md` §2.2 had to work to earn the angle bracket, because `<` is
`Lt` and the ambiguity with comparison is the thing that killed angle-bracket
generics in several languages. **The parenthesis is a much easier case, and the
reason should be stated rather than assumed.**

`(` is `LParen`, and in expression position it means either a call or a grouping.
Both require something to the left that can be called or that ends an expression
which can be grouped. **A numeric literal is neither.** There is no production in
Science today in which a digit is immediately followed by `(`:

| Source | Today | Under this note |
|---|---|---|
| `9.8(15)` | syntax error | uncertainty literal |
| `f(15)` | a call — `f` is an identifier | unchanged |
| `(9.8)(15)` | syntax error — a float is not callable | unchanged |
| `9.8 (15)` | syntax error | unchanged; the space kills the scan |
| `a[0](x)` | syntax error | unchanged; `]` precedes the `(` |

So this notation consumes a position that is currently *dead*, not one that is
currently *contested*. It needs no backtracking rule of the kind §2.3 of that
note had to bound — though the implementation should backtrack anyway, for the
same reason and at the same cost, so that `9.8(x)` reports a good error rather
than a lexical one.

There is no implicit multiplication in Science, so `2(x + 1)` is not a meaning
this forecloses. There is no numeric method call syntax — `unit-literals.md` §2.5
rejected `9.8.metres()` — so `9.8(…)` cannot be confused with one.

**The one thing it does foreclose is itself irreversible**, exactly as §12 of
that note says of `<`: once shipped, `digit (` is consumed forever. That is the
honest entry in §12 below.

### 4.3 What the digits mean

> **Decision 4.3.** The digits of the group align with the last digits of the
> significand. If the significand has `d` digits after its decimal point
> (ignoring `_` separators) and the group is the integer `u`, the standard
> uncertainty is `u × 10⁻ᵈ`, scaled afterwards by the exponent and by the unit's
> conversion factor.

| Written | Nominal | σ |
|---|---|---|
| `9.80665(15)` | 9.80665 | 0.00015 |
| `9.8(15)` | 9.8 | 1.5 |
| `100(5)` | 100.0 | 5.0 |
| `6.67430(15)e-11` | 6.6743e-11 | 1.5e-15 |
| `1.602_176_634(51)e-19` | 1.602176634e-19 | 5.1e-27 |
| `2.5(0)` | 2.5 | 0 — exact, and legal |

This is the CODATA and BIPM convention verbatim and it is not negotiable; a
scientist who reads `9.80665(15)` already knows what it means, and any other
reading would be a trap. `9.8(15)` looking odd is a property of the convention,
not of this design.

`_` separators are permitted inside the significand and are ignored when counting
`d`, which is what lets the CODATA table be pasted in its published grouping.

**The exponent goes after the group**, because that is where CODATA puts it:
`1.602 176 634(51) × 10⁻¹⁹`. `9.8e5(2)` is `SC0013` with an applicable fix that
moves the group. One spelling, per `syntax-revision-2.md` §1.

### 4.4 Relative uncertainty

> **Decision 4.4.** A group whose content is a decimal followed by `%` is a
> *relative* standard uncertainty: `σ = |nominal| × r / 100`.

```science
let resistor be 100(1%)<ohm>          # σ = 1.0 ohm
let scale    be 47.0(0.5%)<g>         # σ = 0.235 g
```

This is the one place this note admits two spellings for one thing, and against
`syntax-revision-2.md` §1 that needs an argument rather than a shrug.

**The argument is that they are not one thing.** A component tolerance, a
detector's quoted precision and a manufacturer's datasheet all specify a
*relative* uncertainty, and the absolute form requires the author to do a
multiplication — magnitude-dependent, easy to get wrong by a factor of ten, and
invisible once done. A resistor is specified as "100 Ω, 1%" in every datasheet
ever printed, and requiring `100(10)` instead is requiring the author to
translate before the compiler can check anything. That is the same argument §1 of
`unit-literals.md` makes for the literal existing at all.

**`sciencec fmt` does not normalise between the two forms.** Rewriting `100(1%)`
to `100(10)` would erase the author's source, which is the datasheet, and
rewriting the other way is not always exact. This is the rare case where two
spellings carry different provenance, so canonicalising would lose information —
the same test `strings-formatting-and-docs.md` §3.3 applies when it keeps the
ASCII and typeset unit spellings apart.

The relative form is restricted to a decimal literal: `9.8(1e-3%)` and
`9.8(-1%)` are `SC0013`. No expression appears inside the group.

### 4.5 Composition, and what the unit does to σ

The full tail, in order: **significand, uncertainty, exponent, width suffix,
unit.**

```science
let a be 9.80665(15)<m/s^2>        # Quantity of (Uncertain of F64, 1,0,-2,0,0,0,0)
let b be 9.80665(15)f32<m/s^2>     # the same, over Uncertain of F32
let c be 9.80665<m/s^2>            # Quantity of (F64, …) — unchanged, no uncertainty
let d be 9.80665(15)               # Uncertain of F64 — no unit
let e be 5i64(2)<kg>               # SC0252: an integer cannot carry an uncertainty
```

The order follows `unit-literals.md` §8's own reasoning taken one step further.
That note argues the width suffix precedes the unit because *"the suffix is part
of the number; the unit is a wrapper around it."* The uncertainty group is one
level tighter still — it is part of the number's **digits**, aligned to them by
§4.3, so it cannot be separated from them by anything. Reading left to right:
*the number 9.80665, known to ±15 in its last digits, as an f32, interpreted as
metres per second squared.* Each qualifier answers a question that the previous
one made askable.

**The unit's conversion factor multiplies the uncertainty, and its offset does
not.** This falls out of the arithmetic and is worth one line because getting it
wrong is invisible:

```science
let x be 2.5(4)<km>          # nominal 2500.0, σ 400.0   — factor scales both
let t be 37.0(2)<degC>       # nominal 310.15, σ 0.2     — offset shifts only the nominal
```

An affine offset is a change of origin, and a change of origin does not change a
spread. `unit-literals.md` §6.1's fold `(L + o) × k` becomes, for the pair,
`nominal = (L + o) × k` and `σ = u × k` — with the relative form computed against
`L` before the offset, since a "1% thermometer" means 1% of the reading it gave.

All of this folds in exact rational arithmetic in the front end, per §5.2 of that
note, and rounds once. An uncertainty that overflows or underflows its target
type is `SC0253`, the same code that already covers the nominal.

### 4.6 `±` is output only

> **Decision 4.6.** `±` is not input syntax. It appears in rendering (§7) and
> nowhere else. There is no infix `±` operator and no `+-` digraph.

Three reasons, and the first two are other notes' decisions rather than this
one's:

- **`strings-formatting-and-docs.md` §6.3 decided no Unicode operators**, with a
  full argument this note will not relitigate. `±` is a Unicode operator.
- **An infix `±` would be ambiguous with a unary sign**, exactly as `+` and `-`
  are, and `a ± b ± c` has no reading anybody agrees on.
- **The general case already has a spelling**: `Uncertain.of(9.8, 0.02)`, for
  when the uncertainty is computed rather than written. The literal exists for
  the case where it is written, and there the concise form is both shorter and
  the one the source document uses.

Rejected with it: `9.8 +- 0.02` as an ASCII digraph, which is a third spelling
for the same operation and buys nothing the constructor does not.

### 4.7 What the literal lowers to

`9.80665(15)<m/s^2>` lowers, before HIR, to:

```science
Quantity of (Uncertain of F64, 1, 0, -2, 0, 0, 0, 0)(
    value: Uncertain.of(9.80665, 0.00015)
)
```

with both numbers folded exactly at compile time and **one fresh source minted at
run time**, at the point the literal is evaluated. Two evaluations of the same
literal — in a loop, or in two calls — are two independent measurements, which is
correct: a literal is a *measurement*, and two readings of the same instrument
are two readings.

The corollary that will surprise someone, and that the reference should show:

```science
let a be 9.80665(15)
let b be 9.80665(15)
print(a - b)                   # 0.00000 ± 0.00021 — not zero
```

`a` and `b` are two measurements that happen to agree, not one measurement used
twice. If one measurement used twice is meant, bind it once. This is the mirror
of `unit-literals.md` §5.3's `1<ft> is 0.3048<m>` — the first thing a user will
test, and better shown than discovered.

---

## 5. Decision 4 — where uncertainty sits relative to `Quantity`

> **Decision 5.** The dimension is **outside** and the uncertainty is **inside**:
> `Quantity of (Uncertain of F64, …)`, never `Uncertain of (Quantity of …)`.
> `scientific-libraries.md` §12.3 does not change.

### 5.1 Why this way round

`§12.3` already reads:

```science
type Quantity of (T, const LENGTH: Int, const MASS: Int, …):
    value: T
```

The representation is already a parameter. Instantiating it at `Uncertain of F64`
requires **no amendment to that declaration, no new parameter, and no new
alias** — `Length of (Uncertain of F64)` works the moment `Uncertain` exists.
That is the cheapest possible answer to §14.6's *"much cheaper to design them
together"*, and it is available only in this order.

Three further reasons:

- **A dimension is not uncertain.** You may not know the value of a length; you
  know it is a length. Putting `Quantity` inside would make the exponent vector a
  property of an uncertain thing, which is a category error that would surface
  the first time somebody asked what the dimension of an uncertain quantity is.
- **The unit check stays where §12.4 put it.** Dimensional analysis runs on the
  outer type and is completely unaffected: `Force of (Uncertain of F64)` plus
  `Time of (Uncertain of F64)` is `SC0256`, with the same message and the same
  code, because the const arguments are the same const arguments.
- **`unit_symbol` is untouched.** `strings-formatting-and-docs.md` §3.3's
  associated function is over the seven const exponents and does not see `T`.

### 5.2 A contradiction with `strings-formatting-and-docs.md` §3.3, named

That note's `Display for Quantity` reads:

```science
Quantity implements Display:
    def display(self, into: mutable borrowed Formatter):
        into.number(self.value as F64)
        into.raw(" ")
        into.raw(Self.unit_symbol())
```

**`self.value as F64` is a hard cast and it does not survive this note.** An
`Uncertain of F64` has no `as F64`, and giving it one would be exactly the silent
drop §2.2 is trying to make visible.

The amendment is one line and it improves that note on its own terms: **delegate
to `T`'s own `DisplayNumber` rendering rather than casting.**

```science
Quantity implements Display where T: DisplayNumber:
    def display(self, into: mutable borrowed Formatter):
        self.value.display(into)
        into.raw(" ")
        into.raw(Self.unit_symbol())
```

For `T = F64` the behaviour is identical. For `T = Uncertain of F64` the whole of
§7 below applies inside the quantity, and `9.80665 ± 0.00015 m/s²` comes out with
no further machinery. The cast was load-bearing only because that note had one
representation in mind.

### 5.3 One requirement on `Quantity`'s operators

`Quantity`'s arithmetic must **borrow** its operands rather than take them by
value, for §3.3's reason: `Uncertain` is not `Copy` when its gradient spills, and
a by-value `Sub` would make `d - d` on an uncertain length a use-after-move. For
`T = F64` this is free. It is an ask on `scientific-libraries.md` §12 (§11).

---

## 6. Decision 5 — what this does to every signature

The practical objection, met head-on: *if `sqrt` takes `Uncertain of F64`, does
every function in `math` need two versions?*

**No — and the reason is that the change the catalogue needs is the one it has
already asked for.**

### 6.1 The rejected answers, first

**Two versions of everything.** `sqrt(F64)` and `sqrt_uncertain(Uncertain of F64)`.
Rejected on arithmetic: §5 of `scientific-libraries.md` is roughly two hundred
functions before `stats`, `optimize` and `signal` are counted. Doubling that is a
doubling of the API surface, the documentation, the test matrix and the
commitment surface §15 of that note already worries about. It also fails at the
first composition: a user function written over `F64` still cannot take an
uncertain input, so the duplication does not stop at the library boundary — it
propagates into user code forever.

**Automatic lifting.** The compiler inserts `Uncertain` versions of `F64`
functions on demand. Rejected outright, and this is the strongest rejection in
the note: **there is no way to lift a function without its derivative.** Lifting
`f: F64 -> F64` to uncertain inputs requires `f'`, which cannot be recovered from
a compiled function. What the compiler could do instead is numerical
differentiation — a finite difference behind the user's back, silently, with a
step size nobody chose. That is a wrong error bar produced by the language
itself, which is §3.1's criterion violated by the compiler rather than by the
library.

**An explicit boundary only** — enter uncertainty, leave it, and do the middle in
plain floats. Rejected as the *only* answer, but adopted as part of the answer:
§6.4 makes it the rule for exactly the cases where it is forced, and no wider.

### 6.2 The decision

> **Decision 6.2.** `scientific-libraries.md` §14.4's ask for a `Float` interface
> is granted as **two** interfaces. `Real` is the arithmetic-and-elementary-functions
> interface that `F32`, `F64` and `Uncertain of T` all implement, and it is what
> numeric signatures are generic over. `Float` refines `Real` with the things
> only a machine float has — widths, bit-level predicates, `ulp`, `next_after`,
> rounding — and is what a signature names when it genuinely needs a float.

```science
interface Real:
    def from_exact(value: F64) -> Self
    def nominal(self) -> F64

    def sqrt(self) -> Self
    def exp(self) -> Self
    def ln(self) -> Self
    def pow(self, exponent: borrowed Self) -> Self
    def sin(self) -> Self
    def cos(self) -> Self
    def atan2(self, other: borrowed Self) -> Self
    def abs(self) -> Self
```

plus the operator interfaces `Add`, `Sub`, `Mul`, `Div`, `Neg` and `Ord`, which
`Real` requires. Whether that requirement is spelled as a supertrait list on the
interface or as a `where` clause at each use is `stdlib-core.md`'s call; nothing
here depends on which.

So the catalogue's signatures become:

```science
def sqrt of T(x: T) -> T where T: Real
def erf of T(x: T) -> T where T: Real
def bessel_j of T(order: Int, x: T) -> T where T: Real

# Unchanged — this one needs bit patterns, and refuses uncertainty on purpose.
def ulp(x: F64) -> F64
def next_after of T(x: T, towards: T) -> T where T: Float
```

**The key property: the width of the gradient never appears in a signature.**
`T: Real` hides the entire representation. A caller passing `F64` gets the
monomorphized float version with no overhead whatever; a caller passing
`Uncertain of F64` gets propagation. Nothing in the signature knows which.

### 6.3 The cost, counted

The real cost is the **derivative rules**, and it is smaller than it looks
because of one observation:

> **A function written in Science over `+ - * /` and the elementary set
> propagates uncertainty for free, by composition, the moment it is generic over
> `Real`.** No rule is needed for `bessel_j` if `bessel_j` is written in Science.

So rules are needed only at the **primitives** — the leaves of the call graph —
and at anything that links against C. Counting §5.1 and §5.2 of the catalogue:

| Group | Rules needed | Note |
|---|---|---|
| Arithmetic | 5 | `+ - * / neg`, hand-written in the prelude |
| Elementary | ~20 | `sqrt cbrt hypot exp exp2 exp10 expm1 ln log2 log10 ln1p log_base pow abs min max clamp lerp fma` |
| Trigonometric | ~14 | `sin cos tan asin acos atan atan2 sinh cosh tanh asinh acosh atanh` |
| Linked special functions | ~12 | only those that link against C rather than being written in Science |
| **Total** | **~50 one-line rules** | |

And the special functions are the cheap part, for a reason worth stating: **the
derivative of a special function is another special function, and the catalogue
already contains it.** `gamma' = gamma · digamma`, and §5.2 has `digamma`.
`erf' = 2/√π · exp(−x²)`, and §5.1 has `exp`. `J_n' = (J_{n−1} − J_{n+1})/2`, and
§5.2 has `bessel_j`. The rule table is transcription, not research.

**Functions that are `Float`-only, by decision, not by omission:**

- **`floor` `ceil` `round` `trunc` `fract`** — discontinuous. Their derivative is
  zero almost everywhere and undefined where it matters, and rounding a value
  that has a spread is a category error. `Float` only.
- **`sign` `copysign` `is_nan` `ulp` `next_after`** — bit-level or
  sign-discriminating. `Float` only.
- **`abs`** — kept in `Real`, with the subgradient at zero taken as zero and
  documented. It is used constantly and refusing it would be worse than the
  ambiguity at one point.

`Float` refining `Real` makes that line **checkable**: a signature says which it
needs, and passing an uncertain value to a `Float`-bound function is `SC0277`
with a message that says why.

### 6.4 The boundary where uncertainty must be entered and left

Three families genuinely cannot propagate, and naming them is more useful than
pretending:

1. **Anything that links against C or Fortran.** §2 of the catalogue links BLAS,
   LAPACK, FFTW and SuiteSparse. Those take doubles. `linalg` is therefore
   `Float`-bound throughout, and uncertain linear algebra goes through
   `stats`'s covariance path.
2. **Iterative solvers whose trip count depends on the value.** `solve_ode`,
   `levenberg_marquardt`, root finders. Propagating a gradient through a
   convergence test means the number of iterations depends on the uncertainty,
   which makes the derivative of the output with respect to the input a step
   function. These take nominal values. Where an uncertainty on the *answer* is
   wanted, the right mechanism is the implicit function theorem applied at the
   solution — which `optimize` can offer as a named function returning the
   parameter covariance, and which `curve_fit` should return anyway.
3. **Anything with data-dependent control flow over the uncertain value.**
   `if x > threshold` on an uncertain `x` takes the branch its nominal takes, and
   the gradient of the *other* branch is silently absent. This is a real
   incompleteness of forward-mode AD and it is the users' to know about. It is
   documented, not diagnosed: a lint would fire on every `if` in the language.

### 6.5 What it does to the catalogue, precisely

The edit is **mechanical and it was happening anyway**:

```
-def sqrt(x: F64) -> F64
+def sqrt of T(x: T) -> T where T: Real
```

Every signature in §5 through §11 of `scientific-libraries.md` that is currently
`F64` and does not need bit patterns takes this edit. §14.4 of that note already
requests the identical edit with `Float` in the bound — *"almost every signature
here is generic over it"* — so **the cost attributable to uncertainty is the
choice of which word goes in the bound**, not the edit itself. Making that choice
now costs one decision. Making it after the catalogue is written costs a sweep of
several hundred signatures and a compatibility break for anyone who wrote against
`Float`.

That is the same shape as `unit-literals.md` §9.3's amendment to the wave table
and `const-expression-arithmetic.md` §10.1's *"cheap now, expensive later"* list,
and it is the strongest practical argument in this note.

---

## 7. Decision 6 — rendering

### 7.1 Adopted, not re-decided

`publishable-output.md` landed while this note was being drafted and its §6
answers the rendering question in full and better than this note's own draft did.
**This note adopts Decisions 9 and 10 of that note without amendment**, in the
same way `unit-literals.md` §7.1 adopts `strings-formatting-and-docs.md` §3.3
rather than competing with it. Restated only far enough to be checkable here:

- **The uncertainty sets the significant figures**, by the Particle Data Group's
  rule — two digits of `u` when its three leading digits are `100`–`354` or
  `950`–`999`, one when they are `355`–`949` — and **both** the value and the
  uncertainty are then rounded to the same decimal place, half to even.
- `9.80665(15)<m/s^2>` therefore prints `9.80665 ± 0.00015 m/s²`, and **never**
  `9.806650000 ± 0.000150000` and never `9.81 ± 0.00015`.
- `u` is the format code, `#u` is the concise parenthetical form, `.1u` and
  `.2u` force the digit count, and `u` is the **default** code for an uncertain
  quantity, so `f"{g}"` is already right.
- A common power of ten is a property of a *column*, not of a cell (§6.4 there),
  which is a better answer than this note's draft had.

That note's §6.2 also settles what this note was about to request: a single new
`code` character in `strings-formatting-and-docs.md` §2.1, reported through its
existing `SC0274`. **The request is granted before it was made and this note
withdraws it, adding only that it should be decided jointly with
`unit-literals.md` §7.3's unit selector**, since two notes now want to extend one
production and deciding them apart is how a grammar acquires two incompatible
extensions.

### 7.2 What this note contributes to §6 there, and one flag

**The contribution is that `u` is the *propagated* uncertainty.** §6 of that note
takes `(v, u)` as given. This note is where `u` comes from, and §3 is the reason
it can be trusted: the number after the `±` is the first-order propagation
through the whole computation with correlations exact, not a magnitude carried
along beside the value. That is precisely the difference that note's §12 item 5
identifies between a `±` and a lie.

**One flag, and it is that note's call, not this one's.** Its Decision 9 step 1
renders a **non-finite** uncertainty as the value alone, with no `±`, grouping it
with the zero case. This note would have printed it — `9.8 ± nan` — on
`strings-formatting-and-docs.md` §2.3's reasoning for `-0.0`: *"it is a different
float and hiding that has cost people days."* A NaN uncertainty means the
propagation hit something undefined, and a bare `9.8` in a published table is the
one rendering that looks fine and is not. Zero and non-finite are different cases
and only zero means "exact". Raised here rather than decided; §6 there owns it.

**`DisplayNumber` is implemented for `Uncertain`**, which is what admits the
numeric codes at all (§2.4 of the formatting note) and what makes
`Quantity of (Uncertain of F64, …)` formattable through §5.2's delegation above.

### 7.3 `Inspect`, and the round-trip that loses something

`strings-formatting-and-docs.md` §3.2 fixes the division: `Display` is for the
user, `Inspect` round-trips into a program.

> **Decision 7.3.** `Inspect` prints the concise **source** spelling when the
> value round-trips exactly — `9.80665(15)<m/s^2>` — and a constructor call
> otherwise — `Uncertain.of(9.81, 0.0173)`. Both paste back into a program.
>
> **And both lose the value's correlations, which must be documented on the
> interface rather than in a footnote.**

The first clause is the same division §3.3 of the formatting note already draws
and §7.1's `#u` respects on the other side: `#u` is the **typeset** concise form,
`9.80665(15) m/s²`, for a table; `Inspect` is the **source** concise form,
`9.80665(15)<m/s^2>`, which compiles. One notation, two spellings, selected by
which of the two interfaces asked.

That second sentence is the sharpest caveat in the note. A printed uncertain
value carries its *marginal* uncertainty and nothing about which sources produced
it. Reading it back mints a fresh independent source. So:

```science
let d be a - b                         # correlated with a and b
print(f"{d!i}")                        # Uncertain.of(2.80, 0.21)
# …pasted into another program: now independent of everything.
```

This is unavoidable — the gradient is over runtime identities that do not survive
a process — and it means **serialisation decorrelates silently**. It is the
reason §12 lists it as a risk and §11 asks `data-io.md` for a paragraph.

### 7.4 The one notational symmetry worth noticing

`9.80665(15)<m/s^2>` is the input spelling (§4) and `9.80665(15) m/s²` is
`publishable-output.md` §6.2's `#u` output spelling. **The literal and the
concise rendering are the same notation pointing in opposite directions**, which
is not a coincidence and is the reason both are worth having: the value goes into
the program in the form CODATA published it and comes out in the form a journal
prints it, with the compiler doing the significant-figure arithmetic in between
and nobody transcribing anything.

---

## 8. Sequencing

### 8.1 Stage 1 — F0, with the unit literal

The literal, `Uncertain of T` in the prelude, exact correlation tracking, `Real`,
the derivative rules for the arithmetic and elementary sets, and §7's rendering.
`physics.constants` ships its CODATA table with real uncertainties.

**Nothing here is gated on the type checker's const machinery** (§8.4).

**`publishable-output.md` §6 rides on this stage.** Its significant-figure rule
and its `u` code need a value carrying a propagated uncertainty and need nothing
else from here, so the two land together or that note's §6.3 author-declared
precision is all there is.

### 8.2 Stage 2 — with `stats`

`from_covariance`, `stats.mean_with_uncertainty` and the other analytic
reductions of §3.5, `curve_fit` returning correlated uncertain parameters, and
`optimize`'s implicit-function-theorem entry point of §6.4. This is where the
feature stops being a literal notation and starts consuming real data.

### 8.3 An amendment to `scientific-libraries.md` §13's wave 2

That note's wave 2 ships `physics.constants` with *"quantities as `F64` with a
documented unit"*. `unit-literals.md` §9.3 already improved that to *"written
with units, checked for conversion, not checked for dimension"*. This note asks
for one more word:

> **Wave 2 ships `physics.constants` with units and uncertainties, checked for
> conversion, not checked for dimension.** The constant is
> `9.80665(15)<m/s^2>`, its value and its uncertainty both exact from the
> published table, and wave 5's retrofit changes the lowering target and not one
> call site.

The cost of *not* doing this is the one §1 named: a constants table whose second
column lives in a comment, written now and migrated later, which is the state
units were in before `<m/s^2>`.

### 8.4 The relationship with `const-expression-arithmetic.md`

That note landed while this one was being drafted and this note has read it. The
finding is a one-liner and it is good news:

> **This note asks nothing of it. Uncertainty is, as far as this set of notes
> goes, the first design that does not queue behind const-expression
> arithmetic.**

The reason is §3.4's: correlation is coefficient arithmetic on runtime floats, so
there is nothing to normalise in type position. The gradient has no static width
because §3.3 declined to give it one.

**One optimisation is available later and is deliberately not taken now.** A
const-width variant — `Uncertain of (T, const SOURCES: Int)` holding a dense
`SOURCES`-long gradient — would be allocation-free and `Copy`, and it is
admissible under that note's §2.3, since `Int` is its F0 kind and combining two
values would need only const *equality*, which §3.3 of it provides on day one. It
is declined because the width has to come from somewhere: either whole-program
inference, which is fragile, or a `measurement` block declaring a source
register, which is syntax for a performance problem that §3.6's boundary already
removed. **Because §6.2's signatures are `T: Real`, the width never appears in
one, so adding the const-width representation later is source-compatible.** It is
named here so that the deferral is a decision.

---

## 9. Diagnostics

Two codes claimed — `SC0277`–`SC0278`, both in the types range — plus two
amendments to codes this note does not own. The discipline is deliberate given
`README.md`'s record of five collisions in one day: **a note that needs a code
outside its block amends rather than claims.**

| Code | Phase | Condition | Applicable fix |
|---|---|---|---|
| `SC0277` | Types | An `Uncertain` value reaches a parameter bound by `Float` rather than `Real`, or any C-linked entry point | **yes** — `.nominal()`, or the named alternative |
| `SC0278` | Types (warning) | The result of `.nominal()` flows directly into `print`, `write`, an `f"…"` hole, or a `data.*` writer | **yes** — drop the `.nominal()` |
| `SC0013` | Lexical | *(existing, `unit-literals.md`)* **Gains** the malformed uncertainty group: `9.8()`, `9.8(a)`, `9.8(-1%)`, `9.8(1e-3%)`, `9.8e5(2)` | **yes** for the last — move the group before the exponent |
| `SC0252` | Types | *(existing, `unit-literals.md`)* **Gains** an integer width suffix on a literal carrying an uncertainty: `5i64(2)<kg>` | **yes** — drop the suffix |

### 9.1 `SC0277`, rendered

This is the error a user will meet most, and like `unit-literals.md` §10.1's
`SC0256` it lives or dies on not printing the type.

```
error[SC0277]: `linalg.solve` cannot propagate uncertainty
  --> fit.science:22:24
   |
20 | let a be matrix_from(readings)
   |          --------------------- Matrix of (Uncertain of F64)
   |
22 | let x, err be linalg.solve(a, b)
   |                            ^ this argument must be a plain float
   |
   = note: `solve` is bound `where T: Float` because it links against LAPACK,
           which takes doubles; there is nothing here that could carry a
           gradient across the call
   = help: solve with the nominal values and propagate through the solution:
             let x, err be linalg.solve(a.nominal(), b.nominal())
           and take the parameter covariance from `stats.least_squares`, which
           returns correlated uncertain parameters directly
```

The naive message is `` `Uncertain of F64` does not implement `Float` ``, which
tells a scientist nothing about what to do. The note and the help are the whole
value of the code.

### 9.2 `SC0278`, rendered

```
warning[SC0278]: this uncertainty is discarded on its way to being printed
  --> table.science:31:35
   |
29 | let g be constants.STANDARD_GRAVITY
   |          -------------------------- 9.80665 ± 0.00015 m/s²
   |
31 |     print(f"{row.label}  {g.nominal():.5f}")
   |                              ^^^^^^^^^ the uncertainty is dropped here
   |
   = note: a number published without its uncertainty cannot be checked by a
           reader, and this one is known
help: print the quantity itself
   |
31 |     print(f"{row.label}  {g:.5f}")
   |                           -----------
```

**The check is deliberately narrow and syntactic**: `.nominal()` whose result is
an immediate argument of `print` or `write`, an immediate `f"…"` hole, or an
immediate argument to a `data.*` writer. It is not a dataflow analysis. It will
miss the value that is bound first and printed three lines later, and that is
accepted — the cheap version catches the common shape, and the expensive version
would be the modality of §2.3 by another route.

`SC0278` is the whole of what design B would have bought over design A (§2.2),
and it is one checker arm.

---

## 10. The honest verdict

The project's owner wants results that go straight into a paper. Measured against
that, three separate judgements, and they do not all point the same way.

**Uncertainty in the type system does not serve it, and is a smaller win than it
looks.** The case for it rests on making "you published a number with no error
bar" a compile-time fact. §2.2 shows that an ordinary record type already makes
it a compile-time fact *that is visible*, and that the remaining increment — a
prohibition on the escape hatch — is recoverable by one warning. Against that
increment: a new kind or a modality, viral through every signature, with one
customer, failing the affordability test this project set itself in
`scientific-libraries.md` §12.5 and applied again in
`const-expression-arithmetic.md`. **Recommend the value-level design, without
reservation.**

**Uncertainty in the value does serve it, and it is cheap.** The whole
language-level cost is one lexer production and one interface — and the interface
was already being asked for under a different name, so the marginal cost is the
choice of a word. The library is a few hundred lines plus a fifty-entry
derivative table that is transcription. Against that: a CODATA table whose
uncertainties are real rather than commented, correct propagation through the
summary calculation that produces a published number, and a rendering rule that
makes the table publishable without the author thinking about significant
figures. For a language whose pitch is verification, an error bar that the
compiler can carry is directly on the pitch.

**The correlation answer is what makes it worth building rather than a toy.** A
value-level uncertainty type with naive magnitude propagation would be a liability
— §3.1 — and most of the reason to write this note down is to make sure the naive
version does not get built by default. The exact version costs eighty bytes and a
merge, and §3.6's scalar boundary is what makes that price obviously fine instead
of arguable. **If the correlation tracking of §3.3 is cut, this note's
recommendation changes from "build it" to "do not build it", and the fallback is
to keep uncertainties as a documented `F64` field on each constant with no
arithmetic at all.** That fallback is honest and it is not embarrassing; a wrong
error bar is.

**The strongest argument is not this note's and should be credited.**
`publishable-output.md` §13 says the whole of that design — the typed table, the
significant-figure rule, `sciencec report --strict` — is *"worthless without the
uncertainty type"*. That note is the one closest to the owner's stated goal of a
result that goes straight into a paper, and it is blocked. An uncertainty type is
therefore not a nice-to-have on the side of the catalogue; it is the missing
input to the feature that most directly delivers the goal. That raises the
priority without changing any of the design conclusions above, which is the ideal
shape for a late-arriving argument.

**Deferring is not recommended, and the reason is a date rather than a
principle.** `physics.constants` is wave 2. Every entry in it carries an
uncertainty. If this is not decided before that table is written, the table is
written without a place to put them, and the migration is a rewrite of every
constant plus every call site that consumed the `F64`. The decision is cheap now
and the cheapness expires.

---

## 11. What this note asks of others

**Of `scientific-libraries.md`:**

1. **§14.4's `Float` ask becomes `Real` with `Float` refining it** (§6.2). The
   edit to the catalogue's signatures is the one that note already requested; only
   the word in the bound changes. This is the single most time-sensitive item
   here, because the signatures are being written now.
2. **§14.6 is answered**: `Uncertain of T`, value-level, with exact correlation.
   That item can be struck and replaced with a pointer.
3. **§12.3 is unchanged, and §12's operators must borrow** (§5.3). `Quantity`'s
   `Add`/`Sub`/`Mul`/`Div` take `self` — the shared borrow, not the by-value
   `self: Self` — so that an uncertain quantity whose gradient has spilled is
   not moved by arithmetic.
4. **§13's wave 2 gains one word** (§8.3): constants ship with units *and
   uncertainties*, checked for conversion.
5. **§5.1 and §5.2 get the `Float`-only list** (§6.3): `floor` `ceil` `round`
   `trunc` `fract` `sign` `copysign` `ulp` `next_after` are `Float`-bound by
   decision, and the reason is one sentence in the catalogue.
6. **§7.1 gains analytic reductions** (§3.5): `stats.mean_with_uncertainty` and
   friends, so that nobody reaches `math.sum` with an uncertain element type and
   discovers the quadratic.
7. **§8.3's `curve_fit` should return correlated uncertain parameters**, via
   `Uncertain.from_covariance` (§3.7). It is the single most valuable consumer of
   this feature and it is already in the catalogue.

**Of `unit-literals.md`:**

8. **§2.1's literal grammar gains the uncertainty group**, and §8's ordering rule
   gains a fourth slot: significand, uncertainty, exponent, width suffix, unit
   (§4.5). The argument is that note's own — the group is part of the digits.
9. **`SC0013` gains the malformed-group cases and `SC0252` the integer case**
   (§9). Amendments, not claims.
10. **§5.2's exact-rational fold applies to the uncertainty**, and §6.1's affine
    offset does not (§4.5). One sentence each; both are invisible if wrong.

**Of `strings-formatting-and-docs.md`:**

11. **§3.3's `Display for Quantity` must delegate to `T`'s `DisplayNumber`
    rather than cast to `F64`** (§5.2). This is a contradiction with that note as
    written and it is argued rather than assumed; for `T = F64` the behaviour is
    identical.
12. **`publishable-output.md` §12 item 1's new `u` code should be decided jointly
    with `unit-literals.md` §7.3's unit selector**, since two notes now want to
    extend §2.1's one production. This note seconds the `u` request and adds
    nothing to it (§7.1).
13. **§10's "uncertainty rendering" open question is answered** — by
    `publishable-output.md` §6 for the rendering and by this note for the value —
    and §3.3's *"it belongs in `Quantity`'s design if anywhere"* should be
    corrected to *below* `Quantity`, in its representation parameter.

**Of `publishable-output.md`:**

14. **Its §14 question 1 is answered**: `Uncertain of T` propagates, first order,
    with correlations exact, and §3.3 here removes the specific objection that
    question raises — there is no derivation graph and no reference, only an
    owned flat gradient (§3.3).
15. **Its §13 risk is discharged** to the extent a design note can discharge one:
    §6 there is no longer conditional on an unspecified type. Its §6.3
    author-declared-precision path stays, because most real columns still have no
    uncertainty attached.
16. **Its Decision 9 step 1 groups a non-finite uncertainty with a zero one**
    (§7.2 here). This note would separate them and print the NaN. Flagged, not
    contradicted; that section owns the rule.
17. **Its §4's typed table already writes
    `Quantity of (Uncertain of F64, 0, 0, -1, 0, 0, 0, 0)`**, which is exactly
    §5's nesting order arrived at independently. Worth recording as agreement
    rather than leaving it to look like coincidence.

**Of `stdlib-core.md`:**

18. **`Uncertain of T` is a prelude type**, level 1, because the literal is in the
    grammar and a literal whose type needs an import is a trap. Only the record,
    its operators and its rendering are in the prelude; the ~50 derivative rules
    for elementary and special functions live with `math`, so `math.sin` of an
    uncertain value needs `use math` exactly as `math.sin` of a float does.
19. **`Real` and `Float` are interface declarations** in the level-1 surface, and
    the supertrait spelling (§6.2) is that note's call.

**Of `data-io.md`:**

20. **Serialisation decorrelates silently** (§7.3). A column of `Uncertain of F64`
    written to CSV and read back is a column of independent values with the right
    marginals and no correlations. That needs a paragraph in that note and
    probably a doc-level warning at the writer; it is not fixable without a
    correlation-preserving container format, which is out of scope for both notes.

**Of the core spec:**

21. **§4.2** gains the uncertainty group in *Literals*, beside `42i32`, `2.5f32`
    and the unit literal.
22. **§9** gains `SC0277` and `SC0278`.
23. **`README.md`** gains this note's row in the index and in the claimed table.
    Its free-list currently shows `SC0277`–`SC0278` against
    `stdlib-shape-and-packages.md`, whose §11 returned them; the standing-asks
    row for *an uncertainty type* records four callers and can now point here.

---

## 12. Risks

**First-order propagation will be trusted past where it is valid.** §3.8 is the
one place this design is knowingly approximate, and a printed `± ` carries
authority the linearisation has not earned for a strongly curved function. The
mitigation is documentation at the top of the module and a named Monte Carlo
escape, and documentation is a weak mitigation. This is the residual risk that
cannot be designed away, because the alternative — carrying a Hessian — triples
the representation to fix the rarer problem.

**Serialisation decorrelates and nothing catches it.** Write an uncertain column,
read it back, and the correlations are gone with no diagnostic and no visible
change in the printed values. §7.3 and ask 20 name it; neither fixes it. Of
everything in this note, this is the failure most likely to produce a wrong
number in a real paper, because it happens between two programs that are each
individually correct.

**`digit (` is consumed forever.** Exactly as `unit-literals.md` §12 says of `<`
after a numeric literal. The position is currently dead rather than contested
(§4.2), so the risk is lower than that note's — but it is the same kind of risk
and it should be taken deliberately before the first release, not drifted into.

**The scalar boundary will be tested immediately.** §3.6 says `Uncertain` does
not go in a tensor, and the first user with a spectrum will put one there anyway,
because `Array of Uncertain of F64` is legal. They will get correct answers
slowly and then very slowly, and they will conclude the feature is slow rather
than that it is being misused. The reduction cliff of §3.5 is the specific shape
this takes. A diagnostic is not available — the array is well-typed — so the
answer has to be that `stats` offers the right thing under an obvious name, which
is ask 6.

**Two spellings inside the group.** `100(10)` and `100(1%)` denote the same value
and §4.4 keeps both, against `syntax-revision-2.md` §1. The argument is that they
carry different provenance and so cannot be canonicalised, which is exactly the
test that note applies — but it is the first time this set of notes has *added* a
second spelling rather than removed one, and if the argument is judged too clever
the relative form is the thing to cut. Cutting it changes nothing else.

**`Uncertain` is not `Copy`.** §3.3's spill makes it a moving type, which puts
ownership friction on a value scientists will treat as a number. Every operator
borrowing (§3.3, §5.3) removes most of it, and some will remain — in
closures, in collections, at struct field moves. The fixed-capacity alternative
that would have made it `Copy` is rejected in §3.5 for a reason this note cannot
compromise on, so the friction is structural.

**Fifty derivative rules is fifty opportunities for a sign error.** Each is one
line, each is individually trivial, and a wrong one produces a plausible wrong
error bar rather than a crash. They need property tests against finite
differences — cheap to write, and they should ship with the table rather than
after it, for the same reason `SC0140` should ship with the error model.

---

## 13. Open questions

**Does an uncertainty belong in a `Frame` column type, and how does it get
there?** `data-io.md` §4.3 turns a record type into a schema. A CSV with `value`
and `value_err` columns is the universal on-disk convention and has no
relationship to `Uncertain of F64` as a field type. Whether the reader pairs the
columns, and how it learns which pairs with which, is that note's question and
mirrors the one `unit-literals.md` §13 already left open for units. The two
should probably be answered together, since a real instrument file has both.

**Should a `Fit` expose its parameter covariance as a matrix, as correlated
uncertain values, or both?** §3.7's `from_covariance` makes the second available
and the first is what every existing library returns. Offering both is two ways
to ask one question; offering only the second loses the matrix that users need for
their own propagation. Deferred to `stats`' design.

**Is there a case for a degrees-of-freedom field?** A standard uncertainty from
four measurements and one from four hundred are not the same thing, and the
Welch–Satterthwaite combination needs the effective degrees of freedom to produce
a coverage interval. Carrying it would let `expanded(k)` and confidence intervals
be correct rather than approximate. It is one more field in the gradient's terms
and it is not obviously worth it; nobody has asked.

**Does `Uncertain` need `Eq` at all?** §6.2 requires `Ord`, over nominals, because
`min`, `max`, `clamp` and sorting need it. Equality on two uncertain values is
either equality of nominals — which is the float equality trap with an extra step
— or a statistical compatibility test, which is a `stats` function and not an
operator. The current position is that `is` compares nominals and is documented as
doing so; the alternative, not implementing `Eq`, is defensible and would be a
small surprise in a language where `is` works on everything else.
