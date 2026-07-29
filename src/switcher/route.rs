use std::ffi::{OsStr, OsString};
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::SwitcherConfig;
use crate::switcher::contract::{
    ClientFingerprint, ClientRecord, Diagnostic, EndpointResult, EndpointStatus, RouteMode,
    RouteOutcome, RouteRequest, RouteResponse, TargetIdentity, TargetKind, ROUTE_SCHEMA,
    SCHEMA_MAJOR,
};
use crate::switcher::endpoint::{verify_socket, RegisteredEndpoint};
use crate::switcher::inventory::{collect_one_for_route, find_endpoint};
use crate::switcher::parser;
use crate::switcher::runner::{run_bounded, RunnerError};

pub fn execute_route(config: &SwitcherConfig, request: RouteRequest) -> RouteResponse {
    let started = Instant::now();
    let deadline_ms = request.deadline_ms.clamp(25, 2_000);
    let deadline = started + Duration::from_millis(deadline_ms);
    let plan_kind = match request.mode {
        RouteMode::PreferClient => "mapped_client",
        RouteMode::NewAttachment => "new_attachment",
    };
    let respond = |outcome, diagnostics| {
        RouteResponse::new(
            request.request_id.clone(),
            plan_kind,
            outcome,
            started.elapsed().as_millis() as u64,
            diagnostics,
        )
    };

    if request.schema.name != ROUTE_SCHEMA || request.schema.major != SCHEMA_MAJOR {
        return respond(
            RouteOutcome::IncompatibleSchema,
            vec![diagnostic(
                "incompatible_schema",
                "unsupported route schema",
            )],
        );
    }
    if request.host_domain != "local" {
        return respond(
            RouteOutcome::UntrustedEndpoint,
            vec![diagnostic(
                "untrusted_host_domain",
                "v1 routes accept only the explicit local host domain",
            )],
        );
    }
    if request.mode != RouteMode::PreferClient || request.client.is_none() {
        return respond(
            RouteOutcome::CommandFailure,
            vec![diagnostic(
                "invalid_route_mode",
                "mapped-client route requires a complete client fingerprint",
            )],
        );
    }
    let endpoint = match find_endpoint(config, &request.endpoint_id) {
        Ok(endpoint) => endpoint,
        Err(_) => {
            return respond(
                RouteOutcome::UntrustedEndpoint,
                vec![diagnostic(
                    "untrusted_endpoint",
                    "endpoint ID is not in the trusted registry",
                )],
            )
        }
    };
    if verify_socket(&endpoint).is_err() {
        return respond(
            RouteOutcome::UntrustedEndpoint,
            vec![diagnostic(
                "untrusted_endpoint",
                "socket identity changed before route validation",
            )],
        );
    }
    if let Err(code) = wait_for_test_barrier(deadline) {
        return respond(RouteOutcome::Timeout, vec![diagnostic("timeout", code)]);
    }
    if Instant::now() >= deadline {
        return respond(
            RouteOutcome::Timeout,
            vec![diagnostic(
                "timeout",
                "route deadline elapsed before mutation",
            )],
        );
    }
    let final_snapshot = collect_one_for_route(&endpoint, config, deadline);
    if let Some(outcome) = endpoint_failure_outcome(&final_snapshot) {
        return respond(outcome, final_snapshot.diagnostics);
    }
    let generation = match &final_snapshot.generation {
        Some(generation) if generation.token == request.expected_generation => generation,
        _ => {
            return respond(
                RouteOutcome::StaleTarget,
                vec![diagnostic(
                    "stale_generation",
                    "tmux server generation changed immediately before mutation",
                )],
            )
        }
    };
    if !target_exists(&final_snapshot, &request.target) {
        return respond(
            RouteOutcome::StaleTarget,
            vec![diagnostic(
                "stale_target",
                "the selected tmux target or its parent identity no longer exists",
            )],
        );
    }
    let expected_client = request.client.as_ref().expect("validated above");
    if expected_client.endpoint_id != endpoint.endpoint_id
        || expected_client.generation != generation.token
    {
        return respond(
            RouteOutcome::StaleClient,
            vec![diagnostic(
                "stale_client",
                "client fingerprint does not belong to the selected endpoint generation",
            )],
        );
    }
    let Some(fresh_client) = final_snapshot
        .clients
        .iter()
        .find(|client| client_matches(client, expected_client))
    else {
        return respond(
            RouteOutcome::StaleClient,
            vec![diagnostic(
                "stale_client",
                "client fingerprint changed immediately before mutation",
            )],
        );
    };
    if client_is_at_target(fresh_client, &request.target) {
        return respond(RouteOutcome::Success, Vec::new());
    }

    let argv = mapped_client_argv(&endpoint, &expected_client.client_name, &request.target);
    let output = run_bounded(
        OsStr::new("tmux"),
        argv.into_iter().map(OsString::from),
        deadline,
        64 * 1024,
        16 * 1024,
    );
    match output {
        Err(RunnerError::Timeout) => {
            return respond(
                RouteOutcome::Timeout,
                vec![diagnostic("timeout", "tmux route command timed out")],
            )
        }
        Err(error) => {
            return respond(
                RouteOutcome::CommandFailure,
                vec![diagnostic("command_failure", &error.to_string())],
            )
        }
        Ok(output) if !output.status.success() => {
            let message = redact_endpoint_message(&output.sanitized_stderr(), &endpoint);
            return respond(
                if message.contains("can't find") || message.contains("no such") {
                    RouteOutcome::StaleTarget
                } else {
                    RouteOutcome::CommandFailure
                },
                vec![diagnostic(
                    "command_failure",
                    if message.is_empty() {
                        "tmux route command failed"
                    } else {
                        &message
                    },
                )],
            );
        }
        Ok(_) => {}
    }
    if let Err(message) = wait_for_barrier("TMX_TEST_POST_BARRIER_FILE", deadline) {
        return respond(
            RouteOutcome::PartialSuccess,
            vec![diagnostic("postcondition_timeout", message)],
        );
    }

    match client_reached_target(
        &endpoint,
        expected_client,
        &request.target,
        &request.expected_generation,
        deadline,
    ) {
        Ok(true) => respond(RouteOutcome::Success, Vec::new()),
        Ok(false) => respond(
            RouteOutcome::PartialSuccess,
            vec![diagnostic(
                "postcondition_mismatch",
                "tmux accepted the route but the exact client postcondition could not be proved",
            )],
        ),
        Err(message) => respond(
            RouteOutcome::PartialSuccess,
            vec![diagnostic("postcondition_query_failed", &message)],
        ),
    }
}

