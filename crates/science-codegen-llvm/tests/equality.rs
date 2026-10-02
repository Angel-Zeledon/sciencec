//! `is` through a type parameter bounded by `Eq`, and a record built with every
//! field, **built, linked, run**.
//!
//! `a is b` over two `&T` with `T: Eq` is a call to `Eq.eq`, resolved at each
//! instantiation: a user's `eq` for a record, the instruction for a scalar,
//! `science_string_eq` for a `String`. And the `SC0551` check that a record
//! literal names every field must not refuse a literal that does.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("equality", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn is_over_borrowed_type_parameters_dispatches_to_eq() {
    let source = "type Tag:
    name: String

Tag implements Eq:
    def eq(self, other: &Tag) -> Bool:
        self.name is other.name

def same[T](a: &T, b: &T) -> Bool
        where T: Eq:
    a is b

def differ[T](a: &T, b: &T) -> Bool
        where T: Eq:
    a is not b

def main():
    let t be Tag(name: \"a\")
    let u be Tag(name: \"a\")
    let v be Tag(name: \"b\")
    print(same(t, u))
    print(same(t, v))
    print(differ(t, v))
    print(same(\"x\", \"x\"))
    print(same(3, 4))
    print(t.name)
";
    assert_eq!(prints("generic_eq", source), "true\nfalse\ntrue\ntrue\nfalse\na\n");
}

#[test]
fn a_record_with_every_field_still_builds_in_any_order() {
    let source = "type Reading:
    channel: Int
    value: F64

def main():
    let a be Reading(channel: 1, value: 2.5)
    let b be Reading(value: 4.0, channel: 7)
    print(a.channel)
    print(a.value)
    print(b.channel)
    print(b.value)
";
    assert_eq!(prints("all_fields", source), "1\n2.5\n7\n4.0\n");
}

#[test]
fn a_comparison_does_not_consume_its_operands() {
    let source = "def main():
    let a be \"x\"
    let b be \"x\"
    if a is b:
        print(\"same\")
    print(a)
    print(b)
";
    assert_eq!(prints("reuse", source), "same\nx\nx\n");
}
