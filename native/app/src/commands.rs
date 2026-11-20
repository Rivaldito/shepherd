//! The command palette: what can be run from the keyboard, and how a typed query narrows it.
//! Anything destructive only *arms* its button in the panel, so the second click stays where it always is.

use crate::panel::{Armed, PanelMsg, Tab};
use crate::theme;
use crate::{Action, Message, Mode};
use shepherd_app_core::data::{task_of_run, Data, Gate};
use shepherd_app_core::snapshot::Snapshot;

/// One row of the palette: its group, its label, a hint on the right (a shortcut or a state), the colour of its
/// square and what it runs.
#[derive(Debug, Clone)]
pub struct Command {
    pub group: &'static str,
    pub label: String,
    pub hint: String,
    pub dot: iced::Color,
    pub run: Vec<Message>,
}

fn cmd(
    group: &'static str,
    label: impl Into<String>,
    hint: impl Into<String>,
    dot: iced::Color,
    run: Vec<Message>,
) -> Command {
    Command {
        group,
        label: label.into(),
        hint: hint.into(),
        dot,
        run,
    }
}

/// Every row, in the order of the groups of the spec: Go to, Needs you, Agents, Tasks, Run a pattern, Commands.
pub fn build(snapshot: &Snapshot, data: &Data) -> Vec<Command> {
    let neutral = theme::LINE2;
    let go = |tab: Tab| {
        vec![
            Message::SetMode(Mode::Shepherd),
            Message::Panel(PanelMsg::Tab(tab)),
        ]
    };
    let mut all = Vec::new();
    for (label, tab, key) in [
        ("Overview", Tab::Overview, "Ctrl+1"),
        ("Runs", Tab::Runs, "Ctrl+2"),
        ("Agents", Tab::Agents, "Ctrl+3"),
        ("Tasks", Tab::Tasks, "Ctrl+4"),
        ("Patterns", Tab::Patterns, "Ctrl+5"),
        ("Activity", Tab::Monitor, "Ctrl+6"),
        ("Skills", Tab::Skills, "Ctrl+7"),
        ("Git", Tab::Git, "Ctrl+8"),
        ("Events", Tab::Events, "Ctrl+9"),
        ("Models", Tab::Models, ""),
        ("Snapshots", Tab::Snapshots, ""),
    ] {
        all.push(cmd("Go to", label, key, neutral, go(tab)));
    }
    all.push(cmd(
        "Go to",
        "Terminal mode",
        "Ctrl+Shift+S",
        neutral,
        vec![Message::SetMode(Mode::Terminal)],
    ));
    for workspace in &snapshot.workspaces {
        all.push(cmd(
            "Go to",
            format!("Workspace: {}", workspace.label),
            "terminal",
            neutral,
            vec![
                Message::SetMode(Mode::Terminal),
                Message::Act(Action::FocusSpace(workspace.workspace_id.clone())),
            ],
        ));
    }
    if let Some(workspace) = snapshot.focused_workspace_id.clone() {
        for tab in snapshot.tabs_of(&workspace) {
            all.push(cmd(
                "Go to",
                format!("Tab: {}", tab.label),
                "terminal",
                neutral,
                vec![
                    Message::SetMode(Mode::Terminal),
                    Message::Act(Action::FocusTab(tab.tab_id.clone())),
                ],
            ));
        }
    }
    // what waits for the human
    for g in &data.gates {
        let run = data
            .runs
            .iter()
            .find(|r| task_of_run(&r.run.task) == Some(g.task_id));
        let mut run_msgs = go(Tab::Runs);
        if let Some(r) = run {
            run_msgs.push(Message::Panel(PanelMsg::SelectRun(r.run.id.clone())));
        }
        all.push(cmd(
            "Needs you",
            match &g.gate {
                Gate::PushPr(n) => format!("Publish subtask {n} of task #{}", g.task_id),
                Gate::Plan => format!("Approve the plan of task #{}", g.task_id),
            },
            "awaiting-approval",
            theme::WARN,
            run_msgs,
        ));
    }
    for agent in &snapshot.agents {
        let status = agent.status();
        all.push(cmd(
            "Agents",
            agent.display_name(),
            theme::status_name(status),
            theme::status_color(status),
            vec![Message::GoToPane(agent.pane_id.clone())],
        ));
    }
    for t in &data.tasks {
        let mut msgs = go(Tab::Tasks);
        msgs.push(Message::Panel(PanelMsg::SelectTask(t.status.id)));
        all.push(cmd(
            "Tasks",
            format!("#{} {}", t.status.id, t.status.title),
            t.status.state.clone(),
            crate::panel::task_state_color(&t.status.state),
            msgs,
        ));
    }
    for p in &data.patterns {
        all.push(cmd(
            "Run a pattern",
            format!("Run {}", p.name),
            "pattern",
            theme::VIOLET,
            vec![
                Message::SetMode(Mode::Shepherd),
                Message::Panel(PanelMsg::PickAndLaunch(p.name.clone())),
            ],
        ));
    }
    all.push(cmd(
        "Commands",
        "Clean finished runs",
        "asks twice",
        neutral,
        vec![
            Message::SetMode(Mode::Shepherd),
            Message::Panel(PanelMsg::Tab(Tab::Runs)),
            Message::Panel(PanelMsg::Arm(Armed::CleanRuns)),
        ],
    ));
    all.push(cmd(
        "Commands",
        "New workspace",
        "",
        neutral,
        vec![Message::NewSpaceOpen],
    ));
    if let Some(workspace) = snapshot.focused_workspace_id.clone() {
        all.push(cmd(
            "Commands",
            "New tab",
            "",
            neutral,
            vec![Message::Act(Action::NewTab(workspace))],
        ));
    }
    all.push(cmd(
        "Commands",
        "Reload the panels",
        "",
        neutral,
        vec![Message::Reload],
    ));
    all
}

