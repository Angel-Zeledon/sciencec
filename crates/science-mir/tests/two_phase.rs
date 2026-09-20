//! §10 item 4: two-phase borrows, which §14 calls *"a silent prerequisite"*.
//!
//! > *"`v.push(v.len())` needs the exclusive borrow of `v` to be reserved at the
//! > call and activated after the arguments are evaluated. This is not a region
//! > question but it is decided in the same pass, and if MIR does not expose the
//! > reservation point, the common case does not compile."*
//!
//! The last test is that sentence, run.

mod support;

use science_mir::mir::{BorrowKind, StatementKind};
use support::lower;

#[test]
fn a_shared_borrow_is_one_phase() {
    let source = concat!(
        "type Doc:\n",
        "    hits: Int\n",
        "\n",
        "def read(d: &Doc) -> Int:\n",
        "    d.hits\n",
        "\n",
        "def f(d: Doc) -> Int:\n",
        "    read(d)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    assert_eq!(body.borrows().len(), 1);
    assert_eq!(body.borrows()[0].kind, BorrowKind::Shared);
    assert!(body.borrows()[0].activation.is_none(), "a shared borrow has one point");
}

/// An exclusive borrow bound to a *name* is single-phase, because it is used an
/// unknown number of times. `lower`'s §6 is the argument.
#[test]
fn an_exclusive_borrow_bound_to_a_name_is_one_phase() {
    let source = concat!(
        "type Doc:\n",
        "    hits: Int\n",
        "\n",
        "def f(d: Doc):\n",
        "    let r be &mut d\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    assert_eq!(body.borrows().len(), 1);
    assert_eq!(body.borrows()[0].kind, BorrowKind::Exclusive);
    assert!(body.borrows()[0].activation.is_none());
}

#[test]
fn an_exclusive_borrow_in_argument_position_is_two_phase() {
    let source = concat!(
        "type Doc:\n",
        "    hits: Int\n",
        "\n",
        "def bump(d: &mut Doc):\n",
        "    d.hits be d.hits + 1\n",
        "\n",
        "def f(d: Doc):\n",
        "    bump(&mut d)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let borrow = &body.borrows()[0];
    assert_eq!(borrow.kind, BorrowKind::TwoPhase);
    let activation = borrow.activation.expect("a two-phase borrow has two points");
    assert!(
        borrow.reserved < activation
            || (borrow.reserved.block.index() < activation.block.index()),
        "reserved {} is not before activated {activation}",
        borrow.reserved
    );
}

/// The activation is a *statement*, not a derivation. `lower`'s §6: rustc finds
/// it as the borrow temporary's unique later use, and §10 item 4 asks MIR to
/// expose the reservation point rather than leave it to be found.
#[test]
fn the_activation_is_an_explicit_statement() {
    let source = concat!(
        "type Doc:\n",
        "    hits: Int\n",
        "\n",
        "def bump(d: &mut Doc):\n",
        "    d.hits be d.hits + 1\n",
        "\n",
        "def f(d: Doc):\n",
        "    bump(&mut d)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let id = body.borrows()[0].id;
    let found = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .any(|statement| statement.kind == StatementKind::Activate(id));
    assert!(found, "no `Activate` statement for the two-phase borrow");
}

/// §10 item 4's own example, and §14's risk, run.
///
/// The receiver of `push` is `mutable self`, so the borrow of `v` is reserved
/// before `v.len()` is evaluated and activated after — which is the whole of
/// what makes the line expressible. The shared borrow that `v.len()` takes sits
/// *between* the two points, and that is the case the rule exists for.
#[test]
fn push_of_len_reserves_before_the_argument_and_activates_after() {
    let source = concat!(
        "type Bag:\n",
        "    items: Array[Int]\n",
        "\n",
        "Bag has:\n",
        "    def len(self) -> Int:\n",
        "        self.items.len()\n",
        "\n",
        "    def push(mutable self, value: Int):\n",
        "        self.items.push(value)\n",
        "\n",
        "def f(b: Bag):\n",
        "    let mutable v be b\n",
        "    v.push(v.len())\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");

    let exclusive = body
        .borrows()
        .iter()
        .find(|data| data.kind == BorrowKind::TwoPhase)
        .expect("`v.push(..)` did not reserve a two-phase borrow of `v`");
    let shared = body
        .borrows()
        .iter()
        .find(|data| data.kind == BorrowKind::Shared)
        .expect("`v.len()` did not take a shared borrow of `v`");
    let activation = exclusive.activation.expect("the exclusive borrow was never activated");

    assert_eq!(exclusive.place, shared.place, "the two borrows are not of the same place");

    // The order that matters: reserve, then read, then activate.
    let order = |point: science_mir::Point| {
        body.points().position(|candidate| candidate == point).expect("a point of this body")
    };
    assert!(
        order(exclusive.reserved) < order(shared.reserved),
        "the exclusive borrow was not reserved before the argument was evaluated"
    );
    assert!(
        order(shared.reserved) < order(activation),
        "the exclusive borrow was activated before the argument was evaluated"
    );
}

/// The reservation set is identified syntactically — *an exclusive borrow in
/// argument position* — and §9's reborrow does not change which borrows are in
/// it.
///
/// A deref inserted at a receiver changes the place a borrow names, not where
/// the borrow stands: `mutable self` is still argument position, so the borrow
/// is still reserved and activated, and `two_phase_of` still finds it by the
/// temporary its reference was stored in.
#[test]
fn a_reborrowed_receiver_is_still_two_phase() {
    let source = concat!(
        "type Bag:\n",
        "    total: Int\n",
        "\n",
        "Bag has:\n",
        "    def add(mutable self, value: Int):\n",
        "        self.total be self.total + value\n",
        "\n",
        "    def add_twice(mutable self, value: Int):\n",
        "        self.add(value)\n",
        "        self.add(value)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("add_twice");
    assert_eq!(body.borrows().len(), 2, "each `self.add(..)` reserves one borrow");
    for data in body.borrows() {
        assert_eq!(
            data.kind,
            BorrowKind::TwoPhase,
            "a receiver reborrow left the reservation set"
        );
        assert!(data.activation.is_some(), "a two-phase borrow was never activated");
        assert!(
            !data.place.is_local(),
            "the receiver was &rather than reborrowed: {}",
            science_mir::dump::body(&lowered.krate.defs, body)
        );
    }
}
