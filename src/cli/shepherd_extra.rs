//! CLI commands for the server operations the terminal UI and the desktop app already use. Each one is a thin
//! mapping from words to a `shepherd.call` operation; the rules (confirmations, refusals) live in the server.

use crate::api::schema::shepherd::{ShepherdCallParams, ShepherdEmptyParams};
use crate::api::schema::{Method, Request};
use crate::cli::projection::ParseError;
use crate::cli::shepherd_bindings::{ShepherdOperation, USAGE_STRING};
use serde_json::{json, Value};

struct Words<'a> {
    positional: Vec<&'a str>,
    flags: Vec<&'a str>,
    values: Vec<(&'a str, &'a str)>,
}

/// Splits arguments into positionals, `--flag`s and `--key value` pairs (for the keys in `valued`).
fn words<'a>(args: &'a [String], valued: &[&str]) -> Words<'a> {
    let mut w = Words {
        positional: vec![],
        flags: vec![],
        values: vec![],
    };
    let mut it = args.iter().map(String::as_str);
    while let Some(a) = it.next() {
        match a {
            "--json" => {}
            _ if valued.contains(&a) => {
                if let Some(v) = it.next() {
                    w.values.push((a, v));
                }
            }
            _ if a.starts_with("--") => w.flags.push(a),
            _ => w.positional.push(a),
        }
    }
    w
}

impl Words<'_> {
    fn flag(&self, name: &str) -> bool {
        self.flags.contains(&name)
    }
    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| *v)
    }
    fn one(&self) -> Result<&str, ParseError> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            _ => Err(usage()),
        }
    }
}

fn usage() -> ParseError {
    ParseError::Usage(USAGE_STRING.to_string())
}

fn call(id: &str, op: &str, mut args: Value) -> Result<Option<ShepherdOperation>, ParseError> {
    let cwd = std::env::current_dir().unwrap_or_default();
    args["root"] = json!(shepherd_core::cli::project_root(cwd).display().to_string());
    Ok(Some(ShepherdOperation::Api(Request {
        id: format!("cli:shepherd:{id}"),
        method: Method::ShepherdCall(ShepherdCallParams {
            op: op.to_owned(),
            args,
        }),
    })))
}

