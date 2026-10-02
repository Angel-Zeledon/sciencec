//! `Map of (K, V)`, **built, linked, run**, with stdout and exit status
//! asserted.
//!
//! # What was actually missing
//!
//! Nothing below this crate. `science-rt`'s `map.rs` is a complete open-
//! addressing table with tombstones and rehashing and fourteen tests;
//! `RUNTIME` declares all seven entry points with the right
//! `descriptor_index`; `RtAggregate::Map` and `RtAggregate::MapInfo` are laid
//! out and checked against the real Rust types; the prelude declares the
//! surface with its borrow decisions argued in place; and `science-mir` emits
//! a `Map` method as the ordinary prelude-method call it is. The **stub**
//! backend even implemented `define_map_info` in full.
//!
//! What had no caller was `DescriptorTable::intern_map` — nowhere in the
//! workspace, not even in a test — and the LLVM emitter's `define_map_info`
//! was a refusal. That refusal was right about the hazard and wrong about the
//! remedy: it said emitting a descriptor *"whose `hash_fn` and `eq_fn` no
//! function in the module defines would produce a module that verifies and
//! crashes"*, and the answer to that is to **check**, which is what
//! `define_type_info` already does for its `drop_fn`.
//!
//! # Why an empty map is a real test here
//!
//! It looks like it proves nothing and it proves the part that could not be
//! proved any other way. `contains` calls the key's `hash_fn` through the
//! descriptor on the very first probe, so a `ScienceMapInfo` whose function
//! pointers were null, or whose two nested `ScienceTypeInfo`s were laid out at
//! the wrong offsets, is a jump to address zero or a read of a bogus size —
//! not a wrong answer, a crash. Running to exit 0 is the assertion.
//!
//! # What is not here
//!
//! `insert` and `remove`. They return `V?` through §5.3's bool-plus-out-
//! parameter convention, and building a `T?` out of a returned `bool` needs a
//! tag written from a *value*; `ExtInst::StoreTag` takes a constant, and a
//! place projection cannot make the basic blocks a branch would need. Three
//! runtime entry points wait on that one convention — `science_map_insert`,
//! `science_map_remove` and `science_array_pop` — so it is worth building
//! once, properly, rather than smuggling in here.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("maps", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// A `String`-keyed map: built, probed through its descriptor, and released.
#[test]
fn a_string_keyed_map_is_built_probed_and_released() {
    assert_eq!(
        prints(
            "strings",
            "let m be Map[String, Int].new()\n\
             let probe be \"uno\"\n\
             let found be m.get(probe)\n\
             print(f\"{m.length()} {m.contains(probe)} {found?}\")\n",
        ),
        "0 false false\n"
    );
}

/// An `Int`-keyed map, which needed a `hash_fn`/`eq_fn` pair that did not exist
/// in any crate until now.
///
/// `String` had one all along — `science_string_hash` and `science_string_eq`
/// were declared in `RUNTIME` and documented as *"the natural `hash_fn`/`eq_fn`
/// for a `Map` keyed by `String`"* — and the integer pair was simply absent.
/// `science-rt`'s own map tests never noticed, because they declare a
/// `hash_u64` of their own: the test suite proved the table worked while the
/// compiler had no way to name a hash function.
#[test]
fn an_integer_keyed_map_has_a_hash_and_an_equality() {
    assert_eq!(
        prints("ints", "let m be Map[Int, Int].new()\nprint(f\"{m.length()} {m.contains(7)}\")\n"),
        "0 false\n"
    );
}

