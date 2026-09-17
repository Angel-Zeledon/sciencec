//! Every program in `examples/`, region-checked, and the census of what it
//! found.
//!
//! §14 names the risk and names the evidence that would settle it: *"there is
//! no way to know before there is a checker and a corpus to run it over"*.
//! There is now a checker. The corpus is twenty programs rather than seventeen
//! thousand lines, so what it settles is small — but it is the difference
//! between a claim and a measurement.
//!
//! **`sciencec check` is at zero diagnostics over this directory and this crate
//! is not wired into it.** These three diagnostics are therefore new
//! information, and each one is attributed below to a hole that is not this
//! crate's. Every entry is a test that fails the day its hole closes.

mod support;

use std::collections::BTreeMap;

use science_diagnostics::{FileId, SourceMap};
use support::check;

/// Every `.science` file in `examples/` that resolves on its own, in name
/// order.
///
/// `17_modules.science` names sibling modules and cannot resolve as one file.
/// The harness requires a clean resolution for `science-resolve`'s own reason —
/// a `Res::Error` in a fixture makes every assertion pass for the wrong reason
/// — so it is filtered here rather than asserted about.
fn corpus() -> Vec<(String, String)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("the examples directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "science" {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let source = std::fs::read_to_string(&path).ok()?;
            let file = FileId(0);
            let (tokens, _) = science_lexer::lex(file, &source);
            let (ast, parsed) = science_parser::parse_module(&tokens, file);
            if parsed.has_errors() {
                return None;
            }
            let (_, resolution) = science_resolve::resolve_module(file, &name, &ast);
            if resolution.has_errors() {
                return None;
            }
            Some((name, source))
        })
        .collect();
    out.sort();
    out
}

fn census() -> BTreeMap<String, Vec<u16>> {
    let mut out = BTreeMap::new();
    for (name, source) in corpus() {
        let checked = check(&source);
        let codes = checked.reported();
        if !codes.is_empty() {
            out.insert(name, codes);
        }
    }
    out
}

/// **The measurement.** Three diagnostics, in two files, out of twenty
/// programs `sciencec check` calls clean.
///
/// Held as an exact map rather than a count, because a change that swapped one
/// finding for another would leave a count alone.
#[test]
fn the_corpus_census_is_three_diagnostics_in_two_files() {
    let found = census();
    let expected: BTreeMap<String, Vec<u16>> = [
        ("00_kitchen_sink.science".to_string(), vec![334]),
        ("09_absence_and_failure.science".to_string(), vec![333, 333]),
    ]
    .into_iter()
    .collect();
    assert_eq!(found, expected, "the census moved");
}

/// **Finding one, and it is not a false positive.**
///
/// `00_kitchen_sink.science` line 365 builds an `Excerpt` holding
/// `borrowed doc`, and line 368 writes `describe(doc)`, where `describe` is
/// `def describe of T: Summarize(value: borrowed T)`.
///
/// **That call moves `doc`.** `science-types`'s auto-borrow fires when the
/// parameter is `borrowed any Summarize` — `describe_any(doc)` two lines later
/// lowers to a `Ref` — and does **not** fire when it is `borrowed T` for a
/// generic `T`. `07_generics.science`'s own §11 says *"a parameter declared
/// `borrowed` is borrowed at the call site without the caller writing
/// anything"*, so this is a gap and not a design.
///
/// The consequence is larger than the borrow: lines 369 and 370 go on to use
/// `doc` after it has been moved, which `SC0301` would report if there were a
/// use-after-move check. Region inference is the first phase with anything to
/// say about it.
#[test]
fn the_kitchen_sink_moves_a_value_that_is_still_borrowed() {
    let (_, source) = corpus()
        .into_iter()
        .find(|(name, _)| name == "00_kitchen_sink.science")
        .expect("the kitchen sink");
    let checked = check(&source);
    assert_eq!(checked.reported(), vec![334]);

    // The fact underneath it: the argument is a move and not a borrow.
    let body = checked.body("main");
    let moves = body
        .blocks()
        .filter(|(_, block)| match &block.terminator.kind {
            science_mir::mir::TerminatorKind::Call { callee, args, .. } => {
                matches!(callee, science_mir::mir::Callee::Def(def)
                    if checked.krate.defs.get(*def).name == "describe")
                    && args.iter().any(|arg| arg.moved_place().is_some())
            }
            _ => false,
        })
        .count();
    assert!(
        moves >= 1,
        "`describe(doc)` is now auto-borrowed, so `science-types`'s auto-borrow gap for \
         `borrowed T` has closed and this finding is stale"
    );
}

