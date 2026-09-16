# Science — Design: unit literals

Date: 2026-09-16
Status: draft for review
Builds on: `scientific-libraries.md` §12, which decided that units live in the
type system and fixed the representation. **This note does not reopen that
decision.** It designs the layer above it: the surface syntax by which a number
becomes a dimensioned quantity. One clause of §12.6 is contradicted, in §6.1
below, and the contradiction is argued rather than assumed.
Written in: `syntax-revision-2.md`. Every code sample here is revision-2 Science.
Also depends on: `reserved-words.md` §0.1 (the dot rule),
`strings-formatting-and-docs.md` §3.3 (which lands `Display for Quantity` and
`unit_symbol` while this note was being written, and which §7 below adopts rather
than re-decides), the core spec §4.2 (numeric literal suffixes), §5.3 (const
generics), §9 (diagnostic ranges), §13 (reserved words).

---

## 1. The sentence this note is trying to earn

`scientific-libraries.md` §12 puts the dimension in the type and then, in §12.7,
shows how a number gets into one:

> ```science
> # Conversion is explicit and is the only place a bare number becomes a quantity.
> def metres(value: F64) -> Length of F64
> ```

That line is correct and it is the whole problem. A feature reached only through
a constructor call is a feature nobody reaches. Nobody writes

```science
let g be metres_per_second_squared(9.8)
let dose be milligrams(5.0)
let clock be megahertz(16.0)
```

three hundred times in a simulation. They write `9.8`, `5.0`, `16.0`, and the
units go back into a comment, which is where they were when the Mars Climate
Orbiter burned up.

So the claim this note makes is narrow and it is the reason to write the note at
all:

> **The units feature is worth exactly what its literal syntax is worth.** §12
> decided the semantics; if the notation costs more than a comment, the
> semantics never get exercised and the check never fires.

F# is the only mainstream language that got this right, and it got it right with
`1.0<m/s^2>`. That is the starting recommendation and §2 adopts it, after paying
the lexical bill in full.

There is a second claim, developed in §9, which is less obvious and matters more
for sequencing:

> **The notation is worth having before the type checking exists.** A literal
> that lowers to a bare `F64` still converts at compile time, still rejects an
> unknown unit, and still records the unit in a form a tool can read. Most of
> what units buy a working scientist is conversion, not dimensional analysis, and
> conversion needs no type-system feature at all.

---

## 2. The literal

### 2.1 Decision

**A unit literal is a numeric literal immediately followed by `<`, a unit
expression, and `>`, with no whitespace anywhere between the first digit and the
closing `>`. It is one token, produced by the lexer.**

```science
let g be 9.8<m/s^2>
let mass be 70<kg>
let dose be 5<mg>
let clock be 16<MHz>
let gas be 8.314<J/mol/K>
let thrust be 300<N>
```

The grammar, as an addition to §4.2 of the core spec:

```
number        := int-literal | float-literal
literal       := number width-suffix? unit-literal?
unit-literal  := "<" unit-expr ">"
```

with the absolute side condition that **no whitespace and no comment may appear
between `number` and the closing `>`**. A space before `<` is a comparison, every
time, with no diagnostic and no ambiguity:

| Source | Meaning |
|---|---|
| `9.8<m/s^2>` | a unit literal |
| `9.8 < x` | a comparison |
| `9.8< x` | a comparison — the scan fails at the space |
| `x<m>` | a comparison; `x` is not a numeric literal |
| `1<<3` | a shift; the scan fails at the second `<` |

### 2.2 The lexical problem, taken seriously

`<` and `>` are `Lt` and `Gt` in `crates/science-lexer/src/token.rs`. Angle
brackets for generics died in several languages for this reason, and the death
was always the same shape: `Array<T>` versus `a < b`, where both operands can be
arbitrary expressions and the disambiguation needs a symbol table or unbounded
lookahead.

**Science does not have that problem, for three independent reasons, and they
should be stated before the rule rather than after it.**

**One. Science already declined angle brackets for generics.** §5.3 writes
`Matrix of (T, const ROWS: Int, const COLS: Int)`, not `Matrix<T, R, C>`.
`llm-ergonomics.md` §7 records `Array[Int]` and `function f[T](x)` as errors the
compiler must diagnose, which confirms it. So `<` and `>` in *type* position are
entirely free, and the only contest is with the comparison operators in
*expression* position.

**Two. The left operand is restricted to a single token class.** A unit literal
can follow a numeric literal and nothing else. Not an identifier, not a call, not
a parenthesised expression, not a field access. `speed<m/s>` is a comparison.
That is a far narrower opening than `Foo<Bar>` ever was, and it is decidable in
the lexer with no symbol table.

**Three. §4.2 already does this.** `42i32` is one token today, produced by
`Lexer::numeric_suffix`, which reads a word immediately after the digits and
folds it into the literal. The rule "something glued to a number is part of the
number" is already the language's rule; the unit literal extends the alphabet of
that something from `[a-z0-9]` to a bracketed sublanguage. No new principle is
introduced.

### 2.3 The scan, and the backtrack

On finishing a numeric literal — including its optional width suffix — the lexer
checks whether the very next byte is `<`. If it is, it **tentatively** scans a
unit expression:

1. Consume `<`.
2. Consume characters from the unit alphabet: Unicode letters, ASCII digits,
   `*`, `/`, `^`, `-`, `(`, `)`, `.`, `%`, and the two normalising characters
   `·` and superscript digits.
3. Stop at the first character outside it.
4. If that character is `>` **and** the consumed text parses as a unit
   expression, emit one `Number { value, width, unit }` token.
5. Otherwise **reset the cursor to just after the number** and carry on as
   though the `<` had never been examined. No diagnostic, no token.

Step 5 is backtracking in a lexer, which is a cost and should be named. It is
bounded three ways: the scan never crosses a newline, it stops at the first
character not in a fixed alphabet, and there is at most one backtrack per numeric
literal. The lexer already does a bounded rescan of exactly this kind — the
comment at `lexer.rs:452`, *"an exponent needs digits, otherwise the `e` is a
suffix"*, is the same manoeuvre on a smaller alphabet.

### 2.4 The residue, and what happens to it

The rule is not perfect. Two inputs are misread, and both are worth showing
rather than hiding.

```science
if 1<n>0:          # lexes as the literal 1<n> followed by 0
```

The user meant `(1 < n) > 0`, which compares a `Bool` to an `Int` and does not
type-check under any reading. So no *correct* program is misread; a *wrong*
program gets a worse error than it deserves.

```science
if 1<n*m>0:        # same, with a two-factor unit expression
```

