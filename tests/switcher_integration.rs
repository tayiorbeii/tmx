mod support;

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use support::{inventory, poll, tmx, PtyClient, TmuxServer};

fn endpoint<'a>(inventory: &'a Value, alias: &str) -> &'a Value {
    inventory["endpoints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["alias"] == alias)
        .unwrap()
}

fn route_args(endpoint: &Value, target: &Value, kind: &str, client: &Value) -> Vec<String> {
    let mut args = vec![
        "route".into(),
        "--schema".into(),
        "1".into(),
        "--json".into(),
        "--request-id".into(),
        "route-test".into(),
        "--host-domain".into(),
        endpoint["host_domain"].as_str().unwrap().into(),
        "--endpoint-id".into(),
        endpoint["endpoint_id"].as_str().unwrap().into(),
        "--generation".into(),
        endpoint["generation"]["token"].as_str().unwrap().into(),
        "--target-kind".into(),
        kind.into(),
        "--session-id".into(),
        target["session_id"].as_str().unwrap().into(),
    ];
    if let Some(value) = target.get("window_id").and_then(Value::as_str) {
        args.extend(["--window-id".into(), value.into()]);
    }
    if let Some(value) = target.get("pane_id").and_then(Value::as_str) {
        args.extend(["--pane-id".into(), value.into()]);
    }
    for (flag, field) in [
        ("--client-name", "client_name"),
        ("--client-tty", "client_tty"),
        ("--client-pid", "client_pid"),
        ("--client-created", "client_created"),
        ("--client-uid", "client_uid"),
    ] {
        args.extend([flag.into(), client[field].as_str().unwrap().into()]);
    }
    args.extend(["--deadline-ms".into(), "2000".into()]);
    args
}

fn set_route_deadline(args: &mut [String], deadline_ms: u64) {
    let deadline_index = args
        .iter()
        .position(|value| value == "--deadline-ms")
        .expect("route arguments must contain a deadline")
        + 1;
    args[deadline_index] = deadline_ms.to_string();
}

fn attach_args(endpoint: &Value, target: &Value, kind: &str) -> Vec<String> {
    let mut args = vec![
        "attach".into(),
        "--schema".into(),
        "1".into(),
        "--request-id".into(),
        "attach-test".into(),
        "--host-domain".into(),
        endpoint["host_domain"].as_str().unwrap().into(),
        "--endpoint-id".into(),
        endpoint["endpoint_id"].as_str().unwrap().into(),
        "--generation".into(),
        endpoint["generation"]["token"].as_str().unwrap().into(),
        "--target-kind".into(),
        kind.into(),
        "--session-id".into(),
        target["session_id"].as_str().unwrap().into(),
    ];
    if let Some(value) = target.get("window_id").and_then(Value::as_str) {
        args.extend(["--window-id".into(), value.into()]);
    }
    if let Some(value) = target.get("pane_id").and_then(Value::as_str) {
        args.extend(["--pane-id".into(), value.into()]);
    }
    args.extend(["--deadline-ms".into(), "2000".into()]);
    args
}

fn assert_route(config: &Path, args: Vec<String>, expected: &str) -> Value {
    let output = tmx(config, &args);
    assert!(
        output.status.success(),
        "route CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["schema"]["name"], "dev.tmx.route");
    assert_eq!(response["outcome"], expected);
    response
}

#[test]
#[serial_test::serial]
fn nonlocal_host_domain_is_rejected_before_endpoint_access() {
    let config = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("config.toml"), "").unwrap();
    let args = vec![
        "route".into(),
        "--schema".into(),
        "1".into(),
        "--json".into(),
        "--host-domain".into(),
        "ssh:host".into(),
        "--endpoint-id".into(),
        "ep_fake".into(),
        "--generation".into(),
        "gen_fake".into(),
        "--target-kind".into(),
        "session".into(),
        "--session-id".into(),
        "$1".into(),
        "--client-name".into(),
        "/dev/tty".into(),
        "--client-tty".into(),
        "/dev/tty".into(),
        "--client-pid".into(),
        "1".into(),
        "--client-created".into(),
        "1".into(),
        "--client-uid".into(),
        "501".into(),
    ];
    let response = assert_route(config.path(), args, "untrusted_endpoint");
    assert_eq!(response["diagnostics"][0]["code"], "untrusted_host_domain");
}

