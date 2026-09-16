# Science — Design: `def` and `lambda`

Date: 2026-09-16
Status: draft for review
Owns: the spelling of the function-declaration keyword, and the question of a
third closure form.
Depends on: `syntax-revision-2.md` (all samples here are in its syntax),
`../specs/2026-09-16-science-f0-core-design.md` §4.1, §4.6, §13,
`llm-ergonomics.md` §3 and §4.2, `reserved-words.md` §0.1 and §2.
Diagnostic block: `SC0119`, `SC0135`–`SC0137`. `SC0138`–`SC0139` are returned
unclaimed to the free pool (§9.3).

---

## 0. The two answers, first

Both requests are declined, and the reasons are different in kind.

| Request | Answer | Decided by |
|---|---|---|
| `function` → `def` | **No.** Keep `function`. | The keyword rule, §4.1, as revision 2 left it — see §1. |
| add `lambda` | **No.** Keep `each` and `giving`. | The reserved-word audit — see §5. `lambda` is a binding-position collision in five fields. |

Neither answer is "the change is small, so no". `def` is declined because it is
the only proposal anyone has made that breaks the one clause of §4.1 still
standing after revision 2, and once that clause goes there is no principle left
to answer the next request with. `lambda` is declined because `let lambda be
532<nm>` is a line a physicist writes in their first hour, and because the form
it would add is *longer* than the form the language already has.

§2 makes the strongest case for `def` I can make, because it is a better case
than it first looks, and §3 says why it still loses. §10 names the argument that
nearly moved me.

---

## 1. What is left of §4.1 after revision 2

The founding sentence is:

> Keywords are English words, and no keyword is an abbreviation or a symbol
> where a word would do. `function`, not `fn`. `returns`, not `->`. `for each`,
> not `for`.

Two of its three examples are dead. It is fair to ask whether the sentence died
with them. It did not, and the way it survived is the whole argument here.

### 1.1 The sentence is two rules wearing one coat

Read it as written and there are two independent prohibitions:

1. **No keyword is an abbreviation.** `function`, not `fn`.
2. **No keyword is a symbol where a word would do.** `returns`, not `->`.

The third example, `for each` not `for`, is neither. `for` is a whole English
word and not a symbol; the change revision 2 made there deleted a *redundant*
word, it did not shorten one. §4.1 listed it as an example of a rule it is not
an example of.

Revision 2 §4 overturned prohibition 2, and said exactly which one it was
overturning:

> `->` is not an abbreviation of an English word, it is a symbol that every
> reader of Rust, Go, Swift, Kotlin, TypeScript and Python type hints already
> reads as "returns".

That sentence is careful. It defends `->` specifically on the ground that it is
*not an abbreviation*. Revision 2 did not weaken prohibition 1; it leaned on it.

### 1.2 So the surviving rule is one clause, and it is exact

> **No keyword is an abbreviation. A keyword is either a whole English word or
> a symbol that every reader of scientific and programming notation already
> reads.**

This is revision 2 §0's replacement rule — *a word where the operation has no
universal symbol, the symbol where one exists and everyone reads it* — with the
part §0 left implicit made explicit: the two categories are exhaustive. There is
no third category, and a shortened word is in neither. `def` is not a symbol. It
is a word with four letters removed.

Test the live keyword list against the one-clause rule. After revision 2 the
list is `function return let be mutable type choice interface implements has of
borrowed any for in if else match loop break continue use where as self Self and
or not true false is const public giving each`.

Every entry is a whole English word, with **one exception**.

### 1.3 The exception is `const`, and it matters

`const` is an abbreviation of `constant`. It is in §13's in-use list, it is
`TokenKind::Const` in `crates/science-lexer/src/token.rs`, and it fails the rule
as plainly as `fn` does.

This is worth stating loudly rather than burying, because it is the strongest
argument the author has for `def` and it is not in the original request: *the
rule is already broken once, so what is one more?*

The answer is that one exception is an exception and two is a pattern. With
`const` alone, the rule still has a predicate: a reviewer can say "that is an
abbreviation, and the only abbreviation in the language is `const`, which is a
mistake we kept". With `def` beside it, the rule becomes "whole words, plus a
universal symbol, plus `const`, plus `def`" — which is not a rule, it is a list,
and a list cannot answer a question it does not already contain. The next
requests are `elif`, `impl`, `str`, `len`, `mut`, `pub`, `fn`. Every one of them
has the identical justification `def` has — Python or Rust spells it that way,
the audience types it by reflex, it is shorter at the start of a line — and
under a list there is no principled answer to any of them. Under the one-clause
rule there is one answer and it takes a sentence.

**Decision.** The surviving rule is §1.2, stated in one clause, and it is
retained. **Rejected alternative:** declaring §4.1 spent and replacing it with
case-by-case taste. Reason: the language has made eleven keyword decisions this
way and every one of them is defensible in the same sentence; taste does not
scale past the first disagreement, and revision 2 §10 already set the bar — *a
third revision should be held to a much higher bar*.

**Cost, stated.** This note is asking the author to keep an eight-character
keyword for a reason that is architectural rather than felt. That is a real
cost and §2 prices it.

### 1.4 The consequential ask: fix `const`

