//! The atom order, and whether it is compilation-stable — §10.1 item 3 and
//! `type-checking-and-mir.md` §8 Decision 17.
//!
//! > **Decision 17:** the atom order is by `DefId`, and `DefId`s are assigned
//! > in source order by the resolver, which makes the order stable across runs
//! > without a hash and stable across incremental rebuilds as long as the
//! > declaration order does not change.
//!
//! That decision has two halves and the second one is the one that drifts.
//! *"Sorting by `DefId` is deterministic"* is trivially true and proves
//! nothing; what has to be true is *"`DefId` order is source order"*, and that
//! is a property of the **resolver**, not of this crate. A test that asserted
//! the comparison function against itself would pass forever while the claim
//! it stands for quietly stopped being true.
//!
//! So this file is the one place in the crate that runs the real pipeline —
//! lexer, parser, resolver — and reads the numbering out of the table the
//! resolver built.

use science_diagnostics::FileId;
use science_resolve::hir::{DefId, DefKind, DefTable};
use science_types::{equal, normalise, Atom, ConstExpr};

/// Lexes, parses and resolves one source, and hands back its definitions.
fn resolve(source: &str) -> DefTable {
    let file = FileId(0);
    let (tokens, lex_diagnostics) = science_lexer::lex(file, source);
    assert!(!lex_diagnostics.has_errors(), "the fixture must lex");
    let (ast, parse_diagnostics) = science_parser::parse_module(&tokens, file);
    assert!(!parse_diagnostics.has_errors(), "the fixture must parse");
    let (krate, resolve_diagnostics) = science_resolve::resolve_module(file, "order.science", &ast);
    assert!(!resolve_diagnostics.has_errors(), "the fixture must resolve");
    krate.defs
}

/// Every const generic parameter in the table, in `DefId` order, with its name.
fn const_params(defs: &DefTable) -> Vec<(DefId, String)> {
    defs.iter()
        .filter(|def| def.kind == DefKind::ConstParam)
        .map(|def| (def.id, def.name.clone()))
        .collect()
}

const FIXTURE: &str = "\
type Window of (T, const WIDTH: Int):
    items: Array of T

type Grid of (T, const ROWS: Int, const COLS: Int):
    cells: Array of T

type Cube of (T, const DEPTH: Int):
    slabs: Array of T
";

/// The same declarations, with the first and the last `type` swapped.
const REORDERED_FIXTURE: &str = "\
type Cube of (T, const DEPTH: Int):
    slabs: Array of T

type Grid of (T, const ROWS: Int, const COLS: Int):
    cells: Array of T

type Window of (T, const WIDTH: Int):
    items: Array of T
";

// --- the half that has to be true ----------------------------------------

#[test]
fn def_ids_come_out_in_source_order() {
    // Decision 17's load-bearing half. If the resolver ever allocates a
    // declaration's const parameters out of source order — by walking a hash
    // map, or by deferring generics to a second pass — this fails, and the
    // atom order stops meaning what §3.1 wants it to mean.
    let defs = resolve(FIXTURE);
    let names: Vec<String> = const_params(&defs).into_iter().map(|(_, name)| name).collect();
    assert_eq!(names, ["WIDTH", "ROWS", "COLS", "DEPTH"]);

    let ids: Vec<DefId> = const_params(&defs).into_iter().map(|(id, _)| id).collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "ascending in source order");
}

#[test]
fn resolving_the_same_source_twice_numbers_it_identically() {
    // "Stable across runs without a hash" (Decision 17). Two independent
    // sessions over one text agree on every id, so they agree on the atom
    // order, so they agree on what a diagnostic prints and on what the
    // monomorphisation key serialises to.
    assert_eq!(const_params(&resolve(FIXTURE)), const_params(&resolve(FIXTURE)));
}

