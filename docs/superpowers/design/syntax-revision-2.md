# Science — Design: syntax revision 2

Date: 2026-09-16
Status: draft for review
Supersedes, in part: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
§4.3 (the keyword table), §4.5 (error propagation, loops), §4.6 (comparison
phrases), §5.4 (`trait`), and §8 (`println`).
Related: `llm-ergonomics.md`, most of whose §4 this revision makes unnecessary;
`reserved-words.md`, five of whose collisions this revision dissolves outright;
`python-interop.md` §6, which already decided the question §8 here asks.

---

## 0. The thesis this revision sharpens

The original redesign replaced symbols with English words wherever a word
existed. This revision corrects an overshoot. The goal was never *maximum*
English; it was:

> **Code that an AI writes and a human reads should say what it does.**

Those are different targets, and where they diverged the first pass chose
English and should have chosen familiarity. A reader who knows Python or Go
should feel they are reading a more modern version of what they know — not a
language that renamed everything to prove a point.

The rule this revision applies throughout:

> **Use a word where the operation has no universal symbol. Use the symbol
> where one exists and everybody already reads it.**

`a is b` has no symbol everyone agrees on — `==` is a programming convention,
not a mathematical one. `a >= b` has one, it is four hundred years old, and
every reader of a scientific paper knows it. That is the whole principle, and
seven of the eight changes below follow from it.

---

## 1. Comparisons: symbols for order, words for identity

**Decision.** Equality and inequality are `is` and `is not`. Ordering is
`>`, `<`, `>=`, `<=`. **There is exactly one spelling for each operation.**

| Operation | Science | Removed |
|---|---|---|
| equal | `a is b` | `a == b` |
| not equal | `a is not b` | `a != b` |
| greater | `a > b` | `a is above b` |
| less | `a < b` | `a is below b` |
| at least | `a >= b` | `a is at least b` |
| at most | `a <= b` | `a is at most b` |

### 1.1 Why this is the single best change in the revision

§4.6 of the core spec carried two complete spellings for six operators and
justified it as debt worth taking. `llm-ergonomics.md` §4.1 then found that the
debt could not be serviced: a formatter cannot canonicalise between `a == b` and
`a is b`, because the choice depends on the *types* of the operands —

```science
self.x == other.x        # F64, so symbols
self.text is other.text  # String, so words
```

— and a formatter that must run after type checking is not a formatter. The two
spellings were therefore permanently un-normalisable, which for a language whose
code is mostly machine-written means permanent drift.

This revision removes the problem by removing the duplication. There is nothing
to canonicalise, `sciencec fmt` needs no rule, and generated code cannot drift
because there is only one way to write each comparison.

### 1.2 What it frees

`at`, `above`, `below`, `most` and `least` stop being reserved words. That is
five entries off §13's list and it resolves, at no cost, three of the eleven
collisions `scientific-libraries.md` §3 had to work around:

- `poly.at(x)` — the natural name for polynomial evaluation, which that note had
  to rename to `evaluate`.
- `least_squares`, `at_most`, `at_least` as ordinary identifiers.
- The comparison-phrase machinery in the parser — the multi-token sequence
  assembly of §4.6 — is deleted rather than maintained.

### 1.3 The cost, stated

`if name is not "":` reads well. `if count >= limit:` reads well. The pairing is
slightly uneven in a mixed expression:

```science
if doc.title is not "" and doc.hits >= 10:
```

That is the price, it is small, and it is the same unevenness Python has with
`==` and `is`, which nobody trips over.

---

## 2. Loops: `for x in xs`, and `loop` with `break`

**Decision.** `each` is removed from the loop form. `while` is removed from the
language. `loop:` is the only unbounded loop and `break` is how it ends.

```science
for row in rows:
    print(row.name)

for i in 0..n:
    total be total + i

loop:
    let line, err be reader.next_line()
    if err?:
        break
    print(line)
```

### 2.1 `for each` → `for`

`for each row in rows` was the clearest case of English for its own sake. `for
row in rows` is what Python, JavaScript, Go, Swift and Kotlin all write, it is
not less readable, and the word `each` bought nothing.

`each` survives as the implicit closure subject, `docs.map(each.title)`, where it
does carry meaning: it names the subject of the enclosing call. That is its only
remaining use and it should be documented as such rather than as a loop word.

### 2.2 `while` → `loop` + `break`

Science now has two loops where it had three: the counting loop `for x in xs`,
and the unbounded `loop:`.

