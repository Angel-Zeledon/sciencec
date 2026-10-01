//! The bundled `collections` module's `Deque`, **built, linked, run**.
//!
//! `Deque` is Science over the prelude's `Array` — two arrays back to back,
//! `front` reversed — so what these tests measure is as much the compiler as
//! the module: a generic record with generic methods, a generic free function
//! taking two `&mut Array[T]` borrows of one record's fields, `T?` moved out
//! of `Array.pop` and narrowed, and a user `Iterate` whose `Item` is a borrow
//! held through a record field. `stdlib-core.md` §3.7's traversal is the last
//! test, and it is what found `science-mir`'s `loop_source` defect — a `for`
//! over a borrow narrowed out of `Map.get` — which `maps.rs` pins on its own.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("collections", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// Pushed at the back and popped at the front is a queue; pushed and popped
/// at one end is a stack, at either end. `first`, `last` and `get` read across
/// the seam between the two arrays.
#[test]
fn a_deque_is_first_in_first_out_and_last_in_first_out() {
    assert_eq!(
        prints(
            "fifo_lifo",
            "use collections (Deque)

def main():
    let mutable queue be Deque[Int].new()
    for i in 0..5:
        queue.push_back(i)
    queue.push_front(-1)
    queue.push_front(-2)
    print(queue.length())
    let head be queue.first()
    let tail be queue.last()
    let middle be queue.get(3)
    if head? and tail? and middle?:
        print(f\"{head} {tail} {middle}\")
    loop:
        let x be queue.pop_front()
        if not x?:
            break
        write(f\"{x} \")
    print(\"\")

    let mutable stack be Deque[Int].new()
    for i in 0..4:
        stack.push_back(i)
    loop:
        let x be stack.pop_back()
        if not x?:
            break
        write(f\"{x} \")
    print(\"\")
    for i in 0..4:
        stack.push_front(i)
    loop:
        let x be stack.pop_front()
        if not x?:
            break
        write(f\"{x} \")
    print(\"\")
    print(stack.is_empty())
",
        ),
        "7\n-2 4 1\n-2 -1 0 1 2 3 4 \n3 2 1 0 \n3 2 1 0 \ntrue\n"
    );
}

/// An empty deque answers `null` from every reader, and is empty again after
/// what was pushed is popped — from the other end, through a rebalance.
#[test]
fn an_empty_deque_pops_null() {
    assert_eq!(
        prints(
            "empty_pops",
            "use collections (Deque)

def main():
    let mutable d be Deque[String].new()
    let a be d.pop_front()
    let b be d.pop_back()
    let c be d.first()
    let e be d.last()
    let f be d.get(0)
    if not a? and not b? and not c? and not e? and not f?:
        print(\"all null\")
    d.push_front(\"only\")
    let g be d.pop_back()
    if g?:
        print(g)
    let h be d.pop_front()
    if not h?:
        print(d.length())
    let i be d.get(-1)
    if not i?:
        print(\"no negative index\")
",
        ),
        "all null\nonly\n0\nno negative index\n"
    );
}

/// Grown far past any array's first capacity from both ends, then drained
/// from alternate ends, so that each end runs dry many times and takes half
/// of the other. Front to back the deque is `-500..1500` throughout, so a
/// `pop_front` must return one more than the last and a `pop_back` one less.
/// Every seventh turn puts its front back, so the two ends meet off-centre;
/// the last line's three numbers are Python's `collections.deque` running the
/// same program.
#[test]
fn a_deque_grows_from_both_ends_and_rebalances_in_order() {
    assert_eq!(
        prints(
            "growth",
            "use collections (Deque)

def main():
    let mutable d be Deque[Int].new()
    for i in 0..1500:
        d.push_back(i)
    for i in 1..501:
        d.push_front(0 - i)
    print(d.length())
    let mutable low be -501
    let mutable high be 1500
    let mutable ordered be true
    let mutable turns be 0
    loop:
        let x be d.pop_front()
        if not x?:
            break
        if x is not low + 1:
            ordered be false
        low be x
        let y be d.pop_back()
        if y?:
            if y is not high - 1:
                ordered be false
            high be y
        if turns % 7 is 0:
            d.push_front(low)
            low be low - 1
        turns be turns + 1
    print(ordered)
    print(f\"{low} {high} {turns}\")
    print(d.is_empty())
",
        ),
        "2000\ntrue\n422 423 1077\ntrue\n"
    );
}

/// Owned items: every `String` pushed is either popped and dropped by the
/// caller or dropped with the deque, and none is dropped twice. A constant
/// live set over two hundred deques is what `leaks --atExit` checks.
#[test]
fn owned_items_are_neither_leaked_nor_freed_twice() {
    let source = "use collections (Deque)

def main():
    let mutable total be 0
    for round in 0..200:
        let mutable q be Deque[String].new()
        for i in 0..50:
            q.push_back(f\"back {round} {i}\")
            q.push_front(f\"front {round} {i}\")
        for _ in 0..30:
            let a be q.pop_front()
            if a?:
                total be total + a.length()
            let b be q.pop_back()
            if b?:
                total be total + b.length()
        for item in q.iterate():
            total be total + item.length()
    print(total)
";
    let dir = scratch("collections", "owned_leaks");
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, "owned_leaks"), OptLevel::O2);
    let ran = run(&built);
    // Per round, `back R I` is 6 + |R| + |I| bytes and `front R I` is 7: over
    // fifty `I`, 650 + 90 + 90 + 100·|R|. Over two hundred rounds |R| sums to
    // 10 + 180 + 300 = 490: 166000 + 49000. Popped or iterated, every string
    // is counted once.
    assert_eq!(ran.stdout, "215000\n", "stderr: {}", ran.stderr);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);

    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&built.executable)
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(
            text.contains(" 0 leaks for 0 total leaked bytes"),
            "`leaks --atExit` found something:\n{text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `iterate` walks front to back across the seam, borrows each element, and
/// is a value that can be bound before the loop.
#[test]
fn a_deque_iterates_front_to_back() {
    assert_eq!(
        prints(
            "iterate",
            "use collections (Deque)

def main():
    let mutable d be Deque[String].new()
    d.push_back(\"c\")
    d.push_front(\"bb\")
    d.push_back(\"dddd\")
    d.push_front(\"a\")
    for item in d.iterate():
        write(item)
    print(\"\")
    let walk be d.iterate()
    let mutable lengths be 0
    for item in walk:
        lengths be lengths + item.length()
    print(lengths)
",
        ),
        "abbcdddd\n8\n"
    );
}

/// `stdlib-core.md` §3.7's `reachable`, as written there but for two
/// spellings: `Set[Int].new()` and `Deque[Int].new()` name their argument,
/// because `builtins.rs`' `Array.new` decision — an associated function's
/// type arguments come from the caller — applies to every generic `new`.
#[test]
fn the_graph_traversal_of_section_3_7_runs() {
    assert_eq!(
        prints(
            "reachable",
            "use collections (Deque)

type Graph:
    edges: Map[Int, Array[Int]]

Graph has:
    def neighbours(self, node: Int) -> Array[Int]:
        let mutable out be Array[Int].new()
        let found be self.edges.get(&node)
        if found?:
            for next in found:
                out.push(next)
        out

def reachable(graph: &Graph, start: Int) -> Set[Int]:
    let mutable seen be Set[Int].new()
    let mutable pending be Deque[Int].new()
    pending.push_back(start)
    loop:
        let node be pending.pop_front()
        if not node?:
            break
        if seen.contains(&node):
            continue
        seen.insert(node)
        write(f\"{node} \")
        for next in graph.neighbours(node):
            pending.push_back(next)
    seen

def main():
    let mutable edges be Map[Int, Array[Int]].new()
    let mutable one be Array[Int].new()
    one.push(2)
    one.push(3)
    edges.insert(1, one)
    let mutable two be Array[Int].new()
    two.push(4)
    edges.insert(2, two)
    let mutable three be Array[Int].new()
    three.push(4)
    three.push(1)
    three.push(5)
    edges.insert(3, three)
    let mutable five be Array[Int].new()
    five.push(6)
    edges.insert(5, five)
    let mutable nine be Array[Int].new()
    nine.push(1)
    edges.insert(9, nine)
    let graph be Graph(edges: edges)
    let seen be reachable(&graph, 1)
    print(\"\")
    print(seen.length())
    print(seen.contains(&9))
",
        ),
        "1 2 3 4 5 6 \n6\nfalse\n"
    );
}
