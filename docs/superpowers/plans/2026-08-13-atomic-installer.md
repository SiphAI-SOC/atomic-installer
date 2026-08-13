# atomic-installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A single-binary Rust CLI that installs the latest PowerShell release from GitHub, symlinks `pwsh` into `~/.local/bin`, and installs Atomic Red Team via the official installer script.

**Architecture:** A linear-phase CLI. `main.rs` orchestrates phases; small focused modules handle platform selection (`pattern.rs`, `prompts.rs`, `cli.rs`), the GitHub API (`github.rs`), download/verify/extract (`download.rs`), filesystem helpers (`fsutil.rs`), and the Atomic Red Team step (`atomic.rs`). All network work is blocking; the Atomic step inherits stdio so installer prompts can be answered.

**Tech Stack:** Rust (edition 2024), `ureq` (HTTP), `flate2`+`tar` (extraction), `serde`+`serde_json` (GitHub API), `sha2`+`hex` (checksums), `anyhow` (errors), `tempfile` (dev-only).

## Global Constraints

- No emojis anywhere in program output.
- All program output uses `[INFO]`, `[WARN]`, or `[ERROR]` prefixes.
- Exit codes: `0` success, `1` runtime failure, `2` CLI usage error. A non-zero Atomic Red Team exit code is propagated as-is.
- Supported platforms are macOS and Linux only (uses `std::os::unix::fs::symlink`).
- Asset naming pattern (verified against real releases): `powershell-{version}-{os}-{arch}.tar.gz` where the token is one of `osx-arm64`, `osx-x64`, `linux-arm64`, `linux-arm32`, `linux-x64`. Exact-match only (excludes `musl`, `fxdependent` variants).
- Version string = release tag with leading `v` stripped.
- The release also carries `hashes.sha256`, which is UTF-16LE with a BOM; the parser must also accept UTF-8.
- Install dir: `$HOME/.local/share/powershell`. Link dir: `$HOME/.local/bin`. Link: `$HOME/.local/bin/pwsh -> $HOME/.local/share/powershell/pwsh`.
- Dependency major versions: `anyhow = "1"`, `flate2 = "1"`, `hex = "0.4"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `sha2 = "0.10"`, `tar = "0.4"`, `ureq = "2"`, dev `tempfile = "3"`.
- Git: this project is not a git repository. If git is desired, run `git init` in Task 1; otherwise skip every commit step.

---

### Task 1: Scaffold the crate and dependencies

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a crate that builds, with `src/main.rs` declaring all modules (`mod cli; mod prompts; mod pattern; mod github; mod download; mod fsutil; mod atomic;`).

- [ ] **Step 1: Write the Cargo.toml**

```toml
[package]
name = "atomic-installer"
version = "0.1.0"
edition = "2024"

[dependencies]
anyhow = "1"
flate2 = "1"
hex = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.10"
tar = "0.4"
ureq = "2"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Replace `src/main.rs` with a module skeleton**

```rust
mod atomic;
mod cli;
mod download;
mod fsutil;
mod github;
mod pattern;
mod prompts;

fn main() {
    println!("Hello, world!");
}
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build`
Expected: builds successfully (module files are declared but do not exist yet, so this will FAIL with "can't find crate" / unresolved `mod` errors).

- [ ] **Step 4: Create empty stub modules so the build passes**

Create these files, each containing only a doc comment and its `mod`-level content placeholder (empty is fine):

- `src/cli.rs`, `src/prompts.rs`, `src/pattern.rs`, `src/github.rs`, `src/download.rs`, `src/fsutil.rs`, `src/atomic.rs`

Each stub is just:
```rust
// see implementation plan
```

- [ ] **Step 5: Verify the build passes**

Run: `cargo build`
Expected: PASS.

- [ ] **Step 6: (Optional) Initialize git**

If the user wants version control: `git init && git add -A && git commit -m "chore: scaffold crate"`. Otherwise skip all commit steps in later tasks.

---

### Task 2: Platform pattern (`src/pattern.rs`)

**Files:**
- Modify: `src/pattern.rs`