This is Go's decision, arrived at from the same direction. Go has exactly one
loop keyword and it is not a hardship. Here the split is cleaner still, because
ranges already cover the counting case that `while` is usually misused for:

```science
# before — while as a counting loop
let mutable i be 0
while i < n:
    step(i)
    i be i + 1

# after — the range says what it means
for i in 0..n:
    step(i)
```

**The cost, stated.** A genuine condition-driven loop is two lines longer:

```science
loop:
    if not converged(state):
        break
    state be refine(state)
```

That is real, and it is the known cost of the Go model. The alternative
considered and rejected was `loop while condition:`, which keeps one keyword but
reintroduces the second form this decision exists to remove. If the verbosity
proves annoying in practice, `loop while` is the obvious place to relent, and it
is source-compatible to add later.

---

## 3. Errors: Go's model, with a nullable type

**Decision.** A function that can fail returns its value *and* an error. The
error type is nullable. `?` tests for presence. `Result of (T, E)` and `try` are
removed; `Option of T` becomes `T?`.

```science
function read_config(path: String) -> (Config, Error?):
    let text, err be read_file(path)
    if err?:
        return (Config.empty(), err)

    let parsed, err be parse(text)
    if err?:
        return (Config.empty(), err)

    return (parsed, null)
```

### 3.1 The pieces

**Nullable types.** `T?` is `T` or `null`. It replaces `Option of T`, and it is
the spelling Kotlin, Swift, TypeScript and C# all use, so it needs no teaching.

```science
let found: Doc? be lookup(id)
let missing: Error? be null
```

**Multiple returns.** `-> (Config, Error?)` returns a pair. Destructuring at the
binding is how it is received:

```science
let config, err be read_config(path)
```

**`?` is the presence test.** `err?` is a `Bool`, true when the value is not
null. It is a postfix operator and it reads as the question it is:

```science
if err?:
    print("could not read the config")
```

`?` was free: the original design removed it as the error-propagation operator,
so the character is available and it now means exactly one thing.

**Narrowing.** Inside `if err?:` the compiler knows `err` is not null, and after
an `if err?: return …` the *value* is known good. This is flow-sensitive typing,
it is what makes the model bearable rather than cast-ridden, and it is the one
piece of real type-checker work this decision requires. TypeScript and Kotlin
both do it; the rules are well understood.

### 3.2 Why this over `Result` and `try`

The honest answer is that it is a taste decision and both models work. What can
be said for this one:

- **It is readable in sequence.** The Go model's virtue is that the error path is
  written where it happens, in the order it happens. For code that is mostly
  read rather than written — which is the premise of §0 — that matters more than
  the brevity `try` bought.
- **It removes a concept.** `Result of (T, E)`, `Option of T`, `try`, and `From`
  widening were four interacting things. `T?`, multiple returns and `?` are
  three, and two of them are already known to anyone who has written TypeScript.
- **`try`'s widening was never specified.** `examples/README.md` records it as a
  standing assumption: §4.5 said `try` "unwraps `Ok`/`Some` or returns the
  failure" and never said the failure was converted, yet two corpus files
  depended on the conversion. That hole closes by construction here — the caller
  writes the conversion, because the caller writes the return.

### 3.3 The cost, stated plainly

**Verbosity.** This is Go's well-known tax and it is real: three lines per
fallible call instead of one. The corpus file `09_option_result.science` roughly
doubles in length.

**No enforcement that the error is checked.** `Result` made ignoring a failure a
type error. A returned `Error?` can be dropped on the floor. Go has this problem
and pays for it. **Mitigation:** make an unchecked error a *compile* error rather
than a lint — if a binding of a nullable error type is never tested with `?`
before the function returns, that is `SC0140`. This is a use-before-check
analysis, it is cheap, and it recovers most of what `Result` was buying. It
should ship with the feature, not after it.

**Interaction with `python-interop.md`.** §6.3 of that note writes every Python
call as `Result of (PyObject, PyError)` and uses `try` for the ergonomics. That
section needs rewriting against this model; the mapping is mechanical
(`-> (PyObject, PyError?)`) and the note's substance does not change.

### 3.4 What `Error` is

§3.1 wrote `-> (Config, Error?)` without saying what `Error` names, and §3.2
removed `From` widening. Together those leave a hole, and it is the hole Go
fills with its one-method interface.

**Decision.** `Error` is an **interface**, in the sense of §6, with one required
method:

```science
interface Error:
    function message(self) -> String
```

