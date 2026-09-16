# Science — F0 Design: the language core

Date: 2026-09-16
Status: approved
Supersedes: the Link F0 design of the same date. This document records the same
project after three decisions: the language is oriented to science and machine
learning, it is named Science, and its syntax is written in English words rather
than symbols and abbreviations.

Scope: phase F0. Later phases are sketched in §2 only far enough to keep F0
honest about what it must not preclude.

---

## 1. What Science is

Science is a compiled programming language for scientific computing and machine
learning. It compiles to native binaries through LLVM. Memory is managed with
compile-time verified ownership and borrows, with no garbage collector.

The bet is **verification**: the mistakes a compiler can catch before a run
starts. Julia has the REPL and the ecosystem and cannot check a shape, a unit,
or a reused random key before running. Mojo has ownership and GPUs and no shapes
in its types. JAX has the right transformation model and no static types. None
of them has all of static shapes, an effect discipline, ownership-tracked device
memory, an interactive tier, and zero-copy Python interop. That combination is
the gap Science aims at, and if Science is not measurably better than Julia and
Mojo at catching mistakes before the run starts, it loses to both on ecosystem.

- Files: `.science`
- Compiler: `sciencec`
- Keywords are English words. Identifiers are in whatever language the author
  prefers.

## 2. The phases

Reordered from the original plan. Tensors were last and are now early, because a
language named Science whose tensor support arrives last is indefensible. The
cost is stated plainly: agents, the original thesis, are demonstrated two phases
later than first planned.

| Phase | Contents | Depends on |
|---|---|---|
| **F0** | Core: lexis, types, generics, traits, operator traits, ranges, associated types, const generics, ownership with regions, LLVM codegen | — |
| **F1** | Tensors with typed shapes; the interactive tier (script mode, JIT, notebooks) | F0 |
| **F2** | Automatic differentiation; data parallelism and GPU targets; Python extension modules | F1 |
| **F3** | Actors: `spawn`, `send`, `receive`, supervision | F0 |
| **F4** | `agent`, `tool`, `prompt` as language constructs | F3 |
| **F5** | Durability: agent bodies lowered to serializable state machines | F4 |

**What F0 must not preclude.** Three constraints, each from a later phase:

- **F1 needs const generics and integer generic arguments.** `Tensor of (F32,
  (batch, 768))` has `768` as an argument, and `768` is not a type. This is why
  const generics are in F0 rather than deferred: retrofitting them means
  reopening the parser, the resolver, and every snapshot.
- **F2 needs closures capturing by reference, and higher-order functions.**
  `grad`, `map`, and parallel loop bodies are all closures over borrowed state.
- **F5 needs the region engine to answer `borrows_live_at(point)`** as a public
  query, so that no borrow survives a suspension point and an agent's captured
  state is serializable.

## 3. Design decisions

| Decision | Alternatives rejected | Reason |
|---|---|---|
| Native AOT via LLVM, plus a JIT tier in F1 | VM, transpilation, interpreter only | Self-contained binaries, and a REPL over the same query database. AOT alone was the one genuine mismatch with a scientific audience. |
| Ownership and borrows, no GC | Per-actor GC, ARC, global GC | Device memory freed at a program point the compiler can name; in-place updates proven safe rather than guessed; random keys that cannot be reused. See §6.5. |
| Full region inference | Explicit lifetimes; second-class borrows | No annotations. Cost: interprocedural analysis and diagnostics that reconstruct the compiler's reasoning. |
| Auto-borrow at call sites | Explicit `&` at every call | A parameter declared borrowed is borrowed automatically. Rust rejected this; Mojo adopted it; for this audience Mojo is right. §6.3. |
| English keywords, no symbol abbreviations | Symbol-dense syntax | Readability for an audience that is not primarily made of systems programmers. Held deliberately short of full natural language: see §4.1. |
| Indentation-delimited blocks | Braces | Same reason, and affinity with the Python-shaped audience. |
| Monomorphization | Uniform representation | Numeric specialization. It is not what obstructs a REPL; AOT-only was. |
| Query architecture (`salsa`) | Batch compilation | Incremental recompilation, and the mechanism the interactive tier is built from: re-running a cell is invalidating a query. |

## 4. Syntax

### 4.1 How far the English goes, and why it stops there

