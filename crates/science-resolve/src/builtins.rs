//! The prelude: the names §5.1 and §8 say exist before any file is read.
//!
//! Without this, `def f(a: borrowed String)` reports an unresolved name, and every
//! later phase would have to special-case a handful of strings — exactly the
//! string lookup the HIR exists to abolish. So the primitives, the library
//! types, their variants, the compiler-known traits and the free functions all
//! get real `DefId`s, in a module of their own that no file can name.
//!
//! Methods used to be out of scope here, on the grounds that *"a method is
//! reached through a receiver whose type this phase does not know … the type
//! checker owns them, and it can hang them off these ids"*. The first half is
//! still true and the second half is what §"The declared surface" below does:
//! this file now allocates the ids **and** emits an ordinary HIR tree for the
//! declarations that hang off them, so that `science-types` lowers the prelude
//! through the same [`Declarations`] walk it lowers a user's `Doc has:`
//! through. Nothing here has a body; see that section for what that costs.
//!
//! Builtin definitions carry [`BUILTIN_SPAN`](crate::hir::BUILTIN_SPAN): they
//! have no source, and §9 has no room for a node without a span.
//!
//! [`Declarations`]: https://docs.rs/science-types

use std::collections::HashMap;

use science_parser::ast::{Ident, SelfKind};

use crate::hir::{self, DefId, DefKind, DefTable, Res, BUILTIN_SPAN};

/// Everything the prelude defines, ready to be merged into a module scope.
pub struct Prelude {
    /// The module the definitions hang from. It is not a child of the crate
    /// root, so no `use` can reach it and no user implementation can claim to own it —
    /// which is what makes `String implements Clone:` an orphan (§5.4).
    pub module: DefId,
    /// Name to definition, for everything but variants.
    pub names: Vec<(String, DefId)>,
    /// Unqualified variants of a choice type: `None` as well as `Option.None`
    /// (§4.5).
    pub variants: Vec<(String, DefId)>,
    /// How many payload types each variant carries, which is what decides
    /// whether a bare name in a pattern is a match or a binding (§4.4).
    pub variant_arity: Vec<(DefId, usize)>,
    /// Nested modules of the prelude and the names in each: today just `ffi`.
    /// The caller gives each one a scope of its own, which is what makes
    /// `ffi.Span` a two-segment path rather than a special case.
    pub modules: Vec<(DefId, Vec<(String, DefId)>)>,
    /// The declared surface as an HIR tree — interfaces, implementation blocks
    /// and free functions, every one of them body-less.
    ///
    /// These are **not** put in a [`hir::Module`] and not appended to
    /// `Crate::modules`: a file is a module (§4.4), the prelude is not a file,
    /// and every walk over `modules` — the dump, the driver's *"the first
    /// module is the one the user named"*, the body checker — would have to
    /// learn to skip it. `Crate::prelude` is the one extra field instead, and
    /// the two consumers that want it say so.
    pub items: Vec<hir::Item>,
}

/// §5.1, plus the aliases. `Never` is §4.5.
const PRIMITIVES: &[&str] = &[
    "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "F16", "BF16", "F32", "F64", "Bool",
    "Char", "String", "Int", "Float", "Never",
];

/// The C scalar vocabulary of `ffi-c-boundary.md` §1.3, usable inside an
/// `extern` block. They are deliberately *not* aliases of the Science
/// primitives: §1.6's whole argument is that a width the author did not think
/// about is how a numerical program gets silently wrong answers, so `CLong`
/// stays a type whose width the author has to consider.
const C_SCALARS: &[&str] = &[
    "CChar", "CInt", "CUInt", "CLong", "CULong", "CLongLong", "CULongLong", "CFloat", "CDouble",
    "CSizeT", "CPtrDiff", "CVoid",
];

/// Library types with a representation in `science-rt` but no declaration in any
/// `.science` file (§8).
///
/// `TextError` joins them because `stdlib-core.md` §7.2 puts exactly three
/// error types at Level 1 — `Error`, `IoError`, `TextError` — and because
/// `String.parse_int` and `String.parse_float` cannot be declared without it.
/// Its variants are *not* declared: §7.4 gives four of them, and a `choice`
/// whose variants are in the prelude puts four more names in every program's
/// scope (§4.5 makes a variant reachable unqualified), which is a namespace
/// decision this pass has no reason to take on the way past.
const LIBRARY_TYPES: &[&str] = &["Array", "Map", "Box", "Chars", "IoError", "TextError"];

/// The interfaces the compiler knows about (§5.4).
/// §5.4 lists seventeen, and the operator ones are load-bearing: "a scientific
/// language in which `+` does not work on your own type is not a scientific
/// language". Leaving them out made `Vector2 implements Add:` unresolvable in
/// three corpus files, which nothing caught until the driver ran resolution
/// over `examples/` for the first time.
const INTERFACES: &[&str] = &[
    "Add", "Sub", "Mul", "Div", "Rem", "Pow", "MatMul", "Neg", "Index", "Eq", "Ord", "Copy",
    "Clone", "Drop", "Iterate", "From", "Display",
    // `Error`, the one-method interface of revision 2 §3.4. It is in the
    // prelude and not in a module because `-> (T, Error?)` is the signature of
    // every fallible function in the language, and a name that common cannot
    // need an import. `Error?` is shorthand for `(any Error)?`; the expansion
    // is in `resolve_nullable_inner`, not here.
    "Error",
];

