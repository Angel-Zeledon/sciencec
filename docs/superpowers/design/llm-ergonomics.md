# Science — Design: ergonomics for language models

Date: 2026-09-16
Status: draft for review
Related: `reserved-words.md`, whose dot rule removes five of the failures
inventoried here; `collections-and-chains.md` §3 (naming) and §9 (diagnostics).
Amends: nothing yet. §9 of the core spec allocates the diagnostic ranges this
note spends.

---

## 1. The premise

Science has no training corpus and will not have one for years. Every model that
writes Science will write it by analogy from Rust and Python, because that is
what the syntax rhymes with and that is what the model has read a hundred
million lines of.

This has an unusual consequence for a compiler:

> **The diagnostics are not the fallback channel. They are the only channel.**

A human learning Science reads a tutorial. A model learning Science reads the
error it just got, in a loop, and the quality of that error is the entire
bandwidth of the teaching. A compiler that says *expected an identifier, found
`&`* has told a model nothing it can act on; a compiler that says *a shared
borrow is written `borrowed T`, not `&T`* has closed the loop in one round trip.

The corollary is that a migration diagnostic is worth more than it looks. It is
not a courtesy for former Rust programmers. It is the mechanism by which the
language is learnable at all by the tools most people will use to write it.

A second consequence, less obvious and at least as important: **an error cascade
is actively harmful.** A human skims three errors and fixes the first. A model
reads three errors as three independent problems and attempts three fixes, two
of which are wrong, and the second round trip starts from a worse position than
the first. §3 measures how often this happens today.

---

## 2. (a) The inventory

Every habit below was run through the real lexer and parser and the output
recorded verbatim. Nothing here is inferred.

Legend: **fix** = the diagnostic carries an applicable `Suggestion` a tool can
apply mechanically. **note** = it carries prose but nothing applicable.
**cascade** = how many further errors the same line produces.

### 2.1 Already good

| Habit | Diagnostic today | Fix | Cascade |
|---|---|---|---|
| `!flag` | `SC0001` — ``` `!` is not an operator in Science ```, note: *negation is spelled `not`* | **yes** | 2 |
| `x = 1` | `SC0100` — *assignment is written `be`* | **yes** | 0 |

Two. Out of fourteen.

### 2.2 Right message, no applicable fix

| Habit | Diagnostic today | Missing |
|---|---|---|
| `g()?` | — — **this row is void since revision 2**; see below | not a fix. A decision about what `g()?` now means |
| `for x in xs:` | `SC0100` — *expected `each` after `for`* | a fix inserting `each` |
| `let x = 1` | `SC0100` — *expected `be`, found `=`* | a fix replacing `=` with `be` — the assignment path already has one |

These are one line of work each: the diagnostic already knows the span and the
replacement text, and `Suggestion` already exists and is already rendered.

**Except the first, which revision 2 turned inside out.** `syntax-revision-2.md` §3
makes `?` a postfix presence test, so `g()?` is no longer a syntax error — it is
legal, it is a `Bool`, and it means *"did `g()` return something non-null"*. The
habit this row was written about is a model reaching for Rust's propagation
operator, and that habit now **parses and compiles into something else**. A
diagnostic cannot catch it at the lexer any more; what catches it is the type
checker, when a `Bool` is bound where a value was wanted, and that message will
not mention error propagation unless somebody makes it. This is the one row in
§2 where revision 2 made the situation *worse*: a hard error became a silent
change of meaning. What to do about it is a diagnostics decision and is not
settled here; no code is claimed for it.

### 2.3 Actively misleading

This is the worst category, and it is the biggest one.

| Habit | Diagnostic today | Why it misleads |
|---|---|---|
| `fn main():` | `SC0101` — *expected `implements` or `has methods` after `fn`* | `fn` was read as a **type name**, so the compiler proposes writing `fn implements …`. A model will try it. |
| `struct Doc:` | `SC0101` — *expected `implements` or `has methods` after `struct`* | same |
| `enum Format:` | `SC0101` — *expected `implements` or `has methods` after `enum`* | same |
| `impl Summarize for Doc:` | `SC0101` — *expected `implements` or `has methods` after `impl`* | same, and the fix is a **reordering** (`Doc implements Summarize`), which no generic message can suggest |
| `pub function f():` | `SC0101` — *expected `implements` or `has methods` after `pub`* | same |
| `let mut x be 1` | `SC0100` — *expected `be`, found `x`* | `mut` was read as the bound name. The compiler thinks the binding is called `mut`. |

