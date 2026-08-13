# Detection and Arrow-Key Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pre-select the host OS/arch (detected from `std::env::consts`) in the interactive prompts and replace the numbered-choice loop with an arrow-key selection menu.

**Architecture:** Two small pure additions to `pattern.rs` (`Os::detect`, `Arch::detect`) plus a rewrite of `prompts.rs`: a `select(question, options, default)` function that renders an in-place arrow-key menu via crossterm raw mode on a TTY, falling back to the existing numbered input loop for piped stdin. The pure menu-stepping logic is factored into a testable module.

**Tech Stack:** Rust (edition 2024), `crossterm = "0.29"` (only new dependency; plain text output, no colors/emojis).

## Global Constraints

- No emojis anywhere in program output. The menu cursor is the ASCII `>`; the hint line is ASCII: `(up/down to move, Enter to select)`.
- All program output uses `[INFO]`, `[WARN]`, or `[ERROR]` prefixes. New prefix lines: `[INFO] detected host OS: ...`, `[INFO] detected host architecture: ...`, `[INFO] detected host bitness: ...`, `[INFO] selected: <label>`.
- Exit codes unchanged: `0` success, `1` runtime failure (Ctrl+C abort and stdin-close both exit 1), `2` CLI usage error.
- CLI flags still take precedence over prompts; detection only affects the prompts that run when a flag is omitted.
- Detection reuses existing parsers: `std::env::consts::OS` is `"macos"`/`"linux"`; `std::env::consts::ARCH` is `"aarch64"`/`"x86_64"`. `Arch::parse` must additionally accept `"x86_64"` (currently missing).
- Unsupported hosts (e.g. `windows`, `riscv64`) yield no detection and no `[INFO] detected` line; the default selection stays the first option.
- Detection only pre-selects; the user always confirms with Enter (or arrows + Enter). No auto-accept.
- Bit prompt: when multiple bit options exist (`linux-arm` only), pre-select by pointer width (`size_of::<usize>() == 8` -> 64-bit).
- Digit keys jump to an option as a non-advertised shortcut.
- Verification gate (run at the end of every task and again in the final task): `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.
- Dev machine: macOS arm64. Manual interactive smoke of the menu is optional; `cargo test` covers the pure logic.

---

### Task 1: Host detection in `pattern.rs`

**Files:**
- Modify: `src/pattern.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `Os::detect() -> Option<Os>`, `Arch::detect() -> Option<Arch>`; `Arch::parse` now also accepts `"x86_64"`.

- [ ] **Step 1: Add `"x86_64"` to `Arch::parse`**

In `src/pattern.rs`, change the `Arch::parse` match arm for x86:

```rust
            "x86" | "x64" | "amd64" | "x86_64" | "intel" => Some(Arch::X86),
```

- [ ] **Step 2: Add the detect methods**

In `impl Os`, after `label()`:

```rust
    /// The host OS from `std::env::consts`, if supported.
    pub fn detect() -> Option<Os> {
        Os::parse(std::env::consts::OS)
    }
```

In `impl Arch`, after `label()`:

```rust
    /// The host CPU architecture from `std::env::consts`, if supported.
    pub fn detect() -> Option<Arch> {
        Arch::parse(std::env::consts::ARCH)
    }
```

- [ ] **Step 3: Add the tests**

Append to the existing `#[cfg(test)] mod tests` block in `src/pattern.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests**

Run: `cargo test pattern::tests`
Expected: PASS (all 22 existing + 3 new tests).

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all clean; tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/pattern.rs
git commit -m "feat: detect host OS and architecture"
```

---

### Task 2: Arrow-key selection menu in `prompts.rs`

**Files:**
- Modify: `Cargo.toml`
- Rewrite: `src/prompts.rs`

**Interfaces:**
- Consumes: `Os::detect`, `Arch::detect` (wired in Task 3, not yet used here).
- Produces: `fn select(question: &str, options: &[(&str, &str)], default: Option<usize>) -> Result<usize, anyhow::Error>`. `select` uses the arrow-key menu on a TTY and the numbered loop otherwise, and echoes `[INFO] selected: <label>`.

- [ ] **Step 1: Add the crossterm dependency**

In `Cargo.toml`, add to `[dependencies]`:

```toml
crossterm = "0.29"
```

- [ ] **Step 2: Run `cargo test` to fetch the dependency**

Run: `cargo test --no-run`
Expected: crossterm resolves and the crate builds (behavior unchanged so far).

