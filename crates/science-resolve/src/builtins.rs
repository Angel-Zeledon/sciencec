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
const LIBRARY_TYPES: &[&str] =
    &["Array", "Map", "Box", "Chars", "Range", "IoError", "TextError"];

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
];

/// The free functions (§8).
///
/// Five, not `stdlib-core.md` §9's thirteen. The eight this list does not have
/// — `print_error`, `write_error`, `flush`, `read_line`, `read_bytes`,
/// `write_bytes`, `read_lines`, `write_lines` — are names in every program's
/// scope forever (§1.3), and §12 of that note names *"thirteen free functions,
/// and the trend is upward"* as its own second risk.
///
/// # Three of the eight stopped being a spec change, and they still do not go in
///
/// This comment used to rest its whole case on *"adding eight global names is a
/// spec change under §2.2's rule"*. **For three of them that is no longer
/// true.** `strings-formatting-and-docs.md` §4.3 makes the change and §8 item 5
/// asks §8 for it by name: the list *"becomes `print`, `write`, `print_error`,
/// `write_error`, `flush`, `panic`, `read_file`, `write_file`"*, and
/// `stdlib-core.md` §4.1 restates the whole of it as settled — *"`print_error`
/// and `write_error` go to stderr, `flush` is a free function … none of that is
/// reopened"*. So the decision this file would have been taking has been taken
/// elsewhere, by the note that owns the surface, and the question left here is
/// not *may they* but *can they mean anything yet*.
///
/// **Decision: they stay off, and the reason is `science-rt`, not the
/// namespace.** A name on this list resolves; a name that resolves with no
/// entry point behind it is checked in silence and refused at the end of the
/// build — `science-codegen-llvm` answers *"a call to `write_error`, which this
/// crate was given no MIR body for"*, after the link the user waited for.
/// Today `print_error("…")` is `SC0201`, at the call, with the name in the
/// message, and it is **true**: this compiler has no `print_error`. Trading a
/// true early diagnostic for a promise redeemed late is not a trade the prelude
/// can make on its own.
///
/// **Why the runtime cannot be assumed to have them.** `science-rt`'s `exit`
/// module has `science_write_error_bytes`, and it is **codegen support**: its
/// own note says it takes bytes rather than a `String` because the message is a
/// constant in the binary, and *"there is no
/// `science_write_error(text: *const ScienceString)` beside this one, because
/// codegen has no call site for it"*. It is a writer for the emitted `main`,
/// not a Science name wearing a different spelling. There is no `science_flush`
/// at all — `science_exit` flushes on the way out and nothing else does — and
/// §4.2's buffering policy (stdout line-buffered on a terminal, 64 KiB
/// block-buffered otherwise, stderr unbuffered) is not implemented: Rust's
/// `LineWriter` line-buffers unconditionally, which is a different policy that
/// happens to agree in the terminal case.
///
/// # The handover, exactly
///
/// Three entry points in `science-rt`, each the stderr or flush twin of a
/// symbol that already exists, each then added to `science-codegen`'s `RUNTIME`
/// table and given an arm beside `lower_print`:
///
/// - `science_print_error(text: *const ScienceString)` — `science_write_error`
///   then one `\n`, which is `science_print`'s relation to `science_write`.
/// - `science_write_error(text: *const ScienceString)` — the `ScienceString`
///   form of `science_write_error_bytes`, unbuffered per §4.2.
/// - `science_flush()` — flushes standard output. §4.2 adds `flush`
///   *"reluctantly"* for exactly one case, a `write` of a progress line with no
///   newline, and that case is live today because `science_write` goes through
///   the same `LineWriter`.
///
/// When those three exist, this list is a three-name edit and nothing else here
/// moves — the names carry no signature, for the reason `print` and `write`
/// carry none.
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
/// Empty since revision 2 §3. `Option[T]` became `T?` and `Result[T, E]`
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
    /// A prelude type applied to arguments: `Array[T]`.
    App(&'static str, &'static [Ty]),
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

