//! §10 item 3: `StorageLive` and `StorageDead`, derived rather than invented.
//!
//! THIR's §5 handed MIR the material and not the item: *"every binding is
//! introduced by a `StmtKind::Let` in a known `Block`, so the point MIR emits
//! `StorageDead` at is derivable without a second analysis"*. These tests check
//! the derivation, and in particular the three cases where getting it wrong is
//! invisible in the easy one — an early `return`, a `break`, and a temporary
//! whose scope is one statement.

mod support;

use science_mir::mir::{LocalKind, StatementKind};
use support::lower;

fn storage_dead_count(lowered: &support::Lowered, body: &str, binding: &str) -> usize {
    let mir = lowered.body(body);
    let local = mir
        .locals()
        .find(|(_, decl)| {
            decl.def().map(|def| lowered.krate.defs.get(def).name == binding).unwrap_or(false)
        })
        .map(|(local, _)| local)
        .unwrap_or_else(|| panic!("no local for `{binding}`"));
    mir.storage_dead_points(local).len()
}

#[test]
fn a_binding_is_storage_live_at_its_let_and_dead_at_the_end_of_its_block() {
    let lowered = lower("def f() -> Int:\n    let x be 1\n    x\n");
    let body = lowered.body("f");
    let statements: Vec<&StatementKind> = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter().map(|s| &s.kind))
        .collect();
    let live = statements
        .iter()
        .position(|kind| matches!(kind, StatementKind::StorageLive(_)))
        .expect("a StorageLive");
    let dead = statements
        .iter()
        .position(|kind| matches!(kind, StatementKind::StorageDead(_)))
        .expect("a StorageDead");
    assert!(live < dead, "storage ended before it began");
}

/// The case that makes the scope stack necessary rather than decorative: a
/// `let`'s storage must outlive the statement that introduced it, and its
/// initialiser's temporaries must not.
#[test]
fn a_binding_outlives_its_statement_and_a_temporary_does_not() {
    let source = concat!(
        "def g() -> Int:\n",
        "    1\n",
        "\n",
        "def f() -> Int:\n",
        "    let x be g()\n",
        "    let y be g()\n",
        "    x + y\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    // `x` is read after `y` is bound, so its storage cannot have ended in
    // between. Its single `StorageDead` is at the end of the body's block.
    assert_eq!(storage_dead_count(&lowered, "f", "x"), 1);
    // Every local has exactly one `StorageLive` on a body with no branch.
    let lives = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|s| matches!(s.kind, StatementKind::StorageLive(_)))
        .count();
    let deads = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|s| matches!(s.kind, StatementKind::StorageDead(_)))
        .count();
    // The parameters have a `StorageDead` and no `StorageLive` (`lower`'s §2),
    // and `f` has none, so the two counts agree.
    assert_eq!(lives, deads, "storage statements do not pair");
}

/// A `return` leaves every scope, so a binding has one storage-dead point per
/// path out. Rule 5 is checked against *all* of them, which is why
/// [`Body::storage_dead_points`] returns a vector.
#[test]
fn an_early_return_ends_storage_on_its_own_path() {
    let source = concat!(
        "def f(c: Bool) -> Int:\n",
        "    let x be 1\n",
        "    if c:\n",
        "        return x\n",
        "    x\n",
    );
    let lowered = lower(source);
    assert_eq!(
        storage_dead_count(&lowered, "f", "x"),
        2,
        "one exit per path, and the early `return` is a path"
    );
}

#[test]
fn a_break_ends_the_storage_of_the_loop_body() {
    let source = concat!(
        "def f() -> Int:\n",
        "    loop:\n",
        "        let x be 1\n",
        "        break\n",
        "    0\n",
    );
    let lowered = lower(source);
    assert!(storage_dead_count(&lowered, "f", "x") >= 1);
}

/// `lower`'s §2: the return place and the drop flags get no storage statements,
/// and a parameter gets an end and no beginning.
#[test]
fn the_return_place_has_no_storage_statements() {
    let lowered = lower("def f() -> Int:\n    1\n");
    let body = lowered.body("f");
    let ret = science_mir::mir::RETURN_PLACE;
    assert!(body.storage_dead_points(ret).is_empty());
    assert!(matches!(body.local_decl(ret).kind, LocalKind::Return));
}

#[test]
fn a_parameter_has_an_end_and_no_beginning() {
    let lowered = lower("def f(n: Int) -> Int:\n    n\n");
    let body = lowered.body("f");
    let param = body.params().next().expect("one parameter");
    assert_eq!(body.storage_dead_points(param).len(), 1);
    let lives = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|s| s.kind == StatementKind::StorageLive(param))
        .count();
    assert_eq!(lives, 0, "a parameter is live from entry; there is no point before it");
}
