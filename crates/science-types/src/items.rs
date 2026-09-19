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
//! **The one narrow exception is an unannotated `const`'s own type**, which
//! §4.4 defines as its initialiser's — and the initialiser is not a body in
//! this argument's sense, because [`crate::constant`] restricts it to a bare
//! literal and a literal needs no declaration to type. `crate::constant`'s
//! module doc §2 is the fuller version of this paragraph.
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
//! - **The method index itself.** It is [`crate::methods`] and not this
//!   module, although this table is what builds it and hands it out
//!   ([`Declarations::methods`]). The split is that this table answers *"what
//!   does this declaration say"* and that one answers *"which declaration does
//!   this receiver reach"*, and the second question needs the first one
//!   answered first: the index is keyed on the head of a **lowered** self type,
//!   so it cannot be built until every `Doc has:` block's `Doc` has been
//!   lowered once — which is §1's whole argument, one level up.
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
use crate::methods::Methods;
use crate::normal::AtomOrder;
use crate::subst::Substitution;
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// One parameter of a declared function.
#[derive(Debug, Clone)]
pub struct Param {
    pub def: DefId,
    pub ty: Ty,
    pub span: Span,
}

/// One obligation a call site owes: a generic parameter of the callee, an
/// interface it was declared to satisfy, and the span of the bound as written.
///
/// **The two spellings are one list.** `def describe of T: Summarize(..)` and
/// `def describe of T(..) where T: Summarize` say the same thing —
/// `examples/07_generics.science` writes both and comments that the second
/// *"keeps a long signature readable"* — so a caller that read only
/// [`Signature::generics`] would enforce the first and not the second, which is
/// a rule that depends on where the author put it.
///
/// **What it costs is the two bound shapes that are not an interface.** A
/// closure bound — `where F: (A) -> B` — names no definition
/// ([`hir::Bound::interface_res`] is `None` for it) and is not here; whether a
/// given `F` has that shape is a structural question and nothing asks it yet.
/// And a `where` predicate whose subject is not a bare parameter — `where
/// Array of T: Ord`, `where Self.Item: Ord` — is not here either: its subject
/// is a type this list is not keyed on, and admitting it would mean carrying a
/// [`Ty`] per predicate and solving it at each call.
#[derive(Debug, Clone, Copy)]
pub struct ParamBound {
    /// The generic parameter the bound is on.
    pub param: DefId,
    /// The interface it must implement.
    pub interface: DefId,
    /// The bound as written, which is what a diagnostic points its secondary
    /// label at — the declaration is what the call site is being held to.
    pub span: Span,
}

