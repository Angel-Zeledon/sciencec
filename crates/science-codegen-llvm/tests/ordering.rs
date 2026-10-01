//! `<`, `>`, `<=` and `>=` on user types, through `Ord.less`, **built, linked,
//! run**.
//!
//! `stdlib-shape-and-packages.md` §4.5's AMENDMENT 1 gives `Ord` one method,
//! `def less(self, other: &Self) -> Bool`, and `science-types`'
//! `BodyChecker::ordering` writes the four operators over it: `a < b` is
//! `a.less(b)`, `a > b` is `b.less(a)`, and `<=` and `>=` are the negations of
//! those two the other way round. `builtins.rs`' `Ord` entry is the decision
//! and what it costs.
//!
//! What has to hold, each pinned below by running it:
//!
//! - **A record and a `choice`** that implement `Ord` answer all four
//!   operators, and the answers are the order `less` defines — including the
//!   equal case, where `<=` and `>=` are both true and `<` and `>` both false.
//! - **Through a `T: Ord` bound**, the same generic body orders a user's
//!   record, an `Int`, a `Char` and a `String`. The record's call reaches its
//!   own `less` through `science_codegen::mono`'s redirect; the prelude's
//!   types have no `less` body and `Lowerer::builtin_less` answers with the
//!   instruction `a < b` already is on them. `largest[T: Ord](xs: &Array[T])
//!   -> (&T)?` is the program the task names, and it took a `science-mir`
//!   fix to run at all: `best be x` inside `if best?:` wrote the pointer
//!   *through* `best` instead of rebinding it.
//! - **The bundled `time` module** orders `Duration`s, which it could not
//!   while `Ord` had no method.
//! - **A type with no `Ord`** is still `SC0535` before anything is built, and
//!   one whose `less` takes `other` by value is `SC0541`.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("ordering", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Why the harness refused to lower `source`: its own assertion message,
/// which prints the checker's `(code, message)` pairs.
fn refused(source: &str) -> String {
    let caught = std::panic::catch_unwind(|| {
        lower(source);
    });
    let payload = caught.expect_err("the fixture must not check");
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

const VERSION: &str = "type Version:
    major: Int
    minor: Int

Version implements Ord:
    def less(self, other: &Version) -> Bool:
        if self.major is not other.major:
            return self.major < other.major
        self.minor < other.minor
";

/// All four operators on a record, at the three relations two values can be
/// in: less, greater and equal.
#[test]
fn a_record_that_implements_ord_answers_all_four_operators() {
    let source = format!(
        "{VERSION}
def order(a: Version, b: Version) -> String:
    f\"{{a < b}} {{a > b}} {{a <= b}} {{a >= b}}\"

def main():
    let old be Version(major: 1, minor: 2)
    let new be Version(major: 1, minor: 10)
    let next be Version(major: 2, minor: 0)
    print(order(old, new))
    print(order(next, new))
    print(order(old, Version(major: 1, minor: 2)))
"
    );
    assert_eq!(
        prints("record", &source),
        "true false true false\nfalse true false true\nfalse false true true\n"
    );
}

/// A `choice` orders through the same method, whatever its variants carry.
#[test]
fn a_choice_that_implements_ord_answers_all_four_operators() {
    assert_eq!(
        prints(
            "choice",
            "choice Level:
    Low
    Mid
    High(Int)

Level has:
    def rank(self) -> Int:
        match self:
            Low: 0
            Mid: 1
            High(n): 2 + n

Level implements Ord:
    def less(self, other: &Level) -> Bool:
        self.rank() < other.rank()

def main():
    print(Low < Mid)
    print(High(3) > High(1))
    print(Mid <= Low)
    print(High(0) >= High(0))
",
        ),
        "true\ntrue\nfalse\ntrue\n"
    );
}

/// **The program the task names**: `largest[T: Ord]` over an `Array`,
/// returning `(&T)?`, run at a user's record and at three prelude types, and
/// its mirror `smallest` beside it so that a `<` and a `>` are both exercised
/// through the bound.
///
/// The `String` words have different lengths, so that the printed length says
/// which word won and byte order is what chose it.
#[test]
fn a_generic_largest_bounded_by_ord_runs_at_a_record_and_at_prelude_types() {
    let source = format!(
        "{VERSION}
def largest[T: Ord](xs: &Array[T]) -> (&T)?:
    let mutable best be xs.get(0)
    for x in xs:
        if best?:
            if x > best: best be x
        else:
            best be x
    best

def smallest[T: Ord](xs: &Array[T]) -> (&T)?:
    let mutable best be xs.get(0)
    for x in xs:
        if best?:
            if x < best: best be x
        else:
            best be x
    best

def main():
    let mutable versions: Array[Version] be Array[Version].new()
    versions.push(Version(major: 1, minor: 2))
    versions.push(Version(major: 3, minor: 0))
    versions.push(Version(major: 1, minor: 9))
    let top be largest(versions)
    if top?:
        print(f\"{{top.major}}.{{top.minor}}\")
    let bottom be smallest(versions)
    if bottom?:
        print(f\"{{bottom.major}}.{{bottom.minor}}\")

    let mutable ints: Array[Int] be Array[Int].new()
    ints.push(4)
    ints.push(-7)
    ints.push(11)
    let high be largest(ints)
    if high?:
        print(high)
    let low be smallest(ints)
    if low?:
        print(low)

    let mutable letters: Array[Char] be Array[Char].new()
    letters.push('q')
    letters.push('b')
    letters.push('x')
    let late be largest(letters)
    if late?:
        print(late)

    let mutable words: Array[String] be Array[String].new()
    words.push(\"pear\")
    words.push(\"fig\")
    words.push(\"zucchini\")
    let last be largest(words)
    if last?:
        print(last.length())
    let first be smallest(words)
    if first?:
        print(first.length())

    let empty: Array[Int] be Array[Int].new()
    let none be largest(empty)
    if not none?:
        print(\"empty\")
"
    );
    assert_eq!(prints("largest", &source), "3.0\n1.2\n11\n-7\nx\n8\n3\nempty\n");
}

/// All four operators through a bound, at a record and at `Int` — the
/// swapped and negated rows of `BodyChecker::ordering` each reached once
/// per instantiation.
#[test]
fn all_four_operators_run_through_a_bound() {
    let source = format!(
        "{VERSION}
def order[T: Ord](a: &T, b: &T) -> String:
    f\"{{a < b}} {{a > b}} {{a <= b}} {{a >= b}}\"

def main():
    print(order(Version(major: 0, minor: 1), Version(major: 0, minor: 2)))
    print(order(5, 5))
    print(order(9, -1))
"
    );
    assert_eq!(
        prints("bound_four", &source),
        "true false true false\nfalse false true true\nfalse true false true\n"
    );
}

/// `time`'s `Duration` and `Instant` implement `Ord` now, and order the way
/// their nanosecond counts do.
#[test]
fn durations_and_instants_are_ordered() {
    assert_eq!(
        prints(
            "time_order",
            "use time (Duration, Instant)

def main():
    let short be Duration.from_milliseconds(250)
    let long be Duration.from_seconds(2)
    print(short < long)
    print(short >= long)
    print(-long < short)
    let early be Instant.from_unix_seconds(0)
    let late be Instant.from_unix_seconds(1)
    print(late > early)
    print(early <= early)
",
        ),
        "true\nfalse\ntrue\ntrue\ntrue\n"
    );
}

/// **The negative**: a record with no `implements Ord:` is `SC0535` at the
/// comparison, and a `less` that takes `other` by value is `SC0541` at the
/// method — both before anything is built.
#[test]
fn a_comparison_without_ord_is_refused_before_it_is_built() {
    let missing = refused(
        "type Point:
    x: Int

def main():
    print(Point(x: 1) < Point(x: 2))
",
    );
    assert!(missing.contains("the fixture must check"), "{missing}");
    assert!(missing.contains("(535, \"`Point` does not implement `Ord`\")"), "{missing}");

    let by_value = refused(
        "type Point:
    x: Int

Point implements Ord:
    def less(self, other: Point) -> Bool:
        self.x < other.x

def main():
    print(Point(x: 1) < Point(x: 2))
",
    );
    assert!(
        by_value.contains("(541, \"`less` does not have the signature `Ord` declares for it\")"),
        "a by-value `other` is the wrong signature: {by_value}"
    );
}
