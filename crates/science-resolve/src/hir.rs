//! The HIR: the syntax tree after names have been resolved.
//!
//! The HIR is the AST with one difference, and the whole crate exists to make
//! that difference: **every reference to a name is a [`DefId`]**. Nothing
//! downstream of this phase ever looks a name up by string, so the type
//! checker, the MIR builder and codegen cannot disagree about which `Doc` a
//! given `Doc` meant.
//!
//! Three consequences shape the types below.
//!
//! - **Names live in the [`DefTable`], not in the tree.** A [`Fn`] node carries
//!   a `DefId`, not an `Ident`: the name, its span and its parent are one
//!   lookup away, and there is exactly one copy of them. A node that keeps a
//!   bare `Ident` anyway — [`ExprKind::Field`], [`ExprKind::MethodCall`] — is
//!   one this phase genuinely cannot resolve, because the answer depends on
//!   the type of the receiver. Those belong to the type checker, and they are
//!   the only ones.
//! - **Every node keeps its span**, §9 of the design spec, no exceptions. The
//!   `X { kind, span }` / `XKind` split of the AST is kept for the reason the
//!   parser gives: the span lives in one place and every traversal can ask any
//!   node where it came from without matching on it first.
//! - **Failure is a node, not an absence.** A name that does not resolve
//!   becomes [`Res::Error`], and a construct that cannot be lowered becomes an
//!   `Error` variant. The tree stays walkable, so one compilation reports
//!   several problems instead of one.
//!
//! The ambiguities §4.4 of the spec hands to name resolution are settled here:
//! an `ast::PatternKind::Binding` that named a unit variant arrives as
//! [`PatternKind::Variant`], an empty-argument call on a fieldless record
//! arrives as [`ExprKind::StructLit`], and the implicit closure of §4.6 has a
//! binder for its `each`. A later phase never has to ask again.

use std::ops::Index;

use science_diagnostics::{FileId, Span};

// Re-exported rather than redefined: these carry no names and no paths, so
// resolution has nothing to say about them, and a second copy would only be a
// second thing to keep in step with the parser.
pub use science_parser::ast::{
    BinaryOp, ConstExpr, ConstExprKind, Ident, Literal, SelfKind, UnaryOp,
};

// --- definitions ---------------------------------------------------------

/// An opaque, unique id for one definition.
///
/// Opaque on purpose: it is an index into a [`DefTable`] and nothing else may
/// depend on that. Ordering is allocation order, which makes any map keyed by
/// `DefId` iterate deterministically — the snapshots depend on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefId(u32);

impl DefId {
    /// The table index. For the table's own use and for stable dumps.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Display for DefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// What kind of thing a [`DefId`] names.
///
/// Every binder in the language is here, down to a `let`: a local is a
/// definition like any other, which is what lets a later phase talk about
/// where a name was introduced without a second side table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefKind {
    /// A file, or a directory with a `mod.science` (§4.4).
    Module,
    Fn,
    /// `function name(..)` inside an `extern` block. A kind of its own, not
    /// because the name works differently — it is an ordinary module-level
    /// name — but because three later rules turn on it: §1.7 permits named
    /// arguments at its call sites and nowhere else, §3.1 requires an
    /// `unsafe` block around the call, and codegen emits a declaration rather
    /// than a definition.
    ExternFn,
    /// `type Doc:` — the record type of §4.4.
    Record,
    /// `choice Format:` — the sum type of §4.4.
    Choice,
    /// One variant of a choice type. Its parent is the choice.
    Variant,
    /// `type Embedding is Array of F32` (§4.4).
    Alias,
    /// `const WIDTH be 768` (§4.4), and `const NAME be literal as T` inside
    /// an `extern` block.
    Const,
    /// `static NAME: T` inside an `extern` block — an exported data symbol
    /// resolved at link, which `c-binding-coverage.md` Decision 7 adds as the
    /// fourth item form. A different kind from a constant because the two are
    /// different things: one is a compile-time value, the other an address.
    Static,
    /// `union Name: size N align M` inside an `extern` block — the opaque
    /// blob a C union is imported as.
    Union,
    Interface,
    /// `type Item` on an interface, and the `type Item is Int` that answers
    /// it. Its parent is the interface or the implementation that wrote it.
    AssocType,
    /// An implementation block — `Doc has:` or `Doc implements Summarize:`.
    /// It has no name of its own; see [`Def::name`].
    Impl,
    /// A record's field. Its parent is the record.
    Field,
    /// A function parameter, and the subject of a closure.
    Param,
    /// The `self`, `mutable self` or `self: Self` receiver.
    SelfParam,
    /// A `let` binding, or a binding introduced by a pattern.
    Local,
    /// A generic type parameter of a function, type, interface or
    /// implementation.
    TypeParam,
    /// A `const WIDTH: Int` generic parameter (§5.3). A different kind from
    /// [`DefKind::TypeParam`] because it stands for a value, not a type.
    ConstParam,
    /// A primitive or library type the compiler knows about (§5.1, §8).
    Primitive,
}

