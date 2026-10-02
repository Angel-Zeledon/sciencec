//! The prelude: the names §5.1 and §8 say exist before any file is read.
//!
//! Without this, `def f(a: &String)` reports an unresolved name, and every
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
    "Char", "String", "Never",
];

/// The primitives that have two spellings, and **one type**.
///
/// # The decision
///
/// `Int` and `I64` name one definition, and so do `Float` and `F64`. The
/// canonical definition is the **width**, and the short name is a second
/// spelling of it.
///
/// That direction is the codebase's own, not a preference: the rendered name
/// reaches diagnostics through `Def::name`, and some forty existing tests
/// already pin `I64` and `F64` as what a type renders as. Making `Int`
/// canonical would have rewritten every one of them for a cosmetic choice,
/// which is churn rather than a decision. It also matches how the note phrases
/// it — *"`Int` **is** `I64`"* names the width as the thing and the short name
/// as the way to say it.
///
/// # The reason: the notes say so, and this file disagreed with them
///
/// `codegen-and-linking.md` §4 is unambiguous — ***"`Int` is `I64`**, per the
/// runtime's §4 and core spec §5.1"* — and `indexing-and-array-literals.md`
/// §3.2 writes *"an unsuffixed integer literal is `Int` (`I64`)"*. This table
/// declared them as two separate primitives, which made them two types that do
/// not unify, and `ty`'s §5 has no implicit numeric conversion to paper over
/// it. The consequence was not academic: `Array.length()` returns `Int` and an
/// unsuffixed literal defaults to `I64`, so
///
/// ```text
/// let mutable total be 0
/// total be total + xs[i]
/// ```
///
/// was `SC0525`, *"expected `Int`, found `I64`"*, in the most ordinary loop
/// anybody writes. `builtins.rs` had the disagreement recorded as a stated cost
/// — *"the disagreement is `stdlib-core.md` §3.6's against Decision 2's integer
/// default"* — and recording it was the right thing to do right up until the
/// notes settled it, which they had.
///
/// # The cost
///
/// **Two names reach `Def::name` as one**, so `Types::render` prints `Int`
/// where a program may have written `I64`, and Decision 16 mangles `Int` into
/// symbols where it used to mangle `I64`. Both are deterministic and neither is
/// a width change: the layout was `I64`'s before and is `I64`'s now.
///
/// **`I8`/`I16`/`I32` stay their own definitions**, which makes `I64` the one
/// width in the family that is not, and that asymmetry is real. It is the
/// asymmetry the language already has: `Int` is the type a program is expected
/// to write and the others are what you reach for when the width is the point.
pub const ALIASED_PRIMITIVES: &[(&str, &str)] = &[("Int", "I64"), ("Float", "F64")];

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
///
/// **`Range` joins them, and it is the type `0..n` has.** The core spec §4.5
/// gives the expression and no type, and
/// `collections-and-chains.md`'s AMENDMENT 14 gives the type its only surface:
/// *"`Range` is not a container and implements `Iterate` directly, which is
/// what makes `for i in 0..n:` the same construct as everything else rather
/// than a special case in the parser"*. Until this line existed the expression
/// had no nominal type at all — `science-types`' `check` typed it
/// `Ty::ERROR` — so the loop variable of the most-written loop in the language
/// had no type either, and `ty`'s §5 absorption made a range agree with
/// everything it met anywhere else it was written.
/// `Formatter` joins them for `strings-formatting-and-docs.md` §3.1:
/// `Display.display(self, into: &mut Formatter)` needs a name for `into`'s
/// type, and §3.1 gives `Formatter` a complete, closed method set — `text`,
/// `raw`, `number`, `integer`, `spec` — which [`BLOCKS`] below transcribes.
/// `Formatter` has no fields a program can name (its own representation is
/// `science-rt`'s to pick, and `format.rs` there now does), so it is declared
/// bare here exactly like `Array`, `Map` and `Box` beside it. `FormatSpec` —
/// the record `spec()` returns — is *not* in this list: it has fields a
/// program *can* name (§3.1's `fill`, `align`, `sign`, …), so it needs
/// `DefKind::Record` and its own field declaration, which [`build`]'s
/// `format_spec` gives it the way `ffi_view` already gives `Span` its two.
const LIBRARY_TYPES: &[&str] = &[
    "Array",
    "Map",
    // `collections-and-chains.md` §5.1's AMENDMENT 10, the one type that note
    // adds to §8's list.
    "Set",
    "Box",
    "Chars",
    // `stdlib-core.md` §4.5's Level 1 `Lines`, from `String.lines()`. Level 1
    // and not `io`'s, on that note's argument: *"a language where avoiding
    // [materialising a forty-gigabyte file] requires an import has put the
    // trap on the default path"*.
    "Lines",
    // `stdlib-core.md` §6.9's `split(self, separator) -> Split`. §9 lists it
    // among the Level 1 types; what it hands out is decided at its block.
    "Split",
    "Range",
    "IoError",
    "TextError",
    "Formatter",
    // `collections-and-chains.md` §1.4's chain, narrowed to the links
    // [`BLOCKS`] transcribes. `ArrayIterate` is §5.4's source type by name;
    // the other six are §1.4's adapter names — `Discard of (Self, P)`,
    // `Keep of (Self, P)`, `MapOver of (Self, F)`, `Take of Self`,
    // `Skip of Self`, `SortedBy of (Self, F)`. What each one's parameters
    // are, and why they are not the note's, is argued at [`BLOCKS`]' chain
    // section.
    "ArrayIterate",
    "Discard",
    "Keep",
    "MapOver",
    "Take",
    "Skip",
    "SortedBy",
    // The second tranche: `numbered()`'s `NumberedOver`, §1.4's `take_while`,
    // `skip_while` and `every` adapters, and §5.4's `Map` sources —
    // `iterate()`'s `MapIterate` (by `ArrayIterate`'s pattern), `keys()`'s
    // `MapKeys` and `values()`'s `MapValues`. Same shape as the seven above.
    "NumberedOver",
    "TakeWhile",
    "SkipWhile",
    "Every",
    "Zip",
    "MapIterate",
    "MapKeys",
    "MapValues",
    "KeepSome",
    "Accumulate",
    "Reverse",
    "Unique",
    "Windows",
    "Batches",
    "Flatten",
    "FollowedBy",
    "Owned",
    // @LIB-END
];

/// The interfaces the compiler knows about (§5.4).
/// §5.4 lists seventeen, and the operator ones are load-bearing: "a scientific
/// language in which `+` does not work on your own type is not a scientific
/// language". Leaving them out made `Vector2 implements Add:` unresolvable in
/// three corpus files, which nothing caught until the driver ran resolution
/// over `examples/` for the first time.
const INTERFACES: &[&str] = &[
    "Add", "Sub", "Mul", "Div", "Rem", "Pow", "MatMul", "Neg", "Index", "Eq", "Ord", "Copy",
    "Clone", "Drop", "Iterate", "From", "Display",
    // `IndexMutably`, the eighteenth. §5.4 lists seventeen and does not have
    // it; `indexing-and-array-literals.md` §1.1's Decision 2 adds it, as the
    // half of indexing that `a[i] be v` dispatches to — the form §6.5 promises
    // and the parser already accepts. Declaring `Index` alone would leave every
    // write through an index unchecked, which is the silence that note's
    // amendment measures.
    "IndexMutably",
    // `Error`, the one-method interface of revision 2 §3.4. It is in the
    // prelude and not in a module because `-> (T, Error?)` is the signature of
    // every fallible function in the language, and a name that common cannot
    // need an import. `Error?` is shorthand for `(any Error)?`; the expansion
    // is in `resolve_nullable_inner`, not here.
    "Error",
    // `Write`, `stdlib-core.md` §4.2's one-method interface over *"a place
    // bytes go"*: Level 1 by §4.5's table, so that `io`'s `BufferedWriter of
    // W` can bound `W` by it without the interface living beside the type.
    "Write",
    // `Read`, §4.2's other half: *"a place bytes come from"*. Level 1 by the
    // same table, so that `io`'s `BufferedReader of R` bounds `R` by it as
    // `BufferedWriter` bounds `W` by `Write`.
    "Read",
];

/// The free functions (§8).
///
/// Nine, not `stdlib-core.md` §9's thirteen. The four this list does not have
/// — `read_bytes`, `write_bytes`, `read_lines`, `write_lines` — are names in
/// every program's scope forever (§1.3), and §12 of that note names
/// *"thirteen free functions, and the trend is upward"* as its own second
/// risk.
///
/// # `read_line`, the ninth, and why it and not the other four
///
/// §4.4 adds it *"with the same reluctance"* as `flush`, and for one program:
/// without it Science cannot write a filter — `cat data | summarise` — and
/// §4.5 puts it at Level 1 by name. It has an entry point,
/// `science_read_line`, and the backend's free-builtin arm beside
/// `read_file`'s, so it is not a name that resolves with nothing behind it.
///
/// **The other four are §5.1's Level 1 too, and still not here**, for the
/// reason the rest of this comment gives for every name that was: no
/// `science-rt` entry point stands behind any of them yet, and §5.3 gives all
/// four a `borrowed Path`, which does not exist. They are §5's surface and
/// this one is §4's. A program that wants a file's lines today has
/// `read_file(path)` then `.lines()`, or `io`'s
/// `File.open(path)` then `.lines()`, which does not hold the file in memory.
///
/// # Three of the thirteen stopped being a spec change, and now they are in
///
/// This comment used to rest its whole case on *"adding eight global names is a
/// spec change under §2.2's rule"*. **For three of them that stopped being
/// true.** `strings-formatting-and-docs.md` §4.3 makes the change and §8 item 5
/// asks §8 for it by name: the list *"becomes `print`, `write`, `print_error`,
/// `write_error`, `flush`, `panic`, `read_file`, `write_file`"*, and
/// `stdlib-core.md` §4.1 restates the whole of it as settled — *"`print_error`
/// and `write_error` go to stderr, `flush` is a free function … none of that is
/// reopened"*. So the decision this file would have been taking had been taken
/// elsewhere, by the note that owns the surface, and the question left here was
/// not *may they* but *can they mean anything yet*.
///
/// **For a while the answer was no, and the reason was `science-rt`, not the
/// namespace.** A name on this list resolves; a name that resolves with no
/// entry point behind it is checked in silence and refused at the end of the
/// build — *"a call to `write_error`, which this crate was given no MIR body
/// for"*, after the link the user waited for. `print_error("…")` was `SC0201`
/// at the call instead, and that was true: the compiler had no `print_error`.
/// The runtime had `science_write_error_bytes`, but as **codegen support** for
/// the emitted `main`, taking bytes because its message is a constant in the
/// binary; and it had no flush but `science_exit`'s.
///
/// # The handover, and how it was redeemed
///
/// The handover this comment wrote asked for three entry points in
/// `science-rt`, each the stderr or flush twin of a symbol that already
/// existed, each added to `science-codegen`'s `RUNTIME` table and given an arm
/// beside `science-codegen-llvm`'s `lower_print`. All three exist:
///
/// - `science_write_error(text: *const ScienceString)` — the `ScienceString`
///   form of `science_write_error_bytes`, and nothing more: it hands that
///   symbol its bytes, so stderr has one writer and one policy.
/// - `science_print_error(text: *const ScienceString)` — the same with one
///   `\n`, which is `science_print`'s relation to `science_write`.
/// - `science_flush()` — the flush `science_exit` performs on the way out,
///   without the way out. §4.2 adds `flush` *"reluctantly"* for exactly one
///   case, a `write` of a progress line with no newline, and that case is
///   live because stdout is buffered — by line on a terminal, by 64 KiB block
///   in a pipe.
///
/// **`print_error` and `write_error` carry no signature, for the reason
/// `print` and `write` carry none** — §4.1 gives all four the same
/// `(value: borrowed any Display)`, and [`FUNCTION_SIGNATURES`] says at length
/// why that parameter cannot be declared yet. They are unary by
/// `science-types`' `Prelude::unary_output`, and a non-`String` argument is
/// rendered by `science-mir`'s `prints_by_rendering` exactly as `print`'s is,
/// so `print_error(42)` means what `print(42)` means, one stream over.
///
/// **`flush` does carry one, and that is a departure from the handover's last
/// line**, which said *"the names carry no signature"*. The reason given for
/// `print` — a `borrowed any Display` parameter the back half cannot lower —
/// does not reach a function with no parameter at all. `def flush()` declares
/// exactly what §4.2 means, so `flush(x)` is the ordinary arity diagnostic
/// rather than a call accepted in silence, and `let x be flush()` is `()`
/// rather than an error type the checker agrees with everything through. The
/// cost is one row in [`FUNCTION_SIGNATURES`] and one arm in the backend's
/// free-builtin dispatch, beside `read_file`'s.
///
/// **§4.2's buffering policy is `science-rt`'s, and implemented there**:
/// stdout line-buffered on a terminal and 64 KiB block-buffered otherwise,
/// decided at the first write (`science-rt`'s `stdout.rs`). Nothing here
/// depends on it; it is recorded because this comment once said it was
/// missing.
const FUNCTIONS: &[&str] = &[
    "print", "write", "print_error", "write_error", "flush", "panic", "read_file", "write_file",
    "read_line",
];

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
    //
    // `Span` and `MutableSpan` are the two names in this list [`build`] gives
    // fields to, rather than declaring bare like the rest: §1.3 states their
    // shape as `{ borrowed T, Int }` — a pointer that borrows and a length
    // that does not cross the ABI — and a *bare* declaration hid that shape
    // from `science_types::ownership::needs_drop`, which cannot tell a view
    // that owns nothing from a choice type or a foreign union and answers
    // conservatively where it cannot tell. That false `true` was Decision
    // 27's field-read widening firing on a `Span` read through a borrow that
    // never should have widened at all — see `science-types`' `check.rs`
    // `borrow_ergonomics`. Declaring the fields is not a wider ask than the
    // rest of this list: §1.3 already commits to the shape, in words; this
    // only transcribes it.
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
/// Empty from revision 2 §3 until `strings-formatting-and-docs.md` §3.1 gave
/// the prelude a reason to have one again: `Align`, `Sign`, `Code` and
/// `Grouping` are `FormatSpec`'s four `choice` fields, and §3.1 names all
/// four by name — *"with `Align`, `Sign`, `Code` and `Grouping` as `choice`
/// types"* — although it gives their variants only as the characters of
/// §2.1's grammar, not as Science identifiers. Every variant below is one of
/// those characters, spelled out and in the grammar's own declaration order,
/// which is not a second decision this file is taking: §2.1's `code`
/// production already lists `f e E g G % d x X o b s` in exactly this order,
/// so `Code`'s eight-to-twelfth variants are that order transcribed, the same
/// move [`INTERFACE_DECLS`]' `Clone` entry makes for a signature no note
/// spells in Science syntax. Every variant is payload-free — `<` needs no
/// argument to be `<` — so every arity below is `0`.
const CHOICES: &[(&str, &[(&str, usize)])] = &[
    // `<` `>` `^`, §2.1's `align` production.
    ("Align", &[("Left", 0), ("Right", 0), ("Center", 0)]),
    // `+` `-` `' '`, §2.1's `sign` production.
    ("Sign", &[("Plus", 0), ("Minus", 0), ("Space", 0)]),
    // `f e E g G % d x X o b s`, §2.1's `code` production, in that order.
    (
        "Code",
        &[
            ("Fixed", 0),
            ("Exp", 0),
            ("ExpUpper", 0),
            ("General", 0),
            ("GeneralUpper", 0),
            ("Percent", 0),
            ("Decimal", 0),
            ("Hex", 0),
            ("HexUpper", 0),
            ("Octal", 0),
            ("Binary", 0),
            ("Str", 0),
        ],
    ),
    // `,` `_`, §2.1's `grouping` production.
    ("Grouping", &[("Comma", 0), ("Underscore", 0)]),
];

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
    /// A prelude type applied to arguments: `Array[T]`.
    App(&'static str, &'static [Ty]),
    /// A type from the `ffi` module applied to arguments: `ffi.Span[T]`.
    ///
    /// **Its own variant because `ffi`'s names are not the prelude's**, and
    /// deliberately: `the_ffi_names_are_not_also_bare_prelude_names` is the
    /// test, and §1.3 spells every one of them `ffi.` so that `Span` stays a
    /// name a user may take. [`Ty::App`] resolves through
    /// [`Declarer::named`], which reads `prelude.names`, and `Span` is not in
    /// it — so a second lookup table is what naming one costs, and this says
    /// which table a name is being looked up in rather than merging the two.
    Ffi(&'static str, &'static [Ty]),
    /// A generic parameter of the enclosing block, by name.
    Var(&'static str),
    /// `Self.Item` (§5.4).
    Assoc(&'static str),
    /// `&T`.
    Ref(&'static Ty),
    /// `&mut T`. Written out rather than a flag on [`Ty::Ref`]
    /// because the one declaration that needs it — `IndexMutably.index_mutably`
    /// — is the only place in this file where the two differ, and a `bool` at
    /// every `Ref` would be a parameter every other line has to read past.
    MutRef(&'static Ty),
    /// `T?` (revision 2 §3.1).
    Opt(&'static Ty),
    /// A pair, which is what `-> (T, Error?)` is.
    Pair(&'static Ty, &'static Ty),
    /// `(A) -> B`: `collections-and-chains.md` §1.2's closure type, which that
    /// note's AMENDMENT 1 decided and the parser already builds.
    ///
    /// **Written out rather than taken as a parameter, and that is forced.**
    /// §1.4 spells every combinator as `map of (U, F)(self, f: F)` with a
    /// `where F: (Self.Item) -> U` beside it, and [`Method`] has no
    /// `where_clause` to put the bound in. Spelling the parameter `f: F`
    /// without the bound would be worse than a deviation — it would be a
    /// silent hole, because a closure literal checked against a bare
    /// `TyKind::Param` reaches `BodyChecker::closure`'s *"no concrete arrow"*
    /// arm, which binds `each` at `Ty::ERROR` and reports nothing. Writing
    /// the arrow out is the same contract with the indirection removed, and
    /// it is the spelling that makes `each` have a type.
    Fn(&'static [Ty], &'static Ty),
    /// `Self`, standing for the block's own implementing type. §5.4's
    /// [`hir::TypeKind::SelfType`] one level down.
    ///
    /// **Not used by [`Block`].** Every inherent and `implements` block below
    /// writes its own concrete type out — `Array.new() -> Array of T`, not
    /// `-> Self` — for the reason [`Scope`]'s own comment gives: the written
    /// form is checkable against the note it transcribes without the reader
    /// holding a substitution in their head. `Self` earns its keep exactly
    /// once, for [`INTERFACE_DECLS`]' `Clone.clone`, where the block that
    /// resolves it is not this file's to write — it is whichever type an
    /// `implements Clone:` names, which by definition this file does not know
    /// yet. `subst`'s `TyKind::SelfType` is built for precisely this: it
    /// carries the block, and `check`'s `block_substitution` rewrites it with
    /// the receiver's own type at the call, the same way it already does for
    /// `Self.Item` and `Self.Output`.
    SelfTy,
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
    /// The method's **own** generic parameters, on top of the block's.
    ///
    /// Empty for every declaration but the chain vocabulary's, and the reason
    /// it had to exist is `map`: `collections-and-chains.md` §1.4 gives it
    /// `map of (U, F)(self, f: F) -> MapOver of (Self, F)`, and `U` — the
    /// item type *after* the link — is introduced by the method and by
    /// nothing else. A block parameter cannot stand in for it, because one
    /// chain value may be mapped twice at two different result types.
    generics: &'static [&'static str],
    /// `None` is an associated function — `Array.new()` — which takes no
    /// receiver at all. `science-types`' `Form` is computed from exactly this.
    recv: Option<SelfKind>,
    params: &'static [(&'static str, Ty)],
    /// `None` is `()`, which is how the HIR spells a `def` with no `->`.
    ret: Option<Ty>,
}

/// `Array[T] has:`, or `String implements Clone:`.
#[derive(Debug, Clone, Copy)]
struct Block {
    ty: &'static str,
    /// The block's own generic parameters — the `T` of `Array[T] has:`.
    generics: &'static [&'static str],
    /// `Some` for `implements`, `None` for the inherent `has:`, carrying the
    /// interface's *arguments*: `Array[T] implements Index[Int]:` is
    /// `Some(("Index", &[INT]))`. The list is empty for an interface that takes
    /// no parameter, which is every one of them but two.
    interface: Option<(&'static str, &'static [Ty])>,
    /// `type Item is Char` — this block's side of §5.4.
    assoc: &'static [(&'static str, Ty)],
    methods: &'static [Method],
}

/// `interface Error:` with what it declares.
#[derive(Debug, Clone, Copy)]
struct InterfaceDecl {
    name: &'static str,
    /// The interface's own parameters — the `Idx` of `interface Index[Idx]:`.
    /// Empty for every interface that is a bare name.
    generics: &'static [&'static str],
    /// `type Item`, declared and unanswered.
    assoc: &'static [&'static str],
    methods: &'static [Method],
}

