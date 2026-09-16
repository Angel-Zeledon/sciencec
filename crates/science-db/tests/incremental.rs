//! Proofs that the query layer is incremental.
//!
//! Every test here asserts on *executions*, not on returned values. A test
//! that only compares what a query returned proves nothing about memoization:
//! recomputing from scratch returns the same answer. The counts come from
//! salsa's own `WillExecute` event, collected by [`ScienceDatabase::with_execution_log`],
//! so nothing in the query bodies has to cooperate for the count to be right.
//!
//! §11 point 4 of the design spec is what these tests are for: "modifying one
//! of three files does not recompile all three".

use science_db::{ast, tokens, ScienceDatabase};

/// A valid module. Its exact contents do not matter, only that it lexes and
/// parses without diagnostics.
const A: &str = "fn a() -> Int:\n    1\n";
const B: &str = "fn b() -> Int:\n    2\n";

#[test]
fn a_query_runs_once_and_is_then_reused() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);

    let first = tokens(&db, a).value().len();
    assert_eq!(db.executions("tokens"), 1, "first call must execute");

    db.clear_execution_log();
    let second = tokens(&db, a).value().len();

    assert_eq!(first, second);
    assert_eq!(
        db.executions("tokens"),
        0,
        "re-requesting an unchanged query must not recompute it"
    );
}

#[test]
fn ast_is_reused_across_unrelated_revisions() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let _ = ast(&db, a);

    // A brand new file starts a new revision, but `a` did not change.
    db.clear_execution_log();
    let b = db.add_file("b.science", B);

    let _ = ast(&db, a);
    assert_eq!(db.executions_for("tokens", a), 0);
    assert_eq!(db.executions_for("ast", a), 0);

    // ... and `b` is computed from scratch, once.
    let _ = ast(&db, b);
    assert_eq!(db.executions_for("tokens", b), 1);
    assert_eq!(db.executions_for("ast", b), 1);
}

#[test]
fn changing_one_file_invalidates_only_that_file() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let b = db.add_file("b.science", B);
    let _ = ast(&db, a);
    let _ = ast(&db, b);

    db.clear_execution_log();
    db.set_file_text(a, "fn a() -> Int:\n    99\n");

    // Requesting both again: only `a` is recomputed.
    let _ = ast(&db, a);
    let _ = ast(&db, b);

    assert_eq!(db.executions_for("tokens", a), 1, "a's tokens are stale");
    assert_eq!(db.executions_for("ast", a), 1, "a's ast is stale");
    assert_eq!(db.executions_for("tokens", b), 0, "b was not touched");
    assert_eq!(db.executions_for("ast", b), 0, "b was not touched");
    assert_eq!(db.executions("tokens"), 1, "no other file recomputed");
    assert_eq!(db.executions("ast"), 1, "no other file recomputed");
}

/// The property §11 point 4 actually demands, and the one that silently fails
/// to hold: salsa's input setters do *not* compare the old and new value, they
/// unconditionally mark the field as changed. Writing the same text back must
/// therefore be filtered out before it reaches the setter.
#[test]
fn setting_the_text_a_file_already_had_invalidates_nothing() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let b = db.add_file("b.science", B);
    let _ = ast(&db, a);
    let _ = ast(&db, b);

    db.clear_execution_log();
    db.set_file_text(a, A); // byte-for-byte what it already had

    let _ = ast(&db, a);
    let _ = ast(&db, b);

    assert_eq!(
        db.executions("tokens"),
        0,
        "an identical write must not invalidate the lexer"
    );
    assert_eq!(
        db.executions("ast"),
        0,
        "an identical write must not invalidate the parser"
    );
}

/// The same property one level up: `add_file` on an already registered path is
/// an update, so re-adding a file the driver already read is free.
#[test]
fn re_adding_a_file_with_identical_text_invalidates_nothing() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let _ = ast(&db, a);

    db.clear_execution_log();
    let again = db.add_file("a.science", A);

    assert_eq!(again, a, "a path keeps its FileId");
    let _ = ast(&db, a);
    assert_eq!(db.executions("tokens"), 0);
    assert_eq!(db.executions("ast"), 0);
}