fn client_reached_target(
    endpoint: &RegisteredEndpoint,
    expected_client: &ClientFingerprint,
    target: &TargetIdentity,
    generation: &str,
    deadline: Instant,
) -> Result<bool, String> {
    let mut argv = endpoint.tmux_prefix();
    argv.extend(["list-clients".into(), "-F".into(), parser::client_format()]);
    let output = run_bounded(
        OsStr::new("tmux"),
        argv.into_iter().map(OsString::from),
        deadline,
        64 * 1024,
        16 * 1024,
    )
    .map_err(|error| match error {
        RunnerError::Timeout => "exact-client postcondition query timed out".to_string(),
        other => other.to_string(),
    })?;
    if !output.status.success() {
        let message = redact_endpoint_message(&output.sanitized_stderr(), endpoint);
        return Err(if message.is_empty() {
            "exact-client postcondition query failed".into()
        } else {
            message
        });
    }
    if output.stdout_truncated {
        return Err("exact-client postcondition output exceeded its bound".into());
    }
    let text = output.stdout_text().map_err(|error| error.to_string())?;
    let parsed = parser::parse_clients(
        &endpoint.endpoint_id,
        generation,
        &expected_client.client_uid,
        text,
    );
    if !parsed.diagnostics.is_empty() {
        return Err("exact-client postcondition output was malformed".into());
    }
    Ok(exact_client_is_at_target(
        &parsed.records,
        expected_client,
        target,
    ))
}

