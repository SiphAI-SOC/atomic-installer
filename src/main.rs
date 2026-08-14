mod atomic;
mod cli;
mod download;
mod fsutil;
mod github;
mod pattern;
mod prompts;

use std::path::{Path, PathBuf};
use std::process::exit;

use anyhow::Context;

fn main() {
    let code = match real_main() {
        Ok(()) => 0,
        Err(e) => {
            if let Some(atomic_exit) = e.downcast_ref::<atomic::AtomicExit>() {
                eprintln!("[ERROR] {}", e);
                atomic_exit.0
            } else {
                eprintln!("[ERROR] {e:#}");
                1
            }
        }
    };
    exit(code);
}

fn real_main() -> Result<(), anyhow::Error> {
    let raw: Vec<String> = std::env::args().collect();
    let args = match cli::parse(&raw) {
        Ok(cli::CliOutcome::Run(a)) => a,
        Ok(cli::CliOutcome::Help) => {
            print!("{}", cli::usage());
            return Ok(());
        }
        Err(msg) => {
            eprintln!("[ERROR] {msg}");
            eprintln!();
            print!("{}", cli::usage());
            exit(2);
        }
    };

    println!("=== atomic-installer ===");
    println!(
        "[INFO] installs PowerShell from the latest GitHub release, links pwsh, and installs Atomic Red Team"
    );
    println!();

    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    let home = PathBuf::from(home);
    let install_dir = home.join(".local/share/powershell");
    let bin_dir = home.join(".local/bin");
    let pwsh_link = bin_dir.join("pwsh");
    let pwsh_binary = install_dir.join("pwsh");

    if args.uninstall {
        return uninstall(&install_dir, &pwsh_link);
    }

    let platform = prompts::resolve_platform(&args, args.auto)?;
    println!();
    println!("[INFO] selected platform: {}", platform.describe());
    println!(
        "[INFO] release asset pattern: powershell-<version>-{}.tar.gz",
        platform.token()
    );

    if !args.force && pwsh_works(&pwsh_link)? {
        println!("[INFO] pwsh is already installed and working; skipping the PowerShell install");
    } else {
        install_powershell(
            &platform,
            &install_dir,
            &bin_dir,
            &pwsh_link,
            &pwsh_binary,
            args.force,
        )?;
    }

    if args.skip_atomic {
        println!("[INFO] --skip-atomic given; not installing Atomic Red Team");
    } else {
        atomic::run_atomic_install(&pwsh_link, !args.no_atomics_repo)?;
    }

    println!();
    println!("=== summary ===");
    println!("[INFO] PowerShell binary: {}", pwsh_binary.display());
    println!("[INFO] pwsh link: {}", pwsh_link.display());
    println!(
        "[INFO] {}",
        atomic_summary_line(args.skip_atomic, args.no_atomics_repo)
    );
    Ok(())
}

fn atomic_summary_line(skip: bool, no_atomics_repo: bool) -> String {
    if skip {
        "Atomic Red Team: skipped (--skip-atomic)".to_string()
    } else if no_atomics_repo {
        "Atomic Red Team module installed without the atomics repository; run 'Invoke-AtomicRedTeam -getAtomics' from pwsh to download it later".to_string()
    } else {
        "Atomic Red Team: run 'Invoke-AtomicTest <technique-id>' from pwsh to use it".to_string()
    }
}

/// Returns true if the link exists and runs a version command successfully.
fn pwsh_works(pwsh_link: &Path) -> Result<bool, anyhow::Error> {
    if !pwsh_link.exists() {
        return Ok(false);
    }
    let out = std::process::Command::new(pwsh_link)
        .arg("-v")
        .output()
        .with_context(|| format!("failed to execute {}", pwsh_link.display()))?;
    let ok = out.status.success() && !out.stdout.is_empty();
    if ok {
        let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
        println!("[INFO] existing pwsh version: {ver}");
    } else {
        println!(
            "[WARN] {} exists but failed the version check; will reinstall",
            pwsh_link.display()
        );
    }
    Ok(ok)
}

fn uninstall(install_dir: &Path, pwsh_link: &Path) -> Result<(), anyhow::Error> {
    uninstall_with_ask(install_dir, pwsh_link, prompts::confirm)
}

fn uninstall_with_ask(
    install_dir: &Path,
    pwsh_link: &Path,
    ask: impl Fn(&str) -> Result<bool, anyhow::Error>,
) -> Result<(), anyhow::Error> {
    if !path_any_exists(install_dir) && !path_any_exists(pwsh_link) {
        println!("[WARN] nothing to uninstall");
        return Ok(());
    }

    println!("[INFO] this will remove:");
    println!("  {}", install_dir.display());
    println!("  {}", pwsh_link.display());
    if !ask("Remove these files and directories? [y/N]: ")? {
        anyhow::bail!("uninstall aborted");
    }

    fsutil::remove_if_symlink(pwsh_link)?;
    fsutil::remove_dir_if_exists(install_dir)?;

    println!("[INFO] uninstalled PowerShell and Atomic Red Team");
    Ok(())
}