/// Commands containing every word of `query` (any case, any order); all of them for an empty query.
pub fn filter<'a>(commands: &'a [Command], query: &str) -> Vec<&'a Command> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    commands
        .iter()
        .filter(|c| {
            let hay = format!("{} {} {}", c.label, c.hint, c.group).to_lowercase();
            words.iter().all(|w| hay.contains(w))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot::parse(
            r#"{"focused_workspace_id":"w1","workspaces":[{"workspace_id":"w1","label":"alpha"},{"workspace_id":"w2","label":"beta"}],
                "tabs":[{"tab_id":"w1:t1","workspace_id":"w1","label":"logs"},{"tab_id":"w2:t1","workspace_id":"w2","label":"other"}],
                "agents":[{"pane_id":"w1:p2","name":"coder-1","agent_status":"idle"}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn the_palette_lists_the_groups_of_the_spec() {
        let all = build(&snapshot(), &Data::default());
        let labels: Vec<&str> = all.iter().map(|c| c.label.as_str()).collect();
        for expected in [
            "Overview",
            "Terminal mode",
            "Workspace: alpha",
            "Workspace: beta",
            "Tab: logs",
            "coder-1",
            "Clean finished runs",
            "New tab",
        ] {
            assert!(
                labels.contains(&expected),
                "{expected} missing in {labels:?}"
            );
        }
        assert!(
            !labels.contains(&"Tab: other"),
            "tabs of other workspaces are not listed"
        );
        let groups: Vec<&str> = all.iter().map(|c| c.group).collect();
        let first_of = |g: &str| groups.iter().position(|x| *x == g).unwrap();
        assert!(
            first_of("Go to") < first_of("Agents") && first_of("Agents") < first_of("Commands")
        );
    }

    #[test]
    fn a_query_needs_every_word_in_any_order_and_case() {
        let all = build(&snapshot(), &Data::default());
        let hits = |q: &str| {
            filter(&all, q)
                .into_iter()
                .map(|c| c.label.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(hits("workspace BETA"), ["Workspace: beta"]);
        assert_eq!(hits("coder agents"), ["coder-1"]);
        assert!(hits("zzz").is_empty());
        assert_eq!(hits("").len(), all.len());
    }

    #[test]
    fn destructive_commands_only_arm_the_button() {
        let all = build(&snapshot(), &Data::default());
        let clean = all
            .iter()
            .find(|c| c.label.starts_with("Clean finished"))
            .unwrap();
        assert!(clean
            .run
            .iter()
            .any(|m| matches!(m, Message::Panel(PanelMsg::Arm(Armed::CleanRuns)))));
        assert!(!clean
            .run
            .iter()
            .any(|m| matches!(m, Message::Panel(PanelMsg::Confirm))));
    }
}
