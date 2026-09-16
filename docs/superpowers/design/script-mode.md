# Science — Design: script mode, and why the grammar decision is F0's

Date: 2026-09-16
Status: draft for review
Amends: `docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §2 (adds a
fourth "F0 must not preclude" constraint), §4.4 and §4.5 (the top-level
production), §11 (definition of done).
Written against: `syntax-revision-2.md`. Every example below is in revision-2
syntax.
Related: `data-io.md` §7 and §10, whose worked example this note shortens;
`python-interop.md` §3.1, §3.3 and §9, whose extension-module case this note must
not break.

---

## 0. The thing being decided, which is smaller than it looks

Two questions get confused, and separating them is the whole note.

1. **Does Science have an interactive tier** — a JIT, a REPL, notebook cells,
   re-execution? That is a large question about the compiler's execution model,
   and §2 of the core spec answers it: **F1**. Nothing here disputes that.
2. **Does the grammar admit statements at the top level of a file?** That is a
   question about one production in `parse_module`. It has no runtime, no JIT and
   no REPL in it.

§2's phase table puts "the interactive tier (script mode, JIT, notebooks)" in one
cell, and the effect of that single cell is that question 2 has been answered
by accident, in the negative, by a decision that was only ever about question 1.

Today the parser says so out loud. `crates/science-parser/src/parser.rs`, in
`parse_impl`, attaches this note to the error it raises when a top-level line is
not a declaration:

> `a module holds declarations only; a statement belongs in a function body`

That sentence is a grammar commitment, it is in shipped diagnostic text, and it
was never argued for. This note argues the other side, and asks for the grammar
to change **in F0**, while the interactive tier stays exactly where §2 put it.

---

## 1. The proposal

### 1.1 The rule

**Decision.** The top level of a `.science` file is a sequence of *items and
statements*, in source order. Items are the declarations §4.4 already lists.
Statements are the statements §4.5 already defines — the same `Stmt` the parser
already produces for function bodies, with no new kinds and no new keywords.

```
module     := top_level*
top_level  := item | stmt
```

**This note reserves no new words**, adds no new statement form, and adds no new
expression form. It adds one alternative to one production.

### 1.2 What a script looks like

The whole of a program, with the core library of §8 and nothing else:

```science
let text, err be read_file("run.csv")
if err?:
    return err

print(text.lines().len())
```

Four lines. Under today's grammar it is six, and every line of the body is
indented one level under a `function main():` that carries no information.

### 1.3 Items and statements coexist

`use` declarations, `type`, `interface`, `function` and `Type has:` blocks are
legal anywhere in the file, interleaved freely with statements:

```science
use data.csv (read_csv, CsvOptions)
use data (Record)

type Reading:
    station: String
    temperature: F32

Reading implements Record

interface Describe:
    function describe(self) -> String

Reading implements Describe:
    function describe(self) -> String:
        self.station

let options be CsvOptions.new().header(true)

let frame, err be read_csv of Reading("run.csv", options)
if err?:
    return err

print("rows:", frame.len())
print(summary(frame.columns().temperature))

function summary(values: borrowed Array of F32) -> String:
    if values.len() is 0:
        return "empty"
    values.mean().to_string()
```

Note the last four lines: `summary` is *called above where it is declared*. That
is not a new rule; §3.2 explains why it already works.

### 1.4 The CSV script, honestly measured

The promise in the framing of this problem is "load a CSV, compute a mean and
print it, in five lines". Here is the real thing, against `data-io.md`'s API:

```science
use data.csv (read_csv, CsvOptions)
use data (Record)

type Reading:
    temperature: F32

Reading implements Record

let frame, err be read_csv of Reading("run.csv", CsvOptions.new().header(true))
if err?:
    return err

print(frame.columns().temperature.mean())
```

Eight non-blank lines, not five. Under a mandatory `main` it is ten, and all but
the first six are indented.

**The line count is not the argument, and pretending otherwise would be
dishonest.** The saving is one line and one indentation level. Three things are
the argument:

1. **Concept count before the first program.** To print a number, today's Science
   requires the reader to already know what a function is, what `main` means,
   why it is called that, and why the whole body is indented under it. Four
   concepts, none of which the program is about. A scientist who has written
   MATLAB for fifteen years has never needed any of them.
2. **The indentation, which in an indentation-delimited language is not
   cosmetic.** Every snippet a scientist copies from a docs page, a paper's
   supplementary material, or a colleague's message has to be re-indented to be
   pasted into a script. Python users do not do this, and the Python-shaped
   audience is exactly the audience §3 of the core spec chose indentation blocks
   to court. A language that takes Python's indentation and also requires a
   wrapper function has taken the cost and refused the benefit.
3. **The schema is already four lines.** `type Reading:` plus `Reading implements
   Record` is irreducible — it is what buys the typed column access that is
   `data-io.md` §1's entire argument for doing this in Science at all. Overhead
   that is *not* buying anything should be removed, because the overhead that
   *is* buying something is already at the audience's tolerance.

### 1.5 Where the five languages stand

| Language | Statements at top level | Entry point | Import runs them | Second file kind |
|---|---|---|---|---|
| Python | yes | first statement of the file | **yes** | no |
| R | yes | first statement | yes (`source`) | no |
| Julia | yes | first statement | yes (`include`) | no |
| MATLAB | yes | first statement | n/a | **yes** — script file vs function file |
| Go | no | `func main` | n/a | no |
| Rust | no | `fn main` | n/a | no |
| **Science (proposed)** | **yes** | **implicit `main`, entry file only** | **no — error** | **no** |

The four languages the audience actually uses all say yes. The two that say no
are systems languages whose users are not this audience. MATLAB is the one that
took the two-file-kind route, and §9 is about why that is the row not to copy.

---

## 2. The execution model

### 2.1 The script body is a function the user did not type

**Decision.** A file's top-level statements, in source order, are the body of an
implicit `function main() -> Error?`. Call it **the script body**. It is an
ordinary function everywhere below HIR: ordinary MIR, ordinary CFG, ordinary
region inference, ordinary drop glue.

This is the load-bearing decision of the whole note, because it is what makes the
change cheap. Everything from §7.1 of the core spec's pipeline onward —
`lowering`, `MIR`, region inference, ownership checking, monomorphization,
codegen — sees a function and cannot tell that the user did not declare it.
The change is confined to the parser, the AST, and one walk of the resolver.

*Serves:* both. Scientists get the syntax; programmers get a model with no new
runtime concept in it.

### 2.2 Both a script body and a `main`

**Decision.** A file with top-level statements *and* an explicit
`function main` is an error, `SC0117`.

```science
let x be compute()

