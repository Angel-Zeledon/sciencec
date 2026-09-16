# UI tests

Programs that must be **rejected**, each next to the diagnostics `linkc` is
expected to print for it, compared byte for byte. Spec §10.3:

> UI tests in the style of rustc's: a `tests/ui/*.link` that must fail, next
> to its expected `.stderr`, compared literally. It is the only known method
> that stops error messages degrading over time. For a language with inferred
> regions it is not optional.

Programs that must be *accepted* live in `examples/` instead.

## Running them

Through [`link-testkit`](../../crates/link-testkit). Nothing runs them yet:
no crate currently walks this directory. The hookup is six lines, and belongs
in whichever crate can render the diagnostics these cases provoke — today
that is `link-lexer`:

```rust
#[test]
fn ui() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui");
    link_testkit::run_ui_tests(&dir, |path, source| {
        let mut map = SourceMap::new();
        // The registered path is what appears after `-->`, so register it
        // relative to the repository root with forward slashes.
        let name = format!("tests/ui/{}", path.file_name().unwrap().to_string_lossy());
        let file = map.add_file(name, source.to_string());
        let (_tokens, diagnostics) = link_lexer::lex(file, source);
        link_diagnostics::render_all(&map, &diagnostics)
    })
    .assert_success();
}
```

`link-testkit` deliberately does not do this itself: it takes the compile step
as a closure so that it stays buildable and testable with no dependency on any
compiler phase.

To update the expectations after deliberately changing a message:

```sh
LINK_BLESS=1 cargo test
git diff tests/ui      # read this before committing
```

A missing `.stderr` is written instead of failing, so a new case is added by
dropping in the `.link` file and blessing once.

## The cases

| Case | Code | What it pins down |
|---|---|---|
| `tab_in_indentation` | `LK0003` | A tab used to indent a line. §4.1 fixes this code and says no attempt is made to interpret the tab. |
| `inconsistent_indentation` | `LK0004` | A line indented to a level that is not on the indentation stack: 8 spaces, after the lexer has popped 12 and is left holding 0 and 4. §4.1 fixes this code. |
| `unterminated_string` | `LK0007` | A `"` with no closing `"` before the end of the line. §9 fixes only the range, `LK0001`–`LK0099`; the lexer allocated the number. |

The `.stderr` files were produced by running the current lexer over these
three programs and reading the output, which is what blessing is for. They
are still an *expectation*: if a message changes, the change should be
visible in `git diff` and defensible, not silently absorbed.

## The format they are in

§9 says what a diagnostic *contains* — code, severity, message, primary span,
secondary spans with labels, notes, and an automatically applicable
suggestion. `link-diagnostics::render` decides how that is printed:

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

`crates/link-testkit/tests/corpus.rs` checks those last structural properties
on every file in this directory, so a hand-written expectation in the wrong
shape fails before anyone tries to run it.

## What is still missing

Only the lexical layer is covered. §11 of the spec asks for UI coverage of
things no phase can produce yet, and each one needs a case here as it lands:

- every ownership violation, with the chain of borrows that explains it
  (`LK0301` and the rest of `LK0300`–`LK0399`);
- a non-exhaustive `match`, listing the patterns that are missing (`LK0210`);
- syntax errors (`LK0100`–`LK0199`) and name resolution failures
  (`LK0200`–`LK0299`).
