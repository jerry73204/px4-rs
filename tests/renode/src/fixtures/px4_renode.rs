//! `Px4RenodeSitl` — RAII handle for a PX4 + NuttX firmware running
//! under a child Renode process.
//!
//! ```ignore
//! use px4_renode_tests::Px4RenodeSitl;
//! use std::time::Duration;
//!
//! let sitl = Px4RenodeSitl::boot()?;
//! sitl.shell("uorb status")?;
//! sitl.wait_for_log("Startup script returned successfully", Duration::from_secs(30))?;
//! // Renode + firmware are killed cleanly when `sitl` goes out of scope.
//! ```
//!
//! Runtime model
//! -------------
//! - `boot()` allocates a pty pair, spawns `renode --console -e
//!   "$bin=…; $slave=…; include @<resc>"`, and points the
//!   firmware's UART2 at the pty slave via the `.resc` script. The
//!   host opens the pty master and tails it.
//! - `shell(cmd)` writes `cmd\r\n` to the pty and reads until the
//!   `pxh>` prompt comes back. The pxh shell loop in PX4's posix
//!   build is reused on NuttX with the same prompt string.
//! - `wait_for_log(pat, dur)` blocks on the shared log buffer until
//!   `pat` appears as a substring of any line, or `dur` elapses.
//! - `Drop` SIGTERMs the Renode process group, waits 3 s, then
//!   SIGKILLs. Without this, an orphan Renode would hold the
//!   Monitor TCP port and the UART pty across the next test run.
//!
//! Skip detection
//! --------------
//! [`renode_available`] returns `false` (and the [`crate::ensure_renode!`]
//! macro skip-returns) when either `RENODE` (the binary path) or
//! `PX4_RENODE_FIRMWARE` (the .elf to boot) is missing. The lighter
//! [`renode_binary_available`] only requires `RENODE`; tests that
//! exercise the platform file without needing firmware (see
//! [`Px4RenodeSitl::probe_platform`]) gate on that instead.

use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsFd, FromRawFd, IntoRawFd};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use nix::pty::{OpenptyResult, openpty};

use crate::process::{graceful_kill, set_new_process_group};
use crate::{Result, TestError};

/// Default Renode boot deadline. PX4 + NuttX start cold in 10–30 s
/// of virtual time on Renode; allow headroom.
const BOOT_TIMEOUT: Duration = Duration::from_secs(60);

/// PX4's pxh prompt. Matches POSIX SITL and NuttX builds verbatim.
const PXH_PROMPT: &str = "pxh>";

/// Shared between the pty drainer thread and the test thread.
#[derive(Default)]
struct LogBuf {
    text: Mutex<String>,
    notify: Condvar,
}

/// A live Renode child running PX4 firmware on emulated H7.
pub struct Px4RenodeSitl {
    child: Mutex<Child>,
    /// pty master file descriptor; we own it for the duration of the
    /// fixture and write subcommands to it.
    pty_master: Mutex<std::fs::File>,
    log: Arc<LogBuf>,
}

