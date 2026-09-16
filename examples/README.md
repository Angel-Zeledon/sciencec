# Science example corpus

Valid `.science` programs, one file per language feature, written against
[`docs/superpowers/specs/2026-09-16-science-f0-core-design.md`](../docs/superpowers/specs/2026-09-16-science-f0-core-design.md).

These files are **input data for the lexer and parser test suites**. They are
meant to be syntactically correct, so anything that fails to lex or parse here
is a bug in the compiler or a gap in the spec — not a typo to be quietly
patched. Programs that must *fail* live in `tests/ui/` instead.

The syntax is the English one: `function` and `->`, `let x be v`, `type`
and `choice`, `Doc implements Summarize`, `Doc has:`, `Array of T` and
`Map of (K, V)`, `borrowed` and `mutable borrowed`, `any Interface`,
`for x in xs:`, `public`. Identifiers are in English too, matching the
keywords.

Absence and failure are §5.5's: absence is `T?` with `null` as its other
value, failure is a second return value of type `Error?`, and `?` is the
postfix presence test. `Option of T`, `Result of (T, E)` and `try` are gone,
and so are `Some`, `None`, `Ok` and `Err` — those four names are free, and a
bare one in a pattern is a binding.

Every file carries a header comment naming the spec sections it exercises.
That header is the authority for its own file; the table below summarizes it.

## What each file covers

