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
//! ## Maximum line width: 100 columns
//!
//! **Decision.** A line is broken when it would otherwise exceed 100
//! characters. A trailing comment does not count toward the width.
//!
//! **Reason.** The corpus's prose wraps at about 76 columns, but its *code*
//! reaches 97 (`examples/08_dyn_dispatch.science`) and 93
//! (`examples/19_stdlib.science`), and those lines are deliberate. Any limit
//! at or below 97 would make the formatter's first act be to break lines their
//! author chose not to break, which is the worst possible introduction. 100 is
//! above everything checked in, so the formatter changes line breaking only
//! where the author broke a line that did not need breaking.
//!
//! **Cost.** 100 is wider than the prose beside it, so a formatted file has a
//! ragged right edge: comments stop at 76 and code may run to 100. And a limit
//! nothing in the corpus reaches is a limit the corpus does not test; the
//! breaking rules are exercised by this crate's own fixtures instead, which is
//! weaker evidence.
//!
//! There is also no width that agrees with the corpus, and that is worth
//! recording rather than hiding. `examples/08_dyn_dispatch.science` leaves a
//! line at 97 columns unbroken; `examples/00_kitchen_sink.science` breaks
//! `def best_of …` before its `where` at 95. No single number honours
//! both, so whichever is chosen the formatter contradicts one of them. 100
//! contradicts the one where the disagreement is *joining* a line, which is
//! recoverable by narrowing the limit later; the other direction would mean
//! shipping a formatter that immediately rewrites code somebody wrote on
//! purpose.
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
//! ## A trailing comma is an instruction
//!
//! **Decision.** A bracket group whose last item is followed by a comma is
//! exploded however short it is, and a group with no trailing comma is joined
//! onto one line whenever it fits. The formatter never adds or removes a
//! comma.
//!
//! **Reason.** The formatter may not change the token stream, and `(a, b)` and
//! `(a, b,)` are different token streams. Deciding whether the difference
//! matters is the parser's job — F0 has tuples, and a trailing comma in a
//! parenthesised expression is exactly the kind of thing that means one thing
//! in one position and another elsewhere. Since the comma has to be preserved
//! anyway, it may as well be given a meaning, and Black and Prettier have both
//! shown that "the author left a trailing comma, so keep this exploded" is a
//! rule people like. `examples/14_line_continuation.science` puts a trailing
//! comma on every list it breaks by hand, so the corpus's careful layout
//! survives contact with the formatter unchanged.
//!
//! **Cost.** A list the author broke by hand *without* a trailing comma gets
//! joined, and `examples/14`'s point about those is lost. And a short list the
//! author left a comma in stays four lines long when one would do, with no way
//! to say otherwise except deleting the comma.
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
//! ## What is never touched
//!
//! A token is re-emitted as the exact bytes it was written with, sliced out of
//! the source. `0xDeadBeef` keeps its case, `1_000.000_1` keeps its
//! separators, `"it\'s"` keeps a redundant escape. None of that is
//! recoverable from a lexed token, and a formatter that normalised it would be
//! making a language decision under the cover of a whitespace decision.
//!
//! # Refusing a file
//!
//! [`format_source`] refuses — returns no text — in exactly two cases:
//!
//! * the source does not lex, in which case its diagnostics come back and the
//!   caller reports them. A file with an error is a file whose token stream is
//!   a guess, and reformatting a guess is how a formatter eats a program.
//! * the formatter's own output does not re-lex to the same token stream, or
//!   does not carry the same comments. That is a bug in this crate, and it is
//!   reported as one (`SC0900`) rather than written to the user's disk.

mod layout;
mod scan;
mod spacing;

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use science_lexer::Token;

pub use layout::INDENT;
use layout::Layout;
use scan::{Comment, LogicalLine};

/// The formatter contradicted itself: the text it produced is not the program
/// it was given. Reported instead of emitted.
const E_FORMATTER_DISAGREES: Code = Code(900);

/// The width a formatted line is kept within. See the module documentation.
pub const MAX_WIDTH: usize = 100;