/// The free functions (§8).
///
/// Five, not `stdlib-core.md` §9's thirteen. The eight this list does not have
/// — `print_error`, `write_error`, `flush`, `read_line`, `read_bytes`,
/// `write_bytes`, `read_lines`, `write_lines` — are names in every program's
/// scope forever (§1.3), and §12 of that note names *"thirteen free functions,
/// and the trend is upward"* as its own second risk. Adding eight global names
/// is a spec change under §2.2's rule and is not a side effect of giving the
/// five that exist a signature.
const FUNCTIONS: &[&str] = &["print", "write", "panic", "read_file", "write_file"];

/// `ffi`, the closed vocabulary of `ffi-c-boundary.md` §1.3.
///
/// A module rather than a scatter of prelude names, because `ffi.Span` is how
/// every example in that note writes it and because a name as short as `Span`
/// in the top-level prelude would collide with the first user who wanted one.
///
/// `Complex32` and `Complex64` are here although §1.3's own table omits them:
/// §2.4 of that note already *uses* `ffi.Complex64`, `c-binding-coverage.md`
/// §4.1 measures 930 complex LAPACK and CBLAS routines behind the pair, and a
/// vocabulary that a sibling section uses and the table does not define is a
/// hole rather than a decision.
///
/// The whole module is an ask, not a fact: §10.3 of the note requests it as a
/// change to §8's closed library and says so rather than assuming it. It is
/// built here because an `extern` block whose parameter types do not resolve
/// teaches nothing, and because the set is closed — adding to it is a spec
/// change, which is exactly what a `const` list in one file makes visible.
const FFI_TYPES: &[&str] = &[
    // Pointers and views (§1.3). `Span` and `MutableSpan` carry a length that
    // does not cross the ABI; `Pointer` is nullable and of unknown validity;
    // `OpaqueHandle` is non-null and never dereferenced.
    "Span",
    "MutableSpan",
    "Pointer",
    "DevicePointer",
    "OpaqueHandle",
    "FunctionPointer",
    // Strings (§1.3). Neither converts to `String` without a copy.
    "CString",
    "CStr",
    // The out-parameter idiom (§1.3).
    "Uninitialized",
    // `c-binding-coverage.md` §4.1(1): half of LAPACK.
    "Complex32",
    "Complex64",
];

/// The marker interface of §1.4: fields in declaration order at the offsets
/// the platform C ABI gives them.
const FFI_INTERFACES: &[&str] = &["CLayout"];

/// `(choice type name, [(variant name, payload arity)])`.
///
/// Empty since revision 2 §3. `Option of T` became `T?` and `Result of (T, E)`
/// became the pair `-> (T, E?)`, which took `Some`, `None`, `Ok` and `Err`
/// with them. The table stays because the prelude will have a choice type
/// again and the machinery below is the part worth keeping; an empty table is
/// also the honest record that these four names are *free*, not merely unused.
const CHOICES: &[(&str, &[(&str, usize)])] = &[];

// --- the declared surface -------------------------------------------------
//
// `stdlib-core.md`, made readable by the checker. Everything below is a
// *declaration*: no method has a body, nothing here is codegen's to emit, and
// none of it is a standard library. What it buys is that the four phases that
// each documented a conservatism about this hole — method lookup, the bound
// check, instance selection, and `science-regions`' assumption about an
// unresolved callee — can stop working around it.

/// A type, written the way the notes write it.
///
/// **A vocabulary rather than a parser.** The alternative was a `.science`
/// prelude compiled at build time, which reads better and puts a source file, a
/// parser invocation and a bootstrapping order between the compiler and its own
/// prelude. This is the same information as data; the cost is that the surface
/// syntax in the notes has to be transcribed by hand, and the check on the
/// transcription is that a signature naming something that does not exist is a
/// panic in [`build`] rather than a silent hole.
#[derive(Debug, Clone, Copy)]
enum Ty {
    /// A prelude type with no arguments: `Int`, `Bool`, `String`.
    Name(&'static str),
    /// A prelude type applied to arguments: `Array of T`.
    App(&'static str, &'static [Ty]),
    /// A generic parameter of the enclosing block, by name.
    Var(&'static str),
    /// `Self.Item` (§5.4).
    Assoc(&'static str),
    /// `borrowed T`.
    Ref(&'static Ty),
    /// `T?` (revision 2 §3.1).
    Opt(&'static Ty),
    /// A pair, which is what `-> (T, Error?)` is.
    Pair(&'static Ty, &'static Ty),
}

const INT: Ty = Ty::Name("Int");
const BOOL: Ty = Ty::Name("Bool");
const STRING: Ty = Ty::Name("String");
const CHAR: Ty = Ty::Name("Char");
const I64: Ty = Ty::Name("I64");
const F64: Ty = Ty::Name("F64");
const IO_ERROR: Ty = Ty::Name("IoError");

/// One declared function: a free function, a method, or an interface's
/// required method.
#[derive(Debug, Clone, Copy)]
struct Method {
    name: &'static str,
    /// `None` is an associated function — `Array.new()` — which takes no
    /// receiver at all. `science-types`' `Form` is computed from exactly this.
    recv: Option<SelfKind>,
    params: &'static [(&'static str, Ty)],
    /// `None` is `()`, which is how the HIR spells a `def` with no `->`.
    ret: Option<Ty>,
}

/// `Array of T has:`, or `String implements Clone:`.
#[derive(Debug, Clone, Copy)]
struct Block {
    ty: &'static str,
    /// The block's own generic parameters — the `T` of `Array of T has:`.
    generics: &'static [&'static str],
    /// `Some` for `implements`, `None` for the inherent `has:`.
    interface: Option<&'static str>,
    /// `type Item is Char` — this block's side of §5.4.
    assoc: &'static [(&'static str, Ty)],
    methods: &'static [Method],
}

