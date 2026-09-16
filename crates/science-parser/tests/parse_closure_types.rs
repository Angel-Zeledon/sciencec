//! The closure type `(A) -> B` — `collections-and-chains.md` §1.2, AMENDMENT 1.
//!
//! The whole feature is one token of lookahead past a closing paren, so nearly
//! every test here is really the same question asked in a different position:
//! *did the parser decide tuple or parameter list, and did it decide without
//! backtracking?* The three-way distinction comes first, because `()`, `(A, B)`
//! and `(A) -> B` all start with the same character and two of them already
//! meant something.
//!
//! The associativity tests use `shape_of_type`, which strips spans: `(A) -> (B)
//! -> C` against `((A) -> B) -> C` is a difference of two lines' indentation
//! and a span on every line buries it.

mod common;
use common::{parse_source, parse_source_allowing_errors, shape_of_type};

// --- the three-way distinction -------------------------------------------

/// `()`, `(A, B)` and `(A) -> B` in one signature.
///
/// §1.2's risk, stated plainly: a closure type opens identically to the
/// commonest type in the language. All three are here side by side so that a
/// regression in the fork shows up as one of them turning into another.
#[test]
fn unit_a_tuple_and_a_closure_are_three_different_types() {
    insta::assert_snapshot!(parse_source(
        r#"def f(a: (), b: (Int, Bool), c: (Int) -> Bool) -> Int:
    0
"#
    ));
}

/// `(T)` is still just `T` — parentheses only group — and `(T) -> U` is a
/// closure taking one parameter. The difference is the arrow and nothing else,
/// which is what "no backtracking" means: the same parse produced both.
#[test]
fn a_parenthesised_type_is_still_not_a_tuple() {
    insta::assert_snapshot!(shape_of_type("(Int)"));
}

#[test]
fn a_one_parameter_closure() {
    insta::assert_snapshot!(shape_of_type("(Int) -> Bool"));
}

/// `() -> B` has no parameters. `Unit` maps to the empty list rather than to a
/// single parameter of type `()`, which is the reason a lone unit parameter is
/// unspellable — and worth nothing, so it is not a loss anybody pays for.
#[test]
fn a_closure_with_no_parameters() {
    insta::assert_snapshot!(shape_of_type("() -> Bool"));
}

/// The two-parameter form AMENDMENT 3's `reduce` needs.
#[test]
fn a_two_parameter_closure() {
    insta::assert_snapshot!(shape_of_type("(F64, Row) -> F64"));
}

// --- associativity and binding strength ----------------------------------

/// `->` is right-associative, so `(A) -> (B) -> C` is `(A) -> ((B) -> C)`:
/// currying, and the only useful reading. It falls out of the return type
/// being a recursive `parse_type` call rather than being arranged for.
#[test]
fn the_arrow_is_right_associative() {
    insta::assert_snapshot!(shape_of_type("(Int) -> (Bool) -> String"));
}

/// The same tree, written with the parentheses §1.2 says are redundant. If
/// these two ever stop matching, the associativity has moved.
#[test]
fn the_explicit_right_grouping_is_the_same_tree() {
    let bare = shape_of_type("(Int) -> (Bool) -> String");
    let grouped = shape_of_type("(Int) -> ((Bool) -> String)");
    assert_eq!(bare, grouped);
}

/// The other grouping has to be written out, and is a different tree: a
/// closure whose *parameter* is a closure.
#[test]
fn the_left_grouping_is_a_closure_taking_a_closure() {
    insta::assert_snapshot!(shape_of_type("((Int) -> Bool) -> String"));
}

/// `?` binds tighter than `->`, because the `?` loop runs inside the recursive
/// call that parses the return type. So `(A) -> B?` is `(A) -> (B?)`.
#[test]
fn a_question_mark_binds_tighter_than_the_arrow() {
    insta::assert_snapshot!(shape_of_type("(Int) -> Bool?"));
}

/// A nullable closure therefore needs the parentheses out loud —
/// `scientific-libraries.md` §8.6 writes exactly this for `lbfgs`'s optional
/// gradient — and it is a different tree from the one above.
#[test]
fn a_nullable_closure_needs_its_own_parentheses() {
    insta::assert_snapshot!(shape_of_type("((Int) -> Bool)?"));
}

