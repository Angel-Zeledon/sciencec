# Science — Design: indexing, slicing, and array literals

Date: 2026-09-16
Status: draft for review
Written in: the syntax of `docs/superpowers/design/syntax-revision-2.md`. Every
sample below is revision-2 Science — `->`, `for x in xs`, `is`/`is not`, `print`,
`interface`, `Type has:`, `T?`, `-> (T, Error?)`.
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §4.5, §4.6,
§4.7, §5.4, §6.5 and §8.
Contradicts, openly: `collections-and-chains.md` §5.3 (the slice refusal) — §2.6
here says why.
Discharges: `data-io.md` §11.6.

---

## 0. The hole this note fills, measured

The language has no way to read the second element of an array.

That is not a rhetorical opening. It is the literal state of the specification:

| Fact | Where |
|---|---|
| The core spec contains no array literal syntax. Zero occurrences of `[` as a value form. | §4.2's literal list stops at `()` |
| The word `index` appears exactly once in the whole spec | §4.6's precedence table, top row, with no operand type and no rule |
| `Index` is named as an operator trait and never given a method, an operand, or a syntax | §5.4 |
| `Array` and `Map` are read through `get`, which returns an optional | §8, as read by `examples/README.md` |
| No file in the corpus contains a `[` | `examples/README.md`, "beyond the spec" |
| A data-format note had to call a builder three times because it could not write a list | `data-io.md` §11.6 |
| §6.5 advertises in-place `x[i] = v` as a thing ownership *buys the user*, and §4 gives no syntax for it | §6.5 vs §4.5 |

The last row is the one to sit with. §6.5 is the section that defends ownership
to a sceptical audience, and its second argument is that Futhark needed
uniqueness types "solely to let a pure language write `x[i] = v` in place safely;
ownership is the same guarantee with a better surface." The spec sells a feature
it cannot spell.

This note closes all of it. The through-line is the language's own bet, from §1
of the core spec — *the mistakes a compiler can catch before a run starts* —
applied to the operation a scientific program performs more often than any other.

**The one-paragraph summary.** `a[i]` indexes and `[1, 2, 3]` is a literal, with
the bracket's role decided by parser position and nothing else. An index is
**checked at compile time wherever the compiler can discharge the obligation
`0 <= i < extent`**, and falls back to a runtime check it reports as a
diagnostic, not silently. `get` survives, returning `T?`, for the case that is
genuinely dynamic. Slices borrow and do not copy. Indices are zero-based, and the
reason is the DLPack membrane, not aesthetics. Comprehensions are rejected.

---

## 1. Indexing

### 1.1 The surface

**Decision 1.** `a[i]` indexes. `m[i, j]` is a single index of arity two, not
`m[i][j]`. The contents of an index bracket are a comma-separated list of **index
positions**, each of which is an expression, a range (§2), or a bare `..`.

```science
let a be [10, 20, 30]
print(a[0])                      # 10

let m be Tensor.from([[1.0, 0.0], [0.0, 1.0]])
print(m[1, 1])                   # 1.0        — F1
```

**An index bracket does not contain a tuple.** `a[t]` where `t` has a pair type
does not index a rank-2 thing. The comma inside the brackets is grammar, not a
constructor. This is §4.6's own rule — *a language cannot resolve an ambiguity by
guessing* — applied one more time.

**`a[i][j]` and `a[i, j]` are different programs and stay different.**

| Expression | Meaning | Where it is right |
|---|---|---|
| `a[i][j]` | two index operations | `Array of (Array of T)`, whose rows are separately allocated |
| `a[i, j]` | one index operation of arity two | `Tensor`, `Matrix` — rank-2, one buffer |
| `m[i]` on a rank-2 | one index position against a rank-2: yields a rank-1 row view | F1 |

Making the two spellings aliases was considered and rejected. On a `Tensor`,
`m[i][j]` would have to materialise or synthesise a row view for every element
access in an inner loop, and the reader would have no way to see which of the two
they wrote. Two spellings, two meanings, no guessing.

**Assignment.** `a[i] be v` writes through the index. This is the syntax §6.5
promised and never supplied.

```science
function scale(values: mutable borrowed Array of F64, factor: F64):
    for i in 0..values.length():
        values[i] be values[i] * factor
```

**Decision 2.** Two traits, not one. `Index` reads and yields `borrowed Output`;
`IndexMutably` writes and yields `mutable borrowed Output`. §5.4 currently lists
`Index` with no parameter, which cannot express slicing (§2) because the index
type varies. It gains one.

```science
interface Index of Idx:
    type Output
    function index(borrowed self, at: Idx) -> borrowed Self.Output

interface IndexMutably of Idx:
    type Output
    function index_mutably(mutable borrowed self, at: Idx) -> mutable borrowed Self.Output
```

`a[i] be v` requires `IndexMutably`; `a[i]` on the right of a `be` requires
`Index`. A read-only container implements only the first, and the diagnostic for
assigning through it names the missing implementation rather than talking about
mutability in the abstract.

### 1.2 The no-panic problem, stated honestly

The brief for this note says §8 "states that no indexing operator may panic."
It does not — and the distinction matters, because it is the difference between
amending a rule and inventing one.

What §8 actually says is that the F0 library is closed and covers `Array` and
`Map`. What `examples/README.md` records, under "beyond the spec", is the
consequence the corpus lives by: `get` "returns a `T?` and cannot panic," and
"no file in the corpus contains a `[`." The prohibition is a **property of the
surface as built**, not a written rule. Nobody has yet had to decide whether an
indexing operator may panic, because there has been no indexing operator.

So this note is not overturning a rule. It is making the decision that was
deferred by the absence of the feature. But the property is worth keeping for its
own sake, and everything below is an attempt to keep as much of it as is
affordable.

### 1.3 The resolution: index obligations