If the one-clause rule is retained, `const` should become `constant`, so the
rule has no exception and `def` has no precedent.

This is a cheap change — `constant` is not a reserved word today, the token
rename is mechanical, and `reserved-words.md` §2 already wants the `const`
*module* renamed to `constants` for a different reason, so the two changes agree
rather than collide. It is recorded here as an ask (§11), not decided here,
because `const` belongs to §4.3 and not to this note.

If the author would rather keep `const`, that is defensible too — but then §4.1
should say so in writing, naming `const` as the one deliberate exception, so
that the next `def`-shaped request meets a stated exception list rather than a
silent one.

---

## 2. The case for `def`, made as well as I can make it

Four arguments, and the fourth is serious.

**1. The audience writes Python all day.** `llm-ergonomics.md` §1's whole premise
is that Science has no training corpus and that analogy from a familiar language
is the failure mode. Meeting the analogy halfway removes a failure instead of
diagnosing it. `AGENTS.md` §1 is a fourteen-row translation table; adopting `def`
deletes a row rather than adding one.

**2. Line-start weight.** `function` is eight characters before the name, at the
start of every declaration in every file.

```science
function summarize(self) -> String:      # 35 characters
def summarize(self) -> String:           # 30
```

Five characters, times every declaration. More than the count: the *name* — the
thing a reader scans a file for — starts at column 10 instead of column 5, and
inside an `interface` or a `has:` block it starts at column 14 instead of
column 9.

**3. Every `def` is a compile error today, and each one costs a round trip.**
Models emit `def` by reflex. `llm-ergonomics.md` §2.6 counts fourteen such
habits and finds nine of them mislead. `def` is currently a fifteenth with no
diagnostic at all.

**4. Revision 2 already chose familiarity over English once, and said that was
the correction.** §0 of that note:

> where they diverged the first pass chose English and should have chosen
> familiarity.

That is the author's own sentence, it is about exactly this trade, and on its
face it points at `def`.

---

## 3. Why `def` loses anyway

### 3.1 Argument 4 is answered by the sentence next to it

Revision 2 §0 did not say "choose familiarity". It said the first pass chose
English where it should have chosen familiarity, and then it stated the rule it
was substituting: *a word where the operation has no universal symbol, the
symbol where one exists and everybody already reads it.* Every one of revision
2's eight changes follows from that rule. `def` follows from none of it. It is
not a symbol; there is no symbol for declaring a function; the rule's first
clause therefore applies and its answer is "a word".

Familiarity was the *motive* for revision 2, not its *rule*. A motive cannot
adjudicate the case where it points one way and the rule points the other, which
is this case.

### 3.2 On familiarity itself, `function` wins the audience

This is the argument I did not expect to find, and it is the one that settles it.

The target audience is scientific computing. Count the languages that audience
actually writes:

| Language | Spelling |
|---|---|
| Fortran | `FUNCTION` |
| MATLAB | `function` |
| R | `function` |
| Julia | `function` |
| JavaScript / TypeScript | `function` |
| PHP | `function` |
| **Python** | **`def`** |

Six to one, and the six include the three languages this project names as its
competition. `reserved-words.md` and `scientific-libraries.md` both build their
collision audits on what a physicist or a chemist types; a physicist who has
written MATLAB and Fortran has typed `function` far more often than `def`. The
familiarity argument, examined, does not favour `def`. It favours *Python*, and
Python is one dialect of the audience rather than the audience.

The counter-counter is honest and should be recorded: the code that will be
*written* is mostly written by models, and models' priors are dominated by
Python, not by Fortran. That is true. It is also exactly what a migration
diagnostic is for, and §9.1 specifies one. A one-round-trip fix with an
applicable suggestion costs the model one round trip; a broken keyword rule
costs the language every future keyword argument.

### 3.3 Revision 2 already refunded the characters

`function summarize(self) -> String:` is 35 characters. Under the *original*
spec it was `function summarize(self) returns String:` — 40. Revision 2 §4 spent
its familiarity budget on `->` and bought back six characters per signature,
more than `def` would buy, on the half of the line where signatures actually get
long. Argument 2 is asking for a second refund on a line that has already been
refunded once.

### 3.4 Decision

**Decision.** `function` is retained. `def` is not adopted and is not reserved.

**Rejected alternatives:**

- **`def`.** Breaks §1.2, the only clause of §4.1 that revision 2 left standing,
  and converts the keyword rule from a predicate into a list (§1.3). Loses the
  familiarity argument on the actual audience (§3.2).
- **`define`.** Fits the rule — it is a whole English word — and saves two
  characters. Rejected because it buys almost nothing and costs the one thing
  `function` has: it is a *noun naming the thing declared*, which is what every
  other declaration keyword in Science is (`type`, `choice`, `interface`). A
  verb in a noun's slot is a worse fit than two extra characters is a cost.
- **`func`.** An abbreviation. Fails §1.2 in the same way as `def`, with none of
  the familiarity.
