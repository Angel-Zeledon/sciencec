# Writing Science

Science is a compiled language for scientific computing and machine learning.
Its syntax rhymes with Rust and Python and **is not either of them**. If you
write it by analogy you will get it wrong in the specific ways below.

Read §1 before writing a line. The compiler is the authority: **every row and
every sample in this file was run through `sciencec check`** (and §5's program
through `sciencec test`) before it was written down, and §6 says how to do the
same with yours. `examples/` holds 22 programs that are guaranteed valid —
imitate them. The design is `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
§4, but its §4.3 table still shows the spellings the bracket revision replaced
(`borrowed T`, `Array of T`, `def largest of T`); where it and the parser
disagree, the parser and `examples/` are right.

> **The corpus's number lives in one place.** `UNMEASURED` in
> `crates/sciencec/tests/corpus_output.rs` names every example that is *not*
> measured and why; `tally` beside it derives the count from that table and the
> directory listing. **20 of 20 examples** build, link, run and have their
> output pinned byte for byte — 22 files less the two that are not programs.
> Every number quoted below is read back against that table by
> `no_document_quotes_a_corpus_count_this_file_does_not`, so a stale one here
> is a failing test and not a surprise for the next reader.

---

## 1. The translation table

The left column is how Rust and Python spell it, and all of it is a compile
error in Science. Where the last column names a code, the diagnostic names the
right form and offers it as a fix. Where it does not, you get a plain parse
error — usually `SC0100 expected end of line` at the word *after* the one that
is wrong — so a line starting `fn`, `struct`, `enum`, `impl` or `pub` that
fails to parse is this table, not a typo.

| Don't write | Write | |
|---|---|---|
| `fn f()` | `def f()` | |
| `function f()` | `def f()` | the keyword until revision 3; `SC0156` |
| `let x = v` | `let x be v` | `SC0100`, "expected `be`" |
| `let mut x = v` | `let mutable x be v` | `SC0100`, fix: `mutable` — `mut` exists only in `&mut T` |
| `x = v` | `x be v` | assignment is also `be`; `SC0100`, fix: `be` |
| `struct Doc:` | `type Doc:` | |
| `enum Format:` | `choice Format:` | |
| `impl Summarize for Doc:` | `Doc implements Summarize:` | **the order reverses** |
| `impl Doc:` | `Doc has:` | |
| `trait Summarize:` | `interface Summarize:` | `SC0139` |
| `&self` / `&mut self` | `self` / `mutable self` | by value is `self: Self` |
| `dyn Trait` | `any Interface` | `&any Shape`, `Box[any Shape]` |
| `while cond:` | `loop:` + `break` | **`while` is gone**; `SC0142`; see §2 |
| `for i in range(n):` | `for i in 0..n:` | there is no `range`; `SC0200` |
| `println(x)` | `print(x)` | `SC0144`; no-newline form is `write(x)` |
| `f()?` (Rust's propagation) | a pair and a check | **not** a `?`; see §2 |
| `Result[T, E]` | `-> (T, E?)` | the error comes back beside the value; `SC0200` |
| `Option[T]` | `T?` | `null` is the absent case; `SC0200` |
| `import io` | `use io` | `import` is reserved; `SC0101` |
| `pub` | `public` | |
| `!flag` | `not flag` | `SC0001` |
| `a == b` / `a != b` | `a is b` / `a is not b` | `SC0016` / `SC0017`; see §4 |

These are **the same as Rust** now, and the table used to say otherwise:

| Rust, and Science | Old Science spelling, now refused |
|---|---|
| `&T`, `&x` | `borrowed T`, `borrowed x` |
| `&mut T` | `mutable borrowed T` |
| `Array[T]`, `Map[K, V]`, `Box[any I]` | `Array of T`, `Map of (K, V)` |
| `def largest[T: Ord](…)` | `def largest of T: Ord(…)` |
| `type Pair[A, B]:`, `Wrapper[T] has:` | `type Pair of (A, B):` |
| `interface Convert[Source]:`, `Load implements From[I64]:` | `From of I64` |

Each old spelling is `SC0100` with a message that names the new one —
"`borrowed T` is now written `&T`", "`Array of T` is now written `Array[T]`".
`borrowed` and `of` stay keywords only so that message can be given, so
neither is usable as a name. Not everything `docs/DREAM.md` Part II proposed
was adopted: `let mut`, `Option[T]`/`Result[T, E]`, `import` and `range(n)`
were not, and are the rows above.

---

## 2. The six rules you cannot guess

These have no Rust or Python analogue. They are where generated code fails even
after the table above is applied.

**1. `be` binds and assigns.** `let x be 1` introduces a name; `x be 2` assigns
to one that exists. `=` is not an assignment operator at all.

**2. Calls always take parentheses.** `text.length()`, never `text.length`.
Parentheses mean call; their absence means a field. `x.dims` is a field access,
`x.dims()` is a method call, and they are different things.

**3. Generic arguments go in brackets, and nest.** `Array[Doc]`,
`Map[String, Int]`, `Array[Map[String, Int]]`. An associated call on a generic
type needs no parentheses around the type — `Array[Doc].new()`,
`Map[String, Array[Int]].new()` — because the `]` closes the argument list on
its own. A bare `Array.new()` with nothing to fix `T` is `SC0536`.

**4. A function that can fail returns its value and an error**, and `?` tests
the error for presence. `?` is *postfix* and binds the whole chain to its left,
so `a.b()?` is `(a.b())?`; covering less takes parentheses. It is a total
operator — it produces a `Bool` and cannot return from the function — which is
the whole difference from the `try` it replaced.

```science
def load(path: &String) -> (String, IoError?):
    let text, err be read_file(path)
    if err?:
        return ("", err)
    (text, null)
