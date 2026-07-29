use std::collections::BinaryHeap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};

use crate::config::{EndpointConfig, SwitcherConfig};

const MAX_ALIAS_BYTES: usize = 64;
const MAX_REGISTERED_ENDPOINTS: usize = 32;
const MAX_DISCOVERY_CANDIDATES: usize = MAX_REGISTERED_ENDPOINTS * 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointSelector {
    Default,
    Name(String),
    Path(PathBuf),
}

impl EndpointSelector {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Name(_) => "name",
            Self::Path(_) => "path",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegisteredEndpoint {
    pub endpoint_id: String,
    pub alias: String,
    pub selector: EndpointSelector,
    pub trust_source: String,
    pub socket_path: PathBuf,
}

impl RegisteredEndpoint {
    /// Commands are pinned to the canonical path that passed trust checks.
    /// The original selector is retained only as provenance/display metadata.
    pub fn tmux_prefix(&self) -> Vec<String> {
        vec!["-S".into(), self.socket_path.to_string_lossy().into_owned()]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketIdentity {
    pub device: u64,
    pub inode: u64,
    pub uid: u32,
    pub canonical_path: PathBuf,
}

impl SocketIdentity {
    pub fn stable_text(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.device,
            self.inode,
            self.uid,
            self.canonical_path.display()
        )
    }
}

pub struct EndpointRegistration {
    pub endpoints: Vec<RegisteredEndpoint>,
    pub truncated: bool,
}

pub fn register_endpoints(config: &SwitcherConfig) -> Result<EndpointRegistration> {
    let configured = if config.endpoints.is_empty() {
        vec![EndpointConfig::default()]
    } else {
        config.endpoints.clone()
    };
    let mut candidates = configured
        .into_iter()
        .map(|entry| (entry, "configuration"))
        .collect::<Vec<_>>();
    if config.discover_named {
        candidates.extend(
            discover_named_configs()?
                .into_iter()
                .map(|entry| (entry, "user_runtime_discovery")),
        );
    }
    let limit = config.max_endpoints.clamp(1, MAX_REGISTERED_ENDPOINTS);
    let mut endpoints: Vec<RegisteredEndpoint> = Vec::new();
    let mut truncated = false;
    for (entry, configured_source) in candidates {
        let selector = parse_selector(&entry.selector)?;
        let path = socket_path_for(&selector)?;
        let canonical = canonical_identity_path(&path)?;
        let endpoint_id = endpoint_id_for(&canonical);
        let alias = sanitize_alias(&entry.alias, selector.kind(), &endpoint_id);
        let trust_source = if entry.selector == "default" {
            "default_environment"
        } else {
            configured_source
        };

        if let Some(existing) = endpoints
            .iter_mut()
            .find(|existing| existing.endpoint_id == endpoint_id)
        {
            if configured_source == "configuration"
                && existing.alias == "default"
                && alias != "default"
            {
                existing.alias = alias;
            }
            continue;
        }
        if endpoints.len() == limit {
            truncated = true;
            continue;
        }
        endpoints.push(RegisteredEndpoint {
            endpoint_id,
            alias,
            selector,
            trust_source: trust_source.into(),
            socket_path: canonical,
        });
    }
    Ok(EndpointRegistration {
        endpoints,
        truncated,
    })
}

pub fn parse_selector(raw: &str) -> Result<EndpointSelector> {
    if raw == "default" {
        return Ok(EndpointSelector::Default);
    }
    if let Some(name) = raw.strip_prefix("name:") {
        validate_socket_name(name)?;
        return Ok(EndpointSelector::Name(name.into()));
    }
    if let Some(path) = raw.strip_prefix("path:") {
        let path = PathBuf::from(path);
        validate_absolute_path(&path)?;
        return Ok(EndpointSelector::Path(path));
    }
    Err(anyhow!(
        "invalid endpoint selector; expected default, name:<name>, or path:<absolute-path>"
    ))
}

fn validate_socket_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 128
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.chars().any(char::is_control)
    {
        return Err(anyhow!("invalid tmux socket name"));
    }
    Ok(())
}

fn validate_absolute_path(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(anyhow!("tmux socket path must be absolute and normalized"));
    }
    Ok(())
}

