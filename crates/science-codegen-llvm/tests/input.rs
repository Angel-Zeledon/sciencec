//! Reading input — `stdlib-core.md` §4's half that was missing — **built,
//! linked, run, and fed on standard input.**
//!
//! # What is here
//!
//! - The prelude's `read_line() -> (String?, IoError?)`, §4.4's ninth free
//!   function, over `science-rt`'s `science_read_line`: `null` at end of
//!   input, `""` for a blank line, a last line with no `\n` still a line, one
//!   `\r` at a line's end removed as `String.lines()` removes it, and bytes
//!   that are not UTF-8 an `IoError` that consumes the line.
//! - The bundled `io`'s read side: `File.open`, `File implements Read`,
//!   `Stdin`, `BufferedReader[R: Read]` and its `read_line`, and
//!   `BufferedLines`, the iterator `File.lines()` and `Stdin.lines()` return.
//! - The filter §4.4 says Science could not write without `read_line`: stdin
//!   to stdout, byte for byte.
//!
//! # Why a runner of its own
//!
//! `harness::run` gives the program no standard input, and every test here is
//! about what arrives on it. The runner below is `harness::run`'s shape — a
//! wall-clock budget, both output streams drained on their own threads — with
//! the input written on a third and the pipe closed after it, which is what
//! makes end of input arrive.

#![cfg(feature = "llvm")]

mod harness;

use harness::{executable, lower, require_runtime, scratch};
use science_codegen::target::OptLevel;

/// What running a program over some input produced.
struct Ran {
    stdout: Vec<u8>,
    stderr: String,
    status: Option<i32>,
    /// The working directory it ran in, kept for a test that looks at what
    /// the program left there; removed with `root` when this is dropped.
    work: std::path::PathBuf,
    root: std::path::PathBuf,
    executable: std::path::PathBuf,
}