pub fn execute_new_attachment(
    config: &SwitcherConfig,
    request: &RouteRequest,
) -> Result<(), RouteResponse> {
    let started = Instant::now();
    let deadline = started + Duration::from_millis(request.deadline_ms.clamp(25, 2_000));
    let fail = |outcome, code: &str, message: &str| {
        RouteResponse::new(
            request.request_id.clone(),
            "new_attachment",
            outcome,
            started.elapsed().as_millis() as u64,
            vec![diagnostic(code, message)],
        )
    };
    if request.schema.name != ROUTE_SCHEMA
        || request.schema.major != SCHEMA_MAJOR
        || request.mode != RouteMode::NewAttachment
    {
        return Err(fail(
            RouteOutcome::IncompatibleSchema,
            "incompatible_schema",
            "unsupported attachment request",
        ));
    }
    if request.host_domain != "local" {
        return Err(fail(
            RouteOutcome::UntrustedEndpoint,
            "untrusted_host_domain",
            "v1 attachments accept only the explicit local host domain",
        ));
    }
    let endpoint = find_endpoint(config, &request.endpoint_id).map_err(|_| {
        fail(
            RouteOutcome::UntrustedEndpoint,
            "untrusted_endpoint",
            "endpoint ID is not in the trusted registry",
        )
    })?;
    let snapshot = collect_one_for_route(&endpoint, config, deadline);
    if let Some(outcome) = endpoint_failure_outcome(&snapshot) {
        return Err(RouteResponse::new(
            request.request_id.clone(),
            "new_attachment",
            outcome,
            started.elapsed().as_millis() as u64,
            snapshot.diagnostics,
        ));
    }
    if snapshot.generation.as_ref().map(|value| &value.token) != Some(&request.expected_generation)
        || !target_exists(&snapshot, &request.target)
    {
        return Err(fail(
            RouteOutcome::StaleTarget,
            "stale_target",
            "target or server generation changed before attachment",
        ));
    }
    if let Err(message) = wait_for_test_barrier(deadline) {
        return Err(fail(RouteOutcome::Timeout, "timeout", message));
    }
    if verify_socket(&endpoint).is_err() {
        return Err(fail(
            RouteOutcome::UntrustedEndpoint,
            "untrusted_endpoint",
            "socket identity changed before attachment",
        ));
    }
    let final_snapshot = collect_one_for_route(&endpoint, config, deadline);
    if let Some(outcome) = endpoint_failure_outcome(&final_snapshot) {
        return Err(RouteResponse::new(
            request.request_id.clone(),
            "new_attachment",
            outcome,
            started.elapsed().as_millis() as u64,
            final_snapshot.diagnostics,
        ));
    }
    if final_snapshot.generation.as_ref().map(|value| &value.token)
        != Some(&request.expected_generation)
        || !target_exists(&final_snapshot, &request.target)
    {
        return Err(fail(
            RouteOutcome::StaleTarget,
            "stale_target",
            "target or server generation changed immediately before attachment",
        ));
    }

    if Instant::now() >= deadline {
        return Err(fail(
            RouteOutcome::Timeout,
            "timeout",
            "attachment deadline elapsed before process creation",
        ));
    }
    let argv = attachment_argv(&endpoint, &request.target);
    let status = Command::new("tmux")
        .args(argv)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            fail(
                RouteOutcome::CommandFailure,
                "command_failure",
                &format!("failed to start tmux attachment: {error}"),
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        eprintln!(
            "tmx: attachment failed; verify the endpoint is running and retry (status {status})"
        );
        Err(fail(
            RouteOutcome::CommandFailure,
            "command_failure",
            "tmux attachment exited before opening the selected target",
        ))
    }
}

fn mapped_client_argv(
    endpoint: &RegisteredEndpoint,
    client_name: &str,
    target: &TargetIdentity,
) -> Vec<String> {
    let mut argv = endpoint.tmux_prefix();
    argv.extend([
        "switch-client".into(),
        "-c".into(),
        client_name.into(),
        "-t".into(),
        tmux_target(target),
    ]);
    argv
}

pub fn attachment_argv(endpoint: &RegisteredEndpoint, target: &TargetIdentity) -> Vec<String> {
    let mut argv = endpoint.tmux_prefix();
    match target.kind {
        TargetKind::Session => argv.extend([
            "attach-session".into(),
            "-t".into(),
            target.session_id.clone(),
        ]),
        TargetKind::Window => {
            argv.extend([
                "select-window".into(),
                "-t".into(),
                tmux_target(target),
                ";".into(),
                "attach-session".into(),
                "-t".into(),
                target.session_id.clone(),
            ]);
        }
        TargetKind::Pane => {
            let window_target = format!(
                "{}:{}",
                target.session_id,
                target.window_id.as_deref().unwrap_or_default()
            );
            argv.extend([
                "select-window".into(),
                "-t".into(),
                window_target,
                ";".into(),
                "select-pane".into(),
                "-t".into(),
                target.pane_id.clone().unwrap_or_default(),
                ";".into(),
                "attach-session".into(),
                "-t".into(),
                target.session_id.clone(),
            ]);
        }
    }
    argv
}

