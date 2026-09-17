//! Decisions 5 and 6: is elision total, and what is `SC0340` actually for?
//!
//! §5 says the replacement for Rust's elision rules *"has to be total"* and §14
//! calls `SC0340` *"the language's bet"*: *"if 'the body does not determine the
//! signature' fires in ordinary code rather than on pathological signatures,
//! the no-syntax promise is unkeepable and the fix is a syntax"*.
//!
//! [`science_regions`]'s §4 is the account. These are the cases.

mod support;

use support::{check, codes};

fn reported(source: &str) -> Vec<u16> {
    check(source).reported()
}

/// §5.2's own example of ambiguity: *"the signature does not say whether it is
/// borrowed from `x` or `y`"*.
///
/// **It is not ambiguous here.** The answer is the set `{x, y}`, the summary
/// records both, and a caller intersects them. Rust needs `'a` because Rust's
/// answer has to be one name; removing the syntax removed the problem it was
/// introduced to solve.
#[test]
fn a_return_borrowed_from_either_parameter_is_a_set_and_not_an_error() {
    let source = "\
def pick(x: borrowed Int, y: borrowed Int, left: Bool) -> borrowed Int:
    if left:
        return x
    y
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));

    let analysis = checked.analysis_of("pick");
    assert_eq!(analysis.summary.returns.len(), 1, "one reference in the return type");
    let (_, from) = &analysis.summary.returns[0];
    let params: Vec<usize> = from.iter().map(|it| it.param).collect();
    assert_eq!(params, vec![0, 1], "the summary lost one of the two sources");
}

/// The one-parameter case Rust's first elision rule covers, for completeness.
#[test]
fn a_return_borrowed_from_one_parameter_names_it() {
    let source = "\
def first(x: borrowed Int, y: borrowed Int) -> borrowed Int:
    x
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new());
    let (_, from) = &checked.analysis_of("first").summary.returns[0];
    assert_eq!(from.iter().map(|it| it.param).collect::<Vec<_>>(), vec![0]);
}

/// Rust's *third* elision rule — *"if there is a `&self`, its lifetime is
/// assigned to all elided output lifetimes"* — is a guess made before the body
/// is read. Here the body is read, and a method that returns a borrow of a
/// parameter rather than of `self` gets the right answer.
#[test]
fn a_method_that_returns_a_borrow_of_an_argument_is_not_assumed_to_return_self() {
    let source = "\
type Table:
    size: Int

Table has:
    def echo(self, other: borrowed Int) -> borrowed Int:
        other
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new());
    let (_, from) = &checked.analysis_of("echo").summary.returns[0];
    assert_eq!(
        from.iter().map(|it| it.param).collect::<Vec<_>>(),
        vec![1],
        "Rust's third rule would have said `self`; the body says otherwise"
    );
}

/// A signature with no reference in its return type has nothing to determine,
/// which is the overwhelming majority of functions and the reason `SC0340` is
/// rare before anything else is said about it.
#[test]
fn a_signature_with_no_borrow_in_the_return_has_no_regions() {
    let source = "\
def add(x: borrowed Int, y: borrowed Int) -> Int:
    1
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new());
    assert!(checked.analysis_of("add").summary.returns.is_empty());
}

/// Returning a borrow of a local is `SC0333` and **not** `SC0340`, which is
/// [`science_regions::check`]'s §5: the body does determine the signature, and
/// what it determines is not allowed.
#[test]
fn returning_a_borrow_of_a_local_is_rule_five_and_not_ambiguity() {
    let source = "\
def escape(x: borrowed Int) -> borrowed Int:
    let local be 5
    borrowed local
";
    assert_eq!(reported(source), vec![333]);
}

/// **The residue: the only program in F0 that reaches `SC0340`.**
///
/// Two mutually recursive functions whose only path to a result is each other.
/// The fixpoint of [`science_regions::analyse_crate`]'s §2 starts both at *no
/// relation*, neither body adds one, and the least solution is that the result
/// borrows from nothing — which is true, because no execution ever produces
/// one.
///
/// **This is what §14's bet comes down to.** The note feared `SC0340` would
/// *"fire in ordinary code"*. Across the corpus and every fixture in this
/// suite, the only shape that reaches it is a function that cannot return. A
/// program that has to be written differently because of `SC0340` has not been
/// found.
#[test]
fn only_a_function_that_never_returns_reaches_sc0340() {
    let source = "\
def a(x: borrowed Int) -> borrowed Int:
    b(x)

def b(x: borrowed Int) -> borrowed Int:
    a(x)
";
    let checked = check(source);
    assert_eq!(
        checked.reported(),
        vec![340, 340],
        "the residue of Decision 6 changed: {:?}",
        codes(&checked.regions)
    );
}

/// And a self-recursive one, for the same reason.
#[test]
fn a_self_recursive_function_that_never_returns_reaches_it_too() {
    let source = "\
def only(x: borrowed Int) -> borrowed Int:
    only(x)
";
    assert_eq!(reported(source), vec![340]);
}

/// **The suppression, asserted as itself.** A body that calls something with no
/// declaration does not report `SC0340`, because the return is unconstrained
/// on account of the missing signature rather than on account of the program.
/// [`science_regions::check`]'s §6.
#[test]
fn a_missing_container_declaration_suppresses_sc0340_rather_than_causing_it() {
    let source = "\
type Store:
    value: Int

Store has:
    def make() -> borrowed Int:
        (Array of Int).new().get(0)
";
    let checked = check(source);
    assert_eq!(
        checked.reported(),
        Vec::<u16>::new(),
        "the suppression is gone, and `stdlib-core.md`'s absence is being reported \
         as the author's mistake: {:?}",
        codes(&checked.regions)
    );
    let analysis = checked.analysis_of("make");
    assert!(analysis.calls_a_hole);
    assert!(
        !analysis.summary.undetermined().is_empty(),
        "the body would report SC0340 if the suppression were removed; if it no \
         longer would, the container has a declaration and the suppression can go"
    );
}

/// And on the corpus's own shapes it never fires. The acceptance case is
/// asserted separately; this is the smaller claim that the ordinary
/// borrow-returning method is determined.
#[test]
fn the_ordinary_accessor_is_determined() {
    let source = "\
type Def:
    name: Int

type Table:
    items: Array of Def

Table has:
    def get(self, at: Int) -> borrowed Def:
        self.items.get(at)

    def first(self) -> borrowed Def:
        self.get(0)
";
    let checked = check(source);
    assert_eq!(checked.reported(), Vec::<u16>::new(), "{:?}", codes(&checked.regions));
    for name in ["get", "first"] {
        let (_, from) = &checked.analysis_of(name).summary.returns[0];
        assert!(!from.is_empty(), "`{name}` is undetermined");
    }
}