| File | Spec | Covers |
|---|---|---|
| `00_kitchen_sink.science` | §11 | **The F0 acceptance program.** Each of the ten requirements in §11's first condition is marked in the file with its number: generics with bounds, const generics, associated types, an interface dispatched both statically and dynamically, an exhaustive `match`, a nullable type and a fallible function returning `(T, Error?)`, a user type implementing operator interfaces, a range-driven loop, a closure, and a type holding a borrow in a field. It is also one of the two files that used to lean on `try`'s invisible `From` call; the two calls are written out now, in `load`. |
| `01_functions.science` | §4.4, §4.5, §6.3 | Functions with and without a `->` return type, the last expression as the function's value, early `return`, parameters by value / `borrowed` / `mutable borrowed`, the unit type, recursion, and call sites that borrow without saying so. |
| `02_bindings.science` | §4.3, §4.4, §5.1, §5.2 | `let x be v` and `let mutable x be v`, optional type annotations, reassignment (which also uses `be`), `as` casts, `const NAME be v` at module level, `type X is Y` aliases, and one binding of every primitive type including the `Int` and `Float` aliases. |
| `03_structs.science` | §4.4, §4.7, §6.2 | `type` declarations, construction with mandatory named arguments in any order, trailing commas, field access, nested records, generic records, a const generic parameter, fields that are shared and exclusive borrows, an inherent `has:` block with an associated function, and a one-line `Marker implements Copy`. |
| `04_enums.science` | §4.4, §4.7, §5.5 | `choice` declarations: C-like choices, positional payloads, variants carrying several values, generic choices over one and two parameters, a payload that is itself a generic type, a variant carrying a borrow, and variants written both bare and qualified (`Bound.Exact(x)`). The library supplies no generic choice of its own since §5.5 removed `Option` and `Result`, so every choice here is a user declaration, and the file says outright that absence is not one of them. F0 has no variants with named fields, so every payload is positional. |
| `05_match.science` | §4.5, §4.7 | Every pattern form: literals (int, char, string, bool), the wildcard `_`, bindings, variant patterns with and without payload, record patterns with named fields, tuple patterns, alternatives with `\|`. Inline and block arms, nested matches, `match` as an expression, qualified variant patterns, an arm whose value is `Never`, and a guard written as a comparison phrase in the arm body — there is no `if` inside a pattern. |
| `06_traits.science` | §4.4, §5.4 | A required method, default methods that call the required one, several implementations of one interface, generic interfaces, associated types (`type Item`, `Self.Item`, with `Iterate.next` returning `Self.Item?`), receivers written `self`, `mutable self` and `self: Self`, a `has:` block with an associated function, a one-line marker implementation, the operator interfaces, and the library interfaces `Clone`, `Eq`, `Ord`, `Drop`, `Display` and `Iterate`. |
| `07_generics.science` | §4.3, §4.4, §5.3 | No bound, one bound in the parameter list, several joined by `+`, a `where` clause, a bound and a `where` clause at once, several bounded parameters, generic records and choices, several type parameters, nested generic arguments including the nullable `(borrowed T)?` that `Array.get` returns, and `(Wrapper of Int).holding(7)` — the parenthesized form §4.3 requires before an associated function on a generic type. |
| `08_dyn_dispatch.science` | §4.3, §5.3 | `any Interface` behind a shared borrow, behind an exclusive borrow, behind `Box`, as a return type, as a record field, and inside `Array of (Box of any Summarize)` — each contrasted with the statically dispatched version of the same call. |
| `09_absence_and_failure.science` | §4.5, §5.5, §8 | **Absence and failure, at length.** `T?` as a return type, as a field and as a parameter; `null`; the postfix `?` and the narrowing that follows it; the pair `-> (T, Error?)`; a fallible function with no value to return, written `-> Error?`; three fallible calls in one function; both error forms §5.5 allows, side by side — the concrete `ConfigError?`, whose `match` is exhaustive, and the interface `Error?`, which composes and boxes; and the conversion between them, written out at the `return`. This file replaced `09_option_result.science`, which was named after two types that no longer exist, and it is nearly twice as long, which §3.3 of `syntax-revision-2.md` predicted and which the file says so itself rather than compressing the cost away. |
| `10_loops.science` | §4.5, §5.4 | `for` over a collection, over `String.chars()`, and over both range forms `0..n` and `0..=n`; a range between two variables; `loop` ended by a condition and `loop` ended by a `break` in its body; `break` and `continue`; inline bodies; a `loop` driven by a `?` test with a `match` inside it; three levels of nesting; a search that returns `Int?`; and a `Map` walked through an `Array` of keys. |
| `11_literals.science` | §4.2 | Decimal, hex, binary and octal integers, `_` separators, every integer and float suffix, exponent notation, all eight string escapes, character literals, booleans, and unit. |
| `12_operators.science` | §4.6, §5.4 | Every level of the precedence table, with the grouping each implies spelled out in comments: call / index / field access / the postfix `?`, unary `-` `not` `borrowed`, `as`, right-associative `**`, `* / % @`, `+ -`, `<< >>`, `&`, `^`, `\|`, the comparisons — identity in words, order in symbols, one spelling each — `and`, `or` — and assignment, which the table omits because it is a statement. `?` is on the tightest rung and is a total operator: it answers a question and, unlike the `try` that used to sit there, can never return from the function around it. |
| `13_inline_blocks.science` | §4.5 | The inline block form next to the indented form of the same construct: `if`/`else`, function bodies, `for` over a range, `for` over a collection, `loop`, `match` arms; inline bodies that are an assignment, a `return`, a `break` and a `continue`; and a dangling `else` binding to the innermost `if`. |
| `14_line_continuation.science` | §4.2, §4.6 | The two ways one logical line spans several physical ones: unclosed parentheses — in signatures, calls, named-argument construction, generic arguments, arithmetic and boolean expressions — and a chain broken by a leading `.`. Both nested inside each other, with trailing commas and comment-only lines throughout. |
| `15_comments.science` | §4.2 | `#` to end of line: at column 0, trailing, inside blocks, over-indented, between an `if` body and its `else`, as the last line of the file, with unicode, and with text that would otherwise lex as keywords or literals. Comment-only and blank lines take no part in the indentation calculation. |
| `16_indentation.science` | §4.2, §4.5, §4.6 | Eight levels of nesting, closing several levels in one dedent, an `if`/`else` ladder, inline blocks inside indented ones, blank lines inside a block, and an indented `.` chain — a continuation that looks like a block and is not one. |
| `17_modules.science` | §4.3, §4.4 | `use text.parser`, `use text.parser (Token, lex)`, a deeper path, a selection broken over several lines with a trailing comma, and `public` on record types, choices, interfaces, functions and inherent methods. There is no `mod` declaration: §4.4 makes a file a module already. |
| `18_ownership.science` | §4.4, §6.1, §6.2, §6.3 | Programs the borrow checker must **accept**: moves by argument, by return and by rebinding; many simultaneous shared borrows; one exclusive borrow; a borrow returned from a function; two fields of one record borrowed at once through auto-dereference; borrows stored in record fields; a one-line `implements Copy`; `Box.new`; and scope-based `Drop`. No region is written anywhere. |
| `19_stdlib.science` | §8, §5.4 | The whole F0 library surface and nothing outside it: `Box`, `String`, `Array`, `Map`, `Error`, the free functions `print`, `write`, `panic`, `read_file` and `write_file`, and the interfaces `Copy`, `Clone`, `Drop`, `Eq`, `Ord`, `Iterate` and `From`. This is the file that fixes the method names the rest of the corpus calls, and the file that records what §5.5 cost: `is_some`, `is_ok`, `unwrap` and `unwrap_or` went with the types that carried them, and each of the four is replaced here by the lines it takes to write out, not by a shorter thing. |
| `20_extern.science` | `ffi-c-boundary.md` §1, §3, §5; `c-binding-coverage.md` §3.4, §4.1, §4.2 | The `extern` block, and the concrete error form at a C boundary — `gemm` returns `BlasError?` alone because it writes its result through a borrow, `native_double_type` returns the pair: `unsafe extern "C" library "..."` with `via pkg-config`, `kind static` and `when available`; all five item forms (`function`, `type ... is`, `const ... be ... as`, `static NAME: T`, `union Name: size N align M`); `symbol` decoupling the Science name from the linker name, and with it both integer-width builds of §1.6; the `ffi` type vocabulary including `Complex32` and `Complex64`; C-compatible records and handles declared *outside* the block; and `unsafe:` at the call site. This is the one file in the corpus whose types are not F0 library types - `ffi` is a standard-library module `ffi-c-boundary.md` §10.3 requests and this corpus assumes. |
| `21_compiler_shapes.science` | `self-hosting.md` Decision 1; core spec §6 | **The acceptance test for "no lifetime syntax", not a tutorial.** Every structure in it is taken from the real compiler in `crates/` rather than invented: the flat `DefTable` handing out `DefId` indices, the `Rib` scope stack, `Parser` holding a borrow of its token array for its whole life, `Node of T` with *two* borrowed fields, and a diagnostics sink borrowed exclusively while the table is borrowed shared. §4 is the case the language's central claim turns on and §5 names the two shapes deliberately absent — a bump arena handing out borrows, and an interner — because the real compiler does not use them either, which makes this test easier than the general case by exactly that much. Today it proves only that the shapes are *sayable*; the day region inference lands it is the first thing run against it. |

