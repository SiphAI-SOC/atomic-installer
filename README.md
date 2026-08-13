# atomic-installer

A single-binary Rust CLI that installs the latest PowerShell release from
GitHub, symlinks it as `pwsh`, and runs the Atomic Red Team installer.

The tool is aimed at security practitioners and lab setups that want a fresh
PowerShell with Atomic Red Team on macOS or Linux without touching the system
package manager.

## What it does

1. Fetches the **latest PowerShell release** from
   `github.com/PowerShell/PowerShell/releases/latest`.
2. Selects the release asset for your platform
   (`powershell-<version>-<os>-<arch>.tar.gz`) with an **exact name match**.
3. Downloads the archive and its `hashes.sha256`, **verifies the SHA-256**
   checksum before anything is extracted.
4. Extracts into `~/.local/share/powershell` and symlinks
   `~/.local/bin/pwsh` -> `~/.local/share/powershell/pwsh`.
5. Verifies the install by running `pwsh -v`.
6. Unless `--skip-atomic` is given, runs the **Atomic Red Team installer**
   (Invoke-AtomicRedTeam) via the freshly installed `pwsh`.

Nothing is installed into system directories and no package manager is used.
The install is fully user-local under `~/.local`.

## Requirements

- macOS or Linux (64-bit ARM or x86).
- Rust toolchain 1.85 or newer (the crate uses edition 2024).
- Outbound network access to `api.github.com` and `github.com`.
- `~/.local/bin` on `PATH` if you want to call `pwsh` by name.
- PowerShell is **not** required beforehand; the tool installs it.

## Build

```sh
cargo build --release -j 10 # Build from current OS arch 
cargo build --release -j 10 --target aarch64-unknown-linux-gnu # Build for specific OS arch

```

## Install

### Option A: `cargo install` from the git remote (recommended)

`cargo install` builds a release binary from source and places it in
`~/.cargo/bin` (a directory already managed by Cargo and typically on your
`PATH`), so there is no need to manually copy the binary or keep the source
tree around.

```sh
cargo install --git https://github.com/SiphAI-SOC/atomic-installer.git
```

That fetches the latest `main` and builds the `atomic-installer` binary. To pin
a specific version or commit instead of tracking the default branch:

```sh
# Pin a published version
cargo install atomic-installer --version 0.1.0

# Pin an exact commit (sha or tag) from the git remote
cargo install --git https://github.com/SiphAI-SOC/atomic-installer.git --rev <commit-sha>
cargo install --git https://github.com/SiphAI-SOC/atomic-installer.git --tag <tag-name>
```

Notes:

- The crate is **not** currently published on `crates.io`, so plain
  `cargo install atomic-installer` fails with a "could not find crate" error.
  Until it is published, use the `--git` form above (the `--version` example
  only works after publishing).
- The binary lands at `~/.cargo/bin/atomic-installer`. Make sure
  `~/.cargo/bin` is on your `PATH`; otherwise call it by full path.
