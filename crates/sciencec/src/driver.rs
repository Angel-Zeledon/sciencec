//! One compilation session: the database, the files put into it, and the
//! commands run over them.
//!
//! Everything here goes through `science-db`. §3 of the core spec makes the
//! query database the mechanism the compiler is built from, so the driver asks
//! it for tokens and syntax trees rather than calling `science_lexer::lex` and
//! `science_parser::parse_module` itself — which also means a future
//! `sciencec watch` gets incrementality for free instead of having to be
//! rewritten onto the database first.
//!
//! The one phase not reached through a query is resolution: `science-db` has
//! no `resolve` query yet (`pending::hir` is still `unimplemented!()`), so
//! [`Session::resolved`] calls `science_resolve::resolve_module` on the tree
//! the `ast` query returned. When that query lands this is the only place that
//! changes.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use science_db::ScienceDatabase;
use science_diagnostics::{Diagnostic, FileId};
use science_parser::Dump as _;
use science_resolve::hir::Crate;

use crate::report::{emit, Tally};
use crate::Outcome;

pub struct Session {
    db: ScienceDatabase,
    tally: Tally,
}

impl Session {
    pub fn new() -> Self {
        Session { db: ScienceDatabase::new(), tally: Tally::default() }
    }

    /// Prints the summary and reports what the process should exit with.
    pub fn finish(self) -> Outcome {
        if let Some(summary) = self.tally.summary() {
            eprintln!("{summary}");
        }
        if self.tally.failed() {
            Outcome::Failed
        } else {
            Outcome::Clean
        }
    }

    // --- commands ---------------------------------------------------------

    /// `sciencec check FILE...`
    ///
    /// Every file is read first, then every file that was read is checked.
    /// A file that fails does not stop the ones after it: the recovery
    /// discipline in the lexer, the parser and the resolver exists so that one
    /// broken file does not hide the next, and a driver that stopped at the
    /// first failure would throw that away.
    pub fn check(&mut self, paths: &[PathBuf]) {
        // A path named twice is one file — `add_file` hands the same `FileId`
        // back — and checking it twice would double its diagnostics and its
        // contribution to the summary.
        let mut files: Vec<FileId> = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(file) = self.load(path) {
                if !files.contains(&file) {
                    files.push(file);
                }
            }
        }