Same analysis. Chained relational operators are not a Science idiom, `n > 0 and
n < m` is, and the formatter puts spaces around every binary operator, so the
misreading cannot survive a `sciencec fmt`.

**Mitigation, and it is required to ship with the feature.** When the parser
fails on the token immediately following a unit literal, the error carries a
note: *`<…>` after a number is a unit literal; put spaces around `<` if you meant
a comparison.* This is a heuristic in the error path, which is exactly where
`reserved-words.md` §3.2 established that heuristics are safe.

### 2.5 The alternatives, ranked

Ranked worst to best, because the best rejected one is close enough to be
revisited if §2.3 misfires in practice.

**Fourth: a suffix, `9.8_m_per_s2`.** Rejected outright. `_` is already the digit
separator (`1_000_000`), so `9.8_m` is one keystroke from a mis-typed separator
and the lexer cannot tell which was meant. Worse, it encodes operators — `per`,
the trailing `2` — inside what is lexically an identifier, so there is no
canonical spelling: `m_per_s2`, `m_s_2`, `mps2` and `m_per_sec_squared` are all
the same unit and the language would have to bless one arbitrarily. `kg*m/s^2`
has no readable suffix form at all. **Serves nobody.**

**Third: a postfix call, `9.8.metres()`.** Rejected. It needs one method per
unit on every numeric type, and with 24 prefixes across a few hundred units that
is a method surface in the tens of thousands — or a macro, which F0 does not
have. It does not compose: there is no `.metres().per_second_squared()` that
means anything. And `9.8.metres()` requires the lexer to decide whether the `.`
after `9.8` opens a fraction or a method call, which is a *second* lexical
ambiguity introduced to avoid the first. **Serves programmers a little,
scientists not at all.**

**Second: a `unit` keyword.** Rejected for the literal. §13 lists `unit` under
*deliberately not reserved*, beside `grad`, `dim`, `axis` and `dtype`, on the
ground that it is a common variable name in the target audience's code; that
judgement is right and this note does not spend it. It is also the wrong shape:
a keyword helps at a *declaration*, not at a literal, and `let g be unit(9.8, m,
s, -2)` is worse than the constructor call it replaces. **Note that `unit` *is*
adopted below, contextually and in declaration position only (§3.4), which costs
nothing under `reserved-words.md` §3.2's pricing.**

**First, and the one worth arguing with: multiplication by a constant,
`9.8 * m / s ** 2`.** This is what `pint` does in Python and `Unitful.jl` does in
Julia, and it needs no lexer change whatever. Rejected for three reasons, in
increasing order of weight:

- **Precedence.** Science's power operator is `**` (`token.rs`), so the unit has
  to be written `m / s ** 2` and the reader has to trust a precedence table to
  know it is not `(9.8 * m / s) ** 2`. `m/s^2` inside a bracket needs no such
  trust.
- **Namespace.** It requires `m`, `s`, `g`, `K`, `A`, `N`, `J`, `W` and two
  hundred more to be *value* bindings in scope. Those are the single most
  contested identifiers in scientific code: `let m be mass` and `let K be
  stiffness` are lines a physicist writes in their first hour, and this design
  makes both of them shadow a unit silently. §3.3 below makes the unit namespace
  disjoint from the value namespace, which is the decisive advantage and is only
  available because the units live inside a bracket.
- **It is a runtime multiplication.** It folds only when the optimiser can see
  both operands, which is usually but not always, and "usually" is not a
  guarantee you can put in a spec. §5 makes the fold a front-end guarantee.

**Fallback if §2.3 fails in practice.** If the backtracking scan misfires on real
code in a way §2.4 has not anticipated, the escape is a one-character sigil:
`9.8u<m/s^2>`, or `9.8[m/s^2]`. Both remove the ambiguity entirely at the cost of
one character and some ugliness. Naming the fallback now is what makes this
decision reversible; see §11.

---

## 3. Inside the brackets

### 3.1 The unit expression grammar

```
unit-expr  := term (("*" | "/") term)*          # left-associative
term       := factor ("^" exponent)?
factor     := unit-ref | "(" unit-expr ")"
exponent   := "-"? digit+
unit-ref   := (module-path ".")? unit-name
unit-name  := prefix? declared-name
```

Left-associative `/` gives `J/mol/K` = `J·mol⁻¹·K⁻¹`, which is both what
ordinary arithmetic means and what a chemist means. The two readings agree, so
this needs no special rule — it is worth saying only because chemists write
`J/mol/K` constantly and would otherwise wonder.

Parentheses exist for `W/(m*K)` and for anyone who prefers them. Exponents are
integers only, positive or negative, per §12.6's exclusion of rational exponents.
An exponent of zero is legal and useless. No numbers other than exponents appear
in a unit expression; a scale is not a unit and belongs in the number.

**`^` is the exponent operator here, and it is `Caret` — bitwise XOR — in ordinary
Science expressions.** That is a genuine inconsistency and it is accepted
deliberately. Inside `< >` the language is a closed sublanguage with no bitwise
anything, `^` is what every scientist, every paper, UCUM, `pint`, `uom` and F# all
write, and `m/s**2` in a unit would look like a typo to the audience this feature
exists for. The precedent for a sublanguage with its own token meanings is
`ffi-c-boundary.md` §1.1, whose `extern` block is "a small separate grammar with
contextual keywords". **Serves scientists, at a small cost to programmers.**

### 3.2 Two spellings the formatter normalises

The lexer additionally accepts `·` (U+00B7) for `*`, and Unicode superscript
digits `²³⁴…` for `^n`, and `μ` (U+03BC GREEK SMALL LETTER MU) for `µ` (U+00B5
MICRO SIGN). `sciencec fmt` rewrites all three to the canonical ASCII form.

This looks like a violation of `syntax-revision-2.md` §1, which removed duplicate
spellings on the ground that they cannot be canonicalised. The difference is
exactly the one that section identified: `==` versus `is` could not be
canonicalised *because the choice depended on the operand types, and a formatter
that must run after type checking is not a formatter*. Here the rewrite is
purely lexical. The formatter can do it, so the drift argument does not apply,
and a scientist pasting `9.81<m·s⁻²>` out of a paper gets a clean file instead of
an error. **Serves scientists; costs programmers nothing, because the formatter
removes the variant before anyone reads the diff.**

Note that `m·s⁻²` pastes with a *superscript minus* (U+207B), which is accepted as
part of a superscript run. `⁻` outside a superscript run is an error.

### 3.3 The unit namespace

**Decision. Unit names live in a namespace of their own, entered only inside
`< >`, disjoint from values, types and modules.**

