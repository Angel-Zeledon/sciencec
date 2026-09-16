//! The UI suite: every `.science` program directly in `tests/ui/` is lexed and
//! its diagnostics compared, byte for byte, against the `.stderr` file next to
//! it.
//!
//! Spec §10.3 asks for rustc-style UI tests. `science-testkit` walks the
//! directory and does the comparing; the compile step is a closure, so this
//! crate plugs in the phase it has — the lexer — and renders whatever it
//! reports through `science-diagnostics`.
//!
//! **Only the top level.** `tests/ui/` is sharded: its subdirectories hold
//! cases for phases the lexer cannot run, and their expectations are the
//! output of a closure that lexes, parses and resolves
//! (`crates/sciencec/tests/ui.rs`). Lexing one of those would compare it
//! against an expectation no lexer ever produced, so this suite stops at the
//! top level and the cases that belong to it — the ones the lexer alone
//! rejects — stay there.
//!
//! Set `SCIENCE_BLESS=1` to rewrite the expectations instead of failing, then
//! read `git diff tests/ui` before committing.

use std::path::Path;

use science_diagnostics::SourceMap;
use science_testkit::UiTestOptions;

#[test]
fn ui() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui");
    let options = UiTestOptions::from_env().without_subdirectories();

    let report = science_testkit::run_ui_tests_with(&dir, options, |path, source| {
        let mut map = SourceMap::new();
        // The registered path is what appears after `-->`, so it is registered
        // relative to the repository root with forward slashes. Anything else
        // would put a Windows path in the expectation and break the suite on
        // Linux, and vice versa.
        let name = format!("tests/ui/{}", path.file_name().unwrap().to_string_lossy());
        let file = map.add_file(name, source.to_string());
        let (_tokens, diagnostics) = science_lexer::lex(file, source);
        science_diagnostics::render_all(&map, &diagnostics)
    });

    // Cheap guard against the suite silently walking an empty or wrong
    // directory: a green run over zero cases is not a green run.
    assert!(report.total() > 0, "no UI tests found in {}", report.directory().display());

    report.assert_success();
}
