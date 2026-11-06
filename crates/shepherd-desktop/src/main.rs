//! Thin Tauri shell over `shepherd-core`: lists, reads and saves the config/pattern files.
//! Paths from the webview are only accepted if they appear in `list_files()` (allowlist).

mod api;

use serde::Serialize;
use serde_json::json;
use serde_yaml_ng::{Mapping, Value};
use shepherd_core::bus::{ConsumerGroupSummary, Message, MessageBus};
use shepherd_core::config::{self, Config, Role, Vendor};
use shepherd_core::git::{collect_git_status, GitRepoStatus};
use shepherd_core::patterns::{self, Pattern, PatternSource, PatternSummary, Problem};
use shepherd_core::service::patterns_run::{load_pattern_runs, pattern_runs_path, PatternRun};
use shepherd_core::skills::{user_skills_dir, Catalog, Skill};
use shepherd_core::{
    list_tasks, load_agent_registry, read_task_section, Agent, AgentRegistryFile, CreatedTask,
    TaskStatus,
};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::Emitter;

#[derive(Serialize)]
struct FileEntry {
    group: &'static str,
    label: String,
    path: String,
    exists: bool,
}

fn project_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn entry(group: &'static str, label: String, p: PathBuf) -> FileEntry {
    FileEntry {
        group,
        label,
        exists: p.is_file(),
        path: p.display().to_string(),
    }
}

fn known_files() -> Vec<FileEntry> {
    let root = project_root();
    let mut v = vec![
        entry(
            "config",
            "config.toml (global)".into(),
            config::global_config_toml_path()
                .unwrap_or_else(|| config::global_config_path().with_file_name("config.toml")),
        ),
        entry(
            "config",
            "config.yaml (global)".into(),
            config::global_config_path(),
        ),
        entry(
            "config",
            "config.yaml (proyecto)".into(),
            config::project_config_path(&root),
        ),
    ];
    if let Ok(list) = patterns::list_patterns(&root, None) {
        for p in list {
            if let Some(path) = p.path {
                v.push(entry("patterns", p.name, PathBuf::from(path)));
            }
        }
    }
    v
}

fn allowed(path: &str) -> Result<PathBuf, String> {
    known_files()
        .into_iter()
        .find(|f| f.path == path)
        .map(|f| PathBuf::from(f.path))
        .ok_or_else(|| "ruta no permitida".to_string())
}

#[tauri::command]
fn list_files() -> Vec<FileEntry> {
    known_files()
}

#[tauri::command]
fn read_file(path: String) -> Result<String, String> {
    let p = allowed(&path)?;
    if !p.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(p).map_err(|e| e.to_string())
}

/// Syntax-check by extension; returns an error message instead of writing invalid text.
fn check_syntax(path: &Path, text: &str) -> Result<(), String> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("toml") => toml::from_str::<toml::Value>(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Some("yaml" | "yml") => serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        _ => Err("extensión no soportada".into()),
    }
}

/// Saves the file and returns semantic warnings (empty = clean).
#[tauri::command]
fn save_file(path: String, text: String) -> Result<Vec<String>, String> {
    let p = allowed(&path)?;
    check_syntax(&p, &text)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, &text).map_err(|e| e.to_string())?;
    Ok(warnings())
}

/// Semantic problems of the effective (merged) config; empty = clean.
fn warnings() -> Vec<String> {
    match config::load_config(project_root()) {
        Ok(cfg) => config::validate(&cfg),
        Err(e) => vec![format!("config no carga: {e}")],
    }
}

#[derive(Serialize)]
struct Models {
    vendors: BTreeMap<String, Vendor>,
    roles: BTreeMap<String, Role>,
    /// File the forms write to (global config.yaml).
    target: String,
}

#[tauri::command]
fn get_models() -> Result<Models, String> {
    let cfg = config::load_config(project_root()).map_err(|e| e.to_string())?;
    Ok(Models {
        vendors: cfg.vendors.into_iter().collect(),
        roles: cfg.roles.into_iter().collect(),
        target: config::global_config_path().display().to_string(),
    })
}