`Error?` in a return position is shorthand for `(any Error)?` — an optional
trait object. The shorthand exists because the long form appears in the
signature of every fallible function in the language and `-> (Config, (any
Error)?)` is not a signature anyone should have to read.

**A function may instead name a concrete error type**, and should when the
caller is expected to distinguish cases:

```science
choice ConfigError:
    Missing(String)
    Malformed(String, Int)

function read_config(path: String) -> (Config, ConfigError?):
    ...
```

The concrete form is the one that keeps `match` exhaustive, which is what §1 of
the core spec is about; the interface form is the one that composes across
library boundaries. Both are legal, and the choice is the author's.

**Conversion is explicit, and that is the point.** With `From` widening gone, a
function whose callee fails with `ConfigError` and whose own signature says
`Error?` converts at the return:

```science
let config, err be read_config(path)
if err?:
    return (Doc.empty(), StartupError.from_config(err))
```

§3.2 claimed this as a virtue — *the caller writes the conversion, because the
caller writes the return* — and this is where that claim is paid for. It is more
typing than `try`'s invisible `From` call, and it is the reason the invisible
call was never specified in the first place: `examples/README.md` recorded the
widening as a standing assumption because §4.5 described `try` without ever
saying the failure was converted.

**Cost.** `any Error` is a boxed trait object, so the interface form allocates
where the concrete form does not. For an error path that is acceptable; for a
function called in a tight loop that returns `Error?` on every iteration it is
not, and the concrete form is the answer. This is worth a lint rather than a
rule.

### 3.5 `print`

`println` becomes `print`, with Python's semantics: it writes its arguments and a
newline. The no-newline form is `write`.

§8's free-function list becomes `print`, `write`, `panic`, `read_file`,
`write_file`.

---

## 4. `->` returns

**Decision.** `returns` becomes `->`.

```science
function summarize(self) -> String:
    self.body.truncate(200)
```

This reverses one of the original redesign's changes, and by §0's rule it is
right to reverse: `->` is not an abbreviation of an English word, it is a symbol
that every reader of Rust, Go, Swift, Kotlin, TypeScript and Python type hints
already reads as "returns". It is also shorter in the place where signatures get
longest.

Consequence: `Arrow` returns to the token set, and the lexer's longest-match
rule gains back `->` before `-`. That code was deleted today and the deletion is
reverted.

---

## 5. `has`, not `has methods`

**Decision.** The inherent-methods block is `Type has:`.

```science
Doc has:
    function new(title: String) -> Doc:
        Doc(title: title, body: "")
```

`has methods` was two words where one carried the meaning. `methods` stops being
a keyword, which frees it as an identifier — a `methods` field or variable is
not exotic.

---

## 6. `trait` → `interface`

**Decision.** `interface`, not `class`.

```science
interface Summarize:
    function summarize(self) -> String

    function preview(self) -> String:
        self.summarize().truncate(80)

Doc implements Summarize:
    function summarize(self) -> String:
        self.body.truncate(200)
```

### 6.1 Why not `class`

`class` is the familiar word, and it is the wrong one, in a way that would cost
more than `trait` ever did.

In every language that has `class` — Python, Java, C#, C++, TypeScript, Ruby —
a class is **a type with data and methods**, usually with inheritance. In
Science, the type with data is `type`, and the methods attach to it with `has`.
Calling the *interface* a class would mean that the one word everybody already
knows means something different here than everywhere else. That is worse than an
unfamiliar word: an unfamiliar word makes a reader look it up, a familiar word
used differently makes a reader confidently wrong.

### 6.2 Why `interface`

- It means exactly this in Java, C#, TypeScript, Go and PHP.
- Go's interfaces are the closest analogue in any mainstream language to what
  Science's traits are, and Go is the language this revision borrows its error
  model from.
- `Doc implements Summarize` was already the Java/C# phrasing. `interface` +
  `implements` is one coherent vocabulary instead of Rust's `trait` bolted to
  Java's `implements`.

The alternative worth naming is `protocol`, which Swift uses and which Python
has as `typing.Protocol`. It is correct and it is less widely known.
`interface` wins on recognition.

---

## 7. What this revision does to the rest of the language

A summary of the knock-on effects, so nothing is discovered later.

