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
//! The phases not reached through a query are everything from resolution down.
//! `science-db` has a query declared for each of them and a body for none:
//! `pending::hir`, `pending::thir`, `pending::mir`, `pending::call_graph` and
//! `pending::region_result` are all still `unimplemented!()`. So
//! [`Session::resolved`] calls `science_resolve::resolve_crate` on the trees
//! the `ast` query returned, and [`type_and_region_check`] calls the three
//! crates below it directly.
//!
//! **That is a shortcut and it is named as one at each of the two functions
//! that take it.** It was one phase's worth of shortcut when the type checker
//! was wired in and it is three phases' worth now, which is the argument for
//! the queries rather than against the wiring: a phase nobody can run is worth
//! nothing, and the day `pending::mir` has a body these two functions are the
//! only places that change.
//!
//! # A file on the command line is a crate, not a module
//!
//! [`Session::crate_sources`] is where `use` becomes a file read. The file
//! named is the entry (`script-mode.md` §4.2 rule 1), the directory it sits in
//! is the crate root, and every module its `use` declarations reach — and
//! every module *those* reach — is loaded, parsed through the same `ast`
//! query, and handed to the resolver in one slice.
//!
//! **This is the third thing the shortcut now costs.** A crate's syntax trees
//! are cloned out of the database once per entry that reaches them, because
//! `SourceModule` owns its tree; `science_db::pending::hir` is where a crate's
//! module list would be a query and none of that would happen twice. The
//! header above says the day that query has a body, two functions change.
//! There are three now.

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
    ///
    /// # What several files mean, now that a crate can be more than one
    ///
    /// **Decision. `sciencec check a.science b.science` is still two crates,
    /// not one.** Each file named is an entry, and each entry brings in the
    /// modules its `use` declarations reach. What changed is what one name on
    /// the command line expands to; what did not change is what two names
    /// mean.
    ///
    /// **Reason, and it is not conservatism.** `script-mode.md` §4.2 rule 1
    /// makes *the file named on the command line* the entry, and one crate has
    /// one entry — §4.3 is written in terms of *"the entry"*, singular, and
    /// `SC0213` is the difference between a script and a module. Folding the
    /// command line into one crate would make every file after the first a
    /// non-entry, so `sciencec check *.science` over a directory of analysis
    /// scripts would report `SC0213` on all but one of them. That is not a
    /// stricter reading of the note; it is the opposite of what the note is
    /// for.
    ///
    /// **Cost, and it is paid here.** A module reached from two entries named
    /// on one command line is checked twice, and its diagnostics arrive twice.
    /// They are deduplicated below rather than left to double, because a
    /// reader counting errors should be counting the program's, not the
    /// driver's traversals. Two *identical* diagnostics are one fact; a
    /// diagnostic that differs between the two crates — `SC0213`, which fires
    /// in the crate that imported the file and not in the crate that is it —
    /// differs, and both are kept.
    pub fn check(&mut self, paths: &[PathBuf]) {
        // A path named twice is one file — `add_file` hands the same `FileId`
        // back — and checking it twice would double its diagnostics and its
        // contribution to the summary.
        let mut entries: Vec<(PathBuf, FileId)> = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(file) = self.load(path) {
                if !entries.iter().any(|(_, seen)| *seen == file) {
                    entries.push((path.clone(), file));
                }
            }
        }

        let mut all: Vec<Diagnostic> = Vec::new();
        for (path, file) in &entries {
            for diagnostic in self.diagnostics(path, *file) {
                // Quadratic in the number of diagnostics one command produces,
                // which is the right trade at this size: the alternative is a
                // hash of a `Diagnostic`, and nothing else in the compiler
                // needs one.
                if !all.contains(&diagnostic) {
                    all.push(diagnostic);
                }
            }
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
    /// **The decision-making is still not this method's.** `science-codegen` is
    /// above Decision 42's line and `science-codegen-llvm` is below it; what
    /// this method contributes is the front end's output and a path to write
    /// to. Which backends exist, what `SC0400` says and how to name the missing
    /// package are [`science_codegen_llvm`]'s, and the call is the same call in
    /// both builds of this crate — see the manifest's `[features]`.
    ///
    /// **Without `--features llvm` it reports `SC0400`, unchanged**, because
    /// that is what a contributor with no LLVM sees and it is the message that
    /// tells them what to do. §11 defines the code as *"a toolchain feature
    /// required to build this program is not compiled into this `sciencec`;
    /// names the feature and how to obtain a build that has it"*, which is the
    /// literal situation.
    ///
    /// # One file is one crate is one executable
    ///
    /// [`Session::check`]'s §"What several files mean" settles that each file
    /// named is its own crate, and `build` follows it rather than inventing a
    /// second rule: **`sciencec build a.science b.science` produces two
    /// executables.** Each goes beside its own source file, named after it
    /// (`hello.science` → `hello.exe` on Windows, `hello` elsewhere).
    ///
    /// **Beside the source and not in the working directory**, which is the one
    /// place this differs from `cc` and from `rustc`. `package-manager.md`
    /// Decision 7 removes environment-dependent inputs from a build's output,
    /// and the working directory is one: `sciencec build examples/hello.science`
    /// would otherwise put `hello.exe` somewhere that depends on where the user
    /// was standing. The cost is that a build writes into the source tree, which
    /// is the wrong default the day there is a `target/` directory to write to
    /// instead — and there is no package manager yet, so there is not one.
    pub fn build(&mut self, paths: &[PathBuf]) {
        let mut entries: Vec<(PathBuf, FileId)> = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(file) = self.load(path) {
                if !entries.iter().any(|(_, seen)| *seen == file) {
                    entries.push((path.clone(), file));
                }
            }
        }
        for (path, file) in &entries {
            let mut all = self.diagnostics(path, *file);
            if has_error(&all) {
                self.report(all);
                continue;
            }
            // The front end runs a second time, and it is not free. `diagnostics`
            // throws its MIR away because `check` has no use for it, and holding
            // it would mean threading `Types` and `Vec<Body>` back out of every
            // command. The day `science_db::pending::mir` has a body, both calls
            // become one query and this paragraph goes with them — that is the
            // same shortcut this module's header names, counted a fourth time.
            match self.emit_executable(path, *file) {
                Ok(()) => {}
                Err(diagnostics) => all.extend(diagnostics),
            }
            self.report(all);
        }
    }

    /// The back half of [`Session::build`] for one entry: MIR, then the backend.
    ///
    /// Split out so that the front end's diagnostics and the back end's are
    /// reported in one batch by the caller, which is what makes a build's output
    /// ordered the way `check`'s is.
    fn emit_executable(&mut self, path: &Path, file: FileId) -> Result<(), Vec<Diagnostic>> {
        let (sources, _) = self.crate_sources(path, file);
        let (krate, _) = self.resolved(&sources);
        let Some(mut lowered) = lower_to_mir(&krate) else {
            // Unreachable in practice: `diagnostics` has already run the same
            // phases and the caller stopped on an error. Kept as a value rather
            // than an `expect`, because "the type checker reported nothing and
            // then refused to hand over a body" is a compiler bug and a panic is
            // the one way to report it that a user cannot act on.
            return Err(vec![science_codegen::diagnostics::no_entry_point(&display_path(path))]);
        };
        // **A file with no entry point is refused here, as `SC0403`, and not by
        // the backend as `SC0400`.**
        //
        // The decision. `build` asks `science-codegen` whether the crate has an
        // entry point before it hands anything to a backend, and reports
        // `SC0403` when it has none.
        //
        // The reason. The two refusals say opposite things about what the
        // reader should do. §11 defines `SC0400` as *"a toolchain feature
        // required to build this program is not compiled into this
        // `sciencec`"*, and its whole obligation is to name *"how to obtain a
        // build that has it"* — it tells the reader to go and install
        // something. `SC0403` is the code §11 added for the other case, quoting
        // the gap it fills: *"`script-mode.md` §4.2 rule 3 defines this
        // situation as a library build and does not say what happens when
        // somebody asks for a binary."* A file of declarations is not waiting
        // on a package; it is not a program, and no version of this compiler
        // will ever build it into one. `examples/20_extern.science` is the
        // corpus's instance — `extern` blocks, a handle type and three wrapper
        // functions, and deliberately no `main` — and it used to be told to
        // upgrade its toolchain. `mono.rs` §7 already claimed this was
        // happening (*"a file with no entry point is a library, and `sciencec
        // build` already refuses it"*) and `SC0403` had no caller; this is the
        // caller.
        //
        // The cost. `build` now runs the entry-point rule twice — once here and
        // once inside `Mono::collect`'s root set — and a build that *does* have
        // a `main` pays one extra scan of the body list for the check that says
        // so. That is the same order as the double front-end run the block
        // above already pays for, and a great deal cheaper.
        if science_codegen::mono::entry_point(
            &krate.defs,
            lowered.bodies.iter().map(science_mir::mir::Body::def),
        )
        .is_none()
        {
            return Err(vec![science_codegen::diagnostics::no_entry_point(&display_path(path))]);
        }
        // **Decision 42's walk, run here and handed down.** The walk needs
        // `&mut Types` — instantiating `identity of T` at `Int` interns types
        // that did not exist before — and `BuildInput::types` is a `&Types`,
        // because a backend must not be able to invent one. So the walk cannot
        // live inside `science-codegen-llvm` even if Decision 42 did not
        // already put it above the line; this is the last place that holds the
        // table mutably.
        //
        // **`RootSet::EntryPoint`**, which is what `sciencec build` means: the
        // set of things the program reaches from `main`. The other root set
        // exists for a library build and this command does not do one — the
        // `SC0403` above is the refusal that says so.
        // **The walk, and then the substitution, in one borrow of `Types`.**
        // `Mono::instantiate` is what turns each named instance into a body a
        // backend can emit, and it interns types that did not exist while the
        // body was still generic — so it needs the same `&mut Types` the walk
        // holds and has to run before the walk is dropped. That is why the two
        // are one block here and one value out of it.
        let (mono, instances) = {
            let mut walk = science_codegen::mono::Mono::new(
                &krate.defs,
                &lowered.decls,
                &mut lowered.types,
                &lowered.bodies,
            );
            let mut set = walk.collect(science_codegen::mono::RootSet::EntryPoint);
            let instances = walk.instantiate(&mut set);
            (set, instances)
        };
        // **The walk's own diagnostics, reported.**
        //
        // `MonoSet::diagnostics` has always been filled and until now nothing
        // read it, which is this codebase's recurring failure and this time
        // it hid a program that ran and printed the wrong answer: two
        // definitions in two modules of one crate mangled to one symbol,
        // `Mono::collect` detected it and pushed `SC0404`, `MonoSet`'s map
        // kept one of the two, and both call sites reached whichever
        // survived. The detection was right. Nobody was listening.
        //
        // Reported here because this is where the walk is run, and as an
        // error that stops the build rather than a warning: a symbol
        // collision means the program the user gets is not the program they
        // wrote, which is the one failure mode worse than not building.
        if !mono.diagnostics().is_empty() {
            return Err(mono.diagnostics().to_vec());
        }
        let request = science_codegen::driver::BuildRequest::new(vec![display_path(path)]);
        // Decision 28 makes the source order of the `library` clauses the link
        // order, so the blocks are collected in source order and handed over as
        // a list rather than a set.
        let externs = science_codegen_llvm::extern_blocks(&krate);
        let input = science_codegen_llvm::BuildInput {
            request: &request,
            defs: &krate.defs,
            types: &lowered.types,
            decls: &lowered.decls,
            externs: &externs,
            bodies: &lowered.bodies,
            mono: &mono,
            instances: &instances,
            output: executable_path(path),
        };
        match science_codegen_llvm::build(&input) {
            Ok(_) => Ok(()),
            Err(diagnostics) => Err(diagnostics.into_vec()),
        }
    }

    /// `sciencec test FILE...`
    ///
    /// Builds each entry exactly as `build` does, then runs the executable it
    /// produced once and reports whether it exited cleanly.
    ///
    /// **There is no `test` item yet.** `stdlib-standard.md` §7 proposes one —
    /// a declaration of its own, skipped in a release build, with a `testing`
    /// module whose `check`/`check_equal` record a failure and let the runner
    /// continue past it — and neither exists. `assert` is the only
    /// verification construct this compiler has, and its failure aborts the
    /// whole process (`TokenKind::Assert`'s decision), so there is no
    /// "continue to the next test" for this command to do: **the program is
    /// the test**, and running it once, to completion or to its first
    /// `assert`, is the whole of what this command can mean until a `test`
    /// item exists to run more than one per file.
    ///
    /// **The verdict is the exit code and nothing else.** `0` is a pass;
    /// anything else is named by [`exit_reason`] and counted as a failure —
    /// including the abort `assert`'s failure path raises, whose status is
    /// platform-defined (`science-rt`'s `panic.rs`: `SIGABRT`/134 on POSIX, `3`
    /// on Windows) and is reported as that code rather than decoded, because
    /// decoding a platform's abort convention is not this driver's job.
    ///
    /// A file that does not build is reported the same way `build` reports
    /// one, and is never run — a program that is not there has no exit code to
    /// judge.
    ///
    /// **A program that never returns is judged too, within [`RUN_BUDGET`].**
    /// `7674487` is why: a `for` loop whose `continue` skipped its own
    /// increment ran forever, in the language's most-written construct, and
    /// the whole suite passed — a hang has no exit code to compare and no
    /// diagnostic to match, so nothing before this asked the one question
    /// that would have caught it. [`run_bounded`] asks it.
    pub fn test(&mut self, paths: &[PathBuf]) {
        let mut entries: Vec<(PathBuf, FileId)> = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(file) = self.load(path) {
                if !entries.iter().any(|(_, seen)| *seen == file) {
                    entries.push((path.clone(), file));
                }
            }
        }
        for (path, file) in &entries {
            let mut all = self.diagnostics(path, *file);
            if has_error(&all) {
                self.report(all);
                continue;
            }
            if let Err(diagnostics) = self.emit_executable(path, *file) {
                all.extend(diagnostics);
                self.report(all);
                continue;
            }
            self.report(all);
            self.run_test(path);
        }
    }

    /// Runs the executable [`Session::emit_executable`] already produced
    /// beside `path`, and prints the one-line verdict `cargo test`'s own
    /// format is borrowed from, since it is a format readers already know.
    ///
    /// Bounded by [`RUN_BUDGET`] rather than `Command::status`'s unconditional
    /// wait — see [`run_bounded`] for why.
    fn run_test(&mut self, path: &Path) {
        let exe = spawnable(&executable_path(path));
        let name = display_path(path);
        match run_bounded(&exe, RUN_BUDGET) {
            Ok(RunOutcome::Exited(status)) if status.success() => {
                print(&format!("test {name} ... ok\n"))
            }
            Ok(RunOutcome::Exited(status)) => {
                print(&format!("test {name} ... FAILED ({})\n", exit_reason(&status)));
                self.tally.error();
            }
            Ok(RunOutcome::TimedOut) => {
                print(&format!(
                    "test {name} ... FAILED: did not exit within {}s and was killed — a hang, \
                     not a wrong answer\n",
                    RUN_BUDGET.as_secs()
                ));
                self.tally.error();
            }
            Err(error) => {
                print(&format!(
                    "test {name} ... FAILED: cannot run `{}`: {}\n",
                    exe.display(),
                    io_reason(&error)
                ));
                self.tally.error();
            }
        }
    }

    /// `sciencec fmt FILE...`, and `sciencec fmt --write FILE...`
    ///
    /// A file is formatted only when it lexes and parses clean. Resolution is
    /// deliberately *not* required: a formatter works on syntax, so syntax is
    /// what it may demand, and `examples/17_modules.science` — which imports
    /// modules that are not files in this repository — is a perfectly
    /// well-formed program that no `check` can resolve, then or now that `use`
    /// loads files. A file with a syntax error is a different matter: its
    /// token stream is a guess, and reformatting a guess is how a formatter
    /// eats a program.
    ///
    /// `fmt` reads exactly the files it was named, and never a module one of
    /// them imports. Formatting a file the user did not ask about — and, with
    /// `--write`, rewriting it — is not something a `use` line should be able
    /// to authorise.
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
    /// **`FILE` is the entry**, and what is dumped is the crate it roots —
    /// every module its `use` declarations reach, in the order they were
    /// loaded, entry first. That is the command's whole value now that a crate
    /// can be more than one file: it is the only way to see the module tree
    /// `use` actually built.
    ///
    /// Nothing is dumped when any file in the crate has lexical or syntax
    /// errors, for the same reason `check` does not resolve one: the
    /// definitions of a file that did not parse are a report about the
    /// recovery, not about the program.
    pub fn dump_resolved(&mut self, path: &Path) {
        let Some(file) = self.load(path) else { return };
        let (sources, whole) = self.crate_sources(path, file);
        let mut syntax = Vec::new();
        for source in &sources {
            syntax.extend(science_db::file_diagnostics(&self.db, source.file).to_vec());
        }
        if !whole || has_error(&syntax) {
            self.report(syntax);
            return;
        }
        let (krate, diagnostics) = self.resolved(&sources);
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
        let (sources, whole) = self.crate_sources(path, file);
        let mut syntax = Vec::new();
        for source in &sources {
            syntax.extend(science_db::file_diagnostics(&self.db, source.file).to_vec());
        }
        if !whole || has_error(&syntax) {
            self.report(syntax);
            return;
        }
        let (krate, diagnostics) = self.resolved(&sources);
        if diagnostics.iter().any(|d| d.severity == science_diagnostics::Severity::Error) {
            let mut all = syntax;
            all.extend(diagnostics);
            self.report(all);
            return;
        }
        // `modules.first()` is the entry: `collect_crate` puts it there and
        // says so. A `tool` in a module the entry imported is not this file's
        // to offer — `mcp-servers.md` §14.3's array is the schema of what
        // *this* program exposes — so the walk stays on the one module.
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
    ///
    /// **The fourth skip — regions after types — is not written here**, and
    /// that is the one deviation. It lives inside [`type_and_region_check`],
    /// because what it guards is the set of tables the type checker has just
    /// finished building, and asking the question out here would mean building
    /// them twice to ask it. That function's §"the ordering rule" states it and
    /// says which way it errs.
    fn diagnostics(&mut self, entry: &Path, file: FileId) -> Vec<Diagnostic> {
        let (sources, whole) = self.crate_sources(entry, file);
        let mut all: Vec<Diagnostic> = Vec::new();
        for source in &sources {
            all.extend(science_db::file_diagnostics(&self.db, source.file).to_vec());
        }
        // The first skip, widened from a file to a crate. A module that could
        // not be read is not in `sources` at all, so resolving would report
        // `SC0202` for a `use` whose real problem has already been named, and
        // then one unresolved name per item that module was going to define.
        if !whole || has_error(&all) {
            return all;
        }
        let (krate, resolution) = self.resolved(&sources);
        all.extend(resolution);
        if has_error(&all) {
            return all;
        }
        all.extend(type_and_region_check(&krate));
        all
    }

    /// The whole crate rooted at one entry file: the entry, plus every module
    /// its `use` declarations reach.
    ///
    /// **The crate root is the directory the entry file sits in**, and every
    /// module path is relative to it. `science_resolve::modules` argues that
    /// choice and says what it leaves open for `package-manager.md`; what this
    /// function adds is the I/O, which that crate does none of.
    ///
    /// Two path spellings meet here and they are not the same string. A file
    /// is *registered* under [`display_path`] — repository-relative, so a
    /// diagnostic reads the same on every machine — and it is *placed in the
    /// module tree* under its path relative to the crate root, because that is
    /// what `module_chain` reads. `examples/text/parser.science` and
    /// `text/parser.science` are the two, in that order.
    ///
    /// The `bool` is false when a file that exists could not be read. A file
    /// that is simply absent is not a failure here: it is `SC0202`, which the
    /// resolver reports against the `use` that asked for it, with the paths it
    /// looked in.
    ///
    /// **Cost.** Every module's syntax tree is cloned out of the database,
    /// once per entry that reaches it, because `SourceModule` owns its tree
    /// and the `ast` query hands back a borrow of the database this function
    /// is still writing to. That is the same shortcut this module's header
    /// names: `pending::hir` is where a crate's modules would be a query and
    /// nothing would be cloned.
    fn crate_sources(
        &mut self,
        entry: &Path,
        file: FileId,
    ) -> (Vec<science_resolve::SourceModule>, bool) {
        let root = entry.parent().unwrap_or(Path::new("")).to_path_buf();
        let name = entry
            .file_name()
            .map(|n| n.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| self.db.path(file).to_string());
        let entry_source = science_resolve::SourceModule {
            file,
            path: name,
            ast: science_db::ast(&self.db, file).value().clone(),
            entry: true,
        };

        let mut whole = true;
        let sources = science_resolve::modules::collect_crate(entry_source, |candidate| {
            let path = root.join(candidate);
            // A missing file is the resolver's to report, against the `use`.
            // Only a file that is there and unusable is reported here.
            if !path.is_file() {
                return None;
            }
            let Some(loaded) = self.load(&path) else {
                whole = false;
                return None;
            };
            Some((loaded, science_db::ast(&self.db, loaded).value().clone()))
        });
        (sources, whole)
    }

    /// The resolved crate.
    ///
    /// `resolve_crate` rather than `resolve_module`, always — a crate of one
    /// file is the same call with a shorter slice, and the driver should not
    /// have two ways of running one phase.
    fn resolved(&self, sources: &[science_resolve::SourceModule]) -> (Crate, Vec<Diagnostic>) {
        let (krate, diagnostics) = science_resolve::resolve_crate(sources);
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

/// How long [`Session::run_test`] waits for the executable before concluding
/// it will never exit, killing it, and reporting that as its own kind of
/// failure.
///
/// Ten seconds against a program this driver just built itself and expects to
/// finish close to instantly. Measured, not guessed: every execution test in
/// this workspace that builds, links and runs a program — including
/// `crates/science-codegen-llvm/tests/methods.rs`'s
/// `a_boxed_value_owning_a_string_is_built_and_freed_ten_thousand_times`, the
/// heaviest one, a real loop run ten thousand times — does all three in under
/// half a second end to end on this machine. Twenty times that leaves room for
/// a slow or loaded machine without leaving room for a program that is
/// actually stuck, which is what a false positive here would have to mean.
const RUN_BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// What [`run_bounded`] found when the wait ended.
enum RunOutcome {
    /// The child exited on its own, within the budget.
    Exited(std::process::ExitStatus),
    /// The deadline passed first. The child has already been killed and
    /// reaped — there is no zombie left behind — and it is the caller's job
    /// to decide what to say about it.
    TimedOut,
}

/// Runs `exe` and waits for it, but not forever.
///
/// `Command::status` blocks on `waitpid` with no way to give up, which is
/// exactly the shape of the defect `7674487` shipped: a `for` loop whose
/// `continue` jumped over its own increment ran forever, and the unconditional
/// wait this replaces is what would have turned that program into a suite
/// that never finishes, the day `test` was pointed at it instead of an
/// execution test with its own budget. Polling
/// [`std::process::Child::try_wait`] against a deadline, rather than blocking
/// on [`std::process::Child::wait`], is what turns a hang into a *reported*
/// failure instead of this call blocking too.
fn run_bounded(exe: &Path, budget: std::time::Duration) -> std::io::Result<RunOutcome> {
    let mut child = std::process::Command::new(exe).spawn()?;
    let deadline = std::time::Instant::now() + budget;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(RunOutcome::Exited(status));
        }
        if std::time::Instant::now() >= deadline {
            // Best-effort: the process is already misbehaving, and a kill or a
            // reap failing here is not this function's failure to report —
            // `TimedOut` is the true answer regardless.
            let _ = child.kill();
            let _ = child.wait();
            return Ok(RunOutcome::TimedOut);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A platform-independent phrase for the errors a driver actually meets.
fn io_reason(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::NotFound => "no such file".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => error.to_string(),
    }
}

/// Why [`Session::run_test`]'s child process ended, for the one line it
/// prints about a failure.
///
/// An exit code exists on every platform this compiler targets, so it is
/// tried first. What has none is a process a POSIX signal killed outright —
/// `process::abort()`, which `science-rt`'s panic path calls, is exactly
/// that — and there `ExitStatus::code()` is `None` by construction; the
/// signal number is what `ExitStatusExt` gives instead, and Windows has no
/// such extension because it has no such signal.
fn exit_reason(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    "terminated".to_string()
}

/// Whether anything here stops the pipeline.
fn has_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == science_diagnostics::Severity::Error)
}

