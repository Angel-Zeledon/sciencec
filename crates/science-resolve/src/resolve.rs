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
//! `science-types`. So does the *label* on a named argument — `docs.sort(by: f)`
//! names a parameter, and there is no `by` in any scope to look up. So does
//! whether an argument list has the right arity, whether a record literal names
//! *all* the mandatory fields, and whether a `match` is exhaustive.

use std::collections::HashMap;

use science_diagnostics::{Code, Diagnostic, Diagnostics, FileId, Label, Span};
use science_parser::ast;

use crate::builtins;
use crate::codes;
use crate::hir::{self, Crate, DefId, DefKind, DefTable, Res};
use crate::modules::module_chain;
use crate::scope::{RibKind, Scopes};

/// One source file handed to the resolver.
pub struct SourceModule {
    pub file: FileId,
    /// The file's path relative to the crate root, e.g. `text/parser.science`.
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
/// `variants` is separate from `names` because §4.5 puts a choice type's
/// variants in the module unqualified *as well as* qualified, and two choice
/// types are allowed to share a variant name — which would be a duplicate
/// definition if they shared one table. Unqualified use of a name two choice
/// types declare is ambiguous, and that is what the qualified form is for.
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
    Record { def: DefId, fields: Vec<DefId> },
    Choice { def: DefId, variants: Vec<DefId> },
    Alias(DefId),
    Const(DefId),
    Interface(DefId),
    Impl(DefId),
    /// One entry per item in the block, in order. `None` is an item the
    /// parser could not read, which has no name to declare and nothing to
    /// resolve.
    Extern(Vec<Option<DefId>>),
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
    /// How many fields each record has, which decides what `Doc()` means.
    record_fields: HashMap<DefId, Vec<DefId>>,
    ribs: Scopes,
    current_module: DefId,
    /// The implementation or interface whose `Self` is in scope, if any. It
    /// is also the parent of the associated types `Self.Item` can reach.
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
        // A module of the prelude gets a scope of its own, so `ffi.Span` is
        // read by the same two-segment path walk as `text.parser.Token`.
        for (module, names) in prelude.modules {
            let scope = scopes.entry(module).or_default();
            for (name, id) in names {
                scope.names.insert(name, id);
            }
        }

