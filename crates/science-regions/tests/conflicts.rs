//! §3 step 4 and §7.3's named failures, one fixture each.
//!
//! Every test here is a *program*, not a constructed constraint set. `§7.1`'s
//! message is a narrative over the author's spans, and a fixture built out of
//! hand-made [`science_mir::mir::BorrowData`] would assert that the solver
//! works on an input the lowering never produces — which is the mistake
//! `science-mir`'s own harness argues against one level down.

mod support;

use support::{check, codes};

fn only(source: &str) -> Vec<u16> {
    let checked = check(source);
    checked.reported()
}

const DOC: &str = "\
type Doc:
    title: Int

def look(d: &Doc) -> Int:
    d.title

def take(d: Doc) -> Int:
    d.title

def touch(d: &mut Doc):
    d.title be 1
";

/// `SC0330`, §7.1's own shape: a shared borrow, an exclusive access, and a
/// later use of the first.
#[test]
fn a_shared_borrow_and_an_exclusive_one_cannot_overlap() {
    let source = format!(
        "{DOC}
def go():
    let mutable doc be Doc(title: 0)
    let shared be &doc
    touch(&mut doc)
    print(look(shared))
"
    );
    assert_eq!(only(&source), vec![330]);
}

/// The same program with the last use moved above the conflict — §7.1's own
/// suggested fix — compiles. The borrow ends at its last use, which is
/// Decision 1's whole point.
#[test]
fn moving_the_last_use_above_the_conflict_is_enough() {
    let source = format!(
        "{DOC}
def go():
    let mutable doc be Doc(title: 0)
    let shared be &doc
    print(look(shared))
    touch(&mut doc)
"
    );
    assert_eq!(only(&source), Vec::<u16>::new());
}

/// Many shared borrows are rule 4's permissive half.
#[test]
fn many_shared_borrows_are_fine() {
    let source = format!(
        "{DOC}
def go():
    let doc be Doc(title: 0)
    let a be &doc
    let b be &doc
    print(look(a))
    print(look(b))
"
    );
    assert_eq!(only(&source), Vec::<u16>::new());
}

/// `SC0333` — rule 5. There is no lifetime syntax to widen, so this is the
/// error that has nothing to offer but a change of signature.
#[test]
fn a_borrow_may_not_outlive_its_referent() {
    let source = "\
def escape() -> &Int:
    let value be 5
    &value
";
    assert_eq!(only(source), vec![333]);
}

/// `SC0334`. The move is legal, the borrow is legal, and the order is not.
#[test]
fn a_value_may_not_be_moved_while_borrowed() {
    let source = format!(
        "{DOC}
def go():
    let doc be Doc(title: 0)
    let shared be &doc
    print(take(doc))
    print(look(shared))
"
    );
    assert_eq!(only(&source), vec![334]);
}

/// `may_overlap` is per field: writing `p.right` does not disturb a borrow of
/// `p.left`. `science-mir`'s [`science_mir::mir::Place`] is what makes this a
/// comparison rather than a guess.
#[test]
fn two_different_fields_do_not_conflict() {
    let source = "\
type Pair:
    left: Int
    right: Int

def keep(x: &Int) -> Bool:
    true

def go():
    let mutable p be Pair(left: 0, right: 0)
    let l be &p.left
    p.right be 1
    print(keep(l))
";
    assert_eq!(only(source), Vec::<u16>::new());
}

/// And the same field does.
#[test]
fn writing_the_borrowed_field_conflicts() {
    let source = "\
type Pair:
    left: Int
    right: Int

def keep(x: &Int) -> Bool:
    true

def go():
    let mutable p be Pair(left: 0, right: 0)
    let l be &p.left
    p.left be 1
    print(keep(l))
";
    assert_eq!(only(source), vec![330]);
}

/// §10 item 4's acceptance case, from this side: an exclusive borrow reserved
/// while a shared borrow of the same place is still live, and dead by the
/// activation.
///
/// **This is the program two-phase borrows exist for**, and
/// [`science_regions::access`]'s §4 is why a checker that treats the
/// reservation as the borrow refuses it.
#[test]
fn a_two_phase_reservation_tolerates_a_shared_borrow_that_ends_first() {
    let source = "\
type Counter:
    total: Int

Counter has:
    def bump(mutable self, by: Int):
        self.total be self.total + by

    def read(self) -> Int:
        self.total

def go():
    let mutable c be Counter(total: 0)
    let s be &c
    c.bump(s.read())
";
    assert_eq!(only(source), Vec::<u16>::new());
}