Keywords are English words, and no keyword is an abbreviation or a symbol where
a word would do. `function`, not `fn`. `returns`, not `->`. `for each`, not
`for`.

The structure is unchanged: calls use parentheses, operators stay symbols, and
blocks are indented. Science deliberately does **not** go further into natural
language. COBOL, AppleScript, and Inform 7 all found the same failure: when code
reads like English, readers assume it behaves like English, and it fails exactly
where a symbol would have promised nothing. `the third word of the file` is
lovely until you need to know what counts as a word. Science takes the
readability and stops before the trap.

### 4.2 Lexical structure

**Encoding.** UTF-8. Identifiers admit Unicode letters.

**Indentation.** Blocks are delimited by indentation; the lexer emits synthetic
`INDENT` and `DEDENT` tokens.

- Spaces only. A tab in the indentation is an error (`SC0003`), with no attempt
  to interpret it.
- A stack of levels: greater indentation pushes and emits `INDENT`, smaller pops
  and emits one `DEDENT` per level closed. Indentation matching no open level is
  an error (`SC0004`).
- Blank lines and comment-only lines take no part.
- Inside unclosed parentheses or brackets, newlines and indentation are ignored
  entirely.

**Comments.** `#` to end of line.

**Literals.**

- Integers: `42`, `1_000_000`, `0xFF`, `0b1010`, `0o777`, with optional suffix
  (`42i32`).
- Floats: `3.14`, `1e-9`, `2.5f32`.
- Strings: `"hello"`, escapes `\n \t \r \\ \" \' \0 \u{1F600}`. A string literal
  has type `String`.
- Characters: `'a'`. Booleans: `true`, `false`. Unit: `()`.

### 4.3 The keyword table

Every symbol or abbreviation that had a natural English word now has it.

| Concept | Science | Was |
|---|---|---|
| Function | `function` | `fn` |
| Return type | `returns` | `->` |
| Return early | `return` | `return` |
| Binding | `let x be v` | `let x = v` |
| Mutable binding | `let mutable x be v` | `let mut x = v` |
| Record type | `type` | `struct` |
| Sum type | `choice` | `enum` |
| Interface | `trait` | `trait` |
| Implementation | `Doc implements Summarize` | `impl Summarize for Doc` |
| Inherent methods | `Doc has methods:` | `impl Doc` |
| Generic arguments | `Array of Doc`, `Map of (String, Int)` | `Array[Doc]` |
| Generic parameters | `function largest of T ...` | `fn largest[T]` |
| Shared borrow | `borrowed T` | `&T` |
| Exclusive borrow | `mutable borrowed T` | `&mut T` |
| Dynamic dispatch | `any Summarize` | `dyn Summarize` |
| Iteration | `for each x in xs:` | `for x in xs` |
| Error propagation | `try f()` | `f()?` |
| Visibility | `public` | `pub` |
| Equality | `is` / `==` | `==` |
| Inequality | `is not` / `!=` | `!=` |
| Ordering | `is at least` / `>=`, and so on | `>= <= > <` |
| Closure | `each.title`, or `doc giving doc.title` | — |
| Compile-time constant | `const` | — |
| Type alias | `type Vec3 is Tensor of ...` | — |

Unchanged because they are already English: `if`, `else`, `match`, `while`,
`loop`, `break`, `continue`, `in`, `use`, `where`, `as`, `self`, `Self`, `and`,
`or`, `not`, `true`, `false`, `is`.

**Multi-word keywords.** `for each`, `let ... be`, `mutable borrowed`, and
`has methods` are sequences of reserved words, not new tokens. The lexer emits
each word separately and the parser recognizes the sequence, so nothing in the
lexer needs to know about phrases.

**Why generic arguments use parentheses when there is more than one.**
`Array of Doc` reads well and parses: a single type argument cannot contain a
top-level comma, so the comma that follows ends it. `Map of String, Int` in a
parameter list does **not** parse — that `Int` could be `Map`'s second argument
or the function's second parameter. Several arguments therefore take
parentheses: `Map of (String, Int)`. Nesting follows the same rule:
`Array of (Map of (String, Int))`.

### 4.4 Declarations

```science
type Doc:
    title: String
    body: String

type View:
    source: borrowed Doc          # the region is inferred

choice Option of T:
    Some(T)
    None

choice Result of (T, E):
    Ok(T)
    Err(E)
```

