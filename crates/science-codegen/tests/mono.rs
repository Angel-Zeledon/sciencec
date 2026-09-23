//! The monomorphisation walk, run over real programs.
//!
//! **The whole pipeline, and a hand-built MIR nowhere.** `science-mir`'s
//! harness gives the argument and it applies twice over here: the thing under
//! test is which instantiations the *checker's* output reaches, so a fixture
//! body would be testing this crate against a guess about what the checker
//! produces. Every test below lexes, parses, resolves, checks and lowers real
//! Science.
//!
//! The four claims this file exists to hold:
//!
//! 1. **The output is reproducible.** `the_same_crate_lowers_to_the_same_bytes`
//!    runs the whole pipeline twice, from two fresh `Types` tables and two
//!    fresh `DefTable`s, and compares the rendered set. A `HashMap` iteration
//!    order leaking into the answer fails it.
//! 2. **File order does not reach a symbol.** `normal.rs` §2's failure was
//!    exactly this and it was found by argument rather than by test; here it is
//!    a test.
//! 3. **The walk terminates.** A generic that calls itself at a larger type is
//!    `SC0407`, and the message is the chain.
//! 4. **Two instantiations are two symbols.** Including the two
//!    `crate::mangle`'s `CgTy` encoder collapses.

use science_codegen::diagnostics::code;
use science_codegen::mono::{Instance, Mono, MonoSet, RootSet, Unsolved};
use science_diagnostics::{Diagnostics, FileId};
use science_mir::mir::Body;
use science_resolve::hir;
use science_types::items::Declarations;
use science_types::ty::GenericArg;
use science_types::{check_crate, thir, Aliases, AtomOrder, NormalForm, Types};

// --- the harness ----------------------------------------------------------

struct Lowered {
    krate: hir::Crate,
    types: Types,
    decls: Declarations,
    bodies: Vec<Body>,
}

/// One program, all the way to MIR.
fn lower(source: &str) -> Lowered {
    lower_at(FileId(0), "mono.science", source)
}

fn lower_at(file: FileId, name: &str, source: &str) -> Lowered {
    let (tokens, lexed) = science_lexer::lex(file, source);
    assert!(!lexed.has_errors(), "the fixture must lex: {:?}", codes(&lexed));
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    assert!(!parsed.has_errors(), "the fixture must parse: {:?}", codes(&parsed));
    let (krate, resolution) = science_resolve::resolve_module(file, name, &ast);
    assert!(!resolution.has_errors(), "the fixture must resolve: {:?}", codes(&resolution));

    let order = AtomOrder::of(&krate.defs);
    let mut types = Types::new();
    let mut diagnostics = Diagnostics::new();
    let mut aliases = Aliases::of(&krate, &mut types, &order, &mut diagnostics);
    let decls = Declarations::of(&krate, &mut types, &order, &mut diagnostics);
    let thir: Vec<thir::Body> =
        check_crate(&krate, &decls, &mut types, &mut aliases, &order, &mut diagnostics);
    let bodies = {
        let mut context = science_mir::Context {
            defs: &krate.defs,
            decls: &decls,
            types: &mut types,
            aliases: &mut aliases,
        };
        science_mir::lower_crate(&mut context, &thir)
    };
    Lowered { krate, types, decls, bodies }
}

impl Lowered {
    fn mono(&mut self, roots: RootSet) -> MonoSet {
        let mut walk =
            Mono::new(&self.krate.defs, &self.decls, &mut self.types, &self.bodies);
        walk.collect(roots)
    }
}

fn codes(diagnostics: &Diagnostics) -> Vec<u16> {
    diagnostics.iter().map(|d| d.code.0).collect()
}

