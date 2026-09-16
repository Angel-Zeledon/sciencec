//! The normal form as the monomorphisation key — §10.1 item 4.
//!
//! > Two instantiations that are `EQUAL` must produce one symbol, or `a * b`
//! > and `b * a` link to two copies of the same function.
//!
//! Nothing calls [`MonoKey`] yet — there is no monomorphiser — so these tests
//! are the consumer it will have, written the way it will use it: put a key in
//! a map, look it up with a differently-spelled instantiation, and expect one
//! entry.

mod common;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use common::Scope;
use science_types::{normalise, ConstExpr, MonoKey, NormalForm};

fn key(exprs: &[ConstExpr]) -> MonoKey {
    MonoKey::of(exprs).expect("these arguments have normal forms")
}

fn hash_of(key: &MonoKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn two_spellings_of_one_instantiation_are_one_key() {
    // `type-checking-and-mir.md` §8 item 4, with `+` in place of the `*` it
    // writes: §2.1's grammar cannot multiply two parameters, so the
    // commutativity that matters for an extent is addition's.
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let written_one_way = key(&[scope.add(scope.param_expr(a), scope.param_expr(b))]);
    let written_the_other = key(&[scope.add(scope.param_expr(b), scope.param_expr(a))]);

    assert_eq!(written_one_way, written_the_other);
    assert_eq!(hash_of(&written_one_way), hash_of(&written_the_other));
}

#[test]
fn one_map_entry_for_two_spellings() {
    // The failure this type exists to prevent, stated as the monomorphiser
    // will meet it: two symbols emitted for one function.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let mut symbols: HashMap<MonoKey, &str> = HashMap::new();
    symbols.insert(key(&[scope.add(scope.param_expr(n), scope.lit(1))]), "matrix$1");

    let looked_up = symbols.get(&key(&[scope.add(scope.lit(1), scope.param_expr(n))]));
    assert_eq!(looked_up, Some(&"matrix$1"));
    assert_eq!(symbols.len(), 1);
}

#[test]
fn provenance_does_not_reach_the_key() {
    // Support 1 of `mono.rs`' four: `Term`'s `Hash` and `Eq` are written by
    // hand over the coefficient and the atom, so where a dimension was
    // written is not part of the symbol it produces. A derived `PartialEq`
    // here would make one library two.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let here = key(&[scope.scale(scope.param_expr(n), 3)]);
    let there = key(&[scope.scale(scope.param_expr(n), 3)]);

    assert_eq!(here, there);
    assert_eq!(hash_of(&here), hash_of(&there));
}

#[test]
fn a_cancelled_term_does_not_reach_the_key() {
    // Support 2: `N - N + 3` and `3` are one instantiation.
    let mut scope = Scope::new();
    let n = scope.param("N");

    let cancelled =
        key(&[scope.add(scope.sub(scope.param_expr(n), scope.param_expr(n)), scope.lit(3))]);
    let plain = key(&[scope.lit(3)]);

    assert_eq!(cancelled, plain);
    assert_eq!(cancelled.to_string(), "3");
}

#[test]
fn argument_position_is_part_of_the_key() {
    // `Matrix of (T, 2, 3)` and `Matrix of (T, 3, 2)` are two types, so the
    // key is a list and not a set.
    let scope = Scope::new();
    let rows_then_cols = key(&[scope.lit(2), scope.lit(3)]);
    let cols_then_rows = key(&[scope.lit(3), scope.lit(2)]);

    assert_ne!(rows_then_cols, cols_then_rows);
    assert_eq!(rows_then_cols.to_string(), "2;3");
}

#[test]
fn distinct_instantiations_have_distinct_keys() {
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let keys = [
        key(&[scope.param_expr(a)]),
        key(&[scope.param_expr(b)]),
        key(&[scope.scale(scope.param_expr(a), 2)]),
        key(&[scope.add(scope.param_expr(a), scope.lit(1))]),
        key(&[scope.neg(scope.param_expr(a))]),
        key(&[scope.lit(0)]),
        key(&[]),
    ];

    for (i, left) in keys.iter().enumerate() {
        for (j, right) in keys.iter().enumerate() {
            if i != j {
                assert_ne!(left, right, "keys {i} and {j} collided");
                assert_ne!(
                    left.to_string(),
                    right.to_string(),
                    "serialisations {i} and {j} collided"
                );
            }
        }
    }
}

#[test]
fn the_serialisation_names_atoms_by_identity_and_not_by_name() {
    // Two parameters called `N` in two scopes must not serialise the same,
    // for the reason `ConstExprKind::Param` holds a `DefId`.
    let mut scope = Scope::new();
    let first = scope.param("N");
    let second = scope.param("N");

    let left = key(&[scope.param_expr(first)]);
    let right = key(&[scope.param_expr(second)]);

    assert_ne!(left.to_string(), right.to_string());
    assert_eq!(left.to_string(), format!("0+1*#{}", first.index()));
}

#[test]
fn the_serialisation_is_in_atom_order() {
    // §3.5 item 4: "the key is the normal form, serialised in atom order".
    let mut scope = Scope::new();
    let (a, b) = (scope.param("a"), scope.param("b"));

    let written_backwards = key(&[scope.sub(scope.param_expr(b), scope.param_expr(a))]);
    assert_eq!(
        written_backwards.to_string(),
        format!("0-1*#{}+1*#{}", a.index(), b.index()),
        "`a` sorts before `b` however the expression was written"
    );
}

#[test]
fn a_key_built_from_forms_matches_one_built_from_expressions() {
    let mut scope = Scope::new();
    let n = scope.param("N");
    let expr = scope.add(scope.param_expr(n), scope.lit(4));

    let from_exprs = key(std::slice::from_ref(&expr));
    let from_forms: MonoKey =
        MonoKey::new(std::iter::once(normalise(&expr).expect("normalises") as NormalForm));

    assert_eq!(from_exprs, from_forms);
}