## Where these files go beyond the spec

Most of what this section used to list is now written down. §4.4 gives generic
interfaces, inherent `has:` blocks, associated functions, the one-line
marker implementation and record-pattern syntax; §4.5 gives the range forms
`0..n` and `0..=n`; §4.7 gives trailing commas, qualified variants,
auto-dereference and `Never`; §5.3 gives const generics; §5.4 gives associated
types and names the operator interfaces, `MatMul` and `Index` among them; and §4.3
gives `public` a rule of its own. The corpus follows all of it.

What is left is the short list of things a runnable program still needs and
the spec does not decide. Each is an assumption, not a fact; they are collected
here so that changing one is a single, visible decision.

- **The method names of the library interfaces.** §5.4 names `Add Sub Mul Div Rem
  Pow MatMul Neg Index Eq Ord Copy Clone Drop Iterate From Display` and spells
  out only `Iterate`, whose `type Item` and `next(mutable self)` are written in
  the section itself. `Copy` and the markers have no methods. Every other name
  used here is the obvious reading: `clone`, `eq`, `less`, `drop`, `display`,
  `from`, and `add` / `sub` / `mul` / `neg` / `matmul` for the operators.
- **The method sets §8 declares closed but no longer lists.** §8 names the
  types, the interfaces and the free functions, and says anything not listed does
  not exist — but it stopped listing. `19_stdlib.science` is where the corpus
  writes its reading down and every other file draws from it: `Box` has `new`
  and nothing else; `String` has `new`, `length`, `is_empty`, `push_str`,
  `truncate`, `starts_with`, `chars`; `Array` has `new`, `length`, `is_empty`,
  `push`, `pop`, `get`, `get_mut`; `Map` has `new`, `length`, `insert`, `get`,
  `remove`, `contains`. `Error` has `message`, which §5.5 writes out itself.
