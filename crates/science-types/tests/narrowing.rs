//! Flow narrowing — §4.2's rules, and §4.3's acceptance test.
//!
//! §4.3 is explicit about what this file is for: field access on a narrowed
//! nullable *"is the operation the whole feature exists for. Without it the
//! model is cast-ridden and §3.1's claim that narrowing 'is what makes the
//! model bearable' is false. **It is therefore the acceptance test for this
//! section**"*. It is the first test below, written exactly as the note writes
//! it.

mod support;

use science_types::narrow::Fact;
use science_types::thir::{ExprKind, Place};
use support::check;

// --- §4.3, the acceptance test -------------------------------------------

const ACCEPTANCE: &str = "\
type Doc:
    title: String

def first_title(a: Doc?, b: Doc?) -> String?:
    if a?:
        return a.title
    if b?:
        return b.title
    null
";

#[test]
fn field_access_on_a_narrowed_nullable_is_the_acceptance_test() {
    let checked = check(ACCEPTANCE);
    checked.assert_clean();
}

#[test]
fn the_narrowed_read_is_a_node_and_its_type_is_the_payload() {
    // `thir`'s §1: a narrowed read is a node, not a retyped one, because
    // dropping the discriminant is a representation change MIR has to see.
    let checked = check(ACCEPTANCE);
    let body = checked.body("first_title");
    let narrows: Vec<String> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Narrow(_)))
        .map(|(_, expr)| checked.render(expr.ty))
        .collect();
    assert_eq!(narrows, vec!["Doc".to_string(), "Doc".to_string()]);
}

#[test]
fn the_field_of_a_narrowed_nullable_resolves_and_then_widens() {
    // Narrow `Doc?` to `Doc`, read `title` as `String`, widen to `String?` for
    // the return. Three of the layer's decisions in one expression, and the
    // one that is easy to get wrong is that the `Widen` is applied to the
    // *field*, not to the base.
    let checked = check(ACCEPTANCE);
    let body = checked.body("first_title");
    let fields: Vec<String> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Field { .. }))
        .map(|(_, expr)| checked.render(expr.ty))
        .collect();
    assert_eq!(fields, vec!["String".to_string(), "String".to_string()]);

    let coercions: Vec<String> = body
        .exprs()
        .filter(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. }))
        .map(|(_, expr)| checked.render(expr.ty))
        .collect();
    assert_eq!(coercions, vec!["String?".to_string(), "String?".to_string()]);
}