| Change | Consequence |
|---|---|
| Comparison phrases removed | `at above below most least` leave §13. Parser loses the phrase-assembly code. `llm-ergonomics.md` §4.1 becomes moot. |
| `while` removed | `while` leaves §13. One less loop form in the grammar. |
| `for each` → `for` | `each` remains reserved, for closures only. |
| `has methods` → `has` | `methods` leaves §13. |
| `trait` → `interface` | `trait` leaves §13, `interface` joins it. |
| `returns` → `->` | `returns` leaves §13. `Arrow` returns to the lexer. |
| `Result`/`try` removed | `try` leaves §13. `?` becomes the presence operator. `Option` becomes `T?`. |
| `println` → `print` | §8's list changes. |

**Net effect on the reserved-word list: nine words removed** — `at`, `above`,
`below`, `most`, `least`, `while`, `methods`, `returns`, `try` — against **two**
added, `interface` and `null`. Combined with `reserved-words.md`'s eight, the
list shrinks by a third, and the scientific-vocabulary collisions of that note's
§2 mostly stop existing.

`null` was missed in this note's first draft, which introduced the literal in
§3.1 and never added it to §13 or to §4.2's literal inventory. It belongs in
both, for the same reason `true` and `false` do: its spelling is fixed, so a
program may not bind the name.

### 7.1 A program in the revised syntax

```science
type Doc:
    title: String
    body: String

interface Summarize:
    function summarize(self) -> String

    function preview(self) -> String:
        self.summarize().truncate(80)

Doc implements Summarize:
    function summarize(self) -> String:
        self.body.truncate(200)

Doc has:
    function new(title: String) -> Doc:
        Doc(title: title, body: "")

function longest of T(items: borrowed Array of T) -> borrowed T where T: Ord:
    let mutable best be items.get(0)
    for item in items:
        if item > best:
            best be item
    best

function load(path: String) -> (Doc, Error?):
    let text, err be read_file(path)
    if err?:
        return (Doc.new(""), err)
    return (Doc(title: "loaded", body: text), null)

function main():
    let doc, err be load("a.txt")
    if err?:
        print("could not load")
        return

    print(doc.preview())

    for i in 0..5:
        print(i)

    let headlines be docs
        .iterate()
        .discard(each.is_empty())
        .map(each.title)
        .collect()
```

---

## 8. Inline foreign code: `python { … }` and `rust { … }`

The question asked was how bad the `asm`-block analogy is. The answer has three
parts, and they land differently for the two languages.

### 8.1 Why `asm` works and this is not the same shape

Inline assembly is cheap for reasons that are specific to assembly and none of
which transfer:

- Assembly is **the compiler's own output**. There is no second toolchain,
  because the compiler already emits assembly; the block is pasted into a stream
  it was going to produce anyway.
- There is **no runtime**. No interpreter, no garbage collector, no allocator to
  reconcile.
- There is **no data marshalling**. Values are already in registers under a
  calling convention the compiler chose.
- There is **one memory model**, and the language is already trusting the author
  with it.

Rust shares the first three of those partially and Python shares none of them.
So the analogy is doing more work than it can carry, and each language has to be
judged separately.

### 8.2 `rust { … }` — feasible, but it is a build-system feature

Rust is the good case. It compiles ahead of time to the same native target, has
a C ABI, and has no runtime to embed. Mechanically, `rust { … }` is: extract the
fragment, write it to a file, invoke `rustc`, link the object. That genuinely
works.

The costs are real but they are not fatal:

- **Every Science build needs a Rust toolchain.** The *output* is still a
  self-contained binary, so §3's core decision survives. The *build* stops being
  self-contained, which is a smaller promise but a promise.
- **Two borrow checkers meet at the block boundary**, and something must
  reconcile them. This is exactly the problem `ffi-c-boundary.md` §2 solves — its
  four ownership cases, `ffi.Span`, `from p` on returned borrows — and an inline
  block does not make that problem easier, it just hides where it is.
- **Errors come from another compiler.** `rustc`'s diagnostics would have to be
  remapped into Science diagnostics with correct spans, or §10.3's byte-exact UI
  tests stop being a coherent discipline for any file containing a block.
- **Dependencies.** A fragment that wants `serde` needs Cargo, which means
  Science needs a package manager to express that, which it does not have.

### 8.3 `python { … }` — this one is already decided, and the answer was no

`python-interop.md` §6.2 ruled on exactly this, and the ruling is approved:

> **Yes in hosted mode, no in standalone mode, in the first version.**
> [...] **Standalone**: an AOT binary that starts an interpreter of its own.
> Rejected for the first version (`SC0455` if a `use python` appears in a
> standalone build).

