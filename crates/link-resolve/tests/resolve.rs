//! What name resolution must do, asserted on the resolved tree rather than on
//! a rendered dump: the snapshots in `snapshots.rs` guard the rendering, these
//! guard the meaning.

mod common;

use common::*;
use link_parser::ast::SelfKind;
use link_resolve::hir::{self, Crate, DefKind, ExprKind, PatternKind, Res, StmtKind, TypeKind};

// --- reaching into the result -------------------------------------------

fn resolve(module: &link_parser::ast::Module) -> (Crate, Vec<String>) {
    let (krate, diagnostics) = link_resolve::resolve_module(FILE, "main.link", module);
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
    let a = struct_item(sp, "A", vec![], vec![field(sp, "b", b_ty)]);
    let a_ty = ty(sp, "A");
    let b = struct_item(sp, "B", vec![], vec![field(sp, "a", a_ty)]);

    let (_, codes) = resolve(&module(vec![a, b]));
    assert!(codes.is_empty(), "mutually recursive types must resolve: {codes:?}");
}

#[test]
fn a_duplicate_definition_is_reported_once_and_points_at_the_first() {
    let sp = &Sp::new();
    let first = func(sp, "f").body(block(sp, vec![], None)).item();
    let first_span = match &first.kind {
        link_parser::ast::ItemKind::Fn(decl) => decl.name.span,
        _ => unreachable!(),
    };
    let second = func(sp, "f").body(block(sp, vec![], None)).item();

    let diagnostics = diagnose(&module(vec![first, second]));

    assert_eq!(codes(&diagnostics), ["LK0201"]);
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
    assert_eq!(codes, ["LK0200"]);
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
    assert_eq!(codes, ["LK0200"], "only the second `T` is unresolved");

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
    assert_eq!(codes, ["LK0200", "LK0200"], "one error must not swallow the next");

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
    assert_eq!(codes, ["LK0208"]);
}

// --- the three ambiguities of §4.4 --------------------------------------

/// `enum Option[T]: Some(T) / None`, plus whatever else the test adds.
fn option_enum(sp: &Sp) -> link_parser::ast::Item {
    let t = generic(sp, "T", vec![]);
    let some_payload = ty(sp, "T");
    let variants = vec![variant(sp, "Some", vec![some_payload]), variant(sp, "None", vec![])];
    enum_item(sp, "Choice", vec![t], variants)
}

#[test]
fn a_bare_name_that_is_a_unit_variant_becomes_a_variant_pattern() {
    let sp = &Sp::new();
    let choice = option_enum(sp);
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
    let choice = option_enum(sp);
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
    let empty = struct_item(sp, "Empty", vec![], vec![]);
    // fn f(): Empty()
    let callee = name(sp, &["Empty"]);
    let body = block(sp, vec![], Some(call(sp, callee, vec![])));
    let f = func(sp, "f").body(body).item();

    let (krate, codes) = resolve(&module(vec![empty, f]));
    assert!(codes.is_empty(), "{codes:?}");

    let empty_id = def_of(&krate, DefKind::Struct, "Empty");
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
    let choice = option_enum(sp);
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
    let doc = struct_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
    // fn f(): Doc("a")      # positional, but `Doc` is a struct
    let callee = name(sp, &["Doc"]);
    let arg = string(sp, "a");
    let body = block(sp, vec![], Some(call(sp, callee, vec![arg])));
    let f = func(sp, "f").body(body).item();

    let diagnostics = diagnose(&module(vec![doc, f]));
    assert_eq!(codes(&diagnostics), ["LK0206"]);
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
    assert_eq!(codes(&diagnostics), ["LK0206"]);
}

