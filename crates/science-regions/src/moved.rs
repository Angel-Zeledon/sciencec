//! Rule 3 — *"using a moved value is an error (`SC0301`)"* — which nothing in
//! the workspace reported until this module existed.
//!
//! # 1. Why the check is here and not in `science-mir`
//!
//! The analysis is [`science_mir::moves`]'s and the diagnostic is not.
//! `science-mir`'s §3 declines to report anything, for a reason that is about
//! spans rather than about convenience: *"once MIR has flattened `a and b` into
//! blocks and jumps, a message about it has to reconstruct what was written"*.
//! This crate is already the phase that renders an ownership mistake over a
//! narrative of spans — Decision 9, [`crate::check`]'s §3 — and it already
//! holds the two tables rule 3 needs, the point numbering and
//! [`crate::access::Accesses`]. So the move analysis is *read* here and run
//! nowhere new.
//!
//! **The code is the core spec's and this crate allocates nothing.**
//! [`crate::codes`]'s §2 is the account: `SC0301` is §6.1 rule 3's, recorded by
//! `docs/superpowers/design/README.md` against `ffi-c-boundary.md` as an
//! existing code that note *reuses*, and `region-inference.md` §12's *"not
//! claimed and not reused"* is a statement about which note **allocates** it,
//! not about which crate may report it. The free block this crate leaves free
//! — `SC0300`, `SC0303`–`SC0329`, `SC0399` — is exactly as free after this
//! module as before it.
//!
//! # 2. Why it is run from [`crate::analyse_crate`] and not from
//! [`crate::check::check_body`]
//!
//! §3's second condition asks a question about a *type*, and answering it needs
//! [`science_types::ty::Types`] and [`science_types::alias::Aliases`] mutably —
//! interning and alias revelation both write. `check_body` takes a
//! [`science_resolve::hir::DefTable`] and a finished [`BodyAnalysis`] and
//! nothing else, deliberately: everything it says is read off tables that are
//! already built. Threading the type tables through it to serve one caller
//! would make every rule in that file able to intern a type, which is a
//! capability none of them should have.
//!
//! # 3. What is reported, and the four ways it declines to
//!
//! **Decision. One `SC0301` per local, at the first point in the body's point
//! order where the local's state is [`State::Gone`], the move that put it there
//! can be named, that move was of the whole local, and the local's type owns
//! something.**
//!
//! Four conditions, and each of them is a refusal to guess.
//!
//! 1. **[`State::Gone`] and not [`State::Maybe`].** `Maybe` is *"it depends on
//!    the path"* — a move inside one arm of an `if`, or one carried around a
//!    loop's back edge. Both are genuine mistakes and rustc reports them; both
//!    are also exactly what Decision 26's drop flag exists for, so the same
//!    state is reached by programs that are correct. Reporting `Gone` only is
//!    the direction [`science_mir::moves`]'s §3 already chose for its own
//!    imprecision: lose errors rather than invent them. `tests/moved.rs`'s
//!    `a_conditional_move_is_not_reported` is the program that gets away.
//! 2. **A move that reaches the use, found by walking back from it.** `Gone` is
//!    also what the analysis says about a local that has *never held a value*:
//!    the entry block starts every non-parameter at `Gone`, because
//!    `StorageLive` gives a local room and not a value. A read with no reaching
//!    move is a use of uninitialised storage — a different mistake, with no
//!    code allocated to it — and saying `SC0301` about it would name a move the
//!    program does not contain. [`move_site`] is the walk and returns `None`
//!    rather than a guess.
//! 3. **The move was of a whole local.** This is
//!    [`science_mir::moves`]'s §3 — *"the analysis tracks whole locals, not move
//!    paths … a move out of `doc.title` marks `doc` as moved"* — met by the
//!    consumer that cannot live with it. That note prices its imprecision
//!    against *drop elaboration*, where it leaks toward a leak; against a
//!    **check** the same imprecision leaks toward a false positive, and the
//!    acceptance case has one: `Scopes.lookup` writes
//!    `if binding.name is name:` and then `return binding.definition`, where
//!    the comparison moves the `String` field and the analysis hears the whole
//!    `binding` go. So a reaching move through a projection disqualifies the
//!    local outright rather than being reported against a different field.
//!    That is a missed error whenever the two projections really are the same
//!    one, and closing it is that note's `move paths` entry and not a change
//!    here.
//! 4. **The moved type owns something**, which is
//!    [`science_mir::moves::needs_drop`]. `science-mir`'s `lower` §5 makes
//!    every non-trivially-copyable operand a `Move` and argues the cost of
//!    being wrong that way is *"at worst a drop flag on a local that does not
//!    need dropping"* — **that pricing is for drop elaboration and it does not
//!    hold here.** Whether a user type is `Copy` is Decision 11's
//!    implementation lookup, which this compiler does not have, so
//!    `examples/21_compiler_shapes.science`'s `DefTable.alloc` — which builds a
//!    `Def` out of a `DefId` and then returns that same `DefId`, a record of
//!    one `Int` — reaches this check as a use after move. Refusing it would be
//!    charging the author for a `Copy` the language would derive. A value that
//!    drops nothing is a bag of scalars: moving it frees nothing and dangles
//!    nothing, so the only thing a missed report costs is the report.
//!
//! # 4. A hole cannot manufacture one, and that is somebody else's decision
//!
//! [`crate::check`]'s §6 has to suppress `SC0340` and rule 5 in a body that
//! contains a hole. **This check needs no such suppression**, and the reason is
//! upstream: `science-mir`'s `lower` §5 makes every argument of a call whose
//! signature it cannot see a *copy*, and [`science_mir::moves::moved_locals`]
//! finds nothing in an [`science_mir::mir::Rvalue::Error`]. So a hole makes
//! moves **invisible** here, never spurious, and the only thing it costs this
//! check is the errors it does not find — which is the entry `science-mir`'s §5
//! already lists as *"a move through a method call is invisible"*.
//!
//! # 5. What counts as a use
//!
//! Every [`AccessKind`] except the four that are not the program reading the
//! value:
//!
//! - **A write to a whole local is not a use, it is a new value.** Rule 3
//!   forbids *using* what was moved, and `s be "otra"` after a move is how a
//!   program takes the name back. A write to a *projection* — `d.title be "x"`
//!   — is a use, because it needs the rest of `d` to still be there.
//! - **A [`AccessKind::Drop`] is not a use.** Drop elaboration has already read
//!   the same analysis and deleted every drop of a local it proved gone
//!   (`science-mir`'s `drops` §1), so a `Drop` surviving on a `Gone` local is a
//!   bug in that pass, and `SC0301` against a line the author did not write is
//!   §7.1's complaint exactly.
//! - **`StorageLive` and `StorageDead` are not uses.** Nothing can name storage
//!   that has not begun, and the end of a scope is rule 5's comparison point
//!   (§10 item 3) rather than a read.

