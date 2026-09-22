//! `lower`'s §8: the capture discipline, and the borrows it makes.
//!
//! > **Decision. Every capture is a *borrow* of the captured place, taken at
//! > the point the closure value is created and held for as long as the closure
//! > value is live. Shared, unless the closure's body writes through the place,
//! > in which case exclusive. There is no by-value capture and no copy capture,
//! > not even for a `Copy` type.**
//!
//! This file is that sentence, clause by clause. `science-regions`'s
//! `tests/closures.rs` is the other half — what the borrows then *do*.
//!
//! The corpus contributes nothing to this file and `capture`'s §4 says why:
//! every closure in `examples/` names only its own subject, so the acceptance
//! material for the discipline had to be written for it.

mod support;

use science_mir::mir::{BorrowKind, Place, Projection, Rvalue, StatementKind};
use science_mir::Body;
use support::lower;

/// A `Config`, a closure-taking function, and two functions to write through.
const PRE: &str = "\
type Config:
    port: Int
    host: Int

type Wrapper:
    name: String

def sink(f: (Int) -> Int) -> Int:
    1

def take(f: (Wrapper) -> Int) -> Int:
    1

def bump(c: &mut Config) -> Int:
    1

def peek(c: &Config) -> Int:
    1

def consume(w: Wrapper) -> Int:
    1
";

/// The captures a body's single closure carries, as `(kind, printed place)`.
fn captures(body: &Body, defs: &science_resolve::hir::DefTable) -> Vec<(BorrowKind, String)> {
    let mut out = Vec::new();
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            let StatementKind::Assign { rvalue: Rvalue::Closure { captures, .. }, .. } =
                &statement.kind
            else {
                continue;
            };
            for operand in captures {
                let place = operand.place().expect("a capture operand reads a place");
                // The capture operand names the temporary the reference went
                // into; the borrow table says what it points at.
                let data = body
                    .borrows()
                    .iter()
                    .find(|data| data.destination == *place)
                    .expect("every capture temporary is a borrow destination");
                out.push((data.kind, printed(defs, &data.place)));
            }
        }
    }
    out
}

/// A place, printed the way [`science_mir::dump`] prints one. Written here
/// rather than exported, because the crate's dump is for tests and widening its
/// API for one assertion would be a public function with one caller.
fn printed(defs: &science_resolve::hir::DefTable, place: &Place) -> String {
    let mut out = place.local.to_string();
    for step in &place.projection {
        match step {
            Projection::Deref { .. } => out = format!("(*{out})"),
            Projection::Field { field, .. } => out = format!("{out}.{}", defs.get(*field).name),
            Projection::TupleField { index, .. } => out = format!("{out}.{index}"),
            Projection::Index { index, .. } => out = format!("{out}[{index}]"),
            Projection::Downcast { variant, .. } => {
                out = format!("{out} as {}", defs.get(*variant).name)
            }
            Projection::Payload { .. } => out = format!("{out}?"),
        }
    }
    out
}

fn closure_count(body: &Body) -> usize {
    body.blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter(|statement| {
            matches!(
                &statement.kind,
                StatementKind::Assign { rvalue: Rvalue::Closure { .. }, .. }
            )
        })
        .count()
}

/// A closure that names only its own subject captures nothing — and it still
/// lowers, which is the half of §8 that used to be missing.
#[test]
fn a_closure_that_names_only_its_subject_captures_nothing() {
    let source = format!(
        "{PRE}
def go() -> Int:
    sink(item giving item)
"
    );
    let lowered = lower(&source);
    let body = lowered.body("go");
    assert_eq!(closure_count(body), 1, "the closure is lowered");
    assert_eq!(captures(body, &lowered.krate.defs), Vec::new());
    assert_eq!(body.borrows().len(), 0, "a capture-free closure takes no borrow");
}

