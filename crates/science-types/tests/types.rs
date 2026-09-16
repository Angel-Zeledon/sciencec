//! The type representation, and the lowering into it.
//!
//! Every claim `ty.rs` makes is a claim about *identity* — two spellings are
//! one type, two types are not one type — and an identity claim that is not
//! tested is one that drifts, because nothing about a `u32` says which type it
//! was supposed to be.
//!
//! These tests run the real pipeline, lexer through resolver, rather than
//! building `hir::Type` by hand. That is the opposite of what `tests/common`
//! does for the const layer, and for the opposite reason: the const layer's
//! contract is its *tree*, which the surface syntax could not yet produce,
//! whereas this layer's contract is *"what the resolver hands over becomes
//! this type"*. Building the HIR by hand would test the lowering against a
//! shape the resolver may not produce, which is exactly the way a lowering
//! goes stale.

use science_diagnostics::{Diagnostics, FileId, Span};
use science_resolve::hir::{self, DefKind, DefTable};
use science_types::ty::{GenericArg, Ty, TyKind, Types};
use science_types::{codes, AtomOrder, TypeLowerer};

// --- the fixture ---------------------------------------------------------

/// One program holding every shape the lowering has an arm for.
///
/// `spellings` is the one that matters most: `N + 1` and `1 + N` are written
/// in **one** signature on purpose, because two signatures would bind two
/// different `N`s — two atoms, two types, and a test that passed for the wrong
/// reason.
const FIXTURE: &str = "\
type Doc:
    title: String

type Matrix of (T, const N: Int):
    cells: Array of T

def spellings of (T, const N: Int)(
    left: Matrix of (T, N + 1),
    right: Matrix of (T, 1 + N),
    bare: Matrix of (T, N),
    parameter: T,
) -> Int:
    0

def shapes(
    plain: Doc,
    optional: Doc?,
    twice: Doc??,
    borrow: borrowed Doc,
    exclusive: mutable borrowed Doc,
    optional_borrow: (borrowed Doc)?,
    pair: (Doc, Int),
    docs: Array of Doc,
    failure: Error?,
    mapper: (Doc) -> Int,
    reversed: (Int) -> Doc,
) -> Int:
    0

def again(more: Array of Doc) -> Int:
    0

def value_as_type(width: 4) -> Int:
    0

def parameter_as_type of (const N: Int)(width: N) -> Int:
    0
";

/// A program the resolver reports on, for the error type.
const UNRESOLVED: &str = "\
def takes(x: Bogus) -> Int:
    0
";

fn parse_and_resolve(source: &str) -> (hir::Crate, Diagnostics) {
    let file = FileId(0);
    let (tokens, lex_diagnostics) = science_lexer::lex(file, source);
    assert!(!lex_diagnostics.has_errors(), "the fixture must lex");
    let (ast, parse_diagnostics) = science_parser::parse_module(&tokens, file);
    assert!(!parse_diagnostics.has_errors(), "the fixture must parse: {parse_diagnostics:?}");
    science_resolve::resolve_module(file, "types.science", &ast)
}

fn resolve(source: &str) -> hir::Crate {
    parse_and_resolve(source).0
}

/// The fixture is only evidence if it is a program. A `Res::Error` hiding in
/// it would make every `Ty` below compatible with every other one, and §5's
/// error rule would swallow the whole file.
#[test]
fn the_fixture_resolves_without_a_diagnostic() {
    let (_, diagnostics) = parse_and_resolve(FIXTURE);
    assert!(!diagnostics.has_errors(), "{diagnostics:?}");
}

fn function<'a>(krate: &'a hir::Crate, name: &str) -> &'a hir::Fn {
    for module in &krate.modules {
        for item in &module.items {
            if let hir::ItemKind::Fn(function) = &item.kind {
                if krate.defs.get(function.def).name == name {
                    return function;
                }
            }
        }
    }
    panic!("the fixture declares no function `{name}`");
}

fn param<'a>(krate: &'a hir::Crate, function_name: &str, param_name: &str) -> &'a hir::Type {
    let function = function(krate, function_name);
    for parameter in &function.params {
        if krate.defs.get(parameter.def).name == param_name {
            return &parameter.ty;
        }
    }
    panic!("`{function_name}` has no parameter `{param_name}`");
}

/// Everything a lowering needs, so that a test is three lines of assertion
/// rather than three lines of setup.
struct Harness {
    krate: hir::Crate,
    order: AtomOrder,
    types: Types,
    diagnostics: Diagnostics,
}

