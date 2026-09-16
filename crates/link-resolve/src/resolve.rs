//! Name resolution: AST in, HIR out.
//!
//! # The shape of the pass
//!
//! Four walks, in this order, and the order is the whole reason forward
//! references work:
//!
//! 1. **The module tree.** Every source file is placed in the tree by its
//!    path alone (§4.3, and [`crate::modules`] for the two decisions the spec
//!    leaves open). Nothing is resolved yet.
//! 2. **Collect definitions.** Every item in every module gets its `DefId`
//!    and its entry in its module's name table, before a single body is
//!    looked at. A function may therefore call one declared later in the file,
//!    and two types may name each other, without any fixed point: by the time
//!    anything is *resolved*, everything is *known*.
//! 3. **`use` declarations.** They may name anything in any module, so they
//!    run after step 2 and before step 4. An import is just another name in
//!    the importing module's table.
//! 4. **Bodies.** The recursive walk that turns paths into [`Res`], opens and
//!    closes ribs, and settles §4.4's three ambiguities.
//!
//! # Not stopping at the first error
//!
//! A name that does not resolve is reported once and becomes [`Res::Error`];
//! the walk continues into the rest of the expression, the rest of the
//! function and the rest of the file. A phase that bailed out would hand the
//! user one problem per compile. A `Res::Error` never produces a second
//! diagnostic of its own, which is what keeps one unknown name from turning
//! into a page of noise.
//!
//! # What this phase cannot do
//!
//! `d.title` and `d.summarize()` are not resolved here and cannot be: which
//! `title` depends on the type of `d`. They keep their `Ident` and belong to
//! `link-types`. So does whether an argument list has the right arity, whether
//! a struct literal names *all* the mandatory fields, and whether a `match` is
//! exhaustive.

use std::collections::HashMap;

use link_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use link_parser::ast;

use crate::builtins;
use crate::codes;
use crate::hir::{self, Crate, DefId, DefKind, DefTable, Res};
use crate::modules::module_chain;
use crate::scope::{RibKind, Scopes};

/// One source file handed to the resolver.
pub struct SourceModule {
    pub file: FileId,
    /// The file's path relative to the crate root, e.g. `text/parser.link`.
    /// It is what places the file in the module tree; see [`crate::modules`].
    pub path: String,
    pub ast: ast::Module,
}

/// Resolves a whole crate.
pub fn resolve_crate(sources: &[SourceModule]) -> (Crate, Diagnostics) {
    let inputs: Vec<Input<'_>> = sources
        .iter()
        .map(|s| Input { file: s.file, path: s.path.as_str(), ast: &s.ast })
        .collect();
    Resolver::new().run(&inputs)
}

/// Resolves a single file, which is the common case and every test's case.
pub fn resolve_module(file: FileId, path: &str, ast: &ast::Module) -> (Crate, Diagnostics) {
    Resolver::new().run(&[Input { file, path, ast }])
}

struct Input<'a> {
    file: FileId,
    path: &'a str,
    ast: &'a ast::Module,
}

// --- module scopes -------------------------------------------------------

/// The names one module offers.
///
/// `variants` is separate from `names` because §4.5 puts an enum's variants in
/// the module unqualified *as well as* qualified, and two enums are allowed to
/// share a variant name — which would be a duplicate definition if they shared
/// one table. Unqualified use of a name two enums declare is ambiguous, and
/// that is what the qualified form is for.
#[derive(Default)]
struct ModuleScope {
    names: HashMap<String, DefId>,
    variants: HashMap<String, Vec<DefId>>,
    children: HashMap<String, DefId>,
}

/// What step 2 recorded for one item, so step 4 does not re-derive it.
///
/// Parallel to the module's `items`, including the `use` declarations, so the
/// two walks stay index-aligned.
enum ItemDefs {
    Use,
    Fn(DefId),
    Struct { def: DefId, fields: Vec<DefId> },
    Enum { def: DefId, variants: Vec<DefId> },
    Trait(DefId),
    Impl(DefId),
}

// --- the resolver --------------------------------------------------------

struct Resolver {
    defs: DefTable,
    diags: Diagnostics,
    root: DefId,
    prelude: DefId,
    scopes: HashMap<DefId, ModuleScope>,
    /// Payload arity per variant, which decides whether a bare name in a
    /// pattern matches or binds (§4.4).
    variant_arity: HashMap<DefId, usize>,
    /// How many fields each struct has, which decides what `Doc()` means.
    struct_fields: HashMap<DefId, Vec<DefId>>,
    ribs: Scopes,
    current_module: DefId,
    /// The `impl` or `trait` whose `Self` is in scope, if any.
    self_owner: Option<DefId>,
}

impl Resolver {
    fn new() -> Self {
        let mut defs = DefTable::new();
        let root = defs.alloc(DefKind::Module, "", hir::BUILTIN_SPAN, None);
        let prelude = builtins::build(&mut defs);

        let mut scopes: HashMap<DefId, ModuleScope> = HashMap::new();
        scopes.insert(root, ModuleScope::default());
        let prelude_scope = scopes.entry(prelude.module).or_default();
        for (name, id) in prelude.names {
            prelude_scope.names.insert(name, id);
        }
        for (name, id) in prelude.variants {
            prelude_scope.variants.entry(name).or_default().push(id);
        }

        Resolver {
            defs,
            diags: Diagnostics::new(),
            root,
            prelude: prelude.module,
            scopes,
            variant_arity: prelude.variant_arity.into_iter().collect(),
            struct_fields: HashMap::new(),
            ribs: Scopes::new(),
            current_module: root,
            self_owner: None,
        }
    }

