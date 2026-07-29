use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use tmx::switcher::runner::{run_bounded, RunnerError};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("tmx-supervisor: {message}");
            ExitCode::from(125)
        }
    }
}

fn run() -> Result<u8, String> {
    let mut args = std::env::args_os().skip(1);
    let mut deadline_ms = 500_u64;
    let mut stdout_limit = 8 * 1024 * 1024_usize;
    let mut stderr_limit = 16 * 1024_usize;
    let mut command: Option<OsString> = None;
    let mut child_args = Vec::new();

    while let Some(arg) = args.next() {
        if command.is_some() {
            child_args.push(arg);
            child_args.extend(args);
            break;
        }
        match arg.to_str() {
            Some("--deadline-ms") => {
                deadline_ms = parse_u64(args.next(), "--deadline-ms")?.clamp(25, 2_000)
            }
            Some("--stdout-limit") => {
                stdout_limit =
                    parse_usize(args.next(), "--stdout-limit")?.clamp(1_024, 8 * 1024 * 1024)
            }
            Some("--stderr-limit") => {
                stderr_limit = parse_usize(args.next(), "--stderr-limit")?.clamp(256, 64 * 1024)
            }
            Some("--") => {
                command = args.next();
                if command.is_none() {
                    return Err("missing child program after --".into());
                }
            }
            _ => return Err("expected limits followed by -- and a child program".into()),
        }
    }
    let command = command.ok_or_else(|| "missing -- and child program".to_string())?;
    let output = run_bounded(
        OsStr::new(&command),
        child_args,
        Instant::now() + Duration::from_millis(deadline_ms),
        stdout_limit,
        stderr_limit,
    );
    match output {
        Ok(output) => {
            io::stdout()
                .write_all(&output.stdout)
                .map_err(|error| format!("write stdout: {error}"))?;
            io::stderr()
                .write_all(&output.stderr)
                .map_err(|error| format!("write stderr: {error}"))?;
            if output.stdout_truncated || output.stderr_truncated {
                eprintln!("tmx-supervisor: child output exceeded its retained limit");
                return Ok(125);
            }
            Ok(output
                .status
                .code()
                .and_then(|code| u8::try_from(code).ok())
                .unwrap_or(125))
        }
        Err(RunnerError::Timeout) => {
            eprintln!("tmx-supervisor: child deadline exceeded");
            Ok(124)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn parse_u64(value: Option<OsString>, flag: &str) -> Result<u64, String> {
    value
        .and_then(|value| value.to_str().and_then(|value| value.parse().ok()))
        .ok_or_else(|| format!("{flag} requires an unsigned integer"))
}

fn parse_usize(value: Option<OsString>, flag: &str) -> Result<usize, String> {
    value
        .and_then(|value| value.to_str().and_then(|value| value.parse().ok()))
        .ok_or_else(|| format!("{flag} requires an unsigned integer"))
}