/// Type-checks one resolved crate, then region-checks it, and returns what
/// both found.
///
/// **Why this is here at all.** Until this function existed the driver ran the
/// lexer, the parser and the resolver, and stopped. `science-types` had a type
/// checker with THIR, bidirectional checking, flow narrowing, `SC0140`, method
/// lookup and two coercions, tested by two hundred and sixty tests — and not
/// one of its diagnostics could reach a person running `sciencec`. A phase
/// nobody can invoke is a phase that does not exist for the user, however well
/// it is tested. `science-regions` was in exactly that state one day later,
/// with `region-inference.md`'s eleven decisions implemented and its acceptance
/// case passing, and [`region_check`] is the second half of the same argument.
///
/// **The order is fixed and it is not this function's to choose.** `Aliases`
/// before `Declarations` because a declaration's annotation may name an alias;
/// both before `check_crate` because a body is checked against signatures.
/// `AtomOrder` first of all, because it is what makes a const expression's
/// normal form reproducible — `normal.rs` §2, and the reason it is built from
/// the `DefTable` rather than from `DefId`s. MIR then regions, because regions
/// are a dataflow over a CFG and THIR is not one — `science-mir`'s §5 is the
/// seam and it is addressed to exactly one caller.
///
/// # The ordering rule for regions
///
/// **Decision. Regions run only when the type checker reported no error, and
/// the skip is here rather than in [`Session::diagnostics`] because this is
/// where the tables it guards are still alive.**
///
/// **Reason.** `diagnostics`'s rule sharpens at every step, and this is the
/// next sharpening. MIR lowered from a body the checker could not type is MIR
/// over `Ty::ERROR` places, and a region solver handed those errs **both ways
/// at once**:
///
/// * *Silently.* A local whose type mentions `Ty::ERROR` has no reference in
///   it to find, so `science-regions`'s `regions` §1 gives it no region
///   variable at all. The reference is gone rather than unconstrained, the
///   constraints that should have tied it to something are never emitted, and
///   the check under-approximates — it loses errors.
/// * *Loudly, and this is the half that decides it.* A call the checker
///   rejected is still a call in MIR. When it is a method the receiver does not
///   have, `science-mir`'s `lower` lowers it to a `Callee::Unresolved`, and
///   `science-regions`'s `generate` §5 assumes an opaque callee *"returns a
///   reference into every argument it was given"*. §7 of that module states
///   which way that assumption points — **toward rejecting** — so a type error
///   does not merely hide region errors, it **manufactures** them, at spans in
///   code that was never the mistake.
///
/// The second bullet is measured, not feared. With this skip removed,
///
/// ```text
/// def lookup(t: borrowed Table, k: borrowed String) -> borrowed Table:
///     t.missing(k)
/// ...
///     let r be lookup(t, "host")
/// ```
///
/// reports `SC0532` — *"`borrowed Table` has no method `missing`"* — **and**
/// an `SC0333` against the literal `"host"`, because the misspelled method made
/// the callee opaque and the opaque rule tied the result to the key. A second
/// ill-typed program gains an `SC0334` the same way. That is the same false
/// positive `examples/09_absence_and_failure.science` produces, reached from a
/// typo instead of from a missing declaration.
/// `tests/cli.rs`'s `a_type_error_stops_the_borrow_check_before_it_invents_anything`
/// is the first of those programs, pinned.
///
/// So the two phases fail in opposite directions and the region checker is the
/// worse of the two to let run: the type checker over an unresolved file is
/// wrong in silence, and the region checker over an ill-typed file is wrong out
/// loud. Skipping it is the only reading under which an `SC0333` from
/// `sciencec` means what it says.
///
/// **The cost.** A file with one type error gets no borrow check at all, so a
/// genuine `SC0334` sitting beside a misspelled type is reported only after the
/// spelling is fixed. That is the same cost every earlier skip pays, and it is
/// the cost `ty`'s §5 chose when it made a hole a value: one mistake, one
/// message.
///
/// # What it costs to run
///
/// Every table is rebuilt per file. That was already true of the type checker
/// and it is more expensive now, because MIR and regions are added to it:
/// lowering every body, a call graph, Tarjan's algorithm over it, a liveness
/// fixpoint and a region fixpoint per body. Measured over `examples/` —
/// twenty-two files, four hundred and forty of them on one command line to get
/// past process startup — the per-file cost went from about 1.0 ms to about
/// 1.5 ms, which is half again as much work for the second half of the front
/// end.
///
/// **That is right for `check`**, which is a batch command over files named on
/// one command line and touches each one once, **and it is now emphatically
/// wrong for a language server**, which would rebuild an entire crate's MIR on
/// every keystroke. `science-db`'s query layer is what memoises them, and
/// `science_db::pending` still has `unimplemented!` for every stage from HIR
/// onward. **The shortcut's standing is therefore weaker than it was**: it was
/// a convenience for one phase and it is now the only way to run three, and
/// `science-regions`'s `analyse_crate` §3 has already said that the cache is
/// salsa's to own and that computing it here memoises nothing. This is still
/// the shortcut, and it is still named as one so nobody mistakes it for the
/// architecture.
fn type_and_region_check(krate: &Crate) -> Vec<Diagnostic> {
    check_and_lower(krate).diagnostics
}

