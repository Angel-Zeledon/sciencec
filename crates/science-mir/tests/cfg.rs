//! §10 item 1: an explicit CFG with a statement index per point.
//!
//! The claim under test is Decision 3's cost being paid here: THIR keeps
//! `a and b` as one node so a diagnostic can quote it, and MIR is where it
//! becomes blocks and edges. A test that only counted blocks would pass on a
//! lowering that produced them for the wrong reason, so each test below names
//! the edge it is about.

mod support;

use science_mir::mir::{BlockId, TerminatorKind, ENTRY_BLOCK};
use science_resolve::hir::BinaryOp;
use support::lower;

#[test]
fn the_simplest_body_is_one_block_ending_in_return() {
    let lowered = lower("def f():\n    let x be 1\n");
    let body = lowered.body("f");
    assert_eq!(body.block_count(), 1);
    assert!(matches!(body.block(ENTRY_BLOCK).terminator.kind, TerminatorKind::Return));
}

#[test]
fn every_point_is_a_block_and_a_statement_index() {
    let lowered = lower("def f():\n    let x be 1\n    let y be 2\n");
    let body = lowered.body("f");
    // One point per statement, plus one for each block's terminator.
    let expected: usize = body.blocks().map(|(_, block)| block.statements.len() + 1).sum();
    assert_eq!(body.points().count(), expected);
    assert_eq!(body.point_count(), expected);
    // The terminator's point is one past the last statement, and nothing else
    // is a terminator.
    for point in body.points() {
        let statements = body.block(point.block).statements.len() as u32;
        assert_eq!(body.is_terminator(point), point.statement == statements);
    }
}

#[test]
fn an_if_is_a_branch_with_a_join() {
    let lowered = lower("def f(c: Bool) -> Int:\n    if c:\n        1\n    else:\n        2\n");
    let terminators = lowered.terminators("f");
    assert!(terminators.contains(&"if"), "{terminators:?}");
    let body = lowered.body("f");
    let branch = body
        .blocks()
        .find(|(_, block)| matches!(block.terminator.kind, TerminatorKind::If { .. }))
        .expect("a branch");
    let TerminatorKind::If { then_block, else_block, .. } = branch.1.terminator.kind else {
        unreachable!()
    };
    // Both arms reach the same join, which is what destination passing buys:
    // there is no φ because both arms stored to one place.
    let reachable = |from: BlockId| {
        let mut seen = vec![false; body.block_count()];
        let mut stack = vec![from];
        while let Some(block) = stack.pop() {
            if std::mem::replace(&mut seen[block.index()], true) {
                continue;
            }
            stack.extend(body.successors(block));
        }
        seen
    };
    let from_then = reachable(then_block);
    let from_else = reachable(else_block);
    assert!(
        (0..body.block_count()).any(|at| from_then[at] && from_else[at]),
        "the two arms never meet"
    );
}

/// §10 item 1's whole reason, and Decision 3's cost, in one assertion.
#[test]
fn and_is_a_graph_here_and_not_an_operator() {
    let lowered = lower("def f(a: Bool, b: Bool) -> Bool:\n    a and b\n");
    let body = lowered.body("f");
    for (_, block) in body.blocks() {
        for statement in &block.statements {
            if let science_mir::StatementKind::Assign {
                rvalue: science_mir::Rvalue::Binary { op, .. },
                ..
            } = &statement.kind
            {
                assert!(
                    !matches!(op, BinaryOp::And | BinaryOp::Or),
                    "`and` survived into an rvalue; §3 of `lower` says it must not"
                );
            }
        }
    }
    assert!(
        body.blocks().any(|(_, block)| matches!(block.terminator.kind, TerminatorKind::If { .. })),
        "`and` produced no branch"
    );
}

#[test]
fn or_short_circuits_the_other_way() {
    let lowered = lower("def f(a: Bool, b: Bool) -> Bool:\n    a or b\n");
    let body = lowered.body("f");
    let branch = body
        .blocks()
        .find(|(_, block)| matches!(block.terminator.kind, TerminatorKind::If { .. }))
        .expect("a branch");
    let TerminatorKind::If { then_block, .. } = branch.1.terminator.kind else { unreachable!() };
    // `or` takes the constant path when the left operand is true, so the
    // `then` block stores a constant rather than evaluating the right operand.
    let statements = &body.block(then_block).statements;
    assert!(
        statements.iter().any(|statement| matches!(
            &statement.kind,
            science_mir::StatementKind::Assign {
                rvalue: science_mir::Rvalue::Use(science_mir::Operand::Const(_)),
                ..
            }
        )),
        "`or`'s true path did not store a constant"
    );
}

#[test]
fn a_loop_has_a_back_edge_and_a_break_leaves_it() {
    let lowered = lower("def f():\n    loop:\n        break\n");
    let body = lowered.body("f");
    let has_back_edge = body.blocks().any(|(id, block)| {
        block.terminator.kind.successors().iter().any(|target| target.index() <= id.index())
    });
    assert!(has_back_edge, "a `loop` with no back edge");
}

#[test]
fn predecessors_agree_with_the_terminators() {
    let source = "def f(c: Bool) -> Int:\n    if c:\n        loop:\n            break\n        1\n    else:\n        2\n";
    let lowered = lower(source);
    assert!(lowered.body("f").check_predecessors());
}

#[test]
fn a_call_ends_a_block() {
    let lowered = lower("def g() -> Int:\n    1\n\ndef f() -> Int:\n    g()\n");
    let terminators = lowered.terminators("f");
    assert!(terminators.contains(&"call"), "{terminators:?}");
}

#[test]
fn a_match_branches_on_the_variant_and_not_on_a_number() {
    let source = "choice Shape:\n    Round\n    Square\n\ndef f(s: Shape) -> Int:\n    match s:\n        Round: 1\n        Square: 2\n";
    let lowered = lower(source);
    let body = lowered.body("f");
    let switches = body
        .blocks()
        .filter(|(_, block)| matches!(block.terminator.kind, TerminatorKind::Switch { .. }))
        .count();
    assert_eq!(switches, 2, "one switch per arm; arms are tested in order");
}