fn target_exists(snapshot: &EndpointResult, target: &TargetIdentity) -> bool {
    match target.kind {
        TargetKind::Session => snapshot
            .sessions
            .iter()
            .any(|session| session.session_id == target.session_id),
        TargetKind::Window => target.window_id.as_ref().is_some_and(|window_id| {
            snapshot.windows.iter().any(|window| {
                window.session_id == target.session_id && window.window_id == *window_id
            })
        }),
        TargetKind::Pane => match (&target.window_id, &target.pane_id) {
            (Some(window_id), Some(pane_id)) => snapshot.panes.iter().any(|pane| {
                pane.session_id == target.session_id
                    && pane.window_id == *window_id
                    && pane.pane_id == *pane_id
            }),
            _ => false,
        },
    }
}

fn client_matches(fresh: &ClientRecord, expected: &ClientFingerprint) -> bool {
    fresh.endpoint_id == expected.endpoint_id
        && fresh.generation == expected.generation
        && fresh.client_name == expected.client_name
        && fresh.client_tty == expected.client_tty
        && fresh.client_pid == expected.client_pid
        && fresh.client_created == expected.client_created
        && fresh.client_uid == expected.client_uid
}

fn exact_client_is_at_target(
    clients: &[ClientRecord],
    expected: &ClientFingerprint,
    target: &TargetIdentity,
) -> bool {
    clients
        .iter()
        .find(|client| client_matches(client, expected))
        .is_some_and(|client| client_is_at_target(client, target))
}

fn client_is_at_target(client: &ClientRecord, target: &TargetIdentity) -> bool {
    if client.attached_session_id != target.session_id {
        return false;
    }
    match target.kind {
        TargetKind::Session => true,
        TargetKind::Window => client.current_window_id == target.window_id,
        TargetKind::Pane => {
            client.current_window_id == target.window_id && client.current_pane_id == target.pane_id
        }
    }
}

fn tmux_target(target: &TargetIdentity) -> String {
    match target.kind {
        TargetKind::Session => target.session_id.clone(),
        TargetKind::Window => format!(
            "{}:{}",
            target.session_id,
            target.window_id.as_deref().unwrap_or_default()
        ),
        TargetKind::Pane => target.pane_id.clone().unwrap_or_default(),
    }
}

fn endpoint_failure_outcome(endpoint: &EndpointResult) -> Option<RouteOutcome> {
    match endpoint.status {
        EndpointStatus::Available | EndpointStatus::Partial if endpoint.generation.is_some() => {
            None
        }
        EndpointStatus::Available => Some(RouteOutcome::UnavailableEndpoint),
        EndpointStatus::UntrustedEndpoint => Some(RouteOutcome::UntrustedEndpoint),
        EndpointStatus::Incompatible => Some(RouteOutcome::IncompatibleSchema),
        EndpointStatus::Timeout => Some(RouteOutcome::Timeout),
        EndpointStatus::UnavailableEndpoint | EndpointStatus::Partial => {
            Some(RouteOutcome::UnavailableEndpoint)
        }
    }
}

fn redact_endpoint_message(message: &str, endpoint: &RegisteredEndpoint) -> String {
    message.replace(
        &endpoint.socket_path.to_string_lossy().to_string(),
        "<socket>",
    )
}

fn diagnostic(code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message: message
            .chars()
            .map(|ch| if ch.is_control() { ' ' } else { ch })
            .take(512)
            .collect(),
        endpoint_id: None,
    }
}

fn wait_for_test_barrier(deadline: Instant) -> Result<(), &'static str> {
    wait_for_barrier("TMX_TEST_BARRIER_FILE", deadline)
}