- [ ] **Step 3: Rewrite `src/prompts.rs`**

Replace the entire file with:

```rust
use anyhow::Context;
use crossterm::cursor::MoveUp;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, KeyEvent};
use crossterm::execute;
use crossterm::terminal::{self, Clear, ClearType};
use std::io::{self, IsTerminal, Write};

use crate::cli::Args;
use crate::pattern::{Arch, Bit, Os, Platform};

fn read_input(prompt: &str) -> Result<String, anyhow::Error> {
    print!("{prompt}");
    io::stdout().flush().context("failed to flush stdout")?;
    let mut line = String::new();
    let n = io::stdin()
        .read_line(&mut line)
        .context("failed to read stdin")?;
    if n == 0 {
        anyhow::bail!("stdin closed; aborting");
    }
    Ok(line.trim().to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuOutcome {
    Move,
    Confirm,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAction {
    Up,
    Down,
    Confirm,
    Abort,
    Jump(usize),
}

#[derive(Debug)]
struct MenuState {
    cursor: usize,
    len: usize,
}

impl MenuState {
    fn new_at(len: usize, default: Option<usize>) -> Option<MenuState> {
        if len == 0 {
            return None;
        }
        let cursor = default.filter(|i| *i < len).unwrap_or(0);
        Some(MenuState { cursor, len })
    }

    fn move_up(&mut self) {
        self.cursor = if self.cursor == 0 { self.len - 1 } else { self.cursor - 1 };
    }

    fn move_down(&mut self) {
        self.cursor = (self.cursor + 1) % self.len;
    }

    fn jump(&mut self, index: usize) {
        if index < self.len {
            self.cursor = index;
        }
    }
}

fn classify_key(event: KeyEvent) -> Option<MenuAction> {
    match (event.code, event.modifiers) {
        (KeyCode::Up, _) => Some(MenuAction::Up),
        (KeyCode::Down, _) => Some(MenuAction::Down),
        (KeyCode::Enter, _) => Some(MenuAction::Confirm),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(MenuAction::Abort),
        (KeyCode::Char(c), KeyModifiers::NONE) if c.is_ascii_digit() => {
            Some(MenuAction::Jump((c as u8 - b'1') as usize))
        }
        _ => None,
    }
}

fn apply(state: &mut MenuState, action: MenuAction) -> MenuOutcome {
    match action {
        MenuAction::Up => {
            state.move_up();
            MenuOutcome::Move
        }
        MenuAction::Down => {
            state.move_down();
            MenuOutcome::Move
        }
        MenuAction::Jump(i) => {
            state.jump(i);
            MenuOutcome::Move
        }
        MenuAction::Confirm => MenuOutcome::Confirm,
        MenuAction::Abort => MenuOutcome::Abort,
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self, anyhow::Error> {
        terminal::enable_raw_mode().context("failed to enter raw mode")?;
        Ok(RawModeGuard)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

const HINT: &str = "(up/down to move, Enter to select)";

fn arrow_menu(
    question: &str,
    options: &[(&str, &str)],
    state: &mut MenuState,
) -> Result<usize, anyhow::Error> {
    let _guard = RawModeGuard::enter()?;
    let block_lines = options.len() + 1;

    let mut stdout = io::stdout();
    write!(stdout, "{question}\r\n")?;
    for (i, (label, desc)) in options.iter().enumerate() {
        let marker = if i == state.cursor { ">" } else { " " };
        write!(stdout, "{marker} {label:<7} {desc}\r\n")?;
    }
    write!(stdout, "{HINT}\r\n")?;
    stdout.flush().context("failed to flush stdout")?;

    loop {
        let ev = event::read().context("failed to read key event")?;
        let action = match ev {
            Event::Key(key) => classify_key(key),
            _ => None,
        };
        let outcome = action.map_or(MenuOutcome::Move, |a| apply(state, a));
        match outcome {
            MenuOutcome::Confirm => return Ok(state.cursor),
            MenuOutcome::Abort => anyhow::bail!("aborted by user"),
            MenuOutcome::Move => {
                execute!(stdout, MoveUp(block_lines as u16))
                    .context("failed to reposition the cursor")?;
                for (i, (label, desc)) in options.iter().enumerate() {
                    let marker = if i == state.cursor { ">" } else { " " };
                    write!(
                        stdout,
                        "{}\r{marker} {label:<7} {desc}\r\n",
                        Clear(ClearType::CurrentLine)
                    )?;
                }
                write!(stdout, "{}\r{HINT}\r\n", Clear(ClearType::CurrentLine))?;
                stdout.flush().context("failed to flush stdout")?;
            }
        }
    }
}

fn numbered_menu(
    question: &str,
    options: &[(&str, &str)],
    state: &MenuState,
) -> Result<usize, anyhow::Error> {
    println!("{question}");
    for (i, (label, desc)) in options.iter().enumerate() {
        println!("  {}: {} ({})", i + 1, label, desc);
    }
    loop {
        let input = read_input(&format!(
            "Enter choice [1-{}, default {}]: ",
            options.len(),
            state.cursor + 1
        ))?;
        if input.is_empty() {
            return Ok(state.cursor);
        }
        if let Ok(n) = input.parse::<usize>()
            && n >= 1
            && n <= options.len()
        {
            return Ok(n - 1);
        }
        eprintln!(
            "[WARN] invalid choice '{input}'; enter a number from 1 to {}",
            options.len()
        );
    }
}

fn select(
    question: &str,
    options: &[(&str, &str)],
    default: Option<usize>,
) -> Result<usize, anyhow::Error> {
    let mut state = MenuState::new_at(options.len(), default)
        .ok_or_else(|| anyhow::anyhow!("select() requires at least one option"))?;
    let idx = if io::stdin().is_terminal() {
        arrow_menu(question, options, &mut state)?
    } else {
        numbered_menu(question, options, &state)?
    };
    println!("[INFO] selected: {}", options[idx].0);
    Ok(idx)
}

fn prompt_os() -> Result<Os, anyhow::Error> {
    let idx = select(
        "Select the operating system:",
        &[
            ("macOS", "Apple Silicon or Intel Mac"),
            ("Linux", "Debian/Ubuntu/RHEL-family distro"),
        ],
        None,
    )?;
    Ok(if idx == 0 { Os::Macos } else { Os::Linux })
}

fn prompt_arch() -> Result<Arch, anyhow::Error> {
    let idx = select(
        "Select the CPU architecture:",
        &[
            ("ARM", "Apple Silicon / AArch64"),
            ("x86", "Intel / AMD 64-bit"),
        ],
        None,
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
    let idx = select(
        "Select the bit type:",
        &[
            ("64-bit", "Recommended"),
            ("32-bit", "Legacy or limited hardware only"),
        ],
        None,
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

Note: `resolve_platform` is unchanged from the current file.

- [ ] **Step 4: Add the stepper tests**

Append to `src/prompts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn menu_state_wraps_at_top() {
        let mut s = MenuState::new_at(3, Some(0)).unwrap();
        s.move_up();
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn menu_state_wraps_at_bottom() {
        let mut s = MenuState::new_at(3, Some(2)).unwrap();
        s.move_down();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn default_is_used_when_in_range() {
        let s = MenuState::new_at(2, Some(1)).unwrap();
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn out_of_range_default_falls_back_to_zero() {
        let s = MenuState::new_at(2, Some(9)).unwrap();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn jump_ignores_out_of_bounds() {
        let mut s = MenuState::new_at(3, Some(0)).unwrap();
        s.jump(5);
        assert_eq!(s.cursor, 0);
        s.jump(2);
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn classify_maps_keys() {
        assert_eq!(
            classify_key(key(KeyCode::Up)),
            Some(MenuAction::Up)
        );
        assert_eq!(
            classify_key(key(KeyCode::Down)),
            Some(MenuAction::Down)
        );
        assert_eq!(
            classify_key(key(KeyCode::Enter)),
            Some(MenuAction::Confirm)
        );
        assert_eq!(
            classify_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(MenuAction::Abort)
        );
        assert_eq!(
            classify_key(key(KeyCode::Char('2'))),
            Some(MenuAction::Jump(1))
        );
        assert_eq!(classify_key(key(KeyCode::Char('x'))), None);
    }

    #[test]
    fn apply_moves_confirms_and_aborts() {
        let mut s = MenuState::new_at(2, None).unwrap();
        assert_eq!(apply(&mut s, MenuAction::Down), MenuOutcome::Move);
        assert_eq!(s.cursor, 1);
        assert_eq!(apply(&mut s, MenuAction::Confirm), MenuOutcome::Confirm);
        assert_eq!(apply(&mut s, MenuAction::Abort), MenuOutcome::Abort);
    }
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test prompts::tests`
Expected: PASS (7 new stepper tests).

- [ ] **Step 6: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all clean; tests pass. If `cargo fmt --check` fails, run `cargo fmt` and re-verify.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/prompts.rs Cargo.lock
git commit -m "feat: arrow-key selection menu with numbered fallback"
```

---

### Task 3: Wire detection defaults into the prompts

**Files:**
- Modify: `src/prompts.rs`
- Modify: `src/cli.rs`

**Interfaces:**
- Consumes: `Os::detect`, `Arch::detect` from Task 1; `select` from Task 2.
- Produces: prompts that pre-select the detected host OS/arch/bitness and log `[INFO] detected ...` lines; updated usage text.

- [ ] **Step 1: Pre-select the OS**

Replace `prompt_os` in `src/prompts.rs`:

```rust
fn prompt_os() -> Result<Os, anyhow::Error> {
    let detected = Os::detect();
    if let Some(os) = detected {
        println!("[INFO] detected host OS: {}", os.label());
    }
    let default = detected.map(|os| if os == Os::Macos { 0 } else { 1 });
    let idx = select(
        "Select the operating system:",
        &[
            ("macOS", "Apple Silicon or Intel Mac"),
            ("Linux", "Debian/Ubuntu/RHEL-family distro"),
        ],
        default,
    )?;
    Ok(if idx == 0 { Os::Macos } else { Os::Linux })
}
```

- [ ] **Step 2: Pre-select the architecture**

Replace `prompt_arch` in `src/prompts.rs`:

```rust
fn prompt_arch() -> Result<Arch, anyhow::Error> {
    let detected = Arch::detect();
    if let Some(arch) = detected {
        println!("[INFO] detected host architecture: {}", arch.label());
    }
    let default = detected.map(|arch| if arch == Arch::Arm { 0 } else { 1 });
    let idx = select(
        "Select the CPU architecture:",
        &[
            ("ARM", "Apple Silicon / AArch64"),
            ("x86", "Intel / AMD 64-bit"),
        ],
        default,
    )?;
    Ok(if idx == 0 { Arch::Arm } else { Arch::X86 })
}
```

- [ ] **Step 3: Pre-select the bitness when more than one option**

Replace `prompt_bit` in `src/prompts.rs`:

```rust
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
    let is_64 = std::mem::size_of::<usize>() == 8;
    println!(
        "[INFO] detected host bitness: {}",
        if is_64 { "64-bit" } else { "32-bit" }
    );
    let default = if is_64 { Some(0) } else { Some(1) };
    let idx = select(
        "Select the bit type:",
        &[
            ("64-bit", "Recommended"),
            ("32-bit", "Legacy or limited hardware only"),
        ],
        default,
    )?;
    Ok(if idx == 0 { Bit::Bit64 } else { Bit::Bit32 })
}
```

- [ ] **Step 4: Update the usage text**

In `src/cli.rs`, inside `usage()`, replace the three option lines:

```rust
    --os <macos|linux>      Operating system (default: detect or prompt)\n    --arch <arm|x86>        CPU architecture (default: detect or prompt)\n    --bit <32|64>           Bit type (default: detect or prompt)\n
```

- [ ] **Step 5: Run the gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all clean; tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/prompts.rs src/cli.rs
git commit -m "feat: pre-select detected host platform in prompts"
```

---

### Task 4: Final verification gate

**Files:**
- None (verification only).

- [ ] **Step 1: Run the full verification gate**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: fmt clean, clippy clean with `-D warnings`, all 29+ tests pass.

- [ ] **Step 2: Sanity-check the non-TTY fallback (no network, no `~/.local` mutation)**

Run: `printf '1\n1\n' | cargo run -- --bit 32`
Expected: with `--bit 32` set, only the OS and arch prompts run; both are answered via the numbered fallback (with the `default N` hint), `[INFO] selected: macOS` and `[INFO] selected: ARM` appear, then `Platform::from_parts` rejects the invalid macOS/ARM/32-bit combo and the program exits 1 with an `[ERROR] invalid combination ...` message — before any download. Verify the exit code with `echo $?` (expected `1`). This exercises the fallback and the new `[INFO] selected` echo without touching the network or `~/.local`.

- [ ] **Step 3: Optional interactive smoke (TTY)**

Run: `cargo run -- --skip-atomic`
Expected: an arrow-key menu renders for OS and arch with `>` on the detected `macOS` / `ARM`; Up/Down move the selection; Enter confirms; Ctrl+C aborts with `[ERROR] aborted by user` and exit 1. If not running on a real TTY, skip this step.
