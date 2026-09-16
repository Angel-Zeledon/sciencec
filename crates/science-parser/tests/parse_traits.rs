//! `interface` and the two implementation heads, which §4.4 gives five shapes
//! between them: an interface with required and default methods, a generic
//! interface, `Type implements Interface:`, the inherent `Type has:`, and the
//! block-less marker form. §5.4 adds the associated type that an interface
//! declares and an implementation binds.
//!
//! An implementation head is the one item in the language that starts with a
//! type rather than with a keyword, so several of these tests are really about
//! the parser recognising an item it has not been told about by its first word.

mod common;
use common::{parse_source, parse_source_allowing_errors};

/// An interface with one required method and one with a default body. Both are
/// the same node; only `body` differs.
#[test]
fn interface_with_a_default_method() {
    insta::assert_snapshot!(parse_source(
        r#"interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        self.summarize().truncate(80)
"#
    ));
}

/// §4.4: an interface may take type parameters, written after the name with
/// `of`.
#[test]
fn generic_interface() {
    insta::assert_snapshot!(parse_source(
        r#"interface From of T:
    def from(value: T) -> Self
"#
    ));
}

/// The interfaces an interface requires, written `interface A: B + C`. The `:`
/// that introduces them and the `:` that opens the body are told apart by what
/// follows: a name against the end of the line.
#[test]
fn interface_with_required_interfaces() {
    insta::assert_snapshot!(parse_source(
        r#"interface Pretty: Summarize + Clone:
    def pretty(self) -> String
"#
    ));
}

/// The same interface with a parameter. `of T` is not bracketed any more, so
/// the bare form `interface Pretty of T: Summarize + Clone:` has two readings
/// and the parameter takes the bounds; the parenthesised `of (T)` closes the
/// parameter list first and leaves the bounds to the interface. Both are
/// here, side by side, because the difference is invisible otherwise.
#[test]
fn a_generic_interface_with_required_interfaces() {
    insta::assert_snapshot!(parse_source(
        r#"interface Pretty of T: Summarize + Clone:
    def pretty(self) -> T

interface Plain of (T): Summarize + Clone:
    def plain(self) -> T
"#
    ));
}

/// An interface whose methods take each of §4.4's three receiver forms: `self`
/// borrows, `mutable self` borrows exclusively, and only the annotated
/// `self: Self` takes the receiver by value.
#[test]
fn interface_receivers() {
    insta::assert_snapshot!(parse_source(
        r#"interface Shape:
    def area(self) -> Int
    def scale(mutable self) -> Int
    def consume(self: Self) -> Int
"#
    ));
}

/// `mutable self` on its own, since it is the receiver that changed spelling
/// most and the one a method that mutates has to reach for.
#[test]
fn a_mutable_receiver() {
    insta::assert_snapshot!(parse_source(
        r#"Note implements Reset:
    def reset(mutable self):
        self.text be ""
"#
    ));
}

/// The by-value receiver on its own. It is written as an ordinary annotated
/// parameter, and the annotation is dropped: a receiver's type is always the
/// implementing type.
#[test]
fn a_by_value_receiver() {
    insta::assert_snapshot!(parse_source(
        r#"Note implements IntoTitle:
    def into_title(self: Self) -> String:
        self.text
"#
    ));
}

/// The annotation on a by-value receiver is checked even though it is dropped,
/// so `self: Int` cannot pass for one.
#[test]
fn a_receiver_annotated_with_anything_but_self_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"Note has:
    def take(self: Int) -> Int:
        1
"#
    ));
}

/// `Type implements Interface:`, the form §4.4 writes down first. The type
/// comes first and the interface second — the reverse of what the old
/// `impl Trait for Type` said.
#[test]
fn a_type_implements_an_interface() {
    insta::assert_snapshot!(parse_source(
        r#"Doc implements Summarize:
    def summarize(self) -> String:
        self.body.truncate(200)
"#
    ));
}

