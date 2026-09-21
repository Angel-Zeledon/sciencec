//! Exhaustiveness and usefulness — Decision 16, and the check §4.5 promised in
//! its second sentence and nothing ever made.
//!
//! > **Decision 16. Usefulness and exhaustiveness by Maranget's algorithm**
//! > (*Warnings for pattern matching*, JFP 2007), which is what rustc uses. A
//! > non-exhaustive `match` is an error and reports a concrete witness — the
//! > shape of a value no arm covers — rather than "not exhaustive".
//! >
//! > Nullable types participate: `match doc:` over a `Doc?` has `null` as a
//! > constructor, and matching `null` plus the non-null case is exhaustive.
//!
//! `science-resolve`'s `resolve.rs` defers the question in as many words —
//! *"and whether a `match` is exhaustive"* is on its list of things a later
//! phase owns — and no later phase took it, so until this module a `match` over
//! a twelve-variant choice that named two of them compiled, and the other ten
//! values ran off the end of the dispatch into whatever `science-mir`'s
//! `lower.rs` had left there. That file says so where the hole is: *"an
//! inexhaustive match does, which is that pass's answer to give"*.
//!
//! # 1. The witness is the decision
//!
//! Everything below is transcription except this. Maranget gives *usefulness*:
//! `useful(P, q)` is whether some value matches the row `q` and no row of the
//! matrix `P`, and exhaustiveness is `useful(P, _)` — is a wildcard row still
//! useful against the arms. What the paper calls `I` is the same recursion
//! carrying the **value** it found, and that value, printed as a pattern, is
//! the witness.
//!
//! **Decision. A non-exhaustive `match` is `SC0250`, and the message is the
//! witness.** Not a note, not a label — the first line, because a diagnostic's
//! first line is the one that gets read. *"this `match` is not exhaustive"*
//! makes the reader re-derive what they missed, and for a choice with twelve
//! variants and nested patterns they will get it wrong; `` `Rect(_, _)` `` is
//! the answer, and it is also a thing they can paste.
//!
//! **What it costs** is that the witness has to be *spellable*, and one of them
//! is not: §7's non-null constructor has no surface syntax (§3 below). It
//! prints `_`, which is a superset of what is missing rather than exactly it,
//! and §5 says why that is the right answer there anyway.
//!
//! # 2. The constructor set, per type, and the reason for each
//!
//! Maranget's completeness test is the whole algorithm: a set of head
//! constructors is *complete* when every constructor of the type appears, and
//! an incomplete one is what produces a witness. So the question this module
//! actually answers, for every type in the language, is **what are its
//! constructors and are there finitely many**.
//!
//! | Type | Constructors | Complete? |
//! |---|---|---|
//! | a choice | its variants, each at its payload arity | yes |
//! | a record | one, at its field arity | yes |
//! | a tuple | one, at its width | yes |
//! | `()` | one, at arity zero | yes |
//! | `Bool` | `true` and `false` | yes |
//! | `T?` | `null`, and *present* at arity one (§3) | yes |
//! | `Never` | none at all | yes, vacuously |
//! | `I64`, `F64`, `Char`, `String` | its values | **no** |
//! | a type parameter, `Self`, `any I`, a closure, `Array of T` | unknown | **no** |
//!
//! **`Bool` is finite and that is not a convenience.**
//! `examples/05_match.science` writes
//!
//! ```text
//! def describe_flag(flag: Bool) -> String:
//!     match flag:
//!         true: "on"
//!         false: "off"
//! ```
//!
//! with no wildcard, so a `Bool` whose set were infinite would make the
//! acceptance corpus report. Two values, both spellable, both named in the
//! source: it is a choice that happens to be built in.
//!
//! **`Char` is finite in principle and infinite here.** There are 1,114,112
//! Unicode scalar values, so a `match` over a `Char` naming every one of them
//! would be exhaustive and this module says it is not. The alternative is a
//! completeness test that builds a million-element set to answer *no* a
//! million times out of a million and one, and the program that would benefit
//! does not exist. The same argument is why rustc treats `char` as having
//! ranges rather than an enumerable set, and Science has no range patterns
//! (§4.5's grammar: `..` is an expression, and `PatternKind` has no arm for
//! one) so it does not even have that. **The cost is stated:** a `match` over a
//! `Char` always needs a catch-all, which `examples/05_match.science` already
//! writes at all three of its `Char` matches.
//!
//! **An integer, a float and a `String` are infinite** for the reason that
//! needs no argument, and the consequence is the one Decision 16 wants: `match
//! digit:` over an `Int` needs `_`, and `examples/05_match.science`'s
//! `digit_name` has it.
//!
//! **A type this phase cannot classify is infinite**, which is the conservative
//! direction *for this check* and the opposite of the direction `assign`'s §3
//! and `methods`' §7 take. Those two refuse to answer *no* on an unanswerable
//! question because a wrong *no* is a false positive. Here the unanswerable
//! answer is *"the set is not complete"*, and a wrong *"not complete"* demands
//! a `_` arm on a `match` over a type parameter — which is not a false positive
//! but a true one, because a `T` genuinely has no variants a pattern can name.
//! The only patterns writable against a `T` are `_`, a binding, and a literal,
//! and a `match` made of literals over an unknown type is not exhaustive.
//!
//! # 3. Nullable, which is the one place this check meets the error model
//!
//! §7 gives half the answer — *"`match doc:` over a `Doc?` has `null` as a
//! constructor"* — and leaves the other half, which is what the *other*
//! constructor is. It has to exist: a set containing only `null` is never
//! complete, so `match err:` with a `null` arm and every variant of the error
//! would still report.
//!
//! **Decision. `T?` has two constructors: `null` at arity zero, and *present*
//! at arity one over `T`, and present is written by writing a pattern of `T`.**
//! There is no `Some`. §5.5 took `Some`, `None`, `Ok` and `Err` out of the
//! language — `examples/04_enums.science` says so in its header — so the
//! present constructor has **no spelling of its own**, and a pattern against a
//! `T?` that is not `null` and not a wildcard is a pattern of `T` sitting
//! directly under an invisible constructor. That is the rule, and it is what
//! makes §7's own sentence true:
//!
//! ```text
//! match err:
//!     null: "fine"
//!     NotFound(key): key
//!     Malformed(detail, line): detail
//!     Io(detail): detail
//! ```
//!
//! is exhaustive over a `ConfigError?`, and dropping the `Io` arm reports
//! `` `Io(_)` `` — a witness *inside* the present constructor, printed without
//! it, because printing it is printing something the author cannot type.
//!
//! **Narrowing is what keeps this from firing everywhere.** `19_stdlib`'s
//! `required` writes `match err:` inside `if err?:`, and Decision 7 makes the
//! scrutinee there an [`ExprKind::Narrow`] whose
//! type is the choice and not the nullable — so the two-arm match over
//! `Missing` and `Corrupt` is exhaustive and no `null` arm is demanded. This
//! module reads the scrutinee node's type and therefore gets narrowing for
//! free; had it re-derived the type from the binding it would have demanded a
//! `null` arm on every error match in the corpus, and `narrow`'s §4 would have
//! acquired a consumer before the region checker runs, which Decision 25
//! forbids. **It does not read a narrowed type it computed — it reads the one
//! the checker already wrote onto the node**, which is the same restraint
//! `unchecked`'s §4 is under.
//!
//! **The cost of the present constructor having no spelling** is §1's: when the
//! *whole* present half is missing — `match n:` over an `Int?` with only a
//! `null` arm — the witness prints `_`. `_` is literally correct as a thing to
//! add (it is what closes the match) and imprecise as a description (it also
//! covers `null`, which is covered). The alternative is inventing a spelling,
//! and a diagnostic that prints syntax the language does not have is worse than
//! one that prints a wider shape than it found.
//!
//! # 4. Unreachable arms, which are not free
//!
//! Decision 16 does not say what happens to an arm no value can reach. It comes
//! out of the same recursion — an arm is dead exactly when it is useless
//! against the arms above it — but *"for free"* is wrong by a factor of the arm
//! count.
//!
//! **Decision. An arm useless against the arms above it is `SC0537`, an
//! error.** The reasons, in order:
//!
//! - **There is no benign reading.** Rust warns rather than errors because a
//!   macro can generate a dead arm in code nobody wrote, and `#[allow]` is
//!   there to say so. Science has no macros and no attribute that silences a
//!   diagnostic, so every arm in a Science `match` was typed by somebody, and
//!   an arm that cannot run is a line they believed did something.
//! - **The body was checked.** An unreachable arm's body is type-checked and
//!   lowered, so it is a maintenance surface with no execution — the worst
//!   combination, and the one that makes a wrong belief survive.
//! - **It is the same mistake as the other one.** A `match` that covers too
//!   little and a `match` that covers something twice are one algorithm's two
//!   answers, and reporting one as an error and the other as a warning would
//!   say they differ in kind.
//!
//! **What it costs, and it is the real price of this module.** Exhaustiveness
//! alone is *one* run of the recursion against the finished matrix.
//! Unreachability is one run per arm, against the matrix of the arms above it
//! — so a `match` with `n` arms runs the recursion `n + 1` times over matrices
//! of average height `n/2`, which is quadratic in the arm count where
//! exhaustiveness alone is linear. For the corpus's largest `match` — four arms
//! — that is twenty row-visits instead of four. It is nothing at this size and
//! it is not nothing asymptotically, and the shape of the bill is stated here
//! rather than discovered on a generated dispatch table with four hundred arms.
//!
//! **And there is no escape hatch, which is the half that fails.** A dead arm
//! kept deliberately — a placeholder for a variant not yet added — has to be
//! deleted or commented out. Rust's `#[allow(unreachable_patterns)]` is the
//! thing this trades away, and the language has nowhere to put one.
//!
//! **One thing it deliberately does not do:** an `|` alternative that is dead
//! while the arm as a whole is live — `Symbol(',') | Symbol(',')` — is not
//! reported. The arm is useful, and reporting inside it would need the
//! recursion run once per alternative as well as once per arm, which is the
//! same quadratic one level down for a mistake nobody makes.
//!
//! # 5. What it refuses to answer
//!
//! Silence, not a guess, wherever the input is already wrong — `ty`'s §5, and
//! the same argument [`crate::unchecked`]'s §5 makes about what it cannot see:
//!
//! - **A scrutinee whose type mentions [`Ty::ERROR`]**, because an erroneous
//!   type agrees with everything and its constructor set is a fiction.
//! - **A pattern that did not resolve** — a [`PatKind::Error`], a variant or a
//!   record whose `def` is `None`. The arm it is in may well have covered the
//!   constructor this module is about to report missing, and a second
//!   diagnostic about the consequence of a reported mistake is the cascade
//!   `ty`'s §5 exists to prevent.
//! - **An alias that does not expand.** [`Aliases::reveal`] can fail on a const
//!   expression it cannot evaluate; that failure is already reported where the
//!   alias is, and this module treats the type as unanswerable.
//! - **A `match` whose analysis exceeds [`BUDGET`] recursive steps.** The
//!   algorithm is worst-case exponential in the nesting of `|` alternatives —
//!   this is Maranget's own §5 — and a compiler that hangs is worse than one
//!   that misses. The budget is a **false negative** and therefore the safe
//!   direction: a `match` that exhausts it is accepted, exactly as every
//!   `match` was accepted before this module existed.
//!
//! # 6. Where the code came from, and the one place it is not §13's band
//!
//! `SC0537` is from this crate's own `SC0520`-`SC0579`, where every condition
//! `type-checking-and-mir.md` §13 assumes and does not number has gone since
//! `SC0523`.
//!
//! **`SC0250` is not, and it is not a slip.** Three documents agree and one
//! older one does not. §9 of `docs/superpowers/specs/2026-09-16-science-f0-core-design.md`
//! settles it in a sentence written for exactly this: *"Exhaustiveness errors
//! belong to the type checker and so take a code in the `SC0250` range, not the
//! `SC0210` the previous spec assigned them."* `docs/superpowers/design/README.md`'s
//! free table lists `SC0250` as unclaimed, and
//! `examples/00_kitchen_sink.science` — which this module may not edit — tells
//! its reader that *"a missing variant is an error in the `SC0250` range"*.
//! `science-resolve`'s `codes` module still reserves `SC0210` for this check
//! and its reservation test still passes, because nothing here takes `SC0210`;
//! that comment is now stale in the direction of holding a number nobody will
//! use, which is the harmless direction.

