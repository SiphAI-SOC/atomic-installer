# `-getAtomics` Support Design

Date: 2026-08-14

## Purpose

`Install-AtomicRedTeam` currently runs without `-getAtomics`, so only the
Invoke-AtomicRedTeam module is installed and the atomics repository is not
downloaded. `Invoke-AtomicTest <technique-id>` is therefore unusable after a
default install. This design makes `-getAtomics` the default and adds an
opt-out flag for users who want only the module.

Preserves the existing exit-code semantics (0 success, 1 runtime, 2 usage), the
`[INFO]`/`[WARN]`/`[ERROR]` output contract, and the no-emojis rule.

## CLI

New valueless boolean flag, long form only (no short alias, matching
`--force`/`--skip-atomic`):

```
atomic-installer [--os macos|linux] [--arch arm|x86] [--bit 32|64]
                 [--auto | -a] [--uninstall | -u]
                 [--force] [--skip-atomic] [--no-atomics-repo] [--help]
```

- `--no-atomics-repo`: skip downloading the atomics repository; install the
  module only. Default (flag absent) passes `-getAtomics`.
- Interaction rules, matching existing flag precedent: parsed but ignored in
  `--uninstall` mode; irrelevant when `--skip-atomic` short-circuits before the
  Atomic Red Team phase.

## Command construction

`src/atomic.rs` replaces the constant command with a pure, testable builder:

- `install_command(get_atomics: bool) -> String` — returns
  `IEX (IWR 'https://raw.githubusercontent.com/redcanaryco/invoke-atomicredteam/master/install-atomicredteam.ps1' -UseBasicParsing); Install-AtomicRedTeam`
  plus a trailing ` -getAtomics` when `get_atomics` is true.
- `run_atomic_install(pwsh: &Path, get_atomics: bool)` uses the builder and
  prints an `[INFO]` line naming the mode before spawning pwsh:
  - `[INFO] installing with the atomics repository (-getAtomics)`
  - `[INFO] installing without the atomics repository (--no-atomics-repo)`
- Exit-code propagation (`AtomicExit`), stdin/stdout/stderr inheritance, and
  stream handling are unchanged.

## Orchestration and summary

`src/main.rs`:

- Call site: `atomic::run_atomic_install(&pwsh_link, !args.no_atomics_repo)?;`
- New pure helper `atomic_summary_line(skip: bool, no_atomics_repo: bool) -> String`:
  - default: `Atomic Red Team: run 'Invoke-AtomicTest <technique-id>' from pwsh to use it` (unchanged)
  - `--no-atomics-repo`: `Atomic Red Team module installed without the atomics repository; run 'Invoke-AtomicRedTeam -getAtomics' from pwsh to download it later`
  - `--skip-atomic`: `Atomic Red Team: skipped (--skip-atomic)` — replaces the
    pre-existing misleading hint that atomics are available when the install
    was skipped.

## Testing

- `cli.rs`: parsing `--no-atomics-repo`; default is `false`; usage lists the flag.
- `atomic.rs` (new offline `#[cfg(test)]` mod): `install_command(true)` ends with
  `-getAtomics`, `install_command(false)` does not. Pure, so exempt from the
  no-unit-tests rule for network-bound functions.
- `main.rs`: `atomic_summary_line` across all three states.
- Gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Non-goals

- No change to pwsh install flow, asset matching, exit codes, or output contract.
- No `-getPrereqs` or other Install-AtomicRedTeam switches.
- No new dependencies.
