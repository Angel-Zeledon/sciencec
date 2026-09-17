//! A readable tree dump of the HIR.
//!
//! The same format the parser's `Dump` produces, and literally the same
//! writer: one node per line, two spaces of indent per level, every line
//! ending in the node's span. Reading an AST dump and an HIR dump side by side
//! is how the difference between them is checked, so they had better look
//! alike.
//!
//! What is added is the resolution itself. Every definition prints its name
//! and its id — ``Fn `main` #47`` — and every reference prints what it
//! resolved to — ``Path -> `main` #47``. A reference that failed prints
//! `-> <error>`, which makes an unresolved name visible in a snapshot instead
//! of being a silently missing line.
//!
//! The prelude is not dumped. Its definitions have no source and appear in
//! every file; printing them would bury the ten lines a test is about under
//! forty that never change.
//!
//! One wrinkle: rendering a node needs the [`DefTable`] to turn a `DefId` into
//! a name, and `Dump` takes no context. [`Node`] carries the table alongside
//! the node so the parser's `DumpWriter` can be reused unchanged.

use science_parser::{Dump, DumpWriter};

use crate::hir::*;

/// Renders a resolved crate.
pub fn dump_crate(krate: &Crate) -> String {
    let mut w = DumpWriter::new();
    for module in &krate.modules {
        Node(&krate.defs, module).dump_node(&mut w);
    }
    w.finish()
}

/// Renders one node, for a test that is about a single item.
pub fn dump<T: DumpIn>(defs: &DefTable, node: &T) -> String {
    let mut w = DumpWriter::new();
    Node(defs, node).dump_node(&mut w);
    w.finish()
}

/// A node paired with the table that gives its ids their names.
pub struct Node<'a, T: ?Sized>(pub &'a DefTable, pub &'a T);

impl<T: DumpIn + ?Sized> Dump for Node<'_, T> {
    fn dump_node(&self, w: &mut DumpWriter) {
        self.1.dump_in(self.0, w);
    }
}

/// What every HIR node implements, since none of them can be rendered without
/// the def table.
pub trait DumpIn {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter);
}

fn nodes<'a, T: DumpIn>(defs: &'a DefTable, items: &'a [T]) -> Vec<Node<'a, T>> {
    items.iter().map(|item| Node(defs, item)).collect()
}

// --- naming --------------------------------------------------------------

/// How a definition is introduced: ``Fn `main` #47``.
fn defined(defs: &DefTable, kind: &str, id: DefId) -> String {
    format!("{kind} {}", def_label(defs, id))
}

/// A definition as it appears in a header: `` `name` #12 ``, or `#12` alone
/// when the name was lost to an error.
///
/// Split out of [`defined`] so a node that names several definitions can join
/// them without re-deriving the spelling.
fn def_label(defs: &DefTable, id: DefId) -> String {
    let name = &defs[id].name;
    if name.is_empty() { format!("{id}") } else { format!("`{name}` {id}") }
}

/// How a reference is printed: ``-> `main` #47``.
fn arrow(defs: &DefTable, res: Res) -> String {
    match res {
        Res::Def(id) => format!("-> `{}` {id}", defs[id].name),
        Res::SelfTy(id) => format!("-> Self of {id}"),
        Res::Error => "-> <error>".to_string(),
    }
}

fn flag(header: &mut String, on: bool, word: &str) {
    if on {
        header.push(' ');
        header.push_str(word);
    }
}

fn literal_header(literal: &Literal) -> String {
    match literal {
        Literal::Int { value, .. } => format!("Int {value}"),
        Literal::Float { value, .. } => format!("Float {value}"),
        Literal::Str(value) => format!("Str {value:?}"),
        Literal::Char(value) => format!("Char {value:?}"),
        Literal::Bool(value) => format!("Bool {value}"),
        Literal::Null => "Null".to_string(),
    }
}

// --- modules and items ---------------------------------------------------

impl DumpIn for Module {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Module", self.def);
        w.node(&header, self.span, |w| {
            w.items(&nodes(defs, &self.items));
        });
    }
}

