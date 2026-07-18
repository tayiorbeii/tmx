use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub default_ui: String,
    pub mobile_width_threshold: u16,
    pub mobile_height_threshold: u16,
    pub selector_backend: String,
    pub project_search_backend: String,
    pub prompt_notes: bool,
    pub note_prompt_min_active_minutes: u64,
    pub note_prompt_cooldown_hours: u64,
    pub mobile_note_prompt_mode: String,
    pub project_roots: Vec<String>,
    pub paths: PathsConfig,
    pub dependencies: DependenciesConfig,
    pub bindings: BindingsConfig,
    pub destructive_actions: DestructiveActionsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathsConfig {
    pub state: String,
    pub layouts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DependenciesConfig {
    pub use_fd: bool,
    pub use_rg: bool,
    pub use_zoxide: bool,
    pub use_fff: bool,
    pub use_television: bool,
    pub mac_notifications: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BindingsConfig {
    pub palette: String,
    pub mobile_palette: String,
    pub last: String,
    pub note: String,
    pub rename_menu: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DestructiveActionsConfig {
    pub show_in_palette: bool,
    pub require_confirmation: bool,
    pub mobile_require_typed_confirmation: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_ui: "auto".into(),
            mobile_width_threshold: 100,
            mobile_height_threshold: 35,
            selector_backend: "fzf".into(),
            project_search_backend: "none".into(),
            prompt_notes: true,
            note_prompt_min_active_minutes: 20,
            note_prompt_cooldown_hours: 4,
            mobile_note_prompt_mode: "explicit".into(),
            project_roots: vec!["~/src".into(), "~/work".into(), "~/Code".into()],
            paths: PathsConfig::default(),
            dependencies: DependenciesConfig::default(),
            bindings: BindingsConfig::default(),
            destructive_actions: DestructiveActionsConfig::default(),
        }
    }
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            state: "~/.local/state/tmx/state.sqlite3".into(),
            layouts: "~/.config/tmx/layouts".into(),
        }
    }
}

impl Default for DependenciesConfig {
    fn default() -> Self {
        Self {
            use_fd: true,
            use_rg: true,
            use_zoxide: true,
            use_fff: false,
            use_television: false,
            mac_notifications: false,
        }
    }
}

impl Default for BindingsConfig {
    fn default() -> Self {
        Self {
            palette: "T".into(),
            mobile_palette: "M".into(),
            last: "L".into(),
            note: "N".into(),
            rename_menu: "R".into(),
        }
    }
}

impl Default for DestructiveActionsConfig {
    fn default() -> Self {
        Self {
            show_in_palette: false,
            require_confirmation: true,
            mobile_require_typed_confirmation: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    pub fn state_path(&self) -> PathBuf {
        expand_path(&self.paths.state)
    }
}

pub fn config_path() -> PathBuf {
    expand_path("~/.config/tmx/config.toml")
}

pub fn expand_path(path: &str) -> PathBuf {
    PathBuf::from(shellexpand::tilde(path).into_owned())
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec_paths() {
        let cfg = Config::default();
        assert_eq!(cfg.paths.state, "~/.local/state/tmx/state.sqlite3");
        assert_eq!(cfg.mobile_width_threshold, 100);
        assert_eq!(cfg.mobile_height_threshold, 35);
        assert_eq!(cfg.selector_backend, "fzf");
        assert!(cfg.prompt_notes);
    }

    #[test]
    fn parses_toml() {
        let cfg: Config =
            toml::from_str("default_ui = 'mobile'\n[dependencies]\nuse_fd = false\n").unwrap();
        assert_eq!(cfg.default_ui, "mobile");
        assert!(!cfg.dependencies.use_fd);
        assert_eq!(cfg.selector_backend, "fzf");
    }
}
