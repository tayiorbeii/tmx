use anyhow::{anyhow, Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::io::{self, Write};

use crate::cli::{
    generate_completions, Cli, Command, NewArgs, NoteArgs, PaletteArgs, RenameArgs, ScopeArg,
    ViewArgs,
};
use crate::config::{config_path, Config};
use crate::mobile::{select_profile, ProfileInputs};
use crate::model::{CurrentTarget, PaletteRow, TargetKind, UiProfile};
use crate::palette::{build_rows, LiveTargets};
use crate::project::{
    origin_inputs_from_env, project_name, repo_root, resolve_origin_cwd, sanitize_session_name,
    session_name_for,
};
use crate::selector::{default_selector, Selector};
use crate::state::{
    stable_pane_key, stable_session_key, stable_session_parts, stable_window_key, Store,
};
use crate::tmux::Tmux;

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Completions(args)) = cli.command.as_ref() {
        generate_completions(args.shell, &mut io::stdout());
        return Ok(());
    }
    if matches!(cli.command, Some(Command::Doctor)) {
        return doctor();
    }
    let cfg = Config::load()?;
    let tmux = Tmux::new();
    let explicit = explicit_profile(cli.desktop, cli.mobile, cli.command.as_ref());
    let profile = determine_profile(&cfg, &tmux, explicit)?;
    let store = Store::open(cfg.state_path())?;
    let mut ctx = Ctx {
        tmux,
        store,
        profile,
        inside_tmux: Tmux::is_inside_tmux(),
    };

    match cli.command {
        None => palette(&mut ctx),
        Some(Command::Palette(args)) => {
            ctx.profile = profile_from_palette_args(ctx.profile, &args);
            palette(&mut ctx)
        }
        Some(Command::Ls) => list(&mut ctx),
        Some(Command::View(args)) => view(&mut ctx, args),
        Some(Command::New(args)) => new_session(&mut ctx, args),
        Some(Command::Last) => last(&mut ctx),
        Some(Command::Recent(args)) => {
            ctx.profile = profile_from_palette_args(ctx.profile, &args);
            recent(&mut ctx)
        }
        Some(Command::Note(args)) => note(&mut ctx, args),
        Some(Command::Rename(args)) => rename(&mut ctx, args),
        Some(Command::Doctor | Command::Completions(_)) => unreachable!(),
    }
}

struct Ctx {
    tmux: Tmux,
    store: Store,
    profile: UiProfile,
    inside_tmux: bool,
}

fn explicit_profile(desktop: bool, mobile: bool, cmd: Option<&Command>) -> (bool, bool) {
    let mut d = desktop;
    let mut m = mobile;
    if let Some(Command::Palette(PaletteArgs { desktop, mobile }))
    | Some(Command::Recent(PaletteArgs { desktop, mobile })) = cmd
    {
        d |= *desktop;
        m |= *mobile;
    }
    (d, m)
}

fn profile_from_palette_args(current: UiProfile, args: &PaletteArgs) -> UiProfile {
    if args.desktop {
        UiProfile::Desktop
    } else if args.mobile {
        UiProfile::Mobile
    } else {
        current
    }
}

fn determine_profile(cfg: &Config, tmux: &Tmux, explicit: (bool, bool)) -> Result<UiProfile> {
    let client_size = if !explicit.0 && !explicit.1 && Tmux::is_inside_tmux() {
        tmux.client_size().unwrap_or(None)
    } else {
        None
    };
    Ok(select_profile(&ProfileInputs {
        explicit_desktop: explicit.0,
        explicit_mobile: explicit.1,
        env_tmx_ui: std::env::var("TMX_UI").ok(),
        config_default_ui: cfg.default_ui.clone(),
        mobile_width_threshold: cfg.mobile_width_threshold,
        mobile_height_threshold: cfg.mobile_height_threshold,
        client_size,
    }))
}

fn live(ctx: &mut Ctx) -> Result<LiveTargets> {
    let mut sessions = ctx.tmux.list_sessions()?;
    let mut windows = ctx.tmux.list_windows()?;
    let mut panes = ctx.tmux.list_panes()?;
    ctx.store
        .sync_live_targets(&sessions, &windows, &panes)
        .ok();
    hydrate_notes(
        &ctx.store,
        &ctx.tmux,
        &mut sessions,
        &mut windows,
        &mut panes,
    );
    Ok(LiveTargets {
        sessions,
        windows,
        panes,
    })
}

