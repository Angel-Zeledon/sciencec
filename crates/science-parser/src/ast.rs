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

/// A string literal with its span: the ABI of an `extern` block, a library
/// name, the `symbol` a foreign function really has.
///
/// `Literal::Str` carries the same text, but these are not expressions — they
/// are clauses of a declaration, and a node that is not an expression should
/// not have to be wrapped in one to keep its span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrLit {
    pub value: String,
    pub span: Span,
}

/// A dotted path, with optional generic arguments on its last segment.
///
/// One type covers every use: `String`, `Array of Doc`, `text.parser.Token`,
/// the module path of a `use`, and the head of a choice or type pattern.
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
    /// The arguments of `of Doc` or `of (String, Int)`, empty when absent.
    ///
    /// §4.3 puts them after the whole name (`text.parser.Token of Doc`), so in
    /// practice only the last segment ever carries any. The field is on the
    /// segment rather than on the path so that a future qualified form has
    /// somewhere to put them, and so a diagnostic can point at the one name
    /// the arguments belong to.
    pub generics: Vec<Type>,
    pub span: Span,
}

/// A trait named as a bound: in `of T: Ord`, in `any Summarize`, after
/// `implements`.
///
/// It is a path, but naming the wrapper records which of the two a given path
/// was, which later phases would otherwise have to rediscover.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeBound {
    pub path: Path,
    pub span: Span,
}

// --- module and items ----------------------------------------------------

/// A whole source file. In Science a file is a module (§4.4).
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
    /// `type Doc:` — the record type of §4.4.
    Record(RecordDecl),
    /// `choice Option of T:` — the sum type of §4.4.
    Choice(ChoiceDecl),
    /// `type Embedding is Array of F32`.
    Alias(AliasDecl),
    /// `const WIDTH be 768`.
    Const(ConstDecl),
    Interface(InterfaceDecl),
    /// `Doc implements Summarize:` and `Doc has:`.
    Impl(ImplBlock),
    /// `unsafe extern "C" library "openblas":` — §1.1 of the FFI note.
    Extern(ExternBlock),
}

/// `use text.parser` or `use text.parser (Token, lex)`.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: Path,
    /// `None` imports the module itself; `Some` imports the listed names.
    pub imports: Option<Vec<Ident>>,
    pub span: Span,
}

/// One generic parameter of §4.4's `of` list: `T`, `T: Ord + Clone`, or
/// `const WIDTH: Int`.
///
/// Bounds from a `where` clause stay in the clause rather than being folded in
/// here, because a diagnostic has to point at where the programmer wrote them.
#[derive(Debug, Clone, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub kind: GenericParamKind,
    pub span: Span,
}

/// §5.3: a generic parameter is a type or a constant. Const generics are in F0
/// because F1's shapes are built from them, and retrofitting them would mean
/// reopening this enum and everything that matches on it.
#[derive(Debug, Clone, PartialEq)]
pub enum GenericParamKind {
    /// `T`, or `T: Ord + Clone`.
    Type { bounds: Vec<TypeBound> },
    /// `const WIDTH: Int`. The annotation is mandatory: a constant's type is
    /// never inferred from a use site.
    Const { ty: Type },
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
    /// `def largest of T(..)`: §4.4 puts the parameters after the name,
    /// introduced by `of`.
    pub generics: Vec<GenericParam>,
    /// The `self`, `mutable self` or `self: Self` receiver, when there is one.
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    /// `None` means unit, since `->` may be left off entirely (§4.4).
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

/// How a method takes its receiver (§4.4).
///
/// The three forms are the three things that can happen to a value, and the
/// English says which: a bare `self` borrows it, `mutable self` borrows it
/// exclusively, and only the annotated `self: Self` takes it away from the
/// caller. `&self` and `&mut self` are gone with the rest of the sigils.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// `self` — a shared borrow.
    Shared,
    /// `mutable self` — an exclusive borrow.
    Mutable,
    /// `self: Self` — by value, written as an ordinary annotated parameter.
    Value,
}

