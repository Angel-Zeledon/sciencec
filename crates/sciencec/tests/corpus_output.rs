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

/// **Why an example is not compared against a pinned output.** There are two
/// reasons and they are not the same kind of fact, so they are not the same
/// variant: one is out of the denominator and one is counted against it.
#[derive(PartialEq, Eq)]
enum Why {
    /// **Out of the denominator.** There is no program here to run, by a
    /// decision this repository has already argued at length and pinned with
    /// its own tests. A row of this kind is not a gap, and making the file
    /// build would not be progress — it would mean the decision was reversed
    /// without anyone saying so, which is why
    /// [`every_example_that_builds_has_its_output_pinned`] fails loudly if it
    /// ever happens.
    NotAProgram,
    /// **In the denominator, and counted against it.** A program this corpus
    /// is meant to run and this compiler cannot build yet. The number every
    /// handoff quotes goes up by one the day this row is deleted.
    NotYet,
}

/// One example this run does not measure.
struct Unmeasured {
    /// The file name, not a path: the loop below matches on exactly this.
    file: &'static str,
    why: Why,
    /// One sentence, and where the long argument for it lives. A reason that
    /// is only a number is the thing this table exists to prevent.
    reason: &'static str,
}

/// **Every example this corpus does not measure, named, with its reason — and
/// therefore the corpus's denominator, which is this table and the directory
/// listing and nothing else.**
///
/// # The hole this closes
///
/// `Verdict::NotBuilt` used to reach a bare `continue`. An example that did
/// not build was not measured and *nothing said so*: no bucket, no assertion,
/// no name anywhere. The module doc above already explains why an abort and a
/// hang each got a bucket of their own rather than being folded into "does not
/// build"; the arm they were folded into was itself invisible, and no amount
/// of care about the other three outcomes helps when the fourth is silence.
///
/// It was not hypothetical. `20_extern.science` had a forty-line carve-out
/// comment and two tests pinning it by name, and everybody described the
/// exclusion as *two files*. It was **three**: `00_kitchen_sink.science` has
/// no `.stdout`, no carve-out and no pin, and fell through the same `continue`
/// without appearing in a single message. Three separate documents then
/// quoted three different denominators over it.
///
/// So a file that is not measured is now named here or the run fails. The two
/// properties are checked against reality on every run, in opposite
/// directions, and neither can be satisfied by editing this table alone:
///
/// - a file that does not build and is **not** listed fails the run — a build
///   that stops working can no longer vanish from the count;
/// - a file that **is** listed and nevertheless builds and runs fails the run
///   too — an exclusion cannot outlive its reason, and the day somebody makes
///   `00_kitchen_sink` build, this is the assertion that announces it.
///
/// # Why a pin that is merely *present* proves nothing either
///
/// Making the skip visible is half of it. The other half is that a recorded
/// expectation is only worth what the run behind it was worth: fixing user
/// `Drop::drop` added **eleven lines across three `.stdout` files** —
/// `06_traits` +1, `18_ownership` +8, `19_stdlib` +2 — every one a pure
/// addition, not a single line removed or altered. `19_stdlib.science` prints
/// `"record dropped"` inside a `Drop` block and its committed expectation
/// contained the string zero times. Three pins had been blessed against a
/// real defect and were certifying it. A test that silently agrees with a bug
/// is worse than no test, and the same is true of a denominator that quietly
/// drops the file it cannot explain.
///
/// # The rows
///
/// Sorted by file name, like [`examples`] itself, so the table reads against
/// the directory listing.
const UNMEASURED: &[Unmeasured] = &[
    Unmeasured {
        file: "00_kitchen_sink.science",
        why: Why::NotYet,
        reason: "§11's acceptance program, and the last one by construction rather than by \
                 oversight: it is the union of every remaining gap, not a task. Today it stops \
                 at `SC0400` on the two chains, `headlines` and `by_length` — \
                 `docs.iterate().map(each.title)` and its sibling. §5.4's chain vocabulary is \
                 on `science-resolve`'s `UNWRITTEN` list, so the closure passed to `map` has no \
                 parameter type and the backend refuses it as a value the front end left as \
                 `TyKind::Error`. Nothing else in the file is short: replace those two bodies \
                 with `Array[String].new()` and the whole program builds, links, runs and exits \
                 0. The reason here used to name a tuple — *\"`science-types`' `ExprKind::Tuple` \
                 arm reads each element before inference has defaulted them\"* — and §3's \
                 finding 20 closed that; `let t be (1, 2)` builds. Delete this row when the \
                 chains land — the run will tell you to.",
    },
    Unmeasured {
        file: "17_modules.science",
        why: Why::NotAProgram,
        reason: "it imports `text.parser` and `compiler.frontend.lexer`: crate-local modules, \
                 resolved relative to the entry file's own directory, which deliberately do \
                 not exist. Not a library gap — `crates/sciencec/tests/cli.rs`'s `UNRESOLVED` \
                 argues it out and pins the count of diagnostics at four: the file exists to \
                 show the two `use` forms the spec writes down, and writing an \
                 `examples/text/parser.science` to satisfy it would turn a syntax example into \
                 a fixture and hide the very diagnostic this corpus measures.",
    },
    Unmeasured {
        file: "20_extern.science",
        why: Why::NotAProgram,
        reason: "`extern` declarations, a handle type and three wrapper functions, and \
                 deliberately no `main` — a library in `script-mode.md` §4.2's sense, which \
                 §11's `SC0403` exists to refuse. Pinned from both sides by \
                 `a_pure_declarations_file_has_no_entry_point_by_design` below: `sciencec \
                 test` must refuse it with `SC0403`, and `sciencec check` — the command §11 \
                 names for a library — must accept the same file cleanly.",
    },
];

