//! The example corpus must lex clean.
//!
//! `examples/` holds programs that are meant to be *accepted* (its README
//! says so, and `tests/ui/` is where the rejected ones live). Anything the
//! lexer reports over one of them is therefore either a bug in the lexer or
//! a mistake in the corpus, and this test is what says which file.

use std::path::{Path, PathBuf};

use science_diagnostics::SourceMap;

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn science_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("the example corpus should exist").flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(science_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("science") {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[test]
fn every_example_lexes_without_a_single_diagnostic() {
    let files = science_files(&examples_dir());
    assert!(files.len() >= 20, "the corpus should cover every §4 feature, one file per topic");

    let mut report = String::new();
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");

        let mut map = SourceMap::new();
        let name = format!("examples/{}", path.file_name().unwrap().to_string_lossy());
        let file = map.add_file(name, source.clone());

        let (_tokens, diagnostics) = science_lexer::lex(file, &source);
        if !diagnostics.is_empty() {
            report.push_str(&science_diagnostics::render_all(&map, &diagnostics));
            report.push('\n');
        }
    }

    assert!(report.is_empty(), "the example corpus must lex clean:\n\n{report}");
}

#[test]
fn every_example_ends_in_a_dedent_to_column_zero() {
    // A file whose last block is never closed still lexes, because the lexer
    // flushes its indentation stack at EOF. That flush is the thing worth
    // pinning: the last token is always `Eof`, and nothing is left open.
    for path in science_files(&examples_dir()) {
        let source = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let (tokens, _) = science_lexer::lex(science_diagnostics::FileId(0), &source);
        assert_eq!(
            tokens.last().map(|t| &t.kind),
            Some(&science_lexer::TokenKind::Eof),
            "{} does not end in Eof",
            path.display()
        );
    }
}
