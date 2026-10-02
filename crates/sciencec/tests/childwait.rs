//! `childwait::wait_within`: a CPU budget that a queue cannot spend, and a
//! wall backstop for a hang that burns nothing.

#![cfg(unix)]

#[path = "../src/childwait.rs"]
mod childwait;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use childwait::{Overrun, wait_within};

fn sh(script: &str) -> std::process::Child {
    Command::new("/bin/sh").arg("-c").arg(script).stdout(Stdio::null()).spawn().expect("sh spawns")
}

#[test]
fn a_program_that_exits_is_reported_with_its_status() {
    let mut child = sh("exit 3");
    let outcome = wait_within(&mut child, Duration::from_secs(30), Duration::from_secs(60)).unwrap();
    assert_eq!(outcome.unwrap().code(), Some(3));
}

#[test]
fn a_spinning_program_is_killed_on_cpu_time() {
    let mut child = sh("while :; do :; done");
    let began = Instant::now();
    let outcome = wait_within(&mut child, Duration::from_millis(500), Duration::from_secs(120)).unwrap();
    assert_eq!(outcome.unwrap_err(), Overrun::Cpu);
    assert!(began.elapsed() < Duration::from_secs(60), "CPU budget, not the backstop, ended it");
}

#[test]
fn a_sleeping_program_spends_no_cpu_and_is_caught_by_the_wall_backstop() {
    let mut child = sh("sleep 60");
    let outcome = wait_within(&mut child, Duration::from_millis(300), Duration::from_secs(2)).unwrap();
    assert_eq!(outcome.unwrap_err(), Overrun::Wall);
}

#[test]
fn a_program_that_waits_longer_than_the_cpu_budget_but_computes_nothing_is_not_killed() {
    // The queueing case: wall time passes past the CPU budget, CPU does not.
    let mut child = sh("sleep 1");
    let outcome = wait_within(&mut child, Duration::from_millis(300), Duration::from_secs(60)).unwrap();
    assert!(outcome.unwrap().success());
}
