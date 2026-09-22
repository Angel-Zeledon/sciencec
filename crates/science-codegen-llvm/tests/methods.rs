//! Method calls, **built, linked, run**, with stdout and exit status asserted.
//!
//! # The measurement this file exists for
//!
//! Every program in `examples/` was built with this backend and none of them
//! reached an object file. In **nine of the twenty-two** the first refusal was
//! a method call, in two spellings. Six of them — `is_empty`, `preview`,
//! `message`, `add`, `summarize` — were refused at the *signature*, saying
//! *"its receiver is a `Self` this crate cannot resolve to a concrete type"*.
//! The other three were `String.length()` and `String.new()`, refused at the
//! call as *"a call to `length`, which this crate was given no MIR body for"* —
//! which is true and is not a gap: a prelude method has no Science body and
//! never will, and `Lowerer::prelude_method` is the table that says which
//! `RUNTIME` entry point it is instead.
//!
//! **The sentence was false.** `science-types`' body substitution rewrites
//! `Self` before THIR is built, so the receiver of `Doc has: def is_empty
//! (self)` reaches MIR as a local of type `borrowed Doc` — already concrete,
//! already in `Body::params`'s dense prefix, already what every statement in
//! the body was lowered against. The refusal was guarding against a shape that
//! does not arrive. Crate §3 finding 24 is the account.
//!
//! What was left of it was one block: the **default body** of a method an
//! `interface` declares, whose `self` really is a `Self` and which needs one
//! copy per implementor. That is monomorphisation, it is above Decision 42's
//! line, and the walk does it now — an `Instance` carries a `self_ty` beside
//! its type arguments, so one body becomes one function per implementor.
//! [`a_default_body_runs_and_differs_per_implementor`] is what replaced the
//! refusal this paragraph used to point at.
//!
//! # Why every test here runs the program
//!
//! §10's discipline, and this file has a specific reason of its own. A method
//! call is a call whose first argument is a pointer the *lowering* inserted —
//! §9 of `science-mir`'s `lower` is the reborrow rule that decides what it
//! points at — and `crate::lower`'s findings 12, 18 and 23 are all the same
//! shape: opaque pointers mean LLVM cannot tell a pointer to a `Doc` from a
//! pointer to the *slot holding* a pointer to a `Doc`. `LLVMVerifyModule`
//! accepts both. So the values below are chosen so that a receiver read through
//! the wrong indirection prints a different number or crashes, and the test
//! reads what the program wrote.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
///
/// `-O2` for `tests/printing.rs`'s reason: a receiver is an `alloca` and an
/// address, and the optimiser is where a load through the wrong one stops
/// being indistinguishable from a load through the right one.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("methods", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The IR of a program that built, for the two assertions that are about shape.
fn ir(name: &str, source: &str) -> String {
    let dir = scratch("methods", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O0);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

/// What stopped a program that did not build.
fn refusal(name: &str, source: &str) -> String {
    let dir = scratch("methods", name);
    let result = lower(source).try_build(&executable(&dir, name), OptLevel::O0);
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(_) => panic!("the program built and this test is about the refusal"),
        Err(diagnostics) => {
            diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>().join("\n")
        }
    }
}

// --- the receiver, in each of the three ways it crosses ---------------------

/// A shared receiver: `self`, which §4's classifier passes as a pointer.
///
/// `409` rather than `1` because a receiver read through one pointer too many
/// would read the *address* of the frame slot, and any small number could be
/// one by accident.
#[test]
fn a_shared_receiver_reads_its_own_fields() {
    let source = "\
type Doc:
    first: Int
    second: Int

Doc has:
    def total(self) -> Int:
        self.first + self.second

def main():
    let doc be Doc(first: 400, second: 9)
    print(doc.total())
";
    assert_eq!(prints("shared", source), "409\n");
}

/// An exclusive receiver: `mutable self`, and the caller sees the write.
///
/// **This is the one that cannot pass by accident.** If the receiver pointer
/// were the address of the caller's *reference temporary* rather than of the
/// `Doc`, the method would write into that temporary and the caller would
/// print the value it started with. So the test prints the field after the
/// call and the two numbers differ.
#[test]
fn an_exclusive_receiver_writes_through_to_the_caller() {
    let source = "\
type Counter:
    hits: Int

Counter has:
    def bump(mutable self):
        self.hits be self.hits + 41

def main():
    let mutable c be Counter(hits: 1)
    print(c.hits)
    c.bump()
    print(c.hits)
";
    assert_eq!(prints("exclusive", source), "1\n42\n");
}

/// A receiver taken by value: `self: Self`, which is Decision 22's aggregate
/// argument — a pointer to a slot the caller gives up.
#[test]
fn a_by_value_receiver_is_an_aggregate_argument() {
    let source = "\
type Point:
    x: Int
    y: Int

Point has:
    def sum(self: Self) -> Int:
        self.x + self.y

def main():
    let p be Point(x: 300, y: 33)
    print(p.sum())
";
    assert_eq!(prints("byvalue", source), "333\n");
}

/// A method that calls another method on the same receiver: §9's reborrow.
///
/// Inside `outer`, `self` is already a `borrowed Doc`, so the receiver borrow
/// `inner` is given must name `(*_1)` and not `_1`. Naming `_1` hands `inner`
/// the address of `outer`'s own parameter slot, which is a pointer read as a
/// `Doc` — finding 18's shape, one construct over, and it verifies.
#[test]
fn a_method_calling_a_method_on_self_reborrows() {
    let source = "\
type Doc:
    n: Int

Doc has:
    def inner(self) -> Int:
        self.n

    def outer(self) -> Int:
        self.inner() + self.inner()

def main():
    let doc be Doc(n: 21)
    print(doc.outer())
";
    assert_eq!(prints("reborrow", source), "42\n");
}

// --- associated functions ---------------------------------------------------

/// An associated function has no receiver at all, and is an ordinary call.
///
/// `science-types`' `thir` says so in as many words — *"an associated function
/// is not one of these: `Doc.blank()` has no receiver value, so it is an
/// `ExprKind::Call` at the method's own definition"* — and the only thing that
/// stopped it was the blanket refusal on `Signature::self_param`, which an
/// associated function does not have. It was the cheapest half.
#[test]
fn an_associated_function_is_called_through_its_type() {
    let source = "\
type Doc:
    n: Int

Doc has:
    def make(n: Int) -> Doc:
        Doc(n: n)

    def get(self) -> Int:
        self.n

def main():
    print(Doc.make(7).get())
";
    assert_eq!(prints("assoc", source), "7\n");
}

/// Two blocks, one method name, two symbols.
///
/// Decision 16 mangles the definition's path, and a method's path runs through
/// its block — so `Left.value` and `Right.value` cannot collide. If they did,
/// the second definition would silently replace the first and both calls would
/// print the same number.
#[test]
fn two_types_may_declare_the_same_method_name() {
    let source = "\
type Left:
    n: Int

type Right:
    n: Int

Left has:
    def value(self) -> Int:
        self.n + 1000

Right has:
    def value(self) -> Int:
        self.n + 2000

def main():
    print(Left(n: 1).value())
    print(Right(n: 1).value())
";
    assert_eq!(prints("names", source), "1001\n2001\n");
}

