//! §10 item 2: explicit places, and the two projections THIR refused.
//!
//! THIR's §3 states the debt precisely — *"what is deliberately not a place: an
//! index (`a[i]`), and a dereference"* — and says the reason is that an index
//! place needs a value and a tree has no temporary to hold one. These tests are
//! that debt paid, and the last two are the invariant the payment rests on.

mod support;

use science_mir::mir::{Local, Place, Projection, Rvalue, StatementKind};
use science_types::ty::Ty;
use support::lower;

/// Every place built while lowering a body, with its projection kinds named.
fn projections(lowered: &support::Lowered, name: &str) -> Vec<String> {
    let body = lowered.body(name);
    let mut out = Vec::new();
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            if let StatementKind::Assign { place, rvalue } = &statement.kind {
                out.push(describe(place));
                if let Rvalue::Ref { place, .. } | Rvalue::Discriminant(place) = rvalue {
                    out.push(describe(place));
                }
                if let Rvalue::Use(operand) = rvalue {
                    if let Some(place) = operand.place() {
                        out.push(describe(place));
                    }
                }
            }
        }
    }
    out
}

fn describe(place: &Place) -> String {
    let mut out = String::new();
    for step in &place.projection {
        out.push(match step {
            Projection::Deref { .. } => '*',
            Projection::Field { .. } => 'f',
            Projection::TupleField { .. } => 't',
            Projection::Index { .. } => 'i',
            Projection::Downcast { .. } => 'd',
        });
    }
    out
}

#[test]
fn a_field_of_a_local_is_a_field_projection() {
    let source = "type Doc:\n    hits: Int\n\ndef f(d: Doc) -> Int:\n    d.hits\n";
    let lowered = lower(source);
    assert!(projections(&lowered, "f").contains(&"f".to_string()));
}

/// The first of the two THIR could not express.
#[test]
fn an_index_is_a_projection_holding_a_temporary() {
    let source = "def f(xs: Array of Int, i: Int) -> Int:\n    xs[i]\n";
    let lowered = lower(source);
    assert!(
        projections(&lowered, "f").contains(&"i".to_string()),
        "`xs[i]` did not produce an index projection"
    );
}

/// The second. Science has no dereference operator, so every one of these was
/// inferred from a type — `lower`'s §4.
#[test]
fn a_field_through_a_borrow_is_a_dereference_then_a_field() {
    let source = concat!(
        "type Parser:\n",
        "    position: Int\n",
        "\n",
        "Parser has:\n",
        "    def at(self) -> Int:\n",
        "        self.position\n",
    );
    let lowered = lower(source);
    assert!(
        projections(&lowered, "at").contains(&"*f".to_string()),
        "a field of `self` did not deref: {:?}",
        projections(&lowered, "at")
    );
}