**Interfaces:**
- Consumes: nothing (pure module).
- Produces:
  - `pub enum Os { Macos, Linux }` with `pub fn parse(s: &str) -> Option<Os>`, `pub fn label(&self) -> &'static str`, `pub fn token(&self) -> &'static str`
  - `pub enum Arch { Arm, X86 }` with `parse`, `label`, `token`
  - `pub enum Bit { Bit32, Bit64 }` with `parse`, `label`, `token`
  - `pub struct Platform { pub os: Os, pub arch: Arch, pub bit: Bit }`
  - `impl Platform`: `pub fn available_bits(os: Os, arch: Arch) -> Vec<Bit>`, `pub fn from_parts(os: Os, arch: Arch, bit: Bit) -> Result<Platform, String>`, `pub fn token(&self) -> String`, `pub fn asset_filename(&self, version: &str) -> String`, `pub fn describe(&self) -> String`
  - All three enums derive `Debug, Clone, Copy, PartialEq, Eq`; `Platform` derives `Debug, Clone, Copy, PartialEq, Eq`.

- [ ] **Step 1: Write the failing tests** (append `#[cfg(test)] mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_combinations_have_correct_tokens() {
        assert_eq!(Platform::from_parts(Os::Macos, Arch::Arm, Bit::Bit64).unwrap().token(), "osx-arm64");
        assert_eq!(Platform::from_parts(Os::Macos, Arch::X86, Bit::Bit64).unwrap().token(), "osx-x64");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit64).unwrap().token(), "linux-arm64");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32).unwrap().token(), "linux-arm32");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::X86, Bit::Bit64).unwrap().token(), "linux-x64");
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
        assert_eq!(p.asset_filename("7.6.4"), "powershell-7.6.4-osx-arm64.tar.gz");
        let p = Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32).unwrap();
        assert_eq!(p.asset_filename("7.6.4"), "powershell-7.6.4-linux-arm32.tar.gz");
    }

    #[test]
    fn available_bits_reflects_real_assets() {
        assert_eq!(Platform::available_bits(Os::Linux, Arch::Arm), vec![Bit::Bit64, Bit::Bit32]);
        assert_eq!(Platform::available_bits(Os::Linux, Arch::X86), vec![Bit::Bit64]);
        assert_eq!(Platform::available_bits(Os::Macos, Arch::Arm), vec![Bit::Bit64]);
        assert_eq!(Platform::available_bits(Os::Macos, Arch::X86), vec![Bit::Bit64]);
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test pattern::tests -v`
Expected: FAIL (types do not exist).

- [ ] **Step 3: Write the implementation**

```rust
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

    pub fn token(&self) -> &'static str {
        match self {
            Os::Macos => "osx",
            Os::Linux => "linux",
        }
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
            "x86" | "x64" | "amd64" | "intel" => Some(Arch::X86),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Arch::Arm => "ARM",
            Arch::X86 => "x86",
        }
    }

    pub fn token(&self) -> &'static str {
        match self {
            Arch::Arm => "arm",
            Arch::X86 => "x",
        }
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

    pub fn token(&self) -> &'static str {
        match self {
            Bit::Bit32 => "32",
            Bit::Bit64 => "64",
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
            let opts = bits.iter().map(|b| b.label()).collect::<Vec<_>>().join(" or ");
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
        format!("{} {} {}", self.os.label(), self.arch.label(), self.bit.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_combinations_have_correct_tokens() {
        assert_eq!(Platform::from_parts(Os::Macos, Arch::Arm, Bit::Bit64).unwrap().token(), "osx-arm64");
        assert_eq!(Platform::from_parts(Os::Macos, Arch::X86, Bit::Bit64).unwrap().token(), "osx-x64");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit64).unwrap().token(), "linux-arm64");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32).unwrap().token(), "linux-arm32");
        assert_eq!(Platform::from_parts(Os::Linux, Arch::X86, Bit::Bit64).unwrap().token(), "linux-x64");
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
        assert_eq!(p.asset_filename("7.6.4"), "powershell-7.6.4-osx-arm64.tar.gz");
        let p = Platform::from_parts(Os::Linux, Arch::Arm, Bit::Bit32).unwrap();
        assert_eq!(p.asset_filename("7.6.4"), "powershell-7.6.4-linux-arm32.tar.gz");
    }

    #[test]
    fn available_bits_reflects_real_assets() {
        assert_eq!(Platform::available_bits(Os::Linux, Arch::Arm), vec![Bit::Bit64, Bit::Bit32]);
        assert_eq!(Platform::available_bits(Os::Linux, Arch::X86), vec![Bit::Bit64]);
        assert_eq!(Platform::available_bits(Os::Macos, Arch::Arm), vec![Bit::Bit64]);
        assert_eq!(Platform::available_bits(Os::Macos, Arch::X86), vec![Bit::Bit64]);
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
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test pattern::tests -v`
Expected: PASS (all 6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/pattern.rs
git commit -m "feat: platform pattern mapping for PowerShell release assets"
```

---

### Task 3: CLI argument parsing (`src/cli.rs`)

**Files:**
- Modify: `src/cli.rs`

**Interfaces:**
- Consumes: `pattern::{Os, Arch, Bit}`.
- Produces:
  - `#[derive(Debug, Clone, Default)] pub struct Args { pub os: Option<Os>, pub arch: Option<Arch>, pub bit: Option<Bit>, pub force: bool, pub skip_atomic: bool }`
  - `pub enum CliOutcome { Run(Args), Help }`
  - `pub fn parse(args: &[String]) -> Result<CliOutcome, String>`
  - `pub fn usage() -> String`

- [ ] **Step 1: Write the failing tests** (append `#[cfg(test)] mod tests`)

```rust
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
            s("prog"), s("--os"), s("linux"), s("--arch"), s("arm"),
            s("--bit"), s("32"), s("--force"), s("--skip-atomic"),
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
        assert!(matches!(parse(&vec![s("prog"), s("--help")]), Ok(CliOutcome::Help)));
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test cli::tests -v`
Expected: FAIL (types do not exist).

- [ ] **Step 3: Write the implementation**

```rust
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
                out.os = Some(Os::parse(v).ok_or_else(|| format!("invalid --os value '{v}' (expected macos|linux)"))?);
            }
            "--arch" => {
                let v = it.next().ok_or("--arch requires a value (arm|x86)")?;
                out.arch = Some(Arch::parse(v).ok_or_else(|| format!("invalid --arch value '{v}' (expected arm|x86)"))?);
            }
            "--bit" => {
                let v = it.next().ok_or("--bit requires a value (32|64)")?;
                out.bit = Some(Bit::parse(v).ok_or_else(|| format!("invalid --bit value '{v}' (expected 32|64)"))?);
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
         OPTIONS:\n    --os <macos|linux>      Operating system (default: prompt)\n    --arch <arm|x86>        CPU architecture (default: prompt)\n    --bit <32|64>           Bit type (default: prompt or auto)\n    --force                 Reinstall pwsh even if already working\n    --skip-atomic           Stop after pwsh is verified; skip Atomic Red Team\n    -h, --help              Print this help text\n",
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
            s("prog"), s("--os"), s("linux"), s("--arch"), s("arm"),
            s("--bit"), s("32"), s("--force"), s("--skip-atomic"),
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
        assert!(matches!(parse(&vec![s("prog"), s("--help")]), Ok(CliOutcome::Help)));
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test cli::tests -v`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat: CLI argument parsing with usage text"
```

---

### Task 4: Interactive prompts (`src/prompts.rs`)

**Files:**
- Modify: `src/prompts.rs`

**Interfaces:**
- Consumes: `pattern::{Os, Arch, Bit, Platform}`, `cli::Args`.
- Produces: `pub fn resolve_platform(args: &Args) -> Result<Platform, anyhow::Error>`. If a part is missing from `args`, prompts for it. Bit is auto-set to 64 when only one option exists (macOS any arch, or Linux x86); otherwise prompts 64/32.

- [ ] **Step 1: Write the implementation**

```rust
use anyhow::Context;
use std::io::{self, Write};

use crate::cli::Args;
use crate::pattern::{Arch, Bit, Os, Platform};

fn read_input(prompt: &str) -> Result<String, anyhow::Error> {
    print!("{prompt}");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut line = String::new();
    let n = io::stdin().read_line(&mut line).context("failed to read stdin")?;
    if n == 0 {
        anyhow::bail!("stdin closed; aborting");
    }
    Ok(line.trim().to_string())
}

fn ask_choice(question: &str, options: &[(&str, &str)]) -> Result<usize, anyhow::Error> {
    println!("{question}");
    for (i, (label, desc)) in options.iter().enumerate() {
        println!("  {}: {} ({})", i + 1, label, desc);
    }
    loop {
        let input = read_input(&format!("Enter choice [1-{}]: ", options.len()))?;
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                return Ok(n - 1);
            }
        }
        eprintln!(
            "[WARN] invalid choice '{input}'; enter a number from 1 to {}",
            options.len()
        );
    }
}

