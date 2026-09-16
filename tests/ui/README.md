# UI tests

Programs that must be **rejected**, each next to the diagnostics `sciencec` is
expected to print for it, compared byte for byte. Spec §10.3:

> **UI tests**: a program that must fail, beside its expected rendered output,
> compared literally. It is the only known method that stops error messages
> from decaying, and for a language with inferred regions it is not optional.

Programs that must be *accepted* live in `examples/` instead.

## How the directory is sharded

A case's expectation is the output of *one* closure, so the directory is split
by the phase that rejects the program, and each shard is walked by the suite
that can render it:

| Where | Codes | Compiled by |
|---|---|---|
| `tests/ui/*.science` | `SC0001`–`SC0099` | [`crates/science-lexer/tests/ui.rs`](../../crates/science-lexer/tests/ui.rs) — lexes only |
| `tests/ui/parse/` | `SC0100`–`SC0199` | [`crates/sciencec/tests/ui.rs`](../../crates/sciencec/tests/ui.rs) — lexes and parses |
| `tests/ui/resolve/` | `SC0200`–`SC0249` | the same suite, which then resolves |

This is what the note that used to stand here predicted: *"when the parser can
produce `SC0100`-range diagnostics the same six lines move to, or are
duplicated in, whichever crate renders them"*. They are duplicated in
`sciencec`, the one crate where the lexer, the parser and the resolver are all
in scope.

The lexical cases could not simply be moved to the fuller pipeline. A program
with a lexical error is a program the parser then reads a guess of:
`tab_in_indentation.science` alone picks up an `SC0100` and an `SC0101` on top
of its `SC0003`, and neither is what the case is about. So the lexer's suite
walks the top level and **not** its subdirectories
(`UiTestOptions::without_subdirectories`), and each shard below it is walked by
the suite that owns it.

The driver's own rule applies inside the shards: **resolution is skipped when
lexing or parsing already found an error**, exactly as in `sciencec check`. A
case in `parse/` therefore never reaches the resolver, and a case in `resolve/`
has to lex and parse clean to be about anything at all.

## Running them

```sh
cargo test -p science-lexer --test ui    # tests/ui/*.science
cargo test -p sciencec --test ui         # tests/ui/parse/, tests/ui/resolve/
```

[`science-testkit`](../../crates/science-testkit) deliberately does not do this
itself: it takes the compile step as a closure so that it stays buildable and
testable with no dependency on any compiler phase.

The one detail worth getting right is the path registered in the `SourceMap`:
it is what the renderer prints after `-->`, so it is registered as the case's
path relative to the repository root, with forward slashes —
`tests/ui/<name>.science`, or `tests/ui/parse/<name>.science` in a shard.
Register an absolute path instead and every expectation here fails on its
location line, and would fail differently on Windows and on Linux.

To update the expectations after deliberately changing a message:

```sh
SCIENCE_BLESS=1 cargo test
git diff tests/ui      # read this before committing
```

A missing `.stderr` is written instead of failing, so a new case is added by
dropping in the `.science` file and blessing once.

## The lexical cases

One case per lexical code — the two removed comparison symbols share one,
because they are one decision — plus one that proves error recovery. Between
them they cover every diagnostic `science-lexer` can emit.

