# Science — Design: the equation as a construct

Date: 2026-09-16
Status: draft for review
Owns: the `equation` item form — its body grammar, its dimensional obligation,
its symbolic derivative, its typeset rendering, and the reservation of the word.
Diagnostics claimed: **`SC0246`–`SC0249`** (resolution). One amendment to a code
this note does not own (`SC0256`), and one request against a reserved block this
note does not own (`SR0008`–`SR0099`).
Related, and in reading order: `publishable-output.md`, which owns the path from
a number to a typeset artefact and which this note extends from the *number* to
the *method*; `scientific-libraries.md` §12 (units in the type), §5.3, §5.4, §5.6
and §8 (the consumers of a symbolic derivative), and §10.1, whose `Formula` type
settles the naming question; `const-expression-arithmetic.md`, on which the
dimensional check is entirely gated; `unit-literals.md` §10.1, whose `SC0256`
rendering this note extends; `uncertainty.md` §3, whose gradient this note can
supply exactly and can print; `reserved-words.md`, whose audit method §8 reuses;
`effects.md` §6, whose `pure def` an equation implies by its form; the core
spec's §2 (the phase table), §4.1 (how far the English goes) and §13 (the
reserved list, which already contains this note's word).
Written in the syntax of `syntax-revision-2.md` and `syntax-revision-3.md`.

**A note on what was already decided before this note was written.** `equation`
is already in §13's *reserved, not yet used* list and already in
`crates/science-lexer/src/token.rs` as `ReservedWord::Equation`, with a doc
comment forward-referencing this file. This note therefore **ratifies a
reservation rather than requesting one**, and §8 is written to justify a decision
already taken and to check that nothing else needs taking with it.

---

## 0. The one sentence

**An equation is a declaration whose body is a single expression drawn from a
closed grammar over typed, dimensioned, typeset-named variables; the compiler
checks that the two sides balance dimensionally, generates the callable, and
generates the LaTeX — and the whole justification for it being a language
construct rather than a library is that a closed grammar makes rendering and
differentiation *total*, where a function body would make them partial.**

Three consequences, each argued below and each with a section that could refute
it:

> **The dimensional check is not this note's contribution.**
> `scientific-libraries.md` §12 already gives it to an ordinary annotated
> function. What this note contributes is that the checked object and the
> typeset object are the same object, and §2.2 is the section that takes that
> objection at full strength rather than around it.

> **The acausal reading — Modelica's — is refused, and it is the most valuable
> thing being refused.** It is real, proven and widely used, and it is a
> compiler-sized subsystem with famously unreadable diagnostics. §6 prices it.

> **The notation reading is refused outright.** Fortress is the warning, §4.1 of
> the core spec is the rule, and §11.3 is the section that has to show Science's
> version is not the same bet in a nicer coat.

---

## 1. Which of four different things is being asked for

"Should the language have a construct for an equation" is four proposals wearing
one word. They share almost nothing: different costs, different precedents,
different failure modes, and three of the four have a dead or a niche language
attached to them. Separating them is most of the work.

### 1.1 A notation — writing mathematics closer to how it is printed

The proposal: let the source look like the paper. Unicode operators, `∑` and
`∇` and `√`, juxtaposition as multiplication, subscripts, and a renderer that
turns the source itself into typeset mathematics.

**Sun's Fortress is the case, and it is the cautionary one.** Fortress
(Guy Steele et al., from the DARPA HPCS programme) had exactly this as its
headline: an ASCII source form and a *"Fortress rendering"* into typeset
mathematics, juxtaposition as multiplication, Unicode operators as first-class
syntax, and a deliberate ambition that a Fortress program should look like the
equations in the paper it implemented. Sun did not win HPCS phase III, the
project moved to an open-source track, and Steele wound it up in 2012 — citing,
among other things, that the type system had turned out to be very hard to
implement efficiently on the JVM.

Two honesty notes, because the easy version of this story is wrong in both
directions.

- **Notation did not kill Fortress on its own.** The funding and the
  implementation difficulty are the proximate causes, and the implementation
  difficulty was mostly the type system (symmetric multiple dispatch with
  overloading rules that had to be checked for ambiguity), not the glyphs.
  Anyone who says "Fortress died because of Unicode" is telling a tidier story
  than the evidence supports.
- **Notation is not universally fatal.** Mathematica has been commercially
  successful for thirty-five years with a notational front end. But its notation
  lives in a proprietary notebook that is also the product, its input is still
  overwhelmingly ASCII function calls, and nobody maintains a Mathematica
  codebase in `git` with four collaborators.

What survives from Fortress as a *usable* warning is narrower and is exactly
§4.1 of the core spec:

> COBOL, AppleScript, and Inform 7 all found the same failure: when code reads
> like English, readers assume it behaves like English, and it fails exactly
> where a symbol would have promised nothing.

Substitute *mathematics* for *English* and the sentence is still true, and it is
worse, because mathematical notation is genuinely ambiguous in ways English is
not: juxtaposition is multiplication except when it is function application,
`dy/dx` is not a quotient, `∑` binds a variable whose scope is a convention, and
a superscript is a power except when it is an index or a label. A language that
accepts the notation has to decide each of those, and every decision it makes
will be the wrong one for some field.

**Refused.** §1.5.

### 1.2 A declarative relation — Modelica's acausal modelling

The proposal: write `f = m * a` without saying which of the three is the output,
and let the compiler solve for whichever is unknown at each use.

This is **not** speculative. It is Modelica, it is thirty years old, it has a
standards body and multiple implementations (Dymola, OpenModelica, Wolfram
System Modeler), and it is the backbone of automotive, aerospace and
building-systems simulation. Julia's ModelingToolkit.jl is the same idea in a
newer language and is the most active current work. **This is the reading with
the most proven value in the set**, and refusing it is the note's largest
decision.

What it costs is §6.

### 1.3 A symbolic expression — SymPy, Symbolics.jl, ModelingToolkit

The proposal: the equation is a manipulable object. It can be simplified,
substituted into, differentiated, integrated, solved, series-expanded, rewritten,
and only then evaluated.

This is a computer algebra system. SymPy is one, Symbolics.jl is one, Maxima and
GiNaC are ones. They are large — SymPy is hundreds of thousands of lines — and
they are libraries in every language that has them, including languages with far
weaker type systems than Science's.

The important observation is that **a very small slice of this is what the other
three readings need**, and the slice is much smaller than a CAS:

- Rendering needs a precedence-aware pretty printer over the expression tree.
- Differentiation needs one recursive rewrite over a closed operator set.
- Uncertainty propagation needs the partial derivatives, which is the same
  rewrite.
- Nothing needs `simplify`, `integrate`, `solve`, `series` or `rewrite`.

So the question is not "should Science have a CAS" — it should not — but "how
small a symbolic layer does the valuable part require", and the answer is: a
tree, a printer, and a differentiator.

### 1.4 A renderable object — the LaTeX is generated, not written

The proposal: the executable definition is the only definition, and the typeset
form is derived from it. The scientist writes the equation once.

This is the cheapest of the four by a wide margin and it is most of the value,
because it is the one that closes a loop nothing else in the design closes.
`publishable-output.md` §0.2 earns this sentence:

> Every number in this document was produced by the program that produced this
> document, in the unit its type says, rounded by the rule stated in the
> appendix.

The equation earns the adjacent one, and the adjacent one is about the *method*
rather than the result:

> **Every equation in this document is the expression that was evaluated, in the
> dimensions its variables declare, with the symbols the declaration names.**

The incumbents in this reading are real and should be named: `latexify_py` and
`handcalcs` render Python source to LaTeX, `Latexify.jl` does it for Julia, and
`sympy.latex` renders a SymPy expression. §2.1 takes them seriously.

### 1.5 The ranking, and the first decision

By value to *this* language's audience — scientists whose deliverable is a paper
— against cost:

| # | Reading | Value | Cost | Verdict |
|---|---|---|---|---|
| 1 | **Renderable** (§1.4) | Closes the drift loop between manuscript and code; nothing else in the design closes it | A printer over a closed tree | **Adopt** |
| 2 | **Symbolic**, restricted to a tree, a printer and a differentiator (§1.3) | Makes (1) possible at all; supplies analytic Jacobians and exact uncertainty coefficients | One recursive rewrite; no simplifier | **Adopt the slice, refuse the CAS** |
| 3 | **Acausal** (§1.2) | Genuinely large, genuinely proven | Modelica's compiler — §6.1 itemises it | **Refuse**, with a stated escape |
| 4 | **Notation** (§1.1) | Aesthetic; negative for readers | Unbounded ambiguity decisions | **Refuse outright** |

> **Decision 1. The `equation` construct is reading (1) built on the smallest
> possible version of reading (2): a declaration whose body is one expression
> from a closed grammar, from which the compiler generates a callable, a
> dimensional obligation, a typeset rendering and — where asked — a symbolic
> derivative. Readings (3) and (4) are refused, in §6 and §11.3 respectively.**
>
> **Corollary: the input language does not change.** No new operator, no
> Unicode requirement, no new expression syntax outside the equation body. An
> equation is written in the ASCII Science that `syntax-revision-2.md` already
> defines. §11.3 is why this matters more than it sounds.

