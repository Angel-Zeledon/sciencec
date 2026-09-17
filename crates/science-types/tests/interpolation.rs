//! `f"…"` through the checker: `strings-formatting-and-docs.md` §1.1, §1.6,
//! §3.4 and §7's `SC0275`.
//!
//! The claims are three, and the first is the one the whole exercise is for:
//! **`let n be 42` then `f"{n}"` checks clean and has type `String`.** The
//! second is that a hole is an ordinary expression with an ordinary type, so
//! nothing about it is special-cased. The third is that §3.4's four refusals —
//! a nullable, an array, a map, a closure — arrive as one diagnostic with a
//! name in it rather than as silence.

mod support;

use science_types::thir::ExprKind;
use support::check;

/// The body wrapped in a `def` the harness can find.
fn program(body: &str) -> String {
    format!("def main():\n{body}\n")
}

// --- the target ----------------------------------------------------------

#[test]
fn an_integer_binding_interpolates() {
    let checked = check(&program("    let n be 42\n    let line be f\"{n}\""));
    checked.assert_clean();
}

#[test]
fn a_float_binding_interpolates() {
    let checked = check(&program("    let x be 42.0\n    let line be f\"{x}\""));
    checked.assert_clean();
}

/// The type of the literal is `String`, whatever is in it. §1.7.
#[test]
fn an_f_string_has_type_string() {
    let checked = check(&program("    let n be 42\n    let line be f\"n is {n}\""));
    checked.assert_clean();
    let id = checked.find("main", |kind| matches!(kind, ExprKind::FString(_)));
    assert_eq!(checked.render(checked.body("main").expr(id).ty), "String");
}

/// An f-string with no holes is still an f-string, and still a `String`.
#[test]
fn an_f_string_with_no_holes_checks() {
    let checked = check(&program("    let line be f\"plain\""));
    checked.assert_clean();
    let id = checked.find("main", |kind| matches!(kind, ExprKind::FString(_)));
    assert_eq!(checked.render(checked.body("main").expr(id).ty), "String");
}

// --- the holes are ordinary expressions ----------------------------------

/// §1.4's case for arbitrary expressions: `frame.len()` is a method call, and
/// a rule that admitted `{n}` and not this would teach a boundary nobody can
/// predict.
#[test]
fn a_hole_may_be_a_call_and_is_checked_as_one() {
    let source = "\
def width() -> I64:
    768

def main():
    let line be f\"{width()} channels\"
";
    check(source).assert_clean();
}

#[test]
fn a_hole_may_be_arithmetic() {
    check(&program("    let n be 42\n    let line be f\"{n + 1}\"")).assert_clean();
}