- **Making `function` optional inside `has:` and `interface:` blocks**, so that
  `Doc has:` is followed by `new(title: String) -> Doc:`. This is the only
  option on the table that would actually shorten the declarations that are
  worst off — the nested ones — and it parses, because a `has:` body has no
  fields, so a name followed by `(` cannot be anything but a function. It is
  named here rather than buried because it is the nearest thing to the author's
  grievance that fits the rules. It is declined for two reasons: it creates a
  second spelling of a declaration, which is the same debt §4 of this note is
  spending its argument to avoid; and it makes a free function and a method
  spelled differently, so moving a function between the two contexts becomes an
  edit rather than a move. If the character count at the declaration head ever
  becomes a felt problem rather than an argued one, this is where to relent, and
  it is source-compatible to add later.

**Cost, stated.** Models will write `def` indefinitely, and every one of those is
an error. §9.1 makes it a one-round-trip error with an applicable fix, and
`AGENTS.md` (§8) pre-empts it. That is the whole mitigation and it is not
complete: a fix applied is still a fix that had to be applied.

---

## 4. `lambda`: which of the four it is

The question posed was whether `lambda` is (a) a third mechanism, (b) sugar over
`giving`, (c) a replacement for `giving`, or (d) rejected.

The answer is **(d)**, and the reason that finally decides it is §5 rather than
anything in this section. But the mechanism question is worth settling first,
because it shows that even if the vocabulary collision did not exist, there is
nothing here to adopt.

### 4.1 It cannot be (a), because there is no third mechanism to be

§4.6 defines two closure forms because they differ in a real property: `each`
omits the binder and `giving` declares it, and the compiler can tell them apart
because one of them cannot nest (`SC0115`).

`lambda x: x > 5` and `x giving x > 5` differ in **no property at all**. Both
declare exactly one binder. Both capture the same way. Both nest the same way.
Both produce the same value. Put them through the parser and both land on

```rust
ExprKind::Closure { param: Option<Ident>, body: Box<Expr> }
```

with `param: Some(x)` — the node that exists today at
`crates/science-parser/src/ast.rs:475`. Their `ast_dump` snapshots would be
**byte-identical**. For a project whose §10.2 makes AST snapshots a testing
layer, that is not an analogy: it is the project's own instrument reporting that
the two constructs are the same construct. A form that cannot be distinguished
in the tree is not a mechanism. It is a second name.

### 4.2 (b) is the honest reading, and it is what makes it fail

So `lambda` can only be **sugar that desugars to the same node**, which is
option (b). That is what the author's own example shows:

```science
# asked for
xs.keep(lambda x: x > 5)

# already legal today, and one character shorter
xs.keep(x giving x > 5)

# and the canonical form under llm-ergonomics.md §4.2, which is shorter still
xs.keep(each > 5)
```

`lambda x: x > 5` is fifteen characters. `x giving x > 5` is fourteen. The
canonical `each > 5` is eight. The request is for a third spelling that is
longer than both spellings it would join, of a thing the language already says
two ways.

The author's constraint — *no quiero tres formas de escribir lo mismo sin
control* — is not merely satisfied by rejecting this. It is the exact objection,
and the author found it before this note did.

### 4.3 (c), replacing `giving`, is the one that deserved a hearing

`giving` is the language's most unusual word. No other language uses it. A model
with no Science in its training mix will not produce `x giving x > 5`
spontaneously; it will produce `lambda x: x > 5` or `|x| x > 5`. On
`llm-ergonomics.md`'s own premise — *diagnostics are the only teaching channel* —
a keyword the model already knows is worth something real, and swapping one
keyword for another adds no form at all.

It fails on three counts, in increasing order of weight:

1. **`giving` reads in a chain and `lambda` does not.** `docs.sort(by: line
   giving line.length())` says what it does in the order it does it. `docs.sort(by:
   lambda line: line.length())` names a letter of the Greek alphabet in the
   middle of an English sentence. §4.1's whole thesis is that the code says what
   it does.
2. **It is a symbol spelled out, which §1.2 does not license either.** `lambda`
   is the *name of the symbol* λ, from a 1936 notation. It is not an English
   word for the operation; the English word for the operation is what `giving`
   is. Adopting it would be the first keyword in Science chosen for its history
   rather than its meaning.
3. **§5.** The vocabulary collision, which is fatal on its own.

### 4.4 Decision

**Decision.** `lambda` is **not adopted, in any of the three positive forms, and
is not reserved.** The closure forms remain exactly the two of §4.6: `each` and
`giving`.

**Rejected alternatives:**

- **(a) a third mechanism** — there is no third mechanism; the AST proves it
  (§4.1).
- **(b) sugar over `giving`** — a third spelling, longer than both existing
  ones, of a construct the author has already said should not have three
  spellings. And see §7: `sciencec fmt` would delete every occurrence on first
  run, because no syntactic property distinguishes it from `giving`.
- **(c) replacing `giving`** — the only version with a real argument (models
  know the word), defeated by §5.

**Cost, stated.** `giving` stays a word no model has seen, and models will keep
reaching for `lambda`. §9.2 turns that into a one-round-trip diagnostic, and it
does so *without reserving the word* — which is the part worth the design effort,
because §5 is the reason the word must stay free.

### 4.5 The real gap, and the thing that does fit

There is one thing `lambda` would genuinely have bought and `giving` does not
provide: **more than one parameter.** `ExprKind::Closure` carries
`param: Option<Ident>` — one, or none. There is no way to write a two-argument
closure in Science today, and `collections-and-chains.md`'s `fold` needs one the
day it lands.