**Rejected alternative: adopt all four and stage them.** This is how a feature
becomes a subsystem. Reading (3) in particular is not a later stage of reading
(1); it is a different architecture with a different compiler in it, and a
design that leaves room for it leaves room for something it will never build.

**Rejected alternative: adopt (1) only, with no symbolic tree.** The rendering
could be produced by a pretty printer over the ordinary AST with no separate
notion of a symbolic expression. This is cheaper and it is what `latexify_py`
does. It is refused because the same tree is what §5's differentiator walks, and
having two walkers over one tree is fine while having a renderer with no
differentiator means the derivative has to be written by hand — which is the
drift this note exists to remove, one level up.

---

## 2. What the language contributes that a library cannot

This is the test every such proposal has to pass, and this note comes closer to
failing it than its brief expects.

### 2.1 The incumbents, described accurately

| Tool | What it does | Where it checks |
|---|---|---|
| **SymPy** + `lambdify` | Build a symbolic expression, `sympy.latex(expr)` for the paper, `lambdify(expr)` for a callable | Nothing is checked; both outputs derive from one object |
| **`sympy.physics.units`** | `Dimension`, `convert_to`, `SI.get_dimension_system()` | At *evaluation*, on a concrete expression, dynamically |
| **`pint`** | Unit-carrying quantities in Python | At runtime, per operation |
| **`handcalcs`** / **`latexify_py`** | Render Python source to LaTeX, in a notebook | Nothing is checked |
| **`Unitful.jl`** | Units in Julia's type parameters | At runtime, though Julia's specialisation often folds it |
| **`Symbolics.jl` / `ModelingToolkit.jl`** | A full CAS, plus acausal modelling and structural simplification | Structurally, at `structural_simplify` time |
| **F# units of measure** | Dimensional checking in the type system, shipped 2005 | **At compile time** |

Three things follow, and the third is uncomfortable.

**SymPy plus `lambdify` already delivers the one-definition property.** This is
the honest core of the objection. A SymPy user who builds `expr` once, renders it
with `sympy.latex(expr)` and evaluates it with `lambdify(expr)` has a rendered
form and an executed form derived from the same object, and they cannot drift.
Anyone claiming Science invents this is wrong.

**F# proves that the dimensional check is a language feature that works.** It has
existed for twenty years, it is static, and nobody calls it a research project.
`scientific-libraries.md` §12.3 already names it as precedent. So the "only a
language can check dimensions" claim is true, well-precedented, and **not this
note's**.

**The check that matters is already delivered by units plus an ordinary
function**, and pretending otherwise would be the note's worst mistake.

### 2.2 The objection that nearly kills this note

Write the thing without the construct:

```science
## Kinetic energy of a body.
pure def kinetic_energy(m: Mass of F64, v: Velocity of F64) -> Energy of F64:
    0.5 * m * v * v
```

Under `scientific-libraries.md` §12.4, once `const-expression-arithmetic.md`
lands, **this already typechecks, and `0.5 * m * v` already does not.** The
dimensional check on the relation is delivered in full by the return type and
`Quantity`'s `Mul`. §3 of the brief calls dimensional analysis the killer
feature; it is a killer feature, and it belongs to `scientific-libraries.md` §12,
not here.

So: what is left? A `def` plus a derive attribute could carry a typeset
symbol table and a renderer. The README already records *a derive mechanism* as
a standing ask with two customers. Adding a third customer is much cheaper than
adding an item form. **If the answer is "nothing survives", the right outcome is
a `@render` derive and no keyword, and this note should say so and stop.**

Four things survive. Only the first is a difference of kind.

**(a) Totality, which comes from the body grammar being closed.** A `def`
body may contain a loop, a branch, a mutable binding, a call to another function,
an I/O operation, and a `python { … }` block. A renderer over that is *partial*:
it works on the bodies that happen to be expressions and fails on the rest. A
differentiator over that is worse than partial — it is wrong, quietly, on any
body with a branch. An `equation` body is one expression from a fifteen-operator
grammar, so:

> Every body the grammar admits has a rendering and a derivative, **because the
> grammar admits nothing else.**

That is `const-expression-arithmetic.md` §0's argument, verbatim, applied to a
different tree, and it is the house pattern: choose a language small enough that
the procedure over it is total, rather than a procedure that is partial over a
language you did not choose. A partial renderer is not a small version of a total
one; it is a feature that fails on the equation the user cared about, which is
the worst possible failure distribution.

**(b) Typeset symbols and per-variable text have somewhere to live.** A parameter
name is an identifier: `Ea`, not `E_a`; `sigma_obs`, not `\sigma_\mathrm{obs}`.
A paper needs the second. A declaration with a variable block has a place to put
it; a parameter list does not, and a derive attribute that carries a parallel
symbol table has reintroduced a second definition that can disagree with the
first — which is this note's whole subject.

**(c) The relation between rendering and evaluation is checkable.** Because an
equation's only uses are *evaluate* and *render*, "this equation was rendered
into the document and never evaluated in the program that rendered it" is a
decidable question. `SC0249` asks the compile-time half of it. For a function,
"rendered" is not a concept and there is nothing to check.

**(d) Purity is implied by the form**, so `effects.md`'s three inferred bits are
all zero by construction and `pure def` does not have to be written. Small,
free, and it makes `SC0249`'s analysis sound.

> **Decision 2. The language's contribution is not the dimensional check —
> `scientific-libraries.md` §12 already delivers that to an annotated function.
> The contribution is that a closed body grammar makes rendering and
> differentiation *total*, and that the object which is checked, the object which
> is evaluated and the object which is typeset are one declaration with no second
> copy anywhere.**
>
> **The fallback, stated so the project can take it.** If totality is judged not
> worth an item form, the correct smaller design is `pure def` plus a
> `@render` derive, accepting a partial renderer, and `equation` stays reserved
> and unused. This note thinks that is the wrong call and does not think it is an
> unreasonable one.

**Rejected alternative: claim the dimensional check as the contribution.** It
would be the more impressive claim and it is not true. Units are a sibling note's
win and this note is their consumer, not their author.

**Rejected alternative: an `Equation` *type* built in a library, constructed at
runtime from combinators.** That is SymPy, and it is a fine library, and it loses
(a), (c) and (d): the expression is built at run time, so nothing about it is
checked before the run, which is the property the whole language is for.

---

## 3. The construct

### 3.1 The declaration

```science
use physics.constants (GAS_CONSTANT)

## The Arrhenius temperature dependence of a rate constant.
equation arrhenius:
    k:  FirstOrderRate of F64  as "k"    ## the rate constant
    A:  FirstOrderRate of F64  as "A"    ## the pre-exponential factor
    Ea: MolarEnergy of F64     as "E_a"  ## the molar activation energy
    T:  AbsoluteTemperature of F64       ## the temperature, on the kelvin scale

    k = A * exp(-Ea / (GAS_CONSTANT * T))
```

Read top to bottom:

- **`equation arrhenius:`** — a declaration-position item, like `type`. The name
  is the equation's name and it is what a cross-reference resolves to.
- **The variable block** — a field list in the shape `type` already uses, so the
  parser reuses `parse_field_list` and nothing new is learned by the reader. Each
  variable has a type, which is a `Quantity` or an ordinary numeric type; an
  optional typeset symbol after `as`; and an optional `##` doc comment.
- **`as "E_a"`** — the typeset symbol. `as` is already reserved and already in
  §13's *in use* list, so this costs no keyword. Omitted, the symbol is the
  variable's own name, which is right for `k`, `A` and `T` and wrong for `Ea`.
- **The body** — exactly one line, of the form `result = expression`.

`AbsoluteTemperature` is the linear kelvin quantity of `scientific-libraries.md`
§12.3, deliberately *not* the affine Celsius type that `unit-literals.md` §6.1
gives separate treatment. Arrhenius is wrong in Celsius and the type says so.

### 3.2 What the declaration introduces

> **Decision 3. An `equation` item introduces one name, which resolves two ways:
> in call position to the generated function, and in value position to a
> generated constant of type `EquationRecord`. This is one resolver rule and no
> new language machinery.**

```science
# Call position: the generated function, whose parameters are the declared
# variables other than the result, in declaration order.
let k be arrhenius(A: a_fit, Ea: ea_fit, T: 310.15<K>)

# Value position: the record, which is what the document takes.
let eq be doc.equation(arrhenius)
doc.text(f"…fitted to {eq.reference()} over the range 280–340 K…")
```

The generated signature is:

```science
pure def arrhenius(
    A: FirstOrderRate of F64,
    Ea: MolarEnergy of F64,
    T: AbsoluteTemperature of F64,
) -> FirstOrderRate of F64
```

**Rejected alternative: two names, `arrhenius` and `arrhenius_rendering`.** It
works and it is uglier, and the second name is one nobody would guess.

**Rejected alternative: make the equation a value with a call operator
interface.** Cleaner in principle, and it requires the call operator to be an
interface a compiler-generated type can implement, which the core spec's §2 lists
under "operator interfaces" without specifying. Refused as premature: the
resolver rule costs one match arm today and can be replaced by the interface
later without changing a single user-visible spelling.

**Cost.** A name whose meaning depends on position is a wart, and it is the kind
of wart that shows up in a language server's "go to definition". It is one item
form and the two targets are generated from one declaration, so there is nothing
to disagree with.

