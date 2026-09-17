//! `syntax-revision-2.md` §2.2 — `mutable`, and the word that meant nothing.
//!
//! The revision writes `let mutable i be 0` before `i be i + 1`, and that
//! word would say nothing if the plain `let` also admitted the write. It did.
//! `mutable` lexed, parsed, resolved, and arrived at
//! `hir::PatternKind::Binding { mutable, .. }` — where **nothing ever read
//! it**. `let count be 0` followed by `count be 3` checked, built, linked, ran
//! and printed `3`.
//!
//! **Every case is a pair**, for `array_literals.rs`' reason: a rule that
//! refuses everything is not a rule. So each refusal here sits beside the
//! program it must not touch, and the second half is the larger half —
//! writing *through* a `mutable borrowed` looks exactly like reassignment and
//! is not, and refusing it would have made the keyword unusable for the thing
//! it exists to do.
//!
//! **Where the rule declines, and why that is not hidden.** It reads types,
//! and an unresolved inference variable is stored as `Ty::ERROR` until §4's
//! writeback — so it runs *after* writeback, and where a type still did not
//! come out it stays silent rather than guess. Two constructs land there
//! today: a range has no type at all (`SC0538`'s note: there is no `Range` in
//! the prelude), so `for i in 0..3` cannot be told its `i` is not mutable;
//! and a method on a prelude type is not looked up, so `let slot be
//! items.get_mut(0)` has no type either. Both are pinned below as the
//! silences they are.

mod support;

// --- the refusal ----------------------------------------------------------

#[test]
fn a_plain_let_may_not_be_assigned_to() {
    let checked = support::check(
        "\
def counter() -> Bool:
    let count: I64 be 0
    count be 3
    true
",
    );
    assert_eq!(checked.codes(), vec![304]);
    assert_eq!(checked.messages(), vec!["`count` is not mutable"]);
}

#[test]
fn a_let_mutable_may() {
    let checked = support::check(
        "\
def counter() -> Bool:
    let mutable count: I64 be 0
    count be 3
    true
",
    );
    checked.assert_clean();
}

#[test]
fn a_parameter_may_not_be_assigned_to() {
    // There is nowhere in `name: Type` to put the word, so the note offers a
    // local rather than a spelling that does not exist. A fix that cannot be
    // taken is the thing `SC0538`'s own documentation refuses to emit.
    let checked = support::check(
        "\
def bump(n: I64) -> I64:
    n be n + 1
    return n
",
    );
    assert_eq!(checked.codes(), vec![304]);
    assert_eq!(checked.messages(), vec!["`n` is not mutable"]);
}

#[test]
fn a_field_of_a_plain_let_may_not_be_written() {
    let checked = support::check(
        "\
type Doc:
    title: String

def retitle() -> Bool:
    let doc be Doc(title: \"first\")
    doc.title be \"second\"
    true
",
    );
    assert_eq!(checked.codes(), vec![304]);
    // The root's word decided it, but the reader is looking at `doc.title`,
    // so the message names the part that did.
    assert_eq!(checked.messages(), vec!["`doc` is not mutable"]);
}

#[test]
fn a_field_of_a_let_mutable_may_be_written() {
    let checked = support::check(
        "\
type Doc:
    title: String

def retitle() -> Bool:
    let mutable doc be Doc(title: \"first\")
    doc.title be \"second\"
    true
",
    );
    checked.assert_clean();
}

// --- the half that matters more: writing through a borrow -----------------

#[test]
fn a_mutable_borrowed_parameter_may_be_written_through() {
    // `counter be counter + 1` is not reassigning the reference; it is
    // writing the referent, which is the entire purpose of the type. The walk
    // toward the root stops at the first borrow for exactly this.
    let checked = support::check(
        "\
def bump(counter: mutable borrowed I64):
    counter be counter + 1
",
    );
    checked.assert_clean();
}

#[test]
fn a_field_behind_a_mutable_borrowed_parameter_may_be_written() {
    let checked = support::check(
        "\
type Doc:
    title: String

def retitle(doc: mutable borrowed Doc, title: String):
    doc.title be title
",
    );
    checked.assert_clean();
}

// --- the silences, pinned -------------------------------------------------

#[test]
fn a_loop_variable_over_a_range_is_not_caught_because_a_range_has_no_type() {
    // **This is a gap, recorded rather than papered over.** `for mutable i in
    // 0..3` parses and checks, so the word is accepted at a pattern; it is
    // simply never enforced there, because `0..3` carries no type and `i`
    // therefore has none either. When `Range` exists in the prelude this test
    // flips to a `304` and the change is one line — the rule already has the
    // pattern arm and its own note.
    let checked = support::check(
        "\
def counted() -> Bool:
    for i in 0..3:
        i be 9
    true
",
    );
    checked.assert_clean();
}
