# Interactive Prompt Detection and Arrow-Key Menu Design

Date: 2026-08-13

## Purpose

Enhance the interactive platform prompts in `atomic-installer`:

1. Detect the host OS and CPU architecture and pre-select the detected option in
   the prompts (user still confirms with Enter).
2. Replace the numbered-choice input loop with an arrow-key selection menu.

Both changes apply only to the interactive prompts. CLI flags (`--os`, `--arch`,
`--bit`) keep taking precedence and are unaffected.

## Detection

Reuse the existing `Os::parse` / `Arch::parse` aliases — `std::env::consts::OS`
yields exactly `"macos"`/`"linux"` and `std::env::consts::ARCH` yields
`"aarch64"`/`"x86_64"`, all already accepted by the parsers.

- `Os::detect() -> Option<Os>` — `Os::parse(std::env::consts::OS)`.
- `Arch::detect() -> Option<Arch>` — `Arch::parse(std::env::consts::ARCH)`.

Unsupported hosts (e.g. `windows`, `riscv64`) return `None`.

Each prompt computes its default index from detection:

- OS detected -> print `[INFO] detected host OS: macOS` and pre-select macOS.
- Arch detected -> print `[INFO] detected host architecture: ARM` and pre-select
  ARM.
- Undetected -> no `[INFO]` line and no pre-selection (default stays the first
  option, as today).

Optional, included: when the bit prompt offers more than one option (`linux-arm`
only), pre-select by pointer width (`size_of::<usize>() == 8` -> 64-bit) and
print `[INFO] detected host bitness: 64-bit` when detected.

## Arrow-key menu

New dependency: `crossterm` (the only added crate). Output stays plain text — no
colors, no emojis — preserving the output contract.

`ask_choice` is replaced by `select(question, options, default_idx) -> Result<usize>`:

- **TTY path** (`stdin().is_terminal()`): enter raw mode, render the question and
  the option list with `>` marking the selected line, redrawing lines in place
  (crossterm `cursor`/`terminal`) on each key press. Keys:
  - `Up`/`Down` — move the selection, wrapping at the ends.
  - `Enter` — confirm, print `[INFO] selected: <label>`.
  - `Ctrl+C` — restore the terminal and abort (`bail!("aborted by user")`,
    exit code 1). In raw mode Ctrl+C is delivered as a key event, so it must be
    handled explicitly.
  - A digit key — jump to that option (accepted as a shortcut, not advertised;
    the primary interaction is arrows).
  - The hint line uses plain ASCII: `(up/down to move, Enter to select)`.
- **Non-TTY fallback** (piped stdin / CI): keep the existing numbered input loop,
  now hinting the default (`Enter choice [1-2, default 1]`).

The terminal is restored via a drop-guard even on error and abort paths.

The pure index-stepping logic (move up/down/wrap, key-to-action mapping) is
factored into a testable function; only the thin terminal IO layer stays
untested, matching the project's convention for IO/network code.

## Files touched

- `Cargo.toml` — add `crossterm`.
- `src/pattern.rs` — `Os::detect`, `Arch::detect`.
- `src/prompts.rs` — `select` menu, detection-based defaults, `[INFO] selected`
  echo.
- `src/cli.rs` — usage text: `--os <macos|linux> Operating system (default:
  detect or prompt)` (and likewise `--arch`).

## Testing

- `pattern.rs`: detection mapping tests via raw strings (target-independent; no
  assertions on `std::env::consts`).
- `prompts.rs`: selection-index stepping tests (wrapping, bounds, digit jump).
- Gate: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`.

## Non-goals

- No change to CLI flag semantics, exit codes, or the execution flow.
- No auto-accept of a detected platform; the user always confirms with Enter.
