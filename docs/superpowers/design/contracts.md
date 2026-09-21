# Science — Design: contracts, and `assert` as a construct

Date: 2026-09-18
Status: draft for review
Owns: `requires`, `ensures`, `invariant`, the meaning of `assert` as a language
construct rather than a runtime call, the three-rung discharge ladder, and the
build-profile rule that says which rung survives into a release binary.
Answers, and is the note it asks for: `reserved-words.md` §4.2 —
*"keep `assert` reserved and specify it in a later note as a construct, not a
library function"* — and its §4.2 **Status** paragraph, which records that only
the first half of its own argument has shipped.
Distinguishes itself from, and does not touch: `stdlib-standard.md` §7.4, which
decided that `testing` does **not** take `assert` and gave the table of
differences between `check` and `assert`. That table is this note's starting
point and §1 reproduces its verdict rather than re-deriving it.
Also depends on: `const-expression-arithmetic.md` §2 (the five-operator grammar
and the normal form, which §3 reuses exactly and does not extend),
`effects.md` §5.1 (purity, which a contract expression requires),
`region-inference.md` (the assumption channel §5 writes into),
`indexing-and-array-literals.md` §5 (bounds checks, the first consumer),
`type-checking-and-mir.md` (THIR and MIR, where §6 lowers),
`llm-ergonomics.md` §1 (the diagnostic is the teaching channel),
`property-testing.md` §7 (which says outright that it is not part of the
verification claim and points here).
Syntax: `syntax-revision-2.md` and `syntax-revision-3.md`.
Diagnostics claimed: **`SC0163`–`SC0165`** (syntax) and **`SC0590`–`SC0609`**
(types).

---

## 0. The verdict up front

§1 of the core spec makes a verification claim. The language today has one
verification-shaped construct, `assert`, and it is a runtime abort — the same
thing a call to `panic` would be, with better syntax. `reserved-words.md` §4.2
said so on the day the word was reserved and asked for this note.

> **Decision 1. A contract is a proposition attached to a definition, and the
> compiler is obliged to try to discharge it. It lands on exactly one of three
> rungs — **proved**, **checked**, or **assumed** — and which rung it landed on
> is reportable, per definition, by a tool the user can run.**

The three-rung ladder is the whole design. Eiffel has contracts and never tries
to prove them; Dafny proves them and refuses to compile what it cannot; Rust's
`debug_assert!` checks them and forgets them. Each of the three is a defensible
point and each is one rung of the ladder with the other two sawn off.

## 1. What the three rungs are

| Rung | Meaning | Cost at runtime | Reported as |
|---|---|---|---|
| **Proved** | The compiler discharged the proposition from the normal form of `const-expression-arithmetic.md` §2 | Nothing. No code is emitted | `proved` |
| **Checked** | It could not be discharged and it is decidable at runtime, so a check is emitted | A branch | `checked` |
| **Assumed** | It could not be discharged and it is *not* checkable — it quantifies, or it names something the runtime cannot see | Nothing | `assumed`, and it is a **warning** |

**A contract never silently disappears.** That sentence is what separates this
from `debug_assert!`, and it is the failure `stdlib-standard.md` §7.4's table
calls *"the second-worst failure mode available"* in the neighbouring case. The
release profile may choose to strip the *checked* rung — §7 — but the tool still
reports the contract as having been stripped, and the reporting is not
configurable.

## 2. The grammar

```
contract-clause := ("requires" | "ensures") expression
invariant-item  := "invariant" expression
```

`requires` and `ensures` appear in a signature, after the return type and
interleaved freely with `where`:

```science
def bisect(
        f: (F64) -> F64,
        lo: F64,
        hi: F64,
        tolerance: F64) -> F64
    requires lo < hi
    requires tolerance > 0.0
    ensures result >= lo and result <= hi:
    ...
```

`invariant` appears in a `type` body, after the fields:

```science
type Probability:
    value: F64
    invariant value >= 0.0 and value <= 1.0
```

> **Decision 2. `requires`, `ensures` and `invariant` are contextual keywords,
> recognised only in signature position and `type`-body position respectively.
> §13's reserved list gains nothing and `reserved-words.md` §2's audit is
> unchanged.**

