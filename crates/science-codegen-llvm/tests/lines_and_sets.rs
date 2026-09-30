//! `String.lines()` and `Set of T`, **built, linked, run**: two of the three
//! names Gate C1 (`self-hosting.md` §10) stopped on.
//!
//! # `Lines`
//!
//! `stdlib-core.md` §4.5's Level 1 source. It walks the string it borrows,
//! `Chars`' way, and hands out **owned** lines — `builtins.rs` argues why a
//! borrowed line is not available. That made it the first iterator whose
//! `Item` owns something, and `science-mir`'s `for` lowering narrowed the
//! `next` result with a *copy*, which the backend rightly refused as a second
//! owner of one buffer. The narrow is a move now wherever the item is not
//! `Copy`.
//!
//! # `Set`
//!
//! A `ScienceMap` whose value is `()`, so four of its five methods are a map's
//! entry points and `insert`/`remove` are its own — `science-rt`'s `map`
//! module says why there is no second table.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("lines_and_sets", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// A `\r` before a `\n` goes with it, a blank line is a line, and a final
/// line with no terminator is still one.
#[test]
fn lines_splits_a_string_into_owned_lines() {
    assert_eq!(
        prints(
            "lines_split",
            "def main():
    let text be \"alpha\\nbeta\\r\\n\\ngamma\"
    for line in text.lines():
        print(f\"[{line}]\")
",
        ),
        "[alpha]\n[beta]\n[]\n[gamma]\n"
    );
}

/// The loop variable owns its line: it can be cloned into a map as a key and
/// read after, and the map's count is right.
#[test]
fn a_line_can_key_a_map() {
    assert_eq!(
        prints(
            "lines_count",
            "def main():
    let text be \"a\\nb\\na\\na\\n\"
    let mutable counts be Map[String, Int].new()
    for line in text.lines():
        let mutable n be 1
        let previous be counts.get(line)
        if previous?:
            n be previous + 1
        counts.insert(line.clone(), n)
    print(counts.length())
    let a be counts.get(\"a\")
    if a?:
        print(a)
",
        ),
        "2\n3\n"
    );
}

/// `insert` answers *new*, `remove` answers *was there*, and a `Set` of an
/// alias of `Int` is keyed by the integer pair `map_key_support` names.
#[test]
fn a_set_answers_membership() {
    assert_eq!(
        prints(
            "set_of_ids",
            "type DefId is Int

def main():
    let mutable seen be Set[DefId].new()
    let mutable fresh be 0
    for i in 0..10:
        let id: DefId be i % 4
        if seen.insert(id):
            fresh be fresh + 1
    print(fresh)
    print(seen.length())
    let probe: DefId be 3
    print(seen.contains(probe))
    print(seen.remove(probe))
    print(seen.remove(probe))
    print(seen.contains(probe))
",
        ),
        "4\n4\ntrue\ntrue\nfalse\nfalse\n"
    );
}

/// A literal handed to a by-value element slot is built into a slot of its
/// own and moved in — on a `Set of String` and on an `Array of String`, which
/// shared the refusal.
#[test]
fn a_string_literal_can_be_moved_into_a_set_or_an_array() {
    assert_eq!(
        prints(
            "set_of_strings",
            "def main():
    let mutable words be Set[String].new()
    print(words.insert(\"alpha\"))
    words.insert(\"beta\")
    print(words.insert(\"alpha\"))
    print(words.length())
    print(words.contains(\"beta\"))
    let mutable xs be Array[String].new()
    xs.push(\"pushed\")
    print(xs.length())
",
        ),
        "true\nfalse\n2\ntrue\n1\n"
    );
}
