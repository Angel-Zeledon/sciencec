# Science — Design: element-wise operations and broadcasting

Date: 2026-09-16
Status: draft for review
Phase: **F1**. What F0 must not preclude is in §11.
Syntax: `syntax-revision-2.md`. Every sample here is revision-2 Science.
Related and depended on: `scientific-libraries.md` §12.4–§12.5 (const-expression
arithmetic in type position — this note is the second consumer of that feature
and does not re-derive the argument); `collections-and-chains.md` §1.4 and §7
(the chain vocabulary and the parallelism commitment); `python-interop.md` §4
(DLPack, and the requirement that a tensor's runtime header is field-for-field a
`DLTensor`); `reserved-words.md` §3 and §6 (this note answers that note's open
question, in §11.7).

---

## 0. The one sentence

> **Broadcasting is the most productive idea in scientific computing and one of
> its two or three most reliable sources of silent wrong answers. Science keeps
> the productivity and takes away the silence by making the programmer write one
> character.**

That character is a leading `.` on the operator, as in Julia: `a .+ b`. The
undotted `a + b` requires the shapes to be equal and the compiler checks it
before the run starts. This note argues that decision, states the broadcasting
rule as an algorithm, works out exactly what the type checker must gain to run
it, and specifies the bridge between statically known and runtime-discovered
shapes — which is the part that decides whether any of this survives contact
with a CSV file.

---

## 1. The problem, stated fairly

### 1.1 The bug

```python
# NumPy. This runs. It produces numbers. They are wrong.
residual = predictions - targets      # (100, 1) and (100,)
```

`predictions` came back from a model with a trailing singleton axis.
`targets` is flat. NumPy right-aligns the shapes, pads the shorter with a 1, and
stretches: `(100, 1)` against `(1, 100)` is `(100, 100)`. Ten thousand
residuals where a hundred were wanted. Nothing raises. The mean of that matrix
is a real number, the loss goes down, and the mistake is found — if it is found
— three weeks later, by someone who notices the training curve is too smooth.

This is not a rare accident. It is the single most reported class of NumPy and
PyTorch bug, it survives code review because the code looks correct, and it
survives testing because the program does not crash. §1 of the core spec names
the target: *"the mistakes a compiler can catch before a run starts"*, and names
the competition: *"Julia has the REPL and the ecosystem and cannot check a shape
[...] before running. Mojo has ownership and GPUs and no shapes in its types."*
A shape bug that produces wrong numbers rather than an exception is the exact
shape of the thing Science exists to catch. If Science does not catch this one,
it is not clear what it is for.

### 1.2 The case for implicit broadcasting, put as strongly as it deserves

It would be easy to write the rest of this note as though NumPy's designers had
been careless. They were not. Implicit broadcasting is why NumPy won, and the
arguments for it are good ones.

**It makes numerical code look like the mathematics.** Standardising a design
matrix is `(x - x.mean(axis=0)) / x.std(axis=0)`. That is how a statistician
writes it on a board. Every dot inserted into that line is a piece of
bookkeeping the reader has to look past to find the formula. A language for
scientists that makes the formula harder to see has taken something real.

**It makes ported code port.** There are, conservatively, a hundred million
lines of NumPy, PyTorch and JAX in the world, and every one of them is a
potential Science program. If `+` broadcasts, a port is a transliteration. If it
does not, every port is an edit — and an edit made by someone who has to decide,
line by line, whether the original author meant same-shape or meant broadcast,
which is information the original source does not carry.

**The audience is not asking for this.** MATLAB, NumPy, R and PyTorch all
broadcast implicitly. Four ecosystems, four decades, and their users have voted
with their fingers. Julia is the exception, and Julia's dots are a recurring
complaint from newcomers — not from Julia veterans, who like them, but the
newcomers are the population Science has to win and does not yet have.

**Machine-written code has seen ten million lines of NumPy and zero of
Science.** This is the 2026 argument and it is the strongest one. A model
writing Science will write NumPy habits in Science syntax. Every construct
Science adds that NumPy lacks is a construct the model gets wrong on the first
attempt.

### 1.3 Where each argument breaks

The first three are real costs and this note accepts them as costs, not as
refutations, on one ground: **implicitness is not what makes the formula
readable; it is what makes the bug invisible.** `(x .- x.mean(axis: 0)) ./
x.deviation(axis: 0)` is the same formula with two more characters. The reader
who could read the NumPy can read this. What has changed is that the line above
it — the one where `predictions` had an extra axis — no longer compiles.

The port argument is real and it is one-directional in a way worth naming: a
port that must insert dots is a port during which a human or a compiler reads
every arithmetic line in the file. Some of those lines contain the bug. That is
not a pure cost.

The fourth argument inverts on inspection, and this is the part worth being
loud about. **For machine-written code the case for explicit broadcast is
stronger, not weaker.** A model that forgets a dot gets a compile error with a
suggested fix, pays a few seconds, and is corrected by the tightest feedback
loop that exists. A model that produces a silent `(100, 100)` produces a program
that compiles, runs, prints plausible numbers, and passes the tests it also
wrote. There is no loop that catches that. The failure mode a compiler can fix
is the cheap one; the failure mode Science must not ship is the one where a
confident, fluent, well-commented wrong program is indistinguishable from a
right one.

### 1.4 What the dot actually says

`a .+ b` says *I know these shapes differ and I meant the stretch.* `a + b` says
*these are the same shape.* Both are assertions, both are checked, and the
programmer chose which one to make. NumPy offers no way to make the second
assertion at all — which is why, in NumPy, there is no way to write down the
thing you meant.

---

## 2. The two operations

### 2.1 Decision — the rule that governs every operator

> **Decision 2.1.** An operator is written undotted when its result shape is
> determined by one operand alone. It takes a leading `.` when the result shape
> is a function of both.

Three cases satisfy the undotted condition and they are the whole list:

| Case | Example | Result shape | Why it is safe |
|---|---|---|---|
| Equal shapes | `a + b`, both `(n, 768)` | `(n, 768)` | The shapes are equal; there is nothing to get wrong. |
| One operand is a scalar or rank-0 | `a * 2.0`, `a + 1.0`, `2.0 * a` | `a`'s shape | A scalar has no shape. No arrangement of a scalar and a tensor produces a shape the tensor did not already have. |
| Unary | `-a`, `not mask` | `a`'s shape | Shape-preserving by construction. |

Everything else — two tensors whose shapes are not equal — takes the dot.

**The scalar carve-out is deliberate and it is the concession to §1.2.** It is
free: the safety argument of §1.1 is entirely about a result shape nobody
intended, and a scalar operand cannot produce one. It also removes most of the
"extra character on every line" tax, because scalar-and-tensor is the commonest
mixed-shape operation in real code by a wide margin. `x * 2.0`, `x + epsilon`,
`1.0 / x`, `x ** 2.0` all stay undotted.

