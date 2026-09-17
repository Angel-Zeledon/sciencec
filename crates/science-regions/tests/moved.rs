//! §6.1 rule 3 — `SC0301` — one program per condition, and the message read
//! rather than counted.
//!
//! Every test here is a *program*, for `tests/conflicts.rs`'s reason: the
//! message is a narrative over the author's spans, and a fixture built out of a
//! hand-made [`science_mir::mir::Body`] would assert that the check works on an
//! input the lowering never produces.
//!
//! # What this file is the record of
//!
//! `SC0301` was allocated by the core spec, named by six design notes and
//! **constructed by no crate in the workspace**. `tests/ui/README.md` said so
//! in as many words — *"`SC0301` and `SC0302` are the core spec's and need a
//! move analysis `science-mir` declines to share"* — and the program that made
//! it matter was not an exotic one:
//!
//! ```text
//! let s be "hola"
//! print(s)
//! print(s)
//! ```
//!
//! which built, linked, ran, exited 0 and printed `hola` and an empty line.
//! Two independent defects met there: `print` moved what it should have read
//! (`science-mir`'s `lower` §5, and `science-codegen-llvm`'s `tests/printing.rs`
//! is the run), and *nothing checked rule 3*. Fixing the first makes that
//! program correct; this file is about the second, which is every program where
//! the move is real.
//!
//! # The four conditions, and the three tests that pin what is not reported
//!
//! [`crate::moved`]'s §3 states them. A check that only demonstrated its true
//! positives would be a check nobody could tell had a false-positive rate, so
//! each declined case is a test with the program in it: a conditional move, a
//! move out of a field, and a value that owns nothing.

mod support;

use support::check;

/// The whole diagnostic, rendered the way `sciencec check` renders it.
///
/// Rendered rather than matched on codes, because the thing under test is a
/// *message*: the two spans, which value is named in each, and which of them
/// carries the caret. A test that asserted `vec![301]` would pass with the
/// labels swapped.
fn rendered(source: &str) -> String {
    let checked = check(source);
    let mut map = science_diagnostics::source_map::SourceMap::new();
    map.add_file("moved.science".into(), source.to_string());
    science_diagnostics::render_all(&map, &checked.regions)
}

fn codes(source: &str) -> Vec<u16> {
    check(source).reported()
}

/// **Rule 3, reported, with the message read.**
///
/// The layout is the one `science-diagnostics`' `render` module documents for
/// this code — a secondary at the move, a primary at the use — and the
/// headline is that module's own sentence, because its worked example is the
/// only rendering of `SC0301` written down in this repository.
#[test]
fn a_moved_binding_cannot_be_used_again() {
    let source = "let s be \"hola\"\nlet t be s\nprint(s)\n";
    assert_eq!(
        rendered(source),
        "\
error[SC0301]: use of a moved value
 --> moved.science:3:1
  |
2 | let t be s
  |          - `s` is moved here
3 | print(s)
  | ^^^^^^^^ ...and used here, after the move
  |
  = note: every value has one owner, and assigning, passing or returning it \
transfers that owner (§6.1 rules 1 and 2), so the name is empty afterwards (rule 3)
  = note: borrow at the earlier use instead of moving, or give the second use a \
value of its own; a type that is not `Copy` is never copied implicitly"
    );
}

/// **A move into a call is a move**, and the second call says *moved again*.
///
/// The verb is the access's, not a fixed word: [`crate::moved::used`] reads it
/// off the [`crate::access::AccessKind`] so that a message about a second move
/// does not say *"used here"* about a line that is doing the same thing as the
/// one above it.
#[test]
fn passing_a_value_twice_by_value_is_reported_once() {
    let source = "\
def take(v: String) -> ():
    return

def main() -> ():
    let s be \"h\"
    take(s)
    take(s)
    take(s)
";
    assert_eq!(codes(source), vec![301], "one mistake, one message");
    assert!(
        rendered(source).contains("...and moved again here, after the move"),
        "the second move should be named as a move:\n{}",
        rendered(source)
    );
}

