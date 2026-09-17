//! §3 step 4 — amended by [`crate::access`]'s §1 — and §7's narrative.
//!
//! # 1. The check
//!
//! For every loan, walk the points of its region in order, and at each one ask
//! what the program does there. A loan that needs its place exclusively
//! conflicts with *any* access to an overlapping place; a loan that needs it
//! shared conflicts with an exclusive one. That is rule 4, and
//! [`crate::access`]'s §1 is why "access" rather than "borrow".
//!
//! The overlap predicate is [`science_mir::mir::Place::may_overlap`], which
//! `science-mir` documents as the deliberately *possible* half of a pair:
//! *"the conflict check of `region-inference.md` §3 step 4 wants possibly the
//! same — being wrong there misses a conflict"*. `a[0]` and `a[1]` therefore
//! conflict, and that note already priced it.
//!
//! # 2. One diagnostic per loan, and the order the three candidates are tried
//! in
//!
//! **Decision. A loan reports its first conflict in point order and then stops
//! being checked.**
//!
//! A loan that outlives its referent conflicts with the drop, with the
//! storage-dead, and with every later read, and reporting all of them is three
//! spellings of one mistake — the failure mode `science-mir`'s §3 calls *"a
//! second report from the level with the worst span to say it from"*, one level
//! on. The cost is that two genuinely different conflicts on one borrow are one
//! message, and the second appears after the first is fixed.
//!
//! **Which one is reported is an order, not a race.** A loan that outlives its
//! referent conflicts with the elaborated drop, with the storage-dead, and with
//! every later read of the place, and the three are one mistake with three
//! spans. The order is: an *explicit* conflict — something the author wrote —
//! first, then rule 5, then the drop. The drop is last because a message that
//! points at the close of a scope and says *"dropped here"* is pointing at a
//! line the author did not write, which §7.1 is the argument against.
//!
//! # 3. Decision 9, and the span that is load-bearing
//!
//! > **Decision 9.** A borrow error is reported as a narrative over three
//! > spans — where the borrow was born, what keeps it alive, and where the
//! > conflict is — and never as a relation between region variables.
//!
//! The first two are on the [`science_mir::mir::BorrowData`] and the
//! [`crate::access::Access`]. The third — *"the first borrow is still needed
//! here"* — is [`last_use`], and §7.1 says it is *"the load-bearing one and …
//! the one a solver without provenance cannot produce"*.
//!
//! **What it is here.** The latest point in the loan's region at which
//! something reads the local the reference was stored in. Latest by *span*
//! rather than by point index, because the reader is looking at a file and the
//! block numbering is not source order — a `loop`'s exit block is numbered
//! after its body and printed before it.
//!
//! **When there is none**, the label is omitted rather than faked. That happens
//! when the thing keeping the loan alive is not a read but a drop — and the
//! message then has two spans and says less, which is the honest outcome and
//! not a bug to paper over.
//!
//! # 4. Rule 5, and the borrow it is not checked for
//!
//! Rule 5 — *"no borrow outlives its referent"* — is checked against
//! [`science_mir::mir::Body::storage_dead_points`] and nothing else, which is
//! §10 item 3.
//!
//! **A borrow taken through a reference is exempt, and that is not a
//! weakening.** `borrowed (*r).field` does not point at `r`'s storage; it
//! points at whatever `r` points at, which is in another frame. Checking it
//! against `r`'s storage-dead would refuse every method that returns a borrow
//! of a field of `self`, which is `DefTable.get` and half of the acceptance
//! case. What constrains it instead is [`crate::constraints::Cause::ReborrowedFrom`],
//! emitted at the borrow: the loan may not outlive the reference it was taken
//! through. [`crate::generate::storage_root`] is the one predicate that decides
//! which of the two applies.
//!
//! # 5. `SC0340`, and why it is quieter than §5.2 expects
//!
//! [`crate::summary`]'s §1 is the argument: a return constrained by two
//! parameters is a *set*, not an ambiguity. What reaches `SC0340` is the empty
//! set, and only when no `SC0333` in the same body already explains it —
//! because a function that returns a borrow of its own local has one mistake,
//! and *"the body does not determine the signature"* is the less useful of the
//! two things to say about it. §6 takes one more case away from it, and
//! [`crate`]'s §4 is the account of what is left.
//!
//! # 6. Two suppressions, and `ty`'s §5 is the precedent for both
//!
//! `science-types`'s `ty` §5 sets the discipline the whole compiler follows
//! about holes: *"a hole is a value rather than an absence … and, in
//! particular, cannot manufacture a cascade"*. Region inference is the first
//! phase with something to say about a reference, so it is the first phase that
//! can say it about a reference that is not there.
//!
//! 1. **`SC0340` is not reported for a body that calls something with no
//!    name.** Four of the corpus's own examples reach it — `07_generics`'s
//!    `largest`, `10_loops`'s `next_line`, `19_stdlib`'s `find` — and in every
//!    one the return is unconstrained because `Array.get` and `Iterate.next`
//!    have no declaration, so the binding is typed
//!    [`science_types::ty::Ty::ERROR`] and the reference is gone. *"Your
//!    signature is undetermined"* is then a message about `stdlib-core.md`.
//! 2. **Rule 5 is not checked against a local the checker could not type.**
//!    [`crate::regions`]'s §3. `borrowed found.inner` where `found` is at
//!    `Ty::ERROR` borrows a temporary holding a `Rvalue::Error`, and a message
//!    about the scope of that temporary is a message about the hole above.
//!
//! **Both are removable and both should be removed** when Decision 11's method
//! lookup reaches the container types. `tests/corpus.rs` holds the census, so
//! the day they close, the numbers move visibly rather than silently.
//!
//! **What they cost** is stated rather than guessed: a genuine `SC0340` or
//! `SC0333` in a body that also calls an unresolved method is not reported.
//! There is no way to tell the two apart without the declaration, and this is
//! the direction `ty`'s §5 chose.
//!
//! # 7. `SC0335`, which has never fired
//!
//! Decision 3's second stated cost is *"the intersection can be empty, and an
//! empty region means the value is dead at birth"*. [`no_common_region`] looks
//! for it at every record construction and [`crate`]'s §5 argues it cannot
//! happen in a solver with no upper bounds. The check is four lines and it is
//! the only thing that would notice if the argument is wrong.

