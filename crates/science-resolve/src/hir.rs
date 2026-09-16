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
//! [`PatternKind::Variant`], and an empty-argument call on a fieldless struct
//! arrives as [`ExprKind::StructLit`]. A later phase never has to ask again.

use std::ops::Index;

use science_diagnostics::{FileId, Span};

// Re-exported rather than redefined: these carry no names and no paths, so
// resolution has nothing to say about them, and a second copy would only be a
// second thing to keep in step with the parser.
pub use science_parser::ast::{BinaryOp, Ident, Literal, SelfKind, UnaryOp};

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
    /// A file, or a directory with a `mod.science` (§4.3).
    Module,
    Fn,
    Struct,
    Enum,
    /// One variant of an enum. Its parent is the enum.
    Variant,
    Trait,
    /// An `impl` block. It has no name of its own; see [`Def::name`].
    Impl,
    /// A struct field. Its parent is the struct.
    Field,
    /// A function parameter.
    Param,
    /// The `self`, `&self` or `&mut self` receiver.
    SelfParam,
    /// A `let` binding, or a binding introduced by a pattern.
    Local,
    /// A generic parameter of a function, type, trait or impl.
    TypeParam,
    /// A primitive or library type the compiler knows about (§5.1, §8).
    Primitive,
}

impl DefKind {
    /// How the kind is named in a diagnostic.
    pub fn describe(self) -> &'static str {
        match self {
            DefKind::Module => "a module",
            DefKind::Fn => "a function",
            DefKind::Struct => "a struct",
            DefKind::Enum => "an enum",
            DefKind::Variant => "an enum variant",
            DefKind::Trait => "a trait",
            DefKind::Impl => "an impl block",
            DefKind::Field => "a field",
            DefKind::Param => "a parameter",
            DefKind::SelfParam => "the `self` receiver",
            DefKind::Local => "a local binding",
            DefKind::TypeParam => "a generic parameter",
            DefKind::Primitive => "a built-in type",
        }
    }

    /// Whether the kind names a type, which is what a type position and an
    /// `impl` header may refer to.
    pub fn is_type(self) -> bool {
        matches!(self, DefKind::Struct | DefKind::Enum | DefKind::TypeParam | DefKind::Primitive)
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
    /// The enclosing definition: the module for an item, the enum for a
    /// variant, the function for a parameter. `None` only for the crate root.
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
    /// it" is the answer for the `impl`, and "belongs to" is the answer for
    /// the trait and the type.
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
    /// `Self` or `self` inside an `impl` or a `trait`. The id is the `impl` or
    /// `trait` it belongs to; what `Self` *is* depends on that block's self
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

/// A file (§4.3: a file is a module).
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
    Struct(Struct),
    Enum(Enum),
    Trait(Trait),
    Impl(Impl),
}

// --- items ---------------------------------------------------------------

/// A function, a method, or a trait's required method — one node for all
/// three, as in the AST. `body: None` is a signature without a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Fn {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    /// `None` means unit (§4.3).
    pub ret: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub def: DefId,
    pub bounds: Vec<Bound>,
    pub span: Span,
}

/// A trait named as a bound: in `T: Ord`, in `dyn Summarize`, after `impl`.
///
/// Kept as its own node rather than collapsed into a [`Type`] because the
/// parser distinguished the two, and because a bound that resolved to
/// something which is not a trait must say so with the span as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    pub res: Res,
    /// Generic arguments on the trait: `From[Doc]`.
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

#[derive(Debug, Clone, PartialEq)]
pub struct Struct {
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

#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub def: DefId,
    /// Positional payload; empty is a unit variant (§4.3).
    pub payload: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trait {
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    pub supertraits: Vec<Bound>,
    pub where_clause: Vec<WherePredicate>,
    pub methods: Vec<Fn>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Impl {
    /// The block's own definition. It is unnamed, and it is the parent of the
    /// methods and of the generic parameters, which is how `module_of` finds
    /// the module for the orphan rule.
    pub def: DefId,
    pub generics: Vec<GenericParam>,
    /// `Some` for `impl Trait for Type`, `None` for an inherent impl.
    pub trait_: Option<Bound>,
    pub self_ty: Type,
    pub where_clause: Vec<WherePredicate>,
    pub methods: Vec<Fn>,
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
    /// `String`, `Array[T]`, `text.parser.Token`, a bare type parameter. The
    /// path is gone; what is left is what it pointed at.
    Path { res: Res, generics: Vec<Type> },
    Ref { mutable: bool, inner: Box<Type> },
    Dyn(Bound),
    /// Two or more elements; `(T)` is `T` and produces no node.
    Tuple(Vec<Type>),
    Unit,
    /// `Self`, carrying the `impl` or `trait` it stands in for.
    SelfType(Res),
    /// A type the parser could not parse, or a name that did not resolve.
    Error,
}

// --- blocks and statements -----------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// The expression the block evaluates to (§4.3), split out so that no
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
    pub def: DefId,
    pub mutable: bool,
    pub ty: Option<Type>,
    pub value: Expr,
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
    /// `f(a, b)` and `Some(x)`: positional arguments (§4.4).
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// `receiver.method(args)`. The method stays a name: which one it is
    /// depends on the receiver's type, so it is the type checker's to resolve.
    MethodCall { receiver: Box<Expr>, method: Ident, generics: Vec<Type>, args: Vec<Expr> },
    /// `base.name`. The field stays a name, for the same reason.
    Field { base: Box<Expr>, name: Ident },
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `Doc(title: "a")`: named arguments (§4.4). Also where `Doc()` lands
    /// once resolution has seen that `Doc` is a fieldless struct.
    StructLit { res: Res, fields: Vec<FieldInit> },
    Tuple(Vec<Expr>),
    Unit,
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Cast { expr: Box<Expr>, ty: Type },
    Try(Box<Expr>),
    Ref { mutable: bool, expr: Box<Expr> },
    If(IfExpr),
    Match(MatchExpr),
    While { cond: Box<Expr>, body: Block },
    Loop { body: Block },
    For { pattern: Pattern, iter: Box<Expr>, body: Block },
    Block(Block),
    /// An expression that could not be lowered. Its span is kept so a later
    /// phase can still say where the hole is.
    Error,
}

/// `name: value` in a struct construction.
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
    /// `Doc(title: t, body: _)`, and `Doc()` on a fieldless struct.
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
