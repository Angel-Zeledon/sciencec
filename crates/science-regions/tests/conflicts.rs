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

def look(d: borrowed Doc) -> Int:
    d.title

def take(d: Doc) -> Int:
    d.title

def touch(d: mutable borrowed Doc):
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
    let shared be borrowed doc
    touch(mutable borrowed doc)
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
    let shared be borrowed doc
    print(look(shared))
    touch(mutable borrowed doc)
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
    let a be borrowed doc
    let b be borrowed doc
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
def escape() -> borrowed Int:
    let value be 5
    borrowed value
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
    let shared be borrowed doc
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

def keep(x: borrowed Int) -> Bool:
    true

def go():
    let mutable p be Pair(left: 0, right: 0)
    let l be borrowed p.left
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

def keep(x: borrowed Int) -> Bool:
    true

def go():
    let mutable p be Pair(left: 0, right: 0)
    let l be borrowed p.left
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
    let s be borrowed c
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
    let s be borrowed c
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
    items: Array of Symbol

Table has:
    def find(self, name: Int) -> borrowed Symbol:
        self.items.get(name)

    def add(mutable self, name: Int):
        self.items.push(Symbol(name: name))

    def intern(mutable self, name: Int) -> borrowed Symbol:
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
    items: Array of Symbol

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
    let shared be borrowed doc
    touch(mutable borrowed doc)
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