The cause is structural and worth naming: the English redesign made an item able
to **start with a bare name** (`Doc implements Summarize:`), so every unknown
leading word is now a plausible type name and falls into the `implements`
branch. The old grammar would have rejected `fn` outright; the new one welcomes
it and then complains about the wrong thing.

### 2.4 No targeted diagnostic at all

| Habit | Diagnostic today | Cascade |
|---|---|---|
| `-> Int` | `SC0100` — *expected end of line, found `-`* | 0 |
| `&Doc` | `SC0104` — *expected a type, found `&`* | **2** |
| `&mut Doc` | `SC0104` — *expected a type, found `&`* | **2** |
| `dyn Summarize` | `SC0100` — *expected `)`, found `Summarize`* — never mentions `dyn` | **1** |
| `Array[Int]` | `SC0100` — *expected `)`, found `[`* | **1** |
| `function f[T](x)` | `SC0100` — *expected `(`, found `[`* | **1** |
| `function f(&self)` | `SC0102` — *expected an identifier, found `&`* | 0 |

`dyn Summarize` is the standout failure: the word `dyn` is consumed silently as
a type name and the error lands on the *next* token. Nothing in the output tells
the author that `dyn` was the problem.

### 2.5 Reserved-word collisions

These belong to `reserved-words.md`, but they are LLM-facing failures too and
the inventory would be dishonest without them.

| Habit | Diagnostic today | Cascade |
|---|---|---|
| `x.shape` | `SC0102` — *expected an identifier, found `shape`, which is reserved for a later phase* | **1** |
| `x.at(0)` | `SC0102` — *expected an identifier, found `at`* | **1** |
| `x.union(y)` | `SC0102` — *… `union`, which is reserved …* | **1** |
| `let model be 1` | `SC0102` — *… `model`, which is reserved …* | 0 |
| `function f(tensor: Int)` | `SC0102` — *… `tensor`, which is reserved …* | **1** |

The dot rule proposed in `reserved-words.md` §0.1 eliminates the first three
outright, cascade included.

### 2.6 Summary

Fourteen habits. Two have an applicable fix. Three have the right words and no
fix. **Nine either mislead or say nothing useful**, and eight of the fourteen
produce a cascade.

---

## 3. What a migration diagnostic should look like

Three requirements, in priority order. They are not stylistic.

**1. Name the construct, not the token.** The author wrote a *shared borrow*;
they spelled it `&T`. The message must contain both halves: what they meant, and
how Science spells it. *expected a type, found `&`* contains neither.

**2. Carry an applicable fix.** A `Suggestion` with a span and replacement text
is machine-applicable. Prose is not. An agent loop that can apply a fix converges
in one round trip; an agent loop that must infer the fix from prose sometimes
does not converge at all. Science already has the machinery — `Suggestion`,
rendered by `science-diagnostics`, tested byte for byte in `tests/ui/`. It is
used twice.

**3. Do not cascade.** When the lexer or parser recognises a known foreign
construct, it should consume the *whole* construct and recover as though the
correct form had been written. `&mut Doc` should produce one error and then parse
as `mutable borrowed Doc`, so the rest of the signature checks normally. This is
the same discipline the lexer already applies to a malformed numeric literal:
consume it all, emit one diagnostic, keep the stream usable.

### 3.1 The shape, concretely

```
error[SC0120]: a shared borrow is written `borrowed T`
  --> model.science:4:14
   |
 4 | function f(x: &Doc) returns Int:
   |               ^ Science spells this `borrowed`
   |
   = note: an exclusive borrow is `mutable borrowed T`, and a call site needs
           neither — a parameter declared `borrowed` is borrowed automatically (§6.3)
help: write the borrow as a word
   |
 4 | function f(x: borrowed Doc) returns Int:
   |               ~~~~~~~~
```

The note carries the *second* thing a Rust programmer gets wrong (writing `&x`
at the call site), which is the cheapest place to pre-empt it.

