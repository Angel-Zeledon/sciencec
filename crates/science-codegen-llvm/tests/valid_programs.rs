//! Valid programs the backend used to refuse or miscompile, **built, linked,
//! run**.
//!
//! Each test is one program that checks clean and that a first-time reader of
//! `examples/` would write, and each names the defect that stopped it. They
//! are here and not in the corpus because the examples' output is pinned byte
//! for byte and a function an example never calls is not a function whose
//! behaviour the pin can change.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("valid_programs", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `examples/07_generics.science`'s `largest`, which `main` never calls.
///
/// **Two defects stood in front of it, one per phase.** `best be item` inside
/// `if best?:` is an assignment to a `(&Int)?` narrowed to `&Int`, and
/// `science-mir` read it as §4.7's *"borrows auto-dereference for assignment"*
/// and stored the pointer **through** the narrowed one, into the `Int` it
/// points at: `SC0402`, *local is i64 and the value stored into it is ptr*.
/// The value is itself the borrow, so it rebinds, which is what
/// `assign_target` already did for a non-narrowed `r be borrowed y`. And
/// `item > best` over `T: Ord` reaches the backend as two `&Int`, because the
/// coercion that reads a `Copy` referent cannot see through a bound; the
/// referents are what compare.
#[test]
fn a_loop_reassigning_a_nullable_borrow_runs() {
    let source = "def largest[T: Ord](items: &Array[T]) -> (&T)?:
    let mutable best be items.get(0)
    for item in items:
        if best?:
            if item > best: best be item
        else:
            best be item
    best

def smallest(items: &Array[Int]) -> (&Int)?:
    let mutable best be items.get(0)
    for item in items:
        if best?:
            if item < best: best be item
        else:
            best be item
    best

def main():
    let xs be [3, 9, 4]
    let top be largest(xs)
    if top?: print(top)
    let low be smallest(xs)
    if low?: print(low)
    let none be Array[Int].new()
    let nothing be largest(none)
    if nothing?: print(\"found\") else: print(\"empty\")
";
    assert_eq!(prints("loop_reassigns_borrow", source), "9\n3\nempty\n");
}

/// `"a" < "b"`: two literals have no place and so no operand type, and the arm
/// that sends a `String` comparison to the runtime never ran. The refusal was
/// *"a comparison of two constants"*. The ordering is `stdlib-core.md` §6.8's
/// byte order, so `"Z" < "a"` and a prefix sorts first.
#[test]
fn two_string_literals_compare() {
    let source = "def main():
    print(\"a\" < \"b\")
    print(\"b\" <= \"a\")
    print(\"b\" > \"a\")
    print(\"a\" >= \"a\")
    print(\"a\" is \"a\")
    print(\"a\" is not \"a\")
    print(\"Z\" < \"a\")
    print(\"ab\" < \"abc\")
";
    assert_eq!(
        prints("string_literals", source),
        "true\nfalse\ntrue\ntrue\ntrue\nfalse\ntrue\ntrue\n"
    );
}

/// `print` of a `(&String)?` narrowed by `if s?:`. `lower_print` recognised a
/// `borrowed String` and not the nullable of one, though Decision 19's niche
/// makes the two the same pointer, so the narrowed local fell through to
/// *"`(&String)?` has no `display` this compiler can call"*. `write` shares the
/// arm, and the absent case must take the other branch and print nothing.
#[test]
fn print_of_a_narrowed_borrowed_string_runs() {
    let source = "def main():
    let mutable m be Map[String, String].new()
    m.insert(\"k\", \"v\")
    let s be m.get(\"k\")
    if s?:
        print(s)
        write(s)
        write(\"|\")
    let t be m.get(\"absent\")
    if t?: print(t) else: print(\"none\")
";
    assert_eq!(prints("narrowed_string", source), "v\nv|none\n");
}

/// `Array[(U8, Int)].new()` and `Array[String?].new()` in an expression. The
/// resolver read a one-argument instantiation back from the parser's `Index`
/// for a path, a nested index and a borrow, and not for a tuple or a
/// presence test, so the type was the error type with no diagnostic and the
/// backend refused with `SC0400`. The same types in an annotation always
/// worked.
#[test]
fn a_tuple_or_nullable_type_argument_in_an_expression_runs() {
    let source = "def main():
    let mutable a be Array[(U8, Int)].new()
    a.push((1, 2))
    a.push((3, 4))
    print(a.length())
    for p in a:
        match p:
            (x, y): print(y)
    let mutable n be Array[String?].new()
    n.push(\"x\")
    n.push(null)
    print(n.length())
    let mutable m be Map[String, (Int, Int)].new()
    print(m.length())
";
    assert_eq!(prints("tuple_type_argument", source), "2\n2\n4\n2\n0\n");
}

/// A prelude `IoError` and `TextError` boxed into `Error?`.
///
/// **Two defects, one per layer.** `intern_descriptor` asked for a record's or
/// a `choice`'s layout and the two error types are neither, so the build
/// stopped at *"a box of `IoError`, which is neither a record nor a `choice`"*.
/// Past that, the `(Error, IoError)` vtable's `message` slot named the mangled
/// `_S7IoError7message`, a function no module defines: the method is a
/// `science-rt` entry point, and the slot now holds `science_io_error_message`
/// (and `science_text_error_message`), which takes a pointer and returns a
/// `String` through `sret` exactly as the Science method would. The call goes
/// through the slot twice, once on the boxed value and once through a
/// `&any Error` parameter.
#[test]
fn a_prelude_error_boxed_into_error_runs() {
    let source = "def save() -> Error?:
    let e be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if e?: return e
    null

def parse(text: &String) -> Error?:
    let n, err be text.parse_int()
    if err?: return err
    print(n)
    null

def report(err: &any Error):
    print(err.message())

def main():
    let e be save()
    if e?:
        report(e)
        print(e.message())
    let p be parse(\"12\")
    if p?: print(\"bad\")
    let q be parse(\"zz\")
    if q?: print(q.message())
";
    assert_eq!(
        prints("prelude_error_boxed", source),
        "not found\nnot found\n12\nnot a number\n"
    );
}

/// A closure passed into a generic function, **and a literal passed to a
/// generic parameter**. These were reported as one defect and are two.
///
/// *The literal.* `run(3, 1)` where `run[T](x: T, k: Int)` named no instance:
/// `science_codegen::mono` solves a callee's parameters from the types of the
/// argument *places*, an `Operand::Const` has none, and the destination (an
/// `Int`) says nothing about `T`. `science-mir` now gives a scalar literal
/// passed to a generic callee a typed temporary. That alone was the refusal
/// *"a value whose type is still a type parameter"* on `run(3, n giving
/// n + 1)`, which is why it looked like the closure's fault.
///
/// *The closure.* `instantiate_call` synthesised a closure argument against
/// the callee's declared `(T) -> T` with `T` still in it, in source order, so
/// `n` was a `T` and the closure's type `(T) -> T` — which `science-mir`
/// withholds a body from. Closures are now checked after the other arguments,
/// against the declared type with what is solved put in, and a parameter whose
/// only evidence is an unsuffixed literal takes Decision 2's default at that
/// point. The cases cover a closure whose result is another type, one that
/// captures, a float and a `Bool`.
#[test]
fn a_closure_passed_into_a_generic_function_runs() {
    let source = "def run[T](x: T, k: Int) -> Int:
    k

def apply[T](x: T, f: (T) -> T) -> T:
    f(x)

def convert[T, U](x: T, f: (T) -> U) -> U:
    f(x)

def twice[T](x: T, f: (T) -> T) -> T:
    f(f(x))

def main():
    print(run(3, 1))
    print(run(true, 2))
    let offset be 10
    print(apply(3, n giving n + offset))
    print(convert(4, n giving n > 3))
    let half: F64 be 2.5
    print(twice(half, h giving h * 2.0))
    print(convert(7, n giving n * 2))
    let flag be apply(true, b giving not b)
    print(flag)
    let three be 3
    print(apply(three, n giving n + 1))
";
    assert_eq!(
        prints("closure_into_generic", source),
        "1\n2\n13\ntrue\n10.0\n14\nfalse\n4\n"
    );
}