impl DumpIn for Item {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        // The wrapper's span only repeats its child's, so it prints nothing of
        // its own — the parser's dump makes the same choice.
        match &self.kind {
            ItemKind::Fn(node) => node.dump_in(defs, w),
            ItemKind::Record(node) => node.dump_in(defs, w),
            ItemKind::Choice(node) => node.dump_in(defs, w),
            ItemKind::Alias(node) => node.dump_in(defs, w),
            ItemKind::Const(node) => node.dump_in(defs, w),
            ItemKind::Interface(node) => node.dump_in(defs, w),
            ItemKind::Impl(node) => node.dump_in(defs, w),
            ItemKind::Extern(node) => node.dump_in(defs, w),
        }
    }
}

impl DumpIn for ExternBlock {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let mut header = format!("Extern {:?}", self.abi);
        if self.is_unsafe {
            header.push_str(" unsafe");
        }
        w.node(&header, self.span, |w| {
            match &self.library {
                Some(library) => library.dump_in(defs, w),
                None => w.note("library (none)"),
            }
            w.list("items", &nodes(defs, &self.items));
        });
    }
}

impl DumpIn for ExternLibrary {
    fn dump_in(&self, _defs: &DefTable, w: &mut DumpWriter) {
        let mut header = format!("Library {:?}", self.name);
        if self.static_link {
            header.push_str(" static");
        }
        if self.when_available {
            header.push_str(" when-available");
        }
        if let Some(module) = &self.pkg_config {
            header.push_str(&format!(" pkg-config {module:?}"));
        }
        w.leaf(&header, self.span);
    }
}

impl DumpIn for ExternItem {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        // As in the parser dump, the wrapper span only repeats its child's.
        match &self.kind {
            ExternItemKind::Fn(node) => node.dump_in(defs, w),
            ExternItemKind::Alias(node) => node.dump_in(defs, w),
            ExternItemKind::Const(node) => node.dump_in(defs, w),
            ExternItemKind::Static(node) => node.dump_in(defs, w),
            ExternItemKind::Union(node) => node.dump_in(defs, w),
        }
    }
}

impl DumpIn for ExternFn {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let mut header = defined(defs, "ExternFn", self.def);
        if self.variadic {
            header.push_str(" variadic");
        }
        if let Some(symbol) = &self.symbol {
            header.push_str(&format!(" symbol {symbol:?}"));
        }
        w.node(&header, self.span, |w| {
            w.list("params", &nodes(defs, &self.params));
            if let Some(ret) = &self.ret {
                w.child("ret", &Node(defs, ret));
            }
        });
    }
}

impl DumpIn for ExternAlias {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "ExternAlias", self.def);
        w.node(&header, self.span, |w| w.child("type", &Node(defs, &self.ty)));
    }
}

impl DumpIn for ExternConst {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let mut header = defined(defs, "ExternConst", self.def);
        if self.negative {
            header.push_str(" negative");
        }
        w.node(&header, self.span, |w| {
            w.note(&format!("value: {}", literal_header(&self.value)));
            w.child("type", &Node(defs, &self.ty));
        });
    }
}

impl DumpIn for ExternStatic {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "ExternStatic", self.def);
        w.node(&header, self.span, |w| w.child("type", &Node(defs, &self.ty)));
    }
}

impl DumpIn for ExternUnion {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "ExternUnion", self.def);
        w.node(&header, self.span, |w| {
            w.note(&format!("size: {}", layout_number(self.size)));
            w.note(&format!("align: {}", layout_number(self.align)));
        });
    }
}

/// A foreign union size or alignment, or that it was missing and reported.
fn layout_number(value: Option<u128>) -> String {
    match value {
        Some(value) => value.to_string(),
        None => "(none)".to_string(),
    }
}

impl DumpIn for Fn {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Fn", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            if let Some(receiver) = &self.self_param {
                w.child("receiver", &Node(defs, receiver));
            }
            w.list("params", &nodes(defs, &self.params));
            if let Some(ret) = &self.ret {
                w.child("ret", &Node(defs, ret));
            }
            w.list("where", &nodes(defs, &self.where_clause));
            match &self.body {
                Some(body) => w.child("body", &Node(defs, body)),
                None => w.note("body: (none)"),
            }
        });
    }
}

impl DumpIn for GenericParam {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            GenericParamKind::Type { bounds } => {
                let header = defined(defs, "TypeParam", self.def);
                w.node(&header, self.span, |w| {
                    w.list("bounds", &nodes(defs, bounds));
                });
            }
            GenericParamKind::Const { kind, annotation } => {
                let header = defined(defs, "ConstParam", self.def);
                w.node(&header, self.span, |w| {
                    // The annotation is a kind, not a type, so it has no node
                    // of its own to descend into — just the word and its span.
                    w.leaf(&format!("kind: {}", kind.describe()), *annotation);
                });
            }
        }
    }
}

