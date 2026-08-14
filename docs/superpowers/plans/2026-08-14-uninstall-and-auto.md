# Uninstall and Auto-Platform Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `--uninstall`/`-u` (remove pwsh + Atomic Red Team) and `--auto`/`-a` (host-detected non-interactive platform selection) to `atomic-installer`.

**Architecture:** `--auto` generalizes `resolve_platform` with per-axis precedence *explicit flag > `detect()` > prompt*, using a new pure bit helper in `pattern.rs`. `--uninstall` is an early branch in `real_main` that orchestrates removal via two new testable `fsutil` helpers and a `prompts::confirm` y/N prompt; the flow is injected with the confirm closure so it is testable offline.

**Tech Stack:** Rust 2024, anyhow, crossterm (existing), no new dependencies. Tests are `#[cfg(test)] mod tests` inside each module.

## Global Constraints

- Rust edition 2024; **no new dependencies**.
- Output contract: `[INFO]`/`[WARN]`/`[ERROR]` prefixes only; **no emojis anywhere**.
- Exit codes: `0` success, `1` runtime error or aborted uninstall, `2` CLI usage error.
- Verification gate (run from repo root): `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
- Per-task quick test runs use `cargo test <module>`, but the full gate MUST pass before the plan is considered done.
- `--auto` precedence per axis: explicit `--os`/`--arch`/`--bit` flag > `detect()` > interactive prompt. Undetectable axes fall back to their normal prompt.
- `--uninstall` ignores all other flags; it always confirms and always exits 1 on decline (empty input is a decline).
- Uninstall scope: remove `~/.local/share/powershell` (includes Atomic Red Team modules) and the `~/.local/bin/pwsh` symlink. Removal helpers must treat a dangling symlink as present (use `symlink_metadata`, never `Path::exists()`).

---

### Task 1: `pattern.rs` — bit helpers for `--auto`

**Files:**
- Modify: `src/pattern.rs` (add two methods inside `impl Platform`, add tests to the existing `mod tests`)

**Interfaces:**
- Produces:
  - `Platform::bit_for_pointer_width(os: Os, arch: Arch, is_64: bool) -> Bit`
  - `Platform::auto_bit(os: Os, arch: Arch) -> Bit`

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` block in `src/pattern.rs`:

```rust
    #[test]
    fn bit_for_pointer_width_single_option_wins() {
        assert_eq!(
            Platform::bit_for_pointer_width(Os::Macos, Arch::Arm, false),
            Bit::Bit64
        );
        assert_eq!(
            Platform::bit_for_pointer_width(Os::Macos, Arch::X86, false),
            Bit::Bit64
        );
        assert_eq!(
            Platform::bit_for_pointer_width(Os::Linux, Arch::X86, false),
            Bit::Bit64
        );
    }

    #[test]
    fn bit_for_pointer_width_linux_arm_follows_pointer_width() {
        assert_eq!(
            Platform::bit_for_pointer_width(Os::Linux, Arch::Arm, true),
            Bit::Bit64
        );
        assert_eq!(
            Platform::bit_for_pointer_width(Os::Linux, Arch::Arm, false),
            Bit::Bit32
        );
    }

    #[test]
    fn auto_bit_matches_host_pointer_width() {
        for os in [Os::Macos, Os::Linux] {
            for arch in [Arch::Arm, Arch::X86] {
                assert_eq!(
                    Platform::auto_bit(os, arch),
                    Platform::bit_for_pointer_width(os, arch, std::mem::size_of::<usize>() == 8)
                );
            }
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test pattern`
Expected: FAIL with "no method named `bit_for_pointer_width`" (compilation error).

- [ ] **Step 3: Write minimal implementation**

Add to `impl Platform` in `src/pattern.rs`, after `available_bits`:

```rust
    /// The bit type `--auto` selects given the host pointer width. A single
    /// available bit always wins; otherwise the pointer width decides, falling
    /// back to 64-bit where 32-bit is not an available asset.
    pub fn bit_for_pointer_width(os: Os, arch: Arch, is_64: bool) -> Bit {
        let bits = Self::available_bits(os, arch);
        if bits.len() == 1 {
            return bits[0];
        }
        if is_64 {
            Bit::Bit64
        } else {
            Bit::Bit32
        }
    }

    /// The bit type `--auto` selects on the current host.
    pub fn auto_bit(os: Os, arch: Arch) -> Bit {
        Self::bit_for_pointer_width(os, arch, std::mem::size_of::<usize>() == 8)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test pattern`