### 3.2 Diagnostics this note asks for

One new block, `SC0120`–`SC0134`, inside the syntax range §9 allocates.

| Code | Fires on | Fix |
|---|---|---|
| `SC0120` | `&T` in type position | `borrowed T` |
| `SC0121` | `&mut T` in type position | `mutable borrowed T` |
| `SC0122` | `&self` / `&mut self` | `self` / `mutable self` |
| `SC0123` | `dyn T` | `any T` |
| `SC0124` | `-> T` where `returns` belongs | `returns T` |
| `SC0125` | `Name[T]` in type position | `Name of T`, or `Name of (A, B)` for several |
| `SC0126` | `name[T](…)` in declaration position | `name of T(…)` |
| `SC0127` | `fn` starting an item | `function` |
| `SC0128` | `struct` starting an item | `type` |
| `SC0129` | `enum` starting an item | `choice` |
| `SC0130` | `impl Trait for Type:` | `Type implements Trait:` — a reordering fix |
| `SC0131` | `impl Type:` | `Type has methods:` |
| `SC0132` | `pub` starting an item | `public` |
| `SC0133` | `mut` after `let` | `mutable` |
| `SC0134` | postfix `?` | **void since revision 2** — `?` is the presence test and `try` does not exist. The code is not reassigned here (see §2.2) |

`SC0127`–`SC0132` must be checked **before** the `implements` branch in
`parse_item`, which is what §2.3 diagnosed. That ordering is the whole fix for
six of the nine bad cases.

`SC0130` needed real work, because the fix is not a substitution at one span:
`impl Summarize for Doc:` has to become `Doc implements Summarize:`. `Suggestion`
must therefore admit a multi-span edit, or it ships as a note.

`SC0134` was the second customer for that extension and it no longer is, because
the edit it wanted — `f()?` to `try f()` — has no destination. That matters
beyond this table: `def-and-lambda.md` §11 item 2 counts `SC0130`, `SC0134` and
`SC0135` as "three customers" for multi-span `Suggestion` and calls that "past the
threshold". With `SC0134` void the count is two, and whether two is still past
the threshold is that note's question to re-ask.

---

## 4. (b) Canonicalisation for `sciencec fmt`

Science has two spellings for two things. §4.6 justifies both and this note does
not reopen either — the argument there is sound, and the audience reason (*a
physicist writing an inequality wants `>=`*) is real. The question is only what a
formatter does when it meets them, so that generated code comes out consistent
instead of drifting.

### 4.1 Comparison phrases vs symbols — the formatter must not choose

The tempting rule is "symbols for formulas, words for prose", which is what §4.6
says and what the corpus does. It cannot be mechanised, and the corpus proves it:

```science
self.x == other.x and self.y == other.y     # 06_traits — symbols
self.text is other.text                     # 06_traits — words
```

Both are field accesses compared for equality. The only thing distinguishing
them is that one pair is `F64` and the other is `String` — **type information**,
which `sciencec fmt` does not have and should not need. A formatter that must
run after type checking is not a formatter.

So the decision is:

> **`sciencec fmt` never rewrites a comparison from one spelling to the other.**

What it does instead is enforce **local consistency**, which is syntactic and
therefore mechanisable:

- Within a single boolean expression — one connected tree of `and` / `or` — all
  comparisons take the same spelling.
- The majority spelling in that tree wins. A tie goes to symbols, because a tie
  means the expression is formula-shaped enough to have symbols in it at all.
- The rule stops at the boolean tree. Two separate `if`s in one function may
  disagree, because they are about different things.

This is deterministic, idempotent, and it fixes the actual failure mode of
generated code, which is not "chose the wrong spelling" but "chose both in one
line":

```science
# before — what a model writes
if doc.title is not "" and doc.body.length() >= 200 and doc.hits > 0:

# after — three comparisons, symbols in the majority, so symbols
if doc.title != "" and doc.body.length() >= 200 and doc.hits > 0:
```

The *style* rule — which spelling to reach for in the first place — belongs in
`AGENTS.md`, where the generator does know the types. §5 states it.

### 4.2 `each` vs `giving` — the formatter can and should choose

