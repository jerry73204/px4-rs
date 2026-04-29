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

use std::fmt::Write as _;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
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

/// Shared between a drainer thread and the test thread.
#[derive(Default)]
struct LogBuf {
    text: Mutex<String>,
    notify: Condvar,
}

/// A live Renode child running PX4 firmware on emulated H7.
pub struct Px4RenodeSitl {
    child: Mutex<Child>,
    /// Renode monitor stdin. We drive USART3 RX through monitor
    /// `sysbus.usart3 WriteChar` commands rather than writing to the
    /// pty: once NuttX configures USART3 (sets FIFOEN etc., bits
    /// Renode marks RESERVED), the pty's master end gets HUP'd and
    /// further writes return EIO. Renode's monitor doesn't have that
    /// problem — `WriteChar` synthesises bytes directly into the
    /// emulated UART regardless of what the firmware's done to it.
    monitor_stdin: Mutex<ChildStdin>,
    /// Symlink Renode created. Cleaned up on Drop so successive
    /// tests don't collide.
    pty_path: PathBuf,
    /// Raw bytes from USART3 — what the firmware actually printed.
    /// Kept separate from `diag_log` because the three drainers
    /// (uart, renode-stdout, renode-stderr) share no synchronisation
    /// at byte granularity; merging them in one buffer slices each
    /// stream's output across the others' chunks, so `"uorb start"`
    /// from the UART would never appear contiguously.
    uart_log: Arc<LogBuf>,
    /// Renode's own stdout/stderr — useful for diagnostics on a
    /// failing test, never searched by `shell`.
    diag_log: Arc<LogBuf>,
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

        // Renode's own stdout/stderr go into `diag_log` for
        // diagnostics. The UART pty has its own buffer so shell
        // searches aren't confused by interleaved Renode warnings.
        let diag_log = Arc::new(LogBuf::default());
        if let Some(out) = child.stdout.take() {
            spawn_drainer(out, Arc::clone(&diag_log));
        }
        if let Some(err) = child.stderr.take() {
            spawn_drainer(err, Arc::clone(&diag_log));
        }
        let monitor_stdin = child
            .stdin
            .take()
            .expect("spawn_renode pipes stdin");

