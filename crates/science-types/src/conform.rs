//! Conformance — whether an `implements` block answers the interface it names.
//!
//! `methods`' §2 reads an implementation block as a *contribution*: whatever it
//! writes becomes callable, and whatever the interface defaulted becomes
//! callable beside it. That is the right reading for a **lookup**, and it is
//! the wrong reading for a **promise**. `Doc implements Render:` is a promise
//! that a `Doc` answers every one of `Render`'s methods, at the types `Render`
//! declared; Decision 13's `any Render` is a value dispatched through that
//! promise, and `def show of T: Render(..)` is a body checked against it. Until
//! this module existed nothing held a block to it: a block that implemented
//! none of the interface's methods, or all of them at the wrong types, checked
//! exactly as clean as one that implemented them correctly.
//!
//! **What that cost is not hypothetical.** A missing method is a call through
//! `any Render` with nothing behind it; a method at the wrong types is a call
//! that type-checks at the declaration's signature and runs at the block's. The
//! second is the worse one, because the author who wrote the block believes
//! they wrote the interface's method, and the compiler agreed with them.
//!
//! # 1. Three refusals, and each is a different sentence
//!
//! - **A declared method the block does not write**, and that the interface did
//!   not default — [`codes::UNIMPLEMENTED_INTERFACE_METHOD`]. The subject is
//!   the *block*: it named an interface and did not finish.
//! - **A method the block writes that the interface does not declare** —
//!   [`codes::NOT_AN_INTERFACE_METHOD`]. The subject is the *method*: it is a
//!   good method in the wrong block, and §3 says where it goes.
//! - **A method whose signature is not the declared one** —
//!   [`codes::MISMATCHED_IMPLEMENTATION`]. The subject is the *signature*, and
//!   the receiver, the parameters and the return are one subject because they
//!   are one thing the author writes on one line and one thing they fix by
//!   rewriting that line.
//!
//! `lib.rs`' `codes` module argues each against the codes that already exist.
//!
//! # 2. The comparison is against the *substituted* declaration, never the
//! declaration as written
//!
//! `interface Index of Idx:` declares `def index(self, at: Idx) -> borrowed
//! Self.Output`, and `Row implements Index of Int:` writes `def index(self, at:
//! Int) -> borrowed F64`. Those disagree at every position and the block is
//! correct. Three things have to be substituted into the declaration before a
//! comparison means anything, and the crate already computes all three for the
//! *body* checker — this module reuses them rather than deriving a second copy:
//!
//! - **`Self`**, which is [`Declarations::self_ty`] at the block.
//! - **The interface's own parameters**, which is
//!   [`Declarations::interface_arguments`] — that function's doc comment is the
//!   `Idx` case above, and it exists because a signature inherited from `Index`
//!   mentions `Idx` and only the block says what it is.
//! - **The associated types**, which is [`Declarations::body_substitution`].
//!   Both spellings are bound there, the interface's `Self.Output` and the
//!   block's, for the reason that function states.
//!
//! **`check.rs`'s `scrutinee_substitution` is the same move one construct
//! over** — a declaration written in terms of parameters, compared against a
//! use that supplied arguments — and this is that pattern at a signature rather
//! than at a variant payload.
//!
//! **What is *not* substituted is the block's own generics**, and that is
//! correct rather than an omission. `Array of T implements Index of Int:` has
//! `Self` = `Array of T` with `T` standing, and the block's `def index(self, at:
//! Int) -> borrowed T` has the same `T` standing in it. The two sides are
//! compared in the block's scope, where `T` means itself on both, which is what
//! makes a generic implementation checkable at all without instantiating it.
//!
//! # 3. The restraint, which is `methods`' §8 one construct over
//!
//! **Decision. *"The interface does not declare this method"* is a fact about a
//! user interface and a fact about the prelude's transcription, and only the
//! first is reported.**
//!
//! `builtins.rs` declares fourteen of the eighteen prelude interfaces as *names
//! with implementations and no methods*, and says why in as many words: `Ord`
//! would need an `Ordering`, `Display` would need a `Formatter`, and *"a
//! signature invented in passing is how a language acquires a design nobody
//! argued for, so the fourteen stay methodless"*. So `Add` declares no `add`,
//! `Clone` declares no `clone` — and `examples/00_kitchen_sink.science` writes
//! both blocks, correctly. A rule that read *"not declared"* off that table
//! would report on nine blocks in the corpus and would be wrong about every one
//! of them.
//!
//! This is exactly [`Methods::surface_is_closed`](crate::methods::Methods::surface_is_closed)'s
//! argument at the other end of the same partial transcription: a builtin
//! *type*'s method set is open, so a name it does not have is *"not written
//! down yet"*; a builtin *interface*'s method set is open for the same reason
//! and by the same edit, so a name it does not declare is *"not written down
//! yet"* too. The line is `Def::is_builtin`, as it is there.
//!
//! **The other two refusals are not under the restraint, and the asymmetry is
//! the point.** *Missing* and *mismatched* read what the table **has**, and
//! `builtins.rs` puts a method in it only *"where a note gives the method's
//! name and its types"* — `Error.message` is `stdlib-core.md` §7.5 verbatim,
//! `Iterate.next` is `collections-and-chains.md`'s, `Index.index` and
//! `IndexMutably.index_mutably` are `indexing-and-array-literals.md` §1.1
//! written out in Science. A method that is *there* is therefore a requirement,
//! and `Error`'s defaulted `cause` is left out of the table precisely so that
//! it is not read as one. So `LoadError implements Error:` with no `message` is
//! a real diagnostic, and it is one nothing could produce before.
//!
//! **What the restraint costs** is a stray method in a block implementing a
//! methodless prelude interface — `Doc implements Clone:` with a `describe`
//! beside its `clone` — which stays silent. It closes with the fourteen: the
//! day `Ord.compare` has a note behind it, the entry goes in `builtins.rs` and
//! this rule starts reporting there with no edit here.
//!
//! # 4. Where it declines, stated rather than hidden
//!
//! Each of these is silence and each has a reason that is not *"hard"*:
//!
//! - **An erroneous type on either side.** `ty`'s §5 makes `Ty::ERROR` agree
//!   with everything, and the mistake that produced it is already reported.
//!   Comparing through one would print a second diagnostic about the first
//!   one's symptom.
//! - **A const generic on the method itself.** Matching `def take of (const
//!   N: Int)(..)` against a declaration's `const M: Int` means substituting a
//!   *normal form* for a const parameter, and the atom that would have to be
//!   built names the block's parameter rather than the declaration's. It is the
//!   type half's one-line analogue with no one-line spelling, and a comparison
//!   that skipped it would compare two signatures at two different `N`s.
//! - **A block whose `Self` did not lower**, and a method with no signature.
//!   Both mean an earlier pass reported, and §5 of `ty` again.
//! - **An interface with no `Declarations` entry for a declared method.** The
//!   table is the authority on what the declaration *is*; without it there is
//!   nothing to compare against and a guess would be a diagnostic about this
//!   compiler rather than about the program.
//!
//! # 5. Every refusal names a fix, and every fix parses
//!
//! `SC0538`'s documentation refuses to emit a fix nobody can act on, and the
//! three messages here are held to it:
//!
//! - **Missing** prints the declaration the block has to write, rendered in
//!   surface syntax through [`Types::render`], which the same doc comment says
//!   round-trips — *"printing a nullable borrow without [parentheses] prints a
//!   type that reads back as a different one"* is the case it was written for.
//!   The spelling is withheld when any type in it renders `{unknown}`, because
//!   that is a fix a tool applies and gets a second error from.
//! - **Not declared** offers the inherent block, `Doc has:`, which is the
//!   language's own answer to *"a method that belongs to no interface"* —
//!   `AGENTS.md`'s table and `examples/06_traits.science` both write it.
//! - **Mismatched** prints the declared signature beside the written one. It is
//!   the same rendering and the same withholding.