impl DefKind {
    /// How the kind is named in a diagnostic.
    pub fn describe(self) -> &'static str {
        match self {
            DefKind::Module => "a module",
            DefKind::Fn => "a function",
            DefKind::ExternFn => "a foreign function",
            DefKind::Record => "a record type",
            DefKind::Choice => "a choice type",
            DefKind::Variant => "a variant",
            DefKind::Alias => "a type alias",
            DefKind::Const => "a constant",
            DefKind::Static => "a foreign global",
            DefKind::Union => "a foreign union",
            DefKind::Interface => "an interface",
            DefKind::AssocType => "an associated type",
            DefKind::Impl => "an implementation block",
            DefKind::Field => "a field",
            DefKind::Param => "a parameter",
            DefKind::SelfParam => "the `self` receiver",
            DefKind::Local => "a local binding",
            DefKind::TypeParam => "a generic parameter",
            DefKind::ConstParam => "a const generic parameter",
            DefKind::Primitive => "a built-in type",
        }
    }

    /// Whether the kind names a type, which is what a type position and an
    /// implementation header may refer to.
    ///
    /// [`DefKind::ConstParam`] is admitted although it names a value. §5.3
    /// puts const arguments in the same `of (..)` list as type arguments, so
    /// `Grid of (T, ROWS)` reaches a const parameter through a type position
    /// and there is no way to tell the two apart without the declared arity of
    /// the thing being applied — which is `science-types`' to know, not this
    /// phase's.
    ///
    /// `const-expression-arithmetic.md` §6.4 **extends this one compromise by
    /// one case rather than introducing a second**: where a parameter's kind
    /// is [`ConstParamKind::Shape`], a parenthesised list in the matching
    /// argument position is a shape rather than a tuple type. Same seam, same
    /// resolution — the declared kind decides, and the declared kind is not
    /// this phase's to know either.
    pub fn is_type(self) -> bool {
        matches!(
            self,
            DefKind::Record
                | DefKind::Choice
                | DefKind::Alias
                | DefKind::Union
                | DefKind::AssocType
                | DefKind::TypeParam
                | DefKind::ConstParam
                | DefKind::Primitive
        )
    }
}

/// The reserved file id builtin definitions live in.
///
/// Builtins have no source text, but §9 admits no node without a span, so they
/// get an empty span in a file no `SourceMap` will ever hold. A diagnostic
/// must never use one as a primary label; [`Def::is_builtin`] is how a caller
/// checks before pointing at a definition site.
pub const BUILTIN_FILE: FileId = FileId(u32::MAX);

/// The span every builtin definition carries.
pub const BUILTIN_SPAN: Span = Span { file: BUILTIN_FILE, start: 0, end: 0 };

/// One definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    pub id: DefId,
    pub kind: DefKind,
    /// The name as written. An `impl` block has none and carries `""`.
    pub name: String,
    /// Where it was declared: the name's span, not the whole declaration's, so
    /// that "defined here" points at the word the reader is looking for.
    pub span: Span,
    /// The enclosing definition: the module for an item, the choice type for
    /// a variant, the function for a parameter. `None` only for the crate
    /// root.
    pub parent: Option<DefId>,
}

impl Def {
    /// Whether this definition comes from the builtin prelude rather than from
    /// source, in which case its span points at no readable text.
    pub fn is_builtin(&self) -> bool {
        self.span.file == BUILTIN_FILE
    }
}

/// Every definition in one compilation, indexed by [`DefId`].
///
/// Flat and append-only. A tree of scopes is what resolution walks; this is
/// what it leaves behind, and it outlives the scopes entirely.
#[derive(Debug, Clone, Default)]
pub struct DefTable {
    defs: Vec<Def>,
}

