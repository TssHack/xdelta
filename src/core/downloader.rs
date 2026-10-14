use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

pub const XRAY_LATEST_X86_URL: &str =
    "https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-64.zip";
pub const XRAY_LATEST_ARM64_URL: &str =
    "https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-arm64-v8a.zip";

/// Downloads and installs the latest official Xray-core binary into ~/.local/share/xdelta/bin.
pub async fn download_and_install_xray<F>(progress_cb: F) -> Result<PathBuf, String>
where
    F: Fn(&str) + 'static,
{
    progress_cb("Connecting to GitHub...");

    let target_bin_dir = crate::core::persist::user_bin_dir()
        .ok_or_else(|| "Could not determine user bin directory".to_string())?;
    let target_assets_dir = crate::core::persist::user_assets_dir()
        .ok_or_else(|| "Could not determine user assets directory".to_string())?;

    let is_arm = std::env::consts::ARCH == "aarch64";
    let url = if is_arm {
        XRAY_LATEST_ARM64_URL
    } else {
        XRAY_LATEST_X86_URL
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .user_agent("xdelta-core-downloader/0.1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    progress_cb("Downloading Xray-core package...");
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with HTTP {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read archive bytes: {e}"))?;

    progress_cb("Extracting core binaries and assets...");
    let temp_zip_path =
        std::env::temp_dir().join(format!("xray_download_{}.zip", std::process::id()));
    {
        let mut file = File::create(&temp_zip_path)
            .map_err(|e| format!("Failed to create temporary archive: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("Failed to write temporary archive: {e}"))?;
    }

    // Unpack using system unzip or python zipfile
    let unpack_res = Command::new("unzip")
        .args([
            "-o",
            "-q",
            temp_zip_path.to_str().unwrap_or(""),
            "xray",
            "geoip.dat",
            "geosite.dat",
            "-d",
            target_bin_dir.to_str().unwrap_or(""),
        ])
        .output();

    let unpacked = match unpack_res {
        Ok(out) if out.status.success() => true,
        _ => {
            let py_res = Command::new("python3")
                .args([
                    "-c",
                    &format!(
                        "import zipfile; z = zipfile.ZipFile(r'{}'); z.extractall(r'{}')",
                        temp_zip_path.display(),
                        target_bin_dir.display()
                    ),
                ])
                .output();
            matches!(py_res, Ok(out) if out.status.success())
        }
    };

    let _ = std::fs::remove_file(&temp_zip_path);

    if !unpacked {
        return Err("Failed to unpack Xray archive (unzip / python3 missing)".to_string());
    }

    let xray_exe = target_bin_dir.join("xray");
    if !xray_exe.exists() {
        return Err("Xray executable was not found in downloaded archive".to_string());
    }

    // Ensure permissions 0o755
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&xray_exe, std::fs::Permissions::from_mode(0o755));
    }

    // Copy dat files to user assets dir as well
    for dat in &["geoip.dat", "geosite.dat"] {
        let src = target_bin_dir.join(dat);
        if src.exists() {
            let dest = target_assets_dir.join(dat);
            let _ = std::fs::copy(&src, &dest);
        }
    }

    progress_cb("Xray-core successfully installed!");
    Ok(xray_exe)
}