fn wait_for_barrier(variable: &str, deadline: Instant) -> Result<(), &'static str> {
    if std::env::var_os("TMX_TEST_MODE").as_deref() != Some(OsStr::new("1")) {
        return Ok(());
    }
    let Some(path) = std::env::var_os(variable) else {
        return Ok(());
    };
    let ready = std::path::PathBuf::from(path);
    let release = ready.with_extension("continue");
    fs::write(&ready, b"ready").map_err(|_| "failed to create test barrier")?;
    while Instant::now() < deadline {
        if release.exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(2));
    }
    Err("test barrier exceeded route deadline")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::switcher::endpoint::EndpointSelector;
    use std::path::PathBuf;

    fn endpoint() -> RegisteredEndpoint {
        RegisteredEndpoint {
            endpoint_id: "ep".into(),
            alias: "work".into(),
            selector: EndpointSelector::Path(PathBuf::from("/tmp/tmux socket")),
            trust_source: "test".into(),
            socket_path: PathBuf::from("/tmp/tmux socket"),
        }
    }

    #[test]
    fn attachment_argv_is_typed_and_attach_is_last() {
        let target = TargetIdentity {
            kind: TargetKind::Pane,
            session_id: "$1".into(),
            window_id: Some("@2".into()),
            pane_id: Some("%3".into()),
        };
        let args = attachment_argv(&endpoint(), &target);
        assert_eq!(
            args,
            [
                "-S",
                "/tmp/tmux socket",
                "select-window",
                "-t",
                "$1:@2",
                ";",
                "select-pane",
                "-t",
                "%3",
                ";",
                "attach-session",
                "-t",
                "$1"
            ]
        );
    }

    #[test]
    fn mapped_client_argv_preserves_hostile_client_as_one_argument() {
        let target = TargetIdentity {
            kind: TargetKind::Pane,
            session_id: "$1".into(),
            window_id: Some("@2".into()),
            pane_id: Some("%3".into()),
        };
        for client in [
            "/dev/tty with spaces",
            "quote'\"",
            "$(touch nope)",
            "`echo nope`",
            "*?[abc]",
            "-leading-option",
            "line\nfeed",
            "back\\slash;semi",
        ] {
            let args = mapped_client_argv(&endpoint(), client, &target);
            assert_eq!(args.len(), 7);
            assert_eq!(args[4], client);
            assert_eq!(args.last().unwrap(), "%3");
            assert!(!args.iter().any(|arg| arg == "sh"));
        }
    }

    #[test]
    fn postcondition_rejects_replacement_that_reuses_client_name() {
        let expected = ClientFingerprint {
            endpoint_id: "ep".into(),
            generation: "generation".into(),
            client_name: "/dev/pts/1".into(),
            client_tty: "/dev/pts/1".into(),
            client_pid: "100".into(),
            client_created: "1000".into(),
            client_uid: "501".into(),
        };
        let target = TargetIdentity {
            kind: TargetKind::Session,
            session_id: "$2".into(),
            window_id: None,
            pane_id: None,
        };
        let replacement = ClientRecord {
            endpoint_id: expected.endpoint_id.clone(),
            generation: expected.generation.clone(),
            client_name: expected.client_name.clone(),
            client_pid: "200".into(),
            client_created: "2000".into(),
            client_tty: expected.client_tty.clone(),
            client_uid: expected.client_uid.clone(),
            attached_session_id: target.session_id.clone(),
            current_window_id: Some("@2".into()),
            current_pane_id: Some("%2".into()),
            activity: None,
            flags: None,
        };
        assert!(!exact_client_is_at_target(
            std::slice::from_ref(&replacement),
            &expected,
            &target
        ));

        let original = ClientRecord {
            client_pid: expected.client_pid.clone(),
            client_created: expected.client_created.clone(),
            ..replacement
        };
        assert!(exact_client_is_at_target(&[original], &expected, &target));
    }

    #[test]
    fn labels_and_shell_metacharacters_never_enter_route_argv() {
        let target = TargetIdentity {
            kind: TargetKind::Session,
            session_id: "$1;$(touch nope)".into(),
            window_id: None,
            pane_id: None,
        };
        let args = attachment_argv(&endpoint(), &target);
        assert_eq!(args.last().unwrap(), "$1;$(touch nope)");
        assert!(!args.iter().any(|arg| arg == "sh" || arg == "-c"));
    }
}