use science_diagnostics::{Diagnostic, Diagnostics, Label};
use science_mir::mir::{
    Body, BorrowData, BorrowKind, LocalKind, Place, Projection, Rvalue, StatementKind,
};
use science_resolve::hir::DefTable;

use crate::access::{Access, AccessKind};
use crate::analysis::BodyAnalysis;
use crate::codes;
use crate::generate::storage_root;
use crate::points::Bits;
use crate::regions::describe_path;

/// Runs §3 step 4 and §5 over one analysed body.
pub fn check_body(
    defs: &DefTable,
    body: &Body,
    analysis: &BodyAnalysis,
    diagnostics: &mut Diagnostics,
) {
    let mut outlives = false;
    for data in body.borrows() {
        // §2's order: an explicit conflict first, then rule 5, then the
        // elaborated drop. A borrow that outlives its referent conflicts with
        // the drop *and* the storage-dead *and* every later read, and the drop
        // is the least informative of the three to name.
        let (explicit, dropped) = rule_four(defs, body, analysis, data);
        if let Some(diagnostic) = explicit {
            diagnostics.push(diagnostic);
            continue;
        }
        if let Some(diagnostic) = rule_five(defs, body, analysis, data) {
            diagnostics.push(diagnostic);
            outlives = true;
            continue;
        }
        if let Some(diagnostic) = dropped {
            diagnostics.push(diagnostic);
        }
    }
    for diagnostic in no_common_region(defs, body, analysis) {
        diagnostics.push(diagnostic);
    }
    if !outlives {
        for diagnostic in undetermined(defs, body, analysis) {
            diagnostics.push(diagnostic);
        }
    }
}

