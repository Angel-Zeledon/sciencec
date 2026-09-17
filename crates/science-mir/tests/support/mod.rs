//! The whole pipeline, down to MIR.
//!
//! `science-types`'s own harness gives the argument for going through the real
//! lexer, parser and resolver rather than building a tree by hand: *"the
//! contract of the checking layer is 'what the author wrote checks', and the
//! only way to have what the author wrote is to run"* them. The contract of
//! this layer is *"what the checker produced lowers"*, so this harness is that
//! one with two more calls on the end.
//!
//! A hand-built [`science_types::thir::Body`] would be worse here than a
//! hand-built `hir::Expr` was there, because the thing under test is precisely
//! the shape the checker happens to produce — where `Coerce` nodes land, where
//! `auto_borrow` inserted a `Borrow`, whether a method resolved.

#![allow(dead_code)]

use science_diagnostics::{Diagnostics, FileId};
use science_mir::mir::{Body, Callee, Rvalue, StatementKind, TerminatorKind};
use science_mir::{lower_crate, CallGraph, Context};
use science_resolve::hir::{self, DefId, DefKind};
use science_types::items::Declarations;
use science_types::thir;
use science_types::{check_crate, Aliases, AtomOrder, Types};

/// One program, lexed, parsed, resolved, checked, and lowered.
pub struct Lowered {
    pub krate: hir::Crate,
    pub types: Types,
    pub decls: Declarations,
    pub thir: Vec<thir::Body>,
    pub bodies: Vec<Body>,
    pub diagnostics: Diagnostics,
    pub resolution: Diagnostics,
}

/// Lowers a program that must lex, parse and resolve cleanly.
///
/// The checker is *not* required to be silent: the holes `check`'s §6 names —
/// method calls, `for`, operators on user types — produce diagnostics on
/// ordinary programs, and a harness that demanded silence could only test the
/// fragment those holes do not touch. What must be clean is resolution, for the
/// reason that crate's harness gives: a `Res::Error` in a fixture makes every
/// assertion pass for the wrong reason.
pub fn lower(source: &str) -> Lowered {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(!lexed.has_errors(), "the fixture must lex: {:?}", codes(&lexed));
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(!parsed.has_errors(), "the fixture must parse: {:?}", codes(&parsed));
    let (krate, resolution) = science_resolve::resolve_module(file, "mir.science", &ast);
    assert!(!resolution.has_errors(), "the fixture must resolve: {:?}", codes(&resolution));

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
    let decls = Declarations::of(&krate, &mut types, &order, &mut diagnostics);
    let thir = check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut diagnostics);

    let bodies = {
        let mut context = Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        lower_crate(&mut context, &thir)
    };
    Lowered { krate, types, decls, thir, bodies, diagnostics, resolution }
}

/// Lowers a program only if it lexes, parses and resolves cleanly.
///
/// The corpus is not guaranteed to *check*, only to reach this crate, and a
/// file that does not resolve is one this crate never sees. Two corpus-wide
/// tests need the same filter — `no_invented_loops.rs` and `captures.rs` —
/// so it lives here rather than in whichever of them was written first.
pub fn lower_if_clean(source: &str) -> Option<Lowered> {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    if lexed.has_errors() {
        return None;
    }
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    if parsed.has_errors() {
        return None;
    }
    let (_, resolution) = science_resolve::resolve_module(file, "example.science", &ast);
    if resolution.has_errors() {
        return None;
    }
    Some(lower(source))
}

/// Every `.science` file in `examples/`, as `(name, source)`.
pub fn corpus() -> Vec<(String, String)> {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&examples).expect("examples/") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("science") {
            continue;
        }
        let name = path.file_name().expect("a file name").to_string_lossy().into_owned();
        out.push((name, std::fs::read_to_string(&path).expect("a readable example")));
    }
    out.sort();
    out
}