use std::collections::HashSet;

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_resolve::hir::{self, DefId, DefKind, DefTable, Literal};

use crate::alias::Aliases;
use crate::codes;
use crate::items::Declarations;
use crate::thir::{Arm, Body, ExprId, ExprKind, PatId, PatKind};
use crate::ty::{Ty, TyKind, Types};

/// The recursion budget of §5, per `match`.
///
/// Sized so that nothing a human writes reaches it: the largest `match` in
/// `examples/` spends fewer than a hundred steps. A generated dispatch wide
/// enough to exhaust this is accepted unchecked, which §5 prices.
pub const BUDGET: u32 = 100_000;

/// How many witnesses a message names before it starts counting.
///
/// **Three, and the number is a compromise between two documents.** Decision 16
/// says *"a concrete witness"*, singular; §4.5 of the core spec says
/// non-exhaustiveness *"is an error listing the missing patterns"*, plural.
/// One witness means a choice missing four variants takes four compile-edit
/// cycles to close. All of them means a `match` over a twelve-variant choice
/// that named one prints eleven shapes and buries its own first line. Three
/// shows the *shape* of the omission and the count that follows says how much
/// more there is.
const SHOWN: usize = 3;

/// Checks every `match` in one body.
///
/// Runs over the finished THIR, after the writeback of [`crate::check`]'s §4,
/// because a scrutinee whose type is still an inference variable carries
/// [`Ty::ERROR`] on its node until then and §5 would refuse every numeric
/// `match` in the crate.
pub fn report(
    body: &Body,
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    diagnostics: &mut Diagnostics,
) {
    let mut checker = Checker { body, defs: &krate.defs, decls, types, aliases, spent: 0 };
    for (_, expr) in body.exprs() {
        let ExprKind::Match { scrutinee, arms } = &expr.kind else { continue };
        checker.spent = 0;
        checker.match_expr(*scrutinee, arms, diagnostics);
    }
}