fn corpus() -> Vec<(String, String)> {
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

/// Whether a source reaches this crate at all: the corpus is guaranteed to
/// lex, parse and resolve, and nothing more.
fn is_clean(source: &str) -> bool {
    let file = FileId(0);
    let (tokens, lexed) = science_lexer::lex(file, source);
    if lexed.has_errors() {
        return false;
    }
    let (ast, parsed) = science_parser::parse_module(&tokens, file);
    if parsed.has_errors() {
        return false;
    }
    let (_, resolution) = science_resolve::resolve_module(file, "example.science", &ast);
    !resolution.has_errors()
}

// --- 1. reproducibility ---------------------------------------------------

const TWO_GENERICS: &str = "\
type Pair[A, B]:
    first: A
    second: B

def identity[T](value: T) -> T:
    value

def pair_up[A, B](left: A, right: B) -> Pair[A, B]:
    Pair(first: left, second: right)

def main():
    let a: Int be identity_int()
    let b: Float be identity_float()
    let p be pair_up(a, b)
    let q be pair_up(b, a)

def identity_int() -> Int:
    let n: Int be 1
    identity(n)

def identity_float() -> Float:
    let x: Float be 2.0
    identity(x)
";

#[test]
fn the_same_crate_lowers_to_the_same_bytes() {
    // The test the module documentation says is worth more than an assertion
    // about a data structure: run the *whole* pipeline twice, from two fresh
    // tables, and compare the rendered set byte for byte. Every `HashMap` in
    // the walk and in every phase below it gets a fresh allocation and a fresh
    // iteration order; if one of them reached the answer, this fails.
    let once = lower(TWO_GENERICS).mono(RootSet::EntryPoint).render();
    let twice = lower(TWO_GENERICS).mono(RootSet::EntryPoint).render();
    assert_eq!(once, twice);
    assert!(!once.is_empty(), "the fixture has to actually produce items");
}

#[test]
fn the_whole_corpus_lowers_to_the_same_bytes_twice() {
    // The same claim over every program in `examples/`, which is the only
    // place in this workspace where the input is not a fixture written by the
    // person writing the assertion.
    let mut checked = 0;
    for (name, source) in corpus() {
        if !is_clean(&source) {
            continue;
        }
        let once = lower(&source).mono(RootSet::EveryBody).render();
        let twice = lower(&source).mono(RootSet::EveryBody).render();
        assert_eq!(once, twice, "{name} is not reproducible");
        checked += 1;
    }
    assert!(checked >= 15, "only {checked} examples reached the walk");
}

#[test]
fn the_file_id_does_not_reach_a_symbol() {
    // `normal.rs` §2's failure, as a test rather than as an argument: the atom
    // order was `DefId`, `DefId` is allocation order, and `FileId`s are handed
    // out in command-line order. A symbol that moved with the invocation is
    // the thing `reproducibility.md` exists to forbid.
    let first = lower_at(FileId(0), "a.science", TWO_GENERICS)
        .mono(RootSet::EntryPoint)
        .render();
    let seventh = lower_at(FileId(7), "z.science", TWO_GENERICS)
        .mono(RootSet::EntryPoint)
        .render();
    assert_eq!(first, seventh);
}

#[test]
fn emission_order_is_sorted_by_mangled_name() {
    // Decision 4, and it is the map's order rather than a sort somebody can
    // delete. Asserting it is still worth a test, because the day somebody
    // swaps the `BTreeMap` for a `Vec` this is what says no.
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EveryBody);
    let symbols: Vec<&str> = set.symbols().collect();
    let mut sorted = symbols.clone();
    sorted.sort_unstable();
    assert_eq!(symbols, sorted);
    assert!(symbols.iter().all(|symbol| symbol.starts_with("_S")), "{symbols:?}");
}

#[test]
fn no_symbol_contains_a_def_id_or_a_hash() {
    // §5 item 2 and Decision 16. The `#` is the only number-bearing escape a
    // symbol may carry — the implementation-block disambiguator — and this
    // fixture has no implementation block, so there should be none at all.
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EveryBody);
    for symbol in set.symbols() {
        assert!(!symbol.contains('#'), "{symbol}");
    }
}

// --- 2. the walk finds the right instances --------------------------------

#[test]
fn one_generic_called_at_two_types_is_two_items() {
    // The whole point of monomorphisation, and the smallest test of it.
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EntryPoint);
    let identity: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("identity["))
        .map(|item| item.description.as_str())
        .collect();
    assert_eq!(identity.len(), 2, "{identity:?}");
    assert!(identity.iter().any(|d| d.contains("I64")), "{identity:?}");
    assert!(identity.iter().any(|d| d.contains("F64")), "{identity:?}");
}

#[test]
fn the_same_generic_at_the_same_type_is_one_item() {
    // The fixed point: `pair_up` is called twice at `(I64, F64)` in a loop-free
    // body and once at `(F64, I64)`, so the set has two and not three.
    let source = "\
def take[T](value: T) -> T:
    value

def main():
    let n: Int be 1
    let a be take(n)
    let b be take(n)
    let c be take(n)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let takes = set.emission_order().filter(|item| item.description.starts_with("take")).count();
    assert_eq!(takes, 1, "{}", set.render());
}

#[test]
fn nested_arguments_are_recovered_from_the_call_site() {
    // §2: the checker's own `instantiate_call` is root-level only, so `T` under
    // an `Array of` is `Ty::ERROR` to it. This pass reads the argument's
    // *lowered* type and gets the answer, which is the reason the recovery is
    // worth having rather than a workaround.
    let source = "\
type Holder[T]:
    item: T

def unwrap[T](holder: Holder[T]) -> T:
    holder.item

def main():
    let n: Int be 3
    let h be Holder(item: n)
    let v be unwrap(h)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let unwrapped: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("unwrap"))
        .map(|item| item.description.as_str())
        .collect();
    assert_eq!(unwrapped.len(), 1, "{}", set.render());
    assert!(unwrapped[0].contains("I64"), "{unwrapped:?}");
}

#[test]
fn an_uncalled_generic_is_not_emitted() {
    // The root set doing its job: a generic nothing reaches has no instance,
    // and a monomorphiser that emitted one would have had to invent arguments.
    let source = "\
def used[T](value: T) -> T:
    value

def never_used[T](value: T) -> T:
    value

def main():
    let n: Int be 1
    let a be used(n)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(
        !set.render().contains("never_used"),
        "an unreachable generic has no instantiation: {}",
        set.render()
    );
}