/// The interfaces that get a method, and why only five do.
///
/// **Decision. An interface is declared with its methods only where a note
/// gives the method's name and its types.** `Error.message` is
/// `stdlib-core.md` §7.5 verbatim; `Iterate.next` is the one
/// `collections-and-chains.md` builds its chain vocabulary on; `Index.index`
/// and `IndexMutably.index_mutably` are `indexing-and-array-literals.md`
/// §1.1's Decision 2, written out in Science in that note and transcribed
/// below; `Clone.clone` is the fifth, and its own comment below says why it
/// passes the same test although no note writes its signature in Science
/// syntax. The other fourteen — `Add`, `Ord`, `Eq`, `Display` and the rest —
/// are declared as **names with implementations and no methods**, which is the
/// whole of what the bound check needs: `methods`' §7 asks *"does `I64`
/// implement `Ord`"* and never *"what is `Ord`'s method called"*.
///
/// **The cost, stated: two operators still do not dispatch.** `check`'s §6
/// wants *"a rule anywhere saying which method name each operator dispatches
/// to"*, and for `< > <= >=` and for `print` there is none. Writing
/// `Ord.compare -> Ordering` or `Display.display(Formatter)` here would invent
/// `Ordering` and `Formatter` — two Level 1 types no note has specified,
/// arriving as a side effect of a bound check — and `Ord` would need a third
/// thing besides: a rule for how four operators sit over one `compare`,
/// including what `F64`'s NaN does to a total order. A signature invented in
/// passing is how a language acquires a design nobody argued for, so the
/// fourteen stay methodless.
///
/// **What the fourteen do buy, now that the implementations are declared**, is
/// the *requirement*: `check`'s `implements_operand` refuses `a < b` on a type
/// that has no `implements Ord:` block without ever naming `Ord`'s method. The
/// hole that is left is the dispatch and only the dispatch.
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
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(STRING),
        }],
    },
    InterfaceDecl {
        name: "Iterate",
        generics: &[],
        assoc: &["Item"],
        methods: &[Method {
            name: "next",
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
            recv: Some(SelfKind::Shared),
            params: &[],
            ret: Some(Ty::SelfTy),
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
            // §3.6 verbatim, parentheses and all: `&T?` would read as
            // `&(T?)`.
            Method {
                name: "get",
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
                recv: Some(SelfKind::Mutable),
                params: &[],
                ret: Some(Ty::Opt(&Ty::Var("T"))),
            },
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
            // `Array.new`'s decision, one arity up, with the same citation
            // (none) and the same cost. `examples/09` and `examples/12` write
            // `(Map[String, String]).new()`, which is the form that
            // compiles; a bare `Map.new()` is `SC0536` naming both `K` and `V`.
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
            recv: None,
            params: &[("value", Ty::Var("T"))],
            ret: Some(Ty::App("Box", &[Ty::Var("T")])),
        }],
    },
    // --- String, §6.9 -----------------------------------------------------
    //
    // Thirteen of the note's nineteen, and `new` is the one of the thirteen
    // that a note declares in full: §6.9's first line is `def new() -> String`,
    // so it is transcription and not a decision.
    //
    // The five left out, with the reason each is out — and the grouping is
    // finer than it was, because two of them were being refused for a reason
    // that is not theirs:
    //
    // - `slice`, `lines` and `split` return `Range`, `Lines` and `Split` —
    //   Level 1 types §9 lists and the prelude does not have.
    // - `from_bytes` and `bytes` are **expressible today**: `&Array of
    //   U8`, `(String, TextError?)` and `&Array[U8]` name nothing the
    //   prelude lacks. They stay out for a different reason, which is that no
    //   program in `examples/` calls either, so landing them would be landing a
    //   signature the acceptance corpus cannot measure. `from_bytes` is now
    //   *reachable* — `science-types` accepts a prelude type as the receiver of
    //   an associated call, which is what `String.new()` needed — so the day
    //   the corpus writes one, the declaration is a transcription of §6.9 and
    //   two lines.
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
                recv: Some(SelfKind::Mutable),
                params: &[("bytes", INT)],
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
    // **`Map` is *not* here, and its absence is a finding.**
    // `collections-and-chains.md` §1.3 says what `Map.iterate()` yields and it
    // is not a `V` and not a tuple: *"`type Entry[K, V]: key: K; value:
    // V`"*, with §1.3's whole argument being that *"pairs are records, never
    // tuples"* because `each.key` works with §4.6's implicit subject and
    // `each.0` does not. `Entry` is not in §8's closed library, this table
    // declares names and methods and cannot give a record its *fields*, and the
    // two spellings that are expressible are both wrong: `Item is &V`
    // throws the key away, and `Item is (&K, &V)` is the tuple
    // §1.3 refuses. `examples/10_loops.science` already walks a map through an
    // `Array` of its keys and says in a comment that it does so because §8 does
    // not give `Map` an `Iterate`. So the honest declaration is none, and what
    // it waits for is `Entry[K, V]` as a Level 1 record.
    Block {
        ty: "Array",
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
    // **`Map` is deliberately absent, and it is the same finding
    // `Map`'s missing `Iterate` is.** `Map[K, V] implements Index[K]`
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
            recv: Some(SelfKind::Mutable),
            params: &[],
            ret: Some(Ty::Opt(&CHAR)),
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
];

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
    // `String` — the five of `stdlib-core.md` §6.9's nineteen that the
    // `String` block above leaves out. That block's own comment names all
    // five and gives the reason for each:
    // `slice`/`lines`/`split` because they return
    // Level 1 types the prelude does not have, and `from_bytes`/`bytes`
    // because no program in `examples/` calls either.
    ("String", &[
        "slice", "lines", "split", "from_bytes", "bytes",
        // `clone` retired from this list the day `INTERFACE_DECLS` gained
        // `Clone.clone`: §6.2's *"`.owned()` and `.clone()` are both written"*
        // is now true of the first half. `owned()` stays — it is
        // `collections-and-chains.md` §1.4's chain terminal, returning an
        // `Owned of Self`, and `Owned` is a Level 1 type this prelude does
        // not have, which is `slice`'s case and not `clone`'s.
        "owned",
    ]),
    // `Array` — every name either note gives it that the block above does not
    // have. None of these has a full signature in a note, which is the reason
    // they are not transcribed: `pop` is `stdlib-core.md` §3.2 (*"`Array` has
    // `push` and `pop` and no `pop_front`"*) as a name only, `sort` is
    // `collections-and-chains.md` §3.2 and §3.3, `reserve` is §5.3. The three
    // `iterate*` are §5.4 and they *do* have signatures — they return
    // `ArrayIterate[T]` and its two siblings, which are types the prelude
    // does not have, so they are `slice`'s case one type along.
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
        "sort", "reserve", "iterate", "iterate_mutably", "iterate_consuming", "clone",
    ]),
    // `Map` — `keys`, `values` and `values_mutably` are
    // `collections-and-chains.md` §5.4 by name; the three `iterate*` are the
    // same section's requirement of *"every collection"*; `from` is §5.5's
    // `Map.from(..)`.
    (
        "Map",
        &[
            "keys",
            "values",
            "values_mutably",
            "iterate",
            "iterate_mutably",
            "iterate_consuming",
            "from",
            // The `Array` row's `clone`, for its reason.
            "clone",
        ],
    ),
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
///   open until `Iterate` carries them.
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
    // The numeric primitives, `Bool` and `Char`, which `Clone.clone` gave an
    // index entry and therefore a closed surface they have no note for.
    "I8", "I16", "I32", "I64", "U8", "U16", "U32", "U64", "F16", "BF16", "F32", "F64", "Int",
    "Float", "Bool", "Char",
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
/// **`print` and `write` are deliberately left undeclared**, and this is the
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
            Ty::MutRef(inner) => {
                hir::TypeKind::Borrowed { mutable: true, inner: Box::new(self.ty(inner, scope)) }
            }
            Ty::Opt(inner) => hir::TypeKind::Nullable(Box::new(self.ty(inner, scope))),
            Ty::Pair(left, right) => {
                hir::TypeKind::Tuple(vec![self.ty(left, scope), self.ty(right, scope)])
            }
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
    // `ffi.Span` and `ffi.MutableSpan` get their fields before anything below
    // has a chance to ask `needs_drop` about one.
    declarer.ffi_view(span_def, false);
    declarer.ffi_view(mutable_span_def, true);
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
