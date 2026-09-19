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

/// A `for` over an `Array` is still refused, and the refusal is the honest one.
///
/// Not an oversight and not effort: `Array of T implements Iterate` declares
/// `next(mutable self)` with **nowhere to keep a cursor**, so no implementation
/// of it could advance. `collections-and-chains.md`'s AMENDMENT 14 says `Range`
/// implements `Iterate` *"directly"* precisely because it is **not a
/// container**, which reads as containers being meant to hand out an iterator
/// the way `String.chars()` hands out a `Chars`. That is a specification
/// question, and until it is answered this loop has no `next` to call.
#[test]
fn a_for_over_an_array_is_still_refused() {
    let lowered = lower("let xs be [1, 2, 3]\nfor x in xs:\n    print(f\"{x}\")\n");
    let dir = scratch("loops", "array");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a `for` over an `Array` is not lowered");
    assert_eq!(
        diagnostics.first().expect("a diagnostic").code,
        science_codegen::diagnostics::code::SC0400
    );
    let _ = std::fs::remove_dir_all(&dir);
}