**Alternatives rejected.**

- *Require the dot for scalars too, as Julia does for `+`.* Julia's rule is
  derived from the mathematics: `A + 1` is not defined for a matrix. That is
  correct mathematics and the wrong rule for this language, because it charges
  the full ergonomic price for zero safety. APL, MATLAB, NumPy, R and PyTorch
  have all meant element-wise-with-scalar by `A + 1` for sixty years, the
  reading is unambiguous, and no shape bug lives there. *Serves scientists.*
- *Make `+` broadcast and add a separate strict operator.* This is the wrong
  default. The defaults in a language are what gets written when nobody is
  thinking, and the case where nobody is thinking is the case that must be safe.
  *Serves both.*

### 2.2 Decision — what `a + b` does when the shapes merely broadcast

> **Decision 2.2.** `a + b` where the shapes are compatible but not equal is a
> **compile error**, `SC0290`, whose message names the broadcast result shape it
> would have produced and offers `.+` as an automatically applicable fix.

This is the whole decision of the note and everything else follows from it.

The error must show what the implicit reading *would have been*, because the
population that hits it most is the population porting NumPy, and that
population needs to see `(100, 1) + (100,)` become `(100, 100)` written down.
Half of them will apply the fix; the other half will discover their bug. Both
outcomes are good, and only one of them is available in NumPy. *Serves both,
and the diagnostic is what makes it serve scientists rather than merely
constrain them.*

**Alternative rejected: warn instead of error.** A warning that fires on a
correct and common idiom is a warning users learn to turn off; once it is off,
Science is NumPy with more syntax. A warning would also be un-suppressible per
call site without a mechanism that does not exist.

**Alternative rejected: error only when both operands stretch** (the true
`(3,1)`-against-`(1,3)` outer-product signature), and broadcast silently when
only one stretches. This is tempting — it targets precisely the notorious case
and leaves `x - x.mean(axis: 0)` alone. It was rejected because the rule cannot
be explained in one sentence, and a shape rule that a user cannot recite is a
shape rule a user cannot rely on. The idea survives in a weaker form as a
warning on the dotted operator: see §3.4.

### 2.3 What `.+` costs, stated plainly

Four things, none of them free.

1. **A character per line on the lines that broadcast.** Real, unavoidable, and
   the single most likely reason for this design to be rejected by its users.
   §12 says what the retreat looks like.
2. **`a .+ b` on equal shapes is legal** and means the same thing as `a + b`.
   That is two spellings for one behaviour on that input, which is the debt
   `syntax-revision-2.md` §1 exists to refuse. It is answered in §2.4.
3. **Generic library code must choose.** A function over
   `Tensor of (T, (N, M))` and `Tensor of (T, (N, M))` writes `+`. A function
   that means to accept a row vector must write `.+` and say so in its
   signature. That is a real API design burden placed on library authors, and it
   is where the feature is felt first.
4. **Every ported line is an edit**, per §1.2.

### 2.4 Why `.+` on equal shapes is not the duplicate-spelling problem

`syntax-revision-2.md` §1 removed `==` beside `is` because the two were the same
operator with two names and a formatter could not canonicalise between them
without running the type checker first. The concern applies here and has to be
answered rather than waved at.

`+` and `.+` are **different operators that happen to agree on one class of
input**, in exactly the way `>` and `>=` agree nowhere and `x + 0` and `x` agree
everywhere. There is no canonical form to normalise toward, because neither is a
special case of the other in the type system: `+` demands shape equality and
`.+` does not, so replacing one with the other changes what the program asserts.

**Consequence, stated so it is not discovered later: `sciencec fmt` has no rule
here and must never acquire one.** In particular there must be no lint
suggesting `+` where the shapes are statically equal, because in generic code
the shapes are *equal at this instantiation* and the author may have meant the
broadcast for the next one.

---

## 3. The operator set

### 3.1 Decision — which operators get a dotted form

> **Decision 3.1.** The `.` prefix composes mechanically with every binary
> operator that dispatches to an operator interface, including user-defined
> ones. It is not a fixed list.

| Undotted | Dotted | Result element type | Notes |
|---|---|---|---|
| `+ - ` | `.+ .-` | as `Add`/`Sub` | |
| `* /` | `.* ./` | as `Mul`/`Div` | see §3.2 |
| `%` | `.%` | as `Rem` | |
| `**` | `.**` | as `Pow` | see §3.2 |
| `> < >= <=` | `.> .< .>= .<=` | `Bool` | dotted form yields a `Bool` tensor |
| `is` / `is not` | `.is` / `.is not` | `Bool` | see §3.3 |
| `and or not` | — | — | see §3.5 |
| `@` | — | — | never dotted; `@` is matrix multiply (§4.6) |

`.op` is defined for any `T` and `U` implementing the corresponding interface
from §5.4 (`Add Sub Mul Div Rem Pow Neg Eq Ord`). A user type that implements
`Add` gets `.+` over tensors of it for free, and that is the point of
mechanising it: a fixed list would mean a `Quantity of (F64, …)` from
`scientific-libraries.md` §12.3 could be added to itself but not broadcast, for
no reason a user could discover.

**Alternative rejected: a fixed list of dotted operators.** Smaller lexer,
smaller grammar, and it breaks user types silently. The cost of the general rule
is entirely in the lexer (§3.6) and it is small.

### 3.2 Decision — `*` on two tensors is not defined at all

> **Decision 3.2.** Between two tensors, `*` and `/` and `**` are **type
> errors** (`SC0292`). Element-wise is `.*`, `./`, `.**`. Matrix product is `@`.
> Matrix power is `linalg.matrix_power`.

This is the oldest argument in array programming and it has no winning side.
NumPy's `*` is element-wise and its users are certain that is right. A
mathematician reads `A * B` on matrices as the matrix product and MATLAB agrees
with the mathematician. Whichever Science picks, half its audience arrives with
the opposite reflex, and gets **wrong numbers, silently**, for every square
matrix — because element-wise and matrix product of two `(n, n)` matrices have
the same shape, so no shape check can save them. That is worse than the
broadcasting bug: it is the same class of failure with the type system unable to
help.

So Science defines neither. `a .* b` is element-wise and says so. `a @ b` is the
matrix product and §4.6 of the core spec already gave it that meaning. `a * b`
between two tensors is a compile error naming both. The MATLAB user and the
NumPy user each get one error, once, and then they are correct forever.

*Serves both, and it is the only decision in this note that is strictly better
for scientists than what NumPy offers.*

Note that Decision 2.1 keeps `2.0 * a` and `a * 2.0` working: those are scalar
multiplication, mathematically defined, shape-determined by one operand.

### 3.3 Decision — `.is` exists, and it is ugly

