use std::ffi::{OsStr, OsString};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::config::SwitcherConfig;
use crate::switcher::contract::{
    AppliedLimits, Diagnostic, EndpointResult, EndpointStatus, Generation, InventoryEnvelope,
    SchemaVersion,
};
use crate::switcher::endpoint::{
    register_endpoints, verify_socket, RegisteredEndpoint, SocketIdentity,
};
use crate::switcher::parser;
use crate::switcher::runner::{run_bounded, RunnerError};

const HARD_MAX_ENDPOINTS: usize = 32;
const HARD_MAX_TARGETS: usize = 10_000;
const HARD_MAX_STDOUT: usize = 4 * 1024 * 1024;
const HARD_MAX_STDERR: usize = 16 * 1024;
const HARD_MAX_CONCURRENCY: usize = 4;
const MIN_DEADLINE_MS: u64 = 100;
const MAX_DEADLINE_MS: u64 = 2_000;

#[derive(Debug, Clone)]
struct EffectiveLimits {
    deadline_ms: u64,
    endpoint_soft_timeout_ms: u64,
    max_endpoints: usize,
    max_targets: usize,
    max_stdout: usize,
    max_stderr: usize,
    concurrency: usize,
}

impl EffectiveLimits {
    fn from_config(config: &SwitcherConfig, deadline_override: Option<u64>) -> Self {
        Self {
            deadline_ms: deadline_override
                .unwrap_or(config.deadline_ms)
                .clamp(MIN_DEADLINE_MS, MAX_DEADLINE_MS),
            endpoint_soft_timeout_ms: config.endpoint_soft_timeout_ms.clamp(25, MAX_DEADLINE_MS),
            max_endpoints: config.max_endpoints.clamp(1, HARD_MAX_ENDPOINTS),
            max_targets: config.max_targets.clamp(1, HARD_MAX_TARGETS),
            max_stdout: config
                .max_stdout_bytes_per_endpoint
                .clamp(1024, HARD_MAX_STDOUT),
            max_stderr: config
                .max_stderr_bytes_per_endpoint
                .clamp(256, HARD_MAX_STDERR),
            concurrency: config.max_concurrency.clamp(1, HARD_MAX_CONCURRENCY),
        }
    }

    fn applied(&self) -> AppliedLimits {
        AppliedLimits {
            deadline_ms: self.deadline_ms,
            max_endpoints: self.max_endpoints,
            max_targets: self.max_targets,
            max_stdout_bytes_per_endpoint: self.max_stdout,
            max_stderr_bytes_per_endpoint: self.max_stderr,
            max_concurrency: self.concurrency,
        }
    }
}

pub fn collect_inventory(
    config: &SwitcherConfig,
    request_id: String,
    deadline_override: Option<u64>,
) -> InventoryEnvelope {
    let limits = EffectiveLimits::from_config(config, deadline_override);
    let started = Instant::now();
    let global_deadline = started + Duration::from_millis(limits.deadline_ms);
    let mut top_diagnostics = Vec::new();
    let mut endpoints = match register_endpoints(config) {
        Ok(registration) => {
            if registration.truncated {
                top_diagnostics.push(Diagnostic {
                    code: "endpoint_limit_reached".into(),
                    message: "endpoint inventory was truncated at the configured limit".into(),
                    endpoint_id: None,
                });
            }
            collect_concurrently(registration.endpoints, &limits, global_deadline)
        }
        Err(error) => {
            top_diagnostics.push(Diagnostic {
                code: "invalid_endpoint_configuration".into(),
                message: bounded_message(&error.to_string()),
                endpoint_id: None,
            });
            Vec::new()
        }
    };

    endpoints.sort_by(|a, b| a.endpoint_id.cmp(&b.endpoint_id));
    apply_target_limit(&mut endpoints, limits.max_targets, &mut top_diagnostics);
    let complete = top_diagnostics.is_empty()
        && !endpoints.is_empty()
        && endpoints
            .iter()
            .all(|endpoint| endpoint.status == EndpointStatus::Available);

    let mut capabilities = vec![
        "clients".into(),
        "endpoint_generation".into(),
        "multi_endpoint".into(),
        "route_pane".into(),
        "route_session".into(),
        "route_window".into(),
    ];
    if config.enabled {
        capabilities.push("augmentation_enabled".into());
    }

    InventoryEnvelope {
        schema: SchemaVersion::inventory(),
        request_id,
        producer_version: env!("CARGO_PKG_VERSION").into(),
        generated_at: Utc::now().to_rfc3339(),
        applied_limits: limits.applied(),
        complete,
        capabilities,
        endpoints,
        diagnostics: top_diagnostics,
    }
}