**Decision 3.** Every `a[i]` emits an **index obligation**: `0 <= i` and
`i < extent(a)`. The compiler tries to discharge it. If it succeeds, the compiled
code contains **no bounds check** and the expression **cannot fail**. If it
fails, the compiler inserts a runtime check that panics, and **reports that it
had to** (`SC0286`).

That last clause is the whole design. Rust and C++ both make this choice, and
both make it silently — you cannot tell by reading `a[i]` whether it compiled to
a load or to a load with a branch, and you certainly cannot tell whether it can
abort your six-hour run at hour five. Science's premise is that the compiler
tells you what it proved. Applying that to indexing costs one diagnostic and buys
the language's thesis at its highest-traffic site.

**What the compiler must prove.** The discharger is a **closed list of four
rules**, deliberately not a solver:

| Rule | Discharges | Needs |
|---|---|---|
| **Constant** | `a[3]` where `a`'s extent is a compile-time constant — an F1 `Tensor of (F64, (2, 2))`, a const-generic `Matrix of (T, M, N)`, or an array literal bound and not resized | Const-generic evaluation, which §5.3 already has |
| **Loop range** | `a[i]` inside `for i in 0..a.length():`, where `a` is not resized or reassigned in the body | Recognising the induction variable's range; the "not resized" half is a borrow fact the region engine already computes |
| **Narrowed** | `a[i]` after `if i < a.length():`, in the branch where it holds | Flow-sensitive narrowing — the same machinery `syntax-revision-2.md` §3.1 already requires for `T?`, extended from nullability to one integer relation |
| **Shape equality** | `left[i]` where `i` came from a loop over `right` and the two share a shape variable — `Tensor of (F32, (n, 768))` twice | F1's shape unifier, which F1 is building anyway |

Anything else is undischarged. `a[compute(x)]`, `a[i - 1]`, `a[i]` where `i` came
off a CSV — all undischarged, all get a runtime check, all get `SC0286`.

**Decision 4: the fallback panics, and it is not a type error.**

Two alternatives were considered.

- **`a[i]` is a type error unless provable.** This is the purist reading of the
  verification thesis and it is unshippable. F0 has no static shapes at all;
  under this rule the **Constant** and **Shape equality** rules never fire in F0,
  leaving only the loop and narrowing forms, and `a[i]` would be illegal in the
  majority of programs anyone writes in the first release. A feature that is a
  compile error in the common case is not a feature.
- **`a[i]` always returns `T?`.** Honest, total, and nobody will write it.
  `a[i]? * b[j]?` in an inner loop is not a notation a physicist will accept, and
  the audience would reach for a library with brackets instead. It is also
  redundant: this *is* `get`, which already exists (§1.4).

So the fallback panics. The mitigation is not that the panic is unlikely — it is
that the panic is **declared**. `SC0286` is a warning by default and an error
under a `--verified` build profile (§8, ask 9), so a lab that wants the guarantee
can have it as a build flag rather than as a dialect.

**Rendered, this is what the load-bearing diagnostic looks like:**

```
warning[SC0286]: this index is not proved to be in range
  --> analysis.science:41:20
   |
41 |     let value be row[offset]
   |                    ^^^^^^^^ `offset` could be anything from `parse_offset`
   |
note: the extent of `row` is not known here
  --> analysis.science:38:9
   |
38 |     let row be frame.column("t")
   |         ^^^ length is a runtime value
   |
help: guard it, and the check disappears
   |     if offset < row.length():
   |         let value be row[offset]
help: or take the optional, and handle the absence
   |     let value be row.get(offset)
```

The message names *what it could not prove* and *which fact it was missing*. A
diagnostic that only says "bounds check inserted" would be turned off within a
week.

### 1.4 `get` survives, and when to reach for which

**Decision 5.** `get` stays, and returns `T?` in the revision-2 spelling — not
`Option of T`, which that revision removed.

```science
Array of T has:
    function get(borrowed self, at: Int) -> (borrowed T)?
    function get_mutably(mutable borrowed self, at: Int) -> (mutable borrowed T)?
```

```science
let first be row.get(0)
if first?:
    print(first)
```

The guidance is short enough to fit on a card, and it should be in the tutorial:

| Situation | Write |
|---|---|
| The index came from a loop over the thing, or from a shape | `a[i]` — it compiles to a load |
| The index came from a file, a user, an argument, or arithmetic | `if i < a.length():` then `a[i]`, or `a.get(i)` |
| Absence is a normal outcome you want to handle, not a bug | `a.get(i)` |
| You are walking all of it | `for x in a:` — no index at all |

The fourth row is the one to push hardest in teaching. Most indices in NumPy code
exist because NumPy has no better loop; Science has `for x in a:` and the whole
chain vocabulary of `collections-and-chains.md`, and an index that could have been
a chain is a code smell the reviewer should catch.

**Who this serves.** Row one serves the numerics audience: the inner loop of a
kernel gets brackets and no checks. Rows two and three serve the pipeline
audience: data off a disk is dynamic, and the language says so in the type. The
two audiences do not conflict here, which is why this is the easy section.

---

## 2. Slicing

### 2.1 The forms

**Decision 6.** Ranges are already the notation (§4.5 chose `..` and `..=` over
`:` precisely because `:` opens blocks), so slicing needs no new syntax at all —
only a rule about where an open end is legal.

```science
let window be samples[1..5]        # elements 1, 2, 3, 4
let head be samples[..5]           # from the start
let tail be samples[2..]           # to the end
let all be samples[..]             # the whole thing, as a view
let closed be samples[1..=5]       # elements 1 through 5
```

And, in F1, across axes:

```science
let column be m[.., 0]             # every row, column 0 — rank-1
let block be m[1..3, ..]           # rows 1 and 2, every column — rank-2
```

**The rank rule is NumPy's, on purpose.** An index position that is an integer
**drops** its axis; a position that is a range or a bare `..` **keeps** it. So
`m[.., 0]` on a rank-2 is rank-1 and `m[1..3, ..]` is rank-2. This serves the
NumPy/PyTorch audience directly, and it is the rule they already have in their
fingers. No argument for a different one survives contact with that fact.