> **Decision 3.3.** Element-wise equality is `a .is b`, yielding a `Bool` tensor.
> Whole-tensor equality remains `a is b`, yielding a `Bool`, and requires equal
> shapes.

`.is` reads badly. `mask be scores .is 0.0` is not a sentence anybody wants to
read aloud. It is chosen anyway, for one reason: Decision 3.1 says the dot
composes mechanically, and `is` is an operator in revision-2 that happens to be
spelled with letters. A rule with one exception is a rule users must memorise
twice.

**Alternative rejected: methods, `a.equal_to(b)` and `a.not_equal_to(b)`.** This
reads better in isolation and worse in place — `(a .> lo) and (a.equal_to(b))`
mixes two grammars in one predicate, and a reader now has to know that `.>` is
an operator and `equal_to` is a method with the same standing. It also means
mask-building code, which is dense with combined predicates, has no single
shape.

**The cost is accepted and it is the weakest decision in this note.** If it
proves intolerable in practice the retreat is to add the methods as an *alias*
for `.is`, which is source-compatible; the retreat that is not available is
removing `.is` after code depends on it.

`a .is not b` is one operator, parsed as the existing `is not` sequence with a
dot prefix, exactly as `a is not b` is parsed today.

### 3.4 Decision — a warning for the accidental outer product

> **Decision 3.4.** `SC0293`, a **warning**, fires when a dotted operator
> stretches each operand on a different axis, with neither operand
> rank-padded — the `(m, 1) .* (1, n)` signature. The message names the result
> shape and the `outer` helper.

This is the residue of the alternative rejected in §2.2, kept where it belongs.
The dot has already established that the author meant a broadcast. What it has
not established is that they meant a result *larger than either input*, which is
a different and much rarer intention. The warning is silenced by writing
`a.outer(b, each_times_each)` or by annotating the binding with its shape.

It is a warning and not an error because the outer product is a legitimate,
frequently wanted operation and `a .* b.transpose()` is how half the world
spells it.

### 3.5 `and`, `or`, `not` are not dotted

`and` and `or` short-circuit, and short-circuiting is meaningless element-wise.
Element-wise logic on `Bool` tensors is `.&`, `.|` and the unary `not` (which is
shape-preserving, so Decision 2.1 exempts it). `&` and `|` on two whole tensors
require equal shapes, as every undotted binary operator does.

### 3.6 Lexing, and one interaction that must be settled now

`.+` is **one token**, produced by longest-match, and this is the same rule that
`syntax-revision-2.md` §4 restored for `->`.

Two interactions, both benign, both worth writing down because discovering them
later is expensive:

1. **The chain-continuation rule.** §4.6 says a line beginning with `.`
   continues the previous line. A line beginning with `.+` also continues it, and
   there is no ambiguity, because `.+` is a single token and cannot be the start
   of a member access — no member name begins with `+`. So this is legal and
   means what it looks like:

   ```science
   let z be centred
       .* weights
       .+ bias
   ```

2. **Float literals.** §4.2 lists `3.14` and `1e-9` and does not admit a leading
   `.5`. Good: `a .* .5` would otherwise be a genuine ambiguity. **This note asks
   that the absence of the leading-dot float form be recorded as deliberate**
   rather than merely unlisted.

Whitespace is not load-bearing: `a.*b`, `a .* b` and `a.* b` are the same
program, because `.` followed by an operator character is always one token.

---

## 4. Broadcasting semantics

### 4.1 Decision — NumPy's rule, unchanged

> **Decision 4.1.** The alignment rule is NumPy's and PyTorch's, exactly:
> right-aligned, left-padded with 1, a dimension of 1 stretches.

Deviating would be the worst available outcome: a language whose users all
arrive knowing a rule, and which uses a *slightly different* one, converts every
piece of transferred knowledge into a trap. The safety in this design comes from
the dot, not from changing what the dot means.

### 4.2 The algorithm

Let `A = (a₁, …, a_m)` and `B = (b₁, …, b_n)` be the shapes of the two operands,
written outermost-first. `r = max(m, n)`.

```
BROADCAST(A, B):
  A' ← (1, …, 1, a₁, …, a_m)      # left-pad to length r
  B' ← (1, …, 1, b₁, …, b_n)      # left-pad to length r
  for i in 1 … r:
      let p = A'ᵢ, q = B'ᵢ
      if   IS_LITERAL(p) and IS_LITERAL(q):
              if p = q              → Rᵢ ← p
              else if p = 1         → Rᵢ ← q
              else if q = 1         → Rᵢ ← p
              else                  → INCOMPATIBLE at axis i
      else if p = 1 (literally)     → Rᵢ ← q
      else if q = 1 (literally)     → Rᵢ ← p
      else if NORMALISE(p) ≡ NORMALISE(q)
                                    → Rᵢ ← p
      else                          → UNDECIDED at axis i
  return R = (R₁, …, R_r)
```

Three properties of that procedure carry the whole of §5 and should be read
carefully.

- **`IS_LITERAL` and the test `= 1` are syntactic**, on the const expression in
  the type. A dimension that is the const parameter `n` is not 1 for this
  purpose even if the program is only ever run with `n` bound to 1.
- **`INCOMPATIBLE` is an error** (`SC0291`). **`UNDECIDED` is a different error**
  (`SC0294`), because the compiler has not proved the shapes wrong, only failed
  to prove them right, and the two must not share a message. §5.4 says what the
  programmer does about it.
- **The zero case.** `0` and `1` broadcast to `0`, by the rule as written
  (`q = 1 → Rᵢ ← p = 0`). `0` against `2` is incompatible. This matches NumPy,
  it is the behaviour empty selections depend on — `a[a > 1e9]` is a real and
  common shape — and `python-interop.md` §4.3 already commits to accepting
  zero-size arrays on import. It is stated here because a reader implementing
  from the prose would otherwise get it wrong.

### 4.3 Worked examples

| A | B | Result | Why |
|---|---|---|---|
| `(n, 768)` | `(768)` | `(n, 768)` | B padded to `(1, 768)`; axis 1 equal, axis 0 stretches |
| `(n, 768)` | `(n, 1)` | `(n, 768)` | axis 1 stretches from B |
| `(n, 768)` | `(n)` | **`SC0291`** | B padded to `(1, n)`; axis 1 is `768` against `n`, undecided unless `n ≡ 768` |
| `(100, 1)` | `(100)` | `(100, 100)`, `SC0293` | §1.1's bug, now visible and warned about |
| `(m, 1)` | `(1, n)` | `(m, n)`, `SC0293` | the outer product |
| `(8, 1, 6)` | `(7, 1, 5)` | **`SC0291`** at axis 2 | `6` against `5` |
| `(0)` | `(1)` | `(0)` | empty selection |
| `(n, 3)` | `(m, 3)` | **`SC0294`** | two distinct symbolic dimensions; §5.4 |

