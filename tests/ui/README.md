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
| `function_word` | `SC0156` | The word revision 3 renamed to `def`, which *does* carry an applicable fix — a word for a word, with nothing around it moving. It reported **two** diagnostics when the case was written, and the expectation has since shrunk to one; the file says what the second one was and why it happened, because the shape of that bug is the reason every recovery in `SC0190`–`SC0198` reports without consuming. |
| `const_factor` | `SC0157` | `N * M`, the refusal `const-expression-arithmetic.md` §2.1 exists for. Also its recovery: the parser steps to the end of the const argument so the argument list still closes, and the case proves there is exactly **one** diagnostic and no cascade. |
| `reserved_word_as_name` | `SC0102` | `def await(…)`. `await` is reserved for a later revision, so it is its own token and never an identifier; the parser refuses the name, and `describe` says which kind of word it is rather than leaving a later phase to complain about something that was never a name. |
| `returns_word` | `SC0118` | The word §4.4 replaced with `->`. It is not one of revision 2's migrations — it predates them — but it is the same shape: an ordinary identifier where a keyword used to be, reported before the grammar can complain that a name is not the end of a line. Its fix replaces exactly the word the caret is under, which is what three of the six below **did not** do until the entry below was fixed. |

The six below are `syntax-revision-2.md`'s migration block,
`SC0138`–`SC0144`.
Each names a word the revision removed, and each carries a **machine-applicable
fix** that tooling applies without anyone reading it. The parser's own snapshots
pin a diagnostic's code, span, message and notes and pin neither the label nor
the suggestion, so before these cases the replacement text was checked by
nothing at all.

| Case | Code | What it pins down |
|---|---|---|
| `each_after_for` | `SC0138` | `for each x in xs`. The one word in the block that is still a keyword — `each` remains the closure subject of `docs.map(each.title)` — so only the loop dropped it, and the note says so. Its fix spans `for each`, not `each`, and **so does its caret**: the expectation pins both, which is what the entry below fixed. The label still names `each`, because that is the word that is wrong. |
| `trait_word` | `SC0139` | The word revision 2 §6 renamed to `interface`. One word for one word, reported without consuming, which is the shape `SC0156`'s bug was fixed into. |
| `methods_after_has` | `SC0141` | `Type has methods:`. Its fix spans `has methods`, and so does its caret. This was the clearest of the three the entry below is about: a caret under `methods` beside *"open the block with: `has`"* told the reader to write the word they had already written. |
| `while_word` | `SC0142` | The loop §2.2 removed. The only fix here that *discards* text: the replacement spans the whole head, condition included, because `loop` takes none, and what to do with the condition is in the note because it is not a substitution. Recovery drops the condition too, so the tree and the fix agree — and the caret spans the head as well, which is the one place in the block where widening it also made the diagnostic *honest* about what disappears. |
| `comparison_phrase` | `SC0143` | All four phrases, because they are one decision — the shape `removed_comparison_symbols` has in the lexical shard. The label interpolates the phrase as written, so each spelling renders its own text, and the two-word and three-word forms take different spans. |
| `println_word` | `SC0144` | The word §3.5 renamed to `print`. Recovery returns the `print` call that was meant, so the argument is parsed once and no second diagnostic follows about an undefined name. |

The nine below are `mcp-servers.md`'s block, `SC0190`–`SC0198`, and they are
the whole of that note's §14.2 stage 0: every one of them is checkable by the
parser with no types at all, which is why the `tool` declaration could land
before the type checker that enforces *the schema is the signature*. Each case
is **one mistake and one diagnostic**; a second diagnostic appearing in any of
these expectations is the regression they exist to catch.