#[test]
fn the_atom_order_is_the_source_order_of_the_binders() {
    let defs = resolve(FIXTURE);
    let ids: Vec<DefId> = const_params(&defs).into_iter().map(|(id, _)| id).collect();
    let (width, rows, cols, depth) = (ids[0], ids[1], ids[2], ids[3]);

    // Written in the reverse of source order; normalised into source order.
    let span = defs.get(width).span;
    let expr = ConstExpr::add(
        ConstExpr::add(
            ConstExpr::add(
                ConstExpr::param(depth, span),
                ConstExpr::param(cols, span),
                span,
            ),
            ConstExpr::param(rows, span),
            span,
        ),
        ConstExpr::param(width, span),
        span,
    );

    let form = normalise(&expr).expect("normalises");
    let atoms: Vec<Atom> = form.atoms().collect();
    assert_eq!(
        atoms,
        vec![Atom::Param(width), Atom::Param(rows), Atom::Param(cols), Atom::Param(depth)]
    );
    assert_eq!(form.render(&defs), "WIDTH + ROWS + COLS + DEPTH");
}

// --- the half that costs --------------------------------------------------

#[test]
fn reordering_the_declarations_reorders_what_a_diagnostic_prints() {
    // §3.1 objects to `DefId` on the ground that it makes a diagnostic's
    // rendering depend on something other than the expression. Under Decision
    // 17 that something is **declaration order**, and this is the test that
    // says so out loud rather than leaving it to be discovered.
    //
    // It is the whole cost, and it is bounded: the rendering moves, the
    // *answer* does not.
    let ordered = resolve(FIXTURE);
    let reordered = resolve(REORDERED_FIXTURE);

    let by_name = |defs: &DefTable, wanted: &str| {
        const_params(defs)
            .into_iter()
            .find(|(_, name)| name == wanted)
            .map(|(id, _)| id)
            .expect("the fixture declares it")
    };

    let render = |defs: &DefTable| {
        let (width, depth) = (by_name(defs, "WIDTH"), by_name(defs, "DEPTH"));
        let span = defs.get(width).span;
        let expr =
            ConstExpr::add(ConstExpr::param(width, span), ConstExpr::param(depth, span), span);
        normalise(&expr).expect("normalises").render(defs)
    };

    assert_eq!(render(&ordered), "WIDTH + DEPTH");
    assert_eq!(render(&reordered), "DEPTH + WIDTH");
}

#[test]
fn reordering_the_declarations_changes_no_answer() {
    // The bound on the cost above. `EQUAL` sorts both sides with the same
    // function, so the order cancels: two forms that were equal under one
    // numbering are equal under every other.
    let reordered = resolve(REORDERED_FIXTURE);
    let ids: Vec<DefId> = const_params(&reordered).into_iter().map(|(id, _)| id).collect();
    let (first, second) = (ids[0], ids[1]);
    let span = reordered.get(first).span;

    let left =
        ConstExpr::add(ConstExpr::param(first, span), ConstExpr::param(second, span), span);
    let right =
        ConstExpr::add(ConstExpr::param(second, span), ConstExpr::param(first, span), span);

    assert!(equal(&normalise(&left).unwrap(), &normalise(&right).unwrap()));
}

// --- the order is a total order -------------------------------------------

#[test]
fn the_order_is_total_antisymmetric_and_transitive() {
    // `NormalForm`'s invariant is "strictly increasing", and a sort against a
    // comparison that is not a total order does not produce one.
    let defs = resolve(FIXTURE);
    let atoms: Vec<Atom> =
        const_params(&defs).into_iter().map(|(id, _)| Atom::Param(id)).collect();

    for a in &atoms {
        assert_eq!(a.cmp(a), std::cmp::Ordering::Equal, "reflexive");
        for b in &atoms {
            assert_eq!(a.cmp(b), b.cmp(a).reverse(), "antisymmetric");
            for c in &atoms {
                if a < b && b < c {
                    assert!(a < c, "transitive");
                }
            }
        }
    }
}