#[test]
fn removing_a_file_invalidates_it_and_nothing_else() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let b = db.add_file("b.science", B);
    let _ = ast(&db, a);
    let _ = ast(&db, b);

    db.clear_execution_log();
    db.remove_file(a);

    let after = ast(&db, a);
    let _ = ast(&db, b);

    assert!(after.value().items.is_empty(), "a removed file has no items");
    assert_eq!(db.executions_for("tokens", a), 1);
    assert_eq!(db.executions_for("tokens", b), 0);
    assert_eq!(db.executions_for("ast", b), 0);
}

/// Removing a file twice is idempotent, so it must not start a revision either.
#[test]
fn removing_an_absent_file_invalidates_nothing() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let b = db.add_file("b.science", B);
    db.remove_file(a);
    let _ = ast(&db, a);
    let _ = ast(&db, b);

    db.clear_execution_log();
    db.remove_file(a);

    let _ = ast(&db, a);
    let _ = ast(&db, b);
    assert_eq!(db.executions("tokens"), 0);
    assert_eq!(db.executions("ast"), 0);
}

/// An edit that genuinely changes the text but not the token stream stops at
/// the lexer.
///
/// The guard in `set_file_text` cannot help here: the text really did change,
/// so the write reaches salsa and `tokens` really does re-execute. What stops
/// the change is backdating — salsa compares the new token stream with the
/// memoized one, finds them equal, and never re-enters `ast`. Editing a
/// comment is the everyday case: comments produce no tokens, so as long as the
/// replacement is the same length, every span behind it stays put.
#[test]
fn an_edit_the_lexer_absorbs_does_not_reach_the_parser() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", "# aaaa\nfn a() -> Int:\n    1\n");
    let before = ast(&db, a).clone();

    db.clear_execution_log();
    db.set_file_text(a, "# bbbb\nfn a() -> Int:\n    1\n");
    let after = ast(&db, a);

    assert_eq!(db.executions_for("tokens", a), 1, "the text did change");
    assert_eq!(
        db.executions_for("ast", a),
        0,
        "the token stream did not, so the parser must not run again"
    );
    assert_eq!(&before, after);
}

/// `ast` depends on `tokens`, not on the text, so a text change that reaches
/// the parser must go through the lexer first. Proven by the execution order.
#[test]
fn the_parser_runs_behind_the_lexer() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let _ = ast(&db, a);

    let log = db.execution_log();
    let tokens_at = log.iter().position(|e| e.starts_with("tokens(")).unwrap();
    let ast_at = log.iter().position(|e| e.starts_with("ast(")).unwrap();
    assert!(
        ast_at < tokens_at,
        "ast is entered first and calls tokens from inside: {log:?}"
    );
}

/// Guards the helpers the tests above rely on. If salsa ever changes how it
/// labels a query in its event log, `executions_for` would quietly count zero
/// and the "did not recompute" assertions would pass vacuously. This test
/// fails loudly instead.
#[test]
fn the_execution_log_is_labelled_the_way_the_helpers_assume() {
    let mut db = ScienceDatabase::with_execution_log();
    let a = db.add_file("a.science", A);
    let _ = tokens(&db, a);

    let log = db.execution_log();
    assert!(
        log.contains(&db.query_label("tokens", a)),
        "log {log:?} does not contain {:?}",
        db.query_label("tokens", a)
    );
    assert_eq!(db.executions_for("tokens", a), 1);
    assert_eq!(db.executions("tokens"), 1);

    // A different file must not share a label.
    let b = db.add_file("b.science", B);
    assert_ne!(db.query_label("tokens", a), db.query_label("tokens", b));
    assert_eq!(db.executions_for("tokens", b), 0);
}

/// Without a log the database still works; the counters are simply empty.
#[test]
fn logging_is_opt_in() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", A);
    let _ = ast(&db, a);
    assert!(db.execution_log().is_empty());
    assert_eq!(db.executions("tokens"), 0);
}
