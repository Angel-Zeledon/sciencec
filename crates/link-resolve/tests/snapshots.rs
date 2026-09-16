//! Snapshots of HIR dumps, §10 layer 2 of the design spec.
//!
//! Each snapshot is the resolved tree followed by the diagnostics, so a
//! regression shows up whichever half it lands in: a reference silently
//! pointing at the wrong definition, a span moving to the wrong node, a
//! diagnostic appearing or vanishing.

mod common;

use common::*;
use link_parser::ast::SelfKind;

// --- a program that resolves cleanly ------------------------------------

#[test]
fn a_program_using_most_of_the_language() {
    let sp = &Sp::new();

    // struct Doc: title: String, body: String
    let title_ty = ty(sp, "String");
    let body_ty = ty(sp, "String");
    let doc = struct_item(
        sp,
        "Doc",
        vec![],
        vec![field(sp, "title", title_ty), field(sp, "body", body_ty)],
    );

    // trait Summarize: fn summarize(&self) -> String
    let summarize_sig = func(sp, "summarize").receiver(sp, SelfKind::Ref).ret(ty(sp, "String")).decl();
    let summarize = trait_item(sp, "Summarize", vec![], vec![summarize_sig]);

    // impl Summarize for Doc: fn summarize(&self) -> String: self.body
    let method_body = block(sp, vec![], Some(field_access(sp, self_value(sp), "body")));
    let method = func(sp, "summarize")
        .receiver(sp, SelfKind::Ref)
        .ret(ty(sp, "String"))
        .body(method_body)
        .decl();
    let impl_block = impl_item(sp, vec![], Some(bound(sp, "Summarize")), ty(sp, "Doc"), vec![method]);

    // fn build(title: String) -> Doc: Doc(title: title, body: "")
    let lit = struct_lit(
        sp,
        &["Doc"],
        vec![("title", name(sp, &["title"])), ("body", string(sp, ""))],
    );
    let build = func(sp, "build")
        .params(vec![param(sp, "title", ty(sp, "String"))])
        .ret(ty(sp, "Doc"))
        .body(block(sp, vec![], Some(lit)))
        .item();

    insta::assert_snapshot!(report(&module(vec![doc, summarize, impl_block, build])));
}

// --- forward references --------------------------------------------------

#[test]
fn a_call_to_a_function_declared_further_down() {
    let sp = &Sp::new();
    let helper_call = call(sp, name(sp, &["helper"]), vec![int(sp, 1)]);
    let first = func(sp, "first").body(block(sp, vec![], Some(helper_call))).item();
    let helper = func(sp, "helper")
        .params(vec![param(sp, "n", ty(sp, "Int"))])
        .ret(ty(sp, "Int"))
        .body(block(sp, vec![], Some(name(sp, &["n"]))))
        .item();

    insta::assert_snapshot!(report(&module(vec![first, helper])));
}

#[test]
fn two_structs_naming_each_other() {
    let sp = &Sp::new();
    let view = struct_item(sp, "View", vec![], vec![field(sp, "source", ty(sp, "Doc"))]);
    let doc = struct_item(sp, "Doc", vec![], vec![field(sp, "back", ty(sp, "View"))]);

    insta::assert_snapshot!(report(&module(vec![view, doc])));
}

// --- scopes --------------------------------------------------------------

