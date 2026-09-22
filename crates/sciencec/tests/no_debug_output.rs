//! **No compiler phase prints to a stream except through a diagnostic.**
//!
//! # Why this exists
//!
//! Twice in one day a debugging `println!` left in a library crate reached a
//! user's terminal, and neither time did it look like what it was:
//!
//! - `DEBUG pending_named entry: def=Pair var=InferVar(8) …` came out of
//!   `science-types` and made `cli.rs`'s
//!   `every_example_is_clean_through_the_whole_front_half_except_the_known_gaps`
//!   fail with the debug text as the diff — a test about the corpus,
//!   reporting a defect that had nothing to do with the corpus.
//! - `DBG call_method: sig found for method=DefId(141) …` came out of the
//!   same crate and was the entire output of
//!   `sciencec test examples/07_generics.science`, which had until then been
//!   reporting a real refusal. The refusal was still there; nobody could see
//!   it.
//!
//! Both were left behind by work that was interrupted, which is the normal
//! way this happens: the print is added deliberately, and removing it is the
//! last step of a task that did not reach its last step.
//!
//! # What it checks, and what it deliberately does not
//!
//! Every crate whose job is to *compute* something — the lexer through the
//! backend — may write to a stream only by handing a
//! [`science_diagnostics::Diagnostic`] to whoever called it. `sciencec` is
//! excluded because printing is its whole purpose, and the runtime crate is
//! excluded because `print` in a Science program lands there.
//!
//! It reads the source text and not the syntax tree. A macro named in a
//! comment or inside a string is therefore a hit, and that is the deliberate
//! direction: this test is cheap and slightly over-eager, and the cost of a
//! false positive is a line moved out of a comment, where the cost of a false
//! negative is what the two incidents above cost. Doc comments *are*
//! skipped, because naming `println!` while arguing against it is what a
//! doc comment in this repository does — `science-codegen`'s diagnostics
//! module does exactly that today.

use std::path::{Path, PathBuf};

/// The crates that may not print. Named rather than discovered, so that a new
/// crate is a deliberate decision about which side of this line it is on.
const SILENT: &[&str] = &[
    "science-lexer",
    "science-parser",
    "science-resolve",
    "science-types",
    "science-mir",
    "science-regions",
    "science-codegen",
    "science-codegen-llvm",
    "science-diagnostics",
];

const MACROS: &[&str] = &["println!", "eprintln!", "print!", "eprint!", "dbg!"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_library_crate_prints_outside_a_diagnostic() {
    let mut found = Vec::new();

    for crate_name in SILENT {
        let src = repo_root().join("crates").join(crate_name).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        files.sort();
        assert!(!files.is_empty(), "no sources found for `{crate_name}`; did the crate move?");

        for file in files {
            let text = std::fs::read_to_string(&file).expect("a source file to read");
            for (number, line) in text.lines().enumerate() {
                let trimmed = line.trim_start();
                // A doc comment or an ordinary comment may name the macro:
                // arguing about `println!` is not calling it.
                if trimmed.starts_with("//") {
                    continue;
                }
                for macro_name in MACROS {
                    if line.contains(macro_name) {
                        found.push(format!(
                            "{}:{}: {}",
                            file.strip_prefix(repo_root()).unwrap_or(&file).display(),
                            number + 1,
                            trimmed.trim_end()
                        ));
                    }
                }
            }
        }
    }

    assert!(
        found.is_empty(),
        "a compiler phase writes to a stream directly. Every one of these reaches a user's \
         terminal in the middle of whatever they were compiling, and the two that did so \
         today each hid a real diagnostic behind it. Report it as a `Diagnostic`, or delete \
         it:\n{}",
        found.join("\n")
    );
}