#[test]
fn a_function_named_as_a_value_is_a_root() {
    // §7's cost: an address-taken function is reachable and the entry point
    // does not call it. If this regresses the failure is a link error at the
    // end of a long build, which is why it has a test of its own.
    let source = "\
def helper(x: Int) -> Int:
    x

def main():
    let f be helper
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(set.render().contains("helper"), "{}", set.render());
}

#[test]
fn a_library_root_set_emits_more_than_a_binary_one() {
    // §7. The two answers that are available, and the gap between them is what
    // a `public` root set would have cost if `public` survived the resolver.
    let source = "\
def a() -> Int:
    1

def b() -> Int:
    2

def main():
    let x be a()
";
    let mut lowered = lower(source);
    let binary = lowered.mono(RootSet::EntryPoint).len();
    let library = lowered.mono(RootSet::EveryBody).len();
    assert_eq!(binary, 2, "main and a");
    assert_eq!(library, 3, "main, a and b");
}

// --- 3. termination -------------------------------------------------------

#[test]
fn a_generic_that_grows_its_own_argument_is_sc0407() {
    // §6's case, and the reason the pass has a termination rule at all:
    // `grow of T` calling `grow of (Holder of T)` needs `grow[Int]`,
    // `grow[Holder of Int]`, `grow[Holder of (Holder of Int)]`, forever.
    let source = "\
type Holder[T]:
    item: T

def grow[T](value: T) -> Int:
    let wrapped be Holder(item: value)
    grow(wrapped)

def main():
    let seed: Int be 1
    let n be grow(seed)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let reported: Vec<u16> = set.diagnostics().iter().map(|d| d.code.0).collect();
    assert!(reported.contains(&code::SC0407.0), "{reported:?}\n{}", set.render());
}

#[test]
fn sc0407_prints_the_chain_and_the_chain_grows() {
    let source = "\
type Holder[T]:
    item: T

def grow[T](value: T) -> Int:
    let wrapped be Holder(item: value)
    grow(wrapped)

def main():
    let seed: Int be 1
    let n be grow(seed)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let diagnostic = set
        .diagnostics()
        .iter()
        .find(|d| d.code == code::SC0407)
        .expect("SC0407");
    let notes = diagnostic.notes.join("\n");
    // The chain, not a depth: every link is an instantiation the walk took,
    // and the reader can see the growth by reading down the list.
    assert!(notes.contains("grow[I64]"), "{notes}");
    assert!(notes.contains("Holder[I64]"), "{notes}");
    assert!(notes.contains("contains an earlier one"), "{notes}");
}

#[test]
fn ordinary_recursion_terminates_and_is_one_item() {
    // The rule must not fire on a recursive function that does *not* grow,
    // which is every recursive function anybody writes. This is the test that
    // says the termination rule is not a recursion ban.
    let source = "\
def countdown[T](value: T, n: Int) -> Int:
    if n <= 0: 0 else: countdown(value, n - 1)

def main():
    let seed: Int be 1
    let n: Int be 10
    let x be countdown(seed, n)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(
        set.diagnostics().iter().all(|d| d.code != code::SC0407),
        "plain recursion is not growth: {}",
        set.render()
    );
    let count =
        set.emission_order().filter(|item| item.description.starts_with("countdown")).count();
    assert_eq!(count, 1, "{}", set.render());
}

#[test]
fn a_non_generic_cycle_terminates() {
    let source = "\
def ping(n: Int) -> Int:
    if n <= 0: 0 else: pong(n - 1)

def pong(n: Int) -> Int:
    ping(n - 1)

def main():
    let n: Int be 4
    let x be ping(n)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(set.diagnostics().is_empty(), "{:?}", codes_of(&set));
    assert_eq!(set.len(), 3);
}

fn codes_of(set: &MonoSet) -> Vec<u16> {
    set.diagnostics().iter().map(|d| d.code.0).collect()
}

// --- 4. the symbol is injective -------------------------------------------

#[test]
fn two_pointer_instantiations_are_two_symbols() {
    // §4. Through `mangle::encode_ty` both of these are `Pb`, because a `CgTy`
    // pointer carries no pointee — one symbol for two instantiations, which is
    // `SC0404` raised against a program with nothing wrong with it. This pass
    // encodes the checker's `Ty`, so they are two.
    let source = "\
type Boxed[T]:
    item: T

def unwrap[T](b: Boxed[T]) -> T:
    b.item

def main():
    let n: Int be 1
    let f: Float be 2.0
    let i be Boxed(item: n)
    let s be Boxed(item: f)
    let a be unwrap(i)
    let c be unwrap(s)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let symbols: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("unwrap"))
        .map(|item| item.symbol.as_str())
        .collect();
    assert_eq!(symbols.len(), 2, "{}", set.render());
    assert_ne!(symbols[0], symbols[1]);
}

