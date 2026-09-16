//! The Science syntax tree.
//!
//! Every node carries its own `Span`, without exception — §9 of the design
//! spec makes that non-negotiable, and a span lost here is an error that is
//! impossible to locate three phases later.
//!
//! The shape follows one rule throughout: anything that varies in kind is an
//! enum `XKind` wrapped in a struct `X { kind, span }`. The span then lives in
//! exactly one place instead of being repeated in every variant, and every
//! traversal can ask any node where it came from without matching on it first.
//! Nodes whose kind never varies (a parameter, a field, a match arm) are plain
//! structs with a `span` field.
//!
//! Names and paths are kept as written. This tree is the *syntax*, so no
//! resolution happens here: `None` in a pattern is a `Binding` until name
//! resolution says otherwise, and `Doc(title: "a")` is a `StructLit` purely
//! because it has named arguments.

use science_diagnostics::Span;
use science_lexer::{IntBase, NumSuffix};

// --- names ---------------------------------------------------------------

/// An identifier as it appears in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

impl Ident {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Ident { name: name.into(), span }
    }
}

/// A dotted path, with optional generic arguments on any segment.
///
/// One type covers every use: `String`, `Array[T]`, `text.parser.Token`, the
/// module path of a `use`, and the head of an enum or struct pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

impl Path {
    /// The path written back out, e.g. `text.parser.Token`. Generic arguments
    /// are left out; they live on the segments.
    pub fn dotted(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.name.name.as_str())
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathSegment {
    pub name: Ident,
    /// `[A, B]`, empty when absent.
    pub generics: Vec<Type>,
    pub span: Span,
}

/// A trait named as a bound: in `T: Ord`, in `dyn Summarize`, after `impl`.
///
/// It is a path, but naming the wrapper records which of the two a given path
/// was, which later phases would otherwise have to rediscover.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBound {
    pub path: Path,
    pub span: Span,
}

// --- module and items ----------------------------------------------------

/// A whole source file. In Science a file is a module (§4.3).
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    Use(UseDecl),
    Fn(FnDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Trait(TraitDecl),
    Impl(ImplBlock),
}

/// `use text.parser` or `use text.parser (Token, lex)`.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: Path,
    /// `None` imports the module itself; `Some` imports the listed names.
    pub imports: Option<Vec<Ident>>,
    pub span: Span,
}

/// One generic parameter, with the bounds written inline: `T: Ord + Clone`.
///
/// Bounds from a `where` clause stay in the clause rather than being folded in
/// here, because a diagnostic has to point at where the programmer wrote them.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub bounds: Vec<TypeBound>,
    pub span: Span,
}

/// One predicate of a `where` clause: `T: Summarize + Clone`.
#[derive(Debug, Clone, PartialEq)]
pub struct WherePredicate {
    pub ty: Type,
    pub bounds: Vec<TypeBound>,
    pub span: Span,
}

/// A function, a method, or a trait's required method.
///
/// All three are the same node. `body: None` is a signature without a body,
/// which is what a trait's required method looks like; a trait method with a
/// default body is just the same node with `body: Some(..)`.
#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    /// The `self`, `&self` or `&mut self` receiver, when there is one.
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    /// `None` means unit, per §4.3.
    pub ret: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    pub body: Option<Block>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfParam {
    pub kind: SelfKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// `self`
    Value,
    /// `&self`
    Ref,
    /// `&mut self`
    RefMut,
}

/// A parameter. The annotation is mandatory (§4.3), so it is not an `Option`.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub fields: Vec<FieldDef>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub is_pub: bool,
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

/// A variant with a positional payload. F0 has no named-field variants (§4.3),
/// so an empty payload is a unit variant.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantDef {
    pub name: Ident,
    pub payload: Vec<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    /// Traits this one requires, written `trait A: B + C`.
    pub supertraits: Vec<TypeBound>,
    pub where_clause: Vec<WherePredicate>,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// `impl Trait for Type` when `trait_` is set, `impl Type` when it is not.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplBlock {
    pub generics: Vec<GenericParam>,
    pub trait_: Option<TypeBound>,
    pub self_ty: Type,
    pub where_clause: Vec<WherePredicate>,
    pub methods: Vec<FnDecl>,
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
    /// `String`, `Array[T]`, `text.parser.Token`, and a bare type parameter.
    Path(Path),
    /// `&T` and `&mut T`.
    Ref { mutable: bool, inner: Box<Type> },
    /// `dyn Trait`. Legal only behind an indirection (§4.3); the parser accepts
    /// it anywhere and lets a later phase say so with a better message.
    Dyn(TypeBound),
    /// `(A, B)`, always two or more elements. `(T)` is just `T` and produces no
    /// node of its own.
    Tuple(Vec<Type>),
    /// `()`.
    Unit,
    /// `Self`.
    SelfType,
    /// A type that failed to parse. Keeps the tree shaped so later phases can
    /// run instead of the parser having to bail out.
    Error,
}