This is not free and the bill is short. `requires` and `ensures` are English
words a scientist may want as an identifier, and under this decision they still
can be: `let requires be …` compiles, because item and signature position is the
only place the parser looks. The real cost is one match arm each and a
lookahead rule that is already needed for `where`. The alternative — three new
reserved words on top of the one `stdlib-standard.md` §7.3 already added back —
was not affordable against `reserved-words.md`'s argument that §13 is a budget.

### 2.1 `result` is bound in `ensures` and nowhere else

`ensures` needs a name for the returned value. `result` is that name, bound only
inside an `ensures` expression, shadowing anything of that name. It is **not**
reserved — outside an `ensures` clause the word is an ordinary identifier, and
`let result be …` inside the body is both legal and extremely common in the
audience's code. A function that does that and also has an `ensures` clause is
fine: the clause is checked at the return, against the returned value, and the
local named `result` is invisible to it. `SC0163` fires if `result` appears in a
`requires`, where there is nothing for it to mean.

A function returning the error pair `-> (T, Error?)` binds `result` to the
**first** component and `error` to the second, and an `ensures` clause on such a
function is checked **only on the path where `error` is `null`**. Any other rule
makes `ensures` unusable for every fallible function in the language, which is
most of them.

## 3. Decision 3 — the prover is the const evaluator, and it is not extended by one operator

> **Decision 3. The proved rung is decided by exactly the normal form
> `const-expression-arithmetic.md` §2.3 already specifies — `k + Σ cᵢ·aᵢ` over
> `+`, `-`, `*` by a literal, and division by a literal — extended with the
> comparison and boolean connectives and nothing else. A proposition outside
> that fragment goes to the checked rung without an attempt.**

This is the single most important decision in the note and it is a decision to
*not* build something. The temptation is an SMT solver: Z3 is a library, Dafny
proves real programs with it, and it would discharge far more than a linear
normal form does.

It loses on four counts, and the first is fatal:

1. **A solver makes compilation time unpredictable and occasionally unbounded.**
   `self-hosting.md` sets a bootstrap the compiler must clear and
   `codegen-and-linking.md` writes down the first performance target the project
   has. A prover that is fast on Tuesday and times out on Wednesday, on
   unchanged source, breaks both — and it breaks them non-locally, so the author
   of the timeout is not the author of the change.
2. **It is a dependency of a size this project refuses elsewhere.**
   `native-dependencies.md` prices every linked C library and
   `package-manager.md` bans build scripts. Z3 is larger than the compiler.
3. **The failure message is unreadable.** `llm-ergonomics.md` §1 says the
   diagnostic is the only teaching channel. "unknown" from a solver, with no
   counterexample and no locus, is the worst diagnostic in the language.
4. **The linear fragment is where the value is.** The contracts a scientist
   writes are `n > 0`, `lo < hi`, `0 <= p <= 1`, `len(a) == len(b)`, `tolerance
   > 0`. These are linear. The exceptions — `result * result ≈ x` for a square
   root — are not provable by *any* practical solver over floats either, and go
   to the checked rung under any design.

**The escape, and it is deliberate:** the fragment is shared with the shape and
unit machinery, so every improvement to it improves three features at once, and
the cost of widening it later is a single note against a single normal form.
That is the shape of decision this project keeps making and it is the right one
here.

### 3.1 Floats are never proved

> **Decision 4. A proposition mentioning a floating-point value is never on the
> proved rung, regardless of its form.**

`x > 0.0 and x < 1.0` implies nothing about `x * x` under IEEE 754 with rounding,
NaN and signed zero in play. `reproducibility.md` §3.2's five float channels are
the same problem seen from the other side. Float contracts are useful and they
are checked, not proved, and the tool says so. This will surprise someone and it
is better than a wrong proof.

## 4. Decision 5 — what `assert` becomes

> **Decision 5. `assert(cond)` keeps its current spelling, its current runtime
> behaviour on failure, and its statement position. What changes is that it is
> now on the ladder: the compiler attempts to discharge it, emits no code when
> it succeeds, and records it as an assumption for the region and shape
> machinery when it cannot.**

This is a strictly compatible change and it discharges the second half of
`reserved-words.md` §4.2's argument, whose Status paragraph records that only the
first half shipped. Every `assert` in the existing corpus keeps working. The
difference is visible in exactly two places: a provable `assert` costs nothing at
runtime, and an `assert` now *teaches the compiler something* downstream of it.

