//! `conform` — whether an `implements` block is held to the interface it names.
//!
//! An `implements` block is a promise, and until `conform` existed nothing
//! collected on it. A block that implemented none of the interface's methods,
//! one that implemented a method the interface never declared, and one that
//! implemented every method at the wrong types all checked exactly as clean as
//! a correct one. Five probes proved it, and they are the first five tests
//! here.
//!
//! **Every case is a pair**, which is `mutability.rs`' and `array_literals.rs`'
//! rule: a check that refuses everything is not a check. So each refusal sits
//! beside the program it must not touch, and the second half is the larger half
//! — the three shapes that must stay legal are an interface's *default* bodies,
//! a *generic* interface whose arguments substitute into the declaration, and
//! an *associated* type the implementation answers. Each of those is a block
//! whose methods disagree with the declaration as written and agree with it as
//! meant, and refusing any one of them would make the language's own corpus
//! uncompilable.
//!
//! **Where it declines, and why that is not hidden.** `conform`'s §3 and §4
//! state the four silences; the last section here pins each one as a test, so
//! that the day the reason for one goes away the test that covered it fails
//! rather than quietly starting to pass.

mod support;

// --- the five refusals, one per probe -------------------------------------

#[test]
fn a_declared_method_the_block_never_implements_is_refused() {
    let checked = support::check(
        "\
interface Render:
    def render(self) -> String
    def width(self) -> Int

type Doc:
    title: String

Doc implements Render:
    def render(self) -> String:
        String.new()
",
    );
    assert_eq!(checked.codes(), vec![539]);
    assert_eq!(
        checked.messages(),
        vec!["this block does not implement `Render`'s method `width`"]
    );
}

#[test]
fn a_block_that_implements_every_declared_method_is_clean() {
    support::check(
        "\
interface Render:
    def render(self) -> String
    def width(self) -> Int

type Doc:
    title: String

Doc implements Render:
    def render(self) -> String:
        String.new()

    def width(self) -> Int:
        self.title.length()
",
    )
    .assert_clean();
}

#[test]
fn a_method_the_interface_does_not_declare_is_refused() {
    let checked = support::check(
        "\
interface Render:
    def render(self) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self) -> String:
        String.new()

    def extra(self) -> Int:
        1
",
    );
    assert_eq!(checked.codes(), vec![540]);
    assert_eq!(checked.messages(), vec!["`Render` declares no method `extra`"]);
}

/// The fix `SC0540` offers, taken — and it has to parse, which is house rule 3.
#[test]
fn the_same_method_in_an_inherent_block_is_clean() {
    support::check(
        "\
interface Render:
    def render(self) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self) -> String:
        String.new()

Doc has:
    def extra(self) -> Int:
        1
",
    )
    .assert_clean();
}

#[test]
fn a_parameter_type_that_disagrees_with_the_declaration_is_refused() {
    let checked = support::check(
        "\
interface Render:
    def render(self, width: Int) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self, width: Bool) -> String:
        String.new()
",
    );
    assert_eq!(checked.codes(), vec![541]);
    assert_eq!(
        checked.messages(),
        vec!["`render` does not have the signature `Render` declares for it"]
    );
}

