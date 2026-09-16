//! What the queries return: the file table, diagnostics flowing through, and
//! the `SourceMap` that `science-diagnostics` renders against.

use science_db::{ast, file_diagnostics, source_text, tokens, Db, ScienceDatabase};
use science_diagnostics::{FileId, Severity};
use science_lexer::TokenKind;

const VALID: &str = "fn main():\n    1\n";

#[test]
fn source_text_round_trips() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    assert_eq!(source_text(&db, a), VALID);
    assert_eq!(db.path(a), "a.science");
    assert!(db.is_present(a));
}

#[test]
fn a_path_keeps_one_file_id() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    let b = db.add_file("b.science", VALID);
    assert_ne!(a, b);
    assert_eq!(db.add_file("a.science", "fn other():\n    2\n"), a);
    assert_eq!(db.file_id("a.science"), a);
    assert_eq!(source_text(&db, a), "fn other():\n    2\n");
}

#[test]
fn a_file_can_be_referred_to_before_it_is_read() {
    let mut db = ScienceDatabase::new();
    // The driver resolves an import to a path it has not read yet.
    let a = db.file_id("not_read_yet.science");
    assert!(!db.is_present(a));
    assert_eq!(source_text(&db, a), "");
    assert!(ast(&db, a).value().items.is_empty());

    // Reading it later fills the same slot.
    assert_eq!(db.add_file("not_read_yet.science", VALID), a);
    assert!(db.is_present(a));
    assert_eq!(ast(&db, a).value().items.len(), 1);
}

#[test]
fn removing_a_file_keeps_its_slot() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    let b = db.add_file("b.science", VALID);
    db.remove_file(a);

    assert!(!db.is_present(a));
    assert_eq!(source_text(&db, a), "");
    assert_eq!(db.present_files(), vec![b]);

    // Re-adding the same path reuses the id rather than shifting b's.
    assert_eq!(db.add_file("a.science", VALID), a);
    assert_eq!(db.present_files(), vec![a, b]);
}

#[test]
fn tokens_carry_the_lexers_diagnostics() {
    let mut db = ScienceDatabase::new();
    // `§` is not a character of the language: SC0001.
    let a = db.add_file("a.science", "fn main():\n    §\n");

    let result = tokens(&db, a);
    assert!(
        !result.value().is_empty(),
        "lexing recovers, it does not bail out"
    );
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.diagnostics()[0].code.to_string(), "SC0001");
    assert!(result.has_errors());
}

#[test]
fn ast_carries_the_parsers_diagnostics() {
    let mut db = ScienceDatabase::new();
    // A `fn` with no name is a syntax error, LK01xx.
    let a = db.add_file("a.science", "fn ():\n    1\n");

    let result = ast(&db, a);
    assert!(result.has_errors());
    assert!(result
        .diagnostics()
        .iter()
        .all(|d| d.code.to_string().starts_with("SC01")));
}

#[test]
fn a_valid_file_produces_no_diagnostics() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    assert!(tokens(&db, a).diagnostics().is_empty());
    assert!(ast(&db, a).diagnostics().is_empty());
    assert!(file_diagnostics(&db, a).is_empty());
}

/// Each query carries its own phase's diagnostics and no one else's, so a
/// caller that wants everything for a file asks for it explicitly and gets no
/// duplicates.
#[test]
fn file_diagnostics_concatenates_the_phases_in_order() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", "fn §():\n    1\n");

    let lexical = tokens(&db, a).diagnostics().len();
    let syntax = ast(&db, a).diagnostics().len();
    assert!(lexical > 0 && syntax > 0, "this source must fail both phases");

    let all = file_diagnostics(&db, a);
    assert_eq!(all.len(), lexical + syntax);
    assert_eq!(&all[..lexical], tokens(&db, a).diagnostics());
    assert!(all.iter().all(|d| d.severity == Severity::Error));
}

#[test]
fn tokens_end_with_eof() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    assert_eq!(tokens(&db, a).value().last().unwrap().kind, TokenKind::Eof);
}

/// The `FileId`s the database hands out have to be the same ones a
/// `SourceMap` uses, or every rendered diagnostic points at the wrong file.
#[test]
fn file_ids_agree_with_the_source_map() {
    let mut db = ScienceDatabase::new();
    let a = db.add_file("a.science", VALID);
    let b = db.add_file("b.science", "fn other():\n    2\n");
    db.remove_file(a);
    let c = db.add_file("c.science", VALID);

    let map = db.source_map();
    assert_eq!(map.file_count(), 3);
    assert_eq!(map.path(b), "b.science");
    assert_eq!(map.text(b), "fn other():\n    2\n");
    assert_eq!(map.path(c), "c.science");
    // A removed file keeps its slot so that the ids of the others do not move.
    assert_eq!(map.path(a), "a.science");
    assert_eq!(map.text(a), "");
}

#[test]
fn a_diagnostic_from_a_query_renders_against_the_databases_source_map() {
    let mut db = ScienceDatabase::new();
    let _pad = db.add_file("pad.science", VALID);
    let a = db.add_file("a.science", "fn main():\n    §\n");

    let diagnostic = tokens(&db, a).diagnostics()[0].clone();
    let map = db.source_map();
    let rendered = science_diagnostics::render(&map, &diagnostic);

    assert!(rendered.contains("a.science"), "{rendered}");
    assert!(rendered.contains("2:5"), "{rendered}");
}

/// `FileId`s are only meaningful inside the database that issued them.
#[test]
#[should_panic(expected = "does not belong to this database")]
fn an_unknown_file_id_panics() {
    let db = ScienceDatabase::new();
    let _ = db.source_file(FileId(7));
}
