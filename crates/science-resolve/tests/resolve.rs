//! What name resolution must do, asserted on the resolved tree rather than on
//! a rendered dump: the snapshots in `snapshots.rs` guard the rendering, these
//! guard the meaning.

mod common;

use common::*;
use science_parser::ast::SelfKind;
use science_resolve::hir::{
    self, Crate, DefKind, ExprKind, GenericArity, PatternKind, Res, StmtKind, TypeKind,
};

// --- reaching into the result -------------------------------------------

fn resolve(module: &science_parser::ast::Module) -> (Crate, Vec<String>) {
    let (krate, diagnostics) = science_resolve::resolve_module(FILE, "main.science", module);
    let codes = codes(&diagnostics);
    (krate, codes)
}

/// The definition a test means: the one written in its source, never the
/// prelude's, which is allocated first and shares names like `None`.
fn def_of(krate: &Crate, kind: DefKind, name: &str) -> hir::DefId {
    krate
        .defs
        .iter()
        .find(|d| d.kind == kind && d.name == name && !d.is_builtin())
        .unwrap_or_else(|| panic!("no {kind:?} named `{name}` in the def table"))
        .id
}

/// The items of the one and only module.
fn items(krate: &Crate) -> &[hir::Item] {
    &krate.modules[0].items
}

fn nth_fn(krate: &Crate, index: usize) -> &hir::Fn {
    match &items(krate)[index].kind {
        hir::ItemKind::Fn(f) => f,
        other => panic!("item {index} is not a function: {other:?}"),
    }
}

fn nth_record(krate: &Crate, index: usize) -> &hir::Record {
    match &items(krate)[index].kind {
        hir::ItemKind::Record(r) => r,
        other => panic!("item {index} is not a record: {other:?}"),
    }
}

fn nth_impl(krate: &Crate, index: usize) -> &hir::Impl {
    match &items(krate)[index].kind {
        hir::ItemKind::Impl(i) => i,
        other => panic!("item {index} is not an impl: {other:?}"),
    }
}

fn tail_of(f: &hir::Fn) -> &hir::Expr {
    f.body.as_ref().expect("no body").tail.as_ref().expect("no tail expression")
}

fn path_res(expr: &hir::Expr) -> Res {
    match &expr.kind {
        ExprKind::Path { res, .. } => *res,
        other => panic!("not a path: {other:?}"),
    }
}

// --- collecting definitions ---------------------------------------------

