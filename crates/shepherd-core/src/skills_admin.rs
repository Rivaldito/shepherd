//! Creating, deleting and globalizing catalog skills. The server operations `skills_create`, `skills_delete`
//! and `skills_global` call these, so the terminal UI, the CLI and the desktop app behave the same.

use crate::config::{global_config_path, load_config_files};
use crate::skills::{name_ok, new_skill, repo_skills_dir, toggle, user_skills_dir, Catalog, Scope};
use std::path::{Path, PathBuf};

fn catalog(root: &Path, user_dir: &Path) -> Catalog {
    Catalog::load(Some(root), Some(user_dir))
}

/// Whether the catalog (defaults, the user's and the project's) has a skill called `name`.
pub fn exists(root: &Path, name: &str) -> bool {
    catalog(root, &user_skills_dir()).get(name).is_some()
}

/// Creates `<skills dir>/<name>/SKILL.md` in the project (`Scope::Repo`) or for the user, and returns the file.
/// A non-empty `description` replaces the template's placeholder.
pub fn create(root: &Path, name: &str, description: &str, scope: Scope) -> Result<PathBuf, String> {
    let dir = match scope {
        Scope::Repo => repo_skills_dir(root),
        Scope::User => user_skills_dir(),
        Scope::Default => return Err("a new skill goes to `user` or `repo`".into()),
    };
    create_in(root, &user_skills_dir(), &dir, name, description)
}

fn create_in(
    root: &Path,
    user_dir: &Path,
    dir: &Path,
    name: &str,
    description: &str,
) -> Result<PathBuf, String> {
    if !name_ok(name) {
        return Err("a name is lowercase letters, digits and dashes".into());
    }
    // a name taken anywhere (default, user or project) would shadow or be shadowed
    if catalog(root, user_dir).get(name).is_some() {
        return Err(format!("a skill called `{name}` already exists"));
    }
    let file = new_skill(dir, name)?;
    let description = description.trim().replace('\n', " ");
    if !description.is_empty() {
        let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
        let text = text
            .lines()
            .map(|l| {
                if l.starts_with("description:") {
                    format!("description: {description}")
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file, text + "\n").map_err(|e| e.to_string())?;
    }
    Ok(file)
}

/// Deletes the folder of a skill of the user or of the project. One that ships with Shepherd cannot be deleted.
pub fn delete(root: &Path, name: &str) -> Result<(), String> {
    delete_in(root, &user_skills_dir(), name)
}

fn delete_in(root: &Path, user_dir: &Path, name: &str) -> Result<(), String> {
    let catalog = catalog(root, user_dir);
    let skill = catalog.get(name).ok_or(format!("unknown skill `{name}`"))?;
    if skill.scope == Scope::Default {
        return Err(format!(
            "`{name}` ships with Shepherd and cannot be deleted"
        ));
    }
    let dir = Path::new(&skill.dir);
    let allowed = [user_dir.to_path_buf(), repo_skills_dir(root)];
    if !allowed.iter().any(|base| dir.starts_with(base)) {
        return Err(format!(
            "{} is not a skills folder of this user or project",
            dir.display()
        ));
    }
    std::fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// Puts a skill in (or takes it out of) `skills.global` of the user's config. When another layer decides it
/// the file is left as it was and the error says why.
pub fn set_global(root: &Path, name: &str, on: bool) -> Result<(), String> {
    set_global_in(root, &user_skills_dir(), &global_config_path(), name, on)
}

fn set_global_in(
    root: &Path,
    user_dir: &Path,
    cfg_path: &Path,
    name: &str,
    on: bool,
) -> Result<(), String> {
    if catalog(root, user_dir).get(name).is_none() {
        return Err(format!("unknown skill `{name}`"));
    }
    // start from what this file lists, not from the merged list: the other layers keep their own
    let own = load_config_files(&[cfg_path.to_path_buf()])
        .map(|c| c.skills.global)
        .map_err(|e| e.to_string())?;
    toggle(cfg_path, "skills.global", name, on, &own, root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("shep-skadmin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_then_delete_a_user_skill() {
        let dir = scratch("cd");
        let (root, user) = (dir.join("proj"), dir.join("user"));
        std::fs::create_dir_all(&root).unwrap();
        let file = create_in(&root, &user, &user, "my-skill", "Use when testing").unwrap();
        assert_eq!(file, user.join("my-skill/SKILL.md"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("description: Use when testing"), "{text}");
        assert!(create_in(&root, &user, &user, "my-skill", "")
            .unwrap_err()
            .contains("exists"));
        assert!(create_in(&root, &user, &user, "Bad Name", "").is_err());
        // a skill that ships with Shepherd is taken
        let builtin = Catalog::load(Some(&root), Some(&user)).all()[0]
            .name
            .clone();
        assert!(create_in(&root, &user, &user, &builtin, "").is_err());
        assert!(delete_in(&root, &user, &builtin)
            .unwrap_err()
            .contains("ships with"));
        delete_in(&root, &user, "my-skill").unwrap();
        assert!(!user.join("my-skill").exists());
        assert!(delete_in(&root, &user, "my-skill")
            .unwrap_err()
            .contains("unknown"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn global_is_idempotent_and_checks_the_name() {
        let dir = scratch("gl");
        let (root, user, cfg) = (dir.join("proj"), dir.join("user"), dir.join("config.yaml"));
        std::fs::create_dir_all(&root).unwrap();
        create_in(&root, &user, &user, "mine", "").unwrap();
        let own = || {
            load_config_files(std::slice::from_ref(&cfg))
                .map(|c| c.skills.global)
                .unwrap_or_default()
        };
        set_global_in(&root, &user, &cfg, "mine", true).unwrap();
        set_global_in(&root, &user, &cfg, "mine", true).unwrap();
        assert_eq!(own().iter().filter(|s| *s == "mine").count(), 1);
        set_global_in(&root, &user, &cfg, "mine", false).unwrap();
        assert!(own().is_empty());
        assert!(set_global_in(&root, &user, &cfg, "nope", true)
            .unwrap_err()
            .contains("unknown skill"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
