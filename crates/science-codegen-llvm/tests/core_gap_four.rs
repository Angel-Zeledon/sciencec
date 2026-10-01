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

#[test]
fn a_field_of_an_indexed_element_is_assigned() {
    let source = r#"type M:
    name: String
    value: Int

type Team:
    members: Array[M]

Team has:
    def rename(mutable self, i: Int, s: String):
        self.members[i].name be s

def relabel(members: &mut Array[M], i: Int):
    members[i].name be f"m{i}"

let mutable members be [M(name: "a", value: 1), M(name: "b", value: 2)]
let i be 1
members[i].value be 9
print(members[1].value)
members[0].name be "z"
print(members[0].name)
relabel(members, 1)
print(members[1].name)
let mutable t be Team(members: members)
t.rename(0, "first")
print(t.members[0].name)
t.members[1].value be 8
print(t.members[1].value)
"#;
    assert_eq!(prints("indexed_field", source), "9\nz\nm1\nfirst\n8\n");
}

#[test]
fn an_indexed_assignment_may_read_the_same_array() {
    let source = r#"def swap_to_front(perm: &mut Array[Int], k: Int, best: Int):
    perm[k] be perm[best]

let mutable v be [1, 2, 3]
v[0] be v[2]
print(v[0])
v[1] be v[0] + v[1]
print(v[1])
let mutable perm be [10, 20, 30]
let k be 0
let best be 2
perm[k] be perm[best]
print(perm[0])
swap_to_front(perm, 1, 2)
print(perm[1])
let mutable names be ["a", "b"]
names[0] be f"{names[1]}!"
print(names[0])
"#;
    assert_eq!(prints("index_assign_same_array", source), "3\n5\n30\n30\nb!\n");
}

#[test]
fn a_nested_tuple_index_is_two_indices_not_a_float() {
    let source = r#"let t be ((1, 2), (3, (4, 5)))
print(t.0.1)
print(t.1.0)
print(t.1.1.1)
let x be 0.5
print(x + 1.25)
"#;
    assert_eq!(prints("nested_tuple_index", source), "2\n3\n5\n1.75\n");
}

#[test]
fn a_function_name_is_a_closure_where_one_is_expected() {
    let source = r#"def half(x: Int) -> Int:
    x / 2

def add(a: Int, b: Int) -> Int:
    a + b

let xs be [2, 4, 6]
let ys be xs.iterate().map(half).collect()
print(ys.length())
let first be ys.get(0)
if first?:
    print(first)
print(xs.iterate().reduce(0, add))
print(xs.iterate().map(half).sum())
"#;
    assert_eq!(prints("function_value", source), "3\n1\n12\n6\n");
}