        let mut all = Vec::new();
        for &file in &files {
            all.extend(self.diagnostics(file));
        }
        self.tally.count(&all);
        // Rendered in one batch so the output is ordered by file and position
        // rather than by the order the phases happened to run in; `render_all`
        // does that sorting, and `FileId`s are handed out in command-line
        // order, so files come out in the order they were named.
        emit(&self.db.source_map(), &all);
    }

    /// `sciencec build FILE...`
    ///
    /// The front end first, then the back end, and the back end is not reached
    /// when the front end reported an error — a build reports the *program's*
    /// problems before the toolchain's, because a user whose file does not
    /// parse is not helped by being told which LLVM to install.
    ///
    /// **What it does today is report `SC0400`.** `codegen-and-linking.md` §11
    /// defines that code as *"a toolchain feature required to build this
    /// program is not compiled into this `sciencec`; names the feature and how
    /// to obtain a build that has it"*, and that is the literal situation: the
    /// LLVM backend lives in `science-codegen-llvm`, which needs `llvm-sys`,
    /// which needs an LLVM 18.1 installation. `science-codegen`'s own module
    /// documentation says what is missing and what installs it.
    ///
    /// The command exists anyway, rather than waiting for the backend, for the
    /// reason the note gives for spending a diagnostic code on this at all: a
    /// compiler that answers `unknown command \`build\`` is indistinguishable
    /// from one that was never going to have it, and a compiler that answers
    /// with `SC0400` has a missing feature it can name.
    ///
    /// The decision-making is `science_codegen::driver`'s, not this method's.
    /// `sciencec`'s manifest argues that the driver is *"a call into an
    /// existing crate plus the bookkeeping a shell needs"*, and which backends
    /// exist, what `SC0400` says and how to name the missing package are not
    /// bookkeeping.
    pub fn build(&mut self, paths: &[PathBuf]) {
        self.check(paths);
        if self.tally.failed() {
            return;
        }
        let inputs = paths.iter().map(|p| display_path(p)).collect();
        let request = science_codegen::driver::BuildRequest::new(inputs);
        if let Err(diagnostics) = science_codegen::driver::build(&request) {
            self.report(diagnostics.into_vec());
        }
    }

    /// `sciencec fmt FILE...`, and `sciencec fmt --write FILE...`
    ///
    /// A file is formatted only when it lexes and parses clean. Resolution is
    /// deliberately *not* required: a formatter works on syntax, so syntax is
    /// what it may demand, and `examples/17_modules.science` — which imports
    /// modules that are not files in this repository — is a perfectly
    /// well-formed program that no single-file `check` can resolve. A file
    /// with a syntax error is a different matter: its token stream is a guess,
    /// and reformatting a guess is how a formatter eats a program.
    ///
    /// Without `--write` the formatted text goes to stdout, like every other
    /// dump. With it, a file is rewritten only when the text actually changed,
    /// so a `fmt --write` over a clean tree leaves every timestamp alone.
    pub fn format(&mut self, paths: &[PathBuf], write: bool) {
        let mut files: Vec<FileId> = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(file) = self.load(path) {
                if !files.contains(&file) {
                    files.push(file);
                }
            }
        }

        for file in files {
            let syntax = science_db::file_diagnostics(&self.db, file).to_vec();
            let broken =
                syntax.iter().any(|d| d.severity == science_diagnostics::Severity::Error);
            self.report(syntax);
            if broken {
                continue;
            }

            // The source is copied out of the database so that reporting,
            // which needs the session, does not hold a borrow of it.
            let source = science_db::source_text(&self.db, file).to_string();
            let formatted = science_fmt::format_source(file, &source);
            // Reported whether or not the file formatted: an unmatched
            // `# fmt: off` is a warning on a file the formatter still hands
            // back, and swallowing it would let the marker silently swallow
            // the rest of the file with it.
            let text = formatted.text;
            self.report(formatted.diagnostics.into_vec());
            let Some(text) = text else { continue };
            if !write {
                print(&text);
                continue;
            }
            if text == source {
                continue;
            }
            let path = self.db.path(file).to_string();
            if let Err(error) = std::fs::write(&path, text.as_bytes()) {
                eprintln!("error: cannot write `{path}`: {}", io_reason(&error));
                self.tally.error();
            }
        }
    }

    /// `sciencec tokens FILE`
    ///
    /// The line format is the lexer's own snapshot dump — `start..end  kind`,
    /// in `crates/science-lexer/tests/common/mod.rs` — so a stream printed
    /// here and a stream in a snapshot can be diffed against each other.
    pub fn dump_tokens(&mut self, path: &Path) {
        let Some(file) = self.load(path) else { return };
        let lexed = science_db::tokens(&self.db, file);
        let mut out = String::new();
        for token in lexed.value() {
            out.push_str(&format!(
                "{:>4}..{:<4}  {:?}\n",
                token.span.start, token.span.end, token.kind
            ));
        }
        print(&out);
        self.report(lexed.diagnostics().to_vec());
    }

    /// `sciencec ast FILE`, in `science-parser`'s `Dump` format.
    pub fn dump_ast(&mut self, path: &Path) {
        let Some(file) = self.load(path) else { return };
        let parsed = science_db::ast(&self.db, file);
        print(&parsed.value().dump());
        self.report(science_db::file_diagnostics(&self.db, file).to_vec());
    }

    /// `sciencec resolve FILE`, in `science-resolve`'s dump format.
    ///
    /// Nothing is dumped when the file has lexical or syntax errors, for the
    /// same reason `check` does not resolve one: the definitions of a file
    /// that did not parse are a report about the recovery, not about the
    /// program.
    pub fn dump_resolved(&mut self, path: &Path) {
        let Some(file) = self.load(path) else { return };
        let syntax = science_db::file_diagnostics(&self.db, file).to_vec();
        if syntax.iter().any(|d| d.severity == science_diagnostics::Severity::Error) {
            self.report(syntax);
            return;
        }
        let (krate, diagnostics) = self.resolved(file);
        print(&science_resolve::dump::dump_crate(&krate));
        let mut all = syntax;
        all.extend(diagnostics);
        self.report(all);
    }

    /// `sciencec tools --json FILE`, the `tools/list` array of
    /// `mcp-servers.md` §14.3.
    ///
    /// It resolves and then walks, for the reason that note gives for building
    /// this before anything else: the schema is derived from the signature, so
    /// the derivation is the claim, and it can be checked years before there
    /// is a server to run it in.
    ///
    /// What it walks is the file's `tool` declarations. `every_function` is
    /// `--all-functions`, which restores the walk this pass had before the
    /// keyword existed; `crate::tools` argues why that is a flag and not the
    /// default.
    ///
    /// Errors stop it. A schema derived from a program that does not resolve
    /// would be a schema for a program that does not exist, and emitting one
    /// is worse than emitting nothing — a caller has no way to tell the two
    /// apart.
    pub fn dump_tools(&mut self, path: &Path, every_function: bool) {
        let Some(file) = self.load(path) else { return };
        let syntax = science_db::file_diagnostics(&self.db, file).to_vec();
        if syntax.iter().any(|d| d.severity == science_diagnostics::Severity::Error) {
            self.report(syntax);
            return;
        }
        let (krate, diagnostics) = self.resolved(file);
        if diagnostics.iter().any(|d| d.severity == science_diagnostics::Severity::Error) {
            let mut all = syntax;
            all.extend(diagnostics);
            self.report(all);
            return;
        }
        if let Some(module) = krate.modules.first() {
            // The word a function was declared with lives in the syntax tree,
            // so the predicate is read from there and the schema from the
            // resolved one. Both describe the same file; `ast` is a query and
            // hands back what it already parsed.
            let spans = if every_function {
                std::collections::HashSet::new()
            } else {
                crate::tools::tool_spans(science_db::ast(&self.db, file).value())
            };
            let walk = if every_function {
                crate::tools::Walk::EveryFunction
            } else {
                crate::tools::Walk::Tools(&spans)
            };
            print(&crate::tools::render(&krate, module, &walk));
        }
        let mut all = syntax;
        all.extend(diagnostics);
        self.report(all);
    }

    // --- the pipeline -----------------------------------------------------

    /// Everything the front half reports about one file, in pipeline order.
    ///
    /// Each phase is skipped when an earlier one found an error, for the same
    /// reason every time: the phases recover rather than aborting, so there
    /// *is* something to hand on, but it is made of error nodes, and the next
    /// phase would report each of them again under its own name. A cascade
    /// buries the one diagnostic the reader needs.
    ///
    /// Concretely: the parser's error nodes come back as unresolved names, and
    /// the resolver's unresolved names come back as `Ty::ERROR`, which `ty`'s
    /// §5 makes agree with everything — so a file that failed to resolve would
    /// type-check *silently and wrongly*, which is worse than not checking it.
    fn diagnostics(&self, file: FileId) -> Vec<Diagnostic> {
        let mut all = science_db::file_diagnostics(&self.db, file).to_vec();
        if has_error(&all) {
            return all;
        }
        let (krate, resolution) = self.resolved(file);
        all.extend(resolution);
        if has_error(&all) {
            return all;
        }
        all.extend(type_check(&krate));
        all
    }

    /// The resolved crate for one file.
    ///
    /// A single file is a crate of one module, which is what §4.2 of
    /// `script-mode.md` describes: the file named on the command line is the
    /// entry. `resolve_module` is given the path the file was registered
    /// under, because that path is what places it in the module tree.
    fn resolved(&self, file: FileId) -> (Crate, Vec<Diagnostic>) {
        let parsed = science_db::ast(&self.db, file);
        let path = self.db.path(file);
        let (krate, diagnostics) = science_resolve::resolve_module(file, path, parsed.value());
        (krate, diagnostics.into_vec())
    }

    // --- files ------------------------------------------------------------

    /// Reads `path` and registers it with the database, or reports why it
    /// could not and returns `None`.
    fn load(&mut self, path: &Path) -> Option<FileId> {
        let name = display_path(path);
        match read_source(path, &name) {
            Ok(text) => Some(self.db.add_file(name, text)),
            Err(message) => {
                eprintln!("error: {message}");
                self.tally.error();
                None
            }
        }
    }

    fn report(&mut self, diagnostics: Vec<Diagnostic>) {
        self.tally.count(&diagnostics);
        emit(&self.db.source_map(), &diagnostics);
    }
}

