//! The declarations a body is checked against, lowered once.
//!
//! # 1. Why this exists at all
//!
//! Decision 1's whole argument is that *"every call site knows its callee's
//! types before it looks at the arguments"*. Knowing them means having lowered
//! them, and lowering them at each call site would do three wrong things: it
//! would lower one annotation once per caller, it would report `SC0520` and
//! `SC0523` once per caller for a mistake written once, and it would give the
//! checker a reason to hold the callee's `hir::Fn` while it holds the caller's
//! — which is the borrow `ty`'s §2 spends a clone to avoid everywhere else.
//!
//! **Decision. Every signature, every record's fields and every variant's
//! payload is lowered once, before any body is checked, and a body reads types
//! out of this table.** The three calls of `lib.rs` §5 happen here for a
//! declaration — `lower`, and then `reveal` at the comparison — and a body that
//! wants a parameter's type asks rather than re-lowers.
//!
//! **What it costs** is one walk of every item before the first body, which is
//! the same walk [`Aliases::of`](crate::alias::Aliases::of) already does, and a
//! table proportional to the program rather than to the body. And one real
//! restriction: a signature that mentions an alias declared *later* in the file
//! is fine, because [`Aliases`](crate::alias::Aliases) was built first, but a
//! signature cannot depend on anything a *body* computes. Nothing in the
//! language lets it.
//!
//! # 2. The prelude ids, found by name, once
//!
//! [`Prelude`] is the same compromise [`Coercions`](crate::assign::Coercions)
//! already made and for the same reason, quoted from its own documentation:
//! *"the prelude does not export its ids"*, so the only way to find `Bool` is a
//! builtin definition of primitive kind named `Bool`. Decision 2 needs `I64`
//! and `F64` by name — `lib.rs` §5 says so in as many words — and the
//! divergence half of `SC0140`'s fourth exclusion needs `Never` and `panic`.
//!
//! **What it costs is a scan of the definition table**, performed once, and a
//! dependency on five names in `builtins.rs` that nothing checks. The honest
//! fix is the one `assign`'s §3 already asked for: the resolver publishing the
//! handful of prelude ids later phases need. This crate should not make that
//! decision by reaching into `builtins`.
//!
//! # 3. What is not in here, and is not an oversight
//!
//! - **Methods.** `builtins.rs` is explicit that the prelude registers none,
//!   and a user's `Doc has:` block does register them — but an *index from a
//!   receiver type to a method* is Decision 11's lookup, which
//!   `type-checking-and-mir.md` §6.1 owns and which nothing in this crate
//!   builds. The methods are walked here for their bodies and for nothing else.
//! - **Arity and kind checking of generic arguments.** `lowering`'s §1 defers
//!   it to *"whoever holds the declaration and the use at once"*, which is this
//!   table plus a call site. It is still deferred: [`Signature::generics`] is
//!   kept so that the check has something to run against, and the check is not
//!   written. [`crate::check`]'s §5 prices what that costs at a call.
//! - **Coherence and the orphan rule.** Decision 12 is a package-level rule and
//!   `science-resolve` already reports `SC0207`.

use std::collections::HashMap;

use science_diagnostics::{Diagnostics, Span};
use science_resolve::hir::{self, DefId, DefKind, DefTable, Res, SelfKind};

use crate::lowering::TypeLowerer;
use crate::normal::AtomOrder;
use crate::subst::Substitution;
use crate::ty::{Ty, TyKind, Types};

/// One parameter of a declared function.
#[derive(Debug, Clone)]
pub struct Param {
    pub def: DefId,
    pub ty: Ty,
    pub span: Span,
}

/// A function's signature, lowered.
#[derive(Debug, Clone)]
pub struct Signature {
    pub def: DefId,
    /// The generic parameters as declared, for the arity and kind check §3
    /// still defers and for [`Substitution::of_generics`](crate::Substitution::of_generics).
    pub generics: Vec<hir::GenericParam>,
    /// `Some` for a method. The receiver's type is `Self`, borrowed or not
    /// according to the kind.
    pub self_param: Option<(DefId, SelfKind)>,
    pub params: Vec<Param>,
    /// `None` in the HIR means unit (§4.4), and it is [`Ty::UNIT`] here: a
    /// signature with a hole in it is a signature every caller has to branch
    /// on.
    pub ret: Ty,
    /// The implementation or interface block this method belongs to, which is
    /// the `owner` a [`TyKind::SelfType`] carries and the key
    /// [`Declarations::self_ty`] answers on.
    pub owner: Option<DefId>,
    pub has_body: bool,
    pub span: Span,
}

