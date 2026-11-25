use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

const API_URL: &str = "https://api.github.com/repos/PowerShell/PowerShell/releases/latest";
const USER_AGENT: &str = concat!("atomic-installer/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    assets: Vec<ApiAsset>,
}

#[derive(Debug, Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub assets: Vec<Asset>,
}

pub fn fetch_latest_release() -> Result<Release, anyhow::Error> {
    let body = ureq::get(API_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("failed to query the GitHub API for the latest PowerShell release")?
        .into_string()
        .context("failed to read the GitHub API response")?;
    parse_release_json(&body)
}

pub fn parse_release_json(body: &str) -> Result<Release, anyhow::Error> {
    let api: ApiRelease =
        serde_json::from_str(body).context("failed to parse the GitHub release JSON")?;
    Ok(Release {
        version: api.tag_name.trim_start_matches('v').to_string(),
        tag: api.tag_name,
        assets: api
            .assets
            .into_iter()
            .map(|a| Asset {
                name: a.name,
                url: a.browser_download_url,
            })
            .collect(),
    })
}

pub fn find_asset<'a>(assets: &'a [Asset], filename: &str) -> Option<&'a Asset> {
    assets.iter().find(|a| a.name == filename)
}

/// Parses hashes.sha256 content (UTF-8 or UTF-16, with or without BOM).
/// Returns a map of filename -> sha256 hex.
pub fn parse_hashes(bytes: &[u8]) -> Result<HashMap<String, String>, anyhow::Error> {
    let text = decode_text(bytes).context("hashes.sha256 is neither valid UTF-8 nor UTF-16")?;
    let text = text.trim_start_matches('\u{FEFF}');
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (hash, filename) = match (parts.next(), parts.next()) {
            (Some(h), Some(f)) => (h, f.trim_start_matches('*')),
            _ => continue,
        };
        map.insert(filename.to_string(), hash.to_string());
    }
    Ok(map)
}

fn decode_text(bytes: &[u8]) -> Option<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect::<Vec<u16>>();
        String::from_utf16(&units).ok()
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect::<Vec<u16>>();
        String::from_utf16(&units).ok()
    } else {
        String::from_utf8(bytes.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_json() {
        let body = r#"{"tag_name":"v7.6.4","assets":[
            {"name":"powershell-7.6.4-osx-arm64.tar.gz","browser_download_url":"https://x/powershell-7.6.4-osx-arm64.tar.gz"},
            {"name":"hashes.sha256","browser_download_url":"https://x/hashes.sha256"}
        ]}"#;
        let release = parse_release_json(body).unwrap();
        assert_eq!(release.tag, "v7.6.4");
        assert_eq!(release.version, "7.6.4");
        assert_eq!(release.assets.len(), 2);
        assert_eq!(
            release.assets[0].url,
            "https://x/powershell-7.6.4-osx-arm64.tar.gz"
        );
    }

    #[test]
    fn find_asset_requires_exact_name_match() {
        let assets = vec![
            Asset {
                name: "powershell-7.6.4-linux-musl-x64.tar.gz".to_string(),
                url: "u1".to_string(),
            },
            Asset {
                name: "powershell-7.6.4-linux-x64.tar.gz".to_string(),
                url: "u2".to_string(),
            },
        ];
        assert_eq!(
            find_asset(&assets, "powershell-7.6.4-linux-x64.tar.gz").map(|a| a.url.as_str()),
            Some("u2")
        );
        assert_eq!(
            find_asset(&assets, "powershell-7.6.4-linux-arm64.tar.gz"),
            None
        );
    }

    #[test]
    fn parses_utf8_hashes() {
        let bytes = b"abc123  powershell-7.6.4-linux-x64.tar.gz\ndef456 *powershell-7.6.4-osx-arm64.tar.gz\n";
        let map = parse_hashes(bytes).unwrap();
        assert_eq!(map["powershell-7.6.4-linux-x64.tar.gz"], "abc123");
        assert_eq!(map["powershell-7.6.4-osx-arm64.tar.gz"], "def456");
    }

    #[test]
    fn parses_utf16le_hashes_like_powershell_ships() {
        let text = "9d82ce  *powershell-7.6.4-osx-arm64.tar.gz\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let map = parse_hashes(&bytes).unwrap();
        assert_eq!(map["powershell-7.6.4-osx-arm64.tar.gz"], "9d82ce");
    }
}