/// **The three-line program is correct now, and nothing is reported about
/// it.**
///
/// This is the half that keeps the two defects separate. `print` reads its
/// argument, so there is no move, so rule 3 has nothing to say — and a check
/// that reported here would have "fixed" the miscompile by refusing the program
/// instead.
#[test]
fn printing_a_binding_twice_is_not_a_move() {
    assert_eq!(codes("let s be \"hola\"\nprint(s)\nprint(s)\n"), Vec::<u16>::new());
}

/// **§3 item 1: a move on one path only is not reported.**
///
/// The analysis says [`science_mir::moves::State::Maybe`] here, which is
/// exactly the state Decision 26 generates a drop flag for — so the same lattice
/// value is reached by programs that are correct, and this check declines it.
/// rustc reports this program; the cost is stated at `moved`'s §3 and this is
/// the program it is stated about.
#[test]
fn a_conditional_move_is_not_reported() {
    let source = "let flag be true\nlet s be \"h\"\nif flag:\n    let t be s\nprint(s)\n";
    assert_eq!(codes(source), Vec::<u16>::new());
}

/// **§3 item 3: a move out of a field is not reported against the whole
/// value.**
///
/// `science-mir`'s `moves` §3 tracks whole locals, so moving `d.title` marks
/// `d` gone and a later `d.count` looks like a use after move. It is not one.
/// The acceptance case has this shape — `Scopes.lookup` compares
/// `binding.name` and then returns `binding.definition` — and it is why
/// [`crate::moved::move_site`] abandons a local whose reaching move went
/// through a projection.
#[test]
fn a_move_out_of_a_field_does_not_condemn_the_rest() {
    let source = "\
type Doc:
    title: String
    count: Int

def main() -> ():
    let d be Doc(title: \"t\", count: 1)
    let taken be d.title
    print(f\"{d.count}\")
";
    assert_eq!(codes(source), Vec::<u16>::new());
}

/// **§3 item 4: a value that owns nothing is not reported.**
///
/// Whether a user record is `Copy` is Decision 11's implementation lookup and
/// this compiler has none, so `science-mir`'s `lower` §5 calls every operand of
/// a non-primitive type a move. A record of scalars frees nothing and dangles
/// nothing when it is moved, so refusing this program would be charging the
/// author for a `Copy` the language would derive — which is what
/// `examples/21_compiler_shapes.science`'s `DefTable.alloc` does with a
/// `DefId`, and it is the program this condition was found on.
#[test]
fn a_record_of_scalars_is_not_reported() {
    let source = "\
type Id:
    index: Int

def take(id: Id) -> Int:
    id.index

def main() -> ():
    let id be Id(index: 1)
    let copied be id
    print(f\"{take(id)}\")
";
    assert_eq!(codes(source), Vec::<u16>::new());
}

/// **A parameter is a value like any other.**
///
/// It arrives holding one — `moves`' `analyse` seeds every parameter `Init` —
/// so a move out of it and a use afterwards is the same mistake as for a
/// binding, and the message names the parameter.
#[test]
fn a_moved_parameter_is_reported_against_its_name() {
    let source = "\
def take(v: String) -> ():
    return

def go(text: String) -> ():
    take(text)
    take(text)
";
    assert!(
        rendered(source).contains("`text` is moved here"),
        "the parameter should be named:\n{}",
        rendered(source)
    );
}

/// **Reassignment takes the name back**, which is §5's *"a write to a whole
/// local is not a use, it is a new value"*.
#[test]
fn a_reassigned_binding_is_usable_again() {
    let source = "\
def take(v: String) -> ():
    return

def main() -> ():
    let mutable s be \"uno\"
    take(s)
    s be \"dos\"
    take(s)
";
    assert_eq!(codes(source), Vec::<u16>::new());
}