        // Renode needs a moment to create the symlink. Poll up to
        // ~5 s; this is a small fraction of the full boot budget
        // and the failure mode (missing symlink → fail open) is
        // immediate and clear.
        let symlink_deadline = Instant::now() + Duration::from_secs(5);
        while !slave_path.exists() {
            if Instant::now() >= symlink_deadline {
                graceful_kill(&mut child, Duration::from_secs(2));
                let snapshot = diag_log.text.lock().unwrap().clone();
                return Err(TestError::BootTimeout {
                    timeout_secs: 5,
                })
                .inspect_err(|_| {
                    eprintln!("Renode never created pty at {}; log:\n{snapshot}", slave_path.display());
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        // Open Renode's pty master read-write and tail it. Opening
        // read-only would let the pty drop into a hangup state on
        // some kernels; we never write to it (see `monitor_stdin`)
        // but keeping the writeable handle alive avoids the issue.
        let master_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&slave_path)?;
        let uart_log = Arc::new(LogBuf::default());
        spawn_drainer(master_file, Arc::clone(&uart_log));

        let sitl = Self {
            child: Mutex::new(child),
            monitor_stdin: Mutex::new(monitor_stdin),
            pty_path: slave_path,
            uart_log,
            diag_log,
        };

        sitl.wait_for_log(BOOT_BANNER, BOOT_TIMEOUT)
            .map_err(|e| match e {
                TestError::LogTimeout { .. } => TestError::BootTimeout {
                    timeout_secs: BOOT_TIMEOUT.as_secs(),
                },
                other => other,
            })?;

        // PX4 SITL auto-starts uorb via its `rcS` script; this board
        // has no ROMFSETC, so we run the equivalent by hand once the
        // shell is up. Tests that exercise uORB topics need it running
        // before they shell their own modules. Skip cleanly on
        // bare-NuttX firmware (no PX4 systemcmds): we look for the
        // `uorb` builtin in `help` first, and only start it if found.
        sitl.maybe_start_uorb()?;

        Ok(sitl)
    }

    /// Bring up the daemons that PX4's `rcS` would normally start.
    /// SITL gets these for free via the POSIX startup script; the
    /// renode-h743 board has no `ROMFSETC` so they're missing on
    /// boot. Skip cleanly on bare-NuttX firmware (no PX4 systemcmds).
    fn maybe_start_uorb(&self) -> Result<()> {
        let help = self.shell("help")?;
        if !help.contains("uorb") {
            return Ok(());
        }
        // Order matters: `work_queue` first (Rust modules expecting
        // `lp_default`/`hp_default` rely on the WorkQueueManager being
        // up), then `uorb`. Already-running calls are no-ops; either
        // outcome is fine.
        let _ = self.shell("work_queue start")?;
        let _ = self.shell("uorb start")?;
        Ok(())
    }

    /// Run a shell command in the firmware's `nsh` shell. Sends
    /// `cmd\r\n` to USART3 RX one byte at a time via the Renode
    /// monitor (see `monitor_stdin`), then reads back everything up
    /// to the next `nsh>` prompt.
    ///
    /// nsh prints a double prompt (`\r\nnsh> \x1b[K\r\nnsh> \x1b[K`)
    /// after every command — likely treats the trailing `\n` as a
    /// second empty command. To avoid the next call matching the
    /// stale second prompt, anchor on the command echo before
    /// scanning for the closing prompt.
    pub fn shell(&self, cmd: &str) -> Result<String> {
        // No quote/backslash/cr/lf in command; the monitor's WriteLine
        // takes a literal double-quoted string and any of those
        // characters would confuse its parser. Reject early so we
        // don't silently truncate.
        assert!(
            !cmd.contains(['\"', '\\', '\r', '\n']),
            "shell() cmd must not contain quotes, backslashes, or newlines: {cmd:?}"
        );
        let pre_len = self.uart_log.text.lock().unwrap().len();
        // Use `WriteLine` (one monitor command) instead of N
        // `WriteChar`s. The per-char form raced — by the time the
        // 12th `WriteChar` for `uorb status\r\n` had cleared the
        // monitor parser, earlier bytes had already been processed
        // and prompts emitted, occasionally leaving the second-prompt
        // race alive. WriteLine submits the whole line atomically
        // and only appends one `\r` (no trailing `\n`), eliminating
        // the empty-line second prompt entirely.
        let line = format!("sysbus.usart3 WriteLine \"{cmd}\"\n");
        {
            let mut stdin = self.monitor_stdin.lock().unwrap();
            stdin.write_all(line.as_bytes())?;
            stdin.flush()?;
        }
        // Wait for the firmware to echo the command back, then for
        // a prompt after that point. The echo guarantees we're
        // looking at output produced by *this* shell call rather
        // than a leftover prompt from the previous one.
        let echo_end = find_in_log(&self.uart_log, cmd, pre_len, Duration::from_secs(10))?
            + cmd.len();
        find_in_log(&self.uart_log, NSH_PROMPT, echo_end, Duration::from_secs(10))?;
        let text = self.uart_log.text.lock().unwrap();
        Ok(text[pre_len..].to_string())
    }

    /// Block until `pattern` appears anywhere in the firmware's
    /// UART output, or `timeout` elapses. Returns the surrounding
    /// line for context.
    pub fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<String> {
        let pos = find_in_log(&self.uart_log, pattern, 0, timeout)?;
        let text = self.uart_log.text.lock().unwrap();
        Ok(line_around(&text, pos, pattern))
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

    /// Snapshot the firmware's UART output. Useful for diagnostics
    /// on a failing test.
    pub fn log_snapshot(&self) -> String {
        self.uart_log.text.lock().unwrap().clone()
    }

    /// Snapshot Renode's own stdout/stderr — its monitor banner,
    /// peripheral warnings, and any errors it emits. Independent
    /// from [`Self::log_snapshot`] so test failures can include
    /// both halves.
    pub fn diag_snapshot(&self) -> String {
        self.diag_log.text.lock().unwrap().clone()
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
        spawn_drainer(out, Arc::clone(&log));
    }
    if let Some(err) = child.stderr.take() {
        spawn_drainer(err, Arc::clone(&log));
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
/// stdin/stdout/stderr piped + its own process group. Shared by
/// `boot()` and `probe_platform()`. Stdin is piped so `boot()` can
/// drive USART3 RX through the monitor; `probe_platform()` simply
/// drops the handle.
fn spawn_renode(renode: &PathBuf, exec: &str) -> Result<Child> {
    let mut cmd = Command::new(renode);
    cmd.arg("--console")
        .arg("--plain")
        .arg("--disable-xwt")
        .arg("-e")
        .arg(exec)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
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

/// Wait for `pat` to appear in `log` at or after byte offset `from`,
/// returning the byte index of the match. Times out with
/// `LogTimeout`.
fn find_in_log(log: &LogBuf, pat: &str, from: usize, timeout: Duration) -> Result<usize> {
    let deadline = Instant::now() + timeout;
    let mut text = log.text.lock().unwrap();
    loop {
        if let Some(rel) = text[from..].find(pat) {
            return Ok(from + rel);
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(TestError::LogTimeout {
                pattern: pat.into(),
                timeout_secs: timeout.as_secs(),
            });
        }
        let (new_text, _) = log.notify.wait_timeout(text, deadline - now).unwrap();
        text = new_text;
    }
}

/// Slice the line containing `pat` at byte offset `pos` in `buf`,
/// for diagnostic context in the return of [`Px4RenodeSitl::wait_for_log`].
fn line_around(buf: &str, pos: usize, pat: &str) -> String {
    let start = buf[..pos].rfind('\n').map(|n| n + 1).unwrap_or(0);
    let end_search_from = pos + pat.len();
    let end = buf[end_search_from..]
        .find('\n')
        .map(|n| end_search_from + n)
        .unwrap_or(buf.len());
    buf[start..end].to_string()
}

/// Tail a stream byte-by-byte into the shared log. A `BufReader::lines()`
/// drainer would buffer partial lines indefinitely — the nsh prompt
/// has no trailing newline, so any line-buffered drain would miss it.
fn spawn_drainer<R: std::io::Read + Send + 'static>(reader: R, log: Arc<LogBuf>) {
    thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    let mut text = log.text.lock().unwrap();
                    text.push_str(&chunk);
                    log.notify.notify_all();
                }
            }
        }
    });
}
