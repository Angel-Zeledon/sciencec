//! `Array of T`, **built, linked, run**, with stdout and exit status asserted.
//!
//! # The measurement this file exists for
//!
//! The array literal's *shape* used to stop in `science-types`. THIR had no
//! variant for it — the doc comment on `thir::ExprKind` named the three
//! exhaustive matches in `science-mir` as the cost of adding one — so `[1, 2,
//! 3]` was checked, given a real `Array of I64`, and then handed to MIR as
//! `ExprKind::Error`, which became `Rvalue::Error` and a refusal. Every test
//! that existed for array literals was a `science-types` test, and all of them
//! passed the whole time.
//!
//! # What is built, and the one decision behind it
//!
//! **Decision 5, literally**: a whole-array operation is a runtime call and
//! never an inlined loop, so `[10, 20, 30]` is one
//! `science_array_with_capacity` and one `science_array_push` per element, and
//! the count the runtime reports is the only thing that can tell the two apart
//! from a compiler that emitted nothing. The descriptor those calls need is
//! **not** in MIR: `science-mir` passes the array and the value, and this crate
//! inserts the `ScienceTypeInfo` global at `RuntimeFn::descriptor_index`,
//! because interning a global is a thing only a backend does.
//!
//! # Why every test here runs the program
//!
//! §10's discipline, with this file's own version of it. A `ScienceArray` is
//! `{ ptr, usize, usize }` and a `ScienceString` is `{ ptr, usize, usize }`,
//! and the element pointer `science_array_push` takes is a pointer to the
//! *value* — so a lowering that passed the slot holding the value, or passed a
//! string's descriptor for an array's, emits a module that verifies, links, and
//! reports a length of 3 while owning three copies of the wrong bytes. Only
//! reading back what the program printed separates those.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
///
/// `-O2` for `tests/methods.rs`'s reason: the array is an `alloca` and an
/// address, and the optimiser is where a call through the wrong one stops
/// being indistinguishable from a call through the right one.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("arrays", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The literal reaches an executable at all, which is the sentence that was
/// false.
#[test]
fn an_array_literal_builds_and_runs() {
    assert_eq!(
        prints(
            "literal",
            "let xs be [10, 20, 30]
print(\"built\")
"
        ),
        "built\n"
    );
}

/// The element count is the runtime's own, and it is the first thing that
/// distinguishes three pushes from none.
#[test]
fn the_runtime_counts_what_the_literal_pushed() {
    assert_eq!(
        prints(
            "length",
            "let xs be [10, 20, 30]
print(f\"{xs.length()}\")
"
        ),
        "3\n"
    );
}

/// `Array.new()` and `is_empty`, against a literal in the same program.
///
/// **Both spellings of construction in one run, deliberately.** `[1, 2, 3]`
/// goes through `science_array_with_capacity` and `(Array of Int).new()`
/// through `science_array_new`; they are separate `RUNTIME` entry points that
/// read `T` off the same fallback — the *destination*, since neither call has
/// an array operand to read it from — so a mistake in that fallback is a
/// mistake in both, and a program that only wrote one of them would not say so.
#[test]
fn an_empty_array_and_a_full_one_disagree() {
    assert_eq!(
        prints(
            "is_empty",
            "let xs be [10, 20, 30]
let ys be Array[Int].new()
print(f\"{xs.is_empty()} {ys.is_empty()} {ys.length()}\")
"
        ),
        "false true 0\n"
    );
}

/// The elements come back out, which is the only thing that says the literal
/// stored what it was written with.
///
/// `get` is `(borrowed T)?` and the runtime's out-of-bounds null **is** that
/// option — §3.4's niche in the data word, no branch anywhere — so one program
/// reads an element that exists and asks after one that does not.
#[test]
fn an_element_reads_back_and_a_missing_one_is_null() {
    assert_eq!(
        prints(
            "get",
            "let xs be [10, 20, 30]
let found be xs.get(1)
if found?:
    print(f\"{found}\")
let gone be xs.get(9)
print(f\"{gone?}\")
"
        ),
        "20\nfalse\n"
    );
}

/// `push` puts the value in, and the value is the one that was written.
///
/// **`science_array_push` takes the element's address and MIR hands over a
/// constant**, which has no slot anywhere, so the argument is spilled into one
/// the call site owns. Reading the element back at the index the push put it at
/// is what separates a spill of the right bytes from a spill of a stale slot.
#[test]
fn a_pushed_value_is_the_value_that_was_pushed() {
    assert_eq!(
        prints(
            "push",
            "let mutable xs be [10, 20]
xs.push(77)
let a be xs.get(0)
let b be xs.get(2)
if a? and b?:
    print(f\"{a} {b} {xs.length()}\")
"
        ),
        "10 77 3\n"
    );
}

/// An `Array of String`: Decision 20's `drop_fn`, and the double free it had.
///
/// # What this is measuring
///
/// Three separate things have to be right and the first two were not.
///
/// **The descriptor names a `drop_fn`.** `science_array_free` runs it over the
/// live prefix of the buffer, and an element that owns something is only
/// released because that field points at `science_string_free`.
///
/// **The literal's elements are moved and not borrowed.** They were borrowed
/// first: each temporary `String` was pushed, then dropped at the end of its
/// own statement, and the buffer was freed a second time on the way out. The
/// program printed its length correctly and trapped at exit — so a test that
/// asserted only the length would have passed, and the assertion that catches
/// it is on the **exit status**, which `prints` makes for every test in this
/// file.
///
/// **A borrowed element reaches a method.** `a.length()` on a narrowed
/// `(borrowed String)?` reborrowed the slot rather than the string and printed
/// an address; nothing below MIR can see that, because the argument is a `ptr`
/// either way.
#[test]
fn an_array_of_strings_is_built_read_and_released() {
    assert_eq!(
        prints(
            "strings",
            "let xs be [\"alpha\", \"beta\"]
let a be xs.get(0)
let b be xs.get(1)
if a? and b?:
    print(f\"{a}/{b} {a.length()}\")
"
        ),
        "alpha/beta 5\n"
    );
}

/// `xs[i]`, read and written, with the elements in the order they were put in.
///
/// **Every index in one program, deliberately.** An off-by-one in the address
/// computation shifts every element by the same amount, so a test that read one
/// index would agree with a lowering that was uniformly wrong; three ascending
/// indices and a write to the middle one do not.
#[test]
fn an_index_reads_and_writes_the_element_it_names() {
    assert_eq!(
        prints(
            "index",
            "let xs be [10, 20, 30]
print(f\"{xs[0]} {xs[1]} {xs[2]}\")
"
        ),
        "10 20 30\n"
    );
    assert_eq!(
        prints(
            "index_write",
            "let mutable xs be [10, 20, 30]
xs[1] be 99
print(f\"{xs[0]} {xs[1]} {xs[2]}\")
"
        ),
        "10 99 30\n"
    );
}

/// §2.4's bounds check fires, at both ends.
///
/// **The negative index is the half that says the comparison is unsigned.** The
/// check `science-mir` emits is one `<` with both sides cast to `U64`, on the
/// ground that a negative `i64` reinterprets above any length an array can
/// have; a signed comparison would pass `-1` straight through to
/// `science_array_get_mut` and index behind the buffer. Both ends are asserted
/// because only one of them can distinguish the two.
#[test]
fn an_index_out_of_bounds_panics_at_either_end() {
    for index in ["3", "-1"] {
        let source = format!(
            "let xs be [10, 20, 30]
let i be {index}
print(f\"{{xs[i]}}\")
"
        );
        let dir = scratch("arrays", "bounds");
        require_runtime();
        let built = lower(&source).build_at(&executable(&dir, "bounds"), OptLevel::O2);
        let ran = run(&built);
        let _ = std::fs::remove_dir_all(&dir);
        assert_ne!(ran.status, Some(0), "index {index} did not fail");
        assert!(
            ran.stderr.contains("index out of bounds"),
            "index {index} failed without saying which check fired: {}",
            ran.stderr
        );
        assert_eq!(ran.stdout, "", "nothing was printed before the panic");
    }
}

/// An index whose element owns something, which is the composition of the two
/// halves this file is about.
#[test]
fn a_string_element_is_indexed_and_used() {
    assert_eq!(
        prints(
            "index_strings",
            "let xs be [\"alpha\", \"beta\"]
print(f\"{xs[1]} {xs[0].length()}\")
"
        ),
        "beta 5\n"
    );
}

/// The most ordinary loop there is, run: index, accumulate, divide, report.
///
/// # Why this test is here and not in a smaller one
///
/// `total be total + xs[i]` was **refused** while `print(f"{xs[i]}")` worked,
/// and the gap between those two sentences is the whole finding. An f-string
/// hole reads through `value_hole`, which never asks for a coercion; an
/// arithmetic operand goes through `Coercion::Copy`, and that coercion is
/// `science-types` saying *"`Index.index` returns `&T`, load it"* about a place
/// `science-mir` had already lowered to the element itself. Two true views of
/// one expression, and the backend refused the join.
///
/// Nothing smaller would have found it. Every array test in this file reads an
/// element through a `print`, which is the one position that does not take the
/// path that was broken — so the suite was green and the single most common
/// loop in any program did not build.
///
/// The program is kept whole rather than reduced for that reason: it indexes,
/// accumulates across iterations, divides (which is its own guard), and prints.
/// A reduction to `10 + xs[1]` reproduces the refusal but would not have been
/// written, because nobody doubted that arithmetic worked.
#[test]
fn a_counting_loop_over_an_array_adds_up() {
    assert_eq!(
        prints(
            "counting-loop",
            "let xs: Array[Int] be [4, 8, 15, 16, 23, 42]\n\
             let n be xs.length()\n\
             let mutable total be 0\n\
             let mutable i be 0\n\
             loop:\n\
             \x20   if i is n:\n\
             \x20       break\n\
             \x20   total be total + xs[i]\n\
             \x20   i be i + 1\n\
             print(f\"n={n} total={total} mean={total / n}\")\n",
        ),
        "n=6 total=108 mean=18\n"
    );
}
