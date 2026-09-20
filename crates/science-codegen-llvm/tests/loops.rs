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