#[test]
#[serial_test::serial]
fn typed_route_errors_cover_incompatible_untrusted_and_timeout() {
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(empty.path().join("config.toml"), "").unwrap();
    let base = vec![
        "route".into(),
        "--schema".into(),
        "2".into(),
        "--json".into(),
        "--endpoint-id".into(),
        "ep_fake".into(),
        "--generation".into(),
        "gen_fake".into(),
        "--target-kind".into(),
        "session".into(),
        "--session-id".into(),
        "$1".into(),
        "--client-name".into(),
        "/dev/tty".into(),
        "--client-tty".into(),
        "/dev/tty".into(),
        "--client-pid".into(),
        "1".into(),
        "--client-created".into(),
        "1".into(),
        "--client-uid".into(),
        "501".into(),
    ];
    assert_route(empty.path(), base.clone(), "incompatible_schema");
    let mut untrusted = base;
    let schema_index = untrusted.iter().position(|value| value == "2").unwrap();
    untrusted[schema_index] = "1".into();
    assert_route(empty.path(), untrusted.clone(), "untrusted_endpoint");
    let mut invalid_mode = untrusted;
    invalid_mode.extend(["--mode".into(), "new-attachment".into()]);
    assert_route(empty.path(), invalid_mode, "command_failure");

    let server = TmuxServer::new("origin");
    server.create_session("target");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["name"] == "target")
        .unwrap();
    let mut args = route_args(ep, target, "session", &ep["clients"][0]);
    set_route_deadline(&mut args, 100);
    let barrier_dir = tempfile::tempdir().unwrap();
    let barrier = barrier_dir.path().join("timeout-ready");
    let output = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env("TMX_TEST_MODE", "1")
        .env("TMX_TEST_BARRIER_FILE", &barrier)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["outcome"], "timeout");
    assert_eq!(
        server.checked(&["list-clients", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn postcondition_race_returns_partial_success_after_proven_mutation() {
    let server = TmuxServer::new("origin");
    server.create_session("target");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "target")
        .unwrap();
    let client = &ep["clients"][0];
    let mut args = route_args(ep, target, "session", client);
    set_route_deadline(&mut args, 2_000);
    let barrier_dir = tempfile::tempdir().unwrap();
    let barrier = barrier_dir.path().join("post-ready");
    let child = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env("TMX_TEST_MODE", "1")
        .env("TMX_TEST_POST_BARRIER_FILE", &barrier)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    poll(Duration::from_secs(3), || barrier.exists());
    assert_eq!(
        server.checked(&["list-clients", "-F", "#{session_name}"]),
        "target"
    );
    server.checked(&[
        "switch-client",
        "-c",
        client["client_name"].as_str().unwrap(),
        "-t",
        "origin",
    ]);
    std::fs::write(barrier.with_extension("continue"), b"continue").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["outcome"], "partial_success");
    assert_eq!(
        server.checked(&["list-clients", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn current_target_route_is_a_defined_noop() {
    let server = TmuxServer::new("current");
    let _client = PtyClient::attach(&server, "current");
    let config = server.config_with(&[]);
    let before = inventory(config.path());
    let ep = endpoint(&before, "primary");
    let session = &ep["sessions"][0];
    assert_route(
        config.path(),
        route_args(ep, session, "session", &ep["clients"][0]),
        "success",
    );
    let after = inventory(config.path());
    let before_ep = endpoint(&before, "primary");
    let after_ep = endpoint(&after, "primary");
    assert_eq!(before_ep["sessions"], after_ep["sessions"]);
    assert_eq!(before_ep["windows"], after_ep["windows"]);
    assert_eq!(before_ep["panes"], after_ep["panes"]);
    assert_eq!(before_ep["clients"], after_ep["clients"]);
}

#[test]
#[serial_test::serial]
fn mapped_client_route_succeeds_with_default_cli_deadline() {
    let server = TmuxServer::new("origin");
    server.create_session("target");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "target")
        .unwrap();
    let mut args = route_args(ep, target, "session", &ep["clients"][0]);
    let deadline_flag = args
        .iter()
        .position(|value| value == "--deadline-ms")
        .unwrap();
    args.drain(deadline_flag..=deadline_flag + 1);

    assert_route(config.path(), args, "success");
    assert_eq!(
        server.checked(&["list-clients", "-F", "#{session_name}"]),
        "target"
    );
}

#[test]
#[serial_test::serial]
fn inventory_and_route_cover_session_window_and_pane_with_exact_client() {
    let server = TmuxServer::new("origin");
    server.create_session("target");
    server.checked(&["new-window", "-d", "-t", "target", "-n", "work"]);
    server.checked(&["split-window", "-d", "-t", "target:work"]);
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);

    let first = inventory(config.path());
    assert!(
        !first
            .to_string()
            .contains(&server.socket_path().to_string_lossy().to_string()),
        "machine inventory exposed a raw socket path"
    );
    let ep = endpoint(&first, "primary");
    assert_eq!(ep["status"], "available");
    assert_eq!(ep["clients"].as_array().unwrap().len(), 1);
    let client = &ep["clients"][0];
    let session = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "target")
        .unwrap();
    assert_route(
        config.path(),
        route_args(ep, session, "session", client),
        "success",
    );

    let second = inventory(config.path());
    let ep = endpoint(&second, "primary");
    let client = &ep["clients"][0];
    assert_eq!(client["attached_session_id"], session["session_id"]);
    let window = ep["windows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["session_id"] == session["session_id"] && value["name"] == "work")
        .unwrap();
    assert_route(
        config.path(),
        route_args(ep, window, "window", client),
        "success",
    );

    let third = inventory(config.path());
    let ep = endpoint(&third, "primary");
    let client = &ep["clients"][0];
    let pane = ep["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| {
            value["session_id"] == session["session_id"]
                && value["window_id"] == window["window_id"]
                && value["index"] == "1"
        })
        .unwrap();
    assert_route(
        config.path(),
        route_args(ep, pane, "pane", client),
        "success",
    );

    let final_inventory = inventory(config.path());
    let ep = endpoint(&final_inventory, "primary");
    let client = &ep["clients"][0];
    assert_eq!(client["attached_session_id"], pane["session_id"]);
    assert_eq!(client["current_window_id"], pane["window_id"]);
    assert_eq!(client["current_pane_id"], pane["pane_id"]);
}

#[test]
#[serial_test::serial]
fn exact_client_route_preserves_unrelated_clients_and_records_shared_pane_effect() {
    let server = TmuxServer::new("alpha");
    server.create_session("beta");
    server.create_session("gamma");
    server.checked(&["split-window", "-d", "-t", "beta:0"]);
    let _alpha = PtyClient::attach(&server, "alpha");
    let _beta = PtyClient::attach(&server, "beta");
    let _gamma = PtyClient::attach(&server, "gamma");
    let config = server.config_with(&[]);
    let before = inventory(config.path());
    let ep = endpoint(&before, "primary");
    assert_eq!(ep["clients"].as_array().unwrap().len(), 3);
    let session_id = |name: &str| {
        ep["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["name"] == name)
            .unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let alpha_id = session_id("alpha");
    let beta_id = session_id("beta");
    let gamma_id = session_id("gamma");
    let alpha_client = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["attached_session_id"] == alpha_id)
        .unwrap();
    let beta_client = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["attached_session_id"] == beta_id)
        .unwrap();
    let gamma_client = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["attached_session_id"] == gamma_id)
        .unwrap();
    let target = ep["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pane| pane["session_id"] == beta_id && pane["index"] == "1")
        .unwrap();
    let beta_name = beta_client["client_name"].as_str().unwrap().to_string();
    let gamma_before = gamma_client.clone();
    let zoom_before = server.checked(&[
        "display-message",
        "-p",
        "-t",
        target["window_id"].as_str().unwrap(),
        "#{window_zoomed_flag}",
    ]);
    assert_route(
        config.path(),
        route_args(ep, target, "pane", alpha_client),
        "success",
    );

    let after = inventory(config.path());
    let ep = endpoint(&after, "primary");
    assert_eq!(ep["clients"].as_array().unwrap().len(), 3);
    let chosen = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_name"] == alpha_client["client_name"])
        .unwrap();
    assert_eq!(chosen["attached_session_id"], beta_id);
    assert_eq!(chosen["current_pane_id"], target["pane_id"]);
    let beta_after = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_name"] == beta_name)
        .unwrap();
    assert_eq!(beta_after["attached_session_id"], beta_id);
    assert_eq!(beta_after["current_pane_id"], target["pane_id"]);
    let gamma_after = ep["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|client| client["client_name"] == gamma_before["client_name"])
        .unwrap();
    for field in [
        "client_name",
        "client_pid",
        "client_created",
        "client_tty",
        "client_uid",
        "attached_session_id",
        "current_window_id",
        "current_pane_id",
    ] {
        assert_eq!(gamma_after[field], gamma_before[field], "changed {field}");
    }
    let zoom_after = server.checked(&[
        "display-message",
        "-p",
        "-t",
        target["window_id"].as_str().unwrap(),
        "#{window_zoomed_flag}",
    ]);
    assert_eq!(zoom_after, zoom_before);
}

#[test]
#[serial_test::serial]
fn new_attachment_selects_exact_pane_and_attaches_last() {
    let server = TmuxServer::new("origin");
    server.create_session("target");
    server.checked(&["new-window", "-d", "-t", "target", "-n", "work"]);
    server.checked(&["split-window", "-d", "-t", "target:work"]);
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let pane = ep["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| {
            let session_id = value["session_id"].as_str().unwrap();
            let session = ep["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|session| session["session_id"] == session_id)
                .unwrap();
            session["name"] == "target" && value["index"] == "1"
        })
        .unwrap();
    let args = attach_args(ep, pane, "pane");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tmx"));
    command
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args);
    let _attachment = PtyClient::spawn(command);
    poll(Duration::from_secs(3), || {
        !server
            .checked(&["list-clients", "-F", "#{client_name}"])
            .is_empty()
    });
    let selected = server.checked(&[
        "list-clients",
        "-F",
        "#{session_id}|#{window_id}|#{pane_id}",
    ]);
    assert_eq!(
        selected,
        format!(
            "{}|{}|{}",
            pane["session_id"].as_str().unwrap(),
            pane["window_id"].as_str().unwrap(),
            pane["pane_id"].as_str().unwrap()
        )
    );
}

#[test]
#[serial_test::serial]
fn new_attachment_barrier_deadline_returns_timeout_before_process_creation() {
    let server = TmuxServer::new("target");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = &ep["sessions"][0];
    let mut args = attach_args(ep, target, "session");
    let deadline_index = args
        .iter()
        .position(|value| value == "--deadline-ms")
        .unwrap()
        + 1;
    args[deadline_index] = "100".into();
    args.extend(["--hold-on-error-ms".into(), "0".into()]);
    let barrier_dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env("TMX_TEST_MODE", "1")
        .env("TMX_TEST_BARRIER_FILE", barrier_dir.path().join("ready"))
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(args)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("timeout"));
    assert!(server
        .checked(&["list-clients", "-F", "#{client_name}"])
        .is_empty());
}

#[test]
#[serial_test::serial]
fn overlapping_runtime_ids_on_two_servers_never_cross_route() {
    let a = TmuxServer::new("same");
    let b = TmuxServer::new("same");
    b.create_session("destination");
    let _client = PtyClient::attach(&b, "same");
    let config = a.config_with(&[(&b, "secondary")]);

    let snapshot = inventory(config.path());
    let ep_a = endpoint(&snapshot, "primary");
    let ep_b = endpoint(&snapshot, "secondary");
    assert_ne!(ep_a["endpoint_id"], ep_b["endpoint_id"]);
    assert_eq!(
        ep_a["sessions"][0]["session_id"],
        ep_b["sessions"][0]["session_id"]
    );
    let target = ep_b["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "destination")
        .unwrap();
    assert_route(
        config.path(),
        route_args(ep_b, target, "session", &ep_b["clients"][0]),
        "success",
    );

    assert_eq!(
        a.checked(&["list-sessions", "-F", "#{session_name}"]),
        "same"
    );
    assert_eq!(
        b.checked(&["list-clients", "-F", "#{session_name}"]),
        "destination"
    );
}

#[test]
#[serial_test::serial]
fn stale_target_fails_without_creating_or_falling_back_by_name() {
    let server = TmuxServer::new("origin");
    server.create_session("stale");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "stale")
        .unwrap();
    let args = route_args(ep, target, "session", &ep["clients"][0]);
    server.checked(&["kill-session", "-t", "stale"]);
    assert_route(config.path(), args, "stale_target");
    assert_eq!(
        server.checked(&["list-sessions", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn deleted_client_fingerprint_is_rejected_without_moving_another_client() {
    let server = TmuxServer::new("origin");
    let client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let args = route_args(ep, &ep["sessions"][0], "session", &ep["clients"][0]);
    drop(client);
    poll(Duration::from_secs(3), || {
        server
            .checked(&["list-clients", "-F", "#{client_name}"])
            .is_empty()
    });
    assert_route(config.path(), args, "stale_client");
    assert_eq!(
        server.checked(&["list-sessions", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn reused_pty_cannot_satisfy_an_old_client_fingerprint() {
    let server = TmuxServer::new("origin");
    server.create_session("target");
    let old_client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let before = inventory(config.path());
    let ep = endpoint(&before, "primary");
    let old_fingerprint = ep["clients"][0].clone();
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|session| session["name"] == "target")
        .unwrap();
    let args = route_args(ep, target, "session", &old_fingerprint);
    let old_tty = old_fingerprint["client_tty"].as_str().unwrap().to_string();
    drop(old_client);
    poll(Duration::from_secs(3), || {
        server
            .checked(&["list-clients", "-F", "#{client_name}"])
            .is_empty()
    });

    let mut replacement = None;
    for _ in 0..20 {
        let candidate = PtyClient::attach(&server, "origin");
        let fresh = inventory(config.path());
        let tty = endpoint(&fresh, "primary")["clients"][0]["client_tty"]
            .as_str()
            .unwrap()
            .to_string();
        if tty == old_tty {
            replacement = Some(candidate);
            break;
        }
        drop(candidate);
        poll(Duration::from_secs(3), || {
            server
                .checked(&["list-clients", "-F", "#{client_name}"])
                .is_empty()
        });
    }
    let _replacement = replacement.expect("the PTY allocator did not reuse the released device");
    assert_route(config.path(), args, "stale_client");
    assert_eq!(
        server.checked(&["list-clients", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn deterministic_barrier_revalidates_target_immediately_before_mutation() {
    let server = TmuxServer::new("origin");
    server.create_session("race-target");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "race-target")
        .unwrap();
    let mut args = route_args(ep, target, "session", &ep["clients"][0]);
    set_route_deadline(&mut args, 2_000);
    let barrier_dir = tempfile::tempdir().unwrap();
    let barrier = barrier_dir.path().join("route-ready");
    let child = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env("TMX_TEST_MODE", "1")
        .env("TMX_TEST_BARRIER_FILE", &barrier)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    poll(Duration::from_secs(3), || barrier.exists());
    server.checked(&["kill-session", "-t", "race-target"]);
    std::fs::write(barrier.with_extension("continue"), b"continue").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["outcome"], "stale_target");
    assert_eq!(
        server.checked(&["list-sessions", "-F", "#{session_name}"]),
        "origin"
    );
}

#[test]
#[serial_test::serial]
fn barrier_deletion_of_each_target_kind_is_stale_without_name_fallback() {
    for kind in ["session", "window", "pane"] {
        let server = TmuxServer::new("origin");
        server.create_session("target");
        server.checked(&["new-window", "-d", "-t", "target", "-n", "extra"]);
        server.checked(&["split-window", "-d", "-t", "target:extra"]);
        let _client = PtyClient::attach(&server, "origin");
        let config = server.config_with(&[]);
        let snapshot = inventory(config.path());
        let ep = endpoint(&snapshot, "primary");
        let target_session = ep["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "target")
            .unwrap();
        let target = match kind {
            "session" => target_session,
            "window" => ep["windows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| {
                    value["session_id"] == target_session["session_id"] && value["name"] == "extra"
                })
                .unwrap(),
            "pane" => ep["panes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|value| {
                    value["session_id"] == target_session["session_id"] && value["index"] == "1"
                })
                .unwrap(),
            _ => unreachable!(),
        };
        let mut args = route_args(ep, target, kind, &ep["clients"][0]);
        set_route_deadline(&mut args, 2_000);
        let barrier_dir = tempfile::tempdir().unwrap();
        let barrier = barrier_dir.path().join("ready");
        let child = Command::new(env!("CARGO_BIN_EXE_tmx"))
            .env("TMX_CONFIG", config.path().join("config.toml"))
            .env("TMX_TEST_MODE", "1")
            .env("TMX_TEST_BARRIER_FILE", &barrier)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .args(args)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        poll(Duration::from_secs(3), || barrier.exists());
        let (command, id) = match kind {
            "session" => ("kill-session", target["session_id"].as_str().unwrap()),
            "window" => ("kill-window", target["window_id"].as_str().unwrap()),
            "pane" => ("kill-pane", target["pane_id"].as_str().unwrap()),
            _ => unreachable!(),
        };
        server.checked(&[command, "-t", id]);
        std::fs::write(barrier.with_extension("continue"), b"continue").unwrap();
        let output = child.wait_with_output().unwrap();
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["outcome"], "stale_target", "kind {kind}");
        assert_eq!(
            server.checked(&["list-clients", "-F", "#{session_name}"]),
            "origin"
        );
    }
}

#[test]
#[serial_test::serial]
fn deterministic_barrier_rejects_socket_inode_and_generation_replacement() {
    let server = TmuxServer::new("origin");
    server.create_session("race-target");
    let _client = PtyClient::attach(&server, "origin");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let target = ep["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["name"] == "race-target")
        .unwrap();
    let mut args = route_args(ep, target, "session", &ep["clients"][0]);
    set_route_deadline(&mut args, 2_000);
    let barrier_dir = tempfile::tempdir().unwrap();
    let barrier = barrier_dir.path().join("socket-ready");
    let child = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .env("TMX_CONFIG", config.path().join("config.toml"))
        .env("TMX_TEST_MODE", "1")
        .env("TMX_TEST_BARRIER_FILE", &barrier)
        .env_remove("TMUX")
        .env_remove("TMUX_PANE")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    poll(Duration::from_secs(3), || barrier.exists());
    server.checked(&["kill-server"]);
    let _ = std::fs::remove_file(server.socket_path());
    let status = server
        .command()
        .args(["new-session", "-d", "-s", "replacement"])
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::write(barrier.with_extension("continue"), b"continue").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["outcome"], "stale_target");
    assert_eq!(
        server.checked(&["list-sessions", "-F", "#{session_name}"]),
        "replacement"
    );
}

#[test]
#[serial_test::serial]
fn restarted_server_generation_rejects_old_selection() {
    let server = TmuxServer::new("before");
    let _client = PtyClient::attach(&server, "before");
    let config = server.config_with(&[]);
    let snapshot = inventory(config.path());
    let ep = endpoint(&snapshot, "primary");
    let args = route_args(ep, &ep["sessions"][0], "session", &ep["clients"][0]);
    server.checked(&["kill-server"]);
    let _ = std::fs::remove_file(server.socket_path());
    let status = server
        .command()
        .args(["new-session", "-d", "-s", "after"])
        .status()
        .unwrap();
    assert!(status.success());
    assert_route(config.path(), args, "stale_target");
    assert_eq!(
        server.checked(&["list-sessions", "-F", "#{session_name}"]),
        "after"
    );
}

#[test]
#[serial_test::serial]
#[ignore = "release benchmark; run via scripts/benchmark-switcher.sh"]
fn route_execution_p95_is_within_budget() {
    let server = TmuxServer::new("left");
    server.create_session("right");
    let _client = PtyClient::attach(&server, "left");
    let config = server.config_with(&[]);
    let mut samples = Vec::new();
    for iteration in 0..30 {
        let snapshot = inventory(config.path());
        let ep = endpoint(&snapshot, "primary");
        let wanted = if iteration % 2 == 0 { "right" } else { "left" };
        let target = ep["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == wanted)
            .unwrap();
        let args = route_args(ep, target, "session", &ep["clients"][0]);
        let started = Instant::now();
        assert_route(config.path(), args, "success");
        samples.push(started.elapsed());
    }
    samples.sort();
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).saturating_sub(1)];
    let max = *samples.last().unwrap();
    println!("route_30 p50={p50:?} p95={p95:?} max={max:?}");
    assert!(p95 <= Duration::from_millis(250), "route p95 was {p95:?}");
}

#[test]
#[serial_test::serial]
fn hung_endpoint_preserves_healthy_results_within_native_fallback_budget() {
    let good = TmuxServer::new("healthy");
    let hung_dir = tempfile::Builder::new()
        .prefix("txhung")
        .tempdir_in("/tmp")
        .unwrap();
    let hung_path = hung_dir.path().join("hung.sock");
    let listener = UnixListener::bind(&hung_path).unwrap();
    let holder = thread::spawn(move || {
        if let Ok((_stream, _)) = listener.accept() {
            thread::sleep(Duration::from_secs(1));
        }
    });
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "[switcher]\nenabled=true\ndeadline_ms=400\nendpoint_soft_timeout_ms=350\nmax_concurrency=2\n[[switcher.endpoints]]\nselector={:?}\nalias=\"healthy\"\n[[switcher.endpoints]]\nselector={:?}\nalias=\"hung\"\n",
            format!("path:{}", good.socket_path().display()),
            format!("path:{}", hung_path.display())
        ),
    )
    .unwrap();
    let warm = Command::new(env!("CARGO_BIN_EXE_tmx"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(warm.status.success());
    let started = Instant::now();
    let output = tmx(
        config.path(),
        &[
            "inventory".into(),
            "--schema".into(),
            "1".into(),
            "--json".into(),
            "--deadline-ms".into(),
            "400".into(),
        ],
    );
    let elapsed = started.elapsed();
    assert!(output.status.success());
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(endpoint(&snapshot, "healthy")["status"], "available");
    assert_eq!(endpoint(&snapshot, "hung")["status"], "timeout");
    assert!(
        elapsed <= Duration::from_millis(500),
        "fallback took {elapsed:?}"
    );
    let leaked = Command::new("pgrep")
        .args(["-f", &hung_path.to_string_lossy()])
        .status()
        .unwrap();
    assert!(
        !leaked.success(),
        "hung tmux child survived inventory return"
    );
    holder.join().unwrap();
}

#[test]
#[serial_test::serial]
fn failed_new_attachment_holds_actionable_diagnostic_without_creation() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("missing.sock");
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "[switcher]\nenabled=true\n[[switcher.endpoints]]\nselector={:?}\nalias=\"missing\"\n",
            format!("path:{}", socket.display())
        ),
    )
    .unwrap();
    let snapshot = inventory(config.path());
    let endpoint_id = snapshot["endpoints"][0]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let args = vec![
        "attach".into(),
        "--schema".into(),
        "1".into(),
        "--endpoint-id".into(),
        endpoint_id,
        "--generation".into(),
        "gen_stale".into(),
        "--target-kind".into(),
        "session".into(),
        "--session-id".into(),
        "$1".into(),
        "--hold-on-error-ms".into(),
        "100".into(),
    ];
    let started = Instant::now();
    let output = tmx(config.path(), &args);
    assert!(!output.status.success());
    assert!(started.elapsed() >= Duration::from_millis(90));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unavailable_endpoint"));
    assert!(stderr.contains("tmux attachment failed"));
    assert!(!stderr.contains(&socket.to_string_lossy().to_string()));
    assert!(!socket.exists());
}

#[test]
#[serial_test::serial]
fn missing_path_endpoint_inventory_never_creates_a_server_or_socket() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("missing.sock");
    let config = tempfile::tempdir().unwrap();
    std::fs::write(
        config.path().join("config.toml"),
        format!(
            "[switcher]\nenabled=true\n[[switcher.endpoints]]\nselector={:?}\nalias=\"missing\"\n",
            format!("path:{}", socket.display())
        ),
    )
    .unwrap();
    let snapshot = inventory(config.path());
    assert_eq!(snapshot["complete"], false);
    assert_eq!(snapshot["endpoints"][0]["status"], "unavailable_endpoint");
    assert!(!snapshot
        .to_string()
        .contains(&socket.to_string_lossy().to_string()));
    assert!(!socket.exists());
}