This is the single most important consequence of putting units inside a bracket
and it is why `9.8 * m / s ** 2` loses. Inside `< >` there is no such thing as a
variable, so `m` is always the metre and never the user's mass. Outside `< >`
there is no such thing as a unit, so `let m be mass` is an ordinary binding that
shadows nothing. Two vocabularies that are each other's worst collision are kept
apart by a bracket.

The cost is that Science now has four namespaces — values, types, modules, units.
That is real conceptual weight and it is the honest price; it is paid once, at the
bracket, and it never surfaces anywhere else in the language.

**Scoping.** A module's `unit` declarations are in the unit namespace wherever
that module is `use`d. Units are not imported one at a time; `use physics.units`
brings all of its units into scope. A name reachable through two `use`d modules
is `SC0235`, with the fix being qualification: `<si.bar>` or `<pressure.bar>`.

**The prelude.** The seven SI base units, the 22 coherent derived units, and the
prefix table are in scope with no `use` at all. Requiring `use physics.units`
before anyone can write `9.8<m/s^2>` would defeat the point of §1.

### 3.4 Declaring a unit: the vocabulary is open

**Decision. The compiler knows the seven base dimensions and the prefix table.
It knows no unit. Every unit — `m`, `s`, `N`, `ft`, `eV` — is a declaration
written in Science, and a user may write more.**

A closed vocabulary was considered and rejected in one line: `chem` wants
`Da`, `M`, `eq`; astronomy wants `pc`, `AU`, `Jy`, `M_sun`; geology wants `Ma`;
finance wants `bp`. A compiler that must be rebuilt to add a unit is a compiler
that will never have the unit you need. The same argument §12.3 gives for the
seven-exponent representation being *open* applies one level up.

The declaration form:

```
unit-decl := "unit" NAME "is" unit-body ("without" "prefixes")?
unit-body := "base" DIMENSION
           | "dimensionless"
           | scalar? unit-expr ("offset" scalar)?
```

`unit` is a contextual keyword in item-initial position only. `reserved-words.md`
§3.2 priced exactly this — an item that begins with a bare name, dispatched on the
second token — at *one match arm*, and observed that `Doc implements Summarize:`
already forces `parse_item` to parse a leading name and look ahead. `unit` stays
an ordinary identifier in every other position, so §13's *deliberately not
reserved* list is untouched.

`is` is the connective because §12.3 already uses it for aliases —
`type Length of T is Quantity of (T, 1, 0, 0, 0, 0, 0, 0)`.

The SI base, verbatim, with `DIMENSION` drawn from the seven const parameter
names of §12.3:

```science
unit m is base LENGTH
unit kg is base MASS without prefixes
unit s is base TIME
unit A is base CURRENT
unit K is base TEMPERATURE
unit mol is base AMOUNT
unit cd is base LUMINOUS
```

Derived units, which resolve by substitution:

```science
unit N is kg*m/s^2
unit J is N*m
unit W is J/s
unit Pa is N/m^2
unit Hz is 1/s
unit V is W/A
unit C is A*s
```

Scaled and non-SI units:

```science
unit g is 0.001 * kg
unit ft is 0.3048 * m without prefixes
unit min is 60 * s without prefixes
unit h is 3600 * s without prefixes
unit L is 0.001 * m^3
unit eV is 1.602176634e-19 * J
unit deg is (PI / 180) * rad without prefixes
```

The scalar may be a literal, a named float constant in scope, or a compile-time
expression over `+ - * /` on those. It is evaluated in the front end and nothing
else is admitted — no calls, no conditionals.

**The kilogram wart.** SI's base unit of mass is the *kilo*gram, which is the one
base unit whose name already carries a prefix. The declarations above handle it
with no special case: `kg` is the base and refuses prefixes; `g` is declared as
`0.001 * kg` and accepts them; `mg`, `Mg` and `ng` all resolve through `g`. Every
units library does this and it is worth the one paragraph because it is the first
thing a reader will try to break.

**Resolution.** A unit expression resolves, entirely in the front end, to a pair:
a vector of seven integer exponents and a scalar factor (plus, for §6, an
optional offset). Products add exponents and multiply factors; quotients subtract
and divide; `^n` multiplies exponents by `n` and raises the factor to `n`. Named
units are substituted recursively; a cycle in the declarations is `SC0236`.

---

## 4. Prefixes

### 4.1 Decision

**Prefixes are part of the grammar and compose. A unit name resolves by exact
match first; only if there is no exact match is a single prefix stripped, and the
remainder must itself be an exactly declared name that has not opted out.**

Three rules, and they are total:

1. **Exact match wins.** Always, unconditionally.
2. **At most one prefix, and the remainder must be a declared name.** Not a
   prefixed name, not another prefix.
3. **A unit declared `without prefixes` cannot be the remainder.**

The alternative — one declaration per prefixed unit — was rejected on arithmetic.
24 SI prefixes plus 8 binary prefixes across a few hundred units is five figures
of declarations, hand-written, each an opportunity to get a factor wrong. Nobody
maintains that table correctly.

### 4.2 What the three rules buy

The brief's own example: **`min` is minutes, not milli-inches.** Rule 1 settles
it — `min` is declared, so the exact match is taken and the prefix split is never
attempted. Rule 3 settles it a second time, because `in` is declared `without
prefixes`. The good cases and the interesting ones:

| Written | Resolves as | By which rule |
|---|---|---|
| `mm` | milli + `m` | no exact `mm`; rule 2 |
| `min` | minute | rule 1, exact match |
| `mol` | mole | rule 1 — not milli + `ol` |
| `cd` | candela | rule 1 — not centi + `d` (day) |
| `Pa` | pascal | rule 1 — not peta + `a` |
| `T` | tesla | rule 1 — not tera + nothing |
| `Gy` | gray | rule 1 — **so a gigayear is not `Gy`**; write `Gyr` |
| `dam` | deca + `m` | rule 2; `d` + `am` fails because `am` is not declared |
| `kg` | kilogram | rule 1, exact |
| `mft` | — | `SC0231`: `ft` takes no prefix |
| `mmm` | — | `SC0232`: milli + `mm` fails rule 2 |

Rule 2's "the remainder must be a *declared* name" is what makes the split
unique: because a prefixed name is never itself declared, a second prefix can
never be stripped, and `dam` has exactly one valid reading.

`SC0232`, ambiguous split, has no instance in the shipped table. It exists so
that a user's own `unit da is …` cannot silently change what `dam` means.

### 4.3 The table