- **A nullable has no methods at all.** `Option`'s `is_some`, `unwrap` and
  `unwrap_or` went with the type, and §5.5 put nothing in their place but the
  postfix `?`. So the accessors that used to return an `Option` — `Array.get`,
  `Array.get_mut`, `Array.pop`, `Map.get`, `Map.remove`, `Map.insert` — return
  `T?`, and every use of one in the corpus is a `?` test followed by a branch.
  Nothing in §8 says a nullable has *no* methods; the corpus reads the absence
  of a type as the absence of its method set, because the alternative is to
  invent a method set for a spelling the spec introduced without one.
- **The chain adaptors.** `iterate`, `discard`, `map`, `take`, `collect` and
  `sort(by: …)` come from §4.6's chain example rather than from §8, and the
  type `.iterate()` returns is named nowhere. The corpus uses exactly those six
  and invents none.
- **`.length()`, not `.len()`.** §4.6 writes `text.length()`; a §4.4 example
  writes `self.body.len()`. The two disagree, and the corpus follows §4.6
  everywhere, because §4.6 is the section that argues for the spelling.
- **`String` does not implement `Clone`.** Nothing says `String: Clone`, so
  every file needing an owned copy of a `borrowed String` builds one with
  `String.new()` and `push_str`. The same pair stands in for concatenation,
  which the library also omits.
- **`Map` does not implement `Iterate`.** Nothing says it does, so
  `10_loops.science` walks a map through an `Array` of keys and `Map.get`
  rather than inventing an entry type.
- **Calling a method through a `Box`.** `Box of any Summarize` is only worth
  having if `value.summarize()` works on it, so it does. §4.7's
  auto-dereference rule is written about borrows, not about `Box`.
- **`any Interface` only behind an indirection.** No section says so. The corpus
  writes `borrowed any T`, `mutable borrowed any T` and `Box of any T`, and
  never `any T` by value.
- **A fallible function with no value returns the error alone.** `-> Error?`
  with no value slot is what `write_file`, `19_stdlib.science`'s `store`,
  `09_absence_and_failure.science`'s `save_config` and `20_extern.science`'s
  `gemm` are declared as. §4.5 says a fallible function returns "its value *and*
  an error" and never says what a function with no value does; the pair form
  would put `()` in the first slot and bind a name that could do nothing, so the
  corpus takes the bare form. The parser accepts it. This is a hole in §4.5, not
  a decision it made.
- **A concrete error goes into an `Error?` slot without being written.** §5.5
  makes `Error?` shorthand for `(any Error)?`, so a `return (doc, err)` whose
  `err` is a concrete `LoadError` is the trait-object coercion, and no section
  says whether it is implicit. The corpus writes it bare, in
  `00_kitchen_sink.science`'s `open`, because the shorthand is useless
  otherwise: every value that reaches an `Error?` is some concrete type. The
  conversions §5.5 *does* require to be written — one concrete error type into
  another — are written, at the `return`, in that file's `load` and in
  `09_absence_and_failure.science`'s `read_config`.
- **A chain of fallible calls has no form.** §4.6's chain is the shape most
  Science code takes, and §4.5's failure model has no place in one: the check is
  a statement, `return` is a statement, and neither can appear between two `.`
  links. `try` could not either, but it did not need to — it propagated. So a
  sequence of fallible calls is written as a sequence of statements everywhere
  in this corpus, three lines apiece, and no file chains one. That is a gap
  between two sections rather than a corpus choice, and it is recorded here
  because the corpus cannot close it.
- **`IoError` is the error type of `read_file` and `write_file`.** §8 gives
  neither a signature and the name appears nowhere in the spec, so the corpus
  fixes it: `read_file` returns `(String, IoError?)` and `write_file` returns
  `IoError?`. A function with an error type of its own converts `IoError` into
  it by calling a `From` implementation, or an ordinary associated function, at
  the `return` — which is what `00_kitchen_sink.science` and
  `09_absence_and_failure.science` do; a function that only passes the failure
  along declares `IoError` itself, as `19_stdlib.science` does. All three files
  agree, and they have to: at most one reading survives a type checker.