/// What [`format_source`] produced.
#[derive(Debug, Clone, Default)]
pub struct Formatting {
    /// The formatted source, or `None` when the formatter refused the file.
    pub text: Option<String>,
    /// Why it refused. Empty on success: formatting a clean file says nothing.
    pub diagnostics: Diagnostics,
    /// How many logical lines were reproduced as written because a comment in
    /// them had no position the layout could put it back in.
    pub preserved: usize,
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
        return Formatting { text: None, diagnostics, preserved: 0 };
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
    let (text, preserved) = printer.run(&lines, &comments);

    match verify(file, source, &tokens, &comments, &text) {
        Ok(()) => Formatting { text: Some(text), diagnostics: Diagnostics::new(), preserved },
        Err(message) => {
            let mut refusal = Diagnostics::new();
            refusal.push(
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
            Formatting { text: None, diagnostics: refusal, preserved }
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
        if new.doc != old.doc {
            return Err(format!(
                "the doc comment on `{:?}` changed from {:?} to {:?}",
                old.kind, old.doc, new.doc
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
    fn run(&self, lines: &[LogicalLine], comments: &[Comment]) -> (String, usize) {
        let units = self.units(lines, comments);
        let mut out: Vec<String> = Vec::new();
        let mut preserved = 0usize;
        let mut previous: Option<(usize, usize)> = None;

        for unit in &units {
            let (first, last, depth) = match unit {
                Unit::Line(line) => (
                    self.line_at(self.start_of(line.start)),
                    self.line_at(self.end_of(line.end - 1).saturating_sub(1)),
                    line.depth,
                ),
                Unit::Comment { index, depth } => {
                    let at = self.line_at(comments[*index].start);
                    (at, at, *depth)
                }
            };

            if let Some((previous_last, previous_depth)) = previous {
                let mut blanks = (previous_last + 1..first)
                    .filter(|&n| self.line_is_blank(n))
                    .count()
                    .min(1);
                // A blank line between a block's header and its first
                // statement is a gap nobody meant to leave.
                if depth > previous_depth {
                    blanks = 0;
                }
                for _ in 0..blanks {
                    out.push(String::new());
                }
            }

            match unit {
                Unit::Comment { index, depth } => {
                    let comment = &comments[*index];
                    out.push(
                        " ".repeat(depth * INDENT) + &self.source[comment.start..comment.end],
                    );
                }
                Unit::Line(line) => match self.layout.lay_out(
                    line.start,
                    line.end,
                    line.depth * INDENT,
                ) {
                    Some(mut laid) => {
                        if let Some(comment) = self.layout.trailing_comment(line.end) {
                            let last = laid.last_mut().expect("a laid-out line is never empty");
                            last.push_str("  ");
                            last.push_str(&self.source[comment.start..comment.end]);
                        }
                        out.extend(laid);
                    }
                    None => {
                        preserved += 1;
                        out.extend(self.verbatim(line, first, last));
                    }
                },
            }
            previous = Some((last, depth));
        }

        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        let mut text = out.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        (text, preserved)
    }

    /// A logical line the layout would not touch, reproduced as written.
    ///
    /// Only the indentation moves, and every physical line moves by the same
    /// amount, so the relative shape the author chose is intact. The first
    /// line lands on the canonical column — which it must, because that column
    /// is what the lexer measures — and the rest are inside a bracket or after
    /// a leading `.`, where §4.2 gives the indentation no meaning at all.
    fn verbatim(&self, line: &LogicalLine, first: usize, last: usize) -> Vec<String> {
        let start = self.start_of(line.start);
        let column = self.source[self.line_starts[first]..start].chars().count();
        let wanted = line.depth * INDENT;

        (first..=last)
            .map(|n| {
                let text = self.physical_line(n).trim_end();
                if text.is_empty() {
                    return String::new();
                }
                if wanted >= column {
                    " ".repeat(wanted - column) + text
                } else {
                    let drop = column - wanted;
                    let keep = text.len() - text.trim_start_matches(' ').len();
                    text[drop.min(keep)..].to_string()
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
