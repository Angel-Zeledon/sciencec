# Link — F0 Design: the language core

Date: 2026-09-16
Status: approved
Scope: phase F0 of five. This document defines **only** the language core.

---

## 1. What Link is

Link is a compiled programming language oriented toward agents and machine
learning models. It compiles to native binaries through LLVM. Memory is managed
with compile-time verified ownership and borrows, with no garbage collector.

What separates Link from a conventional systems language is not in F0: it is
agents, tools, and prompts as language constructs, durability as a
compiler-verified property, and tensors carrying their shape in the type system.
F0 builds the foundation those rest on.

- Files: `.link`
- Compiler: `linkc`
- Keywords in English; identifiers in whatever language the author prefers.

## 2. The five phases

| Phase | Contents | Depends on |
|---|---|---|
| **F0** | Core: lexis, types, generics, traits, ownership with regions, LLVM codegen | — |
| **F1** | Actors: `spawn`, `send`, `receive`, supervision, concurrent runtime | F0 (move semantics for zero-copy messages) |
| **F2** | `agent`, `tool`, `prompt`: provider FFI, model output checked against the declared type | F1 |
| **F3** | Durability: lowering agent bodies to state machines, serialization, resumption | F2 (defines where the suspension points are) |
| **F4** | `Tensor[f32, (batch, 768)]`: shape arithmetic at the type level | F0 |

**A constraint F0 must honor in advance.** F3 requires that no borrow stays
alive across a suspension point, so that an agent's captured state is
serializable. F0's region engine must be able to answer "which borrows are live
at this program point?" as a first-class query, not as an internal detail of the
algorithm. It is designed that way from the start.

## 3. Design decisions and their reasons

A record of the decisions taken, with the reason, so that revisiting one later
is an informed act.

| Decision | Alternatives rejected | Reason |
|---|---|---|
| Native AOT compilation via LLVM | Bytecode VM, transpilation, interpreter | Performance and self-contained binaries. Cost accepted: F3's durability requires lowering agent bodies into explicit state machines rather than getting it free from a VM's state. |
| Ownership and borrows, no GC | Per-actor GC, ARC, global GC | `send` moves the value: messages between agents with no copy and no aliasing. Cost accepted: it is the most expensive type system to implement. |
| Full region inference | Explicit lifetimes; second-class borrows | Maximum expressiveness with no annotations. Cost accepted: interprocedural analysis, and diagnostics that must reconstruct the compiler's own reasoning. |
| Complete foundations before agents | Thin vertical slice; middle path | The core ends up solid and needs no rewrite. Cost accepted: F0 is long, and the language's thesis is not demonstrated until F2. |
| Indentation-delimited blocks | Braces | Readability, and affinity with the ML and Python audience. |
| Generics in square brackets | Angle brackets | No parsing ambiguity against the comparison operators, hence no turbofish. And `Tensor[f32, (batch, 768)]` reads well in F4. |
| Query architecture (`salsa`) from F0 | Batch compilation | Incremental recompilation and a future language server without paying the migration that cost rustc years. Cost accepted: more ceremony in every phase. |
| Compiler written in Rust | C++, OCaml | The best living compiler libraries, `inkwell` for LLVM, and a mental model shared with what is being implemented. |

## 4. Syntax

### 4.1 Lexical structure

**Encoding.** UTF-8. Identifiers admit Unicode letters.

**Indentation.** Blocks are delimited by indentation. The lexer emits synthetic
`INDENT` and `DEDENT` tokens.

- Spaces only. A tab in the indentation is a lexical error (`LK0003`); no
  attempt is made to interpret it.
- A stack of levels is maintained. Indentation greater than the top emits an
  `INDENT` and pushes; smaller emits as many `DEDENT`s as levels close.
  Indentation matching no level on the stack is an error (`LK0004`).
- Blank lines, and lines containing only a comment, take no part in the
  indentation calculation.
- Inside unclosed parentheses or brackets, newlines and indentation are ignored
  entirely: line continuation is implicit.

**Comments.** `#` to end of line. There are no block comments.

**Literals.**

- Integers: `42`, `1_000_000`, `0xFF`, `0b1010`, `0o777`. Optional type suffix:
  `42i32`, `7u8`.
- Floats: `3.14`, `1e-9`, `2.5f32`.
- Strings: `"hello"`, with escapes `\n`, `\t`, `\r`, `\\`, `\"`, `\0`, `\u{1F600}`.
- Characters: `'a'`, `'\n'`.
- Booleans: `true`, `false`.
- Unit: `()`.

