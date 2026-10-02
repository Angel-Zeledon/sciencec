//! The package commands: `sciencec new`, and `check` / `build` / `run` /
//! `test` with no file named.
//!
//! `science_package` reads the manifest and walks the graph; this module is
//! the bookkeeping between that and [`Session`] — which directory is the
//! package, which file is the entry, where the executable goes — and nothing
//! about compiling, which is the same pipeline a script goes through.
//!
//! # When a command is a package command
//!
//! **Decision. A command is a package command exactly when no file is named
//! on it.** `sciencec build` builds the package around the working
//! directory; `sciencec build hello.science` builds that file as a script,
//! exactly as it did before there was a manifest, even inside a package.
//!
//! **Reason.** `package-manager.md` Decision 15 rule 1 — *"the file named on
//! the command line"* wins over everything, and §4.6 argues it: *"A command
//! line that can be overruled by a configuration file is not a command
//! line."* Going further — giving a named file the dependencies of the
//! manifest above it, which §4.6's *"single-file package"* sentence leans
//! toward — would change what every file under a manifest means. That is
//! not hypothetical: this repository's root carries an untracked
//! `science.toml`, and every `sciencec build examples/…` the corpus runs
//! would have started writing a `science.lock` into the checkout.
//!
//! **Cost.** A script inside a package cannot `use` the package's
//! dependencies by being named. `--module-path` reaches a directory of
//! modules from a script; a dependency is reached by building the package.
//!
//! # Where things go
//!
//! - The executable: `target/<name>` (`target/<name>.exe` on Windows) under
//!   the package root. §1.2 makes the build directory per project, and a
//!   package has a root to put it in, which a script did not — the reason
//!   [`Session::build`] writes beside the source is gone here.
//! - A test's executable: `target/tests/<stem>`.
//! - `science.lock`: beside `science.toml`, written before compiling and only
//!   when its text changes (`science_package::lock`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use science_diagnostics::{Code, Diagnostic, Label};
use science_package::{graph, lock, manifest, Graph};

use crate::driver::{BuildFlags, Session};

/// The package around the working directory, read, walked and mounted. The
/// lock is written later, by [`Loaded::write_lock`], once there is something
/// to build.
pub struct Loaded {
    pub graph: Graph,
    /// The working directory, canonical: what [`Loaded::near`] is relative to.
    cwd: PathBuf,
}

impl Loaded {
    /// `path` relative to the working directory — see [`load`] for why.
    fn near(&self, path: &Path) -> PathBuf {
        PathBuf::from(science_package::search::relative(&self.cwd, path))
    }

    pub fn root(&self) -> &Path {
        &self.graph.root().root
    }

    /// `target/` under the package root.
    pub fn target(&self) -> PathBuf {
        self.root().join("target")
    }

    /// Decision 15, rules 2 to 4: `[package] entry`, else `src/main.science`,
    /// else a library — which `build` cannot make an executable of, `SP0064`.
    pub fn entry(&self, session: &mut Session) -> Option<PathBuf> {
        let package = self.graph.root();
        if let Some((entry, span)) = &package.manifest.entry {
            let path = package.root.join(entry);
            if path.is_file() {
                return Some(self.near(&path));
            }
            session.report_all(vec![Diagnostic::error(
                Code::sp(64),
                format!("the entry file `{entry}` does not exist"),
            )
            .with_label(Label::primary(*span, "`[package] entry` names this file"))
            .with_note("`entry` is relative to the package root, the directory holding `science.toml`")]);
            return None;
        }
        let default = package.root.join("src").join("main.science");
        if default.is_file() {
            return Some(self.near(&default));
        }
        session.report_all(vec![Diagnostic::error(
            Code::sp(64),
            format!("the package `{}` has no entry file", package.manifest.name),
        )
        .with_label(Label::primary(package.manifest.name_span, "this package"))
        .with_note("the entry is `[package] entry` when it is set, and `src/main.science` otherwise (Decision 15)")
        .with_note(
            "a package with neither is a library: it can be a dependency, and it has no program to build or run",
        )]);
        None
    }
}

