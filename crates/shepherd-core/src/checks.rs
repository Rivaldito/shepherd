//! Running a project's checks (its test commands) and reporting what really happened.
//!
//! The commands come from `workflow.checks` in the config, or, when there are none, from what the project
//! looks like (`cargo test`, `pytest`, `npm test`, `go test ./...`). Pass or fail is the exit code, never an
//! agent's reading of the output, so the agents in a feedback loop argue about facts.

use serde::Serialize;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
pub const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
/// The last bytes of a command's output kept in the report.
const TAIL_BYTES: usize = 4000;
/// The most output read from a command (the rest is dropped, the command keeps running).
const READ_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CheckResult {
    pub command: String,
    pub passed: bool,
    /// The exit code; `None` when the command was killed (timeout) or could not start.
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// The end of stdout and stderr together.
    pub tail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CheckReport {
    pub passed: bool,
    pub results: Vec<CheckResult>,
}

/// The checks of a project: the configured ones, else the detected ones.
pub fn commands_for(config: &crate::config::Config, worktree: &Path) -> Vec<String> {
    if !config.workflow.checks.is_empty() {
        return config.workflow.checks.clone();
    }
    crate::skills::detect_checks(worktree).0
}

/// Runs every command, in order, in `worktree`; each gets its own `timeout`. A failure does not stop the next one:
/// the report shows them all.
pub fn run(worktree: &Path, commands: &[String], timeout: Duration) -> CheckReport {
    let results: Vec<CheckResult> = commands
        .iter()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
        .map(|command| run_one(worktree, command, timeout))
        .collect();
    CheckReport {
        passed: !results.is_empty() && results.iter().all(|r| r.passed),
        results,
    }
}

fn run_one(worktree: &Path, command: &str, timeout: Duration) -> CheckResult {
    let mut cmd = Command::new("sh");
    // one ordered stream: the command's stderr goes to its stdout, so the end of the report is the end of the run
    // (the command is `$0` of the outer shell, so it needs no quoting)
    cmd.args(["-c", "exec 2>&1; exec sh -lc \"$0\"", command])
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // its own process group, so a timeout can stop the command and everything it started
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CheckResult {
                command: command.to_owned(),
                passed: false,
                exit_code: None,
                timed_out: false,
                tail: format!("could not start: {error}"),
            }
        }
    };
    // drained on its own thread: a full pipe would stall the command
    let pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(pipe) = pipe {
            let _ = pipe.take(READ_LIMIT).read_to_end(&mut out);
        }
        out
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                stop(&mut child);
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    let output = reader.join().unwrap_or_default();
    let text = String::from_utf8_lossy(&output);
    let mut tail = tail_of(text.trim_end());
    if timed_out {
        tail = format!("killed after {}s\n{tail}", timeout.as_secs());
    }
    CheckResult {
        command: command.to_owned(),
        passed: status.is_some_and(|s| s.success()),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        tail,
    }
}

/// Kills the command and the processes it started.
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // a negative pid is the process group, whose id is the child's
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn tail_of(text: &str) -> String {
    if text.len() <= TAIL_BYTES {
        return text.to_owned();
    }
    let mut start = text.len() - TAIL_BYTES;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("...\n{}", &text[start..])
}

/// The report as text for a person or an agent: one block per command, the verdict first.
pub fn format(report: &CheckReport) -> String {
    if report.results.is_empty() {
        return "no checks to run: set workflow.checks in the config (this project has no cargo, go, npm or pytest files)".into();
    }
    let mut out = format!("{}\n", if report.passed { "PASSED" } else { "FAILED" });
    for r in &report.results {
        let code = r
            .exit_code
            .map_or_else(|| "no exit code".to_owned(), |c| format!("exit {c}"));
        out.push_str(&format!(
            "\n$ {} -> {} ({code})\n{}\n",
            r.command,
            if r.passed { "passed" } else { "failed" },
            r.tail
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("shepherd-checks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn pass_or_fail_is_the_exit_code_and_every_command_runs() {
        let d = dir("exit");
        let report = run(
            &d,
            &[
                "echo ok".into(),
                "echo boom >&2; exit 3".into(),
                "echo still runs".into(),
            ],
            Duration::from_secs(10),
        );
        assert!(!report.passed);
        assert_eq!(report.results.len(), 3);
        assert!(report.results[0].passed && report.results[0].tail.contains("ok"));
        assert_eq!(report.results[1].exit_code, Some(3));
        assert!(report.results[1].tail.contains("boom"));
        assert!(
            report.results[2].passed,
            "a failure does not stop the next command"
        );
        assert!(format(&report).starts_with("FAILED"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn all_passing_is_passed_and_runs_in_the_worktree() {
        let d = dir("pass");
        std::fs::write(d.join("marker.txt"), "here").unwrap();
        let report = run(&d, &["cat marker.txt".into()], Duration::from_secs(10));
        assert!(report.passed);
        assert!(report.results[0].tail.contains("here"));
        assert!(format(&report).starts_with("PASSED"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_command_over_the_timeout_is_killed_and_reported() {
        let d = dir("timeout");
        let started = Instant::now();
        let report = run(
            &d,
            &["echo before; sleep 30".into()],
            Duration::from_millis(300),
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it must not wait for the sleep"
        );
        let r = &report.results[0];
        assert!(r.timed_out && !r.passed && r.exit_code.is_none());
        assert!(r.tail.contains("killed after"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn long_output_keeps_its_end_and_no_commands_is_not_a_pass() {
        let d = dir("tail");
        let report = run(&d, &["seq 1 5000".into()], Duration::from_secs(10));
        assert!(report.results[0].tail.starts_with("..."));
        assert!(report.results[0].tail.ends_with("5000"));
        let none = run(&d, &[], Duration::from_secs(1));
        assert!(!none.passed, "nothing ran, so nothing passed");
        assert!(format(&none).contains("no checks to run"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn configured_checks_win_over_detected_ones() {
        let d = dir("detect");
        std::fs::write(d.join("Cargo.toml"), "").unwrap();
        let mut config = crate::config::Config::default();
        assert_eq!(commands_for(&config, &d), vec!["cargo test".to_owned()]);
        config.workflow.checks = vec!["make test".into()];
        assert_eq!(commands_for(&config, &d), vec!["make test".to_owned()]);
        let _ = std::fs::remove_dir_all(d);
    }
}
