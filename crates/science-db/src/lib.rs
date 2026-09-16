//! The compiler database: every phase of Science expressed as a memoized query.
//!
//! §7.3 of the design spec: each phase is a query over a dependency graph, not
//! a function called once, so that changing a file invalidates only what
//! depended on it. This crate holds the database, the input queries, the
//! derived queries for the phases that exist, and the declared-but-unwritten
//! signatures of the ones that do not ([`pending`]).
//!
//! # Using it
//!
//! ```
//! use science_db::{ast, tokens, ScienceDatabase};
//!
//! let mut db = ScienceDatabase::new();
//! let file = db.add_file("main.science", "function main():\n    1\n");
//!
//! let parsed = ast(&db, file);
//! assert!(parsed.diagnostics().is_empty());
//! assert_eq!(parsed.value().items.len(), 1);
//!
//! // Writing the same text back changes nothing, so nothing is recomputed.
//! db.set_file_text(file, "function main():\n    1\n");
//! assert!(!tokens(&db, file).has_errors());
//! ```
//!
//! # Which salsa, and why
//!
//! `salsa 0.28`. The crate has changed shape more than once — the 0.16 line
//! with `#[salsa::query_group]` traits and jars, the separate `salsa-2022`
//! experiment, then the 0.17+ line that became the current API — and material
//! written for any of those does not apply here. What 0.28 actually offers,
//! and what this crate relies on:
//!
//! * `#[salsa::db]` on a plain struct holding a `salsa::Storage<Self>`. No
//!   jars, no query-group traits, no `#[salsa::database(...)]` list: an
//!   ingredient registers itself. The `Db` trait in this crate is ours, not
//!   salsa's machinery, and exists so other crates can define queries without
//!   naming [`ScienceDatabase`].
//! * `#[salsa::input]` structs for what the outside world writes, with a
//!   per-field dependency: a query that reads only `text` is not invalidated
//!   when `present` changes.
//! * `#[salsa::tracked]` functions for derived queries, keyed on a salsa
//!   struct, memoized, with backdating: after re-executing, salsa compares the
//!   new result with the old one using `PartialEq` and, if they are equal,
//!   keeps the old "changed at" revision so the change stops propagating
//!   there.
//! * `salsa::Event` / `EventKind::WillExecute`, which is how the tests in
//!   `tests/incremental.rs` prove that memoization happens instead of assuming
//!   it. See [`ScienceDatabase::with_execution_log`].
//!
//! The `Update` trait older material describes is gone; a `'static` result
//! type needs nothing declared for it, which matters because the types these
//! queries return (`Vec<Token>`, `Module`) belong to other crates and cannot
//! have a salsa derive added to them from here.
//!
//! One behaviour is worth stating because it is a trap rather than a
//! preference: **salsa's input setters do not compare the old value with the
//! new one**, they unconditionally mark the field as changed. Writing a file's
//! own text back to it would therefore start a revision and recompute it.
//! [`ScienceDatabase::set_file_text`] filters that out, which is what makes §11
//! point 4 of the spec hold rather than almost hold.
//!
//! # `FileId` and the query key
//!
//! §7.3 writes the file queries as `tokens(FileId)`. Salsa keys a query on a
//! value it can turn into an id, which a bare `science_diagnostics::FileId` is
//! not, so the tracked functions in [`query`] are keyed on [`SourceFile`], the
//! input struct, and the `FileId` spelling lives here at the crate root:
//! [`source_text`], [`tokens`], [`ast`], [`file_diagnostics`]. They look the
//! file up in the database's table and forward.
//!
//! `FileId` stays the compiler's file identity — it is what spans carry and
//! what [`science_diagnostics::SourceMap`] indexes — and
//! [`ScienceDatabase::source_map`] guarantees the two agree.

pub mod db;
pub mod input;
pub mod pending;
pub mod query;
pub mod result;

pub use db::{Db, ScienceDatabase};
pub use input::{SourceFile, Workspace};
pub use result::WithDiagnostics;

use science_diagnostics::{Diagnostic, FileId};
use science_lexer::Token;
use science_parser::ast::Module;

/// The text of a file. `""` for a file that was removed or never read.
///
/// The input query of §7.3, reading the field the whole pipeline hangs off.
/// Returns a borrow rather than the `String` the spec writes, so that asking
/// for a file's text does not copy it.
///
/// # Panics
/// If the `FileId` was not issued by this database.
pub fn source_text(db: &dyn Db, file: FileId) -> &str {
    db.source_file(file).text(db)
}

/// The token stream of a file, with its lexical diagnostics.
/// See [`query::tokens`].
///
/// # Panics
/// If the `FileId` was not issued by this database.
pub fn tokens(db: &dyn Db, file: FileId) -> &WithDiagnostics<Vec<Token>> {
    query::tokens(db, db.source_file(file))
}

/// The syntax tree of a file, with its syntax diagnostics. See [`query::ast`].
///
/// # Panics
/// If the `FileId` was not issued by this database.
pub fn ast(db: &dyn Db, file: FileId) -> &WithDiagnostics<Module> {
    query::ast(db, db.source_file(file))
}

/// Everything reported about a file, in pipeline order.
/// See [`query::file_diagnostics`].
///
/// # Panics
/// If the `FileId` was not issued by this database.
pub fn file_diagnostics(db: &dyn Db, file: FileId) -> &[Diagnostic] {
    query::file_diagnostics(db, db.source_file(file))
}

/// The files being compiled, in `FileId` order, as a query.
///
/// The tracked counterpart of [`ScienceDatabase::present_files`]: a query that
/// reads this depends on the file *set*, so adding or removing a file
/// invalidates it and editing one does not.
pub fn workspace_files(db: &dyn Db) -> &[FileId] {
    db.workspace().files(db)
}
