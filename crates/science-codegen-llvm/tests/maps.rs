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

/// A key with no pair is refused, and the refusal says why rather than
/// defaulting.
///
/// **The tempting default is wrong exactly where it would be reached.** Hashing
/// the key's bytes and comparing the key's bytes looks universal; a record with
/// padding has bytes that take no part in equality, so two values that *are*
/// equal can hash differently, and the table then loses entries as a function
/// of what the allocator last left in the padding. That is a bug which
/// reproduces on one machine and not the next. A diagnostic costs a sentence.
#[test]
fn a_key_with_no_hash_is_refused_by_name() {
    let lowered = lower(
        "type Doc:\n    id: Int\n\nlet m be Map[Doc, Int].new()\nprint(f\"{m.length()}\")\n",
    );
    let dir = scratch("maps", "refused");
    let diagnostics = lowered
        .try_build(&dir.join("out"), OptLevel::O2)
        .map(|_| ())
        .expect_err("a `Map` keyed by a record is not lowered");
    let first = diagnostics.first().expect("a diagnostic");
    assert_eq!(first.code, science_codegen::diagnostics::code::SC0400);
    assert!(
        first.message.contains("`Doc`") && first.message.contains("hash_fn"),
        "the refusal must name the key and what is missing, and it said: {}",
        first.message
    );
    let _ = std::fs::remove_dir_all(&dir);
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
