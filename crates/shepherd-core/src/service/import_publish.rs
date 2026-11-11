//! The two operations that talk to GitHub: `import_task` (read an issue with `gh`) and `publish_branch` (push a
//! branch and open its PR, only through the approval gate of a task).

use super::OrchestrationService;
use crate::{IssueRef, NewTask};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `42`, `#42`, `owner/repo#42` or an issue URL, with nothing git or `gh` could take for an option.
fn clean_issue_ref(raw: &str) -> Result<String, String> {
    let r = raw.trim();
    let r = r.strip_prefix('#').unwrap_or(r);
    if r.is_empty() || r.starts_with('-') || r.contains(char::is_whitespace) {
        return Err(format!("`{raw}` is not an issue number or URL"));
    }
    Ok(r.to_owned())
}

fn gh_issue_json(root: &Path, issue_ref: &str) -> Result<String, String> {
    let out = Command::new("gh")
        .args([
            "issue",
            "view",
            issue_ref,
            "--json",
            "number,title,body,url",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| {
            format!("`gh` could not run: {e} (install the GitHub CLI and run `gh auth login`)")
        })?;
    if !out.status.success() {
        return Err(format!(
            "gh issue view failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `https://github.com/owner/repo/issues/7` -> `owner/repo`.
fn repo_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split('/')
        .skip(1)
        .take(2)
        .collect::<Vec<_>>()
        .join("/")
}

fn new_task_from_issue(json: &str) -> Result<NewTask, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("unreadable issue: {e}"))?;
    let title = v["title"].as_str().unwrap_or_default().trim().to_owned();
    let url = v["url"].as_str().unwrap_or_default().to_owned();
    let number = v["number"].as_u64().unwrap_or(0) as u32;
    if title.is_empty() || number == 0 {
        return Err("the issue has no title or number".into());
    }
    let body = v["body"].as_str().unwrap_or_default().trim().to_owned();
    Ok(NewTask {
        id: 0,
        description: body.clone(),
        criteria: Vec::new(),
        issue_markdown: Some(format!("# {title}\n\n{body}\n")),
        issue: Some(IssueRef {
            number,
            repo: repo_of(&url),
            url,
        }),
        links: Vec::new(),
        title,
    })
}

fn import_with(
    root: &Path,
    issue_ref: &str,
    fetch: impl FnOnce(&Path, &str) -> Result<String, String>,
) -> Result<crate::CreatedTask, String> {
    let issue_ref = clean_issue_ref(issue_ref)?;
    let input = new_task_from_issue(&fetch(root, &issue_ref)?)?;
    let issue = input.issue.clone().expect("set above");
    let tasks = crate::list_tasks(root, false).map_err(|e| e.to_string())?;
    if let Some(t) = tasks
        .iter()
        .find(|t| t.status.issue.as_ref().is_some_and(|i| i.url == issue.url))
    {
        return Err(format!(
            "issue #{} is already task {}",
            issue.number, t.status.id
        ));
    }
    crate::create_task(root, input).map_err(|e| e.to_string())
}

impl OrchestrationService {
    /// Creates a task from a GitHub issue (title, body and a link to it).
    pub fn import_task(&self, root: &Path, issue_ref: &str) -> Result<Value, String> {
        let created = import_with(root, issue_ref, gh_issue_json)?;
        serde_json::to_value(created).map_err(|e| e.to_string())
    }

    /// Pushes `branch` and opens its PR, but only when a task has a subtask on that branch waiting for approval:
    /// the push is the approval of that gate, never a loose request.
    pub fn publish_branch(&self, root: PathBuf, branch: &str) -> Result<Value, String> {
        if branch.is_empty() {
            return Err("missing 'branch' for publish_branch".into());
        }
        let tasks = crate::list_tasks(&root, false).map_err(|e| e.to_string())?;
        let found = tasks.iter().find_map(|t| {
            t.status
                .subtasks
                .iter()
                .find(|s| s.branch == branch && s.state == "awaiting-approval")
                .map(|s| (t.status.id, s.number))
        });
        let Some((task, number)) = found else {
            return Err(format!(
                "no task is waiting to publish `{branch}`: approve its gate in Runs, or push it yourself with git"
            ));
        };
        self.approve(root, &task.to_string(), &format!("subtask:{number}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUE: &str = r#"{"number":7,"title":"Fix login","body":"It fails.","url":"https://github.com/o/r/issues/7"}"#;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("shep-import-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn refs_cannot_be_options() {
        assert_eq!(clean_issue_ref(" #42 ").unwrap(), "42");
        assert_eq!(clean_issue_ref("o/r#4").unwrap(), "o/r#4");
        for bad in ["", "-R", "--web", "a b", "#"] {
            assert!(clean_issue_ref(bad).is_err(), "{bad}");
        }
        assert_eq!(repo_of("https://github.com/o/r/issues/7"), "o/r");
    }

    #[test]
    fn an_issue_becomes_one_task() {
        let root = scratch("ok");
        let t = import_with(&root, "7", |_, r| {
            assert_eq!(r, "7");
            Ok(ISSUE.into())
        })
        .unwrap();
        assert_eq!(t.status.title, "Fix login");
        let issue = t.status.issue.unwrap();
        assert_eq!((issue.number, issue.repo.as_str()), (7, "o/r"));
        let md = std::fs::read_to_string(t.directory.join("issue.md")).unwrap();
        assert!(md.contains("It fails."));
        // the same issue twice is refused
        let again = import_with(&root, "#7", |_, _| Ok(ISSUE.into())).unwrap_err();
        assert!(again.contains("already task 1"), "{again}");
        // gh failing or answering nonsense creates nothing
        assert!(import_with(&root, "8", |_, _| Err("no auth".into())).is_err());
        assert!(import_with(&root, "8", |_, _| Ok("{}".into())).is_err());
        assert_eq!(crate::list_tasks(&root, false).unwrap().len(), 1);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