use science_diagnostics::{Diagnostic, Diagnostics, Label};
use science_mir::mir::{Body, LocalKind, Place, Point};
use science_mir::moves::{self, State};
use science_mir::Local;
use science_resolve::hir::DefTable;

use crate::access::{Access, AccessKind};
use crate::analysis::BodyAnalysis;
use crate::check::name_of;
use crate::codes;
use crate::regions::Context;

/// §6.1 rule 3 over one analysed body. `SC0301`.
pub fn use_after_move(
    context: &mut Context<'_>,
    body: &Body,
    analysis: &BodyAnalysis,
    diagnostics: &mut Diagnostics,
) {
    let states = moves::analyse(body);
    // §3's fourth condition, asked once per local rather than once per access:
    // a body reads a local far more often than it declares one, and revealing
    // an alias writes to the table.
    let mut owns: Vec<Option<bool>> = vec![None; body.local_count()];
    // §3's third condition. One `SC0301` per local, in point order, so that
    // which use is reported does not depend on how the blocks are numbered.
    let mut reported: Vec<Local> = Vec::new();
    for (block, _) in body.blocks() {
        // One entry per point of the block, in point order, with the
        // terminator's last: `Moves::walk`'s own shape, so the index into it
        // *is* the statement number and no second convention is introduced.
        for (statement, before) in states.walk(body, block).iter().enumerate() {
            let point = Point { block, statement: statement as u32 };
            let at = analysis.index.index(point);
            for access in analysis.accesses.at(at) {
                let local = access.place.local;
                if !is_use(access) || reported.contains(&local) {
                    continue;
                }
                // A drop flag is compiler storage with no user meaning.
                if matches!(body.local_decl(local).kind, LocalKind::DropFlag(_)) {
                    continue;
                }
                if before[local.index()] != State::Gone {
                    continue;
                }
                // §3 items 2 and 3.
                let Some(moved) = move_site(body, analysis, point, local) else {
                    continue;
                };
                // §3 item 4.
                let owning = *owns[local.index()].get_or_insert_with(|| {
                    moves::needs_drop(
                        context.decls,
                        context.types,
                        context.aliases,
                        body.local_decl(local).ty,
                    )
                });
                if !owning {
                    continue;
                }
                reported.push(local);
                diagnostics.push(narrative(context.defs, body, access, &moved, local));
            }
        }
    }
}

/// Whether an access is the program using the value. §5.
fn is_use(access: &Access) -> bool {
    match access.kind {
        AccessKind::Read | AccessKind::Move | AccessKind::Borrow(_, _) => true,
        AccessKind::Write => !access.place.projection.is_empty(),
        AccessKind::Drop { .. } | AccessKind::StorageDead | AccessKind::StorageLive => false,
    }
}

