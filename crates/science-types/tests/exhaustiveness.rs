//! Decision 16 — `crates/science-types/src/exhaustive.rs`.
//!
//! **Every test here asserts the witness and not the refusal.** A test that
//! says *"this errors"* passes against a checker that reports `SC0250` on
//! every `match` in the language, which is the failure mode this check is one
//! step away from at all times; a test that says *which shape it names* does
//! not. So the assertions below are on [`Checked::messages`], and the exact
//! text of a witness is the thing they pin.
//!
//! The clean cases matter just as much and for the mirror reason: the corpus
//! test proves the check is silent on `examples/`, and these prove it is
//! silent for the *right* reason — this `Bool` was complete, that `_` did
//! cover the rest — rather than because it never ran.

mod support;

use support::{check, Checked};

/// Every message, joined, so an assertion can read like a sentence.
fn said(checked: &Checked) -> String {
    checked.messages().join(" | ")
}

fn codes(source: &str) -> Vec<u16> {
    check(source).codes()
}

const FORMAT: &str = "\
choice Format:
    Plain
    Markdown
    Json
";

const SHAPE: &str = "\
choice Shape:
    Nothing
    Circle(I64)
    Rect(I64, I64)
";

// --- a choice, which is the case Decision 16 is written for -----------------

#[test]
fn a_missing_variant_is_named_with_its_payload_as_wildcards() {
    let checked = check(&format!(
        "{SHAPE}
def area(figure: &Shape) -> I64:
    match figure:
        Nothing: 0
        Circle(r): r
"
    ));
    assert_eq!(checked.codes(), vec![250]);
    assert_eq!(
        said(&checked),
        "`&Shape` has a value no arm of this `match` covers: `Rect(_, _)`"
    );
}

/// The reason the arity is in the witness: `Rect` and `Rect(_, _)` are not the
/// same thing to type, and only one of them compiles.
#[test]
fn a_variant_with_no_payload_prints_bare() {
    let checked = check(&format!(
        "{SHAPE}
def area(figure: &Shape) -> I64:
    match figure:
        Circle(r): r
        Rect(w, h): w * h
"
    ));
    assert_eq!(said(&checked), "`&Shape` has a value no arm of this `match` covers: `Nothing`");
}

#[test]
fn several_missing_variants_are_listed_in_declaration_order() {
    let checked = check(&format!(
        "{SHAPE}
def area(figure: &Shape) -> I64:
    match figure:
        Nothing: 0
"
    ));
    assert_eq!(
        said(&checked),
        "`&Shape` has values no arm of this `match` covers: `Circle(_)`, `Rect(_, _)`"
    );
}

/// §1's cap. Four missing shapes are shown as three and a count, because a
/// first line that is a wall of text is a first line nobody reads.
#[test]
fn past_three_the_message_counts_the_rest() {
    let checked = check(
        "\
choice Wide:
    A
    B
    C
    D
    E

def pick(wide: &Wide) -> I64:
    match wide:
        A: 1
",
    );
    assert_eq!(
        said(&checked),
        "`&Wide` has values no arm of this `match` covers: `B`, `C`, `D`, and 1 more"
    );
}

#[test]
fn every_variant_named_is_exhaustive() {
    check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        Markdown: 1
        Json: 2
"
    ))
    .assert_clean();
}

#[test]
fn a_wildcard_covers_the_rest() {
    check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        _: 1
"
    ))
    .assert_clean();
}

#[test]
fn a_binding_covers_the_rest() {
    check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        other: 1
"
    ))
    .assert_clean();
}

/// `|` is three rows and not one head, and the three together complete the set.
#[test]
fn alternatives_complete_a_set_between_them() {
    check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain | Markdown | Json: 0
"
    ))
    .assert_clean();
}

// --- §2's constructor sets, one test each -----------------------------------

