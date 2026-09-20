//! End-to-end tests: the real binary, over real files, asserting on stdout,
//! stderr and the exit code.
//!
//! This is the first place the whole front half runs in one pass. The lexer
//! checks `examples/` and the parser checks `examples/`, each on its own; until
//! there was a binary nothing ran text → tokens → AST → resolved names over
//! them together. Doing so found something, and it is pinned below rather than
//! papered over: see [`UNRESOLVED`].
//!
//! It has since found something twice more, each time from adding the next
//! phase, and each time the finding is pinned here rather than suppressed.
//! [`REGIONS`] is the second list: what the borrow check reports over
//! `examples/`, with each diagnostic marked real or false and with what would
//! close it.
//!
//! Every command runs with the repository root as its working directory,
//! because that is what makes the `-->` path in a diagnostic read
//! `examples/01_functions.science` on every platform.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Runs `sciencec` from the repository root.
fn sciencec(args: &[&str]) -> Run {
    let output = Command::new(env!("CARGO_BIN_EXE_sciencec"))
        .current_dir(repo_root())
        .args(args)
        .output()
        .expect("the sciencec binary must be runnable");
    Run::new(args, output)
}

struct Run {
    command: String,
    code: i32,
    stdout: String,
    stderr: String,
}

impl Run {
    fn new(args: &[&str], output: Output) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
        assert!(
            !stderr.contains("panicked at"),
            "`sciencec {}` panicked:\n{stderr}",
            args.join(" ")
        );
        Run {
            command: format!("sciencec {}", args.join(" ")),
            code: output.status.code().expect("the process must exit normally"),
            stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
            stderr,
        }
    }

    #[track_caller]
    fn succeeded(&self) -> &Self {
        assert_eq!(self.code, 0, "`{}` should exit 0\nstderr:\n{}", self.command, self.stderr);
        self
    }

    #[track_caller]
    fn failed(&self) -> &Self {
        assert_eq!(self.code, 1, "`{}` should exit 1\nstderr:\n{}", self.command, self.stderr);
        self
    }

    #[track_caller]
    fn silent_stderr(&self) -> &Self {
        assert_eq!(self.stderr, "", "`{}` should report nothing", self.command);
        self
    }

    #[track_caller]
    fn stderr_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stderr.contains(needle),
            "`{}` should mention `{needle}`\nstderr:\n{}",
            self.command,
            self.stderr
        );
        self
    }

    /// The trailing `3 errors, 1 warning` line.
    fn summary(&self) -> Option<&str> {
        self.stderr.trim_end().lines().last().filter(|line| {
            line.contains(" error") || line.contains(" warning") || *line == "1 error"
        })
    }
}

fn examples() -> Vec<PathBuf> {
    science_files(&repo_root().join("examples"))
}

fn ui_cases() -> Vec<PathBuf> {
    science_files(&repo_root().join("tests").join("ui"))
}

fn science_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{} must exist: {e}", dir.display()))
        .map(|entry| entry.expect("a readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "science"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} has no .science files", dir.display());
    files
}

/// `examples/<name>`, the way the command line spells it.
fn relative(path: &Path) -> String {
    format!("examples/{}", path.file_name().unwrap().to_string_lossy())
}

// --- the command surface -------------------------------------------------

#[test]
fn version_and_help_succeed_and_go_to_stdout() {
    let version = sciencec(&["--version"]);
    version.succeeded().silent_stderr();
    assert!(version.stdout.starts_with("sciencec "), "{}", version.stdout);

    let help = sciencec(&["--help"]);
    help.succeeded().silent_stderr();
    for command in ["check", "tokens", "ast", "resolve"] {
        assert!(help.stdout.contains(command), "--help should list `{command}`");
    }
}

#[test]
fn no_arguments_prints_the_usage_and_fails() {
    sciencec(&[]).failed().stderr_contains("Usage:");
}

#[test]
fn an_unknown_command_is_named() {
    sciencec(&["frobnicate", "examples/01_functions.science"])
        .failed()
        .stderr_contains("unknown command `frobnicate`");
}

#[test]
fn an_unknown_option_is_named() {
    sciencec(&["check", "--jobs", "examples/01_functions.science"])
        .failed()
        .stderr_contains("unknown option `--jobs`");
}

// --- check ---------------------------------------------------------------

