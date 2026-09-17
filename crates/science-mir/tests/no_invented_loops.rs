//! §3.4's hole, kept open and checked.
//!
//! > **Decision 5.** In F0, a whole-array operation lowers to a *call* to a
//! > runtime or library entry point, never to an inlined MIR loop.
//!
//! §3.4's reason is that F1's fusion pass must have array operations to
//! schedule rather than loops to reverse-engineer — *"precisely the wound §7.1
//! says Julia carries"*. Nothing in F0's THIR is a whole-array operation, so
//! the decision is vacuous as written and cannot be tested as written.
//!
//! What *can* be tested is the stronger claim `lib`'s §4 makes instead:
//!
//! > **Every loop in a body's CFG comes from a `loop` or a `for` the author
//! > wrote. This crate synthesises no loop.**
//!
//! It is run over the whole corpus, because the claim is about the lowering and
//! not about any one program.

mod support;

use science_mir::mir::{reverse_postorder, Body, BlockId};
use support::{lower, lower_if_clean};

/// The distinct **loop headers** of a body: the targets of its back edges.
///
/// Headers and not edges, because one written loop can have several back edges
/// — a `continue` is a second edge to the same header — and the claim under
/// test is about how many *loops* the lowering built, not how many ways there
/// are back into them.
fn loop_headers(body: &Body) -> Vec<BlockId> {
    let mut headers: Vec<BlockId> = back_edges(body).into_iter().map(|(_, to)| to).collect();
    headers.sort();
    headers.dedup();
    headers
}

/// The back edges of a body: an edge to a block that dominates its source in
/// the depth-first order.
///
/// Approximated by reverse post-order position, which is exact for a reducible
/// CFG — and every CFG this crate builds is reducible, because the only edges
/// backwards are the ones `lower_loop` and `lower_for` write.
fn back_edges(body: &Body) -> Vec<(BlockId, BlockId)> {
    let order = reverse_postorder(body);
    let mut position = vec![usize::MAX; body.block_count()];
    for (at, block) in order.iter().enumerate() {
        position[block.index()] = at;
    }
    let mut out = Vec::new();
    for (id, block) in body.blocks() {
        if position[id.index()] == usize::MAX {
            continue;
        }
        for successor in block.terminator.kind.successors() {
            if position[successor.index()] <= position[id.index()] {
                out.push((id, successor));
            }
        }
    }
    out
}

fn loops_written(source: &str) -> usize {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with('#'))
        .filter(|line| *line == "loop:" || line.starts_with("for "))
        .count()
}

#[test]
fn a_body_with_no_loop_has_no_back_edge() {
    let source = concat!(
        "type Doc:\n",
        "    title: String\n",
        "    hits: Int\n",
        "\n",
        "def f(d: Doc, c: Bool) -> Int:\n",
        "    if c and d.hits > 0:\n",
        "        d.hits\n",
        "    else:\n",
        "        0\n",
    );
    let lowered = lower(source);
    assert!(loop_headers(lowered.body("f")).is_empty(), "a branch produced a loop");
}

#[test]
fn an_index_is_a_projection_and_not_a_loop() {
    let lowered = lower("def f(xs: Array of Int, i: Int) -> Int:\n    xs[i]\n");
    assert!(
        loop_headers(lowered.body("f")).is_empty(),
        "indexing lowered to a loop; §3.4's hole is closed"
    );
}

/// The corpus. Each example that lowers is checked to have no more loops in its
/// MIR than the author wrote in its source.
#[test]
fn no_example_acquires_a_loop_it_did_not_write() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&examples).expect("examples/") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("science") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a readable example");
        let Some(lowered) = lower_if_clean(&source) else { continue };
        let written = loops_written(&source);
        let built: usize = lowered.bodies.iter().map(|body| loop_headers(body).len()).sum();
        assert!(
            built <= written,
            "{}: {built} loop headers for {written} written loops",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 15, "only {checked} examples were lowered; the corpus has twenty-odd");
}