/// Sets (`Some`) or removes (`None`) `section.name` in a YAML mapping, leaving other keys alone.
fn apply(map: &mut Mapping, section: &str, name: &str, value: Option<Value>) {
    let key = Value::from(section);
    if !matches!(map.get(&key), Some(Value::Mapping(_))) {
        map.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    if let Some(Value::Mapping(sec)) = map.get_mut(&key) {
        match value {
            Some(v) => sec.insert(Value::from(name), v),
            None => sec.remove(name),
        };
    }
}

/// Rewrites the global config.yaml with one entry changed. A `.bak` copy is kept.
// ponytail: re-serializing drops YAML comments; the raw "Config" tab edits without losing them.
fn update_yaml(section: &str, name: &str, value: Option<Value>) -> Result<Vec<String>, String> {
    if !patterns::is_valid_ident(name) {
        return Err("nombre inválido (usa letras, números, - y _)".into());
    }
    let path = config::global_config_path();
    let mut map = match std::fs::read_to_string(&path) {
        Ok(t) if !t.trim().is_empty() => {
            match serde_yaml_ng::from_str(&t).map_err(|e| e.to_string())? {
                Value::Mapping(m) => m,
                _ => return Err("config.yaml no es un mapa".into()),
            }
        }
        _ => Mapping::new(),
    };
    apply(&mut map, section, name, value);
    let text = serde_yaml_ng::to_string(&map).map_err(|e| e.to_string())?;
    if path.is_file() {
        std::fs::copy(&path, path.with_extension("yaml.bak")).map_err(|e| e.to_string())?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(warnings())
}

#[tauri::command]
fn save_vendor(name: String, vendor: Vendor) -> Result<Vec<String>, String> {
    let v = serde_yaml_ng::to_value(vendor).map_err(|e| e.to_string())?;
    update_yaml("vendors", &name, Some(v))
}

#[tauri::command]
fn save_role(name: String, role: Role) -> Result<Vec<String>, String> {
    let v = serde_yaml_ng::to_value(role).map_err(|e| e.to_string())?;
    update_yaml("roles", &name, Some(v))
}

#[tauri::command]
fn delete_entry(section: String, name: String) -> Result<Vec<String>, String> {
    match section.as_str() {
        "vendors" | "roles" => update_yaml(&section, &name, None),
        _ => Err("sección no permitida".into()),
    }
}

#[derive(Serialize)]
struct LoadedPattern {
    pattern: Pattern,
    source: PatternSource,
    path: Option<String>,
    problems: Vec<Problem>,
}

#[derive(Serialize)]
struct SavedPattern {
    path: String,
    source: PatternSource,
    problems: Vec<Problem>,
}

fn problems_for(pattern: &Pattern) -> Result<Vec<Problem>, String> {
    let cfg = config::load_config(project_root()).map_err(|e| e.to_string())?;
    Ok(patterns::validate(pattern, &cfg))
}

#[tauri::command]
fn pattern_list() -> Result<Vec<PatternSummary>, String> {
    patterns::list_patterns(&project_root(), None).map_err(|e| e.to_string())
}

#[tauri::command]
fn pattern_load(name: String) -> Result<LoadedPattern, String> {
    let (pattern, source, path) =
        patterns::load_pattern(&name, &project_root(), None).map_err(|e| e.to_string())?;
    let problems = problems_for(&pattern)?;
    Ok(LoadedPattern {
        pattern,
        source,
        path: path.map(|p| p.display().to_string()),
        problems,
    })
}

#[tauri::command]
fn pattern_validate(pattern: Pattern) -> Result<Vec<Problem>, String> {
    problems_for(&pattern)
}

/// Existing project patterns are saved back to the project; everything else goes to the global dir.
/// Saving never renames: a new name is a new file (`is_new`), and must not overwrite another.
#[tauri::command]
fn pattern_save(
    pattern: Pattern,
    source: PatternSource,
    is_new: bool,
) -> Result<SavedPattern, String> {
    patterns::validate_pattern_name(&pattern.name).map_err(|e| e.to_string())?;
    let (source, dir) = if source == PatternSource::Project && !is_new {
        (
            PatternSource::Project,
            patterns::project_patterns_dir(&project_root()),
        )
    } else {
        (
            PatternSource::Global,
            patterns::global_patterns_dir_with(None),
        )
    };
    if is_new && dir.join(format!("{}.yaml", pattern.name)).exists() {
        return Err(patterns::PatternError::AlreadyExists(pattern.name).to_string());
    }
    let path = patterns::save_pattern_to_dir(&pattern, &dir).map_err(|e| e.to_string())?;
    let problems = problems_for(&pattern)?;
    Ok(SavedPattern {
        path: path.display().to_string(),
        source,
        problems,
    })
}

#[tauri::command]
fn pattern_delete(name: String) -> Result<(), String> {
    patterns::delete_pattern(&name, &project_root(), None)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Agents as the webview may see them: ordered by start, with their secret token removed.
fn public_agents(registry: AgentRegistryFile) -> Vec<Agent> {
    let mut agents: Vec<Agent> = registry.agents.iter().map(Agent::public).collect();
    agents.sort_by_key(|a| a.seq);
    agents
}

/// Read-only view of the persisted registry (`agents.json`); it can lag behind a running server.
#[tauri::command]
fn agent_list() -> Result<Vec<Agent>, String> {
    load_agent_registry(config::state_dir().join("agents.json"))
        .map(public_agents)
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct RunView {
    #[serde(flatten)]
    run: PatternRun,
    /// Agents of the run that are still live in the registry.
    live_agents: usize,
}

/// Every `pattern_runs.json` that can hold runs we know of: this project, the global state dir,
/// and the root of each registered agent (the same places the service reopens runs from).
fn run_files(registry: &AgentRegistryFile) -> Vec<PathBuf> {
    let (root, state) = (project_root(), config::state_dir());
    let mut paths = vec![
        pattern_runs_path(&root, &state),
        pattern_runs_path(&state, &state),
    ];
    paths.extend(
        registry
            .agents
            .iter()
            .filter(|a| !a.root.is_empty())
            .map(|a| pattern_runs_path(Path::new(&a.root), &state)),
    );
    let mut seen = HashSet::new();
    paths.retain(|p| seen.insert(p.clone()));
    paths
}

/// Merges runs from several files (first occurrence of an id wins), newest first.
fn collect_runs(paths: &[PathBuf], registry: &AgentRegistryFile) -> Result<Vec<RunView>, String> {
    let live: HashSet<&str> = registry.live().iter().map(|a| a.id.as_str()).collect();
    let mut ids = HashSet::new();
    let mut out = Vec::new();
    for path in paths {
        for run in load_pattern_runs(path)? {
            if ids.insert(run.id.clone()) {
                let live_agents = run
                    .agents
                    .iter()
                    .filter(|a| live.contains(a.id.as_str()))
                    .count();
                out.push(RunView { run, live_agents });
            }
        }
    }
    out.sort_by(|a, b| b.run.created_at.cmp(&a.run.created_at));
    Ok(out)
}

/// Read-only view of persisted pattern runs; the live state belongs to the running server.
#[tauri::command]
fn run_list() -> Result<Vec<RunView>, String> {
    let registry = load_agent_registry(config::state_dir().join("agents.json")).unwrap_or_default();
    collect_runs(&run_files(&registry), &registry)
}

/// Root of the project the app was started in: where `.shepherd/` (config, tasks) lives.
fn task_root() -> PathBuf {
    let cfg = config::project_config_path(&project_root());
    cfg.parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(project_root)
}

#[derive(Serialize)]
struct TaskView {
    #[serde(flatten)]
    status: TaskStatus,
    description: String,
    /// Acceptance criteria as written (`- [ ] …` / `- [x] …`).
    criteria: Vec<String>,
    links: Vec<String>,
    plan: String,
}

/// Mirrors the TUI row: the spec unless it is still the untouched template, else the issue body.
fn task_description(spec: &str, issue: &str) -> String {
    const TEMPLATE: &str =
        "Design decisions, constraints, interfaces. Agents read this before coding.";
    let spec = spec.trim();
    if !(spec.is_empty()
        || spec.starts_with("# Spec")
            && spec.contains(TEMPLATE)
            && spec.len() <= TEMPLATE.len() + 20)
    {
        return spec.to_string();
    }
    let issue = issue.trim();
    match issue.strip_prefix("# ") {
        Some(rest) => rest
            .split_once('\n')
            .map(|(_, body)| body.trim().to_string())
            .unwrap_or_default(),
        None => issue.to_string(),
    }
}

fn bullet_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("- ") || l.starts_with("* "))
        .filter(|l| !l.contains("(add checkable criteria; the reviewer verifies each one)"))
        .map(str::to_string)
        .collect()
}

fn task_view(task: CreatedTask) -> TaskView {
    let section = |name: &str| read_task_section(&task, name).unwrap_or_default();
    TaskView {
        description: task_description(&section("spec"), &section("issue")),
        criteria: bullet_lines(&section("acceptance")),
        links: bullet_lines(&section("links")),
        plan: section("plan").trim().to_string(),
        status: task.status,
    }
}

/// Read-only view of the project's tasks (`.shepherd/tasks`), oldest id first.
#[tauri::command]
fn task_list() -> Result<Vec<TaskView>, String> {
    list_tasks(task_root(), false)
        .map(|t| t.into_iter().map(task_view).collect())
        .map_err(|e| e.to_string())
}

/// Where tasks are read from, so an empty list can say where it looked.
#[tauri::command]
fn task_dir() -> String {
    task_root().join(".shepherd/tasks").display().to_string()
}

#[derive(Serialize)]
struct SkillView {
    #[serde(flatten)]
    skill: Skill,
    /// Passed validation (the TUI's "Habilitada").
    enabled: bool,
    /// Full `SKILL.md`.
    body: String,
}

#[derive(Serialize)]
struct SkillCatalog {
    skills: Vec<SkillView>,
    user_dir: String,
    repo_dir: String,
}

fn catalog_and_config() -> Result<(Catalog, Config), String> {
    let root = task_root();
    let cfg = config::load_config(&root).map_err(|e| e.to_string())?;
    Ok((Catalog::load(Some(&root), Some(&user_skills_dir())), cfg))
}

/// Read-only merged catalog (`repo > user > default`); `global` also reflects `skills.global`.
#[tauri::command]
fn skill_catalog() -> Result<SkillCatalog, String> {
    let (cat, cfg) = catalog_and_config()?;
    let globals = cat.global_names(&cfg);
    let skills = cat
        .all()
        .into_iter()
        .map(|mut skill| {
            skill.global = globals.contains(&skill.name);
            SkillView {
                enabled: skill.ok(),
                body: skill.body().to_string(),
                skill,
            }
        })
        .collect();
    let root = task_root();
    Ok(SkillCatalog {
        skills,
        user_dir: user_skills_dir().display().to_string(),
        repo_dir: shepherd_core::skills::repo_skills_dir(&root)
            .display()
            .to_string(),
    })
}

#[derive(Serialize)]
struct ActiveSkill {
    name: String,
    /// Layer that enabled it (`global`, `role`, `session`); unknown for skills no longer resolvable.
    level: Option<&'static str>,
}

#[derive(Serialize)]
struct AgentSkills {
    active: Vec<ActiveSkill>,
    problems: Vec<String>,
}

/// What the registry recorded as installed wins; otherwise the effective set for the agent's role.
fn agent_skills_for(cat: &Catalog, cfg: &Config, agent: &Agent) -> AgentSkills {
    let (resolved, problems) = cat.effective_for(cfg, &agent.role, &[], &agent.skills);
    let level = |name: &str| {
        resolved
            .iter()
            .find(|r| r.skill.name == name)
            .map(|r| r.level.as_str())
    };
    let active = if agent.active_skills.is_empty() {
        resolved
            .iter()
            .map(|r| ActiveSkill {
                name: r.skill.name.clone(),
                level: Some(r.level.as_str()),
            })
            .collect()
    } else {
        agent
            .active_skills
            .iter()
            .map(|n| ActiveSkill {
                name: n.clone(),
                level: level(n),
            })
            .collect()
    };
    AgentSkills { active, problems }
}

#[tauri::command]
fn skill_agent(agent_id: String) -> Result<AgentSkills, String> {
    let registry =
        load_agent_registry(config::state_dir().join("agents.json")).map_err(|e| e.to_string())?;
    let agent = registry
        .agents
        .iter()
        .find(|a| a.id == agent_id)
        .ok_or("agente no encontrado")?;
    let (cat, cfg) = catalog_and_config()?;
    Ok(agent_skills_for(&cat, &cfg, agent))
}

/// Worktrees of the project with dirty/ahead/behind, linked agent and PR (needs `git`; PRs need `gh`).
/// `async` so the command runs off the main thread: git and gh can take seconds.
#[tauri::command(async)]
fn git_status() -> Result<GitRepoStatus, String> {
    let registry = load_agent_registry(config::state_dir().join("agents.json")).unwrap_or_default();
    collect_git_status(&task_root(), &registry.agents)
}

#[derive(Serialize)]
struct TopicView {
    name: String,
    partitions: u32,
    messages_count: usize,
    paused: bool,
    groups: Vec<ConsumerGroupSummary>,
}

/// Opening the bus only reads `messages.jsonl` and its sidecars (nothing is written or committed).
fn open_bus() -> Result<MessageBus, String> {
    MessageBus::open(config::state_dir().join("messages.jsonl")).map_err(|e| e.to_string())
}

fn topics_from(bus: &MessageBus) -> Result<Vec<TopicView>, String> {
    let groups = bus.consumer_groups(None).map_err(|e| e.to_string())?;
    let topics = bus.list_topics().map_err(|e| e.to_string())?;
    Ok(topics
        .into_iter()
        .map(|t| TopicView {
            paused: bus.is_topic_paused(&t.name),
            groups: groups
                .iter()
                .filter(|g| g.topic == t.name)
                .cloned()
                .collect(),
            name: t.name,
            partitions: t.partitions,
            messages_count: t.messages_count,
        })
        .collect())
}

/// `from_offset: None` tails the newest `limit` messages; `Some(n)` replays the first `limit` from offset `n`.
/// Messages come oldest first. `agent` keeps only that sender.
fn messages_from(
    bus: &MessageBus,
    topic: &str,
    from_offset: Option<u64>,
    agent: Option<&str>,
    limit: usize,
) -> Result<Vec<Message>, String> {
    let limit = limit.clamp(1, 500);
    let mut msgs = bus
        .replay(topic, None, from_offset.unwrap_or(0), None)
        .map_err(|e| e.to_string())?;
    if let Some(a) = agent.filter(|a| !a.is_empty()) {
        msgs.retain(|m| m.from == a);
    }
    if from_offset.is_none() {
        let skip = msgs.len().saturating_sub(limit);
        msgs.drain(..skip);
    } else {
        msgs.truncate(limit);
    }
    Ok(msgs)
}

/// Read-only topics of the event bus. `async`: it loads the whole timeline.
#[tauri::command(async)]
fn topic_list() -> Result<Vec<TopicView>, String> {
    topics_from(&open_bus()?)
}

#[tauri::command(async)]
fn topic_messages(
    topic: String,
    from_offset: Option<u64>,
    agent: Option<String>,
    limit: usize,
) -> Result<Vec<Message>, String> {
    messages_from(&open_bus()?, &topic, from_offset, agent.as_deref(), limit)
}

#[derive(Serialize)]
struct ApiStatus {
    socket: String,
    running: bool,
    version: Option<String>,
    protocol: Option<u64>,
    /// Binary used by "start server"; `None` when it cannot be found.
    server_bin: Option<String>,
}

fn api_status_now() -> ApiStatus {
    let socket = api::default_socket_path();
    let pong = api::call(&socket, "ping", json!({})).ok();
    ApiStatus {
        running: pong.is_some(),
        version: pong
            .as_ref()
            .and_then(|p| p["version"].as_str())
            .map(String::from),
        protocol: pong.as_ref().and_then(|p| p["protocol"].as_u64()),
        server_bin: api::default_server_bin().map(|p| p.display().to_string()),
        socket: socket.display().to_string(),
    }
}

#[tauri::command(async)]
fn api_status() -> ApiStatus {
    api_status_now()
}

/// The UI's single door to the server. Only runtime methods pass (see `api::allowed_method`).
#[tauri::command(async)]
fn api_call(
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    if !api::allowed_method(&method) {
        return Err(format!("método no permitido: {method}"));
    }
    api::call(
        &api::default_socket_path(),
        &method,
        params.unwrap_or_else(|| json!({})),
    )
}

/// The console's door to the `shepherd` CLI, aimed at the same socket as the rest of the app.
#[tauri::command(async)]
fn cli_run(args: Vec<String>) -> Result<api::CliOutput, String> {
    if !api::allowed_cli(&args) {
        return Err(format!(
            "comando no permitido: {}",
            args.first().map(String::as_str).unwrap_or("(vacío)")
        ));
    }
    let bin = api::default_server_bin()
        .ok_or("no se encontró el binario `shepherd`: ponlo en el PATH o define SHEPHERD_BIN")?;
    api::run_cli(
        &bin,
        &api::default_socket_path(),
        &args,
        std::time::Duration::from_secs(15),
    )
}

/// Explicit user action: starting a server creates real state, so it never happens on its own.
#[tauri::command(async)]
fn api_start_server() -> Result<ApiStatus, String> {
    let socket = api::default_socket_path();
    if api::call(&socket, "ping", json!({})).is_err() {
        let bin = api::default_server_bin().ok_or(
            "no se encontró el binario `shepherd`: ponlo en el PATH o define SHEPHERD_BIN",
        )?;
        api::start_server(&bin, &socket)?;
    }
    Ok(api_status_now())
}

/// Keeps one subscription alive and forwards server events to the webview, reconnecting forever.
/// `shepherd-event` means "re-read the snapshot"; `shepherd-connection` carries up/down changes.
fn spawn_event_forwarder(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let connected = std::cell::Cell::new(false);
        let announce = |now: bool| {
            if connected.replace(now) != now {
                let _ = app.emit("shepherd-connection", json!({ "connected": now }));
            }
        };
        let result = api::read_events(
            &api::default_socket_path(),
            api::EVENT_TYPES,
            &mut || announce(true),
            &mut |event| {
                let _ = app.emit("shepherd-event", event);
            },
        );
        announce(false);
        if matches!(&result, Err(e) if e.starts_with("events_lost")) {
            let _ = app.emit("shepherd-event", json!({ "event": "events_lost" }));
        }
        std::thread::sleep(std::time::Duration::from_millis(1500));
    });
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            spawn_event_forwarder(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_files,
            read_file,
            save_file,
            get_models,
            save_vendor,
            save_role,
            delete_entry,
            pattern_list,
            pattern_load,
            pattern_validate,
            pattern_save,
            pattern_delete,
            agent_list,
            run_list,
            task_list,
            task_dir,
            skill_catalog,
            skill_agent,
            git_status,
            topic_list,
            topic_messages,
            api_status,
            api_call,
            api_start_server,
            cli_run
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Shepherd desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_check_by_extension() {
        assert!(check_syntax(Path::new("a.toml"), "x = 1").is_ok());
        assert!(check_syntax(Path::new("a.toml"), "x = ").is_err());
        assert!(check_syntax(Path::new("a.yaml"), "a: [1, 2]").is_ok());
        assert!(check_syntax(Path::new("a.yaml"), "a: [1").is_err());
        assert!(check_syntax(Path::new("a.json"), "{}").is_err());
    }

    #[test]
    fn apply_sets_and_removes_only_its_key() {
        let mut m: Mapping =
            serde_yaml_ng::from_str("vendors:\n  a: {command: x}\nlimits: {max_depth: 3}").unwrap();
        apply(&mut m, "vendors", "b", Some(Value::from("y")));
        apply(&mut m, "roles", "r", Some(Value::from("z")));
        assert_eq!(m["vendors"]["a"]["command"].as_str(), Some("x"));
        assert_eq!(m["vendors"]["b"].as_str(), Some("y"));
        assert_eq!(m["limits"]["max_depth"].as_u64(), Some(3));
        assert_eq!(m["roles"]["r"].as_str(), Some("z"));
        apply(&mut m, "vendors", "a", None);
        assert!(m["vendors"].get("a").is_none() && m["vendors"].get("b").is_some());
    }

    #[test]
    fn pattern_accepts_json_the_canvas_sends() {
        // Optional fields arrive as null, counts/levels as numbers, kind as kebab-case.
        let p: Pattern = serde_json::from_str(
            r#"{"name":"t","kind":"market","description":"","topics":[{"name":"ask","partitions":1,"key":null,"filter":null}],
            "agents":[{"id":"a","role":"r","vendor":null,"model":null,"prompt":null,"count":2,"level":null,
            "publishes":["ask"],"consumes":[],"group":null,"start":true}]}"#,
        )
        .unwrap();
        assert_eq!((p.agents[0].count, p.agents[0].start), (2, true));
    }

    #[test]
    fn builtin_pattern_loads_and_round_trips_through_json() {
        let r = pattern_load("market".into()).unwrap();
        assert_eq!(r.source, PatternSource::Builtin);
        // What the webview gets, and sends back after editing, must still be a valid Pattern.
        let json = serde_json::to_string(&r.pattern).unwrap();
        let back: Pattern = serde_json::from_str(&json).unwrap();
        assert_eq!(back, r.pattern);
        assert!(!pattern_list().unwrap().is_empty());
    }

    #[test]
    fn agents_reach_the_webview_without_tokens_and_in_start_order() {
        let agent = |id: &str, seq| Agent {
            id: id.into(),
            seq,
            token: "secret".into(),
            ..Agent::default()
        };
        let reg = AgentRegistryFile {
            agents: vec![agent("b-2", 2), agent("a-1", 1)],
            ..Default::default()
        };
        let out = public_agents(reg);
        assert_eq!(
            out.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["a-1", "b-2"]
        );
        assert!(out.iter().all(|a| a.token.is_empty()));
        assert!(!serde_json::to_string(&out).unwrap().contains("secret"));
    }

    #[test]
    fn runs_merge_across_files_dedupe_and_sort_newest_first() {
        let dir =
            std::env::temp_dir().join(format!("shepherd-desktop-runs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |id: &str, at: &str, agent: &str| {
            format!(
                r#"{{"id":"{id}","pattern":"p","kind":"market","task":"t","state":"running","created_at":"{at}","updated_at":"{at}","agents":[{{"id":"{agent}","role":"r","state":"idle"}}]}}"#
            )
        };
        let (a, b) = (dir.join("a.json"), dir.join("b.json"));
        std::fs::write(
            &a,
            format!(
                "[{},{}]",
                run("r1", "2026-01-01", "x-1"),
                run("r2", "2026-03-01", "x-2")
            ),
        )
        .unwrap();
        std::fs::write(&b, format!("[{}]", run("r1", "2026-02-02", "x-3"))).unwrap();
        let live = Agent {
            id: "x-2".into(),
            state: "working".into(),
            ..Agent::default()
        };
        let reg = AgentRegistryFile {
            agents: vec![live],
            ..Default::default()
        };
        let out = collect_runs(&[a, b, dir.join("missing.json")], &reg).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            out.iter().map(|r| r.run.id.as_str()).collect::<Vec<_>>(),
            ["r2", "r1"]
        );
        assert_eq!(out[0].live_agents, 1);
        assert_eq!(out[1].run.created_at, "2026-01-01"); // first file wins the duplicate
    }

    #[test]
    fn task_description_prefers_a_real_spec_over_the_issue() {
        let template = "# Spec\n\nDesign decisions, constraints, interfaces. Agents read this before coding.\n";
        assert_eq!(
            task_description("# Spec\nUse a queue.", "# T\nbody"),
            "# Spec\nUse a queue."
        );
        assert_eq!(
            task_description(template, "# Título\n\ncuerpo del issue\n"),
            "cuerpo del issue"
        );
        assert_eq!(task_description("", "sin título"), "sin título");
        assert_eq!(task_description("", ""), "");
    }

    #[test]
    fn bullet_lines_keep_checkboxes_and_drop_template_hints() {
        let text = "# Acceptance\n\n- [ ] one\n- [x] two\n- [ ] (add checkable criteria; the reviewer verifies each one)\nprose";
        assert_eq!(bullet_lines(text), ["- [ ] one", "- [x] two"]);
    }

    #[test]
    fn agent_skills_use_the_registry_record_else_the_role_set() {
        let cat = Catalog::load(None, None);
        let mut cfg = Config::default();
        cfg.roles.insert(
            "coder".into(),
            Role {
                skills: vec!["debug".into()],
                ..Role::default()
            },
        );
        let mut agent = Agent {
            role: "coder".into(),
            ..Agent::default()
        };
        let from_role = agent_skills_for(&cat, &cfg, &agent);
        let debug = from_role
            .active
            .iter()
            .find(|s| s.name == "debug")
            .expect("role skill is active");
        assert_eq!(debug.level, Some("role"));
        agent.active_skills = vec!["write-tests".into()];
        let recorded = agent_skills_for(&cat, &cfg, &agent);
        assert_eq!(
            recorded
                .active
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["write-tests"]
        );
        assert_eq!(recorded.active[0].level, None); // not part of the role set any more
    }

    #[test]
    fn git_status_lists_the_main_worktree_of_a_real_repo() {
        let dir = std::env::temp_dir().join(format!("shepherd-desktop-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status()
            .unwrap();
        assert!(init.success());
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let status = collect_git_status(&dir, &[]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(status.error, None);
        assert_eq!(status.worktrees.len(), 1);
        assert!(status.worktrees[0].is_main && !status.worktrees[0].is_clean);
        assert_eq!(status.worktrees[0].dirty_count, 1);
    }

    #[test]
    fn bus_topics_and_messages_tail_replay_and_filter() {
        let dir = std::env::temp_dir().join(format!("shepherd-desktop-bus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bus = MessageBus::open(dir.join("messages.jsonl")).unwrap();
        for (from, text) in [("a-1", "uno"), ("b-1", "dos"), ("a-1", "tres")] {
            bus.publish("orders", text, None, Some(0), from).unwrap();
        }
        bus.subscribe("b-1", "orders", Some("workers")).unwrap();
        let topics = topics_from(&bus).unwrap();
        let texts = |v: Vec<Message>| v.into_iter().map(|m| m.content).collect::<Vec<_>>();
        let tail = texts(messages_from(&bus, "orders", None, None, 2).unwrap());
        let replay = texts(messages_from(&bus, "orders", Some(1), None, 10).unwrap());
        let by_agent = texts(messages_from(&bus, "orders", None, Some("a-1"), 10).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            (topics.len(), topics[0].messages_count, topics[0].paused),
            (1, 3, false)
        );
        assert_eq!(topics[0].groups[0].group, "workers");
        assert_eq!(tail, ["dos", "tres"]); // newest two, oldest first
        assert_eq!(replay, ["dos", "tres"]); // from offset 1
        assert_eq!(by_agent, ["uno", "tres"]);
    }

    #[test]
    fn rejects_paths_outside_allowlist() {
        assert!(allowed("/etc/passwd").is_err());
    }
}
