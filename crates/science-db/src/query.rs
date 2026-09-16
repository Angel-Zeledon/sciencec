//! The derived queries for the phases that exist today.
//!
//! These are the salsa ingredients, keyed on [`SourceFile`]. The `FileId`-keyed
//! spelling from §7.3 of the design spec — `tokens(FileId)`, `ast(FileId)` —
//! lives at the crate root and forwards here; see the crate documentation for
//! why the key is not a `FileId` at this level.

use science_diagnostics::Diagnostic;
use science_lexer::Token;
use science_parser::ast::Module;

use crate::db::Db;
use crate::input::SourceFile;
use crate::result::WithDiagnostics;

/// The token stream of a file, with the lexical diagnostics (SC0001-SC0099)
/// produced while reading it.
///
/// Depends only on the file's text, so an edit to any other file leaves it
/// alone.
#[salsa::tracked]
pub fn tokens(db: &dyn Db, file: SourceFile) -> WithDiagnostics<Vec<Token>> {
    let (tokens, diagnostics) = science_lexer::lex(file.file_id(db), file.text(db));
    WithDiagnostics::new(tokens, diagnostics)
}

/// The syntax tree of a file, with the syntax diagnostics
/// (SC0100-SC0199) produced while building it.
///
/// Depends on the file's text only through [`tokens`], which is what makes an
/// edit that does not change the token stream stop here instead of reaching
/// the phases above: salsa compares the new token stream with the memoized one
/// and, when they are equal, never re-enters this function.
#[salsa::tracked]
pub fn ast(db: &dyn Db, file: SourceFile) -> WithDiagnostics<Module> {
    let tokens = tokens(db, file);
    let (module, diagnostics) = science_parser::parse_module(tokens.value(), file.file_id(db));
    WithDiagnostics::new(module, diagnostics)
}

/// Everything reported about a file by the phases that have run over it, in
/// pipeline order.
///
/// Each query carries only its own phase's diagnostics, so a caller that wants
/// all of them asks for them here rather than collecting the same diagnostic
/// twice from two queries. As later phases land they are appended here.
#[salsa::tracked]
pub fn file_diagnostics(db: &dyn Db, file: SourceFile) -> Vec<Diagnostic> {
    let mut all = tokens(db, file).diagnostics().to_vec();
    all.extend_from_slice(ast(db, file).diagnostics());
    all
}