/// `Bool` is finite, and `examples/05_match.science` depends on it: its
/// `describe_flag` writes both values and no wildcard.
#[test]
fn bool_is_complete_at_two_arms() {
    check(
        "\
def describe(flag: Bool) -> I64:
    match flag:
        true: 1
        false: 0
",
    )
    .assert_clean();
}

#[test]
fn bool_missing_one_value_names_the_other() {
    let checked = check(
        "\
def describe(flag: Bool) -> I64:
    match flag:
        true: 1
",
    );
    assert_eq!(said(&checked), "`Bool` has a value no arm of this `match` covers: `false`");
}

/// An integer's set has no end, so the witness is `_` and not a number: naming
/// one integer would be naming one of 2^64, and the reader would add an arm for
/// it and be told about the next one.
#[test]
fn an_integer_is_never_complete_and_its_witness_is_the_wildcard() {
    let checked = check(
        "\
def digit(n: I64) -> I64:
    match n:
        0: 1
        1: 2
",
    );
    assert_eq!(said(&checked), "`I64` has a value no arm of this `match` covers: `_`");
}

#[test]
fn an_integer_with_a_catch_all_is_exhaustive() {
    check(
        "\
def digit(n: I64) -> I64:
    match n:
        0: 1
        _: 2
",
    )
    .assert_clean();
}

/// §2 states this as a cost rather than hiding it: `Char` is finite in
/// principle and infinite here.
#[test]
fn a_char_always_needs_a_catch_all() {
    let checked = check(
        "\
def kind(c: Char) -> I64:
    match c:
        'a': 1
        'b': 2
",
    );
    assert_eq!(said(&checked), "`Char` has a value no arm of this `match` covers: `_`");
}

#[test]
fn a_string_always_needs_a_catch_all() {
    let checked = check(
        "\
def kind(text: &String) -> I64:
    match text:
        \"hello\": 1
",
    );
    assert_eq!(
        said(&checked),
        "`&String` has a value no arm of this `match` covers: `_`"
    );
}

// --- the nested witness, which is the whole reason for the recursion --------

#[test]
fn a_tuple_witness_is_a_tuple() {
    let checked = check(
        "\
def quadrant(point: (I64, I64)) -> I64:
    match point:
        (0, 0): 0
        (x, 0): 1
",
    );
    assert_eq!(said(&checked), "`(I64, I64)` has a value no arm of this `match` covers: `(_, _)`");
}

#[test]
fn a_tuple_closed_by_a_wildcard_pair_is_exhaustive() {
    check(
        "\
def quadrant(point: (I64, I64)) -> I64:
    match point:
        (0, 0): 0
        (x, 0): 1
        (0, y): 2
        (_, _): 3
",
    )
    .assert_clean();
}

/// A record witness is written the way a record *pattern* is written — with the
/// field names — so that the reader can paste it into the `match`.
#[test]
fn a_record_witness_names_its_fields() {
    let checked = check(
        "\
type Point:
    x: I64
    y: I64

def classify(point: &Point) -> I64:
    match point:
        Point(x: 0, y: 0): 0
",
    );
    assert_eq!(
        said(&checked),
        "`&Point` has a value no arm of this `match` covers: `Point(x: _, y: _)`"
    );
}

#[test]
fn a_record_bound_field_by_field_is_exhaustive() {
    check(
        "\
type Point:
    x: I64
    y: I64

def classify(point: &Point) -> I64:
    match point:
        Point(x: 0, y: 0): 0
        Point(x: x, y: y): 1
",
    )
    .assert_clean();
}

/// The nesting that Decision 16's argument is about: the outer constructor is
/// covered, and what is missing is a shape *inside* it.
#[test]
fn a_witness_nested_inside_a_variant_names_the_inner_shape() {
    let checked = check(&format!(
        "{FORMAT}
choice Wrapped:
    One(Format)

def unwrap(wrapped: &Wrapped) -> I64:
    match wrapped:
        One(Plain): 0
        One(Markdown): 1
"
    ));
    assert_eq!(
        said(&checked),
        "`&Wrapped` has a value no arm of this `match` covers: `One(Json)`"
    );
}