/// A parameter. The annotation is mandatory (§4.4), so it is not an `Option`.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `type Doc:` with its fields — the record type of §4.4.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordDecl {
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

/// `choice Format:` with its variants — the sum type of §4.4.
#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub where_clause: Vec<WherePredicate>,
    pub variants: Vec<VariantDef>,
    pub span: Span,
}

/// A variant with a positional payload. F0 has no named-field variants (§4.4),
/// so an empty payload is a unit variant.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantDef {
    pub name: Ident,
    pub payload: Vec<Type>,
    pub span: Span,
}

/// `type Embedding is Array of F32` (§4.4).
///
/// The generic parameters are the same `of` list a record takes, so
/// `type Handle of T is Box of T` needs no new grammar.
#[derive(Debug, Clone, PartialEq)]
pub struct AliasDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub ty: Type,
    pub span: Span,
}

/// `const WIDTH be 768` (§4.4).
///
/// The annotation is optional and the value is not, which is exactly a `let`:
/// the two differ in when they are evaluated, not in how they are written.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub ty: Option<Type>,
    pub value: Expr,
    pub span: Span,
}

/// `trait Summarize:` with its members.
///
/// Associated types and methods are kept in two lists rather than one list of
/// members. Their relative order carries no meaning — §5.4 gives an
/// implementation no ordering obligation — and every consumer wants one kind
/// or the other, never the interleaving. Each member keeps its own span, so a
/// diagnostic still points where it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub is_pub: bool,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    /// Interfaces this one requires, written `interface A: B + C`.
    pub supers: Vec<TypeBound>,
    pub where_clause: Vec<WherePredicate>,
    /// `type Item` — declared here, supplied by the implementation (§5.4).
    pub assoc_types: Vec<AssocTypeDecl>,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// `type Item` in an interface body: an associated type the implementer must
/// give.
#[derive(Debug, Clone, PartialEq)]
pub struct AssocTypeDecl {
    pub name: Ident,
    pub span: Span,
}

/// `Doc implements Summarize:` when `interface` is set, `Doc has:` when it is
/// not (§4.4).
///
/// `generics` holds the parameters declared on the head — the `of (A, B)` of
/// `Pair of (A, B) implements Swap:` — and `self_ty` is the type they apply
/// to, with those same names echoed back as its arguments. Keeping the two
/// apart is what lets `Grid of (T, const ROWS: Int) has:` declare a
/// const parameter while `self_ty` stays an ordinary type.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplBlock {
    pub generics: Vec<GenericParam>,
    pub interface: Option<TypeBound>,
    pub self_ty: Type,
    pub where_clause: Vec<WherePredicate>,
    /// `type Item is Int` — the implementation's side of §5.4's associated
    /// types.
    pub assoc_types: Vec<AssocTypeBinding>,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// `type Item is Int` in an implementation body.
#[derive(Debug, Clone, PartialEq)]
pub struct AssocTypeBinding {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

// --- extern blocks -------------------------------------------------------

/// `unsafe extern "C" library "openblas" via pkg-config "openblas":`
///
/// §1.1 of `ffi-c-boundary.md`, and §9 of that note calls it "a small separate
/// grammar with contextual keywords". It is separate here too: an extern item
/// is not an [`Item`], because none of the four forms below is a declaration
/// the ordinary grammar can read. The names they introduce are nevertheless
/// ordinary module-level names, which is name resolution's business and not
/// this tree's.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternBlock {
    /// `unsafe` on the block, which §1.1 requires: writing the declaration is
    /// itself the unsafe act. `false` means it was left off and reported.
    pub is_unsafe: bool,
    /// The ABI as written. `"C"` is the only one this phase accepts.
    pub abi: StrLit,
    /// `None` means the `library` clause was missing and was reported; the
    /// block is kept so the items in it still parse.
    pub library: Option<LibraryClause>,
    pub items: Vec<ExternItem>,
    pub span: Span,
}