### 4.4 Where the rule is applied

The dotted operators, and nothing else. In particular **`@` does not broadcast
its matrix dimensions** in F1: `a @ b` requires `(M, K)` and `(K, N)` exactly.
NumPy's `matmul` broadcasts the leading batch dimensions and PyTorch's does too,
and batched matmul is genuinely wanted for anything with a batch axis — but it
is a distinct rule (last two axes matrix-multiply, leading axes broadcast) and
folding it into this note would mean `@` has broadcasting semantics while `*`
does not, which is unexplainable. **Batched matmul is deferred to a named
function, `linalg.batch_matmul`, and the operator form is left open.** Stated as
an open question in §13.

---

## 5. What the type checker needs

This is the heart of the note. `scientific-libraries.md` §12.4 identified
const-expression arithmetic in type position as the missing feature and §12.5
argued that shapes and units need the same machinery and so it should be built
once. That argument is accepted here and not repeated. What follows is what
broadcasting adds to it, and it is more than an increment.

### 5.1 What §12.4 already asks for, restated in one paragraph

Const expressions admitted where a const argument is expected; evaluation of
`+`, `-`, unary negation and division by a literal over integer const
parameters; and equality of const expressions decided on a normal form
(`Σ cᵢ·pᵢ + k`, terms sorted), so that `A + B` and `B + A` are the same type.
§12.5 notes that the shape case additionally needs division by a literal, for
`rfft`'s `N / 2 + 1`, and that units do not.

### 5.2 Decision — the first new ask: shapes are type-level lists, not fixed arity

> **Decision 5.2.** A shape is a **list of const integer expressions whose length
> is statically known but varies per instantiation**, and the type checker needs
> three total operations on such lists: **left-pad with 1 to a given length**,
> **pointwise zip**, and **delete at a const index** (which §7 needs). Plus
> element-wise equality.

This is the largest thing broadcasting asks for and it is qualitatively bigger
than `L1 + L2`.

The reason is rank-padding. `(768)` and `(n, 768)` are operands to one
application of one operator, and the result is `(n, 768)` — a shape list *of a
different length than one of its inputs*. So `Tensor of (F32, (768))` and
`Tensor of (F32, (n, 768))` must be instantiations of **one type constructor at
different arities**. If F0's generic parameter lists are fixed-arity — one
`Vec<GenericArg>` per declaration, checked against a fixed count — then F1
cannot express this without reopening the resolver, which is exactly the failure
mode §2 of the core spec invokes const generics to avoid.

Concretely, the type is

```science
type Tensor of (T, const SHAPE: Shape)
```

where `Shape` is a type-level list of const `Int` expressions, and
`Matrix of (T, const ROWS: Int, const COLS: Int)` from §5.3 of the core spec and
§6.1 of `scientific-libraries.md` is an alias for the rank-2 case:

```science
type Matrix of (T, const R: Int, const C: Int) is Tensor of (T, (R, C))
type Vector of (T, const N: Int) is Tensor of (T, (N))
```

**This is not a contradiction of `scientific-libraries.md` §6.1** — every
signature in that note's §6.7 continues to mean exactly what it means — but it
does say that `Matrix` is a view onto a more general constructor rather than a
primitive, and that note should carry a sentence saying so.

### 5.3 Decision — the compatibility predicate consults literals only, and that is why `max` is not needed

The naive formulation of the output dimension is `Rᵢ = max(A'ᵢ, B'ᵢ)` guarded by
a predicate `A'ᵢ = 1 ∨ B'ᵢ = 1 ∨ A'ᵢ = B'ᵢ`. Read that way, broadcasting adds
`max` to the const-expression language, and `max` is the thing that breaks it:
`max` over linear forms is not a linear form, so `Σ cᵢ·pᵢ + k` stops being a
normal form, and equality of shape expressions — §12.4's point 3, the one that
stalled Rust's const generics for years — stops being decidable by normalisation.

> **Decision 5.3.** The stretch test is **syntactic on literals**. A dimension
> expression stretches only if it is the literal `1`. Consequently every case of
> the algorithm in §4.2 returns one of its *inputs* unchanged, never a new
> expression, and **`max` never enters the const-expression language.**

This is the main engineering payoff of the whole design and it should be stated
where the implementer will see it: **broadcasting adds a predicate and a list
type; it does not add an operation to the arithmetic.** The expression language
stays `Σ cᵢ·pᵢ + k` with division by a literal, exactly as §12.5 specified it
for `rfft`, and it stays decidable — every question the checker asks is a finite
case analysis over a normal form that is already decidable, plus a syntactic
test for the literal 1.

Summarising what broadcasting adds to §12.4's list:

| Ask | Size | Also needed by |
|---|---|---|
| Shapes as variadic-arity type-level const lists | Large. The F0 constraint in §11.1. | Reshape, transpose, concatenation, every `linalg` output shape |
| `pad_left`, `zip`, `delete_at` over those lists | Small, total, non-recursive | Reductions (§7), `insert_axis` |
| The compatibility predicate of §4.2 | Small; literal inspection plus normal-form equality | Nothing else |
| `max` over dimensions | **Not needed** — Decision 5.3 | — |
| Const-expression arithmetic and normalisation | Already asked for by §12.4 | Units, `rfft`, concatenation |

### 5.4 Decision — symbolic dimensions do not broadcast against each other

> **Decision 5.4.** When both dimensions at an axis are non-literal, they must be
> equal after normalisation. There is no way to write a function generic over
> *whether* a dimension stretches.

The consequence is that this does not compile:

```science
# SC0294 at axis 0: `n` and `m` cannot be shown equal, and neither is 1.
def combine of (const N: Int, const M: Int)(
    a: borrowed Tensor of (F32, (N, 3)),
    b: borrowed Tensor of (F32, (M, 3)),
) -> Tensor of (F32, (N, 3)):
    a .+ b
```

and this does:

```science
def centre of (const N: Int)(
    a: borrowed Tensor of (F32, (N, 3)),
    row: borrowed Tensor of (F32, (1, 3)),
) -> Tensor of (F32, (N, 3)):
    a .- row
```

**The cost, stated.** A library author who wants one function accepting both a
full matrix and a broadcast row must write two overloads, or take the dynamic
path of §6. Supporting it properly would mean a `where` clause carrying a
broadcast *constraint* — `where (N, 3) broadcasts (M, 3) giving S` — which means
constraint solving over shape lists, an inference problem rather than a checking
one, and §5.2 of the core spec deliberately rejects global inference. **F1 does
not have it and should not pretend to.** It is named as an open question in §13
because the demand for it will come from library authors within a month of the
feature shipping.

*This decision serves programmers — it keeps the checker decidable and the
errors local — and charges scientists nothing, because scientist-written code
almost always has one concrete dimension.*

