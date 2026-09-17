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
/// # The five entries below are one finding, and it is about the corpus
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
/// So these five are **true positives**: the declaration is right and the five
/// bodies are wrong. They are pinned rather than fixed because `examples/` is
/// not this change's to edit — the fix is one `if x?:` or one `.clone()` per
/// site. Owner: whoever owns the corpus.
// Five appeared the day `Array.get` gained a declaration, and all five were one
// finding in two shapes: a signature promising a *total* result from a partial
// accessor, and a signature promising a *value* where a borrow comes back. They
// type-checked only while `get` returned an error type that agreed with
// anything.
//
// **Three were the corpus's fault and are fixed.** `largest` cannot be total —
// an empty array has no largest element — and `DefTable.get` and `Parser.peek`
// cannot promise a `borrowed X` from an accessor that may find nothing. All
// three now say `(borrowed T)?`, and `parent_of` gained the presence test that
// makes its field read legal, which turned the acceptance example into a
// demonstration of §4.2's narrowing rather than a program that only passed
// because nothing was checking.
//
// **Two are not.** They need a value read *out* of a borrow, and there is no
// spelling for that: the prelude declares `Copy` and `Clone` as interfaces with
// no methods on either, and the language has no dereference operator. Nothing
// the corpus could write would fix these, so they are pinned rather than
// rewritten.
const REMAINING: &[(&str, &[u16])] = &[
    // `Letters.next` returns `Self.Item?` — `Char?` — from
    // `self.glyphs.get(self.cursor)`, which is `(borrowed Char)?`.
    ("06_traits.science", &[525]),
    // `value_at` returns `cell`, narrowed to `borrowed Int`, as `Int`.
    ("10_loops.science", &[525]),
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
/// without an argument, and what closes these two is a decision — whether
/// `Copy`, which the prelude already declares for `Bool`, `Char` and every
/// numeric type, means a borrow of one may be read as a value.
#[test]
fn the_corpus_reports_only_what_no_program_can_say() {
    let total: usize = REMAINING.iter().map(|(_, codes)| codes.len()).sum();
    assert_eq!(total, 2);
}