/// A generic interface implemented for a generic type, both sides carrying
/// arguments. The head declares `A` and `B` once and echoes them back as the
/// arguments of `Pair`.
#[test]
fn a_generic_type_implements_a_generic_interface() {
    insta::assert_snapshot!(parse_source(
        r#"Pair of (A, B) implements Swap of Pair of (B, A):
    def swapped(self) -> Pair of (B, A):
        Pair(first: self.second, second: self.first)
"#
    ));
}

/// The bare `Pair of (A, B) implements Swap:` head of §4.4: the parameters are
/// declared on the implementing type and nothing else carries arguments.
#[test]
fn a_generic_implementation_echoes_its_parameters() {
    insta::assert_snapshot!(parse_source(
        r#"Pair of (A, B) implements Swap:
    type Swapped is Pair of (B, A)

    def swapped(self: Self) -> Self.Swapped:
        Pair(first: self.second, second: self.first)
"#
    ));
}

/// §4.4's inherent block: "a type may have methods that come from no
/// interface. A function in a `has` block with no `self` receiver is an
/// associated function."
#[test]
fn has_with_an_associated_function() {
    insta::assert_snapshot!(parse_source(
        r#"Doc has:
    def new(title: String) -> Doc:
        Doc(title: title, body: "")

    def is_empty(self) -> Bool:
        self.body.length() is 0
"#
    ));
}

/// An inherent block on a generic type, which is how `Array of T` gets its
/// associated function.
#[test]
fn has_on_a_generic_type() {
    insta::assert_snapshot!(parse_source(
        r#"Array of T has:
    def new() -> Array of T:
        empty()
"#
    ));
}

/// The call side of an associated function: `Doc.new("a")`. The parser cannot
/// tell it from a method call, and §4.4's rule is that resolution decides.
#[test]
fn an_associated_function_call_is_a_method_call_until_resolution() {
    insta::assert_snapshot!(parse_source("def main():\n    let d be Doc.new(\"a\")\n"));
}

/// §4.4's marker interface: "an interface with no methods is implemented by a
/// single line with no block, since there is nothing to indent." Without this
/// rule `Copy` would be unimplementable.
#[test]
fn a_marker_implementation_has_no_block() {
    insta::assert_snapshot!(parse_source("Position implements Copy\n"));
}

/// The marker form sits among ordinary items without disturbing them — the
/// interesting case, because the line that follows it is another item that
/// also begins with a type.
#[test]
fn a_marker_implementation_between_other_items() {
    insta::assert_snapshot!(parse_source(
        r#"type Position:
    line: U32

Position implements Copy

Position implements Clone:
    def clone(self) -> Position:
        Position(line: self.line)

def main():
    print(1)
"#
    ));
}

/// A `where` clause on an implementation.
#[test]
fn an_implementation_with_a_where_clause() {
    insta::assert_snapshot!(parse_source(
        r#"Pair of (A, B) implements Summarize where A: Clone, B: Clone:
    def summarize(self) -> String:
        "pair"
"#
    ));
}

/// §4.4: "where a `where` clause ends." A bound list ends only at `+` or `,`,
/// so the first `:` that no bound consumes is the one that opens the block.
#[test]
fn a_where_clause_ends_at_the_colon_that_opens_the_block() {
    insta::assert_snapshot!(parse_source(
        r#"def f of T() -> T where T: A + B:
    body()
"#
    ));
}

// --- associated types (§5.4) ---------------------------------------------

/// The pair that §5.4 introduces: an interface declares `type Item` and leaves
/// the type open; the implementation supplies it with `type Item is Int`. The two
/// members land in `assoc types`, apart from the methods, because their
/// interleaving carries no meaning.
#[test]
fn an_associated_type_is_declared_in_an_interface_and_bound_in_an_implementation() {
    insta::assert_snapshot!(parse_source(
        r#"interface Iterate:
    type Item
    def next(mutable self) -> Option of Self.Item

Countdown implements Iterate:
    type Item is Int

    def next(mutable self) -> Option of Self.Item:
        None
"#
    ));
}