#[test]
fn a_return_type_that_disagrees_with_the_declaration_is_refused() {
    let checked = support::check(
        "\
interface Render:
    def render(self) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self) -> Int:
        1
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

#[test]
fn a_receiver_that_disagrees_with_the_declaration_is_refused() {
    let checked = support::check(
        "\
interface Reset:
    def reset(mutable self)

type Doc:
    title: String

Doc implements Reset:
    def reset(self):
        let x be 1
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

/// `self` and `self: Self` are different receivers, and that is the whole point
/// of §5.4 giving them different spellings: one borrows and one consumes.
#[test]
fn a_by_value_receiver_does_not_answer_a_borrowing_one() {
    let checked = support::check(
        "\
interface Consume:
    def consume(self: Self) -> Int

type Doc:
    title: String

Doc implements Consume:
    def consume(self) -> Int:
        1
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

#[test]
fn the_declared_receiver_is_clean() {
    support::check(
        "\
interface Reset:
    def reset(mutable self)

interface Consume:
    def consume(self: Self) -> Int

type Doc:
    title: String

Doc implements Reset:
    def reset(mutable self):
        let x be 1

Doc implements Consume:
    def consume(self: Self) -> Int:
        1
",
    )
    .assert_clean();
}

/// The arity, which is the disagreement with no type in it.
#[test]
fn a_parameter_count_that_disagrees_is_refused() {
    let checked = support::check(
        "\
interface Render:
    def render(self, width: Int) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self) -> String:
        String.new()
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

// --- what must stay legal, which is the larger half -----------------------

/// Decision 15's default body. `examples/06_traits.science` writes this and its
/// own comment says why: *"only the required method: the defaults are inherited
/// as written"*.
#[test]
fn a_default_body_need_not_be_restated() {
    support::check(
        "\
interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        self.summarize()

type Note:
    text: String

Note implements Summarize:
    def summarize(self) -> String:
        String.new()
",
    )
    .assert_clean();
}

/// And overriding one is not an error either — it is the other half of what a
/// default is for.
#[test]
fn a_default_body_may_be_overridden() {
    support::check(
        "\
interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        self.summarize()

type Note:
    text: String

Note implements Summarize:
    def summarize(self) -> String:
        String.new()

    def preview(self) -> String:
        String.new()
",
    )
    .assert_clean();
}

/// A default overridden at the *wrong* signature is still refused: the block is
/// held to the declaration whether or not the interface also supplied a body.
#[test]
fn an_overridden_default_is_still_held_to_the_declaration() {
    let checked = support::check(
        "\
interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        self.summarize()

type Note:
    text: String

Note implements Summarize:
    def summarize(self) -> String:
        String.new()

    def preview(self) -> Int:
        1
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

/// `conform`'s §2, and the case `Declarations::interface_arguments` exists for:
/// the declaration says `Idx` and the block says `I64`, and they agree because
/// the block supplied the argument.
#[test]
fn a_generic_interface_is_compared_after_substitution() {
    support::check(
        "\
interface Convert of Source:
    def convert(self, from: Source) -> I64

type Doc:
    title: String

Doc implements Convert of I64:
    def convert(self, from: I64) -> I64:
        from
",
    )
    .assert_clean();
}

/// The same substitution, refusing. `Bool` is not what `Convert of I64`
/// declares, and the message names `I64` and not `Source` — a diagnostic that
/// printed the interface's own parameter would name something the author of the
/// block cannot see.
#[test]
fn a_generic_interface_at_the_wrong_argument_is_refused() {
    let checked = support::check(
        "\
interface Convert of Source:
    def convert(self, from: Source) -> I64

type Doc:
    title: String

Doc implements Convert of I64:
    def convert(self, from: Bool) -> I64:
        1
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

/// One interface at two instantiations, which `methods`' §6 is about and which
/// this check must not disturb: each block is conformant at its own argument.
#[test]
fn one_interface_at_several_arguments_is_clean() {
    support::check(
        "\
interface From of Source:
    def from(value: Source) -> Self

type ParseError:
    detail: String

type IoError2:
    detail: String

type LoadError:
    detail: String

LoadError implements From of ParseError:
    def from(value: ParseError) -> Self:
        LoadError(detail: value.detail)

LoadError implements From of IoError2:
    def from(value: IoError2) -> Self:
        LoadError(detail: value.detail)
",
    )
    .assert_clean();
}

/// The associated type, answered — `examples/00_kitchen_sink.science`'s
/// `Countdown`. `Self.Item?` in the declaration and `Int?` in the block agree
/// because `type Item is Int` says so, and `body_substitution` binds both
/// spellings of the name.
#[test]
fn an_answered_associated_type_substitutes_into_the_declaration() {
    support::check(
        "\
type Countdown:
    remaining: Int

Countdown implements Iterate:
    type Item is Int

    def next(mutable self) -> Self.Item?:
        null
",
    )
    .assert_clean();
}

/// And writing the answer out rather than through `Self.Item` is the same
/// signature, which is what makes the substitution the right comparison and a
/// syntactic match the wrong one.
#[test]
fn the_associated_type_may_be_written_out_at_the_method() {
    support::check(
        "\
type Countdown:
    remaining: Int

Countdown implements Iterate:
    type Item is Int

    def next(mutable self) -> Int?:
        null
",
    )
    .assert_clean();
}

/// A prelude interface's declared method is a requirement, and this is the
/// diagnostic nothing could produce before: `Error` declares `message`, and a
/// block that writes something else has not implemented `Error`.
#[test]
fn a_prelude_interfaces_declared_method_is_required() {
    let checked = support::check(
        "\
type ConfigError:
    detail: String

ConfigError implements Error:
    def describe(self) -> String:
        self.detail
",
    );
    // `539` and not `539, 540`: §3's restraint keeps `describe` silent, because
    // `Error`'s transcription is partial and `describe` may yet be one of its
    // methods.
    assert_eq!(checked.codes(), vec![539]);
    assert_eq!(
        checked.messages(),
        vec!["this block does not implement `Error`'s method `message`"]
    );
}

#[test]
fn the_prelude_interface_implemented_correctly_is_clean() {
    support::check(
        "\
type ConfigError:
    detail: String

ConfigError implements Error:
    def message(self) -> String:
        String.new()
",
    )
    .assert_clean();
}

/// A marker interface has nothing to conform to, and the one-line form has no
/// block to look in. `examples/00_kitchen_sink.science` writes seven of these.
#[test]
fn a_marker_implementation_is_clean() {
    support::check(
        "\
type Doc:
    title: String

Doc implements Copy
",
    )
    .assert_clean();
}

/// An inherent block promises nothing, so it is held to nothing. This is the
/// program `SC0540` would have destroyed if the rule had been written over
/// every block rather than over the ones that name an interface.
#[test]
fn an_inherent_block_may_hold_anything() {
    support::check(
        "\
type Doc:
    title: String

Doc has:
    def anything(self) -> Int:
        1

    def at_all(self) -> Bool:
        true
",
    )
    .assert_clean();
}

// --- the silences, pinned -------------------------------------------------

/// `conform`'s §3. The fourteen methodless prelude interfaces are methodless
/// *deliberately* — `builtins.rs` refuses to invent `Ord.compare` or
/// `Display.display` — so a `clone` in a `Clone` block is not an extra method,
/// it is the method nobody has written the declaration for.
///
/// **This is the test that must fail the day `Clone.clone` is declared**, and
/// failing is the right outcome: the entry goes into `builtins.rs` and this
/// block becomes conformant rather than silent.
#[test]
fn a_methodless_prelude_interface_admits_any_method() {
    support::check(
        "\
type Doc:
    title: String

Doc implements Clone:
    def clone(self) -> Doc:
        Doc(title: String.new())

Doc implements Display:
    def whatever_this_is(self) -> Int:
        1
",
    )
    .assert_clean();
}

/// `conform`'s `unanswered`. An implementation that supplies `index` and not
/// `type Output is F64` is what `tests/operators.rs` writes five times, and the
/// element type the language uses is read off the block's own signature. So the
/// return is the *answer* to `Self.Output`, not a disagreement with it.
///
/// **The receiver and the arity are still compared**, which the next test is.
#[test]
fn an_unanswered_associated_type_is_not_compared() {
    support::check(
        "\
type Grid:
    cell: F64

Grid implements Index of I64:
    def index(self, at: I64) -> borrowed F64:
        borrowed self.cell
",
    )
    .assert_clean();
}

#[test]
fn an_unanswered_associated_type_does_not_excuse_the_receiver() {
    let checked = support::check(
        "\
type Grid:
    cell: F64

Grid implements Index of I64:
    def index(mutable self, at: I64) -> borrowed F64:
        borrowed self.cell
",
    );
    assert_eq!(checked.codes(), vec![541]);
}

/// An erroneous type on either side is silence, because `ty`'s §5 already
/// reported whatever produced it. `Missing` is not a type, so `render`'s
/// parameter is `Ty::ERROR` and the comparison declines rather than adding a
/// second diagnostic about the first one's symptom.
#[test]
fn an_erroneous_type_declines_the_comparison() {
    let checked = support::check_allowing_resolution_errors(
        "\
interface Render:
    def render(self, at: Missing) -> String

type Doc:
    title: String

Doc implements Render:
    def render(self, at: Bool) -> String:
        String.new()
",
    );
    assert!(
        !checked.codes().contains(&541),
        "an unresolved annotation must not become a conformance error: {:?}",
        checked.codes()
    );
}