| | | | | | | |
|---|---|---|---|---|---|---|
| `Q` 10³⁰ | `R` 10²⁷ | `Y` 10²⁴ | `Z` 10²¹ | `E` 10¹⁸ | `P` 10¹⁵ | `T` 10¹² |
| `G` 10⁹ | `M` 10⁶ | `k` 10³ | `h` 10² | `da` 10¹ | `d` 10⁻¹ | `c` 10⁻² |
| `m` 10⁻³ | `µ` 10⁻⁶ | `n` 10⁻⁹ | `p` 10⁻¹² | `f` 10⁻¹⁵ | `a` 10⁻¹⁸ | `z` 10⁻²¹ |
| `y` 10⁻²⁴ | `r` 10⁻²⁷ | `q` 10⁻³⁰ | `Ki` 2¹⁰ | `Mi` 2²⁰ | `Gi` 2³⁰ | `Ti` 2⁴⁰ |

Binary prefixes ship because `<MiB>` and `<GiB>` are written constantly in the
kind of code this language is for, and because a byte is dimensionless (§6.6) so
they cost nothing in the type. The remaining binary prefixes `Pi Ei Zi Yi` are in
the table for completeness.

`µ` is U+00B5 MICRO SIGN. `μ` (U+03BC) normalises to it per §3.2. For anyone who
cannot type either, `us` is declared as an alias for microsecond in the prelude,
which is the one case common enough to be worth an exception; everywhere else the
answer is `1e-6<s>`.

**Serves scientists.** A programmer would happily write `MILLI * METRE`. `5<mg>`
is the notation a pharmacologist already uses and the only one they will accept.

---

## 5. Scale, dimension, and what the literal lowers to

### 5.1 Decision

**Consistent with §12.6: `1<m>` and `1<ft>` have the same type and different
values. Every `Quantity` value is stored in SI coherent base units. The unit in
the literal is a compile-time conversion and is not carried into the value.**

The literal `L<U>`, where `U` resolves to exponents `(e₁…e₇)` and factor `k`,
lowers to:

```science
Quantity of (F64, e₁, e₂, e₃, e₄, e₅, e₆, e₇)(value: L × k)
```

with `L × k` **folded in the front end, before HIR**. So:

| Written | Type | Stored value |
|---|---|---|
| `9.8<m/s^2>` | `Acceleration of F64` | `9.8` |
| `2.5<km>` | `Length of F64` | `2500.0` |
| `5<mg>` | `Mass of F64` | `5e-6` |
| `1<ft>` | `Length of F64` | `0.3048` |
| `16<MHz>` | `Frequency of F64` | `1.6e7` |
| `8.314<J/mol/K>` | `Quantity of (F64, 2, 1, -2, 0, -1, -1, 0)` | `8.314` |

### 5.2 Where the factor comes from and when it is applied

From the `unit` declarations of §3.4, resolved at compile time. Applied at
compile time, because both operands are literals. This is a small piece of
constant folding with a large payoff: **the conversion error — the Mars Climate
Orbiter error — is eliminated by the notation alone, with zero runtime cost and,
as §9 develops, with zero type-system machinery.**

Two details that are easy to get wrong and cheap to get right:

**Factors are exact rationals in the compiler.** `0.3048` is not representable in
binary. If the compiler stores the factor as an `f64` and multiplies, `5<ft>`
rounds twice. Decision: a declaration's scalar is parsed into an exact rational
where it is written as a decimal or a ratio of integers, the product `L × k` is
computed in exact rational arithmetic, and it is rounded **once** to the target
type. This is strictly more accurate than the equivalent runtime multiplication,
it costs a rational type in the front end that the const-expression work of §12.4
will want anyway, and it is the sort of thing a scientific language should get
right without being asked.

A factor that is not rational — `deg`, which needs π — falls back to `f64`
evaluation and rounds twice. That is unavoidable and it is documented on the
declaration.

**Overflow is a compile error.** `1<Ym>` in `f32` overflows, and because the fold
is at compile time the compiler says so (`SC0253`) instead of storing an
infinity.

### 5.3 The consequence that will surprise someone

```science
if 1<ft> is 0.3048<m>:
    print("yes")
```

prints. Feet and metres are the *same type* with *equal values*, so this
comparison is true, and a reader who expected `ft` and `m` to be distinguishable
will find that they are not. That is §12.6's decision, not this note's, and §12.6
is right: putting the system in the type doubles the type count to buy a check
against a mistake that explicit conversion has already made impossible. But it is
the first thing a user will test, and the language reference should show this
exact snippet rather than let it be discovered.

### 5.4 Integer-typed quantities

**Decision. A unit literal defaults to `F64`. An integer width suffix is legal
only when the resolved factor is exactly 1 and the folded value is exact in the
target type; otherwise `SC0252`.**

```science
let a be 5<kg>             # Mass of F64, value 5.0
let b be 5i64<kg>          # Mass of I64, value 5 — factor is 1
let c be 5i64<mg>          # SC0252: 5 mg is 5e-6 kg, not an integer
```

The default is float even for `5<kg>`, where the mantissa has no dot, because a
unit conversion is almost never integral and an integer default would make
`5<mg>` an error for a reason the user cannot see. **Serves scientists;
programmers who want the integer say so.**

---

## 6. The awkward cases

Each of these is a thing a scientist writes. Each gets a decision.

### 6.1 Celsius, and a disagreement with §12.6

§12.6 excludes affine units from `Quantity` and says:

> Celsius is a separate `Temperature` type with explicit conversions, and never
> an arithmetic operand.

**This note disagrees with the second half of that sentence and keeps the first.**
The exclusion from `Quantity` is right: `20°C + 20°C` is meaningless and a linear
representation must not admit it. The *separate type* is wrong, and the reason is
the one this whole note is about — once a literal is the only way to obtain a
Celsius value, the affine part happens entirely at compile time and there is
nothing left to protect at runtime.

**Decision. An affine unit may appear in a unit literal, and only there, and only
as the entire unit expression. It converts at compile time and yields an ordinary
kelvin `Quantity`.**

```science
unit degC is K offset 273.15 without prefixes
unit degF is (5.0 / 9.0) * K offset 459.67 without prefixes
```

**The offset is applied before the factor**: a literal `L` in a unit with factor
`k` and offset `o` folds to `(L + o) × k`. So `37<degC>` is `(37 + 273.15) × 1 =
310.15` and `68<degF>` is `(68 + 459.67) × 5/9 = 293.15`. A unit with no `offset`
clause has `o = 0`, which makes the linear case a special case of this one rather
than a separate rule.

```science
let body be 37<degC>                 # Temperature of F64, value 310.15
let boiling be 100<degC>             # value 373.15
let rise be 5<K>                     # a difference; 5.0
let after be body + rise             # 315.15 — checks, and means what it says
```

and the error case:

```science
let bad be 3<degC/s>                 # SC0234
let worse be 2<degC> + 3<degC>       # legal, and means 278.15 K — see below
```