impl DumpIn for Bound {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            BoundKind::Interface { res, generics } => {
                let header = format!("Bound {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.list("generics", &nodes(defs, generics));
                });
            }
            // No arrow: a closure bound resolved to no definition, because it
            // names none. What it resolved to is its parts.
            BoundKind::Closure { params, ret } => {
                w.node("ClosureBound", self.span, |w| {
                    w.list("params", &nodes(defs, params));
                    w.child("ret", &Node(defs, ret.as_ref()));
                });
            }
        }
    }
}

impl DumpIn for WherePredicate {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        w.node("Where", self.span, |w| {
            w.child("type", &Node(defs, &self.ty));
            w.list("bounds", &nodes(defs, &self.bounds));
        });
    }
}

impl DumpIn for SelfParam {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let mut header = defined(defs, "SelfParam", self.def);
        match self.kind {
            SelfKind::Shared => {}
            SelfKind::Mutable => header.push_str(" mutable"),
            SelfKind::Value => header.push_str(" by value"),
        }
        w.leaf(&header, self.span);
    }
}

impl DumpIn for Param {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Param", self.def);
        w.node(&header, self.span, |w| {
            w.child("type", &Node(defs, &self.ty));
        });
    }
}

impl DumpIn for Record {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Record", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            w.list("where", &nodes(defs, &self.where_clause));
            w.list("fields", &nodes(defs, &self.fields));
        });
    }
}

impl DumpIn for Field {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Field", self.def);
        w.node(&header, self.span, |w| {
            w.child("type", &Node(defs, &self.ty));
        });
    }
}

impl DumpIn for Choice {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Choice", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            w.list("where", &nodes(defs, &self.where_clause));
            w.list("variants", &nodes(defs, &self.variants));
        });
    }
}

impl DumpIn for Variant {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Variant", self.def);
        w.node(&header, self.span, |w| {
            w.list("payload", &nodes(defs, &self.payload));
        });
    }
}

impl DumpIn for Alias {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Alias", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            w.child("type", &Node(defs, &self.ty));
        });
    }
}

impl DumpIn for Const {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Const", self.def);
        w.node(&header, self.span, |w| {
            if let Some(ty) = &self.ty {
                w.child("type", &Node(defs, ty));
            }
            w.child("value", &Node(defs, &self.value));
        });
    }
}

impl DumpIn for Interface {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "Interface", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            w.list("supers", &nodes(defs, &self.supers));
            w.list("where", &nodes(defs, &self.where_clause));
            w.list("assoc types", &nodes(defs, &self.assoc_types));
            w.list("methods", &nodes(defs, &self.methods));
        });
    }
}

impl DumpIn for AssocType {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = defined(defs, "AssocType", self.def);
        match &self.ty {
            // An interface declares the name and stops there; printing an
            // empty child would suggest something went missing.
            None => w.leaf(&header, self.span),
            Some(ty) => w.node(&header, self.span, |w| {
                w.child("is", &Node(defs, ty));
            }),
        }
    }
}

impl DumpIn for Impl {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = format!("Impl {}", self.def);
        w.node(&header, self.span, |w| {
            w.list("generics", &nodes(defs, &self.generics));
            if let Some(interface) = &self.interface {
                w.child("interface", &Node(defs, interface));
            }
            w.child("for", &Node(defs, &self.self_ty));
            w.list("where", &nodes(defs, &self.where_clause));
            w.list("assoc types", &nodes(defs, &self.assoc_types));
            w.list("methods", &nodes(defs, &self.methods));
        });
    }
}

// --- types ---------------------------------------------------------------