An inline `python { }` inside a `.science` file compiled to a standalone binary
*is* the standalone case, and the reasons given there still hold in full: the
binary is no longer self-contained; it needs an interpreter of a compatible
version with the right packages findable at runtime; and "the deployment story
becomes Python's deployment story, which is the thing users are running toward
Science to escape."

So `python { }` does not need a fresh decision. It needs to be recognised as the
case that was already rejected, wearing different syntax.

### 8.4 The problem both share, which is about tooling

Independent of runtimes, putting foreign source *inside* a `.science` file
inverts a dependency that should point the other way.

With a declaration — `extern`, `use python` — the foreign code lives in its own
ecosystem: its own file, its own build, its own tests, its own formatter, its
own package manager. Science declares what it needs and links.

With an inline block, foreign source lives where:

- no Rust tool can see it — not `rustfmt`, not `clippy`, not `rust-analyzer`;
- no Python tool can see it — not `black`, not `mypy`, not `pytest`;
- Science's own formatter and language server must either understand three
  languages or give up inside those regions;
- and a syntax error in the fragment surfaces as a Science diagnostic that
  Science did not produce.

For a project whose spec has a section on diagnostics and whose test suite
compares rendered diagnostics byte for byte, that last point is the one that
should decide it.

### 8.5 Where I land

**The instinct is right and the project already acted on it.** Reusing
ecosystems instead of reimplementing them is `ffi-c-boundary.md` §0's opening
argument — *a language that cannot call these is not a scientific language*. The
disagreement is not about whether to reach into Rust and Python. It is only about
**where the foreign source lives**.

The recommendation is a middle path that gets the ergonomics without the holes:

1. **Keep declarations as the interface.** `extern` and `use python` stay exactly
   as their notes specify.
2. **Add a sidecar directory.** `foreign/*.rs` and `foreign/*.py` are real files
   that `sciencec` builds and links, bound to Science names by a manifest. Real
   files mean real tooling — rustfmt, clippy, mypy and pytest all work, because
   nothing is hiding inside a string.
3. **If an inline block is still wanted, make it pure sugar for step 2.** The
   compiler extracts the block into a generated file under `foreign/`, compiles
   it exactly as though the user had written the file, and links it. Then:
   - the inline form introduces **no new semantics** — anything it can express,
     the sidecar can express, and vice versa;
   - diagnostics have a real file and real spans to point at;
   - a fragment can be "promoted" to a real file by moving it, with no change in
     meaning, the day it outgrows being inline.

That ordering matters. Sugar over a mechanism that works is cheap and reversible.
A mechanism whose only form is inline is neither.

**And for Python specifically, the sugar only applies in hosted mode**, because
§6.2 already decided the standalone case, and a syntax change does not reopen a
deployment decision.

---

## 9. What this revision asks for

1. **A spec revision to §4.3, §4.5, §4.6, §5.4 and §8**, carrying the seven
   syntax decisions above. §4.6's comparison-phrase section is deleted rather
   than amended.
2. **Flow-sensitive narrowing for nullable types** (§3.1). The one real
   type-checker feature this revision needs.
3. **`SC0140`, an unchecked-error diagnostic** (§3.3). It should ship with the
   error model, not after it, because it is what keeps the Go model honest.
4. **A rewrite of `python-interop.md` §6.3** against the new error model, and a
   paragraph in that note pointing at §8.3 here, so the inline-block question is
   answered where people will look for it.
5. **A decision on `loop while`** (§2.2) — accepted now, or held until the
   verbosity is felt. This note recommends holding.

---

## 10. Risks

**This is the second syntax revision in one day.** The corpus was migrated to the
first one this morning; `examples/`, `tests/ui/`, the lexer's 117 tests and the
parser's 157 all encode it. Every one of those moves again. That cost is
accepted here on the grounds that the language has no users yet and this is the
cheapest day it will ever be — but it is the last day that argument is free, and
a third revision should be held to a much higher bar.

**The error model is the one change that is not merely syntactic.** Points 1, 2,
4, 5 and 6 are renames a mechanical migration can perform. Point 3 changes what
programs *mean*: every fallible function's signature, every call site, and the
type of every optional value. It should be staged separately from the renames,
and it should land with `SC0140` and with narrowing, or it lands worse than what
it replaces.

**Go's error model is the most-criticised thing about Go.** It is chosen here
deliberately and for a stated reason — readability in sequence, for code that is
mostly read — but the criticism is not baseless, and §3.3's mitigation is what
makes the choice defensible rather than merely fashionable.