use std::collections::{HashMap, HashSet};

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_resolve::hir::{self, DefId, DefKind, DefTable, GenericParamKind, SelfKind};

use crate::codes;
use crate::items::{Declarations, Signature};
use crate::subst::Substitution;
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// Checks every `implements` block in the crate against the interface it names.
///
/// Runs beside the other whole-crate passes in [`crate::check::check_crate`],
/// and before the bodies rather than after: a block that does not conform is a
/// fact about the declaration, and the author would rather be told the
/// signature is wrong than be told what the wrong signature's body does with
/// it.
pub fn report(
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    diagnostics: &mut Diagnostics,
) {
    for module in &krate.modules {
        for item in &module.items {
            if let hir::ItemKind::Impl(block) = &item.kind {
                block_conforms(block, krate, decls, types, diagnostics);
            }
        }
    }
}

fn block_conforms(
    block: &hir::Impl,
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    diagnostics: &mut Diagnostics,
) {
    // `Doc has:` promises nothing, so there is nothing to hold it to. §1.
    let Some(bound) = &block.interface else { return };
    let Some(interface) = bound.interface_res().and_then(|res| res.def_id()) else { return };
    let defs = &krate.defs;
    if defs.get(interface).kind != DefKind::Interface {
        return;
    }
    // §4: both of these mean an earlier pass reported.
    let Some(self_ty) = decls.self_ty(block.def) else { return };
    if types.references_error(self_ty) {
        return;
    }

    // §2. `body_substitution` carries the associated types under *both*
    // spellings and `Self` keyed at the block; the declaration's `Self` is
    // keyed at the interface, so the same map is re-keyed rather than rebuilt.
    let written_subst = decls.body_substitution(defs, block.def);
    let mut declared_subst = written_subst.clone().with_self(interface, self_ty);
    for (param, arg) in decls.interface_arguments(block.def) {
        declared_subst = match arg {
            GenericArg::Type(ty) => declared_subst.with_type(param, ty),
            GenericArg::Const(form) => declared_subst.with_const(param, form),
            // An argument that did not lower. §4.
            GenericArg::Error => return,
        };
    }

    let declared: Vec<(String, DefId)> = defs
        .children(interface)
        .filter(|def| def.kind == DefKind::Fn)
        .map(|def| (def.name.clone(), def.id))
        .collect();
    let written: HashMap<&str, &hir::Fn> = block
        .methods
        .iter()
        .map(|method| (defs.get(method.def).name.as_str(), method))
        .collect();

    for (name, declared_def) in &declared {
        let Some(declaration) = decls.signature(*declared_def) else { continue };
        match written.get(name.as_str()) {
            // Decision 15's default body: the interface wrote it, `methods`' §2
            // makes it callable on the implementor, and a block that restates
            // it is choosing to override rather than obliged to.
            None if declaration.has_body => {}
            None => diagnostics.push(unimplemented(
                defs,
                types,
                bound.span,
                name,
                interface,
                *declared_def,
                declaration,
                &declared_subst,
            )),
            Some(method) => {
                let Some(found) = decls.signature(method.def) else { continue };
                compare(
                    defs,
                    types,
                    diagnostics,
                    name,
                    interface,
                    *declared_def,
                    declaration,
                    &declared_subst,
                    method,
                    found,
                    &written_subst,
                );
            }
        }
    }

    // §3's restraint, and the whole of its condition.
    if defs.get(interface).is_builtin() {
        return;
    }
    let names: HashSet<&str> = declared.iter().map(|(name, _)| name.as_str()).collect();
    for method in &block.methods {
        let name = &defs.get(method.def).name;
        if !names.contains(name.as_str()) {
            diagnostics.push(not_declared(defs, types, defs.get(method.def).span, name, interface, self_ty));
        }
    }
}

