//! A readable tree dump of the AST.
//!
//! Snapshot tests (§10 of the design spec) compare dumps, and the other
//! compiler phases use them to see what they were handed, so the format is
//! tuned for a human reading a diff: one node per line, two spaces of indent
//! per level, and every line ending in the node's span.
//!
//! ```text
//! Fn `longest` @0..62
//!   params
//!     Param `a` @14..32
//!       type: Borrowed @17..32
//!         Path `String` @26..32
//!   ret: Borrowed @46..61
//!     Path `String` @55..61
//!   body: Block @63..64
//! ```
//!
//! The headers name the source construct, not an internal one: a type written
//! `borrowed T` dumps as `Borrowed`, and an implementation as `Implements` or
//! `HasMethods`. A dump that still spoke the old syntax would be a second
//! place the language is described, and it would be the one that rots.
//!
//! Wrapper nodes whose span merely repeats their child's are not printed: an
//! `Item` shows as the declaration it holds, and an expression statement shows
//! as the expression. Everything that carries information of its own gets a
//! line.

use std::fmt::Write as _;

use science_diagnostics::Span;
use science_lexer::{IntBase, NumSuffix};

use crate::ast::*;

/// Anything in the tree can be dumped.
pub trait Dump {
    /// Writes this node, and its children one level deeper.
    fn dump_node(&self, w: &mut DumpWriter);

    /// The dump of this node as a standalone string.
    fn dump(&self) -> String
    where
        Self: Sized,
    {
        let mut w = DumpWriter::new();
        self.dump_node(&mut w);
        w.finish()
    }
}

/// Accumulates the dump, tracking depth and the label of the node being written.
pub struct DumpWriter {
    out: String,
    depth: usize,
    /// Set by `child`, consumed by the next `node`, so a node can be prefixed
    /// with the role it plays in its parent (`ret:`, `cond:`, `body:`).
    pending_label: Option<String>,
}

impl Default for DumpWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl DumpWriter {
    pub fn new() -> Self {
        DumpWriter { out: String::new(), depth: 0, pending_label: None }
    }

    pub fn finish(self) -> String {
        self.out
    }

    fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }

    /// Writes a node's header line, then its children one level deeper.
    pub fn node(&mut self, header: &str, span: Span, children: impl FnOnce(&mut DumpWriter)) {
        let label = self.pending_label.take();
        self.indent();
        if let Some(label) = label {
            self.out.push_str(&label);
            self.out.push_str(": ");
        }
        self.out.push_str(header);
        let _ = write!(self.out, " {}", DisplaySpan(span));
        self.out.push('\n');

        self.depth += 1;
        children(self);
        self.depth -= 1;
    }

    /// A node with no children.
    pub fn leaf(&mut self, header: &str, span: Span) {
        self.node(header, span, |_| {});
    }

    /// A line without a span, used for group headers and for saying that
    /// something is present but empty.
    pub fn note(&mut self, text: &str) {
        self.indent();
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// One child node, prefixed with the role it plays in its parent.
    pub fn child(&mut self, label: &str, node: &dyn Dump) {
        self.pending_label = Some(label.to_string());
        node.dump_node(self);
        // A node always consumes the label, but clearing it keeps a buggy
        // impl from leaking its parent's label onto a later sibling.
        self.pending_label = None;
    }

    /// The same, when the child is optional.
    pub fn child_opt<T: Dump>(&mut self, label: &str, node: Option<&T>) {
        if let Some(node) = node {
            self.child(label, node);
        }
    }

    /// A group of children under a heading. Nothing is written when empty.
    pub fn list<T: Dump>(&mut self, label: &str, nodes: &[T]) {
        if nodes.is_empty() {
            return;
        }
        self.note(label);
        self.depth += 1;
        for node in nodes {
            node.dump_node(self);
        }
        self.depth -= 1;
    }

    /// Children with no heading of their own, for nodes with a single obvious
    /// kind of child.
    pub fn items<T: Dump>(&mut self, nodes: &[T]) {
        for node in nodes {
            node.dump_node(self);
        }
    }
}

/// `@12..20`, or `@f3:12..20` when the span is not in the first file.
struct DisplaySpan(Span);

