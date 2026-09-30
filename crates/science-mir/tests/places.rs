//! §10 item 2: explicit places, and the two projections THIR refused.
//!
//! THIR's §3 states the debt precisely — *"what is deliberately not a place: an
//! index (`a[i]`), and a dereference"* — and says the reason is that an index
//! place needs a value and a tree has no temporary to hold one. These tests are
//! that debt paid, and the last two are the invariant the payment rests on.

mod support;

use science_mir::mir::{
    Callee, Constant, Local, LocalKind, Operand, Place, Projection, Rvalue, StatementKind,
    TerminatorKind,
};
use science_resolve::hir::Literal;
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
                // `Rvalue::Coerce` beside `Rvalue::Use`: `assign`'s §7 reads
                // a value out of a borrow of a `Copy` type, so an `a[i]` whose
                // element is a scalar arrives as the operand of a coercion
                // rather than bare. The place under it is the same place, and a
                // scanner that looked only at `Use` would report the projection
                // missing the day the prelude gave `Index` a return type.
                if let Rvalue::Use(operand) | Rvalue::Coerce { operand, .. } = rvalue {
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
            Projection::Payload { .. } => '?',
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
    let source = "def f(xs: Array[Int], i: Int) -> Int:\n    xs[i]\n";
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
        "def f(xs: Array[Int], i: Int) -> Int:\n",
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

/// The niched half of the same gap: `Array[T].get`'s `-> (&T)?` narrows to a
/// *borrow*, not a record. `examples/19_stdlib.science`'s `hits_of` is this
/// shape (`let first be records.get(0); if first?: return first.hits`), and
/// it is Decision 19's case and not Decision 18's — `auto_deref` alone stops
/// on the place's own declared type, `(&Record)?`, which is a `Nullable` and
/// not itself a `Borrowed`; `deref_to_hole` is the further step keyed on the
/// narrow's own type instead.
#[test]
fn a_field_of_a_niched_narrow_is_a_dereference_then_a_field() {
    let source = concat!(
        "type Record:\n",
        "    hits: Int\n",
        "\n",
        "def hits_of(records: &Array[Record]) -> Int?:\n",
        "    let first be records.get(0)\n",
        "    if first?:\n",
        "        return first.hits\n",
        "    null\n",
    );
    let lowered = lower(source);
    assert!(
        !lowered.statements("hits_of").iter().any(|s| s == "assign error"),
        "a field of a niched narrow degraded to a hole: {}",
        lowered.dump("hits_of")
    );
    assert!(
        projections(&lowered, "hits_of").contains(&"*f".to_string()),
        "the niched narrow's field was not read through a dereference: {}",
        lowered.dump("hits_of")
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
            "`self.size()` &the local holding the reference rather than \
             its referent: {}",
            science_mir::dump::body(&lowered.krate.defs, body)
        );
        let rendered = lowered.types.render(&lowered.krate.defs, data.destination.ty(body));
        assert_eq!(
            rendered, "&Table",
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

/// §4.7's *"borrows auto-dereference for assignment"*: the target of a `be` is
/// the referent, not the reference.
///
/// `def bump(counter: mutable borrowed Int): counter be counter + 1` is
/// `examples/01_functions.science`'s, and Science has **no dereference
/// operator**, so there is no other thing the author could have written.
///
/// **The fixture does not type-check and that is the subject.** `science-mir`'s
/// §7 item 15: the checker compares the target's declared type against the
/// value's and dereferences nothing, so `counter be 5` is `SC0525` — and this
/// crate's harness lowers a program the checker complained about *on purpose*,
/// for the reason it states, so the MIR half of §4.7 can be held to something
/// before the front-end half lands. Without it, a checker that starts typing
/// the value at the referent gets a MIR that stores an `Int` into a slot
/// holding a reference, which nothing between here and LLVM reports.
#[test]
fn an_assignment_through_an_exclusive_borrow_names_the_referent() {
    let source = "def set(counter: &mut Int):\n    counter be 5\n";
    let lowered = lower(source);
    let body = lowered.body("set");
    let target = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .find_map(|statement| match &statement.kind {
            StatementKind::Assign { place, rvalue: Rvalue::Use(_) }
                if place.local == Local::from_index(1) =>
            {
                Some(place.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no assignment to the parameter: {}", lowered.dump("set")));
    assert_eq!(describe(&target), "*", "{}", lowered.dump("set"));
}

/// A reference **is** reassignable when the value is a reference too, so the
/// walk stops on a type equality rather than on the shape of the place.
#[test]
fn a_reference_assigned_a_reference_is_not_dereferenced() {
    let source = concat!(
        "def f(a: &Int, b: &Int) -> Int:\n",
        "    let mutable r be a\n",
        "    r be b\n",
        "    r\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let binding = body
        .locals()
        .find(|(_, decl)| matches!(decl.kind, science_mir::mir::LocalKind::Binding(_)))
        .map(|(local, _)| local)
        .unwrap_or_else(|| panic!("no binding: {}", lowered.dump("f")));
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            if let StatementKind::Assign { place, .. } = &statement.kind {
                if place.local == binding {
                    assert_eq!(
                        describe(place),
                        "",
                        "a reference assigned a reference is written whole: {}",
                        lowered.dump("f")
                    );
                }
            }
        }
    }
}

/// A call's result has no storage until something gives it some, and
/// `as_place` used to have no arm for it — every other arm recurses to
/// storage that already exists. The fallback was `Rvalue::Error`, a hole for
/// a program `sciencec check` had already accepted, which is
/// `examples/03_structs.science`'s `print(make_pair().first)`.
#[test]
fn a_field_of_a_calls_result_is_read_off_a_temporary() {
    // `second` is a `String` and not a second `Int`, so the temporary the
    // call's result is materialised into is not `moves::needs_drop`'s trivial
    // case — a fixture of two `Int`s would prove the projection and nothing
    // about ownership, because there would be no drop to elaborate either way.
    let source = concat!(
        "type Pair:\n",
        "    first: Int\n",
        "    second: String\n",
        "\n",
        "def make_pair() -> Pair:\n",
        "    Pair(first: 1, second: \"two\")\n",
        "\n",
        "def f() -> Int:\n",
        "    make_pair().first\n",
    );
    let lowered = lower(source);
    assert!(
        !lowered.statements("f").iter().any(|s| s == "assign error"),
        "a field of a call's result degraded to a hole: {}",
        lowered.dump("f")
    );
    assert!(
        projections(&lowered, "f").contains(&"f".to_string()),
        "the call's result was not projected into a field: {}",
        lowered.dump("f")
    );
    // `.first` is read by `Copy`, so the temporary stays wholly initialised —
    // `drops` elaborates exactly one (unconditional) `Drop` for it, at its own
    // scope's exit, which releases `second`'s buffer once and not twice.
    let drops = lowered.terminators("f").iter().filter(|kind| **kind == "drop").count();
    assert_eq!(
        drops,
        1,
        "the call's temporary must be dropped exactly once: {}",
        lowered.dump("f")
    );
}

/// The same sentence about a **method** call, which had no arm at all.
///
/// `a.scaled(2.0).x` passed `sciencec check` and reached the backend as
/// `Rvalue::Error`: `as_place`'s materialising arm names `ExprKind::Call` and
/// the `Field` arm's `as_place(base)` therefore answered `None` for an
/// `ExprKind::MethodCall` base. Binding the result first worked, and a free
/// function's result worked, which is what made the shape exactly this one.
///
/// **The arm above is not widened to `ExprKind::MethodCall`, and this test
/// exists partly to say so.** A `None` from `as_place` is also how
/// `borrow_source` learns that a `for` subject is a value the loop owns;
/// answering `Some` for a method call turns `for c in text.chars():` into a
/// *shared* borrow of the `Chars` temporary it then calls
/// `def next(mutable self)` through — `iteration.rs`'s
/// `a_subject_with_no_place_is_borrowed_through_a_temporary` and
/// `the_next_call_reads_through_the_loops_reference` are that invariant, and
/// they fail on exactly that swap. The storage is given in the `Field` arm,
/// where it is needed, and nowhere else.
#[test]
fn a_field_of_a_method_calls_result_is_read_off_a_temporary() {
    // `second: String` for the reason the test above gives: a fixture of two
    // `Int`s would prove the projection and nothing about ownership, because
    // there would be no drop to elaborate either way.
    let source = concat!(
        "type Pair:\n",
        "    first: Int\n",
        "    second: String\n",
        "\n",
        "Pair has:\n",
        "    def again(self) -> Pair:\n",
        "        Pair(first: self.first, second: \"two\")\n",
        "\n",
        "def f(p: Pair) -> Int:\n",
        "    p.again().first\n",
    );
    let lowered = lower(source);
    assert!(
        !lowered.statements("f").iter().any(|s| s == "assign error"),
        "a field of a method call's result degraded to a hole: {}",
        lowered.dump("f")
    );
    assert!(
        projections(&lowered, "f").contains(&"f".to_string()),
        "the method call's result was not projected into a field: {}",
        lowered.dump("f")
    );
    // Two owners and so two drops: `p`, the parameter this body owns, and the
    // temporary `again()`'s result was materialised into. `.first` is read by
    // `Copy`, so the temporary stays wholly initialised and its drop is
    // unconditional — it releases the `String` its `second` holds once, and
    // the count is what says the materialisation did not invent a second
    // owner of the same buffer.
    let drops = lowered.terminators("f").iter().filter(|kind| **kind == "drop").count();
    assert_eq!(
        drops,
        2,
        "the receiver and the method call's temporary are dropped once each: {}",
        lowered.dump("f")
    );
}

/// One fixture for the three tests below, because they are three assertions
/// about one lowering and a second copy of the program would be a second thing
/// to keep in step.
///
/// `label: String` on the receiver for the reason the two tests above give — a
/// record of scalars would prove the projection and nothing about ownership —
/// and `Array of Int` for the same reason on the other side: the temporary the
/// method's result is materialised into owns a heap buffer, so its drop is a
/// real one and a count can tell a second owner from none.
const INDEX_OF_A_METHOD_CALL: &str = concat!(
    "type Holder:\n",
    "    n: Int\n",
    "    label: String\n",
    "\n",
    "Holder has:\n",
    "    def items(self) -> Array[Int]:\n",
    "        [1, 2, 3]\n",
    "\n",
    "def f(h: Holder) -> Int:\n",
    "    h.items()[1]\n",
);

/// Every message a body hands to `science_panic_bytes`, in block order.
/// `division.rs`'s own helper, for the other guard that reports through it.
fn panics(lowered: &support::Lowered, name: &str) -> Vec<String> {
    lowered
        .body(name)
        .blocks()
        .filter_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Runtime("science_panic_bytes"), args, .. } => {
                match args.first() {
                    Some(Operand::Const(Constant::Literal(Literal::Str(text)))) => {
                        Some(text.clone())
                    }
                    _ => Some(String::from("<not a literal>")),
                }
            }
            _ => None,
        })
        .collect()
}

/// The local a [`Callee::Def`] call's result was written into — the storage the
/// base had to be given, named by the one thing that can name it.
fn call_destination(lowered: &support::Lowered, name: &str) -> Local {
    let mut found = lowered.body(name).blocks().filter_map(|(_, block)| {
        match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Def { .. }, destination, .. } => {
                Some(destination.local)
            }
            _ => None,
        }
    });
    let one = found.next().expect("the fixture calls exactly one Science function");
    assert!(found.next().is_none(), "the fixture calls exactly one Science function");
    one
}

/// Every `Rvalue::Ref` whose referent ends in an index — the element borrow.
fn element_borrows(lowered: &support::Lowered, name: &str) -> Vec<Place> {
    let mut out = Vec::new();
    for (_, block) in lowered.body(name).blocks() {
        for statement in &block.statements {
            let StatementKind::Assign { rvalue: Rvalue::Ref { place, .. }, .. } = &statement.kind
            else {
                continue;
            };
            if matches!(place.projection.last(), Some(Projection::Index { .. })) {
                out.push(place.clone());
            }
        }
    }
    out
}

/// The `Field` arm's sentence about an **index**, which is where `as_place`'s
/// last `?` on a place-less base was.
///
/// `h.items()[0]` passed `sciencec check` and reached the backend as
/// `Rvalue::Error` — §5 of `BUG-field-of-method-result.md` reproduced it and
/// left it — because `as_place`'s `ExprKind::Index` arm gave up on a base with
/// no place of its own exactly as the `Field` arm used to.
///
/// **The arm above is still not widened to `ExprKind::MethodCall`**, for the
/// reason the test above states and `iteration.rs`'s two loop-borrow tests
/// enforce: a `None` from `as_place` is also how `borrow_source` learns a `for`
/// subject is the loop's own. The storage is given where it is needed and
/// nowhere else, which is now two arms rather than one.
#[test]
fn an_index_of_a_method_calls_result_is_read_off_a_temporary() {
    let lowered = lower(INDEX_OF_A_METHOD_CALL);
    assert!(
        !lowered.statements("f").iter().any(|s| s == "assign error"),
        "an index of a method call's result degraded to a hole: {}",
        lowered.dump("f")
    );
    assert!(
        projections(&lowered, "f").contains(&"i".to_string()),
        "the method call's result was not projected into an index: {}",
        lowered.dump("f")
    );
    // Two owners, two drops: `h`, whose `label` is a `String`, and the
    // `Array of Int` the call's result was materialised into. The element is
    // read by `Copy`, so the array stays wholly initialised and its drop is
    // unconditional — it releases the buffer once, and the count is what says
    // the materialisation did not invent a second owner of it.
    let drops = lowered.terminators("f").iter().filter(|kind| **kind == "drop").count();
    assert_eq!(
        drops,
        2,
        "the receiver and the method call's temporary are dropped once each: {}",
        lowered.dump("f")
    );
}

/// §2.4's bounds check is emitted over the *temporary*, which is the half of
/// this that `Field` could not have carried over.
///
/// A field projection needs no guard; an index does, and `bounds_check` reads
/// the length through a borrow of the place it is given. If the base were
/// materialised anywhere but in front of the check — or if the check were
/// skipped because `is_array` was asked of the wrong place — this body would
/// still have its index projection and still print the right answer for every
/// index that happens to be in range.
#[test]
fn an_index_of_a_method_calls_result_is_bounds_checked() {
    let lowered = lower(INDEX_OF_A_METHOD_CALL);
    let body = lowered.body("f");
    assert_eq!(
        panics(&lowered, "f"),
        vec!["index out of bounds"],
        "the guard was not emitted for a place-less base: {}",
        lowered.dump("f")
    );

    // The length is read through a borrow of the materialised temporary, and
    // not of the receiver or of anything else in scope. `science_array_len`'s
    // argument is the reference; the reference's own referent is the array.
    let base = call_destination(&lowered, "f");
    let reference = body
        .blocks()
        .find_map(|(_, block)| match &block.terminator.kind {
            TerminatorKind::Call { callee: Callee::Runtime("science_array_len"), args, .. } => {
                args.first().and_then(|arg| arg.place()).map(|place| place.local)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no length call: {}", lowered.dump("f")));
    let referent = body
        .blocks()
        .flat_map(|(_, block)| &block.statements)
        .find_map(|statement| match &statement.kind {
            StatementKind::Assign { place, rvalue: Rvalue::Ref { place: referent, .. } }
                if place.local == reference =>
            {
                Some(referent.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("the length call's argument is not a borrow"));
    assert_eq!(
        referent,
        Place::local(base),
        "the length was read off something other than the call's own temporary: {}",
        lowered.dump("f")
    );
}

/// The element borrow names the temporary the call's result went into.
///
/// This is the other half `Field` could not have carried: reading `xs[i]` goes
/// through `read_ergonomic`, which builds a real `&Int` at the element's
/// address, so an index of a place-less base produces a reference *into* the
/// materialised storage. Rooting it anywhere else — a second temporary holding
/// a copy, say — would be a reference to a value nothing else owns, and every
/// in-range read would still print the right number.
///
/// **It is also why `let one be h.items()[1]` is now `SC0333` rather than
/// `SC0400`.** The reference outlives the array it points into; the array dies
/// at the close of the statement that built it. That diagnostic is rule 5
/// telling the truth about this lowering, and the borrow this test names is
/// the loan it is about.
#[test]
fn the_element_borrow_of_an_indexed_method_call_names_the_temporary() {
    let lowered = lower(INDEX_OF_A_METHOD_CALL);
    let body = lowered.body("f");
    let borrows = element_borrows(&lowered, "f");
    assert_eq!(borrows.len(), 1, "expected one element borrow: {}", lowered.dump("f"));
    let borrow = &borrows[0];

    let base = call_destination(&lowered, "f");
    assert_eq!(
        borrow.local,
        base,
        "the element was borrowed out of something other than the call's own \
         temporary: {}",
        lowered.dump("f")
    );
    assert_eq!(
        body.local_decl(base).kind,
        LocalKind::Temp,
        "the base must be storage this lowering introduced, which the author \
         cannot name: {}",
        lowered.dump("f")
    );
    assert_eq!(borrow.projection.len(), 1, "no step belongs above the index: {}", lowered.dump("f"));
    // The invariant `index_temporaries_are_assigned_once` states, asked again
    // on the path that builds the base as well as the index: `Projection::Index`
    // equality is *the same temporary means the same element*, and a base that
    // is itself materialised is the shape most likely to reuse a slot.
    assert!(
        body.index_temps_are_single_assignment(),
        "an index temporary was written twice: {}",
        lowered.dump("f")
    );
}

/// Presence-narrowing wraps a read as `ExprKind::Narrow(Local(r))`.
/// `as_place`'s `Narrow` arm sees through to the raw local — correct for
/// *storage* — but `record_of`/`field_ty` used to read the place's
/// *declared* type, `Record?`, a `Nullable` and not a `Named`, and degrade to
/// a hole. `examples/19_stdlib.science`'s `if r?: r.hits` is the shape.
#[test]
fn a_field_of_a_narrowed_nullable_reads_the_narrowed_type() {
    let source = concat!(
        "type Record:\n",
        "    hits: Int\n",
        "\n",
        "def f(r: Record?) -> Int:\n",
        "    if r?:\n",
        "        return r.hits\n",
        "    0\n",
    );
    let lowered = lower(source);
    assert!(
        !lowered.statements("f").iter().any(|s| s == "assign error"),
        "a field of a narrowed nullable degraded to a hole: {}",
        lowered.dump("f")
    );
    assert!(
        lowered.statements("f").iter().any(|s| s == "assign narrow"),
        "the narrowed read did not materialise `Rvalue::Narrow`: {}",
        lowered.dump("f")
    );
}

/// §4 again, on a `match` scrutinee: the tag read is the *referent's*.
///
/// `def name_of(format: borrowed Format)` is how the corpus spells every
/// `match` over a choice it does not own. Without the step the discriminant
/// read and every `Downcast` under it name the local holding the reference,
/// which `science-codegen-llvm` refused as *"a discriminant read of a value
/// that is not a `choice`"* — a message about a type, for a missing
/// dereference.
#[test]
fn a_match_on_a_borrowed_choice_reads_through_the_borrow() {
    let source = concat!(
        "choice Format:\n",
        "    Plain\n",
        "    Markdown\n",
        "\n",
        "def name_of(format: &Format) -> Int:\n",
        "    match format:\n",
        "        Plain: 1\n",
        "        Markdown: 2\n",
    );
    let lowered = lower(source);
    let body = lowered.body("name_of");
    let reads: Vec<String> = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter_map(|statement| match &statement.kind {
            StatementKind::Assign { rvalue: Rvalue::Discriminant(place), .. } => {
                Some(describe(place))
            }
            _ => None,
        })
        .collect();
    assert!(!reads.is_empty(), "the fixture must read a tag: {}", lowered.dump("name_of"));
    for read in &reads {
        assert_eq!(read, "*", "{}", lowered.dump("name_of"));
    }
}

/// Every tag read in a body, as its projection string.
fn tag_reads(lowered: &support::Lowered, name: &str) -> Vec<String> {
    lowered
        .body(name)
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter_map(|statement| match &statement.kind {
            StatementKind::Assign { rvalue: Rvalue::Discriminant(place), .. } => {
                Some(describe(place))
            }
            _ => None,
        })
        .collect()
}

/// Decision 28's AMENDMENT 6, the MIR half: a `match` on an owned `Box[Expr]`
/// reads the *payload's* tag — one `Deref` through the box, exactly the
/// projection `Builder::box_deref` already gives a method receiver — and a
/// binding that owns something is a shared loan of the payload, never a move
/// out of it.
#[test]
fn a_match_on_a_box_reads_the_payloads_tag_and_borrows_its_bindings() {
    let source = concat!(
        "choice Expr:\n",
        "    Number(Int)\n",
        "    Name(String)\n",
        "    Neg(Box[Expr])\n",
        "\n",
        "def show(boxed: Box[Expr]) -> Int:\n",
        "    match boxed:\n",
        "        Name(t): t.length()\n",
        "        _: 0\n",
    );
    let lowered = lower(source);
    let reads = tag_reads(&lowered, "show");
    assert_eq!(reads, vec!["*".to_string()], "{}", lowered.dump("show"));
    let body = lowered.body("show");
    let boxed = Local::from_index(1);
    let binding = body
        .borrows()
        .iter()
        .find(|data| data.place.local == boxed)
        .unwrap_or_else(|| panic!("`t` is not a loan of the box: {}", lowered.dump("show")));
    assert_eq!(describe(&binding.place), "*d", "{}", lowered.dump("show"));
    assert_eq!(binding.kind, science_mir::mir::BorrowKind::Shared);
}

/// Rule 6a lowered: `describe(inner)` with `inner: &Box[Expr]` passes a
/// reborrow of the place two `Deref`s in — through the borrow, then through
/// the box — and never an `Rvalue::Coerce`, which nothing downstream reads as
/// a loan. An owned box auto-borrowed at the same parameter borrows its own
/// payload directly: one `Deref`, and no loan of the box itself in between.
#[test]
fn a_borrow_through_a_box_is_a_reborrow_of_the_payload() {
    let source = concat!(
        "choice Expr:\n",
        "    Number(Int)\n",
        "    Neg(Box[Expr])\n",
        "\n",
        "def value(expr: &Expr) -> Int:\n",
        "    match expr:\n",
        "        Number(n): n\n",
        "        Neg(inner): 0 - value(inner)\n",
        "\n",
        "def owned(boxed: Box[Expr]) -> Int:\n",
        "    value(boxed)\n",
    );
    let lowered = lower(source);
    for name in ["value", "owned"] {
        let body = lowered.body(name);
        let coerced = body
            .blocks()
            .flat_map(|(_, block)| block.statements.iter())
            .any(|statement| {
                matches!(
                    &statement.kind,
                    StatementKind::Assign { rvalue: Rvalue::Coerce { .. }, .. }
                )
            });
        assert!(!coerced, "`{name}` lowered a coercion: {}", lowered.dump(name));
    }
    let through: Vec<String> = lowered
        .body("value")
        .borrows()
        .iter()
        .map(|data| describe(&data.place))
        .filter(|place| place.ends_with("**"))
        .collect();
    assert_eq!(through, vec!["**".to_string()], "{}", lowered.dump("value"));
    let owned: Vec<String> =
        lowered.body("owned").borrows().iter().map(|data| describe(&data.place)).collect();
    assert_eq!(owned, vec!["*".to_string()], "{}", lowered.dump("owned"));
}