#[test]
fn a_box_of_a_type_and_a_box_of_an_object_are_two_symbols() {
    // §4, for the pair `science-types`' `assign` §4a made reachable without an
    // explicit annotation. `Box of Doc` and `Box of any Summarize` are now two
    // types one expression can have — the second is what the first coerces to —
    // so a walk that gave them one symbol would emit one body for two layouts:
    // a thin pointer and a fat one.
    //
    // Through `mangle::encode_ty` they are `Pb` and `D`, which happens to
    // differ; the pair that encoder *does* collapse is `borrowed any I` against
    // `Box of any I`, both `D`, and the `needs_drop` note in `descriptor` is
    // where that is written down. This pass does not use that encoder at all,
    // and what it distinguishes is the interface by canonical path — which is
    // the property `SC0404` would otherwise fire on.
    let source = "\
interface Summarize:
    def summarize(self) -> String

type Doc:
    title: String

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

def hold[T](value: Box[T]) -> Bool:
    true

def main():
    let concrete be hold(Box.new(Doc(title: \"a\")))
    let erased: Box[any Summarize] be Box.new(Doc(title: \"b\"))
    let dynamic be hold(erased)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let symbols: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("hold"))
        .map(|item| item.symbol.as_str())
        .collect();
    assert_eq!(symbols.len(), 2, "{}", set.render());
    assert_ne!(symbols[0], symbols[1]);
    assert!(codes_of(&set).is_empty(), "{}", set.render());
}

#[test]
fn no_symbol_collision_is_reported_over_the_corpus() {
    // `SC0404` is an internal consistency check: it firing on real code means
    // the mangler is wrong, not the program. Running it over every example is
    // the cheapest place to find that out.
    for (name, source) in corpus() {
        if !is_clean(&source) {
            continue;
        }
        let mut lowered = lower(&source);
        let set = lowered.mono(RootSet::EveryBody);
        let collisions: Vec<&science_diagnostics::Diagnostic> =
            set.diagnostics().iter().filter(|d| d.code == code::SC0404).collect();
        assert!(collisions.is_empty(), "{name}: {collisions:?}");
    }
}

#[test]
fn a_method_symbol_names_the_type_it_is_on() {
    // Two methods of the same name on two types must be two symbols, and the
    // component that separates them is the self type's name because an
    // implementation block has none of its own.
    let source = "\
type Doc:
    n: Int

type Row:
    n: Int

Doc has:
    def get(self) -> Int:
        self.n

Row has:
    def get(self) -> Int:
        self.n

def main():
    let one: Int be 1
    let two: Int be 2
    let d be Doc(n: one)
    let r be Row(n: two)
    let a be d.get()
    let b be r.get()
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EveryBody);
    let rendered = set.render();
    assert!(rendered.contains("Doc.get"), "{rendered}");
    assert!(rendered.contains("Row.get"), "{rendered}");
    let symbols: Vec<&str> = set.symbols().collect();
    let unique: std::collections::BTreeSet<&str> = symbols.iter().copied().collect();
    assert_eq!(symbols.len(), unique.len(), "{rendered}");
}

// --- 5. Decision 18 -------------------------------------------------------

#[test]
fn a_generic_passed_to_an_extern_function_is_sc0522() {
    // §9. Decision 18 was reserved in `science-types` and enforced nowhere;
    // this is the check, at the one shape the walk can see.
    let source = "\
unsafe extern \"C\" library \"m\":
    def install(callback: ffi.FunctionPointer) -> Int

def identity[T](value: T) -> T:
    value

def main():
    unsafe:
        let n: Int be install(identity)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let reported: Vec<u16> = set.diagnostics().iter().map(|d| d.code.0).collect();
    assert!(reported.contains(&code::SC0522.0), "{reported:?}\n{}", set.render());
}

#[test]
fn a_non_generic_passed_to_an_extern_function_is_not_reported() {
    let source = "\
unsafe extern \"C\" library \"m\":
    def install(callback: ffi.FunctionPointer) -> Int

def concrete(value: Int) -> Int:
    value

def main():
    unsafe:
        let n: Int be install(concrete)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(
        set.diagnostics().iter().all(|d| d.code != code::SC0522),
        "{:?}",
        codes_of(&set)
    );
}

// --- 6. the holes are counted, not assumed away ---------------------------

#[test]
fn a_closure_that_captures_something_is_a_hole_and_is_counted() {
    // §8's hole, narrowed rather than closed: a *captured* closure's body is
    // still not lowered, because there is nowhere in `(A) -> B` to record what
    // it captured (`science-mir`'s `lower.rs` §8.5). Counting it is what stops
    // the set being quietly incomplete.
    //
    // This test used to be `a_closure_is_a_hole_and_is_counted`, over
    // `x giving x`, which captures nothing. `science-mir` gives that one a
    // `Body` now, and this walk enqueues it — `a_capture_free_closure_is_no_
    // longer_a_hole`, below, is what replaced this test's claim for that
    // fixture. This one keeps the sentence true for the case that is still
    // true of.
    let source = "\
def main():
    let n be 1
    let f be x giving x + n
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(set.holes().closures >= 1, "{:?}", set.holes());
}

#[test]
fn a_capture_free_closure_is_no_longer_a_hole() {
    // §8.5's follow-up, the smallest case that exercises it end to end: a
    // closure with nothing captured now has a `Body` — keyed on its own
    // `param` — and this walk enqueues it exactly as it enqueues any other
    // address-taken function. `Holes::closures` stays at zero and the
    // closure's own symbol is in the set a backend would emit.
    let source = "\
def apply(f: (Int) -> Int) -> Int:
    f(1)

def main():
    apply(x giving x + 1)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert_eq!(set.holes().closures, 0, "{:?}", set.holes());
    assert!(
        set.symbols().any(|symbol| symbol.contains("closure$")),
        "the closure's own body must be in the emitted set: {:?}",
        set.symbols().collect::<Vec<_>>()
    );
}

#[test]
fn the_corpus_reports_where_the_walk_stops() {
    // Not an assertion about a number: an assertion that the numbers exist and
    // that the container boundary is where they come from. `science-mir`'s §7
    // item 6 says every `Array` and `Map` method in the corpus is an
    // unresolved callee, and this is that fact arriving one phase later.
    let mut unresolved = 0;
    for (_, source) in corpus() {
        if !is_clean(&source) {
            continue;
        }
        let mut lowered = lower(&source);
        let set = lowered.mono(RootSet::EveryBody);
        unresolved += set.holes().unresolved_callees;
    }
    assert!(unresolved > 0, "the corpus is full of unresolved container methods");
}

#[test]
fn an_unsolved_instantiation_is_recorded_rather_than_guessed() {
    // A generic body cannot be a root, so `EveryBody` records one rather than
    // inventing arguments for it. The alternative — emitting it with `T` still
    // in the symbol — is a symbol two instantiations could share.
    let source = "\
def generic[T](value: T) -> T:
    value

def main():
    let x: Int be 1
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EveryBody);
    assert!(
        set.holes().unsolved.iter().any(|u| matches!(u, Unsolved::Parameter { .. })),
        "{:?}",
        set.holes()
    );
    assert!(!set.render().contains("generic"), "{}", set.render());
}

// --- 7. the entry point ---------------------------------------------------

#[test]
fn a_file_with_no_entry_point_has_no_binary_root_set() {
    let source = "\
def library_only() -> Int:
    1
";
    let mut lowered = lower(source);
    assert!(lowered.mono(RootSet::EntryPoint).is_empty());
    assert_eq!(lowered.mono(RootSet::EveryBody).len(), 1);
}

#[test]
fn a_script_body_is_an_entry_point() {
    // `script-mode.md`'s top-level statements become a generated `main` with a
    // zero-width name span, and the walk has to root it like any other.
    let source = "let x be 1\n";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert_eq!(set.len(), 1, "{}", set.render());
}

#[test]
fn an_instance_of_a_declaration_without_a_body_is_still_a_symbol() {
    // A callee with no MIR — an interface's required method, a prelude
    // signature — is an item a backend has to reference even though this crate
    // cannot walk it. Emitting nothing would be a dangling call.
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EntryPoint);
    for item in set.emission_order() {
        assert!(!item.symbol.is_empty());
    }
    assert!(set.get(set.symbols().next().expect("an item")).is_some());
}

#[test]
fn reached_from_is_the_shortest_path_from_a_root() {
    let source = "\
def leaf() -> Int:
    1

def middle() -> Int:
    leaf()

def main():
    let a be leaf()
    let b be middle()
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let main_symbol = set
        .emission_order()
        .find(|item| item.description == "main")
        .expect("main")
        .symbol
        .clone();
    let leaf = set.emission_order().find(|item| item.description == "leaf").expect("leaf");
    // Breadth-first: `leaf` is reached from `main` directly and from `middle`
    // one level further out, and the first is what is recorded.
    assert_eq!(leaf.reached_from.as_deref(), Some(main_symbol.as_str()));
}

#[test]
fn a_root_has_no_reached_from() {
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EntryPoint);
    let main = set.emission_order().find(|item| item.description == "main").expect("main");
    assert_eq!(main.reached_from, None);
}

