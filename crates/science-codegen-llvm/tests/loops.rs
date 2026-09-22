//! `for`, **built, linked, run**, with stdout and exit status asserted.
//!
//! # What a `for` loop was waiting on
//!
//! Four things, and three of them were answered by one discriminator.
//!
//! `thir::ExprKind::For` gained `next: Option<DefId>` a while back and
//! `lower_for` deliberately did not use it, because building the
//! `Callee::Def` unblocked **no** program and broke a correct one:
//!
//! 1. No `next` had a body — every `implements Iterate` block in the prelude
//!    declares `methods: &[]`, so all of them resolved to the *interface's*
//!    bodiless `next`, which needs one copy per implementor.
//! 2. `Array of T implements Iterate` has nowhere to keep a cursor, so its
//!    `next` could not advance whatever called it.
//! 3. `science-regions` suppresses `SC0340` for a body that calls something
//!    unresolved, so resolving those turned `examples/19_stdlib`'s `find` into
//!    a false positive.
//!
//! All three are true of exactly the blocks that declare no `next`, and of
//! none that declare one. So `lower_for` resolves the callee **only when the
//! method's owner is a type's block rather than an `interface`'s** — which
//! today is `Chars`, and tomorrow is whatever else grows a real `next`.
//!
//! The fourth was §5.3's bool-plus-out-parameter convention, and it was already
//! built: `science_chars_next(iter, out) -> Bool` is the same shape as
//! `Map.insert`, and its own documentation says so. `Chars.next` is one row in
//! the table that convention keys on.
//!
//! # §4.4's shared borrow, and why this loop's is exclusive
//!
//! `Iterate.next` is `def next(mutable self)` and §4.4 says a `for`'s *source*
//! is borrowed shared. Both hold at once, and
//! `collections-and-chains.md` §4.2's AMENDMENT 11 is the reading that makes
//! them agree: *"`for x in xs:` desugars to `xs.iterate()`"*. The **source** is
//! `text`, which `chars()` borrows shared; what the loop advances is the
//! `Chars` temporary the chain produced, which the loop itself owns and nobody
//! else can see. So the exclusive borrow is taken only where the subject has no
//! place of its own, and a named collection keeps §4.4's shared borrow.
//!
//! # The soundness this file does **not** test, and where it is tested
//!
//! `for x in xs: xs.push(1)` must be refused — a push that reallocates the
//! buffer leaves the element reference dangling — and it nearly was not. The
//! first version of the indexed lowering took a shared borrow for the
//! `length()` call and let it die there; region inference computes liveness,
//! not scope, so the loan was dead by the time the body ran and the push
//! conflicted with nothing. `science-regions`' own
//! `a_for_over_a_collection_the_body_mutates_is_refused` caught it. It is
//! tested there and not here because this harness does not run region
//! inference: it goes lex, parse, resolve, check, MIR, backend, so the
//! refusal it would see is `SC0400` and not rule 4's `SC0330`.
//!
//! # Why these programs
//!
//! A loop that runs the wrong number of times and a loop that yields the wrong
//! values fail differently, so both are asserted: one counts, one prints what
//! it was handed. A `Chars` is `{ ptr, len, offset }` **into a string somebody
//! else owns**, so an iterator that copied instead of borrowing, or that failed
//! to advance its offset, produces a program that runs forever rather than a
//! wrong number — which is why the count is against a known string rather than
//! against itself.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("loops", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The loop yields the characters, in order, and they are the right ones.
#[test]
fn a_for_over_chars_yields_each_character() {
    assert_eq!(
        prints("chars", "for c in \"abc\".chars():\n    print(f\"[{c}]\")\n"),
        "[a]\n[b]\n[c]\n"
    );
}

/// It runs exactly as many times as the string is long, over a bound name and
/// over a literal.
///
/// Two subjects because they lower differently: `text.chars()` on a binding
/// takes a shared borrow of `text` first, and `"abc".chars()` builds the string
/// as a temporary and borrows that. The loop's own borrow is exclusive in both
/// cases, because in both the thing it advances is the `Chars` the chain made.
#[test]
fn a_for_runs_once_per_character() {
    assert_eq!(
        prints(
            "count",
            "let text be \"Science\"\n\
             let mutable n: Int be 0\n\
             for c in text.chars():\n\
             \x20   n be n + 1\n\
             let mutable m: Int be 0\n\
             for c in \"abc\".chars():\n\
             \x20   m be m + 1\n\
             print(f\"{n} {m}\")\n",
        ),
        "7 3\n"
    );
}

