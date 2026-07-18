use anyhow::{Context, Result};
use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct OriginInputs {
    pub origin_cwd: Option<String>,
    pub origin_pane: Option<String>,
    pub pwd: Option<String>,
}

pub fn resolve_origin_cwd<F>(inputs: &OriginInputs, pane_lookup: F) -> Result<PathBuf>
where
    F: Fn(&str) -> Result<Option<String>>,
{
    if let Some(cwd) = valid_dir(inputs.origin_cwd.as_deref()) {
        return Ok(cwd);
    }
    if let Some(pane) = &inputs.origin_pane {
        if let Some(cwd) = pane_lookup(pane)? {
            if let Some(cwd) = valid_dir(Some(&cwd)) {
                return Ok(cwd);
            }
        }
    }
    if let Some(cwd) = valid_dir(inputs.pwd.as_deref()) {
        return Ok(cwd);
    }
    env::current_dir().context("resolving current directory")
}

fn valid_dir(path: Option<&str>) -> Option<PathBuf> {
    let path = path?.trim();
    if path.is_empty() {
        return None;
    }
    let expanded = PathBuf::from(shellexpand::tilde(path).into_owned());
    if expanded.is_dir() {
        Some(expanded)
    } else {
        None
    }
}

pub fn origin_inputs_from_env() -> OriginInputs {
    OriginInputs {
        origin_cwd: env::var("TMX_ORIGIN_CWD").ok(),
        origin_pane: env::var("TMX_ORIGIN_PANE").ok(),
        pwd: env::var("PWD").ok(),
    }
}

pub fn repo_root(path: &Path) -> Option<PathBuf> {
    let mut cur = path.canonicalize().ok()?;
    loop {
        if cur.join(".git").exists() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

pub fn project_name(path: &Path) -> String {
    let root = repo_root(path).unwrap_or_else(|| path.to_path_buf());
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("session");
    sanitize_session_name(name)
}

pub fn sanitize_session_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.trim().chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        if keep {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "session".into()
    } else {
        out
    }
}

pub fn session_name_for(
    path: &Path,
    explicit: Option<&str>,
    label: Option<&str>,
    existing: &HashSet<String>,
) -> String {
    let base = explicit
        .map(sanitize_session_name)
        .unwrap_or_else(|| project_name(path));
    let mut name = match label {
        Some(label) if !label.trim().is_empty() => {
            format!("{}-{}", base, sanitize_session_name(label))
        }
        _ => base,
    };
    if !existing.contains(&name) {
        return name;
    }
    let stem = name.clone();
    for i in 2..1000 {
        name = format!("{}-{}", stem, i);
        if !existing.contains(&name) {
            return name;
        }
    }
    name
}

pub fn shorten_path(path: &str, max: usize) -> String {
    if path.is_empty() {
        return String::new();
    }
    let mut s = if let Some(home) = dirs::home_dir().and_then(|p| p.to_str().map(str::to_string)) {
        path.strip_prefix(&home)
            .map(|rest| format!("~{}", rest))
            .unwrap_or_else(|| path.to_string())
    } else {
        path.to_string()
    };
    if s.len() <= max {
        return s;
    }
    let parts: Vec<_> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 2 {
        s = format!("…/{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]);
    }
    if s.len() > max {
        s.truncate(max.saturating_sub(1));
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sanitizes_session_names() {
        assert_eq!(sanitize_session_name("my.project:api"), "my-project-api");
        assert_eq!(sanitize_session_name("  !!!  "), "session");
        assert_eq!(sanitize_session_name("a__b-c"), "a__b-c");
    }

    #[test]
    fn collision_suffixes() {
        let dir = tempdir().unwrap();
        let existing = HashSet::from(["tmp".to_string(), "tmp-2".to_string()]);
        let name = session_name_for(dir.path(), Some("tmp"), None, &existing);
        assert_eq!(name, "tmp-3");
    }

    #[test]
    fn resolves_origin_cwd_priority() {
        let dir = tempdir().unwrap();
        let inputs = OriginInputs {
            origin_cwd: Some(dir.path().display().to_string()),
            origin_pane: Some("%1".into()),
            pwd: None,
        };
        let got = resolve_origin_cwd(&inputs, |_| Ok(None)).unwrap();
        assert_eq!(got, dir.path());
    }
}