Expected: PASS (all existing + new).

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/pattern.rs
git commit -m "feat: add auto bit selection helpers to pattern"
```

---

### Task 2: `fsutil.rs` — removal helpers

**Files:**
- Modify: `src/fsutil.rs` (two new pub functions + tests in the existing `mod tests`)

**Interfaces:**
- Consumes: `anyhow::Context` (already imported).
- Produces:
  - `fsutil::remove_if_symlink(path: &Path) -> Result<(), anyhow::Error>`
  - `fsutil::remove_dir_if_exists(path: &Path) -> Result<(), anyhow::Error>`

Behavior (from spec): `remove_if_symlink` removes only an actual symlink and prints `[INFO] removed symlink <path>`; a non-symlink existing entry prints `[WARN] <path> exists and is not a symlink; leaving it alone`; a missing path is a silent no-op. `remove_dir_if_exists` mirrors this for directories (`[INFO] removed directory <path>`, `[WARN] <path> exists and is not a directory; leaving it alone`). Both use `symlink_metadata` so dangling symlinks are seen as present.

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` block in `src/fsutil.rs`:

```rust
    #[test]
    fn remove_if_symlink_removes_symlink_only() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        remove_if_symlink(&link).unwrap();
        assert!(!link.exists());
        assert!(target.exists());
    }

    #[test]
    fn remove_if_symlink_removes_dangling_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path().join("gone"), &link).unwrap();
        remove_if_symlink(&link).unwrap();
        assert!(!link.exists());
    }

    #[test]
    fn remove_if_symlink_warns_on_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("file");
        std::fs::write(&f, b"x").unwrap();
        remove_if_symlink(&f).unwrap();
        assert!(f.exists());
    }

    #[test]
    fn remove_if_symlink_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        remove_if_symlink(&dir.path().join("missing")).unwrap();
    }

    #[test]
    fn remove_dir_if_exists_removes_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("f"), b"x").unwrap();
        remove_dir_if_exists(&sub).unwrap();
        assert!(!sub.exists());
    }

    #[test]
    fn remove_dir_if_exists_noop_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        remove_dir_if_exists(&dir.path().join("missing")).unwrap();
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test fsutil`
Expected: FAIL with "cannot find function `remove_if_symlink`" (compilation error).

- [ ] **Step 3: Write minimal implementation**

Append to `src/fsutil.rs` after `create_or_replace_symlink`:

```rust
pub fn remove_if_symlink(path: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            std::fs::remove_file(path)
                .with_context(|| format!("failed to remove symlink {}", path.display()))?;
            println!("[INFO] removed symlink {}", path.display());
        }
        Ok(_) => {
            println!(
                "[WARN] {} exists and is not a symlink; leaving it alone",
                path.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    Ok(())
}

pub fn remove_dir_if_exists(path: &Path) -> Result<(), anyhow::Error> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            std::fs::remove_dir_all(path)
                .with_context(|| format!("failed to remove directory {}", path.display()))?;
            println!("[INFO] removed directory {}", path.display());
        }
        Ok(_) => {
            println!(
                "[WARN] {} exists and is not a directory; leaving it alone",
                path.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("failed to inspect {}", path.display()));
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test fsutil`
Expected: PASS (all existing + new).

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/fsutil.rs
git commit -m "feat: add symlink and directory removal helpers"
```

---

### Task 3: `cli.rs` — `--auto` and `--uninstall` flags

**Files:**
- Modify: `src/cli.rs` (`Args`, `parse`, `usage`, tests)

**Interfaces:**
- Produces:
  - `Args.auto: bool`, `Args.uninstall: bool` (both default false via the existing `#[derive(Default)]`).
  - Parses `--auto`/`-a` and `--uninstall`/`-u` as valueless boolean flags.
- Consumes: existing `Args` shape (uninstall mode ignores os/arch/bit/auto/force/skip_atomic; `main.rs` reads the flags).

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `src/cli.rs`:

```rust
    #[test]
    fn parses_auto_and_uninstall_flags() {
        let args = vec![s("prog"), s("--auto"), s("--uninstall")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert!(a.auto);
                assert!(a.uninstall);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parses_short_auto_and_uninstall_flags() {
        let args = vec![s("prog"), s("-a"), s("-u")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert!(a.auto);
                assert!(a.uninstall);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn auto_and_uninstall_are_off_by_default() {
        let args = vec![s("prog")];
        match parse(&args) {
            Ok(CliOutcome::Run(a)) => {
                assert!(!a.auto);
                assert!(!a.uninstall);
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn usage_lists_new_flags() {
        let usage = usage();
        assert!(usage.contains("--auto"));
        assert!(usage.contains("-a,"));
        assert!(usage.contains("--uninstall"));
        assert!(usage.contains("-u,"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test cli`
