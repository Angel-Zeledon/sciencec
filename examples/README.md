# Science example corpus

Valid `.science` programs, one file per language feature, written against
[`docs/superpowers/specs/2026-09-16-link-f0-core-design.md`](../docs/superpowers/specs/2026-09-16-link-f0-core-design.md).

These files are **input data for the lexer and parser test suites**. They are
meant to be syntactically correct, so anything that fails to lex or parse here
is a bug in the compiler or a gap in the spec — not a typo to be quietly
patched. Programs that must *fail* live in `tests/ui/` instead.

Identifiers are in English, matching the English keywords.

## What each file covers

| File | Spec | Covers |
|---|---|---|
| `00_kitchen_sink.science` | §11 | **The F0 acceptance program.** Generics with bounds, traits with static *and* dynamic dispatch, exhaustive `match` over enums, `Option` and `Result` with `?`, and a struct storing a borrow in a field — all in one program. |
| `01_functions.science` | §4.3 | Functions with and without a return type, explicit `()`, last expression as the value, early `return`, parameters by value / `&` / `&mut`, recursion. |
| `02_bindings.science` | §4.3, §5.1 | `let` and `let mut`, optional type annotations, reassignment, one binding of every primitive type, `Int`/`Float` aliases, `as` casts. |
| `03_structs.science` | §4.3, §4.5, §6.2 | Struct declaration, construction with mandatory named arguments in any order, trailing commas, field access through an auto-dereferenced borrow, nested structs, generic structs, fields that are `&`/`&mut` references, an inherent `impl` with an associated function, and a block-less `impl Copy for Marker`. |
| `04_enums.science` | §4.3, §4.5, §5.5 | C-like enums, positional payloads, multi-field variants, generic enums (`Option`, `Result`, `Either`, `Tree`), a variant carrying a borrow, `Box.new`, and variants written both bare and qualified (`Format.Markdown`). |
| `05_match.science` | §4.4, §4.5 | Every pattern form: literals (int, char, string, bool), wildcard `_`, bindings, enum variants with payload, struct patterns with named fields, tuple patterns, alternatives with `\|`. Inline and block arms, nested matches, `match` as an expression, qualified variant patterns, and an arm of type `Never`. |
| `06_traits.science` | §4.3, §5.4 | Required methods, default methods that call each other, overriding a default, several impls, `&self` / `&mut self` / `self`, generic traits, inherent impls with associated functions, a block-less marker impl, and impls of `Clone`, `Eq`, `Drop`, `From`. |
| `07_generics.science` | §4.3, §5.3 | Unbounded generics, one bound, several bounds with `+`, `where` clauses (single and multi-parameter, and combined with in-list bounds), generic structs and enums, nested generic types. |
| `08_dyn_dispatch.science` | §4.3, §5.3 | `dyn` behind `&`, behind `&mut`, behind `Box`, as a return type, as a struct field, and in a heterogeneous `Array[Box[dyn Trait]]` — contrasted with the statically dispatched version of the same call. |
| `09_option_result.science` | §4.4, §4.5, §5.5, §8 | `Option` and `Result` as return types, `?` on both, `?` widening an error through `From`, `?` inside a larger expression, consuming both with `match`, the six methods §8 gives them, and a `Never`-typed arm. |
| `10_loops.science` | §4.4, §4.5 | `for … in` over `Array` and over `String.chars()`, `while` as the counting loop F0 has instead of a range, `loop`; `break` and `continue`; inline bodies; a `loop` whose body is a `match`; three levels of nesting. |
| `11_literals.science` | §4.1 | Decimal / hex / binary / octal integers, `_` separators, every integer and float suffix, exponents, all eight string escapes including `\'` and `\u{…}`, character literals, booleans, unit. |
| `12_operators.science` | §4.4 | Every operator in the precedence table, with the grouping each level implies spelled out in comments: call / `[…]` / field / `?`, unary `-` `not` `&` `&mut`, `as`, `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|`, comparisons, `and`, `or` — and assignment, which the table deliberately omits because it is a statement. |
| `13_inline_blocks.science` | §4.2 | The inline block form `if a: b else: c`, each time next to the indented form of the same construct: `if`/`else`, `while`, `for`, `loop`, `match` arms, inline blocks nested in bindings, arguments and `return`, and a dangling `else` binding to the innermost `if`. |
| `14_line_continuation.science` | §4.1, §4.5 | Implicit line continuation inside unclosed `(` and `[`: signatures, generic parameter lists, nested generic types, calls, named-argument construction, arithmetic and boolean expressions, a generic instantiation broken over lines, trailing commas, and comments inside brackets. |
| `15_comments.science` | §4.1 | `#` to end of line: at column 0, trailing, inside blocks, over-indented, between an `if` body and its `else`, with unicode and with characters that would otherwise lex as tokens. Comment-only and blank lines take no part in the indentation calculation. |
| `16_indentation.science` | §4.1, §4.2 | Eight levels of nesting, dedenting several levels in one step, an `if`/`else` ladder, inline blocks inside indented ones, and blank lines inside a block. |
| `17_modules.science` | §4.3, §12 | `use text.parser`, `use text.parser (Token, lex)`, deeper paths, a selection broken over lines, and `pub` on structs, enums, traits, functions and inherent impls. No `mod` declaration: §12 excludes one from F0. |
| `18_ownership.science` | §4.5, §6.1, §6.2 | Programs that the borrow checker must **accept**: moves by argument / return / rebinding, many simultaneous shared borrows, one exclusive borrow, borrows returned from functions, borrows stored in struct fields, a user type with a block-less `impl Copy`, `Box.new`, and scope-based `Drop`. |
| `19_stdlib.science` | §8 | The whole F0 library surface, and nothing outside it: every method §8 lists for `Option`, `Result`, `Box`, `String`, `Array` and `Map`, the free functions `print`, `println`, `panic`, `read_file` and `write_file`, and the traits `Copy`, `Clone`, `Drop`, `Eq`, `Ord`, `Iterate`, `From`. |

