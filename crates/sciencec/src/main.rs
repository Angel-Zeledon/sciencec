//! `sciencec` — the Science compiler's command-line driver.
//!
//! The front half of the compiler has been runnable from `cargo test` since it
//! was written; this is the first way to run it on a file. The binary is
//! deliberately thin: every command here is a call into an existing crate plus
//! the bookkeeping a shell needs — what goes to stdout, what goes to stderr,
//! and what the process exits with.
//!
//! ```text
//! sciencec check FILE...     lex, parse and resolve; report what is wrong
//! sciencec tokens FILE       dump the token stream
//! sciencec ast FILE          dump the syntax tree
//! sciencec resolve FILE      dump the resolved crate
//! ```
//!
//! # Where output goes
//!
//! Diagnostics and the `3 errors, 1 warning` summary go to **stderr**; dumps
//! go to **stdout**. That is what makes `sciencec ast f.science > f.ast`
//! useful and `sciencec check f.science` quiet when the file is clean.
//!
//! # The exit code
//!
//! `0` when nothing of severity `Error` was reported, `1` otherwise. Warnings
//! alone do not fail, so a CI job can gate on the exit code without arguing
//! about lint levels. A file that cannot be read is an error like any other:
//! it is reported, it sets the exit code, and the remaining files are still
//! checked.
//!
//! # Colour
//!
//! There is none, so `NO_COLOR` is honoured trivially. `science-diagnostics`
//! renders plain text on purpose — the UI suite compares it byte for byte —
//! and adding escape codes here would mean re-rendering diagnostics in the
//! driver, which is exactly what this binary must not do.

mod driver;
mod report;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use driver::Session;

const USAGE: &str = "\
sciencec — the Science compiler

Usage:
    sciencec check FILE...    lex, parse and resolve; report what is wrong
    sciencec tokens FILE      dump the token stream
    sciencec ast FILE         dump the syntax tree
    sciencec resolve FILE     dump the resolved crate
    sciencec --version        print the version
    sciencec --help           print this message

Diagnostics go to stderr, dumps to stdout.
Exits 0 when nothing was reported as an error, 1 otherwise; warnings alone
do not fail.";

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Outcome::Clean => ExitCode::SUCCESS,
        Outcome::Failed => ExitCode::FAILURE,
    }
}

enum Outcome {
    Clean,
    Failed,
}

fn run(args: &[OsString]) -> Outcome {
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return Outcome::Failed;
    };

    match command.to_str() {
        Some("--help") | Some("-h") | Some("help") => {
            println!("{USAGE}");
            return Outcome::Clean;
        }
        Some("--version") | Some("-V") => {
            println!("sciencec {}", env!("CARGO_PKG_VERSION"));
            return Outcome::Clean;
        }
        _ => {}
    }

    let files = match collect_files(rest) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{USAGE}");
            return Outcome::Failed;
        }
    };

    let mut session = Session::new();
    match command.to_str() {
        Some("check") => {
            if files.is_empty() {
                return usage_error("check expects at least one file");
            }
            session.check(&files);
        }
        Some(name @ ("tokens" | "ast" | "resolve")) => {
            let [file] = files.as_slice() else {
                return usage_error(&format!("{name} expects exactly one file"));
            };
            match name {
                "tokens" => session.dump_tokens(file),
                "ast" => session.dump_ast(file),
                _ => session.dump_resolved(file),
            }
        }
        other => {
            let shown = other.map(str::to_string).unwrap_or_else(|| command.to_string_lossy().into_owned());
            if shown.starts_with('-') {
                return usage_error(&format!("unknown option `{shown}`"));
            }
            return usage_error(&format!("unknown command `{shown}`"));
        }
    }

    session.finish()
}

/// Turns the operands into paths, rejecting anything that looks like a flag.
///
/// A path is not required to be valid UTF-8 — the filesystem's opinion wins
/// over the driver's — so operands stay `OsString` until they reach `Path`.
fn collect_files(args: &[OsString]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(text) = arg.to_str() {
            // A bare `-` is not stdin: a diagnostic has to name a file, and
            // `<stdin>` is a name no editor can jump to.
            if text.starts_with('-') && text.len() > 1 {
                return Err(format!("unknown option `{text}`"));
            }
        }
        files.push(PathBuf::from(arg));
    }
    Ok(files)
}

fn usage_error(message: &str) -> Outcome {
    eprintln!("error: {message}");
    eprintln!("{USAGE}");
    Outcome::Failed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_is_a_usage_error() {
        assert!(matches!(run(&args(&[])), Outcome::Failed));
    }

    #[test]
    fn help_and_version_succeed() {
        assert!(matches!(run(&args(&["--help"])), Outcome::Clean));
        assert!(matches!(run(&args(&["--version"])), Outcome::Clean));
    }

    #[test]
    fn unknown_command_fails() {
        assert!(matches!(run(&args(&["frobnicate", "x.science"])), Outcome::Failed));
    }

    #[test]
    fn check_without_files_fails() {
        assert!(matches!(run(&args(&["check"])), Outcome::Failed));
    }

    #[test]
    fn a_dump_command_takes_exactly_one_file() {
        assert!(matches!(run(&args(&["ast", "a.science", "b.science"])), Outcome::Failed));
    }

    #[test]
    fn flags_are_not_mistaken_for_files() {
        assert!(collect_files(&args(&["--jobs"])).is_err());
        assert!(collect_files(&args(&["a.science"])).is_ok());
        // A lone `-` is a path, however odd a one.
        assert!(collect_files(&args(&["-"])).is_ok());
    }
}