/// `interface Error:` with what it declares.
#[derive(Debug, Clone, Copy)]
struct InterfaceDecl {
    name: &'static str,
    /// `type Item`, declared and unanswered.
    assoc: &'static [&'static str],
    methods: &'static [Method],
}

/// The interfaces that get a method, and why only two do.
///
/// **Decision. An interface is declared with its methods only where a note
/// gives the method's name and its types.** `Error.message` is
/// `stdlib-core.md` §7.5 verbatim; `Iterate.next` is the one
/// `collections-and-chains.md` builds its chain vocabulary on. The other
/// fifteen — `Add`, `Ord`, `Eq`, `Display`, `Index` and the rest — are declared
/// as **names with implementations and no methods**, which is the whole of what
/// the bound check needs: `methods`' §7 asks *"does `I64` implement `Ord`"* and
/// never *"what is `Ord`'s method called"*.
///
/// **The cost, stated: operator dispatch does not close.** `check`'s §6 wants
/// *"a rule anywhere saying which method name each operator dispatches to"*,
/// and writing `Ord.compare -> Ordering` or `Display.display(Formatter)` here
/// would invent `Ordering` and `Formatter` as well — two Level 1 types no note
/// has specified, arriving as a side effect of a bound check. A signature
/// invented in passing is how a language acquires a design nobody argued for,
/// so the fifteen stay methodless and `check`'s §6 keeps its hole with a
/// narrower reason: the implementations are there now, and only the method
/// names are missing.
const INTERFACE_DECLS: &[InterfaceDecl] = &[
    InterfaceDecl {
        name: "Error",
        assoc: &[],
        // §7.5's `cause` is *defaulted* — it has a body — and a body is the one
        // thing a declaration cannot carry. Declaring it here would turn a
        // defaulted method into a required one, so it is left out.
        methods: &[Method {
            name: "message",
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(STRING),
        }],
    },
    InterfaceDecl {
        name: "Iterate",
        assoc: &["Item"],
        methods: &[Method {
            name: "next",
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&Ty::Assoc("Item"))),
        }],
    },
];

/// The interfaces every numeric primitive implements.
///
/// `Pow`, `MatMul` and `Index` are deliberately absent: `**` on a scalar is not
/// settled anywhere this file can cite, a matrix product on a scalar is not a
/// thing, and `Int` is not indexable.
const NUMERIC: &[&str] =
    &["Add", "Sub", "Mul", "Div", "Rem", "Neg", "Eq", "Ord", "Copy", "Clone", "Display"];

/// Every prelude type's interfaces, as `(type, interfaces)`.
///
/// **Decision. These are implementations with no methods in them.** The
/// relation `T implements I` is what a bound is checked against; the method
/// bodies are `science-rt`'s and the method *names* are unsettled (see
/// [`INTERFACE_DECLS`]). Declaring the relation without the names is the half
/// that is knowable, and it is the half four of the five conservatisms wanted.
///
/// **`String implements Clone, Eq, Ord, Add, Display`** is `stdlib-core.md`
/// §6.9 exactly, minus `Inspect` and `Hash`, which the prelude does not name as
/// interfaces at all. §6.2 makes `String: Clone` the retirement of a recorded
/// corpus wart and §6.3 makes `String: Add` concatenation; both are read out of
/// the note rather than decided here.
///
/// **What is not here is every generic type.** `Array of T implements Clone`
/// holds only where `T: Clone`, and a conditional implementation is not
/// something this index can express — `methods`' §4 says in as many words that
/// a blanket implementation *"is not looked through"*. Declaring it
/// unconditionally would admit `Array of Doc: Clone` for a `Doc` that is not
/// clonable; declaring it not at all leaves the bound unanswerable, which is
/// what `Methods::answers_for` now says out loud rather than guessing.
const IMPLEMENTS: &[(&str, &[&str])] = &[
    ("I8", NUMERIC),
    ("I16", NUMERIC),
    ("I32", NUMERIC),
    ("I64", NUMERIC),
    ("U8", NUMERIC),
    ("U16", NUMERIC),
    ("U32", NUMERIC),
    ("U64", NUMERIC),
    ("F16", NUMERIC),
    ("BF16", NUMERIC),
    ("F32", NUMERIC),
    ("F64", NUMERIC),
    ("Int", NUMERIC),
    ("Float", NUMERIC),
    ("Bool", &["Eq", "Copy", "Clone", "Display"]),
    ("Char", &["Eq", "Ord", "Copy", "Clone", "Display"]),
    // §6.9. `Copy` is *not* among them and that is the point of §6.2: an owned
    // copy of a `String` allocates, so it is a `clone` and never a move-free
    // duplication.
    ("String", &["Clone", "Eq", "Ord", "Add", "Display"]),
    // §7.2's three Level 1 error types. `Error` on a concrete error is what
    // lets it stand in an `Error?` slot, which `assign`'s §3 asks about by
    // name.
    ("IoError", &["Error", "Display", "Eq", "Clone"]),
    ("TextError", &["Error", "Display", "Eq", "Clone"]),
];