impl Signature {
    /// Whether the function can return to its caller at all.
    ///
    /// `SC0140`'s fourth exclusion, the half that is a property of the
    /// declaration: *"a binding in a function that cannot return (`-> Never`)"*.
    /// The other half — *"or that panics on every path"* — is a property of the
    /// body and [`crate::unchecked`]'s §3 computes it there.
    pub fn can_return(&self, prelude: &Prelude, types: &Types) -> bool {
        !prelude.is_never(types, self.ret)
    }
}

/// A record's fields, lowered, in declaration order.
#[derive(Debug, Clone)]
pub struct Record {
    pub def: DefId,
    pub generics: Vec<hir::GenericParam>,
    pub fields: Vec<(DefId, Ty)>,
}

/// One variant of a choice type, with its positional payload.
#[derive(Debug, Clone)]
pub struct Variant {
    pub def: DefId,
    pub choice: DefId,
    /// The choice type's own generic parameters, carried on the variant
    /// because a use writes `Left(1)` and never names the type — so the
    /// declaration a call site needs is reached through the variant or not at
    /// all.
    pub generics: Vec<hir::GenericParam>,
    pub payload: Vec<Ty>,
}

/// The prelude ids this phase needs by name. §2.
#[derive(Debug, Clone, Default)]
pub struct Prelude {
    names: HashMap<&'static str, DefId>,
}

/// The names §2 justifies looking up, and no others.
///
/// A closed list rather than a general lookup, so that adding a dependency on a
/// prelude name is an edit to this array and shows up in a diff.
const WANTED: &[&str] = &[
    "Bool", "String", "Char", "Never", "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64",
    "F16", "BF16", "F32", "F64", "Int", "Float", "panic",
];

impl Prelude {
    /// Finds them. A table built by hand has none, and every predicate below
    /// then answers `false`, which is right for a table with no prelude in it.
    pub fn of(defs: &DefTable) -> Prelude {
        let mut names = HashMap::new();
        for def in defs.iter() {
            if !def.is_builtin() {
                continue;
            }
            if let Some(wanted) = WANTED.iter().find(|name| **name == def.name) {
                names.entry(*wanted).or_insert(def.id);
            }
        }
        Prelude { names }
    }

    /// The definition of a prelude name, if this compilation has a prelude.
    pub fn get(&self, name: &str) -> Option<DefId> {
        self.names.get(name).copied()
    }

    /// Whether this compilation has a prelude at all.
    ///
    /// A hand-built `DefTable` — every test in this crate that does not go
    /// through the resolver — has none, and a checker that answered *"`1` is
    /// not a `Bool`"* by finding neither name would report on a question it
    /// could not ask. Every judgement in [`crate::check`] that would be a
    /// refusal is turned into an admission by this predicate.
    pub fn is_available(&self) -> bool {
        self.names.contains_key("Bool")
    }

    /// The type of a prelude primitive, interned.
    pub fn ty(&self, types: &mut Types, name: &str) -> Option<Ty> {
        let def = self.get(name)?;
        Some(types.named(def, Vec::new()))
    }

    /// Decision 2's two defaults. `lib.rs` §5 names them.
    pub fn default_int(&self, types: &mut Types) -> Option<Ty> {
        self.ty(types, "I64")
    }

    pub fn default_float(&self, types: &mut Types) -> Option<Ty> {
        self.ty(types, "F64")
    }

    /// Whether a type is this prelude primitive.
    pub fn is(&self, types: &Types, ty: Ty, name: &str) -> bool {
        let Some(def) = self.get(name) else {
            return false;
        };
        matches!(types.kind(ty), TyKind::Named { def: named, args } if *named == def && args.is_empty())
    }

    pub fn is_never(&self, types: &Types, ty: Ty) -> bool {
        self.is(types, ty, "Never")
    }

    pub fn is_bool(&self, types: &Types, ty: Ty) -> bool {
        self.is(types, ty, "Bool")
    }