/// The interfaces that get a method, and why only seven do.
///
/// **Decision. An interface is declared with its methods only where a note
/// gives the method's name and its types.** `Error.message` is
/// `stdlib-core.md` §7.5 verbatim; `Iterate.next` is the one
/// `collections-and-chains.md` builds its chain vocabulary on; `Index.index`
/// and `IndexMutably.index_mutably` are `indexing-and-array-literals.md`
/// §1.1's Decision 2, written out in Science in that note and transcribed
/// below; `Clone.clone` is the fifth, and its own comment below says why it
/// passes the same test although no note writes its signature in Science
/// syntax. `Ord.less` is the seventh, and it is `stdlib-shape-and-packages.md`
/// §4.5's AMENDMENT 1 to Decision 4c, written for it. The other twelve —
/// `Add`, `Eq` and the rest — are declared as **names with implementations
/// and no methods**, which is the whole of what the bound check needs:
/// `methods`' §7 asks *"does `I64` implement `Add`"* and never *"what is
/// `Add`'s method called"*.
///
/// **`Display.display` is the sixth, and it stood refused for three prior
/// commits on a premise this file itself got wrong.** The refusal read
/// *"writing `Display.display(Formatter)` would invent `Formatter`, a Level 1
/// type no note specifies"* — but `strings-formatting-and-docs.md` §3.1 does
/// specify it, completely: `Formatter has: def text/raw/number/integer/spec`,
/// plus `FormatSpec` and its four `choice` fields, `Align`, `Sign`, `Code`,
/// `Grouping`. `STDLIB-DECISIONS.md` §1 traces the error to its root — the
/// comment conflated `Formatter` with `Ordering`, which really is
/// unspecified — and recommends adopting §3.1 verbatim. That recommendation
/// is now taken: `Formatter`, `FormatSpec` and the four `choice` types are
/// declared above ([`LIBRARY_TYPES`], [`CHOICES`], [`Declarer::format_spec`]),
/// so `Display.display`'s signature is transcribed rather than invented, the
/// same move already made for `Clone.clone`.
///
/// **What this declaration reaches, and what it does not.** `science-mir`'s
/// `Builder::display_of` looks the method up at every `f"…"` hole the
/// builder has no entry point for, and `Builder::render_through_display`
/// emits the call; `science-codegen-llvm` gives `Formatter` a layout, maps
/// four of its five methods to `science-rt` entry points, and declares a
/// `Formatter` local owns nothing. So `print(v)` on a concrete type that
/// writes `def display(self, into: &mut Formatter)` builds, links and runs —
/// `examples/06_traits.science` is the program.
///
/// What it does not reach is `any Display`: an interface object's `display`
/// is in a vtable slot and this backend emits no call through one, so
/// `print` of a `&any Display` is still refused. That is Decision 13's
/// dispatch and not this file's.
///
/// **The `WHOLLY_OPEN` regression `Clone.clone` had to patch does not recur
/// here, and that is checked rather than assumed.** Giving `Display` a
/// method flips `Methods::surface_is_closed` for every type
/// [`IMPLEMENTS`] lists against it — every numeric primitive, `Bool`,
/// `Char`, `String`, `IoError`, `TextError` — the same mechanism that made
/// `Clone.clone` add sixteen names to [`WHOLLY_OPEN`]. Every one of those
/// `Display` implementors already implements `Clone` too (compare
/// [`NUMERIC`] and the `Bool`/`Char`/`String`/`IoError`/`TextError` rows of
/// [`IMPLEMENTS`]), so `Clone.clone` already closed all of them; `Display`'s
/// method finds each surface already closed and changes nothing further.
/// [`WHOLLY_OPEN`] is therefore unchanged by this declaration, and the full
/// corpus run this session's report cites is what confirms it rather than a
/// second table audit.
///
/// **What the twelve still buy, now that the implementations are
/// declared**, is the *requirement*: `check`'s `implements_operand` refuses
/// `a + b` on a type that has no `implements Add:` block, and the dispatch
/// names the method beside the operator rather than here. **`< > <= >=` are
/// no longer in that position**: `Ord` has its method now, so the four order
/// operators dispatch to `less` and the requirement is the dispatch's own
/// `SC0535` — see `Ord`'s entry below for the decision and what it costs.
const INTERFACE_DECLS: &[InterfaceDecl] = &[
    InterfaceDecl {
        name: "Error",
        generics: &[],
        assoc: &[],
        // §7.5's `cause` is *defaulted* — it has a body — and a body is the one
        // thing a declaration cannot carry. Declaring it here would turn a
        // defaulted method into a required one, so it is left out.
        methods: &[Method {
            name: "message",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(STRING),
        }],
    },
    // §4.2, verbatim: *"Writes all of `bytes`, or fails. There is no partial
    // write."* Returning `Error?` and not a count is that note's decision
    // against `data-io.md` §7's `(U64, IoError?)`, argued there.
    InterfaceDecl {
        name: "Write",
        generics: &[],
        assoc: &[],
        methods: &[Method {
            name: "write",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[("bytes", Ty::Ref(&Ty::App("Array", &[Ty::Name("U8")])))],
            ret: Some(Ty::Opt(&Ty::Name("Error"))),
        }],
    },
    // §4.2's other half, verbatim: *"Fills as much of `into` as is available.
    // Returns the count. A count of zero with no error means end of input,
    // and is the only end-of-input signal."* `into`'s length is how much is
    // asked for — an `Array` holds its elements, so the caller sizes it —
    // and the count is how many of its first bytes were written.
    InterfaceDecl {
        name: "Read",
        generics: &[],
        assoc: &[],
        methods: &[Method {
            name: "read",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[("into", Ty::MutRef(&Ty::App("Array", &[Ty::Name("U8")])))],
            ret: Some(Ty::Pair(&Ty::Name("U64"), &Ty::Opt(&Ty::Name("Error")))),
        }],
    },
    InterfaceDecl {
        name: "Iterate",
        generics: &[],
        assoc: &["Item"],
        methods: &[Method {
            name: "next",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&Ty::Assoc("Item"))),
        }],
    },
    // --- `indexing-and-array-literals.md` §1.1, Decision 2 ----------------
    //
    // **The third and fourth interfaces to get a method, and they are the
    // first ones this file declares that §5.4 does not.** The rule above is
    // unchanged — *an interface is declared with its methods only where a note
    // gives the method's name **and its types*** — and §1.1 gives both, in
    // Science, verbatim:
    //
    // ```text
    // interface Index[Idx]:
    //     type Output
    //     def index(self, at: Idx) -> &Self.Output
    //
    // interface IndexMutably[Idx]:
    //     type Output
    //     def index_mutably(mutable self, at: Idx) -> &mut Self.Output
    // ```
    //
    // So no name and no type here is invented. What *was* missing is the
    // shape: an associated type on a **parameterised** interface, which that
    // note's own amendment names as the blocker — `Iterate.Item` was the only
    // associated type in the language and `Iterate` takes no parameter. The
    // shape turned out to be two independent pieces the declarer already had
    // one of each: `From[ParseError]` is a parameterised interface the
    // corpus writes, `Iterate.Item` is an associated type an implementation
    // answers, and nothing in `science-types` cared that no declaration put the
    // two in one interface. [`InterfaceDecl::generics`] and [`Block::interface`]
    // carrying arguments are the whole of the change.
    //
    // **`Self.Output` resolves the way `Self.Item` does**, through
    // `Declarations::body_substitution`: the implementation's `type Output is
    // T` is bound under *both* its own definition and the interface's, by name,
    // and `check`'s `block_substitution` then rewrites the `T` in it with the
    // receiver's argument. The interface's `Idx` is not in that path at all —
    // it is a parameter of the *bound*, and the operand's type is read off the
    // implementation's own `index` exactly as `+`'s is. That is what makes the
    // parameterised-plus-associated shape cost nothing extra: the two halves
    // never meet.
    //
    // **Two interfaces rather than one**, which is Decision 2's whole content:
    // a read-only container implements only the first, and `a[i] be v` on one
    // is `SC0535` naming `IndexMutably` rather than a sentence about
    // mutability in the abstract.
    InterfaceDecl {
        name: "Index",
        generics: &["Idx"],
        assoc: &["Output"],
        methods: &[Method {
            name: "index",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[("at", Ty::Var("Idx"))],
            ret: Some(Ty::Ref(&Ty::Assoc("Output"))),
        }],
    },
    InterfaceDecl {
        name: "IndexMutably",
        generics: &["Idx"],
        assoc: &["Output"],
        methods: &[Method {
            name: "index_mutably",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[("at", Ty::Var("Idx"))],
            ret: Some(Ty::MutRef(&Ty::Assoc("Output"))),
        }],
    },
    // --- `Clone`, `stdlib-core.md` §6.2 -----------------------------------
    //
    // **The fifth interface to get a method, and the first where no note
    // writes the signature in Science syntax.** The rule above holds a note
    // to giving *"the method's name and its types"*, and what §6.2 gives is
    // prose: *"`.owned()` and `.clone()` are both written"*, plus
    // `collections-and-chains.md` §1.4's `where Self.Item is borrowed T,
    // T: Clone`, which fixes the *bound* and not the method it requires.
    // Nowhere is there a line reading `def clone(self) -> Self`.
    //
    // **Declared anyway, and the reason is not "it is convenient" — it is
    // that no second candidate exists to invent.** This is the distinction
    // `print`'s decision (above) draws and `Box.new`'s draws again: a
    // signature is a decision taken here only when the alternatives are real.
    // `clone` has none. `Clone` is a marker with no parameter and no
    // associated type (§5.4 lists it bare), so its one method takes no
    // argument beyond the receiver; a duplicate cannot come back narrower or
    // wider than what it duplicates, so the return is the receiver's own
    // type and nothing else; and the receiver reads rather than consumes,
    // which every one of the corpus's three call sites already assumes —
    // `examples/07_generics.science`'s `value.clone()` is called twice on
    // one `borrowed T` and a second call after a moving receiver would be a
    // use-after-move the corpus does not have. `def clone(self) -> Self` is
    // therefore not a choice among signatures; it is the only shape left
    // once "no argument" and "does not consume" are read off the language
    // `Clone` already has to be. `stdlib-shape-and-packages.md` §4.5's
    // Decision 4c — an operator trait's method takes the trait's own name in
    // lowercase when that name is free — is the same reasoning this file
    // already leans on for `Add.add` and `Index.index`, and `clone` is free.
    //
    // **`Self` and not a written-out type**, unlike every other block in this
    // file. [`Ty::SelfTy`]'s own comment says why: the type that answers
    // `Self` here is whichever one writes `implements Clone:`, which this
    // declaration cannot name — `String` today, a user's own record
    // tomorrow — so the block that resolves it has to be read off the
    // *call*, through `TyKind::SelfType`, the same mechanism a user's own
    // `interface Summarize: def duplicate(self) -> Self` would use.
    //
    // **The runtime side is checked, not assumed, against the trap this
    // repository has already been bitten by once.** `science_string_clone`
    // exists in `science-rt/src/string.rs`, is documented there as
    // `` `Clone::clone` for `String` ``, and its signature —
    // `(value: *const ScienceString) -> ScienceString` — is exactly a shared
    // receiver in and the same type out: no width, no mutability and no unit
    // (bytes vs. characters) is left for this declaration to guess at, which
    // is what made `science_string_truncate` wrong. `lower.rs`'s
    // `PRELUDE_METHODS` table gains the row that reaches it.
    //
    // **What is not settled by this:** a `value.clone()` on a *bounded type
    // parameter* — `examples/07_generics.science`'s own call, `T: Clone` —
    // stays silent. `methods`' §5 says why: `Methods::receiver` returns
    // `None` for a `TyKind::Param` on purpose, so that call was silent
    // before this declaration and is silent after it, for a reason this
    // file does not own. What this declaration closes is `"hi".clone()` on a
    // receiver whose type is concrete and known — `methods`' own running
    // example.
    InterfaceDecl {
        name: "Clone",
        generics: &[],
        assoc: &[],
        methods: &[Method {
            name: "clone",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(Ty::SelfTy),
        }],
    },
    // --- `Ord`, `stdlib-shape-and-packages.md` §4.5, AMENDMENT 1 ---------
    //
    // **Decision. `Ord`'s one method is `def less(self, other: &Self) ->
    // Bool`, and the four order operators are written over it:** `a < b` is
    // `a.less(b)`, `a > b` is `b.less(a)`, `a <= b` is `not b.less(a)` and
    // `a >= b` is `not a.less(b)`. `check`'s `BodyChecker::ordering` is the
    // desugaring.
    //
    // **Reason.** The alternative every reader expects — `compare(self,
    // other) -> Ordering` — needs a Level 1 `choice Ordering` that no note
    // specifies, and §8's library is closed: adding a type to it is a spec
    // change, not a detail an operator may bring along. `less` needs nothing
    // that does not exist. It is also what the corpus already writes —
    // `examples/19_stdlib.science`'s `Record implements Ord: def less(self,
    // other: &Record) -> Bool` and `06_traits`' `Note` — and it is the one
    // comparison every sorting and selection algorithm is defined over: a
    // strict weak order is `<` and nothing else, which is why Python's
    // `sorted`, `min` and `max` call only `__lt__` and C++'s `Compare` is a
    // `less`. `F64`'s NaN, the third obstacle this file used to list, is not
    // `Ord`'s question at all: §5.1 keeps floats out of `Ord`, and `<` on an
    // `F64` stays the structural IEEE comparison.
    //
    // **`other` is borrowed**, because a comparison reads both operands and
    // consumes neither — `science-mir`'s `force_copy` over the six comparison
    // operators says the same thing one phase down. `06_traits`' `Note` wrote
    // `other: Note` by value and is amended to `&Note`; a by-value operand
    // would have moved the right-hand side of `a < b` into the call.
    //
    // **`Eq` is not derived from it and does not derive it.** `is` stays
    // `Eq.eq`, a separate implementation, and the law that ties the two —
    // `a is b` exactly when `not a.less(b) and not b.less(a)` — is prose, as
    // `contracts.md` leaves every interface law for now.
    //
    // **Cost.** `a > b` and `a <= b` evaluate `b` before `a`, because the
    // receiver is evaluated first and THIR has no binding form to keep the
    // written order in; it is observable only when both operands have
    // effects. A three-way answer costs two calls where `compare` would be
    // one. And `intrinsics-math-physics.md`'s `total_order -> Ordering`
    // still has no type to return — the day `Ordering` is specified,
    // `compare` joins `less` as a defaulted method and no implementation of
    // `less` changes.
    InterfaceDecl {
        name: "Ord",
        generics: &[],
        assoc: &[],
        methods: &[Method {
            name: "less",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[("other", Ty::Ref(&Ty::SelfTy))],
            ret: Some(BOOL),
        }],
    },
    // --- `Display`, `strings-formatting-and-docs.md` §3.1 ----------------
    //
    // **The sixth interface to get a method, and the first whose signature
    // this file once refused on a premise that was wrong.** §3.1 writes
    // `interface Display: def display(self, into: mutable borrowed
    // Formatter)` verbatim; `mutable borrowed Formatter` is the note's own
    // pre-revision-2 spelling and `&mut Formatter` is `STDLIB-DECISIONS.md`
    // §1.4's translation of it into the syntax this parser accepts today —
    // the same translation [`Ty::MutRef`] performs for `IndexMutably.
    // index_mutably` above. Nothing about the receiver or the argument is
    // invented: `self` is shared (the note writes bare `self`, not `mutable
    // self`), and `Formatter` is [`LIBRARY_TYPES`]'s new entry.
    InterfaceDecl {
        name: "Display",
        generics: &[],
        assoc: &[],
        methods: &[Method {
            name: "display",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[("into", Ty::MutRef(&Ty::Name("Formatter")))],
            ret: None,
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

/// The interfaces a floating-point primitive implements: [`NUMERIC`] without
/// `Ord`.
///
/// **Decision. `F16`, `BF16`, `F32`, `F64` and `Float` implement `Eq` and not
/// `Ord`.** The core spec's §5.1 says it in one line -- *"`F32`/`F64`
/// implement `Eq` but **not** `Ord`, because NaN has no total order. Sorting
/// floats uses an explicit total order function"* -- and
/// `collections-and-chains.md` §5.3 repeats it as the reason `F32` is not a
/// `Hash` key. The table listed them under `Ord` anyway, which let `T: Ord`
/// accept a float and `sorted()` order NaNs by whatever the machine did.
///
/// **Reason.** The bound is a promise of a total order and a float cannot keep
/// it.
///
/// **Cost.** `a < b` on two floats is still a primitive comparison -- the
/// checker's operator arm never asks a numeric primitive for `Ord` -- so
/// ordinary float arithmetic is untouched. What stops compiling is handing a
/// float to a `T: Ord` parameter, which is the point.
const FLOATING: &[&str] = &["Add", "Sub", "Mul", "Div", "Rem", "Neg", "Eq", "Copy", "Clone", "Display"];

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
/// **What is not here is every generic type.** `Array[T] implements Clone`
/// holds only where `T: Clone`, and a conditional implementation is not
/// something this index can express — `methods`' §4 says in as many words that
/// a blanket implementation *"is not looked through"*. Declaring it
/// unconditionally would admit `Array[Doc]: Clone` for a `Doc` that is not
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
    ("F16", FLOATING),
    ("BF16", FLOATING),
    ("F32", FLOATING),
    ("F64", FLOATING),
    // No `Int` or `Float` row: they are `ALIASED_PRIMITIVES` and name `I64` and
    // `F64`, so a row for each would declare every relation twice and make
    // `x.clone()` on an `Int` two candidates (`SC0531`).
    ("Bool", &["Eq", "Copy", "Clone", "Display"]),
    ("Char", &["Eq", "Ord", "Copy", "Clone", "Display"]),
    // §6.9. `Copy` is *not* among them and that is the point of §6.2: an owned
    // copy of a `String` allocates, so it is a `clone` and never a move-free
    // duplication.
    ("String", &["Clone", "Eq", "Ord", "Add", "Display"]),
    // §7.2's three Level 1 error types. `Error` on a concrete error is what
    // lets it stand in an `Error?` slot, which `assign`'s §3 asks about by
    // name.
    // `Error` is not in `IoError`'s or `TextError`'s row: each has a block
    // below that declares it, with the method, for `Chars`' reason.
    ("IoError", &["Display", "Eq", "Clone"]),
    ("TextError", &["Display", "Eq", "Clone"]),
];

/// The thirteen chain links one chain type answers, as [`Method`]s.
///
/// `this` is the chain type itself — what a link consumes and names as its
/// new source; `item` its `Item`; `borrowed` how a predicate or a key closure
/// takes that item (collapsed to `item` on `ArrayIterate`, whose item is
/// already a borrow); `total` what `sum()` declares. [`BLOCKS`]' chain section
/// says why this is a macro and not a hand-written table, and why each
/// signature is the one it is.
macro_rules! chain_links {
    ($this:expr, item: $item:expr, borrowed: $borrowed:expr, total: $total:expr $(,)?) => {
        &[
            // The adapters. Each returns a chain type whose source is `this`.
            Method {
                name: "discard",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(Ty::App("Discard", &[$this, $item])),
            },
            Method {
                name: "keep",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(Ty::App("Keep", &[$this, $item])),
            },
            Method {
                name: "map",
                generics: &["U"],
                recv: Some(SelfKind::Value),
                params: &[("f", Ty::Fn(&[$item], &Ty::Var("U")))],
                ret: Some(Ty::App("MapOver", &[$this, Ty::Var("U")])),
            },
            Method {
                name: "take",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("n", INT)],
                ret: Some(Ty::App("Take", &[$this, $item])),
            },
            Method {
                name: "skip",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("n", INT)],
                ret: Some(Ty::App("Skip", &[$this, $item])),
            },
            Method {
                name: "sorted",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("by", Ty::Fn(&[$borrowed], &INT))],
                ret: Some(Ty::App("SortedBy", &[$this, $item])),
            },
            // `numbered()`: §1.3's `Numbered of T`, one record per item. The
            // adapter's own `Item` is the record, which is what the next
            // link's closure takes and what `collect()` names.
            Method {
                name: "numbered",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("NumberedOver", &[$this, Ty::App("Numbered", &[$item])])),
            },
            Method {
                name: "take_while",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(Ty::App("TakeWhile", &[$this, $item])),
            },
            Method {
                name: "skip_while",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(Ty::App("SkipWhile", &[$this, $item])),
            },
            Method {
                name: "every",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("n", INT)],
                ret: Some(Ty::App("Every", &[$this, $item])),
            },
            // `zip of O(self, other: O) -> Zip of (Self, O)` where `O:
            // Iterate`, narrowed to the one `O` a fused loop can walk beside
            // its own: `Array.iterate()`. The declaration cannot say `O.Item`
            // for a bounded parameter, and a second chain with links of its
            // own has no place in a loop that reads one cursor; an array's
            // `iterate()` is `ArrayIterate[U]`, and its item is `&U`, so the
            // pair is `Pair[Self.Item, &U]` — §1.3's `left` and `right`.
            Method {
                name: "zip",
                generics: &["U"],
                recv: Some(SelfKind::Value),
                params: &[("other", Ty::App("ArrayIterate", &[Ty::Var("U")]))],
                ret: Some(Ty::App(
                    "Zip",
                    &[$this, Ty::App("Pair", &[$item, Ty::Ref(&Ty::Var("U"))])],
                )),
            },
            // The terminals. Each runs the chain.
            Method {
                name: "collect",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("Array", &[$item])),
            },
            Method { name: "count", generics: &[], recv: Some(SelfKind::Value), params: &[], ret: Some(INT) },
            Method { name: "sum", generics: &[], recv: Some(SelfKind::Value), params: &[], ret: Some($total) },
            // `keep_some()`: items `T?` become `T`. The declaration names the
            // item; `science-types`' `chain_item_shape` peels the nullable,
            // as `chain_total` peels `sum()`'s borrow.
            Method {
                name: "keep_some",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("KeepSome", &[$this, $item])),
            },
            // `accumulate(initial, f)`: the running fold. The chain's item is
            // the state, `A`; `science-types` holds `A` to a type that owns
            // nothing, because the chain yields a copy of it each turn.
            Method {
                name: "accumulate",
                generics: &["A"],
                recv: Some(SelfKind::Value),
                params: &[
                    ("initial", Ty::Var("A")),
                    ("f", Ty::Fn(&[Ty::Var("A"), $item], &Ty::Var("A"))),
                ],
                ret: Some(Ty::App("Accumulate", &[$this, Ty::Var("A")])),
            },
            // `reverse()`: §1.4's second barrier. The item is unchanged.
            Method {
                name: "reverse",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("Reverse", &[$this, $item])),
            },
            // `unique()`: each distinct item once, the first of equals. The
            // item is unchanged; `science-types` holds it to a key a `Set`
            // can file (a number, `Bool`, `Char` or `String`, through one
            // borrow), and the fused loop keeps the seen keys in a `Set`.
            // **`unique(by: key)` is not declared**: one name, one shape.
            // `owned()`: a chain of borrows becomes a chain of copies, §1.4's
            // replacement for Rust's `cloned` and `copied`. The declaration
            // names the item unchanged; `science-types` replaces it with the
            // referent and refuses an item that is not a borrow of something
            // that can be copied or cloned.
            Method {
                name: "owned",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("Owned", &[$this, $item])),
            },
            // `followed_by(other)`: this chain's items, then `other`'s. Narrowed
            // as `zip` is to an `Array.iterate()`, whose item is `&U`;
            // `science-types` holds this chain's item to `&U` as well, since
            // one chain cannot yield two types.
            Method {
                name: "followed_by",
                generics: &["U"],
                recv: Some(SelfKind::Value),
                params: &[("other", Ty::App("ArrayIterate", &[Ty::Var("U")]))],
                ret: Some(Ty::App("FollowedBy", &[$this, Ty::Ref(&Ty::Var("U"))])),
            },
            // `flatten()`: the items are arrays (or borrows of arrays) and the
            // chain yields their elements. The declaration names the item
            // unchanged; `science-types`' `chain_item_shape` replaces it with
            // the element, as it does for `keep_some()`.
            Method {
                name: "flatten",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("Flatten", &[$this, $item])),
            },
            // `batches(n)`: disjoint runs of `n` items as arrays; the last is
            // shorter when the chain does not divide. A size below one is a
            // batch of one.
            Method {
                name: "batches",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("n", INT)],
                ret: Some(Ty::App("Batches", &[$this, Ty::App("Array", &[$item])])),
            },
            // `windows(n)`: every run of `n` consecutive items as an `Array` of
            // them — §1.4's overlapping pieces. A width below one yields none.
            Method {
                name: "windows",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("n", INT)],
                ret: Some(Ty::App("Windows", &[$this, Ty::App("Array", &[$item])])),
            },
            Method {
                name: "unique",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::App("Unique", &[$this, $item])),
            },
            // `tally(by: key)`: how many items had each key, a `Map[K, Int]`
            // in order of first appearance. **The key is whatever the
            // closure returns and must be an owned key a `Map` admits** (the
            // `SC0548` pass checks it): `each.name` over borrowed records is a
            // borrowed `String` and is refused, `each.name.clone()` is not.
            Method {
                name: "tally",
                generics: &["K"],
                recv: Some(SelfKind::Value),
                params: &[("by", Ty::Fn(&[$borrowed], &Ty::Var("K")))],
                ret: Some(Ty::App("Map", &[Ty::Var("K"), INT])),
            },
            // `product()` is `sum()`'s twin: the item with a borrow peeled, from
            // one, held to the same numbers by the same checker half.
            Method { name: "product", generics: &[], recv: Some(SelfKind::Value), params: &[], ret: Some($total) },
            // `reduce of A(self, initial: A, f: (A, Self.Item) -> A) -> A`.
            // The closure is the two-parameter form `(acc, x) giving ...`;
            // the item goes in by value, as `map`'s does.
            Method {
                name: "reduce",
                generics: &["A"],
                recv: Some(SelfKind::Value),
                params: &[
                    ("initial", Ty::Var("A")),
                    ("f", Ty::Fn(&[Ty::Var("A"), $item], &Ty::Var("A"))),
                ],
                ret: Some(Ty::Var("A")),
            },
            Method {
                name: "has_any",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(BOOL),
            },
            Method {
                name: "has_all",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(BOOL),
            },
            Method {
                name: "first",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[],
                ret: Some(Ty::Opt(&$item)),
            },
            Method {
                name: "find",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("predicate", Ty::Fn(&[$borrowed], &BOOL))],
                ret: Some(Ty::Opt(&$item)),
            },
            Method { name: "last", generics: &[], recv: Some(SelfKind::Value), params: &[], ret: Some(Ty::Opt(&$item)) },
            Method {
                name: "minimum",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("by", Ty::Fn(&[$borrowed], &INT))],
                ret: Some(Ty::Opt(&$item)),
            },
            Method {
                name: "maximum",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("by", Ty::Fn(&[$borrowed], &INT))],
                ret: Some(Ty::Opt(&$item)),
            },
        ]
    };
}