impl DefTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a definition and hands back its id.
    pub fn alloc(
        &mut self,
        kind: DefKind,
        name: impl Into<String>,
        span: Span,
        parent: Option<DefId>,
    ) -> DefId {
        let id = DefId(self.defs.len() as u32);
        self.defs.push(Def { id, kind, name: name.into(), span, parent });
        id
    }

    pub fn get(&self, id: DefId) -> &Def {
        &self.defs[id.index()]
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Def> {
        self.defs.iter()
    }

    /// The nearest enclosing module, counting `id` itself.
    ///
    /// This is what the orphan rule of §5.4 asks about: "the module declaring
    /// it" is the answer for the implementation, and "belongs to" is the
    /// answer for the interface and the type.
    pub fn module_of(&self, id: DefId) -> Option<DefId> {
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            let def = self.get(current);
            if def.kind == DefKind::Module {
                return Some(current);
            }
            cursor = def.parent;
        }
        None
    }

    /// The dotted path to a definition, for diagnostics: `text.parser.Token`.
    ///
    /// The crate root is unnamed and contributes nothing, so a top-level item
    /// prints as its own name.
    pub fn path_of(&self, id: DefId) -> String {
        let mut parts = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            let def = self.get(current);
            if !def.name.is_empty() {
                parts.push(def.name.as_str());
            }
            cursor = def.parent;
        }
        parts.reverse();
        parts.join(".")
    }

    /// The definitions whose parent is `id`, in declaration order.
    pub fn children(&self, id: DefId) -> impl Iterator<Item = &Def> {
        self.defs.iter().filter(move |d| d.parent == Some(id))
    }
}

impl Index<DefId> for DefTable {
    type Output = Def;

    fn index(&self, id: DefId) -> &Def {
        self.get(id)
    }
}

// --- what a name resolved to ---------------------------------------------

/// The result of resolving one reference.
///
/// There is no `Unresolved` distinct from `Error`: by the time a `Res::Error`
/// exists, the diagnostic that explains it has already been pushed. Later
/// phases treat it as "assume this was fine and keep going", which is what
/// stops one unknown name from producing a page of follow-on errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Res {
    /// Resolved to a definition.
    Def(DefId),
    /// `Self` or `self` inside an implementation or an interface. The id is
    /// the block it belongs to; what `Self` *is* depends on that block's self
    /// type, which the type checker substitutes.
    SelfTy(DefId),
    /// Resolution failed, and was reported.
    Error,
}

impl Res {
    pub fn def_id(self) -> Option<DefId> {
        match self {
            Res::Def(id) => Some(id),
            _ => None,
        }
    }

    pub fn is_error(self) -> bool {
        self == Res::Error
    }
}

// --- crate and modules ---------------------------------------------------

/// A whole compilation: the definitions, and the resolved tree of every file.
#[derive(Debug, Clone)]
pub struct Crate {
    pub defs: DefTable,
    /// The unnamed module every top-level file hangs from.
    pub root: DefId,
    /// One entry per source file, in the order they were handed in.
    pub modules: Vec<Module>,
}

/// A file (§4.4: a file is a module).
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub def: DefId,
    pub items: Vec<Item>,
    pub span: Span,
}

/// `use` does not appear here: an import is a name in a scope, and once the
/// scope is built there is nothing of it left for a later phase to walk. Every
/// other item survives.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Fn(Fn),
    Record(Record),
    Choice(Choice),
    Alias(Alias),
    Const(Const),
    Interface(Interface),
    Impl(Impl),
    /// `unsafe extern "C" library "openblas":` — the block survives because
    /// linking needs it, even though the names inside it are already ordinary
    /// module-level names by the time this tree exists.
    Extern(ExternBlock),
}

// --- items ---------------------------------------------------------------

/// A function, a method, or an interface's required method — one node for all
/// three, as in the AST. `body: None` is a signature without a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Fn {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    /// `None` means unit (§4.4).
    pub ret: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub def: DefId,
    pub kind: GenericParamKind,
    pub span: Span,
}

impl GenericParam {
    /// Whether this parameter absorbs a run of arguments rather than exactly
    /// one — `const-expression-arithmetic.md` §10.1 item 6.
    ///
    /// Variadic-ness is a property of the **kind**, not of a sigil: a
    /// [`ConstParamKind::Shape`] parameter *is* a type-level list of const
    /// `Int` expressions (§6.1 of that note), so one `Shape` parameter stands
    /// for the whole of `(n, 768)` and for the whole of `(768)` alike. That is
    /// why item 5 — the closed kind enum — has to land before item 6 can mean
    /// anything: the kind is where the answer is read from.
    pub fn is_variadic(&self) -> bool {
        match &self.kind {
            GenericParamKind::Type { .. } => false,
            GenericParamKind::Const { kind, .. } => kind.is_variadic(),
        }
    }
}

