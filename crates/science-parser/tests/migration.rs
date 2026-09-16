//! The migration diagnostics of syntax revision 2: `SC0138`–`SC0144` from
//! the parser, and `SC0016`/`SC0017` from the lexer.
//!
//! Every word the revision removed is an ordinary identifier now, so code
//! written before it does not fail where the mistake is — it fails a token or
//! two later, as "expected a declaration" or "expected end of line". These are
//! the diagnostics that name the word instead, and they are the only channel a
//! language with no training corpus has for teaching its own syntax.
//!
//! Each test asserts four things: the code, the message, the *text the fix
//! replaces*, and what it replaces it with. The snapshot format does not
//! render suggestions, so a fix that pointed at the wrong span would otherwise
//! be invisible. And each pair ends with a second file proving the parser
//! carries on: one stale word must cost one diagnostic, not a cascade.

mod common;

use common::parse_source_allowing_errors;

/// The one diagnostic in a source, with the text its fix would replace.
#[track_caller]
fn only_fix(source: &str) -> (String, String, String, String) {
    let (tokens, lexical) = science_lexer::lex(common::FILE, source);
    assert!(lexical.iter().next().is_none(), "the source should lex clean");
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let mut found = diagnostics.iter();
    let diagnostic = found.next().expect("the stale word should have produced a diagnostic");
    assert!(found.next().is_none(), "one stale word is one diagnostic");
    let fix = diagnostic.suggestions.first().expect("the diagnostic should offer a fix");
    (
        diagnostic.code.to_string(),
        diagnostic.message.clone(),
        source[fix.span.start as usize..fix.span.end as usize].to_string(),
        fix.replacement.clone(),
    )
}

/// Every code a source reports, in the order a reader meets them.
///
/// Sorted by span rather than taken in push order, because that is what the
/// renderer does and therefore what an author sees. `while` is the one that
/// makes the difference: its diagnostic is built after its condition has been
/// read, so a stale phrase inside that condition is *pushed* first.
fn codes(source: &str) -> Vec<String> {
    let (tokens, _) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let mut found: Vec<(u32, String)> = diagnostics
        .iter()
        .map(|d| (d.primary_span().map(|s| s.start).unwrap_or(0), d.code.to_string()))
        .collect();
    found.sort_by_key(|(start, _)| *start);
    found.into_iter().map(|(_, code)| code).collect()
}

// --- `for each` (§2.1) ----------------------------------------------------

/// `each` left the loop and kept its other job. It is still a keyword, so the
/// stale loop arrives at the parser as a token rather than as a pattern, and
/// the fix takes both words down to one.
#[test]
fn each_after_for_is_reported() {
    let source = "def f(rows: borrowed Array of Int):\n    for each row in rows:\n        g(row)\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (code, message, replaced, replacement) = only_fix(source);
    assert_eq!(code, "SC0138");
    assert_eq!(message, "the loop is written `for x in xs`");
    assert_eq!(replaced, "for each");
    assert_eq!(replacement, "for");
}