| Case | Code | What it pins down |
|---|---|---|
| `tool_without_description` | `SC0190` | Decision 5, the only construct in Science for which documentation is mandatory. The message carries the *reason* — a model reads the description in order to decide whether to call the tool — because without it the diagnostic is "document your code", which a compiler has no business saying. The second note is the other half: an undescribed tool does not error, it is simply never chosen. |
| `tool_summary_blank` | `SC0195` | The run exists and all of it is sent; what is missing is the summary §5.2 takes the `title` from. The distinction from `SC0190` is the point, and it is the reason two codes were spent rather than one. The caret is on the `##` run and a secondary label names the `tool`, so the expectation pins a multi-line primary span and the `(continues to line N)` the renderer draws for one. |
| `generic_tool` | `SC0191` | `tool f of T(…)`. Also its recovery: the parameter is **kept** after the report, so `T` still resolves where it is used and one stale word does not cost a name-resolution failure per mention. Exactly one diagnostic. |
| `borrowed_tool_parameter` | `SC0192` | Both spellings, so the deleted span is pinned for `borrowed` alone and for `mutable borrowed` as a pair. Two parameters are two mistakes and two diagnostics, which is the count this case checks rather than a cascade. |
| `tool_with_receiver` | `SC0193` | The **absence of a fix**, for `try_word`'s reason in a smaller way: deleting `self` from a list that continues would leave a leading comma, so there is nothing a tool can apply and the note says what to do instead. |
| `parameter_doc_outside_tool` | `SC0194` | Decision 7 scoped to `tool` and to nothing else, so that documenting a `def`'s parameters stays `strings-formatting-and-docs.md`'s open question. No fix: moving prose from one comment into another is an edit, not a substitution. The caret is on the `##` comment, with the parameter's name as a secondary label. |
| `reserved_declaration_word` | `SC0196` | One case for two words, because they are one decision — Decision 2 spends `tool` and only `tool`. Each carries its own reason, and what it replaces is the reason it exists: without it both fall through to `SC0101`, which calls them reserved "for a later phase". |
| `tool_not_at_module_level` | `SC0197` | Two of §16.1's four places, which are the two code paths: `parse_member`, shared by the interface and implementation bodies, and `parse_stmt`. Both report *without consuming*, so the enclosing loop's own recovery drops the declaration and its block in one step — which is exactly what `SC0156` failed to do. |
| `tool_without_body` | `SC0198` | There is no abstract tool. The neighbour it must not become is a `tool` whose body is indented with no `:`: that already reports the missing colon, and saying the body is absent as well would be a second true statement about one mistake. |

## The name-resolution cases, in `resolve/`

| Case | Code | What it pins down |
|---|---|---|
| `const_param_kind` | `SC0220` | `const N: Str`. §2.3 admits exactly two kinds, `Int` and `Shape`, and the parser deliberately does not check — it reads whatever `parse_type` accepts — so this is the resolver's refusal. `Str` is the interesting wrong answer: a real type, in scope, spelled correctly, and still not a kind. |
| `unresolved_name` | `SC0200` | The most-seen error in any compiler: a bare name searched for in the ribs, the module, its variants, the prelude and the crate root, and found in none of them. |
| `unresolved_variant` | `SC0200` | The code's other sentence. `Signal` resolves and the segment after it does not, so the message names the choice rather than calling the whole path unknown. It has to be a **pattern**: an expression path is one segment and every `.` after it is field access (§4.3), so `Signal.Pending` written as a value never reaches this walk at all. |
| `duplicate_definition` | `SC0201` | Two `def`s of one name in one module. Two labels: the second declaration is the error and the first is the context. |
| `unresolved_import` | `SC0202` | `use text.parser` in a crate of one file. The report names the segment that failed, not the whole path. |
| `ambiguous_name` | `SC0203` | §4.5 puts a variant in its module unqualified as well as qualified, and two choice types may share a variant name. One note per candidate, in declaration order. |
| `not_a_module` | `SC0204` | `Doc.Title` in type position. Only a module and a choice type have anything a `.` can reach in a path; a record's fields are reached through a value. |
| `unknown_field` | `SC0205` | A record literal naming a field the record lacks. Which fields are *missing* is the type checker's; which ones do not exist at all is a name. |
| `construction_mismatch` | `SC0206` | `Doc("a")`. §4.4's second point: positional arguments mean a call or a variant, named arguments mean a record, and the parser cannot see which `Doc(..)` is. |
| `orphan_impl` | `SC0207` | `String implements Clone`. Both halves belong to the prelude, which is a module no file can name, so §5.4 cannot be satisfied from anywhere. |
| `self_outside_impl` | `SC0208` | A `self` receiver on a free function. The parser accepts a receiver that is first in its list wherever the list is (`SC0108` is only about a *later* one), so whether anything encloses it is this phase's question. |
| `wrong_namespace` | `SC0211` | Both reporters, because they are different sentences: a function where a type belongs, and `ffi` — the prelude's one module — where a value belongs. Neither is `SC0200`: both names resolve. |
| `each_without_subject` | `SC0212` | A bare `each` with no call around it. The parser reports a *nested* `each`, which it can see; it cannot see whether there is an enclosing argument at all, so the bare node arrives here. |

