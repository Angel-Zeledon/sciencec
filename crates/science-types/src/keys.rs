//! `SC0548`, a `Map` or `Set` keyed by a type that cannot be hashed.
//!
//! # 1. The decision
//!
//! **A key is hashed and compared structurally, and the front end decides
//! which types that is defined for.** A key may be:
//!
//! - an integer of any width, `Bool`, `Char` or `String`;
//! - a tuple, a record or a `choice` whose every part is a key, and a `T?`
//!   whose `T` is one.
//!
//! Its hash is a fold over those parts and its equality their conjunction —
//! `science-codegen-llvm`'s `Lowerer::intern_key_glue` emits both, the way it
//! emits drop glue — so a pad byte between two fields is never read, and two
//! equal values built through different paths always meet in one bucket.
//!
//! # 2. What is refused, and why each one
//!
//! | Key | Why |
//! |---|---|
//! | a float, at any width | `NaN is NaN` is false and `0.0 is -0.0` is true, so `is` on a float is not an equivalence and no hash can agree with it |
//! | a type that writes `implements Eq:` by hand | `is` on it is that `eq`, and a hash read off the fields agrees with an `eq` only when the `eq` reads the fields the same way |
//! | `Array`, `Map`, `Set`, `Box`, `any I`, a borrow, a closure | a container is mutable or shared behind a pointer, and a key that changes after it is filed is lost |
//!
//! **The float row is `collections-and-chains.md` §5.1's reasoning one
//! interface over.** That note keeps `F32` out of `Ord` because its order is
//! not total; equality fails the same way at `NaN`, and a table needs `eq` to
//! be reflexive — a `NaN` key inserted twice would be two entries, and found
//! by neither probe. A program that means a float key means a bucketed one,
//! `(x * 1000.0).round() as Int`, and writing the bucket is the decision a
//! table cannot make for it.
//!
//! **The `Eq` row is a refusal where a derivation would be a guess.** The
//! alternative — use the hand-written `eq` and a structural hash — is wrong
//! the moment the `eq` ignores a field or folds case, and it is wrong silently:
//! two values `is` calls equal land in two buckets. Hand-written hashing needs
//! `collections-and-chains.md` §5.2's AMENDMENT 8 — a prelude `Hash` and a
//! `Hasher` to feed — which this compiler does not have yet; until it does,
//! such a type is refused here, by name, with the reason.
//!
//! # 3. Where it is checked, and the cost
//!
//! **Over every expression's type, after the writeback, once per key type per
//! body.** `Map.insert`'s `where K: Eq + Hash` cannot be written — `Hash` is
//! not a prelude interface — so there is no bound to fail; and the key of
//! `Map.new()` is often fixed only by a later `insert`, so a check at the call
//! would see an inference variable. Every map in a program is the type of
//! some expression in the body that builds it, and the writeback is when that
//! type is final.
//!
//! **What it cannot see is a generic body's key**: `def count[K](xs:
//! &Array[K]) -> Map[K, Int]` is checked with `K` open and answers nothing. A
//! call that instantiates it with `F64` reaches the backend, which refuses it
//! by name as `SC0400` — the same hole every bound on a type parameter has
//! here, and it closes with `Hash` as a bound.

use std::collections::HashSet;

use science_diagnostics::{Diagnostic, Diagnostics, Label, Span};
use science_resolve::hir::{self, DefId, DefKind};

use crate::alias::Aliases;
use crate::codes;
use crate::items::Declarations;
use crate::subst::Substitution;
use crate::thir::Body;
use crate::ty::{GenericArg, Ty, TyKind, Types};

/// Past this the walk stops answering rather than recursing: a key type this
/// deep is not one a program writes, and a type that contains itself without
/// a `Box` is refused long before here.
const MAX_DEPTH: u32 = 32;

/// The integer, `Bool`, `Char` and `String` prelude names: the leaves a key
/// is built from.
const LEAVES: &[&str] = &[
    "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "Int", "Bool", "Char", "String",
];

/// The prelude's floats.
const FLOATS: &[&str] = &["F16", "BF16", "F32", "F64", "Float"];