/// §5.3's two kinds of generic parameter, kept apart because they live in
/// different namespaces: a type parameter stands for a type and a const
/// parameter for a value, and only the second one has an annotation of its
/// own.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericParamKind {
    /// `T`, or `T: Ord + Clone`.
    Type { bounds: Vec<Bound> },
    /// `const WIDTH: Int`. The annotation is mandatory, so it is not optional
    /// here either — and it is a **kind**, not a type. See
    /// [`ConstParamKind`].
    Const {
        kind: ConstParamKind,
        /// The span of the annotation as written, which is what a diagnostic
        /// about the kind points at. The parser parsed it as a type (see
        /// `ast::GenericParamKind::Const`);
        /// nothing of that type survives here, because a kind is not one.
        annotation: Span,
    },
}

/// The closed set of kinds a const generic parameter may be annotated with —
/// `const-expression-arithmetic.md` §2.3, and F0 commitment 5 of its §10.1.
///
/// > **Decision 2.3.** `const N: K` admits exactly `K ∈ { Int, Shape }`.
/// > `Int` is the F0 kind. `Shape` arrives in F1 (§6). Anything else —
/// > `const F: Bool`, `const S: String`, `const W: F64` — is `SC0260`.
///
/// **This is an enum and not a [`Type`] on purpose.** `Shape` is not a type
/// and never will be one: it is a type-level *list* of const `Int`
/// expressions, with no values and no representation. If the annotation
/// position stayed a type position, `Shape` would arrive in F1 as either a
/// fake primitive in the prelude or a second annotation position beside the
/// first, and every phase that matches on a const parameter would be
/// reopened. §10.1's reason for putting the enum in F0 is exactly that: *"so
/// the annotation position is a kind position from the first commit rather
/// than a type position retrofitted into one later."*
///
/// The parser is unchanged, per §2.3 — it still reads the annotation with
/// `parse_type` and accepts whatever it finds, which is the house pattern
/// `ast::TypeKind::Any` already uses.
/// The classification and the diagnostic happen here, in the first phase that
/// runs after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstParamKind {
    /// `const ROWS: Int` — an integer, and the only kind F0 has a checker for.
    Int,
    /// `const SHAPE: Shape` — a type-level list of const `Int` expressions
    /// (§6.1). §2.3 admits the kind today; the shape *layer* — the seven
    /// operations of §6.2 and `EQUAL_SHAPE` — is F1's. What F0 owes it is
    /// that a `Shape` parameter is **variadic** in its declaration's argument
    /// list, which is what [`GenericArity`] records.
    Shape,
    /// Not one of the kinds above. Recovery only, in the manner of
    /// [`Res::Error`]: the diagnostic has already been reported and nothing
    /// downstream reports a second one.
    Error,
}

impl ConstParamKind {
    /// The kinds, spelled as the programmer spells them. Closed: adding to it
    /// is a spec change (§2.2).
    pub const NAMES: &'static [&'static str] = &["Int", "Shape"];

    /// The kind a one-segment annotation names, or `None` when it names none.
    pub fn from_name(name: &str) -> Option<ConstParamKind> {
        match name {
            "Int" => Some(ConstParamKind::Int),
            "Shape" => Some(ConstParamKind::Shape),
            _ => None,
        }
    }

    /// How the kind is spelled, for a dump or a diagnostic.
    pub fn describe(self) -> &'static str {
        match self {
            ConstParamKind::Int => "Int",
            ConstParamKind::Shape => "Shape",
            ConstParamKind::Error => "(not a kind)",
        }
    }

    /// Whether a parameter of this kind absorbs a run of generic arguments
    /// rather than exactly one.
    ///
    /// `Shape` does, because a shape is a list and `broadcasting.md` §11.1
    /// requires `Tensor of (F32, (768))` and `Tensor of (F32, (n, 768))` to be
    /// one type constructor at two arities. `Int` does not. [`Self::Error`]
    /// does not, so that a bad annotation does not silently make a
    /// declaration accept any number of arguments.
    pub fn is_variadic(self) -> bool {
        matches!(self, ConstParamKind::Shape)
    }
}

