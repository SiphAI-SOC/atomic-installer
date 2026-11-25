use crate::pattern::{Arch, Bit, Os};

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub os: Option<Os>,
    pub arch: Option<Arch>,
    pub bit: Option<Bit>,
    pub auto: bool,
    pub uninstall: bool,
    pub force: bool,
    pub skip_atomic: bool,
    pub no_atomics_repo: bool,
    pub version: Option<String>,
    pub dry_run: bool,
    pub keep_archive: bool,
    pub check: bool,
    pub quiet: bool,
    pub retries: u32,
    pub prefix: Option<String>,
    pub get_prereqs: bool,
    pub upgrade: bool,
    pub json: bool,
}

pub enum CliOutcome {
    Run(Args),
    Help,
}

pub fn parse(args: &[String]) -> Result<CliOutcome, String> {
    let mut out = Args::default();
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(CliOutcome::Help),
            "--auto" | "-a" => out.auto = true,
            "--uninstall" | "-u" => out.uninstall = true,
            "--force" => out.force = true,
            "--skip-atomic" => out.skip_atomic = true,
            "--no-atomics-repo" => out.no_atomics_repo = true,
            "--version" => {
                let v = it
                    .next()
                    .ok_or("--version requires a value (e.g. v7.6.4)")?;
                out.version = Some(v.clone());
            }
            "--dry-run" => out.dry_run = true,
            "--keep-archive" => out.keep_archive = true,
            "--check" => out.check = true,
            "--quiet" => out.quiet = true,
            "--retries" => {
                let v = it.next().ok_or("--retries requires a value")?;
                out.retries = v.parse().map_err(|_| "invalid --retries value")?;
            }
            "--prefix" => {
                let v = it.next().ok_or("--prefix requires a value")?;
                out.prefix = Some(v.clone());
            }
            "--get-prereqs" => out.get_prereqs = true,
            "--upgrade" => out.upgrade = true,
            "--json" => out.json = true,
            "--os" => {
                let v = it.next().ok_or("--os requires a value (macos|linux)")?;
                out.os =
                    Some(Os::parse(v).ok_or_else(|| {
                        format!("invalid --os value '{v}' (expected macos|linux)")
                    })?);
            }
            "--arch" => {
                let v = it.next().ok_or("--arch requires a value (arm|x86)")?;
                out.arch = Some(
                    Arch::parse(v)
                        .ok_or_else(|| format!("invalid --arch value '{v}' (expected arm|x86)"))?,
                );
            }
            "--bit" => {
                let v = it.next().ok_or("--bit requires a value (32|64)")?;
                out.bit = Some(
                    Bit::parse(v)
                        .ok_or_else(|| format!("invalid --bit value '{v}' (expected 32|64)"))?,
                );
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
    }
    Ok(CliOutcome::Run(out))
}

pub fn usage() -> String {
    format!(
        "atomic-installer {}\n\
         \n\
         Installs the latest PowerShell release from GitHub, links pwsh, and installs Atomic Red Team.\n\
         \n\
         USAGE:\n    atomic-installer [OPTIONS]\n\
         \n\
         OPTIONS:\n    --os <macos|linux>      Operating system (default: detect or prompt)\n    --arch <arm|x86>        CPU architecture (default: detect or prompt)\n    --bit <32|64>           Bit type (default: detect or prompt)\n    -a, --auto              Auto-select platform from host detection (falls back to prompts if undetectable)\n    -u, --uninstall         Uninstall pwsh and Atomic Red Team instead of installing\n    --force                 Reinstall pwsh even if already working\n    --skip-atomic           Stop after pwsh is verified; skip Atomic Red Team\n    --no-atomics-repo       Skip downloading the atomics repository (module only)\n    --version <tag>         Install a specific PowerShell release (e.g. v7.6.4)\n    --dry-run               Fetch release metadata and print the plan; do not install\n    --keep-archive          Keep the downloaded .tar.gz after extraction\n    --check                 Verify the existing pwsh/Atomic Red Team install; no network\n    --quiet                 Suppress informational output\n    --retries <n>           Retry count for network downloads (default 0)\n    --prefix <dir>          Install root (default ~/.local)\n    --get-prereqs           Install Atomic Red Team prerequisites\n    --upgrade               Reinstall if installed version differs from latest\n    --json                  Print machine-readable summary\n    -h, --help              Print this help text\n",
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{Arch, Bit, Os};

    fn s(v: &str) -> String {
        v.to_string()
    }

    #[test]
    fn parses_all_flags() {
        let args = vec![
            s("prog"),
            s("--os"),
            s("linux"),
            s("--arch"),
            s("arm"),
            s("--bit"),
            s("32"),
            s("--force"),
            s("--skip-atomic"),
            s("--no-atomics-repo"),
        ];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert_eq!(a.os, Some(Os::Linux));
                assert_eq!(a.arch, Some(Arch::Arm));
                assert_eq!(a.bit, Some(Bit::Bit32));
                assert!(a.force);
                assert!(a.skip_atomic);
                assert!(a.no_atomics_repo);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn defaults_are_empty() {
        let args = vec![s("prog")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert!(a.os.is_none());
                assert!(a.arch.is_none());
                assert!(a.bit.is_none());
                assert!(!a.auto);
                assert!(!a.uninstall);
                assert!(!a.force);
                assert!(!a.skip_atomic);
                assert!(!a.no_atomics_repo);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_auto_flag() {
        let args = vec![s("prog"), s("--auto")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => assert!(a.auto),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_short_auto_flag() {
        let args = vec![s("prog"), s("-a")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => assert!(a.auto),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_uninstall_flag() {
        let args = vec![s("prog"), s("--uninstall")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => assert!(a.uninstall),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_short_uninstall_flag() {
        let args = vec![s("prog"), s("-u")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => assert!(a.uninstall),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_no_atomics_repo_flag() {
        let args = vec![s("prog"), s("--no-atomics-repo")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => assert!(a.no_atomics_repo),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn usage_lists_auto_flag() {
        let usage = usage();
        assert!(usage.contains("--auto"));
        assert!(usage.contains("-a,"));
    }

    #[test]
    fn usage_lists_uninstall_flag() {
        let usage = usage();
        assert!(usage.contains("--uninstall"));
        assert!(usage.contains("-u,"));
    }

    #[test]
    fn usage_lists_no_atomics_repo_flag() {
        let usage = usage();
        assert!(usage.contains("--no-atomics-repo"));
    }

    #[test]
    fn help_returns_help_outcome() {
        assert!(matches!(
            parse(&vec![s("prog"), s("--help")]),
            Ok(CliOutcome::Help)
        ));
    }

    #[test]
    fn unknown_flag_is_an_error() {
        assert!(parse(&vec![s("prog"), s("--bogus")]).is_err());
    }

    #[test]
    fn invalid_values_are_errors() {
        assert!(parse(&vec![s("prog"), s("--os"), s("windows")]).is_err());
        assert!(parse(&vec![s("prog"), s("--arch"), s("riscv")]).is_err());
        assert!(parse(&vec![s("prog"), s("--bit"), s("128")]).is_err());
        assert!(parse(&vec![s("prog"), s("--os")]).is_err());
    }
}