/// `of` binds tighter than `->`: `Array of Int -> Bool` is a closure taking an
/// array, not an array of closures.
///
/// **This one did not fall out; it had to be fixed.** §1.2's analysis says the
/// arrow is one token of lookahead and never says whose token it is, and the
/// bare generic argument of `Array of T` parsed its argument with the same
/// greedy `parse_type` as everything else — so `Array of Int -> Bool` came out
/// an `Array` of closures while `Array of (Int) -> Bool` came out a closure
/// taking an array. §4.3 says those two argument lists are the same list, so
/// two spellings the note calls equal named different types. The bare form now
/// stops before the arrow. See `the_two_spellings_of_one_generic_argument_agree`
/// below, which is the test that would have caught it.
#[test]
fn generic_arguments_bind_tighter_than_the_arrow() {
    insta::assert_snapshot!(shape_of_type("Array of Int -> Bool"));
}

/// §4.3 permits one generic argument to be parenthesised or not. Those two
/// spellings must therefore agree about which side of the argument list a
/// following `->` falls on, and the parenthesised one has no choice: its
/// parens close the list, so the arrow is outside.
#[test]
fn the_two_spellings_of_one_generic_argument_agree() {
    assert_eq!(shape_of_type("Array of Int -> Bool"), shape_of_type("Array of (Int) -> Bool"));
}

/// A closure *as* a generic argument writes the parentheses it needs, which is
/// what every other position where two readings meet already asks for.
#[test]
fn an_array_of_closures_is_spelled_with_its_own_parentheses() {
    insta::assert_snapshot!(shape_of_type("Array of ((Int) -> Bool)"));
}

// --- the positions a closure type appears in -----------------------------

/// A closure as a parameter type, which is what every combinator in §1.4
/// needs, and a closure returned from a function, which is what a builder
/// needs. `-> (Int) -> Bool` in return position is the case §1.2 had to state
/// an associativity for: nothing may follow a return type but `where` or `:`,
/// so the greedy reading is the only reading.
#[test]
fn a_closure_as_a_parameter_and_as_a_return_type() {
    insta::assert_snapshot!(parse_source(
        r#"def keep(p: (Int) -> Bool) -> Int:
    0

def adder(n: Int) -> (Int) -> Int:
    n
"#
    ));
}

/// A function returning a closure that returns a closure. The second and third
/// arrows are read greedily and right-associatively, and the `:` that opens
/// the body is what stops them.
#[test]
fn a_closure_returning_a_closure() {
    insta::assert_snapshot!(parse_source(
        r#"def curry() -> (Int) -> (Bool) -> String:
    0
"#
    ));
}

/// **The part nobody had priced.** A bound used to hold a `Path`, so
/// `where P: (borrowed Int) -> Bool` was `SC0102`, *expected an identifier,
/// found `(`*. §1.4's entire vocabulary is written in `where` clauses, so
/// without this the feature buys nothing.
#[test]
fn a_closure_in_a_where_bound() {
    insta::assert_snapshot!(parse_source(
        r#"def keep of P(items: Array of Int, p: P) -> Array of Int where P: (borrowed Int) -> Bool:
    items
"#
    ));
}

/// The inline form of the same bound, `of P: (Int) -> Bool`. A bound list ends
/// at the `(` that opens the parameter list, and a bound that *starts* with
/// `(` is a closure — the two `(`s are told apart by which side of the bound
/// they fall on, not by lookahead.
#[test]
fn a_closure_as_an_inline_generic_bound() {
    insta::assert_snapshot!(parse_source(
        r#"def keep of P: (Int) -> Bool(p: P) -> Int:
    0
"#
    ));
}

/// A closure bound beside an ordinary one, on both sides of the `+`.
#[test]
fn a_closure_bound_in_a_list_with_interface_bounds() {
    insta::assert_snapshot!(parse_source(
        r#"def run of F(f: F) -> Int where F: Clone + (Int) -> Bool + Send:
    0
"#
    ));
}

/// `where F: Int -> Bool` — the left-hand side was never parenthesised, and is
/// one parameter. This is not a convenience: `(T)` collapses with no node, so
/// by the time the arrow is read nothing remembers whether a paren was
/// written, and refusing the bare form would mean refusing `(Int) -> Bool` too.
#[test]
fn an_unparenthesised_left_hand_side_is_one_parameter() {
    insta::assert_snapshot!(parse_source(
        r#"def run of F(f: F) -> Int where F: Int -> Bool:
    0
"#
    ));
}