/// `library "openblas" via pkg-config "openblas" kind static when available`
/// — §5.1 and §5.2.
///
/// The three optional clauses hang off the library rather than off the block
/// because each of them modifies how that one library is found or linked.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryClause {
    pub name: StrLit,
    /// `via pkg-config "openblas"` (§5.1): where the link flags come from.
    /// The plain name stays as the fallback, which is why this is an extra
    /// clause and not an alternative to one.
    pub pkg_config: Option<StrLit>,
    /// `kind static` (§5.1). Dynamic is the default.
    pub static_link: Option<Span>,
    /// `when available` (§5.2): the block lowers to a `dlopen` table instead
    /// of to direct declarations.
    pub when_available: Option<Span>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternItem {
    pub kind: ExternItemKind,
    pub span: Span,
}

/// What an `extern` block may contain.
///
/// §1.2 of the FFI note admits three forms. `c-binding-coverage.md`'s
/// Decision 7 adds [`ExternItemKind::Static`], without which HDF5 and CPython
/// are unusable, and its §3.4 adds [`ExternItemKind::Union`], the opaque
/// blob a C union is imported as.
#[derive(Debug, Clone, PartialEq)]
pub enum ExternItemKind {
    /// `def dgemm(m: BlasInt, ...) -> Herr symbol "dgemm_"`.
    Fn(ExternFn),
    /// `type BlasInt is I32` — a C typedef over an FFI-representable type.
    Alias(ExternAlias),
    /// `const CBLAS_ROW_MAJOR be 101 as CblasLayout` — a `#define` or an
    /// enumerator, transcribed.
    Const(ExternConst),
    /// `static H5T_NATIVE_DOUBLE_g: Hid` — an exported data symbol.
    Static(ExternStatic),
    /// `union H5R_ref_t: size 64 align 8` — an opaque blob of the right size
    /// and alignment.
    Union(ExternUnion),
    /// An item form the block does not have. Kept so the block still shows
    /// what parsed around it.
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternFn {
    pub name: Ident,
    /// Named, because §1.7 permits named arguments at extern call sites and
    /// the names are what makes a thirteen-argument `dgemm` readable.
    pub params: Vec<Param>,
    /// `None` means the function returns unit.
    pub ret: Option<Type>,
    /// `symbol "dgemm_"` (§1.6): the linker name, when it is not the Science
    /// name. This is what makes the ILP64 hazard a link error.
    pub symbol: Option<StrLit>,
    /// The span of a `...` in the parameter list. §1.4 refuses to represent
    /// C's variadic convention, so this is only ever recorded in order to be
    /// reported; it is kept in the tree so that a later phase cannot mistake
    /// the declaration for a complete one.
    pub variadic: Option<Span>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternAlias {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `const NAME be literal as T`.
///
/// The value is a literal, not an expression: §1.2 says a constant in a block
/// transcribes a `#define`, and Decision 7 turns on the distinction between a
/// compile-time literal and a link-time symbol. A leading `-` is admitted
/// because C enumerators are routinely negative (`H5I_BADID`, `Herr`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExternConst {
    pub name: Ident,
    pub negative: bool,
    pub value: Literal,
    pub ty: Type,
    pub span: Span,
}

/// `static H5T_NATIVE_DOUBLE_g: Hid` — `c-binding-coverage.md` Decision 7.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternStatic {
    pub name: Ident,
    pub ty: Type,
    pub span: Span,
}

/// `union H5L_info2_t: size 32 align 8`.
///
/// A C union has no Science layout, so what is imported is the opaque byte
/// array `c-binding-coverage.md` §3.4 names: the size and the alignment, and
/// nothing about the arms. The library's own accessors are ordinary function
/// items beside it.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternUnion {
    pub name: Ident,
    /// `None` when the clause was missing and was reported.
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
    /// `String`, `Array of T`, `text.parser.Token`, and a bare type parameter.
    Path(Path),
    /// `borrowed T` and `mutable borrowed T`. There is no region to write:
    /// §6.1 says the programmer never writes one.
    Borrowed { mutable: bool, inner: Box<Type> },
    /// `any Trait`. Legal only behind an indirection (§4.3); the parser accepts
    /// it anywhere and lets a later phase say so with a better message.
    Any(TypeBound),
    /// `(A, B)`, always two or more elements. `(T)` is just `T` and produces no
    /// node of its own.
    Tuple(Vec<Type>),
    /// `()`.
    Unit,
    /// `T?` — a nullable type (revision 2 §3.1): `T`, or `null`. It replaced
    /// `Option of T`.
    ///
    /// `Error?` in a return position is shorthand for `(any Error)?`, an
    /// optional trait object. The shorthand is not expanded here: the parser
    /// records what was written and resolution decides what `Error` names,
    /// because expanding it would make the parser depend on one name in the
    /// prelude.
    Nullable(Box<Type>),
    /// `Self`.
    SelfType,
    /// `Self.Item`: the associated type `Item` of the type being implemented
    /// (§5.4). Only `Self` can be the base in F0 — a projection through a
    /// generic parameter would need the qualified form Rust spells
    /// `<T as Iterate>::Item`, and F0 has no syntax for it.
    SelfAssoc(Ident),
    /// The `4` in `Window of (Int, 4)`: a const generic argument (§5.3). It is
    /// a value in an argument list that otherwise holds types, which is why it
    /// lives in `TypeKind` rather than anywhere more comfortable.
    Const(ConstExpr),
    /// A type that failed to parse. Keeps the tree shaped so later phases can
    /// run instead of the parser having to bail out.
    Error,
}