// --- §3, the nullable ------------------------------------------------------

/// §7 in one program: `null` plus the non-null case is exhaustive, and the
/// non-null case is written by writing a pattern of `T`.
#[test]
fn null_plus_every_variant_is_exhaustive() {
    check(&format!(
        "{FORMAT}
def name_of(format: Format?) -> I64:
    match format:
        null: 0
        Plain: 1
        Markdown: 2
        Json: 3
"
    ))
    .assert_clean();
}

#[test]
fn a_nullable_missing_null_names_null() {
    let checked = check(&format!(
        "{FORMAT}
def name_of(format: Format?) -> I64:
    match format:
        Plain: 1
        Markdown: 2
        Json: 3
"
    ));
    assert_eq!(said(&checked), "`Format?` has a value no arm of this `match` covers: `null`");
}

/// The witness is *inside* the present constructor and prints without it,
/// because §5.5 took `Some` out of the language and there is nothing to print.
#[test]
fn a_nullable_missing_a_variant_names_the_variant_and_not_a_wrapper() {
    let checked = check(&format!(
        "{FORMAT}
def name_of(format: Format?) -> I64:
    match format:
        null: 0
        Plain: 1
        Markdown: 2
"
    ));
    assert_eq!(said(&checked), "`Format?` has a value no arm of this `match` covers: `Json`");
}

/// §1's stated failure: the present half of a `T?` has no spelling, so when the
/// *whole* of it is missing the witness is the widest thing that can be typed.
#[test]
fn the_present_half_with_nothing_said_about_it_prints_as_the_wildcard() {
    let checked = check(
        "\
def value(n: I64?) -> I64:
    match n:
        null: 0
",
    );
    assert_eq!(said(&checked), "`I64?` has a value no arm of this `match` covers: `_`");
}

#[test]
fn a_wildcard_over_a_nullable_covers_null_too() {
    check(
        "\
def value(n: I64?) -> I64:
    match n:
        null: 0
        _: 1
",
    )
    .assert_clean();
}

/// Decision 7 is what keeps §3 from demanding a `null` arm on every error
/// `match` in the corpus: inside `if err?:` the scrutinee's *node* has the
/// non-null type, and this pass reads the node.
#[test]
fn a_narrowed_scrutinee_needs_no_null_arm() {
    check(
        "\
choice LoadError:
    Missing
    Corrupt

def explain(error: LoadError?) -> I64:
    if error?:
        match error:
            Missing: 1
            Corrupt: 2
    else:
        0
",
    )
    .assert_clean();
}

// --- §4, the unreachable arm ------------------------------------------------

#[test]
fn an_arm_below_a_catch_all_is_dead() {
    let checked = check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        _: 0
        Plain: 1
"
    ));
    assert_eq!(checked.codes(), vec![537]);
    assert_eq!(said(&checked), "no value reaches this arm");
}

#[test]
fn an_arm_repeating_a_variant_is_dead() {
    let checked = check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        Markdown: 1
        Json: 2
        Plain: 3
"
    ));
    assert_eq!(checked.codes(), vec![537]);
}

/// Two literals of one value are the same constructor, which is the only thing
/// `Ctor::Lit` exists for.
#[test]
fn an_arm_repeating_a_literal_is_dead() {
    assert_eq!(
        codes(
            "\
def digit(n: I64) -> I64:
    match n:
        0: 1
        0: 2
        _: 3
"
        ),
        vec![537]
    );
}

/// A dead arm and a missing value are two answers from one recursion, and a
/// `match` can have both at once.
#[test]
fn a_match_can_be_both_dead_and_short() {
    assert_eq!(
        codes(&format!(
            "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        Plain: 1
"
        )),
        vec![537, 250]
    );
}