fn codes(diagnostics: &Diagnostics) -> Vec<(u16, String)> {
    diagnostics.iter().map(|d| (d.code.0, d.message.clone())).collect()
}

impl Lowered {
    /// The MIR of the named function.
    pub fn body(&self, name: &str) -> &Body {
        self.bodies
            .iter()
            .find(|body| self.krate.defs.get(body.def()).name == name)
            .unwrap_or_else(|| panic!("the fixture lowers no body for `{name}`"))
    }

    /// The named function's MIR, rendered. [`science_mir::dump`] says what it
    /// prints and what it deliberately does not.
    pub fn dump(&self, name: &str) -> String {
        science_mir::dump::body(&self.krate.defs, self.body(name))
    }

    pub fn def(&self, name: &str, kind: DefKind) -> DefId {
        self.krate
            .defs
            .iter()
            .find(|def| def.name == name && def.kind == kind)
            .unwrap_or_else(|| panic!("the fixture declares no {kind:?} named `{name}`"))
            .id
    }

    pub fn call_graph(&self) -> CallGraph {
        CallGraph::of(&self.bodies)
    }

    /// Every statement in a body, as the dump would print it.
    pub fn statements(&self, name: &str) -> Vec<String> {
        let body = self.body(name);
        let mut out = Vec::new();
        for (_, block) in body.blocks() {
            for statement in &block.statements {
                out.push(describe_statement(&statement.kind));
            }
        }
        out
    }

    /// Every terminator in a body, by kind name.
    pub fn terminators(&self, name: &str) -> Vec<&'static str> {
        self.body(name)
            .blocks()
            .map(|(_, block)| describe_terminator(&block.terminator.kind))
            .collect()
    }

    /// Every unresolved callee in a body, in block order.
    pub fn unresolved(&self, name: &str) -> Vec<science_mir::Unresolved> {
        self.body(name)
            .blocks()
            .filter_map(|(_, block)| match &block.terminator.kind {
                TerminatorKind::Call { callee: Callee::Unresolved(which), .. } => Some(*which),
                _ => None,
            })
            .collect()
    }
}

pub fn describe_statement(kind: &StatementKind) -> String {
    match kind {
        StatementKind::Assign { rvalue, .. } => format!("assign {}", describe_rvalue(rvalue)),
        StatementKind::StorageLive(_) => "storage-live".to_string(),
        StatementKind::StorageDead(_) => "storage-dead".to_string(),
        StatementKind::SetDropFlag { value, .. } => format!("set-drop-flag {value}"),
        StatementKind::Activate(_) => "activate".to_string(),
        StatementKind::Nop => "nop".to_string(),
    }
}

pub fn describe_rvalue(rvalue: &Rvalue) -> &'static str {
    match rvalue {
        Rvalue::Use(_) => "use",
        Rvalue::Ref { .. } => "ref",
        Rvalue::Unary { .. } => "unary",
        Rvalue::Binary { .. } => "binary",
        Rvalue::Cast { .. } => "cast",
        Rvalue::Record { .. } => "record",
        Rvalue::Variant { .. } => "variant",
        Rvalue::Tuple(_) => "tuple",
        Rvalue::Range { .. } => "range",
        Rvalue::IsPresent(_) => "present",
        Rvalue::Discriminant(_) => "discriminant",
        Rvalue::Coerce { .. } => "coerce",
        Rvalue::Narrow { .. } => "narrow",
        Rvalue::Closure { .. } => "closure",
        Rvalue::Error => "error",
    }
}

pub fn describe_terminator(kind: &TerminatorKind) -> &'static str {
    match kind {
        TerminatorKind::Goto { .. } => "goto",
        TerminatorKind::If { .. } => "if",
        TerminatorKind::Switch { .. } => "switch",
        TerminatorKind::Call { .. } => "call",
        TerminatorKind::Drop { .. } => "drop",
        TerminatorKind::Return => "return",
        TerminatorKind::Unreachable => "unreachable",
    }
}
