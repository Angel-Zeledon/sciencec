//! Method lookup — Decision 11, and the hole the fourth layer was shaped
//! around.
//!
//! > **Decision 11. Method lookup is: inherent methods on the type, then
//! > interface methods from interfaces the type implements that are in scope.
//! > Ambiguity is an error, never a priority ordering.**
//!
//! `items`'s §3 refused to build this table and said why: *"an index from a
//! receiver type to a method is Decision 11's lookup, which
//! `type-checking-and-mir.md` §6.1 owns and which nothing in this crate
//! builds"*. This is that index, and it is what fills
//! [`ExprKind::MethodCall`](crate::thir::ExprKind::MethodCall)'s `method`
//! slot.
//!
//! # 1. The index is keyed on one definition, and that is the whole trick
//!
//! **Decision. A method hangs off the definition a receiver's type *heads* —
//! the record or choice for `Doc` and `borrowed Doc` alike, the interface for
//! `any Summarize` and for the `Self` of an interface's own default body — and
//! the index is one map from that definition to the methods reachable through
//! it.** [`Methods::receiver`] is the function that computes the key and it is
//! the only place a type's shape is inspected.
//!
//! The reason is that every other spelling of the question needs a second
//! structure. Keying on the *type* would make `Doc`, `Doc of T` and `borrowed
//! Doc` three keys for one set of methods and put a normalisation before every
//! lookup; keying on the implementation block would make the lookup a scan.
//! The head definition is what the language means by *"methods on the type"*,
//! and `ty`'s own `TyKind::Named` documentation already says a checker's
//! question about a name is *"which definition"*.
//!
//! **What it costs** is that a type whose head is not a definition has no
//! methods at all here: a tuple, a closure, `Self.Item`, a type parameter. The
//! last is the one that will be missed — `value.summarize()` where `T` is
//! bounded by `Summarize` is a real program and this index cannot answer it —
//! and §5 says what closing it needs.
//!
//! # 2. Two sources, and the interface half is the one with a subtlety
//!
//! [`Source`] is the *"where did this come from"* half of the ambiguity
//! message, and it has two cases because Decision 11 names two places to look.
//!
//! - **`Doc has:`** contributes every method in it, as [`Source::Inherent`].
//! - **`Doc implements Summarize:`** contributes every method in it *and*
//!   every method of `Summarize` the block did not write, as
//!   [`Source::Interface`]. The second half is Decision 15's `for` loop and
//!   every default body in the language: an interface method with a body is
//!   callable on an implementor that never mentioned it, and an index holding
//!   only what the block spells would resolve `doc.summarize()` and not
//!   `doc.describe()` for no reason the author can see.
//!
//! **A block's own method and the interface declaration it answers are not an
//! ambiguity**, and that is the subtlety. They are one method — the
//! declaration and its implementation — so the block's wins by *name*, before
//! ambiguity is considered at all. Getting this wrong makes every
//! implementation of every interface ambiguous with itself, which is the
//! failure mode that would make Decision 11's rule look absurd rather than
//! careful.
//!
//! # 3. The ambiguity, which is the load-bearing half of the decision
//!
//! **Decision. Two candidates of one name on one type is
//! [`Found::Ambiguous`] and never a winner**, whether the two come from two
//! interfaces, from two inherent blocks, or from one of each.
//!
//! A priority ordering — inherent beats interface, or first-declared beats
//! later — resolves the call silently and the author finds out at run time
//! that the wrong code ran. An error costs them one edit and tells them
//! something true about their program. That is the whole argument for the
//! rule, and it is why the diagnostic names **both** candidates and where each
//! came from: an ambiguity error that says only *"ambiguous"* has refused to
//! answer and refused to explain, which is worse than either.
//!
//! **What it costs is a program that used to compile.** Two interfaces that
//! both declare `len` and one type implementing both is legal Rust, resolved
//! there by the same ambiguity error and an escape hatch — `<T as I>::len(x)`
//! — that F0 does not have. §5 records that as the one thing Decision 11
//! assumes and the language does not supply.
//!
//! # 4. What the index is *not*
//!
//! - **Not a coherence check.** Two blocks implementing the same interface for
//!   the same type is Decision 12's, and `science-resolve` reports `SC0207`.
//!   This index holds whatever the crate declares and lets the ambiguity rule
//!   answer for the rest.
//! - **Not a bound solver.** Whether `Doc` implements `Summarize` is answered
//!   here by *"the crate contains `Doc implements Summarize:`"* and nothing
//!   deeper — no blanket implementations, no `where` clauses, no supertrait
//!   walk. [`Methods::implements`] is that predicate, and it is what discharges
//!   `assign`'s §3 and §4 obligations.
//! - **Not a receiver check.** Whether a `mutable self` method may be called
//!   on a shared borrow is rule 4's question and
//!   `region-inference.md` owns it. What this index supplies is the
//!   [`SelfKind`], which is all [`crate::narrow`]'s §4 needed.
//!
//! # 5. What Decision 11 assumes and does not supply
//!
//! **A spelling for the disambiguation.** The decision says an ambiguity
//! *"makes them say which they meant"*, and F0 has no syntax for saying it:
//! there is no `<T as I>::m(x)`, no `I.m(x)`, nothing. So the diagnostic can
//! name the two candidates and cannot name the fix, and the honest help is the
//! one it gives — rename one, or drop one of the implementations. A qualified
//! call syntax is a language change and belongs to the note, not here.
//!
//! **A method on a type parameter.** `def largest of T(a: T) -> T` with `T`
//! bounded by an interface should be able to call that interface's methods on
//! `a`. The bound is in `hir::GenericParam` and the machinery is this file's
//! [`Found`] over the bound's interface instead of over the receiver's head —
//! but the *substitution* is not: `Self` would have to become the type
//! parameter and the call is then generic in a way monomorphisation has to
//! resolve, and there is no monomorphiser. Refused here rather than half-done,
//! and [`Methods::receiver`] returns `None` for a parameter so that the call
//! is silent rather than wrong.
//!
//! # 6. One interface at several arguments is selection, not overloading
//!
//! `examples/00_kitchen_sink.science` writes this:
//!
//! ```text
//! LoadError implements From of ParseError:
//!     def from(value: ParseError) -> Self: ..
//!
//! LoadError implements From of IoError:
//!     def from(value: IoError) -> Self: ..
//! ```
//!
//! and then calls `LoadError.from(io_err)`. Two candidates named `from`, on one
//! type, in the [`Form`] the call site used. §3's rule says that is an error,
//! and reporting it would be reporting on a correct program — which is the one
//! thing `examples/README.md` says the corpus exists to prevent.
//!
//! **This is not overloading, and calling it that is what made it look
//! undecidable.** `From`'s `from` is **one** method, declared once on the
//! interface. What differs between the two candidates is not the method but
//! *which instantiation of the interface* the call is in. That is **instance
//! selection**, and it is what `impl From<A> for T` and `impl From<B> for T`
//! are in Rust — nobody calls those an overload set, and nobody reaches for an
//! overload-resolution algorithm to tell them apart.
//!
//! **Decision. Where every candidate is an implementation of the same
//! interface, differing only in the interface's type arguments, the argument
//! types select among them.** That candidate set is [`Found::Instances`], and
//! the selection itself is `BodyChecker::select` in [`crate::check`], because
//! that is where the arguments are. Three outcomes, and each of them is an
//! answer:
//!
//! - **exactly one candidate accepts the arguments** — the call resolves to it,
//!   exactly as [`Found::One`] resolves;
//! - **more than one** — `SC0531`, Decision 11's code under a message that says
//!   the arguments did not narrow it and names what each implementation takes;
//! - **none** — `SC0533`, naming what was supplied and what the implementations
//!   accept.
//!
//! **What this replaces is silence, and silence was worse than either answer.**
//! The previous decision made this set neither resolved nor reported: the call
//! kept `method: None` and [`Ty::ERROR`], so its arguments were never compared
//! against any signature, its result type was whatever the hole produced, and
//! the author was told nothing at all. A wrong answer is arguable; a call that
//! is not checked is not.
//!
//! **This does not widen Decision 11.** Candidates from two *different*
//! interfaces, and an inherent method beside an interface one, are §3's
//! ambiguity unchanged — one of those candidates is not chosen by any argument,
//! so there is nothing for an argument type to select on. `one_interface` is
//! the whole of the condition. Two blocks implementing one interface at the
//! *same* arguments never reach it either: that is Decision 12's coherence,
//! `science-resolve` reports `SC0207`, and the driver does not type-check a
//! file whose resolution errored.
//!
//! **The cost is Decision 1's, and it is stated where it is paid**:
//! `BodyChecker::select` writes out which half of that decision this makes
//! false and which half survives.
//!
//! # 7. A bound is a question about an interface, and half of them are
//! unanswerable
//!
//! [`Methods::implements`] answers *"does the crate contain `T implements
//! I:`"*, and §4 already says what that cannot see. What §4 did not say is
//! **when a `false` from it is evidence and when it is silence**, and a caller
//! that enforces a generic bound needs the difference: `assign`'s two rules ask
//! about one interface each and accepted the answer, but a bound names whatever
//! interface the author wrote.
//!
//! **Decision. [`Methods::answers_for`] is that question, and the line it draws
//! is the same one [`Methods::receiver`] draws: builtin or not.**
//!
//! `builtins.rs` declares seventeen interfaces — `Add`, `Ord`, `Clone`, `Eq`,
//! `Copy`, `Iterate`, `From`, `Display`, `Error` and the rest — and **not one
//! implementation of any of them**. There is no `I64 implements Ord:` anywhere,
//! because the prelude has no bodies to put one in. So `implements(I64, Ord)`
//! is `false` and the program `def largest of T: Ord(..)` called at `I64` is
//! correct; enforcing the bound on that `false` would report on every numeric
//! program in the language. That is exactly [`Methods::receiver`]'s *"a prelude
//! type reaches here and `builtins.rs` registers no methods at all"*, read
//! across from methods to implementations.
//!
//! A **user** interface is the other case and it is answerable in full: the
//! prelude is built before any file is read, so it cannot name `Summarize`, so
//! every `T implements Summarize:` in the program is in this index. `false`
//! there is a fact, and `SC0534` is reported on it.
//!
//! **What it costs is every bound at a prelude interface, which is most of
//! them.** `T: Ord`, `T: Clone`, `T: Eq`, `T: Add` are unchecked and will stay
//! unchecked until the prelude declares its own implementations — which is the
//! same prelude change `check`'s §6 already needs for operators and for
//! `Iterate`, and which is not this crate's to make. The bound that *is*
//! checked is the one Decision 11 was written for, an interface the program
//! declared, and it is the one a program can get wrong without the prelude's
//! help.

