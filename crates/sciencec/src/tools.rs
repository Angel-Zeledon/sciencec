//! `sciencec tools --json` — a JSON Schema for every function in a file.
//!
//! `mcp-servers.md` §14.3 calls this *"the whole thesis, minus the server"*
//! and it is the first thing that note asks anyone to build. The thesis is
//! Decision 3: **the schema is the signature.** Every other MCP SDK makes an
//! author write a handler and a JSON Schema and keep them in step by hand;
//! Python's reads `inspect.signature` at run time and TypeScript's takes a
//! schema handed to it beside the handler. Science has neither reflection nor
//! a way to check that two artefacts agree, so the schema has to be *derived*
//! or the claim is empty.
//!
//! This is the derivation. It settles the claim before a keyword is spent on
//! it, which is the whole reason it is worth building alone.
//!
//! # What it walks, and why not `tool`
//!
//! §14.2 stages `tool` as a keyword first and this pass second. **It is built
//! the other way round here, deliberately.** The pass is the part that can be
//! wrong: if the type mapping has holes, they are holes whatever declares the
//! function, and finding them costs nothing today. A keyword is a language
//! commitment, and `def-and-lambda.md` is a whole note about how expensive one
//! is to take back. §14.3's own fourth reason for building this first says the
//! pass survives the declaration form changing — *"the same pass over Option
//! B's record types produces the same JSON"* — so building the reversible half
//! first is what that reason implies.
//!
//! So it walks **every function declared at the top level of the file**. The
//! file is the tool list. When `tool` lands, the walk changes by one predicate
//! and nothing else.
//!
//! # What it needs that does not exist
//!
//! No type checker. §5.2 of the core spec makes every signature fully
//! annotated, so the parameter types are written down and resolution has
//! already said what each name points at. That is the entire reason this can
//! be built now: the mapping is a walk over resolved declarations, not an
//! inference.
//!
//! What that costs is stated where it bites, in [`schema_of`]: a type this
//! pass cannot map is reported as a gap rather than an error, because deciding
//! that a type is *wrong* rather than *unmapped* needs the checker. §14.2
//! stages `SC0504`–`SC0518` as errors at stage 2 for exactly this reason.

use std::collections::HashMap;

use science_resolve::hir::{
    Choice, Crate, DefId, DefKind, Item, ItemKind, Module, Record, Res, Type, TypeKind,
};

/// What the walk needs that `Crate` does not index.
///
/// Resolution hands back definitions by id and item bodies in source order,
/// with no map between them, because nothing before this needed one: the
/// dumper walks items and the resolver walks scopes. A schema recurses *into*
/// a named type, so it needs the other direction.
struct Index<'a> {
    krate: &'a Crate,
    records: HashMap<DefId, &'a Record>,
    choices: HashMap<DefId, &'a Choice>,
}

impl<'a> Index<'a> {
    fn of(krate: &'a Crate, module: &'a Module) -> Self {
        let mut records = HashMap::new();
        let mut choices = HashMap::new();
        for item in &module.items {
            match &item.kind {
                ItemKind::Record(r) => {
                    records.insert(r.def, r);
                }
                ItemKind::Choice(c) => {
                    choices.insert(c.def, c);
                }
                _ => {}
            }
        }
        Index { krate, records, choices }
    }
}

/// The JSON Schema a Science type maps to, or why it does not map.
///
/// `Err` carries prose rather than a diagnostic code because none of
/// `SC0504`–`SC0518` can be *reported* yet: they are type errors and there is
/// no type checker. Naming the type and the reason is what this phase can
/// honestly do.
type Mapped = Result<String, String>;

/// Renders the `tools/list` array for one resolved module.
pub fn render(krate: &Crate, module: &Module) -> String {
    let index = Index::of(krate, module);
    let mut out = String::from("[");
    let mut first = true;
    for item in &module.items {
        let Some(tool) = tool_of(&index, item) else { continue };
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(&tool);
    }
    out.push(']');
    out
}

