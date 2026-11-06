//! Discovery for the launch forms: which agents are installed, and which models they offer.
//!
//! Installed-ness is asked of the user's login shell, like Herdr desktop does: a desktop app does
//! not inherit the terminal's PATH. Models come from the agent itself when it can list them
//! (`agy models`), else from the agent's own config; Shepherd's config is merged in by the UI.

use crate::api::run_command;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Kinds `shepherd agent start --kind` accepts (each is also the canonical executable).
pub const KNOWN_KINDS: &[&str] = &[
    "pi",
    "claude",
    "codex",
    "gemini",
    "cursor",
    "devin",
    "agy",
    "cline",
    "omp",
    "mastracode",
    "opencode",
    "copilot",
    "kimi",
    "kiro",
    "droid",
    "amp",
    "grok",
    "hermes",
    "kilo",
    "qodercli",
    "qwen",
    "letta",
    "maki",
    "muse",
];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Detected {
    pub command: String,
    pub path: Option<String>,
    /// One of `KNOWN_KINDS` (the others are vendor commands from the config).
    pub known: bool,
}

/// `command -v` output, one `command<TAB>path` per line. Aliases and builtins print a bare word
/// instead of a path, so only absolute paths count as installed.
pub fn parse_installed(stdout: &str) -> HashMap<String, String> {
    stdout
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(_, path)| path.starts_with('/'))
        .map(|(cmd, path)| (cmd.to_string(), path.trim().to_string()))
        .collect()
}

/// Asks `shell -lc` where each command lives. Commands travel as positional arguments, so nothing
/// is spliced into the script.
pub fn ask_shell(
    shell: &str,
    commands: &[String],
    timeout: Duration,
) -> Result<HashMap<String, String>, String> {
    let mut cmd = Command::new(shell);
    cmd.arg("-lc")
        .arg(r#"for c in "$@"; do p=$(command -v "$c" 2>/dev/null) && printf '%s\t%s\n' "$c" "$p"; done"#)
        .arg("sh")
        .args(commands);
    let out = run_command(cmd, timeout)?;
    if out.timed_out {
        return Err("la shell tardó demasiado en responder".into());
    }
    Ok(parse_installed(&out.stdout))
}

fn on_process_path(command: &str) -> Option<String> {
    let p = Path::new(command);
    if p.components().count() > 1 {
        return p.is_file().then(|| command.to_string());
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
        .map(|f| f.display().to_string())
}

type DetectCache = Option<(Instant, Vec<String>, HashMap<String, String>)>;
static DETECT_CACHE: Mutex<DetectCache> = Mutex::new(None);
const DETECT_TTL: Duration = Duration::from_secs(60);

fn login_shell() -> String {
    std::env::var("SHEPHERD_SHELL")
        .ok()
        .or_else(|| std::env::var("SHELL").ok())
        .filter(|s| Path::new(s).is_file())
        .unwrap_or_else(|| "/bin/sh".into())
}

/// Known kinds plus `extra` commands (e.g. the config's vendors), each with its path if installed.
/// The shell's answer is cached for a minute; a command is also found if it is on this process's PATH.
pub fn detected(extra: &[String], refresh: bool) -> Result<Vec<Detected>, String> {
    let mut commands: Vec<String> = KNOWN_KINDS.iter().map(|k| k.to_string()).collect();
    for e in extra {
        let e = e.trim();
        if !e.is_empty() && !e.contains(['\n', '\t', '\0']) && !commands.iter().any(|c| c == e) {
            commands.push(e.to_string());
        }
    }
    let cached = {
        let guard = DETECT_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some((at, key, found)) if !refresh && *key == commands && at.elapsed() < DETECT_TTL => {
                Some(found.clone())
            }
            _ => None,
        }
    };
    let found = match cached {
        Some(found) => found,
        None => {
            // A slow or odd shell must not hide agents that are plainly on our own PATH.
            let from_shell =
                ask_shell(&login_shell(), &commands, Duration::from_secs(8)).unwrap_or_default();
            *DETECT_CACHE.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((Instant::now(), commands.clone(), from_shell.clone()));
            from_shell
        }
    };
    Ok(commands
        .iter()
        .map(|c| Detected {
            known: KNOWN_KINDS.contains(&c.as_str()),
            path: found.get(c).cloned().or_else(|| on_process_path(c)),
            command: c.clone(),
        })
        .collect())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
}

/// `id<TAB>label` per line, as `agy models` prints; lines without a tab (progress text) are skipped.
pub fn parse_models_tsv(text: &str) -> Vec<ModelInfo> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(id, label)| (id.trim(), label.trim()))
        .filter(|(id, _)| {
            !id.is_empty() && !id.contains(char::is_whitespace) && seen.insert(id.to_string())
        })
        .map(|(id, label)| ModelInfo {
            id: id.into(),
            label: if label.is_empty() {
                id.into()
            } else {
                label.into()
            },
        })
        .collect()
}