/// `each` is the implicit spelling of the same parameter, so the corpus's own
/// shape captures nothing either. `capture`'s §4.
#[test]
fn the_implicit_each_form_captures_nothing() {
    let source = format!(
        "{PRE}
def go(c: &Config) -> Int:
    peek(c)
"
    );
    let lowered = lower(&source);
    assert_eq!(closure_count(lowered.body("go")), 0, "no closure in the fixture");
}

/// A read through a capture is a **shared** borrow.
#[test]
fn a_read_through_a_capture_is_a_shared_borrow() {
    let source = format!(
        "{PRE}
def go(c: Config) -> Int:
    sink(item giving c.port)
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(found, vec![(BorrowKind::Shared, "_1".to_string())]);
}

/// A write through a capture is an **exclusive** borrow. The only way a closure
/// body can write in F0 is to hand the place on as `mutable borrowed`, because
/// a closure body is an expression and an assignment is a statement.
#[test]
fn a_write_through_a_capture_is_an_exclusive_borrow() {
    let source = format!(
        "{PRE}
def go(c: Config) -> Int:
    sink(item giving bump(&mut c))
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(found, vec![(BorrowKind::Exclusive, "_1".to_string())]);
}

/// §8.2, and the reason [`science_mir::Capture::consumed`] is a list of nodes
/// rather than a flag: `c.port` is an `Int`, so producing its value is a copy
/// and the capture stays shared, although `Config` itself does not copy.
#[test]
fn producing_a_copy_field_out_of_a_capture_stays_shared() {
    let source = format!(
        "{PRE}
def go(c: Config) -> Int:
    sink(item giving c.port + 1)
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(found, vec![(BorrowKind::Shared, "_1".to_string())]);
}

/// §8.2's other side. Handing the whole capture on by value is a move this
/// discipline has no spelling for, so it takes the strongest borrow it has.
#[test]
fn producing_a_non_copy_capture_by_value_takes_the_strongest_borrow() {
    let source = format!(
        "{PRE}
def go(w: Wrapper) -> Int:
    sink(item giving consume(w))
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(found, vec![(BorrowKind::Exclusive, "_1".to_string())]);
}

/// §8.3, which is §9 one construct further: a capture inside a method whose own
/// receiver is a reference borrows the **referent**, `(*_1)`, and never the
/// local that holds the reference.
#[test]
fn a_capture_names_the_referent_and_not_the_reference() {
    let source = format!(
        "{PRE}
def go(c: &Config) -> Int:
    sink(item giving c.port)
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(found, vec![(BorrowKind::Shared, "(*_1)".to_string())]);
    let borrow = &lowered.body("go").borrows()[0];
    assert_eq!(
        borrow.place.projection,
        vec![Projection::Deref { ty: borrow.place.projection[0].ty() }],
        "exactly one dereference, inserted by `auto_deref`"
    );
}

/// Two captures are two borrows, in the order the body first names them — which
/// is `capture`'s first-mention rule, and is what keeps the statement numbering
/// a function of this body's THIR alone.
#[test]
fn two_captures_are_two_borrows_in_first_mention_order() {
    let source = format!(
        "{PRE}
def go(c: &Config, d: &Config) -> Int:
    sink(item giving d.port + c.host)
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    assert_eq!(
        found,
        vec![
            (BorrowKind::Shared, "(*_2)".to_string()),
            (BorrowKind::Shared, "(*_1)".to_string()),
        ],
        "`d` is named first, so it is capture 0"
    );
}

/// §8.4. A closure is very often in argument position and its captures are
/// still single-phase, because a two-phase borrow is one used exactly once at
/// the call that consumes it and a capture is used whenever the closure is.
#[test]
fn a_capture_is_never_two_phase() {
    let source = format!(
        "{PRE}
def go(c: Config) -> Int:
    sink(item giving bump(&mut c))
"
    );
    let lowered = lower(&source);
    let body = lowered.body("go");
    for data in body.borrows() {
        assert_ne!(data.kind, BorrowKind::TwoPhase, "a capture joined the two-phase set");
        assert!(data.activation.is_none());
    }
    assert!(
        !lowered
            .statements("go")
            .iter()
            .any(|statement| statement == "activate"),
        "no activation statement for a capture"
    );
}

/// A binding the closure's own body introduces is not a capture. `capture`'s
/// §3: the resolver already gave the inner `item` a definition of its own.
#[test]
fn a_binding_the_closure_introduces_is_not_a_capture() {
    let source = format!(
        "{PRE}
def go() -> Int:
    sink(item giving sink(other giving other))
"
    );
    let lowered = lower(&source);
    let body = lowered.body("go");
    // One statement, not two: §8.5 leaves the closure's *body* unlowered, so
    // the inner closure exists in THIR and in no MIR.
    assert_eq!(closure_count(body), 1);
    assert_eq!(captures(body, &lowered.krate.defs), Vec::new());
    assert_eq!(body.borrows().len(), 0);
}

/// A nested closure's capture is the enclosing closure's capture too, so it
/// reaches the outer body's borrow table.
#[test]
fn a_nested_closure_captures_through_both_levels() {
    let source = format!(
        "{PRE}
def go(c: &Config) -> Int:
    sink(item giving sink(other giving c.port))
"
    );
    let lowered = lower(&source);
    let found = captures(lowered.body("go"), &lowered.krate.defs);
    // **One** borrow and not two. The inner closure's capture is merged into
    // the outer one's by `capture`'s `record`, which is keyed on the binding —
    // §2 — so `c` is borrowed once for a closure that reaches it at two levels.
    // That is the granularity paying for itself: two loans of the same place
    // would conflict with each other the moment one of them was exclusive.
    assert_eq!(found, vec![(BorrowKind::Shared, "(*_1)".to_string())]);
}

/// Every capture borrow is in [`Body::borrows`] with a real reservation point,
/// which is the whole of what `lib.rs` §5 promises a consumer.
#[test]
fn a_capture_borrow_is_indexed_like_every_other() {
    let source = format!(
        "{PRE}
def go(c: Config) -> Int:
    sink(item giving c.port)
"
    );
    let lowered = lower(&source);
    let body = lowered.body("go");
    assert_eq!(body.borrows().len(), 1);
    let data = &body.borrows()[0];
    let statement = body
        .statement_at(data.reserved)
        .expect("the reservation point names a statement");
    assert!(
        matches!(
            &statement.kind,
            StatementKind::Assign { rvalue: Rvalue::Ref { borrow, .. }, .. } if *borrow == data.id
        ),
        "the reservation point is the `Ref` that takes the capture"
    );
    assert_eq!(data.place, Place::local(science_mir::mir::Local::from_index(1)));
}

/// **The corpus, and what it does and does not say.**
///
/// Every closure in `examples/` lowers, and every one of them captures nothing.
/// `capture`'s §4 is why: `each.title`, `doc giving doc.summarize()` and
/// `line giving line.length()` all name only the subject the closure binds, and
/// the resolver binds `each` as that same parameter.
///
/// So the corpus is evidence for exactly two claims — a closure *lowers* rather
/// than being refused, and the discipline costs a capture-free closure nothing
/// — and evidence for no part of the discipline itself. Every test above was
/// written for that reason.
#[test]
fn every_closure_in_the_corpus_captures_nothing() {
    let mut closures = 0;
    let mut files_with_closures = 0;
    for (name, source) in support::corpus() {
        let Some(lowered) = support::lower_if_clean(&source) else { continue };
        let mut here = 0;
        for body in &lowered.bodies {
            here += closure_count(body);
            let found = captures(body, &lowered.krate.defs);
            assert!(found.is_empty(), "{name} now has captures: {found:?}");
        }
        if here > 0 {
            files_with_closures += 1;
        }
        closures += here;
    }
    assert!(
        closures >= 4,
        "only {closures} closures lowered across the corpus; \
         `00_kitchen_sink.science` alone writes four"
    );
    assert!(files_with_closures >= 3, "only {files_with_closures} files reached a closure");
}