- **Indexing is never exercised, and no section forbids it.** §4.6's precedence
  table has an `index` level and §5.4 names an `Index` interface, but neither gives
  the operator an operand type or a rule, and §8 — which declares the method
  sets closed without enumerating them — says nothing about indexing at all. In
  particular **no spec section states that an indexing operator may not panic**;
  that is this corpus's reading, taken because `get` returns an `Option` and is
  the only accessor the corpus commits to. The bracket that used to spell
  `Array[T]` is now `Array of T`, so `[` is free. No file in the corpus contains
  one. See `docs/superpowers/design/indexing-and-array-literals.md`, which takes
  the decision this bullet was standing in for.
- **Explicit `borrowed` in record construction.** §6.3 auto-borrows at call
  sites, so calls pass bare names, and it says an explicit borrow stays legal
  where it clarifies. It does not say whether named-argument construction of a
  record counts as a call site. The corpus writes the borrow out there —
  `View(source: borrowed doc)` — and otherwise only where §6.3's "where it
  clarifies" licenses it.
- **A by-value receiver is written `self: Self`.** §4.3 gives `self` and §5.4
  gives `mutable self`; neither of them is by value, and the annotated form is
  the only spelling left.
- **`Wrapper`'s constructor is called `holding`.** `of` is the word that
  introduces generic arguments (§4.3) and so is no longer an identifier, which
  rules out the name an associated constructor would otherwise want.
- **The `ffi` module does not exist yet.** `20_extern.science` writes `ffi.Span`, `ffi.MutableSpan`, `ffi.Pointer`, `ffi.OpaqueHandle`, `ffi.CStr`, `ffi.Uninitialized`, `ffi.CLayout`, `ffi.Complex32` and `ffi.Complex64`, and the C scalar aliases `CInt`, `CUInt` and `CSizeT`. §8 of the core spec declares the F0 library closed and none of these is in it; `ffi-c-boundary.md` §10.3 asks for the module as a spec change rather than assuming it. The file is in the corpus because it is *syntactically* correct, which is what the corpus is for, and its names resolve only once that module lands.
- **Modules have no declaration form.** §4.4 makes a file a module and a
  directory with `mod.science` a module, and gives no `mod` item;
  `17_modules.science` therefore declares nothing and relies on file-as-module.

## Disagreements found and settled

These were real contradictions between the corpus and the spec. Each is fixed
now; they are listed because the fix was a decision, not a typo correction.

- **`read_file`'s error type** differed in three files. The spec settles
  nothing, so the corpus's own principled reading won — the one
  `00_kitchen_sink.science` already used. The other two were brought into
  line; see the `IoError` entry above.
- **`try`'s invisible widening, which was never specified, is gone.** This list
  used to record it as a standing assumption: §4.5 said `try` "unwraps
  `Ok`/`Some` or returns the failure" and never said the failure was converted
  on the way out, yet `00_kitchen_sink.science` and `09_option_result.science`
  both depended on the conversion. §5.5 closed the hole by construction — the
  caller writes the conversion, because the caller writes the return — and both
  files now write it: `LoadError.from(io_err)` and `LoadError.from(parse_err)`
  in the first, `StartupError.from_io(io_err)` and
  `StartupError.from_config(host_err)` in what is now
  `09_absence_and_failure.science`. The assumption is struck from the list
  above because it is no longer an assumption about anything.
- **`08_dyn_dispatch.science` attributed to §4.3** a rule that `any Interface` may
  appear only behind an indirection. §4.3's keyword table has the `any
  Summarize` row and no such rule. The header now presents it as the corpus's
  reading, which is what it is.
- **`mod` was reserved by the lexer and absent from §13.** The spec was the
  side out of step — `mod.science` already makes the word load-bearing — so
  §13 now lists it, and `17_modules.science`'s header is true as written.
- **§4.4 wrote `(Array of Doc).new()()`**, one call too many, and
  `self.body.len()` where §4.6 writes `.length()`. Both corrected in the spec;
  the corpus uses `.length()` at every one of its call sites.