function main():          # error[SC0117]
    print(x)
```

Alternatives rejected:

- **Statements run, then `main` runs.** This is Python's `if __name__ ==
  "__main__":` wart with the condition made implicit instead of explicit, which
  is worse: at least Python's version is visible in the source. It also makes
  the answer to "what ran first" depend on knowing a rule nobody will look up.
- **`main` wins; the statements are module initialisation.** Reintroduces a
  module-initialisation phase, which §4 then has to specify for every module, and
  §4's decision is that there is no such phase. One coherent answer beats two.
- **Statements win; `main` is dead.** Silently dead code. See §4.2 for why this
  project should not do that anywhere.

The diagnostic offers no automatically applicable fix, because there are two
reasonable ones and the compiler cannot choose: move the statements into `main`,
or delete `main` and let the statements be the script. It names both.

### 2.3 The entry point and the exit code

**Decision.** The script body's signature is `function main() -> Error?`.

| The script ends by | Exit code | stderr |
|---|---|---|
| falling off the end | 0 | — |
| `return` with no value | 0 | — |
| `return err`, `err` is null | 0 | — |
| `return err`, `err` is not null | 1 | `error: ` then the `Display` of the error |
| `panic(…)` | §8's abort, unchanged by this note | the panic message and location |

Falling off the end is an implicit `return null`.

Why `-> Error?` and not `-> ()`: because revision-2 §3 makes `if err?: return
err` **the** idiom for every fallible call, and a top level on which that idiom
does not typecheck is a top level on which the language's own error model cannot
be used. A script is mostly IO and IO is mostly fallible; the entry point has to
be fallible or every script's first line needs a workaround.

This ratifies something `data-io.md` §10 already assumed: its worked example is
`function main() -> DataError?`, an explicitly fallible `main` — the
concrete-error form of `function main() -> Error?`. §11.3 turns that into an
explicit ask of the core spec, because §11 of the core spec never says what
signatures `main` may have.

### 2.4 `return` at the top level

**Decision.** `return` at the top level is legal, means "stop the script", and
carries an optional `Error?`. Bare `return` is sugar for `return null`.

```science
let files, err be glob("data/*.csv")
if err?:
    return err                 # exits 1, prints the error

if files.len() is 0:
    print("nothing to do")
    return                     # exits 0
```

The alternative — making top-level `return` an error, and requiring `panic` or a
nested `if`/`else` cascade — was rejected for the same reason as `-> Error?`:
the early return *is* the error-handling idiom, and a script is where errors are
least tolerable to handle verbosely, because the author is a scientist who did
not want to handle them at all.

`break` and `continue` at the top level outside a loop are the same error they
are inside any function body. This note allocates no code for it because the case
already exists today and is not yet diagnosed anywhere; see §10.4.

*Serves:* scientists, decisively. A programmer would be content with `main`.

---

## 3. Ordering and scope

Three questions, and the third is the dangerous one.

### 3.1 Are statements executed in source order?

**Yes.** There is no hoisting of statements, no re-ordering, no dependency
analysis. A script reads top to bottom and runs top to bottom, which is the
single property the audience is coming for.

### 3.2 May a statement call a function declared below it?

**Yes, and this needs no new work, because it is already true.**

The resolver's own module documentation
(`crates/science-resolve/src/resolve.rs`) states the mechanism:

> Four walks, in this order, and the order is the whole reason forward
> references work: […] **Collect definitions.** Every item in every module gets
> its `DefId` and its entry in its module's name table, before a single body is
> looked at. A function may therefore call one declared later in the file, and
> two types may name each other, without any fixed point: by the time anything
> is *resolved*, everything is *known*.

Items are order-independent in Science today. Adding statements to the top level
does not change that: the collect walk skips statements, the bodies walk resolves
them last, and by then every item is known. §1.3's example, where `summary` is
called five lines above its declaration, works for free.

The practical consequence is the one that matters for the audience: **a script's
helper functions can go at the bottom**, where the reader reaches them after the
thing they are helping with, instead of at the top where MATLAB and C put them.
That is a real readability gain and it costs the compiler nothing.

*Serves:* both. Programmers get the ordinary module semantics they expect;
scientists get the file shape they want.

### 3.3 May a function declared below read a top-level `let` above?

**Decision. No. `SC0212`.** Items do not close over the top level. The top level
is not a scope any item body can see.

```science
let threshold be 0.5f32

function keep(value: F32) -> Bool:
    value > threshold          # error[SC0212]
