//! The relations over the type table: alias expansion, substitution, and
//! assignability.
//!
//! Every claim here is a claim about *identity* again — `ty`'s tests say why
//! that has to be tested rather than read — but one layer up: two types that
//! the table says differ are made one by expansion, and two types that the
//! table says differ are made assignable by a coercion. Both are answers a
//! `u32` cannot carry, so nothing about the representation says either of them
//! drifted.
//!
//! These run the real pipeline, lexer through resolver, for the reason
//! `tests/types.rs` gives: the contract of this layer is *"what the resolver
//! hands over becomes this type"*, and a hand-built `hir::Type` would test the
//! relation against a shape the resolver may not produce. The exception is the
//! const half of substitution, which is `tests/common`'s territory and for
//! `tests/common`'s reason: the tree is the contract there, and the surface
//! syntax cannot yet write the forms that matter.

mod common;

use science_diagnostics::{Diagnostics, FileId, Span};
use science_resolve::hir::{self, DefKind};
use science_types::assign::{assignable, Coercion, Coercions, Site};
use science_types::ty::{GenericArg, Ty, TyKind, Types};
use science_types::{codes, Aliases, AtomOrder, NormalForm, Substitution, TypeLowerer};

// --- the fixture ---------------------------------------------------------

/// One program holding every shape the three relations have an arm for.
///
/// `Grid` and `Matrix` are declared together on purpose: `Grid of (T, N)` is an
/// alias whose body mentions a const parameter, which is the one place where
/// alias expansion and the const half of substitution meet, and the place a
/// test that only used `type Embedding is Array of F32` would never reach.
const FIXTURE: &str = "\
type Doc:
    title: String

type Embedding is Array of F32

type Pair of T is (T, T)

type Matrix of (T, const N: Int):
    cells: Array of T

type Grid of (T, const N: Int) is Matrix of (T, N)

interface Summarize:
    type Item
    def summarize(self) -> Self.Item

Doc has:
    def copy(self) -> Self:
        self

def shapes(
    alias_written: Embedding,
    expansion_written: Array of F32,
    nested_alias: Array of Embedding,
    nested_expansion: Array of (Array of F32),
    pair_of_docs: Pair of Doc,
    docs_tuple: (Doc, Doc),
    doc: Doc,
    optional_doc: Doc?,
    docs: Array of Doc,
    optional_docs: Array of (Doc?),
    failure: Error?,
    boxed: any Error,
    summarizer: any Summarize,
    borrowed_doc: borrowed Doc,
) -> Int:
    0

def spellings of (T, const N: Int, const K: Int)(
    shifted: Matrix of (T, N + 1),
    substituted: Matrix of (T, K + 1),
    bare: Matrix of (T, N),
    concrete: Matrix of (F32, N),
    aliased: Grid of (T, N),
    parameter: T,
    nullable_parameter: T?,
) -> Int:
    0
";

/// The whole pipeline, plus the alias table built over it.
struct Program {
    krate: hir::Crate,
    order: AtomOrder,
    types: Types,
    aliases: Aliases,
    /// Everything the alias pass and the lowerings reported. Resolution's own
    /// diagnostics are asserted empty by [`Program::new`].
    diagnostics: Diagnostics,
}

impl Program {
    fn new(source: &str) -> Program {
        let file = FileId(0);
        let (tokens, lex_diagnostics) = science_lexer::lex(file, source);
        assert!(!lex_diagnostics.has_errors(), "the fixture must lex");
        let (ast, parse_diagnostics) = science_parser::parse_module(&tokens, file);
        assert!(!parse_diagnostics.has_errors(), "the fixture must parse: {parse_diagnostics:?}");
        let (krate, resolve_diagnostics) =
            science_resolve::resolve_module(file, "relations.science", &ast);
        // A `Res::Error` hiding in the fixture would make every type below
        // compatible with every other one, and `ty`'s §5 would swallow the file.
        assert!(!resolve_diagnostics.has_errors(), "{resolve_diagnostics:?}");

        let order = AtomOrder::of(&krate.defs);
        let mut types = Types::new();
        let mut diagnostics = Diagnostics::new();
        let aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
        Program { krate, order, types, aliases, diagnostics }
    }