fn collect_concurrently(
    registered: Vec<RegisteredEndpoint>,
    limits: &EffectiveLimits,
    global_deadline: Instant,
) -> Vec<EndpointResult> {
    let mut all = Vec::with_capacity(registered.len());
    for chunk in registered.chunks(limits.concurrency) {
        if Instant::now() >= global_deadline {
            all.extend(chunk.iter().cloned().map(timeout_result));
            continue;
        }
        let (send, receive) = mpsc::channel();
        let mut handles = Vec::new();
        let owned_endpoints = chunk.to_vec();
        for endpoint in owned_endpoints {
            let send = send.clone();
            let limits = limits.clone();
            handles.push(thread::spawn(move || {
                let result = collect_endpoint(&endpoint, &limits, global_deadline);
                let _ = send.send(result);
            }));
        }
        drop(send);
        all.extend(receive);
        for handle in handles {
            let _ = handle.join();
        }
    }
    all
}

fn collect_endpoint(
    endpoint: &RegisteredEndpoint,
    limits: &EffectiveLimits,
    global_deadline: Instant,
) -> EndpointResult {
    if !endpoint.socket_path.exists() {
        return endpoint_error(
            endpoint,
            EndpointStatus::UnavailableEndpoint,
            "unavailable_endpoint",
            "configured tmux endpoint is not available",
        );
    }
    let identity =
        match verify_socket(endpoint) {
            Ok(identity) => identity,
            Err(_error) => return endpoint_error(
                endpoint,
                EndpointStatus::UntrustedEndpoint,
                "untrusted_endpoint",
                "configured endpoint failed socket type, ownership, mode, or symlink trust checks",
            ),
        };
    let endpoint_deadline = global_deadline
        .min(Instant::now() + Duration::from_millis(limits.endpoint_soft_timeout_ms));
    let mut budget = OutputBudget::new(limits.max_stdout, limits.max_stderr);
    let generation = match read_generation(endpoint, &identity, endpoint_deadline, &mut budget) {
        Ok(generation) => generation,
        Err(CommandIssue::Timeout) => return timeout_result(endpoint.clone()),
        Err(CommandIssue::Untrusted(message)) => {
            return endpoint_error(
                endpoint,
                EndpointStatus::UntrustedEndpoint,
                "untrusted_endpoint",
                &message,
            )
        }
        Err(CommandIssue::Failure(_message)) => {
            return endpoint_error(
                endpoint,
                EndpointStatus::UnavailableEndpoint,
                "unavailable_endpoint",
                "configured tmux endpoint could not be queried",
            )
        }
    };

    if !tmux_version_supported(&generation.tmux_version) {
        return EndpointResult {
            host_domain: "local".into(),
            endpoint_id: endpoint.endpoint_id.clone(),
            alias: endpoint.alias.clone(),
            selector_kind: endpoint.selector.kind().into(),
            trust_source: endpoint.trust_source.clone(),
            generation: Some(generation),
            status: EndpointStatus::Incompatible,
            sessions: Vec::new(),
            windows: Vec::new(),
            panes: Vec::new(),
            clients: Vec::new(),
            diagnostics: vec![Diagnostic {
                code: "incompatible_version".into(),
                message: "tmux 3.2 or newer is required for switcher augmentation".into(),
                endpoint_id: Some(endpoint.endpoint_id.clone()),
            }],
        };
    }

    let generation_token = generation.token.clone();
    let mut diagnostics = Vec::new();
    let session_output = run_inventory_command(
        endpoint,
        ["list-sessions", "-F", &parser::session_format()],
        endpoint_deadline,
        &mut budget,
        &mut diagnostics,
    );
    let window_output = run_inventory_command(
        endpoint,
        ["list-windows", "-a", "-F", &parser::window_format()],
        endpoint_deadline,
        &mut budget,
        &mut diagnostics,
    );
    let pane_output = run_inventory_command(
        endpoint,
        ["list-panes", "-a", "-F", &parser::pane_format()],
        endpoint_deadline,
        &mut budget,
        &mut diagnostics,
    );
    let client_output = run_inventory_command(
        endpoint,
        ["list-clients", "-F", &parser::client_format()],
        endpoint_deadline,
        &mut budget,
        &mut diagnostics,
    );

    let mut sessions = parser::parse_sessions(
        &endpoint.endpoint_id,
        &generation_token,
        session_output.as_deref().unwrap_or(""),
    );
    let mut windows = parser::parse_windows(
        &endpoint.endpoint_id,
        &generation_token,
        window_output.as_deref().unwrap_or(""),
    );
    let mut panes = parser::parse_panes(
        &endpoint.endpoint_id,
        &generation_token,
        pane_output.as_deref().unwrap_or(""),
    );
    let mut clients = parser::parse_clients(
        &endpoint.endpoint_id,
        &generation_token,
        &identity.uid.to_string(),
        client_output.as_deref().unwrap_or(""),
    );
    diagnostics.append(&mut sessions.diagnostics);
    diagnostics.append(&mut windows.diagnostics);
    diagnostics.append(&mut panes.diagnostics);
    diagnostics.append(&mut clients.diagnostics);
    diagnostics.extend(parser::validate_hierarchy(
        &endpoint.endpoint_id,
        &sessions.records,
        &mut windows.records,
        &mut panes.records,
    ));

    sessions
        .records
        .sort_by(|a, b| a.session_id.cmp(&b.session_id));
    windows
        .records
        .sort_by(|a, b| (&a.session_id, &a.window_id).cmp(&(&b.session_id, &b.window_id)));
    panes.records.sort_by(|a, b| {
        (&a.session_id, &a.window_id, &a.pane_id).cmp(&(&b.session_id, &b.window_id, &b.pane_id))
    });
    clients
        .records
        .sort_by(|a, b| a.client_name.cmp(&b.client_name));

    let status = if diagnostics.is_empty() {
        EndpointStatus::Available
    } else if diagnostics.iter().any(|item| item.code == "timeout") {
        EndpointStatus::Timeout
    } else {
        EndpointStatus::Partial
    };
    EndpointResult {
        host_domain: "local".into(),
        endpoint_id: endpoint.endpoint_id.clone(),
        alias: endpoint.alias.clone(),
        selector_kind: endpoint.selector.kind().into(),
        trust_source: endpoint.trust_source.clone(),
        generation: Some(generation),
        status,
        sessions: sessions.records,
        windows: windows.records,
        panes: panes.records,
        clients: clients.records,
        diagnostics,
    }
}