pub fn socket_path_for(selector: &EndpointSelector) -> Result<PathBuf> {
    match selector {
        EndpointSelector::Path(path) => Ok(path.clone()),
        EndpointSelector::Default => {
            if let Some(path) = current_tmux_socket() {
                return Ok(path);
            }
            Ok(runtime_socket_dir()?.join("default"))
        }
        EndpointSelector::Name(name) => Ok(runtime_socket_dir()?.join(name)),
    }
}

fn current_tmux_socket() -> Option<PathBuf> {
    let raw = env::var("TMUX").ok()?;
    let path = raw.split(',').next()?.trim();
    if path.is_empty() {
        None
    } else {
        let path = PathBuf::from(path);
        path.is_absolute().then_some(path)
    }
}

fn runtime_socket_dir() -> Result<PathBuf> {
    // tmux uses TMUX_TMPDIR and otherwise its compile-time /tmp default; it does
    // not follow the process TMPDIR used by many application runtimes.
    let base = env::var_os("TMUX_TMPDIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    validate_absolute_path(&base)?;
    Ok(base.join(format!("tmux-{}", effective_uid())))
}

fn canonical_identity_path(path: &Path) -> Result<PathBuf> {
    validate_absolute_path(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("tmux socket has no parent directory"))?;
    let leaf = path
        .file_name()
        .ok_or_else(|| anyhow!("tmux socket has no filename"))?;
    let canonical_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    Ok(canonical_parent.join(leaf))
}

pub fn endpoint_id_for(path: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(b"dev.tmx.endpoint.v1\0");
    digest.update(path.as_os_str().as_encoded_bytes());
    format!("ep_{}", hex::encode(digest.finalize()))
}

fn sanitize_alias(raw: &str, fallback_kind: &str, endpoint_id: &str) -> String {
    let mut value = raw
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>();
    value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        value = format!("{fallback_kind}-{}", &endpoint_id[3..11]);
    }
    truncate_utf8(&value, MAX_ALIAS_BYTES)
}

fn truncate_utf8(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.into();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

pub fn verify_socket(endpoint: &RegisteredEndpoint) -> Result<SocketIdentity> {
    verify_socket_path(&endpoint.socket_path)
}

#[cfg(unix)]
fn verify_socket_path(path: &Path) -> Result<SocketIdentity> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};

    let leaf = fs::symlink_metadata(path)
        .with_context(|| format!("stat tmux endpoint {}", path.display()))?;
    if leaf.file_type().is_symlink() {
        return Err(anyhow!("tmux socket leaf is a symlink"));
    }
    if !leaf.file_type().is_socket() {
        return Err(anyhow!("tmux endpoint is not a Unix socket"));
    }
    let uid = effective_uid();
    if leaf.uid() != uid {
        return Err(anyhow!("tmux socket is not owned by the effective user"));
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("tmux socket has no parent directory"))?;
    let parent_meta = fs::metadata(parent)
        .with_context(|| format!("stat tmux socket parent {}", parent.display()))?;
    if parent_meta.uid() != uid {
        return Err(anyhow!("tmux socket parent is not user-owned"));
    }
    if parent_meta.permissions().mode() & 0o022 != 0 {
        return Err(anyhow!("tmux socket parent is writable by group or others"));
    }

    let canonical = canonical_identity_path(path)?;
    Ok(SocketIdentity {
        device: leaf.dev(),
        inode: leaf.ino(),
        uid: leaf.uid(),
        canonical_path: canonical,
    })
}

#[cfg(not(unix))]
fn verify_socket_path(_path: &Path) -> Result<SocketIdentity> {
    Err(anyhow!(
        "tmux switcher endpoints require a local Unix platform"
    ))
}