/// The prelude's blocks with methods in them.
///
/// **Every one of these is `open`**: the set of methods declared on a prelude
/// type is smaller than the set that exists, so a name this table does not have
/// is *"not written down yet"* and never *"no such method"*. `Methods` reads
/// that off `Def::is_builtin` rather than off a flag here, because it is true of
/// every builtin head and will stay true until the whole of §9 is transcribed.
const BLOCKS: &[Block] = &[
    // --- Array, §3.6 ------------------------------------------------------
    Block {
        ty: "Array",
        generics: &["T"],
        interface: None,
        assoc: &[],
        methods: &[
            Method {
                name: "new",
                recv: None,
                params: &[],
                ret: Some(Ty::App("Array", &[Ty::Var("T")])),
            },
            // §3.6 verbatim.
            Method {
                name: "push",
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Var("T"))],
                ret: None,
            },
            // §3.6 verbatim, parentheses and all: `borrowed T?` would read as
            // `borrowed (T?)`.
            Method {
                name: "get",
                recv: Some(SelfKind::Shared),
                params: &[("index", INT)],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("T")))),
            },
            // **`pop` is deliberately absent.** §3.2 names it — *"`Array` has
            // `push` and `pop`"* — and gives it no signature, and the two
            // candidates differ in what a program may write:
            // `pop(mutable self) -> T?` hands the element back, and
            // `pop(mutable self)` makes *"take it out and use it"* two calls.
            // `examples/21` writes `self.ribs.pop()` as a statement in a `-> ()`
            // body, which only the second admits. Deciding it here would settle
            // it by accident, in a file nobody reads, on the evidence of one
            // call site; `pop` closes none of the conservatisms this pass is
            // for, so it waits for the note that owns it.
            // `collections-and-chains.md` §3.1: *"`length()` on a collection is
            // O(1), counting a stream consumes it"*. `Int` and not `U64`
            // because §4.6 of the core spec writes `text.length()` into an
            // integer context and Decision 2 defaults an integer literal to
            // `I64`; a `U64` length would make `length() - 1` a mixed-signedness
            // expression in the most-written loop in the language.
            Method { name: "length", recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
        ],
    },
    // --- Map, §3.6 --------------------------------------------------------
    Block {
        ty: "Map",
        generics: &["K", "V"],
        interface: None,
        assoc: &[],
        methods: &[
            Method {
                name: "new",
                recv: None,
                params: &[],
                ret: Some(Ty::App("Map", &[Ty::Var("K"), Ty::Var("V")])),
            },
            // §3.6, minus its `where K: Eq + Hash`. `Hash` is not one of the
            // prelude's interfaces, so writing the bound would name something
            // that does not resolve; and a bound that cannot be checked at a
            // call is a bound that costs a diagnostic and buys nothing. Named
            // in the report as the one clause of §3.6 dropped.
            Method {
                name: "insert",
                recv: Some(SelfKind::Mutable),
                params: &[("key", Ty::Var("K")), ("value", Ty::Var("V"))],
                ret: Some(Ty::Opt(&Ty::Var("V"))),
            },
            // **Decided here, not read out of a note.** §3.6 gives `Array.get`
            // and not `Map.get`; `examples/09` writes *"`Map.get` hands back
            // `(borrowed V)?` for the same reason"* and `science-regions`'
            // `generate` §7 says *"the real `Map.get` borrows only the map"*.
            // Two decisions are being taken:
            //
            // 1. **The value comes back borrowed, not owned.** `-> V?` would
            //    force a copy of every value read out of a map, which for a
            //    `Map of (String, Array of F64)` is the whole array. The cost
            //    is that a caller who wants an owned value writes `.clone()`,
            //    and that `Map` cannot later be given a `get` that returns by
            //    value under the same name.
            // 2. **The key is taken by borrow.** `key: K` would move the key
            //    into the call, so `settings.get(key)` inside a loop would
            //    consume `key` on the first iteration. The cost is that a
            //    caller holding an owned key relies on §6.3's auto-borrow, and
            //    that a `Map of (Int, _)` borrows a number to look it up.
            Method {
                name: "get",
                recv: Some(SelfKind::Shared),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("V")))),
            },
            // §3.4: membership is `contains` on both `Map` and `Set`, because
            // `has` is a keyword and *"between two names that read equally
            // well, the one that compiles wins"*.
            Method {
                name: "contains",
                recv: Some(SelfKind::Shared),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(BOOL),
            },
            // Decided: `remove` hands back what it displaced, matching
            // `insert`. The alternative — `-> ()` — makes *"take it out and use
            // it"* two lookups.
            Method {
                name: "remove",
                recv: Some(SelfKind::Mutable),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(Ty::Opt(&Ty::Var("V"))),
            },
            Method { name: "length", recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
        ],
    },
    // --- Box --------------------------------------------------------------
    Block {
        ty: "Box",
        generics: &["T"],
        interface: None,
        assoc: &[],
        methods: &[Method {
            name: "new",
            recv: None,
            params: &[("value", Ty::Var("T"))],
            ret: Some(Ty::App("Box", &[Ty::Var("T")])),
        }],
    },
    // --- String, §6.9 -----------------------------------------------------
    //
    // Thirteen of the note's nineteen. The six left out are named in the
    // report: `truncate` contradicts eighteen corpus call sites, and
    // `from_bytes`, `slice`, `bytes`, `lines` and `split` return `Range`,
    // `Characters`, `Lines` and `Split` — four Level 1 types §9 lists and the
    // prelude does not have.
    Block {
        ty: "String",
        generics: &[],
        interface: None,
        assoc: &[],
        methods: &[
            Method { name: "new", recv: None, params: &[], ret: Some(STRING) },
            // §6.5: **bytes**, and the note states the cost — `"héllo".length()`
            // is 6 — as documentation debt with no compiler mitigation.
            Method { name: "length", recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
            Method {
                name: "push_str",
                recv: Some(SelfKind::Mutable),
                params: &[("tail", Ty::Ref(&STRING))],
                ret: None,
            },
            Method {
                name: "starts_with",
                recv: Some(SelfKind::Shared),
                params: &[("prefix", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "ends_with",
                recv: Some(SelfKind::Shared),
                params: &[("suffix", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "contains",
                recv: Some(SelfKind::Shared),
                params: &[("needle", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "find",
                recv: Some(SelfKind::Shared),
                params: &[("needle", Ty::Ref(&STRING))],
                ret: Some(Ty::Opt(&INT)),
            },
            // §6.9: ASCII whitespace only, pinned forever.
            Method {
                name: "trim",
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Ref(&STRING)),
            },
            Method {
                name: "replace",
                recv: Some(SelfKind::Shared),
                params: &[("from", Ty::Ref(&STRING)), ("to", Ty::Ref(&STRING))],
                ret: Some(STRING),
            },
            // **Decided: `chars`, not §6.9's `characters`.** The prelude
            // already carries a type named `Chars`, so the compiler took this
            // side before the note was written, and `examples/10` walks
            // `text.chars()`. Renaming both would be a rename of a Level 1 type
            // and a Level 1 method at once, which is a spec change (§2.2) and
            // not a transcription. The cost is that the note and the compiler
            // disagree about the name until somebody reconciles them, and the
            // report says so.
            Method {
                name: "chars",
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Name("Chars")),
            },
            // §6.9: Level 1 because *"correctly-rounded decimal parsing has a
            // unique answer"* and because `read_lines` is useless without them.
            Method {
                name: "parse_int",
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Pair(&I64, &Ty::Opt(&Ty::Name("TextError")))),
            },
            Method {
                name: "parse_float",
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Pair(&F64, &Ty::Opt(&Ty::Name("TextError")))),
            },
        ],
    },
    // --- Chars ------------------------------------------------------------
    //
    // The one iterator §8 hands out by name. `Item is Char` is what
    // `examples/10`'s `for c in text.chars()` reads its binding out of.
    Block {
        ty: "Chars",
        generics: &[],
        interface: Some("Iterate"),
        assoc: &[("Item", CHAR)],
        methods: &[],
    },
];

/// The free functions' signatures.
///
/// **`read_file` takes a `borrowed String`, and `stdlib-core.md` §5.2 says it
/// should take a `borrowed Path`.** The deviation is deliberate and it is
/// costed: `Path` is not in the prelude, §5.2's own mitigation is `SC0257` —
/// *"a `String` was passed where a `Path` is expected"*, with an applicable fix
/// — and that diagnostic is unwritten. Declaring `borrowed Path` today would
/// turn every `read_file("a.txt")` in the corpus into a bare type mismatch with
/// no help text, which is the worse half of the note's own argument. The
/// report names it as the deviation that should be closed by whoever lands
/// `Path`.
///
/// **`print` and `write` are deliberately left undeclared**, and this is the
/// one place a declaration was written, measured and withdrawn.
/// `strings-formatting-and-docs.md` §4.1 gives
/// `def print(value: borrowed any Display)`; declaring it reports on **seven**
/// corpus programs that are correct, from three separate causes, none of which
/// is the signature:
///
/// 1. **An integer literal is not defaulted against an `any I` expectation.**
///    `print(separated)` where `separated` is bound to a literal is
///    `expected any Display, found an integer literal` — Decision 2's default
///    never runs, because the expectation is an interface object rather than a
///    numeric type. Five of the seven.
/// 2. **A borrow of a branch-local temporary dies at the branch.**
///    `print(if flag: "yes" else: "no")` takes §6.3's auto-borrow of a literal
///    whose storage ends inside the arm, and `SC0333` is then correct about
///    the MIR and wrong about the program. Two of the seven.
/// 3. **A corpus type that does not implement `Display` is printed.** One,
///    and that one is a true positive that this pass declines to deliver on
///    its own, because it would arrive mixed in with the six above.
///
/// Causes 1 and 2 are the checker's and the lowering's, they are cheap to
/// state and expensive to fix from here, and `print` is not what any of the
/// five conservatisms turned on. So the hole stays, with a measurement
/// attached instead of a guess.
const FUNCTION_SIGNATURES: &[Method] = &[
    // `-> Never` is the half of `SC0140`'s fourth exclusion that is a property
    // of the declaration, and `items`' `Signature::can_return` is already
    // written against it.
    Method {
        name: "panic",
        recv: None,
        params: &[("message", Ty::Ref(&STRING))],
        ret: Some(Ty::Name("Never")),
    },
    // §7.3: a Level 1 function returns its **concrete** error type, never
    // `Error?` — the interface form allocates, and the concrete form keeps a
    // caller's `match` exhaustive.
    Method {
        name: "read_file",
        recv: None,
        params: &[("path", Ty::Ref(&STRING))],
        ret: Some(Ty::Pair(&STRING, &Ty::Opt(&IO_ERROR))),
    },
    Method {
        name: "write_file",
        recv: None,
        params: &[("path", Ty::Ref(&STRING)), ("text", Ty::Ref(&STRING))],
        ret: Some(Ty::Opt(&IO_ERROR)),
    },
];

/// Builds the HIR for the declared surface, given the names already allocated.
struct Declarer<'a> {
    defs: &'a mut DefTable,
    module: DefId,
    names: HashMap<String, DefId>,
    items: Vec<hir::Item>,
}

/// What a type in one declaration may refer to: the block's generic parameters
/// and its associated types.
///
/// `Self` is not among them, and that is a decision: every declaration below
/// writes its own type out — `Array.new() -> Array of T`, not `-> Self`. The
/// two are the same type after `check`'s `block_substitution` runs, and the
/// written-out form is the one a reader of this file can check against the note
/// it came from without holding a substitution in their head.
struct Scope<'a> {
    generics: &'a HashMap<&'static str, DefId>,
    assocs: &'a HashMap<&'static str, DefId>,
}

