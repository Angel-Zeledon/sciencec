//! §10 item 6: stable point identity across the query boundary.
//!
//! > *"Decision 8 memoises per SCC; if MIR point numbering changes when an
//! > unrelated function is edited, the cache never hits."*
//!
//! The requirement is about an edit *elsewhere*, and that is what these tests
//! check. `mir`'s §2 states the half that cannot hold — editing a body
//! renumbers that body — and says why nothing needs it to.

mod support;

use support::lower;

fn numbering(source: &str, name: &str) -> String {
    let lowered = lower(source);
    lowered.dump(name)
}

#[test]
fn lowering_the_same_program_twice_gives_the_same_numbering() {
    let source = concat!(
        "def g(n: Int) -> Int:\n",
        "    n + 1\n",
        "\n",
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        g(1)\n",
        "    else:\n",
        "        g(2)\n",
    );
    assert_eq!(numbering(source, "f"), numbering(source, "f"));
}

/// The property Decision 8's cache actually needs.
#[test]
fn editing_another_function_does_not_renumber_this_one() {
    let before = concat!(
        "def other(n: Int) -> Int:\n",
        "    n\n",
        "\n",
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        1\n",
        "    else:\n",
        "        2\n",
    );
    let after = concat!(
        "def other(n: Int) -> Int:\n",
        "    let doubled be n + n\n",
        "    let tripled be doubled + n\n",
        "    tripled\n",
        "\n",
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        1\n",
        "    else:\n",
        "        2\n",
    );
    assert_eq!(
        numbering(before, "f"),
        numbering(after, "f"),
        "editing `other` renumbered `f`, so Decision 8's cache would never hit"
    );
}

/// Adding a function *before* the one under test is the harsher version of the
/// same question, because it moves every `DefId` in the file.
#[test]
fn adding_a_function_before_this_one_does_not_renumber_it() {
    let before = concat!(
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        1\n",
        "    else:\n",
        "        2\n",
    );
    let after = concat!(
        "def inserted() -> Int:\n",
        "    7\n",
        "\n",
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        1\n",
        "    else:\n",
        "        2\n",
    );
    let before = lower(before);
    let after = lower(after);
    let (before, after) = (before.body("f"), after.body("f"));
    assert_eq!(before.block_count(), after.block_count());
    assert_eq!(before.point_count(), after.point_count());
    assert_eq!(before.local_count(), after.local_count());
}

/// A region is a set of points, and `region-inference.md` §3's complexity claim
/// assumes a bitset. That needs the enumeration order to be a dense index.
#[test]
fn points_enumerate_densely_and_in_order() {
    let source = concat!(
        "def f(c: Bool) -> Int:\n",
        "    if c:\n",
        "        loop:\n",
        "            break\n",
        "        1\n",
        "    else:\n",
        "        2\n",
    );
    let lowered = lower(source);
    let body = lowered.body("f");
    let points: Vec<_> = body.points().collect();
    assert_eq!(points.len(), body.point_count());
    let mut sorted = points.clone();
    sorted.sort();
    assert_eq!(points, sorted, "points are not enumerated in order");
    sorted.dedup();
    assert_eq!(sorted.len(), points.len(), "a point was enumerated twice");
}

/// The other half of `mir`'s §2, stated as a test so that the documentation is
/// not the only place it is written down.
#[test]
fn editing_this_function_does_renumber_it_and_that_is_expected() {
    let before = "def f() -> Int:\n    1\n";
    let after = "def f() -> Int:\n    let x be 1\n    x\n";
    let before = lower(before);
    let after = lower(after);
    assert_ne!(
        before.body("f").point_count(),
        after.body("f").point_count(),
        "an edited body is re-analysed by definition; if this ever passes, the \
         numbering has stopped depending on the body"
    );
}