---

## 6. Dynamic shapes

A CSV has as many rows as it has. This section decides whether any of the above
is usable, and it is the section most likely to be wrong.

### 6.1 Decision — one tensor type

> **Decision 6.1.** There is **one** tensor type. There is no `DynTensor`, no
> `Tensor of (T, ?)`, and no dimension that is "maybe known".

Two types would be a catastrophe of the ordinary kind: every function in
`linalg`, `signal` and `stats` written twice, every conversion explicit, and a
user population that learns to stay in the dynamic one because it always
compiles. A "maybe known" dimension is worse, because it puts the uncertainty
inside the type where it infects every operation and every error message.

The runtime shape is always present regardless: `python-interop.md` §4.1
requires a tensor's runtime header to be field-for-field a `DLTensor`, which
carries `ndim`, `shape` and `strides`. So a tensor already knows its shape at
runtime, always. The question is never *whether* the shape is available — it is
whether the **type checker** has been told about it.

### 6.2 Decision — `with … as …` promotes a runtime shape into the type

> **Decision 6.2.** A `with` block binds const dimension parameters from a
> value's runtime shape, checked once at the head of the block. Inside the block
> those parameters are rigid and everything in §2–§5 applies unchanged.

`with` is already on §13's reserved-but-unused list, so this costs no new
reserved word.

```science
def main():
    let table, err be read_csv("measurements.csv")
    if err?:
        print("could not read the file")
        return

    with table.shape as (rows, 768):
        let centred be table .- table.mean(axis: 0)
        let z be centred ./ table.deviation(axis: 0)
        print("standardised", rows, "rows")
    else mismatch:
        print("expected 768 columns, found", mismatch.found)
```

Inside the block, `rows` is a const `Int` parameter of unknown value and known
identity, and `table` has type `Tensor of (F32, (rows, 768))`. `table.mean(axis:
0)` has type `Tensor of (F32, (768))` by §7, the `.-` right-aligns and stretches
axis 0, and the whole expression is checked statically. **Nothing inside a
`with` block is dynamically typed.**

Several operands bind in one head, and a repeated binder is a runtime equality
check — which is the whole reason the construct takes a list:

```science
    with features.shape as (n, d), labels.shape as (n):
        let error be predict(weights, features) .- labels
    else mismatch:
        print("features and labels disagree on the number of rows", mismatch)
```

That is the bug of §1.1, caught at the file boundary, once, with a message,
instead of silently at every arithmetic line.

### 6.3 Decision — the `else` branch is required exactly when the pattern is refutable

> **Decision 6.3.** `else` is **required** when the pattern contains a literal,
> a repeated binder, or a rank the operand's type does not already fix. It is
> **forbidden** (`SC0296`) when the pattern is irrefutable.

This mirrors `match` exhaustiveness, which the core spec already requires the
checker to decide, and it means a `with` that cannot fail does not force the
reader past a dead branch:

```science
    # `table` is rank-2 by its type; both binders are fresh; nothing can fail.
    with table.shape as (rows, cols):
        print(rows * cols, "elements")
```

The `else` binder receives a `ShapeError` carrying the pattern, the observed
shape, and the axis that disagreed — the same payload §10 renders for the static
case, so a runtime shape failure and a compile-time one read alike.

**Alternative rejected: make `with` an expression returning `-> (T, Error?)`.**
It is more uniform with revision-2's error model, and it cannot work: the block
binds *type-level* parameters, so the values produced inside it have types
mentioning `rows`, and those types cannot escape the block. A construct whose
result cannot leave its own scope is a block, not an expression, and pretending
otherwise would produce an error model that lies.

### 6.4 Decision — there is no dynamic broadcast operator

> **Decision 6.4.** Science does not provide `a.broadcast_add(b) -> (Tensor,
> ShapeError?)` or any runtime-checked arithmetic. If two shapes are not
> statically related, they are related by a `with` block first.

The runtime check has to happen somewhere. Putting it in the operator means it
happens on every line, each with its own error path, and under revision-2's
error model each of those is three lines of handling — which nobody will write,
so they will call a panicking variant, and the language will have shipped
NumPy's failure mode with extra ceremony. Putting it in `with` means it happens
**once, at the boundary where the data entered the program**, which is both the
cheapest place and the place where a human can say something useful about it.

**The cost, stated plainly.** Code that reads two arrays from two files and adds
them must open a block. That is one line and one indent level more than NumPy.
It is the single most likely thing for a new user to complain about, and the
answer to the complaint — *the check has to go somewhere and this is the one
place it can carry a good message* — is true but is not going to satisfy
everybody.

### 6.5 The seam with Python

A tensor arriving from `np.from_dlpack` has a runtime shape and no static one.
`python-interop.md` §3.4 already refuses rank and extent mismatches at the shim
boundary for *declared* parameter shapes. `with` is the construct for the
undeclared case, and the two compose: an exported function declares
`Tensor of (F32, (N, 768))`, the shim checks 768 and binds N, and the body is
statically checked. **That is the same mechanism as `with`, applied at the FFI
boundary instead of in the source**, and it should be implemented as the same
mechanism rather than a parallel one.

---

## 7. Broadcast function application, reductions, and axes

### 7.1 Decision — no `f.(xs)`

> **Decision 7.1.** Science does not get Julia's `f.(xs)`. Element-wise
> application of a function to a tensor is `a.map(f)`, shape-preserving.

```science
let activated be logits.map(each_value giving max(each_value, 0.0))
let probabilities be logits.map(sigmoid)
```

`collections-and-chains.md` §1.4 already spends `map` on "apply to each
element", and adding a second syntax for the same idea is precisely the debt
`syntax-revision-2.md` §1 removed from comparisons — with the same
un-canonicalisable property, since a formatter could not choose between
`f.(xs)` and `xs.map(f)` without knowing types.

`Tensor.map` is **not** the chain's `map` and does not go through `Iterate`. Its
signature is shape-preserving:

```science
def map of (U)(self, f: (T) -> U) -> Tensor of (U, SHAPE)
```

A chain over a tensor's elements would erase the shape, which is the one thing a
tensor is for. Two methods with the same name on different types, meaning the
same thing and returning the containers they were called on, is ordinary
overloading and not a duplicate spelling.

The binary form is `combine`, and it broadcasts:

```science
let scaled be a.combine(b, (x, y) giving x * exp(y))
```

> **Decision 7.1b.** Every dotted operator is **defined** as `combine` with the
> corresponding operator-interface method. `a .+ b` is `a.combine(b, Add.add)`.

This is the decision that makes fusion one mechanism instead of two: the
optimiser in §8 sees one node kind, whether the user wrote a dot or a closure.

### 7.2 Decision — reductions take `axis:`

