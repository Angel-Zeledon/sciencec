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

/// `stdlib-core.md` §6.3: `String implements Add` and `+` concatenates.
///
/// The checker refused it with `SC0535` because `Add` declares no method, so
/// nothing could dispatch and the structural fall-through knew only numbers.
/// It now answers `String` for two strings (either may be a `borrowed String`),
/// MIR reads both operands without consuming them, and the backend builds the
/// result from `science_string_clone` and `science_string_push_str`.
#[test]
fn string_plus_string_concatenates() {
    let source = "def greet(name: &String) -> String:
    \"hello, \" + name + \"!\"

def main():
    let mutable acc be \"\"
    for i in 0..3:
        acc be acc + \"x\"
    print(acc)
    let a be \"foo\"
    let b be \"bar\"
    let c be a + b
    print(c)
    print(a)
    print(\"lit\" + \"eral\")
    print(greet(b))
    print((a + b + a).length())
";
    assert_eq!(
        prints("string_concat", source),
        "xxx\nfoobar\nfoo\nliteral\nhello, bar!\n9\n"
    );
}

/// `Stack[String](items: ...)`: a record built with one explicit type argument.
/// `Stack[String]` and `xs[i]` are the same tokens, so the parser read it as a
/// subscript and refused the construction with `SC0110`; a named-argument list
/// right after the brackets is what marks them as type arguments.
#[test]
fn a_record_built_with_explicit_type_arguments_runs() {
    let source = "type Stack[T]:
    items: Array[T]
    label: String

def main():
    let mutable s be Stack[String](items: Array[String].new(), label: \"words\")
    s.items.push(\"a\")
    s.items.push(\"b\")
    print(s.items.length())
    print(s.label)
    let t be Stack[Int](items: [1, 2, 3], label: \"n\")
    print(t.items.length())
    let xs be [5, 6]
    let i be 1
    print(xs[i])
";
    assert_eq!(prints("explicit_record_args", source), "2\nwords\n3\n6\n");
}

/// A closure factory: `def make(k: Int) -> (Int) -> Int: x giving x * k`.
///
/// Every capture was a borrow of the creating frame's slot, so returning the
/// closure returned a borrow of `k` and rule 5 refused it (`SC0333`). A `Copy`
/// capture that is only read is now the value, held in a heap environment
/// (`science-mir`'s `lower.rs` §8.7), so two closures from one factory each
/// keep their own.
#[test]
fn a_returned_closure_captures_copy_values_by_value() {
    let source = "def scale(k: Int, offset: Int) -> (Int) -> Int:
    x giving x * k + offset

def above(limit: F64) -> (F64) -> Bool:
    v giving v > limit

def main():
    let a be scale(3, 1)
    let b be scale(10, 2)
    print(a(5))
    print(b(5))
    print(a(1))
    let hot be above(30.5)
    print(hot(40.0))
    print(hot(2.0))
";
    assert_eq!(prints("returned_closure", source), "16\n52\n4\ntrue\nfalse\n");
}

/// `let add be u giving u + 5`: nothing gave `u` a type, so it was `Ty::ERROR`
/// with no diagnostic and the backend refused with `SC0400`. The body's own
/// literal fixes it now (a closure parameter is an inference variable that the
/// body may solve and a numeric literal may default), and a closure whose body
/// fixes nothing is `SC0526` asking for the annotation instead.
#[test]
fn an_unannotated_let_closure_infers_its_parameter_from_its_body() {
    let source = "def main():
    let add be u giving u + 5
    print(add(2))
    let half be v giving v / 2.0
    print(half(5.0))
";
    assert_eq!(prints("let_closure_infers", source), "7\n2.5\n");
}

/// `x.clone()` on an `Int` or a `Float`. The prelude listed `Int`/`Float` *and*
/// `I64`/`F64` as implementing `Clone`, and they are one definition, so the
/// call had two candidates (`SC0531`) whose diagnostic then panicked
/// `sciencec` itself rendering a label in the prelude's file.
#[test]
fn clone_on_an_int_and_a_float_runs() {
    let source = "def main():
    let x be 2.5
    let y be x.clone()
    print(y)
    let n be 7
    print(n.clone() + 1)
    let f: F32 be 1.5
    print(f.clone())
";
    assert_eq!(prints("clone_scalar", source), "2.5\n8\n1.5\n");
}

/// `let held be items[i]` over a `Copy` element copies it out, so a later
/// `push` or write into the array is not a conflict with a live loan.
#[test]
fn a_let_of_an_indexed_copy_element_is_a_copy() {
    let source = "def main():
    let mutable items be Array[Int].new()
    items.push(1)
    items.push(2)
    let held be items[0]
    items.push(5)
    items[0] be 9
    print(held + 10)
    print(items[0])
";
    assert_eq!(prints("let_index_copy", source), "11\n9\n");
}

/// `&mut Array[T]` where `&Array[T]` is expected: a shared reborrow of the
/// exclusive borrow, which stays usable afterwards.
#[test]
fn an_exclusive_borrow_passes_where_a_shared_one_is_expected() {
    let source = "def total(items: &Array[Int]) -> Int:
    let mutable s be 0
    for i in items:
        s be s + i
    s

def fill(items: &mut Array[Int]) -> Int:
    items.push(4)
    let a be total(items)
    items.push(5)
    let b be total(items)
    a * 100 + b

def main():
    let mutable a be [1]
    print(fill(a))
";
    assert_eq!(prints("mut_to_shared", source), "510\n");
}

/// `Array.clone()` of an element that owns nothing is an independent copy.
#[test]
fn array_clone_copies_a_plain_element_array() {
    let source = "def main():
    let mutable a be [1, 2]
    let b be a.clone()
    a.push(3)
    a[0] be 99
    print(b.length())
    print(a.length())
    print(b[0])
    let fs be [1.5, 2.5]
    let g be fs.clone()
    print(g[1])
";
    assert_eq!(prints("array_clone", source), "2\n3\n1\n2.5\n");
}

/// `&[]` passed to a `&Array[Int]` parameter takes its element type from the
/// parameter instead of `SC0282`.
#[test]
fn an_empty_borrowed_literal_takes_its_type_from_the_parameter() {
    let source = "def count(items: &Array[Int]) -> Int:
    items.length()

def main():
    print(count(&[]))
    print(count(&[1, 2, 3]))
";
    assert_eq!(prints("empty_borrowed_literal", source), "0\n3\n");
}

/// A field read off a narrowed owned nullable record neither moves the record
/// nor drops it early: `best.tag` twice inside `if best?:`, and every record is
/// dropped exactly once, at the end of `main`.
#[test]
fn a_field_of_a_narrowed_owned_record_does_not_drop_it() {
    let source = "type Rec:
    tag: Int

Rec implements Drop:
    def drop(mutable self):
        print(f\"drop {self.tag}\")

def main():
    let xs be [1, 2, 3]
    let best be xs.iterate().map(x giving Rec(tag: x * 10)).find(each.tag > 15)
    if best?:
        print(f\"{best.tag} {best.tag}\")
        print(\"in\")
    print(\"after if\")
    print(\"end\")
";
    assert_eq!(
        prints("narrowed_field_no_drop", source),
        "drop 10\n20 20\nin\nafter if\nend\ndrop 20\n"
    );
}

/// `map(each.item)` over `numbered()` of borrowed items: `Numbered[&Doc]`'s
/// `item` is a region of the record parameter, so the returned borrow is tied
/// to it and `SC0340` does not fire.
#[test]
fn map_of_each_item_over_numbered_borrows_runs() {
    let source = "type Doc:
    title: String

def count(docs: &Array[Doc]) -> Int:
    docs.iterate().numbered().map(each.item).count()

def main():
    let docs be [Doc(title: \"ab\"), Doc(title: \"cde\")]
    print(count(docs))
    let mutable total be 0
    for t in docs.iterate().numbered().map(each.item):
        total be total + t.title.length()
    print(total)
";
    assert_eq!(prints("numbered_item_borrow", source), "2\n5\n");
}

/// `panic(err)` with an `Error` prints the error's `message()`.
#[test]
fn panic_of_an_error_prints_its_message() {
    let source = "def save() -> Error?:
    let e be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if e?: return e
    null

def main():
    let e be save()
    if e?:
        panic(e)
";
    let dir = scratch("valid_programs", "panic_error");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "panic_error"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_ne!(ran.status, Some(0));
    assert!(ran.stderr.contains("panic: not found"), "stderr: {}", ran.stderr);
}

/// `&x` where `x` is already a `&T`, at a call expecting `&T`. The checker
/// types it as the one borrow, but the place was `x`'s own slot, so the callee
/// was handed a pointer to a pointer: `show(&record)` printed the length of
/// whatever the slot's address pointed at. It is a reborrow of what `x`
/// points at now, shared and exclusive.
#[test]
fn a_borrow_of_a_borrow_is_a_reborrow() {
    let source = "def show(rows: &Array[String]) -> Int:
    rows.length()

def bump(xs: &mut Array[Int]):
    xs.push(1)

def twice(xs: &mut Array[Int]) -> Int:
    bump(&mut xs)
    bump(&mut xs)
    xs.length()

def main():
    let rows be [[\"a\", \"b\"], [\"c\", \"d\"]]
    for record in rows:
        print(show(&record))
    let r be &rows[0]
    print(show(&r))
    print(show(r))
    let mutable a be [0]
    print(twice(&mut a))
";
    assert_eq!(prints("borrow_of_a_borrow", source), "2\n2\n2\n2\n3\n");
}

/// `Array[String].clone()` clones every string into its own buffer.
#[test]
fn array_clone_of_strings_is_independent() {
    let source = "def main():
    let mutable a be [\"x\", \"yy\"]
    let b be a.clone()
    a[0].push_str(\"zz\")
    print(b.length())
    print(b[0])
    print(a[0])
    print(b[1])
";
    assert_eq!(prints("array_clone_strings", source), "2\nx\nxzz\nyy\n");
}

/// `Array.clone()` of elements that own more than a `String`: a record with a
/// `Clone` impl, and arrays of arrays. Every original and every copy is dropped
/// exactly once (four `drop` lines for two docs cloned once), and the copy is
/// independent of the original.
#[test]
fn array_clone_of_owning_elements_drops_each_copy_once() {
    let source = "type Doc:
    title: String
    n: Int

Doc implements Clone:
    def clone(self) -> Doc:
        Doc(title: self.title.clone(), n: self.n)

Doc implements Drop:
    def drop(mutable self):
        print(f\"drop {self.title}\")

def copies(docs: &Array[Doc]) -> Array[Doc]:
    docs.clone()

def main():
    let docs be [Doc(title: \"a\", n: 1), Doc(title: \"b\", n: 2)]
    let copy be copies(docs)
    print(copy.length())
    let mutable nest be [[1, 2], [3]]
    let nc be nest.clone()
    nest[0].push(9)
    print(nc[0].length())
    print(nest[0].length())
    let names be [[\"p\", \"q\"], [\"r\"]]
    let nn be names.clone()
    print(nn[0][1])
";
    assert_eq!(
        prints("array_clone_owning", source),
        "2\n2\n3\nq\ndrop a\ndrop b\ndrop a\ndrop b\n"
    );
}

/// An escaping closure owns the value it captured (`science-mir` §8.7): the
/// value moves into the environment, is released exactly once when the closure
/// is dropped (wherever it ended up: a local, a parameter, a record field, an
/// array element), and the environment block itself is freed.
#[test]
fn an_escaping_closure_owns_its_capture_and_releases_it_once() {
    let source = "type Tag:
    label: String

Tag implements Drop:
    def drop(mutable self):
        print(f\"drop {self.label}\")

type Holder:
    f: (Int) -> Int
    n: Int

def tagged(tag: Tag) -> (Int) -> Int:
    x giving x + tag.label.length()

def hold(tag: Tag) -> Holder:
    Holder(f: tagged(tag), n: 1)

def pick(flag: Bool, a: Tag, b: Tag) -> (Int) -> Int:
    if flag:
        return tagged(a)
    tagged(b)

def twice(f: (Int) -> Int, v: Int) -> Int:
    f(f(v))

def main():
    let h be hold(Tag(label: \"held\"))
    print((h.f)(1))
    let p be pick(true, Tag(label: \"A\"), Tag(label: \"B\"))
    print(p(0))
    let q be tagged(Tag(label: \"consumed\"))
    print(twice(q, 1))
    print(\"end\")
    let fs be [tagged(Tag(label: \"e1\")), tagged(Tag(label: \"e2\"))]
    print(fs.length())
";
    assert_eq!(
        prints("escaping_closure_owns", source),
        "5\ndrop B\n1\ndrop consumed\n17\nend\n2\ndrop e1\ndrop e2\ndrop A\ndrop held\n"
    );
}

/// An array of string literals that is only walked, never measured.
///
/// The array's descriptor names `science_string_free` as its element's drop
/// glue, and the module only declared that function when some drop
/// terminator called it directly. A program that frees its strings only
/// through the array — `for p in ["x", "y"]:` — failed with `SC0402`, *the
/// descriptor names drop glue the module does not define*.
#[test]
fn an_array_of_string_literals_that_is_only_iterated_runs() {
    let source = "def main():
    for p in [\"x\", \"y\"]:
        print(p)
    let planets be [\"Mercury\", \"Venus\"]
    for name in planets:
        print(name)
";
    assert_eq!(prints("string_literal_array", source), "x\ny\nMercury\nVenus\n");
}