That gap is real and it is worth closing. The nearest thing that fits the
existing language is to extend the word already in it:

```science
totals.fold(0, (acc, x) giving acc + x)
```

This reuses the existing keyword, adds no form, keeps the chain reading, and
costs exactly one AST change: `param: Option<Ident>` becomes
`params: Vec<Ident>`, with the implicit-subject form carrying the empty vector.
It is offered here and **not decided here**, because multi-parameter closures
belong to whoever owns `collections-and-chains.md`'s `fold`. §11 asks for it.

Note what this does to the `lambda` case: the *only* thing `lambda` had that
`giving` lacked is obtainable without `lambda`, and — per §6.1(e) — it is
obtainable *only* without `lambda`, because the multi-parameter form is the one
`lambda x: …` cannot parse.

---

## 5. Reserved-word audit

In the style of `reserved-words.md` §2. `K` = keyword in use, `R` = reserved not
in use, `—` = free. Status verified against
`crates/science-lexer/src/token.rs`: neither word appears in `TokenKind` nor in
`ReservedWord` today.

| Word | Now | Collides with | Position | Decision | If adopted anyway |
|---|---|---|---|---|---|
| `def` | — | `def` as a local in symbolic-algebra and parser code; `default` and `deficiency` are separate identifiers and do not collide | Binding (rare) | **Leave free.** Not adopted (§3.4); not reserved | Low collision cost — `def` fails on the keyword rule, not on the vocabulary |
| `lambda` | — | λ in five fields; see §5.1 | **Binding and parameter** | **Leave free.** Not adopted (§4.4); not reserved | Unacceptable — see §5.2 |

The `def` row is the honest one: **the reserved-word audit does not reject
`def`.** If the author overrules §3.4, no scientist loses an identifier. That
decision is §4.1's to make and this note should not pretend otherwise.

### 5.1 `lambda`, priced

λ is not a fringe symbol. It is a primary named quantity in five fields, and in
every one of them it lands in **binding or parameter position** — the two
positions `reserved-words.md` §0.1's dot rule does *not* rescue, because the dot
rule only frees words that follow a `.`.

| Field | λ is | A line someone writes | Library |
|---|---|---|---|
| Optics, spectroscopy | wavelength | `let lambda be 532<nm>` | `physics`, `signal` |
| Nuclear physics, chemical kinetics | decay constant | `let n be n0 * exp(-lambda * t)` | `physics`, `chem` |
| Linear algebra | eigenvalue | `let lambda, v be eig(a)` | `linalg` |
| Statistics | Poisson / exponential rate | `let lambda be events / interval` | `stats` |
| ML, optimisation | ridge and lasso regularisation strength; Lagrange multiplier | `function ridge(x: Matrix, y: Vector, lambda: F64) -> Vector:` | `optimize`, `stats` |

Two of these deserve calling out because they are worse than the others.

**`let lambda be 532<nm>`** is not a hypothetical. `unit-literals.md` makes that
exact literal a supported form, so the note that is building unit literals for
spectroscopy and the note reserving `lambda` would be shipping against each
other in the same phase.

**`let lambda, v be eig(a)`** is the worst single case in the table. Revision 2
§3.1 made multiple returns with destructuring the *normal* way to receive a
result, so eigen-decomposition is written as a two-name binding, and the first
name is λ. That is a binding position in the one place the language most wants
to be idiomatic for a numerical audience.

And **`lambda: F64`** as a parameter name is the ML case. `models-and-inference.md`
and `scientific-libraries.md` both catalogue `optimize`; a regularised regression
whose penalty parameter cannot be called `lambda` will call it `lam` or `lmbda`,
which is what NumPy-adjacent Python does when a word is taken, and it is exactly
the abbreviation-as-workaround §4.1 exists to prevent.

### 5.2 Why this is decisive rather than merely expensive

`reserved-words.md` §3 is currently arguing to **free** `shape`, `model` and
`tensor`, on the ground that a reserved word in binding position, colliding with
the audience's own vocabulary, is too expensive to keep even when a future phase
wants it.

`lambda` is a worse collision than any of those three:

- `shape`, `model` and `tensor` each collide in **one** area of practice.
  `lambda` collides in five, spanning at least five of the eight libraries
  `scientific-libraries.md` catalogues.
- `shape` is mostly a *member* collision (`x.shape`), and the dot rule fixes it
  outright. `lambda` is never a member. It is a name someone binds.
- The three are reserved to protect **F1 and F4 syntax that the spec promises**.
  `lambda` would be reserved to provide a **third spelling of something that
  already has two**.

Adopting `lambda` while a sibling note argues to free `shape` would be
incoherent, and a reviewer reading both notes in sequence should be able to see
that it would be. That is the whole of §5's argument and it does not need §4.

---

## 6. The separator problem, answered even though it does not arise

`lambda` is rejected, so this section is not load-bearing. It is written because
the question was asked precisely, because the answer contains the finding that
sinks §4.5's multi-parameter case, and because a future note proposing any
`X sep expr` form will need the same analysis.

Facts the analysis rests on, from
`crates/science-parser/src/parser.rs`:

- `at_named_args` (line 1721) tests `LParen`, then `Ident`, then `Colon` — two
  tokens of lookahead past the paren. That is the parser's entire lookahead
  budget today.
