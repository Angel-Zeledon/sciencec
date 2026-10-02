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

/// **Links and terminals after a barrier run over the buffer it filled**, and
/// the buffer is popped, so an owned item is moved out exactly once and what
/// the chain never reaches is released with the buffer. `sorted(by:)` then
/// `first()` is §1.4's top-one: the `1` is returned, the `4`, `3` and `2` it
/// did not return are dropped when the statement ends. `take(1)` after a
/// descending sort pops the `4`, pops the `3` and drops it at the limit, and
/// the `1` and `2` the loop never reached go with the buffer. `top` itself is
/// a binding of `main` and is dropped when `main` ends: reading its field
/// inside `if top?:` no longer moves the payload out and releases it early.
#[test]
fn a_terminal_after_the_barrier_pops_an_owned_buffer() {
    assert_eq!(
        prints(
            "sorted-then-first",
            "type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f\"dropped {self.tag}\")

def main():
    let mutable xs be Array[Int].new()
    xs.push(3)
    xs.push(1)
    xs.push(4)
    xs.push(2)
    let top be xs.iterate().map(x giving Tracer(tag: x)).sorted(by: t giving t.tag).first()
    print(\"after first\")
    if top?:
        print(f\"top {top.tag}\")
    let best be xs.iterate().map(x giving Tracer(tag: x)).sorted(by: t giving 0 - t.tag).take(1).collect()
    print(f\"best {best[0].tag}\")
    print(\"end\")
",
        ),
        "dropped 4\ndropped 3\ndropped 2\nafter first\ntop 1\n\
         dropped 3\ndropped 1\ndropped 2\nbest 4\nend\ndropped 4\ndropped 1\n"
    );
}

/// **`reverse()` is a barrier that turns the stream round**: alone, twice (the
/// identity), after a sort (top-k is `sorted(by:).reverse().take(k)`), before
/// a `map` and a `keep`, as a `for`'s subject, and between two sorts.
#[test]
fn reverse_turns_the_chain_round() {
    assert_eq!(
        prints(
            "reverse",
            "def main():
    let mutable xs be Array[Int].new()
    xs.push(3)
    xs.push(1)
    xs.push(4)
    xs.push(2)
    let r be xs.iterate().reverse().collect()
    print(f\"{r[0]} {r[1]} {r[2]} {r[3]}\")
    let rr be xs.iterate().reverse().reverse().collect()
    print(f\"{rr[0]} {rr[3]}\")
    let top be xs.iterate().sorted(by: each * 1).reverse().take(2).collect()
    print(f\"{top.length()} {top[0]} {top[1]}\")
    let low be xs.iterate().sorted(by: each * 1).take(3).collect()
    print(f\"{low[0]} {low[1]} {low[2]}\")
    print(xs.iterate().sorted(by: each * 1).sum())
    let f be xs.iterate().reverse().first()
    if f?:
        print(f\"first {f}\")
    let m be xs.iterate().reverse().map(each * 10).keep(each > 10).collect()
    print(f\"{m.length()} {m[0]} {m[1]} {m[2]}\")
    for x in xs.iterate().sorted(by: each * 1).reverse():
        print(x)
    let both be xs.iterate().sorted(by: 0 - each).sorted(by: each * 1).collect()
    print(f\"{both[0]} {both[3]}\")
",
        ),
        "2 4 1 3\n3 2\n2 4 3\n1 2 3\n10\nfirst 2\n3 20 40 30\n4\n3\n2\n1\n1 4\n"
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

// --- the second tranche: `numbered`, the filters that read a prefix, the
// --- terminals that keep one item, and `Map`'s sources ----------------------

/// **`numbered()` pairs each item with the position it arrived at**, from zero
/// and counted *where it sits in the chain*: after a `skip` the first item to
/// arrive is number zero, which is what makes `numbered().skip(…)` and
/// `skip(…).numbered()` different programs.
///
/// The record is §1.3's, so `each.index` and `each.item` work as `each` does on
/// any other: a predicate reads the position, a `map` takes the item out. The
/// `map(each.item)` runs over `Int`s the chain owns — over borrows it is the
/// regions engine's `SC0340`, which is its own finding and not this test's.
#[test]
fn numbered_pairs_each_item_with_where_it_arrived() {
    assert_eq!(
        prints(
            "numbered",
            &over_five(
                "    let n be xs.iterate().numbered().collect()
    print(f\"{n.length()} {n[0].index} {n[0].item} {n[4].index} {n[4].item}\")
    let late be xs.iterate().map(each * 10).numbered().discard(each.index < 3).map(each.item).collect()
    print(f\"{late.length()} {late[0]} {late[1]}\")
    print(xs.iterate().numbered().keep(each.index > 2).map(each.index).sum())
    let after_skip be xs.iterate().skip(2).numbered().map(each.index).collect()
    print(f\"{after_skip[0]} {after_skip[1]} {after_skip[2]}\")
    let before_skip be xs.iterate().numbered().skip(2).map(each.index).collect()
    print(f\"{before_skip[0]} {before_skip[1]} {before_skip[2]}\")
"
            ),
        ),
        "5 0 1 4 5\n2 40 50\n7\n0 1 2\n2 3 4\n"
    );
}

/// **A `Numbered` over owned items owns them**: `map` takes the record by
/// value and the closure releases it, and a `discard` that drops one releases
/// the item inside. Four items, four `dropped` lines, each after the turn that
/// made it and before the result is printed.
#[test]
fn a_numbered_record_owns_the_item_it_holds() {
    assert_eq!(
        prints(
            "numbered-owned",
            &traced(
                "    let out be xs.iterate().map(x giving Tracer(tag: x)).numbered().discard(each.index is 1).map(p giving p.item.tag * 10 + p.index).collect()
    print(f\"{out.length()} {out[0]} {out[1]} {out[2]}\")
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\ndropped 4\n3 10 32 43\n"
    );
}

/// **`take_while` ends the chain at the first item the predicate does not hold
/// for**, and reads nothing after it: with a `Tracer` per item, the `4` is
/// never built, and the `3` that failed the test is released by the turn that
/// made it. `[1, 5, 2, 0]` is the data that tells it from `keep`, which would
/// pass the `2` as well.
#[test]
fn take_while_stops_at_the_first_miss() {
    assert_eq!(
        prints(
            "take-while",
            &traced(
                "    let n be xs.iterate().map(x giving Tracer(tag: x)).take_while(each.tag < 3).count()
    print(f\"count {n}\")
    let mutable ys be Array[Int].new()
    ys.push(1)
    ys.push(5)
    ys.push(2)
    ys.push(0)
    let head be ys.iterate().take_while(each < 3).collect()
    print(f\"{head.length()} {head[0]}\")
    print(ys.iterate().take_while(each > 9).count())
    print(ys.iterate().take_while(each >= 0).count())
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\ncount 2\n1 1\n0\n4\n"
    );
}

/// **`skip_while` swallows until the first miss and then passes everything**,
/// including later items the predicate would have swallowed: `[1, 5, 2, 0]`
/// past `< 3` is `[5, 2, 0]`, not `[5]`.
#[test]
fn skip_while_passes_everything_after_the_first_miss() {
    assert_eq!(
        prints(
            "skip-while",
            &traced(
                "    let mutable ys be Array[Int].new()
    ys.push(1)
    ys.push(5)
    ys.push(2)
    ys.push(0)
    let tail be ys.iterate().skip_while(each < 3).collect()
    print(f\"{tail.length()} {tail[0]} {tail[1]} {tail[2]}\")
    print(ys.iterate().skip_while(each > 9).count())
    print(ys.iterate().skip_while(each >= 0).count())
    let n be xs.iterate().map(x giving Tracer(tag: x)).skip_while(each.tag < 3).count()
    print(f\"count {n}\")
"
            ),
        ),
        "3 5 2 0\n4\n0\ndropped 1\ndropped 2\ndropped 3\ndropped 4\ncount 2\n"
    );
}

/// **`every(n)` passes the first item and every `n`th after it** — §1.4's
/// `step_by`. Counted over what reaches it, so `keep(each > 1).every(2)` over
/// `[1, 2, 3, 4, 5]` is `[2, 4]` and not the odd positions of the source.
/// `every(1)` passes everything; an empty source passes nothing.
#[test]
fn every_passes_the_first_of_each_run_of_n() {
    assert_eq!(
        prints(
            "every",
            &over_five(
                "    let two be xs.iterate().every(2).collect()
    print(f\"{two.length()} {two[0]} {two[1]} {two[2]}\")
    let three be xs.iterate().every(3).collect()
    print(f\"{three.length()} {three[0]} {three[1]}\")
    print(xs.iterate().every(1).count())
    print(xs.iterate().every(5).count())
    print(xs.iterate().every(9).count())
    let after be xs.iterate().keep(each > 1).every(2).collect()
    print(f\"{after[0]} {after[1]}\")
    let empty be Array[Int].new()
    print(empty.iterate().every(2).count())
"
            ),
        ),
        "3 1 3 5\n2 1 4\n5\n1\n1\n2 4\n0\n"
    );
}

/// **`last()` is the final item, or `null`**, and walks the whole chain to
/// say so. Over owned items every item it displaces is released as it is
/// replaced — `dropped 1`, `2` and `3` before `got` — and the one it returns
/// is the last thing alive, released when `l` goes out of scope.
#[test]
fn last_keeps_the_final_item_and_releases_the_rest() {
    assert_eq!(
        prints(
            "last",
            &traced(
                "    let l be xs.iterate().map(x giving Tracer(tag: x)).last()
    if l?:
        print(\"got\")
    let n be xs.iterate().map(each * 3).last()
    if n?:
        print(f\"last {n}\")
    let none be xs.iterate().discard(each > 0).last()
    if not none?:
        print(\"last null\")
    print(\"end\")
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\ngot\nlast 12\nlast null\nend\ndropped 4\n"
    );
}

/// A record the extremes can be asked about: an id, and what is measured.
const ROW: &str = "type Row:
    id: Int
    score: Int

";

/// **`minimum(by:)` and `maximum(by:)` hand back the item with the least or
/// greatest key — the record, not the score** — and the **first** of equal keys
/// wins in both, which is the same answer from the same data whichever way
/// round the question is asked. `[(1,5) (2,3) (3,3) (4,9)]`: the least score
/// is row 2 and not row 3; the greatest *negated* score is row 2 too.
#[test]
fn the_extremes_hand_back_the_record_and_the_first_of_equal_keys() {
    assert_eq!(
        prints(
            "extremes",
            &format!(
                "{ROW}def main():
    let mutable rows be Array[Row].new()
    rows.push(Row(id: 1, score: 5))
    rows.push(Row(id: 2, score: 3))
    rows.push(Row(id: 3, score: 3))
    rows.push(Row(id: 4, score: 9))
    let low be rows.iterate().minimum(by: each.score)
    let high be rows.iterate().maximum(by: each.score)
    let flipped be rows.iterate().maximum(by: row giving 0 - row.score)
    if low? and high? and flipped?:
        print(f\"{{low.id}} {{high.id}} {{flipped.id}}\")
    let empty be Array[Row].new()
    let nobody be empty.iterate().minimum(by: each.score)
    let nothing be empty.iterate().maximum(by: each.score)
    if not nobody? and not nothing?:
        print(\"null null\")
"
            ),
        ),
        "2 4 2\nnull null\n"
    );
}

/// **An extreme over owned items releases what it displaces and what it
/// passes over**: the first item is taken, each better one drops the holder,
/// and each that is not better is dropped by its own turn. `minimum` over
/// `10 - tag` keeps the `4` and drops `1`, `2`, `3` as they are replaced;
/// `maximum` over the same key keeps the `1` and drops `2`, `3`, `4` as they
/// arrive.
#[test]
fn an_extreme_over_owned_items_drops_every_one_it_does_not_keep() {
    assert_eq!(
        prints(
            "extremes-owned",
            &traced(
                "    let best be xs.iterate().map(x giving Tracer(tag: x)).minimum(by: t giving 10 - t.tag)
    if best?:
        print(\"min\")
    let worst be xs.iterate().map(x giving Tracer(tag: x)).maximum(by: t giving 10 - t.tag)
    if worst?:
        print(\"max\")
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\nmin\ndropped 2\ndropped 3\ndropped 4\nmax\n\
         dropped 1\ndropped 4\n"
    );
}

/// **`for` over a chain is the chain with the body as its terminal.** The
/// links run as they do before `collect`, `continue` goes to the next item and
/// `break` leaves the chain, and an owned item the pattern binds is released
/// at the end of the turn that bound it — including the turns that `continue`
/// and `break` cut short. `1` is dropped by the `keep` and never reaches the
/// body.
#[test]
fn a_for_over_a_chain_runs_the_body_as_its_terminal() {
    assert_eq!(
        prints(
            "for-chain",
            &traced(
                "    for t in xs.iterate().map(x giving Tracer(tag: x)).keep(each.tag > 1):
        if t.tag is 3:
            continue
        if t.tag is 4:
            break
        print(f\"body {t.tag}\")
    print(\"after\")
    for x in xs.iterate().skip(1).take(2):
        print(x)
    for pair in xs.iterate().numbered().keep(each.index > 1):
        print(f\"{pair.index} {pair.item}\")
"
            ),
        ),
        "dropped 1\nbody 2\ndropped 2\ndropped 3\ndropped 4\nafter\n2\n3\n2 3\n3 4\n"
    );
}

/// **A `for` over an empty chain, or a chain that filters everything out,
/// runs no turn**, and a `for` over `iterate()` alone reads the source in
/// order and leaves it usable.
#[test]
fn a_for_over_an_empty_chain_runs_no_turn() {
    assert_eq!(
        prints(
            "for-empty",
            "def main():
    let empty be Array[Int].new()
    for x in empty.iterate():
        print(\"never\")
    let mutable xs be Array[Int].new()
    xs.push(1)
    xs.push(2)
    for x in xs.iterate().discard(each > 0):
        print(\"never\")
    for x in xs.iterate():
        print(x)
    print(xs.length())
",
        ),
        "1\n2\n2\n"
    );
}

/// `a`, `b`, `c` pushed into a `Map[String, Int]` under `1`, `2`, `3`.
const ABC: &str = "    let mutable m be Map[String, Int].new()
    m.insert(\"a\", 1)
    m.insert(\"b\", 2)
    m.insert(\"c\", 3)
";

/// **`Map.keys()` and `Map.values()` are §5.4's chain sources**, in insertion
/// order (§5.2): every adapter and terminal the `Array` source has works on
/// them, and `iterate()` yields the `Entry` `for entry in m:` does.
#[test]
fn a_maps_keys_and_values_are_chain_sources() {
    assert_eq!(
        prints(
            "map-sources",
            &format!(
                "def main():\n{ABC}    let ks be m.keys().collect()
    print(f\"{{ks.length()}} {{ks[0]}} {{ks[1]}} {{ks[2]}}\")
    print(m.values().sum())
    print(m.values().keep(each > 1).count())
    let shouted be m.keys().map(k giving f\"{{k}}!\").collect()
    print(f\"{{shouted[0]}} {{shouted[2]}}\")
    let es be m.iterate().map(e giving e.value * 10).collect()
    print(f\"{{es[0]}} {{es[2]}}\")
    let top be m.values().keep(each > 2).first()
    if top?:
        print(f\"top {{top}}\")
    let first_key be m.keys().first()
    if first_key?:
        print(f\"{{first_key}}\")
    print(m.keys().numbered().map(each.index).sum())
"
            ),
        ),
        "3 a b c\n6\n2\na! c!\n10 30\ntop 3\na\n3\n"
    );
}

/// **`for k in m.keys():` and `for v in m.values():`**, in insertion order,
/// over a map that has had a key removed — the hole `remove` leaves is stepped
/// over, as `for entry in m:` steps over it — and over an empty map, which
/// runs no turn.
#[test]
fn a_for_over_a_maps_sources_walks_the_live_entries() {
    assert_eq!(
        prints(
            "for-map",
            &format!(
                "def main():\n{ABC}    for k in m.keys():
        print(k)
    for v in m.values():
        print(v)
    for e in m.iterate():
        print(f\"{{e.key}}={{e.value}}\")
    m.remove(\"b\")
    let rest be m.keys().collect()
    print(f\"{{rest.length()}} {{rest[0]}} {{rest[1]}}\")
    for v in m.values():
        print(v)
    let empty be Map[String, Int].new()
    for k in empty.keys():
        print(\"never\")
    print(empty.values().count())
"
            ),
        ),
        "a\nb\nc\n1\n2\n3\na=1\nb=2\nc=3\n2 a c\n1\n3\n0\n"
    );
}

/// **`zip(other)` pairs each item with the next element of `other`** — §1.3's
/// `Pair`, `left` the chain's item and `right` a borrow of the array's — and
/// ends when either side does. The position in `other` is counted over what
/// reaches the link, so `skip(1).zip(…)` starts `other` at its front. Over
/// owned items the one in hand when `other` runs out is released, and nothing
/// after it is built: three `Tracer`s against two names is three drops and a
/// count of two.
#[test]
fn zip_pairs_items_until_either_side_runs_out() {
    assert_eq!(
        prints(
            "zip",
            &traced(
                "    let mutable names be Array[String].new()
    names.push(\"a\")
    names.push(\"b\")
    let z be xs.iterate().zip(names.iterate()).collect()
    print(f\"{z.length()} {z[0].left} {z[0].right} {z[1].left} {z[1].right}\")
    let mutable ys be Array[Int].new()
    ys.push(10)
    ys.push(20)
    ys.push(30)
    ys.push(40)
    ys.push(50)
    print(xs.iterate().zip(ys.iterate()).map(p giving p.left * p.right).sum())
    for p in ys.iterate().skip(3).zip(xs.iterate()):
        print(f\"{p.left} {p.right}\")
    let empty be Array[Int].new()
    print(xs.iterate().zip(empty.iterate()).count())
    let n be xs.iterate().map(x giving Tracer(tag: x)).zip(names.iterate()).count()
    print(f\"count {n}\")
"
            ),
        ),
        "2 1 a 2 b\n300\n40 1\n50 2\n0\ndropped 1\ndropped 2\ndropped 3\ncount 2\n"
    );
}

/// **A closure that takes an aggregate by value is called with its address**,
/// as any Science function is: `map` over an owned record hands the closure
/// the record itself, and a record is more than a register. Before this the
/// loaded value went where a pointer was declared and the module failed
/// verification — the shape every `numbered().map(each.index)` has, and any
/// `map(p giving …)` over a chain that has already built a record.
#[test]
fn a_closure_taking_a_record_by_value_is_called_by_address() {
    assert_eq!(
        prints(
            "closure-record",
            "type Point:
    x: Int
    y: Int

def main():
    let mutable xs be Array[Int].new()
    xs.push(10)
    xs.push(20)
    let sums be xs.iterate().map(n giving Point(x: n, y: 1)).map(p giving p.x + p.y).collect()
    print(f\"{sums[0]} {sums[1]}\")
    let doubled be xs.iterate().map(n giving f\"{n}\").map(s giving s.length()).sum()
    print(doubled)
",
        ),
        "11 21\n4\n"
    );
}

/// **A `zip` whose other side is not an `Array.iterate()` is refused by
/// name.** The declaration takes an `ArrayIterate[U]`, so a bare array leaves
/// `U` unsolved and the checker has nothing to report; the chain then does not
/// fuse, and the backend names the `Zip` it was asked to build as a value —
/// with the unsolved `{unknown}` in it, which is the finding.
#[test]
fn a_zip_with_anything_but_an_array_source_is_refused() {
    let lowered = lower(
        "def main():
    let mutable xs be Array[Int].new()
    xs.push(1)
    let all be xs.iterate().zip(xs).collect()
    print(all.length())
",
    );
    let dir = scratch("chains", "zip-refused");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a zip with no source on the right is not lowered");
    let _ = std::fs::remove_dir_all(&dir);
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(first.message.contains("Zip["), "it must name the chain it could not build: {}", first.message);
}

// --- the third tranche: reduce, product ------------------------------------

/// **`reduce` folds with the two-parameter closure `(acc, x) giving ...`**,
/// from the initial value, in order. Subtraction is the order's witness: a fold
/// from the right gives a different number, and so does one that starts from
/// the first item instead of from `initial`.
#[test]
fn reduce_folds_from_the_initial_value_in_order() {
    assert_eq!(
        prints(
            "reduce",
            &over_five(
                "    print(xs.iterate().reduce(0, (acc, x) giving acc + x))
    print(xs.iterate().reduce(100, (acc, x) giving acc - x))
    print(xs.iterate().reduce(10, (a, b) giving if b > a: b else: a))
    let bound be 3
    print(xs.iterate().keep(each > 1).map(each * 2).reduce(1, (acc, x) giving acc * x + bound))
    let empty be xs.iterate().discard(each > 0).reduce(42, (acc, x) giving acc + x)
    print(empty)
    let text be xs.iterate().map(each * 7).reduce(\"\", (acc, x) giving f\"{acc}<{x}>\")
    print(text)
"
            ),
        ),
        "15\n85\n10\n3633\n42\n<7><14><21><28><35>\n"
    );
}

/// **A `reduce` over owned items and with an owned accumulator releases every
/// one**: each item is dropped by the closure that takes it, and each
/// accumulator it replaces is dropped by the next call, so the only `Tracer`
/// alive at the end is the result, released when it goes out of scope.
#[test]
fn reduce_releases_the_items_and_the_accumulators_it_replaces() {
    assert_eq!(
        prints(
            "reduce-owned",
            &traced(
                "    let sum be xs.iterate().map(x giving Tracer(tag: x)).reduce(0, (acc, t) giving acc + t.tag)
    print(f\"sum {sum}\")
    let last be xs.iterate().reduce(Tracer(tag: 0), (acc, x) giving Tracer(tag: acc.tag + x))
    print(f\"end {last.tag}\")
"
            ),
        ),
        "dropped 1\ndropped 2\ndropped 3\ndropped 4\nsum 10\n\
         dropped 0\ndropped 1\ndropped 3\ndropped 6\nend 10\ndropped 10\n"
    );
}

/// **`product` is `sum`'s twin from one**: an `Int` chain, a float chain, a
/// chain after a `map`, and the empty chain, whose product is the identity.
#[test]
fn product_multiplies_from_one() {
    assert_eq!(
        prints(
            "product",
            &over_five(
                "    print(xs.iterate().product())
    print(xs.iterate().map(each + 1).product())
    print(xs.iterate().discard(each > 0).product())
    let mutable fs be Array[F64].new()
    fs.push(1.5)
    fs.push(4.0)
    print(fs.iterate().product())
"
            ),
        ),
        "120\n720\n1\n6.0\n"
    );
}

// --- keep_some --------------------------------------------------------------

/// **`keep_some()` drops the absent items and unwraps the rest**, over owned
/// `T?` items (a `map` that can fail), and over a borrowed nullable of a type
/// that owns nothing (an `Array[Int?]`). An owned payload that survives is
/// moved on, so the `Tracer`s a `map` made are each released exactly once —
/// by the closure that takes them — and the absent ones leave nothing behind.
#[test]
fn keep_some_drops_the_absent_and_unwraps_the_rest() {
    assert_eq!(
        prints(
            "keep-some",
            "type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f\"dropped {self.tag}\")

def half(n: Int) -> Int?:
    if n % 2 is 0:
        return n / 2
    null

def traced(n: Int) -> Tracer?:
    if n > 2:
        return Tracer(tag: n)
    null

def label(n: Int) -> String?:
    if n > 1:
        return f\"n{n}\"
    null

def main():
    let mutable xs be Array[Int].new()
    xs.push(1)
    xs.push(2)
    xs.push(3)
    xs.push(4)
    let halves be xs.iterate().map(x giving half(x)).keep_some().collect()
    print(f\"{halves.length()} {halves[0]} {halves[1]}\")
    let t be xs.iterate().map(x giving traced(x)).keep_some().map(each.tag).sum()
    print(f\"sum {t}\")
    let words be xs.iterate().map(x giving label(x)).keep_some().collect()
    print(f\"{words.length()} {words[0]} {words[2]}\")
    let mutable ns be Array[Int?].new()
    ns.push(5)
    ns.push(null)
    ns.push(7)
    let kept be ns.iterate().keep_some().collect()
    print(f\"{kept.length()} {kept[0]} {kept[1]}\")
    print(ns.iterate().keep_some().count())
    for v in xs.iterate().map(x giving traced(x)).keep_some():
        print(f\"loop {v.tag}\")
",
        ),
        "2 1 2\ndropped 3\ndropped 4\nsum 7\n3 n2 n4\n2 5 7\n2\n\
         loop 3\ndropped 3\nloop 4\ndropped 4\n"
    );
}

// --- accumulate -------------------------------------------------------------

/// **`accumulate(initial, f)` yields the running state after each item** —
/// `reduce` that keeps its intermediates. The first value is `f(initial,
/// first)`, not `initial`; a link before it filters, a link after it
/// truncates, and `last()` of the chain is what `reduce` would have been.
#[test]
fn accumulate_yields_the_state_after_each_item() {
    assert_eq!(
        prints(
            "accumulate",
            &over_five(
                "    let running be xs.iterate().accumulate(0, (acc, x) giving acc + x).collect()
    print(f\"{running.length()} {running[0]} {running[1]} {running[4]}\")
    let doubling be xs.iterate().accumulate(0.5, (acc, x) giving acc * 2.0).take(3).collect()
    print(f\"{doubling[0]} {doubling[2]}\")
    let tail be xs.iterate().keep(each > 1).accumulate(10, (acc, x) giving acc - x).last()
    if tail?:
        print(tail)
    let total be xs.iterate().accumulate(1, (acc, x) giving acc * x).last()
    if total?:
        print(total)
    for r in xs.iterate().accumulate(0, (acc, x) giving acc + x).skip(3):
        print(r)
"
            ),
        ),
        "5 1 3 15\n1.0 4.0\n-4\n120\n10\n15\n"
    );
}

// --- unique -----------------------------------------------------------------

/// **`unique()` keeps the first of each distinct item**, in arrival order, over
/// borrowed numbers, borrowed strings and owned strings (a `map` that clones),
/// and composes with the links on either side of it: the filter before it runs
/// first, the `take` after it sees only the survivors.
#[test]
fn unique_keeps_the_first_of_each_distinct_item() {
    assert_eq!(
        prints(
            "unique",
            r#"def main():
    let xs be [3, 1, 3, 2, 1, 4]
    let u be xs.iterate().unique().collect()
    print(f"{u.length()} {u[0]} {u[1]} {u[2]} {u[3]}")
    let words be ["b", "a", "b", "c", "a"]
    let w be words.iterate().unique().collect()
    print(f"{w.length()} {w[0]} {w[1]} {w[2]}")
    let owned be words.iterate().map(each.clone()).unique().collect()
    print(f"{owned.length()} {owned[0]} {owned[1]} {owned[2]}")
    print(xs.iterate().unique().count())
    print(xs.iterate().unique().take(2).sum())
    print(xs.iterate().discard(each is 3).unique().sum())
    for x in xs.iterate().unique().skip(2):
        print(x)
    let empty be Array[Int].new()
    print(empty.iterate().unique().count())
"#,
        ),
        "4 3 1 2 4\n3 b a c\n3 b a c\n4\n4\n7\n2\n4\n0\n"
    );
}

// --- windows ----------------------------------------------------------------

/// **`windows(n)` yields every run of `n` consecutive items as an `Array`**,
/// overlapping, in order; a source shorter than `n` yields none, and a width
/// below one yields none. The items may be borrows (an `iterate()`) or owned
/// (a `map` that clones `String`s), and the links on either side compose: a
/// `first()` stops after the first window, a `take` after the second.
#[test]
fn windows_yields_overlapping_runs_in_order() {
    assert_eq!(
        prints(
            "windows",
            r#"def main():
    let xs be [1, 2, 3, 4, 5]
    let w be xs.iterate().windows(3).collect()
    print(f"{w.length()}")
    for win in w:
        print(f"{win[0]} {win[1]} {win[2]}")
    print(xs.iterate().windows(5).count())
    print(xs.iterate().windows(6).count())
    print(xs.iterate().windows(1).count())
    print(xs.iterate().windows(0).count())
    let words be ["a", "b", "c"]
    for pair in words.iterate().map(each.clone()).windows(2):
        print(f"{pair[0]}{pair[1]}")
    let firstw be xs.iterate().windows(2).first()
    if firstw?:
        print(f"first {firstw.length()}")
    print(xs.iterate().keep(each > 1).windows(2).take(2).count())
    print(xs.iterate().windows(2).map(each.length()).sum())
"#,
        ),
        "3\n1 2 3\n2 3 4\n3 4 5\n1\n0\n5\n0\nab\nbc\nfirst 2\n2\n8\n"
    );
}

// --- batches ----------------------------------------------------------------

/// **`batches(n)` cuts the chain into disjoint arrays of `n`, the last one
/// shorter** when `n` does not divide the count: 7 items in threes is 3, 3, 1.
/// A size at or above the count is one batch, an empty source has none, and a
/// size below one is a batch of one. It is a barrier (the short batch needs to
/// know which item was last), so what follows it runs over the buffer: a
/// `take`, a `map`, a `sorted` before it, and a `for` all compose.
#[test]
fn batches_cut_the_chain_into_disjoint_arrays() {
    assert_eq!(
        prints(
            "batches",
            r#"def main():
    let xs be [1, 2, 3, 4, 5, 6, 7]
    let b be xs.iterate().batches(3).collect()
    print(f"{b.length()} {b[0].length()} {b[1].length()} {b[2].length()}")
    print(f"{b[0][0]} {b[1][0]} {b[2][0]}")
    for batch in xs.iterate().batches(2):
        print(f"{batch[0]}")
    print(xs.iterate().batches(7).count())
    print(xs.iterate().batches(8).count())
    print(xs.iterate().batches(0).count())
    let words be ["a", "b", "c"]
    for pair in words.iterate().map(each.clone()).batches(2):
        print(f"{pair.length()} {pair[0]}")
    let none be Array[Int].new()
    print(none.iterate().batches(2).count())
    print(xs.iterate().keep(each > 2).batches(2).take(2).count())
    print(xs.iterate().batches(3).map(each.length()).sum())
    let backwards be xs.iterate().sorted(by: x giving 0 - x).batches(3).collect()
    print(f"{backwards[0][0]} {backwards[2][0]}")
"#,
        ),
        "3 3 3 1\n1 4 7\n1\n3\n5\n7\n1\n1\n7\n2 a\n1 c\n0\n2\n7\n7 1\n"
    );
}

// --- tally ------------------------------------------------------------------

/// **`tally(by: key)` counts items per key**, as a `Map[K, Int]` in order of
/// first appearance: 1..7 by `% 3` is `1 -> 3, 2 -> 2, 0 -> 2`. String keys
/// are owned (`s.clone()`), an empty source is an empty map, and links before
/// it filter what is counted.
#[test]
fn tally_counts_items_per_key() {
    assert_eq!(
        prints(
            "tally",
            r#"def main():
    let xs be [1, 2, 3, 4, 5, 6, 7]
    let t be xs.iterate().tally(by: x giving x % 3)
    print(f"{t.length()}")
    for entry in t:
        print(f"{entry.key} {entry.value}")
    let words be ["a", "b", "a", "a"]
    let w be words.iterate().tally(by: s giving s.clone())
    for entry in w:
        print(f"{entry.key} {entry.value}")
    let none be Array[Int].new()
    print(none.iterate().tally(by: x giving x + 0).length())
    let big be xs.iterate().keep(each > 2).tally(by: x giving x % 2)
    for entry in big:
        print(f"{entry.key} {entry.value}")
"#,
        ),
        "3\n1 3\n2 2\n0 2\na 3\nb 1\n0\n1 3\n0 2\n"
    );
}

// --- owned ------------------------------------------------------------------

/// **`owned()` turns a chain of borrows into a chain of copies**: bitwise for
/// numbers, `clone()` for a `String`, an `Array` and a user's own `Clone`
/// record. The copies are the chain's, so a `map` after it may consume them.
#[test]
fn owned_copies_each_borrowed_item() {
    assert_eq!(
        prints(
            "owned",
            r#"type Point:
    x: Int
    y: Int

Point implements Clone:
    def clone(self) -> Point:
        Point(x: self.x, y: self.y)

def main():
    let xs be [3, 1, 2]
    let a be xs.iterate().owned().collect()
    print(f"{a.length()} {a[0]}")
    let words be ["x", "yy"]
    let w be words.iterate().owned().map(s giving f"{s}!").collect()
    print(f"{w[0]} {w[1]}")
    let ow: Array[String] be words.iterate().owned().collect()
    print(f"{ow[1]}")
    let nested: Array[Array[Int]] be [[1], [2, 3]]
    let copies be nested.iterate().owned().collect()
    print(f"{copies[1].length()}")
    let pts be [Point(x: 1, y: 2)]
    let ps be pts.iterate().owned().collect()
    print(f"{ps[0].y}")
    print(xs.iterate().owned().sum())
    print(xs.iterate().keep(each > 1).owned().count())
"#,
        ),
        "3 3\nx! yy!\nyy\n2\n2\n6\n2\n"
    );
}

// --- followed_by ------------------------------------------------------------

/// **`followed_by(other)` yields this chain's items, then borrows of `other`'s
/// elements**: train then validation. It is a barrier like `reverse`, so the
/// links before it run first, the links after it see both halves in order, and
/// two of them chain.
#[test]
fn followed_by_appends_the_other_array() {
    assert_eq!(
        prints(
            "followed-by",
            r#"def main():
    let train be [1, 2, 3]
    let valid be [10, 20]
    let all be train.iterate().followed_by(valid.iterate()).collect()
    print(f"{all.length()} {all[2]} {all[3]}")
    print(train.iterate().followed_by(valid.iterate()).sum())
    print(train.iterate().keep(each > 1).followed_by(valid.iterate()).take(3).count())
    for x in train.iterate().followed_by(valid.iterate()).reverse().take(2):
        print(f"{x}")
    let words be ["a"]
    let more be ["b", "c"]
    let w be words.iterate().followed_by(more.iterate()).followed_by(words.iterate()).collect()
    print(f"{w.length()} {w[2]} {w[3]}")
    let none be Array[Int].new()
    print(none.iterate().followed_by(valid.iterate()).count())
"#,
        ),
        "5 3 10\n36\n3\n20\n10\n4 c a\n2\n"
    );
}

// --- flatten ----------------------------------------------------------------

/// **`flatten()` yields the elements of each array item in turn**: borrowed
/// when the items are borrows of arrays (`arrays.iterate()`), owned when they
/// are the arrays themselves (a `map` that builds one). Empty arrays
/// contribute nothing, and the links after it see the elements — a `take`, a
/// `sum`, a `batches`, a `reverse`, a `for`.
#[test]
fn flatten_yields_the_elements_of_each_array_in_turn() {
    assert_eq!(
        prints(
            "flatten",
            r#"def main():
    let groups: Array[Array[Int]] be [[1, 2], [3], [], [4, 5, 6]]
    let flat be groups.iterate().flatten().collect()
    print(f"{flat.length()}")
    for x in flat:
        print(f"{x}")
    print(groups.iterate().flatten().sum())
    print(groups.iterate().flatten().take(3).count())
    let words be ["ab", "c"]
    let parts be words.iterate().map(w giving [f"{w}1", f"{w}2"]).flatten().collect()
    print(f"{parts.length()} {parts[0]} {parts[3]}")
    print(groups.iterate().flatten().batches(2).count())
    let backwards be groups.iterate().flatten().reverse().collect()
    print(f"{backwards[0]} {backwards[5]}")
    let nested: Array[Array[Int]] be [[1], [2, 3]]
    for pair in nested.iterate().map(each.clone()).flatten().batches(2):
        print(f"{pair.length()}")
"#,
        ),
        "6\n1\n2\n3\n4\n5\n6\n21\n3\n4 ab1 c2\n3\n6 1\n2\n1\n"
    );
}

/// **Flattened owned items are released exactly once**, whether they reached
/// the end of the chain or a `take` stopped it short.
#[test]
fn flatten_of_owned_items_release_every_one() {
    assert_eq!(
        prints(
            "flatten-owned",
            r#"type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f"dropped {self.tag}")

def pair(n: Int) -> Array[Tracer]:
    [Tracer(tag: n), Tracer(tag: n + 10)]

def main():
    let xs be [1, 2]
    let n be xs.iterate().map(x giving pair(x)).flatten().count()
    print(f"count {n}")
    let t be xs.iterate().map(x giving pair(x)).flatten().take(1).count()
    print(f"take {t}")
"#,
        ),
        "dropped 1\ndropped 11\ndropped 2\ndropped 12\ncount 4\n\
         dropped 1\ndropped 11\ndropped 12\ndropped 2\ntake 1\n"
    );
}

/// **A batch owns the items it holds, and the ones a `take` never reached are
/// released with the buffer**: each `Tracer` is dropped exactly once, whether
/// it was handed on in a batch or left behind.
#[test]
fn batches_of_owned_items_release_every_one() {
    assert_eq!(
        prints(
            "batches-owned",
            r#"type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f"dropped {self.tag}")

def make(n: Int) -> Tracer:
    Tracer(tag: n)

def main():
    let xs be [1, 2, 3]
    let n be xs.iterate().map(x giving make(x)).batches(2).count()
    print(f"count {n}")
    let first be xs.iterate().map(x giving make(x)).batches(2).take(1).count()
    print(f"first {first}")
"#,
        ),
        "dropped 1\ndropped 2\ndropped 3\ncount 2\ndropped 1\ndropped 2\ndropped 3\nfirst 1\n"
    );
}

// --- group ------------------------------------------------------------------

/// **`group(by: key)` collects the items per key**, as a `Map[K, Array[Item]]`
/// in order of first appearance and source order within a group: 1..7 by
/// `% 3` is `1 -> [1, 4, 7]`, `2 -> [2, 5]`, `0 -> [3, 6]`. Owned `String`
/// keys that repeat are dropped on the present path, an empty source is an
/// empty map, and links before it filter what is grouped.
#[test]
fn group_collects_items_per_key() {
    assert_eq!(
        prints(
            "group",
            r#"def main():
    let xs be [1, 2, 3, 4, 5, 6, 7]
    let g be xs.iterate().group(by: x giving x % 3)
    print(f"{g.length()}")
    for entry in g:
        print(f"{entry.key} {entry.value.length()}")
        for v in entry.value:
            print(v)
    let words be ["apple", "avocado", "banana", "blueberry", "apricot"]
    let w be words.iterate().owned().group(by: s giving f"{s.length() % 2}")
    for entry in w:
        print(f"{entry.key}: {entry.value.length()} {entry.value[0]}")
    let none be Array[Int].new()
    print(none.iterate().group(by: x giving x + 0).length())
    let big be xs.iterate().keep(each > 4).group(by: x giving x % 2)
    for entry in big:
        print(f"{entry.key} {entry.value.length()}")
"#,
        ),
        "3\n1 3\n1\n4\n7\n2 2\n2\n5\n0 2\n3\n6\n1: 4 apple\n0: 1 banana\n0\n1 2\n0 1\n"
    );
}

// --- partition --------------------------------------------------------------

/// **`partition(p)` splits one pass into a `Parts`**: `kept` where the
/// predicate held and `discarded` where it did not, each in source order.
/// Owned items move into whichever side takes them, an empty source gives two
/// empty arrays, and links before it filter what is split.
#[test]
fn partition_splits_items_into_kept_and_discarded() {
    assert_eq!(
        prints(
            "partition",
            r#"def main():
    let xs be [1, 2, 3, 4, 5, 6, 7]
    let parts be xs.iterate().partition(x giving x % 2 is 0)
    print(f"{parts.kept.length()} {parts.discarded.length()}")
    for v in parts.kept:
        print(v)
    for v in parts.discarded:
        print(v)
    let words be ["apple", "avocado", "banana"]
    let p2 be words.iterate().owned().partition(s giving s.length() > 5)
    print(f"{p2.kept[0]} {p2.discarded[0]}")
    let none be Array[Int].new()
    let p3 be none.iterate().partition(x giving x > 0)
    print(p3.kept.length() + p3.discarded.length())
    let p4 be xs.iterate().keep(each > 3).partition(each > 5)
    print(f"{p4.kept.length()} {p4.discarded.length()}")
"#,
        ),
        "3 4\n2\n4\n6\n1\n3\n5\n7\navocado apple\n0\n2 2\n"
    );
}

// --- keys after another link ------------------------------------------------

/// **A key closure after another link sees `&&Int`** (the item is already a
/// borrow, and the closure takes a borrow of the item), and returning it as
/// the `Int` key is a copy out through both borrows: `sorted(by: each)`,
/// `minimum` and `maximum` all run after a `keep`.
#[test]
fn a_key_returned_through_two_borrows_is_copied_out() {
    assert_eq!(
        prints(
            "key-two-borrows",
            r#"def main():
    let xs be [3, 1, 2, 5, 4]
    let sorted be xs.iterate().keep(each > 1).sorted(by: each).collect()
    for v in sorted:
        print(v)
    let lowest be xs.iterate().keep(each > 1).minimum(by: v giving v)
    let highest be xs.iterate().discard(each > 4).maximum(by: v giving v)
    if lowest? and highest?:
        print(f"{lowest} {highest}")
"#,
        ),
        "2\n3\n4\n5\n2 4\n"
    );
}

/// **`maximum` and `minimum` over a map's keys and values run** (they were
/// refused by the borrow checker, which read the next turn's fetch of the
/// entry as a write under the borrow the extreme kept).
#[test]
fn extremes_over_a_maps_values_and_keys_run() {
    assert_eq!(
        prints(
            "map-extremes",
            r#"def main():
    let mutable m be Map[String, Int].new()
    m.insert("a", 3)
    m.insert("bbb", 7)
    m.insert("cc", 5)
    let hi be m.values().maximum(by: v giving v)
    let lo be m.values().minimum(by: v giving v)
    let longest be m.keys().maximum(by: k giving k.length())
    if hi? and lo? and longest?:
        print(f"{hi} {lo} {longest}")
"#,
        ),
        "7 3 bbb\n"
    );
}