struct Checker<'a> {
    body: &'a Body,
    defs: &'a DefTable,
    decls: &'a Declarations,
    types: &'a mut Types,
    aliases: &'a mut Aliases,
    /// §5's budget, spent per `match`.
    spent: u32,
}

/// A head constructor.
///
/// `Lit` carries the *rendered* literal rather than the literal itself, because
/// two literals are the same constructor exactly when they are the same value,
/// and rendering is the cheapest total equality a `f64` payload admits.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Ctor {
    /// `null`. §3.
    Null,
    /// The other half of a `T?`, which has no spelling. §3.
    Present,
    Bool(bool),
    /// A literal of a type whose set is infinite (§2), so this never completes
    /// one — it exists so that two arms naming one value are told apart.
    Lit(String),
    Variant(DefId),
    Record(DefId),
    Tuple(usize),
    Unit,
}

/// A value no arm covers, shaped like the pattern that would cover it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Witness {
    Wildcard,
    Null,
    Bool(bool),
    Lit(String),
    Variant(DefId, Vec<Witness>),
    Record(DefId, Vec<(DefId, Witness)>),
    Tuple(Vec<Witness>),
    Unit,
}

/// One cell of a row: a pattern, or the wildcard the query row is made of.
#[derive(Debug, Clone, Copy)]
enum Entry {
    Wild,
    Pat(PatId),
}