- `parse_call_args` (line 1735) tests `Ident` then `Colon` for a named argument.
- `parse_arm_body` (line 1205) consumes exactly one `Colon` and then parses an
  expression; arms are newline-terminated by `parse_indented_body`.
- `parse_expr` (line 1364) recognises the `giving` closure by testing `Ident`
  then `Giving` *above everything else*, because "there is no operator `giving`
  could be an operand of".

### 6.1 `:` — parses for the form asked for, fails silently for the form wanted

`lambda` would be a keyword, so `Lambda Ident Colon` is a three-token prefix
distinct from everything in the grammar. Taken position by position:

**(a) As a bare argument, `f(lambda x: x > 5)`.** `parse_call_args`'s
named-argument test wants `Ident` at `peek()`; it sees `Lambda`, a keyword, so it
does not fire. The lambda is parsed as the argument value. **Unambiguous.**

**(b) As a named argument, `f(by: lambda x: x > 5)`.** The named-argument test
fires on `by` `:`, consumes both, and `parse_expr` then meets the lambda with its
own `:`. The two colons never compete for the same position.
**Unambiguous** — and this is the case the brief flagged as the hard one, so it
is worth being explicit that it is not.

**(c) In a record construction, `Doc(title: lambda x: x)`.** `at_named_args`
tests `( title :` and is satisfied before the lambda is reached.
**Unambiguous.**

**(d) In a `match` arm, `Missing(v): lambda x: x + v`.** `parse_arm_body` takes the
first `:` as the arm separator and hands the rest to `parse_expr`; the lambda
takes the second. The arm cannot run into the next one because
`parse_indented_body` is newline-delimited. **Unambiguous to the parser** — and
illegible to a reader, which is a separate and real cost: one line, two colons,
two unrelated meanings, one opening a body and one separating a parameter. §4.1's
warning about English is the same warning in another key: a reader who meets a
familiar mark assumes the familiar meaning.

**(e) With more than one parameter, `f(lambda a, b: a + b)`. This is where it
breaks, and it breaks the worst way.** `parse_call_args` splits arguments on the
top-level comma before any lambda knows it wanted `b`. What remains, `b: a + b`,
satisfies the named-argument test *exactly*. The call parses — silently — as two
arguments: a malformed `lambda a`, and a named argument `b`. A silently wrong
parse is the category `llm-ergonomics.md` §2.3 calls *actively misleading*, which
that note ranks below saying nothing at all.

The escape is `lambda (a, b): a + b`, parenthesising the parameter list — which
is a fourth spelling of a parameter list in a language that has one. So `:` does
not fail on the form the author asked for. It fails on **the only form that would
have justified adding `lambda` at all** (§4.5).

**(f) Block bodies.** `lambda x:` followed by a newline is, in shape, a
block-opening `:` — and §4.5 makes "`:` and an indented body" the foundation of
the entire indentation grammar. Either `lambda` becomes the one expression in the
language that opens a block, or `:` means "opens a block" everywhere except after
`lambda`. Both are special cases in the rule that can least afford one. Python
resolves this by forbidding statements in a lambda; Science would have to do the
same and then explain why.

### 6.2 `->` — not ambiguous, and still wrong

`Arrow` occurs in exactly one place in the grammar: after a parameter list in a
declaration (revision 2 §4). It has no meaning in expression position. So
`lambda x -> x > 5` is **not grammatically ambiguous against a return type** —
there is no ambiguity of any kind, and a one-token test settles it everywhere.

The objection is semantic, and it is the same objection revision 2 §6.1 used to
reject `class`:

> an unfamiliar word makes a reader look it up, a familiar word used differently
> makes a reader confidently wrong.

In `function f(x: Int) -> Int` the `->` introduces a **type**. In
`lambda x -> x > 5` it would introduce a **value**. Revision 2 adopted `->`
precisely because "every reader … already reads it" — as *returns this type*.
Giving it a second meaning spends the property that justified taking it, and it
makes two unrelated constructs rhyme visually. Not ambiguous; still a net loss.

### 6.3 `giving` — self-refuting

`lambda x giving x > 5` is `x giving x > 5` with a five-letter noise word in
front of it. It is the shortest proof available that the request is for a name,
not for a mechanism.

---

## 7. `sciencec fmt`, and the argument that would have killed `lambda` anyway

`llm-ergonomics.md` §4.2 fixes the existing rule:

> Canonical form is `each` when the closure's parameter occurs exactly once in
> its body and the closure is not nested inside another closure's argument.
> `giving` otherwise.

**Decision.** That rule is **unchanged** and needs no extension, because a
`lambda` never reaches the formatter: it is a parse error and `SC0135` (§9.2)
catches it. One clarifying clause is added so the rule is total:

> A closure with more than one parameter is `giving`, unconditionally. `each`
> names one subject and has no spelling for two.

That clause is a no-op today and becomes live if §4.5's multi-parameter
extension is accepted. It preserves idempotence trivially: it never rewrites.

The formatter argument against `lambda` is worth stating on its own, because it
is independent of §5 and would have been sufficient.