```

There are three reasons, and the third is not a matter of taste.

**Reason one — it would make item bodies depend on execution order.** Today the
meaning of a function body is a property of the file. If it could read
`threshold`, its meaning would depend on *when it is called* relative to the
statement that binds `threshold`, and a function called before line 1 has run
would read an uninitialised binding. Every language that allows this pays for it
with an initialisation-order rule (C++'s static initialisation order fiasco;
Python's `NameError` at call time). Science can simply not have one.

**Reason two — it would put a hole in §5.2's local inference.** Signatures are
fully annotated and bodies are inferred; a body reading an unannotated script
binding would make one function's inference depend on another function's
inferred local, across a boundary §5.2 was drawn to keep closed.

**Reason three — it inverts the analysis order the region engine depends on, and
this one is a genuine cycle, not a preference.** §6.2 of the core spec:

> Functions are analyzed in reverse topological order of the call graph, and
> mutual recursion is resolved by a fixed point over each strongly connected
> component.

Reverse topological order means callees before callers. The script body calls
`keep`, so `keep` is analysed first. But `keep` would be reading `threshold`,
whose region is a fact about the script body's MIR — which has not been analysed
yet, because it is the caller. That is a dependency cycle *across* the call
graph, and the SCC fixed point does not cover it, because it is not a call cycle.
Fixing it means either a separate pre-pass that infers script-binding regions
before any function, or a fixed point over a graph that is no longer the call
graph. Both are new machinery in the one component the spec says must not be
rewritten later ("a solver written without provenance must be rewritten entirely
to gain it" — §6.2, the same warning shape).

**What is still allowed, and why the line is where it is.** A *closure* inside a
top-level statement captures script bindings normally, because the closure is
inside the script body — it is an ordinary capture of an ordinary local, exactly
as §4.6 already specifies, and F2's "closures capturing by reference" is
unaffected:

```science
let threshold be 0.5f32
let kept be readings.iterate().discard(each.temperature > threshold).collect()
```

So the line is: **closures see the top level, named items do not.** That is a
line a user can hold in their head, and it is the line the compiler's own
analysis order draws.

### 3.4 The diagnostic

```
error[SC0212]: `threshold` is a script binding and cannot be used inside a function
  --> analysis.science:4:13
   |
 1 | let threshold be 0.5f32
   |     --------- declared here, at the top level of the script
 …
 4 |     value > threshold
   |             ^^^^^^^^^ used inside `keep`
   |
   = note: a function is not part of the script's execution order, so it cannot
           see values the script computes
help: make it a constant, since its value does not depend on anything the script does
   |
 1 | const THRESHOLD be 0.5f32
   |
```

The `const` suggestion is **automatically applicable only when the initialiser is
a constant expression**. Otherwise the help is a note reading "pass it as a
parameter", which is not applicable because it changes the function's arity and
every call site.

*Serves:* programmers, at the scientist's expense. A Jupyter user defines a
function that reads a notebook variable every day of their life, and this
forbids it. That cost is taken deliberately and §7.3 says what F1 may do about
it. The direction matters: **relaxing this later is source-compatible;
tightening it later is not.**

---

## 4. Modules

### 4.1 Only the entry file's statements run

**Decision.** Top-level statements are executed **only in the entry file**. There
is no module-initialisation phase. Importing a module never runs anything.

Python, R and Julia all run an imported file's top-level code, and Python's
`if __name__ == "__main__":` exists solely to work around it. That idiom is on
the list of things this language exists to not have. The deeper objection is that
it makes `import` an *effect* — `use data.csv` could, in principle, read a file,
allocate, or panic — and a language whose §1 bet is verification should not have
a statement whose observable behaviour is invisible at its use site.

### 4.2 How the compiler knows which file is the entry

**Decision**, in the order the compiler tries them:

1. **The file named on the command line.** `sciencec run analysis.science` and
   `sciencec build analysis.science` make `analysis.science` the entry.
2. **`main.science` at the crate root**, when no file is named.
3. Otherwise, no entry — a library build, and §4.3 applies to every module in it.

This deliberately does **not** introduce a manifest key. §12 of the core spec
puts the package manager out of scope for F0, and an entry point that requires a
manifest would require the manifest. When a manifest arrives it should gain an
`entry` key that takes precedence over rules 1 and 2, and nothing else changes.

### 4.3 Statements in a module that is not the entry

**Decision.** Top-level statements in a module that is *reached by `use` in this
compilation* are an error, `SC0213`. Statements in a module that is simply not
compiled are not anybody's problem.

```
error[SC0213]: top-level statements in a module that is not the entry point
  --> clean.science:14:1
   |
14 | print("loaded", frame.len())
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ this would never run
   |
  ::: plots.science:2:1
   |
 2 | use clean (frame_for)
   | --------------------- `clean` is imported here, so it is a module, not a script
   |
   = note: Science does not run a module's statements when it is imported
   = help: move these statements into a function, or into the entry file
```

Note the exact scope of the rule, because it preserves the workflow that matters.
`sciencec run clean.science` compiles `clean.science` as the entry and its
statements run. Only when *another* file imports it does the same file become an
error — which is precisely the moment the question "do these run?" first has two
possible answers.

Alternatives rejected:

- **Run them once at first import** (Python, Ruby, Julia). §4.1.
- **Do not run them, and say nothing.** The user wrote code that never executes
  and the compiler knew. A language with UI tests comparing diagnostics byte for
  byte should not choose silence.
- **Do not run them, and warn.** `data-io.md` §10 already made this project's
  argument about warnings, calling `SettingWithCopyWarning` "a warning nobody
  reads". Either it matters or it does not.

*Serves:* both, but the error especially serves the scientist, who is the person
who will hit it — by writing two scripts in a directory and importing one from
the other — and who would otherwise spend an afternoon wondering why a `print`
did not print.

### 4.4 Directory modules

§4.4 of the core spec makes a directory with `mod.science` a module containing
its siblings. `mod.science` is never an entry file by rule 2 of §4.2 (which names
`main.science` specifically), and it is reached by `use` whenever the directory
is, so top-level statements in a `mod.science` are always `SC0213`. That is the
right answer and it needs no special case.

---

## 5. The Python extension case

`python-interop.md` §3.1 compiles a `.science` file into a CPython extension
module. Such a file has no `main` and never gets one: its entry points are the
functions marked `public to python`, and §3.3 specifies module initialisation as
multi-phase PEP 489 with a `Py_mod_exec` that does exactly four things and
"imports nothing".

**Decision.** Top-level statements in a file built with `sciencec build --python`
are an error, **`SC0458`**, in the `SC0450`–`SC0479` range that note claims.

```
error[SC0458]: a file built as a Python extension module cannot have top-level statements
  --> pipeline.science:9:1
   |
 9 | let calibration be load_calibration()
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ there is no script to run in an extension module
   |
   = note: an extension module's entry points are its `public to python` functions
   = help: move this into a function, or build without `--python`
