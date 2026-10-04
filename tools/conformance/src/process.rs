//! Runs workers as isolated child processes with time and memory limits and full output
//! capture.

use crate::model::{ProcessEvidence, RenderStats};
use anyhow::{Context, Result, anyhow};
use serde::de::DeserializeOwned;
use std::{
    fs,
    io::Read,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Prefix of the stderr lines workers print when entering a stage.
pub const STAGE_MARKER: &str = "conformance-stage: ";

/// Prefix of the stderr lines carrying a JSON [`RenderStats`] update from a page worker.
pub const STATS_MARKER: &str = "conformance-stats: ";

/// Keeps the tail of stderr; panics and backtraces end there.
const STDERR_LIMIT: usize = 64 * 1024;

/// Resident memory a worker may use by default, in MiB. A runaway read grows until its time
/// limit; several at once exhaust a CI runner, which then stops the whole job.
pub const DEFAULT_MEMORY_LIMIT_MIB: u64 = 3 * 1024;

/// What a worker may use before it is killed.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Wall time.
    pub time: Duration,
    /// Resident memory in bytes; `None` is unlimited. Only enforced where `/proc` exists.
    pub memory: Option<u64>,
}

impl Limits {
    /// Returns limits of `time` and `memory_mib` MiB of resident memory, 0 meaning unlimited.
    pub fn new(time: Duration, memory_mib: u64) -> Self {
        Self {
            time,
            memory: (memory_mib > 0).then(|| memory_mib.saturating_mul(1024 * 1024)),
        }
    }
}

/// How waiting for a worker ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Finished,
    TimedOut,
    OutOfMemory,
}

/// Starts a hidden worker subcommand of this executable with backtraces enabled.
pub fn worker_command() -> Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("worker")
        .env("RUST_BACKTRACE", "full")
        .env("RUST_LIB_BACKTRACE", "1");
    Ok(command)
}

/// Runs `command`, parsing its stdout as JSON on success.
pub fn execute<T: DeserializeOwned>(
    command: Command,
    limits: Limits,
) -> Result<(Option<T>, ProcessEvidence)> {
    let start = Instant::now();
    let (output, stop) = limited_output(command, limits)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut evidence = ProcessEvidence {
        exit_status: output.status.to_string(),
        timed_out: stop == Stop::TimedOut,
        memory_exceeded: stop == Stop::OutOfMemory,
        elapsed_ms: start.elapsed().as_millis(),
        stage: stderr
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix(STAGE_MARKER))
            .map(str::to_owned),
        render: render_stats(&stderr),
        stderr: tail(&stderr, STDERR_LIMIT),
        stdout: None,
    };
    let value = if stop == Stop::Finished && output.status.success() {
        match serde_json::from_slice(&output.stdout) {
            Ok(value) => Some(value),
            Err(error) => {
                evidence.stdout = Some(format!(
                    "invalid worker JSON ({error}): {}",
                    tail(&String::from_utf8_lossy(&output.stdout), 4096)
                ));
                None
            }
        }
    } else {
        None
    };
    Ok((value, evidence))
}

/// Merges the render figures a worker reported, in order; `None` when it reported none.
fn render_stats(stderr: &str) -> Option<RenderStats> {
    stderr
        .lines()
        .filter_map(|line| line.strip_prefix(STATS_MARKER))
        .filter_map(|json| serde_json::from_str::<RenderStats>(json).ok())
        .reduce(|mut stats, update| {
            stats.merge(update);
            stats
        })
}

/// Returns true for the progress lines workers print for the harness rather than for readers.
pub fn is_marker(line: &str) -> bool {
    line.starts_with(STAGE_MARKER) || line.starts_with(STATS_MARKER)
}

fn tail(text: &str, limit: usize) -> String {
    let skip = text.len().saturating_sub(limit);
    let start = (skip..=text.len())
        .find(|index| text.is_char_boundary(*index))
        .unwrap_or(text.len());
    text.get(start..).unwrap_or_default().to_owned()
}

/// Returns the resident memory of a running child in bytes, or `None` where `/proc` is
/// unavailable.
fn resident_bytes(child: &Child) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(kib.saturating_mul(1024))
}

fn limited_output(mut command: Command, limits: Limits) -> Result<(Output, Stop)> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("launching {command:?}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("stdout was not piped"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("stderr was not piped"))?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes)?;
        Ok::<_, std::io::Error>(bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes)?;
        Ok::<_, std::io::Error>(bytes)
    });
    let start = Instant::now();
    let mut stop = Stop::Finished;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if start.elapsed() >= limits.time {
            stop = Stop::TimedOut;
        } else if limits
            .memory
            .zip(resident_bytes(&child))
            .is_some_and(|(limit, used)| used > limit)
        {
            stop = Stop::OutOfMemory;
        }
        if stop != Stop::Finished {
            child.kill()?;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow!("stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow!("stderr reader panicked"))??;
    Ok((
        Output {
            status,
            stdout,
            stderr,
        },
        stop,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn kills_a_worker_over_the_memory_limit() {
        // `tail` buffers the newline-free /dev/zero without bound.
        let mut command = Command::new("tail");
        command.arg("/dev/zero");
        let limits = Limits::new(Duration::from_secs(30), 64);
        let (_, stop) = limited_output(command, limits).unwrap();
        assert_eq!(stop, Stop::OutOfMemory);
    }

    #[test]
    fn stops_at_the_time_limit() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let limits = Limits::new(Duration::from_millis(100), 0);
        let (_, stop) = limited_output(command, limits).unwrap();
        assert_eq!(stop, Stop::TimedOut);
    }

    #[test]
    fn zero_memory_limit_is_unlimited() {
        assert_eq!(Limits::new(Duration::ZERO, 0).memory, None);
        assert_eq!(Limits::new(Duration::ZERO, 2).memory, Some(2 * 1024 * 1024));
    }
}
