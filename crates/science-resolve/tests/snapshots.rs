//! Snapshots of HIR dumps, §10 layer 2 of the design spec.
//!
//! Each snapshot is the resolved tree followed by the diagnostics, so a
//! regression shows up whichever half it lands in: a reference silently
//! pointing at the wrong definition, a span moving to the wrong node, a
//! diagnostic appearing or vanishing.

mod common;

use common::*;
use science_parser::ast::SelfKind;

// --- a program that resolves cleanly ------------------------------------

#[test]
fn a_program_using_most_of_the_language() {
    let sp = &Sp::new();

    // struct Doc: title: String, body: String
    let title_ty = ty(sp, "String");
    let body_ty = ty(sp, "String");
    let doc = record_item(
        sp,
        "Doc",
        vec![],
        vec![field(sp, "title", title_ty), field(sp, "body", body_ty)],
    );

    // trait Summarize: fn summarize(&self) -> String
    let summarize_sig = func(sp, "summarize").receiver(sp, SelfKind::Shared).ret(ty(sp, "String")).decl();
    let summarize = interface_item(sp, "Summarize", vec![], vec![summarize_sig]);

    // impl Summarize for Doc: fn summarize(&self) -> String: self.body
    let method_body = block(sp, vec![], Some(field_access(sp, self_value(sp), "body")));
    let method = func(sp, "summarize")
        .receiver(sp, SelfKind::Shared)
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
    let view = record_item(sp, "View", vec![], vec![field(sp, "source", ty(sp, "Doc"))]);
    let doc = record_item(sp, "Doc", vec![], vec![field(sp, "back", ty(sp, "View"))]);

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
        .ret(ty_borrowed(sp, false, ty(sp, "T")))
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
    let pair = choice_item(
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
    let choice = choice_item(
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
    let empty = record_item(sp, "Empty", vec![], vec![]);
    let choice = choice_item(sp, "Choice", vec![], vec![variant(sp, "No", vec![])]);
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
    let doc = record_item(sp, "Doc", vec![], vec![field(sp, "title", ty(sp, "String"))]);
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
    let doc = record_item(
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
    let token = record_item(sp, "Token", vec![], vec![]);
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
        ("main.science", main),
        ("text/parser.science", parser),
    ]));
}

#[test]
fn a_directory_module_and_its_siblings() {
    let sp = &Sp::new();
    // text/mod.science is the module `text` itself; text/parser.science is
    // `text.parser` under it.
    let text = module(vec![record_item(sp, "Source", vec![], vec![])]);
    let parser = module(vec![func(sp, "lex").body(block(sp, vec![], None)).item()]);
    let main = module(vec![use_item(sp, &["text"], Some(&["Source"]))]);

    insta::assert_snapshot!(report_crate(vec![
        ("main.science", main),
        ("text/mod.science", text),
        ("text/parser.science", parser),
    ]));
}

#[test]
fn an_orphan_impl_and_a_legal_one() {
    let sp = &Sp::new();
    let summarize = module(vec![interface_item(sp, "Summarize", vec![], vec![])]);
    // types.science owns `Doc`, and implements the foreign trait for it: legal.
    let legal = impl_item(
        sp,
        vec![],
        Some(bound_path(sp, &["shapes", "Summarize"])),
        ty(sp, "Doc"),
        vec![],
    );
    let types = module(vec![record_item(sp, "Doc", vec![], vec![]), legal]);
    // main.science owns neither: an orphan.
    let orphan = impl_item(
        sp,
        vec![],
        Some(bound_path(sp, &["shapes", "Summarize"])),
        ty_path(sp, &["types", "Doc"]),
        vec![],
    );
    let main = module(vec![orphan]);

    insta::assert_snapshot!(report_crate(vec![
        ("main.science", main),
        ("shapes.science", summarize),
        ("types.science", types),
    ]));
}

// --- closures and `each` (§4.6) -----------------------------------------