```

Alternatives rejected:

- **Run them in `Py_mod_exec` as module-level initialisation.** This is the
  tempting one and it is wrong on four counts, all of which `python-interop.md`
  already argued for other reasons.
  - §3.3 makes "the module **imports nothing** at init, and has no Python
    dependencies at all" the single most important packaging decision in that
    note. Arbitrary user code at init can read a CSV, allocate a gigabyte, or
    fail — and an extension module that can fail at `import` time is a support
    burden that outlives every other decision here.
  - abi3 puts module state in per-module state (`PyModule_GetState`), not C
    globals, for §5.6's subinterpreter story. Script bindings are arbitrary
    Science values with `Drop` and possibly borrows; putting them in per-module
    state means specifying when module state is torn down and what happens to a
    `Drop` that runs during interpreter finalisation. That is a project.
  - §7.2 has to decide what a panic *during module init* becomes, and the answer
    ("an `ImportError`, maybe") is worse than the answer to every other panic
    question in that note.
  - A statement is not a natural fit for `Py_mod_exec` anyway: the four slots
    that note lists are declarative.
- **Ignore them silently.** §4.3.

### 5.1 One file, two build modes, two answers

This makes `SC0458` a **mode-dependent diagnostic**: the same file compiles as a
script and fails as an extension module. That is worth stating plainly rather
than discovering. It is not a new kind of thing — `python-interop.md` §9 already
allocates `SC0455` for "`use python` in a standalone (non-hosted) build", which
is exactly the same shape: a legal construct that one build mode refuses.

The practical advice, which belongs in the interop note's prose: **a file you
intend to ship to Python should have no script body.** The natural project shape
is one entry script for the scientist's own runs and a separate exported module,
which is the shape `python-interop.md` §8 already shows.

---

## 6. Ownership, regions, and `Drop` at the top level

This is the part most likely to be waved through, and it is where the design
either costs nothing or costs a rewrite.

### 6.1 Script bindings are locals, not statics

**Decision.** A top-level `let` binds a **local of the script body**. Its scope
is the rest of the top level. It is not a static, not a global, and has no
special storage class.

The alternative — treating top-level bindings as program-lifetime statics — is
the one that looks natural and is expensive:

| | Local of the script body | Static |
|---|---|---|
| Region inference (§6.2) | unchanged; an ordinary CFG | needs a `'static` region and a rule for it |
| §6.1 rule 6, "leaving scope destroys a value" | applies; the scope is the script body | no scope to leave; needs a separate teardown concept |
| `Drop` | ordinary drop glue at end of block | needs an at-exit registry and an order |
| Initialisation order | statement order, trivially | a new analysis |
| `Copy`/move rules (§6.1) | unchanged | a static that moves is a new question |
| Work in `science-regions` | **none** | substantial |

Statics buy exactly one thing — the ability for item bodies to read them — and
§3.3 already forbids that for independent reasons. With that gone, the static
interpretation has no benefit left and all of its cost.

### 6.2 When a script binding drops

**Decision.** At the end of the script body, in reverse declaration order, before
the process exits. This is §6.1 rule 6 with no amendment.

It matters concretely, because `data-io.md` §7's cleanup story depends on it:

```science
let scratch, err be temporary_directory()
if err?:
    return err

let out be scratch.path().join("intermediate.parquet")
let write_err be write_parquet(frame, out, ParquetOptions.new())
if write_err?:
    return write_err

print("wrote", out)
# `scratch` drops here, at the end of the script, and the directory goes with it
```

`data-io.md` §7 calls this "the small place where ownership visibly beats every
dynamic language: cleanup happens at a program point the compiler names". The
point the compiler names for a script is the end of the file, and a script
author gets it without writing `try`/`finally`, `with`, or `on.exit` — which is
the single most persuasive thing this language can show an R or Python user in
five lines.

**Two honest caveats.**

- `return err` on line 3 drops `scratch` too, on the early path. That is correct
  and it is exactly what `try`/`finally` is for; it is worth saying because it is
  the case a reader will wonder about.
- **A panic does not run drops**, because §8 specifies panic as abort. So a
  panicking script leaves its temporary directory behind. This is already true
  of every `main` today and script mode changes nothing about it — but it
  interacts with `python-interop.md` §11's ask that `panic` be routed through a
  single runtime entry point so it *can* become an unwind later. If that ask is
  honoured, scripts get cleanup-on-panic for free the day it lands.

### 6.3 Borrows that live the length of a script

A top-level borrow has the longest region any borrow in the program can have:

```science
let doc be load("a.txt")
let view be View(source: doc)      # borrows `doc` for the rest of the script

consume(doc)                       # error[SC0300-range]: `doc` is still borrowed
```

Nothing new is needed to detect this — it is the ordinary conflict of §6.1 rule 4
in the ordinary CFG of §6.2. But **the diagnostic's rendering is not ordinary**,
and this is the one concrete ask this section makes of the diagnostics layer:

> A region that ends at the end of the script body must render as "**the end of
> the script**", and the enclosing function must render as "**the script**", not
> as "`main`".

If that is missed, the first borrow error a scientist ever sees will say "borrow
is live until the end of `main`" about a `main` they did not write, and §6.2's
whole argument — "an error must reconstruct where the borrow was born, what keeps
it alive, and where it conflicts" — fails at its first contact with the audience
it was written for. It is a rendering rule, it is cheap, and it is the kind of
thing that is free now and never gets done later.

