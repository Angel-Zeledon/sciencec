//! `science-fmt` — the Science source formatter.
//!
//! This crate exists because `docs/superpowers/design/self-hosting.md` §6.3
//! rejected "freeze the syntax" as a gate on self-hosting — it is not
//! falsifiable — and replaced it with **mechanisability**: gate E1 is
//! *"`sciencec fmt` exists and is idempotent"*, and gate E2 is a migration tool
//! that replays a syntax revision and reproduces the committed corpus byte for
//! byte. Two other design notes already assume a formatter
//! (`llm-ergonomics.md` §4.1 is entirely about canonicalisation for it, and
//! `syntax-revision-2.md` §1.1 chose its comparison syntax on the grounds that
//! the alternative was *"permanently un-normalisable"*). This is the first one.
//!
//! # What makes a Science formatter different
//!
//! Science delimits blocks by indentation (§4.2). A formatter for a braced
//! language that gets the indentation wrong produces ugly code; a formatter for
//! this language that gets it wrong produces **different** code, or code that
//! does not parse. So the acceptance criteria are not aesthetic:
//!
//! 1. `fmt(fmt(x)) == fmt(x)` for every file in `examples/`;
//! 2. the syntax tree of `fmt(x)` is identical to the tree of `x`, modulo
//!    spans, for every file in `examples/`;
//! 3. every file in `examples/` still passes `sciencec check` afterwards.
//!
//! `crates/science-fmt/tests/` holds all three. They are the point; the style
//! rules below are what is left over once they are satisfied.
//!
//! The implementation keeps a promise stronger than (2): **the output's token
//! stream is the input's token stream.** Not an equivalent one — the same
//! `Vec<TokenKind>`, in order. Most of the decisions below fall out of that
//! promise, and it is why the layout never needs to look at the syntax tree.
//!
//! It is not, however, sufficient, and finding out why was the most useful
//! thing this crate did. `Parser::expect_pkg_config` accepts the three tokens
//! `pkg`, `-`, `config` **only when their spans touch**, so a space inserted
//! between two tokens can change the parse with the token stream untouched. So
//! [`format_source`] checks both: before it hands any text back it re-lexes it,
//! compares the tokens, re-parses it, compares the trees modulo spans, and
//! compares the comments. A formatter that cannot prove its output is the same
//! program is a formatter nobody can run over a whole repository.
//!
//! # The decisions
//!
//! Each one is stated, then justified, then costed.
//!
//! ## Indent width: four spaces
//!
//! **Decision.** One block level is four spaces. Tabs are never emitted.
//!
//! **Reason.** All 22 files in `examples/` use four, the spec's examples use
//! four, and a tab in the indentation is `SC0003`. There is no argument to
//! have.
//!
//! **Cost.** Deeply nested code runs out of width fast, and
//! `examples/16_indentation.science` goes eight levels deep on purpose. At
//! eight levels a line starts at column 32. That is the language's problem
//! rather than the formatter's, but the formatter is where it is felt.
//!
//! ## Maximum line width: 90 columns
//!
//! **Decision.** A line is broken when it would otherwise exceed 90
//! characters. A trailing comment does not count toward the width.
//!
//! **Reason, and this decision was 100 until the corpus was measured
//! properly.** The original argument was that code reaches 97 and 93, that any
//! limit at or below 97 would break lines their author chose not to break, and
//! that 100 is therefore "above everything checked in".
//!
//! The distribution says otherwise. Per-file maximum code width across the
//! twenty-two corpus files is:
//!
//! ```text
//! 84 71 68 86 65 69 82 82 [97] 76 85 83 79 80 70 61 81 60 71 88 66 82
//! ```
//!
//! **Exactly one line in twenty-two files exceeds 88 columns**, and it is
//! `examples/08_dyn_dispatch.science:129` — a `let` with no `where`, no chain
//! and a doubled `Array of (Box of any Summarize)`, which is an outlier with no
//! good break available anyway. The corpus's real working width is about 86.
//!
//! So 100 was not "above everything checked in" in any useful sense: it was
//! twelve columns above everything but one line, and its first act was to
//! manufacture lines of 91, 98 and 99 — each wider than the widest line in
//! twenty-one of the twenty-two files. The old claim that it "changes line
//! breaking only where the author broke a line that did not need breaking" was
//! true as stated and wrong in effect: by the corpus's own standard those
//! lines did need breaking.
//!
//! The 93 in the original argument is also gone. `examples/19_stdlib.science`
//! measures 88 today, because revision 3 renamed `function` to `def` and took
//! five columns off every declaration in the language. The width question was
//! settled against numbers that the language then moved.
//!
//! **Cost.** There is still no width that agrees with the corpus, and that is
//! worth recording rather than hiding. `examples/08_dyn_dispatch.science`
//! leaves a line at 97 columns unbroken; `examples/00_kitchen_sink.science`
//! breaks `def best_of …` before its `where` at 95. No single number honours
//! both. 90 contradicts the first, which is one line and an outlier by the
//! distribution above; 100 contradicted the second, and two more like it.
//!
//! 90 is also close enough to the corpus's working width that the formatter
//! now breaks lines real code writes flat, which 100 never did. When it gets
//! that wrong the answer is `# fmt: off` — see [`suppress`] — and not a wider
//! limit, because a limit wide enough never to be wrong is a limit that does
//! nothing.
//!
//! Excluding the trailing comment from the measurement is deliberate: a line
//! broken into four because someone wrote a long sentence after it is a line
//! whose shape is decided by prose.
//!
//! ## A long list breaks inside its brackets, one item per line
//!
//! **Decision.** When a logical line does not fit, the widest bracket group at
//! its top level is exploded: everything up to and including the opening
//! bracket stays on the first line, each comma-separated item gets a line at
//! `+4`, and the closing bracket — together with everything after it — goes on
//! a line of its own at the original indent.
//!
//! **Reason.** §4.2 makes indentation *inert* inside unclosed brackets, so this
//! is the one place a formatter may put a line break without the lexer
//! noticing. `examples/07_generics.science` already breaks inside parameter
//! lists for exactly this reason and says so. The widest group is chosen
//! rather than the first or the last because that is where the line is
//! actually long, and because "widest" is a function of the tokens alone —
//! nothing about the input's layout feeds into it, which is half of why
//! formatting twice gives the same answer as formatting once.
//!
//! **Cost.** Only one group is exploded per level. A line with two long
//! argument lists side by side gets one of them broken and leaves the other
//! flat and over-wide.
//!
//! ## A trailing comma is an instruction, and it has three states
//!
//! **Decision.** The formatter never adds or removes a comma, and a group's
//! trailing comma means one of three things:
//!
//! | The group | What happens |
//! |---|---|
//! | no trailing comma | joined onto one line whenever it fits |
//! | trailing comma, items all on one line | exploded, one item per line |
//! | trailing comma, items already on several lines | **kept exactly as the author shared them out** |
//!
//! **Reason.** The formatter may not change the token stream, and `(a, b)` and
//! `(a, b,)` are different token streams. Deciding whether the difference
//! matters is the parser's job — F0 has tuples, and a trailing comma in a
//! parenthesised expression is exactly the kind of thing that means one thing
//! in one position and another elsewhere. Since the comma has to be preserved
//! anyway, it may as well be given a meaning, and Black and Prettier have both
//! shown that "the author left a trailing comma, so keep this exploded" is a
//! rule people like.
//!
//! The third state is the one those two formatters do not have, and
//! `examples/20_extern.science` is why. Its BLAS bindings group
//! `m: BlasInt, n: BlasInt, k: BlasInt` on one line because the dimension
//! triple is *one thing* in the C prototype, and put each `a`/`lda` pair
//! together because a matrix and its leading dimension are one argument in two
//! halves. One item per line is not a tidier version of that; it is a
//! different statement about what the arguments are. Nothing in the tokens can
//! tell the formatter which lists are like this, but the author's own line
//! breaks can, and they are already there.
//!
//! It stays idempotent because the rule is a fixpoint on its own output: the
//! items come out on exactly the lines they went in on, so a second pass reads
//! back the same assignment and reaches the same answer.
//!
//! **Cost.** Three states is two more than a formatter should need, and the
//! difference between the second and the third is invisible until you count
//! lines: `f(a, b,)` explodes and the same list broken in two does not. A
//! grouped line is also printed flat however wide it is, so the author can
//! push a line past the width limit by grouping — which is the point, and is
//! still a thing the width no longer governs. A group with a comment inside it
//! falls back to one item per line, because a comment belongs to an item and a
//! packed line may hold three of them.
//!
//! A list the author broke by hand *without* a trailing comma is still joined,
//! and for those there is `# fmt: skip`.
//!
//! ## Three method calls are a pipeline and go down the page
//!
//! **Decision.** A chain of `.name(…)` links ending the logical line is broken
//! before every link, with the receiver on the first line and each link at
//! `+4`. Three or more links break **whatever the width**; two links break only
//! when the line does not fit. A `.field` that is not a call is never a link,
//! and a line holding two separate chains is never broken as a chain at all.
//!
//! **Reason.** §4.6 and `Lexer::eat_line_continuation` make a leading `.`
//! continue the line above, and that is the only continuation available outside
//! brackets. `AGENTS.md` §4 writes a four-link chain down the page as the house
//! style, and `examples/00`, `14` and `16` each break one by hand although all
//! three fit on a line — so "three links go down the page" is a rule the
//! project already had and had not written down. Two links is
//! `self.summarize().truncate(80)`, which the corpus writes flat.
//!
//! The two exclusions were found by running the formatter over the corpus, not
//! by thinking about it.
//!
//! * `if left.summarize().length() >= right.summarize().length():` has four
//!   top-level links belonging to **two** receivers. Breaking before all four
//!   gives `.length() >= right`, which parses — the tree test passed — and is
//!   gibberish. A range with more than one chain in it has no chain to break.
//! * A chain in the middle of an expression strands everything after it on the
//!   last link's line, so the chain must end the logical line. A block header's
//!   trailing `:` does not count as coming after it.
//!
//! **Cost.** `a.b().c().d()` is thirteen characters and becomes four lines.
//! There is no magic trailing comma for chains to say "leave this one alone",
//! and adding one would mean inventing a token the language does not have.
//!
//! ## A long signature breaks before `where`
//!
//! **Decision.** A line that does not fit and has a top-level `where` is
//! broken before it, with the clause at `+8` — two block levels, not one.
//!
//! **Reason.** §4.4 puts `where` in `eat_line_continuation` precisely so a long
//! signature can be broken there. A signature reads better with its parameters
//! on one line and its bounds below than with its bounds inline and its
//! parameters in a column. The clause goes two levels in because one level is
//! where the function's *body* goes, and `where T: Clone:` at the body's
//! indent is a line a reader has to parse twice to find out which it is.
//! `examples/07_generics.science` indents its hand-broken signature
//! continuations by eight spaces, so the corpus had already made this choice.
//!
//! **Cost.** `+8` is a level that means nothing to the lexer and appears
//! nowhere else in the formatter's output, so it has to be learned. The
//! alternative — aligning the clause under the parameters — depends on the
//! width of what came before, and an indentation that depends on a prefix is
//! an indentation that moves when the prefix does.
//!
//! ## Blank lines: at most one, never at the top of a block
//!
//! **Decision.** A run of blank lines is collapsed to one. A blank line is
//! dropped where it would open a block, and at the start and end of the file.
//! Otherwise a blank line the author wrote is kept.
//!
//! **Reason.** Two blank lines and five blank lines never mean different
//! things, and a file whose vertical rhythm is preserved is a file whose diffs
//! are about code.
//!
//! **Cost.** `examples/16_indentation.science` ends with a function whose whole
//! purpose is to show that "blank lines inside a block do not close it, however
//! many there are". The formatter reduces its three blank lines to one and the
//! demonstration goes with them.
//!
//! ## Spacing: a space between every pair of tokens except a listed few
//!
//! **Decision.** `spacing.rs` states the rule as its negation — a space goes
//! everywhere except between the pairs in the table — and the table glues only
//! brackets, separators, `.`, and a unary `-`. Two context rules sit on top of
//! it: `pkg-config` keeps its hyphen, and the `(` of a `use` selection keeps
//! its space.
//!
//! **Reason.** Written the other way round, as a table of pairs that *get* a
//! space, a missing entry would glue two words into one and `a be b` would
//! become `abeb`. Written this way a missing entry costs a space that should
//! not be there, which is ugly and not wrong.
//!
//! `pkg-config` is the exception that matters and it is not cosmetic.
//! `Parser::expect_pkg_config` accepts `pkg`, `-`, `config` **only when their
//! spans touch** — three tokens and one word — so the space the table would
//! otherwise insert turns every `extern` block with a `via` clause into a
//! syntax error, with the token stream unchanged. It is the one place outside
//! the indentation where the gap between two tokens changes what a program
//! means, and it is why [`format_source`] compares trees and not just tokens.
//!
//! **Cost.** Two rules that are about one library each. The hyphen table has
//! one entry and will grow by one every time the grammar spells a word with a
//! character the lexer reads as an operator.
//!
//! ## Line endings and the last line
//!
//! **Decision.** Output uses LF line endings, always, and ends with exactly
//! one of them.
//!
//! **Reason.** Every span in the compiler is a byte offset, every `.stderr`
//! expectation in `tests/ui/` is compared byte for byte, and a file whose line
//! endings depend on which machine checked it out is a file whose diagnostics
//! do too.
//!
//! **Cost.** Formatting a CRLF file rewrites every line of it. There is no
//! option to keep CRLF and there should not be one, but the first
//! `fmt --write` on a Windows checkout with `core.autocrlf` set will produce a
//! diff nobody asked for.
//!
//! ## Comments — the part every formatter gets wrong
//!
//! A comment is not in the syntax tree. `Lexer::skip_comment` consumes it and
//! emits nothing, so a formatter written against the tree, or even against the
//! token stream, deletes every comment in the file and the tests still pass.
//! **Dropping a comment is the one unacceptable failure**, so comments are
//! recovered from the source text in `scan.rs` and carried through the whole
//! pipeline as first-class positions.
//!
//! **Decision.** A comment keeps exactly one property: whether it sat at the
//! end of a line of code or had a line to itself.
//!
//! * A comment at the end of a logical line is re-emitted after the last thing
//!   on that line, separated by two spaces. Its column is not preserved, so a
//!   column of hand-aligned trailing comments is un-aligned.
//! * A comment at the end of an *item* inside an exploded list is re-emitted
//!   after that item's comma, which can move it across a comma it was written
//!   before. It stays attached to the same item.
//! * A comment on a line of its own inside a list, or between the links of a
//!   chain, is re-emitted on a line of its own at the indent of the thing that
//!   follows it.
//! * A comment on a line of its own between two statements is re-emitted at
//!   the indent of whichever neighbour's block level its column is nearer to,
//!   ties going to the following statement. This is the rule that decides
//!   whether a comment at the end of a block belongs to the block or to what
//!   comes after it, and the author's column is the only evidence there is.
//!
//! **What the formatter refuses.** A comment in any other position has nowhere
//! to go: inside the head of a call (`connect( # here`, before the bracket),
//! after a closing bracket but before the end of the line, or in the middle of
//! an expression that has no list and no chain to hang it on. When that
//! happens the formatter **does not format that logical line at all**. It
//! reproduces the line exactly as written, shifting only its leading
//! indentation to the canonical column — which is safe, because every
//! continuation line of a logical line is inside a bracket or after a leading
//! `.`, where §4.2 says indentation means nothing. [`Formatting::preserved`]
//! counts the lines this happened to.
//!
//! **Reason.** The three options for a comment the layout cannot place are:
//! drop it, move it somewhere arbitrary, or stop. Dropping is out. Moving it
//! arbitrarily is worse than useless, because a comment's whole content is its
//! attachment. Stopping costs nothing but an unformatted line.
//!
//! **Cost.** A file can contain a line the formatter will never normalise, and
//! nothing in the output says which. The count is available on the return
//! value, and `sciencec fmt` does not print it — a formatter that nagged about
//! every unusual comment would be a formatter people stopped running.
//!
//! ## Saying no: `# fmt: off`, `# fmt: on`, `# fmt: skip`
//!
//! Every decision above is a rule about what code usually means, and code that
//! exists to demonstrate a *property of the language* is the case where the
//! usual meaning is the wrong one. Running this formatter over `examples/`
//! found four files in that position at once — a parenthesised expression
//! written one operand per line to show that it may be, a comment whose text
//! asserts the column it is written at, a run of blank lines that exists to
//! show a run of blank lines is legal, and an argument list grouped to mirror a
//! C prototype. The formatter is right about the language in all four and
//! destroys the demonstration in all four.
//!
//! **Decision.** Three comments — `# fmt: off`, `# fmt: on` and `# fmt: skip`
//! — turn the formatter off for a region or for one statement. [`suppress`]
//! holds the decisions, the reasons and the costs; the short version is that
//! they are comments, so no token and no grammar is invented, and a marker must
//! stand on a line of its own between statements.
//!
//! **What "leave it" means.** The same thing it already meant for a comment the
//! layout could not place: the source lines are reproduced as written, all
//! moved together by whatever it takes to put the first of them on its
//! canonical column. Byte-for-byte would be wrong — the canonical column is
//! what the lexer measures, so a region left at a column the rest of the file
//! no longer uses would change the block structure. Moving them together is
//! what keeps the shape the author wrote.
//!
//! **Cost.** "A formatted file is canonical" becomes "a formatted file is
//! canonical except where it says otherwise", which is weaker and is the price
//! of the mechanism existing at all. There is no `--force`: a flag that
//! overrode the markers would make them advisory, and an advisory escape hatch
//! is not one.
//!
//! ## What is never touched
//!
//! A token is re-emitted as the exact bytes it was written with, sliced out of
//! the source. `0xDeadBeef` keeps its case, `1_000.000_1` keeps its
//! separators, `"it\'s"` keeps a redundant escape. None of that is
//! recoverable from a lexed token, and a formatter that normalised it would be
//! making a language decision under the cover of a whitespace decision.
//!
//! # Refusing a file, and complaining about one
//!
//! [`format_source`] refuses — returns no text — in exactly two cases:
//!
//! * the source does not lex, in which case its diagnostics come back and the
//!   caller reports them. A file with an error is a file whose token stream is
//!   a guess, and reformatting a guess is how a formatter eats a program.
//! * the formatter's own output does not re-lex to the same token stream, or
//!   does not carry the same comments. That is a bug in this crate, and it is
//!   reported as one (`SC0900`) rather than written to the user's disk.
//!
//! It also has exactly one thing to say about a file it *did* format: an
//! unmatched `# fmt: off` is a warning (`SC0901`), because it suppresses
//! everything below it and nothing else in the output says so. A warning does
//! not fail the exit code, so a file with one still formats and `sciencec fmt`
//! still exits 0.