## Where these files go beyond the spec

Most of what this section used to list is now written down. §4.3 gives
generic traits, inherent impls, associated functions and the block-less
marker impl; §4.4 gives struct-pattern syntax; §4.5 gives trailing commas,
qualified variants, auto-dereference, the absence of range syntax and
`Never`; §8 lists the method set of every library type and declares that list
closed. The corpus follows all of it, and `Box`, `Array` and `Map` are built
through `Box.new(v)` and `Array[T].new()` rather than by construction.

What is left is the short list of things a runnable program still needs and
the spec does not define. Each is an assumption, not a fact; they are
collected here so that changing one is a single, visible decision.

- **The method names of the library traits.** §8 names `Copy`, `Clone`,
  `Drop`, `Eq`, `Ord`, `Iterate[T]` and `From[T]` and lists the *types'*
  method sets in full, but never the traits'. `From.from` is given by the
  §4.3 example; `clone`, `eq`, `less`, `drop` and `next` are not, and are
  written here in the obvious way.
- **`String` does not implement `Clone`.** §8's `impl String` block is closed
  and has no `clone`, and nothing says `String: Clone`, so every file that
  needs an owned copy of a `&String` builds one with `String.new()` and
  `push_str`. The same pair stands in for concatenation, which §8 also omits.
- **Calling a method through a `Box`.** §4.3 requires `Box[dyn Summarize]` to
  be usable — that is the whole point of boxing a trait object — so
  `value.summarize()` on a `Box` has to work. §4.5's auto-dereference rule
  covers references, not `Box`.
- **`Map` does not implement `Iterate`.** §8 does not say it does and its
  method set is closed, so `10_loops.science` walks a map through an `Array` of
  keys and `Map.get` rather than inventing an entry type.
- **The `[…]` postfix has no operand type.** §4.4's precedence table lists
  `index`, but §8 says `Array` and `Map` are read through `get` and that no
  indexing operator may panic, and F0 has no `Index` trait. `12_operators`
  and `14_line_continuation` therefore exercise that postfix where it does
  appear in an expression: naming a generic instantiation, as in
  `Array[Point].new()`.
- **`pub`.** §4.1 reserves the word and no section gives it a rule.
  `17_modules.science` uses it as a prefix marking an item visible outside its
  file, which is the only reading consistent with "a file is a module".
- **`IoError`.** §8's `read_file` and `write_file` return
  `Result[_, IoError]` and the type is never declared anywhere.