/// A `for` over an `Array`, which is the loop most programs are.
///
/// # What this replaced
///
/// This test used to assert the refusal, and its reason was right at the time:
/// *"`Array[T] implements Iterate` declares `next(mutable self)` with nowhere
/// to keep a cursor, so no implementation of it could advance"*. That is still
/// true of the declaration, and it is why the loop is **not** lowered as a
/// call to `next`. `collections-and-chains.md` §4.2's AMENDMENT 11 says
/// `for x in xs:` desugars to `xs.iterate()`; §8 names no type for `iterate()`
/// to return; so `science-mir` emits the indexed loop that desugaring
/// compiles to, and the spec amendment that would name the type is recorded at
/// `lower_for_over_array` as the repair.
///
/// # Why these three programs
///
/// **An accumulator, because the binding is a borrow.** §4.3 makes a loop's
/// `Item` a `borrowed T` *"uniformly"*, so `total + x` meets a reference and
/// only works because `assign`'s §7 reads a borrow of a `Copy` type as the
/// value. The two decisions are load-bearing for each other, and this is the
/// program that fails if either goes.
///
/// **An empty array and a single element**, because an indexed loop's
/// off-by-one lives at both ends and a three-element fixture hides both.
///
/// **A nested loop over an array of arrays**, because that is where a
/// mistaken cursor or a shared index temporary would show: the inner loop
/// must walk the row the outer one bound, not the outer collection.
#[test]
fn a_for_over_an_array_walks_it() {
    assert_eq!(
        prints(
            "array",
            "let xs be [4, 8, 15, 16, 23, 42]\n\
             let mutable total be 0\n\
             for x in xs:\n\
             \x20   total be total + x\n\
             print(f\"{total}\")\n",
        ),
        "108\n"
    );
    assert_eq!(
        prints(
            "array-edges",
            "let empty be Array[Int].new()\n\
             let mutable n be 0\n\
             for x in empty:\n\
             \x20   n be n + 1\n\
             let one be [7]\n\
             let mutable s be 0\n\
             for x in one:\n\
             \x20   s be s + x\n\
             print(f\"{n} {s}\")\n",
        ),
        "0 7\n"
    );
    assert_eq!(
        prints(
            "array-nested",
            "let mutable c be 0\n\
             for a in [1, 2, 3]:\n\
             \x20   for b in [10, 20]:\n\
             \x20       c be c + a * b\n\
             print(f\"{c}\")\n",
        ),
        "180\n"
    );
}


/// `for i in a..b:` — the counting loop, which is the most-written loop there
/// is.
///
/// # Why no `Range` is built
///
/// `collections-and-chains.md`'s AMENDMENT 14 says `Range` implements
/// `Iterate` *"directly, which is what makes `for i in 0..n:` the same
/// construct as everything else rather than a special case in the parser"* —
/// and the ground it gives is that a range **is not a container**: it holds
/// two ends and *computes* each element. So there is nothing to iterate over,
/// only arithmetic, and `science-mir` emits the arithmetic. No `Range[T]`
/// value exists at run time, which is why this stopped being *"a value of
/// §2.6's runtime containers"* without anything lowering one.
///
/// §4.1 is the difference from the array loop: a `Range`'s `Item` is `T` and
/// not `&T`, because there is no element in memory for a borrow to point at.
/// The pattern binds a value and the loop borrows nothing.
///
/// # Why these four
///
/// **Exclusive and inclusive**, because `..` and `..=` differ by exactly one
/// element and a lowering that confused them still produces a plausible sum.
///
/// **Two empty ranges**, `5..5` and `5..0`, because the test is at the head
/// and a loop that ran once before checking would pass every other assertion
/// here.
///
/// **`0..xs.length()` with an index**, because that is the shape the language
/// is for and it is the one that needs both ends evaluated once: an end
/// re-read every turn is a different loop from the one the author wrote.
#[test]
fn a_counting_loop_over_a_range_adds_up() {
    assert_eq!(
        prints(
            "range",
            "let mutable total be 0\n\
             for i in 0..5:\n\
             \x20   total be total + i\n\
             let mutable t2 be 0\n\
             for i in 0..=5:\n\
             \x20   t2 be t2 + i\n\
             let mutable n be 0\n\
             for i in 5..5:\n\
             \x20   n be n + 1\n\
             for i in 5..0:\n\
             \x20   n be n + 1\n\
             let xs be [10, 20, 30]\n\
             let mutable s be 0\n\
             for i in 0..xs.length():\n\
             \x20   s be s + xs[i]\n\
             print(f\"{total} {t2} {n} {s}\")\n",
        ),
        "10 15 0 60\n"
    );
}