impl std::fmt::Display for DisplaySpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.file.0 == 0 {
            write!(f, "@{}..{}", self.0.start, self.0.end)
        } else {
            write!(f, "@f{}:{}..{}", self.0.file.0, self.0.start, self.0.end)
        }
    }
}

// --- shared bits ---------------------------------------------------------

fn flag(header: &mut String, on: bool, word: &str) {
    if on {
        header.push(' ');
        header.push_str(word);
    }
}

fn named(kind: &str, name: &str) -> String {
    format!("{kind} `{name}`")
}

/// Generic arguments hang off path segments; only segments that have any are
/// printed, which in practice is the last one.
fn dump_path_generics(path: &Path, w: &mut DumpWriter) {
    for segment in &path.segments {
        if segment.generics.is_empty() {
            continue;
        }
        w.list(&format!("generics of `{}`", segment.name.name), &segment.generics);
    }
}

fn base_name(base: IntBase) -> Option<&'static str> {
    match base {
        IntBase::Dec => None,
        IntBase::Hex => Some("hex"),
        IntBase::Oct => Some("oct"),
        IntBase::Bin => Some("bin"),
    }
}

fn suffix_name(suffix: NumSuffix) -> &'static str {
    use NumSuffix::*;
    match suffix {
        I8 => "i8",
        I16 => "i16",
        I32 => "i32",
        I64 => "i64",
        U8 => "u8",
        U16 => "u16",
        U32 => "u32",
        U64 => "u64",
        F32 => "f32",
        F64 => "f64",
    }
}

fn literal_header(literal: &Literal) -> String {
    match literal {
        Literal::Int { value, base, suffix } => {
            let mut header = format!("Int {value}");
            if let Some(base) = base_name(*base) {
                header.push(' ');
                header.push_str(base);
            }
            if let Some(suffix) = suffix {
                header.push(' ');
                header.push_str(suffix_name(*suffix));
            }
            header
        }
        Literal::Float { value, suffix } => {
            let mut header = format!("Float {value}");
            if let Some(suffix) = suffix {
                header.push(' ');
                header.push_str(suffix_name(*suffix));
            }
            header
        }
        Literal::Str(value) => format!("Str {value:?}"),
        Literal::Char(value) => format!("Char {value:?}"),
        Literal::Bool(value) => format!("Bool {value}"),
        Literal::Null => "Null".to_string(),
    }
}

// --- names ---------------------------------------------------------------

impl Dump for Ident {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.leaf(&named("Ident", &self.name), self.span);
    }
}

impl Dump for Path {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("Path", &self.dotted()), self.span, |w| dump_path_generics(self, w));
    }
}

impl Dump for TypeBound {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            TypeBoundKind::Interface(path) => {
                w.node(&named("Bound", &path.dotted()), self.span, |w| {
                    dump_path_generics(path, w)
                });
            }
            TypeBoundKind::Closure { params, ret } => {
                w.node("ClosureBound", self.span, |w| {
                    w.list("params", params);
                    w.child("ret", ret.as_ref());
                });
            }
        }
    }
}

// --- module and items ----------------------------------------------------

impl Dump for Module {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Module", self.span, |w| w.items(&self.items));
    }
}

impl Dump for Item {
    fn dump_node(&self, w: &mut DumpWriter) {
        // The item wrapper's span always matches the declaration's, so it
        // would only add a line that says nothing.
        match &self.kind {
            ItemKind::Use(decl) => decl.dump_node(w),
            ItemKind::Fn(decl) => decl.dump_node(w),
            ItemKind::Record(decl) => decl.dump_node(w),
            ItemKind::Choice(decl) => decl.dump_node(w),
            ItemKind::Alias(decl) => decl.dump_node(w),
            ItemKind::Const(decl) => decl.dump_node(w),
            ItemKind::Interface(decl) => decl.dump_node(w),
            ItemKind::Impl(block) => block.dump_node(w),
            ItemKind::Extern(block) => block.dump_node(w),
        }
    }
}

impl Dump for UseDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("Use", &self.path.dotted()), self.span, |w| match &self.imports {
            None => {}
            Some(imports) if imports.is_empty() => w.note("imports (empty)"),
            Some(imports) => w.list("imports", imports),
        });
    }
}