```science
def take_first(xs: borrowed (Array of F64), n: Int) -> Slice of F64:
    assert(n <= xs.length())
    xs[0..n]              # no bounds check emitted: the assert discharged it
```

That second effect is the one with leverage and it is why `assert` belongs in
this note rather than in `testing`. `indexing-and-array-literals.md` §5 wants
exactly this channel and today has no way to be told.

### 4.1 `assert` against `check`, restated

`stdlib-standard.md` §7.4's table stands unchanged and gains one row:

| | `check` | `assert` |
|---|---|---|
| Where | `test` and `property` items only (`SC0269`) | Anywhere |
| On failure | Records a failure; the runner continues | Panics, and aborts |
| Compiler | Knows nothing | **Discharges it, or records it as an assumption** |
| Present | Now | **This note** |

## 5. Decision 6 — the assumption channel, which is what pays for the note

> **Decision 6. An assumed or checked contract is recorded as a fact on the
> MIR block that dominates its use, available to bounds-check elimination,
> shape checking and region inference. The record is a fact, not a hint: a
> consumer may rely on it, because the checked rung guarantees the program
> aborts before reaching a block whose facts are false.**

This is the sentence `reserved-words.md` §4.2 was reaching for with *"record the
assumption for the region and shape machinery to use"*, made precise. It is also
the honest answer to "what does a contract buy a scientist who does not care
about verification": it buys bounds-check elimination on the hot loop, which is
`codegen-and-linking.md`'s performance target, obtained from a line the author
wrote for documentation.

The soundness argument is one line and it depends on §7: **the checked rung may
be stripped from a release binary only if every fact derived from it is also
discarded.** A design that strips the check and keeps the elimination is how
`unsafe` gets into a language by the back door. §7.1 makes this a profile
invariant rather than a convention.

## 6. Where contracts are checked, exactly

| Contract | Checked | On whom |
|---|---|---|
| `requires` | At the **call site**, before the call | The caller. A failing `requires` is the caller's bug and the diagnostic says so |
| `ensures` | At each `return` and at the final expression | The callee |
| `invariant` | After construction, and after any `mutable self` method returns | The type's own methods |

Checking `requires` at the call site rather than in the prologue is what makes
the blame assignment right and it is what lets the proved rung work at all: the
caller usually knows the argument is a literal `10`, and the callee never does.
The cost is that the check is emitted once per call site rather than once per
function, which is a code-size trade the inliner reverses in practice.

### 6.1 Invariants and the borrow checker agree, for once

An `invariant` is checkable after a `mutable self` method precisely because
ownership guarantees nobody else held a reference to the value while the method
ran. In a language with aliasing this construct needs a whole theory; here it
falls out of §6 of the core spec. `effects.md` §2.3's observation that *"the free
discipline is the discipline"* applies exactly.

A field that is a `mutable borrowed T` defeats this, because the borrow can be
written through without any method being called. `SC0592` refuses an `invariant`
that mentions such a field, names it, and says why.

## 7. Decision 7 — the build profile decides the checked rung, and only that

> **Decision 7. Three profiles. `debug` checks everything checkable.
> `release` checks `requires` and `invariant`, and strips `ensures`.
> `release --unchecked` strips all three and, per Decision 6, discards every
> fact derived from them.**

The middle row is the one with content. `requires` is a boundary check — it
catches the caller's bug, it is usually one comparison, and stripping it is how
a library ships an exploitable precondition. `ensures` is the author checking
their own work, it is often the expensive one (`result` may be a whole array),
and by release time it has been checked by every test and every property run.
Stripping the self-check and keeping the boundary check is the same trade every
serious system makes.

`--unchecked` exists because someone will need it for an inner loop and a flag
they can find is better than a fork of the compiler. It is reported by the
provenance record (§8), so a result produced with it is distinguishable.

## 8. Reporting, and the tool

`sciencec contracts ./src` prints one line per contract with its rung. This is
the deliverable of Decision 1 and without it the ladder is unobservable:

```
src/bisect.science:9   requires lo < hi                     checked
src/bisect.science:10  requires tolerance > 0.0             proved   (callers: 3/3)
src/bisect.science:11  ensures  result >= lo and ...        checked
src/stats.science:44   invariant value >= 0.0 and ...       assumed  SC0594
```

