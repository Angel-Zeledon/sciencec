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
//! different pipeline than the one this crate tests.
//!
//! **The census stopped being empty when [`crate::deref_move`] started
//! checking a fourth thing.** [`the_corpus_census_is_empty`]'s name is now a
//! lie its own body corrects — kept because renaming it would erase the
//! record of what it used to measure. Twenty-three new diagnostics, all
//! `303`, across seven files: twenty genuine — a `String` payload or field
//! read out of a shared borrow's `match`, or out of an `is` comparison's
//! operand, bound where only a borrow was needed — and three false, in
//! `20_extern.science`, where the moved type is an opaque FFI view this
//! compiler cannot yet prove `Copy`.
//!
//! **This census and `crates/sciencec/tests/cli.rs`'s `REGIONS` agree on all
//! seven files, for the reason this file's header gives: a disagreement would
//! mean the driver runs a different pipeline than the one this crate tests.**
//! `19_stdlib.science` and `21_compiler_shapes.science` used to be reached by
//! this harness alone — `TYPE_CHECKER_FINDINGS`' `SC0532`s gated them out of
//! the real driver, which stops at the first phase that reports anything
//! (`Session::build`'s own doc: *"the back end is not reached when the front
//! end reported an error"*) — but `TYPE_CHECKER_FINDINGS` is empty now, so
//! both files resolve and type-check clean and the real binary reaches the
//! region check on them exactly as this harness always did. Twenty-three
//! diagnostics, seven files, one count, two ways of arriving at it.
//!
//! **The census is empty again, this time by the fix rather than by a second
//! measurement agreeing with the first.** `type-checking-and-mir.md`
//! Decision 27 — a non-`Copy` field or match payload read through a borrow
//! types as a borrow — is the type rule `deref_move`'s own module comment
//! named as owed, and [`the_corpus_census_is_empty`]'s own doc is the account
//! of what closed and what changed shape instead of closing.
//!
//! **The two censuses no longer agree on `20_extern.science`, and that is not
//! a disagreement about the pipeline.** This harness still finds nothing
//! there — [`the_corpus_census_is_empty`]'s doc says why the false positive
//! disappears from a *region* census without being fixed — but the type
//! checker now reports `SC0525` three times on the same lines, and
//! `Session::build`'s ordering rule stops the real driver before region
//! inference runs on a body the type checker rejected. So
//! `crates/sciencec/tests/cli.rs`'s `REGIONS` no longer carries an entry for
//! this file at all; `TYPE_CHECKER_FINDINGS` does, and that is where the same
//! three lines are pinned from the driver's side.

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

