#![allow(dead_code)]

use std::fs::File;
use std::io;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

pub fn require_tmux() {
    let output = Command::new("tmux").arg("-V").output();
    assert!(
        output.is_ok_and(|value| value.status.success()),
        "required integration prerequisite tmux is unavailable"
    );
}

fn unique(_prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "tx{:x}{:x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub struct TmuxServer {
    pub name: String,
    pub tmp: TempDir,
}

impl TmuxServer {
    pub fn new(session: &str) -> Self {
        require_tmux();
        let server = Self {
            name: unique("tmx-test"),
            tmp: tempfile::Builder::new()
                .prefix("tx")
                .tempdir_in("/tmp")
                .unwrap(),
        };
        let status = server
            .command()
            .args(["new-session", "-d", "-s", session, "-c", "/tmp"])
            .status()
            .unwrap();
        assert!(status.success(), "failed to create isolated tmux server");
        server
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new("tmux");
        command
            .env("TMUX_TMPDIR", self.tmp.path())
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .args(["-L", &self.name, "-f", "/dev/null"]);
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    pub fn checked(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "tmux command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .into()
    }

    pub fn socket_path(&self) -> PathBuf {
        self.tmp
            .path()
            .join(format!("tmux-{}", effective_uid()))
            .join(&self.name)
    }

    pub fn create_session(&self, name: &str) {
        self.checked(&["new-session", "-d", "-s", name, "-c", "/tmp"]);
    }

    pub fn config_with(&self, others: &[(&TmuxServer, &str)]) -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let mut text = String::from(
            "[switcher]\nenabled = true\ndeadline_ms = 800\nendpoint_soft_timeout_ms = 2000\n",
        );
        text.push_str(&format!(
            "[[switcher.endpoints]]\nselector = {:?}\nalias = \"primary\"\n",
            format!("path:{}", self.socket_path().display())
        ));
        for (server, alias) in others {
            text.push_str(&format!(
                "[[switcher.endpoints]]\nselector = {:?}\nalias = {:?}\n",
                format!("path:{}", server.socket_path().display()),
                alias
            ));
        }
        std::fs::write(dir.path().join("config.toml"), text).unwrap();
        dir
    }
}

impl Drop for TmuxServer {
    fn drop(&mut self) {
        let _ = self.command().arg("kill-server").output();
    }
}

pub struct PtyClient {
    child: Child,
    _master: File,
}

impl PtyClient {
    pub fn attach(server: &TmuxServer, session: &str) -> Self {
        let mut command = server.command();
        command.args(["attach-session", "-t", session]);
        let client = Self::spawn(command);
        poll(Duration::from_secs(3), || {
            !server
                .checked(&["list-clients", "-F", "#{client_name}"])
                .is_empty()
        });
        client
    }

    pub fn spawn(mut command: Command) -> Self {
        let (master, slave) = open_pty().expect("allocate PTY");
        command.env("TERM", "xterm-256color");
        let stdout_fd = unsafe { libc::dup(slave) };
        let stderr_fd = unsafe { libc::dup(slave) };
        assert!(stdout_fd >= 0 && stderr_fd >= 0, "duplicate PTY slave");

        unsafe {
            command
                .stdin(Stdio::from_raw_fd(slave))
                .stdout(Stdio::from_raw_fd(stdout_fd))
                .stderr(Stdio::from_raw_fd(stderr_fd));
            command.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                if libc::ioctl(libc::STDIN_FILENO, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Self {
            child: command.spawn().expect("spawn PTY client"),
            _master: unsafe { File::from_raw_fd(master) },
        }
    }
}

impl Drop for PtyClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn open_pty() -> io::Result<(RawFd, RawFd)> {
    let mut master = -1;
    let mut slave = -1;
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Ok((master, slave))
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn poll(timeout: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("condition did not become true within {timeout:?}");
}

pub fn tmx(config_dir: &Path, args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config_dir.join("config.toml"))
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args)
        .output()
        .unwrap()
}

pub fn inventory(config_dir: &Path) -> serde_json::Value {
    let output = tmx(
        config_dir,
        &[
            "inventory".into(),
            "--schema".into(),
            "1".into(),
            "--json".into(),
            "--request-id".into(),
            "integration".into(),
            "--deadline-ms".into(),
            "1000".into(),
        ],
    );
    assert!(
        output.status.success(),
        "inventory failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn effective_uid() -> u32 {
    unsafe { libc::geteuid() }
}
