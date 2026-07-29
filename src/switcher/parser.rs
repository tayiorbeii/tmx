use std::collections::HashSet;

use crate::switcher::contract::{
    ClientRecord, Diagnostic, PaneRecord, SessionRecord, WindowRecord,
};

/// Printable tmux-safe framing token. tmux 3.2–3.3 rewrites literal control
/// separators in format strings, so collisions are rejected rather than using
/// an ASCII control byte that is not portable across supported versions.
pub const FIELD_SEPARATOR: &str = "|:tmx:v1:|";
pub const MAX_LABEL_BYTES: usize = 512;
pub const MAX_LONG_FIELD_BYTES: usize = 4 * 1024;

pub fn generation_format() -> String {
    format_fields(&["pid", "start_time", "socket_path", "version"])
}

pub fn session_format() -> String {
    format_fields(&[
        "session_id",
        "session_name",
        "session_path",
        "session_created",
        "session_activity",
        "session_last_attached",
        "session_attached",
        "session_windows",
    ])
}

pub fn window_format() -> String {
    format_fields(&[
        "session_id",
        "window_id",
        "window_index",
        "window_name",
        "window_active",
        "window_activity",
    ])
}

pub fn pane_format() -> String {
    format_fields(&[
        "session_id",
        "window_id",
        "pane_id",
        "pane_index",
        "pane_active",
        "pane_activity",
        "pane_tty",
        "pane_current_path",
        "pane_current_command",
        "pane_title",
    ])
}

pub fn client_format() -> String {
    format_fields(&[
        "client_name",
        "client_pid",
        "client_created",
        "client_tty",
        "session_id",
        "window_id",
        "pane_id",
        "client_activity",
        "client_flags",
    ])
}

fn format_fields(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| format!("#{{{field}}}"))
        .collect::<Vec<_>>()
        .join(FIELD_SEPARATOR)
}