> **Decision 7.2.** Reductions are methods with an `axis:` label taking a **const**
> argument. §13 of the core spec deliberately does not reserve `axis`, `dim` or
> `dims`, which is what makes this spelling available; this note takes `axis`.

```science
let column_means be table.mean(axis: 0)      # (n, 768) -> (768)
let row_totals   be table.sum(axis: 1)       # (n, 768) -> (n)
let grand_total  be table.sum()              # (n, 768) -> F32
let both         be cube.sum(axes: (0, 2))   # (a, b, c) -> (b)
```

The result shape is `delete_at(SHAPE, axis)` — the third list operation of
Decision 5.2 — and the axis argument must be a const, so `SC0297` when it is
not and `SC0298` when it is out of range for the rank. Both are compile errors
with the rank in the message, which is already better than NumPy's runtime
`AxisError` in the only way that matters: it fires before the eight-hour job
starts.

Reduction *order* is `collections-and-chains.md` §7.1's rule and not renegotiated
here: a tree reduction whose shape is a function of the extent and a fixed grain
size only, never of the thread count or the machine, so two runs of the same
text on the same data agree bitwise. §5.1 of the core spec requires this of every
reduction and a tensor reduction is not an exception.

### 7.3 Decision — no `keepdims`; `insert_axis` instead

> **Decision 7.3.** There is no `keepdims` parameter. The axis is always dropped.
> `insert_axis(const K: Int)` puts one back.

```science
let row_means be table.mean(axis: 1)               # (n, 768) -> (n)
let centred   be table .- row_means.insert_axis(1) # (n, 1), then stretches
```

`keepdims=True` is a boolean parameter whose value changes the *rank of the
return type*. In a language with shapes in the types that is not a stylistic
objection, it is a type error waiting to be specified: either the parameter must
be a const (so `keepdims` is a const generic argument, and the signature becomes
unreadable) or the return type depends on a runtime value, which is dependent
typing and is not on any phase list. `insert_axis` costs one call and is always
well-typed. *Serves programmers; charges scientists a method call on the minority
of reductions that need the rank kept.*

Note that `keepdims` is only ever needed for the axes the alignment rule does not
already handle. `table.mean(axis: 0)` is `(768)`, which right-aligns against
`(n, 768)` with no help. It is `axis: 1` that needs the insert — which is exactly
the asymmetry NumPy hides and which this spelling makes visible.

---

## 8. Fusion

```science
let z be (a .- mean) ./ deviation .* scale .+ shift
```

Written naively, that is four passes over the data and three full-size
temporaries. For a matrix that does not fit in cache it is four times the memory
traffic, which is four times the wall clock, and a large part of why people
write fused kernels by hand.

### 8.1 Decision — the compiler's job, and it ships with F1

> **Decision 8.1.** Fusion of element-wise chains into a single loop nest is a
> **MIR pass in the compiler**, it is not the library's, and it is **not
> deferred**.

**Alternative rejected: a lazy expression type in the library**, the
`Expression`/`LazyTensor` design that Eigen and xtensor use, where `a .- b`
returns a node and materialisation happens on assignment. It is the obvious
design and it is wrong here for a concrete reason: `python-interop.md` §4.1
requires a tensor's runtime header to be field-for-field a `DLTensor`. An
unevaluated expression node is not a `DLTensor` and cannot be handed to
`np.from_dlpack`. So every boundary crossing would need a forcing step, the
forcing step would have to be either implicit (surprising) or explicit
(everywhere), and the types in every signature in `scientific-libraries.md` §6.7
would have to say which of the two things they return. **Expression templates
put the optimiser in the type system, and this language has other plans for its
type system.**

**Alternative rejected: defer fusion to F2, with the GPU work.** Retrofitting
fusion is expensive in a specific way worth naming: if the first version
materialises every temporary, then every library signature, every benchmark
every user publishes, and every piece of user code that takes a borrow of an
intermediate is written against a world where intermediates exist. §12 of this
note treats that as the largest risk.

### 8.2 What fusion operates on

The pass runs on MIR, over the dataflow graph, so it crosses `let` bindings —
`let d be a .- b` followed by `d .* d` fuses — subject to three conditions the
region engine can already answer: the intermediate is not borrowed by anything
live, not mutated, and not returned. That is the same class of query as
`borrows_live_at(point)`, which §6.4 of the core spec already requires as a
public interface for F5.

Shape agreement after broadcasting is what makes the loop nest writable at all,
and it is known statically by §5 — which is the second payoff of putting shapes
in types, after the safety one: **a fusion pass over statically known shapes
does not need alias analysis to prove the extents agree, because the type
checker already did.**

### 8.3 What F0 must preserve for this to be possible

Stated here and repeated in §11: **operator-interface calls must remain
identifiable in MIR after monomorphization.** If `Add.add` on `F32` is lowered to
an opaque function call before MIR, the fusion pass has nothing to recognise and
must reconstruct it by inlining and pattern-matching LLVM IR, which is the
expensive retrofit this decision exists to avoid.

---

## 9. Diagnostics

Codes are proposals in the ranges §9 of the core spec defines; the registry is
the spec's to assign. This note claims `SC0290`–`SC0298` in the types range,
checked free against `SC0250`–`SC0251`, `SC0262`–`SC0264`, `SC0271`–`SC0273` and
the `SC0280`–`SC0289` block `indexing-and-array-literals.md` §7.2 claims. It
claims one syntax code, `SC0160`, checked free against that note's
`SC0150`–`SC0159`.

| Code | Severity | Condition |
|---|---|---|
| `SC0290` | error | Undotted binary operator on two tensors of unequal shape. Shows both shapes, the shape the dotted form would produce, and offers `.op` as an applicable fix. |
| `SC0291` | error | Dotted operator whose shapes do not broadcast. Shows the alignment table of §9.1. |
| `SC0292` | error | An operator whose shape rule is meaningless for these operands: `*`, `/` or `**` between two tensors (names `.*` and `@`, and says which is which), or a dotted operator between two scalars (names the undotted form). |
| `SC0293` | **warning** | Accidental outer product (§3.4). Names the result shape and `outer`. |
| `SC0294` | error | A symbolic dimension pair that cannot be shown equal (§5.4). Says *cannot prove*, never *incompatible*, and names the `with` block as the escape. |
| `SC0295` | error | A refutable `with` pattern with no `else`. Names the axis that can fail. |
| `SC0296` | error | An `else` on an irrefutable `with`. |
| `SC0297` | error | A non-const `axis:` argument. |
| `SC0298` | error | An axis out of range for the rank; the message carries the rank. |
| `SC0160` | error | A dotted operator in an F0 build. "Dotted operators arrive with tensors in F1" (§11.2). Removed when F1 lands. |

### 9.1 The one that matters

