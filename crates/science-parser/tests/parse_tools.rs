//! Snapshot tests for the `tool` declaration and its nine diagnostics.
//!
//! `mcp-servers.md` §14.2 stage 0 is exactly this file's subject: `tool` is a
//! keyword, `parse_item` has an arm for it, and `SC0190`–`SC0198` all fire
//! here with no type checker anywhere. The type-level half of the note —
//! `SC0504`–`SC0518`, which is where "the schema is the signature" is actually
//! enforced — is stage 2 and is nobody's yet.
//!
//! These go through the real lexer rather than a hand-built stream, because
//! half of what is being tested is where a `##` run lands, and a doc comment
//! written as a `Token::doc` by hand would be testing the test.
//!
//! **Every case here is one mistake and one diagnostic.** That is the property
//! the file exists to hold: `SC0156` cascaded for months because its reporter
//! advanced over a word the parser then advanced over again, and a snapshot
//! showing two diagnostics for one stale keyword is what caught it. A second
//! diagnostic appearing in any snapshot below is a regression, whatever it
//! says.

mod common;
use common::{parse_source, parse_source_allowing_errors};

// --- the declaration itself ----------------------------------------------

/// The shape Decision 1 spends the keyword on: a name, a typed parameter list,
/// a description above it, and a result.
#[test]
fn a_tool_is_a_declaration() {
    insta::assert_snapshot!(parse_source(
        "## Fit peaks in a time-of-flight window.
tool fit_peaks(run: String, window_lower: F64, max_peaks: U8) -> Int:
    0
"
    ));
}

/// Decision 6: the run is kept whole on the item, summary included. Splitting
/// it into a `title` and a `description` is the emitter's job and is not done
/// here — `strings-formatting-and-docs.md` §5.4 already owns that rule and the
/// parser does not get a second copy of it.
#[test]
fn a_tools_description_reaches_the_item_whole() {
    insta::assert_snapshot!(parse_source(
        "## Fit peaks in a time-of-flight window.
##
## Uses the run's own dark-current subtraction. Fails if the window contains
## fewer than 32 channels, because the fit is then underdetermined.
tool fit_peaks(run: String) -> Int:
    0
"
    ));
}

/// Decision 7: a `##` run may precede a parameter of a `tool`, and becomes
/// that property's description.
///
/// §2.5 says outright that this is the one thing Option B — a record type plus
/// a derive — would have had for free, and that Decision 1 should be reopened
/// without it. The dump flags the run rather than printing it; what it
/// contains is the schema emitter's business.
#[test]
fn a_tools_parameters_may_be_documented_one_by_one() {
    insta::assert_snapshot!(parse_source(
        "## Fit peaks in a time-of-flight window.
tool fit_peaks(
        ## The run identifier, as printed in the run log.
        run: String,
        ## Lower edge of the fit window, in seconds from the pulse.
        window_lower: F64) -> Int:
    0
"
    ));
}

/// A `tool` and a `def` in one file, told apart in the tree.
///
/// This is the predicate `sciencec tools --json` reads. If the two dumped the
/// same, the walk of §14.3 would have nothing to select on.
#[test]
fn a_tool_and_a_def_are_different_declarations() {
    insta::assert_snapshot!(parse_source(
        "def helper(x: Int) -> Int:
    x

## Expose the helper.
tool exposed(x: Int) -> Int:
    x
"
    ));
}

/// `tool` is a keyword everywhere, not a contextual word. A binding named
/// `tool` is `SC0102`, the same refusal `await` gets, and the parser says
/// which kind of word it is rather than leaving it to a later phase.
#[test]
fn tool_is_not_an_identifier_any_more() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def f() -> Int:
    let tool be 1
"
    ));
}

// --- SC0190 and SC0195: the description ----------------------------------

/// Decision 5, the only construct in Science for which documentation is
/// mandatory. The justification is narrow on purpose: the string is an
/// argument, not prose, because the caller reads it *in order to* decide.
#[test]
fn a_tool_without_a_description_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "tool fit_peaks(run: String) -> Int:
    0
"
    ));
}

/// §5.2 makes the first line the `title`. A run that opens blank has no first
/// line to be one, and the rest of the declaration is fine — so this is
/// `SC0195` and not `SC0190`.
#[test]
fn a_tool_whose_summary_line_is_blank_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "##
## Fit peaks in a time-of-flight window.
tool fit_peaks(run: String) -> Int:
    0