Construction takes named arguments in any order: `let d be Doc(title: "a",
body: "...")`.

```science
trait Summarize:
    function summarize(self) returns String

    function preview(self) returns String:
        truncate(self.summarize(), 80)

Doc implements Summarize:
    function summarize(self) returns String:
        truncate(self.body, 200)
```

**Generic functions and bounds.**

```science
function largest of T(items: borrowed Array of T) returns borrowed T
        where T: Ord:
    let mutable best be items.get(0)
    for each item in items:
        if item > best: best be item
    best
```

Bounds may be written inline (`of T: Ord`) or in a `where` clause. Multiple
bounds join with `+`.

**Inherent methods and associated functions.** A function with no `self` is an
associated function, called through the type.

```science
Doc has methods:
    function new(title: String) returns Doc:
        Doc(title: title, body: "")

    function is_empty(self) returns Bool:
        self.body.len() == 0

let d be Doc.new("a")
let docs be Array of Doc .new()
```

**Marker traits** are implemented on one line, since there is nothing to indent:
`Point implements Copy`.

**Constants and type aliases.**

```science
const WIDTH be 768
type Embedding is Array of F32
```

**Modules.** A file is a module; a directory with `mod.science` is a module
containing its siblings. `use text.parser (Token, lex)`.

### 4.5 Expressions and control flow

`if`/`else` is an expression. `match` is exhaustive; non-exhaustiveness is an
error listing the missing patterns.

```science
match result:
    Ok(value): value
    Err(e): panic(e)
```

A block is introduced with `:` and an indented body, or written inline as a
single expression on one line. The inline body ends **where the expression
ends**, not at the newline: in `if a: b else: c` the then-branch stops at `else`
because `else` cannot continue an expression. Fitting on one line is a
consequence of that rule, not the rule.

**Dangling `else` binds to the innermost `if`.**

An inline body may be an expression, an assignment, or `return`, `break` or
`continue`. It may not be a `let`, which binds a name nothing could then use
(`SC0109`).

**Ranges.** `0..n` is half-open, `0..=n` inclusive. `for each i in 0..n:` is the
counting loop. Ranges are written with `..` and not with `:`, because `:` already
opens blocks, separates match arms, fields, named arguments, and bounds.

**Error propagation.** `try` unwraps `Ok`/`Some` or returns the failure.

```science
function read_config(path: borrowed String) returns Result of (Config, Error):
    let text be try read_file(path)
    let config be try parse(text)
    Ok(config)
```

### 4.6 Chains, and the comparison phrases

The shape most Science code takes is a chain of method calls, so the language is
built to make that read well.

```science
let headlines be docs
    .iterate()
    .discard(each.is_empty())
    .map(each.title)
    .take(5)
    .collect()
```

**Calls always take parentheses**, including when they take no arguments:
`text.length()`, not `text.length`. The parenthesis-free form reads better in a
chain and was considered; it was rejected because once functions are values,
`doc.summary` is ambiguous between calling it and naming it, and a language
cannot resolve that by guessing. Parentheses mean call; their absence means a
field.

**A chain is broken across lines by indenting the continuation.** A line ending
inside an unclosed bracket already continues implicitly (§4.2); a line whose
continuation begins with `.` continues too. Nothing else may be split this way.

#### Closures

Two forms, because one is not enough.

```science
docs.map(each.title)                       # implicit subject
docs.map(doc giving doc.title)             # named parameter
docs.sort(by: doc giving doc.title.length())
```

`each` names the subject of the enclosing call without declaring it. It is the
shorter form and the one most chains want.

`name giving expression` declares the parameter. It is required whenever `each`
cannot work, and there is exactly one such case, which the compiler must report
clearly: **nesting**. In `outer.map(each.inner.map(each.x))` the two `each`
refer to different things and the inner one shadows the outer irrecoverably.
Science rejects a nested `each` (`SC0115`) rather than picking a rule, and the
error names the named form as the fix.

Having two spellings for one idea is design debt, recorded here as deliberate:
Kotlin and Swift carry the same debt and it has not hurt them.

#### Comparison phrases

Every comparison has a word form and a symbol form, and they are the same
operator.