    /// Whether a type is one of §5.1's integer primitives.
    pub fn is_integer(&self, types: &Types, ty: Ty) -> bool {
        ["I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "Int"]
            .iter()
            .any(|name| self.is(types, ty, name))
    }

    /// Whether a type is one of §5.1's floating primitives.
    pub fn is_float(&self, types: &Types, ty: Ty) -> bool {
        ["F16", "BF16", "F32", "F64", "Float"].iter().any(|name| self.is(types, ty, name))
    }

    pub fn is_numeric(&self, types: &Types, ty: Ty) -> bool {
        self.is_integer(types, ty) || self.is_float(types, ty)
    }

    /// Whether this is a prelude type that a number is definitely not.
    ///
    /// The **refusal** side of [`crate::check`]'s §5, and it is deliberately a
    /// short closed list rather than the complement of [`Prelude::is_numeric`].
    /// A `Doc` is not numeric either, but saying so needs to know that no
    /// implementation of `From of Int` exists — which is Decision 11's lookup —
    /// and `ffi.CInt` is numeric although §5.1 does not list it, because
    /// `ffi-c-boundary.md` §1.6 keeps the C widths as *distinct* types rather
    /// than as non-numbers. Naming the four the compiler is sure about is the
    /// answer that cannot be wrong in the direction that matters.
    pub fn is_definitely_not_numeric(&self, types: &Types, ty: Ty) -> bool {
        ["Bool", "String", "Char", "Never"].iter().any(|name| self.is(types, ty, name))
    }

    /// `panic`, which is how a body diverges without a `return`. §2.
    pub fn panic(&self) -> Option<DefId> {
        self.get("panic")
    }
}

/// Every declaration a body can be checked against.
#[derive(Debug, Clone, Default)]
pub struct Declarations {
    fns: HashMap<DefId, Signature>,
    records: HashMap<DefId, Record>,
    variants: HashMap<DefId, Variant>,
    consts: HashMap<DefId, Ty>,
    /// An implementation or interface block's `Self`. §1.
    self_types: HashMap<DefId, Ty>,
    /// `type Item is Int` in a block, by the block that wrote it: the name, the
    /// definition it declares, and the type it answers with. §4.
    block_assocs: HashMap<DefId, Vec<(String, DefId, Ty)>>,
    /// The interface an implementation block implements, when it names one.
    implemented: HashMap<DefId, DefId>,
    prelude: Prelude,
}

impl Declarations {
    /// Lowers every declaration in the crate.
    ///
    /// Reports whatever [`TypeLowerer`] reports about an annotation, exactly
    /// once — which is the reason §1 gives for the table existing.
    pub fn of(
        krate: &hir::Crate,
        types: &mut Types,
        order: &AtomOrder,
        diagnostics: &mut Diagnostics,
    ) -> Declarations {
        let mut decls = Declarations { prelude: Prelude::of(&krate.defs), ..Declarations::default() };
        for module in &krate.modules {
            for item in &module.items {
                decls.item(&item.kind, krate, types, order, diagnostics);
            }
        }
        decls
    }

    /// The prelude ids. §2.
    pub fn prelude(&self) -> &Prelude {
        &self.prelude
    }

    pub fn signature(&self, def: DefId) -> Option<&Signature> {
        self.fns.get(&def)
    }

    pub fn record(&self, def: DefId) -> Option<&Record> {
        self.records.get(&def)
    }

    pub fn variant(&self, def: DefId) -> Option<&Variant> {
        self.variants.get(&def)
    }

    pub fn const_ty(&self, def: DefId) -> Option<Ty> {
        self.consts.get(&def).copied()
    }

    /// What `Self` means inside a block. `lib.rs` §5: *"the `owner` a
    /// `SelfType` already carries"*.
    pub fn self_ty(&self, owner: DefId) -> Option<Ty> {
        self.self_types.get(&owner).copied()
    }

