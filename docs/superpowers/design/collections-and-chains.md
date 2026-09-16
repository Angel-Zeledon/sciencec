# Science — Collections, Iteration, and the Chain API

Date: 2026-09-16
Status: draft for F0
Companion to: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` —
§4.6 (chains, closures, comparison phrases), §5.4 (`Iterate`), §6 (ownership),
§8 (closed method sets).

---

## 0. The premise this note works from

A pipeline is mostly glue. Load, reshape, filter, batch, hand to a model, collect,
write. The arithmetic inside is somebody else's library; the part the user writes
is the part *between* the libraries, and in Science that part is a chain of method
calls (§4.6). The chain vocabulary is therefore not a convenience layer over the
type system — for the working scientist it **is** the language. Two consequences
run through every decision below.

1. **The set is small and closed** (§8). Every method costs a name the user must
   learn, a page in the reference, a shape in the diagnostics, and an obligation
   to have a parallel version in F2. A combinator earns its place by being
   something a working pipeline writes *often*, not by being expressible.
2. **Nothing in a chain is allowed to guess.** §4.6 rejected bare `doc.summary`
   because a language cannot resolve an ambiguity by guessing. The same rule kills
   inference-directed `collect`, kills an `iterate()` that silently picks between
   borrowing and consuming, and kills a `.parallel()` that quietly changes what a
   reduction means.

Everything is written in Science syntax. Where a decision needs something the F0
spec has not settled, it is marked **AMENDMENT n** and collected in §10.

---

## 1. The `Iterate` trait and the chain vocabulary

### 1.1 The trait a user implements

§5.4 fixes the core. This note adds one defaulted method:

```science
type Bounds:
    minimum: Int
    maximum: Int?

interface Iterate:
    type Item

    def next(mutable self) -> Self.Item?

    def estimated_length(borrowed self) -> Bounds:
        Bounds(minimum: 0, maximum: null)
```

One required method. `estimated_length` has a default so that implementing a new
source stays a one-method job, and it exists now rather than later because
`collect()` must preallocate and because F2's `.parallel()` needs a length to
decide how to split (§7). Its fields are `minimum` and `maximum` rather than
`least` and `most`, because §13 reserves those for the comparison phrases.

Everything else in this note is a **provided method** on `Iterate`. Users
implement `next`; they never implement a combinator. That single rule is what
lets F2 hand the identical vocabulary to a parallel trait without touching a line
of user code (§7).

### 1.2 The type of a closure argument

Combinators take closures, and §5.2 requires signatures to be fully annotated, so
a closure needs a nameable type. **This note now owns that spelling** — it was a
standing cross-note ask in `README.md` with three customers and no owner, and the
three customers are `ffi-c-boundary.md` §10.1, `scientific-libraries.md` §14.2
and `broadcasting.md` §11.6.

> **Decision. A closure type is `(A) -> B`. There is no keyword.**

```science
def keep of P(self, predicate: P) -> Keep of (Self, P)
        where P: (borrowed Self.Item) -> Bool