| Phrase | Symbol |
|---|---|
| `a is b` | `a == b` |
| `a is not b` | `a != b` |
| `a is at least b` | `a >= b` |
| `a is at most b` | `a <= b` |
| `a is above b` | `a > b` |
| `a is below b` | `a < b` |

The phrases are for code that is read; the symbols are for dense formulas. Two
spellings is debt again, and the reason to accept it is the audience: a physicist
writing an inequality wants `>=`, and a chain reading `if name is not ""` wants
the words. Forcing either group into the other's notation costs more than the
duplication does.

The word forms are sequences of reserved words, resolved by the parser. `is`
followed by `not`, `at`, `above` or `below` is a comparison; `is` followed by
anything else is equality.

**Precedence**, highest to lowest. Assignment is absent because it is a
statement, not an operator.

```
call, index, field access, try
- not borrowed             (unary)
as
**                         (power, right-associative)
* / % @                    (@ is matrix multiply)
+ -
<< >>
&
^
|
== != < > <= >=          (and their phrase forms: is, is not,
                          is at least, is at most, is above, is below)
and
or
```

### 4.7 Smaller rules

- Trailing commas are allowed in every bracketed and parenthesized list.
- Variants may be qualified: `Some(x)` and `Option.Some(x)` are the same.
- Borrows auto-dereference for field access, method calls, and assignment. There
  is no dereference operator.
- `Never` is the type of an expression that does not return, such as `panic`. It
  coerces to any type, which is what lets a match arm panic.

## 5. Type system

### 5.1 Primitives

`I8 I16 I32 I64 U8 U16 U32 U64 F16 BF16 F32 F64 Bool Char String ()`

`Int` aliases `I64`, `Float` aliases `F64`. No implicit numeric conversion; `as`
is always written, including where it loses precision.

**Numeric semantics**, decided here because a numeric language cannot leave them
open:

- Integer overflow panics in debug builds and wraps in release, as Rust does.
  `wrapping_add` and friends are explicit.
- Integer division truncates toward zero, and `%` takes the sign of the dividend.
- `as` from float to integer saturates, and NaN becomes zero.
- `F32`/`F64` implement `Eq` but **not** `Ord`, because NaN has no total order.
  Sorting floats uses an explicit total order function.
- Reductions have a defined order unless marked `unordered`. Results that differ
  between runs are opt-in, because this is a language whose users publish.

### 5.2 Inference

Local. Signatures are fully annotated; inside a body everything is inferred by
unification. Trait bounds are checked after unification. Global Hindley-Milner is
rejected: it does not hold up with traits and regions, and its errors point far
from the fault.

### 5.3 Generics

By monomorphization. Generic parameters may be **types** or **constants**:

```science
type Matrix of (T, const ROWS: Int, const COLS: Int):
    data: Array of T
```

Const generics are in F0 because F1's shapes are built from them.

### 5.4 Traits

Statically resolved, with the orphan rule: an implementation is legal only if the
trait or the type belongs to the module declaring it.

**Associated types** are in F0. Without them `Iterate` cannot name its element
type and no generic container trait can be written.

```science
trait Iterate:
    type Item
    function next(mutable self) returns Option of Self.Item
```

**Operator traits.** `+ - * / % ** @` and comparison are trait methods, so user
types participate in arithmetic. A scientific language in which `+` does not work
on your own type is not a scientific language.

`Add Sub Mul Div Rem Pow MatMul Neg Index Eq Ord Copy Clone Drop Iterate From
Display`

`Eq` and `Ord` are what the comparison phrases of §4.6 dispatch to, so `is` and
`==` are the same trait method by construction, not by a parser rule that could
drift.

`Display` is what `print` requires, and it is the same trait the interactive
tier's notebook rendering will extend with MIME variants in F1.

### 5.5 Absence and failure

No `null`. Absence is `Option of T`, failure is `Result of (T, E)`. This matters
beyond ergonomics: in F4 a model's output is the archetypal "this may not match
what you asked for", and the language must arrive there with the shape already
established.

## 6. Ownership and regions

### 6.1 Rules

1. Every value has exactly one owner.
2. Assigning, passing, or returning moves ownership, unless the type is `Copy`.
3. Using a moved value is an error (`SC0301`).
4. A value may be borrowed shared any number of times, or exclusively once, never
   both at once.
5. No borrow outlives its referent.
6. Leaving scope destroys a value, running `Drop` if it has one.