`SC0209`, "one of the words §4.1 reserves for F1-F4, used as a name", has no
case and **cannot** have one. Every reserved word is its own token kind, and
`expect_ident` refuses a token that is not `Ident` rather than building an
`Ident` out of it, so no `ast::Ident` produced by this parser can ever carry
one. A reserved word in a name position is `SC0102` and a reserved word in an
expression is `SC0105`; `check_reserved` fires only for a tree some other
program built. See the entry below.

## Messages these cases found wrong

Written down rather than quietly blessed, because a test that pins a confusing
error makes it permanent.

- **`SC0156` cascaded, and the second diagnostic was nonsense.** Fixed, and
  kept here because it is the pattern rather than the incident. `parse_item`
  reported the word and stepped over it, then called `parse_fn`, which opens
  with another `advance()` and so ate the function's *name*; the reader was
  told ``expected an identifier, found `(` `` about a declaration they had
  spelled correctly apart from its first word. `report_trait_word` beside it
  never had the bug, because it reports and lets the parser do the stepping.
  `function_word.stderr` pins one diagnostic now. Every reporter in
  `SC0190`–`SC0198` was written against this: `report_tool_out_of_place` and
  `report_reserved_declaration_word` consume nothing and hand `None` to a
  caller that already synchronises.
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
  it. It means a later revision of the language. `SC0196` now intercepts the
  two words most likely to be met this way — `prompt` and `agent` in
  declaration position — so the phrase is left standing for the rest of the
  list, where nothing yet says what each word is being held for.
- **`SC0194` and `SC0195` pointed at the declaration, not at the `##` line
  they are about.** **Fixed**, and kept here for what it cost. A doc comment is
  trivia carried on a token, and it was carried as a bare string: `Token::doc`
  was an `Option<String>` and nothing recorded where the run had been. `SC0195`'s
  caret therefore landed on the word `tool` and `SC0194`'s on the parameter's
  name, and in both cases the text the reader had to edit was on the line above
  the one the renderer printed.

  `Token::doc` is now an `Option<DocComment>` — the same text, plus the span of
  the `##` lines, first `#` to the last character of the last line, trailing
  whitespace excluded. The two diagnostics point at that span, and each keeps
  the span it used to point at as a *secondary* label, so the snippet still
  shows which declaration is meant: without one the renderer would print the
  run alone and the reader would count lines to find it. `SC0190` did not move:
  it is about a run that is **not there**, and an absent run has no span.

  What it cost, which is the part worth keeping: every consumer of `Token::doc`
  had to be told. `science-fmt`'s round-trip check compared the whole field and
  now compares the text only, because re-indenting a run is exactly what a
  formatter does and the span moving is not the run changing. The syntax tree
  was left alone — `ast::Item` and `ast::Param` still hold an `Option<String>` —
  because every later phase reads a doc comment as prose and none of them points
  at one; `sciencec tools --json` derives a tool's description from it and is
  byte-identical over `examples/` and over `crates/sciencec/tests/cli.rs`.

- **Three migration fixes replaced more text than their caret covered, and the
  renderer prints neither span.** **Half fixed** — the three, not the class.
  `SC0138`, `SC0141` and `SC0142` each put the primary label on one word and the
  `Suggestion` on a wider span: `each` versus `for each`, `methods` versus
  `has methods`, `while` versus the whole head `while n > 0`. A tool applied the
  right thing. A reader saw a caret under `methods` and a line saying *"open the
  block with: `has`"* and was being told to write what they had already written
  — and taken literally it gave `Doc has has:`. The four sound reporters beside
  them (`SC0118`, `SC0139`, `SC0143`, `SC0144`) label exactly the span they
  replace.

  The repair taken was the cheap one: widen the primary label to the
  suggestion's span, so the caret and the `= help:` line are about the same
  text. For `SC0142` that also made the diagnostic honest — the condition really
  is discarded, by the fix and by the recovery alike — and its label says so
  now, because a caret over `n > 0` beside *"`while` is not a keyword"* shows
  two facts and explains one. `each_after_for.stderr`,
  `methods_after_has.stderr` and `while_word.stderr` pin the new text, and the
  parser's snapshots pin the widened spans; no tree changed, so nothing cascaded.

  **The general fix was not taken and is still open.** The renderer prints a
  caret and a `= help:` line and names the span of neither, so a suggestion
  whose span differs from its label's is invisible to the reader in every
  diagnostic, not only these three. Teaching `science-diagnostics::render` to
  draw the suggestion's span when it differs would fix the class. It was left
  because it is a change to how *every* diagnostic in the compiler renders: it
  has to decide what a second span is drawn as and where, and it has to decide
  what to do about the legitimate cases in the other direction — `SC0116`
  deliberately labels wider than it replaces, and `SC0107`'s deletion span
  swallows the trailing space after `public` — and it would re-bless every
  expectation in this directory. Widening three labels makes three messages
  right today and does not stand in the way of it.