/// Writes a dump to stdout.
///
/// `print!` panics when stdout is a closed pipe — `sciencec ast big.science |
/// head` is an ordinary thing to type — so the error is swallowed instead.
fn print(text: &str) {
    let stdout = std::io::stdout();
    let _ = stdout.lock().write_all(text.as_bytes());
}

/// How a path is spelled in a diagnostic's `-->` header.
///
/// Relative to the working directory when it is under it, and always with
/// forward slashes. That is what `crates/science-lexer/tests/ui.rs` registers
/// and what the `.stderr` expectations pin: a Windows path in the header would
/// make the same program produce different output on two machines, and a
/// diagnostic that is not reproducible is not comparable.
fn display_path(path: &Path) -> String {
    let relative = std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf));
    let shown = relative.as_deref().unwrap_or(path);
    shown.to_string_lossy().replace('\\', "/")
}

/// Reads a source file, or explains what stopped it.
///
/// Each of the three ways this fails is checked for by hand rather than left
/// to the operating system, because the driver's job is to fail cleanly:
///
/// * a missing file, whose OS message differs per platform;
/// * a directory, which on Linux reads as "Is a directory" and on Windows as
///   "Access is denied", neither of which says what the user did wrong;
/// * bytes that are not UTF-8, which cannot even reach the lexer — §2 of the
///   core spec makes source UTF-8, and every span in the compiler is a byte
///   offset into a `str`.
fn read_source(path: &Path, name: &str) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("cannot read `{name}`: {}", io_reason(&e)))?;
    if metadata.is_dir() {
        return Err(format!("`{name}` is a directory, not a source file"));
    }
    let bytes =
        std::fs::read(path).map_err(|e| format!("cannot read `{name}`: {}", io_reason(&e)))?;
    String::from_utf8(bytes).map_err(|e| {
        let at = e.utf8_error().valid_up_to();
        format!("`{name}` is not valid UTF-8: the byte at offset {at} does not begin a character")
    })
}

