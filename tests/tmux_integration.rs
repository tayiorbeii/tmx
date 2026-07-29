mod support;

use support::TmuxServer;
use tmx::tmux::Tmux;

#[test]
fn isolated_tmux_lists_and_renames() {
    let server = TmuxServer::new("test");
    let tmux = Tmux::with_socket_path(server.socket_path().to_string_lossy());
    let sessions = tmux.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "test");
    assert_eq!(sessions[0].path, "/tmp");

    tmux.rename("session", "test", "renamed").unwrap();
    let sessions = tmux.list_sessions().unwrap();
    assert_eq!(sessions[0].name, "renamed");

    tmux.set_option("renamed", "@tmx.note", "hello", None)
        .unwrap();
    assert_eq!(
        tmux.get_option("renamed", "@tmx.note", Some("session"))
            .unwrap()
            .as_deref(),
        Some("hello")
    );
}

#[test]
fn isolated_tmux_creates_session_with_cwd() {
    let server = TmuxServer::new("seed");
    let tmux = Tmux::with_socket_path(server.socket_path().to_string_lossy());
    tmux.new_session("cwd-test", "/tmp").unwrap();
    let sessions = tmux.list_sessions().unwrap();
    let created = sessions
        .iter()
        .find(|session| session.name == "cwd-test")
        .unwrap();
    assert_eq!(created.path, "/tmp");
}
