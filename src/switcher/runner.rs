use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;

const POLL_INTERVAL: Duration = Duration::from_millis(2);
const TERMINATE_GRACE: Duration = Duration::from_millis(25);

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("failed to spawn child: {0}")]
    Spawn(#[source] io::Error),
    #[error("child timed out")]
    Timeout,
    #[error("failed to wait for child: {0}")]
    Wait(#[source] io::Error),
    #[error("child output reader failed: {0}")]
    Read(#[source] io::Error),
    #[error("child stdout was not valid UTF-8")]
    InvalidUtf8,
}

#[derive(Debug)]
pub struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

impl BoundedOutput {
    pub fn stdout_text(&self) -> Result<&str, RunnerError> {
        std::str::from_utf8(&self.stdout).map_err(|_| RunnerError::InvalidUtf8)
    }

    pub fn sanitized_stderr(&self) -> String {
        sanitize_diagnostic_bytes(&self.stderr, 512)
    }
}

pub fn run_bounded<I, S>(
    program: &OsStr,
    args: I,
    deadline: Instant,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<BoundedOutput, RunnerError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    if Instant::now() >= deadline {
        return Err(RunnerError::Timeout);
    }
    let mut command = Command::new(program);
    command
        .args(args.into_iter().map(Into::into))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn().map_err(RunnerError::Spawn)?;
    #[cfg(unix)]
    let process_group = child.id() as libc::pid_t;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    if let Err(error) = set_nonblocking(&stdout).and_then(|()| set_nonblocking(&stderr)) {
        terminate_then_kill(&mut child);
        return Err(RunnerError::Read(error));
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let stdout_cancelled = Arc::clone(&cancelled);
    let stderr_cancelled = Arc::clone(&cancelled);
    let stdout_reader =
        thread::spawn(move || read_bounded(stdout, stdout_limit, deadline, stdout_cancelled));
    let stderr_reader =
        thread::spawn(move || read_bounded(stderr, stderr_limit, deadline, stderr_cancelled));

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                terminate_then_kill(&mut child);
                break Err(RunnerError::Timeout);
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(error) => {
                terminate_then_kill(&mut child);
                break Err(RunnerError::Wait(error));
            }
        }
    };

    #[cfg(unix)]
    if status.is_ok() {
        terminate_process_group(process_group);
    }
    if status.is_err() {
        cancelled.store(true, Ordering::Release);
    }
    let stdout = stdout_reader
        .join()
        .expect("stdout reader thread panicked")
        .map_err(RunnerError::Read)?;
    let stderr = stderr_reader
        .join()
        .expect("stderr reader thread panicked")
        .map_err(RunnerError::Read)?;

    Ok(BoundedOutput {
        status: status?,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
    })
}

struct Captured {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(
    mut reader: impl Read,
    limit: usize,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
) -> io::Result<Captured> {
    let mut retained = Vec::with_capacity(limit.min(64 * 1024));
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            break;
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(error) => return Err(error),
        };
        let available = limit.saturating_sub(retained.len());
        let keep = available.min(read);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    Ok(Captured {
        bytes: retained,
        truncated,
    })
}

#[cfg(unix)]
fn set_nonblocking<T: AsRawFd>(pipe: &T) -> io::Result<()> {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_nonblocking<T>(_pipe: &T) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn process_group_exists(group_id: libc::pid_t) -> bool {
    // SAFETY: signal 0 performs an existence/permission check only.
    let result = unsafe { libc::kill(-group_id, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn terminate_process_group(group_id: libc::pid_t) {
    if !process_group_exists(group_id) {
        return;
    }
    // SAFETY: every spawned child is made leader of this private process group.
    unsafe {
        libc::kill(-group_id, libc::SIGTERM);
    }
    let grace_end = Instant::now() + TERMINATE_GRACE;
    while process_group_exists(group_id) && Instant::now() < grace_end {
        thread::sleep(Duration::from_millis(1));
    }
    if process_group_exists(group_id) {
        // Kill descendants that ignore SIGTERM or inherited captured pipes.
        unsafe {
            libc::kill(-group_id, libc::SIGKILL);
        }
    }
}

fn terminate_then_kill(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        terminate_process_group(child.id() as libc::pid_t);
        let _ = child.wait();
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub fn sanitize_diagnostic_bytes(bytes: &[u8], max_bytes: usize) -> String {
    let lossy = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    for ch in lossy.chars() {
        if out.len() >= max_bytes {
            break;
        }
        let replacement = if ch.is_control() && !matches!(ch, ' ' | '\t') {
            ' '
        } else {
            ch
        };
        if out.len() + replacement.len_utf8() > max_bytes {
            break;
        }
        out.push(replacement);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn bounds_retained_stdout_and_stderr() {
        let deadline = Instant::now() + Duration::from_secs(2);
        let output = run_bounded(
            OsStr::new("sh"),
            ["-c", "printf 123456789; printf abcdefghi >&2"],
            deadline,
            4,
            5,
        )
        .unwrap();
        assert_eq!(output.stdout, b"1234");
        assert_eq!(output.stderr, b"abcde");
        assert!(output.stdout_truncated);
        assert!(output.stderr_truncated);
    }

    #[test]
    fn fast_exit_never_loses_captured_output() {
        for _ in 0..100 {
            let output = run_bounded(
                OsStr::new("printf"),
                ["exact-output"],
                Instant::now() + Duration::from_secs(1),
                32,
                32,
            )
            .unwrap();
            assert_eq!(output.stdout, b"exact-output");
        }
    }

    #[cfg(unix)]
    #[test]
    fn successful_leader_cannot_leave_a_term_ignoring_descendant() {
        let temp = tempfile::tempdir().unwrap();
        let pid_file = temp.path().join("descendant.pid");
        let script = "(trap '' TERM; echo ready > \"$1.ready\"; exec sleep 10) & child=$!; while [ ! -s \"$1.ready\" ]; do :; done; echo $child > \"$1\"";
        let args = vec![
            OsString::from("-c"),
            OsString::from(script),
            OsString::from("sh"),
            pid_file.as_os_str().to_os_string(),
        ];
        let started = Instant::now();
        let output = run_bounded(
            OsStr::new("sh"),
            args,
            Instant::now() + Duration::from_secs(2),
            32,
            32,
        )
        .unwrap();
        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_millis(500));
        let pid = fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse::<libc::pid_t>()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            // SAFETY: signal 0 checks whether the recorded process still exists.
            if unsafe { libc::kill(pid, 0) } != 0
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("successful child left descendant {pid} alive");
    }

    #[test]
    fn timeout_kills_and_reaps_child() {
        let result = run_bounded(
            OsStr::new("sh"),
            ["-c", "sleep 10"],
            Instant::now() + Duration::from_millis(20),
            32,
            32,
        );
        assert!(matches!(result, Err(RunnerError::Timeout)));
    }

    #[test]
    fn diagnostics_are_single_line_and_bounded() {
        assert_eq!(
            sanitize_diagnostic_bytes(b"bad\n\x1b[31m thing", 10),
            "bad [31m"
        );
    }
}
