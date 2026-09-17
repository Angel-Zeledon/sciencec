//! Builders that hand the resolver an AST, and the report a snapshot records.
//!
//! These tests do not call the parser, and there is no `.science` source
//! anywhere under this crate: a resolver test that went through the parser
//! would be a test of two phases at once, and it would only be able to reach
//! the trees the current surface syntax happens to produce. The AST is the
//! contract, so the tests build one directly, the way the parser's own tests
//! build token streams instead of calling the lexer.
//!
//! Spans come from [`Sp`], a counter that hands out a fresh, non-overlapping
//! range per node. The numbers are arbitrary; what matters is that they are
//! deterministic, that no two nodes share one, and that a snapshot diff shows
//! immediately when a span moves to the wrong node.

#![allow(dead_code)]

use std::cell::Cell;

use science_diagnostics::{Diagnostics, FileId, Span};
use science_lexer::IntBase;
use science_parser::ast::*;

pub const FILE: FileId = FileId(0);

/// Hands out a fresh span per node, laid out end to end with a one-byte gap.
pub struct Sp {
    next: Cell<u32>,
}

impl Default for Sp {
    fn default() -> Self {
        Self::new()
    }
}

impl Sp {
    pub fn new() -> Self {
        Sp { next: Cell::new(0) }
    }

    /// A span `len` bytes wide, starting after everything handed out so far.
    pub fn take(&self, len: u32) -> Span {
        let start = self.next.get();
        self.next.set(start + len + 1);
        Span::new(FILE, start, start + len)
    }

    /// A span wide enough for a word, sized from the word itself so that a
    /// dump reads plausibly next to the source it stands for.
    pub fn word(&self, text: &str) -> Span {
        self.take(text.len() as u32)
    }
}

// --- names ---------------------------------------------------------------

pub fn ident(sp: &Sp, name: &str) -> Ident {
    let span = sp.word(name);
    Ident::new(name, span)
}

/// A path, one segment per name, with no generic arguments.
pub fn path(sp: &Sp, segments: &[&str]) -> Path {
    let segs: Vec<PathSegment> = segments
        .iter()
        .map(|name| {
            let name = ident(sp, name);
            let span = name.span;
            PathSegment { name, generics: Vec::new(), span }
        })
        .collect();
    let span = merge_all(&segs.iter().map(|s| s.span).collect::<Vec<_>>());
    Path { segments: segs, span }
}

/// A one-segment path carrying generic arguments: `Array[T]`.
pub fn path_generic(sp: &Sp, name: &str, generics: Vec<Type>) -> Path {
    let name = ident(sp, name);
    let span = name.span;
    Path { segments: vec![PathSegment { name, generics, span }], span }
}

pub fn bound(sp: &Sp, name: &str) -> TypeBound {
    let path = path(sp, &[name]);
    let span = path.span;
    TypeBound { kind: TypeBoundKind::Interface(path), span }
}

/// `(A) -> B` where a bound belongs (`collections-and-chains.md` §1.2). The
/// other kind of bound, and the reason `resolve_bound` has two cases.
pub fn bound_closure(params: Vec<Type>, ret: Type) -> TypeBound {
    let span = merge_all(
        &params.iter().map(|t| t.span).chain(std::iter::once(ret.span)).collect::<Vec<_>>(),
    );
    TypeBound { kind: TypeBoundKind::Closure { params, ret: Box::new(ret) }, span }
}

fn merge_all(spans: &[Span]) -> Span {
    let mut it = spans.iter().copied();
    let first = it.next().expect("a node with no span");
    it.fold(first, Span::merge)
}

// --- types ---------------------------------------------------------------

pub fn ty(sp: &Sp, name: &str) -> Type {
    let path = path(sp, &[name]);
    let span = path.span;
    Type { kind: TypeKind::Path(path), span }
}

pub fn ty_path(sp: &Sp, segments: &[&str]) -> Type {
    let path = path(sp, segments);
    let span = path.span;
    Type { kind: TypeKind::Path(path), span }
}

pub fn ty_generic(sp: &Sp, name: &str, args: Vec<Type>) -> Type {
    let path = path_generic(sp, name, args);
    let span = path.span;
    Type { kind: TypeKind::Path(path), span }
}