There is no lifetime syntax. The programmer never writes a region.

### 6.2 Region inference

A region is the set of program points where a borrow is live — a control-flow
property, so the analysis needs a flow graph, which is why the pipeline has a
mid-level IR (§7).

Over each function's MIR: assign a fresh region variable to every borrow and
every reference in a type; collect outlives constraints from assignments, calls
and returns; propagate to a fixed point; then check for conflicts.

**Interprocedural.** A signature carries no annotations, so a function's
parameter and return regions are part of its analysis result. Functions are
analyzed in reverse topological order of the call graph, and mutual recursion is
resolved by a fixed point over each strongly connected component.

**Provenance is not optional.** With regions inferred rather than annotated, the
compiler has nothing of the user's to blame; an error must reconstruct where the
borrow was born, what keeps it alive, and where it conflicts. Every constraint
therefore carries its span and cause, and the solver preserves that chain. A
solver written without provenance must be rewritten entirely to gain it.

### 6.3 Auto-borrow at call sites

If a parameter is declared `borrowed T`, the caller writes `compare(a, b)`, not
`compare(borrowed a, borrowed b)`. The signature still says `borrowed`, so a
reader keeps the information; only the noise at the call site goes. The same rule
already applied to method receivers.

An explicit borrow remains legal where it clarifies.

### 6.4 The interface F5 needs

The region engine exposes `borrows_live_at(point)` as a public query, which F5
uses to reject any borrow live at a suspension point.

### 6.5 What ownership buys a scientific language

Recorded because it is the decision most likely to be questioned by the audience,
and the answer is not the one that motivated it originally.

- **Device memory.** Garbage collectors and GPUs are enemies: a GC does not know
  about device memory pressure, so a program runs out of GPU memory while host
  memory is fine. Julia's CUDA support carries this wound permanently. Ownership
  frees a device buffer at a point the compiler can name.
- **In-place updates without aliasing bugs.** Futhark's uniqueness types exist
  solely to let a pure language write `x[i] = v` in place safely; ownership is
  the same guarantee with a better surface. NumPy's silent view aliasing becomes
  a compile error.
- **Random keys that cannot be reused.** A generator is neither `Copy` nor
  `Clone`; splitting consumes it. Reusing a key — a silent, undetectable bug in
  JAX — is `SC0301`, use after move.
- **No pauses**, which matters for acquisition, control loops, and MPI ranks.

The cost is real and is paid at the surface: a large array assigned to a new name
moves, which will surprise every NumPy user once. Auto-borrow (§6.3) removes the
most visible part of it, and the borrow diagnostics carry the rest.

## 7. Compiler architecture

### 7.1 Pipeline

```
  .science
    |  lexer              INDENT/DEDENT; a span on every token
   tokens
    |  parser
    AST
    |  resolution         modules, scopes, names to DefIds
    HIR
    |  types + traits     inference, bounds, associated types
   THIR
    |  lowering           explicit places and temporaries, CFG
    MIR  <---- region inference + ownership checking
    |  monomorphization
    |  codegen            inkwell -> LLVM IR
    |  linking
  native binary
```

**A note for F1.** Tensor-typed code will need an array-level IR between THIR and
MIR. Julia fuses broadcasts syntactically because by the time its compiler could
fuse, it sees loops rather than array operations; Halide, Futhark, and XLA all
keep whole-array operations first-class until scheduling. Lowering tensor
operations straight to MIR loops makes fusion an LLVM loop-pass problem, and LLVM
is bad at it. F0 does not build that IR, but it must not make it impossible.

### 7.2 Crates

`science-diagnostics`, `science-lexer`, `science-parser`, `science-resolve`,
`science-types`, `science-mir`, `science-regions`, `science-codegen`,
`science-rt`, `science-db`, `science-testkit`, and `sciencec` as the driver.

### 7.3 Query architecture

Every phase is a memoized `salsa` query over a dependency graph. Changing a file
invalidates only what depended on it.

Two properties that do not come free and are therefore tested rather than
assumed: writing a file's existing text back must not start a revision, and a
query whose inputs are unchanged must not re-execute. Both are asserted on
execution counts, never on returned values.

The same database is what F1's interactive tier runs on: re-executing a notebook
cell is invalidating a query.

## 8. Runtime and standard library

