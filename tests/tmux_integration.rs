use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tmx::tmux::Tmux;

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn unique_socket() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("tmx-test-{pid}-{n}-{count}")
}

fn cleanup(socket: &str) {
    let _ = Command::new("tmux")
        .args(["-L", socket, "kill-server"])
        .output();
}

#[test]
fn isolated_tmux_lists_and_renames() {
    if !tmux_available() {
        eprintln!("skipping: tmux unavailable");
        return;
    }
    let socket = unique_socket();
    cleanup(&socket);
    let status = Command::new("tmux")
        .args([
            "-L",
            &socket,
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "test",
            "-c",
            "/tmp",
        ])
        .status()
        .unwrap();
    assert!(status.success());

    let tmux = Tmux::with_socket(&socket);
    let sessions = tmux.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "test");
    assert_eq!(sessions[0].path, "/tmp");

    tmux.rename("session", "test", "renamed").unwrap();
    let sessions = tmux.list_sessions().unwrap();
    assert_eq!(sessions[0].name, "renamed");

    tmux.set_option("renamed", "@tmx.note", "hello", None)
        .unwrap();
    assert_eq!(
        tmux.get_option("renamed", "@tmx.note", Some("session"))
            .unwrap()
            .as_deref(),
        Some("hello")
    );

    cleanup(&socket);
}

#[test]
fn isolated_tmux_creates_session_with_cwd() {
    if !tmux_available() {
        eprintln!("skipping: tmux unavailable");
        return;
    }
    let socket = unique_socket();
    cleanup(&socket);
    let tmux = Tmux::with_socket(&socket);
    tmux.new_session("cwd-test", "/tmp").unwrap();
    let sessions = tmux.list_sessions().unwrap();
    assert_eq!(sessions[0].name, "cwd-test");
    assert_eq!(sessions[0].path, "/tmp");
    cleanup(&socket);
}
