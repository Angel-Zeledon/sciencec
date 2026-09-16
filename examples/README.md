# Link example corpus

Valid `.link` programs, one file per language feature, written against
[`docs/superpowers/specs/2026-09-16-link-f0-core-design.md`](../docs/superpowers/specs/2026-09-16-link-f0-core-design.md).

These files are **input data for the lexer and parser test suites**. They are
meant to be syntactically correct, so anything that fails to lex or parse here
is a bug in the compiler or a gap in the spec — not a typo to be quietly
patched. Programs that must *fail* live in `tests/ui/` instead.

Identifiers are in English, matching the English keywords.

## What each file covers

| File | Spec | Covers |
|---|---|---|
| `00_kitchen_sink.link` | §11 | **The F0 acceptance program.** Generics with bounds, traits with static *and* dynamic dispatch, exhaustive `match` over enums, `Option` and `Result` with `?`, and a struct storing a borrow in a field — all in one program. |
| `01_functions.link` | §4.3 | Functions with and without a return type, explicit `()`, last expression as the value, early `return`, parameters by value / `&` / `&mut`, recursion. |
| `02_bindings.link` | §4.3, §5.1 | `let` and `let mut`, optional type annotations, reassignment, one binding of every primitive type, `Int`/`Float` aliases, `as` casts. |
| `03_structs.link` | §4.3, §6.2 | Struct declaration, construction with mandatory named arguments in any order, field access, nested structs, generic structs, and fields that are `&`/`&mut` references. |
| `04_enums.link` | §4.3, §5.5 | C-like enums, positional payloads, multi-field variants, generic enums (`Option`, `Result`, `Either`, `Tree`), and a variant carrying a borrow. |
| `05_match.link` | §4.4 | Every pattern form: literals (int, char, string, bool), wildcard `_`, bindings, enum variants with payload, struct patterns, tuple patterns, alternatives with `|`. Inline and block arms, nested matches, `match` as an expression. |
| `06_traits.link` | §4.3, §5.4 | Required methods, default methods that call each other, overriding a default, several impls, `&self` / `&mut self` / `self`, generic traits, and impls of `Clone`, `Eq`, `Drop`, `From`. |
| `07_generics.link` | §4.3, §5.3 | Unbounded generics, one bound, several bounds with `+`, `where` clauses (single and multi-parameter, and combined with in-list bounds), generic structs and enums, nested generic types. |
| `08_dyn_dispatch.link` | §4.3, §5.3 | `dyn` behind `&`, behind `&mut`, behind `Box`, as a return type, as a struct field, and in a heterogeneous `Array[Box[dyn Trait]]` — contrasted with the statically dispatched version of the same call. |
| `09_option_result.link` | §4.4, §5.5 | `Option` and `Result` as return types, `?` on both, `?` widening an error through `From`, `?` inside a larger expression, and consuming both with `match`. |
| `10_loops.link` | §4.4 | `for … in` over `Array` and `Map`, `while`, `loop`; `break` and `continue`; inline bodies; a `loop` whose body is a `match`; three levels of nesting. |
| `11_literals.link` | §4.1 | Decimal / hex / binary / octal integers, `_` separators, every integer and float suffix, exponents, every string escape including `\u{…}`, character literals, booleans, unit. |
| `12_operators.link` | §4.4 | Every operator in the precedence table, with the grouping each level implies spelled out in comments: call / index / field / `?`, unary `-` `not` `&` `&mut`, `as`, `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|`, comparisons, `and`, `or`, `=`. |
| `13_inline_blocks.link` | §4.2 | The inline block form `if a: b else: c`, each time next to the indented form of the same construct: `if`/`else`, `while`, `for`, `loop`, `match` arms, and inline blocks nested in bindings, arguments and `return`. |
| `14_line_continuation.link` | §4.1 | Implicit line continuation inside unclosed `(` and `[`: signatures, generic parameter lists, nested generic types, calls, named-argument construction, arithmetic and boolean expressions, indexing, and comments inside brackets. |
| `15_comments.link` | §4.1 | `#` to end of line: at column 0, trailing, inside blocks, over-indented, between an `if` body and its `else`, with unicode and with characters that would otherwise lex as tokens. Comment-only and blank lines take no part in the indentation calculation. |
| `16_indentation.link` | §4.1, §4.2 | Eight levels of nesting, dedenting several levels in one step, an `if`/`else` ladder, inline blocks inside indented ones, and blank lines inside a block. |
| `17_modules.link` | §4.3 | `use text.parser`, `use text.parser (Token, lex)`, deeper paths, `mod` declarations both bare and with an inline body, nested modules, and `pub` on structs, enums, traits and functions. |
| `18_ownership.link` | §6.1, §6.2 | Programs that the borrow checker must **accept**: moves by argument / return / rebinding, many simultaneous shared borrows, one exclusive borrow, borrows returned from functions, borrows stored in struct fields, `Copy` types, `Box`, and scope-based `Drop`. |
| `19_stdlib.link` | §8 | The whole F0 library surface: `Option`, `Result`, `Box`, `String` and `&String`, `Array`, `Map`, `print`/`println`, `read_file`/`write_file`, and the traits `Copy`, `Clone`, `Drop`, `Eq`, `Ord`, `Iterate`, `From`. |

## Where these files go beyond the spec

The spec pins down the grammar of every construct it shows, but a runnable
program needs a few things it never writes down. Each of these is an
assumption, not a fact; they are collected here so that changing one is a
single, visible decision.

- **Method names on library types.** §8 names `String`, `Array`, `Map`,
  `Option`, `Result` and `Box` but gives no method list. Calls such as
  `text.truncate(80)`, `items.push(1)` or `settings.get(key)` are plausible
  placeholders. The call *syntax* is what these files are testing.
- **Constructing `Box` and the library collections.** Written as `Box(value)`,
  `Array()` and `Map()`, following the positional call form that enum variants
  such as `Some(x)` already use. Named-argument construction is specified for
  structs only.
- **No inherent `impl`.** The spec only ever shows `impl Trait for Type`, so
  every method here arrives through a trait. If inherent impls exist, no file
  uses them.
- **Generic traits.** F0 has no associated types (§5.4), so `Iterate` and
  `From` are written with a type parameter — `impl Iterate[Int] for Countdown`,
  `impl From[ParseError] for LoadError` — which is the only way left to
  express them.
- **Struct patterns.** Written in the same named form used to build a struct:
  `Point(x: 0, y: y)`. §4.4 lists struct patterns without giving their syntax.
- **Enum variants unqualified.** Written `Some(x)`, `Ok(v)`, `Plain`, matching
  the spec's own examples, rather than `Option.Some(x)` or `Format.Plain`.
- **Trailing commas** inside bracketed lists, used in
  `14_line_continuation.link`.
- **No range syntax**, so every `for` walks a collection.
- **Marker traits.** `Copy` has no methods and F0 has no empty block, so
  `19_stdlib.link` documents that `impl Copy for T` cannot currently be
  written rather than inventing a syntax for it.
- **`\'` is not used.** §4.1 lists `\"` among the escapes but not `\'`, so no
  character literal here contains a quote.