/// `Ok(None)` when `args` is not one of the commands of this file.
pub(crate) fn parse_extra(args: &[String]) -> Result<Option<ShepherdOperation>, ParseError> {
    let sub = args.get(1).map(String::as_str);
    let rest = args.get(2..).unwrap_or(&[]);
    match (args[0].as_str(), sub) {
        ("resume", _) | ("reconcile", _) => Ok(Some(ShepherdOperation::Api(Request {
            id: format!("cli:shepherd:agent:{}", args[0]),
            method: if args[0] == "resume" {
                Method::ShepherdAgentResume(ShepherdEmptyParams {})
            } else {
                Method::ShepherdAgentReconcile(ShepherdEmptyParams {})
            },
        }))),
        ("cancel", _) => {
            let w = words(&args[1..], &[]);
            call("run:cancel", "cancel_run", json!({ "task": w.one()? }))
        }
        ("task", Some("delete")) => {
            let w = words(rest, &[]);
            call(
                "task:delete",
                "task_delete",
                json!({ "id": w.one()?, "confirm": w.flag("--confirm"), "archived": w.flag("--archived") }),
            )
        }
        ("task", Some("import")) => {
            let w = words(rest, &[]);
            call(
                "task:import",
                "import_task",
                json!({ "issue_ref": w.one()? }),
            )
        }
        ("skills", Some("create")) => {
            let w = words(rest, &["--description"]);
            call(
                "skill:create",
                "skills_create",
                json!({
                    "name": w.one()?,
                    "description": w.value("--description").unwrap_or(""),
                    "scope": if w.flag("--repo") { "repo" } else { "user" },
                    "global": w.flag("--global"),
                }),
            )
        }
        ("skills", Some("delete")) => {
            let w = words(rest, &[]);
            call("skill:delete", "skills_delete", json!({ "name": w.one()? }))
        }
        ("skills", Some("global")) => {
            let w = words(rest, &[]);
            call(
                "skill:global",
                "skills_global",
                json!({ "name": w.one()?, "global": !w.flag("--off") }),
            )
        }
        ("skills", Some("agent")) => {
            let w = words(rest, &[]);
            match w.positional.as_slice() {
                [agent, skill, state @ ("on" | "off")] => call(
                    "skill:agent",
                    "skills_toggle",
                    json!({ "agent_id": agent, "skill": skill, "enable": *state == "on" }),
                ),
                [agent, skill, "default"] => call(
                    "skill:agent",
                    "skills_restore",
                    json!({ "agent_id": agent, "skill": skill }),
                ),
                _ => Err(usage()),
            }
        }
        ("git", Some("branch")) => {
            let w = words(rest, &["--base"]);
            call(
                "git:branch",
                "create_branch",
                json!({ "branch": w.one()?, "base": w.value("--base").unwrap_or("") }),
            )
        }
        ("git", Some("worktree")) => {
            let action = rest.first().map(String::as_str);
            let w = words(rest.get(1..).unwrap_or(&[]), &["--branch"]);
            match action {
                Some("add") => {
                    let branch = w.value("--branch").ok_or_else(usage)?;
                    call(
                        "git:worktree:add",
                        "create_worktree",
                        json!({ "path": w.one()?, "branch": branch }),
                    )
                }
                Some("remove") => call(
                    "git:worktree:remove",
                    "remove_worktree",
                    json!({ "path": w.one()? }),
                ),
                _ => Err(usage()),
            }
        }
        ("git", Some("publish")) => {
            let w = words(rest, &[]);
            call(
                "git:publish",
                "publish_branch",
                json!({ "branch": w.one()? }),
            )
        }
        ("assign", _) => {
            let w = words(&args[1..], &[]);
            match w.positional.as_slice() {
                [agent, task] => call(
                    "agent:assign",
                    "assign_agent_task",
                    json!({ "agentId": agent, "task": if *task == "none" { Value::Null } else { json!(task) } }),
                ),
                _ => Err(usage()),
            }
        }
        ("pattern", Some("delete-run")) => {
            let w = words(rest, &[]);
            call(
                "pattern:delete-run",
                "run_delete",
                json!({ "id": w.one()?, "confirm": w.flag("--confirm") }),
            )
        }
        ("pattern", Some("clean")) => call("pattern:clean", "runs_clean", json!({})),
        ("bus", Some("clean")) => {
            let w = words(rest, &[]);
            call(
                "bus:clean",
                "bus_clean",
                json!({ "force": w.flag("--force") }),
            )
        }
        ("bus", _) => Err(usage()),
        _ => Ok(None),
    }
}