/// A crate that type-checked, plus the two things a backend needs from it.
///
/// The `Types` table travels with the bodies because a MIR body is a graph of
/// `Ty` handles and a handle without its table is a number. `DefTable` is not
/// here because the caller already owns the `Crate` it belongs to.
struct Checked {
    diagnostics: Vec<Diagnostic>,
    types: science_types::Types,
    /// The declared signatures, kept because an `extern "C"` function has no
    /// MIR body and its parameter and return types reach the backend through
    /// nothing else. `science_codegen_llvm::BuildInput::decls` is the account.
    decls: science_types::Declarations,
    bodies: Vec<science_mir::mir::Body>,
}

/// The front end's back half, kept rather than discarded.
///
/// **Why this exists as a separate function from [`type_and_region_check`].**
/// `check` wants the diagnostics and nothing else; `build` wants the MIR as
/// well. Lowering twice to serve both would be two traversals and — worse — two
/// `Types` tables, so the `Ty` handles in one set of bodies would not be
/// comparable against the other's. One function produces all three and each
/// caller takes what it needs.
///
/// `bodies` is empty when the type checker reported an error, because the
/// ordering rule at [`region_check`] means MIR was never built; a caller that
/// wants to know whether it has a program asks `has_error` on the diagnostics,
/// not whether the vector is empty — a crate can legitimately have no bodies.
fn check_and_lower(krate: &Crate) -> Checked {
    let order = science_types::AtomOrder::of(&krate.defs);
    let mut types = science_types::Types::new();
    let mut diagnostics = science_diagnostics::Diagnostics::new();
    let mut aliases = science_types::Aliases::of(krate, &mut types, &order, &mut diagnostics);
    let mut decls = science_types::Declarations::of(krate, &mut types, &order, &mut diagnostics);
    let thir = science_types::check_crate(
        krate,
        &decls,
        &mut types,
        &mut aliases,
        &order,
        &mut diagnostics,
    );
    let mut all = diagnostics.into_vec();
    // The ordering rule above.
    if has_error(&all) {
        return Checked { diagnostics: all, types, decls, bodies: Vec::new() };
    }
    // **Between checking and lowering, which is the only window.** A
    // record's field types and a `choice`'s payloads live in `Declarations`
    // rather than in any body, so `region_check`'s pass over the MIR does not
    // reach them and a field declared `ticks: Counter` has no layout.
    // `Declarations::reveal_layouts` says at length what it touches and what
    // it must not; the timing is the half that belongs here. Earlier, and
    // every `expected … found …` in the program quotes a type the author did
    // not write. Later, and a projection's type disagrees with the type of
    // the local it projects from.
    decls.reveal_layouts(&mut types, &mut aliases);
    let (regions, bodies) = region_check(krate, &decls, &mut types, &mut aliases, &thir);
    all.extend(regions);
    Checked { diagnostics: all, types, decls, bodies }
}