impl Harness {
    fn new(source: &str) -> Harness {
        let krate = resolve(source);
        let order = AtomOrder::of(&krate.defs);
        Harness { krate, order, types: Types::new(), diagnostics: Diagnostics::new() }
    }

    /// Lowers the named parameter's annotation.
    fn lower(&mut self, function_name: &str, param_name: &str) -> Ty {
        let ty = param(&self.krate, function_name, param_name).clone();
        let mut lowerer = TypeLowerer::new(
            &mut self.types,
            &self.krate.defs,
            &self.order,
            &mut self.diagnostics,
        );
        lowerer.lower(&ty)
    }

    fn render(&self, ty: Ty) -> String {
        self.types.render(&self.krate.defs, ty)
    }

    fn codes(&self) -> Vec<u16> {
        self.diagnostics.iter().map(|diagnostic| diagnostic.code.0).collect()
    }
}

// --- interning -----------------------------------------------------------

#[test]
fn one_type_written_in_two_functions_is_one_id() {
    // The claim §1 rests on: equality is `==`, so a type written twice has to
    // *be* the same index, not merely compare equal.
    let mut harness = Harness::new(FIXTURE);
    let here = harness.lower("shapes", "docs");
    let there = harness.lower("again", "more");
    assert_eq!(here, there);
    assert_eq!(harness.render(here), "Array of Doc");
}

#[test]
fn interning_the_same_kind_twice_does_not_grow_the_table() {
    let mut types = Types::new();
    let before = types.len();
    let first = types.tuple(vec![Ty::UNIT, Ty::UNIT]);
    let grown = types.len();
    let second = types.tuple(vec![Ty::UNIT, Ty::UNIT]);
    assert_eq!(first, second);
    assert_eq!(grown, before + 1);
    assert_eq!(types.len(), grown, "the second interning allocated nothing");
}

#[test]
fn the_error_and_unit_types_are_where_their_constants_say() {
    // `Ty::ERROR` and `Ty::UNIT` are `const`s over an index, which is a claim
    // about `Types::new`. `new` asserts it; this says so from outside, so that
    // the claim is a test failure rather than a panic in every other test.
    let types = Types::new();
    assert!(matches!(types.kind(Ty::ERROR), TyKind::Error));
    assert!(matches!(types.kind(Ty::UNIT), TyKind::Unit));
    assert_eq!(types.len(), 2);
}

#[test]
fn types_that_differ_only_in_a_flag_are_different_ids() {
    // `borrowed T` against `mutable borrowed T` is the cheapest way for an
    // interner keyed on a hand-written hash to be wrong, because the two
    // differ in one `bool` and nothing else.
    let mut harness = Harness::new(FIXTURE);
    let shared = harness.lower("shapes", "borrow");
    let exclusive = harness.lower("shapes", "exclusive");
    assert_ne!(shared, exclusive);
    assert_eq!(harness.render(shared), "borrowed Doc");
    assert_eq!(harness.render(exclusive), "mutable borrowed Doc");
}

// --- const arguments -----------------------------------------------------

#[test]
fn a_const_argument_written_two_ways_is_one_type() {
    // §3, and the reason interning cannot be syntactic. `N + 1` and `1 + N`
    // are two `hir::ConstExpr` trees with different shapes and different
    // spans; `normalise` makes them one `k + Σ cᵢ·aᵢ`, and the interner keys
    // on that.
    let mut harness = Harness::new(FIXTURE);
    let left = harness.lower("spellings", "left");
    let right = harness.lower("spellings", "right");
    assert_eq!(left, right);
    assert!(harness.types.compatible(left, right));
}

#[test]
fn a_bare_const_parameter_is_a_const_argument_and_not_a_type() {
    // The `is_type` compromise `hir::DefKind` documents, settled. `Matrix of
    // (T, N)` reaches `N` through a type position, and this is the phase that
    // records which of the two it was.
    let mut harness = Harness::new(FIXTURE);
    let bare = harness.lower("spellings", "bare");
    let TyKind::Named { args, .. } = harness.types.kind(bare).clone() else {
        panic!("`Matrix of (T, N)` is a named type");
    };
    assert_eq!(args.len(), 2);
    assert!(matches!(args[0], GenericArg::Type(_)), "`T` is a type argument");
    let form = args[1].as_const().expect("`N` is a const argument");
    assert_eq!(form.constant(), 0);
    assert_eq!(form.terms().len(), 1);
    assert_eq!(form.terms()[0].coefficient(), 1);
}