/// And the same shared borrow used *after* the call is refused, because it is
/// live at the activation.
#[test]
fn a_shared_borrow_used_after_the_call_is_refused() {
    let source = "\
type Counter:
    total: Int

Counter has:
    def bump(mutable self, by: Int):
        self.total be self.total + by

    def read(self) -> Int:
        self.total

def go():
    let mutable c be Counter(total: 0)
    let s be &c
    c.bump(s.read())
    print(s.read())
";
    assert_eq!(only(source), vec![330]);
}

/// §2's `intern` shape: *"rejected under Decision 1, and sound"*.
///
/// The note names this as the price of NLL over Polonius and says the answer
/// for F0 is to return an id. It is refused, and the message is the
/// shared-against-exclusive one rather than anything about regions.
#[test]
fn the_get_or_insert_shape_is_refused_exactly_as_section_two_predicts() {
    let source = "\
type Symbol:
    name: Int

type Table:
    items: Array[Symbol]

Table has:
    def find(self, name: Int) -> &Symbol:
        self.items.get(name)

    def add(mutable self, name: Int):
        self.items.push(Symbol(name: name))

    def intern(mutable self, name: Int) -> &Symbol:
        let found be self.find(name)
        self.add(name)
        found
";
    let checked = check(source);
    assert_eq!(checked.reported(), vec![330], "{:?}", codes(&checked.regions));
}

/// And §2's own stated workaround — *"return an id, not a borrow"* — compiles.
#[test]
fn returning_an_id_instead_compiles() {
    let source = "\
type Symbol:
    name: Int

type Table:
    items: Array[Symbol]

Table has:
    def position(self, name: Int) -> Int:
        name

    def add(mutable self, name: Int):
        self.items.push(Symbol(name: name))

    def intern(mutable self, name: Int) -> Int:
        let found be self.position(name)
        self.add(name)
        found
";
    assert_eq!(only(source), Vec::<u16>::new());
}

/// Decision 9: three spans, and never a region variable.
#[test]
fn the_message_is_a_narrative_over_spans_and_names_no_region() {
    let source = format!(
        "{DOC}
def go():
    let mutable doc be Doc(title: 0)
    let shared be &doc
    touch(&mut doc)
    print(look(shared))
"
    );
    let checked = check(&source);
    let diagnostic = checked.regions.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.labels.len(), 3, "§7.1 wants three spans: {:?}", diagnostic.labels);
    let rendered = format!("{diagnostic:?}");
    assert!(!rendered.contains("'0"), "a region variable reached the message: {rendered}");
    assert!(
        rendered.contains("still needed here"),
        "the load-bearing third span is missing: {rendered}"
    );
}

// --- §3.1: a call whose receiver and argument are the same place ------------

const COPY_EQ: &str = "\
type P:
    x: Int

P implements Eq:
    def eq(self, other: P) -> Bool:
        self.x is other.x

P implements Copy
";

const OWNING_EQ: &str = "\
type Note:
    text: String

Note implements Eq:
    def eq(self, other: Note) -> Bool:
        self.text is other.text
";

/// **`a is a` on a `Copy` record is not a borrow error, and it used to be.**
///
/// §6.1 rule 2 is *"assigning, passing, or returning moves ownership, **unless
/// the type is `Copy`**"*, and `eq`'s `other` is a passing. The refusal came
/// from `science-mir`'s `lower` §5 making every record operand a `Move`
/// whatever it declared — so this was the rule-4 check being handed a move that
/// rule 2 says is a copy, and the fix is §5.1 one crate up rather than a
/// relaxation here. **Nothing in this file's rules changed to make it pass.**
#[test]
fn a_call_on_a_copy_record_may_name_the_same_place_twice() {
    let source = format!(
        "{COPY_EQ}
def go() -> Bool:
    let a be P(x: 1)
    a is a
"
    );
    assert_eq!(only(&source), Vec::<u16>::new());
}

/// The hand-written call underneath it, which is what `is` lowers to. It gave
/// the byte-identical diagnostic before and it is clean for the same reason
/// now: the operator is not a special case of anything.
#[test]
fn the_same_holds_for_the_method_call_the_operator_lowers_to() {
    let source = format!(
        "{COPY_EQ}
def go() -> Bool:
    let a be P(x: 1)
    a.eq(a)
"
    );
    assert_eq!(only(&source), Vec::<u16>::new());
}

