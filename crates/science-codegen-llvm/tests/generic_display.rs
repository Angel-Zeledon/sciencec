//! `f"{x}"` and `print(x)` on a generic `x: T` with `T: Display`, **built,
//! linked, run**.
//!
//! This was `SC0400` — *"an `f"…"` hole of type `I64`, which neither rendering
//! reaches … a type parameter"* — because `science-mir` lowers the hole once,
//! in the generic body, where `T` has no `Display` it can name. It now calls
//! the interface's `display`; `science-codegen`'s monomorphiser redirects that
//! to the concrete `Self` (a user's own `display`), and for a prelude type
//! `PRELUDE_METHODS` maps it to a `science_display_*` entry point. A spec on
//! the hole reaches the same `Formatter`, so it is honoured too.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("generic_display", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

const SHOW: &str = "\
def show[T](x: &T) -> String
        where T: Display:
    f\"<{x}>\"

def padded[T](x: &T) -> String
        where T: Display:
    f\"[{x:>6}]\"

def shout[T](x: &T)
        where T: Display:
    print(x)
";

#[test]
fn a_generic_hole_renders_through_the_concrete_types_display() {
    let source = format!(
        "{SHOW}
type Point:
    x: Int
    y: Int

Point implements Display:
    def display(self, into: &mut Formatter):
        into.text(f\"({{self.x}}, {{self.y}})\")

def main():
    print(show(Point(x: 1, y: 2)))
    print(show(true))
    print(show('c'))
    print(show(3.25))
    print(show(-4))
    let small: I8 be -5
    print(show(small))
    let big: U64 be 18446744073709551615
    print(show(big))
    let name be \"ada\"
    print(show(name))
    shout(2.5)
    shout(Point(x: 3, y: 4))
"
    );
    assert_eq!(
        prints("hole", &source),
        "<(1, 2)>\n<true>\n<c>\n<3.25>\n<-4>\n<-5>\n<18446744073709551615>\n<ada>\n2.5\n(3, 4)\n"
    );
}

#[test]
fn a_spec_on_a_generic_hole_is_honoured() {
    let source = format!(
        "{SHOW}
def main():
    print(padded(42))
    print(padded(\"ab\"))
    print(padded(true))
"
    );
    assert_eq!(prints("spec", &source), "[    42]\n[    ab]\n[  true]\n");
}

#[test]
fn a_generic_taken_by_value_prints() {
    let source = "\
def by_value[T](x: T)
        where T: Display:
    print(x)

def main():
    let n be 7
    by_value(n)
    let s be \"owned\".clone()
    by_value(s)
";
    assert_eq!(prints("by_value", source), "7\nowned\n");
}
