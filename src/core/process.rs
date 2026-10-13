use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

pub struct CoreProcessManager {
    child: Option<Child>,
    pub binary_path: PathBuf,
}

impl CoreProcessManager {
    pub fn find_xray_binary() -> Result<PathBuf, String> {
        let candidates = ["/usr/bin/xray", "/usr/local/bin/xray", "/opt/xray/xray"];

        for path in &candidates {
            let p = PathBuf::from(path);
            if p.exists() {
                return Ok(p);
            }
        }

        if let Ok(path) = which::which("xray") {
            return Ok(path);
        }

        Err("xray binary not found. Please install xray or verify PATH.".into())
    }

    pub fn start(config_path: &Path) -> Result<Self, String> {
        let binary_path = Self::find_xray_binary()?;

        let child = Command::new(&binary_path)
            .arg("run")
            .arg("-c")
            .arg(config_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn xray process: {}", e))?;

        Ok(Self {
            child: Some(child),
            binary_path,
        })
    }

    /// Waits until the core reports that it started, or until `timeout`.
    ///
    /// The core prints a `started` line once its listeners are accepting. That
    /// line is the only reliable readiness signal: without waiting for it, a
    /// core that exits a moment later (bad config, missing geo assets, PORT in
    /// use) would be reported as a working connection. A process that exits
    /// during the wait is reported as an error carrying its own output, which
    /// is the useful diagnostic.
    pub fn wait_until_ready(&mut self, timeout: Duration) -> Result<(), String> {
        let child = self.child.as_mut().ok_or("process is not running")?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let (tx, rx) = mpsc::channel::<String>();

        // Drain both pipes on their own threads. The core is chatty, and an
        // unread pipe would eventually block it once the buffer fills.
        for stream in [
            stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
            stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        ] {
            let Some(stream) = stream else { continue };
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        drop(tx);

        let deadline = Instant::now() + timeout;
        let mut captured: Vec<String> = Vec::new();
        let mut ready = false;

        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    // The core logs a line containing "started" when ready.
                    if line.contains("started") && !line.contains("Failed") {
                        ready = true;
                        captured.push(line);
                        break;
                    }
                    // A failure line is definitive; no point waiting further.
                    if line.contains("Failed to start") || line.contains("failed to start") {
                        captured.push(line);
                        break;
                    }
                    captured.push(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // The child may have exited without saying anything.
                    if let Some(child) = self.child.as_mut() {
                        if let Ok(Some(status)) = child.try_wait() {
                            return Err(Self::describe_exit(&captured, Some(status)));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if ready {
            return Ok(());
        }

        // Not ready: either it exited, or it is silent. Both are failures here.
        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(Self::describe_exit(&captured, Some(status)));
            }
        }

        Err(Self::describe_exit(&captured, None))
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

            // Send SIGTERM using libc
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }

            // Wait up to 2 seconds for clean exit
            for _ in 0..20 {
                match child.try_wait() {
                    Ok(Some(_status)) => return Ok(()),
                    Ok(None) => thread::sleep(Duration::from_millis(100)),
                    Err(e) => return Err(format!("Error waiting for process: {}", e)),
                }
            }

            // Force kill if still running
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(None) => true,
                _ => false,
            }
        } else {
            false
        }
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
