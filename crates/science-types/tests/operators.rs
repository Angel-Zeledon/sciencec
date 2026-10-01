//! Two language decisions, and the gap that makes them matter.
//!
//! - **`assign`'s §7** — a borrow of a `Copy` type is assignable to the value.
//! - **`builtins.rs`' `Array of T implements Iterate: type Item is borrowed
//!   T`** — `collections-and-chains.md` §4.
//! - **`check`'s §6, closed** — `a + b`, `-a` and `a[i]` dispatch to the
//!   operator interfaces of §5.4.
//!
//! Every test here goes through the whole pipeline, for `tests/support`'s
//! reason: *"the contract of the checking layer is what the author wrote
//! checks"*, and a coercion that only fires on a hand-built type is a coercion
//! no program reaches. It also means the **prelude is real**, which these three
//! decisions need in a way the older fixtures do not: §7's rule is conditioned
//! on `Copy`, and `Copy` is a prelude interface with prelude implementations.
//!
//! **Each decision is tested by name and each is tested negatively**, because
//! all three are rules that say yes, and a rule that says yes is only as good
//! as what it still says no to. The negatives are the ones to read first: a
//! non-`Copy` borrow does not read as a value, an exclusive borrow does not
//! either, a `Copy` *bound* does not license the read, and an operator on a
//! type that implements nothing is refused rather than quietly typed.

mod support;

use science_types::assign::Coercion;
use science_types::thir::ExprKind;

/// Every `Coercion` the body of `name` emits, in order.
fn coercions(checked: &support::Checked, name: &str) -> Vec<Coercion> {
    checked
        .body(name)
        .exprs()
        .filter_map(|(_, expr)| match expr.kind {
            ExprKind::Coerce { coercion, .. } => Some(coercion),
            _ => None,
        })
        .collect()
}

/// The type of the first node of `name`'s body matching a predicate.
fn ty_of(
    checked: &support::Checked,
    name: &str,
    mut matches: impl FnMut(&ExprKind) -> bool,
) -> String {
    let id = checked.find(name, &mut matches);
    checked.render(checked.body(name).ty(id))
}

// --- Decision 1: a borrow of a `Copy` type reads as a value ---------------

/// `assign`'s §7, at its simplest: the rule, and the node it leaves behind.
#[test]
fn a_borrowed_copy_reads_as_a_value() {
    let checked = support::check(
        "\
def read(value: &I64) -> I64:
    value
",
    );
    checked.assert_clean();
    assert_eq!(coercions(&checked, "read"), vec![Coercion::Copy]);
}