    /// Lowers the named parameter's annotation.
    fn ty(&mut self, function_name: &str, param_name: &str) -> Ty {
        let annotation = self.param(function_name, param_name).clone();
        self.lower(&annotation)
    }

    /// Lowers the named method's return type.
    fn ret(&mut self, method_name: &str) -> Ty {
        let annotation = self.method(method_name).ret.clone().expect("the method returns a type");
        self.lower(&annotation)
    }

    fn lower(&mut self, annotation: &hir::Type) -> Ty {
        let mut lowerer = TypeLowerer::new(
            &mut self.types,
            &self.krate.defs,
            &self.order,
            &mut self.diagnostics,
        );
        lowerer.lower(annotation)
    }

    fn reveal(&mut self, ty: Ty) -> Ty {
        self.aliases.reveal(&mut self.types, ty).expect("the fixture's aliases do not overflow")
    }

    /// Lowers and reveals, which is what a checker does at an annotation.
    fn revealed(&mut self, function_name: &str, param_name: &str) -> Ty {
        let ty = self.ty(function_name, param_name);
        self.reveal(ty)
    }

    fn render(&self, ty: Ty) -> String {
        self.types.render(&self.krate.defs, ty)
    }

    fn codes(&self) -> Vec<u16> {
        self.diagnostics.iter().map(|diagnostic| diagnostic.code.0).collect()
    }

    fn coercions(&self) -> Coercions {
        Coercions::of(&self.krate.defs)
    }

    /// The definition with this name and kind. The fixture declares each such
    /// name once, which is what makes the lookup a test helper and not a
    /// resolver.
    fn def(&self, name: &str, kind: DefKind) -> hir::DefId {
        self.krate
            .defs
            .iter()
            .find(|def| def.name == name && def.kind == kind)
            .unwrap_or_else(|| panic!("the fixture declares no {kind:?} named `{name}`"))
            .id
    }

    /// A generic parameter of one function, by name.
    ///
    /// **Not [`Program::def`]**, and the difference cost an afternoon: the
    /// fixture declares four `T`s and three `N`s, because a generic parameter
    /// belongs to the item that binds it. A search over the whole table finds
    /// whichever was allocated first, which is a substitution for somebody
    /// else's parameter — and that is not a panic, it is a fold that changes
    /// nothing and an assertion that the types are equal when they are two.
    fn generic(&self, function_name: &str, param_name: &str) -> hir::DefId {
        self.function(function_name)
            .generics
            .iter()
            .map(|param| param.def)
            .find(|def| self.krate.defs.get(*def).name == param_name)
            .unwrap_or_else(|| panic!("`{function_name}` binds no `{param_name}`"))
    }

    fn function(&self, name: &str) -> &hir::Fn {
        for module in &self.krate.modules {
            if let Some(found) = module.items.iter().find_map(|item| match &item.kind {
                hir::ItemKind::Fn(function)
                    if self.krate.defs.get(function.def).name == name =>
                {
                    Some(function)
                }
                _ => None,
            }) {
                return found;
            }
        }
        panic!("the fixture declares no function `{name}`");
    }

    /// A method of an implementation block, or of an interface.
    fn method(&self, name: &str) -> &hir::Fn {
        for module in &self.krate.modules {
            for item in &module.items {
                let methods = match &item.kind {
                    hir::ItemKind::Impl(block) => &block.methods,
                    hir::ItemKind::Interface(interface) => &interface.methods,
                    _ => continue,
                };
                if let Some(found) =
                    methods.iter().find(|m| self.krate.defs.get(m.def).name == name)
                {
                    return found;
                }
            }
        }
        panic!("the fixture declares no method `{name}`");
    }

    fn param(&self, function_name: &str, param_name: &str) -> &hir::Type {
        let function = self.function(function_name);
        for parameter in &function.params {
            if self.krate.defs.get(parameter.def).name == param_name {
                return &parameter.ty;
            }
        }
        panic!("`{function_name}` has no parameter `{param_name}`");
    }
}

#[test]
fn the_fixture_declares_the_aliases_and_reports_nothing() {
    let program = Program::new(FIXTURE);
    assert_eq!(program.aliases.len(), 3, "`Embedding`, `Pair` and `Grid`");
    assert!(program.codes().is_empty(), "{:?}", program.diagnostics);
}

// --- alias expansion -----------------------------------------------------

