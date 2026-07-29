use anyhow::{anyhow, Result};

/// Printable framing token: tmux 3.2–3.3 rewrite literal control
/// characters embedded in `-F` formats on some platforms.
pub const SEP: &str = "|:tmx:core:v1:|";

#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub activity: i64,
    pub attached: bool,
    pub windows: i64,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct WindowInfo {
    pub session_id: String,
    pub session_name: String,
    pub session_path: String,
    pub id: String,
    pub index: String,
    pub name: String,
    pub active: bool,
    pub cwd: String,
    pub note: String,
}

#[derive(Debug, Clone, Default)]
pub struct PaneInfo {
    pub session_id: String,
    pub session_name: String,
    pub session_path: String,
    pub window_id: String,
    pub window_name: String,
    pub id: String,
    pub index: String,
    pub title: String,
    pub command: String,
    pub cwd: String,
    pub active: bool,
    pub note: String,
}

pub fn session_format() -> String {
    join_format(&[
        "session_id",
        "session_name",
        "session_path",
        "session_activity",
        "session_attached",
        "session_windows",
    ])
}

pub fn window_format() -> String {
    join_format(&[
        "session_id",
        "session_name",
        "session_path",
        "window_id",
        "window_index",
        "window_name",
        "window_active",
        "pane_current_path",
    ])
}

pub fn pane_format() -> String {
    join_format(&[
        "session_id",
        "session_name",
        "session_path",
        "window_id",
        "window_name",
        "pane_id",
        "pane_index",
        "pane_title",
        "pane_current_command",
        "pane_current_path",
        "pane_active",
    ])
}

pub fn current_format() -> String {
    join_format(&[
        "session_id",
        "session_name",
        "session_path",
        "window_id",
        "window_name",
        "window_index",
        "pane_id",
        "pane_title",
        "pane_current_command",
        "pane_current_path",
    ])
}

pub fn client_size_format() -> &'static str {
    "#{client_width}|:tmx:core:v1:|#{client_height}"
}

fn join_format(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|f| format!("#{{{f}}}"))
        .collect::<Vec<_>>()
        .join(SEP)
}

pub fn parse_sessions(out: &str) -> Result<Vec<SessionInfo>> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_session)
        .collect()
}

pub fn parse_windows(out: &str) -> Result<Vec<WindowInfo>> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_window)
        .collect()
}

pub fn parse_panes(out: &str) -> Result<Vec<PaneInfo>> {
    out.lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse_pane)
        .collect()
}

pub fn parse_session(line: &str) -> Result<SessionInfo> {
    let f = split(line);
    require(&f, 6, line)?;
    Ok(SessionInfo {
        id: f[0].into(),
        name: f[1].into(),
        path: f[2].into(),
        activity: f[3].parse().unwrap_or(0),
        attached: f[4].parse::<i64>().unwrap_or(0) > 0,
        windows: f[5].parse().unwrap_or(0),
        note: String::new(),
    })
}

pub fn parse_window(line: &str) -> Result<WindowInfo> {
    let f = split(line);
    require(&f, 8, line)?;
    Ok(WindowInfo {
        session_id: f[0].into(),
        session_name: f[1].into(),
        session_path: f[2].into(),
        id: f[3].into(),
        index: f[4].into(),
        name: f[5].into(),
        active: f[6] == "1",
        cwd: f[7].into(),
        note: String::new(),
    })
}

pub fn parse_pane(line: &str) -> Result<PaneInfo> {
    let f = split(line);
    require(&f, 11, line)?;
    Ok(PaneInfo {
        session_id: f[0].into(),
        session_name: f[1].into(),
        session_path: f[2].into(),
        window_id: f[3].into(),
        window_name: f[4].into(),
        id: f[5].into(),
        index: f[6].into(),
        title: f[7].into(),
        command: f[8].into(),
        cwd: f[9].into(),
        active: f[10] == "1",
        note: String::new(),
    })
}

fn split(line: &str) -> Vec<&str> {
    line.split(SEP).collect()
}

fn require(fields: &[&str], len: usize, line: &str) -> Result<()> {
    if fields.len() != len {
        Err(anyhow!(
            "expected exactly {len} tmux fields, got {} in {line:?}",
            fields.len()
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_format() {
        let line = format!("$1{SEP}work{SEP}/tmp{SEP}123{SEP}1{SEP}2");
        let s = parse_session(&line).unwrap();
        assert_eq!(s.id, "$1");
        assert!(s.attached);
        assert_eq!(s.windows, 2);
    }

    #[test]
    fn framing_collision_is_rejected_instead_of_shifting_fields() {
        let line = format!("$1{SEP}bad{SEP}split{SEP}/tmp{SEP}123{SEP}1{SEP}2");
        assert!(parse_session(&line).is_err());
    }

    #[test]
    fn parses_pane_format_with_spaces() {
        let line = format!("$1{SEP}work{SEP}/tmp{SEP}@2{SEP}dev{SEP}%3{SEP}1{SEP}title with spaces{SEP}zsh{SEP}/tmp/x{SEP}1");
        let p = parse_pane(&line).unwrap();
        assert_eq!(p.title, "title with spaces");
        assert_eq!(p.id, "%3");
    }
}
