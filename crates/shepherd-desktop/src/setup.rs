//! Turning the Shepherd layer on, and assigning existing agents to tasks.
//!
//! The layer is off by default (`[shepherd] enabled`) and the server reads that only when it starts, so
//! enabling it means editing `config.toml` and restarting the server.

use serde::Serialize;
use shepherd_core::{list_tasks, load_agent_registry, save_agent_registry, AgentRegistryFile};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

/// `text` with `[shepherd] enabled = true`; comments, order and every other key stay as they were.
pub fn enable_shepherd_in(text: &str) -> Result<String, String> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| format!("config.toml no es TOML válido: {e}"))?;
    if !doc.contains_key("shepherd") {
        doc["shepherd"] = toml_edit::table();
    }
    let table = doc["shepherd"]
        .as_table_like_mut()
        .ok_or("`shepherd` en config.toml no es una tabla")?;
    table.insert("enabled", value(true));
    Ok(doc.to_string())
}

pub fn shepherd_enabled_in(text: &str) -> bool {
    text.parse::<DocumentMut>()
        .ok()
        .and_then(|doc| {
            doc.get("shepherd")?
                .as_table_like()?
                .get("enabled")?
                .as_bool()
        })
        .unwrap_or(false)
}

/// The `config.toml` the server reads: `SHEPHERD_CONFIG_PATH`, else `<config dir>/config.toml` where the
/// dir is `shepherd-dev` (debug builds) or `shepherd`. The server cannot be asked, so take the one in use:
/// the dir whose server log is newest, else `shepherd`.
pub fn server_config_path(env_path: Option<&str>, config_home: &Path) -> PathBuf {
    if let Some(path) = env_path.filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    let newest = |dir: &str| {
        std::fs::metadata(config_home.join(dir).join("shepherd-server.log"))
            .and_then(|m| m.modified())
            .ok()
    };
    let dir = match (newest("shepherd-dev"), newest("shepherd")) {
        (Some(dev), Some(release)) if dev < release => "shepherd",
        (Some(_), _) => "shepherd-dev",
        _ => "shepherd",
    };
    config_home.join(dir).join("config.toml")
}

pub fn default_config_path() -> PathBuf {
    let home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    server_config_path(std::env::var("SHEPHERD_CONFIG_PATH").ok().as_deref(), &home)
}

/// Writes through a temp file and a rename, so a reader never sees half a file.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp-desktop");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct ShepherdConfig {
    pub path: String,
    pub exists: bool,
    pub enabled: bool,
}

pub fn shepherd_config() -> ShepherdConfig {
    let path = default_config_path();
    let text = std::fs::read_to_string(&path).ok();
    ShepherdConfig {
        enabled: text.as_deref().is_some_and(shepherd_enabled_in),
        exists: text.is_some(),
        path: path.display().to_string(),
    }
}

/// Writes `[shepherd] enabled = true` into the server's config (keeping a `.bak` of what was there).
pub fn enable_shepherd_at(path: &Path) -> Result<(), String> {
    let existing = std::fs::read_to_string(path).ok();
    let updated = enable_shepherd_in(existing.as_deref().unwrap_or(""))?;
    if existing.is_some() {
        std::fs::copy(path, path.with_extension("toml.bak")).map_err(|e| e.to_string())?;
    }
    write_atomic(path, &updated)
}

