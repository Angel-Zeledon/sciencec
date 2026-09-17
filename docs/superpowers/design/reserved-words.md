# Science — Design: reserved words against the scientific vocabulary

Date: 2026-09-16
Status: draft for review
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §13, whose
three-part list of reserved words this note audits and proposes to change.
Related: `ffi-c-boundary.md` §1.1, which already commits to contextual keywords
inside `extern` blocks and therefore settles, by precedent, the question §13
says was considered and rejected.

---

## 0. The finding this note is built on

§13 justifies reserving `shape`, `model` and `tensor` globally like this:

> `shape`, `model` and `tensor` are reserved globally, which means `x.shape` and
> `let model be ...` do not compile. This is a known cost, accepted deliberately
> so that F1 and F4 syntax is guaranteed; the alternative considered was making
> them contextual keywords recognized only in declaration position.

The sentence is accurate about today's implementation and wrong about the choice
it presents. There are not two options. There are three, and the middle one was
never on the table:

1. Reserve the word everywhere.
2. **Reserve the word everywhere it could be ambiguous, which is nowhere after a
   `.`.**
3. Make the word contextual, recognised only in declaration position.

Option 2 is not a weaker form of option 3. It is a different axis, it is five
lines of parser, it costs nothing anywhere, and it independently removes the
single most-cited item on the whole collision list — `x.shape` — along with
`poly.at(x)`, `a.union(b)`, `values.any()` and `re.match(s)`.

Everything below follows from separating those two axes, because most of the ten
collisions the audit was asked about are member-position collisions that option 2
closes outright, and only three are genuine.

### 0.1 Why option 2 is free

Reservation today is decided in the lexer, with no notion of position:

```rust
// crates/science-lexer/src/lexer.rs — fn word()
TokenKind::from_word(text).unwrap_or_else(|| TokenKind::Ident(text.to_string()))
```

`from_word` is a pure string lookup. `shape` becomes `Reserved(Shape)` wherever
it appears, including in `x.shape`. The parser then reaches member access:

```rust
// crates/science-parser/src/parser.rs — fn parse_postfix()
TokenKind::Dot => {
    let Some(name) = self.expect_ident() else { ... };
```

and `expect_ident` accepts `TokenKind::Ident` and nothing else. So `x.shape`
fails, and it fails with `expected an identifier, found a word reserved for a
later phase` — which is true and useless.

But **after a `.` there is nothing a name could be confused with.** A member name
is the only construct the grammar admits in that position. No keyword can begin
anything there, because no statement, expression or type may start immediately
after a field-access dot. The reservation buys nothing at all in that position
and costs the language its most common attribute access.

The fix is one function:

```science
# What the parser should accept after `.`: any word at all, taken as a name.
x.shape          # a field named `shape`
poly.at(x)       # a method named `at`
a.union(b)       # a method named `union`
values.any()     # a method named `any`
re.match(text)   # a method named `match`
n.mod(m)         # a method named `mod`
frame.import()   # a method named `import`
```

None of these is ambiguous. None of them becomes ambiguous later: `.` is a
postfix operator whose right operand is grammatically a single name, forever.

This is not novel. It is what C# does for every contextual keyword, and what Rust
does for `r#`-free member names — `x.union(y)` on a `HashSet` works although
`union` is a Rust keyword in `union` declarations, precisely because the position
disambiguates.

### 0.2 What option 2 does not fix

Binding position. These are still broken after the dot rule and need their own
answer:

```science
let model be load("weights.safetensors")    # `model` in binding position
let shape be tensor.shape                   # `shape` in binding position
def forward(tensor: Tensor) -> Tensor:   # parameter named `tensor`
let yield be produced / theoretical         # `yield` in binding position
def kde(kernel: Kernel) -> Density:      # parameter named `kernel`
```

§3 handles these.

---

## 1. How to read the audit

A word can be wanted in four positions, and a reservation hurts differently in
each. The audit classifies every collision by the *tightest* position the
scientific vocabulary needs.

| Position | Example | Fixed by the dot rule? |
|---|---|---|
| **Member** — after a `.` | `x.shape`, `poly.at(x)` | **Yes**, outright |
| **Binding** — a `let`, a parameter, a field name | `let model be …` | No |
| **Free function** — called bare | `assert(cond)`, `any(mask)` | No |
| **Module** — a name in `use` | `use const (PI)` | No |