The second line is the honest cost. `2<degC> + 3<degC>` compiles, because after
folding both operands are kelvin `Quantity` values, and it computes 275.15 +
276.15 = 551.3 K, which is not what anybody meant. A separate `Temperature` type
would catch it. **The trade is: catching that one mistake costs every downstream
function an overload, a conversion at every boundary, and a second temperature
type that `physics.thermo` must thread through forty signatures.** Adding two
absolute temperatures is a rarer mistake than forgetting to convert Celsius at
all, which is what the literal fixes, and a lint — not a type — is the
proportionate answer: **`SC0255`, a warning when both operands of `+` are
`Temperature` and at least one was written with an affine literal.** That
information is available in the front end because the literal is a token, and it
is thrown away by lowering, so the check must run before the fold.

`without prefixes` is mandatory on an affine unit; a millicelsius has no meaning.

Why `<degC>` and not `<°C>`? `°` is not a letter, and admitting it would mean
admitting `°` into the unit alphabet for one unit. `degC` is what UCUM writes,
and `sciencec fmt` may accept `°C` on input and rewrite it under §3.2 if the
demand is there. **Serves programmers; scientists lose a symbol they can type.**

### 6.2 Angles

**Decision. Angle is not a dimension. `rad` is declared `dimensionless`, and a
unit literal whose exponents are all zero has type `F64` — a plain number.**

```science
unit rad is dimensionless
unit sr is rad^2
unit deg is (PI / 180) * rad without prefixes
```

```science
let quarter be 90<deg>         # F64, value 1.5707963267948966
let full be 2<rad>             # F64, value 2.0
print(math.sin(quarter))       # 1.0
```

The alternative — an eighth pseudo-dimension for angle, which `boost::units`
offers as an option — is rejected. It breaks §12.3's seven-element vector, it
means `math.sin` cannot take an `F64`, and it makes `rad` non-interchangeable
with the dimensionless ratios that arc length divided by radius actually
produces. The cost is stated plainly:

> **This catches no type error.** Passing a length to `sin` still compiles. What
> it removes is the *conversion* error — degrees handed to a function expecting
> radians — which is the one that actually happens, at a rate no dimensional
> analysis would have caught anyway because both are dimensionless.

**Serves scientists.** `90<deg>` is the whole feature.

### 6.3 Decibels, nepers, pH

**Decision. Rejected from the unit grammar, with a diagnostic of their own.**

A decibel is not a scale factor. It is `10·log₁₀` of a ratio against a reference
that the symbol does not name — `dBm` is against a milliwatt, `dBV` against a
volt, `dBSPL` against 20 µPa — and none of that composes with products and
quotients. pH is `−log₁₀` of an activity. Putting a logarithm into a grammar
whose entire semantics is "multiply the factors" would be a lie that surfaces the
first time someone writes `<dB/s>`.

```science
let gain be 3<dB>              # SC0233
```

> ```
> error[SC0233]: `dB` is a logarithmic unit and cannot appear in a unit literal
>   --> gain.science:1:15
>    |
>  1 | let gain be 3<dB>
>    |               ^^ logarithmic, not a scale factor
>    |
>    = note: a decibel is a ratio on a log scale, and its reference is not in the
>            symbol; `dBm`, `dBV` and `dBSPL` have different references
>    = help: use `physics.units.decibels(ratio)` to convert a ratio to dB, or
>            `physics.units.from_decibels(db)` to convert back
> ```

A separate diagnostic rather than "unknown unit" matters: a user who writes
`<dB>` has a correct mental model and needs to be told why the language declines,
not told the symbol does not exist.

### 6.4 Percent

**Decision. `%` is a dimensionless unit with factor 0.01, spelled `%` and only
`%`, legal only inside `< >`.**

```science
let rate be 4.5<%>             # F64, value 0.045
```

`%` is `Percent` in the token set — modulo, in expressions — and the sublanguage
rule of §3.1 makes it free inside the brackets. There is no `percent` spelling:
one spelling per operation, per `syntax-revision-2.md` §1. Permille `‰` is not
shipped; `1e-3` is clearer and nobody will miss it.

### 6.5 Counts, and `mol`

**`mol` stays a base dimension**, as §12.3 has it. It earns its place: `g/mol` is
molar mass, `mol/L` is concentration, and confusing a mass with an amount is a
real error in every chemistry pipeline. The philosophical objection that a mole
is a count and counts are dimensionless is correct and is overruled on the
grounds that SI overruled it first and `chem` needs the check.

**A bare count is not a unit.** There is no `<items>`, no `<count>`, no
`<particles>`. A count is an `Int`. Adding a dimensionless named unit for it
would let `5<items> + 3<apples>` type-check, which is exactly the false comfort a
units feature should not sell.

`B` (byte) is declared dimensionless with factor 1, which is what makes `<MiB>`
work, and it carries the same caveat: `<MiB> + <Mm>` does not type-check only
because the metre has a dimension, and `<MiB> + <%>` does.

---

## 7. Printing

### 7.1 Where the unit string comes from — already decided, and adopted

`strings-formatting-and-docs.md` §3.3 landed while this note was being drafted
and answers the "where does `9.8 m/s²` come from" question in full. **This note
adopts it without amendment.** Its shape:

```science
Quantity implements DisplayNumber

Quantity implements Display:
    def display(borrowed self, into: mutable borrowed Formatter):
        into.number(self.value as F64)
        into.raw(" ")
        into.raw(Self.unit_symbol())
```

with `unit_symbol` an associated function over §12.3's seven const exponents. The
key finding, and it is that note's, not this one's: **no language feature is
required for units to print.** The exponents are const generic parameters, so
after monomorphization `unit_symbol()` is a constant in each instantiation. There
is no runtime table keyed on seven integers, which is what the alternative would
have cost every binary that touches `physics`.

```science
let g be 9.80665<m/s^2>
print(f"{g:.2f}")              # 9.81 m/s²
print(f"{g}")                  # 9.80665 m/s²
```

What that note delegates to the library is *what `unit_symbol` returns*. That is
this note's contribution and it is §7.2.

### 7.2 What `unit_symbol` returns

**Decision.** Two rules, in order:

1. **If the exponent vector matches one of the 22 SI coherent derived units,
   print that symbol.** `N`, `J`, `W`, `Pa`, `Hz`, `V`, `Ω`, `C`, `F`, `S`, `Wb`,
   `T`, `H`, `lm`, `lx`, `Bq`, `Gy`, `Sv`, `kat`, `rad`, `sr`, `°C`.
2. **Otherwise print the base product**, in the canonical SI order
   kg · m · s · A · K · mol · cd: positive exponents first, then negative ones
   after a `/`, with the exponent elided when it is 1 and rendered as a Unicode
   superscript otherwise.