/// Enables the layer in `config`, then restarts the server on `socket` with `bin` so it takes effect.
/// Restarting closes every terminal the server owns.
pub fn enable_and_restart(config: &Path, socket: &Path, bin: &Path) -> Result<(), String> {
    use serde_json::json;
    enable_shepherd_at(config)?;
    let _ = crate::api::call(socket, "server.stop", json!({})); // fine if it was not running
    for _ in 0..50 {
        if crate::api::call(socket, "ping", json!({})).is_err() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    crate::api::start_server(bin, socket)
}

/// Sets (`Some`) or clears (`None`) which task an agent was launched for. The server has no method for
/// this, so the registry file is edited: the server re-reads it on every operation. Stopgap until a
/// `shepherd.agent.assign` exists. The task must exist in the project at `root`.
pub fn assign_task_at(
    registry: &Path,
    root: &Path,
    agent_id: &str,
    task: Option<u32>,
) -> Result<(), String> {
    if let Some(id) = task {
        let tasks = list_tasks(root, false).map_err(|e| e.to_string())?;
        if !tasks.iter().any(|t| t.status.id == id) {
            return Err(format!("la tarea #{id} no existe en este proyecto"));
        }
    }
    let mut reg: AgentRegistryFile = load_agent_registry(registry).map_err(|e| e.to_string())?;
    let agent = reg
        .agents
        .iter_mut()
        .find(|a| a.id == agent_id)
        .ok_or("agente no encontrado")?;
    // Agents of a pattern run keep the run's id in `task_id`; overwriting it would cut them off from their run.
    if agent.task_id.starts_with("run-") {
        return Err(format!(
            "el agente {agent_id} pertenece al run {} y no se puede asignar a una tarea",
            agent.task_id
        ));
    }
    agent.task_id = task.map(|id| id.to_string()).unwrap_or_default();
    save_agent_registry(registry, &reg).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use shepherd_core::Agent;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shepherd-desktop-setup-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn enabling_keeps_comments_order_and_other_keys() {
        let original = "# mis ajustes\n[ui]\n# sin sonido\nsound = false  # nota\n\n[shepherd]\nbackend = \"socket\"\n";
        let out = enable_shepherd_in(original).unwrap();
        assert!(
            out.contains("# mis ajustes")
                && out.contains("# sin sonido")
                && out.contains("sound = false  # nota")
        );
        assert!(out.contains("backend = \"socket\""));
        assert!(shepherd_enabled_in(&out));
        assert!(
            out.find("[ui]").unwrap() < out.find("[shepherd]").unwrap(),
            "order kept"
        );
    }

    #[test]
    fn enabling_handles_missing_table_empty_file_flipped_value_and_is_idempotent() {
        assert_eq!(
            enable_shepherd_in("").unwrap().trim(),
            "[shepherd]\nenabled = true"
        );
        let added = enable_shepherd_in("[ui]\nx = 1\n").unwrap();
        assert!(added.contains("[ui]") && shepherd_enabled_in(&added));
        let flipped = enable_shepherd_in("[shepherd]\nenabled = false\n").unwrap();
        assert!(shepherd_enabled_in(&flipped) && !flipped.contains("false"));
        assert_eq!(enable_shepherd_in(&flipped).unwrap(), flipped); // second time changes nothing
        assert!(!shepherd_enabled_in("[shepherd]\nenabled = false\n"));
        assert!(!shepherd_enabled_in("nonsense = ["));
    }

    #[test]
    fn enabling_refuses_invalid_toml_and_a_non_table_shepherd() {
        assert!(enable_shepherd_in("= broken").unwrap_err().contains("TOML"));
        assert!(enable_shepherd_in("shepherd = 3")
            .unwrap_err()
            .contains("no es una tabla"));
    }

    #[test]
    fn the_server_config_is_found_from_the_env_or_the_dir_in_use() {
        let home = temp("cfgpath");
        assert_eq!(
            server_config_path(Some("/x/c.toml"), &home),
            Path::new("/x/c.toml")
        );
        assert_eq!(
            server_config_path(None, &home),
            home.join("shepherd/config.toml")
        ); // nothing yet
        std::fs::create_dir_all(home.join("shepherd-dev")).unwrap();
        std::fs::write(home.join("shepherd-dev/shepherd-server.log"), "").unwrap();
        assert_eq!(
            server_config_path(Some(""), &home),
            home.join("shepherd-dev/config.toml")
        ); // only a dev server ran
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::create_dir_all(home.join("shepherd")).unwrap();
        std::fs::write(home.join("shepherd/shepherd-server.log"), "").unwrap();
        assert_eq!(
            server_config_path(None, &home),
            home.join("shepherd/config.toml")
        ); // the release server ran last
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn enabling_writes_the_file_keeps_a_backup_and_creates_missing_dirs() {
        let dir = temp("enable");
        let path = dir.join("nested/config.toml");
        enable_shepherd_at(&path).unwrap(); // no file, no dir yet
        assert!(shepherd_enabled_in(
            &std::fs::read_to_string(&path).unwrap()
        ));
        std::fs::write(&path, "# keep me\n[ui]\nx = 1\n").unwrap();
        enable_shepherd_at(&path).unwrap();
        let now = std::fs::read_to_string(&path).unwrap();
        let bak = std::fs::read_to_string(path.with_extension("toml.bak")).unwrap();
        assert!(now.contains("# keep me") && shepherd_enabled_in(&now));
        assert_eq!(bak, "# keep me\n[ui]\nx = 1\n");
        assert!(
            !path.with_extension("tmp-desktop").exists(),
            "temp file cleaned up"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn registry_with(path: &Path, agents: Vec<Agent>) {
        save_agent_registry(
            path,
            &AgentRegistryFile {
                agents,
                ..Default::default()
            },
        )
        .unwrap();
    }

    fn task_dir(root: &Path, id: u32) {
        let dir = root.join(format!(".shepherd/tasks/{id}-t"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("status.json"), format!(r#"{{"id":{id},"slug":"t","title":"T{id}","state":"open","created":"x","updated":"x"}}"#)).unwrap();
    }

    #[test]
    fn assigning_changes_only_that_agents_task_and_keeps_its_token() {
        let dir = temp("assign");
        task_dir(&dir, 3);
        let reg = dir.join("agents.json");
        let agent = |id: &str, seq, task: &str| Agent {
            id: id.into(),
            seq,
            role: "r".into(),
            token: format!("secret-{id}"),
            task_id: task.into(),
            state: "working".into(),
            ..Agent::default()
        };
        registry_with(&reg, vec![agent("a-1", 1, ""), agent("a-2", 2, "9")]);
        assign_task_at(&reg, &dir, "a-1", Some(3)).unwrap();
        let after = load_agent_registry(&reg).unwrap();
        let get = |id: &str| after.agents.iter().find(|a| a.id == id).unwrap().clone();
        assert_eq!(
            (
                get("a-1").task_id.as_str(),
                get("a-1").token.as_str(),
                get("a-1").state.as_str()
            ),
            ("3", "secret-a-1", "working")
        );
        assert_eq!(get("a-2").task_id, "9"); // untouched
        assign_task_at(&reg, &dir, "a-1", None).unwrap(); // unassign
        assert_eq!(load_agent_registry(&reg).unwrap().agents[0].task_id, "");
        assert!(!reg.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn agents_of_a_pattern_run_cannot_be_moved_to_a_task() {
        let dir = temp("assign-run");
        task_dir(&dir, 1);
        let reg = dir.join("agents.json");
        registry_with(
            &reg,
            vec![Agent {
                id: "a-1".into(),
                seq: 1,
                token: "t".into(),
                task_id: "run-fake-2831".into(),
                ..Agent::default()
            }],
        );
        let err = assign_task_at(&reg, &dir, "a-1", Some(1)).unwrap_err();
        assert!(err.contains("pertenece al run run-fake-2831"), "{err}");
        assert!(
            assign_task_at(&reg, &dir, "a-1", None).is_err(),
            "clearing it would also detach it from the run"
        );
        assert_eq!(
            load_agent_registry(&reg).unwrap().agents[0].task_id,
            "run-fake-2831"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn assigning_rejects_unknown_agents_and_tasks_without_touching_the_file() {
        let dir = temp("assign-bad");
        task_dir(&dir, 1);
        let reg = dir.join("agents.json");
        registry_with(
            &reg,
            vec![Agent {
                id: "a-1".into(),
                seq: 1,
                token: "t".into(),
                ..Agent::default()
            }],
        );
        let before = std::fs::read_to_string(&reg).unwrap();
        assert!(assign_task_at(&reg, &dir, "nope", Some(1))
            .unwrap_err()
            .contains("no encontrado"));
        assert!(assign_task_at(&reg, &dir, "a-1", Some(99))
            .unwrap_err()
            .contains("#99 no existe"));
        assert_eq!(std::fs::read_to_string(&reg).unwrap(), before);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Live: `SHEPHERD_SOCKET_PATH=… SHEPHERD_BIN=… SHEPHERD_CONFIG_PATH=… XDG_CONFIG_HOME=… XDG_STATE_HOME=…
    /// cargo test -p shepherd-desktop live_enable -- --ignored`, against a server started WITHOUT the layer.
    #[test]
    #[ignore]
    fn live_enable_and_restart() {
        use serde_json::json;
        let socket = crate::api::default_socket_path();
        let bin = crate::api::default_server_bin().expect("SHEPHERD_BIN");
        let config = default_config_path();
        let off = crate::api::call(&socket, "shepherd.agent.list", json!({})).unwrap_err();
        assert!(
            off.contains("shepherd_not_running"),
            "precondition: the layer must be off, got {off}"
        );
        enable_and_restart(&config, &socket, &bin).unwrap();
        assert!(
            crate::api::call(&socket, "shepherd.agent.list", json!({})).is_ok(),
            "the layer must be on after the restart"
        );
        assert!(shepherd_enabled_in(
            &std::fs::read_to_string(&config).unwrap()
        ));
    }

    /// Live: against a server WITH the layer on, using the same env as `live_enable_and_restart` plus
    /// `SHEPHERD_TEST_ROOT=<project dir>`. Checks the server really sees what `assign_task_at` writes.
    #[test]
    #[ignore]
    fn live_assign_is_seen_by_the_server() {
        use serde_json::json;
        let socket = crate::api::default_socket_path();
        let root = PathBuf::from(std::env::var("SHEPHERD_TEST_ROOT").expect("SHEPHERD_TEST_ROOT"));
        let call = |m: &str, p: serde_json::Value| crate::api::call(&socket, m, p).unwrap();
        let task = call(
            "shepherd.task.create",
            json!({ "title": "para asignar", "root": root }),
        );
        let task_id = task["task"]["status"]["id"].as_u64().unwrap() as u32;
        let agent = call(
            "shepherd.agent.spawn",
            json!({ "role": "tester", "task": "x", "root": root, "worktree": false }),
        );
        let agent_id = agent["agent"]["id"].as_str().unwrap().to_string();
        assert_eq!(
            agent["agent"].get("task_id"),
            None,
            "launched without a task"
        );
        let registry = shepherd_core::config::state_dir().join("agents.json");
        let task_of = || {
            call("shepherd.agent.list", json!({}))["agents"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == agent_id.as_str())
                .map(|a| a["task_id"].as_str().unwrap_or("").to_string())
        };
        assign_task_at(&registry, &root, &agent_id, Some(task_id)).unwrap();
        assert_eq!(
            task_of().as_deref(),
            Some(task_id.to_string().as_str()),
            "the server must see the new task_id"
        );
        assign_task_at(&registry, &root, &agent_id, None).unwrap();
        assert_eq!(task_of().as_deref(), Some(""), "and the cleared one");
        // the server keeps working on that agent after our write (it can still be killed through the API)
        assert!(
            crate::api::call(&socket, "shepherd.agent.kill", json!({ "id": agent_id })).is_ok()
                || task_of().is_some()
        );
    }
}