pub fn ty_borrowed(sp: &Sp, mutable: bool, inner: Type) -> Type {
    let span = sp.take(1).merge(inner.span);
    Type { kind: TypeKind::Borrowed { mutable, inner: Box::new(inner) }, span }
}

pub fn ty_any(sp: &Sp, name: &str) -> Type {
    let bound = bound(sp, name);
    let span = bound.span;
    Type { kind: TypeKind::Any(bound), span }
}

/// `Self.Item` (§5.4).
pub fn ty_self_assoc(sp: &Sp, name: &str) -> Type {
    let name = ident(sp, name);
    let span = name.span;
    Type { kind: TypeKind::SelfAssoc(name), span }
}

/// The `4` of `Window of (Int, 4)`: a const generic argument (§5.3).
pub fn ty_const_int(sp: &Sp, value: u128) -> Type {
    let span = sp.take(value.to_string().len() as u32);
    let literal =
        Literal::Int { value, base: science_lexer::IntBase::Dec, suffix: None };
    Type { kind: TypeKind::Const(ConstExpr { kind: ConstExprKind::Lit(literal), span }), span }
}

/// The `-1` of `Quantity of (T, 1, 0, -1, 0, 0, 0, 0)`: the same, negated.
/// The `-` and the digits each get their own span, as the parser gives them.
pub fn ty_const_neg_int(sp: &Sp, magnitude: u128) -> Type {
    let span = sp.take(magnitude.to_string().len() as u32 + 1);
    let digits = Span::new(FILE, span.start + 1, span.end);
    let literal =
        Literal::Int { value: magnitude, base: science_lexer::IntBase::Dec, suffix: None };
    let operand = ConstExpr { kind: ConstExprKind::Lit(literal), span: digits };
    let value = ConstExpr { kind: ConstExprKind::Neg(Box::new(operand)), span };
    Type { kind: TypeKind::Const(value), span }
}

pub fn ty_self(sp: &Sp) -> Type {
    Type { kind: TypeKind::SelfType, span: sp.word("Self") }
}

pub fn ty_unit(sp: &Sp) -> Type {
    Type { kind: TypeKind::Unit, span: sp.take(2) }
}

/// `(A) -> B` in type position (`collections-and-chains.md` §1.2).
pub fn ty_closure(params: Vec<Type>, ret: Type) -> Type {
    let span = merge_all(
        &params.iter().map(|t| t.span).chain(std::iter::once(ret.span)).collect::<Vec<_>>(),
    );
    Type { kind: TypeKind::Closure { params, ret: Box::new(ret) }, span }
}

// --- items ---------------------------------------------------------------

pub fn module(items: Vec<Item>) -> Module {
    let span = if items.is_empty() {
        Span::at(FILE, 0)
    } else {
        merge_all(&items.iter().map(|i| i.span).collect::<Vec<_>>())
    };
    Module { items, span }
}

fn item(kind: ItemKind, span: Span) -> Item {
    Item { kind, span, doc: None }
}

pub fn generic(sp: &Sp, name: &str, bounds: Vec<TypeBound>) -> GenericParam {
    let name = ident(sp, name);
    let span = name.span;
    GenericParam { name, kind: GenericParamKind::Type { bounds }, span }
}

/// `const ROWS: Int` (§5.3).
pub fn generic_const(sp: &Sp, name: &str, ty: Type) -> GenericParam {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    GenericParam { name, kind: GenericParamKind::Const { ty }, span }
}

pub fn param(sp: &Sp, name: &str, ty: Type) -> Param {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    // `doc` is Decision 7's per-parameter description, which only a `tool`
    // may carry; these helpers build `def`s.
    Param { name, ty, doc: None, span }
}

/// A function declaration, with everything optional defaulted away.
pub struct FnBuilder {
    decl: FnDecl,
}

pub fn func(sp: &Sp, name: &str) -> FnBuilder {
    let name = ident(sp, name);
    let span = name.span;
    FnBuilder {
        decl: FnDecl {
            form: FnForm::Def,
            is_pub: false,
            name,
            generics: Vec::new(),
            self_param: None,
            params: Vec::new(),
            ret: None,
            where_clause: Vec::new(),
            body: None,
            span,
        },
    }
}

