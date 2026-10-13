use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub name: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    pub body: Option<String>,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: String,
    pub title: String,
    pub html_url: String,
    pub published_at: String,
    pub notes: String,
    pub has_update: bool,
    pub download_url: Option<String>,
}

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_API_URL: &str = "https://api.github.com/repos/TssHack/xdelta/releases/latest";

/// Checks GitHub Releases API for the latest version of xdelta.
pub async fn check_for_updates() -> Result<ReleaseInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("xdelta-updater/0.1.0 (Linux; Rust)")
        .build()
        .map_err(|e| format!("Failed to create client: {}", e))?;

    let response = client
        .get(GITHUB_API_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to connect to GitHub API: {}", e))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // No release yet on repo (repo is brand new)
        return Ok(ReleaseInfo {
            version: CURRENT_VERSION.to_string(),
            title: "Up to Date".to_string(),
            html_url: "https://github.com/TssHack/xdelta".to_string(),
            published_at: "Just now".to_string(),
            notes: "You are running the latest build.".to_string(),
            has_update: false,
            download_url: None,
        });
    }

    if !response.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", response.status()));
    }

    let release: GithubRelease = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse release metadata: {}", e))?;

    let latest_clean = release.tag_name.trim_start_matches('v').to_string();
    let has_update = compare_versions(&latest_clean, CURRENT_VERSION);

    let download_url = release
        .assets
        .iter()
        .find(|a| a.name.contains("linux") || a.name.contains("xdelta"))
        .map(|a| a.browser_download_url.clone())
        .or_else(|| Some(release.html_url.clone()));

    Ok(ReleaseInfo {
        version: latest_clean,
        title: release.name.unwrap_or_else(|| release.tag_name.clone()),
        html_url: release.html_url,
        published_at: release.published_at.unwrap_or_default(),
        notes: release.body.unwrap_or_default(),
        has_update,
        download_url,
    })
}

/// Simple semver comparison: returns true if remote > current
fn compare_versions(remote: &str, current: &str) -> bool {
    let parse_parts = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|p| {
                p.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    };

    let r_parts = parse_parts(remote);
    let c_parts = parse_parts(current);

    for (r, c) in r_parts.iter().zip(c_parts.iter()) {
        if r > c {
            return true;
        } else if r < c {
            return false;
        }
    }

    r_parts.len() > c_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_comparison() {
        assert!(compare_versions("0.2.0", "0.1.0"));
        assert!(compare_versions("1.0.0", "0.9.9"));
        assert!(compare_versions("0.1.1", "0.1.0"));
        assert!(!compare_versions("0.1.0", "0.1.0"));
        assert!(!compare_versions("0.0.9", "0.1.0"));
    }
}