### 3.3 The body grammar is closed, and that is the whole trick

> **Decision 4. An equation body is `name = expr`, where `expr` is drawn from a
> closed grammar. Nothing outside the grammar is admitted, and the grammar is
> not extensible by users.**

```
body     := ident "=" expr
expr     := expr ("+" | "-") term | term
term     := term ("*" | "/") power | power
power    := atom ("**" int-literal)?          # integer exponents only
atom     := "-" atom
          | "(" expr ")"
          | number                            # including unit and uncertainty literals
          | ident                             # a declared variable, or a resolvable constant
          | elementary "(" expr ("," expr)* ")"
elementary := "exp" | "ln" | "log10" | "log2" | "sqrt" | "abs"
            | "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2"
            | "sinh" | "cosh" | "tanh"
```

What is deliberately absent, with who would have asked for each:

| Absent | Who would want it | Why refused |
|---|---|---|
| Calls to user functions | Anyone factoring a long equation | A user function's body is not from this grammar, so rendering and differentiation stop being total. This is the whole decision. |
| `if` / `match` | Piecewise functions — genuinely common | A branch has no single rendering and no single derivative. §14 open question 2. |
| Loops, sums over an index | `∑ᵢ xᵢ` | The rendering needs a binder and the derivative needs an index calculus. Refused; write a function. |
| Indexing, method calls, field access | Anything vector-valued | The dimensional rule for `a[i]` is fine and the *rendering* is not — see §14 open question 3. |
| Rational exponents | `sqrt` of an odd dimension | `scientific-libraries.md` §12.6 excludes them from `Quantity`. `sqrt` is admitted and is `SC0254` on an odd dimension. |
| `%`, bitwise operators, comparisons | Nobody, in an equation | Not mathematics as a paper prints it. |

**The elementary set is closed and is not a user hook.** Each member has two
facts attached that the grammar could not otherwise know: its dimensional rule
(the transcendentals require a dimensionless argument and return dimensionless;
`sqrt` halves the exponent vector; `abs` preserves it) and its derivative. A user
function has neither, which is why admitting one collapses the design.

**The dimensionless-argument rule is worth stating on its own**, because it is
the second-most-common dimensional error after the unbalanced equation:
`exp(-k * t)` is legal because `s⁻¹ · s` is dimensionless, and `exp(-t)` is not.
`scientific-libraries.md` §5.1's `exp(F64) -> F64` already rejects the second by
ordinary type checking; the equation form contributes only the better message.

### 3.4 `=` is a sublanguage operator, and there is precedent

`=` is not an operator in Science. Binding is `let x be v`, equality is `is`, and
`==` was removed by `syntax-revision-2.md` §1. Inside an equation body, `=`
appears exactly once, at the top level, separating the result from the
expression, and it means neither binding nor equality-as-a-test: it means *is
defined to be*.

This is a closed sublanguage with its own token meaning, and the project has
adopted two already:

- `ffi-c-boundary.md` §1.1 — *"extern block is a small separate grammar with
  contextual keywords"*.
- `unit-literals.md` §3.1 — `^` is the exponent operator inside `< >` and
  bitwise XOR everywhere else, accepted deliberately and argued at length.

The same argument applies with less force, because it is smaller: `=` occurs once
per equation, in a position where nothing else can occur, and every reader of
every paper already reads it as this. **Serves scientists, at no cost to
programmers**, which is the phrase `unit-literals.md` §3.2 uses for the same
trade.

**Rejected alternative: use `be`.** `k be A * exp(…)` is consistent with the
language and reads as a binding, which it is not — an equation is a statement
about a relation, and `be` would make it look like a mutable assignment inside a
declaration.

**Rejected alternative: use `is`.** `k is A * exp(…)` reads as a boolean test
returning `Bool`, which is exactly the confusion §1.1 warns about: notation that
promises one thing and does another.

---

## 4. Dimensional analysis, worked properly

The check itself is `scientific-libraries.md` §12's, and §2.2 concedes that. What
this section owns is **what the check looks like when it fails on an equation**,
which is different from what it looks like when it fails on an expression, and
better in three specific ways.

### 4.1 The obligation, stated

For an equation `r = e` with declared variables `v₁ … vₙ`:

1. Every name in `e` resolves to a declared variable or to a module-level
   constant of numeric or `Quantity` type (`SC0246` otherwise).
2. `r` does not occur in `e` (`SC0247`).
3. Every `vᵢ` other than `r` occurs at least once in `e` (`SC0247`).
4. Every typeset symbol is distinct (`SC0248`).
5. `dim(e)`, computed by `Quantity`'s operator rules, equals `dim(r)`
   (`SC0256`, amended).

Obligations 1–4 are resolution-phase and are this note's. Obligation 5 is the
type checker's and uses `const-expression-arithmetic.md`'s `NORMALISE` and
`EQUAL` unchanged.

### 4.2 The residual dimension names the missing factor

An expression-level mismatch — `thrust + burn` in `unit-literals.md` §10.1 — has
no declared answer to compare against, so the best diagnostic can do is print
both dimensions and let the user work out the difference. An equation *has* a
declared result, so the compiler can compute the quotient

```
residual = dim(r) − dim(e)        # exponent-wise subtraction
```

and say what the two sides differ **by**. That is a more useful sentence than
"these do not match", and it is only available because the equation declared its
result.

Better still: because the variable set is closed and small, the compiler can
search it. If exactly one declared variable has dimension `residual`, the help
can name it. If one has dimension `residual / n` for a small integer `n`, the
help can name the power. `E = m * c` has `residual = L¹T⁻¹`, which is exactly
`c`, and the help writes itself.

> **Decision 5. The equation case is an *amendment* to `SC0256`, which
> `unit-literals.md` owns, not a new code. The amendment adds the residual line,
> the derivation table of `const-expression-arithmetic.md` §9.3, and the
> missing-factor help.**

Reasons: the condition is identical (two dimensions that must be equal and are
not), the rendering machinery is identical, and the README records five `SC`
collisions in one day from notes claiming codes they could have amended. Adding
cases to a code another note owns is explicitly permitted by the README's
conventions and is what `publishable-output.md` did with `SC0274`.

### 4.3 `E = m * c`, rendered in full

```science
## Mass–energy equivalence.
equation mass_energy:
    E: Energy of F64            ## the rest energy
    m: Mass of F64              ## the rest mass
    c: Velocity of F64          ## the speed of light in vacuum

    E = m * c
```

```
error[SC0256]: the two sides of this equation have different dimensions
  --> relativity.science:7:9
   |
 3 |     E: Energy of F64
   |        ------------- the result is declared as Energy — kg·m²/s²
 4 |     m: Mass of F64
 5 |     c: Velocity of F64
 6 |
 7 |     E = m * c
   |     -   ^^^^^ this has the dimensions of momentum — kg·m/s
   |     |
   |     Energy — kg·m²/s²
   |
   = the two dimensions differ in two exponents:

         base            length   mass   time   current   temp   amount   lum
         E  (declared)        2      1     -2         0      0        0     0
         m * c  (body)        1      1     -1         0      0        0     0
                            ----   ----   ----
         difference           1      0     -1                     →  m/s

   = the right-hand side is short by one factor of a velocity — m/s

help: `c` is a velocity. Multiplying by it once more balances the equation:

 7 |     E = m * c ** 2
   |               ++++
```

Three things in that message are the equation form's and are not available to the
expression-level `SC0256`:

1. **The declared side is labelled `(declared)`.** There is a right answer and
   the compiler knows which side is claimed and which side is computed. In
   `thrust + burn` neither side is privileged.
2. **The `difference` row, reduced to a unit string.** `L¹T⁻¹` is printed as
   `m/s`, using `unit-literals.md` §7.2's `unit_symbol` run at diagnostic time —
   the same code path §10.1 of that note already requires.
3. **The applicable fix**, because the search over three declared variables for
   one of dimension `m/s` has exactly one hit. Where it has none, or more than
   one, the help is dropped and the residual line stays. The rule is the one
   `llm-ergonomics.md` uses throughout: an applicable fix is offered only when it
   is unique.

Where the body is a product of several factors, the derivation table of
`const-expression-arithmetic.md` §9.3 — one row per contributing operand with its
span — is emitted unchanged. That note already requires terms to carry provenance
spans; this note is its second customer for that field and asks for nothing new.

### 4.4 What this does not tell anyone, said before someone assumes it

`statistical-validity.md` §5.2 puts it in one sentence that this note adopts
rather than restates:

> dimensional analysis never told anyone their equation was true, only that it
> was not nonsense.

An equation that balances can still be the wrong equation, can have the wrong
sign, the wrong coefficient, the wrong exponent on a dimensionless group, and can
be a correct equation applied outside its validity range. `E = m * c ** 2` and
`E = 2 * m * c ** 2` both pass. The check turns an undeclared assumption into a
declared one and then checks the declaration mechanically, which is the standard
§5.2 of that note argues every check in this language should be held to, and it
is not a claim about physics.

**This must appear in the generated output too, not only here.**
`publishable-output.md` §7.3 item 2 marks every generated block; the equation
block's marking should not imply verification. §7.6 below says what it does say.

---

## 5. Differentiation, and the F2 overlap

### 5.1 Symbolic differentiation over a closed grammar is total, and small

