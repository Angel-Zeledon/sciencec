# Science — Design: strings, formatting, and documentation comments

Date: 2026-09-16
Status: draft for review
Written against: `docs/superpowers/design/syntax-revision-2.md` (all code below is
revision-2 syntax).
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §4.2
(literals), §5.4 (the interface list), §8 (the closed library surface), §9
(diagnostics), §13 (reserved words).
Settles: `data-io.md` §11.5, which deferred the shape of formatting and named
itself as waiting on the answer.
Related: `python-interop.md` §3.3 and §7 (what a formatted message looks like on
the other side of the membrane), `scientific-libraries.md` §12 (units, and where
`9.8 m/s²` comes from), `unit-literals.md` §2 (whose ASCII unit spelling §3.3
here reconciles with the typeset one), `script-mode.md` §1.3 (which writes a
variadic `print` that §4.1 here removes), `llm-ergonomics.md` (the corpus
argument in §5.5).

---

## 0. What this note settles, and why it is one note

Three features, and the temptation is to split them into three notes. They are
one note because each of the three is unusable without the other two, and
because the same audience meets all three in their first hour.

- A scientist's most common single line of code is a line of output. Today that
  line cannot be written: there is no interpolation and no formatting, so the
  only expressible output is `print(x)`, and `print(x)` for an `F64` is
  `0.3333333333333333`.
- The difference between `0.3333333333333333` and `0.333` is the difference
  between a table that goes in a paper and a table that does not. This is not a
  polish feature. It is the last hundred metres of every program the audience
  writes.
- And the whole thing needs `Display` to mean something more than the one
  sentence §5.4 gives it: *"`Display` is what `print` requires"*, followed by
  nothing.

Documentation comments join them for a narrower reason: the language has exactly
one comment form, `#` to end of line, and no way to attach text to a
declaration. F1 ships notebooks, where `help(f)` and shift-tab are how a
scientist finds out what a function does. A notebook tier over a language with
no doc comments has nothing to put in the tooltip.

**What this note decides.** Summarised, with the sections that argue each.

| # | Decision | Serves |
|---|---|---|
| §1.2 | Interpolation is **marked**: `f"mean {μ}"`. A plain `"…"` is literal text. | Scientists (JSON, LaTeX, regex survive untouched); programmers (no new escaping rule in existing strings) |
| §1.3 | Three literal kinds: `"…"`, `f"…"`, `r"…"`. `{{` and `}}` inside `f"…"` only. | Both |
| §1.4 | Arbitrary expressions inside an interpolation, not just names. | Scientists (`{frame.len()}` is the common case); costs the lexer a mode stack |
| §2.4 | The format spec is **checked at compile time against the argument's type**. | Both; this is §1 of the core spec applied to the most common runtime crash in Python |
| §3.1 | `Display` gains a `Formatter` parameter and therefore sees the spec. | Scientists (a `Quantity` can honour `.3f` and still print its unit) |
| §3.2 | A second interface, `Inspect`, requested with `{x!i}` and never with `{x:?}`. | Programmers |
| §3.3 | A marker interface `DisplayNumber` grants a type the numeric format codes. | Scientists (units, decimals, complex numbers) |
| §4.1 | **`print` is unary, not variadic.** This is what `data-io.md` §11.5 was waiting for. | Both; removes a duplicate spelling, and removes variadics from the language |
| §5.1 | Doc comments are `##` lines before the declaration, not `"""…"""` in the body. | Programmers primarily; §5.3 shows scientists lose nothing |
| §5.5 | Code samples in doc comments are compiled and run. | Both, and the model-generated-code thesis most of all |
| §6.3 | Unicode **identifiers** yes, Unicode **operators** no. | Scientists get `σ` and `Δt`; nobody gets a second spelling of `<=` |

---

## 1. Interpolation

### 1.1 The shape

```science
let μ be mean(xs)
let σ be deviation(xs)
print(f"media {μ:.3f} y desviación {σ:.3f}")
```

Braces inside a string literal. This is Python's, C#'s, Rust's and (modulo the
sigil) Kotlin's and Swift's notation; it is the notation the audience reads
without being taught, and it is the notation a model generates by default. There
is no serious competitor and this note does not pretend to weigh one.

The question that is actually open is not the braces. It is **which string
literals interpolate**, and that question has a real cost on both sides.

### 1.2 Decision: interpolation is marked

**Decision.** A string literal interpolates only when it carries the `f` prefix.
`"{a}"` is four characters of text. `f"{a}"` is the value of `a`.

The three candidates, with what each costs:

