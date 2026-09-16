//! The def table is the spine of the HIR: every name in a resolved tree is a
//! `DefId` into it, so these tests pin down what a `DefId` promises.

use link_diagnostics::{FileId, Span};
use link_resolve::hir::{DefKind, DefTable};

fn span(start: u32, end: u32) -> Span {
    Span::new(FileId(0), start, end)
}

#[test]
fn every_definition_gets_a_distinct_id() {
    let mut defs = DefTable::new();
    let root = defs.alloc(DefKind::Module, "root", span(0, 0), None);
    let a = defs.alloc(DefKind::Fn, "a", span(0, 5), Some(root));
    let b = defs.alloc(DefKind::Fn, "b", span(6, 11), Some(root));

    assert_ne!(a, b);
    assert_eq!(defs[a].name, "a");
    assert_eq!(defs[b].kind, DefKind::Fn);
    assert_eq!(defs[b].span, span(6, 11));
    assert_eq!(defs[a].parent, Some(root));
    assert_eq!(defs.len(), 3);
}

#[test]
fn module_of_walks_up_to_the_nearest_module() {
    let mut defs = DefTable::new();
    let root = defs.alloc(DefKind::Module, "root", span(0, 0), None);
    let text = defs.alloc(DefKind::Module, "text", span(0, 0), Some(root));
    let doc = defs.alloc(DefKind::Struct, "Doc", span(0, 10), Some(text));
    let title = defs.alloc(DefKind::Field, "title", span(4, 9), Some(doc));

    assert_eq!(defs.module_of(title), Some(text));
    assert_eq!(defs.module_of(doc), Some(text));
    // A module is its own module: an impl inside it compares equal.
    assert_eq!(defs.module_of(text), Some(text));
}

#[test]
fn path_of_prints_the_chain_without_the_root() {
    let mut defs = DefTable::new();
    let root = defs.alloc(DefKind::Module, "", span(0, 0), None);
    let text = defs.alloc(DefKind::Module, "text", span(0, 0), Some(root));
    let parser = defs.alloc(DefKind::Module, "parser", span(0, 0), Some(text));
    let token = defs.alloc(DefKind::Enum, "Token", span(0, 10), Some(parser));

    assert_eq!(defs.path_of(token), "text.parser.Token");
    assert_eq!(defs.path_of(text), "text");
}

#[test]
fn a_builtin_definition_is_marked_as_having_no_source() {
    let mut defs = DefTable::new();
    let id = defs.alloc(DefKind::Primitive, "String", link_resolve::hir::BUILTIN_SPAN, None);
    assert!(defs[id].is_builtin());
}