#[test]
fn one_const_argument_and_another_are_different_types() {
    // The other half: `Matrix of (T, N + 1)` and `Matrix of (T, N)` must not
    // collapse, or the normal form would be proving equalities it has not.
    let mut harness = Harness::new(FIXTURE);
    let shifted = harness.lower("spellings", "left");
    let bare = harness.lower("spellings", "bare");
    assert_ne!(shifted, bare);
    assert!(!harness.types.compatible(shifted, bare));
}

#[test]
fn a_type_parameter_lowers_to_a_parameter_and_not_to_a_name() {
    let mut harness = Harness::new(FIXTURE);
    let parameter = harness.lower("spellings", "parameter");
    assert!(matches!(harness.types.kind(parameter), TyKind::Param { .. }));
    assert_eq!(harness.render(parameter), "T");
}

// --- `T?` ----------------------------------------------------------------

#[test]
fn a_nullable_type_is_not_its_inner_type() {
    // Decision 6: `T?` is a distinct type, not a union and not a subtype. The
    // coercion of `T` into `T?` is real and is inference's; nothing here
    // should make the two one type, because then there would be nothing left
    // for the coercion to do.
    let mut harness = Harness::new(FIXTURE);
    let plain = harness.lower("shapes", "plain");
    let optional = harness.lower("shapes", "optional");
    assert_ne!(plain, optional);
    assert!(!harness.types.compatible(plain, optional));
    assert!(!harness.types.compatible(optional, plain));
    assert_eq!(harness.render(optional), "Doc?");
}

#[test]
fn a_double_nullable_collapses_and_is_reported_once() {
    // §3 of the lowering. The type is `Doc?`, the diagnostic is `SC0520`, and
    // there is exactly one of it.
    let mut harness = Harness::new(FIXTURE);
    let once = harness.lower("shapes", "optional");
    let twice = harness.lower("shapes", "twice");
    assert_eq!(once, twice, "`Doc??` is `Doc?`");
    assert_eq!(harness.codes(), vec![codes::DOUBLE_NULLABLE.0]);
}

#[test]
fn the_nullable_constructor_is_idempotent_on_its_own() {
    // The collapsing belongs to the table, not to the lowering, so that no
    // later phase can build the type `T??` by substituting into a `T?`.
    let mut types = Types::new();
    let once = types.nullable(Ty::UNIT);
    let twice = types.nullable(once);
    assert_eq!(once, twice);
}

#[test]
fn a_nullable_borrow_renders_with_the_parentheses_it_needs() {
    // `borrowed T?` parses as `borrowed (T?)`, so a rendering without the
    // parentheses is a rendering that reads back as a different type.
    let mut harness = Harness::new(FIXTURE);
    let optional_borrow = harness.lower("shapes", "optional_borrow");
    assert_eq!(harness.render(optional_borrow), "(borrowed Doc)?");
}

#[test]
fn a_bare_interface_under_a_question_mark_is_already_an_object() {
    // The resolver expanded `Error?` into `(any Error)?` in
    // `resolve_nullable_inner`, so by the time this phase sees it there is no
    // interface left to mistake for a type. Asserting the shape here is what
    // makes that a contract between the two phases rather than a coincidence.
    let mut harness = Harness::new(FIXTURE);
    let failure = harness.lower("shapes", "failure");
    let inner = match harness.types.kind(failure) {
        TyKind::Nullable(inner) => *inner,
        other => panic!("`Error?` is a nullable, not {other:?}"),
    };
    assert!(matches!(harness.types.kind(inner), TyKind::Object { .. }));
    assert_eq!(harness.render(failure), "any Error?");
    assert!(harness.codes().is_empty());
}

// --- the error type ------------------------------------------------------

#[test]
fn an_error_type_is_compatible_with_everything() {
    // §5. One bad annotation is one diagnostic, and this is the rule that
    // makes it so: the resolver reported `Bogus` once, and no comparison
    // against the type it produced ever reports again.
    let mut harness = Harness::new(FIXTURE);
    let doc = harness.lower("shapes", "plain");
    let optional = harness.lower("shapes", "optional");
    let pair = harness.lower("shapes", "pair");

    for ty in [Ty::ERROR, Ty::UNIT, doc, optional, pair] {
        assert!(harness.types.compatible(Ty::ERROR, ty));
        assert!(harness.types.compatible(ty, Ty::ERROR));
    }
    // And it is not a licence to agree with everything: the rule is about the
    // error type, not about `compatible`.
    assert!(!harness.types.compatible(doc, optional));
}