#[test]
fn an_alias_and_its_expansion_are_one_type_once_revealed() {
    // `ty`'s §6 priced this exactly: "until then, `Embedding` and
    // `Array of F32` are two `Ty`s denoting one type, and `compatible` says
    // they differ".
    let mut program = Program::new(FIXTURE);
    let alias = program.ty("shapes", "alias_written");
    let expansion = program.ty("shapes", "expansion_written");
    assert_ne!(alias, expansion, "the table still holds the name the author wrote");
    assert!(!program.types.compatible(alias, expansion));

    let revealed_alias = program.reveal(alias);
    let revealed_expansion = program.reveal(expansion);
    assert_eq!(revealed_alias, revealed_expansion);
    assert_eq!(program.render(revealed_alias), "Array of F32");
    // §1: the written type keeps its name, so a diagnostic can still say it.
    assert_eq!(program.render(alias), "Embedding");
}

#[test]
fn an_alias_inside_a_type_is_expanded_too() {
    let mut program = Program::new(FIXTURE);
    let nested_alias = program.revealed("shapes", "nested_alias");
    let nested_expansion = program.revealed("shapes", "nested_expansion");
    assert_eq!(nested_alias, nested_expansion);
    assert_eq!(program.render(nested_alias), "Array of Array of F32");
}

#[test]
fn a_generic_alias_substitutes_its_argument_into_its_body() {
    let mut program = Program::new(FIXTURE);
    let pair = program.revealed("shapes", "pair_of_docs");
    let tuple = program.revealed("shapes", "docs_tuple");
    assert_eq!(pair, tuple);
    assert_eq!(program.render(pair), "(Doc, Doc)");
}

#[test]
fn a_generic_alias_carries_its_const_argument_through() {
    // The meeting point: expanding `Grid of (T, N)` substitutes a *const*
    // parameter into the alias body, so this fails if either half of
    // `subst` is missing.
    let mut program = Program::new(FIXTURE);
    let aliased = program.revealed("spellings", "aliased");
    let bare = program.revealed("spellings", "bare");
    assert_eq!(aliased, bare);
    assert_eq!(program.render(aliased), "Matrix of (T, N)");
}

#[test]
fn revealing_a_type_with_no_alias_in_it_hands_back_the_same_type() {
    // `fold`'s §3: unchanged means unchanged, by identity, so a checker that
    // reveals every annotation pays nothing for the ones with no alias.
    let mut program = Program::new(FIXTURE);
    let docs = program.ty("shapes", "docs");
    assert_eq!(program.reveal(docs), docs);
}

#[test]
fn a_cyclic_alias_is_a_diagnostic_and_not_a_hang() {
    let program = Program::new(
        "\
type A is B
type B is A
",
    );
    assert_eq!(program.codes(), vec![codes::CYCLIC_ALIAS.0]);
    let diagnostic = program.diagnostics.iter().next().expect("one diagnostic");
    // Both declarations are labelled: neither one is the mistake on its own.
    assert_eq!(diagnostic.labels.len(), 2);
}

#[test]
fn an_alias_that_is_its_own_body_is_the_same_one_diagnostic() {
    let program = Program::new("type Loop is Loop\n");
    assert_eq!(program.codes(), vec![codes::CYCLIC_ALIAS.0]);
}

#[test]
fn an_alias_reaching_a_cycle_is_not_reported_a_second_time() {
    // `C` is not itself cyclic; it merely mentions something that is. One
    // mistake, one diagnostic — `alias`'s §2.
    let mut program = Program::new(
        "\
type A is B
type B is A
type C is A

def uses(c: C) -> Int:
    0
",
    );
    assert_eq!(program.codes(), vec![codes::CYCLIC_ALIAS.0]);

    // And revealing the poisoned alias terminates, producing the error type,
    // which `ty`'s §5 makes compatible with everything so that the one
    // diagnostic stays one.
    let revealed = program.revealed("uses", "c");
    assert!(program.types.references_error(revealed));
}

#[test]
fn a_cycle_through_a_generic_argument_is_still_a_cycle() {
    let program = Program::new(
        "\
type Wrapper of T is Array of T
type Knot is Wrapper of Knot
",
    );
    assert_eq!(program.codes(), vec![codes::CYCLIC_ALIAS.0]);
}

