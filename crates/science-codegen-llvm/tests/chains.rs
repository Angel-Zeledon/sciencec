//! `collections-and-chains.md`'s chain vocabulary, **built, linked, run**.
//!
//! # Why every test here runs the program, and asserts values
//!
//! A chain is six method calls that produce **no value at all** until the
//! terminal: `science-mir`'s `Builder::lower_chain_collect` reads the whole
//! spine off the THIR and emits one loop, so nothing between `iterate()` and
//! `collect()` exists at runtime for anything else to check. There is no
//! intermediate to inspect, no symbol to look for, and no type error to be had
//! — the front end is happy the moment the declarations line up.
//!
//! So the only measurement is what the loop computed. Every assertion below is
//! on the **elements**, not the length: a `discard` with its condition
//! inverted, a `take` that skips instead of stopping, a `map` applied to the
//! wrong end of the chain and a `sorted` that permutes the keys without the
//! values all produce an array of the right size.
//!
//! # The two shapes that are not here
//!
//! A chain **stored in a variable** (§2.3) and a link **after** `sorted(by:)`
//! (§1.4's barrier). Neither lowers; both are refused with a message that says
//! so, and `tests/diagnostics.rs` is where a refusal belongs.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
///
/// `-O2` for `tests/arrays.rs`'s reason, which a chain doubles: the buffer is
/// an `alloca` and an address, and the optimiser is where a loop that reads
/// through the wrong one stops being indistinguishable from one that does not.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("chains", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The source and the terminal on their own: `docs.iterate().collect()`.
///
/// §4.4's first row — *"`iterate()` borrows the source, shared, for the
/// chain's life"* — and §1.4's *"`collect()`: always an `Array`"*. The copy is
/// of borrows, so the elements read back through them and the source is still
/// usable afterwards, which the second line asserts.
#[test]
fn iterate_then_collect_copies_the_source() {
    assert_eq!(
        prints(
            "iterate-collect",
            "def main():\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(10)\n\
             \x20   xs.push(20)\n\
             \x20   xs.push(30)\n\
             \x20   let out be xs.iterate().collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]} {out[2]}\")\n\
             \x20   print(f\"{xs.length()}\")\n",
        ),
        "3 10 20 30\n3\n"
    );
}

/// **`discard` drops what the predicate holds for**, which is the direction
/// §3.1 spends a second verb to make unmistakable: *"a coffee filter keeps the
/// liquid, an air filter keeps the dirt"*.
///
/// Inverting it gives an array of two instead of three, so the length would
/// catch this one — but `[1, 2, 3]` against `[4, 5]` is the assertion that
/// says which way round it went.
#[test]
fn discard_drops_what_the_predicate_holds_for() {
    assert_eq!(
        prints(
            "discard",
            "def main():\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(1)\n\
             \x20   xs.push(2)\n\
             \x20   xs.push(3)\n\
             \x20   xs.push(4)\n\
             \x20   xs.push(5)\n\
             \x20   let out be xs.iterate().discard(each > 3).collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]} {out[2]}\")\n",
        ),
        "3 1 2 3\n"
    );
}

/// `map`, and `take` **stopping** rather than skipping.
///
/// §1.4 calls `take` *"the head of the stream, and the reason laziness pays"*.
/// A `take` that filtered instead of breaking would give the same three
/// elements here; what separates them is that the fused loop leaves at the
/// third push, and the only way to see that from outside is a `take` smaller
/// than the source with the *first* elements in it.
#[test]
fn map_then_take_keeps_the_head_of_the_stream() {
    assert_eq!(
        prints(
            "map-take",
            "def main():\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(1)\n\
             \x20   xs.push(2)\n\
             \x20   xs.push(3)\n\
             \x20   xs.push(4)\n\
             \x20   xs.push(5)\n\
             \x20   let out be xs.iterate().map(each * 10).take(3).collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]} {out[2]}\")\n",
        ),
        "3 10 20 30\n"
    );
}

