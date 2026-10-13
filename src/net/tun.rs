//! TUN support: privilege detection and the one-time setup that grants the core
//! binary the capabilities it needs.
//!
//! Creating a TUN device is a privileged kernel operation. There are two ways
//! to satisfy that on Linux:
//!
//! 1. Run the core as root (for example through `pkexec`). This is what many
//!    clients do, but it breaks the lifecycle: a process started as root cannot
//!    be signalled or reaped by an unprivileged GUI, so "disconnect" would have
//!    to shell out to yet another privileged command and a crash in the GUI
//!    would leave a root process behind holding the tunnel open.
//! 2. Grant the binary `cap_net_admin,cap_net_raw` once, then run it as the
//!    normal user. The GUI keeps full control of the process, and teardown is
//!    an ordinary `SIGTERM`.
//!
//! This module implements (2) and only falls back to reporting the problem when
//! the capability is absent, so the user is never left with a silent failure.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path of the TUN character device.
const TUN_DEVICE: &str = "/dev/net/tun";

/// Capabilities the core needs to create and configure a TUN device.
const REQUIRED_CAPABILITIES: &str = "cap_net_admin,cap_net_raw+ep";

/// Why TUN mode is not usable, in a form the UI can explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunReadiness {
    /// TUN can be used right now.
    Ready,
    /// The device node does not exist, so the kernel has no TUN support.
    Unsupported(String),
    /// The binary exists but lacks the capabilities; setup is required.
    NeedsPrivileges { binary: PathBuf },
}

impl TunReadiness {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Whether the user can act on this by running the setup step.
    pub fn is_fixable_by_setup(&self) -> bool {
        matches!(self, Self::NeedsPrivileges { .. })
    }

    /// Short explanation shown next to the toggle.
    pub fn message(&self) -> String {
        match self {
            Self::Ready => "Ready".to_string(),
            Self::Unsupported(reason) => format!("Unavailable: {reason}"),
            Self::NeedsPrivileges { binary } => format!(
                "One-time setup needed: grant {} permission to create the tunnel device",
                binary.display()
            ),
        }
    }
}

/// Checks whether the kernel exposes the TUN device.
pub fn kernel_supports_tun() -> Result<(), String> {
    let path = Path::new(TUN_DEVICE);
    if path.exists() {
        Ok(())
    } else {
        Err(format!(
            "{TUN_DEVICE} is missing (the 'tun' kernel module may not be loaded)"
        ))
    }
}

