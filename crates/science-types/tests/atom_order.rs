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
use science_types::{equal, normalise, Atom, AtomOrder, ConstExpr};

/// A ranked atom, where the id doubles as the rank.
///
/// Sound in a test because a test is one file, so allocation order *is* the
/// canonical order. Real code builds an `AtomOrder` from the crate's table;
/// this exists so no test hand-writes a rank, which is how three of them came
/// to say `rank: 0` for every parameter at once and stop distinguishing them.
fn atom(def: DefId) -> Atom {
    Atom::Param { rank: def.index() as u32, def }
}


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
type Window[T, const WIDTH: Int]:
    items: Array[T]

type Grid[T, const ROWS: Int, const COLS: Int]:
    cells: Array[T]

type Cube[T, const DEPTH: Int]:
    slabs: Array[T]
";

/// The same declarations, with the first and the last `type` swapped.
const REORDERED_FIXTURE: &str = "\
type Cube[T, const DEPTH: Int]:
    slabs: Array[T]

type Grid[T, const ROWS: Int, const COLS: Int]:
    cells: Array[T]

type Window[T, const WIDTH: Int]:
    items: Array[T]
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
                ConstExpr::param(atom(depth), span),
                ConstExpr::param(atom(cols), span),
                span,
            ),
            ConstExpr::param(atom(rows), span),
            span,
        ),
        ConstExpr::param(atom(width), span),
        span,
    );

    let form = normalise(&expr).expect("normalises");
    let atoms: Vec<Atom> = form.atoms().collect();
    assert_eq!(
        atoms,
        vec![atom(width), atom(rows), atom(cols), atom(depth)]
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
            ConstExpr::add(ConstExpr::param(atom(width), span), ConstExpr::param(atom(depth), span), span);
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
        ConstExpr::add(ConstExpr::param(atom(first), span), ConstExpr::param(atom(second), span), span);
    let right =
        ConstExpr::add(ConstExpr::param(atom(second), span), ConstExpr::param(atom(first), span), span);

    assert!(equal(&normalise(&left).unwrap(), &normalise(&right).unwrap()));
}

// --- the order is a total order -------------------------------------------

#[test]
fn the_order_is_total_antisymmetric_and_transitive() {
    // `NormalForm`'s invariant is "strictly increasing", and a sort against a
    // comparison that is not a total order does not produce one.
    let defs = resolve(FIXTURE);
    let atoms: Vec<Atom> =
        const_params(&defs).into_iter().map(|(id, _)| atom(id)).collect();

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

/// The bug that made this change necessary, held shut.
///
/// `sciencec`'s driver hands out `FileId`s in command-line order, so the same
/// program compiled with its files named in a different order used to number
/// its definitions differently — and the monomorphisation key serialised the
/// raw number, so a symbol name moved with the invocation. That is a cache
/// miss or a pointer comparison that fails, found long after the cause, and
/// `reproducibility.md` exists to forbid it.
///
/// A rank is §3.1's canonical order — the declaring item's path, then position
/// in its `of` list — so it does not move.
#[test]
fn the_canonical_order_does_not_depend_on_the_order_files_were_read() {
    // Two definitions of the same shape, in tables numbered oppositely. This
    // is what reading the files in the other order produces.
    let mut forwards = DefTable::new();
    let root = forwards.alloc(DefKind::Module, "", science_resolve::hir::BUILTIN_SPAN, None);
    let a1 = forwards.alloc(DefKind::Record, "Alpha", science_resolve::hir::BUILTIN_SPAN, Some(root));
    let pa1 = forwards.alloc(DefKind::ConstParam, "N", science_resolve::hir::BUILTIN_SPAN, Some(a1));
    let b1 = forwards.alloc(DefKind::Record, "Beta", science_resolve::hir::BUILTIN_SPAN, Some(root));
    let pb1 = forwards.alloc(DefKind::ConstParam, "N", science_resolve::hir::BUILTIN_SPAN, Some(b1));

    let mut backwards = DefTable::new();
    let root2 = backwards.alloc(DefKind::Module, "", science_resolve::hir::BUILTIN_SPAN, None);
    let b2 = backwards.alloc(DefKind::Record, "Beta", science_resolve::hir::BUILTIN_SPAN, Some(root2));
    let pb2 = backwards.alloc(DefKind::ConstParam, "N", science_resolve::hir::BUILTIN_SPAN, Some(b2));
    let a2 = backwards.alloc(DefKind::Record, "Alpha", science_resolve::hir::BUILTIN_SPAN, Some(root2));
    let pa2 = backwards.alloc(DefKind::ConstParam, "N", science_resolve::hir::BUILTIN_SPAN, Some(a2));

    // The raw ids disagree, which is the whole problem.
    assert_ne!(pa1.index(), pa2.index(), "the two tables number Alpha's `N` differently");

    // The ranks agree, because `Alpha` sorts before `Beta` whichever was read
    // first, and that is a fact about the names the author wrote.
    let one = AtomOrder::of(&forwards);
    let other = AtomOrder::of(&backwards);
    // The *ranks* agree, because `Alpha` sorts before `Beta` whichever file
    // was read first, and that is a fact about the names the author wrote.
    // The ids beside them differ and are meant to: a rank is the order, an id
    // is the identity, and identity is only comparable within one table.
    assert_eq!(one.atom(pa1).rank(), other.atom(pa2).rank(), "Alpha.N ranks the same");
    assert_eq!(one.atom(pb1).rank(), other.atom(pb2).rank(), "Beta.N ranks the same");
    assert!(one.atom(pa1) < one.atom(pb1), "and `Alpha` sorts before `Beta`");
}