impl FnBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.decl.generics = generics;
        self
    }

    pub fn receiver(mut self, sp: &Sp, kind: SelfKind) -> Self {
        self.decl.self_param = Some(SelfParam { kind, span: sp.word("self") });
        self
    }

    pub fn params(mut self, params: Vec<Param>) -> Self {
        self.decl.params = params;
        self
    }

    pub fn ret(mut self, ty: Type) -> Self {
        self.decl.ret = Some(ty);
        self
    }

    pub fn where_clause(mut self, predicates: Vec<WherePredicate>) -> Self {
        self.decl.where_clause = predicates;
        self
    }

    pub fn body(mut self, body: Block) -> Self {
        self.decl.body = Some(body);
        self
    }

    /// The declaration on its own, for a method inside a trait or an impl.
    pub fn decl(mut self) -> FnDecl {
        self.decl.span = self.decl.span.merge(self.extent());
        self.decl
    }

    /// The declaration wrapped as a module item.
    pub fn item(self) -> Item {
        let decl = self.decl();
        let span = decl.span;
        item(ItemKind::Fn(decl), span)
    }

    fn extent(&self) -> Span {
        let mut span = self.decl.name.span;
        for p in &self.decl.params {
            span = span.merge(p.span);
        }
        if let Some(ret) = &self.decl.ret {
            span = span.merge(ret.span);
        }
        if let Some(body) = &self.decl.body {
            span = span.merge(body.span);
        }
        span
    }
}

pub fn field(sp: &Sp, name: &str, ty: Type) -> FieldDef {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    FieldDef { is_pub: false, name, ty, span }
}

pub fn record_item(
    sp: &Sp,
    name: &str,
    generics: Vec<GenericParam>,
    fields: Vec<FieldDef>,
) -> Item {
    let name = ident(sp, name);
    let mut span = name.span;
    for f in &fields {
        span = span.merge(f.span);
    }
    let decl = RecordDecl {
        is_pub: false,
        name,
        generics,
        where_clause: Vec::new(),
        fields,
        span,
    };
    item(ItemKind::Record(decl), span)
}

/// `type Embedding is Array of F32` (§4.4).
pub fn alias_item(sp: &Sp, name: &str, generics: Vec<GenericParam>, ty: Type) -> Item {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    item(ItemKind::Alias(AliasDecl { is_pub: false, name, generics, ty, span }), span)
}

/// `const WIDTH be 768` (§4.4).
pub fn const_item(sp: &Sp, name: &str, ty: Option<Type>, value: Expr) -> Item {
    let name = ident(sp, name);
    let span = name.span.merge(value.span);
    item(ItemKind::Const(ConstDecl { is_pub: false, name, ty, value, span }), span)
}

pub fn variant(sp: &Sp, name: &str, payload: Vec<Type>) -> VariantDef {
    let name = ident(sp, name);
    let mut span = name.span;
    for t in &payload {
        span = span.merge(t.span);
    }
    VariantDef { name, payload, span }
}

pub fn choice_item(
    sp: &Sp,
    name: &str,
    generics: Vec<GenericParam>,
    variants: Vec<VariantDef>,
) -> Item {
    let name = ident(sp, name);
    let mut span = name.span;
    for v in &variants {
        span = span.merge(v.span);
    }
    let decl = ChoiceDecl {
        is_pub: false,
        name,
        generics,
        where_clause: Vec::new(),
        variants,
        span,
    };
    item(ItemKind::Choice(decl), span)
}

pub fn interface_item(
    sp: &Sp,
    name: &str,
    generics: Vec<GenericParam>,
    methods: Vec<FnDecl>,
) -> Item {
    interface_item_assoc(sp, name, generics, &[], methods)
}

/// An interface that also declares associated types: `type Item` (§5.4).
pub fn interface_item_assoc(
    sp: &Sp,
    name: &str,
    generics: Vec<GenericParam>,
    assoc: &[&str],
    methods: Vec<FnDecl>,
) -> Item {
    let name = ident(sp, name);
    let mut span = name.span;
    let assoc_types: Vec<AssocTypeDecl> = assoc
        .iter()
        .map(|n| {
            let name = ident(sp, n);
            let span = name.span;
            AssocTypeDecl { name, span }
        })
        .collect();
    for a in &assoc_types {
        span = span.merge(a.span);
    }
    for m in &methods {
        span = span.merge(m.span);
    }
    let decl = InterfaceDecl {
        is_pub: false,
        name,
        generics,
        supers: Vec::new(),
        where_clause: Vec::new(),
        assoc_types,
        methods,
        span,
    };
    item(ItemKind::Interface(decl), span)
}