mod layout;
mod scan;
mod spacing;
mod suppress;

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use science_lexer::Token;

pub use layout::INDENT;
use layout::Layout;
use scan::{Comment, LogicalLine};

/// The formatter contradicted itself: the text it produced is not the program
/// it was given. Reported instead of emitted.
const E_FORMATTER_DISAGREES: Code = Code(900);

/// A `# fmt: off` with no `# fmt: on` after it. A warning, not an error: see
/// [`suppress`].
const W_UNMATCHED_FMT_OFF: Code = Code(901);

/// The width a formatted line is kept within. See the module documentation.
pub const MAX_WIDTH: usize = 90;

/// What [`format_source`] produced.
#[derive(Debug, Clone, Default)]
pub struct Formatting {
    /// The formatted source, or `None` when the formatter refused the file.
    pub text: Option<String>,
    /// Why it refused, or what it wants to say about a marker. Empty when
    /// there was nothing to report: formatting a clean file says nothing.
    pub diagnostics: Diagnostics,
    /// How many logical lines were reproduced as written because a comment in
    /// them had no position the layout could put it back in.
    pub preserved: usize,
    /// How many physical lines were reproduced as written because a
    /// `# fmt: off` or `# fmt: skip` asked for it.
    pub suppressed: usize,
}

/// The whole file, printed.
struct Printed {
    text: String,
    preserved: usize,
    suppressed: usize,
    /// The byte offset of every `# fmt: off` with no `# fmt: on` after it.
    unmatched: Vec<usize>,
}

