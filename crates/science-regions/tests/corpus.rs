//! Every program in `examples/`, region-checked, and the census of what it
//! found.
//!
//! §14 names the risk and names the evidence that would settle it: *"there is
//! no way to know before there is a checker and a corpus to run it over"*.
//! There is now a checker. The corpus is twenty programs rather than seventeen
//! thousand lines, so what it settles is small — but it is the difference
//! between a claim and a measurement.
//!
//! When this file was written, **`sciencec check` was at zero diagnostics over
//! this directory and this crate was not wired into it**, so its three
//! diagnostics were new information: one real and two false, each attributed
//! below to a hole that is not this crate's, and each a test that fails the day
//! its hole closes.
//!
//! **All three have since closed, and the day each did this test failed, which
//! is the whole point of holding the census as an exact map.** `science-types`
//! now auto-borrows a `borrowed T` parameter, so `00_kitchen_sink.science`'s
//! `SC0334` is gone and [`the_kitchen_sink_no_longer_moves_a_value_that_is_still_borrowed`]
//! is the record of it. The prelude now declares `Map.get`, so
//! `09_absence_and_failure.science`'s two `SC0333`s are gone and
//! [`the_declared_map_get_closed_the_two_false_positives`] is the record of
//! that. **The census is empty.**
//!
//! **An empty census is a weaker test than a non-empty one**, and this file
//! says so where it can be read. Three things keep it from being vacuous:
//! [`the_corpus_is_actually_analysed`] counts bodies, borrows and points;
//! [`sc0340_fires_nowhere_in_the_corpus`] and
//! [`every_undetermined_signature_is_blocked_by_a_missing_declaration`] hold
//! the two remaining suppressions to an exact list; and
//! `crates/sciencec/tests/cli.rs` runs the real binary over the same files.
//!
//! **This crate is also now wired into the driver**, so the census is measured
//! twice: here, over a pipeline this harness builds by hand, and in
//! `crates/sciencec/tests/cli.rs`'s `REGIONS`, by running the binary. The two
//! agree, and they have to, because a disagreement would mean the driver runs a
//! different pipeline than the one this crate tests. `REGIONS` additionally
//! records the verdict — no real findings, two false — in a form a test can
//! check, so that a reader of the compiler's output cannot mistake one kind for
//! the other.

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

/// **The measurement.** Nothing, out of twenty programs every phase above this
/// one calls clean.
///
/// Held as an exact map rather than a count, because a change that swapped one
/// finding for another would leave a count alone — and because that is exactly
/// what happened twice: three in two files until `00_kitchen_sink.science`'s
/// `SC0334` closed, two in one file until `Map.get` got a declaration, and a
/// count of findings would have hidden which one went each time.
#[test]
fn the_corpus_census_is_empty() {
    let found = census();
    assert_eq!(found, BTreeMap::new(), "the census moved");
}

