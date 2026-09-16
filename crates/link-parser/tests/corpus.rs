//! The real acceptance check: lex and parse every program in `examples/`.
//!
//! `examples/README.md` states the contract those files live under — they are
//! meant to be syntactically correct, so "anything that fails to lex or parse
//! here is a bug in the compiler or a gap in the spec". This test holds the
//! parser to it over the whole corpus at once, which no hand-built token
//! stream can do: the corpus is the only place where the lexer's real
//! indentation, comments and line continuations meet the parser.
//!
//! Every file must lex and parse with no diagnostics at all. There are no
//! exemptions: a corpus with a list of files that do not have to work stops
//! being an acceptance check.

use std::path::{Path, PathBuf};

use link_diagnostics::{Diagnostics, FileId};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("examples")
}

/// Every `.link` file in `examples/`, in a stable order.
fn corpus() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples/ must exist")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "link"))
        .collect();
    files.sort();
    files
}

fn render(diagnostics: &Diagnostics, source: &str) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics.iter() {
        let (line, text) = match diagnostic.primary_span() {
            Some(span) => {
                let before = &source[..span.start as usize];
                let line = before.matches('\n').count() + 1;
                let start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
                let end = source[start..].find('\n').map(|i| start + i).unwrap_or(source.len());
                (line, source[start..end].trim())
            }
            None => (0, ""),
        };
        out.push_str(&format!(
            "    line {line}: {} {}\n        > {text}\n",
            diagnostic.code, diagnostic.message
        ));
    }
    out
}

/// Lexes and parses one file, returning every diagnostic from both phases.
fn check(path: &Path) -> String {
    let source = std::fs::read_to_string(path).expect("readable corpus file");
    let (tokens, lex_diagnostics) = link_lexer::lex(FileId(0), &source);
    let (_module, parse_diagnostics) = link_parser::parse_module(&tokens, FileId(0));

    let mut report = render(&lex_diagnostics, &source);
    report.push_str(&render(&parse_diagnostics, &source));
    report
}

/// The acceptance check: the whole corpus lexes and parses clean.
#[test]
fn every_example_parses_without_diagnostics() {
    let files = corpus();
    assert!(!files.is_empty(), "the corpus must not be empty");

    let mut failures = String::new();
    let mut checked = 0;
    for path in &files {
        checked += 1;
        let report = check(path);
        if !report.is_empty() {
            failures.push_str(&format!("{}\n{report}", path.display()));
        }
    }

    assert!(checked >= 19, "expected the whole corpus, only checked {checked} files");
    assert!(failures.is_empty(), "the example corpus must parse clean:\n{failures}");
}

/// A module that parses clean must also come out non-empty: a parser that
/// silently swallowed a file would otherwise pass the test above.
#[test]
fn every_example_produces_items() {
    for path in corpus() {
        let source = std::fs::read_to_string(&path).expect("readable corpus file");
        let (tokens, _) = link_lexer::lex(FileId(0), &source);
        let (module, _) = link_parser::parse_module(&tokens, FileId(0));
        assert!(!module.items.is_empty(), "{} parsed into no items", path.display());
    }
}