impl Px4RenodeSitl {
    /// Boot a Renode child against the configured firmware. Cold
    /// call may take 10–30 s of virtual time; subsequent calls in
    /// the same process pay the same cost (Renode itself is
    /// stateless across runs).
    pub fn boot() -> Result<Self> {
        let renode = renode_binary()?;
        let firmware = firmware_path()?;
        let resc = resc_path();

        // Allocate a pty. The slave path goes into the .resc as the
        // UART backing; the master fd stays here so we can drive
        // the shell. Resolve the slave path *before* consuming the
        // slave fd — `ttyname` borrows a `BorrowedFd`.
        let OpenptyResult { master, slave } = openpty(None, None).map_err(TestError::Nix)?;
        let slave_path = pty_path(slave.as_fd())?;
        // Slave fd can drop now; the path lives on its own. Renode
        // re-opens the slave by name from the .resc.
        drop(slave);

        // The .resc reads `$slave` and `$bin` as Renode-side variables
        // we set on the command line. Quoting matters — Renode's
        // monitor parser is whitespace-sensitive.
        let exec = format!(
            "$slave=\"{slave}\"; $bin=@{bin}; include @{resc}",
            slave = slave_path.display(),
            bin = firmware.display(),
            resc = resc.display(),
        );
        let mut child = spawn_renode(&renode, &exec)?;

        // Drain Renode's own stdout + stderr for diagnostics.
        let log = Arc::new(LogBuf::default());
        if let Some(out) = child.stdout.take() {
            spawn_drainer(out, Arc::clone(&log), "renode-stdout");
        }
        if let Some(err) = child.stderr.take() {
            spawn_drainer(err, Arc::clone(&log), "renode-stderr");
        }

        // Open the master end as a normal file. Spawn another
        // drainer that tails the firmware's UART output into the
        // same shared buffer the wait_for_log helpers consult.
        // SAFETY: master is an owned fd we keep alive on `self`.
        let master_file = unsafe { std::fs::File::from_raw_fd(master.into_raw_fd()) };
        let master_clone = master_file.try_clone()?;
        spawn_drainer(master_clone, Arc::clone(&log), "uart");

        let sitl = Self {
            child: Mutex::new(child),
            pty_master: Mutex::new(master_file),
            log,
        };

        sitl.wait_for_log("Startup script returned successfully", BOOT_TIMEOUT)
            .map_err(|e| match e {
                TestError::LogTimeout { .. } => TestError::BootTimeout {
                    timeout_secs: BOOT_TIMEOUT.as_secs(),
                },
                other => other,
            })?;

        Ok(sitl)
    }

    /// Run a shell command in the firmware's pxh shell. Writes
    /// `cmd\r\n` to the UART and reads back everything up to the
    /// next `pxh>` prompt.
    pub fn shell(&self, cmd: &str) -> Result<String> {
        let pre_len = self.log.text.lock().unwrap().len();
        {
            let mut master = self.pty_master.lock().unwrap();
            master.write_all(cmd.as_bytes())?;
            master.write_all(b"\r\n")?;
            master.flush()?;
        }
        // Wait until the next prompt appears beyond `pre_len`.
        self.wait_for_log_after(PXH_PROMPT, pre_len, Duration::from_secs(10))?;
        let text = self.log.text.lock().unwrap();
        let after = &text[pre_len..];
        Ok(after.to_string())
    }

    /// Block until `pattern` appears anywhere in the captured
    /// firmware log, or `timeout` elapses. Returns the matching
    /// line.
    pub fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<String> {
        self.wait_for_log_after(pattern, 0, timeout)
    }

    fn wait_for_log_after(&self, pattern: &str, from: usize, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut text = self.log.text.lock().unwrap();
        loop {
            if let Some(line) = find_line(&text[from..], pattern) {
                return Ok(line);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(TestError::LogTimeout {
                    pattern: pattern.into(),
                    timeout_secs: timeout.as_secs(),
                });
            }
            let (new_text, _) = self.log.notify.wait_timeout(text, deadline - now).unwrap();
            text = new_text;
        }
    }

    /// Block up to `timeout` waiting for the Renode child to exit.
    pub fn wait_for_exit(&self, timeout: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let mut child = self.child.lock().unwrap();
                if let Ok(Some(status)) = child.try_wait() {
                    return Some(status);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Snapshot the entire captured log. Useful for diagnostics on
    /// a failing test.
    pub fn log_snapshot(&self) -> String {
        self.log.text.lock().unwrap().clone()
    }
}

impl Drop for Px4RenodeSitl {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap();
        graceful_kill(&mut child, Duration::from_secs(3));
    }
}

/// Outcome of a [`probe_platform`] run.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// Renode's combined stdout + stderr.
    pub renode_log: String,
    /// Renode's exit status. `Some` iff Renode quit within the
    /// probe's timeout.
    pub status: Option<ExitStatus>,
}