use std::collections::HashMap;

use science_resolve::hir::{self, DefId, DefTable, Res, SelfKind};

use crate::items::Declarations;
use crate::ty::{Ty, TyKind, Types};

/// Where a candidate came from — the second half of the ambiguity message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `Doc has:` — an inherent block.
    Inherent,
    /// `Doc implements Summarize:`, carrying the interface.
    Interface(DefId),
}

/// One method the lookup found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// The function definition, which is what
    /// [`ExprKind::MethodCall`](crate::thir::ExprKind::MethodCall) carries and
    /// what [`Declarations::signature`](crate::items::Declarations::signature)
    /// answers on.
    pub method: DefId,
    /// The block that *declares* it: the implementation block for a method
    /// written in one, the interface for a default body inherited from one.
    /// This is the `owner` a [`TyKind::SelfType`] carries, and the one
    /// `Self` in this method's signature means.
    pub owner: DefId,
    /// The block the receiver reached it *through*, which is the owner again
    /// except for an inherited default body. §2: the associated types that
    /// answer a `Self.Item` in the signature belong to this block and not to
    /// the interface that declared the name.
    pub block: DefId,
    pub source: Source,
    /// How the method takes its receiver, or `None` for an associated function
    /// — `DefTable.new()` — which takes none.
    ///
    /// Carried on the candidate rather than read off the signature because
    /// `narrow`'s §4 wants exactly this one bit and nothing else in the
    /// signature.
    pub self_kind: Option<SelfKind>,
}