/// A signature with everything §2 substitutes already substituted into it.
struct Shape {
    /// The method's own type parameters, for the rendering. The declaration's
    /// are rewritten to the written method's before a comparison, so the two
    /// sides' lists name the same things.
    generics: Vec<DefId>,
    recv: Option<SelfKind>,
    params: Vec<(DefId, Ty)>,
    ret: Ty,
}

impl Shape {
    /// Whether any type in it is `Ty::ERROR`. §4's first bullet.
    fn is_erroneous(&self, types: &Types) -> bool {
        types.references_error(self.ret)
            || self.params.iter().any(|(_, ty)| types.references_error(*ty))
    }
}

fn shape(types: &mut Types, sig: &Signature, subst: &Substitution) -> Option<Shape> {
    let mut params = Vec::with_capacity(sig.params.len());
    for param in &sig.params {
        params.push((param.def, subst.apply(types, param.ty).ok()?));
    }
    Some(Shape {
        generics: sig.generics.iter().map(|generic| generic.def).collect(),
        recv: sig.self_param.map(|(_, kind)| kind),
        params,
        ret: subst.apply(types, sig.ret).ok()?,
    })
}

/// Whether §2's substitution left a `Self.Item` standing in this position.
///
/// **Decision. A declared position the block did not answer is not compared,
/// and the block is not held to the `Self.Item` it left unanswered.**
///
/// `interface Index of Idx:` declares `type Output` and `def index(self, at:
/// Idx) -> borrowed Self.Output`, and `tests/operators.rs` writes five
/// implementations of it that supply `index` and **not** `type Output is F64`.
/// Those check today, and the element type the language uses is read off the
/// block's own `index` — `BodyChecker::index_expr` takes it from the
/// implementation's signature, not from the associated type. So the block's
/// return is the *answer* to `Self.Output` rather than a disagreement with it,
/// and comparing the two would report `expected Self.Output, found borrowed
/// F64` at a site where nothing is wrong. `Declarations::body_substitution`
/// makes the same call one phase over: `subst`'s §2 *"leaves a `Self.Item`
/// standing"* rather than guessing at one.
///
/// **What it costs is the position, not the signature.** The receiver and the
/// arity are still compared, and a block whose `index` takes two parameters or
/// a `mutable self` is still refused. What is not refused is a block that
/// answers `Self.Output` with the wrong type — which cannot be told from
/// answering it correctly until something requires the associated type to be
/// written. That requirement is a fourth refusal ("an implementation that does
/// not supply a declared associated type") and it is a rule the corpus does not
/// hold today, so taking it here would be deciding it in passing.
fn unanswered(types: &Types, ty: Ty) -> bool {
    match types.kind(ty) {
        TyKind::SelfAssoc { .. } => true,
        TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => unanswered(types, *inner),
        TyKind::Tuple(elements) => elements.iter().any(|inner| unanswered(types, *inner)),
        TyKind::Closure { params, ret } => {
            let ret = *ret;
            params.iter().any(|inner| unanswered(types, *inner)) || unanswered(types, ret)
        }
        TyKind::Named { args, .. } | TyKind::Object { args, .. } => args
            .iter()
            .filter_map(GenericArg::as_type)
            .any(|inner| unanswered(types, inner)),
        _ => false,
    }
}