#[cfg(unix)]
fn effective_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and no side effects.
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn effective_uid() -> u32 {
    0
}

fn discover_named_configs() -> Result<Vec<EndpointConfig>> {
    let dir = runtime_socket_dir()?;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    // Keep enough names to fill the output even when every configured endpoint
    // overlaps discovery. A max-heap makes the selected prefix independent of
    // read_dir order without retaining an unbounded directory listing.
    let mut smallest = BinaryHeap::with_capacity(MAX_DISCOVERY_CANDIDATES + 1);
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if validate_socket_name(&name).is_err() {
            continue;
        }
        let selector = EndpointSelector::Name(name.clone());
        let Ok(path) = socket_path_for(&selector) else {
            continue;
        };
        let placeholder = RegisteredEndpoint {
            endpoint_id: endpoint_id_for(&path),
            alias: format!("discovered-{name}"),
            selector,
            trust_source: "user_runtime_discovery".into(),
            socket_path: path,
        };
        if verify_socket(&placeholder).is_err() {
            continue;
        }
        retain_smallest(&mut smallest, name, MAX_DISCOVERY_CANDIDATES);
    }
    Ok(smallest
        .into_sorted_vec()
        .into_iter()
        .map(|name| EndpointConfig {
            selector: format!("name:{name}"),
            alias: format!("discovered-{name}"),
        })
        .collect())
}