```

The error may be concrete, as here, or the interface `Error?` — shorthand for
`(any Error)?`, which boxes. The concrete form is what builds today when the
error is a prelude type: boxing an `IoError` or a `TextError` into `Error?`
checks clean and is refused by the backend (`SC0400`). A record of your own
that implements `Error` boxes fine — §5 does it.

`?` looks like Rust's propagation operator and is not it. `let n be g()?`
binds a `Bool`, and the error surfaces wherever `n` is used as a number
(`SC0535`, "`Bool` does not implement `Add`"). An error bound and never tested
is `SC0140`; name it `_err` when ignoring it is deliberate.

**5. Blocks are indentation, and an inline body ends where the expression ends.**
`if a: b else: c` works because `else` cannot continue an expression. An inline
body may be an expression, an assignment, or `return` / `break` / `continue` —
but **not** a `let` (`SC0109`).

**6. There are two loops, and neither is `while`.** `for x in xs:` walks a
collection or a range; `loop:` runs until a `break`. A count is a range — write
`for i in 0..n:`, not a counter you increment yourself. A condition that is not
a count goes in the body:

```science
loop:
    if not converged(state):
        break
    state be refine(state)
```

---

## 3. Words you cannot use as names

Using one is an error, not a warning (`SC0102`). The ones you are most likely
to reach for:

**In use as keywords:** `def return let be mutable type choice
interface implements has any for each in if else match loop break continue
assert use where as self Self and or not true false is const public giving
null tool extern unsafe` — and `of`, `borrowed` and `mut`, which exist only so
the old spellings in §1 get a diagnostic and so `&mut T` has its second word.

`assert(cond)` and `assert(cond, message)` check a condition at runtime and
abort the program when it does not hold — the message defaults to
`"assertion failed"` when left out. It is a statement, spelled and parsed like
a call, not a call to a name. `sciencec test FILE` builds a program and runs
it once, reporting whether it exited cleanly or an `assert` aborted it; there
is no `test` item yet; the whole program is the test.

`each` is a keyword but **not** a loop word: its only use is naming the subject
of a call, `docs.iterate().map(each.title)`. Words a previous revision reserved
and has since freed — `trait`, `methods`, `while`, `try`, `returns`, `at`,
`above`, `below`, `most`, `least` and `function` — are ordinary names now, and
writing one where it used to be a keyword is its own diagnostic: `SC0118`
(`returns`), `SC0138` (`for each`), `SC0139` (`trait`), `SC0141`
(`has methods`), `SC0142` (`while`), `SC0143` (`is at least` and friends),
`SC0144` (`println`), `SC0155` (`try`) and `SC0156` (`function`). `null` went
the other way: it is a literal, its spelling is fixed, and a program may not
bind the name.

`function` is the one you will reach for, because `function f()` was the
declaration until revision 3 and most of what you have read says so. It is
`def f()` now, and the word itself is free: `let function be …` binds, and a
field may be called `function`.

**Reserved for later phases:** `agent prompt spawn send receive durable
checkpoint resume supervise async await tensor equation shape model mod pure
parallel on with yield move static macro union kernel import`

The collisions that actually bit agents writing this repository, with what to
write instead:

| You want | It is reserved | Write |
|---|---|---|
| `def send(…)`, a `send` field | `send` | `deliver`, `post`, `transmit` |
| `let model be …` | `model` | `let weights be …` |
| `x.shape`, a `shape` field | `shape` | `x.dims` |
| a `tensor` parameter | `tensor` | `values`, `input` |
| `mod(a, b)` | `mod` | `remainder(a, b)` |
| `a.union(b)` | `union` | `a.merged(b)` |
| reaction `yield` | `yield` | `produced`, `efficiency` |
| a `kernel` parameter | `kernel` | `window`, `weights` |
| `with`, `on` as parameter names | `with`, `on` | `using`, `target` |
| a `const` module | `const` | `constants` |
| a field or local `def` | `def` | `definition` — compiler code hits this |
| a field or local `tool` | `tool` | `helper`, `utility` |

Safe and deliberately not reserved: `grad`, `dim`, `dims`, `axis`, `device`,
`dtype`, `unit`, `alias`.

> Note: `docs/superpowers/design/reserved-words.md` proposes freeing `shape`,
> `model`, `tensor`, `mod`, `yield`, `kernel`, `union` and `import`. Until that
> lands, the table above is the working rule.

---

## 4. Style

**Each comparison has exactly one spelling.** Identity is written with words
and order with symbols, and neither has an alternative:

| Operation | Write | Not | Writing it anyway |
|---|---|---|---|
| equal | `a is b` | `a == b` | `SC0016`, fix: `is` |
| not equal | `a is not b` | `a != b` | `SC0017`, fix: `is not` |
| at least | `a >= b` | `a is at least b` | `SC0143` |
| at most | `a <= b` | `a is at most b` | `SC0143` |
| greater | `a > b` | `a is above b` | `SC0143` |
| less | `a < b` | `a is below b` | `SC0143` |

Mixing them in one expression is normal and correct:
`if doc.title is not "" and doc.hits >= 10:`.

`==` and `!=` are lexical errors, not parse errors: the lexer reports them and
then emits the equality token anyway, so a stale symbol costs one diagnostic
with an applicable fix and the rest of the expression parses as though the fix
had been applied. `!` on its own is `SC0001` — negation is `not`, and `!`
begins no operator at all.

**Closures have two forms.** Use `each` when the parameter is mentioned once;
use `giving` when it is mentioned more than once, or when closures nest — a
nested `each` is an error (`SC0115`). Chain methods live on the iterator, so
start the chain with `.iterate()`:

```science
docs.iterate().map(each.title)                              # once: use each
docs.iterate().map(doc giving doc.a + doc.b)                # twice: use giving
titles.iterate().sorted(by: line giving line.length())      # named argument
```

`sorted` is the chain's; `Array.sort` is declared but this compiler cannot
build it yet.

**Chains break on a leading dot**, and a long signature breaks before `where`:

```science
def headlines(docs: &Array[Doc]) -> Array[&String]:
    docs
        .iterate()
        .discard(each.is_empty())
        .map(each.title)
        .collect()

