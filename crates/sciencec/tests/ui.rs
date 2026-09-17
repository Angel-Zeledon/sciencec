//! The UI suite for the phases after the lexer.
//!
//! `tests/ui/` is sharded by the phase that rejects the program. The files
//! directly in it are the ones the lexer alone rejects, and
//! `crates/science-lexer/tests/ui.rs` walks those. The subdirectories are the
//! ones a later phase rejects, and this is the only crate that can render
//! them: `sciencec` is where every phase from the lexer to region inference is
//! in scope, which is exactly what `tests/ui/README.md` said would have to
//! happen once the parser could produce `SC0100`-range diagnostics.
//!
//! * `tests/ui/parse/` — the parser's, `SC0100`-`SC0199`.
//! * `tests/ui/resolve/` — the resolver's, `SC0200`-`SC0249`.
//! * `tests/ui/types/` — the type checker's, `SC0140` and `SC0520`-`SC0579`.
//! * `tests/ui/regions/` — region inference's, `SC0330`-`SC0340`.
//!
//! The split is not decoration. A case is filed in the shard named by the
//! phase that rejects it, and the shard is a claim about the program as much
//! as the expectation is: a case in `types/` that acquired a parse error would
//! still have an expectation, and the expectation would be about the wrong
//! phase.
//!
//! # The pipeline is the driver's
//!
//! What is rendered here is what `sciencec check` prints, minus the `N errors`
//! summary the driver adds at the end — which belongs to the command and not
//! to any program. One closure runs every shard, and it is
//! [`Session::diagnostics`]'s own sequence, skips included:
//!
//! * **resolution is skipped when lexing or parsing already found an error**,
//!   so a case in `parse/` never reaches the resolver and a case in `resolve/`
//!   has to lex and parse clean to be about anything at all;
//! * **type checking is skipped when resolution found an error**, because the
//!   resolver's unresolved names come back as `Ty::ERROR`, which agrees with
//!   everything, so an unresolved file would type-check silently and wrongly;
//! * **region inference is skipped when the type checker found an error**,
//!   which is `driver::type_and_region_check`'s own rule and the sharpest of
//!   the three: a call the checker rejected is still a call in MIR, and the
//!   opaque-callee rule *manufactures* `SC0333`s at spans in code that was
//!   never the mistake. A case in `regions/` therefore has to type-check
//!   clean, and a second diagnostic from an earlier phase in one of these
//!   expectations is a case that is about something other than what it claims.
//!
//! Set `SCIENCE_BLESS=1` to rewrite the expectations instead of failing, then
//! read `git diff tests/ui` before committing.

use std::path::Path;

use science_db::ScienceDatabase;
use science_diagnostics::{Diagnostic, Diagnostics, Severity};
use science_resolve::hir::Crate;
use science_testkit::UiTestOptions;

#[test]
fn ui() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui");

    for shard in ["parse", "resolve", "types", "regions"] {
        let dir = root.join(shard);
        let options = UiTestOptions::from_env().without_subdirectories();

        let report = science_testkit::run_ui_tests_with(&dir, options, |path, source| {
            let mut db = ScienceDatabase::new();
            // The registered path is what appears after `-->`. It is spelled
            // relative to the repository root with forward slashes, for the
            // reason the lexer's suite spells its own that way: a Windows path
            // in an expectation makes the same program produce different
            // output on two machines.
            let stem =
                path.file_name().expect("a case has a file name").to_string_lossy().into_owned();
            let file = db.add_file(format!("tests/ui/{shard}/{stem}"), source.to_string());

            let mut diagnostics = Diagnostics::new();
            for diagnostic in check(&mut db, shard, &dir, &stem, file) {
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

/// `Session::diagnostics`, rebuilt here because `sciencec` is a binary.
///
/// The three skips are the driver's and are stated in the module header. The
/// duplication is the price of the driver having no library half; what keeps
/// the two honest about each other is `tests/cli.rs`, which runs the binary.
///
/// # A case is an entry file, and may not be the only file
///
/// The case is the entry, its shard directory is the crate root, and `use`
/// loads from there — the driver's rule, with the same
/// `science_resolve::modules::collect_crate` doing the loading. A module a case
/// imports therefore lives in a **subdirectory** of the shard, which is exactly
/// where the walker does not look for cases (`without_subdirectories`), so it
/// is a module and never a case in its own right.
///
/// That is not only a trick to keep the walker quiet. `script-mode.md` §4.3 is
/// the rule that one file is a script alone and a module when imported, and a
/// UI suite that could not hold both files could not show it.
fn check(
    db: &mut ScienceDatabase,
    shard: &str,
    root: &Path,
    entry: &str,
    file: science_diagnostics::FileId,
) -> Vec<Diagnostic> {
    let entry = science_resolve::SourceModule {
        file,
        path: entry.to_string(),
        ast: science_db::ast(db, file).value().clone(),
        entry: true,
    };
    let sources = science_resolve::modules::collect_crate(entry, |candidate| {
        let text = std::fs::read_to_string(root.join(candidate)).ok()?;
        // The same normalisation the harness applies to a case: a module
        // checked out with CRLF must not shift every span by one byte a line.
        let name = format!("tests/ui/{shard}/{candidate}");
        let loaded = db.add_file(name, text.replace("\r\n", "\n"));
        Some((loaded, science_db::ast(db, loaded).value().clone()))
    });

    let mut all = Vec::new();
    for source in &sources {
        all.extend(science_db::file_diagnostics(db, source.file).to_vec());
    }
    if has_error(&all) {
        return all;
    }

    let (krate, resolution) = science_resolve::resolve_crate(&sources);
    all.extend(resolution.into_vec());
    if has_error(&all) {
        return all;
    }

    all.extend(type_and_region_check(&krate));
    all
}

fn type_and_region_check(krate: &Crate) -> Vec<Diagnostic> {
    let order = science_types::AtomOrder::of(&krate.defs);
    let mut types = science_types::Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = science_types::Aliases::of(krate, &mut types, &order, &mut diagnostics);
    let decls = science_types::Declarations::of(krate, &mut types, &order, &mut diagnostics);
    let thir = science_types::check_crate(
        krate,
        &decls,
        &mut types,
        &mut aliases,
        &order,
        &mut diagnostics,
    );
    let mut all = diagnostics.into_vec();
    if has_error(&all) {
        return all;
    }

    let bodies = {
        let mut context = science_mir::Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        science_mir::lower_crate(&mut context, &thir)
    };
    let graph = science_mir::CallGraph::of(&bodies);
    let mut regions = Diagnostics::new();
    let mut context = science_regions::Context {
        defs: &krate.defs,
        decls: &decls,
        types: &mut types,
        aliases: &mut aliases,
    };
    science_regions::analyse_crate(&mut context, &bodies, &graph, &mut regions);
    all.extend(regions.into_vec());
    all
}

fn has_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}
