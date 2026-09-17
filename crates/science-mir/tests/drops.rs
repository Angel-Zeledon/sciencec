//! §10 item 5, and Decision 26's flags.
//!
//! §12 is specific about the thing that decides whether this is affordable:
//! *"flags are generated **only** for locals the move analysis proves
//! conditionally moved, never for every local, which is the difference between
//! a rare cost and a tax on every function"*. The first two tests are that
//! difference; the third is §12's own example.

mod support;

use science_mir::mir::{LocalKind, TerminatorKind};
use support::lower;

/// The fixture §12 writes out, with `Doc` in place of its `Doc`.
const CONDITIONAL_MOVE: &str = concat!(
    "type Doc:\n",
    "    title: String\n",
    "\n",
    "def consume(d: Doc):\n",
    "    let taken be d\n",
    "\n",
    "def f(urgent: Bool):\n",
    "    let doc be Doc(title: \"a\")\n",
    "    if urgent:\n",
    "        consume(doc)\n",
);

fn drops(lowered: &support::Lowered, name: &str) -> Vec<Option<science_mir::Local>> {
    lowered
        .body(name)
        .blocks()
        .filter_map(|(_, block)| match block.terminator.kind {
            TerminatorKind::Drop { flag, .. } => Some(flag),
            _ => None,
        })
        .collect()
}

/// The *rare cost* half. A body with no conditional move has no flags at all.
#[test]
fn an_ordinary_body_gets_no_drop_flags() {
    let source = concat!(
        "type Doc:\n",
        "    title: String\n",
        "\n",
        "def f():\n",
        "    let doc be Doc(title: \"a\")\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    assert!(body.drop_flags().is_empty(), "a flag on a body with no conditional move");
    assert_eq!(drops(&lowered, "f"), vec![None], "the one drop is unconditional");
}

/// A local that needs no drop gets no drop, so it cannot get a flag either.
#[test]
fn a_local_that_needs_no_drop_gets_no_drop() {
    let lowered = lower("def f():\n    let n be 1\n");
    assert!(drops(&lowered, "f").is_empty(), "an `Int` was dropped");
}

/// The *tax* half, which must not happen: a local moved on **every** path is
/// not conditionally moved, so the drop is deleted rather than flagged.
#[test]
fn an_unconditional_move_deletes_the_drop_rather_than_flagging_it() {
    let source = concat!(
        "type Doc:\n",
        "    title: String\n",
        "\n",
        "def consume(d: Doc):\n",
        "    let taken be d\n",
        "\n",
        "def f():\n",
        "    let doc be Doc(title: \"a\")\n",
        "    consume(doc)\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    assert!(body.drop_flags().is_empty(), "an unconditional move was given a flag");
    assert!(drops(&lowered, "f").is_empty(), "a moved-out local was still dropped");
}

/// §12's example, and the whole of Decision 26.
#[test]
fn a_conditionally_moved_local_gets_exactly_one_flag() {
    let lowered = lower(CONDITIONAL_MOVE);
    let body = lowered.body("f");
    let flags = body.drop_flags();
    assert_eq!(flags.len(), 1, "expected one flag, got {flags:?}");

    let (guarded, flag) = flags[0];
    assert!(
        matches!(body.local_decl(flag).kind, LocalKind::DropFlag(owner) if owner == guarded),
        "the flag does not name the local it guards"
    );
    let name = body
        .local_decl(guarded)
        .def()
        .map(|def| lowered.krate.defs.get(def).name.to_string());
    assert_eq!(name.as_deref(), Some("doc"), "the wrong local was flagged");

    // One byte and one branch: the drop that tests it, and no other drop.
    assert_eq!(drops(&lowered, "f"), vec![Some(flag)]);
}

/// The flag is written where the analysis says the state changes, not where the
/// source says a `let` or a move appears. `drops`'s §2 is the argument; this is
/// the observable consequence, which is that both writes exist.
#[test]
fn the_flag_is_set_at_the_initialisation_and_cleared_at_the_move() {
    let lowered = lower(CONDITIONAL_MOVE);
    let body = lowered.body("f");
    let (_, flag) = body.drop_flags()[0];
    let writes: Vec<bool> = body
        .blocks()
        .flat_map(|(_, block)| block.statements.iter())
        .filter_map(|statement| match statement.kind {
            science_mir::StatementKind::SetDropFlag { flag: written, value } if written == flag => {
                Some(value)
            }
            _ => None,
        })
        .collect();
    assert!(writes.contains(&true), "the flag is never set");
    assert!(writes.contains(&false), "the flag is never cleared");
}

/// Drop elaboration runs *during* construction — Decision 26 — so no caller can
/// observe a body whose drops are still unconditional.
#[test]
fn elaboration_has_already_run_when_a_body_is_handed_out() {
    let lowered = lower(CONDITIONAL_MOVE);
    let body = lowered.body("f");
    // If elaboration had not run, `doc` would have an unflagged drop and no
    // flag would exist at all.
    assert_eq!(body.drop_flags().len(), 1);
    assert!(drops(&lowered, "f").iter().all(Option::is_some));
}

/// A drop is an exclusive borrow at a point the user did not write, which is
/// §10 item 5's whole reason for existing. It must therefore be a point.
#[test]
fn a_drop_is_a_terminator_and_so_has_a_point() {
    let source = concat!(
        "type Doc:\n",
        "    title: String\n",
        "\n",
        "def f():\n",
        "    let doc be Doc(title: \"a\")\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let dropping = body
        .blocks()
        .find(|(_, block)| matches!(block.terminator.kind, TerminatorKind::Drop { .. }))
        .expect("a drop");
    let point = dropping.1.terminator_point(dropping.0);
    assert!(body.is_terminator(point));
    assert!(body.points().any(|candidate| candidate == point));
}