Two further facts narrow the problem before the table starts.

**Types are capitalised.** Science writes `String`, `Array`, `Map`, `Doc`,
`Tensor`. The F0 spec's own tensor example is `Tensor of (F32, (batch, 768))` —
capital `T`. So the *types* `Tensor`, `Shape`, `Model` and `Union` never collided
with anything; only the lowercase words did, and the lowercase words are wanted
by users, not by the language. Reserving `tensor` to protect `Tensor` is
protecting a word that was never at risk.

**The FFI note already adopted contextual keywords.** Its summary table reads:

> | Extern block is a small separate grammar with contextual keywords | Reusing Science's declaration grammar unchanged | C's type system is not Science's |

So `extern`, `unsafe`, `union` and `static` are already specified to be
recognised positionally inside a sublanguage. §13's claim that contextual
keywords were considered and rejected is not the project's actual position — a
sibling design note depends on them. This is a contradiction between two approved
documents and it should be resolved in favour of the note that has working
examples.

---

## 2. The audit

`K` = in use as a keyword today. `R` = reserved, not yet used. Status from
`crates/science-lexer/src/token.rs`, verified against §13.

| Word | Now | Collides with | Position | Decision | If kept: use instead |
|---|---|---|---|---|---|
| `shape` | R | `x.shape` — the most common attribute access in numerical Python | Member, binding | **Free** (§3) | — |
| `model` | R | `let model be …` — the most common variable name in ML code | Binding | **Free** (§3) | — |
| `tensor` | R | parameter and local names throughout array code | Binding | **Free** (§3) | — |
| `at` | K | `poly.at(x)`, `series.at(i)` | Member | **Keep**, dot rule suffices | `at` works after `.` |
| `any` | K | `values.any()`, `any(mask)` reduction | Member, free fn | **Make contextual in type position** (§4.1) | `any_of` / `all_of` if not |
| `match` | K | `re.match(s)` | Member | **Keep**, dot rule suffices | `find`, `matches` read better anyway |
| `union` | R | `a.union(b)`, set algebra | Member | **Free** — the FFI sublanguage recognises it contextually | — |
| `mod` | R | `mod(a, b)`, `a.mod(b)` — arithmetic modulo | Member, free fn | **Free** (§2.1) | — |
| `yield` | R | reaction yield, crop yield, bond yield | Binding, field | **Free** (§2.2) | — |
| `kernel` | R | KDE, convolution, GPU kernel | Binding, parameter | **Free**, contextual in F2 | — |
| `import` | R | `data.import(…)` | Member | **Free** — Science spells it `use`; nothing needs the word | — |
| `assert` | R | `assert(cond)` in tests | Free fn | **Keep, and implement it** (§4.2) | — |
| `with` | R | `with_capacity`, context managers | Binding | Keep — pressure is low, `with_*` is one identifier | — |
| `move` | R | `moving_average` is one identifier | — | Keep — no real collision | — |
| `on`, `static`, `pure`, `parallel`, `macro`, `extern`, `unsafe`, `kernel` | R | low | — | Keep; all are declaration-position words the FFI note already treats contextually | — |
| `const` | K | a `const` module holding `PI`, `C`, `H` | Module | **Keep**; rename the module | `constants`, or `physics.constants` |
| `def` | K | `def: DefId`, `binding.def` — the field name for a definition in compiler and symbolic-algebra code | **Binding and field** | **Keep** — it is the declaration keyword since revision 3 (§2.3) | `definition`; `d` after the dot rule lands |
| `function` | free | nothing: it was the declaration keyword until revision 3 and is now an ordinary word | — | **Leave free**, and keep `SC0156` contextual (§2.3) | — |
| `grad`, `dim`, `dims`, `axis`, `device`, `dtype`, `unit`, `alias` | free | — | — | Already correct — §13's best decision | — |

### 2.1 `mod`

Full disclosure: `mod` was added to §13's reserved list earlier today, to make
the spec agree with the lexer, which already reserved it. That was the right call
for consistency and the wrong call on the merits, and this note reverses it.

The case for reserving it was that `mod.science` names a directory module, so the
word is load-bearing. It is not: `mod.science` is a *filename*. Filenames are not
identifiers, and no reservation is needed to give one a special meaning — the
same way `Cargo.toml` needs no keyword.

