//! Input and output past `print`, **built, linked, run**.
//!
//! # `IoError.message()`, which was a miscompile
//!
//! `IoError implements Error` was a row with no methods, so `err.message()`
//! resolved to the *interface's* `message` — no body anywhere — and the
//! backend lowered a call to an interface method as a vtable dispatch
//! whatever the receiver was. On a one-byte error code that read two words of
//! pointer out of nothing: the optimiser proved it undefined and the program
//! trapped, or printed nothing and exited 0 when nothing followed the call.
//! `examples/19_stdlib.science` had the call on a path its run never takes.
//!
//! Two changes, both pinned here: `IoError`'s block declares `message` with
//! `science_io_error_message` as its body, and a dispatch whose receiver is
//! not an interface object is refused by name rather than lowered.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, run, scratch};
use science_codegen::target::OptLevel;

fn prints(name: &str, source: &str) -> String {
    let dir = scratch("io", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran.stdout
}

/// The call runs, returns the sentence, and the lines after it still print —
/// the last is what the miscompile lost.
#[test]
fn an_io_error_has_a_message() {
    assert_eq!(
        prints(
            "io_error_message",
            "def main():
    let err be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if err?:
        let text be err.message()
        print(text)
        print(text.length())
    print(\"after\")
",
        ),
        "not found\n9\nafter\n"
    );
}

/// A user's own `Write`, called through a bound, fed by `String.bytes()`.
///
/// Three things had to change for this to build, each its own defect:
///
/// - `Write.write`'s `Error?` was a nullable *path to the interface* in the
///   prelude and a nullable `any Error` in a user's source, so `SC0541`
///   refused every implementation for returning the type it was told to;
/// - `Write.write` is a builtin named `write`, and both `science-mir` and the
///   backend took any builtin of that name for the free function `write`
///   that prints — now they also ask that its parent be a module;
/// - `String.bytes()` had no lowering. It needs none beyond a pointer: a
///   `String` and an `Array of U8` are the same three words.
#[test]
fn a_user_writer_is_called_through_its_bound() {
    assert_eq!(
        prints(
            "user_writer",
            "type Counter:
    total: Int

Counter implements Write:
    def write(mutable self, bytes: &Array[U8]) -> Error?:
        self.total be self.total + bytes.length()
        null

def emit[W: Write](sink: &mut W, text: &String) -> Error?:
    sink.write(text.bytes())

def main():
    let mutable counter be Counter(total: 0)
    let first be emit(&mut counter, \"hello\")
    let second be emit(&mut counter, \", world\")
    if first? or second?:
        print(\"failed\")
    print(counter.total)
    let text be \"abc\"
    let bytes be text.bytes()
    print(bytes.length())
    let a be bytes.get(0)
    if a?:
        print(a)
    write(\"no newline\")
    print(\"\")
",
        ),
        "12\n3\n97\nno newline\n"
    );
}

/// Builds and runs `source` in a directory of its own, returning stdout and
/// the named file's contents afterwards.
fn writes(name: &str, source: &str, file: &str) -> (String, String) {
    let dir = scratch("io", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let output = std::process::Command::new(&built.executable)
        .current_dir(&dir)
        .output()
        .expect("the program runs");
    let written = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    (String::from_utf8_lossy(&output.stdout).into_owned(), written)
}

/// The bundled `io` module's `File`: created, written, closed — and a create
/// that fails reports why. It reaches the program through the harness's
/// `collect_crate` fallback, which is the driver's.
#[test]
fn a_file_from_the_bundled_io_module_is_written_and_closed() {
    let (stdout, written) = writes(
        "file_write_close",
        "use io (File)

def main():
    let missing, bad be File.create(\"/nonexistent-science-dir/x.txt\")
    if bad?:
        print(bad.message())
    let mutable file, err be File.create(\"out.txt\")
    if err?:
        panic(err.message())
    let w be file.write(\"one line\\n\".bytes())
    if w?:
        panic(w.message())
    let c be file.close()
    if c?:
        panic(c.message())
    print(\"closed\")
",
        "out.txt",
    );
    assert_eq!(stdout, "not found\nclosed\n");
    assert_eq!(written, "one line\n");
}

/// `BufferedWriter` hands its sink pieces of at least `capacity` bytes, and
/// flushes what is left when it is dropped — which needed Decision 12's
/// AMENDMENT 4, since a destructor that ran after its fields flushed a freed
/// buffer into a closed file, and `science_codegen::mono`'s `user_drops`,
/// since the `drop` of `BufferedWriter of W` is one function per `W`.
#[test]
fn a_buffered_writer_batches_and_flushes_when_dropped() {
    let (stdout, written) = writes(
        "buffered_writer",
        "use io (File, BufferedWriter)

type Counting:
    calls: Int
    bytes: Int

Counting implements Write:
    def write(mutable self, bytes: &Array[U8]) -> Error?:
        self.calls be self.calls + 1
        self.bytes be self.bytes + bytes.length()
        null

def main():
    let mutable counted be BufferedWriter.new(Counting(calls: 0, bytes: 0), 64)
    for i in 0..100:
        let line be f\"{i}\\n\"
        let _ignored be counted.write(line.bytes())
    let _flushed be counted.flush()
    print(counted.sink.calls)
    print(counted.sink.bytes)

    let file, err be File.create(\"dropped.txt\")
    if err?:
        panic(err.message())
    let mutable out be BufferedWriter.new(file, 1024)
    let w be out.write(\"never flushed by hand\\n\".bytes())
    if w?:
        panic(w.message())
",
        "dropped.txt",
    );
    assert_eq!(stdout, "5\n290\n", "100 short writes reach the sink as 5");
    assert_eq!(written, "never flushed by hand\n");
}

/// Builds and runs `source`, returning stdout and stderr: the two streams
/// `print` and `print_error` write to, kept apart so that a line on the wrong
/// one fails.
fn streams(name: &str, source: &str) -> (String, String) {
    let dir = scratch("io", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&dir, name), OptLevel::O2);
    let ran = run(&built);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    (ran.stdout, ran.stderr)
}

/// **`IoError implements Display`, run.** `print(err)`, `write(err)` and a
/// hole each render the sentence `err.message()` returns — printed beside them
/// so that the two cannot drift apart unseen — and printing borrows: `err` is
/// used five times and moved by none of them.
///
/// Before `science_string_push_io_error` this program was refused at the first
/// line inside the `if`, `SC0400`, *"`IoError` has no `display` this compiler
/// can call"*, although the prelude lists `IoError` as implementing `Display`.
#[test]
fn an_io_error_prints_its_message() {
    assert_eq!(
        prints(
            "io_error_display",
            "def main():
    let err be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if err?:
        print(err)
        write(err)
        print(\"|\")
        print(f\"write failed: {err} ({err.message()})\")
        print(err.message())
    print(\"after\")
",
        ),
        "not found\nnot found|\nwrite failed: not found (not found)\nnot found\nafter\n"
    );
}

/// `print_error(err)` and `write_error(err)` are the same rendering on the
/// other stream, and nothing of it reaches stdout.
#[test]
fn an_io_error_prints_to_stderr() {
    let (stdout, stderr) = streams(
        "io_error_display_stderr",
        "def main():
    let err be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if err?:
        print_error(err)
        write_error(err)
        print_error(f\"!{err}!\")
    print(\"done\")
",
    );
    assert_eq!(stdout, "done\n");
    assert_eq!(stderr, "not found\nnot found!not found!\n");
}

/// A borrowed `IoError` renders its referent, as a borrowed `String` does: the
/// hole is reborrowed through rather than read as a pointer to a pointer.
#[test]
fn a_borrowed_io_error_prints_its_referent() {
    assert_eq!(
        prints(
            "io_error_display_borrowed",
            "def report(err: &IoError):
    print(f\"[{err}]\")
    print(err)

def main():
    let err be write_file(\"/nonexistent-science-dir/x.txt\", \"hi\")
    if err?:
        report(err)
",
        ),
        "[not found]\nnot found\n"
    );
}
