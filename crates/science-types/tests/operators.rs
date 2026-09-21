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

/// Without §7 this is the diagnostic the same loop would produce, so the
/// mechanism is asserted rather than assumed: a non-`Copy` element does not
/// add, and the message is about the borrow the `for` bound.
#[test]
fn a_loop_over_an_array_of_strings_does_not_add_up() {
    let checked = support::check(
        "\
def joined(words: &Array[String], seed: String) -> String:
    let mutable out be seed
    for word in words:
        out be out + word
    out
",
    );
    // `String implements Add` carries no method the prelude has transcribed,
    // so `methods`' §8 keeps the operator silent; what is left is the
    // structural answer, which refuses a `borrowed String` against a `String`.
    assert_eq!(checked.codes(), vec![525]);
    assert_eq!(checked.messages(), vec!["expected `String`, found `&String`"]);
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
/// corpus already contains; requiring it invents nothing. The node stays a
/// `Binary` because `is not` has no method of its own — see
/// `BodyChecker::implements_operand`.
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
    // `is` is still one node: nothing here desugars.
    assert_eq!(ty_of(&checked, "same", |kind| matches!(kind, ExprKind::Binary { .. })), "Bool");
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

/// And the right program still passes, at each of the four spellings.
///
/// **The block has to contain something and the language does not say what.**
/// `Ord` is declared with no methods, so nothing in the block below is checked
/// against a declaration and nothing is called; but an `implements` block may
/// not be empty (`SC0100`), so the author writes a method and picks its name.
/// That is the residue this change does not close, and `implements_operand`
/// says so.
#[test]
fn a_comparison_on_a_type_that_implements_ord_passes_at_all_four_spellings() {
    let checked = support::check(
        "\
type Held:
    x: F64

Held implements Ord:
    def compare(self, other: Held) -> I64:
        0

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
    // The node is a `Binary` and not a call: `Ord` is required, never
    // dispatched.
    assert_eq!(
        ty_of(&checked, "before", |kind| matches!(kind, ExprKind::Binary { .. })),
        "Bool"
    );
    assert!(
        !checked
            .body("before")
            .exprs()
            .any(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. })),
        "`a < b` must not become a call: `Ord` has no method this crate may name"
    );
}

/// The prelude's own numerics are unaffected, because they implement `Ord` and
/// because `1 < 2` never reaches an implementation at all.
#[test]
fn a_comparison_of_two_numbers_is_still_structural() {
    support::check(
        "\
def before(a: I64, b: I64) -> Bool:
    a < b

def letters(a: Char, b: Char) -> Bool:
    a < b

def words(a: &String, b: &String) -> Bool:
    a < b
",
    )
    .assert_clean();
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

/// **`Ord` is still not dispatched**, and what that waits for is three things
/// no note supplies: a method name, an `Ordering` return type that is not in
/// §8's closed library, and a rule for how four operators sit over one
/// `compare` — including what `F64`'s NaN does to a total order. This test
/// pins the remaining hole: an `implements Ord:` block that writes a `compare`
/// is not consulted, and a comparison of two values it would order wrongly is
/// not the compiler's business yet.
#[test]
fn ord_is_still_not_dispatched_and_its_method_is_not_named() {
    let checked = support::check(
        "\
type Held:
    x: F64

Held implements Ord:
    def compare(self, other: Held) -> I64:
        0

def before(a: Held, b: Held) -> Bool:
    a < b
",
    );
    checked.assert_clean();
    // The `compare` above is an ordinary method of the block. Nothing calls it,
    // and nothing checked its signature against a declaration, because `Ord`
    // declares none.
    assert!(
        !checked
            .body("before")
            .exprs()
            .any(|(_, expr)| matches!(expr.kind, ExprKind::MethodCall { .. })),
        "nothing may dispatch `<` until a note writes `Ord`'s method down"
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