#[test]
fn the_instance_of_a_root_is_not_generic() {
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EntryPoint);
    for item in set.emission_order() {
        if item.reached_from.is_none() {
            assert!(!item.instance.is_generic(), "{}", item.description);
        }
    }
}

#[test]
fn an_instance_is_equal_to_itself_and_keyed_by_its_const_half() {
    let mut lowered = lower(TWO_GENERICS);
    let set = lowered.mono(RootSet::EntryPoint);
    for item in set.emission_order() {
        let copy: Instance = item.instance.clone();
        assert_eq!(copy, item.instance);
        assert_eq!(copy.key(), item.instance.key());
    }
}

// --- 8. const generics, which are what `MonoKey` was built for -------------

#[test]
fn a_const_argument_reaches_the_symbol_through_mono_key() {
    // §3. The const half of the key is `science_types::MonoKey`, the symbol's
    // const field is encoded from that key and from nothing else, and this is
    // the end-to-end evidence that the two agree.
    let source = "\
type Window[T, const N: Int]:
    first: T

def width[T, const N: Int](w: Window[T, N]) -> Int:
    let k: Int be 1
    k

def main():
    let x: Int be 1
    let w: Window[Int, 4] be Window(first: x)
    let n be width(w)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let width = set
        .emission_order()
        .find(|item| item.description.starts_with("width"))
        .expect("width");
    assert_eq!(width.description, "width[I64, 4]");
    assert!(width.symbol.ends_with("K14"), "{}", width.symbol);
    assert_eq!(width.instance.key().len(), 1);
    assert_eq!(width.instance.key().args()[0].as_constant(), Some(4));
}