/// One entry, or `None` when the item is not a function.
fn tool_of(index: &Index, item: &Item) -> Option<String> {
    let ItemKind::Fn(decl) = &item.kind else { return None };
    // A method has a receiver and belongs to a type; only free functions are
    // callable by name from outside the program.
    if decl.self_param.is_some() {
        return None;
    }
    let name = index.krate.defs.get(decl.def).name.clone();

    let mut properties = String::new();
    let mut required = String::new();
    let mut gaps: Vec<String> = Vec::new();

    for param in &decl.params {
        let pname = index.krate.defs.get(param.def).name.clone();
        if !properties.is_empty() {
            properties.push(',');
        }
        match schema_of(index, &param.ty) {
            Ok(schema) => {
                properties.push_str(&format!("{}:{}", quoted(&pname), schema));
                // §4.4: a `T?` parameter is the same schema as `T` with the
                // name absent from `required`. Absence and null are different
                // states in JSON and the same state in Science, and the note
                // says so rather than hiding it.
                if !matches!(param.ty.kind, TypeKind::Nullable(_)) {
                    if !required.is_empty() {
                        required.push(',');
                    }
                    required.push_str(&quoted(&pname));
                }
            }
            Err(why) => {
                properties.push_str(&format!("{}:{{}}", quoted(&pname)));
                gaps.push(format!("{pname}: {why}"));
            }
        }
    }

    let mut entry = format!(
        "{{{}:{},{}:{{\"type\":\"object\",\"properties\":{{{}}},\"required\":[{}]}}",
        quoted("name"),
        quoted(&name),
        quoted("inputSchema"),
        properties,
        required
    );

    // §5: the description is what a model reads to decide whether to call the
    // tool. It is program data, not documentation, and a tool without one is
    // `SC0190` once `tool` exists. Here its absence is visible instead.
    if let Some(doc) = &item.doc {
        entry.push(',');
        entry.push_str(&format!("{}:{}", quoted("description"), quoted(doc)));
    }
    if !gaps.is_empty() {
        entry.push(',');
        entry.push_str(&format!("{}:[{}]", quoted("x-science-unmapped"),
            gaps.iter().map(|g| quoted(g)).collect::<Vec<_>>().join(",")));
    }
    entry.push('}');
    Some(entry)
}

/// The type mapping of §4.1.
///
/// Every row of that table is here, and every rejection of §4.2 comes back as
/// an `Err` naming the reason rather than a code. The bounds on the integers
/// are the point of the whole exercise: `U8` emitting `minimum:0,maximum:255`
/// is something no other SDK's `int` can say, and it costs nothing because the
/// width was already in the type.
fn schema_of(index: &Index, ty: &Type) -> Mapped {
    match &ty.kind {
        TypeKind::Path { res, generics } => path_schema(index, *res, generics),
        // §4.4. A nullable parameter is its payload's schema; the nullability
        // is carried by `required`, not by the schema, because JSON has no
        // "present but null" that means what `T?` means.
        TypeKind::Nullable(inner) => schema_of(index, inner),
        // A borrow is a Science calling convention and is invisible on a wire.
        TypeKind::Borrowed { inner, .. } => schema_of(index, inner),
        TypeKind::Tuple(_) => Err("a tuple has no JSON spelling; use a record type".into()),
        TypeKind::Unit => Err("`()` carries nothing and cannot be a parameter".into()),
        TypeKind::Any(_) => {
            Err("a trait object is dispatched at run time and has no schema".into())
        }
        TypeKind::SelfType(_) | TypeKind::SelfAssoc { .. } => {
            Err("`Self` depends on the implementation and is not known here".into())
        }
        _ => Err("no mapping".into()),
    }
}