/// **The measurement, and it is not empty any more.**
///
/// Held as an exact map rather than a count, because a change that swapped one
/// finding for another would leave a count alone — and because that is exactly
/// what happened twice before: three in two files until
/// `00_kitchen_sink.science`'s `SC0334` closed, two in one file until
/// `Map.get` got a declaration, and a count of findings would have hidden
/// which one went each time.
///
/// **The third time is [`crate::deref_move`]** — read that module's own
/// comment for the mechanism — and it is the biggest of the three: twenty
/// `303`s that are genuine, plus three more that are not, across seven files.
/// This file's own header says why that count and
/// `crates/sciencec/tests/cli.rs`'s `REGIONS` agree exactly.
/// **Empty again, and this time by the answer rather than by a declaration
/// landing.** `type-checking-and-mir.md` Decision 27 is the fix `deref_move`'s
/// own module comment named as still owed: a field or a match payload that
/// owns something, read through a borrow, now types as a borrow, so
/// `science-mir`'s lowering never reaches a `Move` whose place has a `Deref`
/// for the twenty genuine sites below to fire on.
///
/// **`20_extern.science`'s three false positives are gone from this census
/// too, and for a reason worth stating rather than assuming.** `a.data` and
/// `b.data` are shared `ffi.Span`s: Decision 27 types them `&Span[F64]`, and a
/// *shared* borrow is `Copy` (`science-mir`'s `is_copy`), so the read is an
/// `Operand::Copy` and never reaches `deref_move` at all — the same
/// conservatism that used to produce `SC0303` now produces nothing here,
/// because there is no `Move` to ask about. `c.data` is the exclusive
/// `ffi.MutableSpan`, and an exclusive borrow is **not** `Copy` — but
/// `science_mir::lower::Builder::read_ergonomic` (the fix Decision 27 needed
/// at the lowering layer, alongside the type) moves a **fresh temporary**
/// holding a real address, not `c.data`'s own place, so the place `deref_move`
/// would have to see a `Deref` in is never the one that moves. **This
/// crate's own harness runs region inference over every body regardless of
/// what the type checker said about it** (this file's own header), so it is
/// this test — not `crates/sciencec/tests/cli.rs`'s `REGIONS`, which the type
/// checker now stops before region inference runs — that is the honest
/// measurement of what `deref_move` itself still has to say about
/// `20_extern.science`: nothing. The type checker's own report on the same
/// three lines is `SC0525`, and `crates/science-types/tests/corpus.rs`'s
/// `REMAINING` is where that is pinned.
#[test]
fn the_corpus_census_is_empty() {
    let found = census();
    let expected = BTreeMap::new();
    assert_eq!(found, expected, "the census moved");
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
/// **Its report filled with four `SC0303`s and is empty again, and the second
/// change is not this test's doing either.** `deref_move` reported four —
/// `LoadError`'s `message` and `explain` each bound a `String` payload out of a
/// `match` over `borrowed self`/`borrowed error` — a real finding pinned and
/// explained where the first one was, `crates/sciencec/tests/cli.rs`'s
/// `REGIONS`. `type-checking-and-mir.md` Decision 27 is what closed it: the
/// payload now types as a borrow, so neither bind ever reaches
/// `science-mir` as a `Move`. What this test still checks is the fact `SC0303`
/// never touched either way: that `describe`'s four calls move the
/// auto-borrow's temporary and not the user's binding.
#[test]
fn the_kitchen_sink_no_longer_moves_a_value_that_is_still_borrowed() {
    let (_, source) = corpus()
        .into_iter()
        .find(|(name, _)| name == "00_kitchen_sink.science")
        .expect("the kitchen sink");
    let checked = check(&source);
    assert_eq!(
        checked.reported(),
        Vec::<u16>::new(),
        "the kitchen sink's census moved — see `crates/sciencec/tests/cli.rs`'s `REGIONS`"
    );

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
                "`describe` moves the binding `{}` again, so the auto-borrow of `&T` \
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
/// **The two `SC0333`s stayed closed; six `SC0303`s opened and are now
/// closed too.** `deref_move` reported six — `ConfigError`'s `message` and
/// `explain` each bind a `String` payload out of a `match` over `borrowed
/// self`/`borrowed error`, three times each — a different check catching a
/// different mistake in the same file, pinned where the first one was in
/// `crates/sciencec/tests/cli.rs`'s `REGIONS`. `type-checking-and-mir.md`
/// Decision 27 closes it the same way it closes `00_kitchen_sink.science`'s
/// four: the payload types as a borrow, so none of the six ever reaches
/// `science-mir` as a `Move`. `lookup`'s summary, which is what this test was
/// written to guard, is unaffected and still asserted below.
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
/// `x` or `y`"* is not an ambiguity. [`science_regions::check`]'s §6 used to
/// be the other, and **is not any more**: four bodies would have reached it
/// when `Array.get` had no declaration, two when `19_stdlib.science`'s `find`
/// still did, one while `07_generics.science`'s `first_inner` was blocked by
/// a lowering hole — and now none.
///
/// Each step down came from a declaration landing, which is the direction
/// §4's *"what would falsify this"* predicts. The last one did not: it came
/// from the hole closing. `first_inner` returns a borrow out of a narrowed
/// `(&T)?`, Decision 19's niched case, which `as_place` reached through the
/// declared type rather than the narrowed one; `science-mir`'s `Narrow` arm
/// now splits the niched case from the tagged one.
///
/// So §6's suppression is still there and is holding nothing up.
/// [`every_undetermined_signature_is_blocked_by_a_missing_declaration`]
/// asserts the set is empty, which is a stronger claim than the one it used
/// to make.
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
    // **Empty, and it was one.** `07_generics.science:first_inner` was the
    // last entry and its lowering hole is closed, so nothing in the corpus
    // now depends on §6's suppression. An entry appearing here again is a
    // lowering that stopped reaching a place, not a declaration that went
    // missing — the two are told apart by the `calls_a_hole` assertion above.
    assert_eq!(
        undetermined,
        Vec::<String>::new(),
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