/// The value of a const generic argument: the `4` of `Window of (Int, 4)` and
/// the `-1` of `Quantity of (T, 1, 0, -1, 0, 0, 0, 0)`.
///
/// `const-expression-arithmetic.md` §2.1 gives this position a five-operator
/// grammar — `+`, binary `-`, unary `-`, `*` and `/`, with a literal required
/// on one side of `*` and `/` — and §10.1 states two of its F0 commitments as
/// this node:
///
/// > 1. **The const-expression grammar of §2.1 parses in type-argument
/// >    position**, including unary negation.
/// > 2. **The const-argument node carries a signed integer.**
///
/// What exists today is the unary-negation half of commitment 1, and nothing
/// else: without it none of `scientific-libraries.md` §12.3's nine unit
/// aliases parse, because roughly half of every SI dimension vector is
/// negative. The rest of the grammar arrives as further variants of
/// [`ConstExprKind`] — that is the whole reason this is a node and not a
/// `negated: bool` beside a `Literal`. Adding `Add`, `Mul` and `Div` is then
/// adding arms to an enum the parser, the dump and the resolver already walk
/// as a tree, rather than replacing a representation that never was one.
///
/// Commitment 2 is [`ConstExpr::as_i128`]: the sign lives in [`ConstExprKind::Neg`]
/// rather than in the literal (whose `u128` cannot hold it), and the signed
/// value a const argument denotes is read out in exactly one place.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstExpr {
    pub kind: ConstExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstExprKind {
    /// An atom: the `4` of `Window of (Int, 4)`.
    ///
    /// Any [`Literal`], not just an integer one. The parser has always
    /// accepted `Window of (Int, "a")` here and let a later phase say what is
    /// wrong with it — `const-expression-arithmetic.md` §2.3 is that phase,
    /// and it reports `SC0260` naming the two admissible kinds.
    Lit(Literal),
    /// `-` applied to a const expression: the `-1` of a dimension vector.
    ///
    /// The operand keeps its own span, so a diagnostic can point at the
    /// literal, at the `-`, or at the whole negated term.
    Neg(Box<ConstExpr>),
}