A shape error that says "operands could not be broadcast together" has thrown
away the entire advantage over NumPy, which says exactly that. The requirement:
**both shapes, right-aligned as the algorithm aligns them, with the failing axis
marked, and the padding shown as padding.**

```
error[SC0291]: these shapes do not broadcast
  --> model/loss.science:41:18
   |
41 |     let residual be predictions .- targets
   |                     ----------- ^^ -------
   |                     |              |
   |                     (64, 10)       (32,)
   |
   = the operands are aligned from the right:
   
         axis      0     1
         left     64    10
         right     1    32     <- padded with 1
                        ^^
   = axis 1: 10 and 32 are both greater than 1, so neither stretches
   = `targets` has rank 1; if it is meant to name one value per row,
     it must be shaped (64, 1) — try `targets.insert_axis(1)`
```

Three things that table does which prose cannot. It shows **which axis**, so the
user does not count on their fingers. It shows **the padding**, which is the
half of the rule people forget and the half that produced §1.1's bug. And it
shows **the alignment direction**, which is the other half.

`SC0290`, the undotted-with-broadcastable-shapes error, carries the same table
plus the sentence that does the work:

```
   = these shapes broadcast to (100, 100)
   = if you meant that, write `.-`
   = if you meant one residual per row, `predictions` has a trailing
     axis of 1 that `targets` does not — `predictions.remove_axis(1)`
```

The second suggestion is not decoration. It is the difference between a compiler
that enforces a rule and a compiler that finds a bug, and for the population
arriving from NumPy it is the entire first impression.

---

## 10. Worked example

A standardisation and scoring pipeline, with the file boundary, the dynamic
shape, the reductions, the broadcasts and the error handling all visible.

```science
def standardise of (const N: Int, const D: Int)(
    table: borrowed Tensor of (F32, (N, D)),
) -> Tensor of (F32, (N, D)):
    let centre be table.mean(axis: 0)               # (D)
    let spread be table.deviation(axis: 0)          # (D)
    (table .- centre) ./ (spread .+ 1e-8)           # both stretch axis 0

def score of (const N: Int, const D: Int)(
    table: borrowed Tensor of (F32, (N, D)),
    weights: borrowed Tensor of (F32, (D)),
    bias: F32,
) -> Tensor of (F32, (N)):
    (table .* weights).sum(axis: 1) + bias          # scalar add: no dot

def main():
    let table, read_err be read_tensor("measurements.npy")
    if read_err?:
        print("could not read the measurements")
        return

    let weights, weights_err be read_tensor("weights.npy")
    if weights_err?:
        print("could not read the weights")
        return

    with table.shape as (rows, features), weights.shape as (features):
        let z be standardise(table)
        let s be score(z, weights, 0.5)

        let flagged be s .> 3.0                     # (rows) of Bool
        print(flagged.count_true(), "of", rows, "rows are outliers")
    else mismatch:
        print("the weights do not match the measurements:", mismatch)
```

Four things to notice, because they are the note in miniature.

- `table .- centre` broadcasts and says so. `+ bias` does not, because `bias` is
  a scalar and Decision 2.1 exempts it.
- `(table .* weights)` is element-wise and `@` is not written, so no reader has
  to wonder which product was meant.
- The repeated binder `features` in the `with` head is the check that the
  weights match the data. It is written once and costs one runtime comparison.
- Inside the block, `standardise` and `score` are ordinary statically shaped
  generic functions. They do not know they were called from a dynamic context
  and they do not need to be written twice.

---

## 11. What F0 must not preclude

In the style of §2 of the core spec. Each item is cheap now and expensive later,
and the last two are free.

**11.1 Const generic argument lists must admit variadic arity.** `Tensor of (F32,
(n, 768))` and `Tensor of (F32, (768))` are one type constructor at two arities
(§5.2). If F0's resolver checks a declaration's generic arguments against a fixed
count, F1 cannot express rank-polymorphic broadcasting without reopening the
resolver and every snapshot — the exact failure §2 invokes const generics to
avoid. **This is the largest ask and the only one that is structural.**

**11.2 The lexer must tokenise dotted operators from F0.** `.+ .- .* ./ .% .**
.> .< .>= .<= .& .|` as single tokens by longest match, and `.is` as the existing
sequence with a dot prefix. The parser rejects them in F0 with `SC0160`, "dotted
operators arrive with tensors in F1". This is the `->` lesson of
`syntax-revision-2.md` §4
applied before the fact: changing how `.` is tokenised after the snapshots exist
moves every lexer snapshot in the repository, and doing it now moves none of
them.

**11.3 Record that there is no leading-dot float literal.** §4.2 lists `3.14`
and does not list `.5`. That absence is what makes `a .* .5` unambiguous and it
should be deliberate rather than accidental (§3.6).

**11.4 Operator-interface calls must survive monomorphization identifiably.**
§8.3. Without it the fusion pass has nothing to match on.

**11.5 Argument labels must be part of a method's name**, resolved before type
inference. `collections-and-chains.md` §10.2 already asks for this for `sort(by:)`;
`axis:` is the second consumer and `axes:` the third.

**11.6 Closure types must be spelled — the spelling is now decided.** `(T) -> U`
appears in `map` and `combine` (§7.1). `scientific-libraries.md` §14.2 and
`ffi-c-boundary.md` §10.1 had both already asked and this was the third note to
depend on it; `collections-and-chains.md` §1.2 has since taken the ask and
settled the syntax. The ask that remains is the implementation.

**11.7 `shape` should stop being a reserved word, and F1 does not want a `shape`
declaration.** `reserved-words.md` §6 leaves that as an open question for
whoever writes the F1 note:

> **Does F1 actually want a `shape` declaration?** [...] If the F1 note, when it
> is written, finds that shapes are expressed entirely in type-argument position
> [...] then there is no declaration form, `shape` is never needed as a keyword.

**This note answers it: no.** A shape in this design is a type-level list of
const expressions appearing in type-argument position and in a `with` pattern.
Neither is a declaration. There is no `shape Batch:` item form in F1 and none is
wanted. So `reserved-words.md` §3.3's recommendation should be taken in full,
and `x.shape` — which §10 of this note writes, and which every user will write
in their first ten minutes — works.

---

## 12. What this note asks of other notes

1. **`scientific-libraries.md` §12.5** should carry a sentence recording that
   broadcasting is the second consumer of const-expression arithmetic, and that
   the shape case additionally needs type-level *lists* (§5.2), which is a larger
   ask than the `Σ cᵢ·pᵢ + k` normaliser that note prices. Its conclusion — build
   it once — is strengthened, not weakened, but the price is higher than stated.
2. **`scientific-libraries.md` §6.1** should say that `Matrix` and `Vector` are
   aliases for rank-2 and rank-1 `Tensor` rather than primitive type
   constructors (§5.2). No signature in its §6.7 changes.