```
9.8<m/s^2>            →  9.8 m/s²
300<N>                →  300 N
8.314<J/mol/K>        →  8.314 J/(mol·K)
1<kg*m^2/s^3/A^2>     →  1 Ω
2.5<kg*m^4/s^2/mol>   →  2.5 kg·m⁴/(s²·mol)
```

**Rule 1 is wrong for torque and this is accepted.** A newton-metre and a joule
have identical exponents, so a torque prints as `J`. The type system cannot
distinguish them — that is a semantic difference, not a dimensional one, and §12
deliberately declined to model semantics. Every units library in every language
has this behaviour. The escape is `q.format_base()`, which forces rule 2. Naming
the limitation is better than pretending the printer can resolve it.

**A dimension with no conventional name always prints**, via rule 2, which is
total: any exponent vector has a base product. The printer never fails and never
falls back to the raw type.

### 7.3 Printing in a unit that is not the stored one — the one open seam

`print(f"{d}")` prints metres, because metres is what is stored (§5.1). Printing
a length in feet has no spelling today.

`strings-formatting-and-docs.md` §2.1 fixes the spec grammar as Python's, ending
in a single-character `code`, and §2.6 lists what is deliberately excluded. **A
unit selector is in neither list**, because that note was written before this one
and had no reason to reserve room for it. So this is a live request against a
note that is otherwise settled, and it should be made explicitly rather than
assumed:

> **Requirement.** The format specification needs a way to name an output unit —
> `f"{d:ft}"`, or `f"{d:.2f ft}"`, or a conversion flag `f"{d!u(ft)}"`. Which
> shape is that note's call, not this one's.

The cost of refusing it is bounded and should be weighed honestly, because
refusing may well be right:

```science
print(f"{d.in(FOOT)} ft")      # works today; the unit is a string the user typed
```

That is one line, it is explicit, and it has exactly one defect — the `ft` in the
string is not checked against the `FOOT` in the expression, so a copy-paste can
print a length in feet labelled `m`. Whether that defect is worth a spec code is
a judgement for the formatting note. **This note's position: it is worth it, and
it is the only place in the design where a unit can still be wrong at runtime.**

### 7.4 `d.in(FOOT)`, and a dependency on the dot rule

Getting a number *out* in a chosen unit is the mirror of the literal and it is a
runtime operation, so it cannot use the bracket. Each `unit` declaration also
generates an ordinary constant in the *value* namespace, screaming-case, of an
opaque type `UnitOf of (…)` carrying the same factor:

```science
let d be 100<m>
let feet be d.in(FOOT)         # F64, 328.0839895013123
let back be Quantity.from(feet, FOOT)
```

`in` is a keyword — `for x in xs` — so `d.in(FOOT)` is legal **only** under
`reserved-words.md` §0.1's dot rule, which accepts any word as a member name. That
note asks for the rule "regardless of every other decision here"; this note is a
second caller. **If the dot rule is refused, the spelling is `d.value_in(FOOT)`**
and nothing else changes.

An affine unit has no such constant. `DEG_C` does not exist, and
`d.in(DEG_C)` therefore fails at name resolution rather than needing a special
case. `physics.units` provides hand-written `to_celsius` and `to_fahrenheit`
instead, which are the only two places the offset appears at runtime.

---

## 8. Width suffix and unit together

**Decision. Width suffix first, unit second. Both optional. No whitespace. The
reverse order is `SC0014`, with an applicable fix that swaps them.**

```science
let a be 2.5<m>                # Quantity of (F64, 1, 0, 0, 0, 0, 0, 0)
let b be 2.5f32<m>             # Quantity of (F32, 1, 0, 0, 0, 0, 0, 0)
let c be 5i64<kg>              # Quantity of (I64, 0, 1, 0, 0, 0, 0, 0)
let d be 2.5<m>f32             # SC0014: write `2.5f32<m>`
```

Two reasons, and the first is enough.

**The suffix is part of the number; the unit is a wrapper around it.** §4.2 and
`Lexer::numeric_suffix` already treat `f32` as belonging to the literal's
*representation*. Reading left to right, `2.5f32<m>` is "the number 2.5, as an
`f32`, interpreted as metres" — the representation question answered before the
meaning question, which is the order they are asked in.

**The reverse costs a second backtrack.** Admitting `2.5<m>f32` means the lexer
must re-enter suffix scanning after the closing `>`, which stacks on top of §2.3's
tentative scan. One bounded backtrack per literal is a cost worth paying; two is
not.

`SC0014` is worth having rather than folding into `SC0009`, because a model
writing Science will produce `2.5<m>f32` and the fix is mechanical.

While we are here, **`SC0009` needs a note it does not have today.** Its current
message lists the ten width suffixes and stops. When the rejected suffix is a
known unit name, it should say so:

> ```
> error[SC0009]: `m` is not a valid numeric literal suffix
>   --> sim.science:3:13
>    |
>  3 | let g be 9.8m
>    |             ^ unknown suffix
>    |
>    = help: for a unit, write `9.8<m>`
> ```

`9.8m` and `9.8 m` are exactly what a language model trained on Python and
MATLAB will emit, so this is a `llm-ergonomics.md` concern as much as a
diagnostics one.

---

## 9. What this asks of F0, and what waits

This is the section that matters for sequencing, and its finding is that the two
halves of "units" have almost nothing to do with each other.

### 9.1 The two halves

| | Notation | Checking |
|---|---|---|
| **What it is** | `9.8<m/s^2>` lexes, resolves, folds | `a * b` has the right dimension |
| **Where it lives** | lexer, resolver, lowering | type checker |
| **What it needs** | nothing that does not exist | const-expression arithmetic (§12.4) |
| **What it catches** | wrong unit, unknown unit, wrong conversion | wrong dimension |
| **Ships in** | F0, today | F1, after §12.4 |

§12.4 is precise that the blocker is multiplication: `L1 + L2` in type position,
its evaluation, and equality up to normalisation. **None of that is on the path
for the notation.** Nothing in §§2–8 of this note requires a type-system change.
The lexer change is one function and a token field. The resolver change is the
unit namespace, the prefix rules and the folding, all of which are ordinary
front-end work over a declaration form. The lowering is a struct literal.

### 9.2 Three stages, one syntax

**Decision. The literal syntax is identical in all three stages. Only the
lowering target changes, so a program written in stage A recompiles unchanged in
stage C.**

**Stage A — notation only. F0, today, zero type-system change.**
`9.8<m/s^2>` lowers to a bare `F64` with the value 9.8. No `Quantity`, no
exponents, no dimension in the type.

