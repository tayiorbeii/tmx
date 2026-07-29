use serde::{Deserialize, Serialize};

pub const INVENTORY_SCHEMA: &str = "dev.tmx.inventory";
pub const ROUTE_SCHEMA: &str = "dev.tmx.route";
pub const SCHEMA_MAJOR: u16 = 1;
pub const SCHEMA_MINOR: u16 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaVersion {
    pub name: String,
    pub major: u16,
    pub minor: u16,
}

impl SchemaVersion {
    pub fn inventory() -> Self {
        Self {
            name: INVENTORY_SCHEMA.into(),
            major: SCHEMA_MAJOR,
            minor: SCHEMA_MINOR,
        }
    }

    pub fn route() -> Self {
        Self {
            name: ROUTE_SCHEMA.into(),
            major: SCHEMA_MAJOR,
            minor: SCHEMA_MINOR,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppliedLimits {
    pub deadline_ms: u64,
    pub max_endpoints: usize,
    pub max_targets: usize,
    pub max_stdout_bytes_per_endpoint: usize,
    pub max_stderr_bytes_per_endpoint: usize,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Generation {
    pub token: String,
    pub socket_device: String,
    pub socket_inode: String,
    pub socket_uid: String,
    pub server_pid: String,
    pub server_started: String,
    pub socket_identity: String,
    pub tmux_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointStatus {
    Available,
    Partial,
    UnavailableEndpoint,
    UntrustedEndpoint,
    Incompatible,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub endpoint_id: String,
    pub generation: String,
    pub session_id: String,
    pub name: String,
    pub path: String,
    pub created: String,
    pub activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attached: Option<String>,
    pub attached_count: String,
    pub window_count: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowRecord {
    pub endpoint_id: String,
    pub generation: String,
    pub session_id: String,
    pub window_id: String,
    pub index: String,
    pub name: String,
    pub active: bool,
    pub activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneRecord {
    pub endpoint_id: String,
    pub generation: String,
    pub session_id: String,
    pub window_id: String,
    pub pane_id: String,
    pub index: String,
    pub active: bool,
    pub activity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientRecord {
    pub endpoint_id: String,
    pub generation: String,
    pub client_name: String,
    pub client_pid: String,
    pub client_created: String,
    pub client_tty: String,
    pub client_uid: String,
    pub attached_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EndpointResult {
    pub host_domain: String,
    pub endpoint_id: String,
    pub alias: String,
    pub selector_kind: String,
    pub trust_source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<Generation>,
    pub status: EndpointStatus,
    pub sessions: Vec<SessionRecord>,
    pub windows: Vec<WindowRecord>,
    pub panes: Vec<PaneRecord>,
    pub clients: Vec<ClientRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

impl EndpointResult {
    pub fn target_count(&self) -> usize {
        self.sessions.len() + self.windows.len() + self.panes.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryEnvelope {
    pub schema: SchemaVersion,
    pub request_id: String,
    pub producer_version: String,
    pub generated_at: String,
    pub applied_limits: AppliedLimits,
    pub complete: bool,
    pub capabilities: Vec<String>,
    pub endpoints: Vec<EndpointResult>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Session,
    Window,
    Pane,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Window => "window",
            Self::Pane => "pane",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TargetIdentity {
    pub kind: TargetKind,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientFingerprint {
    pub endpoint_id: String,
    pub generation: String,
    pub client_name: String,
    pub client_tty: String,
    pub client_pid: String,
    pub client_created: String,
    pub client_uid: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteMode {
    PreferClient,
    NewAttachment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteRequest {
    pub schema: SchemaVersion,
    pub request_id: String,
    pub host_domain: String,
    pub endpoint_id: String,
    pub expected_generation: String,
    pub target: TargetIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientFingerprint>,
    pub mode: RouteMode,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteOutcome {
    Success,
    SuccessNewAttachment,
    StaleTarget,
    StaleClient,
    UnavailableEndpoint,
    UntrustedEndpoint,
    IncompatibleSchema,
    Timeout,
    CommandFailure,
    PartialSuccess,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RouteResponse {
    pub schema: SchemaVersion,
    pub request_id: String,
    pub plan_kind: String,
    pub outcome: RouteOutcome,
    pub elapsed_ms: u64,
    pub diagnostics: Vec<Diagnostic>,
}

impl RouteResponse {
    pub fn new(
        request_id: impl Into<String>,
        plan_kind: impl Into<String>,
        outcome: RouteOutcome,
        elapsed_ms: u64,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            schema: SchemaVersion::route(),
            request_id: request_id.into(),
            plan_kind: plan_kind.into(),
            outcome,
            elapsed_ms,
            diagnostics,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_versions_are_explicit() {
        assert_eq!(SchemaVersion::inventory().name, INVENTORY_SCHEMA);
        assert_eq!(SchemaVersion::route().major, 1);
    }

    #[test]
    fn unknown_additive_inventory_fields_are_accepted() {
        let input = r#"{
          "schema":{"name":"dev.tmx.inventory","major":1,"minor":1},
          "request_id":"r","producer_version":"x","generated_at":"0",
          "applied_limits":{"deadline_ms":400,"max_endpoints":32,"max_targets":10000,
            "max_stdout_bytes_per_endpoint":4194304,"max_stderr_bytes_per_endpoint":16384,
            "max_concurrency":4},
          "complete":true,"capabilities":[],"endpoints":[],"diagnostics":[],
          "future_optional":true
        }"#;
        let parsed: InventoryEnvelope = serde_json::from_str(input).unwrap();
        assert_eq!(parsed.schema.minor, 1);
    }
}