/// One adapter's block: `X[S, I]`, the chain it consumed and the item it
/// yields, answering [`chain_links!`]' thirteen names.
macro_rules! chain_adapter {
    ($name:literal) => {
        Block {
            ty: $name,
            generics: &["S", "I"],
            interface: None,
            assoc: &[],
            methods: chain_links!(
                Ty::App($name, &[Ty::Var("S"), Ty::Var("I")]),
                item: Ty::Var("I"),
                borrowed: Ty::Ref(&Ty::Var("I")),
                total: Ty::Var("I"),
            ),
        }
    };
}

/// One adapter's `implements Iterate:` block, `Item` being its `I`.
macro_rules! chain_iterate {
    ($name:literal) => {
        Block {
            ty: $name,
            generics: &["S", "I"],
            interface: Some(("Iterate", &[])),
            assoc: &[("Item", Ty::Var("I"))],
            methods: &[],
        }
    };
}

/// `stdlib-core.md` §8.1's thirty-five, on one float width.
///
/// **A macro because the two blocks differ in one name and nothing else.**
/// `F64 has:` and `F32 has:` are the same thirty-five signatures with the
/// width substituted, and two hand-copied tables of thirty-five rows are two
/// places for a parameter to be `F64` where it should say `F32`, verifier-clean
/// and wrong. §8.1's order, group by group.
macro_rules! float_methods {
    ($float:ident) => {{
        const T: Ty = Ty::Name(stringify!($float));
        const fn unary(name: &'static str) -> Method {
            Method { name, generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(T) }
        }
        const fn binary(name: &'static str, params: &'static [(&'static str, Ty)]) -> Method {
            Method { name, generics: &[], recv: Some(SelfKind::Shared), params, ret: Some(T) }
        }
        const fn predicate(name: &'static str) -> Method {
            Method { name, generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) }
        }
        &[
            // Elementary.
            unary("abs"),
            unary("sign"),
            unary("floor"),
            unary("ceil"),
            unary("round"),
            unary("trunc"),
            unary("fract"),
            binary("min", &[("other", T)]),
            binary("max", &[("other", T)]),
            Method {
                name: "clamp",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("low", T), ("high", T)],
                ret: Some(T),
            },
            binary("rem_euclid", &[("divisor", T)]),
            unary("sqrt"),
            unary("cbrt"),
            binary("hypot", &[("other", T)]),
            unary("exp"),
            unary("ln"),
            unary("log2"),
            unary("log10"),
            binary("pow", &[("exponent", T)]),
            // Trigonometry.
            unary("sin"),
            unary("cos"),
            unary("tan"),
            unary("asin"),
            unary("acos"),
            unary("atan"),
            binary("atan2", &[("x", T)]),
            unary("sinh"),
            unary("cosh"),
            unary("tanh"),
            unary("to_degrees"),
            unary("to_radians"),
            // Predicates. `is_close` is §8.4's *"relative comparison with the
            // library's default tolerances"*; the tolerance is decided where
            // it is lowered.
            predicate("is_nan"),
            predicate("is_infinite"),
            predicate("is_finite"),
            Method {
                name: "is_close",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("other", T)],
                ret: Some(BOOL),
            },
        ]
    }};
}