// --- what did not change -------------------------------------------------

/// The error model's return shape. `-> (Config, Error?)` is followed by `:` or
/// `where`, never by `->`, so the one peek leaves it a tuple and no existing
/// signature in the language changes meaning. Both continuations are here.
#[test]
fn a_fallible_return_shape_is_still_a_tuple() {
    insta::assert_snapshot!(parse_source(
        r#"def load(path: String) -> (Config, Error?):
    0

def load_as of T(path: String) -> (T, Error?) where T: Clone:
    0
"#
    ));
}

/// An interface position does not admit a closure, and reports exactly what it
/// reported before the closure type existed. `any` still wants a name, so this
/// is `SC0102` on the `(` — the diagnostic is unchanged, which is the evidence
/// that the fork is never reached here.
///
/// The two `SC0100`s after it are this position's own pre-existing recovery
/// and are byte-for-byte what it produced before any of this: a `(` where a
/// name belongs has always cascaded here. It is left alone deliberately —
/// changing it would be changing a diagnostic the closure type never touched.
#[test]
fn any_still_requires_an_interface_name() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"def f(x: any (Int) -> Bool) -> Int:
    0
"#
    ));
}

/// The same for `implements`, which is `SC0111` and names the keyword.
#[test]
fn implements_still_requires_an_interface_name() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"Doc implements (Int) -> Bool:
    def f(self) -> Int:
        0
"#
    ));
}

/// And for an interface's own super list, which is `SC0102` followed by the
/// same pre-existing cascade. Unchanged for the same reason.
#[test]
fn a_super_interface_list_still_requires_names() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"interface Pretty: (Int) -> Bool:
    def pretty(self) -> Int
"#
    ));
}

// --- the accepted loss ---------------------------------------------------

/// **§1.2's one real loss, pinned rather than fixed.** `(T)` collapses to `T`
/// with no node of its own, so `((Int, Bool)) -> Int` — a closure over a
/// single *tuple* — parses as the two-parameter `(Int, Bool) -> Int` and there
/// is no third paren that would say otherwise. §1.3 closes the hole by ruling
/// that pairs are records and never tuples. This test exists so that the loss
/// is a recorded decision rather than a surprise.
#[test]
fn a_closure_over_a_single_tuple_cannot_be_spelled() {
    let collapsed = shape_of_type("((Int, Bool)) -> Int");
    let two_parameters = shape_of_type("(Int, Bool) -> Int");
    assert_eq!(collapsed, two_parameters);
    insta::assert_snapshot!(collapsed);
}

// --- one diagnostic, never three -----------------------------------------

/// A bound that opens with `(` and does not continue with `->` is `SC0119`,
/// and it is **one** diagnostic: the parameter list is already consumed, the
/// cursor is sitting on the `:` the declaration is waiting for, and nothing is
/// skipped. This is the no-cascade rule applied to the one position the
/// closure type created, and it is the pattern `skip_const_operand` set.
#[test]
fn a_bound_that_opens_with_a_paren_needs_an_arrow() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"def run of F(f: F) -> Int where F: (Int):
    0
"#
    ));
}

/// The same refusal on the empty parameter list, which has no second reading
/// either: `()` is the unit type and the unit type is not an interface.
#[test]
fn an_empty_parameter_list_is_not_a_bound() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"def run of F(f: F) -> Int where F: ():
    0
"#
    ));
}

/// An arrow with nothing usable after it is one diagnostic as well, and this
/// one needed no new code: `be` is already a list boundary, so the type parser
/// reports `SC0104` and *stops* rather than eating the keyword the `let` is
/// about to require.
#[test]
fn an_arrow_with_no_return_type_does_not_derail_the_statement() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"def f() -> Int:
    let g: (Int) -> be 0
    0
"#
    ));
}

/// An unclosed parameter list, for the same reason: one diagnostic about the
/// missing `)`, and the item after it still parses.
#[test]
fn an_unclosed_parameter_list_does_not_eat_the_next_item() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"def f(g: (Int -> Bool) -> Int:
    0

def h() -> Int:
    0
"#
    ));
}