/// The row for `name`, if this corpus knows it is not measured.
fn unmeasured(name: &str) -> Option<&'static Unmeasured> {
    UNMEASURED.iter().find(|entry| entry.file == name)
}

/// **The corpus's number, and the only arithmetic that produces it.**
///
/// Every document that quotes the number quotes *these* three, and
/// [`no_document_quotes_a_corpus_count_this_file_does_not`] is what keeps that
/// true. Four copies of the denominator coexisted in this repository at once —
/// 20, 21, 22 and a stray 15/20 — because each was typed by hand into prose
/// next to the sentence that needed it, and one of them moved as a *side
/// effect* of a commit about the library catalogue. A number nothing computes
/// is a number nothing can contradict.
struct Tally {
    /// Files in `examples/`.
    total: usize,
    /// [`Tally::total`] less the [`Why::NotAProgram`] rows: the programs this
    /// corpus is trying to run at all.
    denominator: usize,
    /// [`Tally::denominator`] less the [`Why::NotYet`] rows: the programs that
    /// build, link, run, and have their output pinned byte for byte.
    measured: usize,
}

fn tally() -> Tally {
    let total = examples().len();
    let kind = |why: Why| UNMEASURED.iter().filter(|entry| entry.why == why).count();
    let denominator = total - kind(Why::NotAProgram);
    Tally { total, denominator, measured: denominator - kind(Why::NotYet) }
}

