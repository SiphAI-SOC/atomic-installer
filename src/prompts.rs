use anyhow::Context;
use crossterm::cursor::MoveUp;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
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
        self.cursor = if self.cursor == 0 {
            self.len - 1
        } else {
            self.cursor - 1
        };
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
        (KeyCode::Char(c), KeyModifiers::NONE) if ('1'..='9').contains(&c) => {
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
        let outcome = match action {
            Some(a) => apply(state, a),
            None => continue,
        };
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
        assert_eq!(classify_key(key(KeyCode::Up)), Some(MenuAction::Up));
        assert_eq!(classify_key(key(KeyCode::Down)), Some(MenuAction::Down));
        assert_eq!(classify_key(key(KeyCode::Enter)), Some(MenuAction::Confirm));
        assert_eq!(
            classify_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(MenuAction::Abort)
        );
        assert_eq!(
            classify_key(key(KeyCode::Char('2'))),
            Some(MenuAction::Jump(1))
        );
        assert_eq!(classify_key(key(KeyCode::Char('0'))), None);
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
}