**Reserved words.** `fn`, `let`, `mut`, `if`, `else`, `match`, `for`, `in`,
`while`, `loop`, `return`, `break`, `continue`, `struct`, `enum`, `trait`,
`impl`, `use`, `mod`, `pub`, `true`, `false`, `self`, `Self`, `as`, `dyn`,
`where`, `and`, `or`, `not`.

Reserved from now although F0 does not use them, so that F1 through F4 need not
break compatibility: `agent`, `tool`, `prompt`, `spawn`, `send`, `receive`,
`durable`, `checkpoint`, `resume`, `supervise`, `async`, `await`, `tensor`,
`shape`, `model`.

### 4.2 Blocks

A block is introduced with `:` followed by a newline, `INDENT`, statements,
`DEDENT`.

There is also an **inline form** for single-expression constructs, valid only if
the whole thing fits on one line:

```link
if a.len() > b.len(): a else: b
```

### 4.3 Declarations

```link
fn longest(a: &String, b: &String) -> &String:
    if a.len() > b.len(): a else: b

fn main():
    let greeting = "hello"
    let mut count = 0
    count = count + 1
```

An omitted return type means unit `()`. A function's value is its last
expression; `return` exists for early exit.

`let` binds immutably; `let mut` allows reassignment. The type annotation is
optional on `let` and mandatory on `fn` parameters and return types.

**Structs.**

```link
struct Doc:
    title: String
    body: String

struct View:
    source: &Doc          # allowed: the region is inferred
```

Construction takes mandatory named arguments, in any order:

```link
let d = Doc(title: "a", body: "...")
```

**Enums.** Variants carry positional payloads. No struct-like variants in F0.

```link
enum Option[T]:
    Some(T)
    None

enum Result[T, E]:
    Ok(T)
    Err(E)
```

**Traits.** With default methods.

```link
trait Summarize:
    fn summarize(&self) -> String

    fn preview(&self) -> String:
        self.summarize().truncate(80)

impl Summarize for Doc:
    fn summarize(&self) -> String:
        self.body.truncate(200)
```

**Generics and bounds.**

```link
fn largest[T: Ord](items: &Array[T]) -> &T:
    let mut best = items.get(0)
    for item in items:
        if item > best: best = item
    best

struct Pair[A, B]:
    first: A
    second: B
```

Multiple bounds with `+`, and a `where` clause for long signatures:

```link
fn describe[T](x: &T) -> String where T: Summarize + Clone:
    x.summarize()
```

**Dynamic dispatch.** `dyn Trait` only behind an indirection: `&dyn Summarize`
or `Box[dyn Summarize]`. A trait is `dyn`-compatible if no method takes `Self`
by value or has generic parameters of its own.

**Modules.** A file is a module; a directory with a `mod.link` is a module
containing its siblings.

```link
use text.parser
use text.parser (Token, lex)
```

### 4.4 Expressions and control flow

`if`/`else` is an expression; both branches must have the same type, unless
there is no `else`, in which case the type is `()`.

`match` is exhaustive. Non-exhaustiveness is an error (`LK0210`) that lists the
missing patterns.

```link
match result:
    Ok(value): value
    Err(e): panic(e)

match point:
    (0, 0): "origin"
    (x, 0): "on the X axis"
    (_, _): "somewhere else"
```

Patterns: literals, the `_` wildcard, bindings, enum variants, structs, tuples,
and alternatives with `|`.

`for x in iterable:` walks anything implementing `Iterate`. `while cond:` and
`loop:` with `break` and `continue`.

**The `?` operator.** On `Result[T, E]` it unwraps `Ok` or returns `Err`,
propagating the error through `From`; on `Option[T]` it unwraps `Some` or
returns `None`.

```link
fn read_config(path: &String) -> Result[Config, Error]:
    let text = read_file(path)?
    let config = parse(&text)?
    Ok(config)
```

**Precedence**, highest to lowest:

```
call, index, field access, ?
- not & &mut          (unary)
as
* / %
+ -
<< >>
&
^
|
== != < > <= >=
and
or
=                     (assignment, non-associative)
```

## 5. Type system

### 5.1 Primitive types

`I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`, `F32`, `F64`, `Bool`,
`Char`, `String`, `()`.

`Int` aliases `I64`, `Float` aliases `F64`. Unsuffixed integer literals default
to `Int` unless context imposes another type. There are no implicit numeric
conversions: they are written with `as`, and `as` is explicit even when it loses
precision.

### 5.2 Inference