/// **Finding one, closed.** It was the only real one, and this test is what it
/// left behind.
///
/// **What it was.** `00_kitchen_sink.science` line 365 builds an `Excerpt`
/// holding `borrowed doc`, and line 368 writes `describe(doc)`, where
/// `describe` is `def describe of T: Summarize(value: borrowed T)`. **That call
/// moved `doc`**: `science-types`'s auto-borrow fired when the parameter was
/// `borrowed any Summarize` — `describe_any(doc)` two lines later lowered to a
/// `Ref` — and did **not** fire when it was `borrowed T` for a generic `T`.
/// `07_generics.science`'s own §11 says *"a parameter declared `borrowed` is
/// borrowed at the call site without the caller writing anything"*, so it was a
/// gap and not a design. The consequence was larger than the borrow: lines 369
/// and 370 go on to use `doc` after it had been moved.
///
/// **What closed it.** `science-types` now auto-borrows a `borrowed T`
/// parameter. The old test asserted `SC0334` here and its failure message said,
/// in as many words, what a failure would mean. It failed, and this is the
/// record.
///
/// **Why the assertion is not simply deleted, and why it is not the one the old
/// test used.** The old control was *"some argument of a `describe` call is
/// moved"*, and **that is still true and always was**: after the fix the call
/// moves the *reference temporary* the auto-borrow created, so
/// `Operand::moved_place` is `Some` either way and the control could not tell
/// the fix from a regression. What distinguishes them is *which place* is
/// moved, so that is what is asserted: every `describe` call in `main` now
/// moves a [`science_mir::mir::LocalKind::Temp`], where before it moved the
/// user's binding — `doc`, `excerpt`, `loaded`, `opened`. A borrow check that
/// stopped working could not produce that; only the auto-borrow can.
#[test]
fn the_kitchen_sink_no_longer_moves_a_value_that_is_still_borrowed() {
    let (_, source) = corpus()
        .into_iter()
        .find(|(name, _)| name == "00_kitchen_sink.science")
        .expect("the kitchen sink");
    let checked = check(&source);
    assert!(checked.reported().is_empty(), "the kitchen sink reports {:?}", checked.reported());

    let body = checked.body("main");
    let mut calls = 0;
    for (_, block) in body.blocks() {
        let science_mir::mir::TerminatorKind::Call { callee, args, .. } = &block.terminator.kind
        else {
            continue;
        };
        if !matches!(callee, science_mir::mir::Callee::Def(def)
            if checked.krate.defs.get(*def).name == "describe")
        {
            continue;
        }
        calls += 1;
        for arg in args.iter() {
            let Some(place) = arg.moved_place() else { continue };
            let decl = body.local_decl(place.local);
            let named = decl.def().map(|def| checked.krate.defs.get(def).name.clone());
            assert_eq!(
                named, None,
                "`describe` moves the binding `{}` again, so the auto-borrow of `borrowed T` \
                 has regressed and `SC0334` is about to come back",
                named.clone().unwrap_or_default()
            );
        }
    }
    assert_eq!(calls, 4, "`main` no longer makes the four `describe` calls this is about");
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
/// The real `Map.get` returns a borrow of the map and not of the key, and
/// **`science-resolve`'s `builtins.rs` now says so**:
///
/// ```text
/// Map of (K, V) has:
///     def get(self, key: borrowed K) -> (borrowed V)?
/// ```
///
/// `science-types`' `Declarations::borrow_sources` reads that signature and
/// answers *"parameter 0"* — the referent mentions `V`, `Self` mentions `V`,
/// `borrowed K` does not — and [`summary`]'s §4 is what this crate does with
/// the answer. The general rule is unchanged and still errs toward rejecting
/// (§7); what changed is that `Map.get` is no longer a callee nothing is known
/// about.
///
/// [`summary`]: science_regions::summary
#[test]
fn the_declared_map_get_closed_the_two_false_positives() {
    let (_, source) = corpus()
        .into_iter()
        .find(|(name, _)| name == "09_absence_and_failure.science")
        .expect("the absence example");
    let checked = check(&source);
    assert_eq!(checked.reported(), Vec::<u16>::new());

    // And the cause, asserted directly: the summary names the map alone.
    let (_, from) = &checked.analysis_of("lookup").summary.returns[0];
    assert_eq!(
        from.iter().map(|it| it.param).collect::<Vec<_>>(),
        vec![0],
        "`lookup` borrows from its key again, so the declaration stopped being read"
    );
}

/// §14's bet, over everything there is to run it over: **`SC0340` fires
/// nowhere.**
///
/// It is quiet for two reasons and only one of them is the language's.
/// [`science_regions`]'s §4 is the real one — the answer is a set, so *"from
/// `x` or `y`"* is not an ambiguity. [`science_regions::check`]'s §6 is the
/// other: **one** body in this corpus would reach it, down from four when
/// `Array.get` had no declaration and from two when `19_stdlib.science`'s
/// `find` still did. Each step down came from a declaration landing, which is
/// the direction §4's *"what would falsify this"* predicts; the one that
/// remains is blocked by a lowering hole rather than by a missing declaration,
/// and [`every_undetermined_signature_is_blocked_by_a_missing_declaration`]
/// names it.
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
        vec!["07_generics.science:first_inner"],
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