#[test]
fn the_two_closure_forms_and_the_subject_each_names() {
    let sp = &Sp::new();
    // function f(docs: Array):
    //     docs.map(each.title)
    //     docs.sort(by: doc giving doc.title)
    let implicit = closure(sp, None, field_access(sp, each(sp), "title"));
    let mapped = method_call_args(sp, name(sp, &["docs"]), "map", vec![arg(implicit)]);

    let named = closure(sp, Some("doc"), field_access(sp, name(sp, &["doc"]), "title"));
    let sorted =
        method_call_args(sp, name(sp, &["docs"]), "sort", vec![named_arg(sp, "by", named)]);

    let f = func(sp, "f")
        .params(vec![param(sp, "docs", ty(sp, "Array"))])
        .body(block(sp, vec![expr_stmt(mapped)], Some(sorted)))
        .item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

#[test]
fn an_each_with_no_call_around_it_has_no_subject() {
    let sp = &Sp::new();
    let f = func(sp, "f").body(block(sp, vec![], Some(each(sp)))).item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

// --- associated types (§5.4) --------------------------------------------

#[test]
fn an_interface_declares_an_associated_type_and_an_implementation_binds_it() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![]);

    // interface Iterate:
    //     type Item
    //     function next(mutable self) -> Self.Item
    let next_sig = func(sp, "next")
        .receiver(sp, SelfKind::Mutable)
        .ret(ty_self_assoc(sp, "Item"))
        .decl();
    let iterate = interface_item_assoc(sp, "Iterate", vec![], &["Item"], vec![next_sig]);

    // Doc implements Iterate:
    //     type Item is Int
    //     function next(mutable self) -> Self.Item: 0
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

    // function show(x: any Iterate)
    let show = func(sp, "show")
        .params(vec![param(sp, "x", ty_any(sp, "Iterate"))])
        .body(block(sp, vec![], None))
        .item();

    insta::assert_snapshot!(report(&module(vec![doc, iterate, implementation, show])));
}

#[test]
fn an_implementation_that_names_an_associated_type_it_never_bound() {
    let sp = &Sp::new();
    let doc = record_item(sp, "Doc", vec![], vec![]);
    let method = func(sp, "get").receiver(sp, SelfKind::Shared).ret(ty_self_assoc(sp, "Item")).decl();
    let implementation = impl_item(sp, vec![], None, ty(sp, "Doc"), vec![method]);

    insta::assert_snapshot!(report(&module(vec![doc, implementation])));
}

// --- const generics, aliases and constants (§4.4, §5.3) -----------------

#[test]
fn a_const_generic_parameter_an_alias_and_a_constant() {
    let sp = &Sp::new();
    // type Window of (T, const N: Int): items: Array of T
    let window = record_item(
        sp,
        "Window",
        vec![generic(sp, "T", vec![]), generic_const(sp, "N", ty(sp, "Int"))],
        vec![field(sp, "items", ty_generic(sp, "Array", vec![ty(sp, "T")]))],
    );
    // type Row is Window of (Int, 4)
    let row = alias_item(
        sp,
        "Row",
        vec![],
        ty_generic(sp, "Window", vec![ty(sp, "Int"), ty_const_int(sp, 4)]),
    );
    // const WIDTH be 768
    let width = const_item(sp, "WIDTH", Some(ty(sp, "Int")), int(sp, 768));
    // function size of (const N: Int)() -> Int: N
    let size = func(sp, "size")
        .generics(vec![generic_const(sp, "N", ty(sp, "Int"))])
        .ret(ty(sp, "Int"))
        .body(block(sp, vec![], Some(name(sp, &["N"]))))
        .item();

    insta::assert_snapshot!(report(&module(vec![window, row, width, size])));
}

