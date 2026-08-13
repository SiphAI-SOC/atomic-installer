use crate::pattern::{Arch, Bit, Os};

#[derive(Debug, Clone, Default)]
pub struct Args {
    pub os: Option<Os>,
    pub arch: Option<Arch>,
    pub bit: Option<Bit>,
    pub force: bool,
    pub skip_atomic: bool,
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
            "--force" => out.force = true,
            "--skip-atomic" => out.skip_atomic = true,
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
         OPTIONS:\n    --os <macos|linux>      Operating system (default: detect or prompt)\n    --arch <arm|x86>        CPU architecture (default: detect or prompt)\n    --bit <32|64>           Bit type (default: detect or prompt)\n    --force                 Reinstall pwsh even if already working\n    --skip-atomic           Stop after pwsh is verified; skip Atomic Red Team\n    -h, --help              Print this help text\n",
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
        ];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert_eq!(a.os, Some(Os::Linux));
                assert_eq!(a.arch, Some(Arch::Arm));
                assert_eq!(a.bit, Some(Bit::Bit32));
                assert!(a.force);
                assert!(a.skip_atomic);
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
                assert!(!a.force);
                assert!(!a.skip_atomic);
            }
            _ => panic!("expected Run"),
        }
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