    /// The substitution a method body inside `owner` is checked under. §4.
    ///
    /// **`Self`, and the associated types the block answers.** `subst`'s §2
    /// leaves a `Self.Item` standing *"precisely so that the phase which can see
    /// both blocks reports it"*, and a method body is that phase: the
    /// implementation writes `type Item is Int` and the interface declared
    /// `Item`, so both names are in hand here and in no earlier one.
    ///
    /// **Both spellings of the name are bound**, and that is the part that is
    /// not obvious. Inside `Doc implements Iterate:`, a `Self.Item` in the
    /// method's own signature resolves to the *implementation's* `Item`; the
    /// same `Self.Item` inherited from the interface's declaration resolves to
    /// the *interface's*. They are two `DefId`s for one type, matched by name,
    /// and binding only one of them leaves half the signatures unsubstituted —
    /// which reads as `expected Self.Item?, found Int` at a site where nothing
    /// is wrong.
    ///
    /// **What it costs** is that the matching is by name. Two associated types
    /// whose names agree and whose meanings do not cannot arise — an interface
    /// declares each name once and an implementation answers it once — but the
    /// rule is a string comparison, which is the lookup the HIR exists to
    /// abolish, performed once per body.
    pub fn body_substitution(&self, defs: &DefTable, owner: DefId) -> Substitution {
        let mut substitution = match self.self_ty(owner) {
            Some(ty) => Substitution::new().with_self(owner, ty),
            None => Substitution::new(),
        };
        let Some(assocs) = self.block_assocs.get(&owner) else {
            return substitution;
        };
        let interface = self.implemented.get(&owner).copied();
        for (name, def, ty) in assocs {
            substitution = substitution.with_assoc(*def, *ty);
            let Some(interface) = interface else { continue };
            for child in defs.children(interface) {
                if child.kind == DefKind::AssocType && child.name == *name {
                    substitution = substitution.with_assoc(child.id, *ty);
                }
            }
        }
        substitution
    }

    /// Every function with a body, in declaration order.
    ///
    /// Declaration order because Decision 1's second dividend is that *"bodies
    /// check in parallel and in any order"* — which makes the order free to
    /// choose, and a deterministic one is what makes a diagnostic list the same
    /// twice.
    pub fn bodies(&self) -> Vec<DefId> {
        let mut ids: Vec<DefId> =
            self.fns.values().filter(|sig| sig.has_body).map(|sig| sig.def).collect();
        ids.sort();
        ids
    }

