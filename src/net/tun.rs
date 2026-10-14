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

/// Standard persistent helper path for TUN route configuration.
pub const HELPER_PATH: &str = "/usr/local/bin/xdelta-tun-helper";
/// Polkit policy action allowing active desktop sessions to configure routes without password prompts.
pub const POLKIT_POLICY_PATH: &str =
    "/usr/share/polkit-1/actions/io.github.xdelta.tun-helper.policy";
/// Sudoers drop-in allowing passwordless execution of the helper.
pub const SUDOERS_PATH: &str = "/etc/sudoers.d/xdelta-tun";

/// Why TUN mode is not usable, in a form the UI can explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunReadiness {
    /// TUN can be used right now.
    Ready,
    /// The device node does not exist, so the kernel has no TUN support.
    Unsupported(String),
    /// The binary exists but lacks the capabilities or routing helper; setup is required.
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
                "One-time setup needed: grant permissions for {} and routing helper",
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

/// Returns true when the persistent TUN helper and permissions are installed and ready.
pub fn is_helper_ready() -> bool {
    let helper = Path::new(HELPER_PATH);
    if !helper.exists() {
        return false;
    }
    // Check if sudo -n test succeeds
    if let Ok(out) = Command::new("sudo")
        .args(["-n", HELPER_PATH, "test"])
        .output()
    {
        if out.status.success() {
            return true;
        }
    }
    // Check if polkit policy exists
    if Path::new(POLKIT_POLICY_PATH).exists() {
        return true;
    }
    false
}

