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
//!
//! # A third way to fail: never printing anything at all
//!
//! `7674487` is a bug this file's own machinery could not have caught before
//! `Session::run_test` learned to give up. `for i in 0..5:` with a `continue`
//! in it hung forever — the language's most-written construct — and every
//! example in this corpus that builds is run once by
//! [`every_example_that_builds_has_its_output_pinned`], with nothing between
//! it and `cargo test` but `Command::output`'s unconditional wait. A hanging
//! example would have hung this file, and through it the whole suite, rather
//! than failing it: no wrong bytes to pin, because there are no bytes.
//!
//! It is fixed at the source and not here: `Session::run_test`
//! (`crates/sciencec/src/driver.rs`) now kills a program that outlives its own
//! `RUN_BUDGET` and reports that as a `FAILED` distinct from a wrong exit
//! code, so `sciencec test` — the command this file measures and the one a
//! user runs — never blocks forever on any input, not just the ones in this
//! corpus. What [`ran`] adds is narrower: it reads that message back out
//! rather than folding a killed program into "does not build", which would
//! have hidden the regression this file exists to catch behind the one
//! `continue` already fixed above the assertion's reach.

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

/// What running one example through `sciencec test` found.
enum Verdict {
    /// It does not build.
    NotBuilt,
    /// It built and `sciencec test` killed it rather than wait forever — see
    /// [`ran`].
    Hung,
    /// It built, ran, and exited; this is what it printed.
    Printed(String),
}

/// Build and run one example.
///
/// `sciencec test` and not `build` then execute, because that is the command
/// a user runs and the one whose behaviour this corpus is a measurement of.
/// Its own trailing `test … ok` line is dropped: it is the harness talking,
/// not the program.
///
/// # Why a hang is not folded into "does not build"
///
/// This function used to return `Option<String>`, `None` meaning only "did
/// not build" — a nonzero exit with no further question asked. `Session::
/// run_test` (`crates/sciencec/src/driver.rs`) no longer blocks forever on a
/// program that never exits: past its own `RUN_BUDGET` it kills the child and
/// prints `"... FAILED: did not exit within {n}s and was killed"` rather than
/// hanging `sciencec test` itself. That message is also a nonzero exit, and
/// folding it into `None` would have swapped one failure to catch this file's
/// module doc opens with for another exactly as bad: the one example that
/// hangs would be silently treated as one that never compiled, forever, by
/// [`every_example_that_builds_has_its_output_pinned`]'s own `continue`. So the
/// message is matched here, and a hang is its own outcome — the exact shape of
/// `7674487`, caught by an assertion instead of a suite that never finishes.
fn ran(example: &Path) -> Verdict {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(["test", example.to_str().expect("a utf-8 path")])
        .output()
        .expect("the sciencec binary must be runnable");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.contains("did not exit within") {
        return Verdict::Hung;
    }
    if !output.status.success() {
        return Verdict::NotBuilt;
    }
    Verdict::Printed(
        text.lines().filter(|line| !line.starts_with("test ")).collect::<Vec<_>>().join("\n"),
    )
}

fn blessing() -> bool {
    std::env::var("SCIENCE_BLESS").is_ok_and(|value| value == "1")
}

