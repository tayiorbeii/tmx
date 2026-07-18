use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::palette::ROW_SEP;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiProfile {
    Desktop,
    Mobile,
}

impl UiProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            UiProfile::Desktop => "desktop",
            UiProfile::Mobile => "mobile",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    Session,
    Window,
    Pane,
    Project,
    Action,
    Layout,
}

impl TargetKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TargetKind::Session => "session",
            TargetKind::Window => "window",
            TargetKind::Pane => "pane",
            TargetKind::Project => "project",
            TargetKind::Action => "action",
            TargetKind::Layout => "layout",
        }
    }

    pub fn parse_kind(s: &str) -> Option<Self> {
        match s {
            "session" => Some(Self::Session),
            "window" => Some(Self::Window),
            "pane" => Some(Self::Pane),
            "project" => Some(Self::Project),
            "action" => Some(Self::Action),
            "layout" => Some(Self::Layout),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PaletteRow {
    pub row_id: String,
    pub row_type: TargetKind,
    pub display: String,
    pub target_kind: Option<TargetKind>,
    pub target_id: Option<String>,
    pub search_blob: String,
}

impl PaletteRow {
    pub fn new(
        row_id: impl Into<String>,
        row_type: TargetKind,
        display: impl Into<String>,
        target_kind: Option<TargetKind>,
        target_id: Option<String>,
        search_blob: impl Into<String>,
    ) -> Self {
        Self {
            row_id: row_id.into(),
            row_type,
            display: display.into(),
            target_kind,
            target_id,
            search_blob: search_blob.into(),
        }
    }

    pub fn encode(&self) -> String {
        [
            sanitize_field(&self.row_id),
            sanitize_field(self.row_type.as_str()),
            sanitize_field(&self.display),
            sanitize_field(
                self.target_kind
                    .as_ref()
                    .map(TargetKind::as_str)
                    .unwrap_or(""),
            ),
            sanitize_field(self.target_id.as_deref().unwrap_or("")),
            sanitize_field(&self.search_blob),
        ]
        .join(ROW_SEP)
    }

    pub fn decode(line: &str) -> Option<Self> {
        let parts: Vec<_> = line.split(ROW_SEP).collect();
        if parts.len() < 6 {
            return None;
        }
        Some(Self {
            row_id: parts[0].to_string(),
            row_type: TargetKind::parse_kind(parts[1])?,
            display: parts[2].to_string(),
            target_kind: if parts[3].is_empty() {
                None
            } else {
                TargetKind::parse_kind(parts[3])
            },
            target_id: if parts[4].is_empty() {
                None
            } else {
                Some(parts[4].to_string())
            },
            search_blob: parts[5].to_string(),
        })
    }
}

fn sanitize_field(s: &str) -> String {
    s.replace(['\n', '\r', '\t'], " ")
        .replace(ROW_SEP, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, Default)]
pub struct CurrentTarget {
    pub session_id: String,
    pub session_name: String,
    pub session_path: String,
    pub window_id: String,
    pub window_name: String,
    pub window_index: String,
    pub pane_id: String,
    pub pane_title: String,
    pub pane_command: String,
    pub cwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub scope: String,
    pub stable_key: String,
    pub note: String,
    pub updated_at: DateTime<Utc>,
}