/// Recovery drops the `each` and parses the loop, so the file after it is
/// read as it was meant to be.
#[test]
fn each_after_for_does_not_derail_the_rest_of_the_file() {
    let source = "def f(rows: borrowed Array of Int):\n    for each row in rows:\n        g(row)\n    for row in rows:\n        g(row)\n";
    assert_eq!(codes(source), ["SC0138"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- `trait` (§6) ---------------------------------------------------------

/// `trait` is an ordinary name now, so `trait Summarize:` looks like the head
/// of an implementation and would otherwise be reported as one.
#[test]
fn the_word_trait_is_reported_where_interface_belongs() {
    let source = "trait Summarize:\n    def summarize(self) -> String\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (code, message, replaced, replacement) = only_fix(source);
    assert_eq!(code, "SC0139");
    assert_eq!(message, "the declaration is written `interface`");
    assert_eq!(replaced, "trait");
    assert_eq!(replacement, "interface");
}

/// Recovery parses the rest of the declaration as the interface it is, so the
/// members inside it and the items after it are still read.
#[test]
fn the_word_trait_does_not_derail_the_rest_of_the_file() {
    let source = "trait Summarize:\n    def summarize(self) -> String\n\ninterface Pretty:\n    def pretty(self) -> String\n";
    assert_eq!(codes(source), ["SC0139"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- `has methods` (§5) ---------------------------------------------------

/// `methods` stopped being a keyword outright, which is what frees it as an
/// ordinary field or variable name.
#[test]
fn methods_after_has_is_reported() {
    let source = "Doc has methods:\n    def new() -> Doc:\n        Doc()\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (code, message, replaced, replacement) = only_fix(source);
    assert_eq!(code, "SC0141");
    assert_eq!(message, "the inherent block is written `Type has:`");
    assert_eq!(replaced, "has methods");
    assert_eq!(replacement, "has");
}

/// The block after it is an ordinary inherent block, so it parses.
#[test]
fn methods_after_has_does_not_derail_the_rest_of_the_file() {
    let source = "Doc has methods:\n    def new() -> Doc:\n        Doc()\n\nNote has:\n    def new() -> Note:\n        Note()\n";
    assert_eq!(codes(source), ["SC0141"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- `while` (§2.2) -------------------------------------------------------

/// `while` left the language. The fix replaces the whole head — the word and
/// its condition — with `loop`, because that is the part a tool can apply
/// without knowing how the body is indented; the note carries the rest.
#[test]
fn the_word_while_is_reported_where_loop_belongs() {
    let source = "def f(n: Int):\n    while n > 0:\n        g(n)\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (code, message, replaced, replacement) = only_fix(source);
    assert_eq!(code, "SC0142");
    assert_eq!(message, "the unbounded loop is written `loop`");
    assert_eq!(replaced, "while n > 0");
    assert_eq!(replacement, "loop");
}

/// The note is where the other half of the fix lives, so it has to say both
/// forms §2.2 leaves.
#[test]
fn the_while_diagnostic_names_both_replacements() {
    let source = "def f(n: Int):\n    while n > 0:\n        g(n)\n";
    let (tokens, _) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let note = diagnostics
        .iter()
        .next()
        .and_then(|d| d.notes.first().cloned())
        .expect("the diagnostic should carry a note");
    assert!(note.contains("break"), "the note should name the `break` form: {note}");
    assert!(note.contains("for i in 0..n"), "the note should name the range form: {note}");
}

/// Recovery parses the body as a `loop`, which is what applying the fix would
/// produce, so the statements after it are still read.
#[test]
fn the_word_while_does_not_derail_the_rest_of_the_file() {
    let source = "def f(n: Int):\n    while n > 0:\n        g(n)\n    loop:\n        break\n";
    assert_eq!(codes(source), ["SC0142"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- the comparison phrases (§1) ------------------------------------------

/// All four phrases, each with the symbol that replaced it. The fix has to
/// cover the whole phrase: replacing only `is` would leave `a >= least b`.
#[test]
fn every_comparison_phrase_is_reported_with_its_symbol() {
    for (phrase, symbol) in
        [("is at least", ">="), ("is at most", "<="), ("is above", ">"), ("is below", "<")]
    {
        let source = format!("def f(a: Int, b: Int) -> Bool:\n    a {phrase} b\n");
        let (code, message, replaced, replacement) = only_fix(&source);
        assert_eq!(code, "SC0143", "`{phrase}`");
        assert_eq!(message, format!("the comparison is written `{symbol}`"), "`{phrase}`");
        assert_eq!(replaced, phrase, "the fix must cover the whole phrase");
        assert_eq!(replacement, symbol, "`{phrase}`");
    }
}

/// The label quotes the phrase back, which means reading words that are
/// identifiers now next to the one keyword still in it.
///
/// The second source is the one that used to fire twice: the precedence climb
/// looks at every rung, and a phrase whose operands bind tighter than it does
/// is looked at once per rung above it.
#[test]
fn the_comparison_diagnostic_quotes_the_whole_phrase() {
    let source = "def f(a: Int, b: Int) -> Bool:\n    a is at least b\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (tokens, _) = science_lexer::lex(common::FILE, source);
    let (_, diagnostics) = science_parser::parse_module(&tokens, common::FILE);
    let label = diagnostics
        .iter()
        .next()
        .and_then(|d| d.labels.first().cloned())
        .expect("the diagnostic should carry a label");
    assert_eq!(label.message, "`is at least` is not an operator in Science");

    let nested = "def f(a: Int, b: Int) -> Bool:
    a + 1 is at least b * 2
";
    assert_eq!(codes(nested), ["SC0143"], "one phrase is one diagnostic, at any depth");
}

/// Recovery is the symbol itself, so the expression keeps the shape it meant
/// and the comparison that follows is untouched.
#[test]
fn a_comparison_phrase_does_not_derail_the_rest_of_the_file() {
    let source = "def f(a: Int, b: Int) -> Bool:\n    let first be a is at least b\n    a >= b\n";
    assert_eq!(codes(source), ["SC0143"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

/// `is above` with no operand after it is equality against a variable called
/// `above`, which is exactly what freeing the word was for (§1.2).
#[test]
fn a_freed_word_is_still_an_ordinary_name() {
    let source = "def f(a: Int, above: Int) -> Bool:\n    a is above\n";
    assert_eq!(codes(source), Vec::<String>::new());
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- `println` (§3.5) -----------------------------------------------------

/// `println` is not a name resolution would ever find, so catching it in the
/// parser costs nothing and says the right thing a phase earlier.
#[test]
fn the_word_println_is_reported_where_print_belongs() {
    let source = "def main():\n    println(\"hello\")\n";
    insta::assert_snapshot!(parse_source_allowing_errors(source));

    let (code, message, replaced, replacement) = only_fix(source);
    assert_eq!(code, "SC0144");
    assert_eq!(message, "the free function is `print`");
    assert_eq!(replaced, "println");
    assert_eq!(replacement, "print");
}

/// Recovery puts `print` in the tree, so the call keeps its arguments and the
/// rest of the body parses.
#[test]
fn the_word_println_does_not_derail_the_rest_of_the_file() {
    let source = "def main():\n    println(\"hello\")\n    print(\"again\")\n";
    assert_eq!(codes(source), ["SC0144"]);
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

// --- `==` and `!=` (§1) ---------------------------------------------------
//
// These two are the lexer's rather than the parser's, because the symbols are
// characters and not words: nothing reaches the parser to complain about. The
// mechanism is the same one every test above checks, though — report once,
// emit the token anyway, and let the rest of the file parse — so the
// assertions live here with the rest of the revision.

/// The one *lexical* diagnostic in a source, with the text its fix replaces.
///
/// `only_fix` insists the source lexes clean, which the removed symbols by
/// definition do not. This is the same four-part assertion on the other side
/// of the phase boundary, and it checks the parser stayed silent: `==` costs
/// its own diagnostic and nothing else.
#[track_caller]
fn only_lexical_fix(source: &str) -> (String, String, String, String) {
    let (tokens, lexical) = science_lexer::lex(common::FILE, source);
    let mut found = lexical.iter();
    let diagnostic = found.next().expect("the stale symbol should have produced a diagnostic");
    assert!(found.next().is_none(), "one stale symbol is one diagnostic");

    let (_, syntax) = science_parser::parse_module(&tokens, common::FILE);
    assert!(
        syntax.is_empty(),
        "the symbol is reported once and then parses: {:?}",
        syntax.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );

    let fix = diagnostic.suggestions.first().expect("the diagnostic should offer a fix");
    (
        diagnostic.code.to_string(),
        diagnostic.message.clone(),
        source[fix.span.start as usize..fix.span.end as usize].to_string(),
        fix.replacement.clone(),
    )
}

/// `==` is SC0016, and its fix replaces exactly the two characters with `is`.
#[test]
fn the_equality_symbol_is_reported() {
    let source = "def f(a: Int, b: Int) -> Bool:
    a == b
";
    let (code, message, replaced, replacement) = only_lexical_fix(source);
    assert_eq!(code, "SC0016");
    assert_eq!(message, "equality is written `is`");
    assert_eq!(replaced, "==");
    assert_eq!(replacement, "is");
}

/// `!=` is SC0017, and its fix is the two words. It is also SC0017 *alone*:
/// the lone-`!` diagnostic must never fire alongside it.
#[test]
fn the_inequality_symbol_is_reported() {
    let source = "def f(a: Int, b: Int) -> Bool:
    a != b
";
    let (code, message, replaced, replacement) = only_lexical_fix(source);
    assert_eq!(code, "SC0017");
    assert_eq!(message, "inequality is written `is not`");
    assert_eq!(replaced, "!=");
    assert_eq!(replacement, "is not");
}

/// The tree a stale symbol produces is the tree the fix would produce, so the
/// file after it parses exactly as it was meant to.
#[test]
fn the_removed_symbols_do_not_derail_the_rest_of_the_file() {
    let source = "def f(a: Int, b: Int) -> Bool:
    let same be a == b
    let other be a is b
    same and other
";
    assert_eq!(codes(source), Vec::<String>::new(), "the parser has nothing to say");
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}

/// Assignment still belongs to the parser. A single `=` is not caught by the
/// new lexical arm on its way past, so `a = b` keeps the message it had.
#[test]
fn a_single_equals_is_still_the_missing_be_and_not_the_new_diagnostic() {
    let source = "def main():
    a = 3
";
    let (tokens, lexical) = science_lexer::lex(common::FILE, source);
    assert!(lexical.iter().next().is_none(), "a lone `=` is not a comparison symbol");

    let (_, syntax) = science_parser::parse_module(&tokens, common::FILE);
    let diagnostic = syntax.iter().next().expect("`a = 3` should still be reported");
    assert_eq!(diagnostic.message, "assignment is written `be`");
    let fix = diagnostic.suggestions.first().expect("the diagnostic should offer a fix");
    assert_eq!(&source[fix.span.start as usize..fix.span.end as usize], "=");
    assert_eq!(fix.replacement, "be");
}

// --- all of them at once ---------------------------------------------------

/// A whole file written in the pre-revision syntax. Every stale word gets its
/// own diagnostic and none of them swallows the next, which is the property
/// that makes these worth having: an author migrating a file wants the whole
/// list in one pass, not one error per run.
#[test]
fn a_pre_revision_file_reports_every_word_once() {
    let source = "trait Summarize:\n    def summarize(self) -> String\n\nDoc has methods:\n    def show(self, rows: borrowed Array of Int, n: Int):\n        for each row in rows:\n            println(row)\n        while n is at least 0:\n            n be n - 1\n";
    assert_eq!(
        codes(source),
        ["SC0139", "SC0141", "SC0138", "SC0144", "SC0142", "SC0143"],
        "every stale word is reported, in the order it is written"
    );
    insta::assert_snapshot!(parse_source_allowing_errors(source));
}
