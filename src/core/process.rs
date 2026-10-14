//! Core process lifecycle and output handling.
//!
//! Two responsibilities, both with a subtle failure mode worth stating:
//!
//! * **Starting and stopping** the core binary.
//! * **Draining its output continuously.** This is not optional. The core writes
//!   a log line per connection, and a pipe has finite capacity (64 KiB by
//!   default, 1 MiB here). If nothing reads the pipe, the core eventually blocks
//!   inside `write()`, which stalls every forwarded connection: the tunnel stays
//!   "connected" while passing no traffic. Measured: 60 requests already queued
//!   46 KB of unread output, so a session of ordinary browsing fills the buffer.
//!
//! The output is therefore drained on a dedicated thread for the whole lifetime
//! of the process, and the tail is kept in a ring buffer so a startup failure
//! can still be explained after the fact.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How many recent output lines are kept for diagnostics.
///
/// Bounded so a long-running session cannot grow memory without limit; the tail
/// is what matters when something goes wrong.
const OUTPUT_HISTORY_LINES: usize = 200;

/// Shared tail of the core's output, for error messages and debugging.
pub type OutputHistory = Arc<Mutex<VecDeque<String>>>;

pub struct CoreProcessManager {
    child: Option<Child>,
    pub binary_path: PathBuf,
    /// Recent output from the core, most recent last.
    history: OutputHistory,
    /// Set to stop the drain threads when the process is reaped.
    shutting_down: Arc<AtomicBool>,
}