#[test]
fn a_clean_check_says_nothing_at_all() {
    let run = sciencec(&["check", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert_eq!(run.stdout, "", "check writes nothing to stdout");
}

/// Every program in `examples/` passes lexing and parsing, end to end through
/// the binary, with nothing reported and an exit code of 0.
///
/// `ast` is the command that runs exactly those two phases — see
/// [`UNRESOLVED`] for why the full `check` is a separate test.
#[test]
fn every_example_is_clean_through_lexing_and_parsing() {
    for path in examples() {
        let name = relative(&path);
        let run = sciencec(&["ast", &name]);
        run.succeeded().silent_stderr();
        assert!(run.stdout.starts_with("Module @"), "{name} should dump a module");
    }
}

/// The examples the front half cannot yet resolve, with the number of errors
/// each produces.
///
/// **This is a finding, not a fixture.** Running the whole front half over
/// `examples/` for the first time — which nothing did before this driver
/// existed, because the lexer and parser test the corpus separately and
/// `science-resolve` tests hand-built trees — showed that
/// `crates/science-resolve/src/builtins.rs` was missing names the examples
/// use. Six files failed. The prelude has since gained the ten operator
/// interfaces of §5.4, the half-precision primitives `F16` and `BF16`, the
/// free function `write`, and the C scalar vocabulary of
/// `ffi-c-boundary.md` §1.3, and four of the six now pass.
///
/// `20_extern.science` was the sixth, and the two things it needed have since
/// been built: the prelude now has a *module* — `ffi`, holding the closed
/// vocabulary of `ffi-c-boundary.md` §1.3 — so `ffi.Span` resolves like any
/// two-segment path, and a foreign function is a `DefKind` of its own, so
/// named arguments at an `extern` call site are a call rather than `SC0206`,
/// which is what §1.7 permits them to be. It is off this list.
///
/// The one that remains is **not** a prelude gap, and a longer list will not
/// fix it: `17_modules.science` imports `text.parser` and
/// `compiler.frontend.lexer`, which are not files in this repository.
///
/// **It was seven and it is four, and the four are not going to become zero
/// here.** This entry used to say the file *"needs a multi-file crate —
/// `resolve_crate`'s job, and a driver command that does not exist yet"*. The
/// driver has it now: `use` loads files, `sciencec check FILE` compiles the
/// crate rooted at `FILE`'s directory, and
/// `a_module_a_use_names_is_compiled_with_it` below is a two-file crate that
/// checks clean through every phase. What that cannot do is conjure
/// `examples/text/parser.science`, and **writing one would be the wrong
/// repair**: the file's header says it exists to show the two `use` forms the
/// spec writes down, and inventing a `text.parser` to satisfy it would turn a
/// syntax example into a fixture and hide the diagnostic this corpus is here
/// to measure.
///
/// So what changed is not the verdict but the report. Three of the seven were
/// a **cascade** — `Token`, `lex` and `text` reported missing a second time,
/// at the uses of names the failed `use` never got to define — and a failed
/// import now poisons what it promised, so the four that remain are the four
/// `use` lines the file actually wrote. Each of them now says which two files
/// were looked for and what the crate root holds instead, which is the
/// difference between *"there is no module `text`"* and a message somebody can
/// act on.
///
/// The counts are exact on purpose: closing a gap breaks this test, which is
/// the point — the list has to shrink deliberately rather than rot.
const UNRESOLVED: &[(&str, usize)] = &[("17_modules.science", 4)];

/// One example's region diagnostics, and whether each is a fact about the
/// program or a fact about a hole in the compiler.
struct Finding {
    file: &'static str,
    /// Every code the file reports, in rendering order.
    codes: &'static [&'static str],
    /// **Whether the program is genuinely wrong.**
    ///
    /// Pinned rather than left to a comment, because a false positive that is
    /// only *described* as one in prose is indistinguishable, to the next
    /// reader and to the next test, from a real finding somebody forgot to fix.
    real: bool,
    /// What has to change for this entry to disappear.
    closed_by: &'static str,
}

/// **The corpus after region inference was wired in**, and the second kind of
/// known gap this file records — `UNRESOLVED`'s discipline, applied to
/// diagnostics rather than to files.
///
/// Before this, `sciencec check examples/*.science` reported nothing but
/// `17_modules.science`'s seven. `crates/science-regions/tests/corpus.rs` had
/// already run the borrow check over the same directory out of tree and
/// measured what wiring it in would produce: **three diagnostics in two files,
/// one real and two false.** That census and this one are the same measurement
/// taken from two sides — that crate's harness builds the pipeline by hand,
/// this one runs the binary — and they must agree exactly, because a
/// disagreement would mean the driver runs a different pipeline than the crate
/// tests.
///
/// **It is now two, and both are false**, because the real one closed between
/// that census and this wiring. `science-types` now auto-borrows a `borrowed T`
/// parameter, so `describe(doc)` in `00_kitchen_sink.science` no longer moves a
/// value an `Excerpt` is still borrowing, and its `SC0334` is gone. That is the
/// outcome that census's own assertion message predicted in as many words, and
/// the same crate's `the_kitchen_sink_no_longer_moves_a_value_that_is_still_borrowed`
/// is the record of it — including the control that separates *"the gap
/// closed"* from *"the check stopped working"*, which is that the call still
/// moves something, and what it now moves is the auto-borrow's temporary rather
/// than the user's binding.
///
/// **The two false positives are not suppressed, and that is the decision.**
/// A compiler that reports two false positives is telling the truth about a
/// hole in itself; one that hides them to keep a corpus green is not, and this
/// project's rule against blessing output nobody read applies to silence as
/// much as to noise. What is owed instead is that nobody later mistakes one for
/// the other, which is what `real` is for — a verdict a test can check rather
/// than a sentence in a comment that the next reader has to believe.
///
/// `driver::region_check`'s doc comment says precisely what closes them. The
/// short version is in `closed_by`, and the day it lands this test fails, which
/// is the point.
/// **Empty, and it was not always.** It held two `SC0333` against
/// `09_absence_and_failure.science`, both false: `lookup(settings, key)` is
/// `settings.get(key)`, `Map.get` had no declaration, so the callee was opaque
/// and the region engine assumed it might return a reference into *every*
/// argument — the key as well as the map. That entry's `closed_by` read
/// *"`Map.get` becoming a declaration the method lookup can find"*, and it is
/// what happened. The field earned its place: the prediction was written down
/// before anyone knew when it would be met, and it was met exactly.
///
/// An empty list is a worse guard than a full one, because it is also what a
/// borrow checker that stopped running would produce. What keeps it honest is
/// that the same corpus is walked by `science-regions`'s own census and by the
/// test below, so silence has to be silence in two places at once.
const REGIONS: &[Finding] = &[];

/// Files the *type* checker reports on, which is a third kind of gap.
///
/// **The two that used to be here are closed.** They read *"a value cannot be
/// read out of a borrow"*: `Array.get` is `-> (borrowed T)?` per
/// `stdlib-core.md` §3.6, the prelude declares `Copy` *and* `Clone* with no
/// methods on either, and the language has no dereference operator, so nothing
/// turned a `borrowed Char` into a `Char`. `science-types`' `assign`'s §7 is
/// that spelling — **a borrow of a `Copy` type is assignable to the value** —
/// and `06_traits` and `10_loops` went with it. Neither needed an edit to
/// `examples/`; the decision was the missing thing.
///
/// **What is here instead is the fourth of the same family.**
/// `builtins.rs` now declares `Array of T implements Iterate: type Item is
/// borrowed T` (`collections-and-chains.md` §4), so a `for` binding has a type
/// where it used to be `Ty::ERROR` and agree with everything. `07_generics`'
/// `largest` then shows what it was hiding: `if item > best` compares a
/// `borrowed T` with the `(borrowed T)?` that `items.get(0)` returned, and
/// Decision 6 makes `T?` never coerce to `T`. It is the same shape as
/// `largest`'s *own* signature bug one revision earlier, in the body this time
/// rather than in the return type, and it closes the same way: one presence
/// test. The fix is the corpus's and this entry is its handoff.
///
/// **And the fifth of the family was here, six times, in two files, and it is
/// gone.** It was the first one that was neither the checker being right about
/// a program nor the checker being right about a gap: it was two sentences of
/// `assign.rs` that did not agree, and a declaration landing is what made them
/// meet.
///
/// `BodyChecker::type_receiver` accepted `Record | Choice | Alias | Interface |
/// Union`, and `builtins.rs` allocates `Array`, `Map`, `Box`, `String` and
/// `Chars` as `DefKind::Primitive` — so a call reached through a prelude *type*
/// found no receiver and came back `Ty::ERROR`. That is every `String.new()`,
/// `(Array of T).new()`, `Map.new()` and `Box.new(x)` in `examples/`, silently,
/// in most of the files this test walks. Accepting `Primitive` gives all of
/// them a type.
///
/// What then reported was `Box.new(Doc(..))` in a slot declared `Box of any
/// Summarize`: the argument fixes `T`, the call is `Box of Doc`, and `Box of
/// Doc` reaching `Box of any Summarize` is an unsizing under a type
/// constructor — `assign`'s §4, first of *"three things it deliberately does
/// not reach"*. The same file's §5 says an owned `any Summarize` *"is
/// constructed where it is written — `Box.new(doc)` — and the corpus already
/// writes every one of them that way"*. Both sentences were in `assign.rs` and
/// the corpus obeyed the second.
///
/// **`assign`'s §4a is the sentence that now agrees with both**, and nothing in
/// `examples/` moved to meet it: `Box of C` unsizes to `Box of any I` for the
/// reason §2 already exempts `(borrowed T)?` from reaching `T?` — one element,
/// not rewritten, no allocation, no value changed. `Coercion::UnsizeInBox` is
/// the variant, `science-mir`'s `tests/unsize.rs` is it arriving in MIR on
/// these same two files, and `science-codegen`'s `tests/mono.rs` is the symbol
/// for what it produces being its own.
///
/// **Four files came off this list across the two changes** — `18_ownership`
/// and `19_stdlib`, whose `Box.new` returns a concrete `Box of Doc` and `Box of
/// Record`, when the declaration landed; then `00_kitchen_sink` and
/// `08_dyn_dispatch` when the coercion did.
///
/// **The list is empty and the test still walks every file**, which is the
/// property that makes emptiness readable. `science-types/tests/corpus.rs` pins
/// the identical facts against the library rather than the binary, so silence
/// here has to be silence in two places at once, and `science-mir`'s
/// `the_corpus_unsizes_in_exactly_these_places` counts the six sites *up* on
/// the same corpus — a third place, and the one that does not go quiet when a
/// phase stops running.
/// # It is not empty any more, and what refilled it is a check that started
///
/// `science-types`' `methods` §8a closed a hole this list could not have seen:
/// a prelude type's method surface was open at **every** name, so
/// `"hi".no_such_method()` checked clean and the whole standard library was
/// exempt from the lookup a user type gets. Two files write a method name no
/// note gives, and they are now reported.
///
/// **Each entry carries the code it is pinned for**, which the shape `(file,
/// count, code)` is: the list used to hold `SC0525` alone and hard-coded it in
/// the assertion, and a second kind of finding could not be told from a first
/// one drifting.
///
/// **It held two entries, and they are gone, because the corpus was wrong
/// rather than the compiler.** `19_stdlib.science` wrote `items.get_mut(0)`;
/// `indexing-and-array-literals.md` §1.4 Decision 5 spells the method
/// `get_mutably`, and `builtins.rs` declares that name, not `get_mut`.
/// `21_compiler_shapes.science` wrote `.len()` three times;
/// `collections-and-chains.md` §5's rejected-names table retires it by name —
/// *"`count()` vs `length()` ← `len()`/`count()` — the abbreviation goes"* —
/// and `stdlib-shape-and-packages.md` restates it as *"`length()`, not
/// `len()`"*, while `Array.length()` is declared. Both files now write the
/// name the note gives, and `check` reports nothing on either. The list is
/// empty and the test still walks every file, which is the property that made
/// emptiness readable the first time this list closed.
///
/// **The property that made emptiness readable still holds, in reverse.**
/// `science-types/tests/corpus.rs` pins the identical fact against the
/// library rather than the binary, so a change that un-silences either file
/// here has to un-silence it in two places at once.
const TYPE_CHECKER_FINDINGS: &[(&str, usize, &str)] = &[];

#[test]
fn every_example_is_clean_through_the_whole_front_half_except_the_known_gaps() {
    for path in examples() {
        let name = relative(&path);
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let unresolved = UNRESOLVED.iter().find(|(n, _)| *n == file).map(|(_, count)| *count);
        let regions = REGIONS.iter().find(|it| it.file == file);
        let borrows = TYPE_CHECKER_FINDINGS.iter().find(|(n, _, _)| *n == file);
        assert!(
            unresolved.is_none() || regions.is_none(),
            "{file} cannot be in both lists: a file that does not resolve is never region-checked"
        );
        let run = sciencec(&["check", &name]);

        match (unresolved, regions) {
            (None, None) if borrows.is_some() => {
                let (_, count, code) = borrows.expect("checked just above");
                run.failed();
                let reported = run.stderr.matches("error[SC").count();
                assert_eq!(
                    reported, *count,
                    "{name} reports a different number of errors than pinned
{}",
                    run.stderr
                );
                // The pinned code and nothing else. If this file ever reports a
                // second *kind* of error, the entry is hiding something.
                for line in run.stderr.lines().filter(|l| l.starts_with("error[SC")) {
                    assert!(
                        line.starts_with(code),
                        "{name} is pinned for {code}, which the corpus has to fix, \
                         and reported something else: {line}"
                    );
                }
            }
            (None, None) => {
                run.succeeded().silent_stderr();
            }
            (Some(count), _) => {
                run.failed();
                let reported = run.stderr.matches("error[SC").count();
                assert_eq!(
                    reported, count,
                    "{name} reports a different number of errors than pinned\n{}",
                    run.stderr
                );
                // Only resolution errors, never a lexical or syntax one: the
                // examples are syntactically valid and must stay that way.
                for line in run.stderr.lines().filter(|l| l.starts_with("error[SC")) {
                    assert!(
                        line.starts_with("error[SC02"),
                        "{name} should only fail to resolve names: {line}"
                    );
                }
            }
            (None, Some(finding)) => {
                run.failed();
                let reported: Vec<&str> = run
                    .stderr
                    .lines()
                    .filter(|l| l.starts_with("error[SC"))
                    .filter_map(|l| l[6..].split(']').next())
                    .collect();
                assert_eq!(
                    reported, finding.codes,
                    "{name}'s region findings moved — see `REGIONS`; this entry closes \
                     when {}\n{}",
                    finding.closed_by, run.stderr
                );
                // Ownership codes and nothing else. A resolution or type error
                // appearing here would mean an *earlier* phase regressed and
                // the ordering rule in `driver::type_and_region_check` never
                // let the borrow check run at all.
                for code in &reported {
                    assert!(
                        code.starts_with("SC03"),
                        "{name} should only report ownership errors: {code}"
                    );
                }
            }
        }
    }
}

/// **The verdict, counted — and it is now zero and zero.**
///
/// This test used to assert two false positives and no real finding, and said:
/// *"when `Map.get` acquires a declaration this becomes zero and zero and the
/// test fails. Somebody then has to come back and say so."* `Map.get` acquired
/// a declaration. This is somebody coming back to say so.
///
/// The shape that was uncomfortable is gone: every diagnostic the borrow check
/// adds to `examples/` was about a hole in the compiler rather than about the
/// program, and it was shipped anyway on the argument that the alternative is a
/// phase nobody runs. Shipping it *counted* is what made the repair legible —
/// the entry named what would close it before anyone knew when, and that is
/// what closed it.
///
/// It keeps asserting the pair rather than being deleted, because the
/// interesting number is the **real** one. A real finding appearing here means
/// the borrow checker has caught something in the corpus, and that deserves to
/// break a test and be read, not to be absorbed into a list.
#[test]
fn the_borrow_check_reports_nothing_in_the_corpus() {
    let real: usize = REGIONS.iter().filter(|it| it.real).map(|it| it.codes.len()).sum();
    let false_positives: usize =
        REGIONS.iter().filter(|it| !it.real).map(|it| it.codes.len()).sum();
    assert_eq!((real, false_positives), (0, 0));
}

/// The borrow check is reached, and reaching it is not the same as the file
/// merely failing.
///
/// The `SC0330`–`SC0379` block is `science-regions`'s alone — `science-mir`
/// reports nothing at all, and `region-inference.md` §12 is where the codes are
/// allocated — so an `SC0333` out of the binary is proof that `check` runs the
/// fourth phase and not only the three before it.
///
/// **The program is written here rather than taken from `examples/`** on
/// purpose. Every finding in `REGIONS` is pinned *because* it is expected to
/// move, and a test of whether the phase runs at all must not be hostage to one
/// of them: returning a borrow of a local is rule 5 with no hole anywhere near
/// it, and it will still be an error on the day the containers land.
#[test]
fn check_reports_the_borrow_check() {
    let file = scratch(
        "leaks_a_borrow.science",
        concat!(
            "type Table:\n",
            "    n: I64\n",
            "\n",
            "def leak() -> &Table:\n",
            "    let t be Table(n: 1)\n",
            "    &t\n",
        )
        .as_bytes(),
    );
    let run = sciencec(&["check", &file]);
    run.failed()
        .stderr_contains("error[SC0333]")
        .stderr_contains("is borrowed for longer than")
        // Decision 9's three spans reach the user, which is the half of the
        // phase a wiring could drop without the code going missing.
        .stderr_contains("`t` is borrowed here")
        .stderr_contains("there is no lifetime syntax to widen");
    assert_eq!(run.summary(), Some("1 error"), "stderr:\n{}", run.stderr);
}

/// **The ordering rule, from outside.**
///
/// `driver::type_and_region_check` refuses to region-check a body the type
/// checker rejected, because MIR over `Ty::ERROR` places makes an opaque callee
/// out of every method call and `science-regions`'s `generate` §7 assumes an
/// opaque callee borrows everything — so the borrow check would *manufacture*
/// diagnostics rather than merely miss them.
///
/// The file below is that situation, and it was **measured rather than
/// imagined**: with the skip removed, `sciencec check` on it reports the type
/// error *and* an `SC0333` against the string literal `"host"`. `lookup`'s body
/// calls a method `Table` does not have, so the callee is opaque, so the
/// inferred signature says the result borrows the key as well as the table, so
/// the result outlives the literal's temporary. That is bit for bit the shape
/// of `REGIONS`' two false positives — reached here from a type error instead
/// of from a missing declaration — and the author's only mistake was the
/// misspelled method.
///
/// What must come out is the type error and nothing else.
#[test]
fn a_type_error_stops_the_borrow_check_before_it_invents_anything() {
    let file = scratch(
        "ill_typed.science",
        concat!(
            "type Table:\n",
            "    n: I64\n",
            "\n",
            "def lookup(t: &Table, k: &String) -> &Table:\n",
            "    t.missing(k)\n",
            "\n",
            "def main():\n",
            "    let t be Table(n: 1)\n",
            "    let r be lookup(t, \"host\")\n",
            "    print(r.n)\n",
        )
        .as_bytes(),
    );
    let run = sciencec(&["check", &file]);
    run.failed().stderr_contains("error[SC0532]");
    for line in run.stderr.lines().filter(|l| l.starts_with("error[SC")) {
        assert!(
            !line.starts_with("error[SC03"),
            "regions ran over a body the checker could not type: {line}"
        );
    }
    assert_eq!(run.summary(), Some("1 error"), "stderr:\n{}", run.stderr);
}

/// `17_modules.science` still behaves: it fails to resolve, so neither the type
/// checker nor the borrow check ever sees it.
///
/// **The count moved from seven to four and that is the finding, not a
/// re-baselining.** `UNRESOLVED` says what closed: three of the seven were the
/// unresolved *names* a failed `use` was supposed to have defined, and a failed
/// `use` now poisons them. The four that remain are the file's four `use`
/// lines, which name modules this repository does not contain.
#[test]
fn the_file_that_cannot_resolve_is_unaffected_by_the_phases_below_resolution() {
    let run = sciencec(&["check", "examples/17_modules.science"]);
    run.failed();
    assert_eq!(run.stderr.matches("error[SC").count(), 4, "{}", run.stderr);
    // Every one of them is the `use`, and none is a name the `use` promised.
    assert_eq!(run.stderr.matches("error[SC0202]").count(), 4, "{}", run.stderr);
    assert!(!run.stderr.contains("error[SC03"), "{}", run.stderr);
}

#[test]
fn check_takes_several_files_and_reports_all_of_them() {
    let run = sciencec(&[
        "check",
        "tests/ui/unknown_character.science",
        "tests/ui/unterminated_string.science",
    ]);
    run.failed()
        .stderr_contains("tests/ui/unknown_character.science")
        .stderr_contains("tests/ui/unterminated_string.science");
}

#[test]
fn a_file_that_cannot_be_read_does_not_hide_the_next_one() {
    let run = sciencec(&[
        "check",
        "no/such/file.science",
        "tests/ui/unterminated_string.science",
        "examples/01_functions.science",
    ]);
    run.failed()
        .stderr_contains("cannot read `no/such/file.science`")
        .stderr_contains("tests/ui/unterminated_string.science");
    // The clean file contributed nothing, which is how you can tell it was not
    // skipped along with the broken ones.
    assert!(!run.stderr.contains("examples/01_functions.science"), "{}", run.stderr);
}

#[test]
fn the_summary_counts_every_error_and_is_the_last_line() {
    let single = sciencec(&["check", "tests/ui/unterminated_string.science"]);
    single.failed();
    let one = single.stderr.matches("error[SC").count();
    assert_eq!(single.summary(), Some(plural(one).as_str()), "{}", single.stderr);

    let missing = sciencec(&["check", "no/such/file.science"]);
    missing.failed();
    assert_eq!(missing.summary(), Some("1 error"));
}

fn plural(n: usize) -> String {
    if n == 1 {
        "1 error".to_string()
    } else {
        format!("{n} errors")
    }
}

// --- the UI suite, run through the binary --------------------------------

/// Every program in `tests/ui/` fails, and every error code its `.stderr`
/// expectation contains is still reported by the driver.
///
/// The output is not compared byte for byte — that is the lexer's UI suite's
/// job, and `check` runs more phases than the lexer does — but a code going
/// missing here would mean the driver dropped a diagnostic on the floor.
#[test]
fn every_ui_case_fails_with_the_codes_its_expectation_names() {
    for path in ui_cases() {
        let name = format!("tests/ui/{}", path.file_name().unwrap().to_string_lossy());
        let expectation = std::fs::read_to_string(path.with_extension("stderr"))
            .unwrap_or_else(|e| panic!("{name} must have a .stderr next to it: {e}"));

        let run = sciencec(&["check", &name]);
        run.failed().stderr_contains(&name);

        for code in codes_in(&expectation) {
            run.stderr_contains(&format!("error[{code}]"));
        }
    }
}

/// The distinct `SC0001`-style codes appearing in a rendered expectation.
fn codes_in(text: &str) -> Vec<String> {
    let mut codes: Vec<String> = text
        .match_indices("error[SC")
        .filter_map(|(at, _)| text[at + 6..].split(']').next())
        .map(str::to_string)
        .collect();
    codes.sort();
    codes.dedup();
    assert!(!codes.is_empty(), "an expectation with no codes in it");
    codes
}

// --- the dumps -----------------------------------------------------------

#[test]
fn the_token_dump_uses_the_lexers_snapshot_format() {
    let file = scratch("tokens.science", b"def main():\n    1\n");
    let run = sciencec(&["tokens", &file]);
    run.succeeded().silent_stderr();
    assert_eq!(
        run.stdout,
        "   0..3     Function\n   4..8     Ident(\"main\")\n   8..9     LParen\n   9..10    RParen\n  10..11    Colon\n  11..12    Newline\n  12..16    Indent\n  16..17    Int { value: 1, base: Dec, suffix: None }\n  17..18    Newline\n  18..18    Dedent\n  18..18    Eof\n",
        "the format is `{{:>4}}..{{:<4}}  {{kind:?}}`, as in crates/science-lexer/tests/common/mod.rs"
    );
}

#[test]
fn the_ast_dump_is_the_parsers_dump() {
    let run = sciencec(&["ast", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.starts_with("Module @"), "{}", run.stdout);
    assert!(run.stdout.contains("Fn `greet`"), "{}", run.stdout);
}

#[test]
fn the_resolve_dump_is_the_resolvers_dump() {
    let run = sciencec(&["resolve", "examples/01_functions.science"]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.starts_with("Module `01_functions` #"), "{}", run.stdout);
    // What distinguishes an HIR dump from an AST one: references carry what
    // they resolved to.
    assert!(run.stdout.contains("Path -> `print` #"), "{}", run.stdout);
}

#[test]
fn a_dump_still_goes_to_stdout_when_the_file_has_errors() {
    let run = sciencec(&["tokens", "tests/ui/unknown_character.science"]);
    run.failed().stderr_contains("error[SC0001]");
    assert!(run.stdout.contains("Function"), "the recovered stream is still dumped");
}

#[test]
fn a_dump_command_takes_exactly_one_file() {
    sciencec(&["ast", "examples/01_functions.science", "examples/02_bindings.science"])
        .failed()
        .stderr_contains("ast expects exactly one file");
}

// --- failing to read a file ----------------------------------------------

#[test]
fn a_missing_file_is_a_clean_error() {
    let run = sciencec(&["check", "no/such/file.science"]);
    run.failed().stderr_contains("error: cannot read `no/such/file.science`: no such file");
    assert_eq!(run.stdout, "");
}

#[test]
fn a_directory_is_a_clean_error() {
    let run = sciencec(&["check", "examples"]);
    run.failed().stderr_contains("error: `examples` is a directory, not a source file");
}

#[test]
fn a_file_that_is_not_utf8_is_a_clean_error() {
    // `0xFF` begins no UTF-8 sequence, and the lexer cannot be handed it: the
    // whole compiler indexes source by byte offset into a `str`.
    let file = scratch("not_utf8.science", b"def main():\n    print(\"\xff\")\n");
    let run = sciencec(&["check", &file]);
    run.failed().stderr_contains("is not valid UTF-8").stderr_contains("offset 23");
}

#[test]
fn each_dump_command_fails_cleanly_on_a_missing_file() {
    for command in ["tokens", "ast", "resolve"] {
        let run = sciencec(&[command, "no/such/file.science"]);
        run.failed().stderr_contains("cannot read");
        assert_eq!(run.stdout, "", "{command} should dump nothing");
    }
}

// --- crates of more than one file ---------------------------------------
//
// `use` loads files. The four tests below are the four claims that makes:
// a module a `use` names is compiled with the entry, a whole-module import is
// usable through its path, a file's statements are `SC0213` once another file
// imports it, and a `use` that finds nothing reports once.

/// The thing that did not exist: a crate of two files, checked as one.
///
/// It goes all the way through — resolution, types, MIR, regions — because the
/// interesting failure is not that the second file is found but that it is
/// found *as a module*, with its items in `text.parser` and its signatures
/// visible to every phase below. A clean exit over `parser.lex(s)` is that,
/// asserted end to end.
#[test]
fn a_module_a_use_names_is_compiled_with_it() {
    let entry = scratch_crate(
        "two_files",
        &[
            (
                "text/parser.science",
                "public def lex(source: &String) -> Int:\n    source.length()\n",
            ),
            (
                "main.science",
                "use text.parser (lex)\n\ndef main():\n    print(lex(\"abc\"))\n",
            ),
        ],
    );
    sciencec(&["check", &entry]).succeeded().silent_stderr();
}

/// §4.4's other import form: `use text.parser`, then `text.parser.lex(s)`.
///
/// This is worth its own test because the path is not a path when the parser
/// is done with it — `a.b(c)` is a method call until something knows whether
/// `a` is a module — so the whole-module form of `use` binds a name that
/// nothing could reach until the resolver folded it back.
#[test]
fn a_module_imported_whole_is_reached_through_its_path() {
    let entry = scratch_crate(
        "whole_module",
        &[
            (
                "text/parser.science",
                "public def lex(source: &String) -> Int:\n    source.length()\n",
            ),
            (
                "main.science",
                "use text.parser\n\ndef main():\n    print(text.parser.lex(\"abc\"))\n",
            ),
        ],
    );
    sciencec(&["check", &entry]).succeeded().silent_stderr();
}

/// `script-mode.md` §4.3, from the binary: the same file, twice, with two
/// answers.
///
/// Checking `clean.science` alone is a script and its `print` runs. Checking
/// `plots.science`, which imports it, makes it a module — and a module's
/// statements never run, so they are `SC0213`. The pair is the test, because
/// either half alone would be consistent with the check never firing or with
/// it firing on everything.
#[test]
fn statements_are_a_script_alone_and_an_error_once_imported() {
    let files: &[(&str, &str)] = &[
        ("clean.science", "public def frame_for(n: Int) -> Int:\n    n + 1\n\nprint(\"loaded\")\n"),
        ("plots.science", "use clean (frame_for)\n\ndef main():\n    print(frame_for(1))\n"),
    ];
    let entry = scratch_crate("script_and_module", files);
    let alone = entry.replace("plots.science", "clean.science");

    sciencec(&["check", &alone]).succeeded().silent_stderr();
    sciencec(&["check", &entry])
        .failed()
        .stderr_contains("error[SC0213]")
        .stderr_contains("this would never run")
        .stderr_contains("is imported here, so it is a module, not a script");
}

/// **The decision, pinned: several files on one command line are several
/// crates.**
///
/// `driver::Session::check` argues it — one crate has one entry, and folding
/// the command line into one crate would make every file after the first a
/// non-entry and so report `SC0213` on a directory of ordinary scripts. This
/// is that argument as a test: two files, each with top-level statements, each
/// importing nothing, named together. If the meaning of the command ever
/// changes, this is what says so.
#[test]
fn several_files_on_one_command_line_are_several_crates() {
    let first = scratch_crate(
        "two_scripts",
        &[("one.science", "print(1)\n"), ("two.science", "print(2)\n")],
    );
    let second = first.replace("one.science", "two.science");
    sciencec(&["check", &first, &second]).succeeded().silent_stderr();
}

/// A `use` that finds nothing reports the `use`, and nothing else.
///
/// The discipline is that a file which fails to load must not produce one
/// diagnostic per name it was supposed to define — three uses of two names
/// here, and one diagnostic. `UNRESOLVED`'s count for `17_modules.science` is
/// the same property measured on the corpus; this is it isolated, so that a
/// regression says which of the two it is.
#[test]
fn a_use_that_finds_no_module_does_not_report_the_names_it_promised() {
    let entry = scratch_crate(
        "missing_module",
        &[(
            "main.science",
            "use ghost.tools (Widget, make)\n\n\
             def build() -> Widget:\n    make(1)\n\n\
             def main():\n    print(ghost.tools.make(2))\n",
        )],
    );
    let run = sciencec(&["check", &entry]);
    run.failed().stderr_contains("there is no module `ghost` in the crate root");
    assert_eq!(
        run.stderr.matches("error[SC").count(),
        1,
        "one failed import is one diagnostic\n{}",
        run.stderr
    );
    // The message names what the crate has, not only what it lacks.
    run.stderr_contains("`ghost.science` or `ghost/mod.science`")
        .stderr_contains("the crate root has one module");
}

/// Writes a crate under the target directory and returns the **last** file's
/// path, which every test above writes as its entry.
///
/// A directory of its own per crate, because the crate root is the entry's
/// directory: two crates sharing one would see each other's modules.
fn scratch_crate(name: &str, files: &[(&str, &str)]) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("crates").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the target directory is writable");
    let mut last = dir.clone();
    for (path, text) in files {
        let file = dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("the target directory is writable");
        }
        std::fs::write(&file, text.as_bytes()).expect("the scratch file is writable");
        last = file;
    }
    last.to_string_lossy().replace('\\', "/")
}