/// **Every example that builds prints what it printed last time.**
#[test]
fn every_example_that_builds_has_its_output_pinned() {
    let mut unpinned = Vec::new();
    let mut wrong = Vec::new();
    let mut hung = Vec::new();

    for example in examples() {
        let name = example.file_name().expect("a name").to_string_lossy().into_owned();
        // **`10_loops.science` was excluded here, and is not any more.**
        //
        // It printed a value that differed between runs — `90219483912`, then
        // `8688778024` — at the sixth line of its output, traced to
        // `value_at`'s `if cell?: cell else: 0` over `Array.get`'s niched
        // `(&Int)?`: the pointer came back where the value belonged, because
        // reading a narrowed niched borrow as a plain value took the *address*
        // of its own storage instead of the bytes already sitting there.
        // `science-mir`'s `read_ergonomic` and `science-codegen`'s
        // `Coercion::Copy` both trusted `TyKind::Borrowed` alone to mean
        // "there is a pointer here", and `(borrowed T)?` narrowed to
        // `borrowed T` is one without saying so at that level; the fix is
        // `Lowerer::borrowed_referent`, asked instead of the bare match, on
        // both sides of the boundary. `lower_match`'s scrutinee and
        // `Rvalue::Narrow`'s owning payload were the same question from two
        // more routes, closed the same day by `Projection::Payload`.
        let actual = match ran(&example) {
            Verdict::NotBuilt => {
                // It does not build. Its expectation, if any, is stale — but
                // removing it here would make a regression look like a
                // blessing, so it is left alone and the build failure is the
                // other tests' business.
                continue;
            }
            Verdict::Hung => {
                // It builds and does not exit. `wrong`/`unpinned` are about
                // what a program prints; a hang has nothing to compare, which
                // is `7674487`'s whole lesson, so it gets its own bucket and
                // its own assertion below rather than being silently read as
                // "does not build".
                hung.push(name);
                continue;
            }
            Verdict::Printed(text) => text,
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
    assert!(
        hung.is_empty(),
        "these examples build and then never exit — `sciencec test` killed them after its own \
         budget rather than hang this suite, which is the fix, but a build that no longer \
         terminates is still a regression and not a pass: {hung:?}"
    );
}

/// **A pinned expectation was wrong here once, and the pin is why that is
/// fixed rather than merely fixed-looking.**
///
/// `examples/18_ownership.stdout` line 13 used to read `0` where the program
/// computes `title.length() + body.length()` over two four-byte strings and
/// should read `8`. Reduced to three lines, the borrow kind was the whole of
/// it:
///
/// ```science
/// def via_shared(doc: &Doc) -> Int:      // 4, correct
///     let t be &doc.title
///     t.length()
///
/// def via_mut(doc: &mut Doc) -> Int:     // used to print 0
///     let t be &doc.title
///     t.length()
/// ```
///
/// A **shared** borrow of a field, taken through an **exclusive** borrow of
/// the record, read as an empty `String`. Reading the field directly was
/// correct either way, and one borrow was enough to show it — it was never
/// about two disjoint ones, which is where it was first noticed.
///
/// **Cause.** `check.rs`'s `ExprKind::Borrowed` arm collapses `&place` into a
/// reborrow of `place`'s own referent when `place` is already ergonomically a
/// borrow (Decision 27), but only did so when the two mutabilities matched
/// exactly. `&doc.title` through a `&mut Doc` widens `doc.title` itself to
/// `&mut String` before the explicit `&` is even applied, so a *shared* `&`
/// over that mismatched and fell to the `_` arm, which kept the whole
/// `&mut String` as the referent — typing `t` as `&(&mut String)` instead of
/// `&String`. `science-mir`'s method-call auto-deref trusted that type and
/// peeled two layers of `Deref` off `t` for `t.length()` where one was
/// correct, reading past the field into whatever followed it in memory.
/// Fixed by collapsing whenever the explicit borrow is shared — asking for
/// less than a field's own ergonomic borrow already grants is always sound —
/// and leaving the reverse direction (`&mut` requested through a field only
/// ergonomically `&`) on its prior, separately-tracked path.
///
/// Pinned rather than excluded, unlike `10_loops.science`'s nondeterministic
/// address: this value was **stable**, so pinning it caught the fix as a
/// diff rather than losing the moment silently. Kept pinned now that it
/// reads `8`, for the same reason every other line is: a pinned expectation
/// is not a promise the value is correct, only a promise that a change to it
/// will be noticed, and this test's own name is that promise's other half —
/// no expectation here is ever paired with nothing.
///
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