/// **And the refusal survives where rule 2 says it should.** `Note` owns a
/// `String`, so it is not `Copy`; `other: Note` therefore moves, `self` holds a
/// shared borrow of the same local, and rule 4 forbids the overlap. This is not
/// the case above with the `Copy` line missing by accident — it is the case the
/// language rejects on purpose, and a checker that accepted it would be
/// accepting a value consumed while it is borrowed.
#[test]
fn a_call_on_a_type_that_owns_something_still_may_not() {
    let source = format!(
        "{OWNING_EQ}
def go() -> Bool:
    let a be Note(text: \"hi\")
    a is a
"
    );
    assert_eq!(only(&source), vec![334]);
}

/// `check`'s §3.1. Decision 9's three spans are one span here, so the message
/// says something other than a narrative — and, in particular, does not advise
/// moving a use above a line that does not exist.
#[test]
fn the_message_for_one_expression_does_not_narrate_three_spans() {
    let source = format!(
        "{OWNING_EQ}
def go() -> Bool:
    let a be Note(text: \"hi\")
    a is a
"
    );
    let checked = check(&source);
    let diagnostic = checked.regions.iter().next().expect("one diagnostic");
    assert_eq!(
        diagnostic.labels.len(),
        1,
        "two labels under the same span is the thing being removed: {:?}",
        diagnostic.labels
    );
    let rendered = format!("{diagnostic:?}");
    assert!(
        !rendered.contains("moving the last use above"),
        "advice that cannot be followed survived: {rendered}"
    );
    assert!(
        rendered.contains("borrows `a` and moves it at the same time"),
        "the shape is not named: {rendered}"
    );
    assert!(
        rendered.contains("implements Copy"),
        "the first of the three fixes is missing: {rendered}"
    );
}

/// **A `mutable self` method may not be handed its own receiver, and `Copy`
/// does not help.** `Counter` *is* `Copy`, so the argument is a copy and there
/// is no move anywhere in the program — and it is still refused, by rule 4's
/// strict half: *"an exclusive borrow admits no other access to the same place
/// while it lasts"*. This is the shape that most needed the negative evidence,
/// because making the argument a copy is exactly what could have let it
/// through.
///
/// **The code moved from `SC0334` to `SC0330` and that is the point.** It used
/// to say *"`c` is moved while it is still borrowed"* about a program with no
/// move in it.
#[test]
fn an_exclusive_receiver_may_not_be_its_own_argument_even_when_copy() {
    let source = "\
type Counter:
    hits: Int

Counter implements Copy

Counter has:
    def absorb(mutable self, other: Counter):
        self.hits be self.hits + other.hits

def go():
    let mutable c be Counter(hits: 1)
    c.absorb(c)
";
    let checked = check(source);
    assert_eq!(checked.reported(), vec![330], "{:?}", codes(&checked.regions));
    let diagnostic = checked.regions.iter().next().expect("one diagnostic");
    assert_eq!(
        diagnostic.labels.len(),
        1,
        "one expression is one span: {:?}",
        diagnostic.labels
    );
    let rendered = format!("{diagnostic:?}");
    assert!(
        rendered.contains("borrows `c` exclusively and reads it"),
        "the shape is not named: {rendered}"
    );
    assert!(
        !rendered.contains("moved"),
        "a program with no move in it is still described as a move: {rendered}"
    );
}

/// The same method with the receiver shared instead. Nothing is written, so
/// nothing conflicts, and the `Copy` argument is an ordinary read.
#[test]
fn a_shared_receiver_may_be_its_own_argument_when_copy() {
    let source = "\
type Counter:
    hits: Int

Counter implements Copy

Counter has:
    def total(self, other: Counter) -> Int:
        self.hits + other.hits

def go() -> Int:
    let c be Counter(hits: 1)
    c.total(c)
";
    assert_eq!(only(source), Vec::<u16>::new());
}

/// **The ordinary three-span narrative is untouched**, which is the guard
/// against §3.1's branch being reached by anything but its own shape: here the
/// borrow, the move and the last use are three different lines and Decision 9
/// still gets all three.
#[test]
fn a_move_and_a_borrow_on_different_lines_keep_the_narrative() {
    let source = format!(
        "{DOC}
def go():
    let doc be Doc(title: 0)
    let shared be &doc
    print(take(doc))
    print(look(shared))
"
    );
    let checked = check(&source);
    assert_eq!(checked.reported(), vec![334], "{:?}", codes(&checked.regions));
    let diagnostic = checked.regions.iter().next().expect("one diagnostic");
    assert_eq!(diagnostic.labels.len(), 3, "§7.1 wants three spans: {:?}", diagnostic.labels);
    assert!(
        format!("{diagnostic:?}").contains("is moved while it is still borrowed"),
        "the general headline was replaced: {diagnostic:?}"
    );
}