`assumed` is a warning, always, because an assumed contract is a claim the
compiler could not check and could not verify — it is a comment with syntax. The
warning is how a reader tells the two apart without running the tool.

The **provenance record** (`reproducibility.md` §5.2) gains one integer triple,
`contracts: [proved, checked, assumed]`, in the build record. It is three
integers, not floats, so Decision 10's rule holds. A reviewer can then see that a
published result came from a binary with 40 assumed contracts, which is exactly
the kind of thing `reproducibility.md` §5.4 says a reviewer should be able to do
at low cost.

## 9. What this note does not do

- **No loop invariants and no `variant`/termination clauses.** They are where a
  verification system starts to pay for real and they need the solver §3
  refused. Nothing here precludes them; they would need their own note and a
  wider fragment.
- **No quantifiers.** `for all i in 0..n` in a proposition is expressible and is
  never checkable in bounded time for a runtime rung, so it would be `assumed`
  always — a construct whose only rung is the warning rung is not worth the
  grammar. When the fragment widens, it comes back.
- **No contracts on interface methods.** A contract on `interface Ord` that every
  implementation must satisfy is behavioural subtyping and it is a genuinely hard
  design; `Ord`'s laws stay prose in `stdlib-core.md` for now.
- **No proof of `pure`.** `effects.md` owns that and has inferred it already.

## 10. Diagnostics allocated

| Code | Phase | Means |
|---|---|---|
| `SC0163` | Syntax | `result` in a `requires` clause (§2.1) |
| `SC0164` | Syntax | `invariant` outside a `type` body, or `requires`/`ensures` outside a signature |
| `SC0165` | Syntax | A contract clause after the `:` that opens the body |
| `SC0590` | Types | A contract expression is not `Bool` |
| `SC0591` | Types | A contract expression is not pure (`effects.md` §5.1) |
| `SC0592` | Types | An `invariant` mentions a `mutable borrowed` field (§6.1) |
| `SC0593` | Types | A `requires` is **disproved** at a call site — a hard error, not a rung |
| `SC0594` | Types | *Warning.* A contract landed on the assumed rung, with the reason |
| `SC0595` | Types | An `ensures` on a function with no return value |
| `SC0596` | Types | A contract references a name not in scope at its checking point |
| `SC0597`–`SC0609` | Types | Held for the loop-invariant form of §9 |

`SC0593` is the rung that is not in §1's table on purpose. A proposition the
prover can *refute* — `requires n > 0` at a call site passing a literal `0` — is
not a failed proof, it is a bug found at compile time, and it is the single best
thing this note produces. It should be the example in every piece of
documentation about contracts.

## 11. What this note asks of others

| Ask | Of | Size |
|---|---|---|
| Extend the normal form with comparisons and connectives | `const-expression-arithmetic.md` §2.3 | Small; the arithmetic is unchanged |
| Consume facts from the assumption channel | `indexing-and-array-literals.md` §5, `region-inference.md`, `broadcasting.md` §5 | Each is a consumer of one API |
| Amend `SC0269` / §7.4's table for the new `assert` | `stdlib-standard.md` §7.4 | One row, already drafted above |
| `contracts: [p, c, a]` in the build record | `reproducibility.md` §5.2 | Three integers |
| Record that §4.2's ask is discharged | `reserved-words.md` §4.2 Status | One sentence |
| The three profiles | `package-manager.md` §2 (`[build]`) | One profile key |

## 12. Risks

- **The proved rung will be narrower than anyone expects.** Decision 3 is a
  small fragment and Decision 4 excludes floats, which is most of a scientist's
  code. A user who writes ten contracts and sees ten `checked` will conclude the
  feature does nothing. The mitigation is §8's tool and honest documentation;
  the alternative is a solver, and §3 gives four reasons that is worse.
- **`ensures` stripped in release is a real semantic difference between
  profiles** and someone will be bitten by a program that passes its tests and
  ships without them. It is the same trade as `debug_assert!` and it is at least
  reported rather than silent.
- **Contextual keywords in signature position** interact with the still-owed
  `(A) -> B` closure type from `README.md`'s standing asks. Both live in the
  same lookahead and whoever builds the second should check the first.