/// The whole round trip: insert, overwrite, read, remove.
///
/// # What this needed that nothing else did
///
/// `Map.insert(mutable self, key: K, value: V) -> V?` is §5.3's
/// **bool-plus-out-parameter** convention: `science_map_insert(P, D, P, P, P)
/// -> Bool` says *whether* a value came back in its return and *what* it was
/// through a trailing pointer, and the two together are the `V?`. Three things
/// had to exist for that:
///
/// - **An out-slot the call site invents.** Nothing in MIR names it; Science
///   passes two arguments where the entry point takes five.
/// - **A tag written from a value.** `ExtInst::StoreTag` takes a *constant*
///   discriminant, so a `T?` built out of a returned `bool` was not expressible
///   at all, and a place projection cannot make the basic blocks a branch would
///   need.
/// - **A narrowed read of a tagged option.** `Rvalue::Narrow` had no lowering:
///   `(&T)?` is Decision 19's niche and needs none, but `I64?` is Decision 18's
///   discriminant-and-payload and the payload has to be loaded at its offset.
///   `let x: Int? be 5` followed by `if x?: print(f"{x}")` did not build
///   either, which says this was never about maps.
///
/// # Why every number is asserted
///
/// The two first inserts must report **no** previous value, the overwrite must
/// report `1` and not `11`, the read must see the overwrite, and the remove
/// must hand back what was there and shrink the table. A convention that
/// returned the *new* value, or that reported presence inverted, passes a test
/// that only checks the length — and both are one character's worth of mistake.
#[test]
fn a_map_insert_overwrite_read_and_remove_round_trip() {
    assert_eq!(
        prints(
            "round-trip",
            "let mutable m be Map[String, Int].new()\n\
             let first be m.insert(\"uno\", 1)\n\
             let second be m.insert(\"dos\", 2)\n\
             print(f\"n={m.length()} first={first?} second={second?}\")\n\
             let old be m.insert(\"uno\", 11)\n\
             if old?:\n\
             \x20   print(f\"overwrote, previous={old}\")\n\
             let k be \"uno\"\n\
             let got be m.get(k)\n\
             if got?:\n\
             \x20   print(f\"read={got} n={m.length()}\")\n\
             let gone be m.remove(k)\n\
             if gone?:\n\
             \x20   print(f\"removed={gone} n={m.length()}\")\n",
        ),
        "n=2 first=false second=false\noverwrote, previous=1\nread=11 n=2\nremoved=11 n=1\n"
    );
}

/// A narrowed read of a **tagged** option, with no map anywhere near it.
///
/// Kept separate because the defect was separate: `Rvalue::Narrow` had no
/// lowering for Decision 18's layout, and the smallest program that shows it
/// is three lines. A test that only exercised it through `Map.insert` would
/// have left a reader thinking the two were the same feature.
#[test]
fn a_tagged_option_narrows_to_its_payload() {
    assert_eq!(prints("narrow", "let x: Int? be 5\nif x?:\n    print(f\"{x}\")\n"), "5\n");
}

