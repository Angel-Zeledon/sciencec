//! Decisions 7 and 8: the order, the summaries, and the fixpoint.

mod support;

use support::{check, codes};

/// §6.2's prescribed order, from the consumer's side: by the time a caller is
/// analysed, its callee's summary exists.
#[test]
fn a_callee_is_summarised_before_its_caller_is_analysed() {
    let source = "\
def inner(x: borrowed Int) -> borrowed Int:
    x

def middle(x: borrowed Int) -> borrowed Int:
    inner(x)

def outer(x: borrowed Int) -> borrowed Int:
    middle(x)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
    for name in ["inner", "middle", "outer"] {
        let (_, from) = &checked.analysis_of(name).summary.returns[0];
        assert_eq!(
            from.iter().map(|it| it.param).collect::<Vec<_>>(),
            vec![0],
            "`{name}` did not inherit its callee's relation"
        );
    }
}

/// And the relation is what crosses: `outer` returns a borrow of *its own*
/// parameter, which it can only know because `middle`'s summary said so — not
/// because of anything in `outer`'s body.
#[test]
fn the_relation_is_transitive_and_comes_from_the_summary() {
    let source = "\
def keep(a: borrowed Int, b: borrowed Int) -> borrowed Int:
    b

def caller(x: borrowed Int, y: borrowed Int) -> borrowed Int:
    keep(x, y)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new());
    let (_, from) = &checked.analysis_of("caller").summary.returns[0];
    assert_eq!(
        from.iter().map(|it| it.param).collect::<Vec<_>>(),
        vec![1],
        "the summary said `b` and the call site used `a`"
    );
}

/// Decision 8's component, and §2 of [`science_regions::analyse_crate`]: a
/// mutually recursive pair starts at *no relation* and grows to the least one
/// that explains both bodies.
#[test]
fn mutual_recursion_reaches_a_fixpoint() {
    let source = "\
def ping(x: borrowed Int, n: Int) -> borrowed Int:
    if n is 0:
        return x
    pong(x, n - 1)

def pong(x: borrowed Int, n: Int) -> borrowed Int:
    if n is 0:
        return x
    ping(x, n - 1)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));

    let fixpoints = checked.analysis.fixpoints();
    assert_eq!(fixpoints.len(), 1, "the two are not one component: {fixpoints:?}");
    assert_eq!(fixpoints[0].0.len(), 2, "the component has the wrong size");
    assert!(
        fixpoints[0].1 <= science_regions::MAX_COMPONENT_PASSES,
        "the fixpoint hit the tripwire"
    );

    for name in ["ping", "pong"] {
        let (_, from) = &checked.analysis_of(name).summary.returns[0];
        assert_eq!(
            from.iter().map(|it| it.param).collect::<Vec<_>>(),
            vec![0],
            "`{name}`'s summary did not converge on its parameter"
        );
    }
}

/// A self-recursive function is its own component and still needs the fixpoint,
/// because its first analysis reads its own summary.
#[test]
fn self_recursion_is_its_own_component() {
    let source = "\
def down(x: borrowed Int, n: Int) -> borrowed Int:
    if n is 0:
        return x
    down(x, n - 1)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new());
    assert_eq!(checked.analysis.fixpoints().len(), 1);
    let (_, from) = &checked.analysis_of("down").summary.returns[0];
    assert_eq!(from.iter().map(|it| it.param).collect::<Vec<_>>(), vec![0]);
}

/// The fixpoint's *optimistic* start is what makes the answer useful. Starting
/// from [`science_regions::Summary::opaque`] would also terminate and would
/// conclude that a recursive function returns a borrow of everything; this
/// asserts it does not.
#[test]
fn the_fixpoint_does_not_conclude_that_everything_is_borrowed() {
    let source = "\
def down(x: borrowed Int, other: borrowed Int, n: Int) -> borrowed Int:
    if n is 0:
        return x
    down(x, other, n - 1)
";
    let checked = check(source);
    let (_, from) = &checked.analysis_of("down").summary.returns[0];
    assert_eq!(
        from.iter().map(|it| it.param).collect::<Vec<_>>(),
        vec![0],
        "the fixpoint pulled in a parameter the body never returns"
    );
}

/// Decision 7's *"exactly the same information"*, made literal: the summary of
/// a one-parameter accessor is `fn f<'a>(x: &'a T) -> &'a U` with the name
/// deleted, and nothing about the body's block numbering is in it.
#[test]
fn a_summary_mentions_no_point_and_no_local() {
    let source = "\
def first(x: borrowed Int) -> borrowed Int:
    x
";
    let checked = check(source);
    let summary = &checked.analysis_of("first").summary;
    let rendered = format!("{summary:?}");
    assert!(!rendered.contains("bb"), "a block reached the summary: {rendered}");
    assert!(!rendered.contains("Point"), "a point reached the summary: {rendered}");
    assert!(!rendered.contains("Local"), "a local reached the summary: {rendered}");
}

/// A callee with no body in this crate — `print`, a builtin — is treated as
/// opaque, which [`science_regions::generate`]'s §7 says errs toward
/// rejecting. This asserts only that it does not *crash* and does not
/// manufacture an error on an ordinary program.
#[test]
fn a_builtin_callee_with_no_body_does_not_manufacture_an_error() {
    let source = "\
def go(x: borrowed Int):
    print(x)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
}