impl DumpIn for Type {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            TypeKind::Path { res, generics } => {
                let header = format!("Path {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.list("generics", &nodes(defs, generics));
                });
            }
            TypeKind::Nullable(inner) => w.node("Nullable", self.span, |w| {
                Node(defs, inner.as_ref()).dump_node(w);
            }),
            TypeKind::Borrowed { mutable, inner } => {
                let mut header = "Borrowed".to_string();
                flag(&mut header, *mutable, "mutable");
                w.node(&header, self.span, |w| {
                    Node(defs, inner.as_ref()).dump_node(w);
                });
            }
            TypeKind::Any(bound) => w.node("Any", self.span, |w| {
                Node(defs, bound).dump_node(w);
            }),
            TypeKind::Tuple(elems) => w.node("Tuple", self.span, |w| {
                w.items(&nodes(defs, elems));
            }),
            TypeKind::Closure { params, ret } => w.node("Closure", self.span, |w| {
                w.list("params", &nodes(defs, params));
                w.child("ret", &Node(defs, ret.as_ref()));
            }),
            TypeKind::Unit => w.leaf("Unit", self.span),
            TypeKind::SelfType(res) => {
                w.leaf(&format!("SelfType {}", arrow(defs, *res)), self.span)
            }
            TypeKind::SelfAssoc { res, name } => w.leaf(
                &format!("SelfAssoc `{}` {}", name.name, arrow(defs, *res)),
                self.span,
            ),
            // The parser's `ConstArg`, under this phase's name for it. The
            // root shares the argument's span, so the two share a line; see
            // the parser's dump for the rule.
            // The const expression is this phase's own node now, because a
            // resolved one names its parameters by definition rather than by
            // text. So this rendering is this phase's too, and it is the one
            // place a const expression can be printed two ways: the parser's
            // dump shows the name, and this one shows what it resolved to.
            TypeKind::Const(value) => match &value.kind {
                // A bare literal stays one line. The root shares the
                // argument's span, so a wrapper that merely repeats its
                // child's span is not printed — the same rule the parser's
                // dump follows, kept here so the two read alike.
                ConstExprKind::Lit(literal) => {
                    w.leaf(&format!("Const {}", literal_header(literal)), self.span)
                }
                // A negation names its operator on the same line for the same
                // reason: it too shares the argument's span.
                ConstExprKind::Neg(operand) => w.node("Const Neg", self.span, |w| {
                    Node(defs, operand.as_ref()).dump_node(w)
                }),
                _ => w.node("Const", self.span, |w| Node(defs, value).dump_node(w)),
            },
            TypeKind::Error => w.leaf("Error", self.span),
        }
    }
}

// --- blocks and statements -----------------------------------------------

impl DumpIn for Block {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        w.node("Block", self.span, |w| {
            w.items(&nodes(defs, &self.stmts));
            if let Some(tail) = &self.tail {
                w.child("tail", &Node(defs, tail.as_ref()));
            }
        });
    }
}

impl DumpIn for ConstExpr {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            ConstExprKind::Lit(literal) => w.leaf(&literal_header(literal), self.span),
            ConstExprKind::Neg(operand) => {
                w.node("Neg", self.span, |w| Node(defs, operand.as_ref()).dump_node(w))
            }
            ConstExprKind::Param(res) => {
                w.leaf(&format!("ConstParam {}", arrow(defs, *res)), self.span)
            }
            ConstExprKind::Add(lhs, rhs) => w.node("Add", self.span, |w| {
                Node(defs, lhs.as_ref()).dump_node(w);
                Node(defs, rhs.as_ref()).dump_node(w);
            }),
            ConstExprKind::Sub(lhs, rhs) => w.node("Sub", self.span, |w| {
                Node(defs, lhs.as_ref()).dump_node(w);
                Node(defs, rhs.as_ref()).dump_node(w);
            }),
            ConstExprKind::Mul { operand, factor, .. } => {
                w.node(&format!("Mul by {}", literal_header(factor)), self.span, |w| {
                    Node(defs, operand.as_ref()).dump_node(w)
                })
            }
            ConstExprKind::Div { operand, divisor, .. } => {
                w.node(&format!("Div by {}", literal_header(divisor)), self.span, |w| {
                    Node(defs, operand.as_ref()).dump_node(w)
                })
            }
        }
    }
}