#[test]
fn a_function_may_call_one_declared_later_in_the_file() {
    let sp = &Sp::new();
    // fn first(): second()
    // fn second(): ()
    let callee = name(sp, &["second"]);
    let body = block(sp, vec![], Some(call(sp, callee, vec![])));
    let first = func(sp, "first").body(body).item();
    let second_body = block(sp, vec![], None);
    let second = func(sp, "second").body(second_body).item();

    let (krate, codes) = resolve(&module(vec![first, second]));

    assert!(codes.is_empty(), "forward reference must resolve: {codes:?}");
    let second_id = def_of(&krate, DefKind::Fn, "second");
    match &tail_of(nth_fn(&krate, 0)).kind {
        ExprKind::Call { callee, .. } => assert_eq!(path_res(callee), Res::Def(second_id)),
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn two_types_may_refer_to_each_other() {
    let sp = &Sp::new();
    // struct A: b: B
    // struct B: a: A
    let b_ty = ty(sp, "B");
    let a = record_item(sp, "A", vec![], vec![field(sp, "b", b_ty)]);
    let a_ty = ty(sp, "A");
    let b = record_item(sp, "B", vec![], vec![field(sp, "a", a_ty)]);

    let (_, codes) = resolve(&module(vec![a, b]));
    assert!(codes.is_empty(), "mutually recursive types must resolve: {codes:?}");
}

#[test]
fn a_duplicate_definition_is_reported_once_and_points_at_the_first() {
    let sp = &Sp::new();
    let first = func(sp, "f").body(block(sp, vec![], None)).item();
    let first_span = match &first.kind {
        science_parser::ast::ItemKind::Fn(decl) => decl.name.span,
        _ => unreachable!(),
    };
    let second = func(sp, "f").body(block(sp, vec![], None)).item();

    let diagnostics = diagnose(&module(vec![first, second]));

    assert_eq!(codes(&diagnostics), ["SC0201"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains('f'), "{}", d.message);
    assert!(
        d.labels.iter().any(|l| !l.primary && l.span == first_span),
        "the note must point at the first definition"
    );
}

// --- scopes --------------------------------------------------------------

#[test]
fn a_let_is_not_visible_before_its_own_declaration() {
    let sp = &Sp::new();
    // fn f():
    //     let a = b      # `b` is not in scope yet
    //     let b = 1
    let b_use = name(sp, &["b"]);
    let a = let_stmt(sp, false, "a", None, b_use);
    let one = int(sp, 1);
    let b = let_stmt(sp, false, "b", None, one);
    let body = block(sp, vec![a, b], None);
    let f = func(sp, "f").body(body).item();

    let (_, codes) = resolve(&module(vec![f]));
    assert_eq!(codes, ["SC0200"]);
}

#[test]
fn a_let_may_shadow_an_earlier_one_and_its_initializer_sees_the_earlier() {
    let sp = &Sp::new();
    // fn f():
    //     let x = 1
    //     let x = x
    //     x
    let first = let_stmt(sp, false, "x", None, int(sp, 1));
    let second = let_stmt(sp, false, "x", None, name(sp, &["x"]));
    let tail = name(sp, &["x"]);
    let body = block(sp, vec![first, second], Some(tail));
    let f = func(sp, "f").body(body).item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "shadowing is legal: {codes:?}");

    let f = nth_fn(&krate, 0);
    let stmts = &f.body.as_ref().unwrap().stmts;
    let outer = match &stmts[0].kind {
        StmtKind::Let(l) => l.def,
        other => panic!("{other:?}"),
    };
    let (inner, initializer) = match &stmts[1].kind {
        StmtKind::Let(l) => (l.def, path_res(&l.value)),
        other => panic!("{other:?}"),
    };

    assert_ne!(outer, inner, "the second `let` is a new definition");
    assert_eq!(initializer, Res::Def(outer), "its initializer sees the first");
    assert_eq!(path_res(tail_of(f)), Res::Def(inner), "afterwards the second wins");
}

#[test]
fn a_parameter_is_in_scope_in_the_body() {
    let sp = &Sp::new();
    let p = param(sp, "a", ty(sp, "Int"));
    let body = block(sp, vec![], Some(name(sp, &["a"])));
    let f = func(sp, "f").params(vec![p]).body(body).item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "{codes:?}");
    assert_eq!(path_res(tail_of(nth_fn(&krate, 0))), Res::Def(def_of(&krate, DefKind::Param, "a")));
}

#[test]
fn a_generic_parameter_is_in_scope_in_the_signature_of_its_own_function_only() {
    let sp = &Sp::new();
    // fn takes[T](a: T)     # fine
    // fn other(a: T)        # `T` is gone
    let t = generic(sp, "T", vec![]);
    let takes = func(sp, "takes")
        .generics(vec![t])
        .params(vec![param(sp, "a", ty(sp, "T"))])
        .body(block(sp, vec![], None))
        .item();
    let other = func(sp, "other")
        .params(vec![param(sp, "a", ty(sp, "T"))])
        .body(block(sp, vec![], None))
        .item();

    let (krate, codes) = resolve(&module(vec![takes, other]));
    assert_eq!(codes, ["SC0200"], "only the second `T` is unresolved");

    let t_id = def_of(&krate, DefKind::TypeParam, "T");
    let takes = nth_fn(&krate, 0);
    match &takes.params[0].ty.kind {
        TypeKind::Path { res, .. } => assert_eq!(*res, Res::Def(t_id)),
        other => panic!("{other:?}"),
    }
    let other = nth_fn(&krate, 1);
    match &other.params[0].ty.kind {
        TypeKind::Path { res, .. } => assert_eq!(*res, Res::Error),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_unresolved_name_becomes_an_error_node_and_the_walk_continues() {
    let sp = &Sp::new();
    // fn f():
    //     missing_one
    //     missing_two
    let first = expr_stmt(name(sp, &["missing_one"]));
    let second = expr_stmt(name(sp, &["missing_two"]));
    let f = func(sp, "f").body(block(sp, vec![first, second], None)).item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert_eq!(codes, ["SC0200", "SC0200"], "one error must not swallow the next");

    let stmts = &nth_fn(&krate, 0).body.as_ref().unwrap().stmts;
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Expr(e) => assert_eq!(path_res(e), Res::Error),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn self_outside_an_impl_is_reported() {
    let sp = &Sp::new();
    let f = func(sp, "f").body(block(sp, vec![], Some(self_value(sp)))).item();

    let (_, codes) = resolve(&module(vec![f]));
    assert_eq!(codes, ["SC0208"]);
}

// --- the three ambiguities of §4.4 --------------------------------------

/// `enum Option[T]: Some(T) / None`, plus whatever else the test adds.
fn option_choice(sp: &Sp) -> science_parser::ast::Item {
    let t = generic(sp, "T", vec![]);
    let some_payload = ty(sp, "T");
    let variants = vec![variant(sp, "Some", vec![some_payload]), variant(sp, "None", vec![])];
    choice_item(sp, "Choice", vec![t], variants)
}

#[test]
fn a_bare_name_that_is_a_unit_variant_becomes_a_variant_pattern() {
    let sp = &Sp::new();
    let choice = option_choice(sp);
    // fn f(x: Choice): match x: None: 0 / other: 1
    let scrutinee = name(sp, &["x"]);
    let none_arm = (pat_binding(sp, false, "None"), int(sp, 0));
    let other_arm = (pat_binding(sp, false, "other"), int(sp, 1));
    let body = block(sp, vec![], Some(match_expr(sp, scrutinee, vec![none_arm, other_arm])));
    let f = func(sp, "f")
        .params(vec![param(sp, "x", ty(sp, "Choice"))])
        .body(body)
        .item();

    let (krate, codes) = resolve(&module(vec![choice, f]));
    assert!(codes.is_empty(), "{codes:?}");

    let none_id = def_of(&krate, DefKind::Variant, "None");
    let arms = match &tail_of(nth_fn(&krate, 1)).kind {
        ExprKind::Match(m) => &m.arms,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        arms[0].pattern.kind,
        PatternKind::Variant { res: Res::Def(none_id), elems: vec![] },
        "`None` names a unit variant, so it matches rather than binds"
    );
    assert!(
        matches!(arms[1].pattern.kind, PatternKind::Binding { .. }),
        "`other` names nothing, so it stays a binding"
    );
}

#[test]
fn a_bare_name_matching_a_payload_carrying_variant_stays_a_binding() {
    let sp = &Sp::new();
    let choice = option_choice(sp);
    // `Some` carries a payload, so a bare `Some` is a binding, not a match.
    let arm = (pat_binding(sp, false, "Some"), int(sp, 0));
    let scrutinee = name(sp, &["x"]);
    let body = block(sp, vec![], Some(match_expr(sp, scrutinee, vec![arm])));
    let f = func(sp, "f")
        .params(vec![param(sp, "x", ty(sp, "Choice"))])
        .body(body)
        .item();

    let (krate, codes) = resolve(&module(vec![choice, f]));
    assert!(codes.is_empty(), "{codes:?}");

    let arms = match &tail_of(nth_fn(&krate, 1)).kind {
        ExprKind::Match(m) => &m.arms,
        other => panic!("{other:?}"),
    };
    assert!(matches!(arms[0].pattern.kind, PatternKind::Binding { .. }));
}

#[test]
fn an_empty_argument_list_on_a_fieldless_struct_becomes_a_struct_literal() {
    let sp = &Sp::new();
    let empty = record_item(sp, "Empty", vec![], vec![]);
    // fn f(): Empty()
    let callee = name(sp, &["Empty"]);
    let body = block(sp, vec![], Some(call(sp, callee, vec![])));
    let f = func(sp, "f").body(body).item();

    let (krate, codes) = resolve(&module(vec![empty, f]));
    assert!(codes.is_empty(), "{codes:?}");

    let empty_id = def_of(&krate, DefKind::Record, "Empty");
    match &tail_of(nth_fn(&krate, 1)).kind {
        ExprKind::StructLit { res, fields } => {
            assert_eq!(*res, Res::Def(empty_id));
            assert!(fields.is_empty());
        }
        other => panic!("`Empty()` must be reclassified as a struct literal, got {other:?}"),
    }
}

#[test]
fn an_empty_argument_list_on_a_unit_variant_stays_a_variant_call() {
    let sp = &Sp::new();
    let choice = option_choice(sp);
    let callee = name(sp, &["None"]);
    let body = block(sp, vec![], Some(call(sp, callee, vec![])));
    let f = func(sp, "f").body(body).item();

    let (krate, codes) = resolve(&module(vec![choice, f]));
    assert!(codes.is_empty(), "{codes:?}");
    assert!(matches!(tail_of(nth_fn(&krate, 1)).kind, ExprKind::Call { .. }));
}

#[test]
fn positional_arguments_on_a_struct_with_fields_are_a_construction_mismatch() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
    // fn f(): Doc("a")      # positional, but `Doc` is a struct
    let callee = name(sp, &["Doc"]);
    let arg = string(sp, "a");
    let body = block(sp, vec![], Some(call(sp, callee, vec![arg])));
    let f = func(sp, "f").body(body).item();

    let diagnostics = diagnose(&module(vec![doc, f]));
    assert_eq!(codes(&diagnostics), ["SC0206"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains("named arguments"), "{}", d.message);
}

#[test]
fn named_arguments_on_something_that_is_not_a_struct_are_a_construction_mismatch() {
    let sp = &Sp::new();
    let g = func(sp, "g").body(block(sp, vec![], None)).item();
    // fn f(): g(a: 1)
    let lit = int(sp, 1);
    let body = block(sp, vec![], Some(struct_lit(sp, &["g"], vec![("a", lit)])));
    let f = func(sp, "f").body(body).item();

    let diagnostics = diagnose(&module(vec![g, f]));
    assert_eq!(codes(&diagnostics), ["SC0206"]);
}

#[test]
fn a_struct_literal_resolves_its_field_names_and_reports_the_unknown_ones() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
    let good = string(sp, "a");
    let bad = string(sp, "b");
    let lit = struct_lit(sp, &["Doc"], vec![("title", good), ("subtitle", bad)]);
    let f = func(sp, "f").body(block(sp, vec![], Some(lit))).item();

    let (krate, codes) = resolve(&module(vec![doc, f]));
    assert_eq!(codes, ["SC0205"]);

    let title = def_of(&krate, DefKind::Field, "title");
    match &tail_of(nth_fn(&krate, 1)).kind {
        ExprKind::StructLit { fields, .. } => {
            assert_eq!(fields[0].field, Res::Def(title));
            assert_eq!(fields[1].field, Res::Error);
        }
        other => panic!("{other:?}"),
    }
}

// --- modules and `use` ---------------------------------------------------

#[test]
fn use_brings_a_name_into_the_importing_module() {
    let sp = Sp::new();
    let token = record_item(&sp, "Token", vec![], vec![]);
    let parser = module(vec![token]);

    let use_decl = use_item(&sp, &["text", "parser"], Some(&["Token"]));
    let f = func(&sp, "f")
        .params(vec![param(&sp, "t", ty(&sp, "Token"))])
        .body(block(&sp, vec![], None))
        .item();
    let main = module(vec![use_decl, f]);

    let sources = [
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(0),
            path: "main.science".into(),
            ast: main,
        },
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(1),
            path: "text/parser.science".into(),
            ast: parser,
        },
    ];
    let (_, diagnostics) = science_resolve::resolve_crate(&sources);
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

#[test]
fn use_of_a_name_that_does_not_exist_is_reported() {
    let sp = Sp::new();
    let parser = module(vec![record_item(&sp, "Token", vec![], vec![])]);
    let main = module(vec![use_item(&sp, &["text", "parser"], Some(&["Missing"]))]);

    let sources = [
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(0),
            path: "main.science".into(),
            ast: main,
        },
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(1),
            path: "text/parser.science".into(),
            ast: parser,
        },
    ];
    let (_, diagnostics) = science_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["SC0202"]);
}