#[test]
fn a_field_of_a_nullable_that_was_not_tested_is_rejected() {
    // The other half of the acceptance test, and the one that says the first
    // half is not vacuous: without the `if`, `.title` is a field of `Doc?`,
    // which has none.
    let checked = check(
        "\
type Doc:
    title: String

def first_title(a: Doc?) -> String?:
    a.title
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

// --- §4.3's other example ------------------------------------------------

#[test]
fn a_narrowed_value_widens_back_to_the_nullable_return_type() {
    // §4.3's first example: `return a` with `a` narrowed to `Doc` against a
    // `Doc?` signature *"works by Decision 6's implicit widening"*.
    let checked = check(
        "\
type Doc:
    title: String

def pick(a: Doc?, b: Doc?) -> Doc?:
    if a?:
        return a
    b
",
    );
    checked.assert_clean();
    let body = checked.body("pick");
    assert!(body
        .exprs()
        .any(|(_, expr)| matches!(expr.kind, ExprKind::Coerce { .. })));
}

// --- §4.2, rule by rule --------------------------------------------------

#[test]
fn not_composes_and_narrows_the_other_way_round() {
    let checked = check(
        "\
type Doc:
    title: String

def title_or_empty(a: Doc?) -> String?:
    if not a?:
        return null
    a.title
",
    );
    checked.assert_clean();
}

#[test]
fn and_narrows_both_operands_in_the_then_branch() {
    let checked = check(
        "\
type Doc:
    title: String

def both(a: Doc?, b: Doc?) -> Bool:
    if a? and b?:
        return a.title is b.title
    false
",
    );
    checked.assert_clean();
}

#[test]
fn the_left_operand_of_an_and_narrows_the_right() {
    // Short-circuit narrowing: `a.title` in the right operand is only reached
    // when the left was true. `narrow`'s §2.
    let checked = check(
        "\
type Doc:
    title: String

def named(a: Doc?, wanted: String) -> Bool:
    a? and a.title is wanted
",
    );
    checked.assert_clean();
}

#[test]
fn or_narrows_neither_operand_in_the_then_branch() {
    // §4.2 in as many words: *"`or` narrows neither, because either may be the
    // reason"*. So `.title` inside the branch is a field of `Doc?`.
    let checked = check(
        "\
type Doc:
    title: String

def either(a: Doc?, b: Doc?) -> String?:
    if a? or b?:
        return a.title
    null
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn the_else_branch_of_an_or_narrows_both() {
    // `narrow`'s §5: the note left this undetermined and the dual is the only
    // answer that makes `not (a? or b?)` agree with `not a? and not b?`.
    let checked = check(
        "\
type Doc:
    title: String

def neither(a: Doc?, b: Doc?) -> Bool:
    if a? or b?:
        return true
    not a? and not b?
",
    );
    checked.assert_clean();
}

#[test]
fn an_early_return_narrows_for_the_rest_of_the_function() {
    // §4.2's third rule, and the one that *"makes the Go model bearable"*.
    // After `if not a?: return null`, `a` is non-null to the end of the body.
    let checked = check(
        "\
type Doc:
    title: String

def titled(a: Doc?) -> String?:
    if not a?:
        return null
    let first be a.title
    first
",
    );
    checked.assert_clean();
}

#[test]
fn a_field_projection_narrows_the_field_and_not_its_base() {
    // §4.2: *"`config.port?` narrows `config.port`, not `config`"*.
    let checked = check(
        "\
type Port:
    number: I64

type Config:
    port: Port?

def port_number(config: Config) -> I64:
    if config.port?:
        return config.port.number
    0
",
    );
    checked.assert_clean();
}

#[test]
fn writing_the_base_invalidates_a_fact_about_its_field() {
    // The prefix rule. `config be other` is a write to a prefix of
    // `config.port`, so the narrowing goes and `.number` is a field of `Port?`.
    let checked = check(
        "\
type Port:
    number: I64

type Config:
    port: Port?

def port_number(start: Config, other: Config) -> I64:
    let mutable config be start
    if config.port?:
        config be other
        return config.port.number
    0
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn creating_an_exclusive_borrow_invalidates_the_narrowing() {
    // Decision 8, and `narrow`'s §4: the invalidation is at the point the
    // exclusive borrow is *created*, not where it writes — because this phase
    // cannot see the writes, and rule 4 is what says it does not have to.
    let checked = check(
        "\
type Doc:
    title: String

def take(doc: mutable borrowed Doc) -> Bool:
    true

def use_after_borrow(start: Doc?) -> String?:
    let mutable a be start
    if a?:
        let _taken be take(mutable borrowed a)
        return a.title
    null
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn a_shared_borrow_leaves_the_narrowing_standing() {
    // The other half of Decision 8: only an *exclusive* borrow can write.
    let checked = check(
        "\
type Doc:
    title: String

def look(doc: borrowed Doc) -> Bool:
    true

def use_after_borrow(a: Doc?) -> String?:
    if a?:
        let _seen be look(borrowed a)
        return a.title
    null
",
    );
    checked.assert_clean();
}

#[test]
fn a_narrowing_does_not_survive_a_loop_that_writes_the_place() {
    // §4.2's loop rule, implemented as `narrow`'s §3: the meet is computed by
    // subtracting everything the body could write before the body is entered.
    let checked = check(
        "\
type Doc:
    title: String

def scan(start: Doc?, replacement: Doc?) -> String?:
    let mutable a be start
    if a?:
        loop:
            a be replacement
            break
        return a.title
    null
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn a_narrowing_survives_a_loop_that_writes_something_else() {
    let checked = check(
        "\
type Doc:
    title: String

def scan(a: Doc?, start: I64) -> String?:
    let mutable count be start
    if a?:
        loop:
            count be count + 1
            break
        return a.title
    null
",
    );
    checked.assert_clean();
}

#[test]
fn a_shared_method_call_on_the_receiver_keeps_the_narrowing() {
    // `narrow`'s §4, as it reads now that Decision 11's lookup answers it.
    // `length` takes `self`, so there is no way to write through the receiver
    // and the fact about it stands — which is what the rule always should have
    // said and could not, because nothing could see the `SelfKind`.
    let checked = check(
        "\
type Doc:
    title: String

Doc has:
    def length(self) -> I64:
        0

def touch(a: Doc?) -> String?:
    if a?:
        let _seen be a.length()
        return a.title
    null
",
    );
    checked.assert_clean();
}

#[test]
fn a_mutable_method_call_on_the_receiver_gives_up_the_narrowing() {
    // The other half, and the one that is Decision 8: `clear` takes `mutable
    // self`, an exclusive borrow of the receiver is what calling it is, and
    // the narrowing goes at the point the borrow is created. `a` is a `Doc?`
    // again afterwards, so `a.title` is `SC0528`.
    let checked = check(
        "\
type Doc:
    title: String

Doc has:
    def clear(mutable self):
        self.title be \"\"

def touch(a: Doc?) -> String?:
    if a?:
        a.clear()
        return a.title
    null
",
    );
    assert_eq!(checked.codes(), vec![528]);
}

#[test]
fn a_declared_shared_receiver_keeps_the_narrowing() {
    // What the declaration buys `narrow`'s §4, which had no `SelfKind` to read
    // and had to assume the worst. `String.length` takes `self`, so the call
    // is not a write, `a` is still known to be present at the `a?`, and the
    // presence test is `SC0530` — *"this value is never absent"* — which is the
    // diagnostic that could not fire before.
    let checked = check(
        "\
type Doc:
    title: String

def touch(a: String?) -> Bool:
    if a?:
        let _seen be a.length()
        return a?
    false
",
    );
    assert_eq!(checked.codes(), vec![530]);
}

#[test]
fn a_method_this_crate_cannot_resolve_still_gives_up_the_narrowing() {
    // What survives of the conservatism, asserted so that it stays a decision.
    // `slice` is one of the six `String` methods `builtins.rs` does not
    // transcribe, so it resolves to nothing, there is no `SelfKind` to read,
    // and the safe answer is the old one: the narrowing goes.
    let checked = check(
        "\
type Doc:
    title: String

def touch(a: String?) -> Bool:
    if a?:
        let _seen be a.slice(0)
        return a?
    false
",
    );
    // `a` is a `String?` again at the `a?`, so the presence test is not
    // `SC0530` — which is exactly the observation: the narrowing was lost.
    checked.assert_clean();
}

// --- the facts themselves -------------------------------------------------

#[test]
fn the_meet_of_two_paths_keeps_only_what_both_agree_on() {
    use science_types::narrow::Facts;
    let root = {
        let checked = check("def f():\n    0\n");
        checked.def("f", science_resolve::hir::DefKind::Fn)
    };
    let place = Place::local(root);
    let mut left = Facts::new();
    left.set(place.clone(), Fact::NonNull);
    let right = Facts::new();
    assert!(left.meet(&right).is_empty());
    assert_eq!(left.meet(&left).get(&place), Some(Fact::NonNull));
}

#[test]
fn a_diverging_branch_contributes_nothing_to_the_join() {
    use science_types::narrow::Facts;
    let root = {
        let checked = check("def f():\n    0\n");
        checked.def("f", science_resolve::hir::DefKind::Fn)
    };
    let place = Place::local(root);
    let mut then_facts = Facts::new();
    then_facts.set(place.clone(), Fact::NonNull);
    let mut else_facts = Facts::new();
    else_facts.set(place.clone(), Fact::Null);

    // The `if err?: return err` shape: the then-branch leaves, so what stands
    // afterwards is the else-branch's knowledge and not the intersection.
    let joined = Facts::join(then_facts.clone(), true, else_facts.clone(), false);
    assert_eq!(joined.get(&place), Some(Fact::Null));
    let both = Facts::join(then_facts, false, else_facts, false);
    assert_eq!(both.get(&place), None);
}

#[test]
fn a_write_to_a_sibling_field_leaves_the_fact_alone() {
    let checked = check(
        "\
type Port:
    number: I64

type Config:
    port: Port?
    other: Port?

def port_number(start: Config, replacement: Port?) -> I64:
    let mutable config be start
    if config.port?:
        config.other be replacement
        return config.port.number
    0
",
    );
    checked.assert_clean();
}