impl Candidate {
    /// Whether calling this invalidates the receiver's narrowing. Decision 8,
    /// and `narrow`'s §4.
    pub fn writes_receiver(&self) -> bool {
        matches!(self.self_kind, Some(SelfKind::Mutable))
    }

    /// The interface this candidate was reached through, when it was reached
    /// through one. §6's selection names it in both of its messages, and the
    /// name is the only thing in them that is the same for every candidate.
    pub fn interface(&self) -> Option<DefId> {
        match self.source {
            Source::Interface(interface) => Some(interface),
            Source::Inherent => None,
        }
    }

    /// Which kind of call site can reach it.
    pub fn form(&self) -> Form {
        match self.self_kind {
            Some(_) => Form::Value,
            None => Form::Type,
        }
    }
}

/// How the call reached the method: through a value, or through a type.
///
/// **Decision. The form is part of the lookup and not a check after it.**
/// `defs.alloc(..)` wants the methods that take a receiver and
/// `DefTable.new()` wants the ones that do not, and filtering first means the
/// two never make each other ambiguous — a type with an instance `get` and an
/// associated `get` is two unrelated names to a call site, because no call site
/// can reach both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `defs.alloc(..)` — the receiver is a value, so the method takes one.
    Value,
    /// `DefTable.new()` — the receiver is a type, so the method takes no
    /// receiver at all.
    Type,
}