/// How many generic arguments a declaration admits — F0 commitment 6 of
/// `const-expression-arithmetic.md` §10.1, which is `broadcasting.md` §11.1:
///
/// > **11.1 Const generic argument lists must admit variadic arity.**
/// > `Tensor of (F32, (n, 768))` and `Tensor of (F32, (768))` are one type
/// > constructor at two arities (§5.2). If F0's resolver checks a
/// > declaration's generic arguments against a fixed count, F1 cannot express
/// > rank-polymorphic broadcasting without reopening the resolver and every
/// > snapshot.
///
/// **This is not "stop checking arity".** `Map of (String, Int, Bool)` is
/// still wrong and [`GenericArity::admits`] still says so. What the type is
/// for is that the answer is computed from the declaration's *parameter
/// kinds* rather than from `params.len()`, so that the one declaration that
/// wants a range gets one and every other declaration keeps its exact count.
///
/// A variadic parameter absorbs the arguments the fixed ones do not take, so
/// the shape of a parameter list is `leading` fixed parameters, then at most
/// one variadic parameter, then `trailing` fixed parameters. A second
/// variadic parameter has no unambiguous split and the resolver reports it
/// (`SC0221`); [`GenericArity::of`] treats the first one as the variadic one
/// so that the rest of the declaration still has an arity to be checked
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenericArity {
    /// Parameters before the variadic one, each taking exactly one argument.
    pub leading: usize,
    /// Parameters after it, each taking exactly one argument.
    pub trailing: usize,
    /// Whether a parameter between them absorbs the rest.
    pub variadic: bool,
}

impl GenericArity {
    /// The arity a parameter list declares.
    pub fn of(params: &[GenericParam]) -> GenericArity {
        match params.iter().position(GenericParam::is_variadic) {
            Some(at) => {
                GenericArity { leading: at, trailing: params.len() - at - 1, variadic: true }
            }
            None => GenericArity { leading: params.len(), trailing: 0, variadic: false },
        }
    }

    /// The fewest arguments the declaration admits. For a non-variadic list
    /// this is also the most.
    pub fn minimum(self) -> usize {
        self.leading + self.trailing
    }

    /// Whether `count` arguments fill this declaration.
    pub fn admits(self, count: usize) -> bool {
        if self.variadic {
            count >= self.minimum()
        } else {
            count == self.minimum()
        }
    }

    /// The arity as a diagnostic says it: `2 generic arguments`, or
    /// `at least 1 generic argument`.
    pub fn describe(self) -> String {
        let n = self.minimum();
        let plural = if n == 1 { "argument" } else { "arguments" };
        if self.variadic {
            format!("at least {n} generic {plural}")
        } else {
            format!("{n} generic {plural}")
        }
    }
}

/// An interface named as a bound: in `of T: Ord`, in `any Summarize`, after
/// `implements`.
///
/// Kept as its own node rather than collapsed into a [`Type`] because the
/// parser distinguished the two, and because a bound that resolved to
/// something which is not an interface must say so with the span as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub res: Res,
    /// Generic arguments on the interface: `From of Doc`.
    pub generics: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate {
    pub ty: Type,
    pub bounds: Vec<Bound>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelfParam {
    pub def: DefId,
    pub kind: SelfKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub def: DefId,
    pub ty: Type,
    pub span: Span,
}

/// `type Doc:` with its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub def: DefId,
    pub ty: Type,
    pub span: Span,
}

/// `choice Format:` with its variants.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub def: DefId,
    /// Positional payload; empty is a unit variant (§4.4).
    pub payload: Vec<Type>,
    pub span: Span,
}

/// `type Embedding is Array of F32` (§4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub ty: Type,
    pub span: Span,
}

/// `const WIDTH be 768` (§4.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Const {
    pub def: DefId,
    pub ty: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

/// `interface Summarize:` with its members.
#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    /// Interfaces this one requires, written `interface A: B + C`.
    pub supers: Vec<Bound>,
    pub where_clause: Vec<WherePredicate>,
    /// `type Item` — declared here, supplied by the implementation (§5.4).
    /// Each one is a definition of its own, reached as `Self.Item`.
    pub assoc_types: Vec<AssocType>,
    pub methods: Vec<Fn>,
    pub span: Span,
}