/// Finds, reads and walks the package around the working directory, mounts
/// every package in it on `session`. `None` when anything stopped it, and
/// the reason has been reported.
pub fn load(session: &mut Session) -> Option<Loaded> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let Some(root) = graph::find_root(&cwd) else {
        session.report_all(vec![Diagnostic::error(
            Code::sp(60),
            format!("no `science.toml` in `{}` or any directory above it", cwd.display()),
        )
        .with_note("with no file named, this command builds the package around the working directory")
        .with_note("`sciencec new NAME` creates a package; `sciencec build FILE` builds one file as a script")]);
        return None;
    };
    // **Every path handed onward is relative to the working directory**, `..`
    // included. The graph is walked over canonical paths so that two spellings
    // of one directory are one package; but a diagnostic in a sibling
    // package's manifest or source would then print `/home/…/util/…`, which
    // is a different line on every machine and the opposite of what
    // `display_path` exists for. `../util/science.toml` is the path the user
    // would type, and the process opens it from the same directory.
    let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let near = |path: &Path| PathBuf::from(science_package::search::relative(&cwd, path));
    let graph = match graph::load(&root, &mut |path, text| session.register(&near(path), text)) {
        Ok(graph) => graph,
        Err(diagnostics) => {
            session.report_all(diagnostics);
            return None;
        }
    };
    // Every package is mounted at its name, the root included: Decision 13
    // mounts a package's `modules` at its name, and a package's own tests
    // reach it that way.
    session.set_mounts(graph.packages.iter().map(|p| (p.manifest.name.clone(), near(&p.modules()))).collect());
    Some(Loaded { graph, cwd })
}

/// Mounts the package around `files` when they are inside one, so that
/// `sciencec test tests/greeting.science` (or `check`, `build`, `run` with a
/// file) resolves `use NAME.module` the way the package-wide form does.
///
/// **Silent when there is nothing to mount.** A file with no `science.toml`
/// above it, a file outside the package its directory sits under, and a
/// manifest that does not load are all a standalone file, exactly as before:
/// the package commands are where a broken manifest is reported, and a script
/// beside one must not start failing for it.
pub fn mount_around(session: &mut Session, files: &[PathBuf]) {
    let Some(first) = files.first() else { return };
    let Ok(file) = std::fs::canonicalize(first) else { return };
    let Some(root) = file.parent().and_then(graph::find_root) else { return };
    if files.iter().any(|f| !std::fs::canonicalize(f).is_ok_and(|f| f.starts_with(&root))) {
        return;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    let near = |path: &Path| PathBuf::from(science_package::search::relative(&cwd, path));
    let Ok(graph) = graph::load(&root, &mut |path, text| session.register(&near(path), text)) else {
        return;
    };
    session.set_mounts(graph.packages.iter().map(|p| (p.manifest.name.clone(), near(&p.modules()))).collect());
}

impl Loaded {
    /// Writes `science.lock`, once the command has found something to
    /// compile.
    ///
    /// **After the entry is found, not as soon as the manifest reads.** A
    /// command that stops at `SP0064` has built nothing, and a lockfile is a
    /// record of what a build was made from. The ordering also has a
    /// concrete payoff: `sciencec build` typed anywhere under a directory
    /// whose `science.toml` is somebody's experiment — this repository's
    /// root has one, untracked, with no entry — fails without writing a file
    /// into their checkout.
    fn write_lock(&self, session: &mut Session) -> bool {
        let text = lock::render(&self.graph, env!("CARGO_PKG_VERSION"));
        match lock::write(self.root(), &text) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("error: cannot write `{}`: {error}", self.root().join(lock::LOCK).display());
                session.fail();
                false
            }
        }
    }
}

fn executable(dir: &Path, stem: &str) -> PathBuf {
    let path = dir.join(stem);
    if cfg!(windows) { path.with_extension("exe") } else { path }
}

/// Creates `dir`, or reports why not.
fn make_dir(session: &mut Session, dir: &Path) -> bool {
    match std::fs::create_dir_all(dir) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("error: cannot create `{}`: {error}", dir.display());
            session.fail();
            false
        }
    }
}

/// `sciencec check`, in a package: the entry, as a script would be checked.
pub fn check(session: &mut Session) {
    let Some(loaded) = load(session) else { return };
    if let Some(entry) = loaded.entry(session) {
        if loaded.write_lock(session) {
            session.check(&[entry]);
        }
    }
}

/// `sciencec build`, in a package. The executable's path when it was built.
pub fn build(session: &mut Session, flags: BuildFlags) -> Option<PathBuf> {
    let loaded = load(session)?;
    let entry = loaded.entry(session)?;
    if !loaded.write_lock(session) {
        return None;
    }
    let target = loaded.target();
    if !make_dir(session, &target) {
        return None;
    }
    let output = executable(&target, &loaded.graph.root().manifest.name);
    session.build_entry(&entry, &output, flags).then_some(output)
}

