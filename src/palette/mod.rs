use crate::model::{PaletteRow, TargetKind, UiProfile};
use crate::project::shorten_path;
use crate::tmux::formats::{PaneInfo, SessionInfo, WindowInfo};

/// ASCII unit separator. Safe for hidden machine-readable row fields.
pub const ROW_SEP: &str = "\x1f";

#[derive(Debug, Clone, Default)]
pub struct LiveTargets {
    pub sessions: Vec<SessionInfo>,
    pub windows: Vec<WindowInfo>,
    pub panes: Vec<PaneInfo>,
}

pub fn build_rows(live: &LiveTargets, profile: UiProfile) -> Vec<PaletteRow> {
    let mut rows = Vec::new();
    for s in &live.sessions {
        rows.push(session_row(s, profile));
    }
    for w in &live.windows {
        rows.push(window_row(w, profile));
    }
    for p in &live.panes {
        rows.push(pane_row(p, profile));
    }
    rows.extend(action_rows(profile));
    rows
}

fn session_row(s: &SessionInfo, profile: UiProfile) -> PaletteRow {
    let path = shorten_path(&s.path, 46);
    let display = match profile {
        UiProfile::Mobile => format!("SESSION {} {} {}", s.name, path, note_suffix(&s.note)),
        UiProfile::Desktop => format!(
            "SESSION  {:<24} {:>2}w  {:<46} {}",
            s.name,
            s.windows,
            path,
            note_suffix(&s.note)
        ),
    };
    PaletteRow::new(
        format!("s:{}", s.id),
        TargetKind::Session,
        display,
        Some(TargetKind::Session),
        Some(s.id.clone()),
        format!("{} {} {}", s.name, s.path, s.note),
    )
}

fn window_row(w: &WindowInfo, profile: UiProfile) -> PaletteRow {
    let path = shorten_path(&w.cwd, 40);
    let mobile_path = shorten_path(&w.cwd, 24);
    let display = match profile {
        UiProfile::Mobile => format!(
            "WINDOW {}/{} {} {}",
            w.session_name,
            w.name,
            mobile_path,
            note_suffix(&w.note)
        ),
        UiProfile::Desktop => format!(
            "WINDOW   {:<18} {:>3}: {:<22} {:<40} {}",
            w.session_name,
            w.index,
            w.name,
            path,
            note_suffix(&w.note)
        ),
    };
    PaletteRow::new(
        format!("w:{}", w.id),
        TargetKind::Window,
        display,
        Some(TargetKind::Window),
        Some(w.id.clone()),
        format!("{} {} {} {}", w.session_name, w.name, w.cwd, w.note),
    )
}

fn pane_row(p: &PaneInfo, profile: UiProfile) -> PaletteRow {
    let path = shorten_path(&p.cwd, 34);
    let mobile_path = shorten_path(&p.cwd, 20);
    let title = if p.title.is_empty() {
        &p.command
    } else {
        &p.title
    };
    let display = match profile {
        UiProfile::Mobile => format!(
            "PANE {}/{}.{} {} {} {} {}",
            p.session_name,
            p.window_name,
            p.index,
            title,
            p.command,
            mobile_path,
            note_suffix(&p.note)
        ),
        UiProfile::Desktop => format!(
            "PANE     {:<14} {:<18} {:>3} {:<16} {:<12} {:<34} {}",
            p.session_name,
            p.window_name,
            p.index,
            title,
            p.command,
            path,
            note_suffix(&p.note)
        ),
    };
    PaletteRow::new(
        format!("p:{}", p.id),
        TargetKind::Pane,
        display,
        Some(TargetKind::Pane),
        Some(p.id.clone()),
        format!(
            "{} {} {} {} {} {}",
            p.session_name, p.window_name, p.title, p.command, p.cwd, p.note
        ),
    )
}

fn action_rows(profile: UiProfile) -> Vec<PaletteRow> {
    let labels = match profile {
        UiProfile::Mobile => vec![
            ("a:new", "ACTION new here", "new current cwd session"),
            ("a:last", "ACTION last", "last recent previous"),
            ("a:note", "ACTION note current", "note current session"),
            (
                "a:rename",
                "ACTION rename session",
                "rename current session",
            ),
        ],
        UiProfile::Desktop => vec![
            (
                "a:new",
                "ACTION   new here",
                "new current cwd session project",
            ),
            ("a:last", "ACTION   last target", "last recent previous mru"),
            (
                "a:note",
                "ACTION   note current session",
                "note current session",
            ),
            (
                "a:rename",
                "ACTION   rename current session",
                "rename current session",
            ),
        ],
    };
    labels
        .into_iter()
        .map(|(id, display, search)| {
            PaletteRow::new(
                id,
                TargetKind::Action,
                display,
                Some(TargetKind::Action),
                Some(id.to_string()),
                search,
            )
        })
        .collect()
}

fn note_suffix(note: &str) -> String {
    if note.trim().is_empty() {
        String::new()
    } else {
        let mut s = note.replace(['\n', '\r'], " ");
        if s.len() > 48 {
            s.truncate(45);
            s.push_str("...");
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_encoding_roundtrip() {
        let row = PaletteRow::new(
            "x",
            TargetKind::Session,
            "S hi\nthere",
            Some(TargetKind::Session),
            Some("$1".into()),
            "hi",
        );
        let encoded = row.encode();
        let decoded = PaletteRow::decode(&encoded).unwrap();
        assert_eq!(decoded.row_id, "x");
        assert_eq!(decoded.target_id.as_deref(), Some("$1"));
        assert!(!decoded.display.contains('\n'));
    }
}
