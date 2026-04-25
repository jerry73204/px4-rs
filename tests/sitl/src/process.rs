//! Process-group helpers. Lifted from `nros-tests/src/process.rs`,
//! trimmed to what `Px4Sitl` needs.

use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// Put the spawned process into its own process group + arrange for
/// it to receive SIGKILL if the test runner dies.
///
/// PX4's `px4 -d` daemon forks worker threads/pthreads but stays in
/// the same group as the parent unless we tell it otherwise. With
/// our own group we can SIGTERM the whole tree later via `kill(-pid)`.
#[cfg(unix)]
pub fn set_new_process_group(command: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: setpgid + prctl are async-signal-safe and called only
    // between fork() and execve() — no Rust state is touched.
    unsafe {
        command.pre_exec(|| {
            libc::setpgid(0, 0);
            #[cfg(target_os = "linux")]
            {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            }
            Ok(())
        })
    }
}

#[cfg(not(unix))]
pub fn set_new_process_group(command: &mut Command) -> &mut Command {
    command
}

/// SIGTERM the process group, wait up to `grace` for clean exit,
/// then SIGKILL if necessary. Always reaps the child so no zombie
/// is left behind.
#[cfg(unix)]
pub fn graceful_kill(handle: &mut Child, grace: Duration) {
    let pid = handle.id() as libc::pid_t;
    // SAFETY: kill on a known PID is benign even if the process is gone.
    unsafe {
        libc::kill(-pid, libc::SIGTERM);
    }

    let start = Instant::now();
    loop {
        match handle.try_wait() {
            Ok(Some(_)) => return, // exited cleanly
            Ok(None) if start.elapsed() < grace => {
                std::thread::sleep(Duration::from_millis(50));
            }
            _ => break, // timed out or wait failed
        }
    }

    // SAFETY: same as above.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    let _ = handle.wait();
}

#[cfg(not(unix))]
pub fn graceful_kill(handle: &mut Child, _grace: Duration) {
    let _ = handle.kill();
    let _ = handle.wait();
}
