//! Waiting for a child program without mistaking a queue for a hang.
//!
//! This file is compiled twice: as a module of `sciencec` (for `sciencec
//! test`) and, by `#[path]`, into `science-codegen-llvm`'s test harness. It
//! depends on `std` alone so that both can have it.
//!
//! # The problem
//!
//! A wall-clock budget on a freshly linked program measures two things at
//! once: the program, and how long the machine takes to let it start. macOS
//! checks the first launch of every new unsigned binary through a
//! machine-wide verification daemon, and under load that queue is long.
//! Measured, on a machine with a load average near 40: a program that prints
//! `1` took 1 to 40 seconds to *start*, run once, with or without an ad-hoc
//! `codesign -s -` (signing does not avoid it), and well under 10 ms to run.
//! A ten-second wall clock therefore failed correct programs, constantly.
//!
//! # The decision
//!
//! The budget is **CPU time the child itself has consumed**. A process stuck
//! in the launch queue has used none, so waiting costs it nothing; a program
//! spinning in a loop that never ends accrues CPU at about one second per
//! second and is killed when it passes `cpu`. A hang that burns no CPU (a
//! deadlock, a blocked read) is caught by `wall`, a backstop long enough to
//! outlast the queue.
//!
//! CPU time is read from the OS while the child runs: `proc_pidinfo` on
//! macOS, `/proc/PID/stat` on Linux. Where neither exists the CPU budget
//! cannot be measured and the wall clock is held to `wall` alone.

#![allow(dead_code)]

use std::process::{Child, ExitStatus};
use std::time::{Duration, Instant};

/// How a wait ended when the child did not exit by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overrun {
    /// The child used more than the CPU budget: it is computing, not queued.
    Cpu,
    /// The wall-clock backstop passed first: it is stuck without computing, or
    /// the machine never let it start.
    Wall,
}

/// Wait for `child`; `Ok(Ok(status))` if it exited, `Ok(Err(why))` if a budget
/// ran out first. The child has been killed and reaped in the second case.
pub fn wait_within(
    child: &mut Child,
    cpu: Duration,
    wall: Duration,
) -> std::io::Result<Result<ExitStatus, Overrun>> {
    let start = Instant::now();
    let pid = child.id();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Ok(status));
        }
        let overrun = if start.elapsed() >= wall {
            Some(Overrun::Wall)
        } else {
            match cpu_time(pid) {
                Some(used) if used >= cpu => Some(Overrun::Cpu),
                _ => None,
            }
        };
        if let Some(why) = overrun {
            // Best-effort: the process is already misbehaving, and a kill or a
            // reap failing is not this function's failure to report.
            let _ = child.kill();
            let _ = child.wait();
            return Ok(Err(why));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// User plus system CPU time `pid` has used so far, or `None` when it cannot
/// be read (the process is gone, or this platform has no way to ask).
#[cfg(target_os = "macos")]
pub fn cpu_time(pid: u32) -> Option<Duration> {
    use std::os::raw::{c_int, c_void};

    // `struct proc_taskinfo` from <sys/proc_info.h>.
    #[repr(C)]
    #[derive(Default)]
    struct TaskInfo {
        virtual_size: u64,
        resident_size: u64,
        total_user: u64,
        total_system: u64,
        threads_user: u64,
        threads_system: u64,
        policy: i32,
        faults: i32,
        pageins: i32,
        cow_faults: i32,
        messages_sent: i32,
        messages_received: i32,
        syscalls_mach: i32,
        syscalls_unix: i32,
        csw: i32,
        threadnum: i32,
        numrunning: i32,
        priority: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    extern "C" {
        fn proc_pidinfo(
            pid: c_int,
            flavor: c_int,
            arg: u64,
            buffer: *mut c_void,
            size: c_int,
        ) -> c_int;
        fn mach_timebase_info(info: *mut Timebase) -> c_int;
    }
    const PROC_PIDTASKINFO: c_int = 4;

    let mut info = TaskInfo::default();
    let size = std::mem::size_of::<TaskInfo>() as c_int;
    // SAFETY: `info` is a correctly laid-out, writable `proc_taskinfo` of
    // `size` bytes; the call writes at most that many.
    let wrote = unsafe {
        proc_pidinfo(pid as c_int, PROC_PIDTASKINFO, 0, (&mut info as *mut TaskInfo).cast(), size)
    };
    if wrote != size {
        return None;
    }
    let mut base = Timebase::default();
    // SAFETY: writes one `mach_timebase_info_data_t` into `base`.
    if unsafe { mach_timebase_info(&mut base) } != 0 || base.denom == 0 {
        return None;
    }
    let ticks = info.total_user.saturating_add(info.total_system);
    let nanos = (ticks as u128) * (base.numer as u128) / (base.denom as u128);
    Some(Duration::from_nanos(nanos.min(u64::MAX as u128) as u64))
}

#[cfg(target_os = "linux")]
pub fn cpu_time(pid: u32) -> Option<Duration> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is parenthesised and may contain spaces; the fields
    // that matter come after its closing parenthesis. utime and stime are
    // fields 14 and 15, i.e. indices 11 and 12 after the state field.
    let rest = &stat[stat.rfind(')')? + 2..];
    let mut fields = rest.split(' ');
    let utime: u64 = fields.nth(11)?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    // USER_HZ is 100 on every Linux this compiler targets.
    Some(Duration::from_millis((utime + stime) * 10))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn cpu_time(_pid: u32) -> Option<Duration> {
    None
}
