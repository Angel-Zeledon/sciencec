//! The ownership codes this crate allocates, and the two it deliberately does
//! not.
//!
//! # 1. The block
//!
//! `region-inference.md` §12 claims **`SC0330`, `SC0333`–`SC0379`** in the
//! Ownership range, and `docs/superpowers/design/README.md`'s table agrees.
//! Five of those are named in §7.3 and four are implemented here. The block
//! `SC0300`, `SC0303`–`SC0329`, `SC0399` was free before this crate and is
//! free after it, **with one exception this crate now spends itself:**
//! [`MOVE_OUT_OF_BORROW`] takes `SC0303`, because [`crate::deref_move`] checks
//! a soundness hole no note has allocated a code for and reporting nothing
//! until one does is the wrong side of that trade. Everything else in the
//! free block — `SC0300`, `SC0304`–`SC0329`, `SC0399` — is exactly as free
//! after this crate as before it.
//!
//! # 2. What is not reported from this crate
//!
//! - **`SC0301` is the core spec's §6.1 rule 3, and this crate now *reports*
//!   it without *allocating* it.** The two words are not the same thing, and
//!   §12's *"not claimed and not reused"* is about the first: it says
//!   `region-inference.md` does not put `SC0301` in its own block, and
//!   `docs/superpowers/design/README.md` records the code against
//!   `ffi-c-boundary.md`, which calls it *"(existing) use after move — applies
//!   unchanged inside `unsafe`"*. Existing, in a note that reuses it, means the
//!   code was allocated by the core spec itself and is waiting for a phase that
//!   can check it.
//!
//!   That phase is this one, and the reason is not opportunism: rule 3 needs
//!   the move analysis (`science_mir::moves`), the point numbering
//!   ([`crate::points`]) and what each point does to each place
//!   ([`crate::access`]), and this crate is where all three already are.
//!   `science_mir`'s §1 used to decline to share its lattice *"with a check
//!   that does not exist"*; the check exists, it is [`crate::moved`], and
//!   sharing cost one reader and no change to the lattice.
//!
//!   **The ownership block this crate otherwise leaves free is unchanged.**
//!   `SC0300`, `SC0304`–`SC0329` and `SC0399` were free before and are free
//!   after: reporting a code another document allocated adds nothing to what
//!   §12 claims. `SC0303` is the one code in the block this crate does spend,
//!   and [`MOVE_OUT_OF_BORROW`]'s own comment is the account of it — a
//!   different trade from `SC0301`'s, made for the opposite reason: there the
//!   code existed and no phase checked it, here the violation exists and no
//!   note has allocated it a code.
//! - **`SC0302` is not reported from here, and it is not use after move.**
//!   The allocation record's own gloss for it is
//!   `ffi-c-boundary.md`'s *"(existing) conflicting borrows — what catches
//!   `gemm(a, b, a)`"*, which is §6.1 **rule 4** — the rule
//!   [`CONFLICTING_BORROWS`] implements in general and reports against an
//!   access. Emitting `SC0302` as well would be two codes for one violation,
//!   decided by which note the reader happened to come from, and §7.3's
//!   treatment of `SC0331` — *"a specialisation … and should be implemented as
//!   one"* — is the precedent for refusing that. What `SC0302` is still owed is
//!   the **specialisation**: a call that passes one place to two parameters,
//!   which needs `ffi-c-boundary.md`'s `extern` signatures to say which
//!   parameters are `mutable borrowed`, and F0 has no such declaration to read.
//! - [`MOVED_WHILE_BORROWED`] is a third question again — a move that is legal
//!   except that something is still watching — and it is this crate's.
//! - **`SC0331` and `SC0332`** are `collections-and-chains.md`'s, and §7.3 is
//!   explicit that `SC0331` *"is a specialisation of `SC0330` and should be
//!   implemented as one"*. [`CONFLICTING_BORROWS`] is the general check; the
//!   specialisation is a wrapper that renames the diagnostic when the middle
//!   span is a chain, and it belongs to the note that owns chains.
//! - **`SC0341`** — a borrow live across a suspension point — is allocated and
//!   reserved by §12: *"it is F5's suspension-point check and there is no F5"*.
//!   [`crate::Analysis::borrows_live_at`] is the query F5 would ask; the
//!   diagnostic is not defined here, because a code defined before its
//!   condition exists is a code that renders whatever the checker happens to
//!   have kept.
//! - **`SC0380`** is `ffi-c-boundary.md`'s.