#[test]
fn a_chain_of_aliases_expands_in_linear_space() {
    // Twenty levels of `type An is (An-1, An-1)` denotes a tree with 2^20
    // leaves and a *graph* with twenty nodes. `alias`'s §3 is the memo that
    // keeps the walk on the graph; without it this test does not finish in the
    // time anyone waits for a test suite.
    let mut source = String::from("type A0 is Int\n");
    for level in 1..=20 {
        source.push_str(&format!("type A{level} is (A{}, A{})\n", level - 1, level - 1));
    }
    source.push_str("\ndef uses(deep: A20) -> Int:\n    0\n");

    let mut program = Program::new(&source);
    assert!(program.codes().is_empty());
    let before = program.types.len();
    let revealed = program.revealed("uses", "deep");
    let grown = program.types.len() - before;
    assert!(grown < 100, "expansion interned {grown} types for twenty levels");
    assert!(matches!(program.types.kind(revealed), TyKind::Tuple(_)));
}

// --- substitution, the type half -----------------------------------------

#[test]
fn an_empty_substitution_is_the_identity() {
    let mut program = Program::new(FIXTURE);
    let ty = program.ty("spellings", "shifted");
    let subst = Substitution::new();
    assert!(subst.is_empty());
    assert_eq!(subst.apply(&mut program.types, ty).unwrap(), ty);
}

#[test]
fn substituting_a_type_parameter_produces_the_type_that_was_written_out() {
    // The identity claim: substituting `T := F32` into `Matrix of (T, N)` is
    // not merely *equal* to the `Matrix of (F32, N)` in the source, it is the
    // same index, because both went through the same interner.
    let mut program = Program::new(FIXTURE);
    let bare = program.ty("spellings", "bare");
    let concrete = program.ty("spellings", "concrete");
    let f32_ty = {
        let float = program.def("F32", DefKind::Primitive);
        program.types.named(float, Vec::new())
    };
    let t = program.generic("spellings", "T");

    let subst = Substitution::new().with_type(t, f32_ty);
    assert_eq!(subst.apply(&mut program.types, bare).unwrap(), concrete);
}

#[test]
fn substituting_a_nullable_type_into_a_nullable_does_not_build_a_double() {
    // `ty`'s §4 predicted this case and ruled on it: the collapse is the
    // table's, and it is *not* `SC0520`, because there is one `?` in the
    // source and one in the argument.
    let mut program = Program::new(FIXTURE);
    let nullable_param = program.ty("spellings", "nullable_parameter");
    let optional_doc = program.ty("shapes", "optional_doc");
    let t = program.generic("spellings", "T");

    let subst = Substitution::new().with_type(t, optional_doc);
    let substituted = subst.apply(&mut program.types, nullable_param).unwrap();
    assert_eq!(substituted, optional_doc);
    assert_eq!(program.render(substituted), "Doc?");
    assert!(program.codes().is_empty(), "no diagnostic: nobody wrote `??`");
}

#[test]
fn self_is_replaced_by_the_implementing_type_and_only_in_its_own_block() {
    let mut program = Program::new(FIXTURE);
    let self_ty = program.ret("copy");
    let doc = program.ty("shapes", "doc");
    let TyKind::SelfType { owner } = *program.types.kind(self_ty) else {
        panic!("`Self` lowers to a `SelfType`");
    };

    let here = Substitution::new().with_self(owner, doc);
    assert_eq!(here.apply(&mut program.types, self_ty).unwrap(), doc);

    // `subst`'s §2: a `Self` belonging to another block keeps its meaning.
    let elsewhere = program.def("Summarize", DefKind::Interface);
    let there = Substitution::new().with_self(elsewhere, doc);
    assert_eq!(there.apply(&mut program.types, self_ty).unwrap(), self_ty);
}

#[test]
fn an_unbound_associated_type_is_left_standing_rather_than_poisoned() {
    // `subst`'s §2: not wrong, not yet known. Turning it into `Ty::ERROR` here
    // would make it compatible with everything, which is a claim this phase
    // has no evidence for.
    let mut program = Program::new(FIXTURE);
    let item = program.ret("summarize");
    assert!(matches!(program.types.kind(item), TyKind::SelfAssoc { .. }));
    assert_eq!(program.render(item), "Self.Item");

    let doc = program.ty("shapes", "doc");
    let unrelated = Substitution::new().with_type(program.generic("spellings", "T"), doc);
    assert_eq!(unrelated.apply(&mut program.types, item).unwrap(), item);

    let assoc = program.def("Item", DefKind::AssocType);
    let bound = Substitution::new().with_assoc(assoc, doc);
    assert_eq!(bound.apply(&mut program.types, item).unwrap(), doc);
}