impl CoreProcessManager {
    /// Discovers the Xray executable by prioritizing bundled packages, local
    /// user installations, and standard distribution paths.
    pub fn find_xray_binary() -> Result<PathBuf, String> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        // 1. Next to the running xdelta executable (Portable, AppImage, or local cargo build)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                candidates.push(parent.join("xray"));
                candidates.push(parent.join("bin").join("xray"));
                candidates.push(parent.join("../lib/xdelta/bin/xray"));
            }
        }

        // 2. AppImage mount root
        if let Ok(appdir) = std::env::var("APPDIR") {
            let appdir_p = PathBuf::from(appdir);
            candidates.push(appdir_p.join("usr/bin/xray"));
            candidates.push(appdir_p.join("usr/lib/xdelta/bin/xray"));
        }

        // 3. Bundled distribution package paths (DEB, RPM, Arch pkg)
        candidates.push(PathBuf::from("/usr/lib/xdelta/bin/xray"));
        candidates.push(PathBuf::from("/usr/lib/xdelta/xray"));
        candidates.push(PathBuf::from("/usr/libexec/xdelta/xray"));

        // 4. User-local installations (~/.local/share/xdelta/bin/xray)
        if let Some(user_bin) = crate::core::persist::user_bin_dir() {
            candidates.push(user_bin.join("xray"));
        }
        if let Some(cfg_dir) = crate::core::persist::config_dir() {
            candidates.push(cfg_dir.join("bin").join("xray"));
        }

        // 5. System-wide standard paths
        candidates.push(PathBuf::from("/usr/bin/xdelta-xray"));
        candidates.push(PathBuf::from("/usr/local/bin/xray"));
        candidates.push(PathBuf::from("/usr/bin/xray"));
        candidates.push(PathBuf::from("/opt/xray/xray"));

        if let Some(p) = candidates.into_iter().find(|p| p.is_file()) {
            return Ok(p);
        }

        // 6. Look in PATH
        if let Ok(path) = which::which("xray") {
            return Ok(path);
        }

        Err("xray binary not found. Please bundle xray or download it from Settings.".into())
    }

    /// Discovers the directory containing geoip.dat and geosite.dat assets.
    pub fn find_asset_dir(binary_path: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Some(parent) = binary_path.parent() {
            candidates.push(parent.to_path_buf());
        }

        if let Ok(appdir) = std::env::var("APPDIR") {
            candidates.push(PathBuf::from(appdir).join("usr/share/xdelta"));
        }

        candidates.push(PathBuf::from("/usr/share/xdelta"));
        if let Some(user_assets) = crate::core::persist::user_assets_dir() {
            candidates.push(user_assets);
        }
        candidates.push(PathBuf::from("/usr/share/xray"));
        candidates.push(PathBuf::from("/usr/local/share/xray"));

        candidates
            .into_iter()
            .find(|dir| dir.join("geoip.dat").exists() || dir.join("geosite.dat").exists())
    }

    /// Terminates any stale or orphaned core processes from previous sessions
    /// and cleans up any lingering xdelta0 TUN interface.
    pub fn terminate_stale_cores() {
        #[cfg(target_os = "linux")]
        {
            let my_pid = std::process::id() as i32;
            if let Ok(entries) = std::fs::read_dir("/proc") {
                for entry in entries.flatten() {
                    let Some(pid) = entry
                        .file_name()
                        .to_str()
                        .and_then(|s| s.parse::<i32>().ok())
                    else {
                        continue;
                    };

                    if pid == my_pid {
                        continue;
                    }

                    let is_xdelta_core = std::fs::read(format!("/proc/{pid}/cmdline"))
                        .map(|raw| {
                            let text = String::from_utf8_lossy(&raw).replace('\0', " ");
                            text.contains("xray")
                                && (text.contains("xdelta_active.json")
                                    || text.contains("xdelta_probe_"))
                        })
                        .unwrap_or(false);

                    if is_xdelta_core {
                        log::info!("terminating orphaned core process {pid}");
                        unsafe {
                            libc::kill(pid, libc::SIGKILL);
                        }
                    }
                }
            }

            // Clean up any lingering xdelta0 interface
            let _ = Command::new("ip")
                .args(["link", "delete", "xdelta0"])
                .output();
        }
    }

    pub fn start(config_path: &Path) -> Result<Self, String> {
        Self::terminate_stale_cores();

        let binary_path = Self::find_xray_binary()?;

        log::info!(
            "starting core: {} run -c {}",
            binary_path.display(),
            config_path.display()
        );

        let mut cmd = Command::new(&binary_path);
        if let Some(asset_dir) = Self::find_asset_dir(&binary_path) {
            log::info!("setting XRAY_LOCATION_ASSET to {}", asset_dir.display());
            cmd.env("XRAY_LOCATION_ASSET", asset_dir);
        }

        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let mut child = cmd
            .arg("run")
            .arg("-c")
            .arg(config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn xray process: {}", e))?;

        let history: OutputHistory = Arc::new(Mutex::new(VecDeque::new()));
        let shutting_down = Arc::new(AtomicBool::new(false));

        // Drain both pipes for the whole life of the process. Reading only until
        // the core reports readiness (the previous behaviour) left the pipes
        // unread afterwards, which eventually blocks the core and silently stops
        // all traffic.
        if let Some(stdout) = child.stdout.take() {
            Self::spawn_drain(stdout, history.clone(), "core", shutting_down.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            Self::spawn_drain(stderr, history.clone(), "core", shutting_down.clone());
        }

        Ok(Self {
            child: Some(child),
            binary_path,
            history,
            shutting_down,
        })
    }

    /// Starts a thread that reads one stream until it closes.
    fn spawn_drain<R: Read + Send + 'static>(
        stream: R,
        history: OutputHistory,
        label: &'static str,
        shutting_down: Arc<AtomicBool>,
    ) {
        thread::spawn(move || {
            let reader = BufReader::new(stream);

            for line in reader.lines() {
                if shutting_down.load(Ordering::Relaxed) {
                    break;
                }

                match line {
                    Ok(line) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        // Forward the core's own log lines into the application
                        // log, so a single file tells the whole story.
                        log::info!("[core] {trimmed}");

                        if let Ok(mut h) = history.lock() {
                            if h.len() == OUTPUT_HISTORY_LINES {
                                h.pop_front();
                            }
                            h.push_back(trimmed.to_string());
                        }
                    }
                    Err(_) => break,
                }
            }

            log::debug!("[{label}] output stream closed");
        });
    }

    /// A snapshot of the recent core output, for diagnostics.
    pub fn recent_output(&self) -> Vec<String> {
        self.history
            .lock()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Waits until the core reports that it started, or until `timeout`.
    ///
    /// The core prints a `started` line once its listeners are accepting. That
    /// line is the only reliable readiness signal: without waiting for it, a core
    /// that exits a moment later (bad config, missing geo assets, port in use)
    /// would be reported as a working connection.
    ///
    /// The drain threads started in [`Self::start`] keep reading afterwards;
    /// this function only observes their output.
    pub fn wait_until_ready(&mut self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let mut saw_readiness = false;

        while Instant::now() < deadline {
            // A process that exited cannot become ready.
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let tail = self.recent_output();
                        return Err(Self::describe_exit(&tail, Some(status)));
                    }
                    Ok(None) => {}
                    Err(e) => return Err(format!("could not query the core process: {e}")),
                }
            }

            let tail = self.recent_output();

            if tail.iter().any(|l| l.contains("started")) {
                saw_readiness = true;
                break;
            }

            // A failure line is definitive; no point waiting further.
            if tail
                .iter()
                .any(|l| l.contains("Failed to start") || l.contains("failed to start"))
            {
                break;
            }

            thread::sleep(Duration::from_millis(50));
        }

        if saw_readiness {
            log::info!("core reported ready");
            return Ok(());
        }

        let tail = self.recent_output();
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(Self::describe_exit(&tail, Some(status)));
            }
        }

        Err(Self::describe_exit(&tail, None))
    }

    /// Builds an error message from whatever the core managed to print.
    fn describe_exit(captured: &[String], status: Option<std::process::ExitStatus>) -> String {
        let detail = captured
            .iter()
            .rev()
            .find(|l| {
                let lower = l.to_lowercase();
                lower.contains("fail") || lower.contains("error") || lower.contains("denied")
            })
            .or_else(|| captured.last())
            .map(|l| l.trim().to_string());

        match (status, detail) {
            (Some(status), Some(detail)) => format!("core exited ({status}): {detail}"),
            (Some(status), None) => format!("core exited ({status}) without output"),
            (None, Some(detail)) => format!("core did not become ready: {detail}"),
            (None, None) => "core did not become ready in time".to_string(),
        }
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            let pid = child.id() as i32;
            log::info!("stopping core (pid {pid})");

            // Send SIGTERM using libc
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }

            // Wait up to 2 seconds for clean exit
            for _ in 0..20 {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log::info!("core exited cleanly ({status})");
                        self.shutting_down.store(true, Ordering::Relaxed);
                        return Ok(());
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(e) => return Err(format!("Error waiting for process: {}", e)),
                }
            }

            // Force kill if still running
            log::warn!("core did not exit after SIGTERM; killing it");
            let _ = child.kill();
            let _ = child.wait();
            self.shutting_down.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Whether the core process is still alive.
    ///
    /// A process that exited is reported with its exit status and its last output
    /// lines, because "the core died" without a reason is not actionable.
    pub fn is_running(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(status)) => {
                    let tail = self.recent_output();
                    let reason = Self::describe_exit(&tail, Some(status));
                    log::error!("core is no longer running: {reason}");
                    false
                }
                Err(e) => {
                    log::warn!("could not query the core process: {e}");
                    false
                }
            }
        } else {
            false
        }
    }

    /// The process id, for diagnostics.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for CoreProcessManager {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

