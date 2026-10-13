use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

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