#[test]
fn a_const_argument_is_recovered_by_one_variable_linear_matching() {
    // §8 item 7, at its first caller. The signature says `N + 1` and the call
    // site says `4`, so `N = 3` — a division with a remainder check, which is
    // `science_types::matching` and is not reimplemented here.
    let source = "\
type Window[T, const N: Int]:
    first: T

def width[T, const N: Int](w: Window[T, N + 1]) -> Int:
    let k: Int be 1
    k

def main():
    let x: Int be 1
    let w: Window[Int, 4] be Window(first: x)
    let n be width(w)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let width = set
        .emission_order()
        .find(|item| item.description.starts_with("width"))
        .expect("width");
    assert_eq!(width.description, "width[I64, 3]", "{}", set.render());
}

#[test]
fn a_growing_const_argument_is_caught_by_the_backstop() {
    // §6's second rule, and the case that proves the first one is incomplete:
    // a const argument does not nest, so nothing here contains anything and
    // the containment rule cannot fire. The depth backstop is what stops it,
    // and the message says which of the two rules it was.
    let source = "\
type Window[T, const N: Int]:
    first: T

def step[T, const N: Int](w: Window[T, N]) -> Int:
    let bigger: Window[T, N + 1] be Window(first: w.first)
    step(bigger)

def main():
    let x: Int be 1
    let w: Window[Int, 1] be Window(first: x)
    let n be step(w)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let diagnostic = set
        .diagnostics()
        .iter()
        .find(|d| d.code == code::SC0407)
        .expect("SC0407");
    let notes = diagnostic.notes.join("\n");
    assert!(notes.contains("the containment rule did not fire"), "{notes}");
    assert!(notes.contains("step[I64, 1]"), "{notes}");
    // And the walk stopped: the set is bounded by the limit rather than by the
    // machine's memory.
    assert!(set.len() <= Mono::LIMIT + 2, "{} items", set.len());
}

#[test]
fn a_long_chain_is_still_readable() {
    let source = "\
type Window[T, const N: Int]:
    first: T

def step[T, const N: Int](w: Window[T, N]) -> Int:
    let bigger: Window[T, N + 1] be Window(first: w.first)
    step(bigger)

def main():
    let x: Int be 1
    let w: Window[Int, 1] be Window(first: x)
    let n be step(w)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let diagnostic = set
        .diagnostics()
        .iter()
        .find(|d| d.code == code::SC0407)
        .expect("SC0407");
    assert!(
        diagnostic.notes.len() < 20,
        "a {}-note diagnostic is not a message",
        diagnostic.notes.len()
    );
}

// --- 9. what the recovery can and cannot read -----------------------------

#[test]
fn a_literal_argument_is_solved_from_the_destination() {
    // §2 cost 2. `Constant::Literal` carries no type, so the only site left is
    // the destination — and it works when the destination has one.
    let source = "\
def identity[T](value: T) -> T:
    value

def main():
    let a: Int be identity(1)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(set.render().contains("identity[I64]"), "{}", set.render());
}

#[test]
fn an_unannotated_literal_binding_is_solved_too() {
    // **This replaces `an_unannotated_literal_binding_leaves_nothing_to_read`,
    // and it was replaced on that test's own instructions.** It asserted that
    // the walk emitted nothing here, and said of itself: *"if this test starts
    // failing, that is good news — it means `science-types` now defaults an
    // unannotated numeric binding, and §2's note about it should be deleted
    // rather than this test relaxed"*. It does, so the note is deleted and
    // this is the positive assertion.
    //
    // What changed is upstream and not here: `check`'s `instantiate_call` now
    // **defers** a parameter whose only evidence is an argument still carrying
    // an unresolved literal class, binding it to that argument's own inference
    // variable instead of forcing `Ty::ERROR`. Decision 2's default then
    // settles the call and the binding in one step, and the destination this
    // walk reads is no longer a hole.
    //
    // `I64` rather than any integer: Decision 2's default for an unsuffixed
    // integer literal is what decides the instance, so the symbol is the
    // observation that the default reached all the way here.
    let source = "\
def identity[T](value: T) -> T:
    value

def main():
    let a be identity(1)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    assert!(set.render().contains("identity[I64]"), "{}", set.render());
    assert!(set.holes().unsolved.is_empty(), "{:?}", set.holes());
}