### 6.4 What §6 does *not* have to change

Stated as a list, because the point of §6.1's decision is that this list is the
whole answer:

- No new region kind.
- No change to the outlives-constraint collection.
- No change to the interprocedural order of §6.2, given §3.3's ban.
- No change to `borrows_live_at(point)`, which §6.4 owes F5.
- No change to drop glue, monomorphization, or codegen.

The region engine is handed a function. It does not know the user did not type
the signature.

---

## 7. What this does to F1

### 7.1 A cell is a fragment of the top level

The interactive tier's unit of work is a cell, and a cell contains statements. If
the top level of a module is a list of items, a cell has no representation in the
module at all, and the REPL must carry **its own grammar** — one that admits
statements at the top level. That is the §9 alternative, arrived at from the
other end: two grammars, one of them reachable only from the REPL, with all the
divergence risk and none of the benefit.

So the question is not *whether* Science gets a statement-admitting top level. F1
builds one or F1 has no cells. The question is only whether anything but the
REPL can use it.

### 7.2 The query architecture lines up, and only under this design

§3 of the core spec:

> Query architecture (`salsa`) […] the mechanism the interactive tier is built
> from: re-running a cell is invalidating a query.

Make that concrete. A notebook of *n* cells has two possible query shapes:

- **(a) The notebook is one input** whose text is the concatenation of the cells.
  Editing cell *k* invalidates `parse(file)` and everything downstream of it —
  the whole notebook. This is incremental in name only, and it is the shape you
  are forced into when the module has no sub-unit smaller than itself.
- **(b) Each cell is its own salsa input**, and the module's top-level list is
  the concatenation of the cells' top-level lists. Editing cell *k* invalidates
  `parse_cell(k)`; resolution and typing are invalidated for statements
  *downstream* of *k* and for nothing upstream.

(b) is what §3's sentence promises, and **(b) is only expressible if the module's
top level is a list you can concatenate.** A list of statements concatenates. An
items-only module does not have cells in it to concatenate.

### 7.3 Easier or harder?

**Easier, in three specific ways, and harder in one.**

Easier:

1. **F1 inherits the grammar instead of building it**, and does not migrate a
   corpus while also building a JIT.
2. **§3.3's ban is what makes cell invalidation tractable.** Because no item body
   reads a script binding, invalidating a script binding invalidates *statements
   only*, never a function body. The dependency edges from cells go forward into
   cells, and never sideways into items. Without the ban, editing a cell that
   binds `threshold` would invalidate the typing of every function in the
   notebook.
3. **Out-of-order execution becomes diagnosable rather than mysterious.** The
   program is the statement list in order; running cell 5 then cell 2 means the
   live state no longer matches any prefix of it. The tier can say so. Jupyter's
   central usability failure is that it cannot, and this is the cheapest
   opportunity Science will get to beat it.

Harder, and it is the honest cost: **F1 will want to relax §3.3.** Defining a
function in one cell that reads a variable from another is what people do all
day in Jupyter, and the ban forbids it. F1 may relax it — for instance by
allowing an item body to read a script binding *only in the interactive tier*,
where a cell already has a defined execution order and the region engine is
running per-cell rather than over a whole-program call graph. That relaxation is
source-compatible with this note. The reverse — shipping the permission in F0 and
withdrawing it in F1 — is not, which is the whole reason to start strict.

---

## 8. Diagnostics

Codes are allocated by **the phase that detects the error**, following the
precedent §9 of the core spec set for itself when it moved exhaustiveness errors:

> Exhaustiveness errors belong to the type checker and so take a code in the
> `SC0250` range, not the `SC0210` the previous spec assigned them.

| Code | Range | Detected by | Meaning | Applicable fix |
|---|---|---|---|---|
| `SC0117` | syntax | parser | a file has both a script body and a `function main` | no — two readings, both named in the note |
| `SC0107` | syntax | parser | `public` on a top-level statement | yes — remove `public`. **Existing code, reused** |
| `SC0101` | syntax | parser | expected a declaration | **existing code, message must change**; see §8.1 |
| `SC0212` | resolution | resolver | an item body names a script binding (§3.3) | yes, when the initialiser is a constant expression: rewrite `let` as `const` |
| `SC0213` | resolution | resolver | top-level statements in a module reached by `use` (§4.3) | no — note only |
| `SC0458` | codegen | driver | top-level statements with `--python` (§5) | no — note only |

`SC0117` and the reuse of `SC0107` are the only claims on the syntax range.
`SC0212` and `SC0213` take the first two free codes after `SC0211`. `SC0458`
takes the first free code in `python-interop.md` §9's claimed block, which that
note already flags as needing reconciliation with its siblings.

### 8.1 One existing diagnostic becomes false

`SC0101` (`EXPECTED_ITEM`) currently carries this note, emitted from `parse_impl`:

> `a module holds declarations only; a statement belongs in a function body`

The first clause stops being true the day this note lands, and a UI test
comparing rendered output byte for byte will catch it — which is the system
working. The replacement message must distinguish the two remaining cases:

- an identifier at the top level followed by neither `implements` nor `has` nor
  anything that continues an expression or an assignment — genuinely malformed;
- a top-level line that is a well-formed statement — now accepted, and no
  diagnostic at all.

### 8.2 The one place the grammar could be ambiguous, and why it is not

Both an implementation header and a statement may begin with an identifier:

```science
Doc has:                    # item
Doc implements Summarize:   # item
doc be Doc.new("a")         # statement (assignment)
doc.save()                  # statement (expression)
```

**Rule.** A top-level logical line beginning with an identifier is an
implementation header **if and only if** the reserved word `implements` or `has`
occurs in it at bracket depth zero before its `:` or its end.

