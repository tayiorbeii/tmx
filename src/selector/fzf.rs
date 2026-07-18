use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

use crate::model::{PaletteRow, UiProfile};
use crate::palette::ROW_SEP;
use crate::selector::Selector;

#[derive(Debug, Clone)]
pub struct FzfSelector {
    bin: String,
}

impl Default for FzfSelector {
    fn default() -> Self {
        Self { bin: "fzf".into() }
    }
}

impl FzfSelector {
    pub fn new(bin: impl Into<String>) -> Self {
        Self { bin: bin.into() }
    }

    pub fn args_for(profile: UiProfile) -> Vec<String> {
        // fzf calculates --nth against the line produced by --with-nth, not the
        // original encoded row. Combining `--with-nth 3` with `--nth 3,6`
        // therefore creates an empty search scope. Keep the machine fields
        // hidden and search the human-readable field that we render below.
        let mut args = vec![
            "--delimiter".into(),
            ROW_SEP.into(),
            "--with-nth".into(),
            "3".into(),
            "--height".into(),
            "100%".into(),
            "--layout".into(),
            "reverse".into(),
            "--prompt".into(),
            "Filter> ".into(),
            "--header".into(),
            "Type to filter | Enter open | Esc close".into(),
            "--header-first".into(),
            "--highlight-line".into(),
        ];
        match profile {
            UiProfile::Desktop => {
                // Mouse input is enabled by default in fzf. Recent fzf versions
                // accept --no-mouse but reject the redundant positive --mouse flag.
                args.extend(["--border".into(), "--info=inline-right".into()]);
            }
            UiProfile::Mobile => {
                args.extend([
                    "--no-mouse".into(),
                    "--info=hidden".into(),
                    "--no-scrollbar".into(),
                ]);
            }
        }
        args
    }
}

impl Selector for FzfSelector {
    fn select(&self, rows: &[PaletteRow], profile: UiProfile) -> Result<Option<PaletteRow>> {
        let mut child = Command::new(&self.bin)
            .args(Self::args_for(profile))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| "starting fzf; install fzf or use non-palette commands")?;

        {
            let stdin = child.stdin.as_mut().context("opening fzf stdin")?;
            for row in rows {
                writeln!(stdin, "{}", row.encode())?;
            }
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let line = String::from_utf8_lossy(&output.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_string();
        if line.is_empty() {
            Ok(None)
        } else {
            Ok(PaletteRow::decode(&line))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{build_rows, LiveTargets};
    use crate::tmux::formats::{PaneInfo, SessionInfo, WindowInfo};

    #[test]
    fn mobile_has_no_preview_or_mouse() {
        let args = FzfSelector::args_for(UiProfile::Mobile).join(" ");
        assert!(args.contains("--no-mouse"));
        assert!(!args.contains("preview"));
    }

    #[test]
    fn desktop_uses_searchable_display_field_and_default_mouse_support() {
        let args = FzfSelector::args_for(UiProfile::Desktop);
        assert!(args.windows(2).any(|w| w[0] == "--with-nth" && w[1] == "3"));
        assert!(!args.iter().any(|arg| arg == "--nth"));
        assert!(!args.iter().any(|arg| arg == "--no-sort"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--prompt" && w[1] == "Filter> "));
        assert!(args.iter().any(|arg| arg == "--header"));
        assert!(!args.iter().any(|arg| arg == "--mouse"));
        assert!(!args.iter().any(|arg| arg == "--no-mouse"));
    }

    #[test]
    fn generated_rows_filter_by_visible_kind_and_metadata() {
        if which::which("fzf").is_err() {
            eprintln!("skipping: fzf unavailable");
            return;
        }

        for profile in [UiProfile::Desktop, UiProfile::Mobile] {
            let rows = representative_rows(profile);
            for (query, expected_row) in [
                ("SESSION", "s:$1"),
                ("alpha-session", "s:$1"),
                ("/srv/session-root", "s:$1"),
                ("deploy-urgent", "s:$1"),
                ("WINDOW", "w:@1"),
                ("dashboard-window", "w:@1"),
                ("/srv/window-cwd", "w:@1"),
                ("frontend-note", "w:@1"),
                ("PANE", "p:%1"),
                ("Backend-Logs", "p:%1"),
                ("cargo-run", "p:%1"),
                ("/srv/pane-cwd", "p:%1"),
                ("pane-note", "p:%1"),
                ("ACTION", "a:new"),
            ] {
                let matches = filter(&rows, profile, query);
                assert!(
                    matches.iter().any(|row| row.row_id == expected_row),
                    "{profile:?} query {query:?} did not match {expected_row:?}: {matches:?}"
                );
            }
        }
    }

    fn representative_rows(profile: UiProfile) -> Vec<PaletteRow> {
        build_rows(
            &LiveTargets {
                sessions: vec![SessionInfo {
                    id: "$1".into(),
                    name: "alpha-session".into(),
                    path: "/srv/session-root".into(),
                    windows: 1,
                    note: "deploy-urgent".into(),
                    ..Default::default()
                }],
                windows: vec![WindowInfo {
                    id: "@1".into(),
                    session_name: "alpha-session".into(),
                    index: "0".into(),
                    name: "dashboard-window".into(),
                    cwd: "/srv/window-cwd".into(),
                    note: "frontend-note".into(),
                    ..Default::default()
                }],
                panes: vec![PaneInfo {
                    id: "%1".into(),
                    session_name: "alpha-session".into(),
                    window_name: "dashboard-window".into(),
                    index: "0".into(),
                    title: "Backend-Logs".into(),
                    command: "cargo-run".into(),
                    cwd: "/srv/pane-cwd".into(),
                    note: "pane-note".into(),
                    ..Default::default()
                }],
            },
            profile,
        )
    }

    fn filter(rows: &[PaletteRow], profile: UiProfile, query: &str) -> Vec<PaletteRow> {
        let mut args = FzfSelector::args_for(profile);
        args.extend(["--filter".into(), query.into()]);
        let mut child = Command::new("fzf")
            .args(args)
            .env("FZF_DEFAULT_OPTS", "")
            .env("FZF_DEFAULT_OPTS_FILE", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        {
            let mut stdin = child.stdin.take().unwrap();
            for row in rows {
                writeln!(stdin, "{}", row.encode()).unwrap();
            }
        }
        let output = child.wait_with_output().unwrap();
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(PaletteRow::decode)
            .collect()
    }
}