    fn run(mut self, sources: &[Input<'_>]) -> (Crate, Diagnostics) {
        // 1. Place every file in the module tree.
        let module_defs: Vec<DefId> = sources
            .iter()
            .map(|s| self.module_for_chain(&module_chain(s.path), Span::at(s.file, 0)))
            .collect();

        // 2. Collect definitions, so that step 4 can look forward.
        let collected: Vec<Vec<ItemDefs>> = sources
            .iter()
            .zip(&module_defs)
            .map(|(source, module)| self.collect_items(*module, source.ast))
            .collect();

        // 3. Imports, once every module knows what it offers.
        for (source, module) in sources.iter().zip(&module_defs) {
            self.resolve_uses(*module, source.ast);
        }

        // 4. Bodies.
        let modules: Vec<hir::Module> = sources
            .iter()
            .zip(&module_defs)
            .zip(&collected)
            .map(|((source, module), item_defs)| self.resolve_module(*module, source.ast, item_defs))
            .collect();

        debug_assert_eq!(self.ribs.depth(), 0, "the walk left a rib open");
        (Crate { defs: self.defs, root: self.root, modules }, self.diags)
    }

    // --- diagnostics -----------------------------------------------------

    fn error(&mut self, code: Code, message: impl Into<String>, span: Span, label: impl Into<String>) {
        self.diags
            .push(Diagnostic::error(code, message).with_label(Label::primary(span, label)));
    }

    /// The same, with a "defined here" note pointing at an existing
    /// definition — skipped when that definition is a builtin, whose span
    /// names no readable text.
    fn error_with_def(
        &mut self,
        code: Code,
        message: impl Into<String>,
        span: Span,
        label: impl Into<String>,
        note_at: DefId,
        note: impl Into<String>,
    ) {
        let mut diagnostic =
            Diagnostic::error(code, message).with_label(Label::primary(span, label));
        let def = self.defs.get(note_at);
        if def.is_builtin() {
            diagnostic = diagnostic.with_note(note.into());
        } else {
            diagnostic = diagnostic.with_label(Label::secondary(def.span, note));
        }
        self.diags.push(diagnostic);
    }

    /// Whether a name is one of the words §4.1 reserves for F1-F4.
    ///
    /// The lexer is the authority: it maps the word to `TokenKind::Reserved`,
    /// so asking it cannot drift from the list in the spec. In a well-formed
    /// compilation this never fires — the parser refuses a `Reserved` token
    /// where an identifier belongs, so no such `Ident` reaches the AST. It
    /// fires for a tree built by anything other than that parser, and when it
    /// does, "reserved for a later phase" beats "cannot find".
    fn is_reserved(name: &str) -> bool {
        matches!(link_lexer::TokenKind::from_word(name), Some(link_lexer::TokenKind::Reserved(_)))
    }

    /// Reports a reserved word, and says so. Returns whether it fired.
    fn check_reserved(&mut self, ident: &ast::Ident) -> bool {
        if !Self::is_reserved(&ident.name) {
            return false;
        }
        self.diags.push(
            Diagnostic::error(
                codes::RESERVED_WORD,
                format!("`{}` is reserved and cannot be used as a name", ident.name),
            )
            .with_label(Label::primary(ident.span, "reserved word"))
            .with_note(
                "it is held back so that a later phase of the language can use it \
                 without breaking code written today",
            ),
        );
        true
    }

    // --- step 1: the module tree -----------------------------------------

    fn module_for_chain(&mut self, chain: &[String], span: Span) -> DefId {
        let mut current = self.root;
        for name in chain {
            if let Some(existing) = self.scopes[&current].children.get(name) {
                current = *existing;
                continue;
            }
            let id = self.defs.alloc(DefKind::Module, name.clone(), span, Some(current));
            let scope = self.scopes.entry(current).or_default();
            scope.children.insert(name.clone(), id);
            scope.names.insert(name.clone(), id);
            self.scopes.entry(id).or_default();
            current = id;
        }
        current
    }

    // --- step 2: collecting definitions ----------------------------------

    fn collect_items(&mut self, module: DefId, ast: &ast::Module) -> Vec<ItemDefs> {
        ast.items.iter().map(|item| self.collect_item(module, item)).collect()
    }

    fn collect_item(&mut self, module: DefId, item: &ast::Item) -> ItemDefs {
        match &item.kind {
            ast::ItemKind::Use(_) => ItemDefs::Use,
            ast::ItemKind::Fn(decl) => {
                ItemDefs::Fn(self.declare_in_module(module, DefKind::Fn, &decl.name))
            }
            ast::ItemKind::Struct(decl) => {
                let def = self.declare_in_module(module, DefKind::Struct, &decl.name);
                let mut fields = Vec::new();
                let mut seen: HashMap<&str, DefId> = HashMap::new();
                for field in &decl.fields {
                    self.check_reserved(&field.name);
                    if let Some(previous) = seen.get(field.name.name.as_str()) {
                        self.duplicate(&field.name, *previous, "field");
                    }
                    let id =
                        self.defs.alloc(DefKind::Field, &field.name.name, field.name.span, Some(def));
                    seen.entry(field.name.name.as_str()).or_insert(id);
                    fields.push(id);
                }
                self.struct_fields.insert(def, fields.clone());
                ItemDefs::Struct { def, fields }
            }
            ast::ItemKind::Enum(decl) => {
                let def = self.declare_in_module(module, DefKind::Enum, &decl.name);
                let mut variants = Vec::new();
                let mut seen: HashMap<&str, DefId> = HashMap::new();
                for variant in &decl.variants {
                    self.check_reserved(&variant.name);
                    if let Some(previous) = seen.get(variant.name.name.as_str()) {
                        self.duplicate(&variant.name, *previous, "variant");
                    }
                    let id = self.defs.alloc(
                        DefKind::Variant,
                        &variant.name.name,
                        variant.name.span,
                        Some(def),
                    );
                    seen.entry(variant.name.name.as_str()).or_insert(id);
                    self.variant_arity.insert(id, variant.payload.len());
                    // §4.5: `Some(x)` and `Option.Some(x)` are the same thing.
                    self.scopes
                        .entry(module)
                        .or_default()
                        .variants
                        .entry(variant.name.name.clone())
                        .or_default()
                        .push(id);
                    variants.push(id);
                }
                ItemDefs::Enum { def, variants }
            }
            ast::ItemKind::Trait(decl) => {
                ItemDefs::Trait(self.declare_in_module(module, DefKind::Trait, &decl.name))
            }
            ast::ItemKind::Impl(block) => {
                // An impl has no name, so nothing goes in the name table; the
                // def exists to parent the methods and to answer `module_of`
                // for the orphan rule.
                ItemDefs::Impl(self.defs.alloc(DefKind::Impl, "", block.span, Some(module)))
            }
        }
    }

    /// Allocates an item's definition and puts its name in the module's table.
    ///
    /// A duplicate is reported and still allocated: the tree needs a `DefId`
    /// for the second declaration, and the *first* keeps the name so that the
    /// uses which follow resolve to something a reader would predict.
    fn declare_in_module(&mut self, module: DefId, kind: DefKind, name: &ast::Ident) -> DefId {
        self.check_reserved(name);
        let def = self.defs.alloc(kind, &name.name, name.span, Some(module));
        match self.scopes.entry(module).or_default().names.get(&name.name) {
            Some(previous) => {
                let previous = *previous;
                self.duplicate(name, previous, "name");
            }
            None => {
                self.scopes.entry(module).or_default().names.insert(name.name.clone(), def);
            }
        }
        def
    }

    fn duplicate(&mut self, name: &ast::Ident, previous: DefId, what: &str) {
        self.error_with_def(
            codes::DUPLICATE_DEFINITION,
            format!("the {what} `{}` is defined more than once", name.name),
            name.span,
            "defined again here",
            previous,
            "first defined here",
        );
    }

    // --- step 3: imports -------------------------------------------------

    fn resolve_uses(&mut self, module: DefId, ast: &ast::Module) {
        for item in &ast.items {
            let ast::ItemKind::Use(decl) = &item.kind else { continue };
            let Some(target) = self.resolve_module_path(&decl.path) else { continue };

            match &decl.imports {
                // `use text.parser` binds the module under its last segment.
                None => {
                    let last = decl.path.segments.last().expect("a path with no segments");
                    self.bind_import(module, &last.name, target);
                }
                // `use text.parser (Token, lex)` binds each listed name.
                Some(names) => {
                    for name in names {
                        match self.lookup_in_module(target, &name.name) {
                            Some(def) => self.bind_import(module, name, def),
                            None => self.error(
                                codes::UNRESOLVED_IMPORT,
                                format!(
                                    "`{}` is not defined in `{}`",
                                    name.name,
                                    self.defs.path_of(target)
                                ),
                                name.span,
                                "not found in that module",
                            ),
                        }
                    }
                }
            }
        }
    }

    /// Walks a `use` path from the crate root.
    ///
    /// A module path is absolute. The spec gives no relative form and no `mod`
    /// declaration to hang one off (§12), so `use text.parser` means the same
    /// thing written anywhere, which is also what makes `text.parser.Token`
    /// work in a type position from any module.
    fn resolve_module_path(&mut self, path: &ast::Path) -> Option<DefId> {
        let mut current = self.root;
        for segment in &path.segments {
            match self.scopes.get(&current).and_then(|s| s.children.get(&segment.name.name)) {
                Some(next) => current = *next,
                None => {
                    let where_ = if current == self.root {
                        "the crate root".to_string()
                    } else {
                        format!("`{}`", self.defs.path_of(current))
                    };
                    self.error(
                        codes::UNRESOLVED_IMPORT,
                        format!("there is no module `{}` in {where_}", segment.name.name),
                        segment.name.span,
                        "no such module",
                    );
                    return None;
                }
            }
        }
        Some(current)
    }

    fn bind_import(&mut self, module: DefId, name: &ast::Ident, def: DefId) {
        if let Some(previous) = self.scopes.entry(module).or_default().names.get(&name.name) {
            let previous = *previous;
            self.duplicate(name, previous, "name");
            return;
        }
        self.scopes.entry(module).or_default().names.insert(name.name.clone(), def);
    }

    /// A name offered by one module: an item, an import, or an unqualified
    /// variant of one of its enums.
    fn lookup_in_module(&mut self, module: DefId, name: &str) -> Option<DefId> {
        let scope = self.scopes.get(&module)?;
        if let Some(def) = scope.names.get(name) {
            return Some(*def);
        }
        match scope.variants.get(name).map(Vec::as_slice) {
            Some([only]) => Some(*only),
            Some(_) | None => None,
        }
    }

    // --- step 4: the walk ------------------------------------------------

    fn resolve_module(
        &mut self,
        module: DefId,
        ast: &ast::Module,
        item_defs: &[ItemDefs],
    ) -> hir::Module {
        self.current_module = module;
        let items = ast
            .items
            .iter()
            .zip(item_defs)
            .filter_map(|(item, defs)| self.resolve_item(item, defs))
            .collect();
        hir::Module { def: module, items, span: ast.span }
    }

    fn resolve_item(&mut self, item: &ast::Item, defs: &ItemDefs) -> Option<hir::Item> {
        let kind = match (&item.kind, defs) {
            // An import is a name in a scope; by now it is one, and there is
            // nothing left of it for a later phase to walk.
            (ast::ItemKind::Use(_), ItemDefs::Use) => return None,
            (ast::ItemKind::Fn(decl), ItemDefs::Fn(def)) => {
                hir::ItemKind::Fn(self.resolve_fn(*def, decl))
            }
            (ast::ItemKind::Struct(decl), ItemDefs::Struct { def, fields }) => {
                hir::ItemKind::Struct(self.resolve_struct(*def, fields, decl))
            }
            (ast::ItemKind::Enum(decl), ItemDefs::Enum { def, variants }) => {
                hir::ItemKind::Enum(self.resolve_enum(*def, variants, decl))
            }
            (ast::ItemKind::Trait(decl), ItemDefs::Trait(def)) => {
                hir::ItemKind::Trait(self.resolve_trait(*def, decl))
            }
            (ast::ItemKind::Impl(block), ItemDefs::Impl(def)) => {
                hir::ItemKind::Impl(self.resolve_impl(*def, block))
            }
            _ => unreachable!("the collected definitions fell out of step with the items"),
        };
        Some(hir::Item { kind, span: item.span })
    }

    fn resolve_fn(&mut self, def: DefId, decl: &ast::FnDecl) -> hir::Fn {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);

        self.ribs.push(RibKind::Params);
        let self_param = decl.self_param.as_ref().map(|receiver| {
            if self.self_owner.is_none() {
                self.error(
                    codes::SELF_OUTSIDE_IMPL,
                    "`self` is only a parameter of a method",
                    receiver.span,
                    "there is no `impl` or `trait` around this function",
                );
            }
            let id = self.defs.alloc(DefKind::SelfParam, "self", receiver.span, Some(def));
            let _ = self.ribs.define("self", id);
            hir::SelfParam { def: id, kind: receiver.kind, span: receiver.span }
        });

        let params = decl.params.iter().map(|p| self.resolve_param(def, p)).collect();
        let ret = decl.ret.as_ref().map(|t| self.resolve_type(t));
        let where_clause = self.resolve_where(&decl.where_clause);
        let body = decl.body.as_ref().map(|b| self.resolve_block(b));

        self.ribs.pop();
        self.ribs.pop();

        hir::Fn { def, generics, self_param, params, ret, where_clause, body, span: decl.span }
    }