/// One line for a `shepherd.call` answer, for people (the JSON is printed as is with `--json`).
pub(crate) fn render_call(
    writer: &mut dyn std::io::Write,
    params: &ShepherdCallParams,
    result: &Value,
) -> std::io::Result<bool> {
    let d = &result["data"];
    let text = |k: &str| d[k].as_str().unwrap_or("").to_owned();
    let line = match params.op.as_str() {
        "cancel_run" => format!(
            "cancelled task {} (closed {} agent(s))",
            d["task"],
            d["killed"].as_array().map_or(0, Vec::len)
        ),
        "task_delete" => format!("deleted task {}", text("deleted")),
        "import_task" => format!(
            "imported issue as task {}-{}",
            d["status"]["id"],
            d["status"]["slug"].as_str().unwrap_or("")
        ),
        "skills_create" => format!(
            "created skill {}: {}",
            text("name"),
            d["file"].as_str().unwrap_or("")
        ),
        "skills_delete" => format!("deleted skill {}", text("name")),
        "skills_global" => format!(
            "skill {} is {} global",
            text("name"),
            if d["global"].as_bool() == Some(true) {
                "now"
            } else {
                "no longer"
            }
        ),
        "skills_toggle" | "skills_restore" => format!(
            "skills of {} saved; they apply the next time it starts",
            text("agent")
        ),
        "create_branch" => format!("created branch {}", text("branch")),
        "create_worktree" => format!(
            "worktree {} on {}",
            d["path"].as_str().unwrap_or(""),
            text("branch")
        ),
        "remove_worktree" => format!("removed worktree {}", text("path")),
        "publish_branch" => format!("pushed; PR {}", text("url")),
        "assign_agent_task" => match d["task"].as_u64() {
            Some(t) => format!("agent {} is now on task {t}", text("agent")),
            None => format!("agent {} is detached from its task", text("agent")),
        },
        "run_delete" => format!("deleted pattern run {}", text("id")),
        "runs_clean" => format!("removed {} finished pattern run(s)", d["removed"]),
        "bus_clean" => format!("removed {} bus message(s)", d["removed"]),
        _ => return Ok(false),
    };
    writeln!(writer, "{line}")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(line: &str) -> (String, Value) {
        let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        match parse_extra(&args).unwrap().expect("a command of this file") {
            ShepherdOperation::Api(Request {
                method: Method::ShepherdCall(p),
                ..
            }) => (p.op, p.args),
            _ => panic!("not a call"),
        }
    }

    #[test]
    fn every_command_maps_to_its_operation() {
        let cases = [
            ("cancel 3", "cancel_run"),
            ("task delete 2 --confirm", "task_delete"),
            ("task import #7", "import_task"),
            ("skills create my-skill --repo --global", "skills_create"),
            ("skills delete my-skill", "skills_delete"),
            ("skills global my-skill --off", "skills_global"),
            ("skills agent coder-1 debug on", "skills_toggle"),
            ("skills agent coder-1 debug off", "skills_toggle"),
            ("skills agent coder-1 debug default", "skills_restore"),
            ("git branch feat/x --base main", "create_branch"),
            ("git worktree add ../wt --branch feat/x", "create_worktree"),
            ("git worktree remove ../wt", "remove_worktree"),
            ("git publish feat/x", "publish_branch"),
            ("pattern clean", "runs_clean"),
            ("assign coder-1 3", "assign_agent_task"),
            ("assign coder-1 none", "assign_agent_task"),
            ("pattern delete-run run-1 --confirm", "run_delete"),
            ("bus clean --force", "bus_clean"),
        ];
        for (line, op) in cases {
            assert_eq!(parsed(line).0, op, "{line}");
        }
    }

    #[test]
    fn arguments_reach_the_operation() {
        let (_, a) = parsed("task delete 2 --archived");
        assert_eq!(
            (
                a["id"].as_str(),
                a["confirm"].as_bool(),
                a["archived"].as_bool()
            ),
            (Some("2"), Some(false), Some(true))
        );
        let (_, a) = parsed("skills create s --description hi --json");
        assert_eq!(
            (
                a["scope"].as_str(),
                a["description"].as_str(),
                a["global"].as_bool()
            ),
            (Some("user"), Some("hi"), Some(false))
        );
        let (_, a) = parsed("skills global s --off");
        assert_eq!(a["global"].as_bool(), Some(false));
        let (_, a) = parsed("git worktree add p --branch b");
        assert_eq!(
            (a["path"].as_str(), a["branch"].as_str()),
            (Some("p"), Some("b"))
        );
        assert!(a["root"].as_str().is_some_and(|r| !r.is_empty()));
    }

    #[test]
    fn bad_shapes_show_usage_and_other_commands_pass_through() {
        for line in [
            "cancel",
            "task delete",
            "git worktree add p",
            "git worktree",
            "bus",
            "skills create",
        ] {
            let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            assert!(parse_extra(&args).is_err(), "{line}");
        }
        for line in [
            "agents",
            "task list",
            "skills list",
            "git status",
            "pattern run x",
            "topic list",
        ] {
            let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
            assert!(parse_extra(&args).unwrap().is_none(), "{line}");
        }
    }

    #[test]
    fn every_command_of_this_file_is_routed_to_it() {
        // a command missing from the router answers "unknown command" before it is ever parsed
        for line in [
            "resume",
            "reconcile",
            "cancel 1",
            "assign a 1",
            "task delete 1",
            "skills create s",
            "git branch b",
            "pattern clean",
            "bus clean",
        ] {
            let first = line.split_whitespace().next().unwrap();
            assert!(
                crate::cli::SHEPHERD_DIRECT_COMMANDS.contains(&first),
                "`{first}` is not in SHEPHERD_DIRECT_COMMANDS"
            );
        }
    }

    #[test]
    fn answers_read_as_one_line() {
        let mut out = Vec::new();
        let p = ShepherdCallParams {
            op: "bus_clean".into(),
            args: json!({}),
        };
        assert!(render_call(&mut out, &p, &json!({ "data": { "removed": 4 } })).unwrap());
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "removed 4 bus message(s)\n"
        );
        let other = ShepherdCallParams {
            op: "unknown".into(),
            args: json!({}),
        };
        assert!(!render_call(&mut Vec::new(), &other, &json!({})).unwrap());
    }
}