/// Reads the capability set of a binary via `getcap`.
///
/// Returns an empty string when the binary carries no capabilities; `getcap`
/// prints nothing in that case and exits successfully.
pub fn read_capabilities(binary: &Path) -> Result<String, String> {
    let output = Command::new("getcap")
        .arg(binary)
        .output()
        .map_err(|e| format!("could not run getcap: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "getcap failed for {}: {}",
            binary.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Returns true when the reported capability set includes what TUN needs.
pub fn has_required_capabilities(reported: &str) -> bool {
    reported.contains("cap_net_admin") && reported.contains("cap_net_raw")
}

/// Determines whether TUN mode can be used with the given core binary.
pub fn check_readiness(binary: &Path) -> TunReadiness {
    if let Err(reason) = kernel_supports_tun() {
        return TunReadiness::Unsupported(reason);
    }

    if !binary.exists() {
        return TunReadiness::Unsupported(format!("{} not found", binary.display()));
    }

    match read_capabilities(binary) {
        Ok(caps) if has_required_capabilities(&caps) => TunReadiness::Ready,
        Ok(_) => TunReadiness::NeedsPrivileges {
            binary: binary.to_path_buf(),
        },
        Err(e) => TunReadiness::Unsupported(e),
    }
}

/// The command that grants the core its capabilities.
///
/// Exposed so the UI can show the user exactly what will run, and so the
/// invocation stays in one place.
pub fn setup_command(binary: &Path) -> Vec<String> {
    vec![
        "pkexec".to_string(),
        "setcap".to_string(),
        REQUIRED_CAPABILITIES.to_string(),
        binary.display().to_string(),
    ]
}

/// The same setup, as a copy-pasteable shell line.
pub fn setup_command_line(binary: &Path) -> String {
    format!("sudo setcap {} {}", REQUIRED_CAPABILITIES, binary.display())
}

/// Runs the capability setup through polkit.
///
/// This is the only privileged action the application performs, it happens once,
/// and the user sees a desktop authentication prompt. It is never called
/// implicitly: the UI requires an explicit click.
pub fn run_setup(binary: &Path) -> Result<(), String> {
    let argv = setup_command(binary);
    let (program, args) = argv.split_first().ok_or("empty setup command")?;

    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("could not run {}: {}", program, e))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else {
        stderr.trim().to_string()
    };

    if detail.is_empty() {
        Err("setup was cancelled or was not permitted".to_string())
    } else {
        Err(detail)
    }
}

/// Removes the capability grant, restoring the binary to its original state.
pub fn revert_setup(binary: &Path) -> Result<(), String> {
    let output = Command::new("pkexec")
        .args(["setcap", "-r", &binary.display().to_string()])
        .output()
        .map_err(|e| format!("could not run setcap -r: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_detection_requires_both_caps() {
        assert!(has_required_capabilities(
            "/usr/bin/xray cap_net_admin,cap_net_raw+ep"
        ));
        assert!(has_required_capabilities("cap_net_raw,cap_net_admin=eip"));
        assert!(!has_required_capabilities("cap_net_admin+ep"));
        assert!(!has_required_capabilities("cap_net_raw+ep"));
        assert!(!has_required_capabilities(""));
    }

    /// `getcap` prints nothing for a binary without capabilities, which is the
    /// common case; that must not be mistaken for success.
    #[test]
    fn empty_capability_output_is_not_ready() {
        assert!(!has_required_capabilities(""));
    }

    #[test]
    fn setup_command_uses_polkit_and_setcap() {
        let cmd = setup_command(Path::new("/usr/bin/xray"));
        assert_eq!(cmd[0], "pkexec");
        assert_eq!(cmd[1], "setcap");
        assert!(cmd[2].contains("cap_net_admin"));
        assert_eq!(cmd[3], "/usr/bin/xray");
    }

    #[test]
    fn setup_command_line_is_copy_pasteable() {
        let line = setup_command_line(Path::new("/usr/bin/xray"));
        assert!(line.starts_with("sudo setcap"));
        assert!(line.contains("cap_net_admin,cap_net_raw+ep"));
        assert!(line.ends_with("/usr/bin/xray"));
    }

    #[test]
    fn readiness_reports_needs_privileges_when_caps_absent() {
        let readiness = TunReadiness::NeedsPrivileges {
            binary: PathBuf::from("/usr/bin/xray"),
        };
        assert!(!readiness.is_ready());
        assert!(readiness.is_fixable_by_setup());
        assert!(readiness.message().contains("/usr/bin/xray"));
    }

    /// An unsupported kernel must not be presented as a fixable problem, or the
    /// user would be told to authenticate for something that cannot work.
    #[test]
    fn unsupported_is_not_fixable_by_setup() {
        let readiness = TunReadiness::Unsupported("no /dev/net/tun".into());
        assert!(!readiness.is_ready());
        assert!(!readiness.is_fixable_by_setup());
        assert!(readiness.message().contains("no /dev/net/tun"));
    }

    #[test]
    fn ready_state_reports_ready() {
        assert!(TunReadiness::Ready.is_ready());
        assert_eq!(TunReadiness::Ready.message(), "Ready");
    }

    /// Any binary path must survive the round trip into the command line, since
    /// the argument order matters to `setcap`.
    #[test]
    fn setup_command_preserves_binary_path() {
        for path in ["/usr/bin/xray", "/opt/custom/xray-core"] {
            let cmd = setup_command(Path::new(path));
            assert_eq!(cmd.last().unwrap(), path);
        }
    }
}