impl Dump for GenericParam {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            GenericParamKind::Type { bounds } => {
                w.node(&named("TypeParam", &self.name.name), self.span, |w| {
                    w.list("bounds", bounds)
                })
            }
            GenericParamKind::Const { ty } => {
                w.node(&named("ConstParam", &self.name.name), self.span, |w| w.child("type", ty))
            }
        }
    }
}

impl Dump for WherePredicate {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Predicate", self.span, |w| {
            w.child("type", &self.ty);
            w.list("bounds", &self.bounds);
        });
    }
}

impl Dump for FnDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        // The word the declaration was written with is part of what the tree
        // says, so a `tool` and a `def` never dump the same.
        let word = match self.form {
            FnForm::Def => "Fn",
            FnForm::Tool => "Tool",
        };
        let mut header = named(word, &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.child_opt("receiver", self.self_param.as_ref());
            w.list("params", &self.params);
            w.child_opt("ret", self.ret.as_ref());
            w.list("where", &self.where_clause);
            w.child_opt("body", self.body.as_ref());
        });
    }
}

impl Dump for SelfParam {
    fn dump_node(&self, w: &mut DumpWriter) {
        let header = match self.kind {
            SelfKind::Shared => "self",
            SelfKind::Mutable => "mutable self",
            SelfKind::Value => "self: Self",
        };
        w.leaf(header, self.span);
    }
}

impl Dump for Param {
    fn dump_node(&self, w: &mut DumpWriter) {
        // A documented parameter is a `tool`'s (Decision 7), and the run is
        // flagged rather than printed: what a description *says* is the
        // emitter's business, and a multi-line string in a one-line node is
        // not a tree dump any more.
        let mut header = named("Param", &self.name.name);
        flag(&mut header, self.doc.is_some(), "documented");
        w.node(&header, self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for RecordDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Record", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.list("where", &self.where_clause);
            w.list("fields", &self.fields);
        });
    }
}

impl Dump for FieldDef {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Field", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for ChoiceDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Choice", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.list("where", &self.where_clause);
            w.list("variants", &self.variants);
        });
    }
}

impl Dump for VariantDef {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("Variant", &self.name.name), self.span, |w| {
            w.list("payload", &self.payload)
        });
    }
}

impl Dump for AliasDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Alias", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.child("type", &self.ty);
        });
    }
}

impl Dump for ConstDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Const", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.child_opt("type", self.ty.as_ref());
            w.child("value", &self.value);
        });
    }
}

impl Dump for InterfaceDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Interface", &self.name.name);
        flag(&mut header, self.is_pub, "public");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.list("supers", &self.supers);
            w.list("where", &self.where_clause);
            w.list("assoc types", &self.assoc_types);
            w.list("methods", &self.methods);
        });
    }
}

impl Dump for AssocTypeDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.leaf(&named("AssocType", &self.name.name), self.span);
    }
}

impl Dump for ImplBlock {
    fn dump_node(&self, w: &mut DumpWriter) {
        // The two heads of §4.4 are different declarations, so they get
        // different headers: a reader should not have to look for an
        // `interface:` line to tell `Doc implements Summarize` from `Doc has`.
        let header = if self.interface.is_some() { "Implements" } else { "Has" };
        w.node(header, self.span, |w| {
            w.list("generics", &self.generics);
            w.child_opt("interface", self.interface.as_ref());
            w.child("type", &self.self_ty);
            w.list("where", &self.where_clause);
            w.list("assoc types", &self.assoc_types);
            w.list("methods", &self.methods);
        });
    }
}

impl Dump for AssocTypeBinding {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("AssocType", &self.name.name), self.span, |w| w.child("type", &self.ty));
    }
}

// --- extern blocks -------------------------------------------------------

impl Dump for StrLit {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.leaf(&format!("Str {:?}", self.value), self.span);
    }
}

impl Dump for ExternBlock {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = format!("Extern {:?}", self.abi.value);
        flag(&mut header, self.is_unsafe, "unsafe");
        w.node(&header, self.span, |w| {
            match &self.library {
                Some(library) => library.dump_node(w),
                None => w.note("library (none)"),
            }
            w.list("items", &self.items);
        });
    }
}