/// `Self.Item` in a return position, bare rather than wrapped in `Option`, so
/// that the `SelfAssoc` node and its span are visible on their own.
#[test]
fn self_dot_item_names_an_associated_type_in_a_return_position() {
    insta::assert_snapshot!(parse_source(
        r#"interface Produce:
    type Output
    def produce(self) -> Self.Output
"#
    ));
}

/// §5.4 allows only `Self` as the base of a projection: F0 has no syntax for
/// the qualified form Rust spells `<T as Iterate>::Item`. `T.Item` is
/// therefore not a projection at all, and the parser — which has no idea what
/// `T` is — reads it as the ordinary dotted path it looks like and leaves the
/// complaint to resolution.
#[test]
fn a_projection_through_something_other_than_self_is_a_plain_path() {
    insta::assert_snapshot!(parse_source(
        r#"interface Produce of T:
    def produce(self) -> T.Output
"#
    ));
}

/// `type Item is Int` written in an *interface* is SC0112, and the message
/// names the form that belongs there: an interface declares the name and
/// leaves the type to the implementation.
///
/// FAILING ON PURPOSE. `parse_member` is told which side it is on and refuses
/// to read the other side's form at all, so the `Member::AssocBinding` arm of
/// `parse_interface_body` — which holds this message — cannot be reached, and
/// what comes out is a bare SC0100 about a stray `is`.
#[test]
fn an_associated_type_bound_inside_an_interface_is_rejected() {
    let report = parse_source_allowing_errors(
        r#"interface Iterate:
    type Item is Int
    def next(mutable self) -> Self.Item
"#,
    );
    insta::assert_snapshot!(report);
    assert!(
        report.contains("SC0112"),
        "the implementation's form written in an interface should be SC0112, naming the form \
         belongs here; got:\n{report}"
    );
}

/// And `type Item` written in an *implementation* is the same code with the
/// other message: the implementation is the side that owes a type.
///
/// FAILING ON PURPOSE, for the mirror-image reason: the `Member::AssocDecl`
/// arm of `parse_impl` is unreachable, and the bare `expect` of `is` reports
/// SC0100 first.
#[test]
fn an_associated_type_left_unbound_inside_an_implementation_is_rejected() {
    let report = parse_source_allowing_errors(
        r#"Countdown implements Iterate:
    type Item
    def next(mutable self) -> Self.Item:
        None
"#,
    );
    insta::assert_snapshot!(report);
    assert!(
        report.contains("SC0112"),
        "the interface's form written in an implementation should be SC0112, saying the \
         implementation owes the type; got:\n{report}"
    );
}

// --- what is rejected ------------------------------------------------------

/// An interface body that contains something that is neither a method nor an
/// associated type is reported, and the members around it still parse.
#[test]
fn a_non_member_in_an_interface_body_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"interface T:
    let x be 1
    def f(self) -> Int
"#
    ));
}

/// `implements` followed by something that is not a path.
#[test]
fn a_non_path_interface_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"Point implements borrowed Doc:
    def f(self) -> Int:
        1
"#
    ));
}

/// An implementation is not a name anything can be imported by, so `public` in
/// front of one is SC0107.
#[test]
fn public_on_an_implementation_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"public Doc implements Summarize:
    def summarize(self) -> String:
        "a"
"#
    ));
}

/// An implementation body that is indented without a `:` gets the same
/// targeted message a function does, rather than a complaint about stray
/// indentation.
#[test]
fn an_implementation_body_without_a_colon_is_rejected() {
    insta::assert_snapshot!(parse_source_allowing_errors(
        r#"Doc implements Summarize
    def summarize(self) -> String:
        "a"
"#
    ));
}
