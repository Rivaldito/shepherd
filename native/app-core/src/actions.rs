//! Everything the panels ask the server to do. The server is the only writer of agents, runs and the bus;
//! the app never edits their files. Calls that carry a workspace go through `shepherd.call`, because the
//! typed `shepherd.pattern.run.start` and `shepherd.agent.spawn` have a frozen shape without one.

use crate::api::call;
use serde_json::{json, Value};
use shepherd_core::snapshot::{CaptureResult, RestoreResult, ShowResult};
use std::path::Path;

pub struct Launched {
    pub run: Value,
    /// The pattern needs more agents than the concurrency limit: the server's confirmation was given.
    pub over_limit: bool,
}

/// Starts a pattern run in `workspace_id`. Going over `max_concurrent_agents` is a deliberate request
/// here, so it is confirmed once and reported instead of failing.
pub fn start_pattern_run(
    api: &Path,
    pattern: &str,
    objective: Option<&str>,
    root: Option<&str>,
    workspace_id: Option<&str>,
) -> Result<Launched, String> {
    let mut args = json!({ "pattern": pattern, "task": objective, "root": root, "workspace_id": workspace_id });
    let first = call(
        api,
        "shepherd.call",
        json!({ "op": "pattern_run_start", "args": args.clone() }),
    );
    match first {
        Ok(out) => Ok(Launched {
            run: out["data"].clone(),
            over_limit: false,
        }),
        Err(e) if e.contains("confirmation_required: max_concurrent_agents") => {
            args["confirm_over_limit"] = json!(true);
            let out = call(
                api,
                "shepherd.call",
                json!({ "op": "pattern_run_start", "args": args }),
            )?;
            Ok(Launched {
                run: out["data"].clone(),
                over_limit: true,
            })
        }
        Err(e) => Err(e),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunControl {
    Pause,
    Resume,
    Stop,
}

pub fn control_run(
    api: &Path,
    id: &str,
    control: RunControl,
    root: Option<&str>,
) -> Result<(), String> {
    let (method, params) = match control {
        RunControl::Pause => (
            "shepherd.pattern.run.pause",
            json!({ "id": id, "root": root }),
        ),
        RunControl::Resume => (
            "shepherd.pattern.run.resume",
            json!({ "id": id, "root": root }),
        ),
        RunControl::Stop => (
            "shepherd.pattern.run.stop",
            json!({ "id": id, "root": root, "confirm": true }),
        ),
    };
    call(api, method, params).map(|_| ())
}

/// Forgets one run, whatever its state. A live run is stopped first (which kills its agents), and the server
/// only does that with `confirm`. The agents the run leaves behind are cleaned too; returns how many.
pub fn delete_run(
    api: &Path,
    id: &str,
    root: Option<&str>,
    confirm: bool,
) -> Result<usize, String> {
    let reply = call(
        api,
        "shepherd.call",
        json!({ "op": "run_delete", "args": { "id": id, "root": root, "confirm": confirm } }),
    )?;
    let ids = run_agent_ids(&reply["data"]);
    if ids.is_empty() {
        return Ok(0);
    }
    // the run is already gone; a failure here only leaves old agents in the list
    Ok(call(api, "shepherd.agent.clean", json!({ "ids": ids }))
        .map(|r| r["cleaned"].as_array().map_or(0, Vec::len))
        .unwrap_or(0))
}

/// The ids of the agents of a run, as the server describes it.
fn run_agent_ids(run: &Value) -> Vec<String> {
    run["agents"]
        .as_array()
        .map(|agents| {
            agents
                .iter()
                .filter_map(|a| a["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Forgets finished runs and cleans the agents that are already gone; returns (runs, agents).
pub fn clean_runs(api: &Path, root: Option<&str>) -> Result<(u64, usize), String> {
    let runs = call(
        api,
        "shepherd.call",
        json!({ "op": "runs_clean", "args": { "root": root } }),
    )?["data"]["removed"]
        .as_u64()
        .unwrap_or(0);
    let agents = call(api, "shepherd.agent.clean", json!({ "ids": [] }))?["cleaned"]
        .as_array()
        .map_or(0, Vec::len);
    Ok((runs, agents))
}

/// Empties the message bus; with live agents the server asks for `force`.
pub fn clean_bus(api: &Path, force: bool) -> Result<u64, String> {
    Ok(call(
        api,
        "shepherd.call",
        json!({ "op": "bus_clean", "args": { "force": force } }),
    )?["data"]["removed"]
        .as_u64()
        .unwrap_or(0))
}

/// Creates a task; returns its number.
pub fn create_task(
    api: &Path,
    root: &str,
    title: &str,
    description: &str,
    criteria: &[String],
) -> Result<u32, String> {
    let reply = call(
        api,
        "shepherd.call",
        json!({ "op": "task_create", "args": {
            "root": root, "title": title, "description": description, "criteria": criteria } }),
    )?;
    Ok(reply["data"]["status"]["id"].as_u64().unwrap_or(0) as u32)
}

/// Changes a task in place (`spec` is left alone when `None`).
pub fn update_task(
    api: &Path,
    root: &str,
    id: u32,
    title: &str,
    description: &str,
    spec: Option<&str>,
    criteria: &[String],
) -> Result<(), String> {
    let mut args = json!({
        "root": root, "id": id.to_string(), "title": title,
        "description": description, "criteria": criteria });
    if let Some(spec) = spec {
        args["spec"] = json!(spec);
    }
    call(
        api,
        "shepherd.call",
        json!({ "op": "task_update", "args": args }),
    )
    .map(|_| ())
}

/// Deletes a task for good (the server refuses while live agents work on it).
pub fn delete_task(api: &Path, id: u32, root: &str) -> Result<(), String> {
    call(
        api,
        "shepherd.call",
        json!({ "op": "task_delete", "args": { "root": root, "id": id.to_string(), "confirm": true } }),
    )
    .map(|_| ())
}

pub fn archive_task(api: &Path, id: u32, root: Option<&str>) -> Result<(), String> {
    call(
        api,
        "shepherd.task.archive",
        json!({ "id": id.to_string(), "root": root }),
    )
    .map(|_| ())
}

/// Spawns a single agent with a Shepherd role. Returns the agent data from the server.
pub fn spawn_agent(
    api: &Path,
    role: &str,
    task: &str,
    root: Option<&str>,
    workspace_id: Option<&str>,
    vendor: Option<&str>,
    model: Option<&str>,
    worktree: bool,
    task_id: Option<u32>,
) -> Result<Value, String> {
    let mut args = json!({
        "role": role,
        "task": task,
        "worktree": worktree,
        "root": root,
        "workspace_id": workspace_id,
    });
    if let Some(id) = task_id {
        // links the agent to the task: its result closes a direct task
        args["task_id"] = json!(id.to_string());
    }
    if let Some(v) = vendor {
        args["vendor"] = json!(v);
    }
    if let Some(m) = model {
        args["model"] = json!(m);
    }
    call(api, "shepherd.call", json!({ "op": "spawn", "args": args }))
        .map(|out| out["data"].clone())
}

/// Approves a gate of a task run: the plan, or publishing a subtask's branch and opening its PR.
pub fn approve_gate(api: &Path, task_id: u32, target: &str, root: &str) -> Result<(), String> {
    call(
        api,
        "shepherd.run.approve",
        json!({ "task": task_id.to_string(), "target": target, "root": root }),
    )
    .map(|_| ())
}

/// Rejects a gate of a task run, with the reason the planner or coder will read.
pub fn reject_gate(
    api: &Path,
    task_id: u32,
    target: &str,
    reason: Option<&str>,
    root: &str,
) -> Result<(), String> {
    let mut params = json!({ "task": task_id.to_string(), "target": target, "root": root });
    if let Some(reason) = reason {
        params["reason"] = json!(reason);
    }
    call(api, "shepherd.run.reject", params).map(|_| ())
}

/// Stops a task's run: the server closes the agents linked to it and marks the task cancelled. Returns how many
/// agents it closed. Commits already made stay on their branches.
pub fn cancel_run(api: &Path, task_id: u32, root: &str) -> Result<usize, String> {
    let reply = call(
        api,
        "shepherd.call",
        json!({ "op": "cancel_run", "args": { "task": task_id.to_string(), "root": root } }),
    )?;
    Ok(reply["data"]["killed"].as_array().map_or(0, Vec::len))
}

/// Creates a task from a GitHub issue (a number, `owner/repo#n` or a URL; needs `gh`). Returns the task's number.
pub fn import_issue(api: &Path, issue_ref: &str, root: &str) -> Result<u32, String> {
    let reply = call(
        api,
        "shepherd.call",
        json!({ "op": "import_task", "args": { "issue_ref": issue_ref.trim(), "root": root } }),
    )?;
    Ok(reply["data"]["status"]["id"].as_u64().unwrap_or(0) as u32)
}

fn typed<T: serde::de::DeserializeOwned>(reply: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(reply[key].clone()).map_err(|e| format!("unexpected answer: {e}"))
}

/// Takes a snapshot of an agent's worktree now.
pub fn snapshot_capture(api: &Path, agent: &str, root: &str) -> Result<CaptureResult, String> {
    let reply = call(
        api,
        "shepherd.snapshot.capture",
        json!({ "agent": agent, "reason": "manual", "root": root }),
    )?;
    typed(&reply, "result")
}

/// What a snapshot holds: its metadata and the files it changed over the agent's previous one.
pub fn snapshot_show(api: &Path, id: &str, root: &str) -> Result<ShowResult, String> {
    let reply = call(
        api,
        "shepherd.snapshot.show",
        json!({ "id": id, "root": root, "full": false }),
    )?;
    typed(&reply, "result")
}

/// Puts the whole worktree back as the snapshot had it. The server keeps a `pre-restore` snapshot of
/// what was there, and leaves files the snapshot does not know (it never deletes them from here).
pub fn snapshot_restore(api: &Path, id: &str, root: &str) -> Result<RestoreResult, String> {
    let reply = call(
        api,
        "shepherd.snapshot.restore",
        json!({ "id": id, "root": root, "confirm": true, "delete_extra": false }),
    )?;
    typed(&reply, "result")
}

pub fn snapshot_delete(api: &Path, id: &str, root: &str) -> Result<(), String> {
    call(
        api,
        "shepherd.snapshot.delete",
        json!({ "id": id, "root": root }),
    )
    .map(|_| ())
}

/// Kills an agent (if not already in a terminal state) and cleans it from the registry.
pub fn kill_and_clean_agent(api: &Path, id: &str) -> Result<(), String> {
    // Kill if still alive
    let _ = call(api, "shepherd.agent.kill", json!({ "id": id }));
    // Clean from registry
    call(api, "shepherd.agent.clean", json!({ "ids": [id] })).map(|_| ())
}

/// Resumes all idle agents.
pub fn resume_agents(api: &Path) -> Result<(), String> {
    call(api, "shepherd.agent.resume", json!({})).map(|_| ())
}

/// Assigns an agent to a project task (or detaches it). Writes the agent registry file.
pub fn assign_agent_task(
    api: &Path,
    agent_id: &str,
    task_id: Option<u32>,
    root: &str,
) -> Result<(), String> {
    // The server has no dedicated method; we edit the registry via shepherd.call
    let params = json!({
        "op": "assign_agent_task",
        "args": {
            "agentId": agent_id,
            "task": task_id.map(|id| id.to_string()),
            "root": root,
        },
    });
    call(api, "shepherd.call", params).map(|_| ())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_agents_of_a_deleted_run_are_found_in_what_the_server_returns() {
        let run = json!({"id":"run-duo-1","agents":[{"id":"specialist-1"},{"id":"specialist-2","role":"x"},{"role":"no id"}]});
        assert_eq!(run_agent_ids(&run), ["specialist-1", "specialist-2"]);
        assert!(run_agent_ids(&Value::Null).is_empty());
    }

    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    /// A server that answers each connection with the next canned reply and records the requests.
    fn fake_server(
        replies: Vec<&'static str>,
    ) -> (std::path::PathBuf, std::thread::JoinHandle<Vec<Value>>) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("native-actions-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                seen.push(serde_json::from_str(&line).unwrap());
                writeln!(stream, "{reply}").unwrap();
            }
            seen
        });
        (path, handle)
    }

    #[test]
    fn a_gate_is_approved_or_rejected_with_the_task_and_target_the_server_expects() {
        let (path, server) = fake_server(vec![
            r#"{"id":"1","result":{"type":"shepherd_run_approve"}}"#,
            r#"{"id":"2","result":{"type":"shepherd_run_reject"}}"#,
            r#"{"id":"3","error":{"code":"shepherd_error","message":"nothing is waiting"}}"#,
        ]);
        approve_gate(&path, 7, "plan", "/p").unwrap();
        reject_gate(&path, 7, "2", Some("too broad"), "/p").unwrap();
        assert_eq!(
            reject_gate(&path, 7, "plan", None, "/p").unwrap_err(),
            "nothing is waiting"
        );
        let seen = server.join().unwrap();
        assert_eq!(seen[0]["method"], "shepherd.run.approve");
        assert_eq!(seen[0]["params"]["task"], "7");
        assert_eq!(seen[0]["params"]["target"], "plan");
        assert_eq!(seen[0]["params"]["root"], "/p");
        assert_eq!(seen[1]["method"], "shepherd.run.reject");
        assert_eq!(seen[1]["params"]["target"], "2");
        assert_eq!(seen[1]["params"]["reason"], "too broad");
        assert!(seen[2]["params"].get("reason").is_none());
    }

    const INFO: &str = r#"{"id":"coder-1/2","agent":"coder-1","n":2,"commit":"abc","reason":"manual","created":100,"worktree":"/w","prev":"coder-1/1","files":1,"added":3,"removed":0}"#;

    #[test]
    fn snapshots_are_asked_for_by_id_and_restored_only_with_the_confirmation() {
        let replies: Vec<&'static str> = vec![
            Box::leak(format!(r#"{{"id":"1","result":{{"type":"x","result":{{"created":true,"snapshot":{INFO}}}}}}}"#).into_boxed_str()),
            Box::leak(format!(r#"{{"id":"2","result":{{"type":"x","result":{{"snapshot":{INFO},"diff":{{"from":"coder-1/1","to":"coder-1/2","files":[{{"path":"a.rs","status":"added","added":3,"removed":0,"binary":false}}],"added":3,"removed":0}}}}}}}}"#).into_boxed_str()),
            Box::leak(format!(r#"{{"id":"3","result":{{"type":"x","result":{{"restored":{INFO},"pre_restore":{INFO},"pre_restore_created":true,"restored_files":["a.rs"],"extra_files":[],"deleted_files":[]}}}}}}"#).into_boxed_str()),
            r#"{"id":"4","result":{"type":"x","result":{"deleted":"coder-1/2"}}}"#,
            r#"{"id":"5","error":{"code":"shepherd_error","message":"unknown snapshot"}}"#,
        ];
        let (path, server) = fake_server(replies);
        let captured = snapshot_capture(&path, "coder-1", "/p").unwrap();
        assert!(captured.created);
        assert_eq!(captured.snapshot.id, "coder-1/2");
        let shown = snapshot_show(&path, "coder-1/2", "/p").unwrap();
        assert_eq!(shown.diff.files[0].path, "a.rs");
        let restored = snapshot_restore(&path, "coder-1/2", "/p").unwrap();
        assert_eq!(restored.restored_files, ["a.rs"]);
        snapshot_delete(&path, "coder-1/2", "/p").unwrap();
        assert_eq!(
            snapshot_show(&path, "nope/1", "/p").unwrap_err(),
            "unknown snapshot"
        );
        let seen = server.join().unwrap();
        assert_eq!(seen[0]["method"], "shepherd.snapshot.capture");
        assert_eq!(seen[0]["params"]["reason"], "manual");
        assert_eq!(seen[2]["method"], "shepherd.snapshot.restore");
        assert_eq!(seen[2]["params"]["confirm"], true);
        assert_eq!(seen[2]["params"]["delete_extra"], false);
        assert_eq!(seen[3]["method"], "shepherd.snapshot.delete");
        assert_eq!(seen[3]["params"]["id"], "coder-1/2");
    }

    #[test]
    fn a_run_is_cancelled_and_an_issue_imported_with_what_the_server_expects() {
        let (path, server) = fake_server(vec![
            r#"{"id":"1","result":{"type":"shepherd_call","data":{"task":3,"state":"cancelled","killed":["coder-1","coder-2"]}}}"#,
            r#"{"id":"2","error":{"code":"shepherd_error","message":"task 3 is already cancelled"}}"#,
            r#"{"id":"3","result":{"type":"shepherd_call","data":{"status":{"id":8,"title":"Fix login"}}}}"#,
            r#"{"id":"4","error":{"code":"shepherd_error","message":"issue #7 is already task 8"}}"#,
        ]);
        assert_eq!(cancel_run(&path, 3, "/p").unwrap(), 2);
        assert_eq!(
            cancel_run(&path, 3, "/p").unwrap_err(),
            "task 3 is already cancelled"
        );
        assert_eq!(import_issue(&path, "  #7 ", "/p").unwrap(), 8);
        assert!(import_issue(&path, "7", "/p")
            .unwrap_err()
            .contains("already task"));
        let seen = server.join().unwrap();
        assert_eq!(seen[0]["params"]["op"], "cancel_run");
        assert_eq!(seen[0]["params"]["args"]["task"], "3");
        assert_eq!(seen[0]["params"]["args"]["root"], "/p");
        assert_eq!(seen[2]["params"]["op"], "import_task");
        assert_eq!(seen[2]["params"]["args"]["issue_ref"], "#7", "trimmed");
    }

    #[test]
    fn a_pattern_over_the_agent_limit_is_confirmed_once_and_reported() {
        let (path, server) = fake_server(vec![
            r#"{"id":"1","error":{"code":"shepherd_error","message":"confirmation_required: max_concurrent_agents (4) reached (0 active + 7 new)"}}"#,
            r#"{"id":"2","result":{"type":"shepherd_call","data":{"id":"run-big-1","agents":[]}}}"#,
        ]);
        let launched =
            start_pattern_run(&path, "big", Some("Task #1: x"), Some("/p"), Some("w2")).unwrap();
        assert!(launched.over_limit);
        assert_eq!(launched.run["id"], "run-big-1");
        let seen = server.join().unwrap();
        assert_eq!(seen[0]["params"]["args"]["workspace_id"], "w2");
        assert!(seen[0]["params"]["args"]
            .get("confirm_over_limit")
            .is_none());
        assert_eq!(seen[1]["params"]["args"]["confirm_over_limit"], true);
        assert_eq!(seen[1]["params"]["args"]["pattern"], "big");
    }

    #[test]
    fn other_errors_are_not_retried() {
        let (path, server) = fake_server(vec![
            r#"{"id":"1","error":{"code":"x","message":"pattern_not_found"}}"#,
        ]);
        assert_eq!(
            start_pattern_run(&path, "nope", None, None, None)
                .err()
                .as_deref(),
            Some("pattern_not_found")
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }

    #[test]
    fn stopping_a_run_carries_the_confirmation() {
        let (path, server) = fake_server(vec![r#"{"id":"1","result":{"type":"ok"}}"#]);
        control_run(&path, "run-1", RunControl::Stop, Some("/p")).unwrap();
        let seen = server.join().unwrap();
        assert_eq!(seen[0]["method"], "shepherd.pattern.run.stop");
        assert_eq!(seen[0]["params"]["confirm"], true);
    }
}
