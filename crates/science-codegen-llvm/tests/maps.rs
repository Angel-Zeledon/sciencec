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
            "let m be (Map of (String, Int)).new()\n\
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
        prints("ints", "let m be (Map of (Int, Int)).new()\nprint(f\"{m.length()} {m.contains(7)}\")\n"),
        "0 false\n"
    );
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
        "type Doc:\n    id: Int\n\nlet m be (Map of (Doc, Int)).new()\nprint(f\"{m.length()}\")\n",
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