    fn resolve_param(&mut self, owner: DefId, param: &ast::Param) -> hir::Param {
        self.check_reserved(&param.name);
        let ty = self.resolve_type(&param.ty);
        let def = self.defs.alloc(DefKind::Param, &param.name.name, param.name.span, Some(owner));
        if let Err(previous) = self.ribs.define(&param.name.name, def) {
            self.duplicate(&param.name, previous, "parameter");
        }
        hir::Param { def, ty, span: param.span }
    }

    /// Declares every generic parameter before resolving any bound, so that
    /// `[A: Into[B], B]` works regardless of the order they are written in.
    ///
    /// The caller must have pushed a `Generics` rib.
    fn declare_generics(
        &mut self,
        owner: DefId,
        params: &[ast::GenericParam],
    ) -> Vec<hir::GenericParam> {
        let ids: Vec<DefId> = params
            .iter()
            .map(|param| {
                self.check_reserved(&param.name);
                let def = self.defs.alloc(
                    DefKind::TypeParam,
                    &param.name.name,
                    param.name.span,
                    Some(owner),
                );
                if let Err(previous) = self.ribs.define(&param.name.name, def) {
                    self.duplicate(&param.name, previous, "generic parameter");
                }
                def
            })
            .collect();

        params
            .iter()
            .zip(ids)
            .map(|(param, def)| hir::GenericParam {
                def,
                bounds: param.bounds.iter().map(|b| self.resolve_bound(b)).collect(),
                span: param.span,
            })
            .collect()
    }