This one is mechanisable, because the deciding fact is syntactic: how many times
the parameter is mentioned.

> **Canonical form is `each` when the closure's parameter occurs exactly once in
> its body and the closure is not nested inside another closure's argument.
> `giving` otherwise.**

`sciencec fmt` rewrites `giving` to `each` when the condition holds, substituting
`each` for every occurrence of the parameter — of which there is one — and never
rewrites in the other direction.

```science
docs.map(doc giving doc.title)          ->  docs.map(each.title)
docs.map(doc giving doc.a + doc.b)      ->  unchanged: the parameter occurs twice
outer.map(o giving o.inner.map(each.x)) ->  unchanged: nested
docs.sort(by: line giving line.length()) -> docs.sort(by: each.length())
```

Idempotent, because `each` has no parameter to count and is never rewritten back.
Safe, because the nesting exclusion is exactly the condition `SC0115` already
rejects — the formatter never produces a program the parser refuses.

### 4.3 What else the formatter should canonicalise

Three more, all syntactic, all sources of drift in generated code:

- **Generic arguments.** `Array of (T)` → `Array of T` when the single argument
  needs no parentheses; parentheses kept when the argument itself contains a
  top-level comma or a nested `of`. The corpus is currently inconsistent here and
  one file had to be fixed by hand.
- **Block form.** An inline body stays inline; an indented body stays indented.
  The formatter does not convert between them, because §4.5 makes the choice
  meaningful — the inline body ends where the expression ends.
- **Explicit borrows at call sites.** `f(borrowed x)` → `f(x)` when the parameter
  is declared `borrowed`. This one **does** need type information, so it belongs
  to a lint, not to `fmt`. Noted here so it is not accidentally put in `fmt`.

---

## 5. (c) `AGENTS.md`

Written to the repository root as part of this note. Its content is reproduced
here in summary only; the file itself is the artefact.

It has to do one job: **let a model write correct Science on the first attempt,
without a training corpus.** That makes it a different document from a tutorial.
A tutorial teaches in the order a human learns. `AGENTS.md` should be ordered by
*what a model will get wrong*, most likely first, because a model reads it as
context and the top of the context is what survives.

Its sections, in that order:

1. **The translation table**, first, because analogy from Rust is the failure
   mode. Fourteen rows, each with the wrong form and the right one.
2. **The five rules that have no Rust or Python analogue** and so cannot be
   guessed: `be` for both binding and assignment; calls always take parentheses;
   `of` for generic arguments with the parenthesisation rule; `?` is a postfix
   presence test and failure is a second return value, not a propagation
   operator; blocks are indentation and the inline body ends where the
   expression ends.
3. **The reserved-word list**, with the collisions spelled out, because a model
   reaching for `model` as a variable name is near-certain.
4. **Style**: which comparison spelling to use — the type-informed rule the
   formatter cannot apply — and when `each` beats `giving`.
5. **How to verify**: the exact commands, and the fact that `examples/` is the
   corpus of known-good programs and `tests/ui/` the corpus of known-bad ones.

The last point is the one that compounds. A model that knows
`cargo test -p science-lexer --test corpus` exists, and that `examples/` contains
twenty programs which are *guaranteed* to be valid, has a reference implementation
to imitate and a check it can run. That is worth more than any amount of prose.

---

## 6. Risks worth naming

**The fixes become the training data.** If `SC0120`–`SC0134` are good, models
will learn Science through them, and the phrasing of those fifteen messages will
shape how a generation of generated Science reads. They deserve the same care as
the syntax itself, and they should be tested in `tests/ui/` byte for byte, which
the existing harness already supports.

**Migration diagnostics have a shelf life, and it is long.** They look like
scaffolding for an audience arriving from Rust. They are not: they are permanent,
because the models will keep arriving from Rust for as long as Rust outweighs
Science in the training mix, which is indefinitely.

**A fix that is wrong is worse than no fix**, because it is applied
mechanically. `SC0130`'s reordering is the one to be careful with: `impl A for B`
→ `B implements A` is right, but `impl A:` → `A has methods:` and
`impl A for B:` differ only by the `for`, and a fix that confuses them produces a
program that compiles and means something else. It needs a test per shape.