/// A const generic argument may be negated, and this phase has nothing to say
/// about it either way.
///
/// `scientific-libraries.md` §12.3's `Velocity`, cut down to the two exponents
/// that matter. A const argument declares no name and mentions none, so the
/// whole of resolution's interest in `const-expression-arithmetic.md` §10.1's
/// commitments 1 and 2 is that the node arrives unchanged and the dump renders
/// its shape — which is what this pins.
#[test]
fn a_negated_const_generic_argument() {
    let sp = &Sp::new();
    // type Quantity of (T, const LENGTH: Int, const TIME: Int): value: T
    let quantity = record_item(
        sp,
        "Quantity",
        vec![
            generic(sp, "T", vec![]),
            generic_const(sp, "LENGTH", ty(sp, "Int")),
            generic_const(sp, "TIME", ty(sp, "Int")),
        ],
        vec![field(sp, "value", ty(sp, "T"))],
    );
    // type Velocity of T is Quantity of (T, 1, -1)
    let velocity = alias_item(
        sp,
        "Velocity",
        vec![generic(sp, "T", vec![])],
        ty_generic(
            sp,
            "Quantity",
            vec![ty(sp, "T"), ty_const_int(sp, 1), ty_const_neg_int(sp, 1)],
        ),
    );

    insta::assert_snapshot!(report(&module(vec![quantity, velocity])));
}

// --- loops, ranges and borrows ------------------------------------------

#[test]
fn a_for_over_a_range_and_a_loop_with_a_break() {
    let sp = &Sp::new();
    // function sum(xs: borrowed Array of Int) -> Int:
    //     let mutable total be 0
    //     borrowed xs
    //     for i in 0..10: total be i
    //     loop: break total
    let total = let_stmt(sp, true, "total", None, int(sp, 0));
    let borrow = expr_stmt(borrowed(sp, false, name(sp, &["xs"])));
    let step = block(sp, vec![assign_stmt(name(sp, &["total"]), name(sp, &["i"]))], None);
    let counted = for_expr(
        sp,
        pat_binding(sp, false, "i"),
        range(sp, int(sp, 0), int(sp, 10), false),
        step,
    );
    let spin = loop_expr(sp, block(sp, vec![break_stmt(sp, Some(name(sp, &["total"])))], None));

    let f = func(sp, "sum")
        .params(vec![param(
            sp,
            "xs",
            ty_borrowed(sp, false, ty_generic(sp, "Array", vec![ty(sp, "Int")])),
        )])
        .ret(ty(sp, "Int"))
        .body(block(sp, vec![total, borrow, expr_stmt(counted)], Some(spin)))
        .item();

    insta::assert_snapshot!(report(&module(vec![f])));
}

fn bound_path(sp: &Sp, segments: &[&str]) -> science_parser::ast::TypeBound {
    let path = path(sp, segments);
    let span = path.span;
    science_parser::ast::TypeBound { path, span }
}

// --- extern blocks -------------------------------------------------------

/// The names an `extern` block declares are ordinary module-level names, and
/// this is the snapshot that says so: the call to `cblas_dgemm` from a
/// function written *above* the block resolves to the block's definition, the
/// `type BlasInt is I32` alias is reached from a parameter of the foreign
/// function itself, and the `static` and the `union` each get a definition of
/// their own kind.
#[test]
fn an_extern_block_declares_ordinary_module_level_names() {
    let sp = &Sp::new();

    // function scale(): unsafe: cblas_dgemm(4)
    //
    // Written before the block, so this is also the forward reference: the
    // block is collected in step 2 like every other item, and nothing about
    // being foreign changes when the name becomes visible.
    let foreign_call = call(sp, name(sp, &["cblas_dgemm"]), vec![int(sp, 4)]);
    let read_global = expr_stmt(name(sp, &["H5T_NATIVE_DOUBLE_g"]));
    let guarded = unsafe_expr(sp, block(sp, vec![read_global], Some(foreign_call)));
    let scale = func(sp, "scale").body(block(sp, vec![], Some(guarded))).item();

    let openblas = extern_item(
        sp,
        "openblas",
        vec![
            extern_alias(sp, "BlasInt", ty(sp, "I32")),
            extern_const(sp, "CBLAS_ROW_MAJOR", false, 101, ty(sp, "BlasInt")),
            extern_const(sp, "H5I_BADID", true, 1, ty(sp, "BlasInt")),
            extern_fn(
                sp,
                "cblas_dgemm",
                vec![param(sp, "m", ty(sp, "BlasInt"))],
                Some(ty(sp, "I32")),
                Some("dgemm_"),
            ),
            extern_static(sp, "H5T_NATIVE_DOUBLE_g", ty(sp, "I64")),
            extern_union(sp, "H5R_ref_t", 64, 8),
        ],
    );

    // A union is a type, so it can be named in a type position like any
    // other; a `static` is not, and naming one in a type position is the type
    // checker's problem, not a resolution one.
    let uses_union = func(sp, "inspect")
        .params(vec![param(sp, "r", ty_borrowed(sp, false, ty(sp, "H5R_ref_t")))])
        .body(block(sp, vec![], None))
        .item();

    insta::assert_snapshot!(report(&module(vec![scale, openblas, uses_union])));
}