impl Dump for LibraryClause {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = format!("Library {:?}", self.name.value);
        flag(&mut header, self.static_link.is_some(), "static");
        flag(&mut header, self.when_available.is_some(), "when-available");
        w.node(&header, self.span, |w| {
            if let Some(module) = &self.pkg_config {
                w.child("pkg-config", module);
            }
        });
    }
}

impl Dump for ExternItem {
    fn dump_node(&self, w: &mut DumpWriter) {
        // As with `Item`, the wrapper's span only repeats its child's.
        match &self.kind {
            ExternItemKind::Fn(node) => node.dump_node(w),
            ExternItemKind::Alias(node) => node.dump_node(w),
            ExternItemKind::Const(node) => node.dump_node(w),
            ExternItemKind::Static(node) => node.dump_node(w),
            ExternItemKind::Union(node) => node.dump_node(w),
            ExternItemKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl Dump for ExternFn {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("ExternFn", &self.name.name);
        flag(&mut header, self.variadic.is_some(), "variadic");
        w.node(&header, self.span, |w| {
            w.list("params", &self.params);
            w.child_opt("ret", self.ret.as_ref());
            w.child_opt("symbol", self.symbol.as_ref());
        });
    }
}

impl Dump for ExternAlias {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("ExternAlias", &self.name.name), self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for ExternConst {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("ExternConst", &self.name.name);
        flag(&mut header, self.negative, "negative");
        w.node(&header, self.span, |w| {
            w.note(&format!("value: {}", literal_header(&self.value)));
            w.child("type", &self.ty);
        });
    }
}

impl Dump for ExternStatic {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("ExternStatic", &self.name.name), self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for ExternUnion {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("ExternUnion", &self.name.name), self.span, |w| {
            w.note(&format!("size: {}", layout_number(self.size)));
            w.note(&format!("align: {}", layout_number(self.align)));
        });
    }
}

/// A union's size or alignment, or that it was missing and reported.
fn layout_number(value: Option<u128>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "(none)".to_string(),
    }
}

// --- types ---------------------------------------------------------------

impl Dump for Type {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            TypeKind::Path(path) => {
                w.node(&named("Path", &path.dotted()), self.span, |w| dump_path_generics(path, w))
            }
            TypeKind::Borrowed { mutable, inner } => {
                let header = if *mutable { "Borrowed mutable" } else { "Borrowed" };
                w.node(header, self.span, |w| inner.dump_node(w));
            }
            TypeKind::Any(bound) => w.node("Any", self.span, |w| bound.dump_node(w)),
            TypeKind::Nullable(inner) => w.node("Nullable", self.span, |w| inner.dump_node(w)),
            TypeKind::Tuple(elems) => w.node("Tuple", self.span, |w| w.items(elems)),
            TypeKind::Closure { params, ret } => w.node("Closure", self.span, |w| {
                w.list("params", params);
                w.child("ret", ret.as_ref());
            }),
            TypeKind::Unit => w.leaf("Unit", self.span),
            TypeKind::SelfType => w.leaf("SelfType", self.span),
            TypeKind::SelfAssoc(name) => w.leaf(&named("SelfAssoc", &name.name), self.span),
            // `ConstArg` and the const expression's root always cover the same
            // span, so they share a line rather than one wrapping the other:
            // a wrapper whose span merely repeats its child's is not printed.
            // A literal stays the single leaf it has always been; a negation
            // names its operator and hangs its operand below, which is the
            // shape `+`, `*` and `/` will arrive in.
            TypeKind::Const(value) => match &value.kind {
                ConstExprKind::Lit(literal) => {
                    w.leaf(&format!("ConstArg {}", literal_header(literal)), self.span)
                }
                ConstExprKind::Neg(operand) => {
                    w.node("ConstArg Neg", self.span, |w| operand.dump_node(w))
                }
                // Everything else hangs its own tree below one header, so the
                // dump of a const argument reads the same whatever is in it.
                _ => w.node("ConstArg", self.span, |w| value.dump_node(w)),
            },
            TypeKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl Dump for ConstExpr {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            ConstExprKind::Lit(literal) => w.leaf(&literal_header(literal), self.span),
            ConstExprKind::Neg(operand) => w.node("Neg", self.span, |w| operand.dump_node(w)),
            ConstExprKind::Param(name) => {
                w.leaf(&named("ConstParam", &name.name), self.span)
            }
            ConstExprKind::Add(lhs, rhs) => w.node("Add", self.span, |w| {
                lhs.dump_node(w);
                rhs.dump_node(w);
            }),
            ConstExprKind::Sub(lhs, rhs) => w.node("Sub", self.span, |w| {
                lhs.dump_node(w);
                rhs.dump_node(w);
            }),
            // The factor is a literal by construction, so it is printed in
            // the header rather than as a child: a child would suggest the
            // grammar admits an expression there, which is the one thing
            // §2.1 exists to forbid.
            ConstExprKind::Mul { operand, factor, .. } => {
                w.node(&format!("Mul by {}", literal_header(factor)), self.span, |w| {
                    operand.dump_node(w)
                })
            }
            ConstExprKind::Div { operand, divisor, .. } => {
                w.node(&format!("Div by {}", literal_header(divisor)), self.span, |w| {
                    operand.dump_node(w)
                })
            }
        }
    }
}

