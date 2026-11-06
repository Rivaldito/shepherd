//! Client for the Shepherd server's JSON socket API (newline-delimited JSON, one request per
//! connection; `events.subscribe` keeps its connection open). The desktop app is only a client:
//! the server owns workspaces, terminals and agents. Unix sockets only for now.

use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::Duration;

/// Lifecycle events the UI cares about. They only say "something changed": the UI re-reads
/// `session.snapshot` instead of replaying payloads (the server documents it that way).
pub const EVENT_TYPES: &[&str] = &[
    "workspace.created",
    "workspace.updated",
    "workspace.renamed",
    "workspace.moved",
    "workspace.reordered",
    "workspace.closed",
    "workspace.focused",
    "tab.created",
    "tab.closed",
    "tab.focused",
    "tab.renamed",
    "tab.moved",
    "pane.created",
    "pane.updated",
    "pane.closed",
    "pane.focused",
    "pane.moved",
    "pane.exited",
    "pane.agent_detected",
    "layout.updated",
    "worktree.created",
    "worktree.opened",
    "worktree.removed",
];

/// Method families the webview may call. Server control (`server.*`), plugins, integrations and
/// the raw agent-reporting methods stay out of reach of the UI.
pub fn allowed_method(method: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "workspace.",
        "tab.",
        "pane.",
        "agent.",
        "worktree.",
        "layout.",
        "shepherd.",
    ];
    const DENIED: &[&str] = &[
        "pane.report_agent",
        "pane.report_agent_session",
        "pane.report_metadata",
        "pane.clear_agent_authority",
        "pane.release_agent",
    ];
    if DENIED.contains(&method) {
        return false;
    }
    matches!(method, "ping" | "session.snapshot") || PREFIXES.iter().any(|p| method.starts_with(p))
}

/// Same resolution as the CLI: `SHEPHERD_SOCKET_PATH`, then `SHEPHERD_SESSION`, then the default.
pub fn socket_path_from(
    socket_env: Option<&str>,
    session_env: Option<&str>,
    config_dir: &Path,
) -> PathBuf {
    if let Some(path) = socket_env.filter(|s| !s.is_empty()) {
        return PathBuf::from(path);
    }
    match session_env.filter(|s| !s.is_empty()) {
        Some(name) => config_dir.join("sessions").join(name).join("shepherd.sock"),
        None => config_dir.join("shepherd.sock"),
    }
}

pub fn default_socket_path() -> PathBuf {
    let config_dir = shepherd_core::config::global_config_path()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    socket_path_from(
        std::env::var("SHEPHERD_SOCKET_PATH").ok().as_deref(),
        std::env::var("SHEPHERD_SESSION").ok().as_deref(),
        &config_dir,
    )
}

/// Where the server binary comes from: `SHEPHERD_BIN`, then `shepherd` on `PATH`, then next to this app.
pub fn find_server_bin(
    env_bin: Option<&str>,
    path_env: Option<&OsStr>,
    exe_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(bin) = env_bin.filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(bin));
    }
    let on_path = path_env
        .into_iter()
        .flat_map(std::env::split_paths)
        .map(|d| d.join("shepherd"));
    on_path
        .chain(exe_dir.map(|d| d.join("shepherd")))
        .find(|p| p.is_file())
}

pub fn default_server_bin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    find_server_bin(
        std::env::var("SHEPHERD_BIN").ok().as_deref(),
        std::env::var_os("PATH").as_deref(),
        exe.as_deref().and_then(Path::parent),
    )
}

fn format_error(err: &Value) -> String {
    let code = err.get("code").and_then(Value::as_str).unwrap_or("error");
    match err.get("message").and_then(Value::as_str) {
        Some(message) => format!("{code}: {message}"),
        None => code.to_string(),
    }
}