// --- rule 5 ---------------------------------------------------------------

/// §4. `SC0333`.
fn rule_five(
    defs: &DefTable,
    body: &Body,
    analysis: &BodyAnalysis,
    data: &BorrowData,
) -> Option<Diagnostic> {
    let root = storage_root(&analysis.table, &data.place)?;
    // §6 item 2.
    if analysis.table.errored(root) || analysis.table.errored(data.place.local) {
        return None;
    }
    let region = analysis.loan_region(data.id);
    let outlived = body
        .storage_dead_points(root)
        .into_iter()
        .any(|point| region.contains(analysis.index.index(point)));
    if !outlived {
        return None;
    }
    let span = body.local_decl(root).span;
    let borrowed = name_of(defs, body, &data.place);
    let referent = name_of(defs, body, &Place::local(root));

    let mut diagnostic = Diagnostic::error(
        codes::BORROW_OUTLIVES_REFERENT,
        format!("{borrowed} is borrowed for longer than {referent} exists"),
    )
    .with_label(Label::secondary(data.span, format!("{borrowed} is borrowed here")));
    if let Some(last) = last_use(analysis, data, region) {
        diagnostic = diagnostic.with_label(Label::primary(
            last.span,
            "...and the borrow is still needed here, after the value is gone",
        ));
    } else {
        diagnostic = diagnostic
            .with_label(Label::primary(data.span, "...and the borrow outlives this scope"));
    }
    Some(
        diagnostic
            .with_label(Label::secondary(span, format!("{referent} is declared here")))
            .with_note(
                "a borrow may not outlive what it points at (§6.1 rule 5); the value's \
                 storage ends at the close of the scope it was declared in",
            )
            .with_note(
                "there is no lifetime syntax to widen: the fix is to return an owned value \
                 or an index, which is what `examples/21_compiler_shapes.science` §1 does",
            ),
    )
}

// --- rule 4 ---------------------------------------------------------------

/// §1 and §3. `SC0330`, or `SC0334` when the conflicting access is a move.
fn rule_four(
    defs: &DefTable,
    body: &Body,
    analysis: &BodyAnalysis,
    data: &BorrowData,
) -> (Option<Diagnostic>, Option<Diagnostic>) {
    let region = analysis.loan_region(data.id).clone();
    let mut points: Vec<usize> = region.iter().collect();
    points.sort();
    let mut dropped = None;
    for point in points {
        let exclusive = analysis.is_exclusive_at(body, data.id, point);
        for access in analysis.accesses.at(point) {
            if !conflicts(data, access, exclusive) {
                continue;
            }
            let diagnostic = narrative(defs, body, analysis, data, access, &region, exclusive);
            // §2: an elaborated drop is kept aside rather than returned. It
            // conflicts with every borrow that outlives its referent, and
            // `SC0333` says the same thing against a span the author can act
            // on.
            if matches!(access.kind, AccessKind::Drop { .. }) {
                dropped.get_or_insert(diagnostic);
                continue;
            }
            return (Some(diagnostic), dropped);
        }
    }
    (None, dropped)
}

/// Whether one access violates rule 4 against a loan that is live there.
fn conflicts(data: &BorrowData, access: &Access, exclusive: bool) -> bool {
    // The loan's own reservation and its own activation are not accesses
    // against it.
    if let AccessKind::Borrow(_, other) = access.kind {
        if other == data.id {
            return false;
        }
    }
    // Rule 5's comparison point, reported as `SC0333` and not here; and the
    // start of a storage nothing can yet name.
    if matches!(access.kind, AccessKind::StorageDead | AccessKind::StorageLive) {
        return false;
    }
    if !access.place.may_overlap(&data.place) {
        return false;
    }
    // A shared loan tolerates any number of readers; an exclusive one tolerates
    // nobody. §1.
    exclusive || access.kind.is_exclusive()
}