| Design | Example | Cost |
|---|---|---|
| **A. Marked literal** (Python, C#) | `f"mean {μ}"` | One character per interpolating literal, and users will forget it |
| **B. Every literal interpolates, brace sigil** (C#'s older `$`, Rust's `format!` under a macro) | `"mean {μ}"` | Every `{` in every string in the language must be written `{{` |
| **C. Every literal interpolates, escape-character sigil** (Swift `\(μ)`, Kotlin `$μ`) | `"mean \(μ)"` | Braces stay free, but the notation is Swift's and nobody else's |

**Why not B.** The audience writes JSON and LaTeX. Those are not exotic corners;
they are the two text formats a scientist produces most after CSV.

```science
let template be "\\frac{\\sigma}{\\sqrt{n}}"
let body be "{\"station\": \"A12\", \"channels\": 768}"
```

Under B both of those become `{{`-riddled and unreadable, and — much worse — a
LaTeX fragment such as `"\\frac{a}{b}"` where `a` and `b` happen to be bindings
in scope would **silently interpolate**. That is a wrong answer produced quietly,
in a language whose entire premise (§1 of the core spec) is catching wrong
answers before the run starts. B is not merely inconvenient; it is off-thesis.

**Why not C.** C is genuinely clever: Swift and Kotlin can interpolate every
literal at no escaping cost because their sigil is a character that *already* had
to be escaped inside a string. `\` is already `\\` in §4.2's escape list, so
`\(…)` steals nothing. Braces have never had to be escaped and stealing them is
what makes B expensive.

C loses on recognition. `f"…"` is what every NumPy and pandas user types; `\(…)`
is what Swift users type, and Swift users are not this audience. §0 of
`syntax-revision-2.md` sets the rule — *use the symbol where one exists and
everybody already reads it* — and for this audience the thing everybody already
reads is the f-string.

**The cost of A, stated.** People forget the `f`. In Python this produces
`mean {μ}` on stdout — a wrong answer, silently, at runtime, which is the exact
failure mode this language exists to remove. So:

> **A plain string literal containing what would be a valid interpolation, where
> every name inside it resolves in the enclosing scope, is `SC0172`** — a
> warning, with an applicable fix that inserts the `f`, and a second fix that
> doubles the braces.

That converts Python's silent wrong output into a diagnostic with two buttons.
It is the whole reason A is affordable, and it should ship with the feature.

The lint's own cost is a false positive rate in LaTeX-heavy code: `"\\frac{a}{b}"`
in a file that also has bindings called `a` and `b` will fire. The suppressions
are `r"…"` (§1.3), which the lint never inspects, or writing it as `f"\\frac{{a}}{{b}}"`,
which is explicit. Neither is free, and §9 names this as a risk rather than
pretending it away.

### 1.3 Literal braces, and the three literal kinds

**Decision.** §4.2 gains two literal prefixes. There are exactly three kinds and
they do not combine.

| Form | Escapes processed | Interpolates | For |
|---|---|---|---|
| `"…"` | yes (§4.2's list) | no | Ordinary text, JSON, anything with a `{` |
| `f"…"` | yes, plus `{{` and `}}` | yes | Output |
| `r"…"` | no | no | LaTeX, regular expressions, Windows paths |

- Inside `f"…"`, a literal brace is `{{` or `}}`. Inside `"…"` and `r"…"` a brace
  is a brace and needs nothing. This is the payoff of §1.2: the escaping rule
  exists only in the literals that asked for it.
- `r"…"` has no way to contain a `"`. That is a real limitation, it is the price
  of not adopting Rust's `r#"…"#`, and the answer for a string containing both a
  quote and a backslash is an ordinary `"…"` with `\\`. If the limitation bites,
  `r#"…"#` is the known extension and it is source-compatible to add.
- `rf"…"` is **rejected**. A raw interpolating string is a fourth set of rules
  for a case nobody has demonstrated, and the combination that people reach for —
  LaTeX with a value substituted in — is better served by building the string in
  two pieces where the seam is visible.

**Lexical check: `f` and `r` do not become reserved words.** Science calls are
always parenthesised (§4.6) and there is no juxtaposition operator, so `f"x"` can
only be a prefixed literal and never `f` applied to `"x"`. `let f be 1` remains
legal, `let r be radius` remains legal, and §13's list does not grow. This was
worth checking before choosing the prefix.

### 1.4 What may appear inside an interpolation

**Decision.** An arbitrary expression, with three restrictions.

```science
print(f"kept {frame.len()} of {frame.len() + frame.rejected_count()} rows")
print(f"{station.name} at {(t - t0) / 60.0:.1f} min")
print(f"{"inner" is not name}")
```

The restrictions:

1. **It is an expression, not a statement.** No `let`, no `return`, no `break`.
   `SC0175`.
2. **It may not span a line.** A string literal does not span lines in §4.2 and
   an interpolation does not change that.
3. **It may not contain a `#` comment.** The `#` would swallow the closing brace
   and the quote.

Nested string literals, including nested `f"…"`, are admitted to any depth. That
is not generosity; it is the consequence of deciding to lex the interior
properly, and once the interior is lexed properly there is no reason to forbid
it.

**Why not bare names only.** Restricting to `f"{μ}"` makes the lexer trivial —
scan to the next `}` and you are done — and it is what Python shipped with in
3.6 and spent until PEP 701 in 3.12 climbing out of. The case that kills it is
the one `data-io.md` writes on its first output line: `frame.len()` is a method
call, and a rule that admits `{μ}` but not `{frame.len()}` teaches users a
boundary they will hit within minutes and cannot predict. Half a feature here is
worse than none.

### 1.5 What arbitrary expressions cost the lexer, honestly

This is the largest single piece of new work in this note and it lands on a lexer
that already has 117 tests and has been migrated twice today.

A scanner that must find the `}` matching a `{` cannot do it by counting
characters, because `f"{m["a"]}"` contains a `"` that does not end the literal
and `f"{f"{x}"}"` contains braces that are not the match. The only correct
implementation is to **tokenize the interior**, which means the lexer stops being
a function over characters and becomes a machine with a mode stack:

- `f"` pushes a *string mode* frame.
- Text runs emit `StrFragment` tokens.
- `{` pushes an *expression mode* frame and emits `InterpStart`.
- `}` at brace-depth zero within the current expression frame pops it and emits
  `InterpEnd`; a `}` inside a nested `(`/`[`/`{` does not.
- `"` inside expression mode pushes another string frame.

The costs, each of which is real:

- **The indentation machine must be suspended inside an interpolation**, exactly
  as §4.2 already suspends it inside unclosed brackets. That rule now has a
  second trigger.
- **Error recovery gets harder.** An unterminated interpolation can leave the
  mode stack arbitrarily deep, and the lexer must unwind to a sane state or a
  single missing `}` produces fifty cascading diagnostics. §10.3's byte-exact UI
  tests will notice.
- **The formatter and the language server must re-render the interior**, so the
  interpolation is not opaque to `sciencec fmt` the way a string is. Both are out
  of scope for F0 (§12) and both inherit this.
- **Spans.** A diagnostic pointing inside an interpolation needs a span in the
  original file, through the escape processing. Getting this wrong produces
  carets that point at the wrong character, and §9 of the core spec makes spans
  non-negotiable.

Rust avoids all of this by making `format!` a macro over a literal, which Science
cannot do because §12 reserves macros and does not implement them. Swift, Kotlin,
JavaScript and now Python all pay exactly this cost. It is paid once.

### 1.6 Interpolation borrows

**Decision.** An interpolation borrows its operands. `f"{doc}"` does not move
`doc`.

This falls out of `Display` taking `self` — the shared-borrow receiver, not
the by-value `self: Self` (§3.1) — and §6.3's auto-borrow, but it must be
stated, because in a language with ownership the alternative is a
footgun of the first order: a debugging `print` that moves the value you were
about to use is a diagnostic in the `SC0300` range caused by a line the user
added to understand a different problem.

Consequence: the expression inside an interpolation is subject to the ordinary
borrow rules, so `f"{v.pop()}"` mutating a value borrowed elsewhere in the same
statement is an error like any other. That is correct and no special case is
needed.

### 1.7 What it compiles to

`f"…"` has type `String`. It lowers to a builder over the fragments, with the
capacity pre-computed from the literal fragments plus a per-type estimate for
each hole, so the common case is one allocation.

**The compiler is permitted, not required, to elide the allocation** when an
f-string appears directly as the argument to `print`, `write`, `panic` or their
stderr twins and is never bound: the fragments are rendered straight into the
sink. This is an optimization with a visible consequence — partial output if the
program aborts mid-render — and it is opt-in to the implementation rather than a
promise in the language.

---

## 2. The format specification

### 2.1 The grammar

After a `:` inside an interpolation:

```
spec      := [[fill] align] [sign] ['#'] ['0'] [width] [grouping] ['.' precision] [code]
align     := '<' | '>' | '^'
sign      := '+' | '-' | ' '
grouping  := ',' | '_'
width     := integer | '{' expression '}'
precision := integer | '{' expression '}'
code      := 'f' | 'e' | 'E' | 'g' | 'G' | '%' | 'd' | 'x' | 'X' | 'o' | 'b' | 's'
```

This is Python's mini-language with four things removed (§2.6) and one thing
added (`_` grouping). Adopting it wholesale is deliberate: the audience already
knows it, models already generate it, and every character of divergence is a
character of surprise for no gain.

A separate conversion flag precedes the `:` and selects the interface rather than
the rendering:

```
conversion := '!' ('i' | 's')
```

`!i` is `Inspect` (§3.2), `!s` is `Display` and is the default. `f"{x!i:>20}"` is
legal: inspect it, then pad it.

### 2.2 The codes, and what each is for

| Code | Applies to | Example | Output |
|---|---|---|---|
| `f` | floats, `DisplayNumber` | `f"{t:.3f}"` | `0.333` |
| `e` `E` | floats, `DisplayNumber` | `f"{p:.2e}"` | `4.51e-07` |
| `g` `G` | floats, `DisplayNumber` | `f"{x:.3g}"` | `0.000451` — **precision is significant figures** |
| `%` | floats, `DisplayNumber` | `f"{r:+.2%}"` | `+12.34%` |
| `d` | integers | `f"{n:d}"` | `1234567` |
| `x` `X` | integers | `f"{mask:#x}"` | `0xff` |
| `o` | integers | `f"{mode:o}"` | `755` |
| `b` | integers | `f"{flags:#b}"` | `0b1010` |
| `s` | anything `Display` | `f"{name:<12}"` | `A12         ` |

`g` is the significant-figures code and it is the one a scientist should reach
for most: `.3g` is three significant digits whatever the magnitude, which is what
"report three sig figs" means and what `.3f` does not do.

The grouping flags:

- `,` gives `1,234,567`.
- `_` gives `1_234_567`, matching Science's own integer literal syntax (§4.2), so
  a number printed by a program can be pasted back into one.

`#` is the alternate form: `0x`, `0o`, `0b` prefixes for `x`/`o`/`b`, and a
retained decimal point for `f`/`e`/`g` at precision zero.

### 2.3 Defaults, decided here because a numeric language cannot leave them open

- **A float with no code prints the shortest decimal string that round-trips.**
  `f"{0.1 + 0.2}"` is `0.30000000000000004`. This is Python 3.1+, Rust and Go,
  and it is right for the same reason §5.1 of the core spec makes reductions
  ordered: *this is a language whose users publish*. A default that silently
  rounds to six places hides exactly the bug the user needed to see. The user who
  wants three places asks for three places.
- **`NaN`, `inf`, `-inf`**, spelled that way, honouring width and alignment and
  ignoring precision. Not `nan`; not `NaN%`.
- **`-0.0` prints as `-0.0`.** It is a different float and hiding that has cost
  people days.
- **Default alignment is left for strings and anything `Display`, right for
  numbers.** Python's rule, and it is the rule that makes a column of numbers a
  column.
- **Default fill is a space**; `0` before the width means zero-fill and sets
  alignment to right (`f"{i:04d}"` is `0007`).
- **Bool prints `true` / `false`**, matching §4.2's literals.

### 2.4 Decision: the spec is checked at compile time

**Decision.** The format spec is parsed at compile time and checked against the
static type of its argument. A mismatch is a compile error, not a runtime one.

This is the single most defensible decision in the note, and the argument is one
sentence: the literal is known at compile time, the type is known at compile
time, and §1 of the core spec says the language exists to catch what a compiler
can catch before the run starts. `ValueError: Unknown format code 'f' for object
of type 'str'` — raised eight hours into a training run, on the logging line —
is a Python failure mode that this language can simply not have.

The rules:

| Spec element | Legal when |
|---|---|
| `f e E g G %` | the type is `F16 BF16 F32 F64`, or implements `DisplayNumber` |
| `d x X o b` | the type is one of `I8…I64 U8…U64` |
| `.precision` | the code is a float code, or the type is `String` (truncation) |
| `s`, fill, align, width | the type implements `Display` |
| `!i` | the type implements `Inspect` |
| `#` | the code is `x X o b f e g` |
| `,` `_` | the code is numeric |

Everything else is `SC0274`, and each case has a specific fix rather than a
generic complaint:

```
error[SC0274]: `.3f` needs a float, and `count` is an I64
  --> report.science:14:24
   |
14 |     print(f"count {count:.3f}")
   |                    ^^^^^ ----  this spec asks for 3 decimal places
   |                    |
   |                    this is an I64
   |
help: convert, if a decimal point is what you want
   |
14 |     print(f"count {count as F64:.3f}")
   |                         ++++++++
help: or drop the precision
   |
14 |     print(f"count {count}")
   |                         --------
```

**What it costs.**

1. **The format spec is no longer a value.** You cannot compute a spec string at
   runtime and apply it. There is no `format(fmt, x)` taking a `String`. For the
   audience this matters in exactly one place — a table writer that chooses
   precision per column — and §2.5 is the hole left open for it.
2. **A typo in a spec is a hard error in an interactive tier.** In F1's notebook
   a bad spec stops the cell instead of printing something wrong. That is the
   intended trade and it should be said out loud rather than discovered.
3. **The type checker acquires a small parser.** The spec grammar has to live
   somewhere and it has to produce spans into the middle of a string literal.
   This is the same span machinery §1.5 already requires.
4. **`DisplayNumber` exists because of this rule.** Without a way for a user type
   to declare that numeric codes are meaningful for it, `f"{q:.2f}"` on a
   `Quantity` would be an error and the units work would print nothing useful.
   §3.3.

### 2.5 The one dynamic hole

**Decision.** Width and precision may themselves be interpolations of an `Int`
expression. The code stays static.

```science
let digits be options.precision
for row in table:
    print(f"{row.label:<12}{row.value:>10.{digits}f}")
```

This is Python's nested-replacement-field syntax and it preserves the whole of
§2.4's check, because the part that can be wrong — *which rendering* — is still
a literal. The part that becomes dynamic is a number, and a number cannot be the
wrong kind of thing.

Dynamic *codes* stay impossible. A program that must choose between `f` and `e`
at runtime writes the `if`. If that proves too little, the escape hatch is a
`Formatter` method taking the pieces as values (`into.number(x, precision: p)`)
and it should be designed when a real caller needs it, not now.

### 2.6 What is deliberately not in the mini-language

- **`n`, the locale-aware code.** Rejected outright. Locale-dependent output means
  the same program prints `3.14` on one machine and `3,14` on another, which
  breaks every downstream parser and every reproducibility claim the language
  makes. Localisation belongs to a library that is explicit about which locale,
  never to a default read from the environment.
- **Positional and implicit fields** — `{0}`, `{}`. There is no argument list to
  be positional in, because §4.1 makes `print` unary. Empty braces are `SC0174`
  with the fix "name the value".
- **`!r` and `!a`.** Python's repr/ascii conversions. `!i` covers the first and
  the second is a Python-2 artefact.
- **Attribute and index access in the *spec*.** The expression side already has
  them; the spec side never needed them.
- **A `Formatter` the user can pass around as a value in F0.** It exists as a
  parameter to `Display` (§3.1) and nothing else; a general sink abstraction is
  `data-io.md` §8's territory.

---

## 3. `Display` and its friends

### 3.1 `Display`, with a `Formatter`

§5.4 lists `Display` among the operator traits and says it is what `print`
requires. That is not enough to implement: it does not say what the method is,
and it gives an implementation no way to see the spec, so `f"{q:.2f}"` on a user
type has nowhere to read `.2` from.

**Decision.** `Display` has one method, taking the sink and the parsed spec
together:

```science
interface Display:
    def display(self, into: mutable borrowed Formatter)
```

```science
type Station:
    id: String
    channels: Int

Station implements Display:
    def display(self, into: mutable borrowed Formatter):
        into.text(self.id)
```

`Formatter` is a new library type with a small closed method set, in the spirit
of §8:

```science
Formatter has:
    ## Write text, honouring fill, alignment and width from the spec.
    def text(mutable self, value: borrowed String)

    ## Write text with no padding, for a fragment of a larger rendering.
    def raw(mutable self, value: borrowed String)

    ## Render a float under the spec's code, precision, sign and grouping.
    def number(mutable self, value: F64)

    ## Render an integer under the spec's code, sign and grouping.
    def integer(mutable self, value: I64)

    ## The parsed spec, for an implementation that needs to branch on it.
    def spec(self) -> FormatSpec
```

`FormatSpec` is a plain record of nullable fields — `fill: Char`, `align: Align?`,
`sign: Sign?`, `width: Int?`, `precision: Int?`, `code: Code?`, `alternate: Bool`,
`grouping: Grouping?` — with `Align`, `Sign`, `Code` and `Grouping` as `choice`
types. Nothing here is exotic; it is listed so that the closed surface is
actually closed.

**Serves:** scientists, because it is the mechanism that makes `{q:.2f}` work on
a `Quantity` at all. Programmers pay one parameter they usually ignore.

### 3.2 `Inspect`, and why not `{x:?}`

**Decision.** Science takes Rust's split. `Display` is for the user, `Inspect` is
for the programmer, and they are different interfaces.

```science
interface Inspect:
    def inspect(self, into: mutable borrowed Formatter)
```

**Why the split, and not one interface.** The argument against a split is real:
it is one more thing to implement and Rust users do grumble. The argument for it
is that a single interface cannot serve both, and the demonstration is one line:

```science
print(f"{name}")      # A12
print(f"{name!i}")    # "A12"
```

A string rendered for a user has no quotes; a string rendered inside a structure
being inspected must have them, or `["a, b"]` and `["a", "b"]` print identically
and a debugging session goes sideways. The same applies to a nullable — `null`
must be visible — and to a float, where `Inspect` should never abbreviate. One
interface forces every type to pick a side, and a language with an F1 notebook
tier that shows values automatically needs both sides.

**Why `!i` and not `{x:?}`.** This is the one place Science must not copy Rust.
`?` is the presence operator (`syntax-revision-2.md` §3.1), so both of these are
legal and they mean opposite things:

```science
print(f"{err?}")     # "true" — err is present
print(f"{err:?}")    # the error, inspected
```

One colon apart, one printing a `Bool` and the other printing the value. That is
a trap, it would be sprung by every Rust user on their first day, and the fix is
to put the request somewhere `?` is not: a conversion flag before the colon.
`!i` also leaves `{x!i:>20}` well-formed, which `{x:?>20}` never was in Rust
either.

**`Inspect` needs derivation or it will not be implemented.** Hand-writing
`inspect` for every record is work nobody does. The mechanism is the same one
`data-io.md` §11.1 asks for with `Record`, and §12 of the core spec reserves
macros without implementing them. This note does not solve that boundary; it
records that `Inspect` is a second customer for whatever solves it, which should
make solving it easier to justify.

**`Inspect` output is specified, not incidental.** §10.3's UI tests compare
rendered diagnostics byte for byte, and diagnostics print values. So the rendering
of `Inspect` for every library type is part of the spec and changing it is a spec
change, exactly like §8's method sets.

### 3.3 `DisplayNumber`, and where `9.8 m/s²` comes from

**Decision.** A marker interface, with no methods, that grants a type the numeric
format codes of §2.4.

```science
Quantity implements DisplayNumber
```

One line, using §4.4's marker-implementation form, the same shape as
`Point implements Copy`. It says: *numeric codes are meaningful for this type,
and its `Display` honours them.*

That is the whole language-level answer to the units question, and the rest is a
library:

```science
Quantity implements Display:
    def display(self, into: mutable borrowed Formatter):
        into.number(self.value as F64)
        into.raw(" ")
        into.raw(Self.unit_symbol())
```

`unit_symbol` is an associated function over the seven const exponents of
`scientific-libraries.md` §12.3 — `(1, 0, -2, 0, 0, 0, 0)` renders `m/s²` — and
it is ordinary Science over const generic parameters. **No language feature is
required for units to print.** The feature units need is const-expression
arithmetic for *multiplication* (§12.4 of that note), and that is F1's problem
and unaffected by anything here.

So:

```science
let g be 9.80665<m/s^2>                       # unit-literals.md §2.1
print(f"{g:.2f}")           # 9.81 m/s²
print(f"{g:.3g}")           # 9.81 m/s²
print(f"{g}")               # 9.80665 m/s²
print(f"{g!i}")             # 9.80665<m/s^2>
```

**The seam with `unit-literals.md`, named.** That note writes the unit in source
as `m/s^2`, ASCII, because §2.2 of it will not put a superscript in a file people
have to type. This note prints `m/s²`, because a table in a paper wants the
superscript. Both are right and they are different spellings of one dimension, so
one of them has to be chosen per rendering:

> **`Display` prints the typeset spelling (`m/s²`); `Inspect` prints the source
> spelling (`9.80665<m/s^2>`), which pastes back into a program unchanged.**

That is the same division `Inspect` already makes for strings in §3.2 — the user
view has no quotes, the programmer view round-trips — and it is the reason this
note does not need a third interface to carry the ASCII form. It does require
`physics` to carry two symbol tables, or one table and a transliteration, and
that is a library cost of a few hundred bytes.

**The third rendering §12 might have wanted, and why it is not a third
interface.** The brief for a scientific language suggests a rendering with
significant figures and units, distinct from both `Display` and `Inspect`. It is
not needed as an interface, because significant figures are `.3g` and units come
from the type. A third interface would have to be selected by something, and the
only honest selector is the spec — which is already the selector. Adding
`Scientific` would give two ways to ask for one rendering, which is §1 of
`syntax-revision-2.md`'s rejected pattern.

**Uncertainty is a different question and is out of scope.** `9.81 ± 0.02 m/s²`
is a value with an uncertainty, not a formatting of a value. It belongs in
`Quantity`'s design if anywhere, and this note does not decide it.

**The limit of the check, stated.** `DisplayNumber` says the numeric codes are
*meaningful*; it does not prove the implementation honours them. A user type can
implement `DisplayNumber`, ignore `into.spec()`, and print the same thing for
`.2f` and `.9e`. The compiler cannot catch that, and the alternative — an
associated constant enumerating exactly which codes a type accepts, checked at
every call site — costs more machinery than the bug is worth. The check is
type-level, and that is where it stops.

### 3.4 What does not implement `Display`

Decided here, because each is a case where the obvious convenience is wrong:

- **`T?` does not implement `Display`.** It implements `Inspect`. Printing a
  possibly-null value must be a decision: after narrowing (`if x?:`) the value is
  `T` and prints; otherwise the user writes the default or asks for `{x!i}` and
  gets `null`. A silent `null` or an empty cell in a published table is the
  failure this prevents. **Serves scientists**, at a small cost in typing.
- **Arrays and maps do not implement `Display`.** They implement `Inspect`.
  Printing a million-element array by accident is a mistake the language should
  not make easy; `{xs!i}` is explicit and truncates at a documented length.
- **Closures and function values implement neither.**
- **`()` implements `Inspect` only.**

---

## 4. `print` and `write`

### 4.1 Decision: `print` is unary. This settles `data-io.md` §11.5

`data-io.md` §11.5 says, in full:

> **Formatting** — the example's `println("kept rows:", frame.len())` assumes a
> variadic `println` over `Display`. If formatting takes a different shape, the
> example changes and nothing else does.

Formatting takes a different shape. **The example changes.**

**Decision.**

```science
def print(value: borrowed any Display)
def write(value: borrowed any Display)
```

One argument. Not variadic. `data-io.md` §10's two output lines become:

```science
print(f"kept rows: {frame.len()}")
print(f"rejected rows: {frame.rejected_count()}")
```

Three reasons, in increasing order of weight:

1. **Variadic `print` and interpolation are two spellings of one thing.**
   `print("n:", n)` and `print(f"n: {n}")` produce the same line. §1 of
   `syntax-revision-2.md` spent its best argument removing exactly this kind of
   duplication from comparisons, and re-introducing it in the language's most
   frequently written call would be inconsistent on the day the ink dried.
2. **The separator is a hidden rule.** Variadic `print` joins with a space, which
   means `print("rows:", n)` prints `rows: 4` and `print("rows: ", n)` prints
   `rows:  4`, and users spend a surprising amount of time on that. Interpolation
   has no hidden rule: the string is the output.
3. **Science has no variadic functions, and should not acquire them for one
   call.** §4.4 gives no variadic parameter form. Adding one means a syntax, a
   type for a heterogeneous argument pack, and a monomorphization strategy for
   it — a genuine language feature, for a convenience that interpolation already
   provides. This is the decisive reason.

**The cost, stated.** `print(a, b, c)` is muscle memory for every Python user and
they will type it. The mitigation is a diagnostic, not a special case:

```
error[SC0275]: `print` takes one value
  --> run.science:9:5
   |
 9 |     print("rows:", n)
   |     ^^^^^^^^^^^^^^^^^
   |
help: interpolate instead
   |
 9 |     print(f"rows: {n}")
```

That fix is mechanical, so the compiler can apply it, and a user meets it once.

**No zero-argument `print`.** There is no overloading and there are no default
arguments in §4.4, so a blank line is `print("")`. Two characters, and no
language feature.

**`panic` follows.** `panic(value: borrowed any Display)`, so
`panic(f"expected {expected} channels, found {found}")` is how a message is
built. This is what `python-interop.md` §7.1's worked traceback already shows —
`spectra.WrongChannelCount: expected 768 channels, found 512` is a `Display`
rendering of a choice variant — so that note's §7 needs no change, only the
knowledge that the message on the Python side is whatever `Display` produced.

**`any Display`, not a generic.** Dynamic dispatch, so `print` is one function in
the binary rather than one per type. Output is not the hot path, the trait object
costs a pointer, and §6.3's auto-borrow means the call site is still `print(doc)`.

### 4.2 Streams and buffering

Decided here because "why does my progress bar not appear" is a week-one
complaint and the answer is a buffering policy nobody wrote down.

| Function | Stream | Newline |
|---|---|---|
| `print` | stdout | yes |
| `write` | stdout | no |
| `print_error` | stderr | yes |
| `write_error` | stderr | no |
| `panic` | stderr | yes, then abort |
| compiler diagnostics | stderr | — |

- **stdout is line-buffered when it is a terminal and block-buffered (64 KiB)
  otherwise.** This is C's policy and every tool in a pipeline assumes it.
- **stderr is unbuffered.** A message that precedes a crash must survive the
  crash.
- **`flush()` is a free function**, because `write` of a progress line with no
  newline is line-buffered into invisibility otherwise. This is the eighth free
  function and it is added reluctantly; the alternative, having `write` flush
  implicitly, makes a loop of `write` calls syscall-bound.
- **Output ordering between the two streams is not guaranteed** when one is
  redirected and the other is not. Saying so is better than a user discovering it
  in a log file.

**What goes to which, as a rule:** results to stdout, progress and warnings to
stderr. A program whose stdout is a CSV must be able to show progress without
corrupting it, and that is the only reason `print_error` exists.

### 4.3 §8's closed list after this note

`syntax-revision-2.md` §3.4 left it as `print`, `write`, `panic`, `read_file`,
`write_file`. It becomes:

> `print`, `write`, `print_error`, `write_error`, `flush`, `panic`, `read_file`,
> `write_file`

and §8's type list gains `Formatter` and `FormatSpec`, while §5.4's interface list
gains `Inspect` and `DisplayNumber` and respecifies `Display`. Eight free
functions is more than five and each one is a name the user must learn; the three
added are the minimum that makes stderr and progress output expressible at all.

---

## 5. Documentation comments

### 5.1 Decision: `##` before the declaration

**Decision.** A documentation comment is a run of `##` lines immediately
preceding a declaration. There is no docstring form and no `"""` literal.

```science
## Standardize each row to zero mean and unit deviation, in place.
##
## Rows whose deviation is zero are left unchanged, because dividing
## by it is the wrong answer rather than an error.
def standardize(rows: mutable borrowed Frame of Row):
    for row in rows:
        row.center()
```

The two candidates were seriously weighed and the Python one loses on a specific,
checkable point.

**Why not `"""…"""` as the first expression of a body.**

1. **Most of the things that need documentation have no body.** A record field, a
   `choice` variant, an interface method with no default, a `const`, a type
   alias, a module — none of them has a block to put a string in. Python has this
   problem too and its answer is a convention (`#:` in Sphinx, a bare string
   after the attribute) that tooling half-supports. Science would be choosing a
   form that covers functions and then needing a second form for everything else,
   which is two forms.
2. **It requires adding a triple-quoted string literal to the language**, which
   §4.2 does not have. That literal then needs rules for indentation stripping,
   for interaction with `f` and `r` prefixes, and for the escape list — and once
   it exists, people will use it for multi-line data strings, and the indentation
   rule will be argued about forever. That is a large lexical surface bought for
   one feature.
3. **A docstring in the body is a value in Python and would be a lie here.** In
   Python `f.__doc__` is the first expression of the body because it *is* an
   expression that was evaluated. In a compiled language it would be an
   expression statement with no effect that the compiler silently treats as
   metadata — a special case in the evaluator of exactly the kind §4.6 of the
   core spec refused when it rejected parenthesis-free calls: *a language cannot
   resolve an ambiguity by guessing.*

**What `##` costs.** Python users type `"""`. That is the whole cost, and §5.3
shows they lose nothing functional: `help()` and shift-tab work regardless of
which side of the declaration the text is written on.

`##` also composes correctly with `#`: `# TODO` next to a doc comment is an
ordinary comment and does not become documentation, which is a distinction
Rust's `//` / `///` split gets right and which a docstring form cannot make at
all.

**Serves programmers**, primarily, and tooling. Scientists are served by §5.3 and
§5.5.

### 5.2 What a doc comment attaches to

| Target | Example |
|---|---|
| Module | a `##` run at the top of the file, before any declaration |
| Function, associated function, method | before `def` |
| `type`, `choice`, `interface`, type alias, `const` | before the keyword |
| Field of a `type` | before the field line |
| Variant of a `choice` | before the variant line |
| Method inside an `interface` or a `has` block | before `def` |
| An `implements` block | before the block; individual methods too |

```science
## Spectral preprocessing.
##
## Everything in this module operates on 768-channel rows and assumes
## the detector's dark current has already been subtracted.

use data (Frame)

## A single detector reading.
type Row:
    ## Instrument identifier, as written in the run log.
    station: String
    ## Channel intensities. Always 768 long; this is checked on load.
    channels: Array of F32

## What went wrong while reading a run.
choice LoadError:
    ## The file had a channel count other than 768.
    WrongChannelCount(Int)
    ## The file was unreadable or absent.
    Unreadable(String)
```

**A module doc is a `##` run before the first declaration.** There is no second
sigil for it — Rust's `//!` / `///` split exists because Rust allows module
bodies inline, and Science's modules are files (§4.4), so position is
unambiguous. One sigil.

**A `##` run attached to nothing** — at the end of a file, or separated from the
next declaration by a blank line — is `SC0176`, with the fix "make it `#`". This
matters because the silent failure mode of every doc-comment system is
documentation that was written and never appears anywhere.

### 5.3 What it compiles to, and how `help()` still works

Nothing is allocated and nothing is on the runtime path.

- The lexer keeps `##` runs as trivia attached to the following token; the parser
  attaches them to the AST node; they survive into HIR. **This much must happen
  in F0.** It is cheap now and it is the part that cannot be retrofitted, because
  once the lexer discards them every tool downstream is built assuming they are
  gone.
- `sciencec` emits them into a metadata section of the object file: a string table
  plus an index from fully-qualified name to offset. Static data, not reachable
  from any code path, strippable.
- **F1's notebook tier reads that table.** `help(f)` and shift-tab show the doc
  because the tooling looks it up by name, not because the string is a runtime
  value. Python's `__doc__` is a runtime attribute out of an implementation
  accident; nothing about the *user experience* depends on that accident.
- **`python-interop.md` §3.3's generated module populates `__doc__`** on each
  exported function, class and method from the same table, and `spectra.pyi`
  (§8.3 of that note) carries them as docstrings so that editors and `mypy` see
  them. A Python user calling `help(spectra.standardize)` gets the Science doc
  comment, formatted as Python expects. This is the concrete answer to the
  objection that the Python shape is required for the Python audience: it is not.

The consumers are F1. F0's obligation is to lex, attach, and preserve.

### 5.4 Content

**Decision.** Markdown. The first line is the summary. No structured parameter
tags.

- **The first line, up to the first blank `##`, is the summary**, and it is what
  a completion list and a one-line `help` show. Everything after is the body.
  This is the only structural rule and it exists because tooling needs a short
  form and generating one by truncation produces garbage.
- **Markdown**, because it is what every doc tool in the audience's world already
  renders, including the notebook.
- **No `@param` / `@returns` tags.** Rejected deliberately: the parameter names
  and types are in the signature, which the compiler knows exactly, and a tag
  that restates them is a second copy that goes stale. A doc tool renders the
  signature from the AST and the prose from the comment. Where a parameter needs
  explanation it gets a sentence in the prose or a Markdown list, and nothing
  checks that the list is complete — which is honest, because a checker that
  demanded a tag per parameter would produce `## @param x the x`.

### 5.5 Doc tests: decision, and what they cost

**Decision.** Code samples inside doc comments are compiled and run. Specified
now, implemented in F1 with the doc tool. F0's obligation is §5.3's preservation
and nothing more.

````science
## The pooled standard deviation of two samples.
##
## Returns zero when both samples have fewer than two members, rather
## than failing: a single measurement has no spread, and that is not
## an error.
##
## ```science
## let s be pooled_deviation(2.5, 8, 3.1, 12)
## print(f"{s:.4f}")
## ```
## ```output
## 2.8734
## ```
def pooled_deviation(s1: F64, n1: Int, s2: F64, n2: Int) -> F64:
    ...
````

Three fence markers and no more: `science` (compile and run), `science,no_run`
(compile only — for a sample that needs a file or a GPU), `science,fails` (must
fail to compile, and the doc tool checks the diagnostic code). A neighbouring
` ```output ` fence is compared to stdout byte for byte, which makes a doc
comment a test of the *formatting* too — which is the point, in a note about
formatting.

**The price, stated plainly.**

- **It is a fifth test layer** beside §10's four, with its own runner, its own
  reporting, and its own place in CI.
- **Each sample is a compilation unit.** It needs the enclosing module in scope
  and a synthesised `main`, and its diagnostics need spans that point into a
  comment inside another file — the same span problem as §1.5, arriving from a
  different direction.
- **It is a real cost on every library build.** The standard library will have
  hundreds of samples and each is a compile. It must be a separate target, not
  part of `sciencec build`.
- **Samples drift toward triviality** if the runner is slow, because authors
  write what runs fast. That is a documentation-culture problem no mechanism
  fixes.

**The win, which for this project is larger than usual.** Science has no training
corpus. `llm-ergonomics.md` is a whole note about writing a language that a model
can generate correctly, and the material a model will learn this language from is
its documentation. Doc tests make every published example **compile-verified
against the current compiler**, which means the corpus cannot contain a single
example that does not work. No language with an existing corpus gets this benefit
from doc tests; Science does, and it is the strongest argument for paying the
price.

One requirement that makes it stick: **the standard library's own documentation
is doc-tested from the day the feature lands.** A doc-test facility nobody uses
is worse than none, because it implies a guarantee it does not provide.

---

## 6. Unicode: identifiers yes, operators no

### 6.1 What is already free, and it is worth saying out loud

§4.2 says *"Identifiers admit Unicode letters."* That one clause means the
following already compiles:

```science
def drift(σ: F64, Δt: F64, θ: F64) -> F64:
    σ * Δt.square_root() * θ.cos()

let μ be mean(readings)
let χ2 be chi_squared(observed, expected)
```

A physicist writes `σ` in the paper and `σ` in the code, and the code and the
paper say the same thing. For the target audience this is a genuine win, it costs
the compiler an identifier predicate over Unicode categories, and it is already
decided. It deserves to be stated as a feature somewhere users will read, because
nobody will guess it from a clause in §4.2.

One boundary worth documenting where users will hit it: `χ2`, not `χ²`. A
superscript two is Unicode category *No*, a number, not a letter, so `χ²` is not
an identifier under §4.2's rule — and neither are `∇`, `∂` or `ℏ`, which are
symbols. The rule is "letters", it is the right rule, and the disappointment
should be met with a diagnostic that says which character was rejected and why
rather than a bare `SC0001`.

### 6.2 The cost that is not zero, and the one rule needed

Unicode identifiers are cheap but not free, and the spec does not currently say
these:

**Decision.** Identifiers are normalised to NFC at lexing time.

Without it, `é` typed as U+00E9 and `é` typed as `e` + U+0301 are two different
identifiers that render identically, and the resulting "undefined name" error
points at a name that is visibly on the previous line. Unicode's own
recommendation (UAX #31) is NFC; Rust, Swift and Python all do it. One call in
the lexer.

**Decision.** A single identifier mixing scripts is a warning (`SC0015`).

`Α` (Greek capital alpha) and `А` (Cyrillic capital a) and `A` are three
characters that look identical, and a file containing two of them has a bug that
no amount of staring finds. The warning fires on *mixing within one identifier*,
not on using a script — `σ` alone, `温度` alone and `σ_max` are all fine — which
is the rule that catches homoglyph accidents without insulting anyone's language.

### 6.3 Decision: no Unicode operators

**Decision.** `≤ ≥ ≠ ∑ ∏ √ × · ÷ ∈ ∘ ⊗` and their relatives are not operators and
are not accepted as aliases for anything. `<=` is the only spelling of `<=`.

The arguments, and the first one is not the one I would have guessed:

1. **It is a second spelling, and that is the pattern the language just spent a
   revision removing.** §1 of `syntax-revision-2.md` deleted the comparison
   phrases for exactly this. In fairness, the case is weaker here than it was
   there: `≤` → `<=` *is* mechanically canonicalisable by a formatter, because it
   is purely lexical and needs no type information — which was precisely why
   `is` / `==` could not be canonicalised. So the "permanent drift" argument does
   not transfer, and pretending it does would be dishonest. It still leaves two
   spellings in every diff, every search, and every model's output distribution.
2. **You cannot type it.** No standard keyboard produces `≤`. Julia's users can
   because Julia's REPL and editor plugins implement `\leq<tab>`, which is real
   infrastructure. §12 of the core spec puts the language server and the
   formatter explicitly out of scope for F0. Admitting a character the language
   provides no way to enter means a language you can read and cannot write, which
   is worse than not having it.
3. **Homoglyphs, again, and worse.** U+2212 MINUS SIGN is not `-`. U+2215
   DIVISION SLASH is not `/`. `×` is not `x` and `·` is not `.`. Every one of
   those arrives by paste from a PDF, and the resulting error is at best
   confusing and at worst — if `×` were an operator — a program that means
   something other than it looks like.
4. **`∑` is not an operator at all.** It is a binder with a bound variable, an
   index range and a body: `∑_{i=1}^{n} x_i`. Accepting it as an operator token
   would be accepting the *symbol* without the *notation*, which gets you
   `∑(xs)` — a function call wearing a costume, where `xs.sum()` already exists
   and reads better in a chain. And admitting `≤` creates the demand for `∑`
   within a week, because the argument that admits one admits all.

### 6.4 The fair version of the other side, and the condition to revisit

Julia does this and Julia's users genuinely love it. The case is not silly:

- A numerical method transcribed from a paper reads like the paper. `α ≤ β` is
  what the reference says, and a transcription error in a numerical kernel is
  expensive and hard to see.
- Julia's experience is that the feature is *used*, not merely available, which
  is the strongest evidence any language feature can have.
- It is a genuine differentiator for exactly this audience, and this language is
  chasing exactly this audience.

What makes it work for Julia is the input method, and what makes it survivable is
that Julia has no compile-time verification story to protect — a homoglyph bug in
Julia is one more runtime error among many, whereas in Science it is a hole in
the premise.

**The condition to revisit.** If F1's notebook tier ships a LaTeX-tab input
method — and it probably should, since a notebook is where a scientist types
`\sigma` anyway — the question reopens. Even then the recommendation is **editor-
side rendering**, not grammar: the file contains `<=`, the editor displays `≤`,
the diff is stable and the model's output distribution is unchanged. That gets
the whole benefit and none of the cost, and it is the answer this note would
reach for first if the demand materialises.

---

## 7. Diagnostics

Codes are proposals in the ranges §9 of the core spec defines; the registry is the
spec's to assign. The syntax range currently uses `SC0100`–`SC0104`, `SC0109`,
`SC0115`–`SC0116`, `SC0120`–`SC0134`, `SC0140` and `SC0142`; `SC0170`–`SC0179` is
free and is taken as a block.

| Code | Phase | Condition | Applicable fix |
|---|---|---|---|
| `SC0170` | Syntax | Unterminated interpolation: `{` in an `f"…"` with no matching `}` before the literal ends. The primary span is the `{`; a secondary span marks where the literal ended. | Insert `}` before the closing quote, or write `{{` for a literal brace |
| `SC0171` | Syntax | A `}` in an `f"…"` with no opener. | Write `}}` |
| `SC0172` | Syntax (**warning**) | A plain `"…"` containing a well-formed interpolation whose every name resolves in scope. §1.2. | Insert the `f` prefix; or double the braces |
| `SC0173` | Syntax | Malformed format spec, or an unknown type code. Message names the offending character and lists the legal codes. | Nearest legal code, when the edit distance is one (`F` → `f`, `%%` → `%`) |
| `SC0174` | Syntax | Empty interpolation, `f"{}"` or `f"{:.3f}"`. There are no positional arguments (§2.6). | Name the value |
| `SC0175` | Syntax | A statement, a `let`, a newline or a `#` comment inside an interpolation. §1.4. | Bind it before the string and interpolate the name |
| `SC0176` | Syntax (**warning**) | A `##` run attached to no declaration — at end of file, or separated from the next declaration by a blank line. | Change `##` to `#`, or delete the blank line |
| `SC0177` | Syntax | `rf"…"` or any other prefix combination. §1.3. | Choose one prefix |
| `SC0015` | Lexical (**warning**) | An identifier not in NFC, or mixing scripts within one identifier. §6.2. | Normalise (NFC form supplied in the fix) |
| `SC0274` | Types | The format spec does not match the argument's type: a float code on an integer, an integer code on a float, a precision on a non-float non-`String`, a numeric code on a type that does not implement `DisplayNumber`. | Per case: `x as F64`; drop the precision; `implements DisplayNumber` |
| `SC0275` | Types | The argument does not implement the requested interface — `Display` for a plain hole, `Inspect` for `!i`. Covers `print` given more than one argument, and a `T?` interpolated without narrowing. | `{x!i}`; narrow with `if x?:`; interpolate instead of passing two arguments |

**Two of these are in the types range and not the syntax range, deliberately.**
`SC0274` and `SC0275` cannot be decided before type checking — the spec is
syntactically well-formed and the question is whether the *argument* fits it.
Filing them under syntax would put a diagnostic in a range the phase that emits
it does not own, which is the mistake §9 of the core spec explicitly corrected
for exhaustiveness errors. The rest of the family is genuinely lexical or
syntactic and takes the `SC0170` block.

---

## 8. What this note asks for

1. **§4.2: two literal prefixes.** `f"…"` with `{{`/`}}`, and `r"…"` with no
   escape processing. Prefixes do not combine. `f` and `r` do **not** join §13 —
   they are not identifiers and cannot be confused with one (§1.3).
2. **§4.2: NFC normalisation of identifiers**, and a mixed-script warning
   (`SC0015`).
3. **The lexer: a mode stack** (§1.5), and suspension of the indentation machine
   inside an interpolation, as §4.2 already does inside brackets. This is the
   largest implementation item in the note.
4. **§5.4: respecify `Display`** as
   `def display(self, into: mutable borrowed Formatter)`, and **add `Inspect`
   and `DisplayNumber`** to the interface list. `DisplayNumber` is a marker with
   no methods.
5. **§8: add `Formatter` and `FormatSpec`** to the library types, and make the
   free-function list `print`, `write`, `print_error`, `write_error`, `flush`,
   `panic`, `read_file`, `write_file` (§4.3).
6. **§8/§4.4: `print` is unary** and Science acquires no variadic parameter form.
   **This settles `data-io.md` §11.5**; that note's §10 example changes its two
   `println` lines to interpolated `print` lines and nothing else in it moves.
   `script-mode.md` §1.3 also writes `print("rows:", frame.len())` and needs the
   same one-line edit. Both are mechanical and neither note's substance changes.
7. **The type checker: a format-spec parser and checker** (§2.4), producing spans
   inside string literals.
8. **F0 must preserve `##` runs** through lexer, parser and HIR (§5.3), and emit
   the metadata section. The doc tool, `help()` and doc tests are F1; the
   preservation is not, because it cannot be retrofitted.
9. **`python-interop.md` §3.3 and §8.3**: populate `__doc__` and the `.pyi`
   docstrings from the doc table. No change to that note's §7 — a formatted
   message is a `Display` rendering and crosses the membrane as the exception's
   `str()`, exactly as its worked examples already show.
10. **The derive question, again.** `Inspect` needs derivation for the same reason
    `data-io.md` §11.1 says `Record` does, against the same `§12 reserves macros`
    boundary. Two customers for one mechanism; whoever draws that boundary should
    know there are now two.
11. **`scientific-libraries.md` §12**: no change requested. `Quantity implements
    DisplayNumber` plus a `Display` implementation over the const exponents is all
    that is needed for `9.8 m/s²`, and it is library code (§3.3).

---

## 9. Risks

**The lexer mode stack lands on a lexer migrated twice today.**
`syntax-revision-2.md` §10 already notes that the corpus, the 117 lexer tests and
the 157 parser tests move once more for that revision. This note adds a third
literal kind and a mode stack to the same lexer in the same week. The mitigation
is ordering: land §1.3's literal *kinds* (which are a prefix character and a flag)
before §1.4's expression interiors, because the first is a day and the second is
not, and `f"{name}"` with bare names covers most of the corpus while the machine
is built.

**Two new lexer features are landing at once, and they meet inside a brace.**
`unit-literals.md` §2.3 adds a scan with a backtrack for `9.8<m/s^2>`; this note
adds a mode stack. They compose in `f"{9.8<m/s^2>:.2f}"`, where the unit
literal's `>` sits inside an expression frame whose terminator is `}` and whose
spec separator is `:`. Neither feature is wrong and the combination is
implementable, but it is the kind of interaction that is found by a user rather
than by a designer unless the two are tested together deliberately. Whichever
lands second owns the joint test.

**`SC0172` will have false positives.** A LaTeX-heavy file with short bindings —
`a`, `b`, `n`, `x` are all plausible — will see the warning on strings that are
correct. The suppressions are `r"…"` and explicit `{{`/`}}`, and neither works for
a string that needs escape processing *and* contains literal braces *and* has
colliding names. Science has no attribute syntax, so there is no
`#[allow(...)]` to reach for. If the noise proves real, the lever is to narrow
the lint — require two or more resolving holes, or restrict it to arguments of
`print`/`write`/`panic` — and both narrowings weaken the protection that made
§1.2's decision affordable. This is the weakest joint in the note.

**Compile-time spec checking makes the formatting mini-language part of the type
system.** Every future extension — a new type code, a date format, a tensor
summary spec — is now a spec change with a diagnostic and UI tests, not a library
addition. That is the intended discipline and it is also a tax on every future
idea in this area.

**There is no runtime formatting at all.** §2.5's nested width and precision is
the whole of the escape hatch. A program that genuinely needs a runtime-chosen
type code — a generic table writer over columns of mixed type is the plausible
case — cannot be written, and will be written with an `if` over three branches or
will be written in Python. If that case turns up in real code it is an argument
for the `Formatter`-as-value API, and taking it would partly undo §2.4.

**`Inspect` without derivation is a dead letter.** If the macro boundary is not
resolved, every user type needs a hand-written `inspect`, nobody will write one,
and `{x!i}` will work on library types and fail on the user's own — which is the
worst possible distribution, because it is the user's own types they are
debugging.

**Doc tests are a fifth test layer that can rot quietly.** They run in a separate
target, so a red doc-test suite does not fail `sciencec build`, so it will be
ignored unless CI treats it as mandatory from the first commit. A doc-test
facility that is 80% green is worse than none, because the guarantee it implies —
*every published example compiles* — is the entire reason for having it.

**Every `Inspect` rendering is now part of the spec**, because §10.3's UI tests
compare rendered diagnostics byte for byte and diagnostics print values. Changing
how a `Map` inspects becomes a spec change with snapshot churn. This is correct
and it is a maintenance cost that should be understood before the first
implementation, not after the first hundred snapshots.

---

## 10. Open questions

- **Multi-line strings.** This note deliberately does not add them; `r"…"` covers
  LaTeX and regex, which were the pressing cases. A SQL query or an embedded
  shader will want them, and when they arrive the interaction with `f` and `r`
  and with indentation stripping has to be decided, which is the surface §5.1
  declined to buy for doc comments.
- **`r#"…"#`.** Deferred until a raw string needs to contain a quote. Adding it
  later is source-compatible.
- **Localisation.** `n` is rejected (§2.6) and nothing replaces it. A program that
  must print `3,14` for a Spanish-language report has no facility. The right shape
  is an explicit `Locale` value passed to a formatting call, which is a library
  design this note does not attempt.
- **Uncertainty rendering.** `9.81 ± 0.02 m/s²` is wanted by the audience and is
  a property of the value, not the format. It belongs with `Quantity`.
- **Tensor and `Frame` rendering.** F1 will want `{t!i}` on a tensor to produce a
  NumPy-like summary with edge elements and an ellipsis, and `Frame` to produce a
  pandas-like table. Both are `Inspect` implementations and both need a
  truncation policy that this note does not set. The notebook tier will want a
  MIME-variant rendering beside them, which §5.4 already anticipates in one
  sentence and nobody has designed.
- **A small allow-list of mathematical symbols as identifier characters.** `ℏ`,
  `∂`, `∇` and the superscript digits are symbols and numbers, not letters, so
  §4.2 excludes them (§6.1). Admitting a short, closed list would be cheap and
  would serve the audience; it is left open because the list has no principled
  boundary and because `hbar` works today. It is not the same question as §6.3,
  which is about *operators*.
- **Whether `write` should exist at all.** With `f"…"` in the language,
  `write(f"…")` in a loop is the only remaining use, and `print(f"…")` with the
  newline inside the string covers most of it. It is kept because a progress line
  ending in `\r` is real, but it is one of the two weakest entries on §4.3's list
  and it would be the first to go if the list is challenged.