fn read_generation(
    endpoint: &RegisteredEndpoint,
    identity: &SocketIdentity,
    deadline: Instant,
    budget: &mut OutputBudget,
) -> Result<Generation, CommandIssue> {
    let format = parser::generation_format();
    let output = run_tmux(
        endpoint,
        ["display-message", "-p", &format],
        deadline,
        budget,
    )?;
    if !output.status.success() {
        return Err(CommandIssue::Failure(if output.stderr.is_empty() {
            "tmux generation probe failed".into()
        } else {
            output.stderr
        }));
    }
    let fields = output
        .stdout
        .trim_end_matches('\n')
        .split(parser::FIELD_SEPARATOR)
        .collect::<Vec<_>>();
    if fields.len() != 4
        || !fields[0].bytes().all(|byte| byte.is_ascii_digit())
        || !fields[1].bytes().all(|byte| byte.is_ascii_digit())
        || fields[0].is_empty()
        || fields[1].is_empty()
    {
        return Err(CommandIssue::Failure(
            "tmux generation probe returned malformed fields".into(),
        ));
    }
    let reported = std::path::PathBuf::from(fields[2]);
    let canonical_reported = reported
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .and_then(|parent| reported.file_name().map(|name| parent.join(name)))
        .unwrap_or_else(|| reported.clone());
    if canonical_reported != identity.canonical_path {
        return Err(CommandIssue::Untrusted(
            "tmux reported a different socket identity".into(),
        ));
    }
    let private_socket_identity = identity.stable_text();
    let token = generation_token(&private_socket_identity, fields[0], fields[1], fields[3]);
    let socket_identity = socket_identity_digest(&private_socket_identity);
    Ok(Generation {
        token,
        socket_device: identity.device.to_string(),
        socket_inode: identity.inode.to_string(),
        socket_uid: identity.uid.to_string(),
        server_pid: fields[0].into(),
        server_started: fields[1].into(),
        socket_identity,
        tmux_version: fields[3].into(),
    })
}

