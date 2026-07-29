pub mod formats;

use anyhow::{anyhow, Context, Result};
use std::ffi::OsStr;
use std::process::{Command, Stdio};

use crate::model::CurrentTarget;
use formats::{parse_panes, parse_sessions, parse_windows, PaneInfo, SessionInfo, WindowInfo};

#[derive(Debug, Clone)]
enum SocketSelector {
    Name(String),
    Path(String),
}

#[derive(Debug, Clone)]
pub struct Tmux {
    bin: String,
    socket: Option<SocketSelector>,
}

impl Default for Tmux {
    fn default() -> Self {
        Self {
            bin: "tmux".into(),
            socket: None,
        }
    }
}

impl Tmux {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_socket(socket: impl Into<String>) -> Self {
        Self {
            bin: "tmux".into(),
            socket: Some(SocketSelector::Name(socket.into())),
        }
    }

    pub fn with_socket_path(socket: impl Into<String>) -> Self {
        Self {
            bin: "tmux".into(),
            socket: Some(SocketSelector::Path(socket.into())),
        }
    }

    pub fn is_inside_tmux() -> bool {
        std::env::var_os("TMUX").is_some()
    }

    pub fn args<'a, I, S>(&self, args: I) -> Vec<String>
    where
        I: IntoIterator<Item = &'a S>,
        S: AsRef<str> + 'a,
    {
        let mut out = Vec::new();
        if let Some(socket) = &self.socket {
            match socket {
                SocketSelector::Name(value) => out.extend(["-L".into(), value.clone()]),
                SocketSelector::Path(value) => out.extend(["-S".into(), value.clone()]),
            }
        }
        out.extend(args.into_iter().map(|s| s.as_ref().to_string()));
        out
    }

