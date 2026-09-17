//! The acceptance check for this layer: type-check every program in
//! `examples/` and pin, by name, everything the checker still says about them.
//!
//! `examples/README.md` states the contract those files live under — they are
//! meant to be *correct programs*, so anything reported here is a bug in the
//! compiler or a gap in the spec, never a typo in the corpus. The lexer and
//! the parser each hold themselves to that over the whole corpus
//! (`science-lexer/tests/corpus.rs`, `science-parser/tests/corpus.rs`); this is
//! the same check one layer up, and it is the only test in this crate whose
//! input is a real program rather than a fixture written to exercise a rule.
//!
//! # This is a ratchet, not an exemption list
//!
//! The parser's corpus test says that *"a corpus with a list of files that do
//! not have to work stops being an acceptance check"*, and it is right.
//! [`REMAINING`] is not that list. It is an exact pin: a file that reports
//! something not in it fails, **and a file that stops reporting something in it
//! also fails**. Closing one of these holes is therefore a two-line change —
//! the fix, and the entry deleted — and the number can only go down by someone
//! noticing.
//!
//! Each entry names the crate that owns the hole and what it is waiting for,
//! because a count with no owner is a number and not a handoff.
//!
//! # What this test does not see
//!
//! `17_modules.science` does not resolve as a single file — it imports modules
//! that are not files in this repository, which `sciencec`'s driver already
//! records — so its seven resolution errors become `Ty::ERROR`s, and `ty`'s
//! §5 makes an error type agree with everything it meets. Whatever that file
//! would say about types is therefore not said, and this test cannot be the
//! thing that notices. The rest of the corpus resolves clean and is checked for
//! real.

mod support;

use std::path::{Path, PathBuf};