#[derive(Debug)]
pub struct ParseBatch<T> {
    pub records: Vec<T>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> ParseBatch<T> {
    fn new() -> Self {
        Self {
            records: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

pub fn parse_sessions(
    endpoint_id: &str,
    generation: &str,
    output: &str,
) -> ParseBatch<SessionRecord> {
    parse_lines(output, endpoint_id, |fields| {
        exact(fields, 8)?;
        sigil(fields[0], '$')?;
        bounded(fields[1], MAX_LABEL_BYTES, "session_name")?;
        bounded(fields[2], MAX_LONG_FIELD_BYTES, "session_path")?;
        numeric(fields[3], "session_created")?;
        numeric(fields[4], "session_activity")?;
        optional_numeric(fields[5], "session_last_attached")?;
        numeric(fields[6], "session_attached")?;
        numeric(fields[7], "session_windows")?;
        Ok(SessionRecord {
            endpoint_id: endpoint_id.into(),
            generation: generation.into(),
            session_id: fields[0].into(),
            name: fields[1].into(),
            path: fields[2].into(),
            created: fields[3].into(),
            activity: fields[4].into(),
            last_attached: optional(fields[5]),
            attached_count: fields[6].into(),
            window_count: fields[7].into(),
            note: None,
        })
    })
}

pub fn parse_windows(
    endpoint_id: &str,
    generation: &str,
    output: &str,
) -> ParseBatch<WindowRecord> {
    parse_lines(output, endpoint_id, |fields| {
        exact(fields, 6)?;
        sigil(fields[0], '$')?;
        sigil(fields[1], '@')?;
        numeric(fields[2], "window_index")?;
        bounded(fields[3], MAX_LABEL_BYTES, "window_name")?;
        boolean(fields[4], "window_active")?;
        numeric(fields[5], "window_activity")?;
        Ok(WindowRecord {
            endpoint_id: endpoint_id.into(),
            generation: generation.into(),
            session_id: fields[0].into(),
            window_id: fields[1].into(),
            index: fields[2].into(),
            name: fields[3].into(),
            active: fields[4] == "1",
            activity: fields[5].into(),
            note: None,
        })
    })
}

pub fn parse_panes(endpoint_id: &str, generation: &str, output: &str) -> ParseBatch<PaneRecord> {
    parse_lines(output, endpoint_id, |fields| {
        exact(fields, 10)?;
        sigil(fields[0], '$')?;
        sigil(fields[1], '@')?;
        sigil(fields[2], '%')?;
        numeric(fields[3], "pane_index")?;
        boolean(fields[4], "pane_active")?;
        optional_numeric(fields[5], "pane_activity")?;
        bounded(fields[6], MAX_LONG_FIELD_BYTES, "pane_tty")?;
        bounded(fields[7], MAX_LONG_FIELD_BYTES, "pane_path")?;
        bounded(fields[8], MAX_LONG_FIELD_BYTES, "pane_command")?;
        bounded(fields[9], MAX_LABEL_BYTES, "pane_title")?;
        Ok(PaneRecord {
            endpoint_id: endpoint_id.into(),
            generation: generation.into(),
            session_id: fields[0].into(),
            window_id: fields[1].into(),
            pane_id: fields[2].into(),
            index: fields[3].into(),
            active: fields[4] == "1",
            activity: fields[5].into(),
            tty: optional(fields[6]),
            path: optional(fields[7]),
            command: optional(fields[8]),
            title: optional(fields[9]),
            note: None,
        })
    })
}

pub fn parse_clients(
    endpoint_id: &str,
    generation: &str,
    client_uid: &str,
    output: &str,
) -> ParseBatch<ClientRecord> {
    parse_lines(output, endpoint_id, |fields| {
        exact(fields, 9)?;
        required(fields[0], "client_name")?;
        bounded(fields[0], MAX_LONG_FIELD_BYTES, "client_name")?;
        numeric(fields[1], "client_pid")?;
        numeric(fields[2], "client_created")?;
        bounded(fields[3], MAX_LONG_FIELD_BYTES, "client_tty")?;
        numeric(client_uid, "client_uid")?;
        sigil(fields[4], '$')?;
        optional_sigil(fields[5], '@')?;
        optional_sigil(fields[6], '%')?;
        optional_numeric(fields[7], "client_activity")?;
        bounded(fields[8], MAX_LABEL_BYTES, "client_flags")?;
        Ok(ClientRecord {
            endpoint_id: endpoint_id.into(),
            generation: generation.into(),
            client_name: fields[0].into(),
            client_pid: fields[1].into(),
            client_created: fields[2].into(),
            client_tty: fields[3].into(),
            client_uid: client_uid.into(),
            attached_session_id: fields[4].into(),
            current_window_id: optional(fields[5]),
            current_pane_id: optional(fields[6]),
            activity: optional(fields[7]),
            flags: optional(fields[8]),
        })
    })
}

fn parse_lines<T>(
    output: &str,
    endpoint_id: &str,
    mut parse: impl FnMut(&[&str]) -> Result<T, String>,
) -> ParseBatch<T> {
    let mut batch = ParseBatch::new();
    for (index, line) in output.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split(FIELD_SEPARATOR).collect::<Vec<_>>();
        match parse(&fields) {
            Ok(record) => batch.records.push(record),
            Err(reason) => batch.diagnostics.push(Diagnostic {
                code: "malformed_record".into(),
                message: format!("record {} rejected: {reason}", index + 1),
                endpoint_id: Some(endpoint_id.into()),
            }),
        }
    }
    batch
}

pub fn validate_hierarchy(
    endpoint_id: &str,
    sessions: &[SessionRecord],
    windows: &mut Vec<WindowRecord>,
    panes: &mut Vec<PaneRecord>,
) -> Vec<Diagnostic> {
    let session_ids = sessions
        .iter()
        .map(|record| record.session_id.as_str())
        .collect::<HashSet<_>>();
    let mut diagnostics = Vec::new();
    windows.retain(|window| {
        let valid = session_ids.contains(window.session_id.as_str());
        if !valid {
            diagnostics.push(orphan(endpoint_id, "window", &window.window_id));
        }
        valid
    });
    let window_ids = windows
        .iter()
        .map(|record| (record.session_id.as_str(), record.window_id.as_str()))
        .collect::<HashSet<_>>();
    panes.retain(|pane| {
        let valid = window_ids.contains(&(pane.session_id.as_str(), pane.window_id.as_str()));
        if !valid {
            diagnostics.push(orphan(endpoint_id, "pane", &pane.pane_id));
        }
        valid
    });
    diagnostics
}

fn orphan(endpoint_id: &str, kind: &str, id: &str) -> Diagnostic {
    Diagnostic {
        code: "orphan_parent".into(),
        message: format!("{kind} {id} rejected because its parent identity is absent"),
        endpoint_id: Some(endpoint_id.into()),
    }
}

fn exact(fields: &[&str], expected: usize) -> Result<(), String> {
    if fields.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "expected exactly {expected} fields, got {}",
            fields.len()
        ))
    }
}