/// A `for` over what `Map.get` found, inside `if found?:` — the row of an
/// adjacency map, which is how `stdlib-core.md` §3.7's traversal reads a
/// graph. Both were `SC0400` from the backend: the subject's place is still
/// the `(&T)?` the narrow sees through, and `science-mir`'s `for` took it as
/// the container without the `Deref` a method receiver in the same position
/// gets (`Builder::loop_source`). An `Array` row fell through to *"no
/// `Iterate` implementation"*; a `Map` row reached `science_map_entry_at` with
/// the nullable as its map. The absent key is the third case: no loop at all.
#[test]
fn a_for_walks_a_container_narrowed_out_of_map_get() {
    assert_eq!(
        prints(
            "for_over_narrowed_get",
            "def main():
    let mutable rows be Map[String, Array[String]].new()
    let mutable row be Array[String].new()
    row.push(\"x\")
    row.push(\"yz\")
    rows.insert(\"first\", row)
    let found be rows.get(&\"first\")
    if found?:
        for item in found:
            print(item)
    let missing be rows.get(&\"second\")
    if missing?:
        for item in missing:
            print(item)
    let mutable nested be Map[Int, Map[Int, Int]].new()
    let mutable inner be Map[Int, Int].new()
    inner.insert(3, 30)
    inner.insert(4, 40)
    nested.insert(1, inner)
    let table be nested.get(&1)
    if table?:
        for entry in table:
            print(entry.key + entry.value)
",
        ),
        "x\nyz\n33\n44\n"
    );
}

/// A key with no hash is refused, **by the front end**, and the refusal says
/// why rather than defaulting.
///
/// **The tempting default is wrong exactly where it would be reached.** Hashing
/// the key's bytes and comparing the key's bytes looks universal; a record with
/// padding has bytes that take no part in equality, so two values that *are*
/// equal can hash differently, and the table then loses entries as a function
/// of what the allocator last left in the padding. That is a bug which
/// reproduces on one machine and not the next. A diagnostic costs a sentence.
///
/// # What changed
///
/// This used to be `Map[Doc, Int]` with `Doc` one `Int`, refused by the
/// backend as `SC0400` after `sciencec check` had called the program clean. A
/// record of hashable fields is a key now —
/// `a_record_key_with_padding_is_found_through_any_path` below — so the
/// refusal moves to the keys that really have no hash, and to the phase that
/// can say so before a build: `SC0548`, from `science-types`' `keys` pass,
/// naming the key, the field that makes it unhashable, and why.
#[test]
fn a_key_with_no_hash_is_refused_by_name() {
    let errors = harness::check_errors(
        "type Doc:\n    id: Int\n    score: F64\n\n\
         type Name:\n    text: String\n\n\
         Name implements Eq:\n    def eq(self, other: &Name) -> Bool:\n        \
         self.text.length() is other.text.length()\n\n\
         def main():\n    let m be Map[Doc, Int].new()\n    let s be Set[Name].new()\n    \
         let a be Set[Array[Int]].new()\n    print(f\"{m.length()} {s.length()} {a.length()}\")\n",
    );
    assert_eq!(
        errors.iter().map(|(code, _)| *code).collect::<Vec<_>>(),
        [548, 548, 548],
        "one `SC0548` per unhashable key and nothing else: {errors:?}"
    );
    let all: String = errors.iter().map(|(_, message)| format!("{message}\n")).collect();
    for named in [
        "`Doc` cannot be a key of a `Map`",
        "`Doc`'s field `score` is `F64`: `F64` is a float",
        "`Name` cannot be an element of a `Set`",
        "`Name` implements `Eq` by hand",
        "`Array[I64]` cannot be an element of a `Set`",
    ] {
        assert!(all.contains(named), "the refusal must name `{named}`, and it said:\n{all}");
    }
}

/// A `Map` whose **value owns memory**, which was the whole feature being
/// unusable.
///
/// # What was wrong
///
/// `Map[String, String].new()` followed by one `insert` was refused with
/// *"drop glue for `String?`, whose concrete type this crate cannot name"* —
/// on an empty map, with no previous value to free, and even when the result
/// was discarded. §5.3's convention materialises a `V?` for **every**
/// `insert` and `remove`, so the gap swallowed the whole method pair whenever
/// `V` owned anything.
///
/// The refusal it met said releasing a `choice`'s payload *"is one block per
/// arm where this builds one block"*. `ExtBody::blocks` is a `Vec` and
/// `Terminator::Switch` has been there since the CFG was: that sentence
/// described how the glue was written, not what the emitter can do.
///
/// **The round-trip test that already existed did not catch it**, because it
/// used `Map[String, Int]` and an `Int?` owns nothing. That is the same blind
/// spot the array tests had — every one of them read an element through
/// `print`, the one position that took a different path — and it is worth
/// naming twice.
///
/// # What was still refused here, and is not any more
///
/// Narrowing the returned option and *reading* it — `if old?: print(f"{old}")`
/// — used to be refused, because reading an owning payload out of a tagged
/// `T?` by value copies an owner while the option is still the one that
/// releases it: the value would be freed twice, and the first version of this
/// change segfaulted on exactly that. That copy is still refused, and still
/// should be — `f"{old}"` renders through `value_hole`, which reads a value
/// and has no way to avoid the copy.
///
/// A **borrow** of the same narrowed payload never had that problem — a
/// borrow makes no second owner — and is what `a_narrowed_map_value_that_owns_
/// memory_is_borrowed_not_copied` below exercises: `print(old)`, not
/// `print(f"{old}")`. `science-mir`'s `borrow_source` projects
/// `Projection::Payload` onto the option and `science-codegen`'s
/// `place_address` reads the tagged pair at `payload_offset`, the way
/// `Projection::Downcast` already read a `choice`'s.
#[test]
fn a_map_whose_value_owns_memory_can_be_inserted_into() {
    assert_eq!(
        prints(
            "owning-values",
            "let mutable m be Map[String, String].new()\n\
             let first be m.insert(\"k\", \"uno\")\n\
             let second be m.insert(\"k\", \"dos\")\n\
             let probe be \"k\"\n\
             let gone be m.remove(probe)\n\
             print(f\"{m.length()} {first?} {second?} {gone?}\")\n",
        ),
        "0 false true true\n"
    );
}

/// The headline case this file's own history refused: a narrowed owning
/// `V?` is **borrowed**, not copied, and `print` reads it in its own storage
/// rather than at the option's.
///
/// The first `insert` has nothing to evict, so `old?` is false and nothing
/// prints for it. The second evicts `"uno"`; the `remove` at the end evicts
/// `"dos"`. Both are printed by narrowing and passing the narrowed value to
/// `print` directly — `AGENTS.md` §4's automatic call-site borrow — and not
/// through `f"{}"`, which is `value_hole`'s path and still copies.
#[test]
fn a_narrowed_map_value_that_owns_memory_is_borrowed_not_copied() {
    assert_eq!(
        prints(
            "narrowed-owning-value",
            "let mutable m be Map[String, String].new()\n\
             let first be m.insert(\"k\", \"uno\")\n\
             if first?:\n\
             \x20   print(first)\n\
             let second be m.insert(\"k\", \"dos\")\n\
             if second?:\n\
             \x20   print(second)\n\
             let gone be m.remove(\"k\")\n\
             if gone?:\n\
             \x20   print(gone)\n",
        ),
        "uno\ndos\n"
    );
}

/// A hundred thousand rounds of insert-narrow-borrow-release, for the same
/// reason `tests/methods.rs`'s
/// `a_boxed_value_owning_a_string_is_built_and_freed_ten_thousand_times` runs
/// ten thousand: a double free corrupts the allocator's own bookkeeping and
/// the process aborts once enough of the heap has been walked over, which is
/// not necessarily the round that caused it. Every round after the first
/// evicts the previous value, borrows it long enough to read its length, and
/// then lets it drop — one allocation and one release per round, and a flat
/// heap is the claim this test cannot make by itself; a manual run of the
/// same program under macOS's `leaks --atExit` during development read *"0
/// leaks for 0 total leaked bytes"*.
#[test]
fn a_narrowed_owning_map_value_is_borrowed_a_hundred_thousand_times() {
    let source = "\
def main():
    let mutable m be Map[String, String].new()
    let mutable total be 0
    for i in 0..100000:
        let old be m.insert(\"k\", \"value\")
        if old?:
            total be total + old.length()
    print(f\"{total}\")
";
    let dir = scratch("maps", "narrowed_owning_loop");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "narrowed_owning_loop"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.stdout, "499995\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
}

// --- Iteration, in insertion order -------------------------------------------
//
// `collections-and-chains.md` §5.2's AMENDMENT 9: `Map` and `Set` iterate in
// the order their keys were first inserted. `for entry in counts:` binds a
// `&Entry[K, V]` (§5.4, §1.3) and `for x in set:` a `&T`; `science-rt`'s
// `map.rs` stores each pair as that record, and `science-mir`'s
// `lower_for_over_map` walks it. Every expected output below is written in
// insertion order and would be in hash order on the table this replaced —
// which is the whole claim, since hash order would differ between hashers and
// insertion order cannot.

/// Twenty keys, inserted in an order that is neither sorted nor anything a
/// hash would produce, come back in that order after the table has been
/// rebuilt from eight slots to thirty-two on the way.
#[test]
fn a_map_iterates_in_insertion_order_across_a_rehash() {
    let source = "\
def main():
    let mutable m be Map[Int, Int].new()
    for i in 0..20:
        let key be (i * 7 + 3) % 20
        m.insert(key, i)
    let mutable line be \"\"
    for entry in m:
        line be f\"{line}{entry.key}:{entry.value} \"
    print(line)
";
    let expected: String =
        (0..20).map(|i| format!("{}:{} ", (i * 7 + 3) % 20, i)).collect::<String>() + "\n";
    assert_eq!(prints("order-rehash", source), expected);
}

/// A removed key vanishes from the walk without moving anything else; the
/// same key inserted again is a new key and goes last; overwriting a key that
/// is present keeps its place (`stdlib-core.md` §3.6: *"re-inserting an
/// existing key does not move it"*). `continue` and `break` reach the loop's
/// increment and exit, which have their own blocks here because a removed
/// entry's hole skips the body.
#[test]
fn removal_reinsertion_and_overwrite_each_do_what_the_order_rules_say() {
    let source = "\
def main():
    let mutable m be Map[Int, Int].new()
    for key in [5, 1, 4, 2, 3]:
        m.insert(key, key * 10)
    let gone be m.remove(4)
    m.insert(4, 44)
    m.insert(1, 11)
    for entry in m:
        if entry.key is 2:
            continue
        print(f\"{entry.key}={entry.value}\")
    for entry in m:
        if entry.key is 3:
            break
        print(entry.key)
";
    assert_eq!(
        prints("order-remove", source),
        "5=50\n1=11\n3=30\n4=44\n5\n1\n2\n"
    );
}

/// `String` keys and values — owned, dropped by the map, borrowed by the loop
/// and read through the borrow — over a map reached through a `&` parameter
/// and through a record field, and a nested loop over the same map, which two
/// shared loans allow.
#[test]
fn a_string_keyed_map_is_walked_through_a_borrow_and_a_field() {
    let source = "\
type Index:
    words: Map[String, String]

def show(words: &Map[String, String]):
    for entry in words:
        print(f\"{entry.key} -> {entry.value} ({entry.key.length()})\")

def main():
    let mutable words be Map[String, String].new()
    words.insert(\"uno\", \"one\")
    words.insert(\"dos\", \"two\")
    words.insert(\"tres\", \"three\")
    words.insert(\"dos\", \"TWO\")
    show(words)
    let index be Index(words: words)
    for a in index.words:
        for b in index.words:
            if a.key is not b.key:
                write(f\"{a.key}/{b.key} \")
    print(\"\")
";
    assert_eq!(
        prints("strings_leak", source),
        "uno -> one (3)\ndos -> TWO (3)\ntres -> three (4)\n\
         uno/dos uno/tres dos/uno dos/tres tres/uno tres/dos \n"
    );
}

/// A value narrower than its key, and a record value, so the value is read at
/// the offset `science-rt`'s `entry_layout` computed and not merely at the
/// key's size: `Entry[Int, Bool]` is sixteen bytes with seven of padding, and
/// `Entry[Int, Pair]` holds a record whose own fields are read through the
/// entry.
#[test]
fn the_entry_layout_agrees_with_the_compilers_for_padded_values() {
    let source = "\
type Pair:
    weight: F64
    flag: Bool

def main():
    let mutable flags be Map[Int, Bool].new()
    for i in 0..10:
        flags.insert(9 - i, i % 3 is 0)
    for e in flags:
        write(f\"{e.key}{e.value} \")
    print(\"\")
    let mutable pairs be Map[Int, Pair].new()
    pairs.insert(2, Pair(weight: 1.5, flag: true))
    pairs.insert(1, Pair(weight: 2.5, flag: false))
    for e in pairs:
        print(f\"{e.key} {e.value.weight} {e.value.flag}\")
";
    assert_eq!(
        prints("layout", source),
        "9true 8false 7false 6true 5false 4false 3true 2false 1false 0true \n\
         2 1.5 true\n1 2.5 false\n"
    );
}

/// `for x in set:` yields the elements, in insertion order, with a duplicate
/// insert ignored and a removed element gone.
///
/// `first` is annotated `Int`, and the annotation is load-bearing for a reason
/// that is not this file's: an unannotated `let mutable first be 0` later
/// assigned a loop item infers `first` at `&Int`, and `first is 0` then
/// reaches the backend as `==` over a borrow (`SC0400`). A `for` over an
/// `Array` does exactly the same; it is `science-types`' inference, measured
/// here and not changed.
#[test]
fn a_set_iterates_its_elements_in_insertion_order() {
    let source = "\
def main():
    let mutable seen be Set[String].new()
    for word in [\"pear\", \"fig\", \"apple\", \"fig\", \"kiwi\"]:
        seen.insert(word.clone())
    seen.remove(\"apple\")
    for word in seen:
        print(word)
    let mutable ids be Set[Int].new()
    for i in 0..12:
        ids.insert(100 - i * 3)
    let mutable total be 0
    let mutable first: Int be 0
    for id in ids:
        if first is 0:
            first be id
        total be total + id
    print(f\"{first} {total}\")
";
    assert_eq!(prints("set", source), "pear\nfig\nkiwi\n100 1002\n");
}

/// **No leak, with a constant live set.** Three thousand rounds of: build a
/// map of twenty owned keys and values, remove one and reinsert it, overwrite
/// another, copy the keys into a `Set` through the loop, walk the set, and
/// drop everything. Each round's live set is the same size, so a leak shows
/// as a growing heap rather than hiding among values that are legitimately
/// kept — `NEXT-SESSION.md`'s rule, after a report counted three hundred
/// thousand retained keys as a leak.
///
/// The printed total checks that every round did the work; `leaks --atExit`
/// checks the heap. The second half runs only where macOS's `leaks` exists.
#[test]
fn iterating_maps_and_sets_of_owned_strings_leaks_nothing() {
    let source = "\
def main():
    let mutable last be 0
    for round in 0..3000:
        let mutable m be Map[String, String].new()
        for i in 0..20:
            m.insert(f\"k{i}\", f\"v{i}\")
        let gone be m.remove(\"k3\")
        m.insert(\"k3\", \"again\")
        m.insert(\"k5\", \"over\")
        let mutable seen be Set[String].new()
        for e in m:
            seen.insert(e.key.clone())
        seen.remove(\"k7\")
        let mutable count be 0
        for s in seen:
            count be count + s.length()
        let mutable lengths be 0
        for e in m:
            lengths be lengths + e.key.length() + e.value.length()
        last be lengths + count
    print(last)
";
    let dir = scratch("maps", "iteration_leaks");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "iteration_leaks"), OptLevel::O2);
    let ran = run(&built);
    // Keys: ten of two bytes and ten of three, 50. Values: the same 50, less
    // `v3` and `v5`, plus `again` and `over`: 55. The set: the 50 key bytes
    // less `k7`: 48.
    assert_eq!(ran.stdout, "153\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);

    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&built.executable)
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(
            text.contains(" 0 leaks for 0 total leaked bytes"),
            "`leaks --atExit` found something:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Keys built from fields ---------------------------------------------------
//
// A key that is not `String` or an eight-byte integer is hashed and compared by
// glue `science-codegen-llvm`'s `Lowerer::intern_key_glue` emits per key type,
// walking its parts the way drop glue does: an integer, `Bool` or `Char` loaded
// at its own width, a `String` through the runtime's pair, an aggregate through
// its own glue, and a `choice` or `T?` by its discriminant and then the active
// variant's payload. `science-types`' `keys` module is the front half — what a
// key may be, and `SC0548` for what it may not.

/// **A record with padding, found through any path.** `Padded` is a `U8` and
/// an `Int`, so seven bytes between them take no part in its value. The keys
/// are filed from literals on the stack; the probes come back out of an array
/// whose buffer was allocated after an array of `-1`s, and through a function
/// that rebuilds the record field by field — paths along which nothing
/// arranges for the pad bytes to agree with the stack's.
///
/// A byte-wise hash would find these keys or not as a function of what those
/// seven bytes held. The glue never reads them: its hash loads one byte at
/// offset 0 and eight at offset 8, and its equality compares the same two.
#[test]
fn a_record_key_with_padding_is_found_through_any_path() {
    let source = "\
type Padded:
    tag: U8
    id: Int

def rebuilt(values: &Array[Padded], at: Int) -> Padded:
    let found be values.get(at)
    if found?:
        return Padded(tag: found.tag, id: found.id)
    Padded(tag: 0, id: 0)

def main():
    let mutable filler be Array[Int].new()
    for i in 0..64:
        filler.push(-1)
    let mutable m be Map[Padded, String].new()
    m.insert(Padded(tag: 7, id: 1000), \"seven\")
    m.insert(Padded(tag: 8, id: 1000), \"eight\")
    m.insert(Padded(tag: 7, id: 1001), \"other\")
    let mutable heap be Array[Padded].new()
    for i in 0..32:
        heap.push(Padded(tag: (i % 3 + 6) as U8, id: 1000))
    let mutable hits be 0
    for i in 0..32:
        if m.contains(rebuilt(heap, i)):
            hits be hits + 1
    for p in heap:
        if m.contains(p):
            hits be hits + 1
    print(hits)
    print(m.contains(Padded(tag: 9, id: 1000)))
    print(m.insert(Padded(tag: 8, id: 1000), \"EIGHT\")?)
    for e in m:
        print(f\"{e.key.tag} {e.key.id} {e.value}\")
";
    // Tags cycle 6, 7, 8: of 32, eleven are 6 (absent), eleven 7 and ten 8.
    // Each path finds the twenty-one present: 42.
    assert_eq!(
        prints("padded-record", source),
        "42\nfalse\ntrue\n7 1000 seven\n8 1000 EIGHT\n7 1001 other\n"
    );
}

/// Tuples as keys: order matters, `(1, 2)` is not `(2, 1)`, and a tuple of
/// mixed widths and an owned `String` is one key.
#[test]
fn a_tuple_key_hashes_its_elements_in_order() {
    let source = "\
def main():
    let mutable grid be Map[(Int, Int), String].new()
    for x in 0..4:
        for y in 0..4:
            grid.insert((x, y), f\"{x}{y}\")
    print(grid.length())
    print(grid.contains((1, 2)))
    print(grid.contains((2, 1)))
    print(grid.contains((4, 0)))
    let mutable mixed be Map[(U8, String, Bool), Int].new()
    mixed.insert((1, \"a\", true), 1)
    mixed.insert((1, \"a\", false), 2)
    mixed.insert((2, \"a\", true), 3)
    mixed.insert((1, \"b\", true), 4)
    mixed.insert((1, \"a\", true), 5)
    print(mixed.length())
    for e in mixed:
        print(e.value)
";
    assert_eq!(
        prints("tuple-keys", source),
        "16\ntrue\ntrue\nfalse\n4\n5\n2\n3\n4\n"
    );
}

/// `choice` keys: a discriminant first, then the active variant's payload,
/// including a `String` and a payload of two fields. Two variants holding the
/// same number are different keys, and a payload-free variant is equal only
/// to itself.
#[test]
fn a_choice_key_compares_its_tag_and_then_its_payload() {
    let source = "\
choice Shape:
    Circle(Int)
    Square(Int)
    Named(String, U8)
    Empty

def main():
    let mutable counts be Map[Shape, Int].new()
    counts.insert(Shape.Circle(3), 1)
    counts.insert(Shape.Square(3), 2)
    counts.insert(Shape.Named(\"disc\", 3), 3)
    counts.insert(Shape.Named(\"disc\", 4), 4)
    counts.insert(Shape.Named(\"ring\", 3), 5)
    counts.insert(Shape.Empty, 6)
    counts.insert(Shape.Circle(3), 7)
    counts.insert(Shape.Empty, 8)
    print(counts.length())
    print(counts.contains(Shape.Named(\"disc\", 3)))
    print(counts.contains(Shape.Named(\"disc\", 5)))
    print(counts.contains(Shape.Square(4)))
    for e in counts:
        print(e.value)
";
    assert_eq!(
        prints("choice-keys", source),
        "6\ntrue\nfalse\nfalse\n7\n2\n3\n4\n5\n8\n"
    );
}

/// Keys nested three deep — a record holding a tuple holding a `choice`
/// holding a record — and a `T?` inside a tuple, where `null` and `0` are two
/// keys and `null` is equal to `null`.
#[test]
fn a_nested_key_reaches_every_level() {
    let source = "\
type Cell:
    row: I32
    column: I32

choice Slot:
    At(Cell)
    Nowhere

type Entry:
    sheet: String
    place: (Slot, U16)

def main():
    let mutable m be Map[Entry, Int].new()
    m.insert(Entry(sheet: \"a\", place: (Slot.At(Cell(row: 1, column: 2)), 0)), 1)
    m.insert(Entry(sheet: \"a\", place: (Slot.At(Cell(row: 2, column: 1)), 0)), 2)
    m.insert(Entry(sheet: \"a\", place: (Slot.Nowhere, 0)), 3)
    m.insert(Entry(sheet: \"b\", place: (Slot.At(Cell(row: 1, column: 2)), 0)), 4)
    m.insert(Entry(sheet: \"a\", place: (Slot.At(Cell(row: 1, column: 2)), 1)), 5)
    print(m.length())
    print(m.contains(Entry(sheet: \"a\", place: (Slot.At(Cell(row: 2, column: 1)), 0))))
    print(m.contains(Entry(sheet: \"a\", place: (Slot.At(Cell(row: -2, column: 1)), 0))))
    print(m.contains(Entry(sheet: \"a\", place: (Slot.Nowhere, 0))))
    print(m.contains(Entry(sheet: \"a\", place: (Slot.Nowhere, 1))))
    let mutable maybe be Map[(String, Int?), Int].new()
    maybe.insert((\"x\", null), 1)
    maybe.insert((\"x\", 0), 2)
    maybe.insert((\"x\", null), 3)
    print(maybe.length())
    for e in maybe:
        print(e.value)
";
    assert_eq!(
        prints("nested-keys", source),
        "5\ntrue\nfalse\ntrue\nfalse\n2\n3\n2\n"
    );
}

/// Integers narrower than eight bytes, `Bool` and `Char`, which had no pair:
/// a `ScienceHashFn` is handed a pointer and no size, so the runtime's
/// eight-byte `science_int_hash` would read past a one-byte slot. Their glue
/// loads each at its own width. Negative keys of a signed width are included
/// because a sign extension and a zero extension disagree about them.
#[test]
fn narrow_integers_bools_and_chars_are_keys() {
    let source = "\
def main():
    let mutable bytes be Map[U8, Int].new()
    for i in 0..300:
        bytes.insert((i % 256) as U8, i)
    print(bytes.length())
    print(bytes.contains(255))
    let mutable signed be Set[I32].new()
    for i in -5..5:
        signed.insert(i as I32)
    print(signed.length())
    print(signed.contains(-5))
    print(signed.contains(5))
    let mutable small be Set[I16].new()
    small.insert(-1)
    small.insert(1)
    small.insert(-1)
    print(small.length())
    let mutable flags be Map[Bool, String].new()
    flags.insert(true, \"yes\")
    flags.insert(false, \"no\")
    flags.insert(true, \"again\")
    print(flags.length())
    let mutable letters be Map[Char, Int].new()
    for c in \"hello\".chars():
        letters.insert(c, 1)
    print(letters.length())
    print(letters.contains('l'))
    print(letters.contains('z'))
";
    assert_eq!(
        prints("narrow-keys", source),
        "256\ntrue\n10\ntrue\nfalse\n2\n2\n4\ntrue\nfalse\n"
    );
}

/// A `Set` of records: `insert` answers whether the value was new, a removed
/// value can be inserted again and goes last, and the walk is in insertion
/// order.
#[test]
fn a_set_of_records_removes_and_reinserts() {
    let source = "\
type Point:
    x: Int
    y: Int

def main():
    let mutable seen be Set[Point].new()
    print(seen.insert(Point(x: 1, y: 2)))
    print(seen.insert(Point(x: 2, y: 1)))
    print(seen.insert(Point(x: 1, y: 2)))
    print(seen.insert(Point(x: 0, y: 0)))
    print(seen.remove(Point(x: 1, y: 2)))
    print(seen.remove(Point(x: 1, y: 2)))
    print(seen.contains(Point(x: 1, y: 2)))
    print(seen.insert(Point(x: 1, y: 2)))
    for p in seen:
        print(f\"{p.x},{p.y}\")
";
    assert_eq!(
        prints("set-of-records", source),
        "true\ntrue\nfalse\ntrue\ntrue\nfalse\nfalse\ntrue\n2,1\n0,0\n1,2\n"
    );
}

/// **No leak, with a constant live set, for keys that own memory.** Each of
/// two thousand rounds builds a map keyed by a record holding a `String` and a
/// `choice` holding another, overwrites a key (which releases the displaced
/// *value* and the *probe* key, and keeps the filed one), removes and
/// reinserts one, fills a `Set` of tuples holding `String`s, and drops it all.
/// A record key's release is its drop glue, reached through the key
/// descriptor's `drop_fn` — the hash glue owns nothing and must not change
/// that.
///
/// **The tuple set is named through an alias, `Set[Pair]`, and not written
/// `Set[(String, String)]`**, because in expression position that spelling is
/// an index whose subscript is a tuple, and the checker reads a tuple
/// subscript as the argument *list* — `Set` given two arguments — and leaves
/// the type as `TyKind::Error` with no diagnostic. That is a front-end gap of
/// its own, older than tuple keys; the annotation `let s: Set[(Int, Int)]`,
/// in type position, is read correctly.
#[test]
fn record_keys_that_own_strings_leak_nothing() {
    let source = "\
choice Tag:
    Label(String)
    Plain

type Pair is (String, String)

type Key:
    name: String
    tag: Tag
    n: U8

def main():
    let mutable last be 0
    for round in 0..2000:
        let mutable m be Map[Key, String].new()
        for i in 0..16:
            m.insert(Key(name: f\"k{i}\", tag: Tag.Label(f\"t{i % 3}\"), n: (i % 4) as U8), f\"v{i}\")
        m.insert(Key(name: \"k1\", tag: Tag.Label(\"t1\"), n: 1), \"over\")
        let gone be m.remove(Key(name: \"k2\", tag: Tag.Label(\"t2\"), n: 2))
        m.insert(Key(name: \"k2\", tag: Tag.Plain, n: 2), \"plain\")
        let mutable pairs be Set[Pair].new()
        for e in m:
            pairs.insert((e.key.name.clone(), e.value.clone()))
        pairs.insert((\"k1\", \"over\"))
        last be m.length() * 100 + pairs.length()
    print(last)
";
    let dir = scratch("maps", "key_leaks");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "key_leaks"), OptLevel::O2);
    let ran = run(&built);
    // Sixteen keys, one overwritten in place, one removed and filed again
    // under a different tag: sixteen. Sixteen distinct pairs, the last insert
    // a duplicate.
    assert_eq!(ran.stdout, "1616\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);

    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&built.executable)
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(
            text.contains(" 0 leaks for 0 total leaked bytes"),
            "`leaks --atExit` found something:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