/// Spawn Renode, load the platform `.repl`, and quit. Verifies
/// that Renode + the platform description parse cleanly without
/// requiring a firmware ELF.
///
/// The probe is non-interactive — no pty, no UART, no shell. It
/// returns once Renode exits, or after `timeout`. Useful as a
/// continuously-runnable smoke even before phase-13's firmware
/// build (work item 13.1) lands.
///
/// Gates on [`renode_binary_available`], not [`renode_available`].
pub fn probe_platform(timeout: Duration) -> Result<ProbeOutcome> {
    let renode = renode_binary()?;
    let repl = repl_path();

    // Load + immediately quit. Any parse error in the .repl falls
    // out of Renode as a non-zero exit + a stderr line we capture.
    let exec = format!(
        "mach create \"probe\"; machine LoadPlatformDescription @{repl}; quit",
        repl = repl.display(),
    );

    let mut child = spawn_renode(&renode, &exec)?;
    let log = Arc::new(LogBuf::default());
    if let Some(out) = child.stdout.take() {
        spawn_drainer(out, Arc::clone(&log), "renode-stdout");
    }
    if let Some(err) = child.stderr.take() {
        spawn_drainer(err, Arc::clone(&log), "renode-stderr");
    }

    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break Some(s);
        }
        if Instant::now() >= deadline {
            graceful_kill(&mut child, Duration::from_secs(2));
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let renode_log = log.text.lock().unwrap().clone();
    Ok(ProbeOutcome { renode_log, status })
}

/// Spawn `renode --console --plain --disable-xwt -e <exec>` with
/// stdout/stderr piped + its own process group. Shared by `boot()`
/// and `probe_platform()`.
fn spawn_renode(renode: &PathBuf, exec: &str) -> Result<Child> {
    let mut cmd = Command::new(renode);
    cmd.arg("--console")
        .arg("--plain")
        .arg("--disable-xwt")
        .arg("-e")
        .arg(exec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    set_new_process_group(&mut cmd);
    Ok(cmd.spawn()?)
}

/// `RENODE` and `PX4_RENODE_FIRMWARE` both point at existing files.
/// Required for [`Px4RenodeSitl::boot`].
pub fn renode_available() -> bool {
    renode_binary().is_ok() && firmware_path().is_ok()
}

/// Just `RENODE`. Sufficient for [`probe_platform`].
pub fn renode_binary_available() -> bool {
    renode_binary().is_ok()
}

fn renode_binary() -> Result<PathBuf> {
    let raw = std::env::var_os("RENODE").ok_or(TestError::NoRenode)?;
    let p = PathBuf::from(raw);
    if !p.is_file() {
        return Err(TestError::NoRenode);
    }
    Ok(p)
}

fn firmware_path() -> Result<PathBuf> {
    let raw = std::env::var_os("PX4_RENODE_FIRMWARE").ok_or(TestError::NoFirmware)?;
    let p = PathBuf::from(raw);
    if !p.is_file() {
        return Err(TestError::NoFirmware);
    }
    Ok(p)
}

/// Path to the bundled `.resc` script that wires Renode up against
/// the firmware. Lives alongside this crate.
fn resc_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("platforms").join("px4_renode_h743.resc")
}

/// Path to the bundled `.repl` platform description.
fn repl_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("platforms").join("px4_renode_h743.repl")
}

/// Resolve the slave-side device path of a pty fd.
fn pty_path(fd: std::os::fd::BorrowedFd<'_>) -> Result<PathBuf> {
    use nix::unistd::ttyname;
    let p = ttyname(fd).map_err(TestError::Nix)?;
    Ok(p)
}

fn find_line(buf: &str, pat: &str) -> Option<String> {
    buf.lines().find(|l| l.contains(pat)).map(str::to_string)
}

fn spawn_drainer<R: std::io::Read + Send + 'static>(
    reader: R,
    log: Arc<LogBuf>,
    tag: &'static str,
) {
    thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            let mut text = log.text.lock().unwrap();
            text.push('[');
            text.push_str(tag);
            text.push_str("] ");
            text.push_str(&line);
            text.push('\n');
            log.notify.notify_all();
        }
    });
}