/// `type Item` on an interface, and `type Item is Int` in an implementation.
///
/// One node for both: they are the same name in the same namespace, and the
/// only difference is whether the type is written yet.
#[derive(Debug, Clone, PartialEq)]
pub struct AssocType {
    pub def: DefId,
    /// `None` on an interface, which declares the name without answering it.
    pub ty: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    /// The block's own definition. It is unnamed, and it is the parent of the
    /// methods and of the generic parameters, which is how `module_of` finds
    /// the module for the orphan rule.
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    /// `Some` for `Doc implements Summarize:`, `None` for the inherent
    /// `Doc has:`.
    pub interface: Option<Bound>,
    pub self_ty: Type,
    pub where_clause: Vec<WherePredicate>,
    /// `type Item is Int` — this block's side of §5.4's associated types.
    pub assoc_types: Vec<AssocType>,
    pub methods: Vec<Fn>,
    pub span: Span,
}

/// `unsafe extern "C" library "openblas" via pkg-config "openblas":`
///
/// Resolution declares each item's name in the enclosing module and resolves
/// the types they mention. What it deliberately does not do is check them:
/// whether a type is FFI-representable under `ffi.CLayout`, whether a read of
/// a `static` sits inside an `unsafe` block, and what the linker is handed are
/// three different later phases.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternBlock {
    pub is_unsafe: bool,
    pub abi: String,
    /// `None` when the clause was missing and was reported by the parser.
    pub library: Option<ExternLibrary>,
    pub items: Vec<ExternItem>,
    pub span: Span,
}

/// The link target and how to find it (§5.1, §5.2 of `ffi-c-boundary.md`).
///
/// It names no definition and holds no `DefId`: a library is a string the
/// driver passes to the system linker, not a name in any Science scope.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternLibrary {
    pub name: String,
    pub pkg_config: Option<String>,
    pub static_link: bool,
    pub when_available: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternItem {
    pub kind: ExternItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExternItemKind {
    Fn(ExternFn),
    Alias(ExternAlias),
    Const(ExternConst),
    Static(ExternStatic),
    Union(ExternUnion),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternFn {
    pub def: DefId,
    pub params: Vec<Param>,
    /// `None` means unit.
    pub ret: Option<Type>,
    /// `symbol "dgemm_"`: the linker name when it is not the Science name.
    pub symbol: Option<String>,
    /// Reported by the parser and kept so that no later phase mistakes the
    /// declaration for a complete one.
    pub variadic: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternAlias {
    pub def: DefId,
    pub ty: Type,
    pub span: Span,
}

/// `const NAME be literal as T`, whose value is a literal rather than an
/// expression: there is nothing in it to resolve, and that is the point.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternConst {
    pub def: DefId,
    pub negative: bool,
    pub value: Literal,
    pub ty: Type,
    pub span: Span,
}

/// `static H5T_NATIVE_DOUBLE_g: Hid`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternStatic {
    pub def: DefId,
    pub ty: Type,
    pub span: Span,
}

/// `union H5R_ref_t: size 64 align 8`. `None` is a number the parser reported
/// as missing.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternUnion {
    pub def: DefId,
    pub size: Option<u128>,
    pub align: Option<u128>,
    pub span: Span,
}

// --- types ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    /// `String`, `Array of T`, `text.parser.Token`, a bare type parameter.
    /// The path is gone; what is left is what it pointed at.
    Path { res: Res, generics: Vec<Type> },
    /// `borrowed T` and `mutable borrowed T`.
    Borrowed { mutable: bool, inner: Box<Type> },
    /// `any Summarize`.
    Any(Bound),
    /// Two or more elements; `(T)` is `T` and produces no node.
    Tuple(Vec<Type>),
    Unit,
    /// `T?` — `T`, or `null` (revision 2 §3.1).
    ///
    /// The `Error?` shorthand for `(any Error)?` is *not* expanded here
    /// either. Resolution records what the author wrote; deciding that a
    /// bare interface name in a nullable return position means a trait
    /// object needs to know the name is an interface, and that is a fact
    /// about the resolved type rather than about the syntax.
    Nullable(Box<Type>),
    /// `Self`, carrying the implementation or interface it stands in for.
    SelfType(Res),
    /// `Self.Item` (§5.4). `res` is the [`AssocType`] it names; `name`
    /// survives for diagnostics, and is all that is left when it does not
    /// resolve.
    SelfAssoc { res: Res, name: Ident },
    /// The `4` in `Window of (Int, 4)`, and the `-1` of a dimension vector:
    /// a const generic argument (§5.3). It declares no name and mentions
    /// none, so there is nothing to resolve and it arrives unchanged.
    Const(ConstExpr),
    /// A type the parser could not parse, or a name that did not resolve.
    Error,
}