fn hydrate_notes(
    store: &Store,
    tmux: &Tmux,
    sessions: &mut [crate::tmux::formats::SessionInfo],
    windows: &mut [crate::tmux::formats::WindowInfo],
    panes: &mut [crate::tmux::formats::PaneInfo],
) {
    for s in sessions {
        s.note = store
            .get_note("session", &stable_session_key(s))
            .ok()
            .flatten()
            .or_else(|| {
                tmux.get_option(&s.id, "@tmx.note", Some("session"))
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
    }
    for w in windows {
        w.note = store
            .get_note("window", &stable_window_key(w))
            .ok()
            .flatten()
            .or_else(|| {
                tmux.get_option(&w.id, "@tmx.note", Some("window"))
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
    }
    for p in panes {
        p.note = store
            .get_note("pane", &stable_pane_key(p))
            .ok()
            .flatten()
            .or_else(|| {
                tmux.get_option(&p.id, "@tmx.note", Some("pane"))
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
    }
}

fn palette(ctx: &mut Ctx) -> Result<()> {
    let live = live(ctx)?;
    let rows = build_rows(&live, ctx.profile);
    let selector = default_selector();
    if let Some(row) = selector.select(&rows, ctx.profile)? {
        execute_row(ctx, row, Some(&live))?;
    }
    Ok(())
}

fn execute_row(ctx: &mut Ctx, row: PaletteRow, live: Option<&LiveTargets>) -> Result<()> {
    if row.row_type == TargetKind::Action {
        return execute_action(ctx, &row.row_id);
    }
    let target = row
        .target_id
        .as_deref()
        .context("selected row has no target id")?;
    let stable = live
        .and_then(|l| stable_key_for_target(l, target))
        .unwrap_or_else(|| target.to_string());
    let kind = row
        .target_kind
        .as_ref()
        .map(TargetKind::as_str)
        .unwrap_or("target");
    ctx.store
        .push_mru(
            kind,
            &stable,
            Some(target),
            client_tty(&ctx.tmux).as_deref(),
        )
        .ok();
    ctx.tmux.switch_to(target, ctx.inside_tmux)
}

fn execute_action(ctx: &mut Ctx, action_id: &str) -> Result<()> {
    match action_id {
        "a:new" => new_session(
            ctx,
            NewArgs {
                name: None,
                label: None,
            },
        ),
        "a:last" => last(ctx),
        "a:note" => {
            let text = prompt_line("note")?;
            note(
                ctx,
                NoteArgs {
                    scope: ScopeArg::Session,
                    set: Some(text),
                },
            )
        }
        "a:rename" => {
            let text = prompt_line("session name")?;
            rename(
                ctx,
                RenameArgs {
                    scope: ScopeArg::Session,
                    name: Some(text),
                },
            )
        }
        other => Err(anyhow!("unknown action {other}")),
    }
}

fn list(ctx: &mut Ctx) -> Result<()> {
    let live = live(ctx)?;
    for s in live.sessions {
        println!("S\t{}\t{}\t{}", s.id, s.name, s.path);
    }
    for w in live.windows {
        println!("W\t{}\t{}/{}\t{}", w.id, w.session_name, w.name, w.cwd);
    }
    for p in live.panes {
        println!(
            "P\t{}\t{}/{}/{}\t{}",
            p.id, p.session_name, p.window_name, p.command, p.cwd
        );
    }
    Ok(())
}

fn view(ctx: &mut Ctx, args: ViewArgs) -> Result<()> {
    if let Some(target) = args.target {
        let target = resolve_target(ctx, &target)?;
        ctx.store
            .push_mru(
                target_kind_for_id(&target),
                &target,
                Some(&target),
                client_tty(&ctx.tmux).as_deref(),
            )
            .ok();
        ctx.tmux.switch_to(&target, ctx.inside_tmux)
    } else {
        palette(ctx)
    }
}

fn resolve_target(ctx: &mut Ctx, raw: &str) -> Result<String> {
    if matches!(raw.chars().next(), Some('$' | '@' | '%')) {
        return Ok(raw.to_string());
    }
    let sessions = ctx.tmux.list_sessions()?;
    if let Some(s) = sessions.iter().find(|s| s.name == raw) {
        return Ok(s.id.clone());
    }
    if let Some(s) = sessions.iter().find(|s| s.name.contains(raw)) {
        return Ok(s.id.clone());
    }
    Err(anyhow!("no tmux target matches {raw:?}"))
}

fn new_session(ctx: &mut Ctx, args: NewArgs) -> Result<()> {
    let cwd = resolve_origin_cwd(&origin_inputs_from_env(), |pane| {
        ctx.tmux.display_pane_cwd(pane)
    })?;
    let existing: HashSet<String> = ctx
        .tmux
        .list_sessions()?
        .into_iter()
        .map(|s| s.name)
        .collect();
    let desired = args
        .name
        .as_deref()
        .map(sanitize_session_name)
        .unwrap_or_else(|| project_name(&cwd));
    let name = if args.label.is_none() {
        desired
    } else {
        session_name_for(&cwd, args.name.as_deref(), args.label.as_deref(), &existing)
    };
    let cwd_s = cwd.display().to_string();
    if existing.contains(&name) {
        ctx.tmux.switch_to(&name, ctx.inside_tmux)?;
    } else {
        ctx.tmux.new_session(&name, &cwd_s)?;
        ctx.tmux.set_option(&name, "@tmx.cwd", &cwd_s, None).ok();
        if let Some(root) = repo_root(&cwd) {
            ctx.tmux
                .set_option(&name, "@tmx.repo", &root.display().to_string(), None)
                .ok();
        }
        if let Some(label) = args.label.as_deref() {
            ctx.tmux.set_option(&name, "@tmx.label", label, None).ok();
        }
        ctx.tmux.switch_to(&name, ctx.inside_tmux)?;
    }
    let repo = repo_root(&cwd).map(|p| p.display().to_string());
    ctx.store
        .upsert_project(&cwd_s, repo.as_deref(), &name, "tmux")?;
    ctx.store
        .push_mru(
            "session",
            &cwd_s,
            Some(&name),
            client_tty(&ctx.tmux).as_deref(),
        )
        .ok();
    Ok(())
}

fn last(ctx: &mut Ctx) -> Result<()> {
    let current = ctx.tmux.current_target().ok();
    for entry in ctx.store.recent_mru(50)? {
        if let Some(target) = entry.tmux_target {
            if Some(target.as_str()) == current.as_ref().map(|c| c.pane_id.as_str())
                || Some(target.as_str()) == current.as_ref().map(|c| c.window_id.as_str())
                || Some(target.as_str()) == current.as_ref().map(|c| c.session_id.as_str())
            {
                continue;
            }
            if ctx.tmux.target_exists(&target) {
                ctx.store
                    .push_mru(
                        &entry.target_kind,
                        &entry.stable_key,
                        Some(&target),
                        client_tty(&ctx.tmux).as_deref(),
                    )
                    .ok();
                return ctx.tmux.switch_to(&target, ctx.inside_tmux);
            }
        }
    }
    Err(anyhow!("no previous live target in tmx MRU"))
}

fn recent(ctx: &mut Ctx) -> Result<()> {
    let mut rows = Vec::new();
    for e in ctx.store.recent_mru(100)? {
        let Some(target) = e.tmux_target.clone() else {
            continue;
        };
        if !ctx.tmux.target_exists(&target) {
            continue;
        }
        let kind = kind_from_str(&e.target_kind).unwrap_or(TargetKind::Session);
        rows.push(PaletteRow::new(
            format!("recent:{target}"),
            kind.clone(),
            format!("R {:<8} {}", e.target_kind, e.stable_key),
            Some(kind),
            Some(target),
            format!("{} {}", e.target_kind, e.stable_key),
        ));
    }
    let selector = default_selector();
    if let Some(row) = selector.select(&rows, ctx.profile)? {
        execute_row(ctx, row, None)?;
    }
    Ok(())
}

fn note(ctx: &mut Ctx, args: NoteArgs) -> Result<()> {
    let current = ctx.tmux.current_target()?;
    let (target, key) = scoped_target_and_key(&current, args.scope);
    if let Some(text) = args.set {
        let ts = ctx.store.set_note(args.scope.as_str(), &key, &text)?;
        ctx.tmux
            .set_option(&target, "@tmx.note", &text, Some(args.scope.as_str()))
            .ok();
        ctx.tmux
            .set_option(
                &target,
                "@tmx.last_note_at",
                &ts.to_string(),
                Some(args.scope.as_str()),
            )
            .ok();
    } else if let Some(text) = ctx.store.get_note(args.scope.as_str(), &key)?.or_else(|| {
        ctx.tmux
            .get_option(&target, "@tmx.note", Some(args.scope.as_str()))
            .ok()
            .flatten()
    }) {
        println!("{text}");
    }
    Ok(())
}

fn rename(ctx: &mut Ctx, args: RenameArgs) -> Result<()> {
    let current = ctx.tmux.current_target()?;
    let (target, _) = scoped_target_and_key(&current, args.scope);
    let name = match args.name {
        Some(n) => n,
        None => prompt_line(args.scope.as_str())?,
    };
    if name.trim().is_empty() {
        return Err(anyhow!("empty name"));
    }
    ctx.tmux.rename(args.scope.as_str(), &target, &name)
}

fn scoped_target_and_key(current: &CurrentTarget, scope: ScopeArg) -> (String, String) {
    let session_key = stable_session_parts(&current.session_path, &current.session_name);
    match scope {
        ScopeArg::Session => (current.session_id.clone(), session_key),
        ScopeArg::Window => (
            current.window_id.clone(),
            format!("{}:window:{}", session_key, current.window_name),
        ),
        ScopeArg::Pane => (
            current.pane_id.clone(),
            format!(
                "{}:window:{}:pane:{}:{}:{}",
                session_key,
                current.window_name,
                current.pane_title,
                current.cwd,
                current.pane_command
            ),
        ),
    }
}

fn stable_key_for_target(live: &LiveTargets, target: &str) -> Option<String> {
    live.sessions
        .iter()
        .find(|s| s.id == target)
        .map(stable_session_key)
        .or_else(|| {
            live.windows
                .iter()
                .find(|w| w.id == target)
                .map(stable_window_key)
        })
        .or_else(|| {
            live.panes
                .iter()
                .find(|p| p.id == target)
                .map(stable_pane_key)
        })
}

fn target_kind_for_id(target: &str) -> &'static str {
    match target.chars().next() {
        Some('%') => "pane",
        Some('@') => "window",
        _ => "session",
    }
}

fn kind_from_str(s: &str) -> Option<TargetKind> {
    match s {
        "session" => Some(TargetKind::Session),
        "window" => Some(TargetKind::Window),
        "pane" => Some(TargetKind::Pane),
        _ => None,
    }
}

fn client_tty(tmux: &Tmux) -> Option<String> {
    tmux.run(["display-message", "-p", "#{client_tty}"])
        .ok()
        .filter(|s| !s.is_empty())
}

fn prompt_line(label: &str) -> Result<String> {
    eprint!("{label}: ");
    io::stderr().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim_end_matches(['\n', '\r']).to_string())
}

fn doctor() -> Result<()> {
    let tmux = Tmux::new();
    println!("tmx doctor");
    check_bin("tmux");
    check_bin("fzf");
    check_bin("git");
    check_optional("fd");
    check_optional("rg");
    check_optional("zoxide");
    check_optional("tv");
    println!("[INFO] fff feature: not compiled/enabled in MVP");
    match tmux.version() {
        Ok(v) => println!("[OK] tmux version: {v}"),
        Err(e) => println!("[FAIL] tmux version: {e}"),
    }
    println!(
        "[{}] inside tmux: {}",
        if Tmux::is_inside_tmux() { "OK" } else { "WARN" },
        Tmux::is_inside_tmux()
    );
    let cfg_path = config_path();
    println!(
        "[INFO] config path: {} ({})",
        cfg_path.display(),
        if cfg_path.exists() {
            "readable"
        } else {
            "missing, defaults used"
        }
    );
    let cfg = Config::load().unwrap_or_default();
    let state_path = cfg.state_path();
    match Store::open(&state_path) {
        Ok(store) => println!("[OK] SQLite path writable: {}", store.path().display()),
        Err(e) => println!("[FAIL] SQLite path writable: {e}"),
    }
    let inputs = origin_inputs_from_env();
    match resolve_origin_cwd(&inputs, |pane| tmux.display_pane_cwd(pane)) {
        Ok(p) => println!("[OK] origin cwd: {}", p.display()),
        Err(e) => println!("[WARN] origin cwd: {e}"),
    }
    println!("[INFO] popup capability: likely if tmux >= 3.2 and inside tmux");
    Ok(())
}

fn check_bin(bin: &str) {
    match which::which(bin) {
        Ok(p) => println!("[OK] {bin}: {}", p.display()),
        Err(_) => println!("[FAIL] {bin}: not found"),
    }
}

fn check_optional(bin: &str) {
    match which::which(bin) {
        Ok(p) => println!("[OK] {bin}: {}", p.display()),
        Err(_) => println!("[WARN] {bin}: not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::Tmux;

    #[test]
    fn target_kind_detection() {
        assert_eq!(target_kind_for_id("%1"), "pane");
        assert_eq!(target_kind_for_id("@1"), "window");
        assert_eq!(target_kind_for_id("$1"), "session");
    }

    #[test]
    fn scoped_keys_include_context() {
        let current = CurrentTarget {
            session_id: "$1".into(),
            session_name: "s".into(),
            session_path: "/tmp".into(),
            window_id: "@1".into(),
            window_name: "w".into(),
            window_index: "0".into(),
            pane_id: "%1".into(),
            pane_title: "p".into(),
            pane_command: "zsh".into(),
            cwd: "/tmp".into(),
        };
        assert_eq!(scoped_target_and_key(&current, ScopeArg::Window).0, "@1");
        assert_eq!(
            scoped_target_and_key(&current, ScopeArg::Session).1,
            "cwd:/tmp"
        );
        assert_eq!(
            scoped_target_and_key(&current, ScopeArg::Pane).1,
            "cwd:/tmp:window:w:pane:p:/tmp:zsh"
        );
    }

    #[test]
    fn command_generation_available() {
        let args = Tmux::rename_args("pane", "%1", "logs");
        assert_eq!(args, vec!["select-pane", "-t", "%1", "-T", "logs"]);
        assert_eq!(
            Tmux::new_session_args("api", "/tmp"),
            vec!["new-session", "-d", "-s", "api", "-c", "/tmp"]
        );
    }
}