impl DumpIn for Stmt {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            StmtKind::Let(decl) => {
                let joined = decl
                    .bindings
                    .iter()
                    .map(|b| def_label(defs, b.def))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut header = format!("Let {joined}");
                flag(&mut header, decl.mutable, "mutable");
                w.node(&header, decl.span, |w| {
                    for binding in &decl.bindings {
                        if let Some(ty) = &binding.ty {
                            w.child("type", &Node(defs, ty));
                        }
                    }
                    w.child("value", &Node(defs, &decl.value));
                });
            }
            // An expression statement shows as the expression, as in the AST.
            StmtKind::Expr(expr) => expr.dump_in(defs, w),
            StmtKind::Assign { target, value } => w.node("Assign", self.span, |w| {
                w.child("target", &Node(defs, target));
                w.child("value", &Node(defs, value));
            }),
            StmtKind::Return(value) => w.node("Return", self.span, |w| {
                if let Some(value) = value {
                    Node(defs, value).dump_node(w);
                }
            }),
            StmtKind::Break(value) => w.node("Break", self.span, |w| {
                if let Some(value) = value {
                    Node(defs, value).dump_node(w);
                }
            }),
            StmtKind::Continue => w.leaf("Continue", self.span),
            StmtKind::Error => w.leaf("Error", self.span),
        }
    }
}

// --- expressions ---------------------------------------------------------

impl DumpIn for Expr {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            ExprKind::Literal(literal) => w.leaf(&literal_header(literal), self.span),
            ExprKind::FString(parts) => w.node("FString", self.span, |w| {
                for part in parts {
                    match part {
                        FStringPart::Text(text) => w.leaf(&format!("Text {text:?}"), self.span),
                        FStringPart::Hole(expr) => w.child("hole", &Node(defs, expr)),
                    }
                }
            }),
            ExprKind::Path { res, generics } => {
                let header = format!("Path {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.list("generics", &nodes(defs, generics));
                });
            }
            ExprKind::SelfValue(res) => {
                w.leaf(&format!("Self {}", arrow(defs, *res)), self.span)
            }
            ExprKind::Call { callee, args } => w.node("Call", self.span, |w| {
                w.child("callee", &Node(defs, callee.as_ref()));
                w.list("args", &nodes(defs, args));
            }),
            ExprKind::MethodCall { receiver, method, generics, args } => {
                // The method is still a name: §7.1 leaves it to `science-types`.
                let header = format!("MethodCall `{}` (unresolved: needs the type)", method.name);
                w.node(&header, self.span, |w| {
                    w.child("receiver", &Node(defs, receiver.as_ref()));
                    w.list("generics", &nodes(defs, generics));
                    w.list("args", &nodes(defs, args));
                });
            }
            ExprKind::Field { base, name } => {
                let header = format!("Field `{}` (unresolved: needs the type)", name.name);
                w.node(&header, self.span, |w| {
                    Node(defs, base.as_ref()).dump_node(w);
                });
            }
            ExprKind::Index { base, index } => w.node("Index", self.span, |w| {
                w.child("base", &Node(defs, base.as_ref()));
                w.child("index", &Node(defs, index.as_ref()));
            }),
            // Named for the AST node it came from, so that `sciencec ast` and
            // `sciencec resolve` print the same word for the same literal.
            ExprKind::ArrayLit(elements) => w.node("ArrayLit", self.span, |w| {
                w.items(&nodes(defs, elements));
            }),
            ExprKind::StructLit { res, fields } => {
                let header = format!("StructLit {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.list("fields", &nodes(defs, fields));
                });
            }
            ExprKind::Tuple(elems) => w.node("Tuple", self.span, |w| {
                w.items(&nodes(defs, elems));
            }),
            ExprKind::Unit => w.leaf("Unit", self.span),
            ExprKind::Unary { op, operand } => {
                w.node(&format!("Unary `{}`", op.as_str()), self.span, |w| {
                    Node(defs, operand.as_ref()).dump_node(w);
                })
            }
            ExprKind::Binary { op, lhs, rhs } => {
                w.node(&format!("Binary `{}`", op.as_str()), self.span, |w| {
                    w.child("lhs", &Node(defs, lhs.as_ref()));
                    w.child("rhs", &Node(defs, rhs.as_ref()));
                })
            }
            ExprKind::Cast { expr, ty } => w.node("Cast", self.span, |w| {
                Node(defs, expr.as_ref()).dump_node(w);
                w.child("type", &Node(defs, ty));
            }),
            ExprKind::Present(inner) => w.node("Present", self.span, |w| {
                Node(defs, inner.as_ref()).dump_node(w);
            }),
            ExprKind::Borrowed { mutable, expr } => {
                let mut header = "Borrowed".to_string();
                flag(&mut header, *mutable, "mutable");
                w.node(&header, self.span, |w| {
                    Node(defs, expr.as_ref()).dump_node(w);
                });
            }
            ExprKind::Range { start, end, inclusive } => {
                let mut header = "Range".to_string();
                flag(&mut header, *inclusive, "inclusive");
                w.node(&header, self.span, |w| {
                    w.child("start", &Node(defs, start.as_ref()));
                    w.child("end", &Node(defs, end.as_ref()));
                });
            }
            ExprKind::Closure { param, body } => {
                let header = defined(defs, "Closure", *param);
                w.node(&header, self.span, |w| {
                    w.child("body", &Node(defs, body.as_ref()));
                });
            }
            ExprKind::Each(res) => {
                w.leaf(&format!("Each {}", arrow(defs, *res)), self.span)
            }
            ExprKind::If(if_expr) => w.node("If", self.span, |w| {
                w.child("cond", &Node(defs, if_expr.cond.as_ref()));
                w.child("then", &Node(defs, &if_expr.then_branch));
                if let Some(else_branch) = &if_expr.else_branch {
                    w.child("else", &Node(defs, else_branch.as_ref()));
                }
            }),
            ExprKind::Match(match_expr) => w.node("Match", self.span, |w| {
                w.child("scrutinee", &Node(defs, match_expr.scrutinee.as_ref()));
                w.list("arms", &nodes(defs, &match_expr.arms));
            }),
            ExprKind::Loop { body } => w.node("Loop", self.span, |w| {
                w.child("body", &Node(defs, body));
            }),
            ExprKind::For { pattern, iter, body } => w.node("For", self.span, |w| {
                w.child("pattern", &Node(defs, pattern));
                w.child("iter", &Node(defs, iter.as_ref()));
                w.child("body", &Node(defs, body));
            }),
            ExprKind::Unsafe(body) => w.node("Unsafe", self.span, |w| {
                w.child("body", &Node(defs, body));
            }),
            ExprKind::Block(block) => block.dump_in(defs, w),
            ExprKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl DumpIn for Arg {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        // The label is not a reference and has no arrow: there is nothing in
        // any scope for it to point at. See `Resolver::resolve_args`.
        match &self.name {
            None => self.value.dump_in(defs, w),
            Some(name) => {
                let header = format!("Arg `{}` (a label, not a name)", name.name);
                w.node(&header, self.span, |w| {
                    Node(defs, &self.value).dump_node(w);
                });
            }
        }
    }
}