fn required(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty() {
        Err(format!("{name} is empty"))
    } else {
        Ok(())
    }
}

fn sigil(value: &str, expected: char) -> Result<(), String> {
    required(value, "runtime_id")?;
    let mut chars = value.chars();
    if chars.next() != Some(expected)
        || !chars.clone().all(|ch| ch.is_ascii_digit())
        || chars.as_str().is_empty()
    {
        return Err(format!("invalid {expected} runtime ID"));
    }
    Ok(())
}

fn optional_sigil(value: &str, expected: char) -> Result<(), String> {
    if value.is_empty() {
        Ok(())
    } else {
        sigil(value, expected)
    }
}

fn numeric(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{name} is not an unsigned integer"));
    }
    value
        .parse::<u64>()
        .map(|_| ())
        .map_err(|_| format!("{name} is outside the supported range"))
}

fn optional_numeric(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty() {
        Ok(())
    } else {
        numeric(value, name)
    }
}

fn boolean(value: &str, name: &str) -> Result<(), String> {
    if matches!(value, "0" | "1") {
        Ok(())
    } else {
        Err(format!("{name} is not 0 or 1"))
    }
}

fn bounded(value: &str, max: usize, name: &str) -> Result<(), String> {
    if value.len() > max {
        Err(format!("{name} exceeds {max} bytes"))
    } else {
        Ok(())
    }
}

fn optional(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.into())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn join(fields: &[&str]) -> String {
        fields.join(FIELD_SEPARATOR)
    }

    #[test]
    fn delimiter_collision_is_rejected_instead_of_shifting_fields() {
        let line = join(&["$1", "bad", "split", "/tmp", "1", "2", "", "0", "1"]);
        let parsed = parse_sessions("ep", "gen", &line);
        assert!(parsed.records.is_empty());
        assert_eq!(parsed.diagnostics[0].code, "malformed_record");
    }

    #[test]
    fn malformed_numbers_are_not_coerced() {
        let line = join(&["$1", "work", "/tmp", "oops", "2", "", "0", "1"]);
        assert!(parse_sessions("ep", "gen", &line).records.is_empty());
    }

    #[test]
    fn parses_complete_hierarchy_and_clients() {
        let sessions = parse_sessions(
            "ep",
            "gen",
            &join(&["$1", "work", "/tmp", "1", "2", "", "1", "1"]),
        );
        let windows = parse_windows("ep", "gen", &join(&["$1", "@2", "0", "dev", "1", "2"]));
        let panes = parse_panes(
            "ep",
            "gen",
            &join(&[
                "$1",
                "@2",
                "%3",
                "0",
                "1",
                "2",
                "/dev/ttys1",
                "/tmp",
                "zsh",
                "shell",
            ]),
        );
        let clients = parse_clients(
            "ep",
            "gen",
            "501",
            &join(&[
                "/dev/ttys1",
                "10",
                "11",
                "/dev/ttys1",
                "$1",
                "@2",
                "%3",
                "12",
                "",
            ]),
        );
        assert_eq!(sessions.records.len(), 1);
        assert_eq!(windows.records.len(), 1);
        assert_eq!(panes.records.len(), 1);
        assert_eq!(clients.records.len(), 1);
    }

    proptest! {
        #[test]
        fn arbitrary_session_lines_never_panic(input in ".{0,2048}") {
            let _ = parse_sessions("ep", "gen", &input);
        }
    }
}