/// Reports every `Map` and `Set` in one body whose key cannot be hashed.
pub fn report(
    body: &Body,
    krate: &hir::Crate,
    decls: &Declarations,
    types: &mut Types,
    aliases: &mut Aliases,
    diagnostics: &mut Diagnostics,
) {
    let prelude = decls.prelude();
    let (Some(map), Some(set)) = (prelude.get("Map"), prelude.get("Set")) else { return };
    let mut keys = Keys { krate, decls, types, aliases, map, set, eq: prelude.get("Eq") };
    // One report per key type: the first expression, in source order, whose
    // type mentions it.
    let mut found: Vec<(DefId, Ty, Span)> = Vec::new();
    for (_, expr) in body.exprs() {
        let mut containers = Vec::new();
        keys.containers(expr.ty, 0, &mut containers);
        for (container, key) in containers {
            match found.iter_mut().find(|(c, k, _)| *c == container && *k == key) {
                Some(entry) if expr.span.start < entry.2.start => entry.2 = expr.span,
                Some(_) => {}
                None => found.push((container, key, expr.span)),
            }
        }
    }
    let mut reported = HashSet::new();
    for (container, key, span) in found {
        if !reported.insert(key) {
            continue;
        }
        let Some(why) = keys.unhashable(key, 0) else { continue };
        let container = &krate.defs.get(container).name;
        let rendered = keys.types.render(&krate.defs, key);
        diagnostics.push(unhashable_key(span, container, &rendered, &why));
    }
}

/// Why a key type cannot be hashed, as the sentence the note ends with and the
/// path of fields that led to it.
struct Why {
    /// `` `Point`'s field `x` is `F64` `` and so on, outermost first.
    path: Vec<String>,
    /// The reason at the bottom of the path.
    reason: String,
}

struct Keys<'a> {
    krate: &'a hir::Crate,
    decls: &'a Declarations,
    types: &'a mut Types,
    aliases: &'a mut Aliases,
    map: DefId,
    set: DefId,
    eq: Option<DefId>,
}