#[test]
fn use_of_a_module_that_does_not_exist_is_reported() {
    let sp = Sp::new();
    let main = module(vec![use_item(&sp, &["nowhere"], None)]);
    let sources = [science_resolve::SourceModule {
        file: science_diagnostics::FileId(0),
        path: "main.science".into(),
        ast: main,
    }];
    let (_, diagnostics) = science_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["SC0202"]);
}

// --- the orphan rule (§5.4) ---------------------------------------------

/// Three modules: the trait in one, the type in another, the impl in a third.
fn orphan_sources(impl_in_type_module: bool) -> Vec<science_resolve::SourceModule> {
    let sp = Sp::new();
    let traits = module(vec![interface_item(&sp, "Summarize", vec![], vec![])]);
    let doc = record_item(&sp, "Doc", vec![], vec![]);

    let block = impl_item(
        &sp,
        vec![],
        Some(bound_path(&sp, &["shapes", "Summarize"])),
        ty_path(&sp, &["types", "Doc"]),
        vec![],
    );

    let (types_items, main_items) =
        if impl_in_type_module { (vec![doc, block], vec![]) } else { (vec![doc], vec![block]) };

    vec![
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(0),
            path: "main.science".into(),
            ast: module(main_items),
        },
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(1),
            path: "shapes.science".into(),
            ast: traits,
        },
        science_resolve::SourceModule {
            file: science_diagnostics::FileId(2),
            path: "types.science".into(),
            ast: module(types_items),
        },
    ]
}