#[test]
fn a_method_on_a_generic_type_is_instantiated_from_its_receiver() {
    // The block's generics come first in an `Instance`'s argument list, and the
    // receiver is what solves them: `self_ty(owner)` is `Cell of T` and the
    // call site's `args[0]` is a `borrowed Cell of Int`.
    let source = "\
type Cell[T]:
    value: T

Cell[T] has:
    def get(self) -> &T:
        &self.value

def main():
    let n: Int be 1
    let c be Cell(value: n)
    let v be c.get()
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let get = set
        .emission_order()
        .find(|item| item.description.starts_with("Cell.get"))
        .unwrap_or_else(|| panic!("{}", set.render()));
    assert_eq!(get.description, "Cell.get[I64]", "{}", set.render());
}

#[test]
fn one_method_at_two_receiver_types_is_two_items() {
    let source = "\
type Cell[T]:
    value: T

Cell[T] has:
    def get(self) -> &T:
        &self.value

def main():
    let n: Int be 1
    let f: Float be 2.0
    let a be Cell(value: n)
    let b be Cell(value: f)
    let x be a.get()
    let y be b.get()
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let gets: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("Cell.get"))
        .map(|item| item.description.as_str())
        .collect();
    assert_eq!(gets.len(), 2, "{}", set.render());
}