- Cargo compiles with the Rust toolchain currently active in your shell. See
  [Requirements](#requirements) for the minimum version.

### Option B: `cargo install` from a local checkout

If you already have the repository cloned, install from the local path (fast
for iteration, and works fully offline once dependencies are cached):

```sh
git clone https://github.com/SiphAI-SOC/atomic-installer.git
cd atomic-installer
cargo install --path .
```

Use `--force` to rebuild and replace an existing `atomic-installer` binary:

```sh
cargo install --path . --force
```

### Option C: manual copy of a release build

Build yourself and copy the binary wherever you like (here, `~/.local/bin`,
which is the same directory the tool itself manages for `pwsh`):

```sh
cargo build --release
install -m 0755 target/release/atomic-installer ~/.local/bin/
```

Then ensure `~/.local/bin` is on your `PATH`.

### Uninstalling

```sh
# Remove the cargo-managed binary (Options A and B)
cargo uninstall atomic-installer

# Or simply delete the binary if you placed it manually (Option C)
rm ~/.local/bin/atomic-installer
```

Uninstalling `atomic-installer` does **not** remove the PowerShell install,
the `pwsh` symlink, or Atomic Red Team from your system — those live under
`~/.local` and are left untouched.

## Usage

```
atomic-installer [OPTIONS]
```

Run without flags to choose the platform interactively, or pass flags for
non-interactive use:

| Flag | Value | Description |
| --- | --- | --- |
| `--os` | `macos` or `linux` | Operating system (aliases accepted: `mac`, `darwin`, `osx`) |
| `--arch` | `arm` or `x86` | CPU architecture (aliases: `aarch64`, `arm64`, `x64`, `amd64`, `intel`) |
| `--bit` | `32` or `64` | Bit type (aliases: `32-bit`, `64bit`, `i386`, `i686`, ...) |
| `--force` | | Reinstall pwsh even if an existing install passes the version check |
| `--skip-atomic` | | Stop after pwsh is verified; do not run the Atomic Red Team installer |
| `-h`, `--help` | | Print help and exit |

In interactive mode the bit type is auto-selected when only one is available
(e.g. macOS is 64-bit only). Invalid platform combinations are rejected with a
clear error.

### Examples

```sh
# Interactive: prompts for OS, architecture, and bit
atomic-installer

# Non-interactive macOS install, skipping the Atomic Red Team step
atomic-installer --os macos --arch arm --bit 64 --skip-atomic

# Full flow for a Linux x86-64 host
atomic-installer --os linux --arch x86 --bit 64

# Reinstall pwsh even if it already works
atomic-installer --os macos --arch arm --bit 64 --force
```

### Supported platforms

| Platform | Release asset |
| --- | --- |
| macOS Apple Silicon | `powershell-<version>-osx-arm64.tar.gz` |
| macOS Intel | `powershell-<version>-osx-x64.tar.gz` |
| Linux ARM64 | `powershell-<version>-linux-arm64.tar.gz` |
| Linux ARM32 | `powershell-<version>-linux-arm32.tar.gz` |
| Linux x86-64 | `powershell-<version>-linux-x64.tar.gz` |

Deliberately rejected: 32-bit macOS (none shipped) and 32-bit x86 Linux
(none shipped). Near-miss assets such as `linux-musl-x64` are **not** matched
against `linux-x64`; asset selection is exact.

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success |
| `1` | Runtime error (network, checksum mismatch, missing asset, ...) |
| `2` | CLI usage error |
| *other* | A non-zero exit from the Atomic Red Team installer is propagated verbatim |

### Output conventions

All messages use `[INFO]`, `[WARN]`, or `[ERROR]` prefixes. The program output
contains no emoji.

## Behavior notes

- **Skip if installed**: if `~/.local/bin/pwsh` exists and `pwsh -v` exits 0
  with non-empty stdout, the PowerShell step is skipped. Use `--force` to
  reinstall.
- **`--force`**: clears the existing contents of `~/.local/share/powershell`
  before extracting. Without `--force`, files are only ever added, never
  removed.
- The downloaded `.tar.gz` is written to a temp file and deleted after
  extraction (best effort).
- The Atomic Red Team installer is invoked as
  `IEX (IWR 'https://raw.githubusercontent.com/redcanaryco/invoke-atomicredteam/master/install-atomicredteam.ps1' -UseBasicParsing); Install-AtomicRedTeam`,
  so it requires network access at runtime. Skippable with `--skip-atomic`.

## How it works

```
main
 ├─ cli::parse          flag parsing / usage / exit 2 on bad args
 ├─ prompts::resolve_platform   interactive fallback for --os/--arch/--bit
 ├─ pattern             platform -> release asset token (e.g. osx-arm64)
 ├─ github              latest release API + hashes.sha256 parser
 ├─ download            streaming download, sha256 verify, tar.gz extract
 ├─ fsutil              directory + symlink helpers
 └─ atomic              spawns pwsh -NoProfile -Command <installer>
```

Module responsibilities:

- `pattern` — platform enum + valid-combination matrix + asset filename
  generation.
- `cli` — argument parsing and help text.
- `prompts` — interactive OS/arch/bit selection.
- `github` — GitHub Releases API client and the `hashes.sha256` parser.
- `download` — HTTP download, SHA-256 verification, tar.gz extraction.
- `fsutil` — `ensure_dir`, `clear_dir_contents`, symlink (re)creation.
- `atomic` — runs the Atomic Red Team installer, propagating its exit code.
- `main` — orchestration, output, and exit-code mapping.

## Security

- The PowerShell archive is verified against the official `hashes.sha256`
  shipped with the release **before** extraction.
- `hashes.sha256` is parsed defensively: it is UTF-16LE with a BOM in
  practice (UTF-8 is also accepted), and the `*` binary-mode marker in each
  line is stripped before the lookup key is used.
- Files are written only under `~/.local`; no `sudo`, no system paths.

## Development

```sh
# Verification gate (run all three, in this order)
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

- All unit tests are offline `#[cfg(test)] mod tests` blocks inside each
  `src/` module. Network-bound functions (`download_to_file`,
  `download_bytes`, `run_atomic_install`) intentionally have no unit tests.
- The design spec and implementation plan (with tracked decisions) live in
  `docs/superpowers/`; they are authoritative for execution flow and exit-code
  semantics.