/// Formats one Science source file.
///
/// The caller is expected to have checked the file first —
/// `sciencec fmt` reports lexical and syntax errors through the ordinary
/// pipeline and never reaches here — but the function is safe on its own: a
/// source that does not lex is refused, with the lexer's own diagnostics.
pub fn format_source(file: FileId, source: &str) -> Formatting {
    let (tokens, diagnostics) = science_lexer::lex(file, source);
    if diagnostics.has_errors() {
        return Formatting { text: None, diagnostics, preserved: 0, suppressed: 0 };
    }

    let comments = scan::comments(source, &tokens);
    let line_starts = scan::line_starts(source);
    let lines = scan::logical_lines(&tokens);
    let spacing = spacing::Spacing::of(&tokens);

    let printer = Printer {
        source,
        tokens: &tokens,
        line_starts: &line_starts,
        layout: Layout {
            source,
            tokens: &tokens,
            comments: &comments,
            line_starts: &line_starts,
            spacing: &spacing,
            width: MAX_WIDTH,
        },
    };
    let printed = printer.run(&lines, &comments);
    let Printed { text, preserved, suppressed, unmatched } = printed;

    let mut reported = Diagnostics::new();
    for at in unmatched {
        let end = source[at..].find('\n').map_or(source.len(), |n| at + n);
        reported.push(
            Diagnostic::warning(W_UNMATCHED_FMT_OFF, "`# fmt: off` has no matching `# fmt: on`")
                .with_label(Label::primary(
                    Span::new(file, at as u32, end as u32),
                    "formatting is suppressed from here to the end of the file",
                ))
                .with_note(
                    "the forgiving reading was chosen deliberately: ignoring the marker would \
                     reformat code that asked to be left alone, which is the one thing the \
                     marker exists to prevent. Write `# fmt: on` where the exception ends.",
                ),
        );
    }

    match verify(file, source, &tokens, &comments, &text) {
        Ok(()) => {
            Formatting { text: Some(text), diagnostics: reported, preserved, suppressed }
        }
        Err(message) => {
            reported.push(
                Diagnostic::error(E_FORMATTER_DISAGREES, "the formatter refused this file")
                    .with_label(Label::primary(
                        Span::new(file, 0, source.len().min(u32::MAX as usize) as u32),
                        message,
                    ))
                    .with_note(
                        "this is a bug in `science-fmt`: the text it produced is not the program \
                         it was given, so the file was left alone. Please report it with the \
                         source that caused it.",
                    ),
            );
            Formatting { text: None, diagnostics: reported, preserved, suppressed }
        }
    }
}

