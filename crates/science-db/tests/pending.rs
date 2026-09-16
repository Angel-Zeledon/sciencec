//! The phases that do not exist yet are declared as real salsa queries, not as
//! comments describing queries. These tests check that: the keys can be built
//! and the functions can be called, so the crate that implements a phase
//! replaces a body rather than designing an interface.

use science_db::pending::{self, CodegenUnit, DefId, ModuleId};
use science_db::ScienceDatabase;

#[test]
fn the_placeholder_keys_are_usable() {
    let mut db = ScienceDatabase::new();
    let file = db.add_file("a.science", "def a():\n    1\n");

    let module = ModuleId::new(&db, file);
    assert_eq!(module.file(&db), file);

    // Interning is what gives these ids their meaning: the same definition
    // named twice must be the same key, or the phase behind it runs twice.
    let def = DefId::new(&db, module, 0);
    assert_eq!(DefId::new(&db, module, 0), def);
    assert_ne!(DefId::new(&db, module, 1), def);
    assert_eq!(def.index(&db), 0);

    assert_eq!(CodegenUnit::new(&db, 0), CodegenUnit::new(&db, 0));
}

#[test]
#[should_panic(expected = "science-resolve")]
fn hir_is_declared_and_unimplemented() {
    let mut db = ScienceDatabase::new();
    let file = db.add_file("a.science", "def a():\n    1\n");
    let _ = pending::hir(&db, ModuleId::new(&db, file));
}

#[test]
#[should_panic(expected = "science-regions")]
fn region_result_is_declared_and_unimplemented() {
    let mut db = ScienceDatabase::new();
    let file = db.add_file("a.science", "def a():\n    1\n");
    let module = ModuleId::new(&db, file);
    let _ = pending::region_result(&db, DefId::new(&db, module, 0));
}

#[test]
#[should_panic(expected = "science-codegen")]
fn mono_items_is_declared_and_unimplemented() {
    let db = ScienceDatabase::new();
    let _ = pending::mono_items(&db);
}
