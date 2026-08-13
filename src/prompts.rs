use anyhow::Context;
use std::io::{self, Write};

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

fn ask_choice(question: &str, options: &[(&str, &str)]) -> Result<usize, anyhow::Error> {
    println!("{question}");
    for (i, (label, desc)) in options.iter().enumerate() {
        println!("  {}: {} ({})", i + 1, label, desc);
    }
    loop {
        let input = read_input(&format!("Enter choice [1-{}]: ", options.len()))?;
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