Local. Function signatures are fully annotated; inside a body everything is
inferred. The algorithm is unification with type variables, solved per function.
Trait bounds are checked after unification.

Global Hindley-Milner is rejected: it does not hold up in the presence of traits
or regions, and its errors point at locations arbitrarily far from the real
fault.

### 5.3 Generics

By monomorphization: each distinct instantiation generates specialized code.
Zero runtime cost, paid for in compile time and binary size. Dynamic dispatch
exists for when that trade is not worth it, and F2 will need it: an agent's list
of tools is heterogeneous.

### 5.4 Traits

Statically resolved. An `impl` is legal only if the trait or the type belongs to
the module declaring it (the orphan rule), which guarantees two conflicting
implementations cannot exist. No specialization and no associated types in F0;
they are added if F1 or F2 need them.

Library traits the compiler knows about: `Copy`, `Clone`, `Drop`, `Eq`, `Ord`,
`Iterate`, `From`.

### 5.5 Absence and failure

There is no `null`. Absence is `Option[T]`; failure is `Result[T, E]`. This
matters from F0 even though it looks premature: in F2 a model's output is the
archetypal case of "this may not match what you asked for", and the language
must arrive there with the right way to express it already established.

## 6. Ownership and regions

### 6.1 Rules

1. Every value has exactly one owner.
2. Assigning, passing, or returning a value **moves** ownership, unless its type
   implements `Copy`.
3. Using a moved value is an error (`LK0301`).
4. A value may be borrowed shared (`&T`) any number of times, or exclusively
   (`&mut T`) once, but never both at once.
5. No borrow may outlive its referent.
6. On leaving scope a value is destroyed; if it implements `Drop`, its
   destructor runs.

There is no lifetime syntax. The programmer never writes a region.

### 6.2 How regions are inferred

A region is a set of program points: those where one particular borrow is still
live. That is a control-flow property, so the analysis cannot run over a tree;
it needs an explicit flow graph. Hence the mid-level IR in the pipeline (§7).

The algorithm, over each function's MIR:

1. Assign a fresh region variable to every borrow and to every reference
   appearing in a type.
2. Collect outlives constraints (`'a: 'b`, "region a contains region b") from
   assignments, calls, and returns.
3. Propagate to a fixed point over the flow graph until no region grows.
4. Check for conflicts: any use of a borrowed place while the borrow is still
   live, and any borrow outliving its referent.

**Interprocedural.** Since a signature carries no annotations, the regions of a
function's parameters and return are part of its analysis result, not of its
declaration. Functions are therefore analyzed **in reverse topological order of
the call graph**: leaves first, then their callers. Cycles (mutual recursion)
are resolved by a fixed point over each strongly connected component, starting
from the most permissive approximation and tightening until it stabilizes.

**Regions in struct fields.** A struct with reference-typed fields receives
implicit region parameters, inferred in the same analysis and propagated to
every site instantiating the type.

### 6.3 Provenance, and why it is not optional

By inferring regions instead of reading them from an annotation, the compiler
leaves itself nothing of the user's to blame. An error cannot say "your
annotation is wrong" because there is no annotation. It has to reconstruct the
reasoning: where the borrow was born, what keeps it alive, and where it
conflicts.

Consequently, **every constraint collected in step 2 carries its provenance** —
the span and the cause that produced it — and the solver preserves that chain as
it propagates. A region error is explained by walking the chain of constraints
that led to the conflict.

This cannot be retrofitted: a solver written without provenance is a solver that
must be rewritten entirely to have it. It is built this way from the first
commit.

### 6.4 The interface F3 needs

The region engine exposes `borrows_live_at(point) -> Set[Borrow]` as a public
query. F3 will use it to reject any borrow live at a suspension point, which is
what makes an agent's state serializable.

## 7. Compiler architecture

### 7.1 Pipeline

```
  .link
    |  lexer              INDENT/DEDENT; a span on every token
   tokens
    |  parser
    AST                   faithful to source, a span on every node
    |  resolution         modules, scopes, imports
    HIR                   names resolved to unique ids
    |  types + traits     local inference, bounds checked
   THIR                   fully typed
    |  lowering           explicit places and temporaries, CFG
    MIR  <---- region inference + ownership checking
    |  monomorphization   one copy per instantiation
    |  codegen            inkwell -> LLVM IR
    |  linking
  native binary
```

### 7.2 Crates