/// What the lookup found. §3 is the second variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// Exactly one candidate.
    One(Candidate),
    /// Decision 11's error: two or more, in declaration order.
    Ambiguous(Vec<Candidate>),
    /// The name is declared on this type and not in the [`Form`] the call site
    /// used — an instance method named through its type, or an associated
    /// function called on a value. §5: F0 spells neither, so this is not a
    /// diagnostic, it is a construct the language has no syntax for.
    Mismatched,
    /// Several candidates, all from implementations of **one** interface at
    /// different generic arguments — `LoadError implements From of IoError:`
    /// and `LoadError implements From of ParseError:`, which
    /// `examples/00_kitchen_sink.science` writes side by side.
    ///
    /// **Not an ambiguity and not an answer either**: it is the candidate set
    /// the *argument types* choose from, in declaration order. §6, and
    /// `BodyChecker::select` in [`crate::check`] is what chooses.
    Instances(Vec<Candidate>),
    /// The receiver is a type this index can speak for, and it has no such
    /// method. This one is a diagnostic.
    None,
}

/// One entry in the index.
#[derive(Debug, Clone)]
struct Entry {
    name: String,
    candidate: Candidate,
}

/// Every method in the crate, by the definition its receiver's type heads. §1.
#[derive(Debug, Clone, Default)]
pub struct Methods {
    index: HashMap<DefId, Vec<Entry>>,
    /// `(type, interface)` for every `T implements I:` in the crate. §4.
    implemented: Vec<(DefId, DefId)>,
}

impl Methods {
    /// Builds the index, given the self types the declaration pass already
    /// lowered.
    ///
    /// Takes the lowered types rather than lowering them again, because
    /// lowering an annotation twice reports its mistakes twice — which is the
    /// whole argument `items`'s §1 makes for the declaration table existing.
    pub(crate) fn of(krate: &hir::Crate, types: &Types, decls: &Declarations) -> Methods {
        let mut methods = Methods::default();
        for module in &krate.modules {
            for item in &module.items {
                match &item.kind {
                    hir::ItemKind::Impl(block) => methods.impl_block(block, krate, types, decls),
                    hir::ItemKind::Interface(interface) => methods.interface(interface, krate),
                    _ => {}
                }
            }
        }
        methods
    }

    /// The definition a receiver's type hangs its methods off, when this index
    /// can speak for it at all. §1.
    ///
    /// **A borrow is transparent**, exactly as it is to a field read: `borrowed
    /// Doc` has `Doc`'s methods because F0 has no explicit dereference to write
    /// instead, and the receiver's form is the signature's question rather than
    /// the lookup's.
    ///
    /// **`None` is *"this index cannot say"*, not *"there is no such
    /// method"***, and the two must not be confused: a prelude type reaches
    /// here and `builtins.rs` registers no methods at all, so answering *"`String`
    /// has no method `len`"* would be a false positive on every correct program
    /// that calls one. The callers of [`Found::None`] report; the callers of
    /// `None` stay silent.
    pub fn receiver(&self, defs: &DefTable, types: &Types, ty: Ty) -> Option<DefId> {
        let def = receiver_head(types, ty)?;
        // §5: the prelude registers no methods, so a builtin head is a question
        // this index cannot answer rather than one it answers with no.
        if defs.get(def).is_builtin() {
            return None;
        }
        Some(def)
    }

