use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::Context;
use sha2::{Digest, Sha256};

const USER_AGENT: &str = concat!("atomic-installer/", env!("CARGO_PKG_VERSION"));

/// Downloads a URL to a file, printing a progress line when the size is known.
pub fn download_to_file(
    url: &str,
    dest: &Path,
    label: &str,
    retries: u32,
) -> Result<(), anyhow::Error> {
    let mut last_err = None;
    for attempt in 0..=retries {
        if attempt > 0 {
            println!("[WARN] retrying {label} (attempt {})", attempt + 1);
        }
        match download_to_file_once(url, dest, label) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap())
}

fn download_to_file_once(url: &str, dest: &Path, label: &str) -> Result<(), anyhow::Error> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("failed to download {label} from {url}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_reader();
    let mut file =
        File::create(dest).with_context(|| format!("failed to create {}", dest.display()))?;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_pct = u64::MAX;
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("failed while downloading {label}"))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .with_context(|| format!("failed to write {}", dest.display()))?;
        downloaded += n as u64;
        if let Some(total) = total {
            let pct = downloaded.saturating_mul(100) / total.max(1);
            if pct != last_pct {
                last_pct = pct;
                print!("\r[INFO] {label}: {downloaded} / {total} bytes ({pct}%)");
                let _ = std::io::stdout().flush();
            }
        }
    }
    if total.is_some() {
        println!();
    }
    println!("[INFO] downloaded {label} to {}", dest.display());
    Ok(())
}

/// Downloads a URL into memory and returns the raw bytes.
pub fn download_bytes(url: &str, label: &str, retries: u32) -> Result<Vec<u8>, anyhow::Error> {
    let mut last_err = None;
    for attempt in 0..=retries {
        if attempt > 0 {
            println!("[WARN] retrying {label} (attempt {})", attempt + 1);
        }
        match download_bytes_once(url, label) {
            Ok(b) => return Ok(b),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap())
}

fn download_bytes_once(url: &str, label: &str) -> Result<Vec<u8>, anyhow::Error> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("failed to download {label} from {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .read_to_end(&mut buf)
        .with_context(|| format!("failed while downloading {label}"))?;
    Ok(buf)
}

pub fn sha256_of_file(path: &Path) -> Result<String, anyhow::Error> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn verify_checksum(file: &Path, expected_hex: &str) -> Result<(), anyhow::Error> {
    let actual = sha256_of_file(file)?;
    if !actual.eq_ignore_ascii_case(expected_hex) {
        anyhow::bail!(
            "SHA256 mismatch for {}: expected {expected_hex}, got {actual}",
            file.display()
        );
    }
    Ok(())
}

pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), anyhow::Error> {
    // Reject path-traversal entries before extracting anything.
    let file =
        File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    for entry in tar
        .entries()
        .with_context(|| format!("failed to read {}", archive.display()))?
    {
        let entry = entry.context("failed to read an archive entry")?;
        let path = entry
            .path()
            .with_context(|| "failed to read an archive entry path")?;
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            anyhow::bail!("unsafe path in archive: {}", path.display());
        }
    }
    drop(tar);

    let file =
        File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    tar.unpack(dest).with_context(|| {
        format!(
            "failed to extract {} into {}",
            archive.display(),
            dest.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_of_file_matches_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f.bin");
        std::fs::write(&f, b"hello world").unwrap();
        assert_eq!(
            sha256_of_file(&f).unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn verify_checksum_accepts_match_and_rejects_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f.bin");
        std::fs::write(&f, b"hello world").unwrap();
        let good = sha256_of_file(&f).unwrap();
        assert!(verify_checksum(&f, &good).is_ok());
        assert!(verify_checksum(&f, &"0".repeat(64)).is_err());
    }

    #[test]
    fn extract_tar_gz_unpacks_fixture() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("fixture.tar.gz");
        let file = std::fs::File::create(&src).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(enc);
        let data: &[u8] = b"pwsh-binary-bytes";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "pwsh", data).unwrap();
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let out_dir = dir.path().join("out");
        std::fs::create_dir(&out_dir).unwrap();
        extract_tar_gz(&src, &out_dir).unwrap();
        assert_eq!(
            std::fs::read(out_dir.join("pwsh")).unwrap(),
            b"pwsh-binary-bytes"
        );
    }
}