/// The MIR of a crate that checked clean, or `None` when it did not.
///
/// The diagnostics are dropped here on purpose: the caller
/// ([`Session::emit_executable`]) has already reported them through
/// [`Session::diagnostics`], and reporting them twice would double every count
/// in the summary.
fn lower_to_mir(krate: &Crate) -> Option<Checked> {
    let checked = check_and_lower(krate);
    if has_error(&checked.diagnostics) { None } else { Some(checked) }
}

/// Where a built executable goes: beside its source, with the platform's
/// extension. See [`Session::build`] for why it is not the working directory.
fn executable_path(source: &Path) -> PathBuf {
    if cfg!(windows) { source.with_extension("exe") } else { source.with_extension("") }
}

/// The same path, in a form [`std::process::Command`] will treat as a file
/// rather than as a command to look up.
///
/// **`sciencec test hello.science` could not run what it had just built.**
/// [`executable_path`] strips the extension, so the program beside
/// `hello.science` is `hello` — a relative path with **no separator in it** —
/// and `Command::new` resolves exactly those through `PATH` instead of
/// against the working directory. The build succeeded, the file was written,
/// and the run failed with *"cannot run `hello`: no such file"*, which reads
/// like the compiler emitted nothing.
///
/// It was invisible from inside the repository because every path the corpus
/// passes has a directory in front of it — `examples/01_functions` contains a
/// separator and is therefore already a path, not a lookup. So the one form
/// that broke is the one a person types and the harness never does.
///
/// A leading `./` is enough to make it a path, and is preferred here over
/// [`std::fs::canonicalize`]: canonicalising resolves symlinks and would make
/// the spawn fail with a different message on a path that is perfectly
/// runnable, for a property no caller needs.
fn spawnable(exe: &Path) -> PathBuf {
    if exe.parent().is_some_and(|parent| !parent.as_os_str().is_empty()) {
        return exe.to_path_buf();
    }
    Path::new(".").join(exe)
}