#[test]
fn shadowing_and_a_let_that_is_not_visible_above_its_own_line() {
    let sp = &Sp::new();
    // fn f(x: Int):
    //     let x = x        # the parameter
    //     let y = z        # `z` does not exist yet
    //     let z = x        # the inner `x`
    //     x
    let shadow = let_stmt(sp, false, "x", None, name(sp, &["x"]));
    let early = let_stmt(sp, false, "y", None, name(sp, &["z"]));
    let late = let_stmt(sp, false, "z", None, name(sp, &["x"]));
    let body = block(sp, vec![shadow, early, late], Some(name(sp, &["x"])));
    let f = func(sp, "f").params(vec![param(sp, "x", ty(sp, "Int"))]).body(body).item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

#[test]
fn a_generic_parameter_reaches_the_signature_and_the_body_but_not_the_next_item() {
    let sp = &Sp::new();
    let t = generic(sp, "T", vec![bound(sp, "Ord")]);
    let inner = ty(sp, "T");
    let largest = func(sp, "largest")
        .generics(vec![t])
        .params(vec![param(sp, "items", ty_generic(sp, "Array", vec![inner]))])
        .ret(ty_ref(sp, false, ty(sp, "T")))
        .body(block(sp, vec![], Some(name(sp, &["items"]))))
        .item();
    // `T` is out of scope here.
    let stray = func(sp, "stray").params(vec![param(sp, "x", ty(sp, "T"))]).item();

    insta::assert_snapshot!(report(&module(vec![largest, stray])));
}

#[test]
fn a_binding_bound_twice_in_one_pattern() {
    let sp = &Sp::new();
    let pattern = pat_variant(
        sp,
        &["Pair"],
        vec![pat_binding(sp, false, "a"), pat_binding(sp, false, "a")],
    );
    let pair = enum_item(
        sp,
        "Two",
        vec![],
        vec![variant(sp, "Pair", vec![ty(sp, "Int"), ty(sp, "Int")])],
    );
    let arm = (pattern, int(sp, 0));
    let f = func(sp, "f")
        .params(vec![param(sp, "t", ty(sp, "Two"))])
        .body(block(sp, vec![], Some(match_expr(sp, name(sp, &["t"]), vec![arm]))))
        .item();

    insta::assert_snapshot!(report(&module(vec![pair, f])));
}

// --- §4.4's three ambiguities -------------------------------------------

#[test]
fn a_bare_name_is_a_variant_when_it_names_one_and_a_binding_otherwise() {
    let sp = &Sp::new();
    // enum Choice: Yes(Int) / No
    let choice = enum_item(
        sp,
        "Choice",
        vec![],
        vec![variant(sp, "Yes", vec![ty(sp, "Int")]), variant(sp, "No", vec![])],
    );
    // match c:
    //     No: 0          # a variant
    //     Yes: 1         # a binding: `Yes` carries a payload
    //     anything: 2    # a binding
    let arms = vec![
        (pat_binding(sp, false, "No"), int(sp, 0)),
        (pat_binding(sp, false, "Yes"), int(sp, 1)),
        (pat_binding(sp, false, "anything"), int(sp, 2)),
    ];
    let f = func(sp, "f")
        .params(vec![param(sp, "c", ty(sp, "Choice"))])
        .body(block(sp, vec![], Some(match_expr(sp, name(sp, &["c"]), arms))))
        .item();

    insta::assert_snapshot!(report(&module(vec![choice, f])));
}

#[test]
fn an_empty_argument_list_against_a_struct_and_against_a_variant() {
    let sp = &Sp::new();
    let empty = struct_item(sp, "Empty", vec![], vec![]);
    let choice = enum_item(sp, "Choice", vec![], vec![variant(sp, "No", vec![])]);
    // fn f(): Empty()      -> a struct literal
    // fn g(): No()         -> a variant call
    let f = func(sp, "f")
        .body(block(sp, vec![], Some(call(sp, name(sp, &["Empty"]), vec![]))))
        .item();
    let g = func(sp, "g")
        .body(block(sp, vec![], Some(call(sp, name(sp, &["No"]), vec![]))))
        .item();

    insta::assert_snapshot!(report(&module(vec![empty, choice, f, g])));
}

#[test]
fn arguments_that_disagree_with_what_the_name_turned_out_to_be() {
    let sp = &Sp::new();
    let doc = struct_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
    let helper = func(sp, "helper").body(block(sp, vec![], None)).item();
    // fn f(): Doc("a")          # positional on a struct
    let f = func(sp, "f")
        .body(block(sp, vec![], Some(call(sp, name(sp, &["Doc"]), vec![string(sp, "a")]))))
        .item();
    // fn g(): helper(title: "a")  # named on a function
    let lit = struct_lit(sp, &["helper"], vec![("title", string(sp, "a"))]);
    let g = func(sp, "g").body(block(sp, vec![], Some(lit))).item();
    // fn h(): Doc(subtitle: "a")  # a field that does not exist
    let unknown = struct_lit(sp, &["Doc"], vec![("subtitle", string(sp, "a"))]);
    let h = func(sp, "h").body(block(sp, vec![], Some(unknown))).item();

    insta::assert_snapshot!(report(&module(vec![doc, helper, f, g, h])));
}

// --- diagnostics ---------------------------------------------------------

#[test]
fn a_duplicate_definition_and_a_duplicate_field() {
    let sp = &Sp::new();
    let first = func(sp, "twice").body(block(sp, vec![], None)).item();
    let second = func(sp, "twice").body(block(sp, vec![], None)).item();
    let doc = struct_item(
        sp,
        "Doc",
        vec![],
        vec![field(sp, "title", ty(sp, "String")), field(sp, "title", ty(sp, "Int"))],
    );

    insta::assert_snapshot!(report(&module(vec![first, second, doc])));
}

#[test]
fn unresolved_names_in_several_positions_at_once() {
    let sp = &Sp::new();
    // fn f(a: Missing) -> AlsoMissing:
    //     nowhere
    let body = block(sp, vec![expr_stmt(name(sp, &["nowhere"]))], None);
    let f = func(sp, "f")
        .params(vec![param(sp, "a", ty(sp, "Missing"))])
        .ret(ty(sp, "AlsoMissing"))
        .body(body)
        .item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

#[test]
fn a_reserved_word_used_as_a_name() {
    let sp = &Sp::new();
    // fn agent(): tool
    let f = func(sp, "agent").body(block(sp, vec![], Some(name(sp, &["tool"])))).item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

#[test]
fn self_where_there_is_no_impl() {
    let sp = &Sp::new();
    let f = func(sp, "f").ret(ty_self(sp)).body(block(sp, vec![], Some(self_value(sp)))).item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

// --- modules, `use`, and the orphan rule --------------------------------

#[test]
fn a_use_that_names_a_module_an_item_and_something_missing() {
    let sp = &Sp::new();
    let token = struct_item(sp, "Token", vec![], vec![]);
    let lex = func(sp, "lex").body(block(sp, vec![], None)).item();
    let parser = module(vec![token, lex]);

    let main = module(vec![
        use_item(sp, &["text", "parser"], Some(&["Token", "missing"])),
        func(sp, "f")
            .params(vec![param(sp, "t", ty(sp, "Token"))])
            .body(block(sp, vec![], Some(call(sp, name(sp, &["text", "parser", "lex"]), vec![]))))
            .item(),
    ]);

    insta::assert_snapshot!(report_crate(vec![
        ("main.link", main),
        ("text/parser.link", parser),
    ]));
}

#[test]
fn a_directory_module_and_its_siblings() {
    let sp = &Sp::new();
    // text/mod.link is the module `text` itself; text/parser.link is
    // `text.parser` under it.
    let text = module(vec![struct_item(sp, "Source", vec![], vec![])]);
    let parser = module(vec![func(sp, "lex").body(block(sp, vec![], None)).item()]);
    let main = module(vec![use_item(sp, &["text"], Some(&["Source"]))]);

    insta::assert_snapshot!(report_crate(vec![
        ("main.link", main),
        ("text/mod.link", text),
        ("text/parser.link", parser),
    ]));
}

#[test]
fn an_orphan_impl_and_a_legal_one() {
    let sp = &Sp::new();
    let summarize = module(vec![trait_item(sp, "Summarize", vec![], vec![])]);
    // types.link owns `Doc`, and implements the foreign trait for it: legal.
    let legal = impl_item(
        sp,
        vec![],
        Some(bound_path(sp, &["shapes", "Summarize"])),
        ty(sp, "Doc"),
        vec![],
    );
    let types = module(vec![struct_item(sp, "Doc", vec![], vec![]), legal]);
    // main.link owns neither: an orphan.
    let orphan = impl_item(
        sp,
        vec![],
        Some(bound_path(sp, &["shapes", "Summarize"])),
        ty_path(sp, &["types", "Doc"]),
        vec![],
    );
    let main = module(vec![orphan]);

    insta::assert_snapshot!(report_crate(vec![
        ("main.link", main),
        ("shapes.link", summarize),
        ("types.link", types),
    ]));
}

fn bound_path(sp: &Sp, segments: &[&str]) -> link_parser::ast::TypeBound {
    let path = path(sp, segments);
    let span = path.span;
    link_parser::ast::TypeBound { path, span }
}
