//! The database: the file table, the input queries, and the bridge to
//! `link-diagnostics`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use link_diagnostics::{FileId, SourceMap};
use salsa::Setter;

use crate::input::{SourceFile, Workspace};

/// What every query in the compiler is written against.
///
/// Query definitions take `&dyn Db` rather than a concrete database so that
/// the later crates (`link-resolve`, `link-types`, ...) can define their own
/// queries without depending on `LinkDatabase`'s private state.
///
/// The two required methods are the bridge between `FileId` — the id spans
/// carry, shared with [`SourceMap`] — and `SourceFile`, the salsa input a
/// query is keyed on.
#[salsa::db]
pub trait Db: salsa::Database {
    /// The input for `file`, or `None` if it was never issued by this
    /// database.
    ///
    /// This is an **untracked** read: it does not record a dependency. That is
    /// sound only because the mapping is append-only and a slot is never
    /// reused for a different path (see [`SourceFile`]'s invariants). Do not
    /// replace it with something that can change without documenting how a
    /// query that read it gets invalidated.
    fn try_source_file(&self, file: FileId) -> Option<SourceFile>;

    /// The set of files being compiled, as a salsa input.
    fn workspace(&self) -> Workspace;

    /// The input for `file`.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database. Every id a caller can
    /// hold came from `add_file` or `file_id`, so an unknown one is a bug in
    /// the caller — the same contract `SourceMap` has.
    fn source_file(&self, file: FileId) -> SourceFile {
        self.try_source_file(file).unwrap_or_else(|| {
            panic!("FileId({}) does not belong to this database", file.0)
        })
    }
}

type Log = Arc<Mutex<Vec<String>>>;

/// The compiler database.
///
/// Holds the salsa storage plus the file table that maps the `FileId`s used by
/// spans and diagnostics onto salsa inputs.
///
/// Deliberately **not** `Clone`. Salsa's storage can be cloned to hand a
/// second thread its own handle, but the file table next to it cannot: two
/// handles that each added a file would disagree about which slot holds which
/// path, and [`SourceFile`]'s invariants would stop being true. F0 is
/// single-threaded (§12 of the design spec); when that changes, the file table
/// moves behind a shared lock and `Clone` comes back with it.
#[salsa::db]
pub struct LinkDatabase {
    storage: salsa::Storage<Self>,

    /// Slot `n` holds the input for `FileId(n)`. Append-only: entries are
    /// never removed or reordered, because `FileId`s are handed out to spans
    /// that outlive any individual file.
    files: Vec<SourceFile>,

    /// Reverse index, so that reading the same path twice yields one slot.
    by_path: HashMap<String, FileId>,

    /// Set during construction and never `None` afterwards.
    workspace: Option<Workspace>,

    /// Collects the key of every query salsa decides to execute, when logging
    /// is enabled. See [`LinkDatabase::with_execution_log`].
    log: Option<Log>,
}

impl Default for LinkDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkDatabase {
    /// An empty database.
    pub fn new() -> Self {
        Self::build(None)
    }

    /// An empty database that records which queries actually execute.
    ///
    /// The log comes from salsa's own `WillExecute` event, so it reports what
    /// the engine did, not what a query body chose to report. It is what the
    /// incrementality tests assert on, and what a `--verbose` driver would
    /// print to explain a rebuild.
    pub fn with_execution_log() -> Self {
        Self::build(Some(Arc::new(Mutex::new(Vec::new()))))
    }

    fn build(log: Option<Log>) -> Self {
        let callback = log.clone().map(|log| {
            let callback = move |event: salsa::Event| {
                if let salsa::EventKind::WillExecute { database_key } = event.kind {
                    // `DatabaseKeyIndex`'s `Debug` prints `query(Id(n))`, which
                    // is what `query_label` reconstructs.
                    log.lock()
                        .expect("execution log poisoned")
                        .push(format!("{database_key:?}"));
                }
            };
            Box::new(callback) as Box<dyn Fn(salsa::Event) + Send + Sync>
        });

        let mut db = LinkDatabase {
            storage: salsa::Storage::new(callback),
            files: Vec::new(),
            by_path: HashMap::new(),
            workspace: None,
            log,
        };
        let workspace = Workspace::new(&db, Vec::new());
        db.workspace = Some(workspace);
        db
    }

    // --- input queries ----------------------------------------------------

    /// The `FileId` for `path`, allocating a slot if this is the first time it
    /// is mentioned.
    ///
    /// A path may be named before its contents are known — an import resolves
    /// to a file the driver has not read yet. The slot it gets is absent
    /// (`is_present` is false, its text is `""`) until [`add_file`] fills it
    /// in, and because the queries that asked about it depend on that slot's
    /// fields, filling it in later invalidates them properly.
    ///
    /// [`add_file`]: LinkDatabase::add_file
    pub fn file_id(&mut self, path: impl Into<String>) -> FileId {
        let path = path.into();
        if let Some(&id) = self.by_path.get(&path) {
            return id;
        }
        let id = FileId(self.files.len() as u32);
        let file = SourceFile::new(self, id, path.clone(), String::new(), false);
        self.files.push(file);
        self.by_path.insert(path, id);
        id
    }

    /// Registers a file and its contents.
    ///
    /// Re-adding a path that is already registered updates it in place and
    /// keeps its `FileId`; if the text is identical, nothing is written at
    /// all, so re-reading an unchanged file from disk costs nothing
    /// downstream.
    pub fn add_file(&mut self, path: impl Into<String>, text: impl Into<String>) -> FileId {
        let id = self.file_id(path);
        self.set_file_text(id, text);
        self.set_present(id, true);
        id
    }