3. **`collections-and-chains.md` §1.4** should note that `Tensor.map` exists,
   is shape-preserving, and does not go through `Iterate` (§7.1) — so that the
   closed set is not read as forbidding it.
4. **`python-interop.md` §3.4** should be cross-referenced to §6.5: the shim's
   rank-and-extent check and the `with` block are the same mechanism and should
   share an implementation and a message.
5. **`reserved-words.md` §6**'s open question is answered by §11.7 and can be
   closed.
6. **The core spec §4.6** gains the dotted operators in the precedence table, at
   the same precedence as their undotted forms. §13's reserved list is unchanged
   by this note except for the `shape` removal already recommended elsewhere.
7. **`indexing-and-array-literals.md` item 10 is accepted.** That note asks that
   "F1's shape note, when it is written, should build the obligation discharger
   once", because its **Constant** and **Shape equality** index-obligation rules
   are the shape checker doing its ordinary job. This is that note, and the
   answer is yes: the normal-form equality of Decision 5.3 is the same predicate
   its **Shape equality** rule needs, and there must be one implementation.
   Its `Tensor of (F64, (2, 2))` spelling in §3.5 is §5.2's type-level shape list
   and the two notes agree. Diagnostic blocks are disjoint by construction: that
   note holds `SC0150`–`SC0159` and `SC0280`–`SC0289`, this one holds `SC0160`
   and `SC0290`–`SC0298`.

---

## 13. Open questions

**Does `@` broadcast its batch dimensions?** §4.4 defers it to
`linalg.batch_matmul`. Every framework in the world broadcasts batched matmul
and users will expect the operator to. The rule is coherent but it is a second
rule, and this note does not decide whether the operator gets it in F1.

**Can a function be generic over whether a dimension stretches?** §5.4 says no.
The demand will come from library authors, and the answer is a broadcast
constraint in a `where` clause, which is constraint solving over shape lists.
Cheaper to design now than to bolt on.

**Is `.is` tolerable?** §3.3 accepts a spelling it does not like, on consistency
grounds, and names the fallback. This should be revisited after the first real
program that builds a lot of masks.

**Does `with` interact with regions correctly?** A `with` block introduces
type-level bindings that constrain the types of values that outlive the block —
and §6.3 says those values cannot escape. Whether that is the right rule for a
tensor that is *materialised* inside the block and wanted outside it is not
settled; the likely answer is that it escapes with its dimensions replaced by a
fresh existential, which is a second mechanism this note has not designed.

**Where does `where` go?** Core spec §13 notes that `where` is reserved, so a
user cannot define the function NumPy users reach for first. Element-wise select
needs a name. `select(condition, a, b)` is the obvious one and it is not decided
here.

---

## 14. Decisions, and who each one serves

| § | Decision | Serves |
|---|---|---|
| 2.1 | Undotted when one operand fixes the result shape; scalars exempt | Scientists (removes most of the dot tax) |
| 2.2 | `a + b` on unequal-but-compatible shapes is an error | Both; the diagnostic is what makes it serve scientists |
| 3.1 | `.` composes mechanically with every operator interface | Programmers (user types are not second class) |
| 3.2 | `*` on two tensors is undefined; `.*` and `@` | Both; strictly better than NumPy for both populations |
| 3.3 | `.is` for element-wise equality | Programmers (consistency); charged to readability |
| 3.4 | Warning on the accidental outer product | Scientists |
| 4.1 | NumPy's alignment rule, unchanged | Scientists (transferred knowledge stays true) |
| 5.2 | Shapes are variadic type-level lists | Programmers (it is the implementation ask) |
| 5.3 | Literal-only stretch test, so no `max` in the type language | Programmers (decidability) |
| 5.4 | No symbolic-against-symbolic broadcasting | Programmers; charged to library authors |
| 6.1 | One tensor type | Both |
| 6.2 | `with … as …` promotes runtime shapes | Scientists (this is what makes real data usable) |
| 6.4 | No dynamic broadcast operator | Both; charged to convenience |
| 7.1 | No `f.(xs)`; `map` and `combine` | Programmers (one spelling per idea) |
| 7.2 | `axis:` labels, const arguments | Scientists (`axis` is their word) |
| 7.3 | No `keepdims`; `insert_axis` | Programmers; charged one call to scientists |
| 8.1 | Fusion is a compiler pass and ships with F1 | Scientists (it is the performance story) |
| 9.1 | The alignment table in the diagnostic | Scientists |

---

## 15. Risks

**The dot tax is the reason this design gets rejected, if it does.** Every
argument in this note is correct and none of them survives a population that
finds the language annoying to type. The scalar exemption of Decision 2.1 is the
mitigation and it is a real one — it removes the dot from the majority of mixed
operations — but the residual cost is on exactly the lines scientists write most.
**The retreat, and its direction.** If the tax proves intolerable, `+` can be
loosened to broadcast later and existing programs keep compiling, because every
program that compiles today has equal shapes or a dot. It cannot be tightened
later. So the strict version is the one to ship, and the loosening is a decision
that stays available for as long as it is wanted. That asymmetry is the strongest
practical argument for Decision 2.2 and it should be stated to anyone who
challenges it.

**Fusion may not pay.** §8 commits the compiler to a pass whose value is entirely
empirical. LLVM may not vectorise the fused nest; the fused loop may lose to four
calls into a hand-written BLAS-like kernel for large sizes; the pass may fire on
20% of the chains users write. **The honest measure is a benchmark against NumPy
on a five-operation chain at three sizes — in cache, out of cache, and out of
memory — and it should exist before the pass is called done**, because a fusion
pass that is believed rather than measured will be defended long after it should
be replaced.

**§5.2 is a larger ask than `scientific-libraries.md` §12.4 priced.** That note
costs const-expression arithmetic at "a few hundred lines" for the normaliser,
which is right for units. Variadic-arity type-level lists with three total
operations, plus the effect on the resolver, monomorphization keys and the
diagnostic renderer, is a different order of work. If §11.1 is not honoured in
F0, this note's entire §4 becomes an F2 feature, and `scientific-libraries.md`
§13's wave 3 — all of `linalg` — moves with it.

**`with` is a new binding construct and it is the least-tested idea here.** It
introduces type-level bindings from a runtime value, which no other construct in
the language does, and §13 records that its interaction with regions and with
values escaping the block is unsettled. It is also load-bearing: without it, §6
has no answer and the feature does not work on data read from files, which is
all data.

**Two tensor libraries will exist anyway.** The prediction is that within a year
of F1 someone will write a dynamic-shape tensor library in Science because they
find `with` too strict, and that library will be popular, and it will reintroduce
every failure mode §1 describes. The defence is not prohibition — it is that
`with` must be good enough, and fast enough, that the dynamic library has nothing
to offer. That is a usability bar, not a technical one, and this note cannot
prove it is met.