impl DumpIn for FieldInit {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = format!("FieldInit `{}` {}", self.name.name, arrow(defs, self.field));
        w.node(&header, self.span, |w| {
            w.child("value", &Node(defs, &self.value));
        });
    }
}

impl DumpIn for MatchArm {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        w.node("Arm", self.span, |w| {
            w.child("pattern", &Node(defs, &self.pattern));
            w.child("body", &Node(defs, &self.body));
        });
    }
}

// --- patterns ------------------------------------------------------------

impl DumpIn for Pattern {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        match &self.kind {
            PatternKind::Wildcard => w.leaf("Wildcard", self.span),
            PatternKind::Literal(literal) => w.leaf(&literal_header(literal), self.span),
            PatternKind::Binding { mutable, def } => {
                let mut header = defined(defs, "Binding", *def);
                flag(&mut header, *mutable, "mutable");
                w.leaf(&header, self.span);
            }
            PatternKind::Variant { res, elems } => {
                let header = format!("Variant {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.items(&nodes(defs, elems));
                });
            }
            PatternKind::Struct { res, fields } => {
                let header = format!("StructPattern {}", arrow(defs, *res));
                w.node(&header, self.span, |w| {
                    w.list("fields", &nodes(defs, fields));
                });
            }
            PatternKind::Tuple(elems) => w.node("Tuple", self.span, |w| {
                w.items(&nodes(defs, elems));
            }),
            PatternKind::Unit => w.leaf("Unit", self.span),
            PatternKind::Or(alts) => w.node("Or", self.span, |w| {
                w.items(&nodes(defs, alts));
            }),
            PatternKind::Error => w.leaf("Error", self.span),
        }
    }
}

impl DumpIn for FieldPattern {
    fn dump_in(&self, defs: &DefTable, w: &mut DumpWriter) {
        let header = format!("FieldPattern `{}` {}", self.name.name, arrow(defs, self.field));
        w.node(&header, self.span, |w| {
            Node(defs, &self.pattern).dump_node(w);
        });
    }
}