`∂e/∂v` for `e` in §3.3's grammar is one recursive rewrite: sum and product rules,
quotient rule, chain rule, a table of fifteen elementary derivatives, and a
constant-folding pass that removes the `0 *` and `1 *` terms the rules generate.
It is a few hundred lines, it terminates, and it is exact.

It is *not* a CAS. There is no `simplify`, so the output is verbose: the
derivative of `A * exp(-Ea / (R * T))` with respect to `T` comes out as

```
A * exp(-Ea / (R * T)) * (Ea / (R * T ** 2))
```

rather than `k * Ea / (R * T ** 2)`, and the second is what a human would write.
The constant-folding pass gets the trivial cases and nothing more. **This is a
real quality gap and it is the honest boundary of refusing a CAS**: a rendered
derivative will sometimes be uglier than the one in the textbook. §14 open
question 1.

> **Decision 6. Symbolic differentiation is part of the construct, restricted to
> first partial derivatives with respect to declared variables, with folding but
> no simplification. Higher derivatives compose by re-entry and are not
> specially handled. No integration, no solving, no series.**

**Rejected alternative: ship a simplifier.** Term rewriting with a normal form
over this algebra is not a small problem — it is where a CAS's complexity lives,
and a half-finished one produces output that is *differently* ugly from run to
run, which is worse than consistently verbose.

### 5.2 Three consumers already in the catalogue

The derivative is not a demo. Three callers exist in sibling notes today.

**`optimize` §8.6 has a hole shaped exactly like this.** Its signature reads:

```science
gradient: ((borrowed Vector of (F64, N)) -> Vector of (F64, N))?,
```

with the comment *"given one, lbfgs uses it; without one it falls back to finite
differences"*. An objective written as an equation can fill that nullable slot at
compile time, with an exact gradient, with no AD and no hand-differentiation.
`levenberg_marquardt` and `gauss_newton` in §8.3 want the same thing and want it
more, because a least-squares Jacobian computed by finite differences is the
usual reason a fit fails to converge.

**`math` §5.4's stiff solvers want an analytic Jacobian.** `bdf`, `radau_iia`,
`rosenbrock` and `trbdf2` all form and factor a Jacobian at every step. An ODE
right-hand side written as an equation supplies it symbolically. This is the same
observation that made Julia's `DifferentialEquations.jl` plus ModelingToolkit
combination valuable, arrived at from a much smaller design.

**`uncertainty.md` §3 wants exactly the partial derivatives.** That note makes an
`Uncertain of T` carry a gradient over independent sources, and arithmetic merges
gradients so correlation is exact. For an equation, the merge coefficients *are*
`∂e/∂vᵢ` evaluated at the operating point. So an equation evaluated on uncertain
inputs propagates exactly, with coefficients the compiler derived rather than the
runtime accumulated — and, more useful for a paper, **the propagation formula can
be printed**:

```
u(k)² = (∂k/∂A)² u(A)² + (∂k/∂Ea)² u(Ea)² + (∂k/∂T)² u(T)²
```

with each partial rendered in LaTeX. That expression is the thing every
undergraduate laboratory manual asks students to derive by hand and that almost
no paper prints, and it is a byproduct here rather than a feature.

### 5.3 The relation to F2's automatic differentiation

§2 of the core spec puts automatic differentiation in F2. The two are
**orthogonal and complementary, and the equation construct should not wait**.

| | Symbolic (this note) | Automatic (F2) |
|---|---|---|
| Input | one expression from a closed grammar | arbitrary program text, loops and branches included |
| Output | a **formula**, renderable | a **number**, or a transformed program |
| When | compile time | run time, or compile time as a transformation |
| Scales to | a textbook equation | a neural network |
| Fails on | anything with a branch | nothing in its domain |

They meet in exactly two places and neither is a conflict:

- **The equation is a source, not a consumer.** It produces derivatives; AD
  consumes programs. An equation compiled to a function is an ordinary function
  that F2's AD can differentiate like any other, and it will get the same answer
  by a different route.
- **Which is a free test oracle.** Differentiating one equation both ways and
  comparing is a property test for the AD implementation over a class of inputs
  with known exact answers. That is genuinely useful to F2 and costs nothing.

> **Decision 7. The equation construct is F1, not F2, and does not wait for
> automatic differentiation. Its gate is `const-expression-arithmetic.md` and
> the units library, because without dimensional checking there is no reason to
> prefer it to a library.**

**Rejected alternative: wait for F2 and build it on AD.** AD gives numbers, not
formulas. The rendered derivative — §5.2's third consumer, and the whole of §7's
closure — is unreachable from AD, so building on it would deliver the weaker half
two phases late.

**Rejected alternative: ship in F0 without units.** An equation with no
dimensional obligation is `latexify_py`, which is a library, which exists, and
which nobody should adopt a new language for.

---

## 6. Solving

### 6.1 What acausal modelling actually costs

Modelica's proposition is that you write `f = m * a` and the tool figures out
what to solve for. Getting from a set of such relations to something executable
is, in order:

1. **Flattening** — inline the component hierarchy into one flat set of
   equations and variables.
2. **Structural analysis** — build the bipartite incidence graph of equations
   against unknowns, find a perfect matching, and report failure when there is
   none. "There is none" is the famous *the model is structurally singular*.
3. **Index reduction** — the Pantelides algorithm, to bring a high-index DAE down
   to index 1 by symbolically differentiating selected equations, which requires
   the symbolic differentiator §5 already has and then requires deciding *which*
   equations.
4. **Dummy derivatives** — Mattsson and Söderlind's method for choosing state
   variables after index reduction, because the naive choice makes the system
   drift.
5. **BLT decomposition** — Tarjan's strongly-connected-components algorithm over
   the matched graph, producing a block lower triangular ordering, so that the
   system is solved as a sequence of small blocks rather than one large one.
6. **Tearing** — inside each irreducible block, choose tearing variables to
   reduce the size of the nonlinear system, which is NP-hard in general and is
   done by heuristics that differ between implementations.
7. **A nonlinear solver per algebraic block**, at every step.
8. **A DAE integrator** — DASSL, IDA — with event detection and consistent
   re-initialisation after each event.

**That is a compiler.** It is not a feature inside a compiler; it is a second
compiler, with its own intermediate representations, its own correctness
questions and its own decades of literature. OpenModelica is roughly a
million lines. Steps 5–8 duplicate `scientific-libraries.md` §5.4's `solve_dae`
and §8.5's `fsolve` and `newton_krylov`, which that note already schedules as
library work.

### 6.2 The diagnostics argument, which is decisive on its own

Even at zero implementation cost, acausal modelling would be wrong for this
language, because **its errors are structural and structural errors are the
hardest kind to explain.** The canonical Modelica messages are of the form
*"the model is structurally singular"* and *"the number of equations (47) does
not match the number of unknowns (46)"*, and the reason a user's model is
singular is typically an interaction between two components neither of whose
authors made a mistake.

`llm-ergonomics.md`'s governing observation is that diagnostics are the only
teaching channel a language with no training corpus has. A feature whose primary
failure mode is a global property of a system, reported with no span that can be
blamed, is the opposite of that. `broadcasting.md` §5.4 made the same call for a
smaller case, refusing symbolic-against-symbolic broadcasting on the grounds that
the resulting message could only say *cannot prove* and never *incompatible*.

And `binding-inventory.md` §'s reading of Julia is directly on point: *"where
Julia's native inventory was best in the world — `DifferentialEquations.jl`,
`JuMP` — adoption happened. That is evidence for inventory and against interop."*
Those two are **libraries**. The acausal layer on top of the first, ModelingToolkit,
is also a library. The evidence for acausal modelling is evidence for building it
as a library, in a language that already has one.

> **Decision 8. Science does not attempt acausal equations. An equation has one
> stated result and it is the name on the left of `=`. If acausal modelling is
> ever wanted, it is a library over the `EquationRecord` type this note defines,
> written by somebody who wants it, and this note's job is to make sure the
> record has enough in it to make that possible — which it does, because the
> record carries the expression tree.**

**Rejected alternative: acausal, restricted to linear systems.** Tempting,
because the structural analysis of a purely linear system is just Gaussian
elimination and the diagnostics are tractable. Refused because the restriction is
invisible to the user until it is violated, at which point they are told their
model is outside a class they were never told about — and because the useful
models are nonlinear, which is why Modelica does the hard version.

### 6.3 What a causal equation still buys

Everything in §4, §5 and §7: the dimensional check, the generated callable, the
symbolic derivative, the propagation formula, the LaTeX, the cross-reference, the
never-evaluated check.

What is lost is precisely this: having written `f = m * a`, you cannot ask for
`a`. You write a second equation, and **the compiler cannot check that the two
agree.** That is a real loss and it is exactly the drift this note exists to
remove, reappearing one level up. Stating it plainly is better than pretending
§6.4 closes it, because §6.4 closes only part of it.

### 6.4 The one inversion that is free: linear in the unknown

A large fraction of the equations in a textbook are *linear in each variable they
might be solved for*. `f = m * a` is linear in `a` and in `m`. `E = m * c ** 2`
is linear in `m`. `C = C0 * exp(-k * t)` is linear in `C0`. Inverting a
linear-in-the-unknown equation is algebra a first-year student does, it needs no
solver, and — crucially — **whether the body is linear in a given variable is
decidable by inspection of the grammar**: the variable occurs exactly once, and
every operator on the path from it to the root is `+`, `-`, `*` by a subtree not
containing it, `/` with the variable in the numerator, or unary `-`.