/// What the two signatures disagree about, which is what the primary label
/// says. One code and several labels, for §1's reason: the author fixes all of
/// them by rewriting one line, and a code per position would be three codes
/// whose only difference is which word of one sentence changed.
enum Disagreement {
    Generics,
    Receiver,
    Arity,
    Param(usize),
    Return,
}

#[allow(clippy::too_many_arguments)]
fn compare(
    defs: &DefTable,
    types: &mut Types,
    diagnostics: &mut Diagnostics,
    name: &str,
    interface: DefId,
    declared_def: DefId,
    declaration: &Signature,
    declared_subst: &Substitution,
    method: &hir::Fn,
    found: &Signature,
    written_subst: &Substitution,
) {
    // The method's own generics, matched positionally so that the declaration's
    // `T` becomes the written method's `T` before anything is compared. §4's
    // second bullet is the arm that gives up.
    let mut declared_subst = declared_subst.clone();
    if declaration.generics.len() == found.generics.len() {
        for (from, to) in declaration.generics.iter().zip(&found.generics) {
            match (&from.kind, &to.kind) {
                (GenericParamKind::Type { .. }, GenericParamKind::Type { .. }) => {
                    let ty = types.param(to.def);
                    declared_subst = declared_subst.with_type(from.def, ty);
                }
                _ => return,
            }
        }
    }

    let Some(want) = shape(types, declaration, &declared_subst) else { return };
    let Some(got) = shape(types, found, written_subst) else { return };
    if want.is_erroneous(types) || got.is_erroneous(types) {
        return;
    }

    let disagreement = if want.generics.len() != got.generics.len() {
        Disagreement::Generics
    } else if want.recv != got.recv {
        Disagreement::Receiver
    } else if want.params.len() != got.params.len() {
        Disagreement::Arity
    } else if let Some(at) = want
        .params
        .iter()
        .zip(&got.params)
        .position(|((_, a), (_, b))| a != b && !unanswered(types, *a))
    {
        Disagreement::Param(at)
    } else if want.ret != got.ret && !unanswered(types, want.ret) {
        Disagreement::Return
    } else {
        return;
    };

    diagnostics.push(mismatched(
        defs,
        types,
        defs.get(method.def).span,
        name,
        interface,
        declared_def,
        &want,
        &got,
        disagreement,
    ));
}