/// Decision 9's three spans, assembled. §3.
fn narrative(
    defs: &DefTable,
    body: &Body,
    analysis: &BodyAnalysis,
    data: &BorrowData,
    access: &Access,
    region: &Bits,
    exclusive: bool,
) -> Diagnostic {
    let name = name_of(defs, body, &data.place);
    let moved = access.kind == AccessKind::Move;
    let code = if moved { codes::MOVED_WHILE_BORROWED } else { codes::CONFLICTING_BORROWS };
    let headline = if moved {
        format!("{name} is moved while it is still borrowed")
    } else {
        format!("{name} is borrowed here and {} before the borrow ends", verb(access))
    };
    let held = if exclusive { "exclusively" } else { "shared" };

    let mut diagnostic = Diagnostic::error(code, headline)
        .with_label(Label::secondary(data.span, format!("{name} is borrowed here, {held}")))
        .with_label(Label::primary(access.span, format!("...and {}", access.kind.described())));
    if let Some(last) = last_use(analysis, data, region) {
        if last.span != access.span {
            diagnostic = diagnostic
                .with_label(Label::secondary(last.span, "the first borrow is still needed here"));
        }
    }
    diagnostic
        .with_note(if exclusive {
            "an exclusive borrow admits no other access to the same place while it lasts \
             (§6.1 rule 4)"
        } else {
            "a shared borrow and an exclusive one cannot overlap (§6.1 rule 4)"
        })
        .with_note(
            "the borrow ends at its last use, so moving the last use above the conflicting \
             line is enough; binding what is needed before it is the other fix",
        )
}

fn verb(access: &Access) -> &'static str {
    match access.kind {
        AccessKind::Read => "read",
        AccessKind::Write => "modified",
        AccessKind::Move => "moved",
        AccessKind::Borrow(BorrowKind::Shared, _) => "borrowed",
        AccessKind::Borrow(_, _) => "borrowed exclusively",
        AccessKind::Drop { .. } => "dropped",
        AccessKind::StorageDead => "dropped",
        AccessKind::StorageLive => "reintroduced",
    }
}

/// §3's third span: the last thing that needs the reference.
fn last_use<'a>(
    analysis: &'a BodyAnalysis,
    data: &BorrowData,
    region: &Bits,
) -> Option<&'a Access> {
    let holder = data.destination.local;
    let mut best: Option<&Access> = None;
    for point in region.iter() {
        for access in analysis.accesses.at(point) {
            if access.place.local != holder {
                continue;
            }
            if !matches!(access.kind, AccessKind::Read | AccessKind::Move) {
                continue;
            }
            if best.is_none_or(|current| access.span.start > current.span.start) {
                best = Some(access);
            }
        }
    }
    best
}

// --- Decision 3's intersection, and Decision 6 ------------------------------

/// §6. `SC0335`.
fn no_common_region(defs: &DefTable, body: &Body, analysis: &BodyAnalysis) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (_, basic) in body.blocks() {
        for statement in &basic.statements {
            let StatementKind::Assign { place, rvalue: Rvalue::Record { def, .. } } =
                &statement.kind
            else {
                continue;
            };
            let Some(positions) = analysis.table.place(place) else { continue };
            if positions.len() < 2 {
                continue;
            }
            let mut common = analysis.solution.region(positions[0].1).clone();
            for (_, var) in &positions[1..] {
                common.intersect_with(analysis.solution.region(*var));
            }
            if !common.is_empty() {
                continue;
            }
            let fields: Vec<String> = positions
                .iter()
                .map(|(position, _)| describe_path(defs, &position.path))
                .collect();
            out.push(
                Diagnostic::error(
                    codes::NO_COMMON_REGION,
                    format!(
                        "`{}` borrows from sources with no common region",
                        defs.get(*def).name
                    ),
                )
                .with_label(Label::primary(
                    statement.span,
                    "there is no point at which all of these borrows are valid",
                ))
                .with_note(format!("the borrowed fields are {}", fields.join(", ")))
                .with_note(
                    "a value whose borrowed fields never overlap is dead the moment it is \
                     built; the fix is to make the two sources live in one scope, or to own \
                     one of them",
                ),
            );
        }
    }
    out
}