/// A function's signature, lowered.
#[derive(Debug, Clone)]
pub struct Signature {
    pub def: DefId,
    /// The generic parameters as declared, for the arity and kind check §3
    /// still defers and for [`Substitution::of_generics`](crate::Substitution::of_generics).
    pub generics: Vec<hir::GenericParam>,
    /// What each of those parameters must implement, from the parameter list
    /// and the `where` clause together. [`ParamBound`] says what is and is not
    /// in it; [`crate::check`]'s §8 is what enforces it.
    pub bounds: Vec<ParamBound>,
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
    // `Iterate`, which `crate::check`'s `for` reads an element type through.
    // It is the first *interface* on this list, and it is here for the same
    // reason the rest are: the question *"is this the prelude's `Iterate` and
    // not a user interface of the same name"* has no other way to be asked.
    "Iterate",
    // The operator interfaces of §5.4, which `crate::check`'s §6 dispatches
    // `+ - * / % ** @ is` and `[` through. Each is here for `Iterate`'s reason
    // and no other: a user may declare an `interface Add:` of their own, and
    // the only way to ask whether the one a block implements is the prelude's
    // is to hold the prelude's id. The *names of the methods* are not here —
    // they are `check`'s `OPERATORS`, beside the operators they belong to.
    "Add", "Sub", "Mul", "Div", "Rem", "Pow", "MatMul", "Neg", "Eq", "Ord", "Index",
    // `IndexMutably`, which is here for `Index`'s reason and is the interface
    // `a[i] be v` dispatches to. Leaving it off this list is not a silence with
    // a message — `check`'s `index_expr` finds no interface, types the write at
    // `Ty::ERROR`, and `ty`'s §5 then makes the slot agree with whatever was
    // assigned into it.
    "IndexMutably",
    // `Display`, which `check`'s `fstring` asks about for every hole of an
    // `f"…"`. It is here for `Iterate`'s reason — a user may declare an
    // `interface Display:` of their own, and holding the prelude's id is the
    // only way to tell the two apart — and for one more that the operator
    // interfaces above do not have: it is the first name on this list that is
    // asked about **without an operator to hang the question on**. `+` reaches
    // `Add` through a token; an interpolation reaches `Display` through a
    // literal, so the name has to be looked up by itself.
    //
    // It buys the *relation* and not a method. `builtins.rs` declares `Display`
    // with no methods, three times over, because writing
    // `display(Formatter)` would invent a `Formatter` no note specifies; that
    // refusal is untouched by this line, which asks only whether a type has an
    // `implements Display:` block.
    "Display",
    // `print` and `write`, which `check`'s `call` refuses to give more than one
    // argument. They are here for `panic`'s reason and not for `Display`'s: the
    // question is *"is this call the prelude's `print`"*, and a user is free to
    // write a `def print(a, b)` of their own in a module, which must not be
    // refused for the shape of its own declaration.
    //
    // **They buy the arity and nothing else.** `builtins.rs` leaves both
    // undeclared — its `FUNCTION_SIGNATURES` note is the measurement — so there
    // is no `Signature` behind either id and no parameter type to check
    // against. §4.1's decision that `print` is unary is a fact about the call
    // rather than about the value, and it is the half of that decision the
    // phases below this one do not have to move for.
    "print", "write",
    // `Array`, which `check`'s `array_lit` builds. It is the first *type
    // constructor* on this list and the first entry here for a reason that is
    // not "tell it from a user's declaration of the same name": Decision 10 of
    // `indexing-and-array-literals.md` says `[e, e]` is an `Array of T`
    // **always**, so the checker has to be able to *name* `Array`, not merely
    // recognise it. Without this line the literal has no head to hang its
    // element type on and §3.1's "always" has nothing to be about.
    //
    // A compilation with no prelude answers `None` and `array_lit` then types
    // the literal at `Ty::ERROR` — the same admission every other judgement in
    // `check` makes through `Prelude::is_available`, for the same reason.
    "Array",
    // `Range`, which `check`'s `range_expr` builds, and it is here for
    // `Array`'s reason exactly: `a..b` has to be *named* at a type, not merely
    // recognised at one. `builtins.rs` declares `Range of T implements
    // Iterate: type Item is T`, so this id is what makes `for i in 0..n:` bind
    // `i` through the same `iterate_item` reading every other `for` goes
    // through, instead of at `Ty::ERROR`.
    //
    // Recognising a user's own `Range` as the prelude's would be the worse
    // half of the same mistake `Iterate` is on this list to avoid, and holding
    // the id answers both questions with one lookup.
    "Range",
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
            // **A primitive with two spellings is one definition**, and this
            // scan matches on `Def::name`, which carries only the canonical
            // one. Without this, `Int` is found and `I64` is not — and the
            // symptom is not a missing name but a missing *default*:
            // `Prelude::default_int` answers `None`, an unsuffixed integer
            // literal's class is never fixed, and `[4, 8, 15]` is `SC0526`,
            // *"the type of this value cannot be inferred"*. The list is
            // `science-resolve`'s so that the two crates cannot disagree about
            // which names are one type.
            for (alias, canonical) in science_resolve::builtins::ALIASED_PRIMITIVES {
                if def.name == *canonical {
                    names.entry(*alias).or_insert(def.id);
                }
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
    /// implementation of `From of Int` exists — which [`Declarations::methods`]
    /// could now answer and which §5.1 has not made the rule — and `ffi.CInt`
    /// is numeric although §5.1 does not list it, because
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

    /// The name of §4.1's unary output function this definition is, if it is
    /// one.
    ///
    /// Two entries and not three: `panic` takes `borrowed any Display` in the
    /// same section and is *declared*, so a `panic("a", b)` is already
    /// `SC0527` from the ordinary arity check and does not need this one.
    /// Returning the name rather than a `bool` is what lets the message say
    /// which function it is about without the caller reaching back into the
    /// definition table for a string it already had.
    pub fn unary_output(&self, def: DefId) -> Option<&'static str> {
        ["print", "write"].into_iter().find(|name| self.get(name) == Some(def))
    }
}