fn socket_identity_digest(private_identity: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dev.tmx.socket-identity.v1\0");
    digest.update(private_identity.as_bytes());
    format!("socket_{}", hex::encode(digest.finalize()))
}

fn tmux_version_supported(version: &str) -> bool {
    let numeric = version
        .trim_start_matches("tmux ")
        .split(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
        .next()
        .unwrap_or_default();
    let mut parts = numeric.split('.');
    let major = parts.next().and_then(|value| value.parse::<u64>().ok());
    let minor = parts.next().and_then(|value| value.parse::<u64>().ok());
    matches!((major, minor), (Some(major), Some(minor)) if major > 3 || (major == 3 && minor >= 2))
}

fn generation_token(socket: &str, pid: &str, started: &str, version: &str) -> String {
    let mut digest = Sha256::new();
    for field in [socket, pid, started, version] {
        digest.update(field.as_bytes());
        digest.update([0]);
    }
    format!("gen_{}", hex::encode(digest.finalize()))
}

#[derive(Debug)]
struct OutputBudget {
    stdout_remaining: usize,
    stderr_remaining: usize,
}

impl OutputBudget {
    fn new(stdout: usize, stderr: usize) -> Self {
        Self {
            stdout_remaining: stdout,
            stderr_remaining: stderr,
        }
    }
}

struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
    truncated: bool,
}

#[derive(Debug)]
enum CommandIssue {
    Timeout,
    Failure(String),
    Untrusted(String),
}

fn run_tmux<I, S>(
    endpoint: &RegisteredEndpoint,
    args: I,
    deadline: Instant,
    budget: &mut OutputBudget,
) -> Result<CommandOutput, CommandIssue>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if deadline.saturating_duration_since(Instant::now()) < Duration::from_millis(5) {
        return Err(CommandIssue::Timeout);
    }
    if budget.stdout_remaining == 0 || budget.stderr_remaining == 0 {
        return Err(CommandIssue::Failure(
            "endpoint output budget exhausted".into(),
        ));
    }
    let mut argv = endpoint
        .tmux_prefix()
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    argv.extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
    let output = run_bounded(
        OsStr::new("tmux"),
        argv,
        deadline,
        budget.stdout_remaining,
        budget.stderr_remaining,
    )
    .map_err(|error| match error {
        RunnerError::Timeout => CommandIssue::Timeout,
        other => CommandIssue::Failure(bounded_message(&other.to_string())),
    })?;
    budget.stdout_remaining = budget.stdout_remaining.saturating_sub(output.stdout.len());
    budget.stderr_remaining = budget.stderr_remaining.saturating_sub(output.stderr.len());
    let stdout = output
        .stdout_text()
        .map_err(|_| CommandIssue::Failure("tmux stdout was not valid UTF-8".into()))?
        .to_string();
    Ok(CommandOutput {
        status: output.status,
        stdout,
        stderr: output.sanitized_stderr(),
        truncated: output.stdout_truncated || output.stderr_truncated,
    })
}