fn bound_path(sp: &Sp, segments: &[&str]) -> science_parser::ast::TypeBound {
    let path = path(sp, segments);
    let span = path.span;
    science_parser::ast::TypeBound { path, span }
}

#[test]
fn an_impl_in_the_module_of_the_type_is_legal() {
    let sources = orphan_sources(true);
    let (_, diagnostics) = science_resolve::resolve_crate(&sources);
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

#[test]
fn an_impl_owning_neither_the_trait_nor_the_type_is_an_orphan() {
    let sources = orphan_sources(false);
    let (_, diagnostics) = science_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["SC0207"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains("orphan") || d.message.contains("neither"), "{}", d.message);
}

#[test]
fn an_inherent_impl_on_a_local_type_is_legal_and_self_resolves_inside_it() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![]);
    let method = func(sp, "id")
        .receiver(sp, SelfKind::Shared)
        .ret(ty_self(sp))
        .body(block(sp, vec![], Some(self_value(sp))))
        .decl();
    let block = impl_item(sp, vec![], None, ty(sp, "Doc"), vec![method]);

    let (krate, codes) = resolve(&module(vec![doc, block]));
    assert!(codes.is_empty(), "{codes:?}");

    let imp = nth_impl(&krate, 1);
    let method = &imp.methods[0];
    let receiver = method.self_param.as_ref().expect("the receiver is a definition");
    assert_eq!(krate.defs[receiver.def].kind, DefKind::SelfParam);
    assert_eq!(method.ret.as_ref().unwrap().kind, TypeKind::SelfType(Res::SelfTy(imp.def)));
    match &tail_of(method).kind {
        ExprKind::SelfValue(res) => assert_eq!(*res, Res::Def(receiver.def)),
        other => panic!("{other:?}"),
    }
}

