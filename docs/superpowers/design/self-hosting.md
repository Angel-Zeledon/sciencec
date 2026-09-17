# Science — Design: self-hosting

Date: 2026-09-16
Status: **§1's measurements are superseded — see the amendment in §1.2.** The
argument of §2 onward stands; the numbers it opens with do not.
Depends on: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
(§6 ownership and regions, §7 compiler architecture, §10 testing, §11 definition
of done, §12 out of scope, §13 reserved words), and the compiler as it exists in
`crates/` today.
Related: `syntax-revision-2.md` §10 (the churn record this note is built on),
`ffi-c-boundary.md` §5 (linking, which is the bootstrap's enabler),
`package-manager.md` §2.2, §2.3, §2.8, §6.4 (content addressing, the `[build]`
table, bit-reproducibility — reused wholesale rather than reinvented),
`rust-interop.md` §3.7 (where the two ownership models disagree),
`stdlib-core.md` §9 and `stdlib-standard.md` §13 (what a compiler may assume).

Scope: writing the Science compiler in Science. What has to be true before that
can start, what "two implementations agree" means concretely, which component
goes first, what happens to the Rust compiler afterwards, what a syntax change
costs once it has happened, and what would make this note wrong.

---

## 0. The question, and the shape of the answer

The question asked was three questions:

> *how far are we from writing our compiler in our own language, what is the
> best plan to get there, and how does that plan not damage what we are already
> trying to be.*

Since it was asked, one thing has changed and it changes this note's job.
**Self-hosting is a stated goal of the project, not a proposal being evaluated.**
So this note does not argue about whether. It argues about *when it is honest to
start*, and — much more usefully — about **what has to be decided today so that
starting later is not a rewrite**.

The one-sentence answer:

> **The port is years of work away and should not start soon; but the single
> most valuable thing self-hosting would tell us is available in about two
> hundred lines of Science, right now, and we should take it immediately rather
> than waiting to learn it at stage three.**

Everything below is that sentence, with the measurements under it.

### 0.1 What this note is arguing against, stated first

The natural way to write this note would be a roadmap: five stages, each one a
component, each one gated on the previous. That note would be wrong in a
specific way, and the wrongness is worth naming before the roadmap appears in §8
anyway.

A staged component port learns things in the order the components are listed,
and the components are listed easiest-first. So it learns the **cheap** things
first and the **load-bearing** thing last. The load-bearing thing here is
whether region inference with no lifetime syntax can express a compiler, and a
component-ordered plan discovers that at stage four, after twenty thousand lines
of Science have been written on the assumption that it can.

§4 of this note is the attempt to answer that question now instead. It is the
part worth reading if you read nothing else.

---

## 1. Where the compiler actually is, measured

### 1.1 The numbers

Counted today over the workspace, `.rs` files only:

| Crate | Source | With tests | What it does |
|---|---|---|---|
| `science-parser` | 6,015 | 11,722 | AST, parser, textual AST dump |
| `science-resolve` | 4,706 | 7,255 | HIR, `DefTable`, scopes, resolution |
| `science-rt` | 2,230 | 4,213 | The runtime ABI |
| `science-lexer` | 1,384 | 3,123 | Tokens, indentation, literals |
| `science-db` | 918 | 1,377 | The salsa query database |
| `science-diagnostics` | 756 | 1,150 | Spans, source map, rendering |
| `science-testkit` | 703 | 1,255 | The UI-test harness |
| `sciencec` | 593 | 1,011 | The driver |
| **Total** | **17,305** | **31,106** | |

680 `#[test]` functions. 204 `insta` snapshots (150 parser, 29 resolve, 16
lexer, 9 diagnostics). 21 `.science` files in `examples/`. 12 failing programs
with byte-exact expected output in `tests/ui/`.

### 1.2 The four crates that do not exist

§7.2 of the core spec names twelve crates. Eight exist. The four that do not are
`science-types`, `science-mir`, `science-regions` and `science-codegen` — in
order, the type checker, the mid-level IR, region inference and code generation.
`crates/science-db/src/pending.rs` declares their query signatures and every body
panics; that file is the honest inventory of the hole, and it says so: *"Every
function below panics. What is real is the signature, the key type and the place
it occupies in the dependency graph."*

There is **no LLVM dependency anywhere** — no `inkwell`, no `llvm-sys`, no
`cranelift`, in any `Cargo.toml` in the workspace. `sciencec` has four
subcommands — `check`, `tokens`, `ast`, `resolve` — and **there is no `build`**.
There is no `fmt` either, which §6.3 will turn out to care about.

So the honest statement of distance is not "we are N% of the way to
self-hosting". It is:

> ~~**Science cannot produce an executable. Until it can, self-hosting is not a
> plan, it is a preference.**~~

> **AMENDMENT: it can, and the whole of §1.1 and §1.2 is superseded.**
>
> `sciencec build hello.science` produces a native executable that prints and
> exits 0. A failing script exits 1 after writing to stderr. A program that
> calls `cos` through an `extern "C"` block links against libm and runs. So the
> sentence above has stopped being the statement of distance, and the honest one
> has moved.
>
> **All four crates §1.2 says do not exist, exist**: the type checker, the
> mid-level IR, region inference and code generation. `sciencec` has `build`,
> and `fmt`, and eight subcommands rather than four. The compiler was 17,305
> lines of source over eight crates; it is **60,733 over fourteen**, and the
> four that were missing are 30,466 of them — half the compiler, and all of it
> the half that would have to be ported.
>
> There is now an LLVM dependency, and §1.2's clause that there is none needs
> reading twice rather than deleting: **no `Cargo.toml` names `inkwell`,
> `llvm-sys` or `cranelift`, and that is now a commitment instead of an
> observation.** The backend is `extern "C"` against LLVM-C, which is the shape
> `codegen-and-linking.md` Decision 1 requires of *both* implementations for
> exactly this note's reason — a self-hosted `science-codegen` cannot call a
> Rust wrapper.
>
> **What is measured, not claimed:** Gate B is met. `examples/21_compiler_shapes.science`
> — a flat index-keyed table with parent links, a type holding a borrowed array
> in a field, a boxed recursive tree, a work-queue traversal over indices —
> passes the borrow checker **with no region written anywhere**. §1's own
> caveat, that this was "an analysis of shapes, not a compilation", no longer
> applies: it is a compilation. That was the load-bearing claim of §3 and §4 and
> it held.
>
> **What the new distance is**, and it is not a percentage. Checking is solid:
> the corpus passes but for one file that imports modules this repository does
> not contain. *Building* is early, and five named pieces stand between here and
> a corpus that compiles — method calls, generics, arrays and indexing, drop
> glue for anything owning more than a bare `String`, and the tuple-and-cast
> pair. The first two are worth more than the other three together.
>
> **And the warning §1 could not have given**, because it predates any backend:
> in this half, silence does not mean it works. Five silent miscompiles have
> been found by *running* the output rather than reading it — a `main` that
> never wrote its return value, a store eight bytes wide into a one-byte slot
> that passes `LLVMVerifyModule`, a `needs_drop` blind to every runtime
> aggregate, a lowering hole that suppressed the borrow checker over whole
> bodies, and a pointer-to-pointer the verifier structurally cannot catch. Every
> construct the backend admits is gated by a program that is built, linked, run
> and checked for its output.

### 1.3 The one thing that is already bootstrap-ready, and it was not planned

`crates/science-rt/src/lib.rs` is 259 lines of module documentation over 2,230
lines of runtime, and its second paragraph says what it is:

> *Everything below is the contract between this crate and `science-codegen`. It
> is written so that codegen can be emitted from this page alone, without reading
> a single function body.*

It then does that. §2 pins `extern "C"` and `#[repr(C)]` on every symbol and
type, and states the `sret` obligation for aggregate returns against System V and
Windows x64 by name. §3 gives the pointer conventions. §5 fixes enum layout, the
niche rule, and how an optional comes back across the boundary. §6 gives the
element-descriptor convention that lets one runtime serve every monomorphisation,
and states its one unchecked precondition in a block quote. §7 pins the
never-null empty-container representation. `tests/layout.rs` asserts every size,
alignment and field offset.

**This is the most bootstrap-ready artefact in the repository**, and it is worth
saying why, because it is not obvious. A bootstrap's hardest hidden cost is
usually that the runtime and the compiler co-evolved in one language and nobody
ever wrote down where the seam is. Here the seam was written down *before* the
consumer existed, in a document whose stated test is that codegen can be emitted
from it alone. Whichever language `science-codegen` is eventually written in, it
reads the same page. **The runtime does not need to be ported for self-hosting at
all** — it is already the C-ABI floor that both implementations stand on.

**Two defects in it, recorded here rather than smoothed over**, because a
bootstrap contract with errors in it is worse than one with holes:

1. **§8 says every symbol is `link_`-prefixed. Every symbol is actually
   `science_`-prefixed** — 45 `science_*` entry points, zero `link_*`. The
   `LINK_OPTION_SOME` / `LINK_RESULT_OK` constants in `abi.rs` preserve the old
   prefix and are the fossil that explains it. Codegen emitted from this page
   alone, as the page invites, would emit undefined symbols.
2. **§5 and §9 are written in the pre-revision-2 error model.** They specify the
   layout of `Option[T]` and `Result[T, E]` and the behaviour of a `?` operator
   *"plus a `From` conversion on the error path"*, and `syntax-revision-2.md` §3
   removed all of that. The **layout rules survive unchanged** — they are rules
   about enums, and `T?` is an enum — but the names, the `LINK_RESULT_*`
   discriminants and the `From` mechanism describe a language that no longer
   exists.

Neither is hard to fix. Both are exactly the kind of drift that a bootstrap turns
from a documentation bug into a miscompilation, which is why they are §1's
business and not a footnote.

---

## 2. The gap is one thing — and the claim needs one amendment

The claim under test was: *until Science emits an executable, self-hosting is not
a plan; everything else is secondary.*

**It survives, and §1.2 is the evidence.** No amount of front-end polish moves
the date. A Science lexer that produces a perfect token stream is a program
nobody can run.

**The amendment is that "secondary" is the wrong word for one of the other
things.** Codegen is the *blocking* item; it is not the *risky* item. Codegen is
a large, well-understood, heavily-precedented piece of engineering with a written
contract already sitting in front of it (§1.3). Region inference over a
compiler's data structures is a piece of language design that has never been done
without lifetime annotations, and if it does not work the fix is a syntax change
that reverses the last line of core spec §6.1.

So the ordering by *blocking* is: codegen, then everything. The ordering by
*risk* is the reverse. A plan that only respects the first ordering discovers the
risk at the worst moment, which is §0.1's point and §4's job.

---

## 3. What a compiler tests, and what it does not

### 3.1 The half of the received claim that is wrong

The claim handed to this note was: *a compiler is not a scientific workload; it
exercises string munging, graphs, arenas and interning, and none of the
language's differentiators — units, tensor shapes, `Quantity`, uncertainty,
DLPack. So it proves the language adequate, not good at what it exists for.*

**The first half is true and checkable.** Read the 17,305 lines: there is no
floating-point arithmetic of consequence anywhere in the front end, no array
mathematics, no `f64` outside literal parsing. A self-hosted `sciencec` would
exercise `unit-literals.md` not at all, `broadcasting.md` not at all,
`uncertainty.md` not at all, `scientific-libraries.md` not at all, and
`data-leakage.md` not at all. Self-hosting is not evidence that Science is good
at science.

**The second half does not follow, and it is backwards.** It assumes the value of
a test is proportional to how much of the surface it covers. The value of a test
is proportional to how much it tells you about the thing most likely to be wrong,
and by that measure a compiler is not a mediocre test of Science. It is the
**adversarial** test of the single most novel claim the language makes.

That claim is the last line of core spec §6.1:

> *There is no lifetime syntax. The programmer never writes a region.*

Rust has lifetime annotations, and one of the historical reasons it has them is
programs shaped like compilers: trees with parent links, a symbol table
referenced from everywhere, strings interned once and borrowed everywhere, graphs
with cycles, arenas that outlive what borrows from them. These are the shapes
that make inference hard, and they are what a compiler is made of. If Science's
inference cannot carry them, *that* is where it shows.

So: self-hosting is not a side quest that proves adequacy. It is the hardest
available test of the load-bearing design decision, and discovering at stage
three that region inference cannot express a symbol table is discovering it at
the most expensive possible moment.

### 3.2 The steelman, taken seriously

The general form of the counter-argument is stronger than the compiler-specific
one and deserves stating at full strength:

> Dogfooding catches what a benchmark never will. A compiler is a large program
> written by the people who know the language best, maintained over years, under
> real pressure, with real tests. Every ergonomic wart gets felt a hundred times
> instead of once. Every missing library function gets discovered by somebody who
> can add it. Rust, Go, TypeScript, Zig and Swift all self-host, and all of them
> cite it as the thing that made the language liveable rather than merely
> defensible. A language whose authors do not use it for their hardest program is
> making a claim they have not tested.

This is correct, and the part of it that is *most* correct is the part about
duration: a corpus file is written once and read by a test; a compiler is edited
for years. Ergonomic costs are integrals over time, and only a long-lived program
measures them.

### 3.3 Where it stops, and the decision that follows

The steelman establishes that the compiler is a uniquely informative program. It
does **not** establish that the port must happen soon, and the reason is a gap
between two quantities that nobody usually separates:

- The **test** — does region inference, with no lifetime syntax, accept the data
  structures a compiler is made of? — is about **two hundred lines of Science**.
- The **port** — is the whole 17,305 lines, plus everything not yet written.

The information is almost all in the first. A `DefTable`, a parser holding
borrowed tokens, a boxed expression tree, an index-keyed graph walked by a work
queue, and a memo table are the *shapes*; writing twenty thousand more lines of
the same shapes tells you about ergonomics, not about soundness.

> **Decision 1. Extract the adversarial test from the port and take it
> immediately. A corpus file — call it `examples/21_compiler_shapes.science` —
> encodes the five data structures §4 examines, in current syntax, and joins the
> 21-file corpus today. It costs nothing now, because the corpus contract is only
> that a file lexes and parses with no diagnostics. The day the borrow checker
> exists, it becomes the acceptance test for the claim self-hosting was going to
> make, and it makes it three years early.**
>
> **Cost.** One more corpus file to migrate on every syntax revision — the exact
> cost §6 spends its length complaining about, accepted here because this file is
> worth more than the other twenty-one put together for this purpose.
>
> **Rejected: wait and find out during the port.** That is the failure mode in
> §0.1 with a schedule attached.

**This is the note's main recommendation and it differs from the one it was asked
to test.** The received position was "defer self-hosting, and here is why". The
position here is **"defer the port and run the experiment now"** — because the
counter-argument won on the point it actually made, and what it wins is an
argument for *urgency about the question*, not urgency about the port.

---

## 4. Can Science express its own compiler? Five structures, checked

### 4.1 The method, and the measurement that reframes it

Rather than reason about compilers in the abstract, take the structures this
compiler is actually made of and ask, one at a time, whether each has a spelling
in Science as specified today. This is possible because the front end is written
and can be read.

Before the individual answers, one measurement:

> **Across 17,305 lines, exactly two types carry a lifetime parameter, and there
> is no `Rc`, no `Arc`, no `RefCell` and no `Cell` in any non-test code.**

`Parser<'t>` in `crates/science-parser/src/parser.rs`, and `Node<'a, T>` in
`crates/science-resolve/src/dump.rs`. Both have a single region. The one
`Arc<Mutex<…>>` in the workspace is `science-db`'s execution log, which exists so
that `tests/incremental.rs` can prove memoisation, and is test infrastructure.

That is a striking result and it is not a coincidence. The front end was written
in the index-not-pointer style throughout: `DefId(u32)` into a flat `DefTable`,
`FileId` into a `SourceMap`, `Span { file, start, end }` rather than a pointer
into text, `Box` for tree recursion and `Vec` for everything else.
`science-resolve/src/hir.rs` says so in its own module documentation — *"Names
live in the `DefTable`, not in the tree"* — and gives the reason, which is about
resolution, not about ownership.

**The project has, for unrelated reasons, already written its compiler in the
style that an ownership system with no shared ownership requires.** That is the
most important single fact in this note and it materially improves the odds.

### 4.2 The `DefTable` — yes, straightforwardly

`hir.rs`: a flat, append-only `Vec<Def>`, `alloc` returns an index, `get` returns
a borrow, `parent` is an `Option<DefId>`, `path_of` walks parents upward,
`children` filters.

```science
type DefId:
    index: Int

type Def:
    id: DefId
    kind: DefKind
    name: String
    span: Span
    parent: DefId?

type DefTable:
    defs: Array of Def

DefTable has:
    def new() -> DefTable:
        DefTable(defs: (Array of Def).new())

    def alloc(mutable self, kind: DefKind, name: String,
            span: Span, parent: DefId?) -> DefId:
        let id be DefId(index: self.defs.length())
        self.defs.push(Def(id: id, kind: kind, name: name,
                           span: span, parent: parent))
        id

    def get(self, id: DefId) -> borrowed Def:
        self.defs[id.index]

    def path_of(self, id: DefId) -> String:
        let mutable parts be (Array of String).new()
        let mutable cursor: DefId? be id
        loop:
            if not cursor?:
                break
            let def be self.get(cursor)
            if def.name.length() > 0:
                parts.push(def.name.clone())
            cursor be def.parent

        let mutable path be String.new()
        for step in 0..parts.length():
            let part be parts[parts.length() - 1 - step]
            if path.length() > 0:
                path.push_str(".")
            path.push_str(part)
        path
```

Every line of that is F0. The `loop` / `break` / `cursor?` shape, with `cursor`
narrowing from `DefId?` to `DefId` after the test, is the one `stdlib-core.md`
§3.7 already uses for a graph traversal and cites as the argument that `loop` and
`break` are enough. `alloc` takes `mutable self` and `get` takes `self`,
and they are never live at once because the Rust code does not hold them at once
either.

**Verdict: yes.** No region is written and none needs to be.

**One cost, and it is real.** `children` returns `impl Iterator<Item = &Def>` in
Rust. Science has no unboxed existential return: `any Interface` is a boxed trait
object, and `syntax-revision-2.md` §3.4 states the allocation cost for the error
case, which applies here too. So `children` must either return
`Box of any Iterate of (borrowed Def)`, allocating per call, or a **named**
iterator type holding `borrowed DefTable` and a cursor. The named type is the
right answer and it is the convention `collections-and-chains.md` already adopts
— `Split`, `Lines` and `Sorted of Self` are all named types for exactly this
reason. The cost is one type declaration per iterator, paid perhaps thirty times
across the compiler.

### 4.3 The parser over borrowed tokens — yes, and it is §11.1's own test case

```rust
pub struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
    eof: Token,
    each_scopes: Vec<bool>,
    diagnostics: Diagnostics,
}
```

In Science:

```science
type Parser:
    tokens: borrowed Array of Token      # the region is inferred
    pos: Int
    eof: Token
    each_scopes: Array of Bool
    diagnostics: Diagnostics

Parser has:
    def peek(self) -> borrowed TokenKind:
        self.token_at(0).kind

    def into_diagnostics(self) -> Diagnostics:
        self.diagnostics
```

Two things to check, and both check out.

**A type holding a borrow in a field** is core spec §4.4's own example —
`type View: source: borrowed Doc  # the region is inferred` — and it is
requirement ten of §11's definition of done, and `examples/03_structs.science`
and `examples/18_ownership.science` both already exercise it. It is committed,
not hoped for.

**`into_diagnostics(self)`** moves out of `self`. §6.1 rule 2 makes passing
`self` by value a move, and the other fields are destroyed at the end. F0 has no
partial-move-out-of-`self` rule written down anywhere, but it does not need one
here: the body is a field read of a moved-in value that is about to die.

**Verdict: yes, conditional on one thing §6.2 does not say.**

`token_at` returns a borrow that comes from *either* `self.tokens` — whose region
came from the caller — or `self.eof`, whose region is `self`'s. Rust unifies them
at the shorter, which is `&self`. Science's inference must do the same, and §6.2
describes the analysis purely as a per-function MIR problem with interprocedural
signature inference. **It never says that a *type* has an inferred region
parameter, or how many.** §4.4's `type View` example asserts that it does, and
`ffi-c-boundary.md` §2 leans on it by name — *"a borrow whose region is inferred
exactly as §4.4 of the core spec infers the region in `type View: source: borrowed
Doc`"* — but nothing specifies the rule.

This is §5.3's finding, and it is the cheapest thing in this note to fix.

### 4.4 The HIR tree — yes, and the derive hole is the cost

`hir.rs`'s `ExprKind` is a 30-variant enum whose payloads are `Box<Expr>` and
`Vec<Arg>`. A Science `choice` with `Box of Expr` and `Array of Arg` is the same
shape, with the same layout rules — `science-rt` §5.1: tagged, discriminant at
offset 0, values in declaration order — and `examples/04_enums.science` already
covers generic choices with payloads and a variant carrying a borrow.

```science
public choice ExprKind:
    Path(Res)
    Call(Box of Expr, Array of Arg)
    Field(Box of Expr, Ident)
    Binary(BinaryOp, Box of Expr, Box of Expr)
    Borrowed(Bool, Box of Expr)
    Error
```

**Verdict: yes.** Two costs, both known:

- **F0 has no variants with named fields.** `examples/04_enums.science` says so
  and every payload in the corpus is positional. Rust's
  `Call { callee, args }` becomes `Call(Box of Expr, Array of Arg)`, and every
  `match` arm loses its field names. Across a 30-variant expression enum and a
  20-variant item enum, that is a genuine readability regression in the most
  pattern-matched code in the project.
- **There is no derive mechanism.** Core spec §12: *"Macros. Reserved as a word,
  not implemented."* `hir.rs` derives `Debug, Clone, PartialEq, Eq, Hash` on
  nearly every type, and `dump.rs` hand-writes the textual dump. In Science,
  `Clone`, `Eq`, `Hash` and `Inspect` must be hand-written for every one of
  roughly ninety HIR and AST types, or the derive standing-ask in the README must
  land first. That ask already has customers in `data-io.md` §11.1,
  `strings-formatting-and-docs.md` and `rust-interop.md` §3.7.3. **A self-hosted
  compiler is the fourth and by volume the largest.**

### 4.5 Interned identifiers — the finding is that there are none

The claim under test was that self-hosting exercises interning. In this compiler
it does not: `ast::Ident` is `{ name: String, span: Span }` — an owned `String`
per occurrence — and the only interning in the workspace is salsa's own, in
`science-db/src/pending.rs`, for query keys.

That is a small performance debt in the Rust compiler and it is a large piece of
luck for the port, because **a string interner is the one classic compiler
structure that an ownership system without shared ownership handles badly**. The
natural shape is a bump arena handing out `&'arena str` that outlive every scope
that produced them, and `rust-interop.md` §3.7.1 names precisely that shape —
*"an arena's `&'arena T`"* — as having *"no Science spelling"*.

The Science-shaped interner is the same index discipline as everything else:
`Map of (String, SymbolId)` plus `Array of String`, handing out a `SymbolId`, with
`resolve(self, id: SymbolId) -> borrowed String` for when the text is
wanted. That is expressible, it is what `DefTable` already does for definitions,
and it costs a lookup where the arena version costs a dereference.

**Verdict: yes, and it should be adopted in the Rust compiler first**, where it
is a straightforward performance win, so that the port is a translation rather
than a redesign.

### 4.6 The salsa query database — the one that does not port, and what to do

`science-db` is 918 lines, of which the substance is `salsa 0.28` annotations:
`#[salsa::db]` on a struct holding `salsa::Storage<Self>`, `#[salsa::input]`,
`#[salsa::tracked]` functions with backdating, `#[salsa::interned]` keys with
`unsafe(no_lifetime)`, and `salsa::Event` for the execution log.

None of that has a Science spelling. Concretely:

- **Procedural macros.** Core spec §12: not implemented, reserved as a word.
- **Interior mutability.** `rust-interop.md` §3.7.2 is unambiguous: *"Rust has
  `Cell`, `RefCell`, `Mutex`, `UnsafeCell`; Science's rule 4 admits none."*
- **Shared ownership.** `stdlib-standard.md` §10.3 says out loud that the
  alternative to moving a value into a thread is *"to wrap it in an `Arc` Science
  does not have"*.
- **A self-referential `Storage<Self>`.** Rule 4 again.

So the naive answer is that memoisation is unwritable, because memoisation means
mutating a table through a borrow that a recursive call also holds:

```science
# Does not compile, and the reason is rule 4 of §6.1.
def ast(db: borrowed Database, file: FileId) -> Module:
    let hit be db.cached_ast(file)            # a shared borrow of `db`
    if hit?:
        return db.module(hit).clone()
    let text be source_text(db, file)         # re-enters `db`, shared again
    let parsed be parse(text)
    db.remember_ast(file, parsed)             # needs `mutable borrowed db`
    parsed
```

**But the naive answer is wrong, and the reason is §4.1.** Apply the project's
own index discipline to the query engine and it comes out:

```science
def ast(db: mutable borrowed Database, file: FileId) -> AstId:
    let cached be db.cached_ast(file)
    if cached?:
        db.record_dependency(cached)
        return cached

    db.push_active_query(QueryKey.Ast(file))
    let text_id be source_text(db, file)
    let parsed be parse(db.text(text_id))
    let id be db.asts.alloc(parsed)
    db.remember_ast(file, id)
    db.pop_active_query(id)
    id

def module(db: borrowed Database, id: AstId) -> borrowed Module:
    db.asts[id.index]
```

Three things fall out, and each is a decision this note is making:

1. **A query returns an id, never a borrow.** The borrow is a second call with a
   shared borrow, after the exclusive one has ended. This is exactly `DefId` into
   `DefTable`, applied one level up.
2. **`mutable borrowed Database` is threaded through every derived query.** It
   reads as a receiver, and the auto-borrow of §6.3 hides it at call sites.
3. **Dependency tracking needs no interior mutability**, which is the part that
   looked impossible. Salsa records dependencies by intercepting reads against a
   thread-local active-query stack precisely *because* its database sits behind a
   shared reference. Here every read already goes through the exclusive `db`, so
   the active-query stack is an ordinary field of it.

> **Decision 2. The self-hosted query database is a rewrite of salsa's idea, not
> a port of salsa. Queries take `mutable borrowed Database` and return ids;
> values are read back through a shared borrow in a second call; the dependency
> stack is a field. No shared ownership, no interior mutability and no macro is
> required.**
>
> **Cost, and it is the largest single cost in the whole plan.** Three parts.
> First, **the per-query boilerplate that `#[salsa::tracked]` generates must be
> hand-written** — the memo lookup, the dependency push, the backdating
> comparison — perhaps forty lines per query across roughly twenty queries. That
> is §4.4's derive ask arriving in its most expensive form. Second, **no
> parallelism**: salsa permits concurrent queries against a shared database and an
> exclusive borrow does not. `.parallel()` is F2 and actors are F3, so nothing is
> lost today, but a self-hosted compiler cannot later become a parallel one
> without revisiting this. Third, **it is new code with no upstream**:
> `science-db`'s module documentation spends a screen on which salsa version
> behaves how, and all of that maintenance knowledge is discarded.
>
> **Rejected: keep `science-db` in Rust forever and link it.** Tempting, and it
> would work — `ffi-c-boundary.md` and `rust-interop.md` give the mechanism. It
> is rejected because the query database is the spine: §7.1's whole pipeline is
> queries and §7.3 says the same database is what F1's interactive tier runs on.
> A "self-hosted" compiler whose spine is foreign is a Science front end embedded
> in a Rust program, which is a different and much less interesting claim.

**And the collision worth recording.** Core spec §11 point 4 makes incremental
recompilation part of the definition of done, proved on execution counts, and
§7.3 says *"the same database is what F1's interactive tier runs on: re-executing
a notebook cell is invalidating a query."* So **if Decision 2 fails, it does not
merely make the self-hosted compiler slow — it forecloses F1's interactive
tier.** That is the highest-consequence dependency in this note, and it runs
between two sections of the core spec that do not reference each other.

### 4.7 Arenas, since the question was asked directly

**Arenas in the bump-allocator sense — a region of memory handing out `&'arena T`
references that outlive the call that created them — are not expressible**, and
`rust-interop.md` §3.7.1 already says so in its own words. There is no `Rc`, no
`UnsafeCell`, and no way to write the type that owns the storage while references
into it circulate.

**Arenas in the sense a compiler actually uses them — one flat, append-only,
index-keyed table per node kind, with allocation returning an index — are exactly
what Science is good at**, and it is what `DefTable` already is. Every use of an
arena in this codebase is the second kind.

The residual cost is the one every index-based design pays: an index is not
type-safe against being used with the wrong table, and it is not invalidated when
the table is cleared. Rust's arena crates do not help with that either. The
mitigation is the newtype discipline `DefId`, `FileId` and `AstId` already
follow.

### 4.8 The score

| Structure | Expressible today? | What it costs |
|---|---|---|
| `DefTable` — flat, index-keyed, parent links | **Yes** | A named type per iterator |
| `Parser` over borrowed tokens | **Yes**, given §5.3 | Nothing, if regions on types are specified |
| HIR tree of `Box`ed nodes | **Yes** | Positional payloads; ~90 hand-written `Clone`/`Eq`/`Hash`/`Inspect` |
| Interned identifiers | **Yes** | A lookup where an arena has a dereference |
| Query database with memoisation | **Yes, by rewrite** | Decision 2's three costs; no parallel queries |
| Bump arena handing out borrows | **No** | Not needed; nothing here uses one |

**Five of six, and the sixth is not used.** That is a better answer than the
question deserved, and the reason is §4.1: the compiler was already written in
the style. The honest caveat is that this is an analysis of shapes, not a
compilation — the borrow checker does not exist, so nothing here has been
*proved*. Decision 1 is how it gets proved.

---

## 5. The decisions being made right now that could foreclose it

Cheap today, expensive later, in rough order of how expensive later is.

### 5.1 The comparable artefacts are Rust's `Debug`, by accident

`sciencec tokens` prints, per token:

```rust
out.push_str(&format!("{:>4}..{:<4}  {:?}\n", token.span.start, token.span.end, token.kind));
```

`{:?}` on `TokenKind`, which is `#[derive(Debug)]`. So the format of the token
dump — the artefact §7 is about to make the gate for the first component port —
is **whatever `derive(Debug)` happens to print**: `Int { value: 1, base: Decimal,
suffix: None }`, with Rust's spacing, Rust's `None`, Rust's field-name syntax. A
Science lexer would have to reproduce `derive(Debug)`'s output conventions, which
are not a specification and are free to change in a Rust release.

The AST dump is better — `science-parser/src/dump.rs` is hand-written and its
output (`Fn \`largest\` public @60..188`) is a deliberate format. The resolve dump
likewise. But neither is *written down* anywhere except as the code that produces
it and 179 snapshots of what it produced.

> **Decision 3. The token, AST and resolved-crate dumps are specified as
> implementation-independent text formats, and the Rust dumpers are checked
> against the specification rather than being it. In particular, the token dump
> stops using `{:?}`.**
>
> **Cost.** A day's work now, a specification to maintain, and 195 snapshots to
> re-bless once. Against: without it, "the two lexers agree" is defined by a Rust
> derive, and the first `rustc` release that changes `Debug` spacing breaks the
> bootstrap gate for a reason that has nothing to do with either compiler.

### 5.2 The MIR dump does not exist, which is the only good time to specify it

Core spec §10 names snapshot tests *"over AST, HIR and MIR dumps"*. The AST and
HIR dumps exist. `science-mir` does not, so the MIR dump is not yet anything.

Stage 3 of §8 needs a comparable artefact for the type checker and the region
solver, and there is no obvious one: types and regions are not trees, they are
constraint systems with solutions. Whatever artefact is chosen — a typed-HIR
dump, a MIR dump with inferred regions annotated at each borrow, a listing of
solved outlives constraints — **it is far cheaper to specify before the Rust
implementation exists than to retrofit afterwards**, and §5.1 is the evidence,
because that is exactly what happened to the token dump.

> **Decision 4. `science-mir`'s and `science-regions`' dump formats are specified
> in the same document as Decision 3's, before those crates are written, and the
> region solver's dump includes the provenance chain §6.2 requires it to carry.**
>
> **Cost.** It constrains the design of two crates that do not exist yet, which
> is a mild imposition on whoever writes them. §6.2 already imposes a much larger
> one — *"a solver written without provenance must be rewritten entirely to gain
> it"* — and this is the same argument applied to the same component.

### 5.3 Region parameters on types: §4.4 commits, §6.2 is silent

§4.4 writes `type View: source: borrowed Doc  # the region is inferred`. §11
makes *"a type holding a borrow in a field"* requirement ten of the definition of
done. `examples/03_structs.science` and `examples/18_ownership.science` exercise
it. `ffi-c-boundary.md` §2 depends on it by name, and §4.2 of that note builds
its `Plan` type on it — *"the region engine now knows that the plan borrows both
buffers"*.

§6.2 describes region inference entirely as a per-function MIR analysis with
interprocedural signature inference. It never says that a *type declaration*
acquires an inferred region parameter, how many it acquires when it has two
borrowed fields from different sources, or what happens when a generic type's
parameter is itself borrowed.

`rust-interop.md` §3.7.1 has already met the consequence from the other side:
multi-lifetime *signatures* have no Science spelling, and Decision 3 of that note
requires the shim to flatten them. The same question for *types* is unanswered.

**The good news, and it is §4.1 again:** across the whole front end, zero types
need two independent regions. `Parser<'t>` has one. `Node<'a, T>` has one region
shared by two fields. So the conservative rule — **a type has at most one
inferred region; two borrowed fields unify into it** — costs this compiler
nothing at all, and there is direct evidence for that rather than a guess.

> **Decision 5. The single-region rule for types is proposed as the F0 rule, and
> the evidence offered for it is that the existing compiler needs nothing more.
> It is stated as a restriction that can be lifted source-compatibly, in the
> shape `ffi-c-boundary.md` §5.4 uses for by-value aggregates.**
>
> **This note does not own core spec §6 and is not amending it.** The ask is
> filed in §14.
>
> **Cost.** A type that genuinely holds two borrows of different provenance is
> rejected, with a diagnostic the ownership range does not yet have a code for.
> The audience most likely to hit it is not scientists; it is whoever writes the
> region solver.

### 5.4 What a compiler needs from the standard library, and what it does not

From `stdlib-core.md` §9's summary table, a compiler needs — and needs *running*,
not merely specified:

**Level 1, all of it load-bearing.** `Array of T`, `Map of (K, V)` with
`K: Eq + Hash` (keyed by `DefId`, by `String`, by `FileId`), `Set of T`,
`String`'s nineteen methods — `push_str`, `split`, `find`, `slice`, `parse_int`
and `parse_float` are each used on nearly every compiler path — `Box`, `T?`,
`Range`, `Iterate` and its provided methods, `Index of Idx`, `Hash`, `Eq`, `Ord`,
`Clone`, `Display`, `Inspect`, the `Error` interface, `IoError`, `TextError`,
`read_file`, `write_file`, `read_lines`, `print`, `write`, `print_error`, and
`Path`.

**Level 2, four modules and no more.** `collections` for `Deque` — every
work-queue in a resolver and a region solver is one, and `stdlib-core.md` §3.7's
own worked example is a graph traversal using it; `io` for `BufferedWriter`,
because a compiler that writes a dump a line at a time through unbuffered `File`
is unusable; `fs` for `list_directory` and `glob`, which the driver needs to walk
a package; and `os` for `args` and `env`, which is how a CLI starts at all.
`testing` is needed by the port's test suite rather than by the compiler.

**Deliberately not needed, which is most of the library:** `math` beyond Level 1,
`linalg`, `signal`, `stats`, `optimize`, `chem`, `bio`, `physics`, every `data.*`
module, `net`, `http`, `random`, `logging`, `thread`, and `text` beyond the core
part. This is the good news hiding inside §3.1's true half: **the bootstrap does
not block on the scientific library at all**, so the two can be built in parallel
and the scientific half stays the priority. That is the direct answer to the
third question in §0 — self-hosting does not compete with the language's
aspirations for library effort, because it needs almost none of the library the
aspirations are about.

**And one thing that is needed and is in the wrong place.**
`package-manager.md` §2.2 identifies every dependency by the SHA-256 of its
canonical archive, and §2.3 puts hashes in `science.lock`. So `sciencec` computes
SHA-256. But `stdlib-standard.md` §13.1 decides that `crypto` **wraps libsodium
and implements nothing**, and `crypto` is Level 2. Following both decisions, **a
self-hosted `sciencec` links libsodium** — the compiler acquires a native
dependency that must be present in order to build the compiler that builds it.
That is a bootstrap circularity of exactly the kind this note exists to find
early. The options are to vendor a SHA-256 implementation into the compiler
(against §13.1's "implement nothing"), to make SHA-256 the one exception in
`crypto` that is written in Science, or to accept libsodium in the seed's
dependency set. **This note does not decide it** — it belongs to
`stdlib-standard.md` §13 — but it is filed in §14, and it is not visible from
either note alone.

### 5.5 Reserved words: all four a compiler wants are already reserved

Checked against §13 directly. `macro`, `static`, `union` and `import` are all on
the **"Reserved, not yet used"** list, together with `mod`, `extern`, `unsafe`,
`move`, `with`, `assert` and `pure`. None of them can be taken by a user program
between now and whenever a self-hosted compiler wants them, which is the entire
purpose of that list, and it is doing its job.

Two hazards that are not on anybody's list:

- **`type` is a keyword and a compiler wants a field called `type` everywhere.**
  The Rust code already writes `ty` — `Cast { expr: Box<Expr>, ty: Type }` — for
  the same reason, so Science inherits the workaround for free. Worth one line so
  that nobody proposes the dot rule as a fix: the dot rule
  (`reserved-words.md` §0.1) makes `def.type` legal as a *member access* and does
  not make `type: TypeId` legal as a *field declaration* inside a `type` body,
  where it collides with the item keyword at the start of a line.
- **`match` is a keyword and a compiler wants `match` as a local**, as in
  `let match be find(pattern, text)`. Blocked; rename to `found`. That is the
  only name in the entire front end that a Science port would have to change for
  reserved-word reasons, which is a good result for §13.

### 5.6 Does anything assume a program is a numerical pipeline? Almost nothing

Checked deliberately, because it was asked. The answer is no, with one near miss
and one genuine friction:

- **`shape`, `model` and `tensor` are reserved globally** (§13), which the spec
  already records as a known cost. A compiler wants `Shape` as a const-parameter
  kind — `science-resolve`'s `ConstParamKind::NAMES` is `["Int", "Shape"]` — and
  the reservation is lower-case, so `Shape` as a variant name is fine. Near miss,
  and it is the second time the case convention has quietly saved something.
- **`collections-and-chains.md`'s laziness and barriers** are designed around
  streaming over data. A compiler's chains are short and over small collections,
  so it pays the barrier cost `.sorted()` documents without getting the streaming
  benefit. Friction, not a blocker.
- **`reproducibility.md`'s float channels and `--deterministic`** are irrelevant
  to a compiler, except in the direction that matters in §7.3: the same
  determinism machinery is what makes the bootstrap fixpoint testable.
- **`effects.md`'s `pure def`** fits a compiler well — most of the front end
  is pure by construction — and the `external` bit correctly marks the driver.

**Nothing in the design assumes a numerical pipeline in a way a compiler
violates.** That was worth checking and the answer is genuinely clean.

---

## 6. Syntax churn, and why the commitment sharpens rather than dissolves it

### 6.1 The record, quoted

`syntax-revision-2.md` §10 says it in as many words:

> **This is the second syntax revision in one day.** The corpus was migrated to
> the first one this morning; `examples/`, `tests/ui/`, the lexer's 117 tests and
> the parser's 157 all encode it. Every one of those moves again. That cost is
> accepted here on the grounds that the language has no users yet and this is the
> cheapest day it will ever be — but it is the last day that argument is free,
> and **a third revision should be held to a much higher bar**.

Today those numbers have grown to 129 and 211, plus 204 snapshots, 21 corpus
files and 12 UI programs. The migration is already not free, and the note that
predicted it said so on the day.

### 6.2 What a syntax change costs after self-hosting, concretely

Today a revision costs: edit the lexer's keyword table, edit the parser, migrate
21 corpus files, migrate 12 UI programs, re-bless 204 snapshots, and fix the Rust
tests that embed source strings. One implementation, one migration, one
afternoon.

After self-hosting, the compiler's own source is Science, and the only binary
that can compile it understands the old syntax. The change becomes the standard
three-build dance, and it is worth writing out because people underestimate it:

1. **Build A.** Edit the Science compiler so that it *accepts both* spellings,
   old and new. Compile that source — which is still written in the old syntax —
   with the existing seed. You now have a compiler that reads both.
2. **Build B.** Mechanically migrate the compiler's own source to the new syntax.
   Compile it with Build A. You now have a compiler written in the new syntax
   that still reads both.
3. **Build C.** Delete the old spelling from the source. Compile with Build B.
   You now have the compiler you wanted.

Three full builds, two of which exist only to get across the gap, plus the
migration of every `.science` file in the project — including the compiler's own
tens of thousands of lines. And one thing that does not go away afterwards:

> **Build A must be kept forever**, or the old seed can no longer reach the new
> compiler and the chain from the last independently-verifiable binary is broken.
> **Every syntax revision after self-hosting adds one permanent artefact to the
> bootstrap chain.**

That is the concrete cost. It is not fatal — Rust and Go both do this routinely —
but it is the difference between an afternoon and a project, and it is paid every
time.

### 6.3 The gate is a tool, not a date

The obvious conclusion is "freeze the syntax before self-hosting", and it is the
wrong conclusion in two ways. It is unfalsifiable — there is no test for "the
syntax is settled" — and it is a hostage: a language cannot promise never to
change its syntax, and pretending otherwise just means the change happens anyway,
unplanned.

The commitment to self-host makes this sharper rather than softer. If the port is
coming, the migration cost is coming too, and the useful question is not *when is
the syntax done* but *how expensive is a migration*.

> **Decision 6. The syntax gate for self-hosting is not stability, it is
> mechanisability: `sciencec fmt` exists, and a migration tool can replay the
> revision-1-to-2 migration and reproduce the committed corpus byte for byte.**
>
> This is objectively testable — the old corpus is in git history and the new one
> is on disk — and it converts an unanswerable scheduling question into a piece
> of engineering. It has a second payoff this project specifically needs:
> `llm-ergonomics.md` §4.1 is entirely about canonicalisation for `sciencec fmt`,
> and `syntax-revision-2.md` §1.1 chose its comparison syntax on the grounds that
> the alternative was *"permanently un-normalisable"* by a formatter. **Both notes
> assume the formatter. Nobody has built it.**
>
> **Cost.** A formatter and a migration tool before the first component port,
> which is real work that is not codegen. Against: the 21-file migration has been
> done by hand twice already, and a third revision under Decision 6 is cheap in a
> way that no amount of waiting makes it.

---

## 7. Differential bootstrapping — the strongest asset, made precise

### 7.1 What is actually available

| Asset | Count | What it is |
|---|---|---|
| `examples/*.science` | 21 | Programs that must lex and parse with **no diagnostics at all** |
| `tests/ui/*.science` + `.stderr` | 12 | Programs that must fail, with byte-exact rendered output |
| `insta` snapshots | 204 | Textual dumps of tokens (16), AST (150), resolution (29), diagnostics (9) |
| `science-testkit` | 703 lines | The UI harness, with the compile step as a **closure** |

The last row is the underrated one. `science-testkit`'s documentation states the
property outright: *"The harness therefore has no dependency on any compiler
phase, and can be finished, tested and relied upon before the lexer, the parser
or the diagnostic renderer exist. Each crate plugs in whatever it has."* **A
harness that takes the compiler as a parameter takes *either* compiler as a
parameter.** The differential runner for the bootstrap is a second closure passed
to code that already exists and is already tested.

`examples/README.md` states the corpus contract in the same load-bearing way:
*"anything that fails to lex or parse here is a bug in the compiler or a gap in
the spec — not a typo to be quietly patched."* And `science-parser`'s corpus test
refuses exemptions: *"a corpus with a list of files that do not have to work
stops being an acceptance check."* Both sentences were written about one
compiler; both hold word for word about two.

### 7.2 What "agree" means, per component

The claim to check was that a token stream is a flat comparable artefact and that
an AST would normally need serialising *except that this project already dumps
ASTs as text*. **Both halves are true.** `science-parser/src/dump.rs` produces
`Module @0..519` / `Fn \`largest\` public @60..188` with a byte span on every
node; `science-resolve/src/dump.rs` does the same for the resolved crate; and
`sciencec ast` and `sciencec resolve` print exactly those. There is no
serialisation work to do. There is *specification* work to do, which is Decision
3.

With that, "agree" is definable per component, and the definitions get weaker as
the components get deeper — which is the honest shape of a bootstrap and is worth
stating rather than hiding:

| Component | The artefact | "Agree" means | Strength |
|---|---|---|---|
| Lexer | `sciencec tokens` | Byte-identical over 21 + 12 + fixtures | **Total** |
| Lexer diagnostics | rendered `.stderr` | Byte-identical over the 12 UI programs | **Total** |
| Parser | `sciencec ast` | Byte-identical over the same corpus; 150 snapshots reproduced | **Total** |
| Resolver | `sciencec resolve` | Byte-identical; 29 snapshots reproduced | **Total** |
| Types | a typed-HIR dump (Decision 4) | Byte-identical, *given a specified dump* | **Conditional** |
| Regions | inferred regions at each borrow, with provenance (Decision 4) | Byte-identical | **Conditional** |
| Codegen | the program's behaviour | Same output, same exit code, same diagnostics — **not** identical LLVM IR | **Weak, deliberately** |
| The whole | the compiler binary | The §7.3 fixpoint | **Total again** |

The codegen row deserves its own sentence. Requiring identical LLVM IR from two
independent codegens is requiring them to be the same program, which forbids the
second implementation from ever being better. Behavioural agreement over the
execution suite is the right bar, and §7.3 is what restores rigour on top of it.

### 7.3 The fixpoint, and the decision that already makes it possible

The classic bootstrap check, and the reason it is worth all of the above:

- **stage 1** — the Science compiler, compiled by the Rust compiler.
- **stage 2** — the Science compiler, compiled by stage 1.
- **stage 3** — the Science compiler, compiled by stage 2.

**stage 2 and stage 3 must be byte-identical.** They were built from the same
source by two compilers that are themselves built from the same source, so any
difference is a bug in one of them. This single check subsumes an enormous amount
of testing, and it is the reason bootstraps are trustworthy at all.

It requires bit-reproducible output, and **this project has already decided to
have it, for an unrelated reason**:

> `package-manager.md` **Decision 7.** *The compiler embeds no build timestamp,
> no hostname, no user name and no absolute source path in an output binary by
> default. Source paths are remapped to package-relative form;
> `--embed-build-info` opts back in.*

That decision was taken so that `sha256sum` matches for a scientist rerunning a
published result. It is exactly and without modification what makes the
stage-2/stage-3 fixpoint testable. **This is the most useful cross-note fact in
this note**, and it is the second time the pattern appears — the first being
§1.3 — where a decision taken for the scientific audience turns out to be what
the bootstrap needed.

### 7.4 The bootstrap enabler, and the correction it needs

The claim handed to this note was: *LLVM's public API is C, and `extern "C"`,
`via pkg-config` and `kind static` are designed and working —
`crates/science-parser/tests/fixtures/dgemm.science` is that exact shape.*

**The design half is right and the "working" half is not, and the difference
matters.** Verified:

- `ffi-c-boundary.md` §1.1 specifies
  `unsafe extern "C" library "openblas" via pkg-config "openblas":`, and §5.1
  specifies `kind static`, the three-tier search-path order, the `--link-arg`
  escape hatch, and the requirement that a `via pkg-config` block still declare a
  plain `library` fallback.
- `dgemm.science` is that exact shape and its header says it is *"the acceptance
  criterion for `extern` blocks … which must parse with no diagnostics at all"*.
  `examples/20_extern.science` additionally covers `kind static`,
  `when available`, `symbol`, and all five item forms.

But **"parses with no diagnostics" is the whole of what works.** §5 of the FFI
note opens with *"Science compiles ahead of time through LLVM to a native
binary"*, and §1.2 of this note establishes that it does not, because there is no
LLVM anywhere. So the correct statement is:

> **The FFI grammar that a self-hosted compiler would use to call LLVM is
> designed, specified and parsed, and nothing has ever been linked.** The
> enabler is real and it is one layer thinner than it looked.

The argument still holds and it is still worth making: because LLVM's public API
is C rather than C++, a Science-hosted `science-codegen` needs no new language
feature to drive it — the `extern` block it needs is the same one `dgemm.science`
already demonstrates, and `native-dependencies.md`'s provider order is how it
would find `libLLVM`. That is a genuine piece of luck. It is a piece of luck
about *design*, not about *implementation*.

---

## 8. The stages, and their gates

Gates, not dates. Each is something that is demonstrably true or demonstrably
not, with a command you can run.

**Gates A through E are preconditions. B, D and E can start today and do not wait
for codegen.**

### Gate A — the compiler can emit a binary

- **A1.** `sciencec build examples/00_kitchen_sink.science` produces an
  executable that runs and exits 0. That file is §11's acceptance program and it
  already exists.
- **A2.** The fourth test layer of core spec §10 — *execution tests: compile,
  run, compare output and exit code* — exists as a harness, in
  `science-testkit`'s closure-parameterised style.
- **A3.** `science-rt`'s §8 naming defect and its §5/§9 `Result`/`Option` residue
  are fixed, so that the contract codegen is emitted from is the contract the
  runtime implements (§1.3).

### Gate B — the language holds a compiler's shapes

- **B1.** `examples/21_compiler_shapes.science` (Decision 1) lexes and parses
  today, with no diagnostics, like every other corpus file.
- **B2.** The day the borrow checker exists, it accepts that file with **no
  region written anywhere**, which is the same bar `examples/18_ownership.science`
  already sets for itself.
- **B3.** The file contains, at minimum: a flat index-keyed table with parent
  links; a type holding a borrowed array in a field; a boxed recursive tree with a
  twenty-plus-variant `choice`; a work-queue graph traversal over indices; and the
  memo-table shape of Decision 2 with an `alloc`-then-read-back pair.

**B2 is the gate that must not slip.** It is the load-bearing claim of §3 and §4,
and it is reachable years before A.

### Gate C — the standard library runs

- **C1.** An execution test builds a `Map of (String, Int)`, a `Set of DefId`, an
  `Array of (Box of Expr)`, splits a file into lines, formats a dump through a
  `BufferedWriter`, and writes it — Level 1 plus `collections`, `io`, `fs` and
  `os` per §5.4 — and its output is byte-identical across two runs and two
  machines.
- **C2.** Either a derive mechanism exists, or the cost of hand-writing `Clone`,
  `Eq`, `Hash` and `Inspect` for ~90 types has been paid once and measured (§4.4).
- **C3.** The SHA-256 circularity of §5.4 is resolved one way or the other.

### Gate D — the artefacts are specified, not incidental

- **D1.** The token, AST and resolve dump formats are written down, the token
  dump no longer uses `{:?}`, and the Rust dumpers are tested against the
  specification (Decision 3).
- **D2.** The typed-HIR, MIR and region dump formats are specified **before**
  `science-types`, `science-mir` and `science-regions` are written (Decision 4).

### Gate E — a syntax migration is mechanical

- **E1.** `sciencec fmt` exists and is idempotent. **Met.** `crates/science-fmt`
  shipped, with corpus and fixture tests.
- **E2.** A migration tool replays the revision-1-to-2 migration from git history
  and reproduces the committed corpus byte for byte (Decision 6). **Not met.**
  There is no migration tool in `crates/`.

> **Gate E has been exercised once, by accident of timing, and half of it held.**
> Syntax revision 3 — `function` becomes `def` — landed hours after `sciencec
> fmt` did: 384 corpus declarations, 264 embedded in Rust test sources, 116
> snapshots, one afternoon. That is the cost Decision 6 predicted a mechanical
> migration would have, and the prediction was right.
>
> Two things must be said against reading that as the gate working. **The gate
> did not gate.** Nothing about revision 3 was conditioned on E1 being met; the
> decision was made by the project owner and would have been made either way. A
> gate that is not consulted is a cost estimate, which is worth having and is not
> a control. **And E2 was never exercised**, because the revision-3 migration was
> not replayable either — it was a mechanical rename applied once, not a tool.
> Decision 6 is confirmed as a *criterion* and is **not discharged**.
> `syntax-revision-3.md` §4 has the full account.

### Gate F — stage 1a: the lexer

- **F1.** The Science lexer's token dump is byte-identical to the Rust lexer's
  over all 21 corpus files, all 12 UI programs and the lexer's own fixtures.
- **F2.** Its rendered diagnostics are byte-identical over `tests/ui/`, all 12,
  including `several_errors_in_one_file.science`, which is the one that tests
  ordering and recovery rather than a single message.
- **F3.** Its own source lexes identically under both implementations. This is
  free, and it is the first moment the bootstrap is self-referential.

### Gate G — stage 1b: the parser

- **G1.** AST dump byte-identical over the same corpus.
- **G2.** All 150 parser snapshots reproduced, **including the 20 migration
  diagnostics**, which are the hardest of the set because they are about
  *recovery* — a file that says `trait` where `interface` belongs must not derail
  the rest of the file, identically, in both.
- **G3.** The Science parser parses its own source, and the AST dump of it is
  identical under both.

### Gate H — stage 2: resolution

- **H1.** Resolve dump byte-identical; 29 snapshots reproduced.
- **H2.** `DefId` allocation order is identical, which the Rust code already
  guarantees by construction — *"Ordering is allocation order, which makes any map
  keyed by `DefId` iterate deterministically — the snapshots depend on it."* That
  sentence was written for snapshot stability. It is a bootstrap gate.

### Gate I — stage 3: types, MIR and regions

- **I1.** Requires D2. Typed-HIR and MIR dumps byte-identical.
- **I2.** Every `SC03xx` ownership diagnostic byte-identical over the UI suite,
  **including the provenance chain** §6.2 requires. This is the gate that is
  genuinely hard and it is worth the most, for the reason `science-testkit`
  gives: *"for a language whose regions are inferred it is not optional: the
  message is the feature, because the programmer never wrote the annotation the
  compiler is complaining about."*

### Gate J — stage 4: codegen and the fixpoint

- **J1.** The execution suite passes identically under both compilers.
- **J2.** stage 2 and stage 3 are byte-identical (§7.3).
- **J3.** `--embed-build-info` is off by default, per `package-manager.md`
  Decision 7, which J2 depends on.

### Gate K — the seed is publishable

- **K1.** A third party, given only the published stage-0 artefacts, rebuilds
  stage 3 and gets the same hash. §10 is what that means.

---

## 9. Which component goes first — the claim, tested

The claim was: the lexer, because it is smallest and its artefact is flattest.

**The claim is right about the lexer and wrong about the reason, and the reason
matters because it changes what the first stage *is*.**

The lexer is the right first *component*: 1,384 lines, no shared state, no
graphs, no regions, an output that is a flat list, 129 tests, and it covers all
12 UI programs' diagnostics because every one of them is a lexical error. Nothing
else is close.

But "smallest, so start there" implies the port is a sequence of components, and
then the lexer's own weakness becomes the plan's weakness: **the lexer exercises
none of the things §3 and §4 say are actually at risk.** It has no borrows in
fields, no graph, no memo table, no recursion over a deep tree. A lexer that
ports cleanly tells you almost nothing about whether a resolver will.

> **Decision 7. The first thing written in Science is not a component. It is
> `examples/21_compiler_shapes.science` (Gate B), which is two hundred lines and
> answers the risky question. The lexer is the first *component*, and its purpose
> is to test the *bootstrap harness* — the dump formats, the differential runner,
> the seed mechanics — on a component small enough that a failure is obviously
> the harness's fault.**

Read that way, the lexer's lack of hard shapes is a feature rather than a defect:
you want the first component port to fail for infrastructure reasons or not at
all.

The rest follows the pipeline, because each phase's input is the previous phase's
output and a differential test needs a known-good input: lexer, parser, resolver,
types, MIR, regions, codegen. There is no cleverness available here and none
needed.

---

## 10. What happens to the Rust compiler — the seed problem

A self-hosted compiler needs a compiler to compile it. That is stage 0, and how
it is kept is a question about trust, not about storage.

### 10.1 The three options, and the decision

- **Keep the Rust compiler alive forever as a maintained parallel
  implementation.** Maximum trust, and the cost is maintaining two compilers
  forever, which no project with this one's resources has ever sustained.
- **Freeze the Rust compiler at the version that first compiles the Science
  compiler, and keep it as source.** It is a Rust program; it builds with a
  `rustc` and a `Cargo.lock`. It bit-rots, but slowly and visibly, and anyone can
  rebuild it from source they can read.
- **Ship a binary seed.** What most languages do and what nobody is comfortable
  with, because Thompson's trusting-trust attack is exactly this shape and
  because a binary is not auditable.

> **Decision 8. The Rust compiler is frozen as *source* at the commit that first
> passes Gate J, tagged, and published as a content-addressed archive with a
> `[build]` table. Binary seeds are published as a convenience and are never the
> only path.**
>
> **Cost.** A frozen Rust tree that slowly stops building as `rustc` moves. The
> `[build]` table's `Cargo.lock` hash and recorded `rustc` version are what make
> that recoverable rather than fatal, and they are already specified.

### 10.2 The mechanism already exists and should not be reinvented

This is the third time in this note that `package-manager.md` turns out to have
built what the bootstrap needs:

- **Decision 2** — *every dependency is identified in `science.lock` by the
  SHA-256 of its canonical archive, not by a version number and a source URL. The
  version is metadata. The hash is the identity.* A stage-0 seed is a dependency
  with unusually high stakes; it gets a hash like everything else, and the
  canonical-archive rules — sorted entries, zeroed mtimes, normalised modes, no
  compression in the hashed form — are already written down.
- **Decision 3** — the `[build]` table records the `sciencec` version, the LLVM
  version, the target triple, the optimisation level and the toolchain's own
  `Cargo.lock` hash, and a mismatch is a warning by default and an error under
  `--locked`. Written because *"codegen is not reproducibility-neutral"* for a
  scientist's numbers; it is exactly the table a bootstrap needs, for exactly the
  same reason.
- **Decision 22** — `sciencec archive` produces *"the package source,
  `science.lock`, the vendored dependencies with their manifests, the `[build]`
  table and the native lock entries"*, described as *"what you attach to a
  paper"*. A stage-0 seed release is an `archive`, with nothing added.
- **Decision 23** — a directory registry of content-addressed archives plus a
  signed index, needing no network and no trusted mirror, *"because the lock's
  hashes mean the mirror need not be trusted"*.

> **Decision 9. The stage-0 seed is released through `sciencec archive` and the
> directory-registry mechanism, with no bootstrap-specific machinery invented for
> it. `package-manager.md`'s Decisions 2, 3, 22 and 23 are adopted wholesale, and
> `--locked` is the mode a seed rebuild runs in.**
>
> **Cost.** It couples the bootstrap to the package manager's schedule.
> `package-manager.md` §7.1 puts only the one-way doors in F0 and the working
> manager in F1, which is comfortably before any of this matters.

### 10.3 Trusting trust, answered honestly

Decision 8 does not defeat the trusting-trust attack and nothing does on its own.
What it buys is the **diverse double-compilation** defence, and it buys it
cheaply because the second implementation is already the thing being built:
compile the Science compiler with the frozen Rust compiler and with an existing
self-hosted binary, and if §7.3's fixpoint holds under both, a compiler-resident
backdoor would have to exist in both toolchains. Since one of them is Rust-built
and one is Science-built, that is a much harder claim to sustain.

**This is the concrete payoff of keeping the Rust compiler as auditable source
rather than as a binary**, and it is the honest version of a claim that is
usually made too strongly.

---

## 11. What would make me abandon this, and what it leaves behind

Three abandonment conditions, in order of likelihood. Naming them is the point: a
plan whose failure modes are unnamed cannot be stopped, only abandoned messily.

**1. Gate B2 fails: region inference cannot accept the shapes.** The most likely
failure, and the reason Decision 1 exists. If it fails, the options are to add
lifetime syntax — reversing the last line of core spec §6.1 and, with it, one of
the three or four things that make Science distinct — or to self-host in a
restricted subset, or to stop. Gate B is early enough that this is a language
decision rather than a project crisis, **which is the entire argument for taking
Decision 1 now.**

**2. Decision 2 fails: the query database has no Science shape.** §4.6 argues it
does, but the argument is on paper. If it fails, the self-hosted compiler is
non-incremental, which fails §11 point 4 of the definition of done and — the
consequence nobody has written down — **forecloses F1's interactive tier**, which
§7.3 says runs on the same database. That is a much larger loss than a slow
compiler, and it should be checked before Gate H, not discovered at it.

**3. The derive hole makes it miserable rather than impossible.** Ninety types
needing hand-written `Clone`, `Eq`, `Hash` and `Inspect`, plus per-query memo
boilerplate, is a few thousand lines nobody wants to write or read. This does not
stop a bootstrap; it makes it unpleasant enough that it stalls, which is how most
bootstraps actually die.

**What abandonment leaves behind, which is not nothing.** In every case the Rust
compiler is still the compiler, still passing, still shipping — that is the whole
point of Decision 8's ordering, and it is why the Rust implementation is frozen
*after* Gate J rather than being progressively deleted. And whatever Science was
written becomes the largest real Science program in existence, which is a test
corpus of a kind the project cannot otherwise buy. A half-finished bootstrap is a
benchmark suite with good intentions. That is a survivable outcome and it should
be said out loud, because the fear of a wasted port is itself a reason projects
delay starting.

---

## 12. What this note deliberately does not do

- **It does not set a date, or a phase.** Every other note in this directory
  carries an F-number. This one carries gates instead, because they are mostly
  conditional on four crates that do not exist, and a phase label would be a
  guess wearing a uniform.
- **It does not design codegen, the type checker, MIR or region inference.**
  Those are four notes that should exist and do not (§14).
- **It does not amend core spec §6.** Decision 5's single-region rule is filed as
  an ask, not applied.
- **It does not propose writing the runtime in Science.** `science-rt` is the
  C-ABI floor both implementations stand on (§1.3); porting it would buy nothing
  and cost the layout tests.
- **It does not propose building the Science query database before Gate B2
  passes.** Decision 2 is a design, not a schedule.
- **It does not edit `examples/`, `tests/` or the README.** Decision 1 and the
  §14 asks are recommendations; this note is one file.

---

## 13. Diagnostics: this note claims none

**This note claims no `SC` code at all**, and it is the second to do so after
`package-manager.md`. The argument has the same shape as that note's, and it is
worth stating explicitly rather than leaving the README's allocation table to
infer it from an absent row:

> Self-hosting introduces no surface syntax, no new type, no new construct and no
> new phase. There is nothing for a user to get wrong that a compiler-phase
> diagnostic would describe. Every error in this note's territory is a *build*
> error — a seed hash mismatch, a `[build]` table mismatch, a differential
> disagreement between two implementations — and none of those happen while
> compiling a user's source.

Two places will eventually need a code, and **both already have an owner**:

- A stage-0 seed whose archive hash does not match is `package-manager.md`'s
  `SP0021`, unchanged. A `[build]` table mismatch is its `SP0022`, unchanged.
  Decision 9 adopts them rather than allocating new ones, which is the whole
  point of §10.2.
- Decision 5's rejection of a type needing two independent regions is an
  ownership diagnostic in `SC0300`–`SC0399`, and it belongs to whoever writes the
  region-inference note, not to this one.

The differential runner of §7 reports disagreements, and they are **test
failures, not diagnostics**. They render as a diff, the way `science-testkit`
already renders a UI-test failure.

---

## 14. What this note asks of the others

| Ask | Of | Why |
|---|---|---|
| **Fix `science-rt`'s §8 prefix** — the page says `link_`, the symbols are `science_` | `crates/science-rt/src/lib.rs` | Codegen emitted from that page, as it invites, emits undefined symbols (§1.3) |
| **Migrate `science-rt` §5 and §9 off `Result`/`Option`/`From`** | same | The layout rules survive; the names and the `?`-plus-`From` mechanism describe a language `syntax-revision-2.md` §3 deleted |
| **Specify how many inferred regions a type has** | core spec §6.2 | §4.4 commits to `type View: source: borrowed Doc` and §6.2 never says (§5.3). The evidence for "one" is that the existing compiler needs one |
| **Specify the MIR and region dump formats before the crates exist** | a new note owning MIR and regions | Decision 4; §5.1 is what happens otherwise |
| **Resolve the SHA-256 circularity** | `stdlib-standard.md` §13 | `package-manager.md` §2.2 needs SHA-256 inside `sciencec`; §13.1 says `crypto` wraps libsodium and implements nothing, so the compiler acquires a native dependency in order to build the compiler (§5.4) |
| **A derive mechanism** | the README's standing-asks table | A self-hosted compiler is the fourth customer and by volume the largest: ~90 types × four interfaces, plus Decision 2's per-query boilerplate (§4.4) |
| **`sciencec fmt` and a replayable migration tool** | `llm-ergonomics.md` §4.1 | Decision 6. Two notes already assume the formatter exists; nobody has built it |
| **Add a row to the README index and to the allocation table** | `docs/superpowers/design/README.md` | This note is in neither, and it claims no `SC` code — which the table should record the way it records `package-manager.md`'s (§13). **This note does not edit that file** |
| **Add `examples/21_compiler_shapes.science`** | `examples/` and its README | Decision 1. It is the highest-value item in this note and it is one file of Science |

---

## 15. Risks

**The biggest risk is that this note is read as permission to start.** It is not.
Gates A and C are years of work, and Gate B is the only one that should be
started now. A note that lays out a plausible staged plan invites somebody to
begin at stage one, and stage one here is a corpus file, not a lexer.

**§4 is analysis, not compilation.** Every "yes" in §4.8 is a reading of a
specification against a Rust source file. The borrow checker does not exist, so
none of it has been mechanically checked, and the history of region inference is
that the hard cases are the ones nobody thought to write down. Decision 1 is the
mitigation and it is the only one available.

**Decision 2 is the least-tested idea in the note.** "Rewrite salsa without
interior mutability by threading an exclusive borrow" is a plausible design that
has been written down once, here, and never built. It is load-bearing for
incrementality and, through core spec §7.3, for F1's interactive tier.

**The dump-format specification (Decisions 3 and 4) is the thing most likely to
be skipped**, because it is tedious, it produces no user-visible feature, and its
payoff is years away. It is also the cheapest item in the note and the one whose
cost curve is steepest: the token dump has already accreted `derive(Debug)` and
195 snapshots of it.

**§3's concession may be too generous.** The dogfooding argument was steelmanned
and it won, and it is possible it won too much: a compiler is an adversarial test
of region inference *as used by compiler writers*, which is not the same as a
test of region inference *as used by scientists*. A language could pass Gate B
and still make ordinary array code miserable, and nothing in this plan would
catch that. The scientific corpus is a separate obligation, and self-hosting does
not discharge it.

**The syntax argument cuts both ways and §6.3 may be wrong about which way.**
Decision 6 says mechanise migration rather than freeze the syntax. If migration
turns out not to be mechanisable — because a revision changes *meaning* rather
than spelling, which `syntax-revision-2.md` §10 explicitly warns about for the
error model, calling it *"the one change that is not merely syntactic"* — then the
gate is unreachable and the real answer was a freeze after all.

**Gate J is the first time anyone finds out whether codegen is deterministic
enough.** `package-manager.md` Decision 7 removes timestamps, hostnames and
absolute paths, which are the known sources. Hash-map iteration order in the
compiler itself is the unknown one, and `hir.rs`'s allocation-ordered `DefId` is
the only part of it that has been thought about.

---

## 16. Summary of decisions

| # | Decision | Cost |
|---|---|---|
| 1 | Extract the adversarial test from the port: a compiler-shapes corpus file, now | One more file to migrate per revision |
| 2 | The query database is a rewrite of salsa's idea — ids in, ids out, an exclusive borrow threaded through, the dependency stack as a field | Hand-written per-query boilerplate; no parallel queries; no upstream |
| 3 | The token, AST and resolve dumps are specified formats; the token dump stops using `{:?}` | A day's work, a spec to maintain, 195 snapshots re-blessed once |
| 4 | MIR and region dump formats are specified before those crates are written | Constrains two unwritten crates |
| 5 | A type has at most one inferred region in F0; filed as an ask, not applied | A two-region type is rejected; the existing compiler needs none |
| 6 | The syntax gate is mechanisability, not stability: `sciencec fmt` plus a replayable migration | A formatter and a migration tool before the first component port |
| 7 | The first Science written is the shapes file, not the lexer; the lexer is the first *component* and its job is to test the harness | None |
| 8 | The Rust compiler is frozen as **source** at Gate J, tagged, content-addressed; binaries are a convenience, never the only path | A frozen Rust tree that slowly bit-rots |
| 9 | The seed ships through `sciencec archive` and the directory registry; no bootstrap-specific machinery, and `SP0021`/`SP0022` are reused | Couples the bootstrap to the package manager's F1 schedule |

And the recommendation, restated because it is the answer to the question that
was asked:

> **We are further from self-hosting than the line count suggests — the compiler
> cannot emit a binary and four of its twelve crates do not exist — and we are
> closer than the architecture suggests, because the front end was already
> written in the flat, index-keyed, no-shared-ownership style that a language
> without `Rc` requires. The port should wait for codegen. The experiment should
> not wait for anything.**