    /// Decision 11's lookup, over the key [`Methods::receiver`] computed.
    pub fn lookup(&self, receiver: DefId, name: &str, form: Form) -> Found {
        let Some(entries) = self.index.get(&receiver) else {
            return Found::None;
        };
        let named: Vec<Candidate> = entries
            .iter()
            .filter(|entry| entry.name == name)
            .map(|entry| entry.candidate)
            .collect();
        let mut found: Vec<Candidate> =
            named.iter().copied().filter(|candidate| candidate.form() == form).collect();
        match found.len() {
            0 if named.is_empty() => Found::None,
            0 => Found::Mismatched,
            1 => Found::One(found.remove(0)),
            // §6, and it comes before §3's rule because it is not the thing
            // §3 is about: these candidates are one method at several
            // instantiations, and the arguments tell them apart.
            _ if one_interface(&found) => Found::Instances(found),
            _ => Found::Ambiguous(found),
        }
    }

    /// Whether the crate declares `T implements I:`. §4.
    ///
    /// This is the predicate `assign`'s §3 and §4 obligations were written
    /// against — *"does `S` implement `Error`"*, and its wider sibling *"does
    /// `C` implement `I`"* — and it answers them the only way a crate-local
    /// index can: by the implementation being written down. A blanket
    /// implementation, a bound on a type parameter and a supertrait are all
    /// three *not* looked through, and each of them is a legal program this
    /// predicate says no to. That is the cost of discharging the obligation at
    /// all, and it is the direction that refuses rather than admits.
    pub fn implements(&self, types: &Types, ty: Ty, interface: DefId) -> bool {
        let Some(head) = head(types, ty) else {
            // A type parameter, `Self`, an unresolved associated type: nothing
            // here can decide, and refusing would report on a correct program.
            return true;
        };
        // An interface object at that interface already is one.
        if head == interface {
            return true;
        }
        self.implemented
            .iter()
            .any(|(implementor, iface)| *implementor == head && *iface == interface)
    }

    /// Whether this index can speak for implementations of an interface at
    /// all — §7.
    ///
    /// **A builtin interface is one it cannot.** `builtins.rs` declares
    /// seventeen of them and zero implementations of any, so *"the crate does
    /// not contain `Doc implements Clone:`"* is silence rather than a no: the
    /// prelude is where `I64 implements Ord:` would live and the prelude
    /// declares nothing. This is [`Methods::receiver`]'s builtin refusal one
    /// level up, made for the same reason and quoting the same sentence —
    /// *"the prelude registers no methods at all"*.
    ///
    /// **A user interface is one it can.** The prelude is built before any file
    /// is read and cannot mention a name a file declares, so every
    /// implementation of a user interface is written in the crate, and this
    /// index has all of them.
    ///
    /// The argument takes `&self` although it reads no field: what it answers
    /// is *"is this index's `false` evidence"*, which is a fact about the
    /// index, and a caller that has one in hand should ask it rather than
    /// reconstruct the rule.
    pub fn answers_for(&self, defs: &DefTable, interface: DefId) -> bool {
        !defs.get(interface).is_builtin()
    }

    fn impl_block(
        &mut self,
        block: &hir::Impl,
        krate: &hir::Crate,
        types: &Types,
        decls: &Declarations,
    ) {
        let Some(ty) = decls.self_ty(block.def) else { return };
        let Some(head) = head(types, ty) else { return };
        let interface = block
            .interface
            .as_ref()
            .and_then(|bound| bound.interface_res())
            .and_then(|res| match res {
                Res::Def(def) => Some(def),
                _ => None,
            });
        let source = match interface {
            Some(interface) => Source::Interface(interface),
            None => Source::Inherent,
        };
        if let Some(interface) = interface {
            self.implemented.push((head, interface));
        }
        for method in &block.methods {
            self.push(head, krate, method, block.def, block.def, source);
        }
        // §2's second half: the interface's own methods that this block did not
        // write. Matched by name, because that is the relation between a
        // declaration and the implementation that answers it.
        let Some(interface) = interface else { return };
        for declared in interface_methods(krate, interface) {
            let name = &krate.defs.get(declared.def).name;
            if block.methods.iter().any(|m| krate.defs.get(m.def).name == *name) {
                continue;
            }
            self.push(head, krate, declared, interface, block.def, source);
        }
    }