// --- closures and `each` (§4.6) -----------------------------------------

#[test]
fn each_names_the_subject_the_implicit_closure_binds() {
    let sp = &Sp::new();
    // function f(docs: Array): docs.map(each)
    let implicit = closure(sp, None, each(sp));
    let mapped = method_call_args(sp, name(sp, &["docs"]), "map", vec![arg(implicit)]);
    let f = func(sp, "f")
        .params(vec![param(sp, "docs", ty(sp, "Array"))])
        .body(block(sp, vec![], Some(mapped)))
        .item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "{codes:?}");

    let args = match &tail_of(nth_fn(&krate, 0)).kind {
        ExprKind::MethodCall { args, .. } => args,
        other => panic!("{other:?}"),
    };
    match &args[0].value.kind {
        ExprKind::Closure { param, body } => {
            assert_eq!(
                krate.defs[*param].name, "each",
                "the implicit form binds a subject the programmer never wrote"
            );
            match &body.kind {
                ExprKind::Each(res) => {
                    assert_eq!(*res, Res::Def(*param), "`each` names that subject")
                }
                other => panic!("{other:?}"),
            }
        }
        other => panic!("the argument must have become a closure, got {other:?}"),
    }
}

#[test]
fn the_named_closure_form_binds_what_was_written() {
    let sp = &Sp::new();
    // function f(docs: Array): docs.map(doc giving doc)
    let named = closure(sp, Some("doc"), name(sp, &["doc"]));
    let mapped = method_call_args(sp, name(sp, &["docs"]), "map", vec![arg(named)]);
    let f = func(sp, "f")
        .params(vec![param(sp, "docs", ty(sp, "Array"))])
        .body(block(sp, vec![], Some(mapped)))
        .item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "{codes:?}");

    let args = match &tail_of(nth_fn(&krate, 0)).kind {
        ExprKind::MethodCall { args, .. } => args,
        other => panic!("{other:?}"),
    };
    match &args[0].value.kind {
        ExprKind::Closure { param, body } => {
            assert_eq!(krate.defs[*param].name, "doc");
            assert_eq!(path_res(body), Res::Def(*param), "the body sees its own parameter");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_closure_parameter_is_gone_once_the_closure_closes() {
    let sp = &Sp::new();
    // function f(docs: Array): docs.map(doc giving doc) ; doc
    let named = closure(sp, Some("doc"), name(sp, &["doc"]));
    let mapped = method_call_args(sp, name(sp, &["docs"]), "map", vec![arg(named)]);
    let f = func(sp, "f")
        .params(vec![param(sp, "docs", ty(sp, "Array"))])
        .body(block(sp, vec![expr_stmt(mapped)], Some(name(sp, &["doc"]))))
        .item();

    let (_, codes) = resolve(&module(vec![f]));
    assert_eq!(codes, ["SC0200"], "`doc` is not in scope after the closure");
}

#[test]
fn an_each_with_no_call_around_it_is_reported() {
    let sp = &Sp::new();
    let f = func(sp, "f").body(block(sp, vec![], Some(each(sp)))).item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["SC0212"]);
}

// --- named arguments -----------------------------------------------------

#[test]
fn the_label_on_a_named_argument_is_not_resolved_as_a_name() {
    let sp = &Sp::new();
    // function f(docs: Array): docs.sort(by: doc giving doc)
    let named = closure(sp, Some("doc"), name(sp, &["doc"]));
    let sorted =
        method_call_args(sp, name(sp, &["docs"]), "sort", vec![named_arg(sp, "by", named)]);
    let f = func(sp, "f")
        .params(vec![param(sp, "docs", ty(sp, "Array"))])
        .body(block(sp, vec![], Some(sorted)))
        .item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "`by` names a parameter, not a value: {codes:?}");

    let args = match &tail_of(nth_fn(&krate, 0)).kind {
        ExprKind::MethodCall { args, .. } => args,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        args[0].name.as_ref().map(|n| n.name.as_str()),
        Some("by"),
        "the label survives for the type checker to match against a parameter"
    );
    assert!(
        !krate.defs.iter().any(|d| d.name == "by"),
        "and it is not a definition of any kind"
    );
}