pub fn impl_item(
    sp: &Sp,
    generics: Vec<GenericParam>,
    interface: Option<TypeBound>,
    self_ty: Type,
    methods: Vec<FnDecl>,
) -> Item {
    impl_item_assoc(sp, generics, interface, self_ty, vec![], methods)
}

/// An implementation that also binds associated types: `type Item is Int`.
pub fn impl_item_assoc(
    sp: &Sp,
    generics: Vec<GenericParam>,
    interface: Option<TypeBound>,
    self_ty: Type,
    assoc: Vec<(&str, Type)>,
    methods: Vec<FnDecl>,
) -> Item {
    let mut span = sp.word("implements").merge(self_ty.span);
    let assoc_types: Vec<AssocTypeBinding> = assoc
        .into_iter()
        .map(|(n, ty)| {
            let name = ident(sp, n);
            let span = name.span.merge(ty.span);
            AssocTypeBinding { name, ty, span }
        })
        .collect();
    for a in &assoc_types {
        span = span.merge(a.span);
    }
    for m in &methods {
        span = span.merge(m.span);
    }
    let block = ImplBlock {
        generics,
        interface,
        self_ty,
        where_clause: Vec::new(),
        assoc_types,
        methods,
        span,
    };
    item(ItemKind::Impl(block), span)
}

// --- extern blocks -------------------------------------------------------

/// `unsafe extern "C" library <name>:` with the items given.
///
/// The `library` clause is always written: a block without one is the
/// parser's diagnostic, not the resolver's, and a builder that made it easy
/// to omit would be inviting a test about the wrong phase.
pub fn extern_item(sp: &Sp, library: &str, items: Vec<ExternItem>) -> Item {
    let start = sp.word("extern");
    let abi = StrLit { value: "C".to_string(), span: sp.word("\"C\"") };
    let name = StrLit { value: library.to_string(), span: sp.word(library) };
    let library = LibraryClause {
        name,
        pkg_config: None,
        static_link: None,
        when_available: None,
        span: sp.word("library"),
    };
    let mut span = start.merge(library.span);
    for one in &items {
        span = span.merge(one.span);
    }
    item(
        ItemKind::Extern(ExternBlock {
            is_unsafe: true,
            abi,
            library: Some(library),
            items,
            span,
        }),
        span,
    )
}

fn extern_item_of(kind: ExternItemKind, span: Span) -> ExternItem {
    ExternItem { kind, span }
}

/// `def name(params) -> ret symbol "..."`.
pub fn extern_fn(
    sp: &Sp,
    name: &str,
    params: Vec<Param>,
    ret: Option<Type>,
    symbol: Option<&str>,
) -> ExternItem {
    let name = ident(sp, name);
    let mut span = name.span;
    for p in &params {
        span = span.merge(p.span);
    }
    if let Some(ret) = &ret {
        span = span.merge(ret.span);
    }
    let symbol = symbol.map(|text| StrLit { value: text.to_string(), span: sp.word(text) });
    if let Some(symbol) = &symbol {
        span = span.merge(symbol.span);
    }
    extern_item_of(
        ExternItemKind::Fn(ExternFn { name, params, ret, symbol, variadic: None, span }),
        span,
    )
}

/// `type BlasInt is I32`.
pub fn extern_alias(sp: &Sp, name: &str, ty: Type) -> ExternItem {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    extern_item_of(ExternItemKind::Alias(ExternAlias { name, ty, span }), span)
}

/// `const NAME be <value> as T`.
pub fn extern_const(sp: &Sp, name: &str, negative: bool, value: u128, ty: Type) -> ExternItem {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    extern_item_of(
        ExternItemKind::Const(ExternConst {
            name,
            negative,
            value: Literal::Int { value, base: IntBase::Dec, suffix: None },
            ty,
            span,
        }),
        span,
    )
}

