use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn outer_supervisor_times_out_and_reaps_a_wedged_child_group() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("child.pid");
    let script = format!("echo $$ > '{}'; exec sleep 5", pid_file.display());
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tmx-supervisor"))
        .args([
            "--deadline-ms",
            "75",
            "--stdout-limit",
            "1024",
            "--stderr-limit",
            "1024",
            "--",
            "/bin/sh",
            "-c",
            &script,
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(124));
    assert!(started.elapsed() < Duration::from_millis(500));
    let pid = std::fs::read_to_string(pid_file).unwrap();
    let pid_number: i32 = pid.trim().parse().unwrap();
    let alive = unsafe { libc::kill(pid_number, 0) == 0 };
    assert!(!alive, "supervisor left child {} alive", pid.trim());
}

#[cfg(unix)]
#[test]
fn successful_supervised_leader_cannot_leave_a_descendant() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("descendant.pid");
    let script = "(trap '' TERM; echo ready > \"$1.ready\"; exec sleep 10) & child=$!; while [ ! -s \"$1.ready\" ]; do :; done; echo $child > \"$1\"";
    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_tmx-supervisor"))
        .args([
            "--deadline-ms",
            "1000",
            "--stdout-limit",
            "1024",
            "--stderr-limit",
            "1024",
            "--",
            "/bin/sh",
            "-c",
            script,
            "sh",
        ])
        .arg(&pid_file)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(started.elapsed() < Duration::from_millis(500));
    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if unsafe { libc::kill(pid, 0) } != 0
            && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("supervisor left successful descendant {pid} alive");
}

#[test]
fn outer_supervisor_preserves_success_stdout_exactly() {
    let output = Command::new(env!("CARGO_BIN_EXE_tmx-supervisor"))
        .args([
            "--deadline-ms",
            "100",
            "--stdout-limit",
            "1024",
            "--stderr-limit",
            "1024",
            "--",
            "/usr/bin/printf",
            "%s",
            "{\"ok\":true}",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, br#"{"ok":true}"#);
    assert!(output.stderr.is_empty());
}