/// `sciencec test`, in a package: every `tests/*.science`, in name order, each
/// built as its own entry against the package and run once.
///
/// **A test file is an entry, like `sciencec test FILE`**, and the reason is
/// that command's: there is no `test` item yet, so a program is a test and
/// its exit code is the verdict. What a package adds is that each test is
/// built with the package mounted at its name, so `tests/parse.science` can
/// `use spectra.parse` and test the library as a dependent would see it.
pub fn test(session: &mut Session, flags: BuildFlags) {
    let Some(loaded) = load(session) else { return };
    let dir = loaded.root().join("tests");
    let mut tests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| loaded.near(&entry.path()))
                .filter(|path| path.is_file() && path.extension().is_some_and(|e| e == "science"))
                .collect()
        })
        .unwrap_or_default();
    tests.sort();
    if tests.is_empty() {
        println!("no tests: `tests/` holds no `.science` file");
        return;
    }
    let target = loaded.target().join("tests");
    if !loaded.write_lock(session) || !make_dir(session, &target) {
        return;
    }
    for test in &tests {
        let stem = test.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let output = executable(&target, &stem);
        if session.build_entry(test, &output, flags) {
            session.run_test(test, &output);
        }
    }
}

/// `sciencec run [FILE] [-- ARGS...]`: build, then run with `args`.
///
/// The exit code is the program's when it ran, so `sciencec run` is
/// transparent in a shell pipeline; `None` when it did not, and the session
/// says why.
pub fn run(session: &mut Session, file: Option<&Path>, args: &[OsString], flags: BuildFlags) -> Option<u8> {
    let exe = match file {
        // Decision 15 rule 1: `sciencec run scripts/plot.science` runs that
        // file, built beside itself as `sciencec build` would.
        Some(file) => {
            let output = if cfg!(windows) { file.with_extension("exe") } else { file.with_extension("") };
            session.build_entry(file, &output, flags).then_some(output)?
        }
        None => build(session, flags)?,
    };
    session.run_program(&exe, args)
}

/// `sciencec new NAME`: a package in `./NAME`, with an entry that prints a
/// greeting from a module of its own and a test that checks it.
///
/// **What the scaffold teaches** is the one thing about a package a reader
/// cannot guess: a module is reached by the package's name (Decision 13), so
/// the entry and the test both write `use NAME.greeting`, and a dependent
/// package would write the same line.
pub fn new(session: &mut Session, name: &str) {
    if !manifest::is_module_name(name) {
        let mut diagnostic = Diagnostic::error(Code::sp(4), format!("`{name}` is not a legal package name"))
            .with_note(
                "a package name is its root module's name (Decision 13), so it is `[a-z][a-z0-9]*`: \
                 no hyphens, no underscores, no dots, no capitals",
            );
        if let Some(fixed) = manifest::normalised(name) {
            diagnostic = diagnostic.with_note(format!("`{fixed}` is the same name, made legal"));
        }
        session.report_all(vec![diagnostic]);
        return;
    }
    let root = PathBuf::from(name);
    if root.exists() {
        eprintln!("error: `{name}` already exists; `sciencec new` creates a directory and never writes into one");
        session.fail();
        return;
    }
    let files: [(&str, String); 5] = [
        (
            "science.toml",
            format!("[package]\nname     = \"{name}\"\nversion  = \"0.1.0\"\nlanguage = \"0.1\"\n\n[dependencies]\n"),
        ),
        (
            "src/main.science",
            format!("use {name}.greeting (greeting)\n\ndef main():\n    print(greeting())\n"),
        ),
        (
            "src/greeting.science",
            "public def greeting() -> String:\n    \"Hello, world!\"\n".to_string(),
        ),
        (
            "tests/greeting.science",
            format!(
                "use {name}.greeting (greeting)\n\ndef main():\n    assert(greeting() is \"Hello, world!\", \"the greeting changed\")\n"
            ),
        ),
        (".gitignore", "/target\n".to_string()),
    ];
    for (path, text) in &files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            if !make_dir(session, parent) {
                return;
            }
        }
        if let Err(error) = std::fs::write(&path, text) {
            eprintln!("error: cannot write `{}`: {error}", path.display());
            session.fail();
            return;
        }
    }
    println!("created package `{name}`: `cd {name}` and `sciencec run`");
}