/// **Finding two, and it *is* a false positive — the measured cost of
/// [`science_regions::generate`]'s §7.**
///
/// `09_absence_and_failure.science`'s `lookup(settings, key)` has the body
/// `settings.get(key)`, whose callee is a `Map` method with no declaration. The
/// rule for a callee with no summary is that it *"may return a reference into
/// every argument it was given"* — so `lookup`'s inferred signature says the
/// result borrows from `settings` **and from `key`**. At the call site the key
/// is the string literal `"host"`, whose temporary dies at the end of the
/// statement, and the result outlives it: `SC0333`.
///
/// The real `Map.get` returns a borrow of the map and not of the key. **There
/// is no way to know that without the declaration**, and the rule errs toward
/// rejecting on purpose — §7 states which way, and this is what it costs: two
/// diagnostics in twenty files, both of them the same shape.
#[test]
fn the_conservative_unresolved_callee_rule_costs_two_false_positives() {
    let (_, source) = corpus()
        .into_iter()
        .find(|(name, _)| name == "09_absence_and_failure.science")
        .expect("the absence example");
    let checked = check(&source);
    assert_eq!(checked.reported(), vec![333, 333]);

    // And the cause, asserted directly: the summary names both parameters.
    let (_, from) = &checked.analysis_of("lookup").summary.returns[0];
    assert_eq!(
        from.iter().map(|it| it.param).collect::<Vec<_>>(),
        vec![0, 1],
        "`lookup` no longer borrows from its key, so `Map.get` has a declaration and this \
         false positive has closed"
    );
}

/// §14's bet, over everything there is to run it over: **`SC0340` fires
/// nowhere.**
///
/// It is quiet for two reasons and only one of them is the language's.
/// [`science_regions`]'s §4 is the real one — the answer is a set, so *"from
/// `x` or `y`"* is not an ambiguity. [`science_regions::check`]'s §6 is the
/// other: four bodies in this corpus would reach it, and in every one the
/// return is unconstrained because `Array.get` and `Iterate.next` have no
/// declaration. **Both halves have to be said**, because the second one is a
/// suppression that must be removed when the containers land.
#[test]
fn sc0340_fires_nowhere_in_the_corpus() {
    for (name, codes) in census() {
        assert!(!codes.contains(&340), "`{name}` needs a signature it cannot state");
    }
}

/// The bodies that would reach `SC0340` if the suppression were removed,
/// counted so that the suppression is a number rather than a claim.
#[test]
fn every_undetermined_signature_is_blocked_by_a_missing_declaration() {
    let mut undetermined = Vec::new();
    for (name, source) in corpus() {
        let checked = check(&source);
        for body in &checked.bodies {
            let analysis = checked.analysis.body(body.def()).expect("analysed");
            if analysis.summary.undetermined().is_empty() {
                continue;
            }
            let who = checked.krate.defs.get(body.def()).name.clone();
            assert!(
                analysis.calls_a_hole,
                "`{who}` in `{name}` is undetermined and calls nothing unresolved, so the \
                 suppression is not what is keeping it quiet"
            );
            undetermined.push(format!("{name}:{who}"));
        }
    }
    undetermined.sort();
    assert_eq!(
        undetermined,
        vec![
            "07_generics.science:first_inner",
            "07_generics.science:largest",
            "10_loops.science:next_line",
            "19_stdlib.science:find",
        ],
        "the set of hole-blocked signatures moved"
    );
}

/// The corpus is analysed rather than skipped: a run that lowered nothing would
/// pass every test above.
#[test]
fn the_corpus_is_actually_analysed() {
    let mut bodies = 0;
    let mut borrows = 0;
    let mut files = 0;
    let mut points = 0;
    for (_, source) in corpus() {
        let checked = check(&source);
        files += 1;
        bodies += checked.analysis.len();
        borrows += checked.bodies.iter().map(|body| body.borrows().len()).sum::<usize>();
        points += checked.bodies.iter().map(|body| body.point_count()).sum::<usize>();
    }
    assert!(files >= 20, "only {files} example files");
    assert!(bodies >= 150, "only {bodies} bodies analysed");
    assert!(borrows >= 40, "only {borrows} borrows in the whole corpus");
    assert!(points >= 2000, "only {points} points in the whole corpus");
}

/// Every diagnostic this crate produces renders, which §7.1 is about and which
/// a `Span` from the wrong file would break.
#[test]
fn every_diagnostic_renders() {
    for (name, source) in corpus() {
        let checked = check(&source);
        if checked.regions.is_empty() {
            continue;
        }
        let mut map = SourceMap::new();
        map.add_file(name.clone(), source.clone());
        let rendered = science_diagnostics::render::render_all(&map, &checked.regions);
        assert!(rendered.contains("-->"), "`{name}`'s diagnostics point at nothing");
        assert!(!rendered.contains("'0"), "a region variable reached a message in `{name}`");
    }
}