/// Re-lexes and re-parses the formatted text and checks it is the same
/// program.
///
/// Three properties, and they are the three the module documentation promises:
/// the token stream is unchanged, the syntax tree is unchanged modulo spans,
/// and every comment is still there in the same order with the same text.
///
/// The tree check looks redundant beside the token check, and is not. A token
/// carries its span, and the grammar reads one: `Parser::expect_pkg_config`
/// accepts `pkg`, `-`, `config` only when nothing is written between them, so a
/// space the formatter inserts can change the parse without changing a single
/// token. `spacing::HYPHENATED` is the fix; this is what found it, and what
/// will find the next one.
fn verify(
    file: FileId,
    source: &str,
    tokens: &[Token],
    comments: &[Comment],
    text: &str,
) -> Result<(), String> {
    let (after, diagnostics) = science_lexer::lex(file, text);
    if diagnostics.has_errors() {
        return Err("the formatted text does not lex".to_string());
    }
    if after.len() != tokens.len() {
        return Err(format!(
            "the formatted text has {} tokens where the source had {}",
            after.len(),
            tokens.len()
        ));
    }
    for (new, old) in after.iter().zip(tokens) {
        if new.kind != old.kind {
            return Err(format!("`{:?}` became `{:?}`", old.kind, new.kind));
        }
        // A `##` run is trivia carried on the token below it, not a token, so
        // moving one — or letting a blank line come between it and what it
        // documents — changes what the program says about itself without
        // changing a single `TokenKind`.
        //
        // The *text* is compared and not the whole `DocComment`. A run also
        // carries the span of the `##` lines it was written on, and moving
        // those lines is exactly what a formatter is for: re-indenting a run
        // shifts its offsets without changing one character of what it says,
        // so comparing the span here would fail every file the formatter
        // actually changed. The token's own span is skipped just above for
        // the same reason.
        let (new_doc, old_doc) = (new.doc.as_ref(), old.doc.as_ref());
        if new_doc.map(|d| &d.text) != old_doc.map(|d| &d.text) {
            return Err(format!(
                "the doc comment on `{:?}` changed from {:?} to {:?}",
                old.kind,
                old_doc.map(|d| &d.text),
                new_doc.map(|d| &d.text)
            ));
        }
    }

    let before = tree(file, tokens);
    let now = tree(file, &after);
    if before != now {
        let at = before
            .lines()
            .zip(now.lines())
            .position(|(a, b)| a != b)
            .map_or_else(|| "its length".to_string(), |n| format!("line {}", n + 1));
        return Err(format!("the syntax tree changed at {at}"));
    }

    let kept = scan::comments(text, &after);
    if kept.len() != comments.len() {
        return Err(format!(
            "the formatted text has {} comments where the source had {}",
            kept.len(),
            comments.len()
        ));
    }
    for (new, old) in kept.iter().zip(comments) {
        if text[new.start..new.end] != source[old.start..old.end] {
            return Err(format!("the comment `{}` was altered", &source[old.start..old.end]));
        }
    }
    Ok(())
}