/// The prelude's blocks with methods in them.
///
/// `T has: def wrapping_add(self, other: T) -> T`, and `_sub` and `_mul`, for
/// one fixed-width integer `T`. See the comment above its uses in [`BLOCKS`].
macro_rules! wrapping_block {
    ($ty:literal) => {
        Block {
            ty: $ty,
            generics: &[],
            interface: None,
            assoc: &[],
            methods: &[
                Method {
                    name: "wrapping_add",
                    generics: &[],
                    recv: Some(SelfKind::Value),
                    params: &[("other", Ty::Name($ty))],
                    ret: Some(Ty::Name($ty)),
                },
                Method {
                    name: "wrapping_sub",
                    generics: &[],
                    recv: Some(SelfKind::Value),
                    params: &[("other", Ty::Name($ty))],
                    ret: Some(Ty::Name($ty)),
                },
                Method {
                    name: "wrapping_mul",
                    generics: &[],
                    recv: Some(SelfKind::Value),
                    params: &[("other", Ty::Name($ty))],
                    ret: Some(Ty::Name($ty)),
                },
            ],
        }
    };
}

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
            // **`new` is decided here, and no note declares it.** §3.6 is
            // headed *"Five signatures"* and `new` is not one of them; what the
            // notes have is *use* — §6.11 writes `let mutable values be
            // Array.new()` and §3.7 writes `Set.new()` and `Deque.new()` — and
            // `examples/` writes `(Array[X]).new()` eleven times. So the
            // name, the arity and the absence of an argument are attested and
            // only the declaration was missing.
            //
            // **The decision is `-> Array[T]` and nothing else**: an
            // associated function whose return names the block's own parameter.
            // It is the only spelling available, because the alternatives are
            // not signatures — they are rules about where `T` comes from, and
            // they live at the call. `science-types`' `receiver_arguments`
            // settles that: the instantiation the author wrote, or §6's
            // root-level match against the arguments, and `SC0536` when neither
            // answers.
            //
            // **The cost is that §6.11's own example does not compile.** `let
            // mutable values be Array.new()` has no instantiation, no
            // arguments, and no expected type, so nothing fixes `T`; the note
            // is reading `T` out of a later `push(value)` and out of the
            // function's `-> (Array[F64], TextError?)`, which is inference
            // across statements that Decision 1 does not have and does not
            // intend to. The same is true of §3.7's `Set.new()`. The corpus
            // already writes the form that does compile. This is a real
            // disagreement between `stdlib-core.md` and the compiler and it is
            // named rather than papered over: whoever reconciles them either
            // rewrites those two examples or argues for an expectation that
            // reaches into an associated call.
            Method {
                name: "new",
                generics: &[],
                recv: None,
                params: &[],
                ret: Some(Ty::App("Array", &[Ty::Var("T")])),
            },
            // §3.6 verbatim.
            Method {
                name: "push",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Var("T"))],
                ret: None,
            },
            // §3.6 verbatim, parentheses and all: `&T?` would read as
            // `&(T?)`.
            Method {
                name: "get",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("index", INT)],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("T")))),
            },
            // **Decided: `get_mutably` is transcribed, and the corpus's
            // `get_mut` is not it.**
            //
            // `indexing-and-array-literals.md` §1.4's Decision 5 writes the
            // block out in Science and gives *both* halves, name and types:
            //
            // ```text
            // Array[T] has:
            //     def get(self, at: Int) -> (&T)?
            //     def get_mutably(mutable self, at: Int) -> (&mut T)?
            // ```
            //
            // So this is transcription under the rule [`INTERFACE_DECLS`]
            // states — *declared only where a note gives the method's name and
            // its types* — and nothing here is invented. The parameter is named
            // `index` rather than §1.4's `at` to agree with `get` two entries
            // up, which §3.6 spells that way; §1.4 spells `get`'s `at` too, so
            // the note disagrees with itself about the label and this file
            // already took `stdlib-core.md`'s side for the read-only twin.
            //
            // **It is the mutable half of an operation whose other three
            // spellings are already declared.** `Array[T] implements
            // IndexMutably[Int]` is in [`BLOCKS`] below, so `a[i] be v`
            // checks; `get` is here, so the checked read checks. Without this
            // there is no checked *mutable* access at all — the author who
            // wants "give me a handle to element 0 if it is there" has only the
            // panicking bracket form.
            //
            // **This is not `from_bytes`' case, although it looks like it.**
            // The `String` block below leaves `from_bytes` and `bytes` out
            // because they are expressible and *no program in `examples/` calls
            // either*, so the acceptance corpus cannot measure them. Here the
            // corpus does call the operation — `19_stdlib.science:180` writes
            // `items.get_mut(0)`, and `examples/README.md` lists
            // `Array.get_mut` twice — it just calls it by a name no note gives.
            // The operation is attested; the spelling is the corpus's error.
            //
            // **The disagreement, stated for a human rather than settled here.**
            // `get_mut` is an abbreviation, and `collections-and-chains.md` §4.3
            // forbids abbreviations where a word exists — §5's table retires
            // `min`/`max`, `size_hint` and `dedup` under exactly that rule, and
            // §5.4 already spells the sibling accessors `values_mutably` and
            // `iterate_mutably`. So the language's own naming rule gives
            // `get_mutably` and the corpus wrote Rust's name. **Declaring
            // `get_mut` instead would carve the one exception §5 refuses to
            // carve**, in a file nobody reads, on the evidence of one call site.
            // The fix is three characters in `19_stdlib.science` and two lines
            // in `examples/README.md`, and it is the corpus's to make; until it
            // does, `items.get_mut(0)` stays `SC0532` and the message is true.
            //
            // **The cost.** A declared prelude method with no row in
            // `science-codegen-llvm`'s `prelude_method` table is accepted by the
            // front end and refused by the backend with `SC0400`, after the link
            // — the trade `FUNCTIONS`' comment above declines to make for
            // `print_error`. It is a smaller cost here for two reasons: nothing
            // in `examples/` reaches this declaration today (the one call site
            // is misspelled), so no program's diagnostic gets worse, and the row
            // is one line against a `science-rt` entry point that is
            // `science_array_get`'s mutable twin.
            Method {
                name: "get_mutably",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("index", INT)],
                ret: Some(Ty::Opt(&Ty::MutRef(&Ty::Var("T")))),
            },
            // **`pop` returns the element**, and this is the decision the
            // comment that stood here declined to take.
            //
            // §3.2 names `pop` — *"`Array` has `push` and `pop`"* — and gives
            // it no signature. The two candidates are `pop(mutable self) ->
            // T?`, which hands the element back, and `pop(mutable self)`,
            // which makes *"take it out and use it"* two calls. The comment
            // here refused to choose, on the ground that deciding it *"in a
            // file nobody reads, on the evidence of one call site"* would
            // settle it by accident. That was right, and it is why this is
            // now a decision taken deliberately rather than a default.
            //
            // **Reason.** `T?` is what `get` and `get_mutably` already
            // return, so an empty collection answers the same way whichever
            // of the three a program asks, and `if xs.pop()?:` is the shape
            // the language already has for it. The alternative makes the
            // empty case unobservable: `pop()` with no return either panics
            // on empty or silently does nothing, and both are worse than a
            // `T?` the caller must look at.
            //
            // **Cost.** `examples/21` wrote `self.ribs.pop()` as a statement
            // in a `-> ()` body, which this signature does not admit. That
            // file is corrected. A program that genuinely wants to discard
            // the element writes the discard, which is the direction §5's
            // *"a value that is thrown away is thrown away in the source"*
            // already points.
            //
            // `science-rt`'s `science_array_pop` was already written and in
            // `RUNTIME`. It needs `Array`'s `ScienceTypeInfo` descriptor to
            // be called, which this backend does not emit yet, so declaring
            // the name moves the refusal from the front end to codegen —
            // which is where the remaining work honestly is.
            Method {
                name: "pop",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[],
                ret: Some(Ty::Opt(&Ty::Var("T"))),
            },
            // `ffi-c-boundary.md` §1.3's `.span()`, which `SC0421`'s own note
            // names: *"a call site still passes the array — `&Array[T]`
            // coerces to `ffi.Span[T]` there, and `.span()` names the
            // conversion **where it has to be written out**"*.
            //
            // **Where it has to be written out is every position that is not
            // an argument.** `assign.rs`'s rule 8 is gated on
            // `Site::Argument`, on §1.3's own ergonomic argument — *"every
            // BLAS call has between two and four array arguments, and
            // `.span()` four times per call would be the most-typed token in
            // numerical Science code"* — so a `let`, a `return` and a record
            // field each still need the spelling. §1.7's `MatrixView(data: …,
            // rows: …, lda: …)` is the shape that needs it, and
            // `codegen-and-linking.md` §10 writes the *call* form out as
            // `LLVMBuildGEP2(builder, ty, base, indices.span(),
            // indices.len(), "")`.
            //
            // **The name of the mutable half is this file's and not a
            // note's.** No note spells it. `collections-and-chains.md` §4.3
            // forbids an abbreviation where a word exists and §5.4 already
            // spells the sibling accessors `values_mutably` and
            // `iterate_mutably`, which `get_mutably` above was decided by;
            // `span_mutably` is that rule applied once more, and it is named
            // here as a decision rather than as a transcription.
            //
            // **The receiver's mutability is the whole of the distinction.** A
            // `Span` holds a shared borrow and a `MutableSpan` an exclusive
            // one, so the two differ in exactly the way `get` and
            // `get_mutably` do and the region engine is asked the question it
            // already answers for those.
            Method {
                name: "span",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Ffi("Span", &[Ty::Var("T")])),
            },
            Method {
                name: "span_mutably",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[],
                ret: Some(Ty::Ffi("MutableSpan", &[Ty::Var("T")])),
            },
            // `collections-and-chains.md` §3.1: *"`length()` on a collection is
            // O(1), counting a stream consumes it"*. `Int` and not `U64`
            // because §4.6 of the core spec writes `text.length()` into an
            // integer context and Decision 2 defaults an integer literal to
            // `I64`; a `U64` length would make `length() - 1` a mixed-signedness
            // expression in the most-written loop in the language.
            Method { name: "length", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
            // `collections-and-chains.md` §5.4, transcribed: *"`Array of T`
            // has: `def iterate(self) -> ArrayIterate of T`"*. Its two
            // siblings, `iterate_mutably` and `iterate_consuming`, stay on
            // [`UNWRITTEN`]: §4.1 gives all three and the corpus writes only
            // this one, and a link with no lowering behind it is a name that
            // type-checks and then refuses.
            Method {
                name: "iterate",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::App("ArrayIterate", &[Ty::Var("T")])),
            },
            // **The rest of the Level 1 surface.** `stdlib-core.md` §3.6 gives
            // five signatures and `push`/`get` are the two of them that are
            // `Array`'s; the catalogue is `docs/DREAM.md` §13.4, which writes
            // an `Array[T] has:` block out in full, and every entry below is
            // transcribed from it unless its comment says otherwise. Each one
            // has a `science-rt` entry point and a row in
            // `science-codegen-llvm`'s tables, and `tests/arrays.rs` there runs
            // a program through it — the rule [`FUNCTIONS`]' comment states,
            // that a declared name with no lowering behind it is a name that
            // type-checks and then refuses.
            //
            // **§13.4's `where` clauses are dropped, as `Map.insert`'s is.**
            // [`Method`] has no `where_clause`, so `contains` and `index_of`'s
            // `where T: Eq` and `sort()`'s `where T: Ord` are not written; an
            // element type with no equality or order the backend can call is
            // refused there, by name, with the element type in the message.
            // The bound is the honest signature and the refusal is where it is
            // enforced until [`Method`] can carry one.
            //
            // §13.4 verbatim: an associated function like `new`, whose `T` is
            // fixed the same way — the instantiation written, or `SC0536`.
            Method {
                name: "with_capacity",
                generics: &[],
                recv: None,
                params: &[("capacity", INT)],
                ret: Some(Ty::App("Array", &[Ty::Var("T")])),
            },
            // §13.4 verbatim. `collections-and-chains.md` §5.3 names
            // `Array.reserve(n)` as what `collect()` feeds from
            // `estimated_length()`, and gives no signature; §13.4 does not
            // list it. `additional` is Rust's sense — room for that many
            // *more* — because that is what `science_array_reserve` already
            // does for the array literal, and the name says so.
            Method { name: "capacity", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method {
                name: "reserve",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("additional", INT)],
                ret: None,
            },
            // §13.4 verbatim: `get(0)` and `get(length() - 1)`, with `get`'s
            // shape and its reason.
            Method {
                name: "first",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("T")))),
            },
            Method {
                name: "last",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("T")))),
            },
            // **Deviation: `insert` returns nothing and panics out of range,
            // where §13.4 writes `-> Error?`.** Two reasons. `stdlib-core.md`
            // §7.3 makes every Level 1 function return its *concrete* error
            // type, and §7.2 gives three — none of which is about an index,
            // so `Error?` here would be the interface §7.3 forbids and any
            // concrete type would be invented. And an index past the end is
            // the bracket form's case, not `get`'s: the caller asserts the
            // position, as `xs[i] be v` does, so it fails the way `xs[i]`
            // does — a panic naming the index and the length. `index` may be
            // `length()`, which appends.
            Method {
                name: "insert",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("index", INT), ("value", Ty::Var("T"))],
                ret: None,
            },
            // §13.4 verbatim: `T?`, `pop`'s answer one position along, and
            // `null` for an index that names no element rather than a panic —
            // the same choice `get` makes. The tail shifts down one.
            Method {
                name: "remove",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("index", INT)],
                ret: Some(Ty::Opt(&Ty::Var("T"))),
            },
            // **Decided here: `swap` and `replace` are not in §13.4.** They
            // are the two primitives that move an element *without* shifting
            // the others, and without them an element in the middle of an
            // array cannot be taken out at all: `xs[i]` is a borrow, `pop`
            // reaches only the end, and `remove` shifts. The `collections`
            // module's own header records that it could not write a ring
            // buffer for exactly that reason. `replace(index, value) -> T` is
            // the move-out primitive — the element comes back owned and
            // `value` takes its slot — and over an `Array of T?` it is
            // `take`: `slots.replace(i, null)`. Both panic out of range, for
            // `insert`'s reason: the caller names the position.
            Method {
                name: "swap",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("first", INT), ("second", INT)],
                ret: None,
            },
            Method {
                name: "replace",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("index", INT), ("value", Ty::Var("T"))],
                ret: Some(Ty::Var("T")),
            },
            // §13.4 verbatim. Both keep the buffer and drop what they cut, in
            // index order; `truncate` is total as `String.truncate` is
            // (`stdlib-core.md` §6.5) — past the end changes nothing and a
            // negative length empties. The parameter is `length`, the
            // array's own word for what is being set, where §13.4 writes `n`.
            Method { name: "clear", generics: &[], recv: Some(SelfKind::Mutable), params: &[], ret: None },
            Method {
                name: "truncate",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("length", INT)],
                ret: None,
            },
            // §13.4 verbatim: `other` is moved in and its elements with it,
            // so nothing is cloned and no bound is needed.
            Method {
                name: "extend",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("other", Ty::App("Array", &[Ty::Var("T")]))],
                ret: None,
            },
            // §13.4 verbatim.
            Method { name: "reverse", generics: &[], recv: Some(SelfKind::Mutable), params: &[], ret: None },
            // An independent copy, `-> Array[T]`. **The bound `where T: Clone`
            // is not written** ([`Method`] has no `where_clause`, and
            // [`IMPLEMENTS`] cannot say `Array[T]: Clone` conditionally), and
            // what stands in for it is the backend: it copies the element
            // bytes, which is a duplicate only for an element that owns
            // nothing, so it refuses an element with drop glue by name.
            Method {
                name: "clone",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::App("Array", &[Ty::Var("T")])),
            },
            // §13.4 verbatim, less `where T: Eq` (see above). Equality is
            // `Set.contains`'s: the backend passes the `(T, ())` map descriptor
            // a `Set of T` already uses, so the element types are the ones a
            // `Set` admits — `String` and the eight-byte integers.
            Method {
                name: "contains",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("value", Ty::Ref(&Ty::Var("T")))],
                ret: Some(BOOL),
            },
            Method {
                name: "index_of",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("value", Ty::Ref(&Ty::Var("T")))],
                ret: Some(Ty::Opt(&INT)),
            },
            // **Two `sort`s, told apart by the label**:
            // `collections-and-chains.md` §3.3's AMENDMENT 2, *"a method's
            // argument labels are part of its name"*, which §3.3 adopts
            // precisely to keep the core spec §4.6's `docs.sort(by: doc giving
            // …)` beside a bare `sort()`. §13.4 spells the second `sort_by(less:
            // (&T, &T) -> Bool)`; §3.3 refuses *"a litter of `_by` suffixes"*
            // and the core spec writes `sort(by:)`, so the label wins.
            // `science-types`' `check` picks between the two by label, for
            // prelude methods only (its `labelled` says why).
            //
            // `sort()` is §13.4's, less `where T: Ord`: `Ord` declares no
            // method yet, so there is no comparison a generic body could call
            // and the backend picks an entry point per element type — `Int`
            // and `String` (byte order, `stdlib-core.md` §6.8) — refusing the
            // rest by name. `sort(by:)` is the chain's `sorted(by:)` on an
            // array in place: the key is an `Int`, computed once per element,
            // and the sort is stable, both for `science_array_sort_by_int_key`'s
            // stated reasons.
            Method { name: "sort", generics: &[], recv: Some(SelfKind::Mutable), params: &[], ret: None },
            Method {
                name: "sort",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("by", Ty::Fn(&[Ty::Ref(&Ty::Var("T"))], &INT))],
                ret: None,
            },
        ],
    },
    // --- Map, §3.6 --------------------------------------------------------
    Block {
        ty: "Map",
        generics: &["K", "V"],
        interface: None,
        assoc: &[],
        methods: &[
            // `Array.new`'s decision, one arity up, with the same citation
            // (none) and the same cost. `examples/09` and `examples/12` write
            // `(Map[String, String]).new()`, which is the form that
            // compiles; a bare `Map.new()` is `SC0536` naming both `K` and `V`.
            Method {
                name: "new",
                generics: &[],
                recv: None,
                params: &[],
                ret: Some(Ty::App("Map", &[Ty::Var("K"), Ty::Var("V")])),
            },
            // §3.6, minus its `where K: Eq + Hash`. `Hash` is not one of the
            // prelude's interfaces, so writing the bound would name something
            // that does not resolve; and a bound that cannot be checked at a
            // call is a bound that costs a diagnostic and buys nothing. Named
            // in the report as the one clause of §3.6 dropped. The key is held
            // to it structurally instead, by `science-types`' `keys` pass
            // (`SC0548`), over the type the map ends up with.
            Method {
                name: "insert",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("key", Ty::Var("K")), ("value", Ty::Var("V"))],
                ret: Some(Ty::Opt(&Ty::Var("V"))),
            },
            // **Decided here, not read out of a note.** §3.6 gives `Array.get`
            // and not `Map.get`; `examples/09` writes *"`Map.get` hands back
            // `(&V)?` for the same reason"* and `science-regions`'
            // `generate` §7 says *"the real `Map.get` borrows only the map"*.
            // Two decisions are being taken:
            //
            // 1. **The value comes back borrowed, not owned.** `-> V?` would
            //    force a copy of every value read out of a map, which for a
            //    `Map[String, Array[F64]]` is the whole array. The cost
            //    is that a caller who wants an owned value writes `.clone()`,
            //    and that `Map` cannot later be given a `get` that returns by
            //    value under the same name.
            // 2. **The key is taken by borrow.** `key: K` would move the key
            //    into the call, so `settings.get(key)` inside a loop would
            //    consume `key` on the first iteration. The cost is that a
            //    caller holding an owned key relies on §6.3's auto-borrow, and
            //    that a `Map[Int, _]` borrows a number to look it up.
            Method {
                name: "get",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(Ty::Opt(&Ty::Ref(&Ty::Var("V")))),
            },
            // §3.4: membership is `contains` on both `Map` and `Set`, because
            // `has` is a keyword and *"between two names that read equally
            // well, the one that compiles wins"*.
            Method {
                name: "contains",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(BOOL),
            },
            // Decided: `remove` hands back what it displaced, matching
            // `insert`. The alternative — `-> ()` — makes *"take it out and use
            // it"* two lookups.
            Method {
                name: "remove",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("key", Ty::Ref(&Ty::Var("K")))],
                ret: Some(Ty::Opt(&Ty::Var("V"))),
            },
            Method { name: "length", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
            // §5.4's `Map` sources, `Array.iterate`'s way: the receiver is
            // shared and the chain's own type names the source. `for k in
            // m.keys():` and `m.keys().collect()` both run, in insertion
            // order (§5.2); `values_mutably` and the other two `iterate*`
            // stay in [`UNWRITTEN`].
            Method {
                name: "iterate",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::App("MapIterate", &[Ty::Var("K"), Ty::Var("V")])),
            },
            Method {
                name: "keys",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::App("MapKeys", &[Ty::Var("K"), Ty::Var("V")])),
            },
            Method {
                name: "values",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::App("MapValues", &[Ty::Var("K"), Ty::Var("V")])),
            },
        ],
    },
    // --- Set, `collections-and-chains.md` §5.1 ----------------------------
    //
    // Five of AMENDMENT 10's nine, and the four left out are left out for
    // four different reasons: `union` is a reserved word (`stdlib-core.md`
    // §3.3's hazard, unresolved); `intersection` and `difference` return a
    // `Set` built from two, which is a loop this compiler would have to emit
    // over a table it cannot iterate; `Set.from` needs §5.5's `From` over a
    // chain. `for x in set:` is supported — see `Set implements Iterate`
    // below — and `iterate()` itself is not declared, for `Map`'s reason in
    // [`UNWRITTEN`]. `has` is spelled `contains` per `stdlib-core.md` §3.4.
    //
    // `T: Eq + Hash` is dropped for `Map.insert`'s reason: `Hash` is not a
    // prelude interface. What stands in for it is `science-types`' `keys`
    // pass, which refuses an element that cannot be hashed structurally as
    // `SC0548` — `collections-and-chains.md` §5.2's AMENDMENT 8a.
    Block {
        ty: "Set",
        generics: &["T"],
        interface: None,
        assoc: &[],
        methods: &[
            // `Map.new`'s decision, one parameter down.
            Method {
                name: "new",
                generics: &[],
                recv: None,
                params: &[],
                ret: Some(Ty::App("Set", &[Ty::Var("T")])),
            },
            // **Decided: `-> Bool`, and `true` means the value was new.** No
            // note gives the return. `()` was the alternative and costs the
            // one question a work-queue asks — `if seen.insert(node):` is
            // *"visit it the first time only"* in one line instead of a
            // `contains` and an `insert`, two lookups. The polarity is Rust's
            // `HashSet::insert` and the opposite of `Map.insert`'s *displaced*
            // — which is why a set does not return `()?`, a type whose only
            // information would be that inverted bit.
            Method {
                name: "insert",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Var("T"))],
                ret: Some(BOOL),
            },
            // `Map.contains`, and the value is borrowed for `Map.get`'s
            // reason: a probe in a loop must not consume the probe.
            Method {
                name: "contains",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("value", Ty::Ref(&Ty::Var("T")))],
                ret: Some(BOOL),
            },
            // `true` when it was there. `insert`'s answer, mirrored.
            Method {
                name: "remove",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Ref(&Ty::Var("T")))],
                ret: Some(BOOL),
            },
            Method { name: "length", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
        ],
    },
    // --- Box --------------------------------------------------------------
    //
    // **`new` is decided here, no note declares it, and it is the one
    // declaration in this file that the corpus now disagrees with.**
    //
    // `def new(value: T) -> Box[T]` is the only signature `Box.new` can have:
    // it takes the value, it moves it to the heap, and what comes back is a
    // `Box` of the value's type. Nothing else is expressible and nothing else
    // would be true.
    //
    // **What disagrees is not the signature — it is a coercion two sentences of
    // `science-types`' `assign` do not agree about.** Six corpus sites write
    // `Box.new(Doc(..))` where a `Box of any Summarize` is declared. With this
    // signature the call is `Box[Doc]`, and `Box[Doc]` reaching `Box of any
    // Summarize` is an *unsizing under a type constructor*, which `assign`'s §4
    // lists first among *"three things it deliberately does not reach"* — while
    // §5 of the same file says an owned `any Summarize` *"is constructed where
    // it is written — `Box.new(doc)` — and the corpus already writes every one
    // of them that way"*. §4 also closes its list with *"none of which the
    // corpus writes"*, and that clause is now false.
    //
    // **The declaration stays**, for the reason a wrong signature would not:
    // this one is right, and withdrawing it would put all ten calls back to
    // `Ty::ERROR` — including the two, in `18_ownership` and `19_stdlib`, that
    // check clean against it today. The gap is a missing coercion and a
    // missing coercion is not a reason to delete a correct declaration.
    // `science-types/tests/corpus.rs` pins the six and carries the three ways
    // they close; all three are `type-checking-and-mir.md` §6.2's, not this
    // file's.
    Block {
        ty: "Box",
        generics: &["T"],
        interface: None,
        assoc: &[],
        methods: &[Method {
            name: "new",
            generics: &[],
            recv: None,
            params: &[("value", Ty::Var("T"))],
            ret: Some(Ty::App("Box", &[Ty::Var("T")])),
        }],
    },
    // --- String, §6.9 -----------------------------------------------------
    //
    // All nineteen of the note's methods, `characters` spelled `chars` (its
    // row says why). `new` is the one of them that a note declares in full:
    // §6.9's first line is `def new() -> String`, so it is transcription and
    // not a decision.
    //
    // `lines` and `bytes` joined for Gate C1: a file split into lines, and a
    // dump written through `Write.write(bytes: &Array[U8])`. `from_bytes`
    // joined when the bundled `os` module called it, and is declared at the
    // end of the block. `trim`, `slice` and `split` were the last three, and
    // two of them are **not** §6.9's signature.
    //
    // # Decision: `trim` and `slice` return an owned `String`
    //
    // §6.9 writes `def trim(self) -> borrowed String` and `def slice(self,
    // bytes: Range of Int) -> (borrowed String, TextError?)`. Here they are
    // `-> String` and `-> (String, TextError?)`: the bytes are copied.
    //
    // # Reason
    //
    // A `String` is `science-rt`'s `{ ptr, len, cap }` and a `borrowed
    // String` is a pointer to such a header. A trimmed or sliced string is
    // the middle of another string's buffer, and has no header of its own for
    // that pointer to reach — the reason `Lines` already hands out owned
    // lines. The three ways out were measured against what each costs:
    //
    // - **A string-view type** — a `{ ptr, len }` that reads as a `String`
    //   wherever a `&String` is accepted, and borrows the receiver. The
    //   faithful one, and a type-system change: every `&String` parameter in
    //   the prelude and the runtime would have to accept two representations,
    //   or the view would need a coercion at every call, and `science-regions`
    //   places a region only at `TyKind::Borrowed`, so the view's borrow of
    //   its receiver would be invisible to it unless the view *is* a
    //   `borrowed` type. That is a design note's worth of decisions, not a
    //   method's.
    // - **A header in a caller-owned slot** — `{ ptr + start, len, 0 }`
    //   written to a temporary and a pointer to it returned. Unsound as soon
    //   as the result is returned from the function that owns the slot:
    //   `def first(s: borrowed String) -> borrowed String: s.trim()` passes
    //   the region checker, which sees a borrow of `s`, and hands back the
    //   address of a dead frame.
    // - **An owned copy** — always sound, and needs nothing new from the type
    //   checker or the region checker.
    //
    // # Cost
    //
    // One allocation per call, and a program that writes `let field be
    // line.trim()` owns `field` rather than borrowing `line` — so it may
    // outlive `line`, which §6.9's signature would forbid. Every use §6.11
    // shows (`field.is_empty()`, `field.starts_with("#")`,
    // `field.parse_float()`) reads the same under both. Moving to the view
    // later narrows what programs may do with the result, so it is a
    // breaking change to these two signatures, and whoever writes the view's
    // design note decides it.
    Block {
        ty: "String",
        generics: &[],
        interface: None,
        assoc: &[],
        methods: &[
            Method { name: "new", generics: &[], recv: None, params: &[], ret: Some(STRING) },
            // §6.5: **bytes**, and the note states the cost — `"héllo".length()`
            // is 6 — as documentation debt with no compiler mitigation.
            Method { name: "length", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "is_empty", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(BOOL) },
            Method {
                name: "push_str",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("tail", Ty::Ref(&STRING))],
                ret: None,
            },
            // **`truncate` mutates in place and returns nothing**, which is
            // §6.9's signature and not the one eighteen corpus call sites
            // read it as. Those sites wrote `self.body.truncate(200)` as an
            // expression producing a `String`; the note says `(mutable self,
            // bytes: Int)`. The corpus was wrong and is corrected, which is
            // the same direction this file took for `Array.get`: a note's
            // signature is the specification and a call site is a use of it.
            //
            // `science-rt`'s `science_string_truncate` was already written
            // and already in `RUNTIME`, reachable by nothing. Declaring the
            // name is the whole of what was missing.
            Method {
                name: "truncate",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("bytes", INT)],
                ret: None,
            },
            Method {
                name: "starts_with",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("prefix", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "ends_with",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("suffix", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "contains",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("needle", Ty::Ref(&STRING))],
                ret: Some(BOOL),
            },
            Method {
                name: "find",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("needle", Ty::Ref(&STRING))],
                ret: Some(Ty::Opt(&INT)),
            },
            // §6.9: ASCII whitespace only — space, tab, CR, LF, FF, VT —
            // pinned forever. Owned, not borrowed: the block's Decision.
            Method {
                name: "trim",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(STRING),
            },
            // §6.4's byte-range view, owned for the same Decision. It fails
            // with `TextError` when either end is inside a character, and
            // when the range is not within the string — the second is
            // `science-rt`'s `science_string_slice`'s to argue.
            Method {
                name: "slice",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("bytes", Ty::App("Range", &[INT]))],
                ret: Some(Ty::Pair(&STRING, &Ty::Opt(&Ty::Name("TextError")))),
            },
            // §6.9's `def split(self, separator: borrowed String) -> Split`,
            // transcribed. What a piece is — every occurrence separates and
            // empty pieces are kept — is `science_split_next`'s to decide.
            Method {
                name: "split",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("separator", Ty::Ref(&STRING))],
                ret: Some(Ty::Name("Split")),
            },
            Method {
                name: "replace",
                generics: &[],
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
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Name("Chars")),
            },
            // §6.9's `def bytes(self) -> borrowed Array of U8`, transcribed.
            // No entry point: `science-codegen-llvm`'s `string_bytes` says
            // why a string's header already is the array this borrows.
            Method {
                name: "bytes",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Ref(&Ty::App("Array", &[Ty::Name("U8")]))),
            },
            // §6.9's `def lines(self) -> Lines`, transcribed. It borrows the
            // string exactly as `chars` does; what it hands out is decided at
            // the `Lines` block below.
            Method {
                name: "lines",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Name("Lines")),
            },
            // §6.9: Level 1 because *"correctly-rounded decimal parsing has a
            // unique answer"* and because `read_lines` is useless without them.
            Method {
                name: "parse_int",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Pair(&I64, &Ty::Opt(&Ty::Name("TextError")))),
            },
            Method {
                name: "parse_float",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Pair(&F64, &Ty::Opt(&Ty::Name("TextError")))),
            },
            // §6.9's `def from_bytes(bytes: borrowed Array of U8) -> (String,
            // TextError?)`, transcribed. It waited, as the note above this
            // block said, for a program that calls it: the bundled `os`
            // module is that program. A string leaving the runtime crosses
            // the `extern` boundary as bytes copied into a Science-owned
            // `Array of U8`, and this is the one door from there to a
            // `String` that does not trust the bytes.
            Method {
                name: "from_bytes",
                generics: &[],
                recv: None,
                params: &[("bytes", Ty::Ref(&Ty::App("Array", &[Ty::Name("U8")])))],
                ret: Some(Ty::Pair(&STRING, &Ty::Opt(&Ty::Name("TextError")))),
            },
        ],
    },
    // --- `Array[T] implements Iterate`, `collections-and-chains.md` §4 ---
    //
    // **`Item` is `&T`, and that is the note's decision rather than
    // this file's.** §4.2's AMENDMENT 11 makes `for x in xs:` desugar to
    // `xs.iterate()`, §4.1 gives the three sources — `iterate()`,
    // `iterate_mutably()`, `iterate_consuming()` — with `Item = &Doc`,
    // `&mut Doc` and `Doc` respectively, and §4.3 says
    // `iterate()` yields `&Item` *"uniformly — no conditional
    // associated type, no specialisation"*. §5's `owned()` is then declared
    // `where Self.Item is &T, T: Clone`, which is a clause that means
    // nothing unless `Item` is already a borrow.
    //
    // **The alternative is ruinous for this audience.** `Item is T` copies
    // every element of every `for` loop in the language; for an `Array of
    // (Array[F64])` that is a heap allocation per row per iteration, which is
    // the normal case rather than the pathological one. The word "consuming"
    // exists precisely so that the copying form has somewhere to be written.
    //
    // **What it costs** is that `for n in numbers:` binds `n` at `borrowed
    // Int`, so every arithmetic operator in every loop body meets a borrow.
    // `assign`'s §7 is what makes that legal — a borrow of a `Copy` type reads
    // as a value — and the two decisions are load-bearing for each other:
    // landing this one without that one makes `total be total + n` a type
    // error in the most-written loop in the language.
    //
    // **This block declares `Item` and no `next`.** `Methods`' §2 contributes
    // the interface's own `def next(mutable self) -> Self.Item?` to a block
    // that did not write it, so the lookup finds one, and `check`'s
    // `iterate_item` reads the element type out of that signature after this
    // block's `Item` is substituted into it. A `next` written here would be a
    // second declaration of the same method with a body neither of them has.
    //
    // `Map` and `Set` are the two blocks after this one. `Map` used to be
    // absent from here with a paragraph saying why — its item is §1.3's
    // `Entry[K, V]`, a record this table could not give fields to — and
    // [`Declarer::entry`] is what closed that.
    Block {
        ty: "Array",
        generics: &["T"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::Var("T")))],
        methods: &[],
    },
    // --- `Map[K, V] implements Iterate`, `collections-and-chains.md` §5.2 ---
    //
    // **Decision: `Item is &Entry[K, V]` — one borrow of a record that
    // exists, not a record of two borrows.**
    //
    // The reason is three sentences of the note read together. §5.4: *"`Map`
    // yields `Entry of (K, V)`"*. §1.3: *"pairs are records, never tuples"*,
    // because `each.key` works with the implicit subject and `each.0` does
    // not — which rules out `(&K, &V)`. §4.3: `iterate()` yields a borrow
    // *"uniformly — no conditional associated type"*, which is `Array`'s
    // `&T` above and makes the item `&Entry[K, V]` rather than
    // `Entry[K, V]` (a copy of every key and value on every turn, the
    // allocation per row per iteration `Array`'s own comment refuses).
    //
    // **The alternative that was measured against it** is
    // `Entry[&K, &V]`, a record holding two borrows by value, which is
    // what a table with separate key and value arrays could hand out. It is
    // refused because `science-regions`' `regions.rs` §2 does not descend into
    // generic arguments — a borrow *inside* `Entry`'s `K` is invisible to it
    // — so `let kept be entry` would carry a borrow of the map that nothing
    // checks. A top-level `&` is exactly what that engine does see. The price
    // was paid in `science-rt` instead: `map.rs` now stores each pair as an
    // `Entry[K, V]` in insertion order (AMENDMENT 9's *"one index array"*),
    // so the borrow has something to point at.
    //
    // **`for (key, value) in counts:` is not a form**, and nothing is lost by
    // it: `for entry in counts:` then `entry.key` / `entry.value` is §1.3's
    // own spelling, and a destructuring pattern in a `for` header is
    // something no note specifies.
    //
    // Method-less for `Array`'s reason: `Iterate`'s own `next` is what the
    // lookup finds, and `science-mir`'s `lower_for_over_map` is what runs,
    // because a map, like an array, has nowhere to keep a cursor.
    Block {
        ty: "Map",
        generics: &["K", "V"],
        interface: Some(("Iterate", &[])),
        assoc: &[(
            "Item",
            Ty::Ref(&Ty::App("Entry", &[Ty::Var("K"), Ty::Var("V")])),
        )],
        methods: &[],
    },
    // --- `Set[T] implements Iterate`, §5.4: *"`Set` yields its elements"* ---
    //
    // `Item is &T`, `Array`'s item: a set's entry is its element (the value is
    // zero-sized), so the borrow points straight at it.
    Block {
        ty: "Set",
        generics: &["T"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::Var("T")))],
        methods: &[],
    },
    // --- `Array[T] implements Index[Int]`, §1.1 ------------------------
    //
    // **`Idx` is `Int`, and it is read off `Array.get` rather than decided
    // here.** §3.6 declares `def get(self, index: Int) -> (&T)?` and
    // §1.3's canonical discharged index is `a[i]` inside
    // `for i in 0..a.length():`, whose bound is `length() -> Int`. So both the
    // sibling accessor and the loop the note writes hand an `Int`, and the two
    // spellings of one operation agree.
    //
    // **The cost, stated, and it is inherited rather than introduced.** `Int`
    // and `I64` are separate primitives in this prelude, so `xs[i]` where `i`
    // is an `I64` is `SC0525` — exactly as `xs.get(i)` already is today. The
    // disagreement is `stdlib-core.md` §3.6's against Decision 2's integer
    // literal default, it predates this declaration, and choosing `I64` here to
    // dodge it would make `a[i]` and `a.get(i)` want different index types,
    // which is the worse half of the same wart. Named in the report as the
    // thing to reconcile.
    //
    // **`Output is T` and not `&T`**, which is where this differs from
    // `Iterate.Item` above. The borrow is in the *method's return* — `->
    // &Self.Output` — so `Output` is the element and the reference is
    // the interface's, not the associated type's. Writing `&T` here
    // would make `a[i]` a `&&T`.
    //
    // **`Map` is deliberately absent**, and it is the finding `Map`'s
    // `Iterate` used to share before `Entry` gave it one: `Map[K, V]
    // implements Index[K]`
    // would be an indexing operation that panics on a key that is not there,
    // and §1.3's four discharge rules are all about an *extent* — none of them
    // can speak about a key. §1.1 never names `Map`, so declaring it would be
    // deciding what `m[k]` does on a miss in a file nobody reads. `Map.get`
    // returns `(&V)?` and says so.
    Block {
        ty: "Array",
        generics: &["T"],
        interface: Some(("Index", &[INT])),
        assoc: &[("Output", Ty::Var("T"))],
        methods: &[],
    },
    Block {
        ty: "Array",
        generics: &["T"],
        interface: Some(("IndexMutably", &[INT])),
        assoc: &[("Output", Ty::Var("T"))],
        methods: &[],
    },
    // --- Chars ------------------------------------------------------------
    //
    // The one iterator §8 hands out by name. `Item is Char` is what
    // `examples/10`'s `for c in text.chars()` reads its binding out of.
    Block {
        ty: "Chars",
        generics: &[],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", CHAR)],
        // **The one `Iterate` implementation that declares its `next`, because
        // it is the one that has a body.**
        //
        // The decision. `Chars` restates `Iterate.next` in its own block.
        //
        // The reason. Every other `implements Iterate` block here declares
        // `methods: &[]`, so `next` resolves to the *interface's* declaration —
        // which has no body anywhere and needs one copy per implementor, i.e.
        // monomorphisation. `Chars` is different: `science_chars_next(iter,
        // out) -> Bool` exists, and its own documentation says it is *"the
        // owned-`T?` convention … §5.3"*, which is the convention `Map.insert`
        // now goes through. Declaring the method here is what gives the call an
        // owner that is a *type* rather than an interface, and that is what
        // `science-codegen-llvm`'s `prelude_method` keys on.
        //
        // The cost. The signature is written twice — here and on `Iterate` —
        // and `SC0541` is what catches them drifting apart, which is the same
        // guard every user `implements` block already relies on.
        methods: &[Method {
            name: "next",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&CHAR)),
        }],
    },
    // --- `IoError implements Error` ----------------------------------------
    //
    // **Declared here with its method, and moved out of [`IMPLEMENTS`], because
    // it has a body.** A method-less `implements Error` row made `err.message()`
    // on an `IoError` resolve to the *interface's* `message`, which has no body
    // anywhere; `science-codegen-llvm` then lowered it as a dispatch through a
    // vtable the one-byte error code does not have. Restating `message` here
    // gives the call a type for an owner, which is what `prelude_method` keys
    // on, and `science_io_error_message` is the body.
    Block {
        ty: "IoError",
        generics: &[],
        interface: Some(("Error", &[])),
        assoc: &[],
        methods: &[Method {
            name: "message",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(STRING),
        }],
    },
    // --- `TextError implements Error` --------------------------------------
    //
    // `IoError`'s block above, for `IoError`'s reason, now that something
    // constructs one: `String.parse_int` and `String.parse_float` lower to
    // `science_string_parse_int` and `science_string_parse_float`, and
    // `science_text_error_message` is this method's body. Until then the row
    // in [`IMPLEMENTS`] was enough, because a call to `message` on a
    // `TextError` could only be reached by a program the backend refused
    // earlier, for the type.
    Block {
        ty: "TextError",
        generics: &[],
        interface: Some(("Error", &[])),
        assoc: &[],
        methods: &[Method {
            name: "message",
            generics: &[],
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(STRING),
        }],
    },
    // --- Lines ------------------------------------------------------------
    //
    // **`Item is String`, owned, and that is a decision no note takes.**
    // §4.5 names `Lines` and gives it two sources, `String.lines()` and
    // `File.lines()`, and says nothing about what a line is. A `&String` item
    // was the other candidate and is not available: a `String` is `{ ptr, len,
    // cap }` and a line in the middle of one has no `String` of its own for a
    // borrow to point at — which is also why `trim` and `slice` above return
    // owned copies, against §6.9. And the file source settles it
    // regardless: a line read from a `File` has no buffer to borrow from, and
    // one `Lines` with one `Item` is what lets §4.5 give both sources one type.
    //
    // The cost is an allocation per line, which is `read_line`'s cost in §4.4
    // and is stated there.
    //
    // `next` restated for `Chars`' reason: it has a body, `science_lines_next`,
    // and declaring it here is what gives the call a type for an owner.
    Block {
        ty: "Lines",
        generics: &[],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", STRING)],
        methods: &[Method {
            name: "next",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&STRING)),
        }],
    },
    // --- Split ------------------------------------------------------------
    //
    // `String.split`'s iterator, `Lines`' shape and `Lines`' reason for an
    // owned `Item`: a piece in the middle of a string has no header for a
    // borrow to point at. **Unlike `Lines` it borrows nothing at all** — it
    // owns copies of the text and the separator, so it has drop glue —
    // because nothing in the type `Split` tells `science-regions` that it
    // borrows, and `line.split(",")`'s separator is a temporary that dies at
    // the end of its statement. `science-rt`'s `ScienceSplit` has the
    // argument in full.
    Block {
        ty: "Split",
        generics: &[],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", STRING)],
        methods: &[Method {
            name: "next",
            generics: &[],
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&STRING)),
        }],
    },
    // --- `Range[T] implements Iterate`, AMENDMENT 14 --------------------
    //
    // **`Item is T` and not `&T`, which is where this differs from
    // `Array` twenty lines up.** A range holds two ends and *computes* each
    // element; there is no element in memory for a borrow to point at, and
    // `Iterate.next` returning `(&Self.Item)?` over a value the call
    // just produced is a borrow of a temporary. `Array`'s borrow is there to
    // stop `for row in matrix:` copying a row per iteration; the cost this one
    // declines is a copy of an integer, which is the thing a register holds.
    //
    // **`Range` is generic over its element, and the one argument against that
    // is worth stating.** Every range in `examples/` is over `Int` — `0..width`
    // with `width: Int`, `first..last` with both `Int`, and bare literals — so
    // a non-generic `Range` with `Item is Int` would carry the whole corpus.
    // It would also make `0..5` an `Int` range, and Decision 2 defaults an
    // unsuffixed integer literal to `I64`, so `let mutable sum be 0` beside
    // `for i in 0..5:` would be an `I64` meeting an `Int` at the first `+`.
    // Generic is the form that leaves Decision 2 alone, and it is the form
    // `data-io.md` §2 already writes: `def slice(self, range: Range[U64])`.
    //
    // **The parameter is unbounded**, which is deliberate and is the one thing
    // this declaration does not say. `Range[String]` is unusable rather than
    // unwritable — `..` over two strings types, and the loop over it yields a
    // `String` nothing can produce — because the bound that would refuse it is
    // `T: Step`, a `stdlib-core.md` interface no note has written. The refusal
    // that exists today is `science-types`' `check`: the two ends must be one
    // type, which is what a range written wrong actually gets wrong.
    Block {
        ty: "Range",
        generics: &["T"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Var("T"))],
        methods: &[],
    },
    // --- `Formatter has:`, `strings-formatting-and-docs.md` §3.1 ----------
    //
    // **The one block here whose method set the note calls closed**, and it is
    // transcribed with all five rather than with the four that are wired.
    // §3.1 writes the block out in Science, doc comments and all:
    //
    // ```text
    // Formatter has:
    //     def text(mutable self, value: borrowed String)
    //     def raw(mutable self, value: borrowed String)
    //     def number(mutable self, value: F64)
    //     def integer(mutable self, value: I64)
    //     def spec(self) -> FormatSpec
    // ```
    //
    // `borrowed String` is `&String` in the syntax this parser accepts, which
    // is the same translation [`INTERFACE_DECLS`]' `Display` entry performs on
    // `mutable borrowed Formatter`; nothing else moves. The header comment on
    // [`BLOCKS`] applies unchanged — the block is `open`, because
    // `Def::is_builtin` is what decides that and not a flag here — although
    // this is the one prelude block for which "the set that exists is larger
    // than the set written down" is *false* today: §3.1 says five and five are
    // here.
    //
    // **Four of the five reach a runtime symbol and `spec` does not**, which
    // is a `science-codegen-llvm` gap and not a declaration one.
    // `PRELUDE_METHODS` there has rows for `text`, `raw`, `number` and
    // `integer`; `spec` returns a `FormatSpec` by value through the `sret`
    // convention, which that table's one-call-one-symbol shape does carry —
    // `science_string_new` is the same shape — but nothing in the corpus calls
    // it, and the rule that table states is *"a row is added when a program
    // that runs it is added with it."* Declaring the name here and leaving the
    // row out is the documented arrangement: the symptom is a refusal naming
    // `spec`, not a wrong rendering.
    Block {
        ty: "Formatter",
        generics: &[],
        interface: None,
        assoc: &[],
        methods: &[
            Method {
                name: "text",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Ref(&STRING))],
                ret: None,
            },
            Method {
                name: "raw",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", Ty::Ref(&STRING))],
                ret: None,
            },
            Method {
                name: "number",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", F64)],
                ret: None,
            },
            Method {
                name: "integer",
                generics: &[],
                recv: Some(SelfKind::Mutable),
                params: &[("value", I64)],
                ret: None,
            },
            Method {
                name: "spec",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[],
                ret: Some(Ty::Name("FormatSpec")),
            },
        ],
    },

    // --- the chain vocabulary -------------------------------------------
    //
    // `collections-and-chains.md` §1.4's closed set, narrowed to the thirteen
    // links below: the source `iterate()`; the adapters `discard`, `keep`,
    // `map`, `take`, `skip` and `sorted(by:)`; and the terminals `collect()`,
    // `count()`, `sum()`, `has_any`, `has_all`, `first()` and `find`. The
    // first six were the ones `examples/00_kitchen_sink.science` writes;
    // `first()` is the one `examples/16_indentation.science` writes; the rest
    // are the group every pipeline reaches for next — the positive filter,
    // the other half of `take`, and the terminals that answer with a scalar
    // instead of an `Array`. Nothing else from that table is declared, under
    // §10's own rule that a name is transcribed when a program that runs it
    // arrives with it — `chains.rs` in `science-codegen-llvm` is that program
    // for every name here.
    //
    // # The second tranche: twenty-one names, and what each one narrows
    //
    // `numbered`, `take_while`, `skip_while`, `every`, `zip` and the `Map`
    // sources `iterate`, `keys` and `values` arrive with `last`, `minimum` and
    // `maximum`; `chains.rs` runs every one. Four narrowings, each the first
    // tranche's own argument applied again:
    //
    // - **`minimum` and `maximum` take `by:` and nothing else**, `sorted`'s
    //   narrowing for `sorted`'s reason: there is no `Ord` to compare a bare
    //   item by, and a name cannot be declared with and without its argument.
    // - **`every(n)` is §1.4's `step_by`**, under the name the note gives it.
    // - **`zip`'s `other` is an `ArrayIterate[U]`**, where the note writes
    //   `O: Iterate` and an item `O.Item` this prelude cannot spell. It is the
    //   one second source a fused loop can walk beside its own, and the
    //   yielded `Pair[Self.Item, &U]` is §1.3's, borrow and all.
    // - **`reduce` was not declared** until the two-parameter closure
    //   (`(acc, x) giving …`, AMENDMENT 3) parsed. It does now, and `reduce`,
    //   `accumulate` and `product` are declared with `keep_some` and
    //   `reverse` (the third tranche). `keep_some`'s result type is peeled by
    //   `science-types`' `chain_item_shape` and `accumulate` is held there to
    //   a state that owns nothing (it yields a copy per item); `reverse` is a
    //   barrier, lowered with `sorted(by:)` by `science-mir`'s
    //   `lower_chain_over`, and anything may follow either barrier.
    //
    // **Every chain type also `implements Iterate`**, at the end of this
    // section: that is what gives `for x in xs.iterate().keep(…):` an element
    // type, and `science-mir` fuses the loop body into the chain as it fuses
    // a terminal.

    // # The four places this departs from the note, each priced
    //
    // 1. **The combinators are inherent methods on each adapter, not provided
    //    methods on `Iterate`.** §1.1 wants the second — *"Users implement
    //    `next`; they never implement a combinator"* — and it is the right
    //    shape. It is not available here: [`InterfaceDecl`] carries no bodies,
    //    so a method declared on `Iterate` is a **required** method, and every
    //    `implements Iterate:` in the corpus would stop compiling the day it
    //    landed. `examples/00_kitchen_sink.science`'s own `Countdown` is one.
    //    The note's §7 point 1 is what this costs: F2's `Divide` will have to
    //    repeat the list rather than inherit it. Closing it needs defaulted
    //    methods in the prelude, which is a change to this file's vocabulary
    //    and not to this table.
    //
    // 2. **An adapter carries its source and its `Item`, not its source and
    //    its closure.** §1.4 writes `MapOver of (Self, F)`. `F` is there so
    //    the lazy struct has a field to hold the closure in; this compiler
    //    fuses the whole chain at the terminal (`science-mir`'s
    //    `Builder::lower_chain`), so no struct is ever built and `F` would be
    //    a parameter nothing reads. `Item` is carried instead, because that is
    //    what `collect()` must name and what the *next* link's closure must
    //    take. The source is kept so the type still identifies the chain.
    //
    // 3. **`sorted(by:)` and not `sort(by:)`.** §3.2 splits the pair —
    //    *"`sorted` yields a new chain, `sort` reorders an `Array` in
    //    place"* — and §1.4 lists `sorted(by: key)` as the barrier adapter.
    //    The example wrote `.sort(by:)` mid-chain, which is the `Array`
    //    method in a position no `Array` is in; the example is what moved.
    //
    // 4. **`first`, `find`, `has_any` and `has_all` take `self`, where §1.4
    //    writes `mutable self`.** The note's reason for the exclusive
    //    receiver is §2.3's point 2 — *"so that a chain can be probed and
    //    then continued; those require `let mutable`"* — and a probed chain
    //    is a chain **stored in a variable**, which is the one shape this
    //    compiler refuses outright (no adapter has a runtime
    //    representation; `science-mir` fuses at the terminal). So the
    //    exclusive receiver would buy nothing a program can reach, and it
    //    would cost every chain written in one expression — every corpus
    //    site — an exclusive borrow of a temporary. By value is what the
    //    other terminals take and what a one-expression chain is. The day a
    //    chain can be stored, these four go back to `mutable self` and
    //    nothing that compiles today stops compiling: a by-value receiver is
    //    the stricter of the two at a stored chain, and the laxer one at a
    //    temporary.
    //
    // # Why the closure parameters are written as arrows
    //
    // See [`Ty::Fn`]. A bare `f: F` would type-check and then bind `each` at
    // `Ty::ERROR` in silence, which is the failure mode this compiler has lost
    // the most time to.

    // # `sorted(by:)` takes an `Int` key, where §1.4 writes `K: Ord`
    //
    // Narrowed deliberately, and the reason it was narrowed is half gone.
    // `Ord` used to be *"a name with implementations and no methods"*, so
    // there was no comparison a generic key could be sorted by at all. It
    // has one now — [`INTERFACE_DECLS`]' `Ord.less` — and generic Science
    // code can write `a < b` under `T: Ord` and run it. What is left is the
    // runtime half: `science_array_sort_by_int_key` compares two `I64`s in
    // Rust, and a `K: Ord` key means calling a monomorphised `less` back
    // from inside the runtime's sort, a callback this backend does not emit.
    //
    // An `Int` key is what §1.4's own example sorts on
    // (`.sorted(by: each.score)` over a numeric field), it is what
    // `examples/00_kitchen_sink.science` writes (`line.length()`), and the
    // difference from the note is a **type error at the call site** rather
    // than a program that checks clean and is refused by the backend — which
    // is the trade this file makes everywhere else. Widening it is one
    // `K: Ord` bound and one comparison in `science_array_sort_by_int_key`,
    // and a callback into the key's `less`, now that `Ord` has the method.

    // # `sum()` returns the item with its borrow peeled, and the checker says so
    //
    // §1.4 writes `sum(self) -> Total where Self.Item: Add of Output =
    // Total`, and §1.5 adds that there is no separate *"`sum` over
    // references"* — a chain of `borrowed Int` sums to an `Int`. Neither half
    // is a signature [`Method`] can carry: it has no `where` clause, and
    // *"`I` with one shared borrow taken off"* is not a type a declaration
    // can spell while `I` is still a parameter.
    //
    // **Decision. The declaration says the item, and `science-types`'
    // `BodyChecker::chain_total` peels it and holds it to §5.1's numbers.**
    // `ArrayIterate[T]` declares `-> T`, which is already exact; each adapter
    // declares `-> I`, which is exact after a `map` and one borrow too many
    // anywhere else. The checker reads the call's result, takes one shared
    // borrow off, and refuses anything that is not a numeric primitive with
    // `SC0547` — `String` included, although `String implements Add`, because
    // §1.5 says in as many words that *"concatenation is not summation"*.
    //
    // **The reason is the diagnostic.** Without the checker's half, `sum()`
    // over `String`s checks clean and reaches the backend as a hole — an
    // `SC0400` about MIR, for a mistake that is one word in the source. The
    // cost is one prelude method the checker knows by name, which is the
    // lookup `science-mir` already makes for every link here; it goes away
    // when `Add` has an `Output` this file can bound on.

    // # A predicate and a key closure take a **borrow** of the item; `map`
    // # takes the item
    //
    // §1.2 writes the predicate out: `where P: (borrowed Self.Item) -> Bool`.
    // That is not decoration — a chain whose `Item` is an owned `String`
    // (anything after a `map` that allocates) would have `discard` and
    // `sorted(by:)` *destroy* the value they were asked to inspect, because a
    // by-value closure parameter is dropped at the end of the closure body.
    // It is a use-after-free with a length of zero for a symptom, and it is
    // what the first version of this table produced. `keep`, `has_any`,
    // `has_all` and `find` are predicates by the same sentence.
    //
    // `map` is the exception and stays `(Self.Item) -> U`, per §1.4: a map
    // *consumes* the item and hands back another, which is exactly what its
    // own link does to the chain.
    //
    // **On `ArrayIterate[T]` the borrow is already there.** §4.3 makes the
    // item `borrowed T`, so `borrowed Self.Item` would be `borrowed borrowed
    // T` — a spelling that says nothing the first borrow does not. It
    // collapses, and the declaration below is written `(borrowed T)` on all
    // of them. `science-mir`'s `Builder::chain_argument` reads whichever of
    // the two spellings a link was declared with rather than assuming either.

    // # Thirteen links on seven types, written once
    //
    // Every chain type answers the same thirteen names, and the only things
    // that differ between them are the type itself, its `Item`, the spelling
    // of a borrowed item (collapsed on the source, above) and what `sum()`
    // declares. [`chain_links!`] takes those four and writes the thirteen. By
    // hand this table was five copies of five methods; at thirteen it would
    // be ninety-one entries whose only content is those four facts, and a
    // copy that drifted — a `keep` returning `Discard`, a `find` taking its
    // item by value — would be a miscompile nothing reports.

    // The source. §5.4: *"`def iterate(self) -> ArrayIterate of T`"*, and §4.3
    // fixes its `Item`: *"`iterate()` yields `borrowed Item` uniformly"*, which
    // §4.1's three-word table writes as `Item = borrowed Doc` and §4.4's
    // ownership table restates as a shared borrow of the source for the
    // chain's life. That decision is the note's, not this file's, and it is
    // why every `Item` below starts life as `&T` and why `headlines` in
    // `examples/00_kitchen_sink.science` collects into `Array[&String]`.
    Block {
        ty: "ArrayIterate",
        generics: &["T"],
        interface: None,
        assoc: &[],
        methods: chain_links!(
            Ty::App("ArrayIterate", &[Ty::Var("T")]),
            item: Ty::Ref(&Ty::Var("T")),
            borrowed: Ty::Ref(&Ty::Var("T")),
            total: Ty::Var("T"),
        ),
    },
    // The six adapters. Each is `X[S, I]` — the chain it consumed, and the
    // `Item` it yields — so the thirteen links read identically on all of
    // them and a chain's type still spells the chain out.
    chain_adapter!("Discard"),
    chain_adapter!("Keep"),
    chain_adapter!("MapOver"),
    chain_adapter!("Take"),
    chain_adapter!("Skip"),
    chain_adapter!("SortedBy"),
    chain_adapter!("NumberedOver"),
    chain_adapter!("TakeWhile"),
    chain_adapter!("SkipWhile"),
    chain_adapter!("Every"),
    chain_adapter!("Zip"),
    chain_adapter!("KeepSome"),
    chain_adapter!("Accumulate"),
    chain_adapter!("Reverse"),
    chain_adapter!("Unique"),
    chain_adapter!("Windows"),
    chain_adapter!("Batches"),
    chain_adapter!("Flatten"),
    chain_adapter!("FollowedBy"),
    chain_adapter!("Owned"),
    // @ADAPTER-END
    // The `Map` sources, §5.4: *"`Map` yields `Entry of (K, V)` and
    // additionally offers `keys()`, `values()`"*. `iterate()` yields the
    // same `&Entry[K, V]` `for entry in m:` does, `keys()` a borrow of each
    // key and `values()` of each value, all in insertion order (§5.2).
    Block {
        ty: "MapIterate",
        generics: &["K", "V"],
        interface: None,
        assoc: &[],
        methods: chain_links!(
            Ty::App("MapIterate", &[Ty::Var("K"), Ty::Var("V")]),
            item: Ty::Ref(&Ty::App("Entry", &[Ty::Var("K"), Ty::Var("V")])),
            borrowed: Ty::Ref(&Ty::App("Entry", &[Ty::Var("K"), Ty::Var("V")])),
            total: Ty::Var("V"),
        ),
    },
    Block {
        ty: "MapKeys",
        generics: &["K", "V"],
        interface: None,
        assoc: &[],
        methods: chain_links!(
            Ty::App("MapKeys", &[Ty::Var("K"), Ty::Var("V")]),
            item: Ty::Ref(&Ty::Var("K")),
            borrowed: Ty::Ref(&Ty::Var("K")),
            total: Ty::Var("K"),
        ),
    },
    Block {
        ty: "MapValues",
        generics: &["K", "V"],
        interface: None,
        assoc: &[],
        methods: chain_links!(
            Ty::App("MapValues", &[Ty::Var("K"), Ty::Var("V")]),
            item: Ty::Ref(&Ty::Var("V")),
            borrowed: Ty::Ref(&Ty::Var("V")),
            total: Ty::Var("V"),
        ),
    },
    // **Every chain type `implements Iterate`**, so that `for x in chain:`
    // has an element type. Method-less, `Array`'s way: the interface's own
    // `next` is what `iterate_item` reads `Item` out of, and
    // `science-mir`'s `lower_for` fuses the chain with the loop body rather
    // than calling a `next` no adapter has (no adapter has a runtime
    // representation: §2.3 of the note, and this block's section above).
    Block {
        ty: "ArrayIterate",
        generics: &["T"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::Var("T")))],
        methods: &[],
    },
    Block {
        ty: "MapIterate",
        generics: &["K", "V"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::App("Entry", &[Ty::Var("K"), Ty::Var("V")])))],
        methods: &[],
    },
    Block {
        ty: "MapKeys",
        generics: &["K", "V"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::Var("K")))],
        methods: &[],
    },
    Block {
        ty: "MapValues",
        generics: &["K", "V"],
        interface: Some(("Iterate", &[])),
        assoc: &[("Item", Ty::Ref(&Ty::Var("V")))],
        methods: &[],
    },
    chain_iterate!("Discard"),
    chain_iterate!("Keep"),
    chain_iterate!("MapOver"),
    chain_iterate!("Take"),
    chain_iterate!("Skip"),
    chain_iterate!("SortedBy"),
    chain_iterate!("NumberedOver"),
    chain_iterate!("TakeWhile"),
    chain_iterate!("SkipWhile"),
    chain_iterate!("Every"),
    chain_iterate!("Zip"),
    chain_iterate!("KeepSome"),
    chain_iterate!("Accumulate"),
    chain_iterate!("Reverse"),
    chain_iterate!("Unique"),
    chain_iterate!("Windows"),
    chain_iterate!("Batches"),
    chain_iterate!("Flatten"),
    chain_iterate!("FollowedBy"),
    chain_iterate!("Owned"),
    // @ITERATE-END
    // --- Level 1 `math`, `stdlib-core.md` §8 --------------------------------
    //
    // **Methods, and §8.2 is why.** *"The Level 1 subset is methods on `F32`,
    // `F64` and the integer types"*: a free `abs`, `min` or `round` would be a
    // global name forever, and as methods they cost none. §8.1's table is the
    // list, transcribed whole: nineteen elementary names, twelve
    // trigonometric, four predicates — thirty-five, which is §9's *"~35
    // methods"* exactly. Every receiver is `self`, which revision 2 makes a
    // shared borrow, because §8.4 writes `def sqrt(self) -> F64` and a
    // by-value receiver would be `self: Self`; the backend reads the scalar
    // through it, which is one load of a value already in a register's reach.
    //
    // **Names and parameter types come from the notes; parameter names are
    // decided here** where no note gives one, except `atan2`'s `x`, which
    // §8.4 writes: `y.atan2(x)` is the quadrant-correct arctangent of `y / x`.
    //
    // **What each one means at the edges is decided once, in
    // `science-codegen-llvm`'s `Lowerer::math_method`**, which is where the
    // NaN rule of `min`/`max`, the tie rule of `round` and the overflow rule
    // of `Int.abs` are written beside the instruction that implements them.
    // The short form, for a reader of this table: `min`/`max` are IEEE-754
    // `minimum`/`maximum` (NaN propagates, `-0.0 < +0.0`), `round` is
    // half-away-from-zero (`intrinsics-math-physics.md` §3.2's `llvm.round`
    // row), `sign` of `±0.0` and of NaN is the argument itself, and the
    // integer forms wrap exactly as `+`, `-` and `**` already do.
    Block { ty: "F64", generics: &[], interface: None, assoc: &[], methods: float_methods!(F64) },
    Block { ty: "F32", generics: &[], interface: None, assoc: &[], methods: float_methods!(F32) },
    // **`I64`, which is `Int`** — [`ALIASED_PRIMITIVES`] makes them one
    // definition, so this block is `Int has:` as well. §8.1 does not
    // enumerate an integer surface; these seven are the names on its list
    // that mean something on an integer, and `intrinsics-math-physics.md`
    // §4.3's integer table names the same seven in its *Arithmetic* row
    // (with `div_euclid`, which §8.1 does not have and [`UNWRITTEN`] holds).
    //
    // `I8`…`I32` and the unsigned widths are **not** given the block yet, and
    // stay in [`WHOLLY_OPEN`] so that `x.abs()` on an `I32` is silence rather
    // than a false `SC0532`: the note gives the names to *"the integer
    // types"*, and only `Int` is transcribed.
    Block {
        ty: "I64",
        generics: &[],
        interface: None,
        assoc: &[],
        methods: &[
            Method { name: "abs", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "sign", generics: &[], recv: Some(SelfKind::Shared), params: &[], ret: Some(INT) },
            Method { name: "min", generics: &[], recv: Some(SelfKind::Shared), params: &[("other", INT)], ret: Some(INT) },
            Method { name: "max", generics: &[], recv: Some(SelfKind::Shared), params: &[("other", INT)], ret: Some(INT) },
            Method {
                name: "clamp",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("low", INT), ("high", INT)],
                ret: Some(INT),
            },
            Method {
                name: "rem_euclid",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("divisor", INT)],
                ret: Some(INT),
            },
            Method {
                name: "pow",
                generics: &[],
                recv: Some(SelfKind::Shared),
                params: &[("exponent", INT)],
                ret: Some(INT),
            },
            // The overflow discipline's wrapping half, declared here rather
            // than by `wrapping_block!` below because this is `I64`'s one
            // block: see the `wrapping_*` comment under it.
            Method {
                name: "wrapping_add",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("other", INT)],
                ret: Some(INT),
            },
            Method {
                name: "wrapping_sub",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("other", INT)],
                ret: Some(INT),
            },
            Method {
                name: "wrapping_mul",
                generics: &[],
                recv: Some(SelfKind::Value),
                params: &[("other", INT)],
                ret: Some(INT),
            },
        ],
    },
    // --- `wrapping_*`, `intrinsics-math-physics.md` §3.3 ------------------
    //
    // **Decision. The three wrapping operations are declared on every
    // fixed-width integer, and nothing else of Decision 4's integer surface
    // is.** The core spec says *"integer overflow panics in debug builds and
    // wraps in release, as Rust does. `wrapping_add` and friends are
    // explicit"*. `science-codegen-llvm` emits only the release half today
    // (`lower_binary`'s *"Overflow"* note), so `a * b` already wraps — but a
    // program that *means* the wrap, which is every hash and every random
    // number generator, would be relying on the half of that sentence that
    // is missing. These are its other half, and `random` is written over them.
    //
    // **The lowering is the plain instruction, and it happens in
    // `science-mir`**: `science_mir::lower`'s `wrapping_operator` rewrites the
    // call into the `Rvalue::Binary` that `+`, `-` or `*` would have built,
    // which is the row the note's table gives (*"the plain instruction …
    // two's complement is exact by definition"*).
    //
    // **Cost.** `checked_*`, `saturating_*`, `rotate_*` and the rest of the
    // table stay untranscribed — the blocks are `open`, so writing one is
    // `SC0400` at the backend rather than a front-end error, as for every
    // other name a note gives a prelude type and this file has not reached.
    //
    // **One block per type.** `I64` — which is `Int`, [`ALIASED_PRIMITIVES`]
    // making them one definition — declares the three in its own block
    // above, beside §8.1's seven, and has no `wrapping_block!` here. A second
    // block naming the same method on one definition was measured as a
    // diagnostic whose span is in no file and a renderer that panics on it.
    //
    // The narrower widths stay in [`WHOLLY_OPEN`], so these three are the
    // only names their blocks answer and every other name is still silence.
    wrapping_block!("I8"),
    wrapping_block!("I16"),
    wrapping_block!("I32"),
    wrapping_block!("U8"),
    wrapping_block!("U16"),
    wrapping_block!("U32"),
    wrapping_block!("U64"),
];