// --- blocks and statements -----------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// The expression the block evaluates to (§4.4), split out so that no
    /// later phase re-derives it.
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    Let(Let),
    Expr(Expr),
    Assign { target: Expr, value: Expr },
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    Error,
}

/// `let x = e`. F0 has no destructuring in `let` (§12), so one binding.
///
/// The binding is in scope from *after* this statement onward, which is why
/// `value` is resolved before `def` is added to the enclosing rib.
#[derive(Debug, Clone, PartialEq)]
pub struct Let {
    /// One binding, or several for revision 2 §3.1's `let value, err be f()`.
    /// Never empty.
    pub bindings: Vec<LetBinding>,
    pub mutable: bool,
    pub value: Expr,
    pub span: Span,
}

/// One name bound by a `let`, resolved.
///
/// Whether the value actually *is* a tuple of the right width is not checked
/// here. This phase knows the binding count and nothing about the
/// initialiser's type, so the arity check belongs to `science-types` along
/// with everything else that needs one.
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub def: DefId,
    pub ty: Option<Type>,
    pub span: Span,
}

// --- expressions ---------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    /// A resolved name: a local, a parameter, a function, a unit variant, or a
    /// type used as a constructor.
    Path { res: Res, generics: Vec<Type> },
    /// `self`.
    SelfValue(Res),
    /// `f(a, b)` and `Some(x)`.
    Call { callee: Box<Expr>, args: Vec<Arg> },
    /// `receiver.method(args)`. The method stays a name: which one it is
    /// depends on the receiver's type, so it is the type checker's to resolve.
    MethodCall { receiver: Box<Expr>, method: Ident, generics: Vec<Type>, args: Vec<Arg> },
    /// `base.name`. The field stays a name, for the same reason.
    Field { base: Box<Expr>, name: Ident },
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `Doc(title: "a")`: named arguments (§4.4). Also where `Doc()` lands
    /// once resolution has seen that `Doc` is a fieldless record.
    StructLit { res: Res, fields: Vec<FieldInit> },
    Tuple(Vec<Expr>),
    Unit,
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Cast { expr: Box<Expr>, ty: Type },
    /// `e?` — the presence test. A `Bool`, and total: unlike the `Try` it
    /// replaced, it cannot return from the enclosing function.
    Present(Box<Expr>),
    /// `borrowed e` and `mutable borrowed e`.
    Borrowed { mutable: bool, expr: Box<Expr> },
    /// `0..n` and `0..=n` (§4.5).
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    /// Both closure forms of §4.6, with the difference between them closed.
    ///
    /// `param` is always a definition: `doc giving doc.title` binds `doc`, and
    /// the implicit `each.title` binds a subject named `each` that the
    /// programmer did not write. [`ExprKind::Each`] then points at it like any
    /// other reference, so nothing downstream has to know which form was
    /// written.
    Closure { param: DefId, body: Box<Expr> },
    /// `each` — the subject of the enclosing implicit closure (§4.6).
    Each(Res),
    If(IfExpr),
    Match(MatchExpr),
    Loop { body: Block },
    For { pattern: Pattern, iter: Box<Expr>, body: Block },
    /// `unsafe:` with a body (§3 of `ffi-c-boundary.md`). It survives
    /// resolution as its own node rather than collapsing into a plain block:
    /// the type checker needs to know which expressions are inside one, and
    /// the block is also what a reviewer reads against the C documentation.
    Unsafe(Block),
    Block(Block),
    /// An expression that could not be lowered. Its span is kept so a later
    /// phase can still say where the hole is.
    Error,
}

/// One argument of a call.
///
/// The name is a *label*, not a reference: `docs.sort(by: f)` names the
/// parameter, and there is no `by` in any scope to resolve it against. It is
/// carried through unresolved so the type checker can match it to a parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