| Case | Code | What it pins down |
|---|---|---|
| `unknown_character` | `SC0001` | Characters that are not part of the language: `$`, a backtick, and a lone `!` — which is the interesting one, because `!` begins no operator at all now that `!=` is gone, and §4.3 spells negation `not`. |
| `tab_in_indentation` | `SC0003` | A tab used to indent a line. §4.2 fixes this code and says no attempt is made to interpret the tab. |
| `inconsistent_indentation` | `SC0004` | A line indented to a level that is not on the indentation stack: 8 spaces, after the lexer has popped 12 and is left holding 0 and 4. §4.2 fixes this code. |
| `integer_overflow` | `SC0005` | An integer literal past `u128::MAX`, in decimal and in hex. §4.2 puts no bound in the grammar, so the bound is the lexer's accumulator. |
| `bad_escape` | `SC0006` | Escapes outside the eight §4.2 lists, reached the way it happens in practice: an undoubled Windows path. Both flavours of the code appear — an unknown escape, and a `\u` with no braces. |
| `unterminated_string` | `SC0007` | A `"` with no closing `"` before the end of the line. §9 fixes only the range, `SC0001`–`SC0099`; the lexer allocated the number. |
| `bad_character_literal` | `SC0008` | All three ways §4.2's "exactly one character, in quotes" is broken: empty, too many, never closed. |
| `invalid_numeric_suffix` | `SC0009` | `42q`, and `2.5i32` — an integer suffix on a float, which gets its own message. |
| `malformed_number` | `SC0010` | A digit outside its base (`0b1012`, `0o778`) and a prefix with no digits at all (`0x`). |
| `float_out_of_range` | `SC0011` | A float literal that `str::parse` saturates to infinity rather than rejecting, which is why the lexer has to check for it. |
| `removed_comparison_symbols` | `SC0016`, `SC0017` | The two spellings `syntax-revision-2.md` §1 removed. `==` and `!=` are still lexed as `EqEq` and `NotEq` so the expression parses as though the fix had been applied, and each costs exactly one diagnostic with an applicable fix — `is` and `is not`. The ordering symbols in the same file are untouched. |
| `several_errors_in_one_file` | six codes | Error *recovery*. The lexer never aborts, so one pass reports `SC0001`, `SC0006`, `SC0007`, `SC0009`, `SC0010` and `SC0003`, in source order. No single message is the point here; the count and the order are. |

The `.stderr` files were produced by running the current lexer over these
programs and reading the output, which is what blessing is for. They are
still an *expectation*: if a message changes, the change should be visible in
`git diff` and defensible, not silently absorbed.

Three messages that used to be worse than they should be have since been
fixed, and the expectations here now pin the good versions:

- `unknown_character` renders a backtick as `` ` `` — the message escapes the
  character before interpolating it, instead of emitting bare backticks.
- `unknown_character` tells someone who typed `!` that negation is spelled
  `not`, notes that `!` begins no operator in Science, and offers `not ` as
  an applicable fix. It no longer points at `!=`, which §1 removed.
- `float_out_of_range` prints `f64::MAX` as `1.7976931348623157e308` rather
  than 309 decimal digits, and `malformed_number` says "an octal literal".

## The format they are in

§9 says what a diagnostic *contains* — code, severity, message, primary span,
secondary spans with labels, notes, and an automatically applicable
suggestion. `science-diagnostics::render` decides how that is printed:

```
error[LKnnnn]: <message>
 --> <path>:<line>:<column>
  |
L | <the source line, verbatim>
  | ^^^ <primary label>
  |
  = note: <note>
  = help: <suggestion message>: `<replacement>`