/// A mistake inside a hole is reported as itself, at its own span, and not as
/// something about the literal.
#[test]
fn a_mistake_inside_a_hole_is_reported_as_itself() {
    let source = "\
type Doc:
    title: String

def main():
    let doc be Doc(title: \"a\")
    let line be f\"{doc.missing}\"
";
    // `SC0528`, no such field — the hole's own diagnostic.
    assert_eq!(support::check(source).codes(), vec![528]);
}

/// A `String` hole goes through `String implements Display`, which
/// `stdlib-core.md` §6.9 gives it.
#[test]
fn a_string_hole_checks() {
    check(&program("    let name be \"A12\"\n    let line be f\"{name}\"")).assert_clean();
}

#[test]
fn a_bool_and_a_char_hole_check() {
    check(&program("    let flag be true\n    let line be f\"{flag}\"")).assert_clean();
    check(&program("    let c be 'x'\n    let line be f\"{c}\"")).assert_clean();
}

// --- §3.4, through SC0275 ------------------------------------------------

/// §3.4: *"`T?` does not implement `Display`. … A silent `null` or an empty
/// cell in a published table is the failure this prevents."*
#[test]
fn a_nullable_hole_is_sc0275() {
    let source = "\
def main(value: I64?):
    let line be f\"{value}\"
";
    let checked = check(source);
    assert_eq!(checked.codes(), vec![275]);
    assert!(
        checked.messages()[0].contains("Display"),
        "the message names the interface: {:?}",
        checked.messages()
    );
}

/// The narrowed form is a `T` and prints, which is §3.4's own answer to the
/// case above.
#[test]
fn the_narrowed_form_of_a_nullable_prints() {
    let source = "\
def main(value: I64?):
    if value?:
        let line be f\"{value}\"
";
    check(source).assert_clean();
}

/// §3.4 decides that *"arrays and maps do not implement `Display`"*, and this
/// pins that **the decision is not enforced** — which is the finding rather
/// than the feature.
///
/// `builtins.rs`' `IMPLEMENTS` table has no `Array` row at all, deliberately:
/// `Array of T implements Clone` holds only where `T: Clone`, and a
/// conditional implementation is not something that index can express. So the
/// checker cannot tell *"`Array` does not implement `Display`"* from *"nobody
/// has written `Array`'s row yet"*, and `requires_display` refuses to report
/// the first on the evidence for the second.
///
/// The test asserts the silence so that whoever gives the prelude a way to
/// state a deliberate absence finds this line and deletes it.
#[test]
fn an_array_hole_is_not_reported_and_section_three_four_says_it_should_be() {
    let source = "\
def main(rows: Array of I64):
    let line be f\"{rows}\"
";
    assert_eq!(check(source).codes(), Vec::<u16>::new());
}

/// A user type with no `implements Display:` block is the same refusal, and
/// the message names the block the author would write.
#[test]
fn a_record_that_implements_nothing_is_sc0275() {
    let source = "\
type Station:
    id: String

def main(s: Station):
    let line be f\"{s}\"
";
    let checked = check(source);
    assert_eq!(checked.codes(), vec![275]);
    assert!(checked.messages()[0].contains("Station"), "{:?}", checked.messages());
}

/// **The cost `fstring`'s own documentation states, pinned.** A user type that
/// *does* implement `Display` passes this check, and nothing can render it:
/// `Display` has no method, because naming one would invent a `Formatter`. The
/// refusal therefore lands in codegen and not here. This test exists so that
/// the day `Display` gains a method, it fails and somebody reads the note.
#[test]
fn a_record_that_implements_display_passes_the_check_and_cannot_be_rendered() {
    let source = "\
type Station:
    id: String

Station implements Display

def main(s: Station):
    let line be f\"{s}\"
";
    check(source).assert_clean();
}

// --- §1.6: an interpolation borrows -------------------------------------

/// §1.6: *"`f\"{doc}\"` does not move `doc`."* The observable consequence at
/// this layer is that the binding is still usable afterwards.
#[test]
fn interpolating_a_string_does_not_consume_it() {
    let source = "\
def main():
    let name be \"A12\"
    let first be f\"{name}\"
    let second be f\"{name}\"
";
    check(source).assert_clean();
}

// --- §4.1 through SC0275: `print` is unary --------------------------------

/// **§4.1's headline, and the silence this closes.**
///
/// > *"`print(a, b, c)` is muscle memory for every Python user and they will
/// > type it. The mitigation is a diagnostic, not a special case."*
///
/// §7's row for `SC0275` says it *"covers `print` given more than one
/// argument"*, and before this it covered nothing: `print` has no declared
/// signature, so the call fell through to the closure arm, was typed
/// `Ty::ERROR`, and reported **nothing**.
#[test]
fn print_given_two_arguments_is_sc0275() {
    let source = "\
def main():
    let n be 4
    print(\"rows:\", n)
";
    let checked = check(source);
    assert_eq!(checked.codes(), vec![275]);
    assert_eq!(checked.messages(), vec!["`print` takes one value".to_string()]);
}

/// `write` is the same decision in §4.1's same sentence, so it is the same
/// diagnostic and the message names which one it is about.
#[test]
fn write_given_two_arguments_is_sc0275() {
    let source = "\
def main():
    write(\"a\", \"b\")
";
    let checked = check(source);
    assert_eq!(checked.codes(), vec![275]);
    assert_eq!(checked.messages(), vec!["`write` takes one value".to_string()]);
}

/// §4.1's last paragraph: *"No zero-argument `print`. There is no overloading
/// and there are no default arguments in §4.4, so a blank line is
/// `print(\"\")`."*
#[test]
fn print_given_no_arguments_is_sc0275() {
    let checked = check("def main():\n    print()\n");
    assert_eq!(checked.codes(), vec![275]);
}

/// **The other half, and the one that makes the check worth having.** §4.1's
/// own replacement for `print(\"rows:\", n)` is an interpolation, and it checks
/// clean — as does the plain one-argument call the corpus is full of.
#[test]
fn the_interpolated_form_and_the_plain_one_still_check() {
    check("def main():\n    let n be 4\n    print(f\"rows: {n}\")\n").assert_clean();
    check("def main():\n    print(\"\")\n").assert_clean();
    check("def main():\n    let n be 4\n    print(n)\n").assert_clean();
    check("def main():\n    write(\"no newline\")\n").assert_clean();
}

/// **A user's own `print` is not the prelude's.** The check is keyed on the
/// definition and not on the spelling, so a module that declares
/// `def print(a: I64, b: I64)` is checked against its own signature.
///
/// Without this the prelude would be reaching into a namespace it does not
/// own, which is the mistake `items`' `WANTED` list exists to prevent for every
/// other name this crate asks about.
#[test]
fn a_users_own_print_is_checked_against_its_own_signature() {
    let source = "\
def print(a: I64, b: I64):
    return

def main():
    print(1, 2)
";
    check(source).assert_clean();
}