/// The move that left `local` gone at `from`, or `None` when this check cannot
/// name one.
///
/// **A backward walk over points rather than a second dataflow pass.** The
/// state at `from` is already known — [`State::Gone`] — so what is missing is
/// only a *span*, and a lattice that carried one would be
/// [`science_mir::moves`] answering a question it says it does not answer. The
/// walk halts on every point that gives the local a value or a new storage,
/// because past one of those the local's history belongs to a different value.
///
/// **A reaching move through a projection abandons the local**, which is §3
/// item 3: whole-local tracking cannot say which part of the value is gone, and
/// a message that named the wrong field would be worse than no message.
///
/// **Which move, when two paths each have one.** The one whose span starts
/// latest. The tie-break is by span and not by point index because point index
/// is block numbering — the same thing [`crate::check`]'s §3 refuses for its
/// third span, for the same reason: *"a `loop`'s exit block is numbered after
/// its body and printed before it"*.
fn move_site(body: &Body, analysis: &BodyAnalysis, from: Point, local: Local) -> Option<Access> {
    let mut seen = vec![false; analysis.index.len()];
    let mut stack = predecessors_of(body, from);
    let mut best: Option<Access> = None;
    while let Some(point) = stack.pop() {
        let at = analysis.index.index(point);
        if seen[at] {
            continue;
        }
        seen[at] = true;
        let mut halt = false;
        for access in analysis.accesses.at(at) {
            if access.place.local != local {
                continue;
            }
            match access.kind {
                AccessKind::Move if access.place.projection.is_empty() => {
                    if best.as_ref().is_none_or(|it| access.span.start > it.span.start) {
                        best = Some(access.clone());
                    }
                    halt = true;
                }
                // §3 item 3.
                AccessKind::Move => return None,
                // A whole-local write, or the beginning or end of the storage:
                // whatever was here before, it is not what `from` read.
                AccessKind::Write if access.place.projection.is_empty() => halt = true,
                AccessKind::StorageLive | AccessKind::StorageDead => halt = true,
                _ => {}
            }
        }
        if !halt {
            stack.extend(predecessors_of(body, point));
        }
    }
    best
}

/// The points control can arrive at `point` from.
///
/// Inside a block that is the statement before; at a block's first point it is
/// every predecessor block's terminator, which is the only point of a block
/// with an outgoing edge.
fn predecessors_of(body: &Body, point: Point) -> Vec<Point> {
    if point.statement > 0 {
        return vec![Point { block: point.block, statement: point.statement - 1 }];
    }
    body.predecessors(point.block)
        .iter()
        .map(|block| Point {
            block: *block,
            statement: body.block(*block).statements.len() as u32,
        })
        .collect()
}

/// Two spans and a name, in the layout `science-diagnostics`' `render` module
/// documents for this code.
///
/// **The headline is the reference's and the labels name the value.** That
/// module's worked example is the only rendering of `SC0301` written down in
/// this repository, and a reporter whose headline differed from it would make
/// the one documented example wrong. What that example does not have is a name
/// — [`crate::check::name_of`] gives one here, and falls back to a description
/// for a temporary, which is §7.2's *"with no names available, the message has
/// no choice but to describe the program"*.
///
/// **Two spans and not Decision 9's three.** The third — *"the first borrow is
/// still needed here"* — is a fact about a loan's region, and rule 3 has no
/// loan: nothing between the move and the use is keeping anything alive.
/// Inventing a third label would be [`crate::check`]'s §3 warning realised:
/// *"when there is none, the label is omitted rather than faked"*.
fn narrative(
    defs: &DefTable,
    body: &Body,
    access: &Access,
    moved: &Access,
    local: Local,
) -> Diagnostic {
    let name = name_of(defs, body, &Place::local(local));
    Diagnostic::error(codes::USE_AFTER_MOVE, "use of a moved value")
        .with_label(Label::secondary(moved.span, format!("{name} is moved here")))
        .with_label(Label::primary(
            access.span,
            format!("...and {} here, after the move", used(access)),
        ))
        .with_note(
            "every value has one owner, and assigning, passing or returning it transfers \
             that owner (§6.1 rules 1 and 2), so the name is empty afterwards (rule 3)",
        )
        .with_note(
            "borrow at the earlier use instead of moving, or give the second use a value of \
             its own; a type that is not `Copy` is never copied implicitly",
        )
}

/// The verb the primary label uses.
///
/// [`AccessKind::described`] is the *borrow check's* phrasing and says of a
/// write *"which needs it exclusively"*, which is a fact about rule 4 and
/// tells a reader of a rule 3 message nothing.
fn used(access: &Access) -> &'static str {
    match access.kind {
        AccessKind::Move => "moved again",
        AccessKind::Borrow(_, _) => "borrowed",
        AccessKind::Write => "written through",
        _ => "used",
    }
}
