//! Guards on the test data itself, rather than on the harness.
//!
//! The `.science` corpus is input for other crates' test suites, and a mistake
//! in it shows up there as a confusing failure in the lexer or the parser.
//! These checks keep it honest from here, where it belongs.

use std::path::{Path, PathBuf};

use science_testkit::{expectation_path, normalize};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Every `.science` file under `dir`, except the ones that are not cases.
///
/// **`shared/` holds modules, not cases**, and the distinction had to become a
/// rule the moment `use` learned to load files: a UI case that imports a module
/// needs that module to exist next to it, and a module has no diagnostic of its
/// own to pin. The guards below would otherwise demand a `.stderr` for a file
/// whose whole purpose is to be silent until someone imports it.
///
/// The name carries the rule, rather than a list here that would drift: a
/// directory called `shared` inside `tests/ui/` is fixtures. The cost is that a
/// case can never be named `shared`, which is a price worth one word.
///
/// It is skipped for `examples/` too, harmlessly — that corpus has no such
/// directory, and a rule with one exception is easier to remember than a rule
/// with one exception that applies in one place.
fn science_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("shared") {
                continue;
            }
            found.extend(science_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("science") {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[test]
fn the_example_corpus_is_not_empty() {
    let examples = repository_root().join("examples");
    assert!(
        science_files(&examples).len() >= 10,
        "the example corpus should cover every §4 feature, one file per topic"
    );
}

#[test]
fn no_example_uses_a_tab_anywhere() {
    // §4.1: indentation is spaces only, and a tab in it is SC0003. Every
    // program in `examples/` is meant to be accepted, so none may contain
    // one — not even inside a string, where it would be too easy to lose
    // track of.
    for path in science_files(&repository_root().join("examples")) {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains('\t'), "tab character in {}", path.display());
    }
}

#[test]
fn every_example_is_listed_in_the_readme() {
    let examples = repository_root().join("examples");
    let readme = std::fs::read_to_string(examples.join("README.md"))
        .expect("examples/README.md should exist");

    for path in science_files(&examples) {
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(readme.contains(name.as_ref()), "{name} is not described in examples/README.md");
    }
}

#[test]
fn the_acceptance_program_exists_and_names_every_requirement() {
    let path = repository_root().join("examples").join("00_kitchen_sink.science");
    let text = std::fs::read_to_string(&path).expect("the §11 acceptance program should exist");

    // §11.1 lists what the acceptance program must use at once. Each needle
    // below is one item of that list, in the order §11 gives them, so a
    // requirement that quietly leaves the program fails here and not in a
    // review six months later.
    for (needle, requirement) in [
        ("[T: Summarize]", "generics with bounds"),
        ("const ", "const generics"),
        ("type Item is", "associated types"),
        ("&any Summarize", "a trait dispatched dynamically"),
        ("Box[any Summarize]", "a trait object behind a box"),
        ("match ", "an exhaustive match"),
        // §11.1's sixth requirement was "Option and Result with `try`" and is
        // now "a nullable type and a fallible function returning `(T, Error?)`".
        // It is two needles because it is two things: one requirement that
        // could be satisfied by either half would not be checking either.
        ("-> String?", "a nullable type"),
        (", Error?)", "a fallible function returning `(T, Error?)`"),
        ("err?", "the presence test that makes the pair usable"),
        ("implements Add", "a user type implementing an operator trait"),
        ("in 0..", "a range-driven loop"),
        ("each.", "a closure"),
        ("&Doc", "a type holding a borrow in a field"),
    ] {
        assert!(
            text.contains(needle),
            "the acceptance program never uses {needle:?}, so it does not show {requirement}"
        );
    }
    assert!(text.contains("where T: Summarize + Clone"), "the `where` clause is gone");
}

#[test]
fn every_ui_case_has_an_expectation_next_to_it() {
    let ui = repository_root().join("tests").join("ui");
    let cases = science_files(&ui);
    assert!(!cases.is_empty(), "there should be UI cases in tests/ui");

    for case in cases {
        let expectation = expectation_path(&case);
        assert!(
            expectation.is_file(),
            "{} has no expectation; add one or bless it with SCIENCE_BLESS=1",
            case.display()
        );
    }
}

#[test]
fn every_ui_expectation_looks_like_a_rendered_diagnostic() {
    let ui = repository_root().join("tests").join("ui");
    for case in science_files(&ui) {
        let expectation = expectation_path(&case);
        let text = normalize(&std::fs::read_to_string(&expectation).unwrap());

        assert!(
            text.starts_with("error[SC"),
            "{} should start with an error code, per §9",
            expectation.display()
        );
        assert!(
            text.contains(" --> "),
            "{} should point at a file, line and column",
            expectation.display()
        );
        // The path in the location line is relative to the repository root
        // and uses forward slashes, so the same file works on every platform.
        assert!(
            text.contains("--> tests/ui/"),
            "{} should use a repository-relative path with forward slashes",
            expectation.display()
        );
    }
}
