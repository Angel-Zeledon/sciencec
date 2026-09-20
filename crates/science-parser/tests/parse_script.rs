//! The top level that admits statements — `script-mode.md` §1.1 — and the
//! desugaring that keeps it from being a second grammar.
//!
//! Everything here goes through the real lexer, because the whole question
//! these tests are about is what a *line* is: §8.2's rule scans a logical line
//! for two reserved words, and a hand-built stream would be a test of the
//! stream rather than of the rule.
//!
//! What each snapshot shows is the whole module, so the generated `def main()
//! -> Error?` is visible in every one of them. That is deliberate. The
//! desugaring's advertised cost is that later phases are handed a function
//! nobody wrote, and a test suite that hid it would be hiding the cost.

mod common;
use common::{parse_source, parse_source_allowing_errors};

// --- the first program ---------------------------------------------------

/// The program the whole note exists for, and the one that did not parse.
///
/// Before this, `sciencec check` on a file containing exactly this line
/// answered ``error[SC0101]: expected `implements` or `has` after `print`,
/// found `(` `` — a syntax error at the first statement of the first program
/// the language would ever run.
#[test]
fn hello_world() {
    insta::assert_snapshot!(parse_source("print(\"hello, world\")\n"));
}

/// Several statements, in source order, and the order is the whole property:
/// there is no hoisting, no re-ordering and no dependency analysis (§3.1).
#[test]
fn statements_keep_their_order() {
    insta::assert_snapshot!(parse_source(
        r#"let text be read("run.csv")
let count be text.lines().len()
print(count)
"#
    ));
}

/// **The body's tail is always the generated `null`, never the last
/// statement.**
///
/// `finish_block` promotes a trailing expression statement to a block's tail,
/// and for the script body that would be wrong twice over: the value of
/// `print(..)` is `()` and the signature says `Error?`, and §2.3's "falling
/// off the end is an implicit `return null`" would have no node to stand for
/// it. So the script body is built by hand and the promotion is skipped — the
/// `print` below stays a statement, and a `Null @..` with a zero-width span
/// follows it.
#[test]
fn the_script_body_ends_in_null_and_not_in_its_last_statement() {
    insta::assert_snapshot!(parse_source("print(1)\n"));
}

/// `return` at the top level means "stop the script" and carries an optional
/// error (§2.4). It needs no new node: the script body is a function, so this
/// is the `return` every function body already has.
#[test]
fn return_at_the_top_level_is_an_ordinary_return() {
    insta::assert_snapshot!(parse_source(
        r#"let text, err be read_file("run.csv")
if err?:
    return err

print(text)
"#
    ));
}

// --- items and statements in one file ------------------------------------

/// §1.3: declarations and statements interleave freely, and a statement may
/// call a function declared below it (§3.2) because the resolver's collect
/// walk runs before any body is looked at.
///
/// This is the file a reader writes second, and the one a careless fix breaks:
/// the failure mode of a new top-level production is not that the statement is
/// rejected, it is that the declaration beside it is.
#[test]
fn items_and_statements_interleave() {
    insta::assert_snapshot!(parse_source(
        r#"use data (Record)

type Reading:
    temperature: F32

Reading implements Record

let readings be load()

print(summary(readings))

def summary(values: &Array[F32]) -> String:
    values.mean().to_string()
"#
    ));
}

/// A file of declarations only gets no `main` at all. The trigger for script
/// mode is the presence of a top-level statement and nothing else, so a module
/// that has none is byte for byte the module it was before this landed.
#[test]
fn a_file_of_declarations_only_gains_nothing() {
    insta::assert_snapshot!(parse_source(
        r#"type Doc:
    title: String

def render(doc: &Doc) -> String:
    doc.title
"#
    ));
}

// --- §8.2's disambiguation rule ------------------------------------------

