//! §3 step 2: the forward walk that emits the constraints.
//!
//! > **Constraint generation.** A forward walk emitting `'a: 'b @ p` for every
//! > assignment, call, return and coercion.
//!
//! # 1. The one rule, applied six ways
//!
//! Everything below is the same rule: **a reference may not be stored anywhere
//! that outlives what it points at.** Written as a constraint, `dest ⊆ src`,
//! which Decision 1 spells `'src: 'dest`.
//!
//! What changes between an assignment, a record construction and a call is only
//! *which position of the destination* a source reaches. A field store reaches
//! `[Field(f)] ++ p`; a tuple store reaches `[Tuple(i)] ++ p`; a plain
//! assignment reaches `p`. [`relate`] takes the prefix and is the only place a
//! constraint is created.
//!
//! # 2. What a `Ref` emits, and the half that is not obvious
//!
//! `x be borrowed p` emits two things:
//!
//! 1. `'loan: 'x` — the loan must cover wherever the reference is live. This is
//!    the constraint that makes a region *grow*, and without it every loan's
//!    region would be the single point it was taken at.
//! 2. **For every reference `p` is reached *through*, that reference's region
//!    outlives the loan.** `borrowed (*r).field` cannot outlive `r`.
//!    [`Cause::ReborrowedFrom`] is that constraint and [`crate::check`]'s §4 is
//!    why rule 5 is checked here rather than against a storage-dead point: the
//!    storage that dies is `r`'s, and `r` is not what the loan points at.
//!
//! # 3. Decision 3's constraint, and the case the claim turns on
//!
//! `Node(defs: a, value: b)` emits `'a: 'node.defs` and `'b: 'node.value` — two
//! constraints, two destination positions, **no constraint between them**.
//! That is the whole of Decision 3 in one statement: the type declaration said
//! only *that* the fields are borrowed, and this construction said where each
//! one came from. A second construction elsewhere says something else, and
//! nothing ever unifies the two.
//!
//! What §4.3 calls *"the plan's own region is the intersection"* is not emitted
//! as a constraint, because it is not one: it is a *reading* of the solution,
//! and [`crate::check`]'s §7 computes it and reports what it found.
//!
//! # 4. Parameters hold everywhere, and that is not a shortcut
//!
//! A parameter's region belongs to the caller. Inside this body there is no
//! point at which it ends, so it is seeded with every point
//! ([`crate::solve`]'s §3) and a constraint `'param: 'anything` is satisfied
//! for free.
//!
//! **This is what makes Decision 5 work**, and it is worth saying why rather
//! than letting it look like an approximation. The analysis is not asking *how
//! long* a parameter lives — that is the caller's question and Decision 7's
//! summary is how the caller is told. It is asking *which* parameters the
//! return is constrained by, and that is a question about the shape of the
//! constraint graph, which the seeding does not disturb.
//!
//! # 5. An unresolved callee: the decision
//!
//! `science-mir`'s §5 puts this squarely here: *"decide what your engine does
//! about an unresolved callee and say so; silently assuming the safest thing is
//! acceptable only if you state which way 'safe' points"*. Seven of the
//! acceptance case's calls have no callee, down from sixteen, and **three of
//! the seven are its three `for` loops** — one element-producing call each,
//! [`science_mir::mir::Unresolved::IterateNext`], waiting on a THIR field
//! rather than on a declaration (`science-mir`'s `lower` §7.2).
//!
//! The rule below is therefore what stands between a loop's element and its
//! source: the element comes back from an opaque callee whose one argument is
//! the loop's shared reference, so §5 ties the element's region to that loan
//! and nothing else does. `science-mir`'s `lower` §7.1 is what made that
//! argument a reference; before it, the argument was a temporary the subject
//! had been *moved* into, and the rule tied every element to a temporary that
//! dies with the body.
//!
//! > **Decision. A callee with no summary — unresolved, cross-crate, builtin,
//! > or called indirectly — is assumed to return a reference into every
//! > argument it was given, and to store every argument into every argument it
//! > could write through.**
//!
//! *"Every argument it was given"* is read through references as well as at
//! them: an argument spelled `(*self).defs` is reached through `self`, so the
//! reference handed back may point into `*self` and is constrained by `self`'s
//! region. [`regions::deref_prefixes`] is that reading and it is the load-
//! bearing half — without it, `DefTable.get`'s `self.defs.get(id.index)` would
//! return a reference constrained by nothing, and the whole file would fail
//! Decision 6.
//!
//! # 6. The receiver borrow `science-mir` does not reborrow
//!
//! **A finding about the lowering, worked around here rather than fixed
//! there.** Inside a method whose own receiver is already a reference,
//! `self.other()` lowers to `_4 = borrowed _1` — a borrow of the *local that
//! holds the reference* — and not to `_4 = borrowed (*_1)`, a reborrow of what
//! it points at. The type that results is `borrowed (mutable borrowed Table)`
//! where the callee's parameter is `borrowed Table`.
//!
//! Read literally that loan points at `_1`'s own storage, which ends at the
//! function's exit, so **every method that returns a borrow derived from
//! calling another method on `self` would be `SC0333`** — a false positive on
//! a signature that is the most ordinary thing in a compiler.
//! `examples/21_compiler_shapes.science` misses it by one step: `parent_of`
//! calls `self.get(id)` and returns `DefId?`, so the loan dies before the
//! exit.
//!
//! **The workaround** is [`through_references`]: a borrow of a whole local
//! whose type is itself a reference is treated as a reborrow *through* that
//! reference — the loan is constrained by the reference's region, and rule 5 is
//! not checked against the reference's storage
//! ([`crate::check`]'s §4). That is what the author wrote (`self` means the
//! referent; Science has no way to say otherwise, `AGENTS.md` §1) and it loses
//! nothing: the inner loan the reference came from is still checked, so
//! returning a borrow of a genuine local is still `SC0333`.
//!
//! The fix belongs in `science-mir`'s `lower_method_call`, which should
//! auto-deref a reference-typed receiver before borrowing it.
//!
//! # 7. Which way "safe" points, and what it costs
//!
//! `science-mir`'s §5 asks for this in as many words: *"silently assuming the
//! safest thing is acceptable only if you state which way 'safe' points"*.
//!
//! **It points toward *rejecting*.** A region that is constrained by
//! more sources is not larger — it is bounded above by more things — so the
//! loans that flow into it stay alive longer and conflict with more accesses.
//! A program this rule refuses may be fine; a program it accepts is not made
//! unsafe by the rule. The one direction it does *not* cover is the argument a
//! callee **moves**: `science-mir`'s `lower` §5 marks every argument of an
//! unresolved call `Copy` on purpose, so a move through a method call is
//! invisible here and `SC0334` cannot see it. That hole is the lowering's and
//! it is stated at [`science_mir::mir::Unresolved`].
//!
//! **What it costs, measured.** Two false positives in the twenty programs of
//! `examples/`, both the same shape and both in
//! `09_absence_and_failure.science`: a `lookup(settings, key)` whose body is
//! `settings.get(key)` is inferred to return a borrow of the **key** as well as
//! of the map, so passing a string literal as the key makes the result outlive
//! the literal's temporary. The real `Map.get` borrows only the map, and there
//! is no way to know that without the declaration. `tests/corpus.rs` holds the
//! census.
//!
//! # 8. A closure's captures, which used to be the hole
//!
//! `science-mir`'s `lower` §8 now makes every capture a borrow, so a closure
//! value reaches this walk as an aggregate of references and the rule of §1
//! applies to it unchanged: *a reference may not be stored anywhere that
//! outlives what it points at*. The destination position is
//! [`regions::Step::Capture`] and [`crate::regions`]'s §5 says why that step
//! comes from the rvalue rather than from the type.
//!
//! **The consequence for `type-checking-and-mir.md` Decision 8 is the point of
//! the exercise.** [`crate`]'s §6 said narrowing was safe across a closure only
//! because `science-types` walks a closure's body inline, *"a different
//! mechanism from the one Decision 8 names"*. It is now rule 4 doing it: a
//! closure that captures a place exclusively takes an exclusive borrow of it,
//! that borrow has a region, and [`crate::access`]'s §1 refuses every read and
//! write of an overlapping place inside it. Decision 8's argument extends to
//! closures for the reason Decision 8 gives, and not by accident.
//!
//! **What it does not extend to** is a call *through* a closure: this walk sees
//! a [`Callee::Indirect`], and §5's rule already assumes such a callee may hand
//! back a reference into anything it can see — which now includes the closure's
//! own captures, because the closure value has regions to be reached. That went
//! from vacuous to load-bearing without a line changing in
//! [`opaque_callee`].
//!

