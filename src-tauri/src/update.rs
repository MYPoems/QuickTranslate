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
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
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

pub fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate)
        .is_some_and(|candidate| parse_version(current).is_some_and(|current| candidate > current))
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value.trim().trim_start_matches('v');
    if core.contains(['-', '+']) {
        return None;
    }
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[derive(Clone, Default, Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Verifying,
    Ready,
    Installing,
    Cancelled,
    Error,
}

#[derive(Clone, Default, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: UpdatePhase,
    pub version: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub message: String,
    pub release_notes: String,
}

pub fn validate_install(
    phase: &UpdatePhase,
    version: &str,
    requested: &str,
    confirmed: bool,
    bytes: &[u8],
) -> Result<(), AppError> {
    if !confirmed {
        return Err(AppError::Update("必须由用户明确确认安装更新".into()));
    }
    if phase != &UpdatePhase::Ready
        || bytes.is_empty()
        || requested != version
        || !is_newer(version, env!("CARGO_PKG_VERSION"))
    {
        return Err(AppError::Update(
            "没有与确认版本匹配、已校验的更新包，请重新检查更新".into(),
        ));
    }
    Ok(())
}

pub fn validate_download_url(url: &reqwest::Url, version: &str) -> Result<(), AppError> {
    let expected = format!("/MYPoems/QuickTranslate/releases/download/v{version}/");
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.path().starts_with(&expected)
        || !url.path().ends_with("-setup.exe")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Update(
            "更新包地址不是该版本的官方 GitHub 安装包".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_versions_without_lexical_errors() {
        assert!(is_newer("v1.10.0", "1.9.9"));
        assert!(!is_newer("v1.0.0", "1.0.0"));
        assert!(!is_newer("invalid", "1.0.0"));
        assert!(!is_newer("v9.0.0-beta", "1.0.0"));
        assert!(!is_newer("1.2.3.4", "1.0.0"));
    }

    #[test]
    fn installation_requires_confirmation_verified_bytes_and_exact_new_version() {
        let current = env!("CARGO_PKG_VERSION");
        let major = current.split('.').next().unwrap().parse::<u64>().unwrap();
        let next = format!("{}.0.0", major + 1);
        assert!(validate_install(&UpdatePhase::Ready, &next, &next, true, b"verified").is_ok());
        for phase in [
            UpdatePhase::Available,
            UpdatePhase::Downloading,
            UpdatePhase::Verifying,
            UpdatePhase::Error,
            UpdatePhase::Cancelled,
        ] {
            assert!(validate_install(&phase, &next, &next, true, b"data").is_err());
        }
        assert!(validate_install(&UpdatePhase::Ready, &next, &next, false, b"data").is_err());
        assert!(validate_install(&UpdatePhase::Ready, &next, current, true, b"data").is_err());
        assert!(validate_install(&UpdatePhase::Ready, &next, &next, true, b"").is_err());
        assert!(validate_install(&UpdatePhase::Ready, current, current, true, b"data").is_err());
        assert!(validate_install(&UpdatePhase::Ready, "1.0.0", "1.0.0", true, b"data").is_err());
    }

    #[test]
    fn rejects_untrusted_download_hosts_versions_and_paths() {
        let good = "https://github.com/MYPoems/QuickTranslate/releases/download/v1.3.0/QuickTranslate_1.3.0_x64-setup.exe";
        assert!(validate_download_url(&reqwest::Url::parse(good).unwrap(), "1.3.0").is_ok());
        for bad in [
            good.replace("https:", "http:"),
            good.replace("github.com", "github.com.evil.test"),
            good.replace("MYPoems", "attacker"),
            good.replace("v1.3.0/", "v1.0.0/"),
            format!("{good}?token=secret"),
        ] {
            assert!(validate_download_url(&reqwest::Url::parse(&bad).unwrap(), "1.3.0").is_err());
        }
    }

    #[test]
    #[ignore = "requires signed release artifact paths in environment"]
    fn verify_release_artifact() {
        use base64::{engine::general_purpose::STANDARD, Engine};
        use minisign_verify::{PublicKey, Signature};
        let artifact = std::fs::read(
            std::env::var_os("QUICKTRANSLATE_VERIFY_ARTIFACT").expect("artifact path"),
        )
        .unwrap();
        let signature = std::fs::read_to_string(
            std::env::var_os("QUICKTRANSLATE_VERIFY_SIGNATURE").expect("signature path"),
        )
        .unwrap();
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let public_key = String::from_utf8(
            STANDARD
                .decode(config["plugins"]["updater"]["pubkey"].as_str().unwrap())
                .unwrap(),
        )
        .unwrap();
        let signature_text = String::from_utf8(STANDARD.decode(signature.trim()).unwrap()).unwrap();
        let public_key = PublicKey::decode(&public_key).unwrap();
        let signature = Signature::decode(&signature_text).unwrap();
        public_key.verify(&artifact, &signature, true).unwrap();
        let signed_version = signature
            .trusted_comment()
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"));
        assert_eq!(
            signed_version,
            Some(env!("CARGO_PKG_VERSION")),
            "signature must bind the release version"
        );
        let mut tampered = artifact;
        tampered[0] ^= 1;
        assert!(
            public_key.verify(&tampered, &signature, true).is_err(),
            "tampered installer must be rejected"
        );
    }
}
