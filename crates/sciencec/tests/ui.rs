//! The UI suite for the phases after the lexer.
//!
//! `tests/ui/` is sharded by the phase that rejects the program. The files
//! directly in it are the ones the lexer alone rejects, and
//! `crates/science-lexer/tests/ui.rs` walks those. The subdirectories are the
//! ones a later phase rejects, and this is the only crate that can render
//! them: `sciencec` is where the lexer, the parser and the resolver are all
//! in scope, which is exactly what `tests/ui/README.md` said would have to
//! happen once the parser could produce `SC0100`-range diagnostics.
//!
//! * `tests/ui/parse/` — the parser's, `SC0100`-`SC0199`.
//! * `tests/ui/resolve/` — the resolver's, `SC0200`-`SC0249`.
//!
//! The split is not decoration. The two closures differ in nothing but the
//! phases they reach, so a case whose expectation was blessed under one and
//! compared under the other would fail for a reason that has nothing to do
//! with the program; keeping each shard beside the suite that owns it is what
//! stops that.
//!
//! # The pipeline is the driver's
//!
//! What is rendered here is what `sciencec check` prints, minus the `N errors`
//! summary the driver adds at the end — which belongs to the command and not
//! to any program. In particular **resolution is skipped when lexing or
//! parsing already found an error**, as in [`Session::diagnostics`]; the
//! parser recovers rather than aborting, so there is a tree to resolve, but
//! its error nodes come back as unresolved names and the cascade buries the
//! one diagnostic the reader needs. A case in `parse/` therefore never reaches
//! the resolver, and a case in `resolve/` has to lex and parse clean to be
//! about anything at all.
//!
//! Set `SCIENCE_BLESS=1` to rewrite the expectations instead of failing, then
//! read `git diff tests/ui` before committing.

use std::path::Path;

use science_db::ScienceDatabase;
use science_diagnostics::{Diagnostics, Severity};
use science_testkit::UiTestOptions;

#[test]
fn ui() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui");

    for shard in ["parse", "resolve"] {
        let dir = root.join(shard);
        let options = UiTestOptions::from_env().without_subdirectories();

        let report = science_testkit::run_ui_tests_with(&dir, options, |path, source| {
            let mut db = ScienceDatabase::new();
            // The registered path is what appears after `-->`. It is spelled
            // relative to the repository root with forward slashes, for the
            // reason the lexer's suite spells its own that way: a Windows path
            // in an expectation makes the same program produce different
            // output on two machines.
            let name = format!(
                "tests/ui/{shard}/{}",
                path.file_name().expect("a case has a file name").to_string_lossy()
            );
            let file = db.add_file(name, source.to_string());

            let mut all: Vec<_> = science_db::file_diagnostics(&db, file).to_vec();
            if !all.iter().any(|d| d.severity == Severity::Error) {
                let parsed = science_db::ast(&db, file);
                let path = db.path(file).to_string();
                let (_krate, resolved) =
                    science_resolve::resolve_module(file, &path, parsed.value());
                all.extend(resolved);
            }

            let mut diagnostics = Diagnostics::new();
            for diagnostic in all {
                diagnostics.push(diagnostic);
            }
            science_diagnostics::render_all(&db.source_map(), &diagnostics)
        });

        // A green run over zero cases is not a green run, and a shard that
        // lost its directory would be exactly that.
        assert!(report.total() > 0, "no UI tests found in {}", report.directory().display());
        report.assert_success();
    }
}