/// A name declared in an `extern` block collides with one declared outside it
/// exactly as two ordinary declarations would. Nothing about the block is a
/// namespace.
#[test]
fn an_extern_name_collides_with_an_ordinary_one() {
    let sp = &Sp::new();
    let ordinary = func(sp, "crc32")
        .params(vec![param(sp, "n", ty(sp, "I32"))])
        .body(block(sp, vec![], None))
        .item();
    let foreign = extern_item(
        sp,
        "z",
        vec![extern_fn(sp, "crc32", vec![param(sp, "n", ty(sp, "I32"))], None, None)],
    );

    insta::assert_snapshot!(report(&module(vec![ordinary, foreign])));
}

/// A type an `extern` item names and nothing declares is reported like any
/// other unresolved name. The parser has already refused the Science layouts
/// it can recognise by name; everything else is a name, and this is the phase
/// that looks names up.
#[test]
fn an_extern_item_naming_a_type_that_does_not_exist() {
    let sp = &Sp::new();
    let foreign = extern_item(
        sp,
        "hdf5",
        vec![
            extern_static(sp, "H5T_NATIVE_DOUBLE_g", ty(sp, "Hid")),
            extern_fn(sp, "H5open", vec![], Some(ty(sp, "Herr")), None),
        ],
    );

    insta::assert_snapshot!(report(&module(vec![foreign])));
}

/// §1.7: an extern call site may use named arguments, in any order, and it is
/// the only call site in the language that may.
///
/// The parser cannot tell `cblas_dgemm(layout: ..., m: ...)` from the record
/// construction `Doc(title: ...)` and does not try — §4.4 hands that ambiguity
/// to name resolution. This is where it is settled: a named-argument call on a
/// foreign function comes out a `Call` with its argument names intact, and the
/// same shape on anything else is still `SC0206`.
#[test]
fn named_arguments_are_a_call_at_an_extern_call_site_and_an_error_elsewhere() {
    let sp = &Sp::new();

    let foreign = extern_item(
        sp,
        "openblas",
        vec![extern_fn(
            sp,
            "cblas_dgemm",
            vec![param(sp, "m", ty(sp, "I32")), param(sp, "n", ty(sp, "I32"))],
            None,
            None,
        )],
    );

    // unsafe: cblas_dgemm(n: 2, m: 1) — written in the other order, which is
    // the whole reason the exception exists.
    let named = struct_lit(
        sp,
        &["cblas_dgemm"],
        vec![("n", int(sp, 2)), ("m", int(sp, 1))],
    );
    let caller = func(sp, "call")
        .body(block(sp, vec![], Some(unsafe_expr(sp, block(sp, vec![], Some(named))))))
        .item();

    // The same shape on an ordinary function stays an error.
    let ordinary = func(sp, "plain")
        .params(vec![param(sp, "m", ty(sp, "I32"))])
        .body(block(sp, vec![], None))
        .item();
    let wrong = struct_lit(sp, &["plain"], vec![("m", int(sp, 1))]);
    let bad_caller = func(sp, "misuse").body(block(sp, vec![], Some(wrong))).item();

    insta::assert_snapshot!(report(&module(vec![foreign, caller, ordinary, bad_caller])));
}