impl Ran {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

impl Drop for Ran {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The same budget `harness::RUN_BUDGET` gives, for its reasons.
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// Builds `source`, writes `files` into an empty working directory, runs the
/// program there with `input` on its standard input, and returns what came
/// back. A program still running after [`BUDGET`] is killed and reported.
fn feeds(name: &str, source: &str, files: &[(&str, &[u8])], input: &[u8]) -> Ran {
    use std::io::{Read, Write};
    use std::process::Stdio;

    let root = scratch("input", name);
    require_runtime();
    let built = lower(source).build_at(&executable(&root, name), OptLevel::O2);
    let work = root.join("work");
    std::fs::create_dir_all(&work).expect("a working directory");
    for (file, bytes) in files {
        std::fs::write(work.join(file), bytes).expect("a fixture file");
    }

    let mut child = std::process::Command::new(&built.executable)
        .current_dir(&work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the program spawns");
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let input = input.to_vec();
    // Dropping `stdin` at the end of the thread closes the pipe, which is the
    // program's end of input. A program that exits before reading it all
    // makes the write fail, and that is not this runner's failure to report.
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr was piped");
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + BUDGET;
    let status = loop {
        match child.try_wait().expect("polling the child") {
            Some(status) => break Some(status),
            None if std::time::Instant::now() >= deadline => break None,
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };
    if status.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let _ = writer.join();
    let stdout = stdout_reader.join().expect("the stdout reader thread");
    let mut stderr = String::from_utf8_lossy(&stderr_reader.join().expect("the stderr reader")).into_owned();
    if status.is_none() {
        stderr.push_str(&format!("\n[input] killed after not exiting within {BUDGET:?}"));
    }
    Ran {
        stdout,
        stderr,
        status: status.and_then(|s| s.code()),
        work,
        root,
        executable: built.executable,
    }
}

/// [`feeds`], asserting a clean exit and nothing on stderr.
fn clean(name: &str, source: &str, files: &[(&str, &[u8])], input: &[u8]) -> Ran {
    let ran = feeds(name, source, files, input);
    assert_eq!(ran.status, Some(0), "stderr: {}", ran.stderr);
    assert_eq!(ran.stderr, "", "nothing belongs on stderr");
    ran
}

/// Every line of standard input, numbered and bracketed so that a blank line
/// and a missing one cannot look alike, then the count, then one more call
/// after the end to show the end stays the end.
const NUMBERED: &str = "def main():
    let mutable count be 0
    loop:
        let line, err be read_line()
        if err?:
            print(f\"error: {err}\")
            continue
        if not line?:
            break
        count be count + 1
        print(f\"{count} [{line}]\")
    let after, err be read_line()
    if err?:
        panic(err.message())
    if after?:
        panic(\"a line after the end of input\")
    print(f\"{count} lines\")
";

/// §4.4's two answers that a bare `String` could not tell apart — `""` for a
/// blank line and `null` for the end — and a last line with no `\n` after it,
/// which is a line and not lost.
#[test]
fn read_line_reads_every_line_and_then_null() {
    let ran = clean("read_line_lines", NUMBERED, &[], b"one\n\ntwo words\nlast, no newline");
    assert_eq!(ran.text(), "1 [one]\n2 []\n3 [two words]\n4 [last, no newline]\n4 lines\n");
}

/// No input at all is `null` on the first call, with no error.
#[test]
fn read_line_on_empty_input_is_null_at_once() {
    let ran = clean("read_line_empty", NUMBERED, &[], b"");
    assert_eq!(ran.text(), "0 lines\n");
}

/// **Decided: `\r\n` ends a line as `\n` does, and the `\r` is not part of
/// it** — `String.lines()`'s rule (`science_lines_next`), which removes one
/// `\r` from the end of every line, the last one with no `\n` included, so
/// reading a file line by line and reading it whole and splitting agree. A
/// `\r` anywhere else is text, and only one is removed.
#[test]
fn read_line_removes_one_carriage_return_at_the_end_of_a_line() {
    let ran = clean("read_line_crlf", NUMBERED, &[], b"dos\r\n\r\nmid\rdle\nboth\r\r\nlast\r");
    assert_eq!(ran.text(), "1 [dos]\n2 []\n3 [mid\rdle]\n4 [both\r]\n5 [last]\n5 lines\n");
}

/// Bytes that are not UTF-8 are an `IoError` — *"invalid data"*, as
/// `read_file` says of the same bytes — and the line is consumed, so the next
/// call reads the line after it rather than failing forever.
#[test]
fn read_line_reports_a_line_that_is_not_utf8_and_moves_past_it() {
    let ran = clean("read_line_not_utf8", NUMBERED, &[], b"before\n\xff\xfe bad\nafter\n");
    assert_eq!(ran.text(), "1 [before]\nerror: invalid data\n2 [after]\n2 lines\n");
}

/// §4.7's program, transcribed: `read_line`'s error is an `IoError?`, so a
/// function returning `(Int, IoError?)` hands it on unconverted — the reason
/// the prelude declares §7.3's concrete type where §4.4 writes `Error?`.
#[test]
fn the_count_lines_program_of_section_4_7_runs() {
    let ran = clean(
        "count_lines",
        "def count_lines() -> (Int, IoError?):
    let mutable total be 0
    loop:
        let line, err be read_line()
        if err?:
            return (0, err)
        if not line?:
            break
        total be total + 1
    print(f\"{total} lines\")
    return (total, null)

def main():
    let total, err be count_lines()
    if err?:
        panic(err.message())
    print(total)
",
        &[],
        b"a\nb\n\nc\n",
    );
    assert_eq!(ran.text(), "4 lines\n4\n");
}

/// The filter §4.4 exists for: standard input to standard output, line by
/// line, through `read_line` and `print`. Forty thousand lines of varying
/// length come out byte for byte — enough to cross `std`'s 8 KiB input buffer
/// and stdout's 64 KiB block several times over.
#[test]
fn a_cat_written_with_read_line_copies_its_input_exactly() {
    let mut input = String::new();
    for i in 0..40_000 {
        input.push_str(&format!("line {i} {}\n", "x".repeat(i % 97)));
    }
    let ran = clean(
        "cat_read_line",
        "def main():
    loop:
        let line, err be read_line()
        if err?:
            panic(err.message())
        if not line?:
            break
        print(line)
",
        &[],
        input.as_bytes(),
    );
    assert!(ran.stdout == input.as_bytes(), "the copy differs from its input");
}

/// The same filter through `io`: a `BufferedReader` over `Stdin`, read with
/// `Read.read` into one buffer and written with `write`, so no line structure
/// is involved at all — and a first line taken with the prelude's
/// `read_line`, which shares standard input's one buffer with `Stdin`, so the
/// two can be mixed without a byte falling between them.
#[test]
fn a_cat_written_with_stdin_and_a_buffered_reader_copies_its_input_exactly() {
    let mut input = Vec::new();
    input.extend_from_slice(b"header\n");
    for i in 0..20_000u32 {
        input.extend_from_slice(format!("{i}\t{}\n", i * 7).as_bytes());
    }
    input.extend_from_slice(b"no newline at the end");
    let ran = clean(
        "cat_buffered",
        "use io (BufferedReader, Stdin)

def main():
    let first, err be read_line()
    if err?:
        panic(err.message())
    if first?:
        print(f\"first: {first}\")
    let mutable reader be BufferedReader.new(Stdin.new(), 4096)
    let mutable chunk be Array[U8].new()
    for _ in 0..1000:
        chunk.push(0)
    loop:
        let count, failed be reader.read(&mut chunk)
        if failed?:
            panic(failed.message())
        if count is 0:
            break
        let mutable piece be Array[U8].new()
        for i in 0..(count as Int):
            piece.push(chunk[i])
        let text, bad be String.from_bytes(&piece)
        if bad?:
            panic(bad.message())
        write(text)
",
        &[],
        &input,
    );
    let rest = &input[b"header\n".len()..];
    let mut want = b"first: header\n".to_vec();
    want.extend_from_slice(rest);
    assert!(ran.stdout == want, "the copy differs from its input");
}

/// `File.open` and `File implements Read`: one unbuffered read fills as much
/// of the buffer as there is, a second gets the rest, a third gets `0` and no
/// error — §4.2's only end-of-input signal. A file that is not there is
/// `"not found"`.
#[test]
fn a_file_is_opened_and_read_into_a_buffer() {
    let ran = clean(
        "file_read",
        "use io (File)

def main():
    let missing, bad be File.open(\"absent.txt\")
    if bad?:
        print(bad.message())
    let mutable file, err be File.open(\"data.txt\")
    if err?:
        panic(err.message())
    let mutable buffer be Array[U8].new()
    for _ in 0..8:
        buffer.push(0)
    loop:
        let count, failed be file.read(&mut buffer)
        if failed?:
            panic(failed.message())
        print(count)
        if count is 0:
            break
        let mutable piece be Array[U8].new()
        for i in 0..(count as Int):
            piece.push(buffer[i])
        let text, wrong be String.from_bytes(&piece)
        if wrong?:
            panic(wrong.message())
        print(f\"[{text}]\")
    let closed be file.close()
    if closed?:
        panic(closed.message())
",
        &[("data.txt", b"twelve bytes")],
        b"",
    );
    assert_eq!(ran.text(), "not found\n8\n[twelve b]\n4\n[ytes]\n0\n");
}

/// A `BufferedReader` over a `File`, line by line, with a buffer of four bytes
/// so that lines, a `\r\n` and the end of the file all fall across refills;
/// then the same file through `File.lines()`, which must agree with it.
#[test]
fn a_buffered_reader_reads_a_file_line_by_line() {
    let ran = clean(
        "buffered_lines",
        "use io (BufferedReader, File)

def main():
    let file, err be File.open(\"poem.txt\")
    if err?:
        panic(err.message())
    let mutable reader be BufferedReader.new(file, 4)
    loop:
        let line, failed be reader.read_line()
        if failed?:
            panic(failed.message())
        if not line?:
            break
        print(f\"[{line}]\")
    let again, bad be File.open(\"poem.txt\")
    if bad?:
        panic(bad.message())
    let mutable lines be again.lines()
    for line in lines:
        print(f\"<{line}>\")
",
        &[("poem.txt", b"so much depends\r\nupon\n\na red wheel\nbarrow\r")],
        b"",
    );
    assert_eq!(
        ran.text(),
        "[so much depends]\n[upon]\n[]\n[a red wheel]\n[barrow]\n\
         <so much depends>\n<upon>\n<>\n<a red wheel>\n<barrow>\n"
    );
}

/// `BufferedLines` over an arbitrary `Read` — §4.5's *"`Lines` … over an
/// arbitrary `Read`"* — here a user's in-memory source that hands out three
/// bytes at a time. A line that is not UTF-8 ends the iteration, and
/// `failure()` says it ended short of the end of input.
#[test]
fn lines_come_from_any_read_and_stop_at_the_first_failure() {
    let ran = clean(
        "user_read_lines",
        "use io (BufferedReader)

type Trickle:
    bytes: Array[U8]
    at: Int

Trickle implements Read:
    def read(mutable self, into: &mut Array[U8]) -> (U64, Error?):
        let mutable count be 0
        loop:
            if count >= 3 or count >= into.length() or self.at >= self.bytes.length():
                break
            into[count] be self.bytes[self.at]
            self.at be self.at + 1
            count be count + 1
        (count as U64, null)

def source(text: &String) -> Trickle:
    let mutable bytes be Array[U8].new()
    for byte in text.bytes():
        bytes.push(byte)
    Trickle(bytes: bytes, at: 0)

def main():
    let mutable good be BufferedReader.new(source(\"alpha\\nbeta\\ngamma\"), 2).lines()
    for line in good:
        print(line)
    print(good.failure())

    let mutable broken be source(\"ok\\n\")
    broken.bytes.push(255)
    broken.bytes.push(10)
    for byte in \"never\\n\".bytes():
        broken.bytes.push(byte)
    let mutable lines be BufferedReader.new(broken, 16).lines()
    for line in lines:
        print(line)
    print(lines.failure())
",
        &[],
        b"",
    );
    assert_eq!(ran.text(), "alpha\nbeta\ngamma\nfalse\nok\ntrue\n");
}

/// `Stdin.lines()`: the same iterator over standard input.
#[test]
fn stdin_lines_iterates_standard_input() {
    let ran = clean(
        "stdin_lines",
        "use io (Stdin)

def main():
    let mutable count be 0
    for line in Stdin.new().lines():
        count be count + 1
        print(f\"{count}: {line}\")
",
        &[],
        b"a\r\nb\n\nc",
    );
    assert_eq!(ran.text(), "1: a\n2: b\n3: \n4: c\n");
}

/// Two thousand files opened, read line by line through `File.lines()` and
/// dropped — the iterator owning the reader owning the `File`, whose `drop`
/// closes it — and two thousand `read_line` calls past the end of input, and
/// nothing is left behind.
///
/// The live set is constant: the same file, the same three lines, dropped at
/// the end of each iteration. A descriptor that was not closed fails the loop
/// itself, long before two thousand, against the default limit of 256; and on
/// macOS `leaks` checks the heap at exit.
#[test]
fn a_reading_loop_leaks_nothing() {
    let source = "use io (File)

def main():
    let mutable total be 0
    for _ in 0..2000:
        let file, err be File.open(\"three.txt\")
        if err?:
            panic(err.message())
        for line in file.lines():
            total be total + line.length()
        let line, failed be read_line()
        if failed?:
            panic(failed.message())
        if line?:
            total be total + line.length()
    print(total)
";
    let ran = clean("leak_loop", source, &[("three.txt", b"one\ntwo\nthree\n")], b"x\ny\n");
    assert_eq!(ran.text(), "22002\n");
    let leaks = std::path::Path::new("/usr/bin/leaks");
    if cfg!(target_os = "macos") && leaks.is_file() {
        let report = std::process::Command::new(leaks)
            .arg("--atExit")
            .arg("--")
            .arg(&ran.executable)
            .current_dir(&ran.work)
            .stdin(std::fs::File::open(ran.work.join("three.txt")).expect("the fixture"))
            .output()
            .expect("`leaks` runs");
        let text = String::from_utf8_lossy(&report.stdout);
        assert!(text.contains(" 0 leaks for 0 total leaked bytes"), "`leaks` found something:\n{text}");
    }
}