Revision 2 §1.1 established the project's standard here: a spelling a formatter
cannot canonicalise is permanent drift, and that is what deleted `==`. `lambda`
fails a sharper version of the same test. A formatter *could* pick a rule — but
the rule has to be stated in terms of some syntactic property, and:

> **There is no syntactic property that distinguishes `lambda x: e` from
> `x giving e`.** Same binder count, same body, same nesting behaviour, same AST
> node, byte-identical snapshot (§4.1).

So every possible rule falls into one of two cases, and both are bad:

1. **`lambda` is canonical somewhere.** Then some syntactic criterion prefers it
   over `giving`, and no such criterion exists — the rule would have to be
   arbitrary, which means two formatter authors would write it differently, which
   is the drift the rule exists to prevent.
2. **`lambda` is canonical nowhere.** Then `sciencec fmt` rewrites every `lambda`
   to `giving` the first time it runs. The feature exists only in unformatted
   source: a model writes it, the formatter deletes it, and the model never sees
   it in the corpus it learns from. That is a keyword that costs a reserved word,
   a parser branch, a diagnostic and a spec section, and appears in zero
   formatted programs.

Case 2 is what `lambda`-as-sugar actually is, and it is the clearest statement of
why sugar over an identical node is not free.

---

## 8. AST and pipeline

Brief, because the answer is "nothing".

**With both rejected, no node changes.** `ExprKind::Closure { param:
Option<Ident>, body: Box<Expr> }` (`crates/science-parser/src/ast.rs:475`)
already carries both existing forms — `None` for `each`, `Some` for `giving` —
and `dump.rs:590` already prints both. `function` is `TokenKind::Function` and
unchanged. Nothing reaches HIR, MIR, the evaluator or codegen differently,
because nothing new reaches the parser.

**Had `lambda` been adopted as sugar (option b)**, still no node: it would
desugar in `parse_expr` to `Closure { param: Some(_), … }`, and the snapshot
tests would be unable to tell the two apart. §4.1 treats that as the argument
against rather than a convenience.

**The one change worth making** is §4.5's, and it is not free:
`param: Option<Ident>` → `params: Vec<Ident>`, with `each` carrying the empty
vector. That touches `ast.rs`, `dump.rs` (the `child_opt("param", …)` line
becomes a list), every AST-dump snapshot containing a closure, and resolution
where the binder is introduced. It is a contained change and it belongs to
whoever lands `fold`.

---

## 9. Diagnostics

Allocated from this note's block: `SC0119`, `SC0135`–`SC0137`.

| Code | Fires on | Fix |
|---|---|---|
| `SC0119` | `def` starting an item | `function` — an applicable replacement |
| `SC0135` | `lambda <name>:` / `lambda <name> ->` / `lambda:` in expression position | `<name> giving …` — an applicable multi-span edit |
| `SC0136` | *contingency only*: `function` starting an item, if §3.4 is overruled and `def` is adopted | `def` |
| `SC0137` | A closure with more than one parameter, in any spelling | names the one-parameter rule, or the parenthesised form once §4.5 lands |

### 9.1 `SC0119` — `def`

The Python sibling of `llm-ergonomics.md`'s `SC0127` (`fn` → `function`), and it
is placed at `SC0119` deliberately: immediately below that note's
`SC0120`–`SC0134` block, so the two migration diagnostics for the same mistake
from the two ecosystems sit together in the table.

It follows the template `llm-ergonomics.md` §3.1 sets and the shape of the
shipped *assignment is written `be`* diagnostic at
`crates/science-parser/src/parser.rs:1257` — name the construct, carry an
applicable `Suggestion`, do not cascade.

```
error[SC0119]: a function declaration is written `function`
  --> model.science:4:1
   |
 4 | def summarize(self) -> String:
   | ^^^ Science spells this `function`
   |
   = note: the keyword is the whole word, not an abbreviation: `function`,
           `mutable`, `public`, `implements` (§4.1)
help: write the keyword in full
   |
 4 | function summarize(self) -> String:
   | ~~~~~~~~
```

**No cascade.** Per `llm-ergonomics.md` §3's third requirement, the parser
consumes `def` and continues *as though* `function` had been written, so the rest
of the signature — parameters, `->`, the block — is checked normally and the file
produces one error rather than three. This is the same recovery
`SC0127`–`SC0132` require and it should share their code path: recognise the
foreign keyword in `parse_item` **before** the `implements` branch, which is the
ordering fix `llm-ergonomics.md` §2.3 already asks for.

The note line is doing deliberate work. It teaches the *rule* rather than the
one substitution, which is the cheapest place to pre-empt `mut`, `pub` and
`impl` — exactly as `SC0120`'s note pre-empts `&x` at the call site.

### 9.2 `SC0135` — `lambda`, without reserving the word

This is the one that needed design rather than transcription, because §5 forbids
reserving `lambda` and a diagnostic that requires a keyword token is therefore
off the table.

It does not need one. `lambda` stays an ordinary identifier, and the parser
recognises a **shape** instead:

> In expression position, an identifier spelled `lambda` followed by
> (`:`), or by (an identifier and then `:`), or by (an identifier and then `->`),
> is `SC0135`.

None of those sequences is a legal Science expression under any reading —
`lambda x :` in expression position has no parse at all — so recognising them
costs nothing and forecloses nothing. And crucially:

```science
let lambda be 532<nm>                                  # legal: `lambda` then `be`
function ridge(x: Matrix, y: Vector, lambda: F64):     # legal: parameter position
let lambda, v be eig(a)                                # legal: binding position
let peak be lambda * 2                                 # legal: `lambda` then an operator
xs.keep(lambda x: x > 5)                               # SC0135
```

Every line §5.1 said a scientist writes still compiles. Only the shape no
scientist writes is diagnosed.

```
error[SC0135]: a closure is written `name giving expression`
  --> filter.science:7:13
   |
 7 |     xs.keep(lambda x: x > 5)
   |             ^^^^^^^^^ Science has no `lambda`
   |
   = note: `lambda` is not reserved — `let lambda be 532<nm>` is a valid binding.
           It is only this shape that has no meaning.
   = note: when the parameter is mentioned exactly once, `each` is shorter
           still: `xs.keep(each > 5)`
help: name the parameter with `giving`
   |
 7 |     xs.keep(x giving x > 5)
   |             ~~~~~~~~~
```

This is a **multi-span edit** — delete `lambda`, move the name, replace `:` with
`giving` — so it needs the same `Suggestion` extension `llm-ergonomics.md` §3.2
already requires for `SC0130` and `SC0134`. That is a third customer for that
extension and it should be counted as one (§11).

The second note is the one that compounds: it points at the canonical form, so a
model that has applied the fix once learns `each` rather than learning `giving`
and stopping there. `sciencec fmt` would make that rewrite anyway (§7), and a
diagnostic that anticipates the formatter converges a round trip sooner.

### 9.3 `SC0136`, `SC0137`, and what is returned

**`SC0136` is written down but not claimed as live.** It is the contingency the
brief asked for: *one of the two words needs a migration diagnostic either way*.
If the author overrules §3.4 and adopts `def`, then `function` becomes the
foreign spelling — for the Fortran, MATLAB, R, Julia and JavaScript writers
counted in §3.2, who are the majority — and it needs the mirror diagnostic:

```
error[SC0136]: a function declaration is written `def`
  --> model.science:4:1
   |
 4 | function summarize(self) -> String:
   | ^^^^^^^^ Science spells this `def`
```

It is allocated now so that the decision, whichever way it goes, does not later
need a code from a block someone else has taken. The migration-diagnostic table
in `llm-ergonomics.md` §3.2 gets exactly one of `SC0119` and `SC0136`, never
both.

**`SC0137`** covers the parse failure §6.1(e) found, which exists today
independently of `lambda`: `f(a, b giving a + b)` parses as two arguments with no
complaint, because `b: …` — or here `b giving …` after the comma split — is a
plausible second argument. That is a silent wrong parse in the current compiler
and it is worth a diagnostic whether or not §4.5's extension lands.

**`SC0138` and `SC0139` are returned to the free pool.** This note does not need
them, and the README records four collisions caused by notes claiming ranges
they did not use. Holding two codes against a feature that has just been rejected
would be the fifth.

---

## 10. The argument on the other side I found most persuasive

Not Python familiarity, and not the character count. It is this, and it is the
author's own sentence from revision 2 §0:

> where they diverged the first pass chose English and should have chosen
> familiarity.

That sentence was written to justify `->`, and it generalises with no strain at
all to `def`. A project that has just reversed `returns` in favour of a symbol on
familiarity grounds, and reversed `for each` in favour of `for` on familiarity
grounds, is not in a strong position to refuse `def` on English grounds on the
same day. I do not think §3's answer fully dissolves that. It answers it —
revision 2 stated a *rule* alongside its motive, and the rule says "a word" here
— but the answer is a rule-versus-motive argument, and rule-versus-motive
arguments are exactly the ones that get reopened.

The thing that moved me off it was §3.2: `function`, not `def`, is the word this
particular audience has typed most. That is a familiarity argument *for*
`function`, which means the familiarity motive and the English rule do not
actually diverge here. They only appear to, if Python is mistaken for the
audience.

---

## 11. What this asks of other notes

1. **`llm-ergonomics.md` §3.2** adds one row to the migration table: `SC0119`,
   `def` starting an item, fix `function`. It sits directly above `SC0127`
   (`fn`), which is the same mistake from the other ecosystem.
2. **`llm-ergonomics.md` §3.2's multi-span `Suggestion` extension** gains another
   customer. It was asked for by `SC0130` (`impl A for B` → `B implements A`) and
   `SC0134` (`f()?` → `try f()`); `SC0135` needs it too.

   **This count no longer holds.** `syntax-revision-2.md` §3 makes `?` a presence
   test and deletes `try`, so `SC0134`'s edit has no destination and that note's
   §3.2 now records the row as void. The customers are `SC0130` and `SC0135` —
   two, not three — and whether two is still past the threshold that note uses
   for "worth the work" is a question for its owner, not settled here.
3. **`llm-ergonomics.md` §4.2** takes the clarifying clause in §7 — multi-parameter
   closures are `giving`, unconditionally. A no-op until item 5 lands.
4. **`reserved-words.md` §2** may record the two rows of §5 as *audited and
   deliberately left free*, which is a different and more useful status than
   *never considered*. `lambda` in particular should be recorded, because it is
   the strongest single case in the whole audit for the principle that note is
   arguing (§5.2), and it is a case where the answer was "do not reserve it"
   rather than "free it late".