/// §10 item 2's actual requirement, which is not *"places exist"* but
/// *"two expressions denoting the same place produce the same place"*.
#[test]
fn two_spellings_of_one_field_are_one_place() {
    let source = concat!(
        "type Doc:\n",
        "    hits: Int\n",
        "\n",
        "def f(d: Doc) -> Int:\n",
        "    let a be d.hits\n",
        "    let b be d.hits\n",
        "    a + b\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let reads: Vec<Place> = body
        .blocks()
        .flat_map(|(_, block)| block.statements.clone())
        .filter_map(|statement| match statement.kind {
            StatementKind::Assign { rvalue: Rvalue::Use(operand), .. } => {
                operand.place().cloned()
            }
            _ => None,
        })
        .filter(|place| !place.projection.is_empty())
        .collect();
    assert_eq!(reads.len(), 2, "{reads:?}");
    assert_eq!(reads[0], reads[1], "two spellings of `d.hits` are two places");
}

/// The other half of item 2, which is the one that is easy to get wrong:
/// equality must not claim more than it knows.
#[test]
fn two_indices_are_not_equal_and_do_overlap() {
    let one = Place::local(Local::from_index(0))
        .project(Projection::Index { index: Local::from_index(1), ty: Ty::ERROR });
    let other = Place::local(Local::from_index(0))
        .project(Projection::Index { index: Local::from_index(2), ty: Ty::ERROR });
    assert_ne!(one, other, "`a[i]` and `a[j]` must not compare equal");
    assert!(one.may_overlap(&other), "`a[i]` and `a[j]` may be the same element");
}

#[test]
fn two_different_fields_do_not_overlap() {
    let source = "type Doc:\n    hits: Int\n    rank: Int\n\ndef f(d: Doc) -> Int:\n    d.hits + d.rank\n";
    let lowered = lower(source);
    // Built by hand rather than read off the body, because what is under test
    // is the predicate and not the lowering.
    let hits = lowered.def("hits", science_resolve::hir::DefKind::Field);
    let rank = lowered.def("rank", science_resolve::hir::DefKind::Field);
    let root = Place::local(Local::from_index(1));
    let one = root.project(Projection::Field { field: hits, ty: Ty::ERROR });
    let other = root.project(Projection::Field { field: rank, ty: Ty::ERROR });
    assert!(!one.may_overlap(&other));
    assert!(one.may_overlap(&root), "a field overlaps the whole");
    assert!(root.may_overlap(&one), "and the whole overlaps the field");
}

/// The invariant [`Projection::Index`]'s equality rests on: an index temporary
/// is written once, so *the same temporary* means *the same element*.
#[test]
fn index_temporaries_are_assigned_once() {
    let source = concat!(
        "def f(xs: Array of Int, i: Int) -> Int:\n",
        "    let a be xs[i]\n",
        "    let b be xs[i]\n",
        "    a + b\n",
    );
    let lowered = lower(source);
    assert!(lowered.body("f").index_temps_are_single_assignment());
}

/// A place sees through a narrowing and not through a coercion, which is THIR's
/// §3 rule kept rather than re-decided.
#[test]
fn a_narrowed_read_is_the_same_place() {
    let source = concat!(
        "def f(n: Int?) -> Int:\n",
        "    if n?:\n",
        "        n\n",
        "    else:\n",
        "        0\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let narrowed: Vec<Place> = body
        .blocks()
        .flat_map(|(_, block)| block.statements.clone())
        .filter_map(|statement| match statement.kind {
            StatementKind::Assign { rvalue: Rvalue::Narrow { operand, .. }, .. } => {
                operand.place().cloned()
            }
            _ => None,
        })
        .collect();
    assert_eq!(narrowed.len(), 1, "expected one narrowed read: {narrowed:?}");
    assert_eq!(
        narrowed[0],
        Place::local(Local::from_index(1)),
        "the narrowed read named a place other than the parameter"
    );
}

/// §4's rule at the *receiver of a method call*, which is the one place it was
/// missed.
///
/// Inside a method whose own receiver is already a reference, `self.other()`
/// must reborrow what `self` points at. Borrowing `_1` itself would be a loan
/// of the callee's own storage: `_1` dies at the end of this frame, so the
/// reference a caller is handed back would point at a dead one, and the type
/// would be `borrowed (borrowed Table)` where the callee's parameter is
/// `borrowed Table`.
#[test]
fn a_method_call_on_a_borrowed_receiver_reborrows_the_referent() {
    let source = concat!(
        "type Table:\n",
        "    count: Int\n",
        "\n",
        "Table has:\n",
        "    def size(self) -> Int:\n",
        "        self.count\n",
        "\n",
        "    def doubled(self) -> Int:\n",
        "        self.size() + self.size()\n",
    );
    let lowered = lower(source);
    let body = lowered.body("doubled");
    let self_local = Local::from_index(1);
    assert_eq!(body.borrows().len(), 2, "each `self.size()` takes one receiver borrow");
    for data in body.borrows() {
        assert_eq!(data.place.local, self_local, "the receiver borrow is not of `self`");
        assert!(
            matches!(data.place.projection.as_slice(), [Projection::Deref { .. }]),
            "`self.size()` borrowed the local holding the reference rather than \
             its referent: {}",
            science_mir::dump::body(&lowered.krate.defs, body)
        );
        let rendered = lowered.types.render(&lowered.krate.defs, data.destination.ty(body));
        assert_eq!(
            rendered, "borrowed Table",
            "the receiver reference has the wrong type for the parameter it fills"
        );
    }
    // §10 item 2, on the projection this inserted: the `Deref`'s type comes
    // from the local's revealed declaration and not from either call's node, so
    // the two receivers denote *one* place rather than two that print alike.
    assert_eq!(
        body.borrows()[0].place,
        body.borrows()[1].place,
        "two calls on one receiver produced two places"
    );
}

/// §9's cost, on the shape that pays it.
///
/// A receiver with no place of its own gets a temporary, and §9 moved the
/// *reference* temporary to after it, because the type of the reference is not
/// known until the dereferences are. What must still be true is that the
/// referent is that temporary — a `Row` is not a reference, so no dereference
/// is inserted here — and that every local is still written once, which is what
/// index equality rests on.
#[test]
fn a_receiver_with_no_place_of_its_own_is_borrowed_from_its_temporary() {
    let source = concat!(
        "type Row:
",
        "    value: Int
",
        "
",
        "Row has:
",
        "    def plus(self, n: Int) -> Int:
",
        "        self.value + n
",
        "
",
        "def make() -> Row:
",
        "    Row(value: 3)
",
        "
",
        "def f(n: Int) -> Int:
",
        "    make().plus(n)
",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    assert_eq!(
        body.borrows().len(),
        1,
        "the fixture must take a receiver borrow, or it asserts nothing: {}",
        lowered.dump("f")
    );
    assert!(
        body.borrows()[0].place.is_local(),
        "the referent is the value temporary and nothing is projected off it: {}",
        lowered.dump("f")
    );
    assert!(body.index_temps_are_single_assignment());
}
