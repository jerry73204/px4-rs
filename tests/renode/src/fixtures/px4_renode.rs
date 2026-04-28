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
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{graceful_kill, set_new_process_group};
use crate::{Result, TestError};

/// Default Renode boot deadline. NuttX/PX4 cold-start in well under
/// a second of virtual time on Renode; the wall-clock budget is
/// dominated by Renode itself starting up. 30 s is generous.
const BOOT_TIMEOUT: Duration = Duration::from_secs(30);

/// Boot signature: NuttX prints this banner once the kernel hits
/// userspace. Works for both bare-NuttX (nsh) and PX4-on-NuttX
/// firmware. The POSIX SITL fixture uses a different banner
/// ("Startup script returned successfully") because POSIX SITL
/// runs PX4's pxh shell directly, not via NuttX.
const BOOT_BANNER: &str = "NuttShell";

/// Shell prompt for both bare-NuttX and PX4-on-NuttX. PX4's pxh on
/// POSIX uses `pxh>`; on NuttX targets PX4 leaves the stock NuttX
/// prompt in place.
const NSH_PROMPT: &str = "nsh>";

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
    /// Symlink Renode created. Cleaned up on Drop so successive
    /// tests don't collide.
    pty_path: PathBuf,
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
        let repl = repl_path();

        // Pick a unique pty-master symlink path Renode will create.
        // Renode's `CreateUartPtyTerminal` allocates its own pty pair
        // internally and symlinks the master end at this path —
        // it's an OUTPUT path, not a pre-existing one.
        let pid = std::process::id();
        let nonce = Instant::now().elapsed().subsec_nanos();
        let slave_path: PathBuf = format!("/tmp/renode-pty-{pid}-{nonce}").into();
        // Make sure no stale symlink lingers from a previous run.
        let _ = std::fs::remove_file(&slave_path);

        // The .resc reads `$slave`, `$bin`, and `$repl` as
        // Renode-side variables we set on the command line.
        // Quoting matters — Renode's monitor parser is
        // whitespace-sensitive. `@<abs path>` resolves to the
        // file directly without needing PATH config.
        let exec = format!(
            "$slave=\"{slave}\"; $bin=@{bin}; $repl=@{repl}; include @{resc}",
            slave = slave_path.display(),
            bin = firmware.display(),
            repl = repl.display(),
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

        // Renode needs a moment to create the symlink. Poll up to
        // ~5 s; this is a small fraction of the full boot budget
        // and the failure mode (missing symlink → fail open) is
        // immediate and clear.
        let symlink_deadline = Instant::now() + Duration::from_secs(5);
        while !slave_path.exists() {
            if Instant::now() >= symlink_deadline {
                graceful_kill(&mut child, Duration::from_secs(2));
                let snapshot = log.text.lock().unwrap().clone();
                return Err(TestError::BootTimeout {
                    timeout_secs: 5,
                })
                .inspect_err(|_| {
                    eprintln!("Renode never created pty at {}; log:\n{snapshot}", slave_path.display());
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // Open Renode's pty master and tail it.
        let master_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&slave_path)?;
        let master_clone = master_file.try_clone()?;
        spawn_drainer(master_clone, Arc::clone(&log), "uart");

        let sitl = Self {
            child: Mutex::new(child),
            pty_master: Mutex::new(master_file),
            pty_path: slave_path,
            log,
        };

        sitl.wait_for_log(BOOT_BANNER, BOOT_TIMEOUT)
            .map_err(|e| match e {
                TestError::LogTimeout { .. } => TestError::BootTimeout {
                    timeout_secs: BOOT_TIMEOUT.as_secs(),
                },
                other => other,
            })?;

        Ok(sitl)
    }

    /// Run a shell command in the firmware's `nsh`/`pxh` shell.
    /// Writes `cmd\r\n` to the UART and reads back everything up to
    /// the next `nsh>` prompt.
    pub fn shell(&self, cmd: &str) -> Result<String> {
        let pre_len = self.log.text.lock().unwrap().len();
        {
            let mut master = self.pty_master.lock().unwrap();
            master.write_all(cmd.as_bytes())?;
            master.write_all(b"\r\n")?;
            master.flush()?;
        }
        // Wait until the next prompt appears beyond `pre_len`.
        self.wait_for_log_after(NSH_PROMPT, pre_len, Duration::from_secs(10))?;
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
        {
            let mut child = self.child.lock().unwrap();
            graceful_kill(&mut child, Duration::from_secs(3));
        }
        // Renode normally cleans the pty symlink on shutdown, but
        // SIGKILL skips that path — sweep it ourselves.
        let _ = std::fs::remove_file(&self.pty_path);
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