/// The method names `stdlib-core.md` §8.1 gives a numeric type, with the
/// numeric types they apply to. Read by `science-types` to decide whether a
/// type-qualified name is one of §8.1's constants, and kept here because this
/// is the file that transcribes §8.
///
/// **The constants are not declarations, and §8.4 says why.** `const PI: F64`
/// inside `F64 has:` is *"an associated constant, which §4.4 does not yet
/// permit in a `has:` block — the one language ask in this note (§11)"*. The
/// HIR has no associated-constant item for a [`Block`] to hold, and inventing
/// one for eight names would be the language feature §11 asks the core spec
/// for, decided by a table. So the spelling §8 gives — `F64.PI`, a field read
/// on the type — is answered where a field read on a type is already
/// answered: `science-types`' `field`, which builds the literal this table
/// gives the value of, beside `Format.Json`'s variant-as-value arm. Nothing
/// below `science-types` sees anything but a float or integer literal.
///
/// **`MIN` is the most negative finite value, not the smallest positive one**,
/// which is Rust's and NumPy's `finfo.min` reading and not C's `DBL_MIN`.
/// `intrinsics-math-physics.md` §4.3 calls `MIN` *"the most-confused constant
/// in every language that has both"* and adds `MIN_POSITIVE` for C's meaning;
/// that name is not §8.1's and is not here.
///
/// `Int` has `MIN` and `MAX` only: `PI` on an integer is not a value.
pub const NUMERIC_CONSTANTS: &[(&str, &str, NumericConstant)] = &[
    ("F64", "PI", NumericConstant::Float(std::f64::consts::PI)),
    ("F64", "E", NumericConstant::Float(std::f64::consts::E)),
    ("F64", "TAU", NumericConstant::Float(std::f64::consts::TAU)),
    ("F64", "INFINITY", NumericConstant::Float(f64::INFINITY)),
    ("F64", "NAN", NumericConstant::Float(f64::NAN)),
    ("F64", "EPSILON", NumericConstant::Float(f64::EPSILON)),
    ("F64", "MIN", NumericConstant::Float(f64::MIN)),
    ("F64", "MAX", NumericConstant::Float(f64::MAX)),
    // Each `F32` value is the `F32` constant widened, which is exact; the
    // backend narrows it back with the rounding that gives the same bits.
    ("F32", "PI", NumericConstant::Float(std::f32::consts::PI as f64)),
    ("F32", "E", NumericConstant::Float(std::f32::consts::E as f64)),
    ("F32", "TAU", NumericConstant::Float(std::f32::consts::TAU as f64)),
    ("F32", "INFINITY", NumericConstant::Float(f64::INFINITY)),
    ("F32", "NAN", NumericConstant::Float(f64::NAN)),
    ("F32", "EPSILON", NumericConstant::Float(f32::EPSILON as f64)),
    ("F32", "MIN", NumericConstant::Float(f32::MIN as f64)),
    ("F32", "MAX", NumericConstant::Float(f32::MAX as f64)),
    ("I64", "MIN", NumericConstant::Int(i64::MIN)),
    ("I64", "MAX", NumericConstant::Int(i64::MAX)),
];