/// `static NAME: T`.
pub fn extern_static(sp: &Sp, name: &str, ty: Type) -> ExternItem {
    let name = ident(sp, name);
    let span = name.span.merge(ty.span);
    extern_item_of(ExternItemKind::Static(ExternStatic { name, ty, span }), span)
}

/// `union Name: size N align M`.
pub fn extern_union(sp: &Sp, name: &str, size: u128, align: u128) -> ExternItem {
    let name = ident(sp, name);
    let span = name.span;
    extern_item_of(
        ExternItemKind::Union(ExternUnion { name, size: Some(size), align: Some(align), span }),
        span,
    )
}

/// `unsafe:` around a block, which §3 of the FFI note makes an expression.
pub fn unsafe_expr(sp: &Sp, body: Block) -> Expr {
    let span = sp.word("unsafe").merge(body.span);
    Expr { kind: ExprKind::Unsafe(body), span }
}

pub fn use_item(sp: &Sp, segments: &[&str], imports: Option<&[&str]>) -> Item {
    let start = sp.word("use");
    let path = path(sp, segments);
    let imports = imports.map(|names| names.iter().map(|n| ident(sp, n)).collect::<Vec<_>>());
    let mut span = start.merge(path.span);
    if let Some(names) = &imports {
        for n in names {
            span = span.merge(n.span);
        }
    }
    item(ItemKind::Use(UseDecl { path, imports, span }), span)
}

// --- blocks and statements -----------------------------------------------

pub fn block(sp: &Sp, stmts: Vec<Stmt>, tail: Option<Expr>) -> Block {
    let mut span = sp.take(1);
    for s in &stmts {
        span = span.merge(s.span);
    }
    if let Some(t) = &tail {
        span = span.merge(t.span);
    }
    Block { stmts, tail: tail.map(Box::new), span }
}

pub fn let_stmt(sp: &Sp, mutable: bool, name: &str, ty: Option<Type>, value: Expr) -> Stmt {
    let name = ident(sp, name);
    let span = name.span.merge(value.span);
    let binding = LetName { span: name.span, name, ty };
    Stmt { kind: StmtKind::Let(LetStmt { mutable, names: vec![binding], value, span }), span }
}

/// A `let` binding several names: `let value, err be f()` (revision 2 §3.1).
pub fn let_many(sp: &Sp, names: &[&str], value: Expr) -> Stmt {
    let names: Vec<_> = names
        .iter()
        .map(|n| {
            let name = ident(sp, n);
            LetName { span: name.span, name, ty: None }
        })
        .collect();
    let span = names[0].span.merge(value.span);
    Stmt { kind: StmtKind::Let(LetStmt { mutable: false, names, value, span }), span }
}

pub fn expr_stmt(value: Expr) -> Stmt {
    let span = value.span;
    Stmt { kind: StmtKind::Expr(value), span }
}

pub fn assign_stmt(target: Expr, value: Expr) -> Stmt {
    let span = target.span.merge(value.span);
    Stmt { kind: StmtKind::Assign { target, value }, span }
}

// --- expressions ---------------------------------------------------------

pub fn name(sp: &Sp, segments: &[&str]) -> Expr {
    let path = path(sp, segments);
    let span = path.span;
    Expr { kind: ExprKind::Path(path), span }
}

pub fn int(sp: &Sp, value: u128) -> Expr {
    let span = sp.take(value.to_string().len() as u32);
    Expr {
        kind: ExprKind::Literal(Literal::Int {
            value,
            base: science_lexer::IntBase::Dec,
            suffix: None,
        }),
        span,
    }
}

pub fn string(sp: &Sp, value: &str) -> Expr {
    let span = sp.take(value.len() as u32 + 2);
    Expr { kind: ExprKind::Literal(Literal::Str(value.to_string())), span }
}

/// An unnamed argument: the ordinary positional case.
pub fn arg(value: Expr) -> Arg {
    let span = value.span;
    Arg { name: None, value, span }
}

/// `by: f` — a *label*, not a name. Resolution must leave it alone.
pub fn named_arg(sp: &Sp, name: &str, value: Expr) -> Arg {
    let name = ident(sp, name);
    let span = name.span.merge(value.span);
    Arg { name: Some(name), value, span }
}