// --- the rendering --------------------------------------------------------

/// How a receiver is written, which is the fix for half of [`mismatched`]'s
/// cases and has to be the spelling `AGENTS.md`'s table gives.
fn render_recv(recv: Option<SelfKind>) -> Option<&'static str> {
    match recv? {
        SelfKind::Shared => Some("self"),
        SelfKind::Mutable => Some("mutable self"),
        SelfKind::Value => Some("self: Self"),
    }
}

/// One signature in surface syntax: `def index of T(self, at: Int) -> borrowed T`.
///
/// `None` where any type in it renders `{unknown}` — §5, and `SC0538`'s rule
/// about a fix that cannot be taken.
fn render_signature(defs: &DefTable, types: &Types, name: &str, shape: &Shape) -> Option<String> {
    let mut out = format!("def {name}");
    if !shape.generics.is_empty() {
        out.push_str(" of ");
        // `AGENTS.md` §3: one argument takes no parentheses and two or more
        // must have them. `Types::render_args` makes the same choice one
        // construct over.
        let names: Vec<&str> =
            shape.generics.iter().map(|def| defs.get(*def).name.as_str()).collect();
        if names.len() > 1 {
            out.push('(');
            out.push_str(&names.join(", "));
            out.push(')');
        } else {
            out.push_str(names[0]);
        }
    }
    out.push('(');
    let mut written = Vec::new();
    if let Some(recv) = render_recv(shape.recv) {
        written.push(recv.to_string());
    }
    for (def, ty) in &shape.params {
        written.push(format!("{}: {}", defs.get(*def).name, types.render(defs, *ty)));
    }
    out.push_str(&written.join(", "));
    out.push(')');
    if shape.ret != Ty::UNIT {
        out.push_str(" -> ");
        out.push_str(&types.render(defs, shape.ret));
    }
    // A `{unknown}` anywhere makes the whole spelling unusable, and the braces
    // are what `Types::render` guarantees no surface syntax uses.
    (!out.contains("{unknown}")).then_some(out)
}

/// The `defined here` label, when there is any text to point at.
///
/// A prelude declaration's span is in `BUILTIN_FILE` and points at no readable
/// line, so labelling it would print a caret into a file the author cannot
/// open. The message names the interface either way.
fn declared_at(defs: &DefTable, declared_def: DefId, message: String) -> Option<Label> {
    let def = defs.get(declared_def);
    (!def.is_builtin()).then(|| Label::secondary(def.span, message))
}

// --- the three diagnostics ------------------------------------------------