        Resolver {
            defs,
            diags: Diagnostics::new(),
            root,
            prelude: prelude.module,
            scopes,
            variant_arity: prelude.variant_arity.into_iter().collect(),
            record_fields: HashMap::new(),
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
        matches!(science_lexer::TokenKind::from_word(name), Some(science_lexer::TokenKind::Reserved(_)))
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
            ast::ItemKind::Record(decl) => {
                let def = self.declare_in_module(module, DefKind::Record, &decl.name);
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
                self.record_fields.insert(def, fields.clone());
                ItemDefs::Record { def, fields }
            }
            ast::ItemKind::Choice(decl) => {
                let def = self.declare_in_module(module, DefKind::Choice, &decl.name);
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
                ItemDefs::Choice { def, variants }
            }
            ast::ItemKind::Alias(decl) => {
                ItemDefs::Alias(self.declare_in_module(module, DefKind::Alias, &decl.name))
            }
            ast::ItemKind::Const(decl) => {
                ItemDefs::Const(self.declare_in_module(module, DefKind::Const, &decl.name))
            }
            ast::ItemKind::Interface(decl) => {
                ItemDefs::Interface(self.declare_in_module(
                    module,
                    DefKind::Interface,
                    &decl.name,
                ))
            }
            // §1.2 of `ffi-c-boundary.md` keeps the block a list of symbols,
            // and the names in that list are ordinary module-level names:
            // `cblas_dgemm` is called like any function and `BlasInt` is
            // written like any alias. So each item is declared in the module
            // exactly as the corresponding declaration outside the block would
            // be, and everything downstream of here can forget the block
            // existed. What it may not forget is which kind each name is, and
            // `DefKind::Static` and `DefKind::Union` exist for that.
            ast::ItemKind::Extern(block) => ItemDefs::Extern(
                block
                    .items
                    .iter()
                    .map(|item| {
                        let (kind, name) = match &item.kind {
                            ast::ExternItemKind::Fn(decl) => (DefKind::ExternFn, &decl.name),
                            ast::ExternItemKind::Alias(decl) => (DefKind::Alias, &decl.name),
                            ast::ExternItemKind::Const(decl) => (DefKind::Const, &decl.name),
                            ast::ExternItemKind::Static(decl) => (DefKind::Static, &decl.name),
                            ast::ExternItemKind::Union(decl) => (DefKind::Union, &decl.name),
                            ast::ExternItemKind::Error => return None,
                        };
                        Some(self.declare_in_module(module, kind, name))
                    })
                    .collect(),
            ),
            ast::ItemKind::Impl(block) => {
                // An implementation has no name, so nothing goes in the name
                // table; the def exists to parent the methods and the
                // associated types, and to answer `module_of` for the orphan
                // rule.
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
            (ast::ItemKind::Record(decl), ItemDefs::Record { def, fields }) => {
                hir::ItemKind::Record(self.resolve_record(*def, fields, decl))
            }
            (ast::ItemKind::Choice(decl), ItemDefs::Choice { def, variants }) => {
                hir::ItemKind::Choice(self.resolve_choice(*def, variants, decl))
            }
            (ast::ItemKind::Alias(decl), ItemDefs::Alias(def)) => {
                hir::ItemKind::Alias(self.resolve_alias(*def, decl))
            }
            (ast::ItemKind::Const(decl), ItemDefs::Const(def)) => {
                hir::ItemKind::Const(self.resolve_const(*def, decl))
            }
            (ast::ItemKind::Interface(decl), ItemDefs::Interface(def)) => {
                hir::ItemKind::Interface(self.resolve_interface(*def, decl))
            }
            (ast::ItemKind::Impl(block), ItemDefs::Impl(def)) => {
                hir::ItemKind::Impl(self.resolve_impl(*def, block))
            }
            (ast::ItemKind::Extern(block), ItemDefs::Extern(defs)) => {
                hir::ItemKind::Extern(self.resolve_extern(block, defs))
            }
            _ => unreachable!("the collected definitions fell out of step with the items"),
        };
        Some(hir::Item { kind, span: item.span, doc: item.doc.clone() })
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
                    "there is no implementation or interface around this function",
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

    /// An `extern` block: the types its items mention, resolved.
    ///
    /// The names were declared in step 2, so a foreign function may be called
    /// from a function written above it and a `type BlasInt is I32` may be
    /// used by a declaration that precedes it — the same forward reference
    /// every other item gets, for the same reason.
    ///
    /// What this phase leaves alone, and why:
    ///
    /// - **Whether a type is FFI-representable.** The parser rejects the
    ///   Science layouts it can name (`SC0420`, `SC0421`); whether a user type
    ///   implements `ffi.CLayout`, and whether every field transitively does
    ///   (`SC0424`), belongs to the type checker, which has the definitions.
    /// - **`unsafe` at the use site.** Calling one of these functions, and
    ///   reading or writing a `static` (`SC0490`), require an `unsafe` block.
    ///   That is a property of an expression context, not of a name.
    /// - **The library, the ABI and the `symbol` string.** All three belong to
    ///   the driver, and none of them is a name in any scope.
    fn resolve_extern(
        &mut self,
        block: &ast::ExternBlock,
        defs: &[Option<DefId>],
    ) -> hir::ExternBlock {
        let items = block
            .items
            .iter()
            .zip(defs)
            .filter_map(|(item, def)| {
                let def = (*def)?;
                let kind = match &item.kind {
                    ast::ExternItemKind::Fn(decl) => {
                        hir::ExternItemKind::Fn(self.resolve_extern_fn(def, decl))
                    }
                    ast::ExternItemKind::Alias(decl) => {
                        hir::ExternItemKind::Alias(hir::ExternAlias {
                            def,
                            ty: self.resolve_type(&decl.ty),
                            span: decl.span,
                        })
                    }
                    ast::ExternItemKind::Const(decl) => {
                        hir::ExternItemKind::Const(hir::ExternConst {
                            def,
                            negative: decl.negative,
                            value: decl.value.clone(),
                            ty: self.resolve_type(&decl.ty),
                            span: decl.span,
                        })
                    }
                    ast::ExternItemKind::Static(decl) => {
                        hir::ExternItemKind::Static(hir::ExternStatic {
                            def,
                            ty: self.resolve_type(&decl.ty),
                            span: decl.span,
                        })
                    }
                    ast::ExternItemKind::Union(decl) => {
                        hir::ExternItemKind::Union(hir::ExternUnion {
                            def,
                            size: decl.size,
                            align: decl.align,
                            span: decl.span,
                        })
                    }
                    // Declared as nothing in step 2, so there is nothing here.
                    ast::ExternItemKind::Error => return None,
                };
                Some(hir::ExternItem { kind, span: item.span })
            })
            .collect();

        hir::ExternBlock {
            is_unsafe: block.is_unsafe,
            abi: block.abi.value.clone(),
            library: block.library.as_ref().map(|library| hir::ExternLibrary {
                name: library.name.value.clone(),
                pkg_config: library.pkg_config.as_ref().map(|m| m.value.clone()),
                static_link: library.static_link.is_some(),
                when_available: library.when_available.is_some(),
                span: library.span,
            }),
            items,
            span: block.span,
        }
    }

    /// A foreign function gets a rib for its parameters although it has no
    /// body to see them: §1.7 permits naming them at the call site, so two
    /// parameters sharing a name is a duplicate, and it has to be reported
    /// here rather than at every call.
    fn resolve_extern_fn(&mut self, def: DefId, decl: &ast::ExternFn) -> hir::ExternFn {
        self.ribs.push(RibKind::Params);
        let params = decl.params.iter().map(|p| self.resolve_param(def, p)).collect();
        let ret = decl.ret.as_ref().map(|t| self.resolve_type(t));
        self.ribs.pop();

        hir::ExternFn {
            def,
            params,
            ret,
            symbol: decl.symbol.as_ref().map(|s| s.value.clone()),
            variadic: decl.variadic.is_some(),
            span: decl.span,
        }
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
    /// `of (A: Into of B, B)` works regardless of the order they are written
    /// in.
    ///
    /// §5.3's two kinds land in two different `DefKind`s: a type parameter
    /// names a type, a `const` parameter names a value, and a later phase that
    /// confuses the two would accept `let n be T`. They share one rib all the
    /// same — they are written in one list and one name cannot mean both.
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
                let kind = match &param.kind {
                    ast::GenericParamKind::Type { .. } => DefKind::TypeParam,
                    ast::GenericParamKind::Const { .. } => DefKind::ConstParam,
                };
                let def =
                    self.defs.alloc(kind, &param.name.name, param.name.span, Some(owner));
                if let Err(previous) = self.ribs.define(&param.name.name, def) {
                    self.duplicate(&param.name, previous, "generic parameter");
                }
                def
            })
            .collect();