/// §5. `SC0340`.
fn undetermined(defs: &DefTable, body: &Body, analysis: &BodyAnalysis) -> Vec<Diagnostic> {
    // §6 item 1.
    if analysis.calls_a_hole {
        return Vec::new();
    }
    let name = &defs.get(body.def()).name;
    analysis
        .summary
        .undetermined()
        .into_iter()
        .map(|position| {
            let where_ = describe_path(defs, &position.path);
            Diagnostic::error(
                codes::UNDETERMINED_SIGNATURE,
                format!("`{name}`'s body does not determine what its result borrows from"),
            )
            .with_label(Label::primary(
                body.span(),
                format!("the returned borrow at {where_} is not tied to any parameter"),
            ))
            .with_note(
                "a signature's regions are an analysis result and there is no syntax to \
                 state them (Decision 5), so a result that no parameter constrains has no \
                 meaning a caller could check",
            )
            .with_note(
                "return an owned value, or an index into something the caller already \
                 holds — which is the choice `examples/21_compiler_shapes.science` §2 makes \
                 for `Scopes.lookup` on design grounds",
            )
        })
        .collect()
}

/// A place's name as a message should show it.
///
/// **A name is quoted and a description is not.** `name_of` falls back to a
/// phrase when a local has no name -- §7.2's *"with no names available, the
/// message has no choice but to describe the program"* -- and the fallback was
/// being wrapped in backticks at every call site, so `SC0333` over the corpus
/// read *"`this expression` is borrowed for longer than `this expression`
/// exists"*: one phrase, three times, punctuated as though it were an
/// identifier the reader could go and find in the file.
///
/// The cost is that two descriptions in one message still read alike. That is
/// a real limit of having no names, and §7.2's problem rather than this
/// function's; what this removes is the false promise that they were names.
fn display(text: String, named: bool) -> String {
    if named {
        format!("`{text}`")
    } else {
        text
    }
}

// --- naming ---------------------------------------------------------------

/// What the author calls a place, for a message that cannot print `'a`.
///
/// §7.2: *"with no names available, the message has no choice but to describe
/// the program"*. A [`LocalKind::Temp`] has no name at all, and the fallback is
/// a description rather than `_7`, because a number the author cannot find in
/// the file is worse than a phrase.
pub fn name_of(defs: &DefTable, body: &Body, place: &Place) -> String {
    let (mut out, named) = match body.local_decl(place.local).kind {
        LocalKind::Param(def) | LocalKind::Binding(def) => (defs.get(def).name.clone(), true),
        LocalKind::Return => ("the returned value".to_string(), false),
        LocalKind::Temp => ("this expression".to_string(), false),
        LocalKind::DropFlag(_) => ("a drop flag".to_string(), false),
    };
    for step in &place.projection {
        match step {
            // Science has no dereference operator, so a `Deref` is printed as
            // nothing: `(*self).tokens` is what the author wrote as
            // `self.tokens`.
            Projection::Deref { .. } | Projection::Downcast { .. } => {}
            Projection::Field { field, .. } => {
                out.push('.');
                out.push_str(&defs.get(*field).name);
            }
            Projection::TupleField { index, .. } => {
                out.push('.');
                out.push_str(&index.to_string());
            }
            Projection::Index { .. } => out.push_str("[..]"),
        }
    }
    display(out, named)
}