// --- substitution, the const half ----------------------------------------

#[test]
fn substituting_a_const_parameter_rewrites_the_normal_form_inside_the_type() {
    // `Matrix of (T, N + 1)` at `N := K` is the `Matrix of (T, K + 1)` the
    // fixture writes out — one index, not two types that happen to render the
    // same.
    let mut program = Program::new(FIXTURE);
    let shifted = program.ty("spellings", "shifted");
    let expected = program.ty("spellings", "substituted");
    let (n, k) = (program.generic("spellings", "N"), program.generic("spellings", "K"));
    let k_form = NormalForm::atom(program.order.atom(k), Span::new(FileId(0), 0, 1));

    let subst = Substitution::new().with_const(n, k_form);
    let substituted = subst.apply(&mut program.types, shifted).unwrap();
    assert_eq!(substituted, expected);
    assert_eq!(program.render(substituted), "Matrix of (T, 1 + K)");
}

#[test]
fn substituting_a_const_parameter_with_a_literal_makes_a_constant_extent() {
    let mut program = Program::new(FIXTURE);
    let bare = program.ty("spellings", "bare");
    let n = program.generic("spellings", "N");

    let subst = Substitution::new().with_const(n, NormalForm::literal(4));
    let substituted = subst.apply(&mut program.types, bare).unwrap();
    assert_eq!(program.render(substituted), "Matrix of (T, 4)");

    let parameter = program.ty("spellings", "parameter");
    let TyKind::Named { args, .. } = program.types.kind(substituted) else {
        panic!("a named type");
    };
    assert_eq!(args[1].as_const().and_then(NormalForm::as_constant), Some(4));
    assert_eq!(args[0], GenericArg::Type(parameter), "the type argument is untouched");
}

#[test]
fn an_overflowing_substitution_is_caught_and_not_wrapped() {
    // `normal`'s §5 ruled on this once for `SCALE`: the two alternatives to
    // refusing are both "the checker states an equality that is false".
    // Substitution is where the large value and the coefficient meet.
    let mut scope = common::Scope::new();
    let n = scope.param("N");
    let doubled = NormalForm::atom(common::atom(n), scope.span()).scale(2).unwrap();

    let subst = Substitution::new().with_const(n, NormalForm::literal(i128::MAX));
    let error = subst.apply_const(&doubled).expect_err("2 * i128::MAX is not an i128");
    assert!(science_types::diagnostics::overflowed(error, scope.span()).code
        == codes::NOT_A_CONST_EXPRESSION);
}

#[test]
fn a_substitution_that_does_not_touch_a_form_leaves_it_alone() {
    let mut scope = common::Scope::new();
    let (n, m) = (scope.param("N"), scope.param("M"));
    let form = NormalForm::atom(common::atom(n), scope.span());

    let subst = Substitution::new().with_const(m, NormalForm::literal(7));
    assert_eq!(subst.apply_const(&form).unwrap(), form);
}

#[test]
fn substitution_keeps_the_provenance_of_every_term_it_did_not_touch() {
    // `subst`'s §3 is the whole of this test: §9.3's derivation block is one
    // row per contributing operand, and a substitution that rebuilt the form
    // could carry at most one span per term.
    let mut scope = common::Scope::new();
    let (n, m) = (scope.param("N"), scope.param("M"));
    let (first, second, third) = (scope.span(), scope.span(), scope.span());

    // `M + M + N`, so `M`'s term carries two spans and `N`'s carries one.
    let form = NormalForm::atom(common::atom(m), first)
        .merge(&NormalForm::atom(common::atom(m), second))
        .unwrap()
        .merge(&NormalForm::atom(common::atom(n), third))
        .unwrap();

    let subst = Substitution::new()
        .with_const(n, NormalForm::literal(0).merge(&NormalForm::literal(5)).unwrap());
    let substituted = subst.apply_const(&form).unwrap();

    assert_eq!(substituted.constant(), 5);
    assert_eq!(substituted.terms().len(), 1, "`N` is gone and `M` remains");
    assert_eq!(substituted.terms()[0].coefficient(), 2);
    assert_eq!(
        substituted.terms()[0].provenance(),
        &[first, second],
        "both operands that made `2*M` are still there"
    );
}

