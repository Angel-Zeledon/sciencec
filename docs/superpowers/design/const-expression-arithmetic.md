# Science — Design: const-expression arithmetic in type position

Date: 2026-09-16
Status: draft for review
Owns: the const-expression grammar, the normal form and its equality procedure,
the `Shape` kind and its list operations, const-argument inference, the `with`
construct's type-level half, and diagnostics `SC0260`–`SC0262`.
Written in the syntax of `syntax-revision-2.md` and `syntax-revision-3.md`.
Customers: `scientific-libraries.md` §12.4/§12.5, `broadcasting.md` §5.2/§5.3/§11,
`indexing-and-array-literals.md` §1.3/§8 item 10, `unit-literals.md` §9.

---

## 0. The one sentence

**A const expression is a linear integer form over const parameters with
literal-scaled terms and literal-divided quotients, equality on it is structural
equality of a canonical `k + Σ cᵢ·aᵢ`, and a shape is a list of those — two
layers, with the list layer depending on the expression layer and never the
reverse.**

Everything below is the consequence of choosing a language small enough that its
normal form is total: **every expression the grammar admits has a normal form,
because the grammar admits nothing else.** Linearity is a property of the
productions, not a property checked afterwards. That is the whole trick, and §5
tests it against the precedent everybody will raise.

---

## 1. Why this note exists, and what it found that the four customers did not

### 1.1 The four asks

| Note | Section | Wants |
|---|---|---|
| `scientific-libraries.md` | §12.4 | `Quantity of (T, L1 + L2, M1 + M2, …)` — exponent addition, so `Mul` on quantities is expressible at all |
| `scientific-libraries.md` | §12.5 | `rfft` returning length `N / 2 + 1`; `concatenate` returning `A + B` |
| `broadcasting.md` | §5.2 | Shapes as **type-level lists of variadic arity**, with `pad_left`, `zip`, `delete_at` |
| `broadcasting.md` | §5.3 | And explicitly **not** `max` — the stretch test is syntactic on the literal `1` |
| `indexing-and-array-literals.md` | §1.3, §8 item 10 | The **Constant** and **Shape equality** index-obligation rules, built once with the normaliser here |
| `unit-literals.md` | §9.1, §9.2 | Stage C — `Mul`, `Div`, integer powers, `sqrt` — is gated entirely on this note landing |

`scientific-libraries.md` §12.5 is right that this should be built once. This
note accepts that argument and does not repeat it. What follows is what the
argument costs when it is made precise, and it is more than §12.4 priced in three
specific places and less in one.

### 1.2 Four findings from reading the shipped compiler before designing on top of it

The brief for this note said to read `crates/science-parser/src/ast.rs` and
`crates/science-resolve/src/` first. Doing so changed four decisions.

**1.2.1 `scientific-libraries.md` §12.3's nine aliases do not parse today.**
`type Velocity of T is Quantity of (T, 1, 0, -1, 0, 0, 0, 0)` contains `-1`. The
lexer emits `Minus` then `Int` (`crates/science-lexer/src/lexer.rs`, `'-' =>
TokenKind::Minus`); there is no negative numeric literal. `parse_type` admits a
const argument only through `literal_of(self.peek())`, which matches `Int`,
`Float`, `Str`, `Char`, `True`, `False` and nothing else. So a `-` in a type
argument is `expected a type, found -`.

This matters more than it sounds. **Unary negation in type position is not polish
that F1 can add; it is required for the units representation that note already
considers settled.** It is on the F0 list in §10 for that reason.

**1.2.2 `Literal::Int { value: u128 }` cannot hold a unit exponent.** The
argument node is `TypeKind::Const(Literal)` and `Literal::Int`'s value field is
unsigned. Even if `-1` parsed, it could not be stored. The const-argument node
has to change shape, which is an AST change, which moves parser snapshots. §10
argues from this that the change belongs in F0 and not later.

**1.2.3 A one-element shape already collapses.** `parse_paren_type` says, in its
own comment, *"`(T)` is just `T`: parentheses only group, so no node of their
own."* So `Vector of (F32, (768))` and `Vector of (F32, 768)` produce the same
tree, and `broadcasting.md` §5.2's `Tensor of (T, (N))` is indistinguishable from
a scalar const argument. §6.4 decides what to do about that, and the answer costs
no syntax.

**1.2.4 `+` is already a separator in one type-adjacent position.** `parse_bounds`
reads `A + B + C` for interface bounds. It does not go through `parse_type` —
`parse_type_bound` calls `parse_path` directly — so there is no ambiguity in the
parenthesised argument list. But `parse_generic_args` has a bare form:

```rust
if self.eat(&TokenKind::LParen).is_none() {
    return vec![self.parse_type()];
}
```

so `T: Bound of N + 1` would let the const-expression climb swallow the `+ 1`
that was meant to separate two bounds. §2.4 settles it with one rule.

### 1.3 Three places where this is more expensive than §12.4 priced, and one where it is less

**More expensive.**

- **Type-level lists** (§6). `broadcasting.md` §15 already says so. This note
  confirms it and bounds it: one kind, six operations, no recursion, no user
  extension.
- **`N / 2` is not linear** (§4), and the honest treatment is an atom in the
  normal form, not a rational coefficient. §12.5's sentence *"the normal form
  must handle division by a literal"* understates the problem: a rational
  coefficient is unsound against §5.1's truncating integer division.
- **Const-argument inference** (§7). Nothing asked for it by name, and `sqrt` on
  a `Quantity` needs it, and so does every call site that does not spell its
  const arguments. §12.4 lists three missing things; this is a fourth.

**Less expensive.**

- **`scientific-libraries.md` §12.5 says units need only linear combinations and
  shapes additionally need division.** That is very nearly right and is wrong in
  one corner: `unit-literals.md` §9.2 promises `sqrt` on even dimensions in stage
  C, and halving an exponent is division. §7.2 discharges it **without** giving
  units division at all, by putting the halving in the parameter position and
  solving rather than in the return position and dividing. So §12.5's claim
  survives — units need no division — but for a reason that note does not give.

---

## 2. The expression language

### 2.1 Decision — five operators, and nothing that is not one of them

> **Decision 2.1.** The const-expression grammar is:
>
> ```
> ConstExpr  := ConstExpr '+' ConstTerm
>             | ConstExpr '-' ConstTerm
>             | ConstTerm
>
> ConstTerm  := ConstTerm '*' IntLiteral
>             | IntLiteral '*' ConstTerm
>             | ConstTerm '/' IntLiteral
>             | ConstAtom
>
> ConstAtom  := IntLiteral
>             | ConstParam
>             | '-' ConstAtom
>             | '(' ConstExpr ')'
> ```
>
> Precedence and associativity are §4.6's, unchanged: `*` and `/` bind tighter
> than `+` and `-`, unary `-` binds tighter than both, and the binaries are
> left-associative. `IntLiteral` is an unsuffixed integer literal in any base.

Five operators: `+`, `-` (binary), `-` (unary), `*`, `/`. That is the complete
list, forever, and every addition to it is an addition to the normaliser, the
equality procedure, the diagnostic renderer and the monomorphization key.

**Read the `ConstTerm` productions again**, because they are the design. `*` and
`/` each require a **literal** on one side. The grammar cannot express `L * K`
for two parameters, so the normaliser never receives a non-linear input and never
needs a case for one. **Linearity is enforced by the productions, not by a check
after parsing.** This is the single most important property in the note and §5
is about what it buys.

### 2.2 Decision — what is deliberately absent, and who asked for each

> **Decision 2.2.** No `%`, no `**`, no `max`, no `min`, no comparison, no
> conditional, no function call, no `const fn`, no recursion, no user-defined
> const operation of any kind. There is no escape hatch and none is planned.

