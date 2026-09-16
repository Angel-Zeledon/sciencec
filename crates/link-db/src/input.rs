//! The inputs: the only values the outside world writes into the database.
//!
//! Everything else in the compiler is a function of these. They are written
//! exclusively through the methods on [`crate::LinkDatabase`], which is what
//! keeps the invariants in [`SourceFile`]'s documentation true.

use link_diagnostics::FileId;

/// One source file, as the database sees it.
///
/// # Invariants
///
/// These hold because `LinkDatabase` is the only thing that creates or writes
/// a `SourceFile`, and a lot depends on them:
///
/// 1. `file_id` is the slot this input was created in and never changes. It is
///    the same `FileId` a [`link_diagnostics::SourceMap`] built from this
///    database uses for the same file, so a span produced by any query renders
///    against the right file.
/// 2. `path` never changes either. A `SourceFile` is identified by its path
///    for the lifetime of the database; writing a different file's text into
///    it would silently reinterpret every span anyone already holds.
/// 3. The `FileId` -> `SourceFile` mapping is therefore append-only: a slot is
///    never reused for another path, not even after the file is removed. That
///    is what makes the lookup in `Db::source_file` safe to do *outside* the
///    dependency graph — an untracked read of an immutable mapping cannot go
///    stale.
///
/// Only `text` and `present` ever change.
#[salsa::input(debug)]
pub struct SourceFile {
    /// Invariant 1 above: set once, never written again.
    #[returns(copy)]
    pub(crate) file_id: FileId,
    /// Invariant 2 above: set once, never written again.
    #[returns(deref)]
    pub(crate) path: String,
    /// The file's contents. `""` for a file that has been removed or that was
    /// referred to (by an import, say) before anything read it from disk.
    #[returns(deref)]
    pub(crate) text: String,
    /// Whether the file actually exists as far as the driver knows.
    ///
    /// Kept apart from `text` because an empty file and a missing file are
    /// different things: the first is a module with no items, the second is an
    /// unresolved import.
    #[returns(copy)]
    pub(crate) present: bool,
}

/// The set of files being compiled.
///
/// A singleton, created with the database. It exists so that a query over "all
/// the files" — `module_tree`, and later `mono_items` — can depend on the file
/// *set* through the dependency graph rather than by reading the driver's
/// bookkeeping behind salsa's back. Adding or removing a file changes it;
/// editing one does not, so an edit never invalidates the module tree by
/// itself.
#[salsa::input(singleton)]
pub struct Workspace {
    /// Present files only, in `FileId` order.
    #[returns(deref)]
    pub(crate) files: Vec<FileId>,
}