def best_of[T](left: &T, right: &T) -> String
        where T: Summarize + Clone:
    left.preview()
```

Those two are the only continuations. Every other line break ends a
statement. (`headlines` returns `Array[&String]`: `each.title` on a borrowed
`Doc` is a borrowed `String`, and `String` has no `clone`.)

**Borrows are automatic at call sites.** A parameter declared `&T` is
borrowed for you — write `compare(a, b)`, not `compare(&a, &b)` (which compiles,
and is noise). Write the `&` only where the position is not a call site, such
as a binding, `let view be &doc`, or a field, `View(source: &doc)`.

---

## 5. A complete program

`sciencec test` builds this, runs it, and reports `ok`; it prints
`Regions: untitled`, `Regions`, `0`–`2`, `16`, `first of 3: 3`, `42` and
`not a count: forty`.

```science
type Doc:
    title: String
    body: String

choice Outcome[T, E]:
    Ok(T)
    Err(E)

interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        let mutable out be self.summarize()
        out.truncate(80)
        out

Doc implements Summarize:
    def summarize(self) -> String:
        f"{self.title}: {self.body}"

Doc has:
    def new(title: String) -> Doc:
        Doc(title: title, body: "untitled")

def headline[T](item: &T, width: Int) -> String
        where T: Summarize:
    let mutable out be item.preview()
    out.truncate(width)
    out