- **`SC0203`, `SC0204` and `SC0205` print an absolute crate path where the
  reader wrote a bare name.** `self.defs.path_of(id)` yields
  `tests.ui.resolve.unknown_field.Doc` in this shard and `main.Doc` in a
  one-file program, so the message names something that appears nowhere in the
  source. `SC0206`, reporting the same record two lines away in the same file,
  prints `path.dotted()` — `Doc`, as written — which is what all four should
  do, qualifying only when the definition is in another module. It also makes
  these expectations depend on the directory the case is filed in, which no
  other expectation here does.
- **`SC0203`'s notes offer a path instead of the qualification to write.** The
  last note says *"write the choice type's name to say which (§4.5)"* and the
  notes above it say *"it could be `tests.ui.resolve.ambiguous_name.Signal.Ready`"*.
  The thing to write is `Signal.Ready`. The note should name each candidate the
  way §4.5 spells it, relative to the use site.
- **`SC0207` recommends something the compiler has made impossible, and says
  `core` twice.** For `String implements Clone` it prints *"the interface
  `core.Clone` belongs to `core`"*, then *"move the implementation into one of
  those modules, or wrap the type in one of your own"*. `core` is the prelude
  — the resolver's own documentation says it is "a module no `use` can reach",
  and that is precisely why this is an orphan — so the first remedy cannot be
  followed and is printed first. When every owner is the prelude the note
  should say so and offer only the wrapper. The doubled name is separate and
  smaller: `belongs to` already carries the module, so the interface should be
  named `Clone`.
- **`SC0209` is dead code, and its message is dead text.** It says *"it is held
  back so that a later phase of the language can use it without breaking code
  written today"* — a better sentence than the `SC0102` a reader actually gets,
  which is the "reserved for a later phase" wording criticised above. Nobody
  will ever see it. Either the parser should hand the reserved name through so
  this phase can say the better sentence, or `SC0209` should be retired and its
  wording moved into `SC0102`.
- **A typo in a qualified variant escapes this phase entirely when it is
  written as a value.** `Signal.Pending` in expression position is parsed as
  field access on `Signal`, and field access is not resolved here, so
  `unresolved_variant` had to be written as a pattern to get a diagnostic at
  all. The value form is left to `science-types`, which will report it as a
  missing *field* rather than a missing *variant*. Recorded here because the
  case next to it shows what the good message looks like.

## What is still missing

The lexical layer is complete: every code `science-lexer` can emit has a case
above. §11 of the spec asks for UI coverage of things no phase can produce
yet, and each one needs a case here as it lands:

- every ownership violation, with the chain of borrows that explains it
  (`SC0301` and the rest of `SC0300`–`SC0399`);
- a non-exhaustive `match`, listing the patterns that are missing (a code in
  the `SC0250` range, which §9 moved it to from `SC0210`);
- the rest of the syntax errors (`SC0100`–`SC0199`) and of the name
  resolution failures (`SC0200`–`SC0299`): `parse/` and `resolve/` cover
  thirty-two codes between them, and every other code those two phases can
  emit still has no case here. In the resolver that is `SC0221` and `SC0209`,
  which cannot be reached at all (above); in the parser it is `SC0100`–
  `SC0101`, `SC0103`–`SC0112`, `SC0115`–`SC0116` and `SC0119`.
  The parser's `extern` block owns `SC0411`–`SC0434` and has none, `science-types` (`SC0260`, `SC0261`) has none, and `science-fmt`
  (`SC0900`, `SC0901`) has none. `SC0190`–`SC0198` are covered in full;
  `SC0199` is held unallocated and must stay that way.