/// One request, one response. Returns the `result` value, or the server's error as text.
#[cfg(unix)]
pub fn call(path: &Path, method: &str, params: Value) -> Result<Value, String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let timeout = Some(Duration::from_secs(10));
    let mut stream = UnixStream::connect(path)
        .map_err(|e| format!("no se pudo conectar a {}: {e}", path.display()))?;
    stream
        .set_read_timeout(timeout)
        .and_then(|_| stream.set_write_timeout(timeout))
        .map_err(|e| e.to_string())?;
    let id = format!("desktop-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
    let mut line = serde_json::to_vec(&json!({ "id": id, "method": method, "params": params }))
        .map_err(|e| e.to_string())?;
    line.push(b'\n');
    stream.write_all(&line).map_err(|e| e.to_string())?;
    let mut reply = String::new();
    BufReader::new(stream)
        .read_line(&mut reply)
        .map_err(|e| e.to_string())?;
    if reply.trim().is_empty() {
        return Err("el servidor cerró la conexión sin responder".into());
    }
    let response: Value =
        serde_json::from_str(&reply).map_err(|e| format!("respuesta inválida: {e}"))?;
    if let Some(err) = response.get("error") {
        return Err(format_error(err));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "respuesta sin result".into())
}

/// Subscribes and blocks, handing every pushed event to `on_event`. Returns `Ok` when the server
/// closes the connection and `Err` for a rejected subscription or `events_lost`; the caller reconnects.
#[cfg(unix)]
pub fn read_events(
    path: &Path,
    types: &[&str],
    on_ready: &mut dyn FnMut(),
    on_event: &mut dyn FnMut(Value),
) -> Result<(), String> {
    let mut stream = UnixStream::connect(path)
        .map_err(|e| format!("no se pudo conectar a {}: {e}", path.display()))?;
    let subs: Vec<Value> = types.iter().map(|t| json!({ "type": t })).collect();
    let mut line = serde_json::to_vec(&json!({ "id": "desktop-events", "method": "events.subscribe", "params": { "subscriptions": subs } }))
        .map_err(|e| e.to_string())?;
    line.push(b'\n');
    stream.write_all(&line).map_err(|e| e.to_string())?;
    let mut acked = false;
    for line in BufReader::new(stream).lines() {
        let value: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if let Some(err) = value.get("error") {
            return Err(format_error(err));
        }
        if !acked {
            acked = true;
            on_ready();
            continue;
        }
        on_event(value);
    }
    Ok(())
}

/// Starts `shepherd server` detached so it outlives this app, then waits until it answers `ping`.
#[cfg(unix)]
pub fn start_server(bin: &Path, socket: &Path) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    Command::new(bin)
        .arg("server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("no se pudo ejecutar {}: {e}", bin.display()))?;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(200));
        if call(socket, "ping", json!({})).is_ok() {
            return Ok(());
        }
    }
    Err("el servidor no respondió tras arrancar".into())
}

#[cfg(not(unix))]
pub fn call(_: &Path, _: &str, _: Value) -> Result<Value, String> {
    Err("la conexión al servidor solo está implementada en Unix".into())
}

#[cfg(not(unix))]
pub fn read_events(
    _: &Path,
    _: &[&str],
    _: &mut dyn FnMut(),
    _: &mut dyn FnMut(Value),
) -> Result<(), String> {
    Err("la conexión al servidor solo está implementada en Unix".into())
}

