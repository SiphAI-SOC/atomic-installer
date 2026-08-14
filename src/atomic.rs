use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::Context;

const INSTALL_COMMAND: &str = "IEX (IWR 'https://raw.githubusercontent.com/redcanaryco/invoke-atomicredteam/master/install-atomicredteam.ps1' -UseBasicParsing); Install-AtomicRedTeam";

pub fn install_command(get_atomics: bool) -> String {
    if get_atomics {
        format!("{INSTALL_COMMAND} -getAtomics")
    } else {
        INSTALL_COMMAND.to_string()
    }
}

#[derive(Debug)]
pub struct AtomicExit(pub i32);

impl std::fmt::Display for AtomicExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Install-AtomicRedTeam exited with code {}", self.0)
    }
}

impl std::error::Error for AtomicExit {}

pub fn run_atomic_install(pwsh: &Path, get_atomics: bool) -> Result<(), anyhow::Error> {
    println!(
        "[INFO] running the Atomic Red Team installer via {}",
        pwsh.display()
    );
    if get_atomics {
        println!("[INFO] installing with the atomics repository (-getAtomics)");
    } else {
        println!("[INFO] installing without the atomics repository (--no-atomics-repo)");
    }
    let status = Command::new(pwsh)
        .arg("-NoProfile")
        .arg("-Command")
        .arg(install_command(get_atomics))
        .status()
        .with_context(|| format!("failed to execute {}", pwsh.display()))?;
    if status.success() {
        println!("[INFO] Atomic Red Team install finished successfully");
        Ok(())
    } else {
        Err(anyhow::Error::new(AtomicExit(exit_code(&status))))
    }
}

fn exit_code(status: &ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_command_includes_get_atomics() {
        assert!(install_command(true).ends_with(" -getAtomics"));
    }

    #[test]
    fn install_command_omits_get_atomics() {
        assert!(!install_command(false).ends_with(" -getAtomics"));
    }
}