```

Details that are easy to get wrong when writing or reviewing one by hand:

- **The gutter is as wide as the widest line number printed.** With one digit
  the rule is `  |` and the header is ` --> `; with two digits they become
  `   |` and `  --> `.
- **The `-->` header points at the first primary label**, not at the first
  label.
- **Source lines are printed verbatim, with no tab expansion**, and are
  trimmed at the right. `tab_in_indentation.stderr` therefore contains a real
  tab character; do not let an editor turn it into spaces.
- **A marker row is indented by `column - 1` spaces** and repeats its marker
  once per character of the span. Secondary labels use `-`, the primary uses
  `^`. Each label gets its own row, even when several share a source line.
- **Notes come before suggestions**, after a blank rule line, as `= note:`
  and `= help:`.
- **A suggestion prints its replacement** in backticks after the message, so
  an unhelpful replacement looks unhelpful.
- **There is no trailing summary line.** `render_all` joins diagnostics with a
  blank line and stops; nothing prints "aborting due to N previous errors".
- **Paths are as registered in the `SourceMap`**, so the caller decides them.
  These files assume repository-relative paths with forward slashes, which is
  what makes the same expectation work on Windows and on Linux.

`crates/science-testkit/tests/corpus.rs` checks those last structural properties
on every file in this directory, so a hand-written expectation in the wrong
shape fails before anyone tries to run it.

## The syntax cases, in `parse/`

| Case | Code | What it pins down |
|---|---|---|
| `try_word` | `SC0155` | The word revision 2 §3 removed with `Result`. The point is the **absence of a fix**: every other migration code renames a word and carries a `= help:` with a replacement a tool can apply, and this one cannot, because the new model turns one expression into a binding, a test and a return, and which value to return on the error path is not something the parser can know. The explanation is in the note instead, and a `= help:` line appearing here would be the regression. |
| `function_word` | `SC0156` | The word revision 3 renamed to `def`, which *does* carry an applicable fix — a word for a word, with nothing around it moving. **The second diagnostic in its expectation is a bug**, described in the file and in the section below. |
| `const_factor` | `SC0157` | `N * M`, the refusal `const-expression-arithmetic.md` §2.1 exists for. Also its recovery: the parser steps to the end of the const argument so the argument list still closes, and the case proves there is exactly **one** diagnostic and no cascade. |
| `reserved_word_as_name` | `SC0102` | `def await(…)`. `await` is reserved for a later revision, so it is its own token and never an identifier; the parser refuses the name, and `describe` says which kind of word it is rather than leaving a later phase to complain about something that was never a name. |

## The name-resolution cases, in `resolve/`

| Case | Code | What it pins down |
|---|---|---|
| `const_param_kind` | `SC0220` | `const N: Str`. §2.3 admits exactly two kinds, `Int` and `Shape`, and the parser deliberately does not check — it reads whatever `parse_type` accepts — so this is the resolver's refusal. `Str` is the interesting wrong answer: a real type, in scope, spelled correctly, and still not a kind. |

## Messages these cases found wrong

Written down rather than quietly blessed, because a test that pins a confusing
error makes it permanent.

- **`SC0156` cascades, and the second diagnostic is nonsense.** `parse_item`
  reports the word and steps over it, then calls `parse_fn`, which opens with
  another `advance()` and so eats the function's *name*. The reader is told
  `expected an identifier, found `(`` about a declaration they spelled
  correctly apart from its first word. `report_trait_word` beside it is
  factored correctly: it advances, and the parser then calls
  `parse_interface_body`, which does not advance again. `function_word.stderr`
  pins both diagnostics today; when this is fixed the expectation shrinks to
  one and the diff will say so.
- **`SC0157` says "multiplies" for a division.** The same code is reported for
  `/`, so `Grid of (Int, N / M)` is rejected with *"a const expression
  multiplies only by a literal"* and a note ending *"never multiplied"*.
- **`SC0157` says "on one side" where the parser means "on the right".**
  §2.1's grammar has `IntLiteral '*' ConstTerm` as well as `ConstTerm '*'
  IntLiteral`, so `2 * N` is legal and the parser rejects it — with a message
  telling the reader to do the thing they did. For `/` the grammar really does
  require the literal on the right, so the phrase is wrong there in the other
  direction. There is no case here for either: one for `2 * N` belongs in
  `examples/` once it is accepted.
- **Every `SC0100`–`SC0106` label repeats its message word for word.** The
  parser's `error` helper passes the message as the label, so a one-line
  diagnostic says the same sentence twice, once after the code and once under
  the caret. The message is the general statement, the label is what is wrong
  *here*; they should not be the same string.
- **"reserved for a later phase"** (`SC0102` over a reserved word) reads, to
  anyone who knows what a compiler phase is, as though a later pass will accept
  it. It means a later revision of the language.

## What is still missing

The lexical layer is complete: every code `science-lexer` can emit has a case
above. §11 of the spec asks for UI coverage of things no phase can produce
yet, and each one needs a case here as it lands:

- every ownership violation, with the chain of borrows that explains it
  (`SC0301` and the rest of `SC0300`–`SC0399`);
- a non-exhaustive `match`, listing the patterns that are missing (a code in
  the `SC0250` range, which §9 moved it to from `SC0210`);
- the rest of the syntax errors (`SC0100`–`SC0199`) and of the name
  resolution failures (`SC0200`–`SC0299`): `parse/` and `resolve/` cover five
  codes between them, and every other code those two phases can emit still has
  no case here. The parser's `extern` block owns `SC0411`–`SC0434` and has
  none, `science-types` (`SC0260`, `SC0261`) has none, and `science-fmt`
  (`SC0900`, `SC0901`) has none.