This is exact, not heuristic, because `implements` and `has` are reserved words
(§13, and revision-2 §7 keeps `has` reserved while freeing `methods`), so neither
can occur anywhere in an expression, a path, a type or an assignment target. The
test is one linear scan of one logical line, with no backtracking, no speculative
parse, and no diagnostic suppression. It runs only on lines that begin with an
identifier; every other top-level line is decided by its first token.

The corresponding obligation, which §10 records as a risk: **if any future syntax
change lets `implements` or `has` appear in expression position, this rule breaks
silently.** A test asserting that both words are rejected in expression position
is what keeps it honest.

---

## 9. The alternative: keep `main`, and give scripts their own file kind

This is the serious alternative and it deserves the argument, not a dismissal.
Two variants: a second extension (`.sci`), or a mode (`#!` line, or
`sciencec run --script`).

### 9.1 What it gets right

- **The `.science` grammar does not change.** No parser work, no AST change, no
  snapshot churn, no `ItemDefs` variant, nothing.
- **There is one obvious place for program logic**, and a reviewer opening a
  `.science` file knows what shape it has.
- **It is impossible to accidentally write a four-hundred-line script.** The
  file extension is a speed bump in front of the failure mode where an
  exploratory script becomes production code without ever being factored.
- **Precedent exists.** Racket's `#lang`, shell-vs-C, MATLAB's script files.
- **Diagnostics can be tailored.** A `.sci` file could have its own, friendlier
  set of messages aimed squarely at beginners.

That is a real case. It is why this section exists rather than a sentence.

### 9.2 Why it loses

**The two grammars do not stay apart.** Work through what a `.sci` file needs,
using the audience's own first program — `data-io.md`'s CSV reader:

- It needs `type Reading:` to declare the schema. §5.1 of that note makes the
  record type the *only* way to type a CSV; there is no schema literal. So `.sci`
  admits `type`.
- It needs `Reading implements Record`. So `.sci` admits `implements`.
- It needs `use data.csv (read_csv, CsvOptions)`. So `.sci` admits `use`.
- It needs a helper function within about twenty lines of real work. So `.sci`
  admits `function`.
- `data-io.md` §10's worked example, the canonical script, uses four of those.

At which point `.sci` admits every item `.science` admits, **plus statements** —
which is exactly the grammar §1.1 proposes, reached through a second file
extension that bought nothing. The second grammar is not a smaller grammar. It
is the same grammar plus one production, given a different file name.

**And then the costs arrive that the single grammar does not have:**

1. **A promotion step.** The day a script becomes a library, the user renames the
   file *and* moves code. Under §1.1, they delete the statements or move them to
   another file; the items stay where they are and nothing is renamed.
2. **`use` across the boundary needs a rule.** May a `.science` file `use` a
   `.sci` file? If yes, §4's whole question reappears with a file extension
   attached to it. If no, a scientist cannot reuse their own script's function
   without retyping it into a second file — which is precisely the workflow this
   language is supposed to smooth.
3. **Every tool doubles its file-kind logic**: the language server, `sciencec
   fmt`, the test harness, the module resolver of §4.4, and the future package
   manager.
4. **MATLAB is the evidence.** It is the one mainstream language in the
   audience's world that took this route, and its script-vs-function rule — a
   file is a function file if its first non-comment token is `function`, and a
   script may only have local functions *at the end* — is a reliable source of
   confusion for exactly these users. The ordering wart in that last clause is
   §3.3's question, answered badly, by the language that already ran the
   experiment.

### 9.3 The mode variant is worse

A `#!` line or a `--script` flag means **the same file parses differently
depending on something outside the file**. The language server does not know the
flag. `sciencec fmt` does not know the flag. And §10.3's byte-exact UI tests stop
being a coherent discipline, because a test file's expected output depends on an
invocation the file does not record.

This is the same objection `syntax-revision-2.md` §8.4 raised against inline
foreign code, and it should carry the same weight here that it carried there:
*"a syntax error in the fragment surfaces as a Science diagnostic that Science
did not produce"* becomes *"a diagnostic that depends on a flag the file does not
mention"*. The project has already decided it does not like this shape.

### 9.4 What is kept from the alternative

Two things, because §9.1's case is not empty:

1. **The naming convention.** `main.science` is the entry file when none is
   named (§4.2). It gives the reviewer the signal the extension would have
   given, at no grammatical cost.
2. **A lint, not a rule.** A script body past some threshold of statements — the
   number is a tuning decision, not a design one — should suggest factoring into
   functions. A lint is reversible and a grammar is not, which is the right way
   round for a judgement call about style.

---

## 10. The timing argument, stated rigorously

The ergonomic case in §1 gets to "we want this eventually". This section is the
argument that eventually means now, and it is deliberately separate, because the
two arguments have different strengths and conflating them weakens both.

### 10.1 The precedent is in the spec already

§2 of the core spec carries three "what F0 must not preclude" constraints. The
first one is this note's argument, verbatim, about a different feature:

> **F1 needs const generics and integer generic arguments.** […] This is why
> const generics are in F0 rather than deferred: retrofitting them means
> reopening the parser, the resolver, and every snapshot.

Const generics are *in* F0 although shapes are F1, because the grammar and the
resolver are where retrofitting is expensive. That is the identical situation:
script mode's *runtime* is F1, its *grammar* is a parser and resolver change, and
retrofitting it means reopening the parser, the resolver, and every snapshot.
This note asks for a fourth bullet of the same shape.

### 10.2 What changes, now versus later