/// Determines whether TUN mode can be used with the given core binary.
pub fn check_readiness(binary: &Path) -> TunReadiness {
    if let Err(reason) = kernel_supports_tun() {
        return TunReadiness::Unsupported(reason);
    }

    if !binary.exists() {
        return TunReadiness::Unsupported(format!("{} not found", binary.display()));
    }

    let caps_ok = match read_capabilities(binary) {
        Ok(caps) => has_required_capabilities(&caps),
        Err(_) => false,
    };

    let helper_ok = is_helper_ready();

    if caps_ok && helper_ok {
        TunReadiness::Ready
    } else {
        TunReadiness::NeedsPrivileges {
            binary: binary.to_path_buf(),
        }
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

/// Runs the one-time capability and routing helper setup through polkit.
///
/// This asks the user for authentication once. After this completes,
/// route setup and teardown are completely passwordless and never prompt again.
pub fn run_setup(binary: &Path) -> Result<(), String> {
    let script_content = format!(
        r#"#!/bin/bash
set -e
XRAY_BIN="{bin_path}"

# 1. Grant Linux capabilities to the Xray binary
if [ -n "$XRAY_BIN" ] && [ -f "$XRAY_BIN" ]; then
    setcap cap_net_admin,cap_net_raw+ep "$XRAY_BIN"
fi

# 2. Install the persistent TUN routing helper
mkdir -p /usr/local/bin
cat << 'HELPER_EOF' > /usr/local/bin/xdelta-tun-helper
#!/bin/bash
set -e

ACTION="$1"
IFACE="${{2:-xdelta0}}"
SERVER_HOST="$3"

case "$ACTION" in
    test)
        exit 0
        ;;
    up)
        for i in {{1..30}}; do
            if ip link show dev "$IFACE" >/dev/null 2>&1; then
                break
            fi
            sleep 0.1
        done

        ip addr add 172.19.0.1/30 dev "$IFACE" 2>/dev/null || true
        ip link set dev "$IFACE" up 2>/dev/null || true

        DEFAULT_ROUTE=$(ip -4 route show default 2>/dev/null | head -1)
        GW=$(echo "$DEFAULT_ROUTE" | awk '{{for(i=1;i<=NF;i++) if($i=="via") print $(i+1)}}')
        DEV=$(echo "$DEFAULT_ROUTE" | awk '{{for(i=1;i<=NF;i++) if($i=="dev") print $(i+1)}}')

        SERVER_IP=""
        if [ -n "$SERVER_HOST" ]; then
            if [[ "$SERVER_HOST" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                SERVER_IP="$SERVER_HOST"
            else
                SERVER_IP=$(getent ahosts "$SERVER_HOST" 2>/dev/null | awk '{{print $1}}' | head -1 || true)
            fi
        fi

        if [ -n "$SERVER_IP" ] && [ -n "$GW" ] && [ -n "$DEV" ]; then
            ip route replace "$SERVER_IP/32" via "$GW" dev "$DEV" 2>/dev/null || true
        fi

        ip route replace 0.0.0.0/1 dev "$IFACE" 2>/dev/null || true
        ip route replace 128.0.0.0/1 dev "$IFACE" 2>/dev/null || true

        if command -v resolvectl >/dev/null 2>&1; then
            resolvectl dns "$IFACE" 1.1.1.1 8.8.8.8 2>/dev/null || true
            resolvectl default-route "$IFACE" true 2>/dev/null || true
        fi
        ;;

    down)
        ip route del 0.0.0.0/1 dev "$IFACE" 2>/dev/null || true
        ip route del 128.0.0.0/1 dev "$IFACE" 2>/dev/null || true
        if command -v resolvectl >/dev/null 2>&1; then
            resolvectl revert "$IFACE" 2>/dev/null || true
        fi
        ip link delete "$IFACE" 2>/dev/null || true
        ;;

    *)
        echo "Usage: $0 {{test|up|down}} [interface] [server_host]" >&2
        exit 1
        ;;
esac
HELPER_EOF

chmod 755 /usr/local/bin/xdelta-tun-helper

# 3. Allow passwordless execution via Sudoers drop-in
mkdir -p /etc/sudoers.d
cat << 'SUDO_EOF' > /etc/sudoers.d/xdelta-tun
ALL ALL=(ALL) NOPASSWD: /usr/local/bin/xdelta-tun-helper
SUDO_EOF
chmod 440 /etc/sudoers.d/xdelta-tun

# 4. Allow passwordless execution via PolicyKit
mkdir -p /usr/share/polkit-1/actions
cat << 'POLKIT_EOF' > /usr/share/polkit-1/actions/io.github.xdelta.tun-helper.policy
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD polkit Policy Configuration 1.0//EN"
"http://www.freedesktop.org/software/polkit/policyconfig-1.dtd">
<policyconfig>
  <vendor>xdelta</vendor>
  <vendor_url>https://github.com/TssHack/xdelta</vendor_url>
  <action id="io.github.xdelta.tun-helper">
    <description>Run xdelta TUN routing helper</description>
    <message>Authentication is required to configure xdelta network routing</message>
    <defaults>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
      <allow_active>yes</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/usr/local/bin/xdelta-tun-helper</annotate>
  </action>
</policyconfig>
POLKIT_EOF
chmod 644 /usr/share/polkit-1/actions/io.github.xdelta.tun-helper.policy
"#,
        bin_path = binary.display()
    );

    let setup_script_path = std::env::temp_dir().join("xdelta_setup_tun.sh");
    std::fs::write(&setup_script_path, script_content)
        .map_err(|e| format!("Failed to create setup script: {e}"))?;

    let output = Command::new("pkexec")
        .args(["bash", setup_script_path.to_str().unwrap_or("")])
        .output()
        .map_err(|e| format!("could not run pkexec: {e}"))?;

    let _ = std::fs::remove_file(setup_script_path);

    if output.status.success() {
        Ok(())
    } else {
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
}

/// Removes the capability grant and helper files, restoring the system.
pub fn revert_setup(binary: &Path) -> Result<(), String> {
    let script = format!(
        "setcap -r {} 2>/dev/null || true; rm -f {} {} {} 2>/dev/null || true",
        binary.display(),
        HELPER_PATH,
        SUDOERS_PATH,
        POLKIT_POLICY_PATH
    );
    let output = Command::new("pkexec")
        .args(["bash", "-c", &script])
        .output()
        .map_err(|e| format!("could not run pkexec: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// Applies system routing table rules to direct OS traffic into the TUN interface.
pub fn apply_tun_routes(interface: &str, server_host: &str) -> Result<(), String> {
    if Path::new(HELPER_PATH).exists() {
        // 1. Try passwordless sudo -n first
        if let Ok(out) = Command::new("sudo")
            .args(["-n", HELPER_PATH, "up", interface, server_host])
            .output()
        {
            if out.status.success() {
                return Ok(());
            }
        }

        // 2. Try pkexec (runs without prompt if polkit policy is present)
        let output = Command::new("pkexec")
            .args([HELPER_PATH, "up", interface, server_host])
            .output()
            .map_err(|e| format!("could not run pkexec: {e}"))?;

        if output.status.success() {
            return Ok(());
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            if !err.trim().is_empty() {
                return Err(format!("TUN routing error: {}", err.trim()));
            }
        }
    }

    // Local user script fallback
    let script_path = crate::core::persist::config_dir()
        .map(|d| d.join("tun-helper.sh"))
        .unwrap_or_else(|| PathBuf::from("/home/ehsan/.config/xdelta/tun-helper.sh"));

    if script_path.exists() {
        let output = Command::new("pkexec")
            .args([
                script_path.to_str().unwrap_or(""),
                "up",
                interface,
                server_host,
            ])
            .output()
            .map_err(|e| format!("could not invoke tun helper: {e}"))?;

        if output.status.success() {
            return Ok(());
        }
    }

    Err("One-time TUN setup needed. Please click 'Grant Permission' in Settings.".to_string())
}

/// Removes system routing rules installed for the TUN interface.
pub fn revert_tun_routes(interface: &str, server_host: &str) -> Result<(), String> {
    if Path::new(HELPER_PATH).exists() {
        if let Ok(out) = Command::new("sudo")
            .args(["-n", HELPER_PATH, "down", interface, server_host])
            .output()
        {
            if out.status.success() {
                return Ok(());
            }
        }

        let _ = Command::new("pkexec")
            .args([HELPER_PATH, "down", interface, server_host])
            .spawn();
        return Ok(());
    }

    let script_path = crate::core::persist::config_dir()
        .map(|d| d.join("tun-helper.sh"))
        .unwrap_or_else(|| PathBuf::from("/home/ehsan/.config/xdelta/tun-helper.sh"));

    if script_path.exists() {
        let _ = Command::new("pkexec")
            .args([
                script_path.to_str().unwrap_or(""),
                "down",
                interface,
                server_host,
            ])
            .spawn();
    }

    Ok(())
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