/// Writes a file under the target directory and returns its path, relative to
/// the repository root so that it is what the `-->` header shows.
fn scratch(name: &str, bytes: &[u8]) -> String {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sciencec");
    std::fs::create_dir_all(&dir).expect("the target directory is writable");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("the scratch file is writable");
    path.to_string_lossy().replace('\\', "/")
}

// --- tools --json --------------------------------------------------------
//
// The walk is `mcp-servers.md` §14.3's "one predicate": a file's `tool`
// declarations are its tool list. Every case below therefore writes `tool`,
// and every one of them writes a `##` comment above it, because a `tool`
// without one is `SC0190`.

/// The thesis of `mcp-servers.md` Decision 3, run: a JSON Schema derived from
/// a signature, with no schema written anywhere by hand.
///
/// The three assertions below are the three things no other MCP SDK can do,
/// and each is free here because the information was already in the type.
#[test]
fn the_schema_comes_out_of_the_signature() {
    let file = scratch(
        "tools.science",
        b"## Counts events above a threshold.\n\
          tool count_above(samples: Array[F64], floor: U16) -> U64:\n\
          \x20   0\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded().silent_stderr();

    // The width of the integer becomes the bounds of the number. `U16` is
    // 0..65535 and the model is told so.
    assert!(run.stdout.contains(r#""minimum":0,"maximum":65535"#), "{}", run.stdout);
    // The doc comment is the description, which is what a model reads to
    // decide whether to call the tool.
    assert!(run.stdout.contains("Counts events above a threshold."), "{}", run.stdout);
    assert!(run.stdout.contains(r#""name":"count_above""#), "{}", run.stdout);
}

/// A `choice` of unit variants is a string `enum`. `mcp-servers.md` §4.5 calls
/// this the largest single win in the mapping, and the reason is that the
/// author writes nothing: it is how Science already spells alternatives.
#[test]
fn a_choice_of_unit_variants_becomes_an_enum() {
    let file = scratch(
        "enum.science",
        b"choice Lineshape:\n\x20   Gaussian\n\x20   Lorentzian\n\
          ## Fit a line shape to the run.\n\
          tool fit(profile: Lineshape) -> Int:\n\x20   0\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded();
    assert!(
        run.stdout.contains(r#""type":"string","enum":["Gaussian","Lorentzian"]"#),
        "{}",
        run.stdout
    );
}

/// `T?` is the payload's schema with the name absent from `required`.
///
/// §4.4 is explicit that absent and null are different states in JSON and the
/// same state in Science, so this is a lossy direction and the note says so
/// rather than hiding it. What matters is that the parameter is not required.
#[test]
fn a_nullable_parameter_is_not_required() {
    let file = scratch(
        "nullable.science",
        b"## Label a run.\ntool label(name: String, note: String?) -> Int:\n\x20   0\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded();
    assert!(run.stdout.contains(r#""required":["name"]"#), "{}", run.stdout);
    assert!(run.stdout.contains(r#""note":{"type":"string"}"#), "{}", run.stdout);
}

/// `U64` gets no `maximum`, and the reason is arithmetic rather than laziness:
/// 2^64-1 is not exactly representable as a JSON number, so emitting it would
/// be a lie about what the other side can send.
#[test]
fn a_u64_has_no_maximum() {
    let file =
        scratch("u64.science", b"## Total the counts.\ntool total(n: U64) -> Int:\n\x20   0\n");
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded();
    assert!(run.stdout.contains(r#""minimum":0}"#), "{}", run.stdout);
    assert!(!run.stdout.contains("18446744073709551615"), "{}", run.stdout);
}

/// A type with no JSON spelling is a named gap, not a silent one and not an
/// error. Deciding a type is *wrong* rather than *unmapped* needs a checker,
/// and there is not one.
#[test]
fn an_unmappable_parameter_is_named_rather_than_dropped() {
    let file = scratch(
        "gap.science",
        b"interface Summarize:\n\x20   def summarize(self) -> String\n\
          ## Show what something says about itself.\n\
          tool show(what: any Summarize) -> Int:\n\x20   0\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded();
    assert!(run.stdout.contains("x-science-unmapped"), "{}", run.stdout);
    assert!(run.stdout.contains("dispatched at run time"), "{}", run.stdout);
}

/// A program that does not resolve emits no schema at all.
///
/// A schema for a program that does not exist is worse than no schema: a
/// caller cannot tell the two apart.
#[test]
fn a_broken_program_emits_no_schema() {
    let file =
        scratch("broken.science", b"## Do something.\ntool f(x: Nonexistent) -> Int:\n\x20   0\n");
    let run = sciencec(&["tools", "--json", &file]);
    run.failed();
    assert!(run.stdout.is_empty(), "stdout was {:?}", run.stdout);
}

/// **The predicate.** A `def` is not on the wire, and a file of them declares
/// no tools.
///
/// This is the whole of the change §14.3 predicted, seen from outside: before
/// `tool` existed this file produced two entries. It now produces one, and it
/// drops the other *silently* — a function that is not a tool is not a
/// mistake, it is a function.
#[test]
fn a_function_that_is_not_a_tool_is_not_in_the_list() {
    let file = scratch(
        "plain.science",
        b"## Counts events above a threshold.\n\
          def count_above(samples: Array[F64], floor: U16) -> U64:\n\
          \x20   0\n\
          ## Label a run.\n\
          tool label(name: String) -> Int:\n\x20   0\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.contains(r#""name":"label""#), "{}", run.stdout);
    assert!(!run.stdout.contains("count_above"), "{}", run.stdout);
}

#[test]
fn a_file_with_no_tools_yields_an_empty_list() {
    let file = scratch("notools.science", b"def f(x: Int) -> Int:\n\x20   x\n");
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded().silent_stderr();
    assert_eq!(run.stdout, "[]");
}

/// **The old walk, pinned byte for byte.**
///
/// `--all-functions` is the measurement `mcp-servers.md` §2.6's second
/// falsifier needs — a census of the type mapping over code written before the
/// keyword — and a census is worth nothing if the instrument moved under it.
/// So this asserts the whole of stdout rather than a substring: what is
/// emitted for a function that is not a `tool` is what was emitted before
/// `tool` existed, character for character.
#[test]
fn the_old_walk_over_every_function_is_unchanged() {
    let file = scratch(
        "census.science",
        b"## Counts events above a threshold.\n\
          def count_above(samples: Array[F64], floor: U16) -> U64:\n\
          \x20   0\n",
    );
    let run = sciencec(&["tools", "--json", "--all-functions", &file]);
    run.succeeded().silent_stderr();
    assert_eq!(
        run.stdout,
        concat!(
            r#"[{"name":"count_above","inputSchema":{"type":"object","properties":{"#,
            r#""samples":{"type":"array","items":{"type":"number"}},"#,
            r#""floor":{"type":"integer","minimum":0,"maximum":65535}},"#,
            r#""required":["samples","floor"]},"#,
            r#""description":"Counts events above a threshold."}]"#,
        )
    );
}

/// The flag belongs to `tools` alone, the way `--write` belongs to `fmt`.
#[test]
fn the_all_functions_flag_is_not_a_file() {
    let run = sciencec(&["check", "--all-functions"]);
    run.failed();
}

// --- script mode ---------------------------------------------------------
//
// `script-mode.md` §1.1: the top level of a file is a sequence of items *and*
// statements. The parser desugars the statements into a generated `def main()
// -> Error?`, so what these assert is that the three commands that walk what
// the parser produces are handed an ordinary module and behave exactly as they
// do for any other file.
//
// The files are written to the scratch directory rather than to `examples/`,
// because `examples/` is the corpus three other suites measure themselves
// against and a file added there is a change to all of them.

/// **The gap this closed.**
///
/// One line, no `def`, no `main`, no indentation. Before the top level
/// admitted statements this answered ``error[SC0101]: expected `implements` or
/// `has` after `print`, found `(` `` — a syntax error on the first statement
/// of the first program the language would ever run.
#[test]
fn a_one_line_script_checks_clean() {
    let file = scratch("hello.science", b"print(\"hello, world\")\n");
    sciencec(&["check", &file]).succeeded().silent_stderr();
}

/// The file a reader writes second, and the one a careless fix breaks: a
/// declaration and a statement together. The failure mode of a new top-level
/// production is not that the statement is rejected, it is that the
/// declaration beside it is.
#[test]
fn a_declaration_and_a_statement_check_clean_together() {
    let file = scratch(
        "mixed.science",
        b"def double(n: I64) -> I64:\n\x20   n * 2\n\nprint(double(21))\n",
    );
    sciencec(&["check", &file]).succeeded().silent_stderr();
}

/// §2.2, end to end: one diagnostic, and no duplicate definition behind it.
///
/// The script body is not generated when the file declares `main`, which is
/// what keeps one mistake to one message. `tests/ui/parse/script_and_main.*`
/// pins the rendering; this pins the exit code and the count.
#[test]
fn a_script_body_beside_a_declared_main_is_one_error() {
    let file =
        scratch("both.science", b"let x be 1\n\ndef main():\n\x20   print(x)\n");
    let run = sciencec(&["check", &file]);
    run.failed().stderr_contains("SC0117");
    assert_eq!(run.summary(), Some("1 error"), "stderr:\n{}", run.stderr);
}

/// `sciencec fmt` round-trips a script.
///
/// The formatter works on the token stream and never looks at the syntax tree
/// for layout, so a top-level statement lays out like any other line. What it
/// *does* do with the tree is verify its own output — it re-parses and
/// compares trees modulo spans — and that check sees the generated `main` on
/// both sides. It agrees because the desugaring is a deterministic function of
/// the token stream, which is the property that makes a parser-side desugaring
/// safe for a formatter at all.
#[test]
fn fmt_round_trips_a_script_and_is_idempotent() {
    let source = "use data (Record)\n\
                  \n\
                  type Reading:\n\
                  \x20   temperature: F32\n\
                  \n\
                  Reading implements Record\n\
                  \n\
                  let readings be load()\n\
                  print(\"kept:\", readings.len())\n";
    let file = scratch("fmt_script.science", source.as_bytes());
    let first = sciencec(&["fmt", &file]);
    first.succeeded().silent_stderr();
    assert_eq!(first.stdout, source, "formatting a script must not change it");

    let again = scratch("fmt_script_2.science", first.stdout.as_bytes());
    let second = sciencec(&["fmt", &again]);
    second.succeeded().silent_stderr();
    assert_eq!(second.stdout, first.stdout, "`fmt` must be idempotent on a script");
}

/// `sciencec tools --json` still selects exactly the `tool` declarations.
///
/// The generated `main` is a `def`, so the walk that reads the word a function
/// was declared with passes over it without being told anything about scripts.
#[test]
fn tools_json_ignores_the_script_body() {
    let file = scratch(
        "tooled.science",
        b"## Adds two numbers.\n\
          tool add(a: I64, b: I64) -> I64:\n\x20   a + b\n\nprint(add(1, 2))\n",
    );
    let run = sciencec(&["tools", "--json", &file]);
    run.succeeded().silent_stderr();
    assert!(run.stdout.contains(r#""name":"add""#), "{}", run.stdout);
    assert!(!run.stdout.contains("main"), "{}", run.stdout);
}

/// **The cost, pinned rather than hidden.**
///
/// `--all-functions` walks every function in the module, and after the
/// desugaring the generated `main` is one of them. So a census of a script
/// reports a function the author did not write. That is the desugaring's
/// advertised price — later phases are handed a `main` and are told nothing
/// about where it came from — and the honest place for it is a test that says
/// so, not a special case in the walk that would teach `sciencec` the word
/// "script" in order to hide it.
#[test]
fn the_census_of_a_script_names_the_generated_main() {
    let file = scratch("census_script.science", b"print(\"hello\")\n");
    let run = sciencec(&["tools", "--json", "--all-functions", &file]);
    run.succeeded().silent_stderr();
    assert_eq!(
        run.stdout,
        r#"[{"name":"main","inputSchema":{"type":"object","properties":{},"required":[]}}]"#
    );
}

// --- `build` --------------------------------------------------------------

/// `build` reports the program's problems before the toolchain's.
///
/// A file that does not check is not a file anyone is helped by being told which
/// LLVM to install for, and the ordering is the same in both builds of this
/// crate — the back end is not reached when the front end failed.
#[test]
fn a_build_of_a_broken_program_reports_the_program_and_not_the_backend() {
    let file = scratch("build_broken.science", b"print(nonexistent_name)\n");
    let run = sciencec(&["build", &file]);
    run.failed();
    assert!(
        !run.stderr.contains("SC0400"),
        "the backend's absence was reported over the program's error:\n{}",
        run.stderr
    );
}

/// **The message a contributor without LLVM sees, and it must not change by
/// accident.**
///
/// `codegen-and-linking.md` §11 gives `SC0400`'s contract as *"names the feature
/// and how to obtain a build that has it"*, and both halves are asserted: the
/// backend's name, and the two things that actually obtain one — an LLVM 18.1
/// installation and `--features llvm`. The second is the half the message this
/// replaces did not have; it told the reader to set `LLVM_SYS_181_PREFIX`, which
/// is `llvm-sys`'s variable, and `science-codegen-llvm` does not use `llvm-sys`.
///
/// `#[cfg(not(feature = "llvm"))]` because with the feature this command
/// *builds*, which is the other test's business.
#[cfg(not(feature = "llvm"))]
#[test]
fn a_build_without_the_backend_is_sc0400_and_says_what_turns_it_on() {
    let file = scratch("build_hello.science", b"print(\"hello, world\")\n");
    let run = sciencec(&["build", &file]);
    run.failed()
        .stderr_contains("SC0400")
        .stderr_contains("no `llvm` backend is compiled into this `sciencec`")
        .stderr_contains("--features llvm")
        .stderr_contains("18.1");
    assert_eq!(run.summary(), Some("1 error"), "stderr:\n{}", run.stderr);
}

/// **A file with no entry point is `SC0403`, not `SC0400`, and the difference
/// is what the reader is told to do about it.**
///
/// `codegen-and-linking.md` §11 splits the two deliberately. `SC0400` is *"a
/// toolchain feature required to build this program is not compiled into this
/// `sciencec`"* and must name *"how to obtain a build that has it"* — it sends
/// the reader to install something. `SC0403` is the row §11 added for the case
/// where there is nothing to install: *"`script-mode.md` §4.2 rule 3 defines
/// this situation as a library build and does not say what happens when
/// somebody asks for a binary."*
///
/// Before this, the LLVM backend's root-set lookup produced *"this `sciencec`
/// cannot build a program with no `main`"* under `SC0400`, whose note lists the
/// stages this compiler implements — so a user handed a library was told their
/// compiler was too old, and `SC0403` had no caller anywhere in the workspace
/// even though `mono.rs` §7 asserted in prose that `build` already refused this.
///
/// No `#[cfg]`: the refusal is the driver's and comes before any backend, so
/// the sentence is the same with and without `--features llvm`. That is half
/// the point — the answer must not depend on which toolchain the reader has.
#[test]
fn a_build_of_a_file_with_no_entry_point_is_sc0403_and_calls_it_a_library() {
    let file = scratch(
        "build_library_only.science",
        b"public def helper(x: Int) -> Int:\n    x + 1\n",
    );
    let run = sciencec(&["build", &file]);
    run.failed()
        .stderr_contains("SC0403")
        .stderr_contains("has no entry point")
        .stderr_contains("`sciencec check` is the command for one");
    assert!(
        !run.stderr.contains("SC0400"),
        "a library was told to upgrade its toolchain:\n{}",
        run.stderr
    );
    assert_eq!(run.summary(), Some("1 error"), "stderr:\n{}", run.stderr);
}

/// The corpus's own instance of the case above, by name.
///
/// `examples/20_extern.science` is `extern` blocks, a handle type and three
/// hand-written wrappers, and `examples/README.md` calls the corpus *"input
/// data for the lexer and parser test suites"* rather than a set of programs.
/// It has no `main` and is not meant to acquire one: nothing in it is callable
/// without OpenBLAS, HDF5 and cuDNN on the machine, and the file's own header
/// says *"nothing in this file is safe to call"*. So the right outcome is a
/// refusal that says *library*, and this pins it against the file rather than
/// against a fixture, because the fixture cannot go stale and the corpus can.
#[test]
fn the_extern_example_is_refused_as_a_library_and_not_as_a_missing_feature() {
    let run = sciencec(&["check", "examples/20_extern.science"]);
    run.succeeded();
    let run = sciencec(&["build", "examples/20_extern.science"]);
    run.failed().stderr_contains("SC0403").stderr_contains("no entry point");
    assert!(!run.stderr.contains("SC0400"), "{}", run.stderr);
}

/// With the backend, the same command produces a program that prints
/// `hello, world` and exits 0. §10's stage 1 and its gate, through the command
/// line a user actually types.
#[cfg(feature = "llvm")]
#[test]
fn a_build_with_the_backend_produces_a_program_that_runs() {
    let file = scratch("build_run_hello.science", b"print(\"hello, world\")\n");
    let run = sciencec(&["build", &file]);
    run.succeeded().silent_stderr();
    let executable = Path::new(&file).with_extension(if cfg!(windows) { "exe" } else { "" });
    assert!(executable.is_file(), "no executable at {}", executable.display());
    let program = Command::new(&executable).output().expect("the program runs");
    assert_eq!(
        String::from_utf8_lossy(&program.stdout).replace("\r\n", "\n"),
        "hello, world\n"
    );
    assert_eq!(program.status.code(), Some(0));
}

// --- `test` -----------------------------------------------------------

/// `sciencec test` without the backend is `SC0400`, the same message `build`
/// gives and for the same reason: there is no executable to run.
#[cfg(not(feature = "llvm"))]
#[test]
fn a_test_without_the_backend_is_sc0400() {
    let file = scratch("test_hello.science", b"def main():\n    assert(1 is 1)\n");
    let run = sciencec(&["test", &file]);
    run.failed().stderr_contains("SC0400");
}

/// A program whose `assert` holds runs to completion, and `test` reports it
/// with `cargo test`'s own "ok" line — the format `Session::run_test`
/// deliberately borrows.
#[cfg(feature = "llvm")]
#[test]
fn a_passing_assert_is_reported_ok() {
    let file = scratch(
        "test_pass.science",
        b"def main():\n    assert(1 + 1 is 2)\n    print(\"reached the end\")\n",
    );
    let run = sciencec(&["test", &file]);
    run.succeeded();
    assert!(run.stdout.contains("reached the end"), "stdout:\n{}", run.stdout);
    assert!(
        run.stdout.contains("test ") && run.stdout.contains("... ok"),
        "stdout:\n{}",
        run.stdout
    );
}

/// A failing `assert` aborts the program — `TokenKind::Assert`'s decision,
/// the same runtime path `panic` uses — and `test` turns that into `FAILED`
/// and a non-zero exit rather than `sciencec`'s own crash.
///
/// The exact number the process died with is platform-defined
/// (`science-rt`'s `panic.rs`), so this checks the shape of the report and
/// not one platform's signal or exit code.
#[cfg(feature = "llvm")]
#[test]
fn a_failing_assert_is_reported_failed() {
    // `1 + 1 is 3` rather than `1 is 2`: two bare integer literals compared
    // directly is `SC0400` today — "a comparison of two constants, whose
    // width and signedness no operand and no destination names" — a
    // pre-existing gap this test has no business exercising. `1 + 1`
    // materialises a typed temporary, which is what the comparison needs to
    // pick a width from.
    let file = scratch(
        "test_fail.science",
        b"def main():\n    assert(1 + 1 is 3, \"math is broken\")\n",
    );
    let run = sciencec(&["test", &file]);
    run.failed();
    assert!(
        run.stdout.contains("test ") && run.stdout.contains("... FAILED"),
        "stdout:\n{}",
        run.stdout
    );
    assert!(
        run.stderr.contains("math is broken"),
        "the assertion's own message should reach the terminal:\n{}",
        run.stderr
    );
}

/// The default message, when `assert` is given no second argument.
#[cfg(feature = "llvm")]
#[test]
fn a_failing_assert_with_no_message_reports_the_default() {
    // See `a_failing_assert_is_reported_failed` for why not `1 is 2`.
    let file = scratch("test_fail_default.science", b"def main():\n    assert(1 + 1 is 3)\n");
    let run = sciencec(&["test", &file]);
    run.failed();
    assert!(
        run.stderr.contains("assertion failed"),
        "stderr:\n{}",
        run.stderr
    );
}

// --- the bracket revision, end to end -------------------------------------
//
// `eebf6a8` did the design and the implementation; what it left for this pass
// was its own tests, and its own commit message asked, in as many words, for
// these constructs run through the real binary: `Array[T]`, `Map[String,
// Int]`, `Array[Map[String, Int]]`, `Map[String, Array[Float]]`, `def
// largest[T](…)`, `def map[T, U](…)`, `&T`, `&mut T`, `Doc implements
// Summarize:` and `Doc has:`. The negative half — `Array of T`, `Map of
// (String, Int)`, `def largest of T(…)`, `borrowed T`, `mutable borrowed T`,
// each reporting the message that names its replacement — lives in
// `science-parser/tests/migration.rs`, next to the parser that raises them,
// and in `tests/ui/parse/{generic_tool,borrowed_tool_parameter}.science`,
// which `sciencec --test ui` already renders through this same binary.

/// Every construct the request named, in one file, checked clean.
///
/// This is the whole list — including a generic function called at its call
/// site (`largest`, `map`) and a container nested inside another
/// (`Array[Map[String, Int]]`, `Map[String, Array[Float]]`) — because `check`
/// runs the lexer, the parser, name resolution and the type checker over all
/// of it. What it does not run is codegen: `a_runnable_subset_of_the_same_
/// constructs_builds_and_prints_the_right_answer` below is where the parts
/// this backend can lower are built and executed for real, and its own
/// comment says which parts of this list are missing and why.
#[test]
fn every_construct_the_bracket_revision_added_checks_clean() {
    let file = scratch(
        "bracket_revision_checked.science",
        b"\
interface Summarize:
    def summarize(self) -> String

type Doc:
    title: String

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

Doc has:
    def shout(self) -> String:
        self.title

def largest[T: Ord](items: &Array[T]) -> (&T)?:
    let mutable best be items.get(0)
    for item in items:
        if best?:
            if item > best: best be item
        else:
            best be item
    best

def map[T, U](items: &Array[T], f: (&T) -> U) -> Array[U]:
    let mutable out be Array[U].new()
    for item in items:
        out.push(f(item))
    out

def rename(d: &mut Doc):
    d.title be \"renamed\"

def main():
    let xs: Array[Int] be [4, 8, 15, 16, 23, 42]
    let biggest be largest(&xs)
    let doubled be map(&xs, n giving n * 2)

    let mutable scores: Map[String, Int] be Map[String, Int].new()
    scores.insert(\"alice\", 10)

    let mutable grid: Array[Map[String, Int]] be Array[Map[String, Int]].new()
    let mutable copy be Map[String, Int].new()
    copy.insert(\"alice\", 10)
    grid.push(copy)

    let mutable buckets: Map[String, Array[Float]] be Map[String, Array[Float]].new()
    buckets.insert(\"x\", [1.5, 2.5])

    let mutable doc be Doc(title: \"hello\")
    rename(&mut doc)

    print(f\"{biggest} {xs.length()} {doubled.length()} {scores.length()} {grid.length()} {buckets.length()} {doc.shout()} {doc.summarize()}\")
",
    );
    let run = sciencec(&["check", &file]);
    run.succeeded().silent_stderr();
}

/// The part of the list above this backend can build and run today, actually
/// built and run — `Array[Int]`, `Map[String, Int]`, `&Doc`, `&mut Doc`,
/// `Doc implements Summarize:` and `Doc has:` — with the output compared
/// against the values those constructs mean, and the exit code checked.
///
/// **Two items on the full list are not here, and neither is a defect this
/// migration introduced.** `largest` and `map` are ordinary generic
/// *functions*, and calling one is `SC0400`: "cannot build a call to the
/// generic function `largest`, which nothing has monomorphised" —
/// `check.rs`'s own `Decision 42` note says the walk that would do it "puts
/// the walk above this crate and no phase runs it yet". `Array[Map[String,
/// Int]]` and `Map[String, Array[Float]]` are `SC0400` for a second reason:
/// `Decision 20`'s `drop_fn` releases an element through a one-argument
/// runtime call, and a container's own element has no such call to be
/// released with. Both refusals are the backend naming its own edge deliberately
/// rather than lowering a construct no execution test has ever run, exactly
/// as its own message says, and both predate the bracket syntax — the
/// generic-function gap and the nested-container gap are equally unreachable
/// through `Array of T`. Closing either is out of this migration's scope.
#[cfg(feature = "llvm")]
#[test]
fn a_runnable_subset_of_the_same_constructs_builds_and_prints_the_right_answer() {
    let file = scratch(
        "bracket_revision_run.science",
        b"\
interface Summarize:
    def word_count(self) -> Int

type Doc:
    title: String

Doc implements Summarize:
    def word_count(self) -> Int:
        self.title.length()

Doc has:
    def shout(self) -> Int:
        self.title.length()

def read_title(d: &Doc) -> Int:
    d.title.length()

def rename(d: &mut Doc):
    d.title be \"renamed\"

def main():
    let xs: Array[Int] be [4, 8, 15, 16, 23, 42]

    let mutable scores: Map[String, Int] be Map[String, Int].new()
    scores.insert(\"alice\", 10)

    let mutable doc be Doc(title: \"hello\")
    rename(&mut doc)

    print(f\"{xs.length()} {scores.length()} {doc.shout()} {doc.word_count()} {read_title(&doc)} {doc.title}\")
",
    );
    let run = sciencec(&["build", &file]);
    run.succeeded().silent_stderr();
    let executable = Path::new(&file).with_extension(if cfg!(windows) { "exe" } else { "" });
    assert!(executable.is_file(), "no executable at {}", executable.display());
    let program = Command::new(&executable).output().expect("the program runs");
    // `xs` has six elements; `scores` has the one key just inserted;
    // `shout`, `word_count` and `read_title` each measure "renamed", which
    // `rename` wrote over the constructor's "hello" through the `&mut Doc`
    // `rename` took; and `doc.title` reads that same string back out.
    assert_eq!(
        String::from_utf8_lossy(&program.stdout).replace("\r\n", "\n"),
        "6 1 7 7 7 renamed\n"
    );
    assert_eq!(program.status.code(), Some(0));
}
