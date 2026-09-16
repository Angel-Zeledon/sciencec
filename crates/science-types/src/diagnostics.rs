//! `SC0260` and `SC0261` — §10.1 item 8.
//!
//! > **§10.1 item 8.** A diagnostic added after the checker exists is a
//! > diagnostic that renders whatever the checker happens to have kept.
//! > Provenance spans (§9.3) have to be in the representation from the start.
//!
//! # 1. The block, and why it is public separately from the error
//!
//! §9.2 renders a shape mismatch in full and the piece this note owes it is
//! **where each symbol came from, and what the checker's normal forms were**:
//!
//! ```text
//!    = the compiler compared these two const expressions:
//!
//!          left    n        normalised   n
//!          right   m        normalised   m
//!
//!      `n` and `m` are distinct const parameters, so no assignment of values
//!      makes them equal for every instantiation
//!
//!    = where each came from:
//! ```
//!
//! Decision 9.1 says that block is reported by `SC0261` *only when no
//! higher-level code owns the site*. A shape comparison reports
//! `broadcasting.md`'s `SC0294`; a dimension comparison reports
//! `unit-literals.md`'s `SC0256`; and in both cases this block is attached to
//! *their* diagnostic as a note. **One code per user-visible mistake; the const
//! layer contributes the explanation, not a second error.**
//!
//! So [`normal_form_block`] hands back the note and the legend labels as
//! pieces, and [`cannot_show_equal`] is the caller that assembles them into an
//! `SC0261`. A higher-level code calls the first and not the second. Had the
//! block only existed inside the error, the two neighbouring notes would each
//! have grown their own copy of it, and three renderings of one explanation is
//! the diagnostic disaster §9.1 is written to avoid.
//!
//! # 2. What the legend is for
//!
//! For a parameter bound by a `with` head (§8.1) the name appears in no
//! signature anywhere, so *"where was `n` bound?"* has no answer a reader can
//! find by searching. The legend is that answer, and it is why the atoms carry
//! `DefId`s rather than names: the definition span is one lookup away, and two
//! parameters that share a name do not share a legend entry.
//!
//! A builtin definition carries `hir::BUILTIN_SPAN`, which points at no
//! readable text, and `hir` is explicit that a diagnostic must never use one as
//! a label. [`normal_form_block`] checks and omits rather than pointing at
//! nothing.
//!
//! # 3. What this actually renders, and the two places it differs from §9.2
//!
//! ```text
//! error[SC0261]: these const expressions cannot be shown equal
//!  --> pipeline/residual.science:5:17
//!   |
//! 1 | def residual of (const n: Int, const m: Int)(
//!   |                        ------ `n` bound here
//!   |                                      ------ `m` bound here
//! 2 |     block: Grid of (F32, n),
//!   |                          --- n
//! 3 |     reference: Grid of (F32, m),
//!   |                              --- m
//! 4 | ) -> Grid of (F32, n):
//! 5 |     block.minus(reference, 1)
//!   |                 ^^^^^^^^^ the two sides must be the same here
//!   |
//!   = note: the compiler compared these two const expressions:
//!
//!       left    n   normalised   n
//!       right   m   normalised   m
//!
//!   `n` and `m` are distinct const parameters, so no assignment of values
//!   makes them equal for every instantiation
//! ```
//!
//! **One.** `science-diagnostics`' renderer prefixes every note with `note: `,
//! so the block's first line reads `= note: the compiler compared …` where
//! §9.2's mock-up has no prefix. Closing that gap means changing the
//! renderer's note handling, which is `science-diagnostics`' decision.
//!
//! **Two.** §9.2 prints the legend under a `= where each came from:` note and
//! a second `-->` header. The renderer groups labels by file and puts every
//! label above every note, so when the binders and the comparison are in one
//! file — which is the ordinary case — the legend lands in the snippet
//! instead, as it does above. That reads better than the mock-up, because the
//! binder and the use are visible at once; it is worse when they are in two
//! files, where the legend becomes a second snippet with no sentence
//! introducing it. Adding the sentence would put it *after* the labels it
//! introduces, so it is left out rather than misplaced.

use std::fmt::Write as _;

use science_diagnostics::{Diagnostic, Label, Span, Suggestion};
use science_resolve::hir::DefTable;

use crate::codes;
use crate::const_expr::ConstExpr;
use crate::normal::{Atom, ConstEvalError, NormalForm};

// --- SC0260 ---------------------------------------------------------------

/// A const argument whose literal is not an integer at all.
///
/// `Window of (Int, "a")` parses — the parser accepts any [`Literal`] in a
/// const-argument position and lets a later phase say what is wrong with it —
/// and this is that phase. The message names what the position admits rather
/// than only what it found, because a reader who wrote a float there is often
/// reaching for a kind the language does not have (§2.3 refused `F64` on the
/// ground that float equality is not equality).
///
/// [`Literal`]: science_resolve::hir::Literal
pub fn non_integer_literal(span: Span, found: &str) -> Diagnostic {
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "a const argument is an integer expression")
        .with_label(Label::primary(span, format!("this is {found}")))
        .with_note(
            "const parameters are annotated `Int` or `Shape` and nothing else, so a const \
             argument denotes an integer or a list of them",
        )
}