Expected: FAIL (tests reference `a.auto` / `a.uninstall` fields that do not exist).

- [ ] **Step 3: Write minimal implementation**

3a. Add the fields to `Args`:

```rust
pub struct Args {
    pub os: Option<Os>,
    pub arch: Option<Arch>,
    pub bit: Option<Bit>,
    pub auto: bool,
    pub uninstall: bool,
    pub force: bool,
    pub skip_atomic: bool,
}
```

3b. Add the two arms to the match in `parse` (before `other => ...`):

```rust
            "--auto" | "-a" => out.auto = true,
            "--uninstall" | "-u" => out.uninstall = true,
```

3c. Add the two lines to `usage()`, after the `--bit` line and keeping the `\n\` continuation style:

```rust
         \n\
    -a, --auto              Auto-select platform from host detection (falls back to prompts if undetectable)\n\
    -u, --uninstall         Uninstall pwsh and Atomic Red Team instead of installing\n\
    --force                 Reinstall pwsh even if already working\n\
```

3d. Update the existing `parses_all_flags` test to also pass `--auto` and `--uninstall` and assert them (add `s("--auto")`, `s("--uninstall")` to the args vector, and `assert!(a.auto); assert!(a.uninstall);` after `assert!(a.skip_atomic);`). Update `defaults_are_empty` to add `assert!(!a.auto); assert!(!a.uninstall);` after `assert!(!a.skip_atomic);`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test cli`
Expected: PASS.

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/cli.rs
git commit -m "feat: add --auto and --uninstall CLI flags"
```

---

### Task 4: `prompts.rs` — confirmation prompt and auto resolution

**Files:**
- Modify: `src/prompts.rs` (`parse_confirmation`, `confirm`, `resolve_platform` signature + auto branches, tests)
- Modify: `src/main.rs:59` (the `resolve_platform(&args)` call site — one line, so the crate compiles at the end of this task)

**Interfaces:**
- Consumes: `Platform::auto_bit` from Task 1; `Args` (now with `auto`) from Task 3.
- Produces:
  - `prompts::parse_confirmation(input: &str) -> bool`
  - `prompts::confirm(prompt: &str) -> Result<bool, anyhow::Error>`
  - `prompts::resolve_platform(args: &Args, auto: bool) -> Result<Platform, anyhow::Error>` (signature change — `main.rs` is updated in Task 5)

- [ ] **Step 1: Write the failing tests**

Add to the existing `mod tests` block in `src/prompts.rs`:

```rust
    #[test]
    fn confirmation_accepts_yes_variants() {
        assert!(parse_confirmation("y"));
        assert!(parse_confirmation("yes"));
        assert!(parse_confirmation("Y"));
        assert!(parse_confirmation("YES"));
        assert!(parse_confirmation(" Yes "));
    }

    #[test]
    fn confirmation_rejects_everything_else() {
        assert!(!parse_confirmation("n"));
        assert!(!parse_confirmation("no"));
        assert!(!parse_confirmation(""));
        assert!(!parse_confirmation(" "));
        assert!(!parse_confirmation("maybe"));
    }

    #[test]
    fn resolve_platform_explicit_flags_win_over_auto() {
        let args = Args {
            os: Some(Os::Linux),
            arch: Some(Arch::Arm),
            bit: Some(Bit::Bit32),
            auto: true,
            ..Default::default()
        };
        let p = resolve_platform(&args, true).unwrap();
        assert_eq!(p.os, Os::Linux);
        assert_eq!(p.arch, Arch::Arm);
        assert_eq!(p.bit, Bit::Bit32);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test prompts`
Expected: FAIL (`parse_confirmation` not found; `resolve_platform` takes one argument).

- [ ] **Step 3: Write minimal implementation**

3a. Add `confirm` and `parse_confirmation` near `read_input`:

```rust
pub fn parse_confirmation(input: &str) -> bool {
    matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

pub fn confirm(prompt: &str) -> Result<bool, anyhow::Error> {
    Ok(parse_confirmation(&read_input(prompt)?))
}
```

3b. Replace `resolve_platform` with the auto-aware version:

```rust
pub fn resolve_platform(args: &Args, auto: bool) -> Result<Platform, anyhow::Error> {
    let os = match args.os {
        Some(o) => o,
        None if auto => match Os::detect() {
            Some(o) => {
                println!("[INFO] auto: using detected host OS: {}", o.label());
                o
            }
            None => prompt_os()?,
        },
        None => prompt_os()?,
    };
    let arch = match args.arch {
        Some(a) => a,
        None if auto => match Arch::detect() {
            Some(a) => {
                println!(
                    "[INFO] auto: using detected host architecture: {}",
                    a.label()
                );
                a
            }
            None => prompt_arch()?,
        },
        None => prompt_arch()?,
    };
    let bit = match args.bit {
        Some(b) => b,
        None if auto => {
            let b = Platform::auto_bit(os, arch);
            println!("[INFO] auto: using detected host bitness: {}", b.label());
            b
        }
        None => prompt_bit(os, arch)?,
    };
    Platform::from_parts(os, arch, bit).map_err(anyhow::Error::msg)
}
```

3c. Update the `resolve_platform` call site in `src/main.rs` (so the crate compiles with the new signature):

```rust
    let platform = prompts::resolve_platform(&args, args.auto)?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test prompts`
Expected: PASS.

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/prompts.rs src/main.rs
git commit -m "feat: add confirmation prompt and auto platform resolution"
```

---

### Task 5: `main.rs` — uninstall branch and flow

**Files:**
- Modify: `src/main.rs` (`real_main` uninstall branch, new `uninstall` / `uninstall_with_ask` functions, tests)

**Interfaces:**
- Consumes: `fsutil::remove_if_symlink`, `fsutil::remove_dir_if_exists` (Task 2); `prompts::confirm`, `prompts::resolve_platform(args, auto)` (Task 4).
- Produces: `uninstall` flow (exit 0 on success/empty, exit 1 on decline via `anyhow::bail!("uninstall aborted")`).

- [ ] **Step 1: Write the failing tests**

Add a new `#[cfg(test)] mod tests` block at the end of `src/main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

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
        std::fs::write(install_dir.join("pwsh"), b"binary").unwrap();
        std::os::unix::fs::symlink(install_dir.join("pwsh"), &pwsh_link).unwrap();
        uninstall_with_ask(&install_dir, &pwsh_link, |_| Ok(true)).unwrap();
        assert!(!install_dir.exists());
        assert!(std::fs::symlink_metadata(&pwsh_link).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test main`
Expected: FAIL ("cannot find function `uninstall_with_ask`").

- [ ] **Step 3: Write minimal implementation**

3a. In `real_main`, add the uninstall branch right before the platform resolution (after the `let install_dir` / `let pwsh_link` path bindings, so the branch can use them):

```rust
    if args.uninstall {
        return uninstall(&install_dir, &pwsh_link);
    }
```

3b. Add the uninstall functions after `pwsh_works`:

```rust
fn uninstall(install_dir: &Path, pwsh_link: &Path) -> Result<(), anyhow::Error> {
    uninstall_with_ask(install_dir, pwsh_link, |prompt| prompts::confirm(prompt))
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS (all modules, including the three new `main` tests).

- [ ] **Step 5: Run the full verification gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs
git commit -m "feat: add --uninstall flow and wire --auto into platform resolution"
```

---

### Task 6: Documentation — usage examples and final review

**Files:**
- Modify: `README.md` (only if it documents the CLI flags; otherwise skip)
- Verify: spec `docs/superpowers/specs/2026-08-14-uninstall-and-auto-design.md` is fully implemented

**Interfaces:**
- None.

- [ ] **Step 1: Check whether README documents flags**

Run: `grep -n -- "--skip-atomic\|--force" README.md`
If README documents flags, add the two new flags with the same style. If README has no flag docs, skip this task's README edits and continue.

- [ ] **Step 2: Spec coverage check**

Skim `docs/superpowers/specs/2026-08-14-uninstall-and-auto-design.md` and confirm every "Testing" bullet maps to a test added in Tasks 1-5:
- `bit_for_pointer_width` both pointer widths -> Task 1
- `parse_confirmation` variants -> Task 4
- `remove_if_symlink` / `remove_dir_if_exists` -> Task 2
- uninstall flow (nothing-installed, declined, full removal) -> Task 5
- `--auto`/`-a`/`--uninstall`/`-u` parsing -> Task 3

- [ ] **Step 3: Manual smoke test (non-destructive)**

Run each of these (they do NOT mutate `~/.local`):

```sh
cargo run -- --help
cargo run -- --uninstall
```

Expected: `--help` shows the two new flags; `--uninstall` with nothing installed prints `[WARN] nothing to uninstall` and exits 0. A `--uninstall` with a real installation would prompt y/N (do NOT run against a real installation on a dev machine without confirmation).

- [ ] **Step 4: Run the full verification gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document --auto and --uninstall flags"
```

(If README was unchanged, run `git status --porcelain` and confirm nothing is pending instead of committing.)
