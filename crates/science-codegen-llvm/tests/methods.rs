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
//! What is left of it is one block: the **default body** of a method an
//! `interface` declares, whose `self` really is a `Self` and which needs one
//! copy per implementor. That is monomorphisation, it is above Decision 42's
//! line, and [`interface_default_bodies_are_refused`] is the refusal.
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
def longest(a: borrowed String, b: borrowed String) -> borrowed String:
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
def show(s: borrowed String):
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

def name_of(format: borrowed Format) -> Int:
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

/// The default body of a method an `interface` declares.
///
/// One body, one `Self`, and one concrete type per implementor: that is a
/// monomorphisation and Decision 42 puts the walk above this crate.
#[test]
fn interface_default_bodies_are_refused() {
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

def main():
    print(Doc(n: 1).twice())
";
    let text = refusal("default", source);
    assert!(text.contains("`interface Summarize`"), "{text}");
    assert!(text.contains("monomorphisation"), "{text}");
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

def measure(it: borrowed any Summarize) -> Int:
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

def measure(it: borrowed any Summarize) -> Int:
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

def take_first(it: borrowed any Pair) -> Int:
    it.first()

def take_second(it: borrowed any Pair) -> Int:
    it.second()

def main():
    let doc be Doc(a: 7, b: 9)
    print(take_first(doc))
    print(take_second(doc))
";
    assert_eq!(prints("vtable_slots", source), "7\n9\n");
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

def measure(it: borrowed any Summarize) -> Int:
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

/// A method on a **generic** block is still a refusal, and it names the
/// parameter rather than the method.
///
/// `Grid of T has:` gives `Self` the concrete-looking type `Grid of T`, so the
/// interface-default check passes it through and `layout_of_ty` is what stops
/// it — with the same message a generic free function gets, which is the right
/// one: what is missing is the monomorphisation walk and not anything about
/// methods.
#[test]
fn a_method_on_a_generic_block_is_refused_as_a_generic() {
    let source = "\
type Wrapper of T:
    inner: T

Wrapper of T has:
    def get(self) -> Int:
        1

def main():
    print(Wrapper(inner: 1).get())
";
    let text = refusal("generic", source);
    assert!(text.contains("monomorphised"), "{text}");
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
def same(a: borrowed String, b: borrowed String) -> Bool:
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
def has_name(name: borrowed String) -> Bool:
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
def before(a: borrowed String, b: borrowed String) -> Bool:
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
    let source = "def bump(counter: mutable borrowed Int):
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
def read(value: borrowed I64) -> I64:
    value

def sum(a: borrowed I64, b: borrowed I64) -> I64:
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
def byte(value: borrowed U8) -> U8:
    value

def add_bytes(a: borrowed U8, b: borrowed U8) -> U8:
    a + b

def letter(value: borrowed Char) -> Char:
    value

def flag(value: borrowed Bool) -> Bool:
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
def read(value: borrowed F64) -> F64:
    value

def add(a: borrowed F64, b: borrowed F64) -> F64:
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

def whole(p: borrowed Pair) -> Pair:
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
def maybe(value: borrowed Int) -> Int?:
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

def longest(a: borrowed String, b: borrowed String) -> borrowed String:
    if a.length() > b.length(): a else: b

def bump(counter: mutable borrowed Int):
    counter be counter + 1

def consume(text: String) -> Int:
    text.length()

def factorial(n: Int) -> Int:
    if n <= 1:
        1
    else:
        n * factorial(n - 1)

def has_name(name: borrowed String) -> Bool:
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
def unwrap(value: (borrowed Int)?) -> Int?:
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

def longest(a: borrowed String, b: borrowed String) -> borrowed String:
    if a.length() > b.length(): a else: b

def consume(text: String) -> Int:
    text.length()

def factorial(n: Int) -> Int:
    if n <= 1:
        1
    else:
        n * factorial(n - 1)

def has_name(name: borrowed String) -> Bool:
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