/// A platform-independent phrase for the errors a driver actually meets.
fn io_reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no such file".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    }
}

/// Whether anything here stops the pipeline.
fn has_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == science_diagnostics::Severity::Error)
}

/// Type-checks one resolved crate and returns what it found.
///
/// **Why this is here at all.** Until this function existed the driver ran the
/// lexer, the parser and the resolver, and stopped. `science-types` had a type
/// checker with THIR, bidirectional checking, flow narrowing, `SC0140`, method
/// lookup and two coercions, tested by two hundred and sixty tests — and not
/// one of its diagnostics could reach a person running `sciencec`. A phase
/// nobody can invoke is a phase that does not exist for the user, however well
/// it is tested.
///
/// **The order is fixed and it is not this function's to choose.** `Aliases`
/// before `Declarations` because a declaration's annotation may name an alias;
/// both before `check_crate` because a body is checked against signatures.
/// `AtomOrder` first of all, because it is what makes a const expression's
/// normal form reproducible — `normal.rs` §2, and the reason it is built from
/// the `DefTable` rather than from `DefId`s.
///
/// **The cost.** Every table is rebuilt per file. That is right for `check`,
/// which is a batch command over files named on one command line, and wrong
/// for a language server, which wants them memoised per crate — that is what
/// `science-db`'s query layer is for, and wiring these stages into it is the
/// work this function stands in for. `science_db::pending` still has
/// `unimplemented!` for every stage from HIR onward; this is the shortcut, and
/// it is named as one so nobody mistakes it for the architecture.
fn type_check(krate: &Crate) -> Vec<Diagnostic> {
    let order = science_types::AtomOrder::of(&krate.defs);
    let mut types = science_types::Types::new();
    let mut diagnostics = science_diagnostics::Diagnostics::new();
    let mut aliases = science_types::Aliases::of(krate, &mut types, &order, &mut diagnostics);
    let decls = science_types::Declarations::of(krate, &mut types, &order, &mut diagnostics);
    science_types::check_crate(
        krate,
        &decls,
        &mut types,
        &mut aliases,
        &order,
        &mut diagnostics,
    );
    diagnostics.into_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relative_path_keeps_its_spelling_but_loses_backslashes() {
        let native = Path::new("examples/01_functions.science");
        assert_eq!(display_path(native), "examples/01_functions.science");
        let windows = Path::new("tests\\ui\\bad_escape.science");
        assert_eq!(display_path(windows), "tests/ui/bad_escape.science");
    }

    #[test]
    fn a_path_under_the_working_directory_is_shown_relative_to_it() {
        let cwd = std::env::current_dir().expect("a working directory");
        let absolute = cwd.join("examples").join("01_functions.science");
        assert_eq!(display_path(&absolute), "examples/01_functions.science");
    }

    #[test]
    fn a_missing_file_is_named_the_way_it_was_typed() {
        let message = read_source(Path::new("no/such/file.science"), "no/such/file.science")
            .expect_err("the file does not exist");
        assert_eq!(message, "cannot read `no/such/file.science`: no such file");
    }

    #[test]
    fn a_directory_is_rejected_before_it_is_read() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let name = "crates/sciencec";
        let message = read_source(dir, name).expect_err("a directory is not a source file");
        assert_eq!(message, "`crates/sciencec` is a directory, not a source file");
    }
}
