//! Two-phase borrows in argument position, **built, linked, run**.
//!
//! `fail(c, "msg", c.pos)` reserves a `&mut` to `c` and reads `c.pos` for the
//! third argument. The read has to happen before the activation, so MIR copies
//! it into a temporary; this runs the program to prove the value that reaches
//! the callee is the one read, and that a borrow variable reassigned in a loop
//! still walks where it should.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("two_phase_arguments", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

#[test]
fn a_later_argument_reads_the_reserved_place() {
    assert_eq!(
        prints(
            "read",
            "type Cursor:\n\
             \x20   pos: Int\n\
             \x20   errs: Int\n\
             \n\
             def fail(c: &mut Cursor, m: String, at: Int):\n\
             \x20   c.errs be c.errs + at\n\
             \x20   print(m)\n\
             \n\
             def main():\n\
             \x20   let mutable c be Cursor(pos: 3, errs: 0)\n\
             \x20   fail(c, \"msg\", c.pos)\n\
             \x20   c.pos be 10\n\
             \x20   fail(c, \"again\", c.pos + c.errs)\n\
             \x20   print(c.errs)\n",
        ),
        "msg\nagain\n16\n"
    );
}

#[test]
fn a_shared_borrow_variable_reassigned_in_a_loop() {
    assert_eq!(
        prints(
            "walk",
            "type Tree:\n\
             \x20   v: Int\n\
             \x20   kids: Array[Tree]\n\
             \n\
             def first_kid(t: &Tree) -> (&Tree)?:\n\
             \x20   t.kids.get(0)\n\
             \n\
             def main():\n\
             \x20   let t be Tree(v: 1, kids: [Tree(v: 2, kids: [Tree(v: 3, kids: [])])])\n\
             \x20   let mutable current be &t\n\
             \x20   let mutable depth be 0\n\
             \x20   loop:\n\
             \x20       let next be first_kid(current)\n\
             \x20       if next?:\n\
             \x20           current be next\n\
             \x20           depth be depth + 1\n\
             \x20       else:\n\
             \x20           break\n\
             \x20   print(current.v)\n\
             \x20   print(depth)\n",
        ),
        "3\n2\n"
    );
}