/// The four links of `headlines` in one chain, over `Int`s so that the
/// arithmetic is readable.
///
/// `[1, 2, 3, 4, 5]`, drop the `3`, times ten, first three: `10 20 40`. Every
/// link has to be in the right order for that to come out — mapping before
/// discarding gives `10 20 40 50` shortened to three as `10 20 30`, and taking
/// before discarding gives `10 20 40` from a different set.
#[test]
fn a_four_link_chain_applies_its_links_in_order() {
    assert_eq!(
        prints(
            "four-links",
            "def main():\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(1)\n\
             \x20   xs.push(2)\n\
             \x20   xs.push(3)\n\
             \x20   xs.push(4)\n\
             \x20   xs.push(5)\n\
             \x20   let out be xs\n\
             \x20       .iterate()\n\
             \x20       .discard(each is 3)\n\
             \x20       .map(each * 10)\n\
             \x20       .take(3)\n\
             \x20       .collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]} {out[2]}\")\n",
        ),
        "3 10 20 40\n"
    );
}

/// **`headlines` itself**: §4.6's headline example, with the `Item` §4.3 gives
/// it.
///
/// The chain is a chain of *borrows* — `.map(each.title)` returns a place
/// expression of a non-`Copy` type, so AMENDMENT 6's copy-out rule hands back
/// a borrow — and `collect()` collects them into an `Array[&String]` that
/// outlives the loop and crosses a function boundary. The lengths are what is
/// printed because `print` of a `&String` is not a rendering this backend has;
/// they are also exactly what a dangling borrow gets wrong.
#[test]
fn the_headlines_chain_collects_borrows_of_the_source() {
    assert_eq!(
        prints(
            "headlines",
            "type Doc:\n\
             \x20   title: String\n\
             \x20   body: String\n\
             \n\
             Doc has:\n\
             \x20   def is_empty(self) -> Bool:\n\
             \x20       self.body.length() is 0\n\
             \n\
             def headlines(docs: &Array[Doc]) -> Array[&String]:\n\
             \x20   docs\n\
             \x20       .iterate()\n\
             \x20       .discard(each.is_empty())\n\
             \x20       .map(each.title)\n\
             \x20       .take(5)\n\
             \x20       .collect()\n\
             \n\
             def main():\n\
             \x20   let mutable docs be Array[Doc].new()\n\
             \x20   docs.push(Doc(title: \"one\", body: \"x\"))\n\
             \x20   docs.push(Doc(title: \"skipped\", body: \"\"))\n\
             \x20   docs.push(Doc(title: \"three\", body: \"y\"))\n\
             \x20   let out be headlines(docs)\n\
             \x20   print(f\"{out.length()} {out[0].length()} {out[1].length()}\")\n",
        ),
        "2 3 5\n"
    );
}

/// **`sorted(by:)`, the barrier**, with the values following their keys.
///
/// §1.4 classes it as a barrier that *"buffers, sorts, yields"*, and
/// `science_array_sort_by_int_key` is the one call a fused chain still makes.
/// The keys are computed on the way into the buffer, so the assertion that
/// matters is that the *elements* came out in key order and are still
/// readable: a sort that permuted the key array and not the value array, or
/// that reordered `String` headers by eight-byte halves, gives four elements
/// of length zero. That is what the first attempt at this did.
#[test]
fn sorted_by_a_key_reorders_the_values_with_it() {
    assert_eq!(
        prints(
            "sorted-by",
            "type Doc:\n\
             \x20   title: String\n\
             \n\
             Doc has:\n\
             \x20   def summarize(self) -> String:\n\
             \x20       let mutable out be String.new()\n\
             \x20       out.push_str(self.title)\n\
             \x20       out\n\
             \n\
             def by_length(docs: &Array[Doc]) -> Array[String]:\n\
             \x20   docs\n\
             \x20       .iterate()\n\
             \x20       .map(doc giving doc.summarize())\n\
             \x20       .sorted(by: line giving line.length())\n\
             \x20       .collect()\n\
             \n\
             def main():\n\
             \x20   let mutable docs be Array[Doc].new()\n\
             \x20   docs.push(Doc(title: \"three\"))\n\
             \x20   docs.push(Doc(title: \"a\"))\n\
             \x20   docs.push(Doc(title: \"tenletters\"))\n\
             \x20   docs.push(Doc(title: \"four\"))\n\
             \x20   let out be by_length(docs)\n\
             \x20   print(f\"{out.length()}\")\n\
             \x20   print(f\"{out[0]} {out[1]} {out[2]} {out[3]}\")\n",
        ),
        "4\na four three tenletters\n"
    );
}