#[test]
fn a_struct_literal_resolves_its_field_names_and_reports_the_unknown_ones() {
    let sp = &Sp::new();
    let doc = struct_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
    let good = string(sp, "a");
    let bad = string(sp, "b");
    let lit = struct_lit(sp, &["Doc"], vec![("title", good), ("subtitle", bad)]);
    let f = func(sp, "f").body(block(sp, vec![], Some(lit))).item();

    let (krate, codes) = resolve(&module(vec![doc, f]));
    assert_eq!(codes, ["LK0205"]);

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
    let token = struct_item(&sp, "Token", vec![], vec![]);
    let parser = module(vec![token]);

    let use_decl = use_item(&sp, &["text", "parser"], Some(&["Token"]));
    let f = func(&sp, "f")
        .params(vec![param(&sp, "t", ty(&sp, "Token"))])
        .body(block(&sp, vec![], None))
        .item();
    let main = module(vec![use_decl, f]);

    let sources = [
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(0),
            path: "main.link".into(),
            ast: main,
        },
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(1),
            path: "text/parser.link".into(),
            ast: parser,
        },
    ];
    let (_, diagnostics) = link_resolve::resolve_crate(&sources);
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

#[test]
fn use_of_a_name_that_does_not_exist_is_reported() {
    let sp = Sp::new();
    let parser = module(vec![struct_item(&sp, "Token", vec![], vec![])]);
    let main = module(vec![use_item(&sp, &["text", "parser"], Some(&["Missing"]))]);

    let sources = [
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(0),
            path: "main.link".into(),
            ast: main,
        },
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(1),
            path: "text/parser.link".into(),
            ast: parser,
        },
    ];
    let (_, diagnostics) = link_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["LK0202"]);
}

#[test]
fn use_of_a_module_that_does_not_exist_is_reported() {
    let sp = Sp::new();
    let main = module(vec![use_item(&sp, &["nowhere"], None)]);
    let sources = [link_resolve::SourceModule {
        file: link_diagnostics::FileId(0),
        path: "main.link".into(),
        ast: main,
    }];
    let (_, diagnostics) = link_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["LK0202"]);
}

// --- the orphan rule (§5.4) ---------------------------------------------

/// Three modules: the trait in one, the type in another, the impl in a third.
fn orphan_sources(impl_in_type_module: bool) -> Vec<link_resolve::SourceModule> {
    let sp = Sp::new();
    let traits = module(vec![trait_item(&sp, "Summarize", vec![], vec![])]);
    let doc = struct_item(&sp, "Doc", vec![], vec![]);

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
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(0),
            path: "main.link".into(),
            ast: module(main_items),
        },
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(1),
            path: "shapes.link".into(),
            ast: traits,
        },
        link_resolve::SourceModule {
            file: link_diagnostics::FileId(2),
            path: "types.link".into(),
            ast: module(types_items),
        },
    ]
}

fn bound_path(sp: &Sp, segments: &[&str]) -> link_parser::ast::TypeBound {
    let path = path(sp, segments);
    let span = path.span;
    link_parser::ast::TypeBound { path, span }
}

#[test]
fn an_impl_in_the_module_of_the_type_is_legal() {
    let sources = orphan_sources(true);
    let (_, diagnostics) = link_resolve::resolve_crate(&sources);
    assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
}

#[test]
fn an_impl_owning_neither_the_trait_nor_the_type_is_an_orphan() {
    let sources = orphan_sources(false);
    let (_, diagnostics) = link_resolve::resolve_crate(&sources);
    assert_eq!(codes(&diagnostics), ["LK0207"]);
    let d = diagnostics.iter().next().unwrap();
    assert!(d.message.contains("orphan") || d.message.contains("neither"), "{}", d.message);
}

#[test]
fn an_inherent_impl_on_a_local_type_is_legal_and_self_resolves_inside_it() {
    let sp = &Sp::new();
    let doc = struct_item(sp, "Doc", vec![], vec![]);
    let method = func(sp, "id")
        .receiver(sp, SelfKind::Ref)
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

// --- reserved words ------------------------------------------------------

#[test]
fn a_reserved_word_used_as_a_definition_name_says_so() {
    let sp = &Sp::new();
    let f = func(sp, "agent").body(block(sp, vec![], None)).item();

    let diagnostics = diagnose(&module(vec![f]));
    assert_eq!(codes(&diagnostics), ["LK0209"]);
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
    assert_eq!(codes(&diagnostics), ["LK0209"], "not LK0200: the word is reserved, not missing");
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