impl ConstExpr {
    /// The signed integer this const argument denotes, or `None` when it does
    /// not denote one.
    ///
    /// This is F0 commitment 2. [`Literal::Int`] holds a `u128` because that
    /// is what the lexer accumulates, and a const argument is signed, so the
    /// representation of a const argument's *value* is `i128` and this is the
    /// single place that says so. `-2^127` is therefore the most negative
    /// value a const argument has; a larger magnitude is `None` here rather
    /// than a wrong number somewhere downstream.
    ///
    /// `None` also covers the const arguments that are not integers at all —
    /// a float, a string, a character, a bool — which parse (see
    /// [`ConstExprKind::Lit`]) and are rejected by kind-checking, not here.
    pub fn as_i128(&self) -> Option<i128> {
        /// The magnitude of `i128::MIN`, which is one past `i128::MAX`.
        const MIN_MAGNITUDE: u128 = i128::MAX as u128 + 1;

        match &self.kind {
            ConstExprKind::Lit(Literal::Int { value, .. }) => i128::try_from(*value).ok(),
            ConstExprKind::Lit(_) => None,
            ConstExprKind::Neg(operand) => match &operand.kind {
                // `-(2^127)` is representable although `+2^127` is not, so a
                // negated literal is negated from its unsigned magnitude and
                // never through an `i128` that cannot hold it.
                ConstExprKind::Lit(Literal::Int { value, .. }) if *value <= MIN_MAGNITUDE => {
                    Some((*value as i128).wrapping_neg())
                }
                ConstExprKind::Lit(_) => None,
                _ => operand.as_i128()?.checked_neg(),
            },
        }
    }
}

// --- blocks and statements -----------------------------------------------

/// A sequence of statements, optionally ending in an expression that is the
/// block's value (§4.4: a function's value is its last expression).
///
/// The tail is split out rather than left as the last statement so that later
/// phases never have to re-derive which expression the block evaluates to.
///
/// Both block forms of §4.5 produce this node: the indented
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
    /// `target be value`. §4.6's precedence table has no assignment in it at
    /// all: assignment is a statement, not an operator; see the note in
    /// `parser.rs`.
    Assign { target: Expr, value: Expr },
    Return(Option<Expr>),
    Break(Option<Expr>),
    Continue,
    /// A statement that failed to parse.
    Error,
}

/// `let x be e`, `let mutable x be e`, `let x: T be e`, and revision 2
/// §3.1's `let value, err be f()`.
///
/// The bound names are still `Ident`s and not patterns, and the list is flat.
///
/// The comment this replaced said §4.5 had *closed* `let` to destructuring.
/// It never did — neither revision of §4.5 contains such a rule, and the
/// prohibition was inferred from an absence. §4.5 now writes
/// `let text, err be read_file(path)` outright, so the flat list is the
/// amendment rather than an exception to one.
///
/// Three corners of it are written nowhere, and each is decided here rather
/// than guessed at by a later phase:
///
/// - **The grammar is narrow.** Names only. Nesting, literals and variant
///   patterns stay where §4.7 put them, in `match`, which is the form that
///   can be checked for exhaustiveness.
/// - **`mutable` distributes over every name.** The list receives one tuple,
///   and a form where half the names were mutable would need a second
///   `mutable` in a position nothing else in the language puts one.
/// - **Ascription is per name**, so `let text: String, err: Error? be f()`
///   says which type is whose. §3.1 shows ascription and destructuring as
///   separate forms and never combines them.
#[derive(Debug, Clone, PartialEq)]
pub struct LetStmt {
    pub mutable: bool,
    /// One name, or two or more for a tuple destructure. Never empty: a
    /// `let` with no name does not parse.
    pub names: Vec<LetName>,
    pub value: Expr,
    pub span: Span,
}

