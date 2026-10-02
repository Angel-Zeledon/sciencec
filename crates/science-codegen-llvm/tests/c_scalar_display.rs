//! `print` and `f"{x}"` of the C scalars (`CInt`, `CUInt`, `CLong`, ...),
//! **built, linked, run**.
//!
//! These are deliberately not aliases of the Science integers (§1.6 of the FFI
//! note), so the interpolation builder had no entry point for them and
//! `print(x)` of a `CInt` was `SC0400`. They now render like the integer or
//! float of the same width and signedness, which the backend reads off the
//! target. A `type Handle is CInt` alias reveals to the same thing.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("c_scalar_display", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn every_c_scalar_prints_like_the_number_it_is() {
    let source = "\
def main():
    let a: CChar be -7
    let b: CInt be -5
    let c: CUInt be 4000000000
    let d: CLong be -9
    let e: CULong be 10
    let f: CLongLong be -11
    let g: CULongLong be 12
    let h: CSizeT be 13
    let i: CPtrDiff be -14
    let j: CFloat be 0.5
    let k: CDouble be 2.25
    print(a)
    print(b)
    print(c)
    print(d)
    print(e)
    print(f)
    print(g)
    print(h)
    print(i)
    print(j)
    print(k)
    print(f\"[{b}] [{c}] [{k}]\")
";
    assert_eq!(
        prints("scalars", source),
        "-7\n-5\n4000000000\n-9\n10\n-11\n12\n13\n-14\n0.5\n2.25\n[-5] [4000000000] [2.25]\n"
    );
}

#[test]
fn a_c_scalar_behind_an_alias_prints() {
    let source = "\
unsafe extern \"C\" library \"c\":
    type Layout is CInt

def main():
    let layout: Layout be 101
    print(layout)
    print(f\"layout {layout}\")
";
    assert_eq!(prints("alias", source), "101\nlayout 101\n");
}
