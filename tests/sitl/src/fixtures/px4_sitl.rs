//! `Px4Sitl` — RAII handle for a running PX4 SITL daemon.
//!
//! ```ignore
//! use px4_sitl_tests::Px4Sitl;
//! use std::time::Duration;
//!
//! let sitl = Px4Sitl::boot()?;
//! sitl.shell("uorb status")?;
//! // SITL is killed cleanly when `sitl` goes out of scope.
//! ```
//!
//! `boot()` ensures `bin/px4` exists (cached `make px4_sitl`),
//! spawns it in daemon mode (`px4 -d etc/init.d-posix/rcS`), and
//! waits for the `Startup script returned successfully` line on
//! stdout before returning.
//!
//! `Drop` SIGTERMs the daemon's process group, then SIGKILLs after a
//! 3-second grace period. Without this, an orphan daemon would hold
//! MAVLink UDP ports across the next test invocation.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{graceful_kill, set_new_process_group};
use crate::{Result, TestError};

use super::build;

/// Shared between the daemon's stdout-draining thread and the test
/// thread that's waiting for log lines.
#[derive(Default)]
struct LogBuf {
    text: Mutex<String>,
    notify: Condvar,
}

/// A live PX4 SITL daemon, plus enough plumbing to send it shell
/// commands and tail its log.
pub struct Px4Sitl {
    child: Mutex<Child>,
    build_dir: PathBuf,
    log: Arc<LogBuf>,
}

impl Px4Sitl {
    /// Boot a fresh daemon. Cold call triggers `make px4_sitl`;
    /// subsequent calls in the same process reuse the cached build.
    pub fn boot() -> Result<Self> {
        let build_dir = build::ensure_built()?;
        Self::boot_in(&build_dir)
    }

    /// Boot a daemon against an explicit build dir. Used by the
    /// `PX4_RS_SITL_BUILD_DIR` override path; tests normally use
    /// [`boot`](Self::boot).
    pub fn boot_in(build_dir: &PathBuf) -> Result<Self> {
        let bin = build_dir.join("bin").join("px4");
        let rc = PathBuf::from("etc").join("init.d-posix").join("rcS");

        let mut cmd = Command::new(&bin);
        cmd.arg("-d")
            .arg(&rc)
            .current_dir(build_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        set_new_process_group(&mut cmd);

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        let log = Arc::new(LogBuf::default());

        // Drain stdout into the shared buffer + drain stderr to
        // /dev/null so the daemon never blocks on a full pipe.
        spawn_drainer(stdout, Arc::clone(&log));
        spawn_drainer(stderr, Arc::clone(&log));

        let sitl = Self {
            child: Mutex::new(child),
            build_dir: build_dir.clone(),
            log,
        };

        sitl.wait_for_log("Startup script returned successfully", Duration::from_secs(15))
            .map_err(|e| match e {
                TestError::LogTimeout { .. } => TestError::BootTimeout { timeout_secs: 15 },
                other => other,
            })?;

        Ok(sitl)
    }

    /// Run a shell command against the running daemon. The first
    /// whitespace-delimited word becomes the `px4-<word>` binary
    /// (e.g. `"uorb status"` → `bin/px4-uorb status`). Returns
    /// captured stdout.
    pub fn shell(&self, cmd: &str) -> Result<String> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let modname = parts.first().copied().unwrap_or_default();
        if modname.is_empty() {
            return Err(TestError::SubprocessFailed {
                cmd: cmd.into(),
                status: -1,
            });
        }
        let bin = self
            .build_dir
            .join("bin")
            .join(format!("px4-{modname}"));
        let out = Command::new(&bin)
            .args(&parts[1..])
            .current_dir(&self.build_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if !out.status.success() {
            return Err(TestError::SubprocessFailed {
                cmd: cmd.into(),
                status: out.status.code().unwrap_or(-1),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Block until `pattern` appears in the daemon's combined stdout/
    /// stderr stream, or `timeout` elapses. Both substring matches
    /// and full lines count. Returns the matching line.
    ///
    /// A polished regex-based variant is work item 11.3; this
    /// substring version is enough for the boot signal.
    pub fn wait_for_log(&self, pattern: &str, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        let mut text = self.log.text.lock().unwrap();
        loop {
            if let Some(line) = find_line(&text, pattern) {
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

    /// Snapshot the entire log captured so far. Mainly useful for
    /// diagnostics when a test fails.
    pub fn log_snapshot(&self) -> String {
        self.log.text.lock().unwrap().clone()
    }

    /// Path to the `bin/` directory inside the SITL build. Useful
    /// when a test wants to invoke a binary not covered by `shell()`.
    pub fn bin_dir(&self) -> PathBuf {
        self.build_dir.join("bin")
    }

    /// Block up to `timeout` waiting for the daemon to exit on its
    /// own. Returns `Some(status)` if it did, `None` on timeout.
    /// Used by the panic test to confirm a panic actually aborts the
    /// process — every other test relies on `Drop` to kill it.
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
}

impl Drop for Px4Sitl {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap();
        graceful_kill(&mut child, Duration::from_secs(3));
    }
}

fn find_line(buf: &str, pat: &str) -> Option<String> {
    buf.lines().find(|l| l.contains(pat)).map(str::to_string)
}

fn spawn_drainer<R: std::io::Read + Send + 'static>(reader: R, log: Arc<LogBuf>) {
    thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            let mut text = log.text.lock().unwrap();
            text.push_str(&line);
            text.push('\n');
            log.notify.notify_all();
        }
    });
}
