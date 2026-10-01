//! The bundled `os` module's `args` and `env`, **built, linked, and run with a
//! command line and an environment of the test's choosing**.
//!
//! # What these pin
//!
//! `crates/science-resolve/stdlib/os.science` is Science over five
//! `science-rt` entry points (`crates/science-rt/src/os.rs`), and the protocol
//! between them — a length, then a copy into a buffer Science allocated, then
//! `String.from_bytes` — is what each test here crosses. The decisions it pins:
//! argument zero is included; an argument with spaces is one argument; a
//! non-ASCII argument arrives byte for byte; a non-UTF-8 one arrives with
//! U+FFFD in place of each invalid sequence rather than refused; an empty
//! variable is set and empty, and an unset one is `null`.
//!
//! # Why not `harness::run`
//!
//! It runs a program with no arguments and the test's own environment, which is
//! exactly what these tests must control. [`run_with`] is `harness::run`'s
//! budget with a `Command` the test configures; the programs print short
//! output, so `wait_with_output` cannot fill a pipe.

#![cfg(feature = "llvm")]

mod harness;

use std::ffi::OsString;
use std::process::{Command, Stdio};

use harness::{executable, lower, require_runtime, scratch};
use science_codegen::target::OptLevel;

/// Prints the argument count, then each argument between brackets — so an
/// empty argument and one with spaces are both visible — skipping argument
/// zero, whose spelling is the scratch path.
const ARGUMENTS: &str = "use os (args)

def main():
    let all be args()
    print(all.length())
    for index in 1..all.length():
        let argument be all.get(index)
        if argument?:
            print(f\"[{argument}]\")
";

/// Prints what three lookups found, `null` spelled out.
const VARIABLES: &str = "use os (env)

def show(name: &String):
    let value be env(name)
    if value?:
        print(f\"{name}=<{value}> {value.length()}\")
    else:
        print(f\"{name} unset\")

def main():
    show(\"SCIENCE_OS_SET\")
    show(\"SCIENCE_OS_EMPTY\")
    show(\"SCIENCE_OS_UNSET\")
    show(\"\")
    show(\"A=B\")
";

/// Build `source` once, run it with `configure` applied, and return stdout.
fn run_with(name: &str, source: &str, configure: impl FnOnce(&mut Command)) -> String {
    let dir = scratch("os", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let mut command = Command::new(&built.executable);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure(&mut command);
    let mut child = command.spawn().expect("the program spawns");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while child.try_wait().expect("polling the child").is_none() {
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("[os] killed after not exiting within 10s");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let output = child.wait_with_output().expect("the program's output");
    let _ = std::fs::remove_dir_all(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
    assert_eq!(stderr, "", "nothing belongs on stderr");
    String::from_utf8(output.stdout).expect("Science prints UTF-8").replace("\r\n", "\n")
}

/// With nothing after the program's name, `args()` is argument zero alone.
#[test]
fn with_no_arguments_args_is_argument_zero_alone() {
    assert_eq!(run_with("no_arguments", ARGUMENTS, |_| {}), "1\n");
}

/// Argument zero is what the process was started as — here, the executable's
/// path — and the program can read it.
#[test]
fn argument_zero_is_the_program() {
    let source = "use os (args)

def main():
    let all be args()
    let zero be all.get(0)
    if zero?:
        print(zero.ends_with(\"argument_zero\"))
";
    assert_eq!(run_with("argument_zero", source, |_| {}), "true\n");
}

/// Several arguments, in order: one with spaces stays one, an empty one is
/// kept, and non-ASCII text arrives byte for byte.
#[test]
fn several_arguments_arrive_in_order_and_whole() {
    let stdout = run_with("several", ARGUMENTS, |command| {
        command.args(["first", "two words", "", "héllo wörld", "世界", "--flag=1"]);
    });
    assert_eq!(stdout, "7\n[first]\n[two words]\n[]\n[héllo wörld]\n[世界]\n[--flag=1]\n");
}

/// An argument that is not UTF-8 is not refused: each invalid sequence becomes
/// U+FFFD, and the valid bytes around it survive. `os.rs` gives the reason —
/// `args` has no error channel — and the cost.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_utf8_arrives_with_replacement_characters() {
    use std::os::unix::ffi::OsStringExt;
    let stdout = run_with("not_utf8", ARGUMENTS, |command| {
        command.arg(OsString::from_vec(b"ok\xffthen\xc3".to_vec())).arg("after");
    });
    assert_eq!(stdout, "3\n[ok\u{FFFD}then\u{FFFD}]\n[after]\n");
}

/// Present, empty, unset — §4.6's *"An empty variable is set and empty"* —
/// and the two names no variable can have, which are unset rather than an
/// error.
#[test]
fn env_tells_present_from_empty_from_unset() {
    let stdout = run_with("variables", VARIABLES, |command| {
        command
            .env("SCIENCE_OS_SET", "value with spaces, ünïcode")
            .env("SCIENCE_OS_EMPTY", "")
            .env_remove("SCIENCE_OS_UNSET");
    });
    assert_eq!(
        stdout,
        "SCIENCE_OS_SET=<value with spaces, ünïcode> 28\n\
         SCIENCE_OS_EMPTY=<> 0\n\
         SCIENCE_OS_UNSET unset\n \
         unset\n\
         A=B unset\n"
    );
}

/// A value that is not UTF-8 is replaced exactly as an argument is.
#[cfg(unix)]
#[test]
fn a_value_that_is_not_utf8_arrives_with_replacement_characters() {
    use std::os::unix::ffi::OsStringExt;
    let source = "use os (env)

def main():
    let value be env(\"SCIENCE_OS_BYTES\")
    if value?:
        print(f\"<{value}>\")
";
    let stdout = run_with("value_not_utf8", source, |command| {
        command.env("SCIENCE_OS_BYTES", OsString::from_vec(b"a\xfeb".to_vec()));
    });
    assert_eq!(stdout, "<a\u{FFFD}b>\n");
}