| Crate | Responsibility |
|---|---|
| `link-diagnostics` | Spans, errors, rendering, suggestions. Everything depends on it. |
| `link-lexer` | Text to tokens, including the indentation stack. |
| `link-parser` | Tokens to AST. |
| `link-resolve` | AST to HIR: modules, scopes, name resolution. |
| `link-types` | HIR to THIR: inference, trait resolution, exhaustiveness. |
| `link-mir` | THIR to MIR: lowering to a flow graph. |
| `link-regions` | Region inference and ownership checking over MIR. |
| `link-codegen` | Monomorphization and LLVM IR emission via `inkwell`. |
| `link-rt` | Minimal runtime, statically linked into every binary. |
| `link-db` | The `salsa` query definitions and the compiler database. |
| `linkc` | Driver: command line, orchestration, linking. |

### 7.3 Query architecture

Each phase is expressed as memoized `salsa` queries over a dependency graph, not
as a function called once. Changing a file invalidates only the queries that
depended on it.

Principal queries:

```
source_text(FileId) -> String                    # input
tokens(FileId) -> Vec<Token>
ast(FileId) -> Ast
module_tree() -> ModuleTree
hir(ModuleId) -> Hir
signature(DefId) -> Signature
type_of(DefId) -> Type
thir(DefId) -> Thir
mir(DefId) -> Mir
call_graph() -> CallGraph
region_result(DefId) -> RegionResult              # needs the leaves first
mono_items() -> Vec<MonoItem>
llvm_module(CodegenUnit) -> LlvmIr
```

`region_result` is the only query depending on the call graph rather than on its
own inputs alone, for the reason given in §6.2. It is implemented by requesting
`region_result` for each callee, which lets `salsa` derive the right order by
itself; recursion is detected and resolved with the component's fixed point.

## 8. Runtime and standard library

`link-rt` is a minimal statically linked runtime: allocation over the system
allocator, `panic` with a message and abort, and the representations of the
library types. No garbage collector and no threads in F0.

The F0 standard library, deliberately small:

- `Option[T]`, `Result[T, E]`, `Box[T]`
- `String` (owned, UTF-8) and `&String` for views
- `Array[T]` (growable, contiguous), `Map[K, V]` (hash table)
- `print`, `println`
- `read_file`, `write_file`
- Traits: `Copy`, `Clone`, `Drop`, `Eq`, `Ord`, `Iterate`, `From`

Everything else is F1 or later.

## 9. Diagnostics

Every AST, HIR, and MIR node carries its span from the lexer, without exception.
A span dropped in an early phase is an error impossible to locate in a late one.

A diagnostic has a code (`LK0142`), a severity, a message, a primary span,
secondary spans with their labels, and, where applicable, an automatically
applicable suggestion.

Code ranges:

| Range | Phase |
|---|---|
| `LK0001`–`LK0099` | Lexical |
| `LK0100`–`LK0199` | Syntax |
| `LK0200`–`LK0299` | Resolution and types |
| `LK0300`–`LK0399` | Ownership and regions |
| `LK0400`–`LK0499` | Codegen and linking |

## 10. Testing strategy

Four layers, all from the first commit. Development is test-driven: the test
before the code.

1. **Unit tests per crate.** Each crate tests its own invariants.
2. **Snapshot tests** with `insta`, over AST, HIR, and MIR dumps. They catch
   regressions in intermediate phases that no execution test observes.
3. **UI tests** in the style of rustc: a `tests/ui/*.link` that must fail,
   alongside its expected `.stderr`, compared literally. It is the only known
   method that stops error messages from decaying over time. For a language with
   inferred regions it is not optional.
4. **Execution tests.** Compile to a binary, run it, compare stdout and exit
   code.

## 11. Definition of done

F0 is done when all of these hold:

1. `linkc` compiles to a native binary a program using, at once: generics with
   bounds, traits with both static and dynamic dispatch, an exhaustive `match`
   over enums, `Option` and `Result` with `?`, and a struct storing a borrow in
   a field.
2. Every ownership violation produces an error with the correct span and the
   chain of borrows explaining it, covered by UI tests.
3. A non-exhaustive `match` is rejected with the missing patterns listed.
4. Recompilation is incremental: modifying one of three files does not recompile
   all three.
5. The full suite passes clean.

## 12. Out of scope

What F0 does **not** include, so it cannot sneak in through the back door:

- Agents, tools, prompts, tensors, concurrency, durability (those are F1–F4).
- Associated types, trait specialization, const generics.
- Closures capturing by reference. F0 closures capture by ownership; the
  by-reference case is revisited in F1.
- Macros of any kind.
- Language server, formatter, package manager.
- Cross-compilation and targets other than the host.
- Custom optimizations: everything is delegated to LLVM's passes.