/// Everything the checker still reports over `examples/`, by file and code.
///
/// **`20_extern.science`, `SC0525`** — `expected MutableSpan of F64, found
/// Span of F64`, at `gemm`'s `c: c.data`. The file declares
/// `MatrixView.data: ffi.Span of F64` and passes it to a `cblas_dgemm` whose
/// `c` parameter is `ffi.MutableSpan of F64`; the same lines are in
/// `docs/superpowers/design/ffi-c-boundary.md` §1.7, so the corpus is copying
/// the note and the note is where the gap is. **No coercion here may close
/// it**: `Span` into `MutableSpan` is the unsound direction, and admitting it
/// would hand C a writable pointer to a shared borrow. The sound reading is
/// that the field is a `MutableSpan` and the two *reading* matrices weaken to
/// `Span`, which is `assign`'s §5 last bullet — `region-inference.md`'s
/// question, not this crate's. Owner: the FFI note, then whoever owns
/// mutability weakening.
///
/// **`21_compiler_shapes.science` is no longer here**, and the shape of what
/// closed it is worth keeping: its `SC0526` was `the type of this value cannot
/// be inferred` at `defs.alloc(DefKind.Module, "main", null)`, where `alloc`
/// declares `parent: DefId?` twenty lines above. The `null` had nothing to take
/// its type from because the call resolved to nothing — Decision 11's lookup
/// did not exist, so every method call carried `method: None` and synthesised
/// its arguments against nothing at all. `science-types`'s `methods` module is
/// that lookup; the entry went with it, which is what this file's opening
/// paragraph says a fix looks like.
///
/// # The entry below is one finding, and it is about the corpus
///
/// `builtins.rs` now declares `Array.get` as `stdlib-core.md` §3.6 writes it:
///
/// ```text
/// ## Parentheses are required: `borrowed T?` would read as `borrowed (T?)`.
/// def get(self, index: Int) -> (borrowed T)?
/// ```
///
/// Five corpus sites disagree with it, in exactly two ways, and **both ways are
/// the corpus reading a hole rather than a note**:
///
/// 1. **`get` is read as total.** `07_generics`' `largest` binds
///    `items.get(0)` and returns it as `borrowed T`; `21_compiler_shapes`'
///    `DefTable.get` and `Parser.peek` do the same. A container accessor that
///    cannot fail is not a design anybody argued for — it is what you get when
///    the call resolves to nothing and `Ty::ERROR` agrees with the return type.
///    `examples/07_generics.science` line 36 even calls it *"the one `Array`
///    accessor §8 gives"*, which is the corpus quoting a section that stopped
///    enumerating.
/// 2. **`get` is read as returning by value.** `06_traits`' `Glyphs.next`
///    returns `Char?` and `10_loops`' `at` returns `Int`, both from a `get` on
///    an `Array` of a `Copy` element. §3.6's answer is borrowed for every `T`;
///    a by-value `get` on a `Map of (String, Array of F64)` would copy the
///    whole array on every lookup, and there is no rule in the language that
///    makes the answer depend on `Copy`.
///
/// So those five were **true positives**: the declaration is right and the five
/// bodies were wrong. Three were corrected in `examples/`; the other two were
/// the language's gap rather than the corpus's, and `assign`'s §7 closed them.
/// The comment below the doc block says what replaced them and why it is the
/// same finding one more time.
// Five appeared the day `Array.get` gained a declaration, and all five were one
// finding in two shapes: a signature promising a *total* result from a partial
// accessor, and a signature promising a *value* where a borrow comes back. They
// type-checked only while `get` returned an error type that agreed with
// anything.
//
// **Three were the corpus's fault and were fixed.** `largest` cannot be total —
// an empty array has no largest element — and `DefTable.get` and `Parser.peek`
// cannot promise a `borrowed X` from an accessor that may find nothing. All
// three now say `(borrowed T)?`, and `parent_of` gained the presence test that
// makes its field read legal.
//
// **The other two needed a value read *out* of a borrow, and both are closed.**
// What closed them is `assign`'s §7: **a borrowed `Copy` type is assignable to
// the value**, which the prelude already declares for `Bool`, `Char` and every
// numeric type. `06`'s `Letters.next` returns `(borrowed Char)?` where `Char?`
// is written, which is `Coercion::CopyWhenPresent`; `10`'s `value_at` returns a
// `borrowed Int` narrowed out of a `(borrowed Int)?` where `Int` is written,
// which is `Coercion::Copy`. Neither is an edit to `examples/` and neither
// could have been: the decision was the missing thing, not the program.
//
// **One entry arrived in their place, and it is the same class as the three
// that were fixed.** `Array of T implements Iterate: type Item is borrowed T`
// — `collections-and-chains.md` §4 — means a `for` binding now has a type,
// where before it was `Ty::ERROR` and agreed with whatever it met. The line
// that shows is `07_generics`' `if item > best`, and the file itself explains
// why it is wrong four lines above: `best` is bound from `items.get(0)` and is
// therefore `(borrowed T)?`, *"so an empty array has no first element and
// `largest` has no answer to give"*. The comparison against `item`, a
// `borrowed T`, was never checked because `item` had no type; Decision 6 makes
// `T?` never coerce to `T`, so comparing the two is comparing a value with a
// value that may be absent.
//
// It is a **true positive** and the fix is one presence test — `if best? and
// item > best` — which is the same one-line fix `parent_of` took. It is pinned
// rather than made, because `examples/` is not this change's to edit. Owner:
// whoever owns the corpus, and it is the last of the five's family.
// --- the fourth arrival of the same shape, and the first one no edit closes
//
// `BodyChecker::type_receiver` accepted `Record | Choice | Alias | Interface |
// Union`, and `builtins.rs` allocates `Array`, `Map`, `Box`, `String` and
// `Chars` as `DefKind::Primitive`. So **a call reached through a prelude type
// resolved to nothing**: `String.new()`, `(Array of T).new()`, `Map.new()` and
// `Box.new(x)` were `Ty::ERROR`, in fifteen of the twenty-two files here, and
// `ty`'s §5 made every one of them agree with whatever slot it was written
// into. One word in that `matches!` closed it, and what it exposed is below.
//
// **Two files stopped reporting and are not on this list.**
// `18_ownership.science`'s `boxed() -> Box of Doc` and `19_stdlib.science`'s
// `boxed(record) -> Box of Record` now type-check for real, against a real
// signature, and pass.
//
// **Six sites in two files report, and they are one finding.** Every one of
// them is `Box.new(C(..))` in a slot declared `Box of any Summarize`:
//
// ```text
// def into_summary(flag: Bool) -> Box of any Summarize:
//     if flag:
//         Box.new(Doc(title: "a", body: "..."))
// ```
//
// `Box.new(value: T) -> Box of T` is the declaration, the argument fixes `T`,
// and the call is `Box of Doc`. `Box of Doc` reaching `Box of any Summarize` is
// an **unsizing under a type constructor**, and `assign`'s §4 lists that first
// among *"three things it deliberately does not reach"*, on §2's grounds.
//
// **So this is not a defect in `examples/`, and it is not a wrong signature.**
// §5 of the same file says, in as many words, *"an owned `any Summarize` is
// constructed where it is written — `Box.new(doc)` — and the corpus already
// writes every one of them that way"*. The corpus is writing the form the note
// tells it to write, against a refusal the note also wrote. The compiler is the
// first thing that could hold both sentences at once, and they do not agree.
//
// **What the corpus does falsify is one clause.** §4 closes its three refusals
// with *"and none of which the corpus writes"*. It writes the first one, six
// times, and only the absent declaration kept that invisible.
//
// **Three things would close it, and each is a note's rather than a checker's:**
//
// 1. **`Box of C` unsizes to `Box of any I`**, a coercion beside
//    `Coercion::Unsize`. This is the cheapest and the best-argued: it is
//    representationally the *same* operation — a pointer that already exists
//    paired with a vtable known at the site, no allocation and no value moved —
//    and §2's stated reason for refusing conversions under a constructor,
//    *"rewriting every element of a container that already exists, at a cost
//    proportional to its length"*, does not hold of a constructor that holds one
//    element and does not rewrite it. That is the identical exemption §2 already
//    grants `Coercion::CopyWhenPresent`. It is **not** taken here for two
//    reasons: `type-checking-and-mir.md` §6.2 owns the count of implicit
//    coercions and this would change it, and a `Coercion` variant nothing
//    lowers is worse than the refusal — `science-mir` and `science-codegen`
//    would each have to emit the vtable pair, and neither is this change's.
// 2. **`Box.new`'s `T` comes from the expectation** rather than from the
//    argument, so the slot's `any Summarize` instantiates it. That needs
//    Decision 14's boxing widened from `Error` to every interface — §5 admits
//    it *"for `Error` alone"* — and an expectation threaded into a method call,
//    which Decision 1 does not have.
// 3. **`Box.new` is not an ordinary generic function** but the written-out form
//    of Decision 14's boxing, whose result type is the slot's. That is the
//    reading §5's prose supports, and it is a specification nobody has written.
//
// Owner: `type-checking-and-mir.md` §6.2, then whoever lowers the coercion it
// chooses.
const REMAINING: &[(&str, &[u16])] = &[
    // Three `Box.new(Doc(..))` in `as_summary`, each in a `-> Box of any
    // Summarize` arm.
    ("00_kitchen_sink.science", &[525, 525, 525]),
    // `largest` compares `item`, a `borrowed T` bound by a `for`, against
    // `best`, a `(borrowed T)?` bound from `items.get(0)`.
    ("07_generics.science", &[525]),
    // `into_summary`'s two arms, and the `describe_boxed(Box.new(..))` in
    // `main`.
    ("08_dyn_dispatch.science", &[525, 525, 525]),
];

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("examples")
}