/// **`continue` advances the loop**, which it did not, in either `for`.
///
/// # The bug, and why nothing caught it
///
/// `LoopScope` had one block for both *"where the test is"* and *"where
/// `continue` goes"*, which is true of a `loop:` — nothing sits between the
/// end of its body and its test — and false of a `for`. A `for` has an
/// increment: `lower_for_over_range`'s cursor, `lower_for_over_array`'s
/// index. That increment was emitted at the **end of the body**, so a
/// `continue` jumped over it and the loop ran forever.
///
/// Both `for` forms hung. `loop:` with a `continue` was fine, and `for` with
/// a `break` was fine, which is why it took a program that used the one
/// combination to find it — and why the whole test suite passed with an
/// infinite loop in the language's most-written construct.
///
/// The repair is that the increment gets a block of its own, which both the
/// body's fall-through and every `continue` reach. There was only ever one
/// way out of the body and the increment was on it; now there are two and it
/// is on both.
///
/// # Why this is an execution test and could not be anything else
///
/// A hang is not a wrong value or a refusal. It has no stdout to compare and
/// no diagnostic to match — every structural assertion about the MIR passed
/// while the program never terminated. Running it is the only thing that
/// distinguishes a loop that advances from one that does not.
#[test]
fn continue_advances_both_kinds_of_for_loop() {
    assert_eq!(
        prints(
            "continue",
            "let mutable over_range be 0\n\
             for i in 0..5:\n\
             \x20   if i is 2:\n\
             \x20       continue\n\
             \x20   over_range be over_range + 1\n\
             let mutable xs be Array[Int].new()\n\
             xs.push(1)\n\
             xs.push(2)\n\
             xs.push(3)\n\
             let mutable over_array be 0\n\
             for v in xs:\n\
             \x20   if v is 2:\n\
             \x20       continue\n\
             \x20   over_array be over_array + 1\n\
             print(f\"{over_range} {over_array}\")\n",
        ),
        "4 2\n"
    );
}

// --- The rest of `7674487`'s class -----------------------------------------
//
// The regression test above is the two forms the bug actually shipped in.
// Everything below is the neighbourhood: the other loop forms `continue` and
// `break` reach, the shapes that put a `continue` somewhere other than the
// body's own top level, and the size of body that makes the increment the
// *whole* question rather than one statement among several.
//
// **`examples/10_loops.science` already writes several of these shapes —
// `consume`'s `match` arm with `continue`, `grid_sum`'s and
// `deeply_nested`'s nesting — and none of it would have caught the bug or
// would catch a return of it.** `consume`, `grid_sum`, `deeply_nested`,
// `first_even`, `first_multiple`, `read_until_blank` and `advance` are never
// called from that file's own `main`, so `science_codegen::mono`'s walk never
// reaches them and no backend ever lowers them — the example demonstrates the
// syntax and `corpus_output.rs` pins the bytes `main` prints, and both are
// silent about a function nothing calls. A hanging `consume` would not have
// failed a single test in this repository. These do call what they exercise.

/// `break` in every `for` form, not only the two
/// [`continue_advances_both_kinds_of_for_loop`] covers.
///
/// This is the output half of the pair the module doc's last section asks
/// for: a `break` that fired on the wrong iteration, or that fired and then
/// let the loop run once more anyway, produces a wrong count rather than a
/// hang, and only an execution test sees it.
#[test]
fn break_stops_each_kind_of_for_loop() {
    assert_eq!(
        prints(
            "break-range",
            "let mutable n be 0\n\
             for i in 0..1000:\n\
             \x20   if i is 3:\n\
             \x20       break\n\
             \x20   n be n + 1\n\
             print(f\"{n}\")\n",
        ),
        "3\n"
    );
    assert_eq!(
        prints(
            "break-array",
            "let xs be [10, 20, 30, 40, 50]\n\
             let mutable n be 0\n\
             for x in xs:\n\
             \x20   if x is 30:\n\
             \x20       break\n\
             \x20   n be n + 1\n\
             print(f\"{n}\")\n",
        ),
        "2\n"
    );
    assert_eq!(
        prints(
            "break-chars",
            "let mutable n be 0\n\
             for c in \"abcdef\".chars():\n\
             \x20   if c is 'd':\n\
             \x20       break\n\
             \x20   n be n + 1\n\
             print(f\"{n}\")\n",
        ),
        "3\n"
    );
}

