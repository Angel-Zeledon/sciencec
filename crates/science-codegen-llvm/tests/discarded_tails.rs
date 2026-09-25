//! A `T?` thrown away at the **tail of a block**, built, linked and run.
//!
//! # The asymmetry these programs pin
//!
//! `m.insert(k, v)` on a line of its own worked and `if ready:` one line above
//! it did not, and the difference was never about `if`, `loop` or `for`. It was
//! about **where in a block the call sits**:
//!
//! - A call written as a *statement* is `thir::StmtKind::Expr`, and
//!   `science-mir`'s `lower_stmt` gives it a temporary built from **the
//!   expression's own type**. `m.insert(k, v)` discarded that way lands in a
//!   `V?` slot, which is what §5.3's bool-plus-out-parameter convention needs.
//! - A call written as a block's *tail* was lowered straight into the
//!   **enclosing construct's** destination, whose type is the construct's and
//!   not the tail's. `check`'s §6 types an `if` with no `else`, a `loop` and a
//!   `for` as `()` however their bodies end, so the destination was a `()`
//!   place and the call had nowhere to write the displaced value. Codegen said
//!   so: *"a call to `science_map_insert` whose destination is `()` rather than
//!   a `T?`"*.
//!
//! Writing `m.insert(k, v)` as the only line of a body puts it in tail
//! position, which is why the gap looked like "nested blocks are broken" and
//! why `m.insert(k, v)` followed by any other line in the same body already
//! worked. `lower_loop` had met one face of it and typed its own discard from
//! the body's tail; that repair is now general, in `lower_block`, and the
//! per-construct one is gone.
//!
//! # Why these are execution tests and not a MIR assertion
//!
//! A destination of the right *type* is a claim about a slot the runtime writes
//! through. The program that proves it is one that inserts, reads the length
//! back and exits zero — a MIR dump asserting the temporary's type would pass
//! for a lowering that then wrote the payload nowhere.
//!
//! # The leak this could have been instead
//!
//! `Map.insert`'s `V?` is *the value it displaced*, so discarding it must still
//! **drop** it; a fix that only silenced the refusal would trade a diagnostic
//! for a leak, and this exact path has form — `define_map_info`'s `drop_fn` was
//! a hardcoded null and leaked every displaced owning key.
//! [`a_discarded_insert_drops_the_value_it_displaced`] is the instrument
//! `tests/user_drop.rs` argues for over a resident-set measurement: the
//! temporary belongs to the block's own scope, so `pop_scope` drops it, and a
//! `drop` that prints says which iteration released what.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

/// Build one program at `-O2`, run it, and give back what it printed.
fn prints(name: &str, source: &str) -> String {
    let dir = scratch("discarded-tails", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// `if c:` whose body is one discarded `insert`.
///
/// The smallest program that showed the gap, and the one a reader writes
/// first: an `if` with no `else` is `()`, its body's tail is a `V?`, and the
/// two used to meet at the call site.
#[test]
fn a_discarded_insert_is_the_tail_of_an_if() {
    assert_eq!(
        prints(
            "if",
            "let mutable m be Map[String, Int].new()\n\
             if m.length() is 0:\n\
             \x20   m.insert(\"k\", 1)\n\
             print(f\"{m.length()}\")\n",
        ),
        "1\n"
    );
}

/// A `loop` that fills a map, which is the survey's *"anyone writing a loop
/// that fills a map hits it immediately"*.
///
/// **The one program here that already built**, and it is kept for that
/// reason: `lower_loop` typed its own discard from the body's tail, which is
/// the repair this change generalised and then deleted from there. Without
/// this test, moving the answer into `lower_block` would be an unmeasured
/// change to a working program.
#[test]
fn a_discarded_insert_is_the_tail_of_a_loop() {
    assert_eq!(
        prints(
            "loop",
            "let mutable m be Map[Int, Int].new()\n\
             let mutable i be 0\n\
             loop:\n\
             \x20   if i is 3:\n\
             \x20       break\n\
             \x20   i be i + 1\n\
             \x20   m.insert(i, i)\n\
             print(f\"{m.length()}\")\n",
        ),
        "3\n"
    );
}

/// The same fill written as a `for` over a range.
#[test]
fn a_discarded_insert_is_the_tail_of_a_for() {
    assert_eq!(
        prints(
            "for",
            "let mutable m be Map[Int, Int].new()\n\
             for k in 0..3:\n\
             \x20   m.insert(k, k)\n\
             print(f\"{m.length()}\")\n",
        ),
        "3\n"
    );
}

/// `remove`, the convention's other `Map` user, discarded in a `for`.
///
/// Kept separate from `insert` because the two reach `lower_owned_nullable_call`
/// through different `RUNTIME` arities — four parameters and five — and a
/// destination repair that only fitted one of them would pass an `insert`-only
/// file.
#[test]
fn a_discarded_remove_is_the_tail_of_a_for() {
    assert_eq!(
        prints(
            "remove",
            "let mutable m be Map[Int, Int].new()\n\
             for k in 0..3:\n\
             \x20   m.insert(k, k)\n\
             for k in 0..2:\n\
             \x20   m.remove(k)\n\
             print(f\"{m.length()}\")\n",
        ),
        "1\n"
    );
}

/// `Array.pop`, the convention's third user, discarded in all three constructs
/// at once.
///
/// One program rather than three because the failure was one: `pop` takes a
/// `ScienceTypeInfo` where the two `Map` methods take a `ScienceMapInfo`, so it
/// is the other branch of `lower_owned_nullable_call`'s descriptor `if`, and
/// the three loop shapes below it are the same `lower_block` either way.
#[test]
fn a_discarded_pop_is_the_tail_of_an_if_a_loop_and_a_for() {
    assert_eq!(
        prints(
            "pop",
            "let mutable xs be Array[Int].new()\n\
             xs.push(1)\n\
             xs.push(2)\n\
             xs.push(3)\n\
             if xs.length() > 2:\n\
             \x20   xs.pop()\n\
             print(f\"if={xs.length()}\")\n\
             loop:\n\
             \x20   if xs.length() < 2:\n\
             \x20       break\n\
             \x20   xs.pop()\n\
             print(f\"loop={xs.length()}\")\n\
             for k in 0..1:\n\
             \x20   xs.pop()\n\
             print(f\"for={xs.length()}\")\n",
        ),
        "if=2\nloop=1\nfor=0\n"
    );
}

/// **The refusal was not traded for a leak**: the displaced value is dropped.
///
/// The same key three times, so the live set never grows and every line of
/// output is a value the map gave *back*. `Tracer` prints its own tag, so the
/// two mid-loop drops are identifiably iterations 0 and 1 — a destructor
/// running on the wrong value, or twice on one, reads differently from a
/// destructor running on time.
///
/// The last line is the map's own release at scope exit, which is the surviving
/// entry and not a fourth drop of a discarded option. A version of this repair
/// that stored the `V?` in a temporary the scope did not own would print the
/// first two lines and never the middle two.
#[test]
fn a_discarded_insert_drops_the_value_it_displaced() {
    assert_eq!(
        prints(
            "displaced",
            "type Tracer:
    tag: Int

Tracer implements Drop:
    def drop(mutable self):
        print(f\"displaced {self.tag}\")

def main():
    let mutable m be Map[Int, Tracer].new()
    for i in 0..3:
        m.insert(7, Tracer(tag: i))
    print(\"after the loop\")
",
        ),
        "displaced 0\ndisplaced 1\nafter the loop\ndisplaced 2\n",
        "the first two lines are the values iterations 1 and 2 displaced, \
         released inside the loop; the last is the surviving entry, released \
         with the map"
    );
}