type Row = Vec<Entry>;

/// What a row's head is, once the column's type is known.
enum Head {
    Wild,
    Ctor(Ctor),
    /// `a | b`, which is two rows and not one head. Expanded where the
    /// recursion meets it rather than flattened up front, so that an
    /// alternative nested inside a payload costs nothing until then.
    Or(Vec<PatId>),
}

impl Checker<'_> {
    fn match_expr(&mut self, scrutinee: ExprId, arms: &[Arm], diagnostics: &mut Diagnostics) {
        let scrutinee_ty = self.body.ty(scrutinee);
        if self.types.references_error(scrutinee_ty) {
            return;
        }
        if arms.iter().any(|arm| self.pattern_is_unanswerable(arm.pattern)) {
            return;
        }
        let Some(column) = self.classify(scrutinee_ty) else { return };

        let cols = vec![column];
        let mut matrix: Vec<Row> = Vec::with_capacity(arms.len());
        let mut dead = Vec::new();
        for arm in arms {
            let row = vec![Entry::Pat(arm.pattern)];
            match self.useful(&matrix, &row, &cols) {
                // §5: the budget ran out, so nothing is claimed about this
                // `match` at all — not even about the arms already walked.
                None => return,
                Some(found) if found.is_empty() => dead.push(self.body.pat(arm.pattern).span),
                Some(_) => {}
            }
            matrix.push(row);
        }

        let Some(witnesses) = self.useful(&matrix, &vec![Entry::Wild], &cols) else { return };
        for span in dead {
            diagnostics.push(unreachable_arm(span));
        }
        if witnesses.is_empty() {
            return;
        }
        let rendered = dedup(
            witnesses.into_iter().filter_map(|mut w| w.pop()).map(|w| self.render(&w)).collect(),
        );
        let ty = self.types.render(self.defs, scrutinee_ty);
        diagnostics.push(not_exhaustive(self.body.expr(scrutinee).span, &ty, &rendered));
    }

    // --- the algorithm ----------------------------------------------------

    /// Maranget's `I`: the values that match `query` and no row of `matrix`.
    ///
    /// An empty answer means the query is useless — covered — which is what
    /// exhaustiveness asks of a wildcard row and what §4 asks of an arm.
    /// `None` is §5's budget, or a column this phase cannot speak for: the
    /// question was abandoned, and the caller reports nothing rather than
    /// reporting an answer it did not compute.
    fn useful(&mut self, matrix: &[Row], query: &Row, cols: &[Ty]) -> Option<Vec<Vec<Witness>>> {
        self.spent += 1;
        if self.spent > BUDGET {
            return None;
        }
        if cols.is_empty() {
            // A row of width zero matches the empty value vector, so the query
            // is useful exactly when nothing above it matched at all.
            return Some(if matrix.is_empty() { vec![Vec::new()] } else { Vec::new() });
        }

        let column = cols[0];
        if self.types.references_error(column) {
            return None;
        }
        match self.head(query[0], column) {
            Head::Or(alternatives) => {
                let mut found = Vec::new();
                for alternative in alternatives {
                    let mut row = query.clone();
                    row[0] = Entry::Pat(alternative);
                    found.extend(self.useful(matrix, &row, cols)?);
                    if found.len() >= SHOWN * 2 {
                        break;
                    }
                }
                Some(found)
            }
            Head::Ctor(ctor) => {
                let fields = self.field_tys(&ctor, column, matrix, query);
                let arity = fields.len();
                let specialised = self.specialise(matrix, &ctor, column, arity);
                let row = self.specialise_row(query, &ctor, column, arity)?;
                let mut tys = fields;
                tys.extend_from_slice(&cols[1..]);
                let found = self.useful(&specialised, &row, &tys)?;
                Some(self.rebuild(&ctor, arity, found))
            }
            Head::Wild => self.wildcard(matrix, query, cols, column),
        }
    }

    /// The wildcard case, which is where the constructor set is consulted and
    /// therefore where every witness comes from.
    fn wildcard(
        &mut self,
        matrix: &[Row],
        query: &Row,
        cols: &[Ty],
        column: Ty,
    ) -> Option<Vec<Vec<Witness>>> {
        let used = self.heads_of(matrix, column);
        let all = self.ctor_set(column);
        let missing: Vec<Ctor> = match &all {
            Some(all) => all.iter().filter(|ctor| !used.contains(*ctor)).cloned().collect(),
            None => Vec::new(),
        };

        if let Some(all) = all.clone() {
            if missing.is_empty() {
                let mut found = Vec::new();
                for ctor in all {
                    let fields = self.field_tys(&ctor, column, matrix, query);
                    let arity = fields.len();
                    let specialised = self.specialise(matrix, &ctor, column, arity);
                    let mut row = vec![Entry::Wild; arity];
                    row.extend_from_slice(&query[1..]);
                    let mut tys = fields;
                    tys.extend_from_slice(&cols[1..]);
                    let deeper = self.useful(&specialised, &row, &tys)?;
                    found.extend(self.rebuild(&ctor, arity, deeper));
                    if found.len() >= SHOWN * 2 {
                        break;
                    }
                }
                return Some(found);
            }
        }

        // Incomplete: the default matrix answers the rest of the row, and the
        // constructors nobody named are what goes in front of that answer.
        let default = self.default(matrix, column);
        let rest = self.useful(&default, &query[1..].to_vec(), &cols[1..])?;
        if rest.is_empty() {
            return Some(Vec::new());
        }

        // Nothing was named at all, or the set cannot be enumerated: `_` is the
        // honest shape. Listing every variant of a choice the author has not
        // started matching on would be a wall of text for an empty `match`, and
        // §2's infinite types have no list to print.
        let heads: Vec<Witness> = if used.is_empty() || all.is_none() {
            vec![Witness::Wildcard]
        } else {
            missing.iter().map(|ctor| self.witness_of(ctor)).collect()
        };

        let mut found = Vec::new();
        for head in heads {
            for tail in &rest {
                let mut row = tail.clone();
                row.push(head.clone());
                found.push(row);
                if found.len() >= SHOWN * 2 {
                    return Some(found);
                }
            }
        }
        Some(found)
    }

    /// The rows of `matrix` whose head admits `ctor`, with that head replaced
    /// by its `arity` sub-patterns.
    fn specialise(&mut self, matrix: &[Row], ctor: &Ctor, column: Ty, arity: usize) -> Vec<Row> {
        let mut out = Vec::new();
        for row in matrix {
            match self.head(row[0], column) {
                Head::Or(alternatives) => {
                    for alternative in alternatives {
                        let mut expanded = row.clone();
                        expanded[0] = Entry::Pat(alternative);
                        out.extend(self.specialise(&[expanded], ctor, column, arity));
                    }
                }
                _ => {
                    if let Some(row) = self.specialise_row(row, ctor, column, arity) {
                        out.push(row);
                    }
                }
            }
        }
        out
    }

    /// One row specialised, or `None` when its head is a different constructor.
    ///
    /// An `Or` head never reaches here — [`Checker::specialise`] expands it, and
    /// a query row carrying one is handled by [`Checker::useful`]'s own arm.
    fn specialise_row(&mut self, row: &Row, ctor: &Ctor, column: Ty, arity: usize) -> Option<Row> {
        let mut head = match self.head(row[0], column) {
            Head::Wild => vec![Entry::Wild; arity],
            Head::Ctor(found) if found == *ctor => self.sub_entries(row[0], ctor, arity),
            _ => return None,
        };
        head.extend_from_slice(&row[1..]);
        Some(head)
    }

    /// The sub-patterns of a row's head, once it is known to be `ctor`.
    fn sub_entries(&self, entry: Entry, ctor: &Ctor, arity: usize) -> Vec<Entry> {
        let Entry::Pat(id) = entry else { return vec![Entry::Wild; arity] };
        match (&self.body.pat(id).kind, ctor) {
            // §3: the present constructor is invisible, so its one field is
            // the pattern itself, read at the inner type.
            (_, Ctor::Present) => vec![Entry::Pat(id)],
            (PatKind::Record { fields, def: Some(def) }, Ctor::Record(_)) => self
                .decls
                .record(*def)
                .map(|record| {
                    record
                        .fields
                        .iter()
                        .map(|(field, _)| {
                            fields
                                .iter()
                                .find(|(named, _)| named == field)
                                .map(|(_, pat)| Entry::Pat(*pat))
                                // A field the pattern does not name is a
                                // wildcard: `Point(x: 0)` says nothing of `y`.
                                .unwrap_or(Entry::Wild)
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![Entry::Wild; arity]),
            (PatKind::Variant { elems, .. }, Ctor::Variant(_))
            | (PatKind::Tuple(elems), Ctor::Tuple(_)) => {
                let mut out: Vec<Entry> = elems.iter().map(|pat| Entry::Pat(*pat)).collect();
                out.resize(arity, Entry::Wild);
                out
            }
            _ => vec![Entry::Wild; arity],
        }
    }

    /// The rows that say nothing about the first column, with it removed.
    fn default(&mut self, matrix: &[Row], column: Ty) -> Vec<Row> {
        let mut out = Vec::new();
        for row in matrix {
            match self.head(row[0], column) {
                Head::Wild => out.push(row[1..].to_vec()),
                Head::Or(alternatives) => {
                    for alternative in alternatives {
                        let mut expanded = row.clone();
                        expanded[0] = Entry::Pat(alternative);
                        out.extend(self.default(&[expanded], column));
                    }
                }
                Head::Ctor(_) => {}
            }
        }
        out
    }

    /// Every head constructor the matrix names in its first column.
    fn heads_of(&mut self, matrix: &[Row], column: Ty) -> HashSet<Ctor> {
        let mut out = HashSet::new();
        let mut stack: Vec<Entry> = matrix.iter().map(|row| row[0]).collect();
        while let Some(entry) = stack.pop() {
            match self.head(entry, column) {
                Head::Wild => {}
                Head::Ctor(ctor) => {
                    out.insert(ctor);
                }
                Head::Or(alternatives) => {
                    stack.extend(alternatives.into_iter().map(Entry::Pat));
                }
            }
        }
        out
    }

    /// What a cell's head is at a column of this type.
    ///
    /// §3 is the nullable arm, and it comes before every other constructor arm
    /// because a pattern of `T` under a `T?` is a `T` pattern *and* a present
    /// head, and the column is what decides which reading applies.
    fn head(&mut self, entry: Entry, column: Ty) -> Head {
        let Entry::Pat(id) = entry else { return Head::Wild };
        let kind = self.body.pat(id).kind.clone();
        match &kind {
            PatKind::Wildcard | PatKind::Binding { .. } => Head::Wild,
            PatKind::Or(alternatives) => Head::Or(alternatives.clone()),
            // §5: already reported, and the arm may have covered anything.
            PatKind::Error => Head::Wild,
            PatKind::Literal(Literal::Null) => Head::Ctor(Ctor::Null),
            _ if self.inner_of(column).is_some() => Head::Ctor(Ctor::Present),
            PatKind::Literal(Literal::Bool(value)) => Head::Ctor(Ctor::Bool(*value)),
            PatKind::Literal(literal) => Head::Ctor(Ctor::Lit(render_literal(literal))),
            PatKind::Variant { def: Some(def), .. } => Head::Ctor(Ctor::Variant(*def)),
            PatKind::Record { def: Some(def), .. } => Head::Ctor(Ctor::Record(*def)),
            PatKind::Tuple(elems) => Head::Ctor(Ctor::Tuple(elems.len())),
            PatKind::Unit => Head::Ctor(Ctor::Unit),
            // A variant or a record that did not resolve. §5 turned the whole
            // `match` off before this could be reached; it says nothing rather
            // than standing for a constructor with no identity.
            PatKind::Variant { .. } | PatKind::Record { .. } => Head::Wild,
        }
    }

    /// The column types a constructor's fields sit at.
    ///
    /// **Read off the THIR wherever a pattern names the constructor**, which is
    /// the whole reason `thir`'s §1 puts a type on a [`Pat`](crate::thir::Pat):
    /// the checker already worked out that `Ident`'s payload is a `String`, and
    /// this pass does not do it again. Every constructor whose fields are asked
    /// for is one some row names — the complete case asks only about
    /// constructors in `used`, and the specialised case asks about the query's
    /// own head — so the fallback below is unreachable rather than approximate.
    ///
    /// **A record is the exception**, and it has to be: a pattern may omit a
    /// field, so for a record the field types come from the declaration, which
    /// is the only place they exist when no node carries one.
    fn field_tys(&mut self, ctor: &Ctor, column: Ty, matrix: &[Row], query: &Row) -> Vec<Ty> {
        match ctor {
            Ctor::Null | Ctor::Unit | Ctor::Bool(_) | Ctor::Lit(_) => Vec::new(),
            Ctor::Present => self.inner_of(column).into_iter().collect(),
            Ctor::Record(def) => self
                .decls
                .record(*def)
                .map(|record| record.fields.iter().map(|(_, ty)| *ty).collect())
                .unwrap_or_default(),
            Ctor::Variant(_) | Ctor::Tuple(_) => {
                let arity = self.arity(ctor);
                let heads: Vec<Entry> =
                    matrix.iter().chain(std::iter::once(query)).map(|row| row[0]).collect();
                for entry in heads {
                    if let Some(tys) = self.sub_tys(entry, ctor, column) {
                        if tys.len() == arity {
                            // `classify`'s own reason, one level down: Decision
                            // 27 can now give a payload sub-pattern `borrowed
                            // Format` where every column below this one still
                            // expects the alias-free, borrow-free type
                            // `ctor_set` and `inner_of` match on. Read straight
                            // off THIR, an unpeeled borrow here is not a
                            // different scrutinee — the constructor set is
                            // still `Format`'s — so this classifies each field
                            // rather than leaving `wildcard` to find no case it
                            // matches and fall back to `_`.
                            return tys
                                .into_iter()
                                .map(|ty| self.classify(ty).unwrap_or(ty))
                                .collect();
                        }
                    }
                }
                vec![Ty::ERROR; arity]
            }
        }
    }

    /// The types of one cell's sub-patterns, when that cell names `ctor`.
    fn sub_tys(&mut self, entry: Entry, ctor: &Ctor, column: Ty) -> Option<Vec<Ty>> {
        let Entry::Pat(id) = entry else { return None };
        let kind = self.body.pat(id).kind.clone();
        if let PatKind::Or(alternatives) = &kind {
            let alternatives = alternatives.clone();
            for alternative in alternatives {
                if let Some(tys) = self.sub_tys(Entry::Pat(alternative), ctor, column) {
                    return Some(tys);
                }
            }
            return None;
        }
        match self.head(entry, column) {
            Head::Ctor(found) if found == *ctor => match &kind {
                PatKind::Variant { elems, .. } | PatKind::Tuple(elems) => {
                    Some(elems.iter().map(|pat| self.body.pat(*pat).ty).collect())
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// How many fields a constructor has, for a witness nothing named.
    fn arity(&self, ctor: &Ctor) -> usize {
        match ctor {
            Ctor::Null | Ctor::Unit | Ctor::Bool(_) | Ctor::Lit(_) => 0,
            Ctor::Present => 1,
            Ctor::Tuple(width) => *width,
            Ctor::Variant(def) => {
                self.decls.variant(*def).map(|variant| variant.payload.len()).unwrap_or(0)
            }
            Ctor::Record(def) => {
                self.decls.record(*def).map(|record| record.fields.len()).unwrap_or(0)
            }
        }
    }

    // --- the types --------------------------------------------------------

    /// The alias-free, borrow-free type a column actually matches on.
    ///
    /// `None` when [`Aliases::reveal`] cannot answer, which §5 makes silence.
    /// **The borrow is peeled** because `match format:` over a `borrowed
    /// Format` is most of the corpus: §6.3 makes the caller write the borrow,
    /// so a scrutinee that came in as a parameter is one, and a constructor set
    /// read off `borrowed Format` rather than off `Format` would be empty and
    /// would report on every `match` in `examples/05_match.science`.
    fn classify(&mut self, ty: Ty) -> Option<Ty> {
        let mut ty = self.aliases.reveal(self.types, ty).ok()?;
        while let TyKind::Borrowed { inner, .. } = self.types.kind(ty) {
            let inner = *inner;
            ty = self.aliases.reveal(self.types, inner).ok()?;
        }
        Some(ty)
    }

    /// `T` when the column is a `T?`, already classified. §3.
    fn inner_of(&mut self, column: Ty) -> Option<Ty> {
        let TyKind::Nullable(inner) = self.types.kind(column) else { return None };
        let inner = *inner;
        self.classify(inner)
    }

    /// §2's table. `None` is that table's *"no"* column: a set with no end.
    fn ctor_set(&mut self, column: Ty) -> Option<Vec<Ctor>> {
        if self.inner_of(column).is_some() {
            return Some(vec![Ctor::Null, Ctor::Present]);
        }
        match self.types.kind(column).clone() {
            TyKind::Unit => Some(vec![Ctor::Unit]),
            TyKind::Tuple(tys) => Some(vec![Ctor::Tuple(tys.len())]),
            TyKind::Named { def, .. } => self.named_set(def, column),
            _ => None,
        }
    }

    fn named_set(&mut self, def: DefId, column: Ty) -> Option<Vec<Ctor>> {
        if self.decls.prelude().is_bool(self.types, column) {
            return Some(vec![Ctor::Bool(true), Ctor::Bool(false)]);
        }
        // An uninhabited type has no constructors, so a `match` over one is
        // exhaustive with no arms at all. It falls out of the same completeness
        // test and costs one line.
        if self.decls.prelude().is_never(self.types, column) {
            return Some(Vec::new());
        }
        match self.defs.get(def).kind {
            DefKind::Choice => Some(
                self.defs
                    .children(def)
                    .filter(|child| child.kind == DefKind::Variant)
                    .map(|child| Ctor::Variant(child.id))
                    .collect(),
            ),
            DefKind::Record => Some(vec![Ctor::Record(def)]),
            _ => None,
        }
    }

    /// Whether any pattern under this one failed to resolve, or carries a type
    /// that is already a hole. §5.
    fn pattern_is_unanswerable(&self, id: PatId) -> bool {
        let pat = self.body.pat(id);
        if self.types.references_error(pat.ty) {
            return true;
        }
        match &pat.kind {
            PatKind::Error => true,
            PatKind::Variant { def: None, .. } | PatKind::Record { def: None, .. } => true,
            PatKind::Variant { elems, .. } | PatKind::Tuple(elems) | PatKind::Or(elems) => {
                elems.iter().any(|pat| self.pattern_is_unanswerable(*pat))
            }
            PatKind::Record { fields, .. } => {
                fields.iter().any(|(_, pat)| self.pattern_is_unanswerable(*pat))
            }
            _ => false,
        }
    }

    // --- witnesses --------------------------------------------------------

    /// Puts a constructor back in front of the witnesses its fields produced.
    fn rebuild(&self, ctor: &Ctor, arity: usize, found: Vec<Vec<Witness>>) -> Vec<Vec<Witness>> {
        found
            .into_iter()
            .map(|mut row| {
                // The recursion builds each answer back to front — a column's
                // witness is *pushed* as the call returns — so the
                // constructor's fields are the last `arity` entries, reversed.
                let at = row.len() - arity;
                let mut fields: Vec<Witness> = row.split_off(at);
                fields.reverse();
                row.push(self.assemble(ctor, fields));
                row
            })
            .collect()
    }

    fn assemble(&self, ctor: &Ctor, fields: Vec<Witness>) -> Witness {
        match ctor {
            Ctor::Null => Witness::Null,
            // §3: the present constructor prints as its field, because there is
            // no spelling to print around it.
            Ctor::Present => fields.into_iter().next().unwrap_or(Witness::Wildcard),
            Ctor::Bool(value) => Witness::Bool(*value),
            Ctor::Lit(text) => Witness::Lit(text.clone()),
            Ctor::Unit => Witness::Unit,
            Ctor::Tuple(_) => Witness::Tuple(fields),
            Ctor::Variant(def) => Witness::Variant(*def, fields),
            Ctor::Record(def) => Witness::Record(
                *def,
                self.decls
                    .record(*def)
                    .map(|record| {
                        let mut fields = fields.into_iter();
                        record
                            .fields
                            .iter()
                            .map(|(field, _)| {
                                (*field, fields.next().unwrap_or(Witness::Wildcard))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
        }
    }

    /// A constructor as a witness with every field a wildcard.
    fn witness_of(&self, ctor: &Ctor) -> Witness {
        self.assemble(ctor, vec![Witness::Wildcard; self.arity(ctor)])
    }

    /// A witness as the author would type it. §1.
    fn render(&self, witness: &Witness) -> String {
        match witness {
            Witness::Wildcard => "_".to_string(),
            Witness::Null => "null".to_string(),
            Witness::Bool(true) => "true".to_string(),
            Witness::Bool(false) => "false".to_string(),
            Witness::Lit(text) => text.clone(),
            Witness::Unit => "()".to_string(),
            Witness::Tuple(elems) => {
                let inner: Vec<String> = elems.iter().map(|w| self.render(w)).collect();
                format!("({})", inner.join(", "))
            }
            Witness::Variant(def, elems) if elems.is_empty() => self.defs.get(*def).name.clone(),
            Witness::Variant(def, elems) => {
                let inner: Vec<String> = elems.iter().map(|w| self.render(w)).collect();
                format!("{}({})", self.defs.get(*def).name, inner.join(", "))
            }
            // A record pattern is written with its field names — `Point(x: 0,
            // y: y)` — so a record witness is too, and the reader can paste it.
            Witness::Record(def, fields) => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(field, w)| format!("{}: {}", self.defs.get(*field).name, self.render(w)))
                    .collect();
                format!("{}({})", self.defs.get(*def).name, inner.join(", "))
            }
        }
    }
}

fn render_literal(literal: &Literal) -> String {
    match literal {
        Literal::Int { value, .. } => value.to_string(),
        Literal::Float { value, .. } => value.to_string(),
        Literal::Str(text) => format!("{text:?}"),
        Literal::Char(c) => format!("'{c}'"),
        Literal::Bool(value) => value.to_string(),
        Literal::Null => "null".to_string(),
    }
}

/// Keeps the first of each, so the order stays the constructor set's order —
/// which is declaration order for a choice, and therefore the order the reader
/// will scan the `choice` block in.
fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|item| seen.insert(item.clone())).collect()
}

/// `SC0250` — §1. The witness is the message.
fn not_exhaustive(span: Span, ty: &str, witnesses: &[String]) -> Diagnostic {
    let shown: Vec<String> = witnesses.iter().take(SHOWN).map(|w| format!("`{w}`")).collect();
    let listed = match witnesses.len().saturating_sub(SHOWN) {
        0 => shown.join(", "),
        more => format!("{}, and {more} more", shown.join(", ")),
    };
    let subject = if witnesses.len() == 1 { "a value" } else { "values" };
    Diagnostic::error(
        codes::NON_EXHAUSTIVE_MATCH,
        format!("`{ty}` has {subject} no arm of this `match` covers: {listed}"),
    )
    .with_label(Label::primary(span, format!("this is `{ty}`")))
    .with_note(format!(
        "a `match` answers for every value or the program has no answer for the rest (§4.5); \
         write an arm for {} — or a `_` arm, which covers everything the arms above leave",
        shown.first().map(String::as_str).unwrap_or("`_`")
    ))
}

/// `SC0537` — §4.
fn unreachable_arm(span: Span) -> Diagnostic {
    Diagnostic::error(codes::UNREACHABLE_ARM, "no value reaches this arm")
        .with_label(Label::primary(span, "every value this matches is matched above"))
        .with_note(
            "an arm is checked and lowered whether or not anything reaches it, so a dead one is \
             code that cannot run and cannot be found to be wrong; there is no attribute that \
             silences this, so the fix is to delete the arm or to move it above the one that \
             shadows it",
        )
}