    fn resolve_where(&mut self, clause: &[ast::WherePredicate]) -> Vec<hir::WherePredicate> {
        clause
            .iter()
            .map(|predicate| hir::WherePredicate {
                ty: self.resolve_type(&predicate.ty),
                bounds: predicate.bounds.iter().map(|b| self.resolve_bound(b)).collect(),
                span: predicate.span,
            })
            .collect()
    }

    fn resolve_struct(
        &mut self,
        def: DefId,
        field_defs: &[DefId],
        decl: &ast::StructDecl,
    ) -> hir::Struct {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);
        let where_clause = self.resolve_where(&decl.where_clause);
        let fields = decl
            .fields
            .iter()
            .zip(field_defs)
            .map(|(field, def)| hir::Field {
                def: *def,
                ty: self.resolve_type(&field.ty),
                span: field.span,
            })
            .collect();
        self.ribs.pop();
        hir::Struct { def, generics, where_clause, fields, span: decl.span }
    }

    fn resolve_enum(
        &mut self,
        def: DefId,
        variant_defs: &[DefId],
        decl: &ast::EnumDecl,
    ) -> hir::Enum {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);
        let where_clause = self.resolve_where(&decl.where_clause);
        let variants = decl
            .variants
            .iter()
            .zip(variant_defs)
            .map(|(variant, def)| hir::Variant {
                def: *def,
                payload: variant.payload.iter().map(|t| self.resolve_type(t)).collect(),
                span: variant.span,
            })
            .collect();
        self.ribs.pop();
        hir::Enum { def, generics, where_clause, variants, span: decl.span }
    }

    fn resolve_trait(&mut self, def: DefId, decl: &ast::TraitDecl) -> hir::Trait {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);
        let supertraits = decl.supertraits.iter().map(|b| self.resolve_bound(b)).collect();
        let where_clause = self.resolve_where(&decl.where_clause);

        let previous_self = self.self_owner.replace(def);
        let methods = self.resolve_methods(def, &decl.methods);
        self.self_owner = previous_self;

        self.ribs.pop();
        hir::Trait { def, generics, supertraits, where_clause, methods, span: decl.span }
    }

    fn resolve_impl(&mut self, def: DefId, block: &ast::ImplBlock) -> hir::Impl {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &block.generics);
        let trait_ = block.trait_.as_ref().map(|b| self.resolve_bound(b));
        let self_ty = self.resolve_type(&block.self_ty);
        let where_clause = self.resolve_where(&block.where_clause);
        self.check_orphan(def, trait_.as_ref(), &self_ty, block.span);

        let previous_self = self.self_owner.replace(def);
        let methods = self.resolve_methods(def, &block.methods);
        self.self_owner = previous_self;

        self.ribs.pop();
        hir::Impl { def, generics, trait_, self_ty, where_clause, methods, span: block.span }
    }

    /// The methods of a trait or an impl.
    ///
    /// They are children of the block, not of the module: a method is reached
    /// through a receiver or through its type, never as a bare name, so it has
    /// no entry in any name table. Two methods with the same name in one block
    /// are still a duplicate, and that is checked here.
    fn resolve_methods(&mut self, owner: DefId, methods: &[ast::FnDecl]) -> Vec<hir::Fn> {
        let mut seen: HashMap<String, DefId> = HashMap::new();
        methods
            .iter()
            .map(|decl| {
                self.check_reserved(&decl.name);
                let def =
                    self.defs.alloc(DefKind::Fn, &decl.name.name, decl.name.span, Some(owner));
                match seen.get(&decl.name.name) {
                    Some(previous) => {
                        let previous = *previous;
                        self.duplicate(&decl.name, previous, "method");
                    }
                    None => {
                        seen.insert(decl.name.name.clone(), def);
                    }
                }
                self.resolve_fn(def, decl)
            })
            .collect()
    }

    /// §5.4: an `impl` is legal only if the trait or the type belongs to the
    /// module declaring it.
    ///
    /// Read literally, which is what makes two conflicting implementations
    /// impossible. Two consequences worth naming: an inherent `impl` is held
    /// to the same rule, since an inherent method on a foreign type collides
    /// just as badly; and a generic parameter cannot stand in for the type,
    /// because `impl Copy for T` would otherwise pass — a type parameter's
    /// module is the one that declared the `impl`.
    fn check_orphan(
        &mut self,
        def: DefId,
        trait_: Option<&hir::Bound>,
        self_ty: &hir::Type,
        span: Span,
    ) {
        let home = self.defs.module_of(def);

        // Nothing to say when a name already failed: the error is reported and
        // a second one about the same line helps nobody.
        if trait_.is_some_and(|b| b.res.is_error()) || matches!(self_ty.kind, hir::TypeKind::Error) {
            return;
        }

        let trait_owner = trait_.and_then(|b| b.res.def_id());
        let type_owner = type_owner(self_ty).filter(|id| {
            matches!(self.defs.get(*id).kind, DefKind::Struct | DefKind::Enum | DefKind::Primitive)
        });
        if let Some(id) = type_owner {
            if self.defs.module_of(id) == home {
                return;
            }
        }
        if let Some(id) = trait_owner {
            if self.defs.module_of(id) == home {
                return;
            }
        }

        let mut diagnostic = Diagnostic::error(
            codes::ORPHAN_IMPL,
            "this `impl` is an orphan: neither the trait nor the type belongs to this module",
        )
        .with_label(Label::primary(span, "declared here"));
        for (id, what) in [(trait_owner, "trait"), (type_owner, "type")] {
            let Some(id) = id else { continue };
            let Some(module) = self.defs.module_of(id) else { continue };
            let name = self.defs.path_of(id);
            let module = self.defs.path_of(module);
            let module = if module.is_empty() {
                "the crate root".to_string()
            } else {
                format!("`{module}`")
            };
            diagnostic =
                diagnostic.with_note(format!("the {what} `{name}` belongs to {module}"));
        }
        diagnostic = diagnostic.with_note(
            "move the `impl` into one of those modules, or wrap the type in one of your own",
        );
        self.diags.push(diagnostic);
    }

    // --- types -----------------------------------------------------------

    fn resolve_type(&mut self, ty: &ast::Type) -> hir::Type {
        let kind = match &ty.kind {
            ast::TypeKind::Path(path) => {
                let (res, generics) = self.resolve_path(path);
                let res = self.require_kind(res, path, DefKind::is_type, "a type");
                hir::TypeKind::Path { res, generics }
            }
            ast::TypeKind::Ref { mutable, inner } => hir::TypeKind::Ref {
                mutable: *mutable,
                inner: Box::new(self.resolve_type(inner)),
            },
            ast::TypeKind::Dyn(bound) => hir::TypeKind::Dyn(self.resolve_bound(bound)),
            ast::TypeKind::Tuple(elems) => {
                hir::TypeKind::Tuple(elems.iter().map(|t| self.resolve_type(t)).collect())
            }
            ast::TypeKind::Unit => hir::TypeKind::Unit,
            ast::TypeKind::SelfType => hir::TypeKind::SelfType(self.resolve_self_ty(ty.span)),
            ast::TypeKind::Error => hir::TypeKind::Error,
        };
        hir::Type { kind, span: ty.span }
    }

    fn resolve_self_ty(&mut self, span: Span) -> Res {
        match self.self_owner {
            Some(owner) => Res::SelfTy(owner),
            None => {
                self.error(
                    codes::SELF_OUTSIDE_IMPL,
                    "`Self` only exists inside an `impl` or a `trait`",
                    span,
                    "no enclosing `impl` or `trait`",
                );
                Res::Error
            }
        }
    }

    fn resolve_bound(&mut self, bound: &ast::TypeBound) -> hir::Bound {
        let (res, generics) = self.resolve_path(&bound.path);
        let res =
            self.require_kind(res, &bound.path, |kind| kind == DefKind::Trait, "a trait");
        hir::Bound { res, generics, span: bound.span }
    }

    /// Rejects a name used in a position its kind cannot fill, e.g. a function
    /// where a type belongs.
    fn require_kind(
        &mut self,
        res: Res,
        path: &ast::Path,
        allowed: impl Fn(DefKind) -> bool,
        expected: &str,
    ) -> Res {
        let Res::Def(id) = res else { return res };
        let kind = self.defs.get(id).kind;
        if allowed(kind) {
            return res;
        }
        self.error_with_def(
            codes::WRONG_NAMESPACE,
            format!("expected {expected}, but `{}` is {}", path.dotted(), kind.describe()),
            path.span,
            format!("not {expected}"),
            id,
            "defined here",
        );
        Res::Error
    }

    // --- paths -----------------------------------------------------------

    /// Resolves a path and the generic arguments written on it.
    ///
    /// Generic arguments are gathered from every segment and concatenated. In
    /// F0 only the final segment can carry any — an earlier segment is a
    /// module or an enum, neither of which takes arguments — so the
    /// concatenation is the last segment's list.
    fn resolve_path(&mut self, path: &ast::Path) -> (Res, Vec<hir::Type>) {
        let generics: Vec<hir::Type> = path
            .segments
            .iter()
            .flat_map(|segment| segment.generics.iter())
            .map(|ty| self.resolve_type(ty))
            .collect();

        let Some((first, rest)) = path.segments.split_first() else {
            return (Res::Error, generics);
        };
        let mut res = self.resolve_name(&first.name);
        for segment in rest {
            res = self.resolve_in(res, &segment.name);
        }
        (res, generics)
    }

    /// The first segment of a path, or a bare name.
    ///
    /// The order is the language's scoping rule in one list: ribs innermost
    /// first, then the current module, then its enums' variants, then the
    /// prelude, and last the crate root's modules — which is what lets
    /// `text.parser.Token` be written from anywhere without an import.
    ///
    /// A module does **not** see its parent's names. F0 has no `mod`
    /// declaration and no relative paths, so inheritance would be a rule
    /// invented here rather than one the spec states; `use` is the way in.
    fn resolve_name(&mut self, ident: &ast::Ident) -> Res {
        if let Some(def) = self.ribs.lookup(&ident.name) {
            return Res::Def(def);
        }
        for module in [self.current_module, self.prelude] {
            if let Some(def) = self.scopes.get(&module).and_then(|s| s.names.get(&ident.name)) {
                return Res::Def(*def);
            }
            match self.variants_named(module, &ident.name) {
                [] => {}
                [only] => return Res::Def(*only),
                several => {
                    let several = several.to_vec();
                    self.ambiguous(ident, &several);
                    return Res::Error;
                }
            }
        }
        if let Some(def) = self.scopes[&self.root].children.get(&ident.name) {
            return Res::Def(*def);
        }

        if self.check_reserved(ident) {
            return Res::Error;
        }
        self.error(
            codes::UNRESOLVED_NAME,
            format!("cannot find `{}` in this scope", ident.name),
            ident.span,
            "not found",
        );
        Res::Error
    }

    fn variants_named(&self, module: DefId, name: &str) -> &[DefId] {
        self.scopes
            .get(&module)
            .and_then(|s| s.variants.get(name))
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn ambiguous(&mut self, ident: &ast::Ident, candidates: &[DefId]) {
        let mut diagnostic = Diagnostic::error(
            codes::AMBIGUOUS_NAME,
            format!("`{}` is a variant of more than one enum in scope", ident.name),
        )
        .with_label(Label::primary(ident.span, "ambiguous"));
        for candidate in candidates {
            let qualified = self.defs.path_of(*candidate);
            diagnostic = diagnostic.with_note(format!("it could be `{qualified}`"));
        }
        diagnostic = diagnostic.with_note("write the enum's name to say which (§4.5)");
        self.diags.push(diagnostic);
    }

    /// A later segment of a path: `parser` in `text.parser`, `Some` in
    /// `Option.Some`.
    fn resolve_in(&mut self, base: Res, ident: &ast::Ident) -> Res {
        // A failed prefix has already been reported; saying so again for every
        // remaining segment helps nobody.
        let Res::Def(id) = base else { return Res::Error };

        match self.defs.get(id).kind {
            DefKind::Module => match self.lookup_in_module(id, &ident.name) {
                Some(def) => Res::Def(def),
                None => {
                    let module = self.defs.path_of(id);
                    let module =
                        if module.is_empty() { "the crate root".to_string() } else { module };
                    self.error(
                        codes::UNRESOLVED_NAME,
                        format!("cannot find `{}` in {module}", ident.name),
                        ident.span,
                        "not found in that module",
                    );
                    Res::Error
                }
            },
            DefKind::Enum => {
                let variant = self
                    .defs
                    .children(id)
                    .find(|child| child.kind == DefKind::Variant && child.name == ident.name)
                    .map(|child| child.id);
                match variant {
                    Some(def) => Res::Def(def),
                    None => {
                        let name = self.defs.path_of(id);
                        self.error(
                            codes::UNRESOLVED_NAME,
                            format!("`{name}` has no variant `{}`", ident.name),
                            ident.span,
                            "no such variant",
                        );
                        Res::Error
                    }
                }
            }
            kind => {
                self.error_with_def(
                    codes::NOT_A_MODULE,
                    format!(
                        "`{}` is {}, so `{}` cannot be reached through it",
                        self.defs.path_of(id),
                        kind.describe(),
                        ident.name
                    ),
                    ident.span,
                    "not a module or an enum",
                    id,
                    "defined here",
                );
                Res::Error
            }
        }
    }

    // --- blocks and statements -------------------------------------------

    fn resolve_block(&mut self, block: &ast::Block) -> hir::Block {
        self.ribs.push(RibKind::Block);
        let stmts = block.stmts.iter().map(|s| self.resolve_stmt(s)).collect();
        let tail = block.tail.as_ref().map(|e| Box::new(self.resolve_expr(e)));
        self.ribs.pop();
        hir::Block { stmts, tail, span: block.span }
    }

    fn resolve_stmt(&mut self, stmt: &ast::Stmt) -> hir::Stmt {
        let kind = match &stmt.kind {
            ast::StmtKind::Let(decl) => {
                self.check_reserved(&decl.name);
                let ty = decl.ty.as_ref().map(|t| self.resolve_type(t));
                // The initializer is resolved *before* the binding exists, so
                // `let x = x` reaches the outer `x` and a `let` is invisible
                // above its own line.
                let value = self.resolve_expr(&decl.value);
                let def = self.defs.alloc(
                    DefKind::Local,
                    &decl.name.name,
                    decl.name.span,
                    Some(self.current_module),
                );
                if let Err(previous) = self.ribs.define(&decl.name.name, def) {
                    self.duplicate(&decl.name, previous, "binding");
                }
                hir::StmtKind::Let(hir::Let {
                    def,
                    mutable: decl.mutable,
                    ty,
                    value,
                    span: decl.span,
                })
            }
            ast::StmtKind::Expr(expr) => hir::StmtKind::Expr(self.resolve_expr(expr)),
            ast::StmtKind::Assign { target, value } => hir::StmtKind::Assign {
                target: self.resolve_expr(target),
                value: self.resolve_expr(value),
            },
            ast::StmtKind::Return(value) => {
                hir::StmtKind::Return(value.as_ref().map(|e| self.resolve_expr(e)))
            }
            ast::StmtKind::Break(value) => {
                hir::StmtKind::Break(value.as_ref().map(|e| self.resolve_expr(e)))
            }
            ast::StmtKind::Continue => hir::StmtKind::Continue,
            ast::StmtKind::Error => hir::StmtKind::Error,
        };
        hir::Stmt { kind, span: stmt.span }
    }

    // --- expressions -----------------------------------------------------

    fn resolve_expr(&mut self, expr: &ast::Expr) -> hir::Expr {
        let kind = match &expr.kind {
            ast::ExprKind::Literal(literal) => hir::ExprKind::Literal(literal.clone()),
            ast::ExprKind::Path(path) => {
                let (res, generics) = self.resolve_path(path);
                let res = self.reject_module_as_value(res, path);
                hir::ExprKind::Path { res, generics }
            }
            ast::ExprKind::SelfValue => hir::ExprKind::SelfValue(self.resolve_self_value(expr.span)),
            ast::ExprKind::Call { callee, args } => self.resolve_call(callee, args, expr.span),
            ast::ExprKind::MethodCall { receiver, method, generics, args } => {
                hir::ExprKind::MethodCall {
                    receiver: Box::new(self.resolve_expr(receiver)),
                    method: method.clone(),
                    generics: generics.iter().map(|t| self.resolve_type(t)).collect(),
                    args: args.iter().map(|a| self.resolve_expr(a)).collect(),
                }
            }
            ast::ExprKind::Field { base, name } => hir::ExprKind::Field {
                base: Box::new(self.resolve_expr(base)),
                name: name.clone(),
            },
            ast::ExprKind::Index { base, index } => hir::ExprKind::Index {
                base: Box::new(self.resolve_expr(base)),
                index: Box::new(self.resolve_expr(index)),
            },
            ast::ExprKind::StructLit { path, fields } => self.resolve_struct_lit(path, fields),
            ast::ExprKind::Tuple(elems) => {
                hir::ExprKind::Tuple(elems.iter().map(|e| self.resolve_expr(e)).collect())
            }
            ast::ExprKind::Unit => hir::ExprKind::Unit,
            ast::ExprKind::Unary { op, operand } => hir::ExprKind::Unary {
                op: *op,
                operand: Box::new(self.resolve_expr(operand)),
            },
            ast::ExprKind::Binary { op, lhs, rhs } => hir::ExprKind::Binary {
                op: *op,
                lhs: Box::new(self.resolve_expr(lhs)),
                rhs: Box::new(self.resolve_expr(rhs)),
            },
            ast::ExprKind::Cast { expr, ty } => hir::ExprKind::Cast {
                expr: Box::new(self.resolve_expr(expr)),
                ty: self.resolve_type(ty),
            },
            ast::ExprKind::Try(inner) => hir::ExprKind::Try(Box::new(self.resolve_expr(inner))),
            ast::ExprKind::Ref { mutable, expr } => hir::ExprKind::Ref {
                mutable: *mutable,
                expr: Box::new(self.resolve_expr(expr)),
            },
            ast::ExprKind::If(if_expr) => hir::ExprKind::If(hir::IfExpr {
                cond: Box::new(self.resolve_expr(&if_expr.cond)),
                then_branch: self.resolve_block(&if_expr.then_branch),
                else_branch: if_expr
                    .else_branch
                    .as_ref()
                    .map(|e| Box::new(self.resolve_expr(e))),
                span: if_expr.span,
            }),
            ast::ExprKind::Match(match_expr) => hir::ExprKind::Match(self.resolve_match(match_expr)),
            ast::ExprKind::While { cond, body } => hir::ExprKind::While {
                cond: Box::new(self.resolve_expr(cond)),
                body: self.resolve_block(body),
            },
            ast::ExprKind::Loop { body } => hir::ExprKind::Loop { body: self.resolve_block(body) },
            ast::ExprKind::For { pattern, iter, body } => {
                // The iterable is evaluated outside the loop's bindings.
                let iter = Box::new(self.resolve_expr(iter));
                self.ribs.push(RibKind::Pattern);
                let pattern = self.resolve_pattern(pattern);
                let body = self.resolve_block(body);
                self.ribs.pop();
                hir::ExprKind::For { pattern, iter, body }
            }
            ast::ExprKind::Block(block) => hir::ExprKind::Block(self.resolve_block(block)),
            ast::ExprKind::Error => hir::ExprKind::Error,
        };
        hir::Expr { kind, span: expr.span }
    }

    fn resolve_self_value(&mut self, span: Span) -> Res {
        match self.ribs.lookup("self") {
            Some(def) => Res::Def(def),
            None => {
                self.error(
                    codes::SELF_OUTSIDE_IMPL,
                    "`self` is only available in a method with a `self` receiver",
                    span,
                    "no `self` in scope",
                );
                Res::Error
            }
        }
    }

    fn reject_module_as_value(&mut self, res: Res, path: &ast::Path) -> Res {
        let Res::Def(id) = res else { return res };
        if self.defs.get(id).kind != DefKind::Module {
            return res;
        }
        self.error(
            codes::WRONG_NAMESPACE,
            format!("`{}` is a module, not a value", path.dotted()),
            path.span,
            "a module has no value",
        );
        Res::Error
    }

    /// §4.4, second and third points: positional arguments mean a call or a
    /// variant, named arguments mean a struct. The parser cannot see the
    /// difference for `Doc()`, which has neither, so it is settled here
    /// against what `Doc` turned out to be.
    fn resolve_call(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Expr],
        span: Span,
    ) -> hir::ExprKind {
        let callee_res = match &callee.kind {
            ast::ExprKind::Path(path) => Some((self.resolve_path(path), path.clone())),
            _ => None,
        };

        let Some(((res, generics), path)) = callee_res else {
            return hir::ExprKind::Call {
                callee: Box::new(self.resolve_expr(callee)),
                args: args.iter().map(|a| self.resolve_expr(a)).collect(),
            };
        };

        if let Res::Def(id) = res {
            if self.defs.get(id).kind == DefKind::Struct {
                let field_count = self.struct_fields.get(&id).map_or(0, Vec::len);
                if args.is_empty() && field_count == 0 {
                    // `Doc()` on a struct with no fields is construction.
                    return hir::ExprKind::StructLit { res, fields: Vec::new() };
                }
                self.error_with_def(
                    codes::CONSTRUCTION_MISMATCH,
                    format!(
                        "`{}` is a struct, so it is built with named arguments",
                        path.dotted()
                    ),
                    span,
                    "positional arguments here",
                    id,
                    "this struct is declared here",
                );
            }
        }

        hir::ExprKind::Call {
            callee: Box::new(hir::Expr {
                kind: hir::ExprKind::Path { res, generics },
                span: callee.span,
            }),
            args: args.iter().map(|a| self.resolve_expr(a)).collect(),
        }
    }

    fn resolve_struct_lit(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldInit],
    ) -> hir::ExprKind {
        let (res, _) = self.resolve_path(path);
        let struct_def = self.expect_struct(res, path, "named arguments build a struct");

        let fields = fields
            .iter()
            .map(|init| hir::FieldInit {
                field: self.resolve_field(struct_def, &init.name),
                name: init.name.clone(),
                value: self.resolve_expr(&init.value),
                span: init.span,
            })
            .collect();
        hir::ExprKind::StructLit { res, fields }
    }

    /// The definition behind a name used where only a struct can go.
    fn expect_struct(&mut self, res: Res, path: &ast::Path, why: &str) -> Option<DefId> {
        let Res::Def(id) = res else { return None };
        let kind = self.defs.get(id).kind;
        if kind == DefKind::Struct {
            return Some(id);
        }
        self.error_with_def(
            codes::CONSTRUCTION_MISMATCH,
            format!("{why}, but `{}` is {}", path.dotted(), kind.describe()),
            path.span,
            "not a struct",
            id,
            "defined here",
        );
        None
    }

    /// One field of a known struct.
    ///
    /// Which fields are *missing* is not checked here: §4.3 makes them
    /// mandatory, but a missing field is a shape error about a value, and the
    /// type checker is already walking the same node to check the types of the
    /// ones that are present.
    fn resolve_field(&mut self, struct_def: Option<DefId>, name: &ast::Ident) -> Res {
        let Some(struct_def) = struct_def else { return Res::Error };
        let field = self
            .defs
            .children(struct_def)
            .find(|child| child.kind == DefKind::Field && child.name == name.name)
            .map(|child| child.id);
        match field {
            Some(def) => Res::Def(def),
            None => {
                let owner = self.defs.path_of(struct_def);
                self.error(
                    codes::UNKNOWN_FIELD,
                    format!("`{owner}` has no field `{}`", name.name),
                    name.span,
                    "no such field",
                );
                Res::Error
            }
        }
    }

    fn resolve_match(&mut self, match_expr: &ast::MatchExpr) -> hir::MatchExpr {
        let scrutinee = Box::new(self.resolve_expr(&match_expr.scrutinee));
        let arms = match_expr
            .arms
            .iter()
            .map(|arm| {
                self.ribs.push(RibKind::Pattern);
                let pattern = self.resolve_pattern(&arm.pattern);
                let body = self.resolve_expr(&arm.body);
                self.ribs.pop();
                hir::MatchArm { pattern, body, span: arm.span }
            })
            .collect();
        hir::MatchExpr { scrutinee, arms, span: match_expr.span }
    }

    // --- patterns --------------------------------------------------------

    fn resolve_pattern(&mut self, pattern: &ast::Pattern) -> hir::Pattern {
        let kind = match &pattern.kind {
            ast::PatternKind::Wildcard => hir::PatternKind::Wildcard,
            ast::PatternKind::Literal(literal) => hir::PatternKind::Literal(literal.clone()),

            // §4.4, first point: a bare name is a binding until resolution
            // says otherwise. It says otherwise exactly when the name is a
            // variant carrying no payload — which is what makes
            // `match x: None: ...` mean what the reader expects.
            ast::PatternKind::Binding { mutable, name } => match self.unit_variant(&name.name) {
                Some(variant) => hir::PatternKind::Variant { res: Res::Def(variant), elems: vec![] },
                None => {
                    self.check_reserved(name);
                    let def = self.defs.alloc(
                        DefKind::Local,
                        &name.name,
                        name.span,
                        Some(self.current_module),
                    );
                    if let Err(previous) = self.ribs.define(&name.name, def) {
                        self.duplicate(name, previous, "binding");
                    }
                    hir::PatternKind::Binding { mutable: *mutable, def }
                }
            },

            ast::PatternKind::Variant { path, elems } => {
                let (res, _) = self.resolve_path(path);
                // §4.4, second point, in pattern position: the parser writes
                // `Doc()` as a variant because it cannot know better.
                if let Res::Def(id) = res {
                    if self.defs.get(id).kind == DefKind::Struct {
                        if elems.is_empty() {
                            return hir::Pattern {
                                kind: hir::PatternKind::Struct { res, fields: Vec::new() },
                                span: pattern.span,
                            };
                        }
                        self.error_with_def(
                            codes::CONSTRUCTION_MISMATCH,
                            format!(
                                "`{}` is a struct, so its pattern names its fields",
                                path.dotted()
                            ),
                            pattern.span,
                            "positional elements here",
                            id,
                            "this struct is declared here",
                        );
                    }
                }
                hir::PatternKind::Variant {
                    res,
                    elems: elems.iter().map(|p| self.resolve_pattern(p)).collect(),
                }
            }

            ast::PatternKind::Struct { path, fields } => {
                let (res, _) = self.resolve_path(path);
                let struct_def =
                    self.expect_struct(res, path, "a named-field pattern matches a struct");
                hir::PatternKind::Struct {
                    res,
                    fields: fields
                        .iter()
                        .map(|field| hir::FieldPattern {
                            field: self.resolve_field(struct_def, &field.name),
                            name: field.name.clone(),
                            pattern: self.resolve_pattern(&field.pattern),
                            span: field.span,
                        })
                        .collect(),
                }
            }

            ast::PatternKind::Tuple(elems) => {
                hir::PatternKind::Tuple(elems.iter().map(|p| self.resolve_pattern(p)).collect())
            }
            ast::PatternKind::Unit => hir::PatternKind::Unit,
            // Every alternative binds into the same rib: `A(x) | B(x)` is one
            // `x`. Whether the alternatives agree on what they bind is a type
            // question, not a name question.
            ast::PatternKind::Or(alts) => {
                hir::PatternKind::Or(alts.iter().map(|p| self.resolve_pattern(p)).collect())
            }
            ast::PatternKind::Error => hir::PatternKind::Error,
        };
        hir::Pattern { kind, span: pattern.span }
    }

    /// The variant a bare name in a pattern refers to, if it refers to one.
    ///
    /// Ribs are deliberately not consulted: a name in a pattern never means
    /// the local it would mean in an expression, it introduces a new one. Only
    /// an item can take the name away from it, and only a unit variant does.
    fn unit_variant(&self, name: &str) -> Option<DefId> {
        for module in [self.current_module, self.prelude] {
            let candidate = match self.scopes.get(&module).and_then(|s| s.names.get(name)) {
                Some(def) => Some(*def),
                None => match self.variants_named(module, name) {
                    [only] => Some(*only),
                    _ => None,
                },
            };
            if let Some(def) = candidate {
                let is_unit = self.defs.get(def).kind == DefKind::Variant
                    && self.variant_arity.get(&def).copied().unwrap_or(0) == 0;
                return is_unit.then_some(def);
            }
        }
        None
    }
}

/// The type an `impl` is for, once references are peeled off.
fn type_owner(ty: &hir::Type) -> Option<DefId> {
    match &ty.kind {
        hir::TypeKind::Path { res, .. } => res.def_id(),
        hir::TypeKind::Ref { inner, .. } => type_owner(inner),
        _ => None,
    }
}