impl Keys<'_> {
    /// Every `Map` or `Set` inside `ty`, as `(container, key)`.
    fn containers(&mut self, ty: Ty, depth: u32, out: &mut Vec<(DefId, Ty)>) {
        if depth > MAX_DEPTH || self.types.references_error(ty) {
            return;
        }
        match self.types.kind(ty).clone() {
            TyKind::Named { def, args } => {
                if def == self.map || def == self.set {
                    if let Some(GenericArg::Type(key)) = args.first() {
                        out.push((def, *key));
                    }
                }
                for arg in args {
                    if let GenericArg::Type(inner) = arg {
                        self.containers(inner, depth + 1, out);
                    }
                }
            }
            TyKind::Borrowed { inner, .. } | TyKind::Nullable(inner) => {
                self.containers(inner, depth + 1, out)
            }
            TyKind::Tuple(elements) => {
                for element in elements {
                    self.containers(element, depth + 1, out);
                }
            }
            _ => {}
        }
    }

    /// `None` when `ty` is a key — or when this cannot tell, which is a type
    /// parameter, `Self` or an erroneous type, and silence is the answer for
    /// all three.
    fn unhashable(&mut self, ty: Ty, depth: u32) -> Option<Why> {
        if depth > MAX_DEPTH || self.types.references_error(ty) {
            return None;
        }
        let ty = self.aliases.reveal(self.types, ty).ok()?;
        let rendered = self.types.render(&self.krate.defs, ty);
        let refuse = |reason: String| Some(Why { path: Vec::new(), reason });
        match self.types.kind(ty).clone() {
            TyKind::Tuple(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    if let Some(mut why) = self.unhashable(*element, depth + 1) {
                        let part = self.types.render(&self.krate.defs, *element);
                        why.path.insert(0, format!("element {index} of `{rendered}` is `{part}`"));
                        return Some(why);
                    }
                }
                None
            }
            TyKind::Nullable(inner) => self.unhashable(inner, depth + 1),
            TyKind::Named { def, args } => self.named(ty, def, &args, &rendered, depth),
            TyKind::Param { .. }
            | TyKind::SelfType { .. }
            | TyKind::SelfAssoc { .. }
            | TyKind::Error => None,
            TyKind::Borrowed { .. } => refuse(format!(
                "`{rendered}` is a borrow, and a key is owned by the table it is filed in"
            )),
            TyKind::Object { .. } => refuse(format!(
                "`{rendered}` is an interface object, whose equality is its concrete type's and \
                 is not known here"
            )),
            TyKind::Closure { .. } => {
                refuse(format!("`{rendered}` is a function, which has no equality"))
            }
            TyKind::Unit => refuse("`()` has one value and is no use as a key".to_string()),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    fn named(
        &mut self,
        ty: Ty,
        def: DefId,
        args: &[GenericArg],
        rendered: &str,
        depth: u32,
    ) -> Option<Why> {
        let refuse = |reason: String| Some(Why { path: Vec::new(), reason });
        let declared = self.krate.defs.get(def);
        if declared.is_builtin() {
            let name = declared.name.as_str();
            if LEAVES.contains(&name) {
                return None;
            }
            if FLOATS.contains(&name) {
                return refuse(format!(
                    "`{rendered}` is a float, and `NaN is NaN` is false while `0.0 is -0.0` is \
                     true, so `is` on a float is not an equivalence a table can file keys by"
                ));
            }
            return refuse(format!(
                "`{rendered}` is a container or a handle, and a key that can change after it is \
                 filed is a key the table loses"
            ));
        }
        if matches!(declared.kind, DefKind::Record | DefKind::Choice) {
            if let Some(eq) = self.eq {
                if self.decls.methods().declares(self.types, ty, eq) {
                    return refuse(format!(
                        "`{rendered}` implements `Eq` by hand, so `is` on it is that `eq`, and a \
                         hash read off its fields would put two values your `eq` calls equal in \
                         two different buckets; a hand-written `Hash` is not supported yet"
                    ));
                }
            }
        }
        match declared.kind {
            DefKind::Record => {
                let record = self.decls.record(def)?.clone();
                let subst = Substitution::of_generics(&record.generics, args);
                for (field, field_ty) in &record.fields {
                    let field_ty = subst.apply(self.types, *field_ty).ok()?;
                    if let Some(mut why) = self.unhashable(field_ty, depth + 1) {
                        let field = &self.krate.defs.get(*field).name;
                        let part = self.types.render(&self.krate.defs, field_ty);
                        why.path.insert(0, format!("`{rendered}`'s field `{field}` is `{part}`"));
                        return Some(why);
                    }
                }
                None
            }
            DefKind::Choice => {
                let variants: Vec<DefId> = self
                    .krate
                    .defs
                    .children(def)
                    .filter(|child| child.kind == DefKind::Variant)
                    .map(|child| child.id)
                    .collect();
                for variant in variants {
                    let Some(declared) = self.decls.variant(variant).cloned() else { continue };
                    let subst = Substitution::of_generics(&declared.generics, args);
                    for payload in &declared.payload {
                        let payload = subst.apply(self.types, *payload).ok()?;
                        if let Some(mut why) = self.unhashable(payload, depth + 1) {
                            let name = &self.krate.defs.get(variant).name;
                            let part = self.types.render(&self.krate.defs, payload);
                            let step = format!("`{rendered}`'s variant `{name}` holds `{part}`");
                            why.path.insert(0, step);
                            return Some(why);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// `SC0548` — a `Map` or `Set` whose key cannot be hashed.
fn unhashable_key(span: Span, container: &str, key: &str, why: &Why) -> Diagnostic {
    let mut note = why.path.join(", and ");
    if !note.is_empty() {
        note.push_str(": ");
    }
    note.push_str(&why.reason);
    let (what, holds) = match container {
        "Set" => ("an element", "holds"),
        _ => ("a key", "is keyed by"),
    };
    Diagnostic::error(
        codes::UNHASHABLE_KEY,
        format!("`{key}` cannot be {what} of a `{container}`"),
    )
    .with_label(Label::primary(span, format!("this `{container}` {holds} `{key}`")))
    .with_note(note)
    .with_note(
        "a key is an integer, `Bool`, `Char` or `String`, or a tuple, record, `choice` or `T?` \
         built only from those; it is hashed and compared field by field",
    )
}