/// One of [`NUMERIC_CONSTANTS`]' values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumericConstant {
    /// A float constant, at `f64` precision whatever its type.
    Float(f64),
    /// An integer constant.
    Int(i64),
}

/// The constant `ty.name` stands for, if §8.1 gives one. `ty` is the
/// canonical name — `I64` and `F64`, never their aliases — which is what
/// `Def::name` holds for both spellings.
pub fn numeric_constant(ty: &str, name: &str) -> Option<NumericConstant> {
    NUMERIC_CONSTANTS.iter().find(|(t, n, _)| *t == ty && *n == name).map(|(_, _, value)| *value)
}

/// The surface a note gives a prelude type and [`BLOCKS`] has not transcribed.
///
/// # Why this table exists
///
/// [`BLOCKS`]' own comment says every prelude block is `open`: *"the set of
/// methods declared on a prelude type is smaller than the set that exists, so a
/// name this table does not have is 'not written down yet' and never 'no such
/// method'"*. `science-types`' `Methods::surface_is_closed` read that off
/// `Def::is_builtin`, and the consequence was total: **`"hi".no_such_method()`
/// and `(1).no_such_method()` checked clean**, and so did every misspelling of
/// every method in the standard library. That is the entire library surface
/// exempt from the check a user type gets, and it is why a binding initialised
/// from `items.get_mut(0)` had no type for anything downstream to read.
///
/// **Decision. The openness is named rather than blanket.** A name a note gives
/// a prelude type, and that [`BLOCKS`] has not transcribed yet, is *"not
/// written down"* and stays silent. Every other name on a prelude type is
/// `SC0532`, exactly as it is on a `Doc`.
///
/// **The reason is that "open" was standing in for two different facts.** One
/// is *"the note says `String.slice` exists and this file has not got to it"* —
/// a fact about this file, and reporting it would put a diagnostic on a correct
/// program. The other is *"nothing anywhere says `String.no_such_method`
/// exists"* — a fact about the program, and it is the one `SC0532` was written
/// to say. Reading them both off `is_builtin` bought the first at the price of
/// the second, and the second is the common case: a misspelling is what authors
/// actually write.
///
/// **Every entry is read out of a note, and the note is cited.** A name that is
/// here because it was convenient would be an exemption, which is the thing
/// `science-types/tests/corpus.rs` says a list of this shape must not become.
///
/// **What it costs, and how it is paid off.** The table is a second place that
/// has to move when [`BLOCKS`] moves: transcribing `String.slice` means adding
/// the [`Method`] and deleting the name here, and forgetting the deletion
/// leaves a silence nothing reports. That is why it lives in this file, beside
/// the table it complements, rather than in `science-types` where it is read —
/// two edits in one file are a diff a reviewer sees, and two edits in two
/// crates are not. `science-types/tests/method_lookup.rs` holds both halves as
/// a pair — a name here is silent, a name nowhere is `SC0532` — so the day this
/// table is empty the constant and the predicate that reads it are both
/// deleted and only the first half of that pair is left.
///
/// **The lookup is by name**, because a `DefId` for a prelude type is allocated
/// at build time and this is a `const`. Prelude type names are unique within
/// the prelude and a user type cannot shadow one into this table — the
/// predicate that reads it asks only about a head whose `Def::is_builtin` is
/// true — so the string comparison is total. It is the lookup the `DefTable`
/// exists to abolish, performed once per otherwise-unresolved method call.
const UNWRITTEN: &[(&str, &[&str])] = &[
    // `String` — §6.9's nineteen are all in the block above now (`from_bytes`
    // left this row when the bundled `os` module called it, `slice` and
    // `split` were the last to leave it, and `lines` and `bytes` had left it
    // in substance at Gate C1 without being struck here), so what remains is
    // a name another note gives.
    ("String", &[
        // `clone` retired from this list the day `INTERFACE_DECLS` gained
        // `Clone.clone`: §6.2's *"`.owned()` and `.clone()` are both written"*
        // is now true of the first half. `owned()` stays — it is
        // `collections-and-chains.md` §1.4's chain terminal, returning an
        // `Owned of Self`, and `Owned` is a Level 1 type this prelude does
        // not have, which is `slice`'s case and not `clone`'s.
        "owned",
    ]),
    // `Array` — every name either note gives it that the block above does not
    // have. `sort` and `reserve` left this row when `docs/DREAM.md` §13.4's
    // catalogue was transcribed into [`BLOCKS`]; what stays is what that
    // catalogue and §5.4 give and nothing here can lower yet. The two
    // `iterate*` are §5.4 and they *do* have signatures — they return
    // `ArrayIterate[T]`'s two siblings, which are types the prelude does not
    // have, so they are `slice`'s case one type along.
    //
    // **§13.4's four that need more than a move.** `filled(value, n)` and
    // `concat(other)` are `where T: Clone`, and copying an element the
    // runtime knows only as `size` bytes needs a clone function no
    // `ScienceTypeInfo` carries — `clone`'s case below. `join(sep)` is
    // declared on `Array[String] has:`, a block on one instantiation, which
    // [`Block`] cannot express without declaring it on every `Array[T]`.
    // `slice(from, to) -> (&Array[T])?` is a borrowed sub-array, which needs
    // a header that points into another array's buffer — the representation
    // `collections-and-chains.md` §5.3 refuses and
    // `indexing-and-array-literals.md` §2 gives to a separate `Slice of T`.
    //
    // **`len` is deliberately absent, and it is now a measured corpus
    // disagreement rather than a hypothetical one.**
    // `collections-and-chains.md` §5's rejected-names table lists `len()` /
    // `count()` against the surviving `count()` / `length()` pair — *"the
    // abbreviation goes, and the surviving pair now marks an `O(1)` lookup
    // against an `O(n)` consumption"* — §4.3 is the general rule it applies,
    // and `stdlib-shape-and-packages.md` §"naming" restates it as
    // *"`length()`, not `len()`"*. So `xs.len()` names nothing any note gives,
    // and `SC0532` on it is true.
    //
    // `examples/21_compiler_shapes.science` writes it three times — lines 82,
    // 185 and 256 — which is the corpus disagreeing with the spec, not a hole
    // in this table. The fix is `length()` at those three sites. Note that
    // several *illustrative* notes (`models-and-inference.md`, `data-io.md`,
    // `script-mode.md`, `region-inference.md`'s `v.push(v.len())`) write
    // `len()` in sample code; none of them is the note that owns collection
    // naming, and §5's table is the one that decided. Whoever reconciles them
    // either fixes the samples or reopens §5 — but the prelude cannot declare
    // both spellings without making the abbreviation rule a dead letter.
    //
    // `get_mut` is the same finding one method along: `19_stdlib.science:180`
    // writes it, `indexing-and-array-literals.md` §1.4 spells it
    // `get_mutably`, and that name *is* transcribed in [`BLOCKS`] above with
    // the reasoning. It is not listed here because a name no note gives does
    // not belong in a table of names the notes give.
    //
    // `clone` is the `String` row's last two lines again: [`IMPLEMENTS`]' own
    // comment says `Array[T] implements Clone` *"holds only where `T:
    // Clone`"* and that a conditional implementation is not something that
    // table can express — so the implementation is asserted to exist, is not
    // declared, and its method name is *"not written down"* by the same
    // argument.
    ("Array", &[
        // `iterate` left this row when [`BLOCKS`] gained it: the reason it
        // was here was that `ArrayIterate[T]` is *"a type the prelude does not
        // have"*, and the prelude has it now. Its two siblings stay, for the
        // reason `iterate`'s own declaration gives.
        "iterate_mutably", "iterate_consuming", "filled", "concat", "join", "slice",
    ]),
    // `Map` — `keys`, `values` and `values_mutably` are
    // `collections-and-chains.md` §5.4 by name; the three `iterate*` are the
    // same section's requirement of *"every collection"*; `from` is §5.5's
    // `Map.from(..)`. `for entry in counts:` works without `iterate` —
    // `Map implements Iterate` in [`BLOCKS`] is what it reads, and
    // `science-mir`'s `lower_for_over_map` what it runs — so `iterate` stays
    // here for `Array`'s sibling reason: it returns a source type
    // (`MapIterate`, by §5.4's pattern) the prelude does not have, and no
    // chain link after it would lower if it did.
    (
        "Map",
        &[
            "values_mutably",
            "iterate_mutably",
            "iterate_consuming",
            "from",
            // The `Array` row's `clone`, for its reason.
            "clone",
        ],
    ),
    // **The seven chain types, each with the same list: the names of
    // `collections-and-chains.md` §1.4 that [`BLOCKS`] has not transcribed.**
    // These rows used to be [`WHOLLY_OPEN`] entries, and that row's own
    // reason — *"listing the other thirty-two in [`UNWRITTEN`], five times
    // over, is transcribing the vocabulary into the wrong table"* — stopped
    // holding once thirteen names were transcribed: the remainder is
    // one list, [`CHAIN_UNWRITTEN`], named once and pointed at seven times,
    // and what closing the types buys is `SC0532` on a misspelled link —
    // `.frist()`, `.colect()` — which until now reached the backend as a
    // hole and was reported as an `SC0400` about MIR.
    ("ArrayIterate", CHAIN_UNWRITTEN),
    ("Discard", CHAIN_UNWRITTEN),
    ("Keep", CHAIN_UNWRITTEN),
    ("MapOver", CHAIN_UNWRITTEN),
    ("Take", CHAIN_UNWRITTEN),
    ("Skip", CHAIN_UNWRITTEN),
    ("SortedBy", CHAIN_UNWRITTEN),
    ("NumberedOver", CHAIN_UNWRITTEN),
    ("TakeWhile", CHAIN_UNWRITTEN),
    ("SkipWhile", CHAIN_UNWRITTEN),
    ("Every", CHAIN_UNWRITTEN),
    ("Zip", CHAIN_UNWRITTEN),
    ("KeepSome", CHAIN_UNWRITTEN),
    ("Accumulate", CHAIN_UNWRITTEN),
    ("Reverse", CHAIN_UNWRITTEN),
    ("Unique", CHAIN_UNWRITTEN),
    ("Windows", CHAIN_UNWRITTEN),
    ("Batches", CHAIN_UNWRITTEN),
    ("Flatten", CHAIN_UNWRITTEN),
    ("FollowedBy", CHAIN_UNWRITTEN),
    ("Owned", CHAIN_UNWRITTEN),
    // @UNWRITTEN-END
    ("MapIterate", CHAIN_UNWRITTEN),
    ("MapKeys", CHAIN_UNWRITTEN),
    ("MapValues", CHAIN_UNWRITTEN),
    // `F64` and `F32` — the eleven names `intrinsics-math-physics.md` §4.2's
    // Decision 8 adds to §8.1's prelude (P1–P3), which [`BLOCKS`] does not
    // transcribe because the task that wrote the block was §8.1's list and
    // not that note's extension of it. `MIN_POSITIVE` is the twelfth and is
    // a constant, so it is not a method name to be silent about.
    ("F64", FLOAT_EXTENSIONS),
    ("F32", FLOAT_EXTENSIONS),
    // `Int` — the same note's §4.3 integer table, less the ten [`BLOCKS`]
    // has: `div_euclid`, the bit operations, and the overflow discipline but
    // for its three `wrapping_*`.
    (
        "I64",
        &[
            "div_euclid",
            "count_ones",
            "count_zeros",
            "leading_zeros",
            "trailing_zeros",
            "reverse_bytes",
            "rotate_left",
            "rotate_right",
            "checked_add",
            "checked_sub",
            "checked_mul",
            "saturating_add",
            "saturating_sub",
            "saturating_mul",
        ],
    ),
];