/// The agents that can list their own models, and how to ask them.
pub fn live_models_command(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "agy" => Some(&["agy", "models"]),
        _ => None,
    }
}

pub fn claude_default_model(settings_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(settings_json)
        .ok()?
        .get("model")?
        .as_str()
        .map(str::to_string)
}

pub fn codex_default_model(config_toml: &str) -> Option<String> {
    toml::from_str::<toml::Value>(config_toml)
        .ok()?
        .get("model")?
        .as_str()
        .map(str::to_string)
}

/// The model the agent itself would use, from its own config file (only that one key is read).
pub fn agent_default_model(kind: &str, home: &Path) -> Option<String> {
    match kind {
        "claude" => {
            claude_default_model(&std::fs::read_to_string(home.join(".claude/settings.json")).ok()?)
        }
        "codex" => {
            codex_default_model(&std::fs::read_to_string(home.join(".codex/config.toml")).ok()?)
        }
        _ => None,
    }
}

type ModelCache = HashMap<String, (Instant, Vec<ModelInfo>)>;
static MODEL_CACHE: OnceLock<Mutex<ModelCache>> = OnceLock::new();
const MODEL_TTL: Duration = Duration::from_secs(300);

/// Asks the agent for its models through the login shell (it may call the network, so give it time).
/// Only kinds in `live_models_command` run anything; successful answers are cached for five minutes.
pub fn live_models(kind: &str, refresh: bool) -> Result<Vec<ModelInfo>, String> {
    let argv =
        live_models_command(kind).ok_or_else(|| format!("{kind} no puede listar sus modelos"))?;
    let cache = MODEL_CACHE.get_or_init(Default::default);
    if !refresh {
        if let Some((at, models)) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(kind) {
            if at.elapsed() < MODEL_TTL {
                return Ok(models.clone());
            }
        }
    }
    let mut cmd = Command::new(login_shell());
    cmd.arg("-lc").arg(r#"exec "$@""#).arg("sh").args(argv);
    let out = run_command(cmd, Duration::from_secs(25))?;
    if out.timed_out {
        return Err("el agente tardó demasiado en listar sus modelos".into());
    }
    let models = parse_models_tsv(&out.stdout);
    if models.is_empty() {
        let why = out.stderr.trim();
        return Err(if why.is_empty() {
            "el agente no devolvió modelos".into()
        } else {
            why.lines().next().unwrap_or("").to_string()
        });
    }
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(kind.to_string(), (Instant::now(), models.clone()));
    Ok(models)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn installed_parsing_keeps_only_absolute_paths() {
        let out = "claude\t/home/u/.local/bin/claude\nll\talias ll='ls -l'\ncd\tcd\ncodex\t/usr/bin/codex\nnoise without tab\n";
        let found = parse_installed(out);
        assert_eq!(found.len(), 2);
        assert_eq!(found["claude"], "/home/u/.local/bin/claude");
        assert_eq!(found["codex"], "/usr/bin/codex");
    }

    #[test]
    fn the_shell_is_asked_with_positional_arguments_never_spliced_into_the_script() {
        // `sh` exists everywhere; the second name does not; the third would run `touch` if spliced.
        let marker =
            std::env::temp_dir().join(format!("shepherd-desktop-splice-{}", std::process::id()));
        let evil = format!("x; touch {}", marker.display());
        let found = ask_shell(
            "/bin/sh",
            &[
                "sh".to_string(),
                "definitely-not-a-command-xyz".to_string(),
                evil,
            ],
            Duration::from_secs(5),
        )
        .unwrap();
        let spliced = marker.exists();
        let _ = std::fs::remove_file(&marker);
        assert!(!spliced, "a command name was executed");
        assert!(found["sh"].starts_with('/'));
        assert!(!found.contains_key("definitely-not-a-command-xyz"));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_missing_shell_is_an_error_and_a_slow_one_times_out() {
        assert!(
            ask_shell("/nonexistent/shell", &["sh".into()], Duration::from_secs(1))
                .unwrap_err()
                .contains("no se pudo ejecutar")
        );
        let dir =
            std::env::temp_dir().join(format!("shepherd-desktop-slowshell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shell = dir.join("slow-shell");
        std::fs::write(&shell, "#!/bin/sh\nsleep 10\n").unwrap();
        std::fs::set_permissions(&shell, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let started = Instant::now();
        let err = ask_shell(
            shell.to_str().unwrap(),
            &["sh".into()],
            Duration::from_millis(200),
        )
        .unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(err.contains("tardó demasiado") && started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn known_kinds_are_complete_and_safe_names() {
        assert_eq!(KNOWN_KINDS.len(), 24);
        assert!(
            KNOWN_KINDS.contains(&"claude")
                && KNOWN_KINDS.contains(&"agy")
                && KNOWN_KINDS.contains(&"codex")
        );
        assert!(KNOWN_KINDS.iter().all(|k| k
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())));
        let mut sorted = KNOWN_KINDS.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), KNOWN_KINDS.len(), "duplicate kind");
    }

    #[test]
    fn detected_marks_known_kinds_and_adds_vendor_commands_without_duplicates() {
        let list = detected(
            &["claude".into(), "sh".into(), "".into(), "bad\tname".into()],
            true,
        )
        .unwrap();
        assert_eq!(list.len(), KNOWN_KINDS.len() + 1); // + "sh"; the duplicate, empty and tab-named ones are dropped
        assert!(list.iter().find(|d| d.command == "claude").unwrap().known);
        let sh = list.iter().find(|d| d.command == "sh").unwrap();
        assert!(!sh.known && sh.path.as_deref().is_some_and(|p| p.starts_with('/')));
        assert!(list.iter().find(|d| d.command == "agy").is_some());
    }

    #[test]
    fn models_parse_the_real_agy_output() {
        let real = "Fetching available models...\ngemini-4-argon-medium\tGemini 4 Argon (Medium)\ngemini-3.8-flash-low\tGemini 3.8 Flash (Low)\n\
                    claude-opus-5-5-high\tClaude Opus 5.5 (High)\ngpt-oss-120b-medium\tGPT-OSS 120B (Medium)\ngemini-4-argon-medium\tduplicate line\n";
        let models = parse_models_tsv(real);
        assert_eq!(models.len(), 4, "progress line skipped, duplicate dropped");
        assert_eq!(
            models[0],
            ModelInfo {
                id: "gemini-4-argon-medium".into(),
                label: "Gemini 4 Argon (Medium)".into()
            }
        );
        assert_eq!(models[3].id, "gpt-oss-120b-medium");
        assert!(parse_models_tsv("Fetching available models...\n").is_empty());
        assert_eq!(parse_models_tsv("only-id\t\n")[0].label, "only-id"); // an empty label falls back to the id
    }

    #[test]
    fn only_agents_that_can_list_models_are_asked() {
        assert_eq!(live_models_command("agy"), Some(&["agy", "models"][..]));
        for kind in ["claude", "codex", "rm", "../evil", ""] {
            assert_eq!(live_models_command(kind), None, "{kind}");
        }
        assert!(live_models("claude", false)
            .unwrap_err()
            .contains("no puede listar"));
    }

    #[test]
    fn default_models_come_from_each_agents_own_config() {
        assert_eq!(
            claude_default_model(r#"{"model":"sonnet","env":{"TOKEN":"secret"}}"#).as_deref(),
            Some("sonnet")
        );
        assert_eq!(claude_default_model(r#"{"theme":"dark"}"#), None);
        assert_eq!(claude_default_model("not json"), None);
        assert_eq!(
            codex_default_model("model = \"o3\"\n[projects.x]\ntrust = true").as_deref(),
            Some("o3")
        );
        assert_eq!(codex_default_model("[tui]\ntheme = 1"), None);
        assert_eq!(codex_default_model("= broken"), None);
        // reads only the model key from files under HOME
        let home =
            std::env::temp_dir().join(format!("shepherd-desktop-home-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/settings.json"), r#"{"model":"haiku"}"#).unwrap();
        let got = (
            agent_default_model("claude", &home),
            agent_default_model("codex", &home),
            agent_default_model("agy", &home),
        );
        std::fs::remove_dir_all(&home).unwrap();
        assert_eq!(got, (Some("haiku".into()), None, None));
    }

    /// Real machine: `cargo test -p shepherd-desktop live_discovery -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_discovery() {
        let list = detected(&[], true).unwrap();
        let installed: Vec<_> = list
            .iter()
            .filter(|d| d.path.is_some())
            .map(|d| format!("{} -> {}", d.command, d.path.as_deref().unwrap()))
            .collect();
        println!("installed: {installed:#?}");
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        for kind in ["claude", "codex", "agy"] {
            println!(
                "{kind}: agent default = {:?}",
                agent_default_model(kind, &home)
            );
        }
        if installed.iter().any(|i| i.starts_with("agy ")) {
            let models = live_models("agy", true).unwrap();
            println!("agy models: {} (first: {:?})", models.len(), models.first());
            assert!(!models.is_empty());
        }
    }
}