fn run_inventory_command<const N: usize>(
    endpoint: &RegisteredEndpoint,
    args: [&str; N],
    deadline: Instant,
    budget: &mut OutputBudget,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<String> {
    match run_tmux(endpoint, args, deadline, budget) {
        Ok(output) if output.status.success() && !output.truncated => Some(output.stdout),
        Ok(output) => {
            diagnostics.push(Diagnostic {
                code: if output.truncated {
                    "output_limit"
                } else {
                    "command_failure"
                }
                .into(),
                message: if output.stderr.is_empty() {
                    "tmux inventory command failed or exceeded its output limit".into()
                } else {
                    redact_endpoint_message(&output.stderr, endpoint)
                },
                endpoint_id: Some(endpoint.endpoint_id.clone()),
            });
            None
        }
        Err(CommandIssue::Timeout) => {
            diagnostics.push(Diagnostic {
                code: "timeout".into(),
                message: "tmux endpoint exceeded its inventory deadline".into(),
                endpoint_id: Some(endpoint.endpoint_id.clone()),
            });
            None
        }
        Err(CommandIssue::Failure(message) | CommandIssue::Untrusted(message)) => {
            diagnostics.push(Diagnostic {
                code: "command_failure".into(),
                message,
                endpoint_id: Some(endpoint.endpoint_id.clone()),
            });
            None
        }
    }
}

fn endpoint_error(
    endpoint: &RegisteredEndpoint,
    status: EndpointStatus,
    code: &str,
    message: &str,
) -> EndpointResult {
    EndpointResult {
        host_domain: "local".into(),
        endpoint_id: endpoint.endpoint_id.clone(),
        alias: endpoint.alias.clone(),
        selector_kind: endpoint.selector.kind().into(),
        trust_source: endpoint.trust_source.clone(),
        generation: None,
        status,
        sessions: Vec::new(),
        windows: Vec::new(),
        panes: Vec::new(),
        clients: Vec::new(),
        diagnostics: vec![Diagnostic {
            code: code.into(),
            message: bounded_message(message),
            endpoint_id: Some(endpoint.endpoint_id.clone()),
        }],
    }
}

fn timeout_result(endpoint: RegisteredEndpoint) -> EndpointResult {
    endpoint_error(
        &endpoint,
        EndpointStatus::Timeout,
        "timeout",
        "tmux endpoint exceeded its inventory deadline",
    )
}

fn apply_target_limit(
    endpoints: &mut [EndpointResult],
    max_targets: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut remaining = max_targets;
    let mut truncated = false;
    fn truncate<T>(items: &mut Vec<T>, remaining: &mut usize) -> bool {
        if items.len() > *remaining {
            items.truncate(*remaining);
            *remaining = 0;
            true
        } else {
            *remaining -= items.len();
            false
        }
    }

    for endpoint in endpoints {
        truncated |= truncate(&mut endpoint.sessions, &mut remaining);
        truncated |= truncate(&mut endpoint.windows, &mut remaining);
        truncated |= truncate(&mut endpoint.panes, &mut remaining);
        if truncated && endpoint.status == EndpointStatus::Available {
            endpoint.status = EndpointStatus::Partial;
        }
    }
    if truncated {
        diagnostics.push(Diagnostic {
            code: "target_limit".into(),
            message: format!("inventory was truncated at {max_targets} targets"),
            endpoint_id: None,
        });
    }
}

fn redact_endpoint_message(message: &str, endpoint: &RegisteredEndpoint) -> String {
    let canonical = endpoint.socket_path.to_string_lossy();
    let mut redacted = message.replace(canonical.as_ref(), "<socket>");
    if let crate::switcher::endpoint::EndpointSelector::Path(original) = &endpoint.selector {
        redacted = redacted.replace(original.to_string_lossy().as_ref(), "<socket>");
    }
    bounded_message(&redacted)
}

fn bounded_message(message: &str) -> String {
    let sanitized = message
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    let compact = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(512).collect()
}

pub fn find_endpoint(config: &SwitcherConfig, endpoint_id: &str) -> Result<RegisteredEndpoint> {
    register_endpoints(config)?
        .endpoints
        .into_iter()
        .find(|endpoint| endpoint.endpoint_id == endpoint_id)
        .ok_or_else(|| anyhow!("endpoint ID is not present in the trusted registry"))
}

pub fn collect_one_for_route(
    endpoint: &RegisteredEndpoint,
    config: &SwitcherConfig,
    deadline: Instant,
) -> EndpointResult {
    let limits = EffectiveLimits::from_config(
        config,
        Some(
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(MIN_DEADLINE_MS as u128, MAX_DEADLINE_MS as u128) as u64,
        ),
    );
    collect_endpoint(endpoint, &limits, deadline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::switcher::contract::{EndpointResult, EndpointStatus};

    fn result(id: &str) -> EndpointResult {
        EndpointResult {
            host_domain: "local".into(),
            endpoint_id: id.into(),
            alias: id.into(),
            selector_kind: "default".into(),
            trust_source: "test".into(),
            generation: None,
            status: EndpointStatus::Available,
            sessions: Vec::new(),
            windows: Vec::new(),
            panes: Vec::new(),
            clients: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn generation_tokens_change_with_server_identity() {
        assert_ne!(
            generation_token("socket", "10", "20", "3.2"),
            generation_token("socket", "10", "21", "3.2")
        );
    }

    #[test]
    fn tmux_floor_parsing_accepts_supported_suffixes_and_rejects_old_versions() {
        assert!(tmux_version_supported("3.2"));
        assert!(tmux_version_supported("3.6a"));
        assert!(tmux_version_supported("tmux 4.0"));
        assert!(!tmux_version_supported("3.1c"));
        assert!(!tmux_version_supported("unknown"));
    }

    #[test]
    fn empty_inventory_is_not_complete() {
        let temp = tempfile::tempdir().unwrap();
        let config = SwitcherConfig {
            endpoints: vec![crate::config::EndpointConfig {
                selector: format!("path:{}", temp.path().join("missing.sock").display()),
                alias: "missing".into(),
            }],
            ..SwitcherConfig::default()
        };
        let envelope = collect_inventory(&config, "test".into(), Some(100));
        assert!(!envelope.complete);
        assert_eq!(
            envelope.endpoints[0].status,
            EndpointStatus::UnavailableEndpoint
        );
    }

    #[test]
    fn endpoint_limit_is_visible_and_only_configured_priority_prefix_is_collected() {
        let temp = tempfile::tempdir().unwrap();
        let config = SwitcherConfig {
            max_endpoints: 1,
            endpoints: vec![
                crate::config::EndpointConfig {
                    selector: format!("path:{}", temp.path().join("first.sock").display()),
                    alias: "first".into(),
                },
                crate::config::EndpointConfig {
                    selector: format!("path:{}", temp.path().join("second.sock").display()),
                    alias: "second".into(),
                },
            ],
            ..SwitcherConfig::default()
        };
        let envelope = collect_inventory(&config, "limit".into(), Some(100));
        assert_eq!(envelope.endpoints.len(), 1);
        assert_eq!(envelope.endpoints[0].alias, "first");
        assert!(envelope
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "endpoint_limit_reached"));
    }

    #[test]
    fn endpoint_diagnostics_redact_original_and_canonical_path_spellings() {
        let endpoint = RegisteredEndpoint {
            endpoint_id: "ep_test".into(),
            alias: "test".into(),
            selector: crate::switcher::endpoint::EndpointSelector::Path(
                "/tmp/link/server.sock".into(),
            ),
            trust_source: "test".into(),
            socket_path: "/private/tmp/real/server.sock".into(),
        };
        let redacted = redact_endpoint_message(
            "failed /tmp/link/server.sock then /private/tmp/real/server.sock",
            &endpoint,
        );
        assert_eq!(redacted, "failed <socket> then <socket>");
    }

    #[test]
    fn canonical_endpoint_order_is_stable() {
        let mut endpoints = [result("z"), result("a")];
        endpoints.sort_by(|a, b| a.endpoint_id.cmp(&b.endpoint_id));
        assert_eq!(endpoints[0].endpoint_id, "a");
    }
}