/// `collections-and-chains.md` §1.4's names that no chain type in [`BLOCKS`]
/// declares yet, in the note's own order.
///
/// Thirteen names (the third tranche removed `reduce`, `product`, `accumulate`,
/// `keep_some` and `reverse`). §1.4 counts *rows* — thirty-eight — and a row is not a
/// name: `minimum`/`maximum` and `has_any`/`has_all` are one row and two
/// names each, and `sorted()`/`sorted(by:)` and `unique()`/`unique(by:)` are
/// two rows and one name each. Twenty-one names are declared; these are the
/// rest, and the two lists together are every name the table gives.
///
/// **`minimum` and `maximum` are declared with `by:` only**, `sorted`'s
/// narrowing for `sorted`'s reason (there is no `Ord` to compare a bare item
/// by), so §1.4's bare `minimum()` is a wrong argument count and not a
/// silence. **`every(n)` is §1.4's `step_by`**, under the name the note gives
/// it. **`zip` is declared for an `Array.iterate()` as its `other` and for no
/// other chain** (see the declaration).
///
/// **Two names in §1.4 are not here although only one of their forms is
/// declared**: `sorted()` beside `sorted(by:)`, and `unique()` beside
/// `unique(by:)`. `sorted` is declared, with its `by:` parameter, so a bare
/// `sorted()` is a wrong argument count and not a silence — the same
/// narrowing [`BLOCKS`]' `sorted(by:)` comment already prices, which is that
/// there is no `Ord` to sort a bare item by. `unique` is here, both forms.
const CHAIN_UNWRITTEN: &[&str] = &[
    // Transforming.
    "expand",
    // Filtering and selecting.
    "keep_ok",
    // Pairing, grouping, windowing.
    // Terminals.
    "collect_or_error",
    "partition_results",
    "partition",
    "group",
];

/// `intrinsics-math-physics.md` §4.3's float names beyond `stdlib-core.md`
/// §8.1 — see [`UNWRITTEN`]'s `F64` row.
const FLOAT_EXTENSIONS: &[&str] = &[
    "exp2",
    "exp10",
    "expm1",
    "ln1p",
    "asinh",
    "acosh",
    "atanh",
    "copysign",
    "round_ties_even",
    "div_euclid",
    "fma",
];

/// Prelude types whose method surface is open *entirely*, with the reason.
///
/// **Decision. A head is wholly open only where a note would have to be written
/// before any name on it could be judged** — not where the transcription is
/// merely behind. [`UNWRITTEN`] is the second case and is a list of names; this
/// is the first and is a list of types, because there is no name to list when
/// the question itself is unanswered.
///
/// - **`Box` is closed, and this is the row that used to hold it open.**
///   Nothing in `stdlib-core.md` or `collections-and-chains.md` ever said
///   whether a method call on a `Box[T]` reaches `T`'s methods — both notes
///   mention `Box` only as a bare name in a list of Level 1 types — and that
///   was the whole of the silence's justification: not a decision, an absence
///   of one. `type-checking-and-mir.md`'s Decision 28 is that decision now:
///   *"a `Box` is transparent to a method call, scoped to a receiver reached
///   by shared borrow."* `science_types::methods::crosses_a_box` and
///   `receiver_head`'s peeling are where it is implemented, and a name this
///   predicate would have excused is now either found — the ordinary case,
///   `Doc`'s method through `Box[Doc]` — or reported as `Doc`'s own `SC0532`,
///   because the receiver `name_is_answerable` judges is `Doc`'s head and not
///   `Box`'s. **`Box` stays a name nothing lists in [`UNWRITTEN`]** — the
///   predicate this row fed is asked about `Box` at all only when the
///   decision's own peeling did not run (`Box` reached at `Form::Type`, or
///   with no prelude to find the definition in), and in both of those a
///   `Box`-headed call was never Decision 28's to answer either.
///
/// - **`Chars`.** Its whole surface is `Iterate`'s thirty-eight provided
///   methods (`collections-and-chains.md` §1.4), and the prelude declares
///   `Iterate` with `next` alone. Listing thirty-eight names in [`UNWRITTEN`]
///   would be transcribing the vocabulary into the wrong table; the type is
///   open until `Iterate` carries them. (The seven chain types took the
///   other road once [`BLOCKS`] declared thirteen of those names on them:
///   the rest is [`CHAIN_UNWRITTEN`], one list. `Chars` declares none of the
///   thirteen, so for it the list would be the whole vocabulary again.)
///
/// - **Every numeric primitive, plus `Bool` and `Char`.** Found while landing
///   `Clone.clone`, and it is the same finding one layer down: `Methods`'
///   `receiver` treats a builtin head as *answerable* the moment
///   [`Methods::index`](crate::methods::Methods) holds one entry for it, and
///   an entry is exactly what `IMPLEMENTS`' `NUMERIC` row started producing —
///   `I64 implements Clone:` has no methods of its own, so `impl_block`
///   contributes `Clone`'s own `clone`, and that one contribution is enough
///   to flip `I64.new()` from silent to `SC0532`. No note anywhere gives `I64`
///   — or `Int`, `F64`, `Bool`, `Char` — a *named* method at all; every one of
///   these types is known only through the operator interfaces, which
///   `check`'s `binary`/`implements_operand` dispatch on structurally and
///   never through this index. So a numeric surface is exactly `Box`'s case:
///   the question of what `.foo()` on an `I64` could mean is unanswered, not
///   merely untranscribed, and `Clone.clone` landing is what first gave these
///   heads an index entry to be judged by at all.
///
/// **What it costs is `SC0532` on these heads**, which is the blanket silence
/// this whole change is narrowing, surviving in named places instead of
/// everywhere. Each closes on its own note, and
/// `science-types/tests/method_lookup.rs` has a test that fails when it does.
const WHOLLY_OPEN: &[&str] = &[
    "Chars",
    // `Lines`, for `Chars`' reason: its surface is `Iterate`'s provided
    // methods and only `next` is transcribed.
    "Lines",
    // `Split`, for `Lines`' reason.
    "Split",
    // **The chain types are not here any more.** They were, for `Chars`'
    // reason, while six of §1.4's thirty-eight were transcribed; with
    // thirteen, the remainder is one list and [`UNWRITTEN`] points each of
    // the seven types at it. `examples/16_indentation.science`'s `.first()`,
    // which is what held them open, is declared now.
    // The numeric primitives, `Bool` and `Char`, which `Clone.clone` gave an
    // index entry and therefore a closed surface they have no note for.
    //
    // **`F64`, `F32` and `I64` left this row with `stdlib-core.md` §8**,
    // which is the note this comment was waiting for: §8.1 gives each of
    // them a surface and [`BLOCKS`] transcribes it, so `x.sqroot()` on an
    // `F64` is `SC0532` now, as it is on a `Doc`. (`Int` and `Float` were
    // never reached — `Def::name` is the canonical width — and went with
    // them.) The names a later note adds on top of §8.1 are [`UNWRITTEN`]'s
    // rows. The narrower integer widths stay, because §8.2 gives the names to
    // *"the integer types"* and only `Int` is transcribed: closing `I32` would
    // make `x.abs()` on it a false `SC0532`.
    "I8", "I16", "I32", "U8", "U16", "U32", "U64", "F16", "BF16", "Bool", "Char",
];

/// Whether *"this prelude type has no method of that name"* is a statement
/// about the program or about [`BLOCKS`].
///
/// `true` means the prelude has not written the name down and the caller must
/// stay silent; `false` means nothing gives the name and `SC0532` is true.
/// `science-types`' `Methods::surface_is_closed` is the only caller, and its
/// doc comment carries the decision.
pub fn is_unwritten(ty: &str, method: &str) -> bool {
    WHOLLY_OPEN.contains(&ty)
        || UNWRITTEN.iter().any(|(name, methods)| *name == ty && methods.contains(&method))
}

/// The fifteen prelude types a chain link is declared on: §5.4's four
/// sources and §1.4's eleven adapters that [`BLOCKS`] transcribes.
///
/// **Public because two crates ask "is this a chain link" by name** —
/// `science-types`, to peel `sum()`'s borrow (see [`BLOCKS`]' chain
/// section), and `science-mir`, to fuse a chain at its terminal — and a
/// list each of them kept would be a third place to update the day an
/// adapter lands, which is exactly the edit nothing would report missing.
pub const CHAIN_TYPES: &[&str] = &[
    "ArrayIterate",
    "Discard",
    "Keep",
    "MapOver",
    "Take",
    "Skip",
    "SortedBy",
    "NumberedOver",
    "TakeWhile",
    "SkipWhile",
    "Every",
    "Zip",
    "MapIterate",
    "MapKeys",
    "MapValues",
    "KeepSome",
    "Accumulate",
    "Reverse",
    "Unique",
    "Windows",
    "Batches",
    "Flatten",
    "FollowedBy",
    "Owned",
    // @CHAIN-END
];

/// The free functions' signatures.
///
/// **`read_file` takes a `&String`, and `stdlib-core.md` §5.2 says it
/// should take a `&Path`.** The deviation is deliberate and it is
/// costed: `Path` is not in the prelude, §5.2's own mitigation is `SC0257` —
/// *"a `String` was passed where a `Path` is expected"*, with an applicable fix
/// — and that diagnostic is unwritten. Declaring `&Path` today would
/// turn every `read_file("a.txt")` in the corpus into a bare type mismatch with
/// no help text, which is the worse half of the note's own argument. The
/// report names it as the deviation that should be closed by whoever lands
/// `Path`.
///
/// **`print` and `write` are deliberately left undeclared** — and
/// `print_error` and `write_error` with them, whose §4.1 parameter is the same
/// `borrowed any Display` and whose lowering is `print`'s — and this is the
/// one place a declaration was written, measured, withdrawn, measured a second
/// time and withdrawn again — for a different reason, which is the part worth
/// reading.
///
/// `strings-formatting-and-docs.md` §4.1 gives
/// `def print(value: &any Display)`.
///
/// # The first measurement, and what happened to it
///
/// Writing that signature used to report on **seven** correct corpus programs,
/// from three causes, none of which was the signature:
///
/// 1. **An integer literal was not defaulted against an `any I` expectation.**
///    `print(separated)` where `separated` is bound to a literal was
///    `expected any Display, found an integer literal` — Decision 2's default
///    never ran, because the expectation is an interface object rather than a
///    numeric type. Five programs, six sites.
/// 2. **A borrow of a branch-local temporary died at the branch.**
///    `print(if flag: "yes" else: "no")` took §6.3's auto-borrow inside each
///    arm, of a temporary whose storage ends there, and `SC0333` was then
///    correct about the MIR and wrong about the program. One program, two
///    sites.
/// 3. **A corpus type that does not implement `Display` is printed.** One.
///
/// **Causes 1 and 2 are closed.** They were the checker's, not the
/// signature's, and they are fixed where they live: `science-types`' `check`
/// §5b defaults a literal that meets a `&any I` slot and hands it to
/// the ordinary coercion, and §1b stops a &expectation at a branch so
/// that §6.3's auto-borrow is taken at the argument. Both were wrong about
/// programs that never mention `print` — `def show(v: &any Display)` in
/// a user's own file met each of them — so closing them was owed whatever
/// happens here.
///
/// **Cause 3 is a true positive and it is still there.**
/// `examples/04_enums.science` writes `print(number)` where `number` is a
/// `Token`, and `Token` implements nothing. That is one honest diagnostic
/// against a corpus file, and it is the corpus's to fix — by a
/// `Token implements Display:` block or by interpolating.
///
/// # The second measurement, which is the one that decides
///
/// **Declaring the parameter breaks the back half of the compiler, and the
/// back half already says so in as many words.** With the signature in place,
/// `print(x)` stops being *move `x` into the call* and becomes a `Borrow`
/// under a `Coerce { Unsize }` — a `&any Display`.
/// `science-codegen-llvm`'s `lower_print` matches exactly two operand shapes,
/// a string constant and a moved `String` local, and its own refusal text for
/// everything else is:
///
/// > *"a `print` of a value that is not a `String`: `print` takes
/// > `&any Display`, this backend emits no vtables, and there is no
/// > `display` method on `Display` to call through even if it did"*
///
/// So every `print` in every program becomes `Unlowered`. The same function's
/// doc comment names this declaration as its own precondition — *"`print` has
/// no declared signature, so the checker cannot know it borrows; MIR therefore
/// records `move` of the local into the call … the call is the value's last
/// owner, so the call frees it"* — and `science-rt`'s `science_print` takes a
/// `*const ScienceString` and nothing else. Measured: declaring it fails
/// **two tests in `science-mir/tests/fstring.rs`, one in `tests/unsize.rs`,
/// three in `science-regions` and two in `sciencec`**, and the fstring two are
/// precisely the accounting above — one more borrow than the lowering expects,
/// and an accumulator that is no longer moved and so is dropped after the call
/// has freed it.
///
/// # The decision
///
/// **The parameter stays undeclared; the *arity* does not.** §4.1's decision
/// has two halves, and only one of them needs the back half of the compiler to
/// move. That `print` is **unary** is a fact about the call, it is what §7's
/// `SC0275` reports, and `science-types`' `check` reports it now — see that
/// crate's `print_takes_one_value`. That the argument must implement `Display`
/// is a fact about the *value*, and nothing below the type checker can carry
/// it: `Display` has no method (see §"The declared surface" above for why),
/// that backend emits no vtables, and a `&any Display` is a type no
/// phase after this one can render.
///
/// **What it costs.** `print(doc)` on a type with no `Display` is still
/// accepted here and refused at codegen, which is the worse place to find out
/// — the same cost `check`'s `fstring` already prices for a user type with an
/// `implements Display:` block. This decision does not create that cost; it
/// declines to trade it for a compiler that cannot print at all.
///
/// **What closes it**, in order: `science-rt` gains a renderer behind
/// `Display` — §3.1's `Formatter`, or an entry point per prelude type on the
/// model of `science_string_push_*`; `science-codegen-llvm` gains a vtable and
/// `lower_print` gains an arm for a `&any Display`; then this list
/// gains four lines, [`Ty`] gains an `Any(&'static str)` variant lowering to
/// [`hir::TypeKind::Any`], and `examples/04_enums.science` owes one
/// implementation block.
const FUNCTION_SIGNATURES: &[Method] = &[
    // `-> Never` is the half of `SC0140`'s fourth exclusion that is a property
    // of the declaration, and `items`' `Signature::can_return` is already
    // written against it.
    Method {
        name: "panic",
        generics: &[],
        recv: None,
        params: &[("message", Ty::Ref(&STRING))],
        ret: Some(Ty::Name("Never")),
    },
    // §7.3: a Level 1 function returns its **concrete** error type, never
    // `Error?` — the interface form allocates, and the concrete form keeps a
    // caller's `match` exhaustive.
    Method {
        name: "read_file",
        generics: &[],
        recv: None,
        params: &[("path", Ty::Ref(&STRING))],
        ret: Some(Ty::Pair(&STRING, &Ty::Opt(&IO_ERROR))),
    },
    Method {
        name: "write_file",
        generics: &[],
        recv: None,
        params: &[("path", Ty::Ref(&STRING)), ("text", Ty::Ref(&STRING))],
        ret: Some(Ty::Opt(&IO_ERROR)),
    },
    // `strings-formatting-and-docs.md` §4.2's `flush()`. Declared where
    // `print_error` and `write_error` are not, because it has no `any
    // Display` parameter to be unable to lower — see [`FUNCTIONS`].
    Method { name: "flush", generics: &[], recv: None, params: &[], ret: None },
    // `stdlib-core.md` §4.4's `read_line()`: `null` at end of input, `""` for
    // a blank line. **`IoError?`, where §4.4 writes `Error?`** — §7.3 of the
    // same note, *"no Level 1 function has `Error?` in its signature"*, which
    // §4.7's own `count_lines` already follows by returning `read_line`'s
    // error as its `IoError?`. `science_read_line` says what a line is.
    Method {
        name: "read_line",
        generics: &[],
        recv: None,
        params: &[],
        ret: Some(Ty::Pair(&Ty::Opt(&STRING), &Ty::Opt(&IO_ERROR))),
    },
];

/// Builds the HIR for the declared surface, given the names already allocated.
struct Declarer<'a> {
    defs: &'a mut DefTable,
    module: DefId,
    names: HashMap<String, DefId>,
    /// `ffi`'s own names, which [`Ty::Ffi`] resolves through. Kept apart from
    /// `names` for that variant's reason.
    ffi_names: HashMap<String, DefId>,
    items: Vec<hir::Item>,
}

/// What a type in one declaration may refer to: the block's generic parameters
/// and its associated types.
///
/// `Self` is not among them, and that is a decision: every declaration below
/// writes its own type out — `Array.new() -> Array[T]`, not `-> Self`. The
/// two are the same type after `check`'s `block_substitution` runs, and the
/// written-out form is the one a reader of this file can check against the note
/// it came from without holding a substitution in their head.
struct Scope<'a> {
    generics: &'a HashMap<&'static str, DefId>,
    assocs: &'a HashMap<&'static str, DefId>,
    /// The block `Ty::SelfTy` stands for: the interface or implementation
    /// being declared. `None` inside [`Declarer::free`], where there is no
    /// enclosing block and nothing should ever ask for `Self` — a lookup
    /// there is a bug in this file, not in a program, so it panics rather
    /// than silently naming the wrong block.
    owner: Option<DefId>,
}