fn retain_smallest(heap: &mut BinaryHeap<String>, candidate: String, limit: usize) {
    if heap.len() < limit {
        heap.push(candidate);
    } else if heap.peek().is_some_and(|largest| candidate < *largest) {
        heap.pop();
        heap.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_commands_are_pinned_to_the_verified_canonical_path() {
        for selector in [
            EndpointSelector::Default,
            EndpointSelector::Name("work".into()),
            EndpointSelector::Path(PathBuf::from("/tmp/untrusted-spelling")),
        ] {
            let endpoint = RegisteredEndpoint {
                endpoint_id: "ep_test".into(),
                alias: "test".into(),
                selector,
                trust_source: "test".into(),
                socket_path: PathBuf::from("/private/tmp/verified.sock"),
            };
            assert_eq!(endpoint.tmux_prefix(), ["-S", "/private/tmp/verified.sock"]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn retargeted_parent_symlink_cannot_change_registered_command_path() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();
        let link = temp.path().join("current");
        std::os::unix::fs::symlink(&first, &link).unwrap();
        let original = link.join("server.sock");
        let verified = first.join("server.sock");
        let endpoint = RegisteredEndpoint {
            endpoint_id: "ep_test".into(),
            alias: "test".into(),
            selector: EndpointSelector::Path(original),
            trust_source: "test".into(),
            socket_path: verified.clone(),
        };
        fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&second, &link).unwrap();
        assert_eq!(
            endpoint.tmux_prefix(),
            ["-S", verified.to_string_lossy().as_ref()]
        );
    }

    #[test]
    fn discovery_prefix_is_deterministic_and_memory_bounded() {
        let names = (0..100)
            .map(|index| format!("socket-{index:03}"))
            .collect::<Vec<_>>();
        let mut forward = BinaryHeap::new();
        let mut reverse = BinaryHeap::new();
        for name in &names {
            retain_smallest(&mut forward, name.clone(), MAX_DISCOVERY_CANDIDATES);
        }
        for name in names.iter().rev() {
            retain_smallest(&mut reverse, name.clone(), MAX_DISCOVERY_CANDIDATES);
        }
        let forward = forward.into_sorted_vec();
        let reverse = reverse.into_sorted_vec();
        assert_eq!(forward, reverse);
        assert_eq!(names[..MAX_DISCOVERY_CANDIDATES], forward[..]);
    }

    #[test]
    fn absolute_paths_are_never_treated_as_names() {
        assert!(parse_selector("name:/tmp/server").is_err());
        let parsed = parse_selector("path:/tmp/server").unwrap();
        assert!(matches!(parsed, EndpointSelector::Path(_)));
    }

    #[test]
    fn endpoint_identity_is_deterministic_and_path_qualified() {
        let a = endpoint_id_for(Path::new("/tmp/a"));
        assert_eq!(a, endpoint_id_for(Path::new("/tmp/a")));
        assert_ne!(a, endpoint_id_for(Path::new("/tmp/b")));
    }

    #[test]
    fn aliases_do_not_expose_paths() {
        let id = endpoint_id_for(Path::new("/tmp/secret.sock"));
        let alias = sanitize_alias("/tmp/secret.sock", "path", &id);
        assert!(!alias.contains("/tmp"));
        assert!(alias.starts_with("path-"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_regular_file_instead_of_unix_socket() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("not-a-socket");
        fs::write(&file, b"not a socket").unwrap();
        let endpoint = RegisteredEndpoint {
            endpoint_id: endpoint_id_for(&file),
            alias: "file".into(),
            selector: EndpointSelector::Path(file.clone()),
            trust_source: "test".into(),
            socket_path: file,
        };
        assert!(verify_socket(&endpoint).is_err());
    }

    #[test]
    fn rejects_symlink_socket_leaf() {
        use std::os::unix::net::UnixListener;

        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real.sock");
        let link = temp.path().join("link.sock");
        let _listener = UnixListener::bind(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(verify_socket_path(&link)
            .unwrap_err()
            .to_string()
            .contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_socket_in_group_writable_parent() {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::net::UnixListener;

        let temp = tempfile::tempdir().unwrap();
        std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
        let socket = temp.path().join("server.sock");
        let _listener = UnixListener::bind(&socket).unwrap();
        assert!(verify_socket_path(&socket)
            .unwrap_err()
            .to_string()
            .contains("writable"));
    }

    #[test]
    fn configured_endpoints_win_when_registration_limit_is_reached() {
        let config = SwitcherConfig {
            max_endpoints: 1,
            endpoints: vec![
                EndpointConfig {
                    selector: "path:/tmp/explicit-first".into(),
                    alias: "first".into(),
                },
                EndpointConfig {
                    selector: "path:/tmp/explicit-second".into(),
                    alias: "second".into(),
                },
            ],
            ..SwitcherConfig::default()
        };
        let registration = register_endpoints(&config).unwrap();
        assert_eq!(registration.endpoints.len(), 1);
        assert_eq!(registration.endpoints[0].alias, "first");
        assert!(registration.truncated);
    }

    #[test]
    fn registration_never_exceeds_contract_wide_endpoint_cap() {
        let config = SwitcherConfig {
            max_endpoints: 100,
            endpoints: (0..40)
                .map(|index| EndpointConfig {
                    selector: format!("path:/tmp/endpoint-{index}"),
                    alias: format!("endpoint-{index}"),
                })
                .collect(),
            ..SwitcherConfig::default()
        };
        let registration = register_endpoints(&config).unwrap();
        assert_eq!(registration.endpoints.len(), 32);
        assert!(registration.truncated);
        assert_eq!(registration.endpoints[0].alias, "endpoint-0");
        assert_eq!(registration.endpoints[31].alias, "endpoint-31");
    }

    #[test]
    fn equivalent_aliases_collapse_to_one_endpoint_identity() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("server.sock");
        let config = SwitcherConfig {
            endpoints: vec![
                EndpointConfig {
                    selector: format!("path:{}", path.display()),
                    alias: "one".into(),
                },
                EndpointConfig {
                    selector: format!("path:{}", path.display()),
                    alias: "two".into(),
                },
            ],
            ..SwitcherConfig::default()
        };
        let registration = register_endpoints(&config).unwrap();
        assert_eq!(registration.endpoints.len(), 1);
        assert!(!registration.truncated);
    }
}
