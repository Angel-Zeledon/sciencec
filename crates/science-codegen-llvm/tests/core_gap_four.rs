//! The four small holes of "core gap 4", each built, linked and run.
//!
//! - `let a, _ be f()` discards an element of a destructured tuple;
//! - `f().0` reads a tuple element by position;
//! - `members[i].value be v` assigns to a field of an indexed element;
//! - a top-level function's name is a value where a closure is expected.
//!
//! Execution tests and not parse or check tests: each of these parsed or
//! checked clean at some point while still being refused or miscompiled by a
//! later phase, so only the printed bytes say it works.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("core-gap-four", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn a_wildcard_discards_an_element_of_a_destructured_tuple() {
    let source = r#"def triple() -> (Int, String, Int):
    (1, "two", 3)

let a, _ be (10, 20)
print(a)
let _, _, c be triple()
print(c)
let _, s, _ be triple()
print(s)
"#;
    assert_eq!(prints("wildcard", source), "10\n3\ntwo\n");
}

#[test]
fn a_tuple_element_is_read_by_position() {
    let source = r#"def pair() -> (Int, Int):
    (3, 4)

def named() -> (String, Int):
    ("hi", 7)

def total(t: &(Int, Int)) -> Int:
    t.0 + t.1

print(pair().1)
print(pair().0 + pair().1)
print(named().0)
let s be named().0
print(s)
let t be (5, 6)
print(t.1)
print(total(pair()))
let mutable m be (1i64, 2i64)
m.0 be 9
print(m.0 + m.1)
"#;
    assert_eq!(prints("tuple_field", source), "4\n7\nhi\nhi\n6\n7\n11\n");
}