impl Declarer<'_> {
    fn named(&self, name: &str) -> DefId {
        *self
            .names
            .get(name)
            .unwrap_or_else(|| panic!("the prelude declares `{name}` before it is used"))
    }

    fn item(&mut self, kind: hir::ItemKind) {
        self.items.push(hir::Item { kind, span: BUILTIN_SPAN, doc: None });
    }

    fn ty(&self, ty: &Ty, scope: &Scope<'_>) -> hir::Type {
        let kind = match ty {
            Ty::Name(name) => {
                hir::TypeKind::Path { res: Res::Def(self.named(name)), generics: Vec::new() }
            }
            Ty::App(name, args) => hir::TypeKind::Path {
                res: Res::Def(self.named(name)),
                generics: args.iter().map(|arg| self.ty(arg, scope)).collect(),
            },
            Ty::Var(name) => {
                let def = *scope
                    .generics
                    .get(name)
                    .unwrap_or_else(|| panic!("`{name}` is not a parameter of this block"));
                hir::TypeKind::Path { res: Res::Def(def), generics: Vec::new() }
            }
            Ty::Assoc(name) => {
                let def = *scope
                    .assocs
                    .get(name)
                    .unwrap_or_else(|| panic!("`Self.{name}` is not declared on this block"));
                hir::TypeKind::SelfAssoc {
                    res: Res::Def(def),
                    name: Ident::new(*name, BUILTIN_SPAN),
                }
            }
            Ty::Ref(inner) => {
                hir::TypeKind::Borrowed { mutable: false, inner: Box::new(self.ty(inner, scope)) }
            }
            Ty::Opt(inner) => hir::TypeKind::Nullable(Box::new(self.ty(inner, scope))),
            Ty::Pair(left, right) => {
                hir::TypeKind::Tuple(vec![self.ty(left, scope), self.ty(right, scope)])
            }
        };
        hir::Type { kind, span: BUILTIN_SPAN }
    }

    fn bound(&self, interface: &str) -> hir::Bound {
        hir::Bound {
            kind: hir::BoundKind::Interface {
                res: Res::Def(self.named(interface)),
                generics: Vec::new(),
            },
            span: BUILTIN_SPAN,
        }
    }

    fn function(&mut self, method: &Method, parent: DefId, scope: &Scope<'_>) -> hir::Fn {
        let def = self.defs.alloc(DefKind::Fn, method.name, BUILTIN_SPAN, Some(parent));
        let self_param = method.recv.map(|kind| hir::SelfParam {
            def: self.defs.alloc(DefKind::SelfParam, "self", BUILTIN_SPAN, Some(def)),
            kind,
            span: BUILTIN_SPAN,
        });
        let params = method
            .params
            .iter()
            .map(|(name, ty)| hir::Param {
                def: self.defs.alloc(DefKind::Param, *name, BUILTIN_SPAN, Some(def)),
                ty: self.ty(ty, scope),
                span: BUILTIN_SPAN,
            })
            .collect();
        hir::Fn {
            def,
            generics: Vec::new(),
            self_param,
            params,
            ret: method.ret.as_ref().map(|ty| self.ty(ty, scope)),
            where_clause: Vec::new(),
            body: None,
            span: BUILTIN_SPAN,
        }
    }

    fn interface(&mut self, decl: &InterfaceDecl) {
        let def = self.named(decl.name);
        let mut assocs = HashMap::new();
        let assoc_types: Vec<hir::AssocType> = decl
            .assoc
            .iter()
            .map(|name| {
                let id = self.defs.alloc(DefKind::AssocType, *name, BUILTIN_SPAN, Some(def));
                assocs.insert(*name, id);
                hir::AssocType { def: id, ty: None, span: BUILTIN_SPAN }
            })
            .collect();
        let generics = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs };
        let methods: Vec<hir::Fn> =
            decl.methods.iter().map(|method| self.function(method, def, &scope)).collect();
        self.item(hir::ItemKind::Interface(hir::Interface {
            def,
            generics: Vec::new(),
            supers: Vec::new(),
            where_clause: Vec::new(),
            assoc_types,
            methods,
            span: BUILTIN_SPAN,
        }));
    }

    fn block(&mut self, block: &Block) {
        // The block's own definition hangs off the prelude module, not off the
        // type: `DefTable::module_of` walks parents to find the module the
        // orphan rule asks about, and `resolve`'s `is_prelude` already treats
        // that module as one no user file can implement into.
        let def = self.defs.alloc(DefKind::Impl, "", BUILTIN_SPAN, Some(self.module));
        let mut generics = HashMap::new();
        let generic_params: Vec<hir::GenericParam> = block
            .generics
            .iter()
            .map(|name| {
                let id = self.defs.alloc(DefKind::TypeParam, *name, BUILTIN_SPAN, Some(def));
                generics.insert(*name, id);
                hir::GenericParam {
                    def: id,
                    kind: hir::GenericParamKind::Type { bounds: Vec::new() },
                    span: BUILTIN_SPAN,
                }
            })
            .collect();

        let self_ty = {
            let head = self.named(block.ty);
            let args = block
                .generics
                .iter()
                .map(|name| hir::Type {
                    kind: hir::TypeKind::Path {
                        res: Res::Def(generics[name]),
                        generics: Vec::new(),
                    },
                    span: BUILTIN_SPAN,
                })
                .collect();
            hir::Type {
                kind: hir::TypeKind::Path { res: Res::Def(head), generics: args },
                span: BUILTIN_SPAN,
            }
        };

        let mut assocs = HashMap::new();
        let mut assoc_types = Vec::new();
        for (name, _) in block.assoc {
            let id = self.defs.alloc(DefKind::AssocType, *name, BUILTIN_SPAN, Some(def));
            assocs.insert(*name, id);
        }
        {
            let scope = Scope { generics: &generics, assocs: &assocs };
            for (name, ty) in block.assoc {
                assoc_types.push(hir::AssocType {
                    def: assocs[name],
                    ty: Some(self.ty(ty, &scope)),
                    span: BUILTIN_SPAN,
                });
            }
        }

        let scope = Scope { generics: &generics, assocs: &assocs };
        let methods: Vec<hir::Fn> =
            block.methods.iter().map(|method| self.function(method, def, &scope)).collect();
        let interface = block.interface.map(|name| self.bound(name));
        self.item(hir::ItemKind::Impl(hir::Impl {
            def,
            generics: generic_params,
            interface,
            self_ty,
            where_clause: Vec::new(),
            assoc_types,
            methods,
            span: BUILTIN_SPAN,
        }));
    }

    /// `T implements I:` with nothing in it.
    fn relation(&mut self, ty: &'static str, interface: &'static str) {
        self.block(&Block {
            ty,
            generics: &[],
            interface: Some(interface),
            assoc: &[],
            methods: &[],
        });
    }

    /// A free function's declaration, at the `DefId` [`FUNCTIONS`] allocated.
    fn free(&mut self, method: &Method) {
        let def = self.named(method.name);
        let generics = HashMap::new();
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs };
        let params = method
            .params
            .iter()
            .map(|(name, ty)| hir::Param {
                def: self.defs.alloc(DefKind::Param, *name, BUILTIN_SPAN, Some(def)),
                ty: self.ty(ty, &scope),
                span: BUILTIN_SPAN,
            })
            .collect();
        let ret = method.ret.as_ref().map(|ty| self.ty(ty, &scope));
        self.item(hir::ItemKind::Fn(hir::Fn {
            def,
            generics: Vec::new(),
            self_param: None,
            params,
            ret,
            where_clause: Vec::new(),
            body: None,
            span: BUILTIN_SPAN,
        }));
    }
}