/// A method written in an `implements` block, called on a concrete receiver.
///
/// The lookup answers with the *implementation's* method, not the interface's
/// declaration — `science-types`' `methods`' `head` answers an interface only
/// for a `TyKind::Object` — so this is a direct call and needs no vtable.
#[test]
fn a_method_from_an_implements_block_is_called_directly() {
    let source = "\
interface Sized:
    def size(self) -> Int

type Box2:
    n: Int

Box2 implements Sized:
    def size(self) -> Int:
        self.n * 3

def main():
    print(Box2(n: 14).size())
";
    assert_eq!(prints("implements", source), "42\n");
}

// --- the prelude's own methods ----------------------------------------------

/// `String.length()` and `String.is_empty()`: prelude declarations with no
/// body, lowered to the `RUNTIME` entry points that implement them.
///
/// **The numbers are bytes and not characters**, which is §6.5's decision and
/// the cost the prelude records in place: `"héllo"` is six.
#[test]
fn the_preludes_string_methods_are_runtime_entry_points() {
    let source = "\
def main():
    let s be \"héllo\"
    print(s.length())
    print(s.is_empty())
    let e be \"\"
    print(e.is_empty())
";
    assert_eq!(prints("string", source), "6\nfalse\ntrue\n");
}

/// `Clone.clone()` on a `String`: `builtins.rs`'s `INTERFACE_DECLS` declares
/// `Clone.clone(self) -> Self`, `String implements Clone:` writes no `clone`
/// of its own, so the call resolves to the interface's contributed method —
/// and `lower.rs`'s `prelude_method` has to read the concrete receiver off
/// the call rather than off that contributed method's `owner`, which is
/// `Clone` and has no `Self` of its own to ask. Before that fix this refused
/// at codegen with *"a call to the method `clone` through `any Clone`"*, a
/// vtable message about a program that never wrote a trait object.
///
/// The mutation after cloning is the assertion: if `clone` had handed back
/// the same buffer, `original`'s `push_str` would be visible through `copy`
/// too, and it is not — `science_string_clone` is documented as *"a fresh,
/// independent copy"* and this is what tests that rather than merely that a
/// call resolved.
#[test]
fn a_string_clone_is_an_independent_copy() {
    let source = "\
def main():
    let mutable original be String.new()
    original.push_str(\"hi\")
    let copy be original.clone()
    original.push_str(\" there\")
    print(copy)
    print(original)
";
    assert_eq!(prints("clone", source), "hi\nhi there\n");
}

/// `String.new()` and `String.push_str(…)`: an associated function that
/// returns an aggregate through `sret`, and a method with an exclusive
/// receiver and a `borrowed String` argument.
///
/// This is `examples/13_inline_blocks.science`'s own sentence — *"§8 gives
/// `String` no concatenation method, so the two halves are joined with
/// `String.new()` and `push_str`"* — run.
#[test]
fn a_string_is_built_with_new_and_push_str() {
    let source = "\
def main():
    let mutable out be String.new()
    print(out.length())
    out.push_str(\"marked\")
    out.push_str(\"down\")
    print(out)
    print(out.length())
";
    assert_eq!(prints("pushstr", source), "0\nmarkeddown\n10\n");
}

/// A method call inside a function that takes its receiver by borrow.
///
/// `def longest(a: borrowed String, b: borrowed String) -> borrowed String` is
/// `examples/01_functions.science`'s, and both the `length()` calls and the
/// returned borrow go through a reference the caller owns.
#[test]
fn a_method_is_called_through_a_borrowed_parameter() {
    let source = "\
def longest(a: &String, b: &String) -> &String:
    if a.length() > b.length(): a else: b

def main():
    print(longest(\"abc\", \"de\"))
    print(longest(\"a\", \"bcde\"))
";
    assert_eq!(prints("longest", source), "abc\nbcde\n");
}

// --- what `print` does with something that is not a `String` ----------------

/// Every width the builder renders, at values where a wrong answer is visible.
///
/// `print(x)` and `print(f"{x}")` mean the same thing and only the second one
/// reached an executable; `science-mir`'s `lower_print_rendered` makes them one
/// lowering. The values are the ones `push_of`'s own note is about: `-1i32`
/// widened as unsigned prints `4294967295`, and `18446744073709551615u64`
/// rendered through the signed entry point prints `-1`.
#[test]
fn print_renders_every_width_the_builder_has() {
    let source = "\
def main():
    print(42)
    print(-7)
    print(-1i8)
    print(-1i16)
    print(-1i32)
    print(255u8)
    print(65535u16)
    print(4294967295u32)
    print(18446744073709551615u64)
    print(0.5f32)
    print(0.1 + 0.2)
    print(true)
    print(false)
    print('ñ')
";
    assert_eq!(
        prints("widths", source),
        "42\n-7\n-1\n-1\n-1\n255\n65535\n4294967295\n18446744073709551615\n0.5\n\
         0.30000000000000004\ntrue\nfalse\nñ\n"
    );
}

/// A `borrowed String` printed is the pointer it already is, and nothing is
/// freed.
///
/// Printed twice and then read again, for `tests/printing.rs`'s reason: a
/// `science_string_free` on a buffer the caller still owns prints an empty
/// line the second time and a human reads straight past it.
#[test]
fn printing_a_borrowed_string_frees_nothing() {
    let source = "\
def show(s: &String):
    print(s)
    print(s)

def main():
    let s be \"hola\"
    show(s)
    print(s)
    print(s.length())
";
    assert_eq!(prints("borrowed", source), "hola\nhola\nhola\n4\n");
}

/// `print` of a value the builder has no entry point for names the **type**.
///
/// The refusal used to be *"a `print` of a value that is not a `String`"*,
/// which names the construct and not the cause. A user type that implements
/// `Display` is accepted by the checker — `science-types`' `check` records
/// that it is *"accepted here and refused by codegen, which is a worse place
/// to find out"* — so the least this crate owes is the name of the type.
#[test]
fn print_of_a_user_type_names_the_type() {
    let source = "\
type Doc:
    n: Int

Doc implements Display

def main():
    print(Doc(n: 1))
";
    let text = refusal("display", source);
    assert!(text.contains("`Doc`"), "the refusal must name the type: {text}");
    assert!(text.contains("Formatter"), "and what a user type would render through: {text}");
}

// --- the entry point --------------------------------------------------------

/// `def main():` — an entry point that returns `()` and cannot fail.
///
/// `script-mode.md` §2.3 fixes the *script body*'s signature at `-> Error?`,
/// and a file that writes `def main():` out by hand returns `()`. The C `main`
/// is then the call and `science_exit(0)` with no branch, because the failing
/// row of §2.3's table is unreachable by construction.
#[test]
fn a_main_that_returns_unit_exits_zero() {
    let source = "def main():\n    print(1)\n";
    let dir = scratch("methods", "unitmain");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "unitmain"), OptLevel::O0);
    let ran = run(&built);
    let text = built.ir.clone();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.stdout, "1\n");
    assert_eq!(ran.status, Some(0));
    // The shape, pinned: one call to the Science entry and one to
    // `science_exit`, and **no** load of a hidden return slot, because there is
    // no `Error?` to test.
    assert!(text.contains("define i32 @main()"), "{text}");
    assert!(text.contains("call void @_S4main()"), "{text}");
}

// --- a `match` whose scrutinee is a borrow ----------------------------------

/// `match` on a `borrowed` choice reads the referent's tag.
///
/// `def name_of(format: borrowed Format)` is how the corpus spells it every
/// time. MIR used to emit `discriminant(_1)` with `_1` holding the *reference*,
/// which this crate refused as *"a discriminant read of a value that is not a
/// `choice`"* — a message about a type, for a missing dereference.
#[test]
fn a_match_on_a_borrowed_choice_reads_the_referent() {
    let source = "\
choice Format:
    Plain
    Markdown

def name_of(format: &Format) -> Int:
    match format:
        Plain: 10
        Markdown: 20

def main():
    print(name_of(Plain))
    print(name_of(Markdown))
";
    assert_eq!(prints("matchborrow", source), "10\n20\n");
}

// --- what is still refused, and the refusal names the case -------------------

/// The default body of a method an `interface` declares, run at two
/// different implementors.
///
/// **This test used to assert the refusal** — *"a call to `twice`, the
/// default body a method declared on `interface Summarize` carries: its
/// `self` is `Self`, which is a different concrete type in every
/// implementation …"* — and the refusal is what
/// [`science_codegen::mono::Instance::self_ty`] replaced: `Mono::solve_call`
/// redirects a call to a method `interface` declares to the definition the
/// receiver's concrete type actually answers with, and binds `Self` to that
/// receiver as a second axis of the instance's identity, beside its ordinary
/// generic arguments. One body becomes one function per implementor, which
/// is the fix the old refusal's own message named.
///
/// **Two implementors and not one**, so a compiler that folded both onto one
/// shared function would print the wrong number for the second: `Doc`'s
/// `size` is `1` and `Row`'s is `10`, so `twice` — `size() + size()`,
/// inherited unwritten by both — is `2` for one and `20` for the other only
/// if each call reaches its *own* `size`.
#[test]
fn a_default_body_runs_and_differs_per_implementor() {
    let source = "\
interface Summarize:
    def size(self) -> Int

    def twice(self) -> Int:
        self.size() + self.size()

type Doc:
    n: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

type Row:
    n: Int

Row implements Summarize:
    def size(self) -> Int:
        self.n

def main():
    print(Doc(n: 1).twice())
    print(Row(n: 10).twice())
";
    assert_eq!(prints("interface-default", source), "2\n20\n");
}

/// Decision 13's dispatch, run: a call through `any I` reaches the
/// implementation the receiver was made from.
///
/// **This test used to assert the refusal** — *"a call to `size` … Decision 13
/// dispatches it through a vtable and this backend emits none"* — and the
/// refusal is what `Lowerer::lower_dispatch` replaced.
///
/// **`409` for `lower_dispatch`'s own version of this file's reason.** A
/// dispatch is three loads before the call — the data word, the vtable word,
/// the slot — and every one of them is a pointer that opaque pointers make
/// indistinguishable from the pointer beside it. A receiver read out of the
/// vtable word, or a function pointer read out of the data word, is a program
/// that verifies; one that reads the *address* of the pair instead of the pair
/// prints a number that is not 409, and a jump through the wrong word does not
/// come back at all.
#[test]
fn a_call_through_an_interface_object_dispatches_to_the_implementation() {
    let source = "\
interface Summarize:
    def size(self) -> Int

type Doc:
    n: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

def measure(it: &any Summarize) -> Int:
    it.size()

def main():
    print(measure(Doc(n: 409)))
";
    assert_eq!(prints("vtable", source), "409\n");
}

/// **Two implementations of one interface, dispatched through one call
/// site**, which is the property a vtable exists for and the one a direct
/// call cannot fake.
///
/// A backend that resolved `size` statically — to whichever implementation it
/// met first — passes the test above and fails this one: the two answers
/// would be the same number twice. `11` and `22` rather than `1` and `2` so
/// that a slot read off by one, or a table shared between the two types,
/// prints something no arithmetic on the right answer produces.
#[test]
fn two_implementations_reach_two_bodies_through_one_call_site() {
    let source = "\
interface Summarize:
    def size(self) -> Int

type Doc:
    n: Int

type Note:
    m: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

Note implements Summarize:
    def size(self) -> Int:
        self.m

def measure(it: &any Summarize) -> Int:
    it.size()

def main():
    print(measure(Doc(n: 11)))
    print(measure(Note(m: 22)))
";
    assert_eq!(prints("vtable_two", source), "11\n22\n");
}

/// The second slot of a two-method interface, which is what says the index is
/// the method's position and not always zero.
///
/// **One table, two slots, and the call sites differ only in which one they
/// read.** A dispatch that ignored the index — or computed it from the
/// implementation block's order rather than the interface's — would print the
/// same number twice here, and `Doc implements` deliberately writes its two
/// methods in the *opposite* order to the interface's declaration so that a
/// table filled from the block would swap them.
#[test]
fn the_slot_index_is_the_methods_position_in_the_interface() {
    let source = "\
interface Pair:
    def first(self) -> Int
    def second(self) -> Int

type Doc:
    a: Int
    b: Int

Doc implements Pair:
    def second(self) -> Int:
        self.b

    def first(self) -> Int:
        self.a

def take_first(it: &any Pair) -> Int:
    it.first()

def take_second(it: &any Pair) -> Int:
    it.second()

def main():
    let doc be Doc(a: 7, b: 9)
    print(take_first(doc))
    print(take_second(doc))
";
    assert_eq!(prints("vtable_slots", source), "7\n9\n");
}

/// Decision 12's glue, run: a record that owns a `String` is built, used and
/// dropped.
///
/// **What a wrong answer looks like here, and why the assertion is the exit
/// status.** The glue releases the `String` the record owns. Emitting none
/// leaks — invisible in a program this size. Emitting one that frees the
/// wrong offset, or frees twice, is `science_dealloc` on a pointer the
/// allocator did not give out, which aborts: the status is `Some(0)` only if
/// the release happened exactly once at exactly the right address. The `3` is
/// the other half — it says the record's *other* field is still readable
/// after the glue was emitted for it, so a glue that walked the wrong field
/// list would be caught by the number rather than by the crash.
#[test]
fn a_record_that_owns_a_string_is_dropped_through_its_glue() {
    let source = "\
type Doc:
    title: String
    n: I64

def main():
    let text be \"hola\"
    let doc be Doc(title: text, n: 3)
    print(f\"{doc.n}\")
";
    assert_eq!(prints("glue", source), "3\n");
}

/// Glue recurses, and it releases in **reverse declaration order**.
///
/// `science_codegen::descriptor::drop_glue` fixes that order and nothing in
/// F0 can observe it — a `String`'s release has no side effect a program can
/// see — so what this asserts is that the nested record's *own* glue ran at
/// all: `Outer` owns no `String` directly, and a walk that only looked one
/// level deep would emit nothing for it, leak both strings and still exit 0
/// with `5` on stdout. What makes that visible is the third string: if
/// `Inner`'s glue is missing, `Outer`'s is too, and `Inner` is then dropped
/// by nothing at either level.
#[test]
fn glue_recurses_into_a_nested_record() {
    let source = "\
type Inner:
    a: String
    b: String

type Outer:
    n: I64
    inner: Inner

def main():
    let x be \"one\"
    let y be \"two\"
    let inner be Inner(a: x, b: y)
    let outer be Outer(n: 5, inner: inner)
    print(f\"{outer.n}\")
";
    assert_eq!(prints("glue_nested", source), "5\n");
}

/// Decision 14's box, run: a concrete value moved to the heap, reached again
/// through the vtable, and read back.
///
/// **`409` is the whole assertion and it is about the allocation.**
/// `science_box_new` copies `info.size` bytes from the value's address into a
/// fresh allocation, so a descriptor with the wrong size copies the wrong
/// number of bytes and a data word built from the wrong pointer reads
/// whatever the heap held. Either way the number that comes back out through
/// `message` is not the number that went in — and the failing edge of §2.3
/// below it would still be taken, which is why the *number* is asserted and
/// not just the exit status.
///
/// The program returns the error rather than binding it to a local that goes
/// out of scope, because dropping an interface object goes through a vtable
/// slot this backend does not emit; `lower_box`'s own note is the account of
/// what that costs.
#[test]
fn a_boxed_error_is_reached_through_its_vtable_and_read_back() {
    let source = "\
type Boom:
    code: I64

Boom implements Error:
    def message(self) -> String:
        f\"boom {self.code}\"

def fail() -> Error?:
    Boom(code: 409)

def report(err: &any Error) -> String:
    err.message()

def main() -> Error?:
    let err be fail()
    if err?:
        print(report(err))
    err
";
    let dir = scratch("methods", "boxed");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "boxed"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.stdout, "boom 409\n", "stderr: {}", ran.stderr);
    // §2.3's fourth row, reached from a Science program rather than from a
    // hand-written `_S4main`: see `tests/exit_code.rs`.
    assert_eq!(ran.status, Some(1), "stderr: {}", ran.stderr);
}

/// §2.6's plain `Box[T]`, not Decision 14's boxed error above: built,
/// dropped, and run enough times that a wrong size or a wrong drop order
/// would corrupt the allocator rather than merely disagree with a number.
///
/// **Why a loop and not one call.** The boxed error test above reads `409`
/// back through the vtable and catches a wrong copy that way; a plain
/// `Box[T]` has no such read-back today — `Lowerer::box_element`'s reason is
/// the same one method lookup has for not looking through a `Box` at all,
/// and `crates/science-types/tests/method_lookup.rs`'s
/// `a_method_through_a_box_is_open_because_no_note_says_otherwise` names the
/// undecided rule. So this test's evidence is different in kind: `Holder`
/// owns a `String`, and the box `n` varies with the loop counter rather than
/// being a compile-time constant, which is what stops the allocator from
/// ever handing back the same bytes twice by accident. Ten thousand rounds
/// of allocate-then-free is enough that a wrong `size`, a wrong `align`, or
/// a `drop_fn` called in the wrong order corrupts the heap and the process
/// aborts before `print` ever runs — this crate has no leak detector to run
/// under, so a clean exit and the right stdout is the evidence available to
/// it, and `/tmp`'s own manual run of the same shape under macOS's `leaks`
/// during development read *"0 leaks for 0 total leaked bytes"* for the
/// stronger claim this test cannot make itself.
#[test]
fn a_boxed_value_owning_a_string_is_built_and_freed_ten_thousand_times() {
    let source = "\
type Holder:
    n: I64
    label: String

def make(n: I64) -> Box[Holder]:
    Box.new(Holder(n: n, label: \"boxed and owned\"))

def main():
    for i in 0..10000:
        let h be make(i)
    print(\"done\")
";
    let dir = scratch("methods", "boxed_value");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "boxed_value"), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.stdout, "done\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
}

/// The vtable is a `private unnamed_addr`-free constant array of pointers, and
/// the dispatch loads out of it rather than calling a symbol.
///
/// The shape assertions sit beside the execution tests above for this file's
/// stated reason: they pin *how* it is done, and the programs above pin that
/// what it does is right.
#[test]
fn the_dispatch_loads_a_slot_and_calls_through_it() {
    let source = "\
interface Summarize:
    def size(self) -> Int

type Doc:
    n: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

def measure(it: &any Summarize) -> Int:
    it.size()

def main():
    print(measure(Doc(n: 1)))
";
    let text = ir("vtable_ir", source);
    assert!(
        text.contains("x ptr] ["),
        "the vtable is not a constant array of pointers:\n{text}"
    );
    assert!(
        text.contains(".vtable."),
        "no vtable global was emitted:\n{text}"
    );
    // The call is through a loaded value, not a named function: `size` has a
    // definition in this module, so a backend that called it directly would
    // also produce a working program — and would not be dispatching.
    assert!(
        text.contains("call i64 %"),
        "the dispatch is not a call through a value:\n{text}"
    );
}

/// A method on a **generic** block runs.
///
/// **This replaces `a_method_on_a_generic_block_is_refused_as_a_generic`,
/// which pinned a refusal that is no longer true.** That test asserted the
/// message contained *"monomorphised"*, and its reasoning was right for the
/// compiler it was written against: `Wrapper[T] has:` gives `Self` the
/// concrete-looking type `Wrapper[T]`, the interface-default check passed it
/// through, and `layout_of_ty` was what stopped it — because nothing could lay
/// out a generic aggregate at all.
///
/// Something can now. `Lowerer::aggregate_env` binds a generic record's
/// parameters to the arguments of each use and walks the declared field list
/// with the binding in hand, so `Wrapper[Int]` has a layout and the method has
/// a receiver. The refusal had nothing left to refuse.
///
/// The receiver is annotated because a bare `Wrapper(inner: 1)` still fails
/// **earlier**, in the checker, which does not infer a type argument from an
/// unannotated integer literal. That is a real gap and it is not this file's:
/// it is `science-types`' `instantiate_call`, and a test that left it in would
/// be asserting the checker's limit while claiming to be about methods.
#[test]
fn a_method_on_a_generic_block_runs() {
    let source = "\
type Wrapper[T]:
    inner: T

Wrapper[T] has:
    def get(self) -> Int:
        1

def main():
    let w: Wrapper[Int] be Wrapper(inner: 1)
    print(w.get())
";
    assert_eq!(prints("generic_method", source), "1\n");
}

// --- shape, where shape is the thing under test ------------------------------

/// The receiver is the **first** parameter and it is a pointer.
///
/// An IR assertion, and it stands for nothing behavioural: every test above is
/// a run. What it pins is the order, which `science-mir`'s `run` fixes —
/// *"the parameters, in the order the signature declared them, with the
/// receiver first"* — and which a call site and a definition have to agree
/// about without anything checking that they do.
#[test]
fn the_receiver_is_the_first_parameter() {
    let source = "\
type Doc:
    n: Int

Doc has:
    def at(self, offset: Int) -> Int:
        self.n + offset

def main():
    print(Doc(n: 1).at(2))
";
    let text = ir("order", source);
    let line = text
        .lines()
        .find(|line| line.contains("define") && line.contains("at"))
        .unwrap_or_else(|| panic!("no definition of `at`:\n{text}"));
    assert!(line.contains("(ptr "), "the receiver is a pointer: {line}");
    assert!(line.contains("i64 "), "and the declared parameter follows it: {line}");
}

// --- `is` and `is not` on a `String` -----------------------------------------

/// §4.6's one spelling of equality, on a type whose values are three words.
///
/// **The values are chosen so that a pointer comparison fails.** `a` and `b`
/// hold equal bytes in two different allocations: a comparison that reached
/// `icmp` on the `{ ptr, len, cap }` aggregate would answer *"is this the same
/// buffer"* — `false` — where the language asks *"is this the same text"*. Both
/// questions type-check, both verify, and only running it separates them.
#[test]
fn string_equality_compares_bytes_and_not_buffers() {
    let source = "\
def same(a: &String, b: &String) -> Bool:
    a is b

def main():
    let a be \"hola\"
    let b be \"hol\"
    let mutable c be b
    c.push_str(\"a\")
    print(same(a, c))
    print(same(a, b))
";
    assert_eq!(prints("stringeq", source), "true\nfalse\n");
}

/// `is not`, and a literal operand — which is Decision 15's temporary, built
/// and freed at the comparison.
///
/// Compared twice so that a missing `science_string_free` is a leak and a
/// premature one is a wrong answer on the second line; run at `-O2` so that the
/// optimiser has seen both.
#[test]
fn a_string_is_compared_against_a_literal() {
    let source = "\
def has_name(name: &String) -> Bool:
    name is not \"\"

def main():
    print(has_name(\"Kepler\"))
    print(has_name(\"\"))
    let s be \"Kepler\"
    print(has_name(s))
    print(s)
";
    assert_eq!(prints("stringlit", source), "true\nfalse\ntrue\nKepler\n");
}

/// `<` on a `String` is refused, and the refusal says why rather than reaching
/// for `science_string_cmp`.
#[test]
fn ordering_two_strings_is_refused() {
    let source = "\
def before(a: &String, b: &String) -> Bool:
    a < b

def main():
    print(before(\"a\", \"b\"))
";
    let text = refusal("stringord", source);
    assert!(text.contains("science_string_cmp"), "{text}");
    assert!(text.contains("Ord"), "{text}");
}

// --- `assign`'s §7: a `Copy` out of a borrow --------------------------------

/// An operator applied to an **exclusive** borrow of a scalar, **built, linked
/// and run** — and this test used to assert a refusal for it.
///
/// §3 finding 27 was that `science-types` wrapped a shared borrow of a scalar in
/// a `Coercion::Copy` before an operator saw it and did not wrap an exclusive
/// one, so `def bump(counter: mutable borrowed Int):` with the body `counter be
/// counter + 1` — `examples/01_functions.science`'s, and the only spelling §4.7
/// leaves — arrived here as a pointer. That hole was closed in `assign`'s §7,
/// and what stood behind it was this crate's, which is now
/// `Lowerer::copy_out_of_borrow`: one `ExtInst::LoadAt` through the pointer.
///
/// **`bump` is called three times and the count is printed once**, because a
/// copy that read the wrong slot has two failure modes that a single call
/// cannot separate. Reading the *address* of the parameter slot rather than the
/// value gives a large arbitrary number, which one call would also catch; but a
/// copy that read the caller's slot and a write that wrote the callee's would
/// print `0` after one call and `0` after three, and a copy that read a stale
/// value would print `1` after three. Only the sequence distinguishes them.
#[test]
fn a_copy_out_of_a_borrow_reads_the_value_behind_it() {
    let source = "def bump(counter: &mut Int):
    counter be counter + 1

def main():
    let mutable hits be 0
    bump(hits)
    bump(hits)
    bump(hits)
    print(hits)
";
    assert_eq!(prints("mutborrow", source), "3\n");
}

/// The same copy out of a **shared** borrow, at the two other positions §7
/// names: a block's tail and an operand of `+`.
///
/// `4294967303` is `2^32 + 7` and is the value the width is read at: a load
/// narrower than the referent prints `7`, and `7` is a number a reader would
/// not look at twice. The sum is over two borrows at once, so a copy that
/// loaded one operand and passed the other through as a pointer prints an
/// address rather than `4294967310`.
#[test]
fn a_copy_out_of_a_shared_borrow_is_the_value_at_its_own_width() {
    let source = "\
def read(value: &I64) -> I64:
    value

def sum(a: &I64, b: &I64) -> I64:
    a + b

def main():
    let big be 4294967303i64
    let seven be 7i64
    print(read(big))
    print(sum(big, seven))
";
    assert_eq!(prints("copy-i64", source), "4294967303\n4294967310\n");
}

/// The narrow widths, where a load of the wrong size is the whole hazard.
///
/// **`U8` is the one that cannot be got right by accident.** A `borrowed U8` is
/// a pointer to one byte, and the load LLVM emits is whatever type the layout
/// says — an eight-byte load from a one-byte slot verifies clean, which is §3
/// finding 12's shape in the other direction. `200` and `13` are adjacent
/// locals, so an over-wide load of either reads the other's bytes or the
/// frame's, and `200 + 13` at eight bits is `213` only if both loads were one
/// byte wide.
///
/// `Char` is a `u32` holding a scalar value and `Bool` is `i8` in memory with
/// `i1` in a register, which are the two scalars whose in-memory and in-register
/// forms differ at all.
#[test]
fn a_copy_out_of_a_borrow_at_the_narrow_widths() {
    let source = "\
def byte(value: &U8) -> U8:
    value

def add_bytes(a: &U8, b: &U8) -> U8:
    a + b

def letter(value: &Char) -> Char:
    value

def flag(value: &Bool) -> Bool:
    value

def main():
    let big be 200u8
    let small be 13u8
    let c be 'q'
    let yes be true
    print(byte(big))
    print(add_bytes(big, small))
    print(letter(c))
    print(flag(yes))
";
    assert_eq!(prints("copy-narrow", source), "200\n213\nq\ntrue\n");
}

/// And a float, which is the load that is not an integer load at all.
///
/// A `borrowed F64` read as an `i64` and printed would be `4612811918334230528`
/// for `2.5`; read as an `F32` it would be a different number of digits. The
/// sum is here so that the value reaches an *instruction* and not only a
/// formatter — `2.5 + 0.25` is `2.75` exactly in binary, so a wrong answer here
/// is a wrong answer and never a rounding question.
#[test]
fn a_copy_out_of_a_borrowed_float_is_a_float_load() {
    let source = "\
def read(value: &F64) -> F64:
    value

def add(a: &F64, b: &F64) -> F64:
    a + b

def main():
    let a be 2.5f64
    let b be 0.25f64
    print(read(a))
    print(add(a, b))
";
    assert_eq!(prints("copy-f64", source), "2.5\n2.75\n");
}

/// And a **record**, which is the load that is not a scalar load.
///
/// §7's rule is conditioned on `Copy` and not on being a scalar, and §4.4's
/// marker syntax — `Position implements Copy`, one line and no block, which
/// `examples/06_traits.science` writes — is how a user type acquires it. So a
/// `borrowed Pair` copies out as a sixteen-byte struct load, and the returned
/// `Pair` is two words, which §4's classifier returns **indirectly**: the
/// destination of the copy is finding 5's return slot rather than an `alloca`.
/// `400` and `9` are `tests/methods.rs`' own adversarial pair — a struct read
/// through one pointer too many gives the frame slot's address in both fields,
/// and neither of those numbers is small.
#[test]
fn a_copy_out_of_a_borrowed_record_is_an_aggregate_load() {
    let source = "\
type Pair:
    a: I64
    b: I64

Pair implements Copy

def whole(p: &Pair) -> Pair:
    p

def main():
    let p be Pair(a: 400, b: 9)
    let q be whole(p)
    print(q.a)
    print(q.b)
";
    assert_eq!(prints("copy-record", source), "400\n9\n");
}

/// `Coercion::CopyThenWiden`: §7's rule and then Decision 6's, in that order.
///
/// **The order is the whole content of the test.** `borrowed Int` into `Int?`
/// is a load and then a tagged store; the reverse would build a `(borrowed
/// Int)?` — a pointer with a null niche — and a `?` on it would be asking
/// whether the *borrow* was null, which is always false, and the payload read
/// back would be an address. So `256` is the value, for
/// `tests/past_stage_three.rs`'s reason: Decision 18 puts the tag at offset 0,
/// and a payload written there instead makes the tag the low byte, which is
/// zero for `256` and reads back as absent.
#[test]
fn a_copy_out_of_a_borrow_then_widened_is_present() {
    let source = "\
def maybe(value: &Int) -> Int?:
    value

def main():
    let big be 256
    let small be 42
    let a be maybe(big)
    let b be maybe(small)
    if a?:
        print(\"a is present\")
    else:
        print(\"a is absent\")
    if b?:
        print(\"b is present\")
    else:
        print(\"b is absent\")
";
    assert_eq!(prints("copy-widen", source), "a is present\nb is present\n");
}

/// And `examples/01_functions.science`, whole — the file this construct was the
/// last thing between and an executable.
///
/// This is that file verbatim, `bump` and all: recursion, early `return`, a
/// unit return type, two shared-borrow parameters, a returned borrow, a moved
/// `String`, `String.length()`, `is not` against a literal, `print` of an `Int`
/// and a `Bool`, and the `mutable borrowed` counter.
#[test]
fn the_functions_example_runs_whole() {
    let source = "\
def greet():
    print(\"hello\")

def greet_twice() -> ():
    greet()
    greet()

def add(a: Int, b: Int) -> Int:
    a + b

def clamp_low(value: Int, floor: Int) -> Int:
    if value < floor:
        return floor
    value

def longest(a: &String, b: &String) -> &String:
    if a.length() > b.length(): a else: b

def bump(counter: &mut Int):
    counter be counter + 1

def consume(text: String) -> Int:
    text.length()

def factorial(n: Int) -> Int:
    if n <= 1:
        1
    else:
        n * factorial(n - 1)

def has_name(name: &String) -> Bool:
    name is not \"\"

def main():
    greet()
    greet_twice()
    print(add(2, 3))
    print(clamp_low(-4, 0))
    print(factorial(10))
    print(has_name(\"Kepler\"))
    print(longest(\"abc\", \"de\"))

    let mutable hits be 0
    bump(hits)
    print(hits)

    let owned be \"a sentence\"
    print(consume(owned))
";
    assert_eq!(
        prints("functions-whole", source),
        "hello\nhello\nhello\n5\n0\n3628800\ntrue\nabc\n1\n10\n"
    );
}

/// The third of §7's variants, which is refused, and the refusal names Decision
/// 5 rather than a missing instruction.
///
/// `(borrowed T)?` into `T?` runs the copy *only when the value is present*,
/// which is a test and two edges: three basic blocks where MIR has one. Every
/// ingredient of it is in this crate — `lower_is_present` asks the question for
/// both of Decision 18's and 19's representations, `copy_out_of_borrow` is the
/// present edge and `store_null` is the absent one — and what is missing is the
/// place to put them, because a statement lowers into one block's instruction
/// list. The half that would fit, a load on the present edge and nothing on the
/// other, verifies and links and reads uninitialised memory.
///
/// **`unwrap` is called and the call is what makes this a test.**
/// `Lowerer::lower_crate` walks `reachable_from(bodies, main)` and lowers
/// nothing else, so the same file with the call removed *builds*: the body
/// carrying the refused coercion is never visited. A refusal test whose fixture
/// does not call the function it is about asserts nothing.
#[test]
fn a_copy_that_runs_only_when_present_is_refused() {
    let source = "\
def unwrap(value: (&Int)?) -> Int?:
    value

def main():
    let m be unwrap(null)
    if m?:
        print(\"present\")
    else:
        print(\"absent\")
";
    let text = refusal("copy-when-present", source);
    assert!(text.contains("only when the value is present"), "{text}");
    assert!(text.contains("Decision 5"), "the refusal names the line it is at: {text}");
}

/// The rest of `examples/01_functions.science` with its `mutable borrowed`
/// statement removed, kept beside the whole file above.
///
/// It is not redundant with [`the_functions_example_runs_whole`]: this one is
/// the program that ran before §7's copy was lowered, so a regression in the
/// copy alone fails one test and not both, which says which half moved.
#[test]
fn the_rest_of_the_functions_example_runs() {
    let source = "\
def greet():
    print(\"hello\")

def greet_twice() -> ():
    greet()
    greet()

def add(a: Int, b: Int) -> Int:
    a + b

def clamp_low(value: Int, floor: Int) -> Int:
    if value < floor:
        return floor
    value

def longest(a: &String, b: &String) -> &String:
    if a.length() > b.length(): a else: b

def consume(text: String) -> Int:
    text.length()

def factorial(n: Int) -> Int:
    if n <= 1:
        1
    else:
        n * factorial(n - 1)

def has_name(name: &String) -> Bool:
    name is not \"\"

def main():
    greet()
    greet_twice()
    print(add(2, 3))
    print(clamp_low(-4, 0))
    print(factorial(10))
    print(has_name(\"Kepler\"))
    print(longest(\"abc\", \"de\"))
    let owned be \"a sentence\"
    print(consume(owned))
";
    assert_eq!(
        prints("functions", source),
        "hello\nhello\nhello\n5\n0\n3628800\ntrue\nabc\n10\n"
    );
}

/// A default body, dispatched through `any I`, on two implementors —
/// [`a_default_body_runs_and_differs_per_implementor`]'s own case, moved from
/// a static call to a real one.
///
/// **This used to be a link error and not a refusal.** `science_codegen::mono`
/// did not model Decision 13's vtables at all, so `Lowerer::vtable_slots`
/// refused a defaulted method's slot outright — *"Decision 15's defaulted
/// method has to be monomorphised at the implementor's `Self` before it has
/// an address, and this backend monomorphises nothing"* — the message
/// `examples/00_kitchen_sink.science` and `examples/08_dyn_dispatch.science`
/// both hit. `science_codegen::mono::Mono::vtable_instances` is the repair:
/// the coercion that builds `&any Summarize` is where the concrete type is
/// still in hand, so that is where the instance a defaulted slot needs is
/// built and enqueued, the same `Instance::self_ty` axis a static call already
/// used.
///
/// **One call site, two answers, so a shared function is caught.** `dispatch`
/// is the only place `twice` is called anywhere in this program — there is no
/// static call to redirect from, which is exactly the shape
/// `Lowerer::lower_crate`'s own comment names: *"a method reached only through
/// a vtable is absent from the set, and the symptom is `define_vtable` naming
/// a symbol nothing defined, and the linker saying so"*. `11 + 11` and
/// `22 + 22` are chosen so that a table sharing one slot between `Doc` and
/// `Row`, or a slot resolved by definition alone and not by receiver, prints
/// `22` twice rather than `22` and `44`.
#[test]
fn a_default_body_dispatched_through_any_i_differs_per_implementor() {
    let source = "\
interface Summarize:
    def size(self) -> Int

    def twice(self) -> Int:
        self.size() + self.size()

type Doc:
    n: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

type Row:
    n: Int

Row implements Summarize:
    def size(self) -> Int:
        self.n

def dispatch(it: &any Summarize) -> Int:
    it.twice()

def main():
    let doc be Doc(n: 11)
    let row be Row(n: 22)
    print(dispatch(doc))
    print(dispatch(row))
";
    assert_eq!(prints("vtable_default", source), "22\n44\n");
}

/// The same default body, reached **both** statically and through `any I`,
/// from one program — proving the two paths land on one instance and not
/// two.
///
/// `describe[T: Summarize]` is `a_default_body_runs_and_differs_per_
/// implementor`'s static path, monomorphised at `T = Doc`;
/// `dispatch(it: &any Summarize)` is the vtable path, run at `Doc` and at
/// `Row`. `describe(Doc(n: 11))` and `dispatch(Doc(n: 11))` printing the same
/// number is the property this pins: if the static call and the vtable slot
/// disagreed about which symbol `Doc`'s `twice` is, the module would carry two
/// definitions of it, and `Lowerer::lower_crate`'s own symbol-collision check
/// — finding 25's guard — would refuse the build rather than let the two
/// silently pick different addresses.
#[test]
fn a_default_body_reached_statically_and_through_any_i_is_one_instance() {
    let source = "\
interface Summarize:
    def size(self) -> Int

    def twice(self) -> Int:
        self.size() + self.size()

type Doc:
    n: Int

Doc implements Summarize:
    def size(self) -> Int:
        self.n

type Row:
    n: Int

Row implements Summarize:
    def size(self) -> Int:
        self.n

def describe[T: Summarize](value: &T) -> Int:
    value.twice()

def dispatch(it: &any Summarize) -> Int:
    it.twice()

def main():
    let doc be Doc(n: 11)
    let row be Row(n: 22)
    print(describe(doc))
    print(dispatch(doc))
    print(dispatch(row))
";
    assert_eq!(prints("vtable_default_shared", source), "22\n22\n44\n");
}

// --- a receiver behind a narrowed, owning `T?` -----------------------------

/// A method receiver reached through a narrowed `Record?`, where the record
/// owns a `String`.
///
/// `h.describe()` inside `if h?:` is `lower_method_call`'s §9 receiver path —
/// the same `borrow_source` a plain `&expr` and `print(x)`'s call-site
/// auto-borrow use — and `h`'s declared type is `Holder?`, laid out as
/// Decision 18's discriminant and payload because `Holder` is not a borrow.
/// Before `Projection::Payload`, borrowing `h` unchanged handed `describe`'s
/// `self` the *option's* address — tag and all — read as a `Holder`, which
/// is `title`'s field offset landing on whatever bytes follow the
/// discriminant. Wrong output or a crash is what a wrong offset looks like
/// here; the exact string is what says the offset is now right.
#[test]
fn a_narrowed_record_that_owns_a_string_is_borrowed_by_its_method() {
    let source = "\
type Holder:
    title: String

Holder has:
    def describe(self) -> String:
        copy_of(self.title)

def copy_of(text: &String) -> String:
    let mutable out be String.new()
    out.push_str(text)
    out

def use_it(h: Holder?):
    if h?:
        print(h.describe())

def main():
    use_it(Holder(title: \"kepler\"))
";
    assert_eq!(prints("narrowed-record-receiver", source), "kepler\n");
}

/// The same shape one level indirect: the narrowed option's payload is a
/// `choice`, and the `choice`'s own variant owns the `String` — two
/// projections deep, `Payload` and then `Downcast`, over the one borrow.
#[test]
fn a_narrowed_choice_that_owns_a_string_is_borrowed_by_its_method() {
    let source = "\
choice Boxed:
    Has(String)
    Empty

Boxed has:
    def describe(self) -> String:
        match self:
            Has(text): copy_of(text)
            Empty: \"empty\"

def copy_of(text: &String) -> String:
    let mutable out be String.new()
    out.push_str(text)
    out

def use_it(b: Boxed?):
    if b?:
        print(b.describe())

def main():
    use_it(Has(\"orbit\"))
    use_it(Empty)
";
    assert_eq!(prints("narrowed-choice-receiver", source), "orbit\nempty\n");
}

/// **The silent miscompile the output ratchet found in
/// `examples/18_ownership.science`'s `disjoint_fields`, reduced to one
/// field.** `via_shared` and `direct` always printed the field's real
/// length; `via_mut` printed `0` — the same field, read the same way,
/// differing only in whether the record itself arrived by `&Doc` or
/// `&mut Doc`.
///
/// **Cause, traced to the type checker.** `crate::check`'s
/// `ExprKind::Borrowed` arm collapses an explicit `&place` into a reborrow of
/// `place`'s own referent when `place` is already ergonomically a borrow —
/// Decision 27 widens `doc.title` itself to `&mut String` before the
/// explicit `&` is even applied, when `doc` is `&mut Doc`. The collapse used
/// to fire only when the two mutabilities matched exactly, so a *shared* `&`
/// over an ergonomically-`&mut` field fell to the `_` arm and kept the whole
/// `&mut String` as the referent, typing `t` as `&(&mut String)` — a double
/// borrow neither `let t be &doc.title` nor any type in this program asked
/// for. `science-mir`'s method-call auto-deref trusted that type and peeled
/// two layers of `Deref` for `t.length()` where one was correct, reading past
/// the field into whatever followed it in memory — which is why it read as
/// an empty `String` rather than refusing to build.
///
/// Fixed by collapsing whenever the explicit borrow is shared, regardless of
/// what the field's own ergonomic mutability is: asking for less than a
/// field already grants is always sound.
#[test]
fn a_shared_borrow_of_a_field_through_an_exclusive_record_borrow_reads_the_real_value() {
    let source = "\
type Doc:
    title: String

def via_shared(doc: &Doc) -> Int:
    let t be &doc.title
    t.length()

def via_mut(doc: &mut Doc) -> Int:
    let t be &doc.title
    t.length()

def direct(doc: &Doc) -> Int:
    doc.title.length()

def main():
    let mutable doc be Doc(title: \"abcd\")
    print(via_shared(doc))
    print(via_mut(doc))
    print(direct(doc))
";
    assert_eq!(prints("shared-field-through-mut-record", source), "4\n4\n4\n");
}

/// The same collapse, one field type over: an `Array` field rather than a
/// `String`, so the fix is not a `String`-specific accident of
/// `needs_drop`'s own table.
#[test]
fn a_shared_borrow_of_an_array_field_through_an_exclusive_record_borrow_reads_the_real_value() {
    let source = "\
type Bag:
    items: Array[Int]

def via_shared(bag: &Bag) -> Int:
    let items be &bag.items
    items.length()

def via_mut(bag: &mut Bag) -> Int:
    let items be &bag.items
    items.length()

def direct(bag: &Bag) -> Int:
    bag.items.length()

def main():
    let mutable bag be Bag(items: [1, 2, 3, 4, 5])
    print(via_shared(bag))
    print(via_mut(bag))
    print(direct(bag))
";
    assert_eq!(prints("shared-array-field-through-mut-record", source), "5\n5\n5\n");
}

/// **The other direction, to show the fix did not make an exclusive borrow
/// read-only by accident.** `&mut doc.title`'s reborrow collapses through the
/// identical `ExprKind::Borrowed` arm — `inner_mutable == *mutable` still
/// matches when both are exclusive — and a write through the result must
/// still reach the record `doc` itself points at, not a copy.
#[test]
fn an_exclusive_borrow_of_a_field_through_an_exclusive_record_borrow_still_writes_through() {
    let source = "\
type Doc:
    title: String

def clear_via_mut(doc: &mut Doc) -> Int:
    let mutable t be &mut doc.title
    t.truncate(0)
    t.length()

def main():
    let mutable doc be Doc(title: \"abcd\")
    print(clear_via_mut(doc))
    print(doc.title.length())
";
    assert_eq!(prints("write-through-mut-field-reborrow", source), "0\n0\n");
}

/// **An aggregate argument that is a field, and a field of a field.**
///
/// Decision 22 passes an aggregate by pointer to a caller-owned slot, and
/// `lower_science_call` used to insist that the slot be a whole local: a
/// `Move` with any projection on it was refused outright, so `take(o.inner.a)`
/// — an ordinary thing to write — could not be built at all. A field the
/// caller owns *is* a caller-owned slot, and the move is what says nothing
/// else will read it again.
///
/// Both depths are here because they take the same path for different
/// reasons: one `Field` step is the shape `science-mir`'s move analysis
/// tracks exactly, and two is the shape it rounds off (see `moves.rs`'
/// `move_event`). The value that arrives must be right either way, and the
/// number is the whole test — a wrong address here hands the callee a
/// neighbouring field and exits 0.
#[test]
fn a_field_and_a_field_of_a_field_can_be_moved_into_a_call() {
    let source = "\
type Inner:
    a: String
    b: String

type Outer:
    inner: Inner
    tag: String

def take(s: String) -> Int:
    s.length()

def main():
    let flat be Inner(a: \"hello\", b: \"wo\")
    print(take(flat.a))
    let nested be Outer(inner: Inner(a: \"hello\", b: \"wo\"), tag: \"t\")
    print(take(nested.inner.a))
    let other be Outer(inner: Inner(a: \"hello\", b: \"wo\"), tag: \"t\")
    print(take(other.inner.b))
";
    assert_eq!(prints("move-a-field-into-a-call", source), "5\n5\n2\n");
}

/// **`?` on a field, Decision 18's tagged representation.**
///
/// `lower_is_present` used to insist on an empty projection and refuse
/// everything else — `over.host?` where `over` is a record and `host` is a
/// `String?` — with SC0400's *"the tag and the niche are read from a local's
/// own address and this instruction set has no typed load from a computed
/// one"*. `String?` is a `choice` with no niche (Decision 19 has none for
/// `String`), so its layout is Decision 18's tagged pair, and the fix reads
/// [`Lowerer::lower_discriminant`]'s own trick — the tag sits at offset 0
/// always, so `place_address`'s computed pointer plus `ExtInst::LoadAt` at the
/// tag's own width reads it with no new instruction.
///
/// **The value is asserted, not just the exit code.** A presence test that
/// answers `true` for an absent field still exits 0 and prints something —
/// the bug this test would have caught is a `?` that always reads whatever
/// garbage sits at the local's *own* address instead of the field's, which
/// looks fine until the field is compared against a sibling that also has
/// one. So this builds one record with the field present and a second with
/// it null, and both branches of the `if` are exercised.
#[test]
fn a_tagged_options_field_answers_its_own_presence_test() {
    let source = "\
type Rec:
    host: String?

def main():
    let r be Rec(host: \"yes\")
    if r.host?:
        print(\"present\")
    else:
        print(\"absent\")
    let n be Rec(host: null)
    if n.host?:
        print(\"present\")
    else:
        print(\"absent\")
";
    assert_eq!(prints("tagged-field-presence", source), "present\nabsent\n");
}

/// **`?` on a field, Decision 19's niched representation.**
///
/// The same refusal, the other of Decision 6's two representations: `(&T)?`
/// has no discriminant at all, so a niched field's `?` is a comparison of the
/// niche's own pointer-sized scalar against `null`, read at a computed
/// address the same way — `owned_nullable_return`'s `BranchAndMaterialiseNull`
/// arm already builds the same scalar layout
/// (`layout_of(self.target, &CgTy::Ptr(PtrKind::Raw))`) for the same reason.
/// Fixing only the tagged path and leaving this one refused would have been
/// half the bug: the two representations take different branches inside
/// `lower_is_present` and one compiling is no evidence the other does.
#[test]
fn a_niched_options_field_answers_its_own_presence_test() {
    let source = "\
type Rec:
    reference: (&Int)?

def main():
    let x be 5
    let r be Rec(reference: &x)
    if r.reference?:
        print(\"present\")
    else:
        print(\"absent\")
    let n be Rec(reference: null)
    if n.reference?:
        print(\"present\")
    else:
        print(\"absent\")
";
    assert_eq!(prints("niched-field-presence", source), "present\nabsent\n");
}
