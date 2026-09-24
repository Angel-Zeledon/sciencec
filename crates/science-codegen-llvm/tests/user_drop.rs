//! Decision 12's second half — *"and then calls the type's own `Drop`
//! implementation if it has one"* — **built, linked, run**, with the order it
//! fixes read back off stdout.
//!
//! # The instrument this file is, and why the drop path needed one
//!
//! Until the call this file tests existed, **no drop in this language had an
//! effect a program could see.** Drop glue's first half worked: a type owning
//! a `String` had its `String` freed on time. Its second half did not, and a
//! user's `def drop` was a body the compiler lexed, parsed, resolved, checked
//! and then never emitted a call to, with no diagnostic anywhere. The two
//! halves are indistinguishable from outside unless the destructor can *print*,
//! so the only instrument the drop path had was a resident-set measurement over
//! millions of allocations — and two defects hid in it behind exactly that.
//!
//! An RSS measurement is a bad instrument for three separate reasons, and this
//! file replaces it for all three. It cannot see **order** at all, which is the
//! half of Decision 12 that is a decision rather than a fact. It cannot see a
//! destructor that is *skipped*, only one whose freeing is skipped — which is
//! how `drop` went missing in the first place. And it is not a regression test
//! anyone writes: the null `drop_fn` in `emit.rs`'s `define_map_info` leaked
//! every displaced owning map key, was found and fixed, and landed **without a
//! test**, because writing one meant allocating a few hundred thousand keys and
//! asserting on a number that moves with the allocator. Every test below is
//! two lines of expected stdout instead.
//!
//! # The order, and where it comes from
//!
//! `codegen-and-linking.md`'s Decision 12, quoted in full: *"Drop glue is an
//! emitted `internal` function per monomorphised type, named by the mangling of
//! §2.7, whose body drops fields in reverse declaration order and then calls
//! the type's own `Drop` implementation if it has one."*
//!
//! Two orderings are stated there and both are tested below, because both are
//! choices a future edit could silently reverse:
//!
//! 1. **fields before the type's own `drop`** —
//!    [`the_fields_are_dropped_before_the_type_s_own_drop`]; and
//! 2. **fields among themselves in reverse declaration order** —
//!    [`the_fields_run_in_reverse_declaration_order`].
//!
//! Note that (1) is the *opposite* of the order Rust runs the two in, and it is
//! not a typo in the note — it is stated as one sentence with one `and then` in
//! it. The cost of reading it literally is real and is recorded in
//! [`a_drop_sees_its_owning_fields_already_released`], which is a test of what
//! this compiler does today rather than an endorsement of it.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
///
/// `-O2` for `tests/arrays.rs`'s reason, which applies with more force here:
/// glue is an `internal` function whose only caller is other glue, so at `-O0`
/// a wrong call is still a call and at `-O2` the inliner has folded the whole
/// chain into the caller. A drop that survives the optimiser is a drop that was
/// really there.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("user-drop", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The sentence that was false: a user's `drop` runs at all.
///
/// **`H` owns nothing**, and that is the whole point of the fixture rather than
/// an incidental simplification. A record of one `Int` is structurally inert,
/// so the old `needs_drop` said `false`, so `science-mir` emitted no `Drop`
/// terminator, so there was nothing for any amount of correct codegen to be
/// reached through. The repair is in three places and this is the program that
/// proves all three connect: `ownership.rs` answers `true` for a nominal
/// reason, `drops.rs` stops rewriting a zero-leaf record's `Drop` into a
/// `Goto`, and `lower.rs` emits the call.
#[test]
fn a_user_s_drop_runs_at_scope_exit() {
    assert_eq!(
        prints(
            "scope_exit",
            "type H:
    v: Int

H implements Drop:
    def drop(mutable self):
        print(\"drop H\")

def f():
    let h be H(v: 1)
    print(\"in f\")

def main():
    f()
    print(\"end\")
"
        ),
        "in f\ndrop H\nend\n",
        "`drop H` belongs between the two, and before this landed it was absent \
         entirely — the program printed `in f` then `end` and reported no error"
    );
}

/// Decision 12's first ordering: the fields, then the type's own `drop`.
///
/// `Outer` owns two `Inner`s and both types print, so the four lines are a
/// total order and not a set. A reordering that ran `Outer.drop` first would
/// still print four lines, still exit zero, and still leak nothing — this
/// assertion is the only thing between that edit and the tree.
#[test]
fn the_fields_are_dropped_before_the_type_s_own_drop() {
    assert_eq!(
        prints(
            "fields_first",
            "type Inner:
    v: Int

Inner implements Drop:
    def drop(mutable self):
        print(\"inner\")

type Outer:
    a: Inner

Outer implements Drop:
    def drop(mutable self):
        print(\"outer\")

def main():
    let o be Outer(a: Inner(v: 1))
    print(\"made\")
"
        ),
        "made\ninner\nouter\n",
        "Decision 12 reads `drops fields in reverse declaration order and then \
         calls the type's own `Drop` implementation` — the field's `drop` runs \
         first and the record's own runs last"
    );
}

/// Decision 12's second ordering: **reverse** declaration order among the
/// fields.
///
/// The two `Inner`s carry different `Int`s so the two lines can be told apart;
/// `second` is declared last and must be released first. A glue that walked
/// declaration order forwards passes every other test in this file.
#[test]
fn the_fields_run_in_reverse_declaration_order() {
    assert_eq!(
        prints(
            "reverse_order",
            "type Inner:
    v: Int

Inner implements Drop:
    def drop(mutable self):
        print(self.v)

type Outer:
    first: Inner
    second: Inner

def main():
    let o be Outer(first: Inner(v: 1), second: Inner(v: 2))
    print(\"made\")
"
        ),
        "made\n2\n1\n",
        "`second` is declared after `first`, so it is released before it"
    );
}

/// A `return` before the end of the body still runs the drop, on the path
/// taken and once.
///
/// Decision 13 puts drop elaboration in MIR and leaves codegen *"only
/// unconditional `Drop` terminators"*, so an early return is a question about
/// whether the elaborator put a terminator on both edges — and a duplicate
/// would print twice rather than fail quietly.
#[test]
fn a_user_s_drop_runs_at_an_early_return() {
    assert_eq!(
        prints(
            "early_return",
            "type H:
    v: Int

H implements Drop:
    def drop(mutable self):
        print(\"drop H\")

def f(early: Bool) -> Int:
    let h be H(v: 1)
    if early:
        print(\"leaving early\")
        return 1
    print(\"leaving late\")
    2

def main():
    print(f(true))
    print(f(false))
"
        ),
        "leaving early\ndrop H\n1\nleaving late\ndrop H\n2\n",
        "one `drop H` per call, on whichever edge the call took, before the \
         value is printed in `main`"
    );
}

/// A value bound by a `match` arm's pattern is released when the arm ends.
///
/// The scrutinee is a `choice` whose payload owns a printing type, so this also
/// pins that the `switch` glue `emit_choice_glue` builds reaches a *variant's*
/// payload and not only a record's field — the two take different block shapes
/// and only one of them is exercised by every other test here.
#[test]
fn a_user_s_drop_runs_for_a_match_arm_s_binding() {
    assert_eq!(
        prints(
            "match_arm",
            "type Payload:
    v: Int

Payload implements Drop:
    def drop(mutable self):
        print(self.v)

choice Shape:
    Round(Payload)
    Flat(Payload)

def main():
    let s be Round(Payload(v: 7))
    match s:
        Round(p): print(\"round\")
        Flat(p): print(\"flat\")
    print(\"end\")
"
        ),
        "round\n7\nend\n",
        "the payload is released when the `match` is over and before `end`"
    );
}

/// An element of an `Array` runs its `drop`, once per element, through the
/// `drop_fn` in Decision 20's descriptor.
///
/// **This is the seam the RSS instrument could see and could not count.** A
/// `drop_fn` called once for a two-element array frees half the memory and
/// leaks the rest, which is a number; called twice it is a double free, which
/// is a crash. Printing the element makes it an exact count, and the order the
/// runtime walks its buffer in is pinned along with it.
#[test]
fn a_user_s_drop_runs_for_every_array_element() {
    assert_eq!(
        prints(
            "array_element",
            "type Cell:
    v: Int

Cell implements Drop:
    def drop(mutable self):
        print(self.v)

def main():
    let xs be [Cell(v: 1), Cell(v: 2), Cell(v: 3)]
    print(xs.length())
"
        ),
        "3\n1\n2\n3\n",
        "three elements, three `drop`s, in the buffer's own order"
    );
}

/// A container nested inside a container releases its contents — the Gate C1
/// clause `Array of Box[Expr]`, and the three shapes beside it.
///
/// # What this used to be
///
/// A refusal, `SC0400`, in [`Lowerer::intern_element_descriptor`]: *"an `Array
/// of Box[Expr]`, whose element is released by a runtime call that takes a
/// descriptor of its own: Decision 20's `drop_fn` is called with the element's
/// address and nothing else, and there is no one-argument symbol to name"*.
/// The reasoning was sound and the conclusion was wrong, and the **workaround
/// is what proves it wrong**: wrapping the inner container in a one-field
/// record made `Array of Row` build the same day, and that record's own
/// Decision 12 glue *is* a one-argument function whose whole body is
/// `science_array_free(field_address, descriptor)`. The record was contributing
/// a name to hang the glue on and nothing else, so
/// `Lowerer::intern_container_thunk` hangs it on the element's rendering
/// instead.
///
/// # Why the inner values print
///
/// A thunk that built the right call with the wrong *argument* — the slot's
/// address where the pointer stored in it belonged, which is the one difference
/// `science_box_free` has from the other three frees — produces a program that
/// verifies, links, reports the right length and then aborts in `free`. Reading
/// the inner `drop` back is what separates "released" from "released the right
/// bytes".
#[test]
fn a_nested_owning_container_releases_its_contents() {
    assert_eq!(
        prints(
            "nested_container",
            "type Expr:
    v: Int

Expr implements Drop:
    def drop(mutable self):
        print(self.v)

def main():
    let boxes be [Box.new(Expr(v: 1)), Box.new(Expr(v: 2))]
    print(boxes.length())
    let rows be [[Expr(v: 3)], [Expr(v: 4)]]
    print(rows.length())
"
        ),
        "2\n2\n3\n4\n1\n2\n",
        "both lengths print before either array is released; then the two \
         locals go in reverse declaration order — `rows`, an `Array of (Array \
         of Expr)`, before `boxes`, an `Array of Box[Expr]` — and each runs \
         its own elements front to back"
    );
}

/// The four nested shapes Gate C1 names, built and run together, with no
/// user `drop` anywhere — the refusal's own list.
///
/// `Array[Array[Int]]`, `Array[Box[Expr]]`, `Array[Box[String]]` and
/// `Map[String, Array[Int]]` each reach
/// [`Lowerer::intern_container_thunk`] by a different route: an array's element
/// twice over, a `Box` whose descriptor comes first rather than second, a `Box`
/// of a type whose own release is a runtime call, and a *map value*, which goes
/// through the same element descriptor an array's element does. A clean exit is
/// the assertion: three of the four abort in the allocator if the thunk names
/// the wrong argument.
#[test]
fn every_nested_container_shape_gate_c1_names_builds_and_runs() {
    assert_eq!(
        prints(
            "gate_c1_shapes",
            "type Expr:
    v: Int

def main():
    let ints be [[1, 2], [3, 4]]
    print(ints.length())

    let boxes be [Box.new(Expr(v: 1)), Box.new(Expr(v: 2))]
    print(boxes.length())

    let s be \"a string long enough to be a heap allocation\"
    let strings be [Box.new(s)]
    print(strings.length())

    let mutable m: Map[String, Array[Int]] be Map[String, Array[Int]].new()
    m.insert(\"k\", [1, 2, 3])
    print(m.length())
"
        ),
        "2\n2\n1\n1\n"
    );
}

/// A record with a field moved out of it releases the fields that are left and
/// does **not** run its own `drop`.
///
/// # The defect this pins, which is a leak and not a wrong line of output
///
/// Making `needs_drop` answer `true` for a nominal reason has a consequence one
/// step away from it: `science-mir`'s `moves::record_fields` decides whether a
/// local is tracked field-by-field, and a first attempt at this work made it
/// answer `None` for every type implementing `Drop` — on the genuinely correct
/// reasoning that a destructor observing the whole record must not be released
/// a field at a time. It is the right instinct in the wrong place. A record
/// whose field has been moved out then had *no* per-field states, fell back to
/// the whole-local table, read "moved", and dropped nothing at all: `consume`
/// below leaked `doc.body` on every call. Measured before and after at 300 000
/// rounds, the resident set went from **21.0 MB to 1.77 MB** against a
/// 1.75 MB floor for the same program with the `implements Drop:` block
/// deleted.
///
/// # Why no `drop` line is the right expectation
///
/// `self` is incomplete — `title` has been moved into the caller — so there is
/// no whole record for `drop(mutable self)` to be handed, and the fields that
/// remain are released one at a time instead. Rust refuses this program
/// outright (`E0509`, *"cannot move out of a type which implements Drop"*) and
/// that is very likely where this language should end up too; until a note
/// says so, releasing the remainder and skipping the destructor is the
/// behaviour, and it is pinned here so that whichever way it is later decided,
/// it is decided rather than drifted into.
#[test]
fn a_partially_moved_record_releases_the_rest_and_skips_its_drop() {
    assert_eq!(
        prints(
            "partial_move",
            "type Doc:
    title: String
    body: String

Doc implements Drop:
    def drop(mutable self):
        print(\"doc dropped\")

def consume(doc: Doc) -> String:
    doc.title

def main():
    let whole be Doc(title: \"t\", body: \"b\")
    print(\"whole made\")
    let moved be Doc(title: \"t\", body: \"b\")
    print(consume(moved))
    print(\"end\")
"
        ),
        "whole made\nt\nend\ndoc dropped\n",
        "`consume`'s partially-moved `Doc` releases `body` silently and runs no \
         `drop`; the one `doc dropped` is `whole`, at the end of `main`, where \
         the record is still entire"
    );
}

/// What reading Decision 12 literally costs: a `drop` cannot see its own
/// owning fields.
///
/// # This is a record of behaviour, not an endorsement of it
///
/// Decision 12 orders the two halves *"drops fields in reverse declaration
/// order **and then** calls the type's own `Drop`"*. Run literally — which is
/// what this compiler now does, and what the note says — the `String` field is
/// released before `drop(mutable self)` is entered, so the destructor reads an
/// emptied field. It is **memory-safe**: `science_string_free`'s own contract
/// is that *"the receiver is left as a valid empty `String` rather than as a
/// dangling one"*, so this is a wrong value and not a use-after-free. A
/// non-owning field — `n` below — is untouched and reads correctly, which is
/// what localises the effect to exactly the fields the first half released.
///
/// Rust runs the two halves the other way round for precisely this reason. The
/// sentence in the note is unambiguous, one `and then` with no qualification,
/// so the note wins and the consequence is written down here rather than
/// quietly fixed against it: changing the order is a change to
/// `codegen-and-linking.md` first and to `emit_user_drop_glue` second, and this
/// assertion is what will fail the day someone makes it — which is the point.
#[test]
fn a_drop_sees_its_owning_fields_already_released() {
    assert_eq!(
        prints(
            "fields_already_freed",
            "type A:
    n: Int
    tag: String

A implements Drop:
    def drop(mutable self):
        print(self.n)
        print(self.tag)

def main():
    let a be A(n: 7, tag: \"hello\")
    print(\"made\")
"
        ),
        "made\n7\n\n",
        "`n` is an `Int` and survives; `tag` was freed by the first half of \
         Decision 12's sentence and prints as the empty string the runtime left \
         in its place"
    );
}
