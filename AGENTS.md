# Writing Science

Science is a compiled language for scientific computing and machine learning.
Its syntax rhymes with Rust and Python and **is not either of them**. If you
write it by analogy you will get it wrong in the twenty-two specific ways below.

Read §1 before writing a line. The authority is
`docs/superpowers/specs/2026-09-16-science-f0-core-design.md` §4, and
`examples/` holds twenty programs that are guaranteed valid — imitate them.

---

## 1. The translation table

The left column is how Rust and Python spell it. All of it is a compile error
in Science, and each one has a diagnostic that names the right form and offers
it as a fix.

| Don't write | Write | |
|---|---|---|
| `fn f()` | `function f()` | |
| `let x = v` | `let x be v` | |
| `let mut x = v` | `let mutable x be v` | |
| `x = v` | `x be v` | assignment is also `be` |
| `struct Doc:` | `type Doc:` | |
| `enum Format:` | `choice Format:` | |
| `impl Summarize for Doc:` | `Doc implements Summarize:` | **the order reverses** |
| `impl Doc:` | `Doc has:` | |
| `trait Summarize:` | `interface Summarize:` | the interface; `impl` is the implementation |
| `Array[T]` | `Array of T` | |
| `Map[K, V]` | `Map of (K, V)` | two or more need parentheses |
| `fn largest[T](…)` | `function largest of T(…)` | |
| `&T` | `borrowed T` | |
| `&mut T` | `mutable borrowed T` | |
| `&self` / `&mut self` | `self` / `mutable self` | by value is `self: Self` |
| `dyn Trait` | `any Interface` | |
| `while cond:` | `loop:` + `break` | **`while` is gone**; see §2 |
| `println(x)` | `print(x)` | no-newline form is `write(x)` |
| `f()?` | `try f()` | **prefix, not postfix** |
| `pub` | `public` | |
| `!flag` | `not flag` | |

---

## 2. The six rules you cannot guess

These have no Rust or Python analogue. They are where generated code fails even
after the table above is applied.

**1. `be` binds and assigns.** `let x be 1` introduces a name; `x be 2` assigns
to one that exists. `=` is not an assignment operator at all.

**2. Calls always take parentheses.** `text.length()`, never `text.length`.
Parentheses mean call; their absence means a field. `x.shape` is a field access,
`x.shape()` is a method call, and they are different things.

**3. `of` takes parentheses for two or more arguments.** `Array of Doc` needs
none. `Map of (String, Int)` does, because `Map of String, Int` in a parameter
list cannot be parsed. Nesting follows the same rule:
`Array of (Map of (String, Int))`. And an associated call on a generic type needs
its own parentheses: `(Array of Doc).new()`, never `Array of Doc.new()`.

**4. `try` goes before the expression** and binds the whole chain to its right.
`try a.b()` means `try (a.b())`. When it must cover only part of an expression,
parenthesise it: `(try read_config(path)).port`.

**5. Blocks are indentation, and an inline body ends where the expression ends.**
`if a: b else: c` works because `else` cannot continue an expression. An inline
body may be an expression, an assignment, or `return` / `break` / `continue` —
but **not** a `let`.

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

Using one is an error, not a warning. The ones you are most likely to reach for:

**In use as keywords:** `function return let be mutable type choice
interface implements has of borrowed any for each in if else match
loop break continue use where as self Self and or not true false is const public
try giving`

`each` is a keyword but **not** a loop word: its only use is naming the subject
of a call, `docs.map(each.title)`. Words a previous revision reserved and has
since freed — `trait`, `methods`, `while`, `at`, `above`, `below`, `most`,
`least` — are ordinary names now, and writing one where it used to be a keyword
is its own diagnostic, `SC0138`–`SC0144`.

**Reserved for later phases:** `agent tool prompt spawn send receive durable
checkpoint resume supervise async await tensor shape model mod extern unsafe
pure parallel on with yield assert move static macro union kernel import`

The collisions that will actually bite you, with what to write instead:

| You want | It is reserved | Write |
|---|---|---|
| `let model be …` | `model` | `let weights be …` |
| `x.shape` | `shape` | `x.dims` |
| a `tensor` parameter | `tensor` | `values`, `input` |
| `mod(a, b)` | `mod` | `remainder(a, b)` |
| `a.union(b)` | `union` | `a.merged(b)` |
| reaction `yield` | `yield` | `produced`, `efficiency` |
| a `kernel` parameter | `kernel` | `window`, `weights` |
| `assert(cond)` | `assert` | `check(cond)` |
| a `const` module | `const` | `constants` |

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
nested `each` is an error.

```science
docs.map(each.title)                        # once: use each
docs.map(doc giving doc.a + doc.b)          # twice: use giving
docs.sort(by: line giving line.length())    # named argument
```

**Chains break on a leading dot**, and a long signature breaks before `where`:

```science
function headlines(docs: borrowed Array of Doc) -> Array of String:
    docs
        .iterate()
        .discard(each.is_empty())
        .map(each.title)
        .collect()

function best_of of T(left: borrowed T, right: borrowed T) -> String
        where T: Summarize + Clone:
    left.preview()
```

Those two words are the only continuations. Every other line break ends a
statement.

**Borrows are automatic at call sites.** A parameter declared `borrowed` is
borrowed for you — write `compare(a, b)`, not `compare(borrowed a, borrowed b)`.
Write the borrow out only where the position is not a call site, such as
`View(source: borrowed doc)`.

---

## 5. A complete program

```science
type Doc:
    title: String
    body: String

choice Outcome of (T, E):
    Ok(T)
    Err(E)

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

function longest of T(items: borrowed Array of T) -> borrowed T
        where T: Ord:
    let mutable best be items.get(0)
    for item in items:
        if item > best:
            best be item
    best

function load(path: borrowed String) -> Result of (Doc, Error):
    let text be try read_file(path)
    Ok(Doc(title: "loaded", body: text))

function main():
    let doc be Doc.new("Regions")
    print(doc.preview())

    for i in 0..5:
        print(i)

    match load("a.txt"):
        Ok(value): print(value.summarize())
        Err(e): panic(e)
```

---

## 6. Verify before you answer

The corpus is the test suite. These commands are the ground truth:

```bash
cargo test -p science-lexer      # lexing, 121 tests, includes all 20 examples
cargo test -p science-parser     # parsing, 176 tests, includes all 20 examples
cargo test --workspace           # everything
```

- `examples/*.science` — twenty programs, one per feature, **guaranteed valid**.
  If you are unsure how something is written, find it there first.
- `tests/ui/*.science` — programs that must **fail**, each paired with the exact
  diagnostic it must produce.
- `examples/README.md` — what each example covers, and the standing assumptions
  the corpus makes where the spec is silent.

A change that makes an example fail to lex or parse is a bug in the change, not
a typo in the corpus.

## 7. Repository layout

```
crates/science-lexer         tokens, indentation, SC0001-SC0099
crates/science-parser        AST and grammar, SC0100-SC0199
crates/science-resolve       names and scopes  (being migrated)
crates/science-diagnostics   rendering, spans, suggestions
crates/science-rt            the runtime every compiled binary links against
crates/science-db            incremental query database (salsa)
crates/science-testkit       the UI-test and corpus harness
docs/superpowers/specs       the F0 core design — the authority
docs/superpowers/design      design notes per area
```

Diagnostics are rendered byte for byte against `.stderr` files. If you change a
message, re-bless with `SCIENCE_BLESS=1 cargo test` and **read the diff** —
blessing without reading is how a wrong message becomes the expectation.