// --- blocks and statements -----------------------------------------------

/// A sequence of statements, optionally ending in an expression that is the
/// block's value (§4.3: a function's value is its last expression).
///
/// The tail is split out rather than left as the last statement so that later
/// phases never have to re-derive which expression the block evaluates to.
///
/// Both block forms of §4.2 produce this node: the indented
/// `:` NEWLINE INDENT .. DEDENT form, and the single-expression inline form,
/// which comes out as an empty `stmts` with a `tail`.
///
/// `span` covers the block's *contents*, not the `:` that introduces it.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
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
    Let(LetStmt),
    /// An expression evaluated for its effect.
    Expr(Expr),
    /// `target = value`. §4.4 lists `=` in the precedence table, but it is
    /// non-associative and cannot appear nested in a useful way, so it is a
    /// statement here; see the note in `parser.rs`.
    Assign { target: Expr, value: Expr },
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    /// A statement that failed to parse.
    Error,
}

/// `let x = e`, `let mut x = e`, `let x: T = e`.
///
/// F0 has no destructuring in `let` (§4.3), so the bound name is an `Ident`
/// rather than a pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct LetStmt {
    pub mutable: bool,
    pub name: Ident,
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
    /// A name, possibly qualified and possibly with generic arguments.
    Path(Path),
    /// `self`.
    SelfValue,
    /// `f(a, b)`. Struct construction with named arguments is `StructLit`.
    Call { callee: Box<Expr>, args: Vec<Expr> },
    /// `receiver.method(args)`.
    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        generics: Vec<Type>,
        args: Vec<Expr>,
    },
    /// `base.name`.
    Field { base: Box<Expr>, name: Ident },
    /// `base[index]`.
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `Doc(title: "a", body: "b")` — construction with named arguments, which
    /// §4.3 makes mandatory for structs. Syntactically this is a call whose
    /// arguments are named; nothing but the names distinguishes the two.
    StructLit { path: Path, fields: Vec<FieldInit> },
    /// `(a, b)`, two or more elements.
    Tuple(Vec<Expr>),
    /// `()`.
    Unit,
    Unary { op: UnaryOp, operand: Box<Expr> },
    Binary { op: BinaryOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// `e as T`.
    Cast { expr: Box<Expr>, ty: Type },
    /// `e?`.
    Try(Box<Expr>),
    /// `&e` and `&mut e`.
    Ref { mutable: bool, expr: Box<Expr> },
    If(IfExpr),
    Match(MatchExpr),
    While { cond: Box<Expr>, body: Block },
    Loop { body: Block },
    For { pattern: Pattern, iter: Box<Expr>, body: Block },
    /// A bare block used as an expression.
    Block(Block),
    /// An expression that failed to parse.
    Error,
}

/// `name: value` inside a struct construction.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

/// `if`/`else` is an expression (§4.4).
///
/// `else_branch` is an `Expr` rather than a `Block` so that `else if` chains
/// are just a nested `If`; a plain `else:` gives an `ExprKind::Block`.
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

/// `pattern: body`. The body is an `Expr`, so an arm with an indented block
/// carries an `ExprKind::Block` and an inline arm carries the expression.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

/// A literal, with the lexer's own representation kept intact: the base and the
/// suffix are part of the source and diagnostics need them back.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int { value: u128, base: IntBase, suffix: Option<NumSuffix> },
    Float { value: f64, suffix: Option<NumSuffix> },
    Str(String),
    Char(char),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-`
    Neg,
    /// `not`
    Not,
}

impl UnaryOp {
    pub fn as_str(self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "not",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitXor,
    BitOr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl BinaryOp {
    pub fn as_str(self) -> &'static str {
        use BinaryOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            Shl => "<<",
            Shr => ">>",
            BitAnd => "&",
            BitXor => "^",
            BitOr => "|",
            Eq => "==",
            Ne => "!=",
            Lt => "<",
            Gt => ">",
            Le => "<=",
            Ge => ">=",
            And => "and",
            Or => "or",
        }
    }
}

// --- patterns ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// `_`
    Wildcard,
    Literal(Literal),
    /// A bare name. A unit enum variant such as `None` also lands here: only
    /// name resolution can tell a binding from a variant, and that is not the
    /// parser's job.
    Binding { mutable: bool, name: Ident },
    /// `Ok(value)`, and `None` once resolution has reclassified it.
    Variant { path: Path, elems: Vec<Pattern> },
    /// `Doc(title: t)` — a struct pattern, matching the named-argument form
    /// that constructs one.
    Struct { path: Path, fields: Vec<FieldPattern> },
    /// `(a, b)`, two or more elements.
    Tuple(Vec<Pattern>),
    /// `()`
    Unit,
    /// `A | B | C`
    Or(Vec<Pattern>),
    /// A pattern that failed to parse.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldPattern {
    pub name: Ident,
    pub pattern: Pattern,
    pub span: Span,
}