/// A const argument whose integer literal carries a type suffix.
///
/// A suffix names a *type*, and a const-argument position has no types in it:
/// the kind is fixed by the declaration (§2.3), so the suffix can only agree
/// or be wrong. Deleting it is always the fix, and it is offered as one.
pub fn suffixed_literal(span: Span) -> Diagnostic {
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "a const argument carries no type suffix")
        .with_label(Label::primary(span, "the suffix names a type, and this position has none"))
        .with_suggestion(Suggestion {
            span,
            replacement: String::new(),
            message: "the kind comes from the declaration".to_string(),
        })
}

/// A const argument whose magnitude leaves `i128`.
///
/// See `normal`'s §5 for why this is a refusal rather than a wrap: the two
/// alternatives both have the checker assert an equality that is false, which
/// is what §4.1 rejects rational coefficients over.
pub fn literal_too_large(span: Span) -> Diagnostic {
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "this const argument is too large")
        .with_label(Label::primary(span, "outside the range of a 128-bit signed integer"))
        .with_note(
            "a const expression is evaluated in `i128`, so every value it can denote is one \
             the compiler can compare exactly",
        )
}

/// A const expression naming something that does not resolve.
///
/// Resolution has already reported the unresolved name. This exists so that
/// lowering has something to return rather than a panic, and it is a
/// **warning-free silent refusal by design**: the caller drops it, because a
/// second diagnostic for one mistake is how a compiler acquires cascades.
pub fn unresolved_const_param(span: Span) -> Diagnostic {
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "this name did not resolve")
        .with_label(Label::primary(span, "reported already"))
}

/// `e / k` — division, which is §4's quotient and is not in F0.
///
/// Refused rather than lowered. The normaliser has no rule for a quotient
/// atom, so accepting this would mean normalising it wrongly, and a checker
/// that asserts a false equality is worse than one that admits less: the
/// first is a miscompilation and the second is a diagnostic.
pub fn division_is_f1(span: Span) -> Diagnostic {
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "a const expression cannot divide yet")
        .with_label(Label::primary(span, "division in a const expression is F1"))
        .with_note(
            "`const-expression-arithmetic.md` §4 specifies the quotient atom; F0 is the              quotient-free fragment, and multiplying by a literal is what F0 offers instead",
        )
}

/// Arithmetic inside a const expression left the range of `i128`.
///
/// The blame lands on the node whose arithmetic overflowed, which is narrower
/// than the whole expression and is the factor a reader has to change.
/// `fallback` covers the case where the error carries no span of its own,
/// which today it always does and tomorrow's arms may not.
pub fn overflowed(error: ConstEvalError, fallback: Span) -> Diagnostic {
    let span = error.span().unwrap_or(fallback);
    Diagnostic::error(codes::NOT_A_CONST_EXPRESSION, "this const expression overflows")
        .with_label(Label::primary(span, "the value leaves the range of a 128-bit signed integer"))
        .with_note(
            "const expressions are evaluated exactly; there is no wrapping, because a type-level \
             value that disagrees with the run-time value it describes is not an approximation",
        )
}

// --- SC0261 ---------------------------------------------------------------

/// One side of a comparison, with everything §9.2's table prints about it.
///
/// `written` and `form` are both carried because the block prints both
/// columns: *"left `n` normalised `n`"*. A caller that has only one of them
/// has only half a message, and the half it is missing is the half that
/// answers *"is `k + 1` versus `1 + k` the problem?"* — which §9.2 names as one
/// of the four things this rendering does that "mismatched types" does not.
pub struct Comparand<'a> {
    /// Where this side was written. The whole const argument, not one operand.
    pub span: Span,
    /// The expression as written.
    pub written: &'a ConstExpr,
    /// Its normal form.
    pub form: &'a NormalForm,
}

/// §9.2's block, in the two pieces a diagnostic is built from.
///
/// `note` goes in `Diagnostic::notes`; `legend` goes in `Diagnostic::labels`.
/// They are separate because the renderer puts labels above notes and groups
/// labels by file, and a caller assembling this into `SC0294` needs to
/// interleave its own labels with these.
pub struct NormalFormBlock {
    /// The normal-form table and the sentence explaining the difference.
    pub note: String,
    /// One secondary label per atom, at its definition site.
    pub legend: Vec<Label>,
    /// Whether any atom was omitted from the legend for having no readable
    /// definition site. A caller that cares can say so in words.
    pub elided_builtins: bool,
}

