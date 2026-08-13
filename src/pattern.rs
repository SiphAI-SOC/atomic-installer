#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Macos,
    Linux,
}

impl Os {
    pub fn parse(s: &str) -> Option<Os> {
        match s.to_ascii_lowercase().as_str() {
            "macos" | "mac" | "darwin" | "osx" => Some(Os::Macos),
            "linux" => Some(Os::Linux),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Os::Macos => "macOS",
            Os::Linux => "Linux",
        }
    }

    /// The host OS from `std::env::consts`, if supported.
    #[allow(dead_code)]
    pub fn detect() -> Option<Os> {
        Os::parse(std::env::consts::OS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm,
    X86,
}

impl Arch {
    pub fn parse(s: &str) -> Option<Arch> {
        match s.to_ascii_lowercase().as_str() {
            "arm" | "aarch64" | "arm64" => Some(Arch::Arm),
            "x86" | "x64" | "amd64" | "x86_64" | "intel" => Some(Arch::X86),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Arch::Arm => "ARM",
            Arch::X86 => "x86",
        }
    }

    /// The host CPU architecture from `std::env::consts`, if supported.
    #[allow(dead_code)]
    pub fn detect() -> Option<Arch> {
        Arch::parse(std::env::consts::ARCH)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bit {
    Bit32,
    Bit64,
}

impl Bit {
    pub fn parse(s: &str) -> Option<Bit> {
        match s.to_ascii_lowercase().as_str() {
            "32" | "32-bit" | "32bit" | "i386" | "i686" => Some(Bit::Bit32),
            "64" | "64-bit" | "64bit" => Some(Bit::Bit64),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Bit::Bit32 => "32-bit",
            Bit::Bit64 => "64-bit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    pub bit: Bit,
}

impl Platform {
    /// The bit widths available for a given OS and architecture,
    /// based on the assets actually shipped by PowerShell.
    pub fn available_bits(os: Os, arch: Arch) -> Vec<Bit> {
        match (os, arch) {
            (Os::Macos, _) => vec![Bit::Bit64],
            (Os::Linux, Arch::Arm) => vec![Bit::Bit64, Bit::Bit32],
            (Os::Linux, Arch::X86) => vec![Bit::Bit64],
        }
    }

    /// Builds a platform from parts, rejecting unsupported combinations.
    pub fn from_parts(os: Os, arch: Arch, bit: Bit) -> Result<Platform, String> {
        let bits = Self::available_bits(os, arch);
        if bits.contains(&bit) {
            Ok(Platform { os, arch, bit })
        } else {
            let opts = bits
                .iter()
                .map(|b| b.label())
                .collect::<Vec<_>>()
                .join(" or ");
            Err(format!(
                "invalid combination: {} {} {} has no PowerShell tar.gz asset; supported bit type(s): {}",
                os.label(),
                arch.label(),
                bit.label(),
                opts
            ))
        }
    }

    /// The release-asset token, e.g. "osx-arm64" or "linux-arm32".
    pub fn token(&self) -> String {
        match (self.os, self.arch, self.bit) {
            (Os::Macos, Arch::Arm, _) => "osx-arm64".to_string(),
            (Os::Macos, Arch::X86, _) => "osx-x64".to_string(),
            (Os::Linux, Arch::Arm, Bit::Bit64) => "linux-arm64".to_string(),
            (Os::Linux, Arch::Arm, Bit::Bit32) => "linux-arm32".to_string(),
            (Os::Linux, Arch::X86, _) => "linux-x64".to_string(),
        }
    }

    /// e.g. "powershell-7.6.4-osx-arm64.tar.gz"
    pub fn asset_filename(&self, version: &str) -> String {
        format!("powershell-{}-{}.tar.gz", version, self.token())
    }

    pub fn describe(&self) -> String {
        format!(
            "{} {} {}",
            self.os.label(),
            self.arch.label(),
            self.bit.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_combinations_have_correct_tokens() {
        assert_eq!(
            Platform::from_parts(Os::Macos, Arch::Arm, Bit::Bit64)
                .unwrap()
                .token(),
            "osx-arm64"
        );
        assert_eq!(
            Platform::from_parts(Os::Macos, Arch::X86, Bit::Bit64)
                .unwrap()
                .token(),
            "osx-x64"
        );
        assert_eq!(
            Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit64)
                .unwrap()
                .token(),
            "linux-arm64"
        );
        assert_eq!(
            Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32)
                .unwrap()
                .token(),
            "linux-arm32"
        );
        assert_eq!(
            Platform::from_parts(Os::Linux, Arch::X86, Bit::Bit64)
                .unwrap()
                .token(),
            "linux-x64"
        );
    }

    #[test]
    fn invalid_combinations_are_rejected() {
        assert!(Platform::from_parts(Os::Macos, Arch::Arm, Bit::Bit32).is_err());
        assert!(Platform::from_parts(Os::Macos, Arch::X86, Bit::Bit32).is_err());
        assert!(Platform::from_parts(Os::Linux, Arch::X86, Bit::Bit32).is_err());
    }

    #[test]
    fn asset_filename_matches_github_naming() {
        let p = Platform::from_parts(Os::Macos, Arch::Arm, Bit::Bit64).unwrap();
        assert_eq!(
            p.asset_filename("7.6.4"),
            "powershell-7.6.4-osx-arm64.tar.gz"
        );
        let p = Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32).unwrap();
        assert_eq!(
            p.asset_filename("7.6.4"),
            "powershell-7.6.4-linux-arm32.tar.gz"
        );
    }

    #[test]
    fn available_bits_reflects_real_assets() {
        assert_eq!(
            Platform::available_bits(Os::Linux, Arch::Arm),
            vec![Bit::Bit64, Bit::Bit32]
        );
        assert_eq!(
            Platform::available_bits(Os::Linux, Arch::X86),
            vec![Bit::Bit64]
        );
        assert_eq!(
            Platform::available_bits(Os::Macos, Arch::Arm),
            vec![Bit::Bit64]
        );
        assert_eq!(
            Platform::available_bits(Os::Macos, Arch::X86),
            vec![Bit::Bit64]
        );
    }

    #[test]
    fn parse_accepts_aliases() {
        assert_eq!(Os::parse("macos"), Some(Os::Macos));
        assert_eq!(Os::parse("linux"), Some(Os::Linux));
        assert_eq!(Os::parse("windows"), None);
        assert_eq!(Arch::parse("arm64"), Some(Arch::Arm));
        assert_eq!(Arch::parse("x64"), Some(Arch::X86));
        assert_eq!(Bit::parse("32"), Some(Bit::Bit32));
        assert_eq!(Bit::parse("64"), Some(Bit::Bit64));
    }

    #[test]
    fn detect_delegates_to_parse() {
        assert_eq!(Os::detect(), Os::parse(std::env::consts::OS));
        assert_eq!(Arch::detect(), Arch::parse(std::env::consts::ARCH));
    }

    #[test]
    fn env_const_strings_are_parseable() {
        assert_eq!(Os::parse("macos"), Some(Os::Macos));
        assert_eq!(Os::parse("linux"), Some(Os::Linux));
        assert_eq!(Arch::parse("aarch64"), Some(Arch::Arm));
        assert_eq!(Arch::parse("x86_64"), Some(Arch::X86));
    }

    #[test]
    fn unsupported_hosts_are_not_detected() {
        assert_eq!(Os::parse("windows"), None);
        assert_eq!(Os::parse("freebsd"), None);
        assert_eq!(Arch::parse("riscv64"), None);
        assert_eq!(Arch::parse("powerpc64"), None);
    }
}
