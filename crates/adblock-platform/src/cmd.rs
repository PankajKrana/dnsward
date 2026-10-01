//! Running system tools safely: absolute paths, argument vectors (never a
//! shell), captured output.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Output, Stdio};

/// Returns the first existing path among `candidates`.
pub fn find_tool(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|p| Path::new(p).exists())
}

/// Runs a command and returns its output, failing on a non-zero exit.
pub fn run(program: &str, args: &[&str]) -> Result<Output> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .output()
        .with_context(|| format!("cannot execute {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed ({}): {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

/// Runs a command and returns trimmed stdout, failing on a non-zero exit.
pub fn stdout(program: &str, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8_lossy(&run(program, args)?.stdout)
        .trim()
        .to_string())
}

/// Runs a command and reports only whether it succeeded.
pub fn succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
