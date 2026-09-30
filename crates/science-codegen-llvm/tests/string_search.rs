//! `String.ends_with`, `contains`, `find` and `replace`, **built, linked,
//! run**, with stdout and exit status asserted.
//!
//! # What was missing
//!
//! The same seam `runtime_reachability.rs` names, from the other side. The
//! prelude declared all four and they type-checked, so a program using them
//! passed `sciencec check` and the backend refused it by name: `science-rt`
//! exported an entry point for none of them, so there was nothing for a
//! `PRELUDE_METHODS` row to point at. `starts_with` had the row *and* the
//! entry point; its four siblings had neither.
//!
//! Three of them are `PRELUDE_METHODS` rows now, and `find` is a row in
//! `owned_nullable_method`'s table, because its `Int?` comes back through
//! §5.3's bool-plus-out-parameter convention.
//!
//! # Why every value is what it is
//!
//! Every test mixes multi-byte characters in, because each method has a
//! plausible wrong implementation that agrees with the right one on ASCII:
//! a `find` counting characters, a `replace` with an empty pattern splitting
//! a character into bytes. And each asserts both answers where there are two,
//! because a row naming the wrong symbol would very likely still return
//! *something* of the declared type — `starts_with_answers_both_ways`'s
//! argument, one file over.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("string_search", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `ends_with` and `contains`, each answering both ways, with the empty
/// needle — which every string ends with and contains — asserted as well.
#[test]
fn ends_with_and_contains_answer_both_ways() {
    assert_eq!(
        prints(
            "ends-contains",
            "let text be \"naïve café\"\n\
             let suffix be \"café\"\n\
             let empty be \"\"\n\
             print(f\"{text.ends_with(suffix)} {text.ends_with(\"naïve\")} {text.ends_with(empty)}\")\n\
             print(f\"{text.contains(\"ïve c\")} {text.contains(\"tea\")} {text.contains(empty)}\")\n",
        ),
        "true false true\ntrue false true\n"
    );
}

/// `find` is a **byte** offset, and the needle here sits after a two-byte
/// `ï`: a character offset would print `6`, not `7`. A miss is `null`, and
/// the empty needle is found at `0`.
///
/// The miss is read as `missing?` rather than printed whole, and the hit is
/// read inside `if at?:`, because what is being measured is the tag this
/// crate writes from the runtime's `Bool` — `StoreTagFromValue` — and both of
/// its values have to come out.
#[test]
fn find_is_a_byte_offset_and_a_miss_is_null() {
    assert_eq!(
        prints(
            "find",
            "let text be \"naïve café, café\"\n\
             let at be text.find(\"café\")\n\
             if at?:\n\
             \x20   print(f\"at {at}\")\n\
             let missing be text.find(\"tea\")\n\
             print(f\"missing {missing?}\")\n\
             let first be text.find(\"\")\n\
             if first?:\n\
             \x20   print(f\"empty at {first}\")\n",
        ),
        "at 7\nmissing false\nempty at 0\n"
    );
}

/// `replace` replaces **every** occurrence, left to right and without
/// overlap, leaves its receiver alone, and — an empty `from` — inserts at
/// every character boundary without splitting the two-byte `é`.
#[test]
fn replace_replaces_every_occurrence_and_leaves_the_receiver_alone() {
    assert_eq!(
        prints(
            "replace",
            "let text be \"café, café\"\n\
             let swapped be text.replace(\"café\", \"té\")\n\
             print(swapped)\n\
             print(text)\n\
             print(\"aaa\".replace(\"aa\", \"b\"))\n\
             print(\"é!\".replace(\"\", \"|\"))\n\
             print(text.replace(\"tea\", \"x\"))\n",
        ),
        "té, té\ncafé, café\nba\n|é|!|\ncafé, café\n"
    );
}

/// The four in a loop, with a `replace` result dropped on every iteration.
///
/// The number is the assertion that the loop ran; that the `String`s
/// `replace` returns are released is measured by `leaks --atExit` over the
/// same program, which is not something a test on every platform can run.
/// Measured on the commit that added this: a live set of 190 allocations
/// (12 KB) after 200 000 `replace` results, and 0 leaks.
#[test]
fn the_four_in_a_loop() {
    assert_eq!(
        prints(
            "loop",
            "let text be \"one, two, three, two, one\"\n\
             let mutable total be 0\n\
             for i in 0..1000:\n\
             \x20   let out be text.replace(\"two\", \"zwei\")\n\
             \x20   total be total + out.length()\n\
             \x20   let at be out.find(\"zwei\")\n\
             \x20   if at?:\n\
             \x20       total be total + at\n\
             \x20   if out.contains(\"zwei\") and out.ends_with(\"one\"):\n\
             \x20       total be total + 1\n\
             print(total)\n",
        ),
        // 27 bytes, found at 5, and 1: 33 a turn.
        "33000\n"
    );
}
