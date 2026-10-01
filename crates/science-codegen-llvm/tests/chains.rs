//! `collections-and-chains.md`'s chain vocabulary, **built, linked, run**.
//!
//! # Why every test here runs the program, and asserts values
//!
//! A chain is six method calls that produce **no value at all** until the
//! terminal: `science-mir`'s `Builder::lower_chain_terminal` reads the whole
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
//! The scalar terminals — `count`, `sum`, `has_any`, `has_all`, `first`,
//! `find` — are measured the same way, and where one of them leaves the loop
//! early the test says so from outside: a `Tracer` whose `drop` prints its
//! tag makes every item the loop *made* visible, so an item it should never
//! have reached is a line too many.
//!
//! # The two shapes that are not here
//!
//! A chain **stored in a variable** (§2.3), and anything but `collect()`
//! **after** `sorted(by:)` (§1.4's barrier). Neither lowers; the second is
//! refused by name at the end of this file, and the type-level refusals —
//! `sum()` over strings, a predicate that is not one — are
//! `science-types/tests/chains.rs`'s, because they never reach a backend.

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

/// `[1, 2, 3, 4, 5]` pushed into `xs`, the source most tests below read.
const FIVE: &str = "    let mutable xs be Array[Int].new()
    xs.push(1)
    xs.push(2)
    xs.push(3)
    xs.push(4)
    xs.push(5)
";

/// `main` with [`FIVE`] and then `body`.
fn over_five(body: &str) -> String {
    format!("def main():\n{FIVE}{body}")
}

/// A type whose `drop` prints its tag, for the tests that have to see which
/// items a loop made and when each was released.
const TRACER: &str = "type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f\"dropped {self.tag}\")

";

/// `[1, 2, 3, 4]`, for the `Tracer` tests, where four drop lines are enough.
const FOUR: &str = "    let mutable xs be Array[Int].new()
    xs.push(1)
    xs.push(2)
    xs.push(3)
    xs.push(4)
";

/// [`TRACER`], then `main` with [`FOUR`] and then `body`.
fn traced(body: &str) -> String {
    format!("{TRACER}def main():\n{FOUR}{body}")
}