#[test]
fn a_substituted_term_takes_the_provenance_of_what_replaced_it() {
    // And the reason: after the substitution, the operand a reader has to look
    // at for that extent is the argument at the instantiation.
    let mut scope = common::Scope::new();
    let (n, k) = (scope.param("N"), scope.param("K"));
    let (declaration, instantiation) = (scope.span(), scope.span());

    let form = NormalForm::atom(common::atom(n), declaration).scale(3).unwrap();
    let subst =
        Substitution::new().with_const(n, NormalForm::atom(common::atom(k), instantiation));
    let substituted = subst.apply_const(&form).unwrap();

    assert_eq!(substituted.terms().len(), 1);
    assert_eq!(substituted.terms()[0].coefficient(), 3);
    assert_eq!(substituted.terms()[0].provenance(), &[instantiation]);
}

// --- assignability -------------------------------------------------------

/// `assignable` with the fixture's prelude and a site.
fn fits(program: &Program, site: Site, source: Ty, target: Ty) -> Option<Coercion> {
    assignable(&program.types, program.coercions(), site, source, target)
}

#[test]
fn the_prelude_error_interface_is_found() {
    // Every boxing test below is vacuous if it is not: with no `Error` in the
    // table, rule 3 cannot fire and the relation quietly becomes Decision 6
    // alone.
    let program = Program::new(FIXTURE);
    assert!(program.coercions().error_interface().is_some());
}

#[test]
fn a_type_is_assignable_to_its_nullable_and_never_the_reverse() {
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    let optional = program.ty("shapes", "optional_doc");

    assert_eq!(fits(&program, Site::Elsewhere, doc, optional), Some(Coercion::Widen));
    assert_eq!(fits(&program, Site::Return, doc, optional), Some(Coercion::Widen));
    // Decision 6: "`T?` never coerces to `T`". This is the direction that
    // would make the null check optional and §4 of the note pointless.
    assert_eq!(fits(&program, Site::Return, optional, doc), None);
    assert_eq!(fits(&program, Site::Argument, optional, doc), None);
}

#[test]
fn a_type_is_assignable_to_itself_with_no_conversion() {
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    let optional = program.ty("shapes", "optional_doc");
    assert_eq!(fits(&program, Site::Elsewhere, doc, doc), Some(Coercion::Identity));
    assert_eq!(fits(&program, Site::Elsewhere, optional, optional), Some(Coercion::Identity));
}

#[test]
fn a_concrete_type_boxes_at_a_return_and_at_an_argument_and_nowhere_else() {
    // Decision 14, and the "nowhere else" is the half that needs a test:
    // it is invisible in the code that has it and invisible in the code that
    // does not.
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    let boxed = program.ty("shapes", "boxed");

    assert_eq!(fits(&program, Site::Return, doc, boxed), Some(Coercion::Box));
    assert_eq!(fits(&program, Site::Argument, doc, boxed), Some(Coercion::Box));
    assert_eq!(fits(&program, Site::Elsewhere, doc, boxed), None);
}

#[test]
fn a_concrete_type_boxes_into_the_nullable_object_a_fallible_signature_names() {
    // `-> (T, Error?)` is the signature Decision 14 was written against, and
    // `(any Error)?` is what `Error?` means after the resolver's shorthand. So
    // the coercion is two steps and this is the one that actually fires in a
    // real program.
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    let failure = program.ty("shapes", "failure");

    assert_eq!(fits(&program, Site::Return, doc, failure), Some(Coercion::BoxThenWiden));
    assert_eq!(fits(&program, Site::Elsewhere, doc, failure), None);
}

#[test]
fn a_concrete_type_does_not_box_into_an_interface_that_is_not_error() {
    // §6.2: Decision 14 is "the one implicit coercion in the language besides
    // `T` into `T?`". An `any Summarize` is constructed where it is written.
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    let summarizer = program.ty("shapes", "summarizer");
    assert_eq!(fits(&program, Site::Return, doc, summarizer), None);
    assert_eq!(fits(&program, Site::Argument, doc, summarizer), None);
}

