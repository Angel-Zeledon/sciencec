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
//! come out it stays silent rather than guess. **One construct lands there
//! today**, and it used to be two: a method whose name the prelude has not
//! transcribed carries no type, so `let slot be items.pop()` gives the rule
//! nothing to read. It is pinned below as the silence it is.
//!
//! **The other one is closed, and the flip is the point.** This header used to
//! say *"a range has no type at all (`SC0538`'s note: there is no `Range` in
//! the prelude), so `for i in 0..3` cannot be told its `i` is not mutable"*.
//! `builtins.rs` now declares `Range of T implements Iterate: type Item is T`,
//! so `check`'s `iterate_item` reads a real element type off it and the loop
//! variable is an ordinary binding with an ordinary type. The pin below that
//! recorded the silence is now the refusal, and it changed by one word — the
//! rule already had the pattern arm, exactly as the old comment predicted.
//!
//! **The second one used to be stated as `items.get_mut(0)` and is narrower
//! now.** `methods`' §8a closed the surrounding hole: a method on a prelude
//! type *is* looked up, and a name no note gives is `SC0532`. `get_mut` is such
//! a name — neither `stdlib-core.md` nor `collections-and-chains.md` has it,
//! and mutable element access is `IndexMutably` — so that call is a diagnostic
//! rather than a silence. What is left is the narrower case below: `pop` is a
//! name `stdlib-core.md` §3.2 gives and `builtins.rs` has not written a
//! signature for, so it resolves to nothing and its binding has no type.

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

// --- the third spelling, which used to be a silence ------------------------

/// **The gap this file recorded, now closed.** It read: *"`for mutable i in
/// 0..3` parses and checks, so the word is accepted at a pattern; it is simply
/// never enforced there, because `0..3` carries no type and `i` therefore has
/// none either. When `Range` exists in the prelude this test flips to a `304`
/// and the change is one line."* `Range` exists, and this is the flip.
///
/// A pattern binding is the **third** spelling of `mutable` — `let` and a
/// parameter are the other two, above — and it is the one no test could reach
/// while the rule's precondition was a type the checker never synthesised.
#[test]
fn a_loop_variable_over_a_range_is_a_binding_like_any_other() {
    let checked = support::check(
        "\
def counted() -> Bool:
    for i in 0..3:
        i be 9
    true
",
    );
    assert_eq!(checked.codes(), vec![304]);
    assert_eq!(checked.messages(), vec!["`i` is not mutable"]);
}

/// And its pair, for this file's own rule: a rule that refuses everything is
/// not a rule. `for mutable i in …` is the spelling that makes the write legal,
/// and it has to keep working or the flip above is a regression wearing a
/// test's clothes.
#[test]
fn a_loop_variable_declared_mutable_may_be_written() {
    let checked = support::check(
        "\
def counted() -> Bool:
    for mutable i in 0..3:
        i be 9
    true
",
    );
    checked.assert_clean();
}

// --- the silence that is left, pinned --------------------------------------

/// The second silence the header names, pinned.
///
/// `pop` is in `builtins.rs`' `UNWRITTEN` — `stdlib-core.md` §3.2 gives the
/// name and no note gives the signature — so the call resolves to nothing,
/// `slot` has `Ty::ERROR`, and a rule that reads types has nothing to read.
/// Writing to it is a program `SC0304` would refuse if it could see it.
///
/// **This test flips the day `Array.pop` is transcribed**, and the entry in
/// `UNWRITTEN` goes with it, which is the handoff that comment describes.
#[test]
fn a_binding_from_an_untranscribed_prelude_method_has_no_type_to_check() {
    let checked = support::check(
        "\
def take(items: mutable borrowed Array of I64) -> Bool:
    let slot be items.pop()
    slot be 1
    true
",
    );
    checked.assert_clean();
}

/// And the half of that pair which is no longer a silence: a name **no** note
/// gives is reported, so the binding's missing type is at least explained.
#[test]
fn a_binding_from_a_name_no_note_gives_is_reported() {
    let checked = support::check(
        "\
def take(items: mutable borrowed Array of I64) -> Bool:
    let slot be items.get_mut(0)
    true
",
    );
    assert_eq!(checked.codes(), vec![532]);
}
