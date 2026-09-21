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
// It was a **true positive** and the fix was a presence test around the
// comparison, which is the same one-line fix `parent_of` took. `examples/`
// took it, and the entry went with it — so the last of the five's family
// closed the way the first three did, by the corpus being corrected rather
// than by the language gaining a rule.
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
// **Six sites in two files reported, they were one finding, and they are
// closed.** Every one of them was `Box.new(C(..))` in a slot declared `Box of
// any Summarize`:
//
// ```text
// def into_summary(flag: Bool) -> Box of any Summarize:
//     if flag:
//         Box.new(Doc(title: "a", body: "..."))
// ```
//
// `Box.new(value: T) -> Box of T` is the declaration, the argument fixes `T`,
// and the call is `Box of Doc`. `Box of Doc` reaching `Box of any Summarize` is
// an **unsizing under a type constructor**, and `assign`'s §4 listed that first
// among *"three things it deliberately does not reach"*, on §2's grounds.
//
// **So it was not a defect in `examples/`, and not a wrong signature.**
// §5 of the same file says, in as many words, *"an owned `any Summarize` is
// constructed where it is written — `Box.new(doc)` — and the corpus already
// writes every one of them that way"*. The corpus was writing the form the note
// tells it to write, against a refusal the note also wrote. The compiler was the
// first thing that could hold both sentences at once, and they did not agree.
//
// **What the corpus falsified was one clause.** §4 closed its three refusals
// with *"and none of which the corpus writes"*. It writes the first one, six
// times, and only the absent declaration kept that invisible.
//
// **Three things would have closed it, and the first is what did:**
//
// 1. **`Box of C` unsizes to `Box of any I`**, a coercion beside
//    `Coercion::Unsize`. This was the cheapest and the best-argued: it is
//    representationally the *same* operation — a pointer that already exists
//    paired with a vtable known at the site, no allocation and no value moved —
//    and §2's stated reason for refusing conversions under a constructor,
//    *"rewriting every element of a container that already exists, at a cost
//    proportional to its length"*, does not hold of a constructor that holds one
//    element and does not rewrite it. That is the identical exemption §2 already
//    granted `Coercion::CopyWhenPresent` and now states about both.
//
//    **It is `Coercion::UnsizeInBox` and it is `assign`'s §4a.** The two
//    reasons it was not taken here are discharged rather than inherited.
//    `type-checking-and-mir.md` §6.2's count survives by §6.2's own AMENDMENT 4
//    test — what it counts is conversions that change a *value*, and this
//    changes only how a value is pointed at, exactly as `Coercion::Unsize`
//    does. And the variant is lowered rather than declared: `science-mir`'s
//    `tests/unsize.rs` is the six sites arriving as `Rvalue::Coerce` with no
//    call beside them, on this very corpus.
// 2. **`Box.new`'s `T` comes from the expectation** rather than from the
//    argument, so the slot's `any Summarize` instantiates it. That needs
//    Decision 14's boxing widened from `Error` to every interface — §5 admits
//    it *"for `Error` alone"* — and an expectation threaded into a method call,
//    which Decision 1 does not have. Not taken, and §4a is why it does not have
//    to be.
// 3. **`Box.new` is not an ordinary generic function** but the written-out form
//    of Decision 14's boxing, whose result type is the slot's. That is the
//    reading §5's prose supports, and it is a specification nobody has written.
//    Not taken: it would make one prelude function magic, where §4a is a rule
//    about two types that any `Box` obeys.
//
// **What `docs/` must now say, which this change may not write.** There are two
// numbers in `type-checking-and-mir.md` §6.2 and they move differently.
//
// - **The count §6.2 defends does not move, and AMENDMENT 4 is why.** That
//   amendment states the measure in as many words — *"the thing this decision
//   counts is **conversions that change a value**"* — and gives the test:
//   `C` into `Box of any I` allocates and is counted; `borrowed C` into
//   `borrowed any I` *"pairs a pointer that already exists with a vtable known
//   at the site"* and is not. `Box of C` into `Box of any I` is the second
//   sentence, not the first: the allocation is the `Box.new` the author wrote,
//   the pointee never moves, and the value does not change. **Still two.**
// - **The variant count in AMENDMENT 4a does move: seven becomes eight.** That
//   number is there for a stated reason — *"a reader counting `Coercion`
//   variants in the implementation finds seven and concludes the note is
//   stale"* — so it is exactly the kind of number that has to be edited rather
//   than left.
//
// And one sentence of AMENDMENT 4a is now half true. It says the owning forms
// *"stay written out … the two spellings no longer look alike in the source,
// which is the point, because one of them allocates"*. That is still the rule —
// `describe_boxed(doc)` does not compile — but `describe_boxed(Box.new(doc))`
// now does, and the reason it is not a weakening is that the allocation is
// still exactly where the author wrote it. `assign`'s §4 and §4a carry that
// argument; §6.2 should carry the correction.
/// **Both entries closed, and neither closed by the compiler gaining
/// anything — the corpus was wrong and is now corrected.**
///
/// **`19_stdlib.science` used to write `items.get_mut(0)`.** `methods`' §8a
/// judged the name against `builtins.rs`' `UNWRITTEN`, read out of
/// `stdlib-core.md` and `collections-and-chains.md`, and neither note gave
/// `Array` a `get_mut` — but `indexing-and-array-literals.md` §1.4 Decision 5
/// writes the signature out in Science as `get_mutably`, and `builtins.rs`
/// declares exactly that name (§5.4 spells the siblings `values_mutably` and
/// `iterate_mutably` the same way). The file now calls `items.get_mutably(0)`
/// and `check` has nothing left to say about it.
///
/// **`21_compiler_shapes.science` used to write `.len()` three times** —
/// `Array of Def`, `Array of Token` and `Array of Diagnostic`.
/// `collections-and-chains.md` §5's rejected-names table retires it by name —
/// *"`count()` vs `length()` ← `len()`/`count()` — the abbreviation goes"*,
/// under §4.3's rule against abbreviations — and `stdlib-shape-and-packages.md`
/// restates it as *"`length()`, not `len()`"*. `Array.length()` is declared in
/// `builtins.rs`. The file now calls `.length()` at all three sites.
///
/// **Both entries were the measured cost of closing `methods`' §8a**, stated
/// here rather than absorbed: before it, this constant was empty because the
/// checker had nothing to say about a method on a prelude type, not because
/// there was nothing to say. Now it is empty again because the two names were
/// corpus mistakes rather than a compiler or spec gap, and both are fixed.
///
/// # A hole closing uncovers another, at `04_enums.science`
///
/// **`04_enums.science`, `SC0536` ×2** — `Node(Box.new(Leaf(1)), Box.new
/// (Leaf(2)))`. `Leaf` is a variant of `Tree of T`, and `Leaf(1)`'s `1` is
/// unsuffixed. `instantiate_payload` used to leave an unsolved parameter
/// `Ty::ERROR` unconditionally, so `Leaf(1)` synthesised as `Tree of ERROR`
/// — and `receiver_arguments`' own `references_error` check reads that and
/// silences `Box`'s report for exactly the reason its doc comment gives:
/// *"the checker failed to type what they wrote"*, not the author leaving
/// something open.
///
/// `call_variant` now defers a variant construction the same way `record_lit`
/// already does — the fix this file's own history is about — so `Leaf(1)`
/// synthesises as a genuinely open variable instead of `Tree of ERROR`.
/// **Not a corpus mistake**: Decision 2 defaults `1` to `I64` and
/// `Box.new(Leaf(1))` is `Box of (Tree of I64)` once it does, exactly as
/// inferable as `identity(7)` was before `instantiate_call` learned to defer
/// rather than guess. But `receiver_arguments` predates that fix and asks
/// `self.infer.resolve` for `InferTy::Known` only, so an argument that is
/// merely *not yet* answered reads the same as one this call cannot solve at
/// all, and reports where it used to read past the `Ty::ERROR` in silence.
///
/// Owner: `science-types`' `receiver_arguments`, which needs the same
/// three-state upgrade — solved, honestly `Ty::ERROR`, or deferred to the
/// argument's own variable — `instantiate_call` already has. That is
/// `Box[T]`'s inference gap, already being closed elsewhere; this entry is
/// its cost showing up here first, one caller's worth, rather than something
/// this file's own fix should chase into another crate's function.
const REMAINING: &[(&str, &[u16])] = &[("04_enums.science", &[536, 536])];

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
///
/// # **It is 0, and the seven closed two different ways**
///
/// - **The six** closed the way their own third kind of entry required. They
///   were never a defect in a program or a gap the language could not name;
///   they were two sentences in `crates/science-types/src/assign.rs` that did
///   not agree, and closing them was making §2's exemption say about `Box` what
///   it already said about `?`. `Coercion::UnsizeInBox` is that rule and
///   `assign`'s §4a is the argument. **Not one character of `examples/`
///   changed**, which is what a third-kind entry closing correctly looks like:
///   the corpus was right, and it went on being written the way `assign`'s §5
///   told it to be.
/// - **The seventh** closed the second kind's way, by the one edit to
///   `examples/07_generics.science` that the entry above named — a presence
///   test around `item > best`.
///
/// **Zero is read here exactly as this file's own warning above says to read
/// it**: a corpus that reports nothing looks identical to a checker that has
/// stopped running, so the number alone is not the evidence. The evidence is
/// that `the_corpus_reports_exactly_what_is_pinned_and_nothing_else` still
/// walks all twenty-two files, that `crates/sciencec/tests/cli.rs` runs the
/// real binary over the same corpus, and that
/// `crates/science-mir/tests/unsize.rs` counts six `UnsizeInBox` rvalues in
/// that corpus by name. Silence here would have to be silence in three places
/// at once, and the third one counts up rather than down.
///
/// **What this number is not.** It is not a claim that `examples/` is fully
/// checked. `17_modules.science` imports files this repository does not
/// contain, so its resolution errors become `Ty::ERROR` and this test cannot
/// see what it would say about types — the opening note says so, and that is
/// unchanged. Nor is it a claim that the language is finished: the holes
/// `check`'s §6 names are holes in what is *asked*, and a question nobody asks
/// reports nothing.
///
/// # **It is still 0, and Decision 16 is why that is worth a paragraph**
///
/// Exhaustiveness landed — `crates/science-types/src/exhaustive.rs` — and this
/// number did not move. Every `match` in `examples/` covers every value of its
/// scrutinee, and no arm in any of them is dead. **That is a finding about the
/// corpus and not about the check**, and it was not the expected one: the four
/// preceding sections of this comment are each a record of a new question being
/// asked and the corpus failing it, so a fifth question that the corpus passes
/// outright is the first of its kind here.
///
/// It is also the reading that has to be defended, because this file's own
/// warning applies to it exactly — *"a corpus that reports nothing is also what
/// a checker that has stopped running reports"* — and a new check reporting
/// nothing on its first contact with real programs is the single most likely
/// shape of a check that does not run. Three things say it does:
///
/// 1. [`the_exhaustiveness_check_had_a_corpus_to_look_at`] counts the `match`
///    expressions this test walked, and asserts a floor. Silence over zero
///    matches and silence over forty are the same `Vec` and this is what tells
///    them apart.
/// 2. `tests/exhaustiveness.rs` asserts the *witness* and not the refusal, at
///    every constructor set the language has, so a check that answered
///    *"exhaustive"* unconditionally fails thirteen of those tests by name.
/// 3. Removing one arm from one corpus file reports. That is not a test — a
///    test may not edit `examples/` — and it is how the claim was checked:
///    `05_match.science`'s `describe` without its `Eof` arm says
///    `` `borrowed Token` has a value no arm of this `match` covers: `Eof` ``.
///
/// **Why the corpus passed, stated rather than assumed.** Its `match`es fall
/// into three groups and each is exhaustive for its own reason.
/// `00_kitchen_sink`, `04_enums`, `05_match`, `09_absence_and_failure`,
/// `13_inline_blocks` and `16_indentation` enumerate every variant of a choice
/// — several of them say in a comment that this is the point, and
/// `19_stdlib`'s says *"a new variant added to the choice breaks this def
/// loudly"*, which was not true until now and is. The matches over an `Int`, a
/// `Char` and a `String` all carry a `_` or a binding, because §2 of
/// `exhaustive` makes those sets infinite. And `19_stdlib`'s two matches over
/// an error sit inside `if err?:`, so Decision 7 has already narrowed the
/// scrutinee and §3's `null` constructor is not in the set at all — which is
/// the one place this check and the error model meet, and the corpus is where
/// it was confirmed to meet quietly.
/// # It is 4, and the four are the corpus being wrong rather than the checker
///
/// **The number went up, for the first time, and it went up because a check
/// started running rather than because one broke.** `methods`' §8a closed the
/// hole that made a prelude type's method surface open at *every* name:
/// `"hi".no_such_method()` used to check clean, which meant the whole standard
/// library was exempt from the lookup a user type gets. The four that appeared
/// are `items.get_mut(0)` in `19_stdlib.science` and three `xs.len()` in
/// `21_compiler_shapes.science`, and [`REMAINING`] argues each one against the
/// note that governs it: **neither name exists**, and `collections-and-chains.md`
/// §3.1 does not merely omit `len` — it turns it down by name in favour of
/// `length`.
///
/// **So this is the case the opening note said could not arise.** *"Anything
/// reported here is a bug in the compiler or a gap in the spec, never a typo in
/// the corpus"* is the contract `examples/README.md` states, and these four are
/// the third thing: two names the corpus writes that no note gives. The entry
/// stays a pin rather than becoming an exemption for exactly the reason the
/// ratchet exists — a file that stops reporting one of these fails too, so the
/// day the corpus is corrected this constant empties and cannot quietly not.
///
/// **What would make this number a lie** is if the four were the *whole* of what
/// §8a found, because a check that reports four things on twenty-two files is
/// as likely to be half-wired as to be right. It is not: closing the surface
/// **completely** — every name on every builtin head — was measured first, and
/// it reports 37 diagnostics across 13 of the 22 files. Thirty-three of those 37
/// are correct programs calling `truncate`, `iterate`, `pop` and three methods
/// through a `Box`, and `builtins.rs`' `UNWRITTEN` and `WHOLLY_OPEN` are the
/// named, cited, shrinking lists that keep them silent. The four left are the
/// residue that no note excuses.
///
/// # **It is still 4, and `Range` is the second question the corpus passed
/// outright**
///
/// `builtins.rs` declares `Range of T implements Iterate: type Item is T` and
/// `check`'s `range_expr` types every `a..b` in the language, where the
/// expression used to be [`Ty::ERROR`] with no diagnostic. **This number did
/// not move**, and the pin is recorded here unmoved rather than left to be
/// inferred from its own absence.
///
/// Each of the four preceding declarations — `Array.get`, `Iterate` on
/// `Array`, associated calls on a prelude head, `methods`' §8a — added a
/// diagnostic to this list on contact with the corpus, so the expected outcome
/// was a fifth. It did not arrive, and the reason is a fact about the corpus:
/// **every range in `examples/` is over one integer type on both ends**, and
/// every loop body that consumes one is inside a function whose return type
/// pins the accumulator to that same type. `10_loops`' `ruler`, `triangular`,
/// `slice_sum`, `first_multiple` and `checkerboard`, `00_kitchen_sink`'s
/// `ruler`, `13_inline_blocks`' `for _ in 0..stack.length():` and
/// `15_comments`' `for _ in a..b:` are the whole set. Nothing in it mixes `Int`
/// with `I64`, which is the one disagreement the new type is able to see —
/// `builtins.rs`' `Array of T implements Index of Int` block already names
/// that wart and says it is `stdlib-core.md` §3.6's against Decision 2's, not
/// this layer's.
///
/// **Exactly this file's own warning applies**, for the reason the
/// exhaustiveness section states: a new type reporting nothing on its first
/// contact with real programs is indistinguishable from a type nothing reads.
/// [`the_range_check_had_a_corpus_to_look_at`] is what tells them apart — it
/// counts the range expressions this test walked, asserts a floor, and asserts
/// that **not one of them is an error type**, which is the property that was
/// false for every range in the corpus until this change.
///
/// # **It is 0 again, and this time the corpus moved, not the checker**
///
/// The four were re-verified against the notes that were supposed to govern
/// them, rather than taken on trust, and both held up as real: `get_mut` is in
/// neither `stdlib-core.md` nor `collections-and-chains.md`, and `len` is the
/// name `collections-and-chains.md` §5 names and rejects. But
/// `indexing-and-array-literals.md` §1.4 Decision 5 — a note `REMAINING`'s
/// entry for `19_stdlib.science` did not cite — spells the mutable accessor
/// `get_mutably` and `builtins.rs` already declares it, so `Array` was never
/// missing a mutable `get`; the corpus was calling it by the wrong name.
/// `19_stdlib.science` now calls `get_mutably`, and `21_compiler_shapes.science`
/// now calls `length()` at its three sites. Neither the compiler nor the spec
/// changed; `examples/` did.
///
/// # **It is 2, and this time neither the compiler nor the corpus was wrong**
///
/// [`REMAINING`]'s new entry, above `REMAINING` itself, is the full account:
/// `call_variant` learned to defer a variant's generic argument to the
/// literal's own still-open variable instead of forcing `Ty::ERROR`, the same
/// upgrade `record_lit` needed for `Pair(first: 3, second: 4)` and
/// `instantiate_call` already had for `identity(7)`. That closed a real hole —
/// `Exact(7)` in `04_enums.science` now types as `Bound of I64` rather than
/// `Bound of ERROR` — and it is also what let `Box.new(Leaf(1))` reach
/// `receiver_arguments` carrying a variable instead of an error, which is the
/// one input that function has never seen: it silences its own report for an
/// error-tainted argument and never learned to *defer* one that is merely
/// open. So the count is 2 and not 0, and it is 2 for the reason `04_enums`'
/// entry states — `Box[T]`'s own inference, not this corpus file.
#[test]
fn the_corpus_reports_only_what_no_program_can_say() {
    let total: usize = REMAINING.iter().map(|(_, codes)| codes.len()).sum();
    assert_eq!(total, 2);
}