/// Builds §9.2's normal-form-and-legend block for two const expressions.
///
/// Reported by `SC0261` when this crate owns the site, and attached as a note
/// to `SC0294` or `SC0256` when one of them does (Decision 9.1).
pub fn normal_form_block(
    left: &Comparand<'_>,
    right: &Comparand<'_>,
    defs: &DefTable,
) -> NormalFormBlock {
    let left_written = left.written.render(defs);
    let right_written = right.written.render(defs);
    let width = left_written.chars().count().max(right_written.chars().count());

    let mut note = String::from("the compiler compared these two const expressions:\n\n");
    let _ = writeln!(
        note,
        "      left    {:width$}   normalised   {}",
        left_written,
        left.form.render(defs),
        width = width
    );
    let _ = writeln!(
        note,
        "      right   {:width$}   normalised   {}",
        right_written,
        right.form.render(defs),
        width = width
    );
    note.push('\n');
    for line in explain(left.form, right.form, defs) {
        let _ = writeln!(note, "  {line}");
    }
    while note.ends_with('\n') {
        note.pop();
    }

    // Atoms in order, without duplicates: the union of the two forms, and both
    // are already sorted, so this is a merge that happens to be written as a
    // sort of a concatenation because the lists are two.
    let mut atoms: Vec<Atom> = left.form.atoms().chain(right.form.atoms()).collect();
    atoms.sort_unstable();
    atoms.dedup();

    let mut elided_builtins = false;
    let mut legend = Vec::new();
    for atom in atoms {
        if atom.is_builtin(defs) {
            elided_builtins = true;
            continue;
        }
        legend.push(Label::secondary(
            atom.definition_span(defs),
            format!("`{}` bound here", atom.render(defs)),
        ));
    }

    NormalFormBlock { note, legend, elided_builtins }
}

/// The `SC0261` diagnostic: two const expressions cannot be shown equal.
///
/// `site` is where the comparison was demanded — the operator, the argument,
/// the assignment. The two comparands get secondary labels so that a reader
/// sees which side is which without counting.
pub fn cannot_show_equal(
    site: Span,
    left: &Comparand<'_>,
    right: &Comparand<'_>,
    defs: &DefTable,
) -> Diagnostic {
    let block = normal_form_block(left, right, defs);

    let message = "these const expressions cannot be shown equal";
    let mut diagnostic = Diagnostic::error(codes::NOT_PROVABLY_EQUAL, message)
        .with_label(Label::primary(site, "the two sides must be the same here"))
        .with_label(Label::secondary(left.span, left.written.render(defs)))
        .with_label(Label::secondary(right.span, right.written.render(defs)));

    for label in block.legend {
        diagnostic = diagnostic.with_label(label);
    }
    diagnostic = diagnostic.with_note(block.note);
    if block.elided_builtins {
        diagnostic = diagnostic.with_note("some of these symbols come from the prelude");
    }
    diagnostic
}

/// Names the first difference between two normal forms, in words.
///
/// §9.2's sentence — *"`n` and `m` are distinct const parameters, so no
/// assignment of values makes them equal for every instantiation"* — is the
/// disjoint-atoms case, and it is the common one, so it is the one spelled
/// out. The others are stated flatly rather than explained, because a reader
/// who can see `2*n` against `3*n` in the table above does not need a
/// paragraph about it.
///
/// On the quotient-free fragment every difference here is a *definite*
/// inequality: §3.4's injectivity says two forms denote the same function
/// exactly when they are structurally equal. The wording avoids promising that
/// forever, because §4.3's quotient atoms make the same procedure incomplete
/// and this sentence is where the incompleteness will have to be admitted.
fn explain(left: &NormalForm, right: &NormalForm, defs: &DefTable) -> Vec<String> {
    let mut only_left = Vec::new();
    let mut only_right = Vec::new();
    let mut differing = Vec::new();

    let mut atoms: Vec<Atom> = left.atoms().chain(right.atoms()).collect();
    atoms.sort_unstable();
    atoms.dedup();
    for atom in atoms {
        let (a, b) = (left.coefficient_of(atom), right.coefficient_of(atom));
        match (a, b) {
            (0, 0) => {}
            (0, _) => only_right.push(atom.render(defs)),
            (_, 0) => only_left.push(atom.render(defs)),
            _ if a != b => differing.push((atom.render(defs), a, b)),
            _ => {}
        }
    }

    let mut lines = Vec::new();
    if !only_left.is_empty() && !only_right.is_empty() {
        lines.push(format!(
            "{} and {} are distinct const parameters, so no assignment of values",
            list(&only_left),
            list(&only_right)
        ));
        lines.push("makes them equal for every instantiation".to_string());
    } else if !only_left.is_empty() {
        lines.push(format!("{} appears on the left and not on the right", list(&only_left)));
    } else if !only_right.is_empty() {
        lines.push(format!("{} appears on the right and not on the left", list(&only_right)));
    }
    for (name, a, b) in differing {
        lines.push(format!("`{name}` has coefficient {a} on the left and {b} on the right"));
    }
    if left.constant() != right.constant() {
        lines.push(format!(
            "the constant terms differ: {} on the left, {} on the right",
            left.constant(),
            right.constant()
        ));
    }
    if lines.is_empty() {
        lines.push("the two normal forms are not structurally equal".to_string());
    }
    lines
}

/// `` `a` ``, `` `a` and `b` ``, `` `a`, `b` and `c` ``.
fn list(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}