/// Every `.science` file in `examples/`, in a stable order.
fn corpus() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples/ must exist")
        .map(|entry| entry.expect("readable directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "science"))
        .collect();
    files.sort();
    files
}

fn expected(name: &str) -> &'static [u16] {
    REMAINING.iter().find(|(file, _)| *file == name).map(|(_, codes)| *codes).unwrap_or(&[])
}

/// One line per diagnostic, with the source line it points at, so that a
/// failure is readable without opening the file.
fn render(checked: &support::Checked, source: &str) -> Vec<String> {
    checked
        .diagnostics
        .iter()
        .map(|diagnostic| {
            let line = match diagnostic.primary_span() {
                Some(span) => {
                    let before = &source[..span.start as usize];
                    let number = before.matches('\n').count() + 1;
                    let start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
                    let end =
                        source[start..].find('\n').map(|i| start + i).unwrap_or(source.len());
                    format!("line {number}: {}", source[start..end].trim())
                }
                None => "no span".to_string(),
            };
            format!("    {} {}\n        > {line}", diagnostic.code, diagnostic.message)
        })
        .collect()
}

#[test]
fn the_corpus_reports_exactly_what_is_pinned_and_nothing_else() {
    let files = corpus();
    assert!(files.len() >= 19, "expected the whole corpus, found {} files", files.len());

    let mut failures = String::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let source = std::fs::read_to_string(path).expect("readable corpus file");
        let checked = support::check_allowing_resolution_errors(&source);
        let codes = checked.codes();
        let want = expected(&name);
        if codes != want {
            failures.push_str(&format!(
                "{name}: pinned {want:?}, got {codes:?}\n{}\n",
                render(&checked, &source).join("\n")
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the corpus no longer matches what this file pins.\n\
         A new code means a bug in the change; a missing one means a hole was \
         closed and its entry in `REMAINING` should go with it.\n\n{failures}"
    );
}

/// Every file pinned in [`REMAINING`] is a file that exists.
///
/// A renamed example would otherwise turn its entry into a permanent silent
/// exemption for a file nobody checks.
#[test]
fn every_pinned_file_is_in_the_corpus() {
    let names: Vec<String> = corpus()
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    for (file, _) in REMAINING {
        assert!(names.iter().any(|name| name == file), "`{file}` is not in examples/");
    }
}

/// The count, stated once, so that a change to it is a line in a diff.
///
/// It was 147 before the checker learned §6.3's auto-borrow, 17 before the
/// parser stopped reading a `(` after an indented block as a call, 11 before
/// `assign`'s §4 admitted unsizing behind a borrow, 3 before checking mode
/// learned that an `unsafe` block is a block, 2 before Decision 11's method
/// lookup landed, and 1 until `20_extern.science` stopped asking one record to
/// be both a shared and an exclusive borrow of its buffer.
///
/// **That last one was never this crate's to fix and it was not a weakening.**
/// It was recorded here, and once in `ffi-c-boundary.md`, as a question for
/// `region-inference.md` — on the reading that a `MutableSpan` should weaken to
/// a `Span`. That reading was wrong twice over. Nothing about it concerns how
/// long anything lives; it reached the region note because the symptom appeared
/// at a borrow. And the weakening would not have helped: declaring the field
/// exclusive makes a view over a shared borrow unconstructible, which is what
/// the reading traded away without noticing.
///
/// The fix is one record generic over its storage, which is `ndarray`'s
/// `ArrayBase` with `ArrayView` and `ArrayViewMut` over it — the answer the
/// scientific-computing ecosystem reached for the identical problem.
///
/// **Zero is a worse guard than any other number**, because a corpus that
/// reports nothing is also what a checker that has stopped running reports.
/// `the_corpus_reports_exactly_what_is_pinned_and_nothing_else` walks every
/// file either way, and `crates/sciencec/tests/cli.rs` runs the real binary
/// over the same corpus, so silence here has to be silence in two places at
/// once.
///
/// **It went 0 to 5 when the prelude got a declaration, then 5 to 2 when the
/// corpus was corrected — and the first of those is the one direction this file
/// says it should not move.** The exception is stated rather than assumed:
/// every earlier entry was the checker reporting on a program the spec says is
/// correct, and those five were the reverse, the checker reporting on a program
/// the spec says is wrong, which it could not do while `Array.get` resolved to
/// nothing.
///
/// Three of the five were `examples/` promising more than a partial accessor
/// can give, and they are fixed. The two that remain need a value read out of a
/// borrow, which has no spelling — so they are the *checker* being right about
/// a gap in the language rather than about a mistake in a program, and no edit
/// to `examples/` closes them.
///
/// The ratchet holds in the sense that matters: nothing may be added here
/// without an argument. The two that stood here are gone, and the decision that
/// closed them is the one this comment named as what they were waiting for —
/// **`Copy`, which the prelude already declares for `Bool`, `Char` and every
/// numeric type, means a borrow of one may be read as a value.** That is
/// `assign`'s §7.
///
/// **It is 1, and the one is not the same kind of entry as the two it
/// replaced.** Those were the checker being right about a gap in the language,
/// which no edit to `examples/` could close. This one is the checker being
/// right about a program, which one edit to `examples/` closes — the third
/// instance of the finding the doc block above records, arriving for the third
/// time from the same cause: a declaration landed, so a line that had been
/// compared against `Ty::ERROR` was compared against a type for the first time.
///
/// `Array of T implements Iterate` is that declaration. Before it, every `for`
/// over an `Array` bound its element at [`Ty::ERROR`] and every use of that
/// element in every loop body in the corpus was unchecked — which is a larger
/// unchecked surface than one diagnostic, and the trade is stated here rather
/// than left to be noticed.
///
/// # **It is 7, and the six that arrived are a third kind of entry**
///
/// The two kinds this file had were *the checker is right about a gap in the
/// language* (no edit to `examples/` closes it) and *the checker is right about
/// a program* (one edit does). The six `Box.new` sites are neither. The
/// **program** is the one `assign`'s §5 tells an author to write, in those
/// words, and the **refusal** is the one `assign`'s §4 wrote down, in those
/// words, and the two are in the same file. Nothing here is wrong; two
/// sentences are, and a compiler is the first thing able to hold both at once.
///
/// **The trade is the same one `Array.get` and `Iterate` made, and it is much
/// the larger.** A prelude type could not be the receiver of an associated
/// call at all, so `String.new()`, `(Array of T).new()`, `Map.new()` and
/// `Box.new(x)` were `Ty::ERROR` in fifteen of the twenty-two files here.
/// Against six diagnostics this buys every one of those calls a type, two
/// files off this list entirely, and the argument of every `Box.new` in the
/// corpus checked against a parameter for the first time. Stating the size of
/// what was unchecked is the point: the number going up is the smaller half of
/// the measurement.
#[test]
fn the_corpus_reports_only_what_no_program_can_say() {
    let total: usize = REMAINING.iter().map(|(_, codes)| codes.len()).sum();
    assert_eq!(total, 7);
}