| Excluded | Who might want it | Why not |
|---|---|---|
| `max` | The naive reading of the broadcast output dimension | `broadcasting.md` §5.3 removed the need. `max` over linear forms is not a linear form, so admitting it destroys the normal form. This is the one exclusion that is load-bearing and it was already decided. |
| `**` with a const exponent | `Quantity.pow of (const K: Int)` — `L * K` | Non-linear by construction (§2.1's grammar cannot write it). `squared`, `cubed` and `sqrt` cover what `physics` needs, with literal coefficients. A user who wants `x ** 7` writes it as a product. §11 asks `scientific-libraries.md` to say so. |
| `%` | Stride and tiling arithmetic in a GPU backend | F2's problem. `a % d` is `a - (a / d) * d`, which the grammar already expresses when `d` is a literal, so nothing is lost except brevity. |
| `min`, `abs`, clamping | Slice length with an open end — `a[2..]` on a `Tensor of (T, (N))` | §4.5. It is `N - 2`, and the negative case is §9's `SC0262` at monomorphization, not a clamp in the type. |
| Comparison, conditional | A `where` clause carrying `N > 0` | This is const **predicates**, which is a different feature and a much larger one. §13 keeps it open; §5.3 explains why admitting it is precisely how Rust's version of this feature stalled. |
| `const fn` | Everyone, eventually | It makes type equality equality of arbitrary programs. §5.2. |
| Bool and Char const parameters | `keepdims` | `broadcasting.md` §7.3 already rejected the only caller. Decision 2.3. |

### 2.3 Decision — a const parameter's annotation is a kind, and there are two

> **Decision 2.3.** `const N: K` admits exactly `K ∈ { Int, Shape }`. `Int` is
> the F0 kind. `Shape` arrives in F1 (§6). Anything else — `const F: Bool`,
> `const S: String`, `const W: F64` — is `SC0260`.

`crates/science-parser/src/parser.rs`'s `parse_generic_param` currently writes
`let ty = self.parse_type();` and accepts any type there. **The parser does not
change.** It keeps accepting the tree and the type checker reports `SC0260` with
the two admissible kinds named, which is the house pattern already used for
`TypeKind::Any` (*"the parser accepts it anywhere and lets a later phase say so
with a better message"*). Cost: zero parser lines, one checker arm, one good
message.

Rejected: **`Bool` const parameters.** They are cheap and they have exactly one
proposed caller, `keepdims`, which `broadcasting.md` §7.3 rejected on the ground
that a boolean whose value changes the rank of the return type is a type error
waiting to be specified. Admitting the kind would reopen that. Rejected:
**`F64`**, because float equality is not equality (§5.1: `F32`/`F64` implement
`Eq` but not `Ord`, and NaN exists), and a normal form over floats is a normal
form over a non-field with a non-value in it.

### 2.4 Decision — operators require the parenthesised argument list

> **Decision 2.4.** A const expression with an operator in it is legal only
> inside `of ( … )`. In the bare form `of X`, a const argument is a `ConstAtom`:
> a literal, a parameter, or a parenthesised expression.

```science
Vector of (F32, N + 1)      # legal
Vector of (N + 1)           # SC0260: write `Vector of ((N + 1))` or use the
                            #         parenthesised argument list
T: Bound of (N + 1)         # legal: the parens close before the bound list resumes
T: Bound of N + 1           # `N` is the argument; `+ 1` is a malformed bound
```

This is forced by §1.2.4 and it is not a compromise — it is the rule that keeps
one `+` from meaning two things at one token of lookahead. The alternative is to
make `parse_bounds` and the const-expression climb negotiate, which means the
parser needs to know a declaration's arity and kinds, which it does not and
should not (the resolver already documents this same boundary: *"there is no way
to tell the two apart without the declared arity of the thing being applied —
which is `science-types`' to know, not this phase's"*).

The cost is one diagnostic on a form nobody writes. Every signature in
`scientific-libraries.md` §6.7, §9.4 and §12.7 already uses the parenthesised
list, because every one of them has a type argument as well.

### 2.5 The AST change, stated as a change

`TypeKind::Const(Literal)` becomes:

- a const-expression node with the five operator forms and two atom forms, and
- a signed integer for the literal, because `Literal::Int`'s `u128` cannot hold
  `-1` (§1.2.2), and the exponent vector of `scientific-libraries.md` §12.3 is
  full of them.

This moves parser snapshots. **That is the argument for doing it in F0**, and it
is §2 of the core spec's own argument: *"retrofitting them means reopening the
parser, the resolver, and every snapshot."* The snapshot set is small today and
grows monotonically.

---

## 3. The normal form, and the decision procedure for equality

### 3.1 Decision — the normal form

> **Decision 3.1.** A normalised const expression is
>
> ```
>     k + Σᵢ cᵢ · aᵢ
> ```
>
> where `k ∈ ℤ`, every `cᵢ ∈ ℤ \ {0}`, and the atoms `aᵢ` are **strictly
> increasing** in a total order that is stable across compilations. An atom is
> either a **const parameter** or a **quotient** `⌊e / d⌋` where `e` is itself
> normalised and `d ≥ 2` is a literal (§4).

The atom order: parameters first, ordered by the canonical path of their
declaring item and then by their index in its `of` list; quotients after, ordered
by their divisor and then recursively by their numerator's normal form.
Deliberately **not** `DefId`, which is an allocation order and would make a
diagnostic's rendering depend on the order files were read.

For units, atoms are always parameters and the form is `k + Σ cᵢ·pᵢ` — exactly
what `scientific-libraries.md` §12.4 point 3 asks for, spelled the same way. **The
quotient atom is charged entirely to shapes; the units checker never constructs
one** (§7.2). That separation is worth stating because it means a reviewer
auditing the units half can stop reading at §3.

### 3.2 NORMALISE

Total, structural, one pass, no fixpoint.

```
NORMALISE(e):
  literal n          ->  (k = n, terms = [])
  param p            ->  (k = 0, terms = [(1, p)])
  -e₁                ->  NEGATE(NORMALISE(e₁))
  e₁ + e₂            ->  MERGE(NORMALISE(e₁), NORMALISE(e₂))
  e₁ - e₂            ->  MERGE(NORMALISE(e₁), NEGATE(NORMALISE(e₂)))
  e₁ * d   (d lit)   ->  SCALE(NORMALISE(e₁), d)
  e₁ / d   (d lit)   ->  DIVIDE(NORMALISE(e₁), d)

MERGE     : add the constants; add coefficients of equal atoms; drop
            any term whose coefficient became 0; keep the order.
NEGATE    : negate k and every cᵢ.
SCALE(f,0): the constant 0.  SCALE(f,d): multiply k and every cᵢ by d.
DIVIDE(f,0)              -> SC0260, division by zero
DIVIDE(f,d), terms empty -> the constant trunc(k / d)
DIVIDE(f,d), d | k and d | every cᵢ
                         -> SCALE-down: (k/d) + Σ (cᵢ/d)·aᵢ      [the fold]
DIVIDE(f,d), otherwise   -> the single atom ⌊f / d⌋, coefficient 1, k = 0
```

`MERGE` over a sorted term list is a linear merge, so `NORMALISE` is linear in
the size of the expression and the recursion depth is the expression's syntactic
depth. There is no possibility of non-termination because there is no production
that grows an expression.

### 3.3 Decision — EQUAL is structural equality on the normal form

> **Decision 3.3.** `EQUAL(e₁, e₂)` is `NORMALISE(e₁) = NORMALISE(e₂)`,
> structurally: equal constants, equal term counts, and pairwise equal
> `(cᵢ, aᵢ)` — with quotient atoms compared by recursing on their numerators'
> normal forms and their divisors.

So `Quantity of (T, L1 + L2, …)` and `Quantity of (T, L2 + L1, …)` are the same
type. Both normalise to `0 + 1·L1 + 1·L2` after the atom order sorts them, and
the library is usable. That is the requirement `scientific-libraries.md` §12.4
point 3 states, and it is met.

### 3.4 The soundness argument, which is two lines and should be written down

Restrict to the quotient-free fragment, which is all of units and most of shapes.
A normalised form denotes a function `ℤⁿ → ℤ` where `n` is the number of const
parameters in scope. Evaluating at the origin recovers `k`; evaluating at the
`i`-th basis vector and subtracting `k` recovers `cᵢ`. So **the map from normal
forms to denoted functions is injective**: two normal forms denote the same
function exactly when they are structurally equal. `EQUAL` is therefore both
sound and complete on that fragment, and it decides in time linear in the forms.

That is the entire theory. It is the free ℤ-module on the parameters, plus ℤ, and
it has a canonical basis because the grammar cannot leave it.

With quotients, an atom `⌊e/d⌋` is treated as **opaque** — a fresh variable
constrained only by being equal to itself. That keeps `EQUAL` sound (two
structurally different forms may still denote the same function, so `EQUAL`
answers "not provably equal", never "equal" wrongly) and makes it **incomplete**
(§4.3 bounds the incompleteness and shows what it costs).

### 3.5 Where EQUAL is called

Four callers, and it is the same procedure in all four. This discharges
`indexing-and-array-literals.md` §8 item 10, which asks that the obligation
discharger be built once with the shape checker.

1. **Type equality**, whenever two instantiations of the same const-generic type
   are compared.
2. **Shape equality** (§6.5), elementwise.
3. The **Constant** and **Shape equality** rules of
   `indexing-and-array-literals.md` §1.3's index-obligation discharger.
   `a[3]` on a `Tensor of (F64, (2, 2))` discharges because `EQUAL` and the
   literal comparison are both available on the normal form; `left[i]` where `i`
   ranges over `right`'s axis discharges because the two shape entries normalise
   equal. **There is one implementation and this note owns it.**
4. **Monomorphization keys.** Two instantiations that are `EQUAL` must produce one
   symbol, or `a * b` and `b * a` link to two copies of the same function. The key
   is the normal form, serialised in atom order — which is why §3.1's order must
   be compilation-stable and not `DefId`.

---

## 4. Division, and the honest treatment of `N / 2`

This is the section `scientific-libraries.md` §12.5 asks for in one sentence and
which does not fit in one sentence.

### 4.1 The problem

`rfft`'s output length is `N / 2 + 1` where `N` is a symbolic const parameter.
That is **floor division on a symbol**, and `⌊N/2⌋` is not a linear form in `N`.
So the normal form of §3.1 either grows a case for it or the customer is refused.

Two treatments were priced.

**(a) Rational coefficients.** Let `cᵢ ∈ ℚ` and `k ∈ ℚ`. Equality is still
decidable — linear forms over a field normalise perfectly well — and the
implementation barely changes. **Rejected, and it is not close.** `N / 2` at
`N = 5` denotes `5/2` in the type and `2` at run time, because §5.1 of the core
spec fixes integer division as truncating toward zero. A type-level value that
disagrees with the run-time value it describes is not a conservative
approximation, it is a lie: the allocated buffer would be indexed past its end
and the language's entire premise — *the compiler checks the shape* — would be
false in exactly the operation that motivated the feature. The only way to make
it honest is to forbid inexact division, which is treatment (c) below, which
refuses `rfft`.

**(b) Quotient atoms. Adopted.** `⌊e / d⌋` enters the normal form as an atom
alongside const parameters, with the fold of §3.2 collapsing it whenever the
numerator is exactly divisible. `N / 2 + 1` normalises to `1 + 1·⌊N/2⌋`. It is
comparable, hashable, orderable, serialisable as a monomorphization key, and it
evaluates at instantiation to exactly what the run time computes.

**(c) Restrict division to exactly-divisible numerators.** The normal form stays
flat, equality stays complete, the implementation is smallest. **Rejected: it
refuses `rfft`, which is the named customer**, and `rfft` is not a corner — it is
the most-used function in `signal`.

> **Decision 4.1.** Division is by a non-zero integer literal, it truncates
> toward zero exactly as §5.1 specifies for values, and an inexact division of a
> symbolic form becomes a quotient atom.

### 4.2 Decision — the fold is whole-form only, and why the obvious improvement is unsound

> **Decision 4.2.** `DIVIDE(f, d)` folds only when `d` divides `k` **and** every
> `cᵢ`. It does not split a form into a divisible part and a remainder.

The tempting improvement is `(2N + 1) / 2 → N + ⌊1/2⌋ → N`. It is wrong. With
truncation toward zero and `N = -1`, the left side is `⌊-1/2⌋ = 0` and the right
side is `-1`. The split is valid only under a non-negativity assumption, and the
const language carries no sign information about its parameters.

Giving it sign information — a `Nat` kind, or a range kind — was considered and
is **rejected for F0 and F1**: it buys one more fold, and it costs a kind lattice
on const parameters, subkinding at every instantiation, and a second failure mode
in every diagnostic ("this parameter is not known to be non-negative"). §13 keeps
it open, because a GPU backend that wants tiling arithmetic will ask for it.

### 4.3 What the incompleteness actually costs, measured against the customers

`EQUAL` will answer "cannot prove" on pairs a mathematician would call equal. The
bound on which pairs: **only where the two forms contain syntactically different
quotient atoms.** Concretely:

| Pair | `EQUAL` says | Right answer | Reached by |
|---|---|---|---|
| `N / 2 + 1` vs `1 + N / 2` | equal | equal | the ordinary case; the fold and the sort handle it |
| `(2 * N) / 2` vs `N` | equal | equal | the fold fires |
| `N / 2 + N / 2` vs `N` | **cannot prove** | not equal (odd `N`) | correct refusal |
| `N / 4` vs `(N / 2) / 2` | **cannot prove** | equal | a genuine false negative |
| `(N + 2) / 2` vs `N / 2 + 1` | **cannot prove** | equal | a genuine false negative |

Two false negatives, both requiring a program that divides the same dimension
twice by different routes and then requires the two results to be the same type.
No signature in `scientific-libraries.md`, `broadcasting.md` or
`indexing-and-array-literals.md` does that. The refusal is `SC0261` and it prints
both normal forms (§9.2), so a user who hits one can see immediately what to
write instead — spell the shape once and reuse the parameter.

**Priced against the alternatives:** completeness here means deciding linear
integer arithmetic with floor, which is Presburger arithmetic. It is decidable,
and it is doubly-exponential, and it means the type checker contains a decision
procedure whose failure mode is a compile that does not finish. **A compiler that
sometimes says "I cannot prove this, write it differently" is strictly better for
this audience than a compiler that sometimes does not terminate**, and the second
one is what "complete" costs. This is the trade §5 says Rust could not make.

### 4.4 What division does *not* do

It does not raise an exactness obligation. `L / 2` in a return position where `L`
is odd is silently `⌊L/2⌋`, which for a unit exponent would be wrong — `sqrt` of
`m³` is not `m`. **This is why units do not use division** (§7.2): the halving is
put in the *parameter* position and solved, which produces the error
`unit-literals.md` §10 already allocates `SC0254` for, at the right moment and
with the right message.

### 4.5 One consequence for slicing

`indexing-and-array-literals.md` §2.1 gives `samples[2..]`, whose result length
on a `Tensor of (T, (N))` is `N - 2`. Under this design that is an ordinary
normalised form, and `N < 2` is not a type error in the generic — it is
`SC0262` at the instantiation that makes the extent negative, with the
instantiation chain (§9.3). §5.3 argues that Science is allowed to make that
choice and Rust was not.

---

## 5. The precedent, tested rather than asserted

`scientific-libraries.md` §12.4 says point 3 *"is where Rust's const generics
stalled for years"* and then says Science's case is easier *"because the
expression language required is deliberately tiny."* The first half misidentifies
the stall and the second half is true for a reason the note does not give. Both
are worth correcting, because the argument will be raised by the first reviewer
and it should survive.

### 5.1 What actually stalled

Rust shipped `min_const_generics` in 1.51 (2021): const parameters, but a const
*argument* had to be a literal or a bare parameter — no arithmetic at all. The
arithmetic lives in `generic_const_exprs`, which has been unstable since 2020 and
still is. Normalising `N + 1` against `1 + N` was **not** the blocker; that part
was understood early. Three other things were:

1. **The expression language is `const fn`.** Rust decided const expressions are
   arbitrary const-evaluable Rust. Equality of two such expressions is equality of
   two programs. There is no normal form and there cannot be one, so equality
   falls back to syntactic identity plus a small amount of unification, which is
   wildly incomplete in a language where users expect `N + 1` and `1 + N` to be
   interchangeable *and* expect `f(N)` and `f(N)` to be too, for a `const fn f`
   whose body they can change.
2. **Well-formedness in the generic context.** `[u8; N - 1]` must be rejected —
   or proved fine — *before* knowing `N`. Rust's promise is that a generic item
   typechecks once and every instantiation is then sound. Honouring that for
   const expressions requires a **predicate** language (`where N >= 1`), an
   implied-bounds story, and an entailment checker over those predicates. That is
   the feature that has not shipped.
3. **Post-monomorphization errors.** Without (2), an error surfaces at
   instantiation, which is the thing Rust's type system is specifically designed
   not to do, because separate compilation and `dyn` both rest on the generic
   having been checked in the abstract.

### 5.2 Difference one: the expression language is closed, and that is real

Science's grammar is §2.1. Five operators, no calls, no recursion, no branching.
Every expression has a normal form because the productions cannot produce
anything else, and §3.4's injectivity argument fits in two lines. Rust cannot have
this without deleting `const fn` from const arguments, which it will not do
because `const fn` is why the feature was wanted.

**This difference is genuine and it is the one §12.4 names.** It is not the whole
answer.

### 5.3 Difference two: Science already monomorphizes, so it may report at instantiation

Point (2) above is the real stall, and it applies to Science too: `Tensor of (F32,
(N - 1))` with `N = 0` is an extent of `-1`, and nothing in §2's grammar prevents
writing it.

Rust must reject or prove that in the generic. **Science need not, and the reason
is §3 of the core spec, which already chose monomorphization over uniform
representation.** There is no separately-typechecked generic object code in
Science; every instantiation is compiled from source. So:

> **Decision 5.3.** Value obligations on const expressions — extent
> non-negativity, and the solvability of §7's inference — are discharged **at
> monomorphization**, per instantiation, and reported as `SC0262` with the
> instantiation chain. Science does **not** promise that a generic item which
> typechecks has no const-expression errors in any instantiation. It promises
> that the error names the instantiation, the parameter, the value and the call
> that produced it.

This is a real cost and it should not be dressed up. A library author can ship a
signature that is fine for every shape they tested and fails for a caller's. The
mitigations are that (a) the error is a compile error at the caller, never a run
time one, (b) the chain in the message names the library function, so the bug
report writes itself, and (c) the obligations are two, not an open-ended set.

The alternative — const predicates in `where`, entailment, implied bounds — is the
feature that has been unstable in Rust for six years. **Pricing it as anything
other than "this is a multi-year research-grade subsystem" would be dishonest.**
§13 keeps it open. It is the right thing to want and the wrong thing to block the
type checker on.

### 5.4 Difference three: inference is restricted by choice, not by accident

Rust wants `T: Trait<{N+1}>` to unify in general, which is solving linear
Diophantine systems with unknown structure. Science restricts const-argument
inference to one-variable linear matching (§7), which is division with a
remainder test. §5.2 of the core spec already rejects global inference for
ordinary types; this is the same decision applied to the same reason.

### 5.5 The verdict, stated so it can be challenged

**The "Rust stalled for years" risk does not apply to §§2–4 of this note, and a
weakened form of it does apply to §13's open questions.** The normaliser and its
equality procedure are a closed, total, linear-time problem with a two-line
correctness argument; they are not research. What *is* research is the predicate
layer, and this note does not build it, does not depend on it, and names exactly
what is deferred because of its absence: sign information (§4.2), generic-context
well-formedness (§5.3), and broadcast constraints (`broadcasting.md` §13).

The honest summary for a reviewer: **Science is not avoiding Rust's problem by
being cleverer. It is avoiding it by refusing the feature that caused it, and
paying for the refusal in post-monomorphization errors, which it can afford
because it monomorphizes everything anyway.**

---

## 6. Type-level lists

`broadcasting.md` §5.2 is the demanding customer and drives this section.

### 6.1 Decision — one kind, `Shape`, and it is not general type-level programming

> **Decision 6.1.** F1 adds a second const-parameter kind, `Shape`: a **list of
> const `Int` expressions**. Its length — the rank — is **always statically
> known**. Its elements may be symbolic. There is no `List of K` for arbitrary
> `K`, no type-level tuples, no type-level strings, and no way for a user to
> declare a new kind.

```science
type Tensor of (T, const SHAPE: Shape):
    ...

type Matrix of (T, const R: Int, const C: Int) is Tensor of (T, (R, C))
type Vector of (T, const N: Int) is Tensor of (T, (N))
```

This is `broadcasting.md` §5.2 adopted verbatim, including its consequence that
`Matrix` and `Vector` are aliases rather than primitive constructors. That note's
ask 2 on `scientific-libraries.md` §6.1 is seconded here.

**Rank is statically known, extents may not be.** That split is the one that keeps
everything decidable, and it deserves its own sentence because it is easy to lose:
there is no `const RANK: Int` and no arithmetic on ranks. A function generic over
rank is not expressible in F1. The cost is `reshape`, `squeeze` and `flatten`,
which take their output rank from a literal shape argument rather than computing
it — `x.reshape((a, b, c))` — and that is what a user writes anyway.

### 6.2 Decision — six operations, all total, all compiler-internal

> **Decision 6.2.** The shape layer has exactly these operations. None is
> user-callable; all are what the checker does when it checks an operator or a
> method whose signature mentions shapes.

| Operation | Signature | Wanted by |
|---|---|---|
| `construct` | `(e₁, …, e_r) ↦ Shape` | every shape written in a type |
| `rank` | `Shape ↦ ℕ` (compiler-side, not a const expression) | the alignment rule, `SC0298` |
| `at` | `(Shape, i : literal ℕ) ↦ ConstExpr` | the alignment rule, `SC0298` |
| `pad_left` | `(Shape, r : ℕ, fill = 1) ↦ Shape` | `broadcasting.md` §4.2 |
| `zip` | `(Shape, Shape, op) ↦ Shape`, equal ranks required | `broadcasting.md` §4.2 |
| `delete_at` | `(Shape, i : literal ℕ) ↦ Shape` | `broadcasting.md` §7.2, reductions |
| `insert_at` | `(Shape, i : literal ℕ, e) ↦ Shape` | `broadcasting.md` §7.3, `insert_axis` |

That is seven rows because `construct` is listed; `broadcasting.md` §5.2 names
three of them and §7 adds `insert_at`. `rank` and `at` are the primitives the
other four are written in terms of. **The set is closed and is extended only by a
spec revision**, on the same discipline `stdlib-shape-and-packages.md` applies to
the library.

Notice what is not here: no `map` over a shape with a user-supplied operation, no
`fold`, no `filter`, no recursion, no length arithmetic. A type-level list with
`map` and `fold` is a type-level programming language, which is a feature with no
customer in these four notes and an unbounded cost.

### 6.3 Decision — the list layer never constructs a const expression

> **Decision 6.3.** Every element of every shape the checker produces is either
> an element of an input shape, unchanged, or the literal `1`. The shape layer
> calls `NORMALISE` **only** through `EQUAL`, and never builds a new expression.

This is `broadcasting.md` Decision 5.3 restated from this side of the seam, and it
is what makes the two layers stack rather than interleave. `pad_left` inserts the
literal `1`. `insert_at` inserts the literal `1`. `zip` under the broadcast
operation returns one of its two arguments unchanged, by Decision 5.3's
literal-only stretch test. `delete_at` removes.

So the dependency graph is: **shape layer → expression layer, one direction, no
cycle.** The expression layer can be implemented, tested and shipped in F0 without
the shape layer existing, which is exactly what §10 asks F0 to do.

**One clarification `broadcasting.md` §5.3 leaves implicit and which a reader will
trip on.** That decision says `max` never enters the const-expression language,
and §4.2 of the same note computes `r = max(m, n)` over the two ranks. Both are
true: `max` over *ranks* is a compiler-side maximum of two literal naturals,
computed by `pad_left`'s caller, and it never appears in a type. `max` over
*extents* is what is banned. The words are the same and the levels are not.

### 6.4 Decision — a const `Int` argument in a `Shape` position lifts to rank 1

> **Decision 6.4.** Where a `Shape` is expected and a const `Int` expression is
> given, it is the one-element shape. `Vector of (T, N)` is
> `Tensor of (T, (N))`, and the two spellings are one type.

This is forced by §1.2.3: `parse_paren_type` collapses `(T)` to `T`, so
`Tensor of (F32, (768))` already *is* `Tensor of (F32, 768)` in the tree, and no
amount of checker cleverness can tell them apart. The lift is keyed on the
declared kind of the parameter being filled, which the checker knows and the
parser does not.

Rejected: **a trailing-comma form, `(768,)`, as Python spells a one-tuple.** It
introduces a comma whose presence changes a type's meaning, in a grammar where
`(A, B)` is already a tuple type, for one case. Worse, it would make
`Vector of (T, N)` — written in `scientific-libraries.md` §6.7 and §9.4, in
`broadcasting.md` §5.2, and in every linear-algebra signature anyone will write —
either illegal or a second spelling. **Rejected decisively.**

The residue: `(A, B)` in an argument position is a *tuple type* when the
parameter's kind is a type and a *shape* when its kind is `Shape`. Two readings,
one syntax, disambiguated by the declared kind. This is not a new compromise — it
is the one `crates/science-resolve/src/hir.rs` already documents on
`DefKind::is_type`, which admits `ConstParam` in a type position precisely because
*"there is no way to tell the two apart without the declared arity of the thing
being applied."* **This note extends that compromise by one case rather than
introducing a second one**, and the resolver's comment should gain a sentence
saying so (§11).

### 6.5 Shape equality

`EQUAL_SHAPE(S, T)` is: equal rank, then `EQUAL` elementwise. Decidable because
both components are, sound and incomplete exactly where §4.3 says. Unequal rank is
not "cannot prove", it is a definite inequality, and the diagnostic says so —
which matters, because a rank mismatch and an extent mismatch have different fixes
(`insert_axis` versus a different call).

---

## 7. Const-argument inference

Nothing asked for this by name and three things need it.

### 7.1 Decision — one-variable linear matching, and nothing more

> **Decision 7.1.** A const parameter `p` is **determined** by an argument
> position whose normal form is `c·p + k` with `c ≠ 0` and no other unbound
> parameter, given a concrete value `v` at that position: `p = (v − k) / c`, and
> it is an error (`SC0262`) if `c` does not divide `v − k`. A position mentioning
> two or more unbound parameters is **not** an inference site. A parameter
> determined at no site must be given explicitly at the call.

This is §5.2 of the core spec's rule — *"local; signatures are fully annotated"* —
applied to const parameters. It is division with a remainder test and it is
decidable in constant time per site. Solving the general case means integer linear
systems with unknown structure, which is what §5.4 says Rust wanted and Science
declines.

The three consumers:

- **`Mul` on quantities.** `self: Quantity of (T, L1, M1, …)` determines `L1` at
  `c = 1, k = 0`. Every parameter of §12.4's signature is determined. Nothing is
  written at the call site, which is the whole point.
- **The shape parameters of every `linalg` signature.** `a: Tensor of (F32,
  (M, K))`, `b: Tensor of (F32, (K, N))` determines `M`, `K`, `N`; the repeated
  `K` is a second site for an already-determined parameter, which is an `EQUAL`
  check, not an inference.
- **`sqrt` on a quantity** — §7.2, which is the interesting one.

### 7.2 Decision — units get `sqrt` through the parameter position, not through division

> **Decision 7.2.** `sqrt` on a `Quantity` is declared with the *doubled*
> exponents in its parameter type and the plain ones in its return type. The
> "odd dimension" error is then a failure of §7.1's divisibility test, not a
> property of `/`.

```science
Quantity has:
    def sqrt of (const L: Int, const M: Int, const T: Int,
                      const I: Int, const K: Int, const Nn: Int, const J: Int)(
        self: Quantity of (F64, L * 2, M * 2, T * 2, I * 2, K * 2, Nn * 2, J * 2),
    ) -> Quantity of (F64, L, M, T, I, K, Nn, J)
```

Call it on an `Area` (`L = 2`): the site is `L * 2`, `c = 2`, `k = 0`, `v = 2`,
so `L = 1` and the result is a `Length`. Call it on a `Volume` (`v = 3`): `2`
does not divide `3`, and that is `SC0262`, which is exactly the condition
`unit-literals.md` §10 allocates `SC0254` for — *"`sqrt` of an odd dimension —
§12.6's rational-exponent exclusion, surfacing."* §11 asks that note to keep
`SC0254` as the code and to have the units checker raise it in place of the
generic `SC0262`, because its message is better.

**Three things this buys.** Units never touch division, so
`scientific-libraries.md` §12.5's claim that units need only linear combinations
stands (§1.3). The error fires at the call site with the caller's type in it,
rather than at a division deep inside a return type. And §12.6's rational-exponent
exclusion is enforced by a mechanism rather than by a special case.

`cbrt` is the same with `* 3`, and `squared` and `cubed` are `L * 2` and `L * 3`
in the return position, needing no inference at all.

### 7.3 Decision — explicit const arguments are available and are the fallback

Where §7.1 determines nothing, the caller writes the arguments. That requires
explicit type arguments at call sites, which `data-io.md` §11.2 and
`scientific-libraries.md` §14.3 have already asked for as a standing cross-note
ask. **This note is the third asker and it is the one that makes it load-bearing
rather than convenient**: without it, a const parameter appearing only in a return
type is uninferable and the signature cannot be called at all. §11 records it.

---

## 8. What the user writes when the checker cannot prove it

A dimension read from a CSV is not a const expression and never will be. This is
the section that decides whether the feature is usable outside library signatures.

### 8.1 Decision — `broadcasting.md` §6.2's `with … as …` is adopted, narrowed twice

> **Decision 8.1.** `with` is adopted as specified in `broadcasting.md` §§6.2–6.3,
> with two narrowings this note adds because it owns the const-parameter side.

The construct, unchanged:

```science
    with table.shape as (rows, 768), weights.shape as (768):
        let z be standardise(table)
        let flagged be score(z, weights, 0.5) .> 3.0
        print(flagged.count_true(), "of", rows, "rows are outliers")
    else mismatch:
        print("the weights do not match the measurements:", mismatch)
```

`rows` inside the block is a const `Int` parameter of **known identity and
unknown value** — the same thing a const parameter of a generic function is. That
is the property that makes everything in §§2–7 apply unchanged inside the block,
and it is why this construct belongs to this note and not only to
`broadcasting.md`.

**Narrowing one: the scrutinee is a shape, not an arbitrary value.**
`with e as (a, b)` requires `e` to be shape-valued. There is no
`with n as const N` binding a const parameter from an arbitrary runtime `Int`,
and none is planned. Rejected because the general form is an existential
introduction over any value, which means every `Int` in the program is a
potential type-level binder and the checker has to decide where the
existential's scope ends for values it did not introduce. The shape-only form has
one scope rule (narrowing two) and one error (`SC0295`/`SC0296`).

**Narrowing two: a value escapes the block iff its type mentions no bound
parameter.** This settles `broadcasting.md` §13's open question — *"Does `with`
interact with regions correctly?"* — in the restrictive direction.

```science
    with table.shape as (rows, cols):
        let total be table.sum()            # F32 — mentions nothing — escapes
        let count be rows * cols            # Int at run time — escapes
        let z be standardise(table)         # Tensor of (F32, (rows, cols)) — does not
```

`z` cannot be assigned to a binding declared outside, cannot be returned, and
cannot be stored in a field of a value that outlives the block. The rule is one
predicate over a type and it is checkable in the same pass that checks the block.

**The cost, stated plainly.** A `with` block that computes a tensor must consume
it inside: reduce it, write it, print it. The pipeline of `broadcasting.md` §10
does exactly that and it is representative — read, check, compute, summarise,
write. But a program that wants to *return* the standardised table from a function
that read it cannot, and that is a real limitation that will be hit in the first
week. The F2 answer is an existential shape — the tensor escapes with its symbolic
dimensions replaced by fresh opaque atoms whose only property is that they equal
themselves — and it is a second mechanism this note does not design. §13.

**Rejected: shape-erasing the value on the way out.** It would work, and it means
a second tensor type at the boundary, which `broadcasting.md` Decision 6.1 rejects
on grounds this note agrees with entirely.

### 8.2 Decision — units have no `with`, and the asymmetry is the point

> **Decision 8.2.** There is no dynamic-dimension escape hatch for `Quantity`. A
> quantity whose dimension is not known statically is constructed through a
> **typed parse**, where the caller writes the dimension and the run time checks
> that the text agrees.

```science
let g, err be units.parse of Acceleration of F64 ("9.80665 m/s^2")
if err?:
    ...
```

The asymmetry with §8.1 is deliberate and rests on a fact about the two domains.
**A shape's extents are genuinely unknown to the programmer** — the file has as
many rows as it has, and nobody knows the number, including the person who wrote
the program. **A quantity's dimension is always known to the programmer** — you
are parsing a velocity or you are not; you wrote the column header. So the shape
case needs a binder that introduces an unknown, and the unit case needs a check
against something the caller already knows.

Giving units a `with` would mean a const parameter bound to one of seven exponents
of a dimension nobody can name, with a diagnostic that cannot say what went wrong
because it does not know what was wanted. Rejected.

### 8.3 What `with` costs the const layer, specifically

One thing, and it is small: **a const parameter can now be introduced by a
statement, not only by a declaration's `of` list.** So the atom order of §3.1 must
be stable for `with`-bound parameters too — they are ordered after all
declaration-bound parameters, by the source position of their binder, which is
stable. And `SC0261`'s rendering must be able to point at a `with` head as a
parameter's definition site, which it does by the same "defined here" mechanism
every other binding uses.

---

## 9. Diagnostics

This note claims `SC0260`–`SC0262`, the block allocated to it in
`docs/superpowers/design/README.md`. Checked free against `SC0250`–`SC0259`
(`unit-literals.md`, `stdlib-core.md`, `models-and-inference.md`),
`SC0263`–`SC0279` and the `SC0280`–`SC0298` blocks
`indexing-and-array-literals.md` and `broadcasting.md` hold.

| Code | Severity | Condition |
|---|---|---|
| `SC0260` | error | A const expression the language does not admit: an operator outside §2.1, `*` or `/` with no literal operand, division by zero, a non-integer or suffixed literal, a const-parameter kind other than `Int` or `Shape`, or an operator in the bare `of X` form (§2.4). |
| `SC0261` | error | Two const expressions cannot be shown equal. Prints **both normal forms**, the legend binding each symbol to its definition site, and — where they differ only in a quotient atom — the note from §4.3. |
| `SC0262` | error | A const expression is inadmissible at this instantiation: a negative extent, or no integer solution at an inference site (§7.1). Carries the **instantiation chain**. |

### 9.1 The division of labour with the two neighbouring notes, so nothing is reported twice

`broadcasting.md` §9 owns `SC0290`–`SC0298`, and its `SC0294` is *"a symbolic
dimension pair that cannot be shown equal."* `unit-literals.md` §10 owns
`SC0256`, *"dimension mismatch in `+`, `-`, or an ordering comparison."* Both are
`EQUAL` returning false. Three codes for one condition would be a diagnostic
disaster.

> **Decision 9.1.** `SC0261` is the **expression-level** failure and it is
> reported only when no higher-level code owns the site. When a shape comparison
> fails, `broadcasting.md`'s `SC0294` is reported and carries `SC0261`'s
> normal-form block as a note. When a dimension comparison fails,
> `unit-literals.md`'s `SC0256` is reported and carries the derivation of §9.3 as
> a note. **One code per user-visible mistake; the const layer contributes the
> explanation, not a second error.**

The same applies to `SC0262`: where `unit-literals.md` §10's `SC0254` describes
the condition better — `sqrt` of an odd dimension — that code is reported and
`SC0262`'s divisibility line becomes its note.

### 9.2 The shape case, rendered in full

This is `broadcasting.md` §9.1's alignment table with the piece this note owes it:
**where each symbol came from, and what the checker's normal forms were.** Without
that block, a user staring at `n` and `m` in a table has no way to find out why
the compiler thinks they are different.

```
error[SC0294]: these dimensions cannot be shown equal
  --> pipeline/standardise.science:52:24
   |
52 |     let residual be block .- reference
   |                     ----- ^^ ---------
   |                     |        |
   |                     (n, 768) (m, 768)
   |
   = the operands are aligned from the right:

         axis      0      1
         left      n    768
         right     m    768
                   ^
   = axis 0: neither dimension is the literal 1, so neither stretches, and
     the two are not equal after normalisation

   = the compiler compared these two const expressions:

         left    n        normalised   n
         right   m        normalised   m

     `n` and `m` are distinct const parameters, so no assignment of values
     makes them equal for every instantiation

   = where each came from:

  --> pipeline/standardise.science:44:26
   |
44 |     with block.shape as (n, 768), reference.shape as (m, 768):
   |                          -                            -
   |                          |                            |
   |                          `n` bound here                `m` bound here
   |
help: if these two are meant to have the same number of rows, bind them with
      one name — the repeated binder becomes a single runtime check
   |
44 |     with block.shape as (n, 768), reference.shape as (n, 768):
   |                                                       ^
```

Four things that rendering does which "mismatched types" does not. It names **the
axis**. It shows **the alignment and the padding**, which is
`broadcasting.md` §9.1's contribution. It shows **the normal forms**, so a user
who wrote `k + 1` and `1 + k` can see at a glance that those are not the problem.
And it shows **where each symbol was bound**, which for a `with`-bound parameter
is the only way to find out, because the name appears nowhere in a signature.

The `help` is the load-bearing line. It is the difference between a compiler that
enforces a rule and one that finds the bug, and the bug here — two files that are
supposed to have the same number of rows — is the one `broadcasting.md` §1.1 opens
with.

### 9.3 The unit case, rendered in full

`unit-literals.md` §10.1 establishes that `Quantity` must never be printed as its
exponent vector, and that it prints as an alias name where one exists and as a
unit string otherwise. This note adopts that requirement in full and adds one
thing to it: **when the mismatched dimension was *computed*, show the
computation.** A user who multiplied three things and got a dimension they did not
expect needs to see which factor contributed what.

```
error[SC0256]: these have different dimensions and cannot be added
  --> flow/drag.science:31:22
   |
27 | let area be width * height
   |             ---------------  Area — m²
28 | let speed be flow / area
   |              ----------     Velocity — m/s
29 | let pressure be density * speed * speed
   |                 -----------------------  Pressure — kg/(m·s²)
30 |
31 | let total be pressure + drag
   |              -------- ^ ----
   |              |            |
   |              Pressure     Force — kg·m/s²
   |              kg/(m·s²)
   |
   = the two dimensions differ in two exponents:

         base       length   mass   time   current   temp   amount   lum
         Pressure       -1      1     -2         0      0        0     0
         Force           1      1     -2         0      0        0     0
                         ^
   = the length exponent of `pressure` was computed:

         density              length  -3     (kg/m³)
         speed                length   1     (m/s)
         speed                length   1     (m/s)
                              ------------
         density * speed * speed      -1

  --> flow/drag.science:29:17
   |
29 | let pressure be density * speed * speed
   |                 ^^^^^^^  the -3 comes from here
   |
help: a pressure and a force differ by an area. If `drag` is a force over the
      whole surface, `drag / area` is a pressure
```

The derivation block is what this note adds and it is the reason the feature is
worth the investment. The exponent arithmetic happens invisibly across three
lines; when it comes out wrong, the user has no way to see which of the three
contributed the wrong exponent, because no intermediate type was ever written
down. **Printing the sum, with one row per contributing operand and its span, is
the only rendering that answers the question the user actually has.**

The alias-and-unit-string printing of `unit-literals.md` §10.1 is reused, run at
diagnostic time; the derivation rows come from the normal form's term list, where
each term already carries the span of the argument that contributed it. That is
the one requirement this note places on the normaliser's representation: **terms
carry provenance spans**, and they are carried through `MERGE` and `SCALE`
unchanged. It costs one field.

### 9.4 The instantiation-chain case, rendered

`SC0262` is the cost of Decision 5.3, so its rendering is what makes that decision
survivable.

```
error[SC0262]: this instantiation gives an extent of -1
  --> analysis/window.science:17:19
   |
17 |     let tail be samples[2..]
   |                 ^^^^^^^^^^^^ result extent is `N - 2`, which is -1 here
   |
   = `N` is 1 at this instantiation
   = the instantiation chain:

         analysis/window.science:17  trailing_window of (N = 1)
         analysis/main.science:9     trailing_window(first_row)
         analysis/main.science:8     `first_row` has type Tensor of (F32, (1))

help: the extent is not checked in the generic — it is checked here, per
      instantiation. Guard the caller, or take the slice with `get_range`,
      which returns an empty tensor instead of failing
```

The chain is the whole message. A post-monomorphization error without one is the
failure mode §5.1 says Rust designed its type system to avoid; with one, it is an
ordinary compile error at the caller with the library's part of the story
attached.

---

## 10. What lands when

The type checker starts as soon as this note is accepted, so the F0 half is a
hard commitment and is written as one.

### 10.1 F0 — required before the type checker is written

| # | Commitment | Why it cannot wait | Cost |
|---|---|---|---|
| 1 | **The const-expression grammar of §2.1 parses in type-argument position**, including unary negation. | `scientific-libraries.md` §12.3's nine unit aliases do not parse today (§1.2.1). It is an AST change and it moves parser snapshots (§1.2.2, §2.5). | ~150 lines of parser, one AST variant, one snapshot sweep |
| 2 | **The const-argument node carries a signed integer.** | `Literal::Int { value: u128 }` cannot hold `-1`. | folded into 1 |
| 3 | **`NORMALISE` and `EQUAL` over the quotient-free fragment** (§3), including the compilation-stable atom order and provenance spans on terms. | F0 already has `Matrix of (T, ROWS, COLS)`; the checker needs const equality on day one, and it must be the implementation the rest of this note extends. | ~600 lines with tests |
| 4 | **The normal form is the monomorphization key** (§3.5 item 4). | Otherwise `a * b` and `b * a` emit two symbols for one function, which is a codegen bug discovered late and fixed by rewriting the key. | small, if done now |
| 5 | **Const-parameter kinds are a closed enum**, `Int` today, with room for `Shape` (§2.3). | So the annotation position is a kind position from the first commit rather than a type position retrofitted into one. | one enum, one checker arm |
| 6 | **Generic argument lists admit variadic arity** (`broadcasting.md` §11.1). | The structural ask. If the resolver checks arguments against a fixed count, F1 cannot express rank-polymorphic shapes without reopening the resolver and every snapshot. | resolver and HIR; the largest of the six |
| 7 | **One-variable linear matching for const-argument inference** (§7.1). | The alternative is that every call to a const-generic function spells its const arguments, and every signature in `scientific-libraries.md` §6.7 becomes unwritable at the call site. | ~150 lines |
| 8 | **`SC0260` and `SC0261`**, with the normal-form-and-legend block of §9.2. | A diagnostic added after the checker exists is a diagnostic that renders whatever the checker happens to have kept. Provenance spans (§9.3) have to be in the representation from the start. | ~300 lines |

Items 1, 2, 5 and 6 are the ones that are cheap now and expensive later, in §2 of
the core spec's sense. Items 3, 4, 7 and 8 are ordinary work that has to happen
before the checker is finished regardless.

### 10.2 F1 — with tensors

- Quotient atoms and `/` (§4).
- The `Shape` kind, the seven operations of §6.2, `EQUAL_SHAPE`, and the
  rank-1 lift of §6.4.
- `with … as …` (§8.1), including the escape rule and `SC0295`/`SC0296` from
  `broadcasting.md` §9.
- `SC0262` and the monomorphization-time obligations (§5.3, §9.4).
- The derivation rendering of §9.3, which needs the units library to exist.

### 10.3 Deferred, and named so the deferral is a decision

- **Sign or range kinds on const parameters** (§4.2). A GPU backend will ask.
- **Const predicates in `where`** (§5.3). This is the Rust-scale subsystem.
- **Broadcast constraints** — `where (N, 3) broadcasts (M, 3) giving S` —
  which `broadcasting.md` §5.4 rejects for F1 and §13 predicts will be demanded
  within a month of shipping.
- **Existential shapes escaping a `with` block** (§8.1). The known-missing piece
  of the dynamic story.
- **Rank polymorphism** (§6.1). No customer yet; `reshape` does not need it.

---

## 11. What this note asks of other notes

Named sections and reasons, per the house rule.

1. **`scientific-libraries.md` §12.4** — the sentence *"Three things are
   missing"* should become four: §7 of this note adds const-argument inference,
   without which none of §12.4's signatures is callable. And point 3's framing —
   *"this is where Rust's const generics stalled for years"* — should be
   corrected against §5.1 here: normalising `L1 + L2` against `L2 + L1` was not
   the stall. The stall was the predicate layer, which this note does not build.
   The note's conclusion is unchanged and better supported.

2. **`scientific-libraries.md` §12.5** — the sentence *"units need only linear
   combinations"* is correct, and its reason should be the one in §7.2 here
   rather than an absence of use cases: `sqrt` does need halving, and it gets it
   through the parameter position instead of through division. Also, §12.5 should
   record that the division it asks for is **floor division with an opaque atom**,
   not a rational coefficient, and that the rational form is unsound against §5.1
   (§4.1).

3. **`scientific-libraries.md` §6.1** — seconding `broadcasting.md` §12 item 2:
   `Matrix` and `Vector` are aliases for rank-2 and rank-1 `Tensor` (§6.1 here).
   No signature in §6.7 changes.

4. **`scientific-libraries.md` §5.6 and §12** — `Quantity` gets `squared`,
   `cubed`, `sqrt` and `cbrt` and does **not** get `pow of (const K: Int)`,
   because `L * K` for two parameters is outside §2.1's grammar (§2.2). One
   sentence in the catalogue.

5. **`unit-literals.md` §9.2** — stage C is unblocked by §10.1 of this note plus
   the `Quantity` library, with no further language work. Its stage boundary is
   accurate and its §9.3 amendment to the wave table stands. It should add that
   `SC0254` is the surfacing of §7.2's divisibility failure here and that the
   units checker raises `SC0254` rather than the generic `SC0262` (§9.1).

6. **`unit-literals.md` §10.1** — its ask on the type printer is adopted in full
   and extended: the printer must also render a const expression in normal form
   with a symbol legend (§9.2), and a computed dimension with its derivation
   (§9.3). The derivation needs provenance spans on normal-form terms, which is
   §10.1 item 3 here.

7. **`broadcasting.md` §5.2 and §11.1** — adopted without change. §5.3 is adopted
   and §6.3 here adds the clarification that `max` over *ranks* is a compiler-side
   maximum of two literal naturals and is not the `max` that is banned; that note
   should carry the sentence, because the two uses of the word are one line apart
   in its §4.2 and §5.3.

8. **`broadcasting.md` §6.2, §6.3 and §13** — `with` is adopted with the two
   narrowings of §8.1. Its §13 open question *"Does `with` interact with regions
   correctly?"* is **answered**: a value escapes iff its type mentions no bound
   parameter, and the existential alternative is deferred to F2 and named. That
   open question can be closed and replaced with a pointer here.

9. **`indexing-and-array-literals.md` §8 item 10** — **accepted, and this is the
   note that discharges it.** The **Constant** and **Shape equality** rules of its
   §1.3 are `EQUAL` and `EQUAL_SHAPE` from §3.3 and §6.5, one implementation, and
   its `SC0286` is reported when they return false rather than a separate bounds
   analysis being written. Its §2.1 `samples[2..]` result extent is `N - 2` and
   its negative case is §9.4's `SC0262`, not a clamp (§4.5).

10. **The core spec §5.3** — gains the const-expression grammar of §2.1 and the
    kind restriction of §2.3. §5.2's *"local inference"* gains §7.1 as its
    const-parameter clause. §12's line *"Units of measure. Wanted, and a separate
    unifier over a free abelian group; after shapes"* should be amended: **there
    is no separate unifier.** The free abelian group over the seven base
    dimensions is the quotient-free fragment of §3.1's normal form, and units are
    a library over the same machinery shapes use. That is `scientific-libraries.md`
    §12.5's argument, and §12 of the spec currently contradicts it.

11. **`crates/science-resolve/src/hir.rs`** — the comment on `DefKind::is_type`
    documenting the const-parameter-in-type-position compromise should gain a
    sentence recording that §6.4 here extends it by one case (a parenthesised list
    in an argument position is a tuple type or a shape according to the declared
    kind) rather than introducing a second compromise. **No code change is
    requested and none is made by this note.**

12. **`docs/superpowers/design/README.md`** — the notes index gains a row for this
    note, `SC0260`–`SC0262` moves out of the Types "free" list, and the standing
    cross-note ask **"Const-expression arithmetic in type position"** can record
    that it is now owned rather than outstanding. The ask **"Explicit type
    arguments at call sites"** gains this note as a third asker, and §7.3 makes it
    load-bearing rather than convenient: without it a const parameter appearing
    only in a return type cannot be supplied at all.

---

## 12. Risks

**The post-monomorphization error is the thing that gets this design attacked,
and the attack is legitimate.** Decision 5.3 says Science does not promise that a
generic item which typechecks is free of const-expression errors in every
instantiation. A library author will ship a signature that breaks for a caller's
shape, and the caller will experience a compiler error inside code they did not
write. §9.4's instantiation chain is the entire mitigation and it is a
diagnostic-quality mitigation, not a type-system one. **If the chain rendering is
not built to the standard of §9.4, this decision is indefensible and should be
revisited before the predicate layer becomes impossible to add.** The retreat
direction is good: const predicates in `where` can be added later and every
program that compiles today keeps compiling, because a predicate only ever
rejects instantiations that would have failed at monomorphization anyway.

**The incompleteness of `EQUAL` is invisible until it is not.** §4.3 says the two
false negatives require dividing the same dimension twice by different routes, and
that no current signature does that. That is an argument from a catalogue written
before this feature existed. The first library author who writes a strided
convolution — output length `(N - K) / S + 1` — will compose divisions, and if two
such expressions must be shown equal, `SC0261` fires on something the author
believes is obviously true. The mitigation is that `SC0261` prints both normal
forms and says which atom it could not relate, so the workaround (bind the
quotient to a parameter once and reuse it) is visible in the message. **The
measure of whether this risk is real is one experiment: write `signal`'s strided
and dilated convolution signatures against §2.1's grammar, before the checker
ships.** If they need a fold the normaliser does not have, that is the moment to
learn it.

**§10.1 item 6 — variadic arity — is the only structural commitment, and it is
the one most likely to be quietly dropped.** It is the largest of the eight, it
buys nothing visible in F0, and the F0 type checker can be written without it and
will pass its tests. `broadcasting.md` §15 says the consequence: if it is not
honoured, that note's entire §4 becomes an F2 feature and
`scientific-libraries.md` §13's wave 3 — all of `linalg` — moves with it. **It
should be in the F0 definition-of-done list (§11 of the core spec), which
currently names const generics but not their arity.**

**The two-layer story may not survive contact with `linalg`.** §6.3 asserts that
the shape layer never constructs a const expression, on the strength of
`broadcasting.md` Decision 5.3. That is true for broadcasting. It is not obviously
true for `concatenate`, whose output extent is `A + B` — a *new* expression, built
by a shape-level operation. This note's answer is that `concatenate` is an
ordinary function whose signature *writes* `A + B`, so the expression comes from
the source and not from the shape layer; the shape layer still only rearranges.
That answer holds for every signature in the catalogue today and it is a property
of the signatures, not a theorem. **If a future operation needs the checker to
synthesise an extent, the layering claim of §6.3 is what breaks**, and with it the
argument that the expression layer can ship in F0 alone.

**`sqrt`-through-the-parameter-position (§7.2) is clever, and clever is a risk.**
It makes a signature that reads oddly — `L * 2` in the parameter type, `L` in the
return type, which is backwards from how anyone thinks about square roots — and it
puts an error at a call site whose cause is a doubling written in a library. It is
right, it produces the best available message, and it will confuse the first
person who reads the signature. **The mitigation is documentation, which is the
weakest kind**, and the alternative — an exactness obligation on `/`, or a second
division operator — was priced in §4 and is worse.

**Nothing here has been tried against a real program.** Every decision in this
note is derived from signatures in four design notes, and no Science program using
const arithmetic exists, because the type checker does not. The normal form is
correct, the equality procedure is correct, and whether the *language* is usable —
whether `with` is bearable, whether §9's messages read well, whether §2.4's
parenthesis rule annoys people — is unknown and will stay unknown until F1 ships.
**The single cheapest way to reduce this risk is to write the `linalg`, `signal`
and `physics` signature sets out in full, against §2.1's grammar, as a document,
before the checker is built.** They are perhaps two hundred lines and they are the
only corpus this feature will have before it is expensive to change.

---

## 13. Open questions

**Do const parameters get sign or range information?** §4.2 rejects it for F0 and
F1. It buys one more fold in `DIVIDE`, it removes a whole class of `SC0262`s at
monomorphization, and it costs a kind lattice. A GPU backend doing tiling
arithmetic in F2 will ask, and the answer will be easier if the kind enum of
§2.3 was built as an enum rather than as two hard-coded cases.

**Do const predicates arrive in `where`?** §5.3 defers it, §12 says the retreat
direction is safe, and §5.1 says it is what has kept Rust's version unstable for
six years. The question is not whether it is wanted but whether a *restricted*
form — linear inequalities over the same normal form, which is a decidable
fragment — is enough for the cases that matter, and that question is answerable
with a survey of what `SC0262` actually fires on in the first year.

**Does a shape escape a `with` block?** §8.1 says no in F1 and names the
existential as the F2 answer. The fresh-opaque-atom formulation would fit §3.1's
atom set exactly — an existential dimension is an atom equal only to itself, which
is what a `with`-bound parameter already is — so the representation may already be
right and only the scoping is missing. That is worth checking before F2 starts,
because if it is true the feature is much smaller than it looks.

**Is rank polymorphism ever needed?** §6.1 says no and every signature in the
catalogue agrees. A generic `sum_all` over any rank would want it. So would a
serialisation format. Neither is pressing.

**Does `concatenate` break the layering?** §12's fourth risk. The answer today is
no, and it rests on a property of the signatures rather than on a theorem, and
somebody should look for the counterexample deliberately rather than waiting for
it.

**Where does the strided-convolution signature land?** §12's second risk names
this as the one experiment that would falsify §4.3's claim that the
incompleteness does not bite. It costs an afternoon and it should be done before
the normaliser is called finished.