/// `SC0539` — an interface method the block never implements.
#[allow(clippy::too_many_arguments)]
fn unimplemented(
    defs: &DefTable,
    types: &mut Types,
    span: Span,
    name: &str,
    interface: DefId,
    declared_def: DefId,
    declaration: &Signature,
    subst: &Substitution,
) -> Diagnostic {
    let interface_name = &defs.get(interface).name;
    let mut diagnostic = Diagnostic::error(
        codes::UNIMPLEMENTED_INTERFACE_METHOD,
        format!("this block does not implement `{interface_name}`'s method `{name}`"),
    )
    .with_label(Label::primary(span, format!("`{name}` is missing from this implementation")));
    if let Some(label) =
        declared_at(defs, declared_def, format!("`{name}` is declared here, with no body"))
    {
        diagnostic = diagnostic.with_label(label);
    }
    diagnostic = diagnostic.with_note(
        "an interface method with no body is required of every implementation; one with a body \
         is a default the block may leave out",
    );
    // §5. The spelling is the substituted declaration, which is the line the
    // author writes verbatim — not the declaration as the interface wrote it,
    // which may still say `Self` or a parameter this block has answered.
    if let Some(shape) = shape(types, declaration, subst) {
        if let Some(signature) = render_signature(defs, types, name, &shape) {
            diagnostic =
                diagnostic.with_note(format!("write it in this block: `{signature}:`"));
        }
    }
    diagnostic
}

/// `SC0540` — a method in the block that the interface does not declare.
fn not_declared(
    defs: &DefTable,
    types: &Types,
    span: Span,
    name: &str,
    interface: DefId,
    self_ty: Ty,
) -> Diagnostic {
    let interface_name = &defs.get(interface).name;
    let ty = types.render(defs, self_ty);
    Diagnostic::error(
        codes::NOT_AN_INTERFACE_METHOD,
        format!("`{interface_name}` declares no method `{name}`"),
    )
    .with_label(Label::primary(span, format!("`{name}` is not one of `{interface_name}`'s methods")))
    .with_note(
        "an `implements` block answers one interface and holds nothing else, so that the \
         methods a type gets from an interface are exactly the ones the interface declares",
    )
    // §5. The inherent block is where a method that belongs to no interface
    // goes, and it is a block the author may already have.
    .with_note(format!("move it to an inherent block: `{ty} has:`"))
}

/// `SC0541` — a method whose signature is not the one the interface declared.
#[allow(clippy::too_many_arguments)]
fn mismatched(
    defs: &DefTable,
    types: &Types,
    span: Span,
    name: &str,
    interface: DefId,
    declared_def: DefId,
    want: &Shape,
    got: &Shape,
    disagreement: Disagreement,
) -> Diagnostic {
    let interface_name = &defs.get(interface).name;
    let label = match disagreement {
        Disagreement::Generics => format!(
            "this takes {} type parameter(s); `{interface_name}` declares {}",
            got.generics.len(),
            want.generics.len()
        ),
        Disagreement::Receiver => {
            let wanted = render_recv(want.recv).unwrap_or("no receiver");
            let found = render_recv(got.recv).unwrap_or("no receiver");
            format!("this takes `{found}`; `{interface_name}` declares `{wanted}`")
        }
        Disagreement::Arity => format!(
            "this takes {} parameter(s); `{interface_name}` declares {}",
            got.params.len(),
            want.params.len()
        ),
        Disagreement::Param(at) => format!(
            "parameter {} is `{}`; `{interface_name}` declares `{}`",
            at + 1,
            types.render(defs, got.params[at].1),
            types.render(defs, want.params[at].1)
        ),
        Disagreement::Return => format!(
            "this returns `{}`; `{interface_name}` declares `{}`",
            types.render(defs, got.ret),
            types.render(defs, want.ret)
        ),
    };
    let mut diagnostic = Diagnostic::error(
        codes::MISMATCHED_IMPLEMENTATION,
        format!("`{name}` does not have the signature `{interface_name}` declares for it"),
    )
    .with_label(Label::primary(span, label));
    if let Some(secondary) =
        declared_at(defs, declared_def, format!("`{name}` is declared here"))
    {
        diagnostic = diagnostic.with_label(secondary);
    }
    diagnostic = diagnostic.with_note(
        "an implementation is held to the declaration, because a call through the interface — \
         `any Summarize`, or a body checked against a bound — is dispatched at the declared \
         signature and not at this one",
    );
    // §5, and it is the whole fix: one line, written out.
    if let Some(signature) = render_signature(defs, types, name, want) {
        diagnostic = diagnostic.with_note(format!("write the declared signature: `{signature}:`"));
    }
    diagnostic
}