// --- blocks and statements -----------------------------------------------

impl Dump for Block {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Block", self.span, |w| {
            w.items(&self.stmts);
            w.child_opt("tail", self.tail.as_deref());
        });
    }
}

impl Dump for Stmt {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            StmtKind::Let(decl) => decl.dump_node(w),
            // An expression statement's span matches the expression's, so the
            // wrapper is not worth a line. A block's tail is labelled, which
            // is what tells the two apart in a dump.
            StmtKind::Expr(expr) => expr.dump_node(w),
            StmtKind::Assign { target, value } => w.node("Assign", self.span, |w| {
                w.child("target", target);
                w.child("value", value);
            }),
            StmtKind::Return(value) => {
                w.node("Return", self.span, |w| w.child_opt("value", value.as_ref()))
            }
            StmtKind::Break(value) => {
                w.node("Break", self.span, |w| w.child_opt("value", value.as_ref()))
            }
            StmtKind::Continue => w.leaf("Continue", self.span),
            StmtKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl Dump for LetStmt {
    fn dump_node(&self, w: &mut DumpWriter) {
        // The names are joined with the comma that separated them, so a
        // single binding dumps exactly the header it always did and a
        // destructure reads back as what was written.
        let joined =
            self.names.iter().map(|n| n.name.name.as_str()).collect::<Vec<_>>().join(", ");
        let mut header = named("Let", &joined);
        flag(&mut header, self.mutable, "mutable");
        w.node(&header, self.span, |w| {
            for binding in &self.names {
                w.child_opt("type", binding.ty.as_ref());
            }
            w.child("value", &self.value);
        });
    }
}

// --- expressions ---------------------------------------------------------

impl Dump for Expr {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            ExprKind::Literal(literal) => w.leaf(&literal_header(literal), self.span),
            ExprKind::Path(path) => {
                w.node(&named("Path", &path.dotted()), self.span, |w| dump_path_generics(path, w))
            }
            ExprKind::SelfValue => w.leaf("Self", self.span),
            ExprKind::Call { callee, args } => w.node("Call", self.span, |w| {
                w.child("callee", &**callee);
                w.list("args", args);
            }),
            ExprKind::MethodCall { receiver, method, generics, args } => {
                w.node(&named("Method", &method.name), self.span, |w| {
                    w.child("receiver", &**receiver);
                    w.list("generics", generics);
                    w.list("args", args);
                })
            }
            ExprKind::Field { base, name } => {
                w.node(&named("Field", &name.name), self.span, |w| w.child("base", &**base))
            }
            ExprKind::Index { base, index } => w.node("Index", self.span, |w| {
                w.child("base", &**base);
                w.child("index", &**index);
            }),
            // Its elements are dumped as items rather than as named children,
            // the way `Tuple`'s are: they are a list and nothing but a list,
            // and a name per element would be an index printed twice.
            ExprKind::ArrayLit(elements) => {
                w.node("ArrayLit", self.span, |w| w.items(elements))
            }
            ExprKind::StructLit { path, fields } => {
                w.node(&named("StructLit", &path.dotted()), self.span, |w| {
                    dump_path_generics(path, w);
                    w.list("fields", fields);
                })
            }
            ExprKind::Tuple(elems) => w.node("Tuple", self.span, |w| w.items(elems)),
            ExprKind::Unit => w.leaf("Unit", self.span),
            ExprKind::Unary { op, operand } => {
                w.node(&named("Unary", op.as_str()), self.span, |w| operand.dump_node(w))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                w.node(&named("Binary", op.as_str()), self.span, |w| {
                    w.child("lhs", &**lhs);
                    w.child("rhs", &**rhs);
                })
            }
            ExprKind::Cast { expr, ty } => w.node("Cast", self.span, |w| {
                w.child("expr", &**expr);
                w.child("type", ty);
            }),
            ExprKind::Present(inner) => w.node("Present", self.span, |w| inner.dump_node(w)),
            ExprKind::Borrowed { mutable, expr } => {
                let header = if *mutable { "Borrowed mutable" } else { "Borrowed" };
                w.node(header, self.span, |w| expr.dump_node(w));
            }
            ExprKind::Range { start, end, inclusive } => {
                let header = if *inclusive { "Range inclusive" } else { "Range" };
                w.node(header, self.span, |w| {
                    w.child("start", &**start);
                    w.child("end", &**end);
                })
            }
            ExprKind::Closure { param, body } => {
                // The implicit form has no parameter to print, and the
                // `Each` in its body is what says so.
                w.node("Closure", self.span, |w| {
                    w.child_opt("param", param.as_ref());
                    w.child("body", &**body);
                })
            }
            ExprKind::Each => w.leaf("Each", self.span),
            ExprKind::If(if_expr) => if_expr.dump_node(w),
            ExprKind::Match(match_expr) => match_expr.dump_node(w),
            ExprKind::Loop { body } => w.node("Loop", self.span, |w| w.child("body", body)),
            ExprKind::For { pattern, iter, body } => w.node("For", self.span, |w| {
                w.child("pattern", pattern);
                w.child("iter", &**iter);
                w.child("body", body);
            }),
            ExprKind::Unsafe(body) => {
                w.node("Unsafe", self.span, |w| w.child("body", body))
            }
            ExprKind::Block(block) => block.dump_node(w),
            ExprKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl Dump for Arg {
    fn dump_node(&self, w: &mut DumpWriter) {
        // A positional argument is its expression: the wrapper would repeat
        // the span and say nothing. A named one has a name worth a line.
        match &self.name {
            None => self.value.dump_node(w),
            Some(name) => w.node(&named("Arg", &name.name), self.span, |w| {
                w.child("value", &self.value)
            }),
        }
    }
}

impl Dump for FieldInit {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("FieldInit", &self.name.name), self.span, |w| {
            w.child("value", &self.value)
        });
    }
}