### 2.2 Open-ended ranges: legal inside brackets, nowhere else

**Decision 7.** `..5`, `2..` and a bare `..` are legal **only as index
positions**. Everywhere else a range requires both endpoints, exactly as today.

The parser's own comment (`crates/science-parser/src/parser.rs`, `parse_range`)
states the problem and the current answer:

> Both ends are required. An open end would have to be told from a `..` that ends
> a line, and F0 has no method that takes one — §8 gives `Array` no slicing.

Both halves of that reasoning change here, but only inside brackets, and the rule
that results is local and needs one token of lookahead:

> **Rule.** Within an index bracket, after `..` or `..=`, if the next token is
> `]` or `,`, the range has no end. Before `..`, if the previous token is `[` or
> `,`, it has no start. Outside an index bracket, both ends are required.

Inside brackets the ambiguity the parser comment describes **cannot arise**,
because §4.2 already says newlines and indentation are ignored entirely inside
unclosed brackets. There is no "`..` that ends a line" in there; there is only a
`..` followed by `]` or `,`, and neither of those can start an expression.

Two alternatives were rejected.

- **Open ends legal everywhere, with the newline case resolved by lookahead.**
  Rejected because it makes a line break load-bearing *inside* an expression.
  §4.6 refused exactly this: "a line break is otherwise the end of a statement,
  and a reader should not have to look ahead to know whether it was." A `let r be
  2..` followed by an indented continuation would be a genuine puzzle, and the
  payoff — being able to store a half-open range in a variable — is a case nobody
  has asked for.
- **Named methods: `a.prefix(5)`, `a.suffix(2)`, `a.between(1, 5)`.** Rejected on
  §8's closed-set discipline: three more names against a notation the audience
  already reads, and the multidimensional case (`m[.., 0]`) has no method form
  that is not worse than the brackets.

**Precedence works out already.** §4.6's table does not place `..`; the parser
places it looser than every operator, which is why `a[0..n - 1]` reads as
`0..(n - 1)` and not as a subtraction from a range. That is the right choice and
this note does not disturb it. §4.6's table should record it rather than leaving
it in a comment (§8, ask 1).

### 2.3 A slice borrows

**Decision 8.** `a[1..5]` has type `Slice of T`. It is `{ borrowed T, Int }` —
a pointer and a length — with the region inferred exactly as §4.4 infers the
region in `type View: source: borrowed Doc`. **It does not copy.** The mutable
form is `MutableSlice of T`, produced through `IndexMutably`.

Copying was considered and it is not a close call. The pitch of `data-io.md` §1 is
that `frame.columns().temperature` is "the actual contiguous buffer, not a copy";
a slicing operator that copies would mean that taking half of a ten-gigabyte
column costs five gigabytes and a memcpy. The entire reason this audience tolerates
ownership is that it gets views for free.

### 2.4 How that lands in §6's regions

A slice is a borrow, so §6.1's rules apply to it unchanged and no new ownership
machinery is needed:

- Rule 4: while a `Slice of T` is live, its source is borrowed shared, so the
  source may be read but not exclusively borrowed. While a `MutableSlice of T` is
  live, the source may not be touched at all.
- Rule 5: a slice may not outlive its source. Returning `local[1..5]` from a
  function is a borrow-escape error, caught by the existing analysis.
- Resizing the source while a slice is live is an error, because `push` takes
  `mutable borrowed self` and rule 4 forbids it. This is the case that would be a
  dangling pointer in C++ and a silent aliasing bug in NumPy, and it falls out for
  free.

**The diagnostic already exists in a sibling note.** `collections-and-chains.md`
§9 allocates `SC0331` for "the source of a live chain is mutated or moved," with
the requirement that it show where the borrow was taken and where it is next
used. That is exactly the message a live slice needs. **This note asks that
`SC0331` be generalised from "chain" to "view", covering both** (§8, ask 4),
rather than allocating a second code whose rendered text would be the first one
with a noun swapped.

**The cost, stated.** `let s be a[1..5]` pins `a` for as long as `s` lives, and
every NumPy user will hit it once. That is not a new cost this note introduces —
§6.5 already banked it: "NumPy's silent view aliasing becomes a compile error."
This is the section where the user finds out.

### 2.5 Contiguity, and what waits for F1

**Decision 9.** In F0, a slice is **contiguous**, one-dimensional, over `Array`.
Strided and multidimensional views are F1, on `Tensor`.

`m[.., 0]` is a column. A column of a row-major matrix is not contiguous, so
`{ ptr, len }` cannot represent it — it needs a stride. F0 has no type with a
stride field. F1 does, and it is already specified: §8 fixes the tensor runtime
header as DLPack-compatible, which is "data pointer, device kind and id, rank,
dtype, shape, strides, byte offset." Strides are in the ABI commitment F0 already
made.

So the split is clean and it does not cost a grammar change later:

| | F0 | F1 |
|---|---|---|
| Container | `Array of T` | `Tensor`, `Matrix`, `Vector` |
| Index arity | 1 | up to rank |
| `a[1..5]` | `Slice of T`, contiguous | a rank-1 tensor view |
| `m[.., 0]` | `SC0283` (arity) | a strided rank-1 view |
| Bounds proof | loop rule, narrowing rule | plus constant and shape-equality rules |

**The grammar is specified in full now and implemented in stages.** That is
deliberate: the parser and the AST should carry index arity and open-ended ranges
from the first commit, so that F1 adds a type rule and not a syntax revision.
This project has had two syntax revisions in one day (`syntax-revision-2.md` §10)
and should not budget for a third.

### 2.6 The contradiction with `collections-and-chains.md` §5.3, named

That note refuses a slice type, and the refusal is worth quoting in full:

> **A slice type.** Chains need sub-ranges without copying, and the answer is
> already in the vocabulary: `.iterate().skip(a).take(b - a)` costs nothing and
> introduces no unsized type into a type system that has never needed one.
> Refused, and this is a saving, not a loss.

**This note contradicts that, and here is why it is not a reversal.**

1. **The refusal is correct for what it was deciding.** For a *chain*, a slice
   type genuinely buys nothing. `.skip(a).take(b - a)` is free and composes. That
   part of §5.3 stands and this note does not ask for it to change.

2. **It does not generalise past chains, because a chain is not addressable.**
   You cannot hand `.skip(a).take(b - a)` to `dgemm`. `ffi-c-boundary.md` §1.7
   calls BLAS with pointers and lengths; `data-io.md` §1 stakes its argument on a
   column being "the actual contiguous buffer"; F1's `Tensor.viewing(borrowed
   Array of T)` (`data-io.md` §11.8) needs a base pointer. A lazy adapter has
   none of those. Streams and buffers are different things and a decision made
   about streams does not bind buffers.

3. **The type already exists, under another name.** `ffi-c-boundary.md` §1.3
   defines `ffi.Span of T` as "`{ borrowed T, Int }` — a pointer with a length,
   holding a borrow whose region is inferred." That is `Slice of T`, field for
   field. The choice is not whether Science has this type; it is whether the type
   is nameable in safe code or only reachable through the FFI module.

4. **The "unsized type" objection does not apply.** §5.3's concern is Rust's `[T]`
   — a dynamically-sized type that needs `Sized` bounds, fat pointers in generic
   position, and a `Box of [T]` story. `Slice of T` here is an ordinary **sized
   two-word value**. Science never needs the unsized form, because nothing in the
   language puts an unsized type behind a pointer of its own.

**The ask** is that §5.3's bullet be amended to read "refused *for the chain
API*," with a pointer here (§8, ask 4). Its reasoning survives; its scope
shrinks.

---

## 3. Array and matrix literals

### 3.1 The forms, and the type

**Decision 10.** `[e, e, ...]` is an array literal. Its type is `Array of T`,
**always**, where `T` is the unification of the element types. It is never
inference-directed into some other collection.

```science
let primes be [2, 3, 5, 7, 11]                 # Array of Int
let weights be [0.1, 0.5, 0.4]                 # Array of F64
let identity be [[1.0, 0.0], [0.0, 1.0]]       # Array of (Array of F64)
let names be [
    "alpha",
    "beta",
    "gamma",
]
```

The trailing comma needs no new rule: §4.7 already allows one "in every bracketed
and parenthesized list," and an array literal is a bracketed list. Keeping that
consistent matters more than it looks — it is the difference between one rule and
two.

**Not inference-directed** is the same decision `collections-and-chains.md` §3.3
made for `collect()`, and for the same reason. A literal whose type depends on
where you put it is a literal you cannot read locally, and §5.2 makes inference
local on purpose. Other collections are built through `From`, which that note
already specified:

```science
let vocabulary be Set.from(["a", "b", "c"].iterate_consuming())
```

That is more to type than `{"a", "b", "c"}` would be. The cost is accepted: one
literal form, one type, no set/dict brace war, and `Set` keeps the explicit
ownership word that §4.1 of that note put in the first link of every chain.

### 3.2 Element types

Unification, per §5.2, with **no implicit numeric conversion**, per §5.1.

```science
let good be [1, 2, 3]              # Array of Int
let also be [1.0f32, 2.0f32]       # Array of F32
let bad be [1, 2.0]                # SC0281 — Int and F64 do not unify
let fixed be [1.0, 2.0]            # write the literal you meant
```

`[1, 2.0]` being an error will annoy someone on their first day. It is the same
rule as `1 + 2.0` being an error, it is §5.1's rule and not this note's, and
making literals the one place implicit widening happens would be worse than the
annoyance.

Defaulting: an unsuffixed integer literal is `Int` (`I64`) and an unsuffixed float
literal is `Float` (`F64`), per §5.1's aliases, unless unified against an
annotation or a parameter type.

### 3.3 The empty literal

**Decision 11.** `[]` takes its element type from the expected type at its
position. With no expected type, it is `SC0282`.

```science
let empty: Array of F64 be []                     # fine
push_all(results, [])                             # fine — the parameter says
let nothing be []                                 # SC0282
```

This is a bounded amount of bidirectional checking — an expected type pushed into
one expression form — and it is worth naming as a cost, because §5.2 describes
inference as unification and this is not that. The alternative, requiring
`(Array of F64).new()` for every empty case, was rejected because the empty
literal appears mostly as a default in a record or an argument, exactly where an
expected type is available, and refusing it there would be pedantry.

`SC0282`'s fix is the annotation, and it should suggest the specific type when
the compiler can see one candidate.

### 3.4 Discharging `data-io.md` §11.6

That note records the gap plainly:

> **Array literals** — this note avoids them (`null_value` is called three times
> rather than taking a list) because §4.2 of the core spec lists no array
> literal. If one is added, these builders should take it.

They should. The worked example in `data-io.md` §10 becomes, in revision-2
syntax:

```science
let options be CsvOptions.new()
    .header(true)
    .delimiter(',')
    .null_values(["", "NA", "-999"])
    .on_bad_row(BadRow.Skip(500))