`science-rt` is statically linked into every binary: allocation over the system
allocator, `panic` with a message and abort, and the representation of every
library type. No garbage collector, no reference counting, no threads in F0.

The F0 library is small and its method sets are **closed** — anything not listed
does not exist, and adding to the list is a spec change. It covers `Option`,
`Result`, `Box`, `String`, `Array`, `Map`, the traits of §5.4, and the free
functions `print`, `println`, `panic`, `read_file`, `write_file`.

**Tensor representation is decided in F0 even though tensors are F1**, because it
is an ABI commitment. The layout is DLPack-compatible: data pointer, device kind
and id, rank, dtype, shape, strides, byte offset. DLPack is what NumPy, PyTorch,
JAX, CuPy and MLX exchange; matching it makes zero-copy interchange a cast, and
missing it makes every exchange a copy forever.

## 9. Diagnostics

Every AST, HIR and MIR node carries its span. A diagnostic has a code
(`SC0142`), a severity, a message, a primary span, secondary spans, and where
applicable an automatically applicable suggestion.

| Range | Phase |
|---|---|
| `SC0001`–`SC0099` | Lexical |
| `SC0100`–`SC0199` | Syntax |
| `SC0200`–`SC0249` | Resolution |
| `SC0250`–`SC0299` | Types and traits |
| `SC0300`–`SC0399` | Ownership and regions |
| `SC0400`–`SC0499` | Codegen and linking |

Exhaustiveness errors belong to the type checker and so take a code in the
`SC0250` range, not the `SC0210` the previous spec assigned them.

## 10. Testing

Four layers, all from the first commit, and development is test-driven.

1. **Unit tests per crate.**
2. **Snapshot tests** over AST, HIR and MIR dumps, which catch regressions in
   intermediate phases no execution test observes.
3. **UI tests**: a program that must fail, beside its expected rendered output,
   compared literally. It is the only known method that stops error messages from
   decaying, and for a language with inferred regions it is not optional.
4. **Execution tests**: compile, run, compare output and exit code.

## 11. Definition of done

1. `sciencec` compiles to a native binary a program using, at once: generics with
   bounds, const generics, associated types, a trait dispatched both statically
   and dynamically, an exhaustive `match`, `Option` and `Result` with `try`, a
   user type implementing operator traits, a range-driven loop, a closure, and a
   type holding a borrow in a field.
2. Every ownership violation produces an error with the correct span and the
   chain of borrows that explains it, covered by UI tests.
3. A non-exhaustive `match` is rejected with the missing patterns listed.
4. Recompilation is incremental, proved by execution counts.
5. The full suite passes clean.

## 12. Out of scope for F0

- Tensors, shapes, automatic differentiation, GPU targets, the interactive tier,
  actors, agents, durability. Those are F1 through F5.
- Trait specialization; higher-kinded types.
- Units of measure. Wanted, and a separate unifier over a free abelian group;
  after shapes.
- Macros. Reserved as a word, not implemented.
- Language server, formatter, package manager. An environment-reproducibility
  story is the single most-cited reason scientists trust results across machines,
  and it is planned, not built.
- Cross-compilation.
- Custom optimizations; everything is delegated to LLVM.

## 13. Reserved words

Reserving costs nothing now and breaks every program using the name later.

**In use:** `function returns return let be mutable type choice trait implements
has methods of borrowed any for each in if else match while loop break continue
use where as self Self and or not true false is const public try giving at above
below most least`

The comparison phrases make `at`, `above`, `below`, `most` and `least` reserved,
and closures make `giving` reserved. `each` is reserved too: it is a binder, and
letting a program shadow it would make every chain ambiguous.

**Reserved, not yet used:** `agent tool prompt spawn send receive durable
checkpoint resume supervise async await tensor shape model extern unsafe pure
parallel on with yield assert move static macro union kernel import`

`shape`, `model` and `tensor` are reserved globally, which means `x.shape` and
`let model be ...` do not compile. This is a known cost, accepted deliberately so
that F1 and F4 syntax is guaranteed; the alternative considered was making them
contextual keywords recognized only in declaration position.

**Deliberately not reserved**, because each is a common variable name in the
target audience's code: `grad`, `dim`, `dims`, `axis`, `device`, `dtype`, `unit`,
`alias`. Note that `where` is reserved, so a user cannot define a function of
that name, which NumPy users will reach for.