impl Declarer<'_> {
    fn named(&self, name: &str) -> DefId {
        *self
            .names
            .get(name)
            .unwrap_or_else(|| panic!("the prelude declares `{name}` before it is used"))
    }

    /// The kind of a prelude name, when it is declared.
    fn defs_kind(&self, name: &str) -> Option<DefKind> {
        self.names.get(name).map(|def| self.defs.get(*def).kind)
    }

    /// [`Declarer::named`] for the `ffi` module's own scope.
    fn ffi_named(&self, name: &str) -> DefId {
        *self
            .ffi_names
            .get(name)
            .unwrap_or_else(|| panic!("the prelude declares `ffi.{name}` before it is used"))
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
            Ty::Ffi(name, args) => hir::TypeKind::Path {
                res: Res::Def(self.ffi_named(name)),
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
            Ty::MutRef(inner) => {
                hir::TypeKind::Borrowed { mutable: true, inner: Box::new(self.ty(inner, scope)) }
            }
            // **`Error?` is `(any Error)?` here too**, which is
            // `resolve_nullable_inner`'s expansion for a user's source and was
            // not this function's. A prelude signature returning `Error?` was a
            // nullable *path to the interface*, and a user's `def write(…) ->
            // Error?` a nullable `any Error` — two types for one spelling, so
            // `SC0541` refused every implementation of `Write` with *"this
            // returns `any Error?`; `Write` declares `Error?`"*.
            Ty::Opt(Ty::Name(name)) if self.defs_kind(name) == Some(DefKind::Interface) => {
                let bound = hir::Bound {
                    kind: hir::BoundKind::Interface {
                        res: Res::Def(self.named(name)),
                        generics: Vec::new(),
                    },
                    span: BUILTIN_SPAN,
                };
                hir::TypeKind::Nullable(Box::new(hir::Type {
                    kind: hir::TypeKind::Any(bound),
                    span: BUILTIN_SPAN,
                }))
            }
            Ty::Opt(inner) => hir::TypeKind::Nullable(Box::new(self.ty(inner, scope))),
            Ty::Pair(left, right) => {
                hir::TypeKind::Tuple(vec![self.ty(left, scope), self.ty(right, scope)])
            }
            Ty::Fn(params, ret) => hir::TypeKind::Closure {
                params: params.iter().map(|param| self.ty(param, scope)).collect(),
                ret: Box::new(self.ty(ret, scope)),
            },
            Ty::SelfTy => {
                let owner = scope
                    .owner
                    .unwrap_or_else(|| panic!("`Self` has no enclosing block in this declaration"));
                hir::TypeKind::SelfType(Res::SelfTy(owner))
            }
        };
        hir::Type { kind, span: BUILTIN_SPAN }
    }

    fn bound(&self, interface: &str, args: &[Ty], scope: &Scope<'_>) -> hir::Bound {
        hir::Bound {
            kind: hir::BoundKind::Interface {
                res: Res::Def(self.named(interface)),
                generics: args.iter().map(|arg| self.ty(arg, scope)).collect(),
            },
            span: BUILTIN_SPAN,
        }
    }

    fn function(&mut self, method: &Method, parent: DefId, scope: &Scope<'_>) -> hir::Fn {
        let def = self.defs.alloc(DefKind::Fn, method.name, BUILTIN_SPAN, Some(parent));
        // **The method's own parameters shadow nothing and extend the
        // block's.** `map[U]` on `MapOver[S, I]` is written in `S`, `I` *and*
        // `U`, so the scope its signature is read in is the block's map with
        // the method's names added — the same nesting a user's
        // `def map[U](…)` inside a `Wrap[T] has:` block gets from the
        // resolver.
        let mut generics = scope.generics.clone();
        let generic_params: Vec<hir::GenericParam> = method
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
        let scope = &Scope { generics: &generics, assocs: scope.assocs, owner: scope.owner };
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
            generics: generic_params,
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
        let mut generics = HashMap::new();
        let generic_params: Vec<hir::GenericParam> = decl
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
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let methods: Vec<hir::Fn> =
            decl.methods.iter().map(|method| self.function(method, def, &scope)).collect();
        self.item(hir::ItemKind::Interface(hir::Interface {
            def,
            generics: generic_params,
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
        //
        // **Named through `alloc_impl`, the same as a written block, and not
        // through a bare `alloc` with `""`.** This table gives `Array` an
        // inherent block and an `Index[Int]` block of its own, two blocks on
        // one type exactly like a user's `Doc has:` beside `Doc implements
        // Sized:` — and before this was `alloc_impl`, every such pair (and
        // every other prelude type's block, all parented to one prelude
        // module) shared the one empty name `path_of` skips, so `Array.new`
        // and `Map.new` monomorphised at the same type argument both walked to
        // the bare path `.new` and mangled to one symbol. That is finding 25
        // again, on the builtins this crate writes by hand instead of parsing,
        // and `alloc_impl` is the one place the name is computed, so the
        // prelude asks it rather than growing a second copy.
        let def = self.defs.alloc_impl(block.ty, BUILTIN_SPAN, Some(self.module));
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
            let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
            for (name, ty) in block.assoc {
                assoc_types.push(hir::AssocType {
                    def: assocs[name],
                    ty: Some(self.ty(ty, &scope)),
                    span: BUILTIN_SPAN,
                });
            }
        }

        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let methods: Vec<hir::Fn> =
            block.methods.iter().map(|method| self.function(method, def, &scope)).collect();
        let interface = block.interface.map(|(name, args)| self.bound(name, args, &scope));
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
            interface: Some((interface, &[])),
            assoc: &[],
            methods: &[],
        });
    }

    /// A free function's declaration, at the `DefId` [`FUNCTIONS`] allocated.
    fn free(&mut self, method: &Method) {
        let def = self.named(method.name);
        let generics = HashMap::new();
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: None };
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

    /// `ffi.Span[T]` and `ffi.MutableSpan[T]`, at the `DefId` `build` already
    /// allocated as `DefKind::Record`: `{ pointer: &T, len: Int }` for
    /// `Span`, `{ pointer: &mut T, len: Int }` for `MutableSpan` —
    /// `ffi-c-boundary.md` §1.3's `{ borrowed T, Int }`, in declaration order.
    ///
    /// Not routed through [`Declarer::block`]: that method builds an *impl*
    /// over an already-named type, and what is missing here is the type's own
    /// declaration — the record and its generic parameter — which
    /// [`Declarer::interface`] is the nearer model for, minus the methods.
    ///
    /// The field names are not load-bearing. Nothing in the corpus or in this
    /// file projects `.pointer` or `.len` off a `Span` — the only field a
    /// program ever reads is `MatrixBase.data`, whose type happens to *be* a
    /// `Span` — so any two names would type-check identically. They exist at
    /// all because [`crate::hir::Record`] has no way to say "one borrowed
    /// field and one `Int`" without naming them.
    fn ffi_view(&mut self, def: DefId, mutable: bool) {
        let param = self.defs.alloc(DefKind::TypeParam, "T", BUILTIN_SPAN, Some(def));
        let mut generics = HashMap::new();
        generics.insert("T", param);
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let pointer_ty = if mutable {
            self.ty(&Ty::MutRef(&Ty::Var("T")), &scope)
        } else {
            self.ty(&Ty::Ref(&Ty::Var("T")), &scope)
        };
        let len_ty = self.ty(&INT, &scope);
        let fields = vec![
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "pointer", BUILTIN_SPAN, Some(def)),
                ty: pointer_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "len", BUILTIN_SPAN, Some(def)),
                ty: len_ty,
                span: BUILTIN_SPAN,
            },
        ];
        self.item(hir::ItemKind::Record(hir::Record {
            def,
            generics: vec![hir::GenericParam {
                def: param,
                kind: hir::GenericParamKind::Type { bounds: Vec::new() },
                span: BUILTIN_SPAN,
            }],
            where_clause: Vec::new(),
            fields,
            span: BUILTIN_SPAN,
        }));
    }

    /// `Entry[K, V]`, at the `DefId` `build` already allocated as
    /// `DefKind::Record`: `{ key: K, value: V }`, in that order —
    /// `collections-and-chains.md` §1.3 transcribed:
    ///
    /// ```text
    /// type Entry of (K, V):          # what Map.iterate() yields
    ///     key: K
    ///     value: V
    /// ```
    ///
    /// [`Declarer::ffi_view`]'s shape with two parameters where it has one.
    ///
    /// **The field order is load-bearing, unlike `Span`'s.** `science-rt`'s
    /// `map.rs` stores every pair as this record and hands out a pointer to
    /// it, computing the value's offset from Decision 17's C layout of *key,
    /// then value*. Swapping the two lines here would make every
    /// `entry.value` read the key's bytes.
    fn entry(&mut self, def: DefId) {
        let key = self.defs.alloc(DefKind::TypeParam, "K", BUILTIN_SPAN, Some(def));
        let value = self.defs.alloc(DefKind::TypeParam, "V", BUILTIN_SPAN, Some(def));
        let mut generics = HashMap::new();
        generics.insert("K", key);
        generics.insert("V", value);
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let key_ty = self.ty(&Ty::Var("K"), &scope);
        let value_ty = self.ty(&Ty::Var("V"), &scope);
        let fields = vec![
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "key", BUILTIN_SPAN, Some(def)),
                ty: key_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "value", BUILTIN_SPAN, Some(def)),
                ty: value_ty,
                span: BUILTIN_SPAN,
            },
        ];
        let param = |def| hir::GenericParam {
            def,
            kind: hir::GenericParamKind::Type { bounds: Vec::new() },
            span: BUILTIN_SPAN,
        };
        self.item(hir::ItemKind::Record(hir::Record {
            def,
            generics: vec![param(key), param(value)],
            where_clause: Vec::new(),
            fields,
            span: BUILTIN_SPAN,
        }));
    }

    /// `Pair[A, B]`, at the `DefId` `build` allocated as `DefKind::Record`:
    /// `{ left: A, right: B }` — `collections-and-chains.md` §1.3
    /// transcribed:
    ///
    /// ```text
    /// type Pair of (A, B):           # what zip() yields
    ///     left: A
    ///     right: B
    /// ```
    ///
    /// [`Declarer::entry`]'s shape under other names. A program's own `type
    /// Pair` shadows it, as it does any prelude name.
    fn pair(&mut self, def: DefId) {
        let left = self.defs.alloc(DefKind::TypeParam, "A", BUILTIN_SPAN, Some(def));
        let right = self.defs.alloc(DefKind::TypeParam, "B", BUILTIN_SPAN, Some(def));
        let mut generics = HashMap::new();
        generics.insert("A", left);
        generics.insert("B", right);
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let left_ty = self.ty(&Ty::Var("A"), &scope);
        let right_ty = self.ty(&Ty::Var("B"), &scope);
        let fields = vec![
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "left", BUILTIN_SPAN, Some(def)),
                ty: left_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "right", BUILTIN_SPAN, Some(def)),
                ty: right_ty,
                span: BUILTIN_SPAN,
            },
        ];
        let param = |def| hir::GenericParam {
            def,
            kind: hir::GenericParamKind::Type { bounds: Vec::new() },
            span: BUILTIN_SPAN,
        };
        self.item(hir::ItemKind::Record(hir::Record {
            def,
            generics: vec![param(left), param(right)],
            where_clause: Vec::new(),
            fields,
            span: BUILTIN_SPAN,
        }));
    }

    /// `Numbered[T]`, at the `DefId` `build` allocated as `DefKind::Record`:
    /// `{ index: Int, item: T }` — `collections-and-chains.md` §1.3
    /// transcribed:
    ///
    /// ```text
    /// type Numbered of T:            # what numbered() yields
    ///     index: Int
    ///     item: T
    /// ```
    ///
    /// [`Declarer::entry`]'s shape with one parameter. Unlike `Entry`, no
    /// runtime stores one: `science-mir` builds each as the chain's fused
    /// loop reaches it, so the order is the note's and nothing else's.
    fn numbered(&mut self, def: DefId) {
        let item = self.defs.alloc(DefKind::TypeParam, "T", BUILTIN_SPAN, Some(def));
        let mut generics = HashMap::new();
        generics.insert("T", item);
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let index_ty = self.ty(&INT, &scope);
        let item_ty = self.ty(&Ty::Var("T"), &scope);
        let fields = vec![
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "index", BUILTIN_SPAN, Some(def)),
                ty: index_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "item", BUILTIN_SPAN, Some(def)),
                ty: item_ty,
                span: BUILTIN_SPAN,
            },
        ];
        self.item(hir::ItemKind::Record(hir::Record {
            def,
            generics: vec![hir::GenericParam {
                def: item,
                kind: hir::GenericParamKind::Type { bounds: Vec::new() },
                span: BUILTIN_SPAN,
            }],
            where_clause: Vec::new(),
            fields,
            span: BUILTIN_SPAN,
        }));
    }

    /// `FormatSpec`, at the `DefId` `build` already allocated as
    /// `DefKind::Record`: `{ fill: Char, align: Align?, sign: Sign?, width:
    /// Int?, precision: Int?, code: Code?, alternate: Bool, grouping:
    /// Grouping? }`, in that field order — `strings-formatting-and-docs.md`
    /// §3.1, transcribed rather than decided. §3.1's own words for the
    /// eight: *"a plain record of nullable fields — `fill: Char, align:
    /// Align?, sign: Sign?, width: Int?, precision: Int?, code: Code?,
    /// alternate: Bool, grouping: Grouping?`"*.
    ///
    /// Not routed through [`Declarer::block`], for [`Declarer::ffi_view`]'s
    /// own reason one level up: what is missing is the type's own field
    /// declaration, and `FormatSpec` has no generic parameter to give
    /// [`Declarer::interface`]'s shape a reason either — it is `ffi_view`
    /// minus the one thing `ffi_view` is generic *for*.
    fn format_spec(&mut self, def: DefId) {
        let generics = HashMap::new();
        let assocs = HashMap::new();
        let scope = Scope { generics: &generics, assocs: &assocs, owner: Some(def) };
        let fill_ty = self.ty(&CHAR, &scope);
        let align_ty = self.ty(&Ty::Opt(&Ty::Name("Align")), &scope);
        let sign_ty = self.ty(&Ty::Opt(&Ty::Name("Sign")), &scope);
        let width_ty = self.ty(&Ty::Opt(&INT), &scope);
        let precision_ty = self.ty(&Ty::Opt(&INT), &scope);
        let code_ty = self.ty(&Ty::Opt(&Ty::Name("Code")), &scope);
        let alternate_ty = self.ty(&BOOL, &scope);
        let grouping_ty = self.ty(&Ty::Opt(&Ty::Name("Grouping")), &scope);
        let fields = vec![
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "fill", BUILTIN_SPAN, Some(def)),
                ty: fill_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "align", BUILTIN_SPAN, Some(def)),
                ty: align_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "sign", BUILTIN_SPAN, Some(def)),
                ty: sign_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "width", BUILTIN_SPAN, Some(def)),
                ty: width_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "precision", BUILTIN_SPAN, Some(def)),
                ty: precision_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "code", BUILTIN_SPAN, Some(def)),
                ty: code_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "alternate", BUILTIN_SPAN, Some(def)),
                ty: alternate_ty,
                span: BUILTIN_SPAN,
            },
            hir::Field {
                def: self.defs.alloc(DefKind::Field, "grouping", BUILTIN_SPAN, Some(def)),
                ty: grouping_ty,
                span: BUILTIN_SPAN,
            },
        ];
        self.item(hir::ItemKind::Record(hir::Record {
            def,
            generics: Vec::new(),
            where_clause: Vec::new(),
            fields,
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
    // `FormatSpec` is `DefKind::Record`, not `DefKind::Primitive` like the
    // rest of the list above — it has fields a program can name (§3.1's
    // `fill`, `align`, …) — but it is a bare top-level prelude name unlike
    // `Span`/`MutableSpan`, which live under `ffi.` alone. Declared here,
    // before `Declarer` is built, the same way every other name above is;
    // given its fields below, once `Align`, `Sign`, `Code` and `Grouping`
    // exist for [`Declarer::format_spec`] to name.
    let format_spec_def = declare(defs, DefKind::Record, "FormatSpec", &mut prelude);
    // `Entry[K, V]`, `collections-and-chains.md` §1.3's record — *"what
    // `Map.iterate()` yields"* — declared the way `FormatSpec` is and for its
    // reason: a program names its fields, `entry.key` and `entry.value`.
    let entry_def = declare(defs, DefKind::Record, "Entry", &mut prelude);
    // `Numbered[T]`, §1.3's other record that `numbered()` yields, for
    // `Entry`'s reason: a program names `item.index` and `item.item`.
    let numbered_def = declare(defs, DefKind::Record, "Numbered", &mut prelude);
    // `Pair[A, B]`, what `zip()` yields, for the same reason.
    let pair_def = declare(defs, DefKind::Record, "Pair", &mut prelude);
    // The second spelling of a primitive that has two. No `alloc`: the whole
    // point is that `Int` and `I64` are one `DefId` and therefore one `Ty`,
    // which is what makes them unify. Pushing a name is all a second spelling
    // is, because `Prelude::names` is a lookup table and not a definition list.
    for (alias, canonical) in ALIASED_PRIMITIVES {
        let id = prelude
            .names
            .iter()
            .find(|(name, _)| name == canonical)
            .map(|(_, id)| *id)
            .expect("a canonical primitive is declared above");
        prelude.names.push(((*alias).to_string(), id));
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
    // `Span` and `MutableSpan` are `DefKind::Record`, not `DefKind::Primitive`
    // like the rest of this list — see `FFI_TYPES`'s comment. Their `DefId`s
    // are captured here because `build`'s `Declarer` only resolves a name
    // through `prelude.names`, which `ffi`'s own names never join (the test
    // `the_ffi_names_are_not_also_bare_prelude_names` is exactly this), so the
    // fields have to be declared against the `DefId` directly rather than by
    // a name lookup.
    let mut span_def = None;
    let mut mutable_span_def = None;
    for name in FFI_TYPES {
        let kind = match *name {
            "Span" | "MutableSpan" => DefKind::Record,
            _ => DefKind::Primitive,
        };
        let id = defs.alloc(kind, *name, BUILTIN_SPAN, Some(ffi));
        match *name {
            "Span" => span_def = Some(id),
            "MutableSpan" => mutable_span_def = Some(id),
            _ => {}
        }
        ffi_names.push((name.to_string(), id));
    }
    let span_def = span_def.expect("`Span` is in `FFI_TYPES`");
    let mutable_span_def = mutable_span_def.expect("`MutableSpan` is in `FFI_TYPES`");
    for name in FFI_INTERFACES {
        let id = defs.alloc(DefKind::Interface, *name, BUILTIN_SPAN, Some(ffi));
        ffi_names.push((name.to_string(), id));
    }
    prelude.modules.push((ffi, ffi_names));

    let mut choice_ids: Vec<(DefId, Vec<DefId>)> = Vec::with_capacity(CHOICES.len());
    for (name, variants) in CHOICES {
        let choice_id = declare(defs, DefKind::Choice, name, &mut prelude);
        let mut variant_ids = Vec::with_capacity(variants.len());
        for (variant, arity) in *variants {
            let id = defs.alloc(DefKind::Variant, *variant, BUILTIN_SPAN, Some(choice_id));
            prelude.variants.push((variant.to_string(), id));
            prelude.variant_arity.push((id, *arity));
            variant_ids.push(id);
        }
        choice_ids.push((choice_id, variant_ids));
    }

    // Every name above exists before a single signature is written, which is
    // what lets a declaration name any of them without an ordering rule.
    let ffi_scope: HashMap<String, DefId> = prelude
        .modules
        .iter()
        .find(|(id, _)| *id == ffi)
        .map(|(_, names)| names.iter().cloned().collect())
        .expect("`ffi` was pushed onto `prelude.modules` above");
    let mut declarer = Declarer {
        defs,
        module,
        names: prelude.names.iter().cloned().collect(),
        ffi_names: ffi_scope,
        items: Vec::new(),
    };
    // `ffi.Span` and `ffi.MutableSpan` get their fields before anything below
    // has a chance to ask `needs_drop` about one.
    declarer.ffi_view(span_def, false);
    declarer.ffi_view(mutable_span_def, true);
    // `FormatSpec` gets its fields the same way, once `Align`, `Sign`,
    // `Code` and `Grouping` — named by [`CHOICES`], processed above — exist
    // for it to name.
    declarer.format_spec(format_spec_def);
    // ...and the four `choice`s themselves get an item, so that
    // `science-types`' declaration table has a payload list for each variant.
    // Every variant is payload-free, so the list is empty, but **absent is
    // not empty**: `science-codegen-llvm`'s `choice_ty` refuses a variant the
    // table has no entry for — *"the variant `Align.Left`, which the
    // declaration table has no lowered payload for"* — and that refusal is
    // what `into.spec()` met the first time a program read a `FormatSpec`'s
    // `align` off a hole's spec. The `DefId`s were allocated above; this
    // allocates none, so no other id moves.
    for (choice_id, variant_ids) in choice_ids {
        declarer.item(hir::ItemKind::Choice(hir::Choice {
            def: choice_id,
            generics: Vec::new(),
            where_clause: Vec::new(),
            variants: variant_ids
                .into_iter()
                .map(|def| hir::Variant { def, payload: Vec::new(), span: BUILTIN_SPAN })
                .collect(),
            span: BUILTIN_SPAN,
        }));
    }
    declarer.entry(entry_def);
    declarer.numbered(numbered_def);
    declarer.pair(pair_def);
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
    fn indexing_is_declared_the_way_section_1_1_writes_it() {
        // Asserted as *text*, for `map_get_borrows_its_map_and_not_its_key`'s
        // reason: `science-types` reads this through three crates and a test at
        // the far end would not say which spelling it was reading.
        //
        // `indexing-and-array-literals.md` §1.1, Decision 2:
        //
        //     interface Index[Idx]:
        //         type Output
        //         def index(self, at: Idx) -> &Self.Output
        //
        //     interface IndexMutably[Idx]:
        //         type Output
        //         def index_mutably(mutable self, at: Idx)
        //             -> &mut Self.Output
        let read = INTERFACE_DECLS.iter().find(|d| d.name == "Index").expect("`Index`");
        assert_eq!(read.generics, &["Idx"]);
        assert_eq!(read.assoc, &["Output"]);
        let index = read.methods.iter().find(|m| m.name == "index").expect("`Index.index`");
        assert!(matches!(index.recv, Some(SelfKind::Shared)));
        assert!(matches!(index.params, [("at", Ty::Var("Idx"))]));
        assert!(matches!(index.ret, Some(Ty::Ref(Ty::Assoc("Output")))));

        let write =
            INTERFACE_DECLS.iter().find(|d| d.name == "IndexMutably").expect("`IndexMutably`");
        assert_eq!(write.generics, &["Idx"]);
        assert_eq!(write.assoc, &["Output"]);
        let index_mutably = write
            .methods
            .iter()
            .find(|m| m.name == "index_mutably")
            .expect("`IndexMutably.index_mutably`");
        assert!(matches!(index_mutably.recv, Some(SelfKind::Mutable)));
        assert!(matches!(index_mutably.params, [("at", Ty::Var("Idx"))]));
        assert!(matches!(index_mutably.ret, Some(Ty::MutRef(Ty::Assoc("Output")))));
    }

    #[test]
    fn an_array_is_indexed_by_an_int_and_yields_its_element() {
        // `Output is T` and not `&T`: the borrow is in the interface's
        // return, so writing it here too would make `a[i]` a
        // `&&T`.
        let blocks: Vec<&Block> = BLOCKS
            .iter()
            .filter(|block| {
                block.ty == "Array"
                    && matches!(block.interface, Some(("Index" | "IndexMutably", _)))
            })
            .collect();
        assert_eq!(blocks.len(), 2, "`Array` implements both halves of Decision 2");
        for block in blocks {
            assert!(matches!(block.interface, Some((_, [Ty::Name("Int")]))));
            assert!(matches!(block.assoc, [("Output", Ty::Var("T"))]));
            // The method is the interface's, contributed by `methods`' §2. A
            // second declaration here would be one the prelude has no body for.
            assert!(block.methods.is_empty());
        }
    }

    #[test]
    fn a_map_is_not_indexable() {
        // §1.1 never names `Map`, and §1.3's four discharge rules are all about
        // an *extent* — none of them can speak about a key. `Map.get` returns
        // `(&V)?` and says so. Pinned, so that adding `Map implements
        // Index[K]:` is a decision somebody takes rather than a line somebody
        // adds.
        assert!(!BLOCKS.iter().any(|block| {
            block.ty == "Map" && matches!(block.interface, Some(("Index" | "IndexMutably", _)))
        }));
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
