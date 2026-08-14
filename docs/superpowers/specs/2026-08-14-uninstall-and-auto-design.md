# Uninstall and Auto-Platform Design

Date: 2026-08-14

## Purpose

Extend `atomic-installer` with two features:

1. `--uninstall` (`-u`): remove the PowerShell installation and the Atomic Red
   Team modules, restoring the machine to its pre-install state.
2. `--auto` (`-a`): select the platform non-interactively from host detection,
   falling back to the interactive prompts when an axis cannot be detected.

Both features preserve the existing exit-code semantics (0 success, 1 runtime,
2 usage), the `[INFO]`/`[WARN]`/`[ERROR]` output contract, and the no-emojis rule.

## CLI

New flags, both valueless booleans:

```
atomic-installer [--os macos|linux] [--arch arm|x86] [--bit 32|64]
                 [--auto | -a] [--uninstall | -u]
                 [--force] [--skip-atomic] [--help]
```

- `--auto` / `-a`: auto-select the platform from host detection. Per axis,
  precedence is *explicit flag > `detect()` > interactive prompt*. An axis that
  cannot be detected falls back to its normal prompt; a successful auto-detection
  prints `[INFO] auto: using detected host OS: macOS` (likewise architecture and
  bitness).
- `--uninstall` / `-u`: uninstall mode. Runs the uninstall flow instead of the
  install flow. All other flags (`--os`, `--arch`, `--bit`, `--auto`, `--force`,
  `--skip-atomic`) are parsed but ignored in uninstall mode.

## `--auto` resolution

`resolve_platform(args, auto)` resolves each axis in order:

- `os`: `args.os` -> `Os::detect()` (if `auto`) -> `prompt_os()`
- `arch`: `args.arch` -> `Arch::detect()` (if `auto`) -> `prompt_arch()`
- `bit`: `args.bit` -> `Platform::auto_bit(os, arch)` (if `auto`) -> `prompt_bit(os, arch)`

Bit selection under `--auto` uses a new pure helper in `pattern.rs`:

- `Platform::bit_for_pointer_width(os, arch, is_64) -> Bit` — if
  `available_bits(os, arch)` has a single option, return it; otherwise return
  `Bit64` when `is_64`, else `Bit32` (valid only for `linux-arm`; any other
  combination falls back to `Bit64`).
- `Platform::auto_bit(os, arch) -> Bit` — delegates to
  `bit_for_pointer_width(os, arch, size_of::<usize>() == 8)`.

Splitting the pointer-width-dependent decision from the caller keeps the 32-bit
branch testable on a 64-bit development machine.

## `--uninstall` flow

`real_main` branches to the uninstall flow immediately after the banner when
`args.uninstall` is set. No platform resolution and no network calls occur.

`uninstall(install_dir, pwsh_link, ask)` — the third parameter is a closure
`Fn(&str) -> Result<bool, anyhow::Error>` used for confirmation (production:
`prompts::confirm`; tests inject a stub). Flow:

1. If neither `install_dir` nor `pwsh_link` exists: print
   `[WARN] nothing to uninstall`, return success (exit 0).
2. Print `[INFO] this will remove:` followed by both paths, then prompt
   `Remove these files and directories? [y/N]`. A decline (including empty input)
   is reported as `[ERROR] uninstall aborted` and returns a runtime error
   (exit 1), consistent with the existing "aborted by user" behavior.
3. Remove `pwsh_link` if it is a symlink. If it exists but is not a symlink,
   print `[WARN] <path> exists and is not a symlink; leaving it alone`.
4. Remove `install_dir` if it exists. This also removes the Atomic Red Team
   modules under `~/.local/share/powershell/Modules`.
5. Print `[INFO] uninstalled PowerShell and Atomic Red Team`, return success.

Removing the whole `~/.local/share/powershell` directory is the intended scope:
Atomic Red Team's user modules live inside it, so pwsh and ART are removed
together.

## Modules and helpers

- `src/cli.rs` — `Args.auto`, `Args.uninstall`; parse `--auto`/`-a` and
  `--uninstall`/`-u`; usage text for both.
- `src/pattern.rs` — `Platform::bit_for_pointer_width`, `Platform::auto_bit`.
- `src/prompts.rs` — `resolve_platform(args, auto)`; `confirm(prompt)` and the
  pure `parse_confirmation(&str) -> bool` (`y`/`yes`, case-insensitive, else
  false).
- `src/fsutil.rs` — `remove_if_symlink(path)` (removes only an actual symlink,
  warns and skips anything else); `remove_dir_if_exists(path)`.
- `src/main.rs` — uninstall branch and `uninstall` orchestration; pass `auto`
  into `resolve_platform`.

## Error handling and output

- Every removal step logs what it will remove and what it removed.
- Messages use the existing `[INFO]`, `[WARN]`, `[ERROR]` prefixes; no emojis.
- Confirmation reads from stdin and requires an explicit `y`/`yes`; empty input
  is a decline.
- Exit codes unchanged: `0` success, `1` runtime error or aborted uninstall,
  `2` CLI usage error.

## Testing

- `cli.rs`: `--auto`/`-a`/`--uninstall`/`-u` parsing, flag combos, defaults.
- `pattern.rs`: `bit_for_pointer_width` for both pointer widths across all
  (os, arch) combinations.
- `prompts.rs`: `parse_confirmation` for `y`/`yes`/`Y`/`YES`/`n`/`no`/empty.
- `fsutil.rs`: `remove_if_symlink` (removes a symlink, warns on a regular-file
  blocker), `remove_dir_if_exists` (removes a directory, no-op when missing) —
  against temp dirs.
- `main.rs`: `uninstall` flow with a stub `ask` closure — nothing-installed
  path, declined path, and full-removal path — against temp dirs.
- Gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Non-goals

- No change to the install flow, asset matching, exit codes, or output contract.
- No separate "keep Atomic Red Team" uninstall mode.
- No new dependencies.
- `--uninstall` is always confirmed; `--force` does not bypass it.