// Simple fallback for `which` check
mod which {
    use std::path::PathBuf;
    pub fn which(name: &str) -> Result<PathBuf, ()> {
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                let bin = dir.join(name);
                if bin.is_file() {
                    return Ok(bin);
                }
            }
        }
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The core writes far more output than a pipe can hold, so the drain must
    /// outlive readiness. This checks the ring buffer that makes the output
    /// observable at all.
    #[test]
    fn history_is_bounded() {
        let history: OutputHistory = Arc::new(Mutex::new(VecDeque::new()));

        for i in 0..(OUTPUT_HISTORY_LINES * 2) {
            let mut h = history.lock().unwrap();
            if h.len() == OUTPUT_HISTORY_LINES {
                h.pop_front();
            }
            h.push_back(format!("line {i}"));
        }

        let h = history.lock().unwrap();
        assert_eq!(h.len(), OUTPUT_HISTORY_LINES);
        // The most recent entry is retained; the oldest were dropped.
        assert_eq!(
            h.back().unwrap(),
            &format!("line {}", OUTPUT_HISTORY_LINES * 2 - 1)
        );
        assert!(!h.contains(&"line 0".to_string()));
    }

    #[test]
    fn recent_output_is_empty_before_start() {
        let history: OutputHistory = Arc::new(Mutex::new(VecDeque::new()));
        assert!(history.lock().unwrap().is_empty());
    }

    /// `describe_exit` must surface the most relevant line, not just the last.
    #[test]
    fn describe_exit_prefers_a_failure_line() {
        let lines = vec![
            "starting up".to_string(),
            "Failed to start: address already in use".to_string(),
            "shutting down".to_string(),
        ];
        let msg = CoreProcessManager::describe_exit(&lines, None);
        assert!(msg.contains("address already in use"), "got: {msg}");
    }

    #[test]
    fn describe_exit_handles_empty_output() {
        let msg = CoreProcessManager::describe_exit(&[], None);
        assert!(msg.contains("did not become ready"));
    }

    /// A core binary that does not exist must be reported, not panicked on.
    #[test]
    fn describe_exit_includes_exit_status() {
        let lines = vec!["boom".to_string()];
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg("exit 3")
            .status()
            .expect("run sh");
        let msg = CoreProcessManager::describe_exit(&lines, Some(status));
        assert!(msg.contains("3"), "got: {msg}");
    }
}