/// Lowers a checked crate to MIR and runs the borrow check over it.
///
/// **The decision this settles: region checking runs in `check` by default,
/// not behind a flag.**
///
/// **Reason.** A flag is defensible when the phase is experimental, slow enough
/// to be opt-in, or wrong often enough that its output is noise. None of the
/// three holds. What a flag *would* buy is a corpus that reports zero, and
/// `crates/science-regions/tests/corpus.rs` had already measured what wiring
/// this in would report and argued that those diagnostics are information
/// rather than noise. A soundness phase that is permanently optional is a phase
/// nobody runs, which is the state that crate was in the day before this and
/// the state `type_and_region_check` exists to end. There is also no version of
/// the flag that could ever be removed: the false positives close when
/// `stdlib-core.md`'s containers acquire declarations, and nothing about *that*
/// day would make anyone delete a `--borrow-check` flag they had grown used to
/// typing.
///
/// **Cost, and it is the uncomfortable shape.** `sciencec check examples/` is
/// no longer silent: **two diagnostics, and both of them are about a hole in
/// this compiler rather than about the program.** The census was three when it
/// was taken, one of them real; that one closed first — `science-types` now
/// auto-borrows a `borrowed T` parameter — so what wiring the phase in actually
/// adds to the corpus today is two false positives and no true one. Shipping
/// that is still right, for the reason above, and the discipline it owes is
/// that it be *counted*: `tests/cli.rs`'s `REGIONS` pins each diagnostic by
/// file, code and verdict, and asserts the (real, false) split, so the ratio is
/// a number somebody has to change rather than a sentence somebody has to
/// believe. That is `UNRESOLVED`'s discipline applied to a second kind of known
/// gap — exact, so that closing it breaks the test rather than rotting it.
///
/// **What closes them** is out of this crate's reach and is worth naming
/// exactly, because *"the containers land"* is too vague to check against:
/// `Map.get` must become a declaration `science-types`'s method lookup can
/// find, so that `science-mir` lowers `settings.get(key)` to a `Callee::Def`
/// instead of a `Callee::Unresolved`, so that `science-regions`'s `generate`
/// takes the `known_callee` path and reads a real summary — *"the result
/// borrows the map"* — instead of §5's assumption that an opaque callee returns
/// a reference into every argument, the key included. The same sentence with
/// `Array.get` and `Iterate.next` in it removes `check`'s §6 suppressions. If
/// the containers live in another crate rather than in the prelude, Decision
/// 7's summary serialisation has to exist first, because `summary`'s §2 has no
/// format to write one into.
fn region_check(
    krate: &Crate,
    decls: &science_types::Declarations,
    types: &mut science_types::Types,
    aliases: &mut science_types::Aliases,
    thir: &[science_types::thir::Body],
) -> (Vec<Diagnostic>, Vec<science_mir::mir::Body>) {
    // The lowering context is dropped before the region one is built: both
    // want `&mut Types` and `&mut Aliases`, and MIR is finished with them.
    let bodies = {
        let mut context = science_mir::Context { defs: &krate.defs, decls, types, aliases };
        science_mir::lower_crate(&mut context, thir)
    };
    // **Aliases are revealed here, once, over every body.**
    //
    // The decision. Every `Ty` in every MIR body is replaced by its
    // alias-free form before anything downstream reads one.
    //
    // The reason. `science-types`' `alias`'s §1 keeps an alias's own
    // `TyKind::Named` in the table and computes the revealed type *beside*
    // it, deliberately: revealing eagerly in the lowering *"loses the
    // diagnostic"*, because `Types::render` prints what it is given and a
    // reader who wrote `Embedding` should not be told about `Array[F32]`.
    // That is right for the front end and it stops being right the moment a
    // type reaches a phase that has to know the *representation*. A backend
    // asked to lay out `Counter` has no diagnostic to protect and no field
    // list to find; it refuses, which is what `examples/02_bindings.science`
    // was doing.
    //
    // Here rather than in the backend, because §1 also says why revealing
    // cannot happen there: it *"needs `&mut Types` to intern what it
    // builds"*, and `BuildInput` hands a backend a `&Types` on purpose. This
    // is the last place that holds the table mutably, the same reason
    // `Mono::instantiate` runs in this file.
    //
    // Before region checking rather than after, so that one phase does not
    // see `Counter` while the next sees `I64`. `assign` already takes
    // revealed types and regions compare types; two spellings of one type
    // reaching a comparison is exactly the bug §6 of `ty` priced.
    //
    // The cost is the one §1 named and accepted in the other direction: past
    // this line the compiler's vocabulary no longer contains the author's
    // name for the type, so a *codegen* refusal says `I64` where the source
    // says `Counter`. Those refusals name a missing backend feature rather
    // than a mistake in the program, and every diagnostic that quotes a type
    // the author wrote is produced before this point.
    let bodies: Vec<science_mir::mir::Body> = bodies
        .iter()
        .map(|body| {
            // An alias whose body did not evaluate is already `Ty::ERROR` in
            // the table — `Aliases::of` cuts every cycle when it is built —
            // so a failure here is a const argument that does not evaluate,
            // and the body is left as it was. `ty`'s §5: an erroneous type
            // must not manufacture a second error, and the unrevealed body
            // refuses by name one phase later.
            science_mir::map_types(body, &mut |ty| aliases.reveal(types, ty))
                .unwrap_or_else(|_| body.clone())
        })
        .collect();
    let graph = science_mir::CallGraph::of(&bodies);
    let mut diagnostics = science_diagnostics::Diagnostics::new();
    let mut context = science_regions::Context { defs: &krate.defs, decls, types, aliases };
    science_regions::analyse_crate(&mut context, &bodies, &graph, &mut diagnostics);
    // The bodies are handed back rather than dropped: `build` needs exactly the
    // ones the borrow check just approved, and lowering a second set to get them
    // would be a second `Types` table. See [`check_and_lower`].
    (diagnostics.into_vec(), bodies)
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