/// The syntax tree of a token stream, as text, with every span removed.
///
/// Reformatting moves every byte in the file, so the spans are the one thing
/// that is *expected* to differ; everything else in `science-parser`'s dump is
/// the tree. Recovery is not skipped: a file that does not parse still has a
/// tree, the error nodes in it are part of the comparison, and a formatter that
/// turned a broken program into a working one would be caught here rather than
/// congratulated.
fn tree(file: FileId, tokens: &[Token]) -> String {
    use science_parser::Dump as _;
    let (module, diagnostics) = science_parser::parse_module(tokens, file);
    let mut out = String::new();
    for diagnostic in diagnostics.iter() {
        out.push_str(&format!("{} {}\n", diagnostic.code, diagnostic.message));
    }
    for line in module.dump().lines() {
        out.push_str(strip_span(line));
        out.push('\n');
    }
    out
}

/// Removes a trailing ` @start..end` or ` @f1:start..end`, and nothing else.
///
/// Written by hand rather than with a pattern so that it is conservative: a
/// line whose tail is not exactly a span keeps its tail. A stripper that was
/// too eager would quietly delete the difference it exists to find.
fn strip_span(line: &str) -> &str {
    let Some(at) = line.rfind(" @") else { return line };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    let mut tail = &line[at + 2..];
    if let Some(rest) = tail.strip_prefix('f') {
        match rest.split_once(':') {
            Some((file, rest)) if digits(file) => tail = rest,
            _ => return line,
        }
    }
    match tail.split_once("..") {
        Some((start, end)) if digits(start) && digits(end) => &line[..at],
        _ => line,
    }
}

/// One thing that occupies a line of its own in the output.
enum Unit {
    Line(LogicalLine),
    /// A comment that is not part of any logical line, with the block depth it
    /// was given.
    Comment { index: usize, depth: usize },
}

struct Printer<'a> {
    source: &'a str,
    tokens: &'a [Token],
    line_starts: &'a [usize],
    layout: Layout<'a>,
}