"
    ));
}

/// An ordinary `def` is untouched by Decision 5. Requiring a description of
/// every function would be "document your code", which is a style rule a
/// compiler has no business enforcing.
#[test]
fn a_def_without_a_description_is_fine() {
    insta::assert_snapshot!(parse_source(
        "def helper(x: Int) -> Int:
    x
"
    ));
}

// --- SC0191 to SC0198 ----------------------------------------------------

/// §2.4 item 2. The list a caller is given is flat and concrete, with one
/// schema per entry, so there is nothing for `T` to be instantiated at.
///
/// The parameter is **kept** after the report. Dropping it would leave every
/// mention of `T` unresolved, and one stale word would cost a diagnostic here
/// plus a name-resolution failure per use — the cascade, arriving late.
#[test]
fn a_generic_tool_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "## Return the largest of the values.
tool largest of T(values: Array[T]) -> Int:
    0
"
    ));
}

/// §2.4 item 3. There is no caller to borrow from: the arguments were built
/// from what arrived a moment ago and the tool is the only owner there is.
///
/// Both spellings, and two parameters, so the span of `mutable borrowed` is
/// pinned as well as `borrowed`'s. Two mistakes, two diagnostics — which is
/// not a cascade, and the count is the check.
#[test]
fn a_borrowed_tool_parameter_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "## Count the rows of a table.
tool count_rows(rows: &Array[Int], scratch: &mut Int) -> Int:
    0
"
    ));
}

/// A tool is called by name alone and there is nothing for `self` to be bound
/// to. The receiver is dropped from the tree as well as reported, so no later
/// phase has to decide what a module-level function with a receiver is.
#[test]
fn a_tool_with_a_receiver_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "## Summarize the document.
tool summarize(self) -> Int:
    0
"
    ));
}

/// Decision 7 is scoped to `tool` deliberately, so that the general question
/// of documenting a `def`'s parameters stays open and belongs to
/// `strings-formatting-and-docs.md` §5.2.
#[test]
fn a_documented_parameter_outside_a_tool_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "def fit_peaks(
        ## The run identifier, as printed in the run log.
        run: String) -> Int:
    0
"
    ));
}

/// Decision 2 spends `tool` and only `tool`. Without `SC0196` these fall
/// through to `SC0101`, which describes them as reserved "for a later phase" —
/// a phrase that reads, to anyone who knows what a compiler phase is, as
/// though a later pass will accept them.
#[test]
fn prompt_and_agent_in_declaration_position_are_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "prompt fit_report(run: String) -> Int:
    0

agent analyst(question: String) -> Int:
    0
"
    ));
}

/// A word that is reserved and *not* in declaration position is untouched:
/// the lookahead is what makes this a declaration diagnostic rather than a
/// second opinion about every use of the word.
#[test]
fn a_reserved_word_that_declares_nothing_is_left_alone() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "prompt
"
    ));
}

/// §16.1 lists four places a `tool` may not be: an `interface`, a `has` block,
/// an `implements` block and a function body. Two of them are here, which is
/// the two code paths — `parse_member` and `parse_stmt`.
#[test]
fn a_tool_outside_module_level_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "type Doc:
    title: String

Doc has:
    ## Summarize the document.
    tool summarize(self) -> Int:
        0

def run() -> Int:
    ## Fit peaks in a window.
    tool fit(window: F64) -> Int:
        0
    0
"
    ));
}

/// There is no abstract tool. A signature with no body is an interface's
/// required method, and that is declared with `def`.
#[test]
fn a_tool_without_a_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "## Fit peaks in a time-of-flight window.
tool fit_peaks(window_lower: F64) -> Int
"
    ));
}

/// A `tool` whose body is indented with no `:` in front of it reports the
/// missing colon and **not** `SC0198` as well. Both would be true and only one
/// is useful: the author wrote a body, and telling them it is absent is the
/// cascade this file exists to keep out.
#[test]
fn a_tool_missing_its_colon_is_one_diagnostic_and_not_two() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        "## Fit peaks in a time-of-flight window.
tool fit_peaks(window_lower: F64) -> Int
    0
"
    ));
}
