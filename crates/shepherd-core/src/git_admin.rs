//! Local git operations behind the server operations `create_branch`, `create_worktree` and `remove_worktree`.
//! They never push, merge or force anything.

use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.trim().trim_start_matches("fatal: ").to_owned())
    }
}

fn check_branch_name(root: &Path, branch: &str) -> Result<(), String> {
    if branch.is_empty() || branch.starts_with('-') {
        return Err(format!("`{branch}` is not a valid branch name"));
    }
    git(root, &["check-ref-format", "--branch", branch])
        .map(|_| ())
        .map_err(|_| format!("`{branch}` is not a valid branch name"))
}

fn branch_exists(root: &Path, branch: &str) -> bool {
    git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

/// Creates `branch` at `base` (the current commit when empty) without switching to it.
pub fn create_branch(root: &Path, branch: &str, base: &str) -> Result<(), String> {
    check_branch_name(root, branch)?;
    if branch_exists(root, branch) {
        return Err(format!("branch `{branch}` already exists"));
    }
    let base = if base.is_empty() { "HEAD" } else { base };
    if base.starts_with('-') {
        return Err(format!("`{base}` is not a valid base"));
    }
    git(root, &["branch", branch, base]).map(|_| ())
}

fn resolve(root: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Adds a worktree at `path` on `branch`, creating the branch from the current commit when it does not exist.
/// Returns the absolute path.
pub fn create_worktree(root: &Path, path: &str, branch: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("missing 'path' for create_worktree".into());
    }
    check_branch_name(root, branch)?;
    let dir = resolve(root, path);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let dir_s = dir.to_string_lossy().into_owned();
    if branch_exists(root, branch) {
        git(root, &["worktree", "add", &dir_s, branch])?;
    } else {
        git(root, &["worktree", "add", "-b", branch, &dir_s])?;
    }
    Ok(dir)
}

/// Removes the worktree at `path`. Refuses the main checkout, a worktree with uncommitted changes (git's own
/// check) and one a live agent is working in. The branch is kept.
pub fn remove_worktree(root: &Path, path: &str, live_agent_paths: &[String]) -> Result<(), String> {
    if path.is_empty() {
        return Err("missing 'path' for remove_worktree".into());
    }
    let dir = resolve(root, path);
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let target = canon(&dir);
    if target == canon(root) {
        return Err("the main checkout cannot be removed".into());
    }
    if let Some(first) = git(root, &["worktree", "list", "--porcelain"])?
        .lines()
        .find_map(|l| l.strip_prefix("worktree "))
    {
        if target == canon(Path::new(first)) {
            return Err("the main checkout cannot be removed".into());
        }
    }
    if live_agent_paths
        .iter()
        .any(|p| !p.is_empty() && canon(Path::new(p)) == target)
    {
        return Err("a live agent is working in this worktree: kill it first".into());
    }
    git(root, &["worktree", "remove", &target.to_string_lossy()]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shep-gitadmin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q", "-b", "main"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "i",
            ],
        ] {
            git(&dir, args).unwrap();
        }
        dir
    }

    #[test]
    fn branches_are_created_once_and_names_are_checked() {
        let r = repo("br");
        create_branch(&r, "feat/x", "").unwrap();
        assert!(branch_exists(&r, "feat/x"));
        assert!(create_branch(&r, "feat/x", "")
            .unwrap_err()
            .contains("already exists"));
        assert!(create_branch(&r, "bad name", "")
            .unwrap_err()
            .contains("not a valid"));
        assert!(create_branch(&r, "-x", "").is_err());
        assert!(create_branch(&r, "y", "nope").is_err());
        create_branch(&r, "y", "feat/x").unwrap();
        std::fs::remove_dir_all(&r).unwrap();
    }

    #[test]
    fn worktrees_are_added_and_removed_safely() {
        let r = repo("wt");
        let wt = r.join("../shep-gitadmin-wt-side");
        let wt_s = wt.to_string_lossy().into_owned();
        let _ = std::fs::remove_dir_all(&wt);
        let made = create_worktree(&r, &wt_s, "side").unwrap();
        assert!(made.join(".git").exists());
        assert!(branch_exists(&r, "side"));
        assert!(create_worktree(&r, &wt_s, "side")
            .unwrap_err()
            .contains("already exists"));
        // an existing branch is reused
        create_branch(&r, "old", "").unwrap();
        let wt2 = r.join("../shep-gitadmin-wt-side2");
        let _ = std::fs::remove_dir_all(&wt2);
        create_worktree(&r, &wt2.to_string_lossy(), "old").unwrap();
        git(&r, &["worktree", "remove", &wt2.to_string_lossy()]).unwrap();

        assert!(remove_worktree(&r, &r.to_string_lossy(), &[])
            .unwrap_err()
            .contains("main checkout"));
        assert!(remove_worktree(&r, &wt_s, &[wt_s.clone()])
            .unwrap_err()
            .contains("live agent"));
        // uncommitted work blocks the removal
        std::fs::write(wt.join("dirty.txt"), "x").unwrap();
        assert!(remove_worktree(&r, &wt_s, &[]).is_err());
        assert!(wt.exists());
        std::fs::remove_file(wt.join("dirty.txt")).unwrap();
        remove_worktree(&r, &wt_s, &[]).unwrap();
        assert!(!wt.exists());
        assert!(branch_exists(&r, "side"), "the branch stays");
        std::fs::remove_dir_all(&r).unwrap();
    }
}