impl Printer<'_> {
    fn start_of(&self, token: usize) -> usize {
        self.tokens[token].span.start as usize
    }

    fn end_of(&self, token: usize) -> usize {
        self.tokens[token].span.end as usize
    }

    fn line_at(&self, pos: usize) -> usize {
        scan::line_of(self.line_starts, pos)
    }

    /// The whole text of physical line `n`, without its line break.
    fn physical_line(&self, n: usize) -> &str {
        let start = self.line_starts[n];
        let end = match self.line_starts.get(n + 1) {
            // `line_starts[n + 1]` always sits just past a `\n`.
            Some(&next) => next - 1,
            None => self.source.len(),
        };
        &self.source[start..end]
    }

    fn line_is_blank(&self, n: usize) -> bool {
        self.physical_line(n).trim().is_empty()
    }

    /// Formats the whole file.
    fn run(&self, lines: &[LogicalLine], comments: &[Comment]) -> Printed {
        let units = self.units(lines, comments);
        let statements: Vec<(usize, usize)> =
            lines.iter().map(|line| self.extent_of(line)).collect();
        let free: Vec<usize> = units
            .iter()
            .filter_map(|unit| match unit {
                Unit::Comment { index, .. } => Some(*index),
                Unit::Line(_) => None,
            })
            .collect();
        let marked =
            suppress::suppressed(self.source, comments, &free, self.line_starts, &statements);

        let mut out: Vec<String> = Vec::new();
        let mut preserved = 0usize;
        let mut suppressed = 0usize;
        let mut previous: Option<(usize, usize)> = None;

        let mut i = 0usize;
        while i < units.len() {
            let (first, last, depth) = self.extent(&units[i], comments);

            // A marker's own line is the first line of its region, so walking
            // the units in order always meets the marker before anything it
            // governs.
            if let Some(region) = marked.regions.iter().find(|r| r.holds(first)) {
                self.gap(&mut out, previous, first, depth);
                let mut delta = None;
                let mut trailing_depth = depth;
                let mut j = i;
                while j < units.len() {
                    let (begins, _, at) = self.extent(&units[j], comments);
                    if begins > region.last {
                        break;
                    }
                    if delta.is_none() {
                        if let Unit::Line(line) = &units[j] {
                            delta = Some(self.delta_for(line, begins));
                        }
                    }
                    trailing_depth = at;
                    j += 1;
                }
                out.extend(self.shifted(region.first, region.last, delta.unwrap_or(0)));
                suppressed += region.last + 1 - region.first;
                previous = Some((region.last, trailing_depth));
                i = j;
                continue;
            }

            self.gap(&mut out, previous, first, depth);
            match &units[i] {
                Unit::Comment { index, depth } => {
                    let comment = &comments[*index];
                    out.push(
                        " ".repeat(depth * INDENT) + &self.source[comment.start..comment.end],
                    );
                }
                Unit::Line(line) => {
                    match self.layout.lay_out(line.start, line.end, line.depth * INDENT) {
                        Some(mut laid) => {
                            if let Some(comment) = self.layout.trailing_comment(line.end) {
                                let tail =
                                    laid.last_mut().expect("a laid-out line is never empty");
                                tail.push_str("  ");
                                tail.push_str(&self.source[comment.start..comment.end]);
                            }
                            out.extend(laid);
                        }
                        None => {
                            preserved += 1;
                            out.extend(self.shifted(first, last, self.delta_for(line, first)));
                        }
                    }
                }
            }
            previous = Some((last, depth));
            i += 1;
        }

        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        let mut text = out.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        Printed { text, preserved, suppressed, unmatched: marked.unmatched }
    }

    /// The physical lines a unit occupies, and the block level it sits at.
    fn extent(&self, unit: &Unit, comments: &[Comment]) -> (usize, usize, usize) {
        match unit {
            Unit::Line(line) => {
                let (first, last) = self.extent_of(line);
                (first, last, line.depth)
            }
            Unit::Comment { index, depth } => {
                let at = self.line_at(comments[*index].start);
                (at, at, *depth)
            }
        }
    }

    /// The first and last physical line of one logical line.
    fn extent_of(&self, line: &LogicalLine) -> (usize, usize) {
        (
            self.line_at(self.start_of(line.start)),
            self.line_at(self.end_of(line.end - 1).saturating_sub(1)),
        )
    }

    /// The blank lines that go between what was printed last and what is about
    /// to be.
    fn gap(
        &self,
        out: &mut Vec<String>,
        previous: Option<(usize, usize)>,
        first: usize,
        depth: usize,
    ) {
        let Some((previous_last, previous_depth)) = previous else { return };
        let mut blanks =
            (previous_last + 1..first).filter(|&n| self.line_is_blank(n)).count().min(1);
        // A blank line between a block's header and its first statement is a
        // gap nobody meant to leave.
        if depth > previous_depth {
            blanks = 0;
        }
        for _ in 0..blanks {
            out.push(String::new());
        }
    }

    /// How far a logical line has to move to sit on its canonical column.
    fn delta_for(&self, line: &LogicalLine, first: usize) -> isize {
        let start = self.start_of(line.start);
        let column = self.source[self.line_starts[first]..start].chars().count();
        (line.depth * INDENT) as isize - column as isize
    }

    /// Physical lines `first..=last` as written, every one of them moved by
    /// the same amount.
    ///
    /// Moving them together is what keeps the shape the author chose: the
    /// first line lands on the canonical column — which it must, because that
    /// column is what the lexer measures — and everything below it keeps its
    /// position relative to that. Inside a logical line the rest are within a
    /// bracket or after a leading `.`, where §4.2 gives indentation no meaning
    /// at all; inside a suppressed region they are whatever the author wrote,
    /// which already lexed.
    fn shifted(&self, first: usize, last: usize, delta: isize) -> Vec<String> {
        (first..=last)
            .map(|n| {
                let text = self.physical_line(n).trim_end();
                if text.is_empty() {
                    return String::new();
                }
                match delta {
                    0 => text.to_string(),
                    d if d > 0 => " ".repeat(d as usize) + text,
                    d => {
                        let drop = d.unsigned_abs();
                        let keep = text.len() - text.trim_start_matches(' ').len();
                        text[drop.min(keep)..].to_string()
                    }
                }
            })
            .collect()
    }

    /// Orders the logical lines and the free-standing comments into one list,
    /// and decides what block level each of those comments belongs to.
    fn units(&self, lines: &[LogicalLine], comments: &[Comment]) -> Vec<Unit> {
        let mut owned = vec![false; comments.len()];
        for line in lines {
            let from = self.start_of(line.start);
            let end = self.end_of(line.end - 1);
            // Everything up to the end of the physical line the last token is
            // on: that covers the comments written between the continuation
            // lines and the one written after the line itself.
            let limit = match self.line_starts.get(self.line_at(end.saturating_sub(1)) + 1) {
                Some(&next) => next,
                None => self.source.len(),
            };
            for (i, comment) in comments.iter().enumerate() {
                if comment.start >= from && comment.start < limit {
                    owned[i] = true;
                }
            }
        }

        let mut units = Vec::new();
        let mut next_line = 0usize;
        for (i, comment) in comments.iter().enumerate() {
            if owned[i] {
                continue;
            }
            while next_line < lines.len() && self.start_of(lines[next_line].start) < comment.start {
                units.push(Unit::Line(lines[next_line]));
                next_line += 1;
            }
            let before = next_line.checked_sub(1).map_or(0, |n| lines[n].depth);
            let after = lines.get(next_line).map_or(0, |l| l.depth);
            units.push(Unit::Comment { index: i, depth: nearer(comment.column, before, after) });
        }
        for line in &lines[next_line..] {
            units.push(Unit::Line(*line));
        }
        units
    }
}