pub fn call(sp: &Sp, callee: Expr, args: Vec<Expr>) -> Expr {
    call_args(sp, callee, args.into_iter().map(arg).collect())
}

pub fn call_args(sp: &Sp, callee: Expr, args: Vec<Arg>) -> Expr {
    let mut span = callee.span.merge(sp.take(2));
    for a in &args {
        span = span.merge(a.span);
    }
    Expr { kind: ExprKind::Call { callee: Box::new(callee), args }, span }
}

pub fn struct_lit(sp: &Sp, segments: &[&str], fields: Vec<(&str, Expr)>) -> Expr {
    let path = path(sp, segments);
    let mut span = path.span;
    let fields: Vec<FieldInit> = fields
        .into_iter()
        .map(|(n, value)| {
            let name = ident(sp, n);
            let span = name.span.merge(value.span);
            FieldInit { name, value, span }
        })
        .collect();
    for f in &fields {
        span = span.merge(f.span);
    }
    Expr { kind: ExprKind::StructLit { path, fields }, span }
}

pub fn field_access(sp: &Sp, base: Expr, field: &str) -> Expr {
    let name = ident(sp, field);
    let span = base.span.merge(name.span);
    Expr { kind: ExprKind::Field { base: Box::new(base), name }, span }
}

pub fn method_call(sp: &Sp, receiver: Expr, method: &str, args: Vec<Expr>) -> Expr {
    method_call_args(sp, receiver, method, args.into_iter().map(arg).collect())
}

pub fn method_call_args(sp: &Sp, receiver: Expr, method: &str, args: Vec<Arg>) -> Expr {
    let method = ident(sp, method);
    let mut span = receiver.span.merge(method.span);
    for a in &args {
        span = span.merge(a.span);
    }
    Expr {
        kind: ExprKind::MethodCall {
            receiver: Box::new(receiver),
            method,
            generics: Vec::new(),
            args,
        },
        span,
    }
}

/// `doc giving doc.title` when `param` is `Some`, and the implicit
/// `each.title` when it is `None` (§4.6).
pub fn closure(sp: &Sp, param: Option<&str>, body: Expr) -> Expr {
    let param = param.map(|n| ident(sp, n));
    let span = param.as_ref().map_or(body.span, |p| p.span.merge(body.span));
    Expr { kind: ExprKind::Closure { param, body: Box::new(body) }, span }
}

/// `each` — the subject the implicit closure form leaves unwritten.
pub fn each(sp: &Sp) -> Expr {
    Expr { kind: ExprKind::Each, span: sp.word("each") }
}

pub fn range(sp: &Sp, start: Expr, end: Expr, inclusive: bool) -> Expr {
    let span = start.span.merge(sp.take(2)).merge(end.span);
    Expr {
        kind: ExprKind::Range { start: Box::new(start), end: Box::new(end), inclusive },
        span,
    }
}

pub fn borrowed(sp: &Sp, mutable: bool, expr: Expr) -> Expr {
    let span = sp.word("borrowed").merge(expr.span);
    Expr { kind: ExprKind::Borrowed { mutable, expr: Box::new(expr) }, span }
}

pub fn loop_expr(sp: &Sp, body: Block) -> Expr {
    let span = sp.word("loop").merge(body.span);
    Expr { kind: ExprKind::Loop { body }, span }
}

pub fn break_stmt(sp: &Sp, value: Option<Expr>) -> Stmt {
    let mut span = sp.word("break");
    if let Some(v) = &value {
        span = span.merge(v.span);
    }
    Stmt { kind: StmtKind::Break(value), span }
}

pub fn self_value(sp: &Sp) -> Expr {
    Expr { kind: ExprKind::SelfValue, span: sp.word("self") }
}

pub fn match_expr(sp: &Sp, scrutinee: Expr, arms: Vec<(Pattern, Expr)>) -> Expr {
    let mut span = sp.word("match").merge(scrutinee.span);
    let arms: Vec<MatchArm> = arms
        .into_iter()
        .map(|(pattern, body)| {
            let span = pattern.span.merge(body.span);
            MatchArm { pattern, body, span }
        })
        .collect();
    for a in &arms {
        span = span.merge(a.span);
    }
    Expr { kind: ExprKind::Match(MatchExpr { scrutinee: Box::new(scrutinee), arms, span }), span }
}