    fn interface(&mut self, interface: &hir::Interface, krate: &hir::Crate) {
        for method in &interface.methods {
            self.push(
                interface.def,
                krate,
                method,
                interface.def,
                interface.def,
                Source::Interface(interface.def),
            );
        }
    }

    fn push(
        &mut self,
        head: DefId,
        krate: &hir::Crate,
        method: &hir::Fn,
        owner: DefId,
        block: DefId,
        source: Source,
    ) {
        let candidate = Candidate {
            method: method.def,
            owner,
            block,
            source,
            self_kind: method.self_param.as_ref().map(|param| param.kind),
        };
        let name = krate.defs.get(method.def).name.clone();
        self.index.entry(head).or_default().push(Entry { name, candidate });
    }
}

/// The head of a type, whether or not this index can speak for it.
///
/// [`Methods::receiver`] is [`receiver_head`] plus the builtin refusal; the
/// obligation predicate wants the head of a prelude type too, because *"does
/// `Int` implement `Error`"* has an answer and it is no.
///
/// **A borrow is transparent** and everything else is not: a tuple, a closure,
/// a type parameter, `Self.Item`. `None` from here is *"no definition to hang a
/// method off"*, and each caller decides what that means.
fn head(types: &Types, ty: Ty) -> Option<DefId> {
    match types.kind(ty) {
        TyKind::Named { def, .. } => Some(*def),
        TyKind::Object { interface, .. } => Some(*interface),
        TyKind::Borrowed { inner, .. } => head(types, *inner),
        _ => None,
    }
}

/// [`head`], plus the one case that is a head for a *lookup* and not for the
/// implements question: inside an interface's own default body, `Self` is
/// itself and the methods in scope are that interface's.
fn receiver_head(types: &Types, ty: Ty) -> Option<DefId> {
    match types.kind(ty) {
        TyKind::SelfType { owner } => Some(*owner),
        TyKind::Borrowed { inner, .. } => receiver_head(types, *inner),
        _ => head(types, ty),
    }
}

/// Whether every candidate comes from an implementation of the same interface.
///
/// §6's condition. An inherent block among them makes it false — `Doc has: def
/// from` beside `Doc implements From of X:` is a real ambiguity, because one of
/// the two is not selected by any argument.
fn one_interface(found: &[Candidate]) -> bool {
    let Some(Source::Interface(first)) = found.first().map(|candidate| candidate.source) else {
        return false;
    };
    found.iter().all(|candidate| candidate.source == Source::Interface(first))
        && found.len() > 1
}

/// The methods an interface declares, found through the crate's items.
///
/// A scan rather than a map: an interface is named once per implementation
/// block, the crate's item list is short, and a second index would be a second
/// thing to keep in step with the first.
fn interface_methods(krate: &hir::Crate, interface: DefId) -> &[hir::Fn] {
    for module in &krate.modules {
        for item in &module.items {
            if let hir::ItemKind::Interface(declared) = &item.kind {
                if declared.def == interface {
                    return &declared.methods;
                }
            }
        }
    }
    &[]
}

/// How a candidate's source reads in a diagnostic. §3.
///
/// `Doc has:` and `Doc implements Summarize:` are the two spellings the author
/// wrote, so they are the two spellings the message uses — a message that said
/// *"an inherent implementation"* would be describing the language back to
/// someone who is looking at their own file.
pub fn describe_source(
    defs: &DefTable,
    types: &Types,
    receiver: Ty,
    candidate: &Candidate,
) -> String {
    let ty = types.render(defs, receiver);
    match candidate.source {
        Source::Inherent => format!("`{ty} has:`"),
        // An interface reached as its own `Self` — a default body calling a
        // sibling — is not written as an implementation of itself.
        Source::Interface(interface) if candidate.block == interface => {
            format!("`interface {}:`", defs.get(interface).name)
        }
        Source::Interface(interface) => {
            format!("`{ty} implements {}:`", defs.get(interface).name)
        }
    }
}