The case against is that `mod` is arithmetic modulo, and a language for
scientific computing that cannot write `mod(a, b)` or `a.mod(b)` has given up a
name it will want in `math` on day one. §4.4 gives modules no declaration form
and §12 excludes one from F0, so nothing is being protected.

**Free `mod`.** The one consequence is that a module may not be named `mod`,
because `mod.science` already means something in that directory. That is a
filename rule, stated in §4.4, not a reservation.

### 2.2 `yield`

`yield` is reserved for a generator form that no phase in §2 asks for. F3 is
actors — `spawn`, `send`, `receive`, supervision. F5 is durability, lowering
agent bodies to state machines. Neither needs `yield` as a surface keyword, and
`Iterate` (§5.4) is how Science produces sequences.

Against that speculative need stands a word that is the *result* in three
separate fields: reaction yield in chemistry, crop yield in agronomy, bond yield
in finance. `let yield be produced / theoretical` is a line a chemist writes in
their first hour.

**Free `yield`.** If a generator form ever lands, it is a declaration-position
word and can be contextual then, at the cost §3.2 measures.

### 2.3 `def`, and the collision this note did not get to price first

`def-and-lambda.md` §5 audited `def` before it was a keyword and recorded the
row honestly: *the reserved-word audit does not reject `def`*. It priced the
collision as low, on the ground that `def` is not a word a physicist or a chemist
binds. That was right about the audience it checked and wrong about the audience
it forgot.

**`def` is ordinary compiler vocabulary, and this language intends to compile
itself.** `self-hosting.md` is a whole note about writing `sciencec` in Science.
A compiler's central table maps a definition id to a definition, and the field is
called `def` in LLVM, in rustc, in every teaching compiler, and it was called
`def` here: `examples/21_compiler_shapes.science` had `def: DefId` and
`binding.def`, and the rename to `def` broke it on the first run. The field is
`definition` now.

So the row above says **Keep**, because the decision is made and a keyword is not
re-litigated by its own collision audit, but the price is a real one and it is
paid by exactly the users this project has promised to become:

- **Binding and field position**, which §1's table marks as the two the dot rule
  does not rescue. `binding.def` is a member access, so the dot rule *would*
  free it — but `def: DefId` in the type declaration is a field name in
  declaration position and no rule reaches it.
- It is the first reservation in the language whose collision is with the
  language's own implementation rather than with its subject matter. Every other
  row in this table is a scientist's word. This one is a compiler writer's, and
  §13 of the core spec has never audited against that vocabulary at all.

**`definition` is the replacement**, and it should be the recommendation
wherever this comes up: it is the whole word, it is what `def` abbreviates, and
it reads better in a field list than `d` or `def_id` do.

**`function` goes the other way.** It left the keyword list and was *not* moved
to the reserved list, which is the choice this note would have argued for anyway:
the compiler recognises the stale declaration as the ordinary word `function`
followed by a name, one token of lookahead, exactly as §0.1's dot rule trades a
reservation for a position test. A reservation would have cost the identifier —
`ffi-c-boundary.md` §4.3 binds a C struct field spelled `function` — and bought
nothing `SC0156` does not already have.

---

## 3. `shape`, `model`, `tensor` — reopening the decision

This is the one the spec asked to have reopened, and it is the highest-impact
ergonomic decision in the language.

### 3.1 What the reservation is protecting

§13 says the reservation exists "so that F1 and F4 syntax is guaranteed". The
honest position is that **neither F1 nor F4 has specified any syntax that uses
these words in lowercase.** What the spec actually writes is:

- §2: `Tensor of (F32, (batch, 768))` — the type, capitalised.
- §2 table, F1: "Tensors with typed shapes" — no surface syntax given.
- §2 table, F4: "`agent`, `tool`, `prompt` as language constructs" — `model` is
  not among them.

So the reservation protects a hypothesis. A reasonable hypothesis — F1 might want
a shape declaration, F4 might want `model Foo:` beside `agent Foo:` — but a
hypothesis, and one whose cost is being paid in full today by every user.

Note also what the hypothesis would look like if it came true:

```science
shape Batch:              # if F1 wants a shape declaration
    batch: Int
    features: Int

model Classifier:         # if F4 wants a model declaration
    ...
```