    pub fn run<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new(&self.bin);
        if let Some(socket) = &self.socket {
            match socket {
                SocketSelector::Name(value) => cmd.arg("-L").arg(value),
                SocketSelector::Path(value) => cmd.arg("-S").arg(value),
            };
        }
        cmd.args(args);
        let output = cmd.output().with_context(|| "running tmux")?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end_matches('\n')
                .to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(anyhow!("tmux failed: {stderr}"))
        }
    }

    pub fn run_inherit<I, S>(&self, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut cmd = Command::new(&self.bin);
        if let Some(socket) = &self.socket {
            match socket {
                SocketSelector::Name(value) => cmd.arg("-L").arg(value),
                SocketSelector::Path(value) => cmd.arg("-S").arg(value),
            };
        }
        let status = cmd
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| "running tmux")?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("tmux exited with status {status}"))
        }
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        match self.run(["list-sessions", "-F", &formats::session_format()]) {
            Ok(out) => parse_sessions(&out),
            Err(e) if e.to_string().contains("no server running") => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    pub fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        match self.run(["list-windows", "-a", "-F", &formats::window_format()]) {
            Ok(out) => parse_windows(&out),
            Err(e) if e.to_string().contains("no server running") => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    pub fn list_panes(&self) -> Result<Vec<PaneInfo>> {
        match self.run(["list-panes", "-a", "-F", &formats::pane_format()]) {
            Ok(out) => parse_panes(&out),
            Err(e) if e.to_string().contains("no server running") => Ok(vec![]),
            Err(e) => Err(e),
        }
    }

    pub fn current_target(&self) -> Result<CurrentTarget> {
        let out = self.run(["display-message", "-p", &formats::current_format()])?;
        let f: Vec<_> = out.split(formats::SEP).collect();
        if f.len() < 10 {
            return Err(anyhow!("unexpected current target format: {out:?}"));
        }
        Ok(CurrentTarget {
            session_id: f[0].into(),
            session_name: f[1].into(),
            session_path: f[2].into(),
            window_id: f[3].into(),
            window_name: f[4].into(),
            window_index: f[5].into(),
            pane_id: f[6].into(),
            pane_title: f[7].into(),
            pane_command: f[8].into(),
            cwd: f[9].into(),
        })
    }

    pub fn display_pane_cwd(&self, pane: &str) -> Result<Option<String>> {
        let out = self.run(["display-message", "-p", "-t", pane, "#{pane_current_path}"])?;
        Ok(if out.trim().is_empty() {
            None
        } else {
            Some(out)
        })
    }

    pub fn client_size(&self) -> Result<Option<(u16, u16)>> {
        let out = self.run(["display-message", "-p", formats::client_size_format()])?;
        let f: Vec<_> = out.split(formats::SEP).collect();
        if f.len() < 2 {
            return Ok(None);
        }
        Ok(Some((f[0].parse().unwrap_or(0), f[1].parse().unwrap_or(0))))
    }

    pub fn version(&self) -> Result<String> {
        self.run(["-V"])
    }

    pub fn target_exists(&self, target: &str) -> bool {
        let fmt = match target.chars().next() {
            Some('$') => "#{session_id}",
            Some('@') => "#{window_id}",
            Some('%') => "#{pane_id}",
            _ => "#{session_id}",
        };
        self.run(["display-message", "-p", "-t", target, fmt])
            .is_ok()
    }

    pub fn switch_to(&self, target: &str, inside_tmux: bool) -> Result<()> {
        if inside_tmux {
            match target.chars().next() {
                Some('%') => {
                    self.run(["select-pane", "-t", target])?;
                    self.run(["switch-client", "-t", target])?;
                }
                Some('@') => {
                    self.run(["select-window", "-t", target])?;
                    self.run(["switch-client", "-t", target])?;
                }
                _ => {
                    self.run(["switch-client", "-t", target])?;
                }
            }
            Ok(())
        } else {
            self.run_inherit(["attach-session", "-t", target])
        }
    }

    pub fn new_session_args(name: &str, cwd: &str) -> Vec<String> {
        vec![
            "new-session".into(),
            "-d".into(),
            "-s".into(),
            name.into(),
            "-c".into(),
            cwd.into(),
        ]
    }

    pub fn new_session(&self, name: &str, cwd: &str) -> Result<()> {
        self.run(Self::new_session_args(name, cwd)).map(|_| ())
    }

    pub fn set_option(
        &self,
        target: &str,
        option: &str,
        value: &str,
        scope: Option<&str>,
    ) -> Result<()> {
        let mut args = vec!["set-option".to_string(), "-q".to_string()];
        if let Some(flag) = option_scope_flag(scope) {
            args.push(flag.into());
        }
        args.extend(["-t".into(), target.into(), option.into(), value.into()]);
        self.run(args).map(|_| ())
    }

    pub fn get_option(
        &self,
        target: &str,
        option: &str,
        scope: Option<&str>,
    ) -> Result<Option<String>> {
        let mut args = vec!["show-options".to_string(), "-qv".to_string()];
        if let Some(flag) = option_scope_flag(scope) {
            args.push(flag.into());
        }
        args.extend(["-t".into(), target.into(), option.into()]);
        let out = self.run(args)?;
        Ok(if out.is_empty() { None } else { Some(out) })
    }

    pub fn rename_args(scope: &str, target: &str, name: &str) -> Vec<String> {
        match scope {
            "window" => vec![
                "rename-window".into(),
                "-t".into(),
                target.into(),
                name.into(),
            ],
            "pane" => vec![
                "select-pane".into(),
                "-t".into(),
                target.into(),
                "-T".into(),
                name.into(),
            ],
            _ => vec![
                "rename-session".into(),
                "-t".into(),
                target.into(),
                name.into(),
            ],
        }
    }

    pub fn rename(&self, scope: &str, target: &str, name: &str) -> Result<()> {
        self.run(Self::rename_args(scope, target, name)).map(|_| ())
    }
}

fn option_scope_flag(scope: Option<&str>) -> Option<&'static str> {
    match scope {
        Some("window") => Some("-w"),
        Some("pane") => Some("-p"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_command_generation() {
        assert_eq!(
            Tmux::rename_args("session", "$1", "api"),
            vec!["rename-session", "-t", "$1", "api"]
        );
        assert_eq!(
            Tmux::rename_args("window", "@2", "dev"),
            vec!["rename-window", "-t", "@2", "dev"]
        );
        assert_eq!(
            Tmux::rename_args("pane", "%3", "logs"),
            vec!["select-pane", "-t", "%3", "-T", "logs"]
        );
    }
}