/// `name: value` in a record construction.
///
/// `field` is the field's `DefId` once it is known; `name` survives for
/// diagnostics and dumps, and is all that is left when it is not.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub field: Res,
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfExpr {
    pub cond: Box<Expr>,
    pub then_branch: Block,
    pub else_branch: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

// --- patterns ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

/// There is no "binding that might be a variant" here. The first of §4.4's
/// three open questions is closed by the time a pattern reaches this type: a
/// bare name is either a [`PatternKind::Binding`] with a fresh `DefId` or a
/// [`PatternKind::Variant`] with no elements, and nothing downstream re-asks.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    Wildcard,
    Literal(Literal),
    /// A name that introduces a new binding.
    Binding { mutable: bool, def: DefId },
    /// `Ok(value)` and `None`. The payload is positional.
    Variant { res: Res, elems: Vec<Pattern> },
    /// `Doc(title: t, body: _)`, and `Doc()` on a fieldless record.
    Struct { res: Res, fields: Vec<FieldPattern> },
    Tuple(Vec<Pattern>),
    Unit,
    Or(Vec<Pattern>),
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldPattern {
    pub field: Res,
    pub name: Ident,
    pub pattern: Pattern,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(kind: GenericParamKind) -> GenericParam {
        GenericParam { def: DefId(0), kind, span: BUILTIN_SPAN }
    }

    fn type_param() -> GenericParam {
        param(GenericParamKind::Type { bounds: Vec::new() })
    }

    fn const_param(kind: ConstParamKind) -> GenericParam {
        param(GenericParamKind::Const { kind, annotation: BUILTIN_SPAN })
    }

    /// The half of commitment 6 that is *not* "stop checking arity": an
    /// ordinary declaration keeps its exact count.
    #[test]
    fn an_ordinary_declaration_admits_exactly_its_parameter_count() {
        // `Map of (K, V)`.
        let arity = GenericArity::of(&[type_param(), type_param()]);
        assert_eq!(arity, GenericArity { leading: 2, trailing: 0, variadic: false });
        assert!(!arity.admits(1));
        assert!(arity.admits(2));
        assert!(!arity.admits(3), "`Map of (String, Int, Bool)` is still wrong");
        assert_eq!(arity.describe(), "2 generic arguments");
    }

    /// A const `Int` parameter is an ordinary one: `Window of (T, const N: Int)`
    /// takes two arguments and no more.
    #[test]
    fn a_const_int_parameter_is_not_variadic() {
        let arity = GenericArity::of(&[type_param(), const_param(ConstParamKind::Int)]);
        assert!(!arity.variadic);
        assert!(arity.admits(2));
        assert!(!arity.admits(3));
    }

    /// `broadcasting.md` §11.1: `Tensor of (F32, (768))` and
    /// `Tensor of (F32, (n, 768))` are one type constructor at two arities.
    #[test]
    fn a_shape_parameter_absorbs_the_rest() {
        // `type Tensor of (T, const SHAPE: Shape)`.
        let arity = GenericArity::of(&[type_param(), const_param(ConstParamKind::Shape)]);
        assert_eq!(arity, GenericArity { leading: 1, trailing: 0, variadic: true });
        assert!(!arity.admits(0), "the element type is still required");
        for count in 1..=6 {
            assert!(arity.admits(count), "rank {} is one of the arities", count - 1);
        }
        assert_eq!(arity.describe(), "at least 1 generic argument");
    }

    /// The variadic parameter need not be last: what follows it is counted
    /// from the right, so the split stays unambiguous.
    #[test]
    fn parameters_after_the_variadic_one_are_counted_from_the_right() {
        let arity = GenericArity::of(&[
            type_param(),
            const_param(ConstParamKind::Shape),
            const_param(ConstParamKind::Int),
        ]);
        assert_eq!(arity, GenericArity { leading: 1, trailing: 1, variadic: true });
        assert!(!arity.admits(1));
        assert!(arity.admits(2));
        assert!(arity.admits(9));
    }

    /// A rejected annotation must not turn into "any arity": that would make
    /// one bad kind silence every argument-count error on the declaration.
    #[test]
    fn a_rejected_kind_is_not_variadic() {
        let arity = GenericArity::of(&[const_param(ConstParamKind::Error)]);
        assert!(!arity.variadic);
        assert!(arity.admits(1));
        assert!(!arity.admits(2));
    }

    #[test]
    fn the_kind_set_is_closed() {
        for name in ConstParamKind::NAMES {
            assert!(ConstParamKind::from_name(name).is_some(), "{name} is one of the kinds");
        }
        for name in ["Bool", "String", "F64", "Matrix", "int", "shape"] {
            assert!(ConstParamKind::from_name(name).is_none(), "{name} is not a kind");
        }
    }
}