impl Dump for IfExpr {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("If", self.span, |w| {
            w.child("cond", &*self.cond);
            w.child("then", &self.then_branch);
            w.child_opt("else", self.else_branch.as_deref());
        });
    }
}

impl Dump for MatchExpr {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Match", self.span, |w| {
            w.child("scrutinee", &*self.scrutinee);
            w.list("arms", &self.arms);
        });
    }
}

impl Dump for MatchArm {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Arm", self.span, |w| {
            w.child("pattern", &self.pattern);
            w.child("body", &self.body);
        });
    }
}

// --- patterns ------------------------------------------------------------

impl Dump for Pattern {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            PatternKind::Wildcard => w.leaf("Wildcard", self.span),
            PatternKind::Literal(literal) => w.leaf(&literal_header(literal), self.span),
            PatternKind::Binding { mutable, name } => {
                let mut header = named("Binding", &name.name);
                flag(&mut header, *mutable, "mutable");
                w.leaf(&header, self.span);
            }
            PatternKind::Variant { path, elems } => {
                w.node(&named("Variant", &path.dotted()), self.span, |w| w.items(elems))
            }
            PatternKind::Struct { path, fields } => {
                w.node(&named("StructPat", &path.dotted()), self.span, |w| w.list("fields", fields))
            }
            PatternKind::Tuple(elems) => w.node("Tuple", self.span, |w| w.items(elems)),
            PatternKind::Unit => w.leaf("Unit", self.span),
            PatternKind::Or(alts) => w.node("Or", self.span, |w| w.items(alts)),
            PatternKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl Dump for FieldPattern {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("FieldPat", &self.name.name), self.span, |w| {
            w.child("pattern", &self.pattern)
        });
    }
}