/// `continue` over `Chars`, the one `for` form `7674487`'s fix did not touch.
///
/// A `for` over `Chars` advances by calling `next()` again, and the loop
/// head *is* that call — there is no separate increment block for a
/// `continue` to jump over, which is exactly why this form was never broken.
/// Nothing had run it either, and "was never broken" and "is tested" are
/// different claims; this closes the second one.
#[test]
fn continue_advances_a_for_over_chars_too() {
    assert_eq!(
        prints(
            "continue-chars",
            "let mutable kept be 0\n\
             for c in \"abcbdb\".chars():\n\
             \x20   if c is 'b':\n\
             \x20       continue\n\
             \x20   kept be kept + 1\n\
             print(f\"{kept}\")\n",
        ),
        "3\n"
    );
}

/// `continue` and `break` together in a bare `loop:`.
///
/// `7674487`'s own fix notes *"`loop:` with a `continue` was fine"* — true,
/// because `loop:` has nothing between the end of its body and its test, so
/// `head` and `continue_to` were always the same block for this form. Nothing
/// in this crate had actually run that combination before this test: the only
/// `loop:` execution tests are `stage_two_and_three.rs`'s, and none of them
/// uses `continue`. A claim in a commit message is not a test.
#[test]
fn continue_and_break_together_in_a_bare_loop() {
    assert_eq!(
        prints(
            "loop-continue-break",
            "let mutable i be 0\n\
             let mutable odds be 0\n\
             loop:\n\
             \x20   i be i + 1\n\
             \x20   if i > 10:\n\
             \x20       break\n\
             \x20   if i % 2 is 0:\n\
             \x20       continue\n\
             \x20   odds be odds + 1\n\
             print(f\"{odds}\")\n",
        ),
        "5\n"
    );
}

/// `continue` and `break` in two nested `for` loops at once, each acting on
/// its own loop and not the other's.
///
/// This is `a_for_over_an_array_walks_it`'s nested case with both jumps added:
/// a `continue` in the inner loop must not advance or exit the outer one, and
/// a `break` in the outer loop must stop it without the inner loop's own
/// state leaking an extra iteration in. Each loop keeps its own `LoopScope` on
/// a stack for exactly this reason, and this is the test that would notice a
/// `continue` or `break` resolving to the wrong frame.
#[test]
fn continue_and_break_in_nested_for_loops() {
    assert_eq!(
        prints(
            "nested-continue-break",
            "let mutable total be 0\n\
             for a in [1, 2, 3, 4, 5]:\n\
             \x20   if a is 4:\n\
             \x20       break\n\
             \x20   for b in [10, 20, 30]:\n\
             \x20       if b is 20:\n\
             \x20           continue\n\
             \x20       total be total + a * b\n\
             print(f\"{total}\")\n",
        ),
        "240\n"
    );
}

/// `continue` written inside a `match` arm inside a `for` loop —
/// `examples/10_loops.science`'s `consume` function's own shape, actually
/// called this time.
///
/// The `continue` here is not a direct statement in the loop body; it is an
/// inline arm of a `match` the loop body contains. `Builder::loops` is a
/// stack independent of `match`'s own lowering, so this asks whether a
/// `continue` still finds the right frame — and, since the subject is a
/// `for` over an array, the right frame's `continue_to` is the index
/// increment block and not the loop head, one level of nesting further from
/// the jump than the regression test's `if` puts it.
#[test]
fn continue_inside_a_match_arm_inside_a_for_loop() {
    assert_eq!(
        prints(
            "match-continue",
            "let tokens be [5, 0, 3, 0, 2]\n\
             let mutable kept be 0\n\
             for value in tokens:\n\
             \x20   match value:\n\
             \x20       0: continue\n\
             \x20       other: kept be kept + other\n\
             print(f\"{kept}\")\n",
        ),
        "10\n"
    );
}

/// A loop whose body is `continue` and nothing else, over both `for` forms
/// the bug lived in.
///
/// The regression test's body has an `if` around the `continue` and a
/// statement after it; the increment still has to run when the `continue` is
/// unconditional and there is nothing else in the body for a reader to
/// mistake for "the increment must be in here somewhere". If the increment
/// were still reachable only through the body's fall-through, this would be
/// the shortest program that never reaches the `print` after it.
#[test]
fn a_loop_whose_entire_body_is_continue_still_terminates() {
    assert_eq!(
        prints(
            "empty-body-range",
            "for i in 0..50:\n\
             \x20   continue\n\
             print(\"done\")\n",
        ),
        "done\n"
    );
    assert_eq!(
        prints(
            "empty-body-array",
            "let mutable xs be Array[Int].new()\n\
             xs.push(1)\n\
             xs.push(2)\n\
             xs.push(3)\n\
             for x in xs:\n\
             \x20   continue\n\
             print(\"done\")\n",
        ),
        "done\n"
    );
}