    fn item(
        &mut self,
        kind: &hir::ItemKind,
        krate: &hir::Crate,
        types: &mut Types,
        order: &AtomOrder,
        diagnostics: &mut Diagnostics,
    ) {
        match kind {
            hir::ItemKind::Fn(function) => {
                let sig = self.lower_fn(function, None, krate, types, order, diagnostics);
                self.fns.insert(function.def, sig);
            }
            hir::ItemKind::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|field| {
                        (field.def, lower(types, krate, order, diagnostics, &field.ty))
                    })
                    .collect();
                self.records.insert(
                    record.def,
                    Record { def: record.def, generics: record.generics.clone(), fields },
                );
            }
            hir::ItemKind::Choice(choice) => {
                for variant in &choice.variants {
                    let payload = variant
                        .payload
                        .iter()
                        .map(|ty| lower(types, krate, order, diagnostics, ty))
                        .collect();
                    self.variants.insert(
                        variant.def,
                        Variant {
                            def: variant.def,
                            choice: choice.def,
                            generics: choice.generics.clone(),
                            payload,
                        },
                    );
                }
            }
            hir::ItemKind::Const(konst) => {
                // A constant's annotation is optional; with none, its type is
                // its value's and the body walk is what learns it. `Ty::ERROR`
                // until then, which is compatible with everything and reports
                // nothing.
                let ty = match &konst.ty {
                    Some(ty) => lower(types, krate, order, diagnostics, ty),
                    None => Ty::ERROR,
                };
                self.consts.insert(konst.def, ty);
            }
            hir::ItemKind::Impl(block) => {
                let self_ty = lower(types, krate, order, diagnostics, &block.self_ty);
                self.self_types.insert(block.def, self_ty);
                if let Some(Res::Def(interface)) =
                    block.interface.as_ref().and_then(|bound| bound.interface_res())
                {
                    self.implemented.insert(block.def, interface);
                }
                let assocs = block
                    .assoc_types
                    .iter()
                    .filter_map(|assoc| {
                        let ty = assoc.ty.as_ref()?;
                        let ty = lower(types, krate, order, diagnostics, ty);
                        Some((krate.defs.get(assoc.def).name.clone(), assoc.def, ty))
                    })
                    .collect();
                self.block_assocs.insert(block.def, assocs);
                for method in &block.methods {
                    let sig =
                        self.lower_fn(method, Some(block.def), krate, types, order, diagnostics);
                    self.fns.insert(method.def, sig);
                }
            }
            hir::ItemKind::Interface(interface) => {
                // `Self` inside an interface is itself: the block is the owner
                // and there is no concrete type to substitute. Recording the
                // `SelfType` as its own meaning is what makes a default method
                // body check without inventing a receiver.
                let self_ty = types.self_type(interface.def);
                self.self_types.insert(interface.def, self_ty);
                for method in &interface.methods {
                    let sig = self.lower_fn(
                        method,
                        Some(interface.def),
                        krate,
                        types,
                        order,
                        diagnostics,
                    );
                    self.fns.insert(method.def, sig);
                }
            }
            hir::ItemKind::Extern(block) => {
                for item in &block.items {
                    if let hir::ExternItemKind::Fn(function) = &item.kind {
                        let params = function
                            .params
                            .iter()
                            .map(|param| Param {
                                def: param.def,
                                ty: lower(types, krate, order, diagnostics, &param.ty),
                                span: param.span,
                            })
                            .collect();
                        let ret = match &function.ret {
                            Some(ty) => lower(types, krate, order, diagnostics, ty),
                            None => Ty::UNIT,
                        };
                        self.fns.insert(
                            function.def,
                            Signature {
                                def: function.def,
                                generics: Vec::new(),
                                self_param: None,
                                params,
                                ret,
                                owner: None,
                                has_body: false,
                                span: function.span,
                            },
                        );
                    }
                }
            }
            hir::ItemKind::Alias(_) => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_fn(
        &mut self,
        function: &hir::Fn,
        owner: Option<DefId>,
        krate: &hir::Crate,
        types: &mut Types,
        order: &AtomOrder,
        diagnostics: &mut Diagnostics,
    ) -> Signature {
        let params = function
            .params
            .iter()
            .map(|param| Param {
                def: param.def,
                ty: lower(types, krate, order, diagnostics, &param.ty),
                span: param.span,
            })
            .collect();
        let ret = match &function.ret {
            Some(ty) => lower(types, krate, order, diagnostics, ty),
            None => Ty::UNIT,
        };
        Signature {
            def: function.def,
            generics: function.generics.clone(),
            self_param: function.self_param.as_ref().map(|s| (s.def, s.kind)),
            params,
            ret,
            owner,
            has_body: function.body.is_some(),
            span: function.span,
        }
    }
}

/// One annotation, lowered. The first of `lib.rs` §5's three calls.
fn lower(
    types: &mut Types,
    krate: &hir::Crate,
    order: &AtomOrder,
    diagnostics: &mut Diagnostics,
    ty: &hir::Type,
) -> Ty {
    TypeLowerer::new(types, &krate.defs, order, diagnostics).lower(ty)
}

/// The kind of thing a resolved name in an expression turned out to be.
///
/// The resolver hands over a [`Res`] and the [`DefKind`] behind it; this is the
/// same question asked in the vocabulary a *value* position cares about, so
/// that the checker's path arm is a match on four cases rather than a chain of
/// `if kind ==`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// A binding: a `let`, a parameter, a closure subject, a pattern binding.
    Local(DefId),
    /// A function or a foreign function, named as a value or called.
    Function(DefId),
    /// A unit variant, or a variant applied to a payload.
    Variant(DefId),
    /// A module-level constant.
    Const(DefId),
    /// A type used where a value is expected, or anything else. The checker
    /// says nothing about it: either the resolver already did, or the
    /// construct is one this phase does not answer.
    Other,
}

/// What a resolved path in a value position names.
pub fn named(defs: &DefTable, res: Res) -> Option<Named> {
    let def = match res {
        Res::Def(def) => def,
        Res::SelfTy(_) | Res::Error => return None,
    };
    Some(match defs.get(def).kind {
        DefKind::Local | DefKind::Param | DefKind::SelfParam => Named::Local(def),
        DefKind::Fn | DefKind::ExternFn => Named::Function(def),
        DefKind::Variant => Named::Variant(def),
        DefKind::Const => Named::Const(def),
        _ => Named::Other,
    })
}
