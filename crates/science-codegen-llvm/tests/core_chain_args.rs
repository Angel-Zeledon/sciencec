//! Closures and loops handed to a chain, each built, linked and run.
//!
//! - a `let`-bound closure and a top-level function over borrowed `Copy`
//!   items: the chain hands `&Int`, the closure and the function take `Int`;
//! - a closure that calls a closure it captured.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("core-chain-args", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn a_named_closure_over_a_copy_item_is_adapted_to_the_borrowed_item() {
    let source = r#"def half(x: Int) -> Int:
    x / 2

let xs be [2, 4, 6]
let add_one be x giving x + 1
let ys be xs.iterate().map(add_one).collect()
for y in ys:
    print(y)
let hs be xs.iterate().map(half).collect()
for h in hs:
    print(h)
print(add_one(10))
"#;
    assert_eq!(prints("named_closure", source), "3\n5\n7\n1\n2\n3\n11\n");
}

#[test]
fn a_closure_may_call_a_closure_it_captured() {
    let source = r#"let add_one be x giving x + 1
let twice be y giving add_one(add_one(y))
print(twice(3))
print(add_one(1))
"#;
    assert_eq!(prints("captured_call", source), "5\n2\n");
}