| | Now (F0) | Later (F1+) |
|---|---|---|
| `ast::Module.items: Vec<Item>` → a two-variant top-level enum | one field, one enum | same |
| `parse_module`'s loop: one predicate (§8.2) and a call to the existing `parse_stmt` | no new parsing code — `parse_stmt` exists and is exercised by the parser suite | same |
| `parser::dump` and every AST-dump snapshot containing a module header | churn | same churn, over a corpus an order of magnitude larger |
| Resolver walk 2 skips statements; walk 4 opens a top-level rib; `ItemDefs` gains a variant to keep the arrays index-aligned | a named, bounded change | same |
| HIR: a body for the script body | small | same |
| MIR, regions, codegen | **zero** (§2.1, §6.4) | zero |
| The corpus: `examples/`, `tests/ui/`, execution tests | rides along with the revision-2 migration that is already in flight | a migration of its own, scheduled against F1's JIT work |
| `SC0101`'s message and note (§8.1) | one string, one UI test | same string, more UI tests |
| **The meaning of existing programs** | nothing has users | **a breaking grammar change** |

The last row is the one that is not a matter of degree.

### 10.3 It is a breaking change, in the project's own sense of the word

Today `x be 5` at the top level of a file is `SC0101`, an error. Under §1.1 it is
a statement. **A construct that is a diagnostic today and valid code tomorrow is
the definition of a breaking grammar change**, and it is the exact shape of the
argument §13 of the core spec makes for reserving words it does not use:

> Reserving costs nothing now and breaks every program using the name later.

The top-level production is the same asset as a reserved word: cheap to decide
before anyone depends on it, and permanently expensive afterward. §13 already
spends real ergonomics — `x.shape` does not compile — to buy this property for
F1's keywords. Buying it for F1's grammar costs less and buys more.

### 10.4 What is *not* being asked for

To keep the ask honest and small, here is what this note explicitly does not
want in F0:

- No JIT, no REPL, no notebook protocol, no cell model. §2's F1 row stands.
- No incremental *execution*. §7.3's query shape is a description of what becomes
  possible, not a request to build it.
- No relaxation of §3.3. That is F1's to consider.
- No new library surface, no `exit(code)`, no argument vector. §10.5.
- No diagnostic for `break` outside a loop. That gap exists today, inside
  function bodies, and is not this note's to fill.

### 10.5 Deliberately out of scope

- **`exit(code)`** for a script that wants an exit code other than 0 or 1. §8 of
  the core spec declares the free-function list closed, and this note does not
  open it. `return err` covers the failure case.
- **Command-line arguments.** A script that wants `argv` has nothing here. It is
  a library question, and it belongs wherever `data-io.md`'s successor puts the
  process environment.
- **Shebang lines.** A leading `#!` is a `#` comment under §4.2 and therefore
  already lexes correctly; making it *executable* is a filesystem-permissions
  and interpreter-dispatch question, not a language one.
- **Top-level `await`, `spawn`, or anything from F3+.** Those phases decide what
  their constructs mean at the top level when they arrive.

---

## 11. What this note asks of the core spec and the other notes

1. **Core spec §2: a fourth "F0 must not preclude" bullet.** "F1's interactive
   tier needs a top level that admits statements. Retrofitting it means reopening
   `parse_item`, the module type, the resolver and every snapshot." Same shape as
   the const-generics bullet, for the same reason.
2. **Core spec §4.4/§4.5: the grammar production of §1.1**, and the
   disambiguation rule of §8.2 stated as a rule rather than left to the
   implementation.
3. **Core spec §11: what signatures `main` may have.** §11 never says. This note
   needs `function main()` and `function main() -> Error?` to both be legal, and
   `data-io.md` §10 already assumed the latter. §11's definition of done should
   also gain one script program — a file with no `main` that compiles, runs, and
   returns a nonzero exit code from a top-level `return err`.
4. **`syntax-revision-2.md`: is `Error` a concrete type or an interface?** §3
   writes `-> (Config, Error?)` throughout without saying which. The script
   body's return type is `Error?`, so the answer lands here: if `Error` is an
   interface it is presumably `any Error?` and the script body boxes; if it is a
   concrete type, every library error must convert to it, and revision-2 §3.2
   explicitly deleted `From` widening. This is the largest unanswered question
   this note depends on and it is not this note's to answer.
5. **`syntax-revision-2.md`: how does `mutable` distribute over a destructuring
   `let`?** §3.1 shows `let config, err be read_config(path)` and never shows a
   mutable one. `data-io.md` §10 needs `let mutable frame, err be …` for its
   `columns_mutable()` call. Whether `mutable` binds the first name or both is
   unspecified, and a script hits it on the first line that needs a mutable
   frame.
6. **`data-io.md` §10: rewrite the worked example as a script.** **Half done.**
   The syntax half has since landed — that section is now revision-2 throughout,
   including the error model — so §12.1's contradiction is closed. What remains
   is the *shape*: §10 is still a `function main()` program rather than a script
   body, and §12.2 below still shows the script rewrite this item asks for.
7. **`data-io.md` §11: a note that `.mean()` is assumed.** §11.4 of that note
   already flags `square_root()` and the reductions as belonging to a numerics
   note. §1.4 here leans on `.mean()` for the shortest honest CSV example, so the
   numerics answer determines what that example looks like.
8. **`python-interop.md` §3.3 and §9: claim `SC0458`**, and add a sentence to
   §3.3 saying that an extension module has no script body and why (§5).
9. **`science-diagnostics`: the rendering rule of §6.3.** "The script" and "the
   end of the script", never "`main`", for a function the user did not write.
10. **No change to §13.** This note reserves nothing. Recorded explicitly,
    because every other syntax note in this directory has had to trade against
    the reserved-word list and this one does not.

---

## 12. Contradictions found

Stated rather than papered over, per §0 of the house convention.

### 12.1 `data-io.md` §10 was written in revision-1 syntax — now closed

The canonical worked example used `for each`, `println`, `Result of ((),
DataError)`, `try`, `is at least`, `is not None`, `has methods`, `returns` and
`Option of F32` — all of which `syntax-revision-2.md` removed the same day, so the
single most-cited piece of Science code in the design directory did not parse
under the syntax the project had adopted. **That has since been fixed in
place**: `data-io.md` §10 is now revision-2 throughout and its fallible calls are
written as pairs with `if err?:`.