What that buys, which is more than it sounds:

- The conversion is done, exactly, at compile time. `5<mg>` is `5e-6`, and the
  factor came from a declaration rather than from the author's arithmetic.
- The unit vocabulary is checked. `9.8<m/z>` does not compile.
- The unit is *in the source, in a canonical form a tool can read*. A linter, a
  doc generator, or a reviewer can see what `9.8` is. A comment cannot be
  checked; `<m/s^2>` can.
- **This is the Mars Climate Orbiter fix.** That failure was a pound-force-second
  handed to a routine expecting a newton-second: a *conversion* error. Stage A
  catches it, with no type system at all.

What it does not buy: nothing catches `thrust + burn`.

**Stage B — dimension in the type, additive checking. F0 plus the `Quantity`
type of §12.3, still no const arithmetic.**
The literal lowers to `Quantity of (F64, …)`. Addition, subtraction, comparison
and function-signature matching are all checked, because — §12.4's own words —
*"Addition is easy: both operands must have identical exponents, which is
ordinary type equality and works today."* Multiplication is not expressible and
is not offered.

The alternative considered for stage B was **generating a finite set of
`Mul`/`Div` implementations** for the dimension pairs `physics` actually uses.
Rejected: the pairs are combinatorial in practice, the orphan rule makes user
extension impossible, and the error message when a pair is missing would be
"`Quantity of (F64, 1, 1, -2, 0, 0, 0, 0)` does not implement `Mul`", which is
worse than having no multiplication at all. **Multiplication waits for §12.4.
Cleanly.**

**Stage C — full. F1, after §12.4 lands for shapes.**
`Mul`, `Div`, integer powers, `sqrt` on even dimensions. §12.5's argument stands
unchanged: this is a consumer of work the tensor shapes require anyway.

### 9.3 What this does to §13's sequencing

`scientific-libraries.md` §13 puts `physics.units` in **wave 5**, gated on
const-expression arithmetic, and has wave 2 ship `physics.constants` with
"quantities as `F64` with a documented unit". This note asks for one amendment:

> **The unit literal moves to wave 1, and wave 2's compromise improves.** Wave 2
> ships quantities as `F64` — but written `9.8<m/s^2>`, with the conversion
> already exact and the unit already in the source. Wave 5 then changes the
> lowering target, not the call sites, which is the same "type change with no
> call-site changes" property §13 already relies on for the retrofit.

That is strictly better than the note's current plan at no extra cost, and it is
the only change to §13 this note requests.

---

## 10. Diagnostics

Codes in §9's ranges: lexical `SC0001`–`SC0099`, syntax `SC0100`–`SC0199`,
resolution `SC0200`–`SC0249`, types `SC0250`–`SC0299`.

| Code | Phase | Condition | Applicable fix |
|---|---|---|---|
| `SC0012` | Lexical | A tentative unit scan reached end of line with no `>`, and the consumed text is a well-formed unit expression. Reported only in this narrow case, so ordinary comparisons are silent. | none; note: *put spaces around `<` for a comparison* |
| `SC0013` | Lexical | Malformed unit expression: `9.8<m//s>`, `9.8<m^>`, `9.8<^2>`, `9.8<m*>` | none |
| `SC0014` | Lexical | Width suffix after the unit: `2.5<m>f32` | **yes** — swap to `2.5f32<m>` |
| `SC0009` | Lexical | *(existing)* Unknown numeric suffix. **Gains a note** when the suffix is a known unit name: `9.8m` | **yes** — `9.8<m>` |
| `SC0230` | Resolution | Unknown unit name, with a did-you-mean over the unit namespace *and* over one-prefix splits | **yes** when the suggestion is unique |
| `SC0231` | Resolution | A prefix applied to a unit declared `without prefixes`: `<mft>` | none; note names the declaration and suggests scaling the number |
| `SC0232` | Resolution | Two valid prefix splits, or a split whose remainder is itself prefixed: `<mmm>` | none; names both readings |
| `SC0233` | Resolution | A logarithmic unit in a literal: `<dB>`, `<Np>`, `<pH>` | **yes** — names `physics.units.decibels` |
| `SC0234` | Resolution | An affine unit not standing alone: `<degC/s>`, `<degC^2>`, `<K*degC>` | **yes** — `<K/s>`, `<K^2>`, `<K>` |
| `SC0235` | Resolution | A unit name reachable through two `use`d modules | **yes** — qualify, `<si.bar>` |
| `SC0236` | Resolution | A cycle in `unit` declarations | none; prints the cycle |
| `SC0256` | Types | Dimension mismatch in `+`, `-`, or an ordering comparison | none |
| `SC0252` | Types | An integer width suffix on a literal whose conversion factor is not 1, or whose folded value is not exact | **yes** — drop the suffix |
| `SC0253` | Types | The folded value overflows or underflows the target type | none |
| `SC0254` | Types | `sqrt` of an odd dimension — §12.6's rational-exponent exclusion, surfacing | none; note names the exclusion |
| `SC0255` | Types (warning) | Both operands of `+` are `Temperature` and at least one was written with an affine literal (§6.1) | none |

### 10.1 The one that decides whether this is usable

`SC0256` is the error a user will see most, and it must not print the type.

```
error[SC0256]: these have different dimensions and cannot be added
  --> burn.science:9:18
   |
 7 | let thrust be 300<N>
   |               -------  Force — kg·m/s²
 8 | let burn be 90<s>
   |             -----    Time — s
 9 | let total be thrust + burn
   |              ------ ^ ---- Time
   |              |
   |              Force
```

The alternative, which is what a naive type printer produces, is:

```
expected `Quantity of (F64, 1, 1, -2, 0, 0, 0, 0)`,
   found `Quantity of (F64, 0, 0, 1, 0, 0, 0, 0)`
```

**This is unreadable and it would sink the feature.** So this note asks for a
concrete thing from the type printer: **`Quantity` is printed specially, as its
alias name where one exists (§12.3 declares nine) and as the §7.2 unit string
otherwise.** The rendering code is the same code as §7.1's `Display`, run at
diagnostic time instead of codegen time, so it is written once.

---

## 11. What this note asks of others

**Of `scientific-libraries.md`:**

1. **Amend §13's wave table**, moving the unit literal into wave 1 and restating
   wave 2's compromise as "written with units, checked for conversion, not
   checked for dimension" (§9.3).
2. **Amend §12.6's affine clause.** Drop "Celsius is a separate `Temperature`
   type" in favour of §6.1 here: affine units exist as literal notation only and
   produce an ordinary kelvin `Quantity`. The exclusion of affine units from
   `Quantity` itself is unchanged and endorsed.