/// The sort is **stable**, which §5.2's reproducibility argument requires one
/// link along: *"this is a language whose users publish"*.
///
/// Three items with the same key, in an order the input fixes. An unstable
/// sort is free to return any of the six permutations and a `sort_unstable`
/// here would return a different one than this on a different day or a
/// different length.
#[test]
fn equal_keys_keep_the_order_they_arrived_in() {
    assert_eq!(
        prints(
            "stable",
            "type Row:\n\
             \x20   tag: Int\n\
             \x20   key: Int\n\
             \n\
             def main():\n\
             \x20   let mutable rows be Array[Row].new()\n\
             \x20   rows.push(Row(tag: 1, key: 7))\n\
             \x20   rows.push(Row(tag: 2, key: 7))\n\
             \x20   rows.push(Row(tag: 3, key: 1))\n\
             \x20   rows.push(Row(tag: 4, key: 7))\n\
             \x20   let out be rows\n\
             \x20       .iterate()\n\
             \x20       .map(each.tag)\n\
             \x20       .sorted(by: n giving n * 0)\n\
             \x20       .collect()\n\
             \x20   print(f\"{out[0]} {out[1]} {out[2]} {out[3]}\")\n",
        ),
        "1 2 3 4\n"
    );
}

/// An **empty source** runs no closure and collects nothing, which is the case
/// `examples/00_kitchen_sink.science` actually exercises and the one a loop
/// with its guard at the bottom would get wrong by running once.
#[test]
fn an_empty_source_collects_nothing() {
    assert_eq!(
        prints(
            "empty",
            "def main():\n\
             \x20   let xs be Array[Int].new()\n\
             \x20   let out be xs.iterate().discard(each > 0).map(each + 1).take(5).collect()\n\
             \x20   print(f\"{out.length()}\")\n",
        ),
        "0\n"
    );
}

/// **`take` past the end of the source stops at the end**, not at the count.
///
/// The guard is two comparisons in one loop — the cursor against the length
/// and the count against the limit — and a chain whose limit is never reached
/// exercises only the first. `examples/00_kitchen_sink.science` writes
/// `.take(5)` over a source of at most three.
#[test]
fn take_larger_than_the_source_takes_the_source() {
    assert_eq!(
        prints(
            "take-past-end",
            "def main():\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(4)\n\
             \x20   xs.push(5)\n\
             \x20   let out be xs.iterate().take(5).collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]}\")\n",
        ),
        "2 4 5\n"
    );
}

/// A closure that **captures** a local, in a chain.
///
/// §7 point 6 wants the capture set recorded from F0, and the chain is where
/// the capture is read on every turn rather than once. The threshold below is
/// a local the predicate closes over, so a lowering that built the closure
/// inside the loop, or that copied the capture at the wrong point, changes the
/// answer rather than failing to build.
#[test]
fn a_chain_closure_reads_a_capture_on_every_turn() {
    assert_eq!(
        prints(
            "capture",
            "def main():\n\
             \x20   let threshold be 2\n\
             \x20   let mutable xs be Array[Int].new()\n\
             \x20   xs.push(1)\n\
             \x20   xs.push(2)\n\
             \x20   xs.push(3)\n\
             \x20   xs.push(4)\n\
             \x20   let out be xs.iterate().discard(each <= threshold).collect()\n\
             \x20   print(f\"{out.length()} {out[0]} {out[1]}\")\n",
        ),
        "2 3 4\n"
    );
}
