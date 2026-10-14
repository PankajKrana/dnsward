//! `fuck-ads logs`: journald on Linux, the log file on macOS.

use crate::context::Ctx;
use anyhow::{Context, Result, bail};
use fuck_ads_platform::LogSource;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

pub fn logs(ctx: &Ctx, follow: bool, lines: usize) -> Result<()> {
    let source = if ctx.paths.user_mode {
        LogSource::File(
            ctx.paths
                .log_file
                .clone()
                .context("no log file in user mode")?,
        )
    } else {
        fuck_ads_platform::current().log_source(&ctx.paths, follow, lines)
    };
    match source {
        LogSource::Command(program, args) => {
            let status = std::process::Command::new(&program)
                .args(&args)
                .status()
                .with_context(|| format!("cannot run {program}"))?;
            if !status.success() {
                bail!(
                    "{program} exited with {status} (you may need sudo or membership in the systemd-journal group)"
                );
            }
            Ok(())
        }
        LogSource::File(path) => tail_file(&path, lines, follow),
    }
}

/// Prints the last `lines` lines of a file and optionally follows it,
/// handling truncation/rotation.
fn tail_file(path: &Path, lines: usize, follow: bool) -> Result<()> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let len = file.metadata()?.len();
    // Read at most the last 4 MiB to find the requested lines.
    let start = len.saturating_sub(4 * 1024 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let all: Vec<&str> = text.lines().collect();
    let mut out = std::io::stdout().lock();
    for line in &all[all.len().saturating_sub(lines)..] {
        writeln!(out, "{line}")?;
    }
    out.flush()?;
    drop(out);
    if !follow {
        return Ok(());
    }
    let mut pos = len;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let current = match std::fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => continue, // rotated; wait for the new file
        };
        if current < pos {
            // Truncated or rotated: start over from the beginning.
            file = std::fs::File::open(path)?;
            pos = 0;
        }
        if current > pos {
            file.seek(SeekFrom::Start(pos))?;
            let mut chunk = Vec::new();
            file.read_to_end(&mut chunk)?;
            pos += chunk.len() as u64;
            std::io::stdout().write_all(&chunk)?;
            std::io::stdout().flush()?;
        }
    }
}