// --- associated types (§5.4) --------------------------------------------

#[test]
fn self_dot_item_names_the_associated_type_of_the_block_it_is_written_in() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![]);
    let next_sig = func(sp, "next")
        .receiver(sp, SelfKind::Mutable)
        .ret(ty_self_assoc(sp, "Item"))
        .decl();
    let iterate = interface_item_assoc(sp, "Iterate", vec![], &["Item"], vec![next_sig]);
    let next = func(sp, "next")
        .receiver(sp, SelfKind::Mutable)
        .ret(ty_self_assoc(sp, "Item"))
        .body(block(sp, vec![], Some(int(sp, 0))))
        .decl();
    let implementation = impl_item_assoc(
        sp,
        vec![],
        Some(bound(sp, "Iterate")),
        ty(sp, "Doc"),
        vec![("Item", ty(sp, "Int"))],
        vec![next],
    );

    let (krate, codes) = resolve(&module(vec![doc, iterate, implementation]));
    assert!(codes.is_empty(), "{codes:?}");

    // The interface's `Self.Item` is the interface's declaration; the
    // implementation's is the implementation's binding. They are two
    // definitions, and each block sees its own.
    let interface = match &items(&krate)[1].kind {
        hir::ItemKind::Interface(i) => i,
        other => panic!("{other:?}"),
    };
    let declared = interface.assoc_types[0].def;
    assert_eq!(krate.defs[declared].kind, DefKind::AssocType);
    assert!(interface.assoc_types[0].ty.is_none(), "an interface declares without answering");
    match &interface.methods[0].ret.as_ref().unwrap().kind {
        // The name keeps the span it was *written* at, not the one it points
        // to: that is what a diagnostic about this return type has to underline.
        TypeKind::SelfAssoc { res, name } => {
            assert_eq!(*res, Res::Def(declared));
            assert_eq!(name.name, "Item");
            assert_ne!(name.span, krate.defs[declared].span);
        }
        other => panic!("{other:?}"),
    }

    let imp = nth_impl(&krate, 2);
    let bound = imp.assoc_types[0].def;
    assert_ne!(bound, declared, "the binding is a definition of its own");
    assert!(imp.assoc_types[0].ty.is_some(), "an implementation answers it");
    match &imp.methods[0].ret.as_ref().unwrap().kind {
        TypeKind::SelfAssoc { res, .. } => assert_eq!(*res, Res::Def(bound)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_associated_type_is_not_a_bare_name_in_the_module() {
    let sp = &Sp::new();
    let iface = interface_item_assoc(sp, "Iterate", vec![], &["Item"], vec![]);
    // `Item` on its own names nothing: the only way in is `Self.Item`.
    let f = func(sp, "f").params(vec![param(sp, "x", ty(sp, "Item"))]).item();

    let (_, codes) = resolve(&module(vec![iface, f]));
    assert_eq!(codes, ["SC0200"]);
}

// --- const generic parameters (§5.3) ------------------------------------

#[test]
fn a_const_generic_parameter_is_its_own_kind_of_definition() {
    let sp = &Sp::new();
    // function size of (T, const N: Int)() -> Int: N
    let f = func(sp, "size")
        .generics(vec![generic(sp, "T", vec![]), generic_const(sp, "N", ty(sp, "Int"))])
        .ret(ty(sp, "Int"))
        .body(block(sp, vec![], Some(name(sp, &["N"]))))
        .item();

    let (krate, codes) = resolve(&module(vec![f]));
    assert!(codes.is_empty(), "{codes:?}");

    let n = def_of(&krate, DefKind::ConstParam, "N");
    assert_ne!(
        krate.defs[n].kind,
        DefKind::TypeParam,
        "a const parameter stands for a value, not a type"
    );
    assert_eq!(path_res(tail_of(nth_fn(&krate, 0))), Res::Def(n));
    assert_eq!(krate.defs[def_of(&krate, DefKind::TypeParam, "T")].kind, DefKind::TypeParam);
}

/// `const-expression-arithmetic.md` §2.3: the annotation is a **kind**, and
/// the kinds are a closed set.
///
/// The parser is unchanged and accepts whatever `parse_type` reads there, so
/// every one of these gets through it; this phase is the one that says so.
#[test]
fn a_const_parameter_may_only_be_annotated_with_a_kind() {
    let sp = &Sp::new();
    for annotation in [
        ty(sp, "Bool"),
        ty(sp, "String"),
        ty(sp, "F64"),
        // A type that exists is no better than one that does not: `Matrix`
        // would be an unresolved name in a type position, and here it is the
        // kind that is wrong.
        ty(sp, "Matrix"),
        ty_generic(sp, "Array", vec![ty(sp, "Int")]),
        ty_borrowed(sp, false, ty(sp, "Int")),
        ty_path(sp, &["core", "Int"]),
    ] {
        let f = func(sp, "f").generics(vec![generic_const(sp, "N", annotation)]).item();
        let (_, codes) = resolve(&module(vec![f]));
        assert_eq!(codes, ["SC0220"], "one diagnostic, and it is about the kind");
    }
}

/// The two kinds §2.3 admits, and nothing else, resolve cleanly. `Shape` is
/// F1's *layer* but F0's kind, which is the point of item 5: the annotation
/// position is closed from the first commit.
#[test]
fn the_two_kinds_resolve_without_a_diagnostic() {
    let sp = &Sp::new();
    for kind in ["Int", "Shape"] {
        let f = func(sp, "f").generics(vec![generic_const(sp, "N", ty(sp, kind))]).item();
        let (_, codes) = resolve(&module(vec![f]));
        assert!(codes.is_empty(), "`const N: {kind}` is admissible: {codes:?}");
    }
}

/// A bad annotation says one thing once: the name that was written, the kinds
/// that were admissible, and what the position means.
#[test]
fn the_kind_diagnostic_names_the_kinds_a_const_parameter_may_have() {
    let sp = &Sp::new();
    let f = func(sp, "f").generics(vec![generic_const(sp, "ROWS", ty(sp, "Matrix"))]).item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["SC0220"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains("`Int`"), "{}", d.message);
    assert!(d.message.contains("`Shape`"), "{}", d.message);
    assert!(
        d.labels.iter().any(|l| l.message.contains("Matrix")),
        "the label names what was written"
    );
    assert!(
        d.notes.iter().any(|n| n.contains("kind, not with a type")),
        "the note says what the position is"
    );
}

// --- variadic arity (§10.1 item 6) --------------------------------------

/// `broadcasting.md` §11.1: a declaration's arity is a range when a parameter
/// absorbs a run of arguments, and an exact count otherwise. Both answers come
/// out of the resolved parameter list rather than out of `params.len()`.
#[test]
fn a_declarations_arity_is_read_from_its_parameter_kinds() {
    let sp = &Sp::new();
    // type Tensor of (T, const SHAPE: Shape): value: T
    let tensor = record_item(
        sp,
        "Tensor",
        vec![generic(sp, "T", vec![]), generic_const(sp, "SHAPE", ty(sp, "Shape"))],
        vec![field(sp, "value", ty(sp, "T"))],
    );
    // type Pair of (K, V): key: K
    let pair = record_item(
        sp,
        "Pair",
        vec![generic(sp, "K", vec![]), generic(sp, "V", vec![])],
        vec![field(sp, "key", ty(sp, "K"))],
    );

    let (krate, codes) = resolve(&module(vec![tensor, pair]));
    assert!(codes.is_empty(), "{codes:?}");

    let tensor_arity = GenericArity::of(&nth_record(&krate, 0).generics);
    assert!(tensor_arity.variadic);
    assert!(tensor_arity.admits(2), "Tensor of (F32, (n, 768))");
    assert!(tensor_arity.admits(4), "and at rank 3");
    assert!(!tensor_arity.admits(0), "the element type is still required");

    let pair_arity = GenericArity::of(&nth_record(&krate, 1).generics);
    assert!(!pair_arity.variadic);
    assert!(!pair_arity.admits(3), "an ordinary declaration still rejects a third argument");
}

/// Two variadic parameters leave no way to say where the first ends.
#[test]
fn a_second_variadic_parameter_is_reported() {
    let sp = &Sp::new();
    let f = func(sp, "f")
        .generics(vec![
            generic_const(sp, "A", ty(sp, "Shape")),
            generic_const(sp, "B", ty(sp, "Shape")),
        ])
        .item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["SC0221"]);
    let d = diagnostics.iter().next().unwrap();
    assert_eq!(d.labels.len(), 2, "the second one and the first one");
}

// --- reserved words ------------------------------------------------------

#[test]
fn a_reserved_word_used_as_a_definition_name_says_so() {
    let sp = &Sp::new();
    let f = func(sp, "agent").body(block(sp, vec![], None)).item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["SC0209"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains("agent"), "{}", d.message);
    assert!(d.message.contains("reserved"), "{}", d.message);
    assert!(
        d.notes.iter().any(|n| n.contains("later phase")),
        "the note must say it is reserved for a later phase: {:?}",
        d.notes
    );
}

#[test]
fn a_reserved_word_used_as_a_name_says_so_instead_of_cannot_find() {
    let sp = &Sp::new();
    let f = func(sp, "f").body(block(sp, vec![], Some(name(sp, &["tensor"])))).item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["SC0209"], "not SC0200: the word is reserved, not missing");
}

#[test]
fn a_word_reserved_for_f0_itself_is_not_flagged_here() {
    // `mod` is a keyword the lexer already rejects as an identifier, and F0
    // has no `mod` declaration; only the F1-F4 words are this crate's to
    // report. `model` is one of them, `module` is not.
    let sp = &Sp::new();
    let f = func(sp, "module").body(block(sp, vec![], None)).item();
    assert!(diagnose(&module(vec![f])).is_empty());
}