pub fn block_expr(block: Block) -> Expr {
    let span = block.span;
    Expr { kind: ExprKind::Block(block), span }
}

pub fn for_expr(sp: &Sp, pattern: Pattern, iter: Expr, body: Block) -> Expr {
    let span = sp.word("for").merge(pattern.span).merge(iter.span).merge(body.span);
    Expr { kind: ExprKind::For { pattern, iter: Box::new(iter), body }, span }
}

// --- patterns ------------------------------------------------------------

pub fn pat_binding(sp: &Sp, mutable: bool, n: &str) -> Pattern {
    let name = ident(sp, n);
    let span = name.span;
    Pattern { kind: PatternKind::Binding { mutable, name }, span }
}

pub fn pat_wildcard(sp: &Sp) -> Pattern {
    Pattern { kind: PatternKind::Wildcard, span: sp.take(1) }
}

pub fn pat_variant(sp: &Sp, segments: &[&str], elems: Vec<Pattern>) -> Pattern {
    let path = path(sp, segments);
    let mut span = path.span;
    for e in &elems {
        span = span.merge(e.span);
    }
    Pattern { kind: PatternKind::Variant { path, elems }, span }
}

pub fn pat_struct(sp: &Sp, segments: &[&str], fields: Vec<(&str, Pattern)>) -> Pattern {
    let path = path(sp, segments);
    let mut span = path.span;
    let fields: Vec<FieldPattern> = fields
        .into_iter()
        .map(|(n, pattern)| {
            let name = ident(sp, n);
            let span = name.span.merge(pattern.span);
            FieldPattern { name, pattern, span }
        })
        .collect();
    for f in &fields {
        span = span.merge(f.span);
    }
    Pattern { kind: PatternKind::Struct { path, fields }, span }
}

// --- running the resolver ------------------------------------------------

/// Resolves a single-file module and renders the HIR dump plus diagnostics.
///
/// Both halves always appear, so a snapshot that loses its diagnostics is as
/// visible as one that gains them — the same rule the parser's snapshots use.
pub fn report(module: &Module) -> String {
    let (krate, diagnostics) = science_resolve::resolve_module(FILE, "main.science", module);
    render(&krate, &diagnostics)
}

/// The same for a multi-file crate, given `(relative path, module)` pairs.
pub fn report_crate(files: Vec<(&str, Module)>) -> String {
    let sources: Vec<science_resolve::SourceModule> = files
        .into_iter()
        .enumerate()
        .map(|(index, (path, ast))| science_resolve::SourceModule {
            file: FileId(index as u32),
            path: path.to_string(),
            ast,
            // The first file is the entry, which is what `sciencec` does with
            // the file named on the command line (`script-mode.md` §4.2).
            entry: index == 0,
        })
        .collect();
    let (krate, diagnostics) = science_resolve::resolve_crate(&sources);
    render(&krate, &diagnostics)
}

pub fn render(krate: &science_resolve::hir::Crate, diagnostics: &Diagnostics) -> String {
    let mut out = science_resolve::dump::dump_crate(krate);
    out.push_str("--- diagnostics ---\n");
    if diagnostics.is_empty() {
        out.push_str("(none)\n");
    }
    for diagnostic in diagnostics.iter() {
        let span = diagnostic
            .primary_span()
            .map(|s| format!("@{}..{}", s.start, s.end))
            .unwrap_or_else(|| "@?".to_string());
        out.push_str(&format!("{} {} {}\n", diagnostic.code, span, diagnostic.message));
        for label in diagnostic.labels.iter().filter(|l| !l.primary) {
            out.push_str(&format!(
                "  note @{}..{} {}\n",
                label.span.start, label.span.end, label.message
            ));
        }
        for note in &diagnostic.notes {
            out.push_str(&format!("  note {note}\n"));
        }
    }
    out
}

/// The diagnostics of a single-file resolution, for tests that assert on codes
/// rather than on a whole dump.
pub fn diagnose(module: &Module) -> Diagnostics {
    science_resolve::resolve_module(FILE, "main.science", module).1
}

/// The codes a resolution produced, in order.
pub fn codes(diagnostics: &Diagnostics) -> Vec<String> {
    diagnostics.iter().map(|d| d.code.to_string()).collect()
}
