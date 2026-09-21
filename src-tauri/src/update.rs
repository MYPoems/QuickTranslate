use serde::{Deserialize, Serialize};

use crate::errors::AppError;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/MYPoems/QuickTranslate/releases/latest";

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current_version: String,
    latest_version: String,
    update_available: bool,
    release_url: String,
}

pub async fn check_for_updates(client: &reqwest::Client) -> Result<UpdateInfo, AppError> {
    let release = client
        .get(LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| AppError::Network(error.to_string()))?
        .error_for_status()
        .map_err(|error| AppError::Network(error.to_string()))?
        .json::<GitHubRelease>()
        .await
        .map_err(|error| AppError::Provider(format!("GitHub Release 响应无效：{error}")))?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let update_available = is_newer(&release.tag_name, &current_version);
    Ok(UpdateInfo {
        current_version,
        latest_version: release.tag_name.trim_start_matches('v').to_string(),
        update_available,
        release_url: release.html_url,
    })
}

fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate)
        .is_some_and(|candidate| parse_version(current).is_some_and(|current| candidate > current))
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value.trim().trim_start_matches('v').split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions_without_lexical_errors() {
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("invalid", "1.0.0"));
    }
}