fn prompt_os() -> Result<Os, anyhow::Error> {
    let idx = ask_choice(
        "Select the operating system:",
        &[
            ("macOS", "Apple Silicon or Intel Mac"),
            ("Linux", "Debian/Ubuntu/RHEL-family distro"),
        ],
    )?;
    Ok(if idx == 0 { Os::Macos } else { Os::Linux })
}

fn prompt_arch() -> Result<Arch, anyhow::Error> {
    let idx = ask_choice(
        "Select the CPU architecture:",
        &[
            ("ARM", "Apple Silicon / AArch64"),
            ("x86", "Intel / AMD 64-bit"),
        ],
    )?;
    Ok(if idx == 0 { Arch::Arm } else { Arch::X86 })
}

fn prompt_bit(os: Os, arch: Arch) -> Result<Bit, anyhow::Error> {
    let bits = Platform::available_bits(os, arch);
    if bits.len() == 1 {
        let bit = bits[0];
        println!(
            "[INFO] only {} is available for {} {}; using it",
            bit.label(),
            os.label(),
            arch.label()
        );
        return Ok(bit);
    }
    let idx = ask_choice(
        "Select the bit type:",
        &[
            ("64-bit", "Recommended"),
            ("32-bit", "Legacy or limited hardware only"),
        ],
    )?;
    Ok(if idx == 0 { Bit::Bit64 } else { Bit::Bit32 })
}

