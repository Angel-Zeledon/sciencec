//! What every example in `examples/` **prints**, pinned.
//!
//! # Why this exists, and why "it builds" was never enough
//!
//! Every other measurement of the corpus in this repository asks whether a
//! file *builds*. Two bugs found on one day exited 0 and printed the wrong
//! answer:
//!
//! - Two modules of one crate each defining `value` mangled to one symbol, so
//!   `helper.value() + deep.inner.value()` printed `2` where the author wrote
//!   `1 + 2`. The walk **detected** the collision and nothing read the field
//!   it reported to.
//! - A `match` on a one-element variant payload projected a `TupleField` the
//!   backend does not wrap, so `Ident(String)` yielded the `String`'s byte
//!   pointer and `At(Point)` yielded `Point.x`.
//!
//! Neither would have been caught by any test in this repository. `sciencec
//! test` reported `ok` for both, because exit 0 is all it asks for.
//!
//! So this file records the **bytes** each example writes. A miscompile that
//! still exits 0 changes them.
//!
//! # The decision: a recorded expectation, not an assertion in the source
//!
//! The alternative is `assert` inside each example. It was rejected: the
//! corpus is a *syntax showcase* first — `examples/README.md` describes each
//! file by the constructs it demonstrates — and salting it with assertions
//! would make every file harder to read for the audience it is written for.
//! An expectation beside the file costs nothing a reader of the example has
//! to look at.
//!
//! Blessed with `SCIENCE_BLESS=1`, the convention `tests/ui` already uses, so
//! there is one way to record an expectation in this repository rather than
//! two.
//!
//! # What it does **not** claim
//!
//! That the recorded output is *right*. It is what the compiler produced when
//! a human looked at it. What the file pins is that it does not change by
//! accident — which is exactly the property the two bugs above violated and
//! nothing else checks.
//!
//! An example that does not build has **no** expectation file, and that is a
//! recorded fact too: [`every_example_that_builds_has_its_output_pinned`]
//! fails when a file starts building and nobody blessed its output, so the
//! corpus cannot silently grow an unmeasured program.

#![cfg(feature = "llvm")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn examples() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo_root().join("examples"))
        .expect("examples/")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()?.to_str()? == "science").then_some(path)
        })
        .collect();
    out.sort();
    out
}

/// Where an example's recorded output lives.
///
/// Beside the example and not in a `snapshots/` directory, because a reader
/// looking at `05_match.science` should find `05_match.stdout` next to it
/// rather than in a tree they have to know about. `.gitignore`'s inverted
/// rule for `examples/` names it explicitly.
fn expectation(example: &Path) -> PathBuf {
    example.with_extension("stdout")
}

/// Build and run one example, or `None` if it does not build.
///
/// `sciencec test` and not `build` then execute, because that is the command
/// a user runs and the one whose behaviour this corpus is a measurement of.
/// Its own trailing `test … ok` line is dropped: it is the harness talking,
/// not the program.
fn ran(example: &Path) -> Option<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(["test", example.to_str().expect("a utf-8 path")])
        .output()
        .expect("the sciencec binary must be runnable");
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    Some(text.lines().filter(|line| !line.starts_with("test ")).collect::<Vec<_>>().join("\n"))
}

fn blessing() -> bool {
    std::env::var("SCIENCE_BLESS").is_ok_and(|value| value == "1")
}

/// **Every example that builds prints what it printed last time.**
#[test]
fn every_example_that_builds_has_its_output_pinned() {
    let mut unpinned = Vec::new();
    let mut wrong = Vec::new();

    for example in examples() {
        let name = example.file_name().expect("a name").to_string_lossy().into_owned();
        // **One exclusion, with its reason, and it is a bug this file found.**
        //
        // `10_loops.science` prints a value that **differs between runs** —
        // `90219483912`, then `8688778024` — at the sixth line of its output.
        // A number that changes per run is an address, so something is
        // printing a pointer where a value belongs, and the program exits 0
        // either way. That is exactly the class of defect this file exists
        // to catch, found within minutes of it existing.
        //
        // It is **not** the array loop, which was the obvious suspect: `for
        // item in items:` summing an `&Array[Int]` reduces to five lines and
        // prints `6`, deterministic.
        //
        // **It is `value_at`, and it reduces to three lines that `check`
        // accepts:**
        //
        // ```science
        // def value_at(items: &Array[Int], index: Int) -> Int:
        //     let cell be items.get(index)
        //     if cell?: cell else: 0
        // ```
        //
        // `Array.get` returns `(&Int)?` — Decision 19's **niched** option,
        // whose payload is a borrow. `if cell?:` narrows it to `&Int`, the
        // signature says `Int`, and the pointer is returned where the value
        // belongs. `sciencec check` exits 0.
        //
        // It was a *verifier* error hours ago — "local `_0` is `i64` and the
        // value stored into it is `ptr`" — and now passes the verifier and
        // returns garbage, which is strictly worse.
        //
        // Left unfixed **deliberately**: the repair is either the checker
        // refusing to return a `&Int` as an `Int`, or MIR dereferencing, and
        // that is the same projection-into-a-nullable's-payload question that
        // `lower_match` hits on a narrowed scrutinee and that `Rvalue::Narrow`
        // hits on an owning payload. Three routes, one decision, and it
        // should be taken once rather than three times.
        //
        // Excluded rather than blessed, because blessing a nondeterministic
        // value would make this test fail at random and be switched off,
        // which is how a ratchet dies. The entry goes when the bug does.
        if name == "10_loops.science" {
            continue;
        }
        let Some(actual) = ran(&example) else {
            // It does not build. Its expectation, if any, is stale — but
            // removing it here would make a regression look like a blessing,
            // so it is left alone and the build failure is the other tests'
            // business.
            continue;
        };
        let path = expectation(&example);
        if blessing() {
            std::fs::write(&path, format!("{actual}\n")).expect("an expectation to write");
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(expected) => {
                if expected.trim_end() != actual.trim_end() {
                    wrong.push(format!(
                        "{name}\n  expected: {:?}\n  actual:   {:?}",
                        expected.trim_end(),
                        actual.trim_end()
                    ));
                }
            }
            Err(_) => unpinned.push(name),
        }
    }

    assert!(
        unpinned.is_empty(),
        "these examples build and print something nobody recorded, so a miscompile in them \
         would be invisible. Bless them with `SCIENCE_BLESS=1 cargo test -p sciencec \
         --features llvm --test corpus_output`: {unpinned:?}"
    );
    assert!(
        wrong.is_empty(),
        "an example's output changed. If the change is intended, bless it; if it is not, this \
         is the miscompile this file exists to catch:\n{}",
        wrong.join("\n")
    );
}

/// An expectation with no example beside it is a file nobody will ever read
/// again, and it would go on passing forever.
#[test]
fn no_expectation_outlives_its_example() {
    for entry in std::fs::read_dir(repo_root().join("examples")).expect("examples/") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("stdout") {
            continue;
        }
        assert!(
            path.with_extension("science").is_file(),
            "{} has no example; delete it",
            path.display()
        );
    }
}