3. **Add the `unit` declarations** to `physics.units` — the seven base, the 22
   coherent derived, the prefix table, and the non-SI set. That table is an API
   surface; see §12.
4. **Confirm** that `Quantity`'s `value` field is constructible from lowered code
   in another module, or say what the lowering should call instead.

**Of the core spec:**

5. **§4.2** gains the unit literal in *Literals*, beside `42i32` and `2.5f32`.
6. **§13** gains `unit` as a contextual, item-initial declaration word, with a
   sentence saying it is not reserved in any other position. Nothing leaves the
   list.
7. **§9** gains the fifteen codes of §10.
8. **The type printer** renders `Quantity` specially (§10.1). This is not
   optional; without it `SC0256` is unreadable.

**Of `reserved-words.md`:**

9. **The dot rule** (§0.1) is a hard dependency for `d.in(FOOT)` (§7.4). This
   note is the second caller for a rule that note already asks for
   unconditionally. If it is refused, the spelling becomes `d.value_in(FOOT)`.

**Of `strings-formatting-and-docs.md`:**

10. **A unit selector in the format specification** (§7.3). That note's §2.1
    grammar has no slot for one and its §2.6 does not exclude one, so this is a
    request, not a contradiction. `f"{d:ft}"` is the obvious shape; the choice is
    that note's. Everything else about printing — `DisplayNumber`, the
    `Formatter` protocol, `unit_symbol` as an associated function over the const
    exponents — is adopted here unchanged, and what `unit_symbol` *returns* is
    specified in §7.2 above, which is the piece that note delegated to the
    library.

**Of `llm-ergonomics.md`:**

11. **`9.8m` and `9.8 m` are what a model will write.** `SC0009`'s new note
    (§10) belongs in that note's table of model-emitted forms, with a fix, at
    zero-token cost.

**Of `data-io.md`:** nothing yet, and one open question — see §13.

---

## 12. Risks

**The angle bracket is the irreversible decision in this note.** Everything else
here could be changed in a later revision at the cost of a mechanical migration.
`<` after a numeric literal cannot: once shipped, it is consumed forever, and any
future syntax that wanted `<` in that position is dead. §2.5 names the fallback —
a sigil, `9.8u<m/s^2>` — and the fallback should be taken *before* the first
release if §2.3's scan misfires on real code, not after. **This is the one
decision that should not be allowed to drift into F0 unexamined.**

**The prelude's unit table is a commitment surface, in the same way
`scientific-libraries.md` §15 says its catalogue is.** Shipping `bar`, `atm`,
`psi`, `cal`, `eV`, `Da`, `AU` and `pc` means committing to their exact factors
forever, because a program that compiled with one factor and now compiles with
another is a silent numerical change with no diagnostic. The table needs a
provenance — CODATA and the SI brochure, cited per entry — and a policy that says
factors never change. That policy does not exist and should be written with the
table.

**Stage A will be read as "units are done".** A release that ships
`9.8<m/s^2>` and no dimension checking looks, to anyone who does not read the
release notes, like the units feature. Then `thrust + burn` compiles, somebody
writes a blog post, and the feature's reputation is set before stage B exists.
Mitigation: stage A's `9.8<m/s^2>` should be documented as *unit notation*, not
as *units*, in the same sentence every time, and the reference should state what
is not checked before it states what is.

**The unit namespace is a fourth namespace.** Values, types, modules, units. It is
invisible in practice because it is entered only by a bracket, but it is one more
thing to explain, one more place a name can hide, and one more resolution path in
the compiler. §3.3 argues it pays for itself. If it does not, the cost is
structural and not easily removed.

**Torque prints as `J`** (§7.2), and it always will. A user who trusts the
printed unit to be the *semantic* unit will be wrong occasionally. Every units
library has this and nobody has solved it, but Science's pitch is verification and
a wrong-looking output undercuts the pitch more here than elsewhere.

**§6.1's disagreement with §12.6 may simply be rejected.** If the libraries
note's author keeps the separate `Temperature` type, this note's §6.1 becomes:
`20<degC>` produces that type, `SC0255` is unnecessary, and every `physics.thermo`
signature that takes a temperature grows an overload. Nothing else in this note
changes. The disagreement is contained.

**`^` means two things.** XOR in expressions, exponent in unit expressions. §3.1
argues the sublanguage makes this safe. It is still a thing that has to be
taught, and the first person to write `2<m^2>` and then `flags ^ MASK` on the next
line will notice.

---

## 13. Open questions

**How does a unit reach a `Frame` column?** `data-io.md` §4.3 turns a record type
into a schema, and a CSV of temperatures in °C has its unit in a header, a
sidecar, or nowhere. `Quantity of (F64, …)` as a field type is the natural answer
and it raises a question this note does not answer: does the *parser* of the file
apply the conversion, and if so where does it learn the unit from? A literal
cannot help, because there is no literal. This wants a paragraph in that note or
a joint one.

**Does a unit belong in an FFI signature?** `ffi-c-boundary.md` §4 requires FFI
types to be representable. A `Quantity` is a one-field wrapper over `F64` and is
representable by any reasonable definition, but "representable" and "safe to hand
to C" differ here: C has no idea what unit the value is in, and the entire value
of the type evaporates at the boundary. Whether `extern` accepts a `Quantity`
parameter — silently unwrapping — or demands `.in(…)` at the call site is a real
decision and it belongs to that note.

**Should `use` be able to import units selectively?** §3.3 says a module's units
arrive wholesale with the module. That is simple and it means a program cannot
narrow the unit namespace to avoid an `SC0235`. The alternative, `use physics.units (m, s, kg)`,
would need `use` to distinguish a unit from a value in its import list, which is
new syntax for a problem nobody has hit yet. Deferred deliberately, and noted so
that it is deferred rather than forgotten.

**What is the uncertainty story?** Three notes now point at the same hole and
none of them fills it. `scientific-libraries.md` §14, item 6, asks whether there
is an `Uncertain of T`, observes that it interacts with `Quantity`, and says it is
much cheaper to design the two together than to bolt one onto the other.
`strings-formatting-and-docs.md` §3.3 says `9.81 ± 0.02 m/s²` "is a value with an
uncertainty, not a formatting of a value. It belongs in `Quantity`'s design if
anywhere." A literal syntax is where the question becomes concrete, because CODATA
writes a value and its uncertainty as one token — `9.80665(15)<m/s^2>` — and the
lexer either admits the parenthesised digits or it does not.

**This note does not decide it.** It records that the decision now has three
callers, that it is a lexical question as much as a type question, and that every
month it waits makes it more expensive, because `physics.constants` ships in wave
2 and every constant in it carries an uncertainty that will have nowhere to go.