pub fn resolve_platform(args: &Args) -> Result<Platform, anyhow::Error> {
    let os = match args.os {
        Some(o) => o,
        None => prompt_os()?,
    };
    let arch = match args.arch {
        Some(a) => a,
        None => prompt_arch()?,
    };
    let bit = match args.bit {
        Some(b) => b,
        None => prompt_bit(os, arch)?,
    };
    Platform::from_parts(os, arch, bit).map_err(anyhow::Error::msg)
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/prompts.rs
git commit -m "feat: interactive OS/arch/bit prompts"
```

---

### Task 5: GitHub release API (`src/github.rs`)

**Files:**
- Modify: `src/github.rs`

**Interfaces:**
- Consumes: nothing external (network at runtime; tests are offline).
- Produces:
  - `#[derive(Debug, Clone)] pub struct Asset { pub name: String, pub url: String }`
  - `#[derive(Debug, Clone)] pub struct Release { pub tag: String, pub version: String, pub assets: Vec<Asset> }`
  - `pub fn fetch_latest_release() -> Result<Release, anyhow::Error>`
  - `pub fn parse_release_json(body: &str) -> Result<Release, anyhow::Error>`
  - `pub fn find_asset<'a>(assets: &'a [Asset], filename: &str) -> Option<&'a Asset>`
  - `pub fn parse_hashes(bytes: &[u8]) -> Result<std::collections::HashMap<String, String>, anyhow::Error>`

- [ ] **Step 1: Write the failing tests** (append `#[cfg(test)] mod tests`)

```rust
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
        assert_eq!(release.assets[0].url, "https://x/powershell-7.6.4-osx-arm64.tar.gz");
    }

    #[test]
    fn find_asset_requires_exact_name_match() {
        let assets = vec![
            Asset { name: "powershell-7.6.4-linux-musl-x64.tar.gz".to_string(), url: "u1".to_string() },
            Asset { name: "powershell-7.6.4-linux-x64.tar.gz".to_string(), url: "u2".to_string() },
        ];
        assert_eq!(find_asset(&assets, "powershell-7.6.4-linux-x64.tar.gz").map(|a| a.url.as_str()), Some("u2"));
        assert_eq!(find_asset(&assets, "powershell-7.6.4-linux-arm64.tar.gz"), None);
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test github::tests -v`
Expected: FAIL (functions do not exist).

- [ ] **Step 3: Write the implementation**

```rust
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

#[derive(Debug, Clone)]
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
        assert_eq!(release.assets[0].url, "https://x/powershell-7.6.4-osx-arm64.tar.gz");
    }

    #[test]
    fn find_asset_requires_exact_name_match() {
        let assets = vec![
            Asset { name: "powershell-7.6.4-linux-musl-x64.tar.gz".to_string(), url: "u1".to_string() },
            Asset { name: "powershell-7.6.4-linux-x64.tar.gz".to_string(), url: "u2".to_string() },
        ];
        assert_eq!(find_asset(&assets, "powershell-7.6.4-linux-x64.tar.gz").map(|a| a.url.as_str()), Some("u2"));
        assert_eq!(find_asset(&assets, "powershell-7.6.4-linux-arm64.tar.gz"), None);
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test github::tests -v`
Expected: PASS (all 4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/github.rs
git commit -m "feat: GitHub latest-release API client and hashes.sha256 parser"
```

---

### Task 6: Download, checksum, extraction (`src/download.rs`)

**Files:**
- Modify: `src/download.rs`

**Interfaces:**
- Consumes: nothing external (network at runtime; tests use local fixtures).
- Produces:
  - `pub fn download_to_file(url: &str, dest: &Path, label: &str) -> Result<(), anyhow::Error>` — streams to `dest`, prints `\r` progress when `Content-Length` is known.
  - `pub fn download_bytes(url: &str, label: &str) -> Result<Vec<u8>, anyhow::Error>`
  - `pub fn sha256_of_file(path: &Path) -> Result<String, anyhow::Error>` — lowercase hex.
  - `pub fn verify_checksum(file: &Path, expected_hex: &str) -> Result<(), anyhow::Error>`
  - `pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), anyhow::Error>`

- [ ] **Step 1: Write the failing tests** (append `#[cfg(test)] mod tests`)

```rust
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
        assert_eq!(std::fs::read(out_dir.join("pwsh")).unwrap(), b"pwsh-binary-bytes");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test download::tests -v`
Expected: FAIL (functions do not exist).

- [ ] **Step 3: Write the implementation**

```rust
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::Context;
use sha2::{Digest, Sha256};

const USER_AGENT: &str = concat!("atomic-installer/", env!("CARGO_PKG_VERSION"));

/// Downloads a URL to a file, printing a progress line when the size is known.
pub fn download_to_file(url: &str, dest: &Path, label: &str) -> Result<(), anyhow::Error> {
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
pub fn download_bytes(url: &str, label: &str) -> Result<Vec<u8>, anyhow::Error> {
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
    let mut file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
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
        assert_eq!(std::fs::read(out_dir.join("pwsh")).unwrap(), b"pwsh-binary-bytes");
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test download::tests -v`
Expected: PASS (all 3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/download.rs
git commit -m "feat: download with progress, sha256 verify, and tar.gz extraction"
```

---

### Task 7: Filesystem helpers (`src/fsutil.rs`)

**Files:**
- Modify: `src/fsutil.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub fn ensure_dir(path: &Path) -> Result<(), anyhow::Error>`
  - `pub fn clear_dir_contents(path: &Path) -> Result<(), anyhow::Error>`
  - `pub fn create_or_replace_symlink(target: &Path, link: &Path) -> Result<(), anyhow::Error>` — replaces an existing or dangling symlink; errors if a regular file/dir occupies `link`.

- [ ] **Step 1: Write the failing tests** (append `#[cfg(test)] mod tests`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_dir_creates_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b/c");
        ensure_dir(&sub).unwrap();
        assert!(sub.is_dir());
    }

    #[test]
    fn ensure_dir_rejects_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        assert!(ensure_dir(&f).is_err());
    }

    #[test]
    fn clear_dir_contents_empties_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f"), b"x").unwrap();
        clear_dir_contents(dir.path()).unwrap();
        assert!(!sub.exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn symlink_created_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn symlink_refuses_regular_file_blocker() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let blocker = dir.path().join("link");
        std::fs::write(&target, b"x").unwrap();
        std::fs::write(&blocker, b"block").unwrap();
        assert!(create_or_replace_symlink(&target, &blocker).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test fsutil::tests -v`
Expected: FAIL (functions do not exist).

- [ ] **Step 3: Write the implementation**

```rust
use std::path::Path;

use anyhow::Context;

pub fn ensure_dir(path: &Path) -> Result<(), anyhow::Error> {
    if path.exists() {
        if !path.is_dir() {
            anyhow::bail!("{} exists but is not a directory", path.display());
        }
        return Ok(());
    }
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;
    println!("[INFO] created directory {}", path.display());
    Ok(())
}

pub fn clear_dir_contents(path: &Path) -> Result<(), anyhow::Error> {
    if !path.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
    {
        let entry = entry.context("failed to read a directory entry")?;
        let p = entry.path();
        if p.is_dir() {
            std::fs::remove_dir_all(&p)
                .with_context(|| format!("failed to remove {}", p.display()))?;
        } else {
            std::fs::remove_file(&p)
                .with_context(|| format!("failed to remove {}", p.display()))?;
        }
    }
    Ok(())
}

pub fn create_or_replace_symlink(target: &Path, link: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(link) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                std::fs::remove_file(link).with_context(|| {
                    format!("failed to remove existing symlink {}", link.display())
                })?;
                println!("[INFO] replaced existing symlink {}", link.display());
            } else {
                anyhow::bail!(
                    "{} already exists and is not a symlink; remove it manually before retrying",
                    link.display()
                );
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", link.display()));
        }
    }
    std::os::unix::fs::symlink(target, link).with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            link.display(),
            target.display()
        )
    })?;
    println!(
        "[INFO] created symlink {} -> {}",
        link.display(),
        target.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_dir_creates_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b/c");
        ensure_dir(&sub).unwrap();
        assert!(sub.is_dir());
    }

    #[test]
    fn ensure_dir_rejects_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        assert!(ensure_dir(&f).is_err());
    }

    #[test]
    fn clear_dir_contents_empties_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f"), b"x").unwrap();
        clear_dir_contents(dir.path()).unwrap();
        assert!(!sub.exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn symlink_created_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        create_or_replace_symlink(&target, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
    }

    #[test]
    fn symlink_refuses_regular_file_blocker() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let blocker = dir.path().join("link");
        std::fs::write(&target, b"x").unwrap();
        std::fs::write(&blocker, b"block").unwrap();
        assert!(create_or_replace_symlink(&target, &blocker).is_err());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test fsutil::tests -v`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/fsutil.rs
git commit -m "feat: directory and symlink helpers"
```

---

### Task 8: Atomic Red Team step (`src/atomic.rs`)

**Files:**
- Modify: `src/atomic.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `#[derive(Debug)] pub struct AtomicExit(pub i32)` implementing `Display` and `std::error::Error`.
  - `pub fn run_atomic_install(pwsh: &Path) -> Result<(), anyhow::Error>` — spawns `pwsh -NoProfile -Command "<IEX ...; Install-AtomicRedTeam>"` with inherited stdio. On non-zero exit, returns `AtomicExit(code)`.

- [ ] **Step 1: Write the implementation**

```rust
use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::Context;

const INSTALL_COMMAND: &str = "IEX (IWR 'https://raw.githubusercontent.com/redcanaryco/invoke-atomicredteam/master/install-atomicredteam.ps1' -UseBasicParsing); Install-AtomicRedTeam";

#[derive(Debug)]
pub struct AtomicExit(pub i32);

impl std::fmt::Display for AtomicExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Install-AtomicRedTeam exited with code {}", self.0)
    }
}

impl std::error::Error for AtomicExit {}

pub fn run_atomic_install(pwsh: &Path) -> Result<(), anyhow::Error> {
    println!(
        "[INFO] running the Atomic Red Team installer via {}",
        pwsh.display()
    );
    let status = Command::new(pwsh)
        .arg("-NoProfile")
        .arg("-Command")
        .arg(INSTALL_COMMAND)
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/atomic.rs
git commit -m "feat: run the Atomic Red Team installer via pwsh"
```

---

### Task 9: Orchestration (`src/main.rs`)

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `cli::{Args, CliOutcome, parse, usage}`, `prompts::resolve_platform`, `pattern::Platform`, `github::{fetch_latest_release, find_asset, parse_hashes}`, `download::{download_to_file, download_bytes, verify_checksum, extract_tar_gz}`, `fsutil::{ensure_dir, clear_dir_contents, create_or_replace_symlink}`, `atomic::{run_atomic_install, AtomicExit}`.
- Produces: the executable program.

- [ ] **Step 1: Write the implementation**

```rust
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
    println!("[INFO] installs PowerShell from the latest GitHub release, links pwsh, and installs Atomic Red Team");
    println!();

    let home = std::env::var("HOME").context("HOME environment variable is not set")?;
    let home = PathBuf::from(home);
    let install_dir = home.join(".local/share/powershell");
    let bin_dir = home.join(".local/bin");
    let pwsh_link = bin_dir.join("pwsh");
    let pwsh_binary = install_dir.join("pwsh");

    let platform = prompts::resolve_platform(&args)?;
    println!();
    println!("[INFO] selected platform: {}", platform.describe());
    println!(
        "[INFO] release asset pattern: powershell-<version>-{}.tar.gz",
        platform.token()
    );

    if !args.force && pwsh_works(&pwsh_link)? {
        println!(
            "[INFO] pwsh is already installed and working; skipping the PowerShell install"
        );
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
        atomic::run_atomic_install(&pwsh_link)?;
    }

    println!();
    println!("=== summary ===");
    println!("[INFO] PowerShell binary: {}", pwsh_binary.display());
    println!("[INFO] pwsh link: {}", pwsh_link.display());
    println!("[INFO] Atomic Red Team: run 'Invoke-AtomicTest <technique-id>' from pwsh to use it");
    Ok(())
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
```

- [ ] **Step 2: Build and run all tests**

Run: `cargo build && cargo test`
Expected: build PASS, all unit tests PASS.

- [ ] **Step 3: Check help and usage-error exits**

Run: `cargo run -- --help` then `cargo run -- --bogus; echo "exit=$?"`
Expected: help prints and exits 0; bogus flag prints an error and usage, exits 2.

- [ ] **Step 4: Check formatting and clippy**

Run: `cargo fmt --check && cargo clippy -- -D warnings`
Expected: PASS (fix any warnings, then re-run).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: orchestrate install flow in main"
```

---

### Task 10: End-to-end verification on the current machine

**Files:**
- None (manual verification).

**Interfaces:**
- Consumes: the completed binary.

- [ ] **Step 1: Run the program with explicit flags, skipping the Atomic step first**

Run: `cargo run -- --os macos --arch arm --bit 64 --skip-atomic`
Expected: prompts skipped (all flags given), pwsh installs, checksum verifies, symlink created, version printed, summary shown, exit 0.

- [ ] **Step 2: Confirm the symlink and binary**

Run: `ls -l ~/.local/bin/pwsh` and `~/.local/bin/pwsh -v`
Expected: symlink points at `~/.local/share/powershell/pwsh`; version prints.

- [ ] **Step 3: Run again to confirm skip-if-installed**

Run: `cargo run -- --os macos --arch arm --bit 64 --skip-atomic`
Expected: reports the existing pwsh version and skips the download/install.

- [ ] **Step 4: Run the full flow including Atomic Red Team**

Run: `cargo run -- --os macos --arch arm --bit 64`
Expected: PowerShell step is skipped (already installed), then the Atomic Red Team installer runs interactively; answer any prompts; it finishes and the summary prints.

- [ ] **Step 5: Verify Atomic Red Team is usable**

Run: `~/.local/bin/pwsh -NoProfile -Command "Import-Module ~/atomic-red-team/atomics/Invoke-AtomicRedTeam/Invoke-AtomicRedTeam.psd1 -Force; Get-Command Invoke-AtomicTest | Select-Object -ExpandProperty Name"`
Expected: prints `Invoke-AtomicTest`.