/// The one place the grammar could be ambiguous, and the four lines that show
/// it is not.
///
/// Both an implementation header and a statement may begin with an identifier.
/// The rule is exact rather than heuristic: the line is an implementation
/// **if and only if** `implements` or `has` occurs in it at bracket depth zero
/// before its `:` or its end. Both are reserved words, so neither can appear
/// in an expression, a path, a type or an assignment target — which is why one
/// linear scan settles it with no backtracking and no speculative parse.
#[test]
fn a_name_begins_an_implementation_or_a_statement() {
    insta::assert_snapshot!(parse_source(
        r#"type Doc:
    title: String

interface Summarize:
    def summarize(self) -> String

Doc has:
    def new(title: String) -> Doc:
        Doc(title: title)

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

let doc be Doc.new("a")
doc.title
"#
    ));
}

/// The depth in "bracket depth zero" earning its keep: the `:` of a const
/// generic parameter is inside the list and is not the header's, so the scan
/// runs past it and finds the `has`.
#[test]
fn a_parameter_lists_colon_does_not_end_the_scan() {
    insta::assert_snapshot!(parse_source(
        r#"type Grid[T, const ROWS: Int]:
    cells: Array[T]

Grid[T, const ROWS: Int] has:
    def rows(self) -> Int:
        ROWS
"#
    ));
}

/// **The obligation that comes with an exact rule.**
///
/// §8.2's scan is exact only while `implements` and `has` cannot stand in
/// expression position. Nothing today threatens that and nothing should be
/// allowed to without noticing, so this asserts the premise rather than the
/// conclusion: each word, used where an expression belongs, is refused. The
/// day either of them parses here, the disambiguation upstairs has quietly
/// become a heuristic and this is what says so.
#[test]
fn implements_and_has_cannot_stand_in_expression_position() {
    for word in ["implements", "has"] {
        let source = format!("let x be {word}\n");
        let rendered = parse_source_allowing_errors(&source);
        assert!(
            rendered.contains("SC0105"),
            "`{word}` must not be an expression, but `{source}` parsed as:\n{rendered}"
        );
    }
}

// --- `SC0117` ------------------------------------------------------------

/// §2.2: a file with top-level statements *and* a declared `main` is an error.
///
/// The three alternatives each need a rule the reader will never look up, and
/// the recovery is the interesting half: the script body is **not** generated,
/// so the file gets one diagnostic about one mistake instead of `SC0117`
/// followed by a duplicate definition of a `main` only one of whose
/// declarations is anywhere in the file.
#[test]
fn a_script_body_and_a_declared_main_collide() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"let x be compute()

def main():
    print(x)
"#
    ));
}

/// The collision is over the *name*, so `tool main` collides too. Which word
/// declared it changes nothing about there being two.
#[test]
fn a_tool_named_main_collides_as_well() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"print("run")

## Runs the thing.
tool main():
    print("again")
"#
    ));
}

// --- `SC0107` ------------------------------------------------------------

/// `public` exports a declaration, and a statement is not one.
///
/// §8 of the note reuses the code the parser already spends on a misplaced
/// `public` rather than taking a number for this. The recovery is to step over
/// the word and parse the statement behind it, so one stray word costs exactly
/// one diagnostic — and the statement still reaches the script body, which the
/// dump shows.
#[test]
fn public_has_no_meaning_on_a_statement() {
    insta::assert_snapshot!(parse_source_allowing_errors("public print(\"hi\")\n"));
}

// --- what `SC0101` has left ----------------------------------------------

/// A token that can begin neither half of the top level.
///
/// This is all `SC0101` reports now, and the message says so: it names the
/// declaration keywords **and** "or a statement". The note it used to carry —
/// *"a module holds declarations only; a statement belongs in a function
/// body"* — was a grammar commitment in shipped diagnostic text, and §8.1
/// predicted the day it stopped being true.
#[test]
fn a_token_that_begins_neither_a_declaration_nor_a_statement() {
    insta::assert_snapshot!(parse_source_allowing_errors(")\n"));
}

/// A top-level line that begins with `[` is a statement.
///
/// §8.2's rule is that anything which is not one of the declaration words is
/// a statement exactly when it could be one, and "could be one" is
/// `starts_expr`. Nothing in the language declares with a `[`, so putting `[`
/// in that predicate moves this line into the script body and nowhere else.
#[test]
fn a_top_level_line_beginning_with_a_bracket_is_a_statement() {
    insta::assert_snapshot!(parse_source(
        "let xs be [1, 2, 3]
print(xs[0])
"
    ));
}