Two things the fix surfaced rather than settled, both of which §11 already
asks for and neither of which is this note's to answer. First, §10's `main`
called `glob`, whose `GlobError` has no variant in `DataError`; under revision 1
the `try` there depended on the `From` widening §3.2 of the revision note deleted,
so the signature is now the interface form `-> ((), Error?)`. Second, the example
needs `let mutable frame, err be …`, and whether `mutable` distributes over a
destructuring `let` is still unspecified (§11.5).

### 12.2 The same example, in revision-2 and as a script

Offered as the concrete version of §11.6, and as the demonstration that this
note's whole ergonomic claim is true of the project's own canonical program:

```science
use fs (Path, glob)
use data (Frame, Record, BadRow)
use data.csv (read_csv, CsvOptions)
use data.parquet (write_parquet, ParquetOptions, Compression)

type Measurement:
    station: String
    timestamp: I64
    temperature: F32
    humidity: F32?

Measurement implements Record

let files, err be glob("measurements/2026-09-*.csv")
if err?:
    return err

let options be CsvOptions.new()
    .header(true)
    .delimiter(',')
    .null_value("")
    .null_value("NA")
    .on_bad_row(BadRow.Skip(500))

let mutable frame, err be read_csv of Measurement(files, options)
    .rows()
    .keep(each.temperature >= -80.0f32)
    .keep(each.temperature <= 70.0f32)
    .keep(each.humidity?)
    .collect()
if err?:
    return err

print("kept rows:", frame.len())
print("rejected rows:", frame.rejected_count())

normalize(frame.columns_mutable().temperature)

let destination be Path.from("measurements").join("clean.parquet")
let write_err be write_parquet(frame, destination, ParquetOptions.new()
    .compression(Compression.Zstd(3))
    .row_group_rows(1_000_000))
if write_err?:
    return write_err

function normalize(values: mutable borrowed Array of F32):
    let count be values.len() as F32
    if count is 0.0f32:
        return

    let mutable total be 0.0f32
    for v in values:
        total be total + v
    let mean be total / count

    let mutable squares be 0.0f32
    for v in values:
        squares be squares + (v - mean) ** 2
    let deviation be (squares / count).square_root()
    if deviation is 0.0f32:
        return

    for i in 0..values.len():
        values.set(i, (values.get(i) - mean) / deviation)
```

The helper is at the bottom, where a reader meets it after the pipeline it
serves (§3.2). The pipeline is at the left margin. `normalize` takes the column
as a parameter and could not have read it from the top level anyway (§3.3), so
the restriction costs this program nothing — which is the general case, and is
why §3.3 is the right default.

### 12.3 `python-interop.md` §3.3's "one file, one module" and §4.2's entry rule

§3.3 says "One `.science` file maps to one module of the same name", and a
directory module maps to one extension module for the whole package. §4.2 here
says the entry file is the one named on the command line. These do not conflict,
but they meet: `sciencec build --python spectra/mod.science` names a file that
§4.2 would otherwise call an entry. §5's rule resolves it — under `--python`
there is no entry and no script body, for any file — but the interop note should
say so rather than leaving a reader to derive it.

### 12.4 Core spec §12 lists "the interactive tier" as out of scope for F0

Read as written, §12 puts script mode out of scope, since §2 files it under the
interactive tier. This note disagrees with the grouping, not the phasing, and
§11.1 asks for §2 and §12 to be amended together so that "the interactive tier"
means the JIT, the REPL and notebooks, and does not silently also mean a
production in the grammar.

---

## 13. Risks

**The relaxation pressure on §3.3 will be constant.** Forbidding an item body
from reading a script binding is correct for the reasons in §3.3 and it will feel
wrong to every user who has ever used a notebook. The mitigation is that the
direction is right — relaxing is source-compatible — but the pressure will start
on day one and the answer has to be ready.

**`SC0140` at the top level is where the verification bet meets the audience.**
Revision-2 §3.3 makes an unchecked error a compile error. A script is mostly IO;
IO is mostly fallible; so **the most likely reason a scientist's first
five-line Science program does not compile is that they ignored an error.** That
is the language working exactly as designed and it will not feel that way. This
is the single biggest adoption risk in this note, it is deliberate, and it should
be met with the best diagnostic in the compiler rather than with a relaxation.

**§4.3's error will be the most-complained-about rule here.** Making imported
statements an error rather than dead code is the strict choice, and the moment a
user has two scripts in a directory and wants to import one from the other, they
meet it. The reversal direction is safe (allow, do not run, warn), which is why
strict is where to start — but expect to defend it.

**§8.2's disambiguation rule is exact only while `implements` and `has` are
unusable in expressions.** Nothing today threatens that, and nothing should be
allowed to without noticing. A test asserting both words are rejected in
expression position is what keeps the rule from becoming a heuristic quietly.

**Mode-dependent diagnostics are a category users find surprising.** `SC0458`
joins `SC0455` in the set of "legal here, illegal under this flag". Two is fine.
A language with fifteen of them has a configuration problem, and this is the
second, so the count should be watched.

**The script body's diagnostics have no user-written span.** §6.3 names the case
that matters most, but it is not the only one: every message that names an
enclosing function, every backtrace frame, every "in function `…`" needs a
rendering for a function nobody wrote. Missing one produces a message about
`main` to a user who has never typed the word, and the failure mode is confusion
rather than a crash, which means it survives testing.

**Snapshot churn, for the third time in a day.** Every AST-dump snapshot with a
module header changes shape. `syntax-revision-2.md` §10 already took this cost
and warned that "it is the last day that argument is free". This note is asking
for the same day's ride-along and it is the last thing that should get one.