```

**AMENDMENT 1.**

#### What this replaces, and why the old argument could not be carried over

This section used to read `function(A) returns B`, and justified it in one
sentence: *a type, formed exactly as a declaration is. No new keyword.* That
sentence was doing two jobs. It meant **no new keyword**, and it meant the type
**read as English** — `f: function(F64) returns F64` is a noun naming what the
parameter is.

Two revisions since took both halves apart. `syntax-revision-2.md` §4 removed
`returns` for `->`, so the spelling used a keyword the compiler rejects
(`SC0118`). `syntax-revision-3.md` renamed the declaration to `def`, and a
mechanical carry-over gives `def(A) -> B`, where the second job is simply gone:
`f: def(F64) -> F64` is not a noun naming anything, it is a declaration that lost
its name. `syntax-revision-3.md` §7 item 6 flagged it from the other side and
declined to decide it.

So the question had to be re-opened rather than inherited, and the answer is that
**the first job never needed a keyword to do it**. `(A) -> B` reads as *takes an
A, gives a B*. It is what Kotlin and Swift write exactly, what TypeScript writes
with `=>`, and what the whole ML family writes with the arrow alone. It sidesteps
the noun-versus-verb problem by having no word to be the wrong part of speech.

#### It does not test the keyword rule, which is the strongest thing about it

`syntax-revision-3.md` §2 left §4.1 weaker than it found it: a whole English word
by default, a shortening permitted only if it beats the **audience test**, and a
shortening adopted without beating it named in a register. That rule is now the
language's only instrument for refusing a keyword, and it has already been
overruled once.

`(A) -> B` **does not put a single new word in front of it.** The only symbol it
uses is `->`, which revision 2 §4 already admitted on the ground that every
reader of Rust, Go, Swift, Kotlin, TypeScript and Python type hints reads it as
"returns". This spelling adds nothing to §13's list, spends none of §4.1's
remaining authority, and asks nobody to re-litigate anything.

The rejected alternatives, and what they would have cost:

- **`def(A) -> B`** — the mechanical result, and the cheapest edit. Rejected on
  reading, for the reason above, and on a second ground the mechanical sweep made
  visible: `def(` is already spoken for in *expression* position in these notes.
  `intrinsics-math-physics.md` writes `let spectral be def(wavelength: F64) ->
  F64:` five times. That form is not a form the language has —
  `def-and-lambda.md` §4 keeps `each` and `giving` and rejects a third closure
  form, and `sciencec` answers it with `SC0105` — but it is what people reach
  for, and giving `def(…)` a second job in type position means the two categories
  are told apart only by whether the parameters are named and whether a `:`
  follows. A keyword that means *here comes code* should not also mean *here
  comes a type that is not code*.
- **`fn(A) -> B`** — refused by the audience test, applied rather than asserted.
  Count `fn` across the languages this audience writes: Fortran `FUNCTION`,
  MATLAB `function`, R `function`, Julia `function`, Python `def`, JavaScript
  `function`, PHP `fn`. **One of seven**, and the one is PHP's arrow function.
  That is the same margin on which `def-and-lambda.md` §3.2 refused `def`, and
  `def` was adopted only by the owner overruling it. Nobody has asked for `fn`.
  §1.5 of that note already names `fn` in the list the audience test refuses in
  one sentence each; adopting it here would mean contradicting a refusal written
  the same day, with no new evidence.

#### The ambiguity with the tuple type, which is the real risk

`(A, B)` is already a tuple, and `-> (Config, Error?)` is the error model's
return shape on every fallible function in the language. A closure type that
opens with `(` therefore starts identically to the commonest type in the
language. Worked against the parser as it stands, in
`crates/science-parser/src/parser.rs`:

**It is one token of lookahead, after the closing paren, with no backtracking.**
`parse_type_atom` already dispatches `LParen` to `parse_paren_type`, which parses
`()`, `(T)` and `(A, B)` to completion. The parameter list of a closure type and
the element list of a tuple have **identical inner grammar** — a comma-separated
list of types — so the same parse serves both readings and only the *reduction*
differs. After `parse_paren_type` returns, `parse_type` peeks once: `Arrow` means
it was a parameter list, anything else means it was what it is today.

`-> (Config, Error?)` is decided by that one peek: the next token is `:` or
`where`, never `->`, so it stays a tuple. No existing signature changes meaning.

**`-> (A) -> B` in return position is not ambiguous, but it needs a stated
associativity.** Nothing in the grammar may follow a return type except `where`
or `:`, so the second `->` cannot begin anything else and the greedy reading is
the only reading. `->` is **right-associative**: `(A) -> (B) -> C` is
`(A) -> ((B) -> C)`, which is currying and the only useful reading.

**`?` binds tighter than `->`.** `(A) -> B?` is `(A) -> (B?)`, because the
right-hand side is a recursive `parse_type` call and the `?` loop runs inside it.
A nullable closure needs explicit parentheses: `((A) -> B)?`.
`scientific-libraries.md` §8.6 writes exactly that for `lbfgs`'s optional
gradient, and it is the same rule Kotlin and TypeScript have.

**The one real loss, stated.** `(T)` collapses to `T` with no node of its own —
deliberately, so that parentheses only group — so a **one-parameter closure whose
parameter is a tuple cannot be spelled**: `((A, B)) -> C` parses as `(A, B) -> C`.
This is a genuine hole and it is already closed by another rule: §1.3 below rules
that pairs are records and never tuples, so the only tuples in F0 are the error
model's return shape, and a closure over one is written with the two-parameter
form that AMENDMENT 3 produces anyway. If F0 ever grows a reason to pass a tuple
as one argument, the answer is a named record, not a third paren.

#### The grammar change, concretely

Four edits, and only the third is work:

1. **`parse_type`** — after the `?`-suffix loop, if the next token is `Arrow`,
   consume it, parse the return type with a recursive `parse_type` (which gives
   right-associativity for free), and rebuild the type just parsed as the
   parameter list. The mapping is total and lossless: `Unit` → no parameters,
   `Tuple(elems)` → `elems`, anything else → one parameter.
2. **`TypeKind`** — one new variant,
   `Closure { params: Vec<Type>, ret: Box<Type> }`.
3. **`parse_type_bound` must stop being a path.** This is the part the ask has
   never been priced at. A bound is `parse_path()` today and a `TypeBound` holds
   a `Path`, so `where F: (A) -> B` does not parse *at all* — and would not have
   parsed under `def(A) -> B` either. Every `where` clause in §1.4 below depends
   on this. It is the same edit under every candidate spelling and it is the real
   cost of the ask.
4. **The lexer** — nothing. `Arrow` came back with revision 2 §4.

Verified against the compiler as it stands: `f: (F64) -> F64` is today two
`SC0100`s and `f: def(F64) -> F64` is three errors, so neither candidate is
implemented and neither is cheaper to reach.

**No new diagnostic code is allocated here.** If the parser should say something
better than `SC0100` when a tuple is followed by `->` before this ships, that is
one code from the syntax block and it is recorded as a need, not taken.

#### What it costs, and what reversing it costs

**The cost is the missing anchor.** `f: (F64) -> F64` is four punctuation marks
and two type names, with no word for the eye to land on. `def(F64) -> F64` has a
visible marker saying *function type here*. That is the whole case for the
keyword form and it is not nothing — it is worth more in a dense `where` clause
than in a parameter list.

**Reversing it is close to free, and that is why the better-reading option wins
over the cheaper-to-type one.** The decision is a *pretty-printer* choice, not a
structural one: all three candidates produce the identical `Closure` node, so a
reversal is one line in `parse_type` to require a leading keyword and one line in
`sciencec fmt`'s type printer to emit it, plus a re-format of the corpus. What
reversal is *not* is a regex: a text tool cannot find a closure type, because
finding one is exactly the one-token-after-the-paren test only a parser can make.
So reversal is cheap **through the formatter** and expensive through anything
else — which is `self-hosting.md` gate E1 doing the job Decision 6 claimed it
would, for the second time.

Closures of more than one parameter (`reduce`, `accumulate`) need a spelling too.
The named form takes a parenthesised list, and `each` — being the name of *the*
subject — is defined only for arity one:

```science
totals.iterate().reduce(0.0, (running, row) giving running + row.weight)
```

**AMENDMENT 3**: `(a, b) giving expression`, and `each` inside a call whose
closure takes more than one parameter is an error naming the named form as the
fix, exactly as `SC0115` does for nesting.

### 1.3 Pairs are records, never tuples

Several combinators produce two things per step. Rust yields tuples and the user
writes `.0` and `.1`. Science yields **named records**, because `each.key` works
with the implicit-subject closure of §4.6 and `each.0` does not — and because F0
has never defined a tuple-field syntax anyway.

```science
type Entry of (K, V):          # what Map.iterate() yields
    key: K
    value: V

type Numbered of T:            # what numbered() yields
    index: Int
    item: T

type Pair of (A, B):           # what zip() yields
    left: A
    right: B

type Parts of T:               # what partition(p) returns
    kept: Array of T
    discarded: Array of T

type Outcome of (T, E):        # what partition_results() returns
    values: Array of T
    errors: Array of E
```

`.numbered().discard(each.index is below 100).map(each.item)` is the payoff, and
`parts.kept` beats `parts.0` in every program that ever reads it. This is the
highest-value ergonomic decision in the note: it keeps `each` usable across the
whole vocabulary instead of across only the unary half of it.

### 1.4 The closed set

Lazy adapters return a named adapter type and do no work. Terminals run the chain.
The **Parallel** column is the F2 commitment recorded now (§7): *safe* means the
parallel form computes an identical value, *ordered* means it needs an
order-preserving merge, *sequential* means `.parallel()` must reject it, *barrier*
means it buffers the whole stream.

#### Transforming (lazy)

| Method | Signature (abbreviated) | Parallel | Why it earns its place |
|---|---|---|---|
| `map(f)` | `map of (U, F)(self, f: F) -> MapOver of (Self, F)` where `F: (Self.Item) -> U` | safe | The one combinator no pipeline omits. Name kept: §3.2. |
| `expand(f)` | `expand of (S, F)(self, f: F) -> Expand of (Self, F)` where `F: (Self.Item) -> S, S: Iterate` | safe | One item becomes many: a document becomes its tokens, a batch becomes its rows. Fused rather than `map` then `flatten` because the intermediate is never wanted. |
| `flatten()` | `flatten(self) -> Flatten of Self` where `Self.Item: Iterate` | safe | The unfused case, when the nesting arrived from somewhere else. |
| `owned()` | `owned(self) -> Owned of Self` where `Self.Item` is `borrowed T, T: Clone` | safe | Turns a chain of borrows into a chain of values. Replaces Rust's `cloned` *and* `copied` (§4.4). |
| `numbered()` | `numbered(self) -> NumberedOver of Self`, `Item = Numbered of Self.Item` | ordered | Row numbers, sample ids, progress reporting. |
| `accumulate(initial, f)` | `accumulate of (A, F)(self, initial: A, f: F) -> Accumulate of (Self, A, F)` where `F: (A, Self.Item) -> A` | sequential | Running totals, cumulative sums, moving averages, online state. `reduce` throws the intermediates away; time-series work wants them. |

#### Filtering and selecting (lazy)

| Method | Signature | Parallel | Why |
|---|---|---|---|
| `keep(p)` | `keep of P(self, p: P) -> Keep of (Self, P)` | safe | Retain what matches. |
| `discard(p)` | `discard of P(self, p: P) -> Discard of (Self, P)` | safe | Drop what matches. Both exist because a negated predicate is the commonest readability wart in filtering code, and §4.6's own example is `discard(each.is_empty())`. |
| `keep_some()` | `keep_some(self) -> KeepSome of Self` where `Self.Item` is `T?` | safe | `map` then `keep_some` is `filter_map` in two honest words, and removes a combinator from the set. |
| `keep_ok()` | `keep_ok(self) -> KeepOk of Self` where `Self.Item` is `(T, E?)` | safe | The "drop the bad rows" error policy, named at the call site (§6). |
| `take(n)` | `take(self, n: Int) -> Take of Self` | ordered | §4.6 uses it. Head of the stream, and the reason laziness pays. |
| `skip(n)` | `skip(self, n: Int) -> Skip of Self` | ordered | Header lines, warm-up steps, burn-in samples. |
| `take_while(p)` | `take_while of P(self, p: P) -> TakeWhile of (Self, P)` | sequential | Read until a sentinel; stop a sweep when the loss stops falling. |
| `skip_while(p)` | `skip_while of P(self, p: P) -> SkipWhile of (Self, P)` | sequential | The complement, and the only clean way past a variable-length preamble. |
| `every(n)` | `every(self, n: Int) -> Every of Self` | ordered | Downsampling: every tenth frame, every hundredth step. This is `step_by`, named for what the user is doing with it. |
| `unique()` | `unique(self) -> Unique of Self` where `Self.Item: Eq + Hash` | sequential | Deduplication is glue work every dataset needs and nobody wants to write twice. |
| `unique(by: key)` | `unique of (K, F)(self, by: F) -> UniqueBy of (Self, F)` | sequential | Dedup by id while keeping the whole record. The bare form is the degenerate case of this one. |

#### Pairing, grouping, windowing

| Method | Signature | Parallel | Why |
|---|---|---|---|
| `zip(other)` | `zip of O(self, other: O) -> Zip of (Self, O)` where `O: Iterate`; `Item = Pair of (Self.Item, O.Item)` | ordered | Features beside labels, predictions beside truth. Not optional in ML glue. |
| `followed_by(other)` | `followed_by of O(self, other: O) -> Then of (Self, O)` where `O: Iterate of Item = Self.Item` | ordered | Train then validation; this shard then that one. Cheap to specify, irritating to live without. |
| `batches(n)` | `batches(self, n: Int) -> Batches of Self`; `Item = Array of Self.Item` | ordered | Minibatching. The most domain-specific entry in the set, and the one that most justifies a closed set containing it rather than a library. Disjoint pieces. |
| `windows(n)` | `windows(self, n: Int) -> Windows of Self`; `Item = Array of Self.Item` | ordered | Rolling means, lags, spectrogram frames. Overlapping, and separate from `batches` for exactly that reason. |
| `sorted()` | `sorted(self) -> Sorted of Self` where `Self.Item: Ord` | barrier | Buffers, sorts, yields. A barrier, documented as one. |
| `sorted(by: key)` | `sorted of (K, F)(self, by: F) -> SortedBy of (Self, F)` where `K: Ord` | barrier | Top-k retrieval is `.sorted(by: each.score).reverse().take(10)`. §4.6 already writes `sort(by:)`. |
| `reverse()` | `reverse(self) -> Reverse of Self` | barrier | Also a buffering barrier. F0 has no double-ended iteration trait and will not grow one: one trait, one direction, and an honest `O(n)` buffer for the one chain per program that runs backwards. |

#### Terminals

| Method | Signature | Parallel | Why |
|---|---|---|---|
| `collect()` | `collect(self) -> Array of Self.Item` | ordered | Always an `Array`. Never inference-directed (§3.3). |
| `collect_or_error()` | `collect_or_error(self) -> (Array of T, E?)` where `Self.Item` is `(T, E?)` | ordered | The "stop at the first bad row" error policy (§6). |
| `partition_results()` | `partition_results(self) -> Outcome of (T, E)` | ordered | The "give me both" policy (§6), and the one a scientist actually wants. |
| `partition(p)` | `partition of P(self, p: P) -> Parts of Self.Item` | ordered | One pass over a source you cannot rewind. That, and only that, is why it exists beside `keep`/`discard`. |
| `reduce(initial, f)` | `reduce of (A, F)(self, initial: A, f: F) -> A` | ordered | The general fold. One form, always with an initial value — the initial-less variant returns a nullable nobody handles. |
| `sum()` | `sum(self) -> Total` where `Self.Item: Add of Output = Total` | ordered | Named separately from `reduce` because summation order is a published-results question (§5.1) and the library owes it a defined answer. |
| `product()` | `product(self) -> Total` where `Self.Item: Mul of Output = Total` | ordered | Same argument; likelihoods and volumes. |
| `count()` | `count(self) -> Int` | safe | Deliberately not called `length`: `length()` on a collection is `O(1)`, counting a stream consumes it, and two names keep the difference visible. |
| `minimum()` / `maximum()` | `-> Self.Item?` where `Self.Item: Ord` | ordered | Full words, per §4.3's rule against abbreviations. |
| `minimum(by: key)` / `maximum(by: key)` | `of (K, F)(self, by: F) -> Self.Item?` where `K: Ord` | ordered | The best *record*, not the best score. |
| `first()` | `first(mutable self) -> Self.Item?` | sequential | Takes `mutable self`, so a chain can be stepped and then resumed. |
| `find(p)` | `find of P(mutable self, p: P) -> Self.Item?` | sequential | First match, short-circuiting, chain still usable afterwards. |
| `last()` | `last(self) -> Self.Item?` | ordered | The final state of a run. Walks the whole chain, and says so. |
| `has_any(p)` / `has_all(p)` | `of P(mutable self, p: P) -> Bool` | safe | Validation predicates. `has_` because `any` is reserved (§4.3, `any Summarize`), and because `if rows.iterate().has_any(each.is_missing())` reads as a sentence. |
| `group(by: key)` | `group of (K, F)(self, by: F) -> Map of (K, Array of Self.Item)` where `K: Eq + Hash` | ordered | The most frequently rewritten loop in data work. Without it every user writes the same six lines and half of them get the insert-or-append wrong. |
| `tally(by: key)` | `tally of (K, F)(self, by: F) -> Map of (K, Int)` where `K: Eq + Hash` | ordered | Class balance, label counts, bin counts. `group(by:)` then `count()` allocates every group only to discard it. |

Thirty-eight entries. Rust's `Iterator` has upward of seventy.

### 1.5 What is deliberately left out

- **`for_each(f)`.** The language already has `for each x in xs:` (§4.3). A
  side-effecting terminal is a second spelling of an existing statement — and §1
  names an effect discipline as part of what Science is betting on. Standardising
  a side-effect combinator *before* the effect system arrives is the wrong order.
- **`inspect` / `peek` / `tap`.** The same argument, and it is the tempting one,
  because debugging a lazy chain is genuinely harder (§2.2). The answer is a
  debugger and a `for each` loop, not an effectful link in a chain that F2 wants
  to prove pure enough to parallelise.
- **`filter_map`.** `map` then `keep_some`.
- **`fold` beside `reduce`.** One name for one idea; §5.1 already says
  "reductions".
- **`nth`, `position`, `rposition`, `cycle`, `fuse`, `peekable`, `by_ref`,
  `unzip`, `max_by` beside `max_by_key`, `chunks_exact`, `dedup`, `sum` over
  references as a separate method.** Each is expressible with the set above, and
  a closed set containing everything expressible is not closed.
- **A double-ended trait.** `reverse()` buffers. `next_back` doubles the
  implementation burden on every source to save one allocation in the rare chain
  that runs backwards.
- **`mean`, `variance`, `median`, `quantile`.** Wanted, and not here. They need a
  dtype-promotion policy and a defined summation order (§5.1) — numerics
  decisions, not iteration decisions. They belong to F1's numerics library, where
  `Tensor` must answer them with the same semantics.
- **`sum()` with no `Add` bound on strings.** Concatenation is not summation;
  `String.from(chain)` (§5.5) is.

---

## 2. Laziness

### 2.1 The decision

**Adapters are lazy. Terminals are eager. There is no eager mode.**

A chain does nothing until a terminal pulls it, and every adapter is a value of a
distinct type that owns its source. `docs.iterate().map(each.title)` allocates
nothing, touches nothing, and is legal over a forty-gigabyte file.

The reason is the frame. Glue code exists to move data that was deliberately not
loaded whole. Ruby's eagerness is affordable because Ruby's arrays are already in
memory; the moment a source is a file, a socket, a device buffer, or
`0..1_000_000_000`, eagerness stops being a style choice and becomes a memory
limit. Six adapters over a million rows allocate six million-element arrays
eagerly and none lazily, and `take(5)` after a `map` over a million rows does five
units of work lazily and a million eagerly.

### 2.2 What it costs, stated plainly

- **Types the user never wrote appear in errors.** The value of
  `docs.iterate().keep(p).map(f)` has type
  `MapOver of (Keep of (ArrayIterate of Document, «closure at line 12»), «closure
  at line 13»)`. This is Rust's worst diagnostic genre and it will be Science's
  too unless the renderer is told. **Requirement, in F0, not retrofitted:** the
  diagnostic renderer abbreviates any `Iterate`-implementing type to
  `a chain over Document (started at line 12)` at top level, and prints the full
  spelling only under `--explain`. It is cheap now and impossible to add
  convincingly later, because by then the tests encode the long form.
- **Monomorphization grows.** Every chain shape is a fresh set of instantiations
  (§5.3). Accepted: it is the same bet §3 already made for numeric
  specialisation, and pipeline chains are few and shallow.
- **Nothing happens until the end.** A chain whose terminal is never called is
  dead code that looks like work. **The compiler must warn when an `Iterate`
  value is dropped without a terminal**, in the same family as `SC0140`'s
  unchecked error. Without it, "I called `.map` and nothing happened" is the first bug
  every user files.
- **Debugging is harder**, and §1.5 refuses the usual mitigation on purpose.

### 2.3 Laziness under ownership: the chain stored in a variable

This is where laziness stops being free, so work it through.

`iterate()` takes a shared borrow of its source (§4), and each adapter *moves*
that borrow forward into itself. A chain value therefore **is** a borrow of its
source, wearing a long type. Storing it in a variable keeps the borrow live as
long as the variable is live.

```science
let mutable docs be load_documents()

let titles be docs
    .iterate()
    .map(each.title)                        # `titles` now borrows `docs`

docs.push(Document.new("late arrival"))     # SC0331

let out be titles.collect()
```

The `push` wants an exclusive borrow of `docs` while `titles` holds a shared one,
which is rule 4 of §6.1. The diagnostic must do what §6.2 demands of every region
error — reconstruct where the borrow was born and what keeps it alive:

```
error[SC0331]: `docs` is modified while a chain is still reading it
  --> pipeline.science:6:1
   |
 3 | let titles be docs
   |               ---- the chain takes a shared borrow of `docs` here
 6 | docs.push(Document.new("late arrival"))
   | ^^^^^^^^^ modifying `docs` needs exclusive access
 8 | let out be titles.collect()
   |            ------ but the chain is still read here
   |
help: run the chain before modifying the source — move line 8 above line 6
```

Three properties make this survivable rather than infuriating:

1. **Regions are flow-based** (§6.2), not lexical. Run the terminal before the
   `push` and there is no error at all, because the borrow is dead at the `push`.
   The obvious fix really is the fix, and the compiler can name it. Had Science
   chosen lexical lifetimes, stored chains would be close to unusable.
2. **A chain is neither `Copy` nor `Clone`, and terminals take `self` by value.**
   Using a chain after its terminal is `SC0301`, use after move, with the chain's
   own name in the message. A half-consumed chain can never be silently observed
   twice — the same argument §6.5 makes for random keys. The exceptions are
   `first`, `find`, `has_any` and `has_all`, which take `mutable self`
   deliberately so that a chain can be probed and then continued; those require
   `let mutable`.
3. **A chain cannot outlive its source, and in F0 cannot leave the function.**
   See §2.4.

A subtler consequence, worth stating because it will surprise: a chain over a
`Map` or `Array` freezes that collection for the chain's whole life, so the
"mutate while iterating" bug is a compile error rather than a rule in the manual.
This is the most valuable thing ownership buys the chain API, and it should be in
the tutorial, not the reference.

### 2.4 Chains do not cross function boundaries in F0

§5.2 requires fully annotated signatures. Writing
`-> MapOver of (Keep of (ArrayIterate of Document, ...), ...)` in a signature
is not a language anyone should ship, and the honest alternatives — an opaque
`some Iterate of Item = T` — are type-system features F0 does not have and §12
does not budget.

**F0's answer: a function returns a collection, not a chain.** Materialise at the
boundary with `.collect()`. It costs one allocation per boundary, and the frame
says the boundary is exactly where a pipeline wants something concrete: the value
handed to the next stage is a dataset, not a recipe.

Streaming across a boundary is not lost, because a *source* type is nameable:

```science
def lines_of(path: borrowed String) -> (Lines, Error?)
```

`Lines` is a concrete type implementing `Iterate`, so the caller chains over it
and the forty-gigabyte file is never materialised. The rule is: **libraries return
sources; user functions return collections.** F1 should add an opaque
`some Iterate of Item = T` return form; nothing here blocks it, and the day it
exists, every boundary `.collect()` becomes deletable without changing a name.

### 2.5 Laziness and dynamic dispatch

Every adapter method takes `self` by value and returns a type mentioning `Self`,
so none of them can appear in the vtable of `any Iterate` (§4.3). Rust calls this
object safety and makes the user write `where Self: Sized`. Science should not.
**AMENDMENT 12:** a trait method whose signature mentions `Self` by value is
automatically excluded from the `any Trait` vtable, and calling it on an
`any Trait` value is a type error that names the fix — bind the concrete chain
first, or `collect()` and chain over the `Array`. `next()` and
`estimated_length()` remain dispatchable, so `any Iterate` is still a usable
"some stream of `Item`" for the cases that need it.

---

## 3. Naming

The rule, derived from §4.3 and §4.6: **a chain should read as an imperative
sentence about the data**, and no name is an abbreviation where a word exists.
Applied consistently, and then broken on purpose where familiarity is worth more
than prose.

### 3.1 Where prose won

| Science | Elsewhere | Reason |
|---|---|---|
| `keep` / `discard` | `filter` / `filter(!p)` | "Filter" is famously ambiguous about direction — a coffee filter keeps the liquid, an air filter keeps the dirt. Two opposite verbs are unmistakable, and the negation leaves the predicate. §4.6 already chose `discard`. |
| `take` / `skip` | `limit` / `offset`, `take` / `drop` | §4.6 chose `take`. `skip` rather than `drop` because `Drop` is a trait (§5.4), and one word should not mean both "destructor" and "ignore some rows". |
| `every(n)` | `step_by(n)` | `every(10)` is what a user says aloud while downsampling. |
| `numbered()` | `enumerate()` | "Enumerate" means four different things to four audiences. `numbered` names the result, and pairs with the `index` field of §1.3. |
| `batches(n)` / `windows(n)` | `chunks(n)` / `windows(n)` | "Chunk" does not say whether the pieces overlap. `batches` does, it is the domain's own word, and the contrast with `windows` now carries information instead of folklore. |
| `followed_by(other)` | `chain(other)` | `chain` is the word for the whole construct; spending it on one link is a collision. |
| `expand(f)` | `flat_map(f)` | Two pieces of jargon compounded. `.expand(each.tokens())` says it in one. |
| `accumulate(...)` | `scan(...)` | "Scan" reads as a read-only traversal to most people; this one carries state. |
| `minimum` / `maximum` | `min` / `max` | §4.3 forbids abbreviations where a word exists. This is that rule with no exception carved out for brevity, and it is the entry most likely to be argued about. |
| `has_any` / `has_all` | `any` / `all` | `any` is reserved for dynamic dispatch. The prefix also makes the call read inside an `if`. |
| `owned()` | `cloned()` / `copied()` | One link instead of two, naming the outcome rather than the mechanism (§4.4). |
| `estimated_length()` | `size_hint()` | Same abbreviation rule. |
| `collect_or_error()` | `collect::<Result<_, _>>()` | The error policy belongs in the name, not in a turbofish (§6). |
| `unique()` | `dedup()` / `distinct()` | `dedup` is an abbreviation; `distinct` is SQL's word and this audience is not writing SQL. |
| `count()` vs `length()` | `len()` / `count()` | The abbreviation goes, and the surviving pair now marks an `O(1)` lookup against an `O(n)` consumption. |

### 3.2 Where familiarity won

| Kept | Why |
|---|---|
| `map` | The best-known combinator name in computing, and this audience already writes it in Python, Julia, JAX and R. `transform` buys prose and spends the one name everybody already has. It collides with the `Map` type, which is tolerable: case separates them and one of them is a noun. |
| `zip` | Universal, and the metaphor is genuinely good. |
| `reduce` | §5.1 already says "reductions". It is this audience's own word, from MapReduce and from every array library they use. |
| `collect` | §4.6 already uses it. |
| `flatten`, `first`, `find`, `last`, `count`, `sum`, `product`, `group`, `tally` | Already English. |
| `partition` | Statistics uses it in precisely this sense. |
| `sorted` / `sort` | `sorted` yields a new chain, `sort` reorders an `Array` in place. The pair mirrors Python's `sorted`/`sort`, which this audience has internalised, and the `-ed` genuinely marks the difference. |
| `union`, `intersection`, `difference` on `Set` | Mathematical terms for a mathematical audience. Renaming them to `shared_with` and `without` would be prose for its own sake. |

### 3.3 Labels are part of the name

§4.6 writes `docs.sort(by: doc giving doc.title.length())`. Keeping that reading
means `sorted()` and `sorted(by:)` must be two methods with one word between
them. **AMENDMENT 2: a method's argument labels are part of its name.**
Resolution picks the entry by name-plus-labels *before* type inference runs, so
this is not type-directed overloading and does not disturb §5.2.

The payoff is one consistent modifier across the vocabulary instead of a litter of
`_by` suffixes:

```science
rows.iterate().sorted(by: each.score)
rows.iterate().unique(by: each.id)
rows.iterate().group(by: each.label)
rows.iterate().tally(by: each.label)
rows.iterate().maximum(by: each.score)
```

When a label appears: **unlabelled when the verb already names its argument**
(`take(5)`, `map(f)`, `keep(p)`), **labelled when it selects between variants of
the same verb** (`by:`).

The same rule is why `collect()` is not inference-directed. Rust spells three
different collections with one method and disambiguates by the annotation on the
binding, which produces the turbofish and the worst inference errors in the
language. Science says: `collect()` returns an `Array`, always; other targets are
constructors (§5.5). One method, one type, no annotation ever required.

---

## 4. Ownership through a chain

### 4.1 Three sources, three words

A chain either reads its source, writes through it, or eats it. These are three
different programs and Science writes three different words:

```science
docs.iterate()                # Item = borrowed Doc          — read
docs.iterate_mutably()        # Item = mutable borrowed Doc  — write in place
docs.iterate_consuming()      # Item = Doc                   — take ownership
```

**This is Rust's answer with the non-word removed, and that is the right trade.**
The prompt is correct that `docs.iterate()` versus `docs.into_iterate()` is ugly.
The ugliness is not the count — three semantics need three names — it is that
`into_` is a preposition doing a verb's job, and that Rust then hides the choice
again behind `for x in v`, which silently consumes and burns every beginner once.

So: keep three, spell them in words, and put the ugliness in the first link of the
chain, where it is read once and every subsequent link stays clean. A reader of
any chain knows its ownership behaviour by looking at exactly one place.

Two alternatives were considered and rejected:

- **One `iterate()` whose mode is inferred from downstream use.** Rejected on
  §4.6's own principle: the language does not resolve ambiguity by guessing. It
  would also make adding a link change the meaning of earlier ones.
- **A `.consuming()` modifier after the source** (`docs.iterate().consuming()`).
  Rejected because it separates the decision from the borrow it governs and lets
  it appear anywhere in the chain, including after an adapter has already fixed
  the item type.

### 4.2 `for each` borrows

**AMENDMENT 11: `for each x in xs:` desugars to `xs.iterate()`** — it borrows. To
consume, the user writes it:

```science
for each doc in docs:                        # borrows; docs is usable afterwards
    println(doc.title)

for each doc in docs.iterate_consuming():    # moves docs; docs is gone
    archive(doc)
```

This fixes, at zero cost, the single most-reported wart in Rust's collection API.
The word "consuming" appears exactly where a value is being destroyed, which is
the standard §6 holds everywhere else.

### 4.3 What the items actually are

`iterate()` yields `borrowed Item` uniformly — no conditional associated type, no
specialisation (§12 excludes it). Two rules make that livable for a numeric
audience:

**AMENDMENT 6, the copy-out rule.** A place expression returned from a closure
yields the *value* when its type is `Copy`, and a *borrow* otherwise. So:

```science
samples.iterate().map(each.weight)   # weight: F32, Copy      -> Item = F32
docs.iterate().map(each.title)       # title: String, not Copy -> Item = borrowed String
```

Both are what the user wanted, and neither required a word. Note the consequence
for §4.6's headline example: `headlines` has type `Array of borrowed String` and
borrows `docs` — zero copies, and it cannot outlive `docs`. That is the right
default for a language whose pitch includes not copying arrays, and the escape is
one link: `.map(each.title).owned()` gives `Array of String`.

**AMENDMENT 7.** Operator traits are implemented for borrowed operands of the
primitives, with an owned `Output`. `borrowed F32 + borrowed F32` is `F32`. This
is what makes `values.iterate().sum()` compile when `Item = borrowed F32`, and it
is the reason `sum()`'s signature is written `where Self.Item: Add of Output =
Total` rather than returning `Self.Item`.

With those two rules, `.owned()` is needed only where it is genuinely meaningful:
the user wants values that outlive the source, and the item is not `Copy`. It is
one method rather than Rust's `cloned` plus `copied`, because the distinction
between those two exists only to make an implicit deep copy visible — and Science
already makes `Clone` visible through the type, so `owned()` is a no-op move for
`Copy` items and a `Clone` call otherwise, decided by the compiler and not by the
name.

### 4.4 Ownership summary through a chain

| Link | Borrows | Consumes | Copies |
|---|---|---|---|
| `iterate()` | the source, shared, for the chain's life | — | — |
| `iterate_mutably()` | the source, exclusively | — | — |
| `iterate_consuming()` | — | the source, at the call | — |
| any adapter | — | the chain it is called on | — |
| `owned()` | — | — | clones each item (moves, if `Copy`) |
| `batches(n)` / `windows(n)` | — | — | allocates one `Array` per step; items keep whatever ownership they had |
| `sorted(...)` / `reverse()` | — | — | buffers every item once |
| any terminal | — | the chain | — |

Items that were borrowed stay borrowed all the way into `collect()`. That is the
one thing users must internalise, and the type printed by `collect()` tells them:
`Array of borrowed Doc` is a set of views into something else, `Array of Doc` is
theirs.

---

## 5. The collection types

### 5.1 `Array` and `Map` (given), plus `Set`

§8 gives F0 `Box`, `String`, `Array`, `Map` and `Error` — absence and failure are
no longer library types under revision 2 (§5.5). This note adds
exactly one type.

**`Set of T` — accepted. AMENDMENT 10.** Membership, vocabularies, label sets,
seen-ids, and the backing store `unique()` already needs. Without it, users write
`Map of (T, ())`, which is worse in every way, and it appears in their code
whether the language ships it or not. It carries `union`, `intersection`,
`difference`, `insert`, `remove`, `has`, `length`, `iterate()`, and `Set.from`.

### 5.2 `Map` and `Set` iterate in insertion order

**AMENDMENT 9, and this is the most consequential paragraph in §5.** §5.1 of the
spec says results that differ between runs are opt-in, "because this is a language
whose users publish". A hash map with a randomised or capacity-dependent iteration
order breaks that promise the first time somebody prints a `group(by:)` result,
and the breakage is invisible in testing and appears in a paper.

`Map` and `Set` therefore iterate in **insertion order**. It matches what this
audience already believes, because Python's dict has worked this way since 3.7.
It costs one index array and it buys reproducibility by construction rather than
by discipline.

This also means `Hash` must exist. **AMENDMENT 8:** add `Hash` to §5.4's trait
list. `Map`, `Set`, `unique`, `group(by:)` and `tally(by:)` all require it, and
§5.4 as written lists no way to hash anything, which is a genuine hole in the F0
spec rather than an addition this note is asking for.

### 5.3 Refused, with reasons

- **A deque.** A pipeline moves forward. The one place a ring buffer is needed is
  inside `windows(n)`, which is an implementation detail of one adapter, not a
  type the user should name. Refused until something outside the chain API asks
  for it.
- **A sorted map / B-tree map.** Its only advantage over an insertion-ordered
  `Map` is iteration in key order, which is `.iterate().sorted(by: each.key)` —
  one link, on the rare occasion it is wanted. It would also force an `Ord` bound
  where `Hash` suffices, and `F32` deliberately does not implement `Ord` (§5.1),
  so a sorted map keyed by a float would not even compile. Refused.
- **A small-vector / inline array.** A pure performance optimisation with an ABI
  cost (§8 fixes representations) and no glue benefit. The allocation it saves is
  recovered better by `Array.reserve(n)` fed from `estimated_length()`, which
  `collect()` does automatically. Refused.
- **A slice type.** Chains need sub-ranges without copying, and the answer is
  already in the vocabulary: `.iterate().skip(a).take(b - a)` costs nothing and
  introduces no unsized type into a type system that has never needed one.
  Refused, and this is a saving, not a loss.
- **A priority queue.** Top-k is `.sorted(by:).reverse().take(k)`. The asymptotic
  win is real and nobody's pipeline is bounded by it. Refused.
- **A linked list.** No.
- **A table / dataframe.** Wanted by this audience, and much too large to be a
  core type. It is a library over `Array`, `Map` and F1's `Tensor`, and it needs
  column types, null handling and a schema — none of which belong in a closed F0
  set. Refused for F0, explicitly not refused forever.

### 5.4 What every collection owes the chain API

Each collection carries the same three source methods as **inherent** methods
(§4.4's `has methods`), not through a trait:

```science
Array of T has methods:
    def iterate(borrowed self) -> ArrayIterate of T
    def iterate_mutably(mutable borrowed self) -> ArrayIterateMutably of T
    def iterate_consuming(self) -> ArrayIterateConsuming of T
```

`Map` yields `Entry of (K, V)` and additionally offers `keys()`, `values()` and
`values_mutably()`. `Set` yields its elements. `String` offers `characters()` and
`lines()` as sources. `Range` is not a container and implements `Iterate`
directly (**AMENDMENT 14**), which is what makes `for each i in 0..n:` the same
construct as everything else rather than a special case in the parser.

F0 has no generic "iterable" trait, on purpose: nothing in F0 is generic over
containers, so the trait would exist only to be implemented. Because every
collection already spells the three methods identically, a `Sequence` trait can
be added in F1 by writing an `implements` block per type and changing no call
site anywhere.

Consequently `expand(f)` and `zip(other)` take something that implements
`Iterate`, not something iterable:

```science
rows.iterate().expand(row giving row.tokens().iterate_consuming())
features.iterate().zip(labels.iterate())
```

Slightly more to type; nothing hidden, and the ownership of the inner source is
visible in the same place as everywhere else.

### 5.5 Building a collection from a chain

`collect()` gives an `Array`. Everything else is a constructor, using the `From`
trait §5.4 already has:

```science
let vocabulary be Set.from(tokens.iterate().unique())
let index be Map.from(rows.iterate().map(r giving Entry(key: r.id, value: r)))
let joined be String.from(names.iterate())
```

One method, one type each; no annotation is ever load-bearing, and there is no
turbofish because there is nothing to disambiguate.

---

## 6. Errors inside a chain

### 6.1 What a fallible step does inside a chain

First, the spelling. `each` is a reserved binder (§13), so it cannot also be a
closure parameter name; the two legal forms are:

```science
lines.iterate().map(parse(each))              # implicit subject
lines.iterate().map(line giving parse(line))  # named parameter
```

`parse` is `-> (Row, ParseError?)`, so the closure returns a pair and the
chain's `Item` is `(Row, ParseError?)`.

**AMENDMENT 4 is withdrawn.** It read: *"`try` inside a closure body returns
from the closure, not from the enclosing function"*, and it existed because
`try` was a control-flow operator and a closure is a function, so the operator
had to be told which function body it returned from. `syntax-revision-2.md` §3
removes `try`; `?` is a presence *test*, not a propagation operator, and it
returns from nothing. The non-local-control-flow question this amendment settled
can no longer be asked, and the diagnostic it requested has no subject.

That is a genuine simplification, and it is the only one this section gets. It
is also worth being precise about what was removed: the amendment was not wrong,
it was made unnecessary by removing the feature it constrained. Nothing became
easier to *write*; one rule stopped needing to exist.

So the chain does not fail. It becomes a chain **of pairs**, and the user then
chooses, by name, what to do about them.

### 6.2 Three policies, three names

```science
# 1. Ignore the failures.
let rows be lines.iterate()
    .map(parse(each))                # Item = (Row, Error?)
    .keep_ok()                       # Item = Row
    .collect()                       # Array of Row

# 2. Stop at the first failure.
let rows, err be lines.iterate()
    .map(parse(each))                # Item = (Row, Error?)
    .collect_or_error()              # (Array of Row, Error?)
if err?:
    return (Array of Row .new(), err)

# 3. Keep both, which is usually what a scientist wants.
let outcome be lines.iterate()
    .map(parse(each))                # Item = (Row, Error?)
    .partition_results()             # Outcome of (Row, Error)

print(outcome.errors.length())
train(outcome.values)
```

Three lines of policy, each one word, each visible at the call site. This is the
whole answer to "how does a chain of fallible steps collect into a single
outcome rather than a mess": **the mess is a real choice and the language makes
the user name which one they made.** Rust's `collect::<Result<Vec<_>, _>>()` does
policy 2 through a type annotation on a binding, which is the cleverest and least
readable thing in its iterator API.

Policy 3 exists because of the frame. Nine thousand nine hundred and ninety-seven
rows parsed and three did not; the pipeline should run and the three should be
inspectable. A language that only offers "ignore" and "abort" forces a hand-rolled
loop the first time a real dataset shows up.

**The three names outlived the type they were named after, and this note has not
renamed them.** `keep_ok` reads `Ok`, `collect_or_error` returned a `Result`, and
`partition_results` is named for results — none of which is a type in the
language after `syntax-revision-2.md` §3. The *policies* survive untouched,
because a policy over `Item = (T, E?)` is the same policy it was over
`Item = Result of (T, E)`; only the names now point at nothing. The signatures in
§1.4 are therefore restated over `(T, E?)`, so that the semantics are unambiguous
whatever the names become, and the rename is left as an open question for
whoever next edits the closed set — it is an API decision, and this note should
not make it as a side effect of a syntax migration.

Note also that policy 2 costs a line it did not cost before. `collect_or_error()`
used to produce a `Result` that the enclosing `try` consumed on the same line;
it now produces a pair that has to be bound and tested. That is revision 2's
stated tax (§3.3 of that note) arriving in the chain API.

### 6.3 Several fallible steps — the hole this section used to fill is open again

Two fallible steps in one chain produce two error types unless something unifies
them. This section used to unify them:

> **AMENDMENT 5: `try` converts the error through `From`**, exactly as Rust's
> `?` does. Without it, the second fallible step in any chain fails to compile
> with a type error about `E1` and `E2` that has nothing to do with what the
> user did wrong.

**That amendment is dead, and it was killed deliberately.**
`syntax-revision-2.md` §3.2 lists `From` widening among the things the new model
removes, and §3.4 states the replacement in full: *"Conversion is explicit, and
that is the point [...] a function whose callee fails with `ConfigError` and
whose own signature says `Error?` converts at the return."*

A chain has no `return`. The conversion revision 2 relocated to the caller's
return statement has no call site inside a chain, so the failure mode AMENDMENT 5
was written to prevent — a type error about `E1` and `E2` that has nothing to do
with what the user did wrong — comes back exactly as described, and this note no
longer has a mechanism to point at.

Three options are visible from here and none of them is this note's alone to
choose:

1. **Convert in the closure.** `.map(row giving widen(validate(row)))`. Explicit,
   consistent with §3.4's principle, and it needs a per-pair conversion helper
   that neither `stdlib-core.md` nor the closed set of §1.4 currently contains.
2. **Declare every fallible step in a chain as `-> (T, Error?)`**, taking the
   boxed interface form. This works today with no new machinery, and §3.4 of the
   revision note prices it against exactly this case: *"for a function called in
   a tight loop that returns `Error?` on every iteration it is not [acceptable],
   and the concrete form is the answer."* A chain link is the tight loop.
3. **A chain-level conversion combinator.** One more name in a set §1.4 argues
   should stay closed, and the only option that keeps the concrete error types
   and the flat chain at once.

This note records the hole rather than closing it. §6.3 was always a dependency
on whoever owns the error design — the original text said so — and the error
design has since been rewritten in a way that removes the mechanism without
supplying a replacement. That is the one place in this note where the new error
model is straightforwardly worse than what it replaced, and it should not be
written up as anything else.

### 6.4 Two rules that keep the story honest

- **A chain never panics on a data error.** Every failure a dataset can cause
  arrives in an error slot. `panic` is for a broken program: an index out of
  bounds, a batch size of zero.
- **`reduce` over fallible items is not special-cased.** No `reduce_or_error`.
  Resolve the pairs first — `.collect_or_error()` and then test the error, or
  `.keep_ok()` — and reduce over values. One fewer name, and the policy stays
  visible.

---

## 7. Parallelism: what the chain API must do now

F2 brings data parallelism. The commitment is that `.parallel()` becomes a link
in an existing chain and nothing else about the chain changes:

```science
rows.iterate()
    .parallel()
    .keep(each.weight is above 0.0)
    .map(row giving score(encoder, row))
    .collect()
```

Seven things must be true in F0 for that to be addable rather than a redesign.

1. **Combinators are provided methods, never user-implemented.** A user
   implements `next()`; F2 adds a second trait — call it `Divide`, with
   `def divide(self) -> (Self, Self)?` — and gives it the same
   provided methods. Nobody's source code changes. Had the combinators been
   free functions or user obligations, every source in existence would need a
   second implementation.
2. **Every combinator is specified by its input-to-output relation, not by what
   `next()` does.** `keep(p)` is "the items for which `p` holds, in order", not
   "call `next` until `p` holds". The parallel form must be able to satisfy the
   spelled contract; a contract written in terms of pull order cannot be
   satisfied in parallel. Every entry in §1.4 must be documented that way in the
   reference, and any that cannot be is in the sequential column.
3. **The sequential/safe/ordered classification is fixed now** (§1.4), not
   discovered in F2. `.parallel()` before `accumulate`, `take_while`,
   `skip_while`, `unique`, `first` or `find` is a compile error naming the
   offending link, because the alternative is silently changing what the program
   computes. Fixing this now means F2 adds a check, not a taxonomy.
4. **`.parallel()` is a modifier on the source, placed before the first
   adapter.** It requires `Self: Divide`, so an `Array`, a `Map`, a `Set` and a
   `Range` are parallelisable and a line-by-line file source is not — and the
   error says exactly that, rather than silently running sequentially. Placing it
   mid-chain would mean an adapter changes the trait its successors flow in,
   which is expressible but makes the error messages incomprehensible.
5. **`estimated_length()` exists from the first commit.** Splitting needs a
   length; adding the method later means every third-party source silently
   returns the useless default forever, because nobody revisits a source that
   compiles.
6. **Closures are typed by their captures, not erased.** §2 already says F2 needs
   closures capturing by reference. `.parallel()` additionally needs to *reject*
   closures capturing anything `mutable borrowed`, since two workers would hold
   exclusive borrows of one thing. The compiler must therefore record each
   closure's capture set in its type from F0 — the check is F2's, the
   information must be F0's. This is the requirement most likely to be missed,
   because nothing in F0 reads that information.
7. **No combinator has an observable effect.** This is why §1.5 refuses
   `for_each` and `inspect`. A chain whose links are pure can be reordered,
   fused, split and re-run; one with a `println` in the middle cannot, and the
   user who wrote it will not understand why their output interleaves.

### 7.1 Reductions and reproducibility

`.parallel().sum()` over floats cannot produce the sequential answer, because
floating-point addition is not associative. §5.1 says reductions have a defined
order unless marked `unordered`, and the resolution is:

- **Determinism is a property of the program text, not of the execution mode.**
  `.parallel().sum()` is defined as a tree reduction whose shape is a function of
  the length and a fixed grain size *only* — never of the thread count, the
  machine, or the scheduler. Two runs of the same text on the same data give bitwise
  identical results anywhere. That is what a published result needs.
- **It differs from the sequential `.sum()` of the same data**, and the text
  differs too: the word `parallel` is on the screen. A user who writes it has
  said they want a different, still reproducible, reduction.
- **`unordered` remains the opt-out** for the faster non-deterministic merge,
  exactly as §5.1 intends.

This is the one place the chain API cannot give the user everything, and the
design choice is to make the trade visible in the source text rather than in a
footnote about thread counts.

---

## 8. A worked example

Rank a corpus against a query, with real error handling, real batching, and the
type at every link. `model` is a reserved word (§13), so the encoder is called
`encoder`; `Match` would shadow nothing but reads like `match`, so a hit is a
`Hit`.

```science
type Row:
    id: String
    text: String
    weight: F32
    split: String

type Embedded:
    id: String
    vector: Embedding

type Hit:
    id: String
    score: F32

def top_matches(path: borrowed String,
                     encoder: borrowed Encoder,
                     query: borrowed Embedding)
        -> (Array of Hit, Error?):

    let raw, err be read_lines(path)
    if err?:
        return (Array of Hit .new(), err)

    let loaded be raw
        .iterate()                                              # 1
        .discard(each.is_empty())                               # 2
        .skip(1)                                                # 3
        .map(line giving parse_row(line))                       # 4
        .partition_results()                                    # 5

    for problem in loaded.errors.iterate().take(5):
        print(problem)

    let ranked be loaded.values
        .iterate()                                              # 6
        .keep(each.split is "train")                            # 7
        .unique(by: each.id)                                    # 8
        .keep(each.weight >= 0.5)                               # 9
        .batches(64)                                            # 10
        .expand(group giving encode_batch(encoder, group)
                             .iterate_consuming())              # 11
        .map(row giving Hit(id: row.id,
                            score: cosine(row.vector, query)))  # 12
        .sorted(by: each.score)                                 # 13
        .reverse()                                              # 14
        .take(10)                                               # 15
        .collect()                                              # 16

    return (ranked, null)
```

The type at every stage:

| # | Link | Value's `Item` after this link |
|---|---|---|
| — | `raw` | `Array of String` (a collection, not a chain) |
| 1 | `.iterate()` | `borrowed String` |
| 2 | `.discard(each.is_empty())` | `borrowed String` |
| 3 | `.skip(1)` | `borrowed String` |
| 4 | `.map(line giving parse_row(line))` | `(Row, ParseError?)` |
| 5 | `.partition_results()` | terminal — `Outcome of (Row, ParseError)` |
| — | `loaded.values` | `Array of Row`; `loaded.errors` is `Array of ParseError` |
| 6 | `.iterate()` | `borrowed Row` |
| 7 | `.keep(each.split is "train")` | `borrowed Row` |
| 8 | `.unique(by: each.id)` | `borrowed Row` |
| 9 | `.keep(each.weight >= 0.5)` | `borrowed Row` |
| 10 | `.batches(64)` | `Array of borrowed Row` |
| 11 | `.expand(...)` | `Embedded` (owned — `encode_batch` allocates them) |
| 12 | `.map(row giving Hit(...))` | `Hit` (owned) |
| 13 | `.sorted(by: each.score)` | `Hit` — **barrier**, buffers the whole stream |
| 14 | `.reverse()` | `Hit` — **barrier** |
| 15 | `.take(10)` | `Hit` |
| 16 | `.collect()` | terminal — `Array of Hit` |

Seven things this example is meant to demonstrate:

- **Links 1–5 are one chain that ends at a terminal**, because the error policy
  has to be chosen before the data can be used. §6's policy 3: the three bad rows
  are printed, the rest of the pipeline runs.
- **Links 6–10 never copy a row.** `Item` is `borrowed Row` all the way, and
  `batches(64)` allocates an `Array` of sixty-four *borrows*, not of rows. The
  chain holds a shared borrow of `loaded.values` for its whole life, so mutating
  `loaded` anywhere between line 6 and line 16 is `SC0331` (§2.3).
- **Link 11 is where ownership changes**, and it is visible: `encode_batch`
  returns an `Array of Embedded` it allocated, and `iterate_consuming()` says the
  chain eats it. Nothing after link 11 borrows anything from `raw`.
- **Link 12 moves `row.id` and `row.vector` out of an owned item** — legal
  because `Item` is owned after link 11, and the reason `Hit` owns its `id`
  instead of borrowing it.
- **The result outlives every source.** `Array of Hit` contains no borrows, so it
  can be returned from the function, which is what §2.4 requires of every
  boundary.
- **Both barriers are at the end, after the filters.** The vocabulary does not
  enforce that, and the reference must teach it: `.sorted(...)` before
  `.keep(...)` buffers rows it is about to throw away. This is the one
  performance trap laziness leaves open, and it is a documentation problem, not a
  language problem.
- **Adding `.parallel()` after link 6 would be a compile error today and in F2**,
  because `unique(by:)` at link 8 is sequential (§1.4). Moving the `unique`
  before the `parallel` makes it legal. The error names the link — which is only
  possible because §1.4 classified every combinator before the feature existed.

---

## 9. Proposed diagnostics

Codes are proposals in the ranges §9 of the spec defines; the registry is the
spec's to assign.

| Code | Phase | Condition |
|---|---|---|
| `SC0116` | Syntax | `each` used in a call whose closure takes more than one parameter. Fix: the named form (compare `SC0115` for nesting). |
| `SC0271` | Types | An adapter method called on an `any Iterate` value (§2.5). Fix: bind the concrete chain, or `collect()` first. |
| `SC0272` | Types | `.parallel()` applied to a chain containing a sequential-only combinator; names the link. |
| `SC0273` | Types | `.parallel()` on a source that is not divisible; names the source type. |
| `SC0331` | Ownership | The source of a live chain is mutated or moved; must show where the chain took the borrow, and where the chain is next used (§2.3). |
| `SC0332` | Ownership | A chain used after its terminal consumed it. A specialisation of `SC0301` whose message says "this chain was consumed by `.collect()` here". |
| warning | Types | An `Iterate` value dropped without a terminal (§2.2). |

---

## 10. Amendments this design requires

Numbered as referenced above. Items 4–7 and 8–9 are the load-bearing ones;
several sit in another designer's territory and are flagged for routing.

1. **Function types are spelled `(A) -> B`**, with no keyword, and may appear in
   `where` clauses — which means `parse_type_bound` must stop being a path.
   Decided in §1.2, where the tuple ambiguity is worked out. *(Type system.)*
2. **Argument labels are part of a method's name**, resolved before type
   inference. Needed to keep §4.6's own `sort(by:)`. *(Resolution.)*
3. **Multi-parameter closures** are `(a, b) giving expression`; `each` is defined
   only at arity one, with a diagnostic. *(Syntax.)*
4. ~~**`try` inside a closure returns from the closure.**~~ **Withdrawn** —
   `syntax-revision-2.md` §3 removes `try`, so the rule has no subject (§6.1).
5. ~~**`try` converts the error through `From`.**~~ **Dead, and the problem it
   solved is open.** `syntax-revision-2.md` §3.2 removes `From` widening and
   §3.4 relocates conversion to the caller's `return`, which a chain does not
   have. A chain with two fallible steps of different error types still does not
   compile, and this note no longer has a mechanism to ask for. *(Errors — still
   not this note's to decide; §6.3 states the three options.)*
6. **The copy-out rule**: a place expression returned from a closure yields the
   value when `Copy`, a borrow otherwise. *(Types / ownership.)*
7. **Operator traits are implemented for borrowed primitive operands** with owned
   `Output`, so arithmetic works on borrowed items. *(Library / traits.)*
8. **Add `Hash` to §5.4's trait list.** `Map` itself does not work without it;
   this is a hole in F0 as written, not an extension. *(Traits.)*
9. **`Map` and `Set` iterate in insertion order**, for the reproducibility
   §5.1 promises. *(Library.)*
10. **Add `Set of T` to §8's library list.** *(Library.)*
11. **`for each x in xs:` desugars to `xs.iterate()`** — it borrows; consuming is
    written. *(Syntax / library.)*
12. **Methods taking `self` by value are automatically excluded from `any Trait`
    vtables**, with a diagnostic instead of a `Sized` bound the user must write.
    *(Traits.)*
13. **An `Iterate` value dropped without a terminal is a warning.** *(Types.)*
14. **`Range` implements `Iterate`.** *(Library.)*
15. **The diagnostic renderer abbreviates chain types** to
    `a chain over T (started at line N)`, with the full spelling under
    `--explain`. Must exist in F0; it is unbuildable later, once the UI tests
    have baked the long form in. *(Diagnostics.)*

### Open questions for the owner

- §4.6's headline example, `headlines`, has type `Array of borrowed String` under
  this design, not `Array of String`. That is the zero-copy default and it is
  defensible, but the spec's example should say so, or add `.owned()`.
- `count()` versus `length()` and `sorted()` versus `sort()` are both
  near-synonym pairs carrying a real distinction. If the owner judges that too
  subtle, the fallback is `consume_counting()` and `sorted_copy()`, which are
  uglier and unmistakable. The recommendation is to keep the short pairs and
  teach them.
- Whether `.parallel()` should be spelled `.in_parallel()` to read as a chain
  link is left open. `parallel` is already reserved (§13); `in` is too.
