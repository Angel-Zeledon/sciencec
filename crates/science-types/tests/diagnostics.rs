//! `SC0261` and §9.2's normal-form-and-legend block — §10.1 item 8.
//!
//! > **§10.1 item 8.** A diagnostic added after the checker exists is a
//! > diagnostic that renders whatever the checker happens to have kept.
//!
//! So these tests render the real thing through `science-diagnostics` against
//! a real source, rather than asserting on the pieces. Four things §9.2 says
//! the block must do, and one it says it must not:
//!
//! - print **both normal forms**, so a user who wrote `k + 1` and `1 + k` can
//!   see at a glance that those are not the problem;
//! - print **where each symbol was bound**, which for a `with`-bound parameter
//!   is the only way to find out;
//! - say **why** they are not equal, in words;
//! - be available **without** the `SC0261` wrapper, so that `SC0294` and
//!   `SC0256` can carry it instead of a second error being pushed
//!   (Decision 9.1);
//! - never point a label at a definition with no readable text.

use science_diagnostics::{render, Diagnostic, FileId, SourceMap, Span};
use science_resolve::hir::{DefId, DefKind, DefTable, BUILTIN_SPAN};
use science_types::diagnostics::{cannot_show_equal, normal_form_block, Comparand};
use science_types::{Atom, codes, normalise, ConstExpr, NormalForm};

/// A ranked atom, where the id doubles as the rank.
///
/// Sound in a test because a test is one file, so allocation order *is* the
/// canonical order. Real code builds an `AtomOrder` from the crate's table;
/// this exists so no test hand-writes a rank, which is how three of them came
/// to say `rank: 0` for every parameter at once and stop distinguishing them.
fn atom(def: DefId) -> Atom {
    Atom::Param { rank: def.index() as u32, def }
}


const SOURCE: &str = "\
def residual of (const n: Int, const m: Int)(
    block: Grid of (F32, n),
    reference: Grid of (F32, m),
) -> Grid of (F32, n):
    block.minus(reference, 1)
";

/// The span of the `nth` occurrence of `needle`, counting from zero.
fn at(needle: &str, nth: usize) -> Span {
    let mut from = 0;
    for _ in 0..nth {
        from = SOURCE[from..].find(needle).expect("occurrence exists") + from + needle.len();
    }
    let start = SOURCE[from..].find(needle).expect("occurrence exists") + from;
    Span::new(FileId(0), start as u32, (start + needle.len()) as u32)
}

fn source_map() -> SourceMap {
    let mut map = SourceMap::new();
    map.add_file("pipeline/residual.science".to_string(), SOURCE.to_string());
    map
}

/// The two const parameters the fixture declares, with their binder spans.
fn fixture() -> (DefTable, DefId, DefId) {
    let mut defs = DefTable::new();
    let root = defs.alloc(DefKind::Module, "", Span::at(FileId(0), 0), None);
    let function = defs.alloc(DefKind::Fn, "residual", at("residual", 0), Some(root));
    // Allocated in source order, which is what the atom order depends on.
    let n = defs.alloc(DefKind::ConstParam, "n", at("n: Int", 0), Some(function));
    let m = defs.alloc(DefKind::ConstParam, "m", at("m: Int", 0), Some(function));
    (defs, n, m)
}

fn form(expr: &ConstExpr) -> NormalForm {
    normalise(expr).expect("normalises")
}

fn rendered(diagnostic: &Diagnostic) -> String {
    render(&source_map(), diagnostic)
}

// --- the whole diagnostic, rendered ---------------------------------------

#[test]
fn two_distinct_parameters_render_the_full_block() {
    let (defs, n, m) = fixture();

    // The two extents, as they appear in the two parameter types.
    let left_expr = ConstExpr::param(atom(n), at("n),", 0));
    let right_expr = ConstExpr::param(atom(m), at("m),", 0));
    let (left_form, right_form) = (form(&left_expr), form(&right_expr));

    let diagnostic = cannot_show_equal(
        at("reference", 1),
        &Comparand { span: left_expr.span, written: &left_expr, form: &left_form },
        &Comparand { span: right_expr.span, written: &right_expr, form: &right_form },
        &defs,
    );

    assert_eq!(diagnostic.code, codes::NOT_PROVABLY_EQUAL);
    let text = rendered(&diagnostic);

    // Printed for the record: this is what §9.2's block actually renders to.
    println!("{text}");

    assert!(text.starts_with("error[SC0261]: these const expressions cannot be shown equal"));
    assert!(text.contains("the compiler compared these two const expressions:"));
    assert!(text.contains("left    n   normalised   n"));
    assert!(text.contains("right   m   normalised   m"));
    assert!(text.contains(
        "`n` and `m` are distinct const parameters, so no assignment of values"
    ));
    assert!(text.contains("makes them equal for every instantiation"));
    assert!(text.contains("`n` bound here"));
    assert!(text.contains("`m` bound here"));
}

#[test]
fn the_normal_form_column_shows_that_reassociation_is_not_the_problem() {
    // §9.2: "it shows the normal forms, so a user who wrote `k + 1` and
    // `1 + k` can see at a glance that those are not the problem."
    let (defs, n, m) = fixture();

    let left_expr = ConstExpr::add(
        ConstExpr::param(atom(n), at("n),", 0)),
        ConstExpr::lit(1, at("1", 0)),
        at("n),", 0),
    );
    let right_expr = ConstExpr::add(
        ConstExpr::lit(1, at("1", 0)),
        ConstExpr::param(atom(m), at("m),", 0)),
        at("m),", 0),
    );
    let (left_form, right_form) = (form(&left_expr), form(&right_expr));

    let block = normal_form_block(
        &Comparand { span: left_expr.span, written: &left_expr, form: &left_form },
        &Comparand { span: right_expr.span, written: &right_expr, form: &right_form },
        &defs,
    );

    // Written one way round, normalised the other: the constant leads, so the
    // two rows line up and the difference is visibly `n` against `m`.
    assert!(block.note.contains("left    n + 1   normalised   1 + n"), "{}", block.note);
    assert!(block.note.contains("right   1 + m   normalised   1 + m"), "{}", block.note);
}