Both are **declaration position**: the word is the first token of an item.

### 3.2 What making them contextual costs, exactly

The question deserves a precise answer rather than a shrug, so here is the whole
bill.

**Lexer.** Three entries leave `from_word`. The words lex as `Ident`. Cost: three
deleted lines, and `ReservedWord` loses three variants. This is negative work.

**Parser, today.** Nothing. There is no F0 syntax using them, so there is no
branch to write. Cost: zero.

**Parser, when F1 lands.** `parse_item` must recognise `shape` in declaration
position. The interesting question is whether that is ambiguous against the
items Science already has, and it is not, because of a property the grammar
gained in the English redesign: **an item may already begin with a bare name.**
`Doc implements Summarize:` and `Doc has methods:` both start with a path. So
`parse_item` already parses a leading name and then dispatches on what follows:

| Source | Second token | Item |
|---|---|---|
| `Doc implements Summarize:` | `implements` | an implementation |
| `Doc has methods:` | `has` | an inherent block |
| `shape Batch:` | an identifier | a shape declaration |

One identifier followed by another identifier is not a form any existing item
takes. Two tokens of lookahead settle it, and `Parser::peek_ahead(offset)`
already exists. Cost: one match arm.

**The real cost, stated plainly.** It is not ambiguity, it is *diagnostics*. When
someone writes `shape Batch` and forgets the colon, a reserved `shape` lets the
compiler say "a shape declaration needs a body". A contextual `shape` sees a name
followed by a name and must guess between "you meant a declaration" and "you
wrote two expressions on one line". The error gets worse for that one mistake.

That is a genuine cost and it is the only one. It is bounded, it applies to a
single malformed input, and it can be mitigated: when the parser sees
`Ident Ident` at item level and the first identifier is one of the known
declaration words, it can say so specifically. The mitigation is a heuristic in
the error path, which is exactly where heuristics are safe.

**Against** that: every user of NumPy, PyTorch, JAX, pandas and xarray types
`.shape` within their first ten minutes, and `model` is the most common variable
name in machine learning. Science's stated audience in §1 is precisely these
people. §1 also says the bet is that Science must be "measurably better than
Julia and Mojo at catching mistakes before the run starts, [or] it loses to both
on ecosystem". Losing on ergonomics before the type system ever gets a chance to
prove itself is the worst possible way to lose that bet.

### 3.3 Recommendation

**Free all three now. Make them contextual if and when a phase actually needs
them.**

Concretely:

1. Remove `shape`, `model` and `tensor` from `ReservedWord` and from §13's
   second list. They become ordinary identifiers.
2. Move them into §13's third list — "deliberately not reserved" — beside `grad`,
   `dim`, `axis`, `device` and `dtype`, with the same justification, which is
   already written there and already applies: *each is a common variable name in
   the target audience's code.* `shape` is more common than every word currently
   in that list.
3. Record in §13 that F1 and F4 may recognise them contextually in declaration
   position, and that §3.2 above priced that and found it to be one match arm
   plus one error-path heuristic.

The types `Tensor`, `Shape` and `Model` remain available and were never in
conflict.

### 3.4 The alternative, if the recommendation is refused

If the core team keeps them reserved, the dot rule of §0.1 must still be adopted,
because it is independently correct and it recovers `x.shape`, which is most of
the loss. The residue is binding position, where users would write `weights`
instead of `model` and `dims` instead of `shape`. That is survivable and it is
not nothing: it means the language's own tutorial cannot use the words its
audience uses.

---

## 4. Two words that earn a second look

### 4.1 `any`

`any` is in use, for dynamic dispatch: `any Summarize`, `borrowed any Summarize`,
`Box of any Summarize`. It collides with the boolean reduction that every array
library has, `any(mask)` and `values.any()`, whose partner `all` is *not*
reserved — an asymmetry that will read as an accident.

The dot rule recovers `values.any()`. The free function `any(mask)` is the
residue.

`any` is worth making contextual because its two uses are in **disjoint
positions**. `any Trait` appears only where a type is expected: after `:`, after
`->`, inside `of (…)`, after `as`. A call `any(mask)` appears only where an
expression is expected. The parser always knows which it is parsing.

**Recommendation:** treat `any` as a keyword in type position and an identifier
in expression position. If that is judged too subtle, the fallback is to name the
reductions `any_of` and `all_of`, which is symmetric and costs one word of
verbosity.