    /// Replaces a file's contents.
    ///
    /// Writing back the text a file already had is a no-op: it does not start
    /// a new revision and therefore cannot invalidate anything. That check is
    /// this method's reason to exist. Salsa's own setters do not compare the
    /// old value with the new one — its documentation is explicit that "a
    /// setter always records its field as changed" — so without the guard here
    /// an editor that saves an unmodified buffer would recompile the file, and
    /// §11 point 4 of the design spec would hold only by accident.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database.
    pub fn set_file_text(&mut self, file: FileId, text: impl Into<String>) {
        let input = self.source_file(file);
        let text = text.into();
        if input.text(self) == text {
            return;
        }
        input.set_text(self).to(text);
    }

    /// Marks a file as no longer existing.
    ///
    /// The slot survives: `FileId`s are never recycled, so the ids of the
    /// other files — and every span already produced against them — stay
    /// valid. The file's text becomes `""`, which is what the queries
    /// downstream see. Removing a file that is already absent does nothing.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database.
    pub fn remove_file(&mut self, file: FileId) {
        self.set_file_text(file, "");
        self.set_present(file, false);
    }

    fn set_present(&mut self, file: FileId, present: bool) {
        let input = self.source_file(file);
        if input.present(self) == present {
            return;
        }
        input.set_present(self).to(present);
        self.refresh_workspace();
    }

    /// Recomputes the workspace's file list after a file appeared or
    /// disappeared. Only writes it back when it actually changed, for the same
    /// reason as [`LinkDatabase::set_file_text`].
    fn refresh_workspace(&mut self) {
        let present: Vec<FileId> = self
            .files
            .iter()
            .filter(|f| f.present(self))
            .map(|f| f.file_id(self))
            .collect();
        let workspace = self.workspace();
        if workspace.files(self) == present {
            return;
        }
        workspace.set_files(self).to(present);
    }

    // --- reading the file table -------------------------------------------

    /// The path `file` was registered under.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database.
    pub fn path(&self, file: FileId) -> &str {
        self.source_file(file).path(self)
    }

    /// Whether `file` exists as far as the driver knows.
    ///
    /// An empty file and a missing file are different things: the first is a
    /// module with no items, the second is an unresolved import. Both have
    /// `""` for their text, and this is what tells them apart.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database.
    pub fn is_present(&self, file: FileId) -> bool {
        self.source_file(file).present(self)
    }

    /// Every file that currently exists, in `FileId` order.
    pub fn present_files(&self) -> Vec<FileId> {
        self.workspace().files(self).to_vec()
    }

    // --- the bridge to link-diagnostics -----------------------------------

    /// A [`SourceMap`] mirroring the current contents of the database.
    ///
    /// `SourceMap` hands out `FileId`s sequentially and never mutates a file,
    /// so this rebuilds it in slot order: slot 0 first, then 1, and so on.
    /// The ids therefore agree by construction, including for removed files,
    /// which keep their slot and come back with empty text rather than
    /// shifting everything after them.
    ///
    /// Rebuilt on demand instead of cached, because a `SourceMap` is only
    /// needed when a diagnostic is rendered — once per compilation, against
    /// text that has already been read — while the database is written on
    /// every keystroke.
    pub fn source_map(&self) -> SourceMap {
        let mut map = SourceMap::new();
        for (slot, file) in self.files.iter().enumerate() {
            let id = map.add_file(file.path(self).to_owned(), file.text(self).to_owned());
            debug_assert_eq!(
                id,
                FileId(slot as u32),
                "SourceMap ids must line up with the database's slots"
            );
        }
        map
    }

    // --- execution instrumentation ----------------------------------------

    /// Every query salsa has executed since the log was last cleared, in
    /// order, labelled as `query(Id(n))`.
    ///
    /// Empty unless the database was built with
    /// [`LinkDatabase::with_execution_log`].
    pub fn execution_log(&self) -> Vec<String> {
        match &self.log {
            Some(log) => log.lock().expect("execution log poisoned").clone(),
            None => Vec::new(),
        }
    }

    pub fn clear_execution_log(&mut self) {
        if let Some(log) = &self.log {
            log.lock().expect("execution log poisoned").clear();
        }
    }

    /// The label salsa's event log uses for `query` applied to `file`.
    ///
    /// Reconstructed from the input's salsa id the same way salsa formats it,
    /// rather than assuming `FileId(n)` and `Id(n)` happen to agree. It uses
    /// `salsa::plumbing`, which is not covered by salsa's semver guarantee;
    /// if it ever goes away this stops compiling, which is the point.
    ///
    /// # Panics
    /// If the `FileId` was not issued by this database.
    pub fn query_label(&self, query: &str, file: FileId) -> String {
        use salsa::plumbing::AsId;
        format!("{query}({:?})", self.source_file(file).as_id())
    }

    /// How many times `query` executed for `file` since the log was cleared.
    pub fn executions_for(&self, query: &str, file: FileId) -> usize {
        let label = self.query_label(query, file);
        self.execution_log().iter().filter(|e| **e == label).count()
    }

    /// How many times `query` executed, for any key, since the log was
    /// cleared.
    pub fn executions(&self, query: &str) -> usize {
        let prefix = format!("{query}(");
        self.execution_log()
            .iter()
            .filter(|e| e.starts_with(&prefix))
            .count()
    }
}

#[salsa::db]
impl salsa::Database for LinkDatabase {}

#[salsa::db]
impl Db for LinkDatabase {
    fn try_source_file(&self, file: FileId) -> Option<SourceFile> {
        self.files.get(file.0 as usize).copied()
    }

    fn workspace(&self) -> Workspace {
        self.workspace.expect("workspace is created with the database")
    }
}
