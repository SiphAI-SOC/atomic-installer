# atomic-installer Design

Date: 2026-08-13

## Purpose

A single-binary Rust CLI that:

1. Installs the latest PowerShell release from GitHub to `~/.local/share/powershell`.
2. Symlinks `pwsh` to `~/.local/bin/pwsh`.
3. Installs Atomic Red Team via the official `install-atomicredteam.ps1` script.

Outputs detailed process information to the user at every step. No emojis.

## Platform selection

Interactive prompts (OS -> arch -> bit), overridable with CLI flags. The user's
selection maps to a platform token:

| OS    | arch | bit  | token         |
|-------|------|------|---------------|
| macos | arm  | 64   | `osx-arm64`   |
| macos | x86  | 64   | `osx-x64`     |
| linux | arm  | 64   | `linux-arm64` |
| linux | arm  | 32   | `linux-arm32` |
| linux | x86  | 64   | `linux-x64`   |

Invalid combinations (32-bit on macOS, 32-bit x86 on either OS) are rejected with
an explanation.

Asset name = `powershell-{version}-{token}.tar.gz`. Matching is exact, which
naturally excludes variants such as `musl` and `fxdependent`.

## CLI

```
atomic-installer [--os macos|linux] [--arch arm|x86] [--bit 32|64]
                 [--force] [--skip-atomic] [--help]
```

- `--os`, `--arch`, `--bit`: skip the corresponding interactive prompt.
- `--force`: reinstall even if pwsh already works.
- `--skip-atomic`: stop after pwsh is verified.
- `--help`: usage text.

Invalid flag values are a usage error (exit code 2).

## Modules

- `main.rs` - phase orchestration, banners, summary, exit codes.
- `cli.rs` - argument parsing.
- `prompts.rs` - interactive selection with validation and re-prompt.
- `pattern.rs` - OS/arch/bit -> token mapping and asset filename construction (pure).
- `github.rs` - latest-release API call; picks the tar.gz and `hashes.sha256` assets.
- `download.rs` - download to a temp file with progress, SHA256 verification, extraction.
- `fsutil.rs` - ensure-dir and symlink create/replace logic.
- `atomic.rs` - runs the pwsh install command and streams output.

## Execution flow

1. Banner. Resolve platform selection (flags or prompts). Print the selection and
   the computed asset filename pattern.
2. Skip check: if `~/.local/bin/pwsh` exists and the version check succeeds, and
   `--force` is not set, skip the install phases and report the installed version.
   The version check runs `pwsh -v` and requires exit code 0 with non-empty output.
3. Fetch the latest release metadata from the GitHub API.
4. Download the matching tar.gz to a temp file (with progress) and the
   `hashes.sha256` file.
5. Verify the SHA256 checksum. On mismatch: abort, delete the temp file, error out.
6. Ensure `~/.local/share/powershell` exists. If `--force`, clear existing
   contents. Extract the tarball into it.
7. Ensure `~/.local/bin` exists. Create or replace the symlink
   `~/.local/bin/pwsh -> ~/.local/share/powershell/pwsh`. Replace an existing or
   dangling symlink; refuse with an error if a regular file or directory blocks the
   path.
8. Verify pwsh works using the same version check (`pwsh -v`) through the new
   symlink.
9. Run the Atomic Red Team install command interactively (stdin/stdout/stderr
   inherited so any installer prompts can be answered):

   ```
   IEX (IWR 'https://raw.githubusercontent.com/redcanaryco/invoke-atomicredteam/master/install-atomicredteam.ps1' -UseBasicParsing); Install-AtomicRedTeam
   ```

   The exit code is propagated.
10. Print a success summary: versions, paths, next steps.

## Error handling and output

- Every phase logs what it is about to do and what it did.
- Messages use `[INFO]`, `[WARN]`, `[ERROR]` prefixes. No emojis.
- Errors carry actionable messages and suggestions.
- Temp files are cleaned up on failure.
- Exit codes: `0` success, `1` runtime failure, `2` CLI usage error.

## Dependencies

- `ureq` - HTTP client (rustls TLS).
- `flate2` + `tar` - tarball extraction.
- `serde` + `serde_json` - GitHub API parsing.
- `sha2` - checksum verification.
- `anyhow` - error handling.

## Testing

- Unit: `pattern.rs` valid/invalid combinations.
- Unit: `hashes.sha256` parsing.
- Unit: asset matching.
- Unit: extraction and symlink helpers against a generated tar.gz fixture in a
  temp directory.
- Manual e2e: a real run on the development machine.