/// A named type, once resolution has said what the name points at.
fn path_schema(index: &Index, res: Res, generics: &[Type]) -> Mapped {
    let Some(id) = res.def_id() else {
        return Err("the name did not resolve".into());
    };
    let def = index.krate.defs.get(id);

    // The scalars are matched by name *and* by coming from the prelude, so a
    // user type called `Int` is not silently given `Int`'s schema. Without a
    // type checker this is the strongest identity check available, and it is
    // exact for everything the prelude declares.
    if def.is_builtin() {
        return match def.name.as_str() {
            "Bool" => Ok(r#"{"type":"boolean"}"#.into()),
            "String" => Ok(r#"{"type":"string"}"#.into()),
            "I8" => Ok(integer(Some(-128), Some(127))),
            "I16" => Ok(integer(Some(-32_768), Some(32_767))),
            "I32" => Ok(integer(Some(-2_147_483_648), Some(2_147_483_647))),
            // `Int` is `I64` (§5.1 of the core spec).
            "I64" | "Int" => Ok(integer(Some(i64::MIN as i128), Some(i64::MAX as i128))),
            "U8" => Ok(integer(Some(0), Some(255))),
            "U16" => Ok(integer(Some(0), Some(65_535))),
            "U32" => Ok(integer(Some(0), Some(4_294_967_295))),
            // §4.3: no `maximum`. 2^64-1 is not exactly representable as a
            // JSON number, so emitting it would be a lie about what the other
            // side can send.
            "U64" => Ok(integer(Some(0), None)),
            // §4.3 again: JSON numbers are not floats. NaN and the infinities
            // cannot be written in JSON at all, so a tool cannot receive one,
            // and no bound expresses that.
            "F32" | "F64" => Ok(r#"{"type":"number"}"#.into()),
            "Array" => match generics.first() {
                Some(item) => Ok(format!(r#"{{"type":"array","items":{}}}"#, schema_of(index, item)?)),
                None => Err("`Array` with no element type".into()),
            },
            "Map" => map_schema(index, generics),
            other => Err(format!("`{other}` has no JSON spelling")),
        };
    }

    match def.kind {
        DefKind::Record => record_schema(index, id),
        DefKind::Choice => choice_schema(index, id),
        DefKind::TypeParam => {
            Err("a generic parameter has no schema; a tool is monomorphic".into())
        }
        DefKind::Interface => {
            Err("an interface names a set of types, not one; no schema".into())
        }
        _ => Err(format!("`{}` is not a type that crosses", def.name)),
    }
}

/// `Map of (String, V)`. §4.1: string keys only, because a JSON object's keys
/// are strings and nothing else.
fn map_schema(index: &Index, generics: &[Type]) -> Mapped {
    let (Some(key), Some(value)) = (generics.first(), generics.get(1)) else {
        return Err("`Map` with fewer than two arguments".into());
    };
    let key_is_string = matches!(&key.kind, TypeKind::Path { res, .. }
        if res.def_id().is_some_and(|id| {
            let d = index.krate.defs.get(id);
            d.is_builtin() && d.name == "String"
        }));
    if !key_is_string {
        return Err("a JSON object's keys are strings; `Map` needs a `String` key".into());
    }
    Ok(format!(r#"{{"type":"object","additionalProperties":{}}}"#, schema_of(index, value)?))
}

/// A record type whose fields all cross becomes an object. Recurses.
fn record_schema(index: &Index, id: DefId) -> Mapped {
    let Some(record) = index.records.get(&id) else {
        return Err("the record's definition is not in this file".into());
    };
    let mut properties = String::new();
    let mut required = String::new();
    for field in &record.fields {
        let name = &index.krate.defs.get(field.def).name;
        if !properties.is_empty() {
            properties.push(',');
        }
        properties.push_str(&format!("{}:{}", quoted(name), schema_of(index, &field.ty)?));
        if !matches!(field.ty.kind, TypeKind::Nullable(_)) {
            if !required.is_empty() {
                required.push(',');
            }
            required.push_str(&quoted(name));
        }
    }
    Ok(format!(
        r#"{{"type":"object","properties":{{{properties}}},"required":[{required}]}}"#
    ))
}

/// §4.5, which `mcp-servers.md` calls the largest single win in the table: a
/// `choice` all of whose variants carry nothing is a string `enum`.
///
/// It is a win because it costs the author nothing. In every other SDK an
/// enumerated parameter is a list of strings written beside the handler and
/// kept in step by hand; here it is how Science already spells alternatives,
/// and the model is told the exact set.
fn choice_schema(index: &Index, id: DefId) -> Mapped {
    let Some(choice) = index.choices.get(&id) else {
        return Err("the choice's definition is not in this file".into());
    };
    let mut names = Vec::new();
    for variant in &choice.variants {
        if !variant.payload.is_empty() {
            return Err(format!(
                "`{}` has a variant carrying a payload; only unit variants become an enum",
                index.krate.defs.get(id).name
            ));
        }
        names.push(quoted(&index.krate.defs.get(variant.def).name));
    }
    Ok(format!(r#"{{"type":"string","enum":[{}]}}"#, names.join(",")))
}

fn integer(min: Option<i128>, max: Option<i128>) -> String {
    let mut s = String::from(r#"{"type":"integer""#);
    if let Some(min) = min {
        s.push_str(&format!(r#","minimum":{min}"#));
    }
    if let Some(max) = max {
        s.push_str(&format!(r#","maximum":{max}"#));
    }
    s.push('}');
    s
}

/// A JSON string literal.
///
/// Written here rather than taken from a library for the reason the driver
/// refused an argument parser: the workspace has one third-party dependency
/// and this is twenty lines. It escapes what RFC 8259 requires and nothing
/// else — Science source is UTF-8 and so is JSON, so no character needs a
/// `\u` escape except the control codes.
fn quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