#[test]
fn an_unresolved_name_lowers_to_the_error_type() {
    let mut harness = Harness::new(UNRESOLVED);
    let bogus = harness.lower("takes", "x");
    assert_eq!(bogus, Ty::ERROR);
    // The resolver said `SC0200`. This phase says nothing, which is the
    // cascade not happening.
    assert!(harness.codes().is_empty());
    assert_eq!(harness.render(bogus), "{unknown}");
}

#[test]
fn an_error_inside_a_type_makes_the_whole_type_erroneous() {
    // The flag propagates, so `Array of {unknown}` is compatible with
    // `Array of Int` and with `String` alike. Without propagation, one
    // unresolved element type would report at every use of the array.
    let mut types = Types::new();
    let inner = types.tuple(vec![Ty::UNIT, Ty::ERROR]);
    let outer = types.borrowed(false, inner);
    let clean = types.borrowed(false, Ty::UNIT);
    assert!(types.references_error(inner));
    assert!(types.references_error(outer));
    assert!(!types.references_error(clean));
    assert!(types.compatible(outer, clean));
}

#[test]
fn an_erroneous_const_argument_poisons_its_type_too() {
    // `GenericArg::Error` is the const analogue of `Ty::ERROR`, and it has to
    // carry the flag or a type whose extent could not be lowered would start
    // reporting mismatches against every other extent.
    let mut defs = DefTable::new();
    let def = defs.alloc(DefKind::Record, "Grid", Span::at(FileId(0), 0), None);
    let mut types = Types::new();
    let broken = types.named(def, vec![GenericArg::Error]);
    let fine = types.named(def, vec![GenericArg::Type(Ty::UNIT)]);
    assert!(types.references_error(broken));
    assert!(!types.references_error(fine));
    assert!(types.compatible(broken, fine));
}

// --- a value where a type belongs ----------------------------------------

#[test]
fn a_const_expression_in_a_type_position_is_reported() {
    // §4 of the lowering. `def f(x: 4)` parses, resolves without complaint —
    // there is no name in it — and would otherwise check as though the
    // annotation were fine.
    let mut harness = Harness::new(FIXTURE);
    let width = harness.lower("value_as_type", "width");
    assert_eq!(width, Ty::ERROR);
    assert_eq!(harness.codes(), vec![codes::CONST_WHERE_TYPE_EXPECTED.0]);
}

#[test]
fn a_const_parameter_in_a_type_position_is_reported() {
    let mut harness = Harness::new(FIXTURE);
    let width = harness.lower("parameter_as_type", "width");
    assert_eq!(width, Ty::ERROR);
    assert_eq!(harness.codes(), vec![codes::CONST_WHERE_TYPE_EXPECTED.0]);
}

// --- rendering -----------------------------------------------------------

#[test]
fn a_type_renders_as_the_surface_syntax_that_would_write_it() {
    let mut harness = Harness::new(FIXTURE);
    let pair = harness.lower("shapes", "pair");
    assert_eq!(harness.render(pair), "(Doc, Int)");
    assert_eq!(harness.render(Ty::UNIT), "()");

    // A prelude type prints its bare name: the prelude's module is `core` and
    // no `use` can reach it, so a path a reader cannot write is noise.
    let docs = harness.lower("shapes", "docs");
    assert_eq!(harness.render(docs), "Array of Doc");

    // Two or more arguments take parentheses; one does not.
    let matrix = harness.lower("spellings", "bare");
    assert_eq!(harness.render(matrix), "Matrix of (T, N)");
}

#[test]
fn a_closure_type_is_its_shape_and_the_order_of_it_matters() {
    // `collections-and-chains.md` §1.2's closure type names no definition, so
    // the only thing that can tell `(Doc) -> Int` from `(Int) -> Doc` is the
    // position of each part in the key. An interner that hashed a set rather
    // than a list would call them one type.
    let mut harness = Harness::new(FIXTURE);
    let mapper = harness.lower("shapes", "mapper");
    let reversed = harness.lower("shapes", "reversed");
    assert_ne!(mapper, reversed);
    assert_eq!(harness.render(mapper), "(Doc) -> Int");
    assert_eq!(harness.render(reversed), "(Int) -> Doc");
}