#[cfg(not(unix))]
pub fn start_server(_: &Path, _: &Path) -> Result<(), String> {
    Err("la conexión al servidor solo está implementada en Unix".into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A one-connection fake server: checks the request line, then writes `replies` verbatim.
    fn fake_server(
        dir: &Path,
        expect_method: &'static str,
        replies: &'static [&'static str],
    ) -> (PathBuf, std::thread::JoinHandle<()>) {
        let path = dir.join("s.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut request)
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["method"], expect_method);
            for reply in replies {
                stream.write_all(reply.as_bytes()).unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });
        (path, handle)
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shepherd-desktop-api-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn call_returns_result_and_surfaces_server_errors() {
        let dir = temp("call");
        let (path, server) = fake_server(
            &dir,
            "pane.read",
            &[r#"{"id":"x","result":{"type":"pane_read","read":{"text":"hola"}}}"#],
        );
        let result = call(&path, "pane.read", json!({ "pane_id": "w1:p1" })).unwrap();
        server.join().unwrap();
        assert_eq!(result["read"]["text"], "hola");

        std::fs::remove_file(&path).unwrap();
        let (path, server) = fake_server(
            &dir,
            "pane.read",
            &[r#"{"id":"x","error":{"code":"pane_not_found","message":"no such pane"}}"#],
        );
        let err = call(&path, "pane.read", json!({})).unwrap_err();
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(err, "pane_not_found: no such pane");
    }

    #[test]
    fn call_reports_an_unreachable_server() {
        let err = call(Path::new("/nonexistent/shepherd.sock"), "ping", json!({})).unwrap_err();
        assert!(err.contains("no se pudo conectar"), "{err}");
    }

    #[test]
    fn events_are_forwarded_after_the_ack_and_end_on_close() {
        let dir = temp("events");
        let (path, server) = fake_server(
            &dir,
            "events.subscribe",
            &[
                r#"{"id":"s","result":{"type":"subscription_started"}}"#,
                r#"{"event":"workspace.created","data":{"n":1}}"#,
                r#"{"event":"pane.closed","data":{}}"#,
            ],
        );
        let (mut ready, mut seen) = (0, Vec::new());
        read_events(&path, EVENT_TYPES, &mut || ready += 1, &mut |e| {
            seen.push(e["event"].as_str().unwrap().to_string())
        })
        .unwrap();
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(ready, 1);
        assert_eq!(seen, ["workspace.created", "pane.closed"]);
    }

    #[test]
    fn a_rejected_subscription_is_an_error() {
        let dir = temp("events-lost");
        let (path, server) = fake_server(
            &dir,
            "events.subscribe",
            &[r#"{"id":"s","error":{"code":"events_lost","message":"too slow"}}"#],
        );
        let err = read_events(&path, EVENT_TYPES, &mut || {}, &mut |_| {}).unwrap_err();
        server.join().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(err, "events_lost: too slow");
    }

    #[test]
    fn only_runtime_methods_are_allowed() {
        for ok in [
            "ping",
            "session.snapshot",
            "workspace.create",
            "tab.list",
            "pane.read",
            "pane.send_keys",
            "agent.start",
            "worktree.list",
            "layout.export",
            "shepherd.agent.spawn",
        ] {
            assert!(allowed_method(ok), "{ok}");
        }
        for no in [
            "server.stop",
            "plugin.pane.open",
            "integration.install",
            "events.subscribe",
            "pane.report_agent",
            "pane.release_agent",
            "client.window_title.set",
            "workspacex.list",
            "",
        ] {
            assert!(!allowed_method(no), "{no}");
        }
    }

    #[test]
    fn socket_path_follows_the_cli_resolution_order() {
        let cfg = Path::new("/home/u/.config/shepherd");
        assert_eq!(
            socket_path_from(Some("/tmp/x.sock"), Some("work"), cfg),
            Path::new("/tmp/x.sock")
        );
        assert_eq!(
            socket_path_from(None, Some("work"), cfg),
            Path::new("/home/u/.config/shepherd/sessions/work/shepherd.sock")
        );
        assert_eq!(
            socket_path_from(Some(""), None, cfg),
            Path::new("/home/u/.config/shepherd/shepherd.sock")
        );
    }

    #[test]
    fn server_binary_lookup_prefers_the_env_override_then_path_then_the_app_dir() {
        let dir = temp("bin");
        let (a, b) = (dir.join("a"), dir.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(b.join("shepherd"), "").unwrap();
        let path_env = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(
            find_server_bin(Some("/opt/shepherd"), Some(&path_env), None),
            Some(PathBuf::from("/opt/shepherd"))
        );
        assert_eq!(
            find_server_bin(None, Some(&path_env), None),
            Some(b.join("shepherd"))
        );
        assert_eq!(
            find_server_bin(None, None, Some(&b)),
            Some(b.join("shepherd"))
        );
        assert_eq!(
            find_server_bin(None, Some(&path_env), Some(&a)),
            Some(b.join("shepherd"))
        );
        assert_eq!(find_server_bin(None, None, Some(&a)), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Live check against a real server: `SHEPHERD_SOCKET_PATH=… cargo test -- --ignored live`.
    #[test]
    #[ignore]
    fn live_ping_and_snapshot() {
        let path = default_socket_path();
        assert_eq!(call(&path, "ping", json!({})).unwrap()["type"], "pong");
        assert_eq!(
            call(&path, "session.snapshot", json!({})).unwrap()["type"],
            "session_snapshot"
        );
    }
}