/// Which of two block levels a comment written at `column` belongs to.
///
/// The comparison is against the levels' *canonical* columns rather than the
/// columns the neighbours happen to be written at, so that formatting a second
/// time reaches the same answer: after one pass the comment is at
/// `depth * INDENT` exactly, which is distance zero from the level it was
/// given and non-zero from the other.
fn nearer(column: usize, before: usize, after: usize) -> usize {
    let distance = |depth: usize| (depth * INDENT).abs_diff(column);
    if distance(before) < distance(after) {
        before
    } else {
        after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(source: &str) -> String {
        let out = format_source(FileId(0), source);
        assert!(out.diagnostics.is_empty(), "unexpected diagnostics");
        out.text.expect("the source formats")
    }

    #[test]
    fn a_comment_is_never_lost() {
        let source = "def f():\n    let a be 1   # why\n";
        assert!(format(source).contains("# why"));
    }

    #[test]
    fn a_block_keeps_its_depth() {
        let source = "def f():\n  if a:\n        b\n  c\n";
        assert_eq!(format(source), "def f():\n    if a:\n        b\n    c\n");
    }

    #[test]
    fn a_run_of_blank_lines_collapses_to_one() {
        let source = "def f():\n    let a be 1\n\n\n\n    let b be 2\n";
        assert_eq!(format(source), "def f():\n    let a be 1\n\n    let b be 2\n");
    }

    #[test]
    fn a_blank_line_at_the_top_of_a_block_goes() {
        let source = "def f():\n\n    let a be 1\n";
        assert_eq!(format(source), "def f():\n    let a be 1\n");
    }

    #[test]
    fn a_trailing_comma_keeps_a_list_exploded() {
        let source = "def f():\n    g(1, 2,)\n";
        assert_eq!(format(source), "def f():\n    g(\n        1,\n        2,\n    )\n");
    }

    #[test]
    fn a_list_with_no_trailing_comma_is_joined() {
        let source = "def f():\n    g(\n        1,\n        2\n    )\n";
        assert_eq!(format(source), "def f():\n    g(1, 2)\n");
    }

    #[test]
    fn a_short_chain_is_joined_and_a_long_one_is_broken() {
        let short = "def f():\n    a\n        .b()\n        .c()\n";
        assert_eq!(format(short), "def f():\n    a.b().c()\n");

        let name = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
        let long = format!("def f():\n    a.{name}().{name}().{name}()\n");
        assert_eq!(
            format(&long),
            format!(
                "def f():\n    a\n        .{name}()\n        .{name}()\n        .{name}()\n"
            )
        );
    }

    #[test]
    fn a_trailing_comma_over_several_lines_keeps_the_authors_grouping() {
        // The third state: the author wrote three items on two lines, and the
        // formatter keeps that rather than making it three lines.
        let source = "def f():\n    g(\n        a, b,\n        c,\n    )\n";
        assert_eq!(format(source), source);

        // The second state is unchanged: all on one line means "explode".
        let flat = "def f():\n    g(a, b, c,)\n";
        assert_eq!(format(flat), "def f():\n    g(\n        a,\n        b,\n        c,\n    )\n");
    }

    #[test]
    fn a_grouped_list_with_a_comment_in_it_falls_back_to_one_per_line() {
        let source = "def f():\n    g(\n        a, b,  # why\n        c,\n    )\n";
        assert_eq!(
            format(source),
            "def f():\n    g(\n        a,\n        b,  # why\n        c,\n    )\n"
        );
    }

    #[test]
    fn fmt_off_and_fmt_on_hold_a_region_as_written() {
        let source = "def f():\n    # fmt: off\n    let a be (\n        1 +\n        2\n    )\n    # fmt: on\n    let b be (\n        1 +\n        2\n    )\n";
        let out = format_source(FileId(0), source);
        let text = out.text.expect("the file formats");
        assert!(text.contains("let a be (\n        1 +\n        2\n    )"), "{text}");
        assert!(text.contains("let b be (1 + 2)"), "{text}");
        assert_eq!(out.suppressed, 6, "the two markers and the four lines between them");
        assert!(out.diagnostics.is_empty());
        assert_eq!(format(&text), text, "a suppressed region is still a fixed point");
    }

    #[test]
    fn fmt_skip_holds_the_next_statement() {
        let source =
            "def f():\n    # fmt: skip\n    let a be (\n        1 +\n        2\n    )\n    let b be (\n        1 +\n        2\n    )\n";
        let text = format(source);
        assert!(text.contains("let a be (\n        1 +\n        2\n    )"), "{text}");
        assert!(text.contains("let b be (1 + 2)"), "{text}");
    }

    #[test]
    fn a_region_holds_blank_lines_and_comment_columns() {
        let source = "# fmt: off\ndef f():\n    let a be 1\n\n\n\n        # far right\n    let b be 2\n# fmt: on\n";
        assert_eq!(format(source), source);
    }

    #[test]
    fn an_unmatched_fmt_off_runs_to_the_end_of_the_file_and_is_warned_about() {
        let source = "def f():\n    # fmt: off\n    let a be (\n        1 +\n        2\n    )\n";
        let out = format_source(FileId(0), source);
        assert_eq!(out.text.as_deref(), Some(source), "nothing below it is touched");
        assert_eq!(out.diagnostics.len(), 1);
        assert!(!out.diagnostics.has_errors(), "it is a warning, not an error");
    }

    /// A marker is a comment, so the verifier already guards it: `verify`
    /// compares every comment's text before and after, and would refuse the
    /// file rather than emit one with a marker missing or altered.
    #[test]
    fn a_marker_is_a_comment_and_the_verifier_counts_it() {
        let source = "# fmt: off\ndef f():\n    let  a  be  1\n# fmt: on\ndef g():\n    let  b  be  2\n";
        let text = format(source);
        assert_eq!(scan::comments(&text, &science_lexer::lex(FileId(0), &text).0).len(), 2);
        assert!(text.contains("# fmt: off") && text.contains("# fmt: on"), "{text}");
        // The region held; the function below it did not.
        assert!(text.contains("let  a  be  1"), "{text}");
        assert!(text.contains("let b be 2"), "{text}");
    }

    #[test]
    fn a_marker_that_is_not_on_a_line_of_its_own_is_an_ordinary_comment() {
        // Black's spelling. It does nothing here, and the module documentation
        // says why: a Science logical line can span a dozen physical lines, so
        // a trailing comment on one is four lines below what it would govern.
        let source = "def f():\n    let a be (\n        1 +\n        2\n    )  # fmt: skip\n";
        let text = format(source);
        assert!(text.contains("let a be (1 + 2)  # fmt: skip"), "{text}");
    }

    #[test]
    fn a_marker_inside_a_bracket_is_an_ordinary_comment() {
        let source = "def f():\n    g(\n        # fmt: off\n        a,\n    )\n";
        let text = format(source);
        assert!(text.contains("# fmt: off"), "{text}");
        assert_eq!(format(&text), text);
    }

    #[test]
    fn a_comment_the_layout_cannot_place_leaves_the_line_alone() {
        let source = "def f():\n    let a be (1 +   # awkward\n        2)\n";
        let out = format_source(FileId(0), source);
        assert_eq!(out.preserved, 1);
        let text = out.text.expect("the file still formats");
        assert!(text.contains("# awkward"), "{text}");
        assert!(text.contains("let a be (1 +"), "{text}");
    }

    #[test]
    fn line_endings_are_normalised_and_the_file_ends_in_one_newline() {
        let wanted = "def f():\n    print(1)\n";
        assert_eq!(format("def f():\r\n    print(1)\r\n"), wanted);
        assert_eq!(format("def f():\n    print(1)"), wanted);
        assert_eq!(format("def f():\n    print(1)\n\n\n"), wanted);
    }

    #[test]
    fn two_chains_on_one_line_are_not_a_chain() {
        // Four top-level links belonging to two receivers. Breaking before all
        // four would produce `.length() >= right`, which parses and is
        // gibberish, so the line is left long instead.
        let wide = "w".repeat(60);
        let source =
            format!("def f():\n    if left.summarize().length() >= right.{wide}().length():\n        1\n");
        let out = format(&source);
        assert_eq!(out.lines().count(), 3, "{out}");
    }

    #[test]
    fn a_file_that_does_not_lex_is_refused() {
        let out = format_source(FileId(0), "def f():\n\tlet a be 1\n");
        assert!(out.text.is_none());
        assert!(out.diagnostics.has_errors());
    }

    #[test]
    fn an_empty_file_formats_to_nothing() {
        assert_eq!(format(""), "");
        assert_eq!(format("\n\n\n"), "");
    }

    #[test]
    fn a_comment_at_the_end_of_a_block_stays_in_the_block() {
        let source = "def f():\n    if a:\n        b\n        # inside\n    c\n";
        assert!(format(source).contains("\n        # inside\n"));
        // Written further right than any open block, it still belongs to the
        // block it was written in rather than opening one of its own.
        let deeper = "def f():\n    if a:\n        b\n            # stray\n    c\n";
        assert!(format(deeper).contains("\n        # stray\n"));
    }

    #[test]
    fn a_comment_after_a_block_leaves_it() {
        let source = "def f():\n    let a be 1\n# outside\n\nfunction g():\n    2\n";
        assert!(format(source).contains("\n# outside\n"));
    }

    #[test]
    fn nearer_prefers_the_following_statement_on_a_tie() {
        assert_eq!(nearer(4, 2, 0), 0);
        assert_eq!(nearer(8, 2, 0), 2);
        assert_eq!(nearer(0, 2, 0), 0);
    }
}