/// True when the path exists, including as a dangling symlink.
fn path_any_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn install_powershell(
    platform: &pattern::Platform,
    install_dir: &Path,
    bin_dir: &Path,
    pwsh_link: &Path,
    pwsh_binary: &Path,
    force: bool,
) -> Result<(), anyhow::Error> {
    println!("[INFO] fetching the latest PowerShell release from GitHub");
    let release = github::fetch_latest_release()?;
    println!(
        "[INFO] latest release: {} (version {})",
        release.tag, release.version
    );

    let filename = platform.asset_filename(&release.version);
    let asset = github::find_asset(&release.assets, &filename).with_context(|| {
        format!(
            "no asset matching {filename} in release {}; available: {}",
            release.tag,
            release
                .assets
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let hash_asset = github::find_asset(&release.assets, "hashes.sha256")
        .context("hashes.sha256 asset not found in release")?;

    let tmp = std::env::temp_dir().join(format!(
        "atomic-installer-{}-{}",
        std::process::id(),
        filename
    ));

    let result = (|| -> Result<(), anyhow::Error> {
        println!("[INFO] downloading {} ({})", asset.name, asset.url);
        download::download_to_file(&asset.url, &tmp, &asset.name)?;

        println!("[INFO] downloading checksums (hashes.sha256)");
        let hash_bytes = download::download_bytes(&hash_asset.url, "hashes.sha256")?;
        let hashes = github::parse_hashes(&hash_bytes).context("failed to parse hashes.sha256")?;
        let expected = hashes
            .get(&filename)
            .with_context(|| format!("no checksum for {filename} in hashes.sha256"))?;

        println!("[INFO] verifying SHA256 of {}", asset.name);
        download::verify_checksum(&tmp, expected)?;
        println!("[INFO] checksum OK");

        fsutil::ensure_dir(install_dir)?;
        if force {
            println!(
                "[INFO] --force: clearing existing contents of {}",
                install_dir.display()
            );
            fsutil::clear_dir_contents(install_dir)?;
        }
        println!(
            "[INFO] extracting {} into {}",
            asset.name,
            install_dir.display()
        );
        download::extract_tar_gz(&tmp, install_dir)?;
        if !pwsh_binary.exists() {
            anyhow::bail!(
                "pwsh binary not found at {} after extraction",
                pwsh_binary.display()
            );
        }
        Ok(())
    })();

    let _ = std::fs::remove_file(&tmp);
    result?;

    fsutil::ensure_dir(bin_dir)?;
    fsutil::create_or_replace_symlink(pwsh_binary, pwsh_link)?;

    let out = std::process::Command::new(pwsh_link)
        .arg("-v")
        .output()
        .with_context(|| format!("failed to verify the installed {}", pwsh_link.display()))?;
    if !out.status.success() {
        anyhow::bail!(
            "installed pwsh at {} failed the version check",
            pwsh_link.display()
        );
    }
    println!(
        "[INFO] installed pwsh version: {}",
        String::from_utf8_lossy(&out.stdout).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_summary_default_points_to_invoke_atomic_test() {
        assert!(atomic_summary_line(false, false).contains("Invoke-AtomicTest"));
    }

    #[test]
    fn atomic_summary_no_atomics_repo_offers_recovery() {
        let line = atomic_summary_line(false, true);
        assert!(line.contains("without the atomics repository"));
        assert!(line.contains("-getAtomics"));
    }

    #[test]
    fn atomic_summary_skipped_reports_skip() {
        assert!(atomic_summary_line(true, false).contains("--skip-atomic"));
        assert!(atomic_summary_line(true, true).contains("--skip-atomic"));
    }

    #[test]
    fn uninstall_nothing_installed_asks_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("share/powershell");
        let pwsh_link = dir.path().join("bin/pwsh");
        let asked = std::cell::Cell::new(false);
        uninstall_with_ask(&install_dir, &pwsh_link, |_| {
            asked.set(true);
            Ok(true)
        })
        .unwrap();
        assert!(!asked.get());
    }

    #[test]
    fn uninstall_declined_aborts_without_removing() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("share/powershell");
        let pwsh_link = dir.path().join("bin/pwsh");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(pwsh_link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(install_dir.join("pwsh"), &pwsh_link).unwrap();
        assert!(uninstall_with_ask(&install_dir, &pwsh_link, |_| Ok(false)).is_err());
        assert!(install_dir.exists());
        assert!(std::fs::symlink_metadata(&pwsh_link).is_ok());
    }

    #[test]
    fn uninstall_removes_symlink_and_install_dir() {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("share/powershell");
        let pwsh_link = dir.path().join("bin/pwsh");
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::create_dir_all(pwsh_link.parent().unwrap()).unwrap();
        std::fs::write(install_dir.join("pwsh"), b"binary").unwrap();
        std::os::unix::fs::symlink(install_dir.join("pwsh"), &pwsh_link).unwrap();
        uninstall_with_ask(&install_dir, &pwsh_link, |_| Ok(true)).unwrap();
        assert!(!install_dir.exists());
        assert!(std::fs::symlink_metadata(&pwsh_link).is_err());
    }
}