/// The evidence that a silent `Range` is a checked silence.
///
/// Built on [`the_exhaustiveness_check_had_a_corpus_to_look_at`]'s model and
/// with one assertion that test has no analogue for: the **types**. A floor on
/// the count says the walk reached ranges; `Ty::ERROR` on none of them says
/// each one got a type, which is the whole of what this change did and the
/// exact thing that was false before it. A range still typed `Ty::ERROR`
/// reports nothing anywhere — that was the old behaviour and it is invisible to
/// every other test in this file.
#[test]
fn the_range_check_had_a_corpus_to_look_at() {
    use science_types::thir::ExprKind;

    let mut ranges = 0;
    let mut files = 0;
    for path in corpus() {
        let source = std::fs::read_to_string(&path).expect("readable corpus file");
        let checked = support::check_allowing_resolution_errors(&source);
        let mut found = 0;
        for body in &checked.bodies {
            for (_, expr) in body.exprs() {
                if !matches!(expr.kind, ExprKind::Range { .. }) {
                    continue;
                }
                found += 1;
                assert!(
                    !checked.types.references_error(expr.ty),
                    "a range in {} has no type: {}",
                    path.display(),
                    checked.render(expr.ty)
                );
            }
        }
        if found > 0 {
            files += 1;
        }
        ranges += found;
    }
    assert!(ranges >= 8, "the corpus writes more than 8 ranges, found {ranges}");
    assert!(files >= 4, "and they are spread over the corpus, found {files} files");
}

/// The evidence that the silence above is a checked silence.
///
/// A count with a floor rather than an exact number, because the exact one —
/// 35 `match`es over 8 files as this was written — moves whenever somebody adds
/// an example, and this test is not the thing that should notice that. The
/// floor is well under what the corpus has, and it is far enough above zero
/// that a pass which stopped walking bodies could not meet it.
#[test]
fn the_exhaustiveness_check_had_a_corpus_to_look_at() {
    use science_types::thir::ExprKind;

    let mut matches = 0;
    let mut files = 0;
    for path in corpus() {
        let source = std::fs::read_to_string(&path).expect("readable corpus file");
        let checked = support::check_allowing_resolution_errors(&source);
        let found: usize = checked
            .bodies
            .iter()
            .map(|body| {
                body.exprs().filter(|(_, e)| matches!(e.kind, ExprKind::Match { .. })).count()
            })
            .sum();
        if found > 0 {
            files += 1;
        }
        matches += found;
    }
    assert!(matches >= 25, "the corpus should hold far more than 25 `match`es, found {matches}");
    assert!(files >= 6, "and they should be spread over the corpus, found {files} files");
}