/// Allocates the prelude into `defs`.
pub fn build(defs: &mut DefTable) -> Prelude {
    let module = defs.alloc(DefKind::Module, "core", BUILTIN_SPAN, None);
    let mut prelude = Prelude {
        module,
        names: Vec::new(),
        variants: Vec::new(),
        variant_arity: Vec::new(),
        modules: Vec::new(),
        items: Vec::new(),
    };

    let declare = |defs: &mut DefTable, kind: DefKind, name: &str, out: &mut Prelude| {
        let id = defs.alloc(kind, name, BUILTIN_SPAN, Some(module));
        out.names.push((name.to_string(), id));
        id
    };

    for name in PRIMITIVES.iter().chain(LIBRARY_TYPES).chain(C_SCALARS) {
        declare(defs, DefKind::Primitive, name, &mut prelude);
    }
    for name in INTERFACES {
        declare(defs, DefKind::Interface, name, &mut prelude);
    }
    for name in FUNCTIONS {
        declare(defs, DefKind::Fn, name, &mut prelude);
    }
    // `ffi` is a module of the prelude, and its own names live in its own
    // scope rather than in the prelude's.
    let ffi = declare(defs, DefKind::Module, "ffi", &mut prelude);
    let mut ffi_names = Vec::new();
    for name in FFI_TYPES {
        let id = defs.alloc(DefKind::Primitive, *name, BUILTIN_SPAN, Some(ffi));
        ffi_names.push((name.to_string(), id));
    }
    for name in FFI_INTERFACES {
        let id = defs.alloc(DefKind::Interface, *name, BUILTIN_SPAN, Some(ffi));
        ffi_names.push((name.to_string(), id));
    }
    prelude.modules.push((ffi, ffi_names));

    for (name, variants) in CHOICES {
        let choice_id = declare(defs, DefKind::Choice, name, &mut prelude);
        for (variant, arity) in *variants {
            let id = defs.alloc(DefKind::Variant, *variant, BUILTIN_SPAN, Some(choice_id));
            prelude.variants.push((variant.to_string(), id));
            prelude.variant_arity.push((id, *arity));
        }
    }

    // Every name above exists before a single signature is written, which is
    // what lets a declaration name any of them without an ordering rule.
    let mut declarer = Declarer {
        defs,
        module,
        names: prelude.names.iter().cloned().collect(),
        items: Vec::new(),
    };
    for decl in INTERFACE_DECLS {
        declarer.interface(decl);
    }
    for block in BLOCKS {
        declarer.block(block);
    }
    for (ty, interfaces) in IMPLEMENTS {
        for interface in *interfaces {
            declarer.relation(ty, interface);
        }
    }
    for function in FUNCTION_SIGNATURES {
        declarer.free(function);
    }
    prelude.items = declarer.items;

    prelude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prelude_defines_the_primitives_of_section_5_1() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for name in ["String", "Bool", "Int", "I32", "Never"] {
            assert!(
                prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` must be in the prelude"
            );
        }
    }

    #[test]
    fn the_option_and_result_variants_are_gone_and_the_names_are_free() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        // Revision 2 §3 removed all four with the types that carried them.
        // This asserts they are *free*, not merely absent: a program may now
        // declare its own `Ok`, and a `Some` in a pattern is a binding.
        for name in ["Some", "None", "Ok", "Err"] {
            assert!(
                !prelude.variants.iter().any(|(n, _)| n == name),
                "`{name}` was removed with `Option` and `Result`"
            );
        }
        for name in ["Option", "Result"] {
            assert!(
                !prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` was removed by revision 2 §3"
            );
        }
    }

    #[test]
    fn error_is_an_interface_in_the_prelude() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        let (_, id) = prelude
            .names
            .iter()
            .find(|(n, _)| n == "Error")
            .expect("`Error` is the interface every fallible signature names");
        assert_eq!(defs.get(*id).kind, DefKind::Interface);
    }


    #[test]
    fn the_ffi_module_holds_the_vocabulary_of_section_1_3() {
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);

        let (_, names) = prelude
            .modules
            .iter()
            .find(|(id, _)| defs.get(*id).name == "ffi")
            .expect("`ffi` must be a module of the prelude");

        for name in ["Span", "MutableSpan", "Pointer", "OpaqueHandle", "CStr", "CString"] {
            assert!(names.iter().any(|(n, _)| n == name), "`ffi.{name}` must exist");
        }
        // The two the parent note uses and its own table never defined.
        for name in ["Complex32", "Complex64"] {
            assert!(names.iter().any(|(n, _)| n == name), "`ffi.{name}` must exist");
        }
        // The marker interface is an interface, not a type: `implements` has
        // to reach it and a type position must not.
        let (_, layout) =
            names.iter().find(|(n, _)| n == "CLayout").expect("`ffi.CLayout` must exist");
        assert_eq!(defs.get(*layout).kind, DefKind::Interface);
    }

    #[test]
    fn the_ffi_names_are_not_also_bare_prelude_names() {
        // `Span` in the top level of the prelude would take a name a user
        // wants, which is the reason §1.3 spells every one of them `ffi.`.
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for name in ["Span", "Pointer", "CStr", "Complex64", "CLayout"] {
            assert!(
                !prelude.names.iter().any(|(n, _)| n == name),
                "`{name}` must be reachable only as `ffi.{name}`"
            );
        }
        assert!(prelude.names.iter().any(|(n, _)| n == "ffi"));
    }
    #[test]
    fn the_declared_surface_is_an_ordinary_hir_tree() {
        // The whole bet of this file: `science-types` lowers these through the
        // same walk it lowers a user's `Doc has:` through, so anything wrong
        // with them is wrong in a way that crate already reports.
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);

        let impls = prelude
            .items
            .iter()
            .filter(|item| matches!(item.kind, hir::ItemKind::Impl(_)))
            .count();
        let interfaces = prelude
            .items
            .iter()
            .filter(|item| matches!(item.kind, hir::ItemKind::Interface(_)))
            .count();
        let functions =
            prelude.items.iter().filter(|item| matches!(item.kind, hir::ItemKind::Fn(_))).count();

        assert_eq!(interfaces, INTERFACE_DECLS.len());
        assert_eq!(functions, FUNCTION_SIGNATURES.len());
        let relations: usize = IMPLEMENTS.iter().map(|(_, list)| list.len()).sum();
        assert_eq!(impls, BLOCKS.len() + relations);
        assert!(prelude.items.iter().all(|item| item.span == BUILTIN_SPAN));
    }

    #[test]
    fn map_get_borrows_its_map_and_not_its_key() {
        // The signature the two false positives in
        // `examples/09_absence_and_failure.science` turned on. Asserted here as
        // *text*, because the region engine reads it through a chain of three
        // crates and a test at the far end would not say which spelling it was
        // reading.
        let map = BLOCKS.iter().find(|block| block.ty == "Map").expect("`Map has:`");
        let get = map.methods.iter().find(|m| m.name == "get").expect("`Map.get`");
        assert!(matches!(get.recv, Some(SelfKind::Shared)));
        assert!(matches!(get.params, [("key", Ty::Ref(Ty::Var("K")))]));
        assert!(matches!(get.ret, Some(Ty::Opt(Ty::Ref(Ty::Var("V"))))));
    }

    #[test]
    fn the_prelude_declares_no_body_anywhere() {
        // A declaration is what the checker can read; a body is what
        // `science-rt` has to supply. Nothing here is codegen's to emit, and
        // this is the assertion that keeps it that way.
        let mut defs = DefTable::new();
        let prelude = build(&mut defs);
        for item in &prelude.items {
            let methods: Vec<&hir::Fn> = match &item.kind {
                hir::ItemKind::Fn(function) => vec![function],
                hir::ItemKind::Impl(block) => block.methods.iter().collect(),
                hir::ItemKind::Interface(interface) => interface.methods.iter().collect(),
                _ => Vec::new(),
            };
            for method in methods {
                assert!(
                    method.body.is_none(),
                    "`{}` has a body, and the prelude has nothing to put one in",
                    defs.get(method.def).name
                );
            }
        }
    }

    #[test]
    fn every_builtin_definition_is_marked_as_having_no_source() {
        let mut defs = DefTable::new();
        build(&mut defs);
        assert!(defs.iter().all(|d| d.is_builtin()));
    }
}