/// The negative that matters most: `Copy` is the condition, not decoration.
///
/// `stdlib-core.md` §6.2 is explicit that `String` is `Clone` and not `Copy`
/// — *"an owned copy of a `String` allocates"* — so this is the exact case the
/// rule must keep refusing, and it is refused with the ordinary mismatch
/// rather than with a message about `Copy`.
#[test]
fn a_borrowed_string_does_not_read_as_a_value() {
    let checked = support::check(
        "\
def read(value: &String) -> String:
    value
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `String`, found `&String`"]);
}

/// The same question asked of a type the *program* declares, both ways round.
///
/// `Marker implements Copy` is the one-line marker form
/// `examples/06_traits.science` writes for `Vector2`; `Held` writes nothing.
#[test]
fn a_user_type_reads_out_of_a_borrow_only_when_it_implements_copy() {
    let checked = support::check(
        "\
type Marker:
    tag: I64

type Held:
    tag: I64

Marker implements Copy

def read_marker(value: &Marker) -> Marker:
    value

def read_held(value: &Held) -> Held:
    value
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `Held`, found `&Held`"]);
    assert_eq!(coercions(&checked, "read_marker"), vec![Coercion::Copy]);
}

/// §5's new bullet: the exclusive borrow is refused, and the message names it.
#[test]
fn an_exclusive_borrow_of_a_copy_does_not_read_as_a_value() {
    let checked = support::check(
        "\
def read(value: &mut I64) -> I64:
    value
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `&mut I64`"]);
}

/// §7's inversion of §3's discipline: `Methods::declares` refuses what it
/// cannot see, and a bound is something it cannot see.
///
/// This is a correct program that the rule says no to, and it is pinned here so
/// that the day `methods`' §5 looks through a bound the refusal has to be
/// deleted deliberately.
#[test]
fn a_copy_bound_on_a_type_parameter_does_not_license_the_read() {
    let checked = support::check(
        "\
def read[T: Copy](value: &T) -> T:
    value
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `T`, found `&T`"]);
}

/// `Coercion::CopyThenWiden`: §7's rule then Decision 6's, which is what a
/// `for` binding returned from a `-> T?` needs.
#[test]
fn a_borrowed_copy_reaches_a_nullable_of_the_value() {
    let checked = support::check(
        "\
def read(value: &I64) -> I64?:
    value
",
    );
    checked.assert_clean();
    assert_eq!(coercions(&checked, "read"), vec![Coercion::CopyThenWiden]);
}

/// `Coercion::CopyWhenPresent`: §2's one exception, and the shape `Array.get`
/// actually produces.
///
/// `stdlib-core.md` §3.6 declares `get` as `-> (borrowed T)?`, so this is the
/// line `examples/06_traits.science`'s `Letters.next` writes.
#[test]
fn a_nullable_borrow_of_a_copy_reaches_the_nullable_value() {
    let checked = support::check(
        "\
def first(values: &Array[Char]) -> Char?:
    values.get(0)
",
    );
    checked.assert_clean();
    assert_eq!(coercions(&checked, "first"), vec![Coercion::CopyWhenPresent]);
}

/// §2 holds everywhere else: the exception is `?` and nothing but `?`.
///
/// An `Array of (borrowed I64)` is not an `Array of I64` however `Copy` the
/// element is, because rewriting it costs one copy per element at an
/// assignment that looks free.
#[test]
fn the_copy_does_not_recurse_into_a_container() {
    let checked = support::check(
        "\
def read(values: Array[&I64]) -> Array[I64]:
    values
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(
        checked.messages(),
        vec!["expected `Array[I64]`, found `Array[&I64]`"]
    );
}

// --- Decision 2: `Array of T`'s `Item` is `borrowed T` --------------------

/// `collections-and-chains.md` §4.1: *"`docs.iterate()` — `Item = borrowed
/// Doc`"*, reached through §4.2's AMENDMENT 11, which makes `for x in xs:` that
/// call.
#[test]
fn a_for_over_an_array_binds_a_borrowed_element() {
    let checked = support::check(
        "\
type Doc:
    title: String

def walk(docs: &Array[Doc]):
    for doc in docs:
        let title be doc.title
",
    );
    checked.assert_clean();
    let body = checked.body("walk");
    let local = body
        .exprs()
        .find_map(|(_, expr)| match expr.kind {
            ExprKind::Field { .. } => Some(expr.ty),
            _ => None,
        })
        .expect("the body reads a field of the binding");
    // Decision 27 (`type-checking-and-mir.md` §7): `title` owns a `String`,
    // and `doc` is a borrow, so the field itself now reads as `&String`
    // rather than as the declared `String` — the same widening `check.rs`'s
    // `borrow_ergonomics` gives any field read through a borrow.
    assert_eq!(checked.render(local), "&String");
    assert_eq!(
        ty_of(&checked, "walk", |kind| matches!(kind, ExprKind::For { .. })),
        "()"
    );
}

/// **The two decisions holding each other up.** `Item is borrowed T` makes `n`
/// a `borrowed I64`, and `assign`'s §7 is the only reason `total + n` is still
/// a program. This is the line the whole pair exists for.
#[test]
fn a_loop_over_an_array_of_numbers_still_adds_up() {
    let checked = support::check(
        "\
def total(numbers: &Array[I64]) -> I64:
    let mutable total be 0
    for n in numbers:
        total be total + n
    total
",
    );
    checked.assert_clean();
    assert_eq!(coercions(&checked, "total"), vec![Coercion::Copy]);
    assert_eq!(
        ty_of(&checked, "total", |kind| matches!(kind, ExprKind::Binary { .. })),
        "I64"
    );
}

/// `String implements Add` and `+` concatenates (`stdlib-core.md` §6.3), so the
/// loop that used to be refused — a `borrowed String` against a `String` — is
/// the idiomatic join: the operands are only read, either may be a borrow, and
/// the sum is an owned `String`.
#[test]
fn a_loop_over_an_array_of_strings_concatenates() {
    let checked = support::check(
        "\
def joined(words: &Array[String], seed: String) -> String:
    let mutable out be seed
    for word in words:
        out be out + word
    out
",
    );
    checked.assert_clean();
}

/// `Map` is deliberately not an `Iterate`, and `builtins.rs` says why: §1.3 of
/// the collections note yields an `Entry of (K, V)` record that §8's closed
/// library does not contain. The binding stays `Ty::ERROR` and nothing is
/// reported, which is `check`'s §6 unchanged for this one container.
#[test]
fn a_for_over_a_map_still_binds_nothing() {
    let checked = support::check(
        "\
def walk(settings: &Map[String, String]):
    for entry in settings:
        let held be entry
",
    );
    checked.assert_clean();
}

/// `for` over a type implementing the author's **own** `interface Iterate:`
/// binds that block's `Item`.
///
/// **The `DefId` identity test used to refuse this, silently.**
/// `check`'s `iterate_item` asked whether the interface the `implements` block
/// named was the *prelude's* `Iterate`, and a program that declares its own —
/// `examples/00_kitchen_sink.science` does, to demonstrate associated types —
/// answered no. `for_expr` then bound the loop variable at `Ty::ERROR` with no
/// diagnostic, `sciencec check` exited 0, and the backend refused the program
/// with a message about a tuple.
///
/// `next` being resolved is the assertion, because that is the half
/// `science-mir` reads: a `For` with `next: None` is a loop with nothing to
/// call.
#[test]
fn a_for_over_a_user_redeclared_iterate_binds_its_item() {
    let checked = support::check(
        "\
interface Iterate:
    type Item
    def next(mutable self) -> Self.Item?

type Countdown:
    remaining: Int

Countdown implements Iterate:
    type Item is Int

    def next(mutable self) -> Self.Item?:
        if self.remaining <= 0:
            null
        else:
            self.remaining be self.remaining - 1
            self.remaining

def walk():
    for value in Countdown(remaining: 3):
        print(value)
",
    );
    checked.assert_clean();
    let resolved = checked
        .body("walk")
        .exprs()
        .any(|(_, expr)| matches!(expr.kind, ExprKind::For { next: Some(_), .. }));
    assert!(resolved, "the loop read no `next` off the implementation");
}

/// And the negative, which is the diagnostic that did not exist: a user type
/// with no `Iterate` at all.
///
/// **`SC0544` is the sentence `for_expr` used to leave unsaid.** The binding at
/// `Ty::ERROR` was the whole of what happened, and a front end that rejects a
/// program by leaving a hole and saying nothing hands the report to a phase
/// that can only describe its own limits.
#[test]
fn a_for_over_a_user_type_with_no_iterate_is_reported() {
    let checked = support::check(
        "\
type Bag:
    n: Int

def walk():
    for value in Bag(n: 3):
        print(value)
",
    );
    assert_eq!(checked.codes(), vec![544]);
    assert_eq!(checked.messages(), vec!["`Bag` cannot be walked by `for`"]);
}

/// The restraint the code is under, at the head that most needs it: a type
/// *parameter* is silent.
///
/// `Methods::receiver` cannot speak for a `TyKind::Param`, so *"`T` implements
/// no `Iterate`"* would be a claim about an instantiation nobody has made —
/// the same restraint `SC0532` and `SC0535` are under.
#[test]
fn a_for_over_a_type_parameter_is_still_silent() {
    let checked = support::check(
        "\
def walk[T](subject: T):
    for value in subject:
        let held be value
",
    );
    checked.assert_clean();
}

/// The regression guard for the declaration that was already there: `Chars`
/// answers `Item` with `Char` and not with a borrow of one.
#[test]
fn chars_still_binds_a_char_by_value() {
    let checked = support::check(
        "\
def spaces(text: &String) -> I64:
    let mutable seen be 0
    for c in text.chars():
        if c is ' ':
            seen be seen + 1
    seen
",
    );
    checked.assert_clean();
    // No copy-out: `Chars.Item` is a value already.
    assert_eq!(coercions(&checked, "spaces"), vec![]);
}

// --- §6, closed: the operators dispatch ----------------------------------

/// `a + b` is `a.add(b)`, and the method is the one the implementation block
/// wrote.
///
/// `type-checking-and-mir.md` §9.3 names the explicit form this desugars to —
/// *"the always-available explicit form (`a.item(k)`, `a.add(b)`)"* — and
/// `stdlib-shape-and-packages.md` §4.5 names the method.
#[test]
fn an_operator_on_a_user_type_resolves_to_its_interface_method() {
    let checked = support::check(
        "\
type Vector:
    x: F64

Vector implements Add:
    def add(self, other: Vector) -> Vector:
        Vector(x: self.x)

def sum(a: Vector, b: Vector) -> Vector:
    a + b
",
    );
    checked.assert_clean();
    let id = checked.find("sum", |kind| matches!(kind, ExprKind::MethodCall { .. }));
    let body = checked.body("sum");
    let ExprKind::MethodCall { method: Some(method), .. } = body.expr(id).kind else {
        panic!("`a + b` is a resolved method call");
    };
    assert_eq!(checked.krate.defs.get(method).name, "add");
    assert_eq!(checked.render(body.ty(id)), "Vector");
}

/// The negative: an operator on a type that implements nothing is `SC0535`,
/// and the message names the block the author would have to write.
#[test]
fn an_operator_on_a_type_implementing_nothing_is_refused() {
    let checked = support::check(
        "\
type Vector:
    x: F64

def sum(a: Vector, b: Vector) -> Vector:
    a + b
",
    );
    // One diagnostic and not two: the expression takes `Ty::ERROR`, which
    // `ty`'s §5 makes agree with the `-> Vector` it is returned at, so a
    // refused operator costs exactly one message.
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Vector` does not implement `Add`"]);
}

/// **The operand's type comes off the implementation and not off `Self`.**
///
/// `examples/06_traits.science` writes `Vector2 implements Mul: def mul(self,
/// scale: F64) -> Vector2` — scalar multiplication, whose right operand is not
/// the receiver's type. A prelude declaration of `def mul(self, other: Self)`
/// would report on that line, which is why `check`'s `operator` writes the
/// method's *name* and nothing else.
#[test]
fn the_operand_is_checked_against_the_implementation_it_dispatched_to() {
    let checked = support::check(
        "\
type Vector:
    x: F64

Vector implements Mul:
    def mul(self, scale: F64) -> Vector:
        Vector(x: self.x)

def scaled(v: Vector) -> Vector:
    v * 2.0
",
    );
    checked.assert_clean();
}

/// And the same implementation refuses the operand it does not take.
#[test]
fn the_operand_that_the_implementation_does_not_take_is_refused() {
    let checked = support::check(
        "\
type Vector:
    x: F64

Vector implements Mul:
    def mul(self, scale: F64) -> Vector:
        Vector(x: self.x)

def squared(v: Vector) -> Vector:
    v * v
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `F64`, found `Vector`"]);
}

/// Unary minus is `Neg.neg`, which takes no operand at all.
#[test]
fn unary_minus_dispatches_to_neg() {
    let checked = support::check(
        "\
type Counts:
    reads: I64

Counts implements Neg:
    def neg(self) -> Counts:
        Counts(reads: 0)

def flipped(c: Counts) -> Counts:
    -c
",
    );
    checked.assert_clean();
    let id = checked.find("flipped", |kind| matches!(kind, ExprKind::MethodCall { .. }));
    let ExprKind::MethodCall { method: Some(method), ref args, .. } =
        checked.body("flipped").expr(id).kind
    else {
        panic!("`-c` is a resolved method call");
    };
    assert_eq!(checked.krate.defs.get(method).name, "neg");
    assert!(args.is_empty(), "`neg` takes no operand");
}

/// A prelude numeric does not go through an implementation, and must not start
/// to: nothing declares `I64.add`, and `1 + 2` stays one `Binary` node for
/// `science-mir` to emit a machine instruction for.
#[test]
fn an_operator_on_a_prelude_numeric_is_still_structural() {
    let checked = support::check(
        "\
def sum(a: I64, b: I64) -> I64:
    a + b
",
    );
    checked.assert_clean();
    assert_eq!(ty_of(&checked, "sum", |kind| matches!(kind, ExprKind::Binary { .. })), "I64");
    assert!(
        !checked
            .body("sum")
            .exprs()
            .any(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. })),
        "`1 + 2` is not a method call"
    );
}

/// `indexing-and-array-literals.md` §1.1's Decision 2: `a[i]` is
/// `Index.index`, whose return is `borrowed Self.Output`.
///
/// The node stays an [`ExprKind::Index`] — it is a place — and what changed is
/// that it has a type and that the index operand is checked.
#[test]
fn indexing_dispatches_to_index_for_its_type() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements Index[I64]:
    def index(self, at: I64) -> &F64:
        &self.cell

def at(grid: &Grid) -> F64:
    grid[0]
",
    );
    checked.assert_clean();
    assert_eq!(
        ty_of(&checked, "at", |kind| matches!(kind, ExprKind::Index { .. })),
        "&F64"
    );
    // And §7 then reads the `F64` out of it, which is the whole point of
    // putting a type on this node.
    assert_eq!(coercions(&checked, "at"), vec![Coercion::Copy]);
}

/// The index operand is checked against the implementation's parameter.
#[test]
fn the_index_operand_is_checked() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements Index[I64]:
    def index(self, at: I64) -> &F64:
        &self.cell

def at(grid: &Grid, key: String) -> F64:
    grid[key]
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `String`"]);
}

/// And the negative: indexing a type that implements nothing.
#[test]
fn indexing_a_type_that_implements_nothing_is_refused() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

def at(grid: &Grid) -> F64:
    grid[0]
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Grid` does not implement `Index`"]);
}

// --- `Eq`, decided; `Ord`, left ------------------------------------------

/// **`Eq` is decided and `Ord` is not**, and this is the pair of tests that
/// says so.
///
/// `examples/06_traits.science` writes `Vector2 implements Eq: def eq(self,
/// other: Vector2) -> Bool`, so `eq` is a method name and a signature the
/// corpus already contains; requiring it invents nothing.
///
/// **The node is a `MethodCall` now, and the assertion below used to say
/// `Binary`.** It was pinning a bug: `check` accepted `a is b` on a user `Eq`,
/// `science-mir` passed the operator through, and `science-codegen-llvm`'s
/// `scalar_of` refused the record with `SC0400` — so the program the assertion
/// certified could not be built. See `is_on_a_user_eq_is_a_call_to_eq` below
/// for the running version.
#[test]
fn is_requires_eq() {
    let checked = support::check(
        "\
type Vector:
    x: F64

Vector implements Eq:
    def eq(self, other: Vector) -> Bool:
        true

type Held:
    x: F64

def same(a: Vector, b: Vector) -> Bool:
    a is b

def also(a: Held, b: Held) -> Bool:
    a is b
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Held` does not implement `Eq`"]);
    // `is` is the call, and it is a `Bool`.
    assert_eq!(ty_of(&checked, "same", |kind| matches!(kind, ExprKind::MethodCall { .. })), "Bool");
}

/// `a is b` on a user `Eq` is `a.eq(b)`, and `a is not b` is `not (a.eq(b))`.
///
/// **This is the regression test for the hole the assertion above used to
/// pin.** §5.4 — *"`Eq` and `Ord` are what §4.6's comparisons dispatch to"* —
/// and Decision 4c, which gives the method the interface's free lowercase name.
/// The `is not` half is two nodes where the author wrote one, and this phase is
/// the only one that can write it: `science-mir` has no method lookup to find
/// `eq` with.
#[test]
fn is_on_a_user_eq_is_a_call_to_eq() {
    let checked = support::check(
        "\
type Counts:
    reads: I64

Counts implements Eq:
    def eq(self, other: Counts) -> Bool:
        self.reads is other.reads

Counts implements Copy

def same(a: Counts, b: Counts) -> Bool:
    a is b

def different(a: Counts, b: Counts) -> Bool:
    a is not b
",
    );
    checked.assert_clean();
    // The receiver's own `self.reads is other.reads` is two `I64`s, which is
    // the structural comparison and stays a `Binary` — the dispatch is for the
    // heads this index can speak for and no others.
    assert_eq!(ty_of(&checked, "eq", |kind| matches!(kind, ExprKind::Binary { .. })), "Bool");
    assert_eq!(ty_of(&checked, "same", |kind| matches!(kind, ExprKind::MethodCall { .. })), "Bool");
    assert!(
        checked
            .body("same")
            .exprs()
            .all(|(_, expr)| !matches!(expr.kind, ExprKind::Binary { .. })),
        "`a is b` left a `Binary` behind"
    );
    // `is not` is the same call under a `not`.
    assert_eq!(
        ty_of(&checked, "different", |kind| matches!(kind, ExprKind::MethodCall { .. })),
        "Bool"
    );
    assert_eq!(
        ty_of(&checked, "different", |kind| matches!(kind, ExprKind::Unary { .. })),
        "Bool"
    );
}

/// And a prelude operand still compares structurally: nothing declares
/// `I64.eq` or `String.eq`, so the lookup declines to speak and `1 is 2` is the
/// one instruction it has always been.
#[test]
fn is_on_a_prelude_type_stays_one_node() {
    let checked = support::check(
        "\
def numbers(a: I64, b: I64) -> Bool:
    a is b

def words(a: String, b: String) -> Bool:
    a is not b
",
    );
    checked.assert_clean();
    assert_eq!(ty_of(&checked, "numbers", |kind| matches!(kind, ExprKind::Binary { .. })), "Bool");
    assert_eq!(ty_of(&checked, "words", |kind| matches!(kind, ExprKind::Binary { .. })), "Bool");
}

/// `<` requires `Ord`, by the same implementation check `is` uses for `Eq`.
///
/// This used to be pinned as a silence — *"so this compiles, and it should
/// not"* — and the silence was two questions sharing one refusal. §5.4 makes
/// `< > <= >=` `Ord`'s, `builtins.rs` says which prelude types implement it,
/// and a user writes `Held implements Ord:` in their own file, so *"does this
/// type implement `Ord`"* is answerable and is now answered.
#[test]
fn a_comparison_requires_ord() {
    let checked = support::check(
        "\
type Held:
    x: F64

def before(a: Held, b: Held) -> Bool:
    a < b
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Held` does not implement `Ord`"]);
}

/// And the right program dispatches, at each of the four spellings, to the one
/// method `stdlib-shape-and-packages.md` §4.5's AMENDMENT 1 gives `Ord`.
///
/// This used to pin the opposite — *"the block has to contain something and
/// the language does not say what"*, with a `compare` nobody called — and the
/// residue it named is closed: the block contains `less`, and `<` calls it.
#[test]
fn a_comparison_on_a_type_that_implements_ord_dispatches_to_less_at_all_four_spellings() {
    let checked = support::check(
        "\
type Held:
    x: F64

Held implements Ord:
    def less(self, other: &Held) -> Bool:
        self.x < other.x

def before(a: Held, b: Held) -> Bool:
    a < b

def after(a: Held, b: Held) -> Bool:
    a > b

def not_after(a: Held, b: Held) -> Bool:
    a <= b

def not_before(a: Held, b: Held) -> Bool:
    a >= b
",
    );
    checked.assert_clean();
    // `(written, receiver, operand, negated)`: the table on
    // `BodyChecker::ordering`.
    for (name, receiver, operand, negated) in [
        ("before", "a", "b", false),
        ("after", "b", "a", false),
        ("not_after", "b", "a", true),
        ("not_before", "a", "b", true),
    ] {
        let body = checked.body(name);
        let id = checked.find(name, |kind| matches!(kind, ExprKind::MethodCall { .. }));
        let ExprKind::MethodCall { receiver: on, method: Some(method), ref args } = body.expr(id).kind
        else {
            panic!("`{name}` is a resolved call to `less`");
        };
        assert_eq!(checked.krate.defs.get(method).name, "less", "{name}");
        assert_eq!(local_under(&checked, name, on), receiver, "{name}: the receiver");
        assert_eq!(local_under(&checked, name, args[0]), operand, "{name}: the operand");
        assert_eq!(checked.render(body.ty(id)), "Bool", "{name}");
        let wrapped = body.exprs().any(|(_, expr)| {
            matches!(expr.kind, ExprKind::Unary { operand, .. } if operand == id)
        });
        assert_eq!(wrapped, negated, "{name}: `<=` and `>=` are the negation");
        assert!(
            !body.exprs().any(|(_, expr)| matches!(expr.kind, ExprKind::Binary { .. })),
            "{name}: a dispatched comparison leaves no structural `Binary` behind"
        );
    }
}

/// A `choice` orders the same way a record does: the operator asks the type
/// for `less` and does not care what kind of type answers.
#[test]
fn a_choice_that_implements_ord_dispatches_to_less() {
    let checked = support::check(
        "\
choice Level:
    Low
    High(I64)

Level has:
    def rank(self) -> I64:
        match self:
            Low: 0
            High(n): 1 + n

Level implements Ord:
    def less(self, other: &Level) -> Bool:
        self.rank() < other.rank()

def above(a: Level, b: Level) -> Bool:
    a >= b
",
    );
    checked.assert_clean();
    let body = checked.body("above");
    let id = checked.find("above", |kind| matches!(kind, ExprKind::MethodCall { .. }));
    let ExprKind::MethodCall { method: Some(method), .. } = body.expr(id).kind else {
        panic!("`a >= b` on a `choice` is a resolved call");
    };
    assert_eq!(checked.krate.defs.get(method).name, "less");
}

/// The name of the local an operand reads, through whatever borrow or copy
/// the checker wrapped round it.
fn local_under(checked: &support::Checked, name: &str, mut id: science_types::thir::ExprId) -> String {
    let body = checked.body(name);
    loop {
        match body.expr(id).kind {
            ExprKind::Coerce { operand, .. } | ExprKind::Borrow { operand, .. } => id = operand,
            ExprKind::Local(def) => return checked.krate.defs.get(def).name.clone(),
            ref other => panic!("`{name}`: an operand that reads no local: {other:?}"),
        }
    }
}

/// The prelude's own numerics are unaffected, because they implement `Ord` and
/// because `1 < 2` never reaches an implementation at all.
#[test]
fn a_comparison_of_two_numbers_is_still_structural() {
    let checked = support::check(
        "\
def before(a: I64, b: I64) -> Bool:
    a < b

def letters(a: Char, b: Char) -> Bool:
    a < b

def words(a: &String, b: &String) -> Bool:
    a < b
",
    );
    checked.assert_clean();
    // **Never a call**, although `Ord.less` is now in each of these types'
    // index: `BodyChecker::ordering` hands a prelude head back to the
    // structural comparison it has always been.
    for name in ["before", "letters", "words"] {
        assert!(
            !checked
                .body(name)
                .exprs()
                .any(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. })),
            "{name}: a prelude comparison is an instruction, not a call to `less`"
        );
    }
}

/// A type parameter is silent, which is `methods`' §5 and the restraint the
/// whole of §6 is under. `examples/07_generics.science` writes exactly this,
/// under a declared `where T: Ord`, and it must keep compiling.
#[test]
fn a_comparison_on_a_type_parameter_reports_nothing() {
    support::check(
        "\
def largest[T](a: &T, b: &T) -> Bool
        where T: Ord:
    a > b
",
    )
    .assert_clean();
}

/// **Through a `T: Ord` bound the comparison is a call too**, to the
/// interface's own `less` — `science_codegen::mono` picks the implementation
/// per instantiation, and at a prelude type `science-codegen-llvm` answers it
/// with the structural instruction. This is what makes `largest[T: Ord]`
/// mean the same thing at `Int` and at a user's record.
///
/// This replaces a test that pinned the hole: an `implements Ord:` block
/// writing a `compare` that nothing called. That block is now `SC0539`, which
/// `conformance.rs` holds.
#[test]
fn a_comparison_on_a_bounded_type_parameter_calls_ord_less() {
    let checked = support::check(
        "\
def largest[T](a: &T, b: &T) -> Bool
        where T: Ord:
    a > b
",
    );
    checked.assert_clean();
    let body = checked.body("largest");
    let id = checked.find("largest", |kind| matches!(kind, ExprKind::MethodCall { .. }));
    let ExprKind::MethodCall { receiver, method: Some(method), .. } = body.expr(id).kind else {
        panic!("`a > b` under `T: Ord` is a resolved call");
    };
    assert_eq!(checked.krate.defs.get(method).name, "less");
    // `a > b` is `b.less(a)`.
    assert_eq!(local_under(&checked, "largest", receiver), "b");
}

/// **A type parameter with no `Ord` bound has nothing to call**, and stays
/// the structural `Binary` it was. It used to be silent and left for the
/// backend to refuse; the checker now reports it as `SC0535` naming the bound
/// to write, and this test pins that no `less` was reached for all the same.
#[test]
fn a_comparison_on_an_unbounded_type_parameter_is_not_a_call() {
    let checked = support::check(
        "\
def bigger[T](a: &T, b: &T) -> Bool:
    a > b
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert!(
        !checked
            .body("bigger")
            .exprs()
            .any(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. })),
        "no bound names `Ord`, so no `less` is in reach"
    );
}

/// **An operator dispatches to the *prelude's* interface and not to a name
/// that matches.** A program may declare an `interface Add:` of its own, and a
/// type implementing that one has not implemented §5.4's — which is the whole
/// reason `Prelude::WANTED` holds the nine operator interfaces' ids rather
/// than the checker comparing a string.
#[test]
fn an_operator_reaches_a_prelude_interface_only_when_it_is_the_prelude_s() {
    let checked = support::check(
        "\
interface Add:
    def add(self, other: Self) -> Self

type Vector:
    x: F64

Vector implements Add:
    def add(self, other: Vector) -> Vector:
        Vector(x: self.x)

def sum(a: Vector, b: Vector) -> Vector:
    a + b
",
    );
    // The fixture's `Add` shadows the prelude's, so `Vector` implements a
    // user interface of that name and not §5.4's. `+` is therefore refused,
    // which is what `Prelude::get` holding the prelude's id is for.
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Vector` does not implement `Add`"]);
}


// --- `Index` and `IndexMutably` on the prelude's `Array` -------------------
//
// `indexing-and-array-literals.md` §1.1's Decision 2, now declared:
// `interface Index of Idx: type Output; def index(self, at: Idx) -> borrowed
// Self.Output`, with `IndexMutably` beside it, and `Array of T implements
// Index of Int: type Output is T`. Each case below is a pair — the wrong
// program is refused, the right one passes — because a node typed `Ty::ERROR`
// would make both silent.

#[test]
fn an_array_element_has_the_arrays_element_type() {
    let checked = support::check(
        "\
def first(xs: &Array[I64]) -> I64:
    xs[0]
",
    );
    checked.assert_clean();
    assert_eq!(
        ty_of(&checked, "first", |kind| matches!(kind, ExprKind::Index { .. })),
        "&I64"
    );
    // §7 reads the `I64` out of the borrow, which is what makes the element
    // usable as a value.
    assert_eq!(coercions(&checked, "first"), vec![Coercion::Copy]);
}

#[test]
fn an_array_element_used_as_the_wrong_type_is_refused() {
    // The silence this closes: `a[i]` was `Ty::ERROR` and agreed with
    // everything, so an element of an `Array of I64` returned as a `String`
    // checked clean.
    let checked = support::check(
        "\
def first(xs: &Array[I64]) -> String:
    let a be xs[0]
    return a
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `String`, found `&I64`"]);
}

#[test]
fn the_element_type_follows_the_arrays_argument() {
    // Not a fixed answer: `Output` is the block's `T` and the receiver's
    // argument is what fixes it.
    let checked = support::check(
        "\
def first(xs: &Array[String]) -> &String:
    xs[0]
",
    );
    checked.assert_clean();
    assert_eq!(
        ty_of(&checked, "first", |kind| matches!(kind, ExprKind::Index { .. })),
        "&String"
    );
}

#[test]
fn a_write_through_an_index_is_checked_against_the_element() {
    let checked = support::check(
        "\
def set(xs: &mut Array[I64]):
    xs[0] be \"nueve\"
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `String`"]);
}

/// §1.1's own example, which is the reason the write's slot is the referent and
/// not the reference: the right-hand side is an `F64`, and a slot of `mutable
/// borrowed F64` would refuse it.
#[test]
fn the_notes_own_scale_loop_compiles() {
    support::check(
        "\
def scale(values: &mut Array[F64], factor: F64):
    for i in 0..values.length():
        values[i] be values[i] * factor
",
    )
    .assert_clean();
}

#[test]
fn the_index_operand_is_checked_against_the_declared_index_type() {
    let checked = support::check(
        "\
def first(xs: &Array[I64], key: String) -> I64:
    xs[key]
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `I64`, found `String`"]);
}

/// Decision 2's whole content: two interfaces, and a container that implements
/// only the first may be read and not written.
#[test]
fn a_write_through_a_type_with_no_index_mutably_names_that_interface() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements Index[I64]:
    def index(self, at: I64) -> &F64:
        &self.cell

def read(grid: &Grid) -> F64:
    grid[0]

def write(grid: &mut Grid):
    grid[0] be 1.0
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Grid` does not implement `IndexMutably`"]);
}

#[test]
fn a_type_that_implements_index_mutably_may_be_written_through() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements IndexMutably[I64]:
    def index_mutably(mutable self, at: I64) -> &mut F64:
        &mut self.cell

def write(grid: &mut Grid):
    grid[0] be 1.0
",
    );
    checked.assert_clean();
}

#[test]
fn the_write_slot_is_the_referent_and_a_wrong_value_is_still_refused() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements IndexMutably[I64]:
    def index_mutably(mutable self, at: I64) -> &mut F64:
        &mut self.cell

def write(grid: &mut Grid):
    grid[0] be \"nueve\"
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `F64`, found `String`"]);
}

/// `Map` is deliberately not indexable, and this pins it rather than leaving it
/// to be discovered: §1.1 never names `Map`, and §1.3's four discharge rules
/// are all about an extent. `Map.get` is `(borrowed V)?` and says so.
#[test]
fn a_map_is_not_indexable_and_is_silent_about_it() {
    // Silent rather than refused, because `methods`' §8 makes a prelude head's
    // method set *open*: "the prelude has not written it down" is not the same
    // sentence as "this type has no such operation".
    support::check(
        "\
def at(m: &Map[String, I64], key: &String) -> I64:
    m[key]
",
    )
    .assert_clean();
}

// --- An alias is its target at an operator ---------------------------------

/// `type Counter is Int` is `Int` under `+`, on either side and against a
/// literal.
///
/// `examples/02_bindings.science` promises *"an alias declared above is a name
/// for the same type"*, and the arithmetic arm unified its operands without
/// revealing them: `a + 1` was `SC0535`, *"`Counter` does not implement
/// `Add`"*, and `a + b` with `b: Int` was `SC0525`, *"expected `Counter`,
/// found `I64`"*. Gate C1's `Set of DefId` is where it was found — a counter
/// of ids, stepped with `+ 1`.
#[test]
fn an_alias_of_a_number_is_that_number_under_an_operator() {
    let checked = support::check(
        "\
type Counter is Int

def step(a: Counter, b: Int) -> Counter:
    let c be a + 1
    let d be b + a
    let e be a * b
    if a < b:
        return c
    d + e
",
    );
    checked.assert_clean();
}

/// The negative: revealing an alias of a record reveals a record, and a record
/// with no `Add` is still refused — naming the record, not the alias.
#[test]
fn an_alias_of_a_record_with_no_add_is_still_refused() {
    let checked = support::check(
        "\
type Point:
    x: Int

type Spot is Point

def twice(a: Spot) -> Spot:
    a + a
",
    );
    assert_eq!(checked.codes(), vec![535]);
}

// --- §7 at an assignment whose target is still a literal's class ----------

/// The program that found it. `let mutable first be 0` gives `first` the
/// literal's class for a type, and `x` is `&I64` because a `for` over an
/// `Array of I64` binds a borrow (`collections-and-chains.md` §4.3). The
/// assignment used to *unify* the class with `&I64`, so the literal `0` was
/// typed as a reference — no diagnostic, and the backend's `SC0402`, *"local
/// _9 is ptr and the value stored into it is i64"*. §7 reads the borrow out
/// instead, exactly as it does when `first` is annotated `I64`.
#[test]
fn a_loop_item_assigned_to_a_literal_initialised_binding_reads_as_a_value() {
    let checked = support::check(
        "\
def last(xs: &Array[I64]) -> I64:
    let mutable first be 0
    for x in xs:
        first be x
    if first is 0:
        return 0
    first
",
    );
    checked.assert_clean();
    assert!(coercions(&checked, "last").contains(&Coercion::Copy));
    assert_eq!(ty_of(&checked, "last", |kind| matches!(kind, ExprKind::Literal(_))), "I64");
}

/// The same shape over `F64`, and over an *integer* literal meeting a float:
/// the class takes `F64`, which §5 admits an integer literal at.
#[test]
fn a_float_loop_item_assigned_to_an_integer_initialised_binding_is_an_f64() {
    let checked = support::check(
        "\
def last(xs: &Array[F64]) -> F64:
    let mutable first be 0
    for x in xs:
        first be x
    first
",
    );
    checked.assert_clean();
    assert_eq!(ty_of(&checked, "last", |kind| matches!(kind, ExprKind::Literal(_))), "F64");
}

/// The negative: a borrow of a type that is not `Copy` is not read out, and a
/// literal's class does not become a reference either — so it is refused,
/// once, at the value, naming the literal.
#[test]
fn a_borrowed_string_assigned_to_a_literal_initialised_binding_is_refused() {
    let checked = support::check(
        "\
def last(xs: &Array[String]):
    let mutable first be 0
    for x in xs:
        first be x
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `an integer literal`, found `&String`"]);
}

/// And an owned value that is not a number is refused the same way. Before
/// this, `n be "s"` unified the literal `0` with `String` and the backend was
/// the first to object.
#[test]
fn a_string_assigned_to_a_literal_initialised_binding_is_refused() {
    let checked = support::check(
        "\
def main():
    let mutable n be 0
    n be \"s\"
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `an integer literal`, found `String`"]);
}

/// §5's exclusive-borrow bullet holds here as it does at an annotated target,
/// where `let mutable n: I64 be 0` then `n be counter` is refused too.
#[test]
fn an_exclusive_borrow_assigned_to_a_literal_initialised_binding_is_refused() {
    let checked = support::check(
        "\
def bump(counter: &mut I64):
    let mutable n be 0
    n be counter
",
    );
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `an integer literal`, found `&mut I64`"]);
}

// --- what `<` is still refused on --------------------------------------------

/// `true < false` was accepted by the checker and refused by the backend as
/// `SC0400`. `Bool`'s row in `builtins.rs` is `Eq, Copy, Clone, Display`: it has
/// no `Ord`, so the front end says so.
#[test]
fn a_comparison_of_two_bools_is_refused() {
    let checked = support::check(
        "\
def before(a: Bool, b: Bool) -> Bool:
    a < b
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`Bool` does not implement `Ord`"]);
}

/// `Bool` is still comparable for equality, which it does implement.
#[test]
fn two_bools_are_still_equal_or_not() {
    support::check(
        "\
def same(a: Bool, b: Bool) -> Bool:
    a is b
",
    )
    .assert_clean();
}

/// `a < b` on a type parameter that was promised nothing was accepted and then
/// refused by the backend. The bounds in scope are what the parameter
/// implements, so the question has an answer here.
#[test]
fn a_comparison_on_an_unbounded_type_parameter_is_refused() {
    let checked = support::check(
        "\
def before[T](a: &T, b: &T) -> Bool:
    a < b
",
    );
    assert_eq!(checked.codes(), vec![535]);
    assert_eq!(checked.messages(), vec!["`T` does not implement `Ord`"]);
}

/// A bound on a *different* interface does not buy `<`.
#[test]
fn a_bound_on_another_interface_does_not_license_order() {
    let checked = support::check(
        "\
def before[T: Clone](a: &T, b: &T) -> Bool:
    a < b
",
    );
    assert_eq!(checked.codes(), vec![535]);
}

/// Floats are off `Ord` (`F32`/`F64` implement `Eq` only, §5.1: NaN has no
/// total order), so a float does not satisfy `T: Ord`...
#[test]
fn a_float_does_not_satisfy_an_ord_bound() {
    let checked = support::check(
        "\
def before[T: Ord](a: &T, b: &T) -> Bool:
    a < b

def main():
    let x: F64 be 1.5
    let y: F64 be 2.5
    let r be before(x, y)
",
    );
    assert_eq!(checked.codes(), vec![534]);
    assert_eq!(checked.messages(), vec!["`F64` does not implement `Ord`"]);
}

/// ...and `<` on two floats is still the primitive comparison.
#[test]
fn a_comparison_of_two_floats_is_still_primitive() {
    support::check(
        "\
def before(a: F64, b: F64) -> Bool:
    a < b

def narrower(a: F32, b: F32) -> Bool:
    a >= b
",
    )
    .assert_clean();
}

/// `let mutable x be null` followed by `x be 5` put the two literals in one
/// class and the class defaulted to `Int`, building the `null` as an `Int`.
/// The author writes the nullable type.
#[test]
fn a_null_binding_later_given_a_number_asks_for_an_annotation() {
    let checked = support::check(
        "\
def main():
    let mutable x be null
    x be 5
",
    );
    assert_eq!(checked.codes(), vec![526]);
    assert_eq!(checked.messages(), vec!["the type of this `null` cannot be inferred"]);
}

/// With the annotation it is the program the note writes.
#[test]
fn an_annotated_nullable_binding_takes_a_number() {
    support::check(
        "\
def main():
    let mutable x: Int? be null
    x be 5
",
    )
    .assert_clean();
}

// --- `Mul[Rhs]`: one operator, several right-hand sides -------------------

const SCALED: &str = "\
type V:
    x: F64

V implements Copy

V implements Mul:
    def mul(self, other: V) -> V:
        V(x: self.x * other.x)

V implements Mul[F64]:
    def mul(self, other: F64) -> V:
        V(x: self.x * other)
";

/// The right operand's type picks the implementation; both calls resolve to a
/// method, and they are two different ones.
#[test]
fn the_right_operand_picks_between_two_implementations_of_one_operator() {
    let source = format!(
        "{SCALED}
def both(a: V, k: F64) -> V:
    let by_value be a * a
    a * k
"
    );
    let checked = support::check(&source);
    checked.assert_clean();
    let methods: Vec<_> = checked
        .body("both")
        .exprs()
        .filter_map(|(_, expr)| match expr.kind {
            ExprKind::MethodCall { method: Some(method), .. } => Some(method),
            _ => None,
        })
        .collect();
    assert_eq!(methods.len(), 2);
    assert_ne!(methods[0], methods[1]);
}

/// A literal refutes only what it cannot be: `2.0` and `2` both reach the `F64`
/// implementation and never the one taking a `V`.
#[test]
fn a_literal_operand_reaches_the_scalar_implementation() {
    let source = format!(
        "{SCALED}
def literals(a: V) -> V:
    let f be a * 2.0
    a * 2
"
    );
    support::check(&source).assert_clean();
}

/// A shared borrow of a `Copy` scalar reads as the value, as at any operator.
#[test]
fn a_borrowed_scalar_operand_is_read_as_the_value() {
    let source = format!(
        "{SCALED}
def through(a: V, ks: &Array[F64]) -> F64:
    let mutable total be 0.0
    for k in ks:
        total be total + (a * k).x
    total
"
    );
    support::check(&source).assert_clean();
}

/// An operand no implementation takes is `SC0533`, the code a written call gets.
#[test]
fn an_operand_no_implementation_takes_is_refused() {
    let source = format!(
        "{SCALED}
def bad(a: V) -> V:
    a * \"two\"
"
    );
    assert_eq!(support::check(&source).codes(), vec![533]);
}

/// With a single implementation the old behaviour is untouched: a mismatched
/// operand is the ordinary type error.
#[test]
fn a_single_implementation_still_checks_its_operand_against_the_signature() {
    let checked = support::check(
        "\
type V:
    x: F64

V implements Mul[F64]:
    def mul(self, other: F64) -> V:
        V(x: self.x * other)

def bad(a: V) -> V:
    a * \"two\"
",
    );
    assert_eq!(checked.codes(), vec![525]);
}

/// A comparison on a literal-inferred `Int` is structural, and stays so beside
/// a type with several implementations of an operator: `i < 3` once reached
/// the operand selection through the prelude's own `Ord` and was `SC0533`.
#[test]
fn a_literal_comparison_beside_mixed_operators_is_still_structural() {
    let source = format!(
        "{SCALED}
def count() -> Int:
    let mutable i be 2
    let mutable n be 0
    if i < 3:
        n be n + 1
    if i >= 2 and 3 > i:
        n be n + 1
    n
"
    );
    support::check(&source).assert_clean();
}