/// One name bound by a `let`, with the type the author wrote for it.
///
/// The annotation is per name rather than per statement so that
/// `let text: String, err: Error? be read(path)` says which type is whose.
/// A single binding is the same node with a one-element list.
#[derive(Debug, Clone, PartialEq)]
pub struct LetName {
    pub name: Ident,
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
    /// A name, possibly qualified and possibly with generic arguments.
    Path(Path),
    /// `self`.
    SelfValue,
    /// `f(a, b)`. Struct construction with named arguments is `StructLit`.
    Call { callee: Box<Expr>, args: Vec<Arg> },
    /// `receiver.method(args)`.
    MethodCall {
        receiver: Box<Expr>,
        method: Ident,
        generics: Vec<Type>,
        args: Vec<Arg>,
    },
    /// `base.name`.
    Field { base: Box<Expr>, name: Ident },
    /// `base[index]`.
    Index { base: Box<Expr>, index: Box<Expr> },
    /// `Doc(title: "a", body: "b")` — construction with named arguments, which
    /// §4.4 makes mandatory for records. Syntactically this is a call whose
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
    /// `e?` — the presence test of revision 2 §3.1. Evaluates to a `Bool`,
    /// true when the operand is not `null`. It is postfix and it binds on the
    /// same rung as call, index and field access, so `f().x?` is `(f().x)?`
    /// and covering less takes parentheses.
    ///
    /// This node replaced `Try`, and the two are not the same shape: `try`
    /// was a prefix that could return from the enclosing function, and `?`
    /// is a total operator that returns a value and can do nothing else.
    Present(Box<Expr>),
    /// `borrowed e` and `mutable borrowed e`. Auto-borrow (§6.3) means a call
    /// rarely needs either, and both stay legal where they clarify.
    Borrowed { mutable: bool, expr: Box<Expr> },
    /// `0..n` and `0..=n` (§4.5).
    Range { start: Box<Expr>, end: Box<Expr>, inclusive: bool },
    /// The two closure forms of §4.6. `param: None` is the implicit-subject
    /// form `each.title`, whose body mentions [`ExprKind::Each`]; `Some(name)`
    /// is `doc giving doc.title`.
    Closure { param: Option<Ident>, body: Box<Expr> },
    /// `each`, the undeclared subject of the enclosing call (§4.6).
    Each,
    If(IfExpr),
    Match(MatchExpr),
    Loop { body: Block },
    For { pattern: Pattern, iter: Box<Expr>, body: Block },
    /// `unsafe:` with a body — §3 of the FFI note. It is an expression and
    /// not a statement for the same reason `if` is: it has the value of its
    /// block, and a foreign call is usually the whole of it.
    Unsafe(Block),
    /// A bare block used as an expression.
    Block(Block),
    /// An expression that failed to parse.
    Error,
}

/// One argument of a call.
///
/// The name is `Some` only for `docs.sort(by: doc giving ..)` (§4.6), where
/// the receiver is not a path and so no record construction could be meant.
/// Where the callee *is* a path, named arguments construct a record and the
/// call becomes a `StructLit` instead; see `parser.rs`.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg {
    pub name: Option<Ident>,
    pub value: Expr,
    pub span: Span,
}

/// `name: value` inside a record construction.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldInit {
    pub name: Ident,
    pub value: Expr,
    pub span: Span,
}

/// `if`/`else` is an expression (§4.5).
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
    /// `null`. A literal rather than a prelude value, because `T?` is a type
    /// the compiler knows and what inhabits it has to be known with it.
    Null,
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

/// The binary operators of §4.6's precedence table.
///
/// A comparison has two spellings — `a is b` and `a == b` — and exactly one
/// variant here, which is §5.4's requirement that the phrase and the symbol be
/// the same operator "by construction, not by a parser rule that could drift".
/// `as_str` prints the symbol form for both, because the trait method they
/// dispatch to has one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    /// `**`, right-associative (§4.6).
    Pow,
    /// `@`, matrix multiplication.
    MatMul,
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
            Pow => "**",
            MatMul => "@",
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
    /// A bare name. A unit choice variant such as `None` also lands here: only
    /// name resolution can tell a binding from a variant, and that is not the
    /// parser's job.
    Binding { mutable: bool, name: Ident },
    /// `Ok(value)`, and `None` once resolution has reclassified it.
    Variant { path: Path, elems: Vec<Pattern> },
    /// `Doc(title: t)` — a record pattern, matching the named-argument form
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