```science
equation newton_second:
    f: Force of F64         as "F"
    m: Mass of F64          as "m"
    a: Acceleration of F64  as "a"

    f = m * a

# The forward direction, generated always.
let force be newton_second(m: mass, a: accel)

# The inverse, generated on request, because `f = m * a` is linear in `a`.
let accel be newton_second.solving_for_a(f: force, m: mass)
```

> **Decision 9. Inversion is offered only where the body is syntactically linear
> in the requested variable, by the single-occurrence test above. Everything else
> is refused with a message that names the restriction and says to write a second
> equation. The inverse is generated on request, not for every variable, because
> generating six functions from one declaration is an API nobody asked for.**

This is the same shape as `const-expression-arithmetic.md` §4.2's treatment of
division — *"the fold is whole-form only, and the obvious improvement is
unsound"* — and it is the same trade: a small total fragment in preference to a
large partial one. The user who is refused is refused by a rule they can check
themselves in two seconds.

**Cost.** A user whose equation is quadratic in the unknown — and the quadratic
formula is not hard — will find the restriction arbitrary. It is arbitrary in the
sense that the line could be drawn elsewhere; it is not arbitrary in the sense
that every place further out requires a real solver.

**This decision is optional and should land after the core.** Nothing in §4, §5
or §7 depends on it. If it is cut, the note loses one convenience and no
guarantee.

---

## 7. The LaTeX closure

This is the section that connects to the owner's stated goal, so it is written
concretely enough to picture.

### 7.1 What is generated, precisely

From the `arrhenius` declaration of §3.1, the compiler produces:

**The body, as LaTeX**, by a precedence-aware printer over the expression tree.
Multiplication is juxtaposition or `\cdot` by a rule (juxtaposition between a
symbol and a symbol, `\cdot` between a number and a symbol); division becomes
`\frac`; `**` becomes `^{…}`; `exp(x)` becomes `e^{x}` when `x` is short and
`\exp\left(x\right)` when it is not; the elementary functions become their
`\mathrm` or standard control sequences.

```latex
k = A \exp\!\left(\frac{-E_a}{R T}\right)
```

**The symbol table**, from the `as` annotations and the variable names, with the
constants' symbols coming from the constants table rather than from the equation
(§12 item 3).

**The unit statement**, from the types, which no other tool can produce because
no other tool has the types:

```latex
where $k$ is in $\mathrm{s^{-1}}$, $A$ in $\mathrm{s^{-1}}$,
$E_a$ in $\mathrm{J\,mol^{-1}}$ and $T$ in $\mathrm{K}$.
```

**The variable glossary**, from the `##` doc comments — *the pre-exponential
factor*, *the molar activation energy*. This is the second customer of
`publishable-output.md` §12 item 2, which asks
`strings-formatting-and-docs.md` §5.3's doc index to reach **fields**. An
equation's variables are fields, so the ask is identical and needs no widening.

**On request, the derivative and the propagation formula** of §5.2, rendered the
same way.

The Typst rendering is the same walk with a different printer, and the Markdown
rendering emits the LaTeX inside `$$` because every Markdown consumer that
matters renders mathematics that way.

### 7.2 The document-record block

`publishable-output.md` Decision 2 makes the document record the single source
every backend renders from, and Decision 2b requires that no field of any record
be a JSON float. An equation block satisfies both trivially, because it contains
no numbers at all — it contains symbols.

```json
{ "kind": "equation",
  "name": "arrhenius",
  "latex": "k = A \\exp\\!\\left(\\frac{-E_a}{R T}\\right)",
  "typst": "k = A exp(-E_a / (R T))",
  "variables": [
    { "name": "k",  "symbol": "k",   "unit": "s⁻¹",      "role": "result",
      "doc": "the rate constant" },
    { "name": "A",  "symbol": "A",   "unit": "s⁻¹",      "role": "input",
      "doc": "the pre-exponential factor" },
    { "name": "Ea", "symbol": "E_a", "unit": "J/mol",    "role": "input",
      "doc": "the molar activation energy" },
    { "name": "T",  "symbol": "T",   "unit": "K",        "role": "input",
      "doc": "the temperature, on the kelvin scale" }
  ],
  "constants": [
    { "name": "GAS_CONSTANT", "symbol": "R", "unit": "J/(mol·K)", "exact": true } ],
  "generated": true,
  "evaluations": 1 }
```

`"generated": true` is Decision 12's marking, unchanged. `"exact": true` on the
gas constant is `publishable-output.md` §12 item 5b's ask — since the 2019 SI
redefinition `R` is exact by definition, and a footnote should be able to say
why it carries no uncertainty.

`"evaluations"` is the count of distinct call sites the compiler saw. Zero means
the equation was typeset and never used, which is §7.4 of `publishable-output.md`'s
failure class in its purest form, and is the render-time half of `SC0249`.

> **Decision 10. An equation is a block kind in `publishable-output.md`'s document
> record, carrying its LaTeX, its Typst, its variable glossary with units, and
> its evaluation count. It carries no numbers, so Decision 2b's no-floats rule
> costs nothing here.**

### 7.3 How it reaches the author's own manuscript

`publishable-output.md` Decision 13 makes the primary deliverable an asset bundle
the author `\input`s into their own journal class, rather than a generated paper.
The equation follows that decision exactly:

```latex
% equations/arrhenius.tex — generated by sciencec report, do not edit
\begin{equation}\label{eq:arrhenius}
k = A \exp\!\left(\frac{-E_a}{R T}\right)
\end{equation}
```

and the author writes, in their own class, in their own editor:

```latex
Rate constants were modelled with the Arrhenius relation, \Cref{eq:arrhenius},
fitted by weighted least squares to \TreatedN{} determinations.
\input{equations/arrhenius.tex}
```

with `\TreatedN` coming from the same bundle's macro file. The cross-reference is
the sibling's Decision 6 mechanism unchanged: `arrhenius` is a binding, so
`doc.equation(arrhenius)` on an equation that has been deleted or renamed is
`SC0201`, an unresolved name, before the compute — not `??` in a PDF after it.

**This is the closure, stated as one chain.** The equation that ran is the
equation that was checked is the equation that was typeset; the numbers around it
came from the same program by `publishable-output.md` §6's rule; and the appendix
carries the `pv:` id of the run that produced both. Nothing in the chain has a
copy-paste boundary in it.

### 7.4 No round trip, and why refusing one is the right call

> **Decision 11. Generation is one-way. Science does not parse LaTeX and will not
> reconstruct an equation from a manuscript.**

Three reasons, in increasing order of weight:

1. **LaTeX is presentation, not semantics.** `\frac{dy}{dx}` is not a quotient,
   `\left(` is not a parenthesis with meaning, `\,` is a space, `\mathrm{d}` is a
   differential or an identifier depending on who typed it, and any real
   manuscript contains author-defined macros. Parsing it is a research problem
   and the research is not settled.
2. **A parser would be partial and would fail silently.** It would succeed on the
   simple half of a manuscript and produce a subtly wrong tree on the other half,
   which is the worst failure distribution available.
3. **A round trip creates a second source**, which is the thing the note exists
   to eliminate. If a manuscript's LaTeX can become an equation and an equation
   can become LaTeX, then there are two definitions again and one of them is
   authoritative on Tuesdays.

**The migration path is a paste, done once.** An author with an existing
manuscript writes the equation in Science, runs `sciencec report`, and replaces
their hand-written display with an `\input` line. That is one manual step per
equation, performed once, and after it the drift is gone. It is not elegant and
it is honest.

### 7.5 The numerical method with no closed form

This is the case that decides whether §7 is useful or decorative, because most
real analyses end in `rk45_dormand_prince`, `lbfgs` or `levenberg_marquardt`, and
none of those has a formula.

The answer is a split that is already in the design:

> **The equation renders the *model*. The analysis record reports the *method*.**

A first-order decay fitted to data has both, and they are different artefacts:

```science
## First-order decay of a concentration.
equation first_order_decay:
    C:  Concentration of F64   as "C"
    C0: Concentration of F64   as "C_0"  ## the concentration at t = 0
    k:  FirstOrderRate of F64  as "k"    ## the decay constant
    t:  Time of F64            as "t"

    C = C0 * exp(-k * t)
```

The model renders as `C = C_0 e^{-k t}`, which is what belongs in the paper.
The method — Levenberg–Marquardt, its tolerance, its iteration count, its
convergence flag, the number of points and the number rejected — belongs in
`publishable-output.md` §7.1's analysis record, which already exists and already
has a place for it. The fitted value of `k` with its uncertainty belongs in a
typed table by §6 of that note. **Three artefacts, three mechanisms, one record**,
and no mechanism claims something it does not know.

Where an analysis genuinely has no closed form anywhere — a neural surrogate, a
tabulated equation of state, an iterative scheme with no analytic statement — the
equation construct is simply not used, the paper describes the method in prose as
it does today, and nothing is generated and nothing is claimed. A construct that
declines to appear is better than one that renders a formula nobody wrote.