5. **`collections-and-chains.md`** owns the multi-parameter closure gap §4.5
   identifies, because `fold` is its first customer. The proposed spelling is
   `(acc, x) giving acc + x` and the AST cost is `param: Option<Ident>` →
   `params: Vec<Ident>`, plus every closure snapshot.
6. **The core spec §4.3** should rename `const` to `constant`, or §4.1 should
   name `const` in writing as its one deliberate exception (§1.4). Either closes
   the hole; leaving it open is what makes the next `def` request hard to answer.
7. **`AGENTS.md`** — §12 below. Not edited here; another agent holds the file.

---

## 12. `AGENTS.md`: exactly what changes

Two edits, both additive. Neither touches the reserved-word list, because §3.4
and §4.4 reserve nothing.

**Edit 1 — §1, "The translation table".** One row, inserted directly below the
`fn f()` row so the two spellings of the same mistake are adjacent. The current
first row reads:

```
| `fn f()` | `function f()` | |
```

It becomes:

```
| `fn f()` | `function f()` | |
| `def f()` | `function f()` | Python's spelling; Science writes the word |
```

**Edit 2 — §4, the closures paragraph.** The current text is:

```
**Closures have two forms.** Use `each` when the parameter is mentioned once;
use `giving` when it is mentioned more than once, or when closures nest — a
nested `each` is an error.
```

It becomes:

```
**Closures have two forms, and there is no third.** Use `each` when the
parameter is mentioned once; use `giving` when it is mentioned more than once,
or when closures nest — a nested `each` is an error. There is no `lambda`:
`lambda x: x > 5` is written `x giving x > 5`, which is shorter, and
`each > 5`, which is shorter still.
```

and the sample block directly below it gains one line:

```science
docs.map(each.title)                        # once: use each
docs.map(doc giving doc.a + doc.b)          # twice: use giving
docs.sort(by: line giving line.length())    # named argument
xs.keep(each > 5)                           # not `lambda x: x > 5`
```

**Nothing else.** The reserved-word paragraph at lines 75–78 is unchanged — that
is the point of §9.2's contextual recognition, and it is worth a reviewer
checking that the paragraph was *not* touched, since a `lambda` added there would
quietly break §5.1's five fields.

A separate observation, outside this note's scope: `AGENTS.md` is still written
in revision-1 syntax throughout — `returns`, `for each x in xs`, `is at least`,
`try f()`, `Result of (Doc, Error)`. It sits outside `docs/`, so the migration of
the design notes to the revision-2 error model did not reach it and it remains
the largest known piece of revision-1 syntax in the repository. Whoever is
editing it now should carry it to revision 2 in the same pass; that is
`syntax-revision-2.md` §9's item 1, not this note's ask, and the two edits above
are written against the file as it stands so they apply either way.

---

## 13. Risks

**The `const` finding cuts both ways, and it is now in writing.** §1.3 hands the
author a live counterexample to the rule this note is defending. That is
deliberate — an argument that hides its best counterexample is worth less than
the counterexample — but it means the `def` decision is now one sentence away
from being reopened by anyone who reads §1.3 and skips §1.4. Item 6 of §11
closes it, and until it is closed this decision is softer than §3.4 makes it
sound.

**Rejecting both requests is a suspicious result.** A note asked to evaluate two
changes and rejecting both should be read with the question *did it just prefer
the status quo?* The honest check is what the note says yes to: §4.5's
multi-parameter `giving`, which is a real AST change with real snapshot churn;
§1.4's `const` → `constant`, which is a keyword rename this note was not asked
for; and `SC0136`, a fully specified diagnostic for the outcome where §3.4 is
overruled. None of those is a status-quo answer, and the second is a change the
author did not request and may not want.

**`SC0135` is recognising a shape, not a token, and shapes rot.** If `lambda`
ever becomes reserved for an unrelated reason, or if a future form makes
`ident ident :` legal in expression position, the recognition rule silently
changes meaning. It should carry a test per shape in `tests/ui/`, as
`llm-ergonomics.md` §6 requires of `SC0130` for the same reason, and the test for
`let lambda be 532<nm>` — the line that must *not* fire — matters more than the
tests for the lines that must.

**Models will keep writing both words indefinitely.** `llm-ergonomics.md` §6 is
right that migration diagnostics are permanent, not scaffolding, and this note
adds two more to a list that will never shrink. The mitigation is that both are
one-round-trip errors with applicable fixes, and the cost is that Science's error
surface now teaches Python users two things about Science before it teaches them
anything Science does.

**The strongest case against §4.4 is not in §4.3.** It is that `giving` is a word
no model has ever seen, and a language whose code is mostly machine-written is
betting that a diagnostic can teach a word faster than a corpus can. That bet is
`llm-ergonomics.md` §1's central bet and this note does not re-argue it — but
`giving` is the single place where the bet is largest, and if the bet fails
anywhere it will fail here first. The evidence to watch for is the `SC0135` rate:
if it does not fall over successive model generations, the premise is wrong and
§4.3 — replacing `giving`, not adding to it — is the decision to reopen, with a
word that is not `lambda`.