/// The sub-pattern is what makes the second arm live, and a check that compared
/// head constructors alone would call it dead.
#[test]
fn a_narrower_arm_above_a_wider_one_leaves_it_alive() {
    check(&format!(
        "{FORMAT}
choice Wrapped:
    One(Format)

def unwrap(wrapped: &Wrapped) -> I64:
    match wrapped:
        One(Plain): 0
        One(other): 1
"
    ))
    .assert_clean();
}

/// §4's stated omission: a dead `|` alternative inside a live arm is not
/// reported, and this pins that rather than leaving it to be discovered.
#[test]
fn a_dead_alternative_inside_a_live_arm_is_not_reported() {
    check(&format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain | Plain | Markdown: 0
        Json: 1
"
    ))
    .assert_clean();
}

// --- §5, what it refuses to answer -----------------------------------------

/// A `match` over a type parameter has no constructor set, so it needs a
/// catch-all — which is a true positive and not a refusal: `_`, a binding and a
/// literal are the only patterns writable against a `T`.
#[test]
fn a_type_parameter_needs_a_catch_all() {
    let checked = check(
        "\
def pick[T](value: T) -> I64:
    match value:
        _: 0
",
    );
    checked.assert_clean();
}

/// Silence where the pattern did not resolve, so that one unknown name is one
/// diagnostic and not two.
#[test]
fn an_unresolved_variant_pattern_reports_nothing_here() {
    let source = format!(
        "{FORMAT}
def name_of(format: &Format) -> I64:
    match format:
        Plain: 0
        Nonsense: 1
"
    );
    let checked = support::check_allowing_resolution_errors(&source);
    // The resolver treats an unknown bare name in a pattern as a binding, so
    // this one resolves; what it must not do is make the `match` short.
    assert!(
        !checked.codes().contains(&250),
        "a name the resolver bound as a catch-all must not be read as a missing variant: {:?}",
        checked.messages()
    );
}

/// A generic choice: the constructor set is the *declaration's* variants, so it
/// does not depend on the instantiation and needs no substitution to read.
///
/// **The payload is bound and not used**, and that is deliberate rather than
/// incidental: what is pinned here is the constructor *set*, which does not
/// depend on the instantiation.
///
/// This test used to carry the hole it found — `check`'s `pattern` gave the
/// binding in `Left(n)` the *declaration's* `L` rather than the instantiation's
/// `I64`, so `Left(n): n` at an `I64` return was `SC0525` on a correct program.
/// It is closed: `check`'s `scrutinee_substitution` is §9, and
/// `tests/checking.rs` holds the pair of tests that the right program passes
/// and the wrong one is still refused.
#[test]
fn a_generic_choice_is_complete_at_its_declared_variants() {
    let checked = check(
        "\
choice Either[L, R]:
    Left(L)
    Right(R)
    Neither

def pick(value: &Either[I64, Bool]) -> I64:
    match value:
        Left(n): 1
        Neither: 0
",
    );
    assert_eq!(
        said(&checked),
        "`&Either[I64, Bool]` has a value no arm of this `match` covers: `Right(_)`"
    );
}

#[test]
fn a_generic_choice_with_every_variant_is_exhaustive() {
    check(
        "\
choice Either[L, R]:
    Left(L)
    Right(R)
    Neither

def pick(value: &Either[I64, Bool]) -> I64:
    match value:
        Left(n): 1
        Right(flag): 2
        Neither: 0
",
    )
    .assert_clean();
}

/// The type the `match` reports is the one the author *wrote*, which is
/// `check`'s §1 rule about not revealing eagerly applied to this message: the
/// alias is what they will search their file for.
#[test]
fn the_reported_type_is_the_written_alias_and_not_its_expansion() {
    let checked = check(&format!(
        "{FORMAT}
type Style is Format

def name_of(style: Style) -> I64:
    match style:
        Plain: 0
"
    ));
    assert_eq!(
        said(&checked),
        "`Style` has values no arm of this `match` covers: `Markdown`, `Json`"
    );
}