**An ODE is the interesting middle case and it works.** `dC/dt = -k * C` *is* a
closed form and is what a paper prints; what has no closed form is the
*trajectory*. So the right-hand side is an equation, the solver is a methods
record line, and the rendering says `\frac{\mathrm{d}C}{\mathrm{d}t} = -k C`.
That requires the result position to admit a derivative spelling, which §14 open
question 4 leaves open — the cheapest version being a result variable literally
named `dC_dt` with `as "\frac{\mathrm{d}C}{\mathrm{d}t}"`, which works today and
is ugly.

### 7.6 What the generated block must not say

`publishable-output.md` Decision 12's four refused categories apply unchanged, and
§4.4 adds one that is specific to equations:

**The block must not imply that the equation was verified.** It was checked for
dimensional consistency, which is a much weaker statement, and the appendix
should say which. The wording that is true:

> Equations in this document were generated from the expressions evaluated by the
> program that produced it and were checked for dimensional consistency. This is
> not a check of correctness.

That sentence is mandatory and non-suppressible, following
`statistical-validity.md` §6.3's precedent for exactly the same hazard: a tool
that reports a check and stops produces a halo, and a tool that states its own
limits does not.

---

## 8. What to reserve, now

The method is `reserved-words.md` §1's: classify every candidate by the tightest
position the scientific vocabulary needs it in, and remember that the dot rule
rescues member position and nothing else.

### 8.1 `equation` — already reserved, and this note ratifies it

`equation` is in §13's *reserved, not yet used* list and in
`crates/science-lexer/src/token.rs` as `ReservedWord::Equation`. Checked against
all three of §13's lists and against the catalogue:

- **Not in the in-use list**, so nothing in F0 breaks.
- **Not in the deliberately-not-reserved list**, whose criterion is *a common
  variable name in the target audience's code*. `equation` is not one. The noun a
  scientist binds is the *specific* equation's name — `arrhenius`,
  `michaelis_menten`, `henderson_hasselbalch` — never the word `equation` itself.
- **No collision in the catalogue.** `scientific-libraries.md` §10.4 has
  `hill_equation`, which is a single identifier and is unaffected; §5.4's
  *ordinary differential equations* is a section heading, and its names are
  `solve_ode`, `OdeProblem`, `solve_dae`. A search of every design note found no
  binding, parameter, field or module named `equation`.
- **Declaration position only.** Per `reserved-words.md` §3.2's pricing, that
  means it can be made contextual later at the cost of one match arm and one
  error-path heuristic, if it ever needs to be.

> **Decision 12. Reserve `equation`, which is already done. Reserve nothing
> else.**

### 8.2 `formula` — refused, with the evidence

`formula` collides head-on with chemistry, and the collision is in the two
positions the dot rule does not save.

From `scientific-libraries.md` §10.1, verbatim:

> `Element` `Isotope` `PeriodicTable` `Formula` `Compound` … `parse_formula`
> `format_formula` `molar_mass` `mass_fractions` `empirical_formula`
> `molecular_formula`

and from §10.7's signatures, verbatim:

```science
def molar_mass(formula: borrowed Formula) -> Quantity of (F64, GramsPerMole)
```

That is `formula` in **parameter position**, which `reserved-words.md` §0.2 lists
among the cases the dot rule explicitly does not fix, alongside `let model be …`
and `def forward(tensor: Tensor)`. Reserving it would mean:

- `Formula` the type survives — types are capitalised and were never at risk,
  which is that note's §1 observation.
- `parse_formula`, `format_formula`, `empirical_formula` and `molecular_formula`
  survive — they are single identifiers containing the word, not the word.
- **`formula: borrowed Formula` does not compile**, and neither does
  `let formula be parse_formula("C6H12O6")`, which is the first line a chemist
  writes.

`scientific-libraries.md` §10.2 already carries the cost of one such reservation
and calls it out as the worst in the catalogue:

> `theoretical_produced` and `percent_produced` are the names §3 forces: `yield`
> is reserved. This is the single most-visible cost of that reservation anywhere
> in the catalogue — *yield* is the word the entire discipline uses.

`reserved-words.md` §2.2 then recommends freeing `yield` for exactly that reason.
Reserving `formula` would create a second instance of the same mistake in the same
module, on the same day the project decided to stop making it.

> **`formula` is not reserved, must not be reserved, and the naming decision the
> brief records — the word is `equation` — is correct and is confirmed by the
> catalogue.**

### 8.3 Seven other candidates, checked and rejected

| Word | Wanted by | Position | Verdict |
|---|---|---|---|
| `symbol` | `chem` §10.1 `element_by_symbol`; `unit-literals.md` §7.2 `unit_symbol`; `bio` sequence code | Binding, parameter | **No.** The typeset symbol is given by `as "…"`, which needs no word. |
| `variable` | Every field, everywhere | Binding | **No.** The variable block needs no introducer; the indentation does the work. |
| `derivative` | `math` §5.3 `derivative`, `derivative_order`; `Polynomial` §5.6 `derivative` | Free function, member | **No**, emphatically. It is a shipped function name in two modules. The derivative is requested through a member on the record. |
| `solve` | `solve_ode`, `solve_bvp`, `solve_dae`, `solve_kinetics`, `linalg` §6.4 | Free function | **No.** §6.4 spells inversion `solving_for_a`, which is a member name and is covered by the dot rule. |
| `latex` | nothing | — | **No.** A rendering backend is a library concern; `publishable-output.md` §8.2 names three and reserves no word for any of them. |
| `relation` | `stats`, and data-frame vocabulary | Binding | **No.** §6's Decision 8 refuses acausal equations, so there is nothing to name. |
| `equations` (plural) | a system of them | Declaration | **No.** A system of equations is the acausal reading, which is refused. Reserving the plural would reserve room for something the project has decided not to build. |

`where` and `as` are already reserved and in use (§13), so §3.1's `as "E_a"` and
any future `where` clause on an equation cost nothing.

### 8.4 What this note deliberately does not want, in support of a sibling

`reserved-words.md` §3.3 recommends **freeing** `shape`, `model` and `tensor`,
and §6 records that `broadcasting.md` §11.7 removed the last hypothesis that
justified keeping them. It is worth saying explicitly, because a note about
mathematical constructs is where someone would look for a counter-argument:

**The equation construct wants none of those three words.** It does not want
`model` (the model is the equation, and the word for it is the equation's own
name), it does not want `shape`, and it does not want `tensor`. This note
therefore adds no new reason to keep any of them reserved and one small reason to
free them: a `let model be fit(…)` beside an `equation` declaration is precisely
the code a user of this feature writes.

---

## 9. Diagnostics

This note claims **`SC0246`–`SC0249`** in the resolution range. The README's free
list records `SC0237`–`SC0249` as free, and its claimed table separately records
`reproducibility.md` holding `SC0237`–`SC0240`; **those two rows contradict each
other**, which is worth fixing but does not affect this claim, because
`SC0246`–`SC0249` is free under either reading. All four are resolution-phase:
they are about names, occurrences and reachability, not about types.

| Code | Phase | Condition | Applicable fix |
|---|---|---|---|
| `SC0246` | Resolution | A name in an equation body is neither a declared variable of that equation nor a resolvable constant of numeric or `Quantity` type | **Yes** when a single near-match exists among the declared variables |
| `SC0247` | Resolution | A declared variable stands in a relation to the body an equation does not allow: the result occurs on the right-hand side, or an input occurs nowhere | **Yes** for the second case — delete the variable, or use it |
| `SC0248` | Resolution | Two variables of one equation render to the same typeset symbol, so the typeset form is ambiguous | **Yes** — name one of them with `as` |
| `SC0249` | Resolution (warning) | An equation is never evaluated anywhere in the program | none; the note says the rendered form is untested by the run |

Three amendments and one request, none of them a claim:

- **`SC0256`** (types; owned by `unit-literals.md`) gains the equation case of
  §4.2 — the `(declared)` label, the `difference` row reduced to a unit string,
  and the missing-factor help. §4.3 renders it in full.
- **`SC0254`** (types; owned by `unit-literals.md`) — `sqrt` of an odd dimension
  — fires unchanged from an equation body. No text change is needed; naming it
  here so the interaction is visible.
- **`SC0201`** (resolution; unresolved name) is what a dangling cross-reference
  produces, per `publishable-output.md` Decision 6. Unchanged.
- **A request, not a claim**: one code from `publishable-output.md`'s reserved
  `SR0008`–`SR0099` for *an equation block in the document record whose
  `evaluations` count is zero*. That is the render-time half of `SC0249` and it
  belongs in that note's namespace because it is found by a tool reading an
  artefact.

### 9.1 `SC0249`, rendered

This is the one that is unusual enough to need showing, because it is the check
that makes §7.3's closure a closure rather than a convention.

```
warning[SC0249]: this equation is never evaluated
  --> kinetics.science:12:1
   |
12 | equation arrhenius:
   | ^^^^^^^^^^^^^^^^^^ declared here, and typeset below
   |
  --> kinetics.science:61:14
   |
61 |     doc.equation(arrhenius)
   |         -------- placed in the document here
   |
   = the program typesets this equation and never calls it, so the expression
     in the paper is not the expression that produced the paper's numbers
   = if the fit is performed by `curve_fit` with a hand-written closure, the
     closure and this equation are two definitions and nothing checks that
     they agree

help: evaluate the equation, or pass it to `curve_fit`:

61 |     let fit, err be curve_fit(arrhenius, temperatures, rates)
```

