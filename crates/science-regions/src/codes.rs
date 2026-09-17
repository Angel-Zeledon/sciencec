//! The ownership codes this crate allocates, and the two it deliberately does
//! not.
//!
//! # 1. The block
//!
//! `region-inference.md` §12 claims **`SC0330`, `SC0333`–`SC0379`** in the
//! Ownership range, and `docs/superpowers/design/README.md`'s table agrees.
//! Five of those are named in §7.3 and four are implemented here. The block
//! `SC0300`, `SC0303`–`SC0329`, `SC0399` was free before this crate and is
//! free after it: **nothing here is allocated outside what §12 claimed.**
//!
//! # 2. What is not reported from this crate
//!
//! - **`SC0301` and `SC0302`** — use after move — are the core spec's, and
//!   §12 says *"not claimed and not reused"*. The move analysis they need is
//!   `science_mir::moves`, whose §1 declines to share its lattice with a check
//!   that does not exist. [`MOVED_WHILE_BORROWED`] is a different question — a
//!   move that is legal except that something is still watching — and it is
//!   this crate's.
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