/// What running one example through `sciencec test` found.
enum Verdict {
    /// It does not build.
    NotBuilt,
    /// It built and `sciencec test` killed it rather than wait forever — see
    /// [`ran`].
    Hung,
    /// It built and ran, and the **program** exited nonzero — a panic, a
    /// failed `assert`, or a runtime abort. See [`ran`] for why this is not
    /// [`Verdict::NotBuilt`].
    Failed(String),
    /// It built, ran, and exited 0; this is what it printed.
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
/// # Why it runs in an empty directory and not in the repository
///
/// A program in this corpus may **read the filesystem**, and one does:
/// `09_absence_and_failure.science` calls `read_config("science.toml")`.
/// Run from the repository root, what it printed depended on whether a
/// `science.toml` happened to be sitting there — an untracked file, present
/// in the checkout it was first blessed in and absent from a fresh clone.
/// Four of its eleven lines changed with it (`host` against `the file could
/// not be read`), so the pin was measuring the state of somebody's working
/// directory as much as the compiler.
///
/// The fix is to give every example the same surroundings: an empty
/// directory nobody else writes to. A file the corpus does not ship is a
/// file the corpus must not see. The example paths are absolute — [`examples`]
/// builds them from [`repo_root`] — so nothing needs the old working
/// directory, and an example that one day *wants* a file beside it should be
/// given one here, deliberately and by name, rather than inheriting whatever
/// the checkout has lying around.
///
/// # Why a program that *aborts* is not folded into "does not build" either
///
/// The paragraph above fixed where an example runs and left a hole behind it
/// that hid the same class of bug for as long as the fix has existed.
/// `19_stdlib.science` called `required("science.toml")`, which `panic`s when
/// the read fails, so in the empty directory this function hands it the
/// program **aborted** — `SIGABRT`, exit nonzero. Nonzero was the whole of the
/// question asked here, so the abort was read as `Verdict::NotBuilt`, the
/// caller's `continue` skipped it in silence, and
/// `examples/19_stdlib.stdout` — thirty-six lines pinning output that
/// included the contents of an untracked manifest — went uncompared for as
/// long. A stale pin that is never read passes forever, which is the one
/// thing this file exists not to do.
///
/// A build failure and an abort look identical in the exit code and are
/// nothing alike: one is an example the compiler cannot yet handle, which
/// four files in this corpus are on purpose and other tests own, and the other
/// is a program that this compiler built, ran, and got a wrong answer out of.
/// They are told apart by the line `Session::run_test` prints: it reports a
/// verdict — `test … ok` or `test … FAILED (…)` — only for a program it
/// actually launched, and a file that never linked has no such line at all.
fn ran(example: &Path) -> Verdict {
    let empty = std::env::temp_dir().join(format!(
        "science-corpus-{}-{}",
        std::process::id(),
        example.file_stem().expect("a name").to_string_lossy()
    ));
    std::fs::create_dir_all(&empty).expect("a scratch directory to run in");
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(&empty)
        .args(["test", example.to_str().expect("a utf-8 path")])
        .output()
        .expect("the sciencec binary must be runnable");
    let _ = std::fs::remove_dir_all(&empty);
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.contains("did not exit within") {
        return Verdict::Hung;
    }
    if !output.status.success() {
        let verdict =
            text.lines().find(|line| line.starts_with("test ") && line.contains("FAILED"));
        return match verdict {
            Some(line) => Verdict::Failed(line.trim().to_owned()),
            None => Verdict::NotBuilt,
        };
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
    let mut aborted = Vec::new();
    let mut unnamed = Vec::new();
    let mut stale = Vec::new();

    for example in examples() {
        let name = example.file_name().expect("a name").to_string_lossy().into_owned();
        // Is this one of the files [`UNMEASURED`] names, and why? Asked before
        // the verdict rather than inside one arm, because the answer changes
        // what *every* outcome means: for a listed file, building is the
        // failure and not building is the expectation, and for every other
        // file it is the other way round.
        let excused = unmeasured(&name);
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
        //
        // **`20_extern.science` is never in this loop's `Printed` arm, and
        // that is by design, not a regression.** It used to be argued here, in
        // a comment forty lines long that no run could read, beside a
        // `continue` that treated it exactly like a file nobody had thought
        // about at all. The argument now lives in [`UNMEASURED`] — where it is
        // one row of three rather than the only case anyone had written down
        // — and `a_pure_declarations_file_has_no_entry_point_by_design`, below,
        // is still the assertion that keeps both halves of it true. If that
        // test starts failing, the fix is almost certainly `main.rs`'s help
        // text or a doc, not this file.
        let actual = match ran(&example) {
            Verdict::NotBuilt => {
                // It does not build. Its expectation, if any, is stale — but
                // removing it here would make a regression look like a
                // blessing, so it is left alone and the build failure is the
                // other tests' business.
                //
                // **Which other test, though?** That question had no answer
                // for as long as this arm was a bare `continue`: a file that
                // stopped building simply left the measurement, and only a
                // test that already names it could notice. So the answer is
                // now required to exist before the skip is allowed.
                if excused.is_none() {
                    unnamed.push(name);
                }
                continue;
            }
            Verdict::Hung => {
                // It builds and does not exit. `wrong`/`unpinned` are about
                // what a program prints; a hang has nothing to compare, which
                // is `7674487`'s whole lesson, so it gets its own bucket and
                // its own assertion below rather than being silently read as
                // "does not build".
                match excused {
                    Some(entry) => stale.push(format!(
                        "{name} — builds now, and hangs\n  listed as not measured because: {}",
                        entry.reason
                    )),
                    None => hung.push(name),
                }
                continue;
            }
            Verdict::Failed(verdict) => {
                // It builds, runs, and the program itself exits nonzero. Its
                // output is a prefix of the run that was meant to happen, so
                // comparing it against the pin would report a diff whose first
                // line is true and whose cause is somewhere else; the abort is
                // the finding. Same reasoning as the arm above, same shape.
                match excused {
                    Some(entry) => stale.push(format!(
                        "{name} — builds now, and aborts: {verdict}\n  listed as not measured \
                         because: {}",
                        entry.reason
                    )),
                    None => aborted.push(format!("{name}\n  {verdict}")),
                }
                continue;
            }
            Verdict::Printed(text) => match excused {
                // It builds, runs and exits 0, and [`UNMEASURED`] says it
                // cannot. One of the two is wrong and it is not the compiler:
                // either somebody closed the gap — in which case this is the
                // good news, and the row goes — or a decision this repository
                // argued out has quietly been reversed.
                Some(entry) => {
                    stale.push(format!(
                        "{name} — builds, runs and exits 0\n  listed as not measured because: {}",
                        entry.reason
                    ));
                    continue;
                }
                None => text,
            },
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
        unnamed.is_empty(),
        "these examples do not build, and nothing in this repository says so. That is the one \
         way a program can leave this measurement without anyone noticing, and it is how \
         `00_kitchen_sink.science` sat outside a corpus everybody described as having two \
         exclusions. If the file is meant to build, this is a regression and the build failure \
         is the finding; if it is not, add a row to `UNMEASURED` above with the reason, and \
         the number will account for it: {unnamed:?}"
    );
    assert!(
        stale.is_empty(),
        "these examples are listed in `UNMEASURED` as not measured, and this compiler built \
         them anyway. An exclusion that outlives its reason is how a denominator drifts, so \
         it is a failure and not a pass — but read it before you fix it, because the usual \
         cause is that somebody closed the gap. If so: delete the row, bless the output with \
         `SCIENCE_BLESS=1 cargo test -p sciencec --features llvm --test corpus_output`, and \
         update the three documents `no_document_quotes_a_corpus_count_this_file_does_not` \
         names. If instead a decision was reversed without being argued, the fix is not \
         here:\n{}",
        stale.join("\n")
    );
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
    assert!(
        aborted.is_empty(),
        "these examples build and run and the program exits nonzero. That is not `does not \
         build` — the compiler produced this program — and it is not a wrong pin either, \
         because a run that ends early has no full output to compare, so the pin beside it \
         stops being read at all. Either the example depends on something this empty \
         directory does not have, in which case the example must make what it reads rather \
         than expect a checkout to have it, or the compiler got a wrong answer:\n{}",
        aborted.join("\n")
    );
}

/// **`examples/20_extern.science` failing `sciencec test` is not the corpus
/// measuring a broken example — it would be the corpus measuring the wrong
/// thing if it treated this file the way it treats every other one.**
///
/// The loop above's `Verdict::NotBuilt` arm is silent by design: for every
/// *other* file, a build failure is a regression and "the other tests'
/// business" to report. This file is the one the corpus carries specifically
/// to show that rule has an exception, and the exception needs its own
/// assertion rather than a comment nobody runs.
///
/// `20_extern.science`'s own header says why a build can never succeed here:
/// *"Nothing in this file is safe to call. That is the point of §0 of the
/// design note: the declaration records a claim the compiler will never
/// check, the `unsafe` marks it as such, and the interface a program is
/// meant to use is the hand-written wrapper at the bottom."* It is `extern`
/// declarations, a handle type and three wrapper functions, and deliberately
/// no `main` — `crates/sciencec/src/driver.rs`'s `emit_executable` names this
/// exact file at its own `SC0403` call site as the reason that refusal has a
/// caller at all. `script-mode.md` §4.2 rule 3 makes a file with neither
/// top-level statements nor `def main` a *library*, and §11 gives `SC0403`
/// exactly so `sciencec build`/`test` can refuse one instead of asking a user
/// to "upgrade their toolchain" for a program that was never going to exist.
///
/// So there are two correct facts about this file, not one, and this test
/// pins both: `sciencec test` refuses it, by `SC0403` and nothing else, and
/// `sciencec check` — the command §11's own note names for a library —
/// accepts the same file cleanly. A regression here is not "this file broke";
/// it is either a stray entry point that crept into a declarations-only
/// showcase, or `SC0403` no longer firing where it must.
#[test]
fn a_pure_declarations_file_has_no_entry_point_by_design() {
    let example = repo_root().join("examples").join("20_extern.science");
    assert!(example.is_file(), "examples/20_extern.science must exist");
    let path = example.to_str().expect("a utf-8 path");

    let test_run = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(["test", path])
        .output()
        .expect("the sciencec binary must be runnable");
    assert!(
        !test_run.status.success(),
        "20_extern.science has deliberately no `main`; `sciencec test` building it would mean \
         either a stray entry point crept in or SC0403 stopped firing"
    );
    let stderr = String::from_utf8_lossy(&test_run.stderr);
    assert!(
        stderr.contains("SC0403"),
        "expected the no-entry-point refusal (SC0403) and nothing else, got:\n{stderr}"
    );

    let check_run = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(["check", path])
        .output()
        .expect("the sciencec binary must be runnable");
    assert!(
        check_run.status.success(),
        "`sciencec check` is the command §11 names for a library and should accept a pure \
         declarations file cleanly:\n{}",
        String::from_utf8_lossy(&check_run.stderr)
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

/// **The corpus is nineteen of twenty, and the twenty is twenty-two less the
/// two files that are not programs.**
///
/// This test's name carries the number on purpose. It is the first thing every
/// handoff in this repository states, and it moved once as an unreviewed side
/// effect of a commit about something else: `c2dcb7a` reported *14 of 22*, and
/// fourteen hours later `4733b9c` — a commit about the library catalogue —
/// introduced *"the denominator is 20 and not 22"* into `docs/DREAM.md` with
/// no other mention of the corpus in it. Neither number was wrong for what it
/// counted. Nothing anywhere said which one the repository meant, so both
/// survived, and by the time anybody looked there were four.
///
/// So the arithmetic is written down once, derived from [`UNMEASURED`] and the
/// directory, and pinned here. Every part of it can move, and each way of
/// moving fails differently:
///
/// - **an example is added or deleted** — `total` moves, and the corpus grew
///   or shrank, which is a reviewable change to the thing being measured;
/// - **a [`Why::NotAProgram`] row is added or deleted** — the *denominator*
///   moves, which is the change that must never happen quietly, because it
///   changes what the number means rather than what it reports;
/// - **a [`Why::NotYet`] row is deleted** — the numerator moves, and that is
///   the good day: the message below says so, and the fix is this file plus
///   the three documents.
///
/// A row cannot be deleted to make the number look better, either.
/// [`every_example_that_builds_has_its_output_pinned`] runs every file, so a
/// deleted row for a file that still does not build fails there as `unnamed`,
/// and a row kept for a file that now builds fails there as `stale`. This test
/// is the count; that one is whether the count is true.
#[test]
fn the_corpus_is_nineteen_of_twenty() {
    let counts = tally();

    for entry in UNMEASURED {
        assert!(
            repo_root().join("examples").join(entry.file).is_file(),
            "`UNMEASURED` names `{}`, which is not a file in `examples/`. An exclusion for a \
             file that does not exist shrinks the denominator for free",
            entry.file
        );
        assert!(
            !entry.reason.trim().is_empty(),
            "`{}` is excluded with no reason. A row here is the only thing standing between an \
             unmeasured example and silence, so a row with nothing in it is worse than no row \
             at all",
            entry.file
        );
        assert_eq!(
            UNMEASURED.iter().filter(|other| other.file == entry.file).count(),
            1,
            "`{}` is listed twice in `UNMEASURED`, so it is subtracted from the denominator \
             twice",
            entry.file
        );
    }

    assert_eq!(
        counts.total, 22,
        "`examples/` holds {} files and this test says 22. If the corpus gained or lost a \
         program that is fine and deliberate — update this number, and the three documents \
         `no_document_quotes_a_corpus_count_this_file_does_not` names, in the same commit",
        counts.total
    );
    assert_eq!(
        counts.denominator, 20,
        "the denominator is {} and this test says 20. It is `examples/`'s {} files less the \
         `Why::NotAProgram` rows of `UNMEASURED`, and moving it changes what every reported \
         number *means*. Two files are in that class today and both decisions are argued and \
         separately pinned — `17_modules` by `cli.rs`'s `UNRESOLVED`, `20_extern` by \
         `a_pure_declarations_file_has_no_entry_point_by_design`. A third needs the same",
        counts.denominator, counts.total
    );
    assert_eq!(
        counts.measured, 19,
        "the corpus now measures {} of {} and this test says 19. If an example started \
         building, this is the good failure: rename this test, fix the number in `AGENTS.md`, \
         `NEXT-SESSION.md` and `docs/DREAM.md`, and say so in the commit message — that is the \
         review the number never got",
        counts.measured, counts.denominator
    );
}

/// The documents that quote the corpus's number, relative to [`repo_root`].
///
/// The first three are where the four coexisting denominators were found.
/// `examples/README.md` quotes none today and is scanned anyway: it is the
/// corpus's own front door and the obvious fifth place for one to appear.
const COUNTING_DOCS: &[&str] =
    &["AGENTS.md", "NEXT-SESSION.md", "docs/DREAM.md", "examples/README.md"];

/// The documents that must quote it, so the reconciliation cannot be performed
/// by deleting the number instead of correcting it.
const MUST_COUNT: &[&str] = &["AGENTS.md", "NEXT-SESSION.md", "docs/DREAM.md"];

/// The nouns that make a number a claim about this corpus rather than about
/// anything else a document counts. Longest first, so `ejemplos` is not read as
/// `ejemplo` with a stray `s` after it.
const CORPUS_NOUNS: &[&str] = &[
    "ejemplos", "ejemplo", "examples", "example", "programas", "programa", "programs", "program",
];

/// `El denominador es 20`, and the one English spelling of it. Checked
/// separately because this phrase is how the denominator is *argued* rather
/// than how it is quoted, and it is the sentence that went stale.
const DENOMINATOR_PHRASES: &[&str] = &["denominador es ", "denominator is "];

/// A sentence in a document that states one of [`Tally`]'s numbers.
enum Claim {
    /// `22 programs`, `22 programas` — how many files the corpus holds.
    Size(usize),
    /// `19 of 20 examples`, `19 de 20 ejemplos`, `19/20 ejemplos` — the
    /// headline.
    Ratio(usize, usize),
    /// `the denominator is 20`, `el denominador es 20`.
    Denominator(usize),
}

fn digits(bytes: &[u8], from: usize) -> (usize, usize) {
    let mut end = from;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let text = std::str::from_utf8(&bytes[from..end]).expect("ascii digits");
    (text.parse().expect("a number that fits"), end)
}

/// Does a corpus noun start at `at`, as a whole word?
fn noun_at(bytes: &[u8], at: usize) -> bool {
    CORPUS_NOUNS.iter().any(|noun| {
        let end = at + noun.len();
        bytes.len() >= end
            && &bytes[at..end] == noun.as_bytes()
            && (end == bytes.len() || !bytes[end].is_ascii_alphanumeric())
    })
}

/// The offset just past the separator of `N de M` / `N of M` / `N/M`.
fn separator_at(bytes: &[u8], at: usize) -> Option<usize> {
    [" de ", " of ", "/"].into_iter().find_map(|separator| {
        let end = at + separator.len();
        (bytes.len() >= end && &bytes[at..end] == separator.as_bytes()).then_some(end)
    })
}

/// Every claim a document makes about the size of this corpus, with the line it
/// is on.
///
/// Deliberately literal, and deliberately not a regular expression: this
/// workspace has one third-party dependency by a decision `sciencec`'s own
/// `Cargo.toml` argues at length, and a short scan in the one test that needs
/// it is cheaper than a second.
///
/// A digit run only begins a claim at a word boundary. `§4.4 example` in
/// `examples/README.md` is the case that matters — without the rule its `4`
/// reads as a count of four programs — and `2026-09-22` and the right-hand side
/// of `17/22` are the same problem. A number with no corpus noun after it is
/// not a claim at all: `2329 en verde` is a test count, and `10/22` in a
/// sentence about a past mismeasurement is history. Neither is this file's
/// business, and widening the scan to catch them would make it fire on prose it
/// has no opinion about.
fn claims(text: &str) -> Vec<(usize, Claim)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut line = 1usize;
    let mut at = 0usize;
    while at < bytes.len() {
        if bytes[at] == b'\n' {
            line += 1;
            at += 1;
            continue;
        }
        if !bytes[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let boundary = at == 0 || !matches!(bytes[at - 1], b'0'..=b'9' | b'.' | b'-' | b'/' | b'_');
        let after_phrase = DENOMINATOR_PHRASES.iter().any(|phrase| {
            at >= phrase.len()
                && bytes[at - phrase.len()..at].eq_ignore_ascii_case(phrase.as_bytes())
        });
        let (first, after) = digits(bytes, at);
        at = after;
        if after_phrase {
            found.push((line, Claim::Denominator(first)));
            continue;
        }
        if !boundary {
            continue;
        }
        if let Some(second_at) = separator_at(bytes, after) {
            if second_at < bytes.len() && bytes[second_at].is_ascii_digit() {
                let (second, after_second) = digits(bytes, second_at);
                if bytes.get(after_second) == Some(&b' ') && noun_at(bytes, after_second + 1) {
                    found.push((line, Claim::Ratio(first, second)));
                    at = after_second;
                    continue;
                }
            }
        }
        if bytes.get(after) == Some(&b' ') && noun_at(bytes, after + 1) {
            found.push((line, Claim::Size(first)));
        }
    }
    found
}

/// **No document states a corpus count that [`tally`] does not produce.**
///
/// An audit found four denominators coexisting in this repository — 20 in
/// `NEXT-SESSION.md`, `docs/DREAM.md` and `AGENTS.md`, 21 in a commit body, 22
/// in `NEXT-SESSION.md` and in the directory itself, and a stray 15/20 — plus
/// two numerators, 18 and 17, each true on the day it was typed and false by
/// the time anybody read it. `AGENTS.md` said `examples/` holds twenty
/// programs; it holds twenty-two.
///
/// None of that was carelessness. It is what hand-copied numbers do, and the
/// only repair that lasts is to stop the copies from being independent. So the
/// count lives in [`UNMEASURED`], [`tally`] derives it, and this test reads the
/// documents back: a sentence that quotes the number must quote *this* number,
/// and the three that carry it must not stop carrying it.
///
/// The scan is narrow on purpose. It reads a number immediately followed by a
/// word for a program, and the sentence that states the denominator outright —
/// the two shapes the drift actually took. A projection (`00` closes and the
/// corpus is 20/20) is not a claim about today and does not use them.
#[test]
fn no_document_quotes_a_corpus_count_this_file_does_not() {
    let counts = tally();
    let mut wrong = Vec::new();

    for doc in COUNTING_DOCS {
        let path = repo_root().join(doc);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|why| panic!("{doc} is a document this corpus counts in: {why}"));
        let found = claims(&text);
        if MUST_COUNT.contains(doc) && found.is_empty() {
            wrong.push(format!(
                "{doc}: states no corpus count at all. It is one of the documents whose \
                 opening number is the first thing a reader takes from this repository, and \
                 reconciling the numbers by deleting them is not reconciling them"
            ));
        }
        for (line, claim) in found {
            match claim {
                Claim::Size(held) if held != counts.total => wrong.push(format!(
                    "{doc}:{line}: says the corpus holds {held} programs; `examples/` holds {}",
                    counts.total
                )),
                Claim::Ratio(measured, denominator)
                    if (measured, denominator) != (counts.measured, counts.denominator) =>
                {
                    wrong.push(format!(
                        "{doc}:{line}: says {measured} of {denominator}; `UNMEASURED` says {} of \
                         {}",
                        counts.measured, counts.denominator
                    ))
                }
                Claim::Denominator(denominator) if denominator != counts.denominator => wrong
                    .push(format!(
                        "{doc}:{line}: calls the denominator {denominator}; `UNMEASURED` makes \
                         it {}",
                        counts.denominator
                    )),
                _ => {}
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "a document quotes a corpus count this repository does not measure. The number is \
         computed by `tally` in this file, from `UNMEASURED` and the directory listing; these \
         sentences disagree with it, and a reader has no way to tell which is current:\n{}",
        wrong.join("\n")
    );
}