The message is long because the failure is subtle and the corrected program is
not obvious. `llm-ergonomics.md`'s rule applies: the diagnostic is the only
teaching channel, and the thing being taught here is the design's central claim.

---

## 10. Where this lives, and what lands when

**F0 — nothing but the reservation, which is already done.** `equation` is
reserved in §13 and in the lexer. No parser work, no AST node, no diagnostic.
The word costs nothing and cannot be obtained later.

**F1 — the construct, gated on two things that are not this note's.**

| # | Requires | Owner | Status |
|---|---|---|---|
| 1 | Const-expression arithmetic in type position | `const-expression-arithmetic.md` §10.1 | Committed for F0 |
| 2 | `Quantity` and the units library | `scientific-libraries.md` §12 | F1 |
| 3 | The doc index reaching **fields** | `strings-formatting-and-docs.md` §5.3, asked by `publishable-output.md` §12 item 2 | Asked; this note is the second customer |
| 4 | The `report` library and the document record | `publishable-output.md` §9 | F1, Level 3 delivered as Level 2 |

The construct itself, priced honestly:

| Piece | Cost |
|---|---|
| Parser: one item form reusing the field-list parser, plus the body grammar | ~250 lines, one AST node, one snapshot sweep |
| Resolution: the two-way name rule (§3.2) and `SC0246`–`SC0249` | ~300 lines |
| Types: the dimensional obligation, which is `EQUAL` over the existing normal form | ~50 lines; the work is already done by item 1 |
| Codegen: the generated function, which is an ordinary `pure def` body | near zero |
| Renderer: the LaTeX and Typst printers | ~400 lines, in the `report` library, not the compiler |
| Differentiator: the rewrite, the derivative table, the folding pass | ~350 lines |
| `SC0256`'s amendment | ~150 lines, reusing §9.3's derivation block |

Roughly **1 500 lines and no new subsystem**, which is the number that makes the
feature affordable and which is affordable only because §6 refused the acausal
reading and §5.1 refused the simplifier.

**F1+, optional and separable — §6.4's linear inversion.** Nothing depends on it.

**Never — the four things that would make this a subsystem**: a simplifier,
integration, acausal solving, and LaTeX parsing.

---

## 11. The honest verdict

### 11.1 The strongest case against, first

Five arguments, in increasing order of how much trouble they cause.

1. **SymPy plus `lambdify` already gives one definition, two outputs.** §2.1
   concedes it. A Python user who is disciplined gets most of this today, in a
   language with an ecosystem.
2. **Fortress is dead and Mathematica is proprietary**, so the two languages that
   made mathematics a language-level concern are one failure and one walled
   garden. That is not an encouraging base rate.
3. **`binding-inventory.md`'s reading of Julia is evidence against language
   constructs generally.** Where Julia won, it won on *inventory* —
   `DifferentialEquations.jl`, `JuMP` — and those are libraries. ModelingToolkit,
   which is the closest existing thing to this proposal, is a library too.
4. **Every hour here is an hour not spent on const-expression arithmetic**, which
   four notes are already blocked on and which this note is the fifth caller of.
   A feature that depends on the bottleneck should not compete with it for
   attention.
5. **The sharpest one: this may be a function with metadata**, and the right way
   to attach metadata is the derive mechanism the README already records as a
   standing ask with customers in `data-io.md` and
   `strings-formatting-and-docs.md`. Adding a third customer to a mechanism that
   is being built anyway is enormously cheaper than adding an item form to the
   grammar. **If this argument holds, the note's conclusion is wrong.**

### 11.2 The answer, which rests on one thing

Arguments 1–4 are answered by degree and argument 5 is answered by kind.

To 1: yes, and the discipline is the point. Python's version is a convention a
user can break at any time by computing with a hand-written lambda while
rendering a symbolic expression, and nothing in the language notices. Science's
version has `SC0249`, which notices.

To 2: §11.3.

To 3: correct, and it is why §6 refuses the part of the proposal that is
genuinely library-shaped. What remains is not library-shaped for the reason in
§2.2(a).

To 4: this note asks for nothing new from const-expression arithmetic and adds a
fifth argument for building it. It is F1 work that starts after that lands.

To 5, which is the only one that matters: **a derive over a `def` produces a
partial renderer, and a partial renderer fails on the equation the user cares
about.** A `def` body may contain a loop, a branch, a foreign block or a
call to a function in another package, and a renderer over it either refuses
those — in which case the restriction exists but is discovered at the end rather
than declared at the start — or renders them wrongly. The closed grammar is not
decoration on the feature; it *is* the feature, and it cannot be expressed as an
attribute on a construct whose body is arbitrary.

That is a single load-bearing argument and the note rests on it. If the project
judges that a renderer which works on most function bodies is good enough, the
`@render` derive is the right answer, `equation` stays reserved and unused, and
nothing is lost but the guarantee.

### 11.3 Why this is not Fortress

Fortress's bet was that **the source should look like mathematics**. Its headline
was a rendering of the program text, its operators were Unicode, and its
multiplication was juxtaposition. That bet requires the language to resolve every
ambiguity in mathematical notation, requires the reader to have the renderer, and
puts the aesthetic claim on the input.

Science's bet is the opposite direction through the same door:

> **Fortress asked the programmer to write mathematics. Science asks the
> programmer to write a program and gives them mathematics back.**

Concretely, and each of these is checkable against §3:

- **No new operator.** `+ - * / **` are the operators Science already has, with
  the meanings it already gives them.
- **No Unicode requirement.** `E_a` is ASCII. The `\sigma` a user wants in the
  paper is written in the `as` string, which is a string, and strings have always
  been allowed to contain anything.
- **No ambiguity to resolve**, because the body grammar is the ordinary
  expression grammar minus almost everything, not mathematical notation plus
  interpretations.
- **The pretty thing is an output**, so it can be wrong without the program being
  wrong, and a user who never renders anything pays nothing for it.
- **§4.1 of the core spec is untouched.** No new English is added, no keyword is
  an abbreviation, and the one symbol introduced — `=` inside a body — is a
  symbol *"every reader of scientific and programming notation already reads"*,
  which is that section's own criterion.

The honest residue is that this argument could be made about any feature that
generates a nice output, and a project that makes equation rendering its
*marketing* would be making Fortress's mistake at the level of positioning even
while avoiding it at the level of syntax. §13 records that as a risk, because it
is one.

### 11.4 The verdict

> **A feature, narrow, F1, conditional on two things that are not this note's.**
>
> Build it if and only if: const-expression arithmetic lands and the units
> library exists; the body grammar stays closed; the acausal reading stays
> refused; and the simplifier is never written. Under those four conditions it is
> roughly 1 500 lines, it closes the one loop `publishable-output.md` cannot
> close, and it is the only construct in the design that makes a *method*
> traceable rather than a *result*.
>
> If any of the four conditions is relaxed, it becomes a computer algebra system
> or a simulation compiler, and the correct answer changes to **library**.
>
> The word is reserved either way, and reserving it was free.

---

## 12. What this note asks of the other notes

**Of `publishable-output.md`:**

1. **An `equation` block kind in the document record** (§7.2), with the fields
   §7.2 lists. It carries no floats, so Decision 2b is unaffected.
2. **One code from the reserved `SR0008`–`SR0099` block**, for an equation block
   whose `evaluations` count is zero (§9). That note owns the namespace; this is
   a request, not a claim.
3. **A sentence in §7.1's list of what can be generated truthfully**, adding the
   equation to the analysis record, the random-key lineage, the data lineage, the
   provenance appendix and the software citations. §7.5 above is the split
   between the rendered *model* and the recorded *method*, and it needs to be
   visible from that note.
4. **Nothing else changes.** This note is a consumer of Decisions 1, 2, 2b, 6, 12
   and 13 and contradicts none of them.

**Of `scientific-libraries.md`:**

5. **Typeset symbols in `physics.constants`** — `R` for `GAS_CONSTANT`, `c` for
   `SPEED_OF_LIGHT`, `h` for `PLANCK`, `k_B` for `BOLTZMANN`. §7.1 needs them and
   only the constants table knows them. This is the same shape as
   `publishable-output.md` §12 item 5b's ask on the same table and should be done
   in the same pass.
6. **That `optimize` §8.6's `gradient: (function …)?` parameter be
   fillable from an equation**, which requires only that an equation's derivative
   be expressible as a value of that function type. §5.2 is the argument; nothing
   in that note's text needs to change.
7. **Nothing that changes §12.** The dimensional check is that section's and this
   note is its consumer. §2.2 says so explicitly rather than borrowing the credit.

**Of `const-expression-arithmetic.md`:**

8. **Nothing.** §9.3's derivation block and the provenance spans on normal-form
   terms are exactly what §4.3 renders, and this note is a second customer for a
   field that note already committed to. Naming it so the interaction is visible.

**Of `strings-formatting-and-docs.md`:**

9. **That §5.3's doc-comment index reach fields** — already asked by
   `publishable-output.md` §12 item 2. This note is the **second** customer, and
   an equation's variables are fields in exactly the same sense, so the ask needs
   no widening.

**Of `uncertainty.md`:**