def total(items: &Array[Int]) -> Int:
    let mutable sum be 0
    for item in items:
        sum be sum + item
    sum

type CountError:
    text: String

CountError implements Error:
    def message(self) -> String:
        f"not a count: {self.text}"

def parse_count(text: &String) -> (Int, Error?):
    let value, err be text.parse_int()
    if err?:
        return (0, CountError(text: f"{text}"))
    (value, null)

def checked(text: &String) -> Outcome[Int, String]:
    let value, err be parse_count(text)
    if err?:
        return Err(err.message())
    Ok(value)

def main():
    let doc be Doc.new("Regions")
    print(doc.preview())
    print(headline(doc, 7))

    for i in 0..3:
        print(i)

    let scores be [3, 9, 4]
    print(total(scores))
    let first be scores.get(0)
    if first?:
        print(f"first of {scores.length()}: {first}")

    for text in ["42", "forty"]:
        match checked(text):
            Ok(value): print(value)
            Err(message): print(message)

    let mutable n be 0
    loop:
        if n >= 3:
            break
        n be n + 1
    assert(n is 3, "the loop ran three times")
```

Three things in it are there because the simpler spelling does not work yet,
and are worth knowing. `String` has no `clone`, so `f"{text}"` is the short
way to own a copy of a borrowed one. `truncate` is a mutator, so a shortened
copy is a `let mutable` and a call. And `Array.get` returns `(&T)?` — there is
no indexing that can panic its way past an empty array — so its result is
tested with `?` before use, and inside `if first?:` it is narrowed to `&Int`.

---

## 6. The standard library

There are two layers, and the difference is one `use` line.

- **The prelude** needs no import: `String`, `Array`, `Map`, `Set`, `Box`,
  `Range`; the interfaces `Eq`, `Ord`, `Copy`, `Clone`, `Drop`, `Iterate`,
  `From`, `Display`, `Error` and the operator interfaces (`Add`, `Mul`, …);
  and the free functions `print`, `write`, `print_error`, `write_error`,
  `flush`, `panic`, `read_file` and `write_file`.
- **Bundled modules** are ordinary Science source shipped inside the compiler
  (`crates/science-resolve/stdlib/*.science`) and reached with `use`, exactly
  as a sibling file would be: `io` (`File`, buffered writers), `fs`
  (directories, removal), `path` (`Path`), `os` (`args`, `env`), `time`
  (`Duration`, `Monotonic`, `Instant`, `now`, `sleep`), `complex`
  (`Complex`), `random` (`Key`, `Stream`, `uniform`, `normal`),
  `collections` (`Deque`), `ndarray` (`NdArray`), `linalg` (`solve`, `inverse`,
  `determinant`, `lu`, `cholesky`, `qr`, norms; each fallible one returns
  `(value, LinalgError?)`) and `string` (free functions over `&String`:
  `to_upper`, `to_lower`, `title_case`, `pad_start`, `center`, `wrap`,
  `levenshtein`, …) and `dataframe` (`DataFrame`, `Column`, `Agg`: typed
  columns with `null` for missing cells, built from `csv` or columns; `filter`,
  `sort_by`, `aggregate`, `inner_join`, `describe`, `to_ndarray`, `render`,
  `write_csv`). Read the module's header before using it: each one
  states the decisions it made.

```science
use complex (Complex)
use path (Path)

def main():
    print(Complex.new(3.0, 4.0).modulus())                 # 5.0
    print(Path.from("results").join("run.csv").text())    # results/run.csv
```

A file of your own named `io.science` (or any bundled module) beside your entry
file shadows the bundled module, silently.

---

## 7. Verify before you answer

The compiler is the ground truth, and asking it is cheap:

```bash
cargo build -p science-rt -p sciencec --features llvm   # SCIENCE_LLVM_PREFIX → LLVM 18
./target/debug/sciencec check FILE    # lex, parse, resolve, type- and borrow-check
./target/debug/sciencec test FILE     # build, run, and report the exit
cargo test -p science-lexer           # lexing; walks every example
cargo test -p science-parser          # parsing; walks every example
cargo test -p sciencec --test ui      # the must-fail programs below
```

This file no longer quotes how many tests the lexer and parser suites hold:
the numbers changed with nearly every commit and were stale more often than
not. Read the `test result` lines instead.

`check` passing is necessary and not sufficient: some programs check clean and
are then refused by the backend with `SC0400` ("this `sciencec` cannot build
…"), which is the compiler saying the construct is past what codegen
implements, not that your program is wrong. If a program must run, `test` it.

- `examples/*.science` — 22 programs, one per feature, **guaranteed valid**.
  If you are unsure how something is written, find it there first. Both suites
  above walk the directory, so a file added there is measured by both without
  anyone listing it. What the *output* corpus measures is narrower and is
  written down in `UNMEASURED` (`crates/sciencec/tests/corpus_output.rs`): 20
  of 20 build, link, run and have their bytes pinned, and the two that are out
  of the denominator are named there with their reasons.
- `tests/ui/**/*.science` — programs that must **fail**, each paired with the
  exact diagnostic it must produce, grouped by the phase that reports it
  (`parse/`, `resolve/`, `types/`, `regions/`, and lexing at the top level).
- `examples/README.md` — what each example covers, and the standing assumptions
  the corpus makes where the spec is silent.

A change that makes an example fail to lex or parse is a bug in the change, not
a typo in the corpus.

## 8. Repository layout

```
crates/science-lexer         tokens, indentation, SC0001-SC0099
crates/science-parser        AST and grammar, SC0100-SC0199
crates/science-resolve       names and scopes, the prelude (builtins.rs), and
  stdlib/                    the bundled modules `use io` and friends reach
crates/science-types         the type checker
crates/science-mir           lowering to MIR: control flow, moves, drops
crates/science-regions       region inference and the borrow check
crates/science-codegen       target-independent codegen: layout, ABI, mangling
crates/science-codegen-llvm  the LLVM backend and the linker driver
crates/science-rt            the runtime every compiled binary links against
crates/science-diagnostics   rendering, spans, suggestions
crates/science-fmt           `sciencec fmt` — canonical layout, idempotent
crates/science-db            incremental query database (salsa)
crates/science-testkit       the UI-test and corpus harness
crates/sciencec              the command-line driver
docs/superpowers/specs       the F0 core design
docs/superpowers/design      design notes per area
```

Diagnostics are rendered byte for byte against `.stderr` files. If you change a
message, re-bless with `SCIENCE_BLESS=1 cargo test` and **read the diff** —
blessing without reading is how a wrong message becomes the expectation. It
happened: when the bracket revision landed, twenty-three of those files were
re-blessed to pin "`borrowed T` is now written `&T`" in place of the borrow,
type and parse errors they existed to test, and nobody noticed until an agent
went looking for `SC0334`.