/// A default body on `interface`, called through two different implementors
/// and never overridden by either.
///
/// `preview` is one `DefId` — `Summarize`'s own — so a walk that treated
/// `Self` as anything other than a second axis of an instance's identity
/// would solve both calls to `Instance::plain(preview_def)` and collapse them
/// onto one item, whichever receiver got there first. `describe`'s
/// `Self=Doc` / `Self=Row` is `Instance::self_ty` read back, and it is what
/// makes the two descriptions differ below.
#[test]
fn a_default_body_called_on_two_implementors_is_two_items() {
    let source = "\
interface Summarize:
    def summarize(self) -> String

    def preview(self) -> String:
        self.summarize()

type Doc:
    title: String

Doc implements Summarize:
    def summarize(self) -> String:
        self.title

type Row:
    label: String

Row implements Summarize:
    def summarize(self) -> String:
        self.label

def main():
    let d be Doc(title: \"hello\")
    let r be Row(label: \"world\")
    let a be d.preview()
    let b be r.preview()
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    let mut previews: Vec<&str> = set
        .emission_order()
        .filter(|item| item.description.starts_with("Summarize.preview"))
        .map(|item| item.description.as_str())
        .collect();
    previews.sort();
    assert_eq!(
        previews,
        vec!["Summarize.preview[Self=Doc]", "Summarize.preview[Self=Row]"],
        "{}",
        set.render()
    );
    assert!(set.holes().is_empty(), "{:?}", set.holes());
}

#[test]
fn a_callee_with_no_mir_is_marked_as_a_declaration() {
    // `defined_here` is the difference between `define` and `declare`, and a
    // backend that guessed would emit a body for an `extern` symbol.
    let source = "\
unsafe extern \"C\" library \"m\":
    def cabs(value: Int) -> Int

def main():
    let n: Int be 1
    unsafe:
        let a be cabs(n)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);
    for item in set.emission_order() {
        if item.description == "main" {
            assert!(item.defined_here, "{}", item.symbol);
        }
    }
    // The `extern` callee is counted rather than emitted: §8.
    assert_eq!(set.holes().extern_calls, 1, "{:?}", set.holes());
}

/// The walk's symbols and the LLVM backend's are the same symbols.
///
/// **This is the fact everything else about connecting the two rests on**, and
/// until it was asserted it was an inference from reading two files. `mangle`'s
/// own doc states the intent — *"`_S`, the length prefixes and the `E`
/// separator live in exactly one function, and everything that produces a
/// Science symbol calls it … what must **not** be duplicated is the frame
/// around them: two spellings of the length prefix is two symbol schemes, and a
/// linker would tell you about it a stage too late"* — but intent is not
/// agreement. `science-codegen-llvm`'s `symbol_of` builds a
/// [`science_types::MonoKey`] out of the definition's path components and calls
/// [`science_codegen::mangle::mangle`]; [`Mono::symbol_of`] builds its own path
/// and calls `assemble` directly. Both end at `assemble`, and this asserts that
/// the *paths* agree too — which is the half neither doc comment covers.
///
/// **Non-generic only, deliberately.** For an instance with arguments the two
/// cannot agree, because the backend has no arguments to encode: it refuses a
/// generic function outright. That refusal is the thing being removed, and the
/// precondition for removing it is that the schemes already coincide where both
/// have an answer. If this test ever fails, driving emission from a `MonoSet`
/// is a symbol migration and not a rewiring, which is a completely different
/// piece of work — so the failure message says that rather than just reporting
/// two strings.
#[test]
fn the_walk_and_the_backend_agree_on_a_plain_symbol() {
    let source = "\
def double(n: Int) -> Int:
    n * 2

def main():
    let a be double(21)
";
    let mut lowered = lower(source);
    let set = lowered.mono(RootSet::EntryPoint);

    for item in set.emission_order() {
        if item.instance.is_generic() {
            continue;
        }
        let name = &lowered.krate.defs.get(item.instance.def).name;
        let theirs = science_codegen::mangle::mangle(
            &science_codegen::mangle::MonoKey::plain(&[name.as_str()]),
        );
        assert_eq!(
            item.symbol, theirs,
            "the monomorphisation walk and `science-codegen-llvm`'s `symbol_of` disagree about \
             `{name}`: connecting the two is a symbol migration, not a rewiring"
        );
    }
}

/// [`Instance::const_arg`] recovers the const argument a const generic
/// parameter is bound to.
///
/// # Why this is the boundary this test holds, and not the whole example
///
/// `examples/03_structs.science`'s `Grid[Int, 3, 3].area()` reads two const
/// parameters — `ROWS`, `COLS` — as *values*, which `science-mir`'s
/// `lower.rs` lowers to `mir::Constant::Item(param_def)` because neither is a
/// `const` declaration (`Declarations::const_value` answers `None` for a
/// const *parameter*) and neither is a `Ty` `science_mir::instantiate` could
/// rewrite. `Instance::const_arg` is the lookup a backend needs to resolve
/// that read once it knows which instance it is lowering — this test is
/// where that lookup is asserted directly, against the two `DefId`s the real
/// checker assigns `ROWS` and `COLS`, with an `Instance` built by hand.
///
/// **This is deliberately not an end-to-end build of the example.** The
/// walk that would have to *discover* `Instance { def: area, args: [Int, 3,
/// 3] }` from `main`'s call — [`Mono::collect`]'s `solve_call` — recovers a
/// callee's arguments by unifying its **declared parameter and return
/// types** against the call's actual ones (`crates/science-codegen/src/mono.rs`,
/// `solve_call`'s own documentation: *"The callee's instantiation, recovered
/// from the call site"*). `area`'s signature is `() -> Int`: no parameter to
/// unify and a return type that names none of `T`, `ROWS`, `COLS`. Nothing
/// in the call's MIR (`mir::Callee::Def(DefId)` carries no arguments at all —
/// `science-mir`'s `instantiate.rs` module doc says so explicitly) says the
/// receiver was written as `Grid[Int, 3, 3]` rather than any other
/// instantiation, and nothing in `science-types`'s checker (`check.rs`'s
/// `associated_call`, whose callee node is pushed at `Ty::ERROR` and never
/// updated) keeps that fact either. Recovering it needs a change in one of
/// those two crates — carrying the receiver's concrete type or arguments
/// through the call — and both are out of this crate's reach. This test
/// covers what *is* in reach: once an `Instance` names the right arguments,
/// by whatever means, `const_arg` reads a const parameter's value out of it
/// correctly.
#[test]
fn const_arg_recovers_a_bound_const_parameter() {
    let source = "\
type Grid[T, const ROWS: Int, const COLS: Int]:
    cells: Array[T]

Grid[T, const ROWS: Int, const COLS: Int] has:
    def area() -> Int:
        ROWS * COLS

def main():
    print(Grid[Int, 3, 3].area())
";
    let lowered = lower(source);

    let area = lowered
        .krate
        .defs
        .iter()
        .find(|def| def.kind == hir::DefKind::Fn && def.name == "area")
        .expect("`area` is declared")
        .id;
    // `ROWS` and `COLS` are declared twice over: once on `type Grid[..]:`
    // and again on `Grid[..] has:`, which restates the record's parameters
    // as its own `DefId`s (`check.rs`'s `const_param_ty` says so: "an `impl`
    // block re-declares the type's parameters on the block ... the record's
    // list is a *different* set of `DefId`s"). `area` reads the block's own
    // copy, found through `block_generics` the same way
    // `science_codegen::mono::generics_of` reads it.
    let owner = lowered.decls.signature(area).and_then(|signature| signature.owner).expect("area has an owner block");
    let block_generics = lowered.decls.block_generics(owner).expect("the block declares generics");
    let rows = block_generics
        .iter()
        .find(|param| lowered.krate.defs.get(param.def).name == "ROWS")
        .expect("`ROWS` is one of the block's generics")
        .def;
    let cols = block_generics
        .iter()
        .find(|param| lowered.krate.defs.get(param.def).name == "COLS")
        .expect("`COLS` is one of the block's generics")
        .def;
    let instance = Instance {
        def: area,
        args: vec![
            GenericArg::Error, // `T`: irrelevant to this test, so left unsolved rather than guessed.
            GenericArg::Const(NormalForm::literal(3)),
            GenericArg::Const(NormalForm::literal(3)),
        ],
        self_ty: None,
    };

    assert_eq!(
        instance.const_arg(&lowered.decls, rows).and_then(|form| form.as_constant()),
        Some(3),
        "`ROWS` should read as the const argument bound in `instance.args`"
    );
    assert_eq!(
        instance.const_arg(&lowered.decls, cols).and_then(|form| form.as_constant()),
        Some(3),
        "`COLS` should read as the const argument bound in `instance.args`"
    );
    assert_eq!(
        instance.const_arg(&lowered.decls, area),
        None,
        "a definition that is not one of `area`'s own generic parameters binds nothing"
    );
}