10. **Nothing that changes its text.** §5.2 there notes that `Quantity`'s
    `Display` must delegate to `T` rather than cast; §5.2 here observes that an
    equation's partial derivatives are precisely the propagation coefficients its
    §3 computes at run time, which means an equation can supply them exactly and
    can print the propagation formula. That is evidence for its design, not a
    change to it.

**Of `reserved-words.md`:**

11. **Nothing that changes its text**, and one supporting datum: §8.4 records
    that the equation construct wants none of `shape`, `model` or `tensor`, which
    removes one place a counter-argument to §3.3 could have come from.

**Of `README.md`:**

12. A row in the note index and a row in the diagnostic table: `equations.md`,
    `SC0246`–`SC0249` in the resolution column, F1. **This note could not add its
    own row** — the brief forbade editing any file but its own, and four other
    agents were writing at the time. That row is missing and should be added by
    whoever lands this.
13. The standing-asks table should record that **const-expression arithmetic now
    has a fifth caller** and that the **doc index reaching fields** ask now has
    two (`publishable-output.md` and this note), which moves it from a single
    note's request to a shared one.
14. **One inconsistency found while checking the allocation** (§9): the *Free as
    of this writing* table lists the whole of `SC0237`–`SC0249` as free in the
    resolution range, while the *Claimed by notes* table gives `SC0237`–`SC0240`
    to `reproducibility.md`. The claimed table is presumably right. It does not
    affect this note — `SC0246`–`SC0249` is free either way — and it is exactly
    the kind of drift that produced the five collisions the README records.

**Of the core spec:**

14. §13's *reserved, not yet used* list already contains `equation`, and
    `crates/science-lexer/src/token.rs` already has `ReservedWord::Equation` with
    a doc comment pointing at this file. **Nothing is asked.** §8 exists to
    justify a decision already taken and to record that `formula` was checked
    against the catalogue and refused.

---

## 13. Risks

**The whole construct is gated on const-expression arithmetic, which is gated on
nothing shipping yet.** §10's table item 1 is a hard dependency: without
`Quantity` multiplication there is no dimensional obligation, and without the
dimensional obligation the construct is `latexify_py` with a keyword. This is the
same dependency four other notes have, and this note is the one that degrades
most completely if it slips — the others lose a check, this one loses its reason
to exist.

**Someone will ask for a user function call in an equation body, and the reason
will be good.** A twelve-term equation wants factoring, and refusing to let the
author name a subexpression is a real ergonomic cost. Granting it collapses
§2.2(a), which is the note's single load-bearing argument. The mitigation that
preserves the argument is to allow *another equation* to be called, since its
body is from the same grammar and composes — but that is a design decision with
its own diagnostics and its own rendering question (does the called equation
inline, or appear as a named symbol?) and it is left to §14.

**A partial renderer will be built anyway, by someone, for functions.** The
`@render` derive of §11.2 is genuinely attractive and someone will want both. Two
renderers over two trees with different guarantees is a maintenance surface and a
user-facing confusion about which one applies, and the project should decide
which it wants rather than acquiring both by accretion.

**Marketing this feature would be Fortress's mistake in a different place.**
§11.3's argument is about syntax, and it holds. It does not hold about
positioning: a project that puts "your code is your equations" on the front page
is making an aesthetic promise to an audience that will judge it on whether their
particular equation renders nicely, and most of them have at least one that does
not. The feature is a quiet correctness property and should be sold as one.

**The derivative will sometimes be uglier than the textbook's**, per §5.1, and
the request for a simplifier will follow immediately and will be reasonable. It
is the request that turns 1 500 lines into a computer algebra system, and the
mitigation is the one `publishable-output.md` §7.3 uses for the same class of
problem: there is no simplifier to switch on, so granting the request means
building something, deliberately, rather than flipping a flag.

**Acausal modelling will be asked for by anyone who has used Modelica**, and
Decision 8 will look conservative to them. It is conservative. The reason to hold
the line is in §6.2 and it is about diagnostics, not about ambition, and it should
be quoted rather than re-argued each time.

**Adoption is the binding constraint here exactly as it is in
`publishable-output.md` §13.** The population writing equations twice is
overwhelmingly writing them in LaTeX and Python, and §7.4's refusal of a round
trip means there is no gradual path: the equation has to be rewritten in Science
to get anything. The one mitigation is that the artefact — a `.tex` file with one
`\input` line — fits into a workflow the author already has, which is the same
mitigation and the same partial answer as that note's asset bundle.

---

## 14. Open questions

1. **How ugly is an unsimplified derivative, in practice?** §5.1 asserts the gap
   is tolerable and does not measure it. Twenty equations from a first-year
   physical chemistry text, differentiated by the rules of §5.1 and compared with
   the book's printed derivative, would settle it in an afternoon, and it should
   be done before the renderer is written rather than after.
2. **Piecewise equations.** `if x < 0` inside a body is refused by §3.3, and
   piecewise definitions are common enough — activation functions, phase
   transitions, tabulated regimes — that the refusal will be felt. LaTeX has
   `\begin{cases}` and a piecewise derivative is well defined away from the
   boundaries. A `cases` form is expressible; whether it is worth the second
   grammar is not decided.
3. **Vector- and matrix-valued equations.** `F = m a` in three dimensions, or any
   equation over a `Tensor`, needs indexing or a vector notation in the body, a
   rendering convention for bold symbols, and a derivative that is a Jacobian.
   None of it is impossible and all of it is a second design. F1 tensors arrive in
   the same phase, so this will be asked for immediately.
4. **Derivatives in the result position.** §7.5's ODE case wants
   `dC/dt = -k * C` and the current answer — a variable named `dC_dt` with a
   hand-written `as "\frac{\mathrm{d}C}{\mathrm{d}t}"` — works and is ugly.
   A dedicated spelling would be better and is a notation decision, which is
   exactly the category §1.1 is nervous about.
5. **Should one equation be callable from another's body?** §13 names it as the
   mitigation for the factoring problem. It preserves totality because the callee
   is from the same grammar. It raises a rendering question — inline the callee's
   body, or print its result symbol and render it separately — whose answer is
   probably "print the symbol, and place both equations", and a cycle check.
6. **Where does the equation's own uncertainty live?** §5.2 can print the
   propagation formula. Whether evaluating an equation on `Uncertain` inputs
   should use the *symbolic* partials, or fall through to `uncertainty.md` §3's
   run-time gradient merge, is a choice between two correct answers with different
   costs, and it is that note's territory as much as this one's.

---

## 15. Decisions, with what each rejected

| # | Decision | Rejected alternative | Why | Cost |
|---|---|---|---|---|
| 1 | The construct is the renderable reading on a minimal symbolic slice; acausal and notation are refused | Adopt all four readings and stage them | Acausal is a second compiler; notation is unbounded ambiguity | Two genuinely valuable readings are given up, one of them proven |
| 2 | The contribution is **totality** from a closed body grammar, not the dimensional check | Claim the dimensional check; or `pure def` + a `@render` derive | Units already check an annotated function; a partial renderer fails on the equation that matters | Rests on one argument; the fallback is named and is not unreasonable |
| 3 | One name, resolving to the function in call position and the record in value position | Two names; or a call-operator interface | One declaration, two generated targets, nothing to disagree with | A name whose meaning depends on position |
| 4 | The body is one expression from a fifteen-operator closed grammar | An arbitrary function body | *"Every body the grammar admits has a rendering and a derivative, because the grammar admits nothing else"* | No factoring, no piecewise, no vectors — §14 items 2, 3, 5 |
| 5 | The dimensional failure **amends `SC0256`**; adds the residual row and the missing-factor help | A new type-range code | Identical condition, identical machinery, and `SC` has produced five collisions already | The amendment has to be agreed with the owning note |
| 6 | Symbolic first partials, with folding and no simplification | Ship a simplifier; no differentiator at all | A simplifier is where a CAS's complexity lives; without one, three catalogue consumers go unserved | Rendered derivatives are sometimes uglier than the textbook's |
| 7 | F1, gated on units — not F2, not waiting for AD | Build on F2's automatic differentiation | AD gives numbers; the rendered derivative and the propagation formula are unreachable from it | Nothing ships until const-expression arithmetic lands |
| 8 | No acausal equations; one stated result | Acausal; or acausal restricted to linear systems | Modelica's compiler is eight named stages, and its errors are structural with no span to blame | You cannot ask `f = m * a` for `a`; a second equation is unchecked against the first |
| 9 | Inversion only where the body is syntactically linear in the unknown, on request | Full symbolic solving; or generating every inverse | Linearity is decidable by a single-occurrence test; everything beyond it needs a solver | A quadratic equation's user finds the line arbitrary |
| 10 | An `equation` block in `publishable-output.md`'s document record | A separate equations artefact | One record, one source of truth, one staleness check — Decision 2 there | A block kind to version alongside tables and figures |
| 11 | Generation is one-way; no LaTeX parsing | Round-trip from the manuscript | LaTeX is presentation; a parser would be silently partial; a round trip recreates the second source | Migration is a manual paste, once per equation |
| 12 | Reserve `equation` — already done — and nothing else | Reserve `formula`, `symbol`, `solve`, `derivative`, `equations` | `formula` is `chem` §10.1's parameter name and type; the rest are shipped function names | One word, in declaration position, contextual later if needed |