use science_diagnostics::Code;

/// A value is moved out of a place reached through a borrow — shared or
/// exclusive, from a parameter or from a local reborrow.
///
/// **Not allocated by any note, and deliberately the first of the free block**
/// `codes`'s §1 names: `SC0300`, `SC0303`–`SC0329`, `SC0399`. This is a stopgap
/// against a soundness hole no note has closed yet — see
/// [`crate::deref_move`]'s §3 — and picking a code out of that free range
/// rather than reusing `SC0301` or `SC0334` keeps it visibly separate from
/// both: it is not "used after moved" (rule 3 presupposes the move was legal)
/// and not "moved while borrowed" (that move is legal and only the order is
/// wrong; this one is never legal).
pub const MOVE_OUT_OF_BORROW: Code = Code(303);

/// A value is used after it is moved — §6.1 rule 3.
///
/// **Allocated by the core spec and reported by [`crate::moved`]**, which §2
/// above distinguishes from the codes this crate claims. `science-diagnostics`'
/// `render` module documents the layout for it, and that worked example is what
/// the reporter renders against.
///
/// Distinct from [`MOVED_WHILE_BORROWED`] in the direction that matters for a
/// message: there the move is legal and the *order* is wrong; here the move
/// already happened and the value is simply not there.
pub const USE_AFTER_MOVE: Code = Code(301);

/// A shared borrow and an exclusive one overlap. §7.3, and the code §7.1
/// renders in full.
///
/// **Reported against an *access*, not against a second borrow**, which is the
/// amendment [`crate::access`]'s §1 argues for: reading a place that something
/// else holds exclusively is the violation of rule 4 that
/// `type-checking-and-mir.md` Decision 8 depends on, and §3 step 4 as written
/// does not cover it.
pub const CONFLICTING_BORROWS: Code = Code(330);

/// A borrow outlives its referent — rule 5.
///
/// Checked against [`science_mir::mir::Body::storage_dead_points`] and nothing
/// else (§10 item 3). A borrow taken *through* a reference is not checked here:
/// the storage that ends is the reference's, not the referent's, and
/// [`crate::check`]'s §4 says what is checked instead.
pub const BORROW_OUTLIVES_REFERENT: Code = Code(333);

/// A value is moved while borrowed.
///
/// Distinct from `SC0301` in the direction that matters for a message: the move
/// is legal and the borrow is legal, and what is wrong is the order. The
/// narrative therefore has the same three spans as [`CONFLICTING_BORROWS`] and
/// a different verb in the middle one.
pub const MOVED_WHILE_BORROWED: Code = Code(334);

/// A type's fields are borrowed from sources with no common region —
/// Decision 3's empty intersection.
///
/// **This crate has never made it fire, and [`crate`]'s §6 argues it cannot.**
/// The check is kept because the argument is a proof about the shape of the
/// solver rather than about programs, and if the solver ever acquires an upper
/// bound the argument stops holding in silence.
pub const NO_COMMON_REGION: Code = Code(335);

/// The body does not determine the signature's regions — Decision 6.
///
/// **Narrower than §5.2 describes**, and [`crate::summary`]'s §1 is why: a
/// return constrained by *two* parameters is not ambiguous here, because the
/// answer is a set and a caller intersects it. What is left is a return
/// constrained by *none*, which is a body that returns a reference and does not
/// say where it points.
pub const UNDETERMINED_SIGNATURE: Code = Code(340);