### 4.2 `assert`

`assert` is the one reserved word whose reservation should be kept *and acted
on*, rather than kept and deferred.

Science's thesis in §1 is verification: "the mistakes a compiler can catch before
a run starts". An `assert` that is an ordinary function is a runtime check. An
`assert` that is a language construct is a place where the compiler can
sometimes discharge the condition statically — and where, when it cannot, it can
record the assumption for the region and shape machinery to use. That is the
difference between a testing utility and a verification feature, and Science has
already reserved the word.

**Recommendation:** keep `assert` reserved and specify it in a later note as a
construct, not a library function. Until then, tests spell their checks
differently, which is a small cost with a clear end date.

**Status.** `assert(cond)` / `assert(cond, message)` is implemented as a
statement — `crates/science-lexer/src/token.rs`'s `TokenKind::Assert` — and
its condition is checked against `Bool` the way an `if`'s is; on failure it
calls the same runtime path `panic` does and aborts. That is only the first
half of this section's argument: the condition is not discharged statically,
and nothing here records it as an assumption for `science-regions` or a
future shape checker to read. `assert` today is exactly the "testing utility"
this section distinguishes itself from, reached through a keyword instead of
a function call because a call cannot make control flow conditional on its
own argument (`TokenKind::Assert`'s own decision, in that file). The
verification-feature half — static discharge, and an assumption recorded for
region or shape inference — is still open.

---

## 5. What this note asks for

1. **The dot rule** (§0.1): after `.`, accept any word as a member name. One
   function in `crates/science-parser/src/parser.rs`, an `expect_member_name`
   beside `expect_ident`. Required regardless of every other decision here.
2. **Free** `shape`, `model`, `tensor`, `mod`, `yield`, `kernel`, `union`,
   `import` — eight words out of `ReservedWord` and §13's second list, three of
   them into §13's third list.
3. **Make `any` contextual** in type position, or rename the reductions.
4. **Keep and specify `assert`** as a construct. Done for the runtime-check
   half (§4.2's "Status"); the static-discharge half is still open.
5. **Resolve the contradiction** between §13 and `ffi-c-boundary.md` on whether
   contextual keywords are acceptable. This note takes the FFI note's side: they
   are, the project already depends on them, and §13's sentence should be
   rewritten to say so.
6. **Rename** the constants module to `constants` (`physics.constants`), which
   needs no language change at all.
7. **Audit against the compiler's own vocabulary** (§2.3), which no list in §13
   has ever done. `def` was found by breaking a corpus file rather than by
   reading; `type`, `has`, `match`, `use` and `const` are all words a compiler
   written in Science will want as field and local names, and the dot rule
   covers only the ones that appear after a `.`.

### 5.1 Diagnostics this changes

`SC0002` — "a word reserved for a later phase used as a name" — fires eight times
less often, and for `shape`, `model` and `tensor` it stops firing entirely. Its
message should gain the fix it currently lacks: when the reserved word appears
after a `.`, the compiler should not report it at all; when it appears in binding
position, the message should name a concrete alternative rather than saying only
that the word is taken.

---

## 6. The open question, since answered

This note left one place where it assumed something outside its territory:

**Does F1 actually want a `shape` declaration?** The whole of §3 prices a
hypothesis. If shapes turn out to be expressed entirely through const generics on
`Tensor` — which §2 of the core spec suggests, since `Tensor of (F32, (batch,
768))` carries the shape as type arguments — then there is no declaration form,
`shape` is never needed as a keyword, and the reservation was pure cost from the
start.

**`broadcasting.md` §11.7 answers it: no.** A shape in that design is a
type-level list of const expressions, appearing in type-argument position and in
a `with` pattern. Neither is a declaration; there is no `shape Batch:` item form
in F1 and none is wanted.

So the hypothesis §3 was pricing does not hold, and the conclusion strengthens
rather than changes: §3.3's recommendation should be taken **in full**, and it is
no longer a bet against a possible future need. `shape` was reserved to protect a
construct that the phase it was reserved for does not want.

That leaves §3's cost accounting one-sided in a way worth restating plainly: the
reservation buys nothing at all, and it costs `x.shape` — which every user of
NumPy, PyTorch, JAX, pandas and xarray writes in their first ten minutes.
