//! A readable tree dump of the AST.
//!
//! Snapshot tests (§10 of the design spec) compare dumps, and the other
//! compiler phases use them to see what they were handed, so the format is
//! tuned for a human reading a diff: one node per line, two spaces of indent
//! per level, and every line ending in the node's span.
//!
//! ```text
//! Fn `longest` @0..48
//!   params
//!     Param `a` @11..21
//!       type: Ref @14..21
//!         Path `String` @15..21
//!   ret: Ref @37..44
//!     Path `String` @38..44
//!   body: Block @46..47
//! ```
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
        w.node(&named("Bound", &self.path.dotted()), self.span, |w| {
            dump_path_generics(&self.path, w)
        });
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
            ItemKind::Struct(decl) => decl.dump_node(w),
            ItemKind::Enum(decl) => decl.dump_node(w),
            ItemKind::Trait(decl) => decl.dump_node(w),
            ItemKind::Impl(block) => block.dump_node(w),
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
        w.node(&named("TypeParam", &self.name.name), self.span, |w| {
            w.list("bounds", &self.bounds)
        });
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
        let mut header = named("Fn", &self.name.name);
        flag(&mut header, self.is_pub, "pub");
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
            SelfKind::Value => "self",
            SelfKind::Ref => "&self",
            SelfKind::RefMut => "&mut self",
        };
        w.leaf(header, self.span);
    }
}

impl Dump for Param {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node(&named("Param", &self.name.name), self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for StructDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Struct", &self.name.name);
        flag(&mut header, self.is_pub, "pub");
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
        flag(&mut header, self.is_pub, "pub");
        w.node(&header, self.span, |w| w.child("type", &self.ty));
    }
}

impl Dump for EnumDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Enum", &self.name.name);
        flag(&mut header, self.is_pub, "pub");
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

impl Dump for TraitDecl {
    fn dump_node(&self, w: &mut DumpWriter) {
        let mut header = named("Trait", &self.name.name);
        flag(&mut header, self.is_pub, "pub");
        w.node(&header, self.span, |w| {
            w.list("generics", &self.generics);
            w.list("supertraits", &self.supertraits);
            w.list("where", &self.where_clause);
            w.list("methods", &self.methods);
        });
    }
}

impl Dump for ImplBlock {
    fn dump_node(&self, w: &mut DumpWriter) {
        w.node("Impl", self.span, |w| {
            w.list("generics", &self.generics);
            w.child_opt("trait", self.trait_.as_ref());
            w.child("type", &self.self_ty);
            w.list("where", &self.where_clause);
            w.list("methods", &self.methods);
        });
    }
}

// --- types ---------------------------------------------------------------

impl Dump for Type {
    fn dump_node(&self, w: &mut DumpWriter) {
        match &self.kind {
            TypeKind::Path(path) => {
                w.node(&named("Path", &path.dotted()), self.span, |w| dump_path_generics(path, w))
            }
            TypeKind::Ref { mutable, inner } => {
                let header = if *mutable { "Ref mut" } else { "Ref" };
                w.node(header, self.span, |w| inner.dump_node(w));
            }
            TypeKind::Dyn(bound) => w.node("Dyn", self.span, |w| bound.dump_node(w)),
            TypeKind::Tuple(elems) => w.node("Tuple", self.span, |w| w.items(elems)),
            TypeKind::Unit => w.leaf("Unit", self.span),
            TypeKind::SelfType => w.leaf("SelfType", self.span),
            TypeKind::Error => w.leaf("Error", self.span),
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
        let mut header = named("Let", &self.name.name);
        flag(&mut header, self.mutable, "mut");
        w.node(&header, self.span, |w| {
            w.child_opt("type", self.ty.as_ref());
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
            ExprKind::Try(inner) => w.node("Try", self.span, |w| inner.dump_node(w)),
            ExprKind::Ref { mutable, expr } => {
                let header = if *mutable { "Ref mut" } else { "Ref" };
                w.node(header, self.span, |w| expr.dump_node(w));
            }
            ExprKind::If(if_expr) => if_expr.dump_node(w),
            ExprKind::Match(match_expr) => match_expr.dump_node(w),
            ExprKind::While { cond, body } => w.node("While", self.span, |w| {
                w.child("cond", &**cond);
                w.child("body", body);
            }),
            ExprKind::Loop { body } => w.node("Loop", self.span, |w| w.child("body", body)),
            ExprKind::For { pattern, iter, body } => w.node("For", self.span, |w| {
                w.child("pattern", pattern);
                w.child("iter", &**iter);
                w.child("body", body);
            }),
            ExprKind::Block(block) => block.dump_node(w),
            ExprKind::Error => w.leaf("Error", self.span),
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
                flag(&mut header, *mutable, "mut");
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