/// **`keep` retains what the predicate holds for** — `discard`'s test with the
/// branch the other way round, and the pair in one chain, so that the two
/// directions are checked against each other rather than each against a
/// guess: keep over one, then discard the five.
#[test]
fn keep_retains_what_the_predicate_holds_for() {
    assert_eq!(
        prints(
            "keep",
            &over_five(
                "    let out be xs.iterate().keep(each > 3).collect()
    print(f\"{out.length()} {out[0]} {out[1]}\")
    let both be xs.iterate().keep(each > 1).discard(each is 5).collect()
    print(f\"{both.length()} {both[0]} {both[2]}\")
"
            ),
        ),
        "2 4 5\n3 2 4\n"
    );
}

/// **`skip` passes over the head and yields the rest**, on either side of a
/// `take`: skip-then-take and take-then-skip over the same source are
/// different windows, and each has to come out as the order of its links
/// says. A `skip` longer than the source gives nothing rather than reading
/// past the end.
#[test]
fn skip_passes_over_the_head_of_the_stream() {
    assert_eq!(
        prints(
            "skip",
            &over_five(
                "    let rest be xs.iterate().skip(2).collect()
    print(f\"{rest.length()} {rest[0]} {rest[2]}\")
    let window be xs.iterate().skip(1).take(2).collect()
    print(f\"{window.length()} {window[0]} {window[1]}\")
    let other be xs.iterate().take(4).skip(2).collect()
    print(f\"{other.length()} {other[0]} {other[1]}\")
    let none be xs.iterate().skip(10).collect()
    print(f\"{none.length()}\")
"
            ),
        ),
        "3 3 5\n2 2 3\n2 3 4\n0\n"
    );
}

/// **`count()` counts what survives every link**, and is an `Int` computed
/// by the loop — not the source's length, which only the first line equals.
#[test]
fn count_counts_what_survives_every_link() {
    assert_eq!(
        prints(
            "count",
            &over_five(
                "    print(xs.iterate().count())
    print(xs.iterate().keep(each > 2).count())
    print(xs.iterate().map(each * 10).skip(1).take(3).discard(each is 30).count())
"
            ),
        ),
        "5\n3\n2\n"
    );
}

/// **`sum()` adds borrowed numbers and owned ones**, from zero. The source's
/// items are `&Int` (§4.3) and are read through; a `map` makes them `Int`; an
/// `F64` sum starts from a float zero, and a total with a fraction in it is
/// what an integer zero reinterpreted as a double would get wrong.
#[test]
fn sum_adds_borrowed_and_owned_numbers_from_zero() {
    assert_eq!(
        prints(
            "sum",
            "type Row:
    score: Int

def main():
    let mutable xs be Array[Int].new()
    xs.push(1)
    xs.push(2)
    xs.push(3)
    xs.push(4)
    xs.push(5)
    print(xs.iterate().sum())
    print(xs.iterate().keep(each > 3).sum())
    print(xs.iterate().map(each * each).sum())
    let mutable fs be Array[F64].new()
    fs.push(1.5)
    fs.push(2.25)
    print(fs.iterate().sum())
    let mutable rows be Array[Row].new()
    rows.push(Row(score: 40))
    rows.push(Row(score: 2))
    print(rows.iterate().map(each.score).sum())
",
        ),
        "15\n9\n55\n3.75\n42\n"
    );
}

/// **`has_any` and `has_all` answer, and stop at the item that decides.**
///
/// §1.4 files both as validation predicates and §2.1's laziness says the loop
/// reads no further than it must, and the `Tracer`s make that visible: each
/// item the loop reads is built by the `map` and dropped at the end of its
/// turn, so `has_any(each.tag is 2)` over four items prints two drops and not
/// four. Running out is the other answer — `false` for `has_any`, `true` for
/// `has_all` — and reads every item.
#[test]
fn has_any_and_has_all_stop_at_the_item_that_decides() {
    assert_eq!(
        prints(
            "has",
            &traced(
                "    let hit be xs.iterate().map(x giving Tracer(tag: x)).has_any(each.tag is 2)
    print(f\"any {hit}\")
    let every be xs.iterate().map(x giving Tracer(tag: x * 10)).has_all(each.tag < 20)
    print(f\"all {every}\")
    let never be xs.iterate().has_any(each > 4)
    let always be xs.iterate().has_all(each > 0)
    print(f\"{never} {always}\")
"
            ),
        ),
        "dropped 1\ndropped 2\nany true\ndropped 10\ndropped 20\nall false\nfalse true\n"
    );
}

/// **`first()` and `find` hand back the item, or `null`.** `find` leaves at
/// its match, so the `20` it returns is the only item alive after the loop,
/// released when `found` goes out of scope and not before — and the `30` and
/// `40` are never built at all.
#[test]
fn first_and_find_hand_back_the_item_or_null() {
    assert_eq!(
        prints(
            "first-find",
            &traced(
                "    let head be xs.iterate().skip(1).first()
    if head?:
        print(f\"first {head}\")
    let big be xs.iterate().map(each * 3).find(each > 7)
    if big?:
        print(f\"find {big}\")
    let missing be xs.iterate().find(each > 9)
    if not missing?:
        print(\"find null\")
    let found be xs.iterate().map(x giving Tracer(tag: x * 10)).find(each.tag > 15)
    if found?:
        print(\"found\")
    print(\"end of main\")
"
            ),
        ),
        "first 2\nfind 9\nfind null\ndropped 10\nfound\nend of main\ndropped 20\n"
    );
}

/// **A chain of owned `String`s**, which is where an item has something to
/// release: every item a `discard`, `skip` or `count` passes over is dropped
/// at the end of its turn, and the one `first()` or `find` returns is moved
/// out and still readable. Measured as values and not only as a clean exit —
/// a `first()` handing back a string whose buffer its turn had already freed
/// would print garbage or nothing.
#[test]
fn a_chain_of_owned_strings_hands_back_what_it_keeps() {
    assert_eq!(
        prints(
            "strings",
            "type Doc:
    title: String

def main():
    let mutable docs be Array[Doc].new()
    docs.push(Doc(title: \"alpha\"))
    docs.push(Doc(title: \"be\"))
    docs.push(Doc(title: \"gamma\"))
    let long be docs.iterate().map(d giving f\"{d.title}!\").discard(each.length() < 4).count()
    print(long)
    let second be docs.iterate().map(d giving f\"<{d.title}>\").skip(1).first()
    if second?:
        print(second)
    let named be docs.iterate().map(d giving f\"{d.title}{d.title}\").find(each.length() is 4)
    if named?:
        print(named)
    print(docs.iterate().map(d giving f\"{d.title}\").has_all(each.length() > 1))
",
        ),
        "2\n<be>\nbebe\ntrue\n"
    );
}

/// **An owned item is dropped once by every turn that does not keep it**, on
/// every edge out of the turn: a `discard` that drops it, a `skip` that
/// passes it, a `count` that counts it. The drop lines are the measurement —
/// four items, four drops, in order, before the count is printed; a turn that
/// leaked would print fewer and one that dropped twice more. The `collect`
/// after it keeps two and drops two, and the two it kept go with the array.
#[test]
fn every_item_a_turn_does_not_keep_is_dropped_once() {
    assert_eq!(
        prints(
            "owned-drops",
            &traced(
                "    let n be xs.iterate().map(x giving Tracer(tag: x)).discard(each.tag is 2).skip(1).count()
    print(f\"count {n}\")
    let kept be xs.iterate().map(x giving Tracer(tag: x * 100)).keep(each.tag > 250).collect()
    print(f\"kept {kept.length()}\")
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\ndropped 4\ncount 2\ndropped 100\ndropped 200\nkept 2\n\
         dropped 300\ndropped 400\n"
    );
}

/// **An empty source runs no closure and gives each terminal its empty
/// answer**: zero for `count` and `sum`, `false` for `has_any`, `true` for
/// `has_all`, `null` for `first` and `find`.
#[test]
fn an_empty_source_gives_every_terminal_its_empty_answer() {
    assert_eq!(
        prints(
            "empty-terminals",
            "def main():
    let xs be Array[Int].new()
    print(xs.iterate().count())
    print(xs.iterate().map(each + 1).sum())
    print(xs.iterate().has_any(each > 0))
    print(xs.iterate().has_all(each > 0))
    let head be xs.iterate().first()
    let hit be xs.iterate().find(each > 0)
    if not head? and not hit?:
        print(\"null null\")
",
        ),
        "0\n0\nfalse\ntrue\nnull null\n"
    );
}

/// **A terminal after the barrier is refused by name**, not mis-lowered.
/// `sorted(by:)` then `first()` is §1.4's top-one, and it is exactly the
/// shape `science-mir`'s `Unresolved::Chain` carries: the buffer holds owned
/// items, and what `first()` does with the ones it does not return is the
/// ownership question the note leaves open.
#[test]
fn a_scalar_terminal_after_sorted_is_refused_by_name() {
    let lowered = lower(
        "def main():
    let mutable xs be Array[Int].new()
    xs.push(3)
    xs.push(1)
    let top be xs.iterate().map(each * 1).sorted(by: n giving n).first()
",
    );
    let dir = scratch("chains", "sorted-then-first");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a terminal after the barrier is not lowered");
    let _ = std::fs::remove_dir_all(&dir);
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("sorted(by:)") && first.message.contains("first()"),
        "the refusal must name the barrier and the terminal after it, and it said: {}",
        first.message
    );
}

/// **A predicate after a `take`, `skip` or `keep` over numbers reads through
/// two borrows.** The item there is already `&Int` and a predicate takes
/// `borrowed Item` (§1.2), so `each` is `&&Int`; `science-types`'
/// `read_through_borrows` peels both layers at the operator. Before it, the
/// operand reached the backend as a reference and the build was refused —
/// with the pre-existing `take` and `discard` alone, which is how long the
/// hole had been open.
#[test]
fn a_predicate_after_a_filter_reads_through_both_borrows() {
    assert_eq!(
        prints(
            "two-borrows",
            &over_five(
                "    print(xs.iterate().take(2).discard(each is 1).count())
    print(xs.iterate().skip(1).keep(each > 3).sum())
    print(xs.iterate().keep(each > 1).has_all(each + 1 > 2))
"
            ),
        ),
        "1\n9\ntrue\n"
    );
}
