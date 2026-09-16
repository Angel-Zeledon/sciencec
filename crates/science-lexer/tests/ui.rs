//! The UI suite: every `.science` program under `tests/ui/` is lexed and its
//! diagnostics compared, byte for byte, against the `.stderr` file next to it.
//!
//! Spec §10.3 asks for rustc-style UI tests. `science-testkit` walks the
//! directory and does the comparing; the compile step is a closure, so this
//! crate plugs in the only phase that exists today — the lexer — and renders
//! whatever it reports through `science-diagnostics`.
//!
//! Set `SCIENCE_BLESS=1` to rewrite the expectations instead of failing, then
//! read `git diff tests/ui` before committing.

use std::path::Path;

use science_diagnostics::SourceMap;

#[test]
fn ui() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui");

    let report = science_testkit::run_ui_tests(&dir, |path, source| {
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