```

Three builder calls collapse to one, and the plural name is now honest. This is
the smallest item in this note and the most immediately felt.

### 3.5 Literals and tensors in F1

**Decision 12.** A literal is always an `Array`. It never becomes a `Tensor`
implicitly. `Tensor.from` is the constructor, and the compiler checks the shape
**from the literal's syntactic shape**, which it can count at compile time.

```science
let identity: Tensor of (F64, (2, 2)) be Tensor.from([[1.0, 0.0], [0.0, 1.0]])
let wrong: Tensor of (F64, (3, 3)) be Tensor.from([[1.0, 0.0], [0.0, 1.0]])
#                                                 ^ SC0284 — literal is 2 by 2
```

Rejected: a polymorphic literal that becomes whatever is expected. It fails the
same test as everything else in this note — the language does not guess — and it
has a concrete second problem. A ragged nested literal must produce a good error,
and it can only do so if the literal has a definite type of its own to disagree
with.

Note the asymmetry, because it is deliberate and will be questioned:

- In **F0**, `[[1, 2], [3]]` is **legal**. It is an `Array of (Array of Int)` and
  arrays of differing lengths are an ordinary value.
- In **F1**, `Tensor.from([[1, 2], [3]])` is `SC0284`, and the message names both
  lengths and both spans.

Raggedness is a tensor property, not a literal property, and the error belongs
where the rectangularity is required.

**This is also the note's worst performance hazard**, and §9 records it: the
obvious way to build a matrix allocates an `Array of (Array of F64)` — one
allocation per row — and then throws it away. F1 must special-case a literal
argument to `Tensor.from` and lower it straight into the tensor buffer, or the
ergonomic path is the slow path and everyone will find out on a 4096×4096.

---

## 4. Zero-based, and the fight

### 4.1 The fight is real

| One-based | Zero-based |
|---|---|
| Fortran, MATLAB, R, Julia, Mathematica, Lua | C, C++, Python, Rust, Go, Java, JavaScript, NumPy, PyTorch, JAX |

The one-based column is not a list of bad languages. It is close to a list of the
languages a computational physicist learned in, and Julia in particular is the
closest thing Science has to a direct competitor for exactly that user. Julia
chose one-based knowingly, over loud objection, and it was not a mistake for
Julia's audience.

**Decision 13. Science is zero-based.** Half-open ranges, `0..n`, `a[0]` first.

### 4.2 The argument is the membrane, not the aesthetics

The usual arguments — pointer arithmetic, `a[i]` as an offset, Dijkstra's note on
half-open intervals — are all true and none of them would be decisive on their
own. This one is.

`python-interop.md` builds the entire adoption strategy on one sentence: "a person
with a working NumPy program can replace one function with a Science one in an
afternoon, keep everything else, and get the speed **without a copy**." The
mechanism is DLPack, and §8 of the core spec has already committed the tensor
layout to it as an ABI promise. §4.1 of the interop note spells out the stakes:
getting the protocol wrong "is how zero-copy becomes silent corruption."

Now put one-based indexing on the Science side of that membrane.

The two languages are looking at **the same bytes**. There is no marshalling step,
no conversion function, no boundary object where an index could be adjusted —
that absence *is* the feature. A NumPy array and a Science tensor over the same
buffer would disagree about which element is `[0]`, forever, and the disagreement
would be:

- **Silent.** Both indices are in range. Nothing panics, nothing type-errors,
  nothing logs. You get element `k-1` where you wanted element `k`.
- **Undetectable by the compiler.** The index that is wrong was computed in
  Python, which Science cannot see into.
- **Undetectable in review.** The Python side reads correctly and the Science side
  reads correctly; only the composition is wrong.
- **Numerically plausible.** A spectrum shifted by one channel, a batch off by one
  row, a time series lagged by one sample. All of these produce results that look
  fine and are wrong, which is the exact failure mode §1 of the core spec says the
  language exists to prevent.

An off-by-one at a zero-copy membrane is a permanent silent bug generator, and
it would sit underneath the one feature the adoption strategy depends on.

### 4.3 The second argument, which is internal

The spec has already chosen, and choosing otherwise now would make it incoherent.

§4.5 gives `0..n` half-open and calls it "the counting loop." Every one-based
language pairs one-based indexing with **inclusive** ranges — Julia's `1:n`,
MATLAB's `1:n`, Fortran's `1,n` — because that is the only combination that
reads. Half-open ranges plus one-based indexing means `for i in 0..n` visits
`0..n-1` and `a[0]` does not exist, so every loop in the language is written
`for i in 1..=n`. Every corpus file, every note's worked example, and
`python-interop.md` §8.1's `for r in 0..batch.extent(0):` would be wrong.

The language has been zero-based since §4.5 was written. This note is recording
it, not deciding it.

### 4.4 What it costs, stated

**It costs the computational-physics audience, and the cost is not trivial.**

A physicist transcribing an algorithm from a paper is reading one-based
mathematics. Matrix entries are a-one-one. Summations run from 1 to n. Quadrature
nodes, band indices, spherical-harmonic orders, Fortran library documentation —
all one-based. Every transcription gets a `-1` sprinkled through it, and each `-1`
is a place a bug can hide. For someone porting a decades-old Fortran kernel, this
is the single most irritating property of the language, and telling them about
pointer arithmetic will not help.

There is **no cheap mitigation and this note does not pretend otherwise.**
Offset-configurable arrays (Julia's `OffsetArrays`) are a per-type escape hatch
that fragments every library that touches them, and would make the index
obligation of §1.3 unprovable in general. Refused.

What is on offer is smaller and honest:

- `0..n` reads as "n elements", which is the half-open compensation and is a real
  readability win in loop bounds — `0..n` and `n..m` tile without arithmetic.
- The index obligation machinery (§1.3) means a transcription error that walks off
  the end is caught at compile time in the loop and narrowing cases, rather than
  producing a wrong number. That is a partial answer to the specific failure this
  audience will hit.
- `for x in xs:` and the chain vocabulary remove most indices from most code, and
  an index that is not written cannot be off by one.

**Who each decision serves, explicitly.** Zero-based serves the NumPy/PyTorch
audience and the adoption strategy, at direct cost to the Fortran/MATLAB/Julia
audience. The two conflict, there is no compromise that is not worse than either
side, and this note chooses the side the adoption strategy is built on.

### 4.5 Negative indices are rejected

**Decision 14.** `a[-1]` is an error, not the last element.

Python's negative indexing is loved, and it is a trap that this language in
particular cannot afford. `a[i - 1]` inside a loop is correct at every `i` except
`0`, where Python silently reads the *last* element instead of failing. That is a
wrong answer with no diagnostic — precisely the class of bug §1 of the core spec
is written against. It also makes the lower half of every index obligation
(`0 <= i`) undischargeable in principle, since a negative index would be legal.

The replacements: `a.last()`, which `collections-and-chains.md` §1.4 already has
as a terminal and which `Array` should carry as an inherent method, and
`a[a.length() - n]` when the offset is not one.

A **literal** negative index gets its own diagnostic (`SC0152`) with both as
applicable fixes, because the Python muscle memory is strong enough that the
error will be common and the fix is mechanical. This is the same play
`llm-ergonomics.md` makes with its `SC0120`–`SC0134` block: catch the habit from
the other language and name the local form.

---

## 5. Comprehensions, rejected

### 5.1 The case for them, made fairly

```python
[x * 2 for x in xs if x > 0]
```

This is one of the best-loved constructs in Python and the argument for it is
strong for short cases. It is shorter than the chain. It is a single expression
with no intermediate names. It maps directly onto set-builder notation, which is
the notation this audience writes in papers, and that is a genuinely good reason
that the usual "Python users like it" framing undersells.

The Science equivalent is longer:

```science
let doubled be xs.iterate().keep(each > 0).map(each * 2).collect()
```

Twelve tokens against nine, and two of the extra ones (`iterate`, `collect`) are
bookkeeping. Anyone who says the chain is obviously better for this case is
selling something.

### 5.2 The case against, which wins

**Decision 15. No comprehensions. One form per operation.**

**The decisive argument is `syntax-revision-2.md` §1, one section old.** That
revision's single best change was deleting the second spelling of every
comparison, and its reasoning was not that `is at least` read badly. It was that
two spellings of one operation are *permanently un-normalisable* — a formatter
cannot canonicalise between them without running after type checking, "and a
formatter that must run after type checking is not a formatter" — so for a
language whose code is mostly machine-written, two spellings mean permanent
drift.

Every word of that applies here, and more strongly. A comprehension and a chain
are two spellings of map-and-filter. `sciencec fmt` could not canonicalise between
them, because the choice would depend on length and taste. Generated code would
drift between the two within a release. Reviewers would argue about which to use
in code review, forever, in a language whose own design note just spent a section
removing exactly that argument.

Adding comprehensions one section after deleting `a is at least b` would be
incoherent, and this note is not going to be the one that does it.

Three supporting reasons, in descending order of weight:

1. **A comprehension is eager; the chain is lazy.** `collections-and-chains.md`
   §2 spends its entire budget on "adapters are lazy, terminals are eager, there
   is no eager mode." A comprehension materialises an `Array` unconditionally.
   The beloved form would be the one that always allocates, which is the exact
   opposite of what §2 bought.
2. **It does not scale past the short case.** Beyond one map and one filter,
   Python programmers abandon comprehensions themselves — nested comprehensions
   are a documented readability cliff, and anything with a `sorted`, a `zip` or a
   group-by falls out of the form entirely. So the construct would cover the
   easiest third of cases and hand back the rest, which is the worst shape for a
   second spelling to have.
3. **It reads backwards.** The output expression comes first and the source
   second, which is why comprehensions do not compose with each other while chains
   compose trivially. Science's whole surface is built around the chain reading
   top to bottom (§4.6).

### 5.3 What is offered instead

A diagnostic, not silence. `SC0153` fires on a comprehension-shaped bracket —
`[` … `for` — and rewrites it:

```
error[SC0153]: Science has no comprehensions; write the chain
  --> model.science:12:18
   |
12 |     let scaled be [x * 2 for x in xs if x > 0]
   |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
help: the chain does the same thing, lazily
   |     let scaled be xs.iterate().keep(each > 0).map(each * 2).collect()
```

The rewrite is mechanical for the one-source, one-filter, one-expression case,
which is the case people write. This is `llm-ergonomics.md`'s thesis applied
again: the fix *is* the teaching material, and it is also the training data.

**If this is ever reconsidered**, it must come back as pure sugar that desugars
to the chain and introduces no semantics of its own — the same discipline
`syntax-revision-2.md` §8.5 applied to inline foreign code, and for the same
reason. Sugar over a mechanism that works is cheap and reversible. A second
mechanism is neither.

---

## 6. Precedence and grammar

### 6.1 Where `[` sits

§4.6's precedence table already has the level, top row:

```
call, index, field access, ?
```

(`try` leaves that row under `syntax-revision-2.md` §3; the postfix `?` presence
test takes its place.) The row needs an operand rule, which it has never had:

> `[` in **postfix** position — immediately after a complete primary expression —
> is an index. It binds as tightly as a call and a field access, and associates
> left, so `a[i].field[j]` groups as `((a[i]).field)[j]`.

### 6.2 Telling an index from a literal

**Decision 16.** The rule is parser position, and nothing else:

> **`[` in prefix position opens an array literal. `[` in postfix position is an
> index.**

In a Pratt parser this is not a rule so much as a consequence — it is the null
denotation versus the left denotation, and `(` already works exactly this way for
grouping versus call. No new machinery, no lookahead, no backtracking.

| Expression | `[` follows | Position | Reading |
|---|---|---|---|
| `a[1]` | `a`, a complete primary | postfix | index |
| `f([1, 2])` | `(` | prefix | literal |
| `[[1, 0], [0, 1]]` | nothing / `,` | prefix | literal both times |
| `[1, 2][0]` | `]`, a complete primary | postfix | literal, then index |
| `m[i, j][k]` | `]` | postfix | index of arity 2, then index |
| `a[1..5][0]` | `]` | postfix | slice, then index |

**Whitespace is not load-bearing.** `a [1]` is an index, the same as `a[1]`. This
is forced by consistency: §4.3 already refused to make the space in
`Array of Doc .new()` significant, on the grounds that "making whitespace
load-bearing is worse" than the ugly parenthesised form. Making it significant
here and nowhere else would be the worst of both.

**The one hazard, checked and dismissed.** A `[` at the start of a line could in
principle attach to the previous line's expression. It cannot here:

- Outside brackets, a newline ends a statement (§4.6), and only a leading `.` or
  `where` continues a line. A `[` on a fresh line therefore starts a new statement
  and is prefix.
- Inside brackets, newlines are ignored (§4.2), so `f(a` / `[1])` reads `a[1]` —
  which is the only sensible reading anyway, since without a comma it was never
  two arguments.

JavaScript's equivalent hazard comes from automatic semicolon insertion, which
Science does not have. Nothing to do.

### 6.3 What revision 2 freed

This decision was **not** available in the original design, and that is worth
recording because it is the reason the bracket is clean now.

§4.3 of the core spec moved generic arguments from `Array[Doc]` to `Array of Doc`.
That means `[` has **no role in type position at all**. In the older design a
parser meeting `Name[` had to decide between a generic instantiation and an index,
with the answer depending on whether `Name` resolved to a type or a value — a
resolution question inside the parser, which is where languages with this problem
end up needing a turbofish. Science does not have that problem, because `of` took
the job.

**And the Rust-ism diagnostics survive untouched.** `llm-ergonomics.md` allocates
`SC0125` for `Name[T]` in type position and `SC0126` for `name[T](…)` in
declaration position. Neither is disturbed: type position has no expressions, so a
`[` there is unambiguously the mistake those codes describe. Making `[` mean
indexing in expression position does not weaken either one.

---

## 7. Diagnostics

Codes are proposals in the ranges §9 defines. Two fresh blocks are claimed:
`SC0150`–`SC0159` in syntax (checked free against `SC0100`–`SC0104`, `SC0109`,
`SC0115`–`SC0116`, `SC0120`–`SC0134`, `SC0140`, `SC0142`), and `SC0280`–`SC0289`
in types and traits (checked free against `SC0250`–`SC0251`, `SC0262`–`SC0264`,
`SC0271`–`SC0273`).

### 7.1 Syntax — `SC0150`–`SC0159`

| Code | Condition | Severity | Applicable fix |
|---|---|---|---|
| `SC0150` | An open-ended range outside an index bracket: `let r be 2..` | error | Supply the endpoint. Secondary: the rule is stated at the span |
| `SC0151` | An empty index bracket: `a[]` | error | `a[..]` for the whole thing, or supply an index |
| `SC0152` | A literal negative index: `a[-1]` | error | `a.last()`; or `a[a.length() - 1]`. Both offered |
| `SC0153` | A comprehension: `[` … `for` … `]` | error | The chain rewrite (§5.3), mechanical for one source and one filter |
| `SC0154` | A range with literal bounds where start exceeds end: `a[5..1]` | error | No fix offered — the compiler cannot know which bound is wrong |

`SC0152` and `SC0153` are detected on the AST, before types, because neither fix
needs a type. That matters for `SC0153` in particular: a comprehension does not
type-check as anything, so it must be caught in the parser or the error will be a
cascade.

### 7.2 Types and traits — `SC0280`–`SC0289`

| Code | Condition | Severity | Applicable fix |
|---|---|---|---|
| `SC0280` | Indexing a value whose type implements neither `Index` nor `IndexMutably`. Names the type | error | `.get(i)` if the type has one; `.iterate()` if it is a chain source; otherwise none |
| `SC0281` | Elements of an array literal disagree in type. Primary on the first element that differs, secondary on the element that fixed the type | error | Where one side is an unsuffixed numeric literal: the suffix or the `as`. Otherwise none |
| `SC0282` | `[]` with no expected type | error | The annotation, naming a candidate type when exactly one is visible |
| `SC0283` | Index arity does not match rank: `m[i, j]` on an `Array`, or `m[i, j, k]` on a rank-2. Names both numbers | error | `a[i][j]` where the type is nested; otherwise none |
| `SC0284` | `Tensor.from` on a literal whose shape does not match, including a ragged one. Names both shapes and both spans | error | When the literal is rectangular and the annotation is wrong, the corrected annotation |
| `SC0285` | A slice whose bounds are **provably** out of range: `a[1..5]` where the extent is a constant below 5 | error | None. Secondary span points at where the extent was fixed |
| `SC0286` | An index obligation could not be discharged (§1.3) | **warning**; error under `--verified` | The guard `if i < a.length():`, and `.get(i)`. Must name the fact it could not establish |
| `SC0287` | `IndexMutably` required but not implemented — assigning through a read-only container | error | None; names the missing implementation |

`SC0286` is the one that decides whether any of this was worth doing, and its
message quality is not a polish item. §10.3's UI tests should cover it first and
hardest.

### 7.3 Ownership

**No new codes.** Every ownership failure a slice can produce is already covered:

| Failure | Code | Source |
|---|---|---|
| The source is mutated or moved while a view is live | `SC0331`, **generalised from "chain" to "view"** | `collections-and-chains.md` §9 |
| A slice outlives its source | `SC0302` | §6.1 rule 5 |
| Use after move of a sliced source | `SC0301` | §6.1 rule 3 |

Resisting a new ownership code here is deliberate. Region diagnostics are the part
of the compiler §6.2 says must be built with provenance from the start, and adding
message variants for a type that is structurally an ordinary borrow would be
duplicating the hard part of the renderer.

---

## 8. What this note asks of others

Ordered by how much is blocked behind each.

1. **A spec revision to §4.5 and §4.6.** §4.5 gains the index and slice grammar,
   the index-position list, and the open-ended range rule of §2.2. §4.6's
   precedence table gains the operand rule for the `index` level (§6.1) and
   finally places `..`, which is currently specified only in a parser comment.
2. **§5.4: `Index` gains an index-type parameter and an `Output`, and
   `IndexMutably` joins the trait list.** `Index` as listed today — a bare name
   with no parameter — cannot express slicing, because the index type varies
   between `Int` and `Range`. This is the largest type-system ask and everything
   in §1 and §2 sits on it.
3. **Flow-sensitive narrowing must extend from nullability to one integer
   relation.** `syntax-revision-2.md` §9 item 2 asks for narrowing for `T?`. The
   **Narrowed** rule of §1.3 needs the same analysis to carry `i < a.length()`
   into the branch. This is a genuine widening of that ask and it should be
   costed as one, not smuggled in: it is the difference between tracking "is this
   binding null" and tracking a relation between two values.
4. **`collections-and-chains.md` §5.3** — amend the slice refusal to "refused *for
   the chain API*", with a pointer here. And **generalise `SC0331` from "chain"
   to "view"** (§7.3), so one message shape covers both.
5. **§8's type list gains `Slice of T` and `MutableSlice of T`.** §8 declares the
   library closed, so this is a spec change and is stated as one. It is two types,
   both two words wide, both already implemented under another name at the FFI
   boundary.
6. **`ffi-c-boundary.md` §1.3** — re-derive `ffi.Span of T` as the ABI view of
   `Slice of T` rather than as an independent type. The existing coercion rule
   (a `borrowed Array of T` argument coerces to `ffi.Span of T` at an extern call
   site) should extend to a slice expression, so `dgemm(..., a[0..k], ...)` works
   with no `.span()`. That note's §1.3 already accepted the coercion and its
   reasoning is unchanged.
7. **§6.5 should point at the syntax it promises.** It advertises in-place
   `x[i] = v` as what ownership buys and the spec has no such expression. §1.1
   supplies `a[i] be v`; §6.5 should name it.
8. **`data-io.md` §11.6 is discharged.** Its builders should take lists
   (`null_values([...])`), and the item should be struck from its §11.
9. **A decision on `--verified`.** §1.3's fallback rests on `SC0286` being an
   error under a build profile a cautious project can turn on. If no such profile
   is planned, say so, because then `SC0286` is a warning forever and §1.3's
   argument is weaker than it reads.
10. **F1's shape note, when it is written, should build the obligation discharger
    once.** The **Constant** and **Shape equality** rules of §1.3 are the shape
    checker doing its ordinary job. Building a separate bounds analysis beside a
    shape unifier would be building the same thing twice.
11. **`python-interop.md` should record zero-based indexing as load-bearing for
    §4**, not as a syntax fact settled elsewhere. §4.2 here is an argument that
    note owns the premise of.

---

## 9. Risks

**The obligation discharger is a range analysis, and range analyses do not stay
small.** This is the largest risk in the note by a distance. §1.3 fixes the
discharger as a **closed list of four rules** for exactly this reason, and that
boundary must be defended in review. The pressure will be constant and reasonable:
someone will want `a[i + 1]` inside `for i in 0..n - 1`, then `a[2 * i]`, then a
symbolic comparison, and each step is individually justifiable. Three steps down
that path is a refinement-type checker with an SMT solver in the build, which
nobody has budgeted and which makes compile times unpredictable. **If the four
rules are not enough, the answer is to add a fifth by an explicit decision, not to
generalise the mechanism.**

**`SC0286` could be noisy enough that people turn it off.** If the four rules miss
the common cases, every real program emits a wall of warnings, the first thing
every project does is silence the code, and Science is Rust with extra steps and a
disabled lint. The cheap test exists and should be run before this ships: take the
corpus and `data-io.md`'s worked example, write them with brackets, and count how
many indices discharge. If it is not most of them, the rule list is wrong and the
feature should wait.

**Slices land in the least-proven part of the compiler.** A slice is the first
type whose value is a borrow with a length, and it exercises region inference in a
way nothing in F0 does today — a borrow stored in a value, flowing through
returns, held across calls. §6.2 warns that "a solver written without provenance
must be rewritten entirely to gain it." Slices are the feature that will find out
whether the provenance chain was really built.

**Zero-based is irreversible.** Every other decision in this note could be revised
in a later version with a mechanical migration. This one cannot: there is no
formatter rewrite, no diagnostic, and no fix-up that turns one-based source into
zero-based source, because the compiler cannot tell an intentional `a[1]` from a
transcribed one. If the audience that matters turns out to be the Fortran
transcribers rather than the NumPy users, this is the decision that was wrong and
cannot be fixed.

**The bracket gets a second meaning in a language that just spent a revision
removing second meanings.** `syntax-revision-2.md` deleted nine reserved words
and one entire duplicate operator set on the principle of one spelling per
operation. This note adds a character that means two things. The defence is that
the two meanings are distinguished by parser position rather than by type or
context — the same way `(` already means two things and nobody notices — and that
§5 refuses the third meaning that would have been genuinely ambiguous. It is still
motion in the opposite direction and it deserves to be seen as such.

**The ergonomic path to a matrix is the slow path unless F1 intervenes.**
`Tensor.from([[...], [...]])` allocates one `Array` per row and discards all of
them (§3.5). Every tutorial will use it, every benchmark that builds a matrix
this way will look bad, and the fix — lowering a literal argument directly into
the tensor buffer — is a special case in the F1 constructor that has to be written
deliberately. It will not happen by itself.

**Open questions left open.**

- Whether a `Slice of T` may be stored in a record field. §4.4 permits `borrowed
  Doc` in a field, so the machinery exists and the answer is probably yes — but
  every such field makes the enclosing type carry a region, and no note has yet
  measured how far that propagates through a real program.
- Whether `a[i]` should auto-borrow the way §6.3 auto-borrows at call sites. It
  reads as though it must, since `a[i].method()` should work on a `Slice`, but
  the interaction between auto-borrow and an indexed place expression on the left
  of a `be` has not been worked through here.
- Whether `Map` gets brackets. `m["key"]` is what every Python user will type,
  and a `Map` lookup that panics on a missing key is a much worse trade than an
  array index that panics out of range, because a missing key is a *normal*
  outcome and an out-of-range index rarely is. The inclination is no —
  `map.get(key)` returning `V?` only — but this note does not decide it, because
  the argument belongs next to `Map`'s design and not next to `Array`'s.