#[test]
fn a_borrow_and_a_nullable_do_not_box() {
    // `assign`'s `boxable`: `any Error` owns its value, and boxing something
    // that may be absent is a conversion that runs or does not depending on
    // the value.
    let mut program = Program::new(FIXTURE);
    let borrowed = program.ty("shapes", "borrowed_doc");
    let optional = program.ty("shapes", "optional_doc");
    let boxed = program.ty("shapes", "boxed");
    let failure = program.ty("shapes", "failure");

    assert_eq!(fits(&program, Site::Return, borrowed, boxed), None);
    assert_eq!(fits(&program, Site::Return, optional, boxed), None);
    assert_eq!(fits(&program, Site::Return, optional, failure), None);
}

#[test]
fn a_coercion_does_not_recurse_into_a_container() {
    // `assign`'s §2. Both coercions change the representation of the value, so
    // applying one under a type constructor is a loop over a container that
    // already exists, at an assignment that looks free.
    let mut program = Program::new(FIXTURE);
    let docs = program.ty("shapes", "docs");
    let optional_docs = program.ty("shapes", "optional_docs");
    let tuple = program.ty("shapes", "docs_tuple");

    assert_eq!(fits(&program, Site::Return, docs, optional_docs), None);
    assert_eq!(fits(&program, Site::Argument, docs, optional_docs), None);
    // And the tuple that Decision 14's own example returns: the *type* is not
    // assignable, and the line works because the checker visits the tuple
    // expression's elements one at a time. `assign`'s §2 and §5.
    let boxed_pair = {
        let boxed = program.ty("shapes", "boxed");
        let doc = program.ty("shapes", "doc");
        program.types.tuple(vec![doc, boxed])
    };
    assert_eq!(fits(&program, Site::Return, tuple, boxed_pair), None);
}

#[test]
fn an_already_reported_type_is_assignable_in_both_directions() {
    // `ty`'s §5, reached through this relation: one bad annotation stays one
    // diagnostic, and the way it stays one is that everything downstream
    // agrees with it.
    let mut program = Program::new(FIXTURE);
    let doc = program.ty("shapes", "doc");
    assert_eq!(fits(&program, Site::Elsewhere, Ty::ERROR, doc), Some(Coercion::Identity));
    assert_eq!(fits(&program, Site::Elsewhere, doc, Ty::ERROR), Some(Coercion::Identity));
}

#[test]
fn an_alias_reaches_its_expansion_through_assignability_only_once_revealed() {
    // The seam this module's opening paragraph states: the relation takes
    // revealed types, and the test that catches a caller who forgot is this
    // one.
    let mut program = Program::new(FIXTURE);
    let alias = program.ty("shapes", "alias_written");
    let expansion = program.ty("shapes", "expansion_written");
    assert_eq!(fits(&program, Site::Elsewhere, alias, expansion), None);

    let alias = program.reveal(alias);
    let expansion = program.reveal(expansion);
    assert_eq!(fits(&program, Site::Elsewhere, alias, expansion), Some(Coercion::Identity));
}

#[test]
fn an_alias_widens_into_a_nullable_of_its_expansion() {
    // The two relations composing: reveal, then Decision 6.
    let mut program = Program::new(FIXTURE);
    let alias = program.revealed("shapes", "alias_written");
    let expansion = program.revealed("shapes", "expansion_written");
    let nullable_expansion = program.types.nullable(expansion);
    assert_eq!(fits(&program, Site::Elsewhere, alias, nullable_expansion), Some(Coercion::Widen));
}

#[test]
fn with_no_prelude_the_relation_is_compatible_plus_decision_six() {
    // `Coercions::new` is what a table built by hand gets, and the claim is
    // that it removes exactly one rule rather than changing any other.
    let mut types = Types::new();
    let unit_or_nothing = types.nullable(Ty::UNIT);
    let coercions = Coercions::new();
    assert_eq!(
        assignable(&types, coercions, Site::Return, Ty::UNIT, unit_or_nothing),
        Some(Coercion::Widen)
    );
    assert_eq!(assignable(&types, coercions, Site::Return, unit_or_nothing, Ty::UNIT), None);
}