use science_diagnostics::Span;
use science_mir::mir::{
    Body, Callee, Local, Operand, Place, Point, Rvalue, StatementKind, TerminatorKind,
    RETURN_PLACE,
};
use science_resolve::hir::DefId;

use crate::constraints::{Cause, Constraints};
use crate::regions::{self, Path, RegionTable, RegionVar, Step};
use crate::summary::{Summaries, Summary};

/// Walks one body and emits every constraint it implies. §3 step 2.
pub fn constraints_of(
    body: &Body,
    table: &RegionTable,
    summaries: &Summaries,
) -> Constraints {
    let mut out = Constraints::new();
    for (block, basic) in body.blocks() {
        for (at, statement) in basic.statements.iter().enumerate() {
            let point = Point { block, statement: at as u32 };
            if let StatementKind::Assign { place, rvalue } = &statement.kind {
                assignment(&mut out, table, place, rvalue, point, statement.span);
            }
        }
        let point = basic.terminator_point(block);
        if let TerminatorKind::Call { callee, args, destination, .. } = &basic.terminator.kind {
            call(
                &mut out,
                body,
                table,
                summaries,
                callee,
                args,
                destination,
                point,
                basic.terminator.span,
            );
        }
    }
    out
}

fn assignment(
    out: &mut Constraints,
    table: &RegionTable,
    dest: &Place,
    rvalue: &Rvalue,
    point: Point,
    span: Span,
) {
    match rvalue {
        // §2.
        Rvalue::Ref { place, borrow, .. } => {
            let loan = table.loan(*borrow);
            // 1. The loan covers wherever the reference is live.
            for (position, var) in destinations(table, dest) {
                match position.split_first() {
                    // The destination's own outer reference *is* the loan.
                    None => out.push(loan, var, point, Cause::Borrowed, span),
                    // Everything beneath the new reference is the borrowed
                    // value's own, reborrowed unchanged: `borrowed parser`
                    // still points at the same tokens.
                    Some((Step::Deref, rest)) => {
                        let rest = rest.to_vec();
                        relate_one(out, table, place, &rest, var, point, Cause::Borrowed, span);
                    }
                    // A position under a reference that is not reached through
                    // it cannot exist; §1 of `regions` builds them all with a
                    // `Deref` first.
                    Some(_) => {}
                }
            }
            // 2. The loan cannot outlive a reference it was taken through.
            //    §2 and §6.
            for var in through_references(table, place) {
                out.push(var, loan, point, Cause::ReborrowedFrom, span);
            }
        }
        Rvalue::Use(operand) | Rvalue::Cast { operand, .. } | Rvalue::Narrow { operand, .. } => {
            // A store into `_0` is the thing Decision 5's analysis reads, so it
            // says so rather than being one more `AssignedFrom` in the chain.
            let cause = if dest.local == RETURN_PLACE && dest.is_local() {
                Cause::Returned
            } else {
                Cause::AssignedFrom
            };
            from_operand(out, table, dest, &[], operand, point, cause, span);
        }
        Rvalue::Coerce { operand, .. } => {
            from_operand(out, table, dest, &[], operand, point, Cause::Coerced, span);
        }
        // §3. One constraint per field, and none between them.
        Rvalue::Record { fields, .. } => {
            for (field, operand) in fields {
                let prefix = [Step::Field(*field)];
                let cause = Cause::StoredInField(*field);
                from_operand(out, table, dest, &prefix, operand, point, cause, span);
            }
        }
        Rvalue::Variant { variant, payload } => {
            for (at, operand) in payload.iter().enumerate() {
                let prefix = [Step::Variant { variant: *variant, index: at as u32 }];
                let cause = Cause::AssignedFrom;
                from_operand(out, table, dest, &prefix, operand, point, cause, span);
            }
        }
        Rvalue::Tuple(operands) => {
            for (at, operand) in operands.iter().enumerate() {
                let prefix = [Step::Tuple(at as u32)];
                let cause = Cause::AssignedFrom;
                from_operand(out, table, dest, &prefix, operand, point, cause, span);
            }
        }
        // A `Bool`, an integer, a discriminant, a range of numbers: no
        // reference reaches the destination, so there is nothing to constrain.
        Rvalue::Unary { .. }
        | Rvalue::Binary { .. }
        | Rvalue::Range { .. }
        | Rvalue::IsPresent(_)
        | Rvalue::Discriminant(_) => {}
        // §8. A closure value is an aggregate of references, so it is the
        // `Tuple` arm with one difference: the destination position is a
        // `Step::Capture` the *rvalue* supplied rather than a step a type walk
        // found. One constraint per capture and none between them, which is
        // Decision 3 applied to the one aggregate whose type does not describe
        // its own contents.
        Rvalue::Closure { captures, .. } => {
            for (at, operand) in captures.iter().enumerate() {
                let at = at as u32;
                let prefix = [Step::Capture(at)];
                from_operand(out, table, dest, &prefix, operand, point, Cause::Captured(at), span);
            }
        }
        // An `Error` rvalue is a mistake already reported, and `ty`'s §5
        // discipline says a hole must not cascade.
        Rvalue::Error => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn call(
    out: &mut Constraints,
    body: &Body,
    table: &RegionTable,
    summaries: &Summaries,
    callee: &Callee,
    args: &[Operand],
    destination: &Place,
    point: Point,
    span: Span,
) {
    let def = match callee {
        Callee::Def(def) => Some(*def),
        Callee::Indirect(_) | Callee::Runtime(_) | Callee::Unresolved(_) => None,
    };
    match def.and_then(|def| summaries.lookup(def)) {
        Some(summary) if !summary.opaque => {
            known_callee(out, table, summary, args, destination, point, span)
        }
        // §5, narrowed by `summary`'s §4 wherever a declaration says which
        // arguments the result may point into.
        _ => {
            let declared = def.and_then(|def| summaries.declared_sources(def));
            opaque_callee(out, body, table, callee, def, args, destination, point, span, declared)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn known_callee(
    out: &mut Constraints,
    table: &RegionTable,
    summary: &Summary,
    args: &[Operand],
    destination: &Place,
    point: Point,
    span: Span,
) {
    let cause = Cause::ReturnedFrom(Some(summary.def));
    for (position, from) in &summary.returns {
        let Some(var) = destination_at(table, destination, &position.path) else { continue };
        for source in from {
            let Some(operand) = args.get(source.param) else { continue };
            let Some(place) = operand.place() else { continue };
            for found in regions_at(table, place, &source.path) {
                out.push(found, var, point, cause, span);
            }
        }
    }
    // §3 of `summary`: what the callee may store back through a parameter.
    for (target, from) in &summary.escapes {
        let Some(operand) = args.get(target.param) else { continue };
        let Some(place) = operand.place() else { continue };
        for var in regions_at(table, place, &target.path) {
            for source in from {
                let Some(operand) = args.get(source.param) else { continue };
                let Some(place) = operand.place() else { continue };
                for found in regions_at(table, place, &source.path) {
                    out.push(found, var, point, Cause::PassedTo(summary.def), span);
                }
            }
        }
    }
}

/// §5's decision, written out.
#[allow(clippy::too_many_arguments)]
fn opaque_callee(
    out: &mut Constraints,
    body: &Body,
    table: &RegionTable,
    callee: &Callee,
    def: Option<DefId>,
    args: &[Operand],
    destination: &Place,
    point: Point,
    span: Span,
    declared: Option<&[usize]>,
) {
    // Everything the call can see: the arguments, and — for an indirect call —
    // the closure value itself, whose captures are references it may hand back
    // (§8). Before `lower`'s §8 lowered them this line was true and vacuous;
    // the closure local had no regions to reach.
    let mut visible: Vec<Place> = args.iter().filter_map(|it| it.place().cloned()).collect();
    if let Callee::Indirect(operand) = callee {
        if let Some(place) = operand.place() {
            visible.push(place.clone());
        }
    }

    let sources: Vec<RegionVar> =
        visible.iter().flat_map(|place| reachable(table, place)).collect();

    // The result may point into any of them — unless the callee was declared,
    // in which case `summary`'s §4 says which of them. The **escape** half
    // below is not narrowed: a declaration says what its return borrows and
    // says nothing about what it writes through a `mutable borrowed`
    // parameter, so §5's assumption stands there and §7's direction is
    // unchanged.
    let returned: Vec<RegionVar> = match declared {
        Some(indices) => indices
            .iter()
            .filter_map(|at| args.get(*at))
            .filter_map(|operand| operand.place())
            .flat_map(|place| reachable(table, place))
            .collect(),
        None => sources.clone(),
    };
    for (_, var) in destinations(table, destination) {
        for source in &returned {
            out.push(*source, var, point, Cause::ReturnedFrom(def), span);
        }
    }

    // And any of them may be stored into any place the call can write through.
    for place in &visible {
        for target in writable(table, place) {
            for source in &sources {
                if *source == target {
                    continue;
                }
                out.push(*source, target, point, Cause::PassedTo(def.unwrap_or(body.def())), span);
            }
        }
    }
}

// --- the plumbing ---------------------------------------------------------

/// The destination's positions, as `(path relative to the destination, var)`.
///
/// Falls back to the whole local's regions when the projection cannot be named
/// — [`RegionTable::place`]'s `None` — because relating too much makes regions
/// bigger, and a bigger region loses no conflict.
fn destinations(table: &RegionTable, place: &Place) -> Vec<(Path, RegionVar)> {
    match table.place(place) {
        Some(found) => found.into_iter().map(|(position, var)| (position.path, var)).collect(),
        None => table.local(place.local).iter().map(|(p, v)| (p.path.clone(), *v)).collect(),
    }
}

fn destination_at(table: &RegionTable, place: &Place, path: &Path) -> Option<RegionVar> {
    destinations(table, place).into_iter().find(|(found, _)| found == path).map(|(_, var)| var)
}

/// The regions a place has at one path beneath it.
fn regions_at(table: &RegionTable, place: &Place, path: &Path) -> Vec<RegionVar> {
    destinations(table, place)
        .into_iter()
        .filter(|(found, _)| found == path)
        .map(|(_, var)| var)
        .collect()
}

/// Every region a place can reach: its own positions, and the regions of every
/// reference it is reached *through*. §5.
fn reachable(table: &RegionTable, place: &Place) -> Vec<RegionVar> {
    let mut out: Vec<RegionVar> = destinations(table, place).into_iter().map(|(_, v)| v).collect();
    for prefix in regions::deref_prefixes(place) {
        out.extend(destinations(table, &prefix).into_iter().map(|(_, v)| v));
    }
    out.sort();
    out.dedup();
    out
}

/// The regions a callee could store *into*, given an argument place.
///
/// A callee writes back through a `mutable borrowed` parameter and through
/// nothing else, so the writable positions are the ones strictly beneath a
/// mutable reference. `summary`'s §3 says why this is exercised by nothing in
/// the corpus.
fn writable(table: &RegionTable, place: &Place) -> Vec<RegionVar> {
    let Some(found) = table.place(place) else { return Vec::new() };
    let mutable: Vec<Path> = found
        .iter()
        .filter(|(position, _)| position.mutable)
        .map(|(position, _)| position.path.clone())
        .collect();
    found
        .into_iter()
        .filter(|(position, _)| {
            mutable.iter().any(|prefix| {
                position.path.len() > prefix.len() && position.path.starts_with(prefix)
            })
        })
        .map(|(_, var)| var)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn from_operand(
    out: &mut Constraints,
    table: &RegionTable,
    dest: &Place,
    prefix: &[Step],
    operand: &Operand,
    point: Point,
    cause: Cause,
    span: Span,
) {
    let Some(place) = operand.place() else { return };
    for (path, var) in destinations(table, dest) {
        if !path.starts_with(prefix) {
            continue;
        }
        let position = path[prefix.len()..].to_vec();
        relate_one(out, table, place, &position, var, point, cause, span);
    }
}

/// `'src: 'dest @ point` for one destination position. [`relate`]'s body, and
/// the only place in the crate a constraint is born.
#[allow(clippy::too_many_arguments)]
fn relate_one(
    out: &mut Constraints,
    table: &RegionTable,
    src: &Place,
    position: &Path,
    dest: RegionVar,
    point: Point,
    cause: Cause,
    span: Span,
) {
    for var in regions_at(table, src, position) {
        out.push(var, dest, point, cause, span);
    }
}

/// The regions of every reference a place is reached *through*.
///
/// Two sources, and the second is §6's workaround:
///
/// 1. the place in front of each [`science_mir::mir::Projection::Deref`], which
///    is the ordinary reborrow;
/// 2. the place itself, when it is a whole local whose *type* is a reference —
///    which is what a receiver borrow lowers to and what §6 is about.
pub fn through_references(table: &RegionTable, place: &Place) -> Vec<RegionVar> {
    let mut out = Vec::new();
    for prefix in regions::deref_prefixes(place) {
        out.extend(reachable(table, &prefix));
    }
    if let Some(var) = own_reference(table, place) {
        out.push(var);
    }
    out.sort();
    out.dedup();
    out
}

/// The region of a place whose **local** is a reference at its root. §6.
///
/// **The whole-local restriction is gone, and that is the second half of the
/// same workaround.** `self` alone and `self.field` are both reached through
/// the reference the local holds; Science has no dereference operator
/// (`AGENTS.md` §1) so neither one writes it, and `science-mir` inserts a
/// [`Projection::Deref`] for some of these and not others. The case that
/// exposed it is `examples/07_generics.science`'s `first_inner`:
///
/// ```text
/// let found be items.get(0)          # `(borrowed Wrapper of T)?`
/// if found?:
///     return borrowed found.inner
/// ```
///
/// `found` holds a reference into the caller's array — `regions`' walk gives it
/// a position at the empty path, because `T?` is not a step — so
/// `borrowed found.inner` points into the caller's frame and not into `found`'s
/// four bytes of storage. Reading it as a borrow of the local is `SC0333` on a
/// correct program, and it is a false positive the *declaration* uncovered:
/// while `Array.get` resolved to nothing, `found` was `Ty::ERROR` and
/// [`crate::check`]'s §6 second suppression kept it quiet.
///
/// **What it costs** is that rule 5 is not checked against any place under a
/// root-level reference, which is right — that storage belongs to the caller —
/// and that the loan is constrained by the *whole* local's region rather than
/// by the region at the field's own path. The second is a loss of precision in
/// §7's direction, because a larger set of constraints only keeps loans alive
/// longer.
fn own_reference(table: &RegionTable, place: &Place) -> Option<RegionVar> {
    table
        .local(place.local)
        .iter()
        .find(|(position, _)| position.path.is_empty())
        .map(|(_, var)| *var)
}

/// The local a borrow's storage question is about, or `None` when what the
/// borrow points at is not storage this body owns. [`crate::check`]'s §4.
///
/// Two exemptions, and they are the two halves of [`through_references`]: a
/// place reached through a [`science_mir::mir::Projection::Deref`] points into
/// another frame, and a whole local that is itself a reference points wherever
/// it points — §6.
pub fn storage_root(table: &RegionTable, place: &Place) -> Option<Local> {
    if !regions::deref_prefixes(place).is_empty() {
        return None;
    }
    if own_reference(table, place).is_some() {
        return None;
    }
    Some(place.local)
}