// --- the explanation, case by case ----------------------------------------

fn explanation(left: &ConstExpr, right: &ConstExpr, defs: &DefTable) -> String {
    let (left_form, right_form) = (form(left), form(right));
    normal_form_block(
        &Comparand { span: left.span, written: left, form: &left_form },
        &Comparand { span: right.span, written: right, form: &right_form },
        defs,
    )
    .note
}

#[test]
fn a_differing_constant_is_named_as_such() {
    let (defs, n, _) = fixture();
    let span = at("n),", 0);
    let left = ConstExpr::add(ConstExpr::param(atom(n), span), ConstExpr::lit(1, span), span);
    let right = ConstExpr::add(ConstExpr::param(atom(n), span), ConstExpr::lit(2, span), span);

    assert!(explanation(&left, &right, &defs)
        .contains("the constant terms differ: 1 on the left, 2 on the right"));
}

#[test]
fn a_differing_coefficient_is_named_as_such() {
    let (defs, n, _) = fixture();
    let span = at("n),", 0);
    let left = ConstExpr::scale(ConstExpr::param(atom(n), span), 2, span, span);
    let right = ConstExpr::scale(ConstExpr::param(atom(n), span), 3, span, span);

    assert!(explanation(&left, &right, &defs)
        .contains("`n` has coefficient 2 on the left and 3 on the right"));
}

#[test]
fn an_atom_present_on_one_side_only_is_named_as_such() {
    let (defs, n, m) = fixture();
    let span = at("n),", 0);
    let left = ConstExpr::add(ConstExpr::param(atom(n), span), ConstExpr::param(atom(m), span), span);
    let right = ConstExpr::param(atom(n), span);

    assert!(explanation(&left, &right, &defs)
        .contains("`m` appears on the left and not on the right"));
}

// --- the legend -----------------------------------------------------------

#[test]
fn the_legend_has_one_entry_per_atom_in_atom_order_without_duplicates() {
    let (defs, n, m) = fixture();
    let span = at("n),", 0);
    // `n` twice on the left, `m` once on the right.
    let left = ConstExpr::add(ConstExpr::param(atom(n), span), ConstExpr::param(atom(n), span), span);
    let right = ConstExpr::add(ConstExpr::param(atom(n), span), ConstExpr::param(atom(m), span), span);
    let (left_form, right_form) = (form(&left), form(&right));

    let block = normal_form_block(
        &Comparand { span: left.span, written: &left, form: &left_form },
        &Comparand { span: right.span, written: &right, form: &right_form },
        &defs,
    );

    let messages: Vec<&str> = block.legend.iter().map(|label| label.message.as_str()).collect();
    assert_eq!(messages, ["`n` bound here", "`m` bound here"]);
    assert!(block.legend.iter().all(|label| !label.primary), "the legend is context");
    assert_eq!(block.legend[0].span, defs.get(n).span);
    assert!(!block.elided_builtins);
}

#[test]
fn a_builtin_definition_never_becomes_a_label() {
    // `hir` is explicit that a diagnostic must never use `BUILTIN_SPAN` as a
    // primary label, and it points at no readable text at all, so the legend
    // omits it and says so instead of pointing at nothing.
    let (mut defs, n, _) = fixture();
    let builtin = defs.alloc(DefKind::ConstParam, "LANES", BUILTIN_SPAN, None);

    let span = at("n),", 0);
    let left = ConstExpr::param(atom(n), span);
    let right = ConstExpr::param(atom(builtin), span);
    let (left_form, right_form) = (form(&left), form(&right));

    let block = normal_form_block(
        &Comparand { span: left.span, written: &left, form: &left_form },
        &Comparand { span: right.span, written: &right, form: &right_form },
        &defs,
    );

    assert!(block.elided_builtins);
    assert_eq!(block.legend.len(), 1);
    assert_eq!(block.legend[0].message, "`n` bound here");
}

// --- Decision 9.1 ---------------------------------------------------------

#[test]
fn the_block_can_be_attached_to_someone_elses_diagnostic() {
    // §9.1: when a shape comparison fails, `broadcasting.md`'s `SC0294` is
    // reported and carries this block as a note. One code per user-visible
    // mistake; the const layer contributes the explanation, not a second
    // error. This is that call, standing in for the caller that does not
    // exist yet.
    let (defs, n, m) = fixture();
    let span = at("n),", 0);
    let left = ConstExpr::param(atom(n), span);
    let right = ConstExpr::param(atom(m), at("m),", 0));
    let (left_form, right_form) = (form(&left), form(&right));

    let block = normal_form_block(
        &Comparand { span: left.span, written: &left, form: &left_form },
        &Comparand { span: right.span, written: &right, form: &right_form },
        &defs,
    );

    let mut borrowed = Diagnostic::error(
        science_diagnostics::Code(294),
        "these dimensions cannot be shown equal",
    )
    .with_note(block.note);
    for label in block.legend {
        borrowed = borrowed.with_label(label);
    }

    let text = rendered(&borrowed);
    assert!(text.starts_with("error[SC0294]:"));
    assert!(text.contains("the compiler compared these two const expressions:"));
    assert!(text.contains("`n` bound here"));
}
