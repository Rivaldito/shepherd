//! Workflow execution and human approval gates (`approve`, `reject`).

use super::OrchestrationService;
use crate::find_task;
use serde_json::Value;
use std::path::PathBuf;

fn agent_task(found: &Option<crate::CreatedTask>) -> String {
    found
        .as_ref()
        .map(|t| t.status.id.to_string())
        .unwrap_or_default()
}

impl OrchestrationService {
    /// Links an agent to a task (or detaches it with `task: None`): sets the agent's task in the registry and
    /// keeps the task's list of agents in step.
    pub fn assign_agent_task(
        &self,
        root: &std::path::Path,
        agent_id: &str,
        task: Option<&str>,
    ) -> Result<Value, String> {
        let found = match task {
            Some(t) => Some(find_task(root, t).map_err(|e| e.to_string())?),
            None => None,
        };
        let mut registry = self.load_registry()?;
        let agent = registry
            .agents
            .iter_mut()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| format!("unknown agent {agent_id:?}"))?;
        let previous = std::mem::take(&mut agent.task_id);
        match &found {
            Some(t) => {
                agent.task_id = t.status.id.to_string();
                agent.task_text = t.status.title.clone();
            }
            None => agent.task_text.clear(),
        }
        agent.updated = self.clock.now_rfc3339();
        crate::save_agent_registry(self.service_config.registry_path(), &registry)
            .map_err(|e| e.to_string())?;
        // the task lists of the old and the new task
        for (reference, add) in [(previous.as_str(), false), (&agent_task(&found), true)] {
            if reference.is_empty() {
                continue;
            }
            if let Ok(mut t) = find_task(root, reference) {
                t.status.agents.retain(|a| a != agent_id);
                if add {
                    t.status.agents.push(agent_id.to_owned());
                }
                let _ = crate::save_task_status(t.directory.join("status.json"), &t.status);
            }
        }
        Ok(serde_json::json!({ "agent": agent_id, "task": found.map(|t| t.status.id) }))
    }

    /// Changes the skills one agent gets: `Some(true)` turns `skill` on, `Some(false)` turns it off, `None` puts it
    /// back to what its role gives. It is recorded in the agent's registry entry and applies the next time the
    /// agent is started; the copy installed for a running agent is not touched.
    pub fn adjust_agent_skill(
        &self,
        root: &std::path::Path,
        agent_id: &str,
        skill: &str,
        enable: Option<bool>,
    ) -> Result<Value, String> {
        if enable == Some(true) && !crate::skills_admin::exists(root, skill) {
            return Err(format!("unknown skill `{skill}`"));
        }
        let mut registry = self.load_registry()?;
        let agent = registry
            .agents
            .iter_mut()
            .find(|a| a.id == agent_id)
            .ok_or_else(|| format!("unknown agent {agent_id:?}"))?;
        agent.skills.add.retain(|s| s != skill);
        agent.skills.remove.retain(|s| s != skill);
        match enable {
            Some(true) => agent.skills.add.push(skill.to_owned()),
            Some(false) => agent.skills.remove.push(skill.to_owned()),
            None => {}
        }
        agent.updated = self.clock.now_rfc3339();
        let adjust = agent.skills.clone();
        crate::save_agent_registry(self.service_config.registry_path(), &registry)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "agent": agent_id,
            "skills": adjust,
            "applies": "next start",
        }))
    }

    /// Stops a task's run: closes the agents linked to it that are still alive and marks the task `cancelled`.
    /// Work already committed stays on its branches. A task that is finished or already cancelled is refused.
    pub fn cancel_run(&self, root: PathBuf, task: &str) -> Result<Value, String> {
        let found = find_task(&root, task).map_err(|e| e.to_string())?;
        let mut status = found.status.clone();
        if matches!(status.state.as_str(), "done" | "cancelled") {
            return Err(format!("task {} is already {}", status.id, status.state));
        }
        let live: Vec<String> = self
            .load_registry()?
            .live()
            .iter()
            .map(|a| a.id.clone())
            .filter(|id| status.agents.contains(id))
            .collect();
        let mut killed = Vec::new();
        for id in live {
            if self.kill(&id).is_ok() {
                killed.push(id);
            }
        }
        status.state = "cancelled".into();
        status.waiting.clear();
        status.updated = crate::tasks::now_rfc3339();
        crate::save_task_status(found.directory.join("status.json"), &status)
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({ "task": status.id, "state": status.state, "killed": killed }))
    }

    pub fn run_workflow(
        &self,
        root: PathBuf,
        task: String,
        mcp_url: Option<String>,
        auto_push: bool,
    ) -> Result<Value, String> {
        let result = (|| {
            let effective_mcp_url = mcp_url
                .filter(|u| !u.is_empty())
                .or_else(|| self.mcp_url())
                .ok_or_else(|| {
                    "mcp_url is required when in-process MCP is not running".to_owned()
                })?;
            let report = crate::workflow::run_coders_with_terminals(
                crate::workflow::RunRequest {
                    root: root.clone(),
                    task: task.clone(),
                    mcp_url: effective_mcp_url,
                },
                self.terminals.as_ref(),
                Some(self.service_config.registry_path()),
                self.active_vendors_override.clone(),
            )
            .map_err(|e| e.to_string())?;

            let mut pushed_prs = Vec::new();
            if auto_push && !report.plan_waiting {
                let loaded_task = find_task(&root, &task).map_err(|e| e.to_string())?;
                for subtask in &loaded_task.status.subtasks {
                    if subtask.state == "awaiting-approval" {
                        let pr_url = crate::workflow::approve_subtask_with_registry(
                            root.clone(),
                            &task,
                            subtask.number,
                            &self.service_config.registry_path(),
                        )
                        .map_err(|e| e.to_string())?;
                        pushed_prs.push(serde_json::json!({
                            "subtask": subtask.number,
                            "pr": pr_url,
                        }));
                    }
                }
            }

            Ok(serde_json::json!({
                "started": report.started,
                "completed_coding": report.completed_coding,
                "waiting_for_human": report.waiting_for_human,
                "failed": report.failed,
                "plan_waiting": report.plan_waiting,
                "auto_push": auto_push,
                "pushed_prs": pushed_prs,
            }))
        })();
        self.log_err("run", result)
    }

    /// Requirement 9: Explicit human approval for `plan` or a subtask number.
    pub fn approve(&self, root: PathBuf, task: &str, target: &str) -> Result<Value, String> {
        let result = (|| {
            let clean_target = target.strip_prefix("subtask:").unwrap_or(target);
            if clean_target == "plan" {
                crate::workflow::approve_plan(root, task).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "ok": true,
                    "task": task,
                    "approved": "plan",
                }))
            } else {
                let number: u32 = clean_target.parse().map_err(|_| {
                    format!("invalid approval target {target:?}; expected 'plan' or subtask number")
                })?;
                let pr_url = crate::workflow::approve_subtask_with_registry(
                    root,
                    task,
                    number,
                    &self.service_config.registry_path(),
                )
                .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "ok": true,
                    "task": task,
                    "approved": number,
                    "url": pr_url,
                    "pr": pr_url,
                }))
            }
        })();
        self.log_err("approve", result)
    }

    /// Explicit human rejection for `plan` or a subtask number.
    pub fn reject(
        &self,
        root: PathBuf,
        task: &str,
        target: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        let result = (|| {
            let clean_target = target.strip_prefix("subtask:").unwrap_or(target);
            if clean_target == "plan" {
                crate::workflow::reject_plan(root, task, reason).map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "ok": true,
                    "task": task,
                    "rejected": "plan",
                    "reason": reason,
                }))
            } else {
                let number: u32 = clean_target.parse().map_err(|_| {
                    format!(
                        "invalid rejection target {target:?}; expected 'plan' or subtask number"
                    )
                })?;
                crate::workflow::reject_subtask(root, task, number, reason)
                    .map_err(|e| e.to_string())?;
                Ok(serde_json::json!({
                    "ok": true,
                    "task": task,
                    "rejected": number,
                    "reason": reason,
                }))
            }
        })();
        self.log_err("reject", result)
    }
}