        let params: Vec<hir::GenericParam> = params
            .iter()
            .zip(ids)
            .map(|(param, def)| {
                let kind = match &param.kind {
                    ast::GenericParamKind::Type { bounds } => hir::GenericParamKind::Type {
                        bounds: bounds.iter().map(|b| self.resolve_bound(b)).collect(),
                    },
                    ast::GenericParamKind::Const { ty } => hir::GenericParamKind::Const {
                        kind: self.const_param_kind(ty),
                        annotation: ty.span,
                    },
                };
                hir::GenericParam { def, kind, span: param.span }
            })
            .collect();
        self.check_one_variadic_at_most(&params);
        params
    }

    /// Classifies a const parameter's annotation as one of the kinds
    /// `const-expression-arithmetic.md` §2.3 admits.
    ///
    /// The annotation is a **kind**, not a type, so it is matched on the name
    /// as written rather than resolved: `Shape` names no type, is in no scope
    /// and will never be in one, because a shape is a type-level list with no
    /// values (§6.1). Resolving it would report an unresolved name for the one
    /// kind that is spelled correctly.
    ///
    /// The parser is unchanged, per that note's §2.3, so everything
    /// `parse_type` accepts arrives here — `borrowed Int`, `Array of Int`,
    /// `any Ord`, a dotted path. None of them is a kind and all of them are
    /// [`codes::NOT_A_CONST_PARAM_KIND`].
    fn const_param_kind(&mut self, ty: &ast::Type) -> hir::ConstParamKind {
        let named = match &ty.kind {
            ast::TypeKind::Path(path) => match path.segments.as_slice() {
                [segment] if segment.generics.is_empty() => Some(&segment.name.name),
                _ => None,
            },
            _ => None,
        };

        if let Some(kind) = named.and_then(|name| hir::ConstParamKind::from_name(name)) {
            return kind;
        }

        let kinds = hir::ConstParamKind::NAMES
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(" or ");
        let label = match named {
            Some(name) => format!("`{name}` is not a const-parameter kind"),
            None => "not a const-parameter kind".to_string(),
        };
        self.diags.push(
            Diagnostic::error(
                codes::NOT_A_CONST_PARAM_KIND,
                format!("a const generic parameter's kind must be {kinds}"),
            )
            .with_label(Label::primary(ty.span, label))
            .with_note(
                "`const N: K` annotates `N` with a kind, not with a type: `Int` is an \
                 integer known at compile time, and `Shape` is a type-level list of them",
            ),
        );
        hir::ConstParamKind::Error
    }

    /// At most one parameter in a list may absorb a run of arguments
    /// (`const-expression-arithmetic.md` §10.1 item 6).
    ///
    /// With one variadic parameter a declaration's arity is a range and every
    /// argument still has exactly one parameter to belong to. With two there
    /// is no split, so the second is reported and
    /// [`hir::GenericArity::of`] keeps the first.
    fn check_one_variadic_at_most(&mut self, params: &[hir::GenericParam]) {
        let mut variadic = params.iter().filter(|param| param.is_variadic());
        let Some(first) = variadic.next() else { return };
        for extra in variadic {
            let name = self.defs.get(extra.def).name.clone();
            let previous = self.defs.get(first.def).name.clone();
            self.diags.push(
                Diagnostic::error(
                    codes::REPEATED_VARIADIC_PARAM,
                    "a generic parameter list may declare at most one variadic parameter",
                )
                .with_label(Label::primary(
                    extra.span,
                    format!("`{name}` also absorbs a run of arguments"),
                ))
                .with_label(Label::secondary(
                    first.span,
                    format!("`{previous}` already does"),
                ))
                .with_note(
                    "a `Shape` parameter stands for a whole list of const arguments, so two \
                     of them in one list leave no way to say where the first ends",
                ),
            );
        }
    }

    /// The associated types a block declares (§5.4).
    ///
    /// They are allocated before anything else in the block is resolved, so
    /// that `Self.Item` reaches one wherever it is written — in a method's
    /// signature, in its body, or in the type another associated type is bound
    /// to. They are children of the block and go in no name table: an
    /// associated type is reached through `Self`, never as a bare name, which
    /// is a namespace of its own with exactly one way in.
    fn declare_assoc_types(&mut self, owner: DefId, names: &[&ast::Ident]) -> Vec<DefId> {
        let mut seen: HashMap<String, DefId> = HashMap::new();
        names
            .iter()
            .map(|name| {
                self.check_reserved(name);
                let def =
                    self.defs.alloc(DefKind::AssocType, &name.name, name.span, Some(owner));
                match seen.get(&name.name) {
                    Some(previous) => {
                        let previous = *previous;
                        self.duplicate(name, previous, "associated type");
                    }
                    None => {
                        seen.insert(name.name.clone(), def);
                    }
                }
                def
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

    fn resolve_record(
        &mut self,
        def: DefId,
        field_defs: &[DefId],
        decl: &ast::RecordDecl,
    ) -> hir::Record {
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
        hir::Record { def, generics, where_clause, fields, span: decl.span }
    }

    fn resolve_choice(
        &mut self,
        def: DefId,
        variant_defs: &[DefId],
        decl: &ast::ChoiceDecl,
    ) -> hir::Choice {
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
        hir::Choice { def, generics, where_clause, variants, span: decl.span }
    }

    /// `type Embedding is Array of F32` (§4.4).
    ///
    /// The generic parameters are an ordinary `of` list, so the alias gets the
    /// same rib every other type declaration gets, and the aliased type is
    /// resolved inside it.
    fn resolve_alias(&mut self, def: DefId, decl: &ast::AliasDecl) -> hir::Alias {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);
        let ty = self.resolve_type(&decl.ty);
        self.ribs.pop();
        hir::Alias { def, generics, ty, span: decl.span }
    }

    /// `const WIDTH be 768` (§4.4).
    ///
    /// The value is an expression at module level, so it sees the module's
    /// names and no ribs. Whether it can be evaluated at compile time is a
    /// question about the expression, not about its names.
    fn resolve_const(&mut self, def: DefId, decl: &ast::ConstDecl) -> hir::Const {
        let ty = decl.ty.as_ref().map(|t| self.resolve_type(t));
        let value = self.resolve_expr(&decl.value);
        hir::Const { def, ty, value, span: decl.span }
    }

    fn resolve_interface(&mut self, def: DefId, decl: &ast::InterfaceDecl) -> hir::Interface {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &decl.generics);
        let supers = decl.supers.iter().map(|b| self.resolve_bound(b)).collect();
        let where_clause = self.resolve_where(&decl.where_clause);

        let names: Vec<&ast::Ident> = decl.assoc_types.iter().map(|a| &a.name).collect();
        let assoc_defs = self.declare_assoc_types(def, &names);
        // An interface declares the name and leaves the type to the
        // implementation, so there is nothing here to resolve.
        let assoc_types: Vec<hir::AssocType> = decl
            .assoc_types
            .iter()
            .zip(assoc_defs)
            .map(|(decl, def)| hir::AssocType { def, ty: None, span: decl.span })
            .collect();

        let previous_self = self.self_owner.replace(def);
        let methods = self.resolve_methods(def, &decl.methods);
        self.self_owner = previous_self;

        self.ribs.pop();
        hir::Interface {
            def,
            generics,
            supers,
            where_clause,
            assoc_types,
            methods,
            span: decl.span,
        }
    }

    fn resolve_impl(&mut self, def: DefId, block: &ast::ImplBlock) -> hir::Impl {
        self.ribs.push(RibKind::Generics);
        let generics = self.declare_generics(def, &block.generics);
        let interface = block.interface.as_ref().map(|b| self.resolve_bound(b));
        let self_ty = self.resolve_type(&block.self_ty);
        let where_clause = self.resolve_where(&block.where_clause);
        self.check_orphan(def, interface.as_ref(), &self_ty, block.span);

        let previous_self = self.self_owner.replace(def);

        // Declared before the bindings are resolved, so that `type A is Self.B`
        // reaches `B` however the two are ordered.
        let names: Vec<&ast::Ident> = block.assoc_types.iter().map(|a| &a.name).collect();
        let assoc_defs = self.declare_assoc_types(def, &names);
        let assoc_types: Vec<hir::AssocType> = block
            .assoc_types
            .iter()
            .zip(assoc_defs)
            .map(|(binding, def)| hir::AssocType {
                def,
                ty: Some(self.resolve_type(&binding.ty)),
                span: binding.span,
            })
            .collect();

        let methods = self.resolve_methods(def, &block.methods);
        self.self_owner = previous_self;

        self.ribs.pop();
        hir::Impl {
            def,
            generics,
            interface,
            self_ty,
            where_clause,
            assoc_types,
            methods,
            span: block.span,
        }
    }

    /// The methods of an interface or an implementation.
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

    /// §5.4: an implementation is legal only if the interface or the type
    /// belongs to the module declaring it.
    ///
    /// Read literally, which is what makes two conflicting implementations
    /// impossible. Two consequences worth naming: an inherent `Doc has:` is
    /// held to the same rule, since an inherent method on a foreign type
    /// collides just as badly; and a generic parameter cannot stand in for the
    /// type, because `T implements Copy:` would otherwise pass — a type
    /// parameter's module is the one that declared the implementation.
    ///
    /// A type alias is not an owner either. `type MyText is String` in this
    /// module does not make `String` this module's to implement on, so an
    /// alias resolves to a `DefKind::Alias` and falls outside the set below.
    fn check_orphan(
        &mut self,
        def: DefId,
        interface: Option<&hir::Bound>,
        self_ty: &hir::Type,
        span: Span,
    ) {
        let home = self.defs.module_of(def);

        // Nothing to say when a name already failed: the error is reported and
        // a second one about the same line helps nobody.
        if interface.is_some_and(|b| b.res.is_error())
            || matches!(self_ty.kind, hir::TypeKind::Error)
        {
            return;
        }

        let interface_owner = interface.and_then(|b| b.res.def_id());
        let type_owner = type_owner(self_ty).filter(|id| {
            matches!(self.defs.get(*id).kind, DefKind::Record | DefKind::Choice | DefKind::Primitive)
        });
        if let Some(id) = type_owner {
            if self.defs.module_of(id) == home {
                return;
            }
        }
        if let Some(id) = interface_owner {
            if self.defs.module_of(id) == home {
                return;
            }
        }

        let mut diagnostic = Diagnostic::error(
            codes::ORPHAN_IMPL,
            "this implementation is an orphan: neither the interface nor the type \
             belongs to this module",
        )
        .with_label(Label::primary(span, "declared here"));
        for (id, what) in [(interface_owner, "interface"), (type_owner, "type")] {
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
            "move the implementation into one of those modules, or wrap the type in one \
             of your own",
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
            ast::TypeKind::Nullable(inner) => {
                hir::TypeKind::Nullable(Box::new(self.resolve_nullable_inner(inner)))
            }
            ast::TypeKind::Borrowed { mutable, inner } => hir::TypeKind::Borrowed {
                mutable: *mutable,
                inner: Box::new(self.resolve_type(inner)),
            },
            ast::TypeKind::Any(bound) => hir::TypeKind::Any(self.resolve_bound(bound)),
            ast::TypeKind::Tuple(elems) => {
                hir::TypeKind::Tuple(elems.iter().map(|t| self.resolve_type(t)).collect())
            }
            ast::TypeKind::Unit => hir::TypeKind::Unit,
            ast::TypeKind::SelfType => hir::TypeKind::SelfType(self.resolve_self_ty(ty.span)),
            ast::TypeKind::SelfAssoc(name) => hir::TypeKind::SelfAssoc {
                res: self.resolve_self_assoc(name, ty.span),
                name: name.clone(),
            },
            // A const generic argument is a literal. There is no name in it, so
            // there is nothing for this phase to say about it.
            ast::TypeKind::Const(value) => hir::TypeKind::Const(value.clone()),
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
                    "`Self` only exists inside an implementation or an interface",
                    span,
                    "no enclosing implementation or interface",
                );
                Res::Error
            }
        }
    }

    /// `Self.Item` (§5.4).
    ///
    /// The one way into the associated-type namespace, and it looks in exactly
    /// one place: the block `Self` stands for. An interface finds what it
    /// declared; an implementation finds what it bound. An implementation that
    /// uses `Self.Item` without binding `Item` is *not* answered from the
    /// interface's declaration — the binding is the implementation's to supply,
    /// and a missing one is §5.4's completeness check, which `science-types`
    /// runs over the same block.
    fn resolve_self_assoc(&mut self, name: &ast::Ident, span: Span) -> Res {
        let Some(owner) = self.self_owner else {
            self.error(
                codes::SELF_OUTSIDE_IMPL,
                "`Self` only exists inside an implementation or an interface",
                span,
                "no enclosing implementation or interface",
            );
            return Res::Error;
        };
        let found = self
            .defs
            .children(owner)
            .find(|child| child.kind == DefKind::AssocType && child.name == name.name)
            .map(|child| child.id);
        match found {
            Some(def) => Res::Def(def),
            None => {
                let what = match self.defs.get(owner).kind {
                    DefKind::Interface => "interface",
                    _ => "implementation",
                };
                self.error(
                    codes::UNRESOLVED_NAME,
                    format!("this {what} declares no associated type `{}`", name.name),
                    name.span,
                    "no such associated type",
                );
                Res::Error
            }
        }
    }

    /// The inside of a `T?`, with §3.4's `Error?` shorthand applied.
    ///
    /// A bare interface name under a `?` means the optional trait object
    /// `(any Error)?`. The shorthand exists because that spelling appears in
    /// the signature of every fallible function in the language and
    /// `-> (Config, (any Error)?)` is not a signature anyone should read.
    ///
    /// §3.4 says the shorthand holds "in a return position". The rule here is
    /// wider — anywhere under a `?` — and it has to be: §3.1's own example
    /// `let missing: Error? be null` is an annotation on a binding and not a
    /// return type at all, so the narrower reading does not describe the
    /// note's own code. See the report in `syntax-revision-2.md` §3.4.
    ///
    /// Only a *bare* interface is rewritten. `any Error` written out stays
    /// what it is, and an interface anywhere else still gets `SC0211`,
    /// because `?` is the thing that makes the object form unambiguous.
    fn resolve_nullable_inner(&mut self, ty: &ast::Type) -> hir::Type {
        if let ast::TypeKind::Path(path) = &ty.kind {
            let (res, generics) = self.resolve_path(path);
            if let Res::Def(id) = res {
                if self.defs.get(id).kind == DefKind::Interface {
                    let bound = hir::Bound { res, generics, span: ty.span };
                    return hir::Type { kind: hir::TypeKind::Any(bound), span: ty.span };
                }
            }
            // The path was resolved to decide the question and resolving it
            // again would allocate a second set of diagnostics for one name,
            // so the ordinary path is rebuilt here rather than delegated.
            let res = self.require_kind(res, path, DefKind::is_type, "a type");
            return hir::Type {
                kind: hir::TypeKind::Path { res, generics },
                span: ty.span,
            };
        }
        self.resolve_type(ty)
    }

    fn resolve_bound(&mut self, bound: &ast::TypeBound) -> hir::Bound {
        let (res, generics) = self.resolve_path(&bound.path);
        let res = self.require_kind(
            res,
            &bound.path,
            |kind| kind == DefKind::Interface,
            "an interface",
        );
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
    /// module or a choice type, neither of which takes arguments — so the
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
    /// first, then the current module, then its choice types' variants, then the
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
            format!("`{}` is a variant of more than one choice type in scope", ident.name),
        )
        .with_label(Label::primary(ident.span, "ambiguous"));
        for candidate in candidates {
            let qualified = self.defs.path_of(*candidate);
            diagnostic = diagnostic.with_note(format!("it could be `{qualified}`"));
        }
        diagnostic = diagnostic.with_note("write the choice type's name to say which (§4.5)");
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
            DefKind::Choice => {
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
                    "not a module or a choice type",
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
                for binding in &decl.names {
                    self.check_reserved(&binding.name);
                }
                let types: Vec<_> = decl
                    .names
                    .iter()
                    .map(|b| b.ty.as_ref().map(|t| self.resolve_type(t)))
                    .collect();
                // The initializer is resolved *before* any binding exists, so
                // `let x = x` reaches the outer `x` and a `let` is invisible
                // above its own line. With several names that rule matters
                // more, not less: `let a, b be f(b)` must reach the outer `b`.
                let value = self.resolve_expr(&decl.value);
                let bindings = decl
                    .names
                    .iter()
                    .zip(types)
                    .map(|(binding, ty)| {
                        let def = self.defs.alloc(
                            DefKind::Local,
                            &binding.name.name,
                            binding.name.span,
                            Some(self.current_module),
                        );
                        if let Err(previous) = self.ribs.define(&binding.name.name, def) {
                            self.duplicate(&binding.name, previous, "binding");
                        }
                        hir::LetBinding { def, ty, span: binding.span }
                    })
                    .collect();
                hir::StmtKind::Let(hir::Let {
                    bindings,
                    mutable: decl.mutable,
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
                    args: self.resolve_args(args),
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
            ast::ExprKind::Present(inner) => {
                hir::ExprKind::Present(Box::new(self.resolve_expr(inner)))
            }
            ast::ExprKind::Borrowed { mutable, expr } => hir::ExprKind::Borrowed {
                mutable: *mutable,
                expr: Box::new(self.resolve_expr(expr)),
            },
            // Both ends are ordinary expressions; `..` binds nothing.
            ast::ExprKind::Range { start, end, inclusive } => hir::ExprKind::Range {
                start: Box::new(self.resolve_expr(start)),
                end: Box::new(self.resolve_expr(end)),
                inclusive: *inclusive,
            },
            ast::ExprKind::Closure { param, body } => self.resolve_closure(param, body, expr.span),
            ast::ExprKind::Each => hir::ExprKind::Each(self.resolve_each(expr.span)),
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
            // `while` is gone (revision 2 §2.1): a conditional loop is `loop`
            // with an `if .. break` in it, and the body is the only thing left
            // to walk. `resolve_block` opens the rib the condition never had.
            ast::ExprKind::Loop { body } => hir::ExprKind::Loop { body: self.resolve_block(body) },
            // `unsafe` grants powers, never scope: §3.2 of the FFI note keeps
            // every rule this phase enforces intact inside one, so the body is
            // resolved exactly as a bare block would be.
            ast::ExprKind::Unsafe(body) => hir::ExprKind::Unsafe(self.resolve_block(body)),
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

    /// §4.6's two closure forms, resolved into one.
    ///
    /// A closure binds exactly one name, so it gets a rib of its own. The
    /// named form binds what was written; the implicit form binds `each`,
    /// which the programmer did not write and which [`ast::ExprKind::Each`]
    /// inside the body then finds by the ordinary rib lookup. `each` is a
    /// keyword, so no user name can collide with the one invented here, and no
    /// local can shadow it.
    fn resolve_closure(
        &mut self,
        param: &Option<ast::Ident>,
        body: &ast::Expr,
        span: Span,
    ) -> hir::ExprKind {
        self.ribs.push(RibKind::Closure);
        let (name, name_span) = match param {
            Some(ident) => {
                self.check_reserved(ident);
                (ident.name.as_str(), ident.span)
            }
            None => ("each", span),
        };
        let def = self.defs.alloc(DefKind::Param, name, name_span, Some(self.current_module));
        let _ = self.ribs.define(name, def);
        let body = Box::new(self.resolve_expr(body));
        self.ribs.pop();
        hir::ExprKind::Closure { param: def, body }
    }

    /// `each` (§4.6).
    ///
    /// The parser wraps the call argument an `each` appears in, so in a
    /// well-formed program there is always a closure rib holding one. A bare
    /// `each` outside any argument reaches here unwrapped — the parser leaves
    /// that judgement to this phase because it has no scopes to make it with.
    fn resolve_each(&mut self, span: Span) -> Res {
        match self.ribs.lookup("each") {
            Some(def) => Res::Def(def),
            None => {
                self.diags.push(
                    Diagnostic::error(
                        codes::EACH_WITHOUT_SUBJECT,
                        "`each` names the subject of the call it is written in, and there \
                         is no call here",
                    )
                    .with_label(Label::primary(span, "no subject to name"))
                    .with_note(
                        "`each` stands for one argument's value inside a call, as in \
                         `docs.map(each.title)`",
                    ),
                );
                Res::Error
            }
        }
    }

    /// The arguments of a call.
    ///
    /// A named argument's label is *not* resolved. `docs.sort(by: f)` names the
    /// parameter `by`, and there is no `by` in any scope; matching it to a
    /// parameter needs the callee's signature, which is `science-types`' to know.
    /// The label is carried through unchanged so that it can.
    fn resolve_args(&mut self, args: &[ast::Arg]) -> Vec<hir::Arg> {
        args.iter()
            .map(|arg| hir::Arg {
                name: arg.name.clone(),
                value: self.resolve_expr(&arg.value),
                span: arg.span,
            })
            .collect()
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
    /// variant, named arguments mean a record. The parser cannot see the
    /// difference for `Doc()`, which has neither, so it is settled here
    /// against what `Doc` turned out to be.
    fn resolve_call(
        &mut self,
        callee: &ast::Expr,
        args: &[ast::Arg],
        span: Span,
    ) -> hir::ExprKind {
        let callee_res = match &callee.kind {
            ast::ExprKind::Path(path) => Some((self.resolve_path(path), path.clone())),
            _ => None,
        };

        let Some(((res, generics), path)) = callee_res else {
            return hir::ExprKind::Call {
                callee: Box::new(self.resolve_expr(callee)),
                args: self.resolve_args(args),
            };
        };

        if let Res::Def(id) = res {
            if self.defs.get(id).kind == DefKind::Record {
                let field_count = self.record_fields.get(&id).map_or(0, Vec::len);
                if args.is_empty() && field_count == 0 {
                    // `Doc()` on a record with no fields is construction.
                    return hir::ExprKind::StructLit { res, fields: Vec::new() };
                }
                self.error_with_def(
                    codes::CONSTRUCTION_MISMATCH,
                    format!(
                        "`{}` is a record type, so it is built with named arguments",
                        path.dotted()
                    ),
                    span,
                    "positional arguments here",
                    id,
                    "this record type is declared here",
                );
            }
        }

        hir::ExprKind::Call {
            callee: Box::new(hir::Expr {
                kind: hir::ExprKind::Path { res, generics },
                span: callee.span,
            }),
            args: self.resolve_args(args),
        }
    }

    fn resolve_struct_lit(
        &mut self,
        path: &ast::Path,
        fields: &[ast::FieldInit],
    ) -> hir::ExprKind {
        let (res, generics) = self.resolve_path(path);

        // §1.7 of `ffi-c-boundary.md`: an extern call site may use named
        // arguments, in any order, and it is the only call site in the
        // language that may. `cblas_dgemm` takes thirteen arguments, two of
        // which are transposition flags that look identical and whose
        // confusion produces a wrong answer rather than an error; the note
        // prices the second call convention and takes it deliberately.
        //
        // Syntactically this is indistinguishable from a record construction,
        // which is why it is settled here and not in the parser: only this
        // phase knows that the name is a foreign function.
        if matches!(res, Res::Def(id) if self.defs.get(id).kind == DefKind::ExternFn) {
            let args = fields
                .iter()
                .map(|init| hir::Arg {
                    name: Some(init.name.clone()),
                    value: self.resolve_expr(&init.value),
                    span: init.span,
                })
                .collect();
            return hir::ExprKind::Call {
                callee: Box::new(hir::Expr {
                    kind: hir::ExprKind::Path { res, generics },
                    span: path.span,
                }),
                args,
            };
        }

        let record_def = self.expect_record(res, path, "named arguments build a record");

        let fields = fields
            .iter()
            .map(|init| hir::FieldInit {
                field: self.resolve_field(record_def, &init.name),
                name: init.name.clone(),
                value: self.resolve_expr(&init.value),
                span: init.span,
            })
            .collect();
        hir::ExprKind::StructLit { res, fields }
    }

    /// The definition behind a name used where only a record can go.
    fn expect_record(&mut self, res: Res, path: &ast::Path, why: &str) -> Option<DefId> {
        let Res::Def(id) = res else { return None };
        let kind = self.defs.get(id).kind;
        if kind == DefKind::Record {
            return Some(id);
        }
        self.error_with_def(
            codes::CONSTRUCTION_MISMATCH,
            format!("{why}, but `{}` is {}", path.dotted(), kind.describe()),
            path.span,
            "not a record type",
            id,
            "defined here",
        );
        None
    }

    /// One field of a known record.
    ///
    /// Which fields are *missing* is not checked here: §4.4 makes them
    /// mandatory, but a missing field is a shape error about a value, and the
    /// type checker is already walking the same node to check the types of the
    /// ones that are present.
    fn resolve_field(&mut self, record_def: Option<DefId>, name: &ast::Ident) -> Res {
        let Some(record_def) = record_def else { return Res::Error };
        let field = self
            .defs
            .children(record_def)
            .find(|child| child.kind == DefKind::Field && child.name == name.name)
            .map(|child| child.id);
        match field {
            Some(def) => Res::Def(def),
            None => {
                let owner = self.defs.path_of(record_def);
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
                    if self.defs.get(id).kind == DefKind::Record {
                        if elems.is_empty() {
                            return hir::Pattern {
                                kind: hir::PatternKind::Struct { res, fields: Vec::new() },
                                span: pattern.span,
                            };
                        }
                        self.error_with_def(
                            codes::CONSTRUCTION_MISMATCH,
                            format!(
                                "`{}` is a record type, so its pattern names its fields",
                                path.dotted()
                            ),
                            pattern.span,
                            "positional elements here",
                            id,
                            "this record type is declared here",
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
                let record_def =
                    self.expect_record(res, path, "a named-field pattern matches a record");
                hir::PatternKind::Struct {
                    res,
                    fields: fields
                        .iter()
                        .map(|field| hir::FieldPattern {
                            field: self.resolve_field(record_def, &field.name),
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

/// The type an implementation is for, once borrows are peeled off.
fn type_owner(ty: &hir::Type) -> Option<DefId> {
    match &ty.kind {
        hir::TypeKind::Path { res, .. } => res.def_id(),
        hir::TypeKind::Borrowed { inner, .. } => type_owner(inner),
        _ => None,
    }
}