/// Every declaration a body can be checked against.
#[derive(Debug, Clone, Default)]
pub struct Declarations {
    fns: HashMap<DefId, Signature>,
    records: HashMap<DefId, Record>,
    variants: HashMap<DefId, Variant>,
    consts: HashMap<DefId, Ty>,
    /// A `const`'s value, when [`crate::constant::lower`] could evaluate its
    /// initialiser and confirm it denotes a value of the const's own type.
    ///
    /// **Absent is the common case for now**, not a failure: every `const`
    /// whose initialiser is anything but a bare literal — a call, an operator,
    /// another `const` — has no entry here, and `science-mir`'s lowering
    /// falls back to `science_mir::mir::Constant::Item` for it exactly as it
    /// always has. `crate::constant`'s module doc §3 is the scope this table
    /// admits and the reason it is drawn there.
    const_values: HashMap<DefId, hir::Literal>,
    /// An implementation or interface block's `Self`. §1.
    self_types: HashMap<DefId, Ty>,
    /// `type Item is Int` in a block, by the block that wrote it: the name, the
    /// definition it declares, and the type it answers with. §4.
    block_assocs: HashMap<DefId, Vec<(String, DefId, Ty)>>,
    /// The interface an implementation block implements, when it names one.
    implemented: HashMap<DefId, DefId>,
    /// The **arguments** that block wrote for it — the `Int` of `Array of T
    /// implements Index of Int:` — and the interface's own parameters, paired
    /// by [`Declarations::interface_arguments`]. §4a.
    implemented_args: HashMap<DefId, Vec<GenericArg>>,
    /// An interface's own generic parameters — the `Idx` of `interface Index of
    /// Idx:`. Keyed on the *interface*, because that is where the method whose
    /// signature mentions them was written. §4a.
    interface_generics: HashMap<DefId, Vec<hir::GenericParam>>,
    /// The generic parameters an implementation block declares — the `T` of
    /// `Wrapper of T has:` — which is what a call through that block's methods
    /// solves against the receiver. [`crate::check`]'s `block_substitution`.
    block_generics: HashMap<DefId, Vec<hir::GenericParam>>,
    /// Decision 11's index, built from the self types above once they are all
    /// lowered. §3.
    methods: Methods,
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
        // `krate.prelude` first: the prelude's own declarations are lowered by
        // this walk and by no other, which is what makes a signature written in
        // `builtins.rs` reach a call site as an ordinary [`Signature`]. They
        // report nothing — every name in them was resolved as it was built —
        // so the *"exactly once"* argument of §1 is unaffected.
        for item in krate.prelude.iter().chain(krate.modules.iter().flat_map(|m| &m.items)) {
            decls.item(&item.kind, krate, types, order, diagnostics);
        }
        // Second, and only second: the index reads the self types the loop
        // above lowered, so it cannot be filled in during it.
        decls.methods = Methods::of(krate, types, &decls);
        decls
    }

    /// Decision 11's lookup. §3.
    pub fn methods(&self) -> &Methods {
        &self.methods
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

    /// The value a `const` denotes, for `science-mir` to substitute at every
    /// reference in place of `science_mir::mir::Constant::Item`. `None` for
    /// every `def` that is not a `const` at all, and for a `const` whose
    /// initialiser `crate::constant::lower` could not evaluate or could not
    /// confirm against its type — both read the same to a caller, which is
    /// right: either way there is nothing here to substitute, and MIR's
    /// existing fallback is what a caller wants.
    pub fn const_value(&self, def: DefId) -> Option<&hir::Literal> {
        self.const_values.get(&def)
    }

    /// The generic parameters an implementation block declares.
    pub fn block_generics(&self, owner: DefId) -> Option<&[hir::GenericParam]> {
        self.block_generics.get(&owner).map(|generics| generics.as_slice())
    }

    /// §4a. An interface's own parameters, paired with the arguments a block
    /// supplied for them: `interface Index of Idx:` met by
    /// `Array of T implements Index of Int:` gives `[(Idx, Int)]`.
    ///
    /// **Why this is a third table and not [`Declarations::block_generics`].**
    /// The parameters belong to the *interface* and the arguments to the
    /// *block*, and neither one alone can be substituted with: a signature
    /// inherited from `Index` mentions `Idx`, and only the block says what it
    /// is. `block_generics` answers the other half — the `T` of `Array of T`,
    /// which the *receiver* fixes — and the two are solved from different
    /// places.
    ///
    /// **This was invisible until an interface both took a parameter and
    /// declared a method the implementation did not write.** `From of
    /// ParseError` is a parameterised interface the corpus has, and every
    /// implementation of it writes its own `from`, so the signature at the call
    /// site was already concrete and no substitution was missing. `Array of T
    /// implements Index of Int:` writes no `index` — the declaration it
    /// inherits is the one `methods`' §2 contributes — so `Idx` reached the
    /// call standing, and `xs["key"]` was refused as *expected `Idx`, found
    /// `String`*: a message naming a parameter the author cannot see.
    ///
    /// Empty for a block that named no interface or supplied no arguments,
    /// which is every block in the language but two.
    pub fn interface_arguments(&self, block: DefId) -> Vec<(DefId, GenericArg)> {
        let Some(args) = self.implemented_args.get(&block) else {
            return Vec::new();
        };
        let Some(interface) = self.implemented.get(&block) else {
            return Vec::new();
        };
        let Some(params) = self.interface_generics.get(interface) else {
            return Vec::new();
        };
        params.iter().zip(args).map(|(param, arg)| (param.def, arg.clone())).collect()
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

    /// Which parameters a **declared but body-less** callee's returned
    /// references may point into, as indices into the call's argument list —
    /// `0` is the receiver of a method and the first declared parameter of a
    /// free function, which is `science-regions`' `ParamRegion::param`.
    ///
    /// # The decision, and why it lives here
    ///
    /// `region-inference.md` Decision 5 abolishes elision rules: a signature's
    /// regions are *"an analysis result, not a declaration"*. That decision is
    /// about a function **with a body**, and it is the right one — the body is
    /// better evidence than any annotation. A prelude declaration has no body
    /// and never will, so there is no analysis to read and the choice is
    /// between this and `generate`'s §5 assumption that an opaque callee
    /// *"returns a reference into every argument it was given"*.
    ///
    /// > **Decision. A reference in a declared return is constrained by every
    /// > parameter whose type mentions a definition that the reference's
    /// > referent mentions.**
    ///
    /// For `Map.get(self, key: borrowed K) -> (borrowed V)?` the referent
    /// mentions `V`; `Self` is `Map of (K, V)` and mentions it, `borrowed K`
    /// does not. So the result borrows the map and not the key, which is what
    /// `generate`'s §7 says the real `Map.get` does and what its two measured
    /// false positives were the absence of.
    ///
    /// **Why this is not elision returning by the back door.** An elision rule
    /// picks *one* source and makes the signature mean it; this picks a **set**,
    /// exactly as `summary`'s §1 does, and a caller intersects them. It cannot
    /// be written differently by the author, because there is no syntax to
    /// write it in — which is Decision 5's actual content.
    ///
    /// **What it costs.** The rule is a mention test over definitions, so it is
    /// coarse in one direction: `def swap(a: borrowed T, b: borrowed T) ->
    /// borrowed T` gets `{a, b}` although a real body would pick one, and a
    /// declaration whose referent shares a definition with a parameter that
    /// cannot actually reach it — `f(seen: borrowed Array of T) -> borrowed T`
    /// where the result comes from somewhere else — is over-constrained. Both
    /// errors are in `generate`'s §7's direction: *"a program this rule refuses
    /// may be fine; a program it accepts is not made unsafe by the rule."*
    ///
    /// **`None` means this is not that question**: the function has a body, or
    /// no signature at all, and `science-regions` should do what it already
    /// does. An empty `Some` is a real answer — *"the return holds no
    /// reference, so it borrows nothing"* — and it is what `read_file` gets.
    pub fn borrow_sources(&self, types: &Types, def: DefId) -> Option<Vec<usize>> {
        let sig = self.signature(def)?;
        if sig.has_body {
            return None;
        }
        let mut referents = Vec::new();
        collect_referents(types, sig.ret, false, &mut referents);
        if referents.is_empty() {
            return Some(Vec::new());
        }

        // Parameter 0 is the receiver, whose type is the block's `Self` and is
        // not in `params`. A method whose block has no lowered self type keeps
        // the slot at `Ty::ERROR` rather than losing it: the index into the
        // argument list is what `science-regions` reads, and dropping the
        // receiver would shift every parameter after it by one.
        let receiver = sig
            .self_param
            .map(|_| sig.owner.and_then(|owner| self.self_ty(owner)).unwrap_or(Ty::ERROR));
        let params: Vec<Ty> =
            receiver.into_iter().chain(sig.params.iter().map(|param| param.ty)).collect();

        let mut sources = Vec::new();
        for (at, ty) in params.iter().enumerate() {
            let mut mentioned = Vec::new();
            collect_definitions(types, *ty, &mut mentioned);
            if mentioned.iter().any(|def| referents.contains(def)) {
                sources.push(at);
            }
        }
        // A referent nothing mentions: fall back to `generate`'s §5, because a
        // reference constrained by nothing is the one answer that is not in the
        // safe direction.
        if sources.is_empty() {
            sources.extend(0..params.len());
        }
        Some(sources)
    }

    /// Every function this table declares and no body defines, sorted.
    ///
    /// The complement of [`Declarations::bodies`], and it exists for one
    /// caller: `science-regions` asks [`Declarations::borrow_sources`] about
    /// each of these once, before any body is analysed, so that a call to one
    /// is not treated as a call to a function nothing is known about.
    pub fn without_bodies(&self) -> Vec<DefId> {
        let mut ids: Vec<DefId> =
            self.fns.values().filter(|sig| !sig.has_body).map(|sig| sig.def).collect();
        ids.sort();
        ids
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
                // its value's. `crate::constant::lower` is the walk that
                // learns it — the module doc there has the full argument for
                // why it is a walk of the initialiser's *syntax* and not of a
                // checked body, and `lib.rs`'s `codes::CONST_INITIALISER_NOT_A_LITERAL`
                // has the history of the comment this replaces, which promised
                // the walk and did not perform it.
                let annotation =
                    konst.ty.as_ref().map(|ty| lower(types, krate, order, diagnostics, ty));
                let evaluated =
                    crate::constant::lower(&konst.value, annotation, &self.prelude, types, diagnostics);
                self.consts.insert(konst.def, evaluated.ty);
                if let Some(value) = evaluated.value {
                    self.const_values.insert(konst.def, value);
                }
            }
            hir::ItemKind::Impl(block) => {
                let self_ty = lower(types, krate, order, diagnostics, &block.self_ty);
                self.self_types.insert(block.def, self_ty);
                self.block_generics.insert(block.def, block.generics.clone());
                if let Some(Res::Def(interface)) =
                    block.interface.as_ref().and_then(|bound| bound.interface_res())
                {
                    self.implemented.insert(block.def, interface);
                    if let Some(hir::BoundKind::Interface { generics, .. }) =
                        block.interface.as_ref().map(|bound| &bound.kind)
                    {
                        let args = TypeLowerer::new(types, &krate.defs, order, diagnostics)
                            .lower_args_public(generics);
                        if !args.is_empty() {
                            self.implemented_args.insert(block.def, args);
                        }
                    }
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
                if !interface.generics.is_empty() {
                    self.interface_generics
                        .insert(interface.def, interface.generics.clone());
                }
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
                                bounds: Vec::new(),
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
            bounds: param_bounds(&function.generics, &function.where_clause),
            self_param: function.self_param.as_ref().map(|s| (s.def, s.kind)),
            params,
            ret,
            owner,
            has_body: function.body.is_some(),
            span: function.span,
        }
    }
}

/// Every interface bound on a function's own type parameters, in one list.
///
/// Reads both places the surface syntax puts one — the parameter list and the
/// `where` clause — and keeps only what a call site can be held to: a bound
/// that names an interface, on a parameter this function declares.
/// [`ParamBound`] states what that leaves out and why.
///
/// **A `where` predicate is matched structurally rather than lowered.** Its
/// subject is a [`hir::TypeKind::Path`] and the only shape this list is keyed
/// on is a bare parameter, so a `Res` comparison answers it. Lowering it would
/// mean interning a type nothing else needs and running `TypeLowerer`'s
/// diagnostics over an annotation that is already reported where it is used.
fn param_bounds(
    generics: &[hir::GenericParam],
    where_clause: &[hir::WherePredicate],
) -> Vec<ParamBound> {
    let mut out = Vec::new();
    let mut push = |param: DefId, bounds: &[hir::Bound]| {
        for bound in bounds {
            if let Some(Res::Def(interface)) = bound.interface_res() {
                out.push(ParamBound { param, interface, span: bound.span });
            }
        }
    };
    for param in generics {
        if let hir::GenericParamKind::Type { bounds } = &param.kind {
            push(param.def, bounds);
        }
    }
    for predicate in where_clause {
        let hir::TypeKind::Path { res: Res::Def(def), generics: args } = &predicate.ty.kind
        else {
            continue;
        };
        if !args.is_empty() || !generics.iter().any(|param| param.def == *def) {
            continue;
        }
        push(*def, &predicate.bounds);
    }
    out
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

/// The definitions mentioned *underneath* a reference in a type.
///
/// `under` is whether the walk is already inside one, so that `borrowed V`
/// contributes `V` and `Array of (borrowed T)` contributes `T`, while a
/// `Map of (K, V)` on its own contributes nothing.
fn collect_referents(types: &Types, ty: Ty, under: bool, out: &mut Vec<DefId>) {
    match types.kind(ty) {
        TyKind::Borrowed { inner, .. } => collect_referents(types, *inner, true, out),
        TyKind::Nullable(inner) => collect_referents(types, *inner, under, out),
        TyKind::Tuple(elements) => {
            for element in elements.clone() {
                collect_referents(types, element, under, out);
            }
        }
        TyKind::Closure { params, ret } => {
            let (params, ret) = (params.clone(), *ret);
            for param in params {
                collect_referents(types, param, under, out);
            }
            collect_referents(types, ret, under, out);
        }
        _ if under => collect_definitions(types, ty, out),
        TyKind::Named { args, .. } | TyKind::Object { args, .. } => {
            for arg in args.clone() {
                if let Some(ty) = arg.as_type() {
                    collect_referents(types, ty, under, out);
                }
            }
        }
        _ => {}
    }
}

/// Every definition a type names, at any depth.
fn collect_definitions(types: &Types, ty: Ty, out: &mut Vec<DefId>) {
    match types.kind(ty) {
        TyKind::Named { def, args } | TyKind::Object { interface: def, args } => {
            let (def, args) = (*def, args.clone());
            out.push(def);
            for arg in args {
                if let Some(ty) = arg.as_type() {
                    collect_definitions(types, ty, out);
                }
            }
        }
        TyKind::Param { def } => out.push(*def),
        TyKind::SelfType { owner } => out.push(*owner),
        TyKind::SelfAssoc { assoc } => out.push(*assoc),
        TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
            collect_definitions(types, *inner, out)
        }
        TyKind::Tuple(elements) => {
            for element in elements.clone() {
                collect_definitions(types, element, out);
            }
        }
        TyKind::Closure { params, ret } => {
            let (params, ret) = (params.clone(), *ret);
            for param in params {
                collect_definitions(types, param, out);
            }
            collect_definitions(types, ret, out);
        }
        TyKind::Error | TyKind::Unit => {}
    }
}
